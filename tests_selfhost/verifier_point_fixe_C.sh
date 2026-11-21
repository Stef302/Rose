#!/usr/bin/env bash
# verifier_point_fixe_C.sh — chaîne C0 → C1 → C2 du self-hosting (HOST-2).
#
#   C0 = graine Rust émettant le RIR du compilateur composite (déjà lancée)
#   C1 = C0(rose_natif.rose)
#   C2 = C1(rose_natif.rose)
#   cmp C1 C2        ← point fixe étendu
#
# Attend la fin de l'émission de C0 (processus `emetteur_elf.rose … rnat.rir`),
# puis enchaîne les trois étapes sans intervention. Sortie : /tmp/opencode/C*.elf,
# journal : /tmp/opencode/point_fixe_C.log.
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
D=/tmp/opencode
LOG=$D/point_fixe_C.log
C0=$D/C0.elf
C1=$D/C1.elf
C2=$D/C2.elf
SRC=$ROOT/compilateur/rose_natif.rose
log() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$LOG"; }

: > "$LOG"
log "attente fin de l'émission de C0"
while pgrep -f 'emetteur_elf.rose /tmp/opencode/rnat.rir' >/dev/null 2>&1; do
  sleep 30
done
log "émission C0 terminée"

magic=$(xxd -p -l 8 "$C0" 2>/dev/null | tr -d '\n')
taille=$(wc -c < "$C0" 2>/dev/null || echo 0)
log "C0 : taille=$taille magic=$magic"
if [ "$magic" != "7f454c4602010100" ]; then
  log "ECHEC : C0 n'est pas un ELF AArch64 valide — arrêt"
  exit 1
fi
chmod +x "$C0"

log "C1 = C0(rose_natif.rose) — démarrage"
"$C0" "$SRC" > "$C1" 2> "$D/C1.err"
rc=$?
m1=$(xxd -p -l 8 "$C1" 2>/dev/null | tr -d '\n')
log "C1 : rc=$rc taille=$(wc -c < "$C1") magic=$m1"
[ "$m1" = "7f454c4602010100" ] || { log "ECHEC : C1 n'est pas un ELF"; exit 1; }

log "C2 = C1(rose_natif.rose) — démarrage"
chmod +x "$C1"
"$C1" "$SRC" > "$C2" 2> "$D/C2.err"
rc=$?
m2=$(xxd -p -l 8 "$C2" 2>/dev/null | tr -d '\n')
log "C2 : rc=$rc taille=$(wc -c < "$C2") magic=$m2"
[ "$m2" = "7f454c4602010100" ] || { log "ECHEC : C2 n'est pas un ELF"; exit 1; }

if cmp -s "$C1" "$C2"; then
  log "SUCCES : C1 == C2 (point fixe étendu), md5=$(md5sum "$C1" | cut -d' ' -f1)"
else
  log "ECHEC : C1 != C2 — $(cmp "$C1" "$C2" 2>&1 | head -1)"
  exit 1
fi
