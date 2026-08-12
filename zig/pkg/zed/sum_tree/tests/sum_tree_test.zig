const std = @import("std");
const sum_tree = @import("sum_tree");

const CountOps = struct {
    pub const Summary = usize;
    pub const Context = void;

    pub fn summary(_: *const u32, _: Context) Summary {
        return 1;
    }

    pub fn zero(_: Context) Summary {
        return 0;
    }

    pub fn addSummary(total: *Summary, value: *const Summary, _: Context) void {
        total.* += value.*;
    }

    pub fn cloneItem(value: *const u32, _: std.mem.Allocator) !u32 {
        return value.*;
    }

    pub fn deinitItem(_: *u32, _: std.mem.Allocator) void {}

    pub fn cloneSummary(value: *const Summary, _: std.mem.Allocator) !Summary {
        return value.*;
    }

    pub fn deinitSummary(_: *Summary, _: std.mem.Allocator) void {}

    pub fn eqlSummary(a: *const Summary, b: *const Summary) bool {
        return a.* == b.*;
    }
};

const SumOps = struct {
    pub const Summary = u64;
    pub const Context = void;

    pub fn summary(value: *const u32, _: Context) Summary {
        return value.*;
    }

    pub fn zero(_: Context) Summary {
        return 0;
    }

    pub fn addSummary(total: *Summary, value: *const Summary, _: Context) void {
        total.* += value.*;
    }

    pub fn cloneItem(value: *const u32, _: std.mem.Allocator) !u32 {
        return value.*;
    }

    pub fn deinitItem(_: *u32, _: std.mem.Allocator) void {}

    pub fn cloneSummary(value: *const Summary, _: std.mem.Allocator) !Summary {
        return value.*;
    }

    pub fn deinitSummary(_: *Summary, _: std.mem.Allocator) void {}

    pub fn eqlSummary(a: *const Summary, b: *const Summary) bool {
        return a.* == b.*;
    }
};

const CountDimension = struct {
    pub const Value = usize;

    pub fn zero(_: CountOps.Context) Value {
        return 0;
    }

    pub fn addSummary(value: *Value, summary: *const CountOps.Summary, _: CountOps.Context) void {
        value.* += summary.*;
    }
};

const CountTarget = struct {
    value: usize,

    pub fn compare(target: CountTarget, location: *const usize, _: CountOps.Context) std.math.Order {
        return std.math.order(target.value, location.*);
    }
};

test "bounded array operations" {
    var array = sum_tree.BoundedArray(u8, 4).init();
    try array.append(1);
    try array.append(3);
    try array.insert(1, 2);
    try array.appendSlice(&.{4});
    try std.testing.expectEqualSlices(u8, &.{ 1, 2, 3, 4 }, array.constSlice());
    try std.testing.expectError(error.CapacityExceeded, array.append(5));
    array.removeRange(1, 3);
    try std.testing.expectEqualSlices(u8, &.{ 1, 4 }, array.constSlice());
    array.truncate(1);
    try std.testing.expectEqualSlices(u8, &.{1}, array.constSlice());
}

test "shared ownership is copy on write" {
    const Hooks = struct {
        pub fn clone(value: *const u32, _: std.mem.Allocator) !u32 {
            return value.*;
        }
        pub fn deinit(_: *u32, _: std.mem.Allocator) void {}
    };
    const SharedU32 = sum_tree.Shared(u32, Hooks);
    var first = try SharedU32.init(std.testing.allocator, 7);
    defer first.deinit();
    var second = first.clone();
    defer second.deinit();

    try std.testing.expect(!first.isUnique());
    (try second.makeUnique()).* = 9;
    try std.testing.expectEqual(@as(u32, 7), first.get().*);
    try std.testing.expectEqual(@as(u32, 9), second.get().*);
}

test "bias and dimensions" {
    try std.testing.expectEqual(sum_tree.Bias.right, sum_tree.Bias.left.invert());
    const Pair = sum_tree.Dimensions(usize, usize, void);
    const value = Pair{ .first = 2, .second = 3, .third = {} };
    try std.testing.expectEqual(@as(usize, 2), value.first);
}

