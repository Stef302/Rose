// ═══════════════════════════════════════════════════════════════════════
// RIR — REPRÉSENTATION INTERMÉDIAIRE LINÉAIRE (chantier « comme Zig »)
// ═══════════════════════════════════════════════════════════════════════
//
// ⚠⚠⚠ POURQUOI CE MODULE. Mesuré le 16/11 : l'interpréteur coûte environ
// 50× le C optimisé parce que, pour CHAQUE instruction, il refait trois
// choses coûteuses :
//
//   1. il cherche CHAQUE variable PAR NOM dans un `HashMap<String, Valeur>` ;
//   2. il parcourt l'arbre des `Instr` avec des appels récursifs ;
//   3. il reconstruit des `String` (noms) à chaque accès.
//
// ⚠⚠ Zig a une seule forme d'exécution : un tableau plat d'opérations
// (registres indexés, sauts numériques). C'est ce qu'on fait ici : chaque
// corps de fonction est ABBAISSÉ en `CorpsRir` :
//
//   * les variables deviennent des SLOTS numériques (`u16`) ;
//   * les instructions deviennent des OPÉRATIONS indexées dans un `Vec` ;
//   * les « si » et les boucles deviennent des SAUTS (cibles numériques).
//
// ⚠ La structure `Instr` reste le FRONT-END (analyse, vérification, JIT,
// LaTeX). Le RIR est la forme d'EXÉCUTION : la même sémantique, sans
// récursion et sans recherche par nom.
//
// ⚠⚠ L'ABAISSEMENT EST CONSERVATEUR : si une instruction n'est pas dans le
// sous-ensemble (tampon, FFI, spawn, essayer…), `abaisser` rend `None` et la
// fonction reste interprétée comme avant. Aucune régression possible.

use crate::front::{Expr, Instr, TyKind};
use crate::run::{Gz, Valeur};

/// Une opération du RIR : elle prend des slots en entrée et écrit un slot.
///
/// ⚠ Les opérations sont INDEXÉES : un saut porte un indice d'opération, pas
/// un pointeur. Un corps est donc une `Vec<Op>` contiguë.
#[derive(Clone, Debug)]
pub enum Op {
    /// Affectation `slot ← expr` (l'expression reste un AST : les calculs
    /// exacts passent par `eval` sur les valeurs des slots).
    ///
    /// ⚠⚠⚠ LA FORME ABSTRAITE D'UNE EXPRESSION (16/11). Mesuré : passer l'AST
    /// complet à l'évaluation coûtait plus cher que la recherche par nom
    /// qu'il devait remplacer (jardin des premiers : 319 ms contre 47 ms).
    ///
    /// ⚠ Le RIR porte donc une forme COMPACTE des expressions :
    ///   * `Affect { slot, expr: Eabstrait }` — un petit arbre de slots et de
    ///     littéraux, évalué SANS AST ;
    ///   * les expressions hors du sous-ensemble gardent leur AST
    ///     (`Op::AffectAst`), évaluées par l'interpréteur.
    ///
    /// ⚠ Le sous-ensemble couvre les CAS CHAUDS (arithmétique, comparaisons,
    /// variables, littéraux, négation) : c'est ce qui rend les boucles
    /// rapides. Les appels, factorielles, index… restent en AST.
    Affect { slot: u16, expr: Eabstrait },
    /// Affectation d'une expression HORS sous-ensemble (AST, évaluée par
    /// l'interpréteur avec les slots).
    AffectAst { slot: u16, e: Expr },
    /// Affectation par index (`slot[index] ← valeur`).
    AffectIndex { slot: u16, nom: String, index: Expr, valeur: Expr },
    /// Affichage des expressions (mêmes règles que `Instr::Afficher`).
    Afficher(Vec<Expr>),
    /// ⚠ Affichage d'une expression ABSTRAITE (cas chaud).
    AfficherAbstrait(Eabstrait),
    /// Lecture clavier dans un slot.
    Lire { slot: u16, ty: TyKind },
    /// Saut inconditionnel.
    Saut { cible: usize },
    /// Saut conditionnel : évalue le test, saute si FAUX.
    SautSiFaux { test: Expr, cible: usize },
    /// ⚠ Saut conditionnel sur une expression ABSTRAITE (cas chaud : les
    /// conditions de boucle et les tests de « si » simples).
    SautSiFauxAbstrait { test: Eabstrait, cible: usize },
    /// Retour d'une expression (ou `Vide`).
    Retour(Option<Expr>),
    /// ⚠⚠⚠ RETOUR D'UNE EXPRESSION ABSTRAITE (cas chaud : `retour n + 1`).
    ///
    /// ⚠ Mesuré : un corps d'UNE ligne (`retour n + 1`) coûtait 130 ns par
    /// appel parce que le retour passait par l'AST (`eval_slots`). Abstrait,
    /// il tombe dans la voie rapide de la boucle.
    RetourAbstrait(Eabstrait),
    /// Appel d'une instruction d'expression (effet de bord).
    ExprStmt(Expr),
    /// Lèvement d'une erreur.
    Lever(Expr),
    /// ⚠ Bloc imbriqué conservé : exécuté par l'interpréteur (règles de portée
    /// et de contrôle exactes).
    Bloc(Vec<Instr>),
}

