const std = @import("std");
const builtin = @import("builtin");
const text = @import("text");

pub fn main(init: std.process.Init) !void {
    const allocator = init.gpa;
    std.debug.print("zig={any} target={s}-{s} mode=ReleaseFast\n", .{ builtin.zig_version, @tagName(builtin.cpu.arch), @tagName(builtin.os.tag) });
    std.debug.print("sizeof Buffer={d} Snapshot={d} Fragment={d} Operation={d} Anchor={d}\n", .{ @sizeOf(text.Buffer), @sizeOf(text.BufferSnapshot), @sizeOf(text.Fragment), @sizeOf(text.Operation), @sizeOf(text.Anchor) });
    var checksum: usize = 0;
    for ([_]usize{ 4 * 1024, 64 * 1024, 2 * 1024 * 1024 }) |size| {
        const source = try makeText(allocator, size);
        defer allocator.free(source);
        const id = try text.BufferId.new(size + 1);
        var mark = std.Io.Clock.Timestamp.now(init.io, .awake);
        var buffer = try text.Buffer.init(allocator, text.ReplicaId.new(8), id, source);
        defer buffer.deinit();
        const construct = mark.untilNow(init.io).raw.toNanoseconds();

        mark = std.Io.Clock.Timestamp.now(init.io, .awake);
        var snapshot = try buffer.cloneSnapshot();
        defer snapshot.deinit();
        var branch = try buffer.branch();
        defer branch.deinit();
        const snapshot_branch = mark.untilNow(init.io).raw.toNanoseconds();

        const middle = buffer.snapshot().clipOffset(buffer.snapshot().len() / 2, .left);
        mark = std.Io.Clock.Timestamp.now(init.io, .awake);
        var operation = try buffer.edit(&.{.{ .start = middle, .end = middle, .new_text = "edit😀" }});
        defer operation.deinit();
        const local_edit = mark.untilNow(init.io).raw.toNanoseconds();

        mark = std.Io.Clock.Timestamp.now(init.io, .awake);
        for (0..2000) |index| {
            const offset = (index *% 104729) % (buffer.snapshot().len() + 1);
            const point = buffer.snapshot().offsetToPoint(offset);
            checksum +%= buffer.snapshot().pointToOffset(point);
            const anchor = buffer.snapshot().anchorAfter(offset);
            checksum +%= buffer.snapshot().offsetForAnchor(anchor) orelse 0;
        }
        const queries = mark.untilNow(init.io).raw.toNanoseconds();

        mark = std.Io.Clock.Timestamp.now(init.io, .awake);
        var chunks = buffer.snapshot().chunks();
        while (chunks.next()) |chunk| checksum +%= chunk.len;
        var scalars = buffer.snapshot().scalars();
        while (scalars.next()) |scalar| checksum +%= scalar;
        const traversal = mark.untilNow(init.io).raw.toNanoseconds();

        mark = std.Io.Clock.Timestamp.now(init.io, .awake);
        var undone = (try buffer.undo()).?;
        undone[1].deinit();
        var redone = (try buffer.redo()).?;
        redone[1].deinit();
        const undo_redo = mark.untilNow(init.io).raw.toNanoseconds();
        std.debug.print("size={d} construct_ns={d} snapshot_branch_ns={d} local_edit_ns={d} query_2000_ns={d} traversal_ns={d} undo_redo_ns={d}\n", .{ size, construct, snapshot_branch, local_edit, queries, traversal, undo_redo });
    }

    for ([_]usize{ 2, 4, 8 }) |count| {
        const replicas = try allocator.alloc(text.Buffer, count);
        defer allocator.free(replicas);
        const id = try text.BufferId.new(10_000 + count);
        for (replicas, 0..) |*replica, index| replica.* = try text.Buffer.init(allocator, text.ReplicaId.new(@intCast(8 + index)), id, "sync");
        defer for (replicas) |*replica| replica.deinit();
        var operations: std.ArrayList(text.Operation) = .empty;
        defer {
            for (operations.items) |*operation| operation.deinit();
            operations.deinit(allocator);
        }
        for (replicas, 0..) |*replica, index| {
            var payload: [16]u8 = undefined;
            const value = try std.fmt.bufPrint(&payload, "-{d}", .{index});
            try operations.append(allocator, try replica.edit(&.{.{ .start = replica.snapshot().len(), .end = replica.snapshot().len(), .new_text = value }}));
        }
        var mark = std.Io.Clock.Timestamp.now(init.io, .awake);
        for (replicas) |*replica| try replica.applyOps(operations.items);
        const sync = mark.untilNow(init.io).raw.toNanoseconds();
        checksum +%= replicas[0].snapshot().len();
        std.debug.print("replicas={d} synchronize_ns={d}\n", .{ count, sync });
    }
    std.debug.print("checksum={d}\n", .{checksum});
}

fn makeText(allocator: std.mem.Allocator, size: usize) ![]u8 {
    const pattern = "abcdefghij\tline 😀\n";
    const result = try allocator.alloc(u8, size);
    var offset: usize = 0;
    while (offset + pattern.len <= size) : (offset += pattern.len) @memcpy(result[offset .. offset + pattern.len], pattern);
    @memset(result[offset..], 'x');
    return result;
}
