// ═══════════════ Rose — moteur natif Zig (binaire autonome, GMP liée) ═══════════════
// Port direct du moteur C++ : lexeur → AST → repli → interpréteur, maths GMP exactes.
// (Vérificateur de types réservé aux moteurs C++/Rust ; cf. README moteur.)
const std = @import("std");
const Allocator = std.mem.Allocator;

// ── GMP : symboles réels (préfixe __g de gmp.h) ──────────────────────
const mp_limb_t = u64;

const Mpz = extern struct { _mp_alloc: c_int, _mp_size: c_int, _mp_d: [*]mp_limb_t };  // mp_size_t = int dans cette build GMP
const Mpq = extern struct { _mp_num: Mpz, _mp_den: Mpz };

const _gmpz_init2 = @extern(*const fn (r: *Mpz, bits: c_ulong) callconv(.C) void, .{ .name = "__gmpz_init2" });
const _gmpz_init = @extern(*const fn (r: *Mpz) callconv(.C) void, .{ .name = "__gmpz_init" });
const _gmpz_clear = @extern(*const fn (r: *Mpz) callconv(.C) void, .{ .name = "__gmpz_clear" });
const _gmpz_set = @extern(*const fn (r: *Mpz, x: *const Mpz) callconv(.C) void, .{ .name = "__gmpz_set" });
const _gmpz_set_si = @extern(*const fn (r: *Mpz, v: c_long) callconv(.C) void, .{ .name = "__gmpz_set_si" });
const _gmpz_set_ui = @extern(*const fn (r: *Mpz, v: c_ulong) callconv(.C) void, .{ .name = "__gmpz_set_ui" });
const _gmpz_set_d = @extern(*const fn (r: *Mpz, v: f64) callconv(.C) void, .{ .name = "__gmpz_set_d" });
const _gmpz_get_d = @extern(*const fn (x: *const Mpz) callconv(.C) f64, .{ .name = "__gmpz_get_d" });
const _gmpz_get_si = @extern(*const fn (x: *const Mpz) callconv(.C) c_long, .{ .name = "__gmpz_get_si" });
const _gmpz_get_ui = @extern(*const fn (x: *const Mpz) callconv(.C) c_ulong, .{ .name = "__gmpz_get_ui" });
const _gmpz_fits_slong_p = @extern(*const fn (x: *const Mpz) callconv(.C) c_int, .{ .name = "__gmpz_fits_slong_p" });
const _gmpz_add = @extern(*const fn (r: *Mpz, a: *const Mpz, b: *const Mpz) callconv(.C) void, .{ .name = "__gmpz_add" });
const _gmpz_sub = @extern(*const fn (r: *Mpz, a: *const Mpz, b: *const Mpz) callconv(.C) void, .{ .name = "__gmpz_sub" });
const _gmpz_mul = @extern(*const fn (r: *Mpz, a: *const Mpz, b: *const Mpz) callconv(.C) void, .{ .name = "__gmpz_mul" });
const _gmpz_import = @extern(*const fn (rop: *Mpz, count: usize, order: c_int, size: usize, endian: c_int, nails: usize, op: *const anyopaque) callconv(.C) void, .{ .name = "__gmpz_import" });
const _gmpz_export = @extern(*const fn (op: *anyopaque, count: *usize, order: c_int, size: usize, endian: c_int, nails: usize, src: *const Mpz) callconv(.C) *anyopaque, .{ .name = "__gmpz_export" });
const _gmpz_mul_ui = @extern(*const fn (r: *Mpz, a: *const Mpz, b: c_ulong) callconv(.C) void, .{ .name = "__gmpz_mul_ui" });
const _gmpz_fdiv_q = @extern(*const fn (r: *Mpz, a: *const Mpz, b: *const Mpz) callconv(.C) void, .{ .name = "__gmpz_fdiv_q" });
const _gmpz_tdiv_q = @extern(*const fn (r: *Mpz, a: *const Mpz, b: *const Mpz) callconv(.C) void, .{ .name = "__gmpz_tdiv_q" });
const _gmpz_fdiv_r = @extern(*const fn (r: *Mpz, a: *const Mpz, b: *const Mpz) callconv(.C) void, .{ .name = "__gmpz_fdiv_r" });
const _gmpz_cmp = @extern(*const fn (a: *const Mpz, b: *const Mpz) callconv(.C) c_int, .{ .name = "__gmpz_cmp" });
const _gmpz_cmp_si = @extern(*const fn (a: *const Mpz, b: c_long) callconv(.C) c_int, .{ .name = "__gmpz_cmp_si" });
const _gmpz_abs = @extern(*const fn (r: *Mpz, a: *const Mpz) callconv(.C) void, .{ .name = "__gmpz_abs" });
const _gmpz_neg = @extern(*const fn (r: *Mpz, a: *const Mpz) callconv(.C) void, .{ .name = "__gmpz_neg" });
const _gmpz_pow_ui = @extern(*const fn (r: *Mpz, a: *const Mpz, e: c_ulong) callconv(.C) void, .{ .name = "__gmpz_pow_ui" });
const _gmpz_gcd = @extern(*const fn (r: *Mpz, a: *const Mpz, b: *const Mpz) callconv(.C) void, .{ .name = "__gmpz_gcd" });
const _gmpz_lcm = @extern(*const fn (r: *Mpz, a: *const Mpz, b: *const Mpz) callconv(.C) void, .{ .name = "__gmpz_lcm" });
const _gmpz_probab_prime_p = @extern(*const fn (n: *const Mpz, reps: c_int) callconv(.C) c_int, .{ .name = "__gmpz_probab_prime_p" });
const _gmpz_tstbit = @extern(*const fn (x: *const Mpz, bit: c_ulong) callconv(.C) c_int, .{ .name = "__gmpz_tstbit" });
const _gmpz_sizeinbase = @extern(*const fn (x: *const Mpz, base: c_int) callconv(.C) usize, .{ .name = "__gmpz_sizeinbase" });
const _gmpz_get_str = @extern(*const fn (buf: ?[*]u8, base: c_int, x: *const Mpz) callconv(.C) [*:0]u8, .{ .name = "__gmpz_get_str" });
const _gmpz_set_str = @extern(*const fn (r: *Mpz, s: [*:0]const u8, base: c_int) callconv(.C) c_int, .{ .name = "__gmpz_set_str" });
const _gmpz_ui_pow_ui = @extern(*const fn (r: *Mpz, base: c_ulong, e: c_ulong) callconv(.C) void, .{ .name = "__gmpz_ui_pow_ui" });
const _gmpq_init = @extern(*const fn (r: *Mpq) callconv(.C) void, .{ .name = "__gmpq_init" });
const _gmpq_clear = @extern(*const fn (r: *Mpq) callconv(.C) void, .{ .name = "__gmpq_clear" });
const _gmpq_set = @extern(*const fn (r: *Mpq, x: *const Mpq) callconv(.C) void, .{ .name = "__gmpq_set" });
const _gmpq_set_z = @extern(*const fn (r: *Mpq, z: *const Mpz) callconv(.C) void, .{ .name = "__gmpq_set_z" });
const _gmpq_set_d = @extern(*const fn (r: *Mpq, d: f64) callconv(.C) void, .{ .name = "__gmpq_set_d" });
const _gmpq_canonicalize = @extern(*const fn (r: *Mpq) callconv(.C) void, .{ .name = "__gmpq_canonicalize" });
const _gmpq_add = @extern(*const fn (r: *Mpq, a: *const Mpq, b: *const Mpq) callconv(.C) void, .{ .name = "__gmpq_add" });
const _gmpq_sub = @extern(*const fn (r: *Mpq, a: *const Mpq, b: *const Mpq) callconv(.C) void, .{ .name = "__gmpq_sub" });
const _gmpq_mul = @extern(*const fn (r: *Mpq, a: *const Mpq, b: *const Mpq) callconv(.C) void, .{ .name = "__gmpq_mul" });
const _gmpq_div = @extern(*const fn (r: *Mpq, a: *const Mpq, b: *const Mpq) callconv(.C) void, .{ .name = "__gmpq_div" });
const _gmpq_cmp = @extern(*const fn (a: *const Mpq, b: *const Mpq) callconv(.C) c_int, .{ .name = "__gmpq_cmp" });
const _gmpq_get_d = @extern(*const fn (x: *const Mpq) callconv(.C) f64, .{ .name = "__gmpq_get_d" });
const _gmpq_set_str = @extern(*const fn (r: *Mpq, s: [*:0]const u8, base: c_int) callconv(.C) c_int, .{ .name = "__gmpq_set_str" });
const _gmpq_get_str = @extern(*const fn (buf: ?[*]u8, base: c_int, x: *const Mpq) callconv(.C) [*:0]u8, .{ .name = "__gmpq_get_str" });

// ── allocation : unique arena pour tout le processus ──────────────────
var arena_instance: std.heap.ArenaAllocator = undefined;
fn arena() Allocator { return arena_instance.allocator(); }

const builtin = @import("builtin");

extern "kernel32" fn SetConsoleOutputCP(w: u32) callconv(.C) c_int;
extern "kernel32" fn SetConsoleCP(w: u32) callconv(.C) c_int;

/// Fins de ligne normalisées en \n : un fichier édité sous Windows arrive
/// en \r\n et le lexeur doit voir une seule ligne par ligne.
fn normaliserFinsDeLigne(s: []const u8) []const u8 {
    var out = std.ArrayList(u8).init(arena());
    var i: usize = 0;
    while (i < s.len) : (i += 1) {
        if (s[i] == '\r') {
            out.append('\n') catch @panic("mémoire");
            if (i + 1 < s.len and s[i + 1] == '\n') i += 1;
        } else out.append(s[i]) catch @panic("mémoire");
    }
    return out.items;
}

fn gmpAlloc(comptime T: type) Er!*T {
    const p = try arena().create(T);
    @memset(@as([*]u8, @ptrCast(p))[0..@sizeOf(T)], 0);
    return p;
}

// ── Z : entier GMP ────────────────────────────────────────────────────
const Z = struct {
    m: *Mpz,

    fn new() Z {
        const p = gmpAlloc(Mpz) catch @panic("mémoire");
        _gmpz_init(p);
        return .{ .m = p };
    }
    fn i(v: i64) Z {
        const z = u(@abs(v));
        return if (v < 0) neg(z) else z;
    }
    fn u(v: u64) Z {
        //mpz_import d'un mot de 8 octets : exact sur toutes les plateformes
        // (mpz_set_si/ui dépend de « long », qui tombe à 32 bits sous Windows).
        const z = new();
        var w: u64 = v;
        _gmpz_import(z.m, 1, -1, @sizeOf(u64), 0, 0, &w);
        return z;
    }
    fn deF64(d: f64) Z { const z = new(); _gmpz_set_d(z.m, d); return z; }
    fn dup(o: Z) Z { const z = new(); _gmpz_set(z.m, o.m); return z; }
    fn depuisTexte(s: []const u8) ?Z {
        const zs = arena().dupeZ(u8, s) catch return null;
        const z = new();
        if (_gmpz_set_str(z.m, zs.ptr, 10) != 0) { _gmpz_clear(z.m); return null; }
        return z;
    }
    fn texte(z: Z) []const u8 {
        const p = _gmpz_get_str(null, 10, z.m);
        return arena().dupe(u8, std.mem.span(p)) catch @panic("mémoire");
    }
    fn sgn(z: Z) i32 { const t = z.m.*._mp_size; return if (t < 0) -1 else if (t > 0) 1 else 0; }
    fn cmp(a: Z, b: Z) i32 { return _gmpz_cmp(a.m, b.m); }
    fn cmpSi(a: Z, b: i64) i32 { return cmp(a, i(b)); }
    fn add(a: Z, b: Z) Z { const r = new(); _gmpz_add(r.m, a.m, b.m); return r; }
    fn sub(a: Z, b: Z) Z { const r = new(); _gmpz_sub(r.m, a.m, b.m); return r; }
    fn mul(a: Z, b: Z) Z { const r = new(); _gmpz_mul(r.m, a.m, b.m); return r; }
    fn mulU(a: Z, b: u64) Z {
        if (b <= 0xffffffff) { const r = new(); _gmpz_mul_ui(r.m, a.m, @intCast(b)); return r; }
        return mul(a, u(b));
    }
    fn fdivQ(a: Z, b: Z) Z { const r = new(); _gmpz_fdiv_q(r.m, a.m, b.m); return r; }
    fn tdivQ(a: Z, b: Z) Z { const r = new(); _gmpz_tdiv_q(r.m, a.m, b.m); return r; }
    fn fdivR(a: Z, b: Z) Z { const r = new(); _gmpz_fdiv_r(r.m, a.m, b.m); return r; }
    fn abs(a: Z) Z { const r = new(); _gmpz_abs(r.m, a.m); return r; }
    fn neg(a: Z) Z { const r = new(); _gmpz_neg(r.m, a.m); return r; }
    fn powU(a: Z, e: u64) Z {
        if (e <= 0xffffffff) { const r = new(); _gmpz_pow_ui(r.m, a.m, @intCast(e)); return r; }
        return powParCarres(a, e);
    }
    fn powParCarres(a: Z, e: u64) Z {
        var base = a;
        var exposant = e;
        var r = i(1);
        while (exposant > 0) : (exposant >>= 1) {
            if ((exposant & 1) == 1) r = mul(r, base);
            base = mul(base, base);
        }
        return r;
    }
    fn gcd(a: Z, b: Z) Z { const r = new(); _gmpz_gcd(r.m, a.m, b.m); return r; }
    fn lcm(a: Z, b: Z) Z { const r = new(); _gmpz_lcm(r.m, a.m, b.m); return r; }
    fn tientI64(a: Z) bool { return _gmpz_fits_slong_p(a.m) != 0; }
    fn enU64(a: Z) u64 {
        var w: u64 = 0;
        var cnt: usize = 1;
        _ = _gmpz_export(&w, &cnt, -1, @sizeOf(u64), 0, 0, a.m);
        return w;
    }
    fn enI64(a: Z) i64 {
        const us = enU64(a);
        return if (sgn(a) < 0) @bitCast(-%@as(u64, us)) else @bitCast(us);
    }
    fn enF64(a: Z) f64 { return _gmpz_get_d(a.m); }
    fn estPremier(a: Z) bool { return _gmpz_probab_prime_p(a.m, 25) != 0; }
    fn bit(a: Z, b: u64) bool {
        if (b > 0xffffffff) return false;
        return _gmpz_tstbit(a.m, @intCast(b)) != 0;
    }
    fn bits(a: Z) u64 { if (a.sgn() == 0) return 0; return _gmpz_sizeinbase(a.m, 2); }
    fn chiffres10(a: Z) usize { return _gmpz_sizeinbase(a.m, 10); }
    fn pow10(e: u64) Z {
        if (e <= 0xffffffff) { const r = new(); _gmpz_ui_pow_ui(r.m, 10, @intCast(e)); return r; }
        return powParCarres(i(10), e);
    }
};

