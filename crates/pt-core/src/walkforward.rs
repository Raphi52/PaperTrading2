//! Sélection glissante (« walk-forward ») : choisir la stratégie avec le passé
//! seulement, puis juger ce PROCÉDÉ de choix.
//!
//! Améliorer en boucle en gardant « la meilleure » stratégie sur l'historique
//! revient à choisir après coup : c'est l'erreur de la première version. Ici, le
//! choix fait lui-même partie de la stratégie jugée :
//!
//! 1. toutes les stratégies sont rejouées sur la MÊME grille de fenêtres (chacune
//!    repart de zéro et se compare à un achat au hasard de même exposition, comme
//!    dans la validation) ;
//! 2. pour chaque fenêtre, on joue la stratégie dont l'écart au hasard a été le
//!    meilleur, en moyenne, sur les `lookback` dernières fenêtres TERMINÉES avant
//!    son début : elle ne voit jamais la fenêtre qu'elle joue ;
//! 3. le résultat de la fenêtre est celui de la stratégie choisie ; le procédé est
//!    jugé par le même test du signe sur fenêtres indépendantes (découpage le
//!    moins favorable), corrigé pour le nombre de VARIANTES de sélection essayées.
//!
//! Le choix parmi les stratégies est déjà payé dans les résultats hors échantillon
//! (on ne garde jamais la meilleure après coup) ; ce qu'il reste à compter, ce sont
//! les variantes du procédé (registre des essais, `selection`).

use crate::candle::Candle;
use crate::portfolio::CostModel;
use crate::strategy::{External, Preset};
use crate::validation::{
    fmt_p, judge_rolling, median, run_window, worst_sign_test, Outcome, Robustness, SignTest, ValidationError,
    WindowResult, MIN_INDEPENDENT_WINDOWS,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const DAY_MS: i64 = 86_400_000;

/// Une stratégie candidate et les données de son unité de temps.
#[derive(Clone, Copy)]
pub struct Candidate<'a> {
    pub preset: &'a Preset,
    pub series: &'a BTreeMap<String, Vec<Candle>>,
    pub ext: &'a External,
}

/// Toutes les candidates rejouées sur la même grille de fenêtres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scores {
    pub window_days: i64,
    pub step_days: i64,
    pub costs: CostModel,
    /// Fenêtres `[début, fin)`, dans l'ordre.
    pub windows: Vec<(i64, i64)>,
    pub ids: Vec<String>,
    pub names: Vec<String>,
    /// `cells[i][j]` : candidate `i` dans la fenêtre `j` ; `None` si elle n'était pas prête.
    pub cells: Vec<Vec<Option<WindowResult>>>,
}

/// Grille commune : départ au premier jour où une candidate est prête, puis une
/// fenêtre tous les `step_days`, tant que TOUTES les unités de temps couvrent la fenêtre.
pub fn common_windows(
    candidates: &[Candidate],
    window_days: i64,
    step_days: i64,
) -> Result<Vec<(i64, i64)>, ValidationError> {
    if window_days < 30 || step_days < 1 || step_days > window_days {
        return Err(ValidationError::InvalidConfig(
            "la fenêtre doit durer au moins 30 jours, et le pas être compris entre 1 jour et la durée d'une fenêtre"
                .into(),
        ));
    }
    if candidates.is_empty() {
        return Err(ValidationError::InvalidConfig("aucune stratégie candidate".into()));
    }
    let ready = |c: &Candidate| {
        let w = c.preset.warmup();
        c.series.values().filter(|s| s.len() > w).map(|s| s[w].open_time).min()
    };
    let end_of_data = |c: &Candidate| c.series.values().filter_map(|s| s.last()).map(|x| x.close_time + 1).max();
    let first = candidates.iter().filter_map(ready).min();
    let last = candidates.iter().map(end_of_data).collect::<Option<Vec<i64>>>().and_then(|v| v.into_iter().min());
    let (Some(first), Some(last)) = (first, last) else {
        return Err(ValidationError::NoWindow(window_days));
    };
    // Débute à minuit UTC : les fenêtres tombent sur des bougies entières, quelle que soit l'unité de temps.
    let first = first.div_euclid(DAY_MS) * DAY_MS + if first.rem_euclid(DAY_MS) == 0 { 0 } else { DAY_MS };
    let (len, step) = (window_days * DAY_MS, step_days * DAY_MS);
    let mut out = Vec::new();
    let mut start = first;
    while start + len <= last {
        out.push((start, start + len));
        start += step;
    }
    if out.is_empty() {
        return Err(ValidationError::NoWindow(window_days));
    }
    Ok(out)
}

