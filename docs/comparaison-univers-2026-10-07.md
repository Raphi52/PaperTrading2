# Comparaison du catalogue — BTCUSDT, ETHUSDT, SOLUSDT, XRPUSDT, ZECUSDT, DOGEUSDT, NEARUSDT, BNBUSDT, SUIUSDT, ADAUSDT, UNIUSDT, AVAXUSDT, ORCAUSDT, NMRUSDT, QNTUSDT, ENAUSDT, HYPEUSDT, WLDUSDT, TAOUSDT, RLCUSDT, PUMPUSDT, TRXUSDT, ONDOUSDT, PEPEUSDT, INJUSDT, FETUSDT, TRUMPUSDT, ACEUSDT, SANDUSDT, ZROUSDT, AAVEUSDT, LTCUSDT, LINKUSDT, PENGUUSDT, MARSCOINUSDT, XLMUSDT, HBARUSDT, ARBUSDT, ASTERUSDT, MINAUSDT, API3USDT, TRBUSDT, PROMUSDT, DOTUSDT, APTUSDT, RENDERUSDT, FILUSDT, BCHUSDT, DASHUSDT, RAYUSDT, GTCUSDT, MUBARAKUSDT, ICPUSDT, MOVRUSDT, VIRTUALUSDT, ETHFIUSDT, XPLUSDT, GRAMUSDT, TIAUSDT, TONUSDT, OPUSDT, METUSDT, POLUSDT, CAKEUSDT, SEIUSDT, EDUUSDT, RADUSDT, PYTHUSDT, AEROUSDT, CHIPUSDT, SHIBUSDT, ARUSDT, MAGICUSDT, WLFIUSDT, UMAUSDT, JTOUSDT, ZAMAUSDT, PARTIUSDT, C98USDT, SAGAUSDT, ETCUSDT, AXSUSDT, LDOUSDT, RESOLVUSDT, JSTUSDT, ORDIUSDT, SYNUSDT, NIGHTUSDT, PENDLEUSDT, LPTUSDT, MIRAUSDT, NILUSDT, STRKUSDT, OGNUSDT, ZENUSDT, HEMIUSDT, SKYUSDT, NFPUSDT, STXUSDT, ALGOUSDT

Frais 0.10 % par côté, glissement 2 pb. Résultats affichés sur 1095 jours. Verdict décidé par une validation sur 3650 jours : fenêtres de 180 jours décalées de 90 jours, chacune comparée à un achat au hasard de même exposition, test du signe sur fenêtres indépendantes, corrigé pour 206 stratégies essayées.

« Hasard à expo. égale » = `(1 + R)^f − 1` : ce qu'un achat au hasard, investi la même part du temps `f`, obtient en moyenne quand « acheter et garder » fait `R`.

