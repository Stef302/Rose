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
struct SIZE { cx: i32, cy: i32 }

// ── GDI+ : tracé ANTIALIASÉ ────────────────────────────────────────────
// GDI (Ellipse, RoundRect) ne sait pas anticréneler : les bords des cercles
// et des coins arrondis sortent escalier (mesuré : les pastilles des étapes
// paraissaient pixelisées). GDI+ le fait, et c'est la seule façon d'obtenir
// des cercles nets sans changer de framework.
//
// ⚠ GDI+ n'est utilisé QUE pour les FORMES. Le logo est dessiné en GDI pur
// (`AlphaBlend`) : la variante GDI+ (`GdipCreateBitmapFromScan0` avec un
// `scan0` pointant sur un `Vec` Rust) laisse GDI+ référencer — voire
// libérer — un tampon qui nous appartient, d'où une double libération et un
// CRASH de l'installeur (fenêtre ouverte avec ses seules BORDURES, sans
// intérieur : « l'effet transparent », mesuré).

/// `GdiplusStartupInput` (gdiplusflat.h).
///
/// ⚠ La structure RÉELLE a **CINQ** champs :
/// `GdiplusVersion`, `GdiplusStartupCallback`, `GdiplusStartupParameter`,
/// `SuppressBackgroundThread`, `SuppressExternalCodecs` — soit 32 octets.
/// La version précédente n'en avait que quatre : GDI+ lisait alors
/// `SuppressExternalCodecs` dans le REMPLISSAGE non initialisé de la
/// structure (4 octets de bourrage après le 4ᵉ champ). On initialise donc
/// tout à zéro, et on déclare le champ manquant.
#[repr(C)]
struct GdiplusStartupInput {
    version: u32,
    callback: *mut c_void,
    callback_data: *mut c_void,
    suppress_background_thread: i32,
    suppress_external_codecs: i32,
}

#[repr(C)]
struct GpRectF { x: f32, y: f32, largeur: f32, hauteur: f32 }

unsafe extern "system" {
    fn GdiplusStartup(jeton: *mut usize, entree: *const GdiplusStartupInput, sortie: *mut c_void) -> u32;
    fn GdipCreateFromHDC(hdc: *mut c_void, graphique: *mut *mut c_void) -> u32;
    fn GdipDeleteGraphics(graphique: *mut c_void) -> u32;
    fn GdipSetSmoothingMode(graphique: *mut c_void, mode: i32) -> u32;
    fn GdipCreateSolidFill(couleur: u32, pinceau: *mut *mut c_void) -> u32;
    fn GdipDeleteBrush(pinceau: *mut c_void) -> u32;
    fn GdipFillEllipse(graphique: *mut c_void, pinceau: *mut c_void,
                       x: f32, y: f32, largeur: f32, hauteur: f32) -> u32;
    fn GdipFillRectangle(graphique: *mut c_void, pinceau: *mut c_void,
                         x: f32, y: f32, largeur: f32, hauteur: f32) -> u32;
    fn GdipCreatePath(mode: i32, chemin: *mut *mut c_void) -> u32;
    fn GdipDeletePath(chemin: *mut c_void) -> u32;
    fn GdipAddPathArc(chemin: *mut c_void, x: f32, y: f32, largeur: f32, hauteur: f32,
                      debut: f32, balayage: f32) -> u32;
    fn GdipAddPathLine(chemin: *mut c_void, x1: f32, y1: f32, x2: f32, y2: f32) -> u32;
    fn GdipClosePathFigure(chemin: *mut c_void) -> u32;
    fn GdipFillPath(graphique: *mut c_void, pinceau: *mut c_void, chemin: *mut c_void) -> u32;
    // Coche ANTIALIASÉE : stylo GDI+ sur un chemin (UnitPixel = 2, LineCapRound = 2).
    fn GdipCreatePen1(couleur: u32, largeur: f32, unite: i32, stylo: *mut *mut c_void) -> u32;
    fn GdipSetPenLineCap197819(stylo: *mut c_void, debut: i32, fin: i32, ancrage: i32) -> u32;
    fn GdipDrawPath(graphique: *mut c_void, stylo: *mut c_void, chemin: *mut c_void) -> u32;
    fn GdipDeletePen(stylo: *mut c_void) -> u32;
}

/// Convertit un COLORREF GDI (`0x00BBGGRR`) en ARGB GDI+ (`0xAARRGGBB`).
///
/// ⚠⚠ INDISPENSABLE. Les deux API rangent les canaux dans l'ordre INVERSE :
/// passer un COLORREF tel quel à GDI+ échange rouge et bleu. Mesuré sur la
/// capture : le vert lime `0x9ACD32` s'affichait en CYAN (`0x32CD9A`), et le
/// rouge vif `0xE62F1E` en BLEU.
fn argb_depuis_colorref(couleur: u32) -> u32 {
    let r = couleur & 0xFF;
    let g = (couleur >> 8) & 0xFF;
    let b = (couleur >> 16) & 0xFF;
    0xFF00_0000 | (r << 16) | (g << 8) | b
}

/// `SmoothingModeAntiAlias` (4) : c'est lui qui anticrénèle les formes.
const SMOOTHING_ANTIALIAS: i32 = 4;
/// `FillModeAlternate` (0).
const FILL_MODE_ALTERNATE: i32 = 0;

/// Jeton GDI+ (0 tant que non initialisé).
static mut JETON_GDIPLUS: usize = 0;

/// Initialise GDI+ une seule fois.
unsafe fn demarrer_gdiplus() {
    if JETON_GDIPLUS != 0 {
        return;
    }
    let entree = GdiplusStartupInput {
        version: 1,
        callback: std::ptr::null_mut(),
        callback_data: std::ptr::null_mut(),
        suppress_background_thread: 0,
        suppress_external_codecs: 0,
    };
    let mut jeton = 0usize;
    if GdiplusStartup(&mut jeton, &entree, std::ptr::null_mut()) == 0 {
        JETON_GDIPLUS = jeton;
    }
}

/// Exécute `f` sur un contexte GDI+ anticrénelé associé à `dc`.
unsafe fn avec_gdiplus<F: FnOnce(*mut c_void)>(dc: HDC, f: F) {
    demarrer_gdiplus();
    if JETON_GDIPLUS == 0 {
        return; // GDI+ indisponible : on ne dessine rien de plus
    }
    let mut g: *mut c_void = std::ptr::null_mut();
    if GdipCreateFromHDC(dc as *mut c_void, &mut g) != 0 || g.is_null() {
        return;
    }
    GdipSetSmoothingMode(g, SMOOTHING_ANTIALIAS);
    f(g);
    GdipDeleteGraphics(g);
}

/// Cercle plein ANTIALIASÉ (pastilles des étapes, voyant de la barre d'état).
unsafe fn cercle_lisse(dc: HDC, cx: i32, cy: i32, rayon: f32, couleur: u32) -> bool {
    demarrer_gdiplus();
    if JETON_GDIPLUS == 0 {
        return false;
    }
    let mut fait = false;
    avec_gdiplus(dc, |g| {
        let mut pinceau: *mut c_void = std::ptr::null_mut();
        let argb = argb_depuis_colorref(couleur);
        if GdipCreateSolidFill(argb, &mut pinceau) == 0 && !pinceau.is_null() {
            let x = cx as f32 - rayon;
            let y = cy as f32 - rayon;
            if GdipFillEllipse(g, pinceau, x, y, rayon * 2.0, rayon * 2.0) == 0 {
                fait = true;
            }
            GdipDeleteBrush(pinceau);
        }
    });
    fait
}

// ⚠ `Copy` : `RECT` est une structure triviale de quatre entiers, et le code
// en copie constamment (`ps.rcPaint`, `zone = rc`). Sans `Copy`, Rust
// interdit la copie implicite et refuse de compiler dès qu'on relit `rc`
// après l'avoir affecté.
#[repr(C)]
#[derive(Clone, Copy)]
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
    fn GetTextExtentPoint32W(dc: HDC, t: *const u16, n: i32, taille: *mut SIZE) -> i32;
    fn GetGlyphIndicesW(dc: HDC, t: *const u16, n: i32, indices: *mut u16, drapeaux: u32) -> u32;
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
    fn SetStretchBltMode(dc: HDC, mode: i32) -> i32;
    fn SetBrushOrgEx(dc: HDC, x: i32, y: i32, precedent: *mut POINT) -> i32;
    fn SetWindowPos(h: HWND, apres: HWND, x: i32, y: i32, cx: i32, cy: i32, f: u32) -> i32;
    fn SetForegroundWindow(h: HWND) -> i32;
    fn GetLastError() -> u32;
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
const WM_ERASEBKGND: u32 = 0x0014;
const WM_CLOSE: u32 = 0x0010;
const WM_TIMER: u32 = 0x0113;
const WM_SIZE: u32 = 0x0005;
const WM_DPICHANGED: u32 = 0x02E0;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_SETCURSOR: u32 = 0x0020;
const WM_SETICON: u32 = 0x0080;
const WM_NCHITTEST: u32 = 0x0084;
const WM_KEYDOWN: u32 = 0x0100;
const VK_UP: u32 = 0x26;
const VK_DOWN: u32 = 0x28;
const VK_PRIOR: u32 = 0x21;   // Page précédente
const VK_NEXT: u32 = 0x22;    // Page suivante
const VK_HOME: u32 = 0x24;
const VK_END: u32 = 0x23;
const VK_RETURN: u32 = 0x0D;
const VK_ESCAPE: u32 = 0x1B;
const WM_APP: u32 = 0x8000;
const WM_APP_FIN: u32 = WM_APP + 1;
/// Fin d'une vérification ou d'un téléchargement de mise à jour lancé en
/// tâche de fond depuis le premier écran (ne fige pas l'interface).
const WM_APP_MAJ: u32 = WM_APP + 2;
const WM_GETMINMAXINFO: u32 = 0x0024;
const WM_SIZING: u32 = 0x0214;

const HTCAPTION: isize = 2;
const HTRIGHT: isize = 11;
const HTBOTTOM: isize = 15;
const HTBOTTOMRIGHT: isize = 17;
const WS_OVERLAPPEDWINDOW: u32 = 0x00CF0000;
const WS_POPUP: u32 = 0x80000000;
const WS_CLIPCHILDREN: u32 = 0x02000000;
const WS_THICKFRAME: u32 = 0x00040000;
const WS_SYSMENU: u32 = 0x00080000;
const WS_MINIMIZEBOX: u32 = 0x00020000;
const WS_MAXIMIZEBOX: u32 = 0x00010000;
const SW_SHOW: i32 = 5;
const SW_SHOWNORMAL: i32 = 1;
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
// Taille d'une grande icône (barre des tâches / Alt+Tab) : 32 px à 96 DPI.
const SM_CXICON: i32 = 11;
const SM_CYICON: i32 = 12;
const SM_CXSMICON: i32 = 49;
const SM_CYSMICON: i32 = 50;
const WS_EX_APPWINDOW: u32 = 0x00040000;
const WS_EX_TOOLWINDOW: u32 = 0x00000080;
// ⚠ `CS_HREDRAW` (0x0002) et `CS_VREDRAW` (0x0001) ne sont PAS déclarées :
// ces styles de CLASSE forcent Windows à invalider toute la fenêtre à chaque
// changement de taille, ce qui annule le repeint ciblé et provoque un
// clignotement. `WM_SIZE` gère déjà l'invalidation.
const TRANSPARENT: i32 = 1;
const DT_WORDBREAK: u32 = 0x0010;
const DT_CENTER: u32 = 0x0001;
const DT_VCENTER: u32 = 0x0004;
const DT_SINGLELINE: u32 = 0x0020;
const DT_RIGHT: u32 = 0x0002;
const DT_LEFT: u32 = 0x0000;
// DT_TOP (0x0000) explicite : `DrawText` centre verticalement le texte d'une
// seule ligne dans son rectangle ; pour un texte ancré en haut d'une carte
// haute, il faut le dire.
const DT_TOP: u32 = 0x0000;
// DT_END_ELLIPSIS (0x8000) : un texte trop long pour son rectangle se
// termine par « … » au lieu d'être TRONQUÉ net — et surtout au lieu de
// déborder sur les autres éléments.
const DT_END_ELLIPSIS: u32 = 0x0000_8000;
// DT_PATH_ELLIPSIS (0x4000) : ellipse au milieu d'un CHEMIN, pour garder le
// nom du dossier (la fin d'un chemin est plus informative que son début).
const DT_PATH_ELLIPSIS: u32 = 0x0000_4000;
const IDC_ARROW: u16 = 32512;
const IDC_HAND: u16 = 32649;
const LOGPIXELSY: i32 = 90;
/// Qualité de police : `ANTIALIASED_QUALITY` = 4. Le tracé passait par la
/// qualité 5 (CLEARTYPE) mais sur un DC mémoire, où ClearType n'est pas
/// appliqué : les textes sortaient crénelés. L'anti-aliasing gris lisse
/// tous les DC, y compris la surface de double-bufferisation.
const QUALITE_POLICE: i32 = 4;
const SRCCOPY: u32 = 0x00CC0020;
// Mode d'étirement pour `AlphaBlend` et `BitBlt`.
//
// ⚠ L'ancien commentaire accusait `COLORONCOLOR` et préconisait `HALFTONE`
// pour le logo. C'était INEXACT pour `AlphaBlend` : MSDN documente que le
// mode est automatiquement converti en COLORONCOLOR pour cette fonction.
// La réduction du logo (276×343 → ~110×137) est donc faite en Rust par
// moyenne de surface (`reduire_rgba`), puis collée à taille ÉGALE. Le
// double-buffer, lui, copie à taille égale (`BitBlt`/`SRCCOPY`) : aucun
// étirement n'intervient. Constante conservée pour compatibilité.
const HALFTONE: i32 = 4;
const TIMER_PROGRESSION: usize = 1;
const ICON_BIG: usize = 1;
const ICON_SMALL: usize = 0;
const LR_SHARED: u32 = 0x8000;
// LR_DEFAULTSIZE = 0x0040 : avec cx = cy = 0, Windows prend la taille
// système par défaut au lieu de renvoyer NULL.
const LR_DEFAULTSIZE: u32 = 0x0040;
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
// ROUGE VIF : la marque Rose, l'action principale (bouton « Mettre à jour »).
// `ROUGE_CLAIR` sert au survol et aux accents ; `ROUGE_SOMBRE` au contour des
// étapes en cours. `ROUGE` reste l'alias historique, pour ne pas retoucher
// les vues qui ne concernent pas la mise à jour.
const ROUGE_VIF: u32 = couleur(0xFF2D1A);
const ROUGE: u32 = ROUGE_VIF;
const ROUGE_CLAIR: u32 = couleur(0xFF5A3C);
const ROUGE_SOMBRE: u32 = couleur(0xA3220F);
// VERT LIME : couleur d'état unique (succès, badges, boutons secondaires).
// L'ancien `VERT` (0x2CCB2C) était un vert moyen, pas le lime de la marque.
const VERT: u32 = couleur(0x9ACD32);
const TEXTE: u32 = couleur(0xE8E2E2);
const TEXTE_DOUX: u32 = couleur(0xA89894);
const TEXTE_FAIBLE: u32 = couleur(0x6E605C);
const FOND_PUCE: u32 = couleur(0x2A1A18);
const BLANC: u32 = couleur(0xFFFFFF);
// Boutons secondaires (Annuler, badges du panneau latéral) : VERT LIME.
// `VERT_LIME` = 0x9ACD32 (yellowgreen/lime-green), lisible sur le fond sombre
// et cohérent avec l'accent vert de Rose. Le survol éclaircit vers
// `VERT_LIME_CLAIR`, le repos reste `VERT_LIME`.
const VERT_LIME: u32 = couleur(0x9ACD32);
const VERT_LIME_CLAIR: u32 = couleur(0xB8E05A);
// Thème « rose update » : le VERT LIME VIF (0x9CFF00) est la couleur de
// PROGRESSION et de SUCCÈS de la fenêtre de mise à jour. Il reste local à
// cette fenêtre : le vert lime historique des autres boutons n'est pas changé.
const VERT_LIME_VIF: u32 = couleur(0x9CFF00);
const VERT_LIME_VIF_CLAIR: u32 = couleur(0xC4FF6A);
const NOIR_DOUX: u32 = couleur(0x14100F);
const SURVOL_SOMBRE: u32 = couleur(0xB8E05A);
const REPOS_SOMBRE: u32 = couleur(0x9ACD32);

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
    /// ⚠ Outil PATH : diagnostic et réparation par plusieurs méthodes.
    /// C'est la réponse à « ça échoue à écrire le PATH » — notamment sur
    /// les machines d'entreprise où la clé est verrouillée par une GPO.
    Path,
    /// ⚠ PREMIER ÉCRAN : choix du mode de mise à jour, AVANT toute
    /// installation ou sélection de version. Ajouté EN FIN d'énumération
    /// pour ne pas décaler les valeurs numériques des cinq étapes (le
    /// panneau latéral compare `etat.vue as usize` à l'index d'étape).
    MisesAJour,
    /// ⚠ SÉLECTEUR DE VERSIONS (LAUNCH2). Alimenté par
    /// `setup_core::launcher_catalogue()`, aucune version en dur. Ajouté
    /// EN FIN, après `MisesAJour`, pour la même raison : les cinq étapes
    /// gardent leurs index 0..4 dans le panneau latéral.
    Versions,
}

