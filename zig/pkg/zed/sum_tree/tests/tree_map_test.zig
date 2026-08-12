const std = @import("std");
const sum_tree = @import("sum_tree");

const U32Ops = struct {
    pub fn compareKeys(a: *const u32, b: *const u32) std.math.Order {
        return std.math.order(a.*, b.*);
    }
    pub fn cloneKey(value: *const u32, _: std.mem.Allocator) !u32 {
        return value.*;
    }
    pub fn deinitKey(_: *u32, _: std.mem.Allocator) void {}
    pub fn cloneValue(value: *const u32, _: std.mem.Allocator) !u32 {
        return value.*;
    }
    pub fn deinitValue(_: *u32, _: std.mem.Allocator) void {}
};
const Map = sum_tree.TreeMap(u32, u32, U32Ops, 2);
const SetOps = struct {
    pub fn compareKeys(a: *const u32, b: *const u32) std.math.Order {
        return std.math.order(a.*, b.*);
    }
    pub fn cloneKey(value: *const u32, _: std.mem.Allocator) !u32 {
        return value.*;
    }
    pub fn deinitKey(_: *u32, _: std.mem.Allocator) void {}
};
const Set = sum_tree.TreeSet(u32, SetOps, 2);

const KeyTarget = struct {
    key: u32,
    pub fn compare(self: KeyTarget, cursor: *const u32) std.math.Order {
        return std.math.order(self.key, cursor.*);
    }
};
const InclusiveEndTarget = struct {
    key: u32,
    pub fn compare(self: InclusiveEndTarget, cursor: *const u32) std.math.Order {
        return if (cursor.* <= self.key) .gt else .lt;
    }
};

test "tree map basic operations" {
    var map = try Map.init(std.testing.allocator);
    defer map.deinit();
    try std.testing.expect(map.isEmpty());
    try map.insert(3, 30);
    try map.insert(1, 10);
    try map.insert(2, 20);
    try std.testing.expectEqual(@as(usize, 3), map.count());
    try std.testing.expectEqual(@as(u32, 20), map.get(&@as(u32, 2)).?.*);
    try std.testing.expect(map.containsKey(&@as(u32, 1)));

    const replaced = try map.insertOrReplace(2, 22);
    try std.testing.expectEqual(@as(u32, 20), replaced.?);
    try std.testing.expectEqual(@as(u32, 22), map.get(&@as(u32, 2)).?.*);
    const closest = map.closest(&@as(u32, 9)).?;
    try std.testing.expectEqual(@as(u32, 3), closest.key);
    try std.testing.expectEqual(@as(u32, 30), closest.value);
    try std.testing.expect(map.closest(&@as(u32, 0)) == null);

    const removed = try map.remove(2);
    try std.testing.expectEqual(@as(u32, 22), removed.?);
    try std.testing.expect(map.get(&@as(u32, 2)) == null);
    try map.clear();
    try std.testing.expect(map.isEmpty());
}

test "map iteration update retain extend and insert map" {
    var map = try Map.init(std.testing.allocator);
    defer map.deinit();
    try map.extend(&.{
        .{ .key = 1, .value = 10 }, .{ .key = 2, .value = 20 }, .{ .key = 4, .value = 40 },
    });
    try std.testing.expect(try map.update(&@as(u32, 2), struct {
        fn call(value: *u32) void {
            value.* += 5;
        }
    }.call));
    try std.testing.expectEqual(@as(u32, 25), map.get(&@as(u32, 2)).?.*);
    try map.retain(struct {
        fn call(key: *const u32, _: *const u32) bool {
            return key.* % 2 == 0;
        }
    }.call);
    try std.testing.expectEqual(@as(usize, 2), map.count());

    var other = try Map.init(std.testing.allocator);
    defer other.deinit();
    try other.insert(2, 200);
    try other.insert(3, 300);
    try map.insertMap(&other);
    try std.testing.expectEqual(@as(u32, 200), map.get(&@as(u32, 2)).?.*);
    try std.testing.expectEqual(@as(u32, 300), map.get(&@as(u32, 3)).?.*);

    var iterator = map.iteratorFrom(&@as(u32, 3));
    try std.testing.expectEqual(@as(u32, 3), iterator.next().?.key);
    try std.testing.expectEqual(@as(u32, 4), iterator.next().?.key);
    try std.testing.expect(iterator.next() == null);
}

test "map custom range targets and snapshot isolation" {
    var map = try Map.init(std.testing.allocator);
    defer map.deinit();
    for (0..10) |index| try map.insert(@intCast(index), @intCast(index * 10));
    var snapshot = map.clone();
    defer snapshot.deinit();

    try map.removeRange(KeyTarget, .{ .key = 3 }, InclusiveEndTarget, .{ .key = 6 });
    for (3..7) |key| try std.testing.expect(map.get(&@as(u32, @intCast(key))) == null);
    try std.testing.expectEqual(@as(usize, 6), map.count());
    try std.testing.expectEqual(@as(usize, 10), snapshot.count());
    try std.testing.expectEqual(@as(u32, 50), snapshot.get(&@as(u32, 5)).?.*);
}

