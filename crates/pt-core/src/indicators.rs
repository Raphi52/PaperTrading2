//! Indicateurs techniques, tous CAUSAUX : la valeur à l'indice `i` ne dépend que
//! des données `0..=i`. Les indices sans valeur (préchauffage) valent `f64::NAN`.
//!
//! Un test vérifie la causalité de chaque indicateur : le calculer sur une série
//! tronquée doit donner exactement la même valeur au dernier indice.

// Les séries parallèles (prix, indicateurs, signaux) se lisent plus clairement par indice.
#![allow(clippy::needless_range_loop)]

use crate::candle::Candle;

pub fn closes(c: &[Candle]) -> Vec<f64> {
    c.iter().map(|x| x.close).collect()
}

pub fn sma(v: &[f64], n: usize) -> Vec<f64> {
    let mut out = vec![f64::NAN; v.len()];
    if n == 0 {
        return out;
    }
    let mut sum = 0.0;
    let mut valid = 0usize;
    for i in 0..v.len() {
        if v[i].is_nan() {
            sum = 0.0;
            valid = 0;
            continue;
        }
        sum += v[i];
        valid += 1;
        if valid > n {
            sum -= v[i - n];
            valid = n;
        }
        if valid == n {
            out[i] = sum / n as f64;
        }
    }
    out
}

/// Moyenne exponentielle amorcée par une SMA. Ignore les NaN de tête.
fn smoothed(v: &[f64], n: usize, alpha: f64) -> Vec<f64> {
    let mut out = vec![f64::NAN; v.len()];
    if n == 0 {
        return out;
    }
    let Some(start) = v.iter().position(|x| !x.is_nan()) else {
        return out;
    };
    if v.len() < start + n {
        return out;
    }
    let seed: f64 = v[start..start + n].iter().sum::<f64>() / n as f64;
    if seed.is_nan() {
        return out;
    }
    let mut prev = seed;
    out[start + n - 1] = seed;
    for i in start + n..v.len() {
        prev = if v[i].is_nan() { prev } else { alpha * v[i] + (1.0 - alpha) * prev };
        out[i] = prev;
    }
    out
}

pub fn ema(v: &[f64], n: usize) -> Vec<f64> {
    smoothed(v, n, 2.0 / (n as f64 + 1.0))
}

/// Lissage de Wilder (RMA), utilisé par RSI, ATR et ADX.
pub fn rma(v: &[f64], n: usize) -> Vec<f64> {
    smoothed(v, n, 1.0 / n as f64)
}

pub fn rsi(close: &[f64], n: usize) -> Vec<f64> {
    let len = close.len();
    let mut gains = vec![f64::NAN; len];
    let mut losses = vec![f64::NAN; len];
    for i in 1..len {
        let d = close[i] - close[i - 1];
        gains[i] = d.max(0.0);
        losses[i] = (-d).max(0.0);
    }
    let ag = rma(&gains, n);
    let al = rma(&losses, n);
    (0..len)
        .map(|i| {
            if ag[i].is_nan() || al[i].is_nan() {
                f64::NAN
            } else if al[i] == 0.0 {
                if ag[i] == 0.0 {
                    50.0
                } else {
                    100.0
                }
            } else {
                100.0 - 100.0 / (1.0 + ag[i] / al[i])
            }
        })
        .collect()
}

pub struct Macd {
    pub macd: Vec<f64>,
    pub signal: Vec<f64>,
    pub hist: Vec<f64>,
}

pub fn macd(close: &[f64], fast: usize, slow: usize, signal: usize) -> Macd {
    let f = ema(close, fast);
    let s = ema(close, slow);
    let m: Vec<f64> = f.iter().zip(&s).map(|(a, b)| a - b).collect();
    let sig = ema(&m, signal);
    let hist = m.iter().zip(&sig).map(|(a, b)| a - b).collect();
    Macd { macd: m, signal: sig, hist }
}

/// Écart-type (population) glissant.
pub fn stddev(v: &[f64], n: usize) -> Vec<f64> {
    let mean = sma(v, n);
    (0..v.len())
        .map(|i| {
            if mean[i].is_nan() {
                return f64::NAN;
            }
            let m = mean[i];
            let var = v[i + 1 - n..=i].iter().map(|x| (x - m).powi(2)).sum::<f64>() / n as f64;
            var.sqrt()
        })
        .collect()
}

