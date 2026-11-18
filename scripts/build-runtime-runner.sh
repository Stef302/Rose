#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════
# M6.2 — Construire le VRAI runner wasmtime officiel de Rose.
#
# Usage :  bash scripts/build-runtime-runner.sh
#
# Ce script est REPRODUCTIBLE et NE PUBLIE RIEN :
#   - il ecrit un crate temporaire HORS du depot (contre-jardin Rust) ;
#   - il compile en release avec wasmtime 50.0.1 (version epinglee) ;
#   - il copie le binaire sous le nom d'artefact attendu par
#     rose/src/wasm_runtime.rs ;
#   - il verifie le contrat fib(27) -> 196418 ;
#   - il affiche le SHA-256 a passer a ROSE_WASM_RUNTIME_SHA256.
#
# Il ne touche a aucun .rs du depot, ni a Cargo.toml, ni a PLAN.md.
# Il ne fait aucun git commit et aucun `gh release upload`.
#
# Variables d'environnement optionnelles :
#   ROSE_RUNNER_BUILD_DIR  dossier du crate temporaire
#                          (defaut : /tmp/opencode/rose-wasmtime-runner)
#   ROSE_RUNNER_TARGET     suffixe de l'artefact (defaut : aarch64-linux)
#   CARGO_BUILD_JOBS       parallelisme cargo (defaut : 8)
# ═══════════════════════════════════════════════════════════════════════
set -euo pipefail

VERSION="0.1.5"
WASMTIME_VERSION="50.0.1"
BUILD_DIR="${ROSE_RUNNER_BUILD_DIR:-/tmp/opencode/rose-wasmtime-runner}"
TARGET="${ROSE_RUNNER_TARGET:-aarch64-linux}"
ARTIFACT="rose-wasmtime-runner-v${VERSION}-${TARGET}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-8}"

# Module fib RECURSIF (i32 -> i32), 61 octets, export "fib".
# md5    : 1ea8a09ca1186a7f9de48d2068f2dcba
# sha256 : 00c490e30433166670217e7999f5bd5dd678e67712ba6850ec94ca12c15119d3
MOD_BASE64="AGFzbQEAAAABBgFgAX8BfwMCAQAHBwEDZmliAAAKHgEcACAAQQJIBH8gAAUgAEEBaxAAIABBAmsQAGoLCw=="

# ── 0. Localiser cargo (hors PATH dans certaines sessions) ───────────
if command -v cargo >/dev/null 2>&1; then
  CARGO="$(command -v cargo)"
elif [ -x "$HOME/.cargo/bin/cargo" ]; then
  CARGO="$HOME/.cargo/bin/cargo"
else
  echo "ERREUR: cargo introuvable (installer Rust ou ajouter ~/.cargo/bin au PATH)" >&2
  exit 127
fi
echo "cargo      : $CARGO"
"$CARGO" --version

# rustc embarque le chemin source dans le binaire (BuildID + chaines) :
# sans remap, deux BUILD_DIR differents donnent deux SHA differents.
# Le prefixe fixe "/rose-runner" rend l'artefact independant du dossier.
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$BUILD_DIR=/rose-runner"
# Preserver les rustflags aarch64 de ~/.cargo/config.toml (RUSTFLAGS les ecrase).
if [ "$(uname -m)" = "aarch64" ] && [ -d "$HOME/.local/lib-links" ]; then
  export RUSTFLAGS="$RUSTFLAGS -L $HOME/.local/lib-links"
fi

# ── 1. Crate temporaire Hors du depot ────────────────────────────────
mkdir -p "$BUILD_DIR/src"
cat > "$BUILD_DIR/Cargo.toml" <<'EOF'
[package]
name = "rose-wasmtime-runner"
version = "0.1.5"
edition = "2021"
publish = false

# Runner autonome pour le contrat M6.2 de Rose.
# Hors du depot : aucune dependance Rust n'est ajoutee a Rose.
[dependencies]
wasmtime = { version = "=50.0.1", default-features = false, features = ["cranelift", "runtime"] }

