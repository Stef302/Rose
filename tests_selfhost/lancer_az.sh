#!/usr/bin/env bash
# AZ — batterie « nouveaux bigints » du compilateur auto-hébergé (A-Z).
#
#   ./tests_selfhost/lancer_az.sh [filtre_nom]
#
# Chaîne par programme : natif (ROSE_SANS_JIT=1) -> dump RIR (x2) ->
# ELF émis par l'émetteur figé (x2) -> sortie ELF (x2).
# Comparaisons octet pour octet : natif/ELF, RIR1/RIR2, ELF1/ELF2,
# sortie ELF 1/2. L'émetteur est figé par copie avant la salve : un autre
# agent peut modifier compilateur/emetteur_elf.rose pendant le mesurage.
#
# Variables : AZ_OUT (défaut /tmp/opencode/az_tests/battery),
#             AZ_EMETTEUR (défaut compilateur/emetteur_elf.rose).
# Classes : CONFORME (4 rc=0, 4 comparaisons égales),
#           DIVERGENT (4 rc=0, au moins une comparaison échoue),
#           BLOQUE (rc<>0, timeout ou artefact absent).
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ROSE="$ROOT/rose/rose"
TESTDIR="$ROOT/tests_selfhost"
OUT="${AZ_OUT:-/tmp/opencode/az_tests/battery}"
SRC_EMETTEUR="${AZ_EMETTEUR:-$ROOT/compilateur/emetteur_elf.rose}"
FILTRE="${1:-}"

mkdir -p "$OUT" || exit 1
SNAP="$OUT/emetteur_snapshot.rose"
cp "$SRC_EMETTEUR" "$SNAP" || exit 1
EM_MD5="$(md5sum "$SNAP" | cut -d' ' -f1)"
ROSE_MD5="$(md5sum "$ROSE" | cut -d' ' -f1)"

TSV="$OUT/resultats.tsv"
RAPPORT="$OUT/rapport.txt"
printf 'nom\tcategorie\tcause\tcmp\trc_nat\trc_rir\trc_emit\trc_elf\tmd5_rir1\tmd5_rir2\tmd5_elf1\tmd5_elf2\tmd5_nat\tmd5_elfout\tcmp_nat_elf\tcmp_rir\tcmp_elf\tcmp_elfout\n' > "$TSV"
: > "$OUT/_div.txt"; : > "$OUT/_blk.txt"

categorie() {
  case "$1" in
    t20[0-9]_*|t200_*|t201_*|t202_*|t203_*|t204_*|t205_*) echo "comparaisons_bigint" ;;
    t21[0-9]_*|t210_*|t211_*|t212_*|t213_*|t214_*|t215_*) echo "operations_mixtes" ;;
    t22[0-9]_*|t220_*|t221_*|t222_*|t223_*|t224_*|t225_*|t226_*) echo "debordements" ;;
    t23[0-9]_*|t230_*|t231_*|t232_*|t233_*) echo "negatifs" ;;
    t24[0-9]_*|t240_*|t241_*|t242_*|t243_*) echo "rationnels_calcul" ;;
    t25[0-9]_*|t250_*|t251_*|t252_*|t253_*) echo "unicode" ;;
    *) echo "autre" ;;
  esac
}

cause() {
  # Étiquette de la cause PROBÉE par le programme (nom de fichier), pas un
  # verdict automatique : la vérification se fait en lisant les sorties NAT/ELF.
  case "$1" in
    t20*|t224_*|t225_*) echo "comparaison_tag3_bigcmp_partiel" ;;
    t214_*|t231_*) echo "soustraction_bigint_ancien_chemin_64bits" ;;
    t215_*) echo "mul_div_mod_bigint_ancien_chemin_64bits" ;;
    t212_*|t232_*) echo "addition_bigadd_signes_opposes" ;;
    t21*) echo "addition_bigint" ;;
    t222_*|t226_*) echo "puissance_big_non_repliee_non_emise" ;;
    t22*) echo "promotion_ou_debordement" ;;
    t233_*) echo "abs_minmax_bigint" ;;
    t23*) echo "negatifs_bigint" ;;
    t24*) echo "division_calculee_ancien_chemin_64bits" ;;
    t25*) echo "unicode_octets_pas_points_de_code" ;;
    *) echo "non_classe" ;;
  esac
}

conforme=0; divergent=0; bloque=0; total=0
cmp_nat=0; cmp_rir=0; cmp_elf=0; cmp_out=0
debut="$(date +%s)"

for prog in "$TESTDIR"/t2[0-9][0-9]_*.rose; do
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
    printf '  %-34s %-44s %s\n' "$nom" "$etape" "$(printf '%s' "$err" | head -c 130)" >> "$OUT/_blk.txt"
  elif [ "$ok_nat" -eq 0 ]; then
    classe="DIVERGENT"; divergent=$((divergent+1))
    printf '### %s  [cause: %s]\n' "$nom" "$cau" >> "$OUT/_div.txt"
    paste <(head -n 8 "$base.nat") <(head -n 8 "$base.elfout") | sed 's/^/    NAT | ELF : /' >> "$OUT/_div.txt"
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
  echo "BATTERIE AZ (nouveaux bigints) — $(date -u +'%Y-%m-%dT%H:%M:%SZ') — durée $((fin-debut)) s"
  echo "émetteur figé : $SRC_EMETTEUR  md5=$EM_MD5"
  echo "binaire Rose   : $ROSE  md5=$ROSE_MD5"
  echo "référence : natif ROSE_SANS_JIT=1 ; dump RIR x2 ; ELF émis x2 ; sortie ELF x2"
  echo "comparaisons octet pour octet : natif/ELF, RIR1/RIR2, ELF1/ELF2, sortie1/sortie2"
  echo "tests=$total  CONFORMES=$conforme  DIVERGENTS=$divergent  BLOQUES=$bloque"
  echo "comparaisons égales : natif/ELF=$cmp_nat/$total  RIR=$cmp_rir/$total  ELF=$cmp_elf/$total  sortie=$cmp_out/$total"
  echo
  echo "=== PAR CATÉGORIE (nom -> conformes / divergents / bloqués) ==="
  awk -F'\t' 'NR>1 {c[$2"|"$4]++} END {for (k in c) print "  "k" -> "c[k]}' "$TSV" | sort
  echo
  echo "=== PAR CAUSE (divergents uniquement) ==="
  awk -F'\t' 'NR>1 && $4=="DIVERGENT" {c[$3]++} END {for (k in c) print "  "k" -> "c[k]}' "$TSV" | sort
  echo
  echo "=== DIVERGENCES : sorties NAT | ELF (8 premières lignes) ==="
  if [ -s "$OUT/_div.txt" ]; then cat "$OUT/_div.txt"; else echo "  (aucune)"; fi
  echo
  echo "=== BLOQUES (étape, rc, stderr) ==="
  if [ -s "$OUT/_blk.txt" ]; then cat "$OUT/_blk.txt"; else echo "  (aucun)"; fi
  echo
  echo "Détail machine : $TSV"
} > "$RAPPORT"

cat "$RAPPORT"
