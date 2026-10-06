# Comparaison du catalogue — BTCUSDT, ETHUSDT, SOLUSDT, BNBUSDT, XRPUSDT

Frais 0.10 % par côté, glissement 2 pb. Résultats affichés sur 1095 jours. Verdict décidé par une validation sur 3650 jours : fenêtres de 180 jours décalées de 90 jours, chacune comparée à un achat au hasard de même exposition, test du signe sur fenêtres indépendantes, corrigé pour 33 stratégies essayées.

« Hasard à expo. égale » = `(1 + R)^f − 1` : ce qu'un achat au hasard, investi la même part du temps `f`, obtient en moyenne quand « acheter et garder » fait `R`.

| Stratégie | UT | Trades | Rendement | Acheter-garder | Expo. | Hasard à expo. égale | Pire baisse | Réf. | Sans frais | Fenêtres gagnées | p corrigé | Verdict |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| Cassure Keltner (4h) | 4h | 391 | +59.9 % | +226.9 % | 19 % | +24.6 % | 27.5 % | 63.9 % | +94.1 % | 13 / 17 | 80.9 % | **Prometteuse** |
| Acheter et garder | 1d | 0 | +230.9 % | +230.9 % | 100 % | +230.9 % | 63.1 % | 63.1 % | +231.4 % | — | — | **Référence** |
| Croisement MACD (1j) | 1d | 197 | +56.4 % | +230.9 % | 21 % | +28.1 % | 13.7 % | 63.1 % | +63.6 % | 12 / 17 | 100.0 % | **Indiscernable du hasard** |
| Ichimoku Tenkan/Kijun (4h) | 4h | 338 | +73.1 % | +226.9 % | 17 % | +22.2 % | 14.2 % | 63.9 % | +105.7 % | 11 / 17 | 100.0 % | **Indiscernable du hasard** |
| Cassure Donchian 20/10 (4h) | 4h | 555 | +59.2 % | +226.9 % | 28 % | +39.7 % | 38.8 % | 63.9 % | +112.2 % | 11 / 17 | 100.0 % | **Indiscernable du hasard** |
| Supertrend 10×3 (1j) | 1d | 65 | +23.5 % | +230.9 % | 10 % | +13.2 % | 12.7 % | 63.1 % | +25.6 % | 10 / 17 | 100.0 % | **Indiscernable du hasard** |
| Momentum 28 jours (1j) | 1d | 234 | +193.1 % | +230.9 % | 56 % | +95.4 % | 35.9 % | 63.1 % | +227.6 % | 10 / 18 | 100.0 % | **Indiscernable du hasard** |
| Croisement EMA 20/50 (4h) | 4h | 322 | +34.8 % | +226.9 % | 17 % | +23.0 % | 20.5 % | 63.9 % | +58.7 % | 10 / 18 | 100.0 % | **Indiscernable du hasard** |
| Au-dessus de la SMA 50 (1j) | 1d | 167 | +352.5 % | +230.9 % | 56 % | +96.1 % | 33.4 % | 63.1 % | +389.5 % | 9 / 17 | 100.0 % | **Indiscernable du hasard** |
| Cassure Donchian 55/20 (1j, « Turtle ») | 1d | 64 | +34.1 % | +230.9 % | 11 % | +13.6 % | 10.2 % | 63.1 % | +36.4 % | 9 / 17 | 100.0 % | **Indiscernable du hasard** |
| Supertrend 10×3 (4h) | 4h | 381 | +14.0 % | +226.9 % | 21 % | +28.8 % | 32.0 % | 63.9 % | +37.4 % | 9 / 18 | 100.0 % | **Perdante** |
| Confluence 4/5 (4h) | 4h | 668 | +17.4 % | +226.9 % | 31 % | +45.0 % | 34.3 % | 63.9 % | +62.5 % | 8 / 17 | 100.0 % | **Perdante** |
| Croisement EMA 12/26 (4h) | 4h | 569 | +7.3 % | +226.9 % | 27 % | +37.9 % | 41.3 % | 63.9 % | +46.8 % | 8 / 17 | 100.0 % | **Perdante** |
| Tendance forte ADX (4h) | 4h | 471 | -9.3 % | +226.9 % | 19 % | +25.4 % | 27.9 % | 63.9 % | +14.0 % | 8 / 17 | 100.0 % | **Perdante** |
| Rebond RSI 30 (4h, tendance haussière) | 4h | 52 | +2.6 % | +226.9 % | 2 % | +2.1 % | 8.5 % | 63.9 % | +5.3 % | 7 / 17 | 100.0 % | **Perdante** |
| Croisement MACD (4h) | 4h | 1277 | +5.0 % | +226.9 % | 44 % | +67.6 % | 39.6 % | 63.9 % | +92.0 % | 7 / 17 | 100.0 % | **Perdante** |
| Confluence 4/5 (1j) | 1d | 110 | +17.9 % | +230.9 % | 14 % | +18.1 % | 17.5 % | 63.1 % | +21.1 % | 6 / 15 | 100.0 % | **Perdante** |
| Ichimoku Tenkan/Kijun (1j) | 1d | 50 | +3.7 % | +230.9 % | 7 % | +8.6 % | 9.1 % | 63.1 % | +5.0 % | 6 / 16 | 100.0 % | **Perdante** |
| Peur extrême (Fear & Greed, 1j) | 1d | 35 | -9.1 % | +230.9 % | 40 % | +61.3 % | 56.3 % | 63.1 % | -7.7 % | 5 / 14 | 100.0 % | **Perdante** |
| RSI(2) de Connors (1j) | 1d | 119 | +2.3 % | +230.9 % | 3 % | +4.1 % | 8.2 % | 63.1 % | +5.3 % | 4 / 14 | 100.0 % | **Perdante** |
| Stoch RSI (4h) | 4h | 628 | -29.1 % | +226.9 % | 22 % | +29.5 % | 47.9 % | 63.9 % | -1.4 % | 5 / 17 | 100.0 % | **Perdante** |
| Achat des replis, 3 couches max (4h) | 4h | 271 | +3.7 % | +226.9 % | 19 % | +25.3 % | 29.4 % | 63.9 % | +12.2 % | 5 / 18 | 100.0 % | **Perdante** |
| Croix dorée EMA 50/200 (1j) | 1d | 13 | +11.1 % | +230.9 % | 2 % | +2.5 % | 3.7 % | 63.1 % | +11.5 % | 2 / 10 | 100.0 % | **Perdante** |
| Retour dans Bollinger (4h) | 4h | 276 | -9.0 % | +226.9 % | 6 % | +6.8 % | 26.1 % | 63.9 % | +4.8 % | 4 / 17 | 100.0 % | **Perdante** |
| CCI −100/+100 (4h) | 4h | 430 | -11.4 % | +226.9 % | 13 % | +16.5 % | 28.5 % | 63.9 % | +12.0 % | 2 / 17 | 100.0 % | **Perdante** |
| Retour au VWAP 24h (1h) | 1h | 498 | -25.5 % | +228.7 % | 5 % | +5.7 % | 34.9 % | 64.8 % | -0.2 % | 2 / 18 | 100.0 % | **Perdante** |
| Croisement EMA 9/21 (1h) | 1h | 2759 | -46.1 % | +228.7 % | 37 % | +55.4 % | 61.0 % | 64.8 % | +166.5 % | 2 / 18 | 100.0 % | **Perdante** |
| Rebond RSI 30 (1h, tendance haussière) | 1h | 176 | -10.0 % | +228.7 % | 2 % | +2.5 % | 13.6 % | 64.8 % | +0.7 % | 1 / 17 | 100.0 % | **Perdante** |
| Supertrend 7×2 (1h) | 1h | 2490 | -40.4 % | +228.7 % | 39 % | +59.8 % | 59.1 % | 64.8 % | +150.1 % | 1 / 18 | 100.0 % | **Perdante** |
| Retour dans Bollinger (1h) | 1h | 1051 | -45.8 % | +228.7 % | 7 % | +8.3 % | 47.9 % | 64.8 % | +1.0 % | 0 / 17 | 100.0 % | **Perdante** |
| Williams %R (1h) | 1h | 2127 | -82.3 % | +228.7 % | 17 % | +22.4 % | 83.6 % | 64.8 % | -35.6 % | 0 / 17 | 100.0 % | **Perdante** |
| Stoch RSI (1h) | 1h | 2277 | -83.6 % | +228.7 % | 24 % | +33.4 % | 85.2 % | 64.8 % | -39.8 % | 0 / 17 | 100.0 % | **Perdante** |
