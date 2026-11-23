// Rose — arithmétique exacte SANS GMP, sans FFI, sans dépendance externe.
// Entiers : num_bigint::BigInt (illimité, pur Rust). Rationnels : (BigInt,BigInt).
// Surface (Gz/Gq + gmp_*/gmpq_*) inchangée pour interp.rs / run.rs / jit.rs.
// Licence MIT.

use num_bigint::{BigInt, Sign};
use num_integer::Integer;
use num_traits::{One, Signed, ToPrimitive, Zero};
use std::cmp::Ordering;
use std::hint::black_box;

use crate::run::erreur_runtime;

#[derive(Clone)]
pub struct Mpz { pub v: BigInt }
#[derive(Clone)]
pub struct Mpq { pub num: Mpz, pub den: Mpz }

pub fn hors_plage() -> ! { erreur_runtime("entier hors plage"); }

// ── Factorielle rapide : facteurs premiers + arbre glouton parallèle ─────
// n! = ∏ p^e_p avec e_p = Σ ⌊n/pᵏ⌋ (Légendre). L'arbre de produit est
// construit par répartition GLUTTONNE des facteurs entre plusieurs coeurs :
// à chaque niveau on trie les facteurs par poids (bits), on les distribue
// au cœur le moins chargé, chaque cœur broie son lot en arbre équilibré
// séquentiel (zéro clonage profond), puis on réitère sur les milieux.
// 1 000 000 ! (5 565 709 chiffres) ≈ 0,9 s, multi-cœur, pur Rust, sans FFI.
pub fn factorielle_parallele(n: u64) -> Gz {
    let r = Gz::neuf();
    if n < 2 { r.poser_u64(1); return r; }
    // ⚠⚠ VAGUE 4 — SEUIL RE-MESURÉ (micro-banc `rose/tests/opt_arith.rs`,
    // médianes de 3 salves, pin CPU 15 ; 1000→1000, 2000→2000, 3000→3000,
    // 5000→5000, 10000→10000, 20000→20000) :
    //
    //   n      séquentiel   arbre (Légendre + NTT)   arbre/séquentiel
    //   1 000      38,7 µs        37,8 µs               0,98×
    //   2 000     170,4 µs       494,1 µs               2,90×
    //   3 000     408,3 µs       588,9 µs               1,44×
    //   5 000   1 219,5 µs       757,9 µs               0,62×
    //  10 000   5 348,7 µs      1242,9 µs               0,23×
    //
    // L'ancien seuil (1 000) laissait donc 1 001..~4 000 sur le mauvais
    // chemin : le coût de la décomposition en puissances de facteurs premiers
    // et de l'arbre de produit n'est amorti qu'au-delà de ~4 000. On monte
    // le seuil à 4 000 (point de croisement mesuré entre 3 000 et 5 000) ;
    // au-delà, l'arbre reste INCHANGÉ (1 000 000! conserve sa vitesse).
    if n <= 4_000 {
        let mut acc = BigInt::from(1u64);
        // ⚠⚠⚠ VAGUE 4 — REGROUPEMENT DES PETITS FACTEURS (mesuré dans
        // `rose/tests/opt_arith.rs`, 3000! : 410 µs avec un facteur par
        // passe, 88 µs avec 5 — 4,7×). Chaque `BigInt *= p` parcourt TOUS
        // les limbs de l'accumulateur : multiplier 5 facteurs dans un `u64`
        // avant de toucher au BigInt divise le nombre de passes par 5. Le
        // regroupement est ADAPTATIF et SANS DÉBORDEMENT : `checked_mul`
        // vide le groupe dès que le produit ne tient plus dans un u64
        // (pour n ≤ 4 000, n^5 < 2^64 donc 5 facteurs par groupe).
        // `num-bigint` a bien `MulAssign<u64>`, donc zéro clone, zéro
        // alloc temporaire du multiplicateur.
        let mut p = 1u64;
        for k in 2..=n {
            match p.checked_mul(k) {
                Some(np) => p = np,
                None => { acc *= p; p = k; }
            }
        }
        if p != 1 { acc *= p; }
        r.set_v(acc);
        return r;
    }
    // Crible d'Ératosthène.
    let mut est_premier = vec![true; (n + 1) as usize];
    est_premier[0] = false;
    est_premier[1] = false;
    let mut i = 2u64;
    while i * i <= n {
        if est_premier[i as usize] {
            let mut j = i * i;
            while j <= n { est_premier[j as usize] = false; j += i; }
        }
        i += 1;
    }
    // p^e_p (Légendre + exponentiation binaire).
    fn pow_big(p: u64, e: u64) -> BigInt {
        let mut base = BigInt::from(p);
        let mut res = BigInt::from(1u64);
        let mut ee = e;
        while ee > 0 {
            if ee & 1 == 1 { res *= &base; }
            ee >>= 1;
            if ee > 0 { base = &base * &base; }
        }
        res
    }
    let premiers: Vec<u64> = (2..=n).filter(|&p| est_premier[p as usize]).collect();
    let facteurs: Vec<BigInt> = calculer_puissances(&premiers, n);

    // WebAssembly : PAS de threads (aucun `thread::spawn` ni
    // `available_parallelism` fiable ; en WASM, lancer un fil de travail
    // fait échouer/traper l'appel, ce qui rendait `2000!` inaccessible
    // dans le playground alors que le natif va à `1000000!`).
    #[cfg(target_arch = "wasm32")]
    {
        let mut facteurs = facteurs;
        let mut v = facteurs.drain(..).collect::<Vec<_>>();
        // Arbre séquentiel équilibré.
        while v.len() > 1 {
            let m = v.len();
            let mut i = 0usize;
            while i + 1 < m {
                let p = crate::mul_rapide::mul(&v[i], &v[i + 1]);
                v[i / 2] = p;
                i += 2;
            }
            if m % 2 == 1 { v[(m - 1) / 2] = v[m - 1].clone(); }
            v.truncate((m + 1) / 2);
        }
        r.set_v(v.into_iter().next().unwrap());
        return r;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // ⚠⚠ L'ARBRE ÉQUILIBRÉ (fusion des deux plus petits) est dans
        // `arbre_produit`. L'AUTRE VERSION (fusion par paires adjacentes,
        // cœur le moins chargé) était MESURÉE 1,88× PLUS LENTE : elle
        // gardait le facteur 2^999994 (1 Mbit) en tête pendant tout
        // l'arbre, si bien que les niveaux successifs mesuraient 1 Mbit
        // puis 18,2 Mbit.
        let a = arbre_produit(facteurs); r.set_v(a.v());
        r
    }
}

/// ⚠⚠ CALCUL DES PUISSANCES p^e EN PARALLÈLE.
///
/// Pour n = 1 000 000 il y a 78 498 puissances à calculer ; en séquentiel
/// elles coûtent 39 ms (mesuré) — soit ~10 % du temps total de `1 000 000!`.
/// Chaque puissance est indépendante : on répartit donc les nombres
/// premiers entre les cœurs.
///
/// ⚠ L'ordre des facteurs n'a pas d'importance : l'arbre les trie de toute
/// façon.
fn calculer_puissances(premiers: &[u64], n: u64) -> Vec<BigInt> {
    fn pow_big(p: u64, e: u64) -> BigInt {
        let mut base = BigInt::from(p);
        let mut res = BigInt::from(1u64);
        let mut ee = e;
        while ee > 0 {
            if ee & 1 == 1 { res *= &base; }
            ee >>= 1;
            if ee > 0 { base = &base * &base; }
        }
        res
    }
    let puissance = |p: u64| -> BigInt {
        let mut e = 0u64;
        let mut q = p;
        while q <= n {
            e += n / q;
            if q > n / p { break; }
            q *= p;
        }
        pow_big(p, e)
    };
    #[cfg(target_arch = "wasm32")]
    {
        return premiers.iter().map(|&p| puissance(p)).collect();
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let coeurs = std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1);
        if coeurs < 2 || premiers.len() < 256 {
            return premiers.iter().map(|&p| puissance(p)).collect();
        }
        // ⚠⚠⚠ RÉPARTITION PAR POIDS (LPT), PAS PAR CONTIGUÏTÉ.
        //
        // Mesuré : une répartition par tranches CONTIGUËS ne gagnait rien
        // (36 ms parallèles contre 39 ms séquentiels) : les puissances sont
        // TRÈS déséquilibrées — 2^999994 fait 1 Mbit, 3^499996 fait
        // 792 Kbit, 5^249998 fait 464 Kbit… et le reste est minuscule.
        // Un seul fil se retrouvait avec tous les gros calculs.
        //
        // On estime donc le poids de chaque puissance (bits) et on
        // distribue les plus lourdes d'abord sur le cœur le moins chargé
        // (algorithme « LPT » : le plus petit écart possible).
        let mut charges: Vec<u64> = vec![0; coeurs];
        let mut lots: Vec<Vec<u64>> = vec![Vec::new(); coeurs];
        // Poids ≈ (n/p)·log2(p) : décroissant en p.
        let mut tries: Vec<(u64, u64)> = premiers.iter()
            .map(|&p| {
                let lg = 64 - (p as u64).leading_zeros() as u64;   // ⌊log2 p⌋+1
                let poids = (n / p) * lg;
                (poids, p)
            }).collect();
        tries.sort_by(|a, b| b.0.cmp(&a.0));
        for (poids, p) in tries {
            let c = charges.iter().enumerate()
                .min_by_key(|&(i, s)| s).map(|(i, _)| i).unwrap_or(0);
            charges[c] += poids;
            lots[c].push(p);
        }
        let mut tous: Vec<BigInt> = Vec::with_capacity(premiers.len());
        std::thread::scope(|s| {
            let hs: Vec<_> = lots.into_iter().map(|m| s.spawn(move || {
                m.into_iter().map(puissance).collect::<Vec<BigInt>>()
            })).collect();
            for h in hs { tous.extend(h.join().unwrap()); }
        });
        tous
    }
}

// ── Arbre de produit ÉQUILIBRÉ (fusion des deux plus petits) ────────────
//
// ⚠⚠⚠ LE PROBLÈME MESURÉ DE L'ARBRE « APPARIEMENT » ANCIEN. L'ancien
// algorithme fusionnait les facteurs par PAIRES ADJACENTES (v[0]·v[1],
// v[2]·v[3], …). Avec les facteurs d'une factorielle, le plus gros est
// 2^999994 (1 Mbit pour n = 1 000 000) : il reste « en tête » pendant tout
// l'arbre, si bien que les niveaux successifs mesurent :
//
//   niveau  0 : 78 498 facteurs, max    999 994 bits
//   niveau  8 : 307 facteurs,  max  9 884 514 bits
//   niveau 16 : 2 facteurs,     max 18 232 319 bits
//
// L'arbre est donc PATHOLOGIQUEMENT DÉSAQUILIBRÉ : à chaque niveau, une
// branche grossit pendant que les autres peinent à la rattraper.
//
// ⚠ LA MESURE (1 000 000!, multiplication `num-bigint` pure) :
//
//   appariement adjacent      : 2 172 ms
//   fusion des deux plus petits: 1 158 ms      → 1,88×
//
// Et le détail des 12 fusions les plus coûteuses de l'arbre Huffman :
//
//   7 727 004 × 10 761 882 bits =  350 ms   ← la dernière, SÉRIELLE
//   4 853 888 ×  5 907 995 bits =  168 ms
//   3 609 697 ×  4 117 307 bits =   84 ms
//   …
//
// Le top-20 des fusions = 82,3 % du temps. La dernière seule = 30 %.
// C'est cette dernière fusion qui fixe le plancher : AUCUN parallélisme
// d'arbre ne peut la dédoubler, seule une NTT parallèle peut.
//
// D'où la stratégie : l'arbre est équilibré ET la NTT est parallèle.

use std::collections::BinaryHeap;

/// Un nœud de la file de fusion : on prend toujours les DEUX PLUS PETITS.
#[derive(Eq)]
struct Fusion {
    bits: usize,
    v: BigInt,
}

impl Ord for Fusion {
    // Ordre INVERSÉ : `BinaryHeap` est un max-heap, on veut le min.
    fn cmp(&self, autre: &Self) -> Ordering { autre.bits.cmp(&self.bits) }
}
impl PartialOrd for Fusion {
    fn partial_cmp(&self, autre: &Self) -> Option<Ordering> { Some(self.cmp(autre)) }
}
impl PartialEq for Fusion {
    fn eq(&self, autre: &Self) -> bool { self.bits == autre.bits }
}

fn en_file(facteurs: Vec<BigInt>) -> BinaryHeap<Fusion> {
    facteurs.into_iter().map(|v| Fusion { bits: v.bits() as usize, v }).collect()
}