[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
strip = true
panic = "abort"
EOF

cat > "$BUILD_DIR/src/main.rs" <<'EOF'
//! Runner WebAssembly officiel — contrat M6.2 de Rose.
//!
//! Appel : runner <module.wasm> <export> [arg1 arg2 ...]
//! Sortie : un entier decimal signe par ligne sur stdout (une ligne par
//!         valeur de retour de l'export). Erreurs sur stderr, code != 0.

use wasmtime::{Engine, Instance, Module, Store, Val, ValType};

fn die(code: i32, msg: impl std::fmt::Display) -> ! {
    eprintln!("{msg}");
    std::process::exit(code);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        die(2, "usage: runner <module.wasm> <export> [args...]");
    }

    let engine = Engine::default();
    let module = Module::from_file(&engine, &args[0])
        .unwrap_or_else(|e| die(1, format!("chargement module: {e}")));

    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[])
        .unwrap_or_else(|e| die(1, format!("instantiation: {e}")));

    let func = match instance.get_func(&mut store, &args[1]) {
        Some(f) => f,
        None => die(1, format!("export '{}' introuvable", args[1])),
    };

    let ty = func.ty(&store);
    let mut params: Vec<Val> = Vec::new();
    for (i, t) in ty.params().enumerate() {
        let raw: i64 = match args.get(i + 2).and_then(|s| s.parse::<i64>().ok()) {
            Some(v) => v,
            None => die(2, format!("argument {i} manquant/invalide")),
        };
        let val = match t {
            ValType::I32 => Val::I32(raw as i32),
            ValType::I64 => Val::I64(raw),
            ValType::F32 => Val::F32((raw as f32).to_bits()),
            ValType::F64 => Val::F64((raw as f64).to_bits()),
            other => die(2, format!("type non pris en charge: {other:?}")),
        };
        params.push(val);
    }

    let mut results = vec![Val::I32(0); ty.results().len()];
    func.call(&mut store, &params, &mut results)
        .unwrap_or_else(|e| die(1, format!("appel de '{}': {e}", args[1])));

    for r in results {
        let n: i64 = match r {
            Val::I32(x) => x as i64,
            Val::I64(x) => x,
            Val::F32(bits) => f32::from_bits(bits) as i64,
            Val::F64(bits) => f64::from_bits(bits) as i64,
            other => die(2, format!("resultat non entier: {other:?}")),
        };
        println!("{n}");
    }
}
EOF

echo "build dir  : $BUILD_DIR"

# ── 2. Compilation release ───────────────────────────────────────────
echo "wasmtime   : $WASMTIME_VERSION (features: cranelift + runtime)"
( cd "$BUILD_DIR" && "$CARGO" build --release )

# ── 3. Artefact au nom exact du contrat ──────────────────────────────
cp "$BUILD_DIR/target/release/rose-wasmtime-runner" "$BUILD_DIR/$ARTIFACT"
chmod 755 "$BUILD_DIR/$ARTIFACT"

# ── 4. Auto-verification du contrat ──────────────────────────────────
MOD="$(mktemp /tmp/rose-runner-fib27-XXXXXX.wasm)"
trap 'rm -f "$MOD"' EXIT
printf '%s' "$MOD_BASE64" | base64 -d > "$MOD"
SORTIE="$("$BUILD_DIR/$ARTIFACT" "$MOD" fib 27)"
if [ "$SORTIE" != "196418" ]; then
  echo "ERREUR: fib(27) a rendu '$SORTIE' au lieu de 196418" >&2
  exit 1
fi
echo "contrat    : fib(27) -> $SORTIE  OK"

# ── 5. Empreintes ────────────────────────────────────────────────────
SHA_BINAIRE="$(sha256sum "$BUILD_DIR/$ARTIFACT" | awk '{print $1}')"
SHA_LOCK="$(sha256sum "$BUILD_DIR/Cargo.lock" | awk '{print $1}')"

echo
echo "artefact   : $BUILD_DIR/$ARTIFACT"
echo "taille     : $(stat -c%s "$BUILD_DIR/$ARTIFACT") octets"
echo "sha256     : $SHA_BINAIRE"
echo "Cargo.lock : $SHA_LOCK  (reproductibilite : doit etre 3f0221319ab5bdbca4b4ae724bd47b893d1ac3c6a72e0c2d427abbdce9a7a26f)"
echo
echo ">>> A passer a l'orchestrateur (ou via ROSE_WASM_RUNTIME_URL miroir) :"
echo "    ROSE_WASM_RUNTIME_SHA256=$SHA_BINAIRE"
echo
echo ">>> Gate de performance M6.2 (6 echantillons, mediane <= 20 ms) :"
echo "    ROSE_CACHE=/tmp/opencode/rose-cache bash scripts/mesurer-wasm.sh"
echo
echo "CE SCRIPT NE PUBLIE RIEN (aucun gh release upload, aucun commit)."
