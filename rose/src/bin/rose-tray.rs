//! Helper ARRIÈRE-PLAN Rose — icône dans la zone des icônes cachées de Windows.
//!
//! Ce binaire est VOLONTAIREMENT minuscule : il n'ouvre aucune fenêtre visible,
//! ne charge pas le moteur Rose et ne dépend d'aucun crate externe. Sur Windows
//! il crée une fenêtre « message-only » (invisible), pose UNE icône dans le tray
//! via `Shell_NotifyIconW`, pose UN timer à 6 h, puis dort dans `GetMessageW`.
//! Chaque tick — et UNE lecture au démarrage — lance UN thread qui fait UNE
//! seule lecture du manifeste du SITE Rose (cache TTL 300 s, timeout 3 s) via
//! `rose::velopack::verification_discrete()`, puis meurt. Aucun polling, aucune
//! boucle active, aucun service, aucune tâche planifiée, aucun lien avec GitHub.
//!
//! L'icône disparaît et le processus sort sur « Désactiver le helper » (le mode
//! `desactive` est écrit dans la config utilisateur, relue au prochain départ,
//! ET l'entrée `shell:startup` est retirée) ou sur « Quitter » (la config reste
//! intacte). `rose-tray --disable` fait la même chose sans clic.
//!
//! L'exe porte SON ICONE dans ses ressources PE (`rose-tray.rc` → RT_ICON +
//! RT_GROUP_ICON), chargée en premier par `LoadIconW` avant les `.ico`
//! externes. Voir `docs/TRAY3.md`.
//!
//! Sous Linux/macOS : no-op explicite, aucun tray n'existe. Voir `docs/TRAY.md`.
//!
//! Licence MIT.

// Pas de console noire derrière l'icône : c'est un programme de fond.
#![cfg_attr(windows, windows_subsystem = "windows")]
// Sur Linux, le module Win32 n'existe pas : les constantes ne servent qu'aux
// tests (le no-op n'a ni tray ni menu).
#![cfg_attr(not(windows), allow(dead_code, unused_imports))]

/// Les trois entrées du menu contextuel, dans l'ordre affiché.
/// Le choix de désactivation est le deuxième : il ÉCRIT la config puis retire
/// l'icône dans la foulée.
#[cfg(any(windows, test))]
const LIBELLES_MENU: [&str; 3] = ["Vérifier maintenant", "Désactiver le helper", "Quitter"];

/// Texte exact du ballon, sans fenêtre intrusive.
#[cfg(any(windows, test))]
fn texte_notification(version: &str) -> String {
    format!("Nouvelle version Rose disponible : {version}")
}

/// Le helper démarre pour les DEUX modes automatiques : « À l'ouverture »
/// (banderole L5 au lancement du launcher) et « Automatique (discret) »
/// (timer 6 h). Seuls « Désactivée » et le mode historique « Manuel » ne
/// posent ni icône, ni timer, ni requête.
///
/// ⚠ Le timer 6 h et la lecture immédiate restent RÉSERVÉS au mode discret
/// (`doit_demarrer_timer_discret`) : en mode « À l'ouverture », l'icône peut
/// apparaître (et offrir le menu), mais aucun timer n'est posé et aucune
/// requête n'est faite au lancement — c'est le launcher qui a déjà lu le
/// manifeste pour la banderole. Voir `docs/TRAY4.md`.
#[cfg(any(windows, test))]
fn doit_demarrer(mode: rose::velopack::ModeMiseAJour) -> bool {
    mode.doit_lancer_helper()
}

