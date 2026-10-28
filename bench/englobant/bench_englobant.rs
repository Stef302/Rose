#![allow(dead_code)]
// ─── Bench englobant V1 : jumeau Rust EXACT de bench_englobant.rose ────────
// AFFICHAGE : la premiere ligne passe par la FONCTION COURTE FIDELE, la
// largeur EXACTE de l'afficheur de l'interpreteur Rose (texte_flottant,
// rose.cpp ~1615 : la plus petite precision p ∈ 1..17 qui se RELISE en
// exactement le meme double). Un « {:.17} » ou « {:.17e }» fixe écrirait autre
// chose que l'interpreteur pour le MEME double — la parite comparant des
// CHAINES, la largeur d'affichage est une donnee du banc, pas un detail.
// Mêmes formules, mêmes bornes, MÊME largeur native : sommes u64 wrapping
// (jamais déclenchées ici : les bornes restent sous 2^64), f64 partout,
// UNE SEULE chaîne de dépendance flottante en S4, huit ops par élément
// (l'addition flottante n'est PAS associative : l'ordre des termes est
// recopié à l'opération près — la chaîne Rust est BIT À BIT celle du bench
//   Rose). ──
// Crible = ALGORITHME EXACT de rose_compter_premiers : table v+1, 0 = premier
// potentiel, borne INCLUSIVE [2..N] ; 564 à N=4096 ; 78498 à N=10^6.
// RNG = splitmix64 + Lemire EXACTS, à l'état près près de rose.cpp :
// graine(2026) = etat := 2026 PUIS rng_tirer() ; CHAQUE appel hasards consomme
// l'etat (etat += C ; r = mix(etat) ; Lemire = (r*10^6)>>64) ; DEUX appels par
// element = DEUX tirages.
// Philox = le MEME generateur A COMPTEUR que le runtime (rose_runtime.cpp,
// philox4x32 + rose_remplir_hasard_philox) : 4x32-10, DIX tours ; graine
// DONNEE TELLE QUE en 64 bits → k0 = graine, k1 = graine >> 32 (le runtime decoupe une graine
//   64 bits en k0 = graine et k1 = graine >> 32 ; ici la graine tient dans un
// mot de 32 bits, donc k0 = 2026 et k1 = 0 — et l'etat
// splitmix N'EST PAS consomme par la voie Philox) ; alphabet 64 = MASQUE & 63 ;
// UN APPEL philox4x32 par BLOC DE 4 OCTETS ; stride 8 x 4096 = 32 768 octets
// glanes par passe dans un tampon de 65 536. Somme (jamais un XOR : une somme
// n'est PAS un invariant de permutation, donc la parite est une epreuve VRAIE ;
// l'ordre de lecture est UNIQUE partout, nthreads = 1 pour 8192 blocs).
// Les 4 valeurs = 4 premières lignes : d45, nb5, MC, tot7 ; le temps total est
// imprimé sur une 5e ligne ignorée du comparateur.
// Build : rustc -C opt-level=3 -C target-cpu=native -C overflow-checks=off x.rs
use std::time::Instant;

const C: u64 = 0x9E3779B97F4A7C15;

// splitmix64 EXACT (rose_rng_tirer_local) + Lemire (r*10^6)>>64.
#[inline(always)]
fn smx(z: &mut u64) -> u64 {
    *z = z.wrapping_add(C);
    let mut x = *z;
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D049BB133111EB);
    x ^ (x >> 31)
}
#[inline(always)]
fn hasard(etat: &mut u64) -> u64 {
    (((smx(etat) as u128) * (1_000_000u128)) >> 64) as u64
}

// = rose_compter_premiers EXACT (borne INCLUSIVE [2..N], marqueur 0 =
// premier potentiel). 564 à N=4096 ; 78498 à N=10^6.
fn crib(v: i64) -> i64 {
    if v < 2 { return 0; }
    let mut cr = vec![0u8; (v as usize) + 1];
    let mut nb: i64 = 0;
    let mut i: i64 = 2;
    while i <= v {
        if cr[i as usize] == 0 {
            nb += 1;
            if i <= v / i {
                let mut j = i * i;
                while j <= v { cr[j as usize] = 1; j += i; }
            }
        }
        i += 1;
    }
    nb
}

