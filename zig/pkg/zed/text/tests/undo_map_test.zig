const std = @import("std");
const text = @import("text");
const clock = text.clock;
const UndoMap = text.UndoMap;
const UndoOperation = text.UndoMapOperation;
const Count = text.UndoMapCount;

fn ts(value: u32, replica: u16) clock.Lamport {
    return .{ .value = value, .replica_id = clock.ReplicaId.new(replica) };
}

fn insert(map: *UndoMap, timestamp: clock.Lamport, counts: []const Count) !void {
    try map.insert(&.{ .timestamp = timestamp, .counts = counts });
}

test "Rust-equivalent ordering, replacement, maximum, parity, and absent vectors" {
    var map = try UndoMap.init(std.testing.allocator);
    defer map.deinit();

    const edit_a = ts(3, 9);
    const edit_b = ts(3, 2); // Same Lamport value, but orders before edit_a by replica.
    try insert(&map, ts(20, 2), &.{
        .{ .edit_id = edit_a, .count = 1 },
        .{ .edit_id = edit_b, .count = 4 },
    });
    try insert(&map, ts(10, 8), &.{.{ .edit_id = edit_a, .count = 2 }});
    try insert(&map, ts(30, 1), &.{.{ .edit_id = edit_a, .count = 3 }});

    try std.testing.expectEqual(@as(u32, 3), map.undoCount(edit_a));
    try std.testing.expect(map.isUndone(edit_a));
    try std.testing.expectEqual(@as(u32, 4), map.undoCount(edit_b));
    try std.testing.expect(!map.isUndone(edit_b));
    try std.testing.expectEqual(@as(u32, 0), map.undoCount(ts(99, 99)));
    try std.testing.expect(!map.isUndone(ts(99, 99)));

    // The same compound key is updated, rather than duplicated. A lower update
    // does not supersede a greater count stored at another undo timestamp.
    try insert(&map, ts(30, 1), &.{.{ .edit_id = edit_a, .count = 6 }});
    try std.testing.expectEqual(@as(u32, 6), map.undoCount(edit_a));
    try std.testing.expect(!map.isUndone(edit_a));
    try insert(&map, ts(30, 1), &.{.{ .edit_id = edit_a, .count = 1 }});
    try std.testing.expectEqual(@as(u32, 2), map.undoCount(edit_a));
}

test "version-relative lookup observes undo clocks before taking maximum" {
    var map = try UndoMap.init(std.testing.allocator);
    defer map.deinit();
    const edit_id = ts(7, 4);

    try insert(&map, ts(5, 1), &.{.{ .edit_id = edit_id, .count = 1 }});
    try insert(&map, ts(9, 2), &.{.{ .edit_id = edit_id, .count = 4 }});
    try insert(&map, ts(12, 1), &.{.{ .edit_id = edit_id, .count = 3 }});

    var version = clock.Global.init(std.testing.allocator);
    defer version.deinit();
    try std.testing.expect(!map.wasUndone(edit_id, &version));
    try version.observe(ts(5, 1));
    try std.testing.expect(map.wasUndone(edit_id, &version));
    try version.observe(ts(9, 2));
    try std.testing.expect(!map.wasUndone(edit_id, &version));
    try version.observe(ts(12, 1));
    // Maximum is 4, not the count belonging to the latest ordered undo id.
    try std.testing.expect(!map.wasUndone(edit_id, &version));
}

test "generated versions match a direct Rust-equivalent maximum/parity model" {
    var map = try UndoMap.init(std.testing.allocator);
    defer map.deinit();
    const edit_id = ts(41, 7);
    const Vector = struct { id: clock.Lamport, count: u32 };
    const vectors = [_]Vector{
        .{ .id = ts(2, 0), .count = 1 },
        .{ .id = ts(8, 3), .count = 2 },
        .{ .id = ts(5, 1), .count = 7 },
        .{ .id = ts(11, 0), .count = 8 },
        .{ .id = ts(6, 3), .count = 9 },
    };
    for (vectors) |vector| try insert(&map, vector.id, &.{.{ .edit_id = edit_id, .count = vector.count }});

    var replica_zero: u32 = 0;
    while (replica_zero <= 12) : (replica_zero += 1) {
        var replica_one: u32 = 0;
        while (replica_one <= 7) : (replica_one += 1) {
            var replica_three: u32 = 0;
            while (replica_three <= 9) : (replica_three += 1) {
                var version = clock.Global.init(std.testing.allocator);
                defer version.deinit();
                try version.observe(ts(replica_zero, 0));
                try version.observe(ts(replica_one, 1));
                try version.observe(ts(replica_three, 3));

                var expected: u32 = 0;
                for (vectors) |vector| if (version.observed(vector.id)) {
                    expected = @max(expected, vector.count);
                };
                try std.testing.expectEqual(expected % 2 == 1, map.wasUndone(edit_id, &version));
            }
        }
    }
}

test "clone is isolated by copy-on-write updates" {
    var map = try UndoMap.init(std.testing.allocator);
    defer map.deinit();
    const edit_id = ts(1, 1);
    try insert(&map, ts(2, 1), &.{.{ .edit_id = edit_id, .count = 1 }});

    var snapshot = try map.clone(std.testing.allocator);
    defer snapshot.deinit();
    try insert(&map, ts(3, 1), &.{.{ .edit_id = edit_id, .count = 2 }});
    try std.testing.expectEqual(@as(u32, 2), map.undoCount(edit_id));
    try std.testing.expectEqual(@as(u32, 1), snapshot.undoCount(edit_id));
}

fn allocationScenario(allocator: std.mem.Allocator) !void {
    var map = try UndoMap.init(allocator);
    defer map.deinit();
    const edit_id = ts(1, 0);
    try insert(&map, ts(2, 0), &.{.{ .edit_id = edit_id, .count = 1 }});

    insert(&map, ts(3, 0), &.{
        .{ .edit_id = edit_id, .count = 2 },
        .{ .edit_id = ts(2, 0), .count = 5 },
        .{ .edit_id = ts(3, 0), .count = 7 },
        .{ .edit_id = ts(4, 0), .count = 9 },
        .{ .edit_id = ts(5, 0), .count = 11 },
        .{ .edit_id = ts(6, 0), .count = 13 },
        .{ .edit_id = ts(7, 0), .count = 15 },
    }) catch |err| {
        // No prefix of the failed batch may become visible.
        try std.testing.expectEqual(@as(u32, 1), map.undoCount(edit_id));
        try std.testing.expectEqual(@as(u32, 0), map.undoCount(ts(2, 0)));
        return err;
    };
    try std.testing.expectEqual(@as(u32, 15), map.undoCount(ts(7, 0)));
}

test "insert is leak-free and transactional at every allocator failure" {
    try std.testing.checkAllAllocationFailures(std.testing.allocator, allocationScenario, .{});
}
