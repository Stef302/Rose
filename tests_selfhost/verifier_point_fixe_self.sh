#!/usr/bin/env bash
# verifier_point_fixe_self.sh — point fixe du self-hosting A-Z.
#   C1 = rose(rose_natif.rose, rose_natif.rose)   compilateur compilant sa source
#   C2 = C1(rose_natif.rose)                      compilateur natif se recompilant
#   cmp C1 C2                                     point fixe étendu
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
B="$ROOT/rose/target/release/rose"
SRC="$ROOT/compilateur/rose_natif.rose"
D=/tmp/opencode
C1=$D/C1_self.elf
C2=$D/C2_self.elf
LOG=$D/point_fixe_self.log
: > "$LOG"
log() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$LOG"; }

cd "$ROOT"
magic() { xxd -p -l 8 "$1" 2>/dev/null | tr -d '\n'; }

log "C1 = compilateur hébergé compilant sa propre source (peut prendre ~1 h)"
timeout 7200 "$B" "$SRC" "$SRC" > "$C1" 2> "$D/C1_self.err"
rc=$?
m1=$(magic "$C1")
log "C1: rc=$rc taille=$(wc -c < "$C1") magic=$m1"
[ "$rc" -eq 0 ] && [ "$m1" = "7f454c4602010100" ] || { log "ECHEC C1"; head -3 "$D/C1_self.err"; exit 1; }

log "C2 = C1 (natif) compilant la même source"
chmod +x "$C1"
timeout 7200 "$C1" "$SRC" > "$C2" 2> "$D/C2_self.err"
rc=$?
m2=$(magic "$C2")
log "C2: rc=$rc taille=$(wc -c < "$C2") magic=$m2"
[ "$rc" -eq 0 ] && [ "$m2" = "7f454c4602010100" ] || { log "ECHEC C2"; head -3 "$D/C2_self.err"; exit 1; }

if cmp -s "$C1" "$C2"; then
  log "SUCCES — C1 == C2, point fixe étendu atteint, md5=$(md5sum "$C1" | cut -d' ' -f1)"
else
  log "ECHEC — C1 != C2 : $(cmp "$C1" "$C2" 2>&1 | head -1)"
  exit 1
fi