/// Action des deux boutons du pied de page.
#[derive(Clone, Copy, PartialEq)]
enum Action {
    Retour,
    Avancer,
    Annuler,
    Fermer,
    Desinstaller,
    /// Ouvre l'outil PATH.
    OutilPath,
    /// Applique une méthode de réparation du PATH (l'index est porté par
    /// l'état, pas par l'action).
    ReparerPath,
    /// Revient à l'écran précédent (depuis l'outil PATH).
    FermerOutil,
    /// Enregistre le mode de mise à jour choisi et quitte le premier écran.
    Continuer,
    /// Ouvre le sélecteur de versions (bouton du premier écran / Options).
    OuvrirVersions,
    /// Quitte le sélecteur pour revenir à l'écran d'origine.
    RetourVersions,
    /// Recharge le catalogue (appel réseau, bouton « Rafraîchir »).
    RafraichirVersions,
    /// Installe/télécharge la version sélectionnée (index porté par l'état).
    InstallerVersion,
    /// Bascule sur une version déjà présente (aucun réseau).
    UtiliserVersion,
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
    /// Diagnostic PATH courant (None = pas encore calculé).
    diagnostic: Option<crate::setup_core::DiagnosticPath>,
    /// Méthode de réparation choisie (index dans METHODES).
    methode: usize,
    /// Journal de la dernière réparation PATH.
    journal_path: Vec<String>,
    /// Mode de mise à jour choisi sur le premier écran (relu de la config
    /// au démarrage, écrit par « Continuer »).
    mode_maj: crate::velopack::ModeMiseAJour,
    /// Catalogue du sélecteur de versions. `None` = jamais chargé ; il est
    /// rempli à l'entrée de `Vue::Versions` par `launcher_catalogue()`.
    catalogue: Option<crate::velopack::CatalogueVersions>,
    /// Index de la version sélectionnée dans `catalogue.versions`.
    version_selection: usize,
    /// Première ligne visible (défilement de la liste).
    version_depart: usize,
    /// Nombre de lignes affichables, posé par le tracé et relu au clavier.
    version_visible: usize,
    /// Écran vers lequel « Retour » ramène depuis le sélecteur.
    version_retour: Vue,
    /// Couleur du message d'action : `true` = échec ((message du backend)).
    version_statut_erreur: bool,
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

/// Écrit le moteur embarqué (`RT_RCDATA 3`) dans un fichier temporaire et
/// rend son chemin, pour que `setup_core::installer` le copie dans `bin\`.
///
/// C'est ce qui fait de `Rose-win-Setup.exe` un installeur « un seul
/// fichier ». Sans cette ressource, `source_moteur()` ne trouvait rien et
/// l'installeur se copiait lui-même sous `rose.exe` — boucle de lancement.
/// Extrait une ressource `RT_RCDATA` dans `%TEMP%\rose-setup-payload\` et
/// rend son chemin.
///
/// `nom_fichier` = nom que portera le fichier temporaire (sans importance,
/// `installer` le renomme à la copie).
#[cfg(windows)]
fn extraire_rcdata(id: u16, nom_fichier: &str) -> Option<std::path::PathBuf> {
    let octets = unsafe { ressource(id, 10) }?; // RT_RCDATA = 10
    if octets.len() < 64 {
        return None;
    }
    let dossier = std::env::temp_dir().join("rose-setup-payload");
    let _ = std::fs::create_dir_all(&dossier);
    let chemin = dossier.join(format!("{}-{nom_fichier}", std::process::id()));
    std::fs::write(&chemin, octets).ok()?;
    Some(chemin)
}

#[cfg(not(windows))]
fn extraire_rcdata(_id: u16, _nom_fichier: &str) -> Option<std::path::PathBuf> {
    None
}

/// Moteur embarqué (`RT_RCDATA 3`), validé comme application CONSOLE.
#[cfg(windows)]
pub fn moteur_embarque() -> Option<std::path::PathBuf> {
    let octets = unsafe { ressource(3, 10) }?; // RT_RCDATA = 10
    if octets.len() < 1024 || &octets[0..2] != b"MZ" {
        return None; // ce n'est pas un PE : on refuse
    }
    // ⚠ Le sous-système doit être CONSOLE (3). Un moteur en GUI (2) est
    // l'installeur : l'installer sous `rose.exe` provoquerait une boucle de
    // lancement (fenêtres Windows Terminal en rafale).
    let pe = u32::from_le_bytes([octets[0x3C], octets[0x3D], octets[0x3E], octets[0x3F]]) as usize;
    if pe + 0x5A > octets.len() || &octets[pe..pe + 4] != b"PE\0\0" {
        return None;
    }
    let o = pe + 24 + 68;
    if o + 2 > octets.len() {
        return None;
    }
    let sous_systeme = u16::from_le_bytes([octets[o], octets[o + 1]]);
    if sous_systeme != 3 {
        // GUI ou autre : ce n'est pas le moteur console.
        return None;
    }
    let dossier = std::env::temp_dir().join("rose-setup-payload");
    let _ = std::fs::create_dir_all(&dossier);
    let chemin = dossier.join(format!("rose-{}.exe", std::process::id()));
    std::fs::write(&chemin, octets).ok()?;
    Some(chemin)
}

#[cfg(not(windows))]
pub fn moteur_embarque() -> Option<std::path::PathBuf> {
    None
}

/// Fichiers compagnons embarqués : (id RT_RCDATA, nom final dans `bin\`).
///
/// ⚠ Ils étaient copiés depuis le dossier de la SOURCE — or la source est le
/// moteur extrait dans `%TEMP%`, qui ne contient que `rose.exe`. Mesuré :
/// l'installation ne posait QUE `rose.exe`, sans helper AOT, sans shell, sans
/// icône de fichier.
pub const COMPAGNONS: [(u16, &str); 4] = [
    (5, "rose-shell.exe"),
    (6, "rose.ico"),
    (7, "rose_app.ico"),
    // ⚠⚠ AJOUT DU 17/11 : `rose_shell.ico` (terminal + rose) pour le profil
    // Windows Terminal. Sans lui, le profil affichait la rose de l'app.
    (8, "rose_shell.ico"),
];

/// Extrait un compagnon embarqué (`None` s'il n'est pas dans les ressources).
pub fn compagnon_embarque(id: u16, nom: &str) -> Option<std::path::PathBuf> {
    let p = extraire_rcdata(id, nom)?;
    if p.is_file() { Some(p) } else { None }
}

/// Charge la police Kare embarquée dans la session GDI du processus.
///
/// ⚠ Le résultat d'`AddFontMemResourceEx` n'était PAS vérifié : quand il
/// échouait (ressource absente, TTF illisible), les titres sortaient dans la
/// police par défaut du système sans que personne ne le remarque. On
/// journalise l'échec ; `kare_disponible()` fait ensuite le vrai contrôle.
unsafe fn charger_police() {
    let Some(ttf) = ressource(2, 8) else {
        eprintln!("rose-setup : police Kare absente des ressources (RT_FONT 2)");
        return;
    };
    let mut n = 0u32;
    let r = AddFontMemResourceEx(ttf.as_ptr() as *const c_void, ttf.len() as u32, std::ptr::null_mut(), &mut n);
    if r == 0 || n == 0 {
        eprintln!("rose-setup : AddFontMemResourceEx a échoué ({n} police(s)) — les titres seront en Segoe UI");
    }
}

/// Bornes du contenu visible d'une image RGBA (alpha non nul).
///
/// Renvoie (x0, y0, x1, y1) — rectangle semi-ouvert. Si rien n'est
/// visible, renvoie (0, 0, 0, 0).
fn bornes_opaques(largeur: usize, hauteur: usize, pixels: &[u8]) -> (usize, usize, usize, usize) {
    let mut x0 = largeur;
    let mut y0 = hauteur;
    let mut x1 = 0usize;
    let mut y1 = 0usize;
    for y in 0..hauteur {
        for x in 0..largeur {
            if pixels[(y * largeur + x) * 4 + 3] > 4 {
                if x < x0 { x0 = x; }
                if x + 1 > x1 { x1 = x + 1; }
                if y < y0 { y0 = y; }
                if y + 1 > y1 { y1 = y + 1; }
            }
        }
    }
    if x1 <= x0 || y1 <= y0 {
        return (0, 0, 0, 0);
    }
    (x0, y0, x1, y1)
}

/// Réduction RGBA par MOYENNE DE SURFACE (area averaging), en pur Rust.
///
/// ⚠ Indispensable : `AlphaBlend` force le mode COLORONCOLOR quel que soit
/// `SetStretchBltMode` (MSDN : « the iStretchMode value is automatically
/// converted to COLORONCOLOR »). Un appel
/// `AlphaBlend(… 276×343 → ~110×137 …)` SUPPRIME donc des lignes/colonnes
/// entières : la tige du bas peut disparaître et le logo paraît pixélisé.
/// On réduit nous-mêmes l'image (chaque pixel source contribue), puis on
/// appelle `AlphaBlend` à taille ÉGALE : aucune interpolation matérielle.
///
/// `pixels` est du RGBA 8 bits NON prémultiplié ; la sortie est du RGBA
/// PRÉMULTIPLIÉ, prêt pour `AC_SRC_ALPHA`.
fn reduire_rgba(pixels: &[u8], largeur: usize, hauteur: usize,
                sx0: usize, sy0: usize, sw: usize, sh: usize,
                cible_l: usize, cible_h: usize) -> Option<Vec<u8>> {
    if cible_l == 0 || cible_h == 0 || sw == 0 || sh == 0 {
        return None;
    }
    if sx0.checked_add(sw)? > largeur || sy0.checked_add(sh)? > hauteur {
        return None;
    }
    if pixels.len() < largeur * hauteur * 4 {
        return None;
    }
    let mut sortie = vec![0u8; cible_l * cible_h * 4];
    for oy in 0..cible_h {
        let y0_abs = sy0 + oy * sh / cible_h;
        let y1_abs = sy0 + ((oy + 1) * sh / cible_h).max(oy * sh / cible_h + 1);
        for ox in 0..cible_l {
            let x0_abs = sx0 + ox * sw / cible_l;
            let x1_abs = sx0 + ((ox + 1) * sw / cible_l).max(ox * sw / cible_l + 1);
            let mut sr = 0u64;
            let mut sg = 0u64;
            let mut sb = 0u64;
            let mut sa = 0u64;
            for py in y0_abs..y1_abs {
                for px in x0_abs..x1_abs {
                    let k = (py * largeur + px) * 4;
                    let a = pixels[k + 3] as u64;
                    sr += pixels[k] as u64 * a;
                    sg += pixels[k + 1] as u64 * a;
                    sb += pixels[k + 2] as u64 * a;
                    sa += a;
                }
            }
            let n = ((x1_abs - x0_abs) * (y1_abs - y0_abs)) as u64;
            let div_premul = n * 255; // moyenne des canaux PRÉMULTIPLIÉS
            let o = (oy * cible_l + ox) * 4;
            sortie[o] = (sr / div_premul) as u8;     // R × alpha
            sortie[o + 1] = (sg / div_premul) as u8; // G × alpha
            sortie[o + 2] = (sb / div_premul) as u8; // B × alpha
            sortie[o + 3] = (sa / n) as u8;          // alpha moyen
        }
    }
    Some(sortie)
}

/// Dessine la rose embarquée, cadrée SANS COUPURE et avec transparence.
///
/// `largeur_max × hauteur_max` est la boîte disponible ; le ratio du contenu
/// utile est conservé et le logo est centré dans cette boîte.
///
/// Chemin GDI pur (`CreateDIBSection` + `AlphaBlend`), volontairement :
/// la variante GDI+ (`GdipCreateBitmapFromScan0` avec un `scan0` pointant
/// sur un `Vec` Rust) laisse GDI+ référencer — voire libérer — un tampon qui
/// nous appartient : double libération à la destruction du `Vec`, donc
/// CRASH de l'installeur. Symptôme mesuré : la fenêtre s'ouvrait avec ses
/// seules BORDURES, sans intérieur (le processus mourait pendant le
/// dessin). GDI+ n'est plus utilisé que pour les FORMES (cercles, coins
/// arrondis, coches), où aucun tampon n'est partagé.
unsafe fn dessiner_rose(dc: HDC, x: i32, y: i32, largeur_max: i32, hauteur_max: i32) -> bool {
    let Some(png) = ressource(1, 10) else { return false };
    let Some((largeur, hauteur, pixels)) = crate::png::decoder_png(png) else { return false };

    let (sx0, sy0, sx1, sy1) = bornes_opaques(largeur, hauteur, &pixels);
    let sw = sx1.saturating_sub(sx0);
    let sh = sy1.saturating_sub(sy0);
    if sw == 0 || sh == 0 || largeur_max <= 0 || hauteur_max <= 0 {
        return false;
    }
    // ── Cadrage SANS COUPURE ───────────────────────────────────────────────
    // Le PNG embarqué fait 276 × 343 et son contenu utile 264 × 337 (mesuré
    // sur l'alpha : bornes x 6..270, y 6..343 — la tige va au dernier pixel).
    // On calcule l'échelle pour que CE CONTENU ENTIER tienne dans la boîte,
    // puis on centre : aucune bordure n'est rognée, ratio conservé.
    let facteur = (largeur_max as f64 / sw as f64).min(hauteur_max as f64 / sh as f64);
    let dw = ((sw as f64 * facteur).round() as i32).max(1).min(largeur_max);
    let dh = ((sh as f64 * facteur).round() as i32).max(1).min(hauteur_max);
    let dx = x + (largeur_max - dw) / 2;
    let dy = y + (hauteur_max - dh) / 2;
    // Réduction par moyenne de surface AVANT AlphaBlend (voir `reduire_rgba`).
    // Le rectangle SOURCE couvre tout le contenu utile, y compris le bas.
    let Some(rgba) = reduire_rgba(&pixels, largeur, hauteur,
                                  sx0, sy0, sw, sh, dw as usize, dh as usize) else {
        return false;
    };

    // La DIB a EXACTEMENT la taille du logo réduit : AlphaBlend travaillera
    // à 1:1 et ne supprimera aucune ligne (contrairement à une réduction
    // matérielle en COLORONCOLOR).
    let info = BITMAPINFO {
        header: BITMAPINFOHEADER {
            bi_size: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            bi_width: dw,
            bi_height: -dh,
            bi_planes: 1,
            bi_bit_count: 32,
            bi_compression: 0,
            bi_size_image: (dw as usize * dh as usize * 4) as u32,
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
    // `rgba` est déjà PRÉMULTIPLIÉ (voir `reduire_rgba`) : il ne reste qu'à
    // passer de l'ordre RGBA au BGRA attendu par `AC_SRC_ALPHA` (alpha_format=1).
    let cible = std::slice::from_raw_parts_mut(bits as *mut u8, dw as usize * dh as usize * 4);
    for k in 0..dw as usize * dh as usize {
        cible[k * 4] = rgba[k * 4 + 2];     // B
        cible[k * 4 + 1] = rgba[k * 4 + 1]; // G
        cible[k * 4 + 2] = rgba[k * 4];     // R
        cible[k * 4 + 3] = rgba[k * 4 + 3]; // A
    }
    let memoire = CreateCompatibleDC(dc);
    let ancien = SelectObject(memoire, bitmap as *mut c_void);
    // ⚠ Pas de `SetStretchBltMode(HALFTONE)` ici : MSDN documente qu'`AlphaBlend`
    // convertit automatiquement le mode en COLORONCOLOR. La qualité vient de
    // NOTRE réduction (moyenne de surface) et du 1:1 ci-dessous.
    let melange = BLENDFUNCTION {
        blend_op: 0,
        blend_flags: 0,
        source_constant_alpha: 255,
        alpha_format: 1,
    };
    // Rectangle SOURCE = l'image réduite entière ; rectangle DEST = même taille.
    AlphaBlend(dc, dx, dy, dw, dh, memoire, 0, 0, dw, dh, melange);
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
///
/// ⚠ Le plancher était `9` px : à 9 pt sur une fenêtre réduite, la police
/// tombait à 9 px, soit 7 px de glyphes — ILLISIBLE. On garde au moins
/// 13 px (≈ 10 pt à 96 DPI), ce qui reste lisible même en 736 × 512.
fn points_ecrits(dpi: i32, points: i32, echelle: f64) -> i32 {
    let base = (points * dpi) / 72;
    let e = if echelle.is_finite() && echelle > 0.0 { echelle } else { 1.0 };
    ((base as f64 * e).round() as i32).max(13)
}

/// Géométrie verticale du bloc « logo / titre / sous-titre / étapes » du
/// panneau latéral. Fonction PURE : aucun appel GDI, donc testable sur toutes
/// les combinaisons DPI × taille de fenêtre (y compris sous Wine).
///
/// ⚠ CAUSE DU TEXTE SUPERPOSÉ (capture utilisateur). L'ancien code posait le
/// sous-titre à `y_marque + s(56)`, un décalage FIXE, alors que le bas réel du
/// titre est `y_marque + s(34) + h_marque / 2` et que `h_marque` suit le DPI
/// (76 px à 96 DPI, 100 px à 144 DPI) : le sous-titre recouvrait « ROSE » de
/// 8 à 55 px selon la fenêtre. On le positionne désormais DEPUIS LE BAS RÉEL
/// DU TITRE, avec une marge explicite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GeometrieMarque {
    plancher: i32,          // rien du panneau ne descend sous cette ligne
    largeur_logo: i32,      // largeur de la boîte du logo
    hauteur_logo: i32,      // hauteur de la boîte (jamais > 343 = hauteur PNG)
    place_logo: i32,        // place réservée au logo ( borne haute )
    haut_titre: i32,
    bas_titre: i32,
    haut_sous_titre: i32,
    bas_sous_titre: i32,
    y_etapes: i32,          // première étape, TOUJOURS sous le sous-titre
}

fn geometrie_marque(ht: i32, dpi: i32, echelle: f64) -> GeometrieMarque {
    let e = if echelle.is_finite() && echelle > 0.0 { echelle } else { 1.0 };
    let s = |v: i32| -> i32 { (v as f64 * e).round() as i32 };

    // Cartes du bas : mêmes réservations que dans `dessiner_decore`.
    let bas = ht - s(30) - s(18);
    let carte2_top = bas - s(38);
    let carte1_top = carte2_top - s(46);
    let plancher = carte1_top - s(16);

    // Hauteur RÉELLE de la cellule du titre : la police suit le DPI et
    // l'échelle, `s(72)` seul rognait le bas des glyphes Kare.
    let marque_px = points_ecrits(dpi, 36, e);
    let h_marque = s(72).max(marque_px + s(28));
    // Marge logo → titre : assez grande pour que le haut du titre (Kare a une
    // montée de 979/1000) ne remonte jamais sur la tige de la rose.
    let marge_marque = s(26).max(h_marque / 2 - s(34) + s(4));

    // Le PNG natif fait 276 × 343 : la hauteur souhaitée suit ce ratio, jamais
    // une constante. `dessiner_rose` recadre ensuite le contenu utile
    // (`bornes_opaques`) SANS couper de pixel et en conservant le ratio.
    let largeur_logo = s(110);
    let hauteur_voulue = ((largeur_logo as f64 * 343.0 / 276.0).round() as i32).max(1);
    // Place réservée au logo : tout ce qui suit `y_marque` est mesuré sur la
    // hauteur RÉELLE du titre, plus le sous-titre et la marge avant les étapes.
    // Réserver seulement `s(120)` compressait la boîte sur fenêtre basse.
    let apres_marque = s(34) + h_marque / 2 + s(10) + s(56) + s(10);
    let place_logo = (plancher - s(78) - marge_marque - apres_marque).max(0);
    let hauteur_logo = hauteur_voulue.min(place_logo).min(343);

    let y_marque = s(78) + hauteur_logo + marge_marque;
    let haut_titre = y_marque + s(34) - h_marque / 2;
    let bas_titre = haut_titre + h_marque;
    let haut_sous_titre = bas_titre + s(10); // marge réelle, plus de s(56) fixe
    let bas_sous_titre = haut_sous_titre + s(56);
    let y_etapes = bas_sous_titre + s(10); // les étapes ne remontent PAS

    GeometrieMarque {
        plancher, largeur_logo, hauteur_logo, place_logo,
        haut_titre, bas_titre, haut_sous_titre, bas_sous_titre, y_etapes,
    }
}

/// Dessine un texte : `famille` = "Kare" (titre) ou "Segoe UI" (courant).
unsafe fn texte(dc: HDC, dpi: i32, famille: &str, s: &str, r: &mut RECT, flags: u32,
                couleur: u32, points: i32, gras: bool) {
    texte_echelle(dc, dpi, famille, s, r, flags, couleur, points, gras, 1.0)
}

/// Comme `texte`, avec une police mise à l'échelle de la fenêtre.
///
/// ⚠ `CreateFontW` peut ÉCHOUER (quota GDI, police inconnue, taille
/// aberrante) : il rend alors NULL, et `SelectObject(dc, NULL)` renvoie NULL.
/// L'ancienne version appelait `DrawTextW` quand même, avec la police de
/// stock de la fenêtre — dont la hauteur par défaut est SANS RAPPORT avec la
/// zone, d'où des textes énormes qui recouvraient tout le tracé. On
/// court-circuite : un texte non dessiné est moins grave qu'une fenêtre
/// couverte de glyphes géants.
#[allow(clippy::too_many_arguments)]
unsafe fn texte_echelle(dc: HDC, dpi: i32, famille: &str, s: &str, r: &mut RECT, flags: u32,
                        couleur: u32, points: i32, gras: bool, echelle: f64) {
    if s.is_empty() || r.right <= r.left || r.bottom <= r.top {
        return; // rectangle inversé : `DrawText` ne dessinerait rien
    }
    let taille = points_ecrits(dpi, points, echelle);
    if taille <= 0 {
        return;
    }
    let Some(police) = creer_police(famille, taille, gras, s) else { return };
    SetBkMode(dc, TRANSPARENT);
    SetTextColor(dc, couleur);
    let ancien = SelectObject(dc, police as *mut c_void);
    let v = hex_w(s);
    DrawTextW(dc, v.as_ptr(), (v.len() - 1) as i32, r, flags);
    SelectObject(dc, ancien);
    DeleteObject(police as *mut c_void);
}

/// Crée une police GDI, ou rend `None` si GDI refuse.
///
/// ⚠⚠ Un glyphe ABSENT de la police ne produit pas d'erreur : Windows dessine
/// un carré vide (le glyphe « .notdef »). Mesuré : `⚿` dans « MODE
/// UTILISATEUR » et le `✓` des cartes « Options »/« Terminé » sortaient en
/// carrés, ce qui donnait une interface « cassée ». `GDI_ERROR` (0xFFFFFFFF)
/// est rendu par `GetGlyphIndicesW` pour un caractère non couvert : on le
/// teste ici et on retombe sur Segoe UI, qui les a tous.
unsafe fn creer_police(famille: &str, taille_px: i32, gras: bool, texte: &str) -> Option<HFONT> {
    let poids = if gras { 700 } else { 400 };
    let mut face = famille;
    // Kare n'est utilisable que si elle est RÉELLEMENT présente ; sinon
    // `CreateFontW("Kare", …)` rendrait la police système, et les titres
    // « ROSE » / « BIENVENUE » sortiraient en Segoe UI sans prévenir.
    if famille == "Kare" && !kare_disponible() {
        face = "Segoe UI";
    } else if famille != "Segoe UI" && !famille_couvre(famille, texte) {
        face = "Segoe UI";
    }
    let nom = hex_w(face);
    let police = CreateFontW(-taille_px, 0, 0, 0, poids, 0, 0, 0, 1, 0, 0, QUALITE_POLICE, 0, nom.as_ptr());
    if police.is_null() { None } else { Some(police) }
}

/// Vrai si `famille` possède tous les glyphes de `texte`.
unsafe fn famille_couvre(famille: &str, texte: &str) -> bool {
    let nom = hex_w(famille);
    let police = CreateFontW(-16, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, QUALITE_POLICE, 0, nom.as_ptr());
    if police.is_null() {
        return false;
    }
    let dc = CreateCompatibleDC(std::ptr::null_mut());
    if dc.is_null() {
        DeleteObject(police as *mut c_void);
        return false;
    }
    let ancien = SelectObject(dc, police as *mut c_void);
    let mut couvre = true;
    for c in texte.chars() {
        // Unité UTF-16 : les caractères hors BMP ne sont de toute façon pas
        // dans Kare (elle s'arrête au latin accentué).
        let mut cp = [c as u16];
        let mut idx = [0u16];
        let n = GetGlyphIndicesW(dc, cp.as_mut_ptr(), 1, idx.as_mut_ptr(), 0);
        if n == u32::MAX || idx[0] == 0xFFFF {
            couvre = false;
            break;
        }
    }
    SelectObject(dc, ancien);
    DeleteDC(dc);
    DeleteObject(police as *mut c_void);
    couvre
}

/// Vrai si la police Kare est réellement disponible dans cette session GDI.
///
/// `AddFontMemResourceEx` peut ÉCHOUER (ressource absente, TTF refusé) :
/// `CreateFontW("Kare", …)` rend alors la police système et les titres
/// sortent en Segoe UI sans que personne ne le remarque. On vérifie donc au
/// premier besoin, une seule fois, en demandant à GDI les glyphes réels.
unsafe fn kare_disponible() -> bool {
    static mut VERDICT: i32 = 0; // 0 = inconnu, 1 = oui, 2 = non
    match VERDICT {
        1 => return true,
        2 => return false,
        _ => {}
    }
    let nom = hex_w("Kare");
    let police = CreateFontW(-16, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, QUALITE_POLICE, 0, nom.as_ptr());
    let dispo = if police.is_null() {
        false
    } else {
        let dc = CreateCompatibleDC(std::ptr::null_mut());
        if dc.is_null() {
            DeleteObject(police as *mut c_void);
            false
        } else {
            let ancien = SelectObject(dc, police as *mut c_void);
            let mut cp = ['R' as u16];
            let mut idx = [0u16];
            let n = GetGlyphIndicesW(dc, cp.as_mut_ptr(), 1, idx.as_mut_ptr(), 0);
            SelectObject(dc, ancien);
            DeleteDC(dc);
            DeleteObject(police as *mut c_void);
            // GDI_ERROR = 0xFFFFFFFF quand la police n'existe pas ; le glyphe
            // « R » doit exister (indice non nul, pas 0xFFFF).
            n != u32::MAX && idx[0] != 0xFFFF && idx[0] != 0
        }
    };
    VERDICT = if dispo { 1 } else { 2 };
    dispo
}

/// Largeur en pixels d'un texte dans une police donnée (mesure GDI réelle).
///
/// Sert au découpage des textes (`crate::paint::lignes_ajustees`) : couper au
/// nombre de caractères ne donne jamais une mise en page propre avec une
/// police proportionnelle.
unsafe fn mesurer_texte(dc: HDC, famille: &str, taille_px: i32, gras: bool, s: &str) -> i32 {
    let Some(police) = creer_police(famille, taille_px, gras, s) else { return s.chars().count() as i32 * 8 };
    let ancien = SelectObject(dc, police as *mut c_void);
    let v = hex_w(s);
    let mut taille = std::mem::zeroed::<SIZE>();
    let ok = GetTextExtentPoint32W(dc, v.as_ptr(), (v.len() - 1) as i32, &mut taille);
    SelectObject(dc, ancien);
    DeleteObject(police as *mut c_void);
    if ok != 0 { taille.cx.max(0) } else { s.chars().count() as i32 * 8 }
}

unsafe fn remplir(dc: HDC, r: &RECT, couleur: u32) {
    let b = CreateSolidBrush(couleur);
    FillRect(dc, r, b);
    DeleteObject(b as *mut c_void);
}

unsafe fn arrondi(dc: HDC, r: &RECT, rayon: i32, couleur: u32) {
    // Tracé ANTIALIASÉ (GDI+) : les coins arrondis des boutons et des cartes
    // sortaient crénelés en GDI pur (`RoundRect`).
    //
    // ⚠ Le repli GDI ne doit JAMAIS être sauté : `arrondi_lisse` rend `false`
    // quand GDI+ n'est pas initialisé (DLL absente, `GdiplusStartup` refusé)
    // ou quand un appel GDI+ échoue. Sans le repli, les cartes, les boutons
    // et les badges étaient simplement ABSENTS — la fenêtre paraissait vide.
    if arrondi_lisse(dc, r, rayon as f32, couleur) {
        return;
    }
    if r.right <= r.left || r.bottom <= r.top {
        return; // rectangle inversé : RoundRect ne dessinerait rien
    }
    // Repli GDI : on fixe pinceau ET stylo (sinon le contour sort noir, et un
    // pinceau par défaut blanc rendrait la forme entièrement blanche).
    let pinceau = CreateSolidBrush(couleur);
    let stylo = CreatePen(0, 1, couleur);
    let ancien_pinceau = SelectObject(dc, pinceau as *mut c_void);
    let ancien_stylo = SelectObject(dc, stylo as *mut c_void);
    RoundRect(dc, r.left, r.top, r.right, r.bottom, rayon, rayon);
    SelectObject(dc, ancien_stylo);
    SelectObject(dc, ancien_pinceau);
    DeleteObject(stylo as *mut c_void);
    DeleteObject(pinceau as *mut c_void);
}

/// Rectangle à coins arrondis ANTIALIASÉ, construit comme un chemin GDI+
/// (4 arcs + 4 segments). Rend `false` si GDI+ n'est pas disponible.
unsafe fn arrondi_lisse(dc: HDC, r: &RECT, rayon: f32, couleur: u32) -> bool {
    demarrer_gdiplus();
    if JETON_GDIPLUS == 0 {
        return false;
    }
    let mut fait = false;
    avec_gdiplus(dc, |g| {
        let mut chemin: *mut c_void = std::ptr::null_mut();
        if GdipCreatePath(FILL_MODE_ALTERNATE, &mut chemin) != 0 || chemin.is_null() {
            return;
        }
        let (x0, y0) = (r.left as f32, r.top as f32);
        let (x1, y1) = (r.right as f32, r.bottom as f32);
        let d = rayon * 2.0;
        // Les arcs GDI+ vont dans le sens horaire, en degrés.
        GdipAddPathArc(chemin, x0, y0, d, d, 180.0, 90.0);
        GdipAddPathArc(chemin, x1 - d, y0, d, d, 270.0, 90.0);
        GdipAddPathArc(chemin, x1 - d, y1 - d, d, d, 0.0, 90.0);
        GdipAddPathArc(chemin, x0, y1 - d, d, d, 90.0, 90.0);
        GdipClosePathFigure(chemin);
        let mut pinceau: *mut c_void = std::ptr::null_mut();
        let argb = argb_depuis_colorref(couleur);
        if GdipCreateSolidFill(argb, &mut pinceau) == 0 && !pinceau.is_null() {
            if GdipFillPath(g, pinceau, chemin) == 0 {
                fait = true;
            }
            GdipDeleteBrush(pinceau);
        }
        GdipDeletePath(chemin);
    });
    fait
}

/// Comme `arrondi_lisse`, mais seuls les DEUX coins du BAS sont arrondis.
///
/// Sert à la barre d'état et au panneau latéral : le fond de la fenêtre est
/// arrondi sur les quatre coins, et une bande pleine largeur (ou le panneau
/// de gauche) repeignait le bas en carré. Le chemin GDI+ est le même que
/// `arrondi_lisse` (arcs + segments), restreint aux coins inférieurs.
unsafe fn arrondi_bas(dc: HDC, r: &RECT, rayon: f32, couleur: u32) -> bool {
    arrondi_coins(dc, r, rayon, couleur, false, true)
}

/// Comme `arrondi_lisse`, mais seul le coin BAS-GAUCHE est arrondi.
unsafe fn arrondi_bas_gauche(dc: HDC, r: &RECT, rayon: f32, couleur: u32) -> bool {
    arrondi_coins(dc, r, rayon, couleur, true, false)
}

/// Tracé d'un rectangle à coins arrondis SÉLECTIONNÉS.
///
/// `haut` = coins supérieurs arrondis, `bas` = coins inférieurs arrondis.
/// Rend `false` si GDI+ n'est pas disponible (l'appelant a alors un repli).
unsafe fn arrondi_coins(dc: HDC, r: &RECT, rayon: f32, couleur: u32,
                        haut: bool, bas: bool) -> bool {
    demarrer_gdiplus();
    if JETON_GDIPLUS == 0 {
        return false;
    }
    if r.right <= r.left || r.bottom <= r.top {
        return false;
    }
    let mut fait = false;
    avec_gdiplus(dc, |g| {
        let mut chemin: *mut c_void = std::ptr::null_mut();
        if GdipCreatePath(FILL_MODE_ALTERNATE, &mut chemin) != 0 || chemin.is_null() {
            return;
        }
        let (x0, y0) = (r.left as f32, r.top as f32);
        let (x1, y1) = (r.right as f32, r.bottom as f32);
        let d = rayon * 2.0;
        // On suit le contour dans le sens horaire, en n'arrondissant que les
        // coins demandés ; les autres passent par un segment droit.
        // Coin haut-gauche → coin haut-droit → bas-droit → bas-gauche.
        GdipAddPathLine(chemin, x0, y0, x1, y0); // côté haut
        if haut {
            GdipAddPathArc(chemin, x1 - d, y0, d, d, 270.0, 90.0);
        }
        GdipAddPathLine(chemin, x1, y0, x1, y1); // côté droit
        if bas {
            GdipAddPathArc(chemin, x1 - d, y1 - d, d, d, 0.0, 90.0);
        }
        GdipAddPathLine(chemin, x1, y1, x0, y1); // côté bas
        if bas {
            GdipAddPathArc(chemin, x0, y1 - d, d, d, 90.0, 90.0);
        }
        GdipAddPathLine(chemin, x0, y1, x0, y0); // côté gauche
        if haut {
            GdipAddPathArc(chemin, x0, y0, d, d, 180.0, 90.0);
        }
        GdipClosePathFigure(chemin);
        let mut pinceau: *mut c_void = std::ptr::null_mut();
        let argb = argb_depuis_colorref(couleur);
        if GdipCreateSolidFill(argb, &mut pinceau) == 0 && !pinceau.is_null() {
            if GdipFillPath(g, pinceau, chemin) == 0 {
                fait = true;
            }
            GdipDeleteBrush(pinceau);
        }
        GdipDeletePath(chemin);
    });
    fait
}
/// `RoundRect` avec un pinceau *nul* (HBRUSH null = pas de remplissage)
/// ne trace que le crayon.
unsafe fn contour(dc: HDC, r: &RECT, rayon: i32, couleur: u32) {
    let stylo = CreatePen(0, 1, couleur);
    let ancien_stylo = SelectObject(dc, stylo as *mut c_void);
    // HOLLOW_BRUSH = 1 : c'est l'objet de stock qui ne remplit RIEN. Le
    // sélectionner évite que `RoundRect` n'aplatisse la zone avec le pinceau
    // courant (qui pouvait être BLANC et masquer le fond sombre).
    let ancien_pinceau = SelectObject(dc, 1 as *mut c_void);
    RoundRect(dc, r.left, r.top, r.right, r.bottom, rayon, rayon);
    // On restaure TOUJOURS les objets d'origine : laisser HOLLOW_BRUSH
    // sélectionné dans le DC ferait remplir les formes suivantes avec le
    // pinceau de stock de la fenêtre (BLANC), d'où les boutons et badges
    // entièrement blancs sur fond noir.
    SelectObject(dc, ancien_pinceau);
    SelectObject(dc, ancien_stylo);
    DeleteObject(stylo as *mut c_void);
}

unsafe fn cercle(dc: HDC, cx: i32, cy: i32, rayon: i32, couleur: u32) {
    // Tracé ANTIALIASÉ via GDI+ (voir `cercle_lisse`). GDI pur produisait un
    // bord crénelé, d'où l'aspect pixelisé des pastilles des étapes.
    //
    // ⚠ Le repli GDI ne doit JAMAIS être sauté : sans lui, les pastilles des
    // étapes et le voyant de la barre d'état disparaissaient quand GDI+
    // refusait de s'initialiser.
    if rayon <= 0 {
        return;
    }
    if cercle_lisse(dc, cx, cy, rayon as f32, couleur) {
        return;
    }
    // Repli GDI si GDI+ n'est pas disponible : on fixe pinceau ET stylo,
    // sinon le contour sort noir et encore plus crénelé.
    let b = CreateSolidBrush(couleur);
    let stylo = CreatePen(0, 1, couleur);
    let ancien_b = SelectObject(dc, b as *mut c_void);
    let ancien_s = SelectObject(dc, stylo as *mut c_void);
    Ellipse(dc, cx - rayon, cy - rayon, cx + rayon, cy + rayon);
    SelectObject(dc, ancien_s);
    SelectObject(dc, ancien_b);
    DeleteObject(stylo as *mut c_void);
    DeleteObject(b as *mut c_void);
}

/// Coche dessinée (deux segments).
/// Coche dessinée : deux segments épais.
///
/// `demi` = demi-hauteur de la coche en pixels. L'ancien code déduisait
/// l'épaisseur de l'ABSOLU de `cx` (une coordonnée de mise en page) : dans
/// la carte « Terminé », la coche sortait énorme et recouvrait le texte
/// « Rose X est installé » (mesuré sur la capture). La taille est donc
/// demandée explicitement.
unsafe fn coche(dc: HDC, cx: i32, cy: i32, demi: i32, couleur: u32) {
    let d = demi.max(2);
    // Épaisseur 2–3 px, proportionnée à la case (jamais déduite d'une coordonnée
    // de mise en page comme dans l'ancienne version de la carte « Terminé »).
    let epaisseur = ((d as f64 * 0.34).round() as i32).clamp(2, 3);
    // GDI (`CreatePen`/`LineTo`) n'anticrénèle PAS : les deux traits de la coche
    // sortaient en escalier, d'où « les checkmarks sont un poil pixelisés ».
    // On trace donc d'abord un CHEMIN GDI+ avec un stylo antialiasé ; le tracé
    // GDI d'origine reste comme repli si GDI+ n'est pas disponible.
    if coche_lisse(dc, cx, cy, d, couleur, epaisseur) {
        return;
    }
    let stylo = CreatePen(0, epaisseur, couleur);
    let ancien = SelectObject(dc, stylo as *mut c_void);
    // Trace « ✓ » : bras court descendant, bras long remontant.
    let a = (cx - d, cy);
    let b = (cx - d / 3, cy + d / 2);
    let c = (cx + d, cy - d / 2);
    MoveToEx(dc, a.0, a.1, std::ptr::null_mut());
    LineTo(dc, b.0, b.1);
    MoveToEx(dc, b.0, b.1, std::ptr::null_mut());
    LineTo(dc, c.0, c.1);
    SelectObject(dc, ancien);
    DeleteObject(stylo as *mut c_void);
}

/// Coche antialiasée : deux segments GDI+ dans un chemin, avec un stylo rond.
/// Rend `false` si GDI+ n'est pas disponible (l'appelant a le repli GDI).
unsafe fn coche_lisse(dc: HDC, cx: i32, cy: i32, d: i32, couleur: u32, epaisseur: i32) -> bool {
    demarrer_gdiplus();
    if JETON_GDIPLUS == 0 {
        return false;
    }
    let mut fait = false;
    avec_gdiplus(dc, |g| {
        let mut chemin: *mut c_void = std::ptr::null_mut();
        if GdipCreatePath(FILL_MODE_ALTERNATE, &mut chemin) != 0 || chemin.is_null() {
            return;
        }
        // Même géométrie que le repli GDI : bras court descendant, bras long
        // remontant. Les coordonnées sont des f32 : GDI+ anticrénèle le tracé.
        let a = ((cx - d) as f32, cy as f32);
        let b = ((cx - d / 3) as f32, (cy + d / 2) as f32);
        let c = ((cx + d) as f32, (cy - d / 2) as f32);
        GdipAddPathLine(chemin, a.0, a.1, b.0, b.1);
        GdipAddPathLine(chemin, b.0, b.1, c.0, c.1);
        let mut stylo: *mut c_void = std::ptr::null_mut();
        if GdipCreatePen1(argb_depuis_colorref(couleur), epaisseur as f32, 2, &mut stylo) == 0
            && !stylo.is_null()
        {
            GdipSetPenLineCap197819(stylo, 2, 2, 0); // LineCapRound : extrémités nettes
            if GdipDrawPath(g, stylo, chemin) == 0 {
                fait = true;
            }
            GdipDeletePen(stylo);
        }
        GdipDeletePath(chemin);
    });
    fait
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
        // Premier écran : « Continuer » enregistre le choix puis ouvre
        // l'assistant d'installation. L'action principale garde le ROUGE_VIF.
        Vue::MisesAJour => (Some((Action::Annuler, "Annuler")), Some((Action::Continuer, "Continuer"))),
        Vue::Bienvenue | Vue::Licence => (Some((Action::Annuler, "Annuler")), Some((Action::Avancer, "Suivant"))),
        Vue::Options => (
            if existant { Some((Action::Desinstaller, "Désinstaller")) } else { Some((Action::Annuler, "Annuler")) },
            Some((Action::Avancer, libelle_principal(existant))),
        ),
        Vue::Installation => (Some((Action::Annuler, "Annuler")), None),
        Vue::Termine => (Some((Action::Retour, "Fermer")), Some((Action::Fermer, "Terminer"))),
        // ⚠ L'outil PATH propose « Réparer » (applique la méthode choisie)
        // et « Fermer ». C'est là que l'utilisateur règle l'écriture du PATH
        // quand l'installation automatique a échoué.
        Vue::Path => (Some((Action::FermerOutil, "Fermer")), Some((Action::ReparerPath, "Réparer"))),
        // Sélecteur : « Retour » ramène à l'écran d'origine ; « Rafraîchir »
        // rappelle launcher_catalogue() (accès réseau, message affiché).
        Vue::Versions => (Some((Action::RetourVersions, "Retour")), Some((Action::RafraichirVersions, "Rafraîchir"))),
    }
}

/// Libellé du bouton de droite de la vue « Options ».
///
/// ⚠ Il était DÉFINI mais jamais utilisé : `boutons` écrivait « Installer »
/// en dur, même quand une installation existait. L'utilisateur ne pouvait
/// donc pas savoir s'il installait ou mettait à jour.
fn libelle_principal(existant: bool) -> &'static str {
    if existant { "Mettre à jour" } else { "Installer" }
}

/* ── Sélecteur de versions : NOYAU PUR, SANS ÉCRAN ────────────────────────
   Tout ce qui décide de ce qui est AFFICHÉ et de ce qui est CLIQUABLE est
   isolé ici, sous forme de fonctions pures : aucun appel GDI, aucun état
   global. Les tests à la fin de ce bloc les vérifient avec des FIXTURES
   (les seules versions littérales autorisées). Les chaînes affichées
   proviennent toujours du backend — jamais d'une constante de version.
   ───────────────────────────────────────────────────────────────────────── */

/// Ligne de titre d'une entrée : version, titre, date.
/// Une date vide est ANNONCÉE comme telle, jamais laissée blanche.
fn format_ligne_version(e: &crate::velopack::EntreeCatalogue) -> String {
    let date = if e.date.trim().is_empty() {
        "date inconnue".to_string()
    } else {
        e.date.chars().take(10).collect::<String>()
    };
    format!("{}  {}  ({})", e.version, e.titre, date)
}

/// Badges d'état. `en_cache` est volontairement libellé « manifeste en
/// cache » : le `.nupkg` est supprimé après extraction, ce drapeau ne
/// garantit PAS une réinstallation hors ligne (contrat § 2.1).
fn badges_version(e: &crate::velopack::EntreeCatalogue) -> String {
    let mut badges: Vec<&str> = Vec::new();
    if e.deja_installee { badges.push("Déjà installée"); }
    if e.en_cache { badges.push("Manifeste en cache"); }
    if e.installable { badges.push("Paquet disponible"); }
    if badges.is_empty() {
        "Aucun artefact installable — aucune action".to_string()
    } else {
        format!("[{}]", badges.join("]  ["))
    }
}

/// Ce qui est RÉELLEMENT proposé pour une entrée (tableau du contrat § 2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ActionsVersion {
    /// « Installer » / « Réinstaller » → `launcher_installer_version`.
    installer: bool,
    /// « Utiliser cette version » → `launcher_selectionner_version`.
    utiliser: bool,
}

