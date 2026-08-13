const std = @import("std");
const clock = @import("clock");
const rope = @import("rope");
const sum_tree = @import("sum_tree");
const fragment = @import("fragment.zig");
const BufferId = @import("buffer_id.zig").BufferId;
const Locator = @import("locator.zig").Locator;
const LineEnding = @import("line_ending.zig").LineEnding;
const undo_map_mod = @import("undo_map.zig");
const UndoMap = undo_map_mod.UndoMap;
const Patch = @import("patch.zig").Patch(usize);
const subscription = @import("subscription.zig");
const waiter = @import("waiter.zig");
const regex_mod = @import("regex.zig");
const LineIndent = @import("line_indent.zig").LineIndent;
const OperationQueue = @import("operation_queue.zig").OperationQueue;

const PatchOps = struct {
    pub fn init(allocator: std.mem.Allocator) !Patch {
        return Patch.empty(allocator);
    }
    pub fn clone(value: *const Patch, allocator: std.mem.Allocator) !Patch {
        return value.clone(allocator);
    }
    pub fn deinit(value: *Patch, _: std.mem.Allocator) void {
        value.deinit();
    }
    pub fn combine(current: *const Patch, update: *const Patch, _: std.mem.Allocator) !Patch {
        return current.compose(update.edits());
    }
};
pub const BufferSubscription = subscription.Subscription(Patch, PatchOps);
const PatchTopic = subscription.Topic(Patch, PatchOps);

pub const max_insertion_len: usize = std.math.maxInt(u32);

pub const AnchorRange = struct { start: Anchor, end: Anchor };

pub const Anchor = struct {
    timestamp: clock.Lamport,
    offset: u32,
    bias: sum_tree.Bias,
    buffer_id: BufferId,

    pub fn init(timestamp: clock.Lamport, offset: u32, bias: sum_tree.Bias, buffer_id: BufferId) Anchor {
        return .{ .timestamp = timestamp, .offset = offset, .bias = bias, .buffer_id = buffer_id };
    }
    pub fn min(buffer_id: BufferId) Anchor {
        return init(clock.Lamport.MIN, 0, .left, buffer_id);
    }
    pub fn max(buffer_id: BufferId) Anchor {
        return init(clock.Lamport.MAX, std.math.maxInt(u32), .right, buffer_id);
    }
    pub fn isMin(self: Anchor) bool {
        return self.timestamp.eql(clock.Lamport.MIN) and self.offset == 0 and self.bias == .left;
    }
    pub fn isMax(self: Anchor) bool {
        return self.timestamp.eql(clock.Lamport.MAX) and self.offset == std.math.maxInt(u32) and self.bias == .right;
    }
    pub fn opaqueId(self: Anchor) [20]u8 {
        var bytes: [20]u8 = @splat(0);
        std.mem.writeInt(u64, bytes[0..8], self.buffer_id.get(), .little);
        std.mem.writeInt(u32, bytes[8..12], self.offset, .little);
        std.mem.writeInt(u32, bytes[12..16], self.timestamp.value, .little);
        std.mem.writeInt(u16, bytes[16..18], self.timestamp.replica_id.asU16(), .little);
        bytes[18] = @intFromEnum(self.bias);
        return bytes;
    }
};

