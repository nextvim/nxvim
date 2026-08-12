const std = @import("std");
const rope = @import("rope");

const Sizes = [_]usize{ 4 * 1024, 64 * 1024, 2 * 1024 * 1024 };

pub fn main(init: std.process.Init) !void {
    const allocator = init.gpa;
    var checksum: u64 = 0;
    for (Sizes) |size| {
        const text = try makeText(allocator, size, false);
        defer allocator.free(text);
        var mark = std.Io.Clock.Timestamp.now(init.io, .awake);
        var value = try rope.Rope.initText(allocator, text);
        defer value.deinit();
        const construct_ns = mark.untilNow(init.io).raw.toNanoseconds();

        mark = std.Io.Clock.Timestamp.now(init.io, .awake);
        var snapshot = value.clone();
        defer snapshot.deinit();
        const middle = value.floorCharBoundary(value.len() / 2);
        try value.replace(.{ .start = middle, .end = middle }, "edit");
        const clone_replace_ns = mark.untilNow(init.io).raw.toNanoseconds();

        mark = std.Io.Clock.Timestamp.now(init.io, .awake);
        const sample_end = value.floorCharBoundary(@min(value.len(), middle + 4096));
        var byte_slice = try value.sliceBytes(.{ .start = middle, .end = sample_end });
        defer byte_slice.deinit();
        var row_slice = try value.sliceRows(.{ .start = 0, .end = @min(@as(u32, 64), value.maxPoint().row + 1) });
        defer row_slice.deinit();
        const slice_ns = mark.untilNow(init.io).raw.toNanoseconds();

        mark = std.Io.Clock.Timestamp.now(init.io, .awake);
        const conversion_count: usize = 2000;
        var conversion_index: usize = 0;
        var offset: usize = 0;
        while (conversion_index < conversion_count) : (conversion_index += 1) {
            offset = value.floorCharBoundary((conversion_index *% 104729) % value.len());
            const point = value.offsetToPoint(offset);
            checksum +%= value.pointToOffset(point);
            checksum +%= value.offsetToOffsetUtf16(offset).value;
            checksum +%= value.offsetUtf16ToOffset(value.offsetToOffsetUtf16(offset));
        }
        const conversion_ns = mark.untilNow(init.io).raw.toNanoseconds();

        mark = std.Io.Clock.Timestamp.now(init.io, .awake);
        const cursor_count: usize = 4000;
        var cursor_index: usize = 0;
        offset = 0;
        while (cursor_index < cursor_count) : (cursor_index += 1) {
            offset = value.floorCharBoundary((cursor_index *% 65537) % value.len());
            var cursor = value.cursor(offset);
            cursor.seekForward(@min(value.len(), offset + 127));
            checksum +%= cursor.offset();
        }
        const cursor_ns = mark.untilNow(init.io).raw.toNanoseconds();

        mark = std.Io.Clock.Timestamp.now(init.io, .awake);
        var chunks = value.chunksIterator();
        while (chunks.next()) |chunk| checksum +%= chunk.len;
        var reverse_chunks = value.reversedChunksInRange(.{ .start = 0, .end = value.len() });
        while (reverse_chunks.next()) |chunk| checksum +%= chunk.len;
        var scalars = value.scalars();
        while (scalars.next()) |scalar| checksum +%= scalar;
        var reverse_scalars = value.reversedScalarsAt(value.len());
        while (reverse_scalars.next()) |scalar| checksum +%= scalar;
        const iteration_ns = mark.untilNow(init.io).raw.toNanoseconds();

        std.debug.print("size={d} construct_ns={d} clone_replace_ns={d} slice_ns={d} conversion_ops={d} conversion_ns_per_op={d} cursor_ops={d} cursor_ns_per_op={d} iteration_ns={d}\n", .{ size, construct_ns, clone_replace_ns, slice_ns, conversion_count, @divTrunc(conversion_ns, @max(@as(usize, 1), conversion_count)), cursor_count, @divTrunc(cursor_ns, @max(@as(usize, 1), cursor_count)), iteration_ns });
    }

    const medium = try makeText(allocator, 64 * 1024, false);
    defer allocator.free(medium);
    var mark = std.Io.Clock.Timestamp.now(init.io, .awake);
    var pushes = try rope.Rope.init(allocator);
    defer pushes.deinit();
    for (0..2000) |_| try pushes.push("small😀\n");
    const small_push_ns = mark.untilNow(init.io).raw.toNanoseconds();

    mark = std.Io.Clock.Timestamp.now(init.io, .awake);
    var serial_chunks: std.ArrayList(rope.Chunk) = .empty;
    defer serial_chunks.deinit(allocator);
    var serial_offset: usize = 0;
    while (serial_offset < medium.len) {
        var end = @min(serial_offset + rope.chunk.MAX_BASE, medium.len);
        while (end < medium.len and (medium[end] & 0xc0) == 0x80) end -= 1;
        try serial_chunks.append(allocator, try rope.Chunk.init(medium[serial_offset..end]));
        serial_offset = end;
    }
    var serial_tree = try rope.ChunkTree.fromSlice(allocator, serial_chunks.items, {});
    defer serial_tree.deinit();
    const large_serial_ns = mark.untilNow(init.io).raw.toNanoseconds();

    mark = std.Io.Clock.Timestamp.now(init.io, .awake);
    var large_push = try rope.Rope.init(allocator);
    defer large_push.deinit();
    try large_push.push(medium);
    const large_push_ns = mark.untilNow(init.io).raw.toNanoseconds();

    var small = try rope.Rope.initText(allocator, "small");
    defer small.deinit();
    var large = try rope.Rope.initText(allocator, medium);
    defer large.deinit();
    mark = std.Io.Clock.Timestamp.now(init.io, .awake);
    var small_large = small.clone();
    defer small_large.deinit();
    try small_large.append(&large);
    const append_small_large_ns = mark.untilNow(init.io).raw.toNanoseconds();
    mark = std.Io.Clock.Timestamp.now(init.io, .awake);
    var large_small = large.clone();
    defer large_small.deinit();
    try large_small.append(&small);
    const append_large_small_ns = mark.untilNow(init.io).raw.toNanoseconds();

    const ascii = try makeText(allocator, 64 * 1024, false);
    defer allocator.free(ascii);
    const complex = try makeText(allocator, 64 * 1024, true);
    defer allocator.free(complex);
    var ascii_rope = try rope.Rope.initText(allocator, ascii);
    defer ascii_rope.deinit();
    var complex_rope = try rope.Rope.initText(allocator, complex);
    defer complex_rope.deinit();
    mark = std.Io.Clock.Timestamp.now(init.io, .awake);
    for (0..10_000) |index| checksum +%= ascii_rope.clipOffset(index % ascii_rope.len(), .left);
    const ascii_clip_ns = mark.untilNow(init.io).raw.toNanoseconds();
    mark = std.Io.Clock.Timestamp.now(init.io, .awake);
    for (0..10_000) |index| {
        const point = complex_rope.offsetToPoint(index % complex_rope.len());
        checksum +%= complex_rope.clipPoint(point, if (index % 2 == 0) .left else .right).column;
    }
    const complex_clip_ns = mark.untilNow(init.io).raw.toNanoseconds();

    std.debug.print("small_push_ns={d} large_build_serial_ns={d} large_push_parallel_ns={d} append_small_large_ns={d} append_large_small_ns={d} ascii_clip_ns={d} complex_clip_ns={d} checksum={d}\n", .{ small_push_ns, large_serial_ns, large_push_ns, append_small_large_ns, append_large_small_ns, ascii_clip_ns, complex_clip_ns, checksum });
}

fn makeText(allocator: std.mem.Allocator, size: usize, complex: bool) ![]u8 {
    const pattern = if (complex) "a👩‍💻é🇺🇸\n" else "abcdefghij\tline\n";
    var text: std.ArrayList(u8) = .empty;
    errdefer text.deinit(allocator);
    try text.ensureTotalCapacity(allocator, size);
    while (text.items.len + pattern.len <= size) try text.appendSlice(allocator, pattern);
    while (text.items.len < size) try text.append(allocator, 'x');
    return text.toOwnedSlice(allocator);
}