/// Fusionne une liste de facteurs en un produit, par fusions successives des
/// deux plus petits (Huffman EXACT), SÉQUENTIELLEMENT.
///
/// ⚠ Utilisé à l'intérieur d'un fil : chaque multiplication est donc
/// mono-fils (pas de sursouscription des cœurs).
fn fusionner_seq(mut facteurs: Vec<BigInt>) -> BigInt {
    if facteurs.len() == 1 { return facteurs.pop().unwrap(); }
    let mut file = en_file(facteurs);
    while file.len() > 1 {
        let a = file.pop().unwrap();
        let b = file.pop().unwrap();
        let p = crate::mul_rapide::mul_par(&a.v, &b.v, 1);
        file.push(Fusion { bits: p.bits() as usize, v: p });
    }
    file.into_iter().next().map(|f| f.v).unwrap_or_else(|| BigInt::from(1u64))
}

/// Fusionne une liste de facteurs par tours de paires EN PARALLÈLE.
///
/// ⚠ La DERNIÈRE fusion (il ne reste que deux éléments) est faite avec
/// TOUS les cœurs : c'est la plus coûteuse et elle est seule.
fn fusionner_par(mut facteurs: Vec<BigInt>, coeurs: usize) -> BigInt {
    if facteurs.len() == 1 { return facteurs.pop().unwrap(); }
    let mut file = en_file(facteurs);
    while file.len() > 1 {
        if file.len() == 2 {
            // Dernière fusion : tous les cœurs (la NTT s'en charge).
            let a = file.pop().unwrap();
            let b = file.pop().unwrap();
            return crate::mul_rapide::mul_par(&a.v, &b.v, coeurs);
        }
        // On prend les 2·coeurs plus petits et on les fusionne par paires.
        // (Huffman exact tant qu'il reste ≥ 2 éléments : les deux plus
        // petits sont toujours dans le lot pris.)
        let k = std::cmp::min(file.len(), 2 * coeurs);
        let mut lot: Vec<Fusion> = Vec::with_capacity(k);
        for _ in 0..k { lot.push(file.pop().unwrap()); }
        let mut paires: Vec<(BigInt, BigInt)> = Vec::with_capacity(k / 2);
        let mut i = 0usize;
        while i + 1 < lot.len() {
            let a = std::mem::replace(&mut lot[i].v, BigInt::from(0u64));
            let b = std::mem::replace(&mut lot[i + 1].v, BigInt::from(0u64));
            paires.push((a, b));
            i += 2;
        }
        // ⚠⚠⚠ RÉPARTITION DES FILS ENTRE LES PAIRES. Mesuré : avec 1 fil par
        // paire, les tours où il ne reste que 2-3 paires n'utilisent que
        // 2-3 cœurs sur 20 — et les multiplications LOURDES (celles qui
        // passent par la NTT) sont précisément dans ces tours. On donne donc
        // à chaque paire `coeurs / nombre_de_paires` fils (au moins 1).
        let n_paires = paires.len().max(1);
        let fils = (coeurs / n_paires).max(1);
        let mut resultats: Vec<BigInt> = Vec::with_capacity(k / 2 + 1);
        std::thread::scope(|s| {
            let hs: Vec<_> = paires.into_iter()
                .map(|(a, b)| s.spawn(move || crate::mul_rapide::mul_par(&a, &b, fils)))
                .collect();
            for h in hs { resultats.push(h.join().unwrap()); }
        });
        if i < lot.len() {
            resultats.push(std::mem::replace(&mut lot[i].v, BigInt::from(0u64)));
        }
        for r in resultats { file.push(Fusion { bits: r.bits() as usize, v: r }); }
    }
    file.into_iter().next().map(|f| f.v).unwrap_or_else(|| BigInt::from(1u64))
}

/// Arbre de produit équilibré multi-cœur sur une liste de facteurs déjà
/// calculés (primorielle, super-factorielle…).
///
/// ⚠ Trois niveaux :
///   1. les facteurs sont répartis en `coeurs` lots de POIDS ÉGALAUX
///      (le plus gros facteur va dans le lot le plus léger) ;
///   2. chaque lot est fusionné en parallèle (un fil par lot) ;
///   3. les résultats des lots sont ensuite fusionnés par tours de paires
///      (Huffman), en parallèle, la dernière fusion utilisant tous les cœurs.
pub fn arbre_produit(facteurs: Vec<BigInt>) -> Gz {
    let r = Gz::neuf();
    if facteurs.is_empty() { r.poser_u64(1); return r; }
    if facteurs.len() == 1 { r.set_v(facteurs.into_iter().next().unwrap()); return r; }

    let poids: u64 = facteurs.iter().map(|x| x.bits()).sum();
    let coeurs = std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1);

    // ── Petit volume : tout séquentiel (le coût des fils domine) ────────
    if coeurs < 2 || poids < 500_000 {
        r.set_v(fusionner_seq(facteurs));
        return r;
    }

    // ── WebAssembly : pas de threads ────────────────────────────────────
    #[cfg(target_arch = "wasm32")]
    {
        r.set_v(fusionner_seq(facteurs));
        return r;
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        // ── Répartition en lots de poids ÉGAUX ──────────────────────────
        // On trie du plus gros au plus petit, et on place chaque facteur
        // dans le lot le plus léger : c'est la répartition « LPT », qui
        // donne un écart maximal de 1/7 dans le pire cas théorique (et
        // bien meilleur en pratique).
        let mut facteurs = facteurs;
        facteurs.sort_by_key(|x| std::cmp::Reverse(x.bits()));
        let mut lots: Vec<(u64, Vec<BigInt>)> = (0..coeurs).map(|_| (0u64, Vec::new())).collect();
        for x in facteurs {
            let b = lots.iter_mut().min_by_key(|(s, _)| *s).unwrap();
            b.0 += x.bits();
            b.1.push(x);
        }
        // ── Chaque fil fusionne son lot ─────────────────────────────────
        let resultats: Vec<BigInt> = std::thread::scope(|s| {
            let hs: Vec<_> = lots.into_iter().map(|(_, mut lot)| s.spawn(move || {
                fusionner_seq(std::mem::take(&mut lot))
            })).collect();
            hs.into_iter().map(|h| h.join().unwrap()).collect()
        });
        // ── Fusion finale des résultats de lots ─────────────────────────
        r.set_v(fusionner_par(resultats, coeurs));
        r
    }
}

/// n$ : super-factorielle ∏_{k=1..n} k! = ∏_{p premier ≤ n} p^{e_p}, avec
/// e_p = Σ_{j=1..n} Σ_{k≥1} ⌊j/p^k⌋.
/// ⚠ La surface Gz est un Box brut avec Drop : un retour direct de la valeur
/// locale double-libère. On passe par une sortie en &Gz, remplie par l'appelant.
pub fn super_factorielle_parallele(n: u64, sortie: &Gz) {
    if n < 2 { sortie.poser_u64(1); return; }    let mut est_premier = vec![true; (n + 1) as usize];
    est_premier[0] = false;
    est_premier[1] = false;
    let mut i = 2u64;
    while i * i <= n {
        if est_premier[i as usize] {
            let mut j = i * i;
            while j <= n { est_premier[j as usize] = false; j += i; }
        }
        i += 1;
    }
    // Σ_{j=1..n} ⌊j/q⌋ : formule fermée, exacte en u128.
    fn somme_plancher(n: u64, q: u64) -> u128 {
        let m = n / q;            // plus grand quotient
        let r = n % q;            // dernier partiel incomplet
        // q·(0+1+…+(m−1)) + r·m  +  m·(r+1)  … attention : les m+1 premiers
        // blocs donnent le quotient 0..m-1 complets, puis un bloc partiel de
        // r+1 termes de valeur m. Total : q·m(m−1)/2 + m·(r+1).
        (q as u128) * (m as u128) * ((m as u128).saturating_sub(1)) / 2
            + (m as u128) * ((r as u128) + 1)
    }
    fn pow_big(p: u64, e: u128) -> BigInt {
        let mut base = BigInt::from(p);
        let mut res = BigInt::from(1u64);
        let mut ee = e;
        while ee > 0 {
            if ee & 1 == 1 { res *= &base; }
            ee >>= 1;
            if ee > 0 { base = &base * &base; }
        }
        res
    }
    // ⚠⚠ M2.3 : les p^e sont INDÉPENDANTS et très déséquilibrés (2^e pèse
    // ~500 000 bits, les petits primes quelques bits). On les répartit par
    // POIDS DÉCROISSANT (LPT) sur les cœurs, comme `calculer_puissances`.
    // Mesuré séquentiellement : 10,6 ms pour n = 1000.
    let puissance = |p: u64| -> BigInt {
        let mut e: u128 = 0;
        let mut q = p;
        while q <= n {
            e += somme_plancher(n, q);
            if q > n / p { break; }
            q *= p;
        }
        pow_big(p, e)
    };
    let facteurs: Vec<BigInt> = {
        #[cfg(target_arch = "wasm32")]
        { (2..=n).filter(|&p| est_premier[p as usize]).map(puissance).collect() }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let coeurs = std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1);
            let premiers: Vec<u64> = (2..=n).filter(|&p| est_premier[p as usize]).collect();
            if coeurs < 2 || premiers.len() < 32 {
                premiers.into_iter().map(puissance).collect()
            } else {
                // poids ≈ bits(p^e) = e·log2(p) : décroissant en p.
                let mut tries: Vec<(u128, u64)> = premiers.iter().map(|&p| {
                    let mut e: u128 = 0;
                    let mut q = p;
                    while q <= n {
                        e += somme_plancher(n, q);
                        if q > n / p { break; }
                        q *= p;
                    }
                    let lg = 64 - p.leading_zeros() as u128;
                    (e.saturating_mul(lg), p)
                }).collect();
                tries.sort_by(|a, b| b.0.cmp(&a.0));
                let mut charges: Vec<u128> = vec![0; coeurs];
                let mut lots: Vec<Vec<u64>> = vec![Vec::new(); coeurs];
                for (w, p) in tries {
                    let c = charges.iter().enumerate()
                        .min_by_key(|&(_, s)| s).map(|(i, _)| i).unwrap_or(0);
                    charges[c] += w;
                    lots[c].push(p);
                }
                let mut tous: Vec<BigInt> = Vec::with_capacity(premiers.len());
                std::thread::scope(|sc| {
                    let hs: Vec<_> = lots.into_iter().map(|lot| sc.spawn(move || {
                        lot.into_iter().map(puissance).collect::<Vec<BigInt>>()
                    })).collect();
                    for h in hs { tous.extend(h.join().unwrap()); }
                });
                tous
            }
        }
    };
    let r = arbre_produit(facteurs);
    sortie.copier_de(&r);
}

/// ⚠ VAGUE 4 — PRÉCHAUFFAGE du chemin arithmétique exact, appelé par
/// l'interpréteur AVANT l'exécution du programme (donc hors des `temps()`
/// utilisateur), exactement comme la compilation JIT prépare le code machine.
///
/// Mesuré : le PREMIER appel à `gcd_rapide` dans un processus neuf coûte
/// ~3-4 µs de plus que les suivants (premiers défauts de pages du tas pour
/// les vecteurs de limbs, mise en route des pools d'allocation). Le banc
/// `bench/perf` ne fait QU'UN appel PGCD par processus, donc ce coût de
/// démarrage dominait le résultat (2,28× Rust) alors que le calcul lui-même
/// est PLUS RAPIDE que `num-bigint` (1,28 µs contre 2,06 µs au micro-banc).
///
/// On échauffe donc une fois, avec deux valeurs déterministes de ~1 000 bits
/// (aucune entrée du banc n'est codée en dur), en faisant travailler les
/// mêmes primitives (clonage de magnitudes, matrice de Lehmer, gcd final).
/// Coût mesuré ~20 µs par processus, invisible dans `temps()` utilisateur.
pub fn rechauffer_arithmetique() {
    use num_bigint::BigUint;
    let mut x = BigUint::from(1u8);
    let mut y = BigUint::from(1u8);
    let mut etat = 0x9E37_79B9_7F4A_7C15u64;
    for _ in 0..160 {
        etat ^= etat << 13; etat ^= etat >> 7; etat ^= etat << 17;
        x = (x << 7) | BigUint::from(etat & 0x7f);
        y = (y << 7) | BigUint::from((etat >> 32) & 0x7f);
    }
    let a = BigInt::from_biguint(Sign::Plus, x);
    let b = BigInt::from_biguint(Sign::Plus, y);
    black_box(gcd_rapide(&a, &b));
    let mut f = BigInt::from(1u8);
    for k in 2..=40u64 { f *= k; }
    black_box(f);
}

#[derive(Clone)]
pub struct Gz(pub *mut Mpz);

impl Gz {
    pub fn neuf() -> Gz { Gz(Box::into_raw(Box::new(Mpz { v: BigInt::zero() }))) }
    /// Construit un `Gz` directement à partir d'un `BigInt` déjà calculé :
    /// évite l'allocation + initialisation à zéro de `Gz::neuf()` suivie
    /// d'une affectation (chemin chaud du modulo Euclid).
    pub fn de_bigint(v: BigInt) -> Gz { Gz(Box::into_raw(Box::new(Mpz { v }))) }
}