pub const BufferSnapshot = struct {
    allocator: std.mem.Allocator,
    visible_text: rope.Rope,
    deleted_text: rope.Rope,
    fragments: fragment.FragmentTree,
    insertions: fragment.InsertionTree,
    undo_map: UndoMap,
    version: clock.Global,
    remote_id: BufferId,
    replica_id: clock.ReplicaId,
    line_ending: LineEnding,

    pub fn deinit(self: *BufferSnapshot) void {
        self.visible_text.deinit();
        self.deleted_text.deinit();
        self.fragments.deinit();
        self.insertions.deinit();
        self.undo_map.deinit();
        self.version.deinit();
        self.* = undefined;
    }

    pub fn clone(self: *const BufferSnapshot, allocator: std.mem.Allocator) !BufferSnapshot {
        return .{
            .allocator = allocator,
            .visible_text = self.visible_text.clone(),
            .deleted_text = self.deleted_text.clone(),
            .fragments = self.fragments.clone(),
            .insertions = self.insertions.clone(),
            .undo_map = try self.undo_map.clone(allocator),
            .version = try self.version.clone(allocator),
            .remote_id = self.remote_id,
            .replica_id = self.replica_id,
            .line_ending = self.line_ending,
        };
    }

    pub fn len(self: *const BufferSnapshot) usize {
        return self.visible_text.len();
    }
    pub fn isEmpty(self: *const BufferSnapshot) bool {
        return self.visible_text.isEmpty();
    }
    pub fn hasEditsSince(self: *const BufferSnapshot, since: *const clock.Global) bool {
        return self.fragments.summary().max_version.changedSince(since);
    }
    pub fn lineLen(self: *const BufferSnapshot, row: u32) u32 {
        return self.visible_text.lineLen(row);
    }
    pub fn maxPoint(self: *const BufferSnapshot) rope.Point {
        return self.visible_text.maxPoint();
    }
    pub fn maxPointUtf16(self: *const BufferSnapshot) rope.PointUtf16 {
        return self.visible_text.maxPointUtf16();
    }
    pub fn pointToOffset(self: *const BufferSnapshot, point: rope.Point) usize {
        return self.visible_text.pointToOffset(point);
    }
    pub fn offsetToPoint(self: *const BufferSnapshot, offset: usize) rope.Point {
        return self.visible_text.offsetToPoint(offset);
    }
    pub fn offsetToPointUtf16(self: *const BufferSnapshot, offset: usize) rope.PointUtf16 {
        return self.visible_text.offsetToPointUtf16(offset);
    }
    pub fn pointToOffsetUtf16(self: *const BufferSnapshot, point: rope.Point) rope.OffsetUtf16 {
        return self.visible_text.pointToOffsetUtf16(point);
    }
    pub fn clipOffset(self: *const BufferSnapshot, offset: usize, bias: sum_tree.Bias) usize {
        return self.visible_text.clipOffset(offset, bias);
    }
    pub fn clipPoint(self: *const BufferSnapshot, point: rope.Point, bias: sum_tree.Bias) rope.Point {
        return self.visible_text.clipPoint(point, bias);
    }
    pub fn text(self: *const BufferSnapshot, allocator: std.mem.Allocator) ![]u8 {
        return self.visible_text.toOwnedSlice(allocator);
    }
    pub fn textForRange(self: *const BufferSnapshot, allocator: std.mem.Allocator, start: usize, end: usize) ![]u8 {
        if (start > end or end > self.len()) return error.InvalidRange;
        var slice = try self.visible_text.sliceBytes(.{ .start = start, .end = end });
        defer slice.deinit();
        return slice.toOwnedSlice(allocator);
    }
    pub fn containsStrAt(self: *const BufferSnapshot, allocator: std.mem.Allocator, position: usize, needle: []const u8) !bool {
        if (position > self.len() or needle.len > self.len() - position) return false;
        const value = try self.textForRange(allocator, position, position + needle.len);
        defer allocator.free(value);
        return std.mem.eql(u8, value, needle);
    }
    pub fn findRegex(self: *const BufferSnapshot, allocator: std.mem.Allocator, matcher: regex_mod.RegexMatcher, start: usize) !?regex_mod.Match {
        const value = try self.text(allocator);
        defer allocator.free(value);
        return matcher.find(value, start);
    }
    pub fn findAllRegex(self: *const BufferSnapshot, allocator: std.mem.Allocator, matcher: regex_mod.RegexMatcher, start: usize) !std.ArrayList(regex_mod.Match) {
        const value = try self.text(allocator);
        defer allocator.free(value);
        var result: std.ArrayList(regex_mod.Match) = .empty;
        errdefer result.deinit(allocator);
        var iterator = regex_mod.matches(matcher, value, start);
        while (iterator.next()) |item| try result.append(allocator, item);
        return result;
    }
    pub fn lineIndent(self: *const BufferSnapshot, allocator: std.mem.Allocator, row: u32) !LineIndent {
        const start = self.pointToOffset(.new(row, 0));
        const end = self.pointToOffset(.new(row +| 1, 0));
        const value = try self.textForRange(allocator, start, end);
        defer allocator.free(value);
        return LineIndent.parse(value);
    }
    pub fn chunks(self: *const BufferSnapshot) rope.Chunks {
        return self.visible_text.chunksIterator();
    }
    pub fn chunksInRange(self: *const BufferSnapshot, start: usize, end: usize) rope.Chunks {
        return self.visible_text.chunksInRange(.{ .start = start, .end = end });
    }
    pub fn bytesInRange(self: *const BufferSnapshot, start: usize, end: usize) rope.Bytes {
        return self.visible_text.bytesInRange(.{ .start = start, .end = end });
    }
    pub fn scalars(self: *const BufferSnapshot) rope.Scalars {
        return self.visible_text.scalars();
    }
    pub fn scalarsAt(self: *const BufferSnapshot, start: usize) rope.Scalars {
        return self.visible_text.scalarsAt(start);
    }
    pub fn reversedScalarsAt(self: *const BufferSnapshot, end: usize) rope.Scalars {
        return self.visible_text.reversedScalarsAt(end);
    }

    pub fn anchorRangeInside(self: *const BufferSnapshot, start: usize, end: usize) AnchorRange {
        return .{ .start = self.anchorAfter(start), .end = self.anchorBefore(end) };
    }
    pub fn anchorRangeOutside(self: *const BufferSnapshot, start: usize, end: usize) AnchorRange {
        return .{ .start = self.anchorBefore(start), .end = self.anchorAfter(end) };
    }
    pub fn anchorBefore(self: *const BufferSnapshot, offset: usize) Anchor {
        return self.anchorAt(offset, .left);
    }
    pub fn anchorAfter(self: *const BufferSnapshot, offset: usize) Anchor {
        return self.anchorAt(offset, .right);
    }
    pub fn anchorAt(self: *const BufferSnapshot, raw_offset: usize, bias: sum_tree.Bias) Anchor {
        const offset = self.clipOffset(raw_offset, bias);
        if (offset == 0 and bias == .left) return Anchor.min(self.remote_id);
        if (offset == self.len() and bias == .right) return Anchor.max(self.remote_id);
        var visible_start: usize = 0;
        var iterator = self.fragments.iterator();
        while (iterator.next()) |item| {
            if (!item.visible) continue;
            const visible_end = visible_start + item.len;
            if (offset < visible_end or (offset == visible_end and bias == .left)) {
                return Anchor.init(item.timestamp, item.insertion_offset + @as(u32, @intCast(offset - visible_start)), bias, self.remote_id);
            }
            visible_start = visible_end;
        }
        return Anchor.max(self.remote_id);
    }

    pub fn offsetForAnchor(self: *const BufferSnapshot, anchor: Anchor) ?usize {
        if (!anchor.buffer_id.eql(self.remote_id)) return null;
        if (anchor.isMin()) return 0;
        if (anchor.isMax()) return self.len();
        var visible_offset: usize = 0;
        var fallback: ?usize = null;
        var iterator = self.fragments.iterator();
        while (iterator.next()) |item| {
            if (item.timestamp.eql(anchor.timestamp) and anchor.offset >= item.insertion_offset and anchor.offset <= item.insertion_offset + item.len) {
                const relative = anchor.offset - item.insertion_offset;
                if (item.visible) return visible_offset + relative;
                fallback = visible_offset;
            }
            if (item.visible) visible_offset += item.len;
        }
        return fallback;
    }

    pub fn isAnchorValid(self: *const BufferSnapshot, anchor: Anchor) bool {
        if (!anchor.buffer_id.eql(self.remote_id)) return false;
        if (anchor.isMin() or anchor.isMax()) return true;
        var iterator = self.fragments.iterator();
        while (iterator.next()) |item| {
            if (item.visible and item.timestamp.eql(anchor.timestamp) and anchor.offset >= item.insertion_offset and anchor.offset <= item.insertion_offset + item.len) return true;
        }
        return false;
    }

    pub fn validate(self: *const BufferSnapshot) !void {
        try self.visible_text.validate();
        try self.deleted_text.validate();
        try self.fragments.validate(null);
        try self.insertions.validate({});
        if (self.visible_text.len() != self.fragments.summary().text.visible) return error.VisibleLengthMismatch;
        if (self.deleted_text.len() != self.fragments.summary().text.deleted) return error.DeletedLengthMismatch;
        var previous_id: ?*const Locator = null;
        var iterator = self.fragments.iterator();
        while (iterator.next()) |item| {
            if (item.len == 0) return error.EmptyFragment;
            if (previous_id) |id| if (id.order(&item.id) != .lt) return error.UnorderedFragments;
            previous_id = &item.id;
            if (!self.version.observed(item.timestamp)) return error.UnobservedInsertion;
        }
        var previous_key: ?fragment.InsertionFragmentKey = null;
        var insertions = self.insertions.iterator();
        while (insertions.next()) |item| {
            const key = fragment.InsertionKeyOps.key(item);
            if (previous_key) |prior| if (prior.order(key) != .lt) return error.UnorderedInsertions;
            previous_key = key;
        }
    }
};

pub const InputEdit = struct { start: usize, end: usize, new_text: []const u8 };
pub const FullRange = struct { start: fragment.FullOffset, end: fragment.FullOffset };