fn actions_version(e: &crate::velopack::EntreeCatalogue, hors_ligne: bool) -> ActionsVersion {
    ActionsVersion {
        // § 3 : hors ligne, le téléchargement est impossible même si le
        // manifeste annonce un paquet. Seule la bascule reste possible.
        installer: e.installable && !hors_ligne,
        utiliser: e.deja_installee,
    }
}

/// Libellé de l'action d'installation : on ne promet pas une réinstallation
/// quand la version n'est pas déjà présente.
fn libelle_installer(e: &crate::velopack::EntreeCatalogue) -> &'static str {
    if e.deja_installee { "RÉINSTALLER" } else { "INSTALLER" }
}

/// Bandeau d'information. Le message vient TOUJOURS du backend ; la ligne
/// n'est jamais vide, y compris si la liste de versions est vide.
fn bandeau_versions(cat: &crate::velopack::CatalogueVersions) -> String {
    let msg = cat.message.trim();
    if cat.hors_ligne {
        let cause = if msg.is_empty() {
            "GitHub inaccessible : seules les versions déjà présentes sur cet appareil sont affichées."
        } else { msg };
        // Le message du backend peut varier : on GARANTIT toujours la précision
        // « téléchargement impossible », sans la répéter s'il l'a déjà dite.
        let bas = cause.to_lowercase();
        let precision = if bas.contains("téléchargement") || bas.contains("telechargement") {
            ""
        } else {
            " Le téléchargement est impossible tant que l'API ne répond pas."
        };
        format!("HORS LIGNE — {cause}{precision}")
    } else if msg.is_empty() {
        "Catalogue chargé depuis l'API publique des releases GitHub.".to_string()
    } else {
        msg.to_string()
    }
}

/// Hauteur d'une ligne et nombre de lignes réellement affichables.
/// `nb` = nombre d'entrées du catalogue (0 = liste vide).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LayoutListeVersions {
    pas: i32,
    visible: usize,
}

fn layout_liste(hauteur: i32, nb: usize) -> LayoutListeVersions {
    let h = hauteur.max(0);
    let pas = (h / nb.max(1) as i32).clamp(46, 76).max(1);
    let visible = ((h / pas) as usize).min(nb);
    LayoutListeVersions { pas, visible }
}

/// Rectangles (ligne, bouton installer, bouton utiliser) pour une entrée.
/// PUR : mêmes nombres pour le tracé et pour le hit-test du clic.
fn zones_ligne_version(liste: &RECT, pas: i32, index: usize, depart: usize) -> (RECT, RECT, RECT) {
    let decalage = (index.saturating_sub(depart) as i32)
        .saturating_mul(pas);
    let ligne = RECT {
        left: liste.left,
        top: liste.top + decalage,
        right: liste.right,
        bottom: liste.top + decalage + pas,
    };
    let marge = ((pas / 6).max(2)).min(10);
    let bh = (pas - 2 * marge).max(10);
    let bw = ((liste.right - liste.left) / 5).clamp(74, 150).max(1);
    let utiliser = RECT {
        left: ligne.right - marge - bw,
        top: ligne.top + marge,
        right: ligne.right - marge,
        bottom: ligne.top + marge + bh,
    };
    let installer = RECT {
        left: utiliser.left - marge - bw,
        top: utiliser.top,
        right: utiliser.left - marge,
        bottom: utiliser.bottom,
    };
    (ligne, installer, utiliser)
}

/// Petit bouton d'accès « VERSIONS », dessiné en haut à droite du contenu.
/// Même style que les autres boutons secondaires (vert lime, texte sombre).
unsafe fn dessiner_bouton_versions(dc: HDC, dpi: i32, echelle: f64, r: &RECT) {
    arrondi(dc, r, 8, VERT_LIME_VIF);
    let mut rt = *r;
    texte_echelle(dc, dpi, "Kare", "VERSIONS", &mut rt,
                  DT_CENTER | DT_VCENTER | DT_SINGLELINE, NOIR_DOUX, 9, false, echelle);
}

/// Peint la fenêtre en entier, quoi qu'il arrive.
///
/// ⚠⚠ C'est la CORRECTION du symptôme « la fenêtre s'ouvre avec ses seules
/// BORDURES, transparente, puis disparaît au bout d'un moment ». Trois
/// défauts se cumulaient :
///
/// 1. `dessiner` était appelé DIRECTEMENT depuis `WM_PAINT`. Une panique
///    (indice hors bornes, débordement, allocation) déroulait la pile du fil
///    de messages et le processus MOURAIT pendant le premier `WM_PAINT` :
///    Windows avait déjà créé la fenêtre, on n'en voyait que le cadre, puis
///    elle disparaissait avec le processus ;
/// 2. une panique laissait `BeginPaint` SANS `EndPaint` : le DC de mise à
///    jour restait verrouillé et toutes les invalidations suivantes étaient
///    ignorées — la fenêtre ne se repeignait plus JAMAIS ;
/// 3. tout le tracé se faisait sur un DC MÉMOIRE, copié vers l'écran à la
///    toute fin : un plantage en cours de route laissait l'écran VIDE, sans
///    même le fond.
///
/// Ici, le fond est peint sur l'écran AVANT tout le reste, le tracé détaillé
/// tourne dans `catch_unwind` (une panique affiche la fenêtre sans
/// décoration au lieu de tuer le processus), et `EndPaint` est TOUJOURS
/// appelé. Les objets GDI du tampon mémoire sont relâchés en cas de panique.
unsafe fn dessiner(h: HWND) {
    let mut ps = std::mem::zeroed::<PAINTSTRUCT>();
    let dc_ecran = BeginPaint(h, &mut ps);
    if dc_ecran.is_null() {
        return; // WM_PAINT non validé : rien à peindre
    }
    let mut rc = std::mem::zeroed::<RECT>();
    GetClientRect(h, &mut rc);
    // ── ZONE À REPEINDRE ───────────────────────────────────────────────────
    //
    // ⚠⚠ CAUSE DU CLIGNOTEMENT AU SURVOL DES BOUTONS. `BeginPaint` remplit
    // `ps.rcPaint` avec le rectangle RÉELLEMENT invalidé (ici : un bouton,
    // ou un bouton de barre de titre). L'ancienne version l'IGNORAIT : elle
    // peignait le fond sur TOUTE la fenêtre puis copiait TOUT le tampon.
    // Résultat : chaque entrée/sortie de souris dans un bouton repeignait
    // 1120 × 780 pixels, et comme le fond est opaque, on voyait le contenu
    // « disparaître » puis revenir — le clignotement « de fou ».
    //
    // On ne touche donc QUE `rcPaint`. Un rectangle vide (premier WM_PAINT,
    // ou invalidation sans zone) est traité comme la fenêtre entière.
    let mut zone = ps.rcPaint;
    let zone_vide = zone.right <= zone.left || zone.bottom <= zone.top;
    if zone_vide {
        zone = rc;
    }
    // Clamp : `rcPaint` ne dépasse jamais le client, mais on s'en assure.
    zone.left = zone.left.max(rc.left);
    zone.top = zone.top.max(rc.top);
    zone.right = zone.right.min(rc.right);
    zone.bottom = zone.bottom.min(rc.bottom);
    if zone.right <= zone.left || zone.bottom <= zone.top {
        EndPaint(h, &ps);
        return;
    }
    // FILE DE SÉCURITÉ : le fond est posé SUR L'ÉCRAN avant toute
    // décoration, mais SEULEMENT dans la zone invalide. Même si le tracé
    // détaillé échoue, la fenêtre n'est jamais « transparente ».
    remplir(dc_ecran, &zone, FOND);

    // Le tracé détaillé est isolé : une panique ne tue pas le processus et ne
    // laisse pas la mise à jour en cours verrouillée.
    let resultat = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        dessiner_decore(h, dc_ecran, &rc, &zone)
    }));
    if resultat.is_err() {
        // On garde une trace lisible pour le diagnostic (et pour le journal
        // de l'installeur, qui affiche les lignes du setup).
        eprintln!("rose-setup : le tracé de la fenêtre a échoué (panique) — la fenêtre reste utilisable");
        // On force un repaint complet : si la panique est venue d'une police
        // ou d'une forme, le prochain passage repart d'un état propre.
        InvalidateRect(h, std::ptr::null(), 0);
    }
    EndPaint(h, &ps);
}

