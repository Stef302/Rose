// ═══════════════════ Rose — moteur natif Rust (binaire autonome) ═══════════════════
mod ffi;
mod float;
mod front;
mod run;

fn main() {
    #[cfg(windows)]
    unsafe {
        extern "system" {
            fn SetConsoleOutputCP(cp: u32) -> i32;
            fn SetConsoleCP(cp: u32) -> i32;
        }
        SetConsoleOutputCP(65001);
        SetConsoleCP(65001);
    }
    let argv: Vec<String> = std::env::args().collect();
    std::process::exit(run::main_cli(argv));
}