pub const EditOperation = struct {
    allocator: std.mem.Allocator,
    timestamp: clock.Lamport,
    version: clock.Global,
    ranges: std.ArrayList(FullRange) = .empty,
    new_text: std.ArrayList([]u8) = .empty,

    pub fn clone(self: *const EditOperation, allocator: std.mem.Allocator) !EditOperation {
        var result = EditOperation{ .allocator = allocator, .timestamp = self.timestamp, .version = try self.version.clone(allocator) };
        errdefer result.deinit();
        try result.ranges.appendSlice(allocator, self.ranges.items);
        try result.new_text.ensureTotalCapacity(allocator, self.new_text.items.len);
        for (self.new_text.items) |item| try result.new_text.append(allocator, try allocator.dupe(u8, item));
        return result;
    }

    pub fn deinit(self: *EditOperation) void {
        for (self.new_text.items) |item| self.allocator.free(item);
        self.ranges.deinit(self.allocator);
        self.new_text.deinit(self.allocator);
        self.version.deinit();
        self.* = undefined;
    }
};

pub const UndoOperation = struct {
    allocator: std.mem.Allocator,
    timestamp: clock.Lamport,
    version: clock.Global,
    counts: std.ArrayList(undo_map_mod.Count) = .empty,

    pub fn clone(self: *const UndoOperation, allocator: std.mem.Allocator) !UndoOperation {
        var result = UndoOperation{ .allocator = allocator, .timestamp = self.timestamp, .version = try self.version.clone(allocator) };
        errdefer result.deinit();
        try result.counts.appendSlice(allocator, self.counts.items);
        return result;
    }

    pub fn deinit(self: *UndoOperation) void {
        self.counts.deinit(self.allocator);
        self.version.deinit();
        self.* = undefined;
    }
};

pub const Operation = union(enum) {
    edit: EditOperation,
    undo: UndoOperation,
    pub fn clone(self: *const Operation, allocator: std.mem.Allocator) !Operation {
        return switch (self.*) {
            .edit => |*value| .{ .edit = try value.clone(allocator) },
            .undo => |*value| .{ .undo = try value.clone(allocator) },
        };
    }
    pub fn deinit(self: *Operation) void {
        switch (self.*) {
            .edit => |*value| value.deinit(),
            .undo => |*value| value.deinit(),
        }
        self.* = undefined;
    }
    pub fn timestamp(self: *const Operation) clock.Lamport {
        return switch (self.*) {
            .edit => |value| value.timestamp,
            .undo => |value| value.timestamp,
        };
    }
};

const OperationOps = struct {
    pub fn timestamp(value: *const Operation) clock.Lamport {
        return value.timestamp();
    }
    pub fn clone(value: *const Operation, allocator: std.mem.Allocator) !Operation {
        return value.clone(allocator);
    }
    pub fn deinit(value: *Operation, _: std.mem.Allocator) void {
        value.deinit();
    }
};
const DeferredOperations = OperationQueue(Operation, OperationOps);

pub const TransactionId = clock.Lamport;

pub const Transaction = struct {
    allocator: std.mem.Allocator,
    id: TransactionId,
    edit_ids: std.ArrayList(clock.Lamport) = .empty,
    start: clock.Global,

    pub fn clone(self: *const Transaction, allocator: std.mem.Allocator) !Transaction {
        var result = Transaction{ .allocator = allocator, .id = self.id, .start = try self.start.clone(allocator) };
        errdefer result.deinit();
        try result.edit_ids.appendSlice(allocator, self.edit_ids.items);
        return result;
    }
    pub fn deinit(self: *Transaction) void {
        self.edit_ids.deinit(self.allocator);
        self.start.deinit();
        self.* = undefined;
    }
    pub fn mergeIn(self: *Transaction, other: *const Transaction) !void {
        try self.edit_ids.appendSlice(self.allocator, other.edit_ids.items);
    }
};

pub const HistoryEntry = struct {
    transaction: Transaction,
    first_edit_at: u64,
    last_edit_at: u64,
    suppress_grouping: bool = false,

    pub fn transactionId(self: *const HistoryEntry) TransactionId {
        return self.transaction.id;
    }
    fn deinit(self: *HistoryEntry) void {
        self.transaction.deinit();
        self.* = undefined;
    }
};

pub const History = struct {
    allocator: std.mem.Allocator,
    base_text: rope.Rope,
    operations: std.ArrayList(Operation) = .empty,
    undo_stack: std.ArrayList(HistoryEntry) = .empty,
    redo_stack: std.ArrayList(HistoryEntry) = .empty,
    transaction_depth: usize = 0,
    group_interval: u64 = 0,

    fn init(allocator: std.mem.Allocator, base_text: *const rope.Rope) History {
        return .{ .allocator = allocator, .base_text = base_text.clone() };
    }
    fn deinit(self: *History) void {
        self.base_text.deinit();
        for (self.operations.items) |*op| op.deinit();
        self.operations.deinit(self.allocator);
        for (self.undo_stack.items) |*entry| entry.deinit();
        self.undo_stack.deinit(self.allocator);
        for (self.redo_stack.items) |*entry| entry.deinit();
        self.redo_stack.deinit(self.allocator);
        self.* = undefined;
    }
    fn pushOperation(self: *History, operation: *const Operation) !void {
        for (self.operations.items) |*existing| if (existing.timestamp().eql(operation.timestamp())) return;
        try self.operations.append(self.allocator, try operation.clone(self.allocator));
    }
    fn clearRedo(self: *History) void {
        for (self.redo_stack.items) |*entry| entry.deinit();
        self.redo_stack.clearRetainingCapacity();
    }
};

pub const EditedBufferSnapshot = struct {
    base_version: clock.Global,
    snapshot: BufferSnapshot,
    did_edit: bool,

    pub fn deinit(self: *EditedBufferSnapshot) void {
        self.base_version.deinit();
        self.snapshot.deinit();
        self.* = undefined;
    }
};