/// Tracé détaillé sur le DC mémoire, puis copie de `zone` vers `dc_ecran`.
///
/// `rc` = le client entier (géométrie du tracé), `zone` = la partie à
/// copier à l'écran (celle que Windows a invalidée). Le tampon mémoire est
/// TOUJOURS peint en entier — la géométrie en dépend — mais seule `zone` est
/// transférée : c'est ce qui supprime le clignotement au survol.
///
/// ⚠ Ne JAMAIS appeler `EndPaint` ici : l'appelant (`dessiner`) le fait
/// toujours, panique comprise.
unsafe fn dessiner_decore(h: HWND, dc_ecran: HDC, rc: &RECT, zone: &RECT) {
    // Double-bufferisation : on dessine tout sur un DC mémoire puis on copie
    // d'un bloc. Sans cela, chaque `InvalidateRect` repeignait directement la
    // fenêtre, ce qui produisait un clignotement et des bords crénelés.
    let dc_memoire = CreateCompatibleDC(dc_ecran);
    let bitmap = CreateCompatibleBitmap(dc_ecran, rc.right.max(1), rc.bottom.max(1));
    if dc_memoire.is_null() || bitmap.is_null() {
        // Plus d'objet GDI (quota épuisé, DC invalide) : on décore DIRECTEMENT
        // sur l'écran plutôt que de laisser la fenêtre vide. Le clignotement
        // est préférable à un cadre sans contenu.
        if !dc_memoire.is_null() { DeleteDC(dc_memoire); }
        if !bitmap.is_null() { DeleteObject(bitmap as *mut c_void); }
        return;
    }
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
    //
    // ⚠ `paint::echelle_tracé` ne rend JAMAIS zéro : au premier `WM_PAINT`,
    // `GetClientRect` peut rendre 0 × 0 (avant `WM_SIZE`). Une échelle nulle
    // donnait des polices de taille 0, un `CreateFontW` en échec et un tracé
    // entièrement dégénéré.
    let echelle = crate::paint::echelle_tracé(w, ht);
    let s = |v: i32| -> i32 { (v as f64 * echelle).round() as i32 };
    // Une seule source de vérité pour la géométrie du panneau : le test
    // unitaire ci-dessous vérifie exactement les mêmes nombres.
    let geo = geometrie_marque(ht, dpi, echelle);
    // Le bouton d'accès au sélecteur n'existe que sur les vues qui le dessinent.
    *std::ptr::addr_of_mut!(ZONE_OUVRIR_VERSIONS) = std::mem::zeroed::<RECT>();

    // ── fond général, COINS ARRONDIS ──
    //
    // Le fond est peint comme un rectangle à coins arrondis (rayon 12 px,
    // comme les cartes) : c'est CE tracé qui donne à la fenêtre ses coins
    // arrondis. On ne demande rien au DWM (voir `lancer` : l'attribut
    // `DWMWA_WINDOW_CORNER_PREFERENCE` laissait la zone cliente VIDE sur une
    // fenêtre `WS_POPUP` sans brosse de classe — « seulement les bordures »).
    //
    // Les quatre coins hors du rayon ne sont pas peints : la fenêtre n'a pas
    // de brosse de fond (`hbrBackground = NULL`) et `WM_ERASEBKGND` rend 1,
    // donc ils restent transparents — le bureau y apparaît, comme sur une
    // fenêtre Windows 11 arrondie.
    if !arrondi_lisse(dc, rc, 12.0, FOND) {
        remplir(dc, rc, FOND); // GDI+ indisponible : coins droits
    }

    // ── barre de titre PERSONNALISÉE (aucun cadre Windows) ──
    // La fenêtre est un WS_POPUP : tout est dessiné ici — fond, rose, marque
    // en Kare, pastille de version et trois boutons (réduire / agrandir /
    // fermer) au style de la maquette, pas ceux de Windows 11.
    let barre = RECT { left: 0, top: 0, right: w, bottom: s(56) };
    remplir(dc, &barre, BARRE);
    // filet lumineux sous la barre : sépare visuellement l'en-tête
    let filet_barre = RECT { left: 0, top: s(55), right: w, bottom: s(56) };
    remplir(dc, &filet_barre, LIGNE_FORTE);
    dessiner_rose(dc, s(18), s(16), s(24), s(24));
    // La marque « Rose » en Kare, comme sur le site et la maquette.
    // Même correction que pour le grand titre : la hauteur du rectangle suit
    // la cellule réelle de la police (DPI), pas seulement l'échelle de fenêtre.
    let barre_marque_px = points_ecrits(dpi, 15, echelle);
    let h_barre_marque = (s(30)).max(barre_marque_px + s(8));
    let centre_barre_marque = s(28);
    let haut_barre_marque = centre_barre_marque - h_barre_marque / 2;
    let mut rt = RECT { left: s(52), top: haut_barre_marque, right: s(150),
                        bottom: haut_barre_marque + h_barre_marque };
    texte_echelle(dc, dpi, "Kare", "Rose", &mut rt, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS, TEXTE, 15, false, echelle);
    // ⚠ Le sous-titre s'arrête AVANT la pastille de version : à droite fixe
    // de 430 sur la grille, il chevauchait « v0.1.5 · x64 » dès que la fenêtre
    // était étroite (mesuré : « Programme d'install v0.1.5 » illisible).
    // On lui donne tout l'espace qui reste jusqu'à la pastille, et on ellipse.
    let mut rt2 = RECT { left: s(150), top: s(16), right: (s(438) - s(8)).max(s(170)), bottom: s(40) };
    texte_echelle(dc, dpi, "Segoe UI", "— Programme d'installation", &mut rt2,
          DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS, TEXTE_DOUX, 10, false, echelle);
    // pastille de version — elle se décale à gauche quand la fenêtre est
    // étroite, pour ne jamais passer sous les boutons réduire/agrandir/fermer.
    let largeur_pastille = s(128);
    let boutons_l = w - 3 * s(46);
    let droite_pastille = (s(438) + largeur_pastille).min(boutons_l - s(8));
    let pastille = RECT { left: droite_pastille - largeur_pastille, top: s(16), right: droite_pastille, bottom: s(40) };
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
    // Coin BAS-GAUCHE de la fenêtre : il est arrondi sur le fond ; le panneau
    // latéral ne doit pas le recouvrir carrément.
    if !arrondi_bas_gauche(dc, &lateral, 12.0, LATERAL) {
        remplir(dc, &lateral, LATERAL);
    }
    let filet = RECT { left: lateral_w - 1, top: s(56), right: lateral_w, bottom: ht - etat_h };
    remplir(dc, &filet, LIGNE_FORTE);

    // Cartes du bas du panneau : elles sont réservées AVANT le reste, sinon
    // la liste des étapes leur passait dessus dès que la fenêtre était basse.
    let bas = ht - etat_h - s(18);
    let carte2 = RECT { left: s(20), top: bas - s(38), right: lateral_w - s(20), bottom: bas };
    let carte1 = RECT { left: s(20), top: carte2.top - s(46), right: lateral_w - s(20), bottom: carte2.top - s(8) };
    let plancher = geo.plancher; // rien ne descend sous les cartes

    // Logo, titre, sous-titre et étapes : TOUTE la géométrie vient de
    // `geometrie_marque` (calculée une seule fois au début du tracé, et
    // vérifiée par les tests ci-dessous). Le rectangle du titre est
    // dimensionné sur la hauteur RÉELLE de la police Kare — qui suit le DPI —
    // et le sous-titre part de son bas réel : plus aucun chevauchement.
    if geo.hauteur_logo >= s(40) {
        dessiner_rose(dc, lateral_w / 2 - geo.largeur_logo / 2, s(78),
                      geo.largeur_logo, geo.hauteur_logo);
    }
    let mut rl = RECT { left: 0, top: geo.haut_titre, right: lateral_w,
                        bottom: geo.bas_titre };
    // Marque en Kare, comme le site : c'est la signature visuelle de Rose.
    texte_echelle(dc, dpi, "Kare", "Rose", &mut rl, DT_CENTER | DT_VCENTER | DT_SINGLELINE, TEXTE, 36, false, echelle);
    let mut rs = RECT { left: s(24), top: geo.haut_sous_titre, right: lateral_w - s(24),
                        bottom: geo.bas_sous_titre };
    texte_echelle(dc, dpi, "Segoe UI", "Le langage compilé en français qui se compile lui-même.",
          &mut rs, DT_CENTER | DT_WORDBREAK | DT_END_ELLIPSIS, TEXTE_DOUX, 9, false, echelle);

    // étapes : la liste ne descend JAMAIS sous les cartes du bas, et ne
    // remonte PAS sur le sous-titre.
    let mut y = geo.y_etapes;
    let pas = ((plancher - y) / 5).clamp(s(26), s(44));
    for (i, nom) in ETAPES.iter().enumerate() {
        if y + pas > plancher + s(4) {
            break; // plus de place : les étapes restantes sont omises
        }
        let atteinte = etat.atteintes[i];
        let courante = (etat.vue as usize) == i;
        let couleur_puce = if atteinte { ROUGE } else if courante { ROUGE_SOMBRE } else { FOND_PUCE };
        let centre = y + pas / 2;
        let rayon = s(14).min(pas / 2 - s(1)).max(s(4));
        cercle(dc, s(48), centre, rayon, couleur_puce);
        if courante && !atteinte {
            // anneau : l'étape en cours se distingue des étapes à venir
            cercle(dc, s(48), centre, rayon, ROUGE_CLAIR);
            cercle(dc, s(48), centre, rayon - s(3), LATERAL);
        }
        if atteinte {
            coche(dc, s(48), centre, rayon / 2, TEXTE);
        } else {
            let mut rn = RECT { left: s(34), top: y, right: s(62), bottom: y + pas };
            texte_echelle(dc, dpi, "Segoe UI", &format!("{}", i + 1), &mut rn,
                  DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                  if courante { TEXTE } else { TEXTE_FAIBLE }, 9, false, echelle);
        }
        let couleur_nom = if courante || atteinte { TEXTE } else { TEXTE_FAIBLE };
        let mut rnom = RECT { left: s(74), top: y, right: lateral_w - s(20), bottom: y + pas };
        texte_echelle(dc, dpi, "Segoe UI", nom, &mut rnom, DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS, couleur_nom, 10, false, echelle);
        y += pas;
    }

    // cartes du bas du panneau latéral — VERT LIME, comme les boutons
    arrondi(dc, &carte2, 10, VERT_LIME);
    let mut rc2 = RECT { left: s(34), top: carte2.top, right: carte2.right - s(14), bottom: carte2.bottom };
    // ⚠ PAS de `⚿` (U+26BF) dans ce libellé : ce glyphe n'existe dans
    // AUCUNE police de Windows 10/11 courante ; il sortait en carré vide, et
    // le badge paraissait « cassé ». Un simple libellé suffit.
    texte_echelle(dc, dpi, "Segoe UI", if etat.utilisateur { "MODE UTILISATEUR" } else { "DROITS REQUIS" },
          &mut rc2, DT_VCENTER | DT_SINGLELINE | DT_CENTER | DT_END_ELLIPSIS, NOIR_DOUX, 9, true, echelle);
    let carte1 = RECT { left: s(20), top: carte2.top - s(46), right: lateral_w - s(20), bottom: carte2.top - s(8) };
    arrondi(dc, &carte1, 10, VERT_LIME);
    let mut rc1 = RECT { left: s(34), top: carte1.top, right: carte1.right - s(14), bottom: carte1.bottom };
    texte_echelle(dc, dpi, "Segoe UI", "{ ROSE } · MIT", &mut rc1, DT_VCENTER | DT_SINGLELINE, NOIR_DOUX, 9, true, echelle);

    // ── zone de contenu ──
    //
    // ⚠⚠ TOUTES les cartes sont désormais dimensionnées sur leur TEXTE RÉEL,
    // mesuré par GDI (`mesurer_texte` + `paint::lignes_ajustees`). Les
    // hauteurs étaient écrites en dur (`s(150)`, `s(186)`…) sur la grille de
    // 1120 × 780 : dès que la fenêtre était plus petite (ou le DPI
    // différent), les cartes se retrouvaient PLUS BASSES QUE LEUR CONTENU et
    // `DrawText` ne dessinait que la première ligne — l'utilisateur voyait une
    // fenêtre « vide ». La licence, elle, était entièrement hors du cadre.
    let contenu = RECT { left: lateral_w + 1, top: s(56), right: w, bottom: ht - etat_h - pied_h };
    let marge = s(56);
    let g = contenu.left + marge;
    let d = contenu.right - marge;
    let largeur_texte = (d - g - s(40)).max(s(80));
    // Interlignes des tailles utilisées (10 pt pour le corps, 9 pt pour les
    // notes), mis à l'échelle comme les polices.
    let interligne = points_ecrits(dpi, 10, echelle) + (s(6)).max(2);
    let interligne_petit = points_ecrits(dpi, 9, echelle) + (s(5)).max(2);
    let mesurer = |taille_pt: i32, gras: bool| {
        move |t: &str| -> i32 {
            let px = points_ecrits(dpi, taille_pt, echelle);
            unsafe { mesurer_texte(dc, "Segoe UI", px, gras, t) }
        }
    };
    // Hauteur d'un bloc de texte, mesurée sur la vraie police.
    let hauteur = |txt: &str, taille_pt: i32, ligne: i32| -> i32 {
        crate::paint::hauteur_texte(txt, largeur_texte, &mesurer(taille_pt, false), ligne)
    };

    let mut yc = contenu.top + s(44);
    let (titre, sous) = match etat.vue {
        Vue::MisesAJour => (
            "Mises à jour".to_string(),
            "Choisissez comment Rose vérifie ses mises à jour. Ce choix est enregistré \
             dans votre profil et relu à chaque lancement — aucun service Windows \
             ni tâche planifiée n'est installé.".to_string(),
        ),
        Vue::Bienvenue => (
            "Bienvenue".to_string(),
            "Ce programme installe le compilateur Rose, ses binaires Windows et le helper AOT.".to_string(),
        ),
        Vue::Licence => ("Licence MIT".to_string(), "Rose est distribué sous licence MIT.".to_string()),
        Vue::Options => ("Options".to_string(), "Dossier d'installation et vérification du paquet.".to_string()),
        Vue::Installation => ("Installation".to_string(), etat.statut.clone()),
        Vue::Termine => ("Terminé".to_string(), "Rose est prêt.".to_string()),
        Vue::Path => (
            "Outil PATH".to_string(),
            "Diagnostic et réparation — plusieurs méthodes, dont un mode entreprise.".to_string(),
        ),
        Vue::Versions => (
            "Versions".to_string(),
            "Versions annoncées par GitHub et état réel de cet appareil. Installez ou \
             basculez sur une version déjà présente — aucun téléchargement hors ligne."
                .to_string(),
        ),
    };
    let mut rti = RECT { left: g, top: yc, right: d, bottom: yc + s(44) };
    // Titre de section en Kare — mais EN MAJUSCULES : Kare est une police
    // d'AFFICHAGE (ses minuscules dessinent les capitales), donc un titre en
    // minuscules se lit mal (« Options » → « PTIONS »). Le site réserve Kare
    // aux titres, à la marque et aux boutons, et Segoe UI au texte courant.
    let titre_kare = titre.to_uppercase();
    texte_echelle(dc, dpi, "Kare", &titre_kare, &mut rti, DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS, TEXTE, 20, false, echelle);
    yc += s(52);
    // Le sous-titre suit le texte de la vue : en mode « Installation » il
    // affiche le STATUT de l'opération, qui peut tenir sur plusieurs lignes.
    let hauteur_sous = hauteur(&sous, 10, interligne).max(s(20));
    let mut rso = RECT { left: g, top: yc, right: d, bottom: yc + hauteur_sous };
    texte_echelle(dc, dpi, "Segoe UI", &sous, &mut rso, DT_WORDBREAK | DT_END_ELLIPSIS, TEXTE_DOUX, 10, false, echelle);
    yc += hauteur_sous + s(28);

    // Hauteur utile restante : aucune carte ne doit déborder du pied de page.
    let bas_utile = contenu.bottom - s(20);
    match etat.vue {
        // ── PREMIER ÉCRAN : choix du mode de mise à jour ──────────────
        // Trois cartes radio, thème Rose (ROUGE_VIF / VERT_LIME_VIF),
        // indicateur = cercle + COCHE ANTIALIASÉE (correction LA). Le choix
        // est écrit par « Continuer » dans le pied de page.
        Vue::MisesAJour => {
            use crate::velopack::ModeMiseAJour;
            // Accès DIRECT au sélecteur de versions, dès le premier écran.
            let r_versions = RECT { left: d - s(126), top: contenu.top + s(6),
                                    right: d, bottom: contenu.top + s(38) };
            dessiner_bouton_versions(dc, dpi, echelle, &r_versions);
            *std::ptr::addr_of_mut!(ZONE_OUVRIR_VERSIONS) = r_versions;
            let options: [(ModeMiseAJour, &str, &str); 3] = [
                (ModeMiseAJour::Auto, "Auto au démarrage",
                 "Vérifie les versions à chaque lancement. Ne télécharge qu'après votre accord — recommandé."),
                (ModeMiseAJour::Manuel, "Manuel",
                 "Aucune vérification automatique : « rose update » ou le bouton « Vérifier les mises à jour »."),
                (ModeMiseAJour::Desactive, "Désactivées",
                 "Aucune vérification, même « rose update » est ignoré (message explicite)."),
            ];
            let h_opt = s(84);
            let ecart = s(12);
            let mut zm = [std::mem::zeroed::<RECT>(), std::mem::zeroed::<RECT>(), std::mem::zeroed::<RECT>()];
            for (i, (mode, titre_opt, desc)) in options.iter().enumerate() {
                let haut = yc + i as i32 * (h_opt + ecart);
                let c = RECT { left: g, top: haut, right: d, bottom: haut + h_opt };
                let selectionne = *mode == etat.mode_maj;
                carte(dc, &c, if selectionne { VERT_LIME_VIF } else { LIGNE_FORTE });
                zm[i] = c;
                let cx = g + s(30);
                let cy = haut + h_opt / 2;
                cercle(dc, cx, cy, s(13), if selectionne { VERT_LIME_VIF } else { LIGNE_FORTE });
                cercle(dc, cx, cy, s(10), CARTE);
                if selectionne { coche(dc, cx, cy, s(6), VERT_LIME_VIF); }
                let mut rt = RECT { left: g + s(58), top: haut + s(12), right: d - s(20), bottom: haut + s(38) };
                texte_echelle(dc, dpi, "Segoe UI", titre_opt, &mut rt,
                              DT_SINGLELINE | DT_VCENTER, if selectionne { TEXTE } else { TEXTE_DOUX },
                              11, selectionne, echelle);
                let mut rd = RECT { left: g + s(58), top: haut + s(40), right: d - s(20), bottom: c.bottom - s(10) };
                texte_echelle(dc, dpi, "Segoe UI", desc, &mut rd,
                              DT_WORDBREAK | DT_TOP | DT_END_ELLIPSIS, TEXTE_DOUX, 9, false, echelle);
            }
            *std::ptr::addr_of_mut!(ZONES_MAJ) = zm;

            // Ligne de statut : vérification auto / bouton de consentement.
            let ys = yc + 3 * (h_opt + ecart) + s(4);
            let suiv = SUIVI_MAJ.lock().unwrap_or_else(|e| e.into_inner());
            let mut libelle_action: Option<&str> = None;
            let (statut, couleur_statut): (String, u32) = match etat.mode_maj {
                ModeMiseAJour::Desactive => (
                    "Mises à jour DÉSACTIVÉES : aucune vérification, même « rose update ».".to_string(),
                    ROUGE_VIF,
                ),
                _ if suiv.telecharge_en_cours => (
                    "Téléchargement de la mise à jour…".to_string(), VERT_LIME_VIF),
                _ if suiv.telechargement.is_some() => match &suiv.telechargement {
                    Some(Ok(Some(v))) => (
                        format!("Paquet Rose {v} téléchargé — relancez l'installation pour l'appliquer."),
                        VERT_LIME_VIF),
                    Some(Ok(None)) => ("Rose est déjà à jour.".to_string(), VERT_LIME_VIF),
                    Some(Err(e)) => (format!("Téléchargement impossible : {e}"), ROUGE_VIF),
                    _ => unreachable!(),
                },
                _ if suiv.verifie_en_cours => (
                    "Vérification des versions disponibles…".to_string(), TEXTE_DOUX),
                _ if suiv.verification.is_some() => match &suiv.verification {
                    Some(Ok(Some(v))) => {
                        libelle_action = Some("Télécharger");
                        (format!("Mise à jour disponible : Rose {v} — téléchargez seulement si vous le souhaitez."),
                         VERT_LIME_VIF)
                    }
                    Some(Ok(None)) => ("Rose est à jour.".to_string(), VERT_LIME_VIF),
                    Some(Err(e)) => {
                        // Échec réseau/hors-ligne : laisser un bouton pour
                        // retenter une vérification explicite.
                        libelle_action = Some("Vérifier les mises à jour");
                        (format!("Vérification impossible : {e}"), ROUGE_CLAIR)
                    }
                    _ => unreachable!(),
                },
                _ if etat.mode_maj == ModeMiseAJour::Manuel => {
                    libelle_action = Some("Vérifier les mises à jour");
                    ("Mode manuel : aucune vérification automatique.".to_string(), TEXTE_DOUX)
                }
                _ => ("Vérification au démarrage…".to_string(), TEXTE_DOUX),
            };
            drop(suiv);
            let mut rs = RECT { left: g, top: ys, right: d - s(218), bottom: (ys + s(46)).min(bas_utile) };
            texte_echelle(dc, dpi, "Segoe UI", &statut, &mut rs,
                          DT_WORDBREAK | DT_END_ELLIPSIS | DT_TOP, couleur_statut, 9, false, echelle);
            if let Some(libelle) = libelle_action {
                let bouton = RECT { left: d - s(210), top: ys, right: d, bottom: ys + s(34) };
                let fond = if libelle == "Télécharger" { VERT_LIME_VIF } else { VERT_LIME };
                arrondi(dc, &bouton, 8, fond);
                contour(dc, &bouton, 8, fond);
                let mut rb = bouton;
                texte_echelle(dc, dpi, "Kare", &libelle.to_uppercase(), &mut rb,
                              DT_CENTER | DT_VCENTER | DT_SINGLELINE, NOIR_DOUX, 9, false, echelle);
                *std::ptr::addr_of_mut!(ZONE_MAJ_ACTION) = bouton;
            } else {
                *std::ptr::addr_of_mut!(ZONE_MAJ_ACTION) = std::mem::zeroed::<RECT>();
            }
        }
        Vue::Bienvenue => {
            let corps = "Rose est un langage de programmation en français, auto-hébergé : \
                         interprète exact, JIT Cranelift, AOT LLVM et module WASM.\n\n\
                         L'installation se fait dans votre profil utilisateur — aucun droit \
                         administrateur n'est demandé.";
            let note = if etat.existant {
                format!(
                    "Une installation existante a été détectée : version {} dans {}\n\n\
                     La suite de l'assistant propose de la mettre à jour \
                     (elle sera réinstallée proprement) ou de la désinstaller.",
                    etat.version_installee, etat.prefixe)
            } else {
                "Aucune installation détectée : l'assistant proposera le dossier par défaut.".to_string()
            };
            let h_corps = hauteur(corps, 10, interligne);
            let h_note = hauteur(&note, 9, interligne_petit);
            // La carte occupe la hauteur disponible : un bloc de texte seul
            // laissait un grand vide sous la note, ce qui donnait l'impression
            // que la fenêtre n'était pas finie de dessiner.
            let h_dispo = (bas_utile - yc - h_note - s(18)).max(s(120));
            let h_carte = crate::paint::hauteur_carte(h_corps, s(18), s(120)).max(h_dispo);
            let c = RECT { left: g, top: yc, right: d, bottom: yc + h_carte };
            carte(dc, &c, ROUGE);
            // Le texte est aligné en haut de la carte : `DT_TOP` évite que
            // `DrawText` le centre verticalement quand la carte est haute.
            let mut r = RECT { left: g + s(20), top: yc + s(16), right: d - s(20), bottom: c.bottom - s(14) };
            texte_echelle(dc, dpi, "Segoe UI", corps, &mut r, DT_WORDBREAK | DT_TOP | DT_END_ELLIPSIS, TEXTE, 10, false, echelle);
            // La note est ancrée sous la carte : elle ne doit JAMAIS être
            // coupée (c'était le cas mesuré).
            let mut ri = RECT { left: g, top: c.bottom + s(14), right: d, bottom: bas_utile };
            texte_echelle(dc, dpi, "Segoe UI", &note, &mut ri, DT_WORDBREAK | DT_END_ELLIPSIS, TEXTE_DOUX, 9, false, echelle);
        }
        Vue::Licence => {
            let c = RECT { left: g, top: yc, right: d, bottom: bas_utile };
            carte(dc, &c, VERT);
            let mut r = RECT { left: g + s(20), top: yc + s(16), right: d - s(20), bottom: c.bottom - s(14) };
            // `DT_END_ELLIPSIS` + barre de défilement absente : on affiche le
            // texte au complet s'il tient, sinon on le coupe proprement avec
            // « … » au lieu de laisser la carte « vide » (texte entièrement
            // hors cadre).
            texte_echelle(dc, dpi, "Segoe UI", &etat.licence, &mut r, DT_WORDBREAK | DT_END_ELLIPSIS, TEXTE, 9, false, echelle);
        }
        Vue::Options => {
            // Second accès au sélecteur, cohérent avec le premier écran.
            let r_versions = RECT { left: d - s(126), top: contenu.top + s(6),
                                    right: d, bottom: contenu.top + s(38) };
            dessiner_bouton_versions(dc, dpi, echelle, &r_versions);
            *std::ptr::addr_of_mut!(ZONE_OUVRIR_VERSIONS) = r_versions;
            // ── carte 1 : dossier d'installation ──────────────────────────
            let libelle = "Dossier d'installation";
            let chemin = etat.prefixe.clone();
            let verdict = if etat.utilisateur {
                "✓  Écriture autorisée dans ce dossier (profil utilisateur)"
            } else {
                "!  Ce dossier n'est pas inscriptible : choisissez un autre emplacement"
            };
            let h1 = s(14) + s(20) + s(24) + s(24) + s(14);
            let c = RECT { left: g, top: yc, right: d, bottom: yc + h1 };
            carte(dc, &c, ROUGE);
            let mut r1 = RECT { left: g + s(20), top: yc + s(14), right: d - s(20), bottom: yc + s(34) };
            texte_echelle(dc, dpi, "Segoe UI", libelle, &mut r1, DT_SINGLELINE | DT_VCENTER, TEXTE_DOUX, 9, true, echelle);
            let mut r2 = RECT { left: g + s(20), top: yc + s(38), right: d - s(20), bottom: yc + s(62) };
            // Le chemin peut être long : ellipse plutôt que débordement.
            texte_echelle(dc, dpi, "Segoe UI", &chemin, &mut r2, DT_SINGLELINE | DT_VCENTER | DT_PATH_ELLIPSIS, TEXTE, 10, false, echelle);
            let mut r3 = RECT { left: g + s(20), top: yc + s(66), right: d - s(20), bottom: yc + s(88) };
            texte_echelle(dc, dpi, "Segoe UI", verdict, &mut r3, DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
                  if etat.utilisateur { VERT } else { ROUGE_CLAIR }, 9, false, echelle);

            // ── carte 2 : paquet à installer ──────────────────────────────
            let y2 = c.bottom + s(16);
            let empreinte = match &etat.empreinte {
                Some(h) => format!("SHA-256 vérifié · {}", h.chars().take(16).collect::<String>()),
                None => "payload absent — l'installeur copiera le moteur du paquet".to_string(),
            };
            let contenu_paquet = "rose.exe · rose-shell.exe · icônes · association .rose · désinstallation";
            let h_paquet = crate::paint::hauteur_carte(
                hauteur(contenu_paquet, 9, interligne_petit), s(14), s(104));
            let c2 = RECT { left: g, top: y2, right: d, bottom: y2 + h_paquet };
            carte(dc, &c2, VERT);
            let mut r4 = RECT { left: g + s(20), top: y2 + s(14), right: d - s(20), bottom: y2 + s(34) };
            texte_echelle(dc, dpi, "Segoe UI", "Paquet à installer", &mut r4, DT_SINGLELINE | DT_VCENTER,
                  TEXTE_DOUX, 9, true, echelle);
            let mut r5 = RECT { left: g + s(20), top: y2 + s(38), right: d - s(20), bottom: y2 + s(60) };
            texte_echelle(dc, dpi, "Segoe UI", &empreinte, &mut r5, DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
                  if etat.empreinte.is_some() { VERT } else { TEXTE_DOUX }, 9, false, echelle);
            let mut r6 = RECT { left: g + s(20), top: y2 + s(64), right: d - s(20), bottom: c2.bottom - s(10) };
            texte_echelle(dc, dpi, "Segoe UI", contenu_paquet, &mut r6, DT_WORDBREAK | DT_END_ELLIPSIS, TEXTE_DOUX, 9, false, echelle);

            // ── carte 3 : installation détectée ───────────────────────────
            // ⚠ L'assistant doit DIRE ce qu'il a trouvé et ce qu'il fera.
            // Sans ce bloc, l'utilisateur ne savait pas s'il réinstallait,
            // mettait à jour ou installait à côté d'une installation
            // existante (mesuré : « version 0.1.5 détectée » invisible).
            if etat.existant {
                let y3 = c2.bottom + s(16);
                let meme = crate::setup_core::comparer_versions(
                    &etat.version_installee, env!("CARGO_PKG_VERSION")) == 0;
                let ligne_version = format!("Version {} — {}", etat.version_installee,
                    if meme { "identique au paquet : réinstallation (repair)" }
                    else { "remplacée par la version du paquet" });
                let note = "« Mettre à jour » réinstalle proprement ; « Désinstaller » retire tout.";
                let h3 = s(14) + s(20) + s(24) + hauteur(note, 9, interligne_petit) + s(12);
                let h3 = crate::paint::hauteur_carte(h3 - s(14), s(12), s(96))
                    .min((bas_utile - y3).max(s(96)));
                let c3 = RECT { left: g, top: y3, right: d, bottom: y3 + h3 };
                // ⚠ `carte()` remplit avec `CARTE` (FONDOIR) et n'ajoute
                // qu'un filet d'accent à gauche. Un texte NOIR dessus était
                // donc ILLISIBLE (mesuré : « l'installation détectée est
                // affichée en noir »). Le texte reste clair.
                // Mise à jour : accent VERT LIME VIF (thème rose update).
                carte(dc, &c3, VERT_LIME_VIF);
                let mut r7 = RECT { left: g + s(20), top: y3 + s(14), right: d - s(20), bottom: y3 + s(34) };
                texte_echelle(dc, dpi, "Segoe UI", "Installation détectée", &mut r7,
                      DT_SINGLELINE | DT_VCENTER, VERT_LIME_VIF, 9, true, echelle);
                let mut r8 = RECT { left: g + s(20), top: y3 + s(38), right: d - s(20), bottom: y3 + s(62) };
                texte_echelle(dc, dpi, "Segoe UI", &ligne_version,
                      &mut r8, DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS, TEXTE, 9, false, echelle);
                let mut r9 = RECT { left: g + s(20), top: y3 + s(66), right: d - s(20), bottom: c3.bottom - s(10) };
                texte_echelle(dc, dpi, "Segoe UI", note, &mut r9, DT_WORDBREAK | DT_END_ELLIPSIS, TEXTE_DOUX, 9, false, echelle);
            }
        }
        Vue::Installation => {
            let statut = etat.statut.clone();
            // ⚠ Si l'opération a ÉCHOUÉ, on ajoute un bouton bien visible
            // « Ouvrir l'outil PATH » : c'est le geste attendu par
            // l'utilisateur, pas un message lui demandant de le faire à la
            // main. Le bouton est dessiné sous la carte d'erreur.
            let en_echec = statut.starts_with("ÉCHEC");
            let h_statut = hauteur(&statut, 10, interligne);
            let h_carte = crate::paint::hauteur_carte(h_statut, s(18), s(72));
            let c = RECT { left: g, top: yc, right: d, bottom: yc + h_carte };
            // Thème rose update : échec = ROUGE VIF, progression = VERT LIME VIF.
            carte(dc, &c, if en_echec { ROUGE_VIF } else { VERT_LIME_VIF });
            let mut r = RECT { left: g + s(20), top: yc + s(16), right: d - s(20), bottom: c.bottom - s(14) };
            texte_echelle(dc, dpi, "Segoe UI", &statut, &mut r, DT_WORDBREAK | DT_END_ELLIPSIS, TEXTE, 10, false, echelle);
            // barre de progression indéterminée — bornée par `paint::segment_progression`
            if en_echec {
                // Bouton d'action : ouvre l'outil PATH.
                let rb = RECT { left: g, top: c.bottom + s(20), right: g + s(280).min(d - g), bottom: c.bottom + s(64) };
                let fond = if etat.survol == 2 { VERT_LIME_VIF_CLAIR } else { VERT_LIME_VIF };
                arrondi(dc, &rb, 10, fond);
                let mut rt = RECT { left: rb.left, top: rb.top, right: rb.right, bottom: rb.bottom };
                texte_echelle(dc, dpi, "Kare", "OUVRIR L'OUTIL PATH", &mut rt,
                              DT_CENTER | DT_VCENTER | DT_SINGLELINE, NOIR_DOUX, 11, false, echelle);
                ZONES_ZONE_PATH = rb;
            } else {
                let piste = RECT { left: g, top: c.bottom + s(20), right: d, bottom: c.bottom + s(26) };
                remplir(dc, &piste, LIGNE);
                let (a, b) = crate::paint::segment_progression(etat.phase, d - g);
                if b > a {
                    let barre = RECT { left: g + a, top: piste.top, right: g + b, bottom: piste.bottom };
                    remplir(dc, &barre, VERT_LIME_VIF);
                }
                ZONES_ZONE_PATH = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            }
        }
        Vue::Path => {
            // ⚠⚠⚠ L'OUTIL PATH. On affiche :
            //   1. le DIAGNOSTIC (une ligne par constat, avec ✓ ou Échec) ;
            //   2. la MÉTHODE choisie (clic sur les lignes pour changer) ;
            //   3. le JOURNAL de la dernière réparation.
            //
            // ⚠ Les méthodes « entreprise » (Lanceur, Raccourci, Association)
            // ne modifient PAS le PATH : elles fonctionnent même quand une
            // stratégie de groupe verrouille HKCU\Environment.
            let diag = match &etat.diagnostic {
                Some(d) => d.messages.join("\n"),
                None => "Diagnostic non calculé.".to_string(),
            };
            let h_diag = hauteur(&diag, 9, interligne_petit);
            let h_c1 = crate::paint::hauteur_carte(h_diag, s(18), s(96));
            let c1 = RECT { left: g, top: yc, right: d, bottom: yc + h_c1 };
            let ok = etat.diagnostic.as_ref().map(|d| d.ok()).unwrap_or(false);
            carte(dc, &c1, if ok { VERT } else { ROUGE_CLAIR });
            let mut r1 = RECT { left: g + s(20), top: yc + s(16), right: d - s(20), bottom: c1.bottom - s(14) };
            texte_echelle(dc, dpi, "Segoe UI", &diag, &mut r1, DT_WORDBREAK | DT_TOP | DT_END_ELLIPSIS,
                          TEXTE, 9, false, echelle);

            // ── carte 2 : méthodes ──────────────────────────────────────
            let y2 = c1.bottom + s(16);
            let mut lignes = String::new();
            for (i, m) in crate::setup_core::METHODES_PATH.iter().enumerate() {
                let marque = if i == etat.methode { "▶" } else { "  " };
                let tag = if m.modifie_path() { "modifie le PATH" } else { "sans toucher au PATH" };
                lignes.push_str(&format!("{marque} {}  ({tag})\n", m.nom()));
            }
            let h_m = hauteur(&lignes, 9, interligne_petit);
            let h_c2 = crate::paint::hauteur_carte(h_m, s(18), s(110));
            let c2 = RECT { left: g, top: y2, right: d, bottom: y2 + h_c2 };
            carte(dc, &c2, VERT_LIME);
            let mut r2 = RECT { left: g + s(20), top: y2 + s(16), right: d - s(20), bottom: c2.bottom - s(14) };
            texte_echelle(dc, dpi, "Segoe UI", &lignes, &mut r2, DT_WORDBREAK | DT_TOP | DT_END_ELLIPSIS,
                          TEXTE, 9, false, echelle);
            ZONES_METHODES = r2;

            // ── carte 3 : journal de la réparation ──────────────────────
            if !etat.journal_path.is_empty() {
                let y3 = c2.bottom + s(16);
                let journal = etat.journal_path.join("\n");
                let h_j = hauteur(&journal, 9, interligne_petit);
                let h_c3 = crate::paint::hauteur_carte(h_j, s(18), s(96))
                    .min((bas_utile - y3).max(s(96)));
                let c3 = RECT { left: g, top: y3, right: d, bottom: y3 + h_c3 };
                carte(dc, &c3, ROUGE);
                let mut r3 = RECT { left: g + s(20), top: y3 + s(16), right: d - s(20), bottom: c3.bottom - s(14) };
                texte_echelle(dc, dpi, "Segoe UI", &journal, &mut r3,
                              DT_WORDBREAK | DT_TOP | DT_END_ELLIPSIS, TEXTE, 9, false, echelle);
            }
        }
        // ── SÉLECTEUR DE VERSIONS (backend dynamique, aucune version en dur) ──
        Vue::Versions => {
            let nb = etat.catalogue.as_ref().map(|c| c.versions.len()).unwrap_or(0);
            let hors = etat.catalogue.as_ref().map(|c| c.hors_ligne).unwrap_or(false);

            // 1. Bandeau : le message du backend, jamais vide, bien visible.
            let message = etat.catalogue.as_ref().map(bandeau_versions)
                .unwrap_or_else(|| "Chargement du catalogue des versions…".to_string());
            let h_message = hauteur(&message, 9, interligne_petit);
            let bandeau = RECT {
                left: g, top: yc, right: d,
                bottom: (yc + h_message + s(24)).min(bas_utile),
            };
            carte(dc, &bandeau, if hors { ROUGE_VIF } else { VERT_LIME_VIF });
            let mut rm = RECT { left: g + s(18), top: bandeau.top + s(8),
                                right: d - s(18), bottom: bandeau.bottom - s(6) };
            texte_echelle(dc, dpi, "Segoe UI", &message, &mut rm,
                          DT_WORDBREAK | DT_END_ELLIPSIS | DT_TOP,
                          if hors { ROUGE_CLAIR } else { TEXTE }, 9, true, echelle);

            // 2. En-tête de la liste + flèches de défilement pour la souris.
            let y_tete = bandeau.bottom + s(12);
            let mut rtete = RECT { left: g, top: y_tete, right: d - s(80),
                                   bottom: y_tete + s(24) };
            texte_echelle(dc, dpi, "Segoe UI",
                          &format!("{nb} version(s) · flèches ↑/↓ ou clic pour naviguer"),
                          &mut rtete, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                          TEXTE_DOUX, 9, false, echelle);
            let fleche_haut = RECT { left: d - s(72), top: y_tete + s(2),
                                     right: d - s(40), bottom: y_tete + s(22) };
            let fleche_bas = RECT { left: d - s(34), top: y_tete + s(2),
                                    right: d - s(2), bottom: y_tete + s(22) };
            arrondi(dc, &fleche_haut, 6, LIGNE_FORTE);
            arrondi(dc, &fleche_bas, 6, LIGNE_FORTE);
            let mut rh = fleche_haut;
            let mut rb = fleche_bas;
            texte_echelle(dc, dpi, "Segoe UI", "▲", &mut rh,
                          DT_CENTER | DT_VCENTER | DT_SINGLELINE, TEXTE, 9, false, echelle);
            texte_echelle(dc, dpi, "Segoe UI", "▼", &mut rb,
                          DT_CENTER | DT_VCENTER | DT_SINGLELINE, TEXTE, 9, false, echelle);
            *std::ptr::addr_of_mut!(ZONE_VERSIONS_HAUT) = fleche_haut;
            *std::ptr::addr_of_mut!(ZONE_VERSIONS_BAS) = fleche_bas;

            // 3. Une LIGNE PAR VERSION, depuis le catalogue (jamais en dur).
            let y_liste = y_tete + s(30);
            let fin_liste = (bas_utile - s(64)).max(y_liste);
            let layout = layout_liste(fin_liste - y_liste, nb);
            let liste = RECT { left: g, top: y_liste, right: d,
                               bottom: y_liste + layout.pas * layout.visible as i32 };
            *std::ptr::addr_of_mut!(ZONE_LISTE_VERSIONS) = liste;
            *std::ptr::addr_of_mut!(PAS_VERSIONS) = layout.pas;
            let depart = etat.version_depart
                .min(nb.saturating_sub(layout.visible));
            // Le clavier lit ces valeurs : elles doivent refléter EXACTEMENT
            // ce que le tracé vient de dessiner. On écrit AVANT d'emprunter
            // `etat.catalogue` pour la boucle, pour rester dans les règles
            // d'emprunt Rust (un seul `&mut Etat` visitable à la fois).
            etat.version_visible = layout.visible;
            etat.version_depart = depart;
            if let Some(cat) = etat.catalogue.as_ref() {
                for i in depart..(depart + layout.visible).min(nb) {
                    let e = &cat.versions[i];
                    let (ligne, ib, ub) = zones_ligne_version(&liste, layout.pas, i, depart);
                    let selectionne = i == etat.version_selection;
                    carte(dc, &ligne, if selectionne { VERT_LIME_VIF } else { LIGNE_FORTE });
                    let mut r1 = RECT { left: ligne.left + s(16), top: ligne.top + s(5),
                                        right: ib.left - s(10), bottom: ligne.top + s(5) + s(22) };
                    texte_echelle(dc, dpi, "Segoe UI", &format_ligne_version(e), &mut r1,
                                  DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                                  if selectionne { TEXTE } else { TEXTE_DOUX },
                                  10, selectionne, echelle);
                    let mut r2 = RECT { left: ligne.left + s(16), top: ligne.top + s(5) + s(22),
                                        right: ib.left - s(10), bottom: ligne.bottom - s(4) };
                    texte_echelle(dc, dpi, "Segoe UI", &badges_version(e), &mut r2,
                                  DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                                  TEXTE_DOUX, 8, false, echelle);

                    let actions = actions_version(e, hors);
                    // Bouton d'installation : grisé hors ligne / sans artefact.
                    arrondi(dc, &ib, 8, if actions.installer { VERT_LIME_VIF } else { LIGNE });
                    let mut rib = ib;
                    let libelle_i = if e.installable && hors { "HORS LIGNE" }
                                    else { libelle_installer(e) };
                    texte_echelle(dc, dpi, "Kare", libelle_i, &mut rib,
                                  DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                                  if actions.installer { NOIR_DOUX } else { TEXTE_FAIBLE },
                                  9, false, echelle);
                    // Bascule : uniquement si la version est déjà sur disque.
                    arrondi(dc, &ub, 8, if actions.utiliser { VERT_LIME } else { LIGNE });
                    let mut rub = ub;
                    texte_echelle(dc, dpi, "Kare", "UTILISER", &mut rub,
                                  DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                                  if actions.utiliser { NOIR_DOUX } else { TEXTE_FAIBLE },
                                  9, false, echelle);
                }
            } else {
                let mut rv = RECT { left: g, top: y_liste + s(20), right: d, bottom: fin_liste };
                texte_echelle(dc, dpi, "Segoe UI", "Chargement du catalogue…", &mut rv,
                              DT_CENTER | DT_VCENTER | DT_SINGLELINE, TEXTE_DOUX, 10, false, echelle);
            }

            // 4. Message de la DERNIÈRE ACTION (succès ou échec), jamais muet.
            let statut_rect = RECT { left: g, top: fin_liste + s(8), right: d, bottom: bas_utile };
            if statut_rect.bottom > statut_rect.top {
                carte(dc, &statut_rect,
                      if etat.version_statut_erreur { ROUGE_VIF } else { VERT_LIME_VIF });
                let mut rst = RECT { left: g + s(16), top: statut_rect.top + s(6),
                                     right: d - s(16), bottom: statut_rect.bottom - s(4) };
                texte_echelle(dc, dpi, "Segoe UI", &etat.statut, &mut rst,
                              DT_WORDBREAK | DT_END_ELLIPSIS | DT_TOP,
                              if etat.version_statut_erreur { ROUGE_CLAIR } else { TEXTE },
                              9, false, echelle);
            }
        }
        Vue::Termine => {
            // ⚠ La carte doit contenir TOUT le texte, mesuré : la dernière
            // ligne (« Ouvrez un nouveau terminal, puis tapez : rose
            // --version ») était COUPÉE en bas (mesuré sur la capture).
            let corps = format!("Rose {} est installé.\n\nDossier : {}\n\nOuvrez un nouveau terminal, puis tapez : rose --version",
                                etat.version_installee, etat.prefixe);
            let largeur_corps = (largeur_texte - s(44)).max(s(80));
            let h_corps = crate::paint::hauteur_texte(&corps, largeur_corps, &mesurer(10, false), interligne);
            let h_carte = crate::paint::hauteur_carte(h_corps, s(18), s(140)).min((bas_utile - yc).max(s(140)));
            let c = RECT { left: g, top: yc, right: d, bottom: yc + h_carte };
            // Succès de la mise à jour : VERT LIME VIF, avec la coche antialiasée.
            carte(dc, &c, VERT_LIME_VIF);
            coche(dc, g + s(30), yc + s(30), s(11), VERT_LIME_VIF);
            let mut r = RECT { left: g + s(64), top: yc + s(16), right: d - s(20), bottom: c.bottom - s(14) };
            texte_echelle(dc, dpi, "Segoe UI", &corps, &mut r, DT_WORDBREAK | DT_END_ELLIPSIS, TEXTE, 10, false, echelle);
        }
    }

    // ── pied de page : boutons ──
    let pied = RECT { left: lateral_w + 1, top: ht - etat_h - pied_h, right: w, bottom: ht - etat_h };
    remplir(dc, &pied, LATERAL_FIN);
    let filet_pied = RECT { left: lateral_w + 1, top: pied.top, right: w, bottom: pied.top + 1 };
    remplir(dc, &filet_pied, LIGNE_FORTE);
    let (gauche, droite) = boutons(etat.vue, etat.existant);
    // Thème « rose update » : ROUGE VIF pour l'action principale, VERT LIME VIF
    // pour la progression/le succès. Limité à la fenêtre de mise à jour.
    let theme_update = etat.existant || matches!(etat.vue, Vue::Installation | Vue::Termine);
    // ⚠ Largeur bornée par le pied de page : deux boutons de 190 px sur une
    // fenêtre étroite sortaient par la gauche. On garde au moins la place des
    // marges (28 px de chaque côté).
    let place = (w - lateral_w - s(56)).max(s(120));
    let bw = s(190).min((place - s(20)) / 2).max(s(96));
    let bh = s(44);
    let by = pied.top + (pied_h - bh) / 2;
    let mut zones = [std::mem::zeroed::<RECT>(), std::mem::zeroed::<RECT>(),
                     std::mem::zeroed::<RECT>()];
    if let Some((action, libelle)) = gauche {
        let r = RECT { left: lateral_w + s(28), top: by, right: lateral_w + s(28) + bw, bottom: by + bh };
        // Bouton secondaire : VERT LIME VIF dans la fenêtre d'update,
        // VERT LIME historique ailleurs ; le survol éclaircit.
        let fond = if theme_update {
            if etat.survol == 0 { VERT_LIME_VIF_CLAIR } else { VERT_LIME_VIF }
        } else if etat.survol == 0 { SURVOL_SOMBRE } else { REPOS_SOMBRE };
        arrondi(dc, &r, 10, fond);
        contour(dc, &r, 10, fond);
        let mut rt = RECT { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
        // Libellés des boutons en Kare MAJUSCULE (comme `.btn` du site) :
        // Kare dessine ses minuscules comme des capitales, l'uppercase est
        // donc la seule forme cohérente.
        // ⚠ Sur un fond vert lime CLAIR, le texte doit être SOMBRE : du texte
        // clair (TEXTE) y serait illisible. `NOIR_DOUX` donne le contraste.
        let teinte = if action == Action::Desinstaller { ROUGE_SOMBRE } else { NOIR_DOUX };
        // Kare n'a PAS de graisse grasse : GDI la fabrique en « smearing »,
        // ce qui rend les glyphes pâteux (mesuré sur l'aperçu). On garde donc
        // le poids normal et on AGrandit le corps (13 pt) : les traits fins
        // restent lisibles sur le vert lime clair.
        texte_echelle(dc, dpi, "Kare", &libelle.to_uppercase(), &mut rt,
              DT_CENTER | DT_VCENTER | DT_SINGLELINE, teinte, 13, false, echelle);
        zones[0] = r;
        let _ = action;
    }
    if let Some((_, libelle)) = droite {
        let r = RECT { left: w - s(28) - bw, top: by, right: w - s(28), bottom: by + bh };
        let couleur = if etat.survol == 1 { ROUGE_CLAIR } else { ROUGE_VIF };
        arrondi(dc, &r, 10, couleur);
        let mut rt = RECT { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
        texte_echelle(dc, dpi, "Kare", &libelle.to_uppercase(), &mut rt,
              DT_CENTER | DT_VCENTER | DT_SINGLELINE, BLANC, 11, false, echelle);
        zones[1] = r;
    }

    // ── barre d'état ──
    let etat_barre = RECT { left: 0, top: ht - etat_h, right: w, bottom: ht };
    // Coins BAS de la fenêtre : même rayon que le fond, sinon la barre d'état
    // repeignait les coins arrondis du fond en carré.
    if !arrondi_bas(dc, &etat_barre, 12.0, BARRE) {
        remplir(dc, &etat_barre, BARRE);
    }
    remplir(dc, &RECT { left: 0, top: etat_barre.top, right: w, bottom: etat_barre.top + 1 }, LIGNE_FORTE);
    cercle(dc, s(16), ht - etat_h / 2, s(5), if etat.empreinte.is_some() { VERT } else { TEXTE_FAIBLE });
    // Statut de la mise à jour dans le thème : vert lime vif en succès,
    // rouge vif en échec, gris sinon (les autres vues ne changent pas).
    let couleur_statut = if theme_update && etat.vue == Vue::Termine {
        VERT_LIME_VIF
    } else if theme_update && etat.statut.starts_with("ÉCHEC") {
        ROUGE_VIF
    } else {
        TEXTE_DOUX
    };
    let mut rg = RECT { left: s(28), top: etat_barre.top, right: w / 2 - s(10), bottom: etat_barre.bottom };
    texte_echelle(dc, dpi, "Segoe UI", &etat.statut, &mut rg, DT_VCENTER | DT_SINGLELINE, couleur_statut, 9, false, echelle);
    let mut rd = RECT { left: w / 2 + s(10), top: etat_barre.top, right: w - s(110), bottom: etat_barre.bottom };
    texte_echelle(dc, dpi, "Segoe UI",
          &format!("Rose-win-Setup.exe · Windows x64 · © 2026 Stef302"),
          &mut rd, DT_VCENTER | DT_SINGLELINE | DT_RIGHT, TEXTE_FAIBLE, 9, false, echelle);
    // ⚠ Bouton « PATH » dans la barre d'état : ouvre l'outil de diagnostic.
    // Il est TOUJOURS là, y compris sur l'écran d'échec — c'est le seul
    // moyen pour l'utilisateur de régler un PATH verrouillé.
    {
        let r = RECT { left: w - s(100), top: etat_barre.top + s(3), right: w - s(8), bottom: etat_barre.bottom - s(3) };
        let fond = if etat.survol == 2 { VERT_LIME_CLAIR } else { VERT_LIME };
        arrondi(dc, &r, 6, fond);
        let mut rt = RECT { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
        texte_echelle(dc, dpi, "Kare", "PATH", &mut rt, DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                      NOIR_DOUX, 10, false, echelle);
        zones[2] = r;
    }

    // garder les zones pour le clic
    ZONES = zones;
    // copie du tampon mémoire vers la fenêtre, LIMITÉE à la zone invalidée,
    // puis libération. Copier tout le tampon à chaque survol de bouton était
    // la cause du clignotement.
    BitBlt(dc_ecran, zone.left, zone.top,
           zone.right - zone.left, zone.bottom - zone.top,
           dc_memoire, zone.left, zone.top, SRCCOPY);
    SelectObject(dc_memoire, ancien_bitmap);
    DeleteObject(bitmap as *mut c_void);
    DeleteDC(dc_memoire);
}

/// ⚠ ZONES[2] = bouton « PATH » de la barre d'état : il ouvre l'outil
/// de diagnostic/réparation du PATH. Il est TOUJOURS visible, y compris
/// quand l'installation a échoué — c'est précisément là qu'on en a besoin.
static mut ZONES: [RECT; 3] = [
    RECT { left: 0, top: 0, right: 0, bottom: 0 },
    RECT { left: 0, top: 0, right: 0, bottom: 0 },
    RECT { left: 0, top: 0, right: 0, bottom: 0 },
];

/// Bords gauches des trois boutons de la barre de titre (réduire, agrandir,
/// fermer), remplis par `dessiner`.
/// Zone cliquable du bouton « Ouvrir l'outil PATH » sur l'écran d'échec.
static mut ZONES_ZONE_PATH: RECT = RECT { left: 0, top: 0, right: 0, bottom: 0 };

/// Zone cliquable de la carte des MÉTHODES (vue PATH).
static mut ZONES_METHODES: RECT = RECT { left: 0, top: 0, right: 0, bottom: 0 };

static mut ZONES_TITRE: [i32; 3] = [0, 0, 0];

/// Zones cliquables des TROIS cartes du premier écran (choix du mode).
static mut ZONES_MAJ: [RECT; 3] = [
    RECT { left: 0, top: 0, right: 0, bottom: 0 },
    RECT { left: 0, top: 0, right: 0, bottom: 0 },
    RECT { left: 0, top: 0, right: 0, bottom: 0 },
];
/// Bouton unique de la ligne de statut du premier écran : « Vérifier les
/// mises à jour » (mode manuel) puis « Télécharger » (consentement).
static mut ZONE_MAJ_ACTION: RECT = RECT { left: 0, top: 0, right: 0, bottom: 0 };

/* ── Sélecteur de versions (LAUNCH2) : zones posées par le TRACÉ ────────
   Une seule source de géométrie : `zones_ligne_version` (fonction pure).
   Le tracé remplit ces zones, le hit-test de WM_LBUTTONUP les relit —
   exactement comme `ZONES_MAJ` le fait pour le premier écran.
   ───────────────────────────────────────────────────────────────────────── */
/// Bouton « VERSIONS » du premier écran / des Options. Remis à zéro à chaque
/// tracé : il n'est cliquable QUE sur les vues où il est réellement dessiné.
static mut ZONE_OUVRIR_VERSIONS: RECT = RECT { left: 0, top: 0, right: 0, bottom: 0 };
/// Rectangle de la liste et hauteur d'une ligne (recalcul pure).
static mut ZONE_LISTE_VERSIONS: RECT = RECT { left: 0, top: 0, right: 0, bottom: 0 };
static mut PAS_VERSIONS: i32 = 0;
/// Flèches de défilement de la liste (souris ; le clavier utilise ↑/↓).
static mut ZONE_VERSIONS_HAUT: RECT = RECT { left: 0, top: 0, right: 0, bottom: 0 };
static mut ZONE_VERSIONS_BAS: RECT = RECT { left: 0, top: 0, right: 0, bottom: 0 };

/// Suivi des opérations réseau du premier écran, rempli par les fils de
/// travail et lu par `dessiner`. `None` = pas encore lancée / pas de résultat.
struct SuiviMaj {
    verifie_en_cours: bool,
    verification: Option<Result<Option<String>, String>>,
    telecharge_en_cours: bool,
    telechargement: Option<Result<Option<String>, String>>,
}
static SUIVI_MAJ: std::sync::Mutex<SuiviMaj> = std::sync::Mutex::new(SuiviMaj {
    verifie_en_cours: false,
    verification: None,
    telecharge_en_cours: false,
    telechargement: None,
});

/// Lance une vérification en tâche de fond.
///
/// `explicite = false` : vérification AU DÉMARRAGE (mode « Auto » seulement,
/// garde-fou dans `velopack::verifier_au_demarrage`) — ne télécharge RIEN.
/// `explicite = true` : bouton « Vérifier les mises à jour » (mode manuel).
fn lancer_verification_maj(h: HWND, explicite: bool) {
    {
        let mut g = match SUIVI_MAJ.lock() { Ok(g) => g, Err(_) => return };
        if g.verifie_en_cours || g.telecharge_en_cours { return; }
        g.verifie_en_cours = true;
        g.verification = None;
        g.telechargement = None;
    }
    let cible = h as isize;
    std::thread::spawn(move || {
        let resultat = if explicite {
            crate::velopack::verifier_manifeste()
        } else {
            crate::velopack::verifier_au_demarrage()
        };
        if let Ok(mut g) = SUIVI_MAJ.lock() {
            g.verification = Some(resultat);
            g.verifie_en_cours = false;
        }
        let h = cible as HWND;
        if !h.is_null() { unsafe { SendMessageW(h, WM_APP_MAJ, 0, 0); } }
    });
}

/// Consentement explicite de l'utilisateur : télécharge effectivement la
/// version trouvée. En mode « désactivé », `check_and_update` refuse avec
/// un message lisible dans la ligne de statut.
fn lancer_telechargement_maj(h: HWND) {
    {
        let mut g = match SUIVI_MAJ.lock() { Ok(g) => g, Err(_) => return };
        if g.verifie_en_cours || g.telecharge_en_cours { return; }
        g.telecharge_en_cours = true;
    }
    let cible = h as isize;
    std::thread::spawn(move || {
        let resultat = crate::velopack::check_and_update(false);
        if let Ok(mut g) = SUIVI_MAJ.lock() {
            g.telechargement = Some(resultat);
            g.telecharge_en_cours = false;
        }
        let h = cible as HWND;
        if !h.is_null() { unsafe { SendMessageW(h, WM_APP_MAJ, 0, 0); } }
    });
}

fn dans(r: &RECT, x: i32, y: i32) -> bool {
    r.right > r.left && x >= r.left && x <= r.right && y >= r.top && y <= r.bottom
}

/// Curseurs de stock, chargés UNE SEULE FOIS.
///
/// ⚠ `LoadCursorW` à chaque `WM_MOUSEMOVE` coûtait un appel système par
/// pixel de souris déplacée : c'est une des causes du clignotement. Les
/// curseurs de stock sont partagés (`LR_SHARED`) et restent valides pour
/// toute la durée du processus, on peut donc les garder.
fn curseur_fleche() -> HCURSOR {
    static mut F: HCURSOR = std::ptr::null_mut();
    unsafe {
        if F.is_null() {
            F = LoadCursorW(std::ptr::null_mut(), IDC_ARROW as *const u16);
        }
        F
    }
}

fn curseur_main() -> HCURSOR {
    static mut M: HCURSOR = std::ptr::null_mut();
    unsafe {
        if M.is_null() {
            M = LoadCursorW(std::ptr::null_mut(), IDC_HAND as *const u16);
        }
        M
    }
}

/// Zone de la barre de titre : -1 hors barre, 0 = déplacement, 1 = réduire,
/// 2 = agrandir, 3 = fermer.
/// Les bornes suivent l'échelle du tracé (barre de 56 px sur la grille
/// 1120 × 780), sinon les zones cliquables ne correspondent plus au dessin
/// quand la fenêtre est redimensionnée.
///
/// ⚠⚠ `ZONES_TITRE` n'est rempli que par `dessiner` : au TOUT PREMIER
/// `WM_NCHITTEST` (souris qui entre dans la fenêtre avant le premier
/// repeint), il vaut encore `[0, 0, 0]`. L'ancien code retournait alors 3
/// pour TOUTE la barre — la croix de fermeture. Mesuré : un simple clic dans
/// la barre de titre FERMAIT l'installeur (la fenêtre « disparaissait »).
/// On ne se fie aux zones que si elles sont cohérentes avec la largeur
/// courante ; sinon on les recalcule.
fn zone_titre(w: i32, h: i32, x: i32, y: i32) -> i32 {
    let echelle = crate::paint::echelle_tracé(w, h);
    let s = |v: i32| -> i32 { (v as f64 * echelle).round() as i32 };
    if y > s(56) {
        return -1;
    }
    let bl = s(46);
    let (b_reduire, b_agrandir, b_fermer) = (w - 3 * bl, w - 2 * bl, w - bl);
    let zones = unsafe { *std::ptr::addr_of!(ZONES_TITRE) };
    let coherentes = zones[2] == b_fermer && zones[1] == b_agrandir && zones[0] == b_reduire;
    let (z_reduire, z_agrandir, z_fermer) = if coherentes {
        (zones[0], zones[1], zones[2])
    } else {
        // Barre pas encore peinte : on calcule sur la géométrie actuelle.
        (b_reduire, b_agrandir, b_fermer)
    };
    if x >= z_fermer {
        3
    } else if x >= z_agrandir {
        2
    } else if x >= z_reduire {
        1
    } else {
        0 // zone de déplacement (HTCAPTION)
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
            // ⚠ NE PAS écrire `etat.maximise = w == 2`. L'agrandissement est
            // fait nous-mêmes par `SetWindowPos` sur la zone de travail (voir
            // WM_LBUTTONUP) : la fenêtre reste alors en SIZE_RESTORED (0) et
            // ce test remettait `maximise` à `false` juste après le clic,
            // d'où une fenêtre impossible à restaurer et un ratio qui se
            // réappliquait au redimensionnement.
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
        WM_PAINT => {
            // ⚠ `dessiner` fait DÉJÀ son propre `catch_unwind` : on garde ici
            // une seconde ceinture, car une panique dans le `wnd_proc` lui-même
            // (avant d'entrer dans `dessiner`) remonterait dans la boucle de
            // messages et tuerait le processus.
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe { dessiner(h) })).is_err() {
                eprintln!("rose-setup : panique pendant WM_PAINT — la fenêtre reste ouverte");
            }
            0
        }
        WM_ERASEBKGND => {
            // ⚠⚠ CAUSE DIRECTE DU CLIGNOTEMENT. Cette branche peignait le
            // fond, puis `WM_PAINT` repeignait par-dessus : la fenêtre était
            // remplie DEUX fois à chaque repeint, et comme les deux
            // remplissages ne sont pas atomiques, on voyait un flash.
            //
            // `WM_PAINT` peint DÉJÀ le fond (`remplir(dc_ecran, …)`) avant
            // toute décoration : effacer ici est donc INUTILE et nuisible.
            // Rendre 1 (valeur non nulle = « effacé ») fait comprendre à
            // Windows que le fond est traité et qu'il ne doit rien faire.
            // C'est la technique standard pour une fenêtre à double tampon.
            1
        }
        WM_SETCURSOR => {
            // Windows demande le curseur à chaque déplacement : sans cette
            // branche, `DefWindowProc` recharge le curseur de la CLASSE à
            // chaque fois, ce qui provoque un scintillement du pointeur.
            // On répond nous-mêmes, avec le curseur déjà en cache.
            let mut rc = std::mem::zeroed::<RECT>();
            GetClientRect(h, &mut rc);
            let zones = &*std::ptr::addr_of!(ZONES);
            let mut pos = POINT { x: 0, y: 0 };
            GetCursorPos(&mut pos);
            ScreenToClient(h, &mut pos);
            let zo = &*std::ptr::addr_of!(ZONE_OUVRIR_VERSIONS);
            let zl = &*std::ptr::addr_of!(ZONE_LISTE_VERSIONS);
            let zh = &*std::ptr::addr_of!(ZONE_VERSIONS_HAUT);
            let zb = &*std::ptr::addr_of!(ZONE_VERSIONS_BAS);
            let survol = dans(&zones[0], pos.x, pos.y)
                || dans(&zones[1], pos.x, pos.y)
                || dans(zo, pos.x, pos.y)
                || ((*ETAT).vue == Vue::Versions
                    && (dans(zl, pos.x, pos.y) || dans(zh, pos.x, pos.y) || dans(zb, pos.x, pos.y)))
                || zone_titre(rc.right, rc.bottom, pos.x, pos.y) > 0;
            SetCursor(if survol { curseur_main() } else { curseur_fleche() });
            1 // traité : ne pas laisser DefWindowProc recharger le curseur
        }
        WM_TIMER => {
            let etat = &mut *ETAT;
            if w == TIMER_PROGRESSION && etat.occupe {
                etat.phase += 20;
                // ⚠⚠ CAUSE DU « GLITCH » PENDANT L'INSTALLATION. Cette
                // branche invalidait TOUTE la fenêtre toutes les 16 ms : 62
                // repeints complets par seconde (décodage PNG, mesures GDI,
                // polices recréées), ce qui saturait le fil de messages et
                // empêchait l'animation de suivre. On n'invalide QUE la
                // piste de progression.
                //
                // Sa géométrie est la même que dans `dessiner` : sous la
                // carte de statut, dans la zone de contenu.
                let mut rc = std::mem::zeroed::<RECT>();
                GetClientRect(h, &mut rc);
                let echelle = crate::paint::echelle_tracé(rc.right, rc.bottom);
                let s = |v: i32| -> i32 { (v as f64 * echelle).round() as i32 };
                let lateral_w = s(320);
                let pied_h = s(76);
                let etat_h = s(30);
                let contenu = RECT {
                    left: lateral_w + 1,
                    top: s(56),
                    right: rc.right,
                    bottom: rc.bottom - etat_h - pied_h,
                };
                let g = contenu.left + s(56);
                let d = contenu.right - s(56);
                // La piste est sous la carte de statut : on couvre largement
                // (statut + piste) pour ne jamais laisser de résidu.
                let piste = RECT {
                    left: g - s(2),
                    top: contenu.top + s(44) + s(52) + s(28),
                    right: d + s(2),
                    bottom: contenu.bottom - s(20),
                };
                if piste.right > piste.left && piste.bottom > piste.top {
                    InvalidateRect(h, &piste, 0);
                } else {
                    InvalidateRect(h, std::ptr::null(), 0);
                }
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
            // ⚠ `lParam` de `WM_NCHITTEST` est en coordonnées ÉCRAN : la
            // conversion est ici LÉGITIME (contrairement à `WM_MOUSEMOVE` et
            // `WM_LBUTTONUP`, qui sont déjà en coordonnées client).
            let mut pos = POINT { x: (l as i32) << 16 >> 16, y: (l as i32) >> 16 };
            ScreenToClient(h, &mut pos);
            let (x, y) = (pos.x, pos.y);
            // Même échelle que le tracé : une échelle nulle (fenêtre pas
            // encore dimensionnée) donnait `bord = 4` partout, et les zones
            // de redimensionnement ne correspondaient plus au dessin.
            let echelle = crate::paint::echelle_tracé(rc.right, rc.bottom);
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
            // ⚠⚠ CAUSE DU CLIGNOTEMENT MESURÉ. Trois défauts se cumulaient :
            //
            // 1. `GetCursorPos` + `ScreenToClient` à CHAQUE mouvement, alors
            //    que `lParam` contient DÉJÀ les coordonnées client (comme
            //    pour `WM_LBUTTONUP`). Le calcul était donc juste, mais il
            //    coûtait deux appels système par pixel de souris ;
            // 2. `LoadCursorW` à chaque mouvement : deux appels système de
            //    plus par pixel de souris ;
            // 3. `InvalidateRect(h, NULL, 0)` dès que le survol changeait :
            //    le curseur entrait et sortait d'une zone à chaque pixel, ce
            //    qui invalidait TOUTE la fenêtre. `WM_PAINT` redessinait donc
            //    1120 × 780 pixels — décodage PNG, mesures GDI, polices
            //    recréées — des dizaines de fois par seconde : la fenêtre
            //    clignotait.
            //
            // Correction : coordonnées lues dans `lParam`, curseurs chargés
            // UNE FOIS, et surtout on n'invalide QUE les zones dont
            // l'apparence change (les deux boutons du pied de page et les
            // trois de la barre de titre), jamais la fenêtre entière.
            let mut rc = std::mem::zeroed::<RECT>();
            GetClientRect(h, &mut rc);
            // `lParam` de WM_MOUSEMOVE : coordonnées CLIENT (x = LOWORD,
            // y = HIWORD), signées.
            let x = (l as i32) << 16 >> 16;
            let y = (l as i32) >> 16;
            let zones = &*std::ptr::addr_of!(ZONES);
            let mut z = -1;
            if dans(&zones[0], x, y) { z = 0; }
            if dans(&zones[1], x, y) { z = 1; }
            // ⚠ La zone 2 est le bouton « PATH » de la barre d'état.
            if dans(&zones[2], x, y) { z = 2; }
            let zt = match zone_titre(rc.right, rc.bottom, x, y) {
                1 => 0,
                2 => 1,
                3 => 2,
                _ => -1,
            };
            let etat = &mut *ETAT;
            let mut ancien_z = etat.survol;
            let mut ancien_zt = etat.survol_titre;
            let mut change = false;
            if z != ancien_z { etat.survol = z; change = true; }
            if zt != ancien_zt { etat.survol_titre = zt; change = true; }
            if change {
                // Repeint CIBLÉ : seulement les boutons concernés.
                if ancien_z != z {
                    for i in [ancien_z, z] {
                        if i >= 0 && (i as usize) < zones.len() {
                            InvalidateRect(h, &zones[i as usize], 0);
                        }
                    }
                }
                if ancien_zt != zt {
                    // Zones de la barre de titre : réduire, agrandir, fermer.
                    let bl = (rc.right as f64 * (46.0 / 1120.0)).round() as i32;
                    let mut zt_rect = std::mem::zeroed::<RECT>();
                    for i in [ancien_zt, zt] {
                        if i < 0 { continue; }
                        let gauche = rc.right - (3 - i as i32) * bl;
                        zt_rect = RECT { left: gauche, top: 0, right: gauche + bl, bottom: rc.bottom.min(56) };
                        InvalidateRect(h, &zt_rect, 0);
                    }
                }
            }
            // Curseurs mis en cache : `LoadCursorW` à chaque mouvement de
            // souris était un appel système inutile par pixel.
            let curseur = if z >= 0 || zt >= 0 { curseur_main() } else { curseur_fleche() };
            SetCursor(curseur);
            0
        }
        WM_LBUTTONUP => {
            // ⚠ `lParam` de WM_LBUTTONUP contient DÉJÀ des coordonnées
            // CLIENT (relatives au contenu de la fenêtre). La première
            // version les passait à `ScreenToClient`, qui les décalait une
            // seconde fois du coin de la fenêtre : aucun bouton ne
            // répondait (Suivant, Annuler et la croix inertes), alors que le
            // survol fonctionnait (WM_MOUSEMOVE utilise GetCursorPos, donc
            // des coordonnées écran légitimes).
            let mut rc = std::mem::zeroed::<RECT>();
            GetClientRect(h, &mut rc);
            let x = (l & 0xffff) as i32;
            let y = ((l >> 16) & 0xffff) as i32;
            let (x, y) = (x, y);
            match zone_titre(rc.right, rc.bottom, x, y) {
                3 => { PostQuitMessage(0); return 0; }
                2 => {
                    // Agrandir / restaurer.
                    //
                    // ⚠ `ShowWindow(h, SW_MAXIMIZE)` sur une fenêtre
                    // `WS_POPUP` SANS `WS_CAPTION` fait DISPARAÎTRE la
                    // fenêtre (mesuré) : Windows la redimensionne sur la
                    // zone maximisée du cadre, qui n'existe pas ici, et la
                    // fenêtre sort de l'écran sans jamais se fermer.
                    // On redimensionne donc NOUS-MÊMES sur la zone de
                    // travail du moniteur, ce qui est aussi plus sûr avec
                    // la barre des tâches.
                    let etat = &mut *ETAT;
                    if etat.maximise {
                        // Restaurer : on revient à la taille initiale
                        // (1120 × 780 à l'échelle du DPI), centrée.
                        etat.maximise = false;
                        let facteur = (if etat.dpi > 0 { etat.dpi } else { 96 }) as f64 / 96.0;
                        let l_w = (1120.0 * facteur).round() as i32;
                        let l_h = (780.0 * facteur).round() as i32;
                        let mut ecran = MONITORINFO {
                            cb_size: std::mem::size_of::<MONITORINFO>() as u32,
                            rc_moniteur: std::mem::zeroed(),
                            rc_travail: std::mem::zeroed(),
                            drapeaux: 0,
                        };
                        let (mut x, mut y) = (0, 0);
                        if GetMonitorInfoW(MonitorFromWindow(h, 2), &mut ecran) != 0 {
                            let z = ecran.rc_travail;
                            x = z.left + ((z.right - z.left) - l_w) / 2;
                            y = z.top + ((z.bottom - z.top) - l_h) / 2;
                        }
                        SetWindowPos(h, HWND_TOP, x, y, l_w, l_h,
                                     SWP_NOZORDER | SWP_FRAMECHANGED);
                    } else {
                        // Agrandir : toute la zone de travail, jamais plus.
                        etat.maximise = true;
                        let mut ecran = MONITORINFO {
                            cb_size: std::mem::size_of::<MONITORINFO>() as u32,
                            rc_moniteur: std::mem::zeroed(),
                            rc_travail: std::mem::zeroed(),
                            drapeaux: 0,
                        };
                        if GetMonitorInfoW(MonitorFromWindow(h, 2), &mut ecran) != 0 {
                            let z = ecran.rc_travail;
                            SetWindowPos(h, HWND_TOP, z.left, z.top,
                                         z.right - z.left, z.bottom - z.top,
                                         SWP_NOZORDER | SWP_FRAMECHANGED);
                        }
                    }
                    InvalidateRect(h, std::ptr::null(), 0);
                    return 0;
                }
                1 => { ShowWindow(h, SW_MINIMIZE); return 0; }
                // ⚠ 0 = zone de DÉPLACEMENT : Windows a déjà déplacé la
                // fenêtre (HTCAPTION). Il ne faut RIEN faire d'autre —
                // surtout pas la fermer. L'ancien `_ => {}` laissait le code
                // tomber sur les zones du pied de page : si le clic avait des
                // coordonnées qui correspondaient à « Annuler », la fenêtre
                // se fermait. On sort explicitement.
                0 => return 0,
                _ => {}
            }
            let zones = &*std::ptr::addr_of!(ZONES);
            // Bouton « VERSIONS » (premier écran / Options). Sa zone est
            // remise à zéro par le tracé sur toutes les autres vues.
            let zo = &*std::ptr::addr_of!(ZONE_OUVRIR_VERSIONS);
            if dans(zo, x, y) {
                ouvrir_versions(h);
                return 0;
            }
            // ⚠ Sur la vue PATH, un clic dans la carte des méthodes CHOISIT
            // la méthode (pas de bouton par méthode : la carte est
            // cliquable ligne par ligne).
            let etat = &mut *ETAT;
            // ── Sélecteur de versions : clic dans la liste / actions ──────
            if etat.vue == Vue::Versions {
                let zh = &*std::ptr::addr_of!(ZONE_VERSIONS_HAUT);
                let zb = &*std::ptr::addr_of!(ZONE_VERSIONS_BAS);
                if dans(zh, x, y) { defiler_versions(h, -1); return 0; }
                if dans(zb, x, y) { defiler_versions(h, 1); return 0; }
                let liste = *std::ptr::addr_of!(ZONE_LISTE_VERSIONS);
                let pas = *std::ptr::addr_of!(PAS_VERSIONS);
                let nb = etat.catalogue.as_ref().map(|c| c.versions.len()).unwrap_or(0);
                if dans(&liste, x, y) && nb > 0 && pas > 0 {
                    let nb_visible = etat.version_visible.max(1);
                    let depart = etat.version_depart.min(nb.saturating_sub(nb_visible));
                    let ligne_rel = ((y - liste.top) / pas) as usize;
                    let i = (depart + ligne_rel).min(nb - 1);
                    let (r_ligne, r_installer, r_utiliser) =
                        zones_ligne_version(&liste, pas, i, depart);
                    let (installer, utiliser) = {
                        let cat = etat.catalogue.as_ref().unwrap();
                        let a = actions_version(&cat.versions[i], cat.hors_ligne);
                        (a.installer, a.utiliser)
                    };
                    if dans(&r_installer, x, y) && installer {
                        etat.version_selection = i;
                        action_version_installer(h);
                        return 0;
                    }
                    if dans(&r_utiliser, x, y) && utiliser {
                        etat.version_selection = i;
                        action_version_utiliser(h);
                        return 0;
                    }
                    if dans(&r_ligne, x, y) {
                        etat.version_selection = i;
                        rafraichir(h);
                        return 0;
                    }
                }
                // Sinon : laisser tomber sur les boutons du pied de page.
            }
            // ── Premier écran : 3 cartes radio + bouton d'action ────────
            if etat.vue == Vue::MisesAJour {
                let z_maj = &*std::ptr::addr_of!(ZONES_MAJ);
                for (i, z) in z_maj.iter().enumerate() {
                    if dans(z, x, y) {
                        etat.mode_maj = match i {
                            0 => crate::velopack::ModeMiseAJour::Auto,
                            1 => crate::velopack::ModeMiseAJour::Manuel,
                            _ => crate::velopack::ModeMiseAJour::Desactive,
                        };
                        {
                            let mut g = SUIVI_MAJ.lock().unwrap_or_else(|e| e.into_inner());
                            g.verification = None;
                            g.telechargement = None;
                        }
                        // Rejoindre le mode « Auto » relance la vérification
                        // au démarrage ; les deux autres n'appellent RIEN.
                        if etat.mode_maj == crate::velopack::ModeMiseAJour::Auto {
                            lancer_verification_maj(h, false);
                        }
                        rafraichir(h);
                        return 0;
                    }
                }
                let za = &*std::ptr::addr_of!(ZONE_MAJ_ACTION);
                if dans(za, x, y) {
                    let suiv = SUIVI_MAJ.lock().unwrap_or_else(|e| e.into_inner());
                    // « Télécharger » n'apparaît qu'après une version trouvée ;
                    // sinon le bouton est « Vérifier les mises à jour ».
                    let consentement = suiv.telechargement.is_none()
                        && suiv.verification.as_ref()
                            .map(|r| matches!(r, Ok(Some(_)))).unwrap_or(false);
                    drop(suiv);
                    if consentement { lancer_telechargement_maj(h); }
                    else { lancer_verification_maj(h, true); }
                    rafraichir(h);
                    return 0;
                }
                // Sinon : laisser tomber sur les boutons du pied de page
                // (« Annuler » / « Continuer »).
            }
            // ⚠ Rust 2024 interdit `&STATIC_MUT` : on passe par `addr_of!`.
            let zm = &*std::ptr::addr_of!(ZONES_METHODES);
            if etat.vue == Vue::Path && dans(zm, x, y) {
                let n = crate::setup_core::METHODES_PATH.len();
                let h_carte = (zm.bottom - zm.top).max(1);
                let ligne = ((y - zm.top) * n as i32 / h_carte) as usize;
                etat.methode = ligne.min(n - 1);
                rafraichir(h);
                return 0;
            }
            let zp = &*std::ptr::addr_of!(ZONES_ZONE_PATH);
            if dans(zp, x, y) { action_path(h); }
            else if dans(&zones[2], x, y) { action_path(h); }
            else if dans(&zones[0], x, y) { action(h, 0); }
            else if dans(&zones[1], x, y) { action(h, 1); }
            0
        }
        // ── Clavier : navigation dans le sélecteur de versions ───────────
        // Le gestionnaire de messages n'avait aucun WM_KEYDOWN : on l'ajoute
        // SANS toucher au hit-test existant. ↑/↓ = ligne, PgUp/PgDn = page,
        // Début/Fin = extrémités, Entrée = action autorisée, Échap = Retour.
        WM_KEYDOWN => {
            let cle = (w as u32) & 0xffff;
            if (*ETAT).vue == Vue::Versions {
                let nb = (*ETAT).catalogue.as_ref().map(|c| c.versions.len()).unwrap_or(0);
                match cle {
                    VK_UP => { deplacer_selection_versions(h, -1); return 0; }
                    VK_DOWN => { deplacer_selection_versions(h, 1); return 0; }
                    VK_PRIOR => {
                        let page = (*ETAT).version_visible.max(1) as i32;
                        deplacer_selection_versions(h, -page); return 0;
                    }
                    VK_NEXT => {
                        let page = (*ETAT).version_visible.max(1) as i32;
                        deplacer_selection_versions(h, page); return 0;
                    }
                    VK_HOME => { deplacer_selection_versions(h, -(nb as i32)); return 0; }
                    VK_END => { deplacer_selection_versions(h, nb as i32); return 0; }
                    VK_RETURN if nb > 0 => {
                        // Bascule prioritaire : aucun réseau. Sinon installer.
                        if tag_action_versions(false).is_some() {
                            action_version_utiliser(h);
                        } else {
                            action_version_installer(h);
                        }
                        return 0;
                    }
                    VK_ESCAPE => { retour_versions(h); return 0; }
                    _ => {}
                }
            }
            DefWindowProcW(h, msg, w, l)
        }
        WM_APP_MAJ => {
            // Une vérification / un téléchargement du premier écran est
            // terminé (tâche de fond) : on repeint pour afficher le statut.
            rafraichir(h);
            0
        }
        WM_APP_FIN => {
            let etat = &mut *ETAT;
            etat.occupe = false;
            // ⚠ w == 2 : c'est la fin d'une RÉPARATION PATH, pas d'une
            // installation. On relit le diagnostic et on reste sur l'outil.
            if w == 2 {
                etat.journal_path = JOURNAL_PATH.lock().ok().map(|g| g.clone()).unwrap_or_default();
                etat.diagnostic = Some(crate::setup_core::diagnostiquer_path(&etat.prefixe));
                unsafe { rafraichir(h); }
                return 0;
            }
            let desinstalle = w == 1;
            // ⚠ LIRE LE JOURNAL pour savoir si l'opération a ÉCHOUÉ.
            // `WM_APP_FIN` affichait TOUJOURS « Vue::Termine » avec « Rose X
            // est installé », même quand le journal ne contenait que des
            // « Échec : … » (mesuré : l'utilisateur voyait « Terminé » alors
            // qu'aucun fichier n'avait été posé, ni PATH, ni association).
            let journal: Vec<String> = JOURNAL
                .lock()
                .ok()
                .map(|g| g.clone())
                .unwrap_or_default();
            let echec = journal.iter().any(|l| l.starts_with("Échec"));
            let dernier = STATUT
                .lock()
                .ok()
                .and_then(|g| g.clone())
                .unwrap_or_else(|| {
                    if desinstalle { "Rose a été désinstallé.".to_string() }
                    else { "Installation terminée · prêt.".to_string() }
                });

            if echec {
                // On RESTE sur la vue « Installation » pour montrer l'erreur,
                // et on remet les étapes à leur état réel : l'installation
                // n'a PAS eu lieu.
                etat.vue = Vue::Installation;
                etat.atteintes[3] = false;
                etat.atteintes[4] = false;
                etat.statut = format!(
                    "ÉCHEC — {}\n\n{}",
                    dernier,
                    journal.iter().filter(|l| l.starts_with("Échec"))
                        .cloned().collect::<Vec<_>>().join("\n")
                );
            } else {
                etat.atteintes[3] = true;
                etat.atteintes[4] = true;
                etat.vue = Vue::Termine;
                if desinstalle {
                    etat.existant = false;
                    etat.statut = dernier;
                } else {
                    etat.existant = true;
                    // ⚠ RELIRE la version installée APRÈS l'opération.
                    // `version_installee` est figé au démarrage : il
                    // contenait la version de l'ANCIENNE installation
                    // (mesuré : « Rose 5.12.0 est installé » alors que le
                    // setup venait d'installer 0.1.5).
                    etat.version_installee =
                        crate::setup_core::version_installee(&etat.prefixe)
                            .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
                    etat.statut = dernier;
                }
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
        Action::Continuer => {
            // Premier écran : PERSISTANCE du choix avant toute installation.
            match crate::velopack::ecrire_config(etat.mode_maj) {
                Ok(chemin) => {
                    etat.statut = format!("Mode « {} » enregistré dans {}",
                        etat.mode_maj.comme_str(), chemin.display());
                }
                Err(e) => {
                    // On n'empêche PAS l'installation si l'écriture échoue
                    // (profil verrouillé) : le défaut reste appliqué, et
                    // l'erreur est visible dans la barre d'état.
                    etat.statut = format!("Configuration non enregistrée : {e}");
                }
            }
            etat.vue = Vue::Bienvenue;
            unsafe { rafraichir(h); }
        }
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
        Action::OutilPath => {
            // ⚠ On CALCULE le diagnostic AVANT d'afficher : l'utilisateur
            // doit voir l'état réel, pas un écran vide.
            etat.diagnostic = Some(crate::setup_core::diagnostiquer_path(&etat.prefixe));
            etat.journal_path.clear();
            etat.vue = Vue::Path;
            unsafe { rafraichir(h); }
        }
        Action::FermerOutil => {
            etat.vue = Vue::Termine;
            unsafe { rafraichir(h); }
        }
        Action::ReparerPath => {
            let prefixe = etat.prefixe.clone();
            let methode = crate::setup_core::METHODES_PATH[
                etat.methode.min(crate::setup_core::METHODES_PATH.len() - 1)];
            etat.journal_path = vec![format!("Application de « {} »…", methode.nom())];
            unsafe { rafraichir(h); }
            // ⚠⚠ La réparation passe par l'API du registre : c'est rapide
            // (quelques millisecondes), mais une GPO ou un antivirus peut
            // la ralentir. On la fait donc dans un fil pour ne pas figer
            // la fenêtre, et on relit le diagnostic en arrivant.
            let cible = h as isize;
            std::thread::spawn(move || {
                let journal = crate::setup_core::reparer_path(&prefixe, methode);
                if let Ok(mut g) = JOURNAL_PATH.lock() { *g = journal; }
                let h = cible as HWND;
                if !h.is_null() { unsafe { SendMessageW(h, WM_APP_FIN, 2, 0); } }
            });
        }
        Action::OuvrirVersions => { ouvrir_versions(h); }
        Action::RetourVersions => { retour_versions(h); }
        Action::RafraichirVersions => { charger_catalogue_versions(h); }
        Action::InstallerVersion => { action_version_installer(h); }
        Action::UtiliserVersion => { action_version_utiliser(h); }
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
                if let Ok(mut g) = JOURNAL.lock() {
                    *g = journal.clone();
                }
                STATUT.lock().unwrap().replace(journal.last().cloned().unwrap_or_else(|| "Désinstallation terminée.".to_string()));
                let h = cible as HWND;
                if !h.is_null() {
                    unsafe { SendMessageW(h, WM_APP_FIN, 1, 0); }
                }
            });
        }
    }
}

/// Ouvre l'outil PATH (bouton de la barre d'état).
///
/// ⚠ C'est le SEUL accès garanti à l'outil : il est visible sur TOUTES les
/// vues, y compris l'écran d'échec, là où l'utilisateur en a besoin.
unsafe fn action_path(h: HWND) {
    let etat = &mut *ETAT;
    etat.diagnostic = Some(crate::setup_core::diagnostiquer_path(&etat.prefixe));
    etat.journal_path.clear();
    etat.vue = Vue::Path;
    rafraichir(h);
}

/* ── Sélecteur de versions : entrée, navigation, actions ───────────────── */

/// Ouvre le sélecteur depuis le premier écran ou les Options, et charge le
/// catalogue immédiatement (contrat LAUNCH1 § 6 : « à l'entrée de l'écran »).
fn ouvrir_versions(h: HWND) {
    {
        let etat = unsafe { &mut *ETAT };
        etat.version_retour = etat.vue;
        etat.vue = Vue::Versions;
    }
    charger_catalogue_versions(h);
}

/// Appelle `setup_core::launcher_catalogue()` (jamais `Err`) et stocke le
/// résultat. Le message du backend est affiché tel quel dans le bandeau ET
/// dans la barre d'état : une liste vide n'est jamais silencieuse.
fn charger_catalogue_versions(h: HWND) {
    {
        let etat = unsafe { &mut *ETAT };
        etat.statut = "Chargement du catalogue des versions…".to_string();
        etat.version_statut_erreur = false;
    }
    // L'appel peut interroger GitHub (gh/curl) : on laisse d'abord le tracé
    // afficher « Chargement… » avant de bloquer le fil de messages.
    unsafe { rafraichir(h) };

    let catalogue = crate::setup_core::launcher_catalogue();
    let nb = catalogue.versions.len();
    let message = catalogue.message.clone();
    let hors_ligne = catalogue.hors_ligne;
    let etat = unsafe { &mut *ETAT };
    let selection = etat.version_selection.min(nb.saturating_sub(1));
    let depart = etat.version_depart.min(nb.saturating_sub(1));
    etat.catalogue = Some(catalogue);
    etat.version_selection = selection;
    etat.version_depart = depart;
    etat.statut = message;
    etat.version_statut_erreur = hors_ligne;
    unsafe { rafraichir(h) };
}

/// Déplace la sélection de `delta` lignes, en gardant la ligne visible.
fn deplacer_selection_versions(h: HWND, delta: i32) {
    let etat = unsafe { &mut *ETAT };
    let nb = etat.catalogue.as_ref().map(|c| c.versions.len()).unwrap_or(0);
    if nb == 0 { return; }
    let visible = etat.version_visible.max(1);
    let sel = (etat.version_selection as i64 + delta as i64)
        .clamp(0, (nb - 1) as i64) as usize;
    etat.version_selection = sel;
    if sel < etat.version_depart {
        etat.version_depart = sel;
    } else if sel >= etat.version_depart + visible {
        etat.version_depart = sel - visible + 1;
    }
    unsafe { rafraichir(h) };
}

/// Défilement (flèches souris), en gardant la sélection dans la fenêtre.
fn defiler_versions(h: HWND, delta: i32) {
    let etat = unsafe { &mut *ETAT };
    let nb = etat.catalogue.as_ref().map(|c| c.versions.len()).unwrap_or(0);
    if nb == 0 { return; }
    let visible = etat.version_visible.max(1);
    let max_depart = nb.saturating_sub(visible);
    let depart = (etat.version_depart as i64 + delta as i64)
        .clamp(0, max_depart as i64) as usize;
    etat.version_depart = depart;
    if etat.version_selection < depart {
        etat.version_selection = depart;
    } else if etat.version_selection >= depart + visible {
        etat.version_selection = depart + visible - 1;
    }
    unsafe { rafraichir(h) };
}

/// Tag de la version sélectionnée, SI son action est autorisée par le contrat.
fn tag_action_versions(installer: bool) -> Option<String> {
    let etat = unsafe { &*ETAT };
    let cat = etat.catalogue.as_ref()?;
    let i = etat.version_selection.min(cat.versions.len().saturating_sub(1));
    let e = &cat.versions[i];
    let actions = actions_version(e, cat.hors_ligne);
    if (installer && actions.installer) || (!installer && actions.utiliser) {
        Some(e.tag.clone())
    } else {
        None
    }
}

/// Remet la sélection sur la version fraîchement installée, puis garde le
/// message d'action VISIBLE (le rechargement du catalogue ne doit pas l'écraser).
fn apres_action_version(h: HWND, statut: &crate::velopack::StatutVersion) {
    let ok = statut.ok();
    let version = statut.version.clone();
    charger_catalogue_versions(h);
    let etat = unsafe { &mut *ETAT };
    if ok {
        etat.version_installee = version.clone();
        if let Some(cat) = etat.catalogue.as_ref() {
            if let Some(i) = cat.versions.iter().position(|e| e.version == version) {
                etat.version_selection = i;
            }
        }
    }
    etat.statut = statut.message.clone();
    etat.version_statut_erreur = !ok;
    unsafe { rafraichir(h) };
}

/// « Installer / télécharger » : téléchargement + SHA-256 dans le backend.
fn action_version_installer(h: HWND) {
    let Some(tag) = tag_action_versions(true) else { return };
    {
        let etat = unsafe { &mut *ETAT };
        etat.statut = "Téléchargement, vérification SHA-256 et installation en cours…".to_string();
        etat.version_statut_erreur = false;
    }
    unsafe { rafraichir(h) };
    let statut = crate::setup_core::launcher_installer_version(&tag);
    apres_action_version(h, &statut);
}

/// « Utiliser cette version » : bascule locale, aucun réseau.
fn action_version_utiliser(h: HWND) {
    let Some(tag) = tag_action_versions(false) else { return };
    let statut = crate::setup_core::launcher_selectionner_version(&tag);
    apres_action_version(h, &statut);
}

/// « Retour » : revient à l'écran d'origine (premier écran ou Options).
fn retour_versions(h: HWND) {
    let etat = unsafe { &mut *ETAT };
    etat.vue = etat.version_retour;
    unsafe { rafraichir(h) };
}

/// Texte partagé entre le fil de travail et la fenêtre.
static STATUT: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Journal de la dernière réparation PATH (fil de travail → fenêtre).
static JOURNAL_PATH: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Journal complet de la dernière opération. Il contient les lignes
/// « Échec : … » produites par `setup_core`, ce qui permet à la fenêtre de
/// savoir si l'installation a RÉELLEMENT eu lieu.
static JOURNAL: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Lance l'installation (ou la mise à jour) dans un fil, pour que la barre de
/// progression continue d'animer : le résultat revient par `WM_APP_FIN`.
fn lancer_travail() {
    let prefixe = unsafe { (*ETAT).prefixe.clone() };
    let existant = unsafe { (*ETAT).existant };
    std::thread::spawn(move || {
        // Source du moteur : `payload\rose.exe` (ou `rose.exe`) à côté de
        // l'installeur, sinon l'exécutable courant (installeur autonome).
        let source = source_moteur();
        // ⚠ Si l'installeur EMBARQUE le moteur, on réinstalle TOUJOURS à
        // partir de ce payload : c'est la version que l'utilisateur vient de
        // télécharger. Passer par `mettre_a_jour` (qui interroge le réseau)
        // pouvait réinstaller une AUTRE version que celle du setup, et
        // échouait sans réseau. `mettre_a_jour` ne sert que si le payload est
        // absent (installeur « nu » qui va chercher la dernière version).
        let journal = if existant && source.is_none() {
            crate::setup_core::mettre_a_jour(&prefixe, false)
        } else {
            crate::setup_core::installer(&prefixe, source.as_deref())
        };
        // Le moteur embarqué est écrit dans un fichier temporaire : on le
        // retire une fois la copie faite, pour ne pas laisser traîner un
        // `rose.exe` complet dans %TEMP%.
        if let Some(p) = source.as_deref() {
            if p.to_string_lossy().contains("rose-setup-payload") {
                let _ = std::fs::remove_file(p);
            }
        }
        let derniere = journal.last().cloned().unwrap_or_else(|| "Terminé.".to_string());
        // On mémorise le JOURNAL COMPLET : la fenêtre en a besoin pour savoir
        // si l'opération a échoué (lignes « Échec : … »).
        if let Ok(mut g) = JOURNAL.lock() {
            *g = journal.clone();
        }
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
        // ⚠ 33 ms (≈30 images/s) et non 16 ms : à 62 Hz, le fil de messages
        // passait plus de temps à repeindre qu'à traiter les messages, et
        // l'animation paraissait saccadée puis figée pendant l'installation.
        // 30 i/s est fluide à l'œil et laisse respirer la boucle.
        SetTimer(h, TIMER_PROGRESSION, 33, std::ptr::null());
    } else {
        KillTimer(h, TIMER_PROGRESSION);
    }
}

/// Binaire Rose à installer.
///
/// Ordre :
/// 1. `payload\rose.exe` à côté de l'installeur (distribution Velopack) ;
/// 2. le moteur EMBARQUÉ dans les ressources (`RT_RCDATA 3`) — c'est ce qui
///    fait de `Rose-win-Setup.exe` un fichier unique.
///
/// ⚠⚠ NE JAMAIS retomber sur `current_exe()`. C'était le bug : cette
/// fonction-ci (distincte de celle de `rose-setup.rs`, et seule utilisée par
/// `lancer_travail`) renvoyait l'INSTALLEUR lui-même, qui se copiait sous
/// `rose.exe`. Le garde-fou `refuser_comme_moteur` le rejetait ensuite — et
/// comme la fenêtre affichait « Terminé » sans lire le journal, l'utilisateur
/// voyait une installation réussie alors que RIEN n'avait été posé (mesuré :
/// aucun fichier, aucun PATH, aucune association `.rose`).
pub fn source_moteur() -> Option<PathBuf> {
    let dossier = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let candidats = [
        dossier.join("payload").join("rose.exe"),
        dossier.join("rose.exe"),
        dossier.join("payload").join("rose"),
        dossier.join("rose"),
    ];
    if let Some(p) = candidats.into_iter().find(|p| p.is_file()) {
        return Some(p);
    }
    // Moteur embarqué dans les ressources de l'installeur.
    moteur_embarque()
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
            diagnostic: None,
            methode: 0,
            journal_path: Vec::new(),
            // Le mode affiché est CELUI DÉJÀ ENREGISTRÉ : relu au démarrage,
            // il peut être changé sur le premier écran puis « Continuer ».
            mode_maj: crate::velopack::lire_config().mode,
            // Sélecteur : catalogue chargé à l'entrée de `Vue::Versions`.
            catalogue: None,
            version_selection: 0,
            version_depart: 0,
            version_visible: 0,
            version_retour: Vue::MisesAJour,
            version_statut_erreur: false,
            vue: Vue::MisesAJour,
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
            version_installee: crate::setup_core::installation_existante()
                .and_then(|p| crate::setup_core::version_installee(&p))
                .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string()),
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
        // ── Icône ────────────────────────────────────────────────────────
        // Chargée AVANT `RegisterClassExW` et posée sur la CLASSE : c'est ce
        // que Windows utilise pour la barre des tâches, Alt+Tab et le
        // sélecteur de fenêtres. La poser seulement par `WM_SETICON` après
        // `CreateWindowExW` laissait l'icône par défaut (mesuré : la fenêtre
        // s'ouvrait sans icône Rose).
        //
        // ⚠ `LoadImageW` sur `RT_GROUP_ICON` avec des tailles EXPLICITES et
        // `LR_SHARED` renvoie NULL sur plusieurs versions de Windows : c'est
        // pourquoi la fenêtre restait sans icône. La combinaison qui marche
        // est `cx = cy = 0` + `LR_DEFAULTSIZE | LR_SHARED` : Windows choisit
        // lui-même la meilleure image du groupe.
        let (grande, petite) = {
            let g = LoadImageW(inst, 1 as *const u16, RT_GROUP_ICON as u32,
                               0, 0, LR_DEFAULTSIZE | LR_SHARED);
            let p = LoadImageW(inst, 1 as *const u16, RT_GROUP_ICON as u32,
                               0, 0, LR_DEFAULTSIZE | LR_SHARED);
            (g as HICON, p as HICON)
        };
        let classe = hex_w("RoseSetupRose");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            // ⚠⚠ PAS de `CS_HREDRAW | CS_VREDRAW`. Ces styles forcent
            // Windows à invalider TOUTE la fenêtre à chaque changement de
            // largeur ou de hauteur : le repaint ciblé du survol était donc
            // annulé, et un simple redimensionnement repeignait 1120 × 780
            // pixels en boucle — d'où le clignotement. `WM_SIZE` appelle déjà
            // `InvalidateRect`, on gère le repeint nous-mêmes.
            style: 0,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0, cbWndExtra: 0,
            hInstance: inst,
            hIcon: if grande.is_null() { std::ptr::null_mut() } else { grande },
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW as *const u16),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: classe.as_ptr(),
            hIconSm: if petite.is_null() { std::ptr::null_mut() } else { petite },
        };
        RegisterClassExW(&wc);
        let titre_w = hex_w(&format!("Rose {} — Programme d'installation", env!("CARGO_PKG_VERSION")));
        // Fenêtre entièrement personnalisée : AUCUN cadre Windows. On dessine
        // nous-mêmes la barre de titre, les boutons réduire / agrandir /
        // fermer et les bords. `WS_CAPTION` ou `WS_OVERLAPPEDWINDOW`
        // laissaient la barre Windows 11 (blanche, arrondie, avec le titre du
        // système) au-dessus de la nôtre.
        //
        // ⚠ PAS de `WS_THICKFRAME` : même avec `WS_POPUP`, ce style fait
        // dessiner par Windows une bordure non-cliente — la BARRE BLANCHE
        // visible au-dessus de la fenêtre. Le redimensionnement est assuré
        // autrement : `WM_NCHITTEST` renvoie HTRIGHT / HTBOTTOM /
        // HTBOTTOMRIGHT sur les bords, ce qui suffit à obtenir le curseur de
        // redimensionnement et le glisser natif, sans aucune bordure peinte.
        //
        // ⚠⚠ `WS_CLIPCHILDREN` : sans lui, Windows repeint les zones
        // « enfants » au-dessus du tracé. La fenêtre n'a pas d'enfant, mais
        // le style évite aussi le clignotement du fond système pendant les
        // redimensionnements.
        let style = WS_POPUP | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_CLIPCHILDREN;
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
        // ── Vérification AU DÉMARRAGE (mode « Auto » uniquement) ──────
        // Le garde-fou est dans `velopack::verifier_au_demarrage` : en mode
        // « Manuel » ou « Désactivé », cette fonction ne fait aucun appel
        // réseau et l'écran n'affiche aucun résultat de vérification.
        // La vérification NE TÉLÉCHARGE RIEN : le bouton « Télécharger »
        // n'apparaît qu'ensuite, comme accord explicite de l'utilisateur.
        if crate::velopack::doit_verifier_au_demarrage() {
            lancer_verification_maj(hwnd, false);
        }
        // L'icône est déjà posée sur la CLASSE (voir plus haut). On la
        // ré-applique sur la FENÊTRE par WM_SETICON : certaines versions de
        // Windows n'honorent l'icône de classe qu'après création, et cela ne
        // coûte rien.
        if !grande.is_null() {
            (*ETAT).icone = grande;
            SendMessageW(hwnd, WM_SETICON, ICON_BIG, grande as LPARAM);
        }
        if !petite.is_null() {
            SendMessageW(hwnd, WM_SETICON, ICON_SMALL, petite as LPARAM);
        }
        // Et l'icône de la CLASSE : c'est elle qui sert aux fenêtres créées
        // ensuite et dans la boîte de dialogue système.
        if !grande.is_null() {
            SetClassLongPtrW(hwnd, -14 /* GCLP_HICON */, grande as isize);
            SetClassLongPtrW(hwnd, -34 /* GCLP_HICONSM */, grande as isize);
        }
        // ── Coins arrondis ──────────────────────────────────────────────
        // `DWMWA_WINDOW_CORNER_PREFERENCE` (33) : 2 = « arrondi » (le
        // système utilise le rayon standard), 1 = « petit rayon ».
        // Indispensable car la fenêtre est `WS_POPUP` : sans cela, Windows
        // dessine un rectangle à angles droits.
        #[cfg(windows)]
        unsafe {
            #[link(name = "dwmapi")]
            unsafe extern "system" {
                fn DwmSetWindowAttribute(hwnd: isize, attr: u32, valeur: *const c_void, taille: u32) -> i32;
            }
            // ⚠⚠ SURTOUT PAS `DWMWA_WINDOW_CORNER_PREFERENCE` (33) ICI.
            // Mesuré sur Windows 11 : demander au DWM d'arrondir une fenêtre
            // `WS_POPUP` dont la classe n'a PAS de brosse de fond
            // (`hbrBackground = NULL`) fait peindre à DWM le cadre arrondi
            // PAR-DESSUS la surface composée ; la zone cliente restait alors
            // VIDE — l'utilisateur ne voyait que les BORDURES, avec « l'effet
            // transparent ». Les coins sont donc arrondis par NOTRE tracé
            // (GDI+ dans `dessiner_decore`, rayon de 12 px) : le résultat est
            // identique, sans dépendre de la composition DWM.
            let _ = DwmSetWindowAttribute; // garde le lien, aucun appel
        }
        // ⚠ `SW_SHOWNORMAL` (1) et non `SW_SHOW` (5) : `SW_SHOW` montre la
        // fenêtre dans son état « courant », qui peut être caché ou minimisé
        // selon la création ; `SW_SHOWNORMAL` la pose au premier plan ET lui
        // donne le focus. Mesuré : avec `SW_SHOW`, la fenêtre s'ouvrait
        // derrière d'autres fenêtres et l'utilisateur la croyait « disparue ».
        ShowWindow(hwnd, SW_SHOWNORMAL);
        UpdateWindow(hwnd);
        SetForegroundWindow(hwnd);
        // ── Boucle de messages ────────────────────────────────────────────
        //
        // ⚠⚠ C'est ICI que se jouait le « puis disparaissait au bout d'un
        // moment ». `GetMessageW` rend -1 en cas d'ERREUR ; la boucle
        // précédente sortait alors SANS rien diagnostiquer, et le processus
        // se terminait normalement : la fenêtre disparaissait, sans message,
        // sans crash. On distingue désormais 0 (WM_QUIT) de -1 (erreur).
        let mut msg = std::mem::zeroed::<MSG>();
        loop {
            let r = GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0);
            if r == 0 {
                break; // WM_QUIT : fermeture demandée
            }
            if r == -1 {
                // Erreur de la boucle : on journalise et on laisse la
                // fenêtre ouverte, au lieu de terminer le processus.
                eprintln!("rose-setup : GetMessageW a échoué (code {}), la fenêtre reste ouverte", GetLastError());
                std::thread::sleep(std::time::Duration::from_millis(50));
                continue;
            }
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

