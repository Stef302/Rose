//! Interface graphique de l'installeur Rose (Windows, Win32 natif 64 bits).
//!
//! Reprise de la maquette de référence : barre de titre sombre, panneau
//! latéral avec le logo, le titre « ROSE » et les cinq étapes, zone de
//! contenu, pied de page avec les boutons, et barre d'état.
//!
//! Ce qui est affiché vient de vraies valeurs :
//! - `SHA-256 vérifié` seulement si `setup_core::sha256_fichier` confirme le
//!   binaire du payload ; sinon `payload absent` ;
//! - `MODE UTILISATEUR` : le dossier d'installation est-il inscriptible ;
//! - la licence MIT est lue dans le fichier `LICENSE` à côté de l'exécutable
//!   (repli : résumé embarqué) ;
//! - les étapes se cochent au fur et à mesure (installation, désinstallation,
//!   mise à jour réelle via `setup_core` / `velopack`).
//!
//! Aucun contrôle standard : tout est dessiné au GDI. La police d'affichage
//! Kare et la rose sont embarquées dans `rose.res` (voir `pack-res.py`).

#![cfg(windows)]

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

// ── FFI Win32 ─────────────────────────────────────────────────────────
type HWND = *mut c_void;
type HDC = *mut c_void;
type HBRUSH = *mut c_void;
type HFONT = *mut c_void;
type HINSTANCE = *mut c_void;
type HICON = *mut c_void;
type HCURSOR = *mut c_void;
type HBITMAP = *mut c_void;
type LRESULT = isize;
type WPARAM = usize;
type LPARAM = isize;
type HRSRC = *mut c_void;
type HGLOBAL = *mut c_void;

#[repr(C)]
struct POINT { x: i32, y: i32 }

#[repr(C)]
struct RECT { left: i32, top: i32, right: i32, bottom: i32 }

#[repr(C)]
struct MSG { hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM, time: u32, pt: POINT }

#[repr(C)]
struct MINMAXINFO { pt_reservee: POINT, taille_max: POINT, position_max: POINT, taille_suivi_min: POINT, taille_suivi_max: POINT }

#[repr(C)]
struct MONITORINFO { cb_size: u32, rc_moniteur: RECT, rc_travail: RECT, drapeaux: u32 }

#[repr(C)]
struct WNDCLASSEXW {
    cbSize: u32, style: u32, lpfnWndProc: Option<unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT>,
    cbClsExtra: i32, cbWndExtra: i32, hInstance: HINSTANCE, hIcon: HICON, hCursor: HCURSOR,
    hbrBackground: HBRUSH, lpszMenuName: *const u16, lpszClassName: *const u16, hIconSm: HICON,
}

#[repr(C)]
struct PAINTSTRUCT { hdc: HDC, fErase: i32, rcPaint: RECT, fRestore: i32, fIncUpdate: i32, rgbReserved: [u8; 32] }

#[repr(C)]
struct BLENDFUNCTION { blend_op: u8, blend_flags: u8, source_constant_alpha: u8, alpha_format: u8 }

#[repr(C)]
struct BITMAPINFOHEADER {
    bi_size: u32, bi_width: i32, bi_height: i32, bi_planes: u16, bi_bit_count: u16,
    bi_compression: u32, bi_size_image: u32, bi_x_pels: i32, bi_y_pels: i32,
    bi_clr_used: u32, bi_clr_important: u32,
}

#[repr(C)]
struct BITMAPINFO { header: BITMAPINFOHEADER, colors: [u32; 1] }

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
    fn LoadCursorW(h: HINSTANCE, n: *const u16) -> HCURSOR;
    fn InvalidateRect(h: HWND, r: *const RECT, e: i32);
    fn GetClientRect(h: HWND, r: *mut RECT);
    fn SetCursor(c: HCURSOR);
    fn GetCursorPos(p: *mut POINT) -> i32;
    fn ScreenToClient(h: HWND, p: *mut POINT) -> i32;
    fn GetDC(h: HWND) -> HDC;
    fn ReleaseDC(h: HWND, dc: HDC) -> i32;
    fn GetDeviceCaps(dc: HDC, i: i32) -> i32;
    fn SetTimer(h: HWND, id: usize, ms: u32, f: *const c_void) -> usize;
    fn KillTimer(h: HWND, id: usize) -> i32;
    fn SetWindowTextW(h: HWND, t: *const u16) -> i32;
    fn SendMessageW(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT;
    // ressources
    fn FindResourceW(h: HINSTANCE, nom: *const u16, type_: *const u16) -> HRSRC;
    fn LoadResource(h: HINSTANCE, r: HRSRC) -> HGLOBAL;
    fn LockResource(r: HGLOBAL) -> *mut c_void;
    fn SizeofResource(h: HINSTANCE, r: HRSRC) -> u32;
    fn AddFontMemResourceEx(p: *const c_void, taille: u32, reserve: *mut c_void, n: *mut u32) -> isize;
    // PNG + alpha
    fn CreateDIBSection(dc: HDC, bmi: *const BITMAPINFO, usage: u32, bits: *mut *mut c_void,
                        section: *mut c_void, offset: u32) -> HBITMAP;
    fn AlphaBlend(dst: HDC, x: i32, y: i32, cx: i32, cy: i32, src: HDC, sx: i32, sy: i32,
                  scx: i32, scy: i32, f: BLENDFUNCTION) -> i32;
    fn CreateCompatibleDC(dc: HDC) -> HDC;
    fn CreateCompatibleBitmap(dc: HDC, cx: i32, cy: i32) -> HBITMAP;
    fn DeleteDC(dc: HDC) -> i32;
    fn SetDIBitsToDevice(dc: HDC, x: i32, y: i32, cx: i32, cy: i32, sx: i32, sy: i32,
                         debut: u32, lignes: u32, bits: *const c_void, bmi: *const BITMAPINFO, usage: u32) -> i32;
    fn BitBlt(dst: HDC, x: i32, y: i32, cx: i32, cy: i32, src: HDC, sx: i32, sy: i32, op: u32) -> i32;
    fn SetWindowPos(h: HWND, apres: HWND, x: i32, y: i32, cx: i32, cy: i32, f: u32) -> i32;
    fn SetForegroundWindow(h: HWND) -> i32;
    fn GetForegroundWindow() -> HWND;
    fn LoadImageW(inst: HINSTANCE, nom: *const u16, type_: u32, cx: i32, cy: i32, flags: u32) -> *mut c_void;
    fn DestroyIcon(i: HICON) -> i32;
    fn CreatePen(style: i32, epaisseur: i32, couleur: u32) -> *mut c_void;
    fn MoveToEx(dc: HDC, x: i32, y: i32, precedent: *mut POINT) -> i32;
    fn LineTo(dc: HDC, x: i32, y: i32) -> i32;
    fn MonitorFromWindow(h: HWND, drapeaux: u32) -> *mut c_void;
    fn GetMonitorInfoW(m: *mut c_void, info: *mut MONITORINFO) -> i32;
    fn SetProcessDpiAwarenessContext(ctx: isize) -> i32;
    fn SetProcessDPIAware() -> i32;
    fn GetDpiForWindow(h: HWND) -> u32;
    fn IsZoomed(h: HWND) -> i32;
    fn GetSystemMetrics(i: i32) -> i32;
    fn SetClassLongPtrW(h: HWND, index: i32, valeur: isize) -> isize;
}

const WM_DESTROY: u32 = 0x0002;
const WM_PAINT: u32 = 0x000F;
const WM_CLOSE: u32 = 0x0010;
const WM_TIMER: u32 = 0x0113;
const WM_SIZE: u32 = 0x0005;
const WM_DPICHANGED: u32 = 0x02E0;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_SETCURSOR: u32 = 0x0020;
const WM_SETICON: u32 = 0x0080;
const WM_NCHITTEST: u32 = 0x0084;
const WM_APP: u32 = 0x8000;
const WM_APP_FIN: u32 = WM_APP + 1;
const WM_GETMINMAXINFO: u32 = 0x0024;
const WM_SIZING: u32 = 0x0214;

