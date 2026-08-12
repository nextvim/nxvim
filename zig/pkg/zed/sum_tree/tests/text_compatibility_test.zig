const std = @import("std");
const sum_tree = @import("sum_tree");

const Version = u32;

const InsertionKey = struct {
    insertion: u32,
    offset: u32,
};

const Fragment = struct {
    insertion_key: InsertionKey,
    operation_id: u32,
    undo_key: u32,
    visible_from: Version,
    hidden_from: ?Version,
    text: []u8,
};

const TextSummary = struct {
    fragments: usize,
    bytes: usize,
    visible_bytes: usize,
};

const TextOps = struct {
    pub const Summary = TextSummary;
    pub const Context = Version;

    pub fn summary(fragment: *const Fragment, version: Version) Summary {
        return .{
            .fragments = 1,
            .bytes = fragment.text.len,
            .visible_bytes = if (isVisible(fragment, version)) fragment.text.len else 0,
        };
    }

    pub fn zero(_: Version) Summary {
        return .{ .fragments = 0, .bytes = 0, .visible_bytes = 0 };
    }

    pub fn addSummary(total: *Summary, value: *const Summary, _: Version) void {
        total.fragments += value.fragments;
        total.bytes += value.bytes;
        total.visible_bytes += value.visible_bytes;
    }

    pub fn cloneItem(fragment: *const Fragment, allocator: std.mem.Allocator) !Fragment {
        var copy = fragment.*;
        copy.text = try allocator.dupe(u8, fragment.text);
        return copy;
    }

    pub fn deinitItem(fragment: *Fragment, allocator: std.mem.Allocator) void {
        allocator.free(fragment.text);
    }

    pub fn cloneSummary(value: *const Summary, _: std.mem.Allocator) !Summary {
        return value.*;
    }

    pub fn deinitSummary(_: *Summary, _: std.mem.Allocator) void {}

    pub fn eqlSummary(a: *const Summary, b: *const Summary) bool {
        return std.meta.eql(a.*, b.*);
    }
};

const VisibleBytes = struct {
    pub const Value = usize;

    pub fn zero(_: Version) Value {
        return 0;
    }

    pub fn addSummary(total: *Value, summary_value: *const TextSummary, _: Version) void {
        total.* += summary_value.visible_bytes;
    }
};

const InsertionKeys = struct {
    pub const Key = InsertionKey;

    pub fn key(fragment: *const Fragment) Key {
        return fragment.insertion_key;
    }

    pub fn compareKeys(a: *const Key, b: *const Key) std.math.Order {
        const insertion_order = std.math.order(a.insertion, b.insertion);
        return if (insertion_order == .eq) std.math.order(a.offset, b.offset) else insertion_order;
    }

    pub fn compareItemKey(fragment: *const Fragment, key_value: *const Key) std.math.Order {
        return compareKeys(&fragment.insertion_key, key_value);
    }
};

fn ScalarKeys(comptime field_name: []const u8) type {
    return struct {
        pub const Key = u32;

        pub fn key(fragment: *const Fragment) Key {
            return @field(fragment, field_name);
        }

        pub fn compareKeys(a: *const Key, b: *const Key) std.math.Order {
            return std.math.order(a.*, b.*);
        }

        pub fn compareItemKey(fragment: *const Fragment, key_value: *const Key) std.math.Order {
            return std.math.order(@field(fragment, field_name), key_value.*);
        }
    };
}

const OperationKeys = ScalarKeys("operation_id");
const UndoKeys = ScalarKeys("undo_key");
const Tree = sum_tree.SumTree(Fragment, TextOps, 2);

fn isVisible(fragment: *const Fragment, version: Version) bool {
    return fragment.visible_from <= version and (fragment.hidden_from == null or version < fragment.hidden_from.?);
}

fn borrowed(
    insertion: u32,
    offset: u32,
    operation_id: u32,
    undo_key: u32,
    visible_from: Version,
    hidden_from: ?Version,
    text: []const u8,
) Fragment {
    return .{
        .insertion_key = .{ .insertion = insertion, .offset = offset },
        .operation_id = operation_id,
        .undo_key = undo_key,
        .visible_from = visible_from,
        .hidden_from = hidden_from,
        .text = @constCast(text),
    };
}

fn deinitOwned(fragment: *Fragment) void {
    TextOps.deinitItem(fragment, std.testing.allocator);
}

fn deinitRemoved(items: *std.ArrayList(Fragment)) void {
    for (items.items) |*item| deinitOwned(item);
    items.deinit(std.testing.allocator);
}

