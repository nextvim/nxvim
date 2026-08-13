const std = @import("std");
const text = @import("text");

const LiteralMatcher = struct {
    needle: []const u8,
    fn find(raw: *anyopaque, haystack: []const u8, start: usize) ?text.RegexMatch {
        const self: *LiteralMatcher = @ptrCast(@alignCast(raw));
        const relative = std.mem.indexOf(u8, haystack[start..], self.needle) orelse return null;
        const match_start = start + relative;
        return .{ .start = match_start, .end = match_start + self.needle.len };
    }
    fn matcher(self: *LiteralMatcher) text.RegexMatcher {
        return .{ .context = self, .find_fn = find };
    }
};

test "version edit and anchor waiters complete exactly after causal observation" {
    const allocator = std.testing.allocator;
    const id = try text.BufferId.new(80);
    var source = try text.Buffer.init(allocator, text.ReplicaId.new(8), id, "a");
    defer source.deinit();
    var target = try text.Buffer.init(allocator, text.ReplicaId.new(9), id, "a");
    defer target.deinit();

    var operation = try source.edit(&.{.{ .start = 1, .end = 1, .new_text = "b" }});
    defer operation.deinit();
    var version_waiter = try target.waitForVersion(&source.snapshot().version);
    defer version_waiter.deinit();
    var edit_waiter = try target.waitForEdits(&.{operation.timestamp()});
    defer edit_waiter.deinit();
    const anchor = source.snapshot().anchorAfter(1);
    var anchor_waiter = try target.waitForAnchors(&.{anchor});
    defer anchor_waiter.deinit();
    try std.testing.expect(!version_waiter.isReady());
    try std.testing.expect(!edit_waiter.isReady());
    try target.applyOps(&.{operation});
    try std.testing.expect(version_waiter.isReady());
    try std.testing.expect(edit_waiter.isReady());
    try std.testing.expect(anchor_waiter.isReady());
    try target.applyOps(&.{operation});
    try std.testing.expect(version_waiter.isReady());
}

test "waiter cancellation drop and buffer cancellation are safe" {
    const allocator = std.testing.allocator;
    const id = try text.BufferId.new(81);
    var source = try text.Buffer.init(allocator, text.ReplicaId.new(8), id, "");
    defer source.deinit();
    var operation = try source.edit(&.{.{ .start = 0, .end = 0, .new_text = "x" }});
    defer operation.deinit();
    var target = try text.Buffer.init(allocator, text.ReplicaId.new(9), id, "");
    var dropped = try target.waitForEdits(&.{operation.timestamp()});
    dropped.deinit();
    var cancelled = try target.waitForEdits(&.{operation.timestamp()});
    target.giveUpWaiting();
    try std.testing.expect(cancelled.isCancelled());
    target.deinit();
    cancelled.deinit();
}

test "engine-neutral regex and remaining query surfaces" {
    const allocator = std.testing.allocator;
    const id = try text.BufferId.new(82);
    var buffer = try text.Buffer.init(allocator, text.ReplicaId.new(8), id, "  one\n\ttwo one\n");
    defer buffer.deinit();
    var literal = LiteralMatcher{ .needle = "one" };
    const first = (try buffer.snapshot().findRegex(allocator, literal.matcher(), 0)).?;
    try std.testing.expectEqual(@as(usize, 2), first.start);
    var all = try buffer.snapshot().findAllRegex(allocator, literal.matcher(), 0);
    defer all.deinit(allocator);
    try std.testing.expectEqual(@as(usize, 2), all.items.len);
    try std.testing.expect(try buffer.snapshot().containsStrAt(allocator, 7, "two"));
    const indent0 = try buffer.snapshot().lineIndent(allocator, 0);
    const indent1 = try buffer.snapshot().lineIndent(allocator, 1);
    try std.testing.expectEqual(@as(u32, 2), indent0.spaces);
    try std.testing.expectEqual(@as(u32, 1), indent1.tabs);
    var old_version = text.clock.Global.init(allocator);
    defer old_version.deinit();
    try std.testing.expect(buffer.snapshot().hasEditsSince(&old_version));
}

test "immediate consumer surface requires no private access" {
    const allocator = std.testing.allocator;
    const id = try text.BufferId.new(83);
    var buffer = try text.Buffer.init(allocator, text.ReplicaId.new(8), id, "consumer");
    defer buffer.deinit();
    var snapshot = try buffer.cloneSnapshot();
    defer snapshot.deinit();
    _ = snapshot.maxPoint();
    _ = snapshot.anchorRangeInside(0, snapshot.len());
    var subscription = try buffer.subscribe();
    defer subscription.deinit();
    var waiter = try buffer.waitForVersion(&snapshot.version);
    defer waiter.deinit();
    try std.testing.expect(waiter.isReady());
}