const HTCAPTION: isize = 2;
const HTRIGHT: isize = 11;
const HTBOTTOM: isize = 15;
const HTBOTTOMRIGHT: isize = 17;
const WS_OVERLAPPEDWINDOW: u32 = 0x00CF0000;
const WS_POPUP: u32 = 0x80000000;
const WS_THICKFRAME: u32 = 0x00040000;
const WS_SYSMENU: u32 = 0x00080000;
const WS_MINIMIZEBOX: u32 = 0x00020000;
const WS_MAXIMIZEBOX: u32 = 0x00010000;
const SW_SHOW: i32 = 5;
const SW_MINIMIZE: i32 = 6;
const SW_RESTORE: i32 = 9;
const SW_MAXIMIZE: i32 = 3;
const SWP_FRAMECHANGED: u32 = 0x0020;
const SWP_NOZORDER: u32 = 0x0004;
const SWP_NOMOVE: u32 = 0x0002;
const SWP_NOSIZE: u32 = 0x0001;
const HWND_TOP: HWND = 0 as HWND;
const SM_CXSCREEN: i32 = 0;
const SM_CYSCREEN: i32 = 1;
const SM_CXSMICON: i32 = 49;
const SM_CYSMICON: i32 = 50;
const WS_EX_APPWINDOW: u32 = 0x00040000;
const WS_EX_TOOLWINDOW: u32 = 0x00000080;
const CS_HREDRAW: u32 = 0x0002;
const CS_VREDRAW: u32 = 0x0001;
const TRANSPARENT: i32 = 1;
const DT_WORDBREAK: u32 = 0x0010;
const DT_CENTER: u32 = 0x0001;
const DT_VCENTER: u32 = 0x0004;
const DT_SINGLELINE: u32 = 0x0020;
const DT_RIGHT: u32 = 0x0002;
const DT_LEFT: u32 = 0x0000;
const IDC_ARROW: u16 = 32512;
const IDC_HAND: u16 = 32649;
const LOGPIXELSY: i32 = 90;
/// Qualité de police : `ANTIALIASED_QUALITY` = 4. Le tracé passait par la
/// qualité 5 (CLEARTYPE) mais sur un DC mémoire, où ClearType n'est pas
/// appliqué : les textes sortaient crénelés. L'anti-aliasing gris lisse
/// tous les DC, y compris la surface de double-bufferisation.
const QUALITE_POLICE: i32 = 4;
const SRCCOPY: u32 = 0x00CC0020;
const TIMER_PROGRESSION: usize = 1;
const ICON_BIG: usize = 1;
const ICON_SMALL: usize = 0;
const LR_SHARED: u32 = 0x8000;
const RT_GROUP_ICON: *const u16 = 14 as *const u16;

// Couleurs : fond sombre unique, un seul rouge d'action, un seul vert d'état.
// GDI veut un COLORREF 0x00BBGGRR : les valeurs ci-dessous sont écrites en
// RGB lisible puis converties par `couleur()` — écrire directement l'hex RGB
// (0xE62F1E) donnait RGB(0x1E, 0x2F, 0xE6), un BLEU : tout l'installeur
// sortait bleu au lieu du rouge de la maquette.
const fn couleur(rgb: u32) -> u32 {
    ((rgb & 0xFF) << 16) | (rgb & 0xFF00) | ((rgb >> 16) & 0xFF)
}

const FOND: u32 = couleur(0x15100F);
const BARRE: u32 = couleur(0x0D0908);
const LATERAL: u32 = couleur(0x120D0C);
const LATERAL_FIN: u32 = couleur(0x1C1412);
const LIGNE: u32 = couleur(0x2A1E1A);
const LIGNE_FORTE: u32 = couleur(0x3A2824);
const CARTE: u32 = couleur(0x1A1311);
const ROUGE: u32 = couleur(0xE62F1E);
const ROUGE_CLAIR: u32 = couleur(0xF0503A);
const ROUGE_SOMBRE: u32 = couleur(0x8A1A1C);
const VERT: u32 = couleur(0x2CCB2C);
const TEXTE: u32 = couleur(0xE8E2E2);
const TEXTE_DOUX: u32 = couleur(0xA89894);
const TEXTE_FAIBLE: u32 = couleur(0x6E605C);
const FOND_PUCE: u32 = couleur(0x2A1A18);
const BLANC: u32 = couleur(0xFFFFFF);
// Survols du bouton secondaire (dégradé léger, en RGB).
const SURVOL_SOMBRE: u32 = couleur(0x2E2422);
const REPOS_SOMBRE: u32 = couleur(0x1E1715);

/// Les cinq étapes affichées dans le panneau latéral.
const ETAPES: [&str; 5] = ["Bienvenue", "Licence MIT", "Options", "Installation", "Terminé"];

/// Ce que la page courante propose.
#[derive(Clone, Copy, PartialEq)]
enum Vue {
    Bienvenue,
    Licence,
    Options,
    Installation,
    Termine,
}

/// Action des deux boutons du pied de page.
#[derive(Clone, Copy, PartialEq)]
enum Action {
    Retour,
    Avancer,
    Annuler,
    Fermer,
    Desinstaller,
}

struct Etat {
    vue: Vue,
    /// Étapes déjà franchies (index dans ETAPES).
    atteintes: [bool; 5],
    /// Résumé de la licence lu à côté de l'exécutable.
    licence: String,
    /// Dossier d'installation proposé.
    prefixe: String,
    /// Vrai si le dossier est inscriptible sans droits administrateur.
    utilisateur: bool,
    /// Vrai si une installation existante a été détectée.
    existant: bool,
    /// Empreinte du payload (`None` = absent ou non vérifié).
    empreinte: Option<String>,
    /// Journal de l'opération en cours (dernière ligne affichée).
    statut: String,
    /// Vrai pendant une opération (barre de progression).
    occupe: bool,
    /// Vrai quand la fenêtre est maximisée (plein écran) : le tracé ne doit
    /// alors PAS conserver le ratio du gabarit, sinon la fenêtre déborde de
    /// l'écran et la barre d'état sort par le bas.
    maximise: bool,
    phase: i32,
    /// Bouton survolé : 0 = gauche, 1 = droite, -1 = aucun.
    survol: i32,
    /// Bouton de la barre de titre survolé : 0 = réduire, 1 = agrandir,
    /// 2 = fermer, -1 = aucun (ou zone de déplacement).
    survol_titre: i32,
    /// Dernière version installée (après succès).
    version_installee: String,
    dpi: i32,
    /// Icône de la fenêtre.
    icone: HICON,
}

static mut ETAT: *mut Etat = std::ptr::null_mut();

fn hex_w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Lit une ressource embarquée par `pack-res.py` : (pointeur, taille).
unsafe fn ressource(nom: u16, type_: u16) -> Option<&'static [u8]> {
    let inst = GetModuleHandleW(std::ptr::null());
    let r = FindResourceW(inst, nom as *const u16, type_ as *const u16);
    if r.is_null() {
        return None;
    }
    let taille = SizeofResource(inst, r);
    let bloc = LoadResource(inst, r);
    if bloc.is_null() || taille == 0 {
        return None;
    }
    let p = LockResource(bloc);
    if p.is_null() {
        return None;
    }
    Some(std::slice::from_raw_parts(p as *const u8, taille as usize))
}

/// Charge la police Kare embarquée dans la session GDI du processus.
unsafe fn charger_police() {
    if let Some(ttf) = ressource(2, 8) {
        let mut n = 0u32;
        AddFontMemResourceEx(ttf.as_ptr() as *const c_void, ttf.len() as u32, std::ptr::null_mut(), &mut n);
    }
}