pub struct Bands {
    pub mid: Vec<f64>,
    pub upper: Vec<f64>,
    pub lower: Vec<f64>,
}

pub fn bollinger(close: &[f64], n: usize, k: f64) -> Bands {
    let mid = sma(close, n);
    let sd = stddev(close, n);
    let upper = mid.iter().zip(&sd).map(|(m, s)| m + k * s).collect();
    let lower = mid.iter().zip(&sd).map(|(m, s)| m - k * s).collect();
    Bands { mid, upper, lower }
}

pub fn true_range(c: &[Candle]) -> Vec<f64> {
    (0..c.len())
        .map(|i| {
            if i == 0 {
                c[0].high - c[0].low
            } else {
                let pc = c[i - 1].close;
                (c[i].high - c[i].low).max((c[i].high - pc).abs()).max((c[i].low - pc).abs())
            }
        })
        .collect()
}

pub fn atr(c: &[Candle], n: usize) -> Vec<f64> {
    rma(&true_range(c), n)
}

/// Plus haut des `n` bougies PRÉCÉDENTES (la bougie courante est exclue).
pub fn prior_highest(c: &[Candle], n: usize) -> Vec<f64> {
    (0..c.len())
        .map(|i| if i < n { f64::NAN } else { c[i - n..i].iter().map(|x| x.high).fold(f64::MIN, f64::max) })
        .collect()
}

/// Plus bas des `n` bougies PRÉCÉDENTES (la bougie courante est exclue).
pub fn prior_lowest(c: &[Candle], n: usize) -> Vec<f64> {
    (0..c.len())
        .map(|i| if i < n { f64::NAN } else { c[i - n..i].iter().map(|x| x.low).fold(f64::MAX, f64::min) })
        .collect()
}

/// Plus haut / plus bas des `n` dernières bougies, bougie courante INCLUSE.
fn window_mid(c: &[Candle], n: usize) -> Vec<f64> {
    (0..c.len())
        .map(|i| {
            if i + 1 < n {
                return f64::NAN;
            }
            let w = &c[i + 1 - n..=i];
            let hh = w.iter().map(|x| x.high).fold(f64::MIN, f64::max);
            let ll = w.iter().map(|x| x.low).fold(f64::MAX, f64::min);
            (hh + ll) / 2.0
        })
        .collect()
}

pub struct StochRsi {
    pub k: Vec<f64>,
    pub d: Vec<f64>,
}

/// Stochastique appliqué au RSI, en 0..100.
pub fn stoch_rsi(close: &[f64], rsi_n: usize, stoch_n: usize, k_smooth: usize, d_smooth: usize) -> StochRsi {
    let r = rsi(close, rsi_n);
    let raw: Vec<f64> = (0..r.len())
        .map(|i| {
            if i + 1 < stoch_n || r[i + 1 - stoch_n..=i].iter().any(|x| x.is_nan()) {
                return f64::NAN;
            }
            let w = &r[i + 1 - stoch_n..=i];
            let hi = w.iter().cloned().fold(f64::MIN, f64::max);
            let lo = w.iter().cloned().fold(f64::MAX, f64::min);
            if hi - lo < 1e-12 {
                50.0
            } else {
                100.0 * (r[i] - lo) / (hi - lo)
            }
        })
        .collect();
    let k = sma(&raw, k_smooth);
    let d = sma(&k, d_smooth);
    StochRsi { k, d }
}

/// Williams %R, en -100..0.
pub fn williams_r(c: &[Candle], n: usize) -> Vec<f64> {
    (0..c.len())
        .map(|i| {
            if i + 1 < n {
                return f64::NAN;
            }
            let w = &c[i + 1 - n..=i];
            let hh = w.iter().map(|x| x.high).fold(f64::MIN, f64::max);
            let ll = w.iter().map(|x| x.low).fold(f64::MAX, f64::min);
            if hh - ll < 1e-12 {
                -50.0
            } else {
                -100.0 * (hh - c[i].close) / (hh - ll)
            }
        })
        .collect()
}

