const std = @import("std");
const rope = @import("rope");

fn makeMultiChunk() !rope.Rope {
    var text: std.ArrayList(u8) = .empty;
    defer text.deinit(std.testing.allocator);
    for (0..70) |index| {
        try text.appendSlice(std.testing.allocator, if (index % 3 == 0) "αβ\tline\n" else "abcdefghij");
    }
    try text.appendSlice(std.testing.allocator, "tail😀");
    return rope.Rope.initText(std.testing.allocator, text.items);
}

fn appendChunks(output: *std.ArrayList(u8), chunks: *rope.Chunks) !void {
    while (chunks.next()) |text| try output.appendSlice(std.testing.allocator, text);
}

test "cursor slice suffix seek and generic summaries" {
    var value = try rope.Rope.initText(std.testing.allocator, "ab😀\nβeta\ntail");
    defer value.deinit();
    var cursor = value.cursor(2);
    try std.testing.expectEqual(@as(usize, 2), cursor.offset());
    const expected_text = "😀\nβeta";
    const end = 2 + expected_text.len;
    const expected = try rope.TextSummary.parse(expected_text);
    try std.testing.expectEqual(expected, cursor.summary(rope.TextSummary, end));
    try std.testing.expectEqual(end, cursor.offset());

    cursor = value.cursor(2);
    var slice = try cursor.slice(end);
    defer slice.deinit();
    const materialized = try slice.toOwnedSlice(std.testing.allocator);
    defer std.testing.allocator.free(materialized);
    try std.testing.expectEqualStrings(expected_text, materialized);

    var suffix = try cursor.suffix();
    defer suffix.deinit();
    const suffix_text = try suffix.toOwnedSlice(std.testing.allocator);
    defer std.testing.allocator.free(suffix_text);
    try std.testing.expectEqualStrings("\ntail", suffix_text);
}

test "forward reverse chunks preserve exact range and bitmaps" {
    var value = try makeMultiChunk();
    defer value.deinit();
    const text = try value.toOwnedSlice(std.testing.allocator);
    defer std.testing.allocator.free(text);
    const range = rope.ByteRange{ .start = 2, .end = text.len - 4 };

    var forward = value.chunksInRange(range);
    var actual: std.ArrayList(u8) = .empty;
    defer actual.deinit(std.testing.allocator);
    try appendChunks(&actual, &forward);
    try std.testing.expectEqualSlices(u8, text[range.start..range.end], actual.items);

    var reverse = value.reversedChunksInRange(range);
    var restored: std.ArrayList(u8) = .empty;
    defer restored.deinit(std.testing.allocator);
    while (reverse.next()) |chunk| try restored.insertSlice(std.testing.allocator, 0, chunk);
    try std.testing.expectEqualSlices(u8, text[range.start..range.end], restored.items);

    var bitmap_chunks = value.chunksInRange(.{ .start = 2, .end = 18 });
    const view = bitmap_chunks.nextWithBitmaps().?;
    try std.testing.expectEqualSlices(u8, text[2 .. 2 + view.text.len], view.text);
    for (view.text, 0..) |byte, index| {
        const bit: rope.chunk.Bitmap = @as(rope.chunk.Bitmap, 1) << @intCast(index);
        try std.testing.expectEqual((byte & 0xc0) != 0x80, (view.chars & bit) != 0);
        try std.testing.expectEqual(byte == '\t', (view.tabs & bit) != 0);
        try std.testing.expectEqual(byte == '\n', (view.newlines & bit) != 0);
    }
}

test "bytes read and scalars work in both directions" {
    const text = "aα😀z";
    var value = try rope.Rope.initText(std.testing.allocator, text);
    defer value.deinit();

    var bytes = value.bytesInRange(.{ .start = 1, .end = text.len - 1 });
    var output: [16]u8 = undefined;
    const count = bytes.read(&output);
    try std.testing.expectEqualSlices(u8, text[1 .. text.len - 1], output[0..count]);

    var reverse_bytes = value.reversedBytesInRange(.{ .start = 1, .end = text.len - 1 });
    const reverse_count = reverse_bytes.read(&output);
    const expected = try std.testing.allocator.dupe(u8, text[1 .. text.len - 1]);
    defer std.testing.allocator.free(expected);
    std.mem.reverse(u8, expected);
    try std.testing.expectEqualSlices(u8, expected, output[0..reverse_count]);

    var scalars = value.scalars();
    for ([_]u21{ 'a', 'α', '😀', 'z' }) |expected_scalar| try std.testing.expectEqual(expected_scalar, scalars.next().?);
    try std.testing.expect(scalars.next() == null);
    var reversed = value.reversedScalarsAt(value.len());
    for ([_]u21{ 'z', '😀', 'α', 'a' }) |expected_scalar| try std.testing.expectEqual(expected_scalar, reversed.next().?);
    try std.testing.expect(reversed.next() == null);
}

fn lineAllocationScenario(allocator_value: std.mem.Allocator) !void {
    var source: std.ArrayList(u8) = .empty;
    defer source.deinit(allocator_value);
    try source.appendNTimes(allocator_value, 'x', 300);
    try source.appendSlice(allocator_value, "\nend");
    var value = try rope.Rope.initText(allocator_value, source.items);
    defer value.deinit();
    var lines = value.lines(allocator_value);
    defer lines.deinit();
    try std.testing.expectEqual(@as(usize, 300), (try lines.next()).?.len);
    try std.testing.expectEqualStrings("end", (try lines.next()).?);
}

test "line scratch cleans up every induced allocation failure" {
    try std.testing.checkAllAllocationFailures(std.testing.allocator, lineAllocationScenario, .{});
}

test "lines span chunks support reverse seek and stable scratch lifetime" {
    var source: std.ArrayList(u8) = .empty;
    defer source.deinit(std.testing.allocator);
    try source.appendNTimes(std.testing.allocator, 'x', 150);
    try source.appendSlice(std.testing.allocator, "\nshort\n");
    try source.appendNTimes(std.testing.allocator, 'y', 170);

    var value = try rope.Rope.initText(std.testing.allocator, source.items);
    defer value.deinit();
    var lines = value.lines(std.testing.allocator);
    defer lines.deinit();
    const first = (try lines.next()).?;
    try std.testing.expectEqual(@as(usize, 150), first.len);
    try std.testing.expectEqual(@as(usize, 151), lines.offset());
    try std.testing.expectEqualStrings("short", (try lines.next()).?);
    lines.seek(0);
    try std.testing.expectEqual(@as(usize, 150), (try lines.next()).?.len);

    var reverse_chunks = value.reversedChunksInRange(.{ .start = 0, .end = value.len() });
    var reverse_lines = reverse_chunks.lines(std.testing.allocator);
    defer reverse_lines.deinit();
    try std.testing.expectEqual(@as(usize, 170), (try reverse_lines.next()).?.len);
    try std.testing.expectEqualStrings("short", (try reverse_lines.next()).?);
    try std.testing.expectEqual(@as(usize, 150), (try reverse_lines.next()).?.len);
    try std.testing.expect((try reverse_lines.next()) == null);
}
