const std = @import("std");

/// An offset measured in UTF-16 code units.
pub const OffsetUtf16 = struct {
    value: usize = 0,

    pub fn new(value: usize) OffsetUtf16 {
        return .{ .value = value };
    }

    pub fn zero() OffsetUtf16 {
        return .{};
    }

    pub fn order(self: OffsetUtf16, other: OffsetUtf16) std.math.Order {
        return std.math.order(self.value, other.value);
    }

    pub fn add(self: OffsetUtf16, other: OffsetUtf16) OffsetUtf16 {
        return .new(self.value + other.value);
    }

    pub fn addAssign(self: *OffsetUtf16, other: OffsetUtf16) void {
        self.value += other.value;
    }

    pub fn sub(self: OffsetUtf16, other: OffsetUtf16) OffsetUtf16 {
        std.debug.assert(other.value <= self.value);
        return .new(self.value - other.value);
    }

    pub fn saturatingSub(self: OffsetUtf16, other: OffsetUtf16) OffsetUtf16 {
        return .new(self.value -| other.value);
    }
};
