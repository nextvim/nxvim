const std = @import("std");
const edit_mod = @import("edit.zig");

pub fn Patch(comptime T: type) type {
    return struct {
        const Self = @This();
        pub const Coordinate = T;
        pub const Edit = edit_mod.Edit(T);

        allocator: std.mem.Allocator,
        list: std.ArrayList(Edit) = .empty,

        pub fn empty(allocator: std.mem.Allocator) Self {
            return .{ .allocator = allocator };
        }

        /// Copies `initial_edits`; the caller retains ownership of the slice.
        pub fn new(allocator: std.mem.Allocator, initial_edits: []const Edit) !Self {
            var result = empty(allocator);
            errdefer result.deinit();
            try result.list.appendSlice(allocator, initial_edits);
            std.debug.assert(result.isCanonical());
            return result;
        }

        pub fn clone(self: *const Self, allocator: std.mem.Allocator) !Self {
            return new(allocator, self.list.items);
        }

        pub fn deinit(self: *Self) void {
            self.list.deinit(self.allocator);
            self.* = undefined;
        }

        pub fn edits(self: *const Self) []const Edit {
            return self.list.items;
        }

        /// Transfers the backing list to the caller. The returned list must be
        /// deinitialized with this patch's allocator; `self` must not be used again.
        pub fn into(self: *Self) std.ArrayList(Edit) {
            const result = self.list;
            self.* = undefined;
            return result;
        }

        pub fn isEmpty(self: *const Self) bool {
            return self.list.items.len == 0;
        }

        pub fn clear(self: *Self) void {
            self.list.clearRetainingCapacity();
        }

        pub fn invert(self: *Self) void {
            for (self.list.items) |*item| std.mem.swap(edit_mod.Range(T), &item.old, &item.new);
            std.debug.assert(self.isCanonical());
        }

        /// Ignores an edit whose old and new ranges are both empty.
        pub fn push(self: *Self, item: Edit) !void {
            if (item.isEmpty()) return;
            try self.pushMaybeEmpty(item);
        }

        /// Appends or coalesces using the checked-in Rust implementation's
        /// precondition: edits arrive in monotonically increasing coordinates.
        pub fn pushMaybeEmpty(self: *Self, item: Edit) !void {
            std.debug.assert(validEdit(item));
            if (self.list.items.len != 0) {
                const last = &self.list.items[self.list.items.len - 1];
                // The Rust algorithm relies on both coordinate spaces advancing
                // monotonically, even when touching old ranges are coalesced.
                std.debug.assert(item.old.start >= last.old.start);
                std.debug.assert(item.old.end >= last.old.end);
                std.debug.assert(item.new.start >= last.new.start);
                std.debug.assert(item.new.end >= last.new.end);
                if (last.old.end >= item.old.start) {
                    last.old.end = item.old.end;
                    last.new.end = item.new.end;
                    return;
                }
            }
            try self.list.append(self.allocator, item);
            std.debug.assert(self.isCanonical());
        }

        /// Compose this old->middle patch with `new_edits` (middle->new).
        /// Neither input is changed, including when allocation fails.
        pub fn compose(self: *const Self, new_edits: []const Edit) !Self {
            var composed = empty(self.allocator);
            errdefer composed.deinit();

            var old_ix: usize = 0;
            var new_ix: usize = 0;
            var old_current: Edit = undefined;
            var new_current: Edit = undefined;
            var have_old = false;
            var have_new = false;
            var old_start: T = std.mem.zeroes(T);
            var new_start: T = std.mem.zeroes(T);

            while (true) {
                if (!have_old and old_ix < self.list.items.len) {
                    old_current = self.list.items[old_ix];
                    have_old = true;
                }
                if (!have_new and new_ix < new_edits.len) {
                    new_current = new_edits[new_ix];
                    have_new = true;
                }

                if (have_old and (!have_new or old_current.new.end < new_current.old.start)) {
                    const catchup = old_current.old.start - old_start;
                    old_start += catchup;
                    new_start += catchup;
                    const old_end = old_start + old_current.oldLen();
                    const new_end = new_start + old_current.newLen();
                    try composed.push(.{ .old = .{ .start = old_start, .end = old_end }, .new = .{ .start = new_start, .end = new_end } });
                    old_start = old_end;
                    new_start = new_end;
                    old_ix += 1;
                    have_old = false;
                    continue;
                }

                if (have_new and (!have_old or new_current.old.end < old_current.new.start)) {
                    const catchup = new_current.new.start - new_start;
                    old_start += catchup;
                    new_start += catchup;
                    const old_end = old_start + new_current.oldLen();
                    const new_end = new_start + new_current.newLen();
                    try composed.push(.{ .old = .{ .start = old_start, .end = old_end }, .new = .{ .start = new_start, .end = new_end } });
                    old_start = old_end;
                    new_start = new_end;
                    new_ix += 1;
                    have_new = false;
                    continue;
                }

                if (!have_old or !have_new) break;

                if (old_current.new.start < new_current.old.start) {
                    const catchup = old_current.old.start - old_start;
                    old_start += catchup;
                    new_start += catchup;
                    const overshoot = new_current.old.start - old_current.new.start;
                    const old_end = @min(old_start + overshoot, old_current.old.end);
                    const new_end = new_start + overshoot;
                    try composed.push(.{ .old = .{ .start = old_start, .end = old_end }, .new = .{ .start = new_start, .end = new_end } });
                    old_current.old.start = old_end;
                    old_current.new.start += overshoot;
                    old_start = old_end;
                    new_start = new_end;
                } else {
                    const catchup = new_current.new.start - new_start;
                    old_start += catchup;
                    new_start += catchup;
                    const overshoot = old_current.new.start - new_current.old.start;
                    const old_end = old_start + overshoot;
                    const new_end = @min(new_start + overshoot, new_current.new.end);
                    try composed.push(.{ .old = .{ .start = old_start, .end = old_end }, .new = .{ .start = new_start, .end = new_end } });
                    new_current.old.start += overshoot;
                    new_current.new.start = new_end;
                    old_start = old_end;
                    new_start = new_end;
                }

                if (old_current.new.end > new_current.old.end) {
                    const old_end = old_start + @min(old_current.oldLen(), new_current.oldLen());
                    const new_end = new_start + new_current.newLen();
                    try composed.push(.{ .old = .{ .start = old_start, .end = old_end }, .new = .{ .start = new_start, .end = new_end } });
                    old_current.old.start = old_end;
                    old_current.new.start = new_current.old.end;
                    old_start = old_end;
                    new_start = new_end;
                    new_ix += 1;
                    have_new = false;
                } else {
                    const old_end = old_start + old_current.oldLen();
                    const new_end = new_start + @min(old_current.newLen(), new_current.newLen());
                    try composed.push(.{ .old = .{ .start = old_start, .end = old_end }, .new = .{ .start = new_start, .end = new_end } });
                    new_current.old.start = old_current.new.end;
                    new_current.new.start = new_end;
                    old_start = old_end;
                    new_start = new_end;
                    old_ix += 1;
                    have_old = false;
                }
            }
            return composed;
        }

        pub fn oldToNew(self: *const Self, old: T) T {
            const ix = self.editAtOrBefore(old) orelse return old;
            const item = self.list.items[ix];
            if (old >= item.old.end) return item.new.end + (old - item.old.end);
            return item.new.start;
        }

        /// Touch is inclusive at the old range's right boundary, matching Rust.
        pub fn editForOldPosition(self: *const Self, old: T) Edit {
            const ix = self.editAtOrBefore(old) orelse return emptyEdit(old, old);
            const item = self.list.items[ix];
            if (old > item.old.end) {
                const translated = item.new.end + (old - item.old.end);
                return emptyEdit(old, translated);
            }
            return item;
        }

        fn editAtOrBefore(self: *const Self, old: T) ?usize {
            var low: usize = 0;
            var high = self.list.items.len;
            while (low < high) {
                const mid = low + (high - low) / 2;
                if (self.list.items[mid].old.start < old) low = mid + 1 else high = mid;
            }
            if (low < self.list.items.len and self.list.items[low].old.start == old) return low;
            if (low == 0) return null;
            return low - 1;
        }

        fn emptyEdit(old_position: T, new_position: T) Edit {
            return .{
                .old = .{ .start = old_position, .end = old_position },
                .new = .{ .start = new_position, .end = new_position },
            };
        }

        fn validEdit(item: Edit) bool {
            return item.old.start <= item.old.end and item.new.start <= item.new.end;
        }

        fn isCanonical(self: *const Self) bool {
            for (self.list.items, 0..) |item, ix| {
                if (!validEdit(item)) return false;
                if (ix != 0) {
                    const previous = self.list.items[ix - 1];
                    if (!(item.old.start > previous.old.end and item.new.start > previous.new.end)) return false;
                }
            }
            return true;
        }
    };
}
