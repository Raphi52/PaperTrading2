# Sélection glissante — BTCUSDT, ETHUSDT, SOLUSDT, BNBUSDT, XRPUSDT

Frais 0.10 % par côté, glissement 2 pb, historique demandé : 3650 jours. Les 31 stratégies du catalogue (la référence exclue) sont rejouées sur la même grille de fenêtres ; chaque fenêtre repart de zéro et se compare à un achat au hasard de même exposition, `(1 + R)^f − 1`. Pour chaque fenêtre, on joue la stratégie qui a le mieux battu ce hasard, en moyenne, sur les dernières fenêtres TERMINÉES avant son début. Variantes de sélection comptées dans la correction : 1 (registre des essais ∪ variantes demandées).

## Choix sur les 2 dernière(s) fenêtre(s) terminée(s)

Verdict : Compatible avec la chance

La stratégie choisie bat la référence à exposition égale dans 6 fenêtre(s) indépendante(s) sur 17. À pile ou face, on ferait au moins aussi bien avec une probabilité de 92.8 %, au-dessus du seuil de 5 %.

- 33 fenêtre(s) jouée(s) sur 36 ; 31 stratégies candidates ; fenêtres de 180 jours décalées de 90 jours. Le choix d'une fenêtre ne regarde que les fenêtres terminées avant son début.
- Test du signe sur une fenêtre sur 2 (découpage le moins favorable) : 6 gagnée(s), 11 perdue(s), 1 nulle(s) ou non jouée(s). Probabilité à pile ou face : 0.9283 ; corrigée pour 1 variante(s) de sélection essayée(s) : 0.9283.
- Écart médian avec un achat au hasard de même exposition : -4.0 points par fenêtre jouée.
- Les 17 fenêtres jouées de ce découpage, mises bout à bout (elles ne se chevauchent pas) : sélection +6.1 %, achat au hasard de même exposition +159.7 %, « acheter et garder » +4784.4 %.
- Stratégies choisies : tsmom_28_1d ×11, sma_trend_50_1d ×8, ichimoku_4h ×3, keltner_breakout_4h ×2, macd_4h ×2, dip_buyer_4h ×1, donchian_20_10_4h ×1, ema_12_26_4h ×1, ema_9_21_1h ×1, fear_greed_1d ×1, macd_1d ×1, stoch_rsi_4h ×1.

