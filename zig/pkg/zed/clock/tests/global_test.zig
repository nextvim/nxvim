const std = @import("std");
const clock = @import("clock");

fn timestamp(id: u16, value: u32) clock.Lamport {
    return .{ .replica_id = .new(id), .value = value };
}

test "observe supports sparse and high replica ids" {
    var global = clock.Global.init(std.testing.allocator);
    defer global.deinit();
    try global.observe(timestamp(1024, 7));
    try std.testing.expectEqual(@as(u32, 7), global.get(.new(1024)));
    try std.testing.expectEqual(@as(u32, 0), global.get(.new(1023)));
    try global.observe(timestamp(1024, 3));
    try std.testing.expectEqual(@as(u32, 7), global.get(.new(1024)));
}

test "join meet and observation preserve Rust semantics" {
    var left = clock.Global.init(std.testing.allocator);
    defer left.deinit();
    var right = clock.Global.init(std.testing.allocator);
    defer right.deinit();
    try left.observe(timestamp(0, 5));
    try left.observe(timestamp(2, 7));
    try right.observe(timestamp(0, 3));
    try right.observe(timestamp(1, 9));

    var joined = try left.clone(std.testing.allocator);
    defer joined.deinit();
    try joined.join(&right);
    try std.testing.expectEqual(@as(u32, 5), joined.get(.new(0)));
    try std.testing.expectEqual(@as(u32, 9), joined.get(.new(1)));
    try std.testing.expectEqual(@as(u32, 7), joined.get(.new(2)));

    var met = try left.clone(std.testing.allocator);
    defer met.deinit();
    try met.meet(&right);
    try std.testing.expectEqual(@as(u32, 3), met.get(.new(0)));
    try std.testing.expectEqual(@as(u32, 9), met.get(.new(1)));
    try std.testing.expectEqual(@as(u32, 7), met.get(.new(2)));
    try std.testing.expect(met.observed(timestamp(1, 8)));
    try std.testing.expect(met.observedAny(&right));
    try std.testing.expect(met.observedAll(&right));
    try std.testing.expect(joined.changedSince(&right));
}

test "clone assign iterator and most recent are deep" {
    var source = clock.Global.init(std.testing.allocator);
    defer source.deinit();
    try source.observe(timestamp(3, 11));
    var copy = try source.clone(std.testing.allocator);
    defer copy.deinit();
    try source.observe(timestamp(0, 20));
    try std.testing.expectEqual(@as(u32, 0), copy.get(.new(0)));
    try std.testing.expect(copy.mostRecent().?.eql(timestamp(3, 11)));

    var iterator = copy.iterator();
    var count: usize = 0;
    while (iterator.next()) |entry| {
        try std.testing.expectEqual(@as(u16, @intCast(count)), entry.replica_id.asU16());
        count += 1;
    }
    try std.testing.expectEqual(@as(usize, 4), count);

    try copy.assign(&source);
    try std.testing.expect(copy.eql(&source));
}

test "owning operations are allocation-failure safe" {
    try std.testing.checkAllAllocationFailures(std.testing.allocator, struct {
        fn run(allocator: std.mem.Allocator) !void {
            var left = clock.Global.init(allocator);
            defer left.deinit();
            try left.observe(timestamp(64, 7));
            var right = try left.clone(allocator);
            defer right.deinit();
            try right.observe(timestamp(128, 9));
            try left.join(&right);
        }
    }.run, .{});
}
