const std = @import("std");

/// A zero-indexed point in a text buffer, measured in rows and UTF-16 code units.
pub const PointUtf16 = struct {
    row: u32 = 0,
    column: u32 = 0,

    pub const max = PointUtf16{ .row = std.math.maxInt(u32), .column = std.math.maxInt(u32) };

    pub fn new(row: u32, column: u32) PointUtf16 {
        return .{ .row = row, .column = column };
    }

    pub fn zero() PointUtf16 {
        return .{};
    }

    pub fn isZero(self: PointUtf16) bool {
        return self.row == 0 and self.column == 0;
    }

    pub fn order(self: PointUtf16, other: PointUtf16) std.math.Order {
        if (self.row < other.row) return .lt;
        if (self.row > other.row) return .gt;
        return std.math.order(self.column, other.column);
    }

    pub fn add(self: PointUtf16, other: PointUtf16) PointUtf16 {
        if (other.row == 0) return .new(self.row, self.column + other.column);
        return .new(self.row + other.row, other.column);
    }

    pub fn addAssign(self: *PointUtf16, other: PointUtf16) void {
        self.* = self.add(other);
    }

    pub fn sub(self: PointUtf16, other: PointUtf16) PointUtf16 {
        std.debug.assert(other.order(self) != .gt);
        if (self.row == other.row) return .new(0, self.column - other.column);
        return .new(self.row - other.row, self.column);
    }

    pub fn saturatingSub(self: PointUtf16, other: PointUtf16) PointUtf16 {
        if (self.order(other) == .lt) return .zero();
        return self.sub(other);
    }
};