test "shared ownership deinitializes heap values" {
    const Hooks = struct {
        pub fn clone(value: *const []u8, allocator: std.mem.Allocator) ![]u8 {
            return allocator.dupe(u8, value.*);
        }
        pub fn deinit(value: *[]u8, allocator: std.mem.Allocator) void {
            allocator.free(value.*);
        }
    };
    const SharedBytes = sum_tree.Shared([]u8, Hooks);
    var original = try SharedBytes.init(std.testing.allocator, try std.testing.allocator.dupe(u8, "zed"));
    defer original.deinit();
    var copy = original.clone();
    defer copy.deinit();
    const unique = try copy.makeUnique();
    unique.*[0] = 'Z';
    try std.testing.expectEqualStrings("zed", original.get().*);
    try std.testing.expectEqualStrings("Zed", copy.get().*);
}

test "immutable construction and iteration" {
    const Tree = sum_tree.SumTree(u32, CountOps, 2);
    var empty = try Tree.init(std.testing.allocator, {});
    defer empty.deinit();
    try std.testing.expect(empty.isEmpty());
    try std.testing.expectEqual(@as(usize, 0), empty.summary().*);
    try empty.validate({});

    var one = try Tree.fromItem(std.testing.allocator, 42, {});
    defer one.deinit();
    try std.testing.expectEqual(@as(u32, 42), one.first().?.*);
    try std.testing.expectEqual(@as(u32, 42), one.last().?.*);
    try one.validate({});

    const values = [_]u32{ 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12 };
    var tree = try Tree.fromSlice(std.testing.allocator, &values, {});
    defer tree.deinit();
    try tree.validate({});
    try std.testing.expect(!tree.isEmpty());
    try std.testing.expectEqual(values.len, tree.summary().*);
    try std.testing.expectEqual(values.len, tree.extent(CountDimension, {}));
    try std.testing.expectEqual(@as(u32, 0), tree.first().?.*);
    try std.testing.expectEqual(@as(u32, 12), tree.last().?.*);

    var iterator = tree.iterator();
    var index: usize = 0;
    while (iterator.next()) |item| : (index += 1) {
        try std.testing.expectEqual(values[index], item.*);
    }
    try std.testing.expectEqual(values.len, index);

    var clone = tree.clone();
    defer clone.deinit();
    try std.testing.expectEqual(tree.summary().*, clone.summary().*);
    try clone.validate({});
}

test "push extend append and snapshots" {
    const Tree = sum_tree.SumTree(u32, CountOps, 2);
    var tree = try Tree.init(std.testing.allocator, {});
    defer tree.deinit();

    for (0..20) |value| try tree.push(@intCast(value), {});
    var snapshot = tree.clone();
    defer snapshot.deinit();

    const suffix = [_]u32{ 50, 51, 52, 53, 54 };
    try tree.extendSlice(&suffix, {});
    var other = try Tree.fromSlice(std.testing.allocator, &.{ 100, 101, 102 }, {});
    defer other.deinit();
    try tree.append(&other, {});

    try tree.validate({});
    try snapshot.validate({});
    try std.testing.expectEqual(@as(usize, 28), tree.itemCount());
    try std.testing.expectEqual(@as(usize, 20), snapshot.itemCount());
    try std.testing.expectEqual(@as(u32, 19), snapshot.last().?.*);
    try std.testing.expectEqual(@as(u32, 102), tree.last().?.*);
    try std.testing.expectEqual(@as(usize, 3), other.itemCount());
}

