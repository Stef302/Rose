// Rose 2.0 — interface C ABI pour le navigateur (WebAssembly).
//
// Le module n'ajoute AUCUNE dépendance externe (pas de wasm-bindgen, pas de
// wasm-pack) : il expose un petit jeu de fonctions `extern "C"` que le
// glue JavaScript (`web/rose-wasm.js`) appelle directement.
//
// ── Convention des tampons ────────────────────────────────────────────────
//
//   rose_alloc(len)        → *mut u8 : `len` octets (alignement 1)
//   rose_free(ptr, len)    → libère un tampon rose_alloc (len OBLIGATOIRE)
//
//   rose_execute(p, len)   → *mut u8 : la SORTIE du programme Rose
//   rose_execute_latex(p,l)→ *mut u8 : le LaTeX du programme
//   rose_version()         → *mut u8 : la version du moteur
//
//   Les trois derniers rendent un tampon « à en-tête » :
//
//        octet 0..4  : u32 PETIT BOYEN ENDS = longueur N du texte
//        octet 4..   : N octets UTF-8 du texte
//
//   rose_free_buffer(ptr)  → libère un tampon à en-tête (N est relu dans
//                            l'en-tête : rien à passer).
//
// Le glue JS lit l'u32, puis décode les octets UTF-8.
//
// ── Robustesse ────────────────────────────────────────────────────────────
//
// Le moteur Rose signale une erreur runtime par une panique
// (`panic!("ROSE_ERR:…")`, voir `run::erreur_runtime`). Une panique qui
// traverse la frontière C ABI serait un comportement indéfini : TOUTES les
// fonctions d'entrée sont donc enveloppées dans `catch_unwind`, et la
// panique est convertie en message d'erreur rendu dans le tampon de sortie.
//
// ATTENTION — `wasm32-unknown-unknown` impose `panic = "abort"` : il n'y a
// donc PAS de remontée d'exception, `catch_unwind` n'y peut rien et la
// panique ABORTE le module (« unreachable »). Le message n'est pas perdu pour
// autant : le hook de panique, appelé AVANT l'abort, le range dans un tampon
// que l'hôte récupère ensuite avec `rose_dernier_panic()`. C'est le glue JS
// (`web/rose-wasm.js`) qui exploite ce canal : il attrape le piège, relit le
// message et le rend à l'utilisateur. Sur les cibles où le déballage est
// possible (natif, wasm32-wasip1), `catch_unwind` fait le même travail.
//
// L'API reste utilisable en natif (tests, outillage) : les fonctions
// `extern "C"` y sont exportées telles quelles.
//
// Licence MIT.

use std::sync::Mutex;

/// Dernière panique rencontrée (message) : le hook de panique est
/// silencieux et range le message ici, l'hôte peut le relire.
static DERNIERE_PANIQUE: Mutex<Option<String>> = Mutex::new(None);

/// Installe (une seule fois) un hook de panique silencieux : en WASM,
/// stderr n'est relié à rien. Le message est rangé dans `DERNIERE_PANIQUE`
/// (les erreurs Rose « ROSE_ERR:… » comprises : ce sont elles qui
/// interessent l'utilisateur) puis l'abort suit son cours.
fn installer_hook() {
    use std::sync::Once;
    static UNE_FOIS: Once = Once::new();
    UNE_FOIS.call_once(|| {
        std::panic::set_hook(Box::new(|info| {
            let msg = info.payload().downcast_ref::<String>().map(|s| s.as_str())
                .or_else(|| info.payload().downcast_ref::<&str>().copied())
                .unwrap_or("erreur inconnue");
            let mut g = match DERNIERE_PANIQUE.lock() { Ok(g) => g, Err(p) => p.into_inner() };
            *g = Some(msg.to_string());
        }));
    });
}

/// Message de la dernière panique (vide si aucune). Le message est
/// CONSOMMÉ : le lire vide le tampon.
#[unsafe(no_mangle)]
pub extern "C" fn rose_dernier_panic() -> *mut u8 {
    installer_hook();
    let texte = {
        let mut g = match DERNIERE_PANIQUE.lock() { Ok(g) => g, Err(p) => p.into_inner() };
        g.take().unwrap_or_default()
    };
    encoder(&texte)
}

/// Vide le tampon des messages de panique (appelé avant chaque exécution).
#[unsafe(no_mangle)]
pub extern "C" fn rose_raz_panic() {
    installer_hook();
    let mut g = match DERNIERE_PANIQUE.lock() { Ok(g) => g, Err(p) => p.into_inner() };
    *g = None;
}

/// Sortie capturée jusqu'ici (à lire après un arrêt du module : le tampon
/// de capture survit à l'abort, il est dans la mémoire linéaire).
#[unsafe(no_mangle)]
pub extern "C" fn rose_prendre_sortie() -> *mut u8 {
    installer_hook();
    let texte = crate::run::take_sink().unwrap_or_default();
    encoder(&texte)
}

