//! Registre des essais : « combien de choses a-t-on essayées sur ces données ? »
//!
//! La correction de Bonferroni multiplie la probabilité d'un coup de chance par
//! le nombre d'essais. Compter seulement le catalogue du moment permettrait de
//! se mentir : retirer les 22 perdantes ferait passer la correction de ×29 à ×7,
//! et changer un réglage (Keltner 2 ATR → 1,5 ATR) ne la changerait pas du tout.
//! Une stratégie chanceuse finirait alors « Solide », exactement l'erreur de la
//! première version (ses « gagnantes » sur 16 jours n'étaient que de la chance).
//!
//! Le registre `crates/pt-core/essais.tsv` est versionné et ne fait que grandir :
//! chaque stratégie (identifiant + réglages exacts) et chaque variante de la
//! sélection glissante essayée sur les données réelles y est inscrite AVANT
//! d'être lancée. Il est lu à la compilation : aucune lecture disque à l'exécution.
//!
//! Nombre d'essais retenu = taille de l'UNION du registre et de ce qui tourne :
//! jamais moins que ce qui a été inscrit, jamais moins que ce qui est essayé.

use crate::candle::Timeframe;
use crate::catalog;
use crate::strategy::{ExitPolicy, Preset, Pyramid, Rule, Sizing};
use serde::Serialize;
use std::collections::BTreeSet;

/// Le registre, tel que versionné.
pub const REGISTRY: &str = include_str!("../essais.tsv");

/// Chemin du registre dans le dépôt, pour les messages.
pub const REGISTRY_PATH: &str = "crates/pt-core/essais.tsv";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// Une stratégie du catalogue, avec ses réglages exacts.
    Strategy,
    /// Une variante de la sélection glissante (`pt walkforward`).
    Selection,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Strategy => "strategie",
            Kind::Selection => "selection",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trial {
    pub date: String,
    pub kind: Kind,
    pub id: String,
    /// Réglages exacts, en texte (voir [`fingerprint`]).
    pub fingerprint: String,
}

impl Trial {
    /// Ligne du registre : `date⇥genre⇥id⇥empreinte`.
    pub fn line(&self) -> String {
        format!("{}\t{}\t{}\t{}", self.date, self.kind.as_str(), self.id, self.fingerprint)
    }
}

/// Lit un registre. Lignes vides et commentaires (`#`) ignorés.
pub fn parse(text: &str) -> Result<Vec<Trial>, String> {
    let mut out = Vec::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim_end_matches('\r');
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() != 4 {
            return Err(format!(
                "{REGISTRY_PATH} ligne {} : 4 colonnes séparées par des tabulations attendues (date, genre, id, empreinte), {} trouvée(s)",
                n + 1,
                cols.len()
            ));
        }
        let kind = match cols[1] {
            "strategie" => Kind::Strategy,
            "selection" => Kind::Selection,
            other => {
                return Err(format!(
                    "{REGISTRY_PATH} ligne {} : genre « {other} » inconnu (strategie ou selection)",
                    n + 1
                ))
            }
        };
        if cols.iter().any(|c| c.trim().is_empty()) {
            return Err(format!("{REGISTRY_PATH} ligne {} : colonne vide", n + 1));
        }
        out.push(Trial { date: cols[0].into(), kind, id: cols[2].into(), fingerprint: cols[3].into() });
    }
    Ok(out)
}

/// Le registre versionné, lu.
pub fn registry() -> Vec<Trial> {
    parse(REGISTRY).unwrap_or_else(|e| panic!("registre des essais illisible : {e}"))
}