test "append handles all relative tree heights" {
    const Tree = sum_tree.SumTree(u32, CountOps, 2);
    var values: [300]u32 = undefined;
    for (&values, 0..) |*value, index| value.* = @intCast(index);
    const cases = [_][2]usize{
        .{ 1, 1 }, .{ 3, 20 }, .{ 20, 3 }, .{ 17, 65 }, .{ 65, 17 }, .{ 64, 129 }, .{ 129, 64 },
    };

    for (cases) |case| {
        var left = try Tree.fromSlice(std.testing.allocator, values[0..case[0]], {});
        defer left.deinit();
        var right = try Tree.fromSlice(std.testing.allocator, values[case[0] .. case[0] + case[1]], {});
        defer right.deinit();
        try left.append(&right, {});
        try left.validate({});
        try std.testing.expectEqual(case[0] + case[1], left.itemCount());
        var iterator = left.iterator();
        var index: usize = 0;
        while (iterator.next()) |item| : (index += 1) try std.testing.expectEqual(values[index], item.*);
        try std.testing.expectEqual(case[0] + case[1], index);
    }
}

test "update first and last recompute summaries without changing snapshots" {
    const Tree = sum_tree.SumTree(u32, SumOps, 2);
    var tree = try Tree.fromSlice(std.testing.allocator, &.{ 1, 2, 3, 4, 5, 6, 7, 8, 9 }, {});
    defer tree.deinit();
    var snapshot = tree.clone();
    defer snapshot.deinit();

    try tree.updateFirst({}, struct {
        fn update(value: *u32) void {
            value.* = 10;
        }
    }.update);
    try tree.updateLast({}, struct {
        fn update(value: *u32) void {
            value.* = 90;
        }
    }.update);

    try tree.validate({});
    try snapshot.validate({});
    try std.testing.expectEqual(@as(u32, 10), tree.first().?.*);
    try std.testing.expectEqual(@as(u32, 90), tree.last().?.*);
    try std.testing.expectEqual(@as(u64, 135), tree.summary().*);
    try std.testing.expectEqual(@as(u64, 45), snapshot.summary().*);
    try std.testing.expectEqual(@as(u32, 1), snapshot.first().?.*);
    try std.testing.expectEqual(@as(u32, 9), snapshot.last().?.*);
}

test "deterministic randomized mutation model" {
    const Tree = sum_tree.SumTree(u32, CountOps, 2);
    const seed: u64 = 0x5eed_0003;
    var random = std.Random.DefaultPrng.init(seed);
    const rng = random.random();
    var tree = try Tree.init(std.testing.allocator, {});
    defer tree.deinit();
    var model: std.ArrayList(u32) = .empty;
    defer model.deinit(std.testing.allocator);

    for (0..250) |_| {
        if (rng.boolean()) {
            const value = rng.int(u32);
            try tree.push(value, {});
            try model.append(std.testing.allocator, value);
        } else {
            var chunk: [17]u32 = undefined;
            const len = rng.uintLessThan(usize, chunk.len + 1);
            for (chunk[0..len]) |*value| value.* = rng.int(u32);
            try tree.extendSlice(chunk[0..len], {});
            try model.appendSlice(std.testing.allocator, chunk[0..len]);
        }

        try tree.validate({});
        try std.testing.expectEqual(model.items.len, tree.itemCount());
        var iterator = tree.iterator();
        var index: usize = 0;
        while (iterator.next()) |item| : (index += 1) try std.testing.expectEqual(model.items[index], item.*);
        try std.testing.expectEqual(model.items.len, index);
    }
}

