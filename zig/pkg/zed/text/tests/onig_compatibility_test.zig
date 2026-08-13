const std = @import("std");
const text = @import("text");
const onig = @import("oniguruma");

const OnigAdapter = struct {
    regex: onig.Regex,

    fn compile(pattern: []const u8) !OnigAdapter {
        return .{ .regex = try onig.Regex.init(pattern, .{}, onig.Encoding.utf8, onig.Syntax.oniguruma, null) };
    }
    fn deinit(self: *OnigAdapter) void {
        self.regex.deinit();
        self.* = undefined;
    }
    fn find(raw: *anyopaque, bytes: []const u8, start: usize) ?text.RegexMatch {
        const self: *OnigAdapter = @ptrCast(@alignCast(raw));
        var region: onig.Region = .{};
        defer region.deinit();
        _ = self.regex.searchAdvanced(bytes, start, bytes.len, &region, .{}) catch return null;
        if (region.count() == 0) return null;
        return .{ .start = @intCast(region.starts()[0]), .end = @intCast(region.ends()[0]) };
    }
    fn matcher(self: *OnigAdapter) text.RegexMatcher {
        return .{ .context = self, .find_fn = find };
    }
};

test "Oniguruma adapter supports Unicode classes and lookaround" {
    const allocator = std.testing.allocator;
    const id = try text.BufferId.new(84);
    var buffer = try text.Buffer.init(allocator, text.ReplicaId.new(8), id, "αβ 123 cat scatter");
    defer buffer.deinit();

    var unicode = try OnigAdapter.compile("\\p{Greek}+");
    defer unicode.deinit();
    const greek = (try buffer.snapshot().findRegex(allocator, unicode.matcher(), 0)).?;
    const greek_text = try buffer.snapshot().textForRange(allocator, greek.start, greek.end);
    defer allocator.free(greek_text);
    try std.testing.expectEqualStrings("αβ", greek_text);

    var lookaround = try OnigAdapter.compile("(?<!s)cat(?!t)");
    defer lookaround.deinit();
    const found = (try buffer.snapshot().findRegex(allocator, lookaround.matcher(), 0)).?;
    const matched = try buffer.snapshot().textForRange(allocator, found.start, found.end);
    defer allocator.free(matched);
    try std.testing.expectEqualStrings("cat", matched);
}
