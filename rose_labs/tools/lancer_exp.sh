#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════════
# lancer_exp.sh — Lance une expérience d'optimisation sur une copie de Rose.
#
# Usage : lancer_exp.sh <nom_exp> [niveau_opt]
#
#   nom_exp       : jit_cranelift | aot_llvm_orc | tier1_copy_patch
#   niveau_opt    : O1 | O2 | O3 | pgo  (défaut : O2)
#
# Les résultats sont ajoutés à rose_labs/bench/resultats.csv
# ═══════════════════════════════════════════════════════════════════════════
set -euo pipefail

RACINE="$(cd "$(dirname "$0")/.." && pwd)"
PROJET_RACINE="$(cd "$RACINE/.." && pwd)"
EXP="$1"
NIVEAU="${2:-O2}"

case "$EXP" in
  jit_cranelift|aot_llvm_orc|tier1_copy_patch) ;;
  *)
    echo "Usage : $0 <jit_cranelift|aot_llvm_orc|tier1_copy_patch> [O1|O2|O3|pgo]"
    exit 1
    ;;
esac

case "$NIVEAU" in
  O1|O2|O3|pgo) ;;
  *)
    echo "✗ Niveau invalide : $NIVEAU (attendu : O1, O2, O3, pgo)"
    exit 1
    ;;
esac

BENCH="$PROJET_RACINE/bench/global/bench_global.rose"
if [ ! -f "$BENCH" ]; then
  echo "✗ Bench introuvable : $BENCH"
  exit 1
fi

RESULTATS="$RACINE/bench/resultats.csv"
mkdir -p "$RACINE/bench"

# En-tête du CSV si absent
if [ ! -f "$RESULTATS" ]; then
  echo "experience,niveau,temps_s,mem_kb,date" > "$RESULTATS"
fi

# ── Préparation de la copie de travail ───────────────────────────────────
COPIE="$RACINE/copies/rose_ref"
if [ ! -f "$COPIE/Cargo.toml" ]; then
  echo "→ Première copie de rose/ (ça peut prendre un moment)"
  bash "$RACINE/tools/copier_rose.sh"
fi

# ── Configuration du niveau d'optimisation ───────────────────────────────
# Cargo attend un chiffre (0, 1, 2, 3, s, z) pour opt-level
case "$NIVEAU" in
  O1) CARGO_OPT="1" ;;
  O2) CARGO_OPT="2" ;;
  O3) CARGO_OPT="3" ;;
  pgo) CARGO_OPT="2" ;;
esac
sed -i 's/^opt-level = .*/opt-level = '"$CARGO_OPT"'/' "$COPIE/Cargo.toml"

# Pour PGO, on ajoute les flags de profilage
PGO_FLAGS=""
if [ "$NIVEAU" = "pgo" ]; then
  PGO_FLAGS="-C link-arg=-fprofile-use"
  echo "⚠ PGO : assurez-vous d'avoir généré le profil d'abord (-fprofile-generate)"
fi

# ── Compilation ──────────────────────────────────────────────────────────
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "▶ Expérience : $EXP"
echo "   Niveau     : $NIVEAU"
echo "   Copie      : $COPIE"
echo "   Bench      : $BENCH"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

export PATH="/home/clausake/mingw/root/usr/bin:$PATH"
export PATH="$HOME/.cargo/bin:$PATH"

cd "$COPIE"

# Nettoyage partiel pour forcer la recompilation avec le nouveau niveau
cargo clean 2>/dev/null || true

TEMPS_COMPIL=$( { time cargo build --release 2>&1; } 2>&1 | grep -E "^real" | awk '{print $2}' )
if [ -z "$TEMPS_COMPIL" ]; then
  # time n'a pas affiché "real", on essaie autrement
  TEMPS_COMPIL=$( { time cargo build --release 2>&1; } 2>&1 | tail -1 )
fi
echo "   Temps compilation : $TEMPS_COMPIL"

# ── Exécution du bench ───────────────────────────────────────────────────
BIN="$COPIE/target/release/rose-20"
if [ ! -x "$BIN" ]; then
  echo "✗ Binaire introuvable : $BIN"
  exit 1
fi

MESURE=$(bash "$RACINE/tools/mesurer.sh" "bench_global ($NIVEAU)" "$BIN" "$BENCH" 2>&1 || true)
echo "   $MESURE"

# Extraction temps et mémoire pour le CSV
# Format de MESURE : "label   X.XXXs   YYYY KB"
TEMPS_EXEC=$(echo "$MESURE" | awk '{for(i=1;i<=NF;i++) if($i ~ /s$/) {gsub(/s/,"",$i); print $i; exit}}')
MEM_EXEC=$(echo "$MESURE" | awk '{for(i=1;i<=NF;i++) if($i ~ /KB/) {gsub(/KB/,"",$i); print $i; exit}}')

DATE=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
echo "$EXP,$NIVEAU,$TEMPS_EXEC,$MEM_EXEC,$DATE" >> "$RESULTATS"

echo "✓ Résultat ajouté à $RESULTATS"
