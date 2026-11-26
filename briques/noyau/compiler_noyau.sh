#!/usr/bin/env bash
# compiler_noyau.sh — compile le noyau démo Rose en binaire machine NU x86-32.
#
#   bash briques/noyau/compiler_noyau.sh
#
# Concatène (Rose n'a pas de modules) : backend universel + source du noyau +
# lieur NU, puis exécute le tout par le moteur Rose sous le garde-fou mémoire.
# Aucun assembleur, aucun linker système, aucun runtime : l'émetteur Rose
# produit directement `briques/noyau/kernel32.bin`.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT" || exit 1

ROSE_BIN="${ROSE_BIN:-}"
if [[ -z "$ROSE_BIN" ]]; then
  for c in "$ROOT/rose/rose" /home/clausake/Stefan/ROSE/rose/rose \
           /home/clausake/Stefan/ROSE/rose/target/release/rose; do
    [[ -x "$c" ]] && { ROSE_BIN="$c"; break; }
  done
fi
[[ -x "$ROSE_BIN" ]] || { echo "ERREUR : moteur Rose introuvable" >&2; exit 1; }

SRC=/tmp/opencode/noyau_nu_cat.rose
cat compilateur/backend_universel.rose \
    briques/noyau/demo.rose \
    briques/noyau/lien_nu.rose > "$SRC" || exit 1

echo "[ Rose ] $ROSE_BIN"
echo "[ Source ] $SRC ($(wc -l < "$SRC") lignes concaténées)"
bash scripts/garde_fou_mem.sh 12000000 "$ROSE_BIN" "$SRC" || exit 1

BIN=briques/noyau/kernel32.bin
[[ -s "$BIN" ]] || { echo "ERREUR : $BIN absent" >&2; exit 1; }
echo "[ Binaire ] $BIN : $(stat -c%s "$BIN") octets"

# Preuve de désassemblage. La commande de référence est
#   objdump -D -b binary -mi386 $BIN
# mais CETTE machine est aarch64 : son objdump GNU ne contient pas i386
# (`objdump -i` ne liste que aarch64/arm). On bascule donc sur llvm-objdump,
# avec un objet elf32-i386 ENVELOPPANT le binaire NU (l'enveloppe sert UNIQUEMENT
# à l'outil, le fichier livré reste le .bin sans header).
if objdump -i 2>/dev/null | grep -qi 'i386'; then
  echo "[ Preuve ] objdump -D -b binary -mi386 $BIN"
  objdump -D -b binary -mi386 "$BIN"
else
  OBJ=/tmp/opencode/kernel32_i386.o
  echo "[ Preuve ] hôte aarch64 : objdump GNU sans i386," \
       "équivalent llvm-objcopy + llvm-objdump --triple=i386"
  llvm-objcopy -I binary -O elf32-i386 -B i386 "$BIN" "$OBJ" || exit 1
  llvm-objdump -D --triple=i386 --section=.data "$OBJ"
fi
