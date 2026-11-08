fn main() {
    if std::env::var_os("CARGO_CFG_TARGET_OS").map_or(false, |v| v == "windows") {
        // Compiler le fichier de compatibilité Windows
        let src = std::path::Path::new("src/windows_compat.c");
        if src.exists() {
            cc::Build::new()
                .file(src)
                .flag_if_supported("-static")
                .compile("windows_compat");
        }
        // Sous Windows (MinGW), un .exe n'exporte pas ses symboles par défaut.
        // --export-all-symbols rend rose_add_ent / rose_sub_ent / etc. visibles
        // depuis le JIT Cranelift (dlsym sur le binaire principal).
        println!("cargo:rustc-link-arg=-Wl,--export-all-symbols");
        println!("cargo:rustc-link-lib=dylib=advapi32");
        // Sous MSVC (cargo xwin), --export-all-symbols n'existe pas : on utilise
        // un fichier .def pour exporter les symboles rose_*.
        let def = std::path::Path::new("exports.def");
        if def.exists() {
            println!("cargo:rustc-link-arg=/DEF:exports.def");
        }
        return;
    }

    // Sur les cibles Unix, on lie explicitement libm/libc et on rend les
    // symboles dynamiques pour le JIT Cranelift (dlsym résout rose_powf,
    // rose_affiche_* dans ce binaire).
    println!("cargo:rustc-link-arg=-lm");
    println!("cargo:rustc-link-arg=-lc");
    println!("cargo:rustc-link-arg=-rdynamic");
}
