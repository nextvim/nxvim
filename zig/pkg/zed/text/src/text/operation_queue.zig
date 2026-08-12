const std = @import("std");
const clock = @import("clock");
const sum_tree = @import("sum_tree");

/// A timestamp-ordered, uniquely keyed queue of owned operations.
///
/// `Ops` is an explicit ownership and timestamp contract:
///
///     pub fn timestamp(item: *const T) clock.Lamport;
///     pub fn clone(item: *const T, allocator: std.mem.Allocator) !T;
///     pub fn deinit(item: *T, allocator: std.mem.Allocator) void;
///
/// `insert` borrows its input. The queue clones accepted values, orders them by
/// Lamport's total order, and keeps only the last value for a duplicate key in a
/// batch. A key already in the queue is replaced by the incoming value.
pub fn OperationQueue(comptime T: type, comptime Ops: type) type {
    comptime {
        requireDecl(Ops, "timestamp");
        requireDecl(Ops, "clone");
        requireDecl(Ops, "deinit");
    }

    const OperationSummary = struct {
        key: clock.Lamport,
        len: usize,
    };

    const TreeOps = struct {
        pub const Summary = OperationSummary;
        pub const Context = void;

        pub fn summary(item: *const T, _: void) Summary {
            return .{ .key = Ops.timestamp(item), .len = 1 };
        }
        pub fn zero(_: void) Summary {
            return .{ .key = clock.Lamport.MIN, .len = 0 };
        }
        pub fn addSummary(total: *Summary, other: *const Summary, _: void) void {
            if (other.len == 0) return;
            total.key = other.key;
            total.len += other.len;
        }
        pub fn cloneItem(item: *const T, allocator: std.mem.Allocator) !T {
            return Ops.clone(item, allocator);
        }
        pub fn deinitItem(item: *T, allocator: std.mem.Allocator) void {
            Ops.deinit(item, allocator);
        }
        pub fn cloneSummary(value: *const Summary, _: std.mem.Allocator) !Summary {
            return value.*;
        }
        pub fn deinitSummary(_: *Summary, _: std.mem.Allocator) void {}
        pub fn eqlSummary(a: *const Summary, b: *const Summary) bool {
            return a.len == b.len and a.key.eql(b.key);
        }
    };

    const Tree = sum_tree.SumTree(T, TreeOps, sum_tree.DefaultTreeBase);

    return struct {
        const Self = @This();

        tree: Tree,

        pub fn init(allocator: std.mem.Allocator) !Self {
            return .{ .tree = try Tree.init(allocator, {}) };
        }

        /// A persistent clone. Items remain alive until the last queue sharing
        /// their SumTree nodes is deinitialized.
        pub fn clone(self: Self) Self {
            return .{ .tree = self.tree.clone() };
        }

        pub fn deinit(self: *Self) void {
            self.tree.deinit();
            self.* = undefined;
        }

        pub fn len(self: *const Self) usize {
            return self.tree.summary().len;
        }

        pub fn isEmpty(self: *const Self) bool {
            return self.len() == 0;
        }

        /// Transactional: on any allocation or item-clone failure, `self` is
        /// unchanged and all temporary ownership is released.
        pub fn insert(self: *Self, batch: []const T) !void {
            if (batch.len == 0) return;
            const allocator = self.tree.getAllocator();

            var owned: std.ArrayList(T) = .empty;
            defer deinitItems(&owned, allocator);
            try owned.ensureTotalCapacity(allocator, batch.len);
            for (batch) |*item| try owned.append(allocator, try Ops.clone(item, allocator));

            // Stable insertion sort makes duplicate handling deterministic.
            for (1..owned.items.len) |index| {
                var cursor = index;
                while (cursor > 0 and Ops.timestamp(&owned.items[cursor]).order(Ops.timestamp(&owned.items[cursor - 1])) == .lt) : (cursor -= 1) {
                    std.mem.swap(T, &owned.items[cursor], &owned.items[cursor - 1]);
                }
            }

            // Compact equal runs in place. Replacing the previous retained item
            // means the final occurrence in the caller's batch wins.
            var unique_len: usize = 0;
            for (0..owned.items.len) |read_index| {
                if (unique_len > 0 and Ops.timestamp(&owned.items[unique_len - 1]).eql(Ops.timestamp(&owned.items[read_index]))) {
                    Ops.deinit(&owned.items[unique_len - 1], allocator);
                    owned.items[unique_len - 1] = owned.items[read_index];
                } else {
                    if (unique_len != read_index) owned.items[unique_len] = owned.items[read_index];
                    unique_len += 1;
                }
            }
            owned.items.len = unique_len;

            // Build the complete replacement off to the side. Besides making
            // the queue atomic, this avoids exposing partially applied batches.
            var merged: std.ArrayList(T) = .empty;
            defer deinitItems(&merged, allocator);
            try merged.ensureTotalCapacity(allocator, self.len() + owned.items.len);
            var current = self.tree.iterator();
            while (current.next()) |item| merged.appendAssumeCapacity(try Ops.clone(item, allocator));

            for (owned.items) |*incoming| {
                const key = Ops.timestamp(incoming);
                var low: usize = 0;
                var high = merged.items.len;
                while (low < high) {
                    const middle = low + (high - low) / 2;
                    if (Ops.timestamp(&merged.items[middle]).order(key) == .lt) low = middle + 1 else high = middle;
                }
                const copy = try Ops.clone(incoming, allocator);
                if (low < merged.items.len and Ops.timestamp(&merged.items[low]).eql(key)) {
                    Ops.deinit(&merged.items[low], allocator);
                    merged.items[low] = copy;
                } else {
                    merged.insert(allocator, low, copy) catch unreachable;
                }
            }

            var replacement = try Tree.fromSlice(allocator, merged.items, {});
            errdefer replacement.deinit();
            self.tree.deinit();
            self.tree = replacement;
        }

        pub fn iterator(self: *const Self) Iterator {
            return .{ .inner = self.tree.iterator() };
        }

        pub const Iterator = struct {
            inner: Tree.Iterator,
            pub fn next(self: *Iterator) ?*const T {
                return self.inner.next();
            }
        };

        /// Moves all current operations into the returned queue. Allocation of
        /// the new empty root happens before mutation, so failure is atomic.
        pub fn drain(self: *Self) !Self {
            const replacement = try Self.init(self.tree.getAllocator());
            const drained = self.*;
            self.* = replacement;
            return drained;
        }

        pub fn clear(self: *Self) !void {
            var drained = try self.drain();
            drained.deinit();
        }

        fn deinitItems(items: *std.ArrayList(T), allocator: std.mem.Allocator) void {
            for (items.items) |*item| Ops.deinit(item, allocator);
            items.deinit(allocator);
        }
    };
}

fn requireDecl(comptime T: type, comptime name: []const u8) void {
    if (!@hasDecl(T, name)) @compileError(@typeName(T) ++ " must declare " ++ name);
}
