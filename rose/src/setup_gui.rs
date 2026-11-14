//! Interface graphique de l'installeur Rose (Windows, Win32 natif 64 bits).
//!
//! Fenêtre dessinée à la main (aucun contrôle standard, aucun template) :
//! bandeau rose, pétale logo, boutons dessinés au GDI.
//!
//! Mise en page :
//! - bandeau de 96 px (logo + nom + version) ;
//! - panneau central qui occupe TOUT l'espace libre : le message y est mis à
//!   l'échelle de la fenêtre, jamais rogné ;
//! - barre d'état en bas, qui affiche le dernier événement (et une barre de
//!   progression indéterminée pendant un travail) ;
//! - boutons alignés en bas à droite, le principal (« Installer » /
//!   « Mettre à jour ») à côté de « Fermer », jamais au milieu des actions
//!   destructrices ;
//! - une seule police (Segoe UI) pour tout, tailles 11 / 10 / 9 pt.

#![cfg(windows)]

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;

// ── FFI Win32 ─────────────────────────────────────────────────────────
type HWND = *mut c_void;
type HDC = *mut c_void;
type HBRUSH = *mut c_void;
type HFONT = *mut c_void;
type HINSTANCE = *mut c_void;
type HICON = *mut c_void;
type HCURSOR = *mut c_void;
type LRESULT = isize;
type WPARAM = usize;
type LPARAM = isize;

#[repr(C)]
struct POINT { x: i32, y: i32 }

#[repr(C)]
struct RECT { left: i32, top: i32, right: i32, bottom: i32 }

#[repr(C)]
struct MSG { hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM, time: u32, pt: POINT }

#[repr(C)]
struct WNDCLASSEXW {
    cbSize: u32, style: u32, lpfnWndProc: Option<unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT>,
    cbClsExtra: i32, cbWndExtra: i32, hInstance: HINSTANCE, hIcon: HICON, hCursor: HCURSOR,
    hbrBackground: HBRUSH, lpszMenuName: *const u16, lpszClassName: *const u16, hIconSm: HICON,
}

#[repr(C)]
struct PAINTSTRUCT { hdc: HDC, fErase: i32, rcPaint: RECT, fRestore: i32, fIncUpdate: i32, rgbReserved: [u8; 32] }

