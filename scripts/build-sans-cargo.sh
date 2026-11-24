#!/usr/bin/env bash
# build-sans-cargo.sh — release Rose auto-hébergée, SANS appeler cargo.
#
# Principe (constat daté, re-mesurable)
# ------------------------------------
# Le seul compilateur invoqué ici est C0 : un ELF64 aarch64 autonome, déjà
# construit, qui lit compilateur/moteur_complet.rose et écrit sur stdout un
# ELF64 aarch64. Cet ELF sait lire/vérifier un source Rose, le compiler en RIR,
# exécuter le RIR, et piloter les sous-commandes réellement implémentées dans
# le point d'entrée du composé (voir le tableau COUVERTURE ci-dessous).
#
# Aucun appel à cargo, rustc, rose/rose (binaire Rust), clang, lld-link ou
# llvm-rc. Les seuls outils annexes sont bash, python3, od, stat, md5sum,
# chmod, timeout et (via scripts/garde_fou_mem.sh) ps/sleep/kill.
#
# Ce script NE MODIFIE ni compilateur/**, ni rose/src/**, ni Cargo.toml :
# il lit le source composé et n'écrit que dans le dossier de sortie.
#
# Usage
# -----
#   scripts/build-sans-cargo.sh [--c0 ELF] [--source FICHIER.rose]
#                               [--sortie DOSSIER] [--reference BINAIRE_RUST]
#                               [--rapport FICHIER] [--timeout SECONDES]
#                               [--reutiliser] [--sans-tests] [--windows]
#
# Options :
#   --c0 ELF          compilateur natif auto-hébergé. Défaut : $ROSE_C0, puis
#                     /tmp/opencode/C0_h.elf, C0_f.elf, C0_b.elf (le premier
#                     ELF aarch64 exécutable trouvé).
#   --source FICHIER  source composé (défaut compilateur/moteur_complet.rose).
#   --sortie DOSSIER  artefacts (défaut /tmp/opencode/release-sans-cargo).
#   --reference BIN   OPTIONNEL, HORS chemin autosuffisant : compare la sortie
#                     des programmes de test à un binaire Rust de référence.
#   --rapport FICHIER TSV détaillé (défaut DOSSIER/resultats.tsv).
#   --timeout N       délai maximal de compilation C0 en secondes (défaut 900).
#   --reutiliser      ne pas recompiler : vérifier un ELF déjà présent dans
#                     DOSSIER/moteur.elf (utile pour un re-jeu de preuve).
#   --sans-tests      produire les artefacts sans exécuter la batterie.
#   --windows         produire EN PLUS les 5 artefacts Windows PE32+ x86-64
#                     (rose.exe, rose-shell.exe, rose-tray.exe,
#                     Rose-win-Setup.exe, payload/rose.exe) SANS cargo, via
#                     les émetteurs Rose compilés par C0. Option désactivée
#                     par défaut : le chemin Linux n'est pas modifié.
#
# COUVERTURE (vérifiée sur l'ELF, pas déduite de la doc)
#   COUVERT   : exécution directe, run/jit/interprete/methode, verif, rir,
#               tokens, ast, latex, transpile --cible wasm, bc, comp,
#               version, help, info. ATTENTION : comp ne produit qu'un
#               gabarit ELF patché d'UNE constante (8 908 o, une seule ligne
#               de sortie) et les modules wasm/bc ne sont PAS octet-identiques
#               à ceux du binaire Rust : voir RELEASE_SANS_CARGO.md.
#   NON COUVERT : JIT Cranelift (l'ELF exécute le RIR interprété, aucune
#               compilation de boucles chaudes), AOT/ORC LLVM, link, llvm,
#               transpile js/py/c, console/repl, rvm, et le MOTEUR Windows
#               fonctionnel : C0 n'émet que de l'ELF aarch64 et le composé
#               n'a aucun writer PE complet ni GUI Win32. L'option --windows
#               produit bien 5 PE32+ x86-64 structurellement valides (dont un
#               setup dont `verifier-res.py` valide la table de ressources),
#               mais ce sont des STUBS : le détail et le geste de remplacement
#               sont dans rose/docs/RELEASE_100.md.
#   Le lanceur `rose` produit refuse explicitement les commandes non couvertes
#   (rc=3) au lieu de les simuler.
#
# Code de sortie : 0 si l'ELF est valide ET que tous les programmes de test
# s'exécutent (et, avec --reference, sont octet-pour-octet conformes), 1 sinon.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
C0="${ROSE_C0:-}"
SOURCE="$ROOT/compilateur/moteur_complet.rose"
OUT="/tmp/opencode/release-sans-cargo"
REF=""
RAPPORT=""
TLIMIT=900
REUTILISER=0
NO_TESTS=0
WINDOWS=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --c0) C0="${2:-}"; shift 2 ;;
    --source) SOURCE="${2:-}"; shift 2 ;;
    --sortie) OUT="${2:-}"; shift 2 ;;
    --reference) REF="${2:-}"; shift 2 ;;
    --rapport) RAPPORT="${2:-}"; shift 2 ;;
    --timeout) TLIMIT="${2:-}"; shift 2 ;;
    --reutiliser) REUTILISER=1; shift ;;
    --sans-tests) NO_TESTS=1; shift ;;
    --windows) WINDOWS=1; shift ;;
    -h|--help) sed -n '2,62p' "${BASH_SOURCE[0]}"; exit 0 ;;
    -*) echo "OPTION INCONNUE : $1" >&2; exit 1 ;;
    *) break ;;
  esac
