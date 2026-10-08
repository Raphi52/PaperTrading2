# PaperTrading

Paper trading crypto en **Rust + Tauri** : des stratégies suivies sur les prix réels de **Bitvavo, en paires euros** avec de l'argent fictif, **frais compris**, et **toujours comparées à « acheter et garder »**. Chaque paire de l'univers s'achète réellement en euros depuis la France : le jour où l'on passe à l'argent réel, seule la connexion à la plateforme change, pas les paires, les prix, les frais ni le montant minimal d'un ordre. Bitvavo B.V. figure sur la [liste blanche des prestataires de services sur crypto-actifs de l'AMF](https://www.amf-france.org/fr/espace-epargnants/proteger-son-epargne/listes-blanches/psca/bitvavo-bv) (agrément MiCA néerlandais, passeport européen, services en France déclarés à partir du 14/07/2025 : plateforme de négociation et conservation), vérifié le 8 octobre 2026. Frais : 0,25 % par ordre exécuté immédiatement sur les paires euros, sous 100 000 € échangés sur 30 jours ([fiche Café de la Bourse](https://www.cafedelabourse.com/fiches-courtiers/bitvavo)).

C'est la réécriture complète de [PaperTrading](https://github.com/Raphi52/PaperTrading) (Python). Le premier projet ne gagnait pas d'argent. Sur ses propres données (304 portefeuilles, 16 jours), le rendement médian était de **+0,38 %**, alors que BTC prenait **+3,8 %** et SOL **+11,5 %** sur la même période. Ce dépôt corrige les causes de cet échec, une par une. Le code et les tests empêchent qu'elles reviennent.

> ⚠️ Simulation uniquement. Aucun ordre réel n'est envoyé, aucune clé n'est demandée. Ce n'est pas un conseil en investissement.

## Ce qui a été corrigé

| Erreur de conception de la v1 | Ce qu'elle provoquait | Correction | Garantie |
|---|---|---|---|
| Règles « SECURE PROFIT » appliquées à **toutes** les stratégies (`bot.py:3343-3356`) | Gains coupés entre +0,1 et +2 %. Même le « HODL » revendait son BTC à +0,1 % au bout de 3 jours | Chaque stratégie porte ses propres sorties. La référence ne vend jamais | `buy_and_hold_never_sells` |
| Pertes laissées courir jusqu'au stop, gains coupés tôt | 541 sorties « URGENT » = +893 $, 454 stops = −21 147 $ | Stops en ATR et stop suiveur, objectifs en multiples du risque | `trailing_stop_ratchets_up_only` |
| Frais d'achat oubliés dans le PnL (`bot.py:3070`) | PnL affiché trop flatteur (~9 400 $ de frais invisibles) | Le coût d'une position inclut les frais d'achat | `pnl_includes_both_fees`, `breakeven_covers_round_trip` |
| Rachats sans filtre d'une crypto déjà détenue | 5,5 achats par position en moyenne, jusqu'à 100, un toutes les 9 min | Jamais de second achat, sauf renforcement explicitement plafonné | `no_second_buy_on_a_held_symbol`, `pyramid_is_bounded` |
| Indicateurs calculés sur la bougie **en cours** | Signaux qui s'allument puis disparaissent | Décision à la clôture, exécution à l'ouverture suivante | `every_indicator_is_causal`, `signals_never_use_future_candles`, `decision_at_close_fills_at_next_open` |
| Micro-trades sur 1m/5m avec des gains ≈ frais | Stoch RSI : 1 390 sorties à +0,18 % de mouvement médian | Objectif minimal ≥ 3× le coût d'un aller-retour, rapport « sans frais » dans chaque backtest | `catalog_is_consistent` |
| 170 noms pour ~15 logiques (« Williams %R » = code Stoch RSI…) | Impossible de savoir ce qui est testé | 30 stratégies, chacune calcule l'indicateur de son nom | `oscillators_stay_in_range` |
| Données **simulées au hasard** (whales, « Pelosi », « Buffett », signal « alpha ») | Décisions et tailles de position influencées par du bruit | Seules les bougies réelles et l'indice Fear & Greed publié servent de signal | — (code supprimé) |
| Martingale « NO LIMIT », renforcement sans stop | Risque de ruine | Interdit par validation : stop obligatoire, 5 couches max, exposition ≤ 50 % | `catalog_is_consistent` |
| Taille = % de la trésorerie **restante** | Positions de plus en plus petites, sans lien avec le risque | Taille = risque fixe (1 %) du capital total entre l'entrée et le stop | `engine::entry_notional` |
| Backtest et bot avec deux logiques différentes | Un backtest ne prédisait rien du bot | Un seul moteur (`Engine::advance`) pour les deux | `live_increments_equal_one_shot_backtest` |
| Aucune référence fiable, optimisation sur 16 jours | Les « meilleures » stratégies étaient de la chance | Barre à battre : un **achat au hasard de même exposition**. Verdict décidé par une **validation sur fenêtres glissantes**, corrigée pour toutes les stratégies essayées | `a_single_period_can_never_be_solide`, `solide_only_ever_comes_from_the_validation` |
| Comparaison au Sharpe brut d'« acheter et garder » (première version de ce dépôt) | Une stratégie peu investie « gagnait » mécaniquement les marchés baissiers : MACD 1j et Turtle étaient classées « Solides » à tort | Même barre et même validation partout : comparateur, backtest et ligne de commande | `judged_against_the_same_exposure_not_the_raw_sharpe` |
| Historique tronqué à 500 trades, JSON de 33 Mo partagé entre processus | Rapports et optimiseur sur données partielles | SQLite transactionnelle, exécutions jamais tronquées, état revérifié au chargement | `tampered_state_is_refused`, `roundtrip_keeps_everything` |
| Glissement aléatoire | Deux backtests identiques donnaient deux résultats | Coûts fixes et documentés | `backtest_is_deterministic_and_consistent` |
| Clés d'API en clair dans `data/settings.json`, dépôt public | Clés compromises | Aucune clé nécessaire : données publiques seulement | — |
| Filtre horaire 23h-9h « basé sur l'analyse » de 16 jours | Surajustement | Supprimé | — |
| Rien ne se passait pendant que le bot était arrêté | Trous dans l'historique | Au redémarrage, les bougies manquées sont rejouées dans l'ordre | `live.rs` (rattrapage) |
| Correction pour essais multiples calculée sur le seul catalogue du moment (première version de ce dépôt) | Retirer les perdantes faisait passer la correction de ×29 à ×7 ; un réglage modifié n'était compté nulle part | Registre des essais versionné (`crates/pt-core/essais.tsv`), qui ne fait que grandir. La correction compte l'union du registre et de ce qui tourne. `--tested` ne peut pas descendre en dessous | `removing_a_strategy_never_lowers_the_correction`, `changing_a_setting_counts_as_a_new_trial`, `every_catalog_preset_is_registered`, `tested_below_registry_is_refused` |
| Préchauffage pris dans la période affichée (première version de ce dépôt) | Chaque stratégie affichait une autre période : sur les « mêmes » 3 ans, « acheter et garder » allait de −8,6 % à +129,7 % selon la ligne | Préchauffage sur les bougies d'avant la période. L'historique chargé couvre le plus long préchauffage de l'unité de temps, avec ou sans validation | `every_strategy_shows_the_same_period`, `history_includes_warmup_without_validation` |
| Garder « la meilleure » stratégie après coup | La meilleure sur l'historique n'est souvent que la plus chanceuse | Sélection glissante : chaque fenêtre est jouée par la stratégie choisie avec les seules fenêtres déjà terminées, et c'est ce procédé qui est jugé | `selection_never_sees_the_window_it_plays`, `selection_on_random_walks_is_not_solide` |
| Contrôle « aucun talent sur des prix aléatoires » fait sur un générateur à tendances (première version de ce dépôt) | Un suivi de tendance y a un vrai talent : le momentum 28 jours y gagnait 14 fenêtres sur 18, et le test aurait fini par interdire toute stratégie de tendance | Le contrôle tourne sur une marche aléatoire PURE (rendements indépendants, sans dérive) ; un second contrôle vérifie qu'une stratégie sans talent n'y bat pas la barre plus de 6 fois sur 10 (mesuré : 13 sur 40 avec frais) | `selection_on_random_walks_is_not_solide`, `no_talent_does_not_beat_the_bar_on_a_pure_random_walk` |
| Deux fenêtres de l'application ouvertes = deux boucles du mode direct (première version de ce dépôt) | Chaque bougie pouvait être traitée deux fois, et la base renvoyer « verrouillée » | Un verrou de fichier : une seule instance fait avancer les portefeuilles ; les autres affichent la même base (« affichage seul ») et prennent le relais si elle se ferme. La base attend 5 s une écriture en cours au lieu d'échouer | `only_one_instance_runs_the_live_loop` |

## Ce que disent les données (frais compris)

Mesuré le 7 octobre 2026, **sur l'ancienne source de prix (paires USDT, frais de 0,1 %), avant le passage aux paires euros de Bitvavo du 8 octobre 2026 ; pas encore recalculé**, sur BTC, ETH, SOL, BNB et XRP, pour les **101 stratégies** du catalogue (105 essais au registre, portage compris). Frais de 0,1 % par côté et glissement de 0,02 %. Résultats affichés sur les mêmes 3 ans pour toutes les stratégies (« acheter et garder » : +240,2 % en 1j) ; verdicts décidés par la validation sur fenêtres glissantes de fin 2017 à 2026. Tableau complet : [`docs/comparaison-2026-10-07.md`](docs/comparaison-2026-10-07.md). Le comparateur complet tourne en moins d'une minute.

| Verdict | Nombre | Stratégies |
|---|---:|---|
| **Solide** | **0** | — |
| Prometteuse | 4 | MACD 8/17/9 1j, Cassure Keltner 2,5 ATR 4h, Cassure Keltner 4h, Supertrend 7×2 1j : 13 fenêtres indépendantes gagnées sur 17 ou 18, mais plus significatif une fois comptés les 105 essais |
| Indiscernable du hasard | 27 | dont MACD 1j, Ichimoku 4h, Donchian 20/10 4h, **Momentum 28 jours 1j**, **Au-dessus de la SMA 50 1j**, Turtle 55/20 1j |
| Perdante | 70 | dont **les 25** stratégies 1h |

> **Comparer plus de stratégies rend la barre plus haute.** Avec 105 essais, une stratégie doit battre le hasard dans presque toutes ses fenêtres pour que ce ne soit pas une chance attendue sur 105 tirages. C'est voulu : choisir la meilleure parmi 100 sans cette correction, c'est l'erreur qui faisait croire à la v1 qu'elle avait des gagnantes.

- **Aucune stratégie du catalogue ne prouve qu'elle fait mieux qu'un achat au hasard investi la même part du temps.** Ce qu'elles apportent, c'est une pire baisse bien plus faible, parce qu'elles sont peu investies.
- **MACD 1j et Turtle 55/20**, classées « Solides » par la première version du comparateur, sont **indiscernables du hasard** (détail ci-dessous).
- **Les frais tuent les stratégies rapides.** Le croisement EMA 9/21 en 1h ferait **+168 % sans frais**, mais il tombe à **−46 % avec frais** (2 757 trades). Les stratégies 1h sont toutes perdantes.
- **Une seule stratégie bat « acheter et garder » en rendement brut** sur ces 3 ans : **Au-dessus de la SMA 50 (1j)**, qui fait **+362,2 %** contre +240,2 %, avec une pire baisse de **33 %** contre 63 %. Elle est investie à parts égales sur chaque symbole tant que son prix est au-dessus de sa moyenne 50 jours, en liquide sinon (Detzel et al., 2021). Mais sur 2017-2026, elle ne bat un timing au hasard de même exposition que dans 9 fenêtres sur 17 : ce gain peut venir de la période. Elle est suivie en direct pour le vérifier sur des prix qu'elle n'a jamais vus.
- **Le momentum 28 jours** (Liu & Tsyvinski, 2021), même principe, fait +201,1 % avec une pire baisse de 36 %, et reste lui aussi indiscernable du hasard (10 fenêtres sur 18).

## Sur les 50 cryptos les plus échangées en euros (`--symbols top50`)

Chaque stratégie peut scanner un **univers de 50 cryptos** (`crates/pt-data/univers.txt`) : les plus échangées **en euros sur Bitvavo** au 8 octobre 2026, classées par la **médiane de leur volume quotidien en EUR sur 90 jours** (les jours d'avant la cotation comptent pour zéro). Toutes s'achètent en euros : le test `univers_tout_achetable` le vérifie contre la liste des marchés Bitvavo relevée le même jour (`crates/pt-data/fixtures/bitvavo-markets.json`). Stablecoins, or tokenisé, jetons enveloppés et paires cotées depuis moins de 45 jours sont exclus. Ces 50 font 87 % du volume en euros ; la 50e échange encore 0,25 M€ par jour. Dans l'application comme dans `pt`, une paire qui n'est pas cotée en euros sur Bitvavo au moment de la demande (absente ou suspendue) est refusée avec son motif.

> Les chiffres ci-dessous datent de l'**ancienne liste de 100 cryptos en USDT** (volume 24 h du 7 octobre 2026) ; ils n'ont pas encore été recalculés sur les 50 paires euros.

| Verdict | Nombre |
|---|---:|
| **Solide** | **0** |
| Prometteuse | 1 (MACD 1j : 13 fenêtres indépendantes gagnées sur 17) |
| Indiscernable du hasard | 16 |
| Perdante | 84, dont les 25 stratégies 1h |

- « Acheter et garder » les 100 cryptos ne fait que **+66,6 %** sur 3 ans (pire baisse 78 %). Plusieurs stratégies 1j font mieux en rendement brut (Cassure Keltner 2 ATR +212 %, Turtle 55/20 +196 %), mais **aucune ne prouve que ce n'est pas la chance**.
- ⚠️ **Biais du survivant** : ce sont les premières *aujourd'hui*. Rejouer le passé sur cette liste favorise les cryptos qui ont survécu et monté ; un backtest y est plus flatteur qu'il ne l'aurait été en temps réel. Seuls les portefeuilles en direct, sur des prix futurs, en sont exempts.

## Coup de chance ou pas ? Validation sur fenêtres glissantes

Les deux stratégies que la première version du comparateur classait « Solides » ont été rejouées de fin 2017 à juillet 2026, sur des fenêtres de 180 jours décalées de 90 jours (34 fenêtres). Chaque fenêtre repart de zéro. Chaque fenêtre se compare à « acheter et garder » **ramené à la même exposition** : `(1 + R)^f − 1`. C'est ce qu'obtient en moyenne un timing au hasard investi la fraction `f` du temps. Rapport complet : [`docs/validation-2026-10-06.md`](docs/validation-2026-10-06.md).

| Stratégie | Fenêtres indépendantes gagnées | Probabilité à pile ou face | Corrigée pour 29 essais | Verdict |
|---|---:|---:|---:|---|
| Croisement MACD (1j) | 12 / 17 | 7,2 % | 100 % | **Compatible avec la chance** |
| Cassure Donchian 55/20 (1j) | 9 / 17 | 50 % | 100 % | **Compatible avec la chance** |

- **Aucune des deux n'est prouvée.** Le résultat ne dépend pas du découpage. Avec des fenêtres de 90 jours ou d'un an, avec ou sans chevauchement, aucune ne passe le seuil de 5 % une fois comptées les 29 stratégies essayées.
- **MACD est la seule piste sérieuse.** Prise isolément, elle passe le seuil dans 2 des 5 découpages essayés (25 fenêtres sur 35 en 90 jours : 0,8 %). Elle gagne presque toutes les fenêtres jusqu'en 2022, mais **perd 6 des 12 dernières**, surtout les fenêtres haussières.
- **Turtle 55/20 se comporte comme un pile ou face** dès que les fenêtres raccourcissent (10 gagnées sur 30 en 90 jours).
- **Ce qui est réel : la protection.** Leur pire fenêtre de 6 mois est de −6,8 % et −4,3 %, contre −62 % pour « acheter et garder ». Cette protection vient surtout du fait de n'être investi qu'entre 0 et 21 % du capital, pas d'un talent de timing démontré.

Pourquoi ne pas comparer simplement au Sharpe d'« acheter et garder » ? Parce qu'une stratégie qui achète **au hasard**, en restant souvent en liquide, « gagnerait » mécaniquement toutes les fenêtres baissières. La référence à exposition égale élimine ce biais. Le test ne retient que des fenêtres sans chevauchement, et garde le découpage le moins favorable.

## Portage du financement des perpétuels : retiré

La stratégie de portage (acheter au comptant, vendre le contrat perpétuel en même quantité et toucher le financement) a été **retirée le 8 octobre 2026**, avec la commande `pt portage`. Elle reposait sur des contrats perpétuels et leurs taux de financement, que la plateforme des prix (Bitvavo) ne propose pas : elle n'était donc pas reproductible avec de vrais euros depuis la France. Son dernier rapport reste archivé dans [`docs/portage-2026-10-06.md`](docs/portage-2026-10-06.md) : prime presque éteinte depuis 2025 (de −0,1 % à +0,9 % par semestre), et verdict « Prometteuse, pas prouvée ». Ses variantes restent au registre des essais, qui ne fait que grandir.

## Améliorer en boucle sans se mentir

Chercher « la meilleure stratégie » sur l'historique puis la garder, c'est choisir après coup. C'est ce qui faisait croire à la première version qu'elle avait des gagnantes. Deux garde-fous rendent possible une boucle d'amélioration honnête.

**Le registre des essais** (`crates/pt-core/essais.tsv`). Chaque stratégie (identifiant et réglages exacts) et chaque variante de sélection essayée sur les données réelles y est inscrite avant d'être lancée, et n'en sort jamais. La correction pour essais multiples compte l'union du registre et de ce qui tourne : retirer les perdantes ou retoucher un réglage ne peut plus rendre un verdict plus flatteur. `pt essais` affiche le total (aujourd'hui : 33 stratégies, dont les 2 variantes de portage, et 1 variante de sélection) et les lignes manquantes. Un test échoue si une stratégie du catalogue n'y est pas, et `pt walkforward` refuse une variante non inscrite.

**La sélection glissante** (`pt walkforward`). Les 31 stratégies du catalogue sont rejouées sur la même grille de fenêtres de 180 jours, décalées de 90 jours. Pour chaque fenêtre, on joue celle qui a le mieux battu un achat au hasard de même exposition, en moyenne, sur les 2 dernières fenêtres **terminées** avant son début. Puis on juge ce procédé comme une stratégie : test du signe sur fenêtres indépendantes, découpage le moins favorable, correction pour les variantes de sélection essayées. Rapport complet : [`docs/walkforward-2026-10-06.md`](docs/walkforward-2026-10-06.md).

Résultat, de novembre 2018 à octobre 2026 :

> Verdict : Compatible avec la chance
>
> La stratégie choisie bat la référence à exposition égale dans 6 fenêtre(s) indépendante(s) sur 17. À pile ou face, on ferait au moins aussi bien avec une probabilité de 92.8 %, au-dessus du seuil de 5 %.

- **Choisir la stratégie qui a le mieux marché récemment ne fait pas mieux que le hasard.**
- **Mises bout à bout, les 17 fenêtres de ce découpage donnent +6,1 %** à la sélection, contre +159,7 % pour un achat au hasard de même exposition et +4 784,4 % pour « acheter et garder ».
- 12 stratégies différentes ont été choisies ; les plus fréquentes sont le momentum 28 jours (11 fois) et la SMA 50 (8 fois).
- Les prochaines idées (nouvelles familles de stratégies, autre taille de position) seront inscrites au registre avant d'être lancées, et jugées de la même façon.

## L'application

- **Portefeuilles** : suivi en direct sur les prix réels. Une seule fenêtre de l'application fait avancer les portefeuilles ; une seconde fenêtre ouverte affiche la même base (« affichage seul ») et prend le relais si la première se ferme. Chaque portefeuille a sa référence « acheter et garder » démarrée au même instant. Il démarre à la première bougie clôturée après sa création : aucun trade antidaté.
- **Backtest** : courbe de valeur contre la référence, avec la frontière hors échantillon, les mesures (rendement, pire baisse, Sharpe, facteur de profit), le résultat « sans frais » et tous les trades. Le panneau **« Est-ce un coup de chance ? »** rejoue la stratégie sur des fenêtres glissantes et rend le verdict ci-dessus.
- **Comparateur** : tout le catalogue d'un coup, sur la même période pour toutes les stratégies, validé sur fenêtres glissantes et classé par verdict : Solide, Prometteuse, Indiscernable du hasard, Perdante. Environ 1 minute une fois l'historique en cache.
- **Stratégies** : ce que chaque stratégie calcule, ses sorties et sa taille de position.
- **Réglages** : frais, glissement, capital par défaut, part hors échantillon.
- **Comptes** : les clés d'API de 8 plateformes au comptant en euros (Bitvavo, Kraken, Coinbase Advanced Trade, Bitstamp, OKX, Bybit EU, Crypto.com Exchange, One Trading). Elles sont rangées dans le Gestionnaire d'identification de Windows (`PaperTrading/<plateforme>`), jamais dans la base ni dans un fichier, et ne sont jamais réaffichées : seuls l'état, les 4 derniers caractères de la clé publique et la date sont montrés. L'application n'envoie encore aucun ordre réel : ces clés préparent le branchement du trading réel.

## Démarrer

Prérequis : [Rust](https://rustup.rs) stable. Sous Windows, il faut aussi les Build Tools C++ de Visual Studio et WebView2 (présent sur Windows 10/11).

```bash
cargo run -p papertrading            # l'application de bureau
cargo test --workspace                # les tests
cargo run --release -p pt-cli -- presets
cargo run --release -p pt-cli -- backtest --preset macd_1d --symbols BTCEUR,ETHEUR --days 1095
cargo run --release -p pt-cli -- compare --days 1095 --validation-days 3650 --window 180 --step 90 --md rapport.md
cargo run --release -p pt-cli -- validate --preset macd_1d --preset donchian_55_20_1d --days 3650 --window 180 --step 90 --md validation.md
cargo run --release -p pt-cli -- walkforward --lookback 2 --md walkforward.md
cargo run --release -p pt-cli -- essais
cargo run --release -p pt-cli -- suivre --preset sma_trend_50_1d --nom "Tendance SMA 50"
```

`pt suivre` crée un portefeuille dans la base de l'application (même dossier qu'elle, ou `--donnees`), avec sa référence « acheter et garder » démarrée au même instant. L'application le fait avancer à son passage suivant, même si elle est déjà ouverte.

Variables d'environnement facultatives :

- `PT_DATA_DIR` : dossier des données de l'application. Par défaut, c'est le dossier de données de l'utilisateur.
- `PT_MARKET_URL` : source des bougies. Par défaut, `https://api.bitvavo.com`, le point d'accès public de Bitvavo, en lecture seule et sans clé.

Installateur Windows : `cargo install tauri-cli --version "^2" --locked`, puis `cargo tauri build`. Il produit `target/release/bundle/nsis/PaperTrading_0.1.0_x64-setup.exe` (3,4 Mo). L'installation se fait pour l'utilisateur courant, sans droits d'administrateur, dans `%LOCALAPPDATA%\PaperTrading2`, avec un raccourci dans le menu Démarrer et un désinstalleur. `/S` l'installe sans fenêtre ; depuis Git Bash, préfixe la commande de `MSYS_NO_PATHCONV=1`, sinon `/S` devient `S:/` et l'assistant s'ouvre. Testé le 6 octobre 2026 : la version installée reprend la base existante (`%APPDATA%\com.raphi52.papertrading2`) et ses portefeuilles. Depuis le renommage en PaperTrading, la base vit dans `%APPDATA%\com.raphi52.papertrading\papertrading.sqlite` ; au premier lancement, l'ancienne base est copiée, jamais effacée.

**L'application ne fait avancer les portefeuilles que lorsqu'elle est ouverte.** Fermée, elle ne perd rien : à la réouverture, elle rejoue dans l'ordre les bougies clôturées entretemps, avec le même moteur que le backtest.

## Architecture

```
crates/pt-core    indicateurs, comptabilité, stratégies, moteur, backtest, validation, sélection glissante, registre des essais (lu à la compilation) — aucun accès réseau ni disque
crates/pt-data    bougies Bitvavo en EUR (clôturées seulement), univers et contrôle des paires achetables, Fear & Greed, cache disque
crates/pt-store   SQLite : état, exécutions complètes, courbe de valeur
crates/pt-cli     `pt` : backtest, comparaison, validation, sélection glissante, registre des essais, création de portefeuilles
src-tauri         application de bureau : boucle du mode direct + commandes ; accounts.rs = clés d'API (Gestionnaire d'identification de Windows)
ui                interface HTML/CSS/JS sans dépendance externe
```

Tout ce qui décide d'un trade vit dans `pt-core`, sous forme de fonctions pures : c'est rejouable et testable.

## Ce qui n'a pas été repris, et pourquoi

- **Trading réel** (plateformes centralisées, Jupiter, Uniswap, PancakeSwap, clés privées) : un outil de paper trading n'a pas à détenir de clés. Les paires, les frais (0,25 % par côté) et le minimum d'ordre (5 €) sont déjà ceux de Bitvavo, pour que le passage à l'argent réel ne change que la connexion. Aucune stratégie n'a démontré d'avantage qui justifierait de l'argent réel.
- **Sniper de tokens DEX** : ses résultats reposaient sur des exécutions et des « rug pulls » simulés au hasard, donc invérifiables.
- **Copie de « whales », du Congrès et d'investisseurs légendaires** : les signaux étaient générés aléatoirement (`_simulate_legendary_trader`).
- **Martingale et renforcement sans stop** : risque de ruine.
- **Ventes à découvert** : impossibles au comptant. Les simuler sans coût d'emprunt surestime les résultats.
- **Alertes Telegram, tableaux de bord Streamlit et Next.js** : remplacés par l'application Tauri.

## Licence

MIT