pub const Buffer = struct {
    allocator: std.mem.Allocator,
    snapshot_value: BufferSnapshot,
    lamport_clock: clock.Lamport,
    subscriptions: PatchTopic,
    deferred_operations: DeferredOperations,
    history: History,
    waiters: std.ArrayList(*waiter.State) = .empty,

    pub fn init(allocator: std.mem.Allocator, replica_id: clock.ReplicaId, buffer_id: BufferId, source: []const u8) !Buffer {
        const ending = LineEnding.detect(source);
        var normalized = try LineEnding.normalize(allocator, source);
        defer normalized.deinit(allocator);
        return initNormalized(allocator, replica_id, buffer_id, ending, normalized.slice());
    }

    pub fn initNormalized(allocator: std.mem.Allocator, replica_id: clock.ReplicaId, buffer_id: BufferId, ending: LineEnding, source: []const u8) !Buffer {
        if (!std.unicode.utf8ValidateSlice(source)) return error.InvalidUtf8;
        var visible = try rope.Rope.initText(allocator, source);
        errdefer visible.deinit();
        var deleted = try rope.Rope.init(allocator);
        errdefer deleted.deinit();
        var fragments = try fragment.FragmentTree.init(allocator, null);
        errdefer fragments.deinit();
        var insertions = try fragment.InsertionTree.init(allocator, {});
        errdefer insertions.deinit();
        var version = clock.Global.init(allocator);
        errdefer version.deinit();
        const insertion_timestamp = clock.Lamport.new(clock.ReplicaId.LOCAL);
        var lamport_clock = clock.Lamport.new(replica_id);
        if (source.len != 0) {
            lamport_clock.observe(insertion_timestamp);
            try version.observe(insertion_timestamp);
            var previous = try Locator.min(allocator);
            defer previous.deinit();
            var maximum = try Locator.max(allocator);
            defer maximum.deinit();
            var text_offset: usize = 0;
            var insertion_offset: u32 = 0;
            while (text_offset < source.len) {
                var chunk_end = @min(source.len, text_offset + max_insertion_len);
                while (chunk_end > text_offset and chunk_end < source.len and source[chunk_end] & 0xc0 == 0x80) chunk_end -= 1;
                if (chunk_end == text_offset) return error.InsertionTooLarge;
                var id = try Locator.between(allocator, &previous, &maximum);
                defer id.deinit();
                var item = try fragment.Fragment.init(allocator, &id, insertion_timestamp, insertion_offset, @intCast(chunk_end - text_offset), true);
                defer item.deinit();
                try fragments.push(item, null);
                var insertion = try fragment.InsertionFragment.init(allocator, &item);
                defer insertion.deinit();
                try insertions.push(insertion, {});
                try previous.assign(&id);
                insertion_offset += item.len;
                text_offset = chunk_end;
            }
        }
        var undo_map = try UndoMap.init(allocator);
        errdefer undo_map.deinit();
        var history = History.init(allocator, &visible);
        errdefer history.deinit();
        return .{ .allocator = allocator, .snapshot_value = .{
            .allocator = allocator,
            .visible_text = visible,
            .deleted_text = deleted,
            .fragments = fragments,
            .insertions = insertions,
            .undo_map = undo_map,
            .version = version,
            .remote_id = buffer_id,
            .replica_id = replica_id,
            .line_ending = ending,
        }, .lamport_clock = lamport_clock, .subscriptions = PatchTopic.init(allocator), .deferred_operations = try DeferredOperations.init(allocator), .history = history };
    }

    pub fn deinit(self: *Buffer) void {
        self.snapshot_value.deinit();
        self.subscriptions.deinit();
        self.deferred_operations.deinit();
        self.history.deinit();
        for (self.waiters.items) |state| {
            state.cancelFromBuffer();
            state.release();
        }
        self.waiters.deinit(self.allocator);
        self.* = undefined;
    }
    pub fn snapshot(self: *const Buffer) *const BufferSnapshot {
        return &self.snapshot_value;
    }
    pub fn cloneSnapshot(self: *const Buffer) !BufferSnapshot {
        return self.snapshot_value.clone(self.allocator);
    }
    pub fn branch(self: *const Buffer) !Buffer {
        var snapshot_copy = try self.snapshot_value.clone(self.allocator);
        snapshot_copy.replica_id = clock.ReplicaId.LOCAL_BRANCH;
        return .{ .allocator = self.allocator, .snapshot_value = snapshot_copy, .lamport_clock = clock.Lamport.new(clock.ReplicaId.LOCAL_BRANCH), .subscriptions = PatchTopic.init(self.allocator), .deferred_operations = try DeferredOperations.init(self.allocator), .history = History.init(self.allocator, &self.history.base_text) };
    }
    pub fn subscribe(self: *Buffer) !BufferSubscription {
        return self.subscriptions.subscribe();
    }

    pub fn setLineEnding(self: *Buffer, ending: LineEnding) void {
        self.snapshot_value.line_ending = ending;
    }

    /// Applies sorted, non-overlapping edits expressed in the pre-edit visible
    /// coordinate space. The replacement snapshot is fully built and validated
    /// before publication, so allocation or validation failure preserves `self`.
    pub fn edit(self: *Buffer, edits: []const InputEdit) !Operation {
        try validateInputEdits(&self.snapshot_value, edits);
        const automatic = self.history.transaction_depth == 0;
        if (automatic) _ = try self.startTransactionAt(0);
        errdefer if (automatic) self.cancelEmptyTransaction();
        const timestamp = self.lamport_clock;
        var replacement = try buildEditedSnapshot(self.allocator, &self.snapshot_value, edits, timestamp);
        errdefer replacement.deinit();
        try replacement.version.observe(timestamp);
        try replacement.validate();

        var operation = EditOperation{ .allocator = self.allocator, .timestamp = timestamp, .version = try self.snapshot_value.version.clone(self.allocator) };
        errdefer operation.deinit();
        try operation.ranges.ensureTotalCapacity(self.allocator, edits.len);
        try operation.new_text.ensureTotalCapacity(self.allocator, edits.len);
        for (edits) |item| {
            var normalized = try LineEnding.normalize(self.allocator, item.new_text);
            defer normalized.deinit(self.allocator);
            try operation.ranges.append(self.allocator, .{ .start = fullOffsetAt(&self.snapshot_value, item.start), .end = fullOffsetAt(&self.snapshot_value, item.end) });
            const owned = try self.allocator.dupe(u8, normalized.slice());
            errdefer self.allocator.free(owned);
            try operation.new_text.append(self.allocator, owned);
        }

        var patch = Patch.empty(self.allocator);
        defer patch.deinit();
        var old_end: usize = 0;
        var new_end: usize = 0;
        for (edits, operation.new_text.items) |item, replacement_text| {
            const unchanged = item.start - old_end;
            const new_start = new_end + unchanged;
            try patch.push(.{ .old = .{ .start = item.start, .end = item.end }, .new = .{ .start = new_start, .end = new_start + replacement_text.len } });
            old_end = item.end;
            new_end = new_start + replacement_text.len;
        }
        try self.subscriptions.publish(&patch);
        self.snapshot_value.deinit();
        self.snapshot_value = replacement;
        self.resolveWaiters();
        self.lamport_clock.value += 1;
        var result: Operation = .{ .edit = operation };
        try self.history.pushOperation(&result);
        try self.history.undo_stack.items[self.history.undo_stack.items.len - 1].transaction.edit_ids.append(self.allocator, timestamp);
        if (automatic) _ = self.endTransactionAt(0);
        return result;
    }

    /// Borrows operations. Accepted operations and deferred operations are
    /// cloned, so callers retain ownership and may immediately deinitialize them.
    pub fn applyOps(self: *Buffer, ops: []const Operation) !void {
        for (ops) |*operation| try self.history.pushOperation(operation);
        try self.deferred_operations.insert(ops);
        try self.flushDeferredOperations();
    }

    pub fn deferredOperationCount(self: *const Buffer) usize {
        return self.deferred_operations.len();
    }

    fn flushDeferredOperations(self: *Buffer) !void {
        while (true) {
            var pending = try self.deferred_operations.drain();
            defer pending.deinit();
            var blocked: std.ArrayList(Operation) = .empty;
            defer blocked.deinit(self.allocator);
            var made_progress = false;
            var iterator = pending.iterator();
            while (iterator.next()) |operation| {
                if (self.snapshot_value.version.observed(operation.timestamp())) continue;
                const ready = switch (operation.*) {
                    .edit => |*edit_operation| self.snapshot_value.version.observedAll(&edit_operation.version),
                    .undo => |*undo_operation| self.snapshot_value.version.observedAll(&undo_operation.version),
                };
                if (!ready) {
                    try blocked.append(self.allocator, operation.*);
                    continue;
                }
                switch (operation.*) {
                    .edit => |*edit_operation| self.applyRemoteEdit(edit_operation) catch |err| {
                        var restore: std.ArrayList(Operation) = .empty;
                        defer restore.deinit(self.allocator);
                        var restore_iterator = pending.iterator();
                        while (restore_iterator.next()) |queued| try restore.append(self.allocator, queued.*);
                        try self.deferred_operations.insert(restore.items);
                        return err;
                    },
                    .undo => |*undo_operation| {
                        self.applyUndo(undo_operation) catch |err| {
                            try self.deferred_operations.insert(&.{operation.*});
                            return err;
                        };
                        self.lamport_clock.observe(undo_operation.timestamp);
                    },
                }
                made_progress = true;
            }
            try self.deferred_operations.insert(blocked.items);
            if (!made_progress) break;
        }
    }

    fn applyRemoteEdit(self: *Buffer, operation: *const EditOperation) !void {
        if (operation.ranges.items.len != operation.new_text.items.len) return error.MalformedOperation;
        var replacement = try buildRemoteSnapshot(self.allocator, &self.snapshot_value, operation);
        errdefer replacement.deinit();
        try replacement.version.observe(operation.timestamp);
        try replacement.validate();
        var patch = try diffPatch(self.allocator, &self.snapshot_value.visible_text, &replacement.visible_text);
        defer patch.deinit();
        try self.subscriptions.publish(&patch);
        self.snapshot_value.deinit();
        self.snapshot_value = replacement;
        self.resolveWaiters();
        self.lamport_clock.observe(operation.timestamp);
    }

    fn applyUndo(self: *Buffer, operation: *const UndoOperation) !void {
        var new_undo_map = try self.snapshot_value.undo_map.clone(self.allocator);
        errdefer new_undo_map.deinit();
        const view = undo_map_mod.UndoOperation{ .timestamp = operation.timestamp, .counts = operation.counts.items };
        try new_undo_map.insert(&view);

        var previous: std.ArrayList(bool) = .empty;
        defer previous.deinit(self.allocator);
        var new_fragments = try fragment.FragmentTree.init(self.allocator, null);
        errdefer new_fragments.deinit();
        var iterator = self.snapshot_value.fragments.iterator();
        while (iterator.next()) |item| {
            try previous.append(self.allocator, item.visible);
            var copy = try item.clone(self.allocator);
            defer copy.deinit();
            copy.visible = copy.isVisible(&new_undo_map);
            try copy.max_undos.observe(operation.timestamp);
            try new_fragments.push(copy, null);
        }
        var rebuilt = try fragment.rebuildRopes(self.allocator, &new_fragments, &self.snapshot_value.visible_text, &self.snapshot_value.deleted_text, previous.items);
        errdefer {
            rebuilt.visible.deinit();
            rebuilt.deleted.deinit();
        }
        var new_version = try self.snapshot_value.version.clone(self.allocator);
        errdefer new_version.deinit();
        try new_version.observe(operation.timestamp);
        var new_insertions = self.snapshot_value.insertions.clone();
        errdefer new_insertions.deinit();
        var patch = try diffPatch(self.allocator, &self.snapshot_value.visible_text, &rebuilt.visible);
        defer patch.deinit();
        try self.subscriptions.publish(&patch);

        self.snapshot_value.visible_text.deinit();
        self.snapshot_value.deleted_text.deinit();
        self.snapshot_value.fragments.deinit();
        self.snapshot_value.insertions.deinit();
        self.snapshot_value.undo_map.deinit();
        self.snapshot_value.version.deinit();
        self.snapshot_value.visible_text = rebuilt.visible;
        self.snapshot_value.deleted_text = rebuilt.deleted;
        self.snapshot_value.fragments = new_fragments;
        self.snapshot_value.insertions = new_insertions;
        self.snapshot_value.undo_map = new_undo_map;
        self.snapshot_value.version = new_version;
        self.resolveWaiters();
    }

    pub fn waitForVersion(self: *Buffer, target: *const clock.Global) !waiter.WaitHandle {
        const state = try waiter.State.create(self.allocator, target);
        errdefer {
            state.release();
            state.release();
        }
        if (self.snapshot_value.version.observedAll(target)) {
            _ = state.resolve(&self.snapshot_value.version);
            state.release();
        } else {
            try self.waiters.append(self.allocator, state);
        }
        return .{ .state = state };
    }

    pub fn waitForEdits(self: *Buffer, edit_ids: []const clock.Lamport) !waiter.WaitHandle {
        var target = clock.Global.init(self.allocator);
        defer target.deinit();
        for (edit_ids) |edit_id| try target.observe(edit_id);
        return self.waitForVersion(&target);
    }

    pub fn waitForAnchors(self: *Buffer, anchors: []const Anchor) !waiter.WaitHandle {
        var target = clock.Global.init(self.allocator);
        defer target.deinit();
        for (anchors) |anchor| if (!anchor.isMin() and !anchor.isMax()) try target.observe(anchor.timestamp);
        return self.waitForVersion(&target);
    }

    pub fn giveUpWaiting(self: *Buffer) void {
        for (self.waiters.items) |state| {
            state.cancelFromBuffer();
            state.release();
        }
        self.waiters.clearRetainingCapacity();
    }

    fn resolveWaiters(self: *Buffer) void {
        var index: usize = 0;
        while (index < self.waiters.items.len) {
            const state = self.waiters.items[index];
            _ = state.resolve(&self.snapshot_value.version);
            if (state.isFinished()) {
                _ = self.waiters.orderedRemove(index);
                state.release();
            } else index += 1;
        }
    }

    pub fn startTransaction(self: *Buffer) !?TransactionId {
        return self.startTransactionAt(0);
    }
    pub fn startTransactionAt(self: *Buffer, now: u64) !?TransactionId {
        if (self.history.transaction_depth != 0) {
            self.history.transaction_depth += 1;
            return null;
        }
        var start = try self.snapshot_value.version.clone(self.allocator);
        errdefer start.deinit();
        try self.history.undo_stack.ensureUnusedCapacity(self.allocator, 1);
        const id = self.lamport_clock.tick();
        self.history.undo_stack.appendAssumeCapacity(.{ .transaction = .{ .allocator = self.allocator, .id = id, .start = start }, .first_edit_at = now, .last_edit_at = now });
        self.history.transaction_depth = 1;
        return id;
    }
    pub fn endTransaction(self: *Buffer) ?TransactionId {
        return self.endTransactionAt(0);
    }
    pub fn endTransactionAt(self: *Buffer, now: u64) ?TransactionId {
        std.debug.assert(self.history.transaction_depth != 0);
        self.history.transaction_depth -= 1;
        if (self.history.transaction_depth != 0) return null;
        var entry = &self.history.undo_stack.items[self.history.undo_stack.items.len - 1];
        if (entry.transaction.edit_ids.items.len == 0) {
            var empty = self.history.undo_stack.pop().?;
            empty.deinit();
            return null;
        }
        entry.last_edit_at = now;
        self.history.clearRedo();
        return self.groupHistory();
    }
    fn groupHistory(self: *Buffer) ?TransactionId {
        if (self.history.undo_stack.items.len == 0) return null;
        while (self.history.undo_stack.items.len >= 2) {
            const last_index = self.history.undo_stack.items.len - 1;
            var prior = &self.history.undo_stack.items[last_index - 1];
            const current = &self.history.undo_stack.items[last_index];
            if (prior.suppress_grouping or current.first_edit_at -| prior.last_edit_at >= self.history.group_interval) break;
            prior.transaction.edit_ids.appendSlice(self.allocator, current.transaction.edit_ids.items) catch break;
            prior.last_edit_at = current.last_edit_at;
            var removed = self.history.undo_stack.pop().?;
            removed.deinit();
        }
        return self.history.undo_stack.items[self.history.undo_stack.items.len - 1].transaction.id;
    }
    pub fn setGroupInterval(self: *Buffer, interval: u64) void {
        self.history.group_interval = interval;
    }
    pub fn baseText(self: *const Buffer) *const rope.Rope {
        return &self.history.base_text;
    }
    pub fn operations(self: *const Buffer) []const Operation {
        return self.history.operations.items;
    }
    pub fn peekUndoStack(self: *const Buffer) ?*const HistoryEntry {
        return if (self.history.undo_stack.items.len == 0) null else &self.history.undo_stack.items[self.history.undo_stack.items.len - 1];
    }
    pub fn peekRedoStack(self: *const Buffer) ?*const HistoryEntry {
        return if (self.history.redo_stack.items.len == 0) null else &self.history.redo_stack.items[self.history.redo_stack.items.len - 1];
    }
    pub fn finalizeLastTransaction(self: *Buffer) ?*const Transaction {
        if (self.history.undo_stack.items.len == 0) return null;
        self.history.undo_stack.items[self.history.undo_stack.items.len - 1].suppress_grouping = true;
        return &self.history.undo_stack.items[self.history.undo_stack.items.len - 1].transaction;
    }
    pub fn getTransaction(self: *const Buffer, id: TransactionId) ?*const Transaction {
        for (self.history.undo_stack.items) |*entry| if (entry.transaction.id.eql(id)) return &entry.transaction;
        for (self.history.redo_stack.items) |*entry| if (entry.transaction.id.eql(id)) return &entry.transaction;
        return null;
    }
    pub fn forgetTransaction(self: *Buffer, id: TransactionId) bool {
        for (self.history.undo_stack.items, 0..) |*entry, index| if (entry.transaction.id.eql(id)) {
            var removed = self.history.undo_stack.orderedRemove(index);
            removed.deinit();
            return true;
        };
        for (self.history.redo_stack.items, 0..) |*entry, index| if (entry.transaction.id.eql(id)) {
            var removed = self.history.redo_stack.orderedRemove(index);
            removed.deinit();
            return true;
        };
        return false;
    }
    pub fn suppressGrouping(self: *Buffer, id: TransactionId) void {
        for (self.history.undo_stack.items) |*entry| if (entry.transaction.id.eql(id)) {
            entry.suppress_grouping = true;
            return;
        };
    }
    pub fn mergeTransactions(self: *Buffer, source: TransactionId, destination: TransactionId) !void {
        var source_index: ?usize = null;
        for (self.history.undo_stack.items, 0..) |entry, index| if (entry.transaction.id.eql(source)) {
            source_index = index;
            break;
        };
        if (source_index == null) return;
        var removed = self.history.undo_stack.orderedRemove(source_index.?);
        defer removed.deinit();
        for (self.history.undo_stack.items) |*entry| if (entry.transaction.id.eql(destination)) {
            try entry.transaction.mergeIn(&removed.transaction);
            return;
        };
    }
    pub fn undo(self: *Buffer) !?struct { TransactionId, Operation } {
        if (self.history.transaction_depth != 0 or self.history.undo_stack.items.len == 0) return null;
        try self.history.redo_stack.ensureUnusedCapacity(self.allocator, 1);
        var entry = self.history.undo_stack.pop().?;
        const id = entry.transaction.id;
        const operation = self.undoOrRedo(&entry.transaction) catch |err| {
            self.history.undo_stack.appendAssumeCapacity(entry);
            return err;
        };
        self.history.redo_stack.appendAssumeCapacity(entry);
        return .{ id, operation };
    }
    pub fn redo(self: *Buffer) !?struct { TransactionId, Operation } {
        if (self.history.transaction_depth != 0 or self.history.redo_stack.items.len == 0) return null;
        try self.history.undo_stack.ensureUnusedCapacity(self.allocator, 1);
        var entry = self.history.redo_stack.pop().?;
        const id = entry.transaction.id;
        const operation = self.undoOrRedo(&entry.transaction) catch |err| {
            self.history.redo_stack.appendAssumeCapacity(entry);
            return err;
        };
        self.history.undo_stack.appendAssumeCapacity(entry);
        return .{ id, operation };
    }
    fn undoOrRedo(self: *Buffer, transaction: *const Transaction) !Operation {
        var operation = UndoOperation{ .allocator = self.allocator, .timestamp = self.lamport_clock.tick(), .version = try self.snapshot_value.version.clone(self.allocator) };
        errdefer operation.deinit();
        try operation.counts.ensureTotalCapacity(self.allocator, transaction.edit_ids.items.len);
        for (transaction.edit_ids.items) |edit_id| try operation.counts.append(self.allocator, .{ .edit_id = edit_id, .count = self.snapshot_value.undo_map.undoCount(edit_id) +| 1 });
        try self.applyUndo(&operation);
        var result: Operation = .{ .undo = operation };
        try self.history.pushOperation(&result);
        return result;
    }

    fn cancelEmptyTransaction(self: *Buffer) void {
        if (self.history.transaction_depth == 0) return;
        self.history.transaction_depth = 0;
        if (self.history.undo_stack.items.len != 0) {
            var entry = self.history.undo_stack.pop().?;
            entry.deinit();
        }
    }

    pub fn validate(self: *const Buffer) !void {
        try self.snapshot_value.validate();
    }
};