/// Décode un PNG RGBA 8 bits en (largeur, hauteur, pixels RGBA).
/// Aucune dépendance : en-têtes locaux + DEFLATE maison.
fn decoder_png(donnees: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    if donnees.len() < 8 || &donnees[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let mut i = 8usize;
    let mut largeur = 0usize;
    let mut hauteur = 0usize;
    let mut compression = Vec::new();
    while i + 8 <= donnees.len() {
        let taille = u32::from_be_bytes([donnees[i], donnees[i + 1], donnees[i + 2], donnees[i + 3]]) as usize;
        let type_ = &donnees[i + 4..i + 8];
        let corps = donnees.get(i + 8..i + 8 + taille)?;
        match type_ {
            b"IHDR" => {
                largeur = u32::from_be_bytes([corps[0], corps[1], corps[2], corps[3]]) as usize;
                hauteur = u32::from_be_bytes([corps[4], corps[5], corps[6], corps[7]]) as usize;
                let profondeur = corps[8];
                let couleur = corps[9];
                if profondeur != 8 || couleur != 6 {
                    return None;
                }
            }
            b"IDAT" => compression.extend_from_slice(corps),
            b"IEND" => break,
            _ => {}
        }
        i += 12 + taille;
    }
    let brut = crate::inflate::decompresser(&compression)?;
    let pas = largeur * 4;
    if brut.len() < hauteur * (pas + 1) {
        return None;
    }
    let mut precedent = vec![0u8; pas];
    let mut pixels = Vec::with_capacity(pas * hauteur);
    for y in 0..hauteur {
        let filtre = brut[y * (pas + 1)];
        let ligne = &brut[y * (pas + 1) + 1..(y + 1) * (pas + 1)];
        let mut sortie = ligne.to_vec();
        for x in 0..pas {
            let a = if x >= 4 { sortie[x - 4] } else { 0 };
            let b = precedent[x];
            let c = if x >= 4 { precedent[x - 4] } else { 0 };
            match filtre {
                1 => sortie[x] = sortie[x].wrapping_add(a),
                2 => sortie[x] = sortie[x].wrapping_add(b),
                3 => sortie[x] = sortie[x].wrapping_add(((a as u16 + b as u16) >> 1) as u8),
                4 => {
                    let p = a as i32 + b as i32 - c as i32;
                    let pa = (p - a as i32).abs();
                    let pb = (p - b as i32).abs();
                    let pc = (p - c as i32).abs();
                    let pr = if pa <= pb && pa <= pc { a } else if pb <= pc { b } else { c };
                    sortie[x] = sortie[x].wrapping_add(pr);
                }
                _ => {}
            }
        }
        pixels.extend_from_slice(&sortie);
        precedent = sortie;
    }
    Some((largeur, hauteur, pixels))
}

/// Dessine la rose embarquée, mise à l'échelle, avec transparence.
unsafe fn dessiner_rose(dc: HDC, x: i32, y: i32, cote: i32) -> bool {
    let Some(png) = ressource(1, 10) else { return false };
    let Some((largeur, hauteur, pixels)) = decoder_png(png) else { return false };

    let info = BITMAPINFO {
        header: BITMAPINFOHEADER {
            bi_size: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            bi_width: largeur as i32,
            bi_height: -(hauteur as i32),
            bi_planes: 1,
            bi_bit_count: 32,
            bi_compression: 0,
            bi_size_image: (largeur * hauteur * 4) as u32,
            bi_x_pels: 0,
            bi_y_pels: 0,
            bi_clr_used: 0,
            bi_clr_important: 0,
        },
        colors: [0],
    };
    let mut bits: *mut c_void = std::ptr::null_mut();
    let bitmap = CreateDIBSection(dc, &info, 0, &mut bits, std::ptr::null_mut(), 0);
    if bitmap.is_null() || bits.is_null() {
        return false;
    }
    // pixels PNG : RGBA → BGRA **prémultiplié**. `AlphaBlend` avec
    // AC_SRC_ALPHA (alpha_format = 1) attend une source prémultipliée ;
    // sans la multiplication, les bords transparents de la rose sortaient
    // avec un halo clair.
    //
    // Le visuel embarqué (`embed-assets.py`) a ses pixels NOIRS rendus
    // transparents : le fond sombre du panneau dessine donc lui-même les
    // contours, comme sur la maquette. Aucune inversion de couleur ici.
    let cible = std::slice::from_raw_parts_mut(bits as *mut u8, largeur * hauteur * 4);
    for k in 0..largeur * hauteur {
        let a = pixels[k * 4 + 3];
        let r = pixels[k * 4];
        let v = pixels[k * 4 + 1];
        let b = pixels[k * 4 + 2];
        cible[k * 4] = ((b as u32 * a as u32) / 255) as u8;
        cible[k * 4 + 1] = ((v as u32 * a as u32) / 255) as u8;
        cible[k * 4 + 2] = ((r as u32 * a as u32) / 255) as u8;
        cible[k * 4 + 3] = a;
    }
    let memoire = CreateCompatibleDC(dc);
    let ancien = SelectObject(memoire, bitmap as *mut c_void);
    let melange = BLENDFUNCTION {
        blend_op: 0,
        blend_flags: 0,
        source_constant_alpha: 255,
        alpha_format: 1,
    };
    AlphaBlend(dc, x, y, cote, cote, memoire, 0, 0, largeur as i32, hauteur as i32, melange);
    SelectObject(memoire, ancien);
    DeleteDC(memoire);
    DeleteObject(bitmap as *mut c_void);
    true
}

/// Hauteur en pixels d'une taille en points.
unsafe fn pixels(dpi: i32, points: i32) -> i32 {
    -((points * dpi) / 72)
}

/// Hauteur d'une taille en points, mise à l'échelle du gabarit : la police
/// suit la fenêtre (une fenêtre réduite garde des textes lisibles, sans
/// débordement).
fn points_ecrits(dpi: i32, points: i32, echelle: f64) -> i32 {
    let base = (points * dpi) / 72;
    ((base as f64 * echelle).round() as i32).max(9)
}

/// Dessine un texte : `famille` = "Kare" (titre) ou "Segoe UI" (courant).
unsafe fn texte(dc: HDC, dpi: i32, famille: &str, s: &str, r: &mut RECT, flags: u32,
                couleur: u32, points: i32, gras: bool) {
    texte_echelle(dc, dpi, famille, s, r, flags, couleur, points, gras, 1.0)
}

/// Comme `texte`, avec une police mise à l'échelle de la fenêtre.
#[allow(clippy::too_many_arguments)]
unsafe fn texte_echelle(dc: HDC, dpi: i32, famille: &str, s: &str, r: &mut RECT, flags: u32,
                        couleur: u32, points: i32, gras: bool, echelle: f64) {
    SetBkMode(dc, TRANSPARENT);
    SetTextColor(dc, couleur);
    let face = hex_w(famille);
    let poids = if gras { 700 } else { 400 };
    let taille = -points_ecrits(dpi, points, echelle);
    let police = CreateFontW(taille, 0, 0, 0, poids, 0, 0, 0, 1, 0, 0, QUALITE_POLICE, 0, face.as_ptr());
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

/// Bordure arrondie fine (1 px) : un vrai contour, pas un aplat.
/// `RoundRect` avec un pinceau *nul* (HBRUSH null = pas de remplissage)
/// ne trace que le crayon. L'ancienne version passait un pinceau plein et
/// peignait toute la zone, ce qui masquait le fond des cartes.
unsafe fn contour(dc: HDC, r: &RECT, rayon: i32, couleur: u32) {
    let stylo = CreatePen(0, 1, couleur);
    let ancien_stylo = SelectObject(dc, stylo as *mut c_void);
    let ancien_pinceau = SelectObject(dc, (1 as usize) as *mut c_void); // NULL_BRUSH
    RoundRect(dc, r.left, r.top, r.right, r.bottom, rayon, rayon);
    SelectObject(dc, ancien_pinceau);
    SelectObject(dc, ancien_stylo);
    DeleteObject(stylo as *mut c_void);
}

unsafe fn cercle(dc: HDC, cx: i32, cy: i32, rayon: i32, couleur: u32) {
    let b = CreateSolidBrush(couleur);
    let ancien = SelectObject(dc, b as *mut c_void);
    Ellipse(dc, cx - rayon, cy - rayon, cx + rayon, cy + rayon);
    SelectObject(dc, ancien);
    DeleteObject(b as *mut c_void);
}

/// Coche dessinée (deux segments).
unsafe fn coche(dc: HDC, cx: i32, cy: i32, couleur: u32) {
    // Une coche = deux segments épais (GDI, pinceau plein) ; le gabarit de
    // 1120 px est déjà appliqué à `cx`/`cy` par l'appelant.
    let epaisseur = ((cx as f64 / 41.0).round() as i32).max(2);
    let stylo = CreatePen(0, epaisseur, couleur);
    let ancien = SelectObject(dc, stylo as *mut c_void);
    let k = epaisseur.max(2);
    let a = (cx - k * 2, cy);
    let b = (cx, cy + k * 2);
    let c = (cx + k * 4, cy - k * 3);
    MoveToEx(dc, a.0, a.1, std::ptr::null_mut());
    LineTo(dc, b.0, b.1);
    MoveToEx(dc, b.0, b.1, std::ptr::null_mut());
    LineTo(dc, c.0, c.1);
    SelectObject(dc, ancien);
    DeleteObject(stylo as *mut c_void);
}

unsafe fn ligne(dc: HDC, x1: i32, y1: i32, x2: i32, y2: i32, couleur: u32, epaisseur: i32) {
    let stylo = CreatePen(0, epaisseur, couleur);
    let ancien = SelectObject(dc, stylo as *mut c_void);
    MoveToEx(dc, x1, y1, std::ptr::null_mut());
    LineTo(dc, x2, y2);
    SelectObject(dc, ancien);
    DeleteObject(stylo as *mut c_void);
}

/// Une carte de contenu : fond `CARTE`, filet d'accent rouge à gauche et
/// bordure fine. C'est le motif unique de toutes les vues.
unsafe fn carte(dc: HDC, r: &RECT, accent: u32) {
    arrondi(dc, r, 12, CARTE);
    let filet = RECT { left: r.left, top: r.top + 10, right: r.left + 3, bottom: r.bottom - 10 };
    remplir(dc, &filet, accent);
}

/// Mise en page : les deux boutons du pied de page pour la vue courante.
fn boutons(vue: Vue, existant: bool) -> (Option<(Action, &'static str)>, Option<(Action, &'static str)>) {
    match vue {
        Vue::Bienvenue | Vue::Licence => (Some((Action::Annuler, "Annuler")), Some((Action::Avancer, "Suivant"))),
        Vue::Options => (
            if existant { Some((Action::Desinstaller, "Désinstaller")) } else { Some((Action::Annuler, "Annuler")) },
            Some((Action::Avancer, "Installer")),
        ),
        Vue::Installation => (Some((Action::Annuler, "Annuler")), None),
        Vue::Termine => (Some((Action::Retour, "Fermer")), Some((Action::Fermer, "Terminer"))),
    }
}

/// Libellé et couleur du bouton de droite de la vue « Options ».
fn libelle_principal(existant: bool) -> &'static str {
    if existant { "Mettre à jour" } else { "Installer" }
}

unsafe fn dessiner(h: HWND) {
    let mut ps = std::mem::zeroed::<PAINTSTRUCT>();
    let dc_ecran = BeginPaint(h, &mut ps);
    let mut rc = std::mem::zeroed::<RECT>();
    GetClientRect(h, &mut rc);
    // Double-bufferisation : on dessine tout sur un DC mémoire puis on copie
    // d'un bloc. Sans cela, chaque `InvalidateRect` repeignait directement la
    // fenêtre, ce qui produisait un clignotement et des bords crénelés.
    let dc_memoire = CreateCompatibleDC(dc_ecran);
    let bitmap = CreateCompatibleBitmap(dc_ecran, rc.right.max(1), rc.bottom.max(1));
    let ancien_bitmap = SelectObject(dc_memoire, bitmap as *mut c_void);
    let dc = dc_memoire;
    let w = rc.right;
    let ht = rc.bottom;
    let etat = &mut *ETAT;
    // DPI réel de la fenêtre (par moniteur). `GetDpiForWindow` est exact même
    // quand la fenêtre change d'écran ; `GetDeviceCaps(LOGPIXELSY)` ne donne
    // que le DPI de démarrage du processus.
    let dpi_fenetre = GetDpiForWindow(h);
    if dpi_fenetre > 0 {
        etat.dpi = dpi_fenetre as i32;
    } else if etat.dpi == 0 {
        etat.dpi = GetDeviceCaps(dc, LOGPIXELSY);
        if etat.dpi <= 0 { etat.dpi = 96; }
    }
    let dpi = etat.dpi;

    // Le tracé est décrit sur une grille de 1120 × 780 : on le met à
    // l'échelle de la fenêtre réelle. Sans cela, une fenêtre de 736 px
    // dessinait un gabarit de 1256 px (boutons coupés, textes rognés).
    //
    // L'échelle tient compte de la LARGEUR **et** de la HAUTEUR : ne suivre
    // que la largeur laissait le pied de page et la barre d'état sortir du
    // cadre dès que la fenêtre était moins haute que le gabarit (le bouton
    // « Mettre à jour » se retrouvait coupé). Le minimum est borné par
    // `WM_GETMINMAXINFO` (voir `lancer`).
    let echelle = (w as f64 / 1120.0).min(ht as f64 / 780.0);
    let s = |v: i32| -> i32 { (v as f64 * echelle).round() as i32 };

    // fond général
    remplir(dc, &rc, FOND);

    // ── barre de titre PERSONNALISÉE (aucun cadre Windows) ──
    // La fenêtre est un WS_POPUP : tout est dessiné ici — fond, rose, marque
    // en Kare, pastille de version et trois boutons (réduire / agrandir /
    // fermer) au style de la maquette, pas ceux de Windows 11.
    let barre = RECT { left: 0, top: 0, right: w, bottom: s(56) };
    remplir(dc, &barre, BARRE);
    // filet lumineux sous la barre : sépare visuellement l'en-tête
    let filet_barre = RECT { left: 0, top: s(55), right: w, bottom: s(56) };
    remplir(dc, &filet_barre, LIGNE_FORTE);
    dessiner_rose(dc, s(18), s(16), s(24));
    // La marque « Rose » en Kare, comme sur le site et la maquette.
    let mut rt = RECT { left: s(52), top: s(13), right: s(210), bottom: s(43) };
    texte_echelle(dc, dpi, "Kare", "Rose", &mut rt, DT_LEFT | DT_VCENTER | DT_SINGLELINE, TEXTE, 15, false, echelle);
    let mut rt2 = RECT { left: s(210), top: s(16), right: s(430), bottom: s(40) };
    texte_echelle(dc, dpi, "Segoe UI", "— Programme d'installation", &mut rt2,
          DT_LEFT | DT_VCENTER | DT_SINGLELINE, TEXTE_DOUX, 10, false, echelle);
    // pastille de version
    let pastille = RECT { left: s(438), top: s(16), right: s(438 + 128), bottom: s(40) };
    contour(dc, &pastille, 8, LIGNE_FORTE);
    let mut rv = RECT { left: pastille.left, top: pastille.top, right: pastille.right, bottom: pastille.bottom };
    texte_echelle(dc, dpi, "Segoe UI", &format!("v{}  ·  x64", env!("CARGO_PKG_VERSION")), &mut rv,
          DT_CENTER | DT_VCENTER | DT_SINGLELINE, VERT, 9, true, echelle);

    // ── trois boutons de fenêtre, dessinés à la main ──
    // Largeur 46 px chacun sur la grille ; le survol éclaircit, le bouton
    // fermer passe au rouge (comportement attendu, mais dans NOTRE style).
    let bouton_l = s(46);
    let (b_fermer, b_agrandir, b_reduire) = (
        w - bouton_l,
        w - 2 * bouton_l,
        w - 3 * bouton_l,
    );
    let survol_titre = if etat.survol_titre >= 0 { etat.survol_titre } else { -1 };
    for (i, gauche) in [(0usize, b_reduire), (1, b_agrandir), (2, b_fermer)] {
        let zone = RECT { left: gauche, top: 0, right: gauche + bouton_l, bottom: s(56) };
        if survol_titre == i as i32 {
            let fond = if i == 2 { ROUGE } else { SURVOL_SOMBRE };
            remplir(dc, &zone, fond);
        }
        let couleur = if survol_titre == i as i32 && i == 2 { BLANC } else { TEXTE_DOUX };
        let cx = gauche + bouton_l / 2;
        let cy = s(28);
        let g = s(5); // demi-taille du glyphe
        match i {
            // réduire : un trait horizontal
            0 => ligne(dc, cx - g, cy, cx + g, cy, couleur, (s(1)).max(1)),
            // agrandir : un carré (ou deux carrés si déjà maximisé)
            1 => {
                if etat.maximise {
                    // deux carrés superposés : « restaurer »
                    let d = g;
                    let e2 = s(2);
                    let stylo = CreatePen(0, (s(1)).max(1), couleur);
                    let ancien = SelectObject(dc, stylo as *mut c_void);
                    let pinceau_nul = SelectObject(dc, (1 as usize) as *mut c_void);
                    RoundRect(dc, cx - d, cy - d + e2, cx + d - e2, cy + d, 0, 0);
                    RoundRect(dc, cx - d + e2, cy - d, cx + d, cy + d - e2, 0, 0);
                    SelectObject(dc, pinceau_nul);
                    SelectObject(dc, ancien);
                    DeleteObject(stylo as *mut c_void);
                } else {
                    let stylo = CreatePen(0, (s(1)).max(1), couleur);
                    let ancien = SelectObject(dc, stylo as *mut c_void);
                    let pinceau_nul = SelectObject(dc, (1 as usize) as *mut c_void);
                    RoundRect(dc, cx - g, cy - g, cx + g, cy + g, 0, 0);
                    SelectObject(dc, pinceau_nul);
                    SelectObject(dc, ancien);
                    DeleteObject(stylo as *mut c_void);
                }
            }
            // fermer : une croix tracée au stylo
            _ => {
                let e = (s(1)).max(1);
                ligne(dc, cx - g, cy - g, cx + g, cy + g, couleur, e);
                ligne(dc, cx - g, cy + g, cx + g, cy - g, couleur, e);
            }
        }
    }
    ZONES_TITRE = [b_reduire, b_agrandir, b_fermer];

    // ── panneau latéral ──
    let pied_h = s(76);
    let etat_h = s(30);
    let lateral_w = s(320);
    let lateral = RECT { left: 0, top: s(56), right: lateral_w, bottom: ht - etat_h };
    remplir(dc, &lateral, LATERAL);
    let filet = RECT { left: lateral_w - 1, top: s(56), right: lateral_w, bottom: ht - etat_h };
    remplir(dc, &filet, LIGNE_FORTE);

    // La rose de la maquette occupe ~110 px sur 320 de large, sous une barre
    // de titre de 56 px : 120 px centrés, titre « ROSE » juste dessous.
    dessiner_rose(dc, lateral_w / 2 - s(60), s(78), s(124));
    let mut rl = RECT { left: 0, top: s(206), right: lateral_w, bottom: s(268) };
    // Marque en Kare, comme le site : c'est la signature visuelle de Rose.
    texte_echelle(dc, dpi, "Kare", "Rose", &mut rl, DT_CENTER | DT_VCENTER | DT_SINGLELINE, TEXTE, 36, false, echelle);
    let mut rs = RECT { left: s(24), top: s(274), right: lateral_w - s(24), bottom: s(334) };
    texte_echelle(dc, dpi, "Segoe UI", "Le langage compilé en français qui se compile lui-même.",
          &mut rs, DT_CENTER | DT_WORDBREAK, TEXTE_DOUX, 9, false, echelle);

    // étapes
    let mut y = s(344);
    for (i, nom) in ETAPES.iter().enumerate() {
        let atteinte = etat.atteintes[i];
        let courante = (etat.vue as usize) == i;
        let couleur_puce = if atteinte { ROUGE } else if courante { ROUGE_SOMBRE } else { FOND_PUCE };
        cercle(dc, s(48), y + s(12), s(14), couleur_puce);
        if courante && !atteinte {
            // anneau : l'étape en cours se distingue des étapes à venir
            cercle(dc, s(48), y + s(12), s(14), ROUGE_CLAIR);
            cercle(dc, s(48), y + s(12), s(11), LATERAL);
        }
        if atteinte {
            coche(dc, s(48), y + s(12), TEXTE);
        } else {
            let mut rn = RECT { left: s(34), top: y, right: s(62), bottom: y + s(24) };
            texte_echelle(dc, dpi, "Segoe UI", &format!("{}", i + 1), &mut rn,
                  DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                  if courante { TEXTE } else { TEXTE_FAIBLE }, 9, false, echelle);
        }
        let couleur_nom = if courante || atteinte { TEXTE } else { TEXTE_FAIBLE };
        let mut rnom = RECT { left: s(74), top: y, right: lateral_w - s(20), bottom: y + s(24) };
        texte_echelle(dc, dpi, "Segoe UI", nom, &mut rnom, DT_SINGLELINE | DT_VCENTER, couleur_nom, 10, false, echelle);
        y += s(44);
    }

    // cartes du bas du panneau latéral
    let bas = ht - etat_h - s(18);
    let carte2 = RECT { left: s(20), top: bas - s(38), right: lateral_w - s(20), bottom: bas };
    contour(dc, &carte2, 10, LIGNE_FORTE);
    let mut rc2 = RECT { left: s(34), top: carte2.top, right: carte2.right - s(14), bottom: carte2.bottom };
    texte_echelle(dc, dpi, "Segoe UI", if etat.utilisateur { "⚿  MODE UTILISATEUR" } else { "⚿  DROITS REQUIS" },
          &mut rc2, DT_VCENTER | DT_SINGLELINE, TEXTE_DOUX, 9, true, echelle);
    let carte1 = RECT { left: s(20), top: carte2.top - s(46), right: lateral_w - s(20), bottom: carte2.top - s(8) };
    contour(dc, &carte1, 10, LIGNE_FORTE);
    let mut rc1 = RECT { left: s(34), top: carte1.top, right: carte1.right - s(14), bottom: carte1.bottom };
    texte_echelle(dc, dpi, "Segoe UI", "{ ROSE } · MIT", &mut rc1, DT_VCENTER | DT_SINGLELINE, VERT, 9, true, echelle);

    // ── zone de contenu ──
    let contenu = RECT { left: lateral_w + 1, top: s(56), right: w, bottom: ht - etat_h - pied_h };
    let marge = s(56);
    let g = contenu.left + marge;
    let d = contenu.right - marge;
    let mut yc = contenu.top + s(44);
    let (titre, sous) = match etat.vue {
        Vue::Bienvenue => (
            "Bienvenue".to_string(),
            "Ce programme installe le compilateur Rose, ses binaires Windows et le helper AOT.".to_string(),
        ),
        Vue::Licence => ("Licence MIT".to_string(), "Rose est distribué sous licence MIT.".to_string()),
        Vue::Options => ("Options".to_string(), "Dossier d'installation et vérification du paquet.".to_string()),
        Vue::Installation => ("Installation".to_string(), etat.statut.clone()),
        Vue::Termine => ("Terminé".to_string(), "Rose est prêt.".to_string()),
    };
    let mut rti = RECT { left: g, top: yc, right: d, bottom: yc + s(44) };
    // Titre de section en Kare — mais EN MAJUSCULES : Kare est une police
    // d'AFFICHAGE (ses minuscules dessinent les capitales), donc un titre en
    // minuscules se lit mal (« Options » → « PTIONS »). Le site réserve Kare
    // aux titres, à la marque et aux boutons, et Segoe UI au texte courant.
    let titre_kare = titre.to_uppercase();
    texte_echelle(dc, dpi, "Kare", &titre_kare, &mut rti, DT_SINGLELINE | DT_VCENTER, TEXTE, 20, false, echelle);
    yc += s(52);
    let mut rso = RECT { left: g, top: yc, right: d, bottom: yc + s(30) };
    texte_echelle(dc, dpi, "Segoe UI", &sous, &mut rso, DT_WORDBREAK, TEXTE_DOUX, 10, false, echelle);
    yc += s(48);

    match etat.vue {
        Vue::Bienvenue => {
            let c = RECT { left: g, top: yc, right: d, bottom: yc + s(150) };
            carte(dc, &c, ROUGE);
            let mut r = RECT { left: g + s(20), top: yc + s(16), right: d - s(20), bottom: yc + s(140) };
            texte_echelle(dc, dpi, "Segoe UI",
                  "Rose est un langage de programmation en français, auto-hébergé : \
                   interprète exact, JIT Cranelift, AOT LLVM et module WASM.\n\n\
                   L'installation se fait dans votre profil utilisateur — aucun droit \
                   administrateur n'est demandé.",
                  &mut r, DT_WORDBREAK, TEXTE, 10, false, echelle);
            let mut ri = RECT { left: g, top: yc + s(164), right: d, bottom: yc + s(230) };
            texte_echelle(dc, dpi, "Segoe UI",
                  if etat.existant {
                      "Une installation existante a été détectée : la suite de l'assistant \
                       propose de la mettre à jour ou de la désinstaller."
                  } else {
                      "Aucune installation détectée : l'assistant proposera le dossier par défaut."
                  },
                  &mut ri, DT_WORDBREAK, TEXTE_DOUX, 9, false, echelle);
        }
        Vue::Licence => {
            let c = RECT { left: g, top: yc, right: d, bottom: contenu.bottom - s(24) };
            carte(dc, &c, VERT);
            let mut r = RECT { left: g + s(20), top: yc + s(16), right: d - s(20), bottom: c.bottom - s(14) };
            texte_echelle(dc, dpi, "Segoe UI", &etat.licence, &mut r, DT_WORDBREAK, TEXTE, 9, false, echelle);
        }
        Vue::Options => {
            let c = RECT { left: g, top: yc, right: d, bottom: yc + s(92) };
            carte(dc, &c, ROUGE);
            let mut r1 = RECT { left: g + s(20), top: yc + s(14), right: d - s(20), bottom: yc + s(34) };
            texte_echelle(dc, dpi, "Segoe UI", "Dossier d'installation", &mut r1, DT_SINGLELINE | DT_VCENTER,
                  TEXTE_DOUX, 9, true, echelle);
            let mut r2 = RECT { left: g + s(20), top: yc + s(38), right: d - s(20), bottom: yc + s(62) };
            texte_echelle(dc, dpi, "Segoe UI", &etat.prefixe, &mut r2, DT_SINGLELINE | DT_VCENTER, TEXTE, 10, false, echelle);
            let mut r3 = RECT { left: g + s(20), top: yc + s(66), right: d - s(20), bottom: yc + s(88) };
            texte_echelle(dc, dpi, "Segoe UI",
                  if etat.utilisateur {
                      "✓  Écriture autorisée dans ce dossier (profil utilisateur)"
                  } else {
                      "!  Ce dossier n'est pas inscriptible : choisissez un autre emplacement"
                  },
                  &mut r3, DT_SINGLELINE | DT_VCENTER,
                  if etat.utilisateur { VERT } else { ROUGE_CLAIR }, 9, false, echelle);

            let c2 = RECT { left: g, top: yc + s(108), right: d, bottom: yc + s(212) };
            carte(dc, &c2, VERT);
            let mut r4 = RECT { left: g + s(20), top: yc + s(122), right: d - s(20), bottom: yc + s(142) };
            texte_echelle(dc, dpi, "Segoe UI", "Paquet à installer", &mut r4, DT_SINGLELINE | DT_VCENTER,
                  TEXTE_DOUX, 9, true, echelle);
            let mut r5 = RECT { left: g + s(20), top: yc + s(146), right: d - s(20), bottom: yc + s(168) };
            texte_echelle(dc, dpi, "Segoe UI",
                  &match &etat.empreinte {
                      Some(h) => format!("SHA-256 vérifié · {}", &h[..16]),
                      None => "payload absent — l'installeur copiera le moteur du paquet".to_string(),
                  },
                  &mut r5, DT_SINGLELINE | DT_VCENTER,
                  if etat.empreinte.is_some() { VERT } else { TEXTE_DOUX }, 9, false, echelle);
            let mut r6 = RECT { left: g + s(20), top: yc + s(170), right: d - s(20), bottom: yc + s(196) };
            texte_echelle(dc, dpi, "Segoe UI",
                  "rose.exe · rose-aot.exe · association .rose · entrée de désinstallation",
                  &mut r6, DT_SINGLELINE | DT_VCENTER, TEXTE_DOUX, 9, false, echelle);
        }
        Vue::Installation => {
            let c = RECT { left: g, top: yc, right: d, bottom: yc + s(70) };
            carte(dc, &c, ROUGE_CLAIR);
            let mut r = RECT { left: g + s(20), top: yc + s(16), right: d - s(20), bottom: yc + s(54) };
            texte_echelle(dc, dpi, "Segoe UI", &etat.statut, &mut r, DT_WORDBREAK, TEXTE, 10, false, echelle);
            // barre de progression indéterminée
            let piste = RECT { left: g, top: yc + s(86), right: d, bottom: yc + s(92) };
            remplir(dc, &piste, LIGNE);
            let segment = (d - g) / 4;
            let course = (d - g) + segment;
            let debut = etat.phase % course;
            let a = (debut - segment).max(0) + g;
            let b = (debut).min(d - g) + g;
            if b > a {
                let barre = RECT { left: a, top: piste.top, right: b, bottom: piste.bottom };
                remplir(dc, &barre, ROUGE_CLAIR);
            }
        }
        Vue::Termine => {
            let c = RECT { left: g, top: yc, right: d, bottom: yc + s(130) };
            carte(dc, &c, VERT);
            coche(dc, g + s(36), yc + s(32), VERT);
            let mut r = RECT { left: g + s(64), top: yc + s(18), right: d - s(20), bottom: yc + s(120) };
            texte_echelle(dc, dpi, "Segoe UI",
                  &format!("Rose {} est installé.\n\nDossier : {}\n\nOuvrez un nouveau terminal, puis tapez : rose --version",
                           etat.version_installee, etat.prefixe),
                  &mut r, DT_WORDBREAK, TEXTE, 10, false, echelle);
        }
    }

    // ── pied de page : boutons ──
    let pied = RECT { left: lateral_w + 1, top: ht - etat_h - pied_h, right: w, bottom: ht - etat_h };
    remplir(dc, &pied, LATERAL_FIN);
    let filet_pied = RECT { left: lateral_w + 1, top: pied.top, right: w, bottom: pied.top + 1 };
    remplir(dc, &filet_pied, LIGNE_FORTE);
    let (gauche, droite) = boutons(etat.vue, etat.existant);
    let bw = s(190);
    let bh = s(44);
    let by = pied.top + (pied_h - bh) / 2;
    let mut zones = [std::mem::zeroed::<RECT>(), std::mem::zeroed::<RECT>()];
    if let Some((action, libelle)) = gauche {
        let r = RECT { left: lateral_w + s(28), top: by, right: lateral_w + s(28) + bw, bottom: by + bh };
        // Le survol éclaircit le bouton secondaire.
        let fond = if etat.survol == 0 { SURVOL_SOMBRE } else { REPOS_SOMBRE };
        arrondi(dc, &r, 10, fond);
        contour(dc, &r, 10, if etat.survol == 0 { LIGNE_FORTE } else { LIGNE });
        let mut rt = RECT { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
        // Libellés des boutons en Kare MAJUSCULE (comme `.btn` du site) :
        // Kare dessine ses minuscules comme des capitales, l'uppercase est
        // donc la seule forme cohérente.
        texte_echelle(dc, dpi, "Kare", &libelle.to_uppercase(), &mut rt,
              DT_CENTER | DT_VCENTER | DT_SINGLELINE,
              if action == Action::Desinstaller { ROUGE_CLAIR } else { TEXTE }, 11, false, echelle);
        zones[0] = r;
        let _ = action;
    }
    if let Some((_, libelle)) = droite {
        let r = RECT { left: w - s(28) - bw, top: by, right: w - s(28), bottom: by + bh };
        let couleur = if etat.survol == 1 { ROUGE_CLAIR } else { ROUGE };
        arrondi(dc, &r, 10, couleur);
        let mut rt = RECT { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
        texte_echelle(dc, dpi, "Kare", &libelle.to_uppercase(), &mut rt,
              DT_CENTER | DT_VCENTER | DT_SINGLELINE, BLANC, 11, false, echelle);
        zones[1] = r;
    }

    // ── barre d'état ──
    let etat_barre = RECT { left: 0, top: ht - etat_h, right: w, bottom: ht };
    remplir(dc, &etat_barre, BARRE);
    remplir(dc, &RECT { left: 0, top: etat_barre.top, right: w, bottom: etat_barre.top + 1 }, LIGNE_FORTE);
    cercle(dc, s(16), ht - etat_h / 2, s(5), if etat.empreinte.is_some() { VERT } else { TEXTE_FAIBLE });
    let mut rg = RECT { left: s(28), top: etat_barre.top, right: w / 2 - s(10), bottom: etat_barre.bottom };
    texte_echelle(dc, dpi, "Segoe UI", &etat.statut, &mut rg, DT_VCENTER | DT_SINGLELINE, TEXTE_DOUX, 9, false, echelle);
    let mut rd = RECT { left: w / 2 + s(10), top: etat_barre.top, right: w - s(14), bottom: etat_barre.bottom };
    texte_echelle(dc, dpi, "Segoe UI",
          &format!("Rose-win-Setup.exe · Windows x64 · © 2026 Stef302"),
          &mut rd, DT_VCENTER | DT_SINGLELINE | DT_RIGHT, TEXTE_FAIBLE, 9, false, echelle);

    // garder les zones pour le clic
    ZONES = zones;
    // copie du tampon mémoire vers la fenêtre, puis libération
    BitBlt(dc_ecran, 0, 0, rc.right, rc.bottom, dc_memoire, 0, 0, SRCCOPY);
    SelectObject(dc_memoire, ancien_bitmap);
    DeleteObject(bitmap as *mut c_void);
    DeleteDC(dc_memoire);
    EndPaint(h, &ps);
}

static mut ZONES: [RECT; 2] = [
    RECT { left: 0, top: 0, right: 0, bottom: 0 },
    RECT { left: 0, top: 0, right: 0, bottom: 0 },
];

/// Bords gauches des trois boutons de la barre de titre (réduire, agrandir,
/// fermer), remplis par `dessiner`.
static mut ZONES_TITRE: [i32; 3] = [0, 0, 0];

fn dans(r: &RECT, x: i32, y: i32) -> bool {
    r.right > r.left && x >= r.left && x <= r.right && y >= r.top && y <= r.bottom
}

/// Zone de la barre de titre : -1 hors barre, 0 = déplacement, 1 = réduire,
/// 2 = agrandir, 3 = fermer.
/// Les bornes suivent l'échelle du tracé (barre de 56 px sur la grille
/// 1120 × 780), sinon les zones cliquables ne correspondent plus au dessin
/// quand la fenêtre est redimensionnée.
fn zone_titre(w: i32, h: i32, x: i32, y: i32) -> i32 {
    let echelle = (w as f64 / 1120.0).min(h as f64 / 780.0);
    let s = |v: i32| -> i32 { (v as f64 * echelle).round() as i32 };
    if y > s(56) {
        return -1;
    }
    let bl = s(46);
    let zones = unsafe { *std::ptr::addr_of!(ZONES_TITRE) };
    if x >= zones[2] {
        3
    } else if x >= zones[1] {
        2
    } else if x >= zones[0] {
        1
    } else if x >= w - 3 * bl {
        // zone restante des boutons (si pas encore peinte) : fermer
        3
    } else {
        0
    }
}

unsafe extern "system" fn wnd_proc(h: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_GETMINMAXINFO => {
            // Taille minimale = la grille de tracé (1120 × 780) à l'échelle
            // du DPI : en dessous, le pied de page et la barre d'état
            // n'auraient plus la place de tenir (bouton « Mettre à jour »
            // coupé). La taille maximale suit la zone utile de l'écran.
            let info = l as *mut MINMAXINFO;
            if !info.is_null() {
                let dpi = if (*ETAT).dpi > 0 { (*ETAT).dpi } else { 96 };
                let facteur = dpi as f64 / 96.0;
                let mini_l = (1120.0 * facteur * 0.72).round() as i32;
                let mini_h = (780.0 * facteur * 0.72).round() as i32;
                (*info).taille_suivi_min = POINT { x: mini_l, y: mini_h };
                let mut ecran = MONITORINFO {
                    cb_size: std::mem::size_of::<MONITORINFO>() as u32,
                    rc_moniteur: std::mem::zeroed(),
                    rc_travail: std::mem::zeroed(),
                    drapeaux: 0,
                };
                if GetMonitorInfoW(MonitorFromWindow(h, 2), &mut ecran) != 0 {
                    (*info).position_max = POINT { x: ecran.rc_travail.right, y: ecran.rc_travail.bottom };
                    (*info).taille_max = POINT {
                        x: ecran.rc_travail.right - ecran.rc_travail.left,
                        y: ecran.rc_travail.bottom - ecran.rc_travail.top,
                    };
                }
            }
            0
        }
        WM_SIZING => {
            // Le redimensionnement à la souris garde le ratio du gabarit :
            // le tracé est calculé sur une grille 1120 × 780, une fenêtre très
            // allongée laisserait de larges bandes vides ou rognerait le
            // contenu. JAMAIS quand la fenêtre est maximisée : forcer le ratio
            // sur un écran 16:9 rendait la fenêtre plus haute que l'écran, et
            // la barre d'état passait sous le bord inférieur, inaccessible.
            if (*ETAT).maximise {
                return 1;
            }
            let r = l as *mut RECT;
            if !r.is_null() {
                let largeur = (*r).right - (*r).left;
                let hauteur = ((largeur as f64) * 780.0 / 1120.0).round() as i32;
                (*r).bottom = (*r).top + hauteur;
            }
            1
        }
        WM_SIZE => {
            // `w` = SIZE_MAXIMIZED (2) ou SIZE_RESTORED (0) : on suit l'état
            // pour que WM_SIZING et le tracé sachent s'il faut garder le ratio.
            let etat = &mut *ETAT;
            etat.maximise = w == 2;
            InvalidateRect(h, std::ptr::null(), 0);
            0
        }
        WM_DPICHANGED => {
            // La fenêtre a changé d'écran (ou l'utilisateur a modifié la mise
            // à l'échelle) : on adopte le nouveau DPI et la taille proposée
            // par Windows, sinon le tracé garde l'ancienne échelle.
            let nouveau = (w & 0xffff) as i32;
            if nouveau > 0 {
                (*ETAT).dpi = nouveau;
            }
            let suggere = l as *const RECT;
            if !suggere.is_null() {
                let r = &*suggere;
                SetWindowPos(h, HWND_TOP, r.left, r.top,
                             r.right - r.left, r.bottom - r.top,
                             SWP_NOZORDER | SWP_FRAMECHANGED);
            }
            InvalidateRect(h, std::ptr::null(), 0);
            0
        }
        WM_PAINT => { dessiner(h); 0 }
        WM_TIMER => {
            let etat = &mut *ETAT;
            if w == TIMER_PROGRESSION && etat.occupe {
                etat.phase += 20;
                InvalidateRect(h, std::ptr::null(), 0);
            }
            0
        }
        WM_NCHITTEST => {
            // Fenêtre sans cadre : on rend nous-mêmes les zones de
            // redimensionnement (bords et coins), la barre de déplacement et
            // les trois boutons. Sans cela, WS_THICKFRAME n'aurait aucune
            // bordure visible à attraper.
            let mut rc = std::mem::zeroed::<RECT>();
            GetClientRect(h, &mut rc);
            let mut pos = POINT { x: (l & 0xffff) as i32, y: ((l >> 16) & 0xffff) as i32 };
            ScreenToClient(h, &mut pos);
            let (x, y) = (pos.x, pos.y);
            let echelle = (rc.right as f64 / 1120.0).min(rc.bottom as f64 / 780.0);
            let bord = ((8.0 * echelle).round() as i32).max(4);
            let (w, ht) = (rc.right, rc.bottom);
            // coins d'abord, puis bords
            if x >= w - bord && y >= ht - bord {
                HTBOTTOMRIGHT
            } else if y >= ht - bord {
                HTBOTTOM
            } else if x >= w - bord {
                HTRIGHT
            } else {
                match zone_titre(w, ht, x, y) {
                    0 => HTCAPTION,
                    _ => 1, // HTCLIENT : les boutons sont gérés dans WM_LBUTTONUP
                }
            }
        }
        WM_MOUSEMOVE => {
            let mut rc = std::mem::zeroed::<RECT>();
            GetClientRect(h, &mut rc);
            let mut pos = POINT { x: 0, y: 0 };
            GetCursorPos(&mut pos);
            ScreenToClient(h, &mut pos);
            let zones = &*std::ptr::addr_of!(ZONES);
            let mut z = -1;
            if dans(&zones[0], pos.x, pos.y) { z = 0; }
            if dans(&zones[1], pos.x, pos.y) { z = 1; }
            let zt = match zone_titre(rc.right, rc.bottom, pos.x, pos.y) {
                1 => 0,
                2 => 1,
                3 => 2,
                _ => -1,
            };
            let etat = &mut *ETAT;
            let mut change = false;
            if z != etat.survol { etat.survol = z; change = true; }
            if zt != etat.survol_titre { etat.survol_titre = zt; change = true; }
            if change {
                InvalidateRect(h, std::ptr::null(), 0);
            }
            SetCursor(if z >= 0 || zt >= 0 {
                LoadCursorW(std::ptr::null_mut(), IDC_HAND as *const u16)
            } else {
                LoadCursorW(std::ptr::null_mut(), IDC_ARROW as *const u16)
            });
            0
        }
        WM_LBUTTONUP => {
            let mut rc = std::mem::zeroed::<RECT>();
            GetClientRect(h, &mut rc);
            let mut pos = POINT { x: (l & 0xffff) as i32, y: ((l >> 16) & 0xffff) as i32 };
            ScreenToClient(h, &mut pos);
            let (x, y) = (pos.x, pos.y);
            match zone_titre(rc.right, rc.bottom, x, y) {
                3 => { PostQuitMessage(0); return 0; }
                2 => {
                    // agrandir / restaurer
                    let etat = &mut *ETAT;
                    if etat.maximise {
                        ShowWindow(h, SW_RESTORE);
                    } else {
                        ShowWindow(h, SW_MAXIMIZE);
                    }
                    return 0;
                }
                1 => { ShowWindow(h, SW_MINIMIZE); return 0; }
                _ => {}
            }
            let zones = &*std::ptr::addr_of!(ZONES);
            if dans(&zones[0], x, y) { action(h, 0); }
            else if dans(&zones[1], x, y) { action(h, 1); }
            0
        }
        WM_APP_FIN => {
            let etat = &mut *ETAT;
            etat.occupe = false;
            etat.atteintes[3] = true;
            etat.atteintes[4] = true;
            etat.vue = Vue::Termine;
            let desinstalle = w == 1;
            if desinstalle {
                etat.existant = false;
                etat.statut = STATUT
                    .lock()
                    .ok()
                    .and_then(|g| g.clone())
                    .unwrap_or_else(|| "Rose a été désinstallé.".to_string());
            } else {
                etat.statut = STATUT
                    .lock()
                    .ok()
                    .and_then(|g| g.clone())
                    .unwrap_or_else(|| "Installation terminée · prêt.".to_string());
            }
            rafraichir(h);
            0
        }
        WM_CLOSE | WM_DESTROY => { PostQuitMessage(0); 0 }
        _ => DefWindowProcW(h, msg, w, l),
    }
}

/// Exécute l'action d'un bouton du pied de page.
fn action(h: HWND, bouton: usize) {
    let etat = unsafe { &mut *ETAT };
    etat.survol = -1;
    let (gauche, droite) = boutons(etat.vue, etat.existant);
    let choix = if bouton == 0 { gauche } else { droite };
    let Some((action, _)) = choix else { return };
    match action {
        Action::Annuler => {
            unsafe { PostQuitMessage(0); }
        }
        Action::Fermer => {
            unsafe { PostQuitMessage(0); }
        }
        Action::Retour => {
            unsafe { PostQuitMessage(0); }
        }
        Action::Avancer => {
            match etat.vue {
                Vue::Bienvenue => { etat.vue = Vue::Licence; etat.atteintes[0] = true; etat.atteintes[1] = true; }
                Vue::Licence => { etat.vue = Vue::Options; etat.atteintes[2] = true; }
                Vue::Options => {
                    etat.vue = Vue::Installation;
                    etat.occupe = true;
                    etat.phase = 0;
                    etat.statut = if etat.existant {
                        "Mise à jour de Rose…".to_string()
                    } else {
                        "Installation de Rose…".to_string()
                    };
                    unsafe { rafraichir(h); }
                    lancer_travail();
                    return;
                }
                _ => {}
            }
            unsafe { rafraichir(h); }
        }
        Action::Desinstaller => {
            etat.vue = Vue::Installation;
            etat.occupe = true;
            etat.phase = 0;
            etat.statut = "Désinstallation de Rose…".to_string();
            unsafe { rafraichir(h); }
            let prefixe = etat.prefixe.clone();
            // HWND n'est pas Send : on transmet son adresse comme entier, et
            // le fil de travail se contente de poster WM_APP_FIN à la fenêtre
            // (le traitement de l'état se fait dans la boucle de messages).
            let cible = h as isize;
            std::thread::spawn(move || {
                let journal = crate::setup_core::desinstaller(&prefixe);
                STATUT.lock().unwrap().replace(journal.last().cloned().unwrap_or_else(|| "Désinstallation terminée.".to_string()));
                let h = cible as HWND;
                if !h.is_null() {
                    unsafe { SendMessageW(h, WM_APP_FIN, 1, 0); }
                }
            });
        }
    }
}

/// Texte partagé entre le fil de travail et la fenêtre.
static STATUT: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Lance l'installation (ou la mise à jour) dans un fil, pour que la barre de
/// progression continue d'animer : le résultat revient par `WM_APP_FIN`.
fn lancer_travail() {
    let prefixe = unsafe { (*ETAT).prefixe.clone() };
    let existant = unsafe { (*ETAT).existant };
    std::thread::spawn(move || {
        // Source du moteur : `payload\rose.exe` (ou `rose.exe`) à côté de
        // l'installeur, sinon l'exécutable courant (installeur autonome).
        let source = source_moteur();
        let journal = if existant {
            crate::setup_core::mettre_a_jour(&prefixe, false)
        } else {
            crate::setup_core::installer(&prefixe, source.as_deref())
        };
        let derniere = journal.last().cloned().unwrap_or_else(|| "Terminé.".to_string());
        STATUT.lock().unwrap().replace(derniere);
        let cible = unsafe { HWND_FENETRE as isize };
        let h = cible as HWND;
        if !h.is_null() {
            unsafe { SendMessageW(h, WM_APP_FIN, 0, 0); }
        }
    });
}

static mut HWND_FENETRE: HWND = std::ptr::null_mut();

unsafe fn rafraichir(h: HWND) {
    InvalidateRect(h, std::ptr::null(), 0);
    UpdateWindow(h);
    let etat = &*ETAT;
    let titre = hex_w(&format!("Rose {} — Programme d'installation", env!("CARGO_PKG_VERSION")));
    SetWindowTextW(h, titre.as_ptr());
    if etat.occupe {
        SetTimer(h, TIMER_PROGRESSION, 16, std::ptr::null());
    } else {
        KillTimer(h, TIMER_PROGRESSION);
    }
}

/// Binaire Rose à installer, trouvé à côté de l'installeur :
/// `payload\rose.exe`, puis `rose.exe`, puis l'exécutable courant (quand
/// `Rose-win-Setup.exe` a été renommé en `rose.exe`).
pub fn source_moteur() -> Option<PathBuf> {
    let dossier = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let courant = std::env::current_exe().ok()?;
    [
        dossier.join("payload").join("rose.exe"),
        dossier.join("rose.exe"),
        dossier.join("payload").join("rose"),
        dossier.join("rose"),
        courant,
    ]
    .into_iter()
    .find(|p| p.is_file())
}

/// Empreinte SHA-256 du payload posé à côté de l'installeur, si présent.
fn empreinte_payload() -> Option<String> {
    crate::setup_core::sha256_fichier(&source_moteur()?)
}

/// Licence MIT : celle du dépôt si elle est à côté de l'exécutable, sinon un
/// résumé qui renvoie au fichier LICENSE.
fn lire_licence() -> String {
    let candidats = [
        "LICENSE",
        "..\\LICENSE",
        "..\\..\\LICENSE",
    ];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dossier) = exe.parent() {
            for c in candidats {
                let p: PathBuf = dossier.join(c);
                if let Ok(t) = std::fs::read_to_string(&p) {
                    if !t.trim().is_empty() {
                        return t;
                    }
                }
            }
        }
    }
    "Licence MIT\n\n\
     Copyright © 2026 Stef302\n\n\
     L'autorisation est accordée gratuitement à toute personne obtenant une copie \
     de ce logiciel et des fichiers de documentation associés, d'utiliser le logiciel \
     sans restriction, y compris les droits d'utiliser, copier, modifier, fusionner, \
     publier, distribuer, sous-licencier et/ou vendre des copies du logiciel.\n\n\
     LE LOGICIEL EST FOURNI « EN L'ÉTAT », SANS GARANTIE D'AUCUNE SORTE.\n\n\
     Le texte complet se trouve dans le fichier LICENSE du dépôt."
        .to_string()
}

