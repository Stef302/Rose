// ═══════════ Rose — moteur Rust : valeurs, interpréteur, FFI, bundle, CLI ═══════════
#![allow(dead_code)]
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};
use std::rc::Rc;

use crate::ffi::{ouvrir_dll, fermer_dll, symbole_dll, Q, Z, RTLD_LOCAL, RTLD_NOW};
use crate::float::{texte_flottant, prefis_flottant};
use crate::front::{E, I, Programme, Signature, Ty, TyN, Verificateur, analyser, replier_constants};

/* ═══════════════════════════ Valeurs ═══════════════════════════════ */

#[derive(Clone)]
pub enum Value {
    Entier(Z),
    Rationnel(Q),
    Flottant(f64),
    Booleen(bool),
    Chaine(Rc<String>),
    Liste(Rc<RefCell<Vec<Value>>>),
    Table(Rc<RefCell<Vec<(Value, Value)>>>),
    Tampon(Rc<RefCell<Vec<u8>>>),   /* buffer contigu d'octets, référence partagée */
    Vide,
}

/* ════════ temps() / duree() — SPEC v3.13 : miroir de l'interprète C++ ════════
   duree(n) met en forme n nanosecondes (n >= 0) : unité = la plus grande de
   ns, µs, ms, s, min, h dont l'échelle est <= n ; jusqu'à 3 décimales
   TRONQUÉES, sans zéros superflus. Division sur la chaîne décimale : même
   algorithme dans tous les moteurs, grands entiers compris.
   ────────────────────────────────────────────────────────────────────── */
fn div_decimale_u64(n_str: &str, div: u64) -> (String, u64) {
    let mut q = String::with_capacity(n_str.len());
    let mut r: u64 = 0;
    for c in n_str.bytes() {
        let cour = r * 10 + (c - b'0') as u64;
        q.push((b'0' + (cour / div) as u8) as char);
        r = cour % div;
    }
    let t = q.trim_start_matches('0');
    (if t.is_empty() { "0".to_string() } else { t.to_string() }, r)
}

fn formater_duree(n_in: &str) -> String {
    const UNITS: [&str; 6] = [" ns", " µs", " ms", " s", " min", " h"];
    const SCALES: [u64; 6] = [1, 1_000, 1_000_000, 1_000_000_000, 60_000_000_000, 3_600_000_000_000];
    let t = n_in.trim_start_matches('0');
    let n_str = if t.is_empty() { "0".to_string() } else { t.to_string() };
    let au_moins = |s: u64| -> bool {
        let u = s.to_string();
        if n_str.len() != u.len() { n_str.len() > u.len() } else { n_str.as_str() >= u.as_str() }
    };
    let mut u = 0usize;
    for i in 1..6 { if au_moins(SCALES[i]) { u = i; } }
    let (part, reste) = div_decimale_u64(&n_str, SCALES[u]);
    if u == 0 || reste == 0 { return format!("{}{}", part, UNITS[u]); }
    let frac3 = reste * 1000 / SCALES[u];
    let mut fs = format!("{:03}", frac3);
    while fs.ends_with('0') { fs.pop(); }
    format!("{}.{}{}", part, fs, UNITS[u])
}

impl Value {
    fn s(t: &str) -> Value { Value::Chaine(Rc::new(t.to_string())) }
    fn z(i: i64) -> Value { Value::Entier(Z::i(i)) }
    fn f(d: f64) -> Value { Value::Flottant(d) }
}

struct RetourSignale { v: Value }
type REx = Result<(), RetourSignale>;

/* Arguments vus par le programme Rose (v3) : nb_arguments()/argument(i). */
static PROG_ARGS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();

fn erreur_runtime(msg: &str) -> ! {
    use std::io::Write;
    let _ = std::io::stdout().flush();
    eprintln!("Rose : erreur — {}", msg);
    std::process::exit(1);
}

/* ── aléatoire : splitmix64, reproductible via graine(n) — séquence
   strictement identique à celle du moteur C++ (graine only : la graine
   par défaut intentionnellement propre à chaque moteur). ─────────────── */
fn rng_etat() -> &'static std::sync::atomic::AtomicU64 {
    use std::sync::atomic::AtomicU64;
    static ETAT: std::sync::OnceLock<AtomicU64> = std::sync::OnceLock::new();
    ETAT.get_or_init(|| AtomicU64::new(
        (std::process::id() as u64).wrapping_mul(0x9E3779B97F4A7C15)
            ^ (&ETAT as *const _ as u64)))
}

