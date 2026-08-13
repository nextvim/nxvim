const std = @import("std");
const text = @import("text");

test "buffer detects and normalizes line endings" {
    const allocator = std.testing.allocator;
    const id = try text.BufferId.new(42);
    var buffer = try text.Buffer.init(allocator, text.clock.ReplicaId.new(8), id, "one\r\ntwo\rthree");
    defer buffer.deinit();
    try buffer.validate();
    try std.testing.expectEqual(text.LineEnding.windows, buffer.snapshot().line_ending);
    const contents = try buffer.snapshot().text(allocator);
    defer allocator.free(contents);
    try std.testing.expectEqualStrings("one\ntwo\nthree", contents);
}

test "snapshots and branches retain persistent text" {
    const allocator = std.testing.allocator;
    const id = try text.BufferId.new(7);
    var buffer = try text.Buffer.init(allocator, text.clock.ReplicaId.new(9), id, "a🙂\nsecond");
    defer buffer.deinit();
    var snapshot = try buffer.cloneSnapshot();
    defer snapshot.deinit();
    var branch = try buffer.branch();
    defer branch.deinit();
    try snapshot.validate();
    try branch.validate();
    try std.testing.expectEqual(buffer.snapshot().len(), snapshot.len());
    try std.testing.expectEqual(text.clock.ReplicaId.LOCAL_BRANCH, branch.snapshot().replica_id);
    try std.testing.expectEqual(@as(usize, 6), snapshot.pointToOffset(.new(1, 0)));
    try std.testing.expectEqual(text.Point.new(1, 0), snapshot.offsetToPoint(6));
}

test "local batch edits preserve snapshots and anchors outside deleted text" {
    const allocator = std.testing.allocator;
    const id = try text.BufferId.new(12);
    var buffer = try text.Buffer.init(allocator, text.clock.ReplicaId.new(8), id, "alpha beta gamma");
    defer buffer.deinit();
    var updates = try buffer.subscribe();
    defer updates.deinit();
    var before = try buffer.cloneSnapshot();
    defer before.deinit();
    const alpha_anchor = before.anchorAfter(2);
    const deleted_anchor = before.anchorAfter(8);
    const gamma_anchor = before.anchorAfter(13);

    var operation = try buffer.edit(&.{
        .{ .start = 6, .end = 10, .new_text = "B\r\n" },
        .{ .start = 16, .end = 16, .new_text = "!" },
    });
    defer operation.deinit();
    try buffer.validate();
    const current = try buffer.snapshot().text(allocator);
    defer allocator.free(current);
    try std.testing.expectEqualStrings("alpha B\n gamma!", current);
    const old = try before.text(allocator);
    defer allocator.free(old);
    try std.testing.expectEqualStrings("alpha beta gamma", old);
    try std.testing.expectEqual(@as(?usize, 2), buffer.snapshot().offsetForAnchor(alpha_anchor));
    try std.testing.expectEqual(@as(?usize, 8), buffer.snapshot().offsetForAnchor(deleted_anchor));
    try std.testing.expect(!buffer.snapshot().isAnchorValid(deleted_anchor));
    try std.testing.expectEqual(@as(?usize, 11), buffer.snapshot().offsetForAnchor(gamma_anchor));
    try std.testing.expectEqual(@as(usize, 2), operation.edit.ranges.items.len);
    try std.testing.expectEqualStrings("B\n", operation.edit.new_text.items[0]);
    try std.testing.expectEqual(@as(usize, 6), operation.edit.ranges.items[0].start.value);
    try std.testing.expectEqual(@as(usize, 10), operation.edit.ranges.items[0].end.value);
    var patch = try updates.consume();
    defer patch.deinit();
    try std.testing.expectEqual(@as(usize, 2), patch.edits().len);
    try std.testing.expectEqual(@as(usize, 6), patch.edits()[0].old.start);
    try std.testing.expectEqual(@as(usize, 8), patch.edits()[0].new.end);
}

test "generated local edits match a flat model and isolate every snapshot" {
    const allocator = std.testing.allocator;
    const id = try text.BufferId.new(14);
    var buffer = try text.Buffer.init(allocator, text.clock.ReplicaId.new(8), id, "seed");
    defer buffer.deinit();
    var expected = try allocator.dupe(u8, "seed");
    defer allocator.free(expected);

    var step: usize = 0;
    while (step < 48) : (step += 1) {
        const start = (step * 7) % (expected.len + 1);
        const remove_len = @min(step % 3, expected.len - start);
        const replacement = if (step % 4 == 0) "XY" else if (step % 4 == 1) "" else "z";
        var retained = try buffer.cloneSnapshot();
        defer retained.deinit();
        const retained_text = try retained.text(allocator);
        defer allocator.free(retained_text);
        var next = try allocator.alloc(u8, start + replacement.len + expected.len - start - remove_len);
        @memcpy(next[0..start], expected[0..start]);
        @memcpy(next[start .. start + replacement.len], replacement);
        @memcpy(next[start + replacement.len ..], expected[start + remove_len ..]);
        var operation = try buffer.edit(&.{.{ .start = start, .end = start + remove_len, .new_text = replacement }});
        operation.deinit();
        allocator.free(expected);
        expected = next;
        try buffer.validate();
        const actual = try buffer.snapshot().text(allocator);
        defer allocator.free(actual);
        try std.testing.expectEqualStrings(expected, actual);
        const retained_again = try retained.text(allocator);
        defer allocator.free(retained_again);
        try std.testing.expectEqualStrings(retained_text, retained_again);
    }
}

test "invalid local edits leave the buffer unchanged" {
    const allocator = std.testing.allocator;
    const id = try text.BufferId.new(13);
    var buffer = try text.Buffer.init(allocator, text.clock.ReplicaId.new(8), id, "a🙂b");
    defer buffer.deinit();
    try std.testing.expectError(error.InvalidUtf8Boundary, buffer.edit(&.{.{ .start = 2, .end = 3, .new_text = "x" }}));
    const current = try buffer.snapshot().text(allocator);
    defer allocator.free(current);
    try std.testing.expectEqualStrings("a🙂b", current);
    try buffer.validate();
}

test "queries clip UTF-8 and anchors resolve" {
    const allocator = std.testing.allocator;
    const id = try text.BufferId.new(11);
    var buffer = try text.Buffer.init(allocator, text.clock.ReplicaId.new(8), id, "a🙂b");
    defer buffer.deinit();
    const snapshot = buffer.snapshot();
    try std.testing.expectEqual(@as(usize, 1), snapshot.clipOffset(2, .left));
    try std.testing.expectEqual(@as(usize, 5), snapshot.clipOffset(2, .right));
    const anchor = snapshot.anchorAfter(1);
    try std.testing.expectEqual(@as(?usize, 1), snapshot.offsetForAnchor(anchor));
    try std.testing.expect(snapshot.isAnchorValid(anchor));
    try std.testing.expectEqual(@as(?usize, null), snapshot.offsetForAnchor(text.Anchor.init(anchor.timestamp, anchor.offset, anchor.bias, try text.BufferId.new(99))));
    const middle = try snapshot.textForRange(allocator, 1, 5);
    defer allocator.free(middle);
    try std.testing.expectEqualStrings("🙂", middle);
}
