//! Phase-0 scaffold for the Zig port of Zed's `text` crate.
//!
//! The central CRDT types are intentionally opaque until the clock and
//! consumer-readiness gates documented in `zig/ZIG-text.md` pass.

pub const clock = @import("clock");
pub const rope = @import("rope");
pub const sum_tree = @import("sum_tree");
pub const trace = @import("trace.zig");

pub const Point = rope.Point;
pub const PointUtf16 = rope.PointUtf16;
pub const OffsetUtf16 = rope.OffsetUtf16;
pub const Rope = rope.Rope;

pub const Buffer = opaque {};
pub const BufferSnapshot = opaque {};
pub const EditedBufferSnapshot = opaque {};
pub const Operation = opaque {};
pub const OperationQueue = @import("operation_queue.zig").OperationQueue;
pub const EditOperation = opaque {};
pub const UndoOperation = opaque {};
pub const Transaction = opaque {};
pub const HistoryEntry = opaque {};
pub const Anchor = opaque {};
pub const Locator = @import("locator.zig").Locator;
pub const SelectionGoal = @import("selection.zig").SelectionGoal;
pub const LineIndent = @import("line_indent.zig").LineIndent;
pub const SubscriptionState = opaque {};

pub const BufferId = @import("buffer_id.zig").BufferId;
pub const ReplicaId = clock.ReplicaId;
pub const TransactionId = u64;

pub const LineEnding = @import("line_ending.zig").LineEnding;
pub const NormalizedLineEndingText = @import("line_ending.zig").Normalized;

pub const Edit = @import("edit.zig").Edit;
pub const Patch = @import("patch.zig").Patch;
pub const Selection = @import("selection.zig").Selection;

pub const UndoMap = @import("undo_map.zig").UndoMap;
pub const UndoMapCount = @import("undo_map.zig").Count;
pub const UndoMapOperation = @import("undo_map.zig").UndoOperation;
pub const Topic = @import("subscription.zig").Topic;
pub const Subscription = @import("subscription.zig").Subscription;

pub const baseline = struct {
    pub const zig = "0.16.0";
    pub const zed_revision = "90d024b88abc91264d9a0ad260eb4f365fa695c3";
    pub const trace_version: u16 = 1;
};
