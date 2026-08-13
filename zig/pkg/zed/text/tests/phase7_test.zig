const std = @import("std");
const text = @import("text");

fn expectText(buffer: *const text.Buffer, expected: []const u8) !void {
    const actual = try buffer.snapshot().text(std.testing.allocator);
    defer std.testing.allocator.free(actual);
    try std.testing.expectEqualStrings(expected, actual);
    try buffer.validate();
}

test "owned replicated undo roundtrips and snapshots stay isolated" {
    const allocator = std.testing.allocator;
    const id = try text.BufferId.new(70);
    var left = try text.Buffer.init(allocator, text.ReplicaId.new(8), id, "abc");
    defer left.deinit();
    var right = try text.Buffer.init(allocator, text.ReplicaId.new(9), id, "abc");
    defer right.deinit();

    var edit = try left.edit(&.{.{ .start = 1, .end = 2, .new_text = "X" }});
    defer edit.deinit();
    try right.applyOps(&.{edit});
    var old = try right.cloneSnapshot();
    defer old.deinit();

    var undone = (try left.undo()).?;
    defer undone[1].deinit();
    var clone = try undone[1].clone(allocator);
    defer clone.deinit();
    try right.applyOps(&.{ clone, clone });
    try expectText(&left, "abc");
    try expectText(&right, "abc");
    const old_text = try old.text(allocator);
    defer allocator.free(old_text);
    try std.testing.expectEqualStrings("aXc", old_text);

    var redone = (try left.redo()).?;
    defer redone[1].deinit();
    try right.applyOps(&.{redone[1]});
    try expectText(&left, "aXc");
    try expectText(&right, "aXc");
}

test "nested transactions and deterministic grouping" {
    const allocator = std.testing.allocator;
    const id = try text.BufferId.new(71);
    var buffer = try text.Buffer.init(allocator, text.ReplicaId.new(8), id, "");
    defer buffer.deinit();
    buffer.setGroupInterval(10);

    const outer = (try buffer.startTransactionAt(100)).?;
    try std.testing.expect((try buffer.startTransactionAt(101)) == null);
    var a = try buffer.edit(&.{.{ .start = 0, .end = 0, .new_text = "a" }});
    defer a.deinit();
    try std.testing.expect(buffer.endTransactionAt(102) == null);
    try std.testing.expect(buffer.endTransactionAt(103).?.eql(outer));

    _ = try buffer.startTransactionAt(108);
    var b = try buffer.edit(&.{.{ .start = 1, .end = 1, .new_text = "b" }});
    defer b.deinit();
    try std.testing.expect(buffer.endTransactionAt(109).?.eql(outer));
    try std.testing.expect(buffer.getTransaction(outer).?.edit_ids.items.len == 2);

    var undone = (try buffer.undo()).?;
    defer undone[1].deinit();
    try expectText(&buffer, "");
}

test "transactions merge suppress grouping and forget" {
    const allocator = std.testing.allocator;
    const id = try text.BufferId.new(73);
    var buffer = try text.Buffer.init(allocator, text.ReplicaId.new(8), id, "");
    defer buffer.deinit();
    buffer.setGroupInterval(0);

    var first = try buffer.edit(&.{.{ .start = 0, .end = 0, .new_text = "a" }});
    defer first.deinit();
    const first_id = buffer.peekUndoStack().?.transactionId();
    buffer.suppressGrouping(first_id);
    var second = try buffer.edit(&.{.{ .start = 1, .end = 1, .new_text = "b" }});
    defer second.deinit();
    const second_id = buffer.peekUndoStack().?.transactionId();
    try buffer.mergeTransactions(second_id, first_id);
    try std.testing.expect(buffer.getTransaction(first_id).?.edit_ids.items.len == 2);
    try std.testing.expect(buffer.getTransaction(second_id) == null);
    try std.testing.expect(buffer.forgetTransaction(first_id));
    try std.testing.expect(buffer.peekUndoStack() == null);
}

test "undo after concurrent remote edit preserves the remote insertion" {
    const allocator = std.testing.allocator;
    const id = try text.BufferId.new(74);
    var left = try text.Buffer.init(allocator, text.ReplicaId.new(8), id, "root");
    defer left.deinit();
    var right = try text.Buffer.init(allocator, text.ReplicaId.new(9), id, "root");
    defer right.deinit();

    var left_edit = try left.edit(&.{.{ .start = 4, .end = 4, .new_text = "L" }});
    defer left_edit.deinit();
    var right_edit = try right.edit(&.{.{ .start = 4, .end = 4, .new_text = "R" }});
    defer right_edit.deinit();
    try left.applyOps(&.{right_edit});
    try right.applyOps(&.{left_edit});
    try expectText(&left, "rootRL");
    try expectText(&right, "rootRL");

    var undone = (try left.undo()).?;
    defer undone[1].deinit();
    try right.applyOps(&.{ undone[1], undone[1] });
    try expectText(&left, "rootR");
    try expectText(&right, "rootR");
}

test "undo waits for its causal edit and converges under delayed delivery" {
    const allocator = std.testing.allocator;
    const id = try text.BufferId.new(72);
    var source = try text.Buffer.init(allocator, text.ReplicaId.new(8), id, "x");
    defer source.deinit();
    var target = try text.Buffer.init(allocator, text.ReplicaId.new(9), id, "x");
    defer target.deinit();

    var edit = try source.edit(&.{.{ .start = 1, .end = 1, .new_text = "y" }});
    defer edit.deinit();
    var undone = (try source.undo()).?;
    defer undone[1].deinit();
    try target.applyOps(&.{ undone[1], undone[1] });
    try std.testing.expectEqual(@as(usize, 1), target.deferredOperationCount());
    try target.applyOps(&.{edit});
    try std.testing.expectEqual(@as(usize, 0), target.deferredOperationCount());
    try expectText(&target, "x");
}