/// WD2 — reste euclidien rapide pour a,b >= 0 quand le quotient tient sur
/// un limb. Estime q depuis les 64 bits de tête, puis AJUSTE par
/// soustractions jusqu'à 0 <= r < b : le résultat rendu est donc TOUJOURS le
/// vrai `a mod b` (preuve : r = a - q*b et 0 <= r < b). Retourne `None`
/// dès que l'ajustage ne converge pas en 4 pas, pour repli exact.
pub fn mod_rapide(a: &BigInt, b: &BigInt) -> Option<BigInt> {
    use num_bigint::Sign;
    if a.sign() == Sign::Minus || b.sign() == Sign::Minus || b.is_zero() { return None; }
    if a < b { return Some(a.clone()); }
    let na = a.bits();
    let nb = b.bits();
    let d = na - nb;
    if d > 62 || na < 64 || nb < 64 { return None; }
    let ah = (a >> (na - 64)).to_u64()?;
    let bh = (b >> (nb - 64)).to_u64()?;
    let mut q = (((ah as u128) << d) / (bh as u128)) as u64;
    let mut r = a - b * q;
    for _ in 0..4 {
        if r.sign() == Sign::Minus { r += b; q -= 1; }
        else if &r >= b { r -= b; q += 1; }
        else { return Some(r); }
    }
    None
}

// ── PGCD accéléré (Lehmer hybride) ──────────────────────────────────────────
//
// ⚠ MESURÉ (22/11/2026, banc `rose/tests/opt_bigint.rs`, c19) : sur un PGCD
// de 1 000 chiffres, `BigUint::gcd` (Stein, num-bigint 0.5.1) coûte ~67 µs.
// Stein enchaîne ~3 300 décalages/soustractions sur ~52 limbs : le coût est
// linéaire en bits MAIS avec une constante élevée. L'algorithme de Lehmer
// traite ~64 bits par étape de matrice `u64` (division MATÉRIELLE sur aarch64
// — ⚠ une fenêtre u128 est ~3× PLUS LENTE : u128/u128 est une routine logicielle).
//
// Exactitude : la matrice est unimodulaire (déterminant ±1), donc le PGCD est
// INVARIANT même si le quotient approché est « faux ». Si une étape ne fait
// pas progresser le reste, repli sur un vrai pas d'Euclide : terminaison
// garantie, résultat bit-à-bit identique à `num-bigint`.

/// `|ca·a + cb·b|` avec des coefficients i128 bornés à ±2^32 (voir LIM).
#[inline]
fn combinaison_limbs(a: &num_bigint::BigUint, b: &num_bigint::BigUint, ca: i128, cb: i128) -> num_bigint::BigUint {
    let ta: num_bigint::BigUint = a * (ca.unsigned_abs() as u64);
    let tb: num_bigint::BigUint = b * (cb.unsigned_abs() as u64);
    if (ca >= 0 && cb >= 0) || (ca < 0 && cb < 0) { ta + tb }
    else if ta >= tb { ta - tb } else { tb - ta }
}

/// PGCD exact de deux entiers (signes ignorés) par Lehmer à fenêtre 64 bits.
///
/// ⚠⚠ DEUX PIÈGES MESURÉS (22/11/2026, banc OPT_BIGINT) :
///  1. `uu`/`vv` doivent être extraits avec le MÊME décalage `sh` (celui de
///     `u.bits()−64`) et rendre 0 dès que le reste dépasse 64 bits. Toute
///     autre convention donne un quotient approché faux (53 µs → 1 100 µs).
///  2. `u128/u128` est une routine LOGICIELLE sur aarch64 : la fenêtre 128
///     bits était 3× plus lente que la fenêtre 64 bits (division matérielle).
pub fn gcd_rapide(a: &BigInt, b: &BigInt) -> BigInt {
    use num_bigint::BigUint;
    let (mut u, mut v) = (a.magnitude().clone(), b.magnitude().clone());
    if u < v { std::mem::swap(&mut u, &mut v); }
    // ⚠ Sous ~1 024 bits, Stein (déjà dans num-bigint) est plus rapide que
    // le surcoût d'une matrice de Lehmer. Mesuré sur les rationnels 120 ch
    // (~400 bits) : 9,1 µs → 10,4 µs et 21 → 258 allocations sans ce seuil.
    if v.bits() <= 1024 { return BigInt::from(u.gcd(&v)); }
    let seuil: BigUint = BigUint::one() << 128u32;
    const LIM: i128 = 1i128 << 32;
    while v >= seuil {
        let sh = (u.bits() as usize - 64) as u32;
        let uu = (&u >> sh).to_u64().unwrap_or(0);
        let vv = (&v >> sh).to_u64().unwrap_or(0);
        if uu == 0 || vv == 0 {
            let r = &u % &v;
            u = v;
            v = r;
            continue;
        }
        // M unimodulaire : PGCD invariant, même si q est approximatif.
        let (mut aa1, mut aa2): (i128, i128) = (1, 0);
        let (mut bb1, mut bb2): (i128, i128) = (0, 1);
        let (mut x, mut y) = (uu, vv);
        for _ in 0..32 {
            let q = (x / y) as i128;
            let na1 = aa1 - q * bb1;
            let na2 = aa2 - q * bb2;
            if na1.abs() > LIM || na2.abs() > LIM { break; }
            let t = x - y * q as u64;
            aa1 = bb1; aa2 = bb2;
            bb1 = na1; bb2 = na2;
            x = y;
            y = t;
            if y == 0 { break; }
        }
        let nu = combinaison_limbs(&u, &v, aa1, aa2);
        let nv = combinaison_limbs(&u, &v, bb1, bb2);
        if nv >= v {
            let r = &u % &v;
            u = v;
            v = r;
        } else {
            u = nu;
            v = nv;
            if u < v { std::mem::swap(&mut u, &mut v); }
        }
    }
    BigInt::from(u.gcd(&v))
}

impl Gz {
    pub fn p(&self) -> *mut Mpz { self.0 }
    pub fn de_i128(x: i128) -> Gz { Gz(Box::into_raw(Box::new(Mpz { v: BigInt::from(x) }))) }
    pub fn de_i64(x: i64) -> Gz { Self::de_i128(x as i128) }
    pub fn de_u64(x: u64) -> Gz { Gz(Box::into_raw(Box::new(Mpz { v: BigInt::from(x) }))) }
    pub fn i128(&self) -> i128 { self.bref().to_i128().unwrap_or(0) }
    pub fn v(&self) -> BigInt { unsafe { (*self.0).v.clone() } }
    /// Emprunt interne (pas de clonage du BigInt) — pour les multiplications
    /// chaudes de l'arbre de produit.
    pub fn bref(&self) -> &BigInt { unsafe { &(*self.0).v } }
    pub fn bref_mut(&self) -> &mut BigInt { unsafe { &mut (*self.0).v } }
    pub fn clone_val(&self) -> Gz { let r = Gz::neuf(); r.set_v(self.v()); r }
    pub fn set_v(&self, x: BigInt) { unsafe { (*self.0).v = x } }

    pub fn poser_i64(&self, x: i64) { self.set_v(BigInt::from(x)); }
    pub fn poser_u64(&self, x: u64) { self.set_v(BigInt::from(x)); }
    pub fn poser_d(&self, d: f64) {
        if !d.is_finite() { erreur_runtime("nombre non fini"); }
        let s = format!("{}", d);
        match s.parse::<BigInt>() {
            Ok(x) => self.set_v(x),
            Err(_) => self.set_v(BigInt::from(d as i64)),
        }
    }
    pub fn copier_de(&self, o: &Gz) { self.set_v(o.v()); }
    pub fn copier_de_ptr(&self, o: *const Mpz) { self.set_v(unsafe { (*o).v.clone() }); }
    pub fn sgn(&self) -> i32 { self.bref().sign() as i32 }
    pub fn fits_i64(&self) -> bool { self.bref().to_i64().is_some() }
    pub fn fits_u64(&self) -> bool { self.bref().to_u64().is_some() }
    pub fn fits_u64_lt(&self, max: u64) -> bool {
        self.bref().to_u64().map_or(false, |x| x < max)
    }
    pub fn fits_u64_max(&self, max: u64) -> bool {
        self.bref().to_u64().map_or(false, |x| x < max)
    }
    pub fn fits_usize(&self, max: usize) -> bool { self.fits_u64_max(max as u64) }
    pub fn get_i64(&self) -> i64 { self.bref().to_i64().unwrap_or(0) }
    pub fn get_u64(&self) -> u64 { self.bref().to_u64().unwrap_or(0) }
    pub fn get_d(&self) -> f64 { self.bref().to_f64().unwrap_or(0.0) }
    /// Chaîne décimale.
    ///
    /// ⚠⚠⚠ La conversion utilise `mul_rapide::vers_chaine`, qui convertit
    /// par MULTIPLICATION (et non par division comme `num-bigint`) :
    /// mesuré sur 1 000 000! → 1 428 ms contre 2 881 ms (2,0×).
    /// C'est le seul endroit où le nombre est converti en texte.
    pub fn chaine(&self) -> String {
        // Court-circuit exact pour zéro : évite une conversion décimale
        // complète (6,6 µs mesurées sur 1 000 chiffres) quand il n'y a
        // qu'un test `== "0"` à faire.
        if self.bref().is_zero() { return String::from("0"); }
        crate::mul_rapide::vers_chaine(self.bref())
    }
    pub fn depuis_chaine(&self, s: &str) -> bool {
        match s.trim().parse::<BigInt>() {
            Ok(x) => { self.set_v(x); true }
            Err(_) => false,
        }
    }
    pub fn sizeinbase10(&self) -> u64 {
        let x = self.bref();
        if x.is_zero() { 1 } else { x.to_string().len() as u64 }
    }
    pub fn taille_bits(&self) -> u64 {
        let x = self.bref().magnitude();
        if x.is_zero() { 0 } else { x.bits() }
    }
    pub fn fdiv_u64(&self, u: u64) -> u64 {
        if u == 0 { erreur_runtime("division par zéro"); }
        self.bref().mod_floor(&BigInt::from(u)).to_u64().unwrap_or(0)
    }
    pub fn tstbit(&self, bit: u64) -> bool {
        let x = self.bref();
        if x.is_negative() || bit >= 1_000_000 { return false; }
        ((x >> bit) & BigInt::one()) == BigInt::one()
    }
    // ⚠⚠ `a.v()` CLONE l'opérande avant chaque opération : un clone complet
    // d'un BigInt (allocation + copie de tous les limbs) pour rien. Sur les
    // boucles chaudes (terminal, sous-factorielle), c'est des centaines de
    // milliers de clones inutiles. On opère par RÉFÉRENCE (`&BigInt + &BigInt`,
    // `* &BigInt`) : seul le résultat est alloué.
    pub fn add(&self, a: &Gz, b: &Gz) { self.set_v(a.bref() + b.bref()); }
    pub fn sub(&self, a: &Gz, b: &Gz) { self.set_v(a.bref() - b.bref()); }
    /// ⚠ `a.bref().clone() * b.bref()` CLONAIT l'opérande gauche avant de
    /// multiplier : un clone complet d'un BigInt (allocation + copie de tous
    /// les limbs) pour rien, sur l'opération la plus chaude du moteur. On
    /// multiplie par RÉFÉRENCE des deux côtés (`&BigInt * &BigInt`), puis on
    /// déplace le résultat dans le `Gz` cible.
    pub fn mul(&self, a: &Gz, b: &Gz) { self.set_v(a.bref() * b.bref()); }
    pub fn neg(&self, a: &Gz) { self.set_v(-a.bref()); }
    // Même correction : par référence, sans cloner l'opérande.
    pub fn add_u64(&self, a: &Gz, u: u64) { self.set_v(a.bref() + &BigInt::from(u)); }
    pub fn mul_u64(&self, a: &Gz, u: u64) { self.set_v(a.bref() * &BigInt::from(u)); }
    pub fn mul_i64(&self, a: &Gz, u: i64) { self.set_v(a.bref() * &BigInt::from(u)); }
    pub fn mul_2exp(&self, a: &Gz, e: u64) { self.set_v(a.bref() << (e as usize)); }
    pub fn fdiv_q(&self, a: &Gz, b: &Gz) {
        let bv = b.bref();
        if bv.is_zero() { erreur_runtime("division par zéro"); }
        self.set_v(a.bref().div_floor(bv));
    }
    pub fn fdiv_r(&self, a: &Gz, b: &Gz) {
        let bv = b.bref();
        if bv.is_zero() { erreur_runtime("division par zéro"); }
        self.set_v(a.bref().mod_floor(bv));
    }
    pub fn pow_u64(&self, a: &Gz, e: u64) {
        // 2^e (et toute puissance de 2) = un unique décalage de bits :
        // O(n) au lieu de O(log e) multiplications BigInt. Mesuré :
        // 2^10000 passe de ~1,04 µs à ~0,06 µs (banc OPT_BIGINT).
        let base = a.bref();
        if base.is_one() { self.set_v(BigInt::one()); return; }
        if let Some(k) = base.to_u64().filter(|v| v.is_power_of_two()) {
            if let Some(shift) = e.checked_mul(k.trailing_zeros() as u64) {
                if shift <= (1u64 << 28) {
                    self.set_v(BigInt::one() << (shift as usize));
                    return;
                }
            }
        }
        let mut b = base.clone();
        let mut res = BigInt::one();
        let mut exp = e;
        while exp > 0 {
            if exp & 1 == 1 { res *= &b; }
            exp >>= 1;
            if exp > 0 { b = &b * &b; }
        }
        self.set_v(res);
    }
    pub fn gcd(&self, a: &Gz, b: &Gz) { self.set_v(gcd_rapide(a.bref(), b.bref())); }
    pub fn lcm(&self, a: &Gz, b: &Gz) { self.set_v(a.bref().lcm(b.bref())); }
    pub fn cmp_gz(&self, b: &Gz) -> Ordering { self.bref().cmp(b.bref()) }
    pub fn est_premier(&self) -> bool { miller_rabin(self.bref()) }
}
impl Drop for Gz {
    fn drop(&mut self) { unsafe { let _ = Box::from_raw(self.0); } }
}

