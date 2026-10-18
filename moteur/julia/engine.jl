# ═══════════════ Rose — moteur en Julia 1.x (BigInt / Rational exacts) ═══════
# Frontend Julia pur (lexeur, parseur), interpréteur direct.
# Helpers de calcul/affichage identiques à la cible Julia v2 (parité caractères).
# Usage : julia engine.jl prog.rose   |   julia engine.jl version | info
import Libdl

rose_erreur(msg) = (flush(stdout); println(stderr, "Rose : erreur — ", msg); exit(1))

# ── runtime (copié de la cible Julia v2 — parité caractère à caractère) ──

_fact(n::Integer) = n < 0 ? rose_erreur("factorielle d'un négatif : impossible") :
    n <= 1 ? big(1) : _arbre(BigInt(2), BigInt(n))
function _arbre(i, j)
    i > j && return BigInt(1)
    i == j && return i
    m = div(i + j, 2)
    return _arbre(i, m) * _arbre(m + 1, j)
end

function _fib(n::Integer)
    n < 0 && rose_erreur("fibonacci négatif : impossible")
    a, b = BigInt(0), BigInt(1)
    for c in string(BigInt(n), base = 2)
        a, b = a * (2b - a), a * a + b * b
        if c == '1'; a, b = b, a + b; end
    end
    return a
end

_pgcd(a, b) = gcd(BigInt(a), BigInt(b))
_ppcm(a, b) = lcm(BigInt(a), BigInt(b))

_mulmod(a, b, m) = mod(BigInt(a) * BigInt(b), m)
_powmod(b0, e0, m) = powermod(BigInt(b0), BigInt(e0), BigInt(m))
function _prem(n::Integer)
    ni = BigInt(n)
    ni < 2 && return false
    for p in (2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37)
        if mod(ni, p) == 0; return ni == p; end
    end
    d = ni - 1; r = 0
    while mod(d, 2) == 0; d = div(d, 2); r += 1; end
    for a in (2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37)
        x = _powmod(a, d, ni)
        (x == 1 || x == ni - 1) && continue
        temoin = false
        for _ in 1:(r - 1)
            x = _mulmod(x, x, ni)
            if x == ni - 1; temoin = true; break; end
        end
        temoin || return false
    end
    return true
end
_prem(n) = _prem(BigInt(n))

_mpz_vers_d(z::BigInt) = (try Float64(z); catch; (z < 0 ? -Inf : Inf); end)
function _vers_d(x)
    x isa AbstractFloat && return Float64(x)
    x isa Rational && return _mpz_vers_d(numerator(x)) / _mpz_vers_d(denominator(x))
    return _mpz_vers_d(BigInt(x))
end

function _div(a, b)
    b == 0 && rose_erreur("division par zéro")
    if a isa Integer && b isa Integer
        return Rational{BigInt}(BigInt(a), BigInt(b))
    end
    return _vers_d(a) / _vers_d(b)
end
function _mod(a, b)
    (a isa AbstractFloat || b isa AbstractFloat || a isa Rational || b isa Rational) &&
        rose_erreur("Le modulo exige deux entiers")
    b == 0 && rose_erreur("modulo par zéro")
    return mod(a, b)
end

function _divf(a, b)
    b == 0 && rose_erreur("division euclidienne par zéro")
    (a isa AbstractFloat || b isa AbstractFloat) && return Float64(floor(a / b))
    qa = a isa Rational ? Rational{BigInt}(numerator(a), denominator(a)) : Rational{BigInt}(BigInt(a), 1)
    qb = b isa Rational ? Rational{BigInt}(numerator(b), denominator(b)) : Rational{BigInt}(BigInt(b), 1)
    q = qa / qb
    return fld(numerator(q), denominator(q))
end

_pow_libm(x::Float64, y::Float64) = ccall((:pow, Base.libm_name), Float64, (Float64, Float64), x, y)

function _powr(a, e)
    # miroir du moteur C++ : exposant flottant → pow ; base flottante → pow ;
    # sinon exposant entier/rationnel tronqué (vers_z = plancher) puis puissance exacte
    e isa AbstractFloat && return _pow_libm(_vers_d(a), Float64(e))
    a isa AbstractFloat && return _pow_libm(a, _vers_d(e))
    if e isa Rational && denominator(e) != 1
        e = fld(numerator(e), denominator(e))
    end
    be = BigInt(e)
    if be > typemax(Int64) || be < typemin(Int64)
        rose_erreur("exposant démesuré (la puissance exploserait au-delà de tout calcul)")
    end
    ei = Int(be)
    if ei < 0
        a == 0 && rose_erreur("0^exposant négatif")
        baseq = a isa Rational ? Rational{BigInt}(BigInt(numerator(a)), BigInt(denominator(a))) :
                Rational{BigInt}(BigInt(a))
        return baseq^ei
    end
    if a isa Integer
        return BigInt(a)^ei
    end
    return a^ei
end

_approche(a, b) = (a isa AbstractFloat || b isa AbstractFloat) ?
    abs(a - b) <= 1e-9 * max(abs(a), abs(b), 1.0) : a == b

function _compter_premiers(n::Integer)
    n < 2 && return BigInt(0)
    n > typemax(Int) - 1 && rose_erreur("crible trop grand")
    ni = Int(n)
    crible = falses(ni + 1)
    compte = BigInt(0)
    for i in 2:ni
        if !crible[i]
            compte += 1
            i <= sqrt(ni) && (crible[i*i:i:ni] .= true)
        end
    end
    return compte
end

function _pi(chiffres::Integer)
    (chiffres <= 0 || chiffres > 2000000) && rose_erreur("décimales de π hors limites (1..2000000)")
    S = Int(chiffres) + 40
    E = big(10)^S
    function arctan_rec(x)
        x2 = BigInt(x)^2
        p = BigInt(x)
        terme = div(E, p)
        s = terme
        k, signe = 1, -1
        while true
            p *= x2
            terme = div(E, p * (2k + 1))
            terme == 0 && break
            s = signe < 0 ? s - terme : s + terme
            signe = -signe
            k += 1
        end
        return s
    end
    piE = 16 * arctan_rec(5) - 4 * arctan_rec(239)
    brut = string(piE)
    dec = brut[2:min(end, 1 + Int(chiffres))]
    arrondir = length(brut) > 1 + Int(chiffres) && brut[2 + Int(chiffres)] >= '5'
    entier = parse(BigInt, string(brut[1]))
    if arrondir
        d = parse(BigInt, dec) + 1
        ds = string(d)
        if length(ds) > length(dec); entier += 1; ds = "0"; end
        dec = lpad(ds, length(dec), '0')
    end
    return "$(entier).$(dec)"
end

function _entier_strict(s::AbstractString)
    m = match(r"^[+-]?[0-9]+$", String(s))
    m === nothing && return nothing
    return parse(BigInt, String(s))