// ── Q : rationnel GMP ─────────────────────────────────────────────────
const Q = struct {
    m: *Mpq,

    fn new() Q { const p = gmpAlloc(Mpq) catch @panic("mémoire"); _gmpq_init(p); return .{ .m = p }; }
    fn depuisZ(z: Z) Q { const q = new(); _gmpq_set_z(q.m, z.m); return q; }
    fn depuisF64(d: f64) Q { const q = new(); _gmpq_set_d(q.m, d); return q; }
    fn dup(o: Q) Q { const q = new(); _gmpq_set(q.m, o.m); return q; }
    fn canon(q: Q) Q { _gmpq_canonicalize(q.m); return q; }
    fn depuisTexte(s: []const u8) ?Q {
        const zs = arena().dupeZ(u8, s) catch return null;
        const q = new();
        if (_gmpq_set_str(q.m, zs.ptr, 10) != 0) { _gmpq_clear(q.m); return null; }
        _gmpq_canonicalize(q.m);
        return q;
    }
    fn avecNumDen(num: Z, den: Z) Q {
        const q = new();
        _gmpz_set(&q.m.*._mp_num, num.m);
        _gmpz_set(&q.m.*._mp_den, den.m);
        _gmpq_canonicalize(q.m);
        return q;
    }
    fn numerateur(q: Q) Z { const z = Z.new(); _gmpz_set(z.m, &q.m.*._mp_num); return z; }
    fn denominateur(q: Q) Z { const z = Z.new(); _gmpz_set(z.m, &q.m.*._mp_den); return z; }
    fn sgn(q: Q) i32 { const t = q.m.*._mp_num._mp_size; return if (t < 0) -1 else if (t > 0) 1 else 0; }
    fn cmp(a: Q, b: Q) i32 { return _gmpq_cmp(a.m, b.m); }
    fn add(a: Q, b: Q) Q { const r = new(); _gmpq_add(r.m, a.m, b.m); return r; }
    fn sub(a: Q, b: Q) Q { const r = new(); _gmpq_sub(r.m, a.m, b.m); return r; }
    fn mul(a: Q, b: Q) Q { const r = new(); _gmpq_mul(r.m, a.m, b.m); return r; }
    fn div(a: Q, b: Q) Q { const r = new(); _gmpq_div(r.m, a.m, b.m); return r; }
    fn enF64(q: Q) f64 { return _gmpq_get_d(q.m); }
    fn neg(q: Q) Q { return avecNumDen(q.numerateur().neg(), q.denominateur()); }
};

// ── erreurs ───────────────────────────────────────────────────────────
const Erreur = struct { msg: []const u8 = "", ligne: i64 = 0, colonne: i64 = 0 };
const Er = error{Rose} || std.mem.Allocator.Error;
var err_derniere: Erreur = .{};
fn err(e: Erreur) error{Rose} { err_derniere = e; return error.Rose; }

fn texteErreur(e: Erreur) []const u8 {
    if (e.ligne != 0)
        return std.fmt.allocPrint(arena(), "[Ligne {d}, Colonne {d}] {s}", .{ e.ligne, e.colonne, e.msg }) catch unreachable;
    return e.msg;
}

// ── Chaînes v3 : indexation par CARACTÈRES Unicode ────────────────────
var progArgs: [][:0]u8 = &[_][:0]u8{};   // argv vus par le programme

fn utf8Seq(c: u8) usize {
    if (c & 0xF8 == 0xF0) return 4;
    if (c & 0xF0 == 0xE0) return 3;
    if (c & 0xE0 == 0xC0) return 2;
    return 1;
}
fn utf8Len(s: []const u8) usize {
    var k: usize = 0;
    for (s) |c| {
        if (c & 0xC0 != 0x80) k += 1;
    }
    return k;
}
fn utf8Off(s: []const u8, kc: usize) usize {
    var vu: usize = 0;
    for (s, 0..) |c, i| {
        if (c & 0xC0 != 0x80) {
            if (vu == kc) return i;
            vu += 1;
        }
    }
    return s.len;
}
fn utf8CharAt(s: []const u8, kc: usize) u21 {
    const o = utf8Off(s, kc);
    const c = s[o];
    const q = utf8Seq(c);
    var v: u32 = if (q == 1) @intCast(c) else @as(u32, c) & (@as(u32, 0xFF) >> @intCast(q + 1));
    var i: usize = 1;
    while (i < q and o + i < s.len) : (i += 1)
        v = (v << 6) | @as(u32, s[o + i] & 0x3F);
    return @intCast(v);
}
fn utf8CharStr(s: []const u8, kc: usize) []const u8 {
    const o = utf8Off(s, kc);
    return s[o..@min(o + utf8Seq(s[o]), s.len)];
}
fn utf8Encode(cp: u21) []const u8 {
    var buf: [4]u8 = undefined;
    const q = std.unicode.utf8Encode(cp, &buf) catch return "";
    return std.fmt.allocPrint(arena(), "{s}", .{buf[0..q]}) catch @panic("mémoire");
}

fn erreurRuntime(msg: []const u8) noreturn {
    std.io.getStdErr().writer().print("Rose : erreur — {s}\n", .{msg}) catch {};
    std.process.exit(1);
}

fn echec(e: Erreur) noreturn {
    std.io.getStdErr().writer().print("Rose : erreur — {s}\n", .{texteErreur(e)}) catch {};
    std.process.exit(1);
}

// ── types statiques ───────────────────────────────────────────────────
const TyN = enum(i32) { vide = 0, entier = 1, rationnel = 2, flottant = 3, booleen = 4, chaine = 5, liste = 6, table = 7, valeur = 8 };

fn tyNom(t: TyN) []const u8 {
    return switch (t) {
        .entier => "entier", .rationnel => "rationnel", .flottant => "flottant",
        .booleen => "booleen", .chaine => "chaine", .liste => "liste", .vide => "vide",
    };
}

// ── jetons ────────────────────────────────────────────────────────────
const Tk = enum {
    eof, ident, mot, nombre, flottant_, chaine,
    plus, moins, etoile, slash, deuxpts, deuxpoints, percent, chapeau, bang,
    point, virgule, diventier, affect, lparen, rparen, lbrace, rbrace,
    lbracket, rbracket, semi, fleched, flecheg, arobase, pointdinterro,
    inf, sup, infeg, supeg, doubleeg, diff, environ, qmarkeg, tildeeg, racine,
};

const Token = struct { t: Tk, texte: []const u8, ligne: i64, colonne: i64 };

fn estMotCle(s: []const u8) bool {
    const mots = [_][]const u8{ "fn", "retour", "si", "sinon", "tant_que", "pour", "de", "vers",
        "vrai", "faux", "et", "ou", "non", "div", "entier", "rationnel", "flottant", "booleen", "chaine", "liste", "table" };
    for (mots) |m| if (std.mem.eql(u8, s, m)) return true;
    return false;
}

fn cpLettre(c: u21) bool { return (c >= 'a' and c <= 'z') or (c >= 'A' and c <= 'Z') or c == '_' or c > 127; }
fn cpIdentSuite(c: u21) bool { return cpLettre(c) or (c >= '0' and c <= '9'); }

const Lexer = struct {
    cs: []const u8,
    pos: usize = 0,
    ligne: i64 = 1,
    col: i64 = 1,
    toks: std.ArrayList(Token),

    fn init(src: []const u8) Lexer {
        return .{ .cs = src, .toks = std.ArrayList(Token).init(arena()) };
    }
    fn dec(self: *Lexer, k: usize) void {
        var n = k;
        while (n > 0) : (n -= 1) {
            if (self.pos < self.cs.len and self.cs[self.pos] == '\n') { self.ligne += 1; self.col = 1; }
            else self.col += 1;
            if (self.pos < self.cs.len) {
                const l = std.unicode.utf8ByteSequenceLength(self.cs[self.pos]) catch 1;
                self.pos += l;
            }
        }
    }
    fn suivant(self: *Lexer) u21 {
        const l = std.unicode.utf8ByteSequenceLength(self.cs[self.pos]) catch 1;
        return std.unicode.utf8Decode(self.cs[self.pos .. self.pos + l]) catch '?';
    }

    fn tokeniser(self: *Lexer) Er!void {
        while (self.pos < self.cs.len) {
            const c = self.suivant();
            if (c == ' ' or c == '\t' or c == '\r' or c == '\n') { self.dec(1); continue; }
            if (c == '#' or (c == '/' and self.pos + 1 < self.cs.len and self.cs[self.pos + 1] == '/')) {
                while (self.pos < self.cs.len and self.cs[self.pos] != '\n') self.dec(1);
                continue;
            }
            if (c == '"' or c == '\'') {
                const dl = self.ligne; const dc = self.col;
                const quote = c;
                self.dec(1);
                var buf = std.ArrayList(u8).init(arena());
                while (true) {
                    if (self.pos >= self.cs.len) return err(Erreur{ .msg = "Chaîne non terminée", .ligne = dl, .colonne = dc });
                    const cc = self.suivant();
                    if (cc == '\\') {
                        self.dec(1);
                        if (self.pos >= self.cs.len) return err(Erreur{ .msg = "Échappement en fin de fichier", .ligne = self.ligne, .colonne = self.col });
                        switch (self.suivant()) {
                            'n' => try buf.append('\n'),
                            't' => try buf.append('\t'),
                            '\\' => try buf.append('\\'),
                            '"' => try buf.append('"'),
                            '\'' => try buf.append('\''),
                            else => |x| { try buf.append('\\'); const l = std.unicode.utf8Encode(x, arena_buf[0..4]) catch unreachable; try buf.appendSlice(arena_buf[0..l]); },
                        }
                        self.dec(1);
                        continue;
                    }
                    if (cc == quote) { self.dec(1); break; }
                    const l = std.unicode.utf8ByteSequenceLength(self.cs[self.pos]) catch 1;
                    try buf.appendSlice(self.cs[self.pos .. self.pos + l]);
                    self.dec(1);
                }
                try self.toks.append(.{ .t = .chaine, .texte = buf.items, .ligne = dl, .colonne = dc });
                continue;
            }
            {
                var uni: ?Tk = null;
                if (c == '\u{2248}') uni = .environ else if (c == '\u{2264}') uni = .infeg else if (c == '\u{2265}') uni = .supeg
                else if (c == '\u{2260}') uni = .diff else if (c == '\u{00D7}' or c == '\u{22C5}' or c == '\u{00B7}') uni = .etoile
                else if (c == '\u{00F7}') uni = .diventier
                else if (c == '\u{2190}') uni = .affect else if (c == '\u{2192}') uni = .pointdinterro else if (c == '\u{221A}') uni = .racine;
                if (uni) |p| {
                    const l = std.unicode.utf8ByteSequenceLength(self.cs[self.pos]) catch 1;
                    self.toks.append(.{ .t = p, .texte = self.cs[self.pos .. self.pos + l], .ligne = self.ligne, .colonne = self.col }) catch @panic("mémoire");
                    self.dec(1);
                    continue;
                }
            }
            if (c == '\u{03C0}') {   // π : constante flottante 3,14159…
                try self.toks.append(.{ .t = .flottant_, .texte = "3.141592653589793", .ligne = self.ligne, .colonne = self.col });
                self.dec(1); continue;
            }
            if (c == '\u{00B2}' or c == '\u{00B3}') {   // x² ≡ x^2
                try self.toks.append(.{ .t = .chapeau, .texte = "^", .ligne = self.ligne, .colonne = self.col });
                try self.toks.append(.{ .t = .nombre, .texte = if (c == '\u{00B2}') "2" else "3", .ligne = self.ligne, .colonne = self.col });
                self.dec(1); continue;
            }
            if (c >= '0' and c <= '9') {
                const dl = self.ligne; const dc = self.col; const debut = self.pos;
                while (self.pos < self.cs.len and self.cs[self.pos] >= '0' and self.cs[self.pos] <= '9') self.dec(1);
                var flott = false;
                if (self.pos + 1 < self.cs.len and self.cs[self.pos] == '.' and self.cs[self.pos + 1] >= '0' and self.cs[self.pos + 1] <= '9') {
                    flott = true; self.dec(1);
                    while (self.pos < self.cs.len and self.cs[self.pos] >= '0' and self.cs[self.pos] <= '9') self.dec(1);
                }
                try self.toks.append(.{ .t = if (flott) .flottant_ else .nombre,
                    .texte = self.cs[debut..self.pos], .ligne = dl, .colonne = dc });
                continue;
            }
            if (c == '$' or cpLettre(c)) {
                const dl = self.ligne; const dc = self.col; const debut = self.pos;
                var dollar = false;
                if (c == '$') { dollar = true; self.dec(1); }
                if (self.pos >= self.cs.len or !cpLettre(self.suivant()))
                    return err(Erreur{ .msg = "Attente d'un nom de variable après '$'", .ligne = dl, .colonne = dc });
                while (self.pos < self.cs.len and cpIdentSuite(self.suivant())) self.dec(1);
                var texte = self.cs[debut..self.pos];
                if (dollar) texte = texte[1..];
                const tt: Tk = if (dollar or estMotCle(texte)) .ident else .mot;
                try self.toks.append(.{ .t = tt, .texte = texte, .ligne = dl, .colonne = dc });
                continue;
            }
            // paires et symboles
            var paire: ?Tk = null;
            if (self.pos + 1 < self.cs.len) {
                const a = self.cs[self.pos]; const b = self.cs[self.pos + 1];
                if (a == '<' and b == '=') paire = .infeg
                else if (a == '>' and b == '=') paire = .supeg
                else if (a == '=' and b == '=') paire = .doubleeg
                else if ((a == '!' or a == '/') and b == '=') paire = .diff
                else if (a == '?' and b == '=') paire = .qmarkeg
                else if ((a == '=' and b == '~') or (a == '~' and b == '=')) paire = .tildeeg
                else if (a == '>' and b == '>') paire = .fleched
                else if (a == '<' and b == '<') paire = .flecheg
                else if (a == '-' and b == '>') paire = .pointdinterro
                else if (a == ':' and b == ':') paire = .deuxpoints;
            }
            if (paire) |p| {
                try self.toks.append(.{ .t = p, .texte = self.cs[self.pos .. self.pos + 2], .ligne = self.ligne, .colonne = self.col });
                self.dec(2);
                continue;
            }
            if (c == '=') {
                try self.toks.append(.{ .t = .affect, .texte = "=", .ligne = self.ligne, .colonne = self.col });
                self.dec(1); continue;
            }
            const car: ?Tk = switch (c) {
                '+' => .plus, '-' => .moins, '*' => .etoile, '/' => .slash, ':' => .deuxpts,
                '%' => .percent, '^' => .chapeau, '!' => .bang, '.' => .point, ',' => .virgule,
                '?' => .qmarkeg, '(' => .lparen, ')' => .rparen, '{' => .lbrace, '}' => .rbrace,
                '[' => .lbracket, ']' => .rbracket, ';' => .semi, '<' => .inf, '>' => .sup,
                '@' => .arobase, else => null,
            };
            if (car) |p| {
                try self.toks.append(.{ .t = p, .texte = self.cs[self.pos .. self.pos + 1], .ligne = self.ligne, .colonne = self.col });
                self.dec(1); continue;
            }
            return err(Erreur{ .msg = "Caractère inattendu", .ligne = self.ligne, .colonne = self.col });
        }
        try self.toks.append(.{ .t = .eof, .texte = "", .ligne = self.ligne, .colonne = self.col });
    }
};
var arena_buf: [4]u8 = undefined;

