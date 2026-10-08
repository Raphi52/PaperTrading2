//! Backtest, mesures de performance et verdict.
//!
//! Corrections par rapport à l'ancien bot :
//! - toute stratégie est comparée à « acheter et garder » sur la même période,
//!   avec les mêmes frais (l'ancien n'avait aucune référence fiable : son HODL
//!   revendait lui-même au bout de 3 jours) ;
//! - le verdict se juge aussi sur une période HORS ÉCHANTILLON, que l'on n'a pas
//!   regardée pour choisir la stratégie ; l'ancien optimiseur classait ses
//!   stratégies sur 16 jours de résultats ;
//! - le même backtest est rejoué sans frais, pour montrer ce que les frais coûtent.

use crate::candle::{Candle, Timeframe};
use crate::catalog;
use crate::engine::{Engine, Feed};
use crate::portfolio::{ClosedTrade, CostModel};
use crate::strategy::{External, Preset};
use crate::validation::{matched_return_pct, rolling_validation, Robustness, RollingConfig, RollingReport};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CurvePoint {
    pub time: i64,
    pub equity: f64,
    /// Part du capital investie, 0..1.
    pub exposure: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Metrics {
    pub start_equity: f64,
    pub final_equity: f64,
    pub total_return_pct: f64,
    pub cagr_pct: f64,
    pub max_drawdown_pct: f64,
    /// Rendement par unité de risque, annualisé (taux sans risque = 0).
    pub sharpe: f64,
    pub volatility_pct: f64,
    pub trades: usize,
    pub win_rate_pct: f64,
    pub profit_factor: Option<f64>,
    pub avg_win_pct: f64,
    pub avg_loss_pct: f64,
    pub expectancy_pct: f64,
    pub avg_bars_held: f64,
    pub exposure_pct: f64,
}

pub fn compute_metrics(curve: &[CurvePoint], trades: &[ClosedTrade], tf: Timeframe) -> Metrics {
    let mut m = Metrics::default();
    if curve.len() < 2 {
        return m;
    }
    let first = curve[0].equity;
    let last = curve[curve.len() - 1].equity;
    m.start_equity = first;
    m.final_equity = last;
    m.total_return_pct = (last / first - 1.0) * 100.0;
    let years = (curve[curve.len() - 1].time - curve[0].time) as f64 / (365.0 * 86_400_000.0);
    m.cagr_pct = if years > 0.0 && last > 0.0 { ((last / first).powf(1.0 / years) - 1.0) * 100.0 } else { 0.0 };
    let mut peak = f64::MIN;
    let mut mdd: f64 = 0.0;
    for p in curve {
        peak = peak.max(p.equity);
        mdd = mdd.max((peak - p.equity) / peak);
    }
    m.max_drawdown_pct = mdd * 100.0;
    let rets: Vec<f64> = curve.windows(2).map(|w| w[1].equity / w[0].equity - 1.0).collect();
    let mean = rets.iter().sum::<f64>() / rets.len() as f64;
    let var = rets.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (rets.len().max(2) - 1) as f64;
    let sd = var.sqrt();
    let k = tf.bars_per_year().sqrt();
    m.sharpe = if sd > 1e-12 { mean / sd * k } else { 0.0 };
    m.volatility_pct = sd * k * 100.0;
    // Le rendement entre deux points est gagné avec les positions détenues pendant la
    // bougie du SECOND point : le premier point (avant tout achat) ne compte pas.
    let held = &curve[1..];
    m.exposure_pct = held.iter().map(|p| p.exposure).sum::<f64>() / held.len() as f64 * 100.0;

    m.trades = trades.len();
    if !trades.is_empty() {
        let wins: Vec<&ClosedTrade> = trades.iter().filter(|t| t.pnl > 0.0).collect();
        let losses: Vec<&ClosedTrade> = trades.iter().filter(|t| t.pnl <= 0.0).collect();
        m.win_rate_pct = wins.len() as f64 / trades.len() as f64 * 100.0;
        let gw: f64 = wins.iter().map(|t| t.pnl).sum();
        let gl: f64 = -losses.iter().map(|t| t.pnl).sum::<f64>();
        m.profit_factor = if gl > 0.0 { Some(gw / gl) } else { None };
        m.avg_win_pct =
            if wins.is_empty() { 0.0 } else { wins.iter().map(|t| t.return_pct).sum::<f64>() / wins.len() as f64 };
        m.avg_loss_pct = if losses.is_empty() {
            0.0
        } else {
            losses.iter().map(|t| t.return_pct).sum::<f64>() / losses.len() as f64
        };
        m.expectancy_pct = trades.iter().map(|t| t.return_pct).sum::<f64>() / trades.len() as f64;
        m.avg_bars_held = trades.iter().map(|t| t.bars_held as f64).sum::<f64>() / trades.len() as f64;
    }
    m
}

/// Verdict d'une stratégie.
///
/// « Solide » ne peut sortir QUE d'une validation sur fenêtres glissantes : un
/// backtest sur une seule période ne distingue pas le talent de la chance. La
/// comparaison se fait contre « acheter et garder » ramené à la même exposition
/// (voir `validation`), jamais contre son Sharpe brut, qui favorise les stratégies
/// peu investies dans les marchés baissiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    /// La référence elle-même.
    Reference,
    /// Bat un timing au hasard de même exposition, et la validation montre que ce
    /// n'est pas de la chance, même en comptant toutes les stratégies essayées.
    Solide,
    /// Significative seule, mais plus une fois comptées toutes les stratégies essayées.
    Prometteuse,
    /// Gagne de l'argent, mais rien ne la distingue d'un timing au hasard.
    Hasard,
    /// Bat la référence à exposition égale sur la période ; validation pas encore faite.
    AValider,
    /// Perd de l'argent, ou fait moins bien qu'un timing au hasard de même exposition.
    Perdante,
    /// Trop peu de trades ou de fenêtres pour juger.
    Insuffisant,
}