// Philox 4x32-10 — recopie EXACTE du runtime (rose_runtime.cpp) : generateur A
// COMPTEUR, l'etat splitmix N'EST PAS consomme ici ; chaque mot de 32 bits de
// la sortie devient un OCTET GLANE par la voie MASQUE & 63.
#[inline(always)]
fn philox4x32(ctr: &mut [u32; 4], key: &mut [u32; 2], out: &mut [u32; 4]) {
    const M0: u32 = 0xD2511F53;
    const M1: u32 = 0xCD9E8D57;
    const W0: u32 = 0x9E3779B9;
    const W1: u32 = 0xBB67AE85;
    let [mut c0, mut c1, mut c2, mut c3] = *ctr;
    let (mut k0, mut k1) = (key[0], key[1]);
    for _ in 0..10 {
        let p0 = (M0 as u64).wrapping_mul(c0 as u64);
        let p1 = (M1 as u64).wrapping_mul(c2 as u64);
        let (hi0, lo0) = ((p0 >> 32) as u32, p0 as u32);
        let (hi1, lo1) = ((p1 >> 32) as u32, p1 as u32);
        let n0 = hi1 ^ c1 ^ k0;
        let n1 = lo1;
        let n2 = hi0 ^ c3 ^ k1;
        let n3 = lo0;
        c0 = n0; c1 = n1; c2 = n2; c3 = n3;
        k0 = k0.wrapping_add(W0);
        k1 = k1.wrapping_add(W1);
    }
    *out = [c0, c1, c2, c3];
}

const ALPHA: &[u8] =
    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_";
// taille réelle de l'alphabet = 64 = PUISSANCE DE 2 → le runtime Rose prend la
// voie MASQUE (out & 63), pas la réduction Horsley-Browne.