// ── AST ───────────────────────────────────────────────────────────────
const AppelForm = struct { module: []const u8, nom: []const u8, args: []const *Expr };

const Expr = union(enum) {
    litteral: struct { texte: []const u8, flott: bool },
    chaine_lit: []const u8,
    booleen_lit: bool,
    liste_lit: []const *Expr,
    var_: []const u8,
    unaire: struct { op: []const u8, operande: *Expr },
    fact: *Expr,
    binaire: struct { op: []const u8, g: *Expr, d: *Expr },
    appel: AppelForm,
    index: struct { base: *Expr, ix: *Expr },
};

const Instr = union(enum) {
    afficher: *Expr,
    lire: []const u8,
    affect: struct { nom: []const u8, expr: *Expr },
    si: struct { cond: *Expr, alors: []const *Instr, sinon: ?[]const *Instr },
    tant_que: struct { cond: *Expr, corps: []const *Instr },
    pour: struct { var_: []const u8, debut: *Expr, fin: *Expr, corps: []const *Instr },
    retour: ?*Expr,
    expr: *Expr,
    bloc: []const *Instr,
};

const Signature = struct {
    nom: []const u8,
    params: []const struct { []const u8, []const u8 },
    retour: []const u8,
    corps: ?[]const *Instr,
    ligne: i64,
};

const Programme = struct {
    signatures: std.StringHashMap(*Signature),
    instructions: []*Instr,
};

// ── parseur ───────────────────────────────────────────────────────────
const Parser = struct {
    toks: []Token,
    pos: usize = 0,

    fn voir(self: *Parser, d: usize) Token {
        const p = @min(self.pos + d, self.toks.len - 1);
        return self.toks[p];
    }
    fn suivant(self: *Parser) Token {
        const t = self.voir(0);
        if (t.t != .eof) self.pos += 1;
        return t;
    }
    fn est(self: *Parser, ks: []const Tk) bool {
        for (ks) |k| if (self.voir(0).t == k) return true;
        return false;
    }
    fn estNom(self: *Parser, texte: []const u8) bool {
        const t = self.voir(0);
        return (t.t == .ident or t.t == .mot) and std.mem.eql(u8, t.texte, texte);
    }
    fn accepter(self: *Parser, ks: []const Tk) bool {
        if (self.est(ks)) { _ = self.suivant(); return true; }
        return false;
    }
    fn accepterNom(self: *Parser, texte: []const u8) bool {
        if (self.estNom(texte)) { _ = self.suivant(); return true; }
        return false;
    }
    fn exiger(self: *Parser, t: Tk, quoi: []const u8) Er!Token {
        const tok = self.suivant();
        if (tok.t != t) return err(Erreur{ .msg = quoi, .ligne = tok.ligne, .colonne = tok.colonne });
        return tok;
    }
    fn nomIdent(self: *Parser) Er![]const u8 {
        const t = self.suivant();
        if (t.t != .ident and t.t != .mot) return err(Erreur{ .msg = "Attendu un identifiant", .ligne = t.ligne, .colonne = t.colonne });
        return t.texte;
    }

    fn nouveauE(self: *Parser, e: Expr) Er!*Expr {
        _ = self;
        const p = try arena().create(Expr);
        p.* = e;
        return p;
    }
    fn nouveauI(self: *Parser, i: Instr) Er!*Instr {
        _ = self;
        const p = try arena().create(Instr);
        p.* = i;
        return p;
    }

    fn programmer(self: *Parser) Er!Programme {
        var prog = Programme{ .signatures = std.StringHashMap(*Signature).init(arena()), .instructions = ArrayListInstr().init(arena()).items };
        var ins = ArrayListInstr().init(arena());
        var ann_dll: []const u8 = ""; var ann_son: []const u8 = ""; var ann_sym: []const u8 = ""; var ann_actif = false;
        while (self.voir(0).t != .eof) {
            if (self.accepter(&.{.semi})) continue;
            const t = self.voir(0);
            if (t.t == .arobase and (self.voir(1).t == .ident or self.voir(1).t == .mot) and
                (std.mem.eql(u8, self.voir(1).texte, "dll") or std.mem.eql(u8, self.voir(1).texte, "sym")))
            {
                _ = self.suivant();
                const genre = try self.nomIdent();
                _ = try self.exiger(.lparen, "Attendu '(' après @dll/@sym");
                if (std.mem.eql(u8, genre, "dll")) {
                    ann_dll = (try self.exiger(.chaine, "nom de bibliothèque")).texte;
                    if (self.accepter(&.{.virgule})) ann_son = (try self.exiger(.chaine, "nom de fichier")).texte;
                } else ann_sym = (try self.exiger(.chaine, "nom de symbole")).texte;
                _ = try self.exiger(.rparen, "Attendu ')'");
                ann_actif = true;
                continue;
            }
            if (self.estNom("fn")) {
                const sig = try self.defFonction(&ann_dll, &ann_son, &ann_sym);
                ann_dll = ""; ann_son = ""; ann_sym = ""; ann_actif = false;
                if (prog.signatures.contains(sig.nom))
                    return err(Erreur{ .msg = "Fonction définie deux fois", .ligne = sig.ligne });
                try prog.signatures.put(sig.nom, sig);
                continue;
            }
            if (ann_actif) return err(Erreur{ .msg = "Annotation @dll/@sym sans fonction juste après", .ligne = t.ligne, .colonne = t.colonne });
            try ins.append(try self.instruction());
        }
        prog.instructions = ins.items;
        return prog;
    }

    fn defFonction(self: *Parser, dll: *[]const u8, son: *[]const u8, sym: *[]const u8) Er!*Signature {
        const tok = self.suivant();
        const nom = try self.nomIdent();
        _ = try self.exiger(.lparen, "Attendu '('");
        var params = std.ArrayList(struct { []const u8, []const u8 }).init(arena());
        if (!self.est(&.{.rparen})) {
            while (true) {
                const pname = try self.nomIdent();
                _ = try self.exiger(.deuxpts, "Attendu ':' (nom: type)");
                const pt = try self.typeRose();
                try params.append(.{ pname, pt });
                if (!self.accepter(&.{.virgule})) break;
            }
        }
        _ = try self.exiger(.rparen, "Attendu ')'");
        var retour: []const u8 = "vide";
        var a_retour = false;
        if (self.accepter(&.{.pointdinterro})) { retour = try self.typeRose(); a_retour = true; }
        var corps: ?[]const *Instr = null;
        if (self.voir(0).t == .lbrace) corps = (try self.bloc()).items;
        if (corps == null and !a_retour)
            return err(Erreur{ .msg = "Une fonction externe doit déclarer un type de retour", .ligne = tok.ligne });
        const sig = try arena().create(Signature);
        sig.* = .{ .nom = nom, .params = params.items, .retour = retour, .corps = corps, .ligne = tok.ligne };
        _ = dll; _ = son; _ = sym;
        return sig;
    }

    fn typeRose(self: *Parser) Er![]const u8 {
        const t = self.suivant();
        if (t.t != .ident and t.t != .mot) return err(Erreur{ .msg = "Attendu un type", .ligne = t.ligne, .colonne = t.colonne });
        return t.texte;
    }

    fn instruction(self: *Parser) Er!*Instr {
        const t = self.voir(0);
        if (t.t == .lbrace) return self.nouveauI(.{ .bloc = (try self.bloc()).items });
        if (t.t == .fleched) { _ = self.suivant(); return self.nouveauI(.{ .afficher = try self.expression() }); }
        if (t.t == .flecheg) { _ = self.suivant(); return self.nouveauI(.{ .lire = try self.nomIdent() }); }
        if (t.t == .arobase) {
            _ = self.suivant();
            const cond = try self.parentheser();
            return self.nouveauI(.{ .tant_que = .{ .cond = cond, .corps = (try self.corpsChevron()).items } });
        }
        if (self.estNom("si")) {
            _ = self.suivant();
            const cond = try self.parentheser();
            const alors = (try self.bloc()).items;
            var sinon: ?[]const *Instr = null;
            if (self.accepterNom("sinon")) {
                sinon = if (self.voir(0).t == .lbrace) (try self.bloc()).items
                        else blk: {
                    var l = ArrayListInstr().init(arena());
                    try l.append(try self.instruction());
                    break :blk l.items;
                };
            }
            return self.nouveauI(.{ .si = .{ .cond = cond, .alors = alors, .sinon = sinon } });
        }
        if (self.estNom("tant_que")) {
            _ = self.suivant();
            return self.nouveauI(.{ .tant_que = .{ .cond = try self.parentheser(), .corps = (try self.bloc()).items } });
        }
        if (self.estNom("pour")) {
            _ = self.suivant();
            const var_ = try self.nomIdent();
            if (!self.accepterNom("de")) return err(Erreur{ .msg = "Attendu 'de' dans pour…de…vers", .ligne = self.voir(0).ligne, .colonne = self.voir(0).colonne });
            const debut = try self.expression();
            if (!self.accepterNom("vers")) return err(Erreur{ .msg = "Attendu 'vers' dans pour…de…vers", .ligne = self.voir(0).ligne, .colonne = self.voir(0).colonne });
            const fin = try self.expression();
            return self.nouveauI(.{ .pour = .{ .var_ = var_, .debut = debut, .fin = fin, .corps = (try self.bloc()).items } });
        }
        if (self.estNom("retour")) {
            _ = self.suivant();
            const e: ?*Expr = if (self.est(&.{ .semi, .rbrace, .eof })) null else try self.expression();
            return self.nouveauI(.{ .retour = e });
        }
        if ((t.t == .ident or t.t == .mot) and self.voir(1).t == .affect) {
            const nom = try self.nomIdent();
            _ = self.suivant();
            return self.nouveauI(.{ .affect = .{ .nom = nom, .expr = try self.expression() } });
        }
        return self.nouveauI(.{ .expr = try self.expression() });
    }

    fn parentheser(self: *Parser) Er!*Expr {
        _ = try self.exiger(.lparen, "Attendu '('");
        const e = try self.expression();
        _ = try self.exiger(.rparen, "Attendu ')'");
        return e;
    }
    fn corpsChevron(self: *Parser) Er!ArrayListInstr() {
        _ = try self.exiger(.lbracket, "Attendu '[' de corps");
        var instrs = ArrayListInstr().init(arena());
        while (!self.est(&.{ .rbracket, .eof })) {
            try instrs.append(try self.instruction());
            if (!self.accepter(&.{.semi})) break;
        }
        _ = try self.exiger(.rbracket, "Attendu ']' de corps");
        return instrs;
    }
    fn bloc(self: *Parser) Er!ArrayListInstr() {
        _ = try self.exiger(.lbrace, "Attendu '{'");
        var instrs = ArrayListInstr().init(arena());
        while (!self.est(&.{ .rbrace, .eof })) {
            if (self.accepter(&.{.semi})) continue;
            try instrs.append(try self.instruction());
            _ = self.accepter(&.{.semi});
        }
        _ = try self.exiger(.rbrace, "Attendu '}'");
        return instrs;
    }

    fn expression(self: *Parser) Er!*Expr { return self.ouExpr(); }

    fn ouExpr(self: *Parser) Er!*Expr {
        var g = try self.etExpr();
        while (self.estNom("ou")) {
            _ = self.suivant();
            const d = try self.etExpr();
            g = try self.nouveauE(.{ .binaire = .{ .op = "ou", .g = g, .d = d } });
        }
        return g;
    }
    fn etExpr(self: *Parser) Er!*Expr {
        var g = try self.nonExpr();
        while (self.estNom("et")) {
            _ = self.suivant();
            const d = try self.nonExpr();
            g = try self.nouveauE(.{ .binaire = .{ .op = "et", .g = g, .d = d } });
        }
        return g;
    }
    fn nonExpr(self: *Parser) Er!*Expr {
        if (self.estNom("non")) {
            _ = self.suivant();
            return self.nouveauE(.{ .unaire = .{ .op = "non", .operande = try self.nonExpr() } });
        }
        return self.comparaison();
    }
    fn comparaison(self: *Parser) Er!*Expr {
        var g = try self.somme();
        while (true) {
            const t = self.voir(0);
            const op: []const u8 = switch (t.t) {
                .inf => "<", .infeg => "<=", .sup => ">", .supeg => ">=",
                .doubleeg => "==", .diff => "!=", .environ => "≈",
                .qmarkeg => "?=", .tildeeg => "=~",
                else => break,
            };
            _ = self.suivant();
            const d = try self.somme();
            g = try self.nouveauE(.{ .binaire = .{ .op = op, .g = g, .d = d } });
        }
        return g;
    }
    fn somme(self: *Parser) Er!*Expr {
        var g = try self.produit();
        while (self.est(&.{ .plus, .moins })) {
            const t = self.suivant();
            const d = try self.produit();
            g = try self.nouveauE(.{ .binaire = .{ .op = if (t.t == .plus) "+" else "-", .g = g, .d = d } });
        }
        return g;
    }
    fn produit(self: *Parser) Er!*Expr {
        var g = try self.puissance();
        while (true) {
            const t = self.voir(0);
            const op: ?[]const u8 = switch (t.t) {
                .etoile => "*",
                .slash => "/",
                .percent => "%",
                .diventier => "div",
                else => if (self.estNom("div")) "div" else null,
            };
            if (op == null) break;
            _ = self.suivant();
            const d = try self.puissance();
            g = try self.nouveauE(.{ .binaire = .{ .op = op.?, .g = g, .d = d } });
        }
        return g;
    }
    fn puissance(self: *Parser) Er!*Expr {
        const g = try self.factoriellePost();
        if (self.accepter(&.{.chapeau})) {
            const d = try self.puissance();
            return self.nouveauE(.{ .binaire = .{ .op = "^", .g = g, .d = d } });
        }
        return g;
    }
    fn factoriellePost(self: *Parser) Er!*Expr {
        var g = try self.unaire();
        while (self.accepter(&.{.bang})) g = try self.nouveauE(.{ .fact = g });
        return g;
    }
    fn unaire(self: *Parser) Er!*Expr {
        if (self.accepter(&.{.racine})) {
            const op = try self.unaire();
            const demi = try arena().create(Expr);
            demi.* = .{ .litteral = .{ .texte = "0.5", .flott = true } };
            return self.nouveauE(.{ .binaire = .{ .op = "^", .g = op, .d = demi } });
        }
        if (self.accepter(&.{.moins}))
            return self.nouveauE(.{ .unaire = .{ .op = "-", .operande = try self.unaire() } });
        if (self.accepter(&.{.plus})) return self.unaire();
        return self.postfixe();
    }
    fn postfixe(self: *Parser) Er!*Expr {
        var g = try self.atome();
        while (true) {
            if (self.accepter(&.{.lparen})) {
                var module: []const u8 = ""; var nom: []const u8 = "";
                switch (g.*) {
                    .appel => |a| {
                        if (a.args.len == 0 and a.module.len > 0) { module = a.module; nom = a.nom; }
                        else return err(Erreur{ .msg = "Seul un nom de fonction peut être appelé", .ligne = self.voir(0).ligne, .colonne = self.voir(0).colonne });
                    },
                    .var_ => |n| { nom = n; },
                    else => return err(Erreur{ .msg = "Seul un nom de fonction peut être appelé", .ligne = self.voir(0).ligne, .colonne = self.voir(0).colonne }),
                }
                var args = std.ArrayList(*Expr).init(arena());
                if (!self.est(&.{.rparen})) {
                    while (true) {
                        try args.append(try self.expression());
                        if (!self.accepter(&.{.virgule})) break;
                    }
                }
                _ = try self.exiger(.rparen, "Attendu ')' de appel");
                g = try self.nouveauE(.{ .appel = .{ .module = module, .nom = nom, .args = args.items } });
                continue;
            }
            if (self.accepter(&.{.lbracket})) {
                if (self.accepter(&.{.deuxpts})) {
                    // tranche s[:b]
                    const debut = try self.nouveauE(.{ .litteral = .{ .texte = "0", .flott = false } });
                    const consumed = self.accepter(&.{.rbracket});
                    const fin = if (consumed) blk: {
                        const lng = try self.nouveauE(.{ .appel = .{ .module = "", .nom = "longueur", .args = &[_]*Expr{g} } });
                        break :blk lng;
                    } else blk2: {
                        const e = try self.expression();
                        _ = try self.exiger(.rbracket, "Attendu ']' de tranche");
                        break :blk2 e;
                    };
                    const args = try arena().alloc(*Expr, 3);
                    args[0] = g; args[1] = debut; args[2] = fin;
                    g = try self.nouveauE(.{ .appel = .{ .module = "", .nom = "sous_chaine", .args = args } });
                    continue;
                }
                const ix = try self.expression();
                if (self.accepter(&.{.deuxpts})) {
                    // tranche s[a:b] ou s[a:]
                    const debut = ix;
                    const consumed = self.accepter(&.{.rbracket});
                    const fin = if (consumed) blk: {
                        const lng = try self.nouveauE(.{ .appel = .{ .module = "", .nom = "longueur", .args = &[_]*Expr{g} } });
                        break :blk lng;
                    } else blk2: {
                        const e = try self.expression();
                        _ = try self.exiger(.rbracket, "Attendu ']' de tranche");
                        break :blk2 e;
                    };
                    const args = try arena().alloc(*Expr, 3);
                    args[0] = g; args[1] = debut; args[2] = fin;
                    g = try self.nouveauE(.{ .appel = .{ .module = "", .nom = "sous_chaine", .args = args } });
                    continue;
                }
                _ = try self.exiger(.rbracket, "Attendu ']' d'index");
                g = try self.nouveauE(.{ .index = .{ .base = g, .ix = ix } });
                continue;
            }
            return g;
        }
    }
    fn atome(self: *Parser) Er!*Expr {
        const t = self.suivant();
        switch (t.t) {
            .nombre => return self.nouveauE(.{ .litteral = .{ .texte = t.texte, .flott = false } }),
            .flottant_ => return self.nouveauE(.{ .litteral = .{ .texte = t.texte, .flott = true } }),
            .chaine => return self.nouveauE(.{ .chaine_lit = t.texte }),
            .lbracket => {
                var elements = std.ArrayList(*Expr).init(arena());
                if (!self.est(&.{.rbracket})) {
                    while (true) {
                        try elements.append(try self.expression());
                        if (!self.accepter(&.{.virgule})) break;
                    }
                }
                _ = try self.exiger(.rbracket, "Attendu ']' de liste");
                return self.nouveauE(.{ .liste_lit = elements.items });
            },
            .lparen => {
                const e = try self.expression();
                _ = try self.exiger(.rparen, "Attendu ')'");
                return e;
            },
            .ident, .mot => {
                if (std.mem.eql(u8, t.texte, "vrai")) return self.nouveauE(.{ .booleen_lit = true });
                if (std.mem.eql(u8, t.texte, "faux")) return self.nouveauE(.{ .booleen_lit = false });
                var module: []const u8 = "";
                if (self.voir(0).t == .deuxpoints) {
                    module = t.texte;
                    _ = self.suivant();
                    const nom2 = try self.nomIdent();
                    return self.nouveauE(.{ .appel = .{ .module = module, .nom = nom2, .args = &.{} } });
                }
                return self.nouveauE(.{ .var_ = t.texte });
            },
            else => return err(Erreur{ .msg = std.fmt.allocPrint(arena(), "Attente d'une expression, trouvé '{s}'", .{t.texte}) catch "Attente d'une expression", .ligne = t.ligne, .colonne = t.colonne }),
        }
    }
};