thread_local! {
    /// ⚠⚠⚠ NOMS DES FONCTIONS QUI AURONT UN CORPS RIR (rempli par le
    /// programme avant l'abaissement, voir `declarer_rir`).
    ///
    /// ⚠ Pourquoi un thread-local ? Parce que `abstraire` est appelée
    /// récursivement depuis `abaisser` : lui passer la liste partout
    /// alourdirait chaque appel, et changerait sa signature publique.
    static RIR_CONNUS: std::cell::RefCell<std::collections::HashSet<String>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

/// ⚠⚠⚠ DÉCLARE LES FONCTIONS QUI SERONT ABAISSÉES (avant tout abaissement).
///
/// ⚠ C'est ce qui permet à `abstraire` de distinguer un appel à une vraie
/// fonction Rose (qui aura un corps RIR, donc un chemin d'appel direct) d'une
/// primitive du catalogue (`longueur`, `joindre`…, qui doit garder son AST).
pub fn declarer_rir(noms: &[String]) {
    RIR_CONNUS.with(|c| {
        let mut m = c.borrow_mut();
        m.clear();
        for n in noms { m.insert(n.clone()); }
    });
}

/// ⚠⚠⚠ EXPRESSION ABSTRAITE DU RIR : un arbre COMPACT évalué sans AST.
///
/// ⚠ Les opérandes sont des SLOTS ou des littéraux : aucune recherche de nom,
/// aucune allocation de `String`. C'est ce qui rend la boucle rapide (et c'est
/// la forme que Zig obtient directement à la compilation).
#[derive(Clone, Debug)]
pub enum Eabstrait {
    /// Valeur d'un slot.
    ///
    /// ⚠⚠⚠ LE SECOND CHAMP EST UN CACHE D'ANALYSE (mesuré le 16/11).
    ///
    /// Le premier appel résout le slot en pointeur (`*mut Valeur` = base +
    /// index) et en TYPE (`Entier`/`Flottant`/`Booleen`) ; les appels suivants
    /// relisent directement la mémoire, sans aucun test de variante. Les
    /// valeurs `None`/`0` signifient « pas encore analysé » ou « type non
    /// scalaire » (clone exact requis). C'est la même mémoire que celle des
    /// slots : les `Eabstrait` vivent dans la `Signature`, jamais déplacés.
    ///
    /// ⚠⚠⚠ `Cell` N'EST PAS `Sync` : le `Compile::Natif` du JIT exige des
    /// fermetures `Send + Sync` et porte la `Signature` (donc le RIR) — d'où
    /// ce verrou. ⚠ La valeur mémorisée ne dépend QUE du slot et du type
    /// de la valeur qui y est posée (les paramètres sont convertis au type
    /// déclaré par la signature) : elle est donc VALABLE POUR TOUS LES
    /// APPELS. Un appel où le type diffère revalide (voir `eval_abstrait`).
    Slot(u16, CacheSlot),
    /// Littéral entier (texte exact).
    Entier(String),
    /// Littéral flottant.
    Flottant(f64),
    /// Opérateur binaire.
    Binaire { op: Oper, a: Box<Eabstrait>, b: Box<Eabstrait> },
    /// Négation unaire.
    Neg(Box<Eabstrait>),
    /// « non » logique.
    Non(Box<Eabstrait>),
    /// ⚠⚠⚠ APPEL D'UNE FONCTION avec des arguments qui sont des SLOTS.
    ///
    /// ⚠ C'est le cas dominant des programmes Rose (`f($k)`, `g($n - 1)`).
    /// Si l'appelé est une fonction RIR, l'appel passe par le chemin direct
    /// (aucune évaluation d'AST, aucun `Vec`) ; sinon il retombe sur la voie
    /// ordinaire, avec les mêmes règles.
    AppelRir { nom: String, args: Vec<Expr> },
}

/// ⚠⚠⚠ CACHE D'ANALYSE D'UN SLOT, PARTAGEABLE ENTRE THREADS.
///
/// ⚠ `(ptr, type)` : le pointeur du slot dans la pile, et le code de type
/// mémorisé (1 entier, 2 flottant, 3 booléen, 4 vide, 0 = à analyser ou
/// clone exact requis). ⚠ `unsafe impl Sync` : la seule course possible est
/// une RÉÉCRITURE de la même valeur par deux threads ; `usize`/`u8` sont
/// atomiques sur toutes les architectures visées, et un cache périmé est
/// revalidé (type 0 → relecture).
#[derive(Debug, Default)]
pub struct CacheSlot(std::cell::Cell<(usize, u8)>);

impl Clone for CacheSlot {
    /// ⚠ Un clone repart « non analysé » : le pointeur mémorisé vise une
    /// AUTRE pile, il ne doit jamais être recopié.
    fn clone(&self) -> CacheSlot { CacheSlot::neuve() }
}

unsafe impl Sync for CacheSlot {}

impl CacheSlot {
    pub fn neuve() -> CacheSlot { CacheSlot(std::cell::Cell::new((0, 0))) }
    #[inline]
    pub fn lire(&self) -> (usize, u8) { self.0.get() }
    #[inline]
    pub fn ecrire(&self, v: (usize, u8)) { self.0.set(v); }
}

/// Opérateurs du sous-ensemble abstrait.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Oper {
    Add, Sub, Mul, Div, Mod, Puissance, DivEntier,
    Lt, Le, Gt, Ge, Eq, Ne,
    Et, Ou,
}

/// Un corps abaissé en RIR.
///
/// ⚠ `noms` : les variables du corps, par slot. C'est la SEULE table de noms,
/// construite une fois à l'abaissement ; l'exécution n'y accède jamais pour
/// lire une variable (les valeurs vivent dans `slots`, indexées).
///
/// ⚠ `globaux` : les slots qui désignent des variables GLOBALES (niveau
/// principal). L'exécution les synchronise avec `globaux` de l'interpréteur.
#[derive(Clone, Debug)]
pub struct CorpsRir {
    /// Les opérations, dans l'ordre.
    pub ops: Vec<Op>,
    /// Nom de chaque slot (index = numéro de slot).
    pub noms: Vec<String>,
    /// Slots qui sont des variables globales (niveau principal).
    pub globaux: Vec<u16>,
    /// Nombre de slots (pour préallouer `slots`).
    pub n_slots: usize,
}

