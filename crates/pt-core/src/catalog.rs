//! Catalogue des stratégies prêtes à l'emploi.
//!
//! L'ancien bot affichait 170 noms pour une quinzaine de logiques réelles.
//! Ici, chaque entrée dit exactement ce qu'elle calcule, sur quelle unité de
//! temps, et comment elle sort.

use crate::candle::Timeframe;
use crate::strategy::*;

#[allow(clippy::too_many_arguments)]
fn p(
    id: &str,
    name: &str,
    family: &str,
    description: &str,
    timeframe: Timeframe,
    rule: Rule,
    exits: ExitPolicy,
    sizing: Sizing,
) -> Preset {
    Preset {
        id: id.into(),
        name: name.into(),
        family: family.into(),
        description: description.into(),
        timeframe,
        rule,
        exits,
        sizing,
        pyramid: None,
        trend_sma: None,
        max_positions: None,
        switch_margin: None,
    }
}

/// Sortie de suivi de tendance : stop initial à 2,5 ATR puis stop suiveur à 3 ATR du plus haut.
fn trend_exits() -> ExitPolicy {
    ExitPolicy {
        stop: Some(Distance::Atr(2.5)),
        take_profit: None,
        trailing: Some(Trailing { activation_pct: 0.0, distance: Distance::Atr(3.0) }),
        max_bars: None,
    }
}

/// Sortie de retour à la moyenne : stop à 2 ATR, objectif 2R, durée bornée.
fn reversion_exits(max_bars: u32) -> ExitPolicy {
    ExitPolicy {
        stop: Some(Distance::Atr(2.0)),
        take_profit: Some(TakeProfit::RMultiple(2.0)),
        trailing: None,
        max_bars: Some(max_bars),
    }
}

/// Sortie d'un filtre de régime : on sort sur le signal ; le stop à 25 % n'est
/// qu'un garde-fou contre un krach entre deux clôtures.
fn regime_exits() -> ExitPolicy {
    ExitPolicy { stop: Some(Distance::Percent(25.0)), take_profit: None, trailing: None, max_bars: None }
}

fn risk() -> Sizing {
    Sizing::Risk { risk_pct: 1.0, max_position_pct: 25.0 }
}