fn ArrayListInstr() type { return std.ArrayList(*Instr); }

// ── repli de constantes ───────────────────────────────────────────────
fn estLitteral(e: *Expr) bool { return e.* == .litteral; }

fn littDepuisQ(q: Q) *Expr {
    const qc = Q.canon(q);
    const num = Q.numerateur(qc); const den = Q.denominateur(qc);
    const p = arena().create(Expr) catch @panic("mémoire");
    if (std.mem.eql(u8, Z.texte(den), "1"))
        p.* = .{ .litteral = .{ .texte = Z.texte(num), .flott = false } }
    else
        p.* = .{ .litteral = .{ .texte = std.fmt.allocPrint(arena(), "{s}/{s}", .{ Z.texte(num), Z.texte(den) }) catch @panic("mémoire"), .flott = false } };
    return p;
}
fn littDepuisZ(z: Z) *Expr {
    const p = arena().create(Expr) catch @panic("mémoire");
    p.* = .{ .litteral = .{ .texte = Z.texte(z), .flott = false } };
    return p;
}

fn factArbre(i: u64, j: u64) Z {
    if (i > j) return Z.i(1);
    if (i == j) return Z.u(i);
    const m = i + (j - i) / 2;
    return Z.mul(factArbre(i, m), factArbre(m + 1, j));
}

fn replier(e: *Expr) Er!*Expr {
    switch (e.*) {
        .liste_lit => |els| {
            const out = try arena().alloc(*Expr, els.len);
            for (els, 0..) |el, k| out[k] = try replier(el);
            const p = try arena().create(Expr);
            p.* = .{ .liste_lit = out };
            return p;
        },
        .unaire => |u| {
            const op = try replier(u.operande);
            if (std.mem.eql(u8, u.op, "-") and op.* == .litteral) {
                if (op.litteral.flott) return littDepuisF64(-litValeurD(op.litteral.texte));
                if (Q.depuisTexte(op.litteral.texte)) |q| return littDepuisQ(Q.neg(q));
            }
            const p = try arena().create(Expr);
            p.* = .{ .unaire = .{ .op = u.op, .operande = op } };
            return p;
        },
        .fact => |op| {
            const op2 = try replier(op);
            if (op2.* == .litteral and !op2.litteral.flott and std.mem.indexOf(u8, op2.litteral.texte, "/") == null) {
                if (Z.depuisTexte(op2.litteral.texte)) |n| {
                    if (n.sgn() >= 0 and n.cmp(Z.u(20000)) <= 0) {
                        if (n.cmp(Z.u(2)) < 0) return littDepuisZ(Z.i(1));
                        return littDepuisZ(factArbre(2, n.enU64()));
                    }
                }
            }
            const p = try arena().create(Expr);
            p.* = .{ .fact = op2 };
            return p;
        },
        .binaire => |b| {
            const g = try replier(b.g);
            const d = try replier(b.d);
            if (g.* == .litteral and d.* == .litteral) {
                if (try repliBinaire(b.op, g.litteral.texte, g.litteral.flott, d.litteral.texte, d.litteral.flott)) |r| return r;
            }
            const p = try arena().create(Expr);
            p.* = .{ .binaire = .{ .op = b.op, .g = g, .d = d } };
            return p;
        },
        .appel => |a| {
            const out = try arena().alloc(*Expr, a.args.len);
            for (a.args, 0..) |el, k| out[k] = try replier(el);
            const p = try arena().create(Expr);
            p.* = .{ .appel = .{ .module = a.module, .nom = a.nom, .args = out } };
            return p;
        },
        .index => |ix| {
            const p = try arena().create(Expr);
            p.* = .{ .index = .{ .base = try replier(ix.base), .ix = try replier(ix.ix) } };
            return p;
        },
        else => return e,
    }
}

fn replierInstr(i: *Instr) Er!*Instr {
    switch (i.*) {
        .afficher => |e| i.* = .{ .afficher = try replier(e) },
        .affect => |a| i.* = .{ .affect = .{ .nom = a.nom, .expr = try replier(a.expr) } },
        .retour => |eo| {
            if (eo) |e| {
                i.* = .{ .retour = try replier(e) };
            }
        },
        .expr => |e| i.* = .{ .expr = try replier(e) },
        .si => |s| {
            const cond = try replier(s.cond);
            for (s.alors) |ins| _ = try replierInstr(ins);
            if (s.sinon) |sn| {
                for (sn) |ins| {
                    _ = try replierInstr(ins);
                }
            }
            i.* = .{ .si = .{ .cond = cond, .alors = s.alors, .sinon = s.sinon } };
        },
        .tant_que => |s| {
            const cond = try replier(s.cond);
            for (s.corps) |ins| _ = try replierInstr(ins);
            i.* = .{ .tant_que = .{ .cond = cond, .corps = s.corps } };
        },
        .pour => |s| {
            i.* = .{ .pour = .{ .var_ = s.var_, .debut = try replier(s.debut), .fin = try replier(s.fin), .corps = s.corps } };
            for (s.corps) |ins| _ = try replierInstr(ins);
        },
        .bloc => |b| {
            for (b) |ins| {
                _ = try replierInstr(ins);
            }
        },
        else => {},
    }
    return i;
}

fn valeurQ(texte: []const u8) ?Q {
    if (std.mem.indexOf(u8, texte, "/") == null) {
        if (Z.depuisTexte(texte)) |z| return Q.depuisZ(z);
        return null;
    }
    return Q.depuisTexte(texte);
}

fn litValeurD(texte: []const u8) f64 {
    const zs = arena().dupeZ(u8, texte) catch @panic("mémoire");
    var fin: [*c]u8 = undefined;
    const a = strtod(zs.ptr, &fin);
    // littéral « p/q » replié : strtod s'arrête à '/' → diviser par la suite
    if (fin > zs.ptr and fin[0] == '/') {
        var fin2: [*c]u8 = undefined;
        const b = strtod(fin + 1, &fin2);
        return a / b;
    }
    return a;
}

fn littDepuisF64(d: f64) *Expr {
    const p = arena().create(Expr) catch @panic("mémoire");
    p.* = .{ .litteral = .{ .texte = texteFlottant(d), .flott = true } };
    return p;
}

fn repliBinaire(op: []const u8, gt: []const u8, gf: bool, dt: []const u8, df: bool) Er!?*Expr {
    if (gf or df) {
        // miroir fold_bin C++ : strtod préfixe sur les deux textes (« 1/3 » vaut 1.0 ici)
        const a = litValeurD(gt);
        const b = litValeurD(dt);
        var r: f64 = undefined;
        if (std.mem.eql(u8, op, "+")) r = a + b
        else if (std.mem.eql(u8, op, "-")) r = a - b
        else if (std.mem.eql(u8, op, "*")) r = a * b
        else if (std.mem.eql(u8, op, "/")) {
            if (b == 0) return null;
            r = a / b;
        } else if (std.mem.eql(u8, op, "^")) r = std.math.pow(f64, a, b)
        else return null;
        if (std.math.isNan(r) or std.math.isInf(r)) return null;
        return littDepuisF64(r);
    }
    if (std.mem.eql(u8, op, "≈") or std.mem.eql(u8, op, "==") or std.mem.eql(u8, op, "!=") or
        std.mem.eql(u8, op, "%") or std.mem.eql(u8, op, "<") or std.mem.eql(u8, op, "<=") or
        std.mem.eql(u8, op, ">") or std.mem.eql(u8, op, ">=") or std.mem.eql(u8, op, "?=") or
        std.mem.eql(u8, op, "=~")) return null;
    const a = valeurQ(gt) orelse return null;
    const b = valeurQ(dt) orelse return null;
    if (std.mem.eql(u8, op, "+")) return littDepuisQ(Q.add(a, b));
    if (std.mem.eql(u8, op, "-")) return littDepuisQ(Q.sub(a, b));
    if (std.mem.eql(u8, op, "*")) return littDepuisQ(Q.mul(a, b));
    if (std.mem.eql(u8, op, "/")) {
        if (b.sgn() == 0) return err(Erreur{ .msg = "Division par zéro dans une constante" });
        return littDepuisQ(Q.div(a, b));
    }
    if (std.mem.eql(u8, op, "^")) {
        if (!std.mem.eql(u8, Z.texte(Q.denominateur(b)), "1")) return null;
        const expo = Q.numerateur(b);
        if (expo.sgn() < 0 and a.sgn() == 0) return err(Erreur{ .msg = "0^négatif dans une constante" });
        if (!expo.tientI64()) return null;
        const e = expo.enI64();
        var num: Z = undefined; var den: Z = undefined;
        if (e >= 0) {
            num = Z.powU(Q.numerateur(a), @intCast(e));
            den = Z.powU(Q.denominateur(a), @intCast(e));
        } else {
            const k: u64 = @intCast(-@as(i128, e));
            num = Z.powU(Q.denominateur(a), k);
            den = Z.powU(Q.numerateur(a), k);
        }
        return littDepuisQ(Q.avecNumDen(num, den));
    }
    if (std.mem.eql(u8, op, "div")) {
        if (b.sgn() == 0) return err(Erreur{ .msg = "division par zéro dans une constante" });
        const q = Q.div(a, b);
        return littDepuisZ(Z.fdivQ(Q.numerateur(q), Q.denominateur(q)));
    }
    return null;
}

