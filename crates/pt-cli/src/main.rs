//! `pt` : backtests en ligne de commande, sur les vraies bougies Binance.
//!
//! Exemples :
//!   pt presets
//!   pt backtest --preset supertrend_10_3_4h --symbols BTCUSDT,ETHUSDT --days 1095
//!   pt compare --symbols BTCUSDT,ETHUSDT,SOLUSDT,BNBUSDT,XRPUSDT --days 1095 --md rapport.md
//!   pt validate --preset macd_1d --preset donchian_55_20_1d --days 3650 --md validation.md
//!   pt walkforward --lookback 2 --md docs/walkforward.md
//!   pt essais

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use pt_core::backtest::{evaluate, history_start, BacktestReport, EvalSettings};
use pt_core::carry::{carry_catalog, carry_fingerprint, carry_validation, find_carry, CarryPreset, CarryReport};
use pt_core::essais;
use pt_core::validation::{rolling_validation, tested_strategies, Outcome, RollingConfig, RollingReport};
use pt_core::walkforward::{score_candidates, walk_forward, Candidate, WalkForwardReport};
use pt_core::{catalog, CostModel, External, Preset, Timeframe};
use pt_data::{fetch_fear_greed, BinanceClient, FundingCache, FundingClient, HistoryCache};
use std::collections::BTreeMap;
use std::path::PathBuf;

const DAY_MS: i64 = 86_400_000;

#[derive(Parser)]
#[command(name = "pt", about = "PaperTrading2 — backtests honnêtes, frais compris, contre « acheter et garder »")]
struct Cli {
    /// Dossier du cache de bougies.
    #[arg(long, default_value = "data/cache", global = true)]
    cache: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(clap::Args, Clone)]
struct Common {
    /// Symboles Binance spot, séparés par des virgules.
    #[arg(long, default_value = "BTCUSDT,ETHUSDT,SOLUSDT,BNBUSDT,XRPUSDT")]
    symbols: String,
    /// Profondeur d'historique, en jours.
    #[arg(long, default_value_t = 1095)]
    days: i64,
    /// Part finale de la période gardée hors échantillon.
    #[arg(long, default_value_t = 0.3)]
    oos: f64,
    /// Frais par côté, en pourcentage.
    #[arg(long, default_value_t = 0.1)]
    fee: f64,
    /// Glissement par côté, en points de base.
    #[arg(long, default_value_t = 2.0)]
    slippage_bps: f64,
    #[arg(long, default_value_t = 10_000.0)]
    cash: f64,
    /// Historique de la validation sur fenêtres glissantes, en jours.
    #[arg(long, default_value_t = 3650)]
    validation_days: i64,
    /// Durée d'une fenêtre de validation, en jours.
    #[arg(long, default_value_t = 180)]
    window: i64,
    /// Décalage entre deux fenêtres de validation, en jours.
    #[arg(long, default_value_t = 90)]
    step: i64,
    /// Sans validation : plus rapide, mais le verdict ne peut pas dépasser « À valider ».
    #[arg(long = "sans-validation")]
    no_validation: bool,
}

fn rolling_cfg(c: &Common) -> Option<RollingConfig> {
    (!c.no_validation).then(|| RollingConfig {
        window_days: c.window,
        step_days: c.step,
        tested_strategies: tested_strategies(),
        initial_cash: c.cash,
    })
}

fn settings(c: &Common, period_start: i64) -> EvalSettings {
    EvalSettings { costs: costs(c), initial_cash: c.cash, oos_fraction: c.oos, period_start, rolling: rolling_cfg(c) }
}

/// Début de l'historique à charger sur `tf` : la période affichée plus le plus long
/// préchauffage du catalogue (pour que toutes les stratégies affichent la même
/// période), ou plus encore pour la validation.
fn load_from(c: &Common, now: i64, tf: Timeframe) -> i64 {
    let from = history_start(now - c.days * DAY_MS, tf);
    if c.no_validation {
        from
    } else {
        from.min(now - c.validation_days * DAY_MS)
    }
}