fn rng_tirer() -> u64 {
    use std::sync::atomic::Ordering;
    let mut z = rng_etat().fetch_add(0x9E3779B97F4A7C15, Ordering::Relaxed)
        .wrapping_add(0x9E3779B97F4A7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

#[repr(C)]
struct CTimeSpec { tv_sec: i64, tv_nsec: i64 }
extern "C" { fn clock_gettime(clock_id: i32, tp: *mut CTimeSpec) -> i32; }

/* ═══════════════════════ conversions de valeurs ═════════════════════ */

fn vers_z(v: &Value) -> Z {
    match v {
        Value::Entier(z) => z.clone(),
        Value::Booleen(b) => Z::i(if *b { 1 } else { 0 }),
        Value::Rationnel(q) => q.numerateur().fdiv_q(&q.denominateur()),
        Value::Flottant(d) => Z::de_f64(*d),
        _ => erreur_runtime("conversion vers entier impossible"),
    }
}

fn vers_q(v: &Value) -> Q {
    match v {
        Value::Entier(z) => Q::depuis_z(z),
        Value::Booleen(b) => Q::depuis_z(&Z::i(if *b { 1 } else { 0 })),
        Value::Rationnel(q) => q.clone(),
        Value::Flottant(d) => {
            if d.is_nan() || d.is_infinite() {
                erreur_runtime("flottant infini/non-nombre sans rationnel correspondant");
            }
            Q::depuis_f64(*d)
        }
        _ => erreur_runtime("conversion vers rationnel impossible"),
    }
}

fn vers_d(v: &Value) -> f64 {
    match v {
        Value::Flottant(d) => *d,
        Value::Entier(z) => z.en_f64(),
        Value::Rationnel(q) => q.en_f64(),
        Value::Booleen(b) => if *b { 1.0 } else { 0.0 },
        _ => erreur_runtime("conversion vers flottant impossible"),
    }
}

fn convertir(v: Value, ty: TyN) -> Value {
    match ty {
        TyN::Entier => match &v {
            Value::Booleen(b) => Value::Entier(Z::i(if *b { 1 } else { 0 })),
            Value::Entier(_) => v,
            _ => Value::Entier(vers_z(&v)),
        },
        TyN::Rationnel => match &v {
            Value::Rationnel(_) => v,
            _ => Value::Rationnel(vers_q(&v)),
        },
        TyN::Flottant => match &v {
            Value::Flottant(_) => v,
            _ => Value::Flottant(vers_d(&v)),
        },
        _ => v,
    }
}

fn chaine_vers_entier(ch: &str) -> Z {
    if let Some(z) = Z::depuis_texte(ch) { return z; }
    if let Some(p) = ch.find('/') {
        if let (Some(n), Some(d)) = (Z::depuis_texte(&ch[..p]), Z::depuis_texte(&ch[p + 1..])) {
            if d.sgn() != 0 { return n.fdiv_q(&d); }
        }
        erreur_runtime("conversion chaine → entier impossible");
    }
    if let Some(pt) = ch.find('.') {
        if let Some(z) = Z::depuis_texte(&ch[..pt]) { return z; }
    }
    erreur_runtime("conversion chaine → entier impossible")
}

fn chaine_vers_flottant(ch: &str) -> f64 {
    match prefis_flottant(ch) {
        Some(d) => d,
        None => erreur_runtime("conversion chaine → flottant impossible"),
    }
}

fn rnk_v(v: &Value) -> i32 {
    match v {
        Value::Booleen(_) | Value::Entier(_) => 1,
        Value::Rationnel(_) => 2,
        Value::Flottant(_) => 3,
        _ => 0,
    }
}

fn cmp_vals(g: &Value, d: &Value) -> i32 {
    if let (Value::Chaine(a), Value::Chaine(b)) = (g, d) {
        return if a == b { 0 } else if a < b { -1 } else { 1 };
    }
    let flot = matches!(g, Value::Flottant(_)) || matches!(d, Value::Flottant(_));
    if flot {
        let (a, b) = (vers_d(g), vers_d(d));
        return if a < b { -1 } else if a > b { 1 } else { 0 };
    }
    if matches!(g, Value::Rationnel(_)) || matches!(d, Value::Rationnel(_)) {
        let (a, b) = (vers_q(g), vers_q(d));
        return a.cmp(&b);
    }
    let (a, b) = (vers_z(g), vers_z(d));
    a.cmp(&b)
}

fn valeur_egale(a: &Value, b: &Value) -> bool {
    if matches!(a, Value::Tampon(_)) || matches!(b, Value::Tampon(_)) {
        return match (a, b) {
            (Value::Tampon(x), Value::Tampon(y)) => {
                let (x, y) = (x.borrow(), y.borrow());
                x.as_slice() == y.as_slice()
            }
            _ => false,
        };
    }
    if let (Value::Liste(x), Value::Liste(y)) = (a, b) {
        let (x, y) = (x.borrow(), y.borrow());
        if x.len() != y.len() { return false; }
        return x.iter().zip(y.iter()).all(|(u, v)| valeur_egale(u, v));
    }
    if let (Value::Table(x), Value::Table(y)) = (a, b) {
        let (x, y) = (x.borrow(), y.borrow());
        if x.len() != y.len() { return false; }
        for kv in x.iter() {
            let mut found = false;
            for kv2 in y.iter() {
                if valeur_egale(&kv.0, &kv2.0) && valeur_egale(&kv.1, &kv2.1) { found = true; break; }
            }
            if !found { return false; }
        }
        return true;
    }
    if matches!(a, Value::Table(_)) || matches!(b, Value::Table(_)) { return false; }
    if matches!(a, Value::Liste(_)) || matches!(b, Value::Liste(_)) { return false; }
    if matches!(a, Value::Chaine(_)) || matches!(b, Value::Chaine(_)) {
        return match (a, b) {
            (Value::Chaine(x), Value::Chaine(y)) => x == y,
            _ => false,
        };
    }
    if matches!(a, Value::Booleen(_)) || matches!(b, Value::Booleen(_)) {
        return match (a, b) {
            (Value::Booleen(x), Value::Booleen(y)) => x == y,
            _ => false,
        };
    }
    cmp_vals(a, b) == 0
}

/* ═══════════════════════════ affichage ══════════════════════════════ */

fn texte_periode(num: &Z, den: &Z) -> String {
    // num/den canonique, den > 1
    if num.chiffres10() as i64 + den.chiffres10() as i64 > 500 {
        return format!("{}/{}", num.texte(), den.texte());
    }
    let entier = num.fdiv_q(den);
    let mut reste = num.abs();
    reste = reste.fdiv_r(den);
    let mut digits: Vec<u8> = Vec::new();
    let mut restes: Vec<(Z, usize)> = Vec::new();
    let mut peri: i64 = -1;
    let mut ndig = 0usize;
    while reste.sgn() != 0 && ndig < 200 {
        if let Some((_, p)) = restes.iter().find(|(r, _)| r.cmp(&reste) == 0) {
            peri = *p as i64;
            break;
        }
        restes.push((reste.clone(), ndig));
        reste = reste.mul_u(10);
        let dig = reste.fdiv_q(den);
        digits.push(dig.en_u64() as u8);
        reste = reste.fdiv_r(den);
        ndig += 1;
    }
    let mut s = format!("{}.", entier.texte());
    if peri >= 0 {
        let p = peri as usize;
        for d in &digits[..p] { s.push((b'0' + *d) as char); }
        s.push('[');
        for d in &digits[p..] { s.push((b'0' + *d) as char); }
        s.push(']');
    } else {
        for d in &digits { s.push((b'0' + *d) as char); }
        if ndig >= 200 { s.push('…'); }
    }
    s
}

fn ecri_str(v: &Value) -> String {
    match v {
        Value::Entier(z) => z.texte(),
        Value::Rationnel(q) => {
            let mut q = q.clone();
            q.canon();
            let num = q.numerateur();
            let den = q.denominateur();
            if den.cmp(&Z::i(1)) == 0 { return num.texte(); }
            texte_periode(&num, &den)
        }
        Value::Flottant(d) => {
            let d = *d;
            if d.is_nan() { return "non_nombre".into(); }
            if d.is_infinite() { return if d > 0.0 { "infini".into() } else { "-infini".into() }; }
            if d == d.floor() && d.abs() < 1e16 { return format!("{}", d as i64); }
            texte_flottant(d)
        }
        Value::Booleen(b) => if *b { "vrai".into() } else { "faux".into() },
        Value::Chaine(s) => s.to_string(),
        Value::Tampon(_) => erreur_runtime(
            "Un tampon ne s'affiche pas directement — utiliser chaine(tampon)"),
        Value::Liste(l) => {
            let l = l.borrow();
            let parts: Vec<String> = l.iter().map(ecri_str).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Table(t) => {
            // Miroir de l'interprete : ecri(T_TABLE) passe par chaine_de(v)
            // (cellules en rendu chaine_de — rationnel « a/b », pas decimal).
            return chaine_de(v);
        }
        Value::Vide => String::new(),
    }
}

fn chaine_de(v: &Value) -> String {
    match v {
        Value::Tampon(t) => String::from_utf8_lossy(&t.borrow()).into_owned(),
        Value::Entier(z) => z.texte(),
        Value::Rationnel(q) => {
            let mut q = q.clone();
            q.canon();
            let num = q.numerateur();
            let den = q.denominateur();
            if den.cmp(&Z::i(1)) == 0 { num.texte() } else { format!("{}/{}", num.texte(), den.texte()) }
        }
        Value::Flottant(d) => {
            let d = *d;
            if d.is_nan() { return "non_nombre".into(); }
            if d.is_infinite() { return if d > 0.0 { "infini".into() } else { "-infini".into() }; }
            if d == d.floor() && d.abs() < 1e16 { return format!("{}", d as i64); }
            texte_flottant(d)
        }
        Value::Booleen(b) => if *b { "vrai".into() } else { "faux".into() },
        Value::Chaine(s) => s.to_string(),
        Value::Liste(l) => {
            let l = l.borrow();
            let parts: Vec<String> = l.iter().map(chaine_de).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Table(t) => {
            let t = t.borrow();
            let parts: Vec<String> = t.iter().map(|kv| format!("{} => {}", chaine_de(&kv.0), chaine_de(&kv.1))).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Vide => String::new(),
    }
}

fn nom_type_v(v: &Value) -> &'static str {
    match v {
        Value::Entier(_) => "entier", Value::Rationnel(_) => "rationnel",
        Value::Flottant(_) => "flottant", Value::Booleen(_) => "booleen",
        Value::Chaine(_) => "chaine", Value::Liste(_) => "liste", Value::Table(_) => "table",
        Value::Tampon(_) => "tampon", Value::Vide => "vide",
    }
}

/* ═══════════════════ bibliothèque mathématique (GMP) ════════════════ */

fn fact_arbre_run(dest_lo: u64, hi: u64) -> Z {
    if dest_lo > hi { return Z::i(1); }
    if hi - dest_lo < 16 {
        let mut dest = Z::u(dest_lo);
        let mut x = dest_lo + 1;
        while x <= hi { dest = dest.mul_u(x); x += 1; }
        return dest;
    }
    let mid = dest_lo + (hi - dest_lo) / 2;
    fact_arbre_run(dest_lo, mid).mul(&fact_arbre_run(mid + 1, hi))
}

fn rose_factorielle(n: &Z) -> Z {
    if n.sgn() < 0 { erreur_runtime("factorielle d'un négatif : impossible"); }
    if n.cmp(&Z::u(0xFFFF_FFFF)) > 0 { erreur_runtime("factorielle trop grande"); }
    let n = n.en_u64();
    if n < 1 { return Z::i(1); }
    fact_arbre_run(1, n)
}

fn rose_fibonacci(n: &Z) -> Z {
    if n.sgn() < 0 { erreur_runtime("fibonacci négatif : impossible"); }
    let mut a = Z::i(0);
    let mut b = Z::i(1);
    let bits = n.bits();
    for ibit in (0..bits).rev() {
        let t1 = a.mul(&b.mul_u(2).sub(&a));
        let t2 = a.mul(&a).add(&b.mul(&b));
        if n.bit(ibit) {
            let bnew = t1.add(&t2);
            a = t2;
            b = bnew;
        } else {
            a = t1;
            b = t2;
        }
    }
    a
}

fn rose_est_premier(n: &Z) -> bool {
    n.est_premier() && n.cmp(&Z::u(1)) > 0
}

fn rose_nb_chiffres(n: &Z) -> Z {
    if n.sgn() == 0 { return Z::i(1); }
    Z::u(n.abs().chiffres10() as u64)
}

fn rose_compter_premiers(n: &Z) -> Z {
    if n.sgn() < 0 || n.cmp(&Z::u(0xFFFF_FFFF)) > 0 { erreur_runtime("crible trop grand"); }
    let n = n.en_u64();
    let mut crible = vec![0u8; (n + 1) as usize];
    let mut compte: u64 = 0;
    let mut i = 2u64;
    while i <= n {
        if crible[i as usize] == 0 {
            compte += 1;
            if i <= n / i {
                let mut j = i * i;
                while j <= n { crible[j as usize] = 1; j += i; }
            }
        }
        i += 1;
    }
    Z::u(compte)
}

fn rose_pi(nn: &Z) -> String {
    if nn.sgn() <= 0 || nn.cmp(&Z::u(2_000_000)) > 0 {
        erreur_runtime("décimales de π hors limites (1..2000000)");
    }
    let chiffres = nn.en_u64();
    let s_exp = chiffres + 40;
    let dix_s = Z::pow10(s_exp);
    let arctan = |x: u64| -> Z {
        let xz = Z::u(x);
        let x2 = xz.mul(&xz);
        let mut p = xz.clone();
        let mut terme = dix_s.fdiv_q(&p);
        let mut dest = terme.clone();
        let mut k: u64 = 1;
        loop {
            p = p.mul(&x2);
            k += 2;
            terme = dix_s.fdiv_q(&p.mul_u(k));
            if terme.sgn() == 0 { break; }
            dest = if k % 4 == 3 { dest.sub(&terme) } else { dest.add(&terme) };
        }
        dest
    };
    let at5 = arctan(5).mul_u(16);
    let at239 = arctan(239).mul_u(4);
    let pi = at5.sub(&at239);
    let brut = pi.texte().into_bytes();
    let lbrut = brut.len();
    let mut sortie: Vec<u8> = vec![b' '; chiffres as usize + 2];
    sortie[0] = brut[0];
    sortie[1] = b'.';
    for i in 0..chiffres as usize {
        sortie[2 + i] = if i + 1 < lbrut { brut[i + 1] } else { b'0' };
    }
    if (1 + chiffres as usize) < lbrut && brut[1 + chiffres as usize] >= b'5' {
        let mut i = chiffres as isize + 1;
        while i >= 0 {
            if sortie[i as usize] == b'.' { i -= 1; continue; }
            sortie[i as usize] += 1;
            if sortie[i as usize] <= b'9' { break; }
            sortie[i as usize] = b'0';
            i -= 1;
        }
    }
    String::from_utf8(sortie).unwrap()
}

fn pow_rationnel(base: &Q, e: i64) -> Q {
    let mut num = base.numerateur();
    let mut den = base.denominateur();
    if e < 0 {
        if num.sgn() == 0 { erreur_runtime("0^exposant négatif"); }
        std::mem::swap(&mut num, &mut den);
        let k = (-(e as i128)) as u64;
        num = num.pow_u(k);
        den = den.pow_u(k);
    } else {
        num = num.pow_u(e as u64);
        den = den.pow_u(e as u64);
    }
    Q::avec_num_den(&num, &den)
}

/* ════════════════════════ gestion des bibliothèques ═════════════════ */

struct Handle(usize);
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

fn charger_biblio(nom: &str) -> usize {
    let candidats: Vec<String> = if nom.contains('/')
        || nom.ends_with(".so") || nom.ends_with(".dll") || nom.ends_with(".dylib") {
        vec![nom.to_string()]
    } else {
        {
            #[cfg(windows)]
            let v = vec![format!("{}.dll", nom), format!("lib{}.dll", nom), nom.to_string()];
            #[cfg(not(windows))]
            let v = vec![format!("lib{}.so", nom), format!("{}.so", nom), nom.to_string(),
                         format!("lib{}.so.6", nom), format!("lib{}.so.7", nom), format!("lib{}.so.8", nom)];
            v
        }
    };
    for c in &candidats {
        if let Ok(cs) = CString::new(c.clone()) {
            let h = unsafe { ouvrir_dll(cs.as_ptr(), RTLD_NOW | RTLD_LOCAL) };
            if !h.is_null() { return h as usize; }
        }
    }
    erreur_runtime(&format!("chargement de bibliothèque impossible ({})", nom))
}

fn symbole(h: usize, sym: &str) -> usize {
    let cs = match CString::new(sym) {
        Ok(c) => c,
        Err(_) => erreur_runtime("symbole introuvable dans la bibliothèque"),
    };
    let p = unsafe { symbole_dll(h as *mut c_void, cs.as_ptr()) };
    if p.is_null() {
        erreur_runtime(&format!("symbole introuvable dans la bibliothèque ({})", sym));
    }
    p as usize
}

fn c_string(s: &str) -> CString {
    match s.find('\0') {
        Some(p) => CString::new(&s[..p]).unwrap(),
        None => CString::new(s).unwrap(),
    }
}

unsafe fn vers_chaine_c(p: *const c_char) -> Value {
    if p.is_null() { return Value::s(""); }
    Value::s(&CStr::from_ptr(p).to_string_lossy().into_owned())
}

/* ═══════════════════════════ interpréteur ═══════════════════════════ */

struct Interp<'p> {
    prog: &'p Programme,
    globaux: HashMap<String, Value>,
    trames: Vec<HashMap<String, Value>>,
    poignees: HashMap<String, usize>,
    poignees_dyn: Vec<usize>,
}

impl<'p> Interp<'p> {
    fn nouveau(prog: &'p Programme) -> Interp<'p> {
        Interp {
            prog,
            globaux: HashMap::new(),
            trames: vec![HashMap::new()],
            poignees: HashMap::new(),
            poignees_dyn: Vec::new(),
        }
    }

    fn cible_var<'s>(&'s mut self, nom: &str) -> Option<&'s mut Value> {
        if let Some(v) = self.trames.last_mut().unwrap().get_mut(nom) { return Some(v); }
        self.globaux.get_mut(nom)
    }

    fn lire_var(&self, nom: &str) -> Option<&Value> {
        if let Some(v) = self.trames.last().unwrap().get(nom) { return Some(v); }
        self.globaux.get(nom)
    }

    fn affecter(&mut self, nom: &str, v: Value) {
        if let Some(c) = self.cible_var(nom) {
            let ty = match c {
                Value::Entier(_) => TyN::Entier,
                Value::Rationnel(_) => TyN::Rationnel,
                Value::Flottant(_) => TyN::Flottant,
                _ => TyN::Vide,
            };
            *c = if ty == TyN::Vide { v } else { convertir(v, ty) };
            return;
        }
        self.trames.last_mut().unwrap().insert(nom.to_string(), v);
    }

    /* ── instructions ──────────────────────────────────────────────── */
    fn exec(&mut self, i: &I) -> REx {
        match i {
            I::Afficher { expr, .. } => {
                let v = self.eval(expr);
                self.afficher(&v);
            }
            I::Lire { nom, typ_read, .. } => {
                let v = self.lire_selon(typ_read);
                self.affecter(nom, v);
            }
            I::Affect { nom, expr, .. } => {
                let v = self.eval(expr);
                self.affecter(nom, v);
            }
            I::Si { test, alors, sinon, .. } => {
                if self.eval_bool(test) {
                    for s in alors { self.exec(s)?; }
                } else if let Some(alors2) = sinon {
                    for s in alors2 { self.exec(s)?; }
                }
            }
            I::TantQue { test, corps, .. } => {
                while self.eval_bool(test) {
                    for s in corps { self.exec(s)?; }
                }
            }
            I::Pour { var, debut, fin, corps, .. } => {
                let fin = vers_z(&self.eval(fin));
                let debut = vers_z(&self.eval(debut));
                self.trames.last_mut().unwrap().insert(var.clone(), Value::Entier(debut));
                let mut res = Ok(());
                loop {
                    let depasse = match self.trames.last().unwrap().get(var) {
                        Some(Value::Entier(z)) => z.cmp(&fin) >= 0,
                        _ => true,
                    };
                    if depasse { break; }
                    for s in corps {
                        if let Err(r) = self.exec(s) { res = Err(r); break; }
                    }
                    if res.is_err() { break; }
                    let nouveau = match self.trames.last().unwrap().get(var) {
                        Some(Value::Entier(z)) => z.add(&Z::i(1)),
                        _ => break,
                    };
                    self.trames.last_mut().unwrap().insert(var.clone(), Value::Entier(nouveau));
                }
                self.trames.last_mut().unwrap().remove(var);
                return res;
            }
            I::Retour { expr, .. } => {
                return Err(RetourSignale { v: expr.as_ref().map(|e| self.eval(e)).unwrap_or(Value::Vide) });
            }
            I::Expr { expr, .. } => { self.eval(expr); }
            I::Bloc { instrs, .. } => {
                for s in instrs { self.exec(s)?; }
            }
        }
        Ok(())
    }

    fn eval_bool(&mut self, e: &E) -> bool {
        match self.eval(e) {
            Value::Booleen(b) => b,
            _ => false,
        }
    }

    fn afficher(&mut self, v: &Value) {
        use std::io::Write;
        let mut out = std::io::stdout();
        let _ = out.write_all(ecri_str(v).as_bytes());
        let _ = out.write_all(b"\n");
    }

    fn lire_ligne() -> String {
        use std::io::BufRead;
        let ligne = std::io::stdin().lines().next();
        match ligne {
            None => erreur_runtime("entrée attendue, fichier fermé"),
            Some(Err(_)) => erreur_runtime("entrée attendue, fichier fermé"),
            Some(Ok(t)) => t.chars().rev().skip_while(|c| *c == '\n' || *c == '\r' || *c == ' ')
                .collect::<String>().chars().rev().collect(),
        }
    }

    fn lire_selon(&mut self, ty: &Ty) -> Value {
        let t = Interp::lire_ligne();
        match ty.n {
            TyN::Flottant => {
                match prefis_flottant(&t) {
                    Some(d) => Value::f(d),
                    None => erreur_runtime("flottant invalide"),
                }
            }
            TyN::Rationnel => {
                match Q::depuis_texte(&t) {
                    Some(q) => Value::Rationnel(q),
                    None => erreur_runtime("rationnel invalide"),
                }
            }
            _ => {
                if t.contains('/') {
                    if let Some(q) = Q::depuis_texte(&t) {
                        return Value::Entier(q.numerateur().tdiv_q(&q.denominateur()));
                    }
                }
                if t.contains('.') {
                    if let Some(d) = prefis_flottant(&t) { return Value::Entier(Z::de_f64(d)); }
                }
                match Z::depuis_texte(&t) {
                    Some(z) => Value::Entier(z),
                    None => erreur_runtime("nombre entier invalide"),
                }
            }
        }
    }

    /* ── expressions ───────────────────────────────────────────────── */
    fn eval(&mut self, e: &E) -> Value {
        match e {
            E::Litteral { texte, flott, .. } => {
                if *flott {
                    return Value::f(prefis_flottant(texte).unwrap_or(0.0));
                }
                if texte.contains('/') {
                    match Q::depuis_texte(texte) {
                        Some(q) => return Value::Rationnel(q),
                        None => erreur_runtime(&format!("constante invalide ({})", texte)),
                    }
                }
                match Z::depuis_texte(texte) {
                    Some(z) => Value::Entier(z),
                    None => erreur_runtime(&format!("constante invalide ({})", texte)),
                }
            }
            E::ChaineLit { valeur, .. } => Value::s(valeur),
            E::BooleenLit { valeur, .. } => Value::Booleen(*valeur),
            E::Var { nom, ligne } => match self.lire_var(nom) {
                Some(v) => v.clone(),
                None => erreur_runtime(&format!("variable '{}' utilisée sans valeur", nom)),
            },
            E::ListeLit { elements, .. } => {
                let l: Vec<Value> = elements.iter().map(|x| self.eval(x)).collect();
                Value::Liste(Rc::new(RefCell::new(l)))
            }
            E::Index { base, index, .. } => {
                let base = self.eval(base);
                let idx = self.eval(index);
                match &base {
                    Value::Liste(l) => {
                        let idx = vers_z(&idx);
                        let n = l.borrow().len() as u64;
                        if idx.sgn() < 0 || !idx.tient_u64() || idx.en_u64() >= n {
                            erreur_runtime("index hors des limites de la liste");
                        }
                        l.borrow()[idx.en_u64() as usize].clone()
                    }
                    Value::Table(t) => {
                        let cle = idx;
                        let t = t.borrow();
                        for kv in t.iter() {
                            if valeur_egale(&kv.0, &cle) { return kv.1.clone(); }
                        }
                        erreur_runtime("clé absente de la table");
                        Value::Vide
                    }
                    Value::Chaine(s) => {
                        let idx = vers_z(&idx);
                        let n = s.chars().count() as u64;
                        if idx.sgn() < 0 || !idx.tient_u64() || idx.en_u64() >= n {
                            erreur_runtime("index hors des limites de la chaîne");
                        }
                        let c = s.chars().nth(idx.en_u64() as usize).unwrap();
                        Value::s(&c.to_string())
                    }
                    _ => erreur_runtime("index sur une valeur qui n'est pas une liste"),
                }
            }
            E::Unaire { op, operande, .. } => {
                let v = self.eval(operande);
                if op == "non" {
                    return match v { Value::Booleen(b) => Value::Booleen(!b), _ => Value::Booleen(false) };
                }
                match &v {
                    Value::Entier(z) => Value::Entier(z.neg()),
                    Value::Rationnel(q) => Value::Rationnel(q.neg()),
                    Value::Flottant(d) => Value::f(-d),
                    Value::Booleen(b) => Value::z(if *b { -1 } else { 0 }),
                    _ => erreur_runtime(&format!("opérateur unaire '-' sur un {}", nom_type_v(&v))),
                }
            }
            E::Fact { operande, .. } => {
                let v = self.eval(operande);
                Value::Entier(rose_factorielle(&vers_z(&v)))
            }
            E::Binaire { .. } => self.binaire(e),
            E::Appel { .. } => self.appel(e),
        }
    }

    fn binaire(&mut self, e: &E) -> Value {
        let (op, g, d) = match e {
            E::Binaire { op, gauche, droite, .. } => (op.clone(), &**gauche, &**droite),
            _ => unreachable!(),
        };
        if op == "et" {
            if !self.eval_bool(g) { return Value::Booleen(false); }
            return Value::Booleen(self.eval_bool(d));
        }
        if op == "ou" {
            if self.eval_bool(g) { return Value::Booleen(true); }
            return Value::Booleen(self.eval_bool(d));
        }
        let g = self.eval(g);
        let d = self.eval(d);
        match op.as_str() {
            "==" | "!=" | "<" | "<=" | ">" | ">=" => {
                if op == "==" || op == "!=" {
                    if matches!(g, Value::Liste(_)) || matches!(d, Value::Liste(_))
                        || matches!(g, Value::Table(_)) || matches!(d, Value::Table(_))
                        || matches!(g, Value::Tampon(_)) || matches!(d, Value::Tampon(_)) {
                        let eg = valeur_egale(&g, &d);
                        return Value::Booleen(if op == "==" { eg } else { !eg });
                    }
                    let g_ch = matches!(g, Value::Chaine(_));
                    let d_ch = matches!(d, Value::Chaine(_));
                    if g_ch != d_ch {
                        return Value::Booleen(op == "!=");
                    }
                }
                let c = cmp_vals(&g, &d);
                let r = match op.as_str() {
                    "==" => c == 0, "!=" => c != 0, "<" => c < 0,
                    "<=" => c <= 0, ">" => c > 0, _ => c >= 0,
                };
                Value::Booleen(r)
            }
            "≈" | "?=" | "=~" => {
                let r = if matches!(g, Value::Flottant(_)) || matches!(d, Value::Flottant(_)) {
                    let (a, b) = (vers_d(&g), vers_d(&d));
                    let mut scale = a.abs().max(b.abs());
                    if scale < 1.0 { scale = 1.0; }
                    (a - b).abs() <= 1e-9 * scale
                } else {
                    cmp_vals(&g, &d) == 0
                };
                Value::Booleen(r)
            }
            "+" if matches!(g, Value::Chaine(_)) && matches!(d, Value::Chaine(_)) => {
                match (&g, &d) {
                    (Value::Chaine(a), Value::Chaine(b)) => Value::s(&(a.to_string() + b)),
                    _ => Value::s(""),
                }
            }
            _ => {
                let m = rnk_v(&g).max(rnk_v(&d));
                let num = |a: f64, b: f64| a;
                let _ = num;
                match op.as_str() {
                    "+" => if m == 3 { Value::f(vers_d(&g) + vers_d(&d)) }
                           else if m == 2 { Value::Rationnel(vers_q(&g).add(&vers_q(&d))) }
                           else { Value::Entier(vers_z(&g).add(&vers_z(&d))) },
                    "-" => if m == 3 { Value::f(vers_d(&g) - vers_d(&d)) }
                           else if m == 2 { Value::Rationnel(vers_q(&g).sub(&vers_q(&d))) }
                           else { Value::Entier(vers_z(&g).sub(&vers_z(&d))) },
                    "*" => if m == 3 { Value::f(vers_d(&g) * vers_d(&d)) }
                           else if m == 2 { Value::Rationnel(vers_q(&g).mul(&vers_q(&d))) }
                           else { Value::Entier(vers_z(&g).mul(&vers_z(&d))) },
                    "/" => {
                        if m == 3 {
                            if vers_d(&d) == 0.0 { erreur_runtime("division par zéro"); }
                            Value::f(vers_d(&g) / vers_d(&d))
                        } else if m == 2 {
                            let dq = vers_q(&d);
                            if dq.sgn() == 0 { erreur_runtime("division par zéro"); }
                            Value::Rationnel(vers_q(&g).div(&dq))
                        } else {
                            let dz = vers_z(&d);
                            if dz.sgn() == 0 { erreur_runtime("division par zéro"); }
                            let (nz, dd) = (vers_z(&g), dz);
                            Value::Rationnel(Q::avec_num_den(&nz, &dd))
                        }
                    }
                    "%" => {
                        let dz = vers_z(&d);
                        if dz.sgn() == 0 { erreur_runtime("modulo par zéro"); }
                        Value::Entier(vers_z(&g).fdiv_r(&dz))
                    }
                    "div" => {
                        if m == 3 {
                            let b = vers_d(&d);
                            if b == 0.0 { erreur_runtime("division euclidienne par zéro"); }
                            Value::f((vers_d(&g) / b).floor())
                        } else if m == 2 {
                            let dq = vers_q(&d);
                            if dq.sgn() == 0 { erreur_runtime("division euclidienne par zéro"); }
                            let q = vers_q(&g).div(&dq);
                            Value::Entier(q.numerateur().fdiv_q(&q.denominateur()))
                        } else {
                            let dz = vers_z(&d);
                            if dz.sgn() == 0 { erreur_runtime("division euclidienne par zéro"); }
                            Value::Entier(vers_z(&g).fdiv_q(&dz))
                        }
                    }
                    "^" => {
                        if matches!(d, Value::Flottant(_)) {
                            return Value::f(vers_d(&g).powf(vers_d(&d)));
                        }
                        if matches!(g, Value::Flottant(_)) {
                            return Value::f(vers_d(&g).powf(vers_d(&d)));
                        }
                        let expo = vers_z(&d);
                        if !expo.tient_i64() {
                            erreur_runtime("exposant démesuré (la puissance exploserait au-delà de tout calcul)");
                        }
                        let e2 = expo.en_i64();
                        if matches!(g, Value::Rationnel(_)) {
                            return Value::Rationnel(pow_rationnel(&vers_q(&g), e2));
                        }
                        let base = vers_z(&g);
                        if e2 >= 0 {
                            Value::Entier(base.pow_u(e2 as u64))
                        } else {
                            if base.sgn() == 0 { erreur_runtime("0^exposant négatif"); }
                            Value::Rationnel(pow_rationnel(&Q::depuis_z(&base), e2))
                        }
                    }
                    _ => erreur_runtime(&format!("opérateur '{}' inconnu", op)),
                }
            }
        }
    }

    /* ── appels ────────────────────────────────────────────────────── */
    fn appel(&mut self, e: &E) -> Value {
        let (module, nom, args) = match e {
            E::Appel { module, nom, args, .. } => (module.clone(), nom.clone(), args),
            _ => unreachable!(),
        };
        if module == "dynamique" {
            let vs: Vec<Value> = args.iter().map(|a| self.eval(a)).collect();
            return self.dynamique(&nom, &vs);
        }
        if args.len() == 1 && matches!(nom.as_str(), "entier" | "rationnel" | "flottant" | "chaine") {
            let v = self.eval(&args[0]);
            return match nom.as_str() {
                "entier" => match &v {
                    Value::Chaine(s) => Value::Entier(chaine_vers_entier(s)),
                    _ => convertir(v, TyN::Entier),
                },
                "rationnel" => convertir(v, TyN::Rationnel),
                "flottant" => match &v {
                    Value::Chaine(s) => Value::f(chaine_vers_flottant(s)),
                    _ => convertir(v, TyN::Flottant),
                },
                _ => Value::s(&chaine_de(&v)),
            };
        }
        let vs: Vec<Value> = args.iter().map(|a| self.eval(a)).collect();
        if let Some(sig) = self.prog.signatures.get(&nom) {
            return self.appeler(sig, vs);
        }
        self.stdlib(&nom, &mut vs.into_iter().collect::<Vec<_>>(), e_ligne(e))
    }

    fn appeler(&mut self, sig: &Signature, mut args: Vec<Value>) -> Value {
        if sig.corps.is_none() {
            return self.appeler_externe(sig, &mut args);
        }
        for (i, (pn, pt)) in sig.params.iter().enumerate() {
            let _ = pn;
            args[i] = convertir(std::mem::replace(&mut args[i], Value::Vide), pt.n);
        }
        let mut trame: HashMap<String, Value> = HashMap::new();
        for ((pn, _), a) in sig.params.iter().zip(args.into_iter()) {
            trame.insert(pn.clone(), a);
        }
        self.trames.push(trame);
        let mut res = Value::Vide;
        if let Some(corps) = &sig.corps {
            for i in corps {
                if let Err(r) = self.exec(i) {
                    res = convertir(r.v, sig.retour.n);
                    break;
                }
            }
        }
        self.trames.pop();
        res
    }

    fn poignee_bibliotheque(&mut self, sig: &Signature) -> usize {
        let nom = if !sig.dll_son.is_empty() { sig.dll_son.clone() }
                  else if !sig.dll.is_empty() { sig.dll.clone() }
                  else { "c".to_string() };
        if let Some(h) = self.poignees.get(&nom).copied() { return h; }
        let h = charger_biblio(&nom);
        self.poignees.insert(nom, h);
        h
    }

    fn appeler_externe(&mut self, sig: &Signature, args: &mut [Value]) -> Value {
        let h = self.poignee_bibliotheque(sig);
        let sym = if sig.symbole.is_empty() { sig.nom.clone() } else { sig.symbole.clone() };
        let s = symbole(h, &sym) as usize;
        let zarg = |i: usize| -> i64 { vers_z(&args[i]).en_i64() };
        let darg = |i: usize| -> f64 { vers_d(&args[i]) };
        let sent = |i: usize| -> bool {
            sig.params.get(i).map(|p| p.1.n == TyN::Chaine).unwrap_or(false)
        };
        match sig.retour.n {
            TyN::Entier => match args.len() {
                0 => { let fp: extern "C" fn() -> i64 = unsafe { std::mem::transmute(s) }; Value::z(fp()) }
                1 => { let fp: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(s) }; Value::z(fp(zarg(0))) }
                2 => { let fp: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(s) }; Value::z(fp(zarg(0), zarg(1))) }
                _ => erreur_runtime("fonction externe : plus de 2 arguments entiers non supporté"),
            },
            TyN::Flottant => match args.len() {
                0 => { let fp: extern "C" fn() -> f64 = unsafe { std::mem::transmute(s) }; Value::f(fp()) }
                1 => {
                    if sent(0) {
                        let c = c_string(chaine_arg(&args[0]));
                        let fp: extern "C" fn(*const c_char) -> f64 = unsafe { std::mem::transmute(s) };
                        Value::f(fp(c.as_ptr()))
                    } else if sig.params[0].1.n == TyN::Flottant {
                        let fp: extern "C" fn(f64) -> f64 = unsafe { std::mem::transmute(s) };
                        Value::f(fp(darg(0)))
                    } else if sig.params[0].1.n == TyN::Booleen {
                        let b = matches!(&args[0], Value::Booleen(true));
                        let fp: extern "C" fn(c_int) -> f64 = unsafe { std::mem::transmute(s) };
                        Value::f(fp(if b { 1 } else { 0 }))
                    } else {
                        let fp: extern "C" fn(i64) -> f64 = unsafe { std::mem::transmute(s) };
                        Value::f(fp(zarg(0)))
                    }
                }
                2 => {
                    let p0 = sig.params[0].1.n;
                    let p1 = sig.params[1].1.n;
                    if p0 == TyN::Flottant && p1 == TyN::Flottant {
                        let fp: extern "C" fn(f64, f64) -> f64 = unsafe { std::mem::transmute(s) };
                        Value::f(fp(darg(0), darg(1)))
                    } else if p0 == TyN::Flottant {
                        let fp: extern "C" fn(f64, i64) -> f64 = unsafe { std::mem::transmute(s) };
                        Value::f(fp(darg(0), zarg(1)))
                    } else if p1 == TyN::Flottant {
                        let fp: extern "C" fn(i64, f64) -> f64 = unsafe { std::mem::transmute(s) };
                        Value::f(fp(zarg(0), darg(1)))
                    } else {
                        let fp: extern "C" fn(i64, i64) -> f64 = unsafe { std::mem::transmute(s) };
                        Value::f(fp(zarg(0), zarg(1)))
                    }
                }
                3 => {
                    let fp: extern "C" fn(f64, f64, f64) -> f64 = unsafe { std::mem::transmute(s) };
                    Value::f(fp(darg(0), darg(1), darg(2)))
                }
                _ => erreur_runtime("fonction externe flottante : arité non supportée"),
            },
            TyN::Booleen => match args.len() {
                0 => { let fp: extern "C" fn() -> c_int = unsafe { std::mem::transmute(s) }; Value::Booleen(fp() != 0) }
                1 => { let fp: extern "C" fn(i64) -> c_int = unsafe { std::mem::transmute(s) }; Value::Booleen(fp(zarg(0)) != 0) }
                _ => erreur_runtime("fonction externe booléenne : arité non supportée"),
            },
            TyN::Chaine => match args.len() {
                0 => { let fp: extern "C" fn() -> *const c_char = unsafe { std::mem::transmute(s) }; unsafe { vers_chaine_c(fp()) } }
                1 => {
                    if sent(0) {
                        let c = c_string(chaine_arg(&args[0]));
                        let fp: extern "C" fn(*const c_char) -> *const c_char = unsafe { std::mem::transmute(s) };
                        unsafe { vers_chaine_c(fp(c.as_ptr())) }
                    } else {
                        let fp: extern "C" fn(i64) -> *const c_char = unsafe { std::mem::transmute(s) };
                        unsafe { vers_chaine_c(fp(zarg(0))) }
                    }
                }
                _ => erreur_runtime("fonction externe texte : arité non supportée"),
            },
            _ => erreur_runtime("type de retour externe non supporté"),
        }
    }

    fn dynamique(&mut self, nom: &str, args: &[Value]) -> Value {
        match nom {
            "charger" => {
                let ch = chaine_arg(&args[0]);
                let cs = c_string(ch);
                let h = unsafe { ouvrir_dll(cs.as_ptr(), RTLD_NOW | RTLD_LOCAL) };
                if h.is_null() {
                    erreur_runtime(&format!("chargement de bibliothèque impossible ({})", ch));
                }
                self.poignees_dyn.push(h as usize);
                Value::z(h as i64)
            }
            "fermer" => {
                let h = vers_z(&args[0]).en_u64() as usize;
                unsafe { fermer_dll(h as *mut c_void) };
                self.poignees_dyn.retain(|p| *p != h);
                Value::Vide
            }
            "appeler0" | "appeler1" | "appeler2" => {
                let h = vers_z(&args[0]).en_u64() as usize;
                let sym = chaine_arg(&args[1]).to_string();
                let s = symbole(h, &sym);
                match nom {
                    "appeler0" => { let fp: extern "C" fn() -> f64 = unsafe { std::mem::transmute(s) }; Value::f(fp()) }
                    "appeler1" => {
                        if matches!(args[2], Value::Entier(_)) {
                            let fp: extern "C" fn(i64) -> f64 = unsafe { std::mem::transmute(s) };
                            Value::f(fp(vers_z(&args[2]).en_i64()))
                        } else {
                            let fp: extern "C" fn(f64) -> f64 = unsafe { std::mem::transmute(s) };
                            Value::f(fp(vers_d(&args[2])))
                        }
                    }
                    _ => {
                        let a_ent = matches!(args[2], Value::Entier(_));
                        let b_ent = matches!(args[3], Value::Entier(_));
                        if !a_ent && !b_ent {
                            let fp: extern "C" fn(f64, f64) -> f64 = unsafe { std::mem::transmute(s) };
                            Value::f(fp(vers_d(&args[2]), vers_d(&args[3])))
                        } else if a_ent && !b_ent {
                            let fp: extern "C" fn(i64, f64) -> f64 = unsafe { std::mem::transmute(s) };
                            Value::f(fp(vers_z(&args[2]).en_i64(), vers_d(&args[3])))
                        } else if !a_ent && b_ent {
                            let fp: extern "C" fn(f64, i64) -> f64 = unsafe { std::mem::transmute(s) };
                            Value::f(fp(vers_d(&args[2]), vers_z(&args[3]).en_i64()))
                        } else {
                            let fp: extern "C" fn(i64, i64) -> f64 = unsafe { std::mem::transmute(s) };
                            Value::f(fp(vers_z(&args[2]).en_i64(), vers_z(&args[3]).en_i64()))
                        }
                    }
                }
            }
            _ => erreur_runtime(&format!("fonction dynamique::{} inconnue", nom)),
        }
    }

    /* ── bibliothèque standard ─────────────────────────────────────── */
    fn stdlib(&mut self, nom: &str, a: &mut [Value], ligne: i64) -> Value {
        let _ = ligne;
        match nom {
            "afficher" => { self.afficher(&a[0]); Value::Vide }
            "saisir_entier" => self.lire_selon(&Ty::nouveau(TyN::Entier)),
            "saisir_flottant" => self.lire_selon(&Ty::nouveau(TyN::Flottant)),
            "millisecondes" => {
                let mut ts = CTimeSpec { tv_sec: 0, tv_nsec: 0 };
                unsafe { clock_gettime(1 /* CLOCK_MONOTONIC */, &mut ts) };
                Value::z(ts.tv_sec * 1000 + ts.tv_nsec / 1_000_000)
            }
            "graine" => {
                let s = vers_z(&a[0]);
                let base = if s.tient_u64() { s.en_u64() } else {
                    // mêmes bits de poids faible que le moteur C++
                    let hi = s.fdiv_r(&Z::u(0xFFFF_FFFF)).en_u64();
                    let lo = s.fdiv_r(&Z::u(1 << 32)).en_u64();
                    hi.wrapping_mul(1 << 32).wrapping_add(lo)
                };
                rng_etat().store(base, std::sync::atomic::Ordering::Relaxed);
                rng_tirer();   // casse la corrélation directe avec la graine
                Value::Vide
            }
            "hasard" => {
                let n = vers_z(&a[0]);
                if n.sgn() <= 0 { erreur_runtime("hasard : bornage positif attendu"); }
                if n.tient_u64() {
                    let nn = n.en_u64();
                    let r = if nn & (nn - 1) == 0 { rng_tirer() & (nn - 1) }
                            else { ((rng_tirer() as u128 * nn as u128) >> 64) as u64 };
                    Value::Entier(Z::u(r))
                } else {
                    Value::Entier(Z::u(rng_tirer()).fdiv_r(&n))
                }
            }
            "tampon" => {
                let n = vers_z(&a[0]);
                if n.sgn() < 0 || !n.tient_u64() { erreur_runtime("tampon : taille démesurée"); }
                Value::Tampon(Rc::new(RefCell::new(vec![0u8; n.en_u64() as usize])))
            }
            "octets" => match &a[0] {
                Value::Chaine(s) => Value::Tampon(Rc::new(RefCell::new(s.as_bytes().to_vec()))),
                _ => erreur_runtime("octets : une chaine est attendue"),
            },
            "tampon_case" => {
                let b = match &a[0] {
                    Value::Tampon(t) => t.borrow(),
                    _ => erreur_runtime("tampon_case : tampon attendu"),
                };
                let idx = vers_z(&a[1]);
                if !idx.tient_i64() { erreur_runtime("index hors tampon"); }
                let i = idx.en_i64();
                if i < 0 || i as usize >= b.len() { erreur_runtime("index hors tampon"); }
                Value::Entier(Z::u(b[i as usize] as u64))
            }
            "tampon_pose" => {
                let brc = match &a[0] {
                    Value::Tampon(t) => t.clone(),
                    _ => erreur_runtime("tampon_pose : tampon attendu"),
                };
                let idx = vers_z(&a[1]);
                if !idx.tient_i64() { erreur_runtime("index hors tampon"); }
                let i = idx.en_i64();
                {
                    let b = brc.borrow();
                    if i < 0 || i as usize >= b.len() { erreur_runtime("index hors tampon"); }
                }
                let v = vers_z(&a[2]);
                brc.borrow_mut()[i as usize] = v.fdiv_r(&Z::u(256)).en_u64() as u8;
                Value::Vide
            }
            "remplir_hasard" => {
                let brc = match &a[0] {
                    Value::Tampon(t) => t.clone(),
                    _ => erreur_runtime("remplir_hasard : tampon attendu"),
                };
                let alpha = match &a[1] {
                    Value::Tampon(t) => t.borrow().clone(),
                    _ => erreur_runtime("remplir_hasard : alphabet non vide attendu"),
                };
                if alpha.is_empty() { erreur_runtime("remplir_hasard : alphabet non vide attendu"); }
                let ab = &alpha[..];
                let s = ab.len();
                let mut buf = brc.borrow_mut();
                let n = buf.len();
                let p = buf.as_mut_slice();
                if s == 1 {
                    for x in p.iter_mut() { *x = ab[0]; }
                } else if s & (s - 1) == 0 && s <= 256 {
                    // puissance de deux ≤ 256 : octet par caractère, masque exact
                    let m = (s - 1) as u64;
                    let mut i = 0usize;
                    while i + 32 <= n {
                        let (r0, r1, r2, r3) = (rng_tirer(), rng_tirer(), rng_tirer(), rng_tirer());
                        for k in 0..8usize {
                            p[i + k] = ab[((r0 >> (8 * k)) & m) as usize];
                            p[i + 8 + k] = ab[((r1 >> (8 * k)) & m) as usize];
                            p[i + 16 + k] = ab[((r2 >> (8 * k)) & m) as usize];
                            p[i + 24 + k] = ab[((r3 >> (8 * k)) & m) as usize];
                        }
                        i += 32;
                    }
                    while i < n { p[i] = ab[(rng_tirer() & m) as usize]; i += 1; }
                } else if s & (s - 1) == 0 {
                    // puissance de deux > 256 : extraction bit à bit (64/bits par tirage)
                    let mask = (s - 1) as u64;
                    let bits = 64u32 - (s as u64).leading_zeros();
                    let per = (64 / bits) as usize;
                    let mut i = 0usize;
                    while i + per <= n {
                        let r = rng_tirer();
                        for k in 0..per { p[i + k] = ab[((r >> (bits * k as u32)) & mask) as usize]; }
                        i += per;
                    }
                    while i < n { p[i] = ab[(rng_tirer() & mask) as usize]; i += 1; }
                } else {
                    // Lemire : uniforme sans biais
                    let su = s as u128;
                    for x in p.iter_mut() { *x = ab[(((rng_tirer() as u128) * su) >> 64) as usize]; }
                }
                Value::Vide
            }
            "factorielle" => Value::Entier(rose_factorielle(&vers_z(&a[0]))),
            "fibonacci" => Value::Entier(rose_fibonacci(&vers_z(&a[0]))),
            "pgcd" => Value::Entier(vers_z(&a[0]).gcd(&vers_z(&a[1]))),
            "ppcm" => Value::Entier(vers_z(&a[0]).lcm(&vers_z(&a[1]))),
            "puissance" => {
                let expo = vers_z(&a[1]);
                if !expo.tient_i64() { erreur_runtime("exposant démesuré"); }
                let e = expo.en_i64();
                match &a[0] {
                    Value::Flottant(d) => Value::f(d.powf(e as f64)),
                    Value::Rationnel(q) => Value::Rationnel(pow_rationnel(q, e)),
                    _ => {
                        if e < 0 {
                            let base = vers_z(&a[0]);
                            if base.sgn() == 0 { erreur_runtime("0^exposant négatif"); }
                            Value::Rationnel(pow_rationnel(&Q::depuis_z(&base), e))
                        } else {
                            Value::Entier(vers_z(&a[0]).pow_u(e as u64))
                        }
                    }
                }
            }
            "est_premier" => Value::Booleen(rose_est_premier(&vers_z(&a[0]))),
            "compter_premiers" => Value::Entier(rose_compter_premiers(&vers_z(&a[0]))),
            "nb_chiffres" => Value::Entier(rose_nb_chiffres(&vers_z(&a[0]))),
            "nombre_pi" => Value::s(&rose_pi(&vers_z(&a[0]))),
            "temps" => {
                static DEPART: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
                let d = std::time::Instant::now()
                    .duration_since(*DEPART.get_or_init(std::time::Instant::now));
                Value::z(d.as_nanos().min(i64::MAX as u128) as i64)
            }
            "duree" => {
                let v = vers_z(&a[0]);
                if v.sgn() < 0 { erreur_runtime("duree : la durée doit être positive ou nulle"); }
                Value::s(&formater_duree(&v.texte()))
            }
            "ordre" => {
                let t = match &a[0] {
                    Value::Chaine(s) => s.clone(),
                    _ => erreur_runtime("ordre attend une chaîne d'un seul caractère"),
                };
                let mut it = t.chars();
                match (it.next(), it.next()) {
                    (Some(c), None) => Value::z(c as i64),
                    _ => erreur_runtime("ordre attend une chaîne d'un seul caractère"),
                }
            }
            "caractere" => {
                let v = vers_z(&a[0]);
                if v.sgn() < 0 || !v.tient_u64() || v.en_u64() > 0x10FFFF {
                    erreur_runtime("caractere : code hors plage (0 à 1114111)");
                }
                match char::from_u32(v.en_u64() as u32) {
                    Some(c) => Value::s(&c.to_string()),
                    None => erreur_runtime("caractere : code réservé (55296 à 57343)"),
                }
            }
            "sous_chaine" => {
                let s: String = match &a[0] {
                    Value::Chaine(s) => s.as_str().to_string(),
                    _ => String::new(),
                };
                let d = vers_z(&a[1]);
                let f = vers_z(&a[2]);
                if d.sgn() < 0 || f.sgn() < 0 || !d.tient_u64() || !f.tient_u64() {
                    erreur_runtime("sous_chaine : les bornes doivent être des entiers positifs");
                }
                let taille = s.chars().count() as u64;
                let db = d.en_u64();
                let mut fb = f.en_u64();
                if db > taille || fb < db { erreur_runtime("sous_chaine : bornes invalides"); }
                if fb > taille { fb = taille; }
                let r: String = s.chars().skip(db as usize).take((fb - db) as usize).collect();
                Value::s(&r)
            }
            "concatener" => match &a[0] {
                Value::Liste(l) => {
                    let b = l.borrow();
                    let mut r = String::new();
                    for e in b.iter() {
                        match e {
                            Value::Chaine(s) => r.push_str(s),
                            _ => erreur_runtime("concatener attend une liste de chaînes"),
                        }
                    }
                    Value::s(&r)
                }
                _ => erreur_runtime("concatener attend une liste"),
            },
            "lire_texte" => {
                let chemin = match &a[0] {
                    Value::Chaine(s) => s.as_str().to_string(),
                    _ => String::new(),
                };
                let octets = match std::fs::read(&chemin) {
                    Ok(o) => o,
                    Err(_) => erreur_runtime(&format!("lecture du fichier impossible ({})", chemin)),
                };
                if octets.len() > (1usize << 28) {
                    erreur_runtime(&format!("fichier trop volumineux ({})", chemin));
                }
                Value::s(&String::from_utf8_lossy(&octets).into_owned())
            }
            "ecrire_texte" => {
                let chemin = match &a[0] {
                    Value::Chaine(s) => s.as_str().to_string(),
                    _ => String::new(),
                };
                let contenu = match &a[1] {
                    Value::Chaine(s) => s.as_str().to_string(),
                    _ => String::new(),
                };
                if std::fs::write(&chemin, contenu.as_bytes()).is_err() {
                    erreur_runtime(&format!("écriture du fichier impossible ({})", chemin));
                }
                Value::Vide
            }
            "nb_arguments" => Value::z(PROG_ARGS.get().map_or(0, |v| v.len() as i64)),
            "argument" => {
                let idx = vers_z(&a[0]);
                let args: Vec<String> = PROG_ARGS.get().cloned().unwrap_or_default();
                if idx.sgn() < 0 || !idx.tient_u64() || idx.en_u64() >= args.len() as u64 {
                    erreur_runtime("index hors des limites des arguments");
                }
                Value::s(&args[idx.en_u64() as usize])
            }
            "afficher_erreur" => {
                eprint!("{}\n", ecri_str(&a[0]));
                Value::Vide
            }
            "quitter" => {
                let c = vers_z(&a[0]);
                if c.cmp(&Z::i(i32::MIN as i64)) < 0 || c.cmp(&Z::i(i32::MAX as i64)) > 0 {
                    erreur_runtime("code de sortie hors plage");
                }
                {
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
                    let _ = std::io::stderr().flush();
                }
                std::process::exit(c.en_i64() as i32)
            }
            "longueur" => match &a[0] {
                Value::Liste(l) => Value::z(l.borrow().len() as i64),
                Value::Table(t) => Value::z(t.borrow().len() as i64),
                Value::Tampon(t) => Value::z(t.borrow().len() as i64),
                Value::Chaine(s) => Value::z(s.chars().count() as i64),
                _ => Value::z(0),
            },
            "table" => Value::Table(Rc::new(RefCell::new(Vec::new()))),
            "inserer" => {
                let t = match &a[0] {
                    Value::Table(t) => t.clone(),
                    _ => erreur_runtime("inserer attend une table"),
                };
                let mut t = t.borrow_mut();
                for kv in t.iter_mut() {
                    if valeur_egale(&kv.0, &a[1]) { kv.1 = a[2].clone(); return Value::Vide; }
                }
                t.push((a[1].clone(), a[2].clone()));
                Value::Vide
            }
            "cle_existe" => {
                let t = match &a[0] {
                    Value::Table(t) => t.borrow(),
                    _ => erreur_runtime("cle_existe attend une table"),
                };
                for kv in t.iter() {
                    if valeur_egale(&kv.0, &a[1]) { return Value::Booleen(true); }
                }
                Value::Booleen(false)
            }
            "supprimer" => {
                let t = match &a[0] {
                    Value::Table(t) => t.clone(),
                    _ => erreur_runtime("supprimer attend une table"),
                };
                let mut t = t.borrow_mut();
                for i in 0..t.len() {
                    if valeur_egale(&t[i].0, &a[1]) { t.remove(i); return Value::Booleen(true); }
                }
                Value::Booleen(false)
            }
            "cles" => {
                let t = match &a[0] {
                    Value::Table(t) => t.borrow(),
                    _ => erreur_runtime("cles attend une table"),
                };
                let l: Vec<Value> = t.iter().map(|kv| kv.0.clone()).collect();
                Value::Liste(Rc::new(RefCell::new(l)))
            }
            "valeurs" => {
                let t = match &a[0] {
                    Value::Table(t) => t.borrow(),
                    _ => erreur_runtime("valeurs attend une table"),
                };
                let l: Vec<Value> = t.iter().map(|kv| kv.1.clone()).collect();
                Value::Liste(Rc::new(RefCell::new(l)))
            }
            "joindre" => {
                let liste = match &a[0] {
                    Value::Liste(l) => l.clone(),
                    _ => erreur_runtime("joindre attend une liste"),
                };
                let front_ty = {
                    let b = liste.borrow();
                    b.first().map(|v| match v {
                        Value::Entier(_) => TyN::Entier,
                        Value::Rationnel(_) => TyN::Rationnel,
                        Value::Flottant(_) => TyN::Flottant,
                        _ => TyN::Vide,
                    })
                };
                let elem = match front_ty {
                    Some(ty) if ty != TyN::Vide => convertir(std::mem::replace(&mut a[1], Value::Vide), ty),
                    _ => std::mem::replace(&mut a[1], Value::Vide),
                };
                liste.borrow_mut().push(elem);
                Value::Vide
            }
            "abs" => match &a[0] {
                Value::Entier(z) => Value::Entier(z.abs()),
                Value::Booleen(b) => Value::z(if *b { 1 } else { 0 }),
                Value::Rationnel(q) => {
                    let mut q = q.clone();
                    q.canon();
                    Value::Rationnel(Q::avec_num_den(&q.numerateur().abs(), &q.denominateur()))
                }
                _ => Value::f(vers_d(&a[0]).abs()),
            },
            "min" | "max" => {
                let m = rnk_v(&a[0]).max(rnk_v(&a[1]));
                let ty = match m { 3 => TyN::Flottant, 2 => TyN::Rationnel, _ => TyN::Entier };
                let x = convertir(std::mem::replace(&mut a[0], Value::Vide), ty);
                let y = convertir(std::mem::replace(&mut a[1], Value::Vide), ty);
                let c = cmp_vals(&x, &y);
                if nom == "min" { if c <= 0 { x } else { y } } else { if c >= 0 { x } else { y } }
            }
            "partie_entiere" => match &a[0] {
                Value::Entier(z) => Value::Entier(z.clone()),
                Value::Booleen(b) => Value::z(if *b { 1 } else { 0 }),
                Value::Rationnel(q) => {
                    let mut q = q.clone();
                    q.canon();
                    Value::Entier(q.numerateur().fdiv_q(&q.denominateur()))
                }
                _ => Value::Entier(Z::de_f64(vers_d(&a[0]).floor())),
            },
            "fraction" => {
                let den = vers_z(&a[1]);
                if den.sgn() == 0 { erreur_runtime("fraction de dénominateur nul"); }
                Value::Rationnel(Q::avec_num_den(&vers_z(&a[0]), &den))
            }
            _ => erreur_runtime(&format!("fonction '{}' inconnue [Ligne {}]", nom, ligne)),
        }
    }

    fn executer(&mut self) -> std::io::Result<()> {
        use std::io::Write;
        let mut out = std::io::stdout();
        for i in &self.prog.instructions {
            match self.exec(i) {
                Ok(_) => {}
                Err(_) => {
                    let _ = out.flush();
                    eprintln!("Rose : erreur — 'retour' hors de fonction");
                    std::process::exit(1);
                }
            }
        }
        let _ = out.flush();
        Ok(())
    }
}