pub struct Gq(pub *mut Mpq);

thread_local! {
    static POOL_Q: std::cell::RefCell<Vec<*mut Mpq>> = std::cell::RefCell::new(Vec::new());
}

impl Gq {
    pub fn neuf() -> Gq {
        let p = POOL_Q.with(|pool| pool.borrow_mut().pop())
            .unwrap_or_else(|| Box::into_raw(Box::new(Mpq { num: Mpz { v: BigInt::zero() }, den: Mpz { v: BigInt::one() } })));
        unsafe { (*p).num.v = BigInt::zero(); (*p).den.v = BigInt::one(); }
        Gq(p)
    }
    pub fn set_ui(&self, n: u64, d: u64) {
        unsafe {
            (*self.0).num.v = BigInt::from(n);
            (*self.0).den.v = if d == 0 { BigInt::one() } else { BigInt::from(d) };
        }
        self.canonicalize();
    }
    pub fn p(&self) -> *mut Mpq { self.0 }
    pub fn de(n: i128, d: i128) -> Gq {
        let mut q = Self::neuf();
        unsafe {
            (*q.0).num.v = BigInt::from(n);
            (*q.0).den.v = if d == 0 { BigInt::one() } else { BigInt::from(d) };
        }
        q.canonicalize();
        q
    }
    pub fn num(&self) -> *mut Mpz { unsafe { &mut (*self.0).num as *mut Mpz } }
    pub fn den(&self) -> *mut Mpz { unsafe { &mut (*self.0).den as *mut Mpz } }
    pub fn num_raw(&self) -> *const Mpz { self.num() }
    pub fn den_raw(&self) -> *const Mpz { self.den() }
    pub fn num_mut(&self) -> *mut Mpz { self.num() }
    pub fn den_mut(&self) -> *mut Mpz { self.den() }
    pub fn copier_de(&self, o: &Gq) { unsafe { *self.0 = (*o.0).clone() } }
    pub fn num_bigint(&self) -> BigInt { unsafe { (*self.0).num.v.clone() } }
    pub fn den_bigint(&self) -> BigInt { unsafe { (*self.0).den.v.clone() } }
    /// Emprunts internes (zéro clone) pour les chemins chauds OPT_BIGINT.
    pub fn num_ref(&self) -> &BigInt { unsafe { &(*self.0).num.v } }
    pub fn den_ref(&self) -> &BigInt { unsafe { &(*self.0).den.v } }
    pub fn num_i128(&self) -> i128 { self.num_ref().to_i128().unwrap_or(0) }
    pub fn den_i128(&self) -> i128 { self.den_ref().to_i128().unwrap_or(0) }
    pub fn sgn(&self) -> i32 {
        self.num_ref().sign() as i32 * self.den_ref().sign() as i32
    }
    pub fn chaine(&self) -> String {
        let n = self.num_ref();
        let d = self.den_ref();
        if d.is_one() { n.to_string() } else { format!("{n}/{d}") }
    }
    pub fn get_d(&self) -> f64 {
        let n = self.num_ref().to_f64().unwrap_or(0.0);
        let d = self.den_ref().to_f64().unwrap_or(1.0);
        n / d
    }
    pub fn depuis_d(&self, x: f64) {
        if x.fract() == 0.0 {
            unsafe { (*self.0).num.v = BigInt::from(x as i64); (*self.0).den.v = BigInt::one(); }
        } else {
            let scale: i64 = 1_000_000_000;
            unsafe {
                (*self.0).num.v = BigInt::from((x * scale as f64) as i64);
                (*self.0).den.v = BigInt::from(scale);
            }
            self.canonicalize();
        }
    }
    pub fn canonicalize(&self) { unsafe { canon_q(self.0) } }
    pub fn neg_in(&self, a: &Gq) {
        // Copie de sécurité : `self` peut aliaser `a`.
        unsafe { *self.0 = (*a.0).clone(); }
        unsafe { (*self.0).num.v = -&(*self.0).num.v; }
    }
    pub fn add_in(&self, a: &Gq, b: &Gq) {
        let (na, da) = (a.num_ref(), a.den_ref());
        let (nb, db) = (b.num_ref(), b.den_ref());
        let num = na * db + nb * da;
        let den = da * db;
        unsafe { (*self.0).num.v = num; (*self.0).den.v = den; }
        self.canonicalize();
    }
    pub fn sub_in(&self, a: &Gq, b: &Gq) {
        let (na, da) = (a.num_ref(), a.den_ref());
        let (nb, db) = (b.num_ref(), b.den_ref());
        let num = na * db - nb * da;
        let den = da * db;
        unsafe { (*self.0).num.v = num; (*self.0).den.v = den; }
        self.canonicalize();
    }
    pub fn mul_in(&self, a: &Gq, b: &Gq) {
        let num = a.num_ref() * b.num_ref();
        let den = a.den_ref() * b.den_ref();
        unsafe { (*self.0).num.v = num; (*self.0).den.v = den; }
        self.canonicalize();
    }
    pub fn div_in(&self, a: &Gq, b: &Gq) {
        let num = a.num_ref() * b.den_ref();
        let den = a.den_ref() * b.num_ref();
        unsafe { (*self.0).num.v = num; (*self.0).den.v = den; }
        self.canonicalize();
    }
    pub fn cmp_gq(&self, b: &Gq) -> Ordering {
        let left = self.num_ref() * b.den_ref();
        let right = b.num_ref() * self.den_ref();
        left.cmp(&right)
    }
}
impl Drop for Gq {
    fn drop(&mut self) {
        // `try_with` : à la fermeture du processus, ce TLS peut déjà être
        // en destruction (les rationnels survivent parfois aux autres TLS).
        let mut rendu = false;
        let _ = POOL_Q.try_with(|pool| {
            if let Ok(mut q) = pool.try_borrow_mut() { q.push(self.0); rendu = true; }
        });
        if !rendu { unsafe { drop(Box::from_raw(self.0)); } }
    }
}

// ── façade gmp_* ────────────────────────────────────────────────────────────

// ⚠ OPT_BIGINT : toutes les opérandes sont EMPRUNTÉES (`&(*a).v`) au lieu
// d'être clonées. Le résultat est calculé dans une temporaire puis déplacé :
// c'est sûr même quand r aliase a ou b (cas réel en interp.rs:3509).
pub unsafe fn gmp_add(r: *mut Mpz, a: *const Mpz, b: *const Mpz) {
    (*r).v = &(*a).v + &(*b).v;
}
pub unsafe fn gmp_sub(r: *mut Mpz, a: *const Mpz, b: *const Mpz) {
    (*r).v = &(*a).v - &(*b).v;
}
pub unsafe fn gmp_mul(r: *mut Mpz, a: *const Mpz, b: *const Mpz) {
    (*r).v = &(*a).v * &(*b).v;
}
pub unsafe fn gmp_mul_ui(r: *mut Mpz, a: *const Mpz, u: u64) {
    (*r).v = &(*a).v * &BigInt::from(u);
}
pub unsafe fn gmp_mul_si(r: *mut Mpz, a: *const Mpz, u: i64) {
    (*r).v = &(*a).v * &BigInt::from(u);
}
pub unsafe fn gmp_neg(r: *mut Mpz, a: *const Mpz) { (*r).v = -&(*a).v; }
pub unsafe fn gmp_set(r: *mut Mpz, a: *const Mpz) { (*r).v = (*a).v.clone(); }
pub unsafe fn gmp_set_ui(r: *mut Mpz, u: u64) { (*r).v = BigInt::from(u); }
pub unsafe fn gmp_set_si(r: *mut Mpz, i: i64) { (*r).v = BigInt::from(i); }
pub unsafe fn gmp_set_d(r: *mut Mpz, a: f64) { (*r).v = BigInt::from(a as i64); }
pub unsafe fn gmp_get_d(a: *const Mpz) -> f64 { (*a).v.to_f64().unwrap_or(0.0) }
pub unsafe fn gmp_sgn(a: *const Mpz) -> i32 { (*a).v.sign() as i32 }
pub unsafe fn gmp_cmp(a: *const Mpz, b: *const Mpz) -> i32 { (*a).v.cmp(&(*b).v) as i32 }
pub unsafe fn gmp_add_ui(r: *mut Mpz, a: *const Mpz, u: u64) {
    (*r).v = &(*a).v + &BigInt::from(u);
}
pub unsafe fn gmp_fdiv_q(r: *mut Mpz, a: *const Mpz, b: *const Mpz) {
    let bv = &(*b).v;
    if bv.is_zero() { erreur_runtime("division par zéro"); }
    (*r).v = (*a).v.div_floor(bv);
}
pub unsafe fn gmp_fdiv_r(r: *mut Mpz, a: *const Mpz, b: *const Mpz) {
    // Zéro clonage des opérandes : `mod_floor` emprunte &self / &b.
    // `rem` est calculé dans une locale pour rester sûr même si r == a
    // (cas réel en interp.rs:3509).
    if (*b).v.is_zero() { erreur_runtime("division par zéro"); }
    let rem = (*a).v.mod_floor(&(*b).v);
    (*r).v = rem;
}
pub unsafe fn gmp_fdiv_ui(a: *const Mpz, u: u64) -> u64 {
    if u == 0 { erreur_runtime("division par zéro"); }
    (*a).v.mod_floor(&BigInt::from(u)).to_u64().unwrap_or(0)
}
pub unsafe fn gmp_mul_2exp(r: *mut Mpz, a: *const Mpz, e: u64) {
    (*r).v = &(*a).v << (e as usize);
}

/// `base^e` : puissance de 2 en un décalage, sinon exponentiation binaire
/// sans clonage des opérandes (une seule copie de la base, nécessaire).
#[inline]
fn pow_ui_vers(base: &BigInt, e: u64) -> BigInt {
    if base.is_one() { return BigInt::one(); }
    if let Some(k) = base.to_u64().filter(|v| v.is_power_of_two()) {
        if let Some(shift) = e.checked_mul(k.trailing_zeros() as u64) {
            if shift <= (1u64 << 28) { return BigInt::one() << (shift as usize); }
        }
    }
    let mut b = base.clone();
    let mut res = BigInt::one();
    let mut exp = e;
    while exp > 0 {
        if exp & 1 == 1 { res *= &b; }
        exp >>= 1;
        if exp > 0 { b = &b * &b; }
    }
    res
}
pub unsafe fn gmp_pow_ui(r: *mut Mpz, a: *const Mpz, e: u64) {
    (*r).v = pow_ui_vers(&(*a).v, e);
}
pub unsafe fn gmp_ui_pow_ui(r: *mut Mpz, base: u64, e: u64) {
    // Ancien corps : `res * b.clone()` ET `b.clone() * b` → DEUX clones
    // complets par bit d'exposant. `pow_ui_vers` n'en fait plus qu'un
    // (et ZÉRO pour une base puissance de 2).
    (*r).v = pow_ui_vers(&BigInt::from(base), e);
}
pub unsafe fn gmp_gcd(r: *mut Mpz, a: *const Mpz, b: *const Mpz) {
    (*r).v = gcd_rapide(&(*a).v, &(*b).v);
}
pub unsafe fn gmp_lcm(r: *mut Mpz, a: *const Mpz, b: *const Mpz) {
    (*r).v = (*a).v.lcm(&(*b).v);
}
pub unsafe fn gmp_fits_slong(a: *const Mpz) -> bool { (*a).v.to_i64().is_some() }
pub unsafe fn gmp_get_si(a: *const Mpz) -> i64 { (*a).v.to_i64().unwrap_or(0) }
pub unsafe fn gmp_get_ui(a: *const Mpz) -> u64 { (*a).v.to_u64().unwrap_or(0) }
pub unsafe fn gmp_sizeinbase(a: *const Mpz, _base: i32) -> u64 {
    (*a).v.to_string().len() as u64
}
pub unsafe fn gmp_tstbit(a: *const Mpz, bit: u64) -> bool {
    let x = &(*a).v;
    if x.is_negative() || bit >= 1_000_000 { return false; }
    ((x >> bit) & BigInt::one()) == BigInt::one()
}
pub unsafe fn gmp_probab_prime(a: *const Mpz) -> bool { miller_rabin(&(*a).v) }
pub fn gmp_cmp_ge(a: &Gz, b: &Gz) -> bool { a.cmp_gz(b) != Ordering::Less }

