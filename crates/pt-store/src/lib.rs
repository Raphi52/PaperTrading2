//! # pt-store
//!
//! Corrections par rapport à l'ancien bot :
//! - une base SQLite transactionnelle remplace le gros `portfolios.json`
//!   (33 Mo réécrits en entier, partagé entre processus avec un verrou maison) ;
//! - les exécutions sont stockées en entier, jamais tronquées ;
//! - au chargement, l'état est revérifié contre les exécutions : un état
//!   incohérent est REFUSÉ au lieu d'être utilisé en silence.

use anyhow::{bail, Context, Result};
use pt_core::engine::{Decision, Engine};
use pt_core::portfolio::{ClosedTrade, Fill, Side};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use std::path::Path;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS portfolios (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    active INTEGER NOT NULL DEFAULT 1,
    last_error TEXT,
    engine TEXT NOT NULL,
    benchmark TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS fills (
    portfolio_id INTEGER NOT NULL REFERENCES portfolios(id) ON DELETE CASCADE,
    book TEXT NOT NULL,
    seq INTEGER NOT NULL,
    time INTEGER NOT NULL,
    symbol TEXT NOT NULL,
    side TEXT NOT NULL,
    qty REAL NOT NULL,
    price REAL NOT NULL,
    fee REAL NOT NULL,
    cash_delta REAL NOT NULL,
    reason TEXT NOT NULL,
    PRIMARY KEY (portfolio_id, book, seq)
);
CREATE TABLE IF NOT EXISTS closed_trades (
    portfolio_id INTEGER NOT NULL REFERENCES portfolios(id) ON DELETE CASCADE,
    book TEXT NOT NULL,
    seq INTEGER NOT NULL,
    data TEXT NOT NULL,
    PRIMARY KEY (portfolio_id, book, seq)
);
CREATE TABLE IF NOT EXISTS equity (
    portfolio_id INTEGER NOT NULL REFERENCES portfolios(id) ON DELETE CASCADE,
    time INTEGER NOT NULL,
    equity REAL NOT NULL,
    benchmark REAL NOT NULL,
    exposure REAL NOT NULL,
    PRIMARY KEY (portfolio_id, time)
);
-- Journal des décisions (ajouté sans toucher aux tables existantes) :
-- entrées, sorties, renforcements et signaux écartés, avec leur motif et un
-- instantané JSON des indicateurs au moment de la décision.
CREATE TABLE IF NOT EXISTS decisions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    portfolio_id INTEGER NOT NULL REFERENCES portfolios(id) ON DELETE CASCADE,
    book TEXT NOT NULL,
    time INTEGER NOT NULL,
    symbol TEXT NOT NULL,
    kind TEXT NOT NULL,
    reason TEXT NOT NULL,
    price REAL,
    indicators TEXT NOT NULL,
    -- rejouer un enregistrement ne duplique rien
    UNIQUE (portfolio_id, book, time, symbol, kind, reason)
);
CREATE INDEX IF NOT EXISTS decisions_by_portfolio ON decisions(portfolio_id, time);
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

/// Un portefeuille en direct : la stratégie et sa référence « acheter et garder »,
/// démarrées au même instant avec le même capital et les mêmes frais.
#[derive(Debug, Clone, PartialEq)]
pub struct LivePortfolio {
    pub id: i64,
    pub name: String,
    pub created_at: i64,
    pub active: bool,
    pub last_error: Option<String>,
    pub engine: Engine,
    pub benchmark: Engine,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, serde::Deserialize)]
pub struct EquityRow {
    pub time: i64,
    pub equity: f64,
    pub benchmark: f64,
    pub exposure: f64,
}

pub struct Store {
    conn: Connection,
}

const STRATEGY: &str = "strategy";
const BENCHMARK: &str = "benchmark";