#[cfg(test)]
mod tests_geometrie {
    use super::*;

    /// Vérifie l'absence de chevauchement sur une grille DPI × taille.
    /// `_taille` est la hauteur client réelle : `echelle_tracé` tient compte
    /// de la largeur ET de la hauteur, exactement comme dans le tracé.
    fn sans_chevauchement(largeur: i32, hauteur: i32, dpi: i32) {
        let e = crate::paint::echelle_tracé(largeur, hauteur);
        let g = geometrie_marque(hauteur, dpi, e);
        let s = |v: i32| -> i32 { (v as f64 * e).round() as i32 };
        let contexte = format!("fenêtre {largeur}×{hauteur}, {dpi} DPI, échelle {e:.3}");

        // La boîte du logo ne dépasse JAMAIS la place réservée, ni la hauteur
        // native du PNG (276 × 343). `dessiner_rose` recadre ensuite le
        // contenu utile (`bornes_opaques`) en conservant le ratio.
        assert!(g.hauteur_logo <= g.place_logo, "logo trop haut : {contexte}");
        assert!(g.hauteur_logo <= 343, "agrandissement du PNG : {contexte}");
        let voulue = ((g.largeur_logo as f64 * 343.0 / 276.0).round() as i32).max(1);
        assert_eq!(g.hauteur_logo, voulue.min(g.place_logo).min(343),
                   "hauteur du logo incohérente : {contexte}");
        // Le titre ne remonte pas sur la tige de la rose.
        assert!(g.haut_titre >= s(78) + g.hauteur_logo,
                "titre sur le logo : {contexte}");

        // ⚠ L'assertion centrale : le sous-titre PART du bas réel du titre,
        // avec une marge. L'ancien `y_marque + s(56)` recouvrait « ROSE ».
        assert!(g.haut_sous_titre >= g.bas_titre,
                "sous-titre AVANT la fin du titre : {contexte}");
        assert!(g.haut_sous_titre - g.bas_titre >= s(10),
                "marge titre/sous-titre trop faible : {contexte}");

        // Les étapes ne remontent pas sur le sous-titre et ne descendent pas
        // sous les cartes du bas.
        assert!(g.y_etapes >= g.bas_sous_titre,
                "étapes sur le sous-titre : {contexte}");
        assert!(g.bas_sous_titre <= g.plancher,
                "sous-titre sous les cartes : {contexte}");
        assert!(g.y_etapes <= g.plancher,
                "étapes sous les cartes : {contexte}");
    }

