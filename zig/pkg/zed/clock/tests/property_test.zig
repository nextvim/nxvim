const std = @import("std");
const clock = @import("clock");

fn fill(global: *clock.Global, seed: u64) !void {
    var random = std.Random.DefaultPrng.init(seed);
    const rng = random.random();
    for (0..64) |_| {
        try global.observe(.{
            .replica_id = .new(rng.intRangeLessThan(u16, 0, 128)),
            .value = rng.intRangeLessThan(u32, 1, 10_000),
        });
    }
}

test "join is associative commutative and idempotent" {
    for (0..32) |seed| {
        var a = clock.Global.init(std.testing.allocator);
        defer a.deinit();
        var b = clock.Global.init(std.testing.allocator);
        defer b.deinit();
        var c = clock.Global.init(std.testing.allocator);
        defer c.deinit();
        try fill(&a, seed * 3);
        try fill(&b, seed * 3 + 1);
        try fill(&c, seed * 3 + 2);

        var ab = try a.clone(std.testing.allocator);
        defer ab.deinit();
        try ab.join(&b);
        var ba = try b.clone(std.testing.allocator);
        defer ba.deinit();
        try ba.join(&a);
        try std.testing.expect(ab.eql(&ba));

        var aa = try a.clone(std.testing.allocator);
        defer aa.deinit();
        try aa.join(&a);
        try std.testing.expect(aa.eql(&a));

        var left = try a.clone(std.testing.allocator);
        defer left.deinit();
        try left.join(&b);
        try left.join(&c);
        var bc = try b.clone(std.testing.allocator);
        defer bc.deinit();
        try bc.join(&c);
        var right = try a.clone(std.testing.allocator);
        defer right.deinit();
        try right.join(&bc);
        try std.testing.expect(left.eql(&right));
    }
}
