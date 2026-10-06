# PaperTrading2

Paper trading crypto en **Rust + Tauri** : des stratégies suivies sur les prix réels de Binance avec de l'argent fictif, **frais compris**, et **toujours comparées à « acheter et garder »**.

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

## Ce que disent les données (frais compris)

Mesuré le 6 octobre 2026 sur BTC, ETH, SOL, BNB et XRP. Frais de 0,1 % par côté et glissement de 0,02 %. Résultats affichés sur les mêmes 3 ans pour toutes les stratégies (« acheter et garder » : +230,9 % en 1j, +231,0 % en 4h, +228,4 % en 1h) ; verdicts décidés par la validation sur fenêtres glissantes de fin 2017 à 2026. Tableau complet : [`docs/comparaison-2026-10-06.md`](docs/comparaison-2026-10-06.md).

| Verdict | Nombre | Stratégies |
|---|---:|---|
| **Solide** | **0** | — |
| Prometteuse | 1 | Cassure Keltner 4h : 13 fenêtres indépendantes gagnées sur 17 (2,5 % seule, 71 % une fois comptées les 29 stratégies) |
| Indiscernable du hasard | 6 | MACD 1j, Ichimoku 4h, Donchian 20/10 4h, Supertrend 1j, EMA 20/50 4h, Turtle 55/20 1j |
| Perdante | 22 | dont **toutes** les stratégies 1h |

- **Aucune stratégie du catalogue ne prouve qu'elle fait mieux qu'un achat au hasard investi la même part du temps.** Ce qu'elles apportent, c'est une pire baisse bien plus faible, parce qu'elles sont peu investies.
- **MACD 1j et Turtle 55/20**, classées « Solides » par la première version du comparateur, sont **indiscernables du hasard** (détail ci-dessous).
- **Les frais tuent les stratégies rapides.** Le croisement EMA 9/21 en 1h ferait **+168 % sans frais**, mais il tombe à **−46 % avec frais** (2 757 trades). Les stratégies 1h sont toutes perdantes.
- **Aucune stratégie ne bat « acheter et garder » en rendement brut** sur cette période haussière : la meilleure, Ichimoku 4h, fait +73,0 % contre +231,0 %.

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

## Améliorer en boucle sans se mentir

Chercher « la meilleure stratégie » sur l'historique puis la garder, c'est choisir après coup. C'est ce qui faisait croire à la première version qu'elle avait des gagnantes. Deux garde-fous rendent possible une boucle d'amélioration honnête.

**Le registre des essais** (`crates/pt-core/essais.tsv`). Chaque stratégie (identifiant et réglages exacts) et chaque variante de sélection essayée sur les données réelles y est inscrite avant d'être lancée, et n'en sort jamais. La correction pour essais multiples compte l'union du registre et de ce qui tourne : retirer les perdantes ou retoucher un réglage ne peut plus rendre un verdict plus flatteur. `pt essais` affiche le total (aujourd'hui : 29 stratégies, 1 variante de sélection) et les lignes manquantes. Un test échoue si une stratégie du catalogue n'y est pas, et `pt walkforward` refuse une variante non inscrite.

**La sélection glissante** (`pt walkforward`). Les 29 stratégies sont rejouées sur la même grille de fenêtres de 180 jours, décalées de 90 jours. Pour chaque fenêtre, on joue celle qui a le mieux battu un achat au hasard de même exposition, en moyenne, sur les 2 dernières fenêtres **terminées** avant son début. Puis on juge ce procédé comme une stratégie : test du signe sur fenêtres indépendantes, découpage le moins favorable, correction pour les variantes de sélection essayées. Rapport complet : [`docs/walkforward-2026-10-06.md`](docs/walkforward-2026-10-06.md).

Résultat, de mai 2018 à octobre 2026 :

> Verdict : Compatible avec la chance
>
> La stratégie choisie bat la référence à exposition égale dans 6 fenêtre(s) indépendante(s) sur 16. À pile ou face, on ferait au moins aussi bien avec une probabilité de 89.5 %, au-dessus du seuil de 5 %.

