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
        // Forcer le lien du windows_compat.lib (cargo-xwin/lld-link ne le
        // tire pas toujours via -lwindows_compat).
        let out = std::path::Path::new("target")
            .join(std::env::var("TARGET").unwrap_or_default())
            .join("release")
            .join("build");
        // chemin réel sera résolu par cargo via link-search ; on ajoute aussi
        // l'argument explicite en cherchant le .lib le plus récent.
        if let Ok(entries) = std::fs::read_dir("target") {
            for _ in entries {}
        }
        println!("cargo:rustc-link-arg=/WHOLEARCHIVE:windows_compat.lib");
        println!("cargo:rustc-link-arg=-Wl,--export-all-symbols");
        println!("cargo:rustc-link-lib=dylib=advapi32");
        // Le fichier .def exporte les helpers du JIT (rose_affiche_*,
        // rose_powf) : ils n'existent QUE dans le binaire « rose » (qui
        // inclut le module jit), PAS dans « rose-aot » (qui embarque ses
        // propres helpers rose_orc_*). Le lier inconditionnellement
        // faisait échouer le lien de rose-aot sur un symbole absent.
        // « rustc-link-arg-bin » applique l'argument à UN seul binaire.
        // L'installeur graphique est une application GUI (pas de console noire
        // derrière la fenêtre) : seul ce binaire passe en /SUBSYSTEM:WINDOWS.
        // rustc pose déjà son propre point d'entrée (mainCRTStartup) : on le
        // fixe explicitement, sinon le CRT msvcrt réclame WinMain.
        println!("cargo:rustc-link-arg-bin=rose-setup=/SUBSYSTEM:WINDOWS");
        println!("cargo:rustc-link-arg-bin=rose-setup=/ENTRY:mainCRTStartup");
        let def = std::path::Path::new("exports.def");
        if def.exists() {
            println!("cargo:rustc-link-arg-bin=rose=/DEF:exports.def");
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
                // Un SEUL fichier de ressources par exécutable : l'installeur
                // reçoit le sien (rose-setup.res, icônes multi-tailles + rose
                // + police Kare) ; les autres reçoivent rose.rc (icône).
                for bin in ["rose", "rose-aot"] {
                    println!("cargo:rustc-link-arg-bin={bin}=rose.res");
                }
            }
        }
        // L'installeur graphique embarque ses ressources (rose PNG, police
        // Kare, icônes multi-tailles) dans `rose-setup.res`, produit par
        // `installateur/velopack/pack-res.py` : une section `.rsrc` COFF
        // complète, lue par FindResourceW dans setup_gui. Le fichier n'est
        // lié qu'au binaire `rose-setup` (les autres n'en ont pas besoin).
        let res_setup = std::path::Path::new("rose-setup.res");
        if res_setup.exists() {
            println!("cargo:rustc-link-arg-bin=rose-setup=rose-setup.res");
            println!("cargo:rerun-if-changed=rose-setup.res");
        }
        return;
    }

    println!("cargo:rustc-link-arg=-lm");
    println!("cargo:rustc-link-arg=-lc");
    println!("cargo:rustc-link-arg=-rdynamic");
}
