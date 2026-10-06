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
    m.exposure_pct = curve.iter().map(|p| p.exposure).sum::<f64>() / curve.len() as f64 * 100.0;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    /// La référence elle-même.
    Reference,
    /// Bat la référence (rendement ajusté du risque) sur toute la période ET hors échantillon.
    Solide,
    /// Ne bat la référence que sur une des deux périodes.
    Fragile,
    /// Ne bat pas la référence.
    Perdante,
    /// Trop peu de trades pour distinguer la stratégie du hasard.
    Insuffisant,
}

impl Verdict {
    pub fn label(&self) -> &'static str {
        match self {
            Verdict::Reference => "Référence",
            Verdict::Solide => "Solide",
            Verdict::Fragile => "Fragile",
            Verdict::Perdante => "Perdante",
            Verdict::Insuffisant => "Trop peu de trades",
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

/// Instant à partir duquel tous les symboles ont fini leur préchauffage.
pub fn common_start(preset: &Preset, series: &BTreeMap<String, Vec<Candle>>) -> Result<i64, BacktestError> {
    let w = preset.warmup();
    let mut start = i64::MIN;
    for (s, c) in series {
        if c.is_empty() {
            return Err(BacktestError::NoData(s.clone()));
        }
        if c.len() <= w + 2 {
            return Err(BacktestError::TooShort { needed: w + 3, got: c.len() });
        }
        start = start.max(c[w].open_time);
    }
    Ok(start)
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

    let (verdict, verdict_reason) = judge(preset, &metrics, &benchmark, &oos, &benchmark_oos);
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
        verdict,
        verdict_reason,
        curve,
        benchmark_curve: bench.curve,
        trades,
    })
}

pub fn judge(
    preset: &Preset,
    full: &Metrics,
    bench: &Metrics,
    oos: &Metrics,
    bench_oos: &Metrics,
) -> (Verdict, String) {
    if preset.is_benchmark() {
        return (Verdict::Reference, "C'est la référence.".into());
    }
    if full.trades < MIN_TRADES_FOR_VERDICT {
        return (
            Verdict::Insuffisant,
            format!(
                "{} trades seulement : il en faut au moins {MIN_TRADES_FOR_VERDICT} pour écarter le hasard.",
                full.trades
            ),
        );
    }
    let beats_full = full.sharpe > bench.sharpe;
    let beats_oos = oos.sharpe > bench_oos.sharpe && oos.total_return_pct > 0.0;
    // Une stratégie qui perd de l'argent, ou dont les gains ne couvrent pas les
    // pertes, est perdante quel que soit son comportement sur un sous-segment.
    let makes_money = full.total_return_pct > 0.0 && full.profit_factor.is_none_or(|p| p > 1.0);
    let pf_strong = full.profit_factor.is_none_or(|p| p > 1.1);
    let v = if !makes_money || (!beats_full && !beats_oos) {
        Verdict::Perdante
    } else if beats_full && beats_oos && pf_strong {
        Verdict::Solide
    } else {
        Verdict::Fragile
    };
    let reason = format!(
        "Sharpe {:.2} contre {:.2} pour la référence sur toute la période ; {:.2} contre {:.2} hors échantillon (rendement {:+.1} %). Facteur de profit {}.",
        full.sharpe,
        bench.sharpe,
        oos.sharpe,
        bench_oos.sharpe,
        oos.total_return_pct,
        full.profit_factor.map(|p| format!("{p:.2}")).unwrap_or_else(|| "sans perte".into())
    );
    (v, reason)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::synthetic;

    fn series(n: usize, seeds: &[u64]) -> BTreeMap<String, Vec<Candle>> {
        seeds.iter().map(|s| (format!("S{s}USDT"), synthetic(n, *s))).collect()
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
    }

    #[test]
    fn a_losing_strategy_is_always_perdante() {
        let p = catalog::find("stoch_rsi_4h").unwrap();
        let mk = |ret: f64, sharpe: f64, pf: Option<f64>| Metrics {
            total_return_pct: ret,
            sharpe,
            trades: 100,
            profit_factor: pf,
            ..Metrics::default()
        };
        let bench = mk(90.0, 0.7, None);
        let bench_oos = mk(-20.0, -0.2, None);
        // Perd 38 % au total mais gagne un peu sur la fin : reste perdante.
        let (v, _) = judge(&p, &mk(-38.0, -0.9, Some(0.8)), &bench, &mk(2.0, 0.1, Some(1.1)), &bench_oos);
        assert_eq!(v, Verdict::Perdante);
        // Gagne de l'argent mais avec un facteur de profit < 1 : impossible, donc perdante.
        let (v, _) = judge(&p, &mk(5.0, 0.8, Some(0.9)), &bench, &mk(2.0, 0.1, Some(1.1)), &bench_oos);
        assert_eq!(v, Verdict::Perdante);
        let (v, _) = judge(&p, &mk(40.0, 0.9, Some(1.5)), &bench, &mk(8.0, 0.9, Some(1.5)), &bench_oos);
        assert_eq!(v, Verdict::Solide);
        let (v, _) = judge(&p, &mk(40.0, 0.5, Some(1.5)), &bench, &mk(8.0, 0.9, Some(1.5)), &bench_oos);
        assert_eq!(v, Verdict::Fragile);
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
