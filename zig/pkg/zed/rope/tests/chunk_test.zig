const std = @import("std");
const chunk = @import("rope").chunk;

fn expectPoint(expected_row: u32, expected_column: u32, actual: chunk.Point) !void {
    try std.testing.expectEqual(expected_row, actual.row);
    try std.testing.expectEqual(expected_column, actual.column);
}

fn verify(text: []const u8, slice: chunk.ChunkSlice) !void {
    try std.testing.expectEqualStrings(text, slice.text);
    try slice.checkInvariants();

    var expected_chars: chunk.Bitmap = 0;
    var expected_utf16: usize = 0;
    var expected_newlines: chunk.Bitmap = 0;
    var expected_tabs: chunk.Bitmap = 0;
    var offset: usize = 0;
    var row: u32 = 0;
    var byte_column: u32 = 0;
    var utf16_column: u32 = 0;
    var tab_char_offset: usize = 0;
    var expected_tab_count: usize = 0;

    while (offset < text.len) {
        expected_chars |= @as(chunk.Bitmap, 1) << @intCast(offset);
        const sequence_len: usize = @intCast(try std.unicode.utf8ByteSequenceLength(text[offset]));
        const codepoint = try std.unicode.utf8Decode(text[offset .. offset + sequence_len]);
        const units: usize = if (codepoint >= 0x10000) 2 else 1;
        expected_utf16 += units;
        if (text[offset] == '\n') expected_newlines |= @as(chunk.Bitmap, 1) << @intCast(offset);
        if (text[offset] == '\t') expected_tabs |= @as(chunk.Bitmap, 1) << @intCast(offset);

        const point = try slice.offsetToPoint(offset);
        try expectPoint(row, byte_column, point);
        try std.testing.expectEqual(offset, try slice.pointToOffset(point));
        try std.testing.expectEqual(expected_utf16 - units, (try slice.offsetToOffsetUtf16(offset)).value);
        try std.testing.expectEqual(offset, try slice.offsetUtf16ToOffset(.{ .value = expected_utf16 - units }));
        const point16 = try slice.offsetToPointUtf16(offset);
        try std.testing.expectEqual(row, point16.row);
        try std.testing.expectEqual(utf16_column, point16.column);
        try std.testing.expectEqual(offset, try slice.pointUtf16ToOffset(point16, false));

        if (text[offset] == '\t') {
            var tabs = slice.tabs();
            var index: usize = 0;
            while (tabs.next()) |position| : (index += 1) {
                if (index == expected_tab_count) {
                    try std.testing.expectEqual(offset, position.byte_offset);
                    try std.testing.expectEqual(tab_char_offset, position.char_offset);
                    break;
                }
            }
            expected_tab_count += 1;
        }

        tab_char_offset += 1;
        if (text[offset] == '\n') {
            row += 1;
            byte_column = 0;
            utf16_column = 0;
        } else {
            byte_column += @intCast(sequence_len);
            utf16_column += @intCast(units);
        }
        offset += sequence_len;
    }

    try std.testing.expectEqual(expected_chars, slice.chars());
    try std.testing.expectEqual(expected_newlines, slice.newlines());
    try std.testing.expectEqual(expected_tabs, slice.tabsBitmap());
    try std.testing.expectEqual(expected_utf16, slice.lenUtf16().value);
    try expectPoint(row, byte_column, slice.lines());
    try std.testing.expectEqual(text.len, try slice.pointToOffset(slice.lines()));
    try std.testing.expectEqual(text.len, try slice.offsetUtf16ToOffset(.{ .value = expected_utf16 }));

    const summary = slice.textSummary();
    try std.testing.expectEqual(text.len, summary.len);
    try std.testing.expectEqual(@popCount(expected_chars), summary.chars);
    try std.testing.expectEqual(expected_utf16, summary.len_utf16.value);
    try std.testing.expectEqual(slice.firstLineChars(), summary.first_line_chars);
    try std.testing.expectEqual(slice.lastLineChars(), summary.last_line_chars);
    try std.testing.expectEqual(slice.lastLineLenUtf16(), summary.last_line_len_utf16);
}

fn boundaries(text: []const u8, output: *[129]usize) []const usize {
    var count: usize = 0;
    var offset: usize = 0;
    while (offset < text.len) {
        output[count] = offset;
        count += 1;
        offset += @intCast(std.unicode.utf8ByteSequenceLength(text[offset]) catch unreachable);
    }
    output[count] = text.len;
    count += 1;
    return output[0..count];
}

test "construction validates UTF-8 and capacity and builds production masks" {
    const value = try chunk.Chunk.init("a😀\n\tb");
    try verify("a😀\n\tb", value.asSlice());
    try std.testing.expectEqual(@as(usize, 128), chunk.max_base);
    try std.testing.expectEqual(@as(usize, 128), @bitSizeOf(chunk.Bitmap));
    try std.testing.expectError(error.InvalidUtf8, chunk.Chunk.init("\xff"));

    var oversized: [129]u8 = @splat('x');
    try std.testing.expectError(error.ChunkTooLong, chunk.Chunk.init(&oversized));
}

