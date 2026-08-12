const std = @import("std");
const text = @import("text");
const LineEnding = text.LineEnding;
const LineIndent = text.LineIndent;

test "line ending detection uses first LF in UTF-8-bounded prefix" {
    try std.testing.expectEqual(LineEnding.windows, LineEnding.detect("first\r\nsecond\n"));
    try std.testing.expectEqual(LineEnding.unix, LineEnding.detect("first\nsecond\r\n"));
    try std.testing.expectEqual(LineEnding.unix, LineEnding.detect("🍐✅\n" ** 200));
    try std.testing.expectEqual(LineEnding.windows, LineEnding.detect("🍐✅\r\n" ** 200));

    var beyond: [1002]u8 = @splat('x');
    beyond[1000] = '\r';
    beyond[1001] = '\n';
    try std.testing.expectEqual(LineEnding.default(), LineEnding.detect(&beyond));
    try std.testing.expectEqualStrings("\n", LineEnding.unix.asStr());
    try std.testing.expectEqualStrings("CRLF", LineEnding.windows.label());
}

test "line ending normalization handles CRLF and bare CR in one pass" {
    const vectors = [_]struct { input: []const u8, expected: []const u8 }{
        .{ .input = "", .expected = "" },
        .{ .input = "plain\n", .expected = "plain\n" },
        .{ .input = "\r", .expected = "\n" },
        .{ .input = "\r\n", .expected = "\n" },
        .{ .input = "\r\r\n", .expected = "\n\n" },
        .{ .input = "a\r\nb\rc\n", .expected = "a\nb\nc\n" },
    };

    for (vectors) |vector| {
        var storage: [32]u8 = undefined;
        @memcpy(storage[0..vector.input.len], vector.input);
        const normalized = LineEnding.normalizeInPlace(storage[0..vector.input.len]);
        try std.testing.expectEqualStrings(vector.expected, normalized);

        const owned = try LineEnding.normalizeOwned(std.testing.allocator, vector.input);
        defer std.testing.allocator.free(owned);
        try std.testing.expectEqualStrings(vector.expected, owned);
    }
}

test "Cow-style normalization borrows unchanged input and owns replacements" {
    const plain = "a\nb";
    var borrowed = try LineEnding.normalize(std.testing.allocator, plain);
    defer borrowed.deinit(std.testing.allocator);
    try std.testing.expect(borrowed == .borrowed);
    try std.testing.expectEqual(@intFromPtr(plain.ptr), @intFromPtr(borrowed.slice().ptr));

    var owned = try LineEnding.normalize(std.testing.allocator, "a\r\nb");
    defer owned.deinit(std.testing.allocator);
    try std.testing.expect(owned == .owned);
    try std.testing.expectEqualStrings("a\nb", owned.slice());
}

fn normalizationAllocationScenario(allocator: std.mem.Allocator) !void {
    var normalized = try LineEnding.normalize(allocator, "one\r\ntwo\rthree");
    defer normalized.deinit(allocator);
    try std.testing.expectEqualStrings("one\ntwo\nthree", normalized.slice());
}

test "normalization cleans up every induced allocation failure" {
    try std.testing.checkAllAllocationFailures(std.testing.allocator, normalizationAllocationScenario, .{});
}

test "indentation parsing matches Rust leading whitespace semantics" {
    try std.testing.expectEqual(LineIndent{ .tabs = 2, .spaces = 3, .line_blank = false }, LineIndent.parse("\t \t  value"));
    try std.testing.expectEqual(LineIndent{ .tabs = 1, .spaces = 2, .line_blank = true }, LineIndent.parse("\t  \nignored"));
    try std.testing.expectEqual(LineIndent{ .tabs = 0, .spaces = 0, .line_blank = true }, LineIndent.parse("\n"));
    try std.testing.expectEqual(LineIndent{ .tabs = 0, .spaces = 0, .line_blank = false }, LineIndent.parse("é"));
    try std.testing.expectEqual(LineIndent{ .tabs = 0, .spaces = 0, .line_blank = false }, LineIndent.parse("\r\n"));
}

test "indent lengths expand every tab by tab size" {
    const indent = LineIndent.parse(" \t  \tX");
    try std.testing.expectEqual(@as(u32, 5), indent.rawLen());
    try std.testing.expectEqual(@as(u32, 11), indent.len(4));
    try std.testing.expect(!indent.isLineBlank());
    try std.testing.expect(LineIndent.parse("").isLineEmpty());
    try std.testing.expectEqual(LineIndent.onlySpaces(3), LineIndent{ .tabs = 0, .spaces = 3, .line_blank = true });
    try std.testing.expectEqual(LineIndent.onlyTabs(2), LineIndent{ .tabs = 2, .spaces = 0, .line_blank = true });
}

test "indent parsing and expansion are deterministic over mixed prefixes" {
    const bytes = [_]u8{ ' ', '\t' };
    for (0..256) |mask| {
        var prefix: [8]u8 = undefined;
        var expected_tabs: u32 = 0;
        var expected_spaces: u32 = 0;
        for (0..8) |index| {
            prefix[index] = bytes[(mask >> @intCast(index)) & 1];
            if (prefix[index] == '\t') expected_tabs += 1 else expected_spaces += 1;
        }
        const indent = LineIndent.parse(&prefix);
        try std.testing.expectEqual(expected_tabs, indent.tabs);
        try std.testing.expectEqual(expected_spaces, indent.spaces);
        try std.testing.expectEqual(expected_tabs * 4 + expected_spaces, indent.len(4));
        try std.testing.expect(indent.isLineBlank());
    }
}
