#!/bin/bash
# os-rose.sh — génère et lance le noyau amorçable écrit en Rose pur.
# Constat daté : 26/11/2026.
#
# 1. `rose os-rose.rose` écrit /tmp/os-rose.elf (ELF32 i386, multiboot v1)
#    en Rose pur : en-tête ELF, header multiboot, code machine, tout est
#    produit par ce programme via `ecrire_bytes` + `octet_brut`.
# 2. qemu-system-x86_64 -kernel charge et exécute le noyau.
# Aucun assembleur, aucun linker, aucun compilateur externe.
set -e
RACINE=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$RACINE"
OUT=${1:-/tmp/os-rose.elf}
./rose-20 interprete os-rose.rose >/dev/null
# os-rose.rose écrit toujours /tmp/os-rose.elf ; on le copie si demandé
if [ "$OUT" != "/tmp/os-rose.elf" ]; then cp /tmp/os-rose.elf "$OUT"; fi
echo "noyau : $OUT ($(wc -c < "$OUT") octets)"
echo "lancement qemu (Ctrl-A puis X pour quitter) :"
exec qemu-system-x86_64 -kernel "$OUT" -nographic
