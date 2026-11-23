#!/usr/bin/env bash
# build-sans-rust.sh — compiler un corpus .rose en ELF natifs SANS binaire Rust.
#
# Principe : le seul outil de compilation appelé est un compilateur natif
# AUTO-HÉBERGÉ (ELF aarch64 produit par compilateur/rose_natif.rose). Aucun
# appel à rose/rose, aucun cargo, aucun compilateur externe. Les seuls outils
# annexes sont chmod/cp/md5sum/od/stat/timeout et, si --verifier est demandé,
# l'exécution directe des ELF produits.
#
# Contrat C0 : `C0 source.rose > sortie.elf` (l'ELF va sur stdout).
# Attention : si le front-end Rose du compilateur rejette la source, le
# composite retombe dans sa queue « vieux compilateur » et écrit un fichier
# `prog` dans le CWD courant tout en affichant un message sur stdout. Ce
# script lance donc C0 avec CWD = dossier de sortie et détecte ce cas par le
# magic : aucun fichier n'est créé dans le dépôt.
#
# Usage :
#   scripts/build-sans-rust.sh --compilateur C0 --sortie DOSSIER FICHIER.rose [...]
#   scripts/build-sans-rust.sh --compilateur C0 --sortie DOSSIER --verifier *.rose
#   scripts/build-sans-rust.sh --compilateur C0 --sortie DOSSIER \
#       --rapport resultats.tsv --timeout 300 FICHIER.rose [...]
#
# Options :
#   --verifier      exécuter chaque ELF produit (rc=0 attendu) et enregistrer
#                   sa première ligne de sortie dans le rapport.
#   --verif-source  J1 : appeler `C0 --verif SOURCE` au lieu de compiler.
#                   Chaque ligne du rapport est le verdict du vérificateur
#                   auto-hébergé ; aucune vérification Rust n'est appelée.
#   --reference B   OPTIONNEL et HORS du chemin Sans Rust : comparer la sortie
#                   à celle du binaire Rust B. Jamais utilisé par défaut.
#   --rapport F     écrire un TSV : nom, verdict, rc, taille, machine, sortie,
#                   référence, cause exacte de l'échec.
#   --timeout N     délai par compilation en secondes (défaut 900).
#
# Code de sortie : 0 si toutes les sources ont produit un ELF aarch64 valide
# (et, avec --verifier, si toutes les exécutions ont rc=0), 1 sinon.
set -uo pipefail

C0=""
OUT="/tmp/opencode/build-sans-rust"
VERIFY=0
MODE_VERIF=0
REF=""
RAPPORT=""
TLIMIT=900
while [[ $# -gt 0 ]]; do
  case "$1" in
    --compilateur) C0="${2:-}"; shift 2 ;;
    --sortie) OUT="${2:-}"; shift 2 ;;
    --verifier) VERIFY=1; shift ;;
    --verif-source) MODE_VERIF=1; shift ;;
    --reference) REF="${2:-}"; shift 2 ;;
    --rapport) RAPPORT="${2:-}"; shift 2 ;;
    --timeout) TLIMIT="${2:-}"; shift 2 ;;
    -h|--help) sed -n '2,40p' "${BASH_SOURCE[0]}"; exit 0 ;;
    -*) echo "OPTION INCONNUE : $1" >&2; exit 1 ;;
    *) break ;;
  esac
done

if [[ -z "$C0" || ! -x "$C0" ]]; then
  echo "ERREUR : --compilateur doit désigner un ELF exécutable auto-hébergé" >&2
  exit 1
fi
if [[ -n "$REF" && ! -x "$REF" ]]; then
  echo "ERREUR : --reference doit désigner un exécutable" >&2
  exit 1
fi
mkdir -p "$OUT" || exit 1
C0="$(realpath "$C0")"
if [[ -n "$RAPPORT" ]]; then : > "$RAPPORT"; fi
rapport() { [[ -n "$RAPPORT" ]] && printf '%s\n' "$*" >> "$RAPPORT"; }
rapport "nom	verdict	rc_comp	taille	machine	sortie	reference	cause"

