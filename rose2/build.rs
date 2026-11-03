// Lie GMP statiquement (le .so système n'exporte pas tous les mpz_*), puis la
// libc en dynamique pour __stack_chk_guard — même recette que moteur/rust/build.sh.
fn main() {
    let candidats = [
        "/usr/lib/aarch64-linux-gnu/libgmp.a",
        "/usr/lib/libgmp.a",
        "/usr/lib/x86_64-linux-gnu/libgmp.a",
    ];
    for c in candidats {
        if std::path::Path::new(c).exists() {
            println!("cargo:rustc-link-arg={}", c);
            println!("cargo:rustc-link-arg=-lm");
            println!("cargo:rustc-link-arg=-lc");
            // le JIT Cranelift résout ses imports (rose_powf, rose_affiche_*)
            // par dlsym DANS ce binaire : les symboles #[no_mangle] doivent être
            // dans la table des symboles dynamiques.
            println!("cargo:rustc-link-arg=-rdynamic");
            println!("cargo:rerun-if-changed={}", c);
            return;
        }
    }
    println!("cargo:rustc-link-lib=gmp");
}