end
function _strtod_prefix(t::AbstractString)
    t = String(t)
    i = (length(t) > 0 && (t[1] == '+' || t[1] == '-')) ? 2 : 1
    j = i
    while j <= length(t) && '0' <= t[j] <= '9'; j += 1; end
    eu = j > i
    if j <= length(t) && t[j] == '.'
        j += 1
        k = j
        while j <= length(t) && '0' <= t[j] <= '9'; j += 1; end
        eu = eu || j > k
    end
    if !eu
        b = lowercase(t)
        startswith(b, "inf") && return (t[1] == '-' ? -Inf : Inf)
        startswith(b, "nan") && return NaN
        t == "infini" && return Inf
        return nothing
    end
    if j <= length(t) && (t[j] == 'e' || t[j] == 'E')
        m2 = j + 1
        m2 <= length(t) && (t[m2] == '+' || t[m2] == '-') && (m2 += 1)
        k = m2
        while m2 <= length(t) && '0' <= t[m2] <= '9'; m2 += 1; end
        m2 > k && (j = m2)
    end
    try
        return parse(Float64, SubString(t, 1, j - 1))
    catch
        return nothing
    end
end
function _vers_entier(x::AbstractString)
    z = _entier_strict(x)
    z !== nothing && return z
    if occursin("/", x)
        a = _entier_strict(split(x, "/")[1]); b = _entier_strict(split(x, "/")[2])
        a !== nothing && b !== nothing && b != 0 && return fld(a, b)   # fdiv : plancher
        rose_erreur("conversion chaine → entier impossible")
    end
    if occursin(".", x)
        z = _entier_strict(split(x, ".")[1])
        z !== nothing && return z
    end
    rose_erreur("conversion chaine → entier impossible")
end
_vers_entier(x::AbstractFloat) = BigInt(trunc(x))
_vers_entier(x::Rational) = BigInt(fld(numerator(x), denominator(x)))
_vers_entier(x::Integer) = BigInt(x)
_vers_entier(x::Bool) = BigInt(x)

_vers_flottant(x::AbstractString) = occursin("/", x) ?
    parse(BigInt, split(x, "/")[1]) / parse(BigInt, split(x, "/")[2]) : parse(Float64, x)
_vers_flottant(x) = Float64(x)

_vers_q(x::AbstractString) = occursin("/", x) ?
    Rational{BigInt}(parse(BigInt, split(x, "/")[1]), parse(BigInt, split(x, "/")[2])) :
    Rational{BigInt}(BigInt(trunc(parse(Float64, x) * 10^15), 10^15))
_vers_q(x::AbstractFloat) = Rational{BigInt}(x)
_vers_q(x::Real) = Rational{BigInt}(BigInt(x), 1)
_vers_q(x::Rational) = x

_partie_entiere(x::Integer) = BigInt(x)
_partie_entiere(x::Bool) = BigInt(x)
_partie_entiere(x::Rational) = fld(BigInt(numerator(x)), BigInt(denominator(x)))
_partie_entiere(x::AbstractFloat) = BigInt(floor(x))

_rose_str(x::Bool) = x ? "vrai" : "faux"
function _rose_str(x::AbstractFloat)
    isnan(x) && return "non_nombre"
    isinf(x) && return x < 0 ? "-infini" : "infini"
    (x == floor(x) && abs(x) < 1e16) && return string(Int(x))
    return string(Float64(x))
end
_rose_str(x::Rational) = _periode(numerator(x), denominator(x))
_rose_str(x::AbstractVector) = "[" * join((_rose_str(e) for e in x), ", ") * "]"
_rose_str(x::Vector{Pair{Any,Any}}) = "[" * join((_rose_str(kv.first) * " => " * _rose_str(kv.second) for kv in x), ", ") * "]"
_chaine(x::Rational) = denominator(x) == 1 ? string(numerator(x)) : "$(numerator(x))/$(denominator(x))"
_chaine(x) = _rose_str(x)
_rose_str(x) = string(x)
_afficher(x) = println(_rose_str(x))

function _periode(n::BigInt, d::BigInt)
    d == 1 && return string(n)
    if length(string(abs(n))) + length(string(d)) > 500; return "$(n)/$(d)"; end
    entier = fld(n, d)              # plancher, comme texte_période du moteur C++
    reste = mod(abs(n), d)
    reste == 0 && return string(entier)
    digits = Char[]
    vus = Dict{BigInt, Int}()
    peri = -1
    k = 0
    while reste != 0 && k < 200
        if haskey(vus, reste); peri = vus[reste]; break; end
        vus[reste] = k
        reste *= 10
        push!(digits, '0' + div(reste, d))
        reste = mod(reste, d)
        k += 1
    end
    s = string(entier) * "." * join(digits[1:(peri >= 0 ? peri : k)])
    peri >= 0 && (s *= "[" * join(digits[(peri + 1):end]) * "]")
    (peri < 0 && k >= 200) && (s *= "…")
    return s
end

function _lire()
    eof(stdin) && rose_erreur("entrée attendue, fichier fermé")
    t = readline(stdin)
    return rstrip(t, (' ', '\r', '\n'))
end
function _lire_entier()
    t = _lire()
    if occursin("/", t)
        a = _entier_strict(split(t, "/")[1]); b = _entier_strict(split(t, "/")[2])
        a === nothing && rose_erreur("nombre entier invalide")
        b === nothing && rose_erreur("nombre entier invalide")
        b == 0 && rose_erreur("nombre entier invalide")
        return BigInt(trunc(Rational{BigInt}(a, b)))          # tdiv : troncature
    end
    if occursin(".", t)
        d = _strtod_prefix(t)
        d === nothing && rose_erreur("nombre entier invalide")
        return BigInt(trunc(d))
    end
    z = _entier_strict(t)
    z === nothing && rose_erreur("nombre entier invalide")
    return z
end
function _lire_flottant()
    d = _strtod_prefix(_lire())
    d === nothing && rose_erreur("flottant invalide")
    return d
end
function _lire_rationnel()
    t = _lire()
    occursin("/", t) && (m = match(r"^[+-]?[0-9]+/[+-]?[0-9]+$", String(t)); m !== nothing &&
        return Rational{BigInt}(parse(BigInt, split(t, "/")[1]), parse(BigInt, split(t, "/")[2])))
    z = _entier_strict(t)
    z !== nothing && return Rational{BigInt}(z)
    rose_erreur("rationnel invalide")
end
_saisir_entier() = _lire_entier()
_saisir_flottant() = _lire_flottant()

function _idx(base, i)
    if base isa Vector{Pair{Any,Any}}
        for kv in base
            kv.first == i && return kv.second
        end
        rose_erreur("clé absente de la table")
    end
    i = Int(i)
    if base isa String
        (i < 0 || i >= length(base)) && rose_erreur("index hors des limites de la chaîne")
        return string(collect(base)[i + 1])
    end
    (i < 0 || i >= length(base)) && rose_erreur("index hors des limites de la liste")
    return base[i + 1]
end
_nb_chiffres(n::Integer) = n == 0 ? BigInt(1) : BigInt(length(string(n < 0 ? -n : n)))
_nb_chiffres(n) = _nb_chiffres(BigInt(n))