    #[test]
    fn geometrie_sans_chevauchement_grille() {
        // 564 × 600 = la fenêtre étroite de la capture utilisateur ;
        // 806 × 562 = la taille minimale imposée par WM_GETMINMAXINFO à 96 DPI.
        for (largeur, hauteur) in [
            (1120, 780), (806, 562), (564, 600), (736, 512),
            (1120, 600), (560, 360), (1920, 1080),
        ] {
            for dpi in [96, 120, 144, 192] {
                sans_chevauchement(largeur, hauteur, dpi);
            }
        }
    }

    #[test]
    fn sous_titre_suit_le_bas_reel_du_titre_quand_le_dpi_monte() {
        // À 1920 × 1080 / 192 DPI, l'ancien décalage fixe `s(56)` recouvrait
        // le titre de 55 px : c'est la régression exacte de la capture.
        let g = geometrie_marque(1080, 192, crate::paint::echelle_tracé(1920, 1080));
        let e = crate::paint::echelle_tracé(1920, 1080);
        let s = |v: i32| -> i32 { (v as f64 * e).round() as i32 };
        assert!(g.haut_sous_titre > g.bas_titre);
        assert!(g.haut_sous_titre - g.bas_titre >= s(10));
        // Le titre occupe bien plus que `s(56)` : la hauteur suit le DPI.
        assert!(g.bas_titre - g.haut_titre > s(72));
    }

