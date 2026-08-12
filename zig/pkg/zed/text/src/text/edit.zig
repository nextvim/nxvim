const std = @import("std");

/// A replacement of `old` by `new`, expressed in their respective coordinate
/// spaces. Coordinates are half-open ranges.
pub fn Range(comptime T: type) type {
    return struct {
        start: T,
        end: T,
    };
}

pub fn Edit(comptime T: type) type {
    return struct {
        const Self = @This();

        old: Range(T),
        new: Range(T),

        pub fn isEmpty(self: Self) bool {
            return std.meta.eql(self.old.start, self.old.end) and
                std.meta.eql(self.new.start, self.new.end);
        }

        pub fn oldLen(self: Self) T {
            return self.old.end - self.old.start;
        }

        pub fn newLen(self: Self) T {
            return self.new.end - self.new.start;
        }
    };
}
