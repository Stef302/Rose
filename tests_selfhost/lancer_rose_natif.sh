#!/usr/bin/env bash
# lancer_rose_natif.sh — batterie du compilateur COMPOSITE .rose -> ELF.
#
#   ./tests_selfhost/lancer_rose_natif.sh [filtre_nom]
#
# Chaîne par programme :
#   natif (ROSE_SANS_JIT=1)            -> référence
#   seed_rose rose_natif.rose prog.rose -> prog.elf (fron-end + backend en Rose)
#   ./prog.elf                         -> sortie comparée octet pour octet
#
# Variables : RN_OUT, RN_SEED (défaut $ROOT/rose/rose),
#             RN_COMPOSITE (défaut compilateur/rose_natif.rose).
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SEED="${RN_SEED:-$ROOT/rose/rose}"
COMP="${RN_COMPOSITE:-$ROOT/compilateur/rose_natif.rose}"
TESTDIR="$ROOT/tests_selfhost"
OUT="${RN_OUT:-/tmp/opencode/rose_natif/battery}"
FILTRE="${1:-}"
FIXED="/tmp/opencode/sh9_agents/battery"
mkdir -p "$OUT" "$FIXED" || exit 1
printf 'input SH9\n' > "$FIXED/input.txt"
printf 'fonction principale : test SH9\n' > "$FIXED/critique_entree.txt"

TSV="$OUT/resultats.tsv"
printf 'nom\tclasse\trc_nat\trc_emit\trc_elf\tmd5_elf\tmd5_nat\tmd5_sortie\n' > "$TSV"
: > "$OUT/_div.txt"; : > "$OUT/_blk.txt"
ok=0; div=0; blk=0; total=0
debut="$(date +%s)"

for prog in "$TESTDIR"/*.rose; do
  nom="$(basename "$prog" .rose)"
  [ -n "$FILTRE" ] && [[ "$nom" != *"$FILTRE"* ]] && continue
  total=$((total+1))
  base="$OUT/$nom"
  args=()
  case "$nom" in
    t80_*|t81_*|t999_*) args=("$FIXED/critique_entree.txt") ;;
  esac

  ROSE_SANS_JIT=1 timeout 60 "$SEED" "$prog" "${args[@]}" >"$base.nat" 2>"$base.nat.err"; rcn=$?
  timeout 300 "$SEED" "$COMP" "$prog" >"$base.elf" 2>"$base.elf.err"; rce=$?
  rcx=1
  if [ $rce -eq 0 ] && [ "$(od -An -tx1 -N4 "$base.elf" 2>/dev/null | tr -d ' \n')" = "7f454c46" ]; then
    chmod +x "$base.elf"
    timeout 30 "$base.elf" "${args[@]}" >"$base.elfout" 2>"$base.elfout.err"; rcx=$?
  fi

  etape=""
  [ $rcn -ne 0 ] && etape="natif(rc=$rcn)"
  [ $rce -ne 0 ] && etape="${etape:+$etape,}emission(rc=$rce)"
  [ $rcx -ne 0 ] && etape="${etape:+$etape,}elf(rc=$rcx)"
  if [ -n "$etape" ]; then
    classe=BLOQUE; blk=$((blk+1))
    printf '  %-26s %s %s\n' "$nom" "$etape" "$(head -n 1 "$base.elf.err" 2>/dev/null | head -c 110)" >> "$OUT/_blk.txt"
  elif ! cmp -s "$base.nat" "$base.elfout"; then
    classe=DIVERGENT; div=$((div+1))
    printf '  %-26s nat=%s | elf=%s\n' "$nom" "$(tr '\n' '|' <"$base.nat" | head -c 100)" "$(tr '\n' '|' <"$base.elfout" | head -c 100)" >> "$OUT/_div.txt"
  else
    classe=CONFORME; ok=$((ok+1))
  fi
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$nom" "$classe" "$rcn" "$rce" "$rcx" \
    "$(md5sum "$base.elf" 2>/dev/null | cut -d' ' -f1)" \
    "$(md5sum "$base.nat" 2>/dev/null | cut -d' ' -f1)" \
    "$(md5sum "$base.elfout" 2>/dev/null | cut -d' ' -f1)" >> "$TSV"
done
fin="$(date +%s)"

{
  echo "BATTERIE rose_natif — $(date -u +'%Y-%m-%dT%H:%M:%SZ') — durée $((fin-debut)) s"
  echo "compilateur composite : $COMP (md5 $(md5sum "$COMP" | cut -d' ' -f1))"
  echo "graine : $SEED (md5 $(md5sum "$SEED" | cut -d' ' -f1))"
  echo "tests=$total  CONFORMES=$ok  DIVERGENTS=$div  BLOQUES=$blk"
  echo "=== DIVERGENCES ==="; cat "$OUT/_div.txt"
  echo "=== BLOQUES ==="; cat "$OUT/_blk.txt"
  echo "Détail : $TSV"
} | tee "$OUT/rapport.txt"
