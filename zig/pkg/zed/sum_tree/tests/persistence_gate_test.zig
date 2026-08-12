const std = @import("std");
const sum_tree = @import("sum_tree");

const Counters = struct {
    item_clones: usize = 0,
    item_deinits: usize = 0,
    summary_clones: usize = 0,
    summary_deinits: usize = 0,
};

const Item = struct { bytes: []u8, counters: *Counters };
const HeapSummary = struct { count: usize, token: []u8, counters: *Counters };

const HeapOps = struct {
    pub const Summary = HeapSummary;
    pub const Context = *Counters;

    pub fn summary(_: *const Item, counters: Context) Summary {
        return .{ .count = 1, .token = std.testing.allocator.dupe(u8, "s") catch @panic("oom"), .counters = counters };
    }

    pub fn zero(counters: Context) Summary {
        return .{ .count = 0, .token = std.testing.allocator.dupe(u8, "z") catch @panic("oom"), .counters = counters };
    }

    pub fn addSummary(total: *Summary, value: *const Summary, _: Context) void {
        total.count += value.count;
    }

    pub fn cloneItem(value: *const Item, allocator: std.mem.Allocator) !Item {
        value.counters.item_clones += 1;
        return .{ .bytes = try allocator.dupe(u8, value.bytes), .counters = value.counters };
    }

    pub fn deinitItem(value: *Item, allocator: std.mem.Allocator) void {
        value.counters.item_deinits += 1;
        allocator.free(value.bytes);
    }

    pub fn cloneSummary(value: *const Summary, allocator: std.mem.Allocator) !Summary {
        value.counters.summary_clones += 1;
        return .{ .count = value.count, .token = try allocator.dupe(u8, value.token), .counters = value.counters };
    }

    pub fn deinitSummary(value: *Summary, allocator: std.mem.Allocator) void {
        value.counters.summary_deinits += 1;
        allocator.free(value.token);
    }

    pub fn eqlSummary(a: *const Summary, b: *const Summary) bool {
        return a.count == b.count;
    }
};

fn borrowed(value: []const u8, counters: *Counters) Item {
    return .{ .bytes = @constCast(value), .counters = counters };
}

test "persistent rope mutations clone only boundary items and isolate snapshots" {
    const Tree = sum_tree.SumTree(Item, HeapOps, 2);
    var counters = Counters{};
    var values: [128]Item = undefined;
    for (&values, 0..) |*value, index| value.* = borrowed(if (index % 2 == 0) "even" else "odd", &counters);

    var tree = try Tree.fromSlice(std.testing.allocator, &values, &counters);
    defer tree.deinit();
    var snapshot = tree.clone();
    defer snapshot.deinit();

    counters.item_clones = 0;
    try tree.push(borrowed("tail", &counters), &counters);
    try std.testing.expect(counters.item_clones <= 8);
    try std.testing.expectEqual(@as(usize, 128), snapshot.itemCount());
    try std.testing.expectEqualStrings("odd", snapshot.last().?.bytes);

    counters.item_clones = 0;
    var suffix = try Tree.fromSlice(std.testing.allocator, &.{ borrowed("a", &counters), borrowed("b", &counters), borrowed("c", &counters), borrowed("d", &counters) }, &counters);
    defer suffix.deinit();
    counters.item_clones = 0;
    try tree.append(&suffix, &counters);
    try std.testing.expect(counters.item_clones <= 8);

    counters.item_clones = 0;
    try tree.updateLast(&counters, struct {
        fn update(value: *Item) void {
            value.bytes[0] = 'D';
        }
    }.update);
    try std.testing.expect(counters.item_clones <= 4);
    try std.testing.expectEqualStrings("d", suffix.last().?.bytes);

    counters.item_clones = 0;
    var middle = try tree.copyRange(8, 120, &counters);
    defer middle.deinit();
    try middle.validate(&counters);
    try std.testing.expectEqual(@as(usize, 112), middle.itemCount());
    try std.testing.expect(counters.item_clones <= 8);

    try tree.validate(&counters);
    try snapshot.validate(&counters);
}
