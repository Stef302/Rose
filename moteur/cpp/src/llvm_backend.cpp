#include "llvm_backend.h"
#include "rir.h"
#include <sstream>
#include <map>
#include <set>

// Map RirType to LLVM type.
static std::string llvm_type(RirType t) {
    switch (t) {
        case RirZ: return "i64";
        case RirF: return "double";
        case RirB: return "i1";
        case RirS: return "i8*";
        case RirTab: return "i8*";  // opaque tampon pointer
        case RirU: return "i64";
        default: return "i64";
    }
}

// Sanitize a name for LLVM IR.
static std::string sanitize(const std::string &name) {
    std::string s = name;
    if (!s.empty() && (s[0] == '%' || s[0] == '$')) s = s.substr(1);
    for (char &c : s) {
        if (!isalnum((unsigned char)c) && c != '_') c = '_';
    }
    if (s.empty()) s = "v";
    // Ensure doesn't start with digit
    if (isdigit((unsigned char)s[0])) s = "v" + s;
    return s;
}

std::string llvm_backend(const RirModule &mod) {
    std::ostringstream out;
    out << "; ModuleID = 'rose'\n";
    out << "target triple = \"aarch64-unknown-linux-gnu\"\n\n";

    // Declare runtime functions
    out << "declare void @rose_runtime_set_args(i32, i8**)\n";
    out << "declare i8* @rose_octets(i8*)\n";
    out << "declare i8* @rose_tampon_new(i64)\n";
    out << "declare void @rose_graine(i64)\n";
    out << "declare void @rose_remplir_hasard(i8*, i8*)\n";
    out << "declare i64 @rose_millisecondes()\n";
    out << "declare i64 @rose_nb_arguments()\n";
    out << "declare i8* @rose_argument(i64)\n";
    out << "declare i64 @rose_entier(i8*)\n";
    out << "declare i8* @rose_chaine(i64)\n";
    out << "declare void @rose_afficher_int(i64)\n";
    out << "declare void @rose_afficher_str(i8*)\n";
    out << "declare i64 @rose_tampon_longueur(i8*)\n";
    out << "declare i64 @rose_pgcd(i64, i64)\n";
    out << "declare i64 @rose_ppcm(i64, i64)\n";
    out << "declare i64 @rose_factorielle(i64)\n";
    out << "declare i64 @rose_fibonacci(i64)\n";
    out << "declare i1 @rose_est_premier(i64)\n";
    out << "declare i64 @rose_compter_premiers(i64)\n";
    out << "declare i64 @rose_nb_chiffres(i64)\n";
    out << "declare i64 @rose_hasard(i64)\n";
    out << "declare i64 @rose_abs(i64)\n";
    out << "declare i64 @rose_min(i64, i64)\n";
    out << "declare i64 @rose_max(i64, i64)\n";
    out << "declare double @rose_afficher_float(double)\n";
    out << "\n";

    for (auto &f : mod.fonctions) {
        std::string fname = (f.nom == "$principal" || f.nom == "principal") ? "main" : sanitize(f.nom);
        std::string ret = "i32";
        if (fname != "main") {
            ret = llvm_type(f.retour);
            if (ret == "i8*") ret = "i64"; // default
        }

        // Function signature
        if (fname == "main") {
            out << "define i32 @main(i32 %argc, i8** %argv) {\n";
            out << "entry:\n";
            out << "  call void @rose_runtime_set_args(i32 %argc, i8** %argv)\n";
        } else {
            out << "define " << ret << " @" << fname << "(";
            for (size_t i = 0; i < f.params.size(); i++) {
                if (i > 0) out << ", ";
                out << llvm_type(f.params[i].second) << " %" << sanitize(f.params[i].first);
            }
            out << ") {\n";
            out << "entry:\n";
        }

        // Collect temp types
        std::map<std::string, RirType> temp_types;
        std::map<std::string, RirType> global_types;
        for (auto &b : f.blocs) {
            for (auto &instr : b.instrs) {
                if (!instr.dest.empty() && instr.dest[0] == '%') {
                    RirType ty = instr.dest_ty;
                    if (instr.op == "call" && !instr.args.empty()) {
                        std::string name = instr.args[0].name;
                        if (name[0] == '$') name = name.substr(1);
                        if (name == "octets" || name == "tampon") ty = RirTab;
                        else if (name == "millisecondes" || name == "nb_arguments" || name == "entier" ||
                                 name == "tampon_case" || name == "nb_chiffres" || name == "compter_premiers" ||
                                 name == "factorielle" || name == "fibonacci" || name == "pgcd" ||
                                 name == "ppcm" || name == "hasard") ty = RirZ;
                        else if (name == "argument" || name == "chaine") ty = RirS;
                        else if (name == "est_premier") ty = RirB;
                    }
                    temp_types[instr.dest] = ty;
                }
                if (instr.op == "affect" && !instr.message.empty()) {
                    std::string g = sanitize(instr.message);
                    RirType ty = RirU;
                    if (instr.args.size() > 0 && instr.args[0].kind == RirOp::Temp) {
                        auto it = temp_types.find(instr.args[0].name);
                        if (it != temp_types.end()) ty = it->second;
                    }
                    if (ty == RirU && instr.args.size() > 0) ty = instr.args[0].ty;
                    if (global_types.find(g) == global_types.end()) global_types[g] = ty;
                }
            }
        }

        // Declare global variables (as allocas)
        for (auto &kv : global_types) {
            std::string lt = llvm_type(kv.second);
            out << "  %" << kv.first << " = alloca " << lt << "\n";
            // Initialize
            if (lt == "i64" || lt == "i1") out << "  store " << lt << " 0, " << lt << "* %" << kv.first << "\n";
            else if (lt == "double") out << "  store double 0.0, double* %" << kv.first << "\n";
            else if (lt == "i8*") out << "  store i8* null, i8** %" << kv.first << "\n";
        }

        // Process each block
        for (auto &b : f.blocs) {
            if (!b.nom.empty() && b.nom != ".entree") {
                out << sanitize(b.nom) << ":\n";
            }
            for (auto &instr : b.instrs) {
                const std::string &op = instr.op;
                if (op == "cst.z") {
                    std::string val = "0";
                    if (!instr.args.empty() && instr.args[0].kind == RirOp::Const) {
                        if (instr.args[0].name.rfind("@c", 0) == 0) val = instr.args[0].name.substr(2);
                        else val = instr.args[0].name;
                    }
                    out << "  store i64 " << val << ", i64* %" << sanitize(instr.dest) << "\n";
                }
                else if (op == "cst.f") {
                    std::string val = "0.0";
                    if (!instr.args.empty() && instr.args[0].kind == RirOp::Const) {
                        if (instr.args[0].name.rfind("@c", 0) == 0) val = instr.args[0].name.substr(2);
                        else val = instr.args[0].name;
                    }
                    out << "  store double " << val << ", double* %" << sanitize(instr.dest) << "\n";
                }
                else if (op == "cst.s") {
                    // String constant - store as i8*
                    std::string s = "";
                    if (!instr.args.empty() && instr.args[0].kind == RirOp::Const) {
                        s = instr.args[0].name;
                    }
                    // For now, use a global string
                    out << "  ; string const: " << s << "\n";
                    out << "  store i8* null, i8** %" << sanitize(instr.dest) << "\n";
                }
                else if (op == "affect") {
                    std::string src_var = sanitize(instr.args[0].name);
                    std::string dst_var = sanitize(instr.message);
                    RirType ty = temp_types.count(instr.args[0].name) ? temp_types[instr.args[0].name] : instr.args[0].ty;
                    if (global_types.count(dst_var)) ty = global_types[dst_var];
                    std::string lt = llvm_type(ty);
                    out << "  %" << sanitize(instr.dest) << "_val = load " << lt << ", " << lt << "* %" << src_var << "\n";
                    // Store to global if needed
                    if (global_types.count(dst_var)) {
                        out << "  store " << lt << " %" << sanitize(instr.dest) << "_val, " << lt << "* %" << dst_var << "\n";
                    }
                }
                else if (op == "afficher") {
                    std::string arg = sanitize(instr.args[0].name);
                    RirType ty = temp_types.count(arg) ? temp_types["%" + arg] : RirU;
                    // Simplified: just call rose_afficher_int
                    out << "  %_aff = load i64, i64* %" << arg << "\n";
                    out << "  call void @rose_afficher_int(i64 %_aff)\n";
                }
                else if (op == "call") {
                    if (instr.args.empty()) continue;
                    std::string name = instr.args[0].name;
                    if (name[0] == '$') name = name.substr(1);
                    // Handle builtins
                    if (name == "octets" || name == "tampon" || name == "graine" ||
                        name == "remplir_hasard" || name == "millisecondes" || name == "nb_arguments" ||
                        name == "argument" || name == "entier" || name == "longueur" || name == "chaine" ||
                        name == "pgcd" || name == "ppcm" || name == "factorielle" || name == "fibonacci" ||
                        name == "est_premier" || name == "compter_premiers" || name == "nb_chiffres" ||
                        name == "hasard" || name == "abs" || name == "min" || name == "max") {
                        out << "  ; call " << name << "\n";
                    }
                }
                else if (op == "br") {
                    if (!instr.label_cible.empty()) {
                        out << "  br label %" << sanitize(instr.label_cible) << "\n";
                    }
                }
                else if (op == "br.cond") {
                    std::string cond = sanitize(instr.args[0].name);
                    out << "  %_cond = load i1, i1* %" << cond << "\n";
                    out << "  br i1 %_cond, label %" << sanitize(instr.label_cible) << ", label %" << sanitize(instr.label_cible2) << "\n";
                }
                else if (op.rfind("cmp.", 0) == 0) {
                    std::string dest = sanitize(instr.dest);
                    std::string a0 = sanitize(instr.args[0].name);
                    std::string a1 = sanitize(instr.args[1].name);
                    out << "  %" << dest << "_v = icmp slt i64 0, 0\n"; // placeholder
                    out << "  store i1 0, i1* %" << dest << "\n";
                }
                else if (op == "add.z" || op == "sub.z" || op == "mul.z") {
                    std::string dest = sanitize(instr.dest);
                    std::string a0 = sanitize(instr.args[0].name);
                    std::string a1 = sanitize(instr.args[1].name);
                    char oper = (op == "add.z") ? '+' : (op == "sub.z") ? '-' : '*';
                    out << "  %" << dest << "_v0 = load i64, i64* %" << a0 << "\n";
                    out << "  %" << dest << "_v1 = load i64, i64* %" << a1 << "\n";
                    out << "  %" << dest << "_r = " << (op=="add.z"?"add":op=="sub.z"?"sub":"mul") << " i64 %" << dest << "_v0, %" << dest << "_v1\n";
                    out << "  store i64 %" << dest << "_r, i64* %" << dest << "\n";
                }
                else if (op == "ret.u" || op == "ret" || op == "ret.z") {
                    out << "  ret i32 0\n";
                }
                else {
                    out << "  ; unsupported: " << op << "\n";
                }
            }
        }

        out << "}\n\n";
    }

    return out.str();
}
