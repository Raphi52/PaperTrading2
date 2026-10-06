//! Validation sur fenêtres glissantes : « est-ce un coup de chance ? »
//!
//! Un seul backtest sur une seule période ne prouve rien : la stratégie a pu
//! tomber sur la bonne phase de marché, ou avoir été choisie parmi 29 autres
//! précisément parce qu'elle avait eu de la chance. Ce module :
//!
//! 1. découpe l'historique en fenêtres de même durée, décalées d'un pas fixe ;
//! 2. rejoue la stratégie dans chaque fenêtre, en partant de zéro (trésorerie
//!    pleine, aucune position) après un préchauffage sur les données ANTÉRIEURES ;
//! 3. compare chaque fenêtre à une référence À EXPOSITION ÉGALE ;
//! 4. fait un test du signe sur des fenêtres qui ne se chevauchent pas, puis
//!    corrige le résultat pour le nombre de stratégies essayées.
//!
//! ## Pourquoi une référence à exposition égale
//!
//! Comparer au Sharpe d'« acheter et garder » est biaisé : une stratégie qui
//! entre et sort AU HASARD en restant investie une fraction `f` du temps a un
//! Sharpe d'environ `√f × Sharpe(référence)`. Elle « bat » donc mécaniquement la
//! référence dans chaque fenêtre baissière.
//!
//! En revanche, un tel timing au hasard capte en moyenne la fraction `f` de la
//! croissance LOGARITHMIQUE de la référence : son rendement attendu vaut
//! `(1 + R)^f − 1`, où `R` est le rendement d'« acheter et garder ». C'est la
//! barre de chaque fenêtre. Sans talent de timing, la dépasser revient à jouer à
//! pile ou face, un peu moins même, car la stratégie paie plus de frais et ne
//! profite pas de la diversification de la référence. Le test du signe est donc
//! honnête, et plutôt prudent.
//!
//! (La version linéaire `f × R` serait fausse : trop exigeante dans les fenêtres
//! très haussières, trop indulgente dans les fenêtres très baissières.)

use crate::backtest::{compute_metrics, run};
use crate::candle::{Candle, Timeframe};
use crate::catalog;
use crate::portfolio::CostModel;
use crate::strategy::{External, Preset};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const DAY_MS: i64 = 86_400_000;

/// Écart de rendement en dessous duquel une fenêtre est déclarée « nulle ».
const TIE_EPSILON_PCT: f64 = 1e-6;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RollingConfig {
    /// Durée d'une fenêtre, en jours.
    pub window_days: i64,
    /// Décalage entre deux fenêtres successives, en jours.
    pub step_days: i64,
    /// Nombre de stratégies essayées avant de retenir celle-ci (correction de Bonferroni).
    pub tested_strategies: usize,
    pub initial_cash: f64,
}

impl RollingConfig {
    /// Refuse une configuration invalide, avant tout calcul.
    pub fn check(&self) -> Result<(), ValidationError> {
        if self.window_days < 30 || self.step_days < 1 || self.step_days > self.window_days {
            return Err(ValidationError::InvalidConfig(
                "la fenêtre doit durer au moins 30 jours, et le pas être compris entre 1 jour et la durée d'une fenêtre"
                    .into(),
            ));
        }
        if !(self.initial_cash.is_finite() && self.initial_cash > 0.0) {
            return Err(ValidationError::InvalidConfig("capital initial invalide".into()));
        }
        Ok(())
    }
}

impl Default for RollingConfig {
    fn default() -> Self {
        RollingConfig {
            window_days: 180,
            step_days: 90,
            tested_strategies: tested_strategies(),
            initial_cash: 10_000.0,
        }
    }
}

