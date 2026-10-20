#include "transpiler.h"
#include "rir.h"
#include "rose_runtime.h"
#include <sstream>
#include <set>
#include <map>
#include <cstdlib>
#include <cerrno>

// Map RirType to C++ type name.
static std::string cpp_type(RirType t) {
    switch (t) {
        case RirZ: return "rose_int";
        case RirF: return "rose_float";
        case RirB: return "rose_bool";
        case RirS: return "rose_str";
        case RirU: return "rose_val";  // generic Valeur
        case RirTab: return "rose_tampon";
        case RirListe: return "rose_liste";
        case RirGd: return "rose_gd";
        default: return "rose_int";
    }
}

// Vrai si le texte d'une constante entière ne tient pas dans un int64.
static bool cst_hors_int64(const std::string &txt) {
    if (txt.empty()) return false;
    errno = 0;
    char *fin = nullptr;
    (void)std::strtoll(txt.c_str(), &fin, 10);
    return errno == ERANGE || (fin && *fin != '\0');
}

// Map a RirOp to a C++ expression.
static std::string sanitize(const std::string &name);
static std::string cpp_operand(const RirOp &op, std::map<std::string,RirType> &temp_types) {
    if (op.kind == RirOp::Const) {
        // Numeric constant: @c123 → 123
        if (op.name.rfind("@c", 0) == 0) {
            std::string txt = op.name.substr(2);
            // Check if it's a float
            if (op.ty == RirF) return txt + ".0"; // approximate
            if (cst_hors_int64(txt)) return "rose_gd(\"" + txt + "\")";
            return txt;
        }
        // String constant: raw text → "text"
        if (op.ty == RirS) {
            // Escape the string
            std::string s = op.name;
            std::string escaped;
            for (char c : s) {
                if (c == '"') escaped += "\\\"";
                else if (c == '\\') escaped += "\\\\";
                else if (c == '\n') escaped += "\\n";
                else escaped += c;
            }
            return "\"" + escaped + "\"";
        }
        // Bool constant
        if (op.name == "vrai") return "true";
        if (op.name == "faux") return "false";
        return "0";
    }
    if (op.kind == RirOp::Temp) {
        return sanitize(op.name);  // %t0 → t0
    }
    if (op.kind == RirOp::Global) {
        return sanitize(op.name);  // $var → var
    }
    return "0";
}

// Sanitize a temp name for C++ (%t0 → t0)
static std::string sanitize(const std::string &name) {
    std::string s = name;
    // Remove % prefix
    if (!s.empty() && s[0] == '%') s = s.substr(1);
    // Remove $ prefix
    if (!s.empty() && s[0] == '$') s = s.substr(1);
    // Replace non-alphanumeric
    for (char &c : s) {
        if (!isalnum((unsigned char)c) && c != '_') c = '_';
    }
    return s;
}