function _charger_dll(nom)
    extensions = any(s -> endswith(nom, s), (".so", ".dll", ".dylib")) || occursin("/", nom)
    candidats = extensions ? [nom] :
        ["lib$(nom).so", "$(nom).so", nom, "lib$(nom).so.6", "lib$(nom).so.7", "lib$(nom).so.8"]
    for c in candidats
        h = Libdl.dlopen(c, Libdl.RTLD_LAZY; throw_error=false)
        h !== nothing && return h
    end
    rose_erreur("chargement de bibliothèque impossible ($(nom))")
end

function _symbole(h, nom)
    p = Libdl.dlsym(h, String(nom); throw_error=false)
    p === C_NULL && rose_erreur("symbole introuvable dans la bibliothèque ($(nom))")
    return p
end

function _appeler(h, nom, args...)
    p = _symbole(h, nom)
    entier_ici(x) = (x isa Integer && !(x isa Bool))
    if length(args) == 0
        return ccall(p, Cdouble, ())
    elseif length(args) == 1
        return entier_ici(args[1]) ?
            ccall(p, Cdouble, (Clong,), BigInt(args[1])) :
            ccall(p, Cdouble, (Cdouble,), Float64(args[1]))
    elseif length(args) == 2
        a1e, a2e = entier_ici(args[1]), entier_ici(args[2])
        if !a1e && !a2e
            return ccall(p, Cdouble, (Cdouble, Cdouble), Float64(args[1]), Float64(args[2]))
        elseif a1e && !a2e
            return ccall(p, Cdouble, (Clong, Cdouble), BigInt(args[1]), Float64(args[2]))
        elseif !a1e && a2e
            return ccall(p, Cdouble, (Cdouble, Clong), Float64(args[1]), BigInt(args[2]))
        else
            return ccall(p, Cdouble, (Clong, Clong), BigInt(args[1]), BigInt(args[2]))
        end
    else
        rose_erreur("trop d'arguments pour l'appel dynamique")
    end
end

# ── lexeur ───────────────────────────────────────────────────────────

const MOTS_CLES = Set(["fn", "retour", "si", "sinon", "tant_que", "pour", "de", "vers",
                       "vrai", "faux", "et", "ou", "non", "div",
                       "entier", "rationnel", "flottant", "booleen", "chaine", "liste", "table"])

const UNIQUES = Dict('≈' => :environ, '≤' => :infeg, '≥' => :supeg, '≠' => :diff,
                    '×' => :etoile, '⋅' => :etoile, '·' => :etoile, '÷' => :diventier,
                    '←' => :affect, '→' => :pointdir, '√' => :racine)
const CARACTERES = Dict('+' => :plus, '-' => :moins, '*' => :etoile, '/' => :slash,
    ':' => :deuxpts, '%' => :percent, '^' => :chapeau, '!' => :bang, '.' => :point,
    ',' => :virgule, '?' => :question, '(' => :lparen, ')' => :rparen, '{' => :lbrace,
    '}' => :rbrace, '[' => :lbracket, ']' => :rbracket, ';' => :semi, '<' => :inf,
    '>' => :sup, '~' => :tilde, '@' => :arobase)

est_lettre(c) = (isletter(c) || c == '_')
est_ident_suite(c) = (isletter(c) || isdigit(c) || c == '_')
est_chiffre(c) = isdigit(c)

function tokeniser(source::AbstractString)
    cs = collect(source)
    toks = []
    i, n = 1, length(cs)
    ligne, col = 1, 1
    function avancer(k)
        for _ in 1:k
            if cs[i] == '\n'; ligne += 1; col = 1; else col += 1; end
            i += 1
        end
    end
    while i <= n
        c = cs[i]
        if c in (' ', '\t', '\r', '\n'); avancer(1); continue; end
        if c == '#' || (c == '/' && i < n && cs[i+1] == '/')
            while i <= n && cs[i] != '\n'; avancer(1); end
            continue
        end
        if c == '"' || c == '\''
            quote_c = c; dl, dc = ligne, col
            avancer(1)
            buf = Char[]
            fini = false
            while !fini
                i > n && rose_erreur("Chaîne non terminée [Ligne $(dl), Colonne $(dc)]")
                if cs[i] == '\\'
                    i + 1 > n && rose_erreur("Échappement en fin de fichier [Ligne $(ligne), Colonne $(col)]")
                    nx = cs[i+1]
                    if nx == 'n'; push!(buf, '\n')
                    elseif nx == 't'; push!(buf, '\t')
                    elseif nx in ('\\', '"', '\''); push!(buf, nx)
                    else; push!(buf, '\\'); push!(buf, nx); end
                    avancer(2)
                    continue
                end
                if cs[i] == quote_c; avancer(1); fini = true; break; end
                push!(buf, cs[i]); avancer(1)
            end
                push!(toks, (:chaine, String(buf), dl, dc))
            continue
        end
        if est_chiffre(c)
            dl, dc = ligne, col
            debut = i
            while i <= n && est_chiffre(cs[i]); avancer(1); end
            flott = false
            if i + 1 <= n && cs[i] == '.' && est_chiffre(cs[i+1])
                flott = true; avancer(1)
                while i <= n && est_chiffre(cs[i]); avancer(1); end
            end
            push!(toks, (flott ? :flottant : :nombre, String(cs[debut:i-1]), dl, dc))
            continue
        end
        if c == 'π'   # constante flottante 3,14159…
            push!(toks, (:flottant, "3.141592653589793", ligne, col)); avancer(1); continue
        end
        if c == '²' || c == '³'   # x² ≡ x^2
            push!(toks, (:chapeau, "^", ligne, col))
            push!(toks, (:nombre, c == '²' ? "2" : "3", ligne, col))
            avancer(1); continue
        end
        if c == '$' || est_lettre(c)
            dl, dc = ligne, col
            debut = i
            dollar = false
            if c == '$'; dollar = true; avancer(1); end
            if i > n || !est_lettre(cs[i])
                rose_erreur("Attente d'un nom de variable après '\$' [Ligne $(dl), Colonne $(dc)]")
            end
            while i <= n && est_ident_suite(cs[i]); avancer(1); end
            texte = String(cs[debut:i-1])
            nu = dollar ? texte[2:end] : texte
            if dollar || nu in MOTS_CLES
                push!(toks, (:ident, nu, dl, dc))
            else
                push!(toks, (:mot, texte, dl, dc))
            end
            continue
        end
        if i < n
            paire = (cs[i], cs[i+1])
            tk = if paire == ('<','='); :infeg
                elseif paire == ('>','='); :supeg
                elseif paire == ('=','='); :doubleeg
                elseif paire == ('!','=') || paire == ('/','='); :diff
                elseif paire == ('?','='); :qmarkeg
                elseif paire == ('=','~') || paire == ('~','='); :tildeeg
                else; nothing; end
            if tk !== nothing
                push!(toks, (tk, String(cs[i:i+1]), ligne, col)); avancer(2); continue
            end
        end
        if i < n && ((c == '>' && cs[i+1] == '>') || (c == '<' && cs[i+1] == '<') || (c == '-' && cs[i+1] == '>'))
            tk = c == '>' ? :fleched : c == '<' ? :flecheg : :pointdir
            push!(toks, (tk, String(cs[i:i+1]), ligne, col)); avancer(2); continue
        end
        if c == '='
            push!(toks, (:affect, "=", ligne, col)); avancer(1); continue
        end
        if i < n && c == ':' && cs[i+1] == ':'
            push!(toks, (:deuxpoints, "::", ligne, col)); avancer(2); continue
        end
        if haskey(UNIQUES, c)
            push!(toks, (UNIQUES[c], string(c), ligne, col)); avancer(1); continue
        end
        if haskey(CARACTERES, c)
            push!(toks, (CARACTERES[c], string(c), ligne, col)); avancer(1); continue
        end
        rose_erreur("Caractère inattendu '$(c)' [Ligne $(ligne), Colonne $(col)]")
    end
    push!(toks, (:eof, "", ligne, col))
    return toks
