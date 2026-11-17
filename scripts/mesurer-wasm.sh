#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════
# M6.2 — Mesure reproductible de la porte de performance du runtime
#        WebAssembly officiel : fib(27) ≤ 20 ms (sortie 196418).
#
# Usage :  bash scripts/mesurer-wasm.sh
#
# Ce script NE télécharge RIEN, N'installe RIEN et NE modifie aucun fichier
# du dépôt. Il détecte un runtime déjà présent, ou déclare la porte non
# mesurable. Il ne revendique jamais ≤ 20 ms sans mesure réelle.
#
# Détection alignée sur rose/src/wasm_runtime.rs :
#   - cache : <base>/rose/runtimes/<runtime>-v0.1.5 (ROSE_CACHE → XDG → HOME)
#   - PATH  : rose-wasmtime-runner / rose-wasmer-runner, puis wasmtime / wasmer
# Contrat runner : runner <module.wasm> <export> [arg1 ...]
# ═══════════════════════════════════════════════════════════════════════
set -u
cd "$(dirname "$0")/.."

VERSION="v0.1.5"           # = VERSION_RUNTIME dans wasm_runtime.rs
SEUIL_NS=20000000          # 20 ms en nanosecondes
NB_ECHANTILLONS=6

# Module fib RÉCURSIF (i32 -> i32), 61 octets, export "fib".
#   Validé : Node WebAssembly -> fib(27) = 196418 ; chargeur M6.1 de Rose OK.
#   md5  : 1ea8a09ca1186a7f9de48d2068f2dcba
#   sha256: 00c490e30433166670217e7999f5bd5dd678e67712ba6850ec94ca12c15119d3
MOD_BASE64="AGFzbQEAAAABBgFgAX8BfwMCAQAHBwEDZmliAAAKHgEcACAAQQJIBH8gAAUgAEEBaxAAIABBAmsQAGoLCw=="

REF_WASM="172,1"           # médiane ms, crate de validation isolée, interpréteur de pile wasm
REF_NATIF="~4,6"           # médiane ms, binaire ./rose/rose (autre chemin de code)

# ── 1. Dossier de cache (même convention que dossier_cache()) ─────────
if [ -n "${ROSE_CACHE:-}" ]; then
  CACHE_DIR="${ROSE_CACHE%/}/runtimes"
elif [ -n "${XDG_CACHE_HOME:-}" ]; then
  CACHE_DIR="${XDG_CACHE_HOME%/}/rose/runtimes"
else
  CACHE_DIR="${HOME:-.}/.cache/rose/runtimes"
fi

# ── 2. Détection d'un runtime ─────────────────────────────────────────
RUNNER=""
MODE=""                     # runner | wasmtime | wasmer

# 2a. cache : nom déterministe <runtime>-0.1.5 (ou tout exécutable wasmtime-*/wasmer-*)
for rt in wasmtime wasmer; do
  for cand in "$CACHE_DIR/${rt}-${VERSION}" "$CACHE_DIR/${rt}-0.1.5"; do
    if [ -x "$cand" ]; then RUNNER="$cand"; MODE="runner"; break 2; fi
  done
done
if [ -z "$RUNNER" ] && [ -d "$CACHE_DIR" ]; then
  for cand in "$CACHE_DIR"/wasmtime-* "$CACHE_DIR"/wasmer-*; do
    if [ -x "$cand" ]; then RUNNER="$cand"; MODE="runner"; break; fi
  done
fi

# 2b. PATH : runners dédiés d'abord, puis binaires officiels nus
if [ -z "$RUNNER" ]; then
  for name in rose-wasmtime-runner rose-wasmer-runner; do
    p=$(command -v "$name" 2>/dev/null || true)
    if [ -n "$p" ] && [ -x "$p" ]; then RUNNER="$p"; MODE="runner"; break; fi
  done
fi
if [ -z "$RUNNER" ]; then
  for name in wasmtime wasmer; do
    p=$(command -v "$name" 2>/dev/null || true)
    if [ -n "$p" ] && [ -x "$p" ]; then RUNNER="$p"; MODE="$name"; break; fi
  done
fi