test "Text fragment summaries are versioned and dimensions expose visibility" {
    const fragments = [_]Fragment{
        borrowed(1, 0, 10, 100, 1, 3, "old"),
        borrowed(2, 0, 20, 200, 2, null, "current"),
        borrowed(3, 0, 30, 300, 4, null, "future"),
    };

    var version_two = try Tree.fromSlice(std.testing.allocator, &fragments, 2);
    defer version_two.deinit();
    var version_four = try Tree.fromSlice(std.testing.allocator, &fragments, 4);
    defer version_four.deinit();

    try std.testing.expectEqual(@as(usize, 16), version_two.summary().bytes);
    try std.testing.expectEqual(@as(usize, 10), version_two.summary().visible_bytes);
    try std.testing.expectEqual(@as(usize, 13), version_four.summary().visible_bytes);

    var cursor = version_two.cursor(VisibleBytes, 2);
    cursor.next();
    while (cursor.item() != null) cursor.next();
    try std.testing.expectEqual(@as(usize, 10), cursor.start().*);
    try version_two.validate(2);
    try version_four.validate(4);
}

test "Text fragment splits retain insertion ordering and isolate snapshots" {
    var tree = try Tree.fromSlice(std.testing.allocator, &.{
        borrowed(7, 0, 70, 700, 1, null, "abcdef"),
        borrowed(8, 0, 80, 800, 1, null, "tail"),
    }, 1);
    defer tree.deinit();
    var snapshot = tree.clone();
    defer snapshot.deinit();

    const Edit = sum_tree.Edit(Fragment, InsertionKey);
    const edits = [_]Edit{
        .{ .remove = .{ .insertion = 7, .offset = 0 } },
        .{ .insert = borrowed(7, 0, 70, 700, 1, null, "abc") },
        .{ .insert = borrowed(7, 3, 71, 701, 1, null, "def") },
    };
    var removed = try tree.editKeyed(InsertionKeys, &edits, 1);
    defer deinitRemoved(&removed);

    try std.testing.expectEqual(@as(usize, 3), tree.itemCount());
    try std.testing.expectEqualStrings("abc", tree.itemAt(0).?.text);
    try std.testing.expectEqualStrings("def", tree.itemAt(1).?.text);
    try std.testing.expectEqualStrings("tail", tree.itemAt(2).?.text);
    try std.testing.expectEqualStrings("def", tree.getKeyed(InsertionKeys, &InsertionKey{ .insertion = 7, .offset = 3 }).?.text);

    try std.testing.expectEqual(@as(usize, 2), snapshot.itemCount());
    try std.testing.expectEqualStrings("abcdef", snapshot.itemAt(0).?.text);
    try std.testing.expect(snapshot.getKeyed(InsertionKeys, &InsertionKey{ .insertion = 7, .offset = 3 }) == null);
    try tree.validate(1);
    try snapshot.validate(1);
}

test "Text operation keys deduplicate while undo keys preserve historical lookup" {
    var operations = try Tree.fromSlice(std.testing.allocator, &.{
        borrowed(1, 0, 10, 100, 1, null, "first"),
        borrowed(2, 0, 20, 200, 1, null, "second"),
    }, 1);
    defer operations.deinit();

    var replaced = (try operations.insertOrReplace(
        OperationKeys,
        borrowed(3, 0, 20, 201, 1, null, "second-rebased"),
        1,
    )).?;
    defer deinitOwned(&replaced);

    try std.testing.expectEqual(@as(usize, 2), operations.itemCount());
    try std.testing.expectEqualStrings("second", replaced.text);
    try std.testing.expectEqualStrings("second-rebased", operations.getKeyed(OperationKeys, &@as(u32, 20)).?.text);

    var history = try Tree.fromSlice(std.testing.allocator, &.{
        borrowed(10, 0, 100, 1000, 1, 2, "before"),
        borrowed(11, 0, 101, 1001, 2, null, "after"),
        borrowed(12, 0, 102, 1002, 3, null, "later"),
    }, 3);
    defer history.deinit();

    try std.testing.expectEqualStrings("before", history.getKeyed(UndoKeys, &@as(u32, 1000)).?.text);
    try std.testing.expectEqual(@as(?Version, 2), history.getKeyed(UndoKeys, &@as(u32, 1000)).?.hidden_from);
    try std.testing.expectEqualStrings("after", history.getKeyed(UndoKeys, &@as(u32, 1001)).?.text);
    try std.testing.expect(history.getKeyed(UndoKeys, &@as(u32, 999)) == null);
    try operations.validate(1);
    try history.validate(3);
}
