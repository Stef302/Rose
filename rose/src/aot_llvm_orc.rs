// ═══════════════════════════════════════════════════════════════════════════
// Rose — AOT LLVM ORC (On-Request Compilation) via inkwell
//
// Compilation LLVM ORC in-process (pas de subprocess C/gcc) :
//   • Génération d'IR LLVM depuis des expressions Rose entières
//   • Émission de fichier objet natif ou IR texte
//   • Support des niveaux -O1 / -O2 / -O3
//
// Usage : rose-20 orc prog.rose [-o sortie] [-O1|-O2|-O3]
//
// ═══════════════════════════════════════════════════════════════════════════

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use inkwell::context::Context;
use inkwell::module::{Module, Linkage};
use inkwell::builder::Builder;
use inkwell::types::BasicMetadataTypeEnum;
use inkwell::values::{BasicValueEnum, BasicMetadataValueEnum};
use inkwell::IntPredicate;
use inkwell::OptimizationLevel;
use inkwell::targets::{
    InitializationConfig, Target, TargetMachine, CodeModel, RelocMode, FileType,
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

/// Point d'entrée CLI pour la commande `orc`
pub fn run_orc(fichier: &str, sortie: &str, opt_level: OrcOptLevel) -> Result<(), String> {
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
    Target::initialize_all(&InitializationConfig::default());

    let context = Context::create();
    let module = context.create_module("rose_orc");
    let builder = context.create_builder();

    let triple = TargetMachine::get_default_triple();
    let target = Target::get_first()
        .ok_or("aucune cible LLVM disponible")?;
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

    // ── Utiliser des références 'static via unsafe (toute la vie du scope) ──
    // inkwell lie toutes ses références à la durée de vie du Context. Comme
    // context, module et builder vivent tous pour la durée de run_orc, on peut
    // les traiter comme 'static en toute sécurité dans ce scope.
    let context: &'static Context = unsafe { std::mem::transmute(&context) };
    let module: &'static Module<'static> = unsafe { std::mem::transmute(&module) };
    let builder: &'static Builder<'static> = unsafe { std::mem::transmute(&builder) };

    // ── Compilation de la fonction main ──────────────────────────────────
    let i64_type = context.i64_type();
    let fn_type = i64_type.fn_type(&[], false);
    let function = module.add_function("main", fn_type, Some(Linkage::External));

    let entry = context.append_basic_block(function, "entry");
    builder.position_at_end(entry);

    // Fonction récursive interne pour compiler une expression
    fn compile_expr<'ctx>(
        builder: &Builder<'ctx>,
        context: &'ctx Context,
        module: &'ctx Module<'ctx>,
        named_values: &mut HashMap<String, BasicValueEnum<'ctx>>,
        expr: &Expr,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        match expr {
            Expr::Litteral { texte, flott, neg, .. } => {
                if *flott {
                    return Err("flottant non supporté en mode ORC entier pour l'instant".to_string());
                }
                let s = texte.trim_start_matches('-');
                let v: i64 = s.parse().map_err(|e| format!("littéral invalide '{}': {}", texte, e))?;
                let val = if *neg { -(v as i64) } else { v };
                Ok(context.i64_type().const_int(val as u64, false).into())
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
                    "non" => {
                        let one = context.bool_type().const_int(1, false);
                        let result = builder.build_xor(operand.into_int_value(), one.into(), "non")
                            .map_err(|e| format!("erreur build_xor : {:?}", e))?;
                        Ok(result.into())
                    }
                    "-" => {
                        Ok(builder.build_int_neg(operand.into_int_value(), "neg")
                            .map_err(|e| format!("erreur build_int_neg : {:?}", e))?
                            .into())
                    }
                    _ => Err(format!("opérateur unaire non supporté en ORC : {}", op)),
                }
            }
            Expr::Binaire { op, a, b, .. } => {
                let left = compile_expr(builder, context, module, named_values, a)?;
                let right = compile_expr(builder, context, module, named_values, b)?;
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
                            .map_err(|e| format!("erreur build_int_z_extend : {:?}", e))?
                            .into())
                    }
                    "<=" => {
                        let cmp = builder.build_int_compare(IntPredicate::SLE, left.into_int_value(), right.into_int_value(), "le")
                            .map_err(|e| format!("erreur build_int_compare : {:?}", e))?;
                        Ok(builder.build_int_z_extend(cmp, context.i64_type(), "le_i64")
                            .map_err(|e| format!("erreur build_int_z_extend : {:?}", e))?
                            .into())
                    }
                    ">" => {
                        let cmp = builder.build_int_compare(IntPredicate::SGT, left.into_int_value(), right.into_int_value(), "gt")
                            .map_err(|e| format!("erreur build_int_compare : {:?}", e))?;
                        Ok(builder.build_int_z_extend(cmp, context.i64_type(), "gt_i64")
                            .map_err(|e| format!("erreur build_int_z_extend : {:?}", e))?
                            .into())
                    }
                    ">=" => {
                        let cmp = builder.build_int_compare(IntPredicate::SGE, left.into_int_value(), right.into_int_value(), "ge")
                            .map_err(|e| format!("erreur build_int_compare : {:?}", e))?;
                        Ok(builder.build_int_z_extend(cmp, context.i64_type(), "ge_i64")
                            .map_err(|e| format!("erreur build_int_z_extend : {:?}", e))?
                            .into())
                    }
                    "et" => Ok(builder.build_and(left.into_int_value(), right.into_int_value(), "and")
                        .map_err(|e| format!("erreur build_and : {:?}", e))?
                        .into()),
                    "ou" => Ok(builder.build_or(left.into_int_value(), right.into_int_value(), "or")
                        .map_err(|e| format!("erreur build_or : {:?}", e))?
                        .into()),
                    _ => Err(format!("opérateur binaire non supporté en ORC : {}", op)),
                }
            }
            Expr::Appel { nom, args, .. } => {
                let mut arg_values: Vec<BasicMetadataValueEnum> = Vec::new();
                for arg in args {
                    let v = compile_expr(builder, context, module, named_values, arg)?;
                    arg_values.push(v.into());
                }
                if let Some(func) = module.get_function(nom) {
                    let call_site = builder.build_call(func, &arg_values, "call")
                        .map_err(|e| format!("erreur build_call : {:?}", e))?;
                    let _ = call_site;
                    Ok(context.i64_type().const_int(0, false).into())
                } else {
                    Err(format!("fonction inconnue en ORC : {}", nom))
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
        module: &'ctx Module<'ctx>,
        named_values: &mut HashMap<String, BasicValueEnum<'ctx>>,
        instr: &Instr,
    ) -> Result<(), String> {
        match instr {
            Instr::Affect { nom, e, .. } => {
                let value = compile_expr(builder, context, module, named_values, e)?;
                named_values.insert(nom.clone(), value);
                Ok(())
            }
            Instr::Afficher(e, _) => {
                let value = compile_expr(builder, context, module, named_values, e)?;

                // Déclaration de printf
                let i64_type = context.i64_type();
                let ptr_type = context.ptr_type(inkwell::AddressSpace::default());
                let printf_type = i64_type.fn_type(
                    &[BasicMetadataTypeEnum::PointerType(ptr_type.into())],
                    true
                );
                let printf_func = module.add_function("printf", printf_type, Some(Linkage::External));

                // Chaîne de format "%lld\n\0"
                let fmt_bytes = b"%lld\n\0";
                let fmt_str = context.const_string(fmt_bytes, false);
                let fmt_global = module.add_global(fmt_str.get_type(), None, "fmt_orc");
                fmt_global.set_linkage(Linkage::Internal);
                fmt_global.set_initializer(&fmt_str);
                fmt_global.set_constant(true);
                fmt_global.set_unnamed_addr(true);

                let fmt_ptr = builder.build_pointer_cast(
                    fmt_global.as_pointer_value(),
                    ptr_type,
                    "fmt_ptr",
                ).map_err(|e| format!("erreur build_pointer_cast : {:?}", e))?;

                let int_val = match value {
                    BasicValueEnum::IntValue(v) => v,
                    _ => return Err("type non supporté pour affichage".to_string()),
                };

                let call_site = builder.build_call(printf_func, &[fmt_ptr.into(), int_val.into()], "printf_call")
                    .map_err(|e| format!("erreur build_call printf : {:?}", e))?;
                let _ = call_site;
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

                let main_func = module.get_function("main").ok_or("fonction main manquante")?;
                let then_block = context.append_basic_block(main_func, "then");
                let else_block = context.append_basic_block(main_func, "else");
                let merge_block = context.append_basic_block(main_func, "merge");

                builder.build_conditional_branch(cond_bool, then_block, else_block);

                builder.position_at_end(then_block);
                for instr in alors {
                    compile_instr(builder, context, module, named_values, instr)?;
                }
                builder.build_unconditional_branch(merge_block);

                builder.position_at_end(else_block);
                if let Some(s) = sinon {
                    for instr in s {
                        compile_instr(builder, context, module, named_values, instr)?;
                    }
                }
                builder.build_unconditional_branch(merge_block);

                builder.position_at_end(merge_block);
                Ok(())
            }
            Instr::TantQue { test, corps, .. } => {
                let main_func = module.get_function("main").ok_or("fonction main manquante")?;
                let loop_block = context.append_basic_block(main_func, "while_loop");
                let body_block = context.append_basic_block(main_func, "while_body");
                let after_block = context.append_basic_block(main_func, "while_after");

                builder.build_unconditional_branch(loop_block);

                builder.position_at_end(loop_block);
                let cond = compile_expr(builder, context, module, named_values, test)?;
                let cond_bool = builder.build_int_compare(
                    IntPredicate::NE,
                    cond.into_int_value(),
                    context.i64_type().const_int(0, false),
                    "while_cond",
                ).map_err(|e| format!("erreur build_int_compare : {:?}", e))?;
                builder.build_conditional_branch(cond_bool, body_block, after_block);

                builder.position_at_end(body_block);
                for instr in corps {
                    compile_instr(builder, context, module, named_values, instr)?;
                }
                builder.build_unconditional_branch(loop_block);

                builder.position_at_end(after_block);
                Ok(())
            }
            Instr::Pour { nom, debut, fin, corps, .. } => {
                let start = compile_expr(builder, context, module, named_values, debut)?;
                named_values.insert(nom.clone(), start);

                let main_func = module.get_function("main").ok_or("fonction main manquante")?;
                let loop_block = context.append_basic_block(main_func, "for_loop");
                let body_block = context.append_basic_block(main_func, "for_body");
                let after_block = context.append_basic_block(main_func, "for_after");

                builder.build_unconditional_branch(loop_block);

                builder.position_at_end(loop_block);
                let current = named_values.get(nom).cloned().ok_or("variable de boucle non initialisée")?;
                let end = compile_expr(builder, context, module, named_values, fin)?;
                let cond = builder.build_int_compare(
                    IntPredicate::SLT,
                    current.into_int_value(),
                    end.into_int_value(),
                    "for_cond",
                ).map_err(|e| format!("erreur build_int_compare : {:?}", e))?;
                builder.build_conditional_branch(cond, body_block, after_block);

                builder.position_at_end(body_block);
                for instr in corps {
                    compile_instr(builder, context, module, named_values, instr)?;
                }
                let next = builder.build_int_add(
                    current.into_int_value(),
                    context.i64_type().const_int(1, false),
                    "for_next",
                ).map_err(|e| format!("erreur build_int_add : {:?}", e))?;
                named_values.insert(nom.clone(), next.into());
                builder.build_unconditional_branch(loop_block);

                builder.position_at_end(after_block);
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
        compile_instr(builder, context, module, &mut named_values, instr)?;
    }

    builder.build_return(Some(&context.i64_type().const_int(0, false)));

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
            target_machine.write_to_file(&module, FileType::Object, path)
                .map_err(|e| format!("échec émission objet : {:?}", e))?;
        }
        _ => {
            let ir_path = format!("{}.ll", sortie);
            let ir_path = Path::new(&ir_path);
            module.print_to_file(ir_path)
                .map_err(|e| format!("échec écriture IR : {:?}", e))?;
            println!("rose-20: {} produit (IR LLVM texte)", ir_path.display());
        }
    }

    Ok(())
}