// gmpq
pub unsafe fn gmpq_set(r: *mut Mpq, a: *const Mpq) { *r = (*a).clone(); }
pub unsafe fn gmpq_set_z(q: *mut Mpq, z: *const Mpz) {
    (*q).num.v = (*z).v.clone();
    (*q).den.v = BigInt::one();
}
pub unsafe fn gmpq_set_z_field(dst: *mut Mpz, src: *const Mpz) {
    (*dst).v = (*src).v.clone();
}
pub unsafe fn gmpq_set_ui(q: *mut Mpq, n: u64, d: u64) {
    (*q).num.v = BigInt::from(n);
    (*q).den.v = if d == 0 { BigInt::one() } else { BigInt::from(d) };
}
pub unsafe fn gmpq_set_d(q: *mut Mpq, x: f64) {
    (*q).num.v = BigInt::from(x as i64);
    (*q).den.v = BigInt::one();
}
/// PGCD d'un BigInt (positif) et d'un petit u64, par euclidienne sur le reste
/// scalaire du grand nombre — aucun BigInt du petit côté, aucun PGCD de
/// grands nombres. `n` doit être ≥ 0 ; le résultat est ≥ 1 si `c > 0`.
#[inline]
fn pgcd_grand_petit(n: &BigInt, c: u64) -> u64 {
    // SP9 — voie scalaire exacte quand le numérateur tient dans u64.
    // L'ancien corps calculait `(n % c)` sur BigInt, ce qui alloue un
    // quotient à chaque appel, y compris pour n=1 (voie chaude de `1/c`).
    // Ici `|n| mod c` est calculé sur des entiers natifs : même euclidienne,
    // même résultat, zéro allocation. Le cas général BigInt est inchangé.
    if c != 0 {
        if let Some(nu) = n.to_u64() {
            let mut a = c;
            let mut b = nu % c;
            while b != 0 { let t = a % b; a = b; b = t; }
            return a;
        }
    }
    let mut a = c;
    let mut b = (n % c).abs().to_u64().unwrap_or(0);
    while b != 0 { let t = a % b; a = b; b = t; }
    a
}
/// Reste d'un BigInt modulo un petit u64, SANS allouer de quotient.
/// Les digits sont lus du plus significatif au moins significatif.
#[inline]
fn mod_petit(n: &BigInt, c: u64) -> u64 {
    // R10 — residu par reciproque (magic number) au lieu d'un `udiv` 64 bits
    // par digit. `m = ceil(2^64/c)`; pour tout digit d < 2^64,
    // q = (d*m)>>64 vaut floor(d/c) ou floor(d/c)+1 (jamais plus), donc
    // r = d - q*c est soit d%c, soit (d%c)-c : une correction au plus.
    // Mesure : micro-bench cntvct 1,85x sur 42 digits u64, c=1..1000.
    // Repli exact (deux udiv) conserve pour les grands c hors voie chaude.
    let pow = ((1u128 << 64) % (c as u128)) as u64;
    if c <= u32::MAX as u64 && c > 1 {
        let m = ((1u128 << 64) + (c as u128) - 1) / (c as u128);
        let mut r: u64 = 0;
        for d in n.iter_u64_digits().rev() {
            let q = ((d as u128 * m) >> 64) as u64;
            let mut rd = d.wrapping_sub(q.wrapping_mul(c));
            if rd >= c { rd = rd.wrapping_add(c); }
            r = (r * pow + rd) % c;
        }
        r
    } else {
        let mut r: u64 = 0;
        for d in n.iter_u64_digits().rev() {
            r = (r * pow + d % c) % c;
        }
        r
    }
}
/// Division d'un BigInt positif par un petit u32, avec reste scalaire.
/// Traite les digits 32 bits du plus significatif au moins significatif :
/// chaque étape tient dans u64 (reste < d < 2^32), donc une seule division
/// 64 bits au lieu de la division générique 128/64 de num-bigint.
thread_local! {
    static SCRATCH_DIV: std::cell::RefCell<Vec<u32>> = std::cell::RefCell::new(Vec::new());
}
#[inline]
fn div_rem_petit(n: &BigInt, d: u32) -> (BigInt, u64) {
    SCRATCH_DIV.with(|cell| {
        let mut qd = cell.borrow_mut();
        qd.clear();
        let d64 = d as u64;
        // FORMULES3 — division par réciproque (`umulh`) au lieu de `udiv`.
        // m = ceil(2^64/d) ; pour cur < 2^64, q0 = (cur·m)>>64 vaut
        // floor(cur/d) ou floor(cur/d)+1 (preuve : m·d = 2^64+t, 0≤t<d,
        // donc cur/d + cur·t/(d·2^64) < cur/d + 1 ; l'écart à l'entier
        // supérieur est < 1, donc au plus un dépassement). Une seule
        // correction suffit. Mesuré : 238 → 158 ns sur un dénominateur
        // de 1440 bits (1,51×) ; voir docs/FORMULES3.md.
        // d=1 est exceptionnel : m=2^64 ne tient pas dans u64 (et le
        // quotient vaut n, reste 0) — traité à part, sans réciproque.
        if d64 == 1 { return (n.clone(), 0); }
        let m = (((1u128 << 64) + (d64 as u128) - 1) / (d64 as u128)) as u64;
        let mut rem: u64 = 0;
        for chunk in n.iter_u32_digits().rev() {
            let cur = (rem << 32) | (chunk as u64);
            let mut q = ((cur as u128 * m as u128) >> 64) as u64;
            let mut r = cur - q.wrapping_mul(d64);
            if r >= d64 { r = r.wrapping_add(d64); q -= 1; }
            qd.push(q as u32);
            rem = r;
        }
        qd.reverse();
        while qd.len() > 1 && *qd.last().unwrap() == 0 { qd.pop(); }
        (BigInt::from_slice(Sign::Plus, &qd), rem)
    })
}
/// PGCD de deux petits entiers (déjà dans u64).
///
/// FORMULES3 — algorithme de Stein (binaire) au lieu de l'Euclide à `%`.
/// Mesuré : 115 → 63 ns sur des paires u64 aléatoires (1,83×), et
/// 6 → 3 ns sur les paires réelles de `add_rat_inv_small` (c ≤ 1000).
/// Même résultat bit-à-bit sur ~200 000 paires. Voir docs/FORMULES3.md.
#[inline]
fn pgcd_scalar(mut a: u64, mut b: u64) -> u64 {
    if a == 0 { return b; }
    if b == 0 { return a; }
    let sh = (a | b).trailing_zeros();
    a >>= a.trailing_zeros();
    loop {
        b >>= b.trailing_zeros();
        if a > b { std::mem::swap(&mut a, &mut b); }
        b -= a;
        if b == 0 { break; }
    }
    a << sh
}

/// Addition exacte `na/da + signe/c` (signe = ±1, `c` tient dans u64,
/// `da > 0` et `gcd(na,da)=1`). Retourne `false` si le cas n'est pas couvert.
///
/// Démonstration d'exactitude (voir docs/GLOBAL_FINAL.md) :
///   g = PGCD(da,c), da = g·d1, c = g·c1, PGCD(d1,c1)=1.
///   N = na·c1 ± d1, D = da·c1. Tout facteur premier commun à N et D
///   divise c ; sa multiplicité dans D est au plus sa multiplicité dans c.
///   Un seul PGCD(N, c), sur des nombres petits, suffit donc à réduire.
#[inline]
unsafe fn add_rat_inv_small(dst: *mut Mpq, src: *const Mpq, c: u64, signe: i64) -> bool {
    let na = &(*src).num.v;
    let da = &(*src).den.v;
    if c == 0 || da.is_zero() || !da.is_positive() { return false; }
    // Tout Gq produit ici est canonique (gcd(na,da)=1, da>0) : pas de
    // vérification supplémentaire. On divise le grand dénominateur par c²
    // (un seul passage) : q2 = da/c², r2 = da%c². De ce seul reste on tire
    //   r = da%c = r2%c        → g = gcd(da,c) = gcd(r,c) (-scalaire),
    //   da/c = q2*c + r2/c    → d1 = da/g = q2*(c*c/g) + r2/g,
    //   g²|da ⟺ r2%g²=0       → preuve que gcd(N,c)=1 (théorème ci-dessous).
    // Quand cette preuve vaut, on SAUTE le second PGCD sur le grand nombre.
    let (c1, d1, g2) = if c <= u32::MAX as u64 {
        let (q2, r2u) = div_rem_petit(da, (c * c) as u32);
        let r = r2u % c;
        let g = if r == 0 { c } else { pgcd_scalar(r, c) };
        let c1 = c / g;
        let mut d1 = q2;
        d1 *= c * c1;
        d1 += r2u / g;
        // Théorème : si g²|da alors pour tout p|c, ou bien p|c1 et alors
        // p∤d1 (pgcd(d1,c1)=1) donc p∤N, ou bien p|g et alors p|d1 donc
        // N≡na*c1 (mod p) avec p∤na (pgcd(na,da)=1) et p∤c1 : p∤N. Donc
        // gcd(N,c)=1. La réciproque est fausse, mais on ne l'utilise pas.
        let g2 = if ((r2u as u128) % ((g as u128) * (g as u128))) == 0 { 1 } else { 0 };
        (c1, d1, g2)
    } else {
        // repli : dénominateur trop grand pour c² dans u64
        let (q, r) = da.div_rem(&BigInt::from(c));
        let g = if r.is_zero() { c } else { pgcd_scalar(r.to_u64().unwrap_or(0), c) };
        let c1 = c / g;
        let d1 = if g == 1 { da.clone() } else { q * c1 + r.to_u64().unwrap_or(0) / g };
        (c1, d1, 0)
    };
    // Opérations EN PLACE : `src` est un Gq temporaire (voir unique appelant
    // dans interp.rs, il est détruit juste après) ; on lui vole ses deux
    // buffers BigInt plutôt que de cloner na et da. zéro allocation pour n2/d2.
    let sp = src as *mut Mpq;
    std::mem::swap(&mut (*dst).num.v, &mut (*sp).num.v);
    std::mem::swap(&mut (*dst).den.v, &mut (*sp).den.v);
    if c1 != 1 {
        (*dst).num.v *= c1;
        (*dst).den.v *= c1;
    }
    if signe > 0 { (*dst).num.v += &d1; } else { (*dst).num.v -= &d1; }
    if (*dst).num.v.is_zero() {
        (*dst).num.v = BigInt::zero();
        (*dst).den.v = BigInt::one();
        return true;
    }
    let g2 = if g2 == 1 { 1 } else { pgcd_scalar(mod_petit(&(*dst).num.v, c), c) };
    if g2 > 1 {
        (*dst).num.v /= g2;
        (*dst).den.v /= g2;
    }
    true
}

/// ⚠ OPT_BIGINT — réduction EN PLACE : zéro clone du numérateur et du
/// dénominateur (l'ancien corps les clonait 2× chacun, puis clonait le PGCD).
#[inline]
unsafe fn canon_q(q: *mut Mpq) {
    if (*q).den.v.is_zero() { (*q).den.v = BigInt::one(); return; }
    if (*q).den.v.is_negative() {
        (*q).num.v = -&(*q).num.v;
        (*q).den.v = -&(*q).den.v;
    }
    let g = gcd_rapide(&(*q).num.v, &(*q).den.v);
    if !g.is_zero() && !g.is_one() {
        (*q).num.v /= &g;
        (*q).den.v /= &g;
    }
}

