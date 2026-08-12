const std = @import("std");
const Locator = @import("text").Locator;

fn make(values: []const u64) !Locator {
    return Locator.init(std.testing.allocator, values);
}

test "min max and strict lexicographic total ordering" {
    var minimum = try Locator.min(std.testing.allocator);
    defer minimum.deinit();
    var maximum = try Locator.max(std.testing.allocator);
    defer maximum.deinit();
    var prefix = try make(&.{ 7, 9 });
    defer prefix.deinit();
    var extension = try make(&.{ 7, 9, 0 });
    defer extension.deinit();
    var greater = try make(&.{8});
    defer greater.deinit();

    try std.testing.expectEqual(std.math.Order.lt, minimum.order(&maximum));
    try std.testing.expectEqual(std.math.Order.lt, prefix.order(&extension));
    try std.testing.expectEqual(std.math.Order.lt, extension.order(&greater));
    try std.testing.expectEqual(std.math.Order.gt, greater.order(&prefix));
    try std.testing.expectEqual(std.math.Order.eq, prefix.order(&prefix));
}

test "clone and assign are deep and assign is transactional" {
    var source = try make(&.{ 3, 5, 8 });
    defer source.deinit();
    var copy = try source.clone(std.testing.allocator);
    defer copy.deinit();
    try std.testing.expect(source.slice().ptr != copy.slice().ptr);
    try std.testing.expectEqualSlices(u64, source.slice(), copy.slice());

    var destination = try make(&.{42});
    defer destination.deinit();
    try destination.assign(&source);
    try std.testing.expect(destination.eql(&source));
    try std.testing.expect(destination.slice().ptr != source.slice().ptr);
    try destination.assign(&destination);
    try std.testing.expect(destination.eql(&source));

    var storage: [0]u8 = .{};
    var failing = std.heap.FixedBufferAllocator.init(&storage);
    var unchanged = try make(&.{ 1, 2, 3 });
    defer unchanged.deinit();
    const original_ptr = unchanged.slice().ptr;
    unchanged.allocator = failing.allocator();
    try std.testing.expectError(error.OutOfMemory, unchanged.assign(&source));
    try std.testing.expectEqual(original_ptr, unchanged.slice().ptr);
    try std.testing.expectEqualSlices(u64, &.{ 1, 2, 3 }, unchanged.slice());
    unchanged.allocator = std.testing.allocator;
}

test "inline locators do not allocate and deeper locators fail cleanly" {
    var storage: [0]u8 = .{};
    var failing = std.heap.FixedBufferAllocator.init(&storage);
    const allocator = failing.allocator();

    var minimum = try Locator.min(allocator);
    defer minimum.deinit();
    var maximum = try Locator.max(allocator);
    defer maximum.deinit();
    var depth_one = try Locator.init(allocator, &.{7});
    defer depth_one.deinit();
    var depth_two = try Locator.init(allocator, &.{ 7, 9 });
    defer depth_two.deinit();
    var initial = try Locator.between(allocator, &minimum, &maximum);
    defer initial.deinit();
    var prefix = try Locator.between(allocator, &minimum, &initial);
    defer prefix.deinit();

    try std.testing.expectEqual(@as(usize, 1), initial.len());
    try std.testing.expectEqual(@as(usize, 2), prefix.len());
    try std.testing.expectError(error.OutOfMemory, Locator.init(allocator, &.{ 1, 2, 3 }));

    var left = try Locator.init(allocator, &.{ 0, 0 });
    defer left.deinit();
    var right = try Locator.init(allocator, &.{ 0, 1 });
    defer right.deinit();
    try std.testing.expectError(error.OutOfMemory, Locator.between(allocator, &left, &right));
}

