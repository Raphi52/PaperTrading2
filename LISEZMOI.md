# Instantané des portefeuilles PaperTrading

Base du mode direct, arrêtée sur le PC le 2026-10-08 (dernier point de courbe : 01:08:53, heure de Paris).
319 portefeuilles · 33 803 exécutions · 1 597 trades clos · 7 283 décisions.

## Restaurer sur le VPS

```sh
git clone -b portefeuilles https://github.com/Raphi52/PaperTrading2 pt-donnees
mkdir -p ~/papertrading-data
gunzip -c pt-donnees/papertrading.sqlite.gz > ~/papertrading-data/papertrading.sqlite
sha256sum pt-donnees/papertrading.sqlite.gz   # doit valoir la ligne ci-dessous
```

`sha256 papertrading.sqlite.gz` : 2121c4ef3a9632d4143ebab9e786aa617c92d8e259049a80320bb74468952e27

Puis lancer l'application avec `PT_DATA_DIR=~/papertrading-data`. Au démarrage, elle
rejoue dans l'ordre les bougies clôturées pendant l'arrêt : aucun trade n'est perdu,
il est seulement exécuté en retard.

**Une seule machine à la fois** : ne relance pas le mode direct sur le PC tant que le VPS tourne.
