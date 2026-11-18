// ═══════════════════════════════════════════════════════════════════════
// G2 — DÉMONSTRATION AOT AUTONOME (sans invoker le JIT Cranelift)
// ═══════════════════════════════════════════════════════════════════════
//
// Ce binaire est une démonstration du chemin **AOT natif** :
//
//   RIR pré-analysé → natif_emetteur::emettre → octets machine
//                   → link_natif::ecrire_elf → ./programme exécutable
//
// AUCUN appel à `rose::jit` (Cranelift) n'est fait ici. Le sous-ensemble
// utilisé est celui que l'émetteur sait traduire (M3.1) :
//   Affect, Saut, SautSiFauxAbstrait, RetourAbstrait
//   Slot, Entier, Binaire(Add/Sub/Mul/comparaisons), Neg, Non
//
// ⚠⚠⚠ **VÉRITÉ SUR LA DÉPENDANCE CRANELIFT** (à lire avant de conclure) :
// ce binaire utilise la bibliothèque `rose`, dont `Cargo.toml` déclare
// `cranelift` comme dépendance NON optionnelle sur toute cible native.
// Le binaire est donc compilé/linké AVEC Cranelift dans son graphe même
// s'il ne l'invoque jamais : `cargo tree -e features -i libc` montre
// `libc features=["std"]` via cranelift. **Ce binaire seul ne prouve PAS
// l'autonomie G2.** La preuve réelle « zéro cranelift » est le harnais
// isolé `/tmp/opencode/rose_g2_harness/` (voir docs/AUTONOMIE_G2.md) :
// il recopie les deux modules M3.1/M3.2 dans une crate à ZÉRO dépendance
// externe, avec des types RIR minimaux locaux.
//
// Le câblage `[[bin]] name="rose-autonome"` sera fait par l'orchestrateur
// (ce agent ne modifie PAS Cargo.toml).
//
// Usage : rose-autonome [chemin_elf_sortie]
//   défaut : /tmp/opencode/rose-autonome-elf
// Le code de retour du programme ELF généré vaut la somme 1+2+...+10 = 55.

use rose::link_natif::{self, Options};
use rose::natif_emetteur::{self, Architecture, MemoireExecutable, Trampoline};
use rose::rir::{CacheSlot, CorpsRir, Eabstrait, Op, Oper};

/// `slot(i)` — une variable du RIR pré-analysé.
fn slot(i: u16) -> Eabstrait {
    Eabstrait::Slot(i, CacheSlot::neuve())
}

/// `littéral entier` — un nombre exact déjà réduit en i64.
fn entier(v: &str) -> Eabstrait {
    Eabstrait::Entier(v.to_string())
}

/// `a <op> b` — un binaire arithmétique/comparaison du sous-ensemble natif.
fn bin(op: Oper, a: Eabstrait, b: Eabstrait) -> Eabstrait {
    Eabstrait::Binaire { op, a: Box::new(a), b: Box::new(b) }
}

/// Programme Rose pré-analysé (sous-ensemble RIR) :
///
/// ```rose
/// prendre n ...            # ici n = 10, posé par le programme lui-même
/// i = 1 ; acc = 0
/// tant que i <= n :
///     acc = acc + i
///     i   = i + 1
/// retour acc               # 55
/// ```
///
/// Le stub `_start` écrit par `link_natif` n'initialise PAS les slots : le
/// programme pose donc sa propre valeur de départ (slot 0 ← 10). Le code
/// machine émis ne fait AUCun appel runtime : la trampoline n'est jamais
/// utilisée, ce qui rend l'ELF autonome (pas de runtime Rose à embarquer).
fn programme_somme() -> CorpsRir {
    CorpsRir {
        // n = 10                       (op 0)
        // i = 1                        (op 1)
        // acc = 0                      (op 2)
        // si !(i <= n) → op 7          (op 3)
        // acc = acc + i                (op 4)
        // i = i + 1                    (op 5)
        // saut → op 3                  (op 6)
        // retour acc                   (op 7)
        ops: vec![
            Op::Affect { slot: 0, expr: entier("10") },
            Op::Affect { slot: 1, expr: entier("1") },
            Op::Affect { slot: 2, expr: entier("0") },
            Op::SautSiFauxAbstrait {
                test: bin(Oper::Le, slot(1), slot(0)),
                cible: 7,
            },
            Op::Affect { slot: 2, expr: bin(Oper::Add, slot(2), slot(1)) },
            Op::Affect { slot: 1, expr: bin(Oper::Add, slot(1), entier("1")) },
            Op::Saut { cible: 3 },
            Op::RetourAbstrait(slot(2)),
        ],
        noms: vec!["n".into(), "i".into(), "acc".into()],
        globaux: vec![],
        n_slots: 3,
    }
}