test "direct find operations match count boundaries and bias" {
    const Tree = sum_tree.SumTree(u32, CountOps, 2);
    const values = [_]u32{ 10, 20, 30, 40, 50 };
    var tree = try Tree.fromSlice(std.testing.allocator, &values, {});
    defer tree.deinit();

    for (0..values.len + 1) |target| {
        const left = tree.find(CountDimension, CountTarget, {}, .{ .value = target }, .left);
        if (target == 0) {
            try std.testing.expectEqual(@as(usize, 0), left.start);
            try std.testing.expectEqual(values[0], left.item.?.*);
        } else if (target <= values.len) {
            try std.testing.expectEqual(target - 1, left.start);
            try std.testing.expectEqual(target, left.end);
            try std.testing.expectEqual(values[target - 1], left.item.?.*);
        } else try std.testing.expect(left.item == null);

        const right = tree.find(CountDimension, CountTarget, {}, .{ .value = target }, .right);
        if (target < values.len) {
            try std.testing.expectEqual(target, right.start);
            try std.testing.expectEqual(target + 1, right.end);
            try std.testing.expectEqual(values[target], right.item.?.*);
        } else try std.testing.expect(right.item == null);
    }

    const exact = tree.findExact(CountDimension, CountTarget, {}, .{ .value = 3 }, .left);
    try std.testing.expectEqual(@as(usize, 2), exact.start);
    try std.testing.expectEqual(@as(u32, 30), exact.item.?.*);
    const beyond = tree.find(CountDimension, CountTarget, {}, .{ .value = 99 }, .right);
    try std.testing.expectEqual(values.len, beyond.start);
    try std.testing.expect(beyond.item == null);
    const with_prev = tree.findWithPrev(CountDimension, CountTarget, {}, .{ .value = 3 }, .left);
    try std.testing.expectEqual(@as(u32, 20), with_prev.previous.?.*);
    try std.testing.expectEqual(@as(u32, 30), with_prev.item.?.*);
}

test "cursor seeks and traverses both directions" {
    const Tree = sum_tree.SumTree(u32, CountOps, 2);
    const values = [_]u32{ 10, 20, 30, 40, 50, 60, 70 };
    var tree = try Tree.fromSlice(std.testing.allocator, &values, {});
    defer tree.deinit();

    var cursor = tree.cursor(CountDimension, {});
    try std.testing.expect(!cursor.didSeek());
    cursor.next();
    try std.testing.expectEqual(@as(usize, 0), cursor.start().*);
    try std.testing.expectEqual(@as(u32, 10), cursor.item().?.*);
    try std.testing.expectEqual(@as(u32, 20), cursor.nextItem().?.*);
    try std.testing.expect(cursor.prevItem() == null);

    cursor.next();
    try std.testing.expectEqual(@as(usize, 1), cursor.start().*);
    try std.testing.expectEqual(@as(u32, 20), cursor.item().?.*);
    cursor.prev();
    try std.testing.expectEqual(@as(u32, 10), cursor.item().?.*);
    cursor.prev();
    try std.testing.expect(cursor.item() == null);
    try std.testing.expectEqual(@as(usize, 0), cursor.start().*);
    try std.testing.expectEqual(@as(u32, 10), cursor.nextItem().?.*);

    try std.testing.expect(cursor.seek(CountTarget, .{ .value = 4 }, .left));
    try std.testing.expectEqual(@as(usize, 3), cursor.start().*);
    try std.testing.expectEqual(@as(u32, 40), cursor.item().?.*);
    try std.testing.expect(cursor.seek(CountTarget, .{ .value = 4 }, .right));
    try std.testing.expectEqual(@as(usize, 4), cursor.start().*);
    try std.testing.expectEqual(@as(u32, 50), cursor.item().?.*);
    try std.testing.expect(cursor.seekForward(CountTarget, .{ .value = 6 }, .right));
    try std.testing.expectEqual(@as(u32, 70), cursor.item().?.*);
    _ = cursor.seek(CountTarget, .{ .value = values.len }, .right);
    try std.testing.expect(cursor.item() == null);
    cursor.prev();
    try std.testing.expectEqual(@as(u32, 70), cursor.item().?.*);
}