std::string transpiler_rir(const RirModule &mod) {
    std::ostringstream out;
    out << "#include \"rose_runtime.h\"\n";
    out << "#include <cstdint>\n";
    out << "#include <cstdio>\n";
    out << "#include <string>\n\n";

    // Noms des fonctions utilisateur (pour dispatcher les appels)
    std::set<std::string> user_fns;
    for (auto &f : mod.fonctions) {
        if (f.nom == "$principal" || f.nom == "principal") continue;
        user_fns.insert(sanitize(f.nom));
    }

    // Forward declare functions
    for (auto &f : mod.fonctions) {
        if (f.nom == "$principal" || f.nom == "principal") continue;
        // Determine return type
        std::string ret = cpp_type(f.retour);
        if (ret == "rose_val") ret = "rose_int"; // default
        out << ret << " " << sanitize(f.nom) << "(";
        for (size_t i = 0; i < f.params.size(); i++) {
            if (i > 0) out << ", ";
            out << cpp_type(f.params[i].second) << " " << sanitize(f.params[i].first);
        }
        out << ");\n";
    }
    out << "\n";

    // Generate function bodies
    for (auto &f : mod.fonctions) {
        std::string fname = (f.nom == "$principal" || f.nom == "principal") ? "main" : sanitize(f.nom);
        std::string ret = cpp_type(f.retour);
        if (ret == "rose_val") ret = "rose_int";
        if (fname == "main") ret = "int";

        out << ret << " " << fname << "(";
        if (fname == "main") {
            out << "int argc, char **argv";
        } else {
            for (size_t i = 0; i < f.params.size(); i++) {
                if (i > 0) out << ", ";
                out << cpp_type(f.params[i].second) << " " << sanitize(f.params[i].first);
            }
        }
        out << ") {\n";

        if (fname == "main") {
            out << "    rose_runtime_set_args(argc, argv);\n";
        }

        // Collect temp variables
        std::set<std::string> temps;
        std::map<std::string, RirType> temp_types;
        std::map<std::string, RirType> global_types;
        for (auto &b : f.blocs) {
            for (auto &instr : b.instrs) {
                if (!instr.dest.empty() && instr.dest[0] == '%') {
                    temps.insert(instr.dest);
                    // Type inference for liste.new
                    if (instr.op == "liste.new") temp_types[instr.dest] = RirListe;
                    // Constante entière hors int64 → grand entier
                    if (instr.op == "cst.z" && !instr.args.empty() &&
                        instr.args[0].kind == RirOp::Const &&
                        cst_hors_int64(instr.args[0].name.rfind("@c", 0) == 0 ?
                                       instr.args[0].name.substr(2) : instr.args[0].name))
                        temp_types[instr.dest] = RirGd;
                }
                // Collect global variables from affect
                // Appels à grands résultats : typer avant l'inférence globale
                if (instr.op == "call" && !instr.dest.empty() && instr.dest[0] == '%' &&
                    !instr.args.empty()) {
                    std::string cn = instr.args[0].name;
                    if (!cn.empty() && cn[0] == '$') cn = cn.substr(1);
                    if (cn == "fibonacci" || cn == "factorielle" || cn == "puissance")
                        temp_types[instr.dest] = RirGd;
                }
                if (instr.op == "affect" && !instr.message.empty()) {
                    std::string g = sanitize(instr.message);
                    RirType ty = RirU;
                    // Infer type from the call that produced the source temp
                    if (instr.args.size() > 0 && instr.args[0].kind == RirOp::Temp) {
                        std::string src_temp = instr.args[0].name;
                        for (auto &b2 : f.blocs) for (auto &i2 : b2.instrs) {
                            if (i2.dest == src_temp) {
                                // Check if it's a tampon-returning builtin
                                if (i2.op == "call" && !i2.args.empty()) {
                                    std::string name = i2.args[0].name;
                                    if (name[0] == '$') name = name.substr(1);
                                    if (name == "octets" || name == "tampon") ty = RirTab;
                                    else ty = i2.dest_ty;
                                } else {
                                    ty = i2.dest_ty;
                                }
                                break;
                            }
                        }
                    }
                    if (ty == RirU && instr.args.size() > 0) ty = instr.args[0].ty;
                    // If still U but assigned from a known tampon, mark as tampon
                    // If source temp is a list, mark global as list
                    if (instr.args.size() > 0 && instr.args[0].kind == RirOp::Temp) {
                        std::string src_temp = instr.args[0].name;
                        if (temp_types.count(src_temp) && temp_types[src_temp] == RirListe) ty = RirListe;
                        if (temp_types.count(src_temp) && temp_types[src_temp] == RirGd) ty = RirGd;
                    }
                    if (global_types.find(g) == global_types.end()) global_types[g] = ty;
                }
            }
        }

        // Declare global variables
        for (auto &kv : global_types) {
            std::string ct = cpp_type(kv.second);
            if (ct == "rose_val") ct = "rose_int";
            if (ct == "rose_tampon") out << "    " << ct << " " << kv.first << ";\n";
            else if (ct == "rose_liste") out << "    " << ct << " " << kv.first << ";\n";
            else if (ct == "rose_str") out << "    " << ct << " " << kv.first << " = \"\";\n";
            else out << "    " << ct << " " << kv.first << " = 0;\n";
        }
        if (!global_types.empty()) out << "\n";

        // Declare temp variables
        // Need to determine types from dest_ty
        for (auto &b : f.blocs) {
            for (auto &instr : b.instrs) {
                if (!instr.dest.empty() && instr.dest[0] == '%') {
                    RirType ty = instr.dest_ty;
                    // Infer type from call name
                    if (instr.op == "call" && !instr.args.empty()) {
                        std::string name = instr.args[0].name;
                        if (name[0] == '$') name = name.substr(1);
                        if (name == "octets" || name == "tampon") ty = RirTab;
                        else if (name == "fibonacci" || name == "factorielle" || name == "puissance") ty = RirGd;
                        else if (name == "millisecondes" || name == "nb_arguments" || name == "entier" || name == "tampon_case" ||
                                 name == "nb_chiffres" || name == "compter_premiers" ||
                                 name == "pgcd" || name == "ppcm" || name == "hasard") ty = RirZ;
                        else if (name == "argument" || name == "chaine") ty = RirS;
                        else if (name == "est_premier") ty = RirB;
                        else if (name == "abs" || name == "min" || name == "max") {
                            // Check if any arg temp is float
                            ty = RirZ;
                            for (size_t ai = 1; ai < instr.args.size(); ai++) {
                                if (instr.args[ai].kind == RirOp::Temp) {
                                    for (auto &b2 : f.blocs) for (auto &i2 : b2.instrs) {
                                        if (i2.dest == instr.args[ai].name && i2.dest_ty == RirF) { ty = RirF; break; }
                                    }
                                }
                            }
                        }
                    }
                    // Opérations arithmétiques : propagent le type grand entier
                    if ((instr.op == "add.z" || instr.op == "sub.z" || instr.op == "mul.z" ||
                         instr.op == "neg.z" || instr.op == "pow" || instr.op == "add" ) && ty == RirZ) {
                        for (size_t ai = 0; ai < instr.args.size(); ai++) {
                            if (instr.args[ai].kind != RirOp::Temp) continue;
                            std::string an = instr.args[ai].name;
                            if (temp_types.count(an) && temp_types[an] == RirGd) { ty = RirGd; break; }
                            if (global_types.count(sanitize(an)) && global_types[sanitize(an)] == RirGd) { ty = RirGd; break; }
                        }
                    }
                    if (temp_types.find(instr.dest) == temp_types.end() ||
                        (temp_types[instr.dest] != RirListe && temp_types[instr.dest] != RirGd))
                        temp_types[instr.dest] = ty;
                }
            }
        }
        for (auto &t : temps) {
            RirType ty = temp_types.count(t) ? temp_types[t] : RirU;
            std::string ct = cpp_type(ty);
            if (ct == "rose_val") ct = "rose_int";
            if (ct == "rose_str") out << "    " << ct << " " << sanitize(t) << " = \"\";\n";
            else if (ct == "rose_tampon") out << "    " << ct << " " << sanitize(t) << ";\n";
            else if (ct == "rose_liste") out << "    " << ct << " " << sanitize(t) << ";\n";
            else out << "    " << ct << " " << sanitize(t) << " = 0;\n";
        }

        // Generate code for each block
        for (auto &b : f.blocs) {
            if (b.nom != ".entree" && !b.nom.empty()) {
                out << "  " << sanitize(b.nom) << ":\n";
            }
            for (auto &instr : b.instrs) {
                const std::string &op = instr.op;
                // cst.z
                if (op == "cst.z") {
                    std::string val = "0";
                    if (!instr.args.empty()) {
                        if (instr.args[0].kind == RirOp::Const) {
                            if (instr.args[0].name.rfind("@c", 0) == 0)
                                val = instr.args[0].name.substr(2);
                            else val = instr.args[0].name;
                        }
                    }
                    if (cst_hors_int64(val))
                        out << "    " << sanitize(instr.dest) << " = rose_gd(\"" << val << "\");\n";
                    else
                        out << "    " << sanitize(instr.dest) << " = " << val << ";\n";
                }
                else if (op == "cst.s") {
                    std::string val = "\"\"";
                    if (!instr.args.empty() && instr.args[0].kind == RirOp::Const) {
                        std::string s = instr.args[0].name;
                        std::string escaped;
                        for (char c : s) {
                            if (c == '"') escaped += "\\\"";
                            else if (c == '\\') escaped += "\\\\";
                            else escaped += c;
                        }
                        val = "\"" + escaped + "\"";
                    }
                    out << "    " << sanitize(instr.dest) << " = " << val << ";\n";
                }
                else if (op == "cst.f") {
                    std::string val = "0.0";
                    if (!instr.args.empty() && instr.args[0].kind == RirOp::Const) {
                        if (instr.args[0].name.rfind("@c", 0) == 0) val = instr.args[0].name.substr(2);
                        else val = instr.args[0].name;
                    }
                    out << "    " << sanitize(instr.dest) << " = " << val << ";\n";
                }
                else if (op == "cst.b") {
                    std::string val = "false";
                    if (!instr.args.empty() && instr.args[0].kind == RirOp::Const) {
                        if (instr.args[0].name == "vrai") val = "true";
                        else if (instr.args[0].name == "faux") val = "false";
                    }
                    out << "    " << sanitize(instr.dest) << " = " << val << ";\n";
                }
                else if (op == "liste.new") {
                    out << "    " << sanitize(instr.dest) << " = rose_liste_new();\n";
                }
                else if (op == "liste.push") {
                    std::string list_var = sanitize(instr.args[0].name);
                    std::string elem = sanitize(instr.args[1].name);
                    // Determine if elem is a string temp
                    out << "    rose_liste_push(" << list_var << ", " << elem << ".c_str());\n";
                }
                else if (op == "index") {
                    std::string dest = sanitize(instr.dest);
                    std::string base = sanitize(instr.args[0].name);
                    std::string idx = sanitize(instr.args[1].name);
                    RirType bty = temp_types.count(instr.args[0].name) ? temp_types[instr.args[0].name] : RirU;
                    if (global_types.count(base)) bty = global_types[base];
                    if (bty == RirListe) out << "    " << dest << " = rose_index_liste(" << base << ", " << idx << ");\n";
                    else out << "    " << dest << " = rose_index_str(" << base << ", " << idx << ");\n";
                }
                else if (op == "liste.len") {
                    std::string dest = sanitize(instr.dest);
                    std::string base = sanitize(instr.args[0].name);
                    out << "    " << dest << " = rose_liste_len(" << base << ");\n";
                }
                else if (op == "affect") {
                    std::string src_expr = cpp_operand(instr.args[0], temp_types);
                    // sanitize temp names
                    if (instr.args[0].kind == RirOp::Temp) src_expr = sanitize(instr.args[0].name);
                    if (instr.args[0].kind == RirOp::Const) {
                        if (instr.args[0].name.rfind("@c", 0) == 0) src_expr = instr.args[0].name.substr(2);
                        else if (instr.args[0].ty == RirS) {
                            std::string s = instr.args[0].name;
                            std::string escaped;
                            for (char c : s) {
                                if (c == '"') escaped += "\\\"";
                                else if (c == '\\') escaped += "\\\\";
                                else escaped += c;
                            }
                            src_expr = "\"" + escaped + "\"";
                        }
                    }
                    out << "    " << sanitize(instr.message) << " = " << src_expr << ";\n";
                }
                else if (op == "afficher") {
                    std::string arg = sanitize(instr.args[0].name);
                    RirType ty = RirU;
                    if (temp_types.count(instr.args[0].name)) ty = temp_types[instr.args[0].name];
                    else if (global_types.count(sanitize(instr.args[0].name))) ty = global_types[sanitize(instr.args[0].name)];
                    else ty = instr.args[0].ty;
                    if (ty == RirZ) out << "    rose_afficher_int(" << arg << ");\n";
                    else if (ty == RirGd) out << "    rose_afficher_gd(" << arg << ");\n";
                    else if (ty == RirF) out << "    rose_afficher_float(" << arg << ");\n";
                    else if (ty == RirB) out << "    rose_afficher_bool(" << arg << ");\n";
                    else if (ty == RirS) out << "    rose_afficher_str(" << arg << ".c_str());\n";
                    else if (ty == RirTab) out << "    rose_afficher_tampon_longueur(" << arg << ");\n";
                    else out << "    rose_afficher_int(" << arg << ");\n";
                }
                else if (op == "call") {
                    // builtin call: $name args...
                    if (instr.args.empty()) continue;
                    std::string name = instr.args[0].name;
                    if (name[0] == '$') name = name.substr(1);
                    std::string sname = sanitize(name);
                    // Appel de fonction utilisateur
                    if (user_fns.count(sname)) {
                        std::string call = sname + "(";
                        for (size_t ai = 1; ai < instr.args.size(); ai++) {
                            if (ai > 1) call += ", ";
                            call += cpp_operand(instr.args[ai], temp_types);
                        }
                        call += ")";
                        if (!instr.dest.empty())
                            out << "    " << sanitize(instr.dest) << " = " << call << ";\n";
                        else
                            out << "    " << call << ";\n";
                    }
                    // Map to runtime function
                    else if (name == "octets") {
                        std::string arg = cpp_operand(instr.args[1], temp_types);
                        if (instr.args[1].kind == RirOp::Temp) arg = sanitize(instr.args[1].name) + ".c_str()";
                        if (instr.args[1].kind == RirOp::Const) {
                            std::string s = instr.args[1].name;
                            std::string escaped;
                            for (char c : s) {
                                if (c == '"') escaped += "\\\"";
                                else if (c == '\\') escaped += "\\\\";
                                else escaped += c;
                            }
                            arg = "\"" + escaped + "\"";
                        }
                        out << "    " << sanitize(instr.dest) << " = rose_octets(" << arg << ");\n";
                    }
                    else if (name == "tampon") {
                        std::string arg = sanitize(instr.args[1].name);
                        out << "    " << sanitize(instr.dest) << " = rose_tampon_new(" << arg << ");\n";
                    }
                    else if (name == "graine") {
                        std::string arg = sanitize(instr.args[1].name);
                        out << "    rose_graine(" << arg << ");\n";
                    }
                    else if (name == "remplir_hasard") {
                        std::string a0 = sanitize(instr.args[1].name);
                        std::string a1 = sanitize(instr.args[2].name);
                        out << "    rose_remplir_hasard(" << a0 << ", " << a1 << ");\n";
                    }
                    else if (name == "millisecondes") {
                        out << "    " << sanitize(instr.dest) << " = rose_millisecondes();\n";
                    }
                    else if (name == "nb_arguments") {
                        out << "    " << sanitize(instr.dest) << " = rose_nb_arguments();\n";
                    }
                    else if (name == "argument") {
                        std::string arg = sanitize(instr.args[1].name);
                        out << "    " << sanitize(instr.dest) << " = rose_argument(" << arg << ");\n";
                    }
                    else if (name == "entier") {
                        std::string arg = sanitize(instr.args[1].name);
                        out << "    " << sanitize(instr.dest) << " = rose_entier(" << arg << ".c_str());\n";
                    }
                    else if (name == "pgcd") {
                        std::string a0 = sanitize(instr.args[1].name);
                        std::string a1 = sanitize(instr.args[2].name);
                        out << "    " << sanitize(instr.dest) << " = rose_pgcd(" << a0 << ", " << a1 << ");\n";
                    }
                    else if (name == "ppcm") {
                        std::string a0 = sanitize(instr.args[1].name);
                        std::string a1 = sanitize(instr.args[2].name);
                        out << "    " << sanitize(instr.dest) << " = rose_ppcm(" << a0 << ", " << a1 << ");\n";
                    }
                    else if (name == "factorielle") {
                        std::string a0 = sanitize(instr.args[1].name);
                        out << "    " << sanitize(instr.dest) << " = rose_factorielle_gd(" << a0 << ");\n";
                    }
                    else if (name == "fibonacci") {
                        std::string a0 = sanitize(instr.args[1].name);
                        out << "    " << sanitize(instr.dest) << " = rose_fibonacci_gd(" << a0 << ");\n";
                    }
                    else if (name == "puissance") {
                        std::string a0 = sanitize(instr.args[1].name);
                        std::string a1 = sanitize(instr.args[2].name);
                        out << "    " << sanitize(instr.dest) << " = rose_puissance_gd(" << a0 << ", " << a1 << ");\n";
                    }
                    else if (name == "est_premier") {
                        std::string a0 = sanitize(instr.args[1].name);
                        out << "    " << sanitize(instr.dest) << " = rose_est_premier(" << a0 << ");\n";
                    }
                    else if (name == "compter_premiers") {
                        std::string a0 = sanitize(instr.args[1].name);
                        out << "    " << sanitize(instr.dest) << " = rose_compter_premiers(" << a0 << ");\n";
                    }
                    else if (name == "nb_chiffres") {
                        std::string a0 = sanitize(instr.args[1].name);
                        RirType aty = RirU;
                        if (temp_types.count(instr.args[1].name)) aty = temp_types[instr.args[1].name];
                        else if (global_types.count(a0)) aty = global_types[a0];
                        if (aty == RirGd)
                            out << "    " << sanitize(instr.dest) << " = rose_nb_chiffres_gd(" << a0 << ");\n";
                        else
                            out << "    " << sanitize(instr.dest) << " = rose_nb_chiffres(" << a0 << ");\n";
                    }
                    else if (name == "hasard") {
                        std::string a0 = sanitize(instr.args[1].name);
                        out << "    " << sanitize(instr.dest) << " = rose_hasard(" << a0 << ");\n";
                    }
                    else if (name == "abs") {
                        std::string a0 = sanitize(instr.args[1].name);
                        RirType aty = temp_types.count(instr.args[1].name) ? temp_types[instr.args[1].name] : instr.args[1].ty;
                        if (aty == RirF) out << "    " << sanitize(instr.dest) << " = rose_abs_f(" << a0 << ");\n";
                        else out << "    " << sanitize(instr.dest) << " = rose_abs(" << a0 << ");\n";
                    }
                    else if (name == "min") {
                        std::string a0 = sanitize(instr.args[1].name);
                        std::string a1 = sanitize(instr.args[2].name);
                        RirType aty = temp_types.count(instr.args[1].name) ? temp_types[instr.args[1].name] : instr.args[1].ty;
                        if (aty == RirF) out << "    " << sanitize(instr.dest) << " = rose_min_f(" << a0 << ", " << a1 << ");\n";
                        else out << "    " << sanitize(instr.dest) << " = rose_min(" << a0 << ", " << a1 << ");\n";
                    }
                    else if (name == "max") {
                        std::string a0 = sanitize(instr.args[1].name);
                        std::string a1 = sanitize(instr.args[2].name);
                        RirType aty = temp_types.count(instr.args[1].name) ? temp_types[instr.args[1].name] : instr.args[1].ty;
                        if (aty == RirF) out << "    " << sanitize(instr.dest) << " = rose_max_f(" << a0 << ", " << a1 << ");\n";
                        else out << "    " << sanitize(instr.dest) << " = rose_max(" << a0 << ", " << a1 << ");\n";
                    }
                    else if (name == "joindre") {
                        // joindre(list, elem): push elem to list
                        std::string list_var = sanitize(instr.args[1].name);
                        std::string elem = sanitize(instr.args[2].name);
                        if (instr.args[2].kind == RirOp::Const) {
                            std::string s = instr.args[2].name;
                            std::string escaped;
                            for (char c : s) {
                                if (c == '"') escaped += "\\\"";
                                else if (c == '\\') escaped += "\\\\";
                                else escaped += c;
                            }
                            out << "    rose_liste_push(" << list_var << ", \"" << escaped << "\".c_str());\n";
                        } else {
                            // elem is a temp - convert to string if needed
                            out << "    rose_liste_push(" << list_var << ", " << elem << ".c_str());\n";
                        }
                    }
                    else if (name == "concatener") {
                        std::string list_var = sanitize(instr.args[1].name);
                        out << "    " << sanitize(instr.dest) << " = rose_liste_concat(" << list_var << ");\n";
                    }
                    else if (name == "longueur") {
                        std::string arg = sanitize(instr.args[1].name);
                        RirType aty = RirU;
                        if (temp_types.count(instr.args[1].name)) aty = temp_types[instr.args[1].name];
                        else if (global_types.count(arg)) aty = global_types[arg];
                        else aty = instr.args[1].ty;
                        if (aty == RirListe) out << "    " << sanitize(instr.dest) << " = rose_liste_len(" << arg << ");\n";
                        else if (aty == RirTab) out << "    " << sanitize(instr.dest) << " = rose_tampon_longueur(" << arg << ");\n";
                        else out << "    " << sanitize(instr.dest) << " = " << arg << ".size();\n";
                    }
                    else if (name == "chaine") {
                        std::string arg = sanitize(instr.args[1].name);
                        out << "    " << sanitize(instr.dest) << " = rose_chaine(" << arg << ");\n";
                    }
                    else {
                        out << "    // unsupported call: " << name << "\n";
                    }
                }
                else if (op == "br") {
                    if (!instr.label_cible.empty()) {
                        out << "    goto " << sanitize(instr.label_cible) << ";\n";
                    }
                }
                else if (op == "br.cond") {
                    std::string cond = sanitize(instr.args[0].name);
                    out << "    if (" << cond << ") goto " << sanitize(instr.label_cible) << ";\n";
                    out << "    else goto " << sanitize(instr.label_cible2) << ";\n";
                }
                else if (op.rfind("cmp.", 0) == 0) {
                    // cmp.lt.z %t4 %k %t3 → t4 = (k < t3)
                    std::string dest = sanitize(instr.dest);
                    std::string a0 = sanitize(instr.args[0].name);
                    std::string a1 = sanitize(instr.args[1].name);
                    // Determine comparison operator
                    std::string cmpop = "<";
                    if (op.find(".lt.") != std::string::npos) cmpop = "<";
                    else if (op.find(".le.") != std::string::npos) cmpop = "<=";
                    else if (op.find(".gt.") != std::string::npos) cmpop = ">";
                    else if (op.find(".ge.") != std::string::npos) cmpop = ">=";
                    else if (op.find(".eq.") != std::string::npos) cmpop = "==";
                    else if (op.find(".ne.") != std::string::npos) cmpop = "!=";
                    out << "    " << dest << " = (" << a0 << " " << cmpop << " " << a1 << ");\n";
                }
                else if (op == "add.z") {
                    std::string dest = sanitize(instr.dest);
                    std::string a0 = sanitize(instr.args[0].name);
                    std::string a1 = sanitize(instr.args[1].name);
                    // Handle const
                    if (instr.args[1].kind == RirOp::Const) {
                        if (instr.args[1].name.rfind("@c", 0) == 0) a1 = instr.args[1].name.substr(2);
                    }
                    out << "    " << dest << " = " << a0 << " + " << a1 << ";\n";
                }
                else if (op == "add.f") {
                    std::string dest = sanitize(instr.dest);
                    std::string a0 = sanitize(instr.args[0].name);
                    std::string a1 = sanitize(instr.args[1].name);
                    out << "    " << dest << " = " << a0 << " + " << a1 << ";\n";
                }
                else if (op == "sub.f") {
                    std::string dest = sanitize(instr.dest);
                    std::string a0 = sanitize(instr.args[0].name);
                    std::string a1 = sanitize(instr.args[1].name);
                    out << "    " << dest << " = " << a0 << " - " << a1 << ";\n";
                }
                else if (op == "mul.f") {
                    std::string dest = sanitize(instr.dest);
                    std::string a0 = sanitize(instr.args[0].name);
                    std::string a1 = sanitize(instr.args[1].name);
                    out << "    " << dest << " = " << a0 << " * " << a1 << ";\n";
                }
                else if (op == "div.f") {
                    std::string dest = sanitize(instr.dest);
                    std::string a0 = sanitize(instr.args[0].name);
                    std::string a1 = sanitize(instr.args[1].name);
                    out << "    " << dest << " = " << a0 << " / " << a1 << ";\n";
                }
                else if (op == "neg.f") {
                    std::string dest = sanitize(instr.dest);
                    std::string a0 = sanitize(instr.args[0].name);
                    out << "    " << dest << " = -" << a0 << ";\n";
                }
                else if (op == "sub.z") {
                    std::string dest = sanitize(instr.dest);
                    std::string a0 = sanitize(instr.args[0].name);
                    std::string a1 = sanitize(instr.args[1].name);
                    if (instr.args[1].kind == RirOp::Const) {
                        if (instr.args[1].name.rfind("@c", 0) == 0) a1 = instr.args[1].name.substr(2);
                    }
                    out << "    " << dest << " = " << a0 << " - " << a1 << ";\n";
                }
                else if (op == "mul.z") {
                    std::string dest = sanitize(instr.dest);
                    std::string a0 = sanitize(instr.args[0].name);
                    std::string a1 = sanitize(instr.args[1].name);
                    if (instr.args[1].kind == RirOp::Const) {
                        if (instr.args[1].name.rfind("@c", 0) == 0) a1 = instr.args[1].name.substr(2);
                    }
                    out << "    " << dest << " = " << a0 << " * " << a1 << ";\n";
                }
                else if (op == "div.z") {
                    std::string dest = sanitize(instr.dest);
                    std::string a0 = sanitize(instr.args[0].name);
                    std::string a1 = sanitize(instr.args[1].name);
                    if (instr.args[1].kind == RirOp::Const) {
                        if (instr.args[1].name.rfind("@c", 0) == 0) a1 = instr.args[1].name.substr(2);
                    }
                    out << "    " << dest << " = " << a0 << " / " << a1 << ";\n";
                }
                else if (op == "pow") {
                    std::string dest = sanitize(instr.dest);
                    std::string a0 = sanitize(instr.args[0].name);
                    std::string a1 = sanitize(instr.args[1].name);
                    out << "    " << dest << " = rose_puissance_gd(" << a0 << ", " << a1 << ");\n";
                }
                else if (op == "rem.z.z") {
                    std::string dest = sanitize(instr.dest);
                    std::string a0 = sanitize(instr.args[0].name);
                    std::string a1 = sanitize(instr.args[1].name);
                    if (instr.args[1].kind == RirOp::Const) {
                        if (instr.args[1].name.rfind("@c", 0) == 0) a1 = instr.args[1].name.substr(2);
                    }
                    out << "    " << dest << " = " << a0 << " % " << a1 << ";\n";
                }
                else if (op == "neg.z") {
                    std::string dest = sanitize(instr.dest);
                    std::string a0 = sanitize(instr.args[0].name);
                    out << "    " << dest << " = -" << a0 << ";\n";
                }
                else if (op == "ret.u" || op == "ret" || op == "ret.z" || op == "ret.f" || op == "ret.b" || op == "ret.s") {
                    if (!instr.args.empty()) {
                        std::string v = instr.args[0].kind == RirOp::Temp ? sanitize(instr.args[0].name)
                                                            : cpp_operand(instr.args[0], temp_types);
                        out << "    return " << v << ";\n";
                    } else {
                        out << "    return 0;\n";
                    }
                }
                else if (op == "index") {
                    std::string dest = sanitize(instr.dest);
                    std::string base = sanitize(instr.args[0].name);
                    std::string idx = sanitize(instr.args[1].name);
                    out << "    " << dest << " = rose_index(" << base << ", " << idx << ");\n";
                }
                else if (op == "add.s") {
                    std::string dest = sanitize(instr.dest);
                    std::string a0 = sanitize(instr.args[0].name);
                    std::string a1 = sanitize(instr.args[1].name);
                    if (instr.args[1].kind == RirOp::Const) {
                        std::string s = instr.args[1].name;
                        std::string escaped;
                        for (char c : s) {
                            if (c == '"') escaped += "\\\"";
                            else if (c == '\\') escaped += "\\\\";
                            else escaped += c;
                        }
                        a1 = "\"" + escaped + "\"";
                    }
                    out << "    " << dest << " = " << a0 << " + " << a1 << ";\n";
                }
            }
        }

        if (fname == "main") {
            out << "    return 0;\n";
        }
        out << "}\n\n";
    }

    return out.str();
}
