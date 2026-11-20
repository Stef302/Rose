#!/usr/bin/env bash
# SH9 — rejoue la batterie de tests self-host et produit un rapport factuel.
#
#   ./tests_selfhost/lancer.sh [filtre_nom]
#
# Variables d'environnement :
#   SH9_OUT        dossier des artefacts (défaut /tmp/opencode/sh9_agents/battery)
#   SH9_EMETTEUR   source Rose de l'émetteur ELF à figer (défaut compilateur/emetteur_elf.rose)
#
# Chaîne par programme : natif (ROSE_SANS_JIT=1) -> RIR -> ELF -> sortie ELF.
# Comparaisons octet pour octet : natif vs ELF, RIR×2, ELF×2.
# Classes : CONFORME (tout rc=0 et octets égaux), DIVERGENT (tout s'exécute
# mais les octets diffèrent), BLOQUE (rc<>0, timeout ou artifact manquant).
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ROSE="$ROOT/rose/rose"
TESTDIR="$ROOT/tests_selfhost"
FIXED="/tmp/opencode/sh9_agents/battery"
OUT="${SH9_OUT:-$FIXED}"
SRC_EMETTEUR="${SH9_EMETTEUR:-$ROOT/compilateur/emetteur_elf.rose}"
FILTRE="${1:-}"

mkdir -p "$OUT" "$FIXED" || exit 1
# Les programmes de E/S lisent/écrivent ces chemins absolus : les créer
# toujours, même si SH9_OUT est redéfini.
printf 'input SH9\n' > "$FIXED/input.txt"
printf 'fonction principale : test SH9\n' > "$FIXED/critique_entree.txt"

# Figer l'émetteur : un autre agent peut le modifier pendant la salve.
SNAP="$OUT/emetteur_snapshot.rose"
cp "$SRC_EMETTEUR" "$SNAP" || exit 1
EM_MD5="$(md5sum "$SNAP" | cut -d' ' -f1)"
ROSE_MD5="$(md5sum "$ROSE" | cut -d' ' -f1)"

TSV="$OUT/resultats.tsv"
RAPPORT="$OUT/rapport.txt"
printf 'nom\tclasse\trc_nat\trc_rir\trc_emit\trc_elf\tmd5_rir\tmd5_elf\tmd5_nat_out\tmd5_elf_out\n' > "$TSV"

conforme=0; divergent=0; bloque=0; total=0
: > "$OUT/_div.txt"; : > "$OUT/_blk.txt"

