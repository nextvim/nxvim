const std = @import("std");
const clock = @import("clock");
const rope = @import("rope");
const sum_tree = @import("sum_tree");
const Locator = @import("locator.zig").Locator;
const UndoMap = @import("undo_map.zig").UndoMap;

pub const Context = ?*const clock.Global;

pub const FragmentTextSummary = struct {
    visible: usize = 0,
    deleted: usize = 0,

    pub fn total(self: FragmentTextSummary) usize {
        return self.visible + self.deleted;
    }
};

pub const Fragment = struct {
    allocator: std.mem.Allocator,
    id: Locator,
    timestamp: clock.Lamport,
    insertion_offset: u32,
    len: u32,
    visible: bool,
    deletions: std.ArrayList(clock.Lamport) = .empty,
    max_undos: clock.Global,

    pub fn init(allocator: std.mem.Allocator, id: *const Locator, timestamp: clock.Lamport, insertion_offset: u32, len: u32, visible: bool) !Fragment {
        if (len == 0) return error.EmptyFragment;
        return .{
            .allocator = allocator,
            .id = try id.clone(allocator),
            .timestamp = timestamp,
            .insertion_offset = insertion_offset,
            .len = len,
            .visible = visible,
            .max_undos = clock.Global.init(allocator),
        };
    }

    pub fn deinit(self: *Fragment) void {
        self.id.deinit();
        self.deletions.deinit(self.allocator);
        self.max_undos.deinit();
        self.* = undefined;
    }

    pub fn clone(self: *const Fragment, allocator: std.mem.Allocator) !Fragment {
        var result = try Fragment.init(allocator, &self.id, self.timestamp, self.insertion_offset, self.len, self.visible);
        errdefer result.deinit();
        try result.deletions.appendSlice(allocator, self.deletions.items);
        result.max_undos.deinit();
        result.max_undos = try self.max_undos.clone(allocator);
        return result;
    }

    pub fn addDeletion(self: *Fragment, timestamp: clock.Lamport) !void {
        try self.deletions.append(self.allocator, timestamp);
    }

    pub fn isVisible(self: *const Fragment, undos: *const UndoMap) bool {
        if (undos.isUndone(self.timestamp)) return false;
        for (self.deletions.items) |deletion| if (!undos.isUndone(deletion)) return false;
        return true;
    }

    pub fn wasVisible(self: *const Fragment, version: *const clock.Global, undos: *const UndoMap) bool {
        if (!version.observed(self.timestamp) or undos.wasUndone(self.timestamp, version)) return false;
        for (self.deletions.items) |deletion| {
            if (version.observed(deletion) and !undos.wasUndone(deletion, version)) return false;
        }
        return true;
    }

    /// Splits this fragment at a fragment-relative byte offset. On failure this
    /// fragment is unchanged; the returned right fragment owns `right_id`'s copy.
    pub fn split(self: *Fragment, at: u32, right_id: *const Locator) !Fragment {
        if (at == 0 or at >= self.len) return error.InvalidSplit;
        var right = try self.clone(self.allocator);
        errdefer right.deinit();
        try right.id.assign(right_id);
        right.insertion_offset += at;
        right.len -= at;
        self.len = at;
        return right;
    }
};

