# Sélection glissante — BTCUSDT, ETHUSDT, SOLUSDT, BNBUSDT, XRPUSDT

Frais 0.10 % par côté, glissement 2 pb, historique demandé : 3650 jours. Les 29 stratégies du catalogue (la référence exclue) sont rejouées sur la même grille de fenêtres ; chaque fenêtre repart de zéro et se compare à un achat au hasard de même exposition, `(1 + R)^f − 1`. Pour chaque fenêtre, on joue la stratégie qui a le mieux battu ce hasard, en moyenne, sur les dernières fenêtres TERMINÉES avant son début. Variantes de sélection comptées dans la correction : 1 (registre des essais ∪ variantes demandées).

## Choix sur les 2 dernière(s) fenêtre(s) terminée(s)

Verdict : Compatible avec la chance

La stratégie choisie bat la référence à exposition égale dans 6 fenêtre(s) indépendante(s) sur 16. À pile ou face, on ferait au moins aussi bien avec une probabilité de 89.5 %, au-dessus du seuil de 5 %.

- 33 fenêtre(s) jouée(s) sur 36 ; 29 stratégies candidates ; fenêtres de 180 jours décalées de 90 jours. Le choix d'une fenêtre ne regarde que les fenêtres terminées avant son début.
- Test du signe sur une fenêtre sur 2 (découpage le moins favorable) : 6 gagnée(s), 10 perdue(s), 2 nulle(s) ou non jouée(s). Probabilité à pile ou face : 0.8949 ; corrigée pour 1 variante(s) de sélection essayée(s) : 0.8949.
- Écart médian avec un achat au hasard de même exposition : +0.2 points par fenêtre jouée.
- Les 16 fenêtres jouées de ce découpage, mises bout à bout (elles ne se chevauchent pas) : sélection +49.2 %, achat au hasard de même exposition +176.9 %, « acheter et garder » +4519.3 %.
- Stratégies choisies : donchian_20_10_4h ×6, ichimoku_4h ×5, macd_1d ×4, macd_4h ×4, keltner_breakout_4h ×3, stoch_rsi_4h ×3, ema_12_26_4h ×2, dip_buyer_4h ×1, ema_50_200_1d ×1, ema_9_21_1h ×1, fear_greed_1d ×1, rsi2_reversion_1d ×1, supertrend_10_3_1d ×1.

