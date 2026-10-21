#!/usr/bin/env python3
"""Suite de couverture sémantique du compilateur AOT (`rose build`).

Métrique : quelle proportion des programmes valides s'exécute en AOT avec
une sémantique IDENTIQUE à l'interpréteur (sortie, code de retour, message
d'erreur), et les fonctionnalités non prises en charge doivent se manifester
soit par un rejet au compilable, soit par une divergence marquée ÉCHEC.

Quatre destins possibles pour un cas :
  RÉUSSI   — compilé et fidèle au caractère près, OU rejet/erreur corrects
  LIMITE   — divergence connue et documentée (liste LIMITES_CONNUES)
  REJET    — programme invalide : le compilable le refuse avec le bon motif
  NON-COMPILABLE — fonctionnalité hors du sous-ensemble AOT (gap mesuré)
"""
import os
import subprocess
import sys
import tempfile

MOTEUR = os.environ.get("ROSE_MOTEUR") or os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "..", "cpp", "rose")
DORES = os.path.join(os.path.dirname(os.path.abspath(__file__)), "dorés")
EXEMPLES = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                        "..", "..", "rose2", "exemples")
TMP = tempfile.mkdtemp(prefix="rose-aot-")

# Compilation portable (le moteur ne connaît plus de chemin absolu) : si GMP
# n'est pas au système, on pointe la toolchain de développement si présente.
ENV = os.environ.copy()
if "ROSE_GMP" not in ENV and os.path.exists("/tmp/opencode/gmp-linux/include"):
    ENV["ROSE_GMP"] = "/tmp/opencode/gmp-linux"

reussis = 0
rejets_ok = 0
limites = []
non_compilables = []
ecarts_connus = []
echoues = []

# Catalogue des écarts AOT mesurés (jalon M3.3bis — fidélité erreurs et
# effets). Un cas du catalogue qui échoue reste compté comme écart CONNU
# (rapporté mais non bloquant) ; un échec HORS catalogue est une régression.
ECARTS_CONNUS = {}


def _build(source, binaire):
    return subprocess.run([MOTEUR, "build", source, "-o", binaire],
                          capture_output=True, text=True, env=ENV)


def _rose_source(nom, code):
    """Écrit une source inline dans le TMP et rend son chemin (cas fidèles)."""
    p = os.path.join(TMP, nom + ".rose")
    with open(p, "w", encoding="utf-8") as f:
        f.write(code)
    return p


def cas_fidele(nom, source, attendu_stdout=None, dore=None, limite=False,
               stdin=None, args=(), cwd=None, attendu_code=0):
    """Le binaire AOT doit reproduire l'interpréteur : sortie, code, stderr."""
    global reussis
    attendu = open(os.path.join(DORES, dore)).read() if dore else attendu_stdout
    # Nom de binaire assaini (M4.2) : « notation symbolique » avec un espace
    # faisait exploser la ligne de commande g++ du build → faux « non compilable ».
    slug = "".join(c if c.isalnum() or c in "._-" else "_" for c in nom)
    binaire = os.path.join(TMP, slug)
    b = _build(source, binaire)
    if b.returncode != 0 or not os.path.exists(binaire):
        log = (b.stderr or b.stdout).strip().splitlines()
        non_compilables.append((nom, log[-1] if log else "non compilé"))
        return
    p = subprocess.run([binaire, *args], input=stdin, cwd=cwd,
                       capture_output=True, text=True)
    if p.returncode == attendu_code and p.stdout == attendu:
        reussis += 1
    elif limite:
        limites.append(nom)
    elif nom in ECARTS_CONNUS:
        ecarts_connus.append(nom)
    else:
        echoues.append((nom, p.returncode, attendu_code,
                        attendu[:160], p.stdout[:160], p.stderr[:120]))