pub const FragmentSummary = struct {
    allocator: std.mem.Allocator,
    text: FragmentTextSummary,
    max_id: Locator,
    max_version: clock.Global,
    min_insertion_version: clock.Global,
    max_insertion_version: clock.Global,

    pub fn init(allocator: std.mem.Allocator) !FragmentSummary {
        return .{
            .allocator = allocator,
            .text = .{},
            .max_id = try Locator.min(allocator),
            .max_version = clock.Global.init(allocator),
            .min_insertion_version = clock.Global.init(allocator),
            .max_insertion_version = clock.Global.init(allocator),
        };
    }

    pub fn deinit(self: *FragmentSummary) void {
        self.max_id.deinit();
        self.max_version.deinit();
        self.min_insertion_version.deinit();
        self.max_insertion_version.deinit();
        self.* = undefined;
    }

    pub fn clone(self: *const FragmentSummary, allocator: std.mem.Allocator) !FragmentSummary {
        var result = try FragmentSummary.init(allocator);
        errdefer result.deinit();
        try result.max_id.assign(&self.max_id);
        result.text = self.text;
        result.max_version.deinit();
        result.max_version = try self.max_version.clone(allocator);
        result.min_insertion_version.deinit();
        result.min_insertion_version = try self.min_insertion_version.clone(allocator);
        result.max_insertion_version.deinit();
        result.max_insertion_version = try self.max_insertion_version.clone(allocator);
        return result;
    }
};

pub const FragmentTreeOps = struct {
    pub const Summary = FragmentSummary;
    pub const Context = @import("fragment.zig").Context;

    pub fn summary(fragment: *const Fragment, _: @This().Context) Summary {
        var result = FragmentSummary.init(fragment.allocator) catch @panic("fragment summary allocation failed");
        result.max_id.assign(&fragment.id) catch @panic("fragment summary allocation failed");
        result.text = if (fragment.visible) .{ .visible = fragment.len } else .{ .deleted = fragment.len };
        result.max_version.observe(fragment.timestamp) catch @panic("fragment summary allocation failed");
        for (fragment.deletions.items) |deletion| result.max_version.observe(deletion) catch @panic("fragment summary allocation failed");
        result.max_version.join(&fragment.max_undos) catch @panic("fragment summary allocation failed");
        result.min_insertion_version.observe(fragment.timestamp) catch @panic("fragment summary allocation failed");
        result.max_insertion_version.observe(fragment.timestamp) catch @panic("fragment summary allocation failed");
        return result;
    }

    pub fn zero(_: @This().Context) Summary {
        return FragmentSummary.init(std.heap.page_allocator) catch @panic("fragment summary allocation failed");
    }

    pub fn addSummary(total: *Summary, value: *const Summary, _: @This().Context) void {
        total.max_id.assign(&value.max_id) catch @panic("fragment summary allocation failed");
        total.text.visible += value.text.visible;
        total.text.deleted += value.text.deleted;
        total.max_version.join(&value.max_version) catch @panic("fragment summary allocation failed");
        total.min_insertion_version.meet(&value.min_insertion_version) catch @panic("fragment summary allocation failed");
        total.max_insertion_version.join(&value.max_insertion_version) catch @panic("fragment summary allocation failed");
    }

    pub fn cloneItem(value: *const Fragment, allocator: std.mem.Allocator) !Fragment {
        return value.clone(allocator);
    }
    pub fn deinitItem(value: *Fragment, _: std.mem.Allocator) void {
        value.deinit();
    }
    pub fn cloneSummary(value: *const Summary, allocator: std.mem.Allocator) !Summary {
        return value.clone(allocator);
    }
    pub fn deinitSummary(value: *Summary, _: std.mem.Allocator) void {
        value.deinit();
    }
    pub fn eqlSummary(a: *const Summary, b: *const Summary) bool {
        return a.text.visible == b.text.visible and a.text.deleted == b.text.deleted and a.max_id.eql(&b.max_id) and
            a.max_version.eql(&b.max_version) and a.min_insertion_version.eql(&b.min_insertion_version) and
            a.max_insertion_version.eql(&b.max_insertion_version);
    }
};

pub const FragmentTree = sum_tree.SumTree(Fragment, FragmentTreeOps, sum_tree.DefaultTreeBase);

pub const FullOffset = struct { value: usize = 0 };

pub const FullOffsetDimension = struct {
    pub const Value = FullOffset;
    pub fn zero(_: Context) Value {
        return .{};
    }
    pub fn addSummary(value: *Value, summary: *const FragmentSummary, _: Context) void {
        value.value += summary.text.total();
    }
};