#[derive(Subcommand)]
enum Cmd {
    /// Liste le catalogue des stratégies.
    Presets,
    /// Registre des essais : combien de stratégies et de variantes ont été essayées.
    Essais,
    /// Backtest d'une stratégie.
    Backtest {
        #[arg(long)]
        preset: String,
        #[command(flatten)]
        common: Common,
        /// Écrit le rapport complet en JSON.
        #[arg(long)]
        json: Option<PathBuf>,
    },
    /// Compare tout le catalogue contre « acheter et garder ».
    Compare {
        #[command(flatten)]
        common: Common,
        /// Écrit le tableau en Markdown.
        #[arg(long)]
        md: Option<PathBuf>,
    },
    /// Sélection glissante : pour chaque fenêtre, joue la stratégie qui a le mieux
    /// battu le hasard sur les fenêtres déjà terminées, puis juge ce procédé.
    Walkforward {
        #[arg(long, default_value = "BTCUSDT,ETHUSDT,SOLUSDT,BNBUSDT,XRPUSDT")]
        symbols: String,
        /// Profondeur d'historique, en jours.
        #[arg(long, default_value_t = 3650)]
        days: i64,
        /// Durée d'une fenêtre, en jours.
        #[arg(long, default_value_t = 180)]
        window: i64,
        /// Décalage entre deux fenêtres, en jours.
        #[arg(long, default_value_t = 90)]
        step: i64,
        /// Fenêtres terminées regardées pour choisir (plusieurs valeurs : 1,2,4 ; chacune est un essai).
        #[arg(long, value_delimiter = ',', default_value = "2")]
        lookback: Vec<usize>,
        #[arg(long, default_value_t = 0.1)]
        fee: f64,
        #[arg(long, default_value_t = 2.0)]
        slippage_bps: f64,
        #[arg(long, default_value_t = 10_000.0)]
        cash: f64,
        /// Écrit le rapport en Markdown.
        #[arg(long)]
        md: Option<PathBuf>,
    },
    /// Crée un portefeuille suivi en direct par l'application (dans sa base), avec sa
    /// référence « acheter et garder » démarrée au même instant.
    Suivre {
        #[arg(long)]
        preset: String,
        #[arg(long, default_value = "BTCUSDT,ETHUSDT,SOLUSDT,BNBUSDT,XRPUSDT")]
        symbols: String,
        #[arg(long, default_value_t = 10_000.0)]
        cash: f64,
        /// Nom affiché dans l'application.
        #[arg(long)]
        nom: String,
        /// Dossier des données de l'application. Par défaut : `PT_DATA_DIR`, sinon le
        /// dossier de l'application (Windows : `%APPDATA%\com.raphi52.papertrading2`).
        #[arg(long)]
        donnees: Option<PathBuf>,
    },
    /// Portage du financement des perpétuels : validation sur fenêtres glissantes.
    Portage {
        /// Variante (option répétable) ; par défaut, toutes.
        #[arg(long = "preset")]
        presets: Vec<String>,
        #[arg(long, default_value = "BTCUSDT,ETHUSDT,SOLUSDT,BNBUSDT,XRPUSDT")]
        symbols: String,
        /// Profondeur d'historique, en jours (le financement Binance commence en septembre 2019).
        #[arg(long, default_value_t = 3650)]
        days: i64,
        #[arg(long, default_value_t = 180)]
        window: i64,
        #[arg(long, default_value_t = 90)]
        step: i64,
        /// Frais du comptant par côté, en %.
        #[arg(long, default_value_t = 0.1)]
        fee: f64,
        #[arg(long, default_value_t = 2.0)]
        slippage_bps: f64,
        #[arg(long, default_value_t = 10_000.0)]
        cash: f64,
        #[arg(long)]
        md: Option<PathBuf>,
    },
    /// Est-ce un coup de chance ? Rejoue des stratégies sur des fenêtres glissantes.
    Validate {
        /// Stratégie à valider (option répétable).
        #[arg(long = "preset", required = true)]
        presets: Vec<String>,
        #[arg(long, default_value = "BTCUSDT,ETHUSDT,SOLUSDT,BNBUSDT,XRPUSDT")]
        symbols: String,
        /// Profondeur d'historique, en jours (les symboles cotés plus tard entrent dès qu'ils sont prêts).
        #[arg(long, default_value_t = 3650)]
        days: i64,
        /// Durée d'une fenêtre, en jours.
        #[arg(long, default_value_t = 180)]
        window: i64,
        /// Décalage entre deux fenêtres, en jours.
        #[arg(long, default_value_t = 90)]
        step: i64,
        /// Nombre de stratégies essayées avant de retenir celles-ci (par défaut, et au minimum : le registre des essais).
        #[arg(long)]
        tested: Option<usize>,
        #[arg(long, default_value_t = 0.1)]
        fee: f64,
        #[arg(long, default_value_t = 2.0)]
        slippage_bps: f64,
        #[arg(long, default_value_t = 10_000.0)]
        cash: f64,
        /// Écrit le rapport en Markdown.
        #[arg(long)]
        md: Option<PathBuf>,
    },
}

fn validation_markdown(r: &RollingReport) -> String {
    let t = &r.sign_test;
    let mut s =
        format!("## {} — {}\n\n**{}** — {}\n\n", r.preset_name, r.timeframe, r.verdict.label(), r.verdict_reason);
    s.push_str(&format!(
        "- {} fenêtres de {} jours, décalées de {} jours ; le test garde une fenêtre sur {} pour qu'elles ne se chevauchent pas (découpage le moins favorable retenu) : {} gagnée(s), {} perdue(s), {} nulle(s).\n",
        r.windows.len(), r.config.window_days, r.config.step_days, r.stride, t.wins, t.losses, t.ties
    ));
    s.push_str(&format!(
        "- Probabilité à pile ou face : {:.4} ; corrigée pour {} stratégies essayées : {:.4}.\n",
        t.p_value, r.config.tested_strategies, r.p_adjusted
    ));
    s.push_str(&format!(
        "- Fenêtres en gain : {}/{} · battant le Sharpe d'« acheter et garder » : {}/{} · écart médian avec la référence à exposition égale : {:+.1} points.\n",
        r.positive_windows, r.windows.len(), r.beats_benchmark_sharpe, r.windows.len(), r.median_excess_pct
    ));
    s.push_str(&format!(
        "- Pire fenêtre : {:+.1} % (« acheter et garder » : pire fenêtre {:+.1} %).\n\n",
        r.worst_return_pct, r.worst_benchmark_return_pct
    ));
    s.push_str("| Début | Fin | Symboles | Trades | Exposition | Stratégie | Acheter-garder | Réf. à expo. égale | Écart | Pire baisse | Réf. | Résultat |\n");
    s.push_str("|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|\n");
    for w in &r.windows {
        s.push_str(&format!(
            "| {} | {} | {} | {} | {:.0} % | {:+.1} % | {:+.1} % | {:+.1} % | {:+.1} | {:.1} % | {:.1} % | {} |\n",
            date(w.start),
            date(w.end - 1),
            w.symbols.len(),
            w.trades,
            w.exposure_pct,
            w.return_pct,
            w.benchmark_return_pct,
            w.matched_return_pct,
            w.excess_pct,
            w.max_drawdown_pct,
            w.benchmark_max_drawdown_pct,
            match w.outcome {
                Outcome::Win => "gagnée",
                Outcome::Loss => "perdue",
                Outcome::Tie => "nulle",
            }
        ));
    }
    s.push('\n');
    s
}

