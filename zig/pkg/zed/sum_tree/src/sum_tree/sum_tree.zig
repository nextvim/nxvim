const std = @import("std");
const BoundedArray = @import("bounded_array.zig").BoundedArray;
const Shared = @import("shared.zig").Shared;

pub const DefaultTreeBase: usize = 6;

pub const NoSummary = struct {};

pub const Bias = enum {
    left,
    right,

    pub fn invert(self: Bias) Bias {
        return switch (self) {
            .left => .right,
            .right => .left,
        };
    }
};

pub fn Dimensions(comptime D1: type, comptime D2: type, comptime D3: type) type {
    return struct {
        first: D1,
        second: D2,
        third: D3,
    };
}

pub fn Edit(comptime Item: type, comptime Key: type) type {
    return union(enum) {
        insert: Item,
        remove: Key,
    };
}

pub fn SumTree(comptime Item: type, comptime Ops: type, comptime tree_base: usize) type {
    comptime {
        if (tree_base == 0) @compileError("SumTree tree_base must be greater than zero");
        requireDecl(Ops, "Summary");
        requireDecl(Ops, "Context");
        requireDecl(Ops, "summary");
        requireDecl(Ops, "zero");
        requireDecl(Ops, "addSummary");
        requireDecl(Ops, "cloneItem");
        requireDecl(Ops, "deinitItem");
        requireDecl(Ops, "cloneSummary");
        requireDecl(Ops, "deinitSummary");
        requireDecl(Ops, "eqlSummary");
    }

    const Summary = Ops.Summary;
    const Context = Ops.Context;
    const capacity = 2 * tree_base;

    return struct {
        const Self = @This();
        pub const ItemType = Item;
        pub const SummaryType = Summary;
        pub const ContextType = Context;
        pub const TreeBase = tree_base;
        pub const CursorMaxHeight: usize = 64;
        const ItemArray = BoundedArray(Item, capacity);
        const SummaryArray = BoundedArray(Summary, capacity);
        const max_parallel_participants: usize = 8;

        const Node = union(enum) {
            leaf: Leaf,
            internal: Internal,

            const Leaf = struct {
                summary: Summary,
                items: ItemArray,
                item_summaries: SummaryArray,
            };

            const Internal = struct {
                height: u32,
                item_count: usize,
                summary: Summary,
                child_summaries: SummaryArray,
                child_trees: ChildArray,
            };
        };

        const NodeHooks = struct {
            pub fn clone(node: *const Node, allocator: std.mem.Allocator) !Node {
                return cloneNode(node, allocator);
            }

            pub fn deinit(node: *Node, allocator: std.mem.Allocator) void {
                deinitNode(node, allocator);
            }
        };

        const SharedNode = Shared(Node, NodeHooks);
        const ChildArray = BoundedArray(Self, capacity);

        root: SharedNode,

        pub fn init(allocator: std.mem.Allocator, context: Context) !Self {
            return initFromSummary(allocator, Ops.zero(context));
        }

        pub fn initFromSummary(allocator: std.mem.Allocator, summary_value: Summary) !Self {
            var summary_copy = summary_value;
            errdefer Ops.deinitSummary(&summary_copy, allocator);
            return .{ .root = try SharedNode.init(allocator, .{ .leaf = .{
                .summary = summary_copy,
                .items = ItemArray.init(),
                .item_summaries = SummaryArray.init(),
            } }) };
        }

        pub fn fromItem(allocator: std.mem.Allocator, item: Item, context: Context) !Self {
            return fromSlice(allocator, &.{item}, context);
        }

        pub fn fromSlice(allocator: std.mem.Allocator, values: []const Item, context: Context) !Self {
            traceBegin(.from_slice, values.len);
            defer traceEnd(.from_slice, values.len);
            if (values.len == 0) return init(allocator, context);

            var nodes: std.ArrayList(Self) = .empty;
            defer deinitTreeList(&nodes, allocator);
            try nodes.ensureTotalCapacity(allocator, std.math.divCeil(usize, values.len, capacity) catch unreachable);

            var offset: usize = 0;
            while (offset < values.len) {
                const chunk_len = balancedChunkLen(values.len - offset);
                const end = offset + chunk_len;
                try nodes.append(allocator, try makeLeaf(allocator, values[offset..end], context));
                offset = end;
            }

            var height: u32 = 0;
            while (nodes.items.len > 1) {
                height += 1;
                var parents: std.ArrayList(Self) = .empty;
                errdefer deinitTreeList(&parents, allocator);
                try parents.ensureTotalCapacity(allocator, std.math.divCeil(usize, nodes.items.len, capacity) catch unreachable);

                var child_offset: usize = 0;
                while (child_offset < nodes.items.len) {
                    const chunk_len = balancedChunkLen(nodes.items.len - child_offset);
                    const child_end = child_offset + chunk_len;
                    try parents.append(allocator, try makeInternal(allocator, nodes.items[child_offset..child_end], height, context));
                    child_offset = child_end;
                }

                for (nodes.items) |*node| node.deinit();
                nodes.deinit(allocator);
                nodes = parents;
            }

            const result = nodes.items[0];
            nodes.items.len = 0;
            return result;
        }

        /// Builds leaf chunks concurrently and assembles them in input order.
        /// Small inputs and unavailable threads use the deterministic serial path.
        pub fn fromParallel(allocator: std.mem.Allocator, values: []const Item, context: Context) !Self {
            traceBegin(.from_parallel, values.len);
            defer traceEnd(.from_parallel, values.len);
            const chunk_count = std.math.divCeil(usize, values.len, capacity) catch unreachable;
            if (values.len == 0 or chunk_count < 2) return fromSlice(allocator, values, context);

            const Slot = struct { tree: ?Self = null, err: ?anyerror = null };
            const Job = struct { values: []const Item, slot: *Slot };
            const Gate = struct {
                allocator: std.mem.Allocator,
                context: Context,
                jobs: []const Job,
                next_job: std.atomic.Value(usize) = std.atomic.Value(usize).init(0),

                fn run(gate: *@This()) void {
                    while (true) {
                        const index = gate.next_job.fetchAdd(1, .monotonic);
                        if (index >= gate.jobs.len) return;
                        const job = gate.jobs[index];
                        job.slot.tree = makeLeaf(gate.allocator, job.values, gate.context) catch |err| {
                            job.slot.err = err;
                            continue;
                        };
                    }
                }
            };

            var slots = try allocator.alloc(Slot, chunk_count);
            defer allocator.free(slots);
            @memset(slots, .{});
            const jobs = try allocator.alloc(Job, chunk_count);
            defer allocator.free(jobs);

            var offset: usize = 0;
            for (jobs, 0..) |*job, index| {
                const chunk_len = balancedChunkLen(values.len - offset);
                job.* = .{ .values = values[offset .. offset + chunk_len], .slot = &slots[index] };
                offset += chunk_len;
            }

            const cpu_count = std.Thread.getCpuCount() catch 1;
            const participant_count = @min(chunk_count, @min(max_parallel_participants, @max(@as(usize, 1), cpu_count)));
            const worker_count = participant_count - 1;
            var threads: [max_parallel_participants - 1]?std.Thread = @splat(null);
            var gate = Gate{ .allocator = allocator, .context = context, .jobs = jobs };
            var spawned: usize = 0;
            while (spawned < worker_count) : (spawned += 1) {
                threads[spawned] = std.Thread.spawn(.{}, Gate.run, .{&gate}) catch break;
            }
            Gate.run(&gate);
            for (threads[0..spawned]) |thread| thread.?.join();
            errdefer for (slots) |*slot| if (slot.tree) |*tree| tree.deinit();
            for (slots) |slot| if (slot.err) |err| return err;

            var nodes: std.ArrayList(Self) = .empty;
            defer deinitTreeList(&nodes, allocator);
            try nodes.ensureTotalCapacity(allocator, chunk_count);
            for (slots) |*slot| {
                try nodes.append(allocator, slot.tree.?);
                slot.tree = null;
            }
            return buildParentLevels(allocator, &nodes, context);
        }

        pub fn clone(self: Self) Self {
            return .{ .root = self.root.clone() };
        }

        /// Appends one item. The operation is transactional: on allocation or
        /// clone failure, this tree remains unchanged.
        pub fn push(self: *Self, item: Item, context: Context) !void {
            traceBegin(.push, 1);
            defer traceEnd(.push, 1);
            try self.extendSlice(&.{item}, context);
        }

        /// Appends a slice while preserving its order.
        pub fn extendSlice(self: *Self, values: []const Item, context: Context) !void {
            if (values.len == 0) return;
            const allocator_value = self.owningAllocator();
            var other = try Self.fromSlice(allocator_value, values, context);
            defer other.deinit();
            try self.append(&other, context);
        }

        /// Appends another tree without consuming it. Both the source tree and
        /// any snapshots remain valid and isolated from later mutations.
        pub fn append(self: *Self, other: *const Self, context: Context) !void {
            traceBegin(.append, other.itemCount());
            defer traceEnd(.append, other.itemCount());
            if (other.isEmpty()) return;
            const allocator_value = self.owningAllocator();
            if (self.isEmpty()) {
                const replacement = other.clone();
                self.deinit();
                self.* = replacement;
                return;
            }

            var joined = try joinTrees(allocator_value, self, other, context);
            defer deinitTreeList(&joined, allocator_value);
            var replacement = if (joined.items.len == 1)
                joined.orderedRemove(0)
            else
                try makeInternal(allocator_value, joined.items, treeHeight(&joined.items[0]) + 1, context);
            errdefer replacement.deinit();
            self.deinit();
            self.* = replacement;
        }

        pub fn getKeyed(self: *const Self, comptime KeyOps: type, key: *const KeyOps.Key) ?*const Item {
            const index = self.lowerBound(KeyOps, key);
            const item = self.itemAt(index) orelse return null;
            return if (KeyOps.compareItemKey(item, key) == .eq) item else null;
        }

        pub fn insertOrReplace(self: *Self, comptime KeyOps: type, item: Item, context: Context) !?Item {
            var edit_values = [_]Edit(Item, KeyOps.Key){.{ .insert = item }};
            var removed = try self.editKeyed(KeyOps, &edit_values, context);
            defer removed.deinit(self.owningAllocator());
            if (removed.items.len == 0) return null;
            const result = removed.items[0];
            removed.items.len = 0;
            return result;
        }

        pub fn removeKeyed(self: *Self, comptime KeyOps: type, key: KeyOps.Key, context: Context) !?Item {
            var edit_values = [_]Edit(Item, KeyOps.Key){.{ .remove = key }};
            var removed = try self.editKeyed(KeyOps, &edit_values, context);
            defer removed.deinit(self.owningAllocator());
            if (removed.items.len == 0) return null;
            const result = removed.items[0];
            removed.items.len = 0;
            return result;
        }

        pub fn editKeyed(self: *Self, comptime KeyOps: type, edits: []const Edit(Item, KeyOps.Key), context: Context) !std.ArrayList(Item) {
            comptime {
                requireDecl(KeyOps, "Key");
                requireDecl(KeyOps, "key");
                requireDecl(KeyOps, "compareKeys");
                requireDecl(KeyOps, "compareItemKey");
            }
            const allocator_value = self.owningAllocator();
            var working: std.ArrayList(Item) = .empty;
            defer deinitItemList(&working, allocator_value);
            try working.ensureTotalCapacity(allocator_value, self.itemCount() + edits.len);
            try appendClonedItems(&working, allocator_value, self);

            var removed: std.ArrayList(Item) = .empty;
            errdefer deinitItemList(&removed, allocator_value);
            for (edits) |*edit_value| {
                const key = switch (edit_value.*) {
                    .insert => |*item_value| KeyOps.key(item_value),
                    .remove => |*key_value| key_value.*,
                };
                const index = lowerBoundItems(KeyOps, working.items, &key);
                if (index < working.items.len and KeyOps.compareItemKey(&working.items[index], &key) == .eq) {
                    const old = working.orderedRemove(index);
                    try removed.append(allocator_value, old);
                }
                switch (edit_value.*) {
                    .insert => |*item_value| {
                        const copy = try Ops.cloneItem(item_value, allocator_value);
                        errdefer {
                            var owned = copy;
                            Ops.deinitItem(&owned, allocator_value);
                        }
                        try working.insert(allocator_value, index, copy);
                    },
                    .remove => {},
                }
            }

            var replacement = try Self.fromOwnedSlice(allocator_value, working.items, context);
            working.items.len = 0;
            errdefer replacement.deinit();
            self.deinit();
            self.* = replacement;
            return removed;
        }

        pub fn parallelExtend(self: *Self, values: []const Item, context: Context) !void {
            if (values.len == 0) return;
            var other = try Self.fromParallel(self.owningAllocator(), values, context);
            defer other.deinit();
            try self.append(&other, context);
        }

        pub fn updateFirst(self: *Self, context: Context, update: anytype) !void {
            if (self.isEmpty()) return;
            try self.updateEndpoint(context, true, update);
        }

        pub fn updateLast(self: *Self, context: Context, update: anytype) !void {
            if (self.isEmpty()) return;
            try self.updateEndpoint(context, false, update);
        }

        pub fn deinit(self: *Self) void {
            self.root.deinit();
            self.* = undefined;
        }

        pub fn getAllocator(self: *const Self) std.mem.Allocator {
            return self.root.allocation.allocator;
        }

        pub fn summary(self: *const Self) *const Summary {
            return nodeSummary(self.root.get());
        }

        pub fn itemCount(self: *const Self) usize {
            return switch (self.root.get().*) {
                .leaf => |leaf| leaf.items.len,
                .internal => |internal| internal.item_count,
            };
        }

        pub fn itemAt(self: *const Self, index: usize) ?*const Item {
            if (index >= self.itemCount()) return null;
            var remaining = index;
            return itemAtNode(self.root.get(), &remaining);
        }

        pub fn itemSummaryAt(self: *const Self, index: usize) ?*const Summary {
            if (index >= self.itemCount()) return null;
            var remaining = index;
            return itemSummaryAtNode(self.root.get(), &remaining);
        }

        pub fn copyRange(self: *const Self, start: usize, end: usize, context: Context) !Self {
            std.debug.assert(start <= end and end <= self.itemCount());
            const allocator_value = self.owningAllocator();
            if (start == end) return init(allocator_value, context);
            if (start == 0 and end == self.itemCount()) return self.clone();

            var pieces: std.ArrayList(Self) = .empty;
            defer deinitTreeList(&pieces, allocator_value);
            try collectRange(self, start, end, allocator_value, context, &pieces);
            std.debug.assert(pieces.items.len > 0);

            // collectRange may produce mixed-height boundary pieces. Joining
            // them in order preserves complete interior subtrees by reference.
            var result = pieces.orderedRemove(0);
            errdefer result.deinit();
            while (pieces.items.len > 0) {
                var next = pieces.orderedRemove(0);
                defer next.deinit();
                try result.append(&next, context);
            }
            return result;
        }

        pub fn FindResult(comptime Dimension: type) type {
            return struct {
                start: Dimension.Value,
                end: Dimension.Value,
                item: ?*const Item,
            };
        }

        pub fn FindWithPrevResult(comptime Dimension: type) type {
            return struct {
                start: Dimension.Value,
                end: Dimension.Value,
                previous: ?*const Item,
                item: ?*const Item,
            };
        }

        pub fn find(self: *const Self, comptime Dimension: type, comptime Target: type, context: Context, target: Target, bias: Bias) FindResult(Dimension) {
            return self.findImpl(Dimension, Target, context, target, bias, false);
        }

        pub fn findExact(self: *const Self, comptime Dimension: type, comptime Target: type, context: Context, target: Target, bias: Bias) FindResult(Dimension) {
            return self.findImpl(Dimension, Target, context, target, bias, true);
        }

        pub fn findWithPrev(self: *const Self, comptime Dimension: type, comptime Target: type, context: Context, target: Target, bias: Bias) FindWithPrevResult(Dimension) {
            const result = self.findImpl(Dimension, Target, context, target, bias, false);
            const previous = if (result.item) |item| blk: {
                var index: usize = 0;
                while (self.itemAt(index)) |candidate| : (index += 1) {
                    if (candidate == item) break :blk if (index == 0) null else self.itemAt(index - 1);
                }
                break :blk null;
            } else null;
            return .{ .start = result.start, .end = result.end, .previous = previous, .item = result.item };
        }

        /// Read-only node access used by the package cursor. The concrete node
        /// representation remains private to SumTree.
        pub const CursorNode = struct {
            tree: *const Self,

            pub fn isLeaf(self: CursorNode) bool {
                return switch (self.tree.root.get().*) {
                    .leaf => true,
                    .internal => false,
                };
            }

            pub fn len(self: CursorNode) usize {
                return switch (self.tree.root.get().*) {
                    .leaf => |leaf| leaf.items.len,
                    .internal => |internal| internal.child_trees.len,
                };
            }

            pub fn summary(self: CursorNode) *const Summary {
                return self.tree.summary();
            }

            pub fn itemCount(self: CursorNode) usize {
                return self.tree.itemCount();
            }

            pub fn child(self: CursorNode, index: usize) CursorNode {
                return switch (self.tree.root.get().*) {
                    .leaf => unreachable,
                    .internal => |*internal| .{ .tree = &internal.child_trees.storage[index] },
                };
            }

            pub fn childSummary(self: CursorNode, index: usize) *const Summary {
                return switch (self.tree.root.get().*) {
                    .leaf => unreachable,
                    .internal => |*internal| &internal.child_summaries.storage[index],
                };
            }

            pub fn item(self: CursorNode, index: usize) *const Item {
                return switch (self.tree.root.get().*) {
                    .leaf => |*leaf| &leaf.items.storage[index],
                    .internal => unreachable,
                };
            }

            pub fn itemSummary(self: CursorNode, index: usize) *const Summary {
                return switch (self.tree.root.get().*) {
                    .leaf => |*leaf| &leaf.item_summaries.storage[index],
                    .internal => unreachable,
                };
            }
        };

        pub fn cursorRoot(self: *const Self) CursorNode {
            return .{ .tree = self };
        }

        pub fn cursor(self: *const Self, comptime Dimension: type, context: Context) @import("cursor.zig").Cursor(Self, Dimension) {
            return @import("cursor.zig").Cursor(Self, Dimension).init(self, context);
        }

        pub fn filter(self: *const Self, comptime Dimension: type, context: Context, filter_value: anytype) @import("cursor.zig").FilterCursor(Self, Dimension, @TypeOf(filter_value)) {
            return @import("cursor.zig").FilterCursor(Self, Dimension, @TypeOf(filter_value)).init(self, context, filter_value);
        }

        pub fn extent(self: *const Self, comptime Dimension: type, context: Context) Dimension.Value {
            comptime {
                requireDecl(Dimension, "Value");
                requireDecl(Dimension, "zero");
                requireDecl(Dimension, "addSummary");
            }
            var result = Dimension.zero(context);
            Dimension.addSummary(&result, self.summary(), context);
            return result;
        }

        pub fn isEmpty(self: *const Self) bool {
            return switch (self.root.get().*) {
                .leaf => |leaf| leaf.items.len == 0,
                .internal => false,
            };
        }

        pub fn first(self: *const Self) ?*const Item {
            var tree = self;
            while (true) {
                switch (tree.root.get().*) {
                    .leaf => |*leaf| return if (leaf.items.len == 0) null else &leaf.items.storage[0],
                    .internal => |*internal| tree = &internal.child_trees.storage[0],
                }
            }
        }

        pub fn last(self: *const Self) ?*const Item {
            var tree = self;
            while (true) {
                switch (tree.root.get().*) {
                    .leaf => |*leaf| return if (leaf.items.len == 0) null else &leaf.items.storage[leaf.items.len - 1],
                    .internal => |*internal| tree = &internal.child_trees.storage[internal.child_trees.len - 1],
                }
            }
        }

        pub fn lastSummary(self: *const Self) ?*const Summary {
            var tree = self;
            while (true) {
                switch (tree.root.get().*) {
                    .leaf => |*leaf| return if (leaf.item_summaries.len == 0) null else &leaf.item_summaries.storage[leaf.item_summaries.len - 1],
                    .internal => |*internal| tree = &internal.child_trees.storage[internal.child_trees.len - 1],
                }
            }
        }

        pub fn iterator(self: *const Self) Iterator {
            return Iterator.init(self);
        }

        pub const Iterator = struct {
            const StackEntry = struct {
                tree: *const Self,
                index: usize,
            };

            root_tree: *const Self,
            stack: BoundedArray(StackEntry, 64),
            started: bool,

            fn init(tree: *const Self) Iterator {
                return .{ .root_tree = tree, .stack = .init(), .started = false };
            }

            pub fn next(self: *Iterator) ?*const Item {
                var descending = false;
                if (!self.started) {
                    self.started = true;
                    self.stack.append(.{ .tree = self.root_tree, .index = 0 }) catch @panic("SumTree iterator exceeded maximum height");
                    descending = true;
                }

                while (self.stack.len > 0) {
                    var entry = &self.stack.storage[self.stack.len - 1];
                    switch (entry.tree.root.get().*) {
                        .leaf => |*leaf| {
                            if (!descending) entry.index += 1;
                            if (entry.index < leaf.items.len) return &leaf.items.storage[entry.index];
                            self.stack.len -= 1;
                            descending = false;
                        },
                        .internal => |*internal| {
                            if (!descending) entry.index += 1;
                            if (entry.index < internal.child_trees.len) {
                                const child = &internal.child_trees.storage[entry.index];
                                self.stack.append(.{ .tree = child, .index = 0 }) catch @panic("SumTree iterator exceeded maximum height");
                                descending = true;
                            } else {
                                self.stack.len -= 1;
                                descending = false;
                            }
                        },
                    }
                }
                return null;
            }
        };

        pub fn validate(self: *const Self, context: Context) !void {
            _ = try validateNode(self, true, context);
        }

        fn validateNode(self: *const Self, is_root: bool, context: Context) !u32 {
            return switch (self.root.get().*) {
                .leaf => |*leaf| blk: {
                    if (leaf.items.len != leaf.item_summaries.len) return error.MismatchedLeafArrays;
                    if (leaf.items.len > capacity) return error.NodeOverCapacity;
                    if (!is_root and leaf.items.len < tree_base) return error.NodeUnderflow;
                    for (leaf.items.constSlice(), leaf.item_summaries.constSlice()) |*item, *cached| {
                        var computed = Ops.summary(item, context);
                        defer Ops.deinitSummary(&computed, self.root.allocation.allocator);
                        if (!Ops.eqlSummary(&computed, cached)) return error.InvalidItemSummary;
                    }
                    var total = try sumSummaries(leaf.item_summaries.constSlice(), self.root.allocation.allocator, context);
                    defer Ops.deinitSummary(&total, self.root.allocation.allocator);
                    if (!Ops.eqlSummary(&total, &leaf.summary)) return error.InvalidNodeSummary;
                    break :blk 0;
                },
                .internal => |*internal| blk: {
                    if (internal.child_trees.len != internal.child_summaries.len) return error.MismatchedInternalArrays;
                    if (internal.child_trees.len == 0) return error.EmptyInternalNode;
                    if (internal.child_trees.len > capacity) return error.NodeOverCapacity;
                    if (!is_root and internal.child_trees.len < tree_base) return error.NodeUnderflow;
                    var computed_count: usize = 0;
                    for (internal.child_trees.constSlice(), internal.child_summaries.constSlice()) |*child, *cached| {
                        computed_count += child.itemCount();
                        const child_height = try child.validateNode(false, context);
                        if (child_height + 1 != internal.height) return error.InvalidHeight;
                        if (!Ops.eqlSummary(child.summary(), cached)) return error.InvalidChildSummary;
                    }
                    if (computed_count != internal.item_count) return error.InvalidItemCount;
                    var total = try sumSummaries(internal.child_summaries.constSlice(), self.root.allocation.allocator, context);
                    defer Ops.deinitSummary(&total, self.root.allocation.allocator);
                    if (!Ops.eqlSummary(&total, &internal.summary)) return error.InvalidNodeSummary;
                    break :blk internal.height;
                },
            };
        }

        fn buildParentLevels(allocator: std.mem.Allocator, nodes: *std.ArrayList(Self), context: Context) !Self {
            var height: u32 = 0;
            while (nodes.items.len > 1) {
                height += 1;
                var parents: std.ArrayList(Self) = .empty;
                errdefer deinitTreeList(&parents, allocator);
                try parents.ensureTotalCapacity(allocator, std.math.divCeil(usize, nodes.items.len, capacity) catch unreachable);
                var offset: usize = 0;
                while (offset < nodes.items.len) {
                    const chunk_len = balancedChunkLen(nodes.items.len - offset);
                    try parents.append(allocator, try makeInternal(allocator, nodes.items[offset .. offset + chunk_len], height, context));
                    offset += chunk_len;
                }
                for (nodes.items) |*node| node.deinit();
                nodes.deinit(allocator);
                nodes.* = parents;
            }
            const result = nodes.items[0];
            nodes.items.len = 0;
            return result;
        }

        fn lowerBound(self: *const Self, comptime KeyOps: type, key: *const KeyOps.Key) usize {
            var low: usize = 0;
            var high = self.itemCount();
            while (low < high) {
                const middle = low + (high - low) / 2;
                if (KeyOps.compareItemKey(self.itemAt(middle).?, key) == .lt)
                    low = middle + 1
                else
                    high = middle;
            }
            return low;
        }

        fn lowerBoundItems(comptime KeyOps: type, items: []const Item, key: *const KeyOps.Key) usize {
            var low: usize = 0;
            var high = items.len;
            while (low < high) {
                const middle = low + (high - low) / 2;
                if (KeyOps.compareItemKey(&items[middle], key) == .lt)
                    low = middle + 1
                else
                    high = middle;
            }
            return low;
        }

        fn findImpl(self: *const Self, comptime Dimension: type, comptime Target: type, context: Context, target: Target, bias: Bias, exact: bool) FindResult(Dimension) {
            comptime {
                requireDecl(Dimension, "Value");
                requireDecl(Dimension, "zero");
                requireDecl(Dimension, "addSummary");
                requireDecl(Target, "compare");
            }
            var tree_cursor = self.cursor(Dimension, context);
            const matched = tree_cursor.seek(Target, target, bias);
            const item = tree_cursor.item();
            if (exact and !matched) return .{ .start = tree_cursor.start().*, .end = tree_cursor.start().*, .item = null };
            return .{ .start = tree_cursor.start().*, .end = tree_cursor.end(), .item = item };
        }

        fn owningAllocator(self: *const Self) std.mem.Allocator {
            return self.getAllocator();
        }

        fn updateEndpoint(self: *Self, context: Context, first_item: bool, update: anytype) !void {
            var replacement = self.clone();
            errdefer replacement.deinit();
            try updateEndpointNode(&replacement, context, first_item, update);
            self.deinit();
            self.* = replacement;
        }

        fn updateEndpointNode(tree: *Self, context: Context, first_item: bool, update: anytype) !void {
            const allocator_value = tree.owningAllocator();
            const node = try tree.root.makeUnique();
            switch (node.*) {
                .leaf => |*leaf| {
                    const index: usize = if (first_item) 0 else leaf.items.len - 1;
                    try invokeUpdate(update, &leaf.items.storage[index]);
                    replaceSummary(&leaf.item_summaries.storage[index], Ops.summary(&leaf.items.storage[index], context), allocator_value);
                    replaceSummary(&leaf.summary, try sumSummaries(leaf.item_summaries.constSlice(), allocator_value, context), allocator_value);
                },
                .internal => |*internal| {
                    const index: usize = if (first_item) 0 else internal.child_trees.len - 1;
                    try updateEndpointNode(&internal.child_trees.storage[index], context, first_item, update);
                    replaceSummary(&internal.child_summaries.storage[index], try Ops.cloneSummary(internal.child_trees.storage[index].summary(), allocator_value), allocator_value);
                    replaceSummary(&internal.summary, try sumSummaries(internal.child_summaries.constSlice(), allocator_value, context), allocator_value);
                },
            }
        }

        fn replaceSummary(old: *Summary, new: Summary, allocator_value: std.mem.Allocator) void {
            Ops.deinitSummary(old, allocator_value);
            old.* = new;
        }

        fn appendClonedItems(list: *std.ArrayList(Item), allocator_value: std.mem.Allocator, tree: *const Self) !void {
            var iterator_value = tree.iterator();
            while (iterator_value.next()) |item| {
                const copy = try Ops.cloneItem(item, allocator_value);
                errdefer {
                    var owned = copy;
                    Ops.deinitItem(&owned, allocator_value);
                }
                try list.append(allocator_value, copy);
            }
        }

        fn fromOwnedSlice(allocator_value: std.mem.Allocator, values: []Item, context: Context) !Self {
            const result = try fromSlice(allocator_value, values, context);
            for (values) |*value| Ops.deinitItem(value, allocator_value);
            return result;
        }

        fn invokeUpdate(update: anytype, item: *Item) !void {
            const result = @call(.auto, update, .{item});
            if (@typeInfo(@TypeOf(result)) == .error_union) try result;
        }

        fn itemAtNode(node: *const Node, remaining: *usize) ?*const Item {
            return switch (node.*) {
                .leaf => |*leaf| if (remaining.* < leaf.items.len) &leaf.items.storage[remaining.*] else null,
                .internal => |*internal| blk: {
                    for (internal.child_trees.constSlice()) |*child| {
                        const count = child.itemCount();
                        if (remaining.* < count) break :blk itemAtNode(child.root.get(), remaining);
                        remaining.* -= count;
                    }
                    break :blk null;
                },
            };
        }

        fn itemSummaryAtNode(node: *const Node, remaining: *usize) ?*const Summary {
            return switch (node.*) {
                .leaf => |*leaf| if (remaining.* < leaf.item_summaries.len) &leaf.item_summaries.storage[remaining.*] else null,
                .internal => |*internal| blk: {
                    for (internal.child_trees.constSlice()) |*child| {
                        const count = child.itemCount();
                        if (remaining.* < count) break :blk itemSummaryAtNode(child.root.get(), remaining);
                        remaining.* -= count;
                    }
                    break :blk null;
                },
            };
        }

        fn countItems(node: *const Node) usize {
            return switch (node.*) {
                .leaf => |leaf| leaf.items.len,
                .internal => |internal| blk: {
                    var count: usize = 0;
                    for (internal.child_trees.constSlice()) |*child| count += child.itemCount();
                    break :blk count;
                },
            };
        }

        fn treeHeight(tree: *const Self) u32 {
            return switch (tree.root.get().*) {
                .leaf => 0,
                .internal => |internal| internal.height,
            };
        }

        fn joinTrees(allocator_value: std.mem.Allocator, left: *const Self, right: *const Self, context: Context) !std.ArrayList(Self) {
            const left_height = treeHeight(left);
            const right_height = treeHeight(right);
            if (left_height == right_height) return joinEqualHeight(allocator_value, left, right, context);

            var children: std.ArrayList(Self) = .empty;
            errdefer deinitTreeList(&children, allocator_value);
            if (left_height > right_height) {
                const internal = &left.root.get().internal;
                try children.ensureTotalCapacity(allocator_value, internal.child_trees.len + 1);
                for (internal.child_trees.constSlice()[0 .. internal.child_trees.len - 1]) |child| try children.append(allocator_value, child.clone());
                var boundary = try joinTrees(allocator_value, &internal.child_trees.storage[internal.child_trees.len - 1], right, context);
                defer deinitTreeList(&boundary, allocator_value);
                try children.appendSlice(allocator_value, boundary.items);
                boundary.items.len = 0;
                return packChildren(allocator_value, &children, left_height, context);
            }

            const internal = &right.root.get().internal;
            try children.ensureTotalCapacity(allocator_value, internal.child_trees.len + 1);
            var boundary = try joinTrees(allocator_value, left, &internal.child_trees.storage[0], context);
            defer deinitTreeList(&boundary, allocator_value);
            try children.appendSlice(allocator_value, boundary.items);
            boundary.items.len = 0;
            for (internal.child_trees.constSlice()[1..]) |child| try children.append(allocator_value, child.clone());
            return packChildren(allocator_value, &children, right_height, context);
        }

        fn joinEqualHeight(allocator_value: std.mem.Allocator, left: *const Self, right: *const Self, context: Context) !std.ArrayList(Self) {
            var result: std.ArrayList(Self) = .empty;
            errdefer deinitTreeList(&result, allocator_value);
            const height = treeHeight(left);
            if (height == 0) {
                const left_leaf = &left.root.get().leaf;
                const right_leaf = &right.root.get().leaf;
                const total = left_leaf.items.len + right_leaf.items.len;
                if (total > capacity and left_leaf.items.len >= tree_base and right_leaf.items.len >= tree_base) {
                    try appendOwnedTree(&result, allocator_value, left.clone());
                    try appendOwnedTree(&result, allocator_value, right.clone());
                    return result;
                }
                var values: [capacity * 2]Item = undefined;
                @memcpy(values[0..left_leaf.items.len], left_leaf.items.constSlice());
                @memcpy(values[left_leaf.items.len..total], right_leaf.items.constSlice());
                const first_len = balancedChunkLen(total);
                try appendOwnedTree(&result, allocator_value, try makeLeaf(allocator_value, values[0..first_len], context));
                if (first_len < total) try appendOwnedTree(&result, allocator_value, try makeLeaf(allocator_value, values[first_len..total], context));
                return result;
            }

            const left_internal = &left.root.get().internal;
            const right_internal = &right.root.get().internal;
            var children: std.ArrayList(Self) = .empty;
            defer deinitTreeList(&children, allocator_value);
            try children.ensureTotalCapacity(allocator_value, left_internal.child_trees.len + right_internal.child_trees.len);
            for (left_internal.child_trees.constSlice()) |child| try children.append(allocator_value, child.clone());
            for (right_internal.child_trees.constSlice()) |child| try children.append(allocator_value, child.clone());
            return packChildren(allocator_value, &children, height, context);
        }

        fn packChildren(allocator_value: std.mem.Allocator, children: *std.ArrayList(Self), parent_height: u32, context: Context) !std.ArrayList(Self) {
            var result: std.ArrayList(Self) = .empty;
            errdefer deinitTreeList(&result, allocator_value);
            if (children.items.len <= capacity) {
                try appendOwnedTree(&result, allocator_value, try makeInternal(allocator_value, children.items, parent_height, context));
            } else {
                const first_len = balancedChunkLen(children.items.len);
                try appendOwnedTree(&result, allocator_value, try makeInternal(allocator_value, children.items[0..first_len], parent_height, context));
                try appendOwnedTree(&result, allocator_value, try makeInternal(allocator_value, children.items[first_len..], parent_height, context));
            }
            deinitTreeList(children, allocator_value);
            children.* = .empty;
            return result;
        }

        fn appendOwnedTree(output: *std.ArrayList(Self), allocator_value: std.mem.Allocator, tree: Self) !void {
            var owned = tree;
            errdefer owned.deinit();
            try output.append(allocator_value, owned);
        }

        fn collectRange(tree: *const Self, start: usize, end: usize, allocator_value: std.mem.Allocator, context: Context, output: *std.ArrayList(Self)) !void {
            const count = tree.itemCount();
            std.debug.assert(start < end and end <= count);
            if (start == 0 and end == count) {
                var copy = tree.clone();
                errdefer copy.deinit();
                try output.append(allocator_value, copy);
                return;
            }
            switch (tree.root.get().*) {
                .leaf => |*leaf| {
                    var copy = try makeLeaf(allocator_value, leaf.items.constSlice()[start..end], context);
                    errdefer copy.deinit();
                    try output.append(allocator_value, copy);
                },
                .internal => |*internal| {
                    var offset: usize = 0;
                    for (internal.child_trees.constSlice()) |*child| {
                        const child_count = child.itemCount();
                        const child_end = offset + child_count;
                        if (start < child_end and end > offset) {
                            try collectRange(child, start -| offset, @min(end, child_end) - offset, allocator_value, context, output);
                        }
                        offset = child_end;
                        if (offset >= end) break;
                    }
                },
            }
        }

        fn balancedChunkLen(remaining: usize) usize {
            if (remaining <= capacity) return remaining;
            const after_full = remaining - capacity;
            if (after_full < tree_base) return capacity - (tree_base - after_full);
            return capacity;
        }

        fn makeLeaf(allocator: std.mem.Allocator, values: []const Item, context: Context) !Self {
            var items = ItemArray.init();
            var summaries = SummaryArray.init();
            errdefer {
                for (items.slice()) |*item| Ops.deinitItem(item, allocator);
                for (summaries.slice()) |*value| Ops.deinitSummary(value, allocator);
            }

            for (values) |*value| {
                const item_copy = try Ops.cloneItem(value, allocator);
                items.append(item_copy) catch unreachable;
                errdefer {
                    items.len -= 1;
                    var owned = item_copy;
                    Ops.deinitItem(&owned, allocator);
                }
                const item_summary = Ops.summary(value, context);
                summaries.append(item_summary) catch unreachable;
            }

            const total = try sumSummaries(summaries.constSlice(), allocator, context);
            return .{ .root = try SharedNode.init(allocator, .{ .leaf = .{
                .summary = total,
                .items = items,
                .item_summaries = summaries,
            } }) };
        }

        fn makeInternal(allocator: std.mem.Allocator, children: []const Self, height: u32, context: Context) !Self {
            var child_trees = ChildArray.init();
            var summaries = SummaryArray.init();
            errdefer {
                for (child_trees.slice()) |*child| child.deinit();
                for (summaries.slice()) |*value| Ops.deinitSummary(value, allocator);
            }

            for (children) |child| {
                child_trees.append(child.clone()) catch unreachable;
                const summary_copy = try Ops.cloneSummary(child.summary(), allocator);
                summaries.append(summary_copy) catch unreachable;
            }
            const total = try sumSummaries(summaries.constSlice(), allocator, context);
            var item_count: usize = 0;
            for (children) |*child| item_count += child.itemCount();
            return .{ .root = try SharedNode.init(allocator, .{ .internal = .{
                .height = height,
                .item_count = item_count,
                .summary = total,
                .child_summaries = summaries,
                .child_trees = child_trees,
            } }) };
        }

        fn sumSummaries(values: []const Summary, allocator: std.mem.Allocator, context: Context) !Summary {
            if (values.len == 0) return Ops.zero(context);
            var total = try Ops.cloneSummary(&values[0], allocator);
            errdefer Ops.deinitSummary(&total, allocator);
            for (values[1..]) |*value| Ops.addSummary(&total, value, context);
            return total;
        }

        fn nodeSummary(node: *const Node) *const Summary {
            return switch (node.*) {
                .leaf => |*leaf| &leaf.summary,
                .internal => |*internal| &internal.summary,
            };
        }

        fn cloneNode(node: *const Node, allocator: std.mem.Allocator) !Node {
            return switch (node.*) {
                .leaf => |*leaf| blk: {
                    var items = ItemArray.init();
                    var summaries = SummaryArray.init();
                    errdefer {
                        for (items.slice()) |*item| Ops.deinitItem(item, allocator);
                        for (summaries.slice()) |*value| Ops.deinitSummary(value, allocator);
                    }
                    for (leaf.items.constSlice()) |*item| items.append(try Ops.cloneItem(item, allocator)) catch unreachable;
                    for (leaf.item_summaries.constSlice()) |*value| summaries.append(try Ops.cloneSummary(value, allocator)) catch unreachable;
                    break :blk .{ .leaf = .{
                        .summary = try Ops.cloneSummary(&leaf.summary, allocator),
                        .items = items,
                        .item_summaries = summaries,
                    } };
                },
                .internal => |*internal| blk: {
                    var children = ChildArray.init();
                    var summaries = SummaryArray.init();
                    errdefer {
                        for (children.slice()) |*child| child.deinit();
                        for (summaries.slice()) |*value| Ops.deinitSummary(value, allocator);
                    }
                    for (internal.child_trees.constSlice()) |child| children.append(child.clone()) catch unreachable;
                    for (internal.child_summaries.constSlice()) |*value| summaries.append(try Ops.cloneSummary(value, allocator)) catch unreachable;
                    break :blk .{ .internal = .{
                        .height = internal.height,
                        .item_count = internal.item_count,
                        .summary = try Ops.cloneSummary(&internal.summary, allocator),
                        .child_summaries = summaries,
                        .child_trees = children,
                    } };
                },
            };
        }

        fn deinitNode(node: *Node, allocator: std.mem.Allocator) void {
            switch (node.*) {
                .leaf => |*leaf| {
                    for (leaf.items.slice()) |*item| Ops.deinitItem(item, allocator);
                    for (leaf.item_summaries.slice()) |*value| Ops.deinitSummary(value, allocator);
                    Ops.deinitSummary(&leaf.summary, allocator);
                },
                .internal => |*internal| {
                    for (internal.child_trees.slice()) |*child| child.deinit();
                    for (internal.child_summaries.slice()) |*value| Ops.deinitSummary(value, allocator);
                    Ops.deinitSummary(&internal.summary, allocator);
                },
            }
        }

        fn deinitTreeList(list: *std.ArrayList(Self), allocator_value: std.mem.Allocator) void {
            for (list.items) |*tree| tree.deinit();
            list.deinit(allocator_value);
        }

        const TraceOperation = enum { from_slice, from_parallel, push, append };

        fn traceBegin(operation: TraceOperation, item_count: usize) void {
            if (@hasDecl(Ops, "traceBegin")) Ops.traceBegin(operation, item_count);
        }

        fn traceEnd(operation: TraceOperation, item_count: usize) void {
            if (@hasDecl(Ops, "traceEnd")) Ops.traceEnd(operation, item_count);
        }

        fn deinitItemList(list: *std.ArrayList(Item), allocator_value: std.mem.Allocator) void {
            for (list.items) |*item| Ops.deinitItem(item, allocator_value);
            list.deinit(allocator_value);
        }
    };
}

fn requireDecl(comptime T: type, comptime name: []const u8) void {
    if (!@hasDecl(T, name)) @compileError(@typeName(T) ++ " must declare " ++ name);
}