- **Choisir la stratégie qui a le mieux marché récemment ne fait pas mieux que le hasard.** Sur les 33 fenêtres jouées, elle en gagne 17 et en perd 16.
- **Mises bout à bout, les 16 fenêtres indépendantes de ce découpage donnent +49,2 %** à la sélection, contre +176,9 % pour un achat au hasard de même exposition et +4 519,3 % pour « acheter et garder ».
- 13 stratégies différentes ont été choisies ; la plus fréquente, Cassure Donchian 20/10 (4h), ne l'a été que 6 fois sur 33.
- Les prochaines idées (nouvelles familles de stratégies, autre taille de position) seront inscrites au registre avant d'être lancées, et jugées de la même façon.

## L'application

- **Portefeuilles** : suivi en direct sur les prix réels. Chaque portefeuille a sa référence « acheter et garder » démarrée au même instant. Il démarre à la première bougie clôturée après sa création : aucun trade antidaté.
- **Backtest** : courbe de valeur contre la référence, avec la frontière hors échantillon, les mesures (rendement, pire baisse, Sharpe, facteur de profit), le résultat « sans frais » et tous les trades. Le panneau **« Est-ce un coup de chance ? »** rejoue la stratégie sur des fenêtres glissantes et rend le verdict ci-dessus.
- **Comparateur** : tout le catalogue d'un coup, sur la même période pour toutes les stratégies, validé sur fenêtres glissantes et classé par verdict : Solide, Prometteuse, Indiscernable du hasard, Perdante. Environ 1 minute une fois l'historique en cache.
- **Stratégies** : ce que chaque stratégie calcule, ses sorties et sa taille de position.
- **Réglages** : frais, glissement, capital par défaut, part hors échantillon.

## Démarrer

Prérequis : [Rust](https://rustup.rs) stable. Sous Windows, il faut aussi les Build Tools C++ de Visual Studio et WebView2 (présent sur Windows 10/11).

```bash
cargo run -p papertrading2            # l'application de bureau
cargo test --workspace                # les tests
cargo run --release -p pt-cli -- presets
cargo run --release -p pt-cli -- backtest --preset macd_1d --symbols BTCUSDT,ETHUSDT --days 1095
cargo run --release -p pt-cli -- compare --days 1095 --validation-days 3650 --window 180 --step 90 --md rapport.md
cargo run --release -p pt-cli -- validate --preset macd_1d --preset donchian_55_20_1d --days 3650 --window 180 --step 90 --md validation.md
cargo run --release -p pt-cli -- walkforward --lookback 2 --md walkforward.md
cargo run --release -p pt-cli -- essais
```

Variables d'environnement facultatives :

- `PT_DATA_DIR` : dossier des données de l'application. Par défaut, c'est le dossier de données de l'utilisateur.
- `PT_BINANCE_URL` : source des bougies. Par défaut, `https://data-api.binance.vision`, le point d'accès public en lecture seule de Binance.

Installateur Windows (non testé dans ce dépôt) : `cargo install tauri-cli --version "^2" --locked`, puis `cargo tauri build`.

## Architecture

```
crates/pt-core    indicateurs, comptabilité, stratégies, moteur, backtest, validation, sélection glissante, registre des essais (lu à la compilation) — aucun accès réseau ni disque
crates/pt-data    bougies Binance (clôturées seulement), Fear & Greed, cache disque
crates/pt-store   SQLite : état, exécutions complètes, courbe de valeur
crates/pt-cli     `pt` : backtest, comparaison, validation, sélection glissante, registre des essais
src-tauri         application de bureau : boucle du mode direct + commandes
ui                interface HTML/CSS/JS sans dépendance externe
```

Tout ce qui décide d'un trade vit dans `pt-core`, sous forme de fonctions pures : c'est rejouable et testable.

## Ce qui n'a pas été repris, et pourquoi

- **Trading réel** (Binance, Jupiter, Uniswap, PancakeSwap, clés privées) : un outil de paper trading n'a pas à détenir de clés. Aucune stratégie n'a démontré d'avantage qui justifierait de l'argent réel.
- **Sniper de tokens DEX** : ses résultats reposaient sur des exécutions et des « rug pulls » simulés au hasard, donc invérifiables.
- **Copie de « whales », du Congrès et d'investisseurs légendaires** : les signaux étaient générés aléatoirement (`_simulate_legendary_trader`).
- **Martingale et renforcement sans stop** : risque de ruine.
- **Ventes à découvert** : impossibles sur Binance spot. Les simuler sans coût d'emprunt surestime les résultats.
- **Alertes Telegram, tableaux de bord Streamlit et Next.js** : remplacés par l'application Tauri.

## Licence

MIT