fn date(ms: i64) -> String {
    // Conversion jours → date civile (algorithme de Howard Hinnant).
    let z = ms.div_euclid(86_400_000) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}

struct Loader {
    cache: HistoryCache,
    fear_greed: Option<Vec<(i64, f64)>>,
    series: BTreeMap<Timeframe, BTreeMap<String, Vec<pt_core::Candle>>>,
}

impl Loader {
    /// Charge (une fois par unité de temps) ce dont `preset` a besoin.
    async fn ensure(&mut self, preset: &Preset, symbols: &[String], start: i64) -> Result<()> {
        let tf = preset.timeframe;
        if !self.series.contains_key(&tf) {
            eprintln!("… bougies {tf} pour {}", symbols.join(", "));
            let s = self.cache.series(symbols, tf, start).await?;
            self.series.insert(tf, s);
        }
        if preset.needs_fear_greed() && self.fear_greed.is_none() {
            eprintln!("… historique Fear & Greed");
            self.fear_greed = Some(fetch_fear_greed().await?);
        }
        Ok(())
    }

    async fn get(
        &mut self,
        preset: &Preset,
        symbols: &[String],
        start: i64,
    ) -> Result<(BTreeMap<String, Vec<pt_core::Candle>>, External)> {
        self.ensure(preset, symbols, start).await?;
        let mut ext = External::default();
        if preset.needs_fear_greed() {
            ext.fear_greed = self.fear_greed.clone().unwrap_or_default();
        }
        Ok((self.series[&preset.timeframe].clone(), ext))
    }
}

/// Valeurs de `--lookback` : au moins 1, sans doublon.
fn parse_lookbacks(asked: &[usize]) -> Result<Vec<usize>> {
    let mut out: Vec<usize> = Vec::new();
    for &l in asked {
        if l == 0 {
            bail!("--lookback 0 refusé : il faut au moins une fenêtre terminée pour choisir");
        }
        if !out.contains(&l) {
            out.push(l);
        }
    }
    if out.is_empty() {
        bail!("--lookback attend au moins une valeur, par exemple 2");
    }
    Ok(out)
}

/// Une variante de sélection ne se lance sur les données réelles qu'une fois
/// inscrite au registre : l'essayer puis l'oublier rendrait le verdict plus
/// flatteur qu'il n'est. Rend le nombre de variantes à compter.
fn registered_selections(fingerprints: &[String], today: &str) -> Result<usize> {
    let reg = essais::registry();
    let missing = essais::unregistered_selections(&reg, fingerprints);
    if !missing.is_empty() {
        let lines: Vec<String> = missing.iter().map(|f| format!("{today}\tselection\twalkforward\t{f}")).collect();
        bail!(
            "variante(s) de sélection absente(s) du registre des essais. Ajoute d'abord ces lignes à {} (séparateur : tabulation) ; elles compteront dans la correction :\n{}",
            essais::REGISTRY_PATH,
            lines.join("\n")
        );
    }
    Ok(essais::selection_trials_in(&reg, fingerprints))
}

/// Dossier de données de l'application de bureau : le même que Tauri (`app_data_dir`).
fn app_data_dir(asked: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(d) = asked {
        return Ok(d);
    }
    if let Ok(d) = std::env::var("PT_DATA_DIR") {
        return Ok(d.into());
    }
    let base = std::env::var("APPDATA")
        .or_else(|_| std::env::var("XDG_DATA_HOME"))
        .or_else(|_| std::env::var("HOME").map(|h| format!("{h}/.local/share")))
        .map_err(|_| anyhow::anyhow!("dossier de données introuvable : précise --donnees"))?;
    Ok(PathBuf::from(base).join("com.raphi52.papertrading2"))
}