impl Verdict {
    pub fn label(&self) -> &'static str {
        match self {
            Verdict::Reference => "Référence",
            Verdict::Solide => "Solide",
            Verdict::Prometteuse => "Prometteuse",
            Verdict::Hasard => "Indiscernable du hasard",
            Verdict::AValider => "À valider",
            Verdict::Perdante => "Perdante",
            Verdict::Insuffisant => "Trop peu de données",
        }
    }

    /// Ordre de classement : le plus convaincant d'abord.
    pub fn rank(&self) -> u8 {
        match self {
            Verdict::Solide => 0,
            Verdict::Prometteuse => 1,
            Verdict::Reference => 2,
            Verdict::AValider => 3,
            Verdict::Hasard => 4,
            Verdict::Perdante => 5,
            Verdict::Insuffisant => 6,
        }
    }
}

pub const MIN_TRADES_FOR_VERDICT: usize = 20;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BacktestReport {
    pub preset_id: String,
    pub preset_name: String,
    pub timeframe: Timeframe,
    pub symbols: Vec<String>,
    pub start_time: i64,
    pub end_time: i64,
    /// Début de la période hors échantillon.
    pub split_time: i64,
    pub costs: CostModel,
    pub fees_paid: f64,
    pub metrics: Metrics,
    pub benchmark: Metrics,
    pub oos: Metrics,
    pub benchmark_oos: Metrics,
    /// Rendement du même backtest sans aucun frais.
    pub gross_return_pct: f64,
    /// « Acheter et garder » ramené à l'exposition moyenne de la stratégie : (1 + R)^f − 1.
    pub matched_return_pct: f64,
    /// Validation sur fenêtres glissantes, quand elle a été demandée et a pu se faire.
    pub validation: Option<RollingReport>,
    /// Pourquoi la validation demandée n'a pas pu se faire.
    pub validation_error: Option<String>,
    pub verdict: Verdict,
    pub verdict_reason: String,
    pub curve: Vec<CurvePoint>,
    pub benchmark_curve: Vec<CurvePoint>,
    pub trades: Vec<ClosedTrade>,
}

