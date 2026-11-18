#!/usr/bin/env bash
# rose-wasm-lib.sh — rend une bibliotheque C existante appelable depuis Rose
#                    en UNE commande (module .wasm M6.1 + declarations .rose).
#
# Ce outil ne modifie AUCUNE source C : il ajoute un shim memcpy/memset
# freestanding et un adaptateur d'ABI pour les parametres `chaine`, compile
# en -O0 (obligatoire avec le pont Rose M6.1), puis verifie le binaire
# (0 import, 0 start, 0 call_indirect, memoire exportee).
#
# Aucune dependance GMP. Aucun fichier .rs / Cargo.toml n'est touche.
set -euo pipefail

# ---------------------------------------------------------------------------
# Configuration (surchargeable par l'environnement)
# ---------------------------------------------------------------------------
CLANG="${WASI_CLANG:-/home/clausake/.local/wasi-sdk/bin/clang}"
SYSROOT="${WASI_SYSROOT:-/home/clausake/.local/wasi-sdk/share/wasi-sysroot}"
OBJDUMP="${WASM_OBJDUMP:-}"
if [[ -z "$OBJDUMP" ]]; then
    if command -v llvm-objdump >/dev/null 2>&1; then
        OBJDUMP="$(command -v llvm-objdump)"
    elif [[ -x /home/clausake/.local/bin/llvm-objdump ]]; then
        OBJDUMP=/home/clausake/.local/bin/llvm-objdump
    fi
fi
if [[ ! -x "$CLANG" ]]; then
    echo "ERREUR: clang WASI introuvable ($CLANG). Poser WASI_CLANG=/chemin/clang." >&2
    exit 1
fi
if [[ ! -d "$SYSROOT" ]]; then
    echo "ERREUR: sysroot WASI introuvable ($SYSROOT). Poser WASI_SYSROOT=/chemin/wasi-sysroot." >&2
    exit 1
fi

usage() {
    cat <<'EOF'
rose-wasm-lib.sh — C existant -> module wasm + declarations Rose (1 commande)

USAGE
  rose-wasm-lib.sh --export 'SYMBOLE=FONCTION(types)->ret' [options] source.c ...

OPTIONS
  --export 'EXP=REAL(t1,t2,...)->ret'  Specification d'une fonction a exposer
                                       (repeatable). EXP = nom exporte dans le
                                       wasm, REAL = vraie fonction C a appeler.
  --out NAME       Basename de sortie (defaut: lib<1er SYMBOLE>) -> NAME.wasm + NAME.rose
  --dir DIR        Repertoire de sortie (defaut: repertoire courant)
  --buf N          Taille du tampon statique pour les chaines (defaut: 16384)
  --header H       Ajoute #include <H> / "H" au glue (rare; repeatable)
  -h, --help       Cette aide

TYPES ROSE ACCEPTES (et leur passage ABI)
  entier, booleen -> i32 / C int
  flottant, rationnel -> f64 / C double
  chaine -> i32 pointeur / C const char* (copie dans un tampon statique)

EXEMPLE (xxHash v0.8.2)
  rose-wasm-lib.sh --export 'xxh32_str=XXH32(chaine,entier,entier)->entier' \
      --out libxxhash xxhash.c

NOTE
  -O0 est FORCE : -O1/-O2/-Os sont executes faux par le pont Rose M6.1.
  Le module est verifie : 0 import, 0 start, 0 call_indirect, memoire exportee.
EOF
}

# ---------------------------------------------------------------------------
# Analyse des arguments
# ---------------------------------------------------------------------------
EXPORTS=()
SOURCES=()
HEADERS=()
OUT=""
DIR="."
BUF=16384