fn carry_markdown(r: &CarryReport) -> String {
    let t = &r.sign_test;
    let mut s = format!("## {}\n\n**{}** — {}\n\n", r.preset_name, r.verdict.label(), r.verdict_reason);
    s.push_str(&format!(
        "- {} fenêtres de {} jours, décalées de {} jours ; le test garde une fenêtre sur {} pour qu'elles ne se chevauchent pas (découpage le moins favorable retenu) : {} gagnée(s), {} perdue(s), {} nulle(s).\n",
        r.windows.len(), r.window_days, r.step_days, r.stride, t.wins, t.losses, t.ties
    ));
    s.push_str(&format!(
        "- Une fenêtre est gagnée si elle finit en gain : le portage n'est pas exposé au prix, sa référence à exposition égale vaut 0 %.\n- Probabilité à pile ou face : {:.6} ; corrigée pour {} stratégies essayées : {:.6}.\n",
        t.p_value, r.tested_strategies, r.p_adjusted
    ));
    s.push_str(&format!(
        "- Rendement médian par fenêtre : {:+.2} % · pire fenêtre : {:+.2} % · fenêtres indépendantes mises bout à bout : {:+.1} % par an.\n\n",
        r.median_return_pct, r.worst_return_pct, r.annualized_pct
    ));
    s.push_str("| Début | Fin | Symboles | Rendement | dont financement | Frais | Pire baisse | Acheter-garder | Pire baisse réf. | Couvert | Opérations | Liquidations | Résultat |\n");
    s.push_str("|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|\n");
    for w in &r.windows {
        s.push_str(&format!(
            "| {} | {} | {} | {:+.2} % | {:+.2} % | {:.2} % | {:.2} % | {:+.1} % | {:.1} % | {:.0} % | {} | {} | {} |\n",
            date(w.start),
            date(w.end - 1),
            w.symbols.len(),
            w.return_pct,
            w.funding_pct,
            w.fees_pct,
            w.max_drawdown_pct,
            w.benchmark_return_pct,
            w.benchmark_max_drawdown_pct,
            w.hedged_pct,
            w.trades,
            w.liquidations,
            outcome_label(w.outcome)
        ));
    }
    s.push('\n');
    s
}

fn outcome_label(o: Outcome) -> &'static str {
    match o {
        Outcome::Win => "gagnée",
        Outcome::Loss => "perdue",
        Outcome::Tie => "nulle",
    }
}

fn walkforward_markdown(r: &WalkForwardReport) -> String {
    let t = &r.sign_test;
    let mut s = format!(
        "## Choix sur les {} dernière(s) fenêtre(s) terminée(s)\n\nVerdict : {}\n\n{}\n\n",
        r.lookback,
        r.verdict.label(),
        r.verdict_reason
    );
    s.push_str(&format!(
        "- {} fenêtre(s) jouée(s) sur {} ; {} stratégies candidates ; fenêtres de {} jours décalées de {} jours. Le choix d'une fenêtre ne regarde que les fenêtres terminées avant son début.\n",
        r.played, r.windows.len(), r.candidates, r.window_days, r.step_days
    ));
    s.push_str(&format!(
        "- Test du signe sur une fenêtre sur {} (découpage le moins favorable) : {} gagnée(s), {} perdue(s), {} nulle(s) ou non jouée(s). Probabilité à pile ou face : {:.4} ; corrigée pour {} variante(s) de sélection essayée(s) : {:.4}.\n",
        r.stride, t.wins, t.losses, t.ties, t.p_value, r.tested_selections, r.p_adjusted
    ));
    s.push_str(&format!(
        "- Écart médian avec un achat au hasard de même exposition : {:+.1} points par fenêtre jouée.\n",
        r.median_excess_pct
    ));
    s.push_str(&format!(
        "- Les {} fenêtres jouées de ce découpage, mises bout à bout (elles ne se chevauchent pas) : sélection {:+.1} %, achat au hasard de même exposition {:+.1} %, « acheter et garder » {:+.1} %.\n",
        r.independent_played, r.compounded_return_pct, r.compounded_matched_pct, r.compounded_benchmark_pct
    ));
    let picks: Vec<String> = r.picks.iter().map(|(id, n)| format!("{id} ×{n}")).collect();
    s.push_str(&format!("- Stratégies choisies : {}.\n\n", picks.join(", ")));
    s.push_str("| Début | Fin | Stratégie choisie | Écart passé | Expo. | Rendement | Acheter-garder | Hasard à expo. égale | Écart | Résultat |\n");
    s.push_str("|---|---|---|---:|---:|---:|---:|---:|---:|---|\n");
    for w in &r.windows {
        match (&w.result, &w.chosen_name, w.past_score) {
            (Some(x), Some(name), Some(score)) => s.push_str(&format!(
                "| {} | {} | {} | {:+.1} | {:.0} % | {:+.1} % | {:+.1} % | {:+.1} % | {:+.1} | {} |\n",
                date(w.start),
                date(w.end - 1),
                name,
                score,
                x.exposure_pct,
                x.return_pct,
                x.benchmark_return_pct,
                x.matched_return_pct,
                x.excess_pct,
                outcome_label(x.outcome)
            )),
            _ => s.push_str(&format!(
                "| {} | {} | — (pas assez de fenêtres terminées) | | | | | | | non jouée |\n",
                date(w.start),
                date(w.end - 1)
            )),
        }
    }
    s.push('\n');
    s
}

/// `--tested` : par défaut le registre ; jamais en dessous (ce serait oublier des essais).
fn tested_count(asked: Option<usize>) -> Result<usize> {
    let floor = tested_strategies();
    match asked {
        None => Ok(floor),
        Some(n) if n < floor => bail!(
            "--tested {n} refusé : le registre des essais ({}) compte déjà {floor} stratégies essayées. Une valeur plus petite rendrait le verdict plus flatteur qu'il n'est.",
            essais::REGISTRY_PATH
        ),
        Some(n) => Ok(n),
    }
}

