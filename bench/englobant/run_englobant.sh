#!/usr/bin/env bash
# ─── BANC ENGLOBANT ROSE (VÉRIFIÉ) — passerelle de vérification ────────────
#
# Exerce TOUTE la syntaxe Rose dans des boucles chaudes et compare :
#   rose (interpréteur/JIT) | AOT natif | AOT --llvm | C++ (g++ -O3
#   -march=native) | Rust (rustc -O3 -C target-cpu=native) | python3 (réf).
# Les jumeaux sont EXACTS : mêmes formules, mêmes bornes, même largeur native,
# MÊME ARITHMÉTIQUE (FMA désactivés partout : -ffp-contract=off, jamais de
# fast-math — sinon la divergence serait un artefact du compilateur, pas une
#   divergence du LANGAGE) et MÊME FORMAT D'AFFICHAGE (flottants en forme courte
#   fidèle, comme l'afficheur « %.*g » de l'interpréteur). ──
#
# Usage :
#   ./run_englobant.sh                      # N = 4096 (l'échelle du banc)
#   ./run_englobant.sh 262144               # N plus grand (x64 la charge)
#   ROSE=/chemin/rose ./run_englobant.sh    # moteur Rose Labs (expérimental)
#   OUT=/tmp/bench_suite/n.csv ./run_englobant.sh
#
# « VÉRIFIÉ » = zéro BUILD FAIL + zéro FAIL + zéro DIVERGENT : les 4 lignes de
# valeurs (d45+d55, nb5, MC, tot7) de CHAQUE moteur doivent être IDENTIQUES à la
# référence (moteur Rose ; sinon jumeau C++ si le moteur est cassé) ; 5 passes,
# minimum retenu. DIVERGENT = banc ROUGE (exit 1).
#
# Pourquoi N = 4096 par défaut : le banc mesure des DÉBITS, pas des temps
#   morts. A N = 1 000 000, S4 seul coute ~2,5 s par passe et par moteur ; x5
#   passes x 6 moteurs ≈ 11 minutes pour UNE mesure, et chaque moteur travaillerait
#   a une echelle differente du voisin (les caches ne tiennent plus pareil) : une
#   divergence n'y prouverait plus une sémantique, elle prouverait un cache. A
#   N = 4096 le banc reste une épreuve VRAIE (8 ops flottantes × 4096 × 5 =
#   1,6 × 10⁸ additions en S4 seul ; 8192 blocs Philox ; 10⁶ tirages Lemire) et
#   il tient dans la machine. ──
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.."; pwd)"
ROSE="${ROSE:-$ROOT/moteur/cpp/rose}"
N="${1:-4096}"
RUNS=5
OUT="${OUT:-$HERE/results.csv}"
export LC_ALL=C

if [ ! -x "$ROSE" ]; then
    echo "ROSE introuvable : $ROSE (définir ROSE=/chemin/rose)"
    exit 2
fi
# Le build AOT/LLVM relit le runtime ; ROSE_HOME = src/ quand le binaire est
#   ailleurs que dans l'arbre source. ──
ROSE_DIR="$(cd "$(dirname "$(readlink -f "$ROSE")")" && pwd)"
if [ -f "$ROSE_DIR/../src/rose_runtime.cpp" ]; then
    export ROSE_HOME="$(cd "$ROSE_DIR/../src" && pwd)"
elif [ -f "$ROSE_DIR/rose_runtime.cpp" ]; then
    export ROSE_HOME="$ROSE_DIR"
fi
export ROSE
# Une CHAÎNE D'OUTILS UNIQUE pour les moteurs AOT et LLVM : une divergence
#   AOT/LLVM ne peut plus être un artefact d'outillage. ──
if command -v clang++ >/dev/null 2>&1; then
    export ROSE_CXX="$(command -v clang++)"
elif command -v g++ >/dev/null 2>&1; then
    export ROSE_CXX="g++"
fi

echo "── banc englobant : N=$N, passes=$RUNS, moteur=$ROSE (ROSE_HOME=${ROSE_HOME:-défaut}, CXX=${ROSE_CXX:-défaut}) ──"

# ── phase 1 : BUILDS (le banc se compile AVEC ses jumeaux) ──
rm -f "$HERE/.b_aot" "$HERE/.b_llvm" "$HERE/.b_cpp" "$HERE/.b_rs"
echo "── builds ──"
BFAIL=""
"$ROSE" build "$HERE/bench_englobant.rose" -o "$HERE/.b_aot"         >/dev/null 2>&1 || true
"$ROSE" build "$HERE/bench_englobant.rose" --llvm -o "$HERE/.b_llvm" >/dev/null 2>&1 || true
# un build réussi = un binaire EXECUTABLE (le .cpp/.ll tronqué ne suffit pas)
[ -x "$HERE/.b_aot" ]  || BFAIL="$BFAIL aot"
[ -x "$HERE/.b_llvm" ] || BFAIL="$BFAIL llvm"
if command -v g++ >/dev/null 2>&1; then
    g++ -O3 -march=native -ffp-contract=off -std=c++17 -pthread "$HERE/bench_englobant.cpp" -o "$HERE/.b_cpp" 2>/dev/null || BFAIL="$BFAIL cpp"
