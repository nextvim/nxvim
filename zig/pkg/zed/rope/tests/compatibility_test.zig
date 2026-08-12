const std = @import("std");
const rope = @import("rope");

fn expectText(expected: []const u8, value: *const rope.Rope) !void {
    const actual = try value.toOwnedSlice(std.testing.allocator);
    defer std.testing.allocator.free(actual);
    try std.testing.expectEqualStrings(expected, actual);
}

test "text consumer compatibility surface" {
    var visible = try rope.Rope.initText(std.testing.allocator, "alpha\nβeta\n👩‍💻 tail");
    defer visible.deinit();
    var deleted = try rope.Rope.init(std.testing.allocator);
    defer deleted.deinit();

    var snapshot = visible.clone();
    defer snapshot.deinit();
    try visible.replace(.{ .start = 0, .end = 5 }, "ALPHA");
    try expectText("alpha\nβeta\n👩‍💻 tail", &snapshot);

    const point = visible.offsetToPoint(6);
    try std.testing.expectEqual(@as(usize, 6), visible.pointToOffset(point));
    const point16 = visible.offsetToPointUtf16(6);
    try std.testing.expectEqual(@as(usize, 6), visible.pointUtf16ToOffset(point16));
    const utf16 = visible.offsetToOffsetUtf16(6);
    try std.testing.expectEqual(@as(usize, 6), visible.offsetUtf16ToOffset(utf16));
    try std.testing.expectEqual(utf16, visible.pointToOffsetUtf16(point));
    try std.testing.expectEqual(utf16, visible.pointUtf16ToOffsetUtf16(point16));
    _ = visible.pointToPointUtf16(point);
    _ = visible.pointUtf16ToPoint(point16);
    _ = visible.clipPoint(.new(2, 5), .left);
    _ = visible.clipPointUtf16(.init(.new(2, 5)), .right);
    _ = visible.clipOffsetUtf16(utf16, .left);
    try std.testing.expect(visible.isCharBoundary(6));
    try std.testing.expect(visible.assertCharBoundary(false, 6));
    try std.testing.expect(!visible.assertCharBoundary(false, 7));
    try std.testing.expect(!visible.assertCharBoundary(false, visible.len() + 1));
    try std.testing.expectEqual(@as(u32, 5), visible.lineLen(0));

    var builder = try rope.Rope.init(std.testing.allocator);
    defer builder.deinit();
    var cursor = visible.cursor(0);
    var first = try cursor.slice(6);
    defer first.deinit();
    try builder.append(&first);
    cursor.seekForward(visible.pointToOffset(.new(1, 0)));
    var tail = try cursor.suffix();
    defer tail.deinit();
    try builder.append(&tail);
    var summary_cursor = visible.cursor(0);
    _ = summary_cursor.summary(rope.TextSummary, visible.len());

    var chunks = visible.chunksInRange(.{ .start = 0, .end = visible.len() });
    var saw_bitmap = false;
    while (chunks.nextWithBitmaps()) |view| {
        saw_bitmap = true;
        try std.testing.expect(view.text.len <= rope.chunk.MAX_BASE);
    }
    try std.testing.expect(saw_bitmap);
    chunks = visible.chunksInRange(.{ .start = 0, .end = visible.len() });
    try std.testing.expect(chunks.nextLine());
    try std.testing.expect(chunks.prevLine());

    var bytes = visible.bytesInRange(.{ .start = 0, .end = visible.len() });
    var buffer: [17]u8 = undefined;
    var byte_count: usize = 0;
    while (true) {
        const count = bytes.read(&buffer);
        if (count == 0) break;
        byte_count += count;
    }
    try std.testing.expectEqual(visible.len(), byte_count);

    var reverse_bytes = visible.reversedBytesInRange(.{ .start = 0, .end = visible.len() });
    try std.testing.expect(reverse_bytes.read(&buffer) > 0);
    var scalars = visible.scalarsAt(0);
    try std.testing.expect(scalars.next() != null);
    var reverse_scalars = visible.reversedScalarsAt(visible.len());
    try std.testing.expect(reverse_scalars.next() != null);

    var lines = visible.lines(std.testing.allocator);
    defer lines.deinit();
    try std.testing.expectEqualStrings("ALPHA", (try lines.next()).?);
    lines.seek(0);
    try std.testing.expectEqual(@as(usize, 0), lines.offset());

    var row_slice = try visible.sliceRows(.{ .start = 1, .end = 2 });
    defer row_slice.deinit();
    try expectText("βeta\n", &row_slice);
    try deleted.push("deleted");
    try std.testing.expect(visible.startsWith("ALPHA"));
    try std.testing.expect(visible.endsWith("tail"));
    try visible.validate();
    try deleted.validate();
}

test "text consumer direct UTF-16 offsets preserve clipping semantics" {
    var value = try rope.Rope.initText(std.testing.allocator, "a😀z\nβ👩‍💻x");
    defer value.deinit();

    const byte_point = rope.Point.new(0, 2);
    try std.testing.expectEqual(value.offsetToOffsetUtf16(2), value.pointToOffsetUtf16(byte_point));

    const inside_surrogate = rope.PointUtf16.new(0, 2);
    try std.testing.expectEqual(
        value.offsetToOffsetUtf16(value.pointUtf16ToOffset(inside_surrogate)),
        value.pointUtf16ToOffsetUtf16(inside_surrogate),
    );

    try std.testing.expectEqual(value.summary().len_utf16, value.pointToOffsetUtf16(rope.Point.max));
    try std.testing.expectEqual(value.summary().len_utf16, value.pointUtf16ToOffsetUtf16(rope.PointUtf16.max));
}
