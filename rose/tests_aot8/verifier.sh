#!/bin/bash
# AOT8 — vérification réelle : 25 programmes, vrais binaires natifs exécutés.
#   aarch64 : exécution native (machine courante)
#   x86-64  : cross-compilation + exécution sous qemu-x86_64-static
#   PE64    : cross-compilation Windows + vérification llvm-readobj --coff-imports
#
# Comparison : sortie octet pour octet avec l'interpréteur Rose, sauf hasard
# (RNG natif non déterministe : contrôle de plage).
set -u
RACINE="$(cd "$(dirname "$0")/../.." && pwd)"
ROSE="${ROSE:-$RACINE/rose/target/release/rose}"
PROG="$RACINE/rose/tests_aot8/programmes"
TMP="${TMP:-/tmp/aot8-verify}"
mkdir -p "$TMP"
echo "BONJOUR" > "$TMP/entree.txt"
# Le programme 18 lit ce fichier : on réécrit son chemin pour le harnais.
sed "s#/tmp/aotwork/entree.txt#$TMP/entree.txt#" "$PROG/18_lire_fichier.rose" > "$TMP/18_lire_fichier.rose"

PASS=0; FAIL=0
printf '%-24s %-9s %-8s %-9s %-8s %-8s\n' PROGRAMME NATIF TAILLE X86 QEMU PE
printf '%.0s-' {1..80}; echo

for src in "$PROG"/*.rose; do
    nom=$(basename "$src" .rose)
    src_case="$src"
    [ "$nom" = "18_lire_fichier" ] && src_case="$TMP/18_lire_fichier.rose"

    # référence interpréteur
    interp=$("$ROSE" "$src_case" 2>&1); ir=$?

    # ── aarch64 natif ──
    a64="$TMP/$nom.a64"
    msg=$("$ROSE" aot "$src_case" -o "$a64" --target aarch64-unknown-linux-gnu 2>&1); arc=$?
    natif=ERR; taille=-
    if [ $arc -eq 0 ] && [ -f "$a64" ]; then
        taille=$(stat -c%s "$a64")
        sortie=$("$a64" 2>&1); brc=$?
        if [ "$nom" = "20_hasard" ]; then
            if [ "$brc" -ge 0 ] && [ "$brc" -lt 100 ]; then natif=OK-RNG; else natif=FAIL; fi
        elif [ "$sortie" = "$interp" ]; then natif=OK; else natif=FAIL; fi
    fi

    # ── x86-64 cross + qemu ──
    x64="$TMP/$nom.x64"
    msgx=$("$ROSE" aot "$src_case" -o "$x64" --target x86_64-unknown-linux-gnu 2>&1); xrc=$?
    qemu=ERR
    if [ $xrc -eq 0 ] && [ -f "$x64" ]; then
        sortie=$(qemu-x86_64-static "$x64" 2>&1); qrc=$?
        if [ "$nom" = "20_hasard" ]; then
            if [ $qrc -ge 0 ] && [ $qrc -lt 100 ]; then qemu=OK-RNG; else qemu=FAIL; fi
        elif [ "$sortie" = "$interp" ]; then qemu=OK; else qemu=FAIL; fi
    fi

    # ── PE64 Windows ──
    exe="$TMP/$nom.exe"
    msgp=$("$ROSE" aot "$src_case" -o "$exe" --target x86_64-pc-windows-msvc 2>&1); prc=$?
    pe=ERR
    if [ $prc -eq 0 ] && [ -f "$exe" ]; then
        if llvm-readobj --coff-imports "$exe" 2>/dev/null | grep -q 'Symbol: HeapAlloc'; then
            pe=OK
        else
            pe=FAIL
        fi
    fi

    printf '%-24s %-9s %-8s %-9s %-8s %-8s\n' "$nom" "$natif" "$taille" "$qemu" "-" "$pe"
    if [ "$natif" = OK ] || [ "$natif" = OK-RNG ]; then PASS=$((PASS+1)); else FAIL=$((FAIL+1)); fi
    if [ "$qemu" = OK ] || [ "$qemu" = OK-RNG ]; then PASS=$((PASS+1)); else FAIL=$((FAIL+1)); fi
    if [ "$pe" = OK ]; then PASS=$((PASS+1)); else FAIL=$((FAIL+1)); fi
done

printf '%.0s-' {1..80}; echo
echo "TOTAL: $PASS OK, $FAIL ECHEC (75 contrôles attendus : 25 programmes × 3 cibles)"
[ "$FAIL" -eq 0 ]