/// Nombre de stratégies à compter dans la correction : le registre des essais
/// ∪ le catalogue actuel (la référence exclue). Retirer une stratégie ou changer
/// un réglage ne peut jamais faire baisser ce nombre (voir [`crate::essais`]).
pub fn tested_strategies() -> usize {
    crate::essais::strategy_trials()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    /// La stratégie fait mieux que la référence à exposition égale.
    Win,
    Loss,
    /// Aucun écart (typiquement : aucun trade dans la fenêtre).
    Tie,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowResult {
    pub start: i64,
    pub end: i64,
    /// Symboles déjà cotés ET préchauffés au début de la fenêtre.
    pub symbols: Vec<String>,
    pub return_pct: f64,
    pub benchmark_return_pct: f64,
    /// Part moyenne du capital investie par la stratégie.
    pub exposure_pct: f64,
    /// Rendement de la référence ramené à la même exposition.
    pub matched_return_pct: f64,
    pub excess_pct: f64,
    pub sharpe: f64,
    pub benchmark_sharpe: f64,
    pub max_drawdown_pct: f64,
    pub benchmark_max_drawdown_pct: f64,
    pub trades: usize,
    pub outcome: Outcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Robustness {
    /// Bat la référence à exposition égale trop souvent pour que ce soit la chance,
    /// même en tenant compte de toutes les stratégies essayées.
    PasDeLaChance,
    /// Significatif seule, mais plus une fois comptées toutes les stratégies essayées.
    Prometteuse,
    /// Rien ne la distingue du hasard.
    CompatibleAvecLaChance,
    /// Pas assez de fenêtres indépendantes pour conclure.
    TropPeuDeFenetres,
}

impl Robustness {
    pub fn label(&self) -> &'static str {
        match self {
            Robustness::PasDeLaChance => "Pas un coup de chance",
            Robustness::Prometteuse => "Prometteuse, pas prouvée",
            Robustness::CompatibleAvecLaChance => "Compatible avec la chance",
            Robustness::TropPeuDeFenetres => "Trop peu de fenêtres",
        }
    }
}

/// Résultat du test du signe pour UNE façon de choisir les fenêtres indépendantes.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SignTest {
    /// Indice de la première fenêtre retenue (les suivantes : tous les `stride`).
    pub phase: usize,
    pub wins: usize,
    pub losses: usize,
    pub ties: usize,
    /// Probabilité d'obtenir au moins autant de victoires à pile ou face.
    pub p_value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RollingReport {
    pub preset_id: String,
    pub preset_name: String,
    pub timeframe: Timeframe,
    pub config: RollingConfig,
    pub costs: CostModel,
    pub windows: Vec<WindowResult>,
    /// Une fenêtre sur `stride` est retenue pour que les fenêtres testées ne se chevauchent pas.
    pub stride: usize,
    /// Le test retenu : celui des choix de fenêtres qui est le MOINS favorable.
    pub sign_test: SignTest,
    /// Probabilité corrigée pour le nombre de stratégies essayées (Bonferroni).
    pub p_adjusted: f64,
    pub positive_windows: usize,
    pub beats_benchmark_sharpe: usize,
    pub median_excess_pct: f64,
    pub worst_return_pct: f64,
    pub worst_benchmark_return_pct: f64,
    pub verdict: Robustness,
    pub verdict_reason: String,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ValidationError {
    #[error("paramètres invalides : {0}")]
    InvalidConfig(String),
    #[error("aucune fenêtre complète : l'historique est trop court pour des fenêtres de {0} jours après préchauffage")]
    NoWindow(i64),
}

/// `P(X ≥ k)` pour `X ~ Binomiale(n, 1/2)`, calculé exactement en espace logarithmique.
pub fn binomial_tail_half(n: usize, k: usize) -> f64 {
    if k == 0 {
        return 1.0;
    }
    if k > n {
        return 0.0;
    }
    let ln2n = n as f64 * std::f64::consts::LN_2;
    // ln C(n, i) construit pas à pas : ln C(n, i) = ln C(n, i-1) + ln(n-i+1) - ln(i)
    let mut ln_c = 0.0;
    let mut total = 0.0;
    for i in 0..=n {
        if i > 0 {
            ln_c += ((n - i + 1) as f64).ln() - (i as f64).ln();
        }
        if i >= k {
            total += (ln_c - ln2n).exp();
        }
    }
    total.min(1.0)
}

/// Test du signe : les fenêtres nulles sont écartées, comme le veut la méthode.
pub fn sign_test(outcomes: &[Outcome], phase: usize) -> SignTest {
    let wins = outcomes.iter().filter(|o| **o == Outcome::Win).count();
    let losses = outcomes.iter().filter(|o| **o == Outcome::Loss).count();
    let ties = outcomes.len() - wins - losses;
    SignTest { phase, wins, losses, ties, p_value: binomial_tail_half(wins + losses, wins) }
}

/// Fenêtres non chevauchantes pour chaque point de départ possible ; on garde le
/// test le MOINS favorable, pour ne pas choisir après coup le découpage qui arrange.
pub fn worst_sign_test(outcomes: &[Outcome], stride: usize) -> SignTest {
    let stride = stride.max(1);
    (0..stride.min(outcomes.len().max(1)))
        .map(|phase| {
            let subset: Vec<Outcome> = outcomes.iter().skip(phase).step_by(stride).copied().collect();
            sign_test(&subset, phase)
        })
        .max_by(|a, b| a.p_value.total_cmp(&b.p_value).then(b.wins.cmp(&a.wins)))
        .unwrap_or(SignTest { phase: 0, wins: 0, losses: 0, ties: 0, p_value: 1.0 })
}

/// Fenêtres indépendantes minimum pour qu'un test du signe puisse conclure à 5 %
/// (avec 5 fenêtres toutes gagnantes, la probabilité vaut 1/32 ≈ 3 %).
pub const MIN_INDEPENDENT_WINDOWS: usize = 5;
pub const SIGNIFICANCE: f64 = 0.05;

pub fn judge_rolling(test: &SignTest, tested_strategies: usize) -> (Robustness, f64, String) {
    let n = test.wins + test.losses;
    let p_adj = (test.p_value * tested_strategies.max(1) as f64).min(1.0);
    if n < MIN_INDEPENDENT_WINDOWS {
        return (
            Robustness::TropPeuDeFenetres,
            p_adj,
            format!(
                "{n} fenêtre(s) indépendante(s) seulement : il en faut au moins {MIN_INDEPENDENT_WINDOWS}. Allonge l'historique ou raccourcis les fenêtres."
            ),
        );
    }
    let base = format!(
        "Bat la référence à exposition égale dans {} fenêtre(s) indépendante(s) sur {n}. À pile ou face, on ferait au moins aussi bien avec une probabilité de {}",
        test.wins,
        fmt_p(test.p_value)
    );
    if p_adj < SIGNIFICANCE {
        (
            Robustness::PasDeLaChance,
            p_adj,
            format!(
                "{base}, et encore {} après avoir compté les {tested_strategies} stratégies essayées.",
                fmt_p(p_adj)
            ),
        )
    } else if test.p_value < SIGNIFICANCE {
        (
            Robustness::Prometteuse,
            p_adj,
            format!(
                "{base}. Mais {tested_strategies} stratégies ont été essayées : une fois ce choix compté, la probabilité monte à {}, au-dessus du seuil de 5 %.",
                fmt_p(p_adj)
            ),
        )
    } else {
        (Robustness::CompatibleAvecLaChance, p_adj, format!("{base}, au-dessus du seuil de 5 %."))
    }
}

pub(crate) fn fmt_p(p: f64) -> String {
    if p < 0.001 {
        format!("{:.2} ‰", p * 1000.0)
    } else {
        format!("{:.1} %", p * 100.0)
    }
}

pub(crate) fn median(v: &mut [f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    let m = v.len() / 2;
    if v.len().is_multiple_of(2) {
        (v[m - 1] + v[m]) / 2.0
    } else {
        v[m]
    }
}

/// Rendement attendu (en %) d'un timing au hasard investi la fraction `exposure`
/// du temps, quand la référence fait `benchmark_return_pct` : `(1 + R)^f − 1`.
pub fn matched_return_pct(benchmark_return_pct: f64, exposure: f64) -> f64 {
    let growth = (1.0 + benchmark_return_pct / 100.0).max(0.0);
    (growth.powf(exposure.clamp(0.0, 1.0)) - 1.0) * 100.0
}

/// Bougies d'un symbole antérieures à `end` (exclu) : la fenêtre ne voit RIEN après sa fin.
fn before(c: &[Candle], end: i64) -> &[Candle] {
    &c[..c.partition_point(|x| x.open_time < end)]
}

/// Rejoue `preset` sur une fenêtre `[start, end)`. Rend `None` si aucun symbole n'est prêt.
pub fn run_window(
    preset: &Preset,
    series: &BTreeMap<String, Vec<Candle>>,
    ext: &External,
    costs: CostModel,
    initial_cash: f64,
    start: i64,
    end: i64,
) -> Option<WindowResult> {
    let warmup = preset.warmup();
    // Un symbole entre dans la fenêtre s'il a déjà assez d'historique au départ.
    // Chaque fenêtre ne garde que `warmup` bougies avant son début : c'est le même
    // préchauffage que le backtest principal, et cela évite de rejouer tout
    // l'historique à chaque fenêtre (80 000 bougies par symbole en 1h sur 10 ans).
    let sliced: BTreeMap<String, Vec<Candle>> = series
        .iter()
        .filter(|(_, c)| c.len() > warmup && c[warmup].open_time <= start)
        .map(|(s, c)| {
            let first = c.partition_point(|x| x.open_time < start).saturating_sub(warmup);
            (s.clone(), before(c, end)[first..].to_vec())
        })
        .filter(|(_, c)| c.last().is_some_and(|x| x.open_time >= start))
        .collect();
    if sliced.is_empty() {
        return None;
    }
    let strat = run(preset, &sliced, ext, costs, initial_cash, start);
    let bh_preset = catalog::find("buy_hold").expect("référence au catalogue");
    let bench = run(&bh_preset, &sliced, ext, costs, initial_cash, start);
    if strat.curve.len() < 2 || bench.curve.len() < 2 {
        return None;
    }
    let tf = preset.timeframe;
    let m = compute_metrics(&strat.curve, &strat.engine.portfolio.closed, tf);
    let b = compute_metrics(&bench.curve, &[], tf);
    let exposure = m.exposure_pct / 100.0;
    let matched = matched_return_pct(b.total_return_pct, exposure);
    let excess = m.total_return_pct - matched;
    let outcome = if excess.abs() <= TIE_EPSILON_PCT {
        Outcome::Tie
    } else if excess > 0.0 {
        Outcome::Win
    } else {
        Outcome::Loss
    };
    Some(WindowResult {
        start,
        end,
        symbols: sliced.keys().cloned().collect(),
        return_pct: m.total_return_pct,
        benchmark_return_pct: b.total_return_pct,
        exposure_pct: exposure * 100.0,
        matched_return_pct: matched,
        excess_pct: excess,
        sharpe: m.sharpe,
        benchmark_sharpe: b.sharpe,
        max_drawdown_pct: m.max_drawdown_pct,
        benchmark_max_drawdown_pct: b.max_drawdown_pct,
        trades: m.trades,
        outcome,
    })
}

/// Validation complète : toutes les fenêtres, test du signe, correction, verdict.
pub fn rolling_validation(
    preset: &Preset,
    series: &BTreeMap<String, Vec<Candle>>,
    ext: &External,
    costs: CostModel,
    config: RollingConfig,
) -> Result<RollingReport, ValidationError> {
    config.check()?;
    let warmup = preset.warmup();
    // Première fenêtre : dès qu'un symbole a fini son préchauffage.
    let first = series.values().filter(|c| c.len() > warmup).map(|c| c[warmup].open_time).min();
    let last = series.values().filter_map(|c| c.last()).map(|c| c.close_time + 1).max();
    let (Some(first), Some(last)) = (first, last) else {
        return Err(ValidationError::NoWindow(config.window_days));
    };
    let len = config.window_days * DAY_MS;
    let step = config.step_days * DAY_MS;
    let mut windows = Vec::new();
    let mut start = first;
    while start + len <= last {
        if let Some(w) = run_window(preset, series, ext, costs, config.initial_cash, start, start + len) {
            windows.push(w);
        }
        start += step;
    }
    if windows.is_empty() {
        return Err(ValidationError::NoWindow(config.window_days));
    }
    let stride = ((config.window_days + config.step_days - 1) / config.step_days) as usize;
    let outcomes: Vec<Outcome> = windows.iter().map(|w| w.outcome).collect();
    let test = worst_sign_test(&outcomes, stride);
    let (verdict, p_adjusted, verdict_reason) = judge_rolling(&test, config.tested_strategies);
    let mut excess: Vec<f64> = windows.iter().map(|w| w.excess_pct).collect();
    Ok(RollingReport {
        preset_id: preset.id.clone(),
        preset_name: preset.name.clone(),
        timeframe: preset.timeframe,
        config,
        costs,
        stride,
        sign_test: test,
        p_adjusted,
        positive_windows: windows.iter().filter(|w| w.return_pct > 0.0).count(),
        beats_benchmark_sharpe: windows.iter().filter(|w| w.sharpe > w.benchmark_sharpe).count(),
        median_excess_pct: median(&mut excess),
        worst_return_pct: windows.iter().map(|w| w.return_pct).fold(f64::INFINITY, f64::min),
        worst_benchmark_return_pct: windows.iter().map(|w| w.benchmark_return_pct).fold(f64::INFINITY, f64::min),
        verdict,
        verdict_reason,
        windows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::synthetic;

    #[test]
    fn binomial_tail_exact_values() {
        assert!((binomial_tail_half(10, 10) - 1.0 / 1024.0).abs() < 1e-15);
        assert!((binomial_tail_half(10, 8) - 56.0 / 1024.0).abs() < 1e-12);
        assert!((binomial_tail_half(10, 0) - 1.0).abs() < 1e-15);
        assert!((binomial_tail_half(17, 13) - 3214.0 / 131_072.0).abs() < 1e-12);
        assert_eq!(binomial_tail_half(5, 6), 0.0);
        // Grand n : pas de débordement, symétrie autour de n/2.
        let p = binomial_tail_half(400, 200);
        assert!(p > 0.5 && p < 0.53, "{p}");
    }

    #[test]
    fn matched_return_is_geometric() {
        // Investi 50 % du temps quand la référence double : √2 − 1 ≈ +41,4 %, pas +50 %.
        assert!((matched_return_pct(100.0, 0.5) - 41.421356).abs() < 1e-5);
        // Quand la référence perd 50 % : 1/√2 − 1 ≈ −29,3 %, pas −25 %.
        assert!((matched_return_pct(-50.0, 0.5) + 29.289322).abs() < 1e-5);
        // Bornes : exposition totale = la référence, exposition nulle = 0 (à l'arrondi flottant près).
        assert!((matched_return_pct(37.0, 1.0) - 37.0).abs() < 1e-9);
        assert!(matched_return_pct(37.0, 0.0).abs() < 1e-12);
    }

    #[test]
    fn ties_are_excluded_from_the_sign_test() {
        use Outcome::*;
        let t = sign_test(&[Win, Win, Tie, Tie, Loss], 0);
        assert_eq!((t.wins, t.losses, t.ties), (2, 1, 2));
        assert!((t.p_value - 0.5).abs() < 1e-12); // P(X ≥ 2 | n = 3) = 4/8
    }

    /// Sur une marche aléatoire PURE, une stratégie sans talent qui entre et sort avec
    /// des positions partielles (momentum 28 jours, un tiers du capital par symbole)
    /// ne doit pas battre la référence à exposition égale plus de 6 fois sur 10.
    /// Mesuré : 13 fenêtres sur 40 avec frais, 17 sur 40 sans frais.
    #[test]
    fn no_talent_does_not_beat_the_bar_on_a_pure_random_walk() {
        let p = catalog::find("tsmom_28_1d").unwrap();
        for costs in [CostModel::default(), CostModel::zero()] {
            let (mut wins, mut n) = (0, 0);
            for seed in 0..40u64 {
                let series: BTreeMap<String, Vec<Candle>> =
                    (0..3).map(|k| (format!("S{k}"), crate::testutil::random_walk(700, 100 + 3 * seed + k))).collect();
                let c = &series["S0"];
                let (start, end) = (c[150].open_time, c[650].open_time);
                let w = run_window(&p, &series, &External::default(), costs, 10_000.0, start, end).unwrap();
                wins += usize::from(w.outcome == Outcome::Win);
                n += 1;
            }
            assert!(wins * 10 <= n * 6, "{wins}/{n} fenêtres battues sans aucun talent ({costs:?})");
        }
    }

    #[test]
    fn the_least_favourable_split_is_kept() {
        use Outcome::*;
        // Fenêtres paires toutes gagnantes, impaires toutes perdantes : choisir les
        // paires serait tricher. Le test doit retenir les impaires.
        let o = [Win, Loss, Win, Loss, Win, Loss, Win, Loss, Win, Loss, Win, Loss];
        let t = worst_sign_test(&o, 2);
        assert_eq!(t.phase, 1);
        assert_eq!(t.wins, 0);
        assert!((t.p_value - 1.0).abs() < 1e-12);
    }

    #[test]
    fn verdict_accounts_for_every_strategy_tried() {
        let mk = |wins: usize, losses: usize| SignTest {
            phase: 0,
            wins,
            losses,
            ties: 0,
            p_value: binomial_tail_half(wins + losses, wins),
        };
        assert_eq!(judge_rolling(&mk(17, 0), 29).0, Robustness::PasDeLaChance);
        // 13/17 : 2,5 % seule, mais 71 % une fois 29 essais comptés.
        let (v, p_adj, _) = judge_rolling(&mk(13, 4), 29);
        assert_eq!(v, Robustness::Prometteuse);
        assert!(p_adj > 0.5);
        assert_eq!(judge_rolling(&mk(9, 8), 29).0, Robustness::CompatibleAvecLaChance);
        assert_eq!(judge_rolling(&mk(4, 0), 29).0, Robustness::TropPeuDeFenetres);
        assert_eq!(judge_rolling(&mk(13, 4), 1).0, Robustness::PasDeLaChance);
    }

    fn daily(n: usize, seed: u64, offset_days: i64) -> Vec<Candle> {
        synthetic(n, seed)
            .into_iter()
            .enumerate()
            .map(|(i, mut c)| {
                c.open_time = (i as i64 + offset_days) * DAY_MS;
                c.close_time = c.open_time + DAY_MS - 1;
                c
            })
            .collect()
    }

    /// Une fenêtre ne voit rien après sa fin : prolonger l'historique ne doit changer
    /// aucune des fenêtres déjà complètes.
    #[test]
    fn windows_never_see_beyond_their_end() {
        let preset = catalog::find("macd_1d").unwrap();
        let full: BTreeMap<String, Vec<Candle>> =
            [("A".to_string(), daily(1400, 31, 0)), ("B".to_string(), daily(1300, 32, 100))].into();
        let short: BTreeMap<String, Vec<Candle>> =
            full.iter().map(|(s, c)| (s.clone(), before(c, 1000 * DAY_MS).to_vec())).collect();
        let cfg = RollingConfig { window_days: 120, step_days: 60, tested_strategies: 29, initial_cash: 10_000.0 };
        let a = rolling_validation(&preset, &full, &External::default(), CostModel::default(), cfg).unwrap();
        let b = rolling_validation(&preset, &short, &External::default(), CostModel::default(), cfg).unwrap();
        assert!(b.windows.len() >= 5 && a.windows.len() > b.windows.len());
        assert_eq!(&a.windows[..b.windows.len()], &b.windows[..], "une fenêtre a lu des données postérieures à sa fin");
        assert!(a.windows.iter().any(|w| w.trades > 0), "test sans valeur : aucun trade");
    }

    #[test]
    fn late_listed_symbols_join_only_once_warmed_up() {
        let preset = catalog::find("donchian_55_20_1d").unwrap();
        let series: BTreeMap<String, Vec<Candle>> =
            [("OLD".to_string(), daily(1500, 41, 0)), ("NEW".to_string(), daily(700, 42, 800))].into();
        let cfg = RollingConfig { window_days: 180, step_days: 180, tested_strategies: 29, initial_cash: 10_000.0 };
        let r = rolling_validation(&preset, &series, &External::default(), CostModel::default(), cfg).unwrap();
        let ready_new = (800 + preset.warmup() as i64) * DAY_MS;
        for w in &r.windows {
            assert_eq!(w.symbols.contains(&"NEW".to_string()), w.start >= ready_new, "fenêtre {}", w.start / DAY_MS);
        }
        assert_eq!(r.stride, 1);
        assert_eq!(r.sign_test.wins + r.sign_test.losses + r.sign_test.ties, r.windows.len());
    }

    /// La référence elle-même, investie en permanence, ne peut pas « battre » la
    /// référence à exposition égale : toutes ses fenêtres sont nulles.
    #[test]
    fn buy_and_hold_has_no_timing_skill() {
        let preset = catalog::find("buy_hold").unwrap();
        let series: BTreeMap<String, Vec<Candle>> = [("A".to_string(), daily(1500, 51, 0))].into();
        let cfg = RollingConfig { window_days: 120, step_days: 120, tested_strategies: 29, initial_cash: 10_000.0 };
        let r = rolling_validation(&preset, &series, &External::default(), CostModel::default(), cfg).unwrap();
        assert!(r.windows.len() >= 8);
        for w in &r.windows {
            assert!((w.return_pct - w.benchmark_return_pct).abs() < 1e-9, "même stratégie, même résultat");
            assert!((w.exposure_pct - 100.0).abs() < 1e-9, "exposition {}", w.exposure_pct);
            assert_eq!(w.outcome, Outcome::Tie);
        }
        assert_eq!(r.verdict, Robustness::TropPeuDeFenetres);
    }

    #[test]
    fn rejects_bad_config() {
        let preset = catalog::find("macd_1d").unwrap();
        let series: BTreeMap<String, Vec<Candle>> = [("A".to_string(), daily(500, 1, 0))].into();
        let bad = RollingConfig { window_days: 90, step_days: 120, tested_strategies: 29, initial_cash: 1.0 };
        assert!(rolling_validation(&preset, &series, &External::default(), CostModel::default(), bad).is_err());
        let too_long = RollingConfig { window_days: 3000, step_days: 30, tested_strategies: 29, initial_cash: 1.0 };
        assert_eq!(
            rolling_validation(&preset, &series, &External::default(), CostModel::default(), too_long).unwrap_err(),
            ValidationError::NoWindow(3000)
        );
    }
}