    #[test]
    fn logo_reste_visible_sur_fenetre_basse_et_etroite() {
        for (largeur, hauteur) in [(564, 600), (806, 562), (736, 512), (560, 360)] {
            let e = crate::paint::echelle_tracé(largeur, hauteur);
            let g = geometrie_marque(hauteur, 96, e);
            let seuil = (40.0 * e).round() as i32;
            assert!(g.hauteur_logo >= seuil,
                    "logo non dessiné en {largeur}×{hauteur} : {} < {seuil}",
                    g.hauteur_logo);
            assert!(g.bas_sous_titre <= g.plancher);
        }
    }

    #[test]
    fn premier_paint_sans_dimension_ne_panique_pas() {
        // `GetClientRect` peut rendre 0 × 0 avant `WM_SIZE` : `echelle_tracé`
        // et la géométrie doivent rester finies (aucun débordement).
        let g = geometrie_marque(0, 96, crate::paint::echelle_tracé(0, 0));
        assert_eq!(g.hauteur_logo, 0);
        assert_eq!(g.place_logo, 0);
        assert!(g.bas_titre < g.haut_sous_titre);
    }
}

#[cfg(test)]
mod tests_versions {
    use super::*;

    // FIXTURES : seules versions littérales du module, interdites en production.
    fn entree(tag: &str, titre: &str, date: &str, installable: bool,
              en_cache: bool, deja_installee: bool) -> crate::velopack::EntreeCatalogue {
        crate::velopack::EntreeCatalogue {
            tag: tag.to_string(),
            version: tag.trim_start_matches('v').to_string(),
            titre: titre.to_string(),
            date: date.to_string(),
            installable,
            en_cache,
            deja_installee,
        }
    }