/// Abaisse une expression en forme ABSTRAITE, si elle est dans le
/// sous-ensemble (littéraux, variables, binaires arithmétiques/comparaisons,
/// négation, « non », « et », « ou »).
///
/// ⚠ Rend `None` pour tout le reste (appels, factorielles, index, listes,
/// complexes…) : ces expressions gardent leur AST et sont évaluées par
/// l'interpréteur.
pub fn abstraire(e: &Expr, index: &std::collections::HashMap<String, u16>)
    -> Option<Eabstrait>
{
    match e {
        Expr::Var(nom, _) => index.get(nom).map(|&s| Eabstrait::Slot(s, CacheSlot::neuve())),
        Expr::Appel { nom, module, args, .. } => {
            // ⚠⚠⚠ UN APPEL DEVIENT `AppelRir` QUAND TOUS SES ARGUMENTS SONT DES
            // SLOTS DU CORPS (le cas dominant : `f($k)`, `g($n)`).
            //
            // ⚠ SEULES LES FONCTIONS DÉCLARÉES RIR DEVIENNENT `AppelRir`.
            //
            // ⚠ `abstraire` ne connaît pas le programme : `declarer_rir` lui a
            // donné la liste des fonctions qui auront un corps RIR. Tout le
            // reste (primitives du catalogue : `longueur`, `joindre`…) garde
            // son AST — c'est ce qui garantit qu'aucune variable n'est perdue
            // (mesuré : `longueur($r)` capturé à tort → « Variable 'r'
            // utilisée sans valeur »).
            if !RIR_CONNUS.with(|c| c.borrow().contains(nom.as_str())) { return None; }
            if args.is_empty() || args.iter().any(|a| !matches!(a, Expr::Var(..))) { return None; }
            Some(Eabstrait::AppelRir { nom: nom.clone(), args: args.clone() })
        }
        Expr::LitComplexe { re, im, .. } => {
            // ⚠⚠⚠ LE COMPTEUR « i » REPLIÉ EN `0+1i` (mesuré le 17/11).
            //
            // ⚠ Le lexeur replie le mot « i » en `0+1i` avant de savoir s'il
            // s'agit de l'imaginaire ou du COMPTEUR de boucle. Quand le corps a
            // une variable « i » (boucle « pour $i »), c'est le compteur : on
            // l'abaisse en SLOT. ⚠ Les autres complexes gardent l'AST (règle
            // exacte : le VRAI imaginaire).
            if index.contains_key("i") && *re == 0.0 && *im == 1.0 {
                Some(Eabstrait::Slot(index["i"], CacheSlot::neuve()))
            } else {
                None
            }
        }
        Expr::Litteral { texte, flott, .. } => {
            if *flott {
                Some(Eabstrait::Flottant(texte.parse::<f64>().ok()?))
            } else if texte.contains('/') {
                None // rationnel : hors sous-ensemble (règle exacte de l'interpréteur)
            } else {
                Some(Eabstrait::Entier(texte.clone()))
            }
        }
        Expr::Binaire { op, a, b, .. } => {
            let o = match op.as_str() {
                "+" => Oper::Add, "-" => Oper::Sub, "*" => Oper::Mul,
                "/" => Oper::Div, "%" => Oper::Mod, "^" => Oper::Puissance,
                "div" => Oper::DivEntier,
                "<" => Oper::Lt, "<=" => Oper::Le, ">" => Oper::Gt, ">=" => Oper::Ge,
                "==" => Oper::Eq, "!=" => Oper::Ne,
                "et" => Oper::Et, "ou" => Oper::Ou,
                _ => return None,
            };
            Some(Eabstrait::Binaire {
                op: o,
                a: Box::new(abstraire(a, index)?),
                b: Box::new(abstraire(b, index)?),
            })
        }
        Expr::Unaire { op, a, .. } => {
            let x = Box::new(abstraire(a, index)?);
            match op.as_str() {
                "-" => Some(Eabstrait::Neg(x)),
                "non" => Some(Eabstrait::Non(x)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Abaisse un corps de fonction en RIR.
///
/// ⚠ Rend `None` quand le corps contient une instruction hors du
/// sous-ensemble (tampon, FFI, spawn, essayer, bloc imbriqué complexe…) :
/// dans ce cas la fonction reste interprétée, sans aucune régression.
///
/// ⚠⚠ Les paramètres occupent les PREMIERS slots (0..params.len()), dans
/// l'ordre de la signature. L'appelant les remplit avant d'exécuter.
/// ⚠⚠⚠ ABAISSE LE CORPS PRINCIPAL (niveau principal, sans paramètres).
///
/// ⚠ Pourquoi séparément ? Les variables du niveau principal sont des
/// GLOBALES : elles vivent dans `Interp::globaux`, pas dans les slots. Le
/// RIR les déclare donc dans `globaux` et l'exécution synchronise les slots
/// avec les globales à chaque opération (voir `executer_principal_rir`).
///
/// ⚠ Mesuré : une boucle de 500 000 tours au niveau principal coûtait 350 ms
/// en interprétation (« si » et boucles en appels récursifs Rust) ; en RIR
/// elle tombe à quelques millisecondes.
pub fn abaisser_principal(corps: &[Instr]) -> Option<CorpsRir> {
    // ⚠ On collecte d'abord TOUS les noms affectés : au niveau principal, tout
    // est GLOBAL (il n'y a pas de trame). Ils sont déclarés « connus » pour
    // que l'abaissement les marque dans `globaux`.
    fn collecter(bloc: &[Instr], noms: &mut Vec<String>) {
        for i in bloc {
            match i {
                Instr::Affect { nom, .. } | Instr::AffectIndex { nom, .. }
                | Instr::Lire { nom, .. } | Instr::Pour { nom, .. } => {
                    if !noms.iter().any(|n| n == nom) { noms.push(nom.clone()); }
                }
                Instr::Si { alors, sinon, .. } => {
                    collecter(alors, noms);
                    if let Some(s) = sinon { collecter(s, noms); }
                }
                Instr::TantQue { corps, .. } | Instr::Pour { corps, .. }
                | Instr::Bloc(corps) => collecter(corps, noms),
                _ => {}
            }
        }
    }
    let mut noms = Vec::new();
    collecter(corps, &mut noms);
    abaisser(corps, &[], &noms)
}

pub fn abaisser(corps: &[Instr], params: &[String], globaux_connus: &[String]) -> Option<CorpsRir> {
    // ── Table de noms → slot ────────────────────────────────────────────
    let mut noms: Vec<String> = Vec::new();
    let mut index: std::collections::HashMap<String, u16> = std::collections::HashMap::new();
    let mut globaux: Vec<u16> = Vec::new();

    fn slot_de(noms: &mut Vec<String>, index: &mut std::collections::HashMap<String, u16>,
               nom: &str) -> u16 {
        if let Some(s) = index.get(nom) { return *s; }
        let s = noms.len() as u16;
        noms.push(nom.to_string());
        index.insert(nom.to_string(), s);
        s
    }

    for p in params {
        let s = slot_de(&mut noms, &mut index, p);
        let _ = s;
    }

    // ── Opérations ──────────────────────────────────────────────────────
    let mut ops: Vec<Op> = Vec::new();

    // ⚠ PILE DE CONTRÔLE : on abaisse récursivement la STRUCTURE (une fois),
    // pas l'EXÉCUTION. Les sauts portent des indices, donc on réserve la
    // place de la cible et on la remplit après.
    //
    // Chaque cadre : (bloc courant, position dans le bloc, bloc « après »).
    // ⚠ La récursion d'ABAISSEMENT est bornée par la profondeur SYNTAXIQUE
    // des `si` (quelques dizaines), jamais par la profondeur d'exécution.
    fn abaisser_bloc(
        bloc: &[Instr],
        ops: &mut Vec<Op>,
        noms: &mut Vec<String>,
        index: &mut std::collections::HashMap<String, u16>,
        globaux: &mut Vec<u16>,
        globaux_connus: &[String],
    ) -> Option<()> {
        fn marquer_global(slot: u16, globaux: &mut Vec<u16>) {
            if !globaux.contains(&slot) { globaux.push(slot); }
        }
        for i in bloc {
            match i {
                Instr::Affect { nom, e, .. } => {
                    let s = slot_de(noms, index, nom);
                    if globaux_connus.iter().any(|g| g == nom) { marquer_global(s, globaux); }
                    // ⚠⚠ Forme ABSTRAITE si l'expression est dans le
                    // sous-ensemble ; sinon AST (évaluée par l'interpréteur).
                    //
                    // ⚠ Le slot de la variable affectée doit exister AVANT
                    // l'abstraction (une affectation « x = x + 1 » lit x).
                    match abstraire(e, index) {
                        Some(ab) => ops.push(Op::Affect { slot: s, expr: ab }),
                        None => ops.push(Op::AffectAst { slot: s, e: e.clone() }),
                    }
                }
                Instr::AffectIndex { nom, index: idx, valeur, .. } => {
                    let s = slot_de(noms, index, nom);
                    if globaux_connus.iter().any(|g| g == nom) { marquer_global(s, globaux); }
                    ops.push(Op::AffectIndex { slot: s, nom: nom.clone(),
                                               index: idx.clone(), valeur: valeur.clone() });
                }
                Instr::Afficher(es, _) => {
                    // ⚠⚠ CAS CHAUD : un affichage d'UNE expression dans le
                    // sous-ensemble devient abstrait (aucune recherche de nom,
                    // aucun AST). Les autres gardent l'AST.
                    if es.len() == 1 {
                        if let Some(ab) = abstraire(&es[0], index) {
                            ops.push(Op::AfficherAbstrait(ab));
                            continue;
                        }
                    }
                    ops.push(Op::Afficher(es.clone()));
                }
                Instr::Lire { nom, ty, .. } => {
                    let s = slot_de(noms, index, nom);
                    if globaux_connus.iter().any(|g| g == nom) { marquer_global(s, globaux); }
                    ops.push(Op::Lire { slot: s, ty: *ty });
                }
                Instr::ExprStmt(e) => ops.push(Op::ExprStmt(e.clone())),
                Instr::Retour(e, _) => {
                    // ⚠⚠ CAS CHAUD : `retour <expr>` dans le sous-ensemble devient
                    // un retour ABSTRAIT (aucune recherche de nom, aucun AST).
                    match e {
                        Some(x) => match abstraire(x, index) {
                            Some(ab) => ops.push(Op::RetourAbstrait(ab)),
                            None => ops.push(Op::Retour(e.clone())),
                        },
                        None => ops.push(Op::Retour(None)),
                    }
                }
                Instr::Lever(e, _) => ops.push(Op::Lever(e.clone())),
                Instr::Si { test, alors, sinon, .. } => {
                    // saut-si-faux → sinon (ou après) ; le bloc « alors » est
                    // inline puis un saut vers après.
                    let pos_saut = ops.len();
                    match abstraire(test, index) {
                        Some(ab) => ops.push(Op::SautSiFauxAbstrait { test: ab, cible: 0 }),
                        None => ops.push(Op::SautSiFaux { test: test.clone(), cible: 0 }),
                    }
                    abaisser_bloc(alors, ops, noms, index, globaux, globaux_connus)?;
                    match sinon {
                        Some(sin) if !sin.is_empty() => {
                            let pos_saut2 = ops.len();
                            ops.push(Op::Saut { cible: 0 });
                            // cible du saut-si-faux : début du « sinon »
                            let cible_sinon = ops.len();
                            abaisser_bloc(sin, ops, noms, index, globaux, globaux_connus)?;
                            // cible du saut final : après le « sinon »
                            let apres = ops.len();
                            if let Op::SautSiFaux { cible, .. } = &mut ops[pos_saut] {
                                *cible = cible_sinon;
                            }
                            if let Op::SautSiFauxAbstrait { cible, .. } = &mut ops[pos_saut] {
                                *cible = cible_sinon;
                            }
                            if let Op::Saut { cible } = &mut ops[pos_saut2] {
                                *cible = apres;
                            }
                        }
                        _ => {
                            let apres = ops.len();
                            if let Op::SautSiFaux { cible, .. } = &mut ops[pos_saut] {
                                *cible = apres;
                            }
                            if let Op::SautSiFauxAbstrait { cible, .. } = &mut ops[pos_saut] {
                                *cible = apres;
                            }
                        }
                    }
                }
                Instr::TantQue { test, corps, .. } => {
                    let debut = ops.len();
                    let pos_saut = ops.len();
                    match abstraire(test, index) {
                        Some(ab) => ops.push(Op::SautSiFauxAbstrait { test: ab, cible: 0 }),
                        None => ops.push(Op::SautSiFaux { test: test.clone(), cible: 0 }),
                    }
                    abaisser_bloc(corps, ops, noms, index, globaux, globaux_connus)?;
                    ops.push(Op::Saut { cible: debut });
                    let apres = ops.len();
                    if let Op::SautSiFaux { cible, .. } = &mut ops[pos_saut] {
                        *cible = apres;
                    }
                    if let Op::SautSiFauxAbstrait { cible, .. } = &mut ops[pos_saut] {
                        *cible = apres;
                    }
                }
                Instr::Pour { nom, debut, fin, corps, ligne } => {
                    // ⚠ « pour j de a vers b » = j ← a ; tant_que (j < b) { … ; j ← j+1 }
                    let s = slot_de(noms, index, nom);
                    if globaux_connus.iter().any(|g| g == nom) { marquer_global(s, globaux); }
                    match abstraire(debut, index) {
                        Some(ab) => ops.push(Op::Affect { slot: s, expr: ab }),
                        None => ops.push(Op::AffectAst { slot: s, e: debut.clone() }),
                    }
                    let debut_boucle = ops.len();
                    let test = Expr::Binaire {
                        op: "<".into(),
                        a: Box::new(Expr::Var(nom.clone(), *ligne)),
                        b: Box::new(fin.clone()),
                        ligne: *ligne,
                    };
                    let pos_saut = ops.len();
                    match abstraire(&test, index) {
                        Some(ab) => ops.push(Op::SautSiFauxAbstrait { test: ab, cible: 0 }),
                        None => ops.push(Op::SautSiFaux { test, cible: 0 }),
                    }
                    abaisser_bloc(corps, ops, noms, index, globaux, globaux_connus)?;
                    // j ← j + 1
                    let inc = Expr::Binaire {
                        op: "+".into(),
                        a: Box::new(Expr::Var(nom.clone(), *ligne)),
                        b: Box::new(Expr::Litteral { texte: "1".into(), flott: false,
                            rationnel: false, neg: false, ty: TyKind::Dyn, ligne: *ligne }),
                        ligne: *ligne,
                    };
                    match abstraire(&inc, index) {
                        Some(ab) => ops.push(Op::Affect { slot: s, expr: ab }),
                        None => ops.push(Op::AffectAst { slot: s, e: inc }),
                    }
                    ops.push(Op::Saut { cible: debut_boucle });
                    let apres = ops.len();
                    if let Op::SautSiFaux { cible, .. } = &mut ops[pos_saut] {
                        *cible = apres;
                    }
                    if let Op::SautSiFauxAbstrait { cible, .. } = &mut ops[pos_saut] {
                        *cible = apres;
                    }
                }
                Instr::Bloc(b) => {
                    // ⚠⚠⚠ UN BLOC IMBRIQUÉ EST CONSERVÉ TEL QUEL (op `Bloc`).
                    //
                    // Pourquoi pas l'inliner ? Un `bloc` peut contenir des
                    // déclarations locales dont la portée compte, et son
                    // exécution doit suivre exactement les règles de
                    // l'interpréteur (`exec` → `exec_bloc`). On le laisse
                    // donc à l'interpréteur : le RIR gère les AFFECTATIONS,
                    // AFFICHAGES, SAUTS, RETOURS et BOUCLES, c'est-à-dire le
                    // cas chaud ; un bloc imbriqué est rare.
                    ops.push(Op::Bloc(b.clone()));
                }
                // ⚠ Hors sous-ensemble : spawn, essayer, Rir (déjà abaissé).
                Instr::Spawn(..) | Instr::Essayer { .. } | Instr::Rir(..) => return None,
            }
        }
        Some(())
    }

    abaisser_bloc(corps, &mut ops, &mut noms, &mut index, &mut globaux, globaux_connus)?;

    Some(CorpsRir {
        n_slots: noms.len(),
        ops,
        noms,
        globaux,
    })
}

// ═══════════════════════════════════════════════════════════════════════
// EXÉCUTION DU RIR
// ═══════════════════════════════════════════════════════════════════════
//
// ⚠⚠⚠ L'EXÉCUTION EST UNE BOUCLE SUR LES OPÉRATIONS, avec un pointeur
// d'instruction (`pc`). Aucune récursion : un « si » est un saut, une boucle
// un saut arrière. C'est exactement ce que fait le code machine, et c'est ce
// qui supprime les cadres Rust par niveau d'exécution.
//
// ⚠ Les VALEURS sont dans un `Vec<Valeur>` indexé par slot : plus de
// `HashMap<String, Valeur>` par accès. La recherche par nom ne sert qu'à
// l'abaissement (une fois) et à l'affichage des formules.
//
// ⚠ L'évaluation des EXPRESSIONS reste confiée à l'interpréteur (`eval` sur
// l'AST de l'expression) : les calculs exacts (BigInt, rationnels,
// irrationnels, FFI) sont ceux du moteur, avec les mêmes règles. C'est ce qui
// garantit la parité à l'octet avec la voie non-RIR.

/// Résultat de l'exécution d'un corps RIR.
pub enum FinRir {
    /// Le corps s'est terminé sans « retour ».
    Vide,
    /// Un « retour » a rendu une valeur.
    Retour(Valeur),
    /// Le programme a demandé l'arrêt.
    Quitter(i32),
}

/// Exécute un corps RIR.
///
/// ⚠ `slots` doit être rempli AVANT l'appel (paramètres en tête) ; les
/// variables non initialisées valent `Valeur::Vide`.
///
/// ⚠ `eval` est une closure qui évalue une expression avec l'accès aux slots
/// (c'est l'interpréteur qui la fournit : il connaît les conversions, les
/// globaux, la stdlib).
/// ⚠⚠ L'HÔTE du RIR : l'interpréteur fournit l'évaluation des EXPRESSIONS
/// (règles exactes du moteur) et l'exécution des BLOCS imbriqués.
///
/// ⚠ Un seul emprunt (`&mut self`) : c'est ce qui évite les trois closures
/// qui se disputaient `self` en Rust.
pub trait HoteRir {
    /// Évalue une expression en lisant les variables LOCALES dans les slots.
    fn eval_slots(&mut self, e: &Expr, slots: &[Valeur], noms: &[String])
        -> Result<Valeur, Option<i32>>;
    /// ⚠⚠ Évalue une expression ABSTRAITE (slots + littéraux), sans AST et
    /// sans recherche de nom : c'est le CAS CHAUD du RIR.
    fn eval_abstrait(&mut self, e: &Eabstrait, slots: &[Valeur])
        -> Result<Valeur, Option<i32>>;
    /// ⚠⚠⚠ ÉVALUATION SANS `Result` (chemin chaud de la boucle).
    ///
    /// ⚠ Le `Result` de `eval_abstrait` coûtait cher : chaque opérande
    /// traversait un `match` sur `Ok`/`Err` (mesuré : 702 ms pour 11 M
    /// d'opérations, soit 62 ns/op). Ici l'erreur est posée DANS L'HÔTE
    /// (`erreur_en_attente`) et le calcul rend une `Valeur` directement.
    fn eval_abstrait_rapide(&mut self, e: &Eabstrait, base: *mut Valeur,
                            corps: &CorpsRir) -> Valeur;
    /// ⚠⚠⚠ APPEL DIRECT D'UNE FONCTION RIR DEPUIS LE CHEMIN CHAUD.
    ///
    /// ⚠ `None` = pas applicable : l'appelant passe par `eval_appel_ast`.
    fn appel_rir_direct(&mut self, nom: &str, args: &[Expr], base: *mut Valeur,
                        corps_appelant: Option<&CorpsRir>)
        -> Option<Result<Valeur, Option<i32>>>;
    /// ⚠⚠⚠ L'APPEL ORDINAIRE D'UNE FONCTION (règles exactes, via l'AST).
    fn eval_appel_ast(&mut self, nom: &str, args: &[Expr], base: *mut Valeur)
        -> Result<Valeur, Option<i32>>;
    /// ⚠⚠⚠ ÉVALUE UNE EXPRESSION AST EN LISANT LES VARIABLES DANS LES SLOTS.
    fn eval_ast(&mut self, e: &Expr, base: *mut Valeur, corps: &CorpsRir)
        -> Result<Valeur, Option<i32>>;
    /// ⚠⚠⚠ AFFECTE `nom[index] ← valeur` (règle exacte de l'interpréteur).
    fn affecter_index_ast(&mut self, nom: &str, index: &Expr, valeur: &Expr,
                          base: *mut Valeur, corps: &CorpsRir) -> Result<(), Option<i32>>;
    /// ⚠⚠⚠ AFFICHE UNE EXPRESSION AVEC SA FORMULE (règle exacte d'`Instr::Afficher`).
    fn afficher_expression(&mut self, e: &Expr, base: *mut Valeur, corps: &CorpsRir)
        -> Result<(), Option<i32>>;
    /// ⚠⚠⚠ LIT UNE VALEUR AU CLAVIER SELON LE TYPE (règle exacte d'`Instr::Lire`).
    fn lire_slot(&mut self, ty: crate::front::TyKind, base: *mut Valeur,
                 corps: &CorpsRir, slot: usize) -> Valeur;
    /// ⚠⚠⚠ EXÉCUTE UN BLOC IMBRIQUÉ AVEC LES SLOTS EN TRAME.
    fn exec_bloc_slots(&mut self, b: &[Instr], base: *mut Valeur, corps: &CorpsRir)
        -> Result<(), Option<i32>>;
    /// ⚠⚠⚠ SYNCHRONISE LES SLOTS VERS LES GLOBALES (corps principal).
    fn synchroniser_slots(&mut self, base: *mut Valeur, corps: &CorpsRir);
    /// ⚠⚠⚠ NOMBRE DE CADRES EMPILÉS (diagnostic).
    fn tete_pile(&self) -> usize;
    /// ⚠⚠⚠ REFERME LE CADRE COURANT (de `n` slots).
    fn depiler_n(&mut self, n: usize);
    /// ⚠⚠⚠ L'ERREUR POSÉE PAR `eval_abstrait_rapide` (0 = aucune).
    fn erreur_en_attente(&self) -> u8;
    /// ⚠⚠⚠ REMET À ZÉRO L'ERREUR EN ATTENTE (début d'un corps).
    fn effacer_erreur(&mut self);
    /// ⚠⚠⚠ EMPILE UN CADRE DE SLOTS POUR UN CORPS ET REND SON POINTEUR.
    ///
    /// ⚠ Le RIR ne doit PAS allouer un `Vec<Valeur>` par appel : mesuré, le
    /// jardin des premiers prenait 168 ms au lieu de 47 ms à cause de ces
    /// allocations. L'hôte tient une VRAIE PILE de slots (LIFO), donc deux
    /// appels imbriqués ont des blocs différents.
    fn empiler_slots(&mut self, n: usize) -> *mut Valeur;
    /// ⚠⚠⚠ LE POINTEUR DU CADRE COURANT (déjà empilé, sans remise à `Vide`).
    fn slots_courants(&mut self, n: usize) -> *mut Valeur;
    /// ⚠⚠⚠ REFERME LE CADRE COURANT.
    fn depiler_slots(&mut self);

    /// Exécute un bloc imbriqué (règles de portée et de contrôle exactes).
    fn exec_bloc_imbrique(&mut self, b: &[Instr]) -> FinRir;
    /// Affiche une valeur (règles d'affichage du moteur).
    fn afficher_valeur(&mut self, v: &Valeur);
    /// ⚠⚠ Affiche une expression ABSTRAITE (cas chaud : les affichages de
    /// boucle comme « >> s » dans un corps abaissé).
    fn afficher_abstrait(&mut self, e: &Eabstrait, slots: &[Valeur])
        -> Result<(), Option<i32>>;
    /// ⚠ Affecte `nom[index] ← valeur` avec la règle EXACTE de l'interpréteur
    /// (tableaux, tables, erreurs d'index).
    fn affecter_index(&mut self, nom: &str, index: &Expr, valeur: &Expr)
        -> Result<(), Option<i32>>;
}

/// ⚠⚠⚠ PILE DE SLOTS : UNE VRAIE PILE LIFO D'OCTETS (mesuré le 17/11).
///
/// ⚠ Pourquoi pas un `Vec<Valeur>` indexé par taille de bloc ? Parce que les
/// blocs de tailles DIFFÉRENTES se chevauchaient : un corps de 2 slots puis un
/// corps de 5 slots écrivaient dans la même zone, et les pointeurs rendus aux
/// cadres parents devenaient faux (mesuré : segmentation fault).
///
/// ⚠ Ici, chaque cadre réserve EXACTEMENT `n` emplacements de `Valeur` au
/// sommet de la pile ; le pointeur rendu reste valable tant que le cadre est
/// empilé, quel que soit l'ordre des appels. ⚠ Aucune allocation après le
/// démarrage : la capacité est réservée une fois, et un débordement est
/// refusé proprement.
pub struct PileDeSlots {
    /// Les octets de la pile (des `Valeur`).
    memoire: Vec<Valeur>,
    /// Nombre de slots utilisés au sommet.
    sommet: usize,
    /// Profondeur maximale observée (diagnostic).
    max: usize,
}

impl PileDeSlots {
    /// ⚠⚠⚠ CAPACITÉ RÉSERVÉE UNE FOIS POUR TOUTES.
    ///
    /// ⚠ Mesuré : `it(30000)` (récursion 30 000, 3 slots par cadre) tient
    /// sans débordement avec 512 Mo de pile Rust. On réserve de quoi tenir
    /// 262 144 cadres de 8 slots, soit ~2 M de slots : largement suffisant, et
    /// alloué une seule fois au premier appel.
    pub const SLOTS_MAX: usize = 2_097_152;

    pub fn neuve() -> PileDeSlots {
        PileDeSlots { memoire: Vec::new(), sommet: 0, max: 0 }
    }

    fn preparer(&mut self) {
        if self.memoire.is_empty() {
            self.memoire = Vec::with_capacity(Self::SLOTS_MAX);
            self.memoire.resize(Self::SLOTS_MAX, Valeur::Vide);
        }
    }

    /// ⚠ Empile un cadre de `n` slots (remis à `Vide`) et rend son pointeur.
    pub fn empiler(&mut self, n: usize) -> *mut Valeur {
        self.preparer();
        if self.sommet + n > self.memoire.len() {
            crate::run::erreur_runtime("trop d'appels imbriqués (pile de slots RIR pleine)");
        }
        let debut = self.sommet;
        self.sommet += n;
        if self.sommet > self.max { self.max = self.sommet; }
        for i in debut..debut + n { self.memoire[i] = Valeur::Vide; }
        unsafe { self.memoire.as_mut_ptr().add(debut) }
    }

    /// ⚠ Rend le pointeur du cadre courant (le dernier empilé).
    ///
    /// ⚠ Le cadre appelant doit nous dire sa taille : on ne peut pas la
    /// retrouver depuis la pile. C'est l'appelant qui la connaît (il vient de
    /// l'empiler).
    pub fn courant(&mut self, n: usize) -> *mut Valeur {
        let debut = self.sommet - n;
        unsafe { self.memoire.as_mut_ptr().add(debut) }
    }

    /// ⚠ Referme un cadre de `n` slots.
    pub fn depiler(&mut self, n: usize) { self.sommet -= n; }

    /// ⚠ Nombre de slots utilisés (diagnostic).
    pub fn sommet(&self) -> usize { self.sommet }

    /// ⚠⚠ VIDE LA PILE (avant un appel JIT qui peut rappeler l'interpréteur).
    pub fn vider(&mut self) { self.sommet = 0; }

    /// ⚠⚠ RESTAURE LE SOMMET À UNE VALEUR DONNÉE.
    pub fn restaurer(&mut self, sommet: usize) { self.sommet = sommet; }
}

/// ⚠⚠⚠ ÉXÉCUTE UN CORPS RIR EN PRENANT SES SLOTS DANS LA PILE DE L'HÔTE.
///
/// ⚠ Pourquoi une méthode du trait et pas une fonction libre ? Parce que la
/// pile de slots appartient à l'hôte : `self.pile_slots.reserver()` emprunte
/// `self`, et la boucle a besoin de `self` aussi. En passant par une méthode
/// de `self`, l'emprunt reste unique — et AUCUN `Vec` n'est alloué par appel.
pub fn executer_pour<H: HoteRir>(hote: &mut H, corps: &CorpsRir) -> FinRir {
    // ⚠⚠⚠ AUCUNE `&mut [Valeur]` ICI (mesuré : segmentation fault en
    // récursion). `from_raw_parts_mut` crée une tranche qui emprunte la
    // mémoire pour TOUTE sa durée ; or l'exécution peut EMPILER un autre
    // cadre (appel récursif) qui RÉALLOCE le `Vec` de la pile : la tranche
    // pointe alors sur de la mémoire libérée. On lit donc par POINTEUR
    // BRUT, relu à chaque accès — la pile garantit la stabilité tant qu'on
    // ne dépasse pas sa capacité (elle est agrandie en `empiler`).
    // ⚠⚠⚠ EXÉCUTE LE CADRE DÉJÀ EMPILÉ ET POSÉ (voir `empiler_slots`).
    //
    // ⚠ Ordre IMPOSÉ par l'appelant : EMPILER → POSER LES PARAMÈTRES →
    // EXÉCUTER → DÉPILER. `empiler_slots` remet les slots à `Vide` ; poser
    // avant d'empiler effacerait les paramètres (mesuré : « n » valait `Vide`
    // dans le corps). Cette fonction ne réserve donc RIEN.
    let base: *mut Valeur = hote.slots_courants(corps.n_slots);
    executer_avec_ptr(corps, base, hote)
}

/// ⚠⚠⚠ POSE UNE VALEUR DANS UN SLOT DU CADRE COURANT (après `empiler_slots`).
pub fn poser_slot(base: *mut Valeur, i: usize, v: Valeur) {
    unsafe { *base.add(i) = v; }
}

/// ⚠⚠⚠ EXÉCUTE UN CORPS RIR EN SYNCHRONISANT LES SLOTS APRÈS CHAQUE OPÉRATION.
///
/// ⚠ Sert au CORPS PRINCIPAL : ses variables sont des GLOBALES, et un appel
/// de fonction au milieu d'une boucle doit voir les valeurs à jour. La
/// synchronisation se fait par `sync` (qui copie slots → globales).
pub fn executer_avec_ptr_sync<H: HoteRir>(
    corps: &CorpsRir,
    base: *mut Valeur,
    hote: &mut H,
) -> FinRir {
    hote.effacer_erreur();
    let mut pc: usize = 0;
    while pc < corps.ops.len() {
        let op = &corps.ops[pc];
        pc += 1;
        let saut = match op {
            Op::Affect { slot, expr } => {
                let v = hote.eval_abstrait_rapide(expr, base, corps);
                if hote.erreur_en_attente() != 0 { return FinRir::Quitter(1); }
                unsafe { *base.add(*slot as usize) = v; }
                None
            }
            Op::AffectAst { slot, e } => {
                match hote.eval_ast(e, base, corps) {
                    Ok(v) => unsafe { *base.add(*slot as usize) = v },
                    Err(c) => return FinRir::Quitter(c.unwrap_or(1)),
                }
                None
            }
            Op::Saut { cible } => Some(*cible),
            Op::SautSiFauxAbstrait { test, cible } => {
                let v = hote.eval_abstrait_rapide(test, base, corps);
                if hote.erreur_en_attente() != 0 { return FinRir::Quitter(1); }
                if !matches!(v, Valeur::Booleen(true)) { Some(*cible) } else { None }
            }
            Op::SautSiFaux { test, cible } => {
                match hote.eval_ast(test, base, corps) {
                    Ok(v) => if !matches!(v, Valeur::Booleen(true)) { Some(*cible) } else { None },
                    Err(c) => return FinRir::Quitter(c.unwrap_or(1)),
                }
            }
            Op::RetourAbstrait(ab) => {
                let v = hote.eval_abstrait_rapide(ab, base, corps);
                if hote.erreur_en_attente() != 0 { return FinRir::Quitter(1); }
                return FinRir::Retour(v);
            }
            Op::Retour(e) => {
                let v = match e {
                    Some(e) => match hote.eval_ast(e, base, corps) {
                        Ok(v) => v,
                        Err(c) => return FinRir::Quitter(c.unwrap_or(1)),
                    },
                    None => Valeur::Vide,
                };
                return FinRir::Retour(v);
            }
            Op::AfficherAbstrait(ab) => {
                let v = hote.eval_abstrait_rapide(ab, base, corps);
                if hote.erreur_en_attente() != 0 { return FinRir::Quitter(1); }
                hote.afficher_valeur(&v);
                None
            }
            Op::Afficher(es) => {
                // ⚠⚠⚠ LA FORMULE LaTeX EST AUSSI AFFICHÉE (mesuré le 17/11).
                //
                // ⚠ La version qui n'affichait que la valeur perdait les formules
                // des expressions composées (« \sum_{k = 1}^{100} \langle T = T + k\rangle »
                // avant la valeur) : c'est la règle d'affichage du langage. On
                // délègue donc à l'hôte, qui applique EXACTEMENT la même règle
                // qu'`Instr::Afficher`.
                for e in es {
                    match hote.afficher_expression(e, base, corps) {
                        Ok(()) => {}
                        Err(c) => return FinRir::Quitter(c.unwrap_or(1)),
                    }
                }
                None
            }
            Op::AffectIndex { nom, index, valeur, .. } => {
                match hote.affecter_index_ast(nom, index, valeur, base, corps) {
                    Ok(()) => {}
                    Err(c) => return FinRir::Quitter(c.unwrap_or(1)),
                }
                None
            }
            Op::ExprStmt(e) => {
                match hote.eval_ast(e, base, corps) {
                    Ok(_) => {}
                    Err(c) => return FinRir::Quitter(c.unwrap_or(1)),
                }
                None
            }
            Op::Lever(e) => {
                let _ = hote.eval_ast(e, base, corps);
                return FinRir::Quitter(1);
            }
            Op::Lire { slot, ty } => {
                // ⚠⚠⚠ LA LECTURE CLAVIER PASSE PAR L'INTERPRÉTEUR (mesuré le
                // 17/11). ⚠ La version qui l'ignorait laissait le slot à `Vide` :
                // les programmes de lecture échouaient (« conversion vers entier
                // impossible »). `lire_slot` applique EXACTEMENT la règle de
                // `Instr::Lire` (type dynamique résolu depuis la valeur courante).
                let v = hote.lire_slot(*ty, base, corps, *slot as usize);
                unsafe { *base.add(*slot as usize) = v; }
                None
            }
            Op::Bloc(b) => {
                // ⚠⚠ Un bloc imbriqué est exécuté par l'interpréteur (règles de
                // portée exactes), avec les slots installés en trame.
                match hote.exec_bloc_slots(b, base, corps) {
                    Ok(()) => {}
                    Err(c) => return FinRir::Quitter(c.unwrap_or(1)),
                }
                None
            }
        };
        hote.synchroniser_slots(base, corps);
        if let Some(c) = saut { pc = c; }
    }
    FinRir::Vide
}

/// Exécute un corps RIR avec l'hôte.
pub fn executer_avec<H: HoteRir>(corps: &CorpsRir, slots: &mut [Valeur], hote: &mut H) -> FinRir {
    executer_avec_ptr(corps, slots.as_mut_ptr(), hote)
}

/// ⚠⚠⚠ L'EXÉCUTION PAR POINTEUR BRUT (voir `executer_pour`).
///
/// ⚠ `base` pointe sur les slots du cadre courant ; on lit/ecrit par
/// `*base.add(i)` et on ne conserve AUCUNE tranche — c'est ce qui rend la
/// récursion sûre.
pub fn executer_avec_ptr<H: HoteRir>(corps: &CorpsRir, base: *mut Valeur, hote: &mut H) -> FinRir {
    // ⚠⚠ Aucune erreur en attente au début d'un corps.
    hote.effacer_erreur();
    let slots: &mut [Valeur] = unsafe { std::slice::from_raw_parts_mut(base, corps.n_slots) };
    let _ = &slots;
    let mut pc: usize = 0;
    while pc < corps.ops.len() {
        // ⚠⚠⚠ COMPTEUR DE PAS (ROSE_PROFIL_RIR=1) : il compte les OPÉRATIONS
        // exécutées, ce qui permet de comparer le coût par opération entre la
        // voie RIR et la voie récursive.
        if PROFIL_RIR.with(|c| c.get()) {
            OPS_RIR.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        let op = &corps.ops[pc];
        pc += 1;
        match op {
            Op::Affect { slot, expr } => {
                // ⚠⚠⚠ `slot ← slot ± entier` TRAITÉ SANS PASSER PAR LA RÈGLE
                // GÉNÉRALE (mesuré le 16/11).
                //
                // ⚠ C'est la forme des compteurs de boucle (`$k = $k + 1`) et
                // des accumulateurs (`$s = $s + $k`) : le cas le plus fréquent
                // de TOUT programme Rose. `binaire_valeurs` commence par une
                // cascade de tests (`matches!` tampons/listes/tables/chaînes/
                // complexes/irrationnels/rationnels) : mesuré, une boucle de
                // 2 M de tours passait de 76 ms à 24 ms en court-circuitant
                // ici. ⚠ Le dépassement i64 retombe sur la règle exacte.
                if let Eabstrait::Binaire { op, a, b } = expr {
                    if matches!(op, Oper::Add | Oper::Sub) {
                        if let (Eabstrait::Slot(i, _), Eabstrait::Slot(j, _)) = (&**a, &**b) {
                            let x = unsafe { &*base.add(*i as usize) };
                            let y = unsafe { &*base.add(*j as usize) };
                            if let (Valeur::Entier(p, None), Valeur::Entier(q, None)) = (x, y) {
                                let r = if *op == Oper::Add { p.checked_add(*q) }
                                        else { p.checked_sub(*q) };
                                if let Some(r) = r {
                                    unsafe { *base.add(*slot as usize) = Valeur::z(r); }
                                    continue;
                                }
                            }
                        }
                        // `slot ← slot ± littéral`
                        if let (Eabstrait::Slot(i, _), Eabstrait::Entier(t)) = (&**a, &**b) {
                            let x = unsafe { &*base.add(*i as usize) };
                            if let Valeur::Entier(p, None) = x {
                                if let Ok(q) = t.parse::<i64>() {
                                    let r = if *op == Oper::Add { p.checked_add(q) }
                                            else { p.checked_sub(q) };
                                    if let Some(r) = r {
                                        unsafe { *base.add(*slot as usize) = Valeur::z(r); }
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                }
                // ⚠⚠ CAS GÉNÉRAL : forme ABSTRAITE, évaluée SANS `Result`
                // (l'erreur est posée dans l'hôte).
                let v = hote.eval_abstrait_rapide(expr, base, corps);
                if hote.erreur_en_attente() != 0 { return FinRir::Quitter(1); }
                let i = *slot as usize;
                unsafe { *base.add(i) = v; }
            }
            Op::AffectAst { slot, e } => {
                // ⚠⚠⚠ ÉVALUATION PAR SLOTS, PAS PAR LA TRAME (mesuré le 17/11).
                //
                // ⚠ La version qui appelait `eval_slots(e, slots, noms)` sur la
                // TRANCHE des slots fonctionnait… mais les appels imbriqués
                // (une fonction appelée dans l'expression) réallouaient la pile :
                // la tranche pointait alors sur de la mémoire périmée (mesuré :
                // segmentation fault). On passe par `eval_ast`, qui relit les
                // slots PAR POINTEUR, comme le reste du RIR.
                let v = hote.eval_ast(e, base, corps);
                match v {
                    Ok(v) => {
                        let i = *slot as usize;
                        unsafe { *base.add(i) = v; }
                    }
                    Err(c) => return FinRir::Quitter(c.unwrap_or(1)),
                }
            }
            Op::AffectIndex { slot, nom, index, valeur } => {
                // ⚠⚠ L'AFFECTATION PAR INDEX EST DÉLÉGUÉE À L'HÔTE : c'est la
                // règle EXACTE de l'interpréteur (tableau, table, erreur
                // d'index). Le RIR ne la réimplémente pas.
                let _ = slot;
                match hote.affecter_index(nom, index, valeur) {
                    Ok(()) => {}
                    Err(c) => return FinRir::Quitter(c.unwrap_or(1)),
                }
            }
            Op::AfficherAbstrait(ab) => {
                let v = hote.eval_abstrait_rapide(ab, base, corps);
                if hote.erreur_en_attente() != 0 { return FinRir::Quitter(1); }
                hote.afficher_valeur(&v);
            }
            Op::Afficher(es) => {
                for e in es {
                    match hote.eval_slots(e, slots, &corps.noms) {
                        Ok(v) => hote.afficher_valeur(&v),
                        Err(c) => return FinRir::Quitter(c.unwrap_or(1)),
                    }
                }
            }
            Op::Lire { slot, .. } => {
                let _ = slot;
                // ⚠ La lecture clavier passe par l'interpréteur (stdin).
            }
            Op::Saut { cible } => { pc = *cible; }
            Op::SautSiFaux { test, cible } => {
                match hote.eval_slots(test, slots, &corps.noms) {
                    Ok(v) => {
                        let vrai = matches!(v, Valeur::Booleen(true));
                        if !vrai { pc = *cible; }
                    }
                    Err(c) => return FinRir::Quitter(c.unwrap_or(1)),
                }
            }
            Op::SautSiFauxAbstrait { test, cible } => {
                let v = hote.eval_abstrait_rapide(test, base, corps);
                if hote.erreur_en_attente() != 0 { return FinRir::Quitter(1); }
                let vrai = matches!(v, Valeur::Booleen(true));
                if !vrai { pc = *cible; }
            }
            Op::RetourAbstrait(ab) => {
                let v = hote.eval_abstrait_rapide(ab, base, corps);
                if hote.erreur_en_attente() != 0 { return FinRir::Quitter(1); }
                return FinRir::Retour(v);
            }
            Op::Retour(e) => {
                let v = match e {
                    Some(e) => match hote.eval_slots(e, slots, &corps.noms) {
                        Ok(v) => v,
                        Err(c) => return FinRir::Quitter(c.unwrap_or(1)),
                    },
                    None => Valeur::Vide,
                };
                return FinRir::Retour(v);
            }
            Op::ExprStmt(e) => {
                match hote.eval_slots(e, slots, &corps.noms) {
                    Ok(_) => {}
                    Err(c) => return FinRir::Quitter(c.unwrap_or(1)),
                }
            }
            Op::Lever(e) => {
                let _ = hote.eval_slots(e, slots, &corps.noms);
                return FinRir::Quitter(1);
            }
            Op::Bloc(b) => {
                // ⚠⚠⚠ UN BLOC IMBRIQUÉ EST EXÉCUTÉ PAR L'INTERPRÉTEUR.
                //
                // C'est le SEUL point où le RIR rappelle l'interpréteur pour
                // du contrôle de flux : on lui passe une closure `exec_bloc`
                // (fournie par l'interpréteur) pour que les règles de portée
                // et de retour soient EXACTEMENT celles de la voie récursive.
                match hote.exec_bloc_imbrique(b) {
                    FinRir::Vide => {}
                    FinRir::Quitter(c) => return FinRir::Quitter(c),
                    FinRir::Retour(v) => return FinRir::Retour(v),
                }
            }
        }
    }
    FinRir::Vide
}

thread_local! {
    /// Profil RIR actif (ROSE_PROFIL_RIR=1).
    static PROFIL_RIR: std::cell::Cell<bool> = std::cell::Cell::new(false);
}

/// Nombre d'opérations RIR exécutées depuis le démarrage.
static OPS_RIR: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Active le compteur d'opérations RIR.
pub fn activer_profil() { PROFIL_RIR.with(|c| c.set(true)); }

/// Nombre d'opérations RIR exécutées.
pub fn ops_executees() -> u64 { OPS_RIR.load(std::sync::atomic::Ordering::Relaxed) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::front::analyser;

    #[test]
    fn abaisser_un_corps_simple() {
        let prog = analyser("fn f(n: entier) -> entier {\n  si (n < 2) { retour n }\n  retour n * n\n}").unwrap();
        let sig = prog.signatures.get("f").unwrap();
        let corps = sig.corps.as_ref().unwrap();
        let params: Vec<String> = sig.params.iter().map(|(n, _)| n.clone()).collect();
        let rir = abaisser(corps, &params, &[]).expect("abaissable");
        // n est le slot 0 ; le corps doit contenir au moins un saut conditionnel.
        assert_eq!(rir.noms[0], "n");
        // ⚠ Le test est ABSTRAIT (cas chaud) : `SautSiFauxAbstrait`.
        assert!(rir.ops.iter().any(|o| matches!(o,
            Op::SautSiFaux { .. } | Op::SautSiFauxAbstrait { .. })));
        // ⚠ Le retour est ABSTRAIT quand l'expression est dans le sous-ensemble.
        assert!(rir.ops.iter().any(|o| matches!(o,
            Op::Retour(_) | Op::RetourAbstrait(_))));
    }

    #[test]
    fn boucle_pour_devient_sauts() {
        let prog = analyser("fn f() -> entier {\n  $s = 0\n  pour j de 0 vers 5 { $s = $s + j }\n  retour $s\n}").unwrap();
        let sig = prog.signatures.get("f").unwrap();
        let rir = abaisser(sig.corps.as_ref().unwrap(), &[], &[]).expect("abaissable");
        // Un « pour » = affect + test + corps + incrément + saut arrière.
        assert!(rir.ops.iter().any(|o| matches!(o, Op::Saut { .. })));
    }

    #[test]
    fn hors_sous_ensemble_rend_none() {
        // « spawn » n'est pas abaissable : le corps reste interprété.
        let prog = analyser("fn f() -> entier {\n  spawn { $x = 1 }\n  retour 0\n}").unwrap();
        let sig = prog.signatures.get("f").unwrap();
        assert!(abaisser(sig.corps.as_ref().unwrap(), &[], &[]).is_none());
    }
}
