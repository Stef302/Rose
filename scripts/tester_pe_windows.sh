#!/usr/bin/env bash
# Preuve MESURÉE — pas une simulation — de ce qui se passe quand on essaie
# d'exécuter le rose.exe PE32+ x86-64 produit par C0 sur cet hôte aarch64.
#
# Usage (depuis la racine du dépôt) :
#   bash scripts/garde_fou_mem.sh 12000000 scripts/tester_pe_windows.sh
#   WINE_ROOT=/chemin/wine-amd64 PE=/chemin/rose.exe bash scripts/tester_pe_windows.sh
#
# Le script NE PRÉTEND PAS qu'un PE s'exécute si ce n'est pas le cas : il
# affiche les codes de sortie, octets de stdout/stderr et les sorties réelles.

set -uo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
PE=${PE:-/tmp/opencode/payload_c0_out/rose.exe}
ROSE=${ROSE_BIN:-$ROOT/bench/perf/algo/bin/rose}
WORK=$ROOT/.preuve_exec_win
DEBS=$WORK/debs
WROOT=${WINE_ROOT:-$WORK/wine-amd64}
PREFIX=$WORK/wineprefix
QEMU=${QEMU:-qemu-x86_64-static}
mkdir -p "$WORK" "$DEBS"

say() { printf '\n=== %s ===\n' "$*"; }

say "1. INVENTAIRE (uname -m = $(uname -m))"
for t in wine wine64 wine-stable qemu-x86_64 qemu-x86_64-static box64 box86 fex qemu-system-x86_64 llvm-objdump objdump file python3 docker; do
  p=$(command -v "$t" 2>/dev/null || true)
  if [ -n "$p" ]; then printf '  %-22s %s\n' "$t" "$p"; else printf '  %-22s ABSENT\n' "$t"; fi
done
if [ -x /usr/lib/wine/wine64 ]; then
  file /usr/lib/wine/wine64
  readelf -h /usr/lib/wine/wine64 | grep -E 'Class|Machine'
fi
[ -f "$PE" ] && { file "$PE"; md5sum "$PE"; }

say "2. ÉMULATEURS USER-SPACE / WINE NATIF : essais directs"
timeout 15 wine "$PE" --help >"$WORK/native_wine.out" 2>"$WORK/native_wine.err"
echo "  wine (aarch64, sans WoW64)       rc=$? stdout=$(wc -c <"$WORK/native_wine.out")o"
timeout 15 box64 "$PE" --help >"$WORK/box64.out" 2>"$WORK/box64.err"
echo "  box64 (Linux x86-64 ELF only)    rc=$? : $(head -1 "$WORK/box64.err")"
timeout 15 "$QEMU" "$PE" --help >"$WORK/qemu.out" 2>"$WORK/qemu.err"
echo "  qemu-x86_64 (Linux ELF only)     rc=$? stdout=$(wc -c <"$WORK/qemu.out")o stderr=$(wc -c <"$WORK/qemu.err")o"