end

# ── parseur ──────────────────────────────────────────────────────────

mutable struct Nd
    genre::Symbol
    ligne::Int
    f::Dict{Symbol, Any}
end
Nd(g, l; kv...) = Nd(g, l, Dict{Symbol, Any}(kv...))

mutable struct Parser
    toks::Vector
    pos::Int
end

voir(p::Parser, d=1) = p.toks[min(p.pos + d - 1, length(p.toks))]
suivant(p::Parser) = (t = voir(p); if t[1] != :eof; p.pos += 1; end; t)
est(p::Parser, ks) = voir(p)[1] in ks
function est_mot(p::Parser, texte)
    t = voir(p)
    return (t[1] == :ident || t[1] == :mot) && t[2] == texte
end
accepter(p::Parser, ks) = est(p, ks) ? (suivant(p); true) : false
accepter_mot(p::Parser, texte) = est_mot(p, texte) ? (suivant(p); true) : false
function exiger(p::Parser, k, quoi)
    t = suivant(p)
    t[1] == k || rose_erreur("Attendu $(quoi), trouvé '$(t[2])' [Ligne $(t[3]), Colonne $(t[4])]")
    return t
end
function nom_ident(p::Parser)
    t = suivant(p)
    (t[1] == :ident || t[1] == :mot) || rose_erreur("Attendu un identifiant, trouvé '$(t[2])' [Ligne $(t[3]), Colonne $(t[4])]")
    return t[2]
end

T_TYPES = Dict("entier" => "entier", "rationnel" => "rationnel", "flottant" => "flottant",
               "booleen" => "booleen", "chaine" => "chaine", "table" => "table")

function type_rose(p::Parser)
    t = suivant(p)
    (t[1] == :ident || t[1] == :mot) || rose_erreur("Attendu un type, trouvé '$(t[2])' [Ligne $(t[3]), Colonne $(t[4])]")
    if t[2] == "liste"
        exiger(p, :inf, "'<' de liste<T>")
        elt = type_rose(p)
        exiger(p, :sup, "'>' de liste<T>")
        return "liste<" * elt * ">"
    end
    if t[2] == "table"
        return "table"
    end
    haskey(T_TYPES, t[2]) || rose_erreur("Type inconnu '$(t[2])' [Ligne $(t[3]), Colonne $(t[4])]")
    return T_TYPES[t[2]]
end

function programmer(source::AbstractString)
    p = Parser(tokeniser(source), 1)
    signatures = Dict{String, Any}()
    instructions = []
    ann = Dict("dll" => "", "dll_son" => "", "sym" => "", "active" => false)
    while !est(p, (:eof,))
        if accepter(p, (:semi,)); continue; end
        t = voir(p)
        if t[1] == :arobase && voir(p, 2)[1] in (:ident, :mot) && voir(p, 2)[2] in ("dll", "sym")
            suivant(p)
            genre = nom_ident(p)
            exiger(p, :lparen, "'(' après @dll/@sym")
            if genre == "dll"
                ann["dll"] = exiger(p, :chaine, "nom de bibliothèque")[2]
                if accepter(p, (:virgule,)); ann["dll_son"] = exiger(p, :chaine, "nom de fichier")[2]; end
            else
                ann["sym"] = exiger(p, :chaine, "nom de symbole")[2]
            end
            exiger(p, :rparen, "')'")
            ann["active"] = true
            continue
        end
        if est_mot(p, "fn")
            sig = def_fonction(p, ann)
            ann = Dict("dll" => "", "dll_son" => "", "sym" => "", "active" => false)
            haskey(signatures, sig.nom) && rose_erreur("Fonction '$(sig.nom)' définie deux fois [Ligne $(sig.ligne)]")
            signatures[sig.nom] = sig
            continue
        end
        ann["active"] && rose_erreur("Annotation @dll/@sym sans fonction juste après [Ligne $(t[3])]")
        push!(instructions, instruction(p))
    end
    return (signatures=signatures, instructions=instructions)
end

mutable struct SignatureRose
    nom::String
    params::Vector{Tuple{String,String}}
    retour::String
    corps   # Nd ou nothing
    dll::String
    dll_son::String
    symbole::String
    ligne::Int
end

function def_fonction(p::Parser, ann)
    tok = suivant(p)                       # 'fn'
    nom = nom_ident(p)
    exiger(p, :lparen, "'('")
    params = Tuple{String,String}[]
    if !est(p, (:rparen,))
        while true
            pname = nom_ident(p)
            exiger(p, :deuxpts, "':' (nom: type)")
            pt = type_rose(p)
            push!(params, (pname, pt))
            accepter(p, (:virgule,)) || break
        end
    end
    exiger(p, :rparen, "')'")
    retour = "vide"
    if accepter(p, (:pointdir,)); retour = type_rose(p); end
    corps = nothing
    if est(p, (:lbrace,))
        corps = bloc(p)
    else
        retour == "vide" && rose_erreur("Une fonction externe doit déclarer un type de retour [Ligne $(tok[3])]")
    end
    return SignatureRose(nom, params, retour, corps, ann["dll"], ann["dll_son"], ann["sym"], tok[3])
end

