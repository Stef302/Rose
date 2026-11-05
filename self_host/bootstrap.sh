#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════════
# bootstrap.sh — Test d'auto-hebergement du moteur Rose (M5).
#
# Tente de compiler l'interpreteur Rose (ecrit en Rose) avec le compilateur
# Rose, puis de l'executer.
#
# Etape 1 : Compiler le compilateur Rose avec le moteur C++ (rose-cpp/rose)
# Etape 2 : Compiler l'interpreteur avec le compilateur Rose obtenu
# Etape 3 : Executer l'interpreteur sur un programme de test
#
# ═══════════════════════════════════════════════════════════════════════════
set -euo pipefail

RACINE="$(cd "$(dirname "$0")/.." && pwd)"
ROSE_CPP="$RACINE/rose-cpp/rose"
ROSE_RUST="$RACINE/rose/rose"

echo "============================================================"
echo "  Auto-hebergement Rose — Test de bootstrap M5"
echo "============================================================"
echo ""

# Vérifier que les moteurs existent
if [ ! -x "$ROSE_CPP" ]; then
    echo "ERREUR : Moteur C++ introuvable : $ROSE_CPP"
    echo "  Construisez-le avec : make -C rose-cpp"
    exit 1
fi
if [ ! -x "$ROSE_RUST" ]; then
    echo "ERREUR : Moteur Rust introuvable : $ROSE_RUST"
    echo "  Construisez-le avec : cd rose && cargo build --release"
    exit 1
fi

echo "[1/3] Compilation du compilateur Rose avec le moteur C++ (rose build)..."
$ROSE_CPP build compilateur/compilateur.rose -o "$RACINE/self_host/rose_compiler" 2>&1 || {
    echo "ECHEC : Le compilateur Rose n'a pas pu etre compile par le moteur C++"
    exit 1
}
echo "  OK : binaire cree dans self_host/rose_compiler"

echo ""
echo "[2/3] Compilation de l'interpreteur Rose avec le compilateur Rose..."
$RACINE/self_host/rose_compiler build self_host/moteur/interpreteur.rose -o "$RACINE/self_host/rose_interpreter" 2>&1 || {
    echo "ECHEC : L'interpreteur Rose n'a pas pu etre compile par le compilateur Rose"
    exit 1
}
echo "  OK : binaire cree dans self_host/rose_interpreter"

echo ""
echo "[3/3] Execution de l'interpreteur Rose sur un programme de test..."
cat > "$RACINE/self_host/tests/test_hello.rose" << 'EOF'
$a = 10
$b = 20
>> $a + $b
>> $a * $b
EOF
$RACINE/self_host/rose_interpreter "$RACINE/self_host/tests/test_hello.rose" 2>&1 || {
    echo "ECHEC : L'interpreteur Rose n'a pas pu executer le programme de test"
    exit 1
}
echo "  OK : programme execute avec succès"

echo ""
echo "============================================================"
echo "  Auto-hebergement Rose : SUCCES"
echo "============================================================"
