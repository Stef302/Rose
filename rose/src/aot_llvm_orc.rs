// ═══════════════════════════════════════════════════════════════════════════
// Rose — AOT LLVM ORC (On-Request Compilation) via inkwell
//
// Compilation LLVM ORC in-process (pas de subprocess C/gcc) :
//   • Génération d'IR LLVM depuis des expressions Rose entières
//   • Émission de fichier objet natif ou IR texte
//   • Support des niveaux -O1 / -O2 / -O3
//   • Phi nodes pour les boucles pour / tant_que
//
// Usage : rose orc prog.rose [-o sortie] [-O1|-O2|-O3]
//
// ═══════════════════════════════════════════════════════════════════════════

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::process::Command;

use inkwell::context::Context;
use inkwell::module::{Module, Linkage};
use inkwell::builder::Builder;
use inkwell::types::BasicMetadataTypeEnum;
use inkwell::values::{BasicValueEnum, BasicMetadataValueEnum};
use inkwell::IntPredicate;
use inkwell::OptimizationLevel;
use inkwell::targets::{
    InitializationConfig, Target, TargetMachine, TargetTriple, CodeModel, RelocMode, FileType,
};

use crate::front::{Expr, Instr, Programme};

/// Niveaux d'optimisation supportés par LLVM ORC
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OrcOptLevel {
    O1,
    O2,
    O3,
}

impl OrcOptLevel {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "O1" | "o1" | "1" => Some(OrcOptLevel::O1),
            "O2" | "o2" | "2" => Some(OrcOptLevel::O2),
            "O3" | "o3" | "3" => Some(OrcOptLevel::O3),
            _ => None,
        }
    }

    pub fn to_inkwell(self) -> OptimizationLevel {
        match self {
            OrcOptLevel::O1 => OptimizationLevel::Less,
            OrcOptLevel::O2 => OptimizationLevel::Default,
            OrcOptLevel::O3 => OptimizationLevel::Aggressive,
        }
    }
}

/// Mode PGO (Profile-Guided Optimization)
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum PgoMode {
    #[default]
    None,
    Generate,  // -fprofile-generate : instrumentation pour collecte
    Use,       // -fprofile-use : optimisation guidée par les profils collectés
}

impl PgoMode {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "generate" | "gen" => Some(PgoMode::Generate),
            "use" => Some(PgoMode::Use),
            _ => None,
        }
    }
}

