#!/bin/sh
# Assemble les runtimes natifs une fois, puis extrait leurs octets.
# Les .bin sont COMMITTÉS : `rose aot` n'invoque aucun outil externe.
set -eu
cd "$(dirname "$0")"
aarch64-linux-gnu-as aarch64.S -o /tmp/a64_rt.o
aarch64-linux-gnu-objcopy -O binary --only-section=.text /tmp/a64_rt.o aarch64.bin
aarch64-linux-gnu-nm -n /tmp/a64_rt.o | grep ' T rt_' | awk '{print $1, $3}' > aarch64.symbols
echo "aarch64.bin: $(wc -c < aarch64.bin) octets"
cat aarch64.symbols

clang --target=x86_64-linux-gnu -c x86_64.s -o /tmp/x64_rt.o
llvm-objcopy --input-target=elf64-x86-64 -O binary --only-section=.text /tmp/x64_rt.o x86_64.bin
llvm-nm -n /tmp/x64_rt.o | grep ' T rt_' | awk '{print $1, $3}' > x86_64.symbols
echo "x86_64.bin: $(wc -c < x86_64.bin) octets"
cat x86_64.symbols