// fix-ok: edits = ajout de 70 variantes + remplacement mesure de 2 variantes (RSI(14) 25->50 1h, replis -15% 1j) qui n'achetaient jamais sur les donnees de test (catalog_is_consistent rouge) ; tests verts apres.
pub fn catalog() -> Vec<Preset> {
    use Timeframe::*;
    let mut v = vec![
        p(
            "buy_hold",
            "Acheter et garder",
            "Référence",
            "Achète chaque symbole à parts égales dès le départ et ne vend jamais. C'est la barre que toute stratégie doit dépasser.",
            D1,
            Rule::BuyHold,
            ExitPolicy::default(),
            Sizing::EqualWeight,
        ),
        p("ema_9_21_1h", "Croisement EMA 9/21 (1h)", "Tendance",
          "Entre quand l'EMA 9 passe au-dessus de l'EMA 21, sort au croisement inverse ou au stop suiveur ATR.",
          H1, Rule::EmaCross { fast: 9, slow: 21 }, trend_exits(), risk()),
        p("ema_12_26_4h", "Croisement EMA 12/26 (4h)", "Tendance",
          "Croisement EMA 12/26 sur 4h : moins de faux signaux que le 1h.",
          H4, Rule::EmaCross { fast: 12, slow: 26 }, trend_exits(), risk()),
        p("ema_20_50_4h", "Croisement EMA 20/50 (4h)", "Tendance",
          "Croisement lent EMA 20/50 sur 4h, pour les tendances de plusieurs semaines.",
          H4, Rule::EmaCross { fast: 20, slow: 50 }, trend_exits(), risk()),
        p("ema_50_200_1d", "Croix dorée EMA 50/200 (1j)", "Tendance",
          "Croisement EMA 50/200 en journalier : très peu de trades, tendances longues.",
          D1, Rule::EmaCross { fast: 50, slow: 200 }, trend_exits(), risk()),
        p("macd_4h", "Croisement MACD (4h)", "Tendance",
          "Entre quand la ligne MACD (12,26) croise sa ligne de signal (9) vers le haut, sort au croisement inverse.",
          H4, Rule::MacdCross { fast: 12, slow: 26, signal: 9 }, trend_exits(), risk()),
        p("macd_1d", "Croisement MACD (1j)", "Tendance",
          "Même règle MACD en journalier.",
          D1, Rule::MacdCross { fast: 12, slow: 26, signal: 9 }, trend_exits(), risk()),
        p("supertrend_7_2_1h", "Supertrend 7×2 (1h)", "Tendance",
          "Entre quand le Supertrend (ATR 7, multiplicateur 2) passe haussier, sort quand il repasse baissier.",
          H1, Rule::Supertrend { period: 7, mult: 2.0 }, trend_exits(), risk()),
        p("supertrend_10_3_4h", "Supertrend 10×3 (4h)", "Tendance",
          "Supertrend classique (ATR 10, multiplicateur 3) sur 4h.",
          H4, Rule::Supertrend { period: 10, mult: 3.0 }, trend_exits(), risk()),
        p("supertrend_10_3_1d", "Supertrend 10×3 (1j)", "Tendance",
          "Supertrend classique en journalier.",
          D1, Rule::Supertrend { period: 10, mult: 3.0 }, trend_exits(), risk()),
        p("ichimoku_4h", "Ichimoku Tenkan/Kijun (4h)", "Tendance",
          "Entre au croisement Tenkan/Kijun vers le haut si le prix est au-dessus du nuage ; sort au croisement inverse ou sous le nuage.",
          H4, Rule::Ichimoku { tenkan: 9, kijun: 26, senkou: 52 }, trend_exits(), risk()),
        p("ichimoku_1d", "Ichimoku Tenkan/Kijun (1j)", "Tendance",
          "Même règle Ichimoku en journalier.",
          D1, Rule::Ichimoku { tenkan: 9, kijun: 26, senkou: 52 }, trend_exits(), risk()),
        p("adx_trend_4h", "Tendance forte ADX (4h)", "Tendance",
          "Entre quand l'ADX(14) dépasse 25 avec +DI > −DI et le prix au-dessus de l'EMA 50 ; sort quand −DI repasse devant.",
          H4, Rule::AdxTrend { period: 14, threshold: 25.0, ema: 50 }, trend_exits(), risk()),
        p("confluence_4h", "Confluence 4/5 (4h)", "Tendance",
          "Score sur 5 critères (EMA 50>200, MACD positif, RSI 45-70, Supertrend haussier, ADX>20 orienté) : entre à 4/5, sort à 2/5 ou moins.",
          H4, Rule::Confluence { min_score: 4, exit_score: 2 }, trend_exits(), risk()),
        p("confluence_1d", "Confluence 4/5 (1j)", "Tendance",
          "Même score de confluence en journalier.",
          D1, Rule::Confluence { min_score: 4, exit_score: 2 }, trend_exits(), risk()),
        p("donchian_20_10_4h", "Cassure Donchian 20/10 (4h)", "Cassure",
          "Entre quand la clôture dépasse le plus haut des 20 bougies précédentes, sort sous le plus bas des 10 précédentes.",
          H4, Rule::DonchianBreakout { entry: 20, exit: 10 }, trend_exits(), risk()),
        p("donchian_55_20_1d", "Cassure Donchian 55/20 (1j, « Turtle »)", "Cassure",
          "Règle des Turtles : cassure du plus haut 55 jours, sortie sous le plus bas 20 jours.",
          D1, Rule::DonchianBreakout { entry: 55, exit: 20 }, trend_exits(), risk()),
        p("keltner_breakout_4h", "Cassure Keltner (4h)", "Cassure",
          "Entre quand la clôture franchit la bande haute de Keltner (EMA 20 + 2 ATR 10), sort sous l'EMA 20.",
          H4, Rule::KeltnerBreakout { ema: 20, atr: 10, mult: 2.0 }, trend_exits(), risk()),
        p("rsi_reversion_1h", "Rebond RSI 30 (1h, tendance haussière)", "Retour à la moyenne",
          "Entre quand le RSI(14) remonte au-dessus de 30 et que le prix est au-dessus de sa SMA 200 ; sort à RSI 55.",
          H1, Rule::RsiReversion { period: 14, oversold: 30.0, exit_level: 55.0 }, reversion_exits(48), risk()),
        p("rsi_reversion_4h", "Rebond RSI 30 (4h, tendance haussière)", "Retour à la moyenne",
          "Même règle RSI sur 4h.",
          H4, Rule::RsiReversion { period: 14, oversold: 30.0, exit_level: 55.0 }, reversion_exits(30), risk()),
        p("rsi2_reversion_1d", "RSI(2) de Connors (1j)", "Retour à la moyenne",
          "Entre quand le RSI(2) remonte au-dessus de 10 au-dessus de la SMA 200, sort à RSI(2) > 70.",
          D1, Rule::RsiReversion { period: 2, oversold: 10.0, exit_level: 70.0 }, reversion_exits(10), risk()),
        p("stoch_rsi_1h", "Stoch RSI (1h)", "Retour à la moyenne",
          "Entre quand %K croise %D vers le haut sous 20, sort au croisement inverse au-dessus de 80.",
          H1, Rule::StochRsi { oversold: 20.0, overbought: 80.0 }, reversion_exits(48), risk()),
        p("stoch_rsi_4h", "Stoch RSI (4h)", "Retour à la moyenne",
          "Même règle Stoch RSI sur 4h.",
          H4, Rule::StochRsi { oversold: 20.0, overbought: 80.0 }, reversion_exits(30), risk()),
        p("bollinger_reversion_1h", "Retour dans Bollinger (1h)", "Retour à la moyenne",
          "Entre quand la clôture repasse au-dessus de la bande basse (20, 2σ), sort à la moyenne mobile.",
          H1, Rule::BollingerReversion { period: 20, k: 2.0 }, reversion_exits(48), risk()),
        p("bollinger_reversion_4h", "Retour dans Bollinger (4h)", "Retour à la moyenne",
          "Même règle Bollinger sur 4h.",
          H4, Rule::BollingerReversion { period: 20, k: 2.0 }, reversion_exits(30), risk()),
        p("williams_r_1h", "Williams %R (1h)", "Retour à la moyenne",
          "Entre quand le %R(14) remonte au-dessus de −80, sort au-dessus de −20.",
          H1, Rule::WilliamsR { period: 14, oversold: -80.0, exit_level: -20.0 }, reversion_exits(48), risk()),
        p("cci_4h", "CCI −100/+100 (4h)", "Retour à la moyenne",
          "Entre quand le CCI(20) remonte au-dessus de −100, sort au-dessus de +100.",
          H4, Rule::CciReversion { period: 20, entry: -100.0, exit_level: 100.0 }, reversion_exits(30), risk()),
        p("vwap_reversion_1h", "Retour au VWAP 24h (1h)", "Retour à la moyenne",
          "Entre quand le prix passe 2 % sous le VWAP glissant de 24 bougies, sort quand il le retrouve.",
          H1, Rule::VwapReversion { period: 24, deviation_pct: 2.0 }, reversion_exits(48), risk()),
        p("fear_greed_1d", "Peur extrême (Fear & Greed, 1j)", "Sentiment",
          "Entre quand l'indice Fear & Greed publié passe sous 25, sort au-dessus de 75. Stop large à 20 %.",
          D1, Rule::FearGreed { buy_below: 25.0, sell_above: 75.0 },
          ExitPolicy { stop: Some(Distance::Percent(20.0)), take_profit: None, trailing: None, max_bars: None },
          Sizing::Fixed { position_pct: 20.0 }),
        p("tsmom_28_1d", "Momentum 28 jours (1j)", "Régime",
          "Investi à parts égales sur chaque symbole tant que sa clôture dépasse celle d'il y a 28 jours, en liquide sinon. Momentum en série temporelle de 1 à 4 semaines (Liu & Tsyvinski, 2021).",
          D1, Rule::Momentum { lookback: 28 }, regime_exits(), Sizing::EqualWeight),
        p("sma_trend_50_1d", "Au-dessus de la SMA 50 (1j)", "Régime",
          "Investi à parts égales sur chaque symbole tant que sa clôture est au-dessus de sa moyenne 50 jours, en liquide sinon (Detzel et al., 2021).",
          D1, Rule::SmaTrend { period: 50 }, regime_exits(), Sizing::EqualWeight),
    ];
    v.extend(variants());
    let more = grid(&v);
    v.extend(more);
    // Les retours à la moyenne n'achètent que dans une tendance de fond haussière.
    for preset in v.iter_mut() {
        if preset.family == "Retour à la moyenne" {
            preset.trend_sma = Some(200);
        }
    }
    let mut dip = p(
        "dip_buyer_4h",
        "Achat des replis, 3 couches max (4h)",
        "Renforcement borné",
        "Achète après une baisse de 8 % depuis le plus haut des 30 dernières bougies, renforce tous les 6 % de baisse (3 couches au plus), revend à +6 % du prix moyen ou au stop à 12 %.",
        Timeframe::H4,
        Rule::DipBuy { lookback: 30, dip_pct: 8.0 },
        ExitPolicy {
            stop: Some(Distance::Percent(12.0)),
            take_profit: Some(TakeProfit::Percent(6.0)),
            trailing: None,
            max_bars: Some(180),
        },
        Sizing::Fixed { position_pct: 8.0 },
    );
    dip.pyramid = Some(Pyramid { step_pct: 6.0, max_layers: 3 });
    v.push(dip);
    // Chaque stratégie scanne tout l'univers (jusqu'à une centaine de cryptos) mais
    // ne tient que 10 positions : les signaux simultanés les plus forts passent
    // d'abord, et une crypto nettement plus forte remplace la plus faible tenue.
    for preset in v.iter_mut().filter(|p| p.id != "buy_hold") {
        preset.max_positions = Some(MAX_POSITIONS);
        preset.switch_margin = Some(SWITCH_MARGIN);
    }
    v
}