/// Réglages exacts d'une stratégie, en texte. Le nom et la description n'en font
/// pas partie : les reformuler n'est pas un nouvel essai. L'empreinte se compare
/// en TEXTE et n'est jamais relue en nombres (sans `float_roundtrip`, serde_json
/// peut changer le dernier chiffre d'un flottant relu).
pub fn fingerprint(p: &Preset) -> String {
    #[derive(Serialize)]
    struct Settings<'a> {
        timeframe: Timeframe,
        rule: &'a Rule,
        exits: &'a ExitPolicy,
        sizing: Sizing,
        pyramid: Option<Pyramid>,
        trend_sma: Option<usize>,
        max_positions: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        switch_margin: Option<f64>,
    }
    serde_json::to_string(&Settings {
        timeframe: p.timeframe,
        rule: &p.rule,
        exits: &p.exits,
        sizing: p.sizing,
        pyramid: p.pyramid,
        trend_sma: p.trend_sma,
        max_positions: p.max_positions,
        switch_margin: p.switch_margin,
    })
    .expect("réglages sérialisables")
}

/// Une variante de la sélection glissante : nombre de fenêtres passées regardées,
/// durée et décalage des fenêtres.
pub fn selection_fingerprint(lookback: usize, window_days: i64, step_days: i64) -> String {
    format!("lookback={lookback};window={window_days};step={step_days}")
}

fn strategy_set(registry: &[Trial], presets: &[Preset]) -> BTreeSet<(String, String)> {
    let mut set: BTreeSet<(String, String)> =
        registry.iter().filter(|t| t.kind == Kind::Strategy).map(|t| (t.id.clone(), t.fingerprint.clone())).collect();
    set.extend(presets.iter().filter(|p| !p.is_benchmark()).map(|p| (p.id.clone(), fingerprint(p))));
    set
}

/// Stratégies à compter dans la correction : registre ∪ stratégies données (la
/// référence exclue). Une même stratégie aux mêmes réglages compte une fois ; un
/// réglage différent compte comme un essai de plus.
pub fn strategy_trials_in(registry: &[Trial], presets: &[Preset]) -> usize {
    strategy_set(registry, presets).len()
}

/// Stratégies à compter, pour le registre versionné et le catalogue actuel.
pub fn strategy_trials() -> usize {
    strategy_trials_in(&registry(), &catalog::catalog())
}

/// Stratégies données absentes du registre, sous forme de lignes à y ajouter.
pub fn unregistered_strategies(registry: &[Trial], presets: &[Preset], date: &str) -> Vec<Trial> {
    let known: BTreeSet<(String, String)> =
        registry.iter().filter(|t| t.kind == Kind::Strategy).map(|t| (t.id.clone(), t.fingerprint.clone())).collect();
    presets
        .iter()
        .filter(|p| !p.is_benchmark())
        .map(|p| Trial { date: date.into(), kind: Kind::Strategy, id: p.id.clone(), fingerprint: fingerprint(p) })
        .filter(|t| !known.contains(&(t.id.clone(), t.fingerprint.clone())))
        .collect()
}

/// Variantes de sélection à compter : registre ∪ variantes demandées.
pub fn selection_trials_in(registry: &[Trial], requested: &[String]) -> usize {
    let mut set: BTreeSet<&str> =
        registry.iter().filter(|t| t.kind == Kind::Selection).map(|t| t.fingerprint.as_str()).collect();
    set.extend(requested.iter().map(String::as_str));
    set.len()
}

