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
| Aucune référence fiable, optimisation sur 16 jours | Les « meilleures » stratégies étaient de la chance | Comparaison à « acheter et garder » + période **hors échantillon** + 20 trades minimum | `a_losing_strategy_is_always_perdante` |
| Historique tronqué à 500 trades, JSON de 33 Mo partagé entre processus | Rapports et optimiseur sur données partielles | SQLite transactionnelle, exécutions jamais tronquées, état revérifié au chargement | `tampered_state_is_refused`, `roundtrip_keeps_everything` |
| Glissement aléatoire | Deux backtests identiques donnaient deux résultats | Coûts fixes et documentés | `backtest_is_deterministic_and_consistent` |
| Clés d'API en clair dans `data/settings.json`, dépôt public | Clés compromises | Aucune clé nécessaire : données publiques seulement | — |
| Filtre horaire 23h-9h « basé sur l'analyse » de 16 jours | Surajustement | Supprimé | — |
| Rien ne se passait pendant que le bot était arrêté | Trous dans l'historique | Au redémarrage, les bougies manquées sont rejouées dans l'ordre | `live.rs` (rattrapage) |

## Ce que disent les données (3 ans, frais compris)

Mesuré le 6 octobre 2026 sur BTC, ETH, SOL, BNB et XRP. Frais de 0,1 % par côté et glissement de 0,02 %. Les 30 % finaux de la période sont gardés hors échantillon. Tableau complet : [`docs/comparaison-2026-10-06.md`](docs/comparaison-2026-10-06.md).

- **2 stratégies « Solides »** sur 30 : le croisement **MACD journalier** et la cassure **Donchian 55/20 journalière** (« Turtle »). Elles gagnent moins que la référence (+35 % contre +94 %), mais avec une pire baisse de **14 % au lieu de 61 %**. Leur rendement ajusté du risque est donc meilleur, aussi sur la période hors échantillon.
- **Les frais tuent les stratégies rapides.** Le croisement EMA 9/21 en 1h ferait **+171 % sans frais**, mais il tombe à **−45 % avec frais** (2 750 trades). Les stratégies 1h sont toutes perdantes.
- **Aucune stratégie ne bat « acheter et garder » en rendement brut** sur cette période haussière.
- Avec 30 stratégies testées, une ou deux peuvent passer par chance. Une stratégie « Solide » mérite un suivi en direct, pas une confiance aveugle.

## L'application

- **Portefeuilles** : suivi en direct sur les prix réels. Chaque portefeuille a sa référence « acheter et garder » démarrée au même instant. Il démarre à la première bougie clôturée après sa création : aucun trade antidaté.
- **Backtest** : courbe de valeur contre la référence, avec la frontière hors échantillon, les mesures (rendement, pire baisse, Sharpe, facteur de profit), le résultat « sans frais » et tous les trades.
- **Comparateur** : tout le catalogue d'un coup, classé par verdict.
- **Stratégies** : ce que chaque stratégie calcule, ses sorties et sa taille de position.
- **Réglages** : frais, glissement, capital par défaut, part hors échantillon.

## Démarrer

Prérequis : [Rust](https://rustup.rs) stable. Sous Windows, il faut aussi les Build Tools C++ de Visual Studio et WebView2 (présent sur Windows 10/11).

```bash
cargo run -p papertrading2            # l'application de bureau
cargo test --workspace                # les tests
cargo run --release -p pt-cli -- presets
cargo run --release -p pt-cli -- backtest --preset macd_1d --symbols BTCUSDT,ETHUSDT --days 1095
cargo run --release -p pt-cli -- compare --days 1095 --md rapport.md
```

Variables d'environnement facultatives :

- `PT_DATA_DIR` : dossier des données de l'application. Par défaut, c'est le dossier de données de l'utilisateur.
- `PT_BINANCE_URL` : source des bougies. Par défaut, `https://data-api.binance.vision`, le point d'accès public en lecture seule de Binance.

Installateur Windows (non testé dans ce dépôt) : `cargo install tauri-cli --version "^2" --locked`, puis `cargo tauri build`.

## Architecture

```
crates/pt-core    indicateurs, comptabilité, stratégies, moteur, backtest — aucun accès réseau ni disque
crates/pt-data    bougies Binance (clôturées seulement), Fear & Greed, cache disque
crates/pt-store   SQLite : état, exécutions complètes, courbe de valeur
crates/pt-cli     `pt` : backtest et comparaison en ligne de commande
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