test "between matches biased Rust examples and rejects invalid bounds" {
    var minimum = try Locator.min(std.testing.allocator);
    defer minimum.deinit();
    var maximum = try Locator.max(std.testing.allocator);
    defer maximum.deinit();
    var initial = try Locator.between(std.testing.allocator, &minimum, &maximum);
    defer initial.deinit();
    try std.testing.expectEqualSlices(u64, &.{65535}, initial.slice());

    var prefix = try Locator.between(std.testing.allocator, &minimum, &initial);
    defer prefix.deinit();
    try std.testing.expectEqualSlices(u64, &.{ 0, 65535 }, prefix.slice());
    try std.testing.expectEqual(std.math.Order.lt, minimum.order(&prefix));
    try std.testing.expectEqual(std.math.Order.lt, prefix.order(&initial));

    try std.testing.expectError(error.InvalidBounds, Locator.between(std.testing.allocator, &initial, &minimum));
    try std.testing.expectError(error.InvalidBounds, Locator.between(std.testing.allocator, &initial, &initial));

    var adjacent_prefix = try make(&.{1});
    defer adjacent_prefix.deinit();
    var adjacent_extension = try make(&.{ 1, 0 });
    defer adjacent_extension.deinit();
    try std.testing.expectError(error.NoSpace, Locator.between(std.testing.allocator, &adjacent_prefix, &adjacent_extension));
}

test "deterministic generated insertions remain strictly between bounds" {
    var prng = std.Random.DefaultPrng.init(0x90d024b);
    const random = prng.random();

    var iteration: usize = 0;
    while (iteration < 2000) : (iteration += 1) {
        var left_values: [5]u64 = undefined;
        var right_values: [5]u64 = undefined;
        const left_len = random.intRangeAtMost(usize, 1, left_values.len);
        const right_len = random.intRangeAtMost(usize, 1, right_values.len);
        for (left_values[0..left_len]) |*value| value.* = random.intRangeAtMost(u64, 1, 100);
        for (right_values[0..right_len]) |*value| value.* = random.intRangeAtMost(u64, 1, 100);

        var left = try make(left_values[0..left_len]);
        defer left.deinit();
        var right = try make(right_values[0..right_len]);
        defer right.deinit();
        if (left.order(&right) == .gt) std.mem.swap(Locator, &left, &right);
        if (left.order(&right) == .eq) continue;

        var middle = try Locator.between(std.testing.allocator, &left, &right);
        defer middle.deinit();
        try std.testing.expectEqual(std.math.Order.lt, left.order(&middle));
        try std.testing.expectEqual(std.math.Order.lt, middle.order(&right));
        const middle_components = middle.slice();
        const left_components = left.slice();
        const right_components = right.slice();
        for (middle_components[0 .. middle_components.len - 1], 0..) |component, index| {
            const left_component = if (index < left_components.len) left_components[index] else 0;
            const right_component = if (index < right_components.len) right_components[index] else 0;
            try std.testing.expect(component == left_component or component == right_component);
        }
    }
}

test "sequential insertion depth follows Rust optimization" {
    var previous = try Locator.min(std.testing.allocator);
    defer previous.deinit();
    var maximum = try Locator.max(std.testing.allocator);
    defer maximum.deinit();

    for (0..100_000) |_| {
        var next = try Locator.between(std.testing.allocator, &previous, &maximum);
        try std.testing.expectEqual(@as(usize, 1), next.len());
        previous.deinit();
        previous = next;
    }
}

test "typing after a split remains at depth two" {
    var minimum = try Locator.min(std.testing.allocator);
    defer minimum.deinit();
    var maximum = try Locator.max(std.testing.allocator);
    defer maximum.deinit();
    var suffix = try Locator.between(std.testing.allocator, &minimum, &maximum);
    defer suffix.deinit();
    var previous = try Locator.between(std.testing.allocator, &minimum, &suffix);
    defer previous.deinit();

    for (0..10_000) |_| {
        var next = try Locator.between(std.testing.allocator, &previous, &suffix);
        try std.testing.expectEqual(@as(usize, 2), next.len());
        previous.deinit();
        previous = next;
    }
}

test "all owning operations are allocation-failure safe" {
    try std.testing.checkAllAllocationFailures(std.testing.allocator, struct {
        fn run(allocator: std.mem.Allocator) !void {
            var left = try Locator.init(allocator, &.{ 0, 0 });
            defer left.deinit();
            var right = try Locator.init(allocator, &.{ 0, 1 });
            defer right.deinit();
            var middle = try Locator.between(allocator, &left, &right);
            defer middle.deinit();
            var copy = try middle.clone(allocator);
            defer copy.deinit();
            var destination = try Locator.init(allocator, &.{9});
            defer destination.deinit();
            try destination.assign(&middle);
        }
    }.run, .{});
}
