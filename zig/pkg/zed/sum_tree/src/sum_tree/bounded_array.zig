const std = @import("std");

pub fn BoundedArray(comptime T: type, comptime capacity: usize) type {
    return struct {
        const Self = @This();

        storage: [capacity]T = undefined,
        len: usize = 0,

        pub const Error = error{CapacityExceeded};

        pub fn init() Self {
            return .{};
        }

        pub fn slice(self: *Self) []T {
            return self.storage[0..self.len];
        }

        pub fn constSlice(self: *const Self) []const T {
            return self.storage[0..self.len];
        }

        pub fn append(self: *Self, value: T) Error!void {
            if (self.len == capacity) return error.CapacityExceeded;
            self.storage[self.len] = value;
            self.len += 1;
        }

        pub fn insert(self: *Self, index: usize, value: T) Error!void {
            std.debug.assert(index <= self.len);
            if (self.len == capacity) return error.CapacityExceeded;
            std.mem.copyBackwards(T, self.storage[index + 1 .. self.len + 1], self.storage[index..self.len]);
            self.storage[index] = value;
            self.len += 1;
        }

        pub fn appendSlice(self: *Self, values: []const T) Error!void {
            if (values.len > capacity - self.len) return error.CapacityExceeded;
            @memcpy(self.storage[self.len .. self.len + values.len], values);
            self.len += values.len;
        }

        pub fn removeRange(self: *Self, start: usize, end: usize) void {
            std.debug.assert(start <= end and end <= self.len);
            const removed = end - start;
            std.mem.copyForwards(T, self.storage[start .. self.len - removed], self.storage[end..self.len]);
            self.len -= removed;
        }

        pub fn truncate(self: *Self, new_len: usize) void {
            std.debug.assert(new_len <= self.len);
            self.len = new_len;
        }

        pub fn clear(self: *Self) void {
            self.len = 0;
        }
    };
}