fn diffPatch(allocator: std.mem.Allocator, old: *const rope.Rope, new: *const rope.Rope) !Patch {
    const old_text = try old.toOwnedSlice(allocator);
    defer allocator.free(old_text);
    const new_text = try new.toOwnedSlice(allocator);
    defer allocator.free(new_text);
    var prefix: usize = 0;
    const common_len = @min(old_text.len, new_text.len);
    while (prefix < common_len and old_text[prefix] == new_text[prefix]) prefix += 1;
    while (prefix > 0 and (!old.isCharBoundary(prefix) or !new.isCharBoundary(prefix))) prefix -= 1;
    var old_end = old_text.len;
    var new_end = new_text.len;
    while (old_end > prefix and new_end > prefix and old_text[old_end - 1] == new_text[new_end - 1]) {
        old_end -= 1;
        new_end -= 1;
    }
    while (old_end < old_text.len and !old.isCharBoundary(old_end)) old_end += 1;
    while (new_end < new_text.len and !new.isCharBoundary(new_end)) new_end += 1;
    var result = Patch.empty(allocator);
    errdefer result.deinit();
    try result.push(.{ .old = .{ .start = prefix, .end = old_end }, .new = .{ .start = prefix, .end = new_end } });
    return result;
}

fn fullOffsetAt(snapshot: *const BufferSnapshot, target: usize) fragment.FullOffset {
    var visible: usize = 0;
    var full: usize = 0;
    var iterator = snapshot.fragments.iterator();
    while (iterator.next()) |item| {
        if (item.visible) {
            if (target <= visible + item.len) return .{ .value = full + target - visible };
            visible += item.len;
        }
        full += item.len;
    }
    return .{ .value = full };
}