/// Point d'entrée CLI pour la commande `orc`
pub fn run_orc(fichier: &str, sortie: &str, opt_level: OrcOptLevel, pgo_mode: PgoMode, target_triple: Option<&str>) -> Result<(), String> {
    // Lire et analyser le source Rose
    let mut source = String::new();
    File::open(fichier)
        .map_err(|e| format!("impossible de lire {} : {}", fichier, e))?
        .read_to_string(&mut source)
        .map_err(|e| format!("impossible de lire {} : {}", fichier, e))?;

    let prog = crate::front::analyser(&source)
        .map_err(|e| format!("erreur d'analyse : {}", e))?;

    // Vérifier le programme
    crate::front::verifier(&mut prog.clone())
        .map_err(|e| format!("erreur de vérification : {}", e))?;

    // ── Initialisation LLVM ──────────────────────────────────────────────
    // Il faut initialiser TOUS les composants (targets, MC, asm parsers,
    // asm printers) pour que TargetMachine puisse émettre des fichiers
    // objets natifs (.o) et de l'IR LLVM.
    Target::initialize_all(&InitializationConfig {
        base: true,
        info: true,
        machine_code: true,
        asm_parser: true,
        asm_printer: true,
        disassembler: true,
    });

    // Initialisation des passes LLVM nécessaires à l'émission d'objet natif
    // (déclarée après la création du module, plus bas dans la fonction)

    let context = Context::create();
    let module = context.create_module("rose_orc");
    let builder = context.create_builder();
    let i64_type = context.i64_type();
    let fn_type = i64_type.fn_type(&[], false);
    let function = module.add_function("main", fn_type, Some(Linkage::External));
    let entry_main = context.append_basic_block(function, "entry");
    builder.position_at_end(entry_main);

    // Note : l'initialisation des passes LLVM n'est pas nécessaire pour
    // l'émission d'objet natif avec TargetMachine::write_to_file

    let triple = if let Some(t) = target_triple {
        TargetTriple::create(t)
    } else {
        TargetMachine::get_default_triple()
    };
    let target = Target::from_triple(&triple)
        .map_err(|e| format!("échec résolution cible : {:?}", e))?;
    let target_machine = target.create_target_machine(
        &triple,
        "",
        "",
        opt_level.to_inkwell(),
        RelocMode::Default,
        CodeModel::Default,
    )
    .ok_or("échec de création de TargetMachine")?;

    // Table des valeurs nommées (variables)
    let mut named_values: HashMap<String, BasicValueEnum> = HashMap::new();

    // ── PGO (Profile-Guided Optimization) ─────────────────────────────────
    // Le PGO complet avec LLVM ORC in-process nécessite des passes
    // d'instrumentation LLVM qui ne sont pas exposées par l'API inkwell 0.7.
    // Pour utiliser le PGO :
    //   1. --pgo-generate : compiler avec l'instrumentation (à implémenter
    //      quand l'API le permettra)
    //   2. Exécuter le binaire généré pour produire un .profraw
    //   3. llvm-profdata merge -o prof.profdata raw.profraw
    //   4. --pgo-use : recompiler avec le .profdata pour l'optimisation guidée
    match pgo_mode {
        PgoMode::Generate => {
            println!("rose: --pgo-generate demandé. Pour le PGO complet :");
            println!("  1. Compiler avec instrumentation (à implémenter)");
            println!("  2. Exécuter le binaire pour générer un .profraw");
            println!("  3. llvm-profdata merge -o prof.profdata raw.profraw");
            println!("  4. Recompiler avec --pgo-use prof.profdata");
        }
        PgoMode::Use => {
            println!("rose: --pgo-use demandé. Fournir un .profdata pour l'optimisation guidée.");
        }
        PgoMode::None => {}
    }

    // ── Fonctions utilisateur ────────────────────────────────────────────
    // Deux passes : (1) toutes les en-têtes — les appels croisés et la
    // récursion trouvent déjà la fonction déclarée dans le module ;
    // (2) les corps. Un builder séparé protège le positionnement du builder
    // principal (resté sur l'entrée de main). Les noms sont figés avant
    // d'itérer. Une fonction déclarée sans corps reste extérieure : c'est ce
    // qui permet aux appels de DLL « extern » de linker contre la libc.
    for nom in &prog.ordre {
        let sig = match prog.signatures.get(nom) { Some(s) => s, None => continue };
        if sig.corps.is_none() { continue; }
        let flott = matches!(sig.retour.n, crate::front::TyKind::Flottant | crate::front::TyKind::Rationnel);
        let mut params: Vec<BasicMetadataTypeEnum> = Vec::new();
        for (_, t) in &sig.params {
            params.push(if matches!(t.n, crate::front::TyKind::Flottant | crate::front::TyKind::Rationnel) {
                context.f64_type().into()
            } else {
                i64_type.into()
            });
        }
        let fn_type = if flott {
            context.f64_type().fn_type(&params, false)
        } else {
            i64_type.fn_type(&params, false)
        };
        module.add_function(nom, fn_type, Some(Linkage::External));
    }
    {
        let fb = context.create_builder();
        let noms: Vec<String> = prog.ordre.iter()
            .filter(|n| prog.signatures.get(*n).map(|s| s.corps.is_some()).unwrap_or(false))
            .cloned()
            .collect();
        for nom in noms {
            let sig = prog.signatures.get(&nom).unwrap();
            let f = module.get_function(&nom).unwrap();
            let corps = sig.corps.clone().unwrap();
            let a_retour = sig.a_retour;
            let flott = matches!(sig.retour.n, crate::front::TyKind::Flottant | crate::front::TyKind::Rationnel);
            let entry = context.append_basic_block(f, "entry");
            fb.position_at_end(entry);
            let mut local: HashMap<String, BasicValueEnum> = HashMap::new();
            for (i, (pn, _)) in sig.params.iter().enumerate() {
                local.insert(pn.clone(), f.get_nth_param(i as u32).unwrap());
            }
            let mut erreur: Option<String> = None;
            for instr in &corps {
                if let Err(m) = compile_instr(&fb, &context, &module, &mut local, instr) {
                    erreur = Some(m);
                    break;
                }
            }
            if let Some(m) = erreur {
                // Corps hors du domaine supporté : on supprime la définition,
                // l'appel redevient une fonction extérieure (libc / DLL).
                unsafe { f.delete() };
                return Err(format!("dans la fonction '{}' : {}", nom, m));
            }
            let termine = fb.get_insert_block().map(|b| b.get_terminator().is_some()).unwrap_or(true);
            if !termine {
                if a_retour {
                    let def: BasicValueEnum = if flott { context.f64_type().const_zero().into() } else { i64_type.const_int(0, false).into() };
                    fb.build_return(Some(&def));
                } else if flott {
                    fb.build_return(Some(&context.f64_type().const_zero()));
                } else {
                    fb.build_return(None);
                }
            }
        }
    }

    // Fonction récursive interne pour compiler une expression
    // Type LLVM d'un complexe (a+bi) : structure de deux f64.
    fn complex_type<'ctx>(context: &'ctx Context) -> inkwell::types::StructType<'ctx> {
        context.struct_type(
            &[context.f64_type().into(), context.f64_type().into()],
            false,
        )
    }

    fn compile_expr<'ctx>(
        builder: &Builder<'ctx>,
        context: &'ctx Context,
        module: &Module<'ctx>,
        named_values: &mut HashMap<String, BasicValueEnum<'ctx>>,
        expr: &Expr,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        match expr {
            Expr::Litteral { texte, flott, neg, .. } => {
                if *flott {
                    let s = texte.trim_start_matches('-');
                    let v: f64 = s.parse().map_err(|e| format!("littéral flottant invalide '{}': {}", texte, e))?;
                    let val = if *neg { -v } else { v };
                    Ok(context.f64_type().const_float(val).into())
                } else {
                    let s = texte.trim_start_matches('-');
                    let v: i64 = s.parse().map_err(|e| format!("littéral invalide '{}': {}", texte, e))?;
                    let val = if *neg { -(v as i64) } else { v };
                    Ok(context.i64_type().const_int(val as u64, false).into())
                }
            }
            Expr::LitComplexe { re, im, .. } => {
                let re_v = context.f64_type().const_float(*re);
                let im_v = context.f64_type().const_float(*im);
                Ok(context.const_struct(&[re_v.into(), im_v.into()], false).into())
            }
            Expr::LitComplexe { re, im, .. } => {
                let re_v = context.f64_type().const_float(*re);
                let im_v = context.f64_type().const_float(*im);
                Ok(context.const_struct(&[re_v.into(), im_v.into()], false).into())
            }
            Expr::BooleenLit(b, _) => {
                Ok(context.bool_type().const_int(if *b { 1 } else { 0 }, false).into())
            }
            Expr::Var(n, _) => {
                named_values.get(n)
                    .cloned()
                    .ok_or_else(|| format!("variable inconnue : {}", n))
            }
            Expr::Unaire { op, a, .. } => {
                let operand = compile_expr(builder, context, module, named_values, a)?;
                match op.as_str() {
                    "-" => {
                        if operand.is_float_value() {
                            let v = operand.into_float_value();
                            Ok(builder.build_float_neg(v, "negf")
                                .map_err(|e| format!("erreur build_float_neg : {:?}", e))?
                                .into())
                        } else if operand.is_struct_value() {
                            let sv = operand.into_struct_value();
                            let re = builder.build_extract_value(sv, 0, "re").map_err(|e| format!("{:?}", e))?.into_float_value();
                            let im = builder.build_extract_value(sv, 1, "im").map_err(|e| format!("{:?}", e))?.into_float_value();
                            let neg_re = builder.build_float_neg(re, "negre").map_err(|e| format!("{:?}", e))?;
                            let neg_im = builder.build_float_neg(im, "negim").map_err(|e| format!("{:?}", e))?;
                            Ok(context.const_struct(&[neg_re.into(), neg_im.into()], false).into())
                        } else {
                            Ok(builder.build_int_neg(operand.into_int_value(), "neg")
                                .map_err(|e| format!("erreur build_int_neg : {:?}", e))?
                                .into())
                        }
                    }
                    "non" => {
                        let one = context.bool_type().const_int(1, false);
                        let result = builder.build_xor(operand.into_int_value(), one.into(), "non")
                            .map_err(|e| format!("erreur build_xor : {:?}", e))?;
                        Ok(result.into())
                    }
                    _ => Err(format!("opérateur unaire non supporté en ORC : {}", op)),
                }
            }
            Expr::Binaire { op, a, b, .. } => {
                let left = compile_expr(builder, context, module, named_values, a)?;
                let right = compile_expr(builder, context, module, named_values, b)?;
                let lc = left.is_struct_value();
                let rc = right.is_struct_value();
                let lf = left.is_float_value();
                let rf = right.is_float_value();
                if lc || rc {
                    // complexe : au moins un opérande est complexe
                    let (lr, li) = if lc {
                        let sv = left.into_struct_value();
                        (builder.build_extract_value(sv, 0, "lr").map_err(|e| format!("{:?}", e))?.into_float_value(),
                         builder.build_extract_value(sv, 1, "li").map_err(|e| format!("{:?}", e))?.into_float_value())
                    } else {
                        let x = left.into_float_value();
                        (x, context.f64_type().const_float(0.0))
                    };
                    let (rr, ri) = if rc {
                        let sv = right.into_struct_value();
                        (builder.build_extract_value(sv, 0, "rr").map_err(|e| format!("{:?}", e))?.into_float_value(),
                         builder.build_extract_value(sv, 1, "ri").map_err(|e| format!("{:?}", e))?.into_float_value())
                    } else {
                        let x = right.into_float_value();
                        (x, context.f64_type().const_float(0.0))
                    };
                    return match op.as_str() {
                        "+" => {
                            let re = builder.build_float_add(lr, rr, "cre").map_err(|e| format!("{:?}", e))?;
                            let im = builder.build_float_add(li, ri, "cim").map_err(|e| format!("{:?}", e))?;
                            Ok(context.const_struct(&[re.into(), im.into()], false).into())
                        }
                        "-" => {
                            let re = builder.build_float_sub(lr, rr, "cre").map_err(|e| format!("{:?}", e))?;
                            let im = builder.build_float_sub(li, ri, "cim").map_err(|e| format!("{:?}", e))?;
                            Ok(context.const_struct(&[re.into(), im.into()], false).into())
                        }
                        "*" => {
                            // formule générale : re = lr*rr - li*ri ; im = lr*ri + li*rr
                            // (même arrondi que la « voie rapide » du moteur :
                            //  le JIT fait de même.)
                            let t1 = builder.build_float_mul(lr, rr, "t1").map_err(|e| format!("{:?}", e))?;
                            let t2 = builder.build_float_mul(li, ri, "t2").map_err(|e| format!("{:?}", e))?;
                            let re = builder.build_float_sub(t1, t2, "cre").map_err(|e| format!("{:?}", e))?;
                            let t3 = builder.build_float_mul(lr, ri, "t3").map_err(|e| format!("{:?}", e))?;
                            let t4 = builder.build_float_mul(li, rr, "t4").map_err(|e| format!("{:?}", e))?;
                            let im = builder.build_float_add(t3, t4, "cim").map_err(|e| format!("{:?}", e))?;
                            Ok(context.const_struct(&[re.into(), im.into()], false).into())
                        }
                        "/" => {
                            let denom = builder.build_float_add(
                                builder.build_float_mul(rr, rr, "d1").map_err(|e| format!("{:?}", e))?,
                                builder.build_float_mul(ri, ri, "d2").map_err(|e| format!("{:?}", e))?,
                                "denom").map_err(|e| format!("{:?}", e))?;
                            let re = builder.build_float_div(
                                builder.build_float_add(
                                    builder.build_float_mul(lr, rr, "n1").map_err(|e| format!("{:?}", e))?,
                                    builder.build_float_mul(li, ri, "n2").map_err(|e| format!("{:?}", e))?,
                                    "rn").map_err(|e| format!("{:?}", e))?,
                                denom, "re").map_err(|e| format!("{:?}", e))?;
                            let im = builder.build_float_div(
                                builder.build_float_sub(
                                    builder.build_float_mul(li, rr, "n3").map_err(|e| format!("{:?}", e))?,
                                    builder.build_float_mul(lr, ri, "n4").map_err(|e| format!("{:?}", e))?,
                                    "in").map_err(|e| format!("{:?}", e))?,
                                denom, "im").map_err(|e| format!("{:?}", e))?;
                            Ok(context.const_struct(&[re.into(), im.into()], false).into())
                        }
                        _ => Err(format!("opérateur binaire complexe non supporté en ORC : {}", op)),
                    };
                }
                if lf || rf {
                    // au moins un flottant : promotion de l'entier vers f64
                    let lv = if lf { left.into_float_value() }
                        else { builder.build_signed_int_to_float(left.into_int_value(), context.f64_type(), "itof")
                            .map_err(|e| format!("{:?}", e))? };
                    let rv = if rf { right.into_float_value() }
                        else { builder.build_signed_int_to_float(right.into_int_value(), context.f64_type(), "itof")
                            .map_err(|e| format!("{:?}", e))? };
                    return match op.as_str() {
                        "+" => Ok(builder.build_float_add(lv, rv, "fadd").map_err(|e| format!("{:?}", e))?.into()),
                        "-" => Ok(builder.build_float_sub(lv, rv, "fsub").map_err(|e| format!("{:?}", e))?.into()),
                        "*" => Ok(builder.build_float_mul(lv, rv, "fmul").map_err(|e| format!("{:?}", e))?.into()),
                        "/" => Ok(builder.build_float_div(lv, rv, "fdiv").map_err(|e| format!("{:?}", e))?.into()),
                        "<" => {
                            let cmp = builder.build_float_compare(inkwell::FloatPredicate::OLT, lv, rv, "flt").map_err(|e| format!("{:?}", e))?;
                            Ok(builder.build_int_z_extend(cmp, context.i64_type(), "flt_i64").map_err(|e| format!("{:?}", e))?.into())
                        }
                        "<=" => {
                            let cmp = builder.build_float_compare(inkwell::FloatPredicate::OLE, lv, rv, "fle").map_err(|e| format!("{:?}", e))?;
                            Ok(builder.build_int_z_extend(cmp, context.i64_type(), "fle_i64").map_err(|e| format!("{:?}", e))?.into())
                        }
                        ">" => {
                            let cmp = builder.build_float_compare(inkwell::FloatPredicate::OGT, lv, rv, "fgt").map_err(|e| format!("{:?}", e))?;
                            Ok(builder.build_int_z_extend(cmp, context.i64_type(), "fgt_i64").map_err(|e| format!("{:?}", e))?.into())
                        }
                        ">=" => {
                            let cmp = builder.build_float_compare(inkwell::FloatPredicate::OGE, lv, rv, "fge").map_err(|e| format!("{:?}", e))?;
                            Ok(builder.build_int_z_extend(cmp, context.i64_type(), "fge_i64").map_err(|e| format!("{:?}", e))?.into())
                        }
                        "==" => {
                            let cmp = builder.build_float_compare(inkwell::FloatPredicate::OEQ, lv, rv, "feq").map_err(|e| format!("{:?}", e))?;
                            Ok(builder.build_int_z_extend(cmp, context.i64_type(), "feq_i64").map_err(|e| format!("{:?}", e))?.into())
                        }
                        "!=" => {
                            let cmp = builder.build_float_compare(inkwell::FloatPredicate::ONE, lv, rv, "fne").map_err(|e| format!("{:?}", e))?;
                            Ok(builder.build_int_z_extend(cmp, context.i64_type(), "fne_i64").map_err(|e| format!("{:?}", e))?.into())
                        }
                        _ => Err(format!("opérateur binaire flottant non supporté en ORC : {}", op)),
                    };
                }
                // entiers (comportement original)
                match op.as_str() {
                    "+" => Ok(builder.build_int_add(left.into_int_value(), right.into_int_value(), "add")
                        .map_err(|e| format!("erreur build_int_add : {:?}", e))?
                        .into()),
                    "-" => Ok(builder.build_int_sub(left.into_int_value(), right.into_int_value(), "sub")
                        .map_err(|e| format!("erreur build_int_sub : {:?}", e))?
                        .into()),
                    "*" => Ok(builder.build_int_mul(left.into_int_value(), right.into_int_value(), "mul")
                        .map_err(|e| format!("erreur build_int_mul : {:?}", e))?
                        .into()),
                    "/" => Ok(builder.build_int_signed_div(left.into_int_value(), right.into_int_value(), "div")
                        .map_err(|e| format!("erreur build_int_signed_div : {:?}", e))?
                        .into()),
                    "%" => Ok(builder.build_int_signed_rem(left.into_int_value(), right.into_int_value(), "rem")
                        .map_err(|e| format!("erreur build_int_signed_rem : {:?}", e))?
                        .into()),
                    "==" => {
                        let cmp = builder.build_int_compare(IntPredicate::EQ, left.into_int_value(), right.into_int_value(), "eq")
                            .map_err(|e| format!("erreur build_int_compare : {:?}", e))?;
                        Ok(builder.build_int_z_extend(cmp, context.i64_type(), "eq_i64")
                            .map_err(|e| format!("erreur build_int_z_extend : {:?}", e))?
                            .into())
                    }
                    "!=" => {
                        let cmp = builder.build_int_compare(IntPredicate::NE, left.into_int_value(), right.into_int_value(), "ne")
                            .map_err(|e| format!("erreur build_int_compare : {:?}", e))?;
                        Ok(builder.build_int_z_extend(cmp, context.i64_type(), "ne_i64")
                            .map_err(|e| format!("erreur build_int_z_extend : {:?}", e))?
                            .into())
                    }
                    "<" => {
                        let cmp = builder.build_int_compare(IntPredicate::SLT, left.into_int_value(), right.into_int_value(), "lt")
                            .map_err(|e| format!("erreur build_int_compare : {:?}", e))?;
                        Ok(builder.build_int_z_extend(cmp, context.i64_type(), "lt_i64")
                            .map_err(|e| format!("erreur build_int_compare : {:?}", e))?
                            .into())
                    }
                    "<=" => {
                        let cmp = builder.build_int_compare(IntPredicate::SLE, left.into_int_value(), right.into_int_value(), "le")
                            .map_err(|e| format!("erreur build_int_compare : {:?}", e))?;
                        Ok(builder.build_int_z_extend(cmp, context.i64_type(), "le_i64")
                            .map_err(|e| format!("erreur build_int_compare : {:?}", e))?
                            .into())
                    }
                    ">" => {
                        let cmp = builder.build_int_compare(IntPredicate::SGT, left.into_int_value(), right.into_int_value(), "gt")
                            .map_err(|e| format!("erreur build_int_compare : {:?}", e))?;
                        Ok(builder.build_int_z_extend(cmp, context.i64_type(), "gt_i64")
                            .map_err(|e| format!("erreur build_int_compare : {:?}", e))?
                            .into())
                    }
                    ">=" => {
                        let cmp = builder.build_int_compare(IntPredicate::SGE, left.into_int_value(), right.into_int_value(), "ge")
                            .map_err(|e| format!("erreur build_int_compare : {:?}", e))?;
                        Ok(builder.build_int_z_extend(cmp, context.i64_type(), "ge_i64")
                            .map_err(|e| format!("erreur build_int_compare : {:?}", e))?
                            .into())
                    }
                    "et" => Ok(builder.build_and(left.into_int_value(), right.into_int_value(), "and")
                        .map_err(|e| format!("erreur build_and : {:?}", e))?
                        .into()),
                    "ou" => Ok(builder.build_or(left.into_int_value(), right.into_int_value(), "or")
                        .map_err(|e| format!("erreur build_and : {:?}", e))?
                        .into()),
                    _ => Err(format!("opérateur binaire non supporté en ORC : {}", op)),
                }
            }
            Expr::Appel { nom, args, .. } => {
                // Intrinsics complexes. Parité avec l'interpréteur
                // (interp.rs « re » : `match Complexe{re,..} => re, _ =>
                // vers_d(x)`) et le JIT (jit.rs) : un réel passe — re(x) =
                // x, im(x) = 0.0, conj(x) = x. Seuls ces trois-là admettent
                // un réel ; modulus d'un non-complexe reste hors
                // sous-ensemble ORC (repli interprète).
                if nom == "re" && args.len() == 1 {
                    let v = compile_expr(builder, context, module, named_values, &args[0])?;
                    if v.is_struct_value() {
                        let sv = v.into_struct_value();
                        return Ok(builder.build_extract_value(sv, 0, "re").map_err(|e| format!("{:?}", e))?.into());
                    }
                    if v.is_float_value() { return Ok(v); }
                    if v.is_int_value() {
                        return Ok(builder.build_signed_int_to_float(v.into_int_value(), context.f64_type(), "re_f")
                            .map_err(|e| format!("{:?}", e))?.into());
                    }
                    return Err(format!("re() : type non supporté en ORC"));
                }
                if nom == "im" && args.len() == 1 {
                    let v = compile_expr(builder, context, module, named_values, &args[0])?;
                    if v.is_struct_value() {
                        let sv = v.into_struct_value();
                        return Ok(builder.build_extract_value(sv, 1, "im").map_err(|e| format!("{:?}", e))?.into());
                    }
                    // im(réel) = 0.0 (miroir interp/jit)
                    return Ok(context.f64_type().const_zero().into());
                }
                if nom == "conj" && args.len() == 1 {
                    let v = compile_expr(builder, context, module, named_values, &args[0])?;
                    if !v.is_struct_value() {
                        // conj(réel) = le réel lui-même (miroir interp/jit)
                        if v.is_float_value() || v.is_int_value() { return Ok(v); }
                        return Err(format!("conj() : type non supporté en ORC"));
                    }
                    let sv = v.into_struct_value();
                    let re = builder.build_extract_value(sv, 0, "re").map_err(|e| format!("{:?}", e))?.into_float_value();
                    let im = builder.build_extract_value(sv, 1, "im").map_err(|e| format!("{:?}", e))?.into_float_value();
                    let neg_im = builder.build_float_neg(im, "negim").map_err(|e| format!("{:?}", e))?;
                    return Ok(context.const_struct(&[re.into(), neg_im.into()], false).into());
                }
                if nom == "modulus" && args.len() == 1 {
                    let v = compile_expr(builder, context, module, named_values, &args[0])?;
                    // Parité interpréteur : modulus(réel) = |x| (fabs),
                    // modulus(complexe) = √(re²+im²).
                    if v.is_float_value() {
                        let f = v.into_float_value();
                        let fabs = module.get_function("fabs").unwrap_or_else(|| {
                            let ft = context.f64_type().fn_type(&[context.f64_type().into()], false);
                            module.add_function("fabs", ft, Some(Linkage::External))
                        });
                        let call = builder.build_call(fabs, &[f.into()], "fabs").map_err(|e| format!("{:?}", e))?;
                        return Ok(call.try_as_basic_value().unwrap_basic().into());
                    }
                    if v.is_int_value() {
                        let vi = v.into_int_value();
                        let zero = context.i64_type().const_zero();
                        let neg = builder.build_int_compare(IntPredicate::SLT, vi, zero, "neg_c")
                            .map_err(|e| format!("{:?}", e))?;
                        let nvi = builder.build_int_neg(vi, "negi").map_err(|e| format!("{:?}", e))?;
                        let abs_i = builder.build_select(neg, nvi, vi, "absi")
                            .map_err(|e| format!("{:?}", e))?
                            .into_int_value();
                        return Ok(builder.build_signed_int_to_float(abs_i, context.f64_type(), "abs_f")
                            .map_err(|e| format!("{:?}", e))?.into());
                    }
                    if !v.is_struct_value() {
                        return Err(format!("modulus() : type non supporté en ORC"));
                    }
                    let sv = v.into_struct_value();
                    let re = builder.build_extract_value(sv, 0, "re").map_err(|e| format!("{:?}", e))?.into_float_value();
                    let im = builder.build_extract_value(sv, 1, "im").map_err(|e| format!("{:?}", e))?.into_float_value();
                    let r2 = builder.build_float_mul(re, re, "r2").map_err(|e| format!("{:?}", e))?;
                    let i2 = builder.build_float_mul(im, im, "i2").map_err(|e| format!("{:?}", e))?;
                    let sum = builder.build_float_add(r2, i2, "sum").map_err(|e| format!("{:?}", e))?;
                    // sqrt via libm
                    let sqrt_fn = module.get_function("sqrt").unwrap_or_else(|| {
                        let ft = context.f64_type().fn_type(&[context.f64_type().into()], false);
                        module.add_function("sqrt", ft, Some(Linkage::External))
                    });
                    let call = builder.build_call(sqrt_fn, &[sum.into()], "sqrt").map_err(|e| format!("{:?}", e))?;
                    return Ok(call.try_as_basic_value().unwrap_basic().into());
                }
                // constantes irrationnelles → pas en ORC (besoin de MPI)
                if nom == "pi" || nom == "phi" || nom == "e" {
                    return Err(format!("constante irrationnelle '{}' non supportée en ORC", nom));
                }
                // fonctions mathématiques libm
                if nom == "racine" && args.len() == 1 {
                    let v = compile_expr(builder, context, module, named_values, &args[0])?;
                    // sqrt ne connaît que les flottants : un complexe ou un
                    // entier hors sous-ensemble → repli (pas un appel
                    // mal typé vers libm).
                    if !v.is_float_value() {
                        return Err(format!("racine() attend un flottant en ORC"));
                    }
                    let sqrt_fn = module.get_function("sqrt").unwrap_or_else(|| {
                        let ft = context.f64_type().fn_type(&[context.f64_type().into()], false);
                        module.add_function("sqrt", ft, Some(Linkage::External))
                    });
                    let call = builder.build_call(sqrt_fn, &[v.into()], "sqrt").map_err(|e| format!("{:?}", e))?;
                    return Ok(call.try_as_basic_value().unwrap_basic().into());
                }
                // appel de fonction utilisateur ou externe
                let mut arg_values: Vec<BasicMetadataValueEnum> = Vec::new();
                for arg in args {
                    let v = compile_expr(builder, context, module, named_values, arg)?;
                    arg_values.push(v.into());
                }
                let fonction = match module.get_function(nom) {
                    Some(f) => f,
                    None => {
                        // fonction externe inconnue : on suppose un retour entier
                        // (le linker résoudra le symbole, comme pour les DLL).
                        let fn_type = context.i64_type().fn_type(
                            &vec![BasicMetadataTypeEnum::IntType(context.i64_type()); args.len()],
                            false,
                        );
                        module.add_function(nom, fn_type, Some(Linkage::External))
                    }
                };
                // type de retour : lu depuis la signature de la fonction dans
                // le module (entier, flottant ou complexe).
                let _ret_ty = fonction.get_type().get_return_type();
                let site = builder.build_call(fonction, &arg_values, "call")
                    .map_err(|e| format!("erreur build_call : {:?}", e))?;
                match site.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => Ok(v),
                    _ => Ok(context.i64_type().const_int(0, false).into()),
                }
            }
            Expr::Unaire { op, a, .. } => {
                let operand = compile_expr(builder, context, module, named_values, a)?;
                match op.as_str() {
                    "-" => {
                        if operand.is_float_value() {
                            let v = operand.into_float_value();
                            return Ok(builder.build_float_neg(v, "negf")
                                .map_err(|e| format!("erreur build_float_neg : {:?}", e))?
                                .into());
                        }
                        if operand.is_struct_value() {
                            let sv = operand.into_struct_value();
                            let re = builder.build_extract_value(sv, 0, "re").map_err(|e| format!("{:?}", e))?.into_float_value();
                            let im = builder.build_extract_value(sv, 1, "im").map_err(|e| format!("{:?}", e))?.into_float_value();
                            let neg_re = builder.build_float_neg(re, "negre").map_err(|e| format!("{:?}", e))?;
                            let neg_im = builder.build_float_neg(im, "negim").map_err(|e| format!("{:?}", e))?;
                            return Ok(context.const_struct(&[neg_re.into(), neg_im.into()], false).into());
                        }
                        Ok(builder.build_int_neg(operand.into_int_value(), "neg")
                            .map_err(|e| format!("erreur build_int_neg : {:?}", e))?
                            .into())
                    }
                    "non" => {
                        let one = context.bool_type().const_int(1, false);
                        let result = builder.build_xor(operand.into_int_value(), one.into(), "non")
                            .map_err(|e| format!("erreur build_xor : {:?}", e))?;
                        Ok(result.into())
                    }
                    _ => Err(format!("opérateur unaire non supporté en ORC : {}", op)),
                }
            }
            Expr::Factorielle(a, ..) => {
                let _ = compile_expr(builder, context, module, named_values, a)?;
                Err("factorielle non implémentée en ORC pour l'instant".to_string())
            }
            _ => Err(format!("expression non supportée en ORC : {:?}", expr)),
        }
    }
    // Fonction récursive interne pour compiler une instruction
    fn compile_instr<'ctx>(
        builder: &Builder<'ctx>,
        context: &'ctx Context,
        module: &Module<'ctx>,
        named_values: &mut HashMap<String, BasicValueEnum<'ctx>>,
        instr: &Instr,
    ) -> Result<(), String> {
        match instr {
            Instr::Affect { nom, e, .. } => {
                let value = compile_expr(builder, context, module, named_values, e)?;
                // Si la variable est un phi de boucle, l'affectation ajoute
                // l'entrée du phi depuis le bloc courant (sinon les usages
                // suivants liraient la valeur morte). Si on émet dans le bloc
                // même du phi, un bloc de continuation est créé.
                use inkwell::values::{BasicValue as _, AsValueRef};
                if let Some(iv) = named_values.get(nom).map(|v| v.as_instruction_value()).flatten() {
                    if iv.get_opcode() == inkwell::values::InstructionOpcode::Phi {
                        let bloc_phi = iv.get_parent().unwrap();
                        let f = bloc_phi.get_parent().unwrap();
                        let courant = builder.get_insert_block().unwrap();
                        let bloc = if courant == bloc_phi {
                            let suite = context.append_basic_block(f, "cont");
                            let _ = builder.build_unconditional_branch(suite);
                            builder.position_at_end(suite);
                            suite
                        } else {
                            courant
                        };
                        let phi = unsafe { inkwell::values::PhiValue::new(iv.as_value_ref()) };
                        phi.add_incoming(&[(&value, bloc)]);
                        // La table garde la VALEUR DU PHI tant que les deux
                        // entrées ont le même type (le phi est le point de
                        // convergence, seul valable après la boucle). Si
                        // l'affectation change le type de la variable
                        // (compteur i64 devenu complexe, accumulateur float
                        // devenu complexe…), le phi ne peut plus représenter
                        // la variable : on garde la dernière valeur, qui
                        // porte le type courant — les lectures dans le corps
                        // ont déjà vu le phi ou cette valeur.
                        if phi.as_basic_value().get_type() == value.get_type() {
                            return Ok(());
                        }
                        named_values.insert(nom.clone(), value);
                        return Ok(());
                    }
                }
                named_values.insert(nom.clone(), value);
                Ok(())
            }
            Instr::AffectIndex { .. } => Err("Affectation par index non supportée par l'AOT LLVM".to_string()),
            Instr::Essayer { .. } => Err("essayer/attraper non supporté par l'AOT LLVM".to_string()),
            Instr::Spawn(_, _) => Err("spawn non supporté par l'AOT LLVM".to_string()),
            Instr::Lever(e, _) => Err("lever non supporté par l'AOT LLVM".to_string()),
            Instr::Afficher(exprs, _) => {
                if exprs.is_empty() { return Ok(()); }
                let e = &exprs[0];
                let value = compile_expr(builder, context, module, named_values, e)?;

                // ── Affichage selon le type (miroir de run::afficher) ──
                if value.is_int_value() {
                    // entier : printf "%lld\n"
                    let i64_type = context.i64_type();
                    let ptr_type = context.ptr_type(inkwell::AddressSpace::default());
                    let printf_func = match module.get_function("printf") {
                        Some(f) => f,
                        None => {
                            let printf_type = i64_type.fn_type(
                                &[BasicMetadataTypeEnum::PointerType(ptr_type.into())],
                                true
                            );
                            module.add_function("printf", printf_type, Some(Linkage::External))
                        }
                    };
                    let fmt_bytes = b"%lld\n\0";
                    let fmt_global = match module.get_global("fmt_orc") {
                        Some(g) => g,
                        None => {
                            let fmt_str = context.const_string(fmt_bytes, false);
                            let g = module.add_global(fmt_str.get_type(), None, "fmt_orc");
                            g.set_linkage(Linkage::Internal);
                            g.set_initializer(&fmt_str);
                            g.set_constant(true);
                            g.set_unnamed_addr(true);
                            g
                        }
                    };
                    let fmt_ptr = builder.build_pointer_cast(
                        fmt_global.as_pointer_value(), ptr_type, "fmt_ptr",
                    ).map_err(|e| format!("erreur build_pointer_cast : {:?}", e))?;
                    let int_val = value.into_int_value();
                    let call_site = builder.build_call(printf_func, &[fmt_ptr.into(), int_val.into()], "printf_call")
                        .map_err(|e| format!("erreur build_call printf : {:?}", e))?;
                    let _ = call_site;
                } else if value.is_float_value() {
                    // flottant : printf "%.16g\n" (miroir du rendu Rose
                    // « 5.0 → 5, 2.5 → 2.5 » pour les valeurs du banc ; le
                    // rendu exact pour les cas exotiques (1e-6 → « 1e-6 »)
                    // est assuré par le moteur natif — l'AOT s'en approche
                    // au caractère près pour les valeurs du banc).
                    let ptr_type = context.ptr_type(inkwell::AddressSpace::default());
                    let printf_func = match module.get_function("printf") {
                        Some(f) => f,
                        None => {
                            let printf_type = context.i64_type().fn_type(
                                &[BasicMetadataTypeEnum::PointerType(ptr_type.into())],
                                true
                            );
                            module.add_function("printf", printf_type, Some(Linkage::External))
                        }
                    };
                    let fmt_bytes = b"%.16g\n\0";
                    let fmt_global = match module.get_global("fmt_f64_orc") {
                        Some(g) => g,
                        None => {
                            let fmt_str = context.const_string(fmt_bytes, false);
                            let g = module.add_global(fmt_str.get_type(), None, "fmt_f64_orc");
                            g.set_linkage(Linkage::Internal);
                            g.set_initializer(&fmt_str);
                            g.set_constant(true);
                            g.set_unnamed_addr(true);
                            g
                        }
                    };
                    let fmt_ptr = builder.build_pointer_cast(
                        fmt_global.as_pointer_value(), ptr_type, "fmt_ptr",
                    ).map_err(|e| format!("erreur build_pointer_cast : {:?}", e))?;
                    let fv = value.into_float_value();
                    builder.build_call(printf_func, &[fmt_ptr.into(), fv.into()], "printf_f64")
                        .map_err(|e| format!("erreur build_call printf_f64 : {:?}", e))?;
                } else if value.is_struct_value() {
                    // complexe : helper Rose (rendu exact : « 3+4i », « 3+i »…)
                    let sv = value.into_struct_value();
                    let re = builder.build_extract_value(sv, 0, "re").map_err(|e| format!("{:?}", e))?.into_float_value();
                    let im = builder.build_extract_value(sv, 1, "im").map_err(|e| format!("{:?}", e))?.into_float_value();
                    let f = module.get_function("rose_orc_affiche_complexe").unwrap_or_else(|| {
                        let ft = context.void_type().fn_type(&[context.f64_type().into(), context.f64_type().into()], false);
                        module.add_function("rose_orc_affiche_complexe", ft, Some(Linkage::External))
                    });
                    builder.build_call(f, &[re.into(), im.into()], "affiche_cplx")
                        .map_err(|e| format!("erreur build_call affiche_cplx : {:?}", e))?;
                } else {
                    return Err("type non supporté pour affichage".to_string());
                }
                Ok(())
            }
            Instr::ExprStmt(e) => {
                let _ = compile_expr(builder, context, module, named_values, e)?;
                Ok(())
            }
            Instr::Si { test, alors, sinon, .. } => {
                let cond = compile_expr(builder, context, module, named_values, test)?;
                let cond_bool = builder.build_int_compare(
                    IntPredicate::NE,
                    cond.into_int_value(),
                    context.i64_type().const_int(0, false),
                    "if_cond",
                ).map_err(|e| format!("erreur build_int_compare : {:?}", e))?;

                let func_courante = builder.get_insert_block().unwrap().get_parent().unwrap();
                let then_block = context.append_basic_block(func_courante, "then");
                let else_block = context.append_basic_block(func_courante, "else");
                let merge_block = context.append_basic_block(func_courante, "merge");

                builder.build_conditional_branch(cond_bool, then_block, else_block);

                builder.position_at_end(then_block);
                for instr in alors {
                    compile_instr(builder, context, module, named_values, instr)?;
                }
                // « retour » dans la branche : le bloc est terminé, ne pas
                // émettre de second terminator (sinon code mort + IR invalide).
                if builder.get_insert_block().map(|b| b.get_terminator().is_none()).unwrap_or(false) {
                    builder.build_unconditional_branch(merge_block);
                }

                builder.position_at_end(else_block);
                if let Some(s) = sinon {
                    for instr in s {
                        compile_instr(builder, context, module, named_values, instr)?;
                    }
                }
                if builder.get_insert_block().map(|b| b.get_terminator().is_none()).unwrap_or(false) {
                    builder.build_unconditional_branch(merge_block);
                }

                builder.position_at_end(merge_block);
                Ok(())
            }
            Instr::TantQue { test, corps, .. } => {
                let func_courante = builder.get_insert_block().unwrap().get_parent().unwrap();
                let loop_block = context.append_basic_block(func_courante, "while_loop");
                let body_block = context.append_basic_block(func_courante, "while_body");
                let after_block = context.append_basic_block(func_courante, "while_after");

                // ── PHI NODES pour toutes les variables nommées ──
                // Structure :
                //   entry -> (saut inconditionnel) -> loop_block
                //   loop_block: phi = phi(val_init, val_corps) ; cond = phi != 0 ; cond ? body : after
                //   body_block: corps ; saut loop_block
                //   after_block: suite

                let pred_block = builder.get_insert_block().unwrap();
                builder.build_unconditional_branch(loop_block);
                builder.position_at_end(loop_block);

                // Créer un phi node pour chaque variable nommée (lues dans le test et modifiées dans le corps)
                let var_list: Vec<(String, BasicValueEnum)> = named_values.iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                let mut phi_nodes: HashMap<String, inkwell::values::PhiValue> = HashMap::new();
                for (var_name, var_val) in var_list {
                    let phi = builder.build_phi(var_val.get_type(), &format!("phi_{}", var_name))
                        .map_err(|e| format!("erreur build_phi : {:?}", e))?;
                    phi.add_incoming(&[(&var_val, pred_block)]);
                    phi_nodes.insert(var_name.clone(), phi);
                    named_values.insert(var_name, phi.as_basic_value());
                }

                // Test de boucle avec les valeurs phi
                let cond = compile_expr(builder, context, module, named_values, test)?;
                let cond_bool = builder.build_int_compare(
                    IntPredicate::NE,
                    cond.into_int_value(),
                    context.i64_type().const_int(0, false),
                    "while_cond",
                ).map_err(|e| format!("erreur build_int_compare : {:?}", e))?;
                builder.build_conditional_branch(cond_bool, body_block, after_block);

                // Corps de la boucle
                builder.position_at_end(body_block);
                for instr in corps {
                    compile_instr(builder, context, module, named_values, instr)?;
                }
                // Ajouter les entrées phi depuis le corps
                let phi_entries: Vec<(String, BasicValueEnum)> = phi_nodes.iter()
                    .map(|(k, _)| (k.clone(), named_values.get(k).cloned().unwrap_or_else(|| {
                        context.i64_type().const_int(0, false).into()
                    })))
                    .collect();
                for (var_name, val) in phi_entries {
                    let phi = phi_nodes.get(&var_name).unwrap();
                    phi.add_incoming(&[(&val, builder.get_insert_block().unwrap())]);
                }
                builder.build_unconditional_branch(loop_block);

                builder.position_at_end(after_block);
                Ok(())
            }
            Instr::Pour { nom, debut, fin, corps, .. } => {
                let start = compile_expr(builder, context, module, named_values, debut)?;

                let func_courante = builder.get_insert_block().unwrap().get_parent().unwrap();
                let loop_block = context.append_basic_block(func_courante, "for_loop");
                let body_block = context.append_basic_block(func_courante, "for_body");
                let after_block = context.append_basic_block(func_courante, "for_after");

                let pred_block = builder.get_insert_block().unwrap();
                // Le compteur `nom` n'est pas encore dans named_values (il
                // est déclaré par le Pour lui-même) : on l'initialise avec
                // `start` pour qu'il participe au phi.
                named_values.insert(nom.clone(), start);
                builder.build_unconditional_branch(loop_block);
                builder.position_at_end(loop_block);

                // PHI pour TOUTES les variables nommées (compteur + accumu-
                // lateurs) : sinon les accumulateurs ($s = $s + k) perdraient
                // leur mise à jour d'une itération à l'autre. Miroir du
                // TantQue qui le fait déjà.
                let var_list: Vec<(String, BasicValueEnum)> = named_values.iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                let mut phi_nodes: HashMap<String, inkwell::values::PhiValue> = HashMap::new();
                for (var_name, var_val) in var_list {
                    // Le compteur de la boucle est un i64 pur. Si la table
                    // des valeurs le tient pour autre chose (déjà float/
                    // structure), ce n'est plus un compteur → hors
                    // sous-ensemble ORC, repli JIT/interprète.
                    // En revanche un phi de boucle est TOUJOURS i64 ici :
                    // la décision se prend à la création du phi, pas après.
                    if &var_name == nom && !var_val.is_int_value() {
                        return Err("compteur de boucle non entier".to_string());
                    }
                    let phi = builder.build_phi(var_val.get_type(), &format!("phi_{}", var_name))
                        .map_err(|e| format!("erreur build_phi : {:?}", e))?;
                    phi.add_incoming(&[(&var_val, pred_block)]);
                    phi_nodes.insert(var_name.clone(), phi);
                    named_values.insert(var_name, phi.as_basic_value());
                }

                let current = match named_values.get(nom).unwrap() { inkwell::values::BasicValueEnum::IntValue(v) => *v, _ => return Err("compteur non entier".to_string()) };
                let end = compile_expr(builder, context, module, named_values, fin)?;
                let cond = builder.build_int_compare(
                    IntPredicate::SLT,
                    current,
                    end.into_int_value(),
                    "for_cond",
                ).map_err(|e| format!("erreur build_int_compare : {:?}", e))?;
                builder.build_conditional_branch(cond, body_block, after_block);

                builder.position_at_end(body_block);
                for instr in corps {
                    compile_instr(builder, context, module, named_values, instr)?;
                }
                // incrémentation du compteur
                let current2 = match named_values.get(nom).unwrap() { inkwell::values::BasicValueEnum::IntValue(v) => *v, _ => return Err("compteur non entier".to_string()) };
                let next = builder.build_int_add(
                    current2,
                    context.i64_type().const_int(1, false),
                    "for_next",
                ).map_err(|e| format!("erreur build_int_add : {:?}", e))?;

                // incrémentation du compteur : on ajoute l'entrée du phi
                // depuis le corps pour le compteur UNIQUEMENT. Les autres
                // variables ont déjà reçu leur entrée via le mécanisme phi
                // de `Affect` (compile_instr, branche Affect) qui ajoute
                // [valeur, body_block] au phi correspondant. Re-ajouter ici
                // créerait un doublon [phi, body_block] qui corromprait la
                // résolution du phi.
                let counter_phi = phi_nodes.get(nom).unwrap();
                let next_bv: inkwell::values::BasicValueEnum = next.into();
                counter_phi.add_incoming(&[(&next_bv, builder.get_insert_block().unwrap())]);
                named_values.insert(nom.clone(), next.into());
                builder.build_unconditional_branch(loop_block);

                builder.position_at_end(after_block);
                // après la boucle : la valeur du compteur n'est plus définie
                named_values.remove(nom);
                Ok(())
            }

            Instr::Retour(e, _) => {
                match e {
                    Some(expr) => {
                        let value = compile_expr(builder, context, module, named_values, expr)?;
                        builder.build_return(Some(&value));
                    }
                    None => {
                        builder.build_return(None);
                    }
                }
                Ok(())
            }
            Instr::Bloc(b) => {
                for instr in b {
                    compile_instr(builder, context, module, named_values, instr)?;
                }
                Ok(())
            }
            Instr::Lire { .. } => Err("lecture clavier non supportée en ORC".to_string()),
        }
    }

    // Compiler les instructions du programme principal
    for instr in &prog.instructions {
        compile_instr(&builder, &context, &module, &mut named_values, instr)?;
    }

    builder.build_return(Some(&context.i64_type().const_int(0, false)));

    // ── Optimisations LLVM ─────────────────────────────────────────────────
    // Le niveau d'optimisation est deja applique lors de la creation du
    // TargetMachine (opt_level.to_inkwell()). Les passes d'optimisation
    // supplementaires (inlining, vectorisation, etc.) peuvent etre ajoutees
    // ici via inkwell::passes::PassManager quand l'API le permettra.
    let _ = opt_level;

    // PGO (Profile-Guided Optimization) ─────────────────────────────────
    // Le PGO complet avec LLVM ORC in-process requiere :
    //   1. --pgo-generate : ajouter des passes d'instrumentation (instr-profiling)
    //      au module LLVM avant l'emission. Le binaire produit generera un
    //      .profraw a l'exécution.
    //   2. llvm-profdata merge : convertir le .profraw en .profdata
    //   3. --pgo-use : recompiler avec le .profdata pour optimisation guidee
    //
    // Avec inkwell 0.7, les passes d'instrumentation PGO ne sont pas encore
    // exposees directement. Le squelette ci-dessous documente la marche a
    // suivre et prepare le terrain pour une integration future.
    match pgo_mode {
        PgoMode::Generate => {
            println!("rose: --pgo-generate demande.");
            println!("  Workflow PGO complet :");
            println!("  1. rose orc prog.rose -o prog.ll -O2 --pgo-generate");
            println!("     (genere l'IR LLVM avec instrumentation)");
            println!("  2. clang -fprofile-instr-generate prog.ll -o prog_instr");
            println!("  3. ./prog_instr  (genere default.profraw)");
            println!("  4. llvm-profdata merge -o prog.profdata default.profraw");
            println!("  5. rose orc prog.rose -o prog_opt.ll -O2 --pgo-use prog.profdata");
            println!("  Note: l'instrumentation PGO in-process necessite LLVM C API");
            println!("  (LLVMAddInstrProfilingPass) non expose par inkwell 0.7.");
        }
        PgoMode::Use => {
            println!("rose: --pgo-use demande.");
            println!("  Fournir un .profdata (genere par llvm-profdata merge)");
            println!("  pour l'optimisation guidee du code.");
            println!("  Exemple : rose orc prog.rose -o prog.ll -O2 --pgo-use prog.profdata");
        }
        PgoMode::None => {}
    }

    // ── Écriture du fichier de sortie ─────────────────────────────────────
    let path = Path::new(sortie);
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    match ext {
        "ll" => {
            module.print_to_file(path)
                .map_err(|e| format!("échec écriture IR : {:?}", e))?;
        }
        "bc" => {
            let mut f = File::create(path)
                .map_err(|e| format!("impossible de créer {} : {}", path.display(), e))?;
            let success = module.write_bitcode_to_file(&mut f, true, true);
            if !success {
                return Err("échec écriture bitcode".to_string());
            }
        }
        "o" | "obj" => {
            // ── Émission de fichier objet natif ──
            // Tentative directe via TargetMachine::write_to_file
            match target_machine.write_to_file(&module, FileType::Object, path) {
                Ok(()) => {
                    println!("rose: {} produit (objet natif via LLVM ORC)", sortie);
                }
                Err(e) => {
                    // Fallback : générer l'IR LLVM (.ll), puis appeler clang -c
                    let ll_path = format!("{}.ll", sortie.trim_end_matches(".o").trim_end_matches(".obj"));
                    let ll_path = Path::new(&ll_path);
                    module.print_to_file(ll_path)
                        .map_err(|e| format!("échec écriture IR LLVM : {:?}", e))?;

                    let clang_status = Command::new("clang")
                        .args(["-c", ll_path.to_str().unwrap(), "-o", sortie])
                        .status();

                    match clang_status {
                        Ok(s) if s.success() => {
                            println!("rose: {} produit (objet natif via clang)", sortie);
                        }
                        _ => {
                            return Err(format!(
                                "échec émission objet : TargetMachine et clang ont échoué : {:?}",
                                e
                            ));
                        }
                    }
                }
            }
        }
        _ => {
            let ir_path = format!("{}.ll", sortie);
            let ir_path = Path::new(&ir_path);
            module.print_to_file(ir_path)
                .map_err(|e| format!("échec écriture IR : {:?}", e))?;
            println!("rose: {} produit (IR LLVM texte)", ir_path.display());
        }
    }

    Ok(())
}