| Début | Fin | Stratégie choisie | Écart passé | Expo. | Rendement | Acheter-garder | Hasard à expo. égale | Écart | Résultat |
|---|---|---|---:|---:|---:|---:|---:|---:|---|
| 2017-08-20 | 2018-02-15 | — (pas assez de fenêtres terminées) | | | | | | | non jouée |
| 2017-11-18 | 2018-05-16 | — (pas assez de fenêtres terminées) | | | | | | | non jouée |
| 2018-02-16 | 2018-08-14 | — (pas assez de fenêtres terminées) | | | | | | | non jouée |
| 2018-05-17 | 2018-11-12 | Croisement EMA 9/21 (1h) | -7.1 | 33 % | -32.6 % | -35.4 % | -13.4 % | -19.1 | perdue |
| 2018-08-15 | 2019-02-10 | Cassure Keltner (4h) | +14.1 | 9 % | +7.9 % | -24.8 % | -2.6 % | +10.4 | gagnée |
| 2018-11-13 | 2019-05-11 | Croisement MACD (4h) | +15.0 | 30 % | +28.2 % | +19.5 % | +5.4 % | +22.8 | gagnée |
| 2019-02-11 | 2019-08-09 | Croisement MACD (4h) | +18.1 | 30 % | +23.9 % | +129.8 % | +28.1 % | -4.2 | perdue |
| 2019-05-12 | 2019-11-07 | Cassure Donchian 20/10 (4h) | +27.8 | 19 % | +10.3 % | +3.5 % | +0.6 % | +9.6 | gagnée |
| 2019-08-10 | 2020-02-05 | Cassure Donchian 20/10 (4h) | +35.1 | 20 % | +2.5 % | -16.5 % | -3.5 % | +6.0 | gagnée |
| 2019-11-08 | 2020-05-05 | Cassure Donchian 20/10 (4h) | +19.7 | 21 % | +24.3 % | -9.0 % | -1.9 % | +26.2 | gagnée |
| 2020-02-06 | 2020-08-03 | Stoch RSI (4h) | +13.8 | 18 % | +11.7 % | +31.9 % | +5.2 % | +6.5 | gagnée |
| 2020-05-06 | 2020-11-01 | Stoch RSI (4h) | +18.8 | 21 % | +10.8 % | +56.2 % | +10.0 % | +0.8 | gagnée |
| 2020-08-04 | 2021-01-30 | Cassure Donchian 20/10 (4h) | +26.7 | 23 % | +13.6 % | +150.4 % | +23.9 % | -10.2 | perdue |
| 2020-11-02 | 2021-04-30 | Cassure Keltner (4h) | +16.9 | 16 % | +51.7 % | +1226.6 % | +50.8 % | +0.9 | gagnée |
| 2021-01-31 | 2021-07-29 | Croisement EMA 12/26 (4h) | +10.6 | 15 % | +1.2 % | +282.4 % | +22.8 % | -21.6 | perdue |
| 2021-05-01 | 2021-10-27 | Croisement MACD (1j) | +25.9 | 13 % | +20.7 % | +47.5 % | +5.3 % | +15.5 | gagnée |
| 2021-07-30 | 2022-01-25 | Croisement MACD (1j) | +36.2 | 9 % | -4.2 % | +36.1 % | +2.9 % | -7.0 | perdue |
| 2021-10-28 | 2022-04-25 | Croisement MACD (1j) | +22.8 | 12 % | -4.3 % | -32.8 % | -4.5 % | +0.2 | gagnée |
| 2022-01-26 | 2022-07-24 | Supertrend 10×3 (1j) | +20.3 | 4 % | -3.2 % | -40.4 % | -2.2 % | -1.0 | perdue |
| 2022-04-26 | 2022-10-22 | Ichimoku Tenkan/Kijun (4h) | +15.3 | 9 % | -5.8 % | -49.5 % | -6.2 % | +0.4 | gagnée |
| 2022-07-25 | 2023-01-20 | Ichimoku Tenkan/Kijun (4h) | +21.3 | 17 % | +5.1 % | +3.7 % | +0.6 % | +4.5 | gagnée |
| 2022-10-23 | 2023-04-20 | Croisement MACD (4h) | +12.2 | 40 % | +6.9 % | +18.7 % | +7.1 % | -0.2 | perdue |
| 2023-01-21 | 2023-07-19 | Croisement MACD (4h) | +13.8 | 38 % | -10.7 % | +26.4 % | +9.4 % | -20.1 | perdue |
| 2023-04-21 | 2023-10-17 | Croisement MACD (1j) | +14.1 | 17 % | -4.8 % | -4.8 % | -0.8 % | -4.0 | perdue |
| 2023-07-20 | 2024-01-15 | Stoch RSI (4h) | +7.1 | 23 % | -0.8 % | +66.1 % | +12.4 % | -13.2 | perdue |
| 2023-10-18 | 2024-04-14 | Achat des replis, 3 couches max (4h) | +5.6 | 15 % | +24.4 % | +185.5 % | +17.3 % | +7.1 | gagnée |
| 2024-01-16 | 2024-07-13 | Croix dorée EMA 50/200 (1j) | +3.3 | 0 % | -1.0 % | +33.0 % | +0.0 % | -1.0 | perdue |
| 2024-04-15 | 2024-10-11 | Cassure Donchian 20/10 (4h) | +8.1 | 29 % | +8.3 % | -3.9 % | -1.2 % | +9.5 | gagnée |
| 2024-07-14 | 2025-01-09 | Cassure Donchian 20/10 (4h) | +6.0 | 34 % | +26.4 % | +89.9 % | +24.5 % | +1.8 | gagnée |
| 2024-10-12 | 2025-04-09 | Croisement EMA 12/26 (4h) | +15.4 | 25 % | -6.3 % | +53.0 % | +11.1 % | -17.4 | perdue |
| 2025-01-10 | 2025-07-08 | Ichimoku Tenkan/Kijun (4h) | +5.7 | 14 % | -5.1 % | -5.3 % | -0.8 % | -4.3 | perdue |
| 2025-04-10 | 2025-10-06 | Ichimoku Tenkan/Kijun (4h) | +4.4 | 21 % | +21.1 % | +100.7 % | +15.9 % | +5.2 | gagnée |
| 2025-07-09 | 2026-01-04 | RSI(2) de Connors (1j) | +3.5 | 4 % | +1.0 % | +0.3 % | +0.0 % | +1.0 | gagnée |
| 2025-10-07 | 2026-04-04 | Peur extrême (Fear & Greed, 1j) | +8.6 | 65 % | -47.7 % | -54.0 % | -39.4 % | -8.3 | perdue |
| 2026-01-05 | 2026-07-03 | Cassure Keltner (4h) | +18.5 | 14 % | -18.1 % | -40.1 % | -6.7 % | -11.4 | perdue |
| 2026-04-05 | 2026-10-01 | Ichimoku Tenkan/Kijun (4h) | +10.3 | 16 % | -2.8 % | +29.6 % | +4.1 % | -6.9 | perdue |