test "sum tree keyed batch edits return removed items" {
    const Item = struct { key: u32, value: u32 };
    const ItemOps = struct {
        pub const Summary = usize;
        pub const Context = void;
        pub fn summary(_: *const Item, _: void) usize {
            return 1;
        }
        pub fn zero(_: void) usize {
            return 0;
        }
        pub fn addSummary(total: *usize, value: *const usize, _: void) void {
            total.* += value.*;
        }
        pub fn cloneItem(value: *const Item, _: std.mem.Allocator) !Item {
            return value.*;
        }
        pub fn deinitItem(_: *Item, _: std.mem.Allocator) void {}
        pub fn cloneSummary(value: *const usize, _: std.mem.Allocator) !usize {
            return value.*;
        }
        pub fn deinitSummary(_: *usize, _: std.mem.Allocator) void {}
        pub fn eqlSummary(a: *const usize, b: *const usize) bool {
            return a.* == b.*;
        }
    };
    const Keys = struct {
        pub const Key = u32;
        pub fn key(item: *const Item) u32 {
            return item.key;
        }
        pub fn compareKeys(a: *const u32, b: *const u32) std.math.Order {
            return std.math.order(a.*, b.*);
        }
        pub fn compareItemKey(item: *const Item, key_value: *const u32) std.math.Order {
            return std.math.order(item.key, key_value.*);
        }
    };
    const Tree = sum_tree.SumTree(Item, ItemOps, 2);
    var tree = try Tree.fromSlice(std.testing.allocator, &.{
        .{ .key = 1, .value = 10 }, .{ .key = 2, .value = 20 }, .{ .key = 4, .value = 40 },
    }, {});
    defer tree.deinit();
    const Edit = sum_tree.Edit(Item, u32);
    const edits = [_]Edit{
        .{ .insert = .{ .key = 2, .value = 22 } },
        .{ .remove = 4 },
        .{ .insert = .{ .key = 3, .value = 30 } },
    };
    var removed = try tree.editKeyed(Keys, &edits, {});
    defer removed.deinit(std.testing.allocator);
    try std.testing.expectEqual(@as(usize, 2), removed.items.len);
    try std.testing.expectEqual(@as(u32, 20), removed.items[0].value);
    try std.testing.expectEqual(@as(u32, 40), removed.items[1].value);
    try std.testing.expectEqual(@as(usize, 3), tree.itemCount());
    try std.testing.expectEqual(@as(u32, 22), tree.getKeyed(Keys, &@as(u32, 2)).?.value);
    try std.testing.expectEqual(@as(u32, 30), tree.getKeyed(Keys, &@as(u32, 3)).?.value);
}

test "tree set operations" {
    var set = try Set.init(std.testing.allocator);
    defer set.deinit();
    try set.extend(&.{ 4, 1, 3, 1, 2 });
    try std.testing.expectEqual(@as(usize, 4), set.count());
    try std.testing.expect(set.contains(&@as(u32, 3)));
    try std.testing.expect(try set.remove(3));
    try std.testing.expect(!set.contains(&@as(u32, 3)));
    var iterator = set.iteratorFrom(&@as(u32, 2));
    try std.testing.expectEqual(@as(u32, 2), iterator.next().?.*);
    try std.testing.expectEqual(@as(u32, 4), iterator.next().?.*);
}

test "randomized map matches ordered reference model" {
    var map = try Map.init(std.testing.allocator);
    defer map.deinit();
    var model: [64]?u32 = [_]?u32{null} ** 64;
    var random = std.Random.DefaultPrng.init(0x5eed_0005);
    const rng = random.random();

    for (0..500) |_| {
        const key = rng.uintLessThan(u32, model.len);
        if (rng.boolean()) {
            const value = rng.int(u32);
            const old = try map.insertOrReplace(key, value);
            try std.testing.expectEqual(model[key], old);
            model[key] = value;
        } else {
            const old = try map.remove(key);
            try std.testing.expectEqual(model[key], old);
            model[key] = null;
        }

        var iterator = map.iterator();
        var expected_count: usize = 0;
        for (model, 0..) |value, expected_key| if (value) |expected_value| {
            const entry = iterator.next().?;
            try std.testing.expectEqual(@as(u32, @intCast(expected_key)), entry.key);
            try std.testing.expectEqual(expected_value, entry.value);
            expected_count += 1;
        };
        try std.testing.expect(iterator.next() == null);
        try std.testing.expectEqual(expected_count, map.count());
    }
}