fn chaine_arg(v: &Value) -> &str {
    match v {
        Value::Chaine(s) => s,
        _ => "",
    }
}

fn e_ligne(e: &E) -> i64 {
    match e {
        E::Appel { ligne, .. } => *ligne,
        _ => 0,
    }
}

/* ═══════════════════════════ bundle autonome ════════════════════════ */

const ROSE_MAGIC: [u8; 8] = [b'R', b'O', b'S', b'E', b'B', b'1', 0x01, 0x02];

fn chemin_moteur() -> Option<std::path::PathBuf> {
    #[cfg(windows)]
    { std::env::current_exe().ok() }
    #[cfg(not(windows))]
    { std::fs::read_link("/proc/self/exe").ok() }
}

fn lire_bundle() -> Option<String> {
    let chemin = chemin_moteur()?;
    let data = std::fs::read(&chemin).ok()?;
    if data.len() < 24 { return None; }
    let n = data.len();
    let queue = &data[n - 16..];
    if queue[8..16] != ROSE_MAGIC { return None; }
    let ln = u64::from_le_bytes(queue[0..8].try_into().ok()?);
    if ln == 0 || ln as usize > n - 16 { return None; }
    let debut = n - 16 - ln as usize;
    let source = String::from_utf8(data[debut..debut + ln as usize].to_vec()).ok()?;
    Some(crate::ffi::normaliser_fins_de_ligne(&source))
}

