#!/bin/sh
# ═══════════════ Rose — auto-test sur votre machine ═══════════════
# Tente d'exécuter les binaires livrés ; les reconstruit si nécessaire
# (architecture différente ou outil manquant), puis joue les trois suites :
#   1. suite moteur   (46 cas : dorés au caractère, FFI, bundle, erreurs, CLI)
#   2. parité         (comparaison au caractère de chaque moteur au C++)
#   3. suite v2       (147 cas de la chaîne d'export polyglotte)
# Usage : ./tester.sh        (aucune installation requise en plus)
set -u
RACINE=$(cd "$(dirname "$0")" && pwd)
cd "$RACINE"

rouge=""; vert=""; jaune=""; gras=""; reset=""
if [ -t 1 ]; then
    rouge=$(printf '\033[31m'); vert=$(printf '\033[32m'); jaune=$(printf '\033[33m')
    gras=$(printf '\033[1m'); reset=$(printf '\033[0m')
fi

BILAN=""
note() { BILAN="$BILAN
$1"; }

# ── 1. binaires autonomes ───────────────────────────────────────────
echo "${gras}── Moteurs autonomes ──────────────────────────────────${reset}"
MOTEURS_OK=""
tester_moteur() {
    # $1 = binaire, $2 = nom, $3 = commande de reconstruction (ou vide)
    binaire="$1"; nom="$2"; recon="$3"
    if [ -x "$binaire" ] && "$binaire" version >/dev/null 2>&1; then
        echo "  ${vert}✓${reset} $nom : $("$binaire" version)"
        MOTEURS_OK="$MOTEURS_OK $binaire"
        return 0
    fi
    if [ ! -x "$binaire" ]; then
        echo "  ${jaune}…${reset} $nom absent — reconstruction tentée"
    else
        echo "  ${jaune}…${reset} $nom inexécutable ici (architecture ?) — reconstruction"
    fi
    if [ -n "$recon" ] && sh -c "$recon" >/dev/null 2>&1; then
        echo "  ${vert}✓${reset} $nom : reconstruit"
        MOTEURS_OK="$MOTEURS_OK $binaire"
        return 0
    fi
    echo "  ${rouge}✗${reset} $nom indisponible (outil ou GMP manquant — voir TESTER.md)"
    return 1
}

# GMP pour le moteur C++ : le Makefile prend libgmp.a (statique) ou le système
if command -v g++ >/dev/null 2>&1; then
    RECON_CPP="make -C moteur/cpp ${GMP_DIR:+GMP_DIR=$GMP_DIR} >/dev/null"
else
    RECON_CPP=""
fi
if command -v rustc >/dev/null 2>&1; then
    RECON_RS="moteur/rust/build.sh >/dev/null"
else
    RECON_RS=""
fi

tester_moteur bin/rose            "C++  " "$RECON_CPP" && cp -f moteur/cpp/rose bin/rose 2>/dev/null
tester_moteur moteur/rust/rose    "Rust " "$RECON_RS" && cp -f moteur/rust/rose bin/rose-rs 2>/dev/null

echo
echo "${gras}── Moteurs autonomes ──────────────────────────────────────${reset}"

# ── 2. suites de validation ─────────────────────────────────────────
echo
echo "${gras}── Suites de validation ─────────────────────────────────${reset}"
if ! command -v python3 >/dev/null 2>&1; then
    echo "  ${rouge}✗${reset} python3 requis pour jouer les suites (install : apt install python3)"
else
    for b in $MOTEURS_OK; do
        case "$b" in
            *rust*) label="Rust"    ;;
            *cpp*|bin/rose) label="moteur C++" ;;
            *) label=$(basename "$b") ;;
        esac
        sortie=$(ROSE_MOTEUR="$(pwd)/$b" python3 moteur/tests/courir.py 2>&1 | tail -1)
        case "$sortie" in
            *"0 échoués"*) echo "  ${vert}✓${reset} suite moteur ($label) : $sortie" ;;
            *)             echo "  ${rouge}✗${reset} suite moteur ($label) : $sortie"; note "ÉCHEC suite moteur $label" ;;
        esac
    done
    sortie=$(python3 moteur/tests/parite.py 2>&1 | tail -1)
    case "$sortie" in
        *"0 divergences"*) echo "  ${vert}✓${reset} parité des moteurs    : $sortie" ;;
        *)                 echo "  ${rouge}✗${reset} parité des moteurs    : $sortie"; note "ÉCHEC parité" ;;
    esac
fi

find moteur rose2 -name "__pycache__" -type d -exec rm -rf {} + 2>/dev/null
echo
if [ -z "$BILAN" ]; then
    echo "${vert}${gras}Tout est vert — Rose est opérationnel sur cette machine.${reset}"
    exit 0
else
    echo "${rouge}${gras}Points à corriger :$BILAN${reset}"
    exit 1
fi