function instruction(p::Parser)
    t = voir(p)
    k = t[1]
    if k == :lbrace
        b = bloc(p)
        return Nd(:Bloc, t[3]; instrs=b.f[:instrs])
    end
    if k == :fleched
        suivant(p)
        return Nd(:Afficher, t[3]; expr=expression(p))
    end
    if k == :flecheg
        suivant(p)
        return Nd(:Lire, t[3]; nom=nom_ident(p))
    end
    if k == :question
        suivant(p)
        tp = parentheser(p)
        corps = corps_chevron(p)
        return Nd(:Si, t[3]; test=tp, alors=corps, sinon=nothing)
    end
    if k == :arobase
        suivant(p)
        tp = parentheser(p)
        corps = corps_chevron(p)
        return Nd(:TantQue, t[3]; test=tp, corps=corps)
    end
    if k == :ident && t[2] == "si"
        suivant(p)
        tp = parentheser(p)
        alors = bloc(p)
        sinon = nothing
        if accepter_mot(p, "sinon")
            if est(p, (:lbrace,))
                sinon = bloc(p)
            else
                sinon = Nd(:Bloc, voir(p)[3]; instrs=[instruction(p)])
            end
        end
        return Nd(:Si, t[3]; test=tp, alors=alors, sinon=sinon)
    end
    if k == :ident && t[2] == "tant_que"
        suivant(p)
        return Nd(:TantQue, t[3]; test=parentheser(p), corps=bloc(p))
    end
    if k == :ident && t[2] == "pour"
        suivant(p)
        var = nom_ident(p)
        accepter_mot(p, "de") || rose_erreur("Attendu 'de' dans pour…de…vers [Ligne $(voir(p)[3])]")
        debut = expression(p)
        accepter_mot(p, "vers") || rose_erreur("Attendu 'vers' dans pour…de…vers [Ligne $(voir(p)[3])]")
        fin = expression(p)
        return Nd(:Pour, t[3]; variable=var, debut=debut, fin=fin, corps=bloc(p))
    end
    if k == :ident && t[2] == "retour"
        suivant(p)
        e = est(p, (:semi, :rbrace, :eof)) ? nothing : expression(p)
        return Nd(:Retour, t[3]; expr=e)
    end
    if (k == :ident || k == :mot) && voir(p, 2)[1] == :affect
        nom = nom_ident(p)
        suivant(p)                              # '='
        return Nd(:Affectation, t[3]; nom=nom, expr=expression(p))
    end
    return Nd(:Expr, t[3]; expr=expression(p))
end

function parentheser(p::Parser)
    exiger(p, :lparen, "'(' ")
    e = expression(p)
    exiger(p, :rparen, "')'")
    return e
end

function corps_chevron(p::Parser)
    t = exiger(p, :lbracket, "'[' de corps")
    instrs = []
    while !est(p, (:rbracket, :eof))
        push!(instrs, instruction(p))
        accepter(p, (:semi,)) || break
    end
    exiger(p, :rbracket, "']' de corps")
    return Nd(:Bloc, t[3]; instrs=instrs)
end

function bloc(p::Parser)
    t = exiger(p, :lbrace, "'{'")
    instrs = []
    while !est(p, (:rbrace, :eof))
        accepter(p, (:semi,)) && continue
        push!(instrs, instruction(p))
        accepter(p, (:semi,))
    end
    exiger(p, :rbrace, "'}'")
    return Nd(:Bloc, t[3]; instrs=instrs)
end

expression(p::Parser) = ou_expr(p)
function ou_expr(p::Parser)
    g = et_expr(p)
    while est_mot(p, "ou")
        suivant(p)
        g = Nd(:Binaire, voir(p)[3]; op="ou", gauche=g, droite=et_expr(p))
    end
    return g
end
function et_expr(p::Parser)
    g = non_expr(p)
    while est_mot(p, "et")
        suivant(p)
        g = Nd(:Binaire, voir(p)[3]; op="et", gauche=g, droite=non_expr(p))
    end
    return g
end
function non_expr(p::Parser)
    if est_mot(p, "non")
        t = suivant(p)
        return Nd(:Unaire, t[3]; op="non", operande=non_expr(p))
    end
    return comparaison(p)
end
const COMP_NOMS = Dict(:inf => "<", :infeg => "<=", :sup => ">", :supeg => ">=",
                       :doubleeg => "==", :diff => "!=", :environ => "≈",
                       :qmarkeg => "?=", :tildeeg => "=~")
function comparaison(p::Parser)
    g = somme(p)
    while true
        t = voir(p)
        haskey(COMP_NOMS, t[1]) || break
        suivant(p)
        g = Nd(:Binaire, t[3]; op=COMP_NOMS[t[1]], gauche=g, droite=somme(p))
    end
    return g
end
function somme(p::Parser)
    g = produit(p)
    while est(p, (:plus, :moins))
        t = suivant(p)
        g = Nd(:Binaire, t[3]; op=t[1] == :plus ? "+" : "-", gauche=g, droite=produit(p))
    end
    return g
end
function produit(p::Parser)
    g = puissance(p)
    while est(p, (:etoile, :slash, :percent, :diventier)) || est_mot(p, "div")
        t = suivant(p)
        op = t[1] == :etoile ? "*" :
             t[1] == :slash ? "/" :
             t[1] == :percent ? "%" :
             t[1] == :diventier || t[1] == :mot ? "div" : "div"
        g = Nd(:Binaire, t[3]; op=op, gauche=g, droite=puissance(p))
    end
    return g
end
function puissance(p::Parser)
    g = factorielle_post(p)
    if est(p, (:chapeau,))
        t = suivant(p)
        return Nd(:Binaire, t[3]; op="^", gauche=g, droite=puissance(p))
    end
    return g
end
function factorielle_post(p::Parser)
    g = unaire(p)
    while est(p, (:bang,))
        t = suivant(p)
        g = Nd(:Factorielle, t[3]; operande=g)
    end
    return g
end
function unaire(p::Parser)
    if est(p, (:racine,))
        t = suivant(p)
        return Nd(:Binaire, t[3]; op="^", gauche=unaire(p), droite=Nd(:Litteral, t[3]; texte="0.5", flottant=true))
    end
    if est(p, (:moins,))
        t = suivant(p)
        return Nd(:Unaire, t[3]; op="-", operande=unaire(p))
    end
    if est(p, (:plus,)); suivant(p); return unaire(p); end
    return postfixe(p)
end
function postfixe(p::Parser)
    g = atome(p)
    while true
        if est(p, (:lparen,))
            t = suivant(p)
            mod_, nom = "", ""
            if g.genre == :Appel && isempty(g.f[:args]) && g.f[:mod] != ""
                mod_, nom = g.f[:mod], g.f[:nom]
            elseif g.genre == :Var
                nom = g.f[:nom]
            else
                rose_erreur("Seul un nom de fonction peut être appelé [Ligne $(t[3])]")
            end
            args = []
            if !est(p, (:rparen,))
                while true
                    push!(args, expression(p))
                    accepter(p, (:virgule,)) || break
                end
            end
            exiger(p, :rparen, "')' de appel")
            g = Nd(:Appel, t[3]; mod=mod_, nom=nom, args=args)
            continue
        end
        if est(p, (:lbracket,))
            t = suivant(p)
            if est(p, (:deuxpts,))
                # tranche s[:b]
                suivant(p)
                debut = Nd(:Litteral, t[3]; texte="0", flottant=false)
                if est(p, (:rbracket,))
                    suivant(p)
                    fin = Nd(:Appel, t[3]; mod="", nom="longueur", args=[g])
                else
                    fin = expression(p)
                    exiger(p, :rbracket, "']' de tranche")
                end
                g = Nd(:Appel, t[3]; mod="", nom="sous_chaine", args=[g, debut, fin])
                continue
            end
            ix = expression(p)
            if est(p, (:deuxpts,))
                # tranche s[a:b] ou s[a:]
                suivant(p)
                debut = ix
                if est(p, (:rbracket,))
                    suivant(p)
                    fin = Nd(:Appel, t[3]; mod="", nom="longueur", args=[g])
                else
                    fin = expression(p)
                    exiger(p, :rbracket, "']' de tranche")
                end
                g = Nd(:Appel, t[3]; mod="", nom="sous_chaine", args=[g, debut, fin])
                continue
            end
            exiger(p, :rbracket, "']' d'index")
            g = Nd(:Index, t[3]; base=g, index=ix)
            continue
        end
        return g
    end
