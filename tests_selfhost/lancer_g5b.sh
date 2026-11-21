#!/usr/bin/env bash
# G5B — batterie indépendante de validation de l'émetteur ELF auto-hébergé.
#
#   ./tests_selfhost/lancer_g5b.sh [filtre_nom]
#
# 30 programmes : 20 ciblant les 14 divergences documentées dans
# rose/docs/AZ_TESTS.md (g5b5xx) + 10 non-régression (g5b6xx).
#
# Chaîne par programme : natif (ROSE_SANS_JIT=1) -> dump RIR (x2) ->
# ELF émis par l'émetteur figé par copie (x2) -> sortie ELF (x2).
# Comparaisons octet pour octet : natif/ELF, RIR1/RIR2, ELF1/ELF2,
# sortie ELF 1/2. L'émetteur est copié avant la salve : un autre agent
# peut modifier compilateur/emetteur_elf.rose pendant le mesurage.
#
# Classes : CONFORME (4 rc=0, 4 comparaisons égales),
#           DIVERGENT (4 rc=0, natif != ELF),
#           BLOQUE (rc<>0, timeout, artefact absent ou non-déterminisme).
#
# Variables : G5B_OUT (défaut /tmp/opencode/g5b_tests/battery),
#             G5B_ROSE (défaut rose/target/release/rose),
#             G5B_EMETTEUR (défaut compilateur/emetteur_elf.rose).
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ROSE="${G5B_ROSE:-$ROOT/rose/target/release/rose}"
TESTDIR="$ROOT/tests_selfhost"
OUT="${G5B_OUT:-/tmp/opencode/g5b_tests/battery}"
SRC_EMETTEUR="${G5B_EMETTEUR:-$ROOT/compilateur/emetteur_elf.rose}"
FILTRE="${1:-}"

mkdir -p "$OUT" || exit 1
SNAP="$OUT/emetteur_snapshot.rose"
cp "$SRC_EMETTEUR" "$SNAP" || exit 1
EM_MD5="$(md5sum "$SNAP" | cut -d' ' -f1)"
ROSE_MD5="$(md5sum "$ROSE" | cut -d' ' -f1)"

TSV="$OUT/resultats.tsv"
RAPPORT="$OUT/rapport.txt"
printf 'nom\tcategorie\tcause\tclasse\trc_nat\trc_rir\trc_emit\trc_elf\tmd5_rir1\tmd5_rir2\tmd5_elf1\tmd5_elf2\tmd5_nat\tmd5_elfout\tcmp_nat_elf\tcmp_rir\tcmp_elf\tcmp_elfout\n' > "$TSV"
: > "$OUT/_div.txt"; : > "$OUT/_blk.txt"

categorie() {
  case "$1" in
    g5b50[1-5]_*|g5b505_*) echo "unicode" ;;
    g5b51[0-3]_*|g5b513_*) echo "rationnel_calcule" ;;
    g5b52[0-2]_*|g5b522_*) echo "puissance_bigint" ;;
    g5b53[0-2]_*|g5b532_*) echo "soustraction_bigint" ;;
    g5b54[0-1]_*|g5b541_*) echo "mul_div_mod_bigint" ;;
    g5b55[0-1]_*|g5b551_*) echo "abs_minmax_bigint" ;;
    g5b560_*) echo "addition_signes_opposes" ;;
    g5b6*) echo "non_regression" ;;
    *) echo "autre" ;;
  esac
}

cause() {
  case "$1" in
    g5b50*) echo "unicode_octets_pas_points_de_code" ;;
    g5b51*) echo "division_calculee_ancien_chemin_64bits" ;;
    g5b52*) echo "puissance_big_non_repliee_non_emise" ;;
    g5b53*) echo "soustraction_bigint_ancien_chemin_64bits" ;;
    g5b540_*) echo "mul_bigint_ancien_chemin_64bits" ;;
    g5b541_*) echo "divmod_bigint_ancien_chemin_64bits" ;;
    g5b55*) echo "abs_minmax_bigint" ;;
    g5b560_*) echo "addition_bigadd_signes_opposes" ;;
    g5b6*) echo "non_regression" ;;
    *) echo "non_classe" ;;
  esac
}

