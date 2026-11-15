//! Spécificités Windows de la console interactive.
//!
//! Ce module règle deux problèmes mesurés par l'utilisateur :
//!
//! 1. **Fenêtre de console sans icône.** Quand on double-clique un fichier
//!    `.rose`, l'Explorateur lance `rose.exe`, ce qui ouvre une fenêtre de
//!    console. Le chargeur met l'icône de l'EXÉCUTABLE sur la fenêtre — il
//!    faut donc que `rose.exe` porte la rose de marque. Cette fonction la
//!    pose explicitement (`SetConsoleWindow` + `WM_SETICON`), ce qui marche
//!    quelle que soit la façon dont la console a été ouverte.
//!
//! 2. **La fenêtre se ferme trop vite.** Un programme Rose affiche son
//!    résultat puis se termine : la console disparaît avant qu'on ait pu le
//!    lire. On détecte que la console a été ouverte POUR NOUS (un seul
//!    processus y est rattaché) et on attend une frappe avant de rendre la
//!    main. Lancé depuis un terminal déjà ouvert, rien ne change.
//!
//! `ROSE_NO_PAUSE=1` désactive l'attente (utile en script).

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

/// Prépare la console : titre et icône de la fenêtre.
pub fn preparer() {
    #[cfg(windows)]
    {
        use std::ffi::c_void;
        unsafe extern "system" {
            fn GetConsoleWindow() -> isize;
            fn SendMessageW(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize;
            fn GetModuleHandleW(nom: *const u16) -> isize;
            fn GetSystemMetrics(i: i32) -> i32;
            fn LoadImageW(inst: isize, nom: *const u16, type_: u32,
                          cx: i32, cy: i32, drapeaux: u32) -> *mut c_void;
            fn SetConsoleTitleW(titre: *const u16) -> i32;
        }
        const WM_SETICON: u32 = 0x0080;
        const ICON_BIG: usize = 1;
        const ICON_SMALL: usize = 0;
        const RT_GROUP_ICON: u32 = 14;
        const LR_DEFAULTSIZE: u32 = 0x0040;
        const LR_SHARED: u32 = 0x8000;

        unsafe {
            // Titre de la fenêtre : « Rose 0.1.5 ».
            let titre: Vec<u16> = format!("Rose {} — console interactive", env!("CARGO_PKG_VERSION"))
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            SetConsoleTitleW(titre.as_ptr());

            // Icône de la fenêtre : le groupe embarqué dans `rose.exe`
            // (RT_GROUP_ICON id 1) = la rose de marque.
            let hfenetre = GetConsoleWindow();
            if hfenetre != 0 {
                let inst = GetModuleHandleW(std::ptr::null());
                // MAKEINTRESOURCE(1) : l'identifiant 1 comme pointeur.
                let grande = LoadImageW(inst, 1 as *const u16, RT_GROUP_ICON,
                                        0, 0, LR_DEFAULTSIZE | LR_SHARED);
                if !grande.is_null() {
                    SendMessageW(hfenetre, WM_SETICON, ICON_BIG, grande as isize);
                }
                let petite = LoadImageW(inst, 1 as *const u16, RT_GROUP_ICON,
                                        GetSystemMetrics(SM_CXSMICON),
                                        GetSystemMetrics(SM_CYSMICON),
                                        LR_DEFAULTSIZE | LR_SHARED);
                if !petite.is_null() {
                    SendMessageW(hfenetre, WM_SETICON, ICON_SMALL, petite as isize);
                }
            }
        }
    }
}

#[cfg(windows)]
const SM_CXSMICON: i32 = 49;
#[cfg(windows)]
const SM_CYSMICON: i32 = 50;

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
