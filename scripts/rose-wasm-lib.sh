#!/usr/bin/env bash
# rose-wasm-lib.sh — rend une bibliotheque existante appelable depuis Rose
#                    en UNE commande (module .wasm + declarations .rose).
#
# Langages sources acceptes (detection par extension) :
#   .c                 clang wasi-sdk, shim memcpy/memset + adaptateur ABI (historique)
#   .cc .cpp .cxx      clang++ wasi-sdk (fonctions a declarer `extern "C"`)
#   .rs               rustc --target=wasm32-unknown-unknown (#[no_mangle] extern "C")
#   .wat .wast        encodeur binaire integre (sous-ensemble documente, pas de wabt)
#   .zig              zig build-exe (si zig est installe ; sinon message explicite)
#   .py .go .js       REFUSES avec une explication honnete (voir doc WASM_IMPORTS_UNIVERSEL.md)
#
# Le comportement C historique est PRESERVE par defaut (-O0, 0 import), mais la
# vague 32 (19/11) ajoute --opt 1|2|s|z : le pont Rose execute desormais
# call_indirect (table d'elements + verification de type) et la section start,
# avec un budget d'instructions borne. --allow-imports autorise les imports
# resolus par le pont (fonctions, memoire, table, globals importes).
#
# Aucune dependance GMP. Aucun fichier .rs / Cargo.toml du projet n'est touche.
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
rose-wasm-lib.sh — C/C++/Rust/WAT/Zig -> module wasm + declarations Rose (1 commande)

USAGE
  rose-wasm-lib.sh --export 'SYMBOLE=FONCTION(types)->ret' [options] source.<ext> ...

OPTIONS
  --export 'EXP=REAL(t1,t2,...)->ret'  Specification d'une fonction a exposer
                                       (repeatable). EXP = nom exporte dans le
                                       wasm, REAL = vraie fonction C a appeler.
  --out NAME       Basename de sortie (defaut: lib<1er SYMBOLE>) -> NAME.wasm + NAME.rose
  --dir DIR        Repertoire de sortie (defaut: repertoire courant)
  --buf N          Taille du tampon statique pour les chaines (defaut: 16384)
  --header H       Ajoute #include <H> / "H" au glue (rare; repeatable)
  --allow-imports  Autorise les imports de fonction (resolus par le pont Rose)
  --malloc         Relaxe les symboles C non definis (-Wl,--allow-undefined) et
                   les resout vers l'allocateur SANDBOXE du pont :
                   env.malloc/calloc/realloc/free (tas incremental, plafond 32 Mio).
                   Utile pour une vraie bibliotheque qui appelle malloc elle-meme
                   (uthash, structures de donnees…). free est un no-op documente.
  --opt NIVEAU     Niveau d'optimisation C/C++/Rust : 0 (defaut historique),
                   1, 2, s (taille) ou z (taille agressive). Depuis la vague 32,
                   le pont Rose execute call_indirect ET la section start : -O1/-O2
                   sont donc utilisables (mesure : LZ4/miniz debloques).
  -h, --help       Cette aide

TYPES ROSE ACCEPTES (et leur passage ABI)
  entier, booleen -> i32 / C int
  flottant, rationnel -> f64 / C double
  chaine -> i32 pointeur / C const char* (copie dans un tampon statique)

EXEMPLE (xxHash v0.8.2)
  rose-wasm-lib.sh --export 'xxh32_str=XXH32(chaine,entier,entier)->entier' \
      --out libxxhash xxhash.c