conforme=0; divergent=0; bloque=0; total=0
cmp_nat=0; cmp_rir=0; cmp_elf=0; cmp_out=0
debut="$(date +%s)"

for prog in "$TESTDIR"/g5b5*.rose "$TESTDIR"/g5b6*.rose; do
  [ -e "$prog" ] || continue
  nom="$(basename "$prog" .rose)"
  if [ -n "$FILTRE" ] && [[ "$nom" != *"$FILTRE"* ]]; then continue; fi
  total=$((total+1))
  base="$OUT/$nom"
  cat_="$(categorie "$nom")"
  cau="$(cause "$nom")"

  ROSE_SANS_JIT=1 timeout 25 "$ROSE" "$prog" >"$base.nat" 2>"$base.nat.err"; rcn=$?
  timeout 25 "$ROSE" rir "$prog" >"$base.rir" 2>"$base.rir.err"; rcr=$?
  timeout 25 "$ROSE" rir "$prog" >"$base.rir2" 2>>"$base.rir.err"; rcr2=$?

  md5r=""; md5r2=""
  [ -s "$base.rir" ] && md5r="$(md5sum "$base.rir" | cut -d' ' -f1)"
  [ -s "$base.rir2" ] && md5r2="$(md5sum "$base.rir2" | cut -d' ' -f1)"

  rce=1; rce2=1; md5e=""; md5e2=""
  if [ "$rcr" -eq 0 ] && [ "$rcr2" -eq 0 ] && [ -s "$base.rir" ] && [ -s "$base.rir2" ]; then
    timeout 90 "$ROSE" "$SNAP" "$base.rir" >"$base.elf" 2>"$base.elf.err"; rce=$?
    timeout 90 "$ROSE" "$SNAP" "$base.rir2" >"$base.elf2" 2>>"$base.elf.err"; rce2=$?
    [ -s "$base.elf" ] && md5e="$(md5sum "$base.elf" | cut -d' ' -f1)"
    [ -s "$base.elf2" ] && md5e2="$(md5sum "$base.elf2" | cut -d' ' -f1)"
  else
    : > "$base.elf"; : > "$base.elf2"; : > "$base.elf.err"
  fi

  rcx=1; rcx2=1
  if [ "$rce" -eq 0 ] && [ "$rce2" -eq 0 ] && [ -s "$base.elf" ] && [ -s "$base.elf2" ]; then
    chmod +x "$base.elf" "$base.elf2"
    timeout 25 "$base.elf" >"$base.elfout" 2>"$base.elfout.err"; rcx=$?
    timeout 25 "$base.elf2" >"$base.elfout2" 2>>"$base.elfout.err"; rcx2=$?
  else
    : > "$base.elfout"; : > "$base.elfout2"; : > "$base.elfout.err"
  fi

  ok_nat=0; ok_rir=0; ok_elf=0; ok_out=0
  cmp -s "$base.nat" "$base.elfout" && ok_nat=1
  cmp -s "$base.rir" "$base.rir2" && ok_rir=1
  cmp -s "$base.elf" "$base.elf2" && ok_elf=1
  cmp -s "$base.elfout" "$base.elfout2" && ok_out=1
  [ "$ok_nat" -eq 1 ] && cmp_nat=$((cmp_nat+1))
  [ "$ok_rir" -eq 1 ] && cmp_rir=$((cmp_rir+1))
  [ "$ok_elf" -eq 1 ] && cmp_elf=$((cmp_elf+1))
  [ "$ok_out" -eq 1 ] && cmp_out=$((cmp_out+1))

  etape=""
  [ "$rcn" -ne 0 ] && etape="${etape:+$etape,}natif($rcn)"
  { [ "$rcr" -ne 0 ] || [ "$rcr2" -ne 0 ]; } && etape="${etape:+$etape,}rir($rcr/$rcr2)"
  { [ "$rce" -ne 0 ] || [ "$rce2" -ne 0 ]; } && etape="${etape:+$etape,}emission($rce/$rce2)"
  { [ "$rcx" -ne 0 ] || [ "$rcx2" -ne 0 ]; } && etape="${etape:+$etape,}elf($rcx/$rcx2)"
  [ "$ok_rir" -eq 0 ] && etape="${etape:+$etape,}rir_non_deterministe"
  [ "$ok_elf" -eq 0 ] && etape="${etape:+$etape,}elf_non_deterministe"
  [ "$ok_out" -eq 0 ] && etape="${etape:+$etape,}elfout_non_deterministe"

  if [ -n "$etape" ]; then
    classe="BLOQUE"; bloque=$((bloque+1))
    err="$(head -n 1 "$base.nat.err" 2>/dev/null)$(head -n 1 "$base.rir.err" 2>/dev/null)$(head -n 1 "$base.elf.err" 2>/dev/null)$(head -n 1 "$base.elfout.err" 2>/dev/null)"
    printf '  %-40s %-46s %s\n' "$nom" "$etape" "$(printf '%s' "$err" | head -c 130)" >> "$OUT/_blk.txt"
  elif [ "$ok_nat" -eq 0 ]; then
    classe="DIVERGENT"; divergent=$((divergent+1))
    printf '### %s  [categorie: %s | cause: %s]\n' "$nom" "$cat_" "$cau" >> "$OUT/_div.txt"
    paste <(head -n 12 "$base.nat") <(head -n 12 "$base.elfout") | sed 's/^/    NAT | ELF : /' >> "$OUT/_div.txt"
    printf '    md5 nat=%s  elf=%s\n' "$(md5sum "$base.nat" | cut -d' ' -f1)" "$(md5sum "$base.elfout" | cut -d' ' -f1)" >> "$OUT/_div.txt"
  else
    classe="CONFORME"; conforme=$((conforme+1))
  fi

  md5n="$(md5sum "$base.nat" | cut -d' ' -f1)"
  md5o="$(md5sum "$base.elfout" | cut -d' ' -f1)"
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$nom" "$cat_" "$cau" "$classe" "$rcn" "$rcr" "$rce" "$rcx" \
    "$md5r" "$md5r2" "$md5e" "$md5e2" "$md5n" "$md5o" \
    "$ok_nat" "$ok_rir" "$ok_elf" "$ok_out" >> "$TSV"
