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

fn risk() -> Sizing {
    Sizing::Risk { risk_pct: 1.0, max_position_pct: 25.0 }
}

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
    ];
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
    v
}

pub fn find(id: &str) -> Option<Preset> {
    catalog().into_iter().find(|p| p.id == id)
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
}