else
    echo "  g++ absent — jumeau C++ sauté (référence de parité indisponible)"
    BFAIL="$BFAIL cpp"
fi
RUSTC="$(command -v rustc || true)"
[ -z "$RUSTC" ] && [ -x "$HOME/.cargo/bin/rustc" ] && RUSTC="$HOME/.cargo/bin/rustc"
if [ -n "$RUSTC" ]; then
    "$RUSTC" -C opt-level=3 -C target-cpu=native -C overflow-checks=off -C debug-assertions=off "$HERE/bench_englobant.rs" -o "$HERE/.b_rs" 2>/dev/null || BFAIL="$BFAIL rs"
else
    echo "  rustc absent — moteur Rust sauté"
fi
if [ -n "$BFAIL" ]; then echo "  BUILD FAIL :$BFAIL" ; fi

# ── phase 2 : PARITÉ (1 passe à l'échelle N) — les 4 lignes de VALEURS de
#   chaque moteur, comparées à la référence (= moteur Rose lui-même ; sinon
#   jumeau C++ si le moteur est cassé). La 5e ligne = un TEMPS, jamais comparée :
#   elle est donc COUPÉE AVANT la comparaison (et non l'inverse : « head -5 »
#   comparait le temps, ce qui fabriquait des DIVERGENTS faux). ──
TMPD="$(mktemp -d)"
REF="$TMPD/ref.out"
GOT="$TMPD/got.out"
PARITE=0
printf "kernel,moteur,secondes,rc\n" > "$OUT"
echo "── parité (N=$N ; 4 valeurs par moteur vs référence Rose) ──"

# $1 = moteur, $2 = commande COMPLÈTE, $3 = fichier de COMMANDES (« - » = la
#   commande elle-même est le programme à lancer : le AOT et le LLVM écrivent
#   leur .cpp/.ll À CÔTÉ DU BINAIRE ET LES RELISENT — un chemin relatif « ./x.rose »
#   y chercherait « <répertoire-du-binaire>/x.rose » = introuvable, et le build
#   échouerait en silence). ──
check() {
    local mote="$1" cmd="$2" srcf="${3:-}"
    if [ ! -s "$REF" ]; then
        echo "  $mote : parité NON ÉTABLIE (référence absente)"
        PARITE=1
        return
    fi
    timeout 300 bash -c "$cmd" > "$GOT" 2>/dev/null || true
    local vref vgot
    vref=$(head -4 "$REF" 2>/dev/null | tr '\n' '|')
    vgot=$(head -4 "$GOT" 2>/dev/null | tr '\n' '|')
    if [ -z "$vgot" ]; then
        echo "  $mote : DIVERGENT (sortie vide)"
        PARITE=1
    elif [ "$vref" = "$vgot" ]; then
        echo "  $mote : parité OK"
    else
        echo "  $mote : DIVERGENT"
        echo "    réf : $vref"
        echo "    obt : $vgot"
        PARITE=1
    fi
}

