//! Données synthétiques déterministes pour les tests (aucun réseau).

use crate::candle::Candle;

const HOUR: i64 = 3_600_000;

/// Bougie d'une heure à l'indice `i`.
pub fn candle(i: i64, open: f64, high: f64, low: f64, close: f64) -> Candle {
    Candle {
        open_time: i * HOUR,
        open,
        high: high.max(open).max(close),
        low: low.min(open).min(close),
        close,
        volume: 1_000.0 + (i % 7) as f64 * 100.0,
        close_time: (i + 1) * HOUR - 1,
    }
}

/// Marche aléatoire à régimes (tendances et ranges alternés), reproductible.
pub fn synthetic(n: usize, seed: u64) -> Vec<Candle> {
    let mut state = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    let mut rnd = move || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((state >> 33) as f64) / (1u64 << 31) as f64
    };
    let mut out = Vec::with_capacity(n);
    let mut price = 100.0;
    let mut drift = 0.0;
    for i in 0..n {
        if i % 150 == 0 {
            drift = (rnd() - 0.5) * 0.006;
        }
        let open = price;
        let ret = drift + (rnd() - 0.5) * 0.03;
        let close = (open * (1.0 + ret)).max(0.01);
        let high = open.max(close) * (1.0 + rnd() * 0.01);
        let low = open.min(close) * (1.0 - rnd() * 0.01);
        let mut c = candle(i as i64, open, high, low, close);
        c.volume = 500.0 + rnd() * 1_500.0;
        out.push(c);
        price = close;
    }
    out
}