/// Positions tenues au plus par une stratégie, quel que soit le nombre de cryptos scannées.
pub const MAX_POSITIONS: usize = 10;
/// Écart de force d'entrée (rendement sur 20 bougies, en ATR) exigé pour changer de crypto.
pub const SWITCH_MARGIN: f64 = 1.0;

/// Durée maximale d'un retour à la moyenne selon l'unité de temps (≈ 2 jours en 1h, 5 en 4h, 10 en 1j).
fn reversion_bars(tf: Timeframe) -> u32 {
    match tf {
        Timeframe::M15 => 96,
        Timeframe::M30 => 72,
        Timeframe::H1 => 48,
        Timeframe::H4 => 30,
        Timeframe::D1 => 10,
    }
}

fn tf_label(tf: Timeframe) -> &'static str {
    match tf {
        Timeframe::M15 => "15m",
        Timeframe::M30 => "30m",
        Timeframe::H1 => "1h",
        Timeframe::H4 => "4h",
        Timeframe::D1 => "1j",
    }
}

fn trend(id: String, name: String, family: &str, desc: String, tf: Timeframe, rule: Rule) -> Preset {
    p(&id, &name, family, &desc, tf, rule, trend_exits(), risk())
}

fn rev(id: String, name: String, desc: String, tf: Timeframe, rule: Rule) -> Preset {
    p(&id, &name, "Retour à la moyenne", &desc, tf, rule, reversion_exits(reversion_bars(tf)), risk())
}

