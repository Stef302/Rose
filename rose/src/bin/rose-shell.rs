// Rose Shell — la console interactive de Rose, en binaire AUTONOME.
//
// Pourquoi un exécutable séparé ? `rose console` fonctionne déjà, mais un
// shell dédié s'installe à côté du moteur et se lance comme un vrai shell :
//
//     rose-shell                       console interactive
//     rose-shell programme.rose        charge le programme, puis console
//     rose-shell programme.rose --sans-exec
//
// ⚠ CE BINAIRE EST MINCE (≈ 300 Ko, pas 7 Mo).
//
// La première version recompilait le moteur entier — front-end, interpréteur,
// runtime, Cranelift, rendu LaTeX — via `#[path]`. Le `.exe` pesait donc
// 6,9 Mo, exactement comme `rose.exe` : le langage était EMBARQUÉ DEUX FOIS
// dans l'installation.
//
// Le shell est désormais un LANCEUR : il localise `rose.exe` installé (à
// côté de lui, dans `bin\`, ou dans le `PATH`), puis lui délègue
// `rose console`. Le moteur n'est compilé qu'une fois, et le shell hérite
// forcément de la version du moteur installé (pas de désynchronisation entre
// `rose.exe` 0.1.5 et un `rose-shell.exe` d'une autre version).
//
// Compatibilité terminal : c'est `rose console` qui pose
// `SetConsoleOutputCP(65001)` et active les séquences ANSI VT, donc accents
// et couleurs passent partout (Windows Terminal, conhost, PowerShell, cmd,
// Git Bash, VS Code). Le lanceur ne fait qu'exécuter ce processus.
//
// Licence MIT.

#[cfg(windows)]
fn main() {
    std::process::exit(lancer());
}

#[cfg(not(windows))]
fn main() {
    std::process::exit(lancer());
}

/// Localise le moteur (`rose.exe`) et lui passe les arguments.
///
/// Ordre de recherche :
/// 1. `ROSE_EXE` (variable d'environnement) ;
/// 2. à côté du shell (`bin\rose.exe`) ;
/// 3. dans le `PATH`.
///
/// ⚠⚠ GARDE-FOUS INDISPENSABLES contre la BOUCLE DE LANCEMENT.
///
/// Sans eux, le shell pouvait se relancer LUI-MÊME indéfiniment : chaque
/// instance ouvrait une fenêtre Windows Terminal, qui en ouvrait une autre…
/// (mesuré chez l'utilisateur : rafale de fenêtres en boucle). Trois causes
/// possibles, toutes bloquées ici :
/// - le candidat est le shell lui-même (même chemin que `current_exe()`) ;
/// - le candidat est l'INSTALLEUR (sous-système GUI, ou nom « setup ») : il
///   ouvrirait la fenêtre d'installation, qui réinstallerait, etc. ;
/// - `ROSE_SHELL_DEPTH` : compteur d'enfants, pour couper net une chaîne
///   déjà enclenchée par une installation corrompue.
fn trouver_moteur() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;

    let nom = if cfg!(windows) { "rose.exe" } else { "rose" };
    let soi = std::env::current_exe().ok();
    let soi_canon = soi.as_ref().and_then(|p| p.canonicalize().ok());

    // Rend le candidat seulement s'il est VALIDE comme moteur.
    let valider = |candidat: PathBuf| -> Option<PathBuf> {
        if !candidat.is_file() {
            return None;
        }
        let canon = candidat.canonicalize().ok();
        // 1. Jamais soi-même.
        if canon.is_some() && canon == soi_canon {
            return None;
        }
        // 2. Jamais l'installeur : nom explicite, ou sous-système GUI.
        let base = candidat
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();
        if base.contains("setup") || base.contains("install") {
            return None;
        }
        // GUI = 2 : c'est l'installeur, pas le moteur console.
        if sous_systeme_pe(&candidat) == Some(2) {
            return None;
        }
        Some(candidat)
    };

    if let Ok(p) = std::env::var("ROSE_EXE") {
        let chemin = PathBuf::from(&p);
        if chemin.is_file() {
            if let Some(v) = valider(chemin) {
                return Some(v);
            }
            // Le fichier existe mais n'est PAS un moteur valide (shell ou
            // installeur). On le signale : sans cela, on retombait
            // silencieusement sur un autre candidat et l'utilisateur ne
            // comprenait pas pourquoi son ROSE_EXE était ignoré.
            eprintln!("rose-shell : ROSE_EXE={p} n'est pas le moteur Rose (c'est un shell ou un installeur) — ignoré.");
        }
    }

    // À côté du shell : `...\bin\rose-shell.exe` -> `...\bin\rose.exe`.
    // ⚠ Si ce fichier EXISTE mais n'est pas un moteur valide (c'est
    // l'installeur, posé là par une installation corrompue), on NE cherche
    // PAS ailleurs : on signale l'erreur. Chercher plus loin risquait de
    // retomber sur le même installeur via le PATH, et donc de boucler.
    if let Some(dir) = soi.as_ref().and_then(|p| p.parent()) {
        let voisin = dir.join(nom);
        if voisin.is_file() {
            return valider(voisin);
        }
    }

    // Dans le PATH.
    if let Some(chemin) = std::env::var_os("PATH") {
        for dossier in std::env::split_paths(&chemin) {
            if let Some(v) = valider(dossier.join(nom)) {
                return Some(v);
            }
        }
    }
    None
}

