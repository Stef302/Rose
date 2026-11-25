#!/usr/bin/env bash
# build_moteur_sans_rust.sh — assemblage J6.7 du moteur complet, SANS outil
#de compilation Rust ni binaire moteur de référence.
#
# Principe
# --------
# Le langage Rose n'a PAS d'instruction `include` : un « module » est un
# fichier source concaténé. Ce script COMPOSE les modules Rose disponibles
# en un seul programme `compilateur/moteur_complet.rose`, puis le confie au
# compilateur natif auto-hébergé C0 (ELF aarch64) qui produit un ELF autonome.
# C0 est un artefact déjà construit : ce script ne le reconstruit PAS.
#
# Usage
# -----
#   scripts/build_moteur_sans_rust.sh --compilateur C0 [--sortie DOSSIER]
#   scripts/build_moteur_sans_rust.sh --source-only
#
# Options :
#   --compilateur C0   ELF du compilateur natif auto-hébergé (obligatoire
#                      sauf avec --source-only).
#   --sortie D         dossier de l'ELF produit (défaut /tmp/opencode/moteur_complet).
#   --source-only      régénorer seulement compilateur/moteur_complet.rose.
#
# Code de sortie : 0 si la composition est vérifiée et l'ELF a le magic
# ELF64 aarch64 attendu, 1 sinon.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
C0=""
OUT="/tmp/opencode/moteur_complet"
SOURCE_ONLY=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --compilateur) C0="${2:-}"; shift 2 ;;
    --sortie) OUT="${2:-}"; shift 2 ;;
    --source-only) SOURCE_ONLY=1; shift ;;
    -h|--help) sed -n '2,26p' "${BASH_SOURCE[0]}"; exit 0 ;;
    -*) echo "OPTION INCONNUE : $1" >&2; exit 1 ;;
    *) break ;;
  esac
done
if [[ $SOURCE_ONLY -eq 0 && ( -z "$C0" || ! -x "$C0" ) ]]; then
  echo "ERREUR : --compilateur doit désigner un ELF exécutable" >&2
  exit 1
fi

DEST="$ROOT/compilateur/moteur_complet.rose"
mkdir -p "$OUT" || exit 1

# ── Composition déterministe (lecture seule des modules) ──────────────
python3 - "$ROOT" "$DEST" <<'PYEOF'
import pathlib, re, sys
from pathlib import Path

root = pathlib.Path(sys.argv[1])
dest = pathlib.Path(sys.argv[2])
comp = root / 'compilateur'

def trouver(nom):
    """Module Rose : d'abord ici, puis dans les worktrees frères."""
    local = comp / nom
    if local.is_file():
        return local
    base = root.parent
    for d in sorted(base.glob('horde_*')):
        c = d / 'compilateur' / nom
        if c.is_file():
            return c
    return None

MODULES = [
    'lecteur_source.rose',    # source -> RIR (inclut le front-end complet)
    'lecteur_bud.rose',       # manifestes .bud -> paires cle=valeur ( Rose pur )
    'interpreteur_rir.rose',  # RIR -> exécution
    'moteur_valeurs.rose',    # représentation taguée des valeurs
    'moteur_ops.rose',        # 20 opérations + repli bigint décimal
    'bigint_rose.rose',       # bigint base 10^9 + Karatsuba
    'gc_rose.rose',           # allocation / récupération en Rose pur
    'tables_rose.rose',       # tables à hachage persistantes
    'chaine_rose.rose',       # rope AVL immuable
    'unicode_rose.rose',      # UTF-8 1..4 octets + NFC élémentaire
    'ffi_rose.rose',          # table de symboles natives
    'ffi_syscalls.rose',      # appels système (horloge, fichiers)
    'repl_rose.rose',         # console interactive
]
OPTIONNELS = ['cli_rose.rose']  # module encore en cours d'écriture

def lire(nom):
    p = trouver(nom)
    return (p, p.read_text(encoding='utf-8')) if p else (None, None)

def fin_fonction(lignes, debut):
    prof = 0
    for i in range(debut, len(lignes)):
        prof += lignes[i].count('{') - lignes[i].count('}')
        if prof == 0 and i > debut:
            return i
    raise SystemExit('fin de fonction introuvable : ' + lignes[debut])

def extraire_fonctions(txt, prefixe, exclues=()):
    """Ne garde que les blocs `fn <prefixe>...` de niveau supérieur."""
    lignes = txt.splitlines()
    blocs = []
    i = 0
    while i < len(lignes):
        m = re.match(r'^fn\s+(\w+)', lignes[i])
        if m and m.group(1).startswith(prefixe) and m.group(1) not in exclues:
            j = fin_fonction(lignes, i)
            blocs.append('\n'.join(lignes[i:j + 1]))
            i = j + 1
        else:
            i += 1
    if not blocs:
        raise SystemExit('aucune fonction ' + prefixe + ' extraite')
    return '\n\n'.join(blocs)