fn comp_programme(fichier: &str, sortie: &str) -> i32 {
    let source = match std::fs::read(fichier) {
        Ok(s) => s,
        Err(_) => { eprintln!("rose: fichier introuvable : {}", fichier); return 1; }
    };
    let source = crate::ffi::normaliser_fins_de_ligne(&String::from_utf8_lossy(&source));
    // valide avant d'embarquer
    let mut prog = match analyser(&source) {
        Ok(p) => p,
        Err(e) => { eprintln!("Rose : erreur — {}", e.texte()); return 1; }
    };
    if let Err(e) = replier_constants(&mut prog) {
        eprintln!("Rose : erreur — {}", e.texte());
        return 1;
    }
    if let Err(e) = Verificateur::verifier(&mut prog) {
        eprintln!("Rose : erreur — {}", e.texte());
        return 1;
    }
    let moteur = match chemin_moteur().and_then(|c| std::fs::read(c).ok()) {
        Some(m) => m,
        None => { eprintln!("rose: moteur illisible"); return 1; }
    };
    let mut out: Vec<u8> = moteur;
    out.extend_from_slice(source.as_bytes());
    let ln = (source.len() as u64).to_le_bytes();
    out.extend_from_slice(&ln);
    out.extend_from_slice(&ROSE_MAGIC);
    if std::fs::write(sortie, &out).is_err() {
        eprintln!("rose: ne peut écrire {}", sortie);
        return 1;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(sortie, std::fs::Permissions::from_mode(0o755));
    }
    eprintln!("rose: {} produit (exécutable autonome)", sortie);
    0
}