test "filter cursor skips rejected item summaries" {
    const FilterOps = struct {
        pub const Summary = struct { count: usize, contains_even: bool };
        pub const Context = void;
        pub fn summary(value: *const u32, _: Context) Summary {
            return .{ .count = 1, .contains_even = value.* % 2 == 0 };
        }
        pub fn zero(_: Context) Summary {
            return .{ .count = 0, .contains_even = false };
        }
        pub fn addSummary(total: *Summary, value: *const Summary, _: Context) void {
            total.count += value.count;
            total.contains_even = total.contains_even or value.contains_even;
        }
        pub fn cloneItem(value: *const u32, _: std.mem.Allocator) !u32 {
            return value.*;
        }
        pub fn deinitItem(_: *u32, _: std.mem.Allocator) void {}
        pub fn cloneSummary(value: *const Summary, _: std.mem.Allocator) !Summary {
            return value.*;
        }
        pub fn deinitSummary(_: *Summary, _: std.mem.Allocator) void {}
        pub fn eqlSummary(a: *const Summary, b: *const Summary) bool {
            return a.count == b.count and a.contains_even == b.contains_even;
        }
    };
    const Dimension = struct {
        pub const Value = usize;
        pub fn zero(_: FilterOps.Context) Value {
            return 0;
        }
        pub fn addSummary(value: *Value, summary: *const FilterOps.Summary, _: FilterOps.Context) void {
            value.* += summary.count;
        }
    };
    const Tree = sum_tree.SumTree(u32, FilterOps, 2);
    var tree = try Tree.fromSlice(std.testing.allocator, &.{ 1, 2, 3, 4, 5, 6 }, {});
    defer tree.deinit();
    const accept: *const fn (*const FilterOps.Summary) bool = struct {
        fn call(summary: *const FilterOps.Summary) bool {
            return summary.contains_even;
        }
    }.call;
    var cursor = tree.filter(Dimension, {}, accept);

    cursor.next();
    try std.testing.expectEqual(@as(u32, 2), cursor.item().?.*);
    try std.testing.expectEqual(@as(usize, 1), cursor.start().*);
    cursor.next();
    try std.testing.expectEqual(@as(u32, 4), cursor.item().?.*);
    cursor.next();
    try std.testing.expectEqual(@as(u32, 6), cursor.item().?.*);
    cursor.prev();
    try std.testing.expectEqual(@as(u32, 4), cursor.item().?.*);
}

test "cursor slice suffix and range summary" {
    const Tree = sum_tree.SumTree(u32, CountOps, 2);
    const values = [_]u32{ 0, 1, 2, 3, 4, 5, 6, 7, 8, 9 };
    var tree = try Tree.fromSlice(std.testing.allocator, &values, {});
    defer tree.deinit();

    var cursor = tree.cursor(CountDimension, {});
    _ = cursor.seek(CountTarget, .{ .value = 2 }, .right);
    var slice = try cursor.slice(CountTarget, .{ .value = 7 }, .right);
    defer slice.deinit();
    try std.testing.expectEqual(@as(usize, 5), slice.itemCount());
    try std.testing.expectEqual(@as(u32, 2), slice.first().?.*);
    try std.testing.expectEqual(@as(u32, 6), slice.last().?.*);

    _ = cursor.seek(CountTarget, .{ .value = 7 }, .right);
    var suffix = try cursor.suffix();
    defer suffix.deinit();
    try std.testing.expectEqual(@as(usize, 3), suffix.itemCount());
    try std.testing.expectEqual(@as(u32, 7), suffix.first().?.*);

    _ = cursor.seek(CountTarget, .{ .value = 1 }, .right);
    const range_count = cursor.rangeSummary(CountTarget, .{ .value = 8 }, .right, CountDimension);
    try std.testing.expectEqual(@as(usize, 7), range_count);
}

test "parallel construction preserves serial order and summaries" {
    const Tree = sum_tree.SumTree(u32, CountOps, 2);
    var values: [513]u32 = undefined;
    for (&values, 0..) |*value, index| value.* = @intCast(index * 3);
    const sizes = [_]usize{ 0, 1, 4, 5, 7, 8, 9, 17, 63, 64, 65, 255, 256, 257, 513 };
    for (sizes) |size| {
        var serial = try Tree.fromSlice(std.testing.allocator, values[0..size], {});
        defer serial.deinit();
        var parallel = try Tree.fromParallel(std.testing.allocator, values[0..size], {});
        defer parallel.deinit();
        try serial.validate({});
        try parallel.validate({});
        try std.testing.expectEqual(serial.summary().*, parallel.summary().*);
        var serial_iterator = serial.iterator();
        var parallel_iterator = parallel.iterator();
        while (serial_iterator.next()) |serial_item| try std.testing.expectEqual(serial_item.*, parallel_iterator.next().?.*);
        try std.testing.expect(parallel_iterator.next() == null);
    }
}