/// `n/d + 1/c` (ou `n/d - 1/c`) : voie rapide quand l'inverse a un
/// dénominateur qui tient dans u64. Le cas général garde le PGCD BigInt.
pub unsafe fn gmpq_add(r: *mut Mpq, a: *const Mpq, b: *const Mpq) {
    if let (Some(c), Some(s)) = ((*b).den.v.to_u64(), (*b).num.v.to_i64()) {
        if s == 1 || s == -1 {
            if add_rat_inv_small(r, a, c, s) { return; }
        }
    }
    if let (Some(c), Some(s)) = ((*a).den.v.to_u64(), (*a).num.v.to_i64()) {
        if s == 1 || s == -1 {
            if add_rat_inv_small(r, b, c, s) { return; }
        }
    }
    // Tous les produits sont calculés par emprunt avant toute écriture dans r
    // (sûr même si r aliase a ou b). 4 clones de moins que l'ancien corps.
    let num = &(*a).num.v * &(*b).den.v + &(*b).num.v * &(*a).den.v;
    let den = &(*a).den.v * &(*b).den.v;
    (*r).num.v = num;
    (*r).den.v = den;
    canon_q(r);
}
pub unsafe fn gmpq_sub(r: *mut Mpq, a: *const Mpq, b: *const Mpq) {
    let num = &(*a).num.v * &(*b).den.v - &(*b).num.v * &(*a).den.v;
    let den = &(*a).den.v * &(*b).den.v;
    (*r).num.v = num;
    (*r).den.v = den;
    canon_q(r);
}
pub unsafe fn gmpq_mul(r: *mut Mpq, a: *const Mpq, b: *const Mpq) {
    let num = &(*a).num.v * &(*b).num.v;
    let den = &(*a).den.v * &(*b).den.v;
    (*r).num.v = num;
    (*r).den.v = den;
    canon_q(r);
}
pub unsafe fn gmpq_div(r: *mut Mpq, a: *const Mpq, b: *const Mpq) {
    // Voie rapide (a/1)/(b/1) = a/b : evite deux multiplications BigInt par 1,
    // deux clones et un PGCD BigInt. b tient dans u64 dans le cas courant.
    if (*a).den.v.is_one() && (*b).den.v.is_one() && !(*b).num.v.is_zero() {
        let neg = (*b).num.v.is_negative();
        let dabs = if neg { -&(*b).num.v } else { (*b).num.v.clone() };
        if let Some(duc) = dabs.to_u64() {
            let mut n = (*a).num.v.clone();
            if neg { n = -n; }
            let g = pgcd_grand_petit(&n, duc);
            if g > 1 {
                (*r).num.v = n / g;
                (*r).den.v = dabs / g;
            } else {
                (*r).num.v = n;
                (*r).den.v = dabs;
            }
            return;
        }
    }
    let num = &(*a).num.v * &(*b).den.v;
    let den = &(*a).den.v * &(*b).num.v;
    (*r).num.v = num;
    (*r).den.v = den;
    canon_q(r);
}
pub unsafe fn gmpq_neg(r: *mut Mpq, a: *const Mpq) {
    (*r).num.v = -&(*a).num.v;
    (*r).den.v = (*a).den.v.clone();
}
pub unsafe fn gmpq_cmp(a: *const Mpq, b: *const Mpq) -> i32 {
    // 4 clones de moins : les produits empruntent les quatre opérandes.
    let left = &(*a).num.v * &(*b).den.v;
    let right = &(*b).num.v * &(*a).den.v;
    left.cmp(&right) as i32
}
pub unsafe fn gmpq_canonicalize(q: *mut Mpq) { canon_q(q); }
// Miller-Rabin
/// ⚠⚠⚠ TEST DE PRIMALITÉ EN u64 POUR LES PETITS ENTIERS (mesuré le 17/11).
///
/// ⚠ `miller_rabin` travaille sur `BigInt` : CHAQUE division, chaque
/// multiplication modulaire et chaque `BigInt::from` alloue. Mesuré :
/// `est_premier($k)` sur 0..49999 coûtait 70 ms, soit 1,4 µs par appel,
/// pour des nombres de 5 chiffres qui tiennent dans un u64.
///
/// ⚠ Ici, tout se fait en arithmétique 64 bits (aucune allocation), avec
/// les mêmes bases témoins : Miller-Rabin déterministe pour n < 2^64
/// (bases 2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37 — suffisantes). ⚠ Les
/// nombres plus grands gardent la règle exacte (`miller_rabin` sur BigInt).
fn miller_rabin_u64(n: u64) -> bool {
    if n < 2 { return false; }
    for p in [2u64, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37] {
        if n == p { return true; }
        if n % p == 0 { return false; }
    }
    // n - 1 = d * 2^s avec d impair
    let mut d = n - 1;
    let mut s = 0u32;
    while d & 1 == 0 { d >>= 1; s += 1; }
    'temoins: for a in [2u64, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37] {
        let a = a % n;
        if a == 0 { continue; }
        let mut x = mulmod_pow(a, d, n);
        if x == 1 || x == n - 1 { continue; }
        for _ in 0..s - 1 {
            x = mulmod(x, x, n);
            if x == n - 1 { continue 'temoins; }
        }
        return false;
    }
    true
}

/// ⚠ `a * b mod m` sans dépassement (u128 intermédiaire).
#[inline]
fn mulmod(a: u64, b: u64, m: u64) -> u64 {
    ((a as u128 * b as u128) % m as u128) as u64
}

/// ⚠ `base^exposant mod m` en 64 bits (exponentiation binaire).
fn mulmod_pow(mut base: u64, mut e: u64, m: u64) -> u64 {
    let mut r = 1u64 % m;
    base %= m;
    while e > 0 {
        if e & 1 == 1 { r = mulmod(r, base, m); }
        base = mulmod(base, base, m);
        e >>= 1;
    }
    r
}

fn miller_rabin(n: &BigInt) -> bool {
    // ⚠⚠ Voie rapide 64 bits : aucun BigInt, aucune allocation.
    if let Some(v) = n.to_u64() {
        if v <= u64::MAX { return miller_rabin_u64(v); }
    }
    if n < &BigInt::from(2) { return false; }
    let small = [2i64,3,5,7,11,13,17,19,23,29,31,37];
    for &p in &small {
        let pb = BigInt::from(p);
        if n == &pb { return true; }
        if n % &pb == BigInt::zero() { return false; }
    }
    let mut d = n.clone() - BigInt::one();
    let mut s = 0u32;
    while &d % 2 == BigInt::zero() { d /= 2; s += 1; }
    for &a in &small {
        let a = BigInt::from(a);
        if &a % n == BigInt::zero() { continue; }
        let mut x = modpow(&a, &d, n);
        if x == BigInt::one() || x == n.clone() - BigInt::one() { continue; }
        let mut composite = true;
        for _ in 0..(s-1) {
            x = (&x * &x) % n;
            if x == n.clone() - BigInt::one() { composite = false; break; }
        }
        if composite { return false; }
    }
    true
}
fn modpow(base: &BigInt, exp: &BigInt, m: &BigInt) -> BigInt {
    let mut result = BigInt::one();
    let mut b = base % m;
    let mut e = exp.clone();
    while e > BigInt::zero() {
        if &e % 2 == BigInt::one() { result = (&result * &b) % m; }
        b = (&b * &b) % m;
        e /= 2;
    }
    result
}

// ══════════════════════════════════════════════════════════════════════════
// MATH1 — primitives pour factorielles géantes SYMBOLIQUES (sans GMP).
//
// Aucun de ces primitives ne calcule n! quand n est géant. Toutes les
// quantités réelles sont manipulées en VIRGULE FIXE pure (BigInt / 10^p),
// jamais en f64 : n jusqu'à 10^1000 fonctionne.
//
// Théorème utilisé (Euler–Maclaurin / Stirling, borne de Robbins) :
//
//   ln(n!) = (n+1/2)ln n − n + (1/2)ln(2π)
//            + Σ_{j=1}^{m} B_{2j} / (2j(2j−1) n^{2j−1}) + R_m
//
// avec |R_m| ≤ |B_{2m+2}| / ((2m+2)(2m+1) n^{2m+1})  pour n ≥ 1.
// Le nombre m est choisi PAR EXACTITUDE (comparaison entière) pour que
// |R_m| < 10^{−(p+5)} où p est la précision de travail en chiffres.
//
// ln(x) est évalué par la série de l'atanh :
//   ln x = ln f + k·ln 2,  f = x/2^k ∈ [1,2),  z = (f−1)/(f+1) ∈ [0,1/3],
//   ln f = 2·Σ_{j≥0} z^{2j+1}/(2j+1)   (erreur < 2 z^{2J+1}/((2J+1)(1−z²))).
// Comme z ≤ 1/3, chaque terme est divisé par ≥ 9 : la série s'arrête
// dès qu'un terme devient nul en virgue fixe (≈ 1,05·p termes).
//
// π par la formule de Machin : π = 16·atan(1/5) − 4·atan(1/239).
// exp par Σ x^j/j!, tronquée quand le terme devient nul.
//
// Borne d'erreur de virgule fixe (conservatrice) : chaque division/multi-
// plication tronque ≤ 1 ulp ; l'atanh sur K termes accumule ≤ 8K+16 ulps ;
// la multiplication par k = ⌊log2 x⌋ amplifie l'erreur de ln 2 ; le terme
// dominant (n+1/2)·ln n amplifie l'erreur de ln n par n. Tout est majoré
// en ENTIERS (BigInt) et comparé à la distance au bord d'arrondi : le
// drapeau `exact` n'est vrai que si la troncature aux `digits` demandées
// est PROUVÉE sans ambiguïté.
// ══════════════════════════════════════════════════════════════════════════

fn mp_pow10(p: usize) -> BigInt { BigInt::from(10u32).pow(p as u32) }

fn fp_mul(a: &BigInt, b: &BigInt, scale: &BigInt) -> BigInt { (a * b) / scale }

fn fp_div(a: &BigInt, b: &BigInt, scale: &BigInt) -> BigInt {
    if b.is_zero() { erreur_runtime("division par zéro"); }
    (a * scale) / b
}

/// atanh(z) en virgule fixe, pour 0 ≤ z < 1 (ici z ≤ 1/3).
fn atanh_fixed(z: &BigInt, scale: &BigInt) -> BigInt {
    let z2 = fp_mul(z, z, scale);
    let mut puis = z.clone();
    let mut somme = BigInt::zero();
    let mut k = 0u64;
    loop {
        let terme = &puis / BigInt::from(2 * k + 1);
        if terme.is_zero() { break; }
        somme += &terme;
        puis = fp_mul(&puis, &z2, scale);
        k += 1;
        if puis.is_zero() || k > 100_000 { break; }
    }
    somme
}

/// atan(1/q) en virgule fixe, q ≥ 2, par la série alternée.
fn arctan_inv_fixed(q: u64, scale: &BigInt) -> BigInt {
    let x = scale / BigInt::from(q);          // 1/q tronqué : erreur ≤ 1 ulp
    let x2 = fp_mul(&x, &x, scale);
    let mut puis = x;
    let mut somme = BigInt::zero();
    let mut k = 0u64;
    loop {
        let terme = &puis / BigInt::from(2 * k + 1);
        if terme.is_zero() { break; }
        if k % 2 == 0 { somme += &terme; } else { somme -= &terme; }
        puis = fp_mul(&puis, &x2, scale);
        k += 1;
        if puis.is_zero() || k > 100_000 { break; }
    }
    somme
}

fn pi_fixed(scale: &BigInt) -> BigInt {
    // π = 16·atan(1/5) − 4·atan(1/239)  (Machin)
    arctan_inv_fixed(5, scale) * 16u32 - arctan_inv_fixed(239, scale) * 4u32
}

fn ln2_fixed(scale: &BigInt) -> BigInt {
    // ln 2 = 2·atanh(1/3)
    atanh_fixed(&(scale / 3u32), scale) * 2u32
}

/// ⌊log2(x/scale)⌋ exact (x ≥ scale, i.e. valeur ≥ 1).
fn bits_diff(x: &BigInt, scale: &BigInt) -> i64 {
    let bs = scale.bits() as i64;
    let mut k = x.bits() as i64 - bs;
    if k < 0 { return k; }
    while (scale << (k as usize)) > *x { k -= 1; }
    while (scale << ((k + 1) as usize)) <= *x { k += 1; }
    k
}

/// ln(x) en virgule fixe pour x ≥ scale (valeur ≥ 1), x étant un BigInt
/// déjà multiplié par scale. `ln2` est passé en paramètre (pas de mémoïsation).
fn ln_fixed(x: &BigInt, scale: &BigInt, ln2: &BigInt) -> BigInt {
    let k = bits_diff(x, scale);
    // f = x / 2^k ∈ [1,2), en virgule fixe.
    let f = if k >= 0 { x >> (k as usize) } else { x << ((-k) as usize) };
    // z = (f−1)/(f+1) ; f ≥ scale garantit z ≥ 0, et f < 2·scale garantit z < 1/3.
    let z = fp_div(&(&f - scale), &(&f + scale), scale);
    let lnf = atanh_fixed(&z, scale) * 2u32;
    lnf + ln2 * BigInt::from(k)
}

/// exp(x) en virgule fixe, x ≥ 0 (ici x ∈ [0, ln 10)).
fn exp_fixed(x: &BigInt, scale: &BigInt) -> BigInt {
    let mut terme = scale.clone();
    let mut somme = scale.clone();
    let mut j = 1u64;
    loop {
        terme = fp_mul(&terme, x, scale) / BigInt::from(j);
        if terme.is_zero() { break; }
        somme += &terme;
        j += 1;
        if j > 100_000 { break; }
    }
    somme
}