pub const VisibleOffsetDimension = struct {
    pub const Value = usize;
    pub fn zero(_: Context) Value {
        return 0;
    }
    pub fn addSummary(value: *Value, summary: *const FragmentSummary, _: Context) void {
        value.* += summary.text.visible;
    }
};

pub const FragmentTextDimension = struct {
    pub const Value = FragmentTextSummary;
    pub fn zero(_: Context) Value {
        return .{};
    }
    pub fn addSummary(value: *Value, summary: *const FragmentSummary, _: Context) void {
        value.visible += summary.text.visible;
        value.deleted += summary.text.deleted;
    }
};

pub const VersionedFullOffset = union(enum) {
    offset: FullOffset,
    invalid,

    pub fn fullOffset(self: VersionedFullOffset) ?FullOffset {
        return switch (self) {
            .offset => |value| value,
            .invalid => null,
        };
    }
};

pub const VersionedFullOffsetDimension = struct {
    pub const Value = VersionedFullOffset;
    pub fn zero(_: Context) Value {
        return .{ .offset = .{} };
    }
    pub fn addSummary(value: *Value, summary: *const FragmentSummary, context: Context) void {
        const version = context orelse @panic("versioned full offset requires a version");
        switch (value.*) {
            .invalid => {},
            .offset => |*offset| {
                if (version.observedAll(&summary.max_insertion_version)) offset.value += summary.text.total() else if (version.observedAny(&summary.min_insertion_version)) value.* = .invalid;
            },
        }
    }
};

pub const InsertionFragmentKey = struct {
    timestamp: clock.Lamport,
    split_offset: u32,

    pub fn order(self: InsertionFragmentKey, other: InsertionFragmentKey) std.math.Order {
        const timestamp_order = self.timestamp.order(other.timestamp);
        return if (timestamp_order == .eq) std.math.order(self.split_offset, other.split_offset) else timestamp_order;
    }
};

pub const InsertionFragment = struct {
    timestamp: clock.Lamport,
    split_offset: u32,
    fragment_id: Locator,

    pub fn init(allocator: std.mem.Allocator, fragment: *const Fragment) !InsertionFragment {
        return .{ .timestamp = fragment.timestamp, .split_offset = fragment.insertion_offset, .fragment_id = try fragment.id.clone(allocator) };
    }
    pub fn deinit(self: *InsertionFragment) void {
        self.fragment_id.deinit();
        self.* = undefined;
    }
};

pub const InsertionSlice = struct {
    edit_id: clock.Lamport,
    insertion_id: clock.Lamport,
    range_start: u32,
    range_end: u32,

    pub fn fromFragment(edit_id: clock.Lamport, fragment: *const Fragment) InsertionSlice {
        return .{ .edit_id = edit_id, .insertion_id = fragment.timestamp, .range_start = fragment.insertion_offset, .range_end = fragment.insertion_offset + fragment.len };
    }
    pub fn order(self: InsertionSlice, other: InsertionSlice) std.math.Order {
        var result = self.edit_id.order(other.edit_id);
        if (result == .eq) result = self.insertion_id.order(other.insertion_id);
        if (result == .eq) result = std.math.order(self.range_start, other.range_start);
        if (result == .eq) result = std.math.order(self.range_end, other.range_end);
        return result;
    }
};