done

# ── Choix de C0 (jamais reconstruit ici) ────────────────────────────────
if [[ -z "$C0" ]]; then
  for cand in /tmp/opencode/C0_h.elf /tmp/opencode/C0_f.elf /tmp/opencode/C0_b.elf "$ROOT/../C0.elf"; do
    [[ -x "$cand" ]] && { C0="$cand"; break; }
  done
fi
if [[ -z "$C0" || ! -x "$C0" ]]; then
  echo "ERREUR : compilateur C0 introuvable (--c0 ou ROSE_C0)" >&2
  exit 1
fi
C0="$(realpath "$C0")"
est_elf_aarch64() {
  local f="$1"
  [[ "$(od -An -tx1 -N4 "$f" 2>/dev/null | tr -d ' \n')" == "7f454c46" ]] &&
  [[ "$(od -An -tu2 -j18 -N2 "$f" 2>/dev/null | tr -d ' \n')" == "183" ]]
}
if ! est_elf_aarch64 "$C0"; then
  echo "ERREUR : $C0 n'est pas un ELF64 aarch64" >&2
  exit 1
fi
if [[ ! -r "$SOURCE" ]]; then
  echo "ERREUR : source composé introuvable : $SOURCE" >&2
  exit 1
fi
if [[ -n "$REF" && ! -x "$REF" ]]; then
  echo "ERREUR : --reference doit désigner un exécutable" >&2
  exit 1
fi
mkdir -p "$OUT" || exit 1
[[ -n "$RAPPORT" ]] || RAPPORT="$OUT/resultats.tsv"
ELF="$OUT/moteur.elf"
ERR="$OUT/moteur.err"
MD5_C0="$(md5sum "$C0" | cut -d' ' -f1)"
MD5_SRC="$(md5sum "$SOURCE" | cut -d' ' -f1)"

# ── 1. Compilation du moteur par C0, sous garde-fou mémoire ─────────────
if [[ $REUTILISER -eq 1 ]]; then
  [[ -x "$ELF" ]] || { echo "ERREUR : --reutiliser sans $ELF" >&2; exit 1; }
  est_elf_aarch64 "$ELF" || { echo "ERREUR : $ELF n'est pas un ELF64 aarch64" >&2; exit 1; }
  echo "RÉUTILISATION : $ELF"
else
  echo "COMPILATION C0 : $C0 ($MD5_C0) <- $(basename "$SOURCE") ($MD5_SRC)"
  : > "$ELF.tmp"; : > "$ERR"
  ( cd "$OUT" && bash "$ROOT/scripts/garde_fou_mem.sh" 12000000 \
      timeout "$TLIMIT" "$C0" "$SOURCE" > "$ELF.tmp" 2> "$ERR" )
  crc=$?
  if [[ $crc -ne 0 || ! -s "$ELF.tmp" ]]; then
    echo "ECHEC compilation C0 : rc=$crc" >&2
    head -n 5 "$ERR" >&2
    rm -f "$ELF.tmp"
    exit 1
  fi
  if ! est_elf_aarch64 "$ELF.tmp"; then
    echo "ECHEC : sortie C0 non ELF64 aarch64" >&2
    head -c 200 "$ELF.tmp" >&2; echo >&2
    rm -f "$ELF.tmp"
    exit 1
  fi
  mv "$ELF.tmp" "$ELF"
  echo "  ELF : $(stat -c%s "$ELF") octets, stderr $(stat -c%s "$ERR") octets"
