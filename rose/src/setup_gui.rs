//! Interface graphique de l'installeur Rose (Windows, Win32 natif 64 bits).
//!
//! Fenêtre dessinée à la main (aucun contrôle standard, aucun template) :
//! bandeau rose, pétale logo, boutons dessinés au GDI.

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
}

const WM_DESTROY: u32 = 0x0002;
const WM_PAINT: u32 = 0x000F;
const WM_CLOSE: u32 = 0x0010;
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
const IDC_ARROW: u16 = 32512;
const IDC_HAND: u16 = 32649;

// couleurs (RGB)
const FOND: u32 = 0x241A30;
const BANDEAU: u32 = 0x7E2062;
const PANNEAU: u32 = 0x322344;
const ACCENT: u32 = 0xE6207E;
const ACCENT_CLAIR: u32 = 0xFF6BB2;
const TEXTE: u32 = 0xF5EFF7;
const TEXTE_DOUX: u32 = 0xB9A8C6;
const SURVOL: u32 = 0x4A3560;

// ── état global de la fenêtre ────────────────────────────────────────
struct Etat {
    titre: String,
    message: String,
    journal: Vec<String>,
    survol: i32,
    termine: bool,
}

static mut ETAT: *mut Etat = std::ptr::null_mut();

/// Les 4 boutons : libellé, action, position x relative.
const BOUTONS: [(&str, i32); 4] = [
    ("Mettre à jour", 0),
    ("Réinstaller", 1),
    ("Désinstaller", 2),
    ("Fermer", 3),
];

fn hex_w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn texte(dc: HDC, s: &str, r: &mut RECT, flags: u32, couleur: u32, gras: bool) {
    SetBkMode(dc, TRANSPARENT);
    SetTextColor(dc, couleur);
    let face = hex_w("Segoe UI");
    let police = if gras {
        CreateFontW(-22, 0, 0, 0, 700, 0, 0, 0, 1, 0, 0, 5, 0, face.as_ptr())
    } else {
        CreateFontW(-15, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, face.as_ptr())
    };
    let ancien = SelectObject(dc, police as *mut c_void);
    let v = hex_w(s);
    DrawTextW(dc, v.as_ptr(), (v.len() - 1) as i32, r, flags);
    SelectObject(dc, ancien);
    DeleteObject(police as *mut c_void);
}

/// Dessine le pétale Rose : un cœur + 5 pétales en ellipses inclinées.
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
    // cœur crème
    let bc = CreateSolidBrush(0xF7EDE4);
    let ancien = SelectObject(dc, bc as *mut c_void);
    Ellipse(dc, cx - rayon / 3, cy - rayon / 4, cx + rayon / 12, cy + rayon / 8);
    Ellipse(dc, cx - rayon / 12, cy - rayon / 4, cx + rayon / 3, cy + rayon / 8);
    SelectObject(dc, ancien);
    DeleteObject(bc as *mut c_void);
}

unsafe fn dessiner(h: HWND) {
    let mut ps = std::mem::zeroed::<PAINTSTRUCT>();
    let dc = BeginPaint(h, &mut ps);
    let mut rc = std::mem::zeroed::<RECT>();
    GetClientRect(h, &mut rc);
    let w = rc.right;
    let ht = rc.bottom;

    // fond
    let bf = CreateSolidBrush(FOND);
    FillRect(dc, &rc, bf);
    DeleteObject(bf as *mut c_void);

    // bandeau supérieur
    let mut bandeau = RECT { left: 0, top: 0, right: w, bottom: 96 };
    let bb = CreateSolidBrush(BANDEAU);
    FillRect(dc, &bandeau, bb);
    DeleteObject(bb as *mut c_void);

    // logo + titre
    dessiner_petale(dc, 62, 48, 54);
    let mut r1 = RECT { left: 112, top: 22, right: w - 20, bottom: 58 };
    texte(dc, "Rose", &mut r1, DT_WORDBREAK, TEXTE, true);
    let mut r2 = RECT { left: 112, top: 54, right: w - 20, bottom: 84 };
    let etat = &*ETAT;
    texte(dc, &format!("{} — installation", etat.titre), &mut r2, DT_WORDBREAK, TEXTE_DOUX, false);

    // panneau message
    let mut panneau = RECT { left: 24, top: 116, right: w - 24, bottom: 300 };
    let bp = CreateSolidBrush(PANNEAU);
    FillRect(dc, &panneau, bp);
    DeleteObject(bp as *mut c_void);
    let mut rm = RECT { left: 40, top: 132, right: w - 40, bottom: 288 };
    texte(dc, &etat.message, &mut rm, DT_WORDBREAK, TEXTE, false);

    // journal (si des opérations ont été faites)
    if !etat.journal.is_empty() {
        let mut rj = RECT { left: 40, top: 288, right: w - 40, bottom: 300 };
        let mut s = String::new();
        for l in etat.journal.iter().rev().take(3).collect::<Vec<_>>().into_iter().rev() {
            s.push_str(l);
            s.push('\n');
        }
        texte(dc, &s, &mut rj, DT_WORDBREAK, ACCENT_CLAIR, false);
    }

    // boutons dessinés à la main
    let mut x = 24i32;
    let bw = (w - 48 - 3 * 12) / 4;
    for (i, (libelle, _)) in BOUTONS.iter().enumerate() {
        let mut r = RECT { left: x, top: ht - 56, right: x + bw, bottom: ht - 18 };
        let survol = etat.survol == i as i32;
        let couleur = match i {
            0 => if survol { ACCENT_CLAIR } else { ACCENT },
            1 => if survol { 0x7A5C96 } else { 0x5A4478 },
            2 => if survol { 0xE05050 } else { 0x8E3030 },
            _ => if survol { 0x7A5C96 } else { 0x4A3560 },
        };
        let b = CreateSolidBrush(couleur);
        let ancien = SelectObject(dc, b as *mut c_void);
        RoundRect(dc, r.left, r.top, r.right, r.bottom, 10, 10);
        SelectObject(dc, ancien);
        DeleteObject(b as *mut c_void);
        let mut rt = RECT { left: r.left, top: r.top + 9, right: r.right, bottom: r.bottom - 8 };
        texte(dc, libelle, &mut rt, DT_CENTER | DT_WORDBREAK, TEXTE, true);
        x += bw + 12;
    }
    EndPaint(h, &ps);
}

