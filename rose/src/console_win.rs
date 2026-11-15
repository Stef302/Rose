//! Spécificités Windows de la console interactive.
//!
//! Ce module règle deux problèmes mesurés par l'utilisateur :
//!
//! 1. **Fenêtre de console sans icône.** Quand on double-clique un fichier
//!    `.rose`, l'Explorateur lance `rose.exe`, ce qui ouvre une fenêtre de
//!    console.
//!
//! 2. **La fenêtre se ferme trop vite.** Un programme Rose affiche son
//!    résultat puis se termine : la console disparaît avant qu'on ait pu le
//!    lire. On détecte que la console a été ouverte POUR NOUS (un seul
//!    processus y est rattaché) et on attend une frappe avant de rendre la
//!    main. Lancé depuis un terminal déjà ouvert, rien ne change.
//!
//! `ROSE_NO_PAUSE=1` désactive l'attente (utile en script).
//!
//! ⚠ **NE JAMAIS utiliser `SendMessageW` sur la fenêtre de console.** Elle
//! appartient à `conhost.exe` : un envoi SYNCHRONE depuis le programme
//! console peut se bloquer (le processus attend conhost, qui attend le
//! verrou de console que nous détenons) — le programme ne rend alors plus la
//! main et l'utilisateur ne voit qu'une fenêtre VIDE, avec les bordures
//! seules (mesuré). On utilise `PostMessageW`, qui ne bloque jamais, et on
//! poste depuis un fil séparé pour que le programme ne dépende même pas du
//! résultat.

/// Vrai quand la console a été ouverte pour ce seul programme.
pub fn console_ephemere() -> bool {
    if std::env::var("ROSE_NO_PAUSE").is_ok() || std::env::var("ROSE_SHELL_PERSIST").is_ok() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::ffi::c_void;
        unsafe extern "system" {
            fn GetConsoleProcessList(pids: *mut u32, n: u32) -> u32;
        }
        let mut pids = [0u32; 8];
        // Une console partagée par plusieurs programmes (cmd, PowerShell,
        // Windows Terminal) a plus d'un PID rattaché : dans ce cas, on ne
        // bloque surtout pas.
        unsafe {
            let n = GetConsoleProcessList(pids.as_mut_ptr(), pids.len() as u32);
            n == 1
        }
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Attend une frappe avant de rendre la main (fenêtre ouverte par double-clic).
pub fn attendre_fermeture() {
    if !console_ephemere() {
        return;
    }
    print!("\nAppuyez sur Entrée pour fermer… ");
    use std::io::Write;
    let _ = std::io::stdout().flush();
    let mut ligne = String::new();
    let _ = std::io::stdin().read_line(&mut ligne);
}

/// Prépare la console : titre de la fenêtre, et icône posée en différé.
pub fn preparer() {
    #[cfg(windows)]
    {
        use std::ffi::c_void;
        unsafe extern "system" {
            fn SetConsoleTitleW(titre: *const u16) -> i32;
        }
        unsafe {
            // Titre de la fenêtre : « Rose 0.1.5 ».
            let titre: Vec<u16> = format!("Rose {} — console interactive", env!("CARGO_PKG_VERSION"))
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            SetConsoleTitleW(titre.as_ptr());
        }
        // L'icône est posée dans un fil à part, et par `PostMessageW` :
        // jamais d'attente, jamais de blocage du programme.
        let _ = std::thread::Builder::new()
            .name("rose-icone".to_string())
            .spawn(poser_icone_console);
    }
}

/// Pose l'icône Rose sur la fenêtre de console, sans jamais bloquer.
#[cfg(windows)]
fn poser_icone_console() {
    use std::ffi::c_void;
    unsafe extern "system" {
        fn GetConsoleWindow() -> isize;
        fn PostMessageW(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize;
        fn GetModuleHandleW(nom: *const u16) -> isize;
        fn GetSystemMetrics(i: i32) -> i32;
        fn LoadImageW(inst: isize, nom: *const u16, type_: u32,
                      cx: i32, cy: i32, drapeaux: u32) -> *mut c_void;
    }
    const WM_SETICON: u32 = 0x0080;
    const ICON_BIG: usize = 1;
    const ICON_SMALL: usize = 0;
    const RT_GROUP_ICON: u32 = 14;
    const LR_DEFAULTSIZE: u32 = 0x0040;
    const LR_SHARED: u32 = 0x8000;
    const SM_CXSMICON: i32 = 49;
    const SM_CYSMICON: i32 = 50;

    unsafe {
        let hfenetre = GetConsoleWindow();
        if hfenetre == 0 {
            return;
        }
        let inst = GetModuleHandleW(std::ptr::null());
        // MAKEINTRESOURCE(1) : l'identifiant 1 comme pointeur.
        let grande = LoadImageW(inst, 1 as *const u16, RT_GROUP_ICON,
                                0, 0, LR_DEFAULTSIZE | LR_SHARED);
        if !grande.is_null() {
            PostMessageW(hfenetre, WM_SETICON, ICON_BIG, grande as isize);
        }
        let petite = LoadImageW(inst, 1 as *const u16, RT_GROUP_ICON,
                                GetSystemMetrics(SM_CXSMICON),
                                GetSystemMetrics(SM_CYSMICON),
                                LR_DEFAULTSIZE | LR_SHARED);
        if !petite.is_null() {
            PostMessageW(hfenetre, WM_SETICON, ICON_SMALL, petite as isize);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rose_no_pause_desactive_l_attente() {
        // `ROSE_NO_PAUSE=1` doit rendre la console NON éphémère : c'est le
        // moyen de ne jamais bloquer (scripts, CI).
        unsafe { std::env::set_var("ROSE_NO_PAUSE", "1") };
        let ephemere = console_ephemere();
        unsafe { std::env::remove_var("ROSE_NO_PAUSE") };
        assert!(!ephemere, "ROSE_NO_PAUSE doit désactiver l'attente");
    }
}