NOTE
  -O0 restait force HISTORIQUEMENT (le pont refusait call_indirect / start).
  Depuis la vague 32, --opt 1|2|s|z est accepte : le pont resout la table
  d'appels indirects, execute `start` et borne les instructions. Le module est
  TOUJOURS verifie : imports resolus, memoire exportee ; start/call_indirect
  sont RAPPORTES (plus refusés). .py/.go/.js : non convertibles directement.
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
ALLOW_IMPORTS=0
MALLOC=0
OPT_NIVEAU=0

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
        --allow-imports)
            ALLOW_IMPORTS=1; shift ;;
        --malloc)
            MALLOC=1; ALLOW_IMPORTS=1; shift ;;
        --opt)
            [[ $# -ge 2 ]] || { echo "ERREUR: --opt attend un argument." >&2; exit 1; }
            OPT_NIVEAU="$2"; shift 2 ;;
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
(( ${#SOURCES[@]} > 0 )) || { echo "ERREUR: au moins un fichier source est requis." >&2; exit 1; }
[[ "$BUF" =~ ^[0-9]+$ ]] || { echo "ERREUR: --buf doit etre un entier." >&2; exit 1; }
case "$OPT_NIVEAU" in
    0|1|2|s|z) ;;
    *) echo "ERREUR: --opt doit etre 0, 1, 2, s ou z (recu: $OPT_NIVEAU)." >&2; exit 1 ;;
esac
OPT_CFLAG="-O$OPT_NIVEAU"
for s in "${SOURCES[@]}"; do
    [[ -f "$s" ]] || { echo "ERREUR: source introuvable: $s" >&2; exit 1; }
done

# ---------------------------------------------------------------------------
# Detection par extension (l'interface CLI et le chemin C restent identiques)
# ---------------------------------------------------------------------------
declare -a FAMILLES
for s in "${SOURCES[@]}"; do
    case "${s##*.}" in
        c)              FAMILLES+=(c) ;;
        cc|cpp|cxx|C)   FAMILLES+=(cpp) ;;
        rs)             FAMILLES+=(rust) ;;
        wat|wast)       FAMILLES+=(wat) ;;
        zig)            FAMILLES+=(zig) ;;
        py)
            echo "ERREUR: '$s' : CPython n'est PAS convertible en wasm autonome." >&2
            echo "  Cython/mypyc generent du C dependant de Python.h (imports non resolus)." >&2
            echo "  Rose n'importe PAS CPython dans le produit. Reecrire en C/Rust/WAT." >&2
            exit 1 ;;
        go)
            echo "ERREUR: '$s' : Go generate un wasm avec runtime/GC et de nombreux" >&2
            echo "  imports WASI non resolus. Utiliser TinyGo -target=wasi puis passer" >&2
            echo "  par .wat/.c, ou reecrire la fonction en C/Rust." >&2
            exit 1 ;;
        js)
            echo "ERREUR: '$s' : JavaScript n'a pas de compilateur wasm autonome ici" >&2
            echo "  (Emscripten absent ; son runtime JS n'est pas un module wasm pur)." >&2
            echo "  Reecrire la fonction en C/Rust/WAT." >&2
            exit 1 ;;
        *)
            echo "ERREUR: extension inconnue: ${s##*.} ($s)" >&2
            echo "  Acceptees: .c .cc .cpp .cxx .rs .wat .wast .zig" >&2
            exit 1 ;;
    esac
done
FAMILLE="${FAMILLES[0]}"
for f in "${FAMILLES[@]}"; do
    if [[ "$f" != "$FAMILLE" && !( "$FAMILLE" == c && "$f" == cpp ) && !( "$FAMILLE" == cpp && "$f" == c ) ]]; then
        echo "ERREUR: melange de langages ($FAMILLE + $f) : une seule famille a la fois." >&2
        exit 1
    fi