done
fin="$(date +%s)"

{
  echo "BATTERIE G5B (validation emetteur, 14 divergences AZ + non-regression) — $(date -u +'%Y-%m-%dT%H:%M:%SZ') — duree $((fin-debut)) s"
  echo "binaire Rose   : $ROSE  md5=$ROSE_MD5"
  echo "emetteur figé  : $SRC_EMETTEUR  md5=$EM_MD5"
  echo "reference : natif ROSE_SANS_JIT=1 ; dump RIR x2 ; ELFemis x2 ; sortie ELF x2"
  echo "comparaisons octet pour octet : natif/ELF, RIR1/RIR2, ELF1/ELF2, sortie1/sortie2"
  echo "tests=$total  CONFORMES=$conforme  DIVERGENTS=$divergent  BLOQUES=$bloque"
  echo "comparaisons egales : natif/ELF=$cmp_nat/$total  RIR=$cmp_rir/$total  ELF=$cmp_elf/$total  sortie=$cmp_out/$total"
  echo
  echo "=== PAR CATEGORIE (nom -> conformes / divergents / bloques) ==="
  awk -F'\t' 'NR>1 {c[$2"|"$4]++} END {for (k in c) print "  "k" -> "c[k]}' "$TSV" | sort
  echo
  echo "=== PAR CAUSE (divergents uniquement) ==="
  awk -F'\t' 'NR>1 && $4=="DIVERGENT" {c[$3]++} END {for (k in c) print "  "k" -> "c[k]}' "$TSV" | sort
  echo
  echo "=== DIVERGENCES : sorties NAT | ELF (12 premieres lignes) ==="
  if [ -s "$OUT/_div.txt" ]; then cat "$OUT/_div.txt"; else echo "  (aucune)"; fi
  echo
  echo "=== BLOQUES (etape, rc, stderr) ==="
  if [ -s "$OUT/_blk.txt" ]; then cat "$OUT/_blk.txt"; else echo "  (aucun)"; fi
  echo
  echo "Detail machine : $TSV"
} > "$RAPPORT"

cat "$RAPPORT"