end
function atome(p::Parser)
    t = voir(p)
    if t[1] == :nombre
        suivant(p)
        return Nd(:Litteral, t[3]; texte=t[2], flottant=false)
    end
    if t[1] == :flottant
        suivant(p)
        return Nd(:Litteral, t[3]; texte=t[2], flottant=true)
    end
    if t[1] == :chaine
        suivant(p)
        return Nd(:Chaine, t[3]; valeur=t[2])
    end
    if t[1] == :lbracket
        tok = suivant(p)
        elements = []
        if !est(p, (:rbracket,))
            while true
                push!(elements, expression(p))
                accepter(p, (:virgule,)) || break
            end
        end
        exiger(p, :rbracket, "']' de liste")
        return Nd(:Liste, tok[3]; elements=elements)
    end
    if t[1] == :lparen
        suivant(p)
        e = expression(p)
        exiger(p, :rparen, "')'")
        return e
    end
    if t[1] in (:ident, :mot)
        if t[2] == "vrai"; suivant(p); return Nd(:Booleen, t[3]; valeur=true); end
        if t[2] == "faux"; suivant(p); return Nd(:Booleen, t[3]; valeur=false); end
        module_ = ""
        if voir(p, 2)[1] == :deuxpoints
            module_ = t[2]; suivant(p); suivant(p)
        end
        nom = nom_ident(p)
        if module_ == ""
            return Nd(:Var, t[3]; nom=nom)
        end
        return Nd(:Appel, t[3]; mod=module_, nom=nom, args=Any[])
    end
    rose_erreur("Attente d'une expression, trouvé '$(t[2])' [Ligne $(t[3]), Colonne $(t[4])]")
end

# ── interpréteur (types dynamiques : BigInt, Rational{BigInt}, Float64…) ──

mutable struct RetourSignale <: Exception
    valeur
end

mutable struct Interp
    prog
    globaux::Dict{String, Any}
    trames::Vector{Dict{String, Any}}
    dlls::Dict{String, Any}
    handles_dyn::Vector
    retour_flag::Bool
    retour_val
end
Interp(prog) = Interp(prog, Dict{String, Any}(), [Dict{String, Any}()], Dict{String, Any}(), [], false, nothing)

function cible_var(it::Interp, nom)
    haskey(it.trames[end], nom) && return it.trames[end]
    haskey(it.globaux, nom) && return it.globaux
    return nothing
end

_conv_t(v, t) = t == "entier" ? (v isa Integer ? v : _vers_entier(v)) :
    t == "rationnel" ? (v isa Rational ? v : _vers_q(v)) :
    t == "flottant" ? (v isa AbstractFloat ? v : Float64(v)) : v

function affecter(it::Interp, nom, v)
    c = cible_var(it, nom)
    if c !== nothing && haskey(c, nom)
        cur = c[nom]
        if cur isa AbstractFloat
            v = Float64(v)
        elseif cur isa Rational
            v = _vers_q(v)
        end
        c[nom] = v
    elseif c !== nothing
        c[nom] = v
    else
        it.trames[end][nom] = v
    end
    return nothing
end

function appeler_sig(it::Interp, sig::SignatureRose, args)
    if sig.corps === nothing
        return appel_externe(sig, args)
    end
    trame = Dict{String, Any}()
    for ((pn, pt), a) in zip(sig.params, args)
        trame[pn] = _conv_t(a, pt)
    end
    push!(it.trames, trame)
    res = nothing
    for i in sig.corps.f[:instrs]
        exec_instr(it, i)
        it.retour_flag && break
    end
    if it.retour_flag
        res = it.retour_val
        it.retour_flag = false
    end
    pop!(it.trames)
    res === nothing ? nothing : _conv_t(res, sig.retour)
end

const EXT_DLLS = Dict{String, Any}()

function ext_symbole(sig::SignatureRose)
    nom_dll = sig.dll_son != "" ? sig.dll_son : (sig.dll != "" ? sig.dll : "c")
    h = get!(EXT_DLLS, nom_dll) do
        _charger_dll(nom_dll)
    end
    return _symbole(h, sig.symbole != "" ? sig.symbole : sig.nom)
end

function appel_externe(sig::SignatureRose, args)
    p = ext_symbole(sig)
    types = [pt[2] for pt in sig.params]
    na = length(args)
    ret = sig.retour
    if ret == "flottant"
        if na == 0
            return Float64(ccall(p, Cdouble, ()))
        elseif na == 1
            t = types[1]
            t == "chaine"    && return Float64(ccall(p, Cdouble, (Cstring,), args[1]))
            t == "flottant"  && return Float64(ccall(p, Cdouble, (Cdouble,), Float64(args[1])))
            t == "booleen"   && return Float64(ccall(p, Cdouble, (Cint,), args[1] ? Cint(1) : Cint(0)))
            return Float64(ccall(p, Cdouble, (Clong,), BigInt(args[1])))
        elseif na == 2
            t1, t2 = types[1], types[2]
            a1 = t1 == "flottant" ? Float64(args[1]) : BigInt(args[1])
            a2 = t2 == "flottant" ? Float64(args[2]) : BigInt(args[2])
            t1 == "flottant" && t2 == "flottant" && return Float64(ccall(p, Cdouble, (Cdouble, Cdouble), a1, a2))
            t1 == "flottant"                         && return Float64(ccall(p, Cdouble, (Cdouble, Clong), a1, a2))
            t2 == "flottant"                         && return Float64(ccall(p, Cdouble, (Clong, Cdouble), a1, a2))
            return Float64(ccall(p, Cdouble, (Clong, Clong), a1, a2))
        elseif na == 3
            return Float64(ccall(p, Cdouble, (Cdouble, Cdouble, Cdouble),
                                 Float64(args[1]), Float64(args[2]), Float64(args[3])))
        end
        rose_erreur("fonction externe flottante : arité non supportée")
    elseif ret == "entier"
        if na == 0
            return BigInt(ccall(p, Clong, ()))
        elseif na == 1
            t = types[1]
            t == "flottant" && return BigInt(ccall(p, Clong, (Cdouble,), Float64(args[1])))
            t == "booleen"  && return BigInt(ccall(p, Clong, (Cint,), args[1] ? Cint(1) : Cint(0)))
            return BigInt(ccall(p, Clong, (Clong,), BigInt(args[1])))
        elseif na == 2
            return BigInt(ccall(p, Clong, (Clong, Clong), BigInt(args[1]), BigInt(args[2])))
        end
        rose_erreur("fonction externe : plus de 2 arguments entiers non supporté")
    elseif ret == "booleen"
        na == 0 && return ccall(p, Cint, ()) != 0
        na == 1 && return ccall(p, Cint, (Clong,), BigInt(args[1])) != 0
        rose_erreur("fonction externe booléenne : arité non supportée")
    elseif ret == "chaine"
        if na == 0
            return unsafe_string(ccall(p, Cstring, ()))
        elseif na == 1
            types[1] == "chaine" && return unsafe_string(ccall(p, Cstring, (Cstring,), args[1]))
            return unsafe_string(ccall(p, Cstring, (Clong,), BigInt(args[1])))
        end
        rose_erreur("fonction externe texte : arité non supportée")
    end
    rose_erreur("type de retour externe non supporté")