def prefixer(txt, prefixe):
    """Renomme TOUTES les fonctions définies (définitions et appels)."""
    noms = sorted({m.group(1) for m in re.finditer(r'^fn\s+(\w+)', txt, re.M)},
                  key=len, reverse=True)
    if not noms:
        raise SystemExit('aucune fonction à préfixer')
    pat = re.compile(r'\b(' + '|'.join(re.escape(n) for n in noms) + r')\b(?=\s*\()')
    return pat.sub(lambda m: prefixe + m.group(1), txt)

def sans_main(txt, debut, fin):
    """Retire TOUTES les lignes entre les deux marqueurs (inclus)."""
    lignes = txt.splitlines()
    d = next((i for i, l in enumerate(lignes) if debut in l), None)
    f = next((i for i, l in enumerate(lignes) if i > d and fin in l), None) if d is not None else None
    if d is None or f is None:
        raise SystemExit('marqueurs de main introuvables : ' + debut)
    return '\n'.join(lignes[:d] + lignes[f + 1:])

morceaux = []
entetes = []
present = []
for nom in MODULES + OPTIONNELS:
    p, txt = lire(nom)
    if txt is None:
        if nom in OPTIONNELS:
            continue
        raise SystemExit('MODULE MANQUANT : ' + nom)
    present.append(nom)
    entetes.append('#   ' + nom + '  (' + str(p) + ')')
    if nom == 'lecteur_source.rose':
        # Son point d'entrée autonome est TOUJOURS en fin de fichier.
        lignes = txt.splitlines()
        d = next(i for i, l in enumerate(lignes)
                 if '# Exécution en ligne de commande' in l)
        txt = '\n'.join(lignes[:d])
        txt = prefixer(txt, 'lsrc_')
    elif nom == 'interpreteur_rir.rose':
        m = next(i for i, l in enumerate(txt.splitlines())
                 if "Point d'entrée" in l)
        txt = '\n'.join(txt.splitlines()[:m])
    elif nom == 'repl_rose.rose':
        txt = sans_main(txt, '# ─── BEGIN REP_MAIN', '# ─── END REP_MAIN')
    elif nom == 'cli_rose.rose':
        # Module MONOLITHIQUE généré : 393 fonctions dupliquent le front-end
        # et le backend déjà présents. On n'importe que la couche CLI
        # (15 fn cli_*), en écartant cli_rir_du (double du lecteur_source).
        txt = extraire_fonctions(txt, 'cli_', ('cli_rir_du',))
    morceaux.append('# ── module : ' + nom + ' ──\n' + txt.rstrip() + '\n')

pilote = Path('/tmp/opencode/point_entree_complet.txt').read_text(encoding='utf-8')

head = '\n'.join([
    '# moteur_complet.rose — assemblage J6.7 (GÉNÉRÉ, ne pas éditer à la main).',
    '# Généré par scripts/build_moteur_sans_rust.sh. modules composés :',
] + entetes + [
    '#',
    '# Un seul point d\'entrée : lecture, vérification, compilation RIR,',
    '# exécution du RIR, affichage. Aucun appel à un binaire moteur externe.',
    '',
])
dest.write_text(head + '\n\n'.join(morceaux) + '\n' + pilote, encoding='utf-8')
txt = dest.read_text(encoding='utf-8')
print('MODULES : ' + ', '.join(present))
print('SOURCE  : ' + dest.name + ' — ' + str(len(txt.splitlines())) + ' lignes, ' + str(len(txt.encode())) + ' octets')
PYEOF
rc=$?
[[ $rc -eq 0 ]] || exit $rc

if [[ $SOURCE_ONLY -eq 1 ]]; then
  echo "SOURCE ONLY : $DEST"
  exit 0
fi

C0="$(realpath "$C0")"
ELF="$OUT/moteur_complet.elf"
: > "$ELF"
( cd "$OUT" && "$C0" "$DEST" > "$ELF" 2> "$OUT/moteur_complet.err" )
rc=$?
magic="$(od -An -tx1 -N4 "$ELF" 2>/dev/null | tr -d ' \n')"
machine="$(od -An -tu2 -j18 -N2 "$ELF" 2>/dev/null | tr -d ' \n')"
if [[ $rc -ne 0 || "$magic" != "7f454c46" || "$machine" != "183" ]]; then
  echo "ECHEC : rc=$rc magic=$magic machine=$machine" >&2
  head -n 5 "$OUT/moteur_complet.err" >&2
  exit 1
fi
chmod 755 "$ELF"
echo "ELF : $ELF ($(stat -c%s "$ELF") octets, ELF aarch64 machine=$machine)"
exit 0