test "append prepend split and slice preserve semantics" {
    var value = try chunk.Chunk.init("中\n");
    const suffix = try chunk.Chunk.init("😀x");
    try value.append(suffix.asSlice());
    try verify("中\n😀x", value.asSlice());

    const prefix = try chunk.Chunk.init("a\t");
    try value.prepend(prefix.asSlice());
    try verify("a\t中\n😀x", value.asSlice());

    const split = try value.splitAt(5);
    try verify("a\t中", split.left);
    try verify("\n😀x", split.right);
    try verify("中\n😀", try value.slice(.{ .start = 2, .end = 10 }));
    try std.testing.expectError(error.NotCodepointBoundary, value.slice(.{ .start = 3, .end = 5 }));
    try std.testing.expectError(error.InvalidRange, value.slice(.{ .start = 5, .end = 2 }));
}

test "row ranges conversions UTF-16 clipping and grapheme clipping" {
    const value = try chunk.Chunk.init("a😀e\u{301}\n👩‍💻z");
    const slice = value.asSlice();

    const first = try slice.offsetRangeForRow(0);
    try std.testing.expectEqualStrings("a😀e\u{301}", slice.text[first.start..first.end]);
    const second = try slice.offsetRangeForRow(1);
    try std.testing.expectEqualStrings("👩‍💻z", slice.text[second.start..second.end]);
    try std.testing.expectEqual(second.end, try slice.pointUtf16ToOffset(.{ .row = 1, .column = 99 }, true));
    try std.testing.expectError(error.PointOutOfBounds, slice.pointUtf16ToOffset(.{ .row = 1, .column = 99 }, false));

    const emoji_start16 = try slice.offsetToOffsetUtf16(1);
    try std.testing.expectEqual(emoji_start16.value, slice.clipOffsetUtf16(.{ .value = emoji_start16.value + 1 }, .left).value);
    try std.testing.expectEqual(emoji_start16.value + 2, slice.clipOffsetUtf16(.{ .value = emoji_start16.value + 1 }, .right).value);

    try expectPoint(0, 5, slice.clipPoint(.{ .row = 0, .column = 6 }, .left));
    try expectPoint(0, 8, slice.clipPoint(.{ .row = 0, .column = 6 }, .right));
    try expectPoint(1, 0, slice.clipPoint(.{ .row = 1, .column = 5 }, .left));
    try expectPoint(1, 11, slice.clipPoint(.{ .row = 1, .column = 5 }, .right));
    try expectPoint(slice.lines().row, slice.lines().column, slice.clipPoint(.{ .row = 99, .column = 99 }, .right));
}

test "longest row summaries and tabs iterator" {
    const value = try chunk.Chunk.init("a\té\nxyz\n😀");
    const slice = value.asSlice();
    const longest = slice.longestRow();
    try std.testing.expectEqual(@as(u32, 0), longest.row);
    try std.testing.expectEqual(@as(u32, 3), longest.chars);
    try std.testing.expectEqual(@as(usize, 9), longest.total_chars);

    var tabs = slice.tabs();
    const tab = tabs.next().?;
    try std.testing.expectEqual(@as(usize, 1), tab.byte_offset);
    try std.testing.expectEqual(@as(usize, 1), tab.char_offset);
    try std.testing.expect(tabs.next() == null);
}

test "deterministic randomized operations match flat UTF-8 model" {
    var prng = std.Random.DefaultPrng.init(0x6e78_7669_6d_726f_70);
    const random = prng.random();
    const atoms = [_][]const u8{ "a", "Z", "\n", "\t", "é", "中", "😀", "e\u{301}" };

    for (0..500) |_| {
        var flat: [128]u8 = undefined;
        var flat_len: usize = 0;
        while (flat_len < 128) {
            const atom = atoms[random.uintLessThan(usize, atoms.len)];
            if (flat_len + atom.len > 128 or random.uintLessThan(u8, 7) == 0) break;
            @memcpy(flat[flat_len .. flat_len + atom.len], atom);
            flat_len += atom.len;
        }
        const text = flat[0..flat_len];
        const value = try chunk.Chunk.init(text);
        try verify(text, value.asSlice());

        var storage: [129]usize = undefined;
        const points = boundaries(text, &storage);
        const start_index = random.uintLessThan(usize, points.len);
        const end_index = start_index + random.uintLessThan(usize, points.len - start_index);
        const start = points[start_index];
        const end = points[end_index];
        try verify(text[start..end], try value.slice(.{ .start = start, .end = end }));

        const split_index = random.uintLessThan(usize, points.len);
        const split_offset = points[split_index];
        const split = try value.splitAt(split_offset);
        try verify(text[0..split_offset], split.left);
        try verify(text[split_offset..], split.right);

        var combined = try split.left.toChunk();
        try combined.append(split.right);
        try verify(text, combined.asSlice());

        var reversed = try split.right.toChunk();
        try reversed.prepend(split.left);
        try verify(text, reversed.asSlice());
    }
}
