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
        case RirQ: return "rose_qd";
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

// Vrai si une constante entière doit vivre en grand entier : hors int64,
// ou aux marges de l'arithmétique machine (|v| >= 2^62). L'interprète tient
// tout en GMP exact ; les valeurs proches des bornes débordent dès qu'on
// les combine (a-1, a*a, 2*a). En dessous du seuil, l'int64 rapide suffit
// aux programmes usuels ; au-dessus, on choisit l'exactitude GMP.
static bool cst_promu_gd(const std::string &txt) {
    if (cst_hors_int64(txt)) return true;
    size_t i = (txt[0] == '-' || txt[0] == '+') ? 1 : 0;
    unsigned __int128 v = 0;
    for (; i < txt.size(); i++) {
        if (txt[i] < '0' || txt[i] > '9') return false;
        if (v > (((__int128 unsigned)1 << 126))) return true;
        v = v * 10 + (unsigned)(txt[i] - '0');
    }
    return v >= ((__int128 unsigned)1 << 62);
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
            if (op.ty == RirQ || (txt.find('/') != std::string::npos && txt.find_first_of("0123456789-") == 0
                    && txt.find_first_not_of("0123456789-/") == std::string::npos))
                return "rose_qd(\"" + txt + "\")";
            if (cst_promu_gd(txt)) return "rose_gd(\"" + txt + "\")";
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
                    // Rationnels et opérations exactes
                    if (instr.op == "cst.q" || instr.op == "div.q" || instr.op == "div.z" ||
                        instr.op == "add.q" || instr.op == "sub.q" || instr.op == "mul.q")
                        temp_types[instr.dest] = RirQ;
                    // Index dans une chaîne → élément chaîne
                    if (instr.op == "index" && !instr.args.empty()) {
                        std::string bn = sanitize(instr.args[0].name);
                        RirType bty = RirU;
                        auto bt1 = temp_types.find(instr.args[0].name);
                        if (bt1 != temp_types.end()) bty = bt1->second;
                        auto bt2 = global_types.find(bn);
                        if (bty == RirU && bt2 != global_types.end()) bty = bt2->second;
                        if (bty == RirS) temp_types[instr.dest] = RirS;
                    }
                    // Constante entière hors int64 → grand entier
                    if (instr.op == "cst.z" && !instr.args.empty() &&
                        instr.args[0].kind == RirOp::Const &&
                        cst_promu_gd(instr.args[0].name.rfind("@c", 0) == 0 ?
                                     instr.args[0].name.substr(2) : instr.args[0].name))
                        temp_types[instr.dest] = RirGd;
                }
                // Collect global variables from affect
                // Appels à grands résultats : typer avant l'inférence globale
                if (instr.op == "call" && !instr.dest.empty() && instr.dest[0] == '%' &&
                    !instr.args.empty()) {
                    std::string cn = instr.args[0].name;
                    if (!cn.empty() && cn[0] == '$') cn = cn.substr(1);
                    if (cn == "fraction") temp_types[instr.dest] = RirQ;
                    if (cn == "fibonacci" || cn == "factorielle" || cn == "puissance")
                        temp_types[instr.dest] = RirGd;
                }
                // Propagation grand entier pendant la collecte
                if (!instr.dest.empty() && instr.dest[0] == '%' && temp_types.find(instr.dest) == temp_types.end() &&
                    (instr.op == "add.z" || instr.op == "sub.z" || instr.op == "mul.z" || instr.op == "neg.z" ||
                     instr.op == "pow" || instr.op.rfind("divf", 0) == 0 || instr.op.rfind("rem.z", 0) == 0)) {
                    bool gd = false;
                    for (size_t ai = 0; ai < instr.args.size(); ai++) {
                        std::string an = instr.args[ai].name;
                        if (instr.args[ai].kind == RirOp::Const) {
                            std::string t = an;
                            if (t.rfind("@c", 0) == 0) t = t.substr(2);
                            if (cst_promu_gd(t)) gd = true;
                            continue;
                        }
                        if (temp_types.count(an) && temp_types[an] == RirGd) gd = true;
                        if (global_types.count(sanitize(an)) && global_types[sanitize(an)] == RirGd) gd = true;
                    }
                    if (gd) temp_types[instr.dest] = RirGd;
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
                        if (temp_types.count(src_temp) && temp_types[src_temp] == RirQ) ty = RirQ;
                        if (temp_types.count(src_temp) && temp_types[src_temp] == RirS &&
                            (ty == RirU || ty == RirZ)) ty = RirS;
                    }
                    auto rang = [](RirType t) { return t == RirGd ? 5 : t == RirQ ? 4 : t == RirListe ? 3 : t == RirS ? 2 : t == RirF ? 2 : t == RirZ ? 1 : 0; };
                    auto gt = global_types.find(g);
                    if (gt == global_types.end()) global_types[g] = ty;
                    else if (ty != RirU && rang(ty) > rang(gt->second) &&
                             !(gt->second == RirS && ty != RirS && rang(ty) < 3)) gt->second = ty;
                }
            }
        }

        // Declare global variables
        for (auto &kv : global_types) {
            // Un paramètre éclipse le global de même nom (comme l'interprète).
            bool ombre = false;
            for (auto &p : f.params) if (sanitize(p.first) == kv.first) ombre = true;
            if (ombre) continue;
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
                        else if (name == "fraction") ty = RirQ;
                        else if (name == "fibonacci" || name == "factorielle" || name == "puissance") ty = RirGd;
                        else if (name == "millisecondes" || name == "nb_arguments" || name == "entier" || name == "tampon_case" ||
                                 name == "nb_chiffres" || name == "compter_premiers" ||
                                 name == "pgcd" || name == "ppcm" || name == "hasard") ty = RirZ;
                        else if (name == "argument" || name == "chaine" || name == "nombre_pi") ty = RirS;
                        else if (name == "partie_entiere") ty = RirZ;
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
                         instr.op == "neg.z" || instr.op == "pow" || instr.op == "add" ||
                         instr.op.rfind("divf", 0) == 0 || instr.op.rfind("rem.z", 0) == 0) &&
                        (ty == RirZ || ty == RirU)) {
                        for (size_t ai = 0; ai < instr.args.size(); ai++) {
                            std::string an = instr.args[ai].name;
                            if (instr.args[ai].kind == RirOp::Const) {
                                std::string t = an;
                                if (t.rfind("@c", 0) == 0) t = t.substr(2);
                                if (cst_promu_gd(t)) { ty = RirGd; break; }
                                continue;
                            }
                            if (temp_types.count(an) && temp_types[an] == RirGd) { ty = RirGd; break; }
                            if (global_types.count(sanitize(an)) && global_types[sanitize(an)] == RirGd) { ty = RirGd; break; }
                        }
                    }
                    if (temp_types.find(instr.dest) == temp_types.end() ||
                        (temp_types[instr.dest] != RirListe && temp_types[instr.dest] != RirGd &&
                         temp_types[instr.dest] != RirQ &&
                         !(temp_types[instr.dest] == RirS && ty == RirU)))
                        temp_types[instr.dest] = ty;
                }
            }
        }
        // Type d'un opérande (temp, global ou constante).
        std::map<std::string, RirType> prod_ty;
        for (auto &b : f.blocs) for (auto &i : b.instrs)
            if (!i.dest.empty()) prod_ty[i.dest] = i.dest_ty;
        auto ty_of = [&](const RirOp &o) -> RirType {
            auto it = temp_types.find(o.name);
            if (it != temp_types.end() && it->second != RirU) return it->second;
            std::string s = sanitize(o.name);
            auto gt = global_types.find(s);
            if (gt != global_types.end() && gt->second != RirU) return gt->second;
            auto pt = prod_ty.find(o.name);
            if (pt != prod_ty.end() && pt->second != RirU) return pt->second;
            return o.ty;
        };
        for (auto &t : temps) {
            RirType ty = temp_types.count(t) ? temp_types[t] : RirU;            std::string ct = cpp_type(ty);
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
                    if (cst_promu_gd(val))
                        out << "    " << sanitize(instr.dest) << " = rose_gd(\"" << val << "\");\n";
                    else
                        out << "    " << sanitize(instr.dest) << " = " << val << ";\n";
                }
                else if (op == "cst.q") {
                    std::string txt;
                    if (!instr.args.empty() && instr.args[0].kind == RirOp::Const) {
                        txt = instr.args[0].name;
                        if (txt.rfind("@c", 0) == 0) txt = txt.substr(2);
                    }
                    out << "    " << sanitize(instr.dest) << " = rose_qd(\"" << txt << "\");\n";
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
                    RirType ety = ty_of(instr.args[1]);
                    if (instr.args[1].kind == RirOp::Const && instr.args[1].ty != RirS &&
                        instr.args[1].name.rfind("@c", 0) == 0) ety = RirZ;
                    if (ety == RirS)
                        out << "    rose_liste_push(" << list_var << ", " << elem << ".c_str());\n";
                    else if (ety == RirGd)
                        out << "    rose_liste_push_gd(" << list_var << ", " << elem << ");\n";
                    else if (ety == RirQ)
                        out << "    rose_liste_push_q(" << list_var << ", " << elem << ");\n";
                    else if (ety == RirF)
                        out << "    rose_liste_push_flt(" << list_var << ", " << elem << ");\n";
                    else
                        out << "    rose_liste_push_int(" << list_var << ", " << elem << ");\n";
                }
                else if (op == "index") {
                    std::string dest = sanitize(instr.dest);
                    std::string base = sanitize(instr.args[0].name);
                    std::string idx = sanitize(instr.args[1].name);
                    RirType bty = temp_types.count(instr.args[0].name) ? temp_types[instr.args[0].name] : RirU;
                    if (global_types.count(base)) bty = global_types[base];
                    if (bty == RirListe) {
                        // Liste : élément lu comme chaîne ou entier selon la destination.
                        RirType dty = temp_types.count(instr.dest) ? temp_types[instr.dest] : RirU;
                        if (dty == RirS)
                            out << "    " << dest << " = rose_liste_get_str(" << base << ", " << idx << ");\n";
                        else
                            out << "    " << dest << " = rose_a_entier(rose_liste_get_str(" << base << ", " << idx << "));\n";
                    }
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
                    else if (ty == RirQ) out << "    rose_afficher_q(" << arg << ");\n";
                    else if (ty == RirGd) out << "    rose_afficher_gd(" << arg << ");\n";
                    else if (ty == RirF) out << "    rose_afficher_float(" << arg << ");\n";
                    else if (ty == RirB) out << "    rose_afficher_bool(" << arg << ");\n";
                    else if (ty == RirS) out << "    rose_afficher_str(" << arg << ".c_str());\n";
                    else if (ty == RirTab) out << "    rose_afficher_tampon_longueur(" << arg << ");\n";
                    else if (ty == RirListe) out << "    rose_afficher_liste(" << arg << ");\n";
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
                        RirType aty = ty_of(instr.args[1]);
                        if (instr.args[1].kind == RirOp::Const && instr.args[1].name.rfind("@c", 0) == 0)
                            out << "    " << sanitize(instr.dest) << " = " << instr.args[1].name.substr(2) << ";\n";
                        else if (aty == RirF)
                            out << "    " << sanitize(instr.dest) << " = rose_entier_f(" << arg << ");\n";
                        else if (aty == RirQ)
                            out << "    " << sanitize(instr.dest) << " = rose_entier_q(" << arg << ");\n";
                        else if (aty == RirS || aty == RirU)
                            out << "    " << sanitize(instr.dest) << " = rose_entier(" << arg << ".c_str());\n";
                        else
                            out << "    " << sanitize(instr.dest) << " = " << arg << ";\n";
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
                    else if (name == "nombre_pi") {
                        std::string a0 = sanitize(instr.args[1].name);
                        out << "    " << sanitize(instr.dest) << " = rose_nombre_pi(" << a0 << ");\n";
                    }
                    else if (name == "partie_entiere") {
                        std::string a0 = sanitize(instr.args[1].name);
                        RirType aty = ty_of(instr.args[1]);
                        if (aty == RirF)
                            out << "    " << sanitize(instr.dest) << " = rose_partie_entiere_f(" << a0 << ");\n";
                        else if (aty == RirQ)
                            out << "    " << sanitize(instr.dest) << " = rose_partie_entiere_q(" << a0 << ");\n";
                        else
                            out << "    " << sanitize(instr.dest) << " = " << a0 << ";\n";
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
                    else if (name == "fraction") {
                        std::string a0 = cpp_operand(instr.args[1], temp_types);
                        std::string a1 = cpp_operand(instr.args[2], temp_types);
                        out << "    " << sanitize(instr.dest) << " = rose_qd(rose_qd(" << a0 << ") / rose_qd(" << a1 << "));\n";
                    }
                    else if (name == "tampon_case") {
                        std::string a0 = sanitize(instr.args[1].name);
                        std::string a1 = sanitize(instr.args[2].name);
                        out << "    " << sanitize(instr.dest) << " = rose_tampon_case(" << a0 << ", " << a1 << ");\n";
                    }
                    else if (name == "tampon_pose") {
                        std::string a0 = sanitize(instr.args[1].name);
                        std::string a1 = sanitize(instr.args[2].name);
                        std::string a2 = sanitize(instr.args[3].name);
                        out << "    rose_tampon_pose(" << a0 << ", " << a1 << ", " << a2 << ");\n";
                    }
                    else if (name == "joindre") {
                        // joindre(list, elem): push elem to list
                        std::string list_var = sanitize(instr.args[1].name);
                        std::string elem = sanitize(instr.args[2].name);
                        if (instr.args[2].kind == RirOp::Const) {
                            std::string s = instr.args[2].name;
                            if (s.rfind("@c", 0) == 0) {
                                out << "    rose_liste_push_int(" << list_var << ", " << s.substr(2) << ");\n";
                                continue;
                            }
                            std::string escaped;
                            for (char c : s) {
                                if (c == '"') escaped += "\\\"";
                                else if (c == '\\') escaped += "\\\\";
                                else escaped += c;
                            }
                            out << "    rose_liste_push(" << list_var << ", \"" << escaped << "\".c_str());\n";
                        } else {
                            // elem est un temp : pousser selon son type.
                            RirType ety = ty_of(instr.args[2]);
                            if (ety == RirGd)
                                out << "    rose_liste_push_gd(" << list_var << ", " << elem << ");\n";
                            else if (ety == RirQ)
                                out << "    rose_liste_push_q(" << list_var << ", " << elem << ");\n";
                            else if (ety == RirF)
                                out << "    rose_liste_push_flt(" << list_var << ", " << elem << ");\n";
                            else if (ety == RirZ)
                                out << "    rose_liste_push_int(" << list_var << ", " << elem << ");\n";
                            else
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
                        RirType aty = ty_of(instr.args[1]);
                        if (aty == RirQ)
                            out << "    " << sanitize(instr.dest) << " = rose_texte_q(" << arg << ");\n";
                        else if (aty == RirF)
                            out << "    " << sanitize(instr.dest) << " = rose_texte_flt(" << arg << ");\n";
                        else if (aty == RirGd)
                            out << "    " << sanitize(instr.dest) << " = std::string(" << arg << ".get_str().c_str());\n";
                        else if (aty == RirZ)
                            out << "    " << sanitize(instr.dest) << " = std::to_string(" << arg << ");\n";
                        else
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
                else if (op == "not.b") {
                    out << "    " << sanitize(instr.dest) << " = !" << sanitize(instr.args[0].name) << ";\n";
                }
                else if (op == "and.b") {
                    out << "    " << sanitize(instr.dest) << " = (" << sanitize(instr.args[0].name)
                        << " && " << sanitize(instr.args[1].name) << ");\n";
                }
                else if (op == "or.b") {
                    out << "    " << sanitize(instr.dest) << " = (" << sanitize(instr.args[0].name)
                        << " || " << sanitize(instr.args[1].name) << ");\n";
                }
                else if (op.rfind("cmp.", 0) == 0) {
                    // cmp.lt.z %t4 %k %t3 → t4 = (k < t3)
                    std::string dest = sanitize(instr.dest);
                    std::string a0 = sanitize(instr.args[0].name);
                    std::string a1 = sanitize(instr.args[1].name);
                    RirType t0 = ty_of(instr.args[0]), t1 = ty_of(instr.args[1]);
                    bool q0 = (t0 == RirQ), q1 = (t1 == RirQ);
                    // Approximation ≈ / ?= / =~ : tolérance 1e-9 relative (miroir).
                    if (op == "cmp.approx") {
                        bool fla = (t0 == RirF || t1 == RirF);
                        if (q0 || q1)
                            out << "    " << dest << " = rose_approx_q(rose_qd(" << a0 << "), rose_qd(" << a1 << "), " << (fla ? 1 : 0) << ");\n";
                        else if (fla)
                            out << "    " << dest << " = rose_approx_f(" << a0 << ", " << a1 << ");\n";
                        else
                            out << "    " << dest << " = (" << a0 << " == " << a1 << ");\n";
                        continue;
                    }
                    // Comparison operator
                    std::string cmpop = "<";
                    if (op.find(".lt.") != std::string::npos) cmpop = "<";
                    else if (op.find(".le.") != std::string::npos) cmpop = "<=";
                    else if (op.find(".gt.") != std::string::npos) cmpop = ">";
                    else if (op.find(".ge.") != std::string::npos) cmpop = ">=";
                    else if (op.find(".eq.") != std::string::npos) cmpop = "==";
                    else if (op.find(".ne.") != std::string::npos) cmpop = "!=";
                    if (q0 || q1) {
                        // Comparaison exacte rationnel vs entier/flottant.
                        std::string call = "rose_q_cmp(rose_qd(" + a0 + "), rose_qd(" + a1 + ")) " +
                                          (cmpop == "<" ? "< 0" : cmpop == "<=" ? "<= 0" :
                                           cmpop == ">" ? "> 0" : cmpop == ">=" ? ">= 0" :
                                           cmpop == "==" ? "== 0" : "!= 0");
                        out << "    " << dest << " = (" << call << ");\n";
                    } else if ((t0 == RirTab || t1 == RirTab) && (cmpop == "==" || cmpop == "!=")) {
                        // Tampons : comparaison par contenu (pas par pointeur).
                        out << "    " << dest << " = (*" << a0 << " " << cmpop << " *" << a1 << ");\n";
                    } else {
                        out << "    " << dest << " = (" << a0 << " " << cmpop << " " << a1 << ");\n";
                    }
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
                else if (op == "div.z" || op == "div.q" || op == "div.z.z" || op == "div.q.q") {
                    // Division exacte de Rose : flottant sinon RATIONNEL.
                    std::string dest = sanitize(instr.dest);
                    std::string a0 = sanitize(instr.args[0].name);
                    std::string a1 = sanitize(instr.args[1].name);
                    out << "    " << dest << " = rose_q_div(rose_qd(" << a0 << "), rose_qd(" << a1 << "));\n";
                }
                else if (op.rfind("divf", 0) == 0) {
                    // Division euclidienne (plancher).
                    std::string dest = sanitize(instr.dest);
                    std::string a0 = sanitize(instr.args[0].name);
                    std::string a1 = sanitize(instr.args[1].name);
                    RirType t0 = ty_of(instr.args[0]), t1 = ty_of(instr.args[1]);
                    if (t0 == RirF || t1 == RirF)
                        out << "    " << dest << " = std::floor(" << a0 << " / " << a1 << ");\n";
                    else if (t0 == RirGd || t1 == RirGd)
                        out << "    " << dest << " = rose_div_florentin_gd(" << a0 << ", " << a1 << ");\n";
                    else
                        out << "    " << dest << " = rose_div_florentin(" << a0 << ", " << a1 << ");\n";
                }
                else if (op.rfind("add.q", 0) == 0 || op.rfind("sub.q", 0) == 0 || op.rfind("mul.q", 0) == 0) {
                    std::string dest = sanitize(instr.dest);
                    std::string a0 = sanitize(instr.args[0].name);
                    std::string a1 = sanitize(instr.args[1].name);
                    char o = op[0] == 'a' ? '+' : op[0] == 's' ? '-' : '*';
                    out << "    " << dest << " = " << a0 << " " << o << " " << a1 << ";\n";
                }
                else if (op == "pow") {
                    std::string dest = sanitize(instr.dest);
                    std::string a0 = sanitize(instr.args[0].name);
                    std::string a1 = sanitize(instr.args[1].name);
                    out << "    " << dest << " = rose_puissance_gd(" << a0 << ", " << a1 << ");\n";
                }
                else if (op.rfind("rem.z", 0) == 0) {
                    // Modulo à la Rose : plancher (miroir mpz_fdiv_r).
                    std::string dest = sanitize(instr.dest);
                    std::string a0 = sanitize(instr.args[0].name);
                    std::string a1 = sanitize(instr.args[1].name);
                    if (ty_of(instr.args[0]) == RirGd || ty_of(instr.args[1]) == RirGd)
                        out << "    " << dest << " = rose_rem_florentin_gd(" << a0 << ", " << a1 << ");\n";
                    else
                        out << "    " << dest << " = rose_rem_florentin(" << a0 << ", " << a1 << ");\n";
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