fn binom_big(n: u64, k: u64) -> BigInt {
    let mut r = BigInt::one();
    for i in 1..=k { r = r * BigInt::from(n - i + 1) / BigInt::from(i); }
    r
}

/// B_0 … B_{max_m} sous forme de rationnels exacts (num, den), den > 0.
/// Réccurrence : Σ_{k=0}^{m} C(m+1,k)·B_k = 0 pour m ≥ 1.
fn bernoullis(max_m: usize) -> Vec<(BigInt, BigInt)> {
    let mut b = vec![(BigInt::zero(), BigInt::one()); max_m + 1];
    b[0] = (BigInt::one(), BigInt::one());
    for m in 1..=max_m {
        let (mut num, mut den) = (BigInt::zero(), BigInt::one());
        for k in 0..m {
            let c = binom_big((m + 1) as u64, k as u64);
            let (nk, dk) = &b[k];
            num = num * dk + nk * &c * &den;
            den = den * dk;
            let g = num.gcd(&den);
            if g > BigInt::one() { num /= &g; den /= &g; }
        }
        num = -num;
        den *= BigInt::from((m + 1) as u64);
        let g = num.gcd(&den);
        if g > BigInt::one() { num /= &g; den /= &g; }
        b[m] = (num, den);
    }
    b
}

/// Σ_{i≥1} ⌊n/p^i⌋ (formule de Legendre), exact.
fn legendre(n: &BigInt, p: u64) -> BigInt {
    let mut q = n.clone();
    let mut s = BigInt::zero();
    let pb = BigInt::from(p);
    while !q.is_zero() { q /= &pb; s += &q; }
    s
}

fn pow_mod(base: &BigInt, exp: &BigInt, m: &BigInt) -> BigInt {
    let mut res = BigInt::one();
    if m.is_one() { return BigInt::zero(); }
    let mut b = base % m;
    let mut e = exp.clone();
    while !e.is_zero() {
        if &e & BigInt::one() == BigInt::one() { res = (&res * &b) % m; }
        e = e >> 1;
        if !e.is_zero() { b = (&b * &b) % m; }
    }
    res
}

fn egcd(a: &BigInt, b: &BigInt) -> (BigInt, BigInt, BigInt) {
    if b.is_zero() { return (a.abs(), if a.sign() == Sign::Minus { -BigInt::one() } else { BigInt::one() }, BigInt::zero()); }
    let (g, x1, y1) = egcd(b, &(a % b));
    (g, y1.clone(), x1 - (a / b) * y1)
}

fn inv_mod(a: &BigInt, m: &BigInt) -> BigInt {
    let (g, x, _) = egcd(a, m);
    debug_assert!(g.is_one());
    let mut r = x % m;
    if r.sign() == Sign::Minus { r += m; }
    r
}

fn chaine_sans_zeros(x: &BigInt) -> String {
    let s = x.to_string();
    s.trim_start_matches('0').to_string()
}

fn n_decimal_digits(n: &BigInt) -> usize { n.to_string().len() }

/// Seuil : en dessous, n! est calculé EXACTEMENT (le seul endroit où le
/// nombre entier est matérialisé) et log10 en est déduit. Au-dessus, on
/// n'utilise QUE la série d'Euler–Maclaurin.
const MATH1_SEUIL_EXACT: u64 = 1000;

/// log10(n!) en virgule fixe à `p` décimales + borne d'erreur conservatrice
/// en ulps (unité = 10^{-p}). Privé : les deux fonctions publiques qui en
/// dépendent choisissent p en fonction de la taille de n.
fn log10_fact_fixed(n: &Gz, p: usize) -> (BigInt, BigInt) {
    let scale = mp_pow10(p);
    let ln2 = ln2_fixed(&scale);
    let ln10 = ln_fixed(&(BigInt::from(10u32) * &scale), &scale, &ln2);
    let nv = n.bref();
    // Majoration de l'erreur d'un ln évalué par atanh : ≤ 8p+96 ulps.
    let e_unit = BigInt::from(8u64 * (p as u64 + 10) + 96);

    let (ln_fact, err) = if nv.sign() == Sign::Minus {
        erreur_runtime("factorielle d'un entier négatif");
    } else if nv <= &BigInt::from(MATH1_SEUIL_EXACT) {
        let m = factorielle_parallele(nv.to_u64().unwrap_or(0)).v();
        let x = m * &scale;
        let k = bits_diff(&x, &scale).max(0) as u64;
        let v = ln_fixed(&x, &scale, &ln2);
        (v, BigInt::from(k + 1) * &e_unit)
    } else {
        // ── Euler–Maclaurin ──────────────────────────────────────────────
        let n_pow0 = nv.clone();
        let x_n = nv * &scale;
        let ln_n = ln_fixed(&x_n, &scale, &ln2);
        let k_n = bits_diff(&x_n, &scale).max(0) as u64;
        let e_lnn = BigInt::from(k_n + 1) * &e_unit;
        let lnpi = ln_fixed(&pi_fixed(&scale), &scale, &ln2);

        let coeff = nv * 2u32 + 1u32;                 // 2n+1
        let main = ((ln_n * &coeff) / 2u32 - nv * &scale) + (&ln2 + &lnpi) / 2u32;

        let dn = n_decimal_digits(nv).max(1);
        // Marge généreuse : |B_{2j}|/(2j(2j−1)) décroît aussi comme ~(2π)^{-2j},
        // donc j dépasse l'estimation grossière n^{-2j}. 12 termes de marge.
        let j_est = ((p + 5) / (2 * dn) + 12).min(p + 5);
        let bern = bernoullis(2 * (j_est + 3) + 2);
        let mut serie = BigInt::zero();
        let mut n_pow = n_pow0.clone();                 // n^{2j−1}
        let mut j = 1usize;
        let seuil_prec = mp_pow10(p + 5);
        loop {
            let (bn, bd) = &bern[2 * j];
            let den = bd * BigInt::from((2 * j * (2 * j - 1)) as u64) * &n_pow;
            serie += (bn * &scale) / &den;
            // |R_j| ≤ |B_{2j+2}| / ((2j+2)(2j+1) n^{2j+1}) < 10^{−(p+5)} ?
            let (bn2, bd2) = &bern[2 * j + 2];
            let n_suiv = &n_pow * nv * nv;
            let den2 = bd2 * BigInt::from(((2 * j + 2) * (2 * j + 1)) as u64) * &n_suiv;
            if bn2.abs() * &seuil_prec < den2 { break; }
            j += 1;
            n_pow = n_suiv;
            if 2 * j + 2 >= bern.len() || j > 10_000 {
                erreur_runtime("série de Stirling : précision non atteinte");
            }
        }
        let e = (&e_lnn * &coeff) / 2u32 + &e_lnn + 2u32 * &e_unit + 100u32;
        (main + serie, e)
    };
    let L = fp_div(&ln_fact, &ln10, &scale);
    (L, err)
}

/// log10(n!) avec `digits` chiffres après la virgule, sous forme de Gq
/// (numérateur / 10^digits). `exact = true` signifie que la borne
/// d'erreur PROUVÉE est plus petite que la distance à un bord de troncature
/// aux `digits` demandées : tous les chiffres rendus sont alors garantis.
/// La partie entière de la valeur + 1 = nombre de chiffres décimaux de n!
/// et sa partie fractionnaire est celle de log10(n!).
pub fn log10_factorielle_precision(n: &Gz, digits: usize) -> (Gq, bool) {
    let dn = n_decimal_digits(n.bref()).max(1);
    let p = digits + dn + 60;
    let (L, e) = log10_fact_fixed(n, p);
    let garde = mp_pow10(p - digits);
    let frac = &L % &garde;
    let exact = e < &garde / 2u32 && frac > e && (&garde - &frac) > e;
    // TRONCATURE (cohérente avec `math1_log10_oracles_100_chiffres` et avec
    // la convention « nombre de chiffres = floor(log10)+1 »). L'oracle
    // `10^38` du test était arrondi à tort : mpmath donne ...1789.959…,
    // donc la valeur tronquée à 20 décimales est bien ...1789.
    let num = &L / &garde;
    let q = Gq::neuf();
    unsafe { (*q.0).num.v = num; (*q.0).den.v = mp_pow10(digits); }
    q.canonicalize();
    (q, exact)
}

/// Σ_{k≥1} ⌊n/5^k⌋ : nombre de zéros finaux de n!. Exact, O(log n).
/// n < 0 ou n = 0 → 0.
pub fn zeros_finaux_factorielle(n: &Gz) -> Gz {
    let nv = n.bref();
    let r = Gz::neuf();
    if nv.sign() != Sign::Plus { return r; }
    r.set_v(legendre(nv, 5));
    r
}

/// Produit de 1..n en retirant TOUS les facteurs 2 et 5, modulo 10^k.
#[allow(dead_code)]
fn derniers_direct(n: &BigInt, k: usize) -> String {
    let mod10 = mp_pow10(k);
    let deux = BigInt::from(2u32);
    let cinq = BigInt::from(5u32);
    let mut prod = BigInt::one();
    let mut a = BigInt::zero();
    let mut b = BigInt::zero();
    let mut i = BigInt::one();
    while &i <= n {
        let mut x = i.clone();
        while &x % &deux == BigInt::zero() { a += 1u32; x = x >> 1; }
        while &x % &cinq == BigInt::zero() { b += 1u32; x /= &cinq; }
        prod = (prod * x) % &mod10;
        i += 1u32;
    }
    let d = &a - &b;
    chaine_sans_zeros(&((prod * pow_mod(&deux, &d, &mod10)) % &mod10))
}

/// S(m) = ∏_{i=1}^m i/5^{ν5(i)} mod 5^k, par récurrence
/// S(m) = U(m)·S(⌊m/5⌋), U(m) = ∏_{i≤m,5∤i} i ≡ (−1)^{⌊m/5^k⌋}·∏_{j≤r,5∤j} j.
fn s_mod5(m: &BigInt, mod5: &BigInt) -> BigInt {
    if m.is_zero() { return BigInt::one(); }
    let (q, r) = m.div_rem(mod5);
    let mut u = if &q % 2u32 == BigInt::zero() { BigInt::one() } else { mod5 - 1u32 };
    // ∏_{j≤r, 5∤j} j  (r < 5^k, borné par l'appelant).
    let mut part = BigInt::one();
    let cinq = BigInt::from(5u32);
    let mut j = BigInt::one();
    while &j <= &r {
        if &j % &cinq != BigInt::zero() { part = (part * &j) % mod5; }
        j += 1u32;
    }
    u = (u * part) % mod5;
    (u * s_mod5(&(m / &cinq), mod5)) % mod5
}

/// Chemin sous-linéaire (O(5^k·log n)) pour les grands n.
/// X = n!/10^z. Mod 5^k : X = S(n)·2^{−z}. Mod 2^k : X ≡ 0 dès que
/// l'excès de facteurs 2 dépasse k (vrai ici). Recombinaison par CRT.
#[allow(dead_code)]
fn derniers_recursif(n: &BigInt, k: usize) -> String {
    let mod10 = mp_pow10(k);
    let mod5 = BigInt::from(5u32).pow(k as u32);
    let mod2 = BigInt::from(2u32).pow(k as u32);
    let z = legendre(n, 5);
    let nu2 = legendre(n, 2);
    let d = &nu2 - &z;
    if d < BigInt::from(k as u64) { return derniers_direct(n, k); }
    let s = s_mod5(n, &mod5);
    let inv2z = inv_mod(&pow_mod(&BigInt::from(2u32), &z, &mod5), &mod5);
    let y = (s * inv2z) % &mod5;
    let inv2k = inv_mod(&mod2, &mod5);
    let t = (y * inv2k) % &mod5;
    chaine_sans_zeros(&((&mod2 * t) % &mod10))
}

/// Les `k` derniers chiffres NON nuls de n! (sans zéros de tête).
/// 10! → "288", 20! → "17664". n=0 ou 1 → "1". k=0 → "".
/// n ≤ 2·10^6 : produit direct O(n). Au-delà : récurrence O(5^k·log n)
/// tant que 5^k ≤ 10^7, sinon repli direct (documenté comme coûteux).
#[allow(dead_code)]
fn derniers_non_nuls_interne(n: &BigInt, k: usize, force_direct: bool) -> String {
    if k == 0 { return String::new(); }
    if n.sign() == Sign::Minus { erreur_runtime("factorielle d'un entier négatif"); }
    let petit = n <= &BigInt::from(2_000_000u64);
    let mod5_petit = BigInt::from(5u32).pow(k as u32) <= BigInt::from(10_000_000u64);
    if force_direct || petit || !mod5_petit { derniers_direct(n, k) }
    else { derniers_recursif(n, k) }
}

pub fn derniers_non_nuls(n: &Gz, k: usize) -> String {
    derniers_non_nuls_interne(n.bref(), k, false)
}