/* ═══════════════════════════════ CLI ════════════════════════════════ */

fn run_pipeline(src: &str) -> Result<(), String> {
    let mut prog = analyser(src).map_err(|e| e.texte())?;
    replier_constants(&mut prog).map_err(|e| e.texte())?;
    Verificateur::verifier(&mut prog).map_err(|e| e.texte())?;
    let mut interp = Interp::nouveau(&prog);
    let _ = interp.executer();
    Ok(())
}

fn executer_source(source: &str) -> i32 {
    use std::io::Write;
    let src = crate::ffi::normaliser_fins_de_ligne(source);
    // grande pile pour la récursion profonde (comme le moteur C++)
    let builder = std::thread::Builder::new().stack_size(512 * 1024 * 1024);
    let joint = match builder.spawn(move || run_pipeline(&src)) {
        Ok(h) => match h.join() {
            Ok(r) => Some(r),
            Err(_) => Some(Err(String::new())),   // panic : code 2 discret
        },
        Err(_) => None,                            // pas de thread possible : en ligne
    };
    let r = match joint {
        Some(r) => r,
        None => run_pipeline(source),
    };
    match r {
        Ok(()) => 0,
        Err(msg) => {
            let _ = std::io::stdout().flush();
            if !msg.is_empty() { eprintln!("Rose : erreur — {}", msg); }
            1
        }
    }
}