| Début | Fin | Stratégie choisie | Écart passé | Expo. | Rendement | Acheter-garder | Hasard à expo. égale | Écart | Résultat |
|---|---|---|---:|---:|---:|---:|---:|---:|---|
| 2017-08-20 | 2018-02-15 | — (pas assez de fenêtres terminées) | | | | | | | non jouée |
| 2017-11-18 | 2018-05-16 | — (pas assez de fenêtres terminées) | | | | | | | non jouée |
| 2018-02-16 | 2018-08-14 | — (pas assez de fenêtres terminées) | | | | | | | non jouée |
| 2018-05-17 | 2018-11-12 | Croisement EMA 9/21 (1h) | -7.1 | 33 % | -32.6 % | -35.4 % | -13.4 % | -19.1 | perdue |
| 2018-08-15 | 2019-02-10 | Momentum 28 jours (1j) | +22.0 | 33 % | -9.1 % | -22.5 % | -8.1 % | -1.1 | perdue |
| 2018-11-13 | 2019-05-11 | Croisement MACD (4h) | +15.0 | 30 % | +28.2 % | +19.5 % | +5.4 % | +22.8 | gagnée |
| 2019-02-11 | 2019-08-09 | Croisement MACD (4h) | +18.1 | 30 % | +23.9 % | +129.8 % | +28.1 % | -4.2 | perdue |
| 2019-05-12 | 2019-11-07 | Momentum 28 jours (1j) | +29.3 | 51 % | +2.9 % | +5.6 % | +2.8 % | +0.2 | gagnée |
| 2019-08-10 | 2020-02-05 | Momentum 28 jours (1j) | +39.2 | 38 % | -14.6 % | -14.7 % | -5.9 % | -8.7 | perdue |
| 2019-11-08 | 2020-05-05 | Cassure Donchian 20/10 (4h) | +19.7 | 21 % | +24.3 % | -9.0 % | -1.9 % | +26.2 | gagnée |
| 2020-02-06 | 2020-08-03 | Au-dessus de la SMA 50 (1j) | +16.8 | 62 % | +36.1 % | +27.8 % | +16.3 % | +19.7 | gagnée |
| 2020-05-06 | 2020-11-01 | Au-dessus de la SMA 50 (1j) | +23.7 | 70 % | +22.8 % | +59.2 % | +38.7 % | -15.9 | perdue |
| 2020-08-04 | 2021-01-30 | Au-dessus de la SMA 50 (1j) | +27.1 | 76 % | +115.7 % | +151.6 % | +101.5 % | +14.2 | gagnée |
| 2020-11-02 | 2021-04-30 | Cassure Keltner (4h) | +16.9 | 16 % | +51.7 % | +1226.6 % | +50.8 % | +0.9 | gagnée |
| 2021-01-31 | 2021-07-29 | Momentum 28 jours (1j) | +21.2 | 61 % | +270.8 % | +280.0 % | +125.1 % | +145.7 | gagnée |
| 2021-05-01 | 2021-10-27 | Momentum 28 jours (1j) | +176.1 | 54 % | +10.3 % | +47.5 % | +23.5 % | -13.2 | perdue |
| 2021-07-30 | 2022-01-25 | Momentum 28 jours (1j) | +222.6 | 58 % | +96.0 % | +36.1 % | +19.7 % | +76.3 | gagnée |
| 2021-10-28 | 2022-04-25 | Au-dessus de la SMA 50 (1j) | +129.7 | 39 % | -20.6 % | -32.8 % | -14.5 % | -6.0 | perdue |
| 2022-01-26 | 2022-07-24 | Au-dessus de la SMA 50 (1j) | +56.8 | 26 % | -15.3 % | -40.4 % | -12.7 % | -2.5 | perdue |
| 2022-04-26 | 2022-10-22 | Au-dessus de la SMA 50 (1j) | +44.0 | 27 % | -12.3 % | -46.3 % | -15.5 % | +3.2 | gagnée |
| 2022-07-25 | 2023-01-20 | Ichimoku Tenkan/Kijun (4h) | +21.3 | 17 % | +5.1 % | +3.7 % | +0.6 % | +4.5 | gagnée |
| 2022-10-23 | 2023-04-20 | Momentum 28 jours (1j) | +19.0 | 55 % | +6.4 % | +15.7 % | +8.4 % | -2.0 | perdue |
| 2023-01-21 | 2023-07-19 | Momentum 28 jours (1j) | +24.9 | 57 % | -3.7 % | +27.5 % | +14.7 % | -18.4 | perdue |
| 2023-04-21 | 2023-10-17 | Croisement MACD (1j) | +14.1 | 17 % | -4.8 % | -4.8 % | -0.8 % | -4.0 | perdue |
| 2023-07-20 | 2024-01-15 | Stoch RSI (4h) | +7.1 | 23 % | -0.8 % | +66.1 % | +12.4 % | -13.2 | perdue |
| 2023-10-18 | 2024-04-14 | Achat des replis, 3 couches max (4h) | +5.6 | 15 % | +24.4 % | +185.5 % | +17.3 % | +7.1 | gagnée |
| 2024-01-16 | 2024-07-13 | Momentum 28 jours (1j) | +18.2 | 53 % | +35.4 % | +33.0 % | +16.4 % | +19.0 | gagnée |
| 2024-04-15 | 2024-10-11 | Momentum 28 jours (1j) | +30.8 | 43 % | -28.4 % | -1.2 % | -0.5 % | -27.9 | perdue |
| 2024-07-14 | 2025-01-09 | Momentum 28 jours (1j) | +20.5 | 63 % | +20.5 % | +87.5 % | +48.3 % | -27.9 | perdue |
| 2024-10-12 | 2025-04-09 | Croisement EMA 12/26 (4h) | +15.4 | 25 % | -6.3 % | +53.0 % | +11.1 % | -17.4 | perdue |
| 2025-01-10 | 2025-07-08 | Ichimoku Tenkan/Kijun (4h) | +5.7 | 14 % | -5.1 % | -5.3 % | -0.8 % | -4.3 | perdue |
| 2025-04-10 | 2025-10-06 | Au-dessus de la SMA 50 (1j) | +7.7 | 75 % | +55.9 % | +106.5 % | +72.2 % | -16.2 | perdue |
| 2025-07-09 | 2026-01-04 | Au-dessus de la SMA 50 (1j) | +13.8 | 48 % | +22.6 % | +0.3 % | +0.2 % | +22.5 | gagnée |
| 2025-10-07 | 2026-04-04 | Peur extrême (Fear & Greed, 1j) | +8.6 | 65 % | -47.7 % | -54.0 % | -39.4 % | -8.3 | perdue |
| 2026-01-05 | 2026-07-03 | Cassure Keltner (4h) | +18.5 | 14 % | -18.1 % | -40.1 % | -6.7 % | -11.4 | perdue |
| 2026-04-05 | 2026-10-01 | Ichimoku Tenkan/Kijun (4h) | +10.3 | 16 % | -2.8 % | +29.6 % | +4.1 % | -6.9 | perdue |