// forme courte fidele = l'afficheur de l'interprete Rose (texte_flottant).
fn fmt_g(d: f64) -> String {
    // equivalent EXACT de « %.*g » de la libc (l'afficheur de l'interprete Rose,
    // texte_flottant, rose.cpp ~1615) : la plus petite precision p en 1..=17 qui
    // se RELIT en exactement le meme double, et qui se rend dans la MEME FAMILLE
    // de FORMAT que « %g » (forme FIXE quand l'exposant est dans [-4, p),
    // EXPONENTIELLE sinon). Le format « {:e } » de Rust est exponentielle FORCEE :
    // il ecrivait 2.9789793245670356e7 la ou l'interpreteur et le jumeau C++
    // ecrivent 29789793.245670356 = un FAUX DIVERGENT entre trois moteurs qui
    // calculent pareil. Le comparateur du banc porte sur la CHAINE, pas sur le
    // bit pattern : les deux sont le MEME double, mais pas la MEME chaine.
    for p in 1usize..=17 {
        let e = format!("{:.*e}", p - 1, d);
        // l'exposant de cette forme dit si « %g » choisirait la forme fixe
        if let Some(pos) = e.find('e') {
            let exp: i32 = e[pos + 1..].parse().unwrap_or(0);
            if exp >= -4 && exp < p as i32 {
                let dec = (p as i32 - 1 - exp).max(0) as usize;
                let s = format!("{:.*}", dec, d);
                if s.parse::<f64>().ok().map(|x| x.to_bits()) == Some(d.to_bits()) {
                    return if s.contains('.') { s } else { format!("{s}.0") };
                }
            } else {
                if e.parse::<f64>().ok().map(|x| x.to_bits()) == Some(d.to_bits()) {
                    return e;
                }
            }
        }
    }
    format!("{:.17e}", d)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let n: usize = if args.len() > 1 { args[1].parse().unwrap() } else { 1_000_000 };
    let f: usize = if args.len() > 2 { args[2].parse().unwrap() } else { 5 };
    let t0 = Instant::now();

    // S4 : DEUX chaines de dependance ADDITIONNEES A LA FIN (l'addition
    // flottante n'est pas associative : « d45 + d55 » n'est PAS une chaine
    // unique a huit termes — recopiee a l'operation pres du bench Rose).
    let mut d45: f64 = 0.0;
    let mut d55: f64 = 0.0;
    for _p in 0..f {
        d45 = 0.0;
        d55 = 0.0;
        for i in 0..n {
            let x4 = i as f64 / 100.0;
            let y4 = i as f64 / 71.0;
            let u5 = (x4 + y4) * (x4 + y4);
            let u6 = x4 * y4;
            let x3 = x4 * x4;
            let y3 = y4 * y4;
            let xy = x4 * y4;
            d45 = d45 + x3;
            d45 = d45 + y3;
            d45 = d45 + xy;
            d45 = d45 + u6;
            d45 = d45 + u5;
            d45 = d45 + u6;
            d55 = d55 + u5;
            d55 = d55 + u6;
        }
    }
    // S5 : crible = rose_compter_premiers EXACT, borne INCLUSIVE
    let mut nb5: i64 = 0;
    for _p in 0..f { nb5 += crib(n as i64); }
    // S6 : RNG + MC-pi — DEUX appels hasards par élément (x puis y), DEUX
    // tirages ; graine(2026) = etat := 2026 PUIS un tirage d'avance (exact).
    let mut interieur6: i64 = 0;
    for _p in 0..f {
        let mut etat: u64 = 2026;
        smx(&mut etat);                          // graine(2026) : avance unique
        interieur6 = 0;
        for _i in 0..200000usize {
            let x6 = hasard(&mut etat) as f64 / 1000000.0;
            let y6 = hasard(&mut etat) as f64 / 1000000.0;
            if x6 * x6 + y6 * y6 <= 1.0 { interieur6 += 1; }
        }
    }
    // S7 : Philox — stride 8 × 4096 = 32 768 octets GLANÉS par passe dans un
    // tampon de 65 536 (UN mot philox par OCTET glané, 2 mots par élément). Le
    // tampon natif est REINITIALISE a chaque passe (pas de lecture non
    // initialisee = pas d'UB ; le contenu est IDENTIQUE a celui que la voie
    // Philox du runtime ecrit, passe apres passe — la graine ne depend PAS du
    // nombre de passes). Jamais de wrap : la somme par passe < 4096 × 8 × 122
    // ≈ 4 Mo, et les produits restent < 2^64.
    let mut tot7: u64 = 0;
    {
        const BUF: usize = 65536;
        let mut buf = vec![0u8; BUF];
        for _p in 0..f {
            let mut ctr: [u32; 4] = [0, 0, 0, 0];
            let mut key: [u32; 2] = [2026, 0];      // k0 = graine, k1 = graine >> 32
            for b in 0..BUF / 4 {
                let mut out = [0u32; 4];
                ctr[0] = b as u32;
                ctr[1] = (b >> 32) as u32;
                ctr[2] = 0;
                ctr[3] = 0;
                philox4x32(&mut ctr, &mut key, &mut out);
                let i = b * 4;
                for j in 0..4 {
                    buf[i + j] = ALPHA[(out[j] & 63) as usize];
                }
            }
            tot7 = 0;
            for i in 0..4096usize {
                let b = i * 8;
                let m = buf[b] as u64 * 16777216
                      + buf[b + 1] as u64 * 65536
                      + buf[b + 2] as u64 * 256
                      + buf[b + 3] as u64
                      + buf[b + 4] as u64 * 16777216
                      + buf[b + 5] as u64 * 65536
                      + buf[b + 6] as u64 * 256
                      + buf[b + 7] as u64;
                tot7 = tot7.wrapping_add(m);
            }
        }
    }
    let dt = t0.elapsed().as_secs_f64() * 1000.0;
    println!("{}", fmt_g(d45 + d55));
    println!("{}", nb5);
    println!("{}", interieur6);
    println!("{}", tot7);
    println!("min: {:.3} ms", dt);
}