/// Trampolines de sécurité : le programme n'en a pas besoin, mais l'API
/// d'appel en exige une. Ces fonctions ne doivent JAMAIS être atteintes.
extern "C" fn tramp_interdit(_: i64, _: i64, _: i64, _: i64, _: *const u8) -> i64 {
    panic!("trampoline appelée : le programme aurait dû être 100% natif")
}
extern "C" fn fallback_interdit(_: *mut std::ffi::c_void, _: u32, _: *mut i64) -> i64 {
    panic!("fallback appelé : le programme aurait dû être 100% natif")
}

fn main() {
    let sortie = std::env::args().nth(1)
        .unwrap_or_else(|| "/tmp/opencode/rose-autonome-elf".to_string());
    let arch = Architecture::native();

    let corps = programme_somme();
    assert!(
        natif_emetteur::peut_emettre(&corps),
        "le programme n'appartient pas au sous-ensemble émettable"
    );

    let emitted = natif_emetteur::emettre(&corps, arch)
        .expect("émission machine impossible");
    println!(
        "AOT natif sur {} : {} octets machine, {} ops, {} appels runtime, {} fallbacks",
        arch.nom(), emitted.code.len(), emitted.op_offsets.len() - 1,
        emitted.appels.len(), emitted.fallbacks.len()
    );
    assert!(emitted.appels.is_empty() && emitted.fallbacks.is_empty(),
            "l'ELF autonome exige zéro appel runtime");

    // (1) Exécution directe du code machine mmap'é, sans ELF ni linker.
    let mem = MemoireExecutable::nouveau(&emitted.code).expect("mmap impossible");
    let tramp = Trampoline {
        appel_rir: tramp_interdit,
        eval_fallback: fallback_interdit,
        ctx: std::ptr::null_mut(),
    };
    let mut slots = vec![0i64; emitted.n_slots_total];
    let resultat = unsafe { mem.appeler(slots.as_mut_ptr(), &tramp) };
    println!("exécution machine directe : somme(1..10) = {}", resultat);
    assert_eq!(resultat, 55, "résultat machine faux");

    // (2) Écriture de l'ELF64 statique, sans gcc/clang/ld.
    let chemin = std::path::Path::new(&sortie);
    link_natif::ecrire_elf_depuis_emitted(&emitted, chemin)
        .expect("écriture ELF impossible");
    let octets_elf = std::fs::read(chemin).expect("relecture ELF impossible");
    println!(
        "ELF écrit : {} ({} octets, magic={:02X?})",
        chemin.display(), octets_elf.len(), &octets_elf[..4]
    );
    println!("À exécuter : {} ; echo $?   → attendu 55", chemin.display());

    // (3) `Options::depuis_emitted` public — prouve que l'API de lien est
    //     bien atteignable depuis un binaire hors du moteur.
    let opts = Options::depuis_emitted(&emitted);
    assert_eq!(opts.n_slots_total, emitted.n_slots_total);
    println!("options de lien : slots={} base=0x{:X}",
             opts.n_slots_total, opts.adresse_base);
}