fn validateInputEdits(snapshot: *const BufferSnapshot, edits: []const InputEdit) !void {
    var previous_end: usize = 0;
    for (edits, 0..) |item, index| {
        if (item.start > item.end or item.end > snapshot.len()) return error.InvalidRange;
        if (index != 0 and item.start < previous_end) return error.OverlappingEdits;
        if (!snapshot.visible_text.isCharBoundary(item.start) or !snapshot.visible_text.isCharBoundary(item.end)) return error.InvalidUtf8Boundary;
        if (!std.unicode.utf8ValidateSlice(item.new_text)) return error.InvalidUtf8;
        previous_end = item.end;
    }
}

const EditBuild = struct {
    allocator: std.mem.Allocator,
    timestamp: clock.Lamport,
    fragments: fragment.FragmentTree,
    insertions: fragment.InsertionTree,
    visible: rope.Rope,
    deleted: rope.Rope,
    previous_id: Locator,
    maximum_id: Locator,
    insertion_offset: u32 = 0,

    fn deinit(self: *EditBuild) void {
        self.fragments.deinit();
        self.insertions.deinit();
        self.visible.deinit();
        self.deleted.deinit();
        self.previous_id.deinit();
        self.maximum_id.deinit();
    }

    fn emitExisting(self: *EditBuild, source: *const fragment.Fragment, relative_start: u32, length: u32, visible: bool, bytes: []const u8) !void {
        if (length == 0) return;
        var id = try Locator.between(self.allocator, &self.previous_id, &self.maximum_id);
        defer id.deinit();
        var item = try source.clone(self.allocator);
        defer item.deinit();
        try item.id.assign(&id);
        item.insertion_offset += relative_start;
        item.len = length;
        if (source.visible and !visible) try item.addDeletion(self.timestamp);
        item.visible = visible;
        try self.fragments.push(item, null);
        var insertion = try fragment.InsertionFragment.init(self.allocator, &item);
        defer insertion.deinit();
        if (try self.insertions.insertOrReplace(fragment.InsertionKeyOps, insertion, {})) |removed_value| {
            var removed = removed_value;
            removed.deinit();
        }
        if (visible) try self.visible.push(bytes) else try self.deleted.push(bytes);
        try self.previous_id.assign(&id);
    }

    fn emitRemoteExisting(self: *EditBuild, source: *const fragment.Fragment, relative_start: u32, length: u32, add_deletion: bool, bytes: []const u8) !void {
        if (length == 0) return;
        var id = try Locator.between(self.allocator, &self.previous_id, &self.maximum_id);
        defer id.deinit();
        var item = try source.clone(self.allocator);
        defer item.deinit();
        try item.id.assign(&id);
        item.insertion_offset += relative_start;
        item.len = length;
        if (add_deletion) try item.addDeletion(self.timestamp);
        item.visible = source.visible and !add_deletion;
        try self.fragments.push(item, null);
        var insertion = try fragment.InsertionFragment.init(self.allocator, &item);
        defer insertion.deinit();
        if (try self.insertions.insertOrReplace(fragment.InsertionKeyOps, insertion, {})) |removed_value| {
            var removed = removed_value;
            removed.deinit();
        }
        if (item.visible) try self.visible.push(bytes) else try self.deleted.push(bytes);
        try self.previous_id.assign(&id);
    }

    fn emitInsertion(self: *EditBuild, input: []const u8) !void {
        var normalized = try LineEnding.normalize(self.allocator, input);
        defer normalized.deinit(self.allocator);
        const bytes = normalized.slice();
        var offset: usize = 0;
        while (offset < bytes.len) {
            var end = @min(bytes.len, offset + max_insertion_len);
            while (end > offset and end < bytes.len and bytes[end] & 0xc0 == 0x80) end -= 1;
            if (end == offset) return error.InsertionTooLarge;
            var id = try Locator.between(self.allocator, &self.previous_id, &self.maximum_id);
            defer id.deinit();
            var item = try fragment.Fragment.init(self.allocator, &id, self.timestamp, self.insertion_offset, @intCast(end - offset), true);
            defer item.deinit();
            try self.fragments.push(item, null);
            var insertion = try fragment.InsertionFragment.init(self.allocator, &item);
            defer insertion.deinit();
            if (try self.insertions.insertOrReplace(fragment.InsertionKeyOps, insertion, {})) |removed_value| {
                var removed = removed_value;
                removed.deinit();
            }
            try self.visible.push(bytes[offset..end]);
            try self.previous_id.assign(&id);
            self.insertion_offset += item.len;
            offset = end;
        }
    }
};