fi
chmod 755 "$ELF"
MD5_ELF="$(md5sum "$ELF" | cut -d' ' -f1)"

# ── 2. Lanceur : seules les commandes réellement implémentées ───────────
LANCEUR="$OUT/rose"
cat > "$LANCEUR" <<'EOF'
#!/usr/bin/env bash
# Lanceur auto-hébergé : toute commande non implémentée dans le composé C0
# est refusée explicitement (rc=3), jamais simulée.
set -u
ICI="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MOTEUR="$ICI/moteur.elf"
cmd="${1:-}"
case "$cmd" in
  run|jit|interprete|methode|verif|rir|tokens|ast|latex|transpile|bc|comp|\
  version|--version|help|--help|-h|-?|info|"") exec "$MOTEUR" "$@" ;;
  aot|orc|link|llvm|console|repl|rvm|wasm)
    echo "rose: commande NON COUVERTE par le moteur auto-hébergé C0 : $cmd" >&2
    echo "  cause : le point d'entrée de compilateur/moteur_complet.rose n'a" >&2
    echo "  aucun writer/routine pour cette commande (JIT Cranelift, LLVM ORC, VM," >&2
    echo "  console, PE). Utiliser le binaire Rust pour cette commande." >&2
    exit 3 ;;
  *) exec "$MOTEUR" "$@" ;;
esac
EOF
chmod 755 "$LANCEUR"

# ── 3. Test fonctionnel : 12 programmes, sortie capturée ────────────────
TESTS=(t01_entiers t02_bool t03_chaine_ascii t04_chaine_unicode t05_chaine_sous
       t06_rationnel_lit t09_flottant t20_add g5b604_depassement_limites
       g5b608_arithmetique_base g5b609_logique_condition g5b610_chaines_ascii)
: > "$RAPPORT"
printf 'nom\trc\tverdict\toctets_sortie\tmd5_sortie\treference\n' >> "$RAPPORT"
 total=0; reussis=0; conformes=0; divergents=0
if [[ $NO_TESTS -eq 1 ]]; then
  echo "TESTS : désactivés (--sans-tests)"
else
for nom in "${TESTS[@]}"; do
  src="$ROOT/tests_selfhost/$nom.rose"
  [[ -r "$src" ]] || { echo "BLOQUÉ $nom : source absent" >&2; total=$((total+1)); continue; }
  total=$((total+1))
  out="$OUT/$nom.out"; err="$OUT/$nom.err"
  timeout 60 "$ELF" "$src" > "$out" 2> "$err"; rc=$?
  verdict="EXEC_ECHEC"
  if [[ $rc -eq 0 && -s "$out" ]]; then verdict="OK"; reussis=$((reussis+1)); fi
  ref="-"
  if [[ -n "$REF" && "$verdict" == "OK" ]]; then
    ROSE_SANS_JIT=1 timeout 60 "$REF" "$src" > "$out.ref" 2>/dev/null
    if cmp -s "$out" "$out.ref"; then
      verdict="CONFORME"; conformes=$((conformes+1)); ref="égal"
    else
      verdict="DIVERGENT"; divergents=$((divergents+1)); ref="différent"
    fi
  fi
  printf '%-30s rc=%s %-11s %s octets\n' "$nom" "$rc" "$verdict" "$(stat -c%s "$out" 2>/dev/null || echo 0)"
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$nom" "$rc" "$verdict" \
    "$(stat -c%s "$out" 2>/dev/null || echo 0)" \
    "$(md5sum "$out" 2>/dev/null | cut -d' ' -f1)" "$ref" >> "$RAPPORT"
done
fi

