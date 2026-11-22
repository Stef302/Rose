#!/usr/bin/env bash
# verifier_point_fixe_C2.sh — point fixe étendu via le compilateur NATIF C0.
#   C0 = émetteur machine du RIR du composite (généré en fond)
#   C1 = C0(rose_natif.rose), C2 = C1(rose_natif.rose), cmp C1 C2
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$ROOT/compilateur/rose_natif.rose"
D=/tmp/opencode
C0=$D/C0_new.elf
C1=$D/C1_nat.elf
C2=$D/C2_nat.elf
LOG=$D/point_fixe_C2.log
: > "$LOG"
log() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$LOG"; }
magic() { xxd -p -l 8 "$1" 2>/dev/null | tr -d '\n'; }
MAGIC=7f454c4602010100

log "attente de l'émission de C0"
while pgrep -f 'emetteur_elf.rose /tmp/opencode/rnat2.rir' >/dev/null 2>&1; do sleep 30; done
m0=$(magic "$C0")
log "C0: taille=$(wc -c < "$C0") magic=$m0"
[ "$m0" = "$MAGIC" ] || { log "ECHEC C0"; exit 1; }
chmod +x "$C0"

log "C1 = C0(rose_natif.rose)"
timeout 3600 "$C0" "$SRC" > "$C1" 2> "$D/C1_nat.err"
rc=$?; m1=$(magic "$C1")
log "C1: rc=$rc taille=$(wc -c < "$C1") magic=$m1"
[ "$rc" -eq 0 ] && [ "$m1" = "$MAGIC" ] || { log "ECHEC C1"; head -3 "$D/C1_nat.err"; exit 1; }

log "C2 = C1(rose_natif.rose)"
chmod +x "$C1"
timeout 3600 "$C1" "$SRC" > "$C2" 2> "$D/C2_nat.err"
rc=$?; m2=$(magic "$C2")
log "C2: rc=$rc taille=$(wc -c < "$C2") magic=$m2"
[ "$rc" -eq 0 ] && [ "$m2" = "$MAGIC" ] || { log "ECHEC C2"; head -3 "$D/C2_nat.err"; exit 1; }

if cmp -s "$C1" "$C2"; then
  log "SUCCES — C1 == C2, point fixe étendu, md5=$(md5sum "$C1" | cut -d' ' -f1)"
else
  log "ECHEC — C1 != C2 : $(cmp "$C1" "$C2" 2>&1 | head -1)"
  exit 1
fi