pub const InsertionTreeOps = struct {
    pub const Summary = InsertionFragmentKey;
    pub const Context = void;
    pub fn summary(value: *const InsertionFragment, _: void) Summary {
        return .{ .timestamp = value.timestamp, .split_offset = value.split_offset };
    }
    pub fn zero(_: void) Summary {
        return .{ .timestamp = clock.Lamport.MIN, .split_offset = 0 };
    }
    pub fn addSummary(total: *Summary, value: *const Summary, _: void) void {
        total.* = value.*;
    }
    pub fn cloneItem(value: *const InsertionFragment, allocator: std.mem.Allocator) !InsertionFragment {
        return .{ .timestamp = value.timestamp, .split_offset = value.split_offset, .fragment_id = try value.fragment_id.clone(allocator) };
    }
    pub fn deinitItem(value: *InsertionFragment, _: std.mem.Allocator) void {
        value.deinit();
    }
    pub fn cloneSummary(value: *const Summary, _: std.mem.Allocator) !Summary {
        return value.*;
    }
    pub fn deinitSummary(_: *Summary, _: std.mem.Allocator) void {}
    pub fn eqlSummary(a: *const Summary, b: *const Summary) bool {
        return a.timestamp.eql(b.timestamp) and a.split_offset == b.split_offset;
    }
};

pub const InsertionKeyOps = struct {
    pub const Key = InsertionFragmentKey;
    pub fn key(value: *const InsertionFragment) Key {
        return .{ .timestamp = value.timestamp, .split_offset = value.split_offset };
    }
    pub fn compareKeys(a: *const Key, b: *const Key) std.math.Order {
        return a.order(b.*);
    }
    pub fn compareItemKey(value: *const InsertionFragment, key_value: *const Key) std.math.Order {
        return key(value).order(key_value.*);
    }
};

pub const InsertionTree = sum_tree.SumTree(InsertionFragment, InsertionTreeOps, sum_tree.DefaultTreeBase);

pub const FragmentBuilder = struct {
    allocator: std.mem.Allocator,
    tree: FragmentTree,

    pub fn init(allocator: std.mem.Allocator, context: Context) !FragmentBuilder {
        return .{ .allocator = allocator, .tree = try FragmentTree.init(allocator, context) };
    }
    pub fn deinit(self: *FragmentBuilder) void {
        self.tree.deinit();
        self.* = undefined;
    }
    pub fn push(self: *FragmentBuilder, fragment: Fragment, context: Context) !void {
        try self.tree.push(fragment, context);
    }
    pub fn append(self: *FragmentBuilder, fragments: *const FragmentTree, context: Context) !void {
        try self.tree.append(fragments, context);
    }
    pub fn summary(self: *const FragmentBuilder) *const FragmentSummary {
        return self.tree.summary();
    }
    pub fn finish(self: *FragmentBuilder) FragmentTree {
        const result = self.tree;
        self.* = undefined;
        return result;
    }
};

pub const RebuiltRopes = struct { visible: rope.Rope, deleted: rope.Rope };

/// Rebuilds visible/deleted ropes by consuming bytes from the old rope selected
/// by each fragment's previous visibility and routing them by current visibility.
pub fn rebuildRopes(allocator: std.mem.Allocator, fragments: *const FragmentTree, old_visible: *const rope.Rope, old_deleted: *const rope.Rope, previous_visibility: []const bool) !RebuiltRopes {
    if (previous_visibility.len != fragments.itemCount()) return error.VisibilityCountMismatch;
    var result: RebuiltRopes = .{ .visible = try rope.Rope.init(allocator), .deleted = try rope.Rope.init(allocator) };
    errdefer {
        result.visible.deinit();
        result.deleted.deinit();
    }
    var visible_offset: usize = 0;
    var deleted_offset: usize = 0;
    var iterator = fragments.iterator();
    var index: usize = 0;
    while (iterator.next()) |fragment| : (index += 1) {
        const start = if (previous_visibility[index]) visible_offset else deleted_offset;
        const end = start + fragment.len;
        var part = try (if (previous_visibility[index]) old_visible else old_deleted).sliceBytes(.{ .start = start, .end = end });
        defer part.deinit();
        if (fragment.visible) try result.visible.append(&part) else try result.deleted.append(&part);
        if (previous_visibility[index]) visible_offset = end else deleted_offset = end;
    }
    if (visible_offset != old_visible.len() or deleted_offset != old_deleted.len()) return error.SourceLengthMismatch;
    return result;
}
