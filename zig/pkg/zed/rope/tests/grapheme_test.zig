const std = @import("std");
const grapheme = @import("rope").grapheme;
const conformance = @import("grapheme_conformance_data.zig");

fn expectBoundaries(text: []const u8, expected: []const usize) !void {
    var offset: usize = 0;
    while (offset <= text.len) : (offset += 1) {
        const wanted = for (expected) |boundary| {
            if (boundary == offset) break true;
        } else false;
        try std.testing.expectEqual(wanted, try grapheme.isBoundary(text, offset));

        var previous: usize = 0;
        var next: usize = text.len;
        for (expected) |boundary| {
            if (boundary <= offset) previous = boundary;
            if (boundary >= offset) {
                next = boundary;
                break;
            }
        }
        try std.testing.expectEqual(previous, try grapheme.previousBoundary(text, offset));
        try std.testing.expectEqual(next, try grapheme.nextBoundary(text, offset));
    }
}

test "ascii and empty boundaries" {
    try expectBoundaries("", &.{0});
    try expectBoundaries("abc", &.{ 0, 1, 2, 3 });
}

test "cr lf is one grapheme" {
    try expectBoundaries("a\r\nb", &.{ 0, 1, 3, 4 });
}

test "combining mark and emoji modifier extend" {
    try expectBoundaries("e\u{301}f", &.{ 0, 3, 4 });
    try expectBoundaries("👍🏽", &.{ 0, 8 });
}

test "regional indicators pair" {
    try expectBoundaries("🇺🇸🇨", &.{ 0, 8, 12 });
}

test "emoji zwj sequence" {
    try expectBoundaries("👩‍💻", &.{ 0, 11 });
}

test "hangul jamo sequence" {
    try expectBoundaries("각", &.{ 0, 9 });
}

test "prepend joins following grapheme" {
    try expectBoundaries("\u{600}a", &.{ 0, 3 });
}

test "gb9c indic conjunct" {
    try expectBoundaries("क्‍त", &.{ 0, 12 });
    try expectBoundaries("क्aत", &.{ 0, 6, 7, 10 });
}

test "Unicode 17 extended grapheme conformance vectors" {
    try std.testing.expectEqual(@as(usize, 768), conformance.vectors.len);
    for (conformance.vectors) |vector| {
        const text = conformance.data[vector.data_start .. vector.data_start + vector.data_len];
        const expected = conformance.boundaries[vector.boundary_start .. vector.boundary_start + vector.boundary_len];
        var expected_usize: [32]usize = undefined;
        try std.testing.expect(expected.len <= expected_usize.len);
        for (expected, 0..) |boundary, index| expected_usize[index] = boundary;
        try expectBoundaries(text, expected_usize[0..expected.len]);
    }
}

test "invalid input and offsets are errors" {
    try std.testing.expectError(error.InvalidUtf8, grapheme.isBoundary("\xff", 0));
    try std.testing.expectError(error.OffsetOutOfBounds, grapheme.isBoundary("a", 2));
}
