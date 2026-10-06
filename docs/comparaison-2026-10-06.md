# Comparaison du catalogue — BTCUSDT, ETHUSDT, SOLUSDT, BNBUSDT, XRPUSDT

Frais 0.10 % par côté, glissement 2 pb. Résultats affichés sur 1095 jours. Verdict décidé par une validation sur 3650 jours : fenêtres de 180 jours décalées de 90 jours, chacune comparée à un achat au hasard de même exposition, test du signe sur fenêtres indépendantes, corrigé pour 29 stratégies essayées.

« Hasard à expo. égale » = `(1 + R)^f − 1` : ce qu'un achat au hasard, investi la même part du temps `f`, obtient en moyenne quand « acheter et garder » fait `R`.

| Stratégie | UT | Trades | Rendement | Acheter-garder | Expo. | Hasard à expo. égale | Pire baisse | Réf. | Sans frais | Fenêtres gagnées | p corrigé | Verdict |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| Cassure Keltner (4h) | 4h | 387 | +59.0 % | +229.4 % | 19 % | +24.8 % | 27.5 % | 63.9 % | +92.5 % | 13 / 17 | 71.1 % | **Prometteuse** |
| Acheter et garder | 1d | 0 | +129.7 % | +129.7 % | 100 % | +129.7 % | 61.4 % | 61.4 % | +130.0 % | — | — | **Référence** |
| Croisement MACD (1j) | 1d | 180 | +35.3 % | +94.1 % | 20 % | +14.5 % | 13.7 % | 61.3 % | +40.9 % | 12 / 17 | 100.0 % | **Indiscernable du hasard** |
| Ichimoku Tenkan/Kijun (4h) | 4h | 323 | +53.3 % | +120.7 % | 17 % | +14.0 % | 14.2 % | 62.3 % | +80.8 % | 11 / 17 | 100.0 % | **Indiscernable du hasard** |
| Cassure Donchian 20/10 (4h) | 4h | 551 | +57.2 % | +229.4 % | 28 % | +40.4 % | 38.8 % | 63.9 % | +109.0 % | 11 / 17 | 100.0 % | **Indiscernable du hasard** |
| Supertrend 10×3 (1j) | 1d | 63 | +21.6 % | +129.7 % | 10 % | +8.8 % | 12.7 % | 61.4 % | +23.5 % | 10 / 17 | 100.0 % | **Indiscernable du hasard** |
| Croisement EMA 20/50 (4h) | 4h | 317 | +24.5 % | +158.9 % | 17 % | +17.8 % | 20.5 % | 62.9 % | +46.1 % | 10 / 18 | 100.0 % | **Indiscernable du hasard** |
| Cassure Donchian 55/20 (1j, « Turtle ») | 1d | 43 | +14.7 % | +30.5 % | 9 % | +2.5 % | 8.9 % | 61.7 % | +16.0 % | 9 / 17 | 100.0 % | **Indiscernable du hasard** |
| Supertrend 10×3 (4h) | 4h | 381 | +14.0 % | +244.7 % | 22 % | +30.5 % | 32.0 % | 64.0 % | +37.4 % | 9 / 18 | 100.0 % | **Perdante** |
| Confluence 4/5 (4h) | 4h | 598 | +7.2 % | +89.7 % | 31 % | +21.7 % | 34.3 % | 62.1 % | +43.3 % | 8 / 17 | 100.0 % | **Perdante** |
| Tendance forte ADX (4h) | 4h | 459 | -12.3 % | +158.9 % | 19 % | +19.7 % | 27.9 % | 62.9 % | +9.4 % | 8 / 17 | 100.0 % | **Perdante** |
| Croisement EMA 12/26 (4h) | 4h | 563 | +2.6 % | +211.1 % | 27 % | +36.0 % | 41.3 % | 63.6 % | +39.8 % | 8 / 17 | 100.0 % | **Perdante** |
| Rebond RSI 30 (4h, tendance haussière) | 4h | 43 | -2.4 % | +89.7 % | 2 % | +1.0 % | 8.5 % | 62.1 % | -0.3 % | 7 / 17 | 100.0 % | **Perdante** |
| Croisement MACD (4h) | 4h | 1259 | -4.5 % | +180.5 % | 43 % | +56.6 % | 39.6 % | 63.6 % | +73.0 % | 7 / 17 | 100.0 % | **Perdante** |
| Confluence 4/5 (1j) | 1d | 42 | -0.2 % | -8.6 % | 11 % | -1.0 % | 14.8 % | 61.9 % | +0.9 % | 6 / 15 | 100.0 % | **Perdante** |
| Ichimoku Tenkan/Kijun (1j) | 1d | 40 | -4.1 % | +37.9 % | 6 % | +2.1 % | 7.3 % | 62.2 % | -3.1 % | 6 / 16 | 100.0 % | **Perdante** |
| Peur extrême (Fear & Greed, 1j) | 1d | 35 | -9.1 % | +129.7 % | 42 % | +41.4 % | 56.3 % | 61.4 % | -7.7 % | 5 / 14 | 100.0 % | **Perdante** |
| RSI(2) de Connors (1j) | 1d | 38 | +1.0 % | -8.6 % | 2 % | -0.2 % | 3.4 % | 61.9 % | +2.1 % | 4 / 14 | 100.0 % | **Perdante** |
| Stoch RSI (4h) | 4h | 535 | -38.2 % | +89.7 % | 21 % | +14.3 % | 48.5 % | 62.1 % | -19.0 % | 5 / 17 | 100.0 % | **Perdante** |
| Achat des replis, 3 couches max (4h) | 4h | 269 | +2.8 % | +205.5 % | 19 % | +23.9 % | 29.4 % | 63.5 % | +11.2 % | 5 / 18 | 100.0 % | **Perdante** |
| Croix dorée EMA 50/200 (1j) | 1d | 3 | +3.8 % | -8.6 % | 3 % | -0.2 % | 3.7 % | 61.9 % | +3.9 % | 2 / 10 | 100.0 % | **Perdante** |
| Retour dans Bollinger (4h) | 4h | 237 | -16.5 % | +89.7 % | 5 % | +3.5 % | 26.1 % | 62.1 % | -5.7 % | 4 / 17 | 100.0 % | **Perdante** |
| CCI −100/+100 (4h) | 4h | 371 | -15.7 % | +89.7 % | 12 % | +8.3 % | 28.5 % | 62.1 % | +2.4 % | 2 / 17 | 100.0 % | **Perdante** |
| Retour au VWAP 24h (1h) | 1h | 488 | -29.2 % | +160.2 % | 5 % | +4.5 % | 34.9 % | 64.0 % | -5.7 % | 2 / 18 | 100.0 % | **Perdante** |
| Croisement EMA 9/21 (1h) | 1h | 2750 | -44.9 % | +238.7 % | 37 % | +57.2 % | 61.0 % | 64.9 % | +170.9 % | 2 / 18 | 100.0 % | **Perdante** |
| Rebond RSI 30 (1h, tendance haussière) | 1h | 175 | -10.2 % | +160.2 % | 2 % | +2.0 % | 13.6 % | 64.0 % | +0.4 % | 1 / 17 | 100.0 % | **Perdante** |
| Supertrend 7×2 (1h) | 1h | 2485 | -39.6 % | +233.0 % | 39 % | +60.6 % | 59.1 % | 64.9 % | +152.7 % | 1 / 18 | 100.0 % | **Perdante** |
| Retour dans Bollinger (1h) | 1h | 1020 | -46.7 % | +160.2 % | 7 % | +6.6 % | 47.9 % | 64.0 % | -2.5 % | 0 / 17 | 100.0 % | **Perdante** |
| Williams %R (1h) | 1h | 2071 | -83.1 % | +160.2 % | 17 % | +17.5 % | 83.6 % | 64.0 % | -40.7 % | 0 / 17 | 100.0 % | **Perdante** |
| Stoch RSI (1h) | 1h | 2199 | -83.7 % | +160.2 % | 24 % | +25.9 % | 84.6 % | 64.0 % | -41.9 % | 0 / 17 | 100.0 % | **Perdante** |
