#!/usr/bin/env bash
# build-sans-rust.sh — compiler des sources Rose en ELF natifs SANS binaire Rust.
#
# Principe : le seul outil appelé est un compilateur natif AUTO-HÉBERGÉ
# (ELF aarch64 produit par compilateur/rose_natif.rose). Aucun appel à
# rose/rose, aucun cargo, aucun outil externe autre que chmod/cp/md5sum.
#
# Usage :
#   scripts/build-sans-rust.sh --compilateur CHEMIN_C0 --sortie DOSSIER FICHIER.rose [...]
#   scripts/build-sans-rust.sh --verifier --compilateur CHEMIN_C0 --sortie DOSSIER
#
# Le compilateur C0 reçoit le chemin source en argument et écrit l'ELF sur
# stdout (contrat de compilateur/rose_natif.rose). Chaque sortie est vérifiée :
# magic ELF, machine AArch64, puis exécution avec comparaison facultative à
# une référence produite par le binaire Rust (hors de ce script).
set -uo pipefail

C0=""
OUT="/tmp/opencode/build-sans-rust"
VERIFY=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --compilateur) C0="${2:-}"; shift 2 ;;
    --sortie) OUT="${2:-}"; shift 2 ;;
    --verifier) VERIFY=1; shift ;;
    -h|--help) sed -n '2,14p' "${BASH_SOURCE[0]}"; exit 0 ;;
    -*) echo "OPTION INCONNUE : $1" >&2; exit 1 ;;
    *) break ;;
  esac
done

if [[ -z "$C0" || ! -x "$C0" ]]; then
  echo "ERREUR : --compilateur doit désigner un ELF exécutable auto-hébergé" >&2
  exit 1
fi
mkdir -p "$OUT" || exit 1

n=0; ok=0
for src in "$@"; do
  [[ -r "$src" ]] || { echo "BLOQUÉ $src : source illisible"; n=$((n+1)); continue; }
  nom="$(basename "$src" .rose)"
  elf="$OUT/$nom.elf"
  n=$((n+1))
  if ! timeout 900 "$C0" "$src" >"$elf" 2>"$OUT/$nom.err"; then
    echo "BLOQUÉ $nom : émission rc=$? ($(tail -n 1 "$OUT/$nom.err" | cut -c1-120))"
    continue
  fi
  magic="$(od -An -tx1 -N4 "$elf" 2>/dev/null | tr -d ' \n')"
  machine="$(od -An -tu2 -j18 -N2 "$elf" 2>/dev/null | tr -d ' \n')"
  if [[ "$magic" != "7f454c46" || "$machine" != "183" ]]; then
    echo "BLOQUÉ $nom : n'est pas un ELF aarch64 (magic=$magic machine=$machine)"
    continue
  fi
  chmod 755 "$elf"
  if [[ $VERIFY -eq 1 ]]; then
    if timeout 60 "$elf" >"$OUT/$nom.out" 2>"$OUT/$nom.run.err"; then
      echo "OK $nom : $elf ($(stat -c%s "$elf") o), exécution rc=0, sortie='$(head -n 1 "$OUT/$nom.out" | cut -c1-80)'"
      ok=$((ok+1))
    else
      echo "BLOQUÉ $nom : ELF émis mais exécution rc=$?"
    fi
  else
    echo "OK $nom : $elf ($(stat -c%s "$elf") o), ELF aarch64"
    ok=$((ok+1))
  fi
done

echo "RÉSULTAT : $ok/$n artefacts ELF natifs produits Sans Rust (compilateur $(md5sum "$C0" | cut -d' ' -f1))"
[[ $ok -eq $n && $n -gt 0 ]]
