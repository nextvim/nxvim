const std = @import("std");
const summary_mod = @import("rope");
const Point = summary_mod.Point;
const PointUtf16 = summary_mod.PointUtf16;
const OffsetUtf16 = summary_mod.OffsetUtf16;
const TextSummary = summary_mod.TextSummary;

test "TextSummary parses UTF-8 and UTF-16 metrics" {
    const summary = try TextSummary.parse("ab🙂\nβc\n終");
    try std.testing.expectEqual(@as(usize, 14), summary.len);
    try std.testing.expectEqual(@as(usize, 8), summary.chars);
    try std.testing.expectEqual(OffsetUtf16.new(9), summary.len_utf16);
    try std.testing.expectEqual(Point.new(2, 3), summary.lines);
    try std.testing.expectEqual(PointUtf16.new(2, 1), summary.linesUtf16());
    try std.testing.expectEqual(@as(u32, 3), summary.first_line_chars);
    try std.testing.expectEqual(@as(u32, 1), summary.last_line_chars);
    try std.testing.expectEqual(@as(u32, 1), summary.last_line_len_utf16);
    try std.testing.expectEqual(@as(u32, 0), summary.longest_row);
    try std.testing.expectEqual(@as(u32, 3), summary.longest_row_chars);
}

test "newline and addNewline agree with parsing" {
    try std.testing.expectEqual(try TextSummary.parse("\n"), TextSummary.newline());
    var summary = try TextSummary.parse("a🙂");
    summary.addNewline();
    try std.testing.expectEqual(try TextSummary.parse("a🙂\n"), summary);
}

test "invalid UTF-8 is rejected without allocation" {
    try std.testing.expectError(error.InvalidUtf8, TextSummary.parse(&.{ 0xc3, 0x28 }));
}

test "summary composition equals reparsing at every codepoint boundary" {
    const fixtures = [_][]const u8{
        "",       "ascii", "\n", "a\n", "\na",
        "a\n\nβ",
        "é🙂\nβc\n終",
        "🙂🙂🙂",
        "x\r\ny",
    };
    for (fixtures) |text| {
        const whole = try TextSummary.parse(text);
        var split: usize = 0;
        while (split <= text.len) : (split += 1) {
            if (!std.unicode.utf8ValidateSlice(text[0..split])) continue;
            const left = try TextSummary.parse(text[0..split]);
            const right = try TextSummary.parse(text[split..]);
            try std.testing.expectEqual(whole, left.add(right));
        }
    }
}

test "summary composition is associative for deterministic fragments" {
    const parts = [_][]const u8{ "", "a", "🙂", "\n", "β\n", "終x" };
    for (parts) |a| for (parts) |b| for (parts) |c| {
        const sa = try TextSummary.parse(a);
        const sb = try TextSummary.parse(b);
        const sc = try TextSummary.parse(c);
        try std.testing.expectEqual(sa.add(sb).add(sc), sa.add(sb.add(sc)));
    };
}

test "TextDimension and DimensionPair preserve paired metrics" {
    const DimensionPair = summary_mod.DimensionPair(Point, OffsetUtf16);
    const first = DimensionPair.fromTextSummary(try TextSummary.parse("ab\n🙂"));
    const second = DimensionPair.fromTextSummary(try TextSummary.parse("x"));
    var combined = first;
    combined.addAssign(second);
    try std.testing.expectEqual(Point.new(1, 5), combined.key);
    try std.testing.expectEqual(OffsetUtf16.new(6), combined.value.?);

    const same_key = DimensionPair{ .key = combined.key, .value = null };
    try std.testing.expect(combined.eql(same_key));
    try std.testing.expectEqual(Point.new(0, 1), combined.sub(first).key);
}
