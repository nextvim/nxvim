const std = @import("std");
const clock = @import("clock");
const sum_tree = @import("sum_tree");

pub const Count = struct {
    edit_id: clock.Lamport,
    count: u32,
};

/// The portion of Zed's UndoOperation consumed by UndoMap.
pub const UndoOperation = struct {
    timestamp: clock.Lamport,
    counts: []const Count,
};

pub const UndoMapKey = struct {
    edit_id: clock.Lamport,
    undo_id: clock.Lamport,

    pub fn order(self: UndoMapKey, other: UndoMapKey) std.math.Order {
        const edit_order = self.edit_id.order(other.edit_id);
        return if (edit_order == .eq) self.undo_id.order(other.undo_id) else edit_order;
    }

    pub fn eql(self: UndoMapKey, other: UndoMapKey) bool {
        return self.edit_id.eql(other.edit_id) and self.undo_id.eql(other.undo_id);
    }
};

const Entry = struct {
    key: UndoMapKey,
    undo_count: u32,
};

const TreeOps = struct {
    pub const Summary = UndoMapKey;
    pub const Context = void;

    pub fn summary(entry: *const Entry, _: void) Summary {
        return entry.key;
    }

    pub fn zero(_: void) Summary {
        return .{ .edit_id = clock.Lamport.MIN, .undo_id = clock.Lamport.MIN };
    }

    pub fn addSummary(total: *Summary, value: *const Summary, _: void) void {
        if (total.order(value.*) == .lt) total.* = value.*;
    }

    pub fn cloneItem(entry: *const Entry, _: std.mem.Allocator) !Entry {
        return entry.*;
    }

    pub fn deinitItem(_: *Entry, _: std.mem.Allocator) void {}

    pub fn cloneSummary(value: *const Summary, _: std.mem.Allocator) !Summary {
        return value.*;
    }

    pub fn deinitSummary(_: *Summary, _: std.mem.Allocator) void {}

    pub fn eqlSummary(a: *const Summary, b: *const Summary) bool {
        return a.eql(b.*);
    }
};

const KeyOps = struct {
    pub const Key = UndoMapKey;

    pub fn key(entry: *const Entry) Key {
        return entry.key;
    }

    pub fn compareKeys(a: *const Key, b: *const Key) std.math.Order {
        return a.order(b.*);
    }

    pub fn compareItemKey(entry: *const Entry, key_value: *const Key) std.math.Order {
        return entry.key.order(key_value.*);
    }
};

const Tree = sum_tree.SumTree(Entry, TreeOps, sum_tree.DefaultTreeBase);

pub const UndoMap = struct {
    tree: Tree,

    pub fn init(allocator: std.mem.Allocator) !UndoMap {
        return .{ .tree = try Tree.init(allocator, {}) };
    }

    pub fn deinit(self: *UndoMap) void {
        self.tree.deinit();
        self.* = undefined;
    }

    /// Creates a cheap copy-on-write snapshot. The supplied allocator is kept
    /// for API symmetry with other owning text types; SumTree retains its
    /// original owning allocator across snapshots.
    pub fn clone(self: *const UndoMap, _: std.mem.Allocator) !UndoMap {
        return .{ .tree = self.tree.clone() };
    }

    /// Inserts or updates every `(edit_id, undo timestamp)` entry atomically.
    /// If any allocation fails, `self` remains byte-for-byte logically intact.
    pub fn insert(self: *UndoMap, undo: *const UndoOperation) !void {
        var replacement = self.tree.clone();
        errdefer replacement.deinit();

        for (undo.counts) |count| {
            _ = try replacement.insertOrReplace(KeyOps, .{
                .key = .{ .edit_id = count.edit_id, .undo_id = undo.timestamp },
                .undo_count = count.count,
            }, {});
        }

        self.tree.deinit();
        self.tree = replacement;
    }

    pub fn isUndone(self: *const UndoMap, edit_id: clock.Lamport) bool {
        return self.undoCount(edit_id) % 2 == 1;
    }

    pub fn wasUndone(self: *const UndoMap, edit_id: clock.Lamport, version: *const clock.Global) bool {
        return self.filteredUndoCount(edit_id, version) % 2 == 1;
    }

    pub fn undoCount(self: *const UndoMap, edit_id: clock.Lamport) u32 {
        return self.filteredUndoCount(edit_id, null);
    }

    fn filteredUndoCount(self: *const UndoMap, edit_id: clock.Lamport, version: ?*const clock.Global) u32 {
        var maximum: u32 = 0;
        var index: usize = 0;
        while (self.tree.itemAt(index)) |entry| : (index += 1) {
            switch (entry.key.edit_id.order(edit_id)) {
                .lt => continue,
                .gt => break,
                .eq => {},
            }
            if (version == null or version.?.observed(entry.key.undo_id))
                maximum = @max(maximum, entry.undo_count);
        }
        return maximum;
    }
};