/// L'icône Rose est cherchée dans cet ORDRE exact : à côté de l'exe
/// (`CurrentExe`, donc le vrai dossier de `rose-tray.exe`), puis `bin\`
/// de ce dossier, puis le répertoire COURANT (où l'utilisateur a lancé
/// le setup), puis `bin\` du répertoire courant. En dernier recours,
/// `IDI_APPLICATION` garantit un `HICON` non nul à `Shell_NotifyIconW`.
/// Fonction PURE, déclarée HORS du module Win32 : testable sur Linux.
#[cfg(any(windows, test))]
fn candidats_icone(exe: Option<std::path::PathBuf>,
                   cwd: Option<std::path::PathBuf>) -> Vec<std::path::PathBuf> {
    let mut dossiers: Vec<std::path::PathBuf> = Vec::new();
    if let Some(p) = exe.and_then(|p| p.parent().map(|p| p.to_path_buf())) {
        dossiers.push(p);
    }
    if let Some(p) = cwd { dossiers.push(p); }
    let mut sortie: Vec<std::path::PathBuf> = Vec::new();
    for dossier in dossiers {
        for relatif in ["rose.ico", "bin/rose.ico"] {
            let candidat = dossier.join(relatif);
            if !sortie.contains(&candidat) { sortie.push(candidat); }
        }
    }
    sortie
}

#[cfg(not(windows))]
fn main() {
    if std::env::args().skip(1).any(|a| a == "--disable") {
        println!("rose-tray : aucun dossier shell:startup hors Windows — rien à désactiver.");
        return;
    }
    println!(
        "rose-tray : la zone des icônes cachées n'existe que sur Windows. \
         Aucun timer, aucune requête réseau, aucun processus lancé (mode no-op)."
    );
}

#[cfg(windows)]
mod win {
    #![allow(unsafe_op_in_unsafe_fn)] // fonctions Win32 : l'appelant garantit le contrat

