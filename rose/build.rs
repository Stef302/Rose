// Aucune dépendance externe : pas de GMP, pas de .a.
// Lie la libc/la libm et rend le binaire exportable pour le JIT Cranelift
// (ses imports rose_powf / rose_affiche_* sont résolus par dlsym DANS ce
// binaire : les symboles #[unsafe(no_mangle)] doivent être dans la table des
// symboles dynamiques).
fn main() {
    println!("cargo:rustc-link-arg=-lm");
    println!("cargo:rustc-link-arg=-lc");
    println!("cargo:rustc-link-arg=-rdynamic");
}