fn strip(e: &Engine) -> Engine {
    let mut e = e.clone();
    e.portfolio.fills.clear();
    e.portfolio.closed.clear();
    e
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Store> {
        let conn = Connection::open(path.as_ref()).with_context(|| format!("ouverture {}", path.as_ref().display()))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        // Une seconde fenêtre de l'application lit la même base : on attend
        // qu'une écriture se termine au lieu d'échouer sur « base verrouillée ».
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        Self::init(conn)
    }

    pub fn in_memory() -> Result<Store> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Store> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Store { conn })
    }

    pub fn create(&mut self, name: &str, created_at: i64, engine: &Engine, benchmark: &Engine) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO portfolios (name, created_at, active, engine, benchmark) VALUES (?1, ?2, 1, ?3, ?4)",
            params![
                name,
                created_at,
                serde_json::to_string(&strip(engine))?,
                serde_json::to_string(&strip(benchmark))?
            ],
        )?;
        let id = self.conn.last_insert_rowid();
        let p = LivePortfolio {
            id,
            name: name.to_string(),
            created_at,
            active: true,
            last_error: None,
            engine: engine.clone(),
            benchmark: benchmark.clone(),
        };
        self.save(&p)?;
        Ok(id)
    }

    pub fn ids(&self) -> Result<Vec<i64>> {
        let mut st = self.conn.prepare("SELECT id FROM portfolios ORDER BY id")?;
        let ids = st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<Vec<i64>>>()?;
        Ok(ids)
    }

    fn load_fills(&self, id: i64, book: &str) -> Result<Vec<Fill>> {
        let mut st = self.conn.prepare(
            "SELECT time, symbol, side, qty, price, fee, cash_delta, reason FROM fills WHERE portfolio_id=?1 AND book=?2 ORDER BY seq",
        )?;
        let rows = st.query_map(params![id, book], |r| {
            let side: String = r.get(2)?;
            Ok(Fill {
                time: r.get(0)?,
                symbol: r.get(1)?,
                side: if side == "BUY" { Side::Buy } else { Side::Sell },
                qty: r.get(3)?,
                price: r.get(4)?,
                fee: r.get(5)?,
                cash_delta: r.get(6)?,
                reason: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    fn load_closed(&self, id: i64, book: &str) -> Result<Vec<ClosedTrade>> {
        let mut st =
            self.conn.prepare("SELECT data FROM closed_trades WHERE portfolio_id=?1 AND book=?2 ORDER BY seq")?;
        let rows = st.query_map(params![id, book], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }

    /// Charge un portefeuille et REVÉRIFIE sa comptabilité contre ses exécutions.
    pub fn load(&self, id: i64) -> Result<LivePortfolio> {
        let row = self
            .conn
            .query_row(
                "SELECT name, created_at, active, last_error, engine, benchmark FROM portfolios WHERE id=?1",
                params![id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, bool>(2)?,
                        r.get::<_, Option<String>>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, String>(5)?,
                    ))
                },
            )
            .optional()?;
        let Some((name, created_at, active, last_error, e_json, b_json)) = row else {
            bail!("portefeuille {id} introuvable");
        };
        let mut engine: Engine = serde_json::from_str(&e_json).context("état de la stratégie illisible")?;
        let mut benchmark: Engine = serde_json::from_str(&b_json).context("état de la référence illisible")?;
        engine.portfolio.fills = self.load_fills(id, STRATEGY)?;
        engine.portfolio.closed = self.load_closed(id, STRATEGY)?;
        benchmark.portfolio.fills = self.load_fills(id, BENCHMARK)?;
        benchmark.portfolio.closed = self.load_closed(id, BENCHMARK)?;
        engine.portfolio.check_invariants().map_err(|e| anyhow::anyhow!("portefeuille {id} incohérent : {e}"))?;
        benchmark.portfolio.check_invariants().map_err(|e| anyhow::anyhow!("référence {id} incohérente : {e}"))?;
        Ok(LivePortfolio { id, name, created_at, active, last_error, engine, benchmark })
    }

    pub fn load_all(&self) -> Result<Vec<LivePortfolio>> {
        self.ids()?.into_iter().map(|id| self.load(id)).collect()
    }

    fn count(&self, table: &str, id: i64, book: &str) -> Result<usize> {
        let n: i64 = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM {table} WHERE portfolio_id=?1 AND book=?2"),
            params![id, book],
            |r| r.get(0),
        )?;
        Ok(n as usize)
    }

    /// Enregistre tout, nom et activation compris.
    pub fn save(&mut self, p: &LivePortfolio) -> Result<()> {
        self.save_progress(p)?;
        self.conn.execute("UPDATE portfolios SET name=?2, active=?3 WHERE id=?1", params![p.id, p.name, p.active])?;
        Ok(())
    }

    /// `save_progress` puis vide le journal des décisions en mémoire, une fois
    /// la transaction validée : sans cela il grossirait à chaque passage du mode
    /// direct et serait réinséré en entier à chaque enregistrement.
    pub fn save_progress_and_flush(&mut self, p: &mut LivePortfolio) -> Result<()> {
        self.save_progress(p)?;
        p.engine.decisions.clear();
        p.benchmark.decisions.clear();
        Ok(())
    }

    pub fn set_active(&self, id: i64, active: bool) -> Result<()> {
        let n = self.conn.execute("UPDATE portfolios SET active=?2 WHERE id=?1", params![id, active])?;
        if n == 0 {
            bail!("portefeuille {id} introuvable");
        }
        Ok(())
    }

    /// Enregistre l'avancement du moteur (état + NOUVELLES exécutions), dans une
    /// transaction, sans toucher au nom ni à l'activation : une mise en pause
    /// faite pendant un passage du mode direct n'est donc jamais écrasée.
    pub fn save_progress(&mut self, p: &LivePortfolio) -> Result<()> {
        let exists: bool =
            self.conn.query_row("SELECT EXISTS(SELECT 1 FROM portfolios WHERE id=?1)", params![p.id], |r| r.get(0))?;
        if !exists {
            bail!("portefeuille {} supprimé entre-temps", p.id);
        }
        let books = [(STRATEGY, &p.engine), (BENCHMARK, &p.benchmark)];
        let mut starts = Vec::new();
        for (book, e) in books {
            let nf = self.count("fills", p.id, book)?;
            let nc = self.count("closed_trades", p.id, book)?;
            if nf > e.portfolio.fills.len() || nc > e.portfolio.closed.len() {
                bail!("portefeuille {} : l'état en mémoire est plus ancien que la base", p.id);
            }
            starts.push((book, e, nf, nc));
        }
        let tx = self.conn.transaction()?;
        for (book, e, nf, nc) in starts {
            for (seq, f) in e.portfolio.fills.iter().enumerate().skip(nf) {
                tx.execute(
                    "INSERT INTO fills (portfolio_id, book, seq, time, symbol, side, qty, price, fee, cash_delta, reason) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
                    params![
                        p.id,
                        book,
                        seq as i64,
                        f.time,
                        f.symbol,
                        if f.side == Side::Buy { "BUY" } else { "SELL" },
                        f.qty,
                        f.price,
                        f.fee,
                        f.cash_delta,
                        f.reason
                    ],
                )?;
            }
            for (seq, t) in e.portfolio.closed.iter().enumerate().skip(nc) {
                tx.execute(
                    "INSERT INTO closed_trades (portfolio_id, book, seq, data) VALUES (?1, ?2, ?3, ?4)",
                    params![p.id, book, seq as i64, serde_json::to_string(t)?],
                )?;
            }
        }
        for (book, e) in books {
            for d in &e.decisions {
                tx.execute(
                    "INSERT OR IGNORE INTO decisions (portfolio_id, book, time, symbol, kind, reason, price, indicators) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                    params![
                        p.id,
                        book,
                        d.time,
                        d.symbol,
                        d.kind.as_str(),
                        d.reason,
                        d.price.is_finite().then_some(d.price),
                        serde_json::to_string(&d.indicators)?
                    ],
                )?;
            }
        }
        // Rétention : les signaux ÉCARTÉS (des centaines par passage sur 100 cryptos)
        // ne sont gardés que 30 jours ; entrées, sorties et renforts restent pour toujours.
        if let Some(latest) = books.iter().flat_map(|(_, e)| e.decisions.iter().map(|d| d.time)).max() {
            tx.execute(
                "DELETE FROM decisions WHERE portfolio_id=?1 AND kind='SKIPPED' AND time < ?2",
                params![p.id, latest - 30 * 86_400_000],
            )?;
        }
        tx.execute(
            "UPDATE portfolios SET last_error=?2, engine=?3, benchmark=?4 WHERE id=?1",
            params![
                p.id,
                p.last_error,
                serde_json::to_string(&strip(&p.engine))?,
                serde_json::to_string(&strip(&p.benchmark))?
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Journal des décisions d'un portefeuille (stratégie), du plus ancien au plus récent.
    pub fn decisions(&self, id: i64) -> Result<Vec<Decision>> {
        let mut st = self.conn.prepare(
            "SELECT time, symbol, kind, reason, price, indicators FROM decisions WHERE portfolio_id=?1 AND book=?2 ORDER BY id",
        )?;
        let rows = st.query_map(params![id, STRATEGY], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<f64>>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?;
        rows.map(|r| {
            let (time, symbol, kind, reason, price, ind) = r?;
            let kind = match kind.as_str() {
                "ENTRY" => pt_core::engine::DecisionKind::Entry,
                "ADD" => pt_core::engine::DecisionKind::Add,
                "EXIT" => pt_core::engine::DecisionKind::Exit,
                _ => pt_core::engine::DecisionKind::Skipped,
            };
            Ok(Decision { time, symbol, kind, reason, price: price.unwrap_or(f64::NAN), indicators: serde_json::from_str(&ind)? })
        })
        .collect()
    }

    pub fn push_equity(&self, id: i64, row: EquityRow) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO equity (portfolio_id, time, equity, benchmark, exposure) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, row.time, row.equity, row.benchmark, row.exposure],
        )?;
        Ok(())
    }

    pub fn last_equity_time(&self, id: i64) -> Result<Option<i64>> {
        Ok(self.conn.query_row("SELECT MAX(time) FROM equity WHERE portfolio_id=?1", params![id], |r| r.get(0))?)
    }

    pub fn equity_curve(&self, id: i64) -> Result<Vec<EquityRow>> {
        let mut st = self
            .conn
            .prepare("SELECT time, equity, benchmark, exposure FROM equity WHERE portfolio_id=?1 ORDER BY time")?;
        let rows = st.query_map(params![id], |r| {
            Ok(EquityRow { time: r.get(0)?, equity: r.get(1)?, benchmark: r.get(2)?, exposure: r.get(3)? })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM portfolios WHERE id=?1", params![id])?;
        Ok(())
    }

    pub fn get_setting<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let v: Option<String> =
            self.conn.query_row("SELECT value FROM settings WHERE key=?1", params![key], |r| r.get(0)).optional()?;
        v.map(|s| serde_json::from_str(&s).context("réglage illisible")).transpose()
    }

    pub fn set_setting<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, serde_json::to_string(value)?],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pt_core::{find, CostModel};

    fn engines() -> (Engine, Engine) {
        let syms = vec!["BTCEUR".to_string(), "ETHEUR".to_string()];
        let e = Engine::new(find("macd_1d").unwrap(), syms.clone(), 10_000.0, CostModel::default(), 0);
        let b = Engine::new(find("buy_hold").unwrap(), syms, 10_000.0, CostModel::default(), 0);
        (e, b)
    }

    #[test]
    fn roundtrip_keeps_everything() {
        let mut s = Store::in_memory().unwrap();
        let (mut e, b) = engines();
        let id = s.create("Test", 1, &e, &b).unwrap();
        e.portfolio.buy("BTCEUR", 1_000.0, 50_000.0, 10, &CostModel::default(), "entrée").unwrap();
        e.portfolio.sell_all("BTCEUR", 51_000.0, 20, &CostModel::default(), "sortie").unwrap();
        e.portfolio.buy("ETHEUR", 500.0, 2_000.0, 30, &CostModel::default(), "entrée").unwrap();
        let mut p = s.load(id).unwrap();
        p.engine = e.clone();
        s.save(&p).unwrap();
        s.save(&p).unwrap(); // rejouer l'enregistrement ne duplique rien
        let back = s.load(id).unwrap();
        assert_eq!(back.engine, e);
        assert_eq!(back.engine.portfolio.fills.len(), 3);
        s.push_equity(id, EquityRow { time: 5, equity: 10_010.0, benchmark: 9_990.0, exposure: 0.05 }).unwrap();
        assert_eq!(s.equity_curve(id).unwrap().len(), 1);
    }

    #[test]
    fn decisions_are_journaled_without_touching_history() {
        let mut s = Store::in_memory().unwrap();
        let (mut e, b) = engines();
        let id = s.create("Test", 1, &e, &b).unwrap();
        e.portfolio.buy("BTCEUR", 1_000.0, 50_000.0, 10, &CostModel::default(), "entrée").unwrap();
        let mut p = s.load(id).unwrap();
        p.engine = e.clone();
        s.save(&p).unwrap();
        let before = serde_json::to_string(&strip(&p.engine)).unwrap();
        let mut ind = std::collections::BTreeMap::new();
        ind.insert("rsi14".to_string(), 61.5);
        let d = |kind, reason: &str| Decision {
            time: 20,
            symbol: "BTCEUR".into(),
            kind,
            reason: reason.into(),
            price: 51_000.0,
            indicators: ind.clone(),
        };
        use pt_core::engine::DecisionKind::*;
        p.engine.decisions = vec![d(Entry, "SIGNAL D'ENTRÉE"), d(Exit, "SIGNAL DE SORTIE"), d(Skipped, "PLUS DE PLACE")];
        s.save_progress(&p).unwrap();
        s.save_progress(&p).unwrap(); // rejouer ne duplique rien
        let got = s.decisions(id).unwrap();
        assert_eq!(got, p.engine.decisions);
        // Le journal vit dans sa table : l'état du moteur n'a pas grossi,
        // et l'historique (exécutions, contrôles au chargement) est intact.
        assert_eq!(serde_json::to_string(&strip(&p.engine)).unwrap(), before);
        let back = s.load(id).unwrap();
        assert_eq!(back.engine.portfolio.fills.len(), 1);
        assert!(back.engine.decisions.is_empty());
        // Le mode direct vide le journal après chaque enregistrement validé.
        s.save_progress_and_flush(&mut p).unwrap();
        assert!(p.engine.decisions.is_empty() && p.benchmark.decisions.is_empty());
        p.engine.decisions = vec![d(Skipped, "SECOND PASSAGE")];
        s.save_progress_and_flush(&mut p).unwrap();
        assert!(p.engine.decisions.is_empty());
        assert_eq!(s.decisions(id).unwrap().len(), 4);
    }

    #[test]
    fn tampered_state_is_refused() {
        let mut s = Store::in_memory().unwrap();
        let (mut e, b) = engines();
        let id = s.create("Test", 1, &e, &b).unwrap();
        e.portfolio.buy("BTCEUR", 1_000.0, 50_000.0, 10, &CostModel::default(), "entrée").unwrap();
        let mut p = s.load(id).unwrap();
        p.engine = e;
        s.save(&p).unwrap();
        // Quelqu'un « corrige » la trésorerie à la main : le chargement doit refuser.
        let mut forged = p.engine.clone();
        forged.portfolio.cash += 500.0;
        s.conn
            .execute(
                "UPDATE portfolios SET engine=?2 WHERE id=?1",
                params![id, serde_json::to_string(&strip(&forged)).unwrap()],
            )
            .unwrap();
        let err = s.load(id).unwrap_err().to_string();
        assert!(err.contains("incohérent"), "{err}");
    }

    #[test]
    fn progress_save_never_overwrites_a_pause() {
        let mut s = Store::in_memory().unwrap();
        let (e, b) = engines();
        let id = s.create("Test", 1, &e, &b).unwrap();
        let running_copy = s.load(id).unwrap(); // chargé par le mode direct
        s.set_active(id, false).unwrap(); // l'utilisateur met en pause pendant le passage
        s.save_progress(&running_copy).unwrap();
        assert!(!s.load(id).unwrap().active, "la pause doit survivre au passage du mode direct");
        s.delete(id).unwrap();
        assert!(s.save_progress(&running_copy).is_err(), "un portefeuille supprimé ne ressuscite pas");
    }

    #[test]
    fn delete_cascades_and_settings_roundtrip() {
        let mut s = Store::in_memory().unwrap();
        let (e, b) = engines();
        let id = s.create("Test", 1, &e, &b).unwrap();
        s.push_equity(id, EquityRow { time: 1, equity: 1.0, benchmark: 1.0, exposure: 0.0 }).unwrap();
        s.delete(id).unwrap();
        assert!(s.ids().unwrap().is_empty());
        assert!(s.equity_curve(id).unwrap().is_empty());
        s.set_setting("costs", &CostModel::default()).unwrap();
        assert_eq!(s.get_setting::<CostModel>("costs").unwrap(), Some(CostModel::default()));
    }
}

/// Nom du fichier de base dans le dossier de l'application.
pub const DB_FILE: &str = "papertrading.sqlite";

/// Ouvre la base de l'application dans `dir`. Le projet s'appelait
/// « PaperTrading2 » : si la base n'existe pas encore sous le nouveau nom, on
/// COPIE (sans rien effacer) l'ancienne, prise dans `dir` ou dans le dossier
/// voisin `com.raphi52.papertrading2`, avec ses fichiers -wal/-shm.
pub fn open_app_store(dir: &Path) -> Result<Store> {
    let target = dir.join(DB_FILE);
    if !target.exists() {
        let legacy_dirs = [
            Some(dir.to_path_buf()),
            dir.parent().map(|p| p.join("com.raphi52.papertrading2")),
        ];
        for d in legacy_dirs.into_iter().flatten() {
            let old = d.join("papertrading2.sqlite");
            if old.exists() {
                for suffix in ["", "-wal", "-shm"] {
                    let src = d.join(format!("papertrading2.sqlite{suffix}"));
                    if src.exists() {
                        std::fs::copy(&src, dir.join(format!("{DB_FILE}{suffix}")))
                            .with_context(|| format!("reprise de {}", src.display()))?;
                    }
                }
                break;
            }
        }
    }
    Store::open(target)
}

#[cfg(test)]
mod reprise_ancien_nom {
    use super::*;

    #[test]
    fn copie_la_base_du_dossier_papertrading2() {
        let root = std::env::temp_dir().join(format!("pt-reprise-{}", std::process::id()));
        let old = root.join("com.raphi52.papertrading2");
        let new = root.join("com.raphi52.papertrading");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&new).unwrap();
        drop(Store::open(old.join("papertrading2.sqlite")).unwrap());
        drop(open_app_store(&new).unwrap());
        assert!(new.join(DB_FILE).exists());
        assert!(old.join("papertrading2.sqlite").exists(), "l'ancienne base reste intacte");
        let _ = std::fs::remove_dir_all(&root);
    }
}