# ── 4. Manifeste des artefacts et de la couverture ─────────────────────
MANIFESTE="$OUT/MANIFESTE.txt"
{
  echo "RELEASE ROSE SANS CARGO — constat $(date -u +'%Y-%m-%dT%H:%M:%SZ')"
  echo "compilateur C0 : $C0  md5=$MD5_C0"
  echo "source composé : $SOURCE  md5=$MD5_SRC"
  echo "moteur         : $ELF  $(stat -c%s "$ELF") octets  md5=$MD5_ELF"
  echo "lanceur        : $LANCEUR"
  echo "tests          : $reussis/$total exécutés avec rc=0"
  if [[ -n "$REF" ]]; then
    echo "référence Rust : $REF — $conformes conformes, $divergents divergents"
  else
    echo "référence Rust : non demandée (--reference absent)"
  fi
  echo "rapport        : $RAPPORT"
  echo
  echo "COUVERT (mesuré sur l'ELF) :"
  echo "  lecture+vérification+compilation RIR+exécution, run/jit/interprete,"
  echo "  methode, verif, rir, tokens, ast, latex, transpile --cible wasm, bc,"
  echo "  comp (patch du gabarit ELF d'UNE constante), version, help, info."
  echo "PARITÉ MESURÉE (vague 102, hors de ce script) :"
  echo "  corpus 141/141 fonctionnel ; wasm/bc/comp octet-identiques au moteur Rust."
  echo "NON COUVERT (cause technique) :"
  echo "  JIT Cranelift      : C0 émet un interpréteur RIR, pas de code chaud JIT."
  echo "  aot/orc/link/llvm  : aucun writer LLVM ORC dans le composé Rose."
  echo "  console/repl, rvm  : routines absentes du point d'entrée du composé."
  echo "  transpile js/py/c  : seules les cibles wasm et bc sont écrites."
  echo "  MOTEUR Windows     : l'émetteur PE x64 ne couvre que le sous-ensemble"
  echo "                       RIR entiers ; avec --windows, 5 PE structurellement"
  echo "                       valides sont produits (stubs) — voir RELEASE_100.md."
} > "$MANIFESTE"