def cas_rejet(nom, source=None, code=None, motif=""):
    """Programme invalide ou en erreur : rejet au compilable OU erreur
    d'exécution avec le même motif — les deux sont conformes. S'il s'en
    sort « comme si de rien n'était », c'est une divergence."""
    global rejets_ok
    slug = "".join(c if c.isalnum() or c in "._-" else "_" for c in nom)
    if source is None:
        source = os.path.join(TMP, f"rejet_{slug}.rose")
        with open(source, "w", encoding="utf-8") as fh:
            fh.write(code)
    binaire = os.path.join(TMP, f"rejet_{slug}")
    b = _build(source, binaire)
    if b.returncode != 0 or not os.path.exists(binaire):
        if motif in (b.stderr + b.stdout):
            rejets_ok += 1
        elif nom in ECARTS_CONNUS:
            ecarts_connus.append(nom)
        else:
            echoues.append((nom, "rejet sans motif", motif, "", "",
                            (b.stderr + b.stdout)[:160]))
        return
    # compilé → l'exécution doit produire l'erreur attendue (rc=1, motif)
    p = subprocess.run([binaire], capture_output=True, text=True)
    if p.returncode == 1 and motif in p.stderr and p.stdout == "":
        rejets_ok += 1
    elif nom in ECARTS_CONNUS:
        ecarts_connus.append(nom)
    else:
        echoues.append((nom, p.returncode, 1, f"stderr∋{motif!r}", p.stdout[:160],
                        p.stderr[:160]))


# ── 1. dorés stdout : fidélité caractère par caractère ──────────────
cas_fidele("t_exact", f"{DORES}/t_exact.rose", dore="t_exact_c.txt")
cas_fidele("t_machine", f"{DORES}/t_machine.rose", dore="out_c.txt")
cas_fidele("machine", f"{DORES}/machine.rose", dore="machine_c.txt")
# Gap connu et assumé : facto_iteratif(30) — accumulateur hors int64 depuis
# de petites valeurs. Promotion runtime overflow→GMP = jalon M3.4.
cas_fidele("signature", f"{EXEMPLES}/signature.rose", dore="sig_c.txt")
cas_fidele("alias", f"{DORES}/alias.rose", dore="alias_c.txt")
cas_fidele("t_si_imbr", f"{DORES}/t_si_imbr.rose", dore="t_si_imbr_c.txt")
cas_fidele("t_tampon", f"{DORES}/t_tampon.rose", dore="t_tampon_c.txt")
cas_fidele("t_flottant", f"{DORES}/t_flottant.rose", dore="t_flottant_c.txt")
cas_fidele("t_builtin", f"{DORES}/t_builtin.rose", dore="t_builtin_c.txt")
cas_fidele("t_liste", f"{DORES}/t_liste.rose", dore="t_liste_c.txt")
cas_fidele("v3_chaines", f"{DORES}/t_v3.rose", dore="t_v3_c.txt")

cas_fidele("performance", f"{EXEMPLES}/performance.rose",
           attendu_stdout="456574\n20899\n3.14159265358979323846264338327950288419716939937511\n664579\nvrai\n")
cas_fidele("notation symbolique", f"{DORES}/t_symboles.rose",
           attendu_stdout="14\n42\n3\nvrai\n3\n1.4142135623730951\n0.5\n33\n3.141592653589793\n12.566370614359172\nsymbole\n7\nsym\nrose\n12.566370614359172\n")
cas_fidele("mixtes rationnel⊛flottant", f"{DORES}/t_mixtes.rose",
           attendu_stdout="0.75\n0.05\n0.08333333333333331\n1.3333333333333333\n0.5\n0.75\n0.05\n0.08333333333333331\n1.3333333333333333\n0.7071067811865476\n0.25\n")
cas_fidele("tables", f"{DORES}/t_tables.rose",
           attendu_stdout="[" + "nom => Bob, age => 30" + "]\n2\nvrai\nfaux\nBob\n30\nvrai\n[nom => Bob]\n[nom]\n[Bob]\nfaux\nvrai\n")
cas_fidele("chronometre", f"{DORES}/t_chrono.rose", dore="t_chrono_c.txt")

# ── 2. effets du monde : stdin, argv, fichiers, sortie anticipée ────
_rep_v3 = tempfile.mkdtemp(prefix="rose_aot_v3_")
cas_fidele("lecture_stdin", f"{DORES}/lecture.rose",
           attendu_stdout="49\n2.5\n", stdin="7\n2.25\n")
cas_fidele("v3_arguments", f"{DORES}/t_v3_args.rose",
           attendu_stdout="2\nbêta\n", args=("alpha", "bêta"))
cas_fidele("v3_fichiers", f"{DORES}/t_v3_fich.rose",
           attendu_stdout="14\nligne2\nremplacé\n42\n", cwd=_rep_v3)
cas_fidele("v3_quitter", f"{DORES}/t_v3_quitter.rose",
           attendu_stdout="avant\n2\n", attendu_code=3)

