// Helper AOT de Rose : la chaîne LLVM (`orc`, `link`) vit dans un binaire
// SÉPARÉ, que le binaire principal télécharge à la demande quand il n'est
// pas embarqué (voir src/auto_llvm.rs).
//
// Utilisation :
//     rose-aot orc  programme.rose -o programme.ll
//     rose-aot link programme.o   -o programme
//
// Ce binaire est construit avec la feature « llvm » pour être utile :
//     cargo build --release --features llvm --bin rose-aot
//
// Licence MIT.

// Les modules partagés sont repris tels quels (#[path]) : le helper n'a
// besoin que du front-end, du runtime et de la chaîne LLVM.
#[path = "../front.rs"]
mod front;
#[path = "../run.rs"]
mod run;
#[path = "../i128x.rs"]
mod i128x;
// i128x.rs appelle `crate::mul_rapide` : le helper AOT doit donc le fournir.
#[path = "../mul_rapide.rs"]
mod mul_rapide;
#[path = "../irrationnel.rs"]
mod irrationnel;
#[path = "../cmd_llvm.rs"]
mod cmd_llvm;
#[cfg(feature = "llvm")]
#[path = "../aot_llvm_orc.rs"]
mod aot_llvm_orc;
#[cfg(feature = "llvm")]
#[path = "../elf_linker.rs"]
mod elf_linker;

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    if argv.len() < 2 {
        eprintln!("rose-aot : usage : rose-aot (orc|link) [arguments…]");
        std::process::exit(2);
    }
    cmd_llvm::execute(&argv[1], &argv);
}
