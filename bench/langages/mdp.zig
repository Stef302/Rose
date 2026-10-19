// Le jumeau Zig de remplir_hasard de Rose (splitmix64, aligné octet,
// 4 tirages par lot). Zig pur, sans Rose ni GMP.
const std = @import("std");

var etat: u64 = 2026;
fn tirer() u64 {
    etat +%= 0x9E3779B97F4A7C15;
    var z = etat;
    z = (z ^ (z >> 30)) *% 0xBF58476D1CE4E5B9;
    z = (z ^ (z >> 27)) *% 0x94D049BB133111EB;
    return z ^ (z >> 31);
}

pub fn main() !void {
    const alpha = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_";
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    const al = gpa.allocator();
    const argv = try std.process.argsAlloc(al);
    const n: usize = if (argv.len > 1) try std.fmt.parseInt(usize, argv[1], 10)
                     else 1_000_000_000;
    const t0 = std.time.Instant.now() catch unreachable;
    const buf = try al.alloc(u8, n);
    @memset(buf, 0);
    var i: usize = 0;
    while (i + 32 <= buf.len) : (i += 32) {
        const r0 = tirer(); const r1 = tirer(); const r2 = tirer(); const r3 = tirer();
        var k: usize = 0;
        while (k < 8) : (k += 1) {
            const sh: u6 = @intCast(8 * k);
            buf[i + k] = alpha[@intCast((r0 >> sh) & 63)];
            buf[i + 8 + k] = alpha[@intCast((r1 >> sh) & 63)];
            buf[i + 16 + k] = alpha[@intCast((r2 >> sh) & 63)];
            buf[i + 24 + k] = alpha[@intCast((r3 >> sh) & 63)];
        }
    }
    while (i < buf.len) : (i += 1) buf[i] = alpha[@intCast(tirer() & 63)];
    const t1 = std.time.Instant.now() catch unreachable;
    const ms = @divTrunc(t1.since(t0), 1_000_000);
    const w = std.io.getStdOut().writer();
    try w.print("{d}\n{d} ms\n", .{ buf.len, ms });
}