# ── J1 : mode vérification, aucun ELF produit, aucun Rust appelé ────────
# `C0 --verif SOURCE` imprime « vérifié » ou le message d'erreur exact.
if [[ $MODE_VERIF -eq 1 ]]; then
  n=0; ok=0
  for src in "$@"; do
    [[ -r "$src" ]] || { echo "BLOQUÉ $src : source illisible"; n=$((n+1)); continue; }
    src="$(realpath "$src")"
    nom="$(basename "$src" .rose)"
    n=$((n+1))
    verdict="$(timeout "$TLIMIT" "$C0" --verif "$src" 2>"$OUT/$nom.err")"
    crc=$?
    if [[ $crc -eq 0 && "$verdict" == "vérifié" ]]; then
      ok=$((ok+1)); classe=CONFORME
    elif [[ $crc -eq 0 ]]; then
      classe=REJETE
    else
      classe=BLOQUE
    fi
    echo "$classe $nom : $(echo "$verdict" | head -n 1 | cut -c1-160)"
    rapport "$nom	$classe	$crc	-	-	$(echo "$verdict" | head -n 1 | cut -c1-160)	-	-"
  done
  echo "RÉSULTAT : $ok/$n sources vérifiées Sans Rust (compilateur $(md5sum "$C0" | cut -d' ' -f1))"
  [[ $ok -eq $n && $n -gt 0 ]]
  exit $?
fi

n=0; ok=0
for src in "$@"; do
  [[ -r "$src" ]] || { echo "BLOQUÉ $src : source illisible"; n=$((n+1)); continue; }
  src="$(realpath "$src")"
  nom="$(basename "$src" .rose)"
  elf="$OUT/$nom.elf"
  err="$OUT/$nom.err"
  n=$((n+1))
  : > "$elf"; : > "$err"
  # CWD = sortie : le repli « vieux compilateur » ne pollue pas le dépôt.
  (cd "$OUT" && timeout "$TLIMIT" "$C0" "$src" > "$elf" 2> "$err")
  crc=$?
  magic="$(od -An -tx1 -N4 "$elf" 2>/dev/null | tr -d ' \n')"
  machine="$(od -An -tu2 -j18 -N2 "$elf" 2>/dev/null | tr -d ' \n')"
  cause=""
  if [[ $crc -ne 0 ]]; then
    cause="compilation rc=$crc; $(tail -n 1 "$err" | cut -c1-120)"
  fi
  if [[ "$magic" != "7f454c46" || "$machine" != "183" ]]; then
    # Message du repli (front-end) en premier : c'est la cause exacte.
    cause="${cause:-$(head -n 1 "$elf" | cut -c1-160)}"
    cause="${cause:-$(head -n 1 "$err" | cut -c1-160)}"
    [[ -n "$cause" ]] || cause="sortie non ELF (magic=$magic machine=$machine)"
    echo "BLOQUÉ $nom : $cause"
    rapport "$nom	INVALIDE	$crc	$(stat -c%s "$elf")	$machine	-	-	$cause"
    continue
  fi
  chmod 755 "$elf"
  sortie="-"; reference="-"; verdict="OK"
  if [[ $VERIFY -eq 1 ]]; then
    if timeout 60 "$elf" >"$OUT/$nom.out" 2>"$OUT/$nom.run.err"; then
      sortie="$(head -n 1 "$OUT/$nom.out" | cut -c1-120)"
      if [[ -n "$REF" ]]; then
        reference="$(timeout 60 "$REF" "$src" 2>/dev/null | head -n 1 | cut -c1-120)"
        [[ "$sortie" == "$reference" ]] || verdict="DIVERGENT"
      fi
    else
      code=$?
      verdict="EXEC_ECHEC"
      cause="exécution rc=$code : $(head -n 1 "$OUT/$nom.run.err" | cut -c1-120)"
    fi
  fi
  [[ "$verdict" == "OK" ]] && ok=$((ok+1))
  echo "$verdict $nom : $elf ($(stat -c%s "$elf") o), ELF aarch64 machine=$machine${sortie:+ sortie='$sortie'}${cause:+ cause='$cause'}"
  rapport "$nom	$verdict	$crc	$(stat -c%s "$elf")	$machine	$sortie	$reference	$cause"
done

echo "RÉSULTAT : $ok/$n artefacts ELF natifs produits Sans Rust (compilateur $(md5sum "$C0" | cut -d' ' -f1))"
[[ $ok -eq $n && $n -gt 0 ]]
