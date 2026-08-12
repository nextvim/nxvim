const std = @import("std");
const sum_tree = @import("sum_tree");

const TestSummary = struct {
    count: usize,
    sum: usize,
    contains_multiple_of_97: bool,
};

const TreeOps = struct {
    pub const Summary = TestSummary;
    pub const Context = void;

    pub fn summary(value: *const usize, _: void) TestSummary {
        return .{
            .count = 1,
            .sum = value.*,
            .contains_multiple_of_97 = value.* % 97 == 0,
        };
    }

    pub fn zero(_: void) TestSummary {
        return .{ .count = 0, .sum = 0, .contains_multiple_of_97 = false };
    }

    pub fn addSummary(total: *TestSummary, value: *const TestSummary, _: void) void {
        total.count += value.count;
        total.sum += value.sum;
        total.contains_multiple_of_97 = total.contains_multiple_of_97 or value.contains_multiple_of_97;
    }

    pub fn cloneItem(value: *const usize, _: std.mem.Allocator) !usize {
        return value.*;
    }

    pub fn deinitItem(_: *usize, _: std.mem.Allocator) void {}

    pub fn cloneSummary(value: *const TestSummary, _: std.mem.Allocator) !TestSummary {
        return value.*;
    }

    pub fn deinitSummary(_: *TestSummary, _: std.mem.Allocator) void {}

    pub fn eqlSummary(a: *const TestSummary, b: *const TestSummary) bool {
        return a.count == b.count and a.sum == b.sum and a.contains_multiple_of_97 == b.contains_multiple_of_97;
    }
};

var dimension_additions: usize = 0;

const Count = struct {
    pub const Value = usize;
    pub fn zero(_: void) usize {
        return 0;
    }
    pub fn addSummary(total: *usize, value: *const TestSummary, _: void) void {
        dimension_additions += 1;
        total.* += value.count;
    }
};

const Sum = struct {
    pub const Value = usize;
    pub fn zero(_: void) usize {
        return 0;
    }
    pub fn addSummary(total: *usize, value: *const TestSummary, _: void) void {
        total.* += value.sum;
    }
};

const Target = struct {
    value: usize,
    pub fn compare(self: Target, position: *const usize, _: void) std.math.Order {
        return std.math.order(self.value, position.*);
    }
};

const Tree = sum_tree.SumTree(usize, TreeOps, 2);

fn multipleOf97(summary: *const TestSummary) bool {
    return summary.contains_multiple_of_97;
}

test "bounded cursor seeks and traverses a deep tree in both directions" {
    var values: [4097]usize = undefined;
    for (&values, 0..) |*value, index| value.* = index;
    var tree = try Tree.fromSlice(std.testing.allocator, &values, {});
    defer tree.deinit();

    var cursor = tree.cursor(Count, {});
    for ([_]usize{ 0, 1, 3, 63, 1024, 2048, 4096, 4097 }) |target| {
        try std.testing.expect(cursor.seek(Target, .{ .value = target }, .right));
        try std.testing.expectEqual(target, cursor.start().*);
        if (target < values.len) {
            try std.testing.expectEqual(target, cursor.item().?.*);
            try std.testing.expectEqual(@as(usize, 1), cursor.itemSummary().?.count);
        } else try std.testing.expect(cursor.item() == null);
    }

    cursor.prev();
    var expected: usize = values.len - 1;
    while (cursor.item()) |item| {
        try std.testing.expectEqual(expected, item.*);
        if (expected == 0) break;
        expected -= 1;
        cursor.prev();
    }
    cursor.prev();
    try std.testing.expect(cursor.item() == null);
    try std.testing.expectEqual(@as(usize, 0), cursor.start().*);
    cursor.next();
    try std.testing.expectEqual(@as(usize, 0), cursor.item().?.*);
}

test "seek visits logarithmically bounded cached summaries" {
    var values: [32_768]usize = undefined;
    for (&values, 0..) |*value, index| value.* = index;
    var tree = try Tree.fromSlice(std.testing.allocator, &values, {});
    defer tree.deinit();
    var cursor = tree.cursor(Count, {});
    dimension_additions = 0;
    _ = cursor.seek(Target, .{ .value = 31_777 }, .right);
    try std.testing.expect(dimension_additions < 128);
}

test "filter and range summary use cached subtree summaries" {
    var values: [2000]usize = undefined;
    for (&values, 0..) |*value, index| value.* = index + 1;
    var tree = try Tree.fromSlice(std.testing.allocator, &values, {});
    defer tree.deinit();

    const accept: *const fn (*const TestSummary) bool = multipleOf97;
    var filtered = tree.filter(Count, {}, accept);
    filtered.next();
    try std.testing.expectEqual(@as(usize, 97), filtered.item().?.*);
    filtered.next();
    try std.testing.expectEqual(@as(usize, 194), filtered.item().?.*);
    filtered.prev();
    try std.testing.expectEqual(@as(usize, 97), filtered.item().?.*);

    var cursor = tree.cursor(Count, {});
    _ = cursor.seek(Target, .{ .value = 123 }, .right);
    const total = cursor.rangeSummary(Target, .{ .value = 1876 }, .right, Sum);
    const expected = (124 + 1876) * (1876 - 124 + 1) / 2;
    try std.testing.expectEqual(expected, total);
    try std.testing.expectEqual(@as(usize, 1876), cursor.start().*);
    try std.testing.expectEqual(@as(usize, 1877), cursor.item().?.*);
}
