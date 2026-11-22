//! MEM_HOST — non-régression de `ChaineRep::tranche` (chemin hébergé).
//!
//! `sous_chaine` n'aplatit plus la rope entière ; ces tests vérifient que la
//! tranche rend EXACTEMENT les mêmes points de code que l'ancien chemin
//! `aplatir().chars().collect()`, y compris sur des feuilles ASCII,
//! non-ASCII indexées (table `caractère → octet`) et non indexées (courtes),
//! et sur une rope profonde construite par concaténations successives.

use rose::run::ChaineRep;
use std::rc::Rc;

fn feuille(s: &str) -> Rc<ChaineRep> { ChaineRep::plein(s.to_string()) }

fn attendu(rep: &ChaineRep, d: usize, f: usize) -> String {
    rep.aplatir().chars().skip(d).take(f.saturating_sub(d)).collect()
}

#[test]
fn tranche_feuilles_et_rope() {
    let longue = "a≈b𝄞c — π≈3,14 fin";
    let cas: Vec<Rc<ChaineRep>> = vec![
        feuille("abc def ghi"),                 // ASCII, `Plein`
        feuille(longue),                        // non-ASCII indexé (> 32 o)
        feuille("≈é"),                          // non-ASCII court, non indexé
        ChaineRep::concat(&feuille("ab"), &feuille("≈cd")),
    ];
    // Rope profonde (200 étages) : la pile de `tranche` ne doit pas déborder.
    let mut profonde = feuille("s");
    for _ in 0..200 { profonde = ChaineRep::concat(&profonde, &feuille("≈x")); }
    let mut tout = cas;
    tout.push(profonde);

    for rep in &tout {
        let n = rep.n;
        for d in 0..=n {
            for f in d..=n {
                assert_eq!(rep.tranche(d, f), attendu(rep, d, f),
                           "tranche {d}..{f} sur {} caractères", n);
            }
        }
    }
}

#[test]
fn tranche_compte_les_points_de_code_pas_les_octets() {
    let r = ChaineRep::concat(&feuille("a"), &feuille("𝄞b"));
    assert_eq!(r.n, 3);
    assert_eq!(r.octets, 1 + 4 + 1);
    assert_eq!(r.tranche(1, 2), "𝄞");
    assert_eq!(r.tranche(0, 3), "a𝄞b");
}
