// Rose Shell — la console interactive de Rose, en binaire AUTONOME.
//
// Pourquoi un exécutable séparé ? `rose console` fonctionne déjà, mais un
// shell dédié s'installe à côté du moteur et se lance comme un vrai shell :
//
//     rose-shell                       console interactive
//     rose-shell programme.rose        charge le programme, puis console
//     rose-shell programme.rose --sans-exec
//
// Compatibilité terminal : Windows Terminal, conhost, PowerShell, cmd,
// Git Bash, VS Code — le module `repl` pose `SetConsoleOutputCP(65001)` et
// active les séquences ANSI VT, donc les accents et les couleurs passent
// partout. Sous Linux/macOS il s'utilise comme n'importe quel REPL.
//
// Licence MIT.

// Les modules sont repris tels quels (#[path]) : le shell n'a besoin que du
// front-end, de l'interpréteur, du runtime et du rendu LaTeX.
#[path = "../front.rs"]
mod front;
#[path = "../i128x.rs"]
mod i128x;
#[path = "../irrationnel.rs"]
mod irrationnel;
#[path = "../interp.rs"]
mod interp;
#[path = "../jit.rs"]
mod jit;
#[path = "../run.rs"]
mod run;
#[path = "../latex.rs"]
mod latex;
#[path = "../sha256.rs"]
mod sha256;
#[path = "../auto_llvm.rs"]
mod auto_llvm;
#[path = "../repl.rs"]
mod repl;

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    std::process::exit(repl::depuis_arguments(&argv));
}