| Stratégie | UT | Trades | Rendement | Acheter-garder | Expo. | Hasard à expo. égale | Pire baisse | Réf. | Sans frais | Fenêtres gagnées | p corrigé | Verdict |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| Croisement MACD (1j) | 1d | 1117 | +32.9 % | +66.6 % | 44 % | +25.1 % | 50.2 % | 78.4 % | +55.6 % | 13 / 17 | 100.0 % | **Prometteuse** |
| Acheter et garder | 1d | 0 | +66.6 % | +66.6 % | 94 % | +61.7 % | 78.4 % | 78.4 % | +66.9 % | — | — | **Référence** |
| Cassure Donchian 20/10 (1j) | 1d | 874 | +162.2 % | +66.6 % | 40 % | +22.9 % | 40.6 % | 78.4 % | +200.0 % | 12 / 17 | 100.0 % | **Indiscernable du hasard** |
| Supertrend 7×2 (1j) | 1d | 679 | +36.5 % | +66.6 % | 43 % | +24.3 % | 40.7 % | 78.4 % | +50.3 % | 12 / 18 | 100.0 % | **Indiscernable du hasard** |
| Cassure Keltner 2 ATR (1j) | 1d | 660 | +212.4 % | +66.6 % | 32 % | +17.6 % | 33.9 % | 78.4 % | +246.3 % | 11 / 17 | 100.0 % | **Indiscernable du hasard** |
| Supertrend 10×3 (1j) | 1d | 495 | +110.8 % | +66.6 % | 32 % | +17.8 % | 31.0 % | 78.4 % | +127.6 % | 11 / 17 | 100.0 % | **Indiscernable du hasard** |
| Croisement MACD 8/17/9 (1j) | 1d | 1418 | +84.2 % | +66.6 % | 44 % | +25.2 % | 40.2 % | 78.4 % | +124.9 % | 11 / 17 | 100.0 % | **Indiscernable du hasard** |
| Au-dessus de la SMA 20 (1j) | 1d | 1416 | +255.5 % | +66.6 % | 87 % | +56.0 % | 79.7 % | 78.4 % | +407.5 % | 11 / 18 | 100.0 % | **Indiscernable du hasard** |
| Cassure Donchian 55/20 (1j, « Turtle ») | 1d | 581 | +196.1 % | +66.6 % | 30 % | +16.2 % | 33.0 % | 78.4 % | +224.8 % | 10 / 17 | 100.0 % | **Indiscernable du hasard** |
| Croisement EMA 20/50 (1j) | 1d | 447 | +40.8 % | +66.6 % | 29 % | +15.7 % | 49.9 % | 78.4 % | +50.9 % | 10 / 17 | 100.0 % | **Indiscernable du hasard** |
| Ichimoku 20/60/120 (1j) | 1d | 186 | +58.6 % | +66.6 % | 13 % | +6.9 % | 23.1 % | 78.4 % | +63.7 % | 7 / 12 | 100.0 % | **Indiscernable du hasard** |
| Peur < 30, sortie > 70 (Fear & Greed, 1j) | 1d | 77 | +29.8 % | +66.6 % | 33 % | +18.6 % | 55.3 % | 78.4 % | +34.0 % | 8 / 14 | 100.0 % | **Indiscernable du hasard** |
| Supertrend 14×4 (1j) | 1d | 384 | +46.7 % | +66.6 % | 25 % | +13.7 % | 37.6 % | 78.4 % | +55.9 % | 10 / 18 | 100.0 % | **Indiscernable du hasard** |
| Croisement EMA 12/26 (1j) | 1d | 667 | +22.7 % | +66.6 % | 38 % | +21.1 % | 48.2 % | 78.4 % | +35.7 % | 10 / 18 | 100.0 % | **Indiscernable du hasard** |
| Au-dessus de la SMA 50 (1j) | 1d | 1190 | +160.1 % | +66.6 % | 86 % | +55.3 % | 83.0 % | 78.4 % | +239.1 % | 9 / 17 | 100.0 % | **Indiscernable du hasard** |
| Croisement EMA 21/55 (1j) | 1d | 423 | +105.0 % | +66.6 % | 28 % | +15.2 % | 40.5 % | 78.4 % | +119.0 % | 9 / 17 | 100.0 % | **Indiscernable du hasard** |
| Ichimoku Tenkan/Kijun (1j) | 1d | 430 | +78.8 % | +66.6 % | 27 % | +14.7 % | 41.5 % | 78.4 % | +90.7 % | 8 / 15 | 100.0 % | **Indiscernable du hasard** |
| Croisement EMA 9/21 (1j) | 1d | 721 | +51.6 % | +66.6 % | 41 % | +23.2 % | 46.5 % | 78.4 % | +68.9 % | 9 / 18 | 100.0 % | **Indiscernable du hasard** |
| Peur < 20, sortie > 60 (Fear & Greed, 1j) | 1d | 49 | -19.8 % | +66.6 % | 27 % | +14.8 % | 52.3 % | 78.4 % | -18.0 % | 8 / 13 | 100.0 % | **Perdante** |
| Ichimoku Tenkan/Kijun (4h) | 4h | 2063 | +18.2 % | +57.6 % | 47 % | +24.0 % | 59.3 % | 78.6 % | +114.9 % | 10 / 18 | 100.0 % | **Perdante** |
| Momentum 7 bougies (1j) | 1d | 1997 | +51.3 % | +66.6 % | 83 % | +52.6 % | 87.5 % | 78.4 % | +139.3 % | 9 / 18 | 100.0 % | **Perdante** |
| Croisement EMA 20/50 (4h) | 4h | 2127 | -42.2 % | +57.6 % | 49 % | +24.7 % | 70.5 % | 78.6 % | +8.9 % | 9 / 18 | 100.0 % | **Perdante** |
| Momentum 28 jours (1j) | 1d | 1252 | +305.9 % | +66.6 % | 88 % | +57.0 % | 82.5 % | 78.4 % | +429.5 % | 8 / 17 | 100.0 % | **Perdante** |
| Momentum 14 bougies (1j) | 1d | 1445 | +178.7 % | +66.6 % | 87 % | +55.7 % | 83.6 % | 78.4 % | +282.9 % | 8 / 17 | 100.0 % | **Perdante** |
| Supertrend 20×5 (1j) | 1d | 288 | +58.8 % | +66.6 % | 18 % | +9.9 % | 24.5 % | 78.4 % | +65.9 % | 8 / 17 | 100.0 % | **Perdante** |
| Cassure Keltner 2.5 ATR (4h) | 4h | 2030 | +30.9 % | +57.6 % | 39 % | +19.4 % | 48.1 % | 78.6 % | +140.0 % | 8 / 17 | 100.0 % | **Perdante** |
| Momentum 42 bougies (4h) | 4h | 6878 | -1.7 % | +57.6 % | 88 % | +49.0 % | 94.3 % | 78.6 % | +383.3 % | 8 / 17 | 100.0 % | **Perdante** |
| Confluence 4/5 (1j) | 1d | 799 | +79.8 % | +66.6 % | 42 % | +23.7 % | 48.7 % | 78.4 % | +102.3 % | 7 / 15 | 100.0 % | **Perdante** |
| Ichimoku 20/60/120 (4h) | 4h | 1280 | +27.0 % | +57.6 % | 32 % | +15.5 % | 45.1 % | 78.6 % | +90.3 % | 8 / 18 | 100.0 % | **Perdante** |
| Supertrend 14×4 (4h) | 4h | 1708 | +2.9 % | +57.6 % | 43 % | +21.7 % | 54.8 % | 78.6 % | +72.8 % | 8 / 18 | 100.0 % | **Perdante** |
| Au-dessus de la SMA 100 (1j) | 1d | 1054 | +133.5 % | +66.6 % | 86 % | +55.3 % | 79.6 % | 78.4 % | +199.8 % | 7 / 16 | 100.0 % | **Perdante** |
| Rebond RSI(14) 30 → 55 (1j, tendance haussière) | 1d | 30 | -3.2 % | +66.6 % | 1 % | +0.7 % | 7.7 % | 78.4 % | -2.7 % | 5 / 12 | 100.0 % | **Perdante** |
| Momentum 56 bougies (1j) | 1d | 1201 | +403.7 % | +66.6 % | 88 % | +56.7 % | 71.3 % | 78.4 % | +534.1 % | 7 / 17 | 100.0 % | **Perdante** |
| Momentum 180 bougies (4h) | 4h | 5743 | +129.6 % | +57.6 % | 88 % | +49.4 % | 89.3 % | 78.6 % | +746.2 % | 7 / 17 | 100.0 % | **Perdante** |
| Confluence 5/5, sortie 2/5 (4h) | 4h | 2585 | +10.9 % | +57.6 % | 53 % | +27.2 % | 71.5 % | 78.6 % | +118.9 % | 7 / 17 | 100.0 % | **Perdante** |
| Tendance forte ADX > 25 (1j) | 1d | 792 | -7.2 % | +66.6 % | 36 % | +20.2 % | 71.2 % | 78.4 % | +3.6 % | 7 / 17 | 100.0 % | **Perdante** |
| Au-dessus de la SMA 50 (4h) | 4h | 6798 | -1.5 % | +57.6 % | 87 % | +48.7 % | 93.6 % | 78.6 % | +366.6 % | 7 / 17 | 100.0 % | **Perdante** |
| Cassure Keltner (4h) | 4h | 2936 | -32.4 % | +57.6 % | 52 % | +26.4 % | 69.4 % | 78.6 % | +55.1 % | 7 / 17 | 100.0 % | **Perdante** |
| Cassure Donchian 55/20 (4h) | 4h | 2803 | -54.3 % | +57.6 % | 49 % | +25.1 % | 76.1 % | 78.6 % | -4.4 % | 7 / 17 | 100.0 % | **Perdante** |
| Cassure Donchian 100/50 (1j) | 1d | 423 | +145.6 % | +66.6 % | 22 % | +11.8 % | 29.2 % | 78.4 % | +162.2 % | 6 / 15 | 100.0 % | **Perdante** |
| Au-dessus de la SMA 150 (1j) | 1d | 951 | +163.4 % | +66.6 % | 84 % | +53.7 % | 78.5 % | 78.4 % | +227.2 % | 6 / 16 | 100.0 % | **Perdante** |
| Au-dessus de la SMA 200 (1j) | 1d | 820 | +125.0 % | +66.6 % | 87 % | +55.9 % | 77.2 % | 78.4 % | +169.4 % | 5 / 14 | 100.0 % | **Perdante** |
| RSI(2) de Connors (1j) | 1d | 678 | +27.6 % | +66.6 % | 13 % | +6.9 % | 19.1 % | 78.4 % | +43.5 % | 5 / 14 | 100.0 % | **Perdante** |
| Momentum 90 bougies (1j) | 1d | 1049 | +290.5 % | +66.6 % | 86 % | +55.1 % | 80.2 % | 78.4 % | +391.0 % | 6 / 17 | 100.0 % | **Perdante** |
| Cassure Donchian 20/10 (4h) | 4h | 3911 | -37.6 % | +57.6 % | 64 % | +33.6 % | 74.0 % | 78.6 % | +75.0 % | 6 / 17 | 100.0 % | **Perdante** |
| Cassure Keltner 1.5 ATR (4h) | 4h | 3837 | -47.2 % | +57.6 % | 62 % | +32.9 % | 78.8 % | 78.6 % | +44.8 % | 6 / 17 | 100.0 % | **Perdante** |
| Croisement EMA 9/21 (4h) | 4h | 3177 | -57.0 % | +57.6 % | 66 % | +35.3 % | 80.4 % | 78.6 % | +7.6 % | 6 / 17 | 100.0 % | **Perdante** |
| Confluence 3/5, sortie 1/5 (1j) | 1d | 883 | +5.1 % | +66.6 % | 49 % | +28.1 % | 67.2 % | 78.4 % | +19.7 % | 5 / 15 | 100.0 % | **Perdante** |
| Croisement EMA 12/26 (4h) | 4h | 2863 | -75.4 % | +57.6 % | 62 % | +32.7 % | 87.5 % | 78.6 % | -47.2 % | 6 / 18 | 100.0 % | **Perdante** |
| Croisement EMA 5/13 (4h) | 4h | 4605 | -91.3 % | +57.6 % | 73 % | +39.4 % | 95.1 % | 78.6 % | -69.7 % | 6 / 18 | 100.0 % | **Perdante** |
| Peur extrême (Fear & Greed, 1j) | 1d | 65 | -40.0 % | +66.6 % | 41 % | +23.4 % | 74.2 % | 78.4 % | -38.4 % | 4 / 13 | 100.0 % | **Perdante** |
| Retour dans Bollinger 2σ (1j) | 1d | 219 | -2.2 % | +66.6 % | 8 % | +4.1 % | 29.0 % | 78.4 % | +1.2 % | 4 / 14 | 100.0 % | **Perdante** |
| Croisement EMA 50/200 (4h) | 4h | 940 | -12.4 % | +57.6 % | 27 % | +13.2 % | 54.3 % | 78.6 % | +18.7 % | 5 / 17 | 100.0 % | **Perdante** |
| Retour dans Bollinger 2.5σ (4h) | 4h | 856 | -45.2 % | +57.6 % | 15 % | +7.0 % | 59.9 % | 78.6 % | -26.2 % | 5 / 17 | 100.0 % | **Perdante** |
| Confluence 4/5 (4h) | 4h | 4064 | -19.8 % | +57.6 % | 72 % | +39.0 % | 83.5 % | 78.6 % | +142.7 % | 5 / 17 | 100.0 % | **Perdante** |
| Stoch RSI (4h) | 4h | 3504 | -71.7 % | +57.6 % | 61 % | +32.3 % | 89.2 % | 78.6 % | -18.3 % | 5 / 17 | 100.0 % | **Perdante** |
| Ichimoku 9/26/52 (1h) | 1h | 6305 | -91.2 % | +54.9 % | 57 % | +28.1 % | 95.3 % | 79.0 % | +51.1 % | 5 / 17 | 100.0 % | **Perdante** |
| Supertrend 10×3 (4h) | 4h | 2154 | -54.7 % | +57.6 % | 55 % | +28.1 % | 76.9 % | 78.6 % | -15.3 % | 5 / 18 | 100.0 % | **Perdante** |
| Supertrend 7×2 (4h) | 4h | 2952 | -67.8 % | +57.6 % | 70 % | +37.2 % | 87.2 % | 78.6 % | -26.1 % | 5 / 18 | 100.0 % | **Perdante** |
| Cassure Donchian 10/5 (4h) | 4h | 5050 | -71.7 % | +57.6 % | 70 % | +37.8 % | 89.0 % | 78.6 % | +12.4 % | 5 / 18 | 100.0 % | **Perdante** |
| CCI(20) -100/100 (1j) | 1d | 401 | -8.4 % | +66.6 % | 18 % | +9.4 % | 43.5 % | 78.4 % | -2.0 % | 3 / 14 | 100.0 % | **Perdante** |
| Rebond RSI 30 (4h, tendance haussière) | 4h | 337 | -24.6 % | +57.6 % | 9 % | +4.3 % | 39.4 % | 78.6 % | -15.4 % | 4 / 17 | 100.0 % | **Perdante** |
| CCI −100/+100 (4h) | 4h | 2735 | -74.6 % | +57.6 % | 47 % | +23.6 % | 86.0 % | 78.6 % | -33.4 % | 4 / 17 | 100.0 % | **Perdante** |
| Tendance forte ADX (4h) | 4h | 3498 | -86.0 % | +57.6 % | 61 % | +32.1 % | 93.3 % | 78.6 % | -62.3 % | 4 / 17 | 100.0 % | **Perdante** |
| Croisement MACD 8/17/9 (4h) | 4h | 6777 | -94.8 % | +57.6 % | 73 % | +39.5 % | 97.7 % | 78.6 % | -60.8 % | 4 / 17 | 100.0 % | **Perdante** |
| Croix dorée EMA 50/200 (1j) | 1d | 163 | +47.6 % | +66.6 % | 11 % | +5.8 % | 21.4 % | 78.4 % | +51.1 % | 3 / 15 | 100.0 % | **Perdante** |
| CCI(14) -200/0 (4h) | 4h | 1270 | -49.4 % | +57.6 % | 18 % | +8.5 % | 68.4 % | 78.6 % | -18.1 % | 3 / 17 | 100.0 % | **Perdante** |
| Retour dans Bollinger (4h) | 4h | 1826 | -59.7 % | +57.6 % | 27 % | +13.0 % | 73.6 % | 78.6 % | -23.5 % | 3 / 17 | 100.0 % | **Perdante** |
| Stoch RSI 10/90 (4h) | 4h | 2669 | -77.2 % | +57.6 % | 58 % | +30.4 % | 89.5 % | 78.6 % | -47.5 % | 3 / 17 | 100.0 % | **Perdante** |
| Croisement MACD (4h) | 4h | 5218 | -81.6 % | +57.6 % | 73 % | +39.2 % | 93.6 % | 78.6 % | -14.2 % | 3 / 17 | 100.0 % | **Perdante** |
| Stoch RSI 20/80 (1j) | 1d | 663 | +24.3 % | +66.6 % | 28 % | +15.3 % | 37.8 % | 78.4 % | +39.1 % | 2 / 14 | 100.0 % | **Perdante** |
| Williams %R(14) (1j) | 1d | 522 | -10.8 % | +66.6 % | 21 % | +11.1 % | 46.8 % | 78.4 % | -3.3 % | 2 / 14 | 100.0 % | **Perdante** |
| Achat des replis de 10 %, 3 couches max (1j) | 1d | 2162 | -79.1 % | +66.6 % | 46 % | +26.6 % | 90.0 % | 78.4 % | -67.0 % | 3 / 18 | 100.0 % | **Perdante** |
| Tendance forte ADX > 20 (4h) | 4h | 4273 | -61.5 % | +57.6 % | 70 % | +37.2 % | 87.1 % | 78.6 % | +16.6 % | 2 / 17 | 100.0 % | **Perdante** |
| Williams %R(14) (4h) | 4h | 3459 | -73.9 % | +57.6 % | 54 % | +27.9 % | 88.5 % | 78.6 % | -15.8 % | 2 / 17 | 100.0 % | **Perdante** |
| Retour au VWAP 48 bougies, écart 3 % (1h) | 1h | 4695 | -94.4 % | +54.9 % | 42 % | +19.9 % | 97.3 % | 79.0 % | -60.9 % | 2 / 17 | 100.0 % | **Perdante** |
| Croisement EMA 20/50 (1h) | 1h | 5642 | -98.1 % | +54.9 % | 57 % | +28.4 % | 99.1 % | 79.0 % | -73.9 % | 2 / 18 | 100.0 % | **Perdante** |
| Retour au VWAP 24 bougies, écart 3 % (4h) | 4h | 3341 | -37.7 % | +57.6 % | 43 % | +21.4 % | 80.3 % | 78.6 % | +65.3 % | 1 / 17 | 100.0 % | **Perdante** |
| Achat des replis, 3 couches max (4h) | 4h | 6234 | -78.4 % | +57.6 % | 68 % | +36.5 % | 93.0 % | 78.6 % | -19.0 % | 1 / 17 | 100.0 % | **Perdante** |
| Rebond RSI 30 (1h, tendance haussière) | 1h | 1206 | -51.0 % | +54.9 % | 13 % | +5.8 % | 65.9 % | 79.0 % | -5.5 % | 1 / 18 | 100.0 % | **Perdante** |
| Cassure Keltner 2 ATR (1h) | 1h | 7888 | -90.8 % | +54.9 % | 58 % | +29.1 % | 97.5 % | 79.0 % | +242.3 % | 1 / 18 | 100.0 % | **Perdante** |
| Croisement EMA 9/21 (1h) | 1h | 7891 | -99.6 % | +54.9 % | 67 % | +34.0 % | 99.6 % | 79.0 % | -84.2 % | 1 / 18 | 100.0 % | **Perdante** |
| Croisement EMA 12/26 (1h) | 1h | 7386 | -99.1 % | +54.9 % | 68 % | +34.5 % | 99.5 % | 79.0 % | -75.6 % | 1 / 18 | 100.0 % | **Perdante** |
| Retour dans Bollinger 2.5σ (1h) | 1h | 3183 | -81.4 % | +54.9 % | 20 % | +9.3 % | 84.9 % | 79.0 % | +0.5 % | 0 / 17 | 100.0 % | **Perdante** |
| Rebond RSI(2) 10 → 70 (4h, tendance haussière) | 4h | 5025 | -91.7 % | +57.6 % | 34 % | +16.6 % | 93.7 % | 78.6 % | -50.0 % | 0 / 17 | 100.0 % | **Perdante** |
| Rebond RSI(2) 10 → 70 (1h, tendance haussière) | 1h | 11881 | -99.6 % | +54.9 % | 28 % | +13.3 % | 99.6 % | 79.0 % | +51.7 % | 0 / 17 | 100.0 % | **Perdante** |
| Retour dans Bollinger (1h) | 1h | 6303 | -97.1 % | +54.9 % | 33 % | +15.7 % | 97.7 % | 79.0 % | -27.6 % | 0 / 17 | 100.0 % | **Perdante** |
| Croisement MACD 5/35/5 (1h) | 1h | 9553 | -99.6 % | +54.9 % | 35 % | +16.4 % | 99.6 % | 79.0 % | -77.8 % | 0 / 18 | 100.0 % | **Perdante** |
| Rebond RSI(7) 30 → 60 (1h, tendance haussière) | 1h | 6848 | -97.6 % | +54.9 % | 42 % | +20.4 % | 98.1 % | 79.0 % | -22.1 % | 0 / 17 | 100.0 % | **Perdante** |
| Williams %R (1h) | 1h | 7905 | -99.6 % | +54.9 % | 47 % | +22.7 % | 99.7 % | 79.0 % | -82.9 % | 0 / 17 | 100.0 % | **Perdante** |
| Retour au VWAP 24h (1h) | 1h | 7899 | -99.2 % | +54.9 % | 50 % | +24.3 % | 99.4 % | 79.0 % | -74.9 % | 0 / 17 | 100.0 % | **Perdante** |
| CCI(20) -100/100 (1h) | 1h | 7708 | -99.6 % | +54.9 % | 50 % | +24.7 % | 99.7 % | 79.0 % | -81.8 % | 0 / 17 | 100.0 % | **Perdante** |
| Croisement MACD 12/26/9 (1h) | 1h | 8571 | -99.6 % | +54.9 % | 52 % | +25.8 % | 99.6 % | 79.0 % | -74.7 % | 0 / 18 | 100.0 % | **Perdante** |
| Williams %R(21) (1h) | 1h | 7179 | -99.6 % | +54.9 % | 55 % | +27.2 % | 99.7 % | 79.0 % | -86.4 % | 0 / 17 | 100.0 % | **Perdante** |
| Supertrend 10×3 (1h) | 1h | 5946 | -96.1 % | +54.9 % | 62 % | +31.0 % | 97.1 % | 79.0 % | -40.3 % | 0 / 18 | 100.0 % | **Perdante** |
| Croisement EMA 5/13 (1h) | 1h | 8458 | -99.6 % | +54.9 % | 56 % | +27.8 % | 99.6 % | 79.0 % | -73.6 % | 0 / 18 | 100.0 % | **Perdante** |
| Stoch RSI (1h) | 1h | 7963 | -99.6 % | +54.9 % | 60 % | +30.1 % | 99.7 % | 79.0 % | -82.3 % | 0 / 17 | 100.0 % | **Perdante** |
| Cassure Donchian 20/10 (1h) | 1h | 8459 | -99.6 % | +54.9 % | 64 % | +32.4 % | 99.6 % | 79.0 % | -64.8 % | 0 / 18 | 100.0 % | **Perdante** |
| Supertrend 7×2 (1h) | 1h | 6356 | -99.6 % | +54.9 % | 65 % | +32.8 % | 99.6 % | 79.0 % | -93.3 % | 0 / 18 | 100.0 % | **Perdante** |
| Tendance forte ADX > 25 (1h) | 1h | 8537 | -98.8 % | +54.9 % | 68 % | +34.6 % | 99.1 % | 79.0 % | -41.1 % | 0 / 18 | 100.0 % | **Perdante** |
| Achat des replis de 5 %, 3 couches max (1h) | 1h | 15677 | -95.1 % | +54.9 % | 76 % | +39.8 % | 98.2 % | 79.0 % | +31.0 % | 0 / 18 | 100.0 % | **Perdante** |
| Confluence 4/5, sortie 2/5 (1h) | 1h | 9576 | -98.3 % | +54.9 % | 77 % | +40.3 % | 99.2 % | 79.0 % | -17.7 % | 0 / 17 | 100.0 % | **Perdante** |
