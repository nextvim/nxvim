const std = @import("std");
const text = @import("text");

test "phase-0 public declarations compile" {
    try std.testing.expectEqualStrings("0.16.0", text.baseline.zig);
    try std.testing.expectEqualStrings(
        "7a9ce83c781e725cb45940a8772527a991d4f9a4",
        text.baseline.zed_revision,
    );
    try std.testing.expectEqual(@as(u16, 1), text.baseline.trace_version);

    _ = text.Buffer;
    _ = text.BufferSnapshot;
    _ = text.EditedBufferSnapshot;
    _ = text.BufferId;
    _ = text.ReplicaId;
    _ = text.TransactionId;
    _ = text.Transaction;
    _ = text.HistoryEntry;
    _ = text.Operation;
    _ = text.EditOperation;
    _ = text.UndoOperation;
    _ = text.OperationQueue(text.Operation);
    _ = text.Edit(usize);
    _ = text.Patch(text.Point);
    _ = text.Anchor;
    _ = text.Selection(text.Point);
    _ = text.SelectionGoal;
    _ = text.LineEnding;
    _ = text.LineIndent;
    _ = text.Topic(u8);
    _ = text.Subscription(u8);

    try std.testing.expect(text.Point == text.rope.Point);
    try std.testing.expect(text.sum_tree.SumTree == text.rope.sum_tree.SumTree);
}