# ── fidélité v3.13.1 : domaine rationnel mélangé (testeur génératif) ─
# Les correctifs du transpileur (opérateurs mixtes Z⊛Q, abs/min/max en Q,
# division euclidienne Q⊛Gd et Gd⊛Q) sont épinglés ici : toute régression
# de l'un d'eux ressort en ÉCHEC, plus en silence.
cas_fidele("q_domaine_mixte", _rose_source("q_domaine_mixte", """
$q = 23 / 6
>> $q + 2
>> 5 - $q
>> $q * 3
>> $q / 4
>> abs(0 - $q)
>> min($q, 1)
>> max(0 - $q, 0 - 3)
>> $q div 2
>> 7 div (1 / 2)
$g = 423243272715318256248 * 22272815071249072011497
>> $g div $q
>> abs(0 - 3 / 4) + max(1 / 8, $q)
"""), attendu_stdout="""5.8[3]
1.1[6]
11.5
0.958[3]
3.8[3]
1
-3
1
14
2459170211305701287898175296192056308195632
4.58[3]
""")

# ── écart connu n°1 : variable affectée seulement sur un chemin mort ─
# Découvert par le testeur génératif (graine 200051, 21/10/2026) et rendu
# visible par le correctif v3.13.1 (le cœur RIR et le JIT rejettent enfin
# la lecture sans valeur au lieu d'afficher le vide). FERMÉ EN v3.14 :
# le transpileur marque les variables dont la première écriture n'est pas
# dans le préfixe linéaire (drapeau _def_ + lecture rose_lit) et rend
# désormais la même erreur que le cœur interprété, au même endroit.
# Écart historiquement mesuré du 21/10 au 21/10/2026 — jamais caché.
cas_fidele("var_chemin_mort", _rose_source("var_chemin_mort", """
>> 43
si (faux) {
  $theta = (-9 / 2 + 27 / 4)
} sinon si (1.5 != (theta ^ -40.3744)) {
  >> 36
}
"""), attendu_stdout="43\n", attendu_code=1)

# v3.14 — la garde ne doit jamais gêner l'initialisée : variable écrite
# dans un branche, lue dans l'autre rendue atteignable — le drapeau est
# posé par la première branche et la lecture traverse la garde sans erreur
# (miroir exact des trois autres voies ; le lecteur hors branche, lui, est
# refusé par le vérificateur — cartes doubles connues, non concerné ici).
cas_fidele("var_branche_definie", _rose_source("var_branche_definie", """
$c = 5 > 3
si ($c) { $x = 7 } sinon { $x = 9 }
si ($c) { >> $x } sinon { >> $x }
"""), attendu_stdout="7\n", attendu_code=0)

# v3.14 — chaine() sur chaque domaine : identité pour chaine, « vrai »
# pour booleen (l'surcharges rose_chaine lèveraient une ambiguïté GMP).
cas_fidele("chaine_domaines", _rose_source("chaine_domaines", """
$s = "abc"
>> chaine($s)
>> chaine(5)
>> chaine(2.5)
>> chaine(1/3)
>> chaine(vrai)
>> longueur(chaine(faux))
"""), attendu_stdout="abc\n5\n2.5\n1/3\nvrai\n4\n", attendu_code=0)

# v3.14 — comparaison F⊛rationnel/grand : la spécification cmp_vals du cœur
# veut le flottant DOMINANT (double pur des deux côtés). L'ancien chemin
# convertissait le FLOTTEUR en mpq : SIGFPE de mpq_set_d dès que delta
# valait ±inf (0.0^…) — graine générative 20000000087. Miroir rétabli.
cas_fidele("cmp_flottant_dominant", _rose_source("cmp_flottant_dominant", """
$q = 1 / 2
si ($q >= ((0.0 - 1.0) ^ 100000)) { >> 1 } sinon { >> 0 }
si ($q + 0.5 <= 2.0 ^ 1) { >> "deux" } sinon { >> "un" }
"""), attendu_stdout="0\ndeux\n", attendu_code=0)

# v3.14 — ≈/?=/=~ : la spec cœur veut déjà le FLOTTANT dominant (double pur,
# tolérance 1e-9 relative). L'ancien chemin montait le FLOTTEUR en mpq :
# SIGFPE sur ±inf et résultats faux sur Q⊛F pathologiques. Q⊛Q reste exact.
cas_fidele("approx_flottant_dominant", _rose_source("approx_flottant_dominant", """
$q = 1 / 2
>> 355 / 113 ?= 3.14159265358979
>> ($q ≈ ((0.0 - 1.0) ^ 100000))
>> (1 / 3 ?= 2 / 6)
>> 0.1 ≈ 0.1
>> (7 ≈ (3 / 2))
"""), attendu_stdout="faux\nfaux\nvrai\nvrai\nfaux\n", attendu_code=0)