/// Commodity Channel Index.
pub fn cci(c: &[Candle], n: usize) -> Vec<f64> {
    let tp: Vec<f64> = c.iter().map(|x| x.typical_price()).collect();
    let m = sma(&tp, n);
    (0..c.len())
        .map(|i| {
            if m[i].is_nan() {
                return f64::NAN;
            }
            let md = tp[i + 1 - n..=i].iter().map(|x| (x - m[i]).abs()).sum::<f64>() / n as f64;
            if md < 1e-12 {
                0.0
            } else {
                (tp[i] - m[i]) / (0.015 * md)
            }
        })
        .collect()
}

pub struct Adx {
    pub adx: Vec<f64>,
    pub plus_di: Vec<f64>,
    pub minus_di: Vec<f64>,
}

pub fn adx(c: &[Candle], n: usize) -> Adx {
    let len = c.len();
    let mut pdm = vec![f64::NAN; len];
    let mut mdm = vec![f64::NAN; len];
    let mut tr = vec![f64::NAN; len];
    let all_tr = true_range(c);
    for i in 1..len {
        let up = c[i].high - c[i - 1].high;
        let down = c[i - 1].low - c[i].low;
        pdm[i] = if up > down && up > 0.0 { up } else { 0.0 };
        mdm[i] = if down > up && down > 0.0 { down } else { 0.0 };
        tr[i] = all_tr[i];
    }
    let str_ = rma(&tr, n);
    let spdm = rma(&pdm, n);
    let smdm = rma(&mdm, n);
    let mut plus_di = vec![f64::NAN; len];
    let mut minus_di = vec![f64::NAN; len];
    let mut dx = vec![f64::NAN; len];
    for i in 0..len {
        if str_[i].is_nan() || str_[i] <= 0.0 {
            continue;
        }
        plus_di[i] = 100.0 * spdm[i] / str_[i];
        minus_di[i] = 100.0 * smdm[i] / str_[i];
        let s = plus_di[i] + minus_di[i];
        dx[i] = if s <= 0.0 { 0.0 } else { 100.0 * (plus_di[i] - minus_di[i]).abs() / s };
    }
    Adx { adx: rma(&dx, n), plus_di, minus_di }
}

pub struct Supertrend {
    pub line: Vec<f64>,
    /// +1 tendance haussière, -1 baissière, 0 indéterminé.
    pub direction: Vec<i8>,
}

pub fn supertrend(c: &[Candle], n: usize, mult: f64) -> Supertrend {
    let len = c.len();
    let a = atr(c, n);
    let mut line = vec![f64::NAN; len];
    let mut direction = vec![0i8; len];
    let mut fu = f64::NAN;
    let mut fl = f64::NAN;
    for i in 0..len {
        if a[i].is_nan() {
            continue;
        }
        let hl2 = (c[i].high + c[i].low) / 2.0;
        let bu = hl2 + mult * a[i];
        let bl = hl2 - mult * a[i];
        let prev_close = if i > 0 { c[i - 1].close } else { c[i].close };
        let prev_dir = if i > 0 { direction[i - 1] } else { 0 };
        fu = if fu.is_nan() || bu < fu || prev_close > fu { bu } else { fu };
        fl = if fl.is_nan() || bl > fl || prev_close < fl { bl } else { fl };
        let d = match prev_dir {
            0 => {
                if c[i].close >= hl2 {
                    1
                } else {
                    -1
                }
            }
            1 => {
                if c[i].close < fl {
                    -1
                } else {
                    1
                }
            }
            _ => {
                if c[i].close > fu {
                    1
                } else {
                    -1
                }
            }
        };
        direction[i] = d;
        line[i] = if d == 1 { fl } else { fu };
    }
    Supertrend { line, direction }
}

pub struct Ichimoku {
    pub tenkan: Vec<f64>,
    pub kijun: Vec<f64>,
    /// Nuage AFFICHÉ à l'indice i, donc calculé à i - décalage : aucune donnée future.
    pub span_a: Vec<f64>,
    pub span_b: Vec<f64>,
}