#[derive(Debug, thiserror::Error)]
pub enum BacktestError {
    #[error("aucune donnée pour {0}")]
    NoData(String),
    #[error("historique trop court : {needed} bougies nécessaires, {got} disponibles sur la période commune")]
    TooShort { needed: usize, got: usize },
}

pub struct Run {
    pub engine: Engine,
    pub curve: Vec<CurvePoint>,
}

/// Instant à partir duquel le PREMIER symbole a fini son préchauffage. Chaque
/// crypto n'entre ensuite qu'après son propre préchauffage (voir `Engine::advance`) :
/// une crypto cotée tard n'ampute plus l'historique des autres, et aucune crypto
/// n'est utilisée avant sa date de cotation.
pub fn common_start(preset: &Preset, series: &BTreeMap<String, Vec<Candle>>) -> Result<i64, BacktestError> {
    let w = preset.warmup();
    let mut start: Option<i64> = None;
    let mut longest = 0;
    for (s, c) in series {
        if c.is_empty() {
            return Err(BacktestError::NoData(s.clone()));
        }
        longest = longest.max(c.len());
        if c.len() > w + 2 {
            start = Some(start.map_or(c[w].open_time, |x| x.min(c[w].open_time)));
        }
    }
    start.ok_or(BacktestError::TooShort { needed: w + 3, got: longest })
}

/// Rejoue le moteur sur des séries de bougies CLÔTURÉES.
pub fn run(
    preset: &Preset,
    series: &BTreeMap<String, Vec<Candle>>,
    ext: &External,
    costs: CostModel,
    initial_cash: f64,
    trade_from: i64,
) -> Run {
    let symbols: Vec<String> = series.keys().cloned().collect();
    let mut engine = Engine::new(preset.clone(), symbols, initial_cash, costs, trade_from);
    let feeds: BTreeMap<String, Feed> =
        series.iter().map(|(s, c)| (s.clone(), Feed { closed: c, forming: None })).collect();
    let mut curve = Vec::new();
    engine.advance(&feeds, ext, |tick, e| {
        if tick.open_time >= trade_from {
            curve.push(CurvePoint {
                time: tick.close_time,
                equity: e.equity(),
                exposure: e.portfolio.exposure(&e.marks),
            });
        }
    });
    Run { engine, curve }
}

fn segment(curve: &[CurvePoint], from: i64) -> &[CurvePoint] {
    let i = curve.partition_point(|p| p.time < from);
    &curve[i.saturating_sub(1).min(curve.len().saturating_sub(1))..]
}

/// Backtest complet : stratégie, référence, hors échantillon, sans frais, verdict.
pub fn backtest(
    preset: &Preset,
    series: &BTreeMap<String, Vec<Candle>>,
    ext: &External,
    costs: CostModel,
    initial_cash: f64,
    oos_fraction: f64,
) -> Result<BacktestReport, BacktestError> {
    let start = common_start(preset, series)?;
    let main = run(preset, series, ext, costs, initial_cash, start);
    let gross = run(preset, series, ext, CostModel::zero(), initial_cash, start);
    let bh = catalog::find("buy_hold").expect("référence au catalogue");
    let bench = run(&bh, series, ext, costs, initial_cash, start);

    let curve = main.curve;
    if curve.len() < 10 {
        return Err(BacktestError::TooShort { needed: preset.warmup() + 10, got: curve.len() });
    }
    let end = curve[curve.len() - 1].time;
    let first = curve[0].time;
    let split = first + ((end - first) as f64 * (1.0 - oos_fraction.clamp(0.1, 0.9))) as i64;

    let trades = main.engine.portfolio.closed.clone();
    let tf = preset.timeframe;
    let metrics = compute_metrics(&curve, &trades, tf);
    let benchmark = compute_metrics(&bench.curve, &[], tf);
    let oos_trades: Vec<ClosedTrade> = trades.iter().filter(|t| t.exit_time >= split).cloned().collect();
    let oos = compute_metrics(segment(&curve, split), &oos_trades, tf);
    let benchmark_oos = compute_metrics(segment(&bench.curve, split), &[], tf);
    let gross_return_pct = gross.curve.last().map(|p| (p.equity / initial_cash - 1.0) * 100.0).unwrap_or(0.0);
    let matched_return_pct = matched_return_pct(benchmark.total_return_pct, metrics.exposure_pct / 100.0);

    let (verdict, verdict_reason) = judge(preset, &metrics, &benchmark, None);
    debug_assert!(main.engine.portfolio.check_invariants().is_ok());

    Ok(BacktestReport {
        preset_id: preset.id.clone(),
        preset_name: preset.name.clone(),
        timeframe: tf,
        symbols: series.keys().cloned().collect(),
        start_time: first,
        end_time: end,
        split_time: split,
        costs,
        fees_paid: main.engine.portfolio.fees_paid,
        metrics,
        benchmark,
        oos,
        benchmark_oos,
        gross_return_pct,
        matched_return_pct,
        validation: None,
        validation_error: None,
        verdict,
        verdict_reason,
        curve,
        benchmark_curve: bench.curve,
        trades,
    })
}