/// Variantes demandées qui ne sont pas encore inscrites au registre.
pub fn unregistered_selections(registry: &[Trial], requested: &[String]) -> Vec<String> {
    let known: BTreeSet<&str> =
        registry.iter().filter(|t| t.kind == Kind::Selection).map(|t| t.fingerprint.as_str()).collect();
    let mut out: Vec<String> = requested.iter().filter(|f| !known.contains(f.as_str())).cloned().collect();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_parses_and_never_counts_below_the_catalog() {
        let reg = registry();
        assert!(!reg.is_empty(), "le registre versionné est vide");
        let in_catalog = catalog::catalog().iter().filter(|p| !p.is_benchmark()).count();
        assert!(strategy_trials() >= in_catalog);
    }

    #[test]
    fn every_catalog_preset_is_registered() {
        let missing = unregistered_strategies(&registry(), &catalog::catalog(), "AAAA-MM-JJ");
        let lines: Vec<String> = missing.iter().map(Trial::line).collect();
        assert!(
            missing.is_empty(),
            "stratégies du catalogue absentes de {REGISTRY_PATH} (ajoute ces lignes, ou lance `pt essais`) :\n{}",
            lines.join("\n")
        );
    }

    #[test]
    fn removing_a_strategy_never_lowers_the_correction() {
        let reg = registry();
        let full = catalog::catalog();
        let before = strategy_trials_in(&reg, &full);
        // On ne garde que la référence et Keltner : 28 stratégies retirées.
        let kept: Vec<Preset> =
            full.iter().filter(|p| p.is_benchmark() || p.id == "keltner_breakout_4h").cloned().collect();
        assert_eq!(strategy_trials_in(&reg, &kept), before);
        assert_eq!(strategy_trials_in(&reg, &[]), before);
    }

    #[test]
    fn changing_a_setting_counts_as_a_new_trial() {
        let reg = registry();
        let full = catalog::catalog();
        let before = strategy_trials_in(&reg, &full);
        let mut tuned = full.clone();
        for p in tuned.iter_mut() {
            if let (true, Rule::KeltnerBreakout { mult, .. }) = (p.id == "keltner_breakout_4h", &mut p.rule) {
                *mult = 1.5;
            }
        }
        assert_eq!(strategy_trials_in(&reg, &tuned), before + 1, "un réglage changé doit compter");
        // Le registre garde l'ancien réglage : l'inscrire ne fait pas recompter le nouveau.
        let mut grown = reg.clone();
        grown.extend(unregistered_strategies(&reg, &tuned, "2026-10-06"));
        assert_eq!(strategy_trials_in(&grown, &tuned), before + 1);
        // Reformuler le nom n'est pas un nouvel essai.
        let mut renamed = full.clone();
        renamed[1].name = "Autre nom".into();
        renamed[1].description = "Autre description".into();
        assert_eq!(strategy_trials_in(&reg, &renamed), before);
    }

    #[test]
    fn duplicate_trial_counts_once() {
        let mut reg = registry();
        let before = strategy_trials_in(&reg, &[]);
        let first = reg.iter().find(|t| t.kind == Kind::Strategy).cloned().expect("au moins une stratégie");
        reg.push(Trial { date: "2027-01-01".into(), ..first.clone() });
        reg.push(first);
        assert_eq!(strategy_trials_in(&reg, &[]), before);
        let sel = selection_fingerprint(2, 180, 90);
        assert_eq!(
            selection_trials_in(&reg, &[sel.clone(), sel.clone()]),
            selection_trials_in(&reg, std::slice::from_ref(&sel))
        );
    }

    #[test]
    fn each_lookback_value_is_a_trial() {
        let reg = registry();
        let asked: Vec<String> = [1, 2, 4].iter().map(|l| selection_fingerprint(*l, 180, 90)).collect();
        assert!(selection_trials_in(&reg, &asked) >= 3);
        // Une autre durée de fenêtre est aussi une autre variante.
        let other = vec![selection_fingerprint(2, 90, 90)];
        assert!(selection_trials_in(&reg, &other) > selection_trials_in(&reg, &[]));
    }

    #[test]
    fn malformed_registry_is_refused() {
        assert!(parse("2026-10-06\tstrategie\tx").is_err());
        assert!(parse("2026-10-06\tautre\tx\t{}").is_err());
        assert!(parse("2026-10-06\tstrategie\t\t{}").is_err());
        let ok =
            parse("# commentaire\n\n2026-10-06\tselection\twalkforward\tlookback=2;window=180;step=90\r\n").unwrap();
        assert_eq!(ok.len(), 1);
        assert_eq!(ok[0].kind, Kind::Selection);
        assert_eq!(parse(&ok[0].line()).unwrap(), ok);
    }
}
