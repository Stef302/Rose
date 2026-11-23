//! R1 — primitive `fib_mod(n,m)` et détecteurs de récurrence fibonaccienne.
//!
//! Verrouille :
//!   1. l'exactitude de `fib_mod` (n = 0,1,2,10,28,50,100 ; plusieurs modules,
//!      grand n hors u64, grand module hors u64) et ses GARDES (n >= 0, m > 0) ;
//!   2. la réécriture de la récurrence `si n < 2 { retour n } ; retour
//!      f(n-1)+f(n-2)` vers la primitive exacte (mêmes valeurs que la suite) ;
//!   3. la réécriture de la boucle `a,b ← b,(a+b)%m` vers `fib_mod` ;
//!   4. le REPLI EXACT quand une garde de forme échoue (récurrence ou boucle
//!      approchante) : le programme doit garder SA sémantique, pas devenir
//!      Fibonacci ;
//!   5. la parité JIT ↔ interpréteur sur tous ces programmes.
//!
//! Aucun seuil de temps : les durées sont des constats datés, pas des
//! contrats de test (même convention que `rose/tests/opt_jit.rs`).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const DIR: &str = "/tmp/opencode/opt_fibmod";

fn binaire() -> &'static str {
    env!("CARGO_BIN_EXE_rose")
}

fn ecrire(nom: &str, source: &str) -> PathBuf {
    std::fs::create_dir_all(DIR).expect("création du dossier de test");
    let p = Path::new(DIR).join(format!("{nom}.rose"));
    std::fs::write(&p, source).expect("écriture du programme de test");
    p
}

fn lancer(prog: &Path, sans_jit: bool) -> Output {
    let mut cmd = Command::new(binaire());
    cmd.arg(prog);
    if sans_jit { cmd.env("ROSE_SANS_JIT", "1"); }
    cmd.output().expect("lancement de Rose")
}

fn sortie(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn resultat(out: &Output) -> String {
    sortie(out).lines().next().unwrap_or("").trim().to_string()
}

/// Référence INDÉPENDANTE (itérative, O(n)) — pas de doublement, pour ne pas
/// répliquer l'implémentation sous test.
fn fib_ref(n: u32) -> u128 {
    let (mut a, mut b) = (0u128, 1u128);
    for _ in 0..n { let t = a + b; a = b; b = t; }
    a
}

fn fib_mod_ref(n: u32, m: u128) -> u128 {
    if m == 1 { return 0; }
    let (mut a, mut b) = (0u128, 1u128 % m);
    for _ in 0..n { let t = (a + b) % m; a = b; b = t; }
    a
}

fn verifier(nom: &str, source: &str, attendu: &str) {
    let prog = ecrire(nom, source);
    for sans_jit in [false, true] {
        let out = lancer(&prog, sans_jit);
        assert!(out.status.success(), "{nom} (sans_jit={sans_jit}) a échoué : {} {}",
                sortie(&out), String::from_utf8_lossy(&out.stderr));
        assert_eq!(resultat(&out), attendu,
                   "{nom} (sans_jit={sans_jit}) : attendu {attendu}, obtenu {}", resultat(&out));
    }
    assert_eq!(sortie(&lancer(&prog, false)), sortie(&lancer(&prog, true)),
               "divergence JIT ↔ interpréteur sur {nom}");
}

#[test]
fn fib_mod_valeurs_exactes() {
    for n in [0u32, 1, 2, 10, 28, 50, 100] {
        for m in [2u128, 7, 97, 1_000_000_007] {
            let src = format!(">> chaine(fib_mod({n}, {m}))\n");
            verifier(&format!("fm_{n}_{m}"), &src, &(fib_ref(n) % m).to_string());
        }
    }
}

#[test]
fn fib_mod_grand_n_et_grand_module() {
    // n = 2^64 (> u64) : la période de Pisano modulo 7 est 16 et 2^64 ≡ 0
    // mod 16, donc F_n ≡ F_0 ≡ 0. Vérifie la voie BigInt du doublement.
    verifier("fm_gros_n", ">> chaine(fib_mod(18446744073709551616, 7))\n", "0");
    // module > F_100 : le résultat doit être F_100 entier.
    verifier("fm_gros_m",
             ">> chaine(fib_mod(100, 1000000000000000000000000007))\n",
             "354224848179261915075");
    // Le banc : 10^6 tours en O(n) → même résultat qu'un seul appel O(log n).
    verifier("fm_banc", ">> chaine(fib_mod(1000000, 1000000007))\n", "918091266");
}

#[test]
fn recurrence_fib_detectee_et_exacte() {
    let src = "fn fib(n: entier) -> entier {\n\
               \x20   si (n < 2) { retour n }\n\
               \x20   retour fib(n - 1) + fib(n - 2)\n\
               }\n";
    for n in [0u32, 1, 2, 10, 28, 35, 50, 100] {
        verifier(&format!("rec_{n}"), &format!("{src}>> chaine(fib({n}))\n"),
                 &fib_ref(n).to_string());
    }
}

#[test]
fn boucle_fib_mod_detectee_et_exacte() {
    for (nom, n, m) in [("bl_1", 50u32, 7u128), ("bl_2", 500, 97),
                        ("bl_3", 1000, 1_000_000_007),
                        ("bl_4", 1_000_000, 1_000_000_007)] {
        let src = format!(
            "$a = 0\n$b = 1\npour i de 1 vers {} {{\n\
             $t = ($a + $b) % {}\n$a = $b\n$b = $t\n}}\n>> chaine($a)\n",
            n + 1, m);
        verifier(nom, &src, &fib_mod_ref(n, m).to_string());
    }
}

#[test]
fn repli_exact_si_le_motif_ne_correspond_pas() {
    // Récurrence approchante (n < 3 au lieu de 2) : la suite n'est PAS F_n.
    let src = "fn fib(n: entier) -> entier {\n\
               \x20   si (n < 3) { retour n }\n\
               \x20   retour fib(n - 1) + fib(n - 2)\n\
               }\n>> chaine(fib(12))\n";
    fn weird(n: u32) -> u128 { if n < 3 { return n as u128; } weird(n - 1) + weird(n - 2) }
    verifier("repli_rec", src, &weird(12).to_string());

    // Boucle approchante (+1 dans le modulo) : la suite n'est PAS F_n mod m.
    let src2 = "$a = 0\n$b = 1\npour i de 1 vers 101 {\n\
                $t = ($a + $b + 1) % 97\n$a = $b\n$b = $t\n}\n>> chaine($a)\n";
    let (mut a, mut b) = (0u128, 1u128);
    for _ in 0..100 { let t = (a + b + 1) % 97; a = b; b = t; }
    verifier("repli_boucle", src2, &a.to_string());
}

#[test]
fn gardes_n_negatif_et_module_nul() {
    for (nom, src) in [
        ("garde_n", ">> chaine(fib_mod(-1, 7))\n"),
        ("garde_m", ">> chaine(fib_mod(10, 0))\n"),
        ("garde_m_neg", ">> chaine(fib_mod(10, -7))\n"),
    ] {
        let prog = ecrire(nom, src);
        let out = lancer(&prog, false);
        assert!(!out.status.success(), "{nom} aurait dû échouer");
        let msg = String::from_utf8_lossy(&out.stderr);
        assert!(msg.contains("fib_mod"), "{nom} : message inattendu {msg}");
    }
}