/// Juge une stratégie : sur la période affichée contre la référence à exposition
/// égale, puis — si elle a été faite — d'après la validation sur fenêtres glissantes.
/// Sans validation, le meilleur verdict possible est « À valider ».
pub fn judge(
    preset: &Preset,
    full: &Metrics,
    bench: &Metrics,
    validation: Option<&RollingReport>,
) -> (Verdict, String) {
    if preset.is_benchmark() {
        return (Verdict::Reference, "C'est la référence.".into());
    }
    let f = full.exposure_pct / 100.0;
    let matched = matched_return_pct(bench.total_return_pct, f);
    let pf_text = full.profit_factor.map(|p| format!("{p:.2}")).unwrap_or_else(|| "sans perte".into());
    let period = format!(
        "Sur la période : {:+.1} % contre {:+.1} % pour un achat au hasard investi {:.0} % du temps (« acheter et garder » : {:+.1} %). Facteur de profit {pf_text}.",
        full.total_return_pct,
        matched,
        f * 100.0,
        bench.total_return_pct,
    );
    // Perd de l'argent, gains qui ne couvrent pas les pertes, ou moins bien qu'un
    // timing au hasard de même exposition.
    let period_fails = full.total_return_pct <= 0.0
        || full.profit_factor.is_some_and(|p| p <= 1.0)
        || full.total_return_pct <= matched;
    match validation {
        None => {
            if full.trades < MIN_TRADES_FOR_VERDICT {
                let why = format!(
                    "{} trades seulement : il en faut au moins {MIN_TRADES_FOR_VERDICT} pour juger.",
                    full.trades
                );
                (Verdict::Insuffisant, format!("{why} {period}"))
            } else if period_fails {
                (Verdict::Perdante, period)
            } else {
                (
                    Verdict::AValider,
                    format!("{period} Un seul historique ne distingue pas le talent de la chance : lance la validation sur fenêtres glissantes."),
                )
            }
        }
        Some(v) => {
            // Perdre la majorité des fenêtres contre le hasard, c'est faire moins bien
            // que le hasard : perdante, même si la période affichée finit en gain.
            let loses_most_windows = v.sign_test.losses > v.sign_test.wins;
            let verdict = match v.verdict {
                Robustness::PasDeLaChance => Verdict::Solide,
                Robustness::Prometteuse => Verdict::Prometteuse,
                Robustness::TropPeuDeFenetres => Verdict::Insuffisant,
                Robustness::CompatibleAvecLaChance if period_fails || loses_most_windows => Verdict::Perdante,
                Robustness::CompatibleAvecLaChance => Verdict::Hasard,
            };
            (verdict, format!("{} {period}", v.verdict_reason))
        }
    }
}

/// Bougies postérieures ou égales à `start`, pour chaque symbole.
pub fn since(series: &BTreeMap<String, Vec<Candle>>, start: i64) -> BTreeMap<String, Vec<Candle>> {
    series.iter().map(|(s, c)| (s.clone(), c[c.partition_point(|x| x.open_time < start)..].to_vec())).collect()
}