done
[[ "$FAMILLE" == cpp ]] && FAMILLE_CXX=1 || FAMILLE_CXX=0
if [[ "$FAMILLE" == wat || "$FAMILLE" == zig || "$FAMILLE" == rust ]] && (( ${#SOURCES[@]} != 1 )); then
    echo "ERREUR: $FAMILLE n'accepte qu'un seul fichier source." >&2; exit 1
fi

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
# C / C++ : generation du shim memoire (une seule fois) + glue par export
# ---------------------------------------------------------------------------
if [[ "$FAMILLE" == c || "$FAMILLE" == cpp ]]; then
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
    # EXP == REAL : le source exporte deja le symbole, un wrapper le doublerait.
    [[ "${EXP_NAME[$i]}" == "${REAL_NAME[$i]}" ]] && continue
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
fi

# ---------------------------------------------------------------------------
# Compilation par famille
# ---------------------------------------------------------------------------
if [[ "$FAMILLE" == c || "$FAMILLE" == cpp ]]; then
    CFLAGS=( --target=wasm32-wasi --sysroot="$SYSROOT" "$OPT_CFLAG" -nostdlib -Wl,--no-entry -Wl,--export-memory )
    # Crochet de test : RW_ALLOW_UNDEFINED=1 laisse wasm-ld produire un module
    # avec des imports. --malloc fait de meme et resout les symboles C vers
    # l'allocateur sandboxe du pont (env.malloc/calloc/realloc/free).
    if [[ -n "${RW_ALLOW_UNDEFINED:-}" || "$MALLOC" == 1 ]]; then
        CFLAGS+=( -Wl,--allow-undefined )
    fi
    for ((i=0; i<${#EXPORTS[@]}; i++)); do CFLAGS+=( "-Wl,--export=${EXP_NAME[$i]}" ); done
    for s in "${SOURCES[@]}"; do
        d="$(cd "$(dirname "$s")" && pwd)"
        CFLAGS+=( "-I$d" )
    done
    if (( FAMILLE_CXX )); then
        CXX="${WASI_CLANGXX:-${CLANG/clang/clang++}}"
        [[ -x "$CXX" ]] || { echo "ERREUR: clang++ WASI introuvable ($CXX)." >&2; exit 1; }
        CFLAGS+=( -fno-exceptions -fno-rtti )
        CFLAGS_COMPILE=()
        for f in "${CFLAGS[@]}"; do [[ "$f" == -Wl,* ]] || CFLAGS_COMPILE+=( "$f" ); done
        OBJ=(); k=0
        for s in "${SOURCES[@]}"; do
            o="$WORK/source_$k.o"; k=$((k+1))
            if [[ "${s##*.}" == c ]]; then
                "$CLANG" "${CFLAGS_COMPILE[@]}" -c "$s" -o "$o"
            else
                "$CXX" "${CFLAGS_COMPILE[@]}" -c "$s" -o "$o"
            fi
            OBJ+=( "$o" )
        done
        # ⚠ La glue et le shim restent compilés en C (pas de name mangling) ;
        # seules les sources .cpp passent par clang++.
        GLUE_OBJ=(); k=0
        for g in "${GLUES[@]}" "$WORK/rw_shim.c"; do
            o="$WORK/glue_$k.o"; k=$((k+1))
            "$CLANG" "${CFLAGS_COMPILE[@]}" -c "$g" -o "$o"
            GLUE_OBJ+=( "$o" )
        done
        echo "==> compilation C++: $CXX ... $OPT_CFLAG ... -> $OUT_WASM"
        "$CXX" "${CFLAGS[@]}" "${GLUE_OBJ[@]}" "${OBJ[@]}" -o "$OUT_WASM"
    else
        echo "==> compilation: $CLANG ... $OPT_CFLAG ... -> $OUT_WASM"
        "$CLANG" "${CFLAGS[@]}" "${GLUES[@]}" "$WORK/rw_shim.c" "${SOURCES[@]}" -o "$OUT_WASM"
    fi

elif [[ "$FAMILLE" == rust ]]; then
    RUSTC="${RUSTC:-$HOME/.cargo/bin/rustc}"
    [[ -x "$RUSTC" ]] || { echo "ERREUR: rustc introuvable ($RUSTC)." >&2; exit 1; }
    "$RUSTC" --print target-list 2>/dev/null | grep -qx wasm32-unknown-unknown || {
        echo "ERREUR: cible Rust wasm32-unknown-unknown absente." >&2
        echo "  Installer: rustup target add wasm32-unknown-unknown" >&2; exit 1; }
    # Types Rose -> types FFI Rust (ABI wasm32 : i32 / f64 / pointeur)
    rust_type() {
        case "$1" in
            entier|booleen)     printf 'i32' ;;
            flottant|rationnel) printf 'f64' ;;
            chaine)             printf '*const u8' ;;
            *) return 1 ;;
        esac
    }
    GLUE_RS="$WORK/rw_crate.rs"
    {
        echo '// AUTO-GENERE par rose-wasm-lib.sh — adaptateur ABI Rust.'
        echo '#![no_std]'
        echo '#![no_main]'
        echo '#![allow(non_snake_case, unused, improper_ctypes)]'
        # ⚠ no_std + panic_handler : le std de rustc génère des tables
        # call_indirect (14 mesurés) que le pont Rose refuse. Les fonctions
        # converties doivent donc être du calcul pur (core uniquement).
        echo '#[panic_handler]'
        echo 'fn rw_panic(_: &core::panic::PanicInfo) -> ! { loop {} }'
        # ⚠ rustc n'accepte qu'UN fichier d'entree : on cree une racine de
        # crate qui inclut chaque source comme module (chemin absolu).
        k=0
        for s in "${SOURCES[@]}"; do
            sabs="$(cd "$(dirname "$s")" && pwd)/$(basename "$s")"
            echo "#[path = \"$sabs\"] mod user_$k;"
            k=$((k+1))
        done
        for ((i=0; i<${#EXPORTS[@]}; i++)); do
            [[ "${EXP_NAME[$i]}" == "${REAL_NAME[$i]}" ]] && continue
            rret="$(rust_type "${RET_TY[$i]}")" || { echo "ERREUR: type de retour Rust inconnu" >&2; exit 1; }
            IFS=',' read -ra pt <<< "${PARAM_TY[$i]}"
            [[ ${#pt[@]} -eq 1 && -z "${pt[0]}" ]] && pt=()
            params=""; appels=""; j=0
            for t in "${pt[@]}"; do
                rt="$(rust_type "$t")" || { echo "ERREUR: type de parametre Rust inconnu: $t" >&2; exit 1; }
                [[ -n "$params" ]] && { params+=", "; appels+=", "; }
                params+="a${j}: ${rt}"; appels+="a${j}"; j=$((j+1))
            done
            echo "#[no_mangle]"
            echo "pub extern \"C\" fn ${EXP_NAME[$i]}(${params}) -> ${rret} {"
            echo "    unsafe { user_0::${REAL_NAME[$i]}(${appels}) }"
            echo "}"
        done
    } > "$GLUE_RS"
    echo "==> compilation Rust: $RUSTC --target wasm32-unknown-unknown ... -> $OUT_WASM"
    # ⚠ overflow-checks=off + panic=abort : les vérifications Rust -O0 génèrent
    #    des blocs imbriqués mal gérés par le pont (mesuré : appel bloqué).
    "$RUSTC" --target wasm32-unknown-unknown --crate-type cdylib \
        -C opt-level="$OPT_NIVEAU" -C overflow-checks=off -C panic=abort \
        "$GLUE_RS" -o "$OUT_WASM"

elif [[ "$FAMILLE" == wat ]]; then
    echo "==> encodage WAT (encodeur binaire integre, sans wabt) -> $OUT_WASM"
    python3 - "${SOURCES[0]}" "$OUT_WASM" <<'PY'
import struct, sys
SRC, OUT = sys.argv[1], sys.argv[2]
TEXTE = open(SRC, 'r', encoding='utf-8').read()

def tokens(s):
    out, i = [], 0
    while i < len(s):
        c = s[i]
        if c.isspace(): i += 1
        elif s.startswith(';;', i):
            j = s.find('\n', i); i = len(s) if j < 0 else j + 1
        elif s.startswith('(;', i):
            depth, i = 1, i + 2
            while i < len(s) and depth:
                if s.startswith('(;', i): depth += 1; i += 2
                elif s.startswith(';)', i): depth -= 1; i += 2
                else: i += 1
        elif c in '();': out.append(c); i += 1
        elif c == '"':
            j, buf = i + 1, ''
            while j < len(s) and s[j] != '"':
                if s[j] == '\\':
                    j += 1; buf += {'n':'\n','t':'\t','r':'\r','0':'\0'}.get(s[j], s[j])
                else: buf += s[j]
                j += 1
            out.append(('str', buf)); i = j + 1
        else:
            j = i
            while j < len(s) and not s[j].isspace() and s[j] not in '();"': j += 1
            out.append(s[i:j]); i = j
    return out

def sexpr(toks):
    pile, cour = [], []
    for t in toks:
        if t == '(': pile.append(cour); cour = []
        elif t == ')':
            if not pile: raise SystemExit('ERREUR WAT: parenthèse fermante sans ouvrante')
            pere = pile.pop(); pere.append(cour); cour = pere
        else: cour.append(t)
    if pile: raise SystemExit('ERREUR WAT: parenthèse non fermée')
    return cour

def val(x): return x[1] if isinstance(x, tuple) and x[0] == 'str' else x
def ident(x): return x[1:] if isinstance(x, str) and x.startswith('$') else None
def sans_id(l): return l[1:] if l and ident(l[0]) else l

def uleb(v):
    v &= (1 << 64) - 1; out = bytearray()
    while True:
        b = v & 0x7f; v >>= 7
        out.append(b | (0x80 if v else 0))
        if not v: return bytes(out)

def sleb(v):
    out = bytearray()
    while True:
        b = v & 0x7f; v >>= 7
        fini = (v == 0 and not b & 0x40) or (v == -1 and b & 0x40)
        out.append(b | (0 if fini else 0x80))
        if fini: return bytes(out)

def vec(o): return uleb(len(o)) + b''.join(o)
def nom(s):
    b = s.encode('utf-8'); return uleb(len(b)) + b

def ty(t):
    v = {'i32':0x7f,'i64':0x7e,'f32':0x7d,'f64':0x7c}.get(t)
    if v is None: raise SystemExit('ERREUR WAT: type inconnu ' + str(t))
    return v

def champs(l):
    p, r, loc = [], [], []
    for x in l:
        if isinstance(x, list) and x and x[0] in ('param','result','local'):
            {'param':p,'result':r,'local':loc}[x[0]].extend(ty(v) for v in sans_id(x[1:]))
    reste = [x for x in l if not (isinstance(x, list) and x and x[0] in ('param','result','local'))]
    return p, r, loc, reste

mod = sexpr(tokens(TEXTE))
if mod and len(mod) == 1 and isinstance(mod[0], list): mod = mod[0]
if not mod or mod[0] != 'module':
    raise SystemExit('ERREUR WAT: un seul (module ...) est attendu (.wast avec assertions refuse)')
corps = [x for x in mod[1:] if isinstance(x, list)]
imports, funcs, exports, mem = [], [], [], None
for f in corps:
    t = f[0]
    if t == 'import':
        m, n, desc = val(f[1]), val(f[2]), f[3]
        if desc[0] == 'func':
            pp, rr, _, _ = champs(sans_id(desc[1:]))
            imports.append(dict(kind='func', mod=m, nom=n, id=ident(desc[1]), params=pp, results=rr))
        elif desc[0] == 'memory': imports.append(dict(kind='memory', mod=m, nom=n))
        else: raise SystemExit('ERREUR WAT: import ' + str(desc[0]) + ' non supporté')
    elif t == 'func':
        name = ident(f[1]); pp, rr, loc, reste = champs(sans_id(f[1:])); instr = []
        for x in reste:
            if isinstance(x, list) and x and x[0] == 'export': exports.append((val(x[1]), 'func', name))
            else: instr.append(x)
        funcs.append(dict(id=name, params=pp, results=rr, locals=loc, instr=instr))
    elif t == 'memory':
        for x in f[1:]:
            if isinstance(x, list) and x and x[0] == 'export': exports.append((val(x[1]), 'memory', 0))
        n = [x for x in f[1:] if isinstance(x, str)]
        if not n: raise SystemExit('ERREUR WAT: taille de mémoire absente')
        mem = (int(n[0]), None)
    elif t == 'export': exports.append((val(f[1]), f[2][0], val(f[2][1])))
    elif t == 'data': pass
    else: raise SystemExit('ERREUR WAT: champ de module non supporté: ' + str(t))

types = []
def type_idx(pp, rr):
    sig = (tuple(pp), tuple(rr))
    if sig not in types: types.append(sig)
    return types.index(sig)
func_types, idx_of_id, idx = [], {}, 0
for imp in imports:
    if imp['kind'] == 'func':
        idx_of_id[imp['id']] = idx; func_types.append(type_idx(imp['params'], imp['results'])); idx += 1
for f in funcs:
    if f['id']: idx_of_id[f['id']] = idx
    func_types.append(type_idx(f['params'], f['results'])); idx += 1

OPS = {
 'unreachable':0x00,'nop':0x01,'else':0x05,'end':0x0b,'return':0x0f,'drop':0x1a,'select':0x1b,
 'i32.eqz':0x45,'i32.eq':0x46,'i32.ne':0x47,'i32.lt_s':0x48,'i32.lt_u':0x49,'i32.gt_s':0x4a,
 'i32.gt_u':0x4b,'i32.le_s':0x4c,'i32.le_u':0x4d,'i32.ge_s':0x4e,'i32.ge_u':0x4f,
 'i64.eqz':0x50,'i64.eq':0x51,'i64.ne':0x52,'i64.lt_s':0x53,'i64.gt_s':0x55,
 'i64.le_s':0x57,'i64.ge_s':0x59,
 'i32.add':0x6a,'i32.sub':0x6b,'i32.mul':0x6c,'i32.div_s':0x6d,'i32.div_u':0x6e,
 'i32.rem_s':0x6f,'i32.rem_u':0x70,'i32.and':0x71,'i32.or':0x72,'i32.xor':0x73,
 'i32.shl':0x74,'i32.shr_s':0x75,'i32.shr_u':0x76,'i32.rotl':0x77,'i32.rotr':0x78,
 'i64.add':0x7c,'i64.sub':0x7d,'i64.mul':0x7e,'i64.div_s':0x7f,'i64.div_u':0x80,
 'i64.and':0x83,'i64.or':0x84,'i64.xor':0x85,'i64.shl':0x86,'i64.shr_u':0x88,
 'i32.wrap_i64':0xa7,'i64.extend_i32_s':0xac,'i64.extend_i32_u':0xad}
BLOCS = {'block':0x02,'loop':0x03,'if':0x04}
LOADS = {'i32.load':0x28,'i32.load8_u':0x2d,'i32.load16_u':0x2f,'i64.load':0x29,'f64.load':0x2b}
STORES = {'i32.store':0x36,'i32.store8':0x3a,'i64.store':0x37,'f64.store':0x39}

def ref(x):
    if isinstance(x, str) and x.startswith('$'): x = x[1:]
    if x in idx_of_id: return idx_of_id[x]
    try: return int(x)
    except (TypeError, ValueError): raise SystemExit('ERREUR WAT: référence inconnue ' + str(x))

def encode_body(f):
    corps, i, b = bytearray(), 0, f['instr']
    while i < len(b):
        x = b[i]; i += 1
        if x == 'i32.const':
            v = b[i]; i += 1; corps += bytes([0x41]) + sleb(int(str(v).split()[-1]))
        elif x == 'i64.const':
            v = b[i]; i += 1; corps += bytes([0x42]) + sleb(int(str(v).split()[-1]))
        elif x == 'f64.const':
            v = b[i]; i += 1; corps += bytes([0x44]) + struct.pack('<d', float(str(v).split()[-1]))
        elif x in BLOCS: corps.append(BLOCS[x]); corps += uleb(0x40)
        elif x in ('br','br_if'):
            corps.append(0x0c if x == 'br' else 0x0d); corps += uleb(ref(b[i])); i += 1
        elif x in OPS: corps.append(OPS[x])
        elif x in ('local.get','local.set','local.tee','global.get','global.set','call'):
            op = {'local.get':0x20,'local.set':0x21,'local.tee':0x22,
                  'global.get':0x23,'global.set':0x24,'call':0x10}[x]
            corps.append(op); arg = b[i]; i += 1
            corps += uleb(int(arg) if x.startswith('local.') else ref(arg))
        elif x in LOADS or x in STORES:
            corps.append(LOADS[x] if x in LOADS else STORES[x]); corps += uleb(2) + uleb(0)
        elif x == 'memory.size': corps += bytes([0x3f, 0])
        elif x == 'memory.grow': corps += bytes([0x40, 0])
        else: raise SystemExit('ERREUR WAT: instruction non supportée: ' + str(x))
    corps.append(0x0b)
    locs = bytearray(uleb(len(f['locals'])))
    for t in f['locals']: locs += uleb(1) + bytes([t])
    corps = bytes(locs) + bytes(corps)
    return uleb(len(corps)) + corps

sec_type = vec([bytes([0x60]) + vec([bytes([p]) for p in pp]) + vec([bytes([r]) for r in rr]) for pp, rr in types])
sec_import = b''
if imports:
    ents = []
    for imp in imports:
        if imp['kind'] == 'func':
            ents.append(nom(imp['mod']) + nom(imp['nom']) + b'\x00' + uleb(type_idx(imp['params'], imp['results'])))
        else: ents.append(nom(imp['mod']) + nom(imp['nom']) + b'\x02\x00' + uleb(1))
    sec_import = vec(ents)
sec_func = uleb(len(funcs)) + b''.join(uleb(t) for t in func_types[len(imports):]) if funcs else b''
sec_mem = uleb(1) + b'\x00' + uleb(mem[0]) if mem and not any(i['kind']=='memory' for i in imports) else b''
sec_export = b''
if exports:
    ents = []
    for n, kind, idx0 in exports:
        k = 0 if kind == 'func' else 2
        if kind == 'func': idx0 = ref(idx0)
        elif idx0 is None: idx0 = 0
        ents.append(nom(n) + bytes([k]) + uleb(int(idx0)))
    sec_export = vec(ents)
sec_code = uleb(len(funcs)) + b''.join(encode_body(f) for f in funcs) if funcs else b''
data_segs = []
for f in corps:
    if f[0] == 'data':
        off = int(str(f[1][1]).split()[-1]) if isinstance(f[1], list) and len(f[1]) > 1 else 0
        octets = ''.join(val(x) for x in f[2:]).encode('utf-8')
        data_segs.append(b'\x00' + bytes([0x41]) + sleb(off) + b'\x0b' + uleb(len(octets)) + octets)
sec_data = uleb(len(data_segs)) + b''.join(data_segs) if data_segs else b''
sections = [(1,sec_type),(2,sec_import),(3,sec_func),(5,sec_mem),(7,sec_export),(10,sec_code),(11,sec_data)]
b = b'\0asm' + b'\x01\0\0\0' + b''.join(bytes([i]) + uleb(len(s)) + s for i, s in sections if s)
open(OUT, 'wb').write(b)
print('WAT_ENCODE ok: %d octets, %d fonction(s), %d import(s)' % (len(b), len(funcs), len(imports)))
PY

elif [[ "$FAMILLE" == zig ]]; then
    ZIG="${ZIG:-$(command -v zig 2>/dev/null || true)}"
    if [[ -z "$ZIG" ]]; then
        echo "ERREUR: zig est absent de cette machine — conversion non effectuee." >&2
        echo "  Installation (une fois):" >&2
        echo "    curl -L https://ziglang.org/download/0.14.1/zig-x86_64-linux-0.14.1.tar.xz -o /tmp/opencode/zig.tar.xz" >&2
        echo "    tar -xJf /tmp/opencode/zig.tar.xz -C /home/clausake/.local" >&2
        echo "    export ZIG=/home/clausake/.local/zig-x86_64-linux-0.14.1/zig" >&2
        echo "  Puis: ZIG=... $0 --export '...' --out ... source.zig" >&2
        exit 1
    fi
    ZFLAGS=( -target wasm32-freestanding -ODebug -fno-entry )
    for ((i=0; i<${#EXPORTS[@]}; i++)); do ZFLAGS+=( "--export=${EXP_NAME[$i]}" ); done
    echo "==> compilation Zig: $ZIG build-exe ... -> $OUT_WASM"
    "$ZIG" build-exe "${ZFLAGS[@]}" "${SOURCES[0]}" -femit-bin="$OUT_WASM"
fi

# ---------------------------------------------------------------------------
# Verification : imports resolus, memoire exportee ; start/call_indirect rapportes
# ---------------------------------------------------------------------------
python3 - "$OUT_WASM" "$OBJDUMP" "$ALLOW_IMPORTS" "$WORK/imports.txt" "$(IFS=,; echo "${EXP_NAME[*]}")" <<'PY'
import sys, subprocess
path = sys.argv[1]
objdump = sys.argv[2] if len(sys.argv) > 2 else ''
allow = len(sys.argv) > 3 and sys.argv[3] == '1'
imports_file = sys.argv[4] if len(sys.argv) > 4 else ''
attendus = [x for x in sys.argv[5].split(',') if x] if len(sys.argv) > 5 else []
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
imports_liste = []
start = False
exports = []
while p < len(b):
    sid = b[p]; p += 1
    size, p = uleb(p)
    end = p + size
    if sid == 2:  # IMPORT
        cnt, p = uleb(p); imports = cnt
        for _ in range(cnt):
            mm, p = name(p); nn, p = name(p)
            k = b[p]; p += 1
            imports_liste.append((mm + '.' + nn, k))
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
print("imports_liste=" + ",".join("%s(kind=%d)" % (n, k) for n, k in imports_liste))
if imports_file:
    open(imports_file, 'w').write(','.join(n for n, k in imports_liste))
# ⚠ Miroir du registre Rust (wasm_lecteur.rs::HOTES_INTEGRES) — VAGUE 32.
# 22 fonctions WASI par namespace (dont les stubs sandboxes path_*/prestat_*).
WASI_NOMS = ['fd_write', 'fd_read', 'proc_exit', 'random_get',
             'args_get', 'args_sizes_get', 'environ_get', 'environ_sizes_get',
             'clock_time_get', 'sched_yield', 'fd_close', 'fd_seek',
             'fd_fdstat_get', 'fd_filestat_get', 'fd_prestat_get', 'fd_prestat_dir_name',
             'fd_sync', 'fd_datasync',
             'path_open', 'path_filestat_get', 'path_link', 'path_rename',
             'path_symlink', 'path_readlink', 'path_unlink_file',
             'path_remove_directory', 'path_create_directory']
HOSTS = {'env.log', 'env.ecrire'}
# ⚠ Miroir de l'allocateur sandboxé (VAGUE 33, wasm_lecteur.rs::HOTES_INTEGRES).
HOSTS.update({'env.malloc', 'env.calloc', 'env.realloc', 'env.free', 'env.exit'})
for ns in ('wasi_snapshot_preview1', 'wasi_unstable'):
    for n in WASI_NOMS:
        HOSTS.add(ns + '.' + n)
manquants = [e for e in attendus if e not in fns]
if manquants:
    raise SystemExit("ERREUR: exports attendus absents du wasm: " + ', '.join(manquants) +
                     " (exports reels: " + ','.join(fns) + ") — pour WAT/Rust, le nom d'export vient du source")
nonres = []
for n, k in imports_liste:
    if k == 0 and n not in HOSTS: nonres.append(n + ' (fonction)')
    elif k == 2 and n != 'env.memory': nonres.append(n + ' (memoire)')
    # k == 1 (table) et k == 3 (global) sont ACCEPTES depuis la vague 32 :
    # le pont alloue la table et initialise les globals importes a zero.
if nonres:
    raise SystemExit("ERREUR: imports non resolus par le pont Rose: " + ', '.join(nonres))
bad = []
if imports and not allow: bad.append("%d import(s) (utiliser --allow-imports)" % imports)
# ⚠ VAGUE 32 : `start` et `call_indirect` ne sont PLUS des motifs de refus.
# Le pont les execute (table d'elements, verification de type, budget borne).
if not mem: bad.append("memoire non exportee")
if bad:
    raise SystemExit("ERREUR: incompatible avec le pont Rose: " + ", ".join(bad))
PY

# ---------------------------------------------------------------------------
# Generation du fichier . Rose (declarations @dll/@sym/fn pretes a l'emploi)
# ---------------------------------------------------------------------------
{
    echo "# AUTO-GENERE par rose-wasm-lib.sh — importer ces declarations dans Rose."
    echo "# Module: $OUT_WASM_ABS"
    if [[ -s "$WORK/imports.txt" ]]; then
        echo "# imports resolus par le pont Rose: $(cat "$WORK/imports.txt")"
    else
        echo "# imports: 0 (module autonome)"
    fi
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
echo "==> Verifie: imports resolus, memoire exportee (start/call_indirect: voir WASM_CHECK)."
if (( ALLOW_IMPORTS )); then
    echo "==> --allow-imports: imports de fonction/MEMOIRE/TABLE/GLOBAL autorises et verifies."
else
    echo "==> 0 import exige (comportement C historique)."
fi
if (( MALLOC )); then
    echo "==> --malloc: allocateur sandboxe env.malloc/calloc/realloc/free (tas incremental, plafond 32 Mio, free=no-op)."
fi
echo "==> Optimisation: $OPT_CFLAG (pont: call_indirect + start + budget bornes)."