end

function exec_instr(it::Interp, i::Nd)
    g = i.genre
    if g == :Afficher
        _afficher(eval_expr(it, i.f[:expr])); return
    end
    if g == :Lire
        nom = i.f[:nom]
        existante = cible_var(it, nom)
        v = if existante !== nothing && existante[nom] isa AbstractFloat
                _lire_flottant()
            elseif existante !== nothing && existante[nom] isa Rational
                _lire_rationnel()
            else
                _lire_entier()
            end
        affecter(it, nom, v)
        return
    end
    if g == :Affectation
        affecter(it, i.f[:nom], eval_expr(it, i.f[:expr])); return
    end
    if g == :Si
        if eval_expr(it, i.f[:test])::Bool
            for s in i.f[:alors].f[:instrs]; exec_instr(it, s); end
        elseif i.f[:sinon] !== nothing
            for s in i.f[:sinon].f[:instrs]; exec_instr(it, s); end
        end
        return
    end
    if g == :TantQue
        while eval_expr(it, i.f[:test])::Bool
            for s in i.f[:corps].f[:instrs]; exec_instr(it, s); end
        end
        return
    end
    if g == :Pour
        debut, fin = BigInt(eval_expr(it, i.f[:debut])), BigInt(eval_expr(it, i.f[:fin]))
        k = debut
        var = i.f[:variable]
        try
            while k < fin
                it.trames[end][var] = k
                for s in i.f[:corps].f[:instrs]; exec_instr(it, s); end
                k = BigInt(it.trames[end][var]) + 1
            end
        finally
            delete!(it.trames[end], var)
        end
        return
    end
    if g == :Retour
        it.retour_val = i.f[:expr] === nothing ? nothing : eval_expr(it, i.f[:expr])
        it.retour_flag = true
        return
    end
    if g == :Expr
        eval_expr(it, i.f[:expr]); return
    end
    if g == :Bloc
        for s in i.f[:instrs]; exec_instr(it, s); end
        return
    end
    rose_erreur("instruction inconnue ($(g))")
end

function eval_expr(it::Interp, e::Nd)
    g = e.genre
    if g == :Litteral
        if e.f[:flottant]; return parse(Float64, e.f[:texte]); end
        texte = e.f[:texte]
        if occursin("/", texte)
            n, d = split(texte, "/")
            return Rational{BigInt}(parse(BigInt, n), parse(BigInt, d))
        end
        return parse(BigInt, texte)
    end
    g == :Chaine && return e.f[:valeur]
    g == :Booleen && return e.f[:valeur]
    if g == :Var
        c = cible_var(it, e.f[:nom])
        c === nothing && rose_erreur("variable '$(e.f[:nom])' utilisée sans valeur [Ligne $(e.ligne)]")
        return c[e.f[:nom]]
    end
    if g == :Liste
        return Any[eval_expr(it, x) for x in e.f[:elements]]
    end
    if g == :Index
        return _idx(eval_expr(it, e.f[:base]), eval_expr(it, e.f[:index]))
    end
    if g == :Unaire
        v = eval_expr(it, e.f[:operande])
        if e.f[:op] == "non"; return !v; end
        if e.f[:op] == "+"; return v; end
        return -v
    end
    if g == :Factorielle
        return _fact(BigInt(eval_expr(it, e.f[:operande])))
    end
    if g == :Binaire; return binaire_eval(it, e); end
    if g == :Appel; return appel_eval(it, e); end
    rose_erreur("expression inconnue ($(g))")
end

function binaire_eval(it::Interp, e::Nd)
    op = e.f[:op]
    if op == "et"
        return eval_expr(it, e.f[:gauche])::Bool ? eval_expr(it, e.f[:droite]) : false
    end
    if op == "ou"
        return eval_expr(it, e.f[:gauche])::Bool ? true : eval_expr(it, e.f[:droite])
    end
    g = eval_expr(it, e.f[:gauche])
    d = eval_expr(it, e.f[:droite])
    if op == "/"; return _div(g, d); end
    if op == "%"; return _mod(g, d); end
    if op == "div"; return _divf(g, d); end
    if op == "^"; return _powr(g, d); end
    if op in ("≈", "?=", "=~"); return _approche(g, d); end
    if op == "+"
        if g isa String && d isa String; return g * d; end
        (g isa AbstractFloat || d isa AbstractFloat) && return _vers_d(g) + _vers_d(d)
        return g + d
    end
    if op == "-"
        (g isa AbstractFloat || d isa AbstractFloat) && return _vers_d(g) - _vers_d(d)
        return g - d
    end
    if op == "*"
        (g isa AbstractFloat || d isa AbstractFloat) && return _vers_d(g) * _vers_d(d)
        return g * d
    end
    if op == "=="
        if g isa Vector{Pair{Any,Any}} && d isa Vector{Pair{Any,Any}}
            length(g) == length(d) || return false
            for kv in g
                any(kv2 -> kv2.first == kv.first && kv2.second == kv.second, d) || return false
            end
            return true
        end
        return g == d
    end
    if op == "!="
        if g isa Vector{Pair{Any,Any}} && d isa Vector{Pair{Any,Any}}
            return !(g == d)
        end
        return g != d
    end
    if op == "!="; return g != d; end
    if op == "<"; return g < d; end
    if op == "<="; return g <= d; end
    if op == ">"; return g > d; end
    if op == ">="; return g >= d; end
    rose_erreur("opérateur '$(op)' inconnu")
end