/// Variantes : mêmes logiques, autres réglages ou autres unités de temps.
/// Chaque entrée diffère des autres par au moins un réglage qui change les
/// trades (le test `every_preset_has_distinct_settings` le vérifie) et son nom
/// dit ce réglage. Toutes sont inscrites au registre des essais.
fn variants() -> Vec<Preset> {
    use Timeframe::*;
    let mut v = Vec::new();
    for (fast, slow, tf) in [
        (5, 13, H1),
        (5, 13, H4),
        (9, 21, H4),
        (9, 21, D1),
        (12, 26, H1),
        (12, 26, D1),
        (20, 50, H1),
        (20, 50, D1),
        (21, 55, D1),
        (50, 200, H4),
    ] {
        v.push(trend(
            format!("ema_{fast}_{slow}_{}", tf.as_str()),
            format!("Croisement EMA {fast}/{slow} ({})", tf_label(tf)),
            "Tendance",
            format!("Entre quand l'EMA {fast} passe au-dessus de l'EMA {slow}, sort au croisement inverse ou au stop suiveur ATR."),
            tf, Rule::EmaCross { fast, slow },
        ));
    }
    for (fast, slow, signal, tf) in [(12, 26, 9, H1), (8, 17, 9, H4), (8, 17, 9, D1), (5, 35, 5, H1)] {
        v.push(trend(
            format!("macd_{fast}_{slow}_{signal}_{}", tf.as_str()),
            format!("Croisement MACD {fast}/{slow}/{signal} ({})", tf_label(tf)),
            "Tendance",
            format!("Entre quand la ligne MACD ({fast},{slow}) croise sa ligne de signal ({signal}) vers le haut, sort au croisement inverse."),
            tf, Rule::MacdCross { fast, slow, signal },
        ));
    }
    for (period, mult, tf) in [(7, 2.0, H4), (7, 2.0, D1), (10, 3.0, H1), (14, 4.0, H4), (14, 4.0, D1), (20, 5.0, D1)] {
        v.push(trend(
            format!("supertrend_{period}_{mult}_{}", tf.as_str()),
            format!("Supertrend {period}×{mult} ({})", tf_label(tf)),
            "Tendance",
            format!("Entre quand le Supertrend (ATR {period}, multiplicateur {mult}) passe haussier, sort quand il repasse baissier."),
            tf, Rule::Supertrend { period, mult },
        ));
    }
    for (tenkan, kijun, senkou, tf) in [(9, 26, 52, H1), (20, 60, 120, H4), (20, 60, 120, D1)] {
        v.push(trend(
            format!("ichimoku_{tenkan}_{kijun}_{senkou}_{}", tf.as_str()),
            format!("Ichimoku {tenkan}/{kijun}/{senkou} ({})", tf_label(tf)),
            "Tendance",
            format!("Entre au croisement Tenkan({tenkan})/Kijun({kijun}) vers le haut si le prix est au-dessus du nuage (Senkou {senkou}) ; sort au croisement inverse ou sous le nuage."),
            tf, Rule::Ichimoku { tenkan, kijun, senkou },
        ));
    }
    for (threshold, tf) in [(25.0, H1), (25.0, D1), (20.0, H4)] {
        v.push(trend(
            format!("adx_{threshold}_{}", tf.as_str()),
            format!("Tendance forte ADX > {threshold} ({})", tf_label(tf)),
            "Tendance",
            format!("Entre quand l'ADX(14) dépasse {threshold} avec +DI > −DI et le prix au-dessus de l'EMA 50 ; sort quand −DI repasse devant."),
            tf, Rule::AdxTrend { period: 14, threshold, ema: 50 },
        ));
    }
    for (min_score, exit_score, tf) in [(4u8, 2u8, H1), (5, 2, H4), (3, 1, D1)] {
        v.push(trend(
            format!("confluence_{min_score}_{exit_score}_{}", tf.as_str()),
            format!("Confluence {min_score}/5, sortie {exit_score}/5 ({})", tf_label(tf)),
            "Tendance",
            format!("Score sur 5 critères (EMA 50>200, MACD positif, RSI 45-70, Supertrend haussier, ADX>20 orienté) : entre à {min_score}/5, sort à {exit_score}/5 ou moins."),
            tf, Rule::Confluence { min_score, exit_score },
        ));
    }
    for (entry, exit, tf) in [(20, 10, H1), (20, 10, D1), (55, 20, H4), (10, 5, H4), (100, 50, D1)] {
        v.push(trend(
            format!("donchian_{entry}_{exit}_{}", tf.as_str()),
            format!("Cassure Donchian {entry}/{exit} ({})", tf_label(tf)),
            "Cassure",
            format!("Entre quand la clôture dépasse le plus haut des {entry} bougies précédentes, sort sous le plus bas des {exit} précédentes."),
            tf, Rule::DonchianBreakout { entry, exit },
        ));
    }
    for (mult, tf) in [(2.0, H1), (2.0, D1), (1.5, H4), (2.5, H4)] {
        v.push(trend(
            format!("keltner_{mult}_{}", tf.as_str()),
            format!("Cassure Keltner {mult} ATR ({})", tf_label(tf)),
            "Cassure",
            format!("Entre quand la clôture franchit la bande haute de Keltner (EMA 20 + {mult} ATR 10), sort sous l'EMA 20."),
            tf, Rule::KeltnerBreakout { ema: 20, atr: 10, mult },
        ));
    }
    for (period, oversold, exit_level, tf) in
        [(14, 30.0, 55.0, D1), (2, 10.0, 70.0, H1), (2, 10.0, 70.0, H4), (7, 30.0, 60.0, H1)]
    {
        v.push(rev(
            format!("rsi_{period}_{oversold}_{exit_level}_{}", tf.as_str()),
            format!("Rebond RSI({period}) {oversold} → {exit_level} ({}, tendance haussière)", tf_label(tf)),
            format!("Entre quand le RSI({period}) remonte au-dessus de {oversold} et que le prix est au-dessus de sa SMA 200 ; sort à RSI {exit_level}."),
            tf, Rule::RsiReversion { period, oversold, exit_level },
        ));
    }
    for (oversold, overbought, tf) in [(20.0, 80.0, D1), (10.0, 90.0, H4)] {
        v.push(rev(
            format!("stoch_rsi_{oversold}_{overbought}_{}", tf.as_str()),
            format!("Stoch RSI {oversold}/{overbought} ({})", tf_label(tf)),
            format!("Entre quand %K croise %D vers le haut sous {oversold}, sort au croisement inverse au-dessus de {overbought}."),
            tf, Rule::StochRsi { oversold, overbought },
        ));
    }
    for (k, tf) in [(2.0, D1), (2.5, H4), (2.5, H1)] {
        v.push(rev(
            format!("bollinger_{k}_{}", tf.as_str()),
            format!("Retour dans Bollinger {k}σ ({})", tf_label(tf)),
            format!("Entre quand la clôture repasse au-dessus de la bande basse (20, {k}σ), sort à la moyenne mobile."),
            tf,
            Rule::BollingerReversion { period: 20, k },
        ));
    }
    for (period, tf) in [(14, H4), (14, D1), (21, H1)] {
        v.push(rev(
            format!("williams_r_{period}_{}", tf.as_str()),
            format!("Williams %R({period}) ({})", tf_label(tf)),
            format!("Entre quand le %R({period}) remonte au-dessus de −80, sort au-dessus de −20."),
            tf,
            Rule::WilliamsR { period, oversold: -80.0, exit_level: -20.0 },
        ));
    }
    for (period, entry, exit_level, tf) in [(20, -100.0, 100.0, H1), (20, -100.0, 100.0, D1), (14, -200.0, 0.0, H4)] {
        v.push(rev(
            format!("cci_{period}_{entry}_{exit_level}_{}", tf.as_str()),
            format!("CCI({period}) {entry}/{exit_level} ({})", tf_label(tf)),
            format!("Entre quand le CCI({period}) remonte au-dessus de {entry}, sort au-dessus de {exit_level}."),
            tf,
            Rule::CciReversion { period, entry, exit_level },
        ));
    }
    for (period, deviation_pct, tf) in [(24, 3.0, H4), (48, 3.0, H1)] {
        v.push(rev(
            format!("vwap_{period}_{deviation_pct}_{}", tf.as_str()),
            format!("Retour au VWAP {period} bougies, écart {deviation_pct} % ({})", tf_label(tf)),
            format!("Entre quand le prix passe {deviation_pct} % sous le VWAP glissant de {period} bougies, sort quand il le retrouve."),
            tf, Rule::VwapReversion { period, deviation_pct },
        ));
    }
    for (lookback, tf) in [(7, D1), (14, D1), (56, D1), (90, D1), (42, H4), (180, H4)] {
        v.push(p(
            &format!("tsmom_{lookback}_{}", tf.as_str()),
            &format!("Momentum {lookback} bougies ({})", tf_label(tf)),
            "Régime",
            &format!("Investi à parts égales sur chaque symbole tant que sa clôture dépasse celle d'il y a {lookback} bougies, en liquide sinon."),
            tf, Rule::Momentum { lookback }, regime_exits(), Sizing::EqualWeight,
        ));
    }
    for (period, tf) in [(20, D1), (100, D1), (150, D1), (200, D1), (50, H4)] {
        v.push(p(
            &format!("sma_trend_{period}_{}", tf.as_str()),
            &format!("Au-dessus de la SMA {period} ({})", tf_label(tf)),
            "Régime",
            &format!("Investi à parts égales sur chaque symbole tant que sa clôture est au-dessus de sa moyenne {period} bougies, en liquide sinon."),
            tf, Rule::SmaTrend { period }, regime_exits(), Sizing::EqualWeight,
        ));
    }
    for (buy_below, sell_above) in [(20.0, 60.0), (30.0, 70.0)] {
        v.push(p(
            &format!("fear_greed_{buy_below}_{sell_above}_1d"),
            &format!("Peur < {buy_below}, sortie > {sell_above} (Fear & Greed, 1j)"),
            "Sentiment",
            &format!("Entre quand l'indice Fear & Greed publié passe sous {buy_below}, sort au-dessus de {sell_above}. Stop large à 20 %."),
            D1, Rule::FearGreed { buy_below, sell_above },
            ExitPolicy { stop: Some(Distance::Percent(20.0)), take_profit: None, trailing: None, max_bars: None },
            Sizing::Fixed { position_pct: 20.0 },
        ));
    }
    for (lookback, dip_pct, tf) in [(48, 5.0, H1), (20, 10.0, D1)] {
        let mut d = p(
            &format!("dip_buyer_{lookback}_{dip_pct}_{}", tf.as_str()),
            &format!("Achat des replis de {dip_pct} %, 3 couches max ({})", tf_label(tf)),
            "Renforcement borné",
            &format!("Achète après une baisse de {dip_pct} % depuis le plus haut des {lookback} dernières bougies, renforce tous les 6 % de baisse (3 couches au plus), revend à +6 % du prix moyen ou au stop à 12 %."),
            tf, Rule::DipBuy { lookback, dip_pct },
            ExitPolicy {
                stop: Some(Distance::Percent(12.0)),
                take_profit: Some(TakeProfit::Percent(6.0)),
                trailing: None,
                max_bars: Some(180),
            },
            Sizing::Fixed { position_pct: 8.0 },
        );
        d.pyramid = Some(Pyramid { step_pct: 6.0, max_layers: 3 });
        v.push(d);
    }
    v
}