const USAGE: &str = "\
Rose 2.0 — moteur natif Rust (binaire autonome, maths exactes embarquées)
Usage :
  rose prog.rose            interpréter un programme Rose
  rose run prog.rose        interpréter (identique)
  rose comp prog.rose [-o sortie]   produire un exécutable autonome
  rose info                 état du moteur et des dépendances
  rose version              numéro de version
";

pub fn main_cli(argv: Vec<String>) -> i32 {
    if let Some(source) = lire_bundle() {
        let _ = PROG_ARGS.set(argv[1..].to_vec());
        return executer_source(&source);
    }
    let action = argv.get(1).cloned().unwrap_or_default();
    if action == "version" {
        println!("Rose 3.13.1 — moteur natif Rust/GMP");
        return 0;
    }
    if action == "info" {
        println!("rose 3.13.0");
        println!("  moteur            : natif 64 bits Rust — binaire autonome, aucune dépendance hors libc");
        println!("  maths exactes     : GMP liée statiquement (entiers et rationnels sans limite)");
        println!("  interpréteur      : intégré (mode par défaut)");
        println!("  compilation       : rose comp — exécutable autonome (moteur + programme)");
        #[cfg(not(windows))]
        println!("  FFI               : @dll/@sym + dynamique:: via dlopen (libc/libm intégrés)");
        #[cfg(windows)]
        println!("  FFI               : @dll/@sym + dynamique:: via LoadLibrary (msvcrt intégrée)");
        return 0;
    }
    if action == "aide" || action == "--help" || action == "-h" {
        print!("{}", USAGE);
        return 0;
    }
    if action.is_empty() {
        eprint!("{}", USAGE);
        return 1;
    }
    if argv.iter().any(|a| a == "--cible") || action == "export" {
        eprint!("rose: le moteur natif interprète (rose prog.rose) et produit des\n\
                 \x20     exécutables autonomes (rose comp prog.rose) sans cible externe.\n\
                 \x20     Pour émettre du code C/Python/Julia/Rust/Zig : rose2/bin/rose\n\
                 \x20     run fichier.rose --cible X\n");
        return 1;
    }
    if action == "run" {
        let pos = argv[2..].iter().position(|a| !a.starts_with('-')).map(|p| p + 2);
        let fichier = match pos {
            Some(p) => argv[p].clone(),
            None => { eprintln!("rose: quel programme interpréter ?"); return 1; }
        };
        let _ = PROG_ARGS.set(argv[pos.unwrap_or(1) + 1..].to_vec());
        let source = match std::fs::read(&fichier) {
            Ok(s) => String::from_utf8_lossy(&s).into_owned(),
            Err(_) => { eprintln!("rose: fichier introuvable : {}", fichier); return 1; }
        };
        return executer_source(&source);
    }
    if action == "comp" || action == "build" {
        let mut fichier = String::new();
        let mut sortie = String::new();
        let mut i = 2usize;
        while i < argv.len() {
            if argv[i] == "-o" || argv[i] == "--sortie" {
                if i + 1 < argv.len() { sortie = argv[i + 1].clone(); i += 2; continue; }
            } else if !argv[i].starts_with('-') {
                fichier = argv[i].clone();
            }
            i += 1;
        }
        if fichier.is_empty() { eprintln!("rose: quel programme compiler ?"); return 1; }
        if sortie.is_empty() {
            #[cfg(windows)]
            let base = fichier.rsplit(['\\', '/']).next().unwrap_or(&fichier).to_string();
            #[cfg(not(windows))]
            let base = fichier.rsplit('/').next().unwrap_or(&fichier).to_string();
            sortie = match base.rfind(".rose") {
                Some(p) => base[..p].to_string(),
                None => base,
            };
            if sortie.is_empty() { sortie = "programme".into(); }
        }
        #[cfg(windows)]
        { if !sortie.to_lowercase().ends_with(".exe") { sortie.push_str(".exe"); } }
        return comp_programme(&fichier, &sortie);
    }
    // premier argument = fichier .rose sans sous-commande
    let fichier = action.clone();
    let _ = PROG_ARGS.set(argv[2..].to_vec());
    let source = match std::fs::read(&fichier) {
        Ok(s) => String::from_utf8_lossy(&s).into_owned(),
        Err(_) => { eprintln!("rose: fichier introuvable : {}", fichier); return 1; }
    };
    executer_source(&source)
}
