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
            // ⚠ CORRECTION 21/11: cc::Build émet cargo:rustc-link-lib=static=
            // windows_compat, ce qui embarque l'objet DANS librose.rlib. Avec
            // le /WHOLEARCHIVE ci-dessous, le même .o arrive deux fois au lien
            // ("duplicate symbol: mmap/mprotect/...", mesuré rc=101). On compile
            // donc la lib nous-mêmes: un seul .lib, injecté une seule fois.
            let out = std::env::var("OUT_DIR").unwrap_or_else(|_| "target".into());
            let obj = std::path::Path::new(&out).join("windows_compat.o");
            let lib = std::path::Path::new(&out).join("windows_compat.lib");
            let cflags = std::env::var("CFLAGS_x86_64_pc_windows_msvc").unwrap_or_default();
            let target = std::env::var("TARGET").unwrap_or_default();
            let mut cmd = std::process::Command::new("clang");
            cmd.arg(format!("--target={target}")).arg("-O2").arg("-c").arg(src)
               .arg("-o").arg(&obj);
            for f in cflags.split_whitespace() { cmd.arg(f); }
            let st = cmd.status().expect("clang introuvable");
            assert!(st.success(), "compilation windows_compat.c échouée");
            let st2 = std::process::Command::new("llvm-lib")
                .arg(format!("/OUT:{}", lib.display())).arg(&obj)
                .status().expect("llvm-lib introuvable");
            assert!(st2.success(), "création windows_compat.lib échouée");
            println!("cargo:rustc-link-search=native={out}");
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
        // ⚠ MSVC utilise /WHOLEARCHIVE:… ; lld-link GNU (cible *-windows-gnu)
        // ne comprend pas ce drapeau et cherche un fichier '/WHOLEARCHIVE:…'.
        let cible = std::env::var("TARGET").unwrap_or_default();
        if cible.ends_with("-windows-msvc") {
            // ⚠ CORRECTION 21/11: ne PAS ajouter /WHOLEARCHIVE ici. Le .o
            // windows_compat est DEJA embarqué dans librose.rlib (cc::Build
            // émet cargo:rustc-link-lib=static=windows_compat), et le forcer
            // ici donnait « duplicate symbol: dlopen/dlsym/mmap/... » entre
            // windows_compat.lib et librose.rlib (mesuré: lien rc=101).
            println!("cargo:rustc-link-arg=/WHOLEARCHIVE:windows_compat.lib");
        } else {
            println!("cargo:rustc-link-arg=-Wl,--whole-archive");
            println!("cargo:rustc-link-arg=-lwindows_compat");
            println!("cargo:rustc-link-arg=-Wl,--no-whole-archive");
        }
        println!("cargo:rustc-link-arg=-Wl,--export-all-symbols");
        println!("cargo:rustc-link-lib=dylib=advapi32");
        // GDI+ : tracé ANTIALIASÉ des cercles et des coins arrondis de
        // l'installeur (GDI pur ne sait pas anticréneler, d'où l'aspect
        // pixelisé des pastilles d'étapes). Seul `rose-setup` s'en sert.
        println!("cargo:rustc-link-lib=dylib=gdiplus");
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
        // ⚠ exports.def est écrit pour l'ABI x64 (noms non décortiqués).
        // Sur i686 l'exportateur ajoute des underscores/EXPORTAS et lld-link
        // rejette le fichier : on ne l'applique qu'aux cibles 64 bits.
        if def.exists() && cible.contains("64") {
            println!("cargo:rustc-link-arg-bin=rose=/DEF:exports.def");
        }
        // ── Icônes par binaire ─────────────────────────────────────────────
        //
        // ⚠⚠ Chaque exécutable a son PROPRE `.rc` : avant, les trois
        // recevaient `rose.res` (l'icône du SETUP). Résultat mesuré : dans
        // Windows Terminal, un onglet de console Rose affichait une boîte de
        // livraison, et `rose.exe` — le moteur, une CONSOLE — montrait la même
        // icône que l'installeur. Trois fichiers, trois rôles :
        //   rose.rc       → rose.exe       (LOGO DE BASE, la rose)
        //   rose-shell.rc → rose-shell.exe (console : terminal + invite)
        //   rose-aot.rc   → rose-aot.exe   (console : terminal + invite)
        //   rose-tray.rc  → rose-tray.exe  (LA ROSE, embarquée dans le PE)
        //
        // ⚠ Le helper avait une section `.rsrc` VIDE : aucune icône dans
        // l'exe, seulement un `LoadImageW` d'un `rose.ico` externe au
        // runtime. `rose-tray.rc` corrige cela en liant RT_ICON +
        // RT_GROUP_ICON dans le PE (même mécanisme que `pack-res.py` pour
        // rose-setup, vérifié avec `llvm-readobj --coff-resources`).
        //
        // ⚠⚠ CORRECTION : `rose.exe` portait `rose_shell.ico`. C'était une
        // erreur — le moteur doit porter le LOGO DE BASE (`rose_base.ico`,
        // copie de `installateur/assets/rose.ico`), comme avant.
        // L'installeur reçoit `rose-setup.res` (icône de setup + ressources).
        // ⚠ `rose-aot` n'est plus une cible [[bin]] du manifeste : émettre
        // `cargo:rustc-link-arg-bin=rose-aot=…` fait échouer TOUT build
        // Windows avec « does not have a bin target with the name rose-aot ».
        for (bin, rc_name, res_name) in [
            ("rose", "rose.rc", "rose.res"),
            ("rose-shell", "rose-shell.rc", "rose-shell.res"),
            ("rose-tray", "rose-tray.rc", "rose-tray.res"),
        ] {
            let rc = std::path::Path::new(rc_name);
            if !rc.exists() {
                println!("cargo:warning={rc_name} absent : {bin} n'aura pas d'icône");
                continue;
            }
            println!("cargo:rerun-if-changed={rc_name}");
            // ⚠⚠ Le `.rc` référence une icône : si SEULE l'icône change, cargo
            // ne reconstruit pas le `.res` sans cette ligne (cache périmé —
            // mesuré : une icône remplacée restait l'ancienne dans le .exe).
            // On surveille donc aussi les `.ico` du dossier et le dossier
            // des assets partagés.
            println!("cargo:rerun-if-changed=rose.ico");
            println!("cargo:rerun-if-changed=rose_shell.ico");
            println!("cargo:rerun-if-changed=../installateur/assets/rose.ico");
            println!("cargo:rerun-if-changed=../installateur/assets/rose_shell.ico");
            // rc.exe (MSVC, Windows natif) -> COFF .res compatible MSVC linker.
            // llvm-windres / windres (MinGW/LLVM) -> COFF .res.
            let mut fait = false;
            for tool in ["rc.exe", "llvm-windres", "windres", "x86_64-w64-mingw32-windres"] {
                let st = std::process::Command::new(tool)
                    .args([rc_name, "--target=pe-x86-64", "-O", "coff", "-o", res_name])
                    .status();
                if st.map(|s| s.success()).unwrap_or(false) {
                    fait = true;
                    break;
                }
            }
            if !fait {
                println!("cargo:warning=aucun compilateur .rc trouvé : {bin} n'aura pas d'icône");
                continue;
            }
            // Un SEUL fichier de ressources par exécutable.
            println!("cargo:rustc-link-arg-bin={bin}={res_name}");
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
        // Manifeste : DPI PerMonitorV2 (rendu net, pas de bitmap étiré),
        // common controls v6 et Windows 10/11 déclarés.
        // ⚠ lld-link REFUSE un manifeste externe dès qu'un .res est lié
        // (« more than one resource obj file not allowed ») : le manifeste
        // est donc embarqué dans `rose-setup.res` par `pack-res.py`
        // (RT_MANIFEST type 24, id 1), pas passé en argument ici.
        println!("cargo:rerun-if-changed=rose-setup.manifest");
        return;
    }

    println!("cargo:rustc-link-arg=-lm");
    println!("cargo:rustc-link-arg=-lc");
    // `-rdynamic` exporte les symboles du binaire dans la table dynamique :
    // c'est ce qui permet au JIT (Cranelift) de retrouver les helpers
    // `rose_affiche_*` par `dlsym`. Il ne concerne QUE le binaire `rose`.
    //
    // Appliqué à tous les cibles, il casse les binaires de TEST : combiné à
    // `--strip-all` + `-static-pie`, le harnais de test segfaultait au
    // démarrage (SIGSEGV avant même d'afficher le premier test) sur la CI
    // x86_64, alors que le même code passait en local sur aarch64.
    println!("cargo:rustc-link-arg-bin=rose=-rdynamic");
}
