#!/bin/bash
# noyau.sh — assemble et exécute le prototype d'OS en Rose pur.
# Constat daté : 25/11/2026.
#
# Les briques Rose n'ont pas de modules : elles se CONCATÈNENT dans un ordre
# précis (chaque brique ne dépend que de ce qui précède). `octet_brut` est
# défini à l'identique dans briques/image/contour.rose ET
# compilateur/backend_universel.rose : on ne garde que la première définition.
set -e
RACINE=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$RACINE"
OUT=${1:-/tmp/opencode/noyau_cat.rose}
{
  cat compilateur/lecteur_bud.rose
  cat briques/memoire/arena.rose \
      briques/interruptions/vecteurs.rose \
      briques/pilote/contrat.rose \
      briques/abi/appel.rose \
      briques/image/contour.rose \
      briques/profil/noyau.rose \
      compilateur/brique_runtime.rose
  awk '/^fn octet_brut\(/{skip=3} skip>0{skip--;next} {print}' \
      compilateur/backend_universel.rose
  cat briques/quantique/qubit.rose briques/appareil/contrat.rose \
      briques/appareil/chargeur.rose noyau.rose
} > "$OUT"
./rose-20 interprete "$OUT"
