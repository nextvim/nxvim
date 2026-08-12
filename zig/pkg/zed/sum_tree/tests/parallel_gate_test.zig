const std = @import("std");
const sum_tree = @import("sum_tree");

const State = struct {
    active: std.atomic.Value(usize) = std.atomic.Value(usize).init(0),
    peak: std.atomic.Value(usize) = std.atomic.Value(usize).init(0),
};

const Ops = struct {
    pub const Summary = usize;
    pub const Context = *State;

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
        const state = test_state.?;
        const active = state.active.fetchAdd(1, .acq_rel) + 1;
        defer _ = state.active.fetchSub(1, .acq_rel);

        var peak = state.peak.load(.acquire);
        while (active > peak) {
            peak = state.peak.cmpxchgWeak(peak, active, .acq_rel, .acquire) orelse break;
        }
        var spin: usize = 0;
        while (spin < 100_000) : (spin += 1) std.atomic.spinLoopHint();
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

    var test_state: ?*State = null;
};

const Tree = sum_tree.SumTree(u32, Ops, 1);

fn expectOrder(tree: *const Tree, expected: []const u32) !void {
    try std.testing.expectEqual(expected.len, tree.itemCount());
    var iterator = tree.iterator();
    for (expected) |value| try std.testing.expectEqual(value, iterator.next().?.*);
    try std.testing.expect(iterator.next() == null);
}

test "parallel construction uses a deterministic bounded gate" {
    var state = State{};
    Ops.test_state = &state;
    defer Ops.test_state = null;

    var values: [256]u32 = undefined;
    for (&values, 0..) |*value, index| value.* = @intCast(index);

    var tree = try Tree.fromParallel(std.testing.allocator, &values, &state);
    defer tree.deinit();
    try tree.validate(&state);
    try expectOrder(&tree, &values);

    const cpu_count = std.Thread.getCpuCount() catch 1;
    const expected_bound = @min(@as(usize, 8), @max(@as(usize, 1), cpu_count));
    try std.testing.expect(state.peak.load(.acquire) <= expected_bound);
}

test "parallel extend preserves prefix and input order" {
    var state = State{};
    Ops.test_state = &state;
    defer Ops.test_state = null;

    var tree = try Tree.fromSlice(std.testing.allocator, &.{ 7, 8, 9 }, &state);
    defer tree.deinit();
    var suffix: [128]u32 = undefined;
    for (&suffix, 0..) |*value, index| value.* = @intCast(index + 10);

    try tree.parallelExtend(&suffix, &state);
    try tree.validate(&state);

    var expected: [131]u32 = undefined;
    expected[0..3].* = .{ 7, 8, 9 };
    @memcpy(expected[3..], &suffix);
    try expectOrder(&tree, &expected);
    try std.testing.expect(state.peak.load(.acquire) <= 8);
}
