#!/usr/bin/env bash
# Comparatif des moteurs Rose — TABLEAU DE LISIBLE.
# Chaque moteur est NOMMÉ avec sa vraie identité (binaire + moteur + arithmétique),
# jamais un « rust » ambigu. Les numéros viennent de ce script, pas de mémoire.
#
#   Moteur                Binaire                              Arithmétique grande taille
#   C++ (référence)       moteur/cpp/rose                      GMP (libgmp)
#   Rust 2.0 (rose2)      rose2/target/release/rose-20         NATIVE i128 (zéro GMP, zéro dep)
#   Rust ancien (obsolète) moteur/rust/rose                    GMP via FFI
#   Zig                   moteur/zig/rose-zig                  GMP
#   Python                moteur/python/engine.py              int/Fraction natifs Python
#   Julia (si présent)    moteur/julia                         BigInt
set -u
R="$(cd "$(dirname "$0")" && pwd)"
N=${N:-200000}
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

# programmes de mesure (les mêmes pour tous les moteurs)
cat > "$TMP/boucle.rose" <<'EOF'
$total = 0
pour i de 1 vers 10000001 { $total = $total + i }
>> $total
EOF
cat > "$TMP/fib.rose" <<'EOF'
fn fib(n: entier) -> entier {
    si (n < 2) { retour n }
    retour fib(n - 1) + fib(n - 2)
}
>> fib(28)
EOF
cat > "$TMP/big.rose" <<'EOF'
>> factorielle(30)
>> puissance(2, 100)
$r = 1 / 3
>> $r + $r
EOF
cat > "$TMP/crible.rose" <<'EOF'
>> compter_premiers(5000000)
EOF

mesurer() {  # mesurer <label> <cmd...> — le programme est passé en dernier
    local label="$1"; shift
    local f="$TMP/prog.rose"
    local best=999.999 ok=1
    for _ in 1 2 3; do
        local t0 t1
        t0=$(date +%s.%N)
        if ! "$@" "$f" > "$TMP/out.txt" 2>"$TMP/err.txt"; then ok=0; break; fi
        t1=$(date +%s.%N)
        d=$(awk -v a="$t0" -v b="$t1" 'BEGIN{print b-a}')
        best=$(awk -v a="$best" -v b="$d" 'BEGIN{print (b<a)?b:a}')
    done
    if [ "$ok" = 1 ]; then
        printf '  %-22s %8.3fs\n' "$label" "$best"
    else
        printf '  %-22s ÉCHEC (%s)\n' "$label" "$(head -c 90 "$TMP/err.txt" | tr '\n' ' ')"
    fi
}

moteurs() {  # la liste des moteurs, identifiés
    echo "C++ référence|moteur/cpp/rose"
    echo "Rust 2.0 rose2|rose2/target/release/rose-20"
    echo "Rust ancien (GMP)|moteur/rust/rose"
    echo "Zig|moteur/zig/rose-zig"
    echo "Python|moteur/python/engine.py"
}

for prog in boucle fib big crible; do
    echo "── $prog.rose ─────────────────────────────────────────"
    cp "$TMP/$prog.rose" "$TMP/prog.rose"
    while IFS='|' read -r label bin; do
        if [ -x "$R/$bin" ]; then mesurer "$label" "$R/$bin"; continue; fi
        if [ -f "$R/$bin" ] && [ "${bin##*.}" = "py" ]; then mesurer "$label" python3 "$R/$bin"; continue; fi
    done < <(moteurs)
    echo
done

# vérification que le Rust 2.0 est bien SANS GMP (preuve, pas promesse)
echo "── preuve zéro GMP (rose2) ────────────────────────────"
B="$R/rose2/target/release/rose-20"
if [ -x "$B" ]; then
    if ldd "$B" | grep -qi gmp; then echo "  ✗ ROSE-20 LIE ENCORE libgmp !"; else echo "  ✓ ldd : aucune libgmp"; fi
    n=$(nm "$B" 2>/dev/null | grep -c '__gmpz' || true)
    echo "  ✓ symboles GMP (__gmpz*) dans le binaire : $n"
    printf '>> puissance(2, 400)\n' > "$TMP/hors.rose"
    out=$("$B" "$TMP/hors.rose" 2>&1 || true)
    echo "  test 2^400 (hors i128) : $out"
fi