/// Alloue `len` octets pour l'hôte (le glue JS y écrit le source).
#[unsafe(no_mangle)]
pub extern "C" fn rose_alloc(len: usize) -> *mut u8 {
    let mut v: Vec<u8> = Vec::with_capacity(len);
    v.resize(len, 0);
    into_raw_tampon(v)
}

/// Libère un tampon alloué par `rose_alloc`. `len` DOIT être la longueur
/// d'origine : c'est elle qui détermine la taille de la reconstruction.
#[unsafe(no_mangle)]
pub extern "C" fn rose_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 { return; }
    // SAFETY : `ptr` vient de `rose_alloc` (un Vec<u8> de longueur `len`).
    unsafe { drop(Vec::from_raw_parts(ptr, len, len)); }
}

/// Exécute un programme Rose et rend sa sortie (stdout) encodée.
#[unsafe(no_mangle)]
pub extern "C" fn rose_execute(ptr: *const u8, len: usize) -> *mut u8 {
    installer_hook();
    rose_raz_panic();
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let source = lire_source(ptr, len);
        crate::rose_executer(&source)
    }));
    let texte = match res {
        Ok(t) => t,
        Err(payload) => format!("Rose : erreur — {}\n", crate::message_panique(&payload)),
    };
    encoder(&texte)
}

/// Rend le programme Rose sous forme LaTeX (encodé comme `rose_execute`).
#[unsafe(no_mangle)]
pub extern "C" fn rose_execute_latex(ptr: *const u8, len: usize) -> *mut u8 {
    installer_hook();
    rose_raz_panic();
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let source = lire_source(ptr, len);
        crate::rose_latex(&source)
    }));
    let texte = match res {
        Ok(t) => t,
        Err(payload) => format!("Erreur : {}\n", crate::message_panique(&payload)),
    };
    encoder(&texte)
}

/// Version du moteur Rose (encodée comme `rose_execute`).
#[unsafe(no_mangle)]
pub extern "C" fn rose_version() -> *mut u8 {
    installer_hook();
    encoder(&crate::interp::version())
}

/// Analyse un programme Rose sans l'exécuter (encodé comme `rose_execute`).
#[unsafe(no_mangle)]
pub extern "C" fn rose_analyse(ptr: *const u8, len: usize) -> *mut u8 {
    installer_hook();
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let source = lire_source(ptr, len);
        match crate::rose_analyser(&source) {
            Ok(s) => s,
            // Le message du front-end porte déjà le préfixe du moteur
            // (« Rose : erreur — … ») : on ne le double pas.
            Err(e) => e,
        }
    }));
    let texte = match res {
        Ok(t) => t,
        Err(payload) => format!("Rose : erreur — {}\n", crate::message_panique(&payload)),
    };
    encoder(&texte)
}

/// Libère un tampon rendu par `rose_execute`, `rose_execute_latex`,
/// `rose_version`, `rose_analyse` ou `rose_dernier_panic`.
#[unsafe(no_mangle)]
pub extern "C" fn rose_free_buffer(ptr: *mut u8) {
    if ptr.is_null() { return; }
    // SAFETY : le tampon a été produit par `encoder` : u32 LE en tête,
    // suivi des octets. On relit la longueur pour reconstruire le Vec.
    unsafe {
        let n = u32::from_le_bytes([
            *ptr, *ptr.add(1), *ptr.add(2), *ptr.add(3),
        ]) as usize;
        drop(Vec::from_raw_parts(ptr, 4 + n, 4 + n));
    }
}

// ── Internes ──────────────────────────────────────────────────────────────

/// Lit le source Rose fourni par l'hôte (octets UTF-8, invalides tolérés).
fn lire_source(ptr: *const u8, len: usize) -> String {
    if ptr.is_null() || len == 0 { return String::new(); }
    // SAFETY : l'hôte garantit `len` octets valides à partir de `ptr`.
    let octets = unsafe { std::slice::from_raw_parts(ptr, len) };
    String::from_utf8_lossy(octets).into_owned()
}

/// Encode un texte : u32 LE (longueur) puis les octets UTF-8.
fn encoder(texte: &str) -> *mut u8 {
    let octets = texte.as_bytes();
    let mut v: Vec<u8> = Vec::with_capacity(4 + octets.len());
    v.extend_from_slice(&(octets.len() as u32).to_le_bytes());
    v.extend_from_slice(octets);
    into_raw_tampon(v)
}

/// Transforme un `Vec<u8>` en pointeur brut C, en garantissant que la
/// TAILLE de l'allocation est EXACTEMENT celle du Vec (donc que la
/// libération par `Vec::from_raw_parts` rend exactement a l'allocateur ce
/// qu'il a donné).
fn into_raw_tampon(v: Vec<u8>) -> *mut u8 {
    let mut b: Box<[u8]> = v.into_boxed_slice();
    let p = b.as_mut_ptr();
    // La boite est « oubliee » : l'allocation reste vivante jusqu'a
    // `rose_free` / `rose_free_buffer` qui la rendent a l'allocateur.
    std::mem::forget(b);
    p
}
