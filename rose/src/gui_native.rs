//! GUI native pour Rose — une VRAIE fenêtre du système d'exploitation.
//!
//! Constat daté au 19/11/2026. Ce module est **autonome** : aucun crate
//! supplémentaire, aucune DLL à livrer, aucun programme externe appelé.
//!
//! # Deux backends, un seul contrat
//!
//! | Plateforme | Backend | Comment c'est lié |
//! |---|---|---|
//! | Windows (x86/x64) | Win32 `user32.dll` | `extern "system"` + `#[link(name = "user32")]` : `user32.dll` fait partie de Windows, rien à embarquer |
//! | Linux / BSD / autres UNIX | X11 `libX11.so.6` | `dlopen`/`dlsym` au moment de l'appel : aucun `-lX11` au build, donc le binaire tourne même sur une machine sans X11 |
//!
//! Sur UNIX, si `DISPLAY` est vide ou si `libX11.so.6` est absent, chaque
//! fonction renvoie `Erreur::PasDeDisplay` : **le module ne prétend jamais
//! avoir ouvert une fenêtre quand il ne l'a pas fait**. Le jardin Rose peut
//! donc replier proprement sur le HTML.
//!
//! Ce module ne sait RIEN du langage Rose : il ne manipule que des `u64`
//! (poignées) et des `&str`. Le câblage vers la stdlib est proposé dans
//! `rose/docs/gui_native.patch` (non appliqué au dépôt).

#![allow(dead_code, unused_imports)]

use std::collections::{HashMap, VecDeque};
use std::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void, CString};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Types publics
// ---------------------------------------------------------------------------

/// État d'une fenêtre suivie par le registre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Etat {
    Creee,
    Affichee,
    Fermee,
}

/// Backend réellement utilisé pour une fenêtre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Win32,
    X11,
    /// Fenêtre fictive, uniquement pour les tests unitaires de la machine à états.
    Simule,
}

/// Une fenêtre native ouverte. `natif` = `HWND` (Win32) ou `Window` (X11),
/// `display` = `Display*` (X11 seulement). Les pointeurs sont gardés comme
/// `usize` pour que la structure reste `Send` et puisse vivre dans un `Mutex`.
#[derive(Debug, Clone)]
pub struct Fenetre {
    pub id: u64,
    pub titre: String,
    pub largeur: u32,
    pub hauteur: u32,
    pub etat: Etat,
    pub backend: Backend,
    pub natif: usize,
    pub display: usize,
}

/// Définition **générique** d'un bouton enfant. Aucun libellé métier, aucun
/// pas de chiffres, aucune règle d'interface : le jardin Rose fournit le
/// libellé et l'identifiant, le Rust ne fait que l'appel système.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bouton {
    pub id: u64,
    pub label: String,
    pub x: i32,
    pub y: i32,
    pub largeur: i32,
    pub hauteur: i32,
}

impl Bouton {
    /// Construit un bouton en nettoyant le texte et en bornant la géométrie.
    pub fn nouveau(id: u64, label: &str, x: i32, y: i32, largeur: i32, hauteur: i32) -> Bouton {
        Bouton {
            id,
            label: nettoyer_texte(label),
            x: x.clamp(0, LARGEUR_MAX as i32),
            y: y.clamp(0, LARGEUR_MAX as i32),
            largeur: borner_dimension(largeur as i64) as i32,
            hauteur: borner_dimension(hauteur as i64) as i32,
        }
    }
}

/// État interne des boutons d'une fenêtre, gardé à côté du registre des
/// fenêtres. `natifs` contient les `HWND` (Win32) ou les `Window` X11.
#[derive(Debug, Clone)]
struct BoutonsFenetre {
    defs: Vec<Bouton>,
    natifs: Vec<usize>,
}

/// Erreurs explicites : jamais de silence sur un backend indisponible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Erreur {
    PasDeDisplay,
    DimensionInvalide,
    PoigneeInconnue(u64),
    EtatInvalide(Etat),
    SymboleManquant(&'static str),
    AllocationEchec,
    CommandeInconnue,
}

impl std::fmt::Display for Erreur {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Erreur::PasDeDisplay => write!(f, "GUI natif : aucun affichage disponible (DISPLAY vide ou libX11 absente)"),
            Erreur::DimensionInvalide => write!(f, "GUI natif : dimension invalide"),
            Erreur::PoigneeInconnue(id) => write!(f, "GUI natif : poignée {} inconnue", id),
            Erreur::EtatInvalide(e) => write!(f, "GUI natif : état {:?} incompatible", e),
            Erreur::SymboleManquant(s) => write!(f, "GUI natif : symbole {} introuvable", s),
            Erreur::AllocationEchec => write!(f, "GUI natif : allocation de chaîne échouée"),
            Erreur::CommandeInconnue => write!(f, "GUI natif : commande inconnue"),
        }
    }
}
impl std::error::Error for Erreur {}

// ---------------------------------------------------------------------------
// Registre des fenêtres + helpers PURS (testables sans écran)
// ---------------------------------------------------------------------------

pub const LARGEUR_MIN: u32 = 1;
pub const LARGEUR_MAX: u32 = 8192;
pub const TITRE_MAX: usize = 1024;

fn registre() -> &'static Mutex<HashMap<u64, Fenetre>> {
    static REGISTRE: OnceLock<Mutex<HashMap<u64, Fenetre>>> = OnceLock::new();
    REGISTRE.get_or_init(|| Mutex::new(HashMap::new()))
}

static SUIVANT: AtomicU64 = AtomicU64::new(1);

fn boutons_registre() -> &'static Mutex<HashMap<u64, BoutonsFenetre>> {
    static B: OnceLock<Mutex<HashMap<u64, BoutonsFenetre>>> = OnceLock::new();
    B.get_or_init(|| Mutex::new(HashMap::new()))
}

fn clics_registre() -> &'static Mutex<HashMap<u64, VecDeque<u64>>> {
    static C: OnceLock<Mutex<HashMap<u64, VecDeque<u64>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