/// Plus long préchauffage (en bougies) des stratégies du catalogue sur `tf`.
pub fn longest_warmup(tf: Timeframe) -> usize {
    let all = catalog();
    all.iter()
        .filter(|p| p.timeframe == tf)
        .map(Preset::warmup)
        .max()
        .unwrap_or_else(|| all.iter().map(Preset::warmup).max().unwrap_or(0))
}

pub fn find(id: &str) -> Option<Preset> {
    catalog().into_iter().find(|p| p.id == id)
}

/// Grille : les mêmes logiques sur chaque unité de temps, du 15m (intraday) au 1j.
/// Une combinaison déjà présente (même identifiant ou mêmes réglages) est sautée,
/// pour que chaque entrée reste un essai distinct.
fn grid(existing: &[Preset]) -> Vec<Preset> {
    use Timeframe::*;
    let mut ids: std::collections::HashSet<String> = existing.iter().map(|p| p.id.clone()).collect();
    let mut prints: std::collections::HashSet<String> = existing.iter().map(crate::essais::fingerprint).collect();
    let mut v = Vec::new();
    for tf in [M15, M30, H1, H4, D1] {
        let l = tf_label(tf);
        let s = tf.as_str();
        let mut g = Vec::new();
        for (fast, slow) in [(3, 10), (5, 13), (5, 20), (8, 21), (9, 21), (10, 30), (12, 26), (13, 34), (15, 45), (20, 50), (20, 100), (21, 55), (25, 75), (30, 100), (40, 120), (50, 200)] {
            g.push(trend(
                format!("ema_{fast}_{slow}_{s}"),
                format!("Croisement EMA {fast}/{slow} ({l})"),
                "Tendance",
                format!("Entre quand l'EMA {fast} passe au-dessus de l'EMA {slow}, sort au croisement inverse ou au stop suiveur ATR."),
                tf, Rule::EmaCross { fast, slow },
            ));
        }
        for (fast, slow, signal) in [(12, 26, 9), (8, 17, 9), (5, 35, 5), (19, 39, 9), (3, 10, 16), (24, 52, 9)] {
            g.push(trend(
                format!("macd_{fast}_{slow}_{signal}_{s}"),
                format!("Croisement MACD {fast}/{slow}/{signal} ({l})"),
                "Tendance",
                format!("Entre quand la ligne MACD ({fast},{slow}) croise sa ligne de signal ({signal}) vers le haut, sort au croisement inverse."),
                tf, Rule::MacdCross { fast, slow, signal },
            ));
        }
        for (period, mult) in [(7, 2.0), (7, 3.0), (10, 2.0), (10, 3.0), (10, 4.0), (14, 3.0), (14, 4.0), (20, 5.0)] {
            g.push(trend(
                format!("supertrend_{period}_{mult}_{s}"),
                format!("Supertrend {period}×{mult} ({l})"),
                "Tendance",
                format!("Entre quand le Supertrend (ATR {period}, multiplicateur {mult}) passe haussier, sort quand il repasse baissier."),
                tf, Rule::Supertrend { period, mult },
            ));
        }
        for (tenkan, kijun, senkou) in [(7, 22, 44), (9, 26, 52), (20, 60, 120)] {
            g.push(trend(
                format!("ichimoku_{tenkan}_{kijun}_{senkou}_{s}"),
                format!("Ichimoku {tenkan}/{kijun}/{senkou} ({l})"),
                "Tendance",
                format!("Entre au croisement Tenkan({tenkan})/Kijun({kijun}) vers le haut si le prix est au-dessus du nuage (Senkou {senkou}) ; sort au croisement inverse ou sous le nuage."),
                tf, Rule::Ichimoku { tenkan, kijun, senkou },
            ));
        }
        for threshold in [20.0, 25.0, 30.0, 35.0] {
            g.push(trend(
                format!("adx_{threshold}_{s}"),
                format!("Tendance forte ADX > {threshold} ({l})"),
                "Tendance",
                format!("Entre quand l'ADX(14) dépasse {threshold} avec +DI > −DI et le prix au-dessus de l'EMA 50 ; sort quand −DI repasse devant."),
                tf, Rule::AdxTrend { period: 14, threshold, ema: 50 },
            ));
        }
        for (min_score, exit_score) in [(3u8, 1u8), (4, 2), (5, 2)] {
            g.push(trend(
                format!("confluence_{min_score}_{exit_score}_{s}"),
                format!("Confluence {min_score}/5, sortie {exit_score}/5 ({l})"),
                "Tendance",
                format!("Score sur 5 critères (EMA 50>200, MACD positif, RSI 45-70, Supertrend haussier, ADX>20 orienté) : entre à {min_score}/5, sort à {exit_score}/5 ou moins."),
                tf, Rule::Confluence { min_score, exit_score },
            ));
        }
        for (entry, exit) in [(5, 3), (10, 5), (20, 10), (30, 15), (40, 20), (55, 20), (80, 40), (100, 50)] {
            g.push(trend(
                format!("donchian_{entry}_{exit}_{s}"),
                format!("Cassure Donchian {entry}/{exit} ({l})"),
                "Cassure",
                format!("Entre quand la clôture dépasse le plus haut des {entry} bougies précédentes, sort sous le plus bas des {exit} précédentes."),
                tf, Rule::DonchianBreakout { entry, exit },
            ));
        }
        for mult in [1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0] {
            g.push(trend(
                format!("keltner_{mult}_{s}"),
                format!("Cassure Keltner {mult} ATR ({l})"),
                "Cassure",
                format!("Entre quand la clôture franchit la bande haute de Keltner (EMA 20 + {mult} ATR 10), sort sous l'EMA 20."),
                tf, Rule::KeltnerBreakout { ema: 20, atr: 10, mult },
            ));
        }
        for (period, oversold, exit_level) in [(2, 10.0, 70.0), (2, 5.0, 60.0), (2, 15.0, 75.0), (3, 15.0, 65.0), (5, 25.0, 60.0), (7, 30.0, 60.0), (14, 30.0, 55.0), (14, 35.0, 60.0), (21, 35.0, 55.0)] {
            g.push(rev(
                format!("rsi_{period}_{oversold}_{exit_level}_{s}"),
                format!("Rebond RSI({period}) {oversold} → {exit_level} ({l}, tendance haussière)"),
                format!("Entre quand le RSI({period}) remonte au-dessus de {oversold} et que le prix est au-dessus de sa SMA 200 ; sort à RSI {exit_level}."),
                tf, Rule::RsiReversion { period, oversold, exit_level },
            ));
        }
        for (oversold, overbought) in [(20.0, 80.0), (10.0, 90.0), (15.0, 85.0), (30.0, 70.0)] {
            g.push(rev(
                format!("stoch_rsi_{oversold}_{overbought}_{s}"),
                format!("Stoch RSI {oversold}/{overbought} ({l})"),
                format!("Entre quand %K croise %D vers le haut sous {oversold}, sort au croisement inverse au-dessus de {overbought}."),
                tf, Rule::StochRsi { oversold, overbought },
            ));
        }
        for k in [1.5, 1.75, 2.0, 2.25, 2.5] {
            g.push(rev(
                format!("bollinger_{k}_{s}"),
                format!("Retour dans Bollinger {k}σ ({l})"),
                format!("Entre quand la clôture repasse au-dessus de la bande basse (20, {k}σ), sort à la moyenne mobile."),
                tf, Rule::BollingerReversion { period: 20, k },
            ));
        }
        for period in [7, 10, 14, 21, 28] {
            g.push(rev(
                format!("williams_r_{period}_{s}"),
                format!("Williams %R({period}) ({l})"),
                format!("Entre quand le %R({period}) remonte au-dessus de −80, sort au-dessus de −20."),
                tf, Rule::WilliamsR { period, oversold: -80.0, exit_level: -20.0 },
            ));
        }
        for (period, entry, exit_level) in [(20, -100.0, 100.0), (14, -200.0, 0.0), (30, -150.0, 50.0), (10, -150.0, 50.0), (40, -100.0, 0.0)] {
            g.push(rev(
                format!("cci_{period}_{entry}_{exit_level}_{s}"),
                format!("CCI({period}) {entry}/{exit_level} ({l})"),
                format!("Entre quand le CCI({period}) remonte au-dessus de {entry}, sort au-dessus de {exit_level}."),
                tf, Rule::CciReversion { period, entry, exit_level },
            ));
        }
        for (period, deviation_pct) in [(12, 1.5), (24, 2.0), (48, 2.0), (48, 3.0), (96, 2.0), (24, 1.5)] {
            g.push(rev(
                format!("vwap_{period}_{deviation_pct}_{s}"),
                format!("Retour au VWAP {period} bougies, écart {deviation_pct} % ({l})"),
                format!("Entre quand le prix passe {deviation_pct} % sous le VWAP glissant de {period} bougies, sort quand il le retrouve."),
                tf, Rule::VwapReversion { period, deviation_pct },
            ));
        }
        for lookback in [12, 24, 48, 96, 192] {
            g.push(p(
                &format!("tsmom_{lookback}_{s}"),
                &format!("Momentum {lookback} bougies ({l})"),
                "Régime",
                &format!("Investi à parts égales sur chaque symbole tant que sa clôture dépasse celle d'il y a {lookback} bougies, en liquide sinon."),
                tf, Rule::Momentum { lookback }, regime_exits(), Sizing::EqualWeight,
            ));
        }
        for period in [20, 50, 100, 150, 200] {
            g.push(p(
                &format!("sma_trend_{period}_{s}"),
                &format!("Au-dessus de la SMA {period} ({l})"),
                "Régime",
                &format!("Investi à parts égales sur chaque symbole tant que sa clôture est au-dessus de sa moyenne {period} bougies, en liquide sinon."),
                tf, Rule::SmaTrend { period }, regime_exits(), Sizing::EqualWeight,
            ));
        }
        for preset in g {
            let print = crate::essais::fingerprint(&preset);
            if !ids.contains(&preset.id) && !prints.contains(&print) {
                ids.insert(preset.id.clone());
                prints.insert(print);
                v.push(preset);
            }
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portfolio::CostModel;

    #[test]
    fn catalog_is_consistent() {
        let all = catalog();
        let rt = CostModel::default().round_trip();
        let mut ids = std::collections::HashSet::new();
        for p in &all {
            assert!(ids.insert(p.id.clone()), "identifiant en double : {}", p.id);
            p.validate(rt).unwrap();
            assert!(!p.description.is_empty());
        }
        assert!(all.iter().any(|p| p.is_benchmark()));
    }

    #[test]
    fn catalog_compares_at_least_100_strategies() {
        assert!(catalog().len() >= 100, "{} stratégies", catalog().len());
    }

    /// Demande : 500 à 1000 stratégies en tout, dont au moins 100 en intraday (15m, 30m).
    #[test]
    fn catalog_has_500_to_1000_strategies_with_100_intraday() {
        let all = catalog();
        assert!((500..=1000).contains(&all.len()), "{} stratégies", all.len());
        let intraday = all.iter().filter(|p| matches!(p.timeframe, Timeframe::M15 | Timeframe::M30)).count();
        assert!(intraday >= 100, "{intraday} stratégies en 15m/30m");
    }

    #[test]
    fn every_preset_has_distinct_settings() {
        let mut seen = std::collections::HashMap::new();
        for p in catalog() {
            let key = crate::essais::fingerprint(&p);
            if let Some(other) = seen.insert(key, p.id.clone()) {
                panic!("{} et {} ont exactement les mêmes réglages", other, p.id);
            }
        }
    }
}