/// Bougies à partir de `start`, plus les `warmup` bougies qui le précèdent : la
/// stratégie se préchauffe AVANT la période, qui commence donc bien à `start`
/// (comme chaque fenêtre de la validation).
pub fn with_warmup(series: &BTreeMap<String, Vec<Candle>>, start: i64, warmup: usize) -> BTreeMap<String, Vec<Candle>> {
    series
        .iter()
        .map(|(s, c)| {
            let first = c.partition_point(|x| x.open_time < start).saturating_sub(warmup);
            (s.clone(), c[first..].to_vec())
        })
        .collect()
}

/// Début de l'historique à charger pour afficher une période commençant à
/// `period_start` sur l'unité de temps `tf` : la période, plus le plus long
/// préchauffage du catalogue sur cette unité de temps. Le même historique sert
/// ainsi à toutes les stratégies, qui affichent toutes la même période.
pub fn history_start(period_start: i64, tf: Timeframe) -> i64 {
    period_start - catalog::longest_warmup(tf) as i64 * tf.millis()
}

/// Réglages d'une évaluation complète.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EvalSettings {
    pub costs: CostModel,
    pub initial_cash: f64,
    /// Part finale de la période affichée gardée hors échantillon (informatif).
    pub oos_fraction: f64,
    /// Début de la période affichée.
    pub period_start: i64,
    /// Validation sur fenêtres glissantes ; sans elle, le verdict plafonne à « À valider ».
    pub rolling: Option<RollingConfig>,
}