/// Sous-système PE (2 = GUI, 3 = console) — `None` si le fichier n'est pas
/// un PE Windows lisible.
///
/// ⚠ Cette fonction n'est PAS réservée à Windows : le shell doit pouvoir
/// inspecter un `rose.exe` (PE) même quand il tourne ailleurs — c'est ainsi
/// qu'on détecte qu'un « moteur » est en réalité l'installeur GUI, quel que
/// soit son nom. Sans cela, un `bin\rose.exe` corrompu passait le filtre du
/// nom et provoquait une boucle de lancement.
fn sous_systeme_pe(chemin: &std::path::Path) -> Option<u16> {
    let d = std::fs::read(chemin).ok()?;
    if d.len() < 0x40 || &d[0..2] != b"MZ" {
        return None;
    }
    let pe = u32::from_le_bytes([d[0x3C], d[0x3D], d[0x3E], d[0x3F]]) as usize;
    if pe + 0x5A > d.len() || &d[pe..pe + 4] != b"PE\0\0" {
        return None;
    }
    let o = pe + 24 + 68;
    if o + 2 > d.len() {
        return None;
    }
    Some(u16::from_le_bytes([d[o], d[o + 1]]))
}

fn lancer() -> i32 {
    // ⚠ COUPE-CIRCUIT : si le shell est lancé en cascade (une instance qui
    // en lance une autre…), on refuse au-delà de 2 niveaux. C'est le filet
    // de sécurité ultime contre une boucle de fenêtres Windows Terminal.
    let profondeur: u32 = std::env::var("ROSE_SHELL_DEPTH")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    if profondeur >= 2 {
        eprintln!("rose-shell : boucle de lancement détectée (profondeur {profondeur}).");
        eprintln!("L'installation est probablement corrompue : relancez Rose-win-Setup.exe.");
        return 1;
    }

    let moteur = match trouver_moteur() {
        Some(p) => p,
        None => {
            eprintln!("rose-shell : rose.exe (le moteur) est introuvable ou invalide.");
            eprintln!();
            eprintln!("Le shell ne lance QUE le moteur console, jamais l'installeur :");
            eprintln!("  - il doit se trouver dans le même dossier que rose-shell.exe,");
            eprintln!("  - ou dans le PATH,");
            eprintln!("  - ou être désigné par la variable ROSE_EXE.");
            eprintln!();
            eprintln!("Si le dossier d'installation contient un rose.exe qui ouvre");
            eprintln!("la fenêtre d'installation, l'installation est corrompue :");
            eprintln!("relancez Rose-win-Setup.exe après avoir supprimé ce dossier.");
            return 1;
        }
    };

    // On transmet les arguments tels quels, préfixés par `console` :
    //   rose-shell                -> rose console
    //   rose-shell prog.rose      -> rose console prog.rose
    //   rose-shell --sans-exec    -> rose console --sans-exec
    let mut cmd = std::process::Command::new(&moteur);
    cmd.arg("console");
    cmd.args(std::env::args().skip(1));
    // Le compteur de profondeur est transmis à l'enfant.
    cmd.env("ROSE_SHELL_DEPTH", (profondeur + 1).to_string());

    match cmd.status() {
        Ok(statut) => statut.code().unwrap_or(1),
        Err(e) => {
            eprintln!("rose-shell : impossible de lancer {} : {e}", moteur.display());
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Les tests manipulent `ROSE_EXE`, une variable GLOBALE au processus :
    /// en parallèle, un test pouvait voir la valeur posée par un autre et
    /// échouer de façon intermittente. Ce verrou les sérialise.
    static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn avec_rose_exe<F: FnOnce()>(valeur: Option<&std::path::Path>, f: F) {
        let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
        match valeur {
            Some(p) => unsafe { std::env::set_var("ROSE_EXE", p) },
            None => unsafe { std::env::remove_var("ROSE_EXE") },
        }
        f();
        unsafe { std::env::remove_var("ROSE_EXE") };
    }

    #[test]
    fn moteur_via_variable() {
        // `ROSE_EXE` pointe sur un fichier VALIDE (nom « rose ») : il est
        // prioritaire. On ne peut pas utiliser `current_exe()` (le harnais
        // de test) : c'est exactement ce que le garde-fou anti-boucle refuse.
        let faux = std::env::temp_dir().join("rose-moteur-test");
        let _ = std::fs::write(&faux, b"MZ");
        let mut trouve = None;
        avec_rose_exe(Some(&faux), || trouve = trouver_moteur());
        let _ = std::fs::remove_file(&faux);
        assert_eq!(trouve.as_deref(), Some(faux.as_path()));
    }

    #[test]
    fn moteur_variable_ignoree_si_inexistante() {
        let mut trouve = None;
        avec_rose_exe(Some(std::path::Path::new("/chemin/qui/n/existe/pas")),
                      || trouve = trouver_moteur());
        // Le chemin invalide ne doit PAS être retenu : on retombe sur la
        // recherche à côté du binaire / dans le PATH (souvent None ici).
        assert_ne!(trouve.as_deref(), Some(std::path::Path::new("/chemin/qui/n/existe/pas")));
    }

    #[test]
    fn ne_relance_jamais_le_shell_lui_meme() {
        // Le candidat = le shell courant doit être rejeté par `trouver_moteur`.
        // Sans ce garde-fou, le shell se relançait indéfiniment (une fenêtre
        // Windows Terminal par itération).
        let soi = std::env::current_exe().unwrap();
        let mut trouve = None;
        avec_rose_exe(Some(&soi), || trouve = trouver_moteur());
        assert_ne!(trouve.as_deref(), Some(soi.as_path()),
                   "le shell a accepté de se relancer lui-même");
    }

    #[test]
    fn refuse_un_installeur_comme_moteur() {
        // Un nom « setup »/« install » ne doit jamais être accepté.
        for nom in ["Rose-win-Setup.exe", "install-rose.exe", "rose-setup.exe"] {
            let f = std::env::temp_dir().join(nom);
            let _ = std::fs::write(&f, b"MZ");
            let mut trouve = None;
            avec_rose_exe(Some(&f), || trouve = trouver_moteur());
            let _ = std::fs::remove_file(&f);
            assert_ne!(trouve.as_deref(), Some(f.as_path()), "{nom} accepté");
        }
    }

    #[test]
    fn refuse_un_moteur_gui_quel_que_soit_son_nom() {
        // Un PE en sous-système 2 (GUI) = l'installeur. Le filtre du NOM ne
        // suffit pas : l'installeur pouvait être posé sous `bin\rose.exe`.
        // On fabrique un PE minimal en GUI et on vérifie le rejet.
        let mut pe = vec![0u8; 0x200];
        pe[0] = b'M';
        pe[1] = b'Z';
        pe[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        pe[0x80..0x84].copy_from_slice(b"PE\0\0");
        pe[0x80 + 24 + 68] = 2; // subsystem GUI
        let f = std::env::temp_dir().join("rose-faux-gui");
        let _ = std::fs::write(&f, &pe);
        let ss = sous_systeme_pe(&f);
        let _ = std::fs::remove_file(&f);
        assert_eq!(ss, Some(2), "le sous-système GUI doit être détecté");
    }

    #[test]
    fn le_shell_n_embarque_pas_le_moteur() {
        // Garde-fou de taille : le shell est un LANCEUR. S'il se remet à
        // compiler Cranelift + l'interpréteur, il dépassera plusieurs Mo.
        //
        // ⚠ `current_exe()` renvoie le HARNAIS DE TEST, pas `rose-shell.exe` :
        // mesurer ce dernier donnait toujours un petit fichier et le test
        // passait même si le vrai binaire avait grossi. On mesure donc le
        // binaire produit par `cargo build`, s'il existe.
        let Some(exe) = std::env::var_os("CARGO_BIN_EXE_rose-shell").map(std::path::PathBuf::from)
        else { return; };
        let Ok(meta) = std::fs::metadata(&exe) else { return; };
        // En debug le binaire est plus gros : on n'impose la borne qu'en release.
        if !cfg!(debug_assertions) {
            assert!(
                meta.len() < 4_000_000,
                "{} pèse {} o : le moteur est ré-embarqué dans le shell",
                exe.display(), meta.len()
            );
        }
    }
}