# v3.14 — min/max : la spec cœur promeut au domaine dominant (F>Q>Gd>Z) puis
# « c <= 0 ? x : y » — l'égalité comme NaN renvoient le PREMIER opérande, et
# un flottant en second opérande dominait autant qu'en premier (l'ancien
# chemin ne testait que args[1] : min(2, 1.5) cassait g++, min(NaN, 2)
# renvoyait 2). Miroir inline strict désormais.
cas_fidele("min_max_flottant_dominant", _rose_source("min_max_flottant_dominant", """
>> min(1/3, 0.4)
>> min(2, 1.5)
>> max(1/2, 0.2)
>> max(2, 1.9)
>> min(1.5, 0.4)
>> max(1/3, 1/4)
"""), attendu_stdout="0.3333333333333333\n1.5\n0.5\n2\n0.4\n0.[3]\n", attendu_code=0)

# v3.14 — le transpileur préfixe tous les identifiants générés par r_ : un
# nom Rose peut être un mot-clé C++ ou collisionner l'API du runtime sans
# jamais casser g++ (contrat « émettre ou rejeter poliment »).
cas_fidele("nom_mot_cle_cpp", _rose_source("nom_mot_cle_cpp", """
fn double(n: entier) → entier { retour n * 3 }
$int = 5
>> double($int)
$classe = "rose"
>> longueur($classe)
"""), attendu_stdout="15\n4\n", attendu_code=0)

# v3.14.1 — les paramètres sont des variables du corps : longueur() compte
# des CARACTÈRES (pas des octets), les listes voyagent par référence (le
# joindre() de l'appelée est visible de l'appelante, comme le shared_ptr de
# l'interprète), et les éléments d'une liste<chaine> se lisent en textuel.
# Trouvé par le lexeur auto-hébergé de M4 se lexant lui-même (« ≈ » près).
cas_fidele("param_chaine_utf8", _rose_source("param_chaine_utf8", """
fn n_car(t: chaine) -> entier { retour longueur(t) }
fn ecartèle(t: chaine) -> chaine {
  $o = ""
  $j = 0
  $n = longueur(t)
  tant_que ($j < $n) {
    $o = $o + t[$j] + "-"
    $j = $j + 1
  }
  retour $o
}
$s = "abé"
>> longueur($s)
>> n_car($s)
>> ecartèle($s)
"""), attendu_stdout="3\n3\na-b-é-\n", attendu_code=0)

cas_fidele("param_liste_partage", _rose_source("param_liste_partage", """
fn pousse(l: liste<entier>) { joindre($l, 99) }
fn n(l: liste<entier>) -> entier { retour longueur(l) }
fn passe(l: liste<entier>) { pousse($l) }
$l = [1, 2]
pousse($l)
>> $l
passe($l)
>> $l
>> n($l)
"""), attendu_stdout="[1, 2, 99]\n[1, 2, 99, 99]\n4\n", attendu_code=0)

cas_fidele("param_liste_chaine", _rose_source("param_liste_chaine", """
fn n_liste(l: liste<entier>) -> entier { retour longueur(l) }
fn premier(l: liste<entier>) -> entier { retour l[0] }
fn mots(l: liste<chaine>) -> chaine { retour l[1] }
$l = [7, 8, 9]
$w = ["alpha", "bêta"]
>> n_liste($l)
>> premier($l)
>> mots($w)
"""), attendu_stdout="3\n7\nbêta\n", attendu_code=0)

# ── 3. erreurs et rejets : le compilable doit dire non, ou l'exécution
#       rendre le même message que l'interpréteur ────────────────────
cas_rejet("nom_reserve", f"{DORES}/nom_reserve.rose", motif="réservé par le moteur")
cas_rejet("v3_index_chaine", f"{DORES}/t_v3_idx.rose",
          motif="index hors des limites de la chaîne")
cas_rejet("v3_ordre", f"{DORES}/t_v3_ordre.rose",
          motif="ordre attend une chaîne d'un seul caractère")
cas_rejet("v3_bornes", f"{DORES}/t_v3_souschaine.rose",
          motif="sous_chaine : bornes invalides")
cas_rejet("v3_fichier_absent", f"{DORES}/t_v3_absent.rose",
          motif="lecture du fichier impossible")
