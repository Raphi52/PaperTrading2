//! Stratégies : des fonctions PURES des bougies clôturées.
//!
//! Corrections par rapport à l'ancien bot :
//! - chaque stratégie calcule réellement l'indicateur qui lui donne son nom
//!   (l'ancien « Williams %R », « CCI » et « Order Block » exécutaient tous le
//!   même code Stoch RSI) ;
//! - les sorties sont propres à chaque stratégie : aucune règle globale ne
//!   coupe un gagnant à +0,3 % (l'ancien « SECURE PROFIT » s'appliquait à
//!   toutes, même au « HODL ») ;
//! - aucune donnée simulée au hasard : seules les bougies réelles et l'indice
//!   Fear & Greed publié servent de signal.

// Les séries parallèles (prix, indicateurs, signaux) se lisent plus clairement par indice.
#![allow(clippy::needless_range_loop)]

use crate::candle::{Candle, Timeframe};
use crate::indicators as ind;
use serde::{Deserialize, Serialize};

/// Données externes publiques, datées, utilisables sans regarder le futur.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct External {
    /// Indice Fear & Greed : (horodatage ms du jour publié, valeur 0..100), trié.
    pub fear_greed: Vec<(i64, f64)>,
}

impl External {
    /// Dernière valeur connue à `close_time` (publication supposée disponible
    /// une heure après son horodatage, pour ne jamais lire une valeur future).
    pub fn fear_greed_at(&self, close_time: i64) -> Option<f64> {
        let limit = close_time - 3_600_000;
        let idx = self.fear_greed.partition_point(|(t, _)| *t <= limit);
        if idx == 0 {
            None
        } else {
            Some(self.fear_greed[idx - 1].1)
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signal {
    pub enter: bool,
    pub exit: bool,
}

/// La règle de décision et ses paramètres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Rule {
    /// Référence : achète une fois et ne vend jamais.
    BuyHold,
    EmaCross {
        fast: usize,
        slow: usize,
    },
    MacdCross {
        fast: usize,
        slow: usize,
        signal: usize,
    },
    RsiReversion {
        period: usize,
        oversold: f64,
        exit_level: f64,
    },
    StochRsi {
        oversold: f64,
        overbought: f64,
    },
    BollingerReversion {
        period: usize,
        k: f64,
    },
    DonchianBreakout {
        entry: usize,
        exit: usize,
    },
    Supertrend {
        period: usize,
        mult: f64,
    },
    Ichimoku {
        tenkan: usize,
        kijun: usize,
        senkou: usize,
    },
    AdxTrend {
        period: usize,
        threshold: f64,
        ema: usize,
    },
    KeltnerBreakout {
        ema: usize,
        atr: usize,
        mult: f64,
    },
    WilliamsR {
        period: usize,
        oversold: f64,
        exit_level: f64,
    },
    CciReversion {
        period: usize,
        entry: f64,
        exit_level: f64,
    },
    VwapReversion {
        period: usize,
        deviation_pct: f64,
    },
    DipBuy {
        lookback: usize,
        dip_pct: f64,
    },
    FearGreed {
        buy_below: f64,
        sell_above: f64,
    },
    /// Score 0..5 : tendance EMA50/200, MACD, RSI sain, Supertrend, ADX.
    Confluence {
        min_score: u8,
        exit_score: u8,
    },
}

impl Rule {
    /// Nombre de bougies nécessaires avant que les signaux soient fiables
    /// (trois fois la plus longue période, pour laisser converger les moyennes).
    pub fn warmup(&self) -> usize {
        let longest = match self {
            Rule::BuyHold => 1,
            Rule::EmaCross { slow, .. } => *slow,
            Rule::MacdCross { slow, signal, .. } => slow + signal,
            Rule::RsiReversion { period, .. } => *period,
            Rule::StochRsi { .. } => 34,
            Rule::BollingerReversion { period, .. } => *period,
            Rule::DonchianBreakout { entry, exit } => *entry.max(exit),
            Rule::Supertrend { period, .. } => *period,
            Rule::Ichimoku { kijun, senkou, .. } => kijun + senkou,
            Rule::AdxTrend { period, ema, .. } => (2 * period).max(*ema),
            Rule::KeltnerBreakout { ema, atr, .. } => *ema.max(atr),
            Rule::WilliamsR { period, .. } => *period,
            Rule::CciReversion { period, .. } => *period,
            Rule::VwapReversion { period, .. } => *period,
            Rule::DipBuy { lookback, .. } => *lookback,
            Rule::FearGreed { .. } => 1,
            Rule::Confluence { .. } => 200,
        };
        (3 * longest).max(30)
    }
}

/// Calcule les signaux à la clôture de chaque bougie.
pub fn compute_signals(rule: &Rule, c: &[Candle], ext: &External, trend_sma: Option<usize>) -> Vec<Signal> {
    let n = c.len();
    let close = ind::closes(c);
    let mut out = vec![Signal::default(); n];
    match rule {
        Rule::BuyHold => {
            for s in out.iter_mut() {
                s.enter = true;
            }
        }
        Rule::EmaCross { fast, slow } => {
            let f = ind::ema(&close, *fast);
            let s = ind::ema(&close, *slow);
            for i in 0..n {
                out[i] = Signal { enter: ind::crossed_above(&f, &s, i), exit: ind::crossed_below(&f, &s, i) };
            }
        }
        Rule::MacdCross { fast, slow, signal } => {
            let m = ind::macd(&close, *fast, *slow, *signal);
            for i in 0..n {
                out[i] = Signal {
                    enter: ind::crossed_above(&m.macd, &m.signal, i),
                    exit: ind::crossed_below(&m.macd, &m.signal, i),
                };
            }
        }
        Rule::RsiReversion { period, oversold, exit_level } => {
            let r = ind::rsi(&close, *period);
            for i in 0..n {
                out[i] = Signal { enter: ind::crossed_up_level(&r, *oversold, i), exit: r[i] >= *exit_level };
            }
        }
        Rule::StochRsi { oversold, overbought } => {
            let s = ind::stoch_rsi(&close, 14, 14, 3, 3);
            for i in 0..n {
                out[i] = Signal {
                    enter: ind::crossed_above(&s.k, &s.d, i) && s.d[i] < *oversold,
                    exit: ind::crossed_below(&s.k, &s.d, i) && s.d[i] > *overbought,
                };
            }
        }
        Rule::BollingerReversion { period, k } => {
            let b = ind::bollinger(&close, *period, *k);
            for i in 1..n {
                let back_inside = close[i - 1] < b.lower[i - 1] && close[i] > b.lower[i];
                out[i] = Signal { enter: back_inside, exit: close[i] >= b.mid[i] };
            }
        }
        Rule::DonchianBreakout { entry, exit } => {
            let hh = ind::prior_highest(c, *entry);
            let ll = ind::prior_lowest(c, *exit);
            for i in 0..n {
                out[i] = Signal { enter: close[i] > hh[i], exit: close[i] < ll[i] };
            }
        }
        Rule::Supertrend { period, mult } => {
            let st = ind::supertrend(c, *period, *mult);
            for i in 1..n {
                out[i] = Signal {
                    enter: st.direction[i] == 1 && st.direction[i - 1] == -1,
                    exit: st.direction[i] == -1 && st.direction[i - 1] == 1,
                };
            }
        }
        Rule::Ichimoku { tenkan, kijun, senkou } => {
            let ic = ind::ichimoku(c, *tenkan, *kijun, *senkou);
            for i in 0..n {
                let top = ic.span_a[i].max(ic.span_b[i]);
                let bottom = ic.span_a[i].min(ic.span_b[i]);
                let cloud_ok = !top.is_nan();
                out[i] = Signal {
                    enter: cloud_ok && ind::crossed_above(&ic.tenkan, &ic.kijun, i) && close[i] > top,
                    exit: ind::crossed_below(&ic.tenkan, &ic.kijun, i) || (cloud_ok && close[i] < bottom),
                };
            }
        }
        Rule::AdxTrend { period, threshold, ema } => {
            let a = ind::adx(c, *period);
            let e = ind::ema(&close, *ema);
            let cond = |i: usize| a.adx[i] > *threshold && a.plus_di[i] > a.minus_di[i] && close[i] > e[i];
            for i in 1..n {
                out[i] = Signal { enter: cond(i) && !cond(i - 1), exit: a.plus_di[i] < a.minus_di[i] };
            }
        }
        Rule::KeltnerBreakout { ema, atr, mult } => {
            let k = ind::keltner(c, *ema, *atr, *mult);
            for i in 0..n {
                out[i] = Signal { enter: ind::crossed_above(&close, &k.upper, i), exit: close[i] < k.mid[i] };
            }
        }
        Rule::WilliamsR { period, oversold, exit_level } => {
            let w = ind::williams_r(c, *period);
            for i in 0..n {
                out[i] = Signal { enter: ind::crossed_up_level(&w, *oversold, i), exit: w[i] >= *exit_level };
            }
        }
        Rule::CciReversion { period, entry, exit_level } => {
            let x = ind::cci(c, *period);
            for i in 0..n {
                out[i] = Signal { enter: ind::crossed_up_level(&x, *entry, i), exit: x[i] >= *exit_level };
            }
        }
        Rule::VwapReversion { period, deviation_pct } => {
            let v = ind::rolling_vwap(c, *period);
            let below = |i: usize| !v[i].is_nan() && close[i] < v[i] * (1.0 - deviation_pct / 100.0);
            for i in 1..n {
                out[i] = Signal { enter: below(i) && !below(i - 1), exit: !v[i].is_nan() && close[i] >= v[i] };
            }
        }
        Rule::DipBuy { lookback, dip_pct } => {
            let dipped = |i: usize| {
                if i < *lookback {
                    return false;
                }
                let peak = close[i - lookback..i].iter().cloned().fold(f64::MIN, f64::max);
                close[i] <= peak * (1.0 - dip_pct / 100.0)
            };
            for i in 1..n {
                out[i] = Signal { enter: dipped(i) && !dipped(i - 1), exit: false };
            }
        }
        Rule::FearGreed { buy_below, sell_above } => {
            let fg: Vec<f64> = c.iter().map(|x| ext.fear_greed_at(x.close_time).unwrap_or(f64::NAN)).collect();
            for i in 0..n {
                out[i] = Signal {
                    enter: ind::crossed_down_level(&fg, *buy_below, i),
                    exit: !fg[i].is_nan() && fg[i] > *sell_above,
                };
            }
        }
        Rule::Confluence { min_score, exit_score } => {
            let e50 = ind::ema(&close, 50);
            let e200 = ind::ema(&close, 200);
            let m = ind::macd(&close, 12, 26, 9);
            let r = ind::rsi(&close, 14);
            let st = ind::supertrend(c, 10, 3.0);
            let a = ind::adx(c, 14);
            let score = |i: usize| -> u8 {
                [
                    e50[i] > e200[i],
                    m.hist[i] > 0.0,
                    r[i] >= 45.0 && r[i] <= 70.0,
                    st.direction[i] == 1,
                    a.adx[i] > 20.0 && a.plus_di[i] > a.minus_di[i],
                ]
                .iter()
                .filter(|b| **b)
                .count() as u8
            };
            for i in 1..n {
                let (now, prev) = (score(i), score(i - 1));
                out[i] = Signal { enter: now >= *min_score && prev < *min_score, exit: now <= *exit_score };
            }
        }
    }
    if let Some(p) = trend_sma {
        let s = ind::sma(&close, p);
        for i in 0..n {
            // Faux aussi quand la SMA est encore indéfinie (NaN) : pas d'entrée avant la fin du préchauffage.
            let above_trend = close[i] > s[i];
            if !above_trend {
                out[i].enter = false;
            }
        }
    }
    out
}

/// Distance d'un stop : en pourcentage du prix, ou en multiples d'ATR(14).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Distance {
    Percent(f64),
    Atr(f64),
}

impl Distance {
    pub fn below(&self, price: f64, atr: f64) -> Option<f64> {
        let p = match self {
            Distance::Percent(x) => price * (1.0 - x / 100.0),
            Distance::Atr(m) => {
                if atr.is_nan() {
                    return None;
                }
                price - m * atr
            }
        };
        (p > 0.0).then_some(p)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum TakeProfit {
    Percent(f64),
    /// Multiple du risque initial (distance entrée → stop).
    RMultiple(f64),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Trailing {
    /// Gain (en %, mesuré sur le plus haut) à partir duquel le stop suiveur s'active.
    pub activation_pct: f64,
    pub distance: Distance,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ExitPolicy {
    pub stop: Option<Distance>,
    pub take_profit: Option<TakeProfit>,
    pub trailing: Option<Trailing>,
    pub max_bars: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Sizing {
    /// Risque une fraction fixe du capital entre l'entrée et le stop.
    Risk { risk_pct: f64, max_position_pct: f64 },
    /// Part fixe du capital par position (ou par couche).
    Fixed { position_pct: f64 },
    /// Capital réparti également entre les symboles (référence).
    EqualWeight,
}

/// Renforcement BORNÉ : une couche de plus à chaque baisse de `step_pct` depuis
/// le dernier achat, au plus `max_layers` couches. (L'ancien « martingale »
/// n'avait ni stop ni limite.)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Pyramid {
    pub step_pct: f64,
    pub max_layers: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub family: String,
    pub description: String,
    pub timeframe: Timeframe,
    pub rule: Rule,
    pub exits: ExitPolicy,
    pub sizing: Sizing,
    pub pyramid: Option<Pyramid>,
    /// N'entre que si la clôture est au-dessus de la SMA de cette période.
    pub trend_sma: Option<usize>,
    pub max_positions: Option<usize>,
}

impl Preset {
    pub fn warmup(&self) -> usize {
        self.rule.warmup().max(self.trend_sma.unwrap_or(0) * 3).max(45)
    }

    pub fn needs_fear_greed(&self) -> bool {
        matches!(self.rule, Rule::FearGreed { .. })
    }

    pub fn is_benchmark(&self) -> bool {
        matches!(self.rule, Rule::BuyHold)
    }

    /// Contrôles de cohérence : une cible de gain doit couvrir largement les frais.
    pub fn validate(&self, round_trip: f64) -> Result<(), String> {
        if let Some(TakeProfit::Percent(p)) = self.exits.take_profit {
            if p / 100.0 < 3.0 * round_trip {
                return Err(format!("{} : objectif {p}% < 3× le coût d'un aller-retour", self.id));
            }
        }
        if let Some(Distance::Percent(p)) = self.exits.stop {
            if p / 100.0 < 2.0 * round_trip {
                return Err(format!("{} : stop {p}% trop serré face aux frais", self.id));
            }
        }
        if let Some(py) = self.pyramid {
            if py.max_layers == 0 || py.max_layers > 5 {
                return Err(format!("{} : renforcement non borné", self.id));
            }
            if self.exits.stop.is_none() {
                return Err(format!("{} : renforcement sans stop interdit", self.id));
            }
            let Sizing::Fixed { position_pct } = self.sizing else {
                return Err(format!("{} : le renforcement exige une taille fixe par couche", self.id));
            };
            if position_pct * py.max_layers as f64 > 50.0 {
                return Err(format!("{} : exposition maximale par symbole > 50 %", self.id));
            }
        }
        if !self.is_benchmark() && self.exits.stop.is_none() {
            return Err(format!("{} : stratégie active sans stop", self.id));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::catalog;
    use crate::testutil::synthetic;

    /// Aucune stratégie ne doit changer d'avis sur le passé quand de nouvelles
    /// bougies arrivent : c'est la garantie « pas de regard vers le futur ».
    #[test]
    fn signals_never_use_future_candles() {
        let c = synthetic(900, 5);
        let ext = External {
            fear_greed: c.iter().enumerate().map(|(i, x)| (x.open_time, ((i * 37) % 100) as f64)).collect(),
        };
        for p in catalog() {
            let full = compute_signals(&p.rule, &c, &ext, p.trend_sma);
            for k in [400usize, 650, 899] {
                let part = compute_signals(&p.rule, &c[..=k], &ext, p.trend_sma);
                assert_eq!(full[k], part[k], "{} regarde le futur à l'indice {k}", p.id);
            }
        }
    }

    #[test]
    fn every_active_strategy_trades_on_noise() {
        let c = synthetic(3000, 9);
        let ext =
            External { fear_greed: c.iter().enumerate().map(|(i, x)| (x.open_time, ((i * 7) % 100) as f64)).collect() };
        for p in catalog() {
            let s = compute_signals(&p.rule, &c, &ext, p.trend_sma);
            assert!(s.iter().any(|x| x.enter), "{} n'émet jamais de signal d'entrée", p.id);
        }
    }

    #[test]
    fn fear_greed_lookup_respects_publication_delay() {
        let ext = External { fear_greed: vec![(0, 10.0), (86_400_000, 90.0)] };
        assert_eq!(ext.fear_greed_at(86_400_000), Some(10.0));
        assert_eq!(ext.fear_greed_at(86_400_000 + 3_600_000), Some(90.0));
        assert_eq!(ext.fear_greed_at(-1), None);
    }
}
