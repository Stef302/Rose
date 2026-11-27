#!/usr/bin/env bash
# Comparaison Rust vs moteur self-hosté C0 du PROGRAMME CONCATENE de la
# preuve d'execution nu. Les deux moteurs doivent produire exactement les
# memes octets (source Rose -> RIR -> hex des binaires nus).
set -uo pipefail
cd "$(dirname "$0")/.."
R=${ROSE_BIN:-/home/clausake/Stefan/ROSE/rose/rose}
C0=${C0_ELF:-/tmp/opencode/C0_h.elf}
CHOIX=${1:-3}
printf '%s' "$CHOIX" > nu_exec/choix.txt
bash scripts/garde_fou_mem.sh 12000000 $R interprete /tmp/opencode/nu_exec.rose \
  > /tmp/opencode/nu_c0_rust.out
bash scripts/garde_fou_mem.sh 12000000 $C0 /tmp/opencode/nu_exec.rose \
  > /tmp/opencode/nu_c0_self.elf
chmod +x /tmp/opencode/nu_c0_self.elf
/tmp/opencode/nu_c0_self.elf > /tmp/opencode/nu_c0_self.out
diff /tmp/opencode/nu_c0_rust.out /tmp/opencode/nu_c0_self.out \
  && echo "RUST=C0 octet pour octet (choix=$CHOIX)" \
  && md5sum /tmp/opencode/nu_c0_rust.out /tmp/opencode/nu_c0_self.out
