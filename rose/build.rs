// Aucune dépendance externe : pas de GMP, pas de .a.
// Lie la libc/la libm et rend le binaire exportable pour le JIT Cranelift
// (ses imports rose_powf / rose_affiche_* sont résolus par dlsym DANS ce
// binaire : les symboles #[unsafe(no_mangle)] doivent être dans la table des
// symboles dynamiques).
//
// Sous Windows (MinGW), on compile en plus src/windows_compat.c qui fournit
// les stubs dlopen/dlsym/dlclose (mappés sur LoadLibraryA/GetProcAddress/
// FreeLibrary) et getrandom (mappé sur SystemFunction036/RtlGenRandom de
// advapi32). Aucune option -lm/-lc/-rdynamic n'est nécessaire sous Windows.
fn main() {
    if std::env::var_os("CARGO_CFG_TARGET_OS").map_or(false, |v| v == "windows") {
        // Compiler le fichier de compatibilité Windows
        let src = std::path::Path::new("src/windows_compat.c");
        if src.exists() {
            cc::Build::new()
                .file(src)
                .flag("-static")
                .compile("windows_compat");
        }
        println!("cargo:rustc-link-lib=dylib=advapi32");
        return;
    }

    // Sur les cibles Unix, on lie explicitement libm/libc et on rend les
    // symboles dynamiques pour le JIT Cranelift (dlsym résout rose_powf,
    // rose_affiche_* dans ce binaire).
    println!("cargo:rustc-link-arg=-lm");
    println!("cargo:rustc-link-arg=-lc");
    println!("cargo:rustc-link-arg=-rdynamic");
}
