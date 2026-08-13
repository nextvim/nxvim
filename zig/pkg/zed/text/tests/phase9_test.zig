const std = @import("std");
const text = @import("text");

const seeds = [_]u64{ 0x90d0_24b0_0001, 0x90d0_24b0_0002, 0x90d0_24b0_0003 };
const atoms = [_][]const u8{ "", "a", "Z", "\n", "é", "中", "😀", "e\xcc\x81" };

fn boundaries(bytes: []const u8, storage: *[2048]usize) []const usize {
    var count: usize = 1;
    storage[0] = 0;
    var index: usize = 0;
    while (index < bytes.len) {
        index += std.unicode.utf8ByteSequenceLength(bytes[index]) catch unreachable;
        storage[count] = index;
        count += 1;
    }
    return storage[0..count];
}

fn replaceFlat(allocator: std.mem.Allocator, old: []const u8, start: usize, end: usize, replacement: []const u8) ![]u8 {
    const result = try allocator.alloc(u8, start + replacement.len + old.len - end);
    @memcpy(result[0..start], old[0..start]);
    @memcpy(result[start .. start + replacement.len], replacement);
    @memcpy(result[start + replacement.len ..], old[end..]);
    return result;
}

fn expectBuffer(buffer: *const text.Buffer, expected: []const u8) !void {
    const actual = try buffer.snapshot().text(std.testing.allocator);
    defer std.testing.allocator.free(actual);
    try std.testing.expectEqualStrings(expected, actual);
    try buffer.validate();
}

test "multi-seed stateful UTF-8 edits match a flat model after every operation" {
    const allocator = std.testing.allocator;
    for (seeds, 0..) |seed, seed_index| {
        var prng = std.Random.DefaultPrng.init(seed);
        const random = prng.random();
        var model = try allocator.dupe(u8, "α seed 😀\n");
        defer allocator.free(model);
        const id = try text.BufferId.new(100 + seed_index);
        var buffer = try text.Buffer.init(allocator, text.ReplicaId.new(@intCast(8 + seed_index)), id, model);
        defer buffer.deinit();
        var retained: ?text.BufferSnapshot = null;
        defer if (retained) |*snapshot| snapshot.deinit();
        var retained_text: ?[]u8 = null;
        defer if (retained_text) |bytes| allocator.free(bytes);

        for (0..160) |step| {
            var storage: [2048]usize = undefined;
            const points = boundaries(model, &storage);
            const start_index = random.uintLessThan(usize, points.len);
            const end_index = start_index + random.uintLessThan(usize, points.len - start_index);
            const start = points[start_index];
            const end = points[end_index];
            const replacement = atoms[random.uintLessThan(usize, atoms.len)];
            if (step % 37 == 0) {
                if (retained) |*snapshot| snapshot.deinit();
                if (retained_text) |bytes| allocator.free(bytes);
                retained = try buffer.cloneSnapshot();
                retained_text = try retained.?.text(allocator);
            }
            const next = try replaceFlat(allocator, model, start, end, replacement);
            var operation = try buffer.edit(&.{.{ .start = start, .end = end, .new_text = replacement }});
            operation.deinit();
            allocator.free(model);
            model = next;
            try expectBuffer(&buffer, model);
            const point = buffer.snapshot().offsetToPoint(random.uintLessThan(usize, model.len + 1));
            const clipped = buffer.snapshot().pointToOffset(point);
            try std.testing.expect(clipped <= model.len);
            if (retained) |*snapshot| {
                const unchanged = try snapshot.text(allocator);
                defer allocator.free(unchanged);
                try std.testing.expectEqualStrings(retained_text.?, unchanged);
            }
        }
    }
}

test "seeded four-replica schedules converge with reverse random and duplicate delivery" {
    const allocator = std.testing.allocator;
    for (seeds) |seed| {
        const id = try text.BufferId.new(200 + seed % 1000);
        var replicas: [4]text.Buffer = undefined;
        for (&replicas, 0..) |*replica, index| replica.* = try text.Buffer.init(allocator, text.ReplicaId.new(@intCast(8 + index)), id, "root");
        defer for (&replicas) |*replica| replica.deinit();
        var operations: std.ArrayList(text.Operation) = .empty;
        defer {
            for (operations.items) |*operation| operation.deinit();
            operations.deinit(allocator);
        }
        for (&replicas, 0..) |*replica, replica_index| {
            for (0..4) |round| {
                var payload: [8]u8 = undefined;
                const value = try std.fmt.bufPrint(&payload, "{d}{d}", .{ replica_index, round });
                try operations.append(allocator, try replica.edit(&.{.{ .start = replica.snapshot().len(), .end = replica.snapshot().len(), .new_text = value }}));
            }
        }
        const order = try allocator.alloc(usize, operations.items.len);
        defer allocator.free(order);
        for (order, 0..) |*value, index| value.* = index;
        var prng = std.Random.DefaultPrng.init(seed);
        prng.random().shuffle(usize, order);
        for (&replicas, 0..) |*replica, replica_index| {
            if (replica_index % 2 == 0) std.mem.reverse(usize, order);
            for (order) |index| try replica.applyOps(&.{ operations.items[index], operations.items[index] });
        }
        const expected = try replicas[0].snapshot().text(allocator);
        defer allocator.free(expected);
        for (&replicas) |*replica| try expectBuffer(replica, expected);
    }
}

test "multi-chunk middle insertion rebuilds canonical Rope chunks" {
    const allocator = std.testing.allocator;
    const source = try allocator.alloc(u8, 4096);
    defer allocator.free(source);
    @memset(source, 'x');
    const id = try text.BufferId.new(401);
    var buffer = try text.Buffer.init(allocator, text.ReplicaId.new(8), id, source);
    defer buffer.deinit();
    var operation = try buffer.edit(&.{.{ .start = 2048, .end = 2048, .new_text = "edit😀" }});
    defer operation.deinit();
    try buffer.validate();
    try std.testing.expectEqual(@as(usize, 4096 + "edit😀".len), buffer.snapshot().len());
}

test "retained snapshots support concurrent read-only traversal" {
    const allocator = std.testing.allocator;
    const id = try text.BufferId.new(400);
    var buffer = try text.Buffer.init(allocator, text.ReplicaId.new(8), id, "line one\nline two 😀\n");
    defer buffer.deinit();
    var snapshots: [4]text.BufferSnapshot = undefined;
    for (&snapshots) |*snapshot| snapshot.* = try buffer.cloneSnapshot();
    defer for (&snapshots) |*snapshot| snapshot.deinit();
    const Worker = struct {
        fn run(snapshot: *const text.BufferSnapshot) void {
            var checksum: usize = 0;
            for (0..1000) |index| {
                const offset = index % (snapshot.len() + 1);
                checksum +%= snapshot.pointToOffset(snapshot.offsetToPoint(offset));
                var chunks = snapshot.chunks();
                while (chunks.next()) |chunk| checksum +%= chunk.len;
            }
            std.mem.doNotOptimizeAway(checksum);
        }
    };
    var threads: [4]std.Thread = undefined;
    for (&threads, &snapshots) |*thread, *snapshot| thread.* = try std.Thread.spawn(.{}, Worker.run, .{snapshot});
    for (&threads) |*thread| thread.join();
}