pub fn ichimoku(c: &[Candle], tenkan_n: usize, kijun_n: usize, senkou_n: usize) -> Ichimoku {
    let tenkan = window_mid(c, tenkan_n);
    let kijun = window_mid(c, kijun_n);
    let b_raw = window_mid(c, senkou_n);
    let shift = kijun_n;
    let len = c.len();
    let mut span_a = vec![f64::NAN; len];
    let mut span_b = vec![f64::NAN; len];
    for i in shift..len {
        span_a[i] = (tenkan[i - shift] + kijun[i - shift]) / 2.0;
        span_b[i] = b_raw[i - shift];
    }
    Ichimoku { tenkan, kijun, span_a, span_b }
}

pub fn keltner(c: &[Candle], ema_n: usize, atr_n: usize, mult: f64) -> Bands {
    let mid = ema(&closes(c), ema_n);
    let a = atr(c, atr_n);
    let upper = mid.iter().zip(&a).map(|(m, x)| m + mult * x).collect();
    let lower = mid.iter().zip(&a).map(|(m, x)| m - mult * x).collect();
    Bands { mid, upper, lower }
}

/// VWAP glissant sur `n` bougies (prix typique pondéré par le volume).
pub fn rolling_vwap(c: &[Candle], n: usize) -> Vec<f64> {
    (0..c.len())
        .map(|i| {
            if i + 1 < n {
                return f64::NAN;
            }
            let w = &c[i + 1 - n..=i];
            let vol: f64 = w.iter().map(|x| x.volume).sum();
            if vol <= 0.0 {
                return f64::NAN;
            }
            w.iter().map(|x| x.typical_price() * x.volume).sum::<f64>() / vol
        })
        .collect()
}

/// Vrai si `a` croise `b` par le haut à l'indice i (a[i-1] <= b[i-1] et a[i] > b[i]).
pub fn crossed_above(a: &[f64], b: &[f64], i: usize) -> bool {
    i > 0 && [a[i - 1], b[i - 1], a[i], b[i]].iter().all(|x| !x.is_nan()) && a[i - 1] <= b[i - 1] && a[i] > b[i]
}

pub fn crossed_below(a: &[f64], b: &[f64], i: usize) -> bool {
    i > 0 && [a[i - 1], b[i - 1], a[i], b[i]].iter().all(|x| !x.is_nan()) && a[i - 1] >= b[i - 1] && a[i] < b[i]
}

/// Vrai si la série franchit le niveau constant vers le haut à l'indice i.
pub fn crossed_up_level(a: &[f64], level: f64, i: usize) -> bool {
    i > 0 && !a[i - 1].is_nan() && !a[i].is_nan() && a[i - 1] <= level && a[i] > level
}