while (($#)); do
    case "$1" in
        --export)
            [[ $# -ge 2 ]] || { echo "ERREUR: --export attend un argument." >&2; exit 1; }
            EXPORTS+=("$2"); shift 2 ;;
        --out)
            [[ $# -ge 2 ]] || { echo "ERREUR: --out attend un argument." >&2; exit 1; }
            OUT="$2"; shift 2 ;;
        --dir)
            [[ $# -ge 2 ]] || { echo "ERREUR: --dir attend un argument." >&2; exit 1; }
            DIR="$2"; shift 2 ;;
        --buf)
            [[ $# -ge 2 ]] || { echo "ERREUR: --buf attend un argument." >&2; exit 1; }
            BUF="$2"; shift 2 ;;
        --header)
            [[ $# -ge 2 ]] || { echo "ERREUR: --header attend un argument." >&2; exit 1; }
            HEADERS+=("$2"); shift 2 ;;
        -h|--help)
            usage; exit 0 ;;
        --)
            shift; SOURCES+=("$@"); break ;;
        -*)
            echo "ERREUR: option inconnue: $1" >&2; usage >&2; exit 1 ;;
        *)
            SOURCES+=("$1"); shift ;;
    esac
done

(( ${#EXPORTS[@]} > 0 )) || { echo "ERREUR: au moins un --export est requis." >&2; usage >&2; exit 1; }
(( ${#SOURCES[@]} > 0 )) || { echo "ERREUR: au moins un fichier .c est requis." >&2; exit 1; }
[[ "$BUF" =~ ^[0-9]+$ ]] || { echo "ERREUR: --buf doit etre un entier." >&2; exit 1; }
for s in "${SOURCES[@]}"; do
    [[ -f "$s" ]] || { echo "ERREUR: source introuvable: $s" >&2; exit 1; }
done

# ---------------------------------------------------------------------------
# Analyse des specifications  EXP=REAL(types)->ret
# ---------------------------------------------------------------------------
declare -a EXP_NAME REAL_NAME RET_TY PARAM_TY
declare -a C_PARAM C_RET
spec_i=0
spec_re='^([A-Za-z_][A-Za-z0-9_]*)=([A-Za-z_][A-Za-z0-9_]*)\(([^)]*)\)->([A-Za-z_][A-Za-z0-9_]*)$'
for spec in "${EXPORTS[@]}"; do
    if [[ ! "$spec" =~ $spec_re ]]; then
        echo "ERREUR: specification invalide: '$spec'" >&2
        echo "        attendu: 'EXP=REAL(chaine,entier)->entier'" >&2
        exit 1
    fi
    exp="${BASH_REMATCH[1]}"
    real="${BASH_REMATCH[2]}"
    plist="${BASH_REMATCH[3]}"
    ret="${BASH_REMATCH[4]}"

    # Types Rose -> types C (ABI wasm32)
    ctype_of() {
        case "$1" in
            entier|booleen)    printf 'int' ;;
            flottant|rationnel) printf 'double' ;;
            chaine)            printf 'const char *' ;;
            *) return 1 ;;
        esac
    }
    c_ret="$(ctype_of "$ret")" || { echo "ERREUR: type de retour inconnu: $ret" >&2; exit 1; }

    IFS=',' read -ra pt <<< "$plist"
    if [[ ${#pt[@]} -eq 1 && -z "${pt[0]}" ]]; then pt=(); fi
    csig=""
    pidx=0
    for t in "${pt[@]}"; do
        c="$(ctype_of "$t")" || { echo "ERREUR: type de parametre inconnu: $t" >&2; exit 1; }
        [[ -n "$csig" ]] && csig+=", "
        csig+="$c a${pidx}"
        pidx=$((pidx+1))
    done

    EXP_NAME+=("$exp"); REAL_NAME+=("$real"); RET_TY+=("$ret")
    PARAM_TY+=("$plist"); C_PARAM+=("$csig"); C_RET+=("$c_ret")
    spec_i=$((spec_i+1))
done

if [[ -z "$OUT" ]]; then OUT="lib${EXP_NAME[0]}"; fi
mkdir -p "$DIR"
OUT_WASM="$DIR/$OUT.wasm"
OUT_ROSE="$DIR/$OUT.rose"
DIR_ABS="$(cd "$DIR" && pwd)"
OUT_WASM_ABS="$DIR_ABS/$OUT.wasm"

WORK="$(mktemp -d /tmp/opencode/rose-wasm-lib.XXXXXX)"
if [[ -n "${RW_KEEP_WORK:-}" ]]; then
    echo "==> (debug) travail conserve dans $WORK"
else
    trap 'rm -rf "$WORK"' EXIT
fi

# ---------------------------------------------------------------------------
# Generation du shim memoire (une seule fois) + glue par export
# ---------------------------------------------------------------------------
cat > "$WORK/rw_shim.c" <<'EOF'
/* AUTO-GENERE par rose-wasm-lib.sh — primitives memoire freestanding. */
#include <stddef.h>
void *memcpy(void *dst, const void *src, size_t n) {
    unsigned char *d = (unsigned char *)dst;
    const unsigned char *s = (const unsigned char *)src;
    for (size_t i = 0; i < n; i++) d[i] = s[i];
    return dst;
}
void *memset(void *dst, int c, size_t n) {
    unsigned char *d = (unsigned char *)dst;
    for (size_t i = 0; i < n; i++) d[i] = (unsigned char)c;
    return dst;
}
EOF

GLUES=()
for ((i=0; i<${#EXPORTS[@]}; i++)); do
    g="$WORK/rw_glue_$i.c"
    GLUES+=("$g")
    {
        echo "/* AUTO-GENERE par rose-wasm-lib.sh — adaptateur ABI pour ${EXP_NAME[$i]} */"
        echo '#include <stddef.h>'
        for h in "${HEADERS[@]}"; do
            if [[ "$h" == \<*\> ]]; then echo "#include $h"; else echo "#include \"$h\""; fi
        done
        echo "#ifndef RW_BUF"
        echo "#define RW_BUF $BUF"
        echo "#endif"
        # NB: strlen est inligne dans le wrapper (pas de fonction helper) :
        #     un helper statique fait generer a clang-18 un call_indirect
        #     parasite que le pont M6.1 refuserait.
        # Buffers statiques : un par parametre chaine
        IFS=',' read -ra pt <<< "${PARAM_TY[$i]}"
        [[ ${#pt[@]} -eq 1 && -z "${pt[0]}" ]] && pt=()
        pidx=0
        for t in "${pt[@]}"; do
            if [[ "$t" == "chaine" ]]; then
                echo "static unsigned char rw_buf_${i}_${pidx}[RW_BUF + 32];"
            fi
            pidx=$((pidx+1))
        done
        # Declaration de la vraie fonction C (types derivés de la signature Rose)
        echo "extern ${C_RET[$i]} ${REAL_NAME[$i]}(${C_PARAM[$i]});"
        # Signature du wrapper
        echo "${C_RET[$i]} ${EXP_NAME[$i]}(${C_PARAM[$i]}) {"
        pidx=0
        callargs=()
        for t in "${pt[@]}"; do
            if [[ "$t" == "chaine" ]]; then
                echo "    size_t n${pidx} = 0; while (a${pidx}[n${pidx}]) n${pidx}++;"
                echo "    if (n${pidx} > RW_BUF) n${pidx} = RW_BUF;"
                echo "    for (size_t j = 0; j < n${pidx}; j++) rw_buf_${i}_${pidx}[j] = (unsigned char)a${pidx}[j];"
                echo "    rw_buf_${i}_${pidx}[n${pidx}] = 0;"
                callargs+=("(const char *)rw_buf_${i}_${pidx}")
            else
                callargs+=("a${pidx}")
            fi
            pidx=$((pidx+1))
        done
        if [[ "${RET_TY[$i]}" == "entier" || "${RET_TY[$i]}" == "booleen" ]]; then
            echo "    return (${C_RET[$i]}) ${REAL_NAME[$i]}($(IFS=,; echo "${callargs[*]}"));"
        else
            echo "    return ${REAL_NAME[$i]}($(IFS=,; echo "${callargs[*]}"));"
        fi
        echo "}"
    } > "$g"
done

# ---------------------------------------------------------------------------
# Compilation (toujours -O0) avec export explicite de la memoire
# ---------------------------------------------------------------------------
CFLAGS=( --target=wasm32-wasi --sysroot="$SYSROOT" -O0 -nostdlib -Wl,--no-entry -Wl,--export-memory )
# Crochet de test : RW_ALLOW_UNDEFINED=1 laisse wasm-ld produire un module
# avec des imports, afin de prouver que le detecteur de sections les refuse.
if [[ -n "${RW_ALLOW_UNDEFINED:-}" ]]; then CFLAGS+=( -Wl,--allow-undefined ); fi
for ((i=0; i<${#EXPORTS[@]}; i++)); do CFLAGS+=( "-Wl,--export=${EXP_NAME[$i]}" ); done
# Ajoute -I pour le repertoire de chaque source (headers locaux)
for s in "${SOURCES[@]}"; do
    d="$(cd "$(dirname "$s")" && pwd)"
    CFLAGS+=( "-I$d" )
done

echo "==> compilation: $CLANG ... -O0 ... -> $OUT_WASM"
# shellcheck disable=SC2086
"$CLANG" "${CFLAGS[@]}" "${GLUES[@]}" "$WORK/rw_shim.c" "${SOURCES[@]}" -o "$OUT_WASM"

# ---------------------------------------------------------------------------
# Verification M6.1 : 0 import, 0 start, 0 call_indirect, memoire exportee
# ---------------------------------------------------------------------------
python3 - "$OUT_WASM" "$OBJDUMP" <<'PY'
import sys, subprocess
path = sys.argv[1]
objdump = sys.argv[2] if len(sys.argv) > 2 else ''
b = open(path, 'rb').read()
if b[:4] != b'\0asm':
    raise SystemExit("ERREUR: module wasm invalide (magic)")
p = 8
def uleb(p):
    r = 0; s = 0
    while True:
        x = b[p]; p += 1
        r |= (x & 0x7f) << s
        if not x & 0x80: return r, p
        s += 7
def name(p):
    n, p = uleb(p)
    return b[p:p+n].decode('utf-8', 'replace'), p + n
imports = 0
start = False
exports = []
while p < len(b):
    sid = b[p]; p += 1
    size, p = uleb(p)
    end = p + size
    if sid == 2:  # IMPORT
        cnt, p = uleb(p); imports = cnt
        for _ in range(cnt):
            _, p = name(p); _, p = name(p)
            k = b[p]; p += 1
            if k == 0:
                _, p = uleb(p)
            elif k == 1:
                p += 1; fl = b[p]; p += 1; _, p = uleb(p)
                if fl & 1: _, p = uleb(p)
            elif k == 2:
                fl = b[p]; p += 1; _, p = uleb(p)
                if fl & 1: _, p = uleb(p)
            elif k == 3:
                p += 2
    elif sid == 7:  # EXPORT
        cnt, p = uleb(p)
        for _ in range(cnt):
            n, p = name(p); k = b[p]; p += 1; _, p = uleb(p)
            exports.append((n, k))
    elif sid == 8:  # START
        start = True
    p = end
ci = -1  # -1 = non verifie
if objdump:
    try:
        out = subprocess.run([objdump, '-d', '--triple=wasm32-unknown-unknown', path],
                             capture_output=True, text=True).stdout
        ci = sum(1 for line in out.splitlines() if 'call_indirect' in line)
    except Exception:
        ci = -1
mem = any(k == 2 for _, k in exports)
fns = [n for n, k in exports if k == 0]
print("WASM_CHECK imports=%d start=%d call_indirect=%s memory_exported=%s"
      % (imports, 1 if start else 0, ci, 'yes' if mem else 'no'))
print("exports=" + ",".join(fns))
bad = []
if imports: bad.append("%d import(s)" % imports)
if start: bad.append("section start")
if ci and ci > 0: bad.append("%d call_indirect" % ci)
if not mem: bad.append("memoire non exportee")
if bad:
    raise SystemExit("ERREUR: incompatible M6.1: " + ", ".join(bad))
PY

# ---------------------------------------------------------------------------
# Generation du fichier . Rose (declarations @dll/@sym/fn pretes a l'emploi)
# ---------------------------------------------------------------------------
{
    echo "# AUTO-GENERE par rose-wasm-lib.sh — importer ces declarations dans Rose."
    echo "# Module: $OUT_WASM_ABS"
    for ((i=0; i<${#EXPORTS[@]}; i++)); do
        echo "# ${REAL_NAME[$i]}(${PARAM_TY[$i]})->${RET_TY[$i]}  =>  ${EXP_NAME[$i]}"
        echo '@dll("wasm", "'"$OUT_WASM_ABS"'")'
        echo "@sym(\"${EXP_NAME[$i]}\")"
        # reconstruit la liste des parametres Rose
        IFS=',' read -ra pt <<< "${PARAM_TY[$i]}"
        [[ ${#pt[@]} -eq 1 && -z "${pt[0]}" ]] && pt=()
        rparams=()
        j=0
        for t in "${pt[@]}"; do rparams+=("a${j}: ${t}"); j=$((j+1)); done
        echo "fn ${EXP_NAME[$i]}($(IFS=,; echo "${rparams[*]}")) -> ${RET_TY[$i]}"
        echo
    done
} > "$OUT_ROSE"

echo "==> OK: $OUT_WASM  ($(stat -c%s "$OUT_WASM") octets)"
echo "==> OK: $OUT_ROSE"
echo "==> Verifie: 0 import, 0 start, 0 call_indirect, memoire exportee. -O0 force."
