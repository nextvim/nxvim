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

const buffer = @import("buffer.zig");
pub const Buffer = buffer.Buffer;
pub const BufferSnapshot = buffer.BufferSnapshot;
pub const EditedBufferSnapshot = buffer.EditedBufferSnapshot;
pub const Anchor = buffer.Anchor;
pub const AnchorRange = buffer.AnchorRange;
pub const MAX_INSERTION_LEN = buffer.max_insertion_len;
pub const Operation = buffer.Operation;
pub const InputEdit = buffer.InputEdit;
pub const BufferSubscription = buffer.BufferSubscription;
pub const OperationQueue = @import("operation_queue.zig").OperationQueue;
pub const EditOperation = buffer.EditOperation;
pub const UndoOperation = buffer.UndoOperation;
pub const Transaction = buffer.Transaction;
pub const HistoryEntry = buffer.HistoryEntry;
pub const History = buffer.History;
pub const Locator = @import("locator.zig").Locator;
pub const SelectionGoal = @import("selection.zig").SelectionGoal;
pub const LineIndent = @import("line_indent.zig").LineIndent;
pub const WaitHandle = @import("waiter.zig").WaitHandle;
pub const RegexMatcher = @import("regex.zig").RegexMatcher;
pub const RegexMatch = @import("regex.zig").Match;
pub const RegexMatchIterator = @import("regex.zig").MatchIterator;
pub const regexMatches = @import("regex.zig").matches;

pub const BufferId = @import("buffer_id.zig").BufferId;
pub const ReplicaId = clock.ReplicaId;
pub const TransactionId = buffer.TransactionId;

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

const fragment = @import("fragment.zig");
pub const Fragment = fragment.Fragment;
pub const FragmentSummary = fragment.FragmentSummary;
pub const FragmentTextSummary = fragment.FragmentTextSummary;
pub const FragmentTree = fragment.FragmentTree;
pub const FragmentBuilder = fragment.FragmentBuilder;
pub const FullOffset = fragment.FullOffset;
pub const FullOffsetDimension = fragment.FullOffsetDimension;
pub const VisibleOffsetDimension = fragment.VisibleOffsetDimension;
pub const FragmentTextDimension = fragment.FragmentTextDimension;
pub const VersionedFullOffset = fragment.VersionedFullOffset;
pub const VersionedFullOffsetDimension = fragment.VersionedFullOffsetDimension;
pub const InsertionFragment = fragment.InsertionFragment;
pub const InsertionFragmentKey = fragment.InsertionFragmentKey;
pub const InsertionSlice = fragment.InsertionSlice;
pub const InsertionTree = fragment.InsertionTree;
pub const InsertionKeyOps = fragment.InsertionKeyOps;
pub const rebuildFragmentRopes = fragment.rebuildRopes;

pub const baseline = struct {
    pub const zig = "0.16.0";
    pub const zed_revision = "90d024b88abc91264d9a0ad260eb4f365fa695c3";
    pub const trace_version: u16 = 1;
};