fn zone_bouton(x: i32, y: i32, largeur: i32, hauteur: i32) -> i32 {
    let mut x0 = 24i32;
    let bw = (largeur - 48 - 3 * 12) / 4;
    for (i, _) in BOUTONS.iter().enumerate() {
        let bx0 = x0;
        let bx1 = x0 + bw;
        let by0 = hauteur - 56;
        let by1 = hauteur - 18;
        if x >= bx0 && x <= bx1 && y >= by0 && y <= by1 { return i as i32; }
        x0 = bx1 + 12;
    }
    -1
}

unsafe extern "system" fn wnd_proc(h: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => { dessiner(h); 0 }
        WM_MOUSEMOVE => {
            let mut p = POINT { x: (l & 0xffff) as i32, y: ((l >> 16) & 0xffff) as i32 };
            let mut rc = std::mem::zeroed::<RECT>();
            GetClientRect(h, &mut rc);
            let z = zone_bouton(p.x, p.y, rc.right, rc.bottom);
            if z != (*ETAT).survol {
                (*ETAT).survol = z;
                InvalidateRect(h, std::ptr::null(), 0);
            }
            let mut pos = POINT { x: 0, y: 0 };
            GetCursorPos(&mut pos);
            ScreenToClient(h, &mut pos);
            let zc = zone_bouton(pos.x, pos.y, rc.right, rc.bottom);
            SetCursor(if zc >= 0 { LoadCursorW(std::ptr::null_mut(), IDC_HAND as *const u16) }
                     else { LoadCursorW(std::ptr::null_mut(), IDC_ARROW as *const u16) });
            0
        }
        WM_LBUTTONUP => {
            let x = (l & 0xffff) as i32;
            let y = ((l >> 16) & 0xffff) as i32;
            let mut rc = std::mem::zeroed::<RECT>();
            GetClientRect(h, &mut rc);
            let z = zone_bouton(x, y, rc.right, rc.bottom);
            if z >= 0 { executer_action(h, z); }
            0
        }
        WM_CLOSE => { PostQuitMessage(0); 0 }
        WM_DESTROY => { PostQuitMessage(0); 0 }
        _ => DefWindowProcW(h, msg, w, l),
    }
}

/// Exécute l'action du bouton choisi et met à jour l'affichage.
fn executer_action(h: HWND, action: i32) {
    let existant = crate::setup_core::installation_existante();
    let prefixe = existant.clone().unwrap_or_else(crate::setup_core::prefixe_defaut);
    let mut etat = unsafe { &mut *ETAT };
    etat.survol = -1;
    match action {
        0 => {
            etat.message = "Recherche de la dernière release sur GitHub…\n\nMerci de patienter…".to_string();
            etat.journal = crate::setup_core::mettre_a_jour(&prefixe, false);
            etat.termine = true;
            etat.message = format!("Mise à jour terminée.\n\nDossier : {}\n\nOuvrez un nouveau terminal,\npuis tapez : rose --version", prefixe);
        }
        1 => {
            etat.message = format!("Installation de Rose dans\n{}\n\nMerci de patienter…", prefixe);
            etat.journal = crate::setup_core::installer(&prefixe, None);
            etat.termine = true;
            etat.message = format!("Rose {} est installé.\n\nDossier : {}\n\nOuvrez un nouveau terminal,\npuis tapez : rose --version",
                                  env!("CARGO_PKG_VERSION"), prefixe);
        }
        2 => {
            match existant {
                Some(ref p) => {
                    etat.journal = crate::setup_core::desinstaller(p);
                    etat.termine = true;
                    etat.message = "Rose a été désinstallé.".to_string();
                }
                None => { etat.message = "Aucune installation de Rose détectée.".to_string(); }
            }
        }
        _ => { unsafe { PostQuitMessage(0); } return; }
    }
    unsafe { InvalidateRect(h, std::ptr::null(), 0); }
    unsafe { UpdateWindow(h); }
}

/// Point d'entrée de la fenêtre d'installation.
pub fn lancer() {
    unsafe {
        let titre = format!("Rose {}", env!("CARGO_PKG_VERSION"));
        let existant = crate::setup_core::installation_existante();
        let message = match existant {
            Some(ref p) => format!(
                "Une installation de Rose a été détectée :\n\n{}\n\nQue souhaitez-vous faire ?", p),
            None => format!(
                "Rose {} va être installé.\n\nEmplacement :\n{}\n\nAucun droit administrateur n'est requis.", 
                env!("CARGO_PKG_VERSION"),
                crate::setup_core::prefixe_defaut()),
        };
        let etat = Box::new(Etat { titre, message, journal: Vec::new(), survol: -1, termine: false });
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
        let hwnd = CreateWindowExW(0, classe.as_ptr(), titre_w.as_ptr(), WS_OVERLAPPEDWINDOW,
                                   120, 100, 600, 430, std::ptr::null_mut(), 0, inst, std::ptr::null_mut());
        if hwnd.is_null() { return; }
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
     Sur Windows, l'installeur graphique « Rose-Setup.exe » offre la même chose\n\
     avec une fenêtre dédiée.".to_string()
}
