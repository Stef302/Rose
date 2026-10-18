#!/usr/bin/env bash
# Benchmarks Rose — mêmes programmes, tous les moteurs, temps mesurés.
# Usage : ./bench.sh          (moteurs compilés : cpp, rust, zig)
#         ./bench.sh python julia   (+ références vivantes)
set -u
cd "$(dirname "$0")"
RACINE=../..
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

declare -A MOTEURS=(
  [cpp]="$RACINE/moteur/cpp/rose"
  [rust]="$RACINE/moteur/rust/rose"
  [zig]="$RACINE/moteur/zig/rose"
)
if [ $# -gt 0 ]; then
  declare -A CHOISIS
  for m in "$@"; do
    case "$m" in
      python) MOTEURS[python]="python3 $RACINE/moteur/python/engine.py"; CHOISIS[$m]=1 ;;
      julia)  MOTEURS[julia]="${JULIA:-julia} $RACINE/moteur/julia/engine.jl"; CHOISIS[$m]=1 ;;
      *)      CHOISIS[$m]=1 ;;
    esac
  done
fi

PROGRAMMES=(boucle_entier boucle_flottante factorielle puissance fibonacci premiers pgcd chaines)

mesure() {  # $1=moteur $2=programme → secondes (min de 2)
  local cmd=${MOTEURS[$1]} meilleure=999999 t0 t1 d
  for _ in 1 2; do
    t0=$(date +%s%N)
    $cmd "bench/$2.rose" > "$TMP/$2.out" 2>&1 || { echo "ERREUR"; return; }
    t1=$(date +%s%N)
    d=$(awk "BEGIN{printf \"%.3f\", ($t1-$t0)/1e9}")
    meilleure=$(awk "BEGIN{print ($d < $meilleure) ? $d : $meilleure}")
  done
  echo "$meilleure"
}

ORDRE=(cpp rust zig python julia)
actifs=()
for m in "${ORDRE[@]}"; do
  [ -n "${MOTEURS[$m]:-}" ] || continue
  if [ $# -gt 0 ] && [ -z "${CHOISIS[$m]:-}" ]; then continue; fi
  actifs+=("$m")
done
echo "programme        $(for m in "${actifs[@]}"; do printf "%8s" "$m"; done)"
for p in "${PROGRAMMES[@]}"; do
  ligne=$(printf "%-16s" "$p")
  for m in "${actifs[@]}"; do
    s=$(mesure "$m" "$p")
    ligne="$ligne$(printf "%8s" "$s")"
  done
  echo "$ligne"
done
