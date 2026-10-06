# Comparaison du catalogue — BTCUSDT, ETHUSDT, SOLUSDT, BNBUSDT, XRPUSDT

Frais 0.10 % par côté, glissement 2 pb. Résultats affichés sur 1095 jours. Verdict décidé par une validation sur 3650 jours : fenêtres de 180 jours décalées de 90 jours, chacune comparée à un achat au hasard de même exposition, test du signe sur fenêtres indépendantes, corrigé pour 29 stratégies essayées.

« Hasard à expo. égale » = `(1 + R)^f − 1` : ce qu'un achat au hasard, investi la même part du temps `f`, obtient en moyenne quand « acheter et garder » fait `R`.

| Stratégie | UT | Trades | Rendement | Acheter-garder | Expo. | Hasard à expo. égale | Pire baisse | Réf. | Sans frais | Fenêtres gagnées | p corrigé | Verdict |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| Cassure Keltner (4h) | 4h | 391 | +59.9 % | +231.0 % | 19 % | +24.9 % | 27.5 % | 63.9 % | +94.1 % | 13 / 17 | 71.1 % | **Prometteuse** |
| Acheter et garder | 1d | 0 | +230.9 % | +230.9 % | 100 % | +230.9 % | 63.1 % | 63.1 % | +231.4 % | — | — | **Référence** |
| Croisement MACD (1j) | 1d | 197 | +56.4 % | +230.9 % | 21 % | +28.1 % | 13.7 % | 63.1 % | +63.6 % | 12 / 17 | 100.0 % | **Indiscernable du hasard** |
| Ichimoku Tenkan/Kijun (4h) | 4h | 339 | +73.0 % | +231.0 % | 17 % | +22.4 % | 14.2 % | 63.9 % | +105.8 % | 11 / 17 | 100.0 % | **Indiscernable du hasard** |
| Cassure Donchian 20/10 (4h) | 4h | 555 | +59.2 % | +231.0 % | 28 % | +40.2 % | 38.8 % | 63.9 % | +112.2 % | 11 / 17 | 100.0 % | **Indiscernable du hasard** |
| Supertrend 10×3 (1j) | 1d | 65 | +23.5 % | +230.9 % | 10 % | +13.2 % | 12.7 % | 63.1 % | +25.6 % | 10 / 17 | 100.0 % | **Indiscernable du hasard** |
| Croisement EMA 20/50 (4h) | 4h | 322 | +35.1 % | +231.0 % | 17 % | +23.2 % | 20.5 % | 63.9 % | +59.0 % | 10 / 18 | 100.0 % | **Indiscernable du hasard** |
| Cassure Donchian 55/20 (1j, « Turtle ») | 1d | 64 | +34.1 % | +230.9 % | 11 % | +13.6 % | 10.2 % | 63.1 % | +36.4 % | 9 / 17 | 100.0 % | **Indiscernable du hasard** |
| Supertrend 10×3 (4h) | 4h | 381 | +14.0 % | +231.0 % | 21 % | +29.2 % | 32.0 % | 63.9 % | +37.4 % | 9 / 18 | 100.0 % | **Perdante** |
| Confluence 4/5 (4h) | 4h | 668 | +17.6 % | +231.0 % | 31 % | +45.6 % | 34.3 % | 63.9 % | +62.8 % | 8 / 17 | 100.0 % | **Perdante** |
| Croisement EMA 12/26 (4h) | 4h | 569 | +7.6 % | +231.0 % | 27 % | +38.4 % | 41.3 % | 63.9 % | +47.2 % | 8 / 17 | 100.0 % | **Perdante** |
| Tendance forte ADX (4h) | 4h | 471 | -9.2 % | +231.0 % | 19 % | +25.6 % | 27.9 % | 63.9 % | +14.1 % | 8 / 17 | 100.0 % | **Perdante** |
| Rebond RSI 30 (4h, tendance haussière) | 4h | 52 | +2.6 % | +231.0 % | 2 % | +2.1 % | 8.5 % | 63.9 % | +5.3 % | 7 / 17 | 100.0 % | **Perdante** |
| Croisement MACD (4h) | 4h | 1277 | +5.2 % | +231.0 % | 44 % | +68.5 % | 39.6 % | 63.9 % | +92.3 % | 7 / 17 | 100.0 % | **Perdante** |
| Confluence 4/5 (1j) | 1d | 110 | +17.9 % | +230.9 % | 14 % | +18.1 % | 17.5 % | 63.1 % | +21.1 % | 6 / 15 | 100.0 % | **Perdante** |
| Ichimoku Tenkan/Kijun (1j) | 1d | 50 | +3.7 % | +230.9 % | 7 % | +8.6 % | 9.1 % | 63.1 % | +5.0 % | 6 / 16 | 100.0 % | **Perdante** |
| Peur extrême (Fear & Greed, 1j) | 1d | 35 | -9.1 % | +230.9 % | 40 % | +61.3 % | 56.3 % | 63.1 % | -7.7 % | 5 / 14 | 100.0 % | **Perdante** |
| RSI(2) de Connors (1j) | 1d | 119 | +2.3 % | +230.9 % | 3 % | +4.1 % | 8.2 % | 63.1 % | +5.3 % | 4 / 14 | 100.0 % | **Perdante** |
| Stoch RSI (4h) | 4h | 625 | -27.3 % | +231.0 % | 22 % | +29.9 % | 47.0 % | 63.9 % | +1.0 % | 5 / 17 | 100.0 % | **Perdante** |
| Achat des replis, 3 couches max (4h) | 4h | 271 | +3.7 % | +231.0 % | 19 % | +25.6 % | 29.4 % | 63.9 % | +12.2 % | 5 / 18 | 100.0 % | **Perdante** |
| Croix dorée EMA 50/200 (1j) | 1d | 13 | +11.1 % | +230.9 % | 2 % | +2.5 % | 3.7 % | 63.1 % | +11.5 % | 2 / 10 | 100.0 % | **Perdante** |
| Retour dans Bollinger (4h) | 4h | 276 | -9.0 % | +231.0 % | 6 % | +6.9 % | 26.1 % | 63.9 % | +4.8 % | 4 / 17 | 100.0 % | **Perdante** |
| CCI −100/+100 (4h) | 4h | 430 | -11.1 % | +231.0 % | 13 % | +16.6 % | 28.5 % | 63.9 % | +12.4 % | 2 / 17 | 100.0 % | **Perdante** |
| Retour au VWAP 24h (1h) | 1h | 498 | -25.5 % | +228.4 % | 5 % | +5.7 % | 34.9 % | 64.8 % | -0.2 % | 2 / 18 | 100.0 % | **Perdante** |
| Croisement EMA 9/21 (1h) | 1h | 2757 | -45.7 % | +228.4 % | 37 % | +55.3 % | 61.0 % | 64.8 % | +168.4 % | 2 / 18 | 100.0 % | **Perdante** |
| Rebond RSI 30 (1h, tendance haussière) | 1h | 176 | -10.0 % | +228.4 % | 2 % | +2.5 % | 13.6 % | 64.8 % | +0.7 % | 1 / 17 | 100.0 % | **Perdante** |
| Supertrend 7×2 (1h) | 1h | 2487 | -39.9 % | +228.4 % | 39 % | +59.7 % | 59.1 % | 64.8 % | +152.2 % | 1 / 18 | 100.0 % | **Perdante** |
| Retour dans Bollinger (1h) | 1h | 1051 | -45.7 % | +228.4 % | 7 % | +8.3 % | 47.9 % | 64.8 % | +1.3 % | 0 / 17 | 100.0 % | **Perdante** |
| Williams %R (1h) | 1h | 2126 | -82.3 % | +228.4 % | 17 % | +22.4 % | 83.6 % | 64.8 % | -35.7 % | 0 / 17 | 100.0 % | **Perdante** |
| Stoch RSI (1h) | 1h | 2270 | -84.2 % | +228.4 % | 24 % | +33.5 % | 85.7 % | 64.8 % | -42.0 % | 0 / 17 | 100.0 % | **Perdante** |