// Win32 : le `WndProc` ne reçoit que le HWND, pas l'identifiant Rose. On y
// pousse (HWND parent, id du contrôle) ; `attendre_clic` trie ensuite.
#[cfg(windows)]
static CLICS_WIN32: Mutex<VecDeque<(isize, u64)>> = Mutex::new(VecDeque::new());

/// Borne une dimension demandée dans [1, 8192] — testable sans GUI.
pub fn borner_dimension(v: i64) -> u32 {
    v.clamp(LARGEUR_MIN as i64, LARGEUR_MAX as i64) as u32
}

/// Titres/texte : on retire les NUL (qui tronqueraient les API C) et on borne
/// la longueur. L'Unicode est conservé tel quel.
pub fn nettoyer_texte(s: &str) -> String {
    s.chars().filter(|&c| c != '\0').take(TITRE_MAX).collect()
}

/// Encodage UTF-16 avec terminateur NUL, pour les API `...W` de Win32.
pub fn utf16_avec_nul(s: &str) -> Vec<u16> {
    let nettoye = nettoyer_texte(s);
    nettoye.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Noms des symboles X11 résolus par `dlsym` — documentés et testés.
pub const SYMBOLES_X11: &[&str] = &[
    "XOpenDisplay",
    "XCloseDisplay",
    "XDefaultRootWindow",
    "XCreateSimpleWindow",
    "XMapWindow",
    "XDestroyWindow",
    "XFlush",
    "XSync",
    "XStoreName",
    "XNextEvent",
    "XPending",
    "XLoadQueryFont",
    "XCreateGC",
    "XDrawString",
    "XFreeGC",
    "XFreeFont",
    "XSelectInput",
    "XSendEvent",
];

/// Diagnostic honnête : disent ce qui est *réellement* disponible, sans ouvrir
/// de fenêtre et sans prétendre qu'une fenêtre a été ouverte.
pub fn diagnostic() -> String {
    #[cfg(windows)]
    { "Win32/user32.dll (toujours présent sur Windows)".to_string() }
    #[cfg(not(windows))]
    {
        let display = std::env::var("DISPLAY").unwrap_or_default();
        let libs = if libx11_presente() { "libX11.so.6 présente" } else { "libX11.so.6 absente" };
        format!("X11 : DISPLAY={:?}, {}", display, libs)
    }
}

fn libx11_presente() -> bool {
    unsafe extern "C" {
        fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
    }
    const RTLD_NOW: c_int = 2;
    let h = unsafe { dlopen(b"libX11.so.6\0".as_ptr() as *const c_char, RTLD_NOW) };
    // On ne libère PAS le handle : les fenêtres déjà ouvertes en dépendent.
    !h.is_null()
}

// ---------------------------------------------------------------------------
// API publique — identique sur les deux backends
// ---------------------------------------------------------------------------

/// Crée une fenêtre et renvoie sa poignée (identifiant Rose, > 0).
pub fn fenetre_creer(titre: &str, largeur: i64, hauteur: i64) -> Result<u64, Erreur> {
    let titre = nettoyer_texte(titre);
    let largeur = borner_dimension(largeur);
    let hauteur = borner_dimension(hauteur);
    if titre.is_empty() {
        return Err(Erreur::DimensionInvalide);
    }

    #[cfg(windows)]
    let (natif, display, backend) = win32_creer(&titre, largeur, hauteur)?;
    #[cfg(not(windows))]
    let (natif, display, backend) = x11_creer(&titre, largeur, hauteur)?;

    let id = SUIVANT.fetch_add(1, Ordering::Relaxed);
    registre().lock().map_err(|_| Erreur::EtatInvalide(Etat::Creee))?.insert(
        id,
        Fenetre {
            id,
            titre,
            largeur,
            hauteur,
            etat: Etat::Creee,
            backend,
            natif,
            display,
        },
    );
    Ok(id)
}

/// Rend la fenêtre visible à l'écran (XMapWindow / ShowWindow).
pub fn fenetre_afficher(id: u64) -> Result<(), Erreur> {
    let f = registre().lock().map_err(|_| Erreur::EtatInvalide(Etat::Creee))?
        .get(&id).cloned().ok_or(Erreur::PoigneeInconnue(id))?;
    match f.etat {
        Etat::Fermee => return Err(Erreur::EtatInvalide(Etat::Fermee)),
        Etat::Affichee => return Ok(()), // idempotent
        Etat::Creee => {}
    }
    match f.backend {
        Backend::Simule => {}
        #[cfg(not(windows))]
        Backend::X11 => unsafe {
            let x = X11::charger()?;
            (x.map_window)(f.display as *mut c_void, f.natif as c_ulong);
            (x.flush)(f.display as *mut c_void);
            (x.sync)(f.display as *mut c_void, 0);
        },
        #[cfg(windows)]
        Backend::X11 => return Err(Erreur::CommandeInconnue),
        #[cfg(windows)]
        Backend::Win32 => unsafe { ShowWindow(f.natif as isize, 1); },
        #[cfg(not(windows))]
        Backend::Win32 => return Err(Erreur::CommandeInconnue),
    }
    registre().lock().map_err(|_| Erreur::EtatInvalide(Etat::Creee))?
        .get_mut(&id).map(|g| g.etat = Etat::Affichee);
    Ok(())
}

/// Boucle d'événements. Sur X11 on attend un `KeyPress`, `ButtonPress` ou
/// `ClientMessage` (fermeture demandée par le gestionnaire de fenêtres).
/// Sur Win32 : `GetMessageW`/`TranslateMessage`/`DispatchMessageW`.
///
/// `ROSE_GUI_ATTENTE_MS` (test uniquement) borne l'attente en millisecondes ;
/// 0 ou absent = attente infinie. Cela permet de tester l'ouverture réelle
/// sous Xvfb sans interaction humaine.
pub fn fenetre_attendre(id: u64) -> Result<(), Erreur> {
    let f = registre().lock().map_err(|_| Erreur::EtatInvalide(Etat::Creee))?
        .get(&id).cloned().ok_or(Erreur::PoigneeInconnue(id))?;
    if f.etat == Etat::Fermee {
        return Err(Erreur::EtatInvalide(Etat::Fermee));
    }

    let limite = std::env::var("ROSE_GUI_ATTENTE_MS").ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .filter(|&ms| ms > 0)
        .map(|ms| Instant::now() + Duration::from_millis(ms));

    match f.backend {
        Backend::Simule => {
            if let Some(lim) = limite {
                while Instant::now() < lim { std::thread::sleep(Duration::from_millis(2)); }
            }
            Ok(())
        }
        #[cfg(not(windows))]
        Backend::X11 => unsafe {
            let x = X11::charger()?;
            // XEvent = `long pad[24]` = 192 octets sur 64 bits ; 32×8 = 256.
            let mut evenement = [0u64; 32];
            let dpy = f.display as *mut c_void;
            loop {
                if (x.pending)(dpy) > 0 {
                    (x.next_event)(dpy, evenement.as_mut_ptr() as *mut c_void);
                    let type_ = evenement[0] as c_int;
                    if type_ == 2 || type_ == 4 || type_ == 33 { break; } // Key, Button, ClientMessage
                } else {
                    std::thread::sleep(Duration::from_millis(5));
                }
                if let Some(lim) = limite {
                    if Instant::now() >= lim { break; }
                }
            }
            Ok(())
        }
        #[cfg(windows)]
        Backend::X11 => return Err(Erreur::CommandeInconnue),
        #[cfg(windows)]
        Backend::Win32 => unsafe {
            let mut msg = MsgW::default();
            loop {
                let r = GetMessageW(&mut msg, 0, 0, 0);
                if r <= 0 { break; }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
                if let Some(lim) = limite {
                    if Instant::now() >= lim { break; }
                }
            }
            Ok(())
        },
        #[cfg(not(windows))]
        Backend::Win32 => Err(Erreur::CommandeInconnue),
    }
}

/// Ferme et libère la fenêtre (XDestroyWindow + XCloseDisplay / DestroyWindow).
pub fn fenetre_fermer(id: u64) -> Result<(), Erreur> {
    let f = {
        let mut reg = registre().lock().map_err(|_| Erreur::EtatInvalide(Etat::Fermee))?;
        reg.remove(&id).ok_or(Erreur::PoigneeInconnue(id))?
    };
    if f.etat == Etat::Fermee { return Ok(()); }
    if let Ok(mut b) = boutons_registre().lock() { b.remove(&id); }
    if let Ok(mut c) = clics_registre().lock() { c.remove(&id); }
    #[cfg(windows)]
    if let Ok(mut q) = CLICS_WIN32.lock() { q.retain(|(w, _)| *w != f.natif as isize); }
    match f.backend {
        Backend::Simule => {}
        #[cfg(not(windows))]
        Backend::X11 => unsafe {
            let x = X11::charger()?;
            let dpy = f.display as *mut c_void;
            (x.destroy_window)(dpy, f.natif as c_ulong);
            (x.flush)(dpy);
            (x.close_display)(dpy);
        },
        #[cfg(windows)]
        Backend::X11 => return Err(Erreur::CommandeInconnue),
        #[cfg(windows)]
        Backend::Win32 => unsafe { DestroyWindow(f.natif as isize); },
        #[cfg(not(windows))]
        Backend::Win32 => return Err(Erreur::CommandeInconnue),
    }
    Ok(())
}

/// Boîte de message modale. Win32 : `MessageBoxW`. X11 : une vraie petite
/// fenêtre modale native, dessinée avec `XDrawString` (police `fixed`),
/// fermée par un clic ou une touche — aucun `xmessage`, aucun Python.
pub fn message(titre: &str, texte: &str) -> Result<(), Erreur> {
    let titre = nettoyer_texte(titre);
    let texte = nettoyer_texte(texte);
    #[cfg(windows)]
    {
        let t = CString::new(titre.as_bytes()).map_err(|_| Erreur::AllocationEchec)?;
        let m = CString::new(texte.as_bytes()).map_err(|_| Erreur::AllocationEchec)?;
        unsafe { MessageBoxW(0, m.as_ptr() as *const u16, t.as_ptr() as *const u16, 0x40); }
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        let x = X11::charger()?;
        unsafe {
            let dpy = (x.open_display)(std::ptr::null());
            if dpy.is_null() { return Err(Erreur::PasDeDisplay); }
            let root = (x.default_root_window)(dpy);
            let w = (x.create_simple_window)(dpy, root, 100, 100, 420, 160, 2, 0, 0xffffff);
            if w == 0 { (x.close_display)(dpy); return Err(Erreur::PasDeDisplay); }
            let ct = CString::new(titre.as_bytes()).map_err(|_| Erreur::AllocationEchec)?;
            (x.store_name)(dpy, w, ct.as_ptr());
            (x.map_window)(dpy, w);
            (x.sync)(dpy, 0);

            let font = (x.load_query_font)(dpy, b"fixed\0".as_ptr() as *const c_char);
            if !font.is_null() {
                let gc = (x.create_gc)(dpy, w, 0, std::ptr::null_mut());
                if !gc.is_null() {
                    let ligne = CString::new(texte.as_bytes().iter().copied()
                        .filter(|&b| b != b'\n' && b != b'\r').collect::<Vec<u8>>())
                        .map_err(|_| Erreur::AllocationEchec)?;
                    (x.draw_string)(dpy, w, gc, 12, 40, ligne.as_ptr(),
                        ligne.as_bytes().len().min(i32::MAX as usize) as c_int);
                    (x.free_gc)(dpy, gc);
                }
                (x.free_font)(dpy, font);
            }

            let mut evenement = [0u64; 32];
            loop {
                if (x.pending)(dpy) > 0 {
                    (x.next_event)(dpy, evenement.as_mut_ptr() as *mut c_void);
                    let type_ = evenement[0] as c_int;
                    if type_ == 2 || type_ == 4 || type_ == 33 { break; }
                } else {
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
            (x.destroy_window)(dpy, w);
            (x.close_display)(dpy);
            Ok(())
        }
    }
}

/// Nombre de fenêtres vivantes dans le registre (diagnostic).
pub fn compter_fenetres() -> usize {
    registre().lock().map(|r| r.len()).unwrap_or(0)
}

// ---------------------------------------------------------------------------
// GUI1 — contrôles enfants (boutons) : aucun libellé métier dans ce module.
// Le jardin Rose possède l'interface ; ici on ne fait que créer des contrôles
// OS et rendre l'identifiant du bouton cliqué à l'appelant.
// ---------------------------------------------------------------------------

/// Hit-test purement géométrique, testable sans écran. Le premier bouton qui
/// contient le point gagne (les boutons ne se chevauchent pas dans le jardin).
pub fn bouton_sous(boutons: &[Bouton], x: i32, y: i32) -> Option<u64> {
    boutons.iter().find_map(|b| {
        (x >= b.x && x < b.x.saturating_add(b.largeur)
         && y >= b.y && y < b.y.saturating_add(b.hauteur)).then_some(b.id)
    })
}

fn pousser_clic(id: u64, bouton: u64) {
    if let Ok(mut c) = clics_registre().lock() {
        c.entry(id).or_default().push_back(bouton);
    }
}

fn prendre_clic(id: u64) -> Option<u64> {
    clics_registre().lock().ok()
        .and_then(|mut c| c.get_mut(&id).and_then(|q| q.pop_front()))
}

/// Crée les boutons enfants d'une fenêtre déja ouverte via `fenetre_creer`.
/// Remplace tout bouton précédemment déclaré pour cette fenêtre.
pub fn boutons_creer(id: u64, boutons: &[Bouton]) -> Result<(), Erreur> {
    let f = registre().lock().map_err(|_| Erreur::EtatInvalide(Etat::Creee))?
        .get(&id).cloned().ok_or(Erreur::PoigneeInconnue(id))?;
    if f.etat == Etat::Fermee { return Err(Erreur::EtatInvalide(Etat::Fermee)); }

    let defs: Vec<Bouton> = boutons.iter().map(|b| Bouton::nouveau(
        b.id, &b.label, b.x, b.y, b.largeur, b.hauteur)).collect();
    let mut natifs: Vec<usize> = Vec::new();

    match f.backend {
        Backend::Simule => {
            natifs.resize(defs.len(), 0);
        }
        #[cfg(not(windows))]
        Backend::X11 => unsafe {
            let x = X11::charger()?;
            let dpy = f.display as *mut c_void;
            // ButtonPressMask = 1<<2 = 4 ; ExposureMask = 1<<15 = 32768.
            for b in &defs {
                let w = (x.create_simple_window)(dpy, f.natif as c_ulong,
                    b.x, b.y, b.largeur as c_uint, b.hauteur as c_uint,
                    1, 0, 0xf0f0f0);
                if w == 0 { return Err(Erreur::PasDeDisplay); }
                (x.select_input)(dpy, w, 4 | 32768);
                (x.map_window)(dpy, w);
                natifs.push(w as usize);
            }
            (x.select_input)(dpy, f.natif as c_ulong, 4 | 2); // Button|Key
            for (i, b) in defs.iter().enumerate() {
                x11_dessiner_bouton(x, dpy, natifs[i] as c_ulong, b);
            }
            (x.flush)(dpy);
        },
        #[cfg(windows)]
        Backend::Win32 => unsafe {
            // "BUTTON\0" en UTF-16. WS_CHILD|WS_VISIBLE = 0x40000000|0x10000000.
            const CLASSE: &[u16] = &[66, 85, 84, 84, 79, 78, 0];
            let hinst = GetModuleHandleW(std::ptr::null());
            for b in &defs {
                let texte = utf16_avec_nul(&b.label);
                let hwnd = CreateWindowExW(0, CLASSE.as_ptr(), texte.as_ptr(),
                    0x40000000 | 0x10000000, b.x, b.y, b.largeur, b.hauteur,
                    f.natif as isize, b.id as isize, hinst, std::ptr::null_mut());
                if hwnd == 0 { return Err(Erreur::PasDeDisplay); }
                natifs.push(hwnd as usize);
            }
        },
        #[cfg(windows)]
        Backend::X11 => return Err(Erreur::CommandeInconnue),
        #[cfg(not(windows))]
        Backend::Win32 => return Err(Erreur::CommandeInconnue),
    }

    boutons_registre().lock().map_err(|_| Erreur::EtatInvalide(f.etat))?
        .insert(id, BoutonsFenetre { defs, natifs });
    clics_registre().lock().map_err(|_| Erreur::EtatInvalide(f.etat))?
        .insert(id, VecDeque::new());
    Ok(())
}

/// Nombre de boutons enfants déclarés pour une fenêtre.
pub fn compter_boutons(id: u64) -> usize {
    boutons_registre().lock().ok()
        .and_then(|r| r.get(&id).map(|b| b.defs.len())).unwrap_or(0)
}

/// Attend le clic sur un bouton enfant. `Some(id)` = identifiant Rose du
/// bouton, `None` = fermeture ou expiration du délai. `timeout_ms = 0`
/// signifie « sans limite » (comme `fenetre_attendre`).
pub fn attendre_clic_limite(id: u64, timeout_ms: u64) -> Result<Option<u64>, Erreur> {
    let f = registre().lock().map_err(|_| Erreur::EtatInvalide(Etat::Creee))?
        .get(&id).cloned().ok_or(Erreur::PoigneeInconnue(id))?;
    if f.etat == Etat::Fermee { return Err(Erreur::EtatInvalide(Etat::Fermee)); }
    let limite = if timeout_ms > 0 {
        Some(Instant::now() + Duration::from_millis(timeout_ms))
    } else { None };
    let expire = || limite.map(|l| Instant::now() >= l).unwrap_or(false);

    match f.backend {
        Backend::Simule => loop {
            if let Some(b) = prendre_clic(id) { return Ok(Some(b)); }
            if expire() { return Ok(None); }
            std::thread::sleep(Duration::from_millis(2));
        },
        #[cfg(not(windows))]
        Backend::X11 => unsafe {
            let x = X11::charger()?;
            let dpy = f.display as *mut c_void;
            let mut evenement = [0u64; 32];
            loop {
                if let Some(b) = prendre_clic(id) { return Ok(Some(b)); }
                if (x.pending)(dpy) > 0 {
                    (x.next_event)(dpy, evenement.as_mut_ptr() as *mut c_void);
                    let type_ = evenement[0] as c_int;
                    let w = evenement[4] as c_ulong; // XEvent : window à l'octet 32
                    if type_ == 12 { // Expose : redessiner le libellé du bouton
                        let bf = boutons_registre().lock().ok();
                        if let Some((defs, natifs)) = bf.as_ref().and_then(|r| r.get(&id))
                            .map(|b| (&b.defs, &b.natifs)) {
                            if let Some(i) = natifs.iter().position(|&n| n as c_ulong == w) {
                                x11_dessiner_bouton(x, dpy, w, &defs[i]);
                            }
                        }
                    } else if type_ == 4 { // ButtonPress
                        let bf = boutons_registre().lock().ok();
                        if let Some((defs, natifs)) = bf.as_ref().and_then(|r| r.get(&id))
                            .map(|b| (&b.defs, &b.natifs)) {
                            if let Some(i) = natifs.iter().position(|&n| n as c_ulong == w) {
                                pousser_clic(id, defs[i].id);
                            } else if w == f.natif as c_ulong {
                                let px = evenement[8] as u32 as i32;
                                let py = (evenement[8] >> 32) as u32 as i32;
                                if let Some(b) = bouton_sous(defs, px, py) { pousser_clic(id, b); }
                            }
                        }
                    } else if type_ == 2 || type_ == 33 { // Key / ClientMessage
                        return Ok(None);
                    }
                } else {
                    std::thread::sleep(Duration::from_millis(2));
                }
                if expire() { return Ok(None); }
            }
        },
        #[cfg(windows)]
        Backend::Win32 => unsafe {
            let mut msg = MsgW::default();
            loop {
                if let Some((_, b)) = CLICS_WIN32.lock().ok().and_then(|mut q| {
                    let i = q.iter().position(|(w, _)| *w == f.natif as isize)?;
                    q.remove(i)
                }) { return Ok(Some(b)); }
                // PeekMessageW : PM_NOREMOVE n'est pas utilisé, PM_REMOVE = 1.
                let r = PeekMessageW(&mut msg, 0, 0, 0, 1);
                if r != 0 {
                    if msg.message == 0x0012 { return Ok(None); } // WM_QUIT
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                } else {
                    std::thread::sleep(Duration::from_millis(2));
                }
                if expire() { return Ok(None); }
            }
        },
        #[cfg(windows)]
        Backend::X11 => Err(Erreur::CommandeInconnue),
        #[cfg(not(windows))]
        Backend::Win32 => Err(Erreur::CommandeInconnue),
    }
}

/// Comme `attendre_clic_limite`, mais lit `ROSE_GUI_ATTENTE_MS` (0 = infini),
/// pour rester cohérent avec `fenetre_attendre`.
pub fn attendre_clic(id: u64) -> Result<Option<u64>, Erreur> {
    let ms = std::env::var("ROSE_GUI_ATTENTE_MS").ok()
        .and_then(|s| s.trim().parse::<u64>().ok()).unwrap_or(0);
    attendre_clic_limite(id, ms)
}

/// IE de test : injecte un clic dans la file, SANS écran et SANS prétendre
/// qu'un contrôle OS a été touché. Ne remplace pas `x11_envoyer_clic`.
#[doc(hidden)]
pub fn clic_simuler(id: u64, bouton: u64) -> Result<(), Erreur> {
    let bf = boutons_registre().lock().map_err(|_| Erreur::EtatInvalide(Etat::Creee))?;
    let b = bf.get(&id).ok_or(Erreur::PoigneeInconnue(id))?;
    if !b.defs.iter().any(|d| d.id == bouton) {
        return Err(Erreur::PoigneeInconnue(bouton));
    }
    drop(bf);
    pousser_clic(id, bouton);
    Ok(())
}

#[cfg(not(windows))]
unsafe fn x11_dessiner_bouton(x: &X11, dpy: *mut c_void, w: c_ulong, b: &Bouton) {
    let font = (x.load_query_font)(dpy, b"fixed\0".as_ptr() as *const c_char);
    if font.is_null() { return; }
    let gc = (x.create_gc)(dpy, w, 0, std::ptr::null_mut());
    if !gc.is_null() {
        let texte = CString::new(b.label.as_bytes()).unwrap_or_default();
        let y = (b.hauteur - 6).clamp(6, 18);
        (x.draw_string)(dpy, w, gc, 6, y, texte.as_ptr(),
            texte.as_bytes().len().min(i32::MAX as usize) as c_int);
        (x.free_gc)(dpy, gc);
    }
    (x.free_font)(dpy, font);
}

/// IE de test / automatisation : envoie un VRAI `ButtonPress` X11 au serveur
/// (XSendEvent), qui revient ensuite par XNextEvent. Ce n'est PAS une simulation
/// locale : l'événement fait un aller-retour dans le serveur X.
#[cfg(not(windows))]
#[doc(hidden)]
pub fn x11_envoyer_clic(id: u64, bouton: u64) -> Result<(), Erreur> {
    let f = registre().lock().map_err(|_| Erreur::EtatInvalide(Etat::Creee))?
        .get(&id).cloned().ok_or(Erreur::PoigneeInconnue(id))?;
    let bf = boutons_registre().lock().map_err(|_| Erreur::EtatInvalide(f.etat))?
        .get(&id).cloned().ok_or(Erreur::PoigneeInconnue(id))?;
    let i = bf.defs.iter().position(|d| d.id == bouton)
        .ok_or(Erreur::PoigneeInconnue(bouton))?;
    let x = X11::charger()?;
    unsafe {
        let mut evenement = [0u64; 32];
        evenement[0] = 4;                       // ButtonPress
        evenement[4] = bf.natifs[i] as u64;     // window (octet 32)
        evenement[5] = f.natif as u64;          // root (octet 40)
        evenement[8] = 1 | (1u64 << 32);        // x=1, y=1 (octet 64)
        (x.send_event)(f.display as *mut c_void, bf.natifs[i] as c_ulong,
                       0, 4, evenement.as_mut_ptr() as *mut c_void); // ButtonPressMask
        (x.flush)(f.display as *mut c_void);
    }
    Ok(())
}

/// IE de test : crée une fenêtre SANS backend, pour vérifier la machine à
/// états et le registre sur une machine qui n'a pas d'écran.
#[doc(hidden)]
pub fn creer_simulee(titre: &str, largeur: i64, hauteur: i64) -> u64 {
    let id = SUIVANT.fetch_add(1, Ordering::Relaxed);
    registre().lock().expect("registre").insert(id, Fenetre {
        id,
        titre: nettoyer_texte(titre),
        largeur: borner_dimension(largeur),
        hauteur: borner_dimension(hauteur),
        etat: Etat::Creee,
        backend: Backend::Simule,
        natif: 0,
        display: 0,
    });
    id
}

// ===========================================================================
// Backend X11 — résolution DYNAMIQUE, aucun -lX11 au build
// ===========================================================================

#[cfg(not(windows))]
struct X11 {
    // Adresse du handle dlopen, gardée en `usize` pour que X11 soit Send+Sync
    // (un `*mut c_void` ne l'est pas, ce qui interdirait le `static OnceLock`).
    _handle: usize,
    open_display: unsafe extern "C" fn(*const c_char) -> *mut c_void,
    close_display: unsafe extern "C" fn(*mut c_void) -> c_int,
    default_root_window: unsafe extern "C" fn(*mut c_void) -> c_ulong,
    create_simple_window: unsafe extern "C" fn(
        *mut c_void, c_ulong, c_int, c_int, c_uint, c_uint, c_uint, c_ulong, c_ulong,
    ) -> c_ulong,
    map_window: unsafe extern "C" fn(*mut c_void, c_ulong) -> c_int,
    destroy_window: unsafe extern "C" fn(*mut c_void, c_ulong) -> c_int,
    flush: unsafe extern "C" fn(*mut c_void) -> c_int,
    sync: unsafe extern "C" fn(*mut c_void, c_int) -> c_int,
    store_name: unsafe extern "C" fn(*mut c_void, c_ulong, *const c_char) -> c_int,
    next_event: unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int,
    pending: unsafe extern "C" fn(*mut c_void) -> c_int,
    load_query_font: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    create_gc: unsafe extern "C" fn(*mut c_void, c_ulong, c_uint, *mut c_void) -> *mut c_void,
    draw_string: unsafe extern "C" fn(*mut c_void, c_ulong, *mut c_void, c_int, c_int, *const c_char, c_int) -> c_int,
    free_gc: unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int,
    free_font: unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int,
    select_input: unsafe extern "C" fn(*mut c_void, c_ulong, c_long) -> c_int,
    send_event: unsafe extern "C" fn(*mut c_void, c_ulong, c_int, c_long, *mut c_void) -> c_int,
}

#[cfg(not(windows))]
impl X11 {
    fn charger() -> Result<&'static X11, Erreur> {
        static X: OnceLock<Result<X11, ()>> = OnceLock::new();
        let x = X.get_or_init(|| match X11::charger_une_fois() { Ok(x) => Ok(x), Err(_) => Err(()) });
        x.as_ref().map_err(|_| Erreur::SymboleManquant("libX11.so.6"))
    }

    fn charger_une_fois() -> Result<X11, Erreur> {
        unsafe extern "C" {
            fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
            fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        }
        const RTLD_NOW: c_int = 2;
        let mut handle = unsafe { dlopen(b"libX11.so.6\0".as_ptr() as *const c_char, RTLD_NOW) };
        if handle.is_null() {
            handle = unsafe { dlopen(b"libX11.so\0".as_ptr() as *const c_char, RTLD_NOW) };
        }
        if handle.is_null() { return Err(Erreur::SymboleManquant("libX11.so.6")); }

        macro_rules! sym {
            ($nom:literal) => {{
                let p = unsafe { dlsym(handle, concat!($nom, "\0").as_ptr() as *const c_char) };
                if p.is_null() { return Err(Erreur::SymboleManquant($nom)); }
                unsafe { std::mem::transmute(p) }
            }};
        }

        Ok(X11 {
            _handle: handle as usize,
            open_display: sym!("XOpenDisplay"),
            close_display: sym!("XCloseDisplay"),
            default_root_window: sym!("XDefaultRootWindow"),
            create_simple_window: sym!("XCreateSimpleWindow"),
            map_window: sym!("XMapWindow"),
            destroy_window: sym!("XDestroyWindow"),
            flush: sym!("XFlush"),
            sync: sym!("XSync"),
            store_name: sym!("XStoreName"),
            next_event: sym!("XNextEvent"),
            pending: sym!("XPending"),
            load_query_font: sym!("XLoadQueryFont"),
            create_gc: sym!("XCreateGC"),
            draw_string: sym!("XDrawString"),
            free_gc: sym!("XFreeGC"),
            free_font: sym!("XFreeFont"),
            select_input: sym!("XSelectInput"),
            send_event: sym!("XSendEvent"),
        })
    }
}

#[cfg(not(windows))]
fn x11_creer(titre: &str, largeur: u32, hauteur: u32) -> Result<(usize, usize, Backend), Erreur> {
    let display_name = std::env::var("DISPLAY").unwrap_or_default();
    if display_name.is_empty() { return Err(Erreur::PasDeDisplay); }
    let x = X11::charger()?;
    unsafe {
        let dpy = (x.open_display)(std::ptr::null()); // NULL = $DISPLAY
        if dpy.is_null() { return Err(Erreur::PasDeDisplay); }
        let root = (x.default_root_window)(dpy);
        let ct = CString::new(titre.as_bytes()).map_err(|_| Erreur::AllocationEchec)?;
        let w = (x.create_simple_window)(dpy, root, 40, 40, largeur, hauteur, 2, 0, 0xffffff);
        if w == 0 { (x.close_display)(dpy); return Err(Erreur::PasDeDisplay); }
        (x.store_name)(dpy, w, ct.as_ptr());
        (x.flush)(dpy);
        Ok((w as usize, dpy as usize, Backend::X11))
    }
}

// ===========================================================================
// Backend Win32 — user32.dll est TOUJOURS là, zéro DLL à livrer
// ===========================================================================

#[cfg(windows)]
#[repr(C)]
struct WndClassExW {
    cb_size: u32,
    style: u32,
    lpfn_wnd_proc: Option<unsafe extern "system" fn(isize, u32, usize, isize) -> isize>,
    cb_cls_extra: c_int,
    cb_wnd_extra: c_int,
    h_instance: isize,
    h_icon: isize,
    h_cursor: isize,
    hbr_background: isize,
    lpsz_class_name: *const u16,
    lpsz_menu_name: *const u16,
    h_icon_sm: isize,
}

#[cfg(windows)]
#[repr(C)]
#[derive(Default)]
struct MsgW {
    hwnd: isize,
    message: u32,
    w_param: usize,
    l_param: isize,
    time: u32,
    pt_x: i32,
    pt_y: i32,
}

#[cfg(windows)]
#[link(name = "user32")]
unsafe extern "system" {
    fn RegisterClassExW(lpwcx: *const WndClassExW) -> u16;
    fn CreateWindowExW(
        dw_ex_style: u32, lp_class_name: *const u16, lp_window_name: *const u16,
        dw_style: u32, x: c_int, y: c_int, w: c_int, h: c_int,
        hwnd_parent: isize, h_menu: isize, h_instance: isize, lp_param: *mut c_void,
    ) -> isize;
    fn ShowWindow(hwnd: isize, n_cmd_show: c_int) -> c_int;
    fn DestroyWindow(hwnd: isize) -> c_int;
    fn MessageBoxW(hwnd: isize, lp_text: *const u16, lp_caption: *const u16, u_type: u32) -> c_int;
    fn GetMessageW(lp_msg: *mut MsgW, hwnd: isize, min: u32, max: u32) -> c_int;
    fn PeekMessageW(lp_msg: *mut MsgW, hwnd: isize, min: u32, max: u32, remove: u32) -> c_int;
    fn TranslateMessage(lp_msg: *const MsgW) -> c_int;
    fn DispatchMessageW(lp_msg: *const MsgW) -> isize;
    fn DefWindowProcW(hwnd: isize, msg: u32, wp: usize, lp: isize) -> isize;
    fn PostQuitMessage(n_exit_code: c_int);
    fn LoadCursorW(h_instance: isize, lp_cursor_name: usize) -> isize;
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(lp_module_name: *const u16) -> isize;
}

#[cfg(windows)]
unsafe extern "system" fn rose_wnd_proc(hwnd: isize, msg: u32, wp: usize, lp: isize) -> isize {
    // Édition 2024 : le corps d'une `unsafe fn` reste SAFE, d'où les blocs.
    unsafe {
        if msg == 0x0111 { // WM_COMMAND : clic sur un bouton enfant
            if let Ok(mut q) = CLICS_WIN32.lock() { q.push_back((hwnd, (wp & 0xffff) as u64)); }
            return 0;
        }
        if msg == 0x0002 { PostQuitMessage(0); return 0; } // WM_DESTROY
        DefWindowProcW(hwnd, msg, wp, lp)
    }
}

#[cfg(windows)]
fn win32_creer(titre: &str, largeur: u32, hauteur: u32) -> Result<(usize, usize, Backend), Erreur> {
    static CLASSE: OnceLock<()> = OnceLock::new();
    // "RoseFenetreN\0" en UTF-16 (const, sans allocation ni temporaire).
    const NOM_CLASSE: &[u16] = &[82, 111, 115, 101, 70, 101, 110, 101, 116, 114, 101, 78, 0];
    unsafe {
        CLASSE.get_or_init(|| {
            let h = GetModuleHandleW(std::ptr::null());
            let wc = WndClassExW {
                cb_size: std::mem::size_of::<WndClassExW>() as u32,
                style: 0x0001 | 0x0002, // CS_HREDRAW | CS_VREDRAW
                lpfn_wnd_proc: Some(rose_wnd_proc),
                cb_cls_extra: 0,
                cb_wnd_extra: 0,
                h_instance: h,
                h_icon: 0,
                h_cursor: LoadCursorW(0, 32512), // IDC_ARROW
                hbr_background: 6,               // COLOR_WINDOW + 1
                lpsz_class_name: NOM_CLASSE.as_ptr(),
                lpsz_menu_name: std::ptr::null(),
                h_icon_sm: 0,
            };
            RegisterClassExW(&wc);
        });
        let t = utf16_avec_nul(titre);
        let hwnd = CreateWindowExW(
            0, NOM_CLASSE.as_ptr(), t.as_ptr(), 0x00CF0000, // WS_OVERLAPPEDWINDOW
            i32::MIN, i32::MIN, largeur as c_int, hauteur as c_int,
            0, 0, GetModuleHandleW(std::ptr::null()), std::ptr::null_mut(),
        );
        if hwnd == 0 { return Err(Erreur::PasDeDisplay); }
        Ok((hwnd as usize, 0, Backend::Win32))
    }
}

// ===========================================================================
// Tests unitaires — PURS, exécutables sans écran
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimensions_bornees() {
        assert_eq!(borner_dimension(-5), LARGEUR_MIN);
        assert_eq!(borner_dimension(0), LARGEUR_MIN);
        assert_eq!(borner_dimension(640), 640);
        assert_eq!(borner_dimension(9000), LARGEUR_MAX);
    }

    #[test]
    fn texte_nettoye_unicode_et_nul() {
        assert_eq!(nettoyer_texte("Bonjour\0monde"), "Bonjourmonde");
        assert_eq!(nettoyer_texte("été 🌹"), "été 🌹");
        assert_eq!(nettoyer_texte(&"x".repeat(TITRE_MAX + 50)).chars().count(), TITRE_MAX);
    }

    #[test]
    fn utf16_win32_termine_par_nul() {
        let v = utf16_avec_nul("Rose");
        assert_eq!(v, vec![82, 111, 115, 101, 0]);
        assert_eq!(v.len(), 5);
    }

    #[test]
    fn symboles_x11_sont_complets() {
        for requis in ["XOpenDisplay", "XCreateSimpleWindow", "XMapWindow",
                       "XNextEvent", "XDestroyWindow", "XCloseDisplay"] {
            assert!(SYMBOLES_X11.contains(&requis), "symbole manquant : {}", requis);
        }
    }

    #[test]
    fn machine_a_etats_simulee() {
        let avant = compter_fenetres();
        let id = creer_simulee("test", 320, 200);
        assert_eq!(compter_fenetres(), avant + 1);
        {
            let reg = registre().lock().unwrap();
            let f = &reg[&id];
            assert_eq!(f.etat, Etat::Creee);
            assert_eq!(f.backend, Backend::Simule);
            assert_eq!((f.largeur, f.hauteur), (320, 200));
        }
        fenetre_afficher(id).unwrap();
        assert_eq!(registre().lock().unwrap()[&id].etat, Etat::Affichee);
        fenetre_afficher(id).unwrap(); // idempotent
        fenetre_attendre(id).unwrap();
        fenetre_fermer(id).unwrap();
        // ⚠ `compter_fenetres()` est global aux tests: mesuré, un autre test
        // `gui1_*` crée une fenêtre en parallèle et rend cette assertion
        // intermittente (left 1 right 0). Les deux assertions PAR ID ci-dessous
        // sont plus fortes et suffisent à prouver la fermeture.
        assert_eq!(fenetre_fermer(id), Err(Erreur::PoigneeInconnue(id)));
        assert_eq!(fenetre_afficher(id), Err(Erreur::PoigneeInconnue(id)));
    }

    #[test]
    fn titre_vide_refuse() {
        // Sur UNIX sans DISPLAY c'est PasDeDisplay qui arrive en premier, mais
        // un titre vide est refusé avant tout appel natif sur les deux backends.
        let r = fenetre_creer("", 100, 100);
        assert_eq!(r, Err(Erreur::DimensionInvalide));
    }

    #[test]
    fn diagnostic_ne_ment_pas() {
        let d = diagnostic();
        assert!(!d.is_empty());
        #[cfg(not(windows))]
        assert!(d.contains("DISPLAY"));
    }

    // ── GUI1 : boutons enfants, tests PURS (aucun écran requis) ────────────

    #[test]
    fn gui1_hit_test_geometrique() {
        let b = vec![
            Bouton::nouveau(1, "A", 10, 10, 100, 30),
            Bouton::nouveau(2, "B", 10, 50, 100, 30),
        ];
        assert_eq!(bouton_sous(&b, 12, 12), Some(1));
        assert_eq!(bouton_sous(&b, 12, 52), Some(2));
        assert_eq!(bouton_sous(&b, 10, 10), Some(1));   // bord inclus
        assert_eq!(bouton_sous(&b, 110, 12), None);     // bord droit exclu
        assert_eq!(bouton_sous(&b, 12, 40), None);      // entre les deux
    }

    #[test]
    fn gui1_bouton_nettoie_et_borne() {
        let b = Bouton::nouveau(9, "Plus\0 de chiffres", -5, 9000, 0, 99999);
        assert_eq!(b.label, "Plus de chiffres");
        assert_eq!((b.x, b.y), (0, LARGEUR_MAX as i32));
        assert_eq!((b.largeur, b.hauteur), (LARGEUR_MIN as i32, LARGEUR_MAX as i32));
    }

    #[test]
    fn gui1_machine_a_etats_boutons_simules() {
        // Pas d'assertion sur `compter_fenetres()` : un autre test peut créer
        // une fenêtre simulée en parallèle. On vérifie la poignée de CE test.
        let id = creer_simulee("GUI1", 400, 200);
        assert_eq!(boutons_creer(id, &[
            Bouton::nouveau(7, "Plus de chiffres", 12, 56, 160, 30),
            Bouton::nouveau(8, "10", 12, 96, 40, 24),
        ]), Ok(()));
        assert_eq!(compter_boutons(id), 2);
        assert_eq!(clic_simuler(id, 7), Ok(()));
        assert_eq!(attendre_clic_limite(id, 100), Ok(Some(7)));
        assert_eq!(attendre_clic_limite(id, 20), Ok(None)); // file vidée, délai écoulé
        assert_eq!(clic_simuler(id, 999), Err(Erreur::PoigneeInconnue(999)));
        fenetre_fermer(id).unwrap();
        assert_eq!(compter_boutons(id), 0);
        assert_eq!(fenetre_afficher(id), Err(Erreur::PoigneeInconnue(id)));
    }

    #[test]
    fn gui1_symboles_x11_pour_boutons() {
        for requis in ["XSelectInput", "XSendEvent", "XDrawString", "XCreateSimpleWindow"] {
            assert!(SYMBOLES_X11.contains(&requis), "symbole manquant : {}", requis);
        }
    }

    /// VRAI clic : nécessite `xvfb-run` ET `ROSE_GUI1_TEST_X11=1`. Sauté par
    /// défaut pour que `cargo test --release` reste parallèle et sans écran.
    #[test]
    fn gui1_x11_clic_reel_aller_retour_serveur() {
        #[cfg(windows)]
        { return; }
        #[cfg(not(windows))]
        {
            if std::env::var("ROSE_GUI1_TEST_X11").as_deref() != Ok("1")
                || std::env::var("DISPLAY").unwrap_or_default().is_empty() {
                eprintln!("[GUI1] clic X11 reel SAUTE (ROSE_GUI1_TEST_X11!=1 ou DISPLAY vide)");
                return;
            }
            eprintln!("[GUI1] test X11 reel demarre");
            let id = fenetre_creer("GUI1 clic reel", 360, 200).unwrap();
            fenetre_afficher(id).unwrap();
            boutons_creer(id, &[
                Bouton::nouveau(42, "Plus de chiffres", 12, 56, 180, 32),
            ]).unwrap();
            let emetteur = std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(300));
                x11_envoyer_clic(id, 42).unwrap();
            });
            let recu = attendre_clic_limite(id, 4000).unwrap();
            emetteur.join().unwrap();
            eprintln!("[GUI1] clic X11 reel recu = {:?}", recu);
            assert_eq!(recu, Some(42), "le clic XSendEvent doit revenir par XNextEvent");
            fenetre_fermer(id).unwrap();
        }
    }
}