// ── valeurs ───────────────────────────────────────────────────────────
const TableEntry = struct { cle: Value, val: Value };
const Tampon = struct { b: []u8 };
const Value = union(enum) {
    entier: Z,
    rationnel: Q,
    flottant: f64,
    booleen: bool,
    chaine: []const u8,
    liste: *std.ArrayList(Value),
    table: *std.ArrayList(TableEntry),
    tampon: *Tampon,   // buffer contigu d'octets, référence partagée
    vide,

    fn deS(s: []const u8) Value { return .{ .chaine = s }; }
    fn deZ(z: Z) Value { return .{ .entier = z }; }
    fn deI(v: i64) Value { return .{ .entier = Z.i(v) }; }
    fn deT(t: *std.ArrayList(TableEntry)) Value { return .{ .table = t }; }
};

fn versZ(v: Value) Z {
    return switch (v) {
        .entier => |z| z,
        .booleen => |b| Z.i(if (b) 1 else 0),
        .rationnel => |q| Z.fdivQ(Q.numerateur(q), Q.denominateur(q)),
        .flottant => |d| Z.deF64(d),
        else => erreurRuntime("conversion vers entier impossible"),
    };
}
fn versQ(v: Value) Q {
    return switch (v) {
        .entier => |z| Q.depuisZ(z),
        .booleen => |b| Q.depuisZ(Z.i(if (b) 1 else 0)),
        .rationnel => |q| q,
        .flottant => |d| if (std.math.isNan(d) or std.math.isInf(d))
            erreurRuntime("flottant infini/non-nombre sans rationnel correspondant")
        else Q.depuisF64(d),
        else => erreurRuntime("conversion vers rationnel impossible"),
    };
}
fn versD(v: Value) f64 {
    return switch (v) {
        .flottant => |d| d,
        .entier => |z| Z.enF64(z),
        .rationnel => |q| Q.enF64(q),
        .booleen => |b| if (b) 1.0 else 0.0,
        else => erreurRuntime("conversion vers flottant impossible"),
    };
}

fn chaineVersEntier(ch: []const u8) Z {
    if (Z.depuisTexte(ch)) |z| return z;
    if (std.mem.indexOfScalar(u8, ch, '/')) |p| {
        const n = Z.depuisTexte(ch[0..p]);
        const d = Z.depuisTexte(ch[p + 1 ..]);
        if (n != null and d != null and d.?.sgn() != 0) return Z.fdivQ(n.?, d.?);
        erreurRuntime("conversion chaine → entier impossible");
    }
    if (std.mem.indexOfScalar(u8, ch, '.')) |p| {
        if (Z.depuisTexte(ch[0..p])) |z| return z;
    }
    erreurRuntime("conversion chaine → entier impossible");
}

// ── %g fidèle (via snprintf de la libc, comme le moteur C++) ─────────
extern fn snprintf(buf: [*c]u8, n: usize, fmt: [*c]const u8, ...) c_int;
extern fn strlen(s: [*c]const u8) usize;
extern fn strtod(s: [*:0]const u8, fin: *[*c]u8) f64;

fn gPrecC(p: usize, d: f64) []const u8 {
    const fs = std.fmt.allocPrint(arena(), "%.{d}g", .{p}) catch @panic("mémoire");
    const fmtz = arena().dupeZ(u8, fs) catch @panic("mémoire");
    var buf: [48]u8 = undefined;
    _ = snprintf(&buf, buf.len, fmtz.ptr, d);
    const n = strlen(&buf);
    return arena().dupe(u8, buf[0..n]) catch @panic("mémoire");
}

fn strtodFidele(s: []const u8) ?f64 {
    if (s.len >= 512) return null;
    const zs = arena().dupeZ(u8, s) catch @panic("mémoire");
    var fin: [*c]u8 = undefined;
    return strtod(zs, &fin);
}

fn texteFlottant(d: f64) []const u8 {
    if (std.math.isNan(d)) return "nan";
    if (std.math.isInf(d)) return if (d > 0) "inf" else "-inf";
    var p: usize = 1;
    while (p <= 17) : (p += 1) {
        const s = gPrecC(p, d);
        if (strtodFidele(s)) |r| {
            if (r == d) {
                if (std.mem.indexOfScalar(u8, s, '.') == null and std.mem.indexOfScalar(u8, s, 'e') == null)
                    return std.fmt.allocPrint(arena(), "{s}.0", .{s}) catch @panic("mémoire");
                return s;
            }
        }
    }
    return gPrecC(17, d);
}

// ── affichage ─────────────────────────────────────────────────────────
fn textePeriode(num: Z, den: Z) []const u8 {
    if (num.chiffres10() + den.chiffres10() > 500)
        return std.fmt.allocPrint(arena(), "{s}/{s}", .{ Z.texte(num), Z.texte(den) }) catch @panic("mémoire");
    const entier = Z.fdivQ(num, den);
    var reste = Z.fdivR(Z.abs(num), den);
    var digits = std.ArrayList(u8).init(arena());
    var restes = std.ArrayList(Z).init(arena());
    var peri: i64 = -1;
    while (reste.sgn() != 0 and digits.items.len < 200) {
        for (restes.items, 0..) |r, k| if (Z.cmp(r, reste) == 0) { peri = @intCast(k); break; };
        if (peri >= 0) break;
        restes.append(reste) catch @panic("mémoire");
        reste = Z.mulU(reste, 10);
        const dig = Z.fdivQ(reste, den);
        digits.append(@intCast(Z.enU64(dig))) catch @panic("mémoire");
        reste = Z.fdivR(reste, den);
    }
    var s = std.ArrayList(u8).init(arena());
    s.appendSlice(Z.texte(entier)) catch @panic("mémoire");
    s.append('.') catch @panic("mémoire");
    if (peri >= 0) {
        for (digits.items[0..@intCast(peri)]) |dd| s.append('0' + dd) catch @panic("mémoire");
        s.append('[') catch @panic("mémoire");
        for (digits.items[@intCast(peri)..]) |dd| s.append('0' + dd) catch @panic("mémoire");
        s.append(']') catch @panic("mémoire");
    } else {
        for (digits.items) |dd| s.append('0' + dd) catch @panic("mémoire");
        if (digits.items.len >= 200) s.appendSlice("…") catch @panic("mémoire");
    }
    return s.items;
}

fn ecriStr(v: Value) []const u8 {
    switch (v) {
        .entier => |z| return Z.texte(z),
        .rationnel => |q| {
            const qc = Q.canon(q);
            const num = Q.numerateur(qc); const den = Q.denominateur(qc);
            if (std.mem.eql(u8, Z.texte(den), "1")) return Z.texte(num);
            return textePeriode(num, den);
        },
        .flottant => |d| {
            if (std.math.isNan(d)) return "non_nombre";
            if (std.math.isInf(d)) return if (d > 0) "infini" else "-infini";
            if (d == @floor(d) and @abs(d) < 1e16)
                return std.fmt.allocPrint(arena(), "{d}", .{@as(i64, @intFromFloat(d))}) catch @panic("mémoire");
            return texteFlottant(d);
        },
        .booleen => |b| return if (b) "vrai" else "faux",
        .chaine => |s| return s,
        .tampon => erreurRuntime("Un tampon ne s'affiche pas directement — utiliser chaine(tampon)"),
        .liste => |l| {
            var out = std.ArrayList(u8).init(arena());
            out.append('[') catch @panic("mémoire");
            for (l.items, 0..) |el, k| {
                if (k > 0) out.appendSlice(", ") catch @panic("mémoire");
                out.appendSlice(ecriStr(el)) catch @panic("mémoire");
            }
            out.append(']') catch @panic("mémoire");
            return out.items;
        },
        .vide => return "",
        .table => |t| {
            var out = std.ArrayList(u8).init(arena());
            out.append('[') catch @panic("mémoire");
            for (t.items, 0..) |kv, k| {
                if (k > 0) out.appendSlice(", ") catch @panic("mémoire");
                out.appendSlice(ecriStr(kv.cle)) catch @panic("mémoire");
                out.appendSlice(" => ") catch @panic("mémoire");
                out.appendSlice(ecriStr(kv.val)) catch @panic("mémoire");
            }
            out.append(']') catch @panic("mémoire");
            return out.items;
        },
    }
}

fn chaineVersFlottant(ch: []const u8) f64 {
    const zs = arena().dupeZ(u8, ch) catch @panic("mémoire");
    var fin: [*c]u8 = undefined;
    const d = strtod(zs.ptr, &fin);
    if (fin == @as([*c]u8, @ptrCast(zs.ptr))) {
        if (std.mem.eql(u8, ch, "infini")) return std.math.inf(f64);
        erreurRuntime("conversion chaine → flottant impossible");
    }
    return d;
}

fn chaineDe(v: Value) []const u8 {
    switch (v) {
        .tampon => |t| return t.b,
        .entier => |z| return Z.texte(z),
        .rationnel => |q| {
            const qc = Q.canon(q);
            const num = Q.numerateur(qc); const den = Q.denominateur(qc);
            if (std.mem.eql(u8, Z.texte(den), "1")) return Z.texte(num);
            return std.fmt.allocPrint(arena(), "{s}/{s}", .{ Z.texte(num), Z.texte(den) }) catch @panic("mémoire");
        },
        else => return ecriStr(v),
    }
}

fn convertir(v: Value, t: TyN) Value {
    return switch (t) {
        .entier => if (v == .entier) v else Value.deZ(versZ(v)),
        .rationnel => if (v == .rationnel) v else Value{ .rationnel = versQ(v) },
        .flottant => if (v == .flottant) v else Value{ .flottant = versD(v) },
        else => v,
    };
}

fn rnkV(v: Value) i32 {
    return switch (v) {
        .booleen, .entier => 1,
        .rationnel => 2,
        .flottant => 3,
        else => 0,
    };
}

fn cmpVals(g: Value, d: Value) i32 {
    if (g == .chaine and d == .chaine) {
        const o = std.mem.order(u8, g.chaine, d.chaine);
        return if (o == .eq) 0 else if (o == .lt) -1 else 1;
    }
    if (g == .flottant or d == .flottant) {
        const a = versD(g); const b = versD(d);
        return if (a < b) -1 else if (a > b) 1 else 0;
    }
    if (g == .rationnel or d == .rationnel) return Q.cmp(versQ(g), versQ(d));
    return Z.cmp(versZ(g), versZ(d));
}

fn valeurEgale(a: Value, b: Value) bool {
    if (a == .tampon or b == .tampon) {
        if (a != .tampon or b != .tampon) return false;
        return std.mem.eql(u8, a.tampon.b, b.tampon.b);
    }
    if (a == .table or b == .table) {
        if (a != .table or b != .table) return false;
        if (a.table.items.len != b.table.items.len) return false;
        for (a.table.items) |kv| {
            var found = false;
            for (b.table.items) |kv2| {
                if (valeurEgale(kv.cle, kv2.cle) and valeurEgale(kv.val, kv2.val)) { found = true; break; }
            }
            if (!found) return false;
        }
        return true;
    }
    if (a == .liste or b == .liste) {
        if (a != .liste or b != .liste) return false;
        if (a.liste.items.len != b.liste.items.len) return false;
        for (a.liste.items, b.liste.items) |x, y| if (!valeurEgale(x, y)) return false;
        return true;
    }
    if (a == .chaine or b == .chaine) {
        if (a != .chaine or b != .chaine) return false;
        return std.mem.eql(u8, a.chaine, b.chaine);
    }
    if (a == .booleen or b == .booleen) {
        if (a != .booleen or b != .booleen) return false;
        return a.booleen == b.booleen;
    }
    return cmpVals(a, b) == 0;
}

// ── stdlib numérique ──────────────────────────────────────────────────

// ── aléatoire : splitmix64, reproductible via graine(n) — séquence
//    strictement identique aux moteurs C++ et Rust. ────────────────────
var g_etat: u64 = 0;
var g_init: bool = false;
fn rngEtat() *u64 {
    if (!g_init) {
        g_etat = @as(u64, @intCast(std.time.nanoTimestamp()))
               ^ @as(u64, @intFromPtr(&g_etat));
        g_init = true;
    }
    return &g_etat;
}
fn rngTirer() u64 {
    const e = rngEtat();
    e.* +%= 0x9E3779B97F4A7C15;
    var z = e.*;
    z = (z ^ (z >> 30)) *% 0xBF58476D1CE4E5B9;
    z = (z ^ (z >> 27)) *% 0x94D049BB133111EB;
    return z ^ (z >> 31);
}
const Timespec = extern struct { tv_sec: i64, tv_nsec: i64 };
extern fn clock_gettime(clock_id: c_int, tp: *Timespec) c_int;
fn factArbreRun(bas: u64, haut: u64) Z {
    if (bas > haut) return Z.i(1);
    if (haut - bas < 16) {
        var dest = Z.u(bas);
        var x = bas + 1;
        while (x <= haut) : (x += 1) dest = Z.mulU(dest, x);
        return dest;
    }
    const mi = bas + (haut - bas) / 2;
    return Z.mul(factArbreRun(bas, mi), factArbreRun(mi + 1, haut));
}

fn roseFactorielle(n: Z) Z {
    if (n.sgn() < 0) erreurRuntime("factorielle d'un négatif : impossible");
    if (n.cmp(Z.u(0xFFFFFFFF)) > 0) erreurRuntime("factorielle trop grande");
    const nn = n.enU64();
    if (nn < 1) return Z.i(1);
    return factArbreRun(1, nn);
}

