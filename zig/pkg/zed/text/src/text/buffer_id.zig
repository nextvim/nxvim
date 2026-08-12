const std = @import("std");

/// A non-zero identifier for a text buffer.
pub const BufferId = struct {
    value: u64,

    pub const Error = error{
        ZeroBufferId,
        BufferIdOverflow,
    };

    /// Constructs an id, rejecting zero as Rust's `NonZeroU64` does.
    pub fn new(value: u64) Error!BufferId {
        if (value == 0) return error.ZeroBufferId;
        return .{ .value = value };
    }

    /// Returns the wrapped non-zero integer value.
    pub fn get(self: BufferId) u64 {
        return self.value;
    }

    /// Post-increments this id with saturation and returns its previous value,
    /// matching Rust's `NonZeroU64::saturating_add` behavior.
    pub fn next(self: *BufferId) BufferId {
        const previous = self.*;
        self.value +|= 1;
        return previous;
    }

    /// Checked post-increment for callers that must distinguish exhaustion.
    /// The receiver is unchanged when the increment cannot be represented.
    pub fn checkedNext(self: *BufferId) Error!BufferId {
        if (self.value == std.math.maxInt(u64)) return error.BufferIdOverflow;
        const previous = self.*;
        self.value += 1;
        return previous;
    }

    pub fn toProto(self: BufferId) u64 {
        return self.value;
    }

    pub fn fromProto(value: u64) Error!BufferId {
        return new(value);
    }

    pub fn eql(self: BufferId, other: BufferId) bool {
        return self.value == other.value;
    }

    pub fn order(self: BufferId, other: BufferId) std.math.Order {
        return std.math.order(self.value, other.value);
    }
};
