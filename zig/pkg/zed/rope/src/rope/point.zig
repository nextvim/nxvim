const std = @import("std");

/// A zero-indexed point in a text buffer, measured in rows and UTF-8 bytes.
pub const Point = struct {
    row: u32 = 0,
    column: u32 = 0,

    pub const max = Point{ .row = std.math.maxInt(u32), .column = std.math.maxInt(u32) };

    pub fn new(row: u32, column: u32) Point {
        return .{ .row = row, .column = column };
    }

    pub fn zero() Point {
        return .{};
    }

    pub fn rowRange(start: u32, end: u32) struct { start: Point, end: Point } {
        return .{ .start = .new(start, 0), .end = .new(end, 0) };
    }

    pub fn parse(text: []const u8) Point {
        var result = Point.zero();
        for (text) |byte| {
            if (byte == '\n') {
                result.row += 1;
                result.column = 0;
            } else {
                result.column += 1;
            }
        }
        return result;
    }

    pub fn isZero(self: Point) bool {
        return self.row == 0 and self.column == 0;
    }

    pub fn order(self: Point, other: Point) std.math.Order {
        if (self.row < other.row) return .lt;
        if (self.row > other.row) return .gt;
        return std.math.order(self.column, other.column);
    }

    pub fn add(self: Point, other: Point) Point {
        if (other.row == 0) return .new(self.row, self.column + other.column);
        return .new(self.row + other.row, other.column);
    }

    pub fn addAssign(self: *Point, other: Point) void {
        self.* = self.add(other);
    }

    pub fn sub(self: Point, other: Point) Point {
        std.debug.assert(other.order(self) != .gt);
        if (self.row == other.row) return .new(0, self.column - other.column);
        return .new(self.row - other.row, self.column);
    }

    pub fn saturatingSub(self: Point, other: Point) Point {
        if (self.order(other) == .lt) return .zero();
        return self.sub(other);
    }
};