pub fn crossed_down_level(a: &[f64], level: f64, i: usize) -> bool {
    i > 0 && !a[i - 1].is_nan() && !a[i].is_nan() && a[i - 1] >= level && a[i] < level
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::synthetic;

    fn approx(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn sma_exact() {
        let v = [1.0, 2.0, 3.0, 4.0, 5.0];
        let s = sma(&v, 3);
        assert!(s[0].is_nan() && s[1].is_nan());
        assert_eq!(&s[2..], &[2.0, 3.0, 4.0]);
    }

    #[test]
    fn ema_matches_naive_recursion() {
        let v: Vec<f64> = (0..50).map(|i| 100.0 + (i as f64 * 0.7).sin() * 5.0).collect();
        let e = ema(&v, 10);
        let alpha = 2.0 / 11.0;
        let mut prev = v[..10].iter().sum::<f64>() / 10.0;
        assert!(approx(e[9], prev, 1e-12));
        for i in 10..v.len() {
            prev = alpha * v[i] + (1.0 - alpha) * prev;
            assert!(approx(e[i], prev, 1e-9));
        }
    }

    #[test]
    fn rsi_extremes_and_reference() {
        let up: Vec<f64> = (0..40).map(|i| 100.0 + i as f64).collect();
        assert!(approx(*rsi(&up, 14).last().unwrap(), 100.0, 1e-9));
        let down: Vec<f64> = (0..40).map(|i| 100.0 - i as f64).collect();
        assert!(approx(*rsi(&down, 14).last().unwrap(), 0.0, 1e-9));
        // Exemple de référence de Wilder (StockCharts) : premier RSI(14) ≈ 70.5.
        let ref_closes =
            [44.34, 44.09, 44.15, 43.61, 44.33, 44.83, 45.10, 45.42, 45.84, 46.08, 45.89, 46.03, 45.61, 46.28, 46.28];
        let r = rsi(&ref_closes, 14);
        assert!(r[13].is_nan());
        assert!(approx(r[14], 70.5, 0.5), "RSI de référence = {}", r[14]);
    }

    #[test]
    fn bollinger_symmetry() {
        let c = synthetic(200, 1);
        let b = bollinger(&closes(&c), 20, 2.0);
        for i in 19..c.len() {
            assert!(approx(b.upper[i] - b.mid[i], b.mid[i] - b.lower[i], 1e-9));
            assert!(b.upper[i] >= b.lower[i]);
        }
    }

    #[test]
    fn oscillators_stay_in_range() {
        let c = synthetic(600, 7);
        let cl = closes(&c);
        let s = stoch_rsi(&cl, 14, 14, 3, 3);
        let w = williams_r(&c, 14);
        let r = rsi(&cl, 14);
        let a = adx(&c, 14);
        for i in 0..c.len() {
            for (x, lo, hi) in [(s.k[i], 0.0, 100.0), (w[i], -100.0, 0.0), (r[i], 0.0, 100.0), (a.adx[i], 0.0, 100.0)] {
                if !x.is_nan() {
                    assert!(x >= lo - 1e-9 && x <= hi + 1e-9, "valeur {x} hors [{lo},{hi}] à {i}");
                }
            }
        }
    }

    #[test]
    fn supertrend_flags_trends() {
        let mut up = synthetic(10, 3);
        for i in 0..120 {
            let p = 100.0 + i as f64;
            up.push(crate::testutil::candle(i + 10, p, p + 0.5, p - 0.5, p + 0.4));
        }
        let st = supertrend(&up, 10, 3.0);
        assert_eq!(*st.direction.last().unwrap(), 1);
    }

    /// Causalité : chaque indicateur calculé sur une série tronquée doit donner
    /// la même valeur au dernier indice que sur la série complète.
    #[test]
    fn every_indicator_is_causal() {
        let c = synthetic(400, 11);
        type Ind = Box<dyn Fn(&[Candle]) -> Vec<f64>>;
        let inds: Vec<(&str, Ind)> = vec![
            ("ema", Box::new(|c: &[Candle]| ema(&closes(c), 21))),
            ("rsi", Box::new(|c: &[Candle]| rsi(&closes(c), 14))),
            ("macd", Box::new(|c: &[Candle]| macd(&closes(c), 12, 26, 9).hist)),
            ("boll", Box::new(|c: &[Candle]| bollinger(&closes(c), 20, 2.0).lower)),
            ("atr", Box::new(|c: &[Candle]| atr(c, 14))),
            ("stoch", Box::new(|c: &[Candle]| stoch_rsi(&closes(c), 14, 14, 3, 3).d)),
            ("wr", Box::new(|c: &[Candle]| williams_r(c, 14))),
            ("cci", Box::new(|c: &[Candle]| cci(c, 20))),
            ("adx", Box::new(|c: &[Candle]| adx(c, 14).adx)),
            ("st", Box::new(|c: &[Candle]| supertrend(c, 10, 3.0).line)),
            ("ichi_a", Box::new(|c: &[Candle]| ichimoku(c, 9, 26, 52).span_a)),
            ("ichi_b", Box::new(|c: &[Candle]| ichimoku(c, 9, 26, 52).span_b)),
            ("kelt", Box::new(|c: &[Candle]| keltner(c, 20, 10, 2.0).upper)),
            ("vwap", Box::new(|c: &[Candle]| rolling_vwap(c, 24))),
            ("donch", Box::new(|c: &[Candle]| prior_highest(c, 20))),
        ];
        for (name, f) in &inds {
            let full = f(&c);
            for k in [120usize, 200, 333, 399] {
                let part = f(&c[..=k]);
                let (a, b) = (full[k], part[k]);
                assert!(
                    (a.is_nan() && b.is_nan()) || approx(a, b, 1e-9 * a.abs().max(1.0)),
                    "{name} n'est pas causal à {k} : {a} vs {b}"
                );
            }
        }
    }
}