fn costs(c: &Common) -> CostModel {
    CostModel { fee_rate: c.fee / 100.0, slippage_bps: c.slippage_bps }
}

/// Symboles : en majuscules, sans doublon, jamais vide.
fn parse_symbols(raw: &str) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for s in raw.split(',').map(|s| s.trim().to_uppercase()).filter(|s| !s.is_empty()) {
        if !out.contains(&s) {
            out.push(s);
        }
    }
    if out.is_empty() {
        bail!("aucun symbole : --symbols attend une liste comme BTCUSDT,ETHUSDT");
    }
    Ok(out)
}

fn check_range(name: &str, value: i64, range: std::ops::RangeInclusive<i64>) -> Result<()> {
    if !range.contains(&value) {
        bail!("{name} {value} refusé : entre {} et {} jours", range.start(), range.end());
    }
    Ok(())
}

/// Mêmes bornes que l'application, vérifiées AVANT tout téléchargement ou calcul.
fn check_common(c: &Common) -> Result<Vec<String>> {
    check_range("--days", c.days, 30..=3650)?;
    check_range("--validation-days", c.validation_days, 365..=3650)?;
    RollingConfig { window_days: c.window, step_days: c.step, tested_strategies: 1, initial_cash: c.cash }.check()?;
    parse_symbols(&c.symbols)
}

fn print_report(r: &BacktestReport) {
    let m = &r.metrics;
    let b = &r.benchmark;
    println!("\n{} — {} — {}", r.preset_name, r.symbols.join(", "), r.timeframe);
    println!(
        "Période : {} → {} (hors échantillon depuis {})",
        date(r.start_time),
        date(r.end_time),
        date(r.split_time)
    );
    println!("{:<28}{:>14}{:>14}", "", "Stratégie", "Acheter-garder");
    println!("{:<28}{:>13.1}%{:>13.1}%", "Rendement total", m.total_return_pct, b.total_return_pct);
    println!("{:<28}{:>13.1}%{:>13.1}%", "Rendement annualisé", m.cagr_pct, b.cagr_pct);
    println!("{:<28}{:>13.1}%{:>13.1}%", "Pire baisse (max drawdown)", m.max_drawdown_pct, b.max_drawdown_pct);
    println!("{:<28}{:>14.2}{:>14.2}", "Sharpe", m.sharpe, b.sharpe);
    println!("{:<28}{:>14.2}{:>14.2}", "Sharpe hors échantillon", r.oos.sharpe, r.benchmark_oos.sharpe);
    println!(
        "{:<28}{:>13.1}%{:>13.1}%",
        "Rendement hors échantillon", r.oos.total_return_pct, r.benchmark_oos.total_return_pct
    );
    println!("{:<28}{:>14}", "Trades", m.trades);
    println!("{:<28}{:>13.1}%", "Trades gagnants", m.win_rate_pct);
    println!("{:<28}{:>14}", "Facteur de profit", m.profit_factor.map(|p| format!("{p:.2}")).unwrap_or("—".into()));
    println!("{:<28}{:>13.2}%{:>13.2}%", "Gain moyen / perte moyenne", m.avg_win_pct, m.avg_loss_pct);
    println!("{:<28}{:>13.1}%", "Temps investi", m.exposure_pct);
    println!("{:<28}{:>13.1}%", "Hasard à exposition égale", r.matched_return_pct);
    println!("{:<28}{:>14.2}", "Frais payés", r.fees_paid);
    println!("{:<28}{:>13.1}%", "Rendement sans frais", r.gross_return_pct);
    if let Some(v) = &r.validation {
        let t = &v.sign_test;
        println!(
            "Validation : {} fenêtres indépendantes gagnées sur {} · probabilité {:.1} % · corrigée {:.1} %",
            t.wins,
            t.wins + t.losses,
            t.p_value * 100.0,
            v.p_adjusted * 100.0
        );
    }
    println!("Verdict : {} — {}", r.verdict.label(), r.verdict_reason);
}

/// Classement : verdict, puis probabilité corrigée, puis écart avec le hasard.
fn by_conviction(a: &BacktestReport, b: &BacktestReport) -> std::cmp::Ordering {
    let p = |r: &BacktestReport| r.validation.as_ref().map_or(1.0, |v| v.p_adjusted);
    let raw = |r: &BacktestReport| r.validation.as_ref().map_or(1.0, |v| v.sign_test.p_value);
    let edge = |r: &BacktestReport| r.metrics.total_return_pct - r.matched_return_pct;
    a.verdict
        .rank()
        .cmp(&b.verdict.rank())
        .then(p(a).total_cmp(&p(b)))
        .then(raw(a).total_cmp(&raw(b)))
        .then(edge(b).total_cmp(&edge(a)))
}