/// Ce qui est montré à l'utilisateur ET ce qui décide du verdict : un backtest sur la
/// période affichée (depuis `period_start`), plus, si `rolling` est fourni, une
/// validation sur fenêtres glissantes sur TOUT l'historique fourni.
pub fn evaluate(
    preset: &Preset,
    history: &BTreeMap<String, Vec<Candle>>,
    ext: &External,
    s: EvalSettings,
) -> Result<BacktestReport, BacktestError> {
    let costs = s.costs;
    let period = with_warmup(history, s.period_start, preset.warmup());
    let mut report = backtest(preset, &period, ext, costs, s.initial_cash, s.oos_fraction)?;
    if preset.is_benchmark() {
        return Ok(report);
    }
    if let Some(cfg) = s.rolling {
        match rolling_validation(preset, history, ext, costs, cfg) {
            Ok(v) => report.validation = Some(v),
            Err(e) => report.validation_error = Some(e.to_string()),
        }
        let (verdict, mut reason) = judge(preset, &report.metrics, &report.benchmark, report.validation.as_ref());
        if let Some(e) = &report.validation_error {
            reason = format!("Validation impossible : {e}. {reason}");
        }
        report.verdict = verdict;
        report.verdict_reason = reason;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::synthetic;

    fn series(n: usize, seeds: &[u64]) -> BTreeMap<String, Vec<Candle>> {
        seeds.iter().map(|s| (format!("S{s}EUR"), synthetic(n, *s))).collect()
    }

    #[test]
    fn backtest_is_deterministic_and_consistent() {
        let s = series(2500, &[1, 2, 3]);
        let preset = catalog::find("supertrend_7_2_1h").unwrap();
        let a = backtest(&preset, &s, &External::default(), CostModel::default(), 10_000.0, 0.3).unwrap();
        let b = backtest(&preset, &s, &External::default(), CostModel::default(), 10_000.0, 0.3).unwrap();
        assert_eq!(a, b, "deux exécutions doivent donner exactement le même résultat");
        assert!(a.metrics.trades > 0);
        assert!(
            a.gross_return_pct >= a.metrics.total_return_pct - 1e-9,
            "les frais ne peuvent pas améliorer le résultat"
        );
        assert!(a.split_time > a.start_time && a.split_time < a.end_time);
    }

    #[test]
    fn every_preset_keeps_accounting_invariants() {
        let s = series(2500, &[4, 5]);
        let ext = External {
            fear_greed: s
                .values()
                .next()
                .unwrap()
                .iter()
                .enumerate()
                .map(|(i, c)| (c.open_time, ((i * 13) % 100) as f64))
                .collect(),
        };
        for p in catalog::catalog() {
            let start = common_start(&p, &s).unwrap();
            let r = run(&p, &s, &ext, CostModel::default(), 10_000.0, start);
            r.engine.portfolio.check_invariants().unwrap_or_else(|e| panic!("{} : {e}", p.id));
            assert!(r.curve.iter().all(|x| x.equity > 0.0 && x.exposure <= 1.0 + 1e-9), "{}", p.id);
            for t in &r.engine.portfolio.closed {
                assert!(t.exit_time >= t.entry_time, "{} : sortie avant l'entrée", p.id);
            }
        }
    }

    #[test]
    fn benchmark_matches_hand_computation() {
        let s = series(400, &[8]);
        let bh = catalog::find("buy_hold").unwrap();
        let costs = CostModel::default();
        let rep = backtest(&bh, &s, &External::default(), costs, 1_000.0, 0.3).unwrap();
        let c = s.values().next().unwrap();
        let start = common_start(&bh, &s).unwrap();
        let i = c.iter().position(|x| x.open_time == start).unwrap();
        let qty = 1_000.0 * (1.0 - costs.fee_rate) / costs.buy_price(c[i + 1].open);
        let expected = qty * c.last().unwrap().close;
        assert!((rep.metrics.final_equity - expected).abs() < 1e-6, "{} vs {}", rep.metrics.final_equity, expected);
        assert_eq!(rep.verdict, Verdict::Reference);
        // La référence est investie en permanence : exposition 100 %, et la
        // référence à exposition égale est donc la référence elle-même.
        assert!((rep.metrics.exposure_pct - 100.0).abs() < 1e-9, "{}", rep.metrics.exposure_pct);
        assert!((rep.matched_return_pct - rep.benchmark.total_return_pct).abs() < 1e-9);
    }

    fn mk(ret: f64, sharpe: f64, pf: Option<f64>, exposure_pct: f64) -> Metrics {
        Metrics { total_return_pct: ret, sharpe, trades: 100, profit_factor: pf, exposure_pct, ..Metrics::default() }
    }

    fn rolling(verdict: Robustness) -> RollingReport {
        RollingReport {
            preset_id: "x".into(),
            preset_name: "x".into(),
            timeframe: Timeframe::D1,
            config: RollingConfig::default(),
            costs: CostModel::default(),
            windows: vec![],
            stride: 2,
            sign_test: crate::validation::SignTest { phase: 0, wins: 0, losses: 0, ties: 0, p_value: 1.0 },
            p_adjusted: 1.0,
            positive_windows: 0,
            beats_benchmark_sharpe: 0,
            median_excess_pct: 0.0,
            worst_return_pct: 0.0,
            worst_benchmark_return_pct: 0.0,
            verdict,
            verdict_reason: "validation".into(),
        }
    }

    /// Un seul historique ne peut JAMAIS rendre « Solide » : c'était le défaut du
    /// premier comparateur, qui classait « Solide » ce que la validation a ensuite
    /// montré compatible avec la chance.
    #[test]
    fn a_single_period_can_never_be_solide() {
        let p = catalog::find("macd_1d").unwrap();
        let bench = mk(94.0, 0.71, None, 100.0);
        let (v, reason) = judge(&p, &mk(35.0, 0.87, Some(1.6), 15.0), &bench, None);
        assert_eq!(v, Verdict::AValider);
        assert!(reason.contains("validation"), "{reason}");
    }

    /// Le biais corrigé : en marché haussier, une stratégie peu investie qui gagne
    /// moins qu'un achat au hasard de même exposition est perdante, même si son
    /// Sharpe dépasse celui d'« acheter et garder ».
    #[test]
    fn judged_against_the_same_exposure_not_the_raw_sharpe() {
        let p = catalog::find("stoch_rsi_4h").unwrap();
        let bench = mk(100.0, 0.7, None, 100.0);
        // 30 % investi quand la référence double : un achat au hasard ferait 2^0,3 − 1 ≈ +23,1 %.
        let (v, _) = judge(&p, &mk(20.0, 1.2, Some(1.5), 30.0), &bench, None);
        assert_eq!(v, Verdict::Perdante, "Sharpe meilleur, mais moins bien que le hasard à exposition égale");
        let (v, _) = judge(&p, &mk(30.0, 1.2, Some(1.5), 30.0), &bench, None);
        assert_eq!(v, Verdict::AValider);
        // Perte d'argent, ou gains qui ne couvrent pas les pertes : perdante.
        let bear = mk(-50.0, -0.8, None, 100.0);
        assert_eq!(judge(&p, &mk(-3.0, -0.1, Some(0.8), 10.0), &bear, None).0, Verdict::Perdante);
        assert_eq!(judge(&p, &mk(5.0, 0.8, Some(0.9), 10.0), &bench, None).0, Verdict::Perdante);
    }

    #[test]
    fn validation_decides_the_verdict() {
        let p = catalog::find("macd_1d").unwrap();
        let bench = mk(94.0, 0.71, None, 100.0);
        let good = mk(35.0, 0.87, Some(1.6), 15.0);
        let bad = mk(5.0, 0.3, Some(1.2), 15.0); // sous les +10,4 % du hasard à 15 %
        let cases = [
            (Robustness::PasDeLaChance, &good, Verdict::Solide),
            (Robustness::Prometteuse, &good, Verdict::Prometteuse),
            (Robustness::CompatibleAvecLaChance, &good, Verdict::Hasard),
            (Robustness::CompatibleAvecLaChance, &bad, Verdict::Perdante),
            (Robustness::TropPeuDeFenetres, &good, Verdict::Insuffisant),
        ];
        for (robustness, m, expected) in cases {
            let (v, reason) = judge(&p, m, &bench, Some(&rolling(robustness)));
            assert_eq!(v, expected, "{robustness:?}");
            assert!(reason.starts_with("validation"), "la raison commence par celle de la validation");
        }
        // Finit en gain sur la période, mais perd 10 fenêtres sur 14 contre le hasard
        // (cas réel du RSI(2) de Connors) : perdante, pas « indiscernable du hasard ».
        let mut mostly_lost = rolling(Robustness::CompatibleAvecLaChance);
        mostly_lost.sign_test.wins = 4;
        mostly_lost.sign_test.losses = 10;
        assert_eq!(judge(&p, &good, &bench, Some(&mostly_lost)).0, Verdict::Perdante);
        mostly_lost.sign_test.wins = 9;
        mostly_lost.sign_test.losses = 8;
        assert_eq!(judge(&p, &good, &bench, Some(&mostly_lost)).0, Verdict::Hasard);
    }

    /// Propriété sur tout le catalogue : « Solide » si et seulement si la validation
    /// conclut « pas un coup de chance » ; jamais sans validation.
    #[test]
    fn solide_only_ever_comes_from_the_validation() {
        let day = 86_400_000;
        let history: BTreeMap<String, Vec<Candle>> = [7u64, 8]
            .iter()
            .map(|s| {
                let c = synthetic(1500, *s)
                    .into_iter()
                    .enumerate()
                    .map(|(i, mut c)| {
                        c.open_time = i as i64 * day;
                        c.close_time = c.open_time + day - 1;
                        c
                    })
                    .collect();
                (format!("S{s}EUR"), c)
            })
            .collect();
        let period_start = 700 * day;
        let cfg = RollingConfig { window_days: 120, step_days: 60, tested_strategies: 29, initial_cash: 10_000.0 };
        let costs = CostModel::default();
        let ext = External::default();
        for p in catalog::catalog().into_iter().filter(|p| p.timeframe == Timeframe::D1) {
            let base = EvalSettings { costs, initial_cash: 10_000.0, oos_fraction: 0.3, period_start, rolling: None };
            let alone = evaluate(&p, &history, &ext, base).unwrap();
            assert!(
                matches!(
                    alone.verdict,
                    Verdict::AValider | Verdict::Perdante | Verdict::Insuffisant | Verdict::Reference
                ),
                "{} : {:?} sans validation",
                p.id,
                alone.verdict
            );
            let full = evaluate(&p, &history, &ext, EvalSettings { rolling: Some(cfg), ..base }).unwrap();
            if p.is_benchmark() {
                assert!(full.validation.is_none());
                continue;
            }
            let v = full.validation.as_ref().expect("validation faite");
            assert_eq!(full.verdict == Verdict::Solide, v.verdict == Robustness::PasDeLaChance, "{}", p.id);
            // Le backtest affiché ne dépend pas de la validation.
            assert_eq!(full.metrics, alone.metrics, "{}", p.id);
        }
    }

    fn on_timeframe(n: usize, seeds: &[u64], tf: Timeframe) -> BTreeMap<String, Vec<Candle>> {
        let step = tf.millis();
        seeds
            .iter()
            .map(|s| {
                let c = synthetic(n, *s)
                    .into_iter()
                    .enumerate()
                    .map(|(i, mut c)| {
                        c.open_time = i as i64 * step;
                        c.close_time = c.open_time + step - 1;
                        c
                    })
                    .collect();
                (format!("S{s}EUR"), c)
            })
            .collect()
    }

    /// Toutes les stratégies d'une même unité de temps affichent la MÊME période,
    /// qui commence à la date demandée : le préchauffage se fait sur les bougies
    /// d'avant. (Avant : il était pris dans la période, d'une durée différente pour
    /// chaque stratégie ; « acheter et garder » allait de −8,6 % à +129,7 % sur la
    /// même période demandée.)
    #[test]
    fn every_strategy_shows_the_same_period() {
        let costs = CostModel::default();
        let ext = External::default();
        for tf in [Timeframe::H1, Timeframe::H4, Timeframe::D1] {
            let history = on_timeframe(1800, &[11, 12], tf);
            let period_start = 900 * tf.millis();
            let settings =
                EvalSettings { costs, initial_cash: 10_000.0, oos_fraction: 0.3, period_start, rolling: None };
            let reports: Vec<BacktestReport> = catalog::catalog()
                .into_iter()
                .filter(|p| p.timeframe == tf)
                .map(|p| evaluate(&p, &history, &ext, settings).unwrap())
                .collect();
            assert!(reports.len() > 2, "{tf} : trop peu de stratégies pour le test");
            for r in &reports {
                assert_eq!(
                    r.start_time,
                    period_start + tf.millis() - 1,
                    "{} ({tf}) commence à la clôture de la première bougie de la période",
                    r.preset_id
                );
                assert_eq!(r.benchmark, reports[0].benchmark, "{} ({tf}) : autre « acheter et garder »", r.preset_id);
            }
        }
    }

    /// L'historique chargé couvre le préchauffage de toutes les stratégies de
    /// l'unité de temps, même sans validation : la période commence bien à la
    /// date demandée.
    #[test]
    fn history_includes_warmup_without_validation() {
        let costs = CostModel::default();
        let ext = External::default();
        for tf in [Timeframe::H1, Timeframe::H4, Timeframe::D1] {
            let full = on_timeframe(2000, &[13, 14], tf);
            let period_start = 1500 * tf.millis();
            let from = history_start(period_start, tf);
            let loaded = since(&full, from);
            let settings =
                EvalSettings { costs, initial_cash: 10_000.0, oos_fraction: 0.3, period_start, rolling: None };
            for p in catalog::catalog().into_iter().filter(|p| p.timeframe == tf) {
                assert!(
                    from <= period_start - p.warmup() as i64 * tf.millis(),
                    "{} : l'historique chargé ne couvre pas son préchauffage",
                    p.id
                );
                let r = evaluate(&p, &loaded, &ext, settings).unwrap();
                assert_eq!(r.start_time, period_start + tf.millis() - 1, "{} ({tf})", p.id);
            }
        }
    }

    #[test]
    fn drawdown_and_return_on_known_curve() {
        let pts: Vec<CurvePoint> = [100.0, 120.0, 90.0, 110.0]
            .iter()
            .enumerate()
            .map(|(i, e)| CurvePoint { time: i as i64 * 86_400_000, equity: *e, exposure: 1.0 })
            .collect();
        let m = compute_metrics(&pts, &[], Timeframe::D1);
        assert!((m.total_return_pct - 10.0).abs() < 1e-9);
        assert!((m.max_drawdown_pct - 25.0).abs() < 1e-9);
    }
}