# ── 3. Absent : porte non mesurable ──────────────────────────────────
if [ -z "$RUNNER" ]; then
  echo "PORTE NON MESURABLE: runtime absent"
  echo
  echo "  Détection effectuée (aucun téléchargement, aucune installation) :"
  echo "    cache scruté      : $CACHE_DIR"
  echo "    noms attendus     : wasmtime-$VERSION, wasmer-$VERSION"
  echo "    PATH (runners)    : rose-wasmtime-runner, rose-wasmer-runner"
  echo "    PATH (binaires)   : wasmtime, wasmer"
  printf '    which wasmtime    : %s\n' "$(command -v wasmtime 2>/dev/null || echo absent)"
  printf '    which wasmer      : %s\n' "$(command -v wasmer   2>/dev/null || echo absent)"
  echo
  echo "  Référence maison (crate de validation isolée, profil release,"
  echo "    interpréteur de pile wasm, 17/11/2026) : médiane ${REF_WASM} ms"
  echo "    6 échantillons : 171,4 / 171,8 / 172,0 / 172,2 / 172,4 / 172,5 ms"
  echo "    fib(27) = 196418"
  echo "  Référence maison (binaire natif ./rose/rose, commande du chantier) :"
  echo "    ${REF_NATIF} ms en haute résolution (autre chemin de code, JIT/natif)."
  echo "    La commande /usr/bin/time -f '%e' affiche 0.00 (résolution 10 ms)."
  echo
  echo "  Seuil M6.2 : fib(27) via runtime officiel ≤ 20 ms — NON MESURÉ, non revendiqué."
  echo "  Procédure après publication de l'artefact : rose/docs/PORTE_WASM.md"
  exit 0
fi

# ── 4. Présent : mesure A/A/A entrelacée, 6 échantillons, médiane ────
MOD=$(mktemp /tmp/rose-fib27-XXXXXX.wasm)
TIMES=$(mktemp /tmp/rose-fib27-times-XXXXXX)
ERRLOG=$(mktemp /tmp/rose-fib27-err-XXXXXX)
trap 'rm -f "$MOD" "$TIMES" "$TIMES.tri" "$ERRLOG"' EXIT
printf '%s' "$MOD_BASE64" | base64 -d > "$MOD" || {
  echo "ERREUR: décodage du module fib.wasm intégré"; exit 1;
}

echo "PORTE MESURABLE — runtime détecté : $RUNNER (source: $([ "$MODE" = runner ] && echo cache/PATH-runner || echo PATH-binaire))"
echo "  mode d'invocation : $MODE"
echo "  module : fib.wasm récursif i32->i32, 61 octets, md5 1ea8a09ca1186a7f9de48d2068f2dcba"
echo "  mesure : $NB_ECHANTILLONS échantillons médians (seuil ≤ 20 ms)"

run_sample() {
  case "$MODE" in
    runner)   "$RUNNER" "$MOD" fib 27 ;;
    wasmtime) "$RUNNER" run --invoke fib "$MOD" 27 ;;
    wasmer)   "$RUNNER" run "$MOD" --invoke fib -- 27 ;;
  esac
}

sortie_ok=1
i=1
while [ "$i" -le "$NB_ECHANTILLONS" ]; do
  t0=$(date +%s%N)
  sortie=$(run_sample 2>>"$ERRLOG")
  rc=$?
  t1=$(date +%s%N)
  if [ "$rc" -ne 0 ] || ! printf '%s\n' "$sortie" | grep -qw 196418; then
    echo "  échantillon $i : sortie inattendue (rc=$rc) : '$sortie'"
    sortie_ok=0
  fi
  echo "$((t1 - t0))" >> "$TIMES"
  i=$((i + 1))
done

sort -n "$TIMES" > "$TIMES.tri"
T3=$(sed -n '3p' "$TIMES.tri")
T4=$(sed -n '4p' "$TIMES.tri")
MEDIAN_NS=$(( (T3 + T4) / 2 ))
MEDIAN_MS=$(awk -v ns="$MEDIAN_NS" 'BEGIN { printf "%.3f", ns/1000000 }')

echo "  échantillons (ms) : $(awk '{printf "%.3f ", $1/1000000}' "$TIMES")"
echo "  médiane : ${MEDIAN_MS} ms"
echo "  sortie  : 196418 (identique à l'interpréteur) — $(
  [ "$sortie_ok" -eq 1 ] && echo OK || echo INCOHÉRENTE
)"
if [ "$MEDIAN_NS" -le "$SEUIL_NS" ] && [ "$sortie_ok" -eq 1 ]; then
  echo "VERDICT: PASS — porte ≤ 20 ms mesurée et validée."
else
  echo "VERDICT: ÉCHEC — médiane > 20 ms ou sortie incohérente ; porte non validée."
fi