    fn catalogue(versions: Vec<crate::velopack::EntreeCatalogue>, hors_ligne: bool,
                 message: &str) -> crate::velopack::CatalogueVersions {
        crate::velopack::CatalogueVersions {
            versions,
            hors_ligne,
            message: message.to_string(),
        }
    }

    #[test]
    fn ligne_affiche_version_titre_et_date() {
        let e = entree("v0.1.6", "Rose — correctifs", "2026-11-19T13:54:57Z", true, false, false);
        let ligne = format_ligne_version(&e);
        assert!(ligne.contains("0.1.6"), "{ligne}");
        assert!(ligne.contains("Rose — correctifs"), "{ligne}");
        assert!(ligne.contains("2026-11-19"), "{ligne}");
        // Aucune version en dur dans la fonction : une autre fixture donne une autre ligne.
        let autre = entree("v9.8.7", "Titre quelconque", "", false, false, false);
        assert!(format_ligne_version(&autre).contains("9.8.7"));
    }

    #[test]
    fn date_vide_est_dite_inconnue_pas_laissee_blanche() {
        let e = entree("v0.1.3", "Legacy", "", false, false, false);
        assert!(format_ligne_version(&e).contains("date inconnue"));
    }

    #[test]
    fn badges_refletent_exactement_les_trois_drapeaux() {
        let e = entree("v1", "T", "2026-01-01", true, true, true);
        let b = badges_version(&e);
        assert!(b.contains("Déjà installée"), "{b}");
        assert!(b.contains("Manifeste en cache"), "{b}");
        assert!(b.contains("Paquet disponible"), "{b}");
        // Le libellé ne promet PAS une réinstallation hors ligne.
        assert!(!b.to_lowercase().contains("prêt à installer"), "{b}");

        let rien = entree("v2", "T", "", false, false, false);
        assert_eq!(badges_version(&rien), "Aucun artefact installable — aucune action");
    }

    #[test]
    fn actions_respectent_le_tableau_du_contrat() {
        // installable=true, deja=false → Installer uniquement.
        assert_eq!(actions_version(&entree("v1", "T", "", true, false, false), false),
                   ActionsVersion { installer: true, utiliser: false });
        // installable=true, deja=true → Réinstaller OU Utiliser.
        assert_eq!(actions_version(&entree("v2", "T", "", true, true, true), false),
                   ActionsVersion { installer: true, utiliser: true });
        // installable=false, deja=true → Utiliser uniquement.
        assert_eq!(actions_version(&entree("v3", "T", "", false, false, true), false),
                   ActionsVersion { installer: false, utiliser: true });
        // installable=false, deja=false → rien.
        assert_eq!(actions_version(&entree("v4", "T", "", false, false, false), false),
                   ActionsVersion { installer: false, utiliser: false });
    }

    #[test]
    fn hors_ligne_desactive_installer_mais_garde_utiliser() {
        let e = entree("v1", "T", "2026-01-01", true, true, true);
        let a = actions_version(&e, true);
        assert!(!a.installer, "le téléchargement doit être désactivé hors ligne");
        assert!(a.utiliser, "la bascule reste possible sans réseau");
    }

    #[test]
    fn bandeau_est_explicite_et_jamais_vide() {
        let msg = "GitHub inaccessible (HTTP 000) : voici les versions déjà présentes.";
        let hors = catalogue(Vec::new(), true, msg);
        let bandeau = bandeau_versions(&hors);
        assert!(bandeau.contains("HORS LIGNE"), "{bandeau}");
        assert!(bandeau.contains(msg), "{bandeau}");
        assert!(bandeau.contains("téléchargement"), "{bandeau}");

        let vide = catalogue(Vec::new(), true, "");
        assert!(!bandeau_versions(&vide).is_empty());
        let en_ligne = catalogue(Vec::new(), false, "");
        assert!(!bandeau_versions(&en_ligne).is_empty());
        let avec_msg = catalogue(Vec::new(), false, "7 versions disponibles");
        assert!(bandeau_versions(&avec_msg).contains("7 versions disponibles"));
    }

    #[test]
    fn layout_liste_borne_le_nombre_de_lignes_visibles() {
        assert_eq!(layout_liste(400, 0), LayoutListeVersions { pas: 76, visible: 0 });
        let quatre = layout_liste(400, 4);
        assert_eq!(quatre.visible, 4);
        assert!((46..=76).contains(&quatre.pas));
        let beaucoup = layout_liste(400, 100);
        assert!(beaucoup.visible > 0 && beaucoup.visible < 100);
        assert_eq!(beaucoup.visible, (400 / beaucoup.pas) as usize);
        assert_eq!(layout_liste(-5, 3).visible, 0);
    }

    #[test]
    fn zones_ligne_sont_deterministes_et_ne_se_chevauchent_pas() {
        let liste = RECT { left: 100, top: 200, right: 800, bottom: 600 };
        let (l0, i0, u0) = zones_ligne_version(&liste, 60, 0, 0);
        let (l1, _, _) = zones_ligne_version(&liste, 60, 1, 0);
        assert_eq!(l1.top - l0.top, 60);
        assert_eq!(l0.bottom - l0.top, 60);
        assert!(i0.right < u0.left, "les deux boutons ne doivent pas se superposer");
        assert!(i0.left >= l0.left && u0.right <= l0.right, "boutons hors de la ligne");
        // Le défilement décale la même géométrie sans la modifier.
        let (l0d, _, _) = zones_ligne_version(&liste, 60, 5, 5);
        assert_eq!((l0.left, l0.top, l0.right, l0.bottom),
                   (l0d.left, l0d.top, l0d.right, l0d.bottom));
    }

    #[test]
    fn enumeration_vue_garde_les_index_des_cinq_etapes() {
        // Non-régression : le panneau latéral compare `vue as usize` à 0..4.
        assert_eq!(Vue::Bienvenue as usize, 0);
        assert_eq!(Vue::Licence as usize, 1);
        assert_eq!(Vue::Options as usize, 2);
        assert_eq!(Vue::Installation as usize, 3);
        assert_eq!(Vue::Termine as usize, 4);
        assert_eq!(ETAPES.len(), 5);
        for (i, _) in ETAPES.iter().enumerate() {
            let vue = match i {
                0 => Vue::Bienvenue, 1 => Vue::Licence, 2 => Vue::Options,
                3 => Vue::Installation, _ => Vue::Termine,
            };
            assert_eq!(vue as usize, i);
        }
        // Les deux écrans ajoutés sont APRÈS les cinq étapes.
        assert_eq!(Vue::MisesAJour as usize, 6);
        assert_eq!(Vue::Versions as usize, 7);
        assert!(Vue::Versions as usize >= ETAPES.len());
    }
}