fn roseFibonacci(n: Z) Z {
    if (n.sgn() < 0) erreurRuntime("fibonacci négatif : impossible");
    var a = Z.i(0); var b = Z.i(1);
    const nb = Z.bits(n);
    var ib: u64 = nb;
    while (ib > 0) : (ib -= 1) {
        const t1 = Z.mul(a, Z.sub(Z.mulU(b, 2), a));
        const t2 = Z.add(Z.mul(a, a), Z.mul(b, b));
        if (Z.bit(n, ib - 1)) {
            const bn = Z.add(t1, t2);
            a = t2; b = bn;
        } else { a = t1; b = t2; }
    }
    return a;
}

fn roseEstPremier(n: Z) bool { return Z.estPremier(n) and n.cmp(Z.u(1)) > 0; }

fn roseNbChiffres(n: Z) Z {
    if (n.sgn() == 0) return Z.i(1);
    return Z.u(@intCast(Z.chiffres10(Z.abs(n))));
}

fn roseCompterPremiers(n: Z) Z {
    if (n.sgn() < 0 or n.cmp(Z.u(0xFFFFFFFF)) > 0) erreurRuntime("crible trop grand");
    const nn = n.enU64();
    const crible = arena().alloc(u8, @intCast(nn + 1)) catch @panic("mémoire");
    @memset(crible, 0);
    var compte: u64 = 0;
    var ii: u64 = 2;
    while (ii <= nn) : (ii += 1) {
        if (crible[ii] == 0) {
            compte += 1;
            if (ii <= nn / ii) {
                var j = ii * ii;
                while (j <= nn) : (j += ii) crible[j] = 1;
            }
        }
    }
    return Z.u(compte);
}

fn rosePi(nn: Z) []const u8 {
    if (nn.sgn() <= 0 or nn.cmp(Z.u(2000000)) > 0) erreurRuntime("décimales de π hors limites (1..2000000)");
    const chiffres = nn.enU64();
    const dix_s = Z.pow10(chiffres + 40);
    const arctan = struct {
        fn f(x: u64, ds: Z, ch: u64) Z {
            _ = ch;
            const xz = Z.u(x);
            const x2 = Z.mul(xz, xz);
            var p = xz;
            var terme = Z.fdivQ(ds, p);
            var dest = terme;
            var k: u64 = 1;
            while (true) {
                p = Z.mul(p, x2);
                k += 2;
                terme = Z.fdivQ(ds, Z.mulU(p, k));
                if (terme.sgn() == 0) break;
                dest = if (k % 4 == 3) Z.sub(dest, terme) else Z.add(dest, terme);
            }
            return dest;
        }
    }.f;
    const pi = Z.sub(Z.mulU(arctan(5, dix_s, chiffres), 16), Z.mulU(arctan(239, dix_s, chiffres), 4));
    const brut = Z.texte(pi);
    var sortie = arena().alloc(u8, @as(usize, @intCast(chiffres)) + 2) catch @panic("mémoire");
    sortie[0] = brut[0];
    sortie[1] = '.';
    const chz: usize = @intCast(chiffres);
    var ii: usize = 0;
    while (ii < chz) : (ii += 1)
        sortie[2 + ii] = if (ii + 1 < brut.len) brut[ii + 1] else '0';
    if (1 + chz < brut.len and brut[1 + chz] >= '5') {
        var i: i64 = @as(i64, @intCast(chz)) + 1;
        while (i >= 0) : (i -= 1) {
            if (sortie[@intCast(i)] == '.') continue;
            sortie[@intCast(i)] += 1;
            if (sortie[@intCast(i)] <= '9') break;
            sortie[@intCast(i)] = '0';
        }
    }
    return sortie;
}

fn powRationnel(base: Q, e: i64) Q {
    var num = Q.numerateur(base);
    var den = Q.denominateur(base);
    if (e < 0) {
        if (Z.sgn(num) == 0) erreurRuntime("0^exposant négatif");
        const t = num; num = den; den = t;
        const k: u64 = @intCast(-@as(i128, e));
        num = Z.powU(num, k);
        den = Z.powU(den, k);
    } else {
        num = Z.powU(num, @intCast(e));
        den = Z.powU(den, @intCast(e));
    }
    return Q.avecNumDen(num, den);
}

// ── interpréteur ──────────────────────────────────────────────────────
const Trame = std.StringHashMap(Value);