/// Rejoue chaque candidate dans chaque fenêtre de la grille commune (une tâche par candidate).
pub fn score_candidates(
    candidates: &[Candidate],
    costs: CostModel,
    window_days: i64,
    step_days: i64,
    initial_cash: f64,
) -> Result<Scores, ValidationError> {
    if !(initial_cash.is_finite() && initial_cash > 0.0) {
        return Err(ValidationError::InvalidConfig("capital initial invalide".into()));
    }
    let windows = common_windows(candidates, window_days, step_days)?;
    let cells: Vec<Vec<Option<WindowResult>>> = std::thread::scope(|scope| {
        let tasks: Vec<_> = candidates
            .iter()
            .map(|c| {
                let windows = &windows;
                scope.spawn(move || {
                    windows
                        .iter()
                        .map(|&(a, b)| run_window(c.preset, c.series, c.ext, costs, initial_cash, a, b))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        tasks.into_iter().map(|t| t.join().expect("calcul d'une candidate")).collect()
    });
    Ok(Scores {
        window_days,
        step_days,
        costs,
        windows,
        ids: candidates.iter().map(|c| c.preset.id.clone()).collect(),
        names: candidates.iter().map(|c| c.preset.name.clone()).collect(),
        cells,
    })
}

/// Le choix, fenêtre par fenêtre, à partir des écarts au hasard (`excess[i][j]`).
///
/// Pour la fenêtre `j` : seules comptent les `lookback` fenêtres TERMINÉES avant
/// son début. Une candidate n'est éligible que si elle a un résultat dans chacune
/// d'elles et qu'elle est prête pour `j`. Meilleure moyenne ; à égalité, la
/// première dans l'ordre donné. `None` : aucune candidate éligible.
pub fn select(excess: &[Vec<Option<f64>>], windows: &[(i64, i64)], lookback: usize) -> Vec<Option<(usize, f64)>> {
    windows
        .iter()
        .enumerate()
        .map(|(j, &(start, _))| {
            if lookback == 0 {
                return None;
            }
            let finished: Vec<usize> = (0..windows.len()).filter(|&k| windows[k].1 <= start).collect();
            if finished.len() < lookback {
                return None;
            }
            let past = &finished[finished.len() - lookback..];
            let mut best: Option<(usize, f64)> = None;
            for (i, row) in excess.iter().enumerate() {
                if row.get(j).copied().flatten().is_none() {
                    continue;
                }
                let Some(values) = past.iter().map(|&k| row.get(k).copied().flatten()).collect::<Option<Vec<f64>>>()
                else {
                    continue;
                };
                let score = values.iter().sum::<f64>() / lookback as f64;
                if best.is_none_or(|(_, b)| score > b) {
                    best = Some((i, score));
                }
            }
            best
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayedWindow {
    pub start: i64,
    pub end: i64,
    /// Stratégie choisie ; `None` si aucune n'avait assez de fenêtres passées.
    pub chosen_id: Option<String>,
    pub chosen_name: Option<String>,
    /// Écart moyen au hasard, sur les fenêtres passées, qui a décidé du choix.
    pub past_score: Option<f64>,
    /// Résultat de la stratégie choisie dans CETTE fenêtre.
    pub result: Option<WindowResult>,
    pub outcome: Outcome,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WalkForwardReport {
    pub lookback: usize,
    pub window_days: i64,
    pub step_days: i64,
    pub costs: CostModel,
    pub candidates: usize,
    /// Variantes de sélection comptées dans la correction (registre ∪ demandées).
    pub tested_selections: usize,
    pub windows: Vec<PlayedWindow>,
    pub played: usize,
    /// Une fenêtre sur `stride` est retenue pour que les fenêtres testées ne se chevauchent pas.
    pub stride: usize,
    pub sign_test: SignTest,
    pub p_adjusted: f64,
    pub verdict: Robustness,
    pub verdict_reason: String,
    pub median_excess_pct: f64,
    /// Fenêtres jouées du découpage retenu, mises bout à bout (elles ne se chevauchent pas).
    pub independent_played: usize,
    pub compounded_return_pct: f64,
    pub compounded_benchmark_pct: f64,
    pub compounded_matched_pct: f64,
    /// Nombre de fenêtres jouées par stratégie choisie, du plus fréquent au moins fréquent.
    pub picks: Vec<(String, usize)>,
}

/// Juge le procédé « choisir avec les `lookback` dernières fenêtres terminées ».
pub fn walk_forward(scores: &Scores, lookback: usize, tested_selections: usize) -> WalkForwardReport {
    let excess: Vec<Vec<Option<f64>>> =
        scores.cells.iter().map(|row| row.iter().map(|c| c.as_ref().map(|w| w.excess_pct)).collect()).collect();
    let choices = select(&excess, &scores.windows, lookback);
    let windows: Vec<PlayedWindow> = scores
        .windows
        .iter()
        .zip(&choices)
        .enumerate()
        .map(|(j, (&(start, end), choice))| match choice {
            Some((i, score)) => {
                let result = scores.cells[*i][j].clone().expect("une candidate éligible est prête");
                PlayedWindow {
                    start,
                    end,
                    chosen_id: Some(scores.ids[*i].clone()),
                    chosen_name: Some(scores.names[*i].clone()),
                    past_score: Some(*score),
                    outcome: result.outcome,
                    result: Some(result),
                }
            }
            None => PlayedWindow {
                start,
                end,
                chosen_id: None,
                chosen_name: None,
                past_score: None,
                result: None,
                // Une fenêtre non jouée est écartée du test, comme une fenêtre nulle.
                outcome: Outcome::Tie,
            },
        })
        .collect();
    let stride = ((scores.window_days + scores.step_days - 1) / scores.step_days) as usize;
    let outcomes: Vec<Outcome> = windows.iter().map(|w| w.outcome).collect();
    let test = worst_sign_test(&outcomes, stride);
    let tested = tested_selections.max(1);
    let (verdict, p_adjusted, _) = judge_rolling(&test, tested);
    let n = test.wins + test.losses;
    let verdict_reason = if verdict == Robustness::TropPeuDeFenetres {
        format!(
            "{n} fenêtre(s) indépendante(s) jouée(s) seulement : il en faut au moins {MIN_INDEPENDENT_WINDOWS}. Il faut plus d'historique, ou moins de fenêtres passées pour choisir."
        )
    } else {
        let base = format!(
            "La stratégie choisie bat la référence à exposition égale dans {} fenêtre(s) indépendante(s) sur {n}. À pile ou face, on ferait au moins aussi bien avec une probabilité de {}",
            test.wins,
            fmt_p(test.p_value)
        );
        match verdict {
            Robustness::PasDeLaChance => format!(
                "{base}, et encore {} après avoir compté les {tested} variante(s) de sélection essayée(s).",
                fmt_p(p_adjusted)
            ),
            Robustness::Prometteuse => format!(
                "{base}. Mais {tested} variantes de sélection ont été essayées : une fois ce choix compté, la probabilité monte à {}, au-dessus du seuil de 5 %.",
                fmt_p(p_adjusted)
            ),
            _ => format!("{base}, au-dessus du seuil de 5 %."),
        }
    };
    let played: Vec<&WindowResult> = windows.iter().filter_map(|w| w.result.as_ref()).collect();
    let mut excess_played: Vec<f64> = played.iter().map(|w| w.excess_pct).collect();
    let independent: Vec<&WindowResult> =
        windows.iter().skip(test.phase).step_by(stride.max(1)).filter_map(|w| w.result.as_ref()).collect();
    let compound = |f: &dyn Fn(&WindowResult) -> f64| {
        (independent.iter().map(|w| 1.0 + f(w) / 100.0).product::<f64>() - 1.0) * 100.0
    };
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for w in &windows {
        if let Some(id) = &w.chosen_id {
            *counts.entry(id.clone()).or_default() += 1;
        }
    }
    let mut picks: Vec<(String, usize)> = counts.into_iter().collect();
    picks.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    WalkForwardReport {
        lookback,
        window_days: scores.window_days,
        step_days: scores.step_days,
        costs: scores.costs,
        candidates: scores.ids.len(),
        tested_selections: tested,
        played: played.len(),
        stride,
        sign_test: test,
        p_adjusted,
        verdict,
        verdict_reason,
        median_excess_pct: median(&mut excess_played),
        independent_played: independent.len(),
        compounded_return_pct: compound(&|w| w.return_pct),
        compounded_benchmark_pct: compound(&|w| w.benchmark_return_pct),
        compounded_matched_pct: compound(&|w| w.matched_return_pct),
        picks,
        windows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candle::Timeframe;
    use crate::catalog;
    use crate::testutil::synthetic;

    fn on_timeframe(n: usize, seeds: &[u64], tf: Timeframe) -> BTreeMap<String, Vec<Candle>> {
        generated(synthetic, n, seeds, tf)
    }

    fn generated(
        make: fn(usize, u64) -> Vec<Candle>,
        n: usize,
        seeds: &[u64],
        tf: Timeframe,
    ) -> BTreeMap<String, Vec<Candle>> {
        let step = tf.millis();
        seeds
            .iter()
            .map(|s| {
                let c = make(n, *s)
                    .into_iter()
                    .enumerate()
                    .map(|(i, mut c)| {
                        c.open_time = i as i64 * step;
                        c.close_time = c.open_time + step - 1;
                        c
                    })
                    .collect();
                (format!("S{s}USDT"), c)
            })
            .collect()
    }

    /// Grille de 10 fenêtres de 60 jours décalées de 30 jours.
    fn grid(n: usize) -> Vec<(i64, i64)> {
        (0..n as i64).map(|j| (j * 30 * DAY_MS, (j * 30 + 60) * DAY_MS)).collect()
    }

    #[test]
    fn selection_never_sees_the_window_it_plays() {
        let windows = grid(10);
        // A gagne modérément tout le temps ; B perd tout, sauf un gain énorme en fenêtre 6.
        let a: Vec<Option<f64>> = vec![Some(1.0); 10];
        let mut b: Vec<Option<f64>> = vec![Some(-1.0); 10];
        b[6] = Some(1_000.0);
        let picks = select(&[a.clone(), b.clone()], &windows, 2);
        assert_eq!(picks[6].map(|p| p.0), Some(0), "le gain de B en fenêtre 6 ne peut pas décider de la fenêtre 6");
        // Fenêtre 6 = [180, 240) jours : la fenêtre 5 ([150, 210)) n'est pas finie au jour 180.
        assert_eq!(picks[7].map(|p| p.0), Some(0), "fenêtre 6 pas encore terminée au début de la fenêtre 7");
        assert_eq!(picks[8].map(|p| p.0), Some(1), "fenêtre 6 terminée au début de la fenêtre 8");
        // Propriété : changer ce qui n'est pas terminé au début de j ne change jamais le choix de j.
        let mut state = 7u64;
        let mut rnd = move || {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((state >> 33) as f64) / (1u64 << 31) as f64 * 200.0 - 100.0
        };
        let base = select(&[a.clone(), b.clone()], &windows, 2);
        for j in 0..windows.len() {
            for _ in 0..20 {
                let mut rows = vec![a.clone(), b.clone()];
                for row in rows.iter_mut() {
                    for k in 0..windows.len() {
                        if windows[k].1 > windows[j].0 {
                            row[k] = Some(rnd());
                        }
                    }
                }
                assert_eq!(select(&rows, &windows, 2)[j].map(|p| p.0), base[j].map(|p| p.0), "fenêtre {j}");
            }
        }
    }

    #[test]
    fn unready_strategy_is_not_eligible() {
        let windows = grid(6);
        // La première candidate gagnerait toutes les égalités, mais n'a pas de passé avant la fenêtre 3.
        let late = vec![None, None, None, Some(50.0), Some(50.0), Some(50.0)];
        let early = vec![Some(-5.0); 6];
        let picks = select(&[late.clone(), early.clone()], &windows, 2);
        assert_eq!(picks[0], None, "aucune fenêtre terminée avant la première");
        assert_eq!(picks[1], None, "une seule fenêtre ne peut pas être terminée avant la deuxième");
        assert_eq!(picks[3].map(|p| p.0), Some(1));
        assert_eq!(picks[4].map(|p| p.0), Some(1), "la fenêtre 2 de la tardive est vide : pas éligible");
        // Prête dans le passé mais pas dans la fenêtre jouée : pas éligible non plus.
        let gap = vec![Some(90.0), Some(90.0), Some(90.0), None, Some(90.0), Some(90.0)];
        assert_eq!(select(&[gap, early.clone()], &windows, 2)[3].map(|p| p.0), Some(1));
        // Personne de prêt : la fenêtre n'est pas jouée.
        assert!(select(&[vec![None; 6]], &windows, 1).iter().all(Option::is_none));
    }

    #[test]
    fn selection_is_deterministic_on_ties() {
        let windows = grid(8);
        let rows = vec![vec![Some(2.0); 8], vec![Some(2.0); 8], vec![Some(1.0); 8]];
        let a = select(&rows, &windows, 3);
        let b = select(&rows, &windows, 3);
        assert_eq!(a, b);
        assert!(a.iter().flatten().all(|(i, _)| *i == 0), "à égalité, la première dans l'ordre du catalogue");
        // Tous les scores passés négatifs : on choisit quand même la moins mauvaise.
        let bad = vec![vec![Some(-3.0); 8], vec![Some(-1.0); 8]];
        assert!(select(&bad, &windows, 1).iter().flatten().all(|(i, _)| *i == 1));
    }

    #[test]
    fn every_strategy_is_scored_on_the_same_windows() {
        let h1 = on_timeframe(24 * 420, &[3, 4], Timeframe::H1);
        let h4 = on_timeframe(6 * 420, &[3, 4], Timeframe::H4);
        let d1 = on_timeframe(420, &[3, 4], Timeframe::D1);
        let ext = External::default();
        let presets: Vec<Preset> =
            ["ema_9_21_1h", "ema_12_26_4h", "macd_1d"].iter().map(|id| catalog::find(id).unwrap()).collect();
        let candidates: Vec<Candidate> = presets
            .iter()
            .map(|p| Candidate {
                preset: p,
                series: match p.timeframe {
                    Timeframe::H1 => &h1,
                    Timeframe::H4 => &h4,
                    _ => &d1,
                },
                ext: &ext,
            })
            .collect();
        let s = score_candidates(&candidates, CostModel::default(), 60, 30, 10_000.0).unwrap();
        assert!(s.windows.len() >= 8, "{} fenêtres", s.windows.len());
        for (i, row) in s.cells.iter().enumerate() {
            assert_eq!(row.len(), s.windows.len());
            let mut seen_ready = false;
            for (j, cell) in row.iter().enumerate() {
                match cell {
                    Some(w) => {
                        assert_eq!((w.start, w.end), s.windows[j], "{} fenêtre {j}", s.ids[i]);
                        seen_ready = true;
                    }
                    None => assert!(!seen_ready, "{} : trou après avoir été prête (fenêtre {j})", s.ids[i]),
                }
            }
            assert!(seen_ready, "{} jamais prête", s.ids[i]);
        }
        // Chaque fenêtre commence à minuit, pour toutes les unités de temps.
        assert!(s.windows.iter().all(|(a, _)| a % DAY_MS == 0));
    }

    fn daily_candidates(presets: &[Preset]) -> Vec<&Preset> {
        presets.iter().filter(|p| p.timeframe == Timeframe::D1 && !p.is_benchmark()).collect()
    }

    /// Sur des prix tirés au hasard, aucun talent n'existe : le procédé ne doit
    /// jamais être déclaré « Pas un coup de chance ». S'il voyait le futur, il
    /// gagnerait presque toutes les fenêtres. Marche aléatoire PURE : le générateur
    /// à régimes alterne des tendances de 150 bougies, qu'un suivi de tendance
    /// exploite réellement (le momentum 28 jours y gagnait 14 fenêtres sur 18).
    #[test]
    fn selection_on_random_walks_is_not_solide() {
        let all = catalog::catalog();
        let presets = daily_candidates(&all);
        let ext = External::default();
        for seeds in [[21u64, 22], [23, 24], [25, 26], [27, 28], [29, 30]] {
            let d1 = generated(crate::testutil::random_walk, 2500, &seeds, Timeframe::D1);
            let candidates: Vec<Candidate> =
                presets.iter().map(|p| Candidate { preset: p, series: &d1, ext: &ext }).collect();
            let s = score_candidates(&candidates, CostModel::default(), 120, 60, 10_000.0).unwrap();
            let r = walk_forward(&s, 2, 1);
            let n = r.sign_test.wins + r.sign_test.losses;
            assert!(n >= MIN_INDEPENDENT_WINDOWS, "{seeds:?} : test vide ({n} fenêtres indépendantes)");
            assert_ne!(r.verdict, Robustness::PasDeLaChance, "{seeds:?} : {}", r.verdict_reason);
            assert!(
                (r.sign_test.wins as f64) < 0.85 * n as f64,
                "{seeds:?} : {} / {n} gagnées sur du bruit",
                r.sign_test.wins
            );
        }
    }

    #[test]
    fn huge_lookback_gives_too_few_windows() {
        let d1 = on_timeframe(900, &[31, 32], Timeframe::D1);
        let ext = External::default();
        let p = catalog::find("macd_1d").unwrap();
        let s =
            score_candidates(&[Candidate { preset: &p, series: &d1, ext: &ext }], CostModel::default(), 120, 60, 1e4)
                .unwrap();
        let r = walk_forward(&s, 1_000, 1);
        assert_eq!(r.played, 0);
        assert_eq!(r.verdict, Robustness::TropPeuDeFenetres);
        assert!(r.verdict_reason.contains("au moins"), "{}", r.verdict_reason);
        assert_eq!(walk_forward(&s, 0, 1).played, 0, "lookback nul : rien n'est joué, sans plantage");
    }

    #[test]
    fn played_windows_report_the_chosen_strategy_result() {
        let d1 = on_timeframe(1600, &[33, 34], Timeframe::D1);
        let ext = External::default();
        let all = catalog::catalog();
        let presets = daily_candidates(&all);
        let candidates: Vec<Candidate> =
            presets.iter().map(|p| Candidate { preset: p, series: &d1, ext: &ext }).collect();
        let s = score_candidates(&candidates, CostModel::default(), 120, 60, 10_000.0).unwrap();
        let r = walk_forward(&s, 2, 3);
        assert!(r.played > 0);
        assert_eq!(r.tested_selections, 3);
        for (j, w) in r.windows.iter().enumerate() {
            if let Some(id) = &w.chosen_id {
                let i = s.ids.iter().position(|x| x == id).unwrap();
                assert_eq!(w.result.as_ref(), s.cells[i][j].as_ref());
                assert_eq!(w.outcome, s.cells[i][j].as_ref().unwrap().outcome);
            } else {
                assert_eq!(w.outcome, Outcome::Tie);
            }
        }
        assert_eq!(r.picks.iter().map(|(_, n)| n).sum::<usize>(), r.played);
    }

    #[test]
    fn rejects_bad_grid() {
        let d1 = on_timeframe(400, &[35], Timeframe::D1);
        let ext = External::default();
        let p = catalog::find("macd_1d").unwrap();
        let c = [Candidate { preset: &p, series: &d1, ext: &ext }];
        assert!(score_candidates(&c, CostModel::default(), 10, 5, 1e4).is_err());
        assert!(score_candidates(&c, CostModel::default(), 60, 90, 1e4).is_err());
        assert!(score_candidates(&c, CostModel::default(), 60, 30, 0.0).is_err());
        assert!(score_candidates(&[], CostModel::default(), 60, 30, 1e4).is_err());
        assert_eq!(
            score_candidates(&c, CostModel::default(), 3000, 30, 1e4).unwrap_err(),
            ValidationError::NoWindow(3000)
        );
    }
}
