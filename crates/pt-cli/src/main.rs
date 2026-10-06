//! `pt` : backtests en ligne de commande, sur les vraies bougies Binance.
//!
//! Exemples :
//!   pt presets
//!   pt backtest --preset supertrend_10_3_4h --symbols BTCUSDT,ETHUSDT --days 1095
//!   pt compare --symbols BTCUSDT,ETHUSDT,SOLUSDT,BNBUSDT,XRPUSDT --days 1095 --md rapport.md
//!   pt validate --preset macd_1d --preset donchian_55_20_1d --days 3650 --md validation.md

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use pt_core::backtest::{evaluate, BacktestReport, EvalSettings};
use pt_core::validation::{rolling_validation, tested_strategies_in_catalog, Outcome, RollingConfig, RollingReport};
use pt_core::{catalog, CostModel, External, Preset, Timeframe};
use pt_data::{fetch_fear_greed, BinanceClient, HistoryCache};
use std::collections::BTreeMap;
use std::path::PathBuf;

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
        tested_strategies: tested_strategies_in_catalog(),
        initial_cash: c.cash,
    })
}

fn settings(c: &Common, period_start: i64) -> EvalSettings {
    EvalSettings { costs: costs(c), initial_cash: c.cash, oos_fraction: c.oos, period_start, rolling: rolling_cfg(c) }
}

/// Début de l'historique à charger : la période affichée, ou plus pour la validation.
fn history_start(c: &Common, now: i64) -> i64 {
    let days = if c.no_validation { c.days } else { c.days.max(c.validation_days) };
    now - days * 86_400_000
}

#[derive(Subcommand)]
enum Cmd {
    /// Liste le catalogue des stratégies.
    Presets,
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
        /// Nombre de stratégies essayées avant de retenir celles-ci (par défaut : tout le catalogue).
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
    async fn get(
        &mut self,
        preset: &Preset,
        symbols: &[String],
        start: i64,
    ) -> Result<(BTreeMap<String, Vec<pt_core::Candle>>, External)> {
        let tf = preset.timeframe;
        if !self.series.contains_key(&tf) {
            eprintln!("… bougies {tf} pour {}", symbols.join(", "));
            let s = self.cache.series(symbols, tf, start).await?;
            self.series.insert(tf, s);
        }
        let mut ext = External::default();
        if preset.needs_fear_greed() {
            if self.fear_greed.is_none() {
                eprintln!("… historique Fear & Greed");
                self.fear_greed = Some(fetch_fear_greed().await?);
            }
            ext.fear_greed = self.fear_greed.clone().unwrap_or_default();
        }
        Ok((self.series[&tf].clone(), ext))
    }
}

fn costs(c: &Common) -> CostModel {
    CostModel { fee_rate: c.fee / 100.0, slippage_bps: c.slippage_bps }
}

fn symbols(c: &Common) -> Vec<String> {
    c.symbols.split(',').map(|s| s.trim().to_uppercase()).filter(|s| !s.is_empty()).collect()
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
            tested_strategies_in_catalog()
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
        Cmd::Presets => {
            for p in catalog() {
                println!("{:<26} {:<4} {:<22} {}", p.id, p.timeframe.as_str(), p.family, p.name);
            }
        }
        Cmd::Backtest { preset, common, json } => {
            let Some(p) = pt_core::find(&preset) else { bail!("stratégie inconnue : {preset} (voir `pt presets`)") };
            let syms = symbols(&common);
            let (history, ext) = loader.get(&p, &syms, history_start(&common, now)).await?;
            let period_start = now - common.days * 86_400_000;
            let r = evaluate(&p, &history, &ext, settings(&common, period_start))?;
            print_report(&r);
            if let Some(path) = json {
                std::fs::write(&path, serde_json::to_string_pretty(&r)?)?;
                eprintln!("rapport écrit : {}", path.display());
            }
        }
        Cmd::Compare { common, md } => {
            let syms = symbols(&common);
            let start = history_start(&common, now);
            let period_start = now - common.days * 86_400_000;
            let mut rows = Vec::new();
            for p in catalog() {
                let (history, ext) = loader.get(&p, &syms, start).await?;
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
        Cmd::Validate { presets, symbols: raw, days, window, step, tested, fee, slippage_bps, cash, md } => {
            let syms: Vec<String> = raw.split(',').map(|s| s.trim().to_uppercase()).filter(|s| !s.is_empty()).collect();
            let start = now - days * 86_400_000;
            let cfg = RollingConfig {
                window_days: window,
                step_days: step,
                tested_strategies: tested.unwrap_or_else(tested_strategies_in_catalog),
                initial_cash: cash,
            };
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
    #[test]
    fn date_formatting() {
        assert_eq!(super::date(0), "1970-01-01");
        assert_eq!(super::date(1_791_244_800_000), "2026-10-06");
    }
}