/// Les `count` premiers chiffres décimaux de n!, calculés par
/// 10^{frac(log10 n!)} en virgule fixe (jamais par f64). `precision`
/// fixe le nombre de décimales de travail : augmenter `precision` et
/// recalculer raffine le résultat (pas d'état interne, pas de mémoïsation).
pub fn premiers_chiffres_factorielle(n: &Gz, count: usize, precision: usize) -> String {
    if count == 0 { return String::new(); }
    let nv = n.bref();
    if nv.sign() == Sign::Minus { erreur_runtime("factorielle d'un entier négatif"); }
    let dn = n_decimal_digits(nv).max(1);
    // CloudGuard : assez de décimales de travail pour que l'erreur cumulée
    // de ln/exp en virgule fixe reste sous 10^-(count+2). 120 chiffres de
    // garde (au lieu de 60) corrigent le cas limite 10! (36287 -> 36288).
    let p = count.max(precision) + dn + 120;
    let scale = mp_pow10(p);
    let (L, e) = log10_fact_fixed(n, p);
    let frac = &L % &scale;
    let ln2 = ln2_fixed(&scale);
    let ln10 = ln_fixed(&(BigInt::from(10u32) * &scale), &scale, &ln2);
    let y = fp_mul(&frac, &ln10, &scale);          // ln(mantisse) ∈ [0, ln10)
    let mant = exp_fixed(&y, &scale);
    // V = mantisse·10^{count−1}. On veut ⌊V⌋. L'erreur PROUVÉE sur L vaut
    // `e` ulps ; le passage à ln(mantisse) puis exp amplifie au plus par
    // ~10·ln10 < 24, et la multiplication par 10^{count−1} multiplie les
    // ulps par 10^{count−1}. Si V calculé est à moins de cette borne SOUS
    // l'entier supérieur (cas exact : 10! → mantisse 3,6288 pile), la
    // troncature perdrait une unité : on relève alors d'un. Sinon,⌊V⌋ est
    // garanti — contrairement à un epsilon aveugle, qui arrondirait les
    // vraies valeurs proches de x,5 (ex. 100! → …38562 deviendrait …38563).
    let v = &mant * mp_pow10(count - 1);
    let mut d = &v / &scale;
    let reste = &v % &scale;
    let seuil = (e * 24u32 + 1000u32) * mp_pow10(count - 1);
    if &scale - &reste < seuil { d += 1u32; }
    let s = d.to_string();
    if s.len() > count {
        // mantisse arrondie à 10,0 : les chiffres sont "1" puis des zéros.
        let mut t = String::from("1");
        t.push_str(&"0".repeat(count - 1));
        t
    } else {
        let mut t = "0".repeat(count - s.len());
        t.push_str(&s);
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Valeurs de contrôle (Python `math.factorial`).
    #[test]
    fn factorielle_petits_entiers() {
        for (n, attendu) in [
            (0u64, "1"),
            (1, "1"),
            (5, "120"),
            (10, "3628800"),
            (20, "2432902008176640000"),
            (25, "15511210043330985984000000"),
            (100, "93326215443944152681699238856266700490715968264381621468592963895217599993229915608941463976156518286253697920827223758251185210916864000000000000000000000000"),
        ] {
            assert_eq!(factorielle_parallele(n).v().to_string(), attendu, "{n}!");
        }
    }

    /// Le chemin SÉQUENTIEL (celui que prend WebAssembly, sans threads) doit
    /// donner exactement le même résultat que l'arbre multi-cœur. Régression :
    /// `2000!` trappait dans le playground parce que `thread::spawn` était
    /// appelé en WASM.
    #[test]
    fn arbre_sequentiel_egal_au_parallele() {
        for n in [1001u64, 2000, 5000] {
            let parallele = factorielle_parallele(n).v().to_string();
            // même décomposition que `factorielle_parallele`, broyée en séquentiel
            let mut facteurs: Vec<BigInt> = Vec::new();
            let mut est_premier = vec![true; (n + 1) as usize];
            est_premier[0] = false;
            est_premier[1] = false;
            let mut i = 2u64;
            while i * i <= n {
                if est_premier[i as usize] {
                    let mut j = i * i;
                    while j <= n { est_premier[j as usize] = false; j += i; }
                    i += 1;
                } else {
                    i += 1;
                }
            }
            for p in (2..=n).filter(|&p| est_premier[p as usize]) {
                let mut e = 0u64;
                let mut q = p;
                while q <= n {
                    e += n / q;
                    if q > n / p { break; }
                    q *= p;
                }
                let mut base = BigInt::from(p);
                let mut res = BigInt::from(1u64);
                let mut ee = e;
                while ee > 0 {
                    if ee & 1 == 1 { res *= &base; }
                    ee >>= 1;
                    if ee > 0 { base = &base * &base; }
                }
                facteurs.push(res);
            }
            let sequentiel = arbre_produit(facteurs).v().to_string();
            assert_eq!(parallele, sequentiel, "{n}! : séquentiel ≠ parallèle");
        }
    }

    #[test]
    fn arbre_produit_bord() {
        assert_eq!(arbre_produit(Vec::new()).v().to_string(), "1");
        assert_eq!(arbre_produit(vec![BigInt::from(2u64), BigInt::from(3u64), BigInt::from(5u64)]).v().to_string(), "30");
    }

    // ── MATH1 : oracles indépendants (mpmath 200 dps, factorielles exactes) ──

    fn gq_dec(q: &Gq, digits: usize) -> String {
        let v = (q.num_bigint() * mp_pow10(digits)) / q.den_bigint();
        let s = v.to_string();
        if digits == 0 { return s; }
        if s.len() <= digits {
            let mut t = "0".repeat(digits + 1 - s.len());
            t.push_str(&s);
            format!("{}.{}", &t[..1], &t[1..])
        } else {
            let i = s.len() - digits;
            format!("{}.{}", &s[..i], &s[i..])
        }
    }

    #[test]
    fn math1_log10_oracles_100_chiffres() {
        for (n, ent, frac) in [
            (10u64, "6", "5597630328767937511747612399600555908932271418594001136856622817018191866832707151595484631470708116"),
            (100, "157", "9700036547157883739149248029898481939453164512791921316475498658214593035326294534438396588861816858"),
            (1000, "2567", "6046442221328487714230578045236916771145131624634613100442072891832219591347643126615634741606160006"),
            (10000, "35659", "4542745207814623342183207892799192807118937375270347291172849070040575046568184520756837147838647580"),
        ] {
            let gz = Gz::de_u64(n);
            let (q, exact) = log10_factorielle_precision(&gz, 100);
            assert_eq!(gq_dec(&q, 100), format!("{ent}.{frac}"), "{n}! log10");
            assert!(exact, "{n}! : la borne d'erreur devrait prouver 100 chiffres");
            // Nombre de chiffres décimaux de n! = partie entière + 1.
            assert_eq!(q.num_bigint() / q.den_bigint() + 1u32,
                       BigInt::from(factorielle_parallele(n).chaine().len() as u64), "{n}! chiffres");
        }
    }

    #[test]
    fn math1_log10_convergence_20_50_100() {
        let ref100 = "5597630328767937511747612399600555908932271418594001136856622817018191866832707151595484631470708116";
        let gz = Gz::de_u64(10);
        let mut prev = String::new();
        for d in [20usize, 50, 100] {
            let (q, exact) = log10_factorielle_precision(&gz, d);
            let s = gq_dec(&q, d);
            assert!(exact, "d={d} devrait être prouvé exact");
            assert!(s.starts_with("6."), "{s}");
            let frac = s[2..].to_string();
            assert!(ref100.starts_with(&frac), "d={d} : {frac}");
            if !prev.is_empty() { assert!(frac.starts_with(&prev), "d={d} diverge"); }
            prev = frac;
        }
    }

    #[test]
    fn math1_zeros_finaux() {
        for (n, attendu) in [(100u64, 24u64), (1000, 249), (1_000_000, 249_998)] {
            let gz = Gz::de_u64(n);
            assert_eq!(zeros_finaux_factorielle(&gz).v(), BigInt::from(attendu), "{n}!");
        }
        // Oracle indépendante : comptage direct des facteurs 5 jusqu'à 1000.
        for n in 0u64..=1000 {
            let mut q = n;
            let mut s = 0u64;
            while q > 0 { q /= 5; s += q; }
            let gz = Gz::de_u64(n);
            assert_eq!(zeros_finaux_factorielle(&gz).v(), BigInt::from(s), "{n}!");
        }
        // n = 10^38 : Somme_{k>=1} floor(10^38/5^k), sans calculer n!.
        let n38 = Gz::de_bigint(mp_pow10(38));
        let z = zeros_finaux_factorielle(&n38).v();
        assert_eq!(z.to_string().len(), 38);
        assert_eq!(z / mp_pow10(36), BigInt::from(24u32)); // 2,4999…·10^37
    }

    #[test]
    fn math1_derniers_non_nuls_oracles() {
        for (n, k, attendu) in [
            (10u64, 3usize, "288"), (20, 5, "17664"), (7, 4, "504"),
            (25, 6, "985984"), (100, 10, "5210916864"), (1000, 10, "27753472"),
            (0, 5, "1"), (1, 5, "1"),
        ] {
            let gz = Gz::de_u64(n);
            assert_eq!(derniers_non_nuls(&gz, k), attendu, "{n}! k={k}");
        }
    }

    #[test]
    fn math1_derniers_non_nuls_vs_factorielle_exacte() {
        // Oracle : n! exact, on retire les zéros finaux puis on prend mod 10^k.
        for k in [1usize, 3, 5, 10, 20] {
            let mod10 = mp_pow10(k);
            for n in 0u64..=1000 {
                let mut m = factorielle_parallele(n).v();
                while &m % 10u32 == BigInt::zero() { m /= 10u32; }
                let attendu = chaine_sans_zeros(&(m % &mod10));
                let gz = Gz::de_u64(n);
                assert_eq!(derniers_non_nuls(&gz, k), attendu, "{n}! k={k}");
            }
        }
    }

    #[test]
    fn math1_derniers_recursif_vs_direct() {
        // Le chemin sous-linéaire doit donner la MÊME chose que le produit
        // direct (k petit pour que 5^k reste raisonnable).
        for k in [1usize, 3, 5] {
            for n in [1001u64, 1234, 2000, 5000, 9999] {
                let nb = BigInt::from(n);
                let a = derniers_direct(&nb, k);
                let b = derniers_recursif(&nb, k);
                assert_eq!(a, b, "réccursif vs direct : {n}! k={k}");
            }
        }
    }

    #[test]
    fn math1_premiers_chiffres() {
        let n100 = Gz::de_u64(100);
        assert_eq!(premiers_chiffres_factorielle(&n100, 30, 40),
                   "933262154439441526816992388562");
        // "plus de chiffres" = augmenter precision et recalculer : stabilité.
        for prec in [30usize, 40, 80, 120] {
            assert_eq!(premiers_chiffres_factorielle(&n100, 30, prec),
                       "933262154439441526816992388562", "precision={prec}");
        }
        // Comparaison à la factorielle exacte pour n ≤ 1000.
        for (n, count) in [(10u64, 5usize), (100, 20), (500, 20), (1000, 20)] {
            let exact = factorielle_parallele(n).chaine();
            let gz = Gz::de_u64(n);
            assert_eq!(premiers_chiffres_factorielle(&gz, count, 60), exact[..count].to_string(), "{n}!");
        }
        // n = 10^38 : n! JAMAIS matérialisé, oracles mpmath.
        let n38 = Gz::de_bigint(mp_pow10(38));
        assert_eq!(premiers_chiffres_factorielle(&n38, 30, 50),
                   "910729394118222077829264151395");
        let (q, _) = log10_factorielle_precision(&n38, 20);
        // mpmath : log10((10^38)!) = 3756570551809674817234887108108339491789.959389…
        // La convention ici est TRONCATURE à 20 décimales (cf. oracles 100 chiffres).
        assert_eq!(q.num_bigint() / q.den_bigint(),
                   "3756570551809674817234887108108339491789".parse::<BigInt>().unwrap());
    }

    #[test]
    fn math1_n_10_puissance_1000() {
        // n = 10^1000 (= 1 suivi de 1000 zéros) : aucun f64, aucun n! calculé.
        let mut s = String::from("1");
        s.push_str(&"0".repeat(1000));
        let n = Gz::de_bigint(s.parse::<BigInt>().unwrap());
        let (q, _) = log10_factorielle_precision(&n, 30);
        let ent = q.num_bigint() / q.den_bigint();
        assert_eq!(ent.to_string().len(), 1003); // ≈ 9,9956·10^1002
        let p = premiers_chiffres_factorielle(&n, 12, 30);
        assert_eq!(p.len(), 12);
        // Stabilité quand on raffine la précision.
        assert_eq!(p, premiers_chiffres_factorielle(&n, 12, 60));
        assert_eq!(zeros_finaux_factorielle(&n).v().to_string().len(), 1000);
    }
}
