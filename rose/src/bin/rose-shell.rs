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

/// Localise `rose.exe` (ou `rose` hors Windows) et lui passe les arguments.
///
/// Ordre de recherche :
/// 1. `ROSE_EXE` (variable d'environnement) — pour les tests et les
///    installations déplacées ;
/// 2. à côté du shell (`bin\rose.exe`) — c'est la disposition de
///    l'installeur ;
/// 3. dans le `PATH` — une installation système.
fn trouver_moteur() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;

    if let Ok(p) = std::env::var("ROSE_EXE") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }

    let nom = if cfg!(windows) { "rose.exe" } else { "rose" };

    // À côté du shell : `...\bin\rose-shell.exe` -> `...\bin\rose.exe`.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidat = dir.join(nom);
            if candidat.is_file() {
                return Some(candidat);
            }
        }
    }

    // Dans le PATH.
    let chemin = std::env::var_os("PATH")?;
    for dossier in std::env::split_paths(&chemin) {
        let candidat = dossier.join(nom);
        if candidat.is_file() {
            return Some(candidat);
        }
    }
    None
}

fn lancer() -> i32 {
    let moteur = match trouver_moteur() {
        Some(p) => p,
        None => {
            eprintln!("rose-shell : rose.exe est introuvable.");
            eprintln!();
            eprintln!("Le shell a besoin du moteur Rose installé. Cherchez-le :");
            eprintln!("  1. dans le même dossier que rose-shell.exe (bin\\),");
            eprintln!("  2. ou dans le PATH (installation système),");
            eprintln!("  3. ou désignez-le avec la variable ROSE_EXE.");
            eprintln!();
            eprintln!("Installez Rose avec Rose-win-Setup.exe si ce n'est pas déjà fait.");
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

    #[test]
    fn moteur_via_variable() {
        // `ROSE_EXE` pointe sur un fichier existant : c'est prioritaire.
        let exe = std::env::current_exe().unwrap();
        // SAFETY : variable d'environnement du processus de test.
        unsafe { std::env::set_var("ROSE_EXE", &exe) };
        assert_eq!(trouver_moteur().as_deref(), Some(exe.as_path()));
        unsafe { std::env::remove_var("ROSE_EXE") };
    }

    #[test]
    fn moteur_variable_ignoree_si_inexistante() {
        unsafe { std::env::set_var("ROSE_EXE", "/chemin/qui/n/existe/pas") };
        let trouve = trouver_moteur();
        unsafe { std::env::remove_var("ROSE_EXE") };
        // Le chemin invalide ne doit PAS être retenu : on retombe sur la
        // recherche à côté du binaire / dans le PATH (souvent None ici).
        assert_ne!(trouve.as_deref(), Some(std::path::Path::new("/chemin/qui/n/existe/pas")));
    }

    #[test]
    fn le_shell_n_embarque_pas_le_moteur() {
        // Garde-fou de taille : le shell est un LANCEUR. S'il se remet à
        // compiler Cranelift + l'interpréteur, il dépassera plusieurs Mo.
        // On mesure le fichier pour attraper une régression d'architecture.
        let exe = std::env::current_exe().unwrap();
        let taille = std::fs::metadata(&exe).map(|m| m.len()).unwrap_or(0);
        // En debug les binaires sont plus gros ; on ne teste qu'en release.
        if !cfg!(debug_assertions) && taille > 0 {
            assert!(taille < 4_000_000, "rose-shell pèse {taille} o : le moteur est ré-embarqué");
        }
    }
}
