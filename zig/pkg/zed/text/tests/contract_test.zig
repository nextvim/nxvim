const std = @import("std");
const text = @import("text");

test "phase-0 public declarations compile" {
    try std.testing.expectEqualStrings("0.16.0", text.baseline.zig);
    try std.testing.expectEqualStrings(
        "90d024b88abc91264d9a0ad260eb4f365fa695c3",
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
    _ = text.OperationQueue;
    _ = text.Edit(usize);
    _ = text.Patch(text.Point);
    _ = text.Anchor;
    _ = text.Selection(text.Point);
    _ = text.SelectionGoal;
    _ = text.LineEnding;
    _ = text.LineIndent;
    _ = text.Topic;
    _ = text.Subscription;
    _ = text.UndoMap;

    try std.testing.expect(text.Point == text.rope.Point);
    try std.testing.expect(text.sum_tree.SumTree == text.rope.sum_tree.SumTree);
}