test "parallel extend is deterministic and snapshot safe" {
    const Tree = sum_tree.SumTree(u32, CountOps, 2);
    var tree = try Tree.fromSlice(std.testing.allocator, &.{ 1, 2, 3 }, {});
    defer tree.deinit();
    var snapshot = tree.clone();
    defer snapshot.deinit();
    var suffix: [100]u32 = undefined;
    for (&suffix, 0..) |*value, index| value.* = @intCast(index + 4);
    try tree.parallelExtend(&suffix, {});
    try tree.validate({});
    try std.testing.expectEqual(@as(usize, 103), tree.itemCount());
    try std.testing.expectEqual(@as(usize, 3), snapshot.itemCount());
    try std.testing.expectEqual(@as(u32, 103), tree.last().?.*);
}

test "optional trace hooks bracket instrumented operations" {
    const TraceOps = struct {
        pub const Summary = usize;
        pub const Context = *struct { begins: usize = 0, ends: usize = 0 };
        pub fn summary(_: *const u32, _: Context) Summary {
            return 1;
        }
        pub fn zero(_: Context) Summary {
            return 0;
        }
        pub fn addSummary(total: *Summary, value: *const Summary, _: Context) void {
            total.* += value.*;
        }
        pub fn cloneItem(value: *const u32, _: std.mem.Allocator) !u32 {
            return value.*;
        }
        pub fn deinitItem(_: *u32, _: std.mem.Allocator) void {}
        pub fn cloneSummary(value: *const Summary, _: std.mem.Allocator) !Summary {
            return value.*;
        }
        pub fn deinitSummary(_: *Summary, _: std.mem.Allocator) void {}
        pub fn eqlSummary(a: *const Summary, b: *const Summary) bool {
            return a.* == b.*;
        }
        var trace_context: ?Context = null;
        pub fn traceBegin(_: anytype, _: usize) void {
            trace_context.?.begins += 1;
        }
        pub fn traceEnd(_: anytype, _: usize) void {
            trace_context.?.ends += 1;
        }
    };
    const Tree = sum_tree.SumTree(u32, TraceOps, 2);
    var state: @typeInfo(TraceOps.Context).pointer.child = .{};
    TraceOps.trace_context = &state;
    defer TraceOps.trace_context = null;
    var tree = try Tree.fromParallel(std.testing.allocator, &.{ 1, 2, 3, 4, 5, 6, 7, 8 }, &state);
    defer tree.deinit();
    try tree.push(9, &state);
    try std.testing.expectEqual(state.begins, state.ends);
    try std.testing.expect(state.begins >= 3);
}

test "bulk construction balances boundary and multi-level sizes" {
    const Tree = sum_tree.SumTree(u32, CountOps, 2);
    var values: [257]u32 = undefined;
    for (&values, 0..) |*value, index| value.* = @intCast(index);

    const sizes = [_]usize{ 0, 1, 2, 3, 4, 5, 7, 8, 9, 13, 16, 17, 63, 64, 65, 256, 257 };
    for (sizes) |size| {
        var tree = try Tree.fromSlice(std.testing.allocator, values[0..size], {});
        defer tree.deinit();
        try tree.validate({});
        try std.testing.expectEqual(size, tree.summary().*);

        var iterator = tree.iterator();
        var index: usize = 0;
        while (iterator.next()) |item| : (index += 1) {
            try std.testing.expectEqual(values[index], item.*);
        }
        try std.testing.expectEqual(size, index);
    }
}
