const std = @import("std");
const sum_tree = @import("sum_tree.zig");

pub fn TreeMap(comptime K: type, comptime V: type, comptime Ops: type, comptime tree_base: usize) type {
    comptime {
        requireDecl(Ops, "compareKeys");
        requireDecl(Ops, "cloneKey");
        requireDecl(Ops, "deinitKey");
        requireDecl(Ops, "cloneValue");
        requireDecl(Ops, "deinitValue");
    }

    const Entry = struct { key: K, value: V };
    const EntryOps = struct {
        pub const Summary = usize;
        pub const Context = void;
        pub fn summary(_: *const Entry, _: void) usize {
            return 1;
        }
        pub fn zero(_: void) usize {
            return 0;
        }
        pub fn addSummary(total: *usize, value: *const usize, _: void) void {
            total.* += value.*;
        }
        pub fn cloneItem(entry: *const Entry, allocator: std.mem.Allocator) !Entry {
            const key = try Ops.cloneKey(&entry.key, allocator);
            errdefer {
                var owned_key = key;
                Ops.deinitKey(&owned_key, allocator);
            }
            return .{ .key = key, .value = try Ops.cloneValue(&entry.value, allocator) };
        }
        pub fn deinitItem(entry: *Entry, allocator: std.mem.Allocator) void {
            Ops.deinitKey(&entry.key, allocator);
            Ops.deinitValue(&entry.value, allocator);
        }
        pub fn cloneSummary(value: *const usize, _: std.mem.Allocator) !usize {
            return value.*;
        }
        pub fn deinitSummary(_: *usize, _: std.mem.Allocator) void {}
        pub fn eqlSummary(a: *const usize, b: *const usize) bool {
            return a.* == b.*;
        }
    };
    const KeyOps = struct {
        pub const Key = K;
        pub fn key(entry: *const Entry) K {
            return entry.key;
        }
        pub fn compareKeys(a: *const K, b: *const K) std.math.Order {
            return Ops.compareKeys(a, b);
        }
        pub fn compareItemKey(entry: *const Entry, key_value: *const K) std.math.Order {
            return Ops.compareKeys(&entry.key, key_value);
        }
    };
    const Tree = sum_tree.SumTree(Entry, EntryOps, tree_base);

    return struct {
        const Self = @This();
        pub const EntryType = Entry;

        tree: Tree,

        pub fn init(allocator: std.mem.Allocator) !Self {
            return .{ .tree = try Tree.init(allocator, {}) };
        }

        pub fn fromOrderedEntries(allocator: std.mem.Allocator, entries: []const Entry) !Self {
            return .{ .tree = try Tree.fromSlice(allocator, entries, {}) };
        }

        pub fn clone(self: Self) Self {
            return .{ .tree = self.tree.clone() };
        }
        pub fn deinit(self: *Self) void {
            self.tree.deinit();
            self.* = undefined;
        }
        pub fn isEmpty(self: *const Self) bool {
            return self.tree.isEmpty();
        }
        pub fn count(self: *const Self) usize {
            return self.tree.itemCount();
        }
        pub fn containsKey(self: *const Self, key: *const K) bool {
            return self.get(key) != null;
        }

        pub fn get(self: *const Self, key: *const K) ?*const V {
            const entry = self.tree.getKeyed(KeyOps, key) orelse return null;
            return &entry.value;
        }

        pub fn insert(self: *Self, key: K, value: V) !void {
            var old = try self.insertOrReplace(key, value);
            if (old) |*owned| Ops.deinitValue(owned, self.getAllocator());
        }

        pub fn insertOrReplace(self: *Self, key: K, value: V) !?V {
            var replaced = try self.tree.insertOrReplace(KeyOps, .{ .key = key, .value = value }, {});
            if (replaced) |*entry| {
                Ops.deinitKey(&entry.key, self.getAllocator());
                return entry.value;
            }
            return null;
        }

        pub fn extend(self: *Self, entries: []const Entry) !void {
            for (entries) |entry| try self.insert(entry.key, entry.value);
        }

        pub fn clear(self: *Self) !void {
            const replacement = try Tree.init(self.getAllocator(), {});
            self.tree.deinit();
            self.tree = replacement;
        }

        pub fn remove(self: *Self, key: K) !?V {
            var removed = try self.tree.removeKeyed(KeyOps, key, {});
            if (removed) |*entry| {
                Ops.deinitKey(&entry.key, self.getAllocator());
                return entry.value;
            }
            return null;
        }

        pub fn removeRange(self: *Self, comptime StartTarget: type, start: StartTarget, comptime EndTarget: type, end: EndTarget) !void {
            const allocator_value = self.getAllocator();
            var entries: std.ArrayList(Entry) = .empty;
            defer deinitEntries(&entries, allocator_value);
            var iterator_value = self.iterator();
            while (iterator_value.next()) |entry| {
                const in_range = StartTarget.compare(start, &entry.key) != .gt and EndTarget.compare(end, &entry.key) == .gt;
                if (!in_range) try entries.append(allocator_value, try EntryOps.cloneItem(entry, allocator_value));
            }
            const replacement = try Tree.fromSlice(allocator_value, entries.items, {});
            self.tree.deinit();
            self.tree = replacement;
        }

        pub fn closest(self: *const Self, key: *const K) ?*const Entry {
            var low: usize = 0;
            var high = self.count();
            while (low < high) {
                const middle = low + (high - low) / 2;
                if (Ops.compareKeys(&self.tree.itemAt(middle).?.key, key) != .gt) low = middle + 1 else high = middle;
            }
            return if (low == 0) null else self.tree.itemAt(low - 1);
        }

        pub fn update(self: *Self, key: *const K, update_fn: anytype) !bool {
            const existing = self.tree.getKeyed(KeyOps, key) orelse return false;
            const allocator_value = self.getAllocator();
            var entry = try EntryOps.cloneItem(existing, allocator_value);
            defer EntryOps.deinitItem(&entry, allocator_value);
            try invokeUpdate(update_fn, &entry.value);
            var old = try self.tree.insertOrReplace(KeyOps, entry, {});
            if (old) |*removed| EntryOps.deinitItem(removed, allocator_value);
            return true;
        }

        pub fn retain(self: *Self, predicate: anytype) !void {
            const allocator_value = self.getAllocator();
            var entries: std.ArrayList(Entry) = .empty;
            defer deinitEntries(&entries, allocator_value);
            var iterator_value = self.iterator();
            while (iterator_value.next()) |entry| {
                if (@call(.auto, predicate, .{ &entry.key, &entry.value }))
                    try entries.append(allocator_value, try EntryOps.cloneItem(entry, allocator_value));
            }
            const replacement = try Tree.fromSlice(allocator_value, entries.items, {});
            self.tree.deinit();
            self.tree = replacement;
        }

        pub fn insertMap(self: *Self, other: *const Self) !void {
            var iterator_value = other.iterator();
            while (iterator_value.next()) |entry| try self.insert(entry.key, entry.value);
        }

        pub fn first(self: *const Self) ?*const Entry {
            return self.tree.first();
        }
        pub fn last(self: *const Self) ?*const Entry {
            return self.tree.last();
        }
        pub fn iterator(self: *const Self) Iterator {
            return .{ .inner = self.tree.iterator() };
        }
        pub fn iteratorFrom(self: *const Self, key: *const K) Iterator {
            return .{ .inner = self.tree.iterator(), .skip_before = key };
        }

        pub const Iterator = struct {
            inner: Tree.Iterator,
            skip_before: ?*const K = null,
            pub fn next(self: *Iterator) ?*const Entry {
                while (self.inner.next()) |entry| {
                    if (self.skip_before) |key| {
                        if (Ops.compareKeys(&entry.key, key) == .lt) continue;
                        self.skip_before = null;
                    }
                    return entry;
                }
                return null;
            }
        };

        fn getAllocator(self: *const Self) std.mem.Allocator {
            return self.tree.getAllocator();
        }
        fn deinitEntries(entries: *std.ArrayList(Entry), allocator_value: std.mem.Allocator) void {
            for (entries.items) |*entry| EntryOps.deinitItem(entry, allocator_value);
            entries.deinit(allocator_value);
        }
        fn invokeUpdate(update_fn: anytype, value: *V) !void {
            const result = @call(.auto, update_fn, .{value});
            if (@typeInfo(@TypeOf(result)) == .error_union) try result;
        }
    };
}