# ── phase 3 : MESURE — $RUNS passes, MIN retenu, chaque moteur à la MÊME
#   échelle N (les caches, l'arithmétique et la file RNG sont à la même échelle
#   partout : c'est la condition pour qu'un écart soit un écart de LANGAGE). ──
mesure() {
    local mote="$1" cmd="$2" srcf="${3:-}"
    local best="" rc=0 t tf prec
    # « time » est un MOT-CLE du shell : dans une fonction, sa sortie ne peut
    # PAS etre capturee par $( { time ... } 2>&1 ) — le banc lisait un temps
    # vide et annoncait a tort « FAIL (rc=2) » a des moteurs sains. On passe
    # donc par un FICHIER.
    # « time -p » ne rend QUE DEUX DECIMALES : a cette resolution, le jumeau
    # C++ (7 ms) et le Rust (5 ms) tombaient tous les deux a « 0.01 » = un
    # 0.000 dans le CSV, ce qui fabrique un faux « plus rapide que ». On sonde
    # donc d'ABORD la resolution de l'horloge avec une commande de DUREE
    # CONNUE, et on la DEPASSE par des passes intermediaires quand elle est
    # trop grosse. Un resultat ne peut plus etre 0.000 que si l'horloge est
    # elle-meme muette — auquel cas le banc reste rouge, comme il le doit.
    tf="$(mktemp)"
    prec=$( { time -p sleep 0.5 >/dev/null; } 2>"$tf"; awk '/^real/{print $2; exit}' "$tf")
    [ -n "$prec" ] || prec=0.01
    # La boucle d'ECHAUFFEMENT multiplie les passes par « warm » AVANT la
    # mesure — c'est ce qui fait que le min rendu est un temps MOYENNE sur
    # « warm » executions, et non le plancher de resolution de « time -p ».
    # Un moteur LENT (l'interpréteur, Python) n'est pas multiplié ×64 : on ne
    # chauffe que ce qui descend sous la résolution de l'horloge. Le seuil est
    # donc pris sur la durée RÉELLE de la commande, pas seulement sur le tic.
    local warm=1
    if awk -v a="$prec" 'BEGIN{exit !(a+0 >= 0.005)}'; then warm=64; fi
    local probe t0probe
    probe=$( { time -p bash -c "$cmd" >/dev/null; } 2>"$tf"; awk '/^real/{print $2; exit}' "$tf")
    [ -n "$probe" ] || probe=999
    if awk -v a="$probe" -v w="$warm" 'BEGIN{exit !((a*w)+0 > 20)}'; then warm=1; fi
    for r in $(seq 1 "$RUNS"); do
        { time -p bash -c "for w in \$(seq 1 $warm); do $cmd; done" >/dev/null; } 2>"$tf" || { rc=$?; break; }
        t=$(awk '/^real/{print $2; exit}' "$tf")
        [ -n "$t" ] || { rc=2; break; }
        t=$(awk -v a="$t" -v w="$warm" 'BEGIN{printf "%.6f", a/w}')
        best=$(awk -v a="${best:-9999999}" -v b="$t" 'BEGIN{ if (b+0<a+0) printf "%.6f", b; else printf "%.6f", a }')
    done
    if [ "$rc" = "0" ]; then
        echo "  $mote : min=$(printf '%.3f' "$best")s"
        printf "englobant,%s,%s,%s\n" "$mote" "$(printf '%.6f' "$best")" "$rc" >> "$OUT"
    else
        echo "  $mote : FAIL (rc=$rc)"
        printf "englobant,%s,FAIL,%s\n" "$mote" "$rc" >> "$OUT"
    fi
    rm -f "$tf"
}

# ── LA RÉFÉRENCE = le moteur Rose lui-même (l'interpréteur, ou le moteur
#   expérimental si ROSE pointe ailleurs). Si elle tombe, le jumeau C++ prend le
#   relais : comparer des fuites entre jumeaux ne prouverait rien sur le banc. ──
timeout 300 "$ROSE" "$HERE/bench_englobant.rose" "$N" 1 > "$REF" 2>/dev/null || true
if [ ! -s "$REF" ] && [ -x "$HERE/.b_cpp" ]; then
    echo "  référence : moteur Rose muet — parité contre le jumeau C++"
    timeout 300 "$HERE/.b_cpp" "$N" 1 > "$REF" 2>/dev/null || true
fi

check rose   "timeout 300 $ROSE $HERE/bench_englobant.rose $N 1"
check aot    "timeout 300 $HERE/.b_aot $N 1"
check llvm   "timeout 300 $HERE/.b_llvm $N 1"
check cpp    "timeout 300 $HERE/.b_cpp $N 1"
[ -x "$HERE/.b_rs" ] && check rs "timeout 300 $HERE/.b_rs $N 1"
check py     "timeout 900 python3 $HERE/bench_englobant.py $N 1"

echo "── mesures (N=$N × $RUNS passes, min retenu) ──"
mesure rose    "timeout 300 $ROSE $HERE/bench_englobant.rose $N $RUNS"
mesure aot     "timeout 300 $HERE/.b_aot $N $RUNS"
mesure llvm    "timeout 300 $HERE/.b_llvm $N $RUNS"
mesure cpp     "timeout 300 $HERE/.b_cpp $N $RUNS"
[ -x "$HERE/.b_rs" ] && mesure rs "timeout 300 $HERE/.b_rs $N $RUNS"
mesure py      "timeout 900 python3 $HERE/bench_englobant.py $N $RUNS"

echo "── résultats : $OUT ──"
cat "$OUT"
rm -rf "$TMPD"
if grep -qE "FAIL|DIVERGENT" "$OUT" || [ "$PARITE" = 1 ]; then
    echo "── BANC NON VÉRIFIÉ : BUILD FAIL / FAIL / DIVERGENT subsistants ──"
    exit 1
fi
