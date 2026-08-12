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
pub const EditOperation = opaque {};
pub const UndoOperation = opaque {};
pub const Transaction = opaque {};
pub const HistoryEntry = opaque {};
pub const Anchor = opaque {};
pub const Locator = opaque {};
pub const SelectionGoal = opaque {};
pub const LineIndent = opaque {};
pub const SubscriptionState = opaque {};

pub const BufferId = u64;
pub const ReplicaId = clock.ReplicaId;
pub const TransactionId = u64;

pub const LineEnding = enum {
    unix,
    windows,
};

pub fn Edit(comptime T: type) type {
    return opaque {
        pub const Coordinate = T;
    };
}

pub fn Patch(comptime T: type) type {
    return opaque {
        pub const Coordinate = T;
    };
}

pub fn Selection(comptime T: type) type {
    return opaque {
        pub const Coordinate = T;
    };
}

pub fn OperationQueue(comptime T: type) type {
    return opaque {
        pub const Item = T;
    };
}

pub fn Topic(comptime T: type) type {
    return opaque {
        pub const Item = T;
    };
}

pub fn Subscription(comptime T: type) type {
    return opaque {
        pub const Item = T;
    };
}

pub const baseline = struct {
    pub const zig = "0.16.0";
    pub const zed_revision = "7a9ce83c781e725cb45940a8772527a991d4f9a4";
    pub const trace_version: u16 = 1;
};