    use super::*;
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, Ordering};

    type HWND = *mut c_void;
    type HICON = *mut c_void;
    type HMENU = *mut c_void;
    type HINSTANCE = *mut c_void;
    type LRESULT = isize;
    type WPARAM = usize;
    type LPARAM = isize;

    #[repr(C)]
    struct Point { x: i32, y: i32 }

    #[repr(C)]
    struct Msg {
        hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM, time: u32, pt: Point,
    }

    #[repr(C)]
    struct WndClassExW {
        cb_size: u32,
        style: u32,
        lpfn_wnd_proc: Option<unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT>,
        cb_cls_extra: i32,
        cb_wnd_extra: i32,
        h_instance: HINSTANCE,
        h_icon: HICON,
        h_cursor: *mut c_void,
        hbr_background: *mut c_void,
        lpsz_menu_name: *const u16,
        lpsz_class_name: *const u16,
        h_icon_sm: HICON,
    }

    /// NOTIFYICONDATAW (V3, 952 octets sur x64) : le ballon NIF_INFO est inclus.
    #[repr(C)]
    struct NotifyIconDataW {
        cb_size: u32,
        hwnd: HWND,
        u_id: u32,
        u_flags: u32,
        u_callback_message: u32,
        h_icon: HICON,
        sz_tip: [u16; 128],
        dw_state: u32,
        dw_state_mask: u32,
        sz_info: [u16; 256],
        u_timeout: u32,
        sz_info_title: [u16; 64],
        dw_info_flags: u32,
    }

    // Messages privés : le retour du tick ne doit PAS être confondu avec le
    // message que shell32 envoie pour les clics sur l'icône.
    const WM_APP_TICK: u32 = 0x8000 + 1;
    const WM_TRAY_CLIC: u32 = 0x8000 + 2;
    const WM_RBUTTONUP: u32 = 0x0205;
    const WM_COMMAND: u32 = 0x0111;
    const WM_TIMER: u32 = 0x0113;
    const WM_DESTROY: u32 = 0x0002;
    const WM_NULL: u32 = 0x0000;

    const HWND_MESSAGE: HWND = -3isize as HWND;
    const ID_ICONE: u32 = 1;
    const ID_TIMER: usize = 1;
    const ID_VERIFIER: usize = 1;
    const ID_DESACTIVER: usize = 2;
    const ID_QUITTER: usize = 3;

    const NIM_ADD: u32 = 0;
    const NIM_MODIFY: u32 = 1;
    const NIM_DELETE: u32 = 2;
    const NIF_MESSAGE: u32 = 0x0000_0001;
    const NIF_ICON: u32 = 0x0000_0002;
    const NIF_TIP: u32 = 0x0000_0004;
    const NIF_INFO: u32 = 0x0000_0010;
    const NIIF_INFO: u32 = 0x0000_0001;
    const NIIF_NOSOUND: u32 = 0x0000_0010;

    const MF_STRING: u32 = 0x0000_0000;
    const TPM_RIGHTBUTTON: u32 = 0x0000_0002;
    const IMAGE_ICON: u32 = 1;
    const LR_LOADFROMFILE: u32 = 0x0000_0008;
    const LR_DEFAULTSIZE: u32 = 0x0000_0040;
    const LR_SHARED: u32 = 0x0000_8000;
    const IDI_APPLICATION: usize = 32512;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn RegisterClassExW(c: *const WndClassExW) -> u16;
        fn CreateWindowExW(style_ex: u32, classe: *const u16, titre: *const u16, style: u32,
                           x: i32, y: i32, w: i32, h: i32, parent: HWND, menu: isize,
                           instance: HINSTANCE, param: *mut c_void) -> HWND;
        fn DefWindowProcW(h: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT;
        fn GetMessageW(msg: *mut Msg, h: HWND, min: u32, max: u32) -> i32;
        fn TranslateMessage(msg: *const Msg);
        fn DispatchMessageW(msg: *const Msg) -> LRESULT;
        fn PostQuitMessage(code: i32);
        fn PostMessageW(h: HWND, msg: u32, w: WPARAM, l: LPARAM) -> i32;
        fn GetModuleHandleW(nom: *const u16) -> HINSTANCE;
        fn LoadIconW(instance: HINSTANCE, nom: *const u16) -> HICON;
        fn LoadImageW(instance: HINSTANCE, nom: *const u16, type_: u32,
                      cx: i32, cy: i32, options: u32) -> *mut c_void;
        fn DestroyWindow(h: HWND) -> i32;
        fn SetTimer(h: HWND, id: usize, ms: u32, proc: *const c_void) -> usize;
        fn KillTimer(h: HWND, id: usize) -> i32;
        fn SetForegroundWindow(h: HWND) -> i32;
        fn GetCursorPos(pt: *mut Point) -> i32;
        fn CreatePopupMenu() -> HMENU;
        fn AppendMenuW(menu: HMENU, drapeaux: u32, id: usize, texte: *const u16) -> i32;
        fn TrackPopupMenu(menu: HMENU, drapeaux: u32, x: i32, y: i32, reserve: i32,
                          h: HWND, rect: *const c_void) -> i32;
        fn DestroyMenu(menu: HMENU) -> i32;
    }

    #[link(name = "shell32")]
    unsafe extern "system" {
        fn Shell_NotifyIconW(action: u32, donnees: *const NotifyIconDataW) -> i32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLastError() -> u32;
    }

    /// Journal SANS dépendance : `%TEMP%\rose-tray.log`, en ajout. Le helper
    /// n'a pas de console (`windows_subsystem = "windows"`), donc sans ce
    /// fichier un échec `Shell_NotifyIconW` était TOTALEMENT silencieux —
    /// c'est précisément pour cela que l'icône pouvait manquer sans trace.
    /// Un échec d'écriture du journal ne doit JAMAIS empêcher le tray.
    fn journal(evenement: &str) {
        let chemin = std::env::temp_dir().join("rose-tray.log");
        let Ok(mut fichier) = std::fs::OpenOptions::new()
            .create(true).append(true).open(chemin) else { return };
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        use std::io::Write;
        let _ = writeln!(fichier, "{secs} — {evenement}");
    }

    fn utf16(texte: &str) -> Vec<u16> {
        texte.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Copie dans un champ fixe de `NOTIFYICONDATAW` sans jamais déborder.
    fn copier(champ: &mut [u16], texte: &str) {
        let limite = champ.len().saturating_sub(1);
        for (case, unite) in champ.iter_mut().zip(texte.encode_utf16().take(limite)) {
            *case = unite;
        }
    }

    /// Charge l'icône avec `LoadImageW(..., LR_SHARED)` ; chaque chemin testé
    /// est journalisé. « rose.ico absent » était l'une des causes possibles
    /// de l'absence d'icône sur la capture utilisateur — désormais prouvée.
    fn charger_icone() -> HICON {
        unsafe {
            // ── 1. ICÔNE EMBARQUÉE DANS LE PE (rose-tray.rc → .res) ────────
            // `rose-tray.exe` porte RT_GROUP_ICON 1 (LA ROSE). C'est la
            // source FIRST : elle ne dépend d'aucun fichier externe, donc
            // l'icône du tray reste la nôtre même si `rose.ico` n'a pas été
            // copié dans `bin\` (cause de la capture utilisateur). Le repli
            // sur les fichiers ci-dessous ne sert qu'aux anciens exe sans
            // ressources.
            let embedded = LoadIconW(GetModuleHandleW(std::ptr::null()),
                                     ID_ICONE as *const u16);
            if !embedded.is_null() {
                journal("icône : RT_GROUP_ICON 1 embarquée dans le PE (rose-tray.rc) chargée");
                return embedded;
            }
            journal("icône : aucune ressource PE exploitable — repli sur les fichiers .ico");

            // ── 2. FICHIERS .ICO À CÔTÉ DE L'EXE (anciens builds) ─────────
            let candidats = candidats_icone(std::env::current_exe().ok(),
                                            std::env::current_dir().ok());
            journal(&format!("icône : {} chemin(s) à tester", candidats.len()));
            for chemin in candidats {
                if !chemin.is_file() {
                    journal(&format!("icône absente : {}", chemin.display()));
                    continue;
                }
                let nom = utf16(&chemin.to_string_lossy());
                // 16×16 = icône de la zone de notification (petite icône).
                // LR_SHARED évite tout DestroyIcon et garantit la durée de vie
                // du HICON pendant toute la boucle GetMessageW.
                let h = LoadImageW(std::ptr::null_mut(), nom.as_ptr(), IMAGE_ICON, 16, 16,
                                   LR_LOADFROMFILE | LR_SHARED);
                let h = if h.is_null() {
                    LoadImageW(std::ptr::null_mut(), nom.as_ptr(), IMAGE_ICON, 0, 0,
                               LR_LOADFROMFILE | LR_DEFAULTSIZE | LR_SHARED)
                } else { h };
                if !h.is_null() {
                    journal(&format!("icône chargée depuis {} (LR_SHARED, GetLastError={})",
                                     chemin.display(), GetLastError()));
                    return h as HICON;
                }
                journal(&format!("LoadImageW a échoué pour {} (GetLastError={})",
                                 chemin.display(), GetLastError()));
            }
            // Repli obligatoire : une icône système vaut mieux qu'un NIM_ADD
            // avec HICON=null, que shell32 refuse silencieusement.
            let repli = LoadIconW(std::ptr::null_mut(), IDI_APPLICATION as *const u16);
            journal(&format!("repli IDI_APPLICATION : {} (GetLastError={})",
                             if repli.is_null() { "ÉCHEC" } else { "OK" }, GetLastError()));
            repli
        }
    }

    /// Chargée UNE seule fois pour tout le processus (usize = pointeur sans
    /// contrainte `Send`/`Sync` sur un `HICON` brut).
    static ICONE: std::sync::OnceLock<usize> = std::sync::OnceLock::new();

    fn icone() -> HICON {
        *ICONE.get_or_init(|| charger_icone() as usize) as HICON
    }

    fn donnees(hwnd: HWND, drapeaux: u32) -> NotifyIconDataW {
        let mut d: NotifyIconDataW = unsafe { std::mem::zeroed() };
        d.cb_size = std::mem::size_of::<NotifyIconDataW>() as u32;
        d.hwnd = hwnd;
        d.u_id = ID_ICONE;
        d.u_flags = drapeaux;
        d.u_callback_message = WM_TRAY_CLIC;
        d
    }

    /// Un SEUL point d'appel Win32 pour ajouter, modifier ou retirer l'icône.
    /// Le retour est VÉRIFIÉ et l'échec journalisé (code GetLastError) : sans
    /// cela, l'absence d'icône dans la zone de notification était invisible.
    fn notifier(action: u32, d: &NotifyIconDataW) -> bool {
        unsafe {
            let ok = Shell_NotifyIconW(action, d) != 0;
            if !ok {
                let nom = match action {
                    NIM_ADD => "NIM_ADD",
                    NIM_MODIFY => "NIM_MODIFY",
                    NIM_DELETE => "NIM_DELETE",
                    _ => "action inconnue",
                };
                journal(&format!("Shell_NotifyIconW {nom} a échoué : GetLastError={}", GetLastError()));
            }
            ok
        }
    }

    fn ajouter_icone(hwnd: HWND) -> bool {
        let mut d = donnees(hwnd, NIF_MESSAGE | NIF_ICON | NIF_TIP);
        d.h_icon = icone();
        copier(&mut d.sz_tip, "Rose — vérification des mises à jour");
        let ok = notifier(NIM_ADD, &d);
        journal(&format!("NIM_ADD : {}", if ok { "OK — icône posée" } else { "ÉCHEC" }));
        ok
    }

    fn retirer_icone(hwnd: HWND) {
        let d = donnees(hwnd, 0);
        let _ = notifier(NIM_DELETE, &d);
    }

    /// Ballon discret (`NIF_INFO`) : pas de fenêtre, pas de son.
    fn ballon(hwnd: HWND, texte: &str) {
        let mut d = donnees(hwnd, NIF_INFO | NIF_ICON);
        d.h_icon = icone();
        copier(&mut d.sz_info, texte);
        copier(&mut d.sz_info_title, "Mise à jour Rose");
        d.dw_info_flags = NIIF_INFO | NIIF_NOSOUND;
        let _ = notifier(NIM_MODIFY, &d);
    }

    /// Un seul thread par tick : lit UNE fois le manifeste du SITE, poste le
    /// résultat à la fenêtre, puis meurt. Le drapeau interdit tout empilement
    /// si l'utilisateur clique plusieurs fois sur « Vérifier maintenant ».
    static EN_COURS: AtomicBool = AtomicBool::new(false);

    fn lancer_verification(hwnd: HWND) {
        if EN_COURS.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            return;
        }
        let cible = hwnd as isize;
        std::thread::spawn(move || {
            // `verification_discrete()` = garde-fou de mode + UNE requête au
            // site (cache frais sinon curl borné à 3 s), AUCUN repli GitHub.
            let texte = match rose::velopack::verification_discrete() {
                Ok(Some(b)) => Some(texte_notification(&b.disponible)),
                _ => None,
            };
            let boite = Box::into_raw(Box::new(texte)) as isize;
            let h = cible as HWND;
            if unsafe { PostMessageW(h, WM_APP_TICK, 0, boite) } == 0 {
                // Fenêtre déjà fermée : on libère la boîte nous-même.
                unsafe { drop(Box::from_raw(boite as *mut Option<String>)) };
            }
            EN_COURS.store(false, Ordering::SeqCst);
        });
    }

    /// Menu contextuel sur clic DROIT uniquement (le clic gauche ne fait rien,
    /// pour éviter une vérification accidentelle).
    unsafe fn afficher_menu(hwnd: HWND) {
        let menu = CreatePopupMenu();
        if menu.is_null() { return; }
        for (i, libelle) in LIBELLES_MENU.iter().enumerate() {
            let texte = utf16(libelle);
            let _ = AppendMenuW(menu, MF_STRING, i + 1, texte.as_ptr());
        }
        SetForegroundWindow(hwnd);
        let mut pt = Point { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        TrackPopupMenu(menu, TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, std::ptr::null());
        // Astuce Win32 : sans ce message, le menu peut rester « collé » au
        // clic suivant après `SetForegroundWindow`.
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);
    }

    unsafe extern "system" fn wnd_proc(h: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        match msg {
            WM_TRAY_CLIC => {
                if (l as u32) == WM_RBUTTONUP { afficher_menu(h); }
                0
            }
            WM_COMMAND => {
                match w & 0xffff {
                    ID_VERIFIER => lancer_verification(h),
                    ID_DESACTIVER => {
                        // Persistance : le prochain lancement verra `desactive`
                        // et ne posera NI icône, NI timer, NI requête.
                        // ⚠ On retire AUSSI l'entrée `shell:startup` : sinon
                        // le helper se relancerait à chaque logon (il sortirait
                        // tout seul, mais l'utilisateur verrait un .cmd clignoter).
                        desactiver();
                        retirer_icone(h);
                        PostQuitMessage(0);
                    }
                    ID_QUITTER => {
                        // Simple sortie : la config n'est PAS modifiée.
                        retirer_icone(h);
                        PostQuitMessage(0);
                    }
                    _ => {}
                }
                0
            }
            WM_TIMER if w == ID_TIMER => {
                lancer_verification(h);
                0
            }
            WM_APP_TICK => {
                let boite = l as *mut Option<String>;
                if !boite.is_null() {
                    if let Some(texte) = *Box::from_raw(boite) { ballon(h, &texte); }
                }
                0
            }
            WM_DESTROY => {
                retirer_icone(h);
                KillTimer(h, ID_TIMER);
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(h, msg, w, l),
        }
    }

    /// DÉSACTIVATION SYSTÉMIQUE — même chemin pour le menu contextuel et pour
    /// `rose-tray --disable` : écrire `desactive` dans la config ET retirer
    /// l'entrée du dossier Démarrage. Aucun droit admin (tout est sous le
    /// profil utilisateur). Chaque étape est journalisée et vérifiée.
    pub fn desactiver() {
        match rose::velopack::ecrire_config(rose::velopack::ModeMiseAJour::Desactive) {
            Ok(chemin) => journal(&format!("désactivation : config {} écrite", chemin.display())),
            Err(e) => journal(&format!("désactivation : écriture config ÉCHOUÉE — {e}")),
        }
        let retrait = rose::demarrage::desactiver();
        for ligne in &retrait.journal { journal(ligne); }
        journal(&format!("désactivation : entrée Démarrage {}",
                         if retrait.reussi { "RETIRÉE (vérifié)" } else { "NON RETIRÉE" }));
    }

    /// Point d'entrée Windows : zéro fenêtre visible, zéro thread dormant,
    /// zéro boucle active. `GetMessageW` bloque le fil jusqu'à un message
    /// (consommation CPU nulle entre les ticks).
    pub fn demarrer() {
        let mode = rose::velopack::lire_config().mode;
        journal(&format!("démarrage — mode configuré : {}", mode.comme_str()));
        if !doit_demarrer(mode) {
            journal("mode non discret : aucun tray, aucun timer (arrêt immédiat)");
            return;
        }
        unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let classe = utf16("RoseTrayMessageWindow");
            let mut wc: WndClassExW = std::mem::zeroed();
            wc.cb_size = std::mem::size_of::<WndClassExW>() as u32;
            wc.lpfn_wnd_proc = Some(wnd_proc);
            wc.h_instance = instance;
            wc.lpsz_class_name = classe.as_ptr();
            if RegisterClassExW(&wc) == 0 {
                journal(&format!("RegisterClassExW a échoué : GetLastError={}", GetLastError()));
                return;
            }

            let hwnd = CreateWindowExW(0, classe.as_ptr(), std::ptr::null(), 0,
                                       0, 0, 0, 0, HWND_MESSAGE, 0, instance, std::ptr::null_mut());
            if hwnd.is_null() {
                journal(&format!("CreateWindowExW a échoué : GetLastError={}", GetLastError()));
                return;
            }
            if !ajouter_icone(hwnd) { DestroyWindow(hwnd); return; }
            // ⚠ « À l'ouverture » garde son icône mais AUCUN timer 6 h et
            // AUCUNE requête au lancement : la lecture a déjà été faite par
            // le launcher pour la banderole (L5). Seul le mode discret pose
            // le timer et fait une lecture immédiate.
            if mode.doit_demarrer_timer_discret() {
                if SetTimer(hwnd, ID_TIMER,
                            rose::velopack::INTERVALLE_TIMER_DISCRET_MS, std::ptr::null()) == 0 {
                    journal(&format!("SetTimer a échoué : GetLastError={}", GetLastError()));
                    retirer_icone(hwnd);
                    DestroyWindow(hwnd);
                    return;
                }
                journal("tray actif : icône + timer 6 h posés, entrée dans GetMessageW");
                // UNE lecture au démarrage, puis plus rien avant 6 heures.
                lancer_verification(hwnd);
            } else {
                journal("tray actif : icône SANS timer (mode À l'ouverture), entrée dans GetMessageW");
            }

            let mut msg: Msg = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}

#[cfg(windows)]
fn main() {
    // `rose-tray --disable` : désactive le mode discret ET supprime le
    // lanceur du dossier Démarrage, sans passer par le clic sur l'icône.
    // Le helper sort tout de suite : aucun tray, aucun timer, aucun réseau.
    if std::env::args().skip(1).any(|a| a == "--disable") {
        win::desactiver();
        return;
    }
    win::demarrer();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icone_cherchee_dans_l_exe_puis_le_cwd() {
        let exe = Some(std::path::PathBuf::from("C:/Programs/Rose/rose-tray.exe"));
        let cwd = Some(std::path::PathBuf::from("C:/Downloads"));
        let c = candidats_icone(exe, cwd);
        assert_eq!(c.len(), 4);
        assert_eq!(c[0], std::path::PathBuf::from("C:/Programs/Rose/rose.ico"));
        assert_eq!(c[1], std::path::PathBuf::from("C:/Programs/Rose/bin/rose.ico"));
        assert_eq!(c[2], std::path::PathBuf::from("C:/Downloads/rose.ico"));
        assert_eq!(c[3], std::path::PathBuf::from("C:/Downloads/bin/rose.ico"));
    }

    #[test]
    fn texte_ballon_exact() {
        assert_eq!(texte_notification("0.1.72"), "Nouvelle version Rose disponible : 0.1.72");
    }

    #[test]
    fn menu_contextuel_trois_entrees() {
        assert_eq!(LIBELLES_MENU.len(), 3);
        assert_eq!(LIBELLES_MENU[0], "Vérifier maintenant");
        assert_eq!(LIBELLES_MENU[1], "Désactiver le helper");
        assert_eq!(LIBELLES_MENU[2], "Quitter");
    }

    #[test]
    fn garde_fou_de_mode() {
        use rose::velopack::ModeMiseAJour;
        // Les deux modes automatiques lancent le helper (icône éventuelle).
        assert!(!doit_demarrer(ModeMiseAJour::Desactive));
        assert!(doit_demarrer(ModeMiseAJour::Auto));
        assert!(!doit_demarrer(ModeMiseAJour::Manuel));
        assert!(doit_demarrer(ModeMiseAJour::Automatique));
    }

    #[test]
    fn timer_6h_reserve_au_mode_discret() {
        let code = code_production();
        // Le SetTimer unique est sous la garde du prédicat « discret » :
        // « À l'ouverture » ne pose AUCUN timer, mais garde son icône.
        assert!(code.contains("if mode.doit_demarrer_timer_discret() {"),
                "le timer 6 h n'est pas conditionné au mode discret");
        assert!(code.contains("icône SANS timer"));
        assert_eq!(rose::velopack::ModeMiseAJour::Auto.doit_demarrer_timer_discret(), false);
        assert_eq!(rose::velopack::ModeMiseAJour::Automatique.doit_demarrer_timer_discret(), true);
    }

    #[test]
    fn structure_un_timer_une_requete_aucun_polling() {
        let src = include_str!("rose-tray.rs");
        // On ne compte que le CODE : les motifs apparaissent aussi dans ce
        // test, ce qui doublerait les occurrences.
        let code = src.split("#[cfg(test)]").next().unwrap_or(src);
        // Un seul timer posé, un seul point d'appel Shell_NotifyIcon, un seul
        // thread lancé (le tick). Le fil principal dort dans GetMessageW.
        assert_eq!(code.matches("SetTimer(hwnd").count(), 1);
        assert_eq!(code.matches("Shell_NotifyIconW(action, d) != 0").count(), 1);
        assert_eq!(code.matches("std::thread::spawn(").count(), 1);
        // Le retour Win32 est vérifié et journalisé : aucune erreur silencieuse.
        assert!(code.contains("GetLastError()"));
        assert!(code.contains("rose-tray.log"));
        // Repli sur l'icône système si rose.ico est absent/illisible.
        assert!(code.contains("IDI_APPLICATION as *const u16"));
        assert!(!code.contains("loop {"));
        assert!(!code.contains("thread::sleep"));
        assert!(!code.contains("lister_versions_source"));
        assert_eq!(rose::velopack::INTERVALLE_TIMER_DISCRET_MS, 21_600_000);
        assert!(rose::velopack::intervalle_timer_discret_valide(
            rose::velopack::INTERVALLE_TIMER_DISCRET_MS));
    }

    /// Le code de production, tranché AVANT les tests pour ne pas compter nos
    /// propres chaînes littérales (même technique que le test structurel).
    fn code_production() -> &'static str {
        include_str!("rose-tray.rs").split("#[cfg(test)]").next().unwrap()
    }

    #[test]
    fn option_disable_sans_clic_retire_le_mode_et_le_demarrage() {
        let code = code_production();
        assert!(code.contains("a == \"--disable\""),
                "`rose-tray --disable` n'est pas reconnu");
        assert!(code.contains("win::desactiver()"),
                "--disable ne passe pas par la désactivation systémique");
        // La fonction partagée écrit le mode ET retire l'entrée shell:startup.
        let debut = code.find("pub fn desactiver()").expect("désactivation absente");
        let fonction = &code[debut..code.find("pub fn demarrer()").unwrap()];
        assert!(fonction.contains("ModeMiseAJour::Desactive"));
        assert!(fonction.contains("rose::demarrage::desactiver()"));
        assert!(fonction.contains("journal("), "désactivation silencieuse");
    }

    #[test]
    fn menu_desactiver_retire_aussi_l_entree_shell_startup() {
        let code = code_production();
        let debut = code.find("ID_DESACTIVER =>").expect("menu Désactiver absent");
        let fin = code.find("ID_QUITTER =>").unwrap();
        let bras = &code[debut..fin];
        assert!(bras.contains("desactiver()"),
                "le menu ne retire pas l'entrée Démarrage : le helper reviendrait au logon");
        assert!(bras.contains("retirer_icone(h)"));
        assert!(bras.contains("PostQuitMessage(0)"));
    }

    #[test]
    fn icone_embarquee_dans_le_pe_est_la_premiere_source() {
        let code = code_production();
        let debut = code.find("fn charger_icone()").unwrap();
        let fonction = &code[debut..code.find("static ICONE").unwrap()];
        assert!(fonction.contains("LoadIconW(GetModuleHandleW(std::ptr::null())"),
                "l'icône RT_GROUP_ICON 1 du PE n'est pas chargée en premier");
        assert!(fonction.contains("ID_ICONE as *const u16"));
        // Les fichiers externes restent en repli, mais ne sont plus la source
        // unique (c'était la cause de l'icône manquante).
        assert!(fonction.contains("LR_LOADFROMFILE"));
        assert!(fonction.contains("IDI_APPLICATION as *const u16"));
    }
}