fn markdown(rows: &[BacktestReport], syms: &[String], c: &Common) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "# Comparaison du catalogue — {}\n\nFrais {:.2} % par côté, glissement {} pb. Résultats affichés sur {} jours. ",
        syms.join(", "),
        c.fee,
        c.slippage_bps,
        c.days,
    ));
    if c.no_validation {
        s.push_str("**Sans validation** : aucun verdict ne peut dépasser « À valider ».\n\n");
    } else {
        s.push_str(&format!(
            "Verdict décidé par une validation sur {} jours : fenêtres de {} jours décalées de {} jours, chacune comparée à un achat au hasard de même exposition, test du signe sur fenêtres indépendantes, corrigé pour {} stratégies essayées.\n\n",
            c.validation_days,
            c.window,
            c.step,
            tested_strategies()
        ));
    }
    s.push_str("« Hasard à expo. égale » = `(1 + R)^f − 1` : ce qu'un achat au hasard, investi la même part du temps `f`, obtient en moyenne quand « acheter et garder » fait `R`.\n\n");
    s.push_str("| Stratégie | UT | Trades | Rendement | Acheter-garder | Expo. | Hasard à expo. égale | Pire baisse | Réf. | Sans frais | Fenêtres gagnées | p corrigé | Verdict |\n");
    s.push_str("|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|\n");
    for r in rows {
        let (windows, p_adj) = match &r.validation {
            Some(v) => (
                format!("{} / {}", v.sign_test.wins, v.sign_test.wins + v.sign_test.losses),
                format!("{:.1} %", v.p_adjusted * 100.0),
            ),
            None => ("—".into(), "—".into()),
        };
        s.push_str(&format!(
            "| {} | {} | {} | {:+.1} % | {:+.1} % | {:.0} % | {:+.1} % | {:.1} % | {:.1} % | {:+.1} % | {} | {} | **{}** |\n",
            r.preset_name,
            r.timeframe,
            r.metrics.trades,
            r.metrics.total_return_pct,
            r.benchmark.total_return_pct,
            r.metrics.exposure_pct,
            r.matched_return_pct,
            r.metrics.max_drawdown_pct,
            r.benchmark.max_drawdown_pct,
            r.gross_return_pct,
            windows,
            p_adj,
            r.verdict.label()
        ));
    }
    s
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut loader = Loader {
        cache: HistoryCache::new(&cli.cache, BinanceClient::new()),
        fear_greed: None,
        series: BTreeMap::new(),
    };
    let now = pt_data::now_ms();
    match cli.cmd {
        Cmd::Essais => {
            let reg = essais::registry();
            let missing = essais::unregistered_strategies(&reg, &catalog(), &date(now));
            println!("Stratégies à compter dans la correction : {} (registre ∪ catalogue).", tested_strategies());
            println!("Variantes de sélection inscrites : {}.", essais::selection_trials_in(&reg, &[]));
            let carry_missing: Vec<String> = carry_catalog()
                .iter()
                .filter(|p| !reg.iter().any(|t| t.id == p.id && t.fingerprint == carry_fingerprint(p)))
                .map(|p| format!("{}\tstrategie\t{}\t{}", date(now), p.id, carry_fingerprint(p)))
                .collect();
            if !carry_missing.is_empty() {
                println!("Variantes de portage absentes de {} : ajoute ces lignes.", essais::REGISTRY_PATH);
                for l in &carry_missing {
                    println!("{l}");
                }
            }
            if missing.is_empty() {
                println!("Toutes les stratégies du catalogue sont inscrites dans {}.", essais::REGISTRY_PATH);
            } else {
                println!("Stratégies du catalogue absentes de {} : ajoute ces lignes.", essais::REGISTRY_PATH);
                for t in &missing {
                    println!("{}", t.line());
                }
            }
        }
        Cmd::Presets => {
            for p in catalog() {
                println!("{:<26} {:<4} {:<22} {}", p.id, p.timeframe.as_str(), p.family, p.name);
            }
        }
        Cmd::Backtest { preset, common, json } => {
            let Some(p) = pt_core::find(&preset) else { bail!("stratégie inconnue : {preset} (voir `pt presets`)") };
            let syms = check_common(&common)?;
            let (history, ext) = loader.get(&p, &syms, load_from(&common, now, p.timeframe)).await?;
            let period_start = now - common.days * 86_400_000;
            let r = evaluate(&p, &history, &ext, settings(&common, period_start))?;
            print_report(&r);
            if let Some(path) = json {
                std::fs::write(&path, serde_json::to_string_pretty(&r)?)?;
                eprintln!("rapport écrit : {}", path.display());
            }
        }
        Cmd::Compare { common, md } => {
            let syms = check_common(&common)?;
            let period_start = now - common.days * 86_400_000;
            let mut rows = Vec::new();
            for p in catalog() {
                let (history, ext) = loader.get(&p, &syms, load_from(&common, now, p.timeframe)).await?;
                let res = evaluate(&p, &history, &ext, settings(&common, period_start));
                match res {
                    Ok(r) => {
                        eprintln!("  {:<26} {:>8.1} %  {}", p.id, r.metrics.total_return_pct, r.verdict.label());
                        rows.push(r);
                    }
                    Err(e) => eprintln!("  {:<26} ignorée : {e}", p.id),
                }
            }
            rows.sort_by(by_conviction);
            let text = markdown(&rows, &syms, &common);
            println!("{text}");
            if let Some(path) = md {
                std::fs::write(&path, &text)?;
                eprintln!("tableau écrit : {}", path.display());
            }
        }
        Cmd::Walkforward { symbols: raw, days, window, step, lookback, fee, slippage_bps, cash, md } => {
            let syms = parse_symbols(&raw)?;
            check_range("--days", days, 365..=3650)?;
            RollingConfig { window_days: window, step_days: step, tested_strategies: 1, initial_cash: cash }.check()?;
            let lookbacks = parse_lookbacks(&lookback)?;
            let fingerprints: Vec<String> =
                lookbacks.iter().map(|l| essais::selection_fingerprint(*l, window, step)).collect();
            let tested = registered_selections(&fingerprints, &date(now))?;
            let start = now - days * DAY_MS;
            let presets: Vec<Preset> = catalog().into_iter().filter(|p| !p.is_benchmark()).collect();
            for p in &presets {
                loader.ensure(p, &syms, start).await?;
            }
            let ext = External { fear_greed: loader.fear_greed.clone().unwrap_or_default() };
            let candidates: Vec<Candidate> = presets
                .iter()
                .map(|p| Candidate { preset: p, series: &loader.series[&p.timeframe], ext: &ext })
                .collect();
            let cost = CostModel { fee_rate: fee / 100.0, slippage_bps };
            eprintln!("… {} stratégies rejouées sur une grille commune de fenêtres", candidates.len());
            let scores = score_candidates(&candidates, cost, window, step, cash)?;
            let mut text = format!(
                "# Sélection glissante — {}\n\nFrais {fee:.2} % par côté, glissement {slippage_bps} pb, historique demandé : {days} jours. Les {} stratégies du catalogue (la référence exclue) sont rejouées sur la même grille de fenêtres ; chaque fenêtre repart de zéro et se compare à un achat au hasard de même exposition, `(1 + R)^f − 1`. Pour chaque fenêtre, on joue la stratégie qui a le mieux battu ce hasard, en moyenne, sur les dernières fenêtres TERMINÉES avant son début. Variantes de sélection comptées dans la correction : {tested} (registre des essais ∪ variantes demandées).\n\n",
                syms.join(", "),
                candidates.len()
            );
            for l in &lookbacks {
                let r = walk_forward(&scores, *l, tested);
                eprintln!("  lookback {l} : {} — {}", r.verdict.label(), r.verdict_reason);
                text.push_str(&walkforward_markdown(&r));
            }
            println!("{text}");
            if let Some(path) = md {
                std::fs::write(&path, &text)?;
                eprintln!("rapport écrit : {}", path.display());
            }
        }
        Cmd::Suivre { preset, symbols: raw, cash, nom, donnees } => {
            let Some(p) = pt_core::find(&preset) else { bail!("stratégie inconnue : {preset} (voir `pt presets`)") };
            if p.is_benchmark() {
                bail!("la référence est déjà incluse dans chaque portefeuille");
            }
            let name = nom.trim();
            if name.is_empty() || name.chars().count() > 80 {
                bail!("donne un nom au portefeuille (80 caractères au plus)");
            }
            if !(100.0..=1e9).contains(&cash) {
                bail!("le capital doit être entre 100 et 1 milliard");
            }
            let mut syms = parse_symbols(&raw)?;
            syms.sort();
            let client = BinanceClient::new();
            for s in &syms {
                client
                    .recent(s, p.timeframe, 5)
                    .await
                    .map_err(|e| anyhow::anyhow!("{s} introuvable sur Binance spot : {e:#}"))?;
            }
            let dir = app_data_dir(donnees)?;
            std::fs::create_dir_all(&dir)?;
            let mut store = pt_store::Store::open(dir.join("papertrading2.sqlite"))?;
            // Les frais réglés dans l'application, sinon ceux par défaut.
            let costs = store
                .get_setting::<serde_json::Value>("settings")?
                .and_then(|v| serde_json::from_value::<CostModel>(v["costs"].clone()).ok())
                .unwrap_or_default();
            let created = pt_data::now_ms();
            let engine = pt_core::Engine::new(p.clone(), syms.clone(), cash, costs, created);
            let bh = pt_core::find("buy_hold").expect("référence au catalogue");
            let benchmark = pt_core::Engine::new(bh, syms.clone(), cash, costs, created);
            let id = store.create(name, created, &engine, &benchmark)?;
            println!(
                "portefeuille n° {id} « {name} » créé dans {} : {} sur {}, {cash:.0} $. L'application le fait avancer à son prochain passage.",
                dir.display(),
                p.name,
                syms.join(", ")
            );
        }
        Cmd::Portage { presets, symbols: raw, days, window, step, fee, slippage_bps, cash, md } => {
            let syms = parse_symbols(&raw)?;
            check_range("--days", days, 365..=3650)?;
            let start = now - days * DAY_MS;
            let cost = CostModel { fee_rate: fee / 100.0, slippage_bps };
            let chosen: Vec<CarryPreset> = if presets.is_empty() {
                carry_catalog()
            } else {
                presets
                    .iter()
                    .map(|id| find_carry(id).ok_or_else(|| anyhow::anyhow!("variante de portage inconnue : {id}")))
                    .collect::<Result<_>>()?
            };
            let funding_cache = FundingCache::new(&cli.cache, FundingClient::new());
            let mut prices = BTreeMap::new();
            let mut funding = BTreeMap::new();
            for s in &syms {
                eprintln!("… {s} : bougies 1h et taux de financement");
                prices.insert(s.clone(), loader.cache.history(s, Timeframe::H1, start).await?);
                funding.insert(s.clone(), funding_cache.history(s, start).await?);
            }
            let tested = tested_strategies();
            let mut text = format!(
                "# Portage du financement des perpétuels — {}\n\nAchat au comptant et vente du perpétuel en même quantité : le prix s'annule, le financement versé toutes les 8 h reste. La moitié de chaque poche sert de marge (levier 1×). Frais : comptant {fee:.2} % par côté, perpétuel {:.3} % par côté (preneur), glissement {slippage_bps} pb par jambe. L'écart de prix entre perpétuel et comptant n'est pas modélisé, et la trésorerie ne rapporte rien.\n\n",
                syms.join(", "),
                chosen.first().map_or(0.05, |p| p.perp_fee_rate * 100.0)
            );
            for p in &chosen {
                let r = carry_validation(p, &prices, &funding, cost, cash, window, step, tested)?;
                eprintln!("  {:<20} {} — {}", p.id, r.verdict.label(), r.verdict_reason);
                text.push_str(&carry_markdown(&r));
            }
            println!("{text}");
            if let Some(path) = md {
                std::fs::write(&path, &text)?;
                eprintln!("rapport écrit : {}", path.display());
            }
        }
        Cmd::Validate { presets, symbols: raw, days, window, step, tested, fee, slippage_bps, cash, md } => {
            let syms = parse_symbols(&raw)?;
            check_range("--days", days, 365..=3650)?;
            let start = now - days * 86_400_000;
            let cfg = RollingConfig {
                window_days: window,
                step_days: step,
                tested_strategies: tested_count(tested)?,
                initial_cash: cash,
            };
            cfg.check()?;
            let cost = CostModel { fee_rate: fee / 100.0, slippage_bps };
            let mut text = format!(
                "# Validation sur fenêtres glissantes — {}\n\nFrais {fee:.2} % par côté, glissement {slippage_bps} pb, historique demandé : {days} jours. Chaque fenêtre repart de zéro et se compare à « acheter et garder » ramené à la même exposition : `(1 + R)^f − 1`, ce qu'un timing au hasard investi la fraction `f` du temps obtient en moyenne.\n\n",
                syms.join(", ")
            );
            for id in &presets {
                let Some(p) = pt_core::find(id) else { bail!("stratégie inconnue : {id} (voir `pt presets`)") };
                let (series, ext) = loader.get(&p, &syms, start).await?;
                let r = rolling_validation(&p, &series, &ext, cost, cfg)?;
                eprintln!("  {:<22} {} — {}", p.id, r.verdict.label(), r.verdict_reason);
                text.push_str(&validation_markdown(&r));
            }
            println!("{text}");
            if let Some(path) = md {
                std::fs::write(&path, &text)?;
                eprintln!("rapport écrit : {}", path.display());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookback_values_are_checked() {
        assert!(parse_lookbacks(&[0]).is_err());
        assert!(parse_lookbacks(&[2, 0]).is_err());
        assert!(parse_lookbacks(&[]).is_err());
        assert_eq!(parse_lookbacks(&[2, 2, 1]).unwrap(), vec![2, 1]);
    }

    #[test]
    fn unregistered_selection_is_refused() {
        let registered = essais::selection_fingerprint(2, 180, 90);
        assert!(registered_selections(&[registered], "2026-10-06").unwrap() >= 1);
        let err = registered_selections(&[essais::selection_fingerprint(97, 180, 90)], "2026-10-06").unwrap_err();
        assert!(err.to_string().contains("lookback=97;window=180;step=90"), "{err}");
    }

    #[test]
    fn empty_symbol_list_is_refused() {
        assert!(parse_symbols("").is_err());
        assert!(parse_symbols(" , ,,").is_err());
        assert_eq!(parse_symbols("btcusdt").unwrap(), vec!["BTCUSDT"]);
    }

    #[test]
    fn repeated_symbols_count_once() {
        assert_eq!(parse_symbols("BTCUSDT, btcusdt,ETHUSDT,BTCUSDT").unwrap(), vec!["BTCUSDT", "ETHUSDT"]);
    }

    #[test]
    fn days_are_bounded_like_the_app() {
        assert!(check_range("--days", 0, 30..=3650).is_err());
        assert!(check_range("--days", -5, 30..=3650).is_err());
        assert!(check_range("--days", 3651, 30..=3650).is_err());
        assert!(check_range("--days", 30, 30..=3650).is_ok());
        assert!(check_range("--days", 3650, 30..=3650).is_ok());
    }

    #[test]
    fn tested_below_registry_is_refused() {
        let floor = tested_strategies();
        assert_eq!(tested_count(None).unwrap(), floor);
        assert!(tested_count(Some(0)).is_err());
        assert!(tested_count(Some(floor - 1)).is_err());
        assert_eq!(tested_count(Some(floor)).unwrap(), floor);
        assert_eq!(tested_count(Some(floor + 10)).unwrap(), floor + 10);
    }

    #[test]
    fn date_formatting() {
        assert_eq!(super::date(0), "1970-01-01");
        assert_eq!(super::date(1_791_244_800_000), "2026-10-06");
    }
}