for nom, code, motif in [
    ("err_div_zero_const", "$x = 5 / 0\n>> $x\n", "Division par zéro dans une constante"),
    ("err_div_zero_run", "$z = 3 - 3\n>> 5 / $z\n", "division par zéro"),
    ("err_factorielle_neg", ">> factorielle(-3)\n", "factorielle d'un négatif"),
    ("err_index", "$l = [1, 2]\n>> $l[5]\n", "index hors des limites"),
    ("err_var_inconnue", ">> zoup\n", "Variable 'zoup' utilisée sans valeur"),
    ("err_type", '$x = 1\n$x = "t"\n', "on ne peut pas y stocker du chaine"),
    ("err_fonction", ">> inconnue(2)\n", "Fonction 'inconnue' inconnue"),
    ("err_syntaxe", "si 3 { >> 1 }\n", "Attendu '('"),
    ("err_modulo_zero", ">> 5 % 0\n", "modulo par zéro"),
    ("err_chaine_entier", '>> entier("abc")\n', "conversion chaine → entier impossible"),
    ("err_affect_flotteur", "$x = 0\n$x = 1.5\n", "on ne peut pas y stocker"),
    # v3.13.1 : les gardes ajoutées par le testeur génératif — la division
    # flottante par zéro doit refuser dans les deux voies (pas de ±inf),
    # et une variable que le vérificateur ne peut typer sur un seul domaine
    # doit être rejetée poliment, jamais compilée à l'aveugle.
    ("err_div_zero_flottant", "$z = 5 - 5\n>> 1.5 / $z\n", "division par zéro"),
    ("err_div_euclide_flottant", "$z = 5 - 5\n>> 7.5 div $z\n",
     "division euclidienne par zéro"),
    ("err_div_euclide_zero", "$z = 5 - 5\n>> 7 div $z\n",
     "division euclidienne par zéro"),
    ("var_domaine_changeant", "si (faux) {\n  $gamma = 40.8851\n}\n"
     "$gamma = 1 / 2\n>> $gamma\n", "change de domaine"),
    ("err_type_deux_branches", "si (1 > 2) {\n  $gamma = chaine(4)\n"
     "} sinon si (2 > 3) {\n  si (faux) {\n    $delta = 1\n  } sinon {\n"
     "    $gamma = 40.8851\n  }\n}\n>> 7\n", "on ne peut pas y stocker"),
    # v3.14.1 — deux classes d'inreprésentable en C++ désormais nommées
    # au lieu d'aborder ou de casser g++ : le chemin sans « retour » d'une
    # fonction à retour déclaré (l'interprète rend la valeur vidée, qui se
    # voit : « rien() == "" » rend faux), et la table qui franchit une
    # frontière de fonction (l'interprète la partage, la copie la trahirait).
    ("err_chemin_sans_retour", "fn rien() -> chaine { $x = 1 }\n$l = [1]\n>> rien()\n",
     "chemin sans « retour »"),
    ("err_param_table_appel", "fn relais(t: table) -> chaine { retour \"?\" }\n"
     "$t = table()\n>> relais($t)\n", "table ou cellule dynamique"),
    ("err_retour_liste_aliassee", "fn passe(l: liste<entier>) -> liste<entier> { retour $l }\n"
     "$v = [3]\n>> passe($v)\n", "paramètre-liste"),
]:
    cas_rejet(nom, code=code, motif=motif)

# ── rapport de couverture ───────────────────────────────────────────
for nom, err in non_compilables:
    print(f"NON-COMPILABLE {nom}: {err}")
for nom in limites:
    print(f"LIMITE {nom} (promotion overflow→GMP non implémentée en AOT — M3.4)")
for nom in sorted(set(ecarts_connus)):
    print(f"ÉCART CONNU {nom}: {ECARTS_CONNUS[nom]}")
for nom, rc, rc_att, attendu, obtenu, stderr in echoues:
    print(f"ÉCHEC {nom}: rc={rc} (attendu {rc_att})\n  attendu: {attendu!r}\n"
          f"  obtenu : {obtenu!r}\n  stderr : {stderr!r}")

total = (reussis + rejets_ok + len(limites) + len(non_compilables)
         + len(set(ecarts_connus)) + len(echoues))
print(f"\nAOT : {reussis} fidèles + {rejets_ok} rejets corrects sur {total} cas, "
      f"{len(limites)} limites, {len(non_compilables)} non compilables, "
      f"{len(set(ecarts_connus))} écarts connus, {len(echoues)} régressions")
sys.exit(1 if echoues else 0)