pub fn TreeSet(comptime K: type, comptime Ops: type, comptime tree_base: usize) type {
    const ValueOps = struct {
        pub fn compareKeys(a: *const K, b: *const K) std.math.Order {
            return Ops.compareKeys(a, b);
        }
        pub fn cloneKey(key: *const K, allocator: std.mem.Allocator) !K {
            return Ops.cloneKey(key, allocator);
        }
        pub fn deinitKey(key: *K, allocator: std.mem.Allocator) void {
            Ops.deinitKey(key, allocator);
        }
        pub fn cloneValue(_: *const void, _: std.mem.Allocator) !void {}
        pub fn deinitValue(_: *void, _: std.mem.Allocator) void {}
    };
    const Map = TreeMap(K, void, ValueOps, tree_base);
    return struct {
        const Self = @This();
        map: Map,
        pub fn init(allocator: std.mem.Allocator) !Self {
            return .{ .map = try Map.init(allocator) };
        }
        pub fn clone(self: Self) Self {
            return .{ .map = self.map.clone() };
        }
        pub fn deinit(self: *Self) void {
            self.map.deinit();
            self.* = undefined;
        }
        pub fn isEmpty(self: *const Self) bool {
            return self.map.isEmpty();
        }
        pub fn count(self: *const Self) usize {
            return self.map.count();
        }
        pub fn contains(self: *const Self, key: *const K) bool {
            return self.map.containsKey(key);
        }
        pub fn insert(self: *Self, key: K) !void {
            try self.map.insert(key, {});
        }
        pub fn remove(self: *Self, key: K) !bool {
            return (try self.map.remove(key)) != null;
        }
        pub fn extend(self: *Self, keys: []const K) !void {
            for (keys) |key| try self.insert(key);
        }
        pub fn iterator(self: *const Self) Iterator {
            return .{ .inner = self.map.iterator() };
        }
        pub fn iteratorFrom(self: *const Self, key: *const K) Iterator {
            return .{ .inner = self.map.iteratorFrom(key) };
        }
        pub const Iterator = struct {
            inner: Map.Iterator,
            pub fn next(self: *Iterator) ?*const K {
                const entry = self.inner.next() orelse return null;
                return &entry.key;
            }
        };
    };
}

fn requireDecl(comptime T: type, comptime name: []const u8) void {
    if (!@hasDecl(T, name)) @compileError(@typeName(T) ++ " must declare " ++ name);
}