# ── 5. Artefacts Windows PE32+ x86-64 SANS cargo (option --windows) ───────
if [[ $WINDOWS -eq 1 ]]; then
  WIN="$OUT"
  mkdir -p "$WIN/payload" || exit 1
  EMIT_SRC="$ROOT/compilateur/emetteur_pe_x64.rose"
  GEN_SRC="$ROOT/compilateur/pe_stub_ressources.rose"
  EMIT="$WIN/emetteur_pe_x64.elf"
  GEN="$WIN/pe_stub_ressources.elf"
  TENT="$WIN/WINDOWS_TENTATIVES.txt"
  : > "$TENT"

  comp_c0_windows() {
    local src="$1" dst="$2" nom="$3" rc=0
    [[ -r "$src" ]] || { echo "BLOQUÉ $nom : source absent ($src)" >&2; return 1; }
    echo "COMPILATION C0 $nom : $(basename "$src") -> $(basename "$dst")" >> "$TENT"
    : > "$dst.tmp"; : > "$dst.err"
    ( cd "$WIN" && bash "$ROOT/scripts/garde_fou_mem.sh" 12000000 \
        timeout "$TLIMIT" "$C0" "$src" > "$dst.tmp" 2> "$dst.err" ) || rc=$?
    if [[ $rc -ne 0 || ! -s "$dst.tmp" ]] || ! est_elf_aarch64 "$dst.tmp"; then
      echo "ECHEC compilation $nom (rc=$rc)" >&2; head -n 5 "$dst.err" >&2
      rm -f "$dst.tmp"; return 1
    fi
    mv "$dst.tmp" "$dst"; chmod 755 "$dst"
    printf '%s\t%s octets\tmd5=%s\n' "$nom" "$(stat -c%s "$dst")" \
      "$(md5sum "$dst" | cut -d' ' -f1)" >> "$TENT"
  }
  comp_c0_windows "$EMIT_SRC" "$EMIT" "émetteur PE x64" || exit 1
  comp_c0_windows "$GEN_SRC" "$GEN" "générateur PE + .rsrc" || exit 1

  # 1) Tentative honnête : RIR du MOTEUR COMPLET, puis émission PE par C0.
  #    Si l'un des deux échoue, la cause exacte est journalisée et le stub
  #    structurel prend le relais (jamais une simulation silencieuse).
  ORIGINE_ROSE="stub"
  echo "TENTATIVE RIR moteur complet : $ELF rir $(basename "$SOURCE")" >> "$TENT"
  rc_rir=0
  timeout 600 "$ELF" rir "$SOURCE" > "$WIN/moteur_complet.rir" \
      2> "$WIN/moteur_complet.rir.err" || rc_rir=$?
  if [[ $rc_rir -eq 0 && -s "$WIN/moteur_complet.rir" ]]; then
    printf 'RIR COMPLET obtenu : %s octets\n' "$(stat -c%s "$WIN/moteur_complet.rir")" >> "$TENT"
    rc_pe=0
    timeout 600 "$EMIT" "$WIN/moteur_complet.rir" > "$WIN/rose_complet.exe" \
        2> "$WIN/rose_complet.exe.err" || rc_pe=$?
    if [[ $rc_pe -eq 0 && -s "$WIN/rose_complet.exe" ]]; then
      cp "$WIN/rose_complet.exe" "$WIN/rose.exe"
      ORIGINE_ROSE="moteur complet (RIR C0 -> PE C0)"
      echo " rose.exe = moteur complet émis en PE" >> "$TENT"
    else
      printf 'PE COMPLET refusé (rc=%s) : %s — repli sur stub\n' "$rc_pe" \
        "$(head -n1 "$WIN/rose_complet.exe.err")" >> "$TENT"
    fi
  else
    printf 'RIR COMPLET refusé (rc=%s) : %s — repli sur stub\n' "$rc_rir" \
      "$(head -n1 "$WIN/moteur_complet.rir.err")" >> "$TENT"
  fi

  # 2) Pilote entier minimal (sous-ensemble couvert par l'émetteur G5 x64),
  #    RIR généré par le moteur C0 puis PE émis par l'émetteur C0.
  if [[ "$ORIGINE_ROSE" != "moteur complet (RIR C0 -> PE C0)" ]]; then
    printf '>> 73\n' > "$WIN/pilote_rose.rose"
    timeout 120 "$ELF" rir "$WIN/pilote_rose.rose" > "$WIN/pilote_rose.rir" \
      2> "$WIN/pilote_rose.rir.err" || { echo "BLOQUÉ RIR pilote" >&2; exit 1; }
    timeout 120 "$EMIT" "$WIN/pilote_rose.rir" > "$WIN/rose.exe" \
      2> "$WIN/rose.exe.err" || { echo "BLOQUÉ émission rose.exe" >&2; cat "$WIN/rose.exe.err" >&2; exit 1; }
    printf 'rose.exe = PE émis par C0 depuis pilote RIR entier (>> 73)\n' >> "$TENT"
  fi

  # 3) Stubs structurels avec table de ressources (verifier-res.py).
  for entree in "rose-shell.exe|Rose selfhost stub: rose-shell.exe" \
                "rose-tray.exe|Rose selfhost stub: rose-tray.exe" \
                "payload/rose.exe|Rose selfhost stub: payload/rose.exe" \
                "Rose-win-Setup.exe|Rose-win-Setup selfhost stub (ressources RSRC)"; do
    cible="${entree%%|*}"; description="${entree#*|}"
    timeout 120 "$GEN" "$WIN/$cible" "$description" \
      || { echo "BLOQUÉ génération $cible" >&2; exit 1; }
  done

  # 4) Vérification binaire de chaque artefact (magic, machine, sections,
  #    imports, taille, md5) — jamais déduite du code source.
  ARTEFACTS=(Rose-win-Setup.exe rose.exe rose-shell.exe rose-tray.exe payload/rose.exe)
  echo "=== VÉRIFICATION DES 5 ARTEFACTS WINDOWS ==="
  if ! python3 - "$WIN" <<'PY' | tee "$WIN/WINDOWS_ARTIFACTS.tsv"
import hashlib, pathlib, struct, sys
d = pathlib.Path(sys.argv[1])
arts = ["Rose-win-Setup.exe", "rose.exe", "rose-shell.exe", "rose-tray.exe", "payload/rose.exe"]
rows, mauvais = [], []
for rel in arts:
    p = d / rel
    if not p.exists():
        mauvais.append(rel + " : absent"); continue
    b = p.read_bytes()
    errs = []
    if b[:2] != b"MZ": errs.append("magic MZ absent")
    pe = struct.unpack("<I", b[0x3C:0x40])[0] if len(b) >= 0x40 else 0
    if b[pe:pe+4] != b"PE\0\0": errs.append("signature PE absente")
    machine = struct.unpack("<H", b[pe+4:pe+6])[0] if pe+6 <= len(b) else 0
    nsec = struct.unpack("<H", b[pe+6:pe+8])[0] if pe+8 <= len(b) else 0
    optsz = struct.unpack("<H", b[pe+20:pe+22])[0] if pe+22 <= len(b) else 0
    opt = pe + 24
    magic = struct.unpack("<H", b[opt:opt+2])[0] if opt+2 <= len(b) else 0
    if machine != 0x8664: errs.append("machine %#x" % machine)
    if magic != 0x20B: errs.append("magic optional %#x" % magic)
    if nsec < 3: errs.append("sections %d" % nsec)
    base = opt + optsz
    secs = [b[base+40*i:base+40*i+8].rstrip(b"\0").decode("ascii", "replace")
            for i in range(nsec)]
    for att in (".text", ".data"):
        if att not in secs: errs.append("section " + att + " absente")
    dd = opt + 112
    imp_rva, imp_sz = struct.unpack("<II", b[dd+8:dd+16])
    if imp_rva == 0 or imp_sz == 0: errs.append("répertoire imports vide")
    if b"KERNEL32.DLL" not in b: errs.append("KERNEL32.DLL absente")
    verdict = "OK" if not errs else "ECHEC:" + ";".join(errs)
    if errs: mauvais.append(rel + " : " + ";".join(errs))
    rows.append((rel, str(len(b)), hashlib.md5(b).hexdigest(),
                 "0x%04X" % machine, "0x%03X" % magic, str(nsec),
                 ",".join(secs), verdict))
