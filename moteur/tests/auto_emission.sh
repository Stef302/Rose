#!/bin/sh
# Porte d'auto-émission M4.4 : le traducteur RIR -> C++ écrit en Rose
# (rose/emetteur_cpp.rose) relit son PROPRE RIR et doit rendre le .cpp
# que produit l'oracle « rose build » — à l'octet près.
# Usage : ./moteur/tests/auto_emission.sh   (GMP via ROSE_GMP)
set -e
RACINE=$(cd "$(dirname "$0")/../.." && pwd)
cd "$RACINE"
G=${ROSE_GMP:-/tmp/opencode/gmp-linux}
B=${AUTOEM_DIR:-/tmp/opencode/autoem}
mkdir -p "$B"
echo "— oracle : AOT du traducteur ($B/emis, écrit $B/emis.cpp)"
ROSE_GMP=$G moteur/cpp/rose build rose/emetteur_cpp.rose -o "$B/emis" > "$B/build.log" 2>&1
echo "— RIR du traducteur (régénéré ici : les labels portent les numéros de lignes de la source)"
moteur/cpp/rose rir rose/emetteur_cpp.rose > "$B/em.rir"
echo "— auto-émission native (le binaire AOT du traducteur traduisant son propre RIR)"
"$B/emis" "$B/em.rir" > "$B/em.cpp"
if cmp "$B/emis.cpp" "$B/em.cpp"; then
  echo "AUTO-ÉMISSION OCTET-IDENTIQUE ($(wc -c < "$B/em.cpp") octets)"
else
  echo "DIVERGENCE :"; diff "$B/emis.cpp" "$B/em.cpp" | head -20; exit 1
fi