say "3. MONTAGE D'UN WINE amd64 SOUS QEMU (aucun root, fichiers dans $WORK)"
# wine64:amd64 + libwine:amd64 + Charges utiles minimales (versions noble 24.04).
deb_urls=(
  'wine64_9.0~repack-4build3_amd64.deb|http://archive.ubuntu.com/ubuntu/pool/universe/w/wine/wine64_9.0~repack-4build3_amd64.deb'
  'libwine_9.0~repack-4build3_amd64.deb|http://archive.ubuntu.com/ubuntu/pool/universe/w/wine/libwine_9.0~repack-4build3_amd64.deb'
  'libc6_2.39-0ubuntu8.12_amd64.deb|http://archive.ubuntu.com/ubuntu/pool/main/g/glibc/libc6_2.39-0ubuntu8.12_amd64.deb'
  'libunwind8_1.8.3-0ubuntu1_amd64.deb|http://archive.ubuntu.com/ubuntu/pool/main/libu/libunwind/libunwind8_1.8.3-0ubuntu1_amd64.deb'
  'liblzma5_5.8.4-1_amd64.deb|http://archive.ubuntu.com/ubuntu/pool/main/x/xz-utils/liblzma5_5.8.4-1_amd64.deb'
  'zlib1g_1.3.dfsg+really1.3.2-3ubuntu1.1_amd64.deb|http://archive.ubuntu.com/ubuntu/pool/main/z/zlib/zlib1g_1.3.dfsg+really1.3.2-3ubuntu1.1_amd64.deb'
  'libz-mingw-w64_1.3.1+dfsg-1_all.deb|http://archive.ubuntu.com/ubuntu/pool/universe/libz/libz-mingw-w64/libz-mingw-w64_1.3.1+dfsg-1_all.deb'
  'libgcc-s1_14-20240412-0ubuntu1_amd64.deb|http://archive.ubuntu.com/ubuntu/pool/main/g/gcc-14/libgcc-s1_14-20240412-0ubuntu1_amd64.deb'
  'libfreetype6_2.13.2+dfsg-1build3_amd64.deb|http://archive.ubuntu.com/ubuntu/pool/main/f/freetype/libfreetype6_2.13.2+dfsg-1build3_amd64.deb'
  'libfontconfig1_2.15.0-1.1ubuntu2_amd64.deb|http://archive.ubuntu.com/ubuntu/pool/main/f/fontconfig/libfontconfig1_2.15.0-1.1ubuntu2_amd64.deb'
  'libexpat1_2.6.1-2build1_amd64.deb|http://archive.ubuntu.com/ubuntu/pool/main/e/expat/libexpat1_2.6.1-2build1_amd64.deb'
)
if [ ! -x "$WROOT/usr/lib/wine/wine64" ]; then
  mkdir -p "$WROOT/lib64" "$WROOT/lib"
  for spec in "${deb_urls[@]}"; do
    f=${spec%%|*}; u=${spec#*|}
    [ -f "$DEBS/$f" ] || timeout 600 curl -fsSL -o "$DEBS/$f" "$u" || echo "  TELECHARGEMENT ÉCHOUÉ: $f"
    [ -f "$DEBS/$f" ] && dpkg-deb -x "$DEBS/$f" "$WROOT"
  done
  ln -sfn ../usr/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2 "$WROOT/lib64/ld-linux-x86-64.so.2"
  ln -sfn "$WROOT/usr/lib/x86_64-linux-gnu" "$WROOT/lib/x86_64-linux-gnu"
  D="$WROOT/usr/lib/x86_64-linux-gnu/wine/x86_64-windows"
  cp "$WROOT/usr/x86_64-w64-mingw32/lib/zlib1.dll" "$D/zlib1.dll"
fi
D="$WROOT/usr/lib/x86_64-linux-gnu/wine/x86_64-windows"
echo "  wine64 amd64 : $('file' -b "$WROOT/usr/lib/wine/wine64")"
echo "  DLL builtins : $(find "$D" -maxdepth 1 -name '*.dll' | wc -l)"

export QEMU_LD_PREFIX="$WROOT"
export WINEPREFIX="$PREFIX"
export WINEDEBUG=-all
RUNWINE=("$QEMU" -L "$WROOT" "$WROOT/usr/lib/wine/wine64")

timeout 60 "${RUNWINE[@]}" --version 2>&1 | sed 's/^/  wine --version: /'
mkdir -p "$PREFIX"
timeout 180 "${RUNWINE[@]}" "$D/wineboot.exe" -u >"$WORK/wineboot.out" 2>&1
echo "  wineboot rc=$? : $(grep -v FreeType "$WORK/wineboot.out" | tr '\n' ' ' | head -c 140)"
S="$PREFIX/drive_c/windows/system32"
mkdir -p "$S"
for f in "$D"/*; do [ -e "$S/$(basename "$f")" ] || ln -s "$f" "$S/$(basename "$f")"; done
echo "  DLLs dans le prefix system32 : $(find "$S" -maxdepth 1 -type l | wc -l)"

say "4. TÉMOIN POSITIF : un vrai PE x86-64 qui, lui, s'exécute"
timeout 90 "${RUNWINE[@]}" "$D/cmd.exe" /c echo CONTROL_OK >"$WORK/control.out" 2>"$WORK/control.err"
echo "  cmd.exe rc=$? stdout=$(tr '\n' ' ' <"$WORK/control.out" | head -c 120)"

say "5. COMPARAISON rose aot (natif aarch64) vs rose.exe (PE sous Wine/amd64/qemu)"
mkdir -p "$WORK/progs"
cat >"$WORK/progs/p1.rose" <<'EOF'
>> 2 + 2
EOF
cat >"$WORK/progs/p2.rose" <<'EOF'
>> 6 * 7
EOF
cat >"$WORK/progs/p3.rose" <<'EOF'
>> 100 / 4
EOF
cat >"$WORK/progs/p4.rose" <<'EOF'
>> 2 ^ 10
EOF
cat >"$WORK/progs/p5.rose" <<'EOF'
>> 1/3 + 1/6
EOF
(
  cd "$WORK/progs" || exit 1
  for f in p1 p2 p3 p4 p5; do
    timeout 90 "$ROSE" aot "$f.rose" >"$WORK/aot_$f.out" 2>&1
    aot_rc=$?
    nat=$(timeout 30 "./$f" 2>&1 | tr '\n' ' ')
    nat_rc=${PIPESTATUS[0]}
    timeout 60 "${RUNWINE[@]}" "$PE" "$WORK/progs/$f.rose" >"$WORK/pe_$f.out" 2>"$WORK/pe_$f.err"
    pe_rc=$?
    printf '  %s : rose aot rc=%s binaire=%q | rose.exe rc=%s stdout=%so stderr=%so\n' \
      "$f" "$aot_rc" "$nat" "$pe_rc" "$(wc -c <"$WORK/pe_$f.out")" "$(wc -c <"$WORK/pe_$f.err")"
  done
)

say "6. PREUVES STATIQUES VÉRIFIABLES (ce qui est prouvé, ce qui ne l'est pas)"
llvm-objdump -d --section=.text "$PE" >"$WORK/text.dis" 2>"$WORK/text.err"
echo "  .text désassemblé : rc=$? lignes=$(wc -l <"$WORK/text.dis") erreurs=$(wc -c <"$WORK/text.err")o"
echo "  marqueurs (bad)/invalid : $(grep -ciE '\(bad\)|invalid|unable|error' "$WORK/text.dis" || true)"
echo "  appels directs à l'entrée 0x140001000 : $(grep -c 'callq.*140001000' "$WORK/text.dis" || true)"
llvm-objdump -p "$PE" | sed -n '/The Import Tables/,$p' | grep -E '^ +[0-9]+ ' | awk '{print $2}' | sort -u >"$WORK/imports.txt"
llvm-objdump -p "$D/kernel32.dll" | grep -E '^ *[0-9]+ +0x[0-9a-f]+ +[A-Za-z_]|forwarded to' | awk '{for(i=1;i<=NF;i++) if($i ~ /^[A-Za-z_][A-Za-z0-9_]*$/ && $i != "forwarded" && $i != "to") {print $i}}' | sort -u >"$WORK/k32.txt"
echo "  imports rose.exe : $(tr '\n' ' ' <"$WORK/imports.txt")"
echo "  imports non résolus par kernel32.dll : $(comm -23 "$WORK/imports.txt" "$WORK/k32.txt" | tr '\n' ' ')"

say "FIN — aucun code de sortie n'a été inventé ; les chiffres ci-dessus sont mesurés."