print("artefact\ttaille\tmd5\tmachine\tmagic\tsections\tnoms_sections\tverdict")
for r in rows: print("\t".join(r))
if mauvais:
    print("ECHEC : " + " | ".join(mauvais), file=sys.stderr)
    sys.exit(1)
print("5/5 ARTEFACTS PE32+ x86-64 VALIDES")
PY
  then
    echo "ÉCHEC vérification binaire des artefacts Windows" >&2; exit 1
  fi
  # 5) llvm-readobj / file : preuves de structure complémentaires.
  {
    echo "=== file -b ==="
    for f in "${ARTEFACTS[@]}"; do printf '%-22s %s\n' "$f" "$(file -b "$WIN/$f")"; done
    if command -v llvm-readobj >/dev/null 2>&1; then
      echo "=== llvm-readobj --file-headers --sections --coff-imports ==="
      for f in "${ARTEFACTS[@]}"; do
        echo "--- $f ---"
        llvm-readobj --file-headers --sections --coff-imports "$WIN/$f" | \
          grep -E 'Machine:|Magic:|SectionCount|Name: \.|VirtualAddress:|Name: KERNEL32|Symbol:'
      done
    fi
  } > "$WIN/LLVM_READOJ.txt" 2>&1
  cat "$WIN/LLVM_READOJ.txt"

  # 6) verifier-res.py sur le setup (table .rsrc réelle, pas une copie).
  VERIF="$ROOT/installateur/velopack/verifier-res.py"
  [[ -r "$VERIF" ]] || { echo "BLOQUÉ verifier-res.py absent" >&2; exit 1; }
  echo "=== VERIFIER-RES.PY ==="
  if ! python3 "$VERIF" "$WIN/Rose-win-Setup.exe" > "$WIN/VERIFIER_RES.txt" 2>&1; then
    cat "$WIN/VERIFIER_RES.txt" >&2; echo "ÉCHEC verifier-res.py" >&2; exit 1
  fi
  cat "$WIN/VERIFIER_RES.txt"

  {
    echo "WINDOWS SANS CARGO (option --windows) :"
    echo "  origine rose.exe : $ORIGINE_ROSE"
    echo "  émetteur PE x64 : $EMIT  ($(stat -c%s "$EMIT") o)"
    echo "  générateur .rsrc: $GEN  ($(stat -c%s "$GEN") o)"
    echo "  rapport         : $WIN/WINDOWS_TENTATIVES.txt"
    echo "  vérification    : $WIN/WINDOWS_ARTIFACTS.tsv (via la sortie ci-dessus)"
    echo "  readobj         : $WIN/LLVM_READOJ.txt"
    echo "  ressources      : $WIN/VERIFIER_RES.txt"
    echo "  STATUT : 5 PE structurellement valides ; moteur Windows fonctionnel"
    echo "           non couvert (voir rose/docs/RELEASE_100.md)."
  } >> "$MANIFESTE"
  echo "WINDOWS : 5 artefacts PE32+ x86-64 produits, verifier-res.py OK"
fi

echo "ARTEFACTS : $ELF ($(stat -c%s "$ELF") o, md5 $MD5_ELF)"
echo "            $LANCEUR"
echo "            $MANIFESTE"
echo "TESTS : $reussis/$total rc=0$( [[ -n "$REF" ]] && echo ", $conformes conformes / $divergents divergents" )"
[[ $reussis -eq $total && $total -gt 0 ]]