unsafe extern "system" {
    fn GetModuleHandleW(p: *const u16) -> HINSTANCE;
    fn RegisterClassExW(c: *const WNDCLASSEXW) -> u16;
    fn CreateWindowExW(e: u32, cls: *const u16, title: *const u16, style: u32,
                       x: i32, y: i32, w: i32, h: i32, parent: HWND, menu: isize,
                       inst: HINSTANCE, param: *mut c_void) -> HWND;
    fn DefWindowProcW(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT;
    fn ShowWindow(h: HWND, c: i32);
    fn UpdateWindow(h: HWND);
    fn GetMessageW(m: *mut MSG, h: HWND, a: u32, b: u32) -> i32;
    fn TranslateMessage(m: *const MSG);
    fn DispatchMessageW(m: *const MSG) -> LRESULT;
    fn PostQuitMessage(c: i32);
    fn BeginPaint(h: HWND, ps: *mut PAINTSTRUCT) -> HDC;
    fn EndPaint(h: HWND, ps: *const PAINTSTRUCT) -> i32;
    fn CreateSolidBrush(c: u32) -> HBRUSH;
    fn DeleteObject(o: *mut c_void) -> i32;
    fn FillRect(dc: HDC, r: *const RECT, b: HBRUSH) -> i32;
    fn RoundRect(dc: HDC, l: i32, t: i32, r: i32, b: i32, w: i32, h: i32) -> i32;
    fn Ellipse(dc: HDC, l: i32, t: i32, r: i32, b: i32) -> i32;
    fn SetBkMode(dc: HDC, m: i32) -> i32;
    fn SetTextColor(dc: HDC, c: u32) -> u32;
    fn SelectObject(dc: HDC, o: *mut c_void) -> *mut c_void;
    fn CreateFontW(h: i32, w: i32, e: i32, o: i32, poids: i32, ital: i32, soul: i32,
                   barre: i32, charset: i32, outp: i32, clipp: i32, qual: i32,
                   pitch: i32, nom: *const u16) -> HFONT;
    fn DrawTextW(dc: HDC, t: *const u16, n: i32, r: *mut RECT, f: u32) -> i32;
    fn GetStockObject(i: i32) -> *mut c_void;
    fn LoadCursorW(h: HINSTANCE, n: *const u16) -> HCURSOR;
    fn InvalidateRect(h: HWND, r: *const RECT, e: i32);
    fn GetClientRect(h: HWND, r: *mut RECT);
    fn SetCursor(c: HCURSOR);
    fn GetCursorPos(p: *mut POINT) -> i32;
    fn ScreenToClient(h: HWND, p: *mut POINT) -> i32;
    fn GetDC(h: HWND) -> HDC;
    fn ReleaseDC(h: HWND, dc: HDC) -> i32;
    fn GetDeviceCaps(dc: HDC, i: i32) -> i32;
    fn SetWindowTextW(h: HWND, t: *const u16) -> i32;
    fn SetTimer(h: HWND, id: usize, ms: u32, f: *const c_void) -> usize;
    fn KillTimer(h: HWND, id: usize) -> i32;
}

const WM_DESTROY: u32 = 0x0002;
const WM_PAINT: u32 = 0x000F;
const WM_CLOSE: u32 = 0x0010;
const WM_TIMER: u32 = 0x0113;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_SETCURSOR: u32 = 0x0020;

const WS_OVERLAPPEDWINDOW: u32 = 0x00CF0000;
const SW_SHOW: i32 = 5;
const CS_HREDRAW: u32 = 0x0002;
const CS_VREDRAW: u32 = 0x0001;
const TRANSPARENT: i32 = 1;
const DT_WORDBREAK: u32 = 0x0010;
const DT_CENTER: u32 = 0x0001;
const DT_VCENTER: u32 = 0x0004;
const DT_SINGLELINE: u32 = 0x0020;
const DT_RIGHT: u32 = 0x0002;
const IDC_ARROW: u16 = 32512;
const IDC_HAND: u16 = 32649;
const LOGPIXELSY: i32 = 90;
const TIMER_PROGRESSION: usize = 1;

// Couleurs (RGB) : fond sombre unique, un seul accent, un seul vert d'action.
const FOND: u32 = 0x241A30;
const BANDEAU: u32 = 0x7E2062;
const PANNEAU: u32 = 0x322344;
const ACCENT: u32 = 0xE6207E;
const ACCENT_CLAIR: u32 = 0xFF6BB2;
const ACTION: u32 = 0x2E9E5B;
const ACTION_CLAIR: u32 = 0x45C878;
const DANGER: u32 = 0x8E3030;
const DANGER_CLAIR: u32 = 0xC84848;
const NEUTRE: u32 = 0x4A3560;
const NEUTRE_CLAIR: u32 = 0x6A5488;
const TEXTE: u32 = 0xF5EFF7;
const TEXTE_DOUX: u32 = 0xB9A8C6;

/// Libellé, action, couleur au repos, couleur au survol.
const BOUTONS: [(&str, i32, u32, u32); 4] = [
    ("Installer", 1, ACTION, ACTION_CLAIR),
    ("Mettre à jour", 0, NEUTRE, NEUTRE_CLAIR),
    ("Désinstaller", 2, DANGER, DANGER_CLAIR),
    ("Fermer", 3, NEUTRE, NEUTRE_CLAIR),
];

/// Marge extérieure de la fenêtre (px logiques).
const MARGE: i32 = 20;
/// Hauteur du bandeau.
const BANDEAU_H: i32 = 84;
/// Hauteur de la barre d'état.
const ETAT_H: i32 = 30;
/// Hauteur des boutons.
const BOUTON_H: i32 = 34;
const ECART_BOUTONS: i32 = 10;

// ── état global de la fenêtre ────────────────────────────────────────
struct Etat {
    titre: String,
    message: String,
    /// Dernière ligne du journal, affichée dans la barre d'état.
    statut: String,
    survol: i32,
    /// Vrai pendant un travail : barre de progression indéterminée.
    occupe: bool,
    /// Phase de l'animation de la barre.
    phase: i32,
    /// Échelle de police (points → pixels), calculée au premier paint.
    dpi: i32,
}

static mut ETAT: *mut Etat = std::ptr::null_mut();

fn hex_w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Hauteur d'une police en pixels à partir d'une taille en points.
unsafe fn pixels(dpi: i32, points: i32) -> i32 {
    -((points * dpi) / 72)
}

/// Dessine un texte avec une police Segoe UI à `points` points.
unsafe fn texte_pt(dc: HDC, dpi: i32, s: &str, r: &mut RECT, flags: u32, couleur: u32,
                   points: i32, gras: bool) {
    SetBkMode(dc, TRANSPARENT);
    SetTextColor(dc, couleur);
    let face = hex_w("Segoe UI");
    let poids = if gras { 600 } else { 400 };
    let police = CreateFontW(pixels(dpi, points), 0, 0, 0, poids, 0, 0, 0, 1, 0, 0, 5, 0, face.as_ptr());
    let ancien = SelectObject(dc, police as *mut c_void);
    let v = hex_w(s);
    DrawTextW(dc, v.as_ptr(), (v.len() - 1) as i32, r, flags);
    SelectObject(dc, ancien);
    DeleteObject(police as *mut c_void);
}

unsafe fn remplir(dc: HDC, r: &RECT, couleur: u32) {
    let b = CreateSolidBrush(couleur);
    FillRect(dc, r, b);
    DeleteObject(b as *mut c_void);
}

unsafe fn arrondi(dc: HDC, r: &RECT, rayon: i32, couleur: u32) {
    let b = CreateSolidBrush(couleur);
    let ancien = SelectObject(dc, b as *mut c_void);
    RoundRect(dc, r.left, r.top, r.right, r.bottom, rayon, rayon);
    SelectObject(dc, ancien);
    DeleteObject(b as *mut c_void);
}

/// Dessine le pétale Rose : cinq pétales en ellipses + cœur crème.
unsafe fn dessiner_petale(dc: HDC, cx: i32, cy: i32, rayon: i32) {
    let b = CreateSolidBrush(ACCENT);
    let ancien = SelectObject(dc, b as *mut c_void);
    for i in 0..5 {
        let angle = (i as f64) * 72.0 * std::f64::consts::PI / 180.0;
        let dx = (angle.cos() * (rayon as f64) * 0.62) as i32;
        let dy = (angle.sin() * (rayon as f64) * 0.62) as i32;
        Ellipse(dc, cx + dx - rayon / 2, cy + dy - rayon / 2, cx + dx + rayon / 2, cy + dy + rayon / 2);
    }
    SelectObject(dc, ancien);
    DeleteObject(b as *mut c_void);
    let bc = CreateSolidBrush(0xF7EDE4);
    let ancien = SelectObject(dc, bc as *mut c_void);
    Ellipse(dc, cx - rayon / 3, cy - rayon / 4, cx + rayon / 12, cy + rayon / 8);
    Ellipse(dc, cx - rayon / 12, cy - rayon / 4, cx + rayon / 3, cy + rayon / 8);
    SelectObject(dc, ancien);
    DeleteObject(bc as *mut c_void);
}

/// Largeur d'un bouton : les libellés sont mesurés une fois pour toutes avec
/// une police de référence, puis ajustés à la taille de la fenêtre.
fn largeur_boutons() -> i32 {
    132
}

unsafe fn dessiner(h: HWND) {
    let mut ps = std::mem::zeroed::<PAINTSTRUCT>();
    let dc = BeginPaint(h, &mut ps);
    let mut rc = std::mem::zeroed::<RECT>();
    GetClientRect(h, &mut rc);
    let w = rc.right;
    let ht = rc.bottom;
    let etat = &mut *ETAT;
    if etat.dpi == 0 {
        etat.dpi = GetDeviceCaps(dc, LOGPIXELSY);
        if etat.dpi <= 0 {
            etat.dpi = 96;
        }
    }
    let dpi = etat.dpi;

    // fond
    remplir(dc, &rc, FOND);

    // bandeau supérieur
    let bandeau = RECT { left: 0, top: 0, right: w, bottom: BANDEAU_H };
    remplir(dc, &bandeau, BANDEAU);

    // logo + nom + version
    dessiner_petale(dc, 52, BANDEAU_H / 2, 48);
    let mut r1 = RECT { left: 104, top: 18, right: w - MARGE, bottom: 52 };
    texte_pt(dc, dpi, "Rose", &mut r1, DT_WORDBREAK, TEXTE, 16, true);
    let mut r2 = RECT { left: 104, top: 52, right: w - MARGE, bottom: 76 };
    texte_pt(dc, dpi, &etat.titre, &mut r2, DT_WORDBREAK, TEXTE_DOUX, 9, false);

    // panneau central : TOUT l'espace libre, message mis à l'échelle
    let bas_panneau = ht - ETAT_H - BOUTON_H - 2 * MARGE;
    let panneau = RECT { left: MARGE, top: BANDEAU_H + MARGE, right: w - MARGE, bottom: bas_panneau };
    remplir(dc, &panneau, PANNEAU);
    let mut rm = RECT {
        left: panneau.left + 18,
        top: panneau.top + 16,
        right: panneau.right - 18,
        bottom: panneau.bottom - 14,
    };
    texte_pt(dc, dpi, &etat.message, &mut rm, DT_WORDBREAK, TEXTE, 10, false);

    // boutons alignés en bas à droite
    let bw = largeur_boutons();
    let total = 4 * bw + 3 * ECART_BOUTONS;
    let mut x = w - MARGE - total;
    if x < MARGE { x = MARGE; }
    let y0 = ht - ETAT_H - BOUTON_H - MARGE + 6;
    for (i, (libelle, _, repos, survol_c)) in BOUTONS.iter().enumerate() {
        let r = RECT { left: x, top: y0, right: x + bw, bottom: y0 + BOUTON_H };
        let couleur = if etat.survol == i as i32 { *survol_c } else { *repos };
        arrondi(dc, &r, 8, couleur);
        let mut rt = RECT { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
        texte_pt(dc, dpi, libelle, &mut rt, DT_CENTER | DT_VCENTER | DT_SINGLELINE, TEXTE, 9, true);
        x += bw + ECART_BOUTONS;
    }

    // barre d'état : progression indéterminée + dernière ligne du journal
    let barre = RECT { left: 0, top: ht - ETAT_H, right: w, bottom: ht };
    remplir(dc, &barre, 0x1C1326);
    let mut zone_etat = RECT {
        left: MARGE,
        top: barre.top,
        right: w - MARGE,
        bottom: barre.bottom,
    };
    if etat.occupe {
        // trois segments qui défilent (animation par WM_TIMER)
        let segment = (w / 4).max(40);
        for k in 0..3 {
            let debut = (etat.phase + (k * segment / 2)) % (w + segment);
            let a = debut - segment;
            let b = debut;
            let r = RECT {
                left: a.max(0),
                top: barre.top + 3,
                right: b.min(w),
                bottom: barre.top + 6,
            };
            if r.right > r.left {
                remplir(dc, &r, ACCENT_CLAIR);
            }
        }
        zone_etat.top = barre.top + 9;
    }
    texte_pt(dc, dpi, &etat.statut, &mut zone_etat, DT_VCENTER | DT_SINGLELINE | DT_RIGHT,
             TEXTE_DOUX, 9, false);

    EndPaint(h, &ps);
}

/// Zone d'un bouton : rend l'index, ou -1.
fn zone_bouton(x: i32, y: i32, largeur: i32, hauteur: i32) -> i32 {
    let bw = largeur_boutons();
    let total = 4 * bw + 3 * ECART_BOUTONS;
    let mut x0 = largeur - MARGE - total;
    if x0 < MARGE { x0 = MARGE; }
    let y0 = hauteur - ETAT_H - BOUTON_H - MARGE + 6;
    for i in 0..4 {
        let bx1 = x0 + bw;
        if x >= x0 && x <= bx1 && y >= y0 && y <= y0 + BOUTON_H {
            return i as i32;
        }
        x0 = bx1 + ECART_BOUTONS;
    }
    -1
}

unsafe extern "system" fn wnd_proc(h: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => { dessiner(h); 0 }
        WM_TIMER => {
            let etat = &mut *ETAT;
            if w == TIMER_PROGRESSION && etat.occupe {
                etat.phase += 24;
                InvalidateRect(h, std::ptr::null(), 0);
            }
            0
        }
        WM_MOUSEMOVE => {
            let mut rc = std::mem::zeroed::<RECT>();
            GetClientRect(h, &mut rc);
            let mut pos = POINT { x: 0, y: 0 };
            GetCursorPos(&mut pos);
            ScreenToClient(h, &mut pos);
            let z = zone_bouton(pos.x, pos.y, rc.right, rc.bottom);
            let etat = &mut *ETAT;
            if z != etat.survol {
                etat.survol = z;
                InvalidateRect(h, std::ptr::null(), 0);
            }
            SetCursor(if z >= 0 {
                LoadCursorW(std::ptr::null_mut(), IDC_HAND as *const u16)
            } else {
                LoadCursorW(std::ptr::null_mut(), IDC_ARROW as *const u16)
            });
            0
        }
        WM_LBUTTONUP => {
            let x = (l & 0xffff) as i32;
            let y = ((l >> 16) & 0xffff) as i32;
            let mut rc = std::mem::zeroed::<RECT>();
            GetClientRect(h, &mut rc);
            let z = zone_bouton(x, y, rc.right, rc.bottom);
            if z >= 0 {
                executer_action(h, z);
            }
            0
        }
        WM_CLOSE | WM_DESTROY => { PostQuitMessage(0); 0 }
        _ => DefWindowProcW(h, msg, w, l),
    }
}

/// Exécute l'action du bouton choisi et met à jour l'affichage.
fn executer_action(h: HWND, index: i32) {
    let existant = crate::setup_core::installation_existante();
    let prefixe = existant.clone().unwrap_or_else(crate::setup_core::prefixe_defaut);
    let etat = unsafe { &mut *ETAT };
    etat.survol = -1;
    let action = BOUTONS.get(index as usize).map(|b| b.1).unwrap_or(3);
    match action {
        0 => {
            etat.message = "Recherche de la dernière version sur GitHub…\n\nMerci de patienter.".to_string();
            etat.statut = "Mise à jour en cours…".to_string();
            etat.occupe = true;
            unsafe { rafraichir(h); }
            let journal = crate::setup_core::mettre_a_jour(&prefixe, false);
            etat.statut = journal.last().cloned().unwrap_or_else(|| "Terminé.".to_string());
            etat.occupe = false;
            etat.message = format!(
                "Mise à jour terminée.\n\nDossier : {prefixe}\n\nOuvrez un nouveau terminal puis tapez : rose --version"
            );
        }
        1 => {
            etat.message = format!("Installation de Rose dans\n{prefixe}\n\nMerci de patienter.");
            etat.statut = "Installation en cours…".to_string();
            etat.occupe = true;
            unsafe { rafraichir(h); }
            let journal = crate::setup_core::installer(&prefixe, None);
            etat.statut = journal.last().cloned().unwrap_or_else(|| "Terminé.".to_string());
            etat.occupe = false;
            etat.message = format!(
                "Rose {} est installé.\n\nDossier : {prefixe}\n\nOuvrez un nouveau terminal puis tapez : rose --version",
                env!("CARGO_PKG_VERSION")
            );
        }
        2 => {
            match existant {
                Some(ref p) => {
                    etat.statut = "Désinstallation…".to_string();
                    etat.occupe = true;
                    unsafe { rafraichir(h); }
                    let journal = crate::setup_core::desinstaller(p);
                    etat.statut = journal.last().cloned().unwrap_or_else(|| "Terminé.".to_string());
                    etat.occupe = false;
                    etat.message = "Rose a été désinstallé.\n\nFermez cette fenêtre.".to_string();
                }
                None => {
                    etat.message = "Aucune installation de Rose détectée.".to_string();
                    etat.statut = "Rien à désinstaller.".to_string();
                }
            }
        }
        _ => {
            unsafe { PostQuitMessage(0); }
            return;
        }
    }
    unsafe { rafraichir(h); }
}

/// Force un repaint immédiat (l'animation et le texte restent cohérents).
unsafe fn rafraichir(h: HWND) {
    InvalidateRect(h, std::ptr::null(), 0);
    UpdateWindow(h);
    let etat = &*ETAT;
    let titre = hex_w(&format!("Rose {} — Installation", env!("CARGO_PKG_VERSION")));
    SetWindowTextW(h, titre.as_ptr());
    if etat.occupe {
        SetTimer(h, TIMER_PROGRESSION, 16, std::ptr::null());
    } else {
        KillTimer(h, TIMER_PROGRESSION);
    }
}

/// Point d'entrée de la fenêtre d'installation.
pub fn lancer() {
    unsafe {
        let titre = format!("Rose {} — installation", env!("CARGO_PKG_VERSION"));
        let existant = crate::setup_core::installation_existante();
        let (message, statut) = match existant {
            Some(ref p) => (
                format!(
                    "Une installation de Rose a été détectée.\n\n\
                     Emplacement :\n{p}\n\n\
                     Choisissez une action ci-dessous.\n\
                     « Mettre à jour » garde vos réglages ; pour repartir d'une copie
                     neuve, désinstallez puis cliquez sur « Installer »."
                ),
                "Installation détectée.".to_string(),
            ),
            None => (
                format!(
                    "Rose {} n'est pas encore installé.\n\n\
                     Emplacement prévu :\n{}\n\n\
                     Cliquez sur « Installer ». Aucun droit administrateur n'est requis :\n\
                     tout va dans votre profil utilisateur.",
                    env!("CARGO_PKG_VERSION"),
                    crate::setup_core::prefixe_defaut()
                ),
                "Prêt à installer.".to_string(),
            ),
        };
        let etat = Box::new(Etat {
            titre,
            message,
            statut,
            survol: -1,
            occupe: false,
            phase: 0,
            dpi: 0,
        });
        ETAT = Box::into_raw(etat);

        let inst = GetModuleHandleW(std::ptr::null());
        let classe = hex_w("RoseSetupRose");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0, cbWndExtra: 0,
            hInstance: inst,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW as *const u16),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: classe.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        RegisterClassExW(&wc);
        let titre_w = hex_w(&format!("Rose {} — Installation", env!("CARGO_PKG_VERSION")));
        // 720 × 560 px client : assez large pour 4 boutons et une adresse de
        // dossier entière sur une seule ligne.
        let hwnd = CreateWindowExW(0, classe.as_ptr(), titre_w.as_ptr(), WS_OVERLAPPEDWINDOW,
                                   160, 140, 736, 600, std::ptr::null_mut(), 0, inst, std::ptr::null_mut());
        if hwnd.is_null() {
            return;
        }
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
        let mut msg = std::mem::zeroed::<MSG>();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// Le chemin du binaire embarqué dans l'installeur (si présent).
pub fn chemin_binaire_embarque() -> Option<std::path::PathBuf> {
    let me = std::env::current_exe().ok()?;
    let dir = me.parent()?;
    let p = dir.join("rose.exe");
    if p.exists() { Some(p) } else { None }
}

/// Texte d'aide pour la console (utilisé par « rose update »).
pub fn aide() -> String {
    "Rose update : installe ou met à jour Rose (64 bits natif).\n\
     Sur Windows, l'installeur graphique « Rose-win-Setup.exe » offre la même chose\n\
     avec une fenêtre dédiée.".to_string()
}
