const std = @import("std");

/// The horizontal target retained by vertical selection movement.
pub const SelectionGoal = union(enum) {
    none,
    horizontal_position: f64,
    horizontal_range: struct { start: f64, end: f64 },
    wrapped_horizontal_position: struct { row: u32, x: f32 },
};

pub fn Range(comptime T: type) type {
    return struct { start: T, end: T };
}

/// A half-open ordered range whose `reversed` bit records which endpoint is
/// the moving head. As in Rust, callers are responsible for constructing
/// selections with ordered endpoints.
pub fn Selection(comptime T: type) type {
    return struct {
        const Self = @This();

        id: usize,
        start: T,
        end: T,
        reversed: bool = false,
        goal: SelectionGoal = .none,

        pub fn fromOffset(offset: T) Self {
            return .{ .id = 0, .start = offset, .end = offset };
        }

        pub fn head(self: Self) T {
            return if (self.reversed) self.start else self.end;
        }

        pub fn tail(self: Self) T {
            return if (self.reversed) self.end else self.start;
        }

        pub fn range(self: Self) Range(T) {
            return .{ .start = self.start, .end = self.end };
        }

        pub fn len(self: Self) @TypeOf(self.end - self.start) {
            return self.end - self.start;
        }

        pub fn isEmpty(self: Self) bool {
            return std.meta.eql(self.start, self.end);
        }

        pub fn equals(self: Self, other: Range(T)) bool {
            return std.meta.eql(self.start, other.start) and std.meta.eql(self.end, other.end);
        }

        /// Zig's value parameters provide the copy semantics required by the
        /// pinned Rust `T: Clone` implementation without allocation.
        pub fn map(self: Self, comptime S: type, mapper: anytype) Selection(S) {
            return .{
                .id = self.id,
                .start = mapper(self.start),
                .end = mapper(self.end),
                .reversed = self.reversed,
                .goal = self.goal,
            };
        }

        pub fn collapseTo(self: *Self, point: T, new_goal: SelectionGoal) void {
            self.start = point;
            self.end = point;
            self.reversed = false;
            self.goal = new_goal;
        }

        pub fn setHead(self: *Self, new_head: T, new_goal: SelectionGoal) void {
            const old_tail = self.tail();
            if (new_head < old_tail) {
                if (!self.reversed) {
                    self.end = self.start;
                    self.reversed = true;
                }
                self.start = new_head;
            } else {
                if (self.reversed) {
                    self.start = self.end;
                    self.reversed = false;
                }
                self.end = new_head;
            }
            self.goal = new_goal;
        }

        pub fn setTail(self: *Self, new_tail: T, new_goal: SelectionGoal) void {
            const old_head = self.head();
            if (new_tail <= old_head) {
                if (self.reversed) {
                    self.end = self.start;
                    self.reversed = false;
                }
                self.start = new_tail;
            } else {
                if (!self.reversed) {
                    self.start = self.end;
                    self.reversed = true;
                }
                self.end = new_tail;
            }
            self.goal = new_goal;
        }

        pub fn setHeadTail(self: *Self, new_head: T, new_tail: T, new_goal: SelectionGoal) void {
            if (new_head < new_tail) {
                self.reversed = true;
                self.start = new_head;
                self.end = new_tail;
            } else {
                self.reversed = false;
                self.start = new_tail;
                self.end = new_head;
            }
            self.goal = new_goal;
        }

        pub fn swapHeadTail(self: *Self) void {
            if (self.reversed) {
                self.reversed = false;
            } else {
                std.mem.swap(T, &self.start, &self.end);
            }
        }
    };
}