const Interp = struct {
    prog: Programme,
    trames: std.ArrayList(Trame),

    fn init(prog: Programme) Interp {
        var trames = std.ArrayList(Trame).init(arena());
        trames.append(Trame.init(arena())) catch @panic("mémoire");
        return .{ .prog = prog, .trames = trames };
    }

    fn cibleVar(self: *Interp, nom: []const u8) ?*Trame {
        const f = &self.trames.items[self.trames.items.len - 1];
        if (f.contains(nom)) return f;
        if (self.trames.items.len > 1) {
            const g = &self.trames.items[0];
            if (g.contains(nom)) return g;
        }
        return null;
    }
    fn lireVar(self: *Interp, nom: []const u8) ?Value {
        const f = &self.trames.items[self.trames.items.len - 1];
        if (f.get(nom)) |v| return v;
        if (self.trames.items.len > 1) return self.trames.items[0].get(nom);
        return null;
    }
    fn affecter(self: *Interp, nom: []const u8, vv: Value) void {
        if (self.cibleVar(nom)) |f| {
            const cur = f.get(nom).?;
            const ajuste = switch (cur) {
                .entier => if (vv == .entier) vv else convertir(vv, .entier),
                .rationnel => if (vv == .rationnel) vv else convertir(vv, .rationnel),
                .flottant => if (vv == .flottant) vv else convertir(vv, .flottant),
                else => vv,
            };
            f.put(nom, ajuste) catch @panic("mémoire");
            return;
        }
        const f = &self.trames.items[self.trames.items.len - 1];
        f.put(nom, vv) catch @panic("mémoire");
    }

    fn afficherV(self: *Interp, v: Value) void {
        _ = self;
        var out = std.io.bufferedWriter(std.io.getStdOut().writer());
        out.writer().writeAll(ecriStr(v)) catch @panic("écriture");
        out.writer().writeAll("\n") catch @panic("écriture");
        out.flush() catch {};
    }

    fn eval(self: *Interp, e: *Expr) Value {
        switch (e.*) {
            .litteral => |l| {
                if (l.flott) {
                    var buf: [512:0]u8 = undefined;
                    if (l.texte.len >= buf.len) return Value{ .flottant = 0 };
                    @memcpy(buf[0..l.texte.len], l.texte);
                    buf[l.texte.len] = 0;
                    var fin: [*c]u8 = undefined;
                    return Value{ .flottant = strtod(&buf, &fin) };
                }
                if (std.mem.indexOfScalar(u8, l.texte, '/') != null) {
                    if (Q.depuisTexte(l.texte)) |q| return Value{ .rationnel = q };
                    erreurRuntime("constante invalide");
                }
                if (Z.depuisTexte(l.texte)) |z| return Value.deZ(z);
                erreurRuntime("constante invalide");
            },
            .chaine_lit => |s| return Value.deS(s),
            .booleen_lit => |b| return Value{ .booleen = b },
            .var_ => |n| {
                if (self.lireVar(n)) |v| return v;
                erreurRuntime(std.fmt.allocPrint(arena(), "variable '{s}' utilisée sans valeur", .{n}) catch @panic("mémoire"));
            },
            .liste_lit => |els| {
                const l = arena().create(std.ArrayList(Value)) catch @panic("mémoire");
                l.* = std.ArrayList(Value).init(arena());
                for (els) |el| l.append(self.eval(el)) catch @panic("mémoire");
                return Value{ .liste = l };
            },
            .unaire => |u| {
                const v = self.eval(u.operande);
                if (std.mem.eql(u8, u.op, "non")) return Value{ .booleen = switch (v) { .booleen => |b| !b, else => false } };
                return switch (v) {
                    .entier => |z| Value.deZ(Z.neg(z)),
                    .rationnel => |q| Value{ .rationnel = Q.neg(q) },
                    .flottant => |d| Value{ .flottant = -d },
                    .booleen => |b| Value.deI(if (b) -1 else 0),
                    else => erreurRuntime("opérateur unaire '-' impossible"),
                };
            },
            .fact => |op| return Value.deZ(roseFactorielle(versZ(self.eval(op)))),
            .index => |ix| {
                const base = self.eval(ix.base);
                const idxv = self.eval(ix.ix);
                if (base == .table) {
                    const cle = idxv;
                    for (base.table.items) |kv| {
                        if (valeurEgale(kv.cle, cle)) return kv.val;
                    }
                    erreurRuntime("clé absente de la table");
                    return Value{ .vide = {} };
                }
                if (base != .liste and base != .chaine) erreurRuntime("index sur une valeur qui n'est pas une liste");
                const idx = versZ(idxv);
                const n: u64 = if (base == .liste) @intCast(base.liste.items.len) else @intCast(utf8Len(base.chaine));
                if (idx.sgn() < 0 or !idx.tientI64() or idx.enU64() >= n)
                    erreurRuntime(if (base == .liste) "index hors des limites de la liste" else "index hors des limites de la chaîne");
                if (base == .liste) return base.liste.items[idx.enU64()];
                return Value.deS(utf8CharStr(base.chaine, idx.enU64()));
            },
            .binaire => |b| {
                if (std.mem.eql(u8, b.op, "et")) {
                    const gv = self.eval(b.g);
                    if (gv != .booleen or !gv.booleen) return Value{ .booleen = false };
                    const dv = self.eval(b.d);
                    return Value{ .booleen = dv == .booleen and dv.booleen };
                }
                if (std.mem.eql(u8, b.op, "ou")) {
                    const gv = self.eval(b.g);
                    if (gv == .booleen and gv.booleen) return Value{ .booleen = true };
                    const dv = self.eval(b.d);
                    return Value{ .booleen = dv == .booleen and dv.booleen };
                }
                return binaireOp(b.op, self.eval(b.g), self.eval(b.d));
            },
            .appel => |a| return self.appelE(a),
        }
    }

    fn binaireOp(op: []const u8, g: Value, d: Value) Value {
        if (std.mem.eql(u8, op, "==") or std.mem.eql(u8, op, "!=")) {
            if (g == .table or d == .table or g == .liste or d == .liste
                or g == .tampon or d == .tampon) {
                const eq = valeurEgale(g, d);
                return Value{ .booleen = if (std.mem.eql(u8, op, "==")) eq else !eq };
            }
            const gc = g == .chaine; const dc = d == .chaine;
            if (gc != dc) return Value{ .booleen = std.mem.eql(u8, op, "!=") };
            const c = cmpVals(g, d);
            return Value{ .booleen = if (std.mem.eql(u8, op, "==")) c == 0 else c != 0 };
        }
        if (std.mem.eql(u8, op, "<") or std.mem.eql(u8, op, "<=") or std.mem.eql(u8, op, ">") or std.mem.eql(u8, op, ">=")) {
            const c = cmpVals(g, d);
            const r = if (std.mem.eql(u8, op, "<")) c < 0 else if (std.mem.eql(u8, op, "<=")) c <= 0
                      else if (std.mem.eql(u8, op, ">")) c > 0 else c >= 0;
            return Value{ .booleen = r };
        }
        if (std.mem.eql(u8, op, "≈") or std.mem.eql(u8, op, "?=") or std.mem.eql(u8, op, "=~")) {
            const r = if (g == .flottant or d == .flottant) blk: {
                const a = versD(g); const b = versD(d);
                var scale = @max(@abs(a), @abs(b));
                if (scale < 1.0) scale = 1.0;
                break :blk @abs(a - b) <= 1e-9 * scale;
            } else cmpVals(g, d) == 0;
            return Value{ .booleen = r };
        }
        if (std.mem.eql(u8, op, "+") and g == .chaine and d == .chaine)
            return Value.deS(std.fmt.allocPrint(arena(), "{s}{s}", .{ g.chaine, d.chaine }) catch @panic("mémoire"));
        const m = @max(rnkV(g), rnkV(d));
        if (std.mem.eql(u8, op, "+")) return if (m == 3) Value{ .flottant = versD(g) + versD(d) }
            else if (m == 2) Value{ .rationnel = Q.add(versQ(g), versQ(d)) } else Value.deZ(Z.add(versZ(g), versZ(d)));
        if (std.mem.eql(u8, op, "-")) return if (m == 3) Value{ .flottant = versD(g) - versD(d) }
            else if (m == 2) Value{ .rationnel = Q.sub(versQ(g), versQ(d)) } else Value.deZ(Z.sub(versZ(g), versZ(d)));
        if (std.mem.eql(u8, op, "*")) return if (m == 3) Value{ .flottant = versD(g) * versD(d) }
            else if (m == 2) Value{ .rationnel = Q.mul(versQ(g), versQ(d)) } else Value.deZ(Z.mul(versZ(g), versZ(d)));
        if (std.mem.eql(u8, op, "/")) {
            if (m == 3) {
                if (versD(d) == 0) erreurRuntime("division par zéro");
                return Value{ .flottant = versD(g) / versD(d) };
            }
            if (m == 2) {
                const dq = versQ(d);
                if (dq.sgn() == 0) erreurRuntime("division par zéro");
                return Value{ .rationnel = Q.div(versQ(g), dq) };
            }
            const dz = versZ(d);
            if (dz.sgn() == 0) erreurRuntime("division par zéro");
            return Value{ .rationnel = Q.avecNumDen(versZ(g), dz) };
        }
        if (std.mem.eql(u8, op, "%")) {
            if (m == 3 or m == 2) erreurRuntime("Le modulo exige deux entiers");
            const dz = versZ(d);
            if (dz.sgn() == 0) erreurRuntime("modulo par zéro");
            return Value.deZ(Z.fdivR(versZ(g), dz));
        }
        if (std.mem.eql(u8, op, "div")) {
            if (m == 3) {
                if (versD(d) == 0) erreurRuntime("division euclidienne par zéro");
                return Value{ .flottant = @floor(versD(g) / versD(d)) };
            }
            if (m == 2) {
                const dq = versQ(d);
                if (dq.sgn() == 0) erreurRuntime("division euclidienne par zéro");
                const q = Q.div(versQ(g), dq);
                return Value.deZ(Z.fdivQ(Q.numerateur(q), Q.denominateur(q)));
            }
            const dz = versZ(d);
            if (dz.sgn() == 0) erreurRuntime("division euclidienne par zéro");
            return Value.deZ(Z.fdivQ(versZ(g), dz));
        }
        if (std.mem.eql(u8, op, "^")) {
            if (d == .flottant) return Value{ .flottant = std.math.pow(f64, versD(g), versD(d)) };
            if (g == .flottant) return Value{ .flottant = std.math.pow(f64, versD(g), versD(d)) };
            const expo = versZ(d);
            if (!expo.tientI64()) erreurRuntime("exposant démesuré (la puissance exploserait au-delà de tout calcul)");
            const e2 = expo.enI64();
            if (g == .rationnel) return Value{ .rationnel = powRationnel(versQ(g), e2) };
            const base = versZ(g);
            if (e2 >= 0) return Value.deZ(Z.powU(base, @intCast(e2)));
            if (base.sgn() == 0) erreurRuntime("0^exposant négatif");
            return Value{ .rationnel = powRationnel(Q.depuisZ(base), e2) };
        }
        erreurRuntime("opérateur inconnu");
    }

    fn appelE(self: *Interp, a: AppelForm) Value {
        if (a.module.len > 0) erreurRuntime("les modules (dynamique::) sont réservés aux moteurs C++/Rust");
        // conversions d'un argument
        if (a.args.len == 1 and (std.mem.eql(u8, a.nom, "entier") or std.mem.eql(u8, a.nom, "rationnel")
            or std.mem.eql(u8, a.nom, "flottant") or std.mem.eql(u8, a.nom, "chaine")))
        {
            const v = self.eval(a.args[0]);
            if (std.mem.eql(u8, a.nom, "chaine")) return Value.deS(chaineDe(v));
            if (std.mem.eql(u8, a.nom, "entier")) return if (v == .chaine) Value.deZ(chaineVersEntier(v.chaine)) else convertir(v, .entier);
            if (std.mem.eql(u8, a.nom, "flottant")) return if (v == .chaine) Value{ .flottant = chaineVersFlottant(v.chaine) } else convertir(v, .flottant);
            return convertir(v, .rationnel);
        }
        var vs = std.ArrayList(Value).init(arena());
        for (a.args) |x| vs.append(self.eval(x)) catch @panic("mémoire");
        if (self.prog.signatures.get(a.nom)) |sig| return self.appeler(sig, vs.items);
        return self.stdlib(a.nom, vs.items);
    }

    fn appeler(self: *Interp, sig: *Signature, args: []Value) Value {
        if (sig.corps == null) erreurRuntime("les fonctions externes (@dll/@sym) sont réservées aux moteurs C++/Rust");
        var trame = Trame.init(arena());
        for (sig.params, 0..) |p, k| {
            const t: TyN = if (std.mem.eql(u8, p[1], "entier")) .entier else if (std.mem.eql(u8, p[1], "rationnel")) .rationnel
                          else if (std.mem.eql(u8, p[1], "flottant")) .flottant else .vide;
            trame.put(p[0], if (t == .vide) args[k] else convertir(args[k], t)) catch @panic("mémoire");
        }
        self.trames.append(trame) catch @panic("mémoire");
        var res = Value{ .vide = {} };
        for (sig.corps.?) |ins| {
            if (self.execRetour(ins, &res)) break;
        }
        _ = self.trames.pop();
        const rt: TyN = if (std.mem.eql(u8, sig.retour, "entier")) .entier else if (std.mem.eql(u8, sig.retour, "rationnel")) .rationnel
                        else if (std.mem.eql(u8, sig.retour, "flottant")) .flottant else .vide;
        return if (rt == .vide) res else convertir(res, rt);
    }

    // true = retour signalé
    fn execRetour(self: *Interp, i: *Instr, res: *Value) bool {
        switch (i.*) {
            .afficher => |e| self.afficherV(self.eval(e)),
            .affect => |a| self.affecter(a.nom, self.eval(a.expr)),
            .lire => |nom| {
                const cur = if (self.cibleVar(nom)) |f| f.get(nom) else null;
                const vv: Value = if (cur != null and cur.? == .flottant) blk: {
                    break :blk Value{ .flottant = lireFlottantClavier() };
                } else Value.deZ(lireEntierClavier());
                self.affecter(nom, vv);
            },
            .si => |s| {
                const t = self.eval(s.cond);
                const branch = if (t == .booleen and t.booleen) s.alors else if (s.sinon) |sn| sn else &[_]*Instr{};
                for (branch) |ins| if (self.execRetour(ins, res)) return true;
            },
            .tant_que => |s| {
                while (true) {
                    const t = self.eval(s.cond);
                    if (!(t == .booleen and t.booleen)) break;
                    for (s.corps) |ins| if (self.execRetour(ins, res)) return true;
                }
            },
            .pour => |s| {
                const fin = versZ(self.eval(s.fin));
                {
                    const f0 = &self.trames.items[self.trames.items.len - 1];
                    f0.put(s.var_, Value.deZ(versZ(self.eval(s.debut)))) catch @panic("mémoire");
                }
                while (true) {
                    const f = &self.trames.items[self.trames.items.len - 1];
                    const cur = f.get(s.var_).?;
                    if (cur != .entier or Z.cmp(cur.entier, fin) >= 0) break;
                    var retro = false;
                    for (s.corps) |ins| if (self.execRetour(ins, res)) { retro = true; break; };
                    if (retro) {
                        const f2 = &self.trames.items[self.trames.items.len - 1];
                        _ = f2.remove(s.var_);
                        return true;
                    }
                    const f2 = &self.trames.items[self.trames.items.len - 1];
                    const nouveau = Z.add(f2.get(s.var_).?.entier, Z.i(1));
                    f2.put(s.var_, Value.deZ(nouveau)) catch @panic("mémoire");
                }
                const ff = &self.trames.items[self.trames.items.len - 1];
                _ = ff.remove(s.var_);
            },
            .retour => |eo| {
                if (eo) |e| res.* = self.eval(e) else res.* = Value{ .vide = {} };
                return true;
            },
            .expr => |e| _ = self.eval(e),
            .bloc => |b| for (b) |ins| if (self.execRetour(ins, res)) return true,
        }
        return false;
    }

    fn executer(self: *Interp) void {
        var res = Value{ .vide = {} };
        for (self.prog.instructions) |ins| {
            if (self.execRetour(ins, &res)) {
                std.io.getStdErr().writer().print("Rose : erreur — 'retour' hors de fonction\n", .{}) catch {};
                std.process.exit(1);
            }
        }

    }

    fn stdlib(self: *Interp, nom: []const u8, aa: []Value) Value {
        const a = aa;
        if (std.mem.eql(u8, nom, "afficher")) { self.afficherV(a[0]); return Value{ .vide = {} }; }
        if (std.mem.eql(u8, nom, "millisecondes")) {
            var ts = Timespec{ .tv_sec = 0, .tv_nsec = 0 };
            const CLOCK_MONOTONIC: c_int = 1;
            _ = clock_gettime(CLOCK_MONOTONIC, &ts);
            return Value.deI(ts.tv_sec * 1000 + @divTrunc(ts.tv_nsec, 1_000_000));
        }
        if (std.mem.eql(u8, nom, "graine")) {
            const s = versZ(a[0]);
            var base: u64 = undefined;
            if (s.sgn() >= 0 and s.bits() <= 64) base = s.enU64() else {
                // mêmes bits de poids faible que les moteurs C++ et Rust
                const hi = Z.fdivR(s, Z.u(0xFFFF_FFFF)).enU64();
                const lo = Z.fdivR(s, Z.u(@intCast(@as(u64, 1) << 32))).enU64();
                base = hi *% (@as(u64, 1) << 32) +% lo;
            }
            rngEtat().* = base;
            _ = rngTirer();   // casse la corrélation directe avec la graine
            return Value{ .vide = {} };
        }
        if (std.mem.eql(u8, nom, "hasard")) {
            const n = versZ(a[0]);
            if (n.sgn() <= 0) erreurRuntime("hasard : bornage positif attendu");
            if (n.bits() <= 64) {
                const nn = n.enU64();
                const r = if ((nn & (nn - 1)) == 0) rngTirer() & (nn - 1)
                          else @as(u64, @truncate(@as(u128, rngTirer()) * @as(u128, nn) >> 64));
                return Value.deZ(Z.u(r));
            }
            return Value.deZ(Z.fdivR(Z.u(rngTirer()), n));
        }
        if (std.mem.eql(u8, nom, "tampon")) {
            const n = versZ(a[0]);
            if (n.sgn() < 0 or n.bits() > 64) erreurRuntime("tampon : taille démesurée");
            const t = arena().create(Tampon) catch @panic("mémoire");
            t.b = arena().alloc(u8, @intCast(n.enU64())) catch @panic("mémoire");
            @memset(t.b, 0);
            return Value{ .tampon = t };
        }
        if (std.mem.eql(u8, nom, "octets")) {
            if (a[0] != .chaine) erreurRuntime("octets : une chaine est attendue");
            const t = arena().create(Tampon) catch @panic("mémoire");
            t.b = arena().dupe(u8, a[0].chaine) catch @panic("mémoire");
            return Value{ .tampon = t };
        }
        if (std.mem.eql(u8, nom, "tampon_case")) {
            if (a[0] != .tampon) erreurRuntime("tampon_case : tampon attendu");
            const idx = versZ(a[1]);
            if (!idx.tientI64()) erreurRuntime("index hors tampon");
            const i = idx.enI64();
            if (i < 0 or @as(u64, @intCast(i)) >= a[0].tampon.b.len) erreurRuntime("index hors tampon");
            return Value.deI(@intCast(a[0].tampon.b[@intCast(i)]));
        }
        if (std.mem.eql(u8, nom, "tampon_pose")) {
            if (a[0] != .tampon) erreurRuntime("tampon_pose : tampon attendu");
            const idx = versZ(a[1]);
            if (!idx.tientI64()) erreurRuntime("index hors tampon");
            const i = idx.enI64();
            if (i < 0 or @as(u64, @intCast(i)) >= a[0].tampon.b.len) erreurRuntime("index hors tampon");
            const v = versZ(a[2]);
            a[0].tampon.b[@intCast(i)] = @intCast(Z.fdivR(v, Z.u(256)).enU64());
            return Value{ .vide = {} };
        }
        if (std.mem.eql(u8, nom, "remplir_hasard")) {
            if (a[0] != .tampon) erreurRuntime("remplir_hasard : tampon attendu");
            if (a[1] != .tampon or a[1].tampon.b.len == 0)
                erreurRuntime("remplir_hasard : alphabet non vide attendu");
            const buf = a[0].tampon.b;
            const ab = a[1].tampon.b;
            const s = ab.len;
            if (s == 1) {
                @memset(buf, ab[0]);
            } else if ((s & (s - 1)) == 0 and s <= 256) {
                // puissance de deux ≤ 256 : octet par caractère, masque exact
                const m: u64 = s - 1;
                var i: usize = 0;
                while (i + 32 <= buf.len) : (i += 32) {
                    const r0 = rngTirer(); const r1 = rngTirer();
                    const r2 = rngTirer(); const r3 = rngTirer();
                    var k: usize = 0;
                    while (k < 8) : (k += 1) {
                        const sh: u6 = @intCast(8 * k);
                        buf[i + k] = @intCast(ab[@intCast((r0 >> sh) & m)]);
                        buf[i + 8 + k] = @intCast(ab[@intCast((r1 >> sh) & m)]);
                        buf[i + 16 + k] = @intCast(ab[@intCast((r2 >> sh) & m)]);
                        buf[i + 24 + k] = @intCast(ab[@intCast((r3 >> sh) & m)]);
                    }
                }
                while (i < buf.len) : (i += 1) buf[i] = @intCast(ab[@intCast(rngTirer() & m)]);
            } else if ((s & (s - 1)) == 0) {
                const m: u64 = s - 1;
                const bits_u: u64 = 64 - @as(u64, @clz(@as(u64, s)));
                const per: usize = @intCast(@divTrunc(64, bits_u));
                var i: usize = 0;
                while (i + per <= buf.len) : (i += per) {
                    const r = rngTirer();
                    var k: usize = 0;
                    while (k < per) : (k += 1) {
                        const sh: u6 = @intCast(bits_u * @as(u64, @intCast(k)));
                        buf[i + k] = @intCast(ab[@intCast((r >> sh) & m)]);
                    }
                }
                while (i < buf.len) : (i += 1) buf[i] = @intCast(ab[@intCast(rngTirer() & m)]);
            } else {
                for (buf) |*x| x.* = @intCast(ab[@intCast((@as(u128, rngTirer()) * @as(u128, s)) >> 64)]);
            }
            return Value{ .vide = {} };
        }
        if (std.mem.eql(u8, nom, "factorielle")) return Value.deZ(roseFactorielle(versZ(a[0])));
        if (std.mem.eql(u8, nom, "fibonacci")) return Value.deZ(roseFibonacci(versZ(a[0])));
        if (std.mem.eql(u8, nom, "pgcd")) return Value.deZ(Z.gcd(versZ(a[0]), versZ(a[1])));
        if (std.mem.eql(u8, nom, "ppcm")) return Value.deZ(Z.lcm(versZ(a[0]), versZ(a[1])));
        if (std.mem.eql(u8, nom, "puissance")) {
            const expo = versZ(a[1]);
            if (!expo.tientI64()) erreurRuntime("exposant démesuré");
            const e = expo.enI64();
            if (a[0] == .flottant) return Value{ .flottant = std.math.pow(f64, a[0].flottant, @floatFromInt(e)) };
            if (a[0] == .rationnel) return Value{ .rationnel = powRationnel(a[0].rationnel, e) };
            if (e < 0) {
                const base = versZ(a[0]);
                if (base.sgn() == 0) erreurRuntime("0^exposant négatif");
                return Value{ .rationnel = powRationnel(Q.depuisZ(base), e) };
            }
            return Value.deZ(Z.powU(versZ(a[0]), @intCast(e)));
        }
        if (std.mem.eql(u8, nom, "est_premier")) return Value{ .booleen = roseEstPremier(versZ(a[0])) };
        if (std.mem.eql(u8, nom, "compter_premiers")) return Value.deZ(roseCompterPremiers(versZ(a[0])));
        if (std.mem.eql(u8, nom, "nb_chiffres")) return Value.deZ(roseNbChiffres(versZ(a[0])));
        if (std.mem.eql(u8, nom, "nombre_pi")) return Value.deS(rosePi(versZ(a[0])));
        if (std.mem.eql(u8, nom, "longueur")) return switch (a[0]) {
            .liste => |l| Value.deI(@intCast(l.items.len)),
            .table => |t| Value.deI(@intCast(t.items.len)),
            .tampon => |t| Value.deI(@intCast(t.b.len)),
            .chaine => |ch| Value.deI(@intCast(utf8Len(ch))),
            else => Value.deI(0),
        };
        if (std.mem.eql(u8, nom, "table")) {
            const t = arena().create(std.ArrayList(TableEntry)) catch @panic("mémoire");
            t.* = std.ArrayList(TableEntry).init(arena());
            return Value.deT(t);
        }
        if (std.mem.eql(u8, nom, "inserer")) {
            if (a[0] != .table) erreurRuntime("inserer attend une table");
            const t = a[0].table;
            for (t.items) |*kv| {
                if (valeurEgale(kv.cle, a[1])) { kv.val = a[2]; return Value{ .vide = {} }; }
            }
            t.append(.{ .cle = a[1], .val = a[2] }) catch @panic("mémoire");
            return Value{ .vide = {} };
        }
        if (std.mem.eql(u8, nom, "cle_existe")) {
            if (a[0] != .table) erreurRuntime("cle_existe attend une table");
            for (a[0].table.items) |kv| {
                if (valeurEgale(kv.cle, a[1])) return Value{ .booleen = true };
            }
            return Value{ .booleen = false };
        }
        if (std.mem.eql(u8, nom, "supprimer")) {
            if (a[0] != .table) erreurRuntime("supprimer attend une table");
            const t = a[0].table;
            var i: usize = 0;
            while (i < t.items.len) : (i += 1) {
                if (valeurEgale(t.items[i].cle, a[1])) {
                    _ = t.orderedRemove(i);
                    return Value{ .booleen = true };
                }
            }
            return Value{ .booleen = false };
        }
        if (std.mem.eql(u8, nom, "cles")) {
            if (a[0] != .table) erreurRuntime("cles attend une table");
            var l = std.ArrayList(Value).init(arena());
            for (a[0].table.items) |kv| l.append(kv.cle) catch @panic("mémoire");
            const lp = arena().create(std.ArrayList(Value)) catch @panic("mémoire");
            lp.* = l;
            return Value{ .liste = lp };
        }
        if (std.mem.eql(u8, nom, "valeurs")) {
            if (a[0] != .table) erreurRuntime("valeurs attend une table");
            var l = std.ArrayList(Value).init(arena());
            for (a[0].table.items) |kv| l.append(kv.val) catch @panic("mémoire");
            const lp = arena().create(std.ArrayList(Value)) catch @panic("mémoire");
            lp.* = l;
            return Value{ .liste = lp };
        }
        if (std.mem.eql(u8, nom, "joindre")) {
            if (a[0] != .liste) erreurRuntime("joindre attend une liste");
            const l = a[0].liste;
            var elem = a[1];
            if (l.items.len > 0) elem = switch (l.items[0]) {
                .entier => convertir(elem, .entier),
                .rationnel => convertir(elem, .rationnel),
                .flottant => convertir(elem, .flottant),
                else => elem,
            };
            l.append(elem) catch @panic("mémoire");
            return Value{ .vide = {} };
        }
        if (std.mem.eql(u8, nom, "ordre")) {
            if (a[0] != .chaine or utf8Len(a[0].chaine) != 1)
                erreurRuntime("ordre attend une chaîne d'un seul caractère");
            return Value.deI(@intCast(utf8CharAt(a[0].chaine, 0)));
        }
        if (std.mem.eql(u8, nom, "caractere")) {
            const c = versZ(a[0]);
            if (c.sgn() < 0 or !c.tientI64() or c.enU64() > 0x10FFFF)
                erreurRuntime("caractere : code hors plage (0 à 1114111)");
            const cp: u21 = @intCast(c.enU64());
            if (cp >= 0xD800 and cp <= 0xDFFF) erreurRuntime("caractere : code réservé (55296 à 57343)");
            return Value.deS(utf8Encode(cp));
        }
        if (std.mem.eql(u8, nom, "sous_chaine")) {
            const s: []const u8 = if (a[0] == .chaine) a[0].chaine else "";
            const d = versZ(a[1]);
            const f = versZ(a[2]);
            if (d.sgn() < 0 or f.sgn() < 0 or !d.tientI64() or !f.tientI64())
                erreurRuntime("sous_chaine : les bornes doivent être des entiers positifs");
            const taille = utf8Len(s);
            const db: usize = @intCast(d.enU64());
            var fb: usize = @intCast(f.enU64());
            if (db > taille or fb < db) erreurRuntime("sous_chaine : bornes invalides");
            if (fb > taille) fb = taille;
            const o1 = utf8Off(s, db);
            const o2 = if (fb == taille) s.len else utf8Off(s, fb);
            return Value.deS(s[o1..o2]);
        }
        if (std.mem.eql(u8, nom, "concatener")) {
            if (a[0] != .liste) erreurRuntime("concatener attend une liste");
            var t = std.ArrayList(u8).init(arena());
            for (a[0].liste.items) |elem| {
                if (elem != .chaine) erreurRuntime("concatener attend une liste de chaînes");
                t.appendSlice(elem.chaine) catch @panic("mémoire");
            }
            return Value.deS(t.items);
        }
        if (std.mem.eql(u8, nom, "lire_texte")) {
            const chemin: []const u8 = if (a[0] == .chaine) a[0].chaine else "";
            const brut = std.fs.cwd().readFileAlloc(arena(), chemin, 1 << 28) catch
                erreurRuntime(std.fmt.allocPrint(arena(), "lecture du fichier impossible ({s})", .{chemin}) catch @panic("mémoire"));
            return Value.deS(brut);
        }
        if (std.mem.eql(u8, nom, "ecrire_texte")) {
            const chemin: []const u8 = if (a[0] == .chaine) a[0].chaine else "";
            const contenu: []const u8 = if (a[1] == .chaine) a[1].chaine else "";
            std.fs.cwd().writeFile(.{ .sub_path = chemin, .data = contenu }) catch
                erreurRuntime(std.fmt.allocPrint(arena(), "écriture du fichier impossible ({s})", .{chemin}) catch @panic("mémoire"));
            return Value{ .vide = {} };
        }
        if (std.mem.eql(u8, nom, "nb_arguments")) return Value.deI(@intCast(progArgs.len));
        if (std.mem.eql(u8, nom, "argument")) {
            const idx = versZ(a[0]);
            if (idx.sgn() < 0 or !idx.tientI64() or idx.enU64() >= progArgs.len)
                erreurRuntime("index hors des limites des arguments");
            return Value.deS(progArgs[idx.enU64()]);
        }
        if (std.mem.eql(u8, nom, "afficher_erreur")) {
            std.io.getStdErr().writer().print("{s}\n", .{ecriStr(a[0])}) catch {};
            return Value{ .vide = {} };
        }
        if (std.mem.eql(u8, nom, "quitter")) {
            const c = versZ(a[0]);
            if (!c.tientI64()) erreurRuntime("code de sortie hors plage");
            const x = c.enI64();
            if (x < -2147483648 or x > 2147483647) erreurRuntime("code de sortie hors plage");
            std.process.exit(@intCast(x));
        }
        if (std.mem.eql(u8, nom, "abs")) return switch (a[0]) {
            .entier => |z| Value.deZ(Z.abs(z)),
            .booleen => |b| Value.deI(if (b) 1 else 0),
            .rationnel => |q| Value{ .rationnel = Q.avecNumDen(Z.abs(Q.numerateur(q)), Q.denominateur(q)) },
            else => Value{ .flottant = @abs(versD(a[0])) },
        };
        if (std.mem.eql(u8, nom, "min") or std.mem.eql(u8, nom, "max")) {
            const m = @max(rnkV(a[0]), rnkV(a[1]));
            const t: TyN = if (m == 3) .flottant else if (m == 2) .rationnel else .entier;
            const x = convertir(a[0], t); const y = convertir(a[1], t);
            const c = cmpVals(x, y);
            const est_min = std.mem.eql(u8, nom, "min");
            return if (est_min) (if (c <= 0) x else y) else (if (c >= 0) x else y);
        }
        if (std.mem.eql(u8, nom, "partie_entiere")) return switch (a[0]) {
            .entier => |z| Value.deZ(z),
            .booleen => |b| Value.deI(if (b) 1 else 0),
            .rationnel => |q| blk: {
                const qc = Q.canon(q);
                break :blk Value.deZ(Z.fdivQ(Q.numerateur(qc), Q.denominateur(qc)));
            },
            else => Value.deZ(Z.deF64(@floor(versD(a[0])))),
        };
        if (std.mem.eql(u8, nom, "fraction")) {
            const den = versZ(a[1]);
            if (den.sgn() == 0) erreurRuntime("fraction de dénominateur nul");
            return Value{ .rationnel = Q.avecNumDen(versZ(a[0]), den) };
        }
        if (std.mem.eql(u8, nom, "saisir_entier")) return Value.deZ(lireEntierClavier());
        if (std.mem.eql(u8, nom, "saisir_flottant")) return Value{ .flottant = lireFlottantClavier() };
        erreurRuntime(std.fmt.allocPrint(arena(), "fonction '{s}' inconnue", .{nom}) catch @panic("mémoire"));
    }
};

