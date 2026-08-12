const std = @import("std");
const text = @import("text");
const clock = text.clock;

const Operation = struct {
    timestamp: clock.Lamport,
    payload: []const u8,
};

const Ops = struct {
    pub fn timestamp(item: *const Operation) clock.Lamport {
        return item.timestamp;
    }
    pub fn clone(item: *const Operation, allocator: std.mem.Allocator) !Operation {
        return .{ .timestamp = item.timestamp, .payload = try allocator.dupe(u8, item.payload) };
    }
    pub fn deinit(item: *Operation, allocator: std.mem.Allocator) void {
        allocator.free(item.payload);
        item.* = undefined;
    }
};

const Queue = text.OperationQueue(Operation, Ops);

fn stamp(value: u32, replica: u16) clock.Lamport {
    return .{ .value = value, .replica_id = .new(replica) };
}

fn expectQueue(queue: *const Queue, expected_timestamps: []const clock.Lamport, expected_payloads: []const []const u8) !void {
    try std.testing.expectEqual(expected_timestamps.len, queue.len());
    try std.testing.expectEqual(expected_timestamps.len == 0, queue.isEmpty());
    var iterator = queue.iterator();
    var index: usize = 0;
    while (iterator.next()) |item| : (index += 1) {
        try std.testing.expect(index < expected_timestamps.len);
        try std.testing.expect(item.timestamp.eql(expected_timestamps[index]));
        try std.testing.expectEqualStrings(expected_payloads[index], item.payload);
    }
    try std.testing.expectEqual(expected_timestamps.len, index);
}

test "sorted unique insertion, same-batch dedup, and cross-batch replacement" {
    var queue = try Queue.init(std.testing.allocator);
    defer queue.deinit();

    const first = [_]Operation{
        .{ .timestamp = stamp(3, 0), .payload = "three" },
        .{ .timestamp = stamp(1, 2), .payload = "old" },
        .{ .timestamp = stamp(2, 9), .payload = "two-nine" },
        .{ .timestamp = stamp(1, 2), .payload = "same-batch-last" },
        .{ .timestamp = stamp(2, 1), .payload = "two-one" },
    };
    try queue.insert(&first);
    try expectQueue(&queue, &.{ stamp(1, 2), stamp(2, 1), stamp(2, 9), stamp(3, 0) }, &.{ "same-batch-last", "two-one", "two-nine", "three" });

    const second = [_]Operation{
        .{ .timestamp = stamp(2, 9), .payload = "replacement" },
        .{ .timestamp = stamp(4, 0), .payload = "four" },
    };
    try queue.insert(&second);
    try expectQueue(&queue, &.{ stamp(1, 2), stamp(2, 1), stamp(2, 9), stamp(3, 0), stamp(4, 0) }, &.{ "same-batch-last", "two-one", "replacement", "three", "four" });
}

test "generated duplicate batches retain one sorted value per Lamport timestamp" {
    var queue = try Queue.init(std.testing.allocator);
    defer queue.deinit();
    var prng = std.Random.DefaultPrng.init(0x0bad_f00d_0003);
    const random = prng.random();

    var batch: [240]Operation = undefined;
    var payloads: [240][24]u8 = undefined;
    for (&batch, 0..) |*item, index| {
        const value = random.intRangeLessThan(u32, 1, 31);
        const replica = random.intRangeLessThan(u16, 0, 4);
        const payload = try std.fmt.bufPrint(&payloads[index], "item-{d}", .{index});
        item.* = .{ .timestamp = stamp(value, replica), .payload = payload };
    }
    try queue.insert(&batch);

    var previous: ?clock.Lamport = null;
    var count: usize = 0;
    var iterator = queue.iterator();
    while (iterator.next()) |item| : (count += 1) {
        if (previous) |key| try std.testing.expectEqual(std.math.Order.lt, key.order(item.timestamp));
        previous = item.timestamp;

        var expected_index: ?usize = null;
        for (batch, 0..) |candidate, index| {
            if (candidate.timestamp.eql(item.timestamp)) expected_index = index;
        }
        var expected_buffer: [24]u8 = undefined;
        const expected = try std.fmt.bufPrint(&expected_buffer, "item-{d}", .{expected_index.?});
        try std.testing.expectEqualStrings(expected, item.payload);
    }
    try std.testing.expectEqual(queue.len(), count);
}

test "queue owns inserted values; clone, drain, and clear are independently observable" {
    var queue = try Queue.init(std.testing.allocator);
    defer queue.deinit();

    var source = [_]u8{ 'o', 'w', 'n', 'e', 'd' };
    try queue.insert(&.{.{ .timestamp = stamp(1, 0), .payload = &source }});
    @memset(&source, 'x');
    try expectQueue(&queue, &.{stamp(1, 0)}, &.{"owned"});

    var snapshot = queue.clone();
    defer snapshot.deinit();
    var drained = try queue.drain();
    defer drained.deinit();
    try std.testing.expect(queue.isEmpty());
    try expectQueue(&drained, &.{stamp(1, 0)}, &.{"owned"});
    try expectQueue(&snapshot, &.{stamp(1, 0)}, &.{"owned"});

    try drained.clear();
    try std.testing.expect(drained.isEmpty());
    try expectQueue(&snapshot, &.{stamp(1, 0)}, &.{"owned"});
}

fn allocationScenario(allocator: std.mem.Allocator) !void {
    var queue = try Queue.init(allocator);
    defer queue.deinit();
    try queue.insert(&.{
        .{ .timestamp = stamp(2, 0), .payload = "two" },
        .{ .timestamp = stamp(1, 0), .payload = "one" },
        .{ .timestamp = stamp(2, 0), .payload = "two-last" },
    });
    var snapshot = queue.clone();
    defer snapshot.deinit();
    try queue.insert(&.{
        .{ .timestamp = stamp(1, 0), .payload = "one-replaced" },
        .{ .timestamp = stamp(3, 0), .payload = "three" },
    });
    var drained = try queue.drain();
    defer drained.deinit();
    try queue.clear();
}

test "all owning and transactional paths tolerate every allocation failure" {
    try std.testing.checkAllAllocationFailures(std.testing.allocator, allocationScenario, .{});
}
