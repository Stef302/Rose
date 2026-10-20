#include "transpiler.h"
#include "rir.h"
#include "rose_runtime.h"
#include <sstream>
#include <set>
#include <map>

// Map RirType to C++ type name.
static std::string cpp_type(RirType t) {
    switch (t) {
        case RirZ: return "rose_int";
        case RirF: return "rose_float";
        case RirB: return "rose_bool";
        case RirS: return "rose_str";
        case RirU: return "rose_val";  // generic Valeur
        case RirTab: return "rose_tampon";
        default: return "rose_int";
    }
}

// Map a RirOp to a C++ expression.
static std::string cpp_operand(const RirOp &op, std::map<std::string,RirType> &temp_types) {
    if (op.kind == RirOp::Const) {
        // Numeric constant: @c123 → 123
        if (op.name.rfind("@c", 0) == 0) {
            std::string txt = op.name.substr(2);
            // Check if it's a float
            if (op.ty == RirF) return txt + ".0"; // approximate
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
        return op.name;  // %t0 → %t0 (will be sanitized)
    }
    if (op.kind == RirOp::Global) {
        return op.name;  // $var → var
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
        std::map<std::string, RirType> global_types;
        for (auto &b : f.blocs) {
            for (auto &instr : b.instrs) {
                if (!instr.dest.empty() && instr.dest[0] == '%') {
                    temps.insert(instr.dest);
                }
                // Collect global variables from affect
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
                    if (global_types.find(g) == global_types.end()) global_types[g] = ty;
                }
            }
        }

        // Declare global variables
        for (auto &kv : global_types) {
            std::string ct = cpp_type(kv.second);
            if (ct == "rose_val") ct = "rose_int";
            if (ct == "rose_tampon") out << "    " << ct << " " << kv.first << ";\n";
            else if (ct == "rose_str") out << "    " << ct << " " << kv.first << " = \"\";\n";
            else out << "    " << ct << " " << kv.first << " = 0;\n";
        }
        if (!global_types.empty()) out << "\n";

        // Declare temp variables
        // Need to determine types from dest_ty
        std::map<std::string, RirType> temp_types;
        for (auto &b : f.blocs) {
            for (auto &instr : b.instrs) {
                if (!instr.dest.empty() && instr.dest[0] == '%') {
                    RirType ty = instr.dest_ty;
                    // Infer type from call name
                    if (instr.op == "call" && !instr.args.empty()) {
                        std::string name = instr.args[0].name;
                        if (name[0] == '$') name = name.substr(1);
                        if (name == "octets" || name == "tampon") ty = RirTab;
                        else if (name == "millisecondes" || name == "nb_arguments" || name == "entier" || name == "tampon_case") ty = RirZ;
                        else if (name == "argument") ty = RirS;
                        else if (name == "chaine") ty = RirS;
                    }
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
                    RirType ty = temp_types.count(instr.args[0].name) ? temp_types[instr.args[0].name] : instr.args[0].ty;
                    if (ty == RirZ) out << "    rose_afficher_int(" << arg << ");\n";
                    else if (ty == RirS) out << "    rose_afficher_str(" << arg << ".c_str());\n";
                    else if (ty == RirTab) out << "    rose_afficher_tampon_longueur(" << arg << ");\n";
                    else out << "    rose_afficher_int(" << arg << ");\n";
                }
                else if (op == "call") {
                    // builtin call: $name args...
                    if (instr.args.empty()) continue;
                    std::string name = instr.args[0].name;
                    if (name[0] == '$') name = name.substr(1);
                    // Map to runtime function
                    if (name == "octets") {
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
                    else if (name == "longueur") {
                        std::string arg = sanitize(instr.args[1].name);
                        out << "    " << sanitize(instr.dest) << " = rose_tampon_longueur(" << arg << ");\n";
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
                else if (op == "ret.u" || op == "ret" || op == "ret.z") {
                    out << "    return 0;\n";
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