// ── lecture clavier ───────────────────────────────────────────────────
fn lireLigne() []u8 {
    const r = std.io.getStdIn().reader();
    const line = r.readUntilDelimiterOrEofAlloc(arena(), '\n', 1 << 16) catch
        erreurRuntime("entrée attendue, fichier fermé");
    if (line == null) erreurRuntime("entrée attendue, fichier fermé");
    var t = line.?;
    while (t.len > 0 and (t[t.len - 1] == '\n' or t[t.len - 1] == '\r' or t[t.len - 1] == ' ')) t = t[0 .. t.len - 1];
    return t;
}
fn lireEntierClavier() Z {
    const t = lireLigne();
    if (std.mem.indexOfScalar(u8, t, '/')) |p| {
        const n = Z.depuisTexte(t[0..p]);
        const d = Z.depuisTexte(t[p + 1 ..]);
        if (n != null and d != null and d.?.sgn() != 0) return Z.tdivQ(n.?, d.?);
        erreurRuntime("nombre entier invalide");
    }
    if (std.mem.indexOfScalar(u8, t, '.') != null) {
        var buf: [512:0]u8 = undefined;
        if (t.len >= buf.len) erreurRuntime("nombre entier invalide");
        @memcpy(buf[0..t.len], t);
        buf[t.len] = 0;
        var fin: [*c]u8 = undefined;
        const d = strtod(&buf, &fin);
        if (fin == @as([*c]u8, @ptrCast(&buf))) erreurRuntime("nombre entier invalide");
        return Z.deF64(d);
    }
    if (Z.depuisTexte(t)) |z| return z;
    erreurRuntime("nombre entier invalide");
}
fn lireFlottantClavier() f64 {
    const t = lireLigne();
    var buf: [512:0]u8 = undefined;
    if (t.len >= buf.len) erreurRuntime("flottant invalide");
    @memcpy(buf[0..t.len], t);
    buf[t.len] = 0;
    var fin: [*c]u8 = undefined;
    const d = strtod(&buf, &fin);
    if (fin == @as([*c]u8, @ptrCast(&buf))) erreurRuntime("flottant invalide");
    return d;
}

// ── point d'entrée ────────────────────────────────────────────────────
const USAGE =
    \\Rose 2.0 — moteur natif Zig (binaire autonome, maths exactes embarquées)
    \\Usage :
    \\  rose prog.rose            interpréter un programme Rose
    \\  rose run prog.rose        interpréter (identique)
    \\  rose info                 état du moteur et des dépendances
    \\  rose version              numéro de version
    \\
;

fn executerSource(source: []const u8) i32 {
    var lx = Lexer.init(source);
    lx.tokeniser() catch |er| switch (er) {
        error.Rose => echec(err_derniere),
        else => echec(.{ .msg = @errorName(er) }),
    };
    var parser = Parser{ .toks = lx.toks.items };
    const prog = parser.programmer() catch |er| switch (er) {
        error.Rose => echec(err_derniere),
        else => echec(.{ .msg = @errorName(er) }),
    };
    for (prog.instructions) |ins| {
        _ = replierInstr(ins) catch |er| switch (er) {
            error.Rose => echec(err_derniere),
            else => echec(.{ .msg = @errorName(er) }),
        };
    }
    var interp = Interp.init(prog);
    interp.executer();
    return 0;
}

pub fn main() !void {
    if (builtin.os.tag == .windows) {
        // Console UTF-8 : messages accentués et notation périodique.
        _ = SetConsoleOutputCP(65001);
        _ = SetConsoleCP(65001);
    }
    arena_instance = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    const args = try std.process.argsAlloc(arena());
    if (args.len < 2) {
        std.io.getStdErr().writeAll(USAGE) catch {};
        std.process.exit(1);
    }
    const action = args[1];
    if (std.mem.eql(u8, action, "version")) {
        std.io.getStdOut().writeAll("Rose 2.0.0 — moteur natif Zig/GMP\n") catch {};
        return;
    }
    if (std.mem.eql(u8, action, "info")) {
        std.io.getStdOut().writeAll(
            \\rose 2.0.0
            \\  moteur            : natif 64 bits Zig — binaire autonome, GMP liée statiquement
            \\  maths exactes     : GMP (entiers et rationnels sans limite)
            \\  interpréteur      : intégré (mode par défaut)
            \\  vérificateur      : réservé aux moteurs C++/Rust (erreurs à la compilation)
            \\
        ) catch {};
        return;
    }
    if (std.mem.eql(u8, action, "aide") or std.mem.eql(u8, action, "--help") or std.mem.eql(u8, action, "-h")) {
        std.io.getStdOut().writeAll(USAGE) catch {};
        return;
    }
    const fichier = if (std.mem.eql(u8, action, "run") and args.len > 2) args[2] else action;
    progArgs = if (std.mem.eql(u8, action, "run")) if (args.len > 3) args[3..] else args[2..2] else args[2..];
    const brut = std.fs.cwd().readFileAlloc(arena(), fichier, 1 << 28) catch {
        std.io.getStdErr().writer().print("rose: fichier introuvable : {s}\n", .{fichier}) catch {};
        std.process.exit(1);
    };
    const source = normaliserFinsDeLigne(brut);
    const lanceur = struct {
        fn go(src: []const u8) void { _ = executerSource(src); }
    }.go;
    const t = std.Thread.spawn(.{ .stack_size = 512 << 20 }, lanceur, .{source}) catch {
        _ = executerSource(source); // repli : exécution inline
        return;
    };
    t.join();
}
