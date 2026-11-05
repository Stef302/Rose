#!/usr/bin/env bash
# Auto-hébergement Rose — Test de bootstrap (100% Rust, zéro C++).
set -euo pipefail

RACINE="$(cd "$(dirname "$0")/.." && pwd)"
ROSE_RUST="$RACINE/rose/rose"

echo "============================================================"
echo "  Auto-hébergement Rose — Bootstrap (moteur Rust)"
echo "============================================================"
echo ""

if [ ! -x "$ROSE_RUST" ]; then
    echo "ERREUR : Moteur Rust introuvable : $ROSE_RUST"
    echo "  Construisez-le avec : cd rose && cargo build --release"
    exit 1
fi

echo "[1/3] Compilation du compilateur Rose avec le moteur Rust..."
$ROSE_RUST comp compilateur/compilateur.rose -o "$RACINE/self_host/rose_compiler" 2>&1 || {
    echo "ECHEC"
    exit 1
}
echo "  OK"

echo ""
echo "[2/3] Compilation de l'interpréteur Rose avec le compilateur Rose..."
$RACINE/self_host/rose_compiler comp self_host/moteur/interpreteur.rose -o "$RACINE/self_host/rose_interpreter" 2>&1 || {
    echo "ECHEC"
    exit 1
}
echo "  OK"

echo ""
echo "[3/3] Exécution de l'interpréteur Rose..."
cat > "$RACINE/self_host/tests/test_hello.rose" << 'EOF'
$a = 10
$b = 20
>> $a + $b
>> $a * $b
EOF
$RACINE/self_host/rose_interpreter "$RACINE/self_host/tests/test_hello.rose" 2>&1 || {
    echo "ECHEC"
    exit 1
}
echo "  OK"

echo ""
echo "============================================================"
echo "  Auto-hébergement Rose : SUCCES (100% Rust)"
echo "============================================================"
