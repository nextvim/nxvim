const std = @import("std");
const text = @import("text");

const OwnedBytes = struct {
    pub fn init(allocator: std.mem.Allocator) ![]u8 {
        return allocator.alloc(u8, 0);
    }

    pub fn clone(value: *const []u8, allocator: std.mem.Allocator) ![]u8 {
        return allocator.dupe(u8, value.*);
    }

    pub fn deinit(value: *[]u8, allocator: std.mem.Allocator) void {
        allocator.free(value.*);
        value.* = undefined;
    }

    pub fn combine(current: *const []u8, update: *const []u8, allocator: std.mem.Allocator) ![]u8 {
        const result = try allocator.alloc(u8, current.len + update.len);
        @memcpy(result[0..current.len], current.*);
        @memcpy(result[current.len..], update.*);
        return result;
    }
};

const BytesTopic = text.Topic([]u8, OwnedBytes);
const BytesSubscription = text.Subscription([]u8, OwnedBytes);

const Patch = text.Patch(usize);
const PatchOps = struct {
    pub fn init(allocator: std.mem.Allocator) !Patch {
        return Patch.empty(allocator);
    }
    pub fn clone(value: *const Patch, allocator: std.mem.Allocator) !Patch {
        return value.clone(allocator);
    }
    pub fn deinit(value: *Patch, _: std.mem.Allocator) void {
        value.deinit();
    }
    pub fn combine(current: *const Patch, update: *const Patch, _: std.mem.Allocator) !Patch {
        return current.compose(update.edits());
    }
};

const PatchTopic = text.Topic(Patch, PatchOps);

test "Patch subscriptions compose exactly like direct Patch composition" {
    var topic = PatchTopic.init(std.testing.allocator);
    defer topic.deinit();
    var subscription = try topic.subscribe();
    defer subscription.deinit();

    var first = try Patch.new(std.testing.allocator, &.{.{
        .old = .{ .start = 1, .end = 3 },
        .new = .{ .start = 1, .end = 4 },
    }});
    defer first.deinit();
    var second = try Patch.new(std.testing.allocator, &.{.{
        .old = .{ .start = 3, .end = 5 },
        .new = .{ .start = 3, .end = 6 },
    }});
    defer second.deinit();

    try topic.publish(&first);
    try topic.publish(&second);
    var actual = try subscription.consume();
    defer actual.deinit();
    var expected = try first.compose(second.edits());
    defer expected.deinit();
    try std.testing.expectEqualSlices(Patch.Edit, expected.edits(), actual.edits());
}

test "publish accumulates, read clones, and consume takes then resets" {
    var topic = BytesTopic.init(std.testing.allocator);
    defer topic.deinit();
    var first = try topic.subscribe();
    defer first.deinit();
    var second = try topic.subscribe();
    defer second.deinit();

    const ab: []u8 = @constCast("ab");
    const cd: []u8 = @constCast("cd");
    try topic.publish(&ab);
    try topic.publish(&cd);

    var snapshot = try first.read(std.testing.allocator);
    defer OwnedBytes.deinit(&snapshot, std.testing.allocator);
    try std.testing.expectEqualStrings("abcd", snapshot);

    var consumed = try first.consume();
    defer OwnedBytes.deinit(&consumed, std.testing.allocator);
    try std.testing.expectEqualStrings("abcd", consumed);

    var empty = try first.consume();
    defer OwnedBytes.deinit(&empty, std.testing.allocator);
    try std.testing.expectEqual(@as(usize, 0), empty.len);

    var other = try second.consume();
    defer OwnedBytes.deinit(&other, std.testing.allocator);
    try std.testing.expectEqualStrings("abcd", other);
}

test "stale subscribers are ignored and pruned without affecting live ones" {
    var topic = BytesTopic.init(std.testing.allocator);
    defer topic.deinit();
    var stale = try topic.subscribe();
    var live = try topic.subscribe();
    defer live.deinit();

    stale.deinit();
    const update: []u8 = @constCast("live");
    try topic.publish(&update);
    try std.testing.expectEqual(@as(usize, 1), topic.subscribers.items.len);
    try std.testing.expectError(error.Cancelled, stale.consume());

    var value = try live.consume();
    defer OwnedBytes.deinit(&value, std.testing.allocator);
    try std.testing.expectEqualStrings("live", value);
}

test "subscription remains usable after publisher deinit" {
    var topic = BytesTopic.init(std.testing.allocator);
    var sub = try topic.subscribe();
    defer sub.deinit();
    const update: []u8 = @constCast("pending");
    try topic.publish(&update);
    topic.deinit();

    var value = try sub.consume();
    defer OwnedBytes.deinit(&value, std.testing.allocator);
    try std.testing.expectEqualStrings("pending", value);
}

fn allocationScenario(allocator: std.mem.Allocator) !void {
    var topic = BytesTopic.init(allocator);
    defer topic.deinit();
    var first = try topic.subscribe();
    defer first.deinit();
    var second = try topic.subscribe();
    defer second.deinit();
    const update: []u8 = @constCast("an allocating update");
    try topic.publish(&update);
    var value = try first.read(allocator);
    defer OwnedBytes.deinit(&value, allocator);
    var consumed = try second.consume();
    defer OwnedBytes.deinit(&consumed, allocator);
}

test "owning operations clean up under every allocation failure" {
    try std.testing.checkAllAllocationFailures(std.testing.allocator, allocationScenario, .{});
}

test "failed publish loses no previously pending update" {
    var storage: [512]u8 = undefined;
    var fixed = std.heap.FixedBufferAllocator.init(&storage);
    const allocator = fixed.allocator();
    var topic = BytesTopic.init(allocator);
    defer topic.deinit();
    var sub = try topic.subscribe();
    defer sub.deinit();

    const kept: []u8 = @constCast("kept");
    try topic.publish(&kept);
    var enormous: [1024]u8 = @splat('x');
    const too_large: []u8 = &enormous;
    try std.testing.expectError(error.OutOfMemory, topic.publish(&too_large));

    var value = try sub.consume();
    defer OwnedBytes.deinit(&value, allocator);
    try std.testing.expectEqualStrings("kept", value);
}

test "concurrent publishers serialize without losing updates" {
    var topic = BytesTopic.init(std.heap.page_allocator);
    defer topic.deinit();
    var sub = try topic.subscribe();
    defer sub.deinit();

    const Worker = struct {
        fn run(topic_ptr: *BytesTopic, byte: u8) void {
            var storage = [_]u8{byte};
            var update: []u8 = &storage;
            for (0..200) |_| topic_ptr.publish(&update) catch unreachable;
        }
    };

    var left = try std.Thread.spawn(.{}, Worker.run, .{ &topic, @as(u8, 'a') });
    var right = try std.Thread.spawn(.{}, Worker.run, .{ &topic, @as(u8, 'b') });
    left.join();
    right.join();

    var value = try sub.consume();
    defer OwnedBytes.deinit(&value, std.heap.page_allocator);
    try std.testing.expectEqual(@as(usize, 400), value.len);
    var a_count: usize = 0;
    var b_count: usize = 0;
    for (value) |byte| if (byte == 'a') {
        a_count += 1;
    } else if (byte == 'b') {
        b_count += 1;
    };
    try std.testing.expectEqual(@as(usize, 200), a_count);
    try std.testing.expectEqual(@as(usize, 200), b_count);
}