debut="$(date +%s)"
for prog in "$TESTDIR"/*.rose; do
  nom="$(basename "$prog" .rose)"
  if [ -n "$FILTRE" ] && [[ "$nom" != *"$FILTRE"* ]]; then continue; fi
  total=$((total+1))
  base="$OUT/$nom"
  args=()
  case "$nom" in
    t80_*|t81_*|t999_*) args=("$FIXED/critique_entree.txt") ;;
  esac

  ROSE_SANS_JIT=1 timeout 25 "$ROSE" "$prog" "${args[@]}" >"$base.nat" 2>"$base.nat.err"; rcn=$?
  timeout 25 "$ROSE" rir "$prog" >"$base.rir" 2>"$base.rir.err"; rcr=$?
  timeout 25 "$ROSE" rir "$prog" >"$base.rir2" 2>>"$base.rir.err"; rcr2=$?
  md5r=""; md5r2=""
  [ -s "$base.rir" ] && md5r="$(md5sum "$base.rir" | cut -d' ' -f1)"
  [ -s "$base.rir2" ] && md5r2="$(md5sum "$base.rir2" | cut -d' ' -f1)"

  rce=1; rce2=1; md5e=""; md5e2=""
  if [ "$rcr" -eq 0 ] && [ -s "$base.rir" ]; then
    timeout 90 "$ROSE" "$SNAP" "$base.rir" >"$base.elf" 2>"$base.elf.err"; rce=$?
    timeout 90 "$ROSE" "$SNAP" "$base.rir2" >"$base.elf2" 2>>"$base.elf.err"; rce2=$?
    [ -s "$base.elf" ] && md5e="$(md5sum "$base.elf" | cut -d' ' -f1)"
    [ -s "$base.elf2" ] && md5e2="$(md5sum "$base.elf2" | cut -d' ' -f1)"
  else
    : > "$base.elf"; : > "$base.elf2"; : > "$base.elf.err"
  fi

  rcx=1
  if [ "$rce" -eq 0 ] && [ -s "$base.elf" ]; then
    chmod +x "$base.elf"
    timeout 25 "$base.elf" "${args[@]}" >"$base.elfout" 2>"$base.elfout.err"; rcx=$?
  else
    : > "$base.elfout"; : > "$base.elfout.err"
  fi

  note=""
  etape=""
  if [ "$rcn" -ne 0 ]; then etape="natif(rc=$rcn)"; fi
  if [ "$rcr" -ne 0 ] || [ "$rcr2" -ne 0 ]; then etape="${etape:+$etape,}rir(rc=$rcr/$rcr2)"; fi
  if [ "$rce" -ne 0 ] || [ "$rce2" -ne 0 ]; then etape="${etape:+$etape,}emission(rc=$rce/$rce2)"; fi
  if [ "$rcx" -ne 0 ]; then etape="${etape:+$etape,}elf(rc=$rcx)"; fi
  if [ "$md5r" != "$md5r2" ] && [ -n "$md5r" ]; then etape="${etape:+$etape,}rir_non_deterministe"; fi
  if [ "$md5e" != "$md5e2" ] && [ -n "$md5e" ]; then etape="${etape:+$etape,}elf_non_deterministe"; fi

  if [ -n "$etape" ]; then
    classe="BLOQUE"; bloque=$((bloque+1))
    err="$(head -n 1 "$base.nat.err" 2>/dev/null)$(head -n 1 "$base.rir.err" 2>/dev/null)$(head -n 1 "$base.elf.err" 2>/dev/null)$(head -n 1 "$base.elfout.err" 2>/dev/null)"
    printf '  %-26s %-46s %s\n' "$nom" "$etape" "$(printf '%s' "$err" | head -c 150)" >> "$OUT/_blk.txt"
    note="$etape"
  elif ! cmp -s "$base.nat" "$base.elfout"; then
    classe="DIVERGENT"; divergent=$((divergent+1))
    nat="$(tr '\n' '|' < "$base.nat" | head -c 110)"
    elf="$(tr '\n' '|' < "$base.elfout" | head -c 110)"
    printf '  %-26s nat=%s\n  %-26s elf=%s\n' "$nom" "$nat" "" "$elf" >> "$OUT/_div.txt"
    note="sortie"
  else
    classe="CONFORME"; conforme=$((conforme+1))
  fi

  md5n="$(md5sum "$base.nat" | cut -d' ' -f1)"
  md5o="$(md5sum "$base.elfout" | cut -d' ' -f1)"
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$nom" "$classe" "$rcn" "$rcr" "$rce" "$rcx" "$md5r" "$md5e" "$md5n" "$md5o" \
    >> "$TSV"
done
fin="$(date +%s)"

{
  echo "BATTERIE SH9 — $(date -u +'%Y-%m-%dT%H:%M:%SZ') — durée $((fin-debut)) s"
  echo "émetteur figé : $SRC_EMETTEUR  md5=$EM_MD5"
  echo "binaire Rose md5=$ROSE_MD5"
  echo "référence : ROSE_SANS_JIT=1 (nat), dump RIR ×2, ELF émis ×2, sortie ELF"
  echo "comparaisons octet pour octet : natif vs ELF ; RIR1 vs RIR2 ; ELF1 vs ELF2"
  echo "tests=$total  CONFORMES=$conforme  DIVERGENTS=$divergent  BLOQUES=$bloque"
  echo
  echo "=== DIVERGENCES (sortie) ==="
  if [ -s "$OUT/_div.txt" ]; then cat "$OUT/_div.txt"; else echo "  (aucune)"; fi
  echo
  echo "=== BLOQUES (étape, rc, stderr) ==="
  if [ -s "$OUT/_blk.txt" ]; then cat "$OUT/_blk.txt"; else echo "  (aucun)"; fi
  echo
  echo "Détail machine : $TSV"
} > "$RAPPORT"

cat "$RAPPORT"