function appel_eval(it::Interp, e::Nd)
    args = [eval_expr(it, a) for a in e.f[:args]]
    if e.f[:mod] == "dynamique"
        nom = e.f[:nom]
        nom == "charger" && return _charger_dll(args[1])
        nom == "fermer" && return nothing
        return _appeler(args[1], args[2], args[3:end]...)
    end
    sig = get(it.prog.signatures, e.f[:nom], nothing)
    if sig !== nothing
        return appeler_sig(it, sig, args)
    end
    nom = e.f[:nom]
    if nom == "joindre"
        push!(args[1], args[2])
        return nothing
    end
    if nom == "ordre"
        (args[1] isa String && length(args[1]) == 1) ||
            rose_erreur("ordre attend une chaîne d'un seul caractère")
        return BigInt(codepoint(args[1][1]))
    end
    if nom == "caractere"
        c = BigInt(args[1])
        (c < 0 || c > 1114111) && rose_erreur("caractere : code hors plage (0 à 1114111)")
        ci = Int(c)
        (55296 <= ci <= 57343) && rose_erreur("caractere : code réservé (55296 à 57343)")
        return string(Char(ci))
    end
    if nom == "sous_chaine"
        ch = (args[1] isa String) ? args[1] : ""
        d = BigInt(args[2]); f = BigInt(args[3])
        (d < 0 || f < 0 || d > 4611686018427387904 || f > 4611686018427387904) &&
            rose_erreur("sous_chaine : les bornes doivent être des entiers positifs")
        cs = collect(ch); taille = length(cs)
        d = Int(d); f = Int(f)
        (d > taille || f < d) && rose_erreur("sous_chaine : bornes invalides")
        f = min(f, taille)
        return join(cs[d + 1:f], "")
    end
    if nom == "concatener"
        (args[1] isa Vector) || rose_erreur("concatener attend une liste")
        all(x -> x isa String, args[1]) || rose_erreur("concatener attend une liste de chaînes")
        return join(args[1], "")
    end
    if nom == "lire_texte"
        chemin = (args[1] isa String) ? args[1] : ""
        if !isfile(chemin); rose_erreur("lecture du fichier impossible ($(chemin))"); end
        filesize(chemin) > 268435456 && rose_erreur("fichier trop volumineux ($(chemin))")
        try
            return read(chemin, String)
        catch
            rose_erreur("lecture du fichier impossible ($(chemin))")
        end
    end
    if nom == "ecrire_texte"
        chemin = (args[1] isa String) ? args[1] : ""
        contenu = (args[2] isa String) ? args[2] : ""
        try
            write(chemin, contenu)
        catch
            rose_erreur("écriture du fichier impossible ($(chemin))")
        end
        return nothing
    end
    if nom == "nb_arguments"; return BigInt(length(PROG_ARGS)); end
    if nom == "argument"
        i = Int(args[1])
        (i < 0 || i >= length(PROG_ARGS)) && rose_erreur("index hors des limites des arguments")
        return PROG_ARGS[i + 1]
    end
    if nom == "afficher_erreur"; println(stderr, _chaine(args[1])); flush(stderr); return nothing; end
    if nom == "quitter"
        c = BigInt(args[1])
        (c < -2147483648 || c > 2147483647) && rose_erreur("code de sortie hors plage")
        flush(stdout); flush(stderr)
        exit(Int(c))
    end
    if nom == "fraction"
        den = BigInt(args[2])
        den == 0 && rose_erreur("fraction de dénominateur nul")
        return Rational{BigInt}(BigInt(args[1]), den)
    end
    if nom == "factorielle"; return _fact(BigInt(args[1])); end
    if nom == "fibonacci"; return _fib(BigInt(args[1])); end
    if nom == "pgcd"; return _pgcd(args[1], args[2]); end
    if nom == "ppcm"; return _ppcm(args[1], args[2]); end
    if nom == "puissance"; return _powr(args[1], args[2]); end
    if nom == "est_premier"; return _prem(args[1]); end
    if nom == "compter_premiers"; return _compter_premiers(args[1]); end
    if nom == "longueur"
        if args[1] isa Vector{Pair{Any,Any}}; return length(args[1]); end
        return length(args[1])
    end
    if nom == "table"; return Pair{Any,Any}[]; end
    if nom == "inserer"
        t = args[1]; cle = args[2]; val = args[3]
        for (i, kv) in enumerate(t)
            if kv.first == cle; t[i] = Pair{Any,Any}(cle, val); return nothing; end
        end
        push!(t, Pair{Any,Any}(cle, val))
        return nothing
    end
    if nom == "cle_existe"
        t = args[1]; cle = args[2]
        return any(kv -> kv.first == cle, t)
    end
    if nom == "supprimer"
        t = args[1]; cle = args[2]
        for (i, kv) in enumerate(t)
            if kv.first == cle; deleteat!(t, i); return true; end
        end
        return false
    end
    if nom == "cles"
        return [kv.first for kv in args[1]]
    end
    if nom == "valeurs"
        return [kv.second for kv in args[1]]
    end
    if nom == "abs"; return abs(args[1]); end
    if nom == "min"; return min(args[1], args[2]); end
    if nom == "max"; return max(args[1], args[2]); end
    if nom == "nombre_pi"; return _pi(args[1]); end
    if nom == "saisir_entier"; return _saisir_entier(); end
    if nom == "saisir_flottant"; return _saisir_flottant(); end
    if nom == "nb_chiffres"; return _nb_chiffres(args[1]); end
    if nom == "partie_entiere"; return _partie_entiere(args[1]); end
    if nom == "entier"; return _vers_entier(args[1]); end
    if nom == "flottant"; return _vers_flottant(args[1]); end
    if nom == "rationnel"; return _vers_q(args[1]); end
    if nom == "chaine"; return _chaine(args[1]); end
    if nom == "afficher"; _afficher(args[1]); return nothing; end
    rose_erreur("fonction '$(nom)' inconnue [Ligne $(e.ligne)]")
end

function executer(prog)
    it = Interp(prog)
    for i in prog.instructions
        exec_instr(it, i)
    end
    flush(stdout)
end

# ── point d'entrée ───────────────────────────────────────────────────

const PROG_ARGS = String[]   # argv vus par le programme Rose

function main_cli(args)
    if length(args) < 1
        println(stderr, "usage : julia engine.jl prog.rose | version | info")
        exit(1)
    end
    a = args[1]
    if a == "version"; println("Rose 2.0.0 — moteur Julia (BigInt / Rational exacts)"); return 0; end
    if a == "info"
        println("rose 2.0.0 — moteur Julia")
        println("  entiers    : BigInt précision arbitraire")
        println("  rationnels : Rational{BigInt} exact")
        println("  flottants  : Float64")
        println("  FFI        : Libdl (dlopen)")
        return 0
    end
    fichier = a == "run" && length(args) > 1 ? args[2] : a
    append!(PROG_ARGS, a == "run" ? args[3:end] : args[2:end])
    if startswith(a, "-") || a == "run"
        println(stderr, "rose: quel programme interpréter ?")
        return 1
    end
    isfile(fichier) || (println(stderr, "rose: fichier introuvable : $(fichier)"); return 1)
    source = read(fichier, String)
    try
        executer(programmer(source))
    catch e
        if e isa InterruptException; rethrow(); end
        flush(stdout)
        msg = sprint(showerror, e)
        if occursin("Rose : erreur", msg)
            # déjà affiché par rose_erreur via exit — inutile
        else
            println(stderr, "Rose : erreur — ", replace(strip(msg), "\n" => " "))
        end
        return 1
    end
    return 0
end

if !isinteractive()
    exit(main_cli(ARGS))
end