fn canonicalizeBuiltRope(allocator: std.mem.Allocator, value: *rope.Rope) !void {
    const bytes = try value.toOwnedSlice(allocator);
    defer allocator.free(bytes);
    var replacement = try rope.Rope.initText(allocator, bytes);
    value.deinit();
    value.* = replacement;
    replacement = undefined;
}

fn finishBuild(allocator: std.mem.Allocator, old: *const BufferSnapshot, build: *EditBuild) !BufferSnapshot {
    try canonicalizeBuiltRope(allocator, &build.visible);
    try canonicalizeBuiltRope(allocator, &build.deleted);
    var undo_map = try old.undo_map.clone(allocator);
    errdefer undo_map.deinit();
    var version = try old.version.clone(allocator);
    errdefer version.deinit();
    const result = BufferSnapshot{ .allocator = allocator, .visible_text = build.visible, .deleted_text = build.deleted, .fragments = build.fragments, .insertions = build.insertions, .undo_map = undo_map, .version = version, .remote_id = old.remote_id, .replica_id = old.replica_id, .line_ending = old.line_ending };
    build.previous_id.deinit();
    build.maximum_id.deinit();
    return result;
}

fn buildRemoteSnapshot(allocator: std.mem.Allocator, old: *const BufferSnapshot, operation: *const EditOperation) !BufferSnapshot {
    var previous_end: usize = 0;
    for (operation.ranges.items, 0..) |range, index| {
        if (range.start.value > range.end.value or (index != 0 and range.start.value < previous_end)) return error.MalformedOperation;
        if (!std.unicode.utf8ValidateSlice(operation.new_text.items[index])) return error.InvalidUtf8;
        previous_end = range.end.value;
    }
    var build = EditBuild{
        .allocator = allocator,
        .timestamp = operation.timestamp,
        .fragments = try fragment.FragmentTree.init(allocator, null),
        .insertions = try fragment.InsertionTree.init(allocator, {}),
        .visible = try rope.Rope.init(allocator),
        .deleted = try rope.Rope.init(allocator),
        .previous_id = try Locator.min(allocator),
        .maximum_id = try Locator.max(allocator),
    };
    errdefer build.deinit();
    const visible_bytes = try old.visible_text.toOwnedSlice(allocator);
    defer allocator.free(visible_bytes);
    const deleted_bytes = try old.deleted_text.toOwnedSlice(allocator);
    defer allocator.free(deleted_bytes);

    var version_offset: usize = 0;
    var visible_offset: usize = 0;
    var deleted_offset: usize = 0;
    var edit_index: usize = 0;
    var inserted = false;
    var iterator = old.fragments.iterator();
    while (iterator.next()) |item| {
        const source_start = if (item.visible) visible_offset else deleted_offset;
        const source = if (item.visible) visible_bytes[source_start .. source_start + item.len] else deleted_bytes[source_start .. source_start + item.len];
        if (item.visible) visible_offset += item.len else deleted_offset += item.len;
        const observed = operation.version.observed(item.timestamp);
        if (!observed) {
            // Rust's apply_remote_edit keeps higher Lamport concurrent insertions
            // before the incoming insertion at the same versioned full offset.
            if (edit_index < operation.ranges.items.len and !inserted and version_offset == operation.ranges.items[edit_index].start.value and item.timestamp.order(operation.timestamp) != .gt) {
                try build.emitInsertion(operation.new_text.items[edit_index]);
                inserted = true;
                if (operation.ranges.items[edit_index].start.value == operation.ranges.items[edit_index].end.value) {
                    edit_index += 1;
                    inserted = false;
                }
            }
            try build.emitRemoteExisting(item, 0, item.len, false, source);
            continue;
        }
        var relative: usize = 0;
        while (relative < item.len) {
            if (edit_index < operation.ranges.items.len and !inserted and version_offset == operation.ranges.items[edit_index].start.value) {
                try build.emitInsertion(operation.new_text.items[edit_index]);
                inserted = true;
                if (operation.ranges.items[edit_index].start.value == operation.ranges.items[edit_index].end.value) {
                    edit_index += 1;
                    inserted = false;
                }
            }
            if (edit_index >= operation.ranges.items.len or version_offset < operation.ranges.items[edit_index].start.value) {
                const stop = if (edit_index < operation.ranges.items.len) @min(@as(usize, item.len), relative + operation.ranges.items[edit_index].start.value - version_offset) else item.len;
                try build.emitRemoteExisting(item, @intCast(relative), @intCast(stop - relative), false, source[relative..stop]);
                version_offset += stop - relative;
                relative = stop;
            } else {
                const range = operation.ranges.items[edit_index];
                const stop = @min(@as(usize, item.len), relative + range.end.value - version_offset);
                const delete = item.wasVisible(&operation.version, &old.undo_map);
                try build.emitRemoteExisting(item, @intCast(relative), @intCast(stop - relative), delete, source[relative..stop]);
                version_offset += stop - relative;
                relative = stop;
                if (version_offset == range.end.value) {
                    edit_index += 1;
                    inserted = false;
                }
            }
        }
    }
    while (edit_index < operation.ranges.items.len) : (edit_index += 1) {
        const range = operation.ranges.items[edit_index];
        if (range.start.value != version_offset or range.end.value != version_offset) return error.InvalidRange;
        try build.emitInsertion(operation.new_text.items[edit_index]);
    }
    return finishBuild(allocator, old, &build);
}

