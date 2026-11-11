fn main() {
    // WebAssembly : pas de libc système, donc pas de -lm/-lc/-rdynamic
    // (le lieur wasm-ld refuse ces drapeaux). Le module WASM n'a besoin
    // d'aucun symbole natif : les helpers du JIT sont absents en WASM.
    if std::env::var_os("CARGO_CFG_TARGET_ARCH").map_or(false, |v| v == "wasm32") {
        return;
    }

    if std::env::var_os("CARGO_CFG_TARGET_OS").map_or(false, |v| v == "windows") {
        // Icône embarquée dans le .exe (Windows)
        #[cfg(target_os = "windows")]
        {
            // Utilise winres si disponible (crate optionnel)
        }
        let src = std::path::Path::new("src/windows_compat.c");
        if src.exists() {
            cc::Build::new()
                .file(src)
                .flag_if_supported("-static")
                .compile("windows_compat");
        }
        println!("cargo:rustc-link-arg=-Wl,--export-all-symbols");
        println!("cargo:rustc-link-lib=dylib=advapi32");
        let def = std::path::Path::new("exports.def");
        if def.exists() {
            println!("cargo:rustc-link-arg=/DEF:exports.def");
        }
        // Compile .rc icon if present
        let rc = std::path::Path::new("rose.rc");
        if rc.exists() {
            // llvm-windres (LLVM) -> COFF .res compatible MSVC linker
            for tool in ["llvm-windres", "windres", "x86_64-w64-mingw32-windres"] {
                let st = std::process::Command::new(tool)
                    .args(["rose.rc", "--target=pe-x86-64", "-O", "coff", "-o", "rose.res"])
                    .status();
                if st.map(|s| s.success()).unwrap_or(false) { break; }
            }
            let res = std::path::Path::new("rose.res");
            if res.exists() {
                println!("cargo:rustc-link-arg=rose.res");
            }
        }
        return;
    }

    println!("cargo:rustc-link-arg=-lm");
    println!("cargo:rustc-link-arg=-lc");
    println!("cargo:rustc-link-arg=-rdynamic");
}