fn est_inscriptible(dossier: &Path) -> bool {
    crate::setup_core::inscriptible(&dossier.display().to_string())
}

/// Point d'entrée de la fenêtre d'installation.
pub fn lancer() {
    unsafe {
        // Sensibilité au DPI : sans appel explicite, Windows rend la fenêtre
        // dans une surface virtuelle puis l'ÉTIRE en bitmap sur les écrans
        // mis à l'échelle (125 %, 150 %…), d'où le rendu pixélisé. On demande
        // la conscience PER_MONITOR_V2 (-4) ; si l'API est absente
        // (Windows 8.1), `SetProcessDPIAware` prend le relais.
        if SetProcessDpiAwarenessContext(-4) == 0 {
            SetProcessDPIAware();
        }
        charger_police();
        let prefixe = crate::setup_core::installation_existante()
            .unwrap_or_else(crate::setup_core::prefixe_defaut);
        let existant = crate::setup_core::installation_existante().is_some();
        let utilisateur = est_inscriptible(&PathBuf::from(&prefixe));
        let empreinte = empreinte_payload();
        let statut = if empreinte.is_some() {
            "SHA-256 vérifié · prêt.".to_string()
        } else {
            "Payload absent · prêt.".to_string()
        };
        let etat = Box::new(Etat {
            vue: Vue::Bienvenue,
            atteintes: [false; 5],
            licence: lire_licence(),
            prefixe,
            utilisateur,
            existant,
            empreinte,
            statut,
            occupe: false,
            maximise: false,
            phase: 0,
            survol: -1,
            survol_titre: -1,
            version_installee: env!("CARGO_PKG_VERSION").to_string(),
            dpi: 0,
            icone: std::ptr::null_mut(),
        });
        ETAT = Box::into_raw(etat);
        // DPI lu avant la création de la fenêtre : `WM_GETMINMAXINFO` en a
        // besoin dès le premier redimensionnement.
        {
            let dc = GetDC(std::ptr::null_mut());
            let d = GetDeviceCaps(dc, LOGPIXELSY);
            ReleaseDC(std::ptr::null_mut(), dc);
            (*ETAT).dpi = if d <= 0 { 96 } else { d };
        }

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
        let titre_w = hex_w(&format!("Rose {} — Programme d'installation", env!("CARGO_PKG_VERSION")));
        // Fenêtre entièrement personnalisée : AUCUN cadre Windows. On utilise
        // WS_POPUP + WS_THICKFRAME (bords redimensionnables sans cadre) et on
        // dessine nous-mêmes la barre de titre, les boutons réduire / agrandir
        // / fermer et les bords. `WS_CAPTION` ou `WS_OVERLAPPEDWINDOW`
        // laissaient la barre Windows 11 (blanche, arrondie, avec le titre du
        // système) au-dessus de la nôtre.
        let style = WS_POPUP | WS_THICKFRAME | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX;
        // WS_EX_TOOLWINDOW : pas d'entrée « barre des tâches » redondante ;
        // WS_EX_APPWINDOW la rétablit, nécessaire pour retrouver une fenêtre
        // maximisée réduite. On garde APPWINDOW pour le comportement attendu.
        let ex_style = WS_EX_APPWINDOW;
        // 1120 × 780 = le gabarit de tracé (voir `dessiner`), ajusté au DPI
        // de l'écran ; le tracé suit ensuite la taille réelle de la fenêtre.
        let dpi_ecran = {
            let dc = GetDC(std::ptr::null_mut());
            let d = GetDeviceCaps(dc, LOGPIXELSY);
            ReleaseDC(std::ptr::null_mut(), dc);
            if d <= 0 { 96 } else { d }
        };
        let facteur = dpi_ecran as f64 / 96.0;
        // La grille est en pixels logiques (1120 × 780). Sur un écran
        // 1080p en 150 %, elle ferait 1680 × 1170 : plus haute que la zone
        // utile, et le pied de page sortait du cadre. On borne la fenêtre à
        // 92 % de l'écran en gardant le ratio du gabarit.
        let mut largeur = (1120.0 * facteur).round() as i32;
        let mut hauteur = (780.0 * facteur).round() as i32;
        let dc_ecran = GetDC(std::ptr::null_mut());
        let ecran_l = GetDeviceCaps(dc_ecran, 8); // HORZRES
        let ecran_h = GetDeviceCaps(dc_ecran, 10); // VERTRES
        ReleaseDC(std::ptr::null_mut(), dc_ecran);
        if ecran_l > 0 && ecran_h > 0 {
            let max_l = (ecran_l as f64 * 0.92).round() as i32;
            let max_h = (ecran_h as f64 * 0.92).round() as i32;
            if largeur > max_l {
                largeur = max_l;
                hauteur = ((largeur as f64) * 780.0 / 1120.0).round() as i32;
            }
            if hauteur > max_h {
                hauteur = max_h;
                largeur = ((hauteur as f64) * 1120.0 / 780.0).round() as i32;
            }
        }
        let x = ((ecran_l.max(1) - largeur) / 2).max(0);
        let y = ((ecran_h.max(1) - hauteur) / 2).max(0);
        let hwnd = CreateWindowExW(ex_style, classe.as_ptr(), titre_w.as_ptr(), style,
                                   x, y, largeur, hauteur, std::ptr::null_mut(), 0, inst, std::ptr::null_mut());
        if hwnd.is_null() {
            return;
        }
        HWND_FENETRE = hwnd;
        // Icône de la fenêtre ET de la barre des tâches. Le groupe d'icônes
        // embarqué est `RT_GROUP_ICON` (type 14) id 1 — PAS `RT_ICON`
        // (type 1), qui ne contient que les images individuelles. Charger le
        // type 1 par erreur laissait la fenêtre sans icône (icône Windows
        // par défaut dans la barre des tâches et dans Alt+Tab).
        let grande = LoadImageW(inst, 1 as *const u16, RT_GROUP_ICON as u32, 0, 0, LR_SHARED);
        if !grande.is_null() {
            (*ETAT).icone = grande as HICON;
            SendMessageW(hwnd, WM_SETICON, ICON_BIG, grande as LPARAM);
        }
        // petite icône : 16 px pour la barre de titre (sans cadre ici, mais
        // Windows l'utilise dans la barre des tâches et Alt+Tab)
        let petite = LoadImageW(inst, 1 as *const u16, RT_GROUP_ICON as u32,
                                GetSystemMetrics(SM_CXSMICON), GetSystemMetrics(SM_CYSMICON), LR_SHARED);
        if !petite.is_null() {
            SendMessageW(hwnd, WM_SETICON, ICON_SMALL, petite as LPARAM);
        }
        // Et l'icône de la CLASSE : c'est elle qui sert aux fenêtres créées
        // ensuite et dans la boîte de dialogue système.
        if !grande.is_null() {
            SetClassLongPtrW(hwnd, -14 /* GCLP_HICON */, grande as isize);
            SetClassLongPtrW(hwnd, -34 /* GCLP_HICONSM */, grande as isize);
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
    source_moteur()
}

/// Texte d'aide pour la console (utilisé par « rose update »).
pub fn aide() -> String {
    "Rose update : installe ou met à jour Rose (64 bits natif).\n\
     Sur Windows, l'installeur graphique « Rose-win-Setup.exe » offre la même chose\n\
     avec une fenêtre dédiée.".to_string()
}