fn buildEditedSnapshot(allocator: std.mem.Allocator, old: *const BufferSnapshot, edits: []const InputEdit, timestamp: clock.Lamport) !BufferSnapshot {
    var build = EditBuild{
        .allocator = allocator,
        .timestamp = timestamp,
        .fragments = try fragment.FragmentTree.init(allocator, null),
        .insertions = try fragment.InsertionTree.init(allocator, {}),
        .visible = try rope.Rope.init(allocator),
        .deleted = try rope.Rope.init(allocator),
        .previous_id = try Locator.min(allocator),
        .maximum_id = try Locator.max(allocator),
    };
    errdefer build.deinit();
    const visible_bytes = try old.visible_text.toOwnedSlice(allocator);
    defer allocator.free(visible_bytes);
    const deleted_bytes = try old.deleted_text.toOwnedSlice(allocator);
    defer allocator.free(deleted_bytes);

    var visible_offset: usize = 0;
    var deleted_offset: usize = 0;
    var edit_index: usize = 0;
    var inserted = false;
    var iterator = old.fragments.iterator();
    while (iterator.next()) |item| {
        if (!item.visible) {
            const end = deleted_offset + item.len;
            try build.emitExisting(item, 0, item.len, false, deleted_bytes[deleted_offset..end]);
            deleted_offset = end;
            continue;
        }
        const fragment_start = visible_offset;
        const fragment_end = fragment_start + item.len;
        var cursor = fragment_start;
        while (cursor < fragment_end) {
            while (edit_index < edits.len and edits[edit_index].end == cursor and edits[edit_index].start == cursor) {
                try build.emitInsertion(edits[edit_index].new_text);
                edit_index += 1;
            }
            if (edit_index >= edits.len or cursor < edits[edit_index].start) {
                const end = if (edit_index < edits.len) @min(fragment_end, edits[edit_index].start) else fragment_end;
                try build.emitExisting(item, @intCast(cursor - fragment_start), @intCast(end - cursor), true, visible_bytes[cursor..end]);
                cursor = end;
                continue;
            }
            if (!inserted) {
                try build.emitInsertion(edits[edit_index].new_text);
                inserted = true;
            }
            const end = @min(fragment_end, edits[edit_index].end);
            try build.emitExisting(item, @intCast(cursor - fragment_start), @intCast(end - cursor), false, visible_bytes[cursor..end]);
            cursor = end;
            if (cursor == edits[edit_index].end) {
                edit_index += 1;
                inserted = false;
            }
        }
        visible_offset = fragment_end;
    }
    while (edit_index < edits.len) : (edit_index += 1) {
        if (edits[edit_index].start != visible_offset or edits[edit_index].end != visible_offset) return error.InvalidRange;
        try build.emitInsertion(edits[edit_index].new_text);
    }
    if (visible_offset != visible_bytes.len or deleted_offset != deleted_bytes.len) return error.SourceLengthMismatch;
    return finishBuild(allocator, old, &build);
}
