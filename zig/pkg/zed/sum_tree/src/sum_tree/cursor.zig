const std = @import("std");
const Bias = @import("sum_tree.zig").Bias;

/// A dimension contract declares `Value`, `zero(context)`, and
/// `addSummary(*Value, *const Summary, context)`.
/// A target contract declares `compare(target, *const Value, context)`.
pub fn Cursor(comptime Tree: type, comptime Dimension: type) type {
    comptime {
        requireDecl(Dimension, "Value");
        requireDecl(Dimension, "zero");
        requireDecl(Dimension, "addSummary");
    }

    const Item = Tree.ItemType;
    const Summary = Tree.SummaryType;
    const Context = Tree.ContextType;
    const Value = Dimension.Value;

    return struct {
        const Self = @This();

        tree: *const Tree,
        context: Context,
        position: Value,
        index: usize,
        did_seek: bool,
        before_start: bool,

        pub fn init(tree: *const Tree, context: Context) Self {
            return .{
                .tree = tree,
                .context = context,
                .position = Dimension.zero(context),
                .index = 0,
                .did_seek = false,
                .before_start = false,
            };
        }

        pub fn reset(self: *Self) void {
            self.position = Dimension.zero(self.context);
            self.index = 0;
            self.did_seek = false;
            self.before_start = false;
        }

        pub fn didSeek(self: *const Self) bool {
            return self.did_seek;
        }

        pub fn start(self: *const Self) *const Value {
            return &self.position;
        }

        pub fn end(self: *const Self) Value {
            self.assertDidSeek();
            var result = self.position;
            if (self.itemSummary()) |item_summary| Dimension.addSummary(&result, item_summary, self.context);
            return result;
        }

        pub fn item(self: *const Self) ?*const Item {
            self.assertDidSeek();
            if (self.before_start or self.index >= self.tree.itemCount()) return null;
            return self.tree.itemAt(self.index);
        }

        pub fn itemSummary(self: *const Self) ?*const Summary {
            self.assertDidSeek();
            if (self.before_start or self.index >= self.tree.itemCount()) return null;
            return self.tree.itemSummaryAt(self.index);
        }

        pub fn nextItem(self: *const Self) ?*const Item {
            self.assertDidSeek();
            if (self.before_start) return self.tree.first();
            return self.tree.itemAt(self.index + 1);
        }

        pub fn prevItem(self: *const Self) ?*const Item {
            self.assertDidSeek();
            if (self.before_start or self.index == 0) return null;
            return self.tree.itemAt(self.index - 1);
        }

        pub fn next(self: *Self) void {
            self.searchForward(struct {
                fn accept(_: *const Summary) bool {
                    return true;
                }
            }.accept);
        }

        pub fn prev(self: *Self) void {
            self.searchBackward(struct {
                fn accept(_: *const Summary) bool {
                    return true;
                }
            }.accept);
        }

        pub fn searchForward(self: *Self, filter: anytype) void {
            if (!self.did_seek) {
                self.did_seek = true;
                self.index = 0;
                self.before_start = false;
            } else if (self.before_start) {
                self.before_start = false;
                self.index = 0;
            } else if (self.index < self.tree.itemCount()) {
                if (self.tree.itemSummaryAt(self.index)) |summary| Dimension.addSummary(&self.position, summary, self.context);
                self.index += 1;
            }

            while (self.index < self.tree.itemCount()) {
                const summary = self.tree.itemSummaryAt(self.index).?;
                if (invokeFilter(filter, summary)) return;
                Dimension.addSummary(&self.position, summary, self.context);
                self.index += 1;
            }
        }

        pub fn searchBackward(self: *Self, filter: anytype) void {
            if (!self.did_seek) {
                self.did_seek = true;
                self.index = self.tree.itemCount();
                self.position = self.tree.extent(Dimension, self.context);
                self.before_start = false;
            }

            while (self.index > 0) {
                self.index -= 1;
                self.position = self.positionAt(self.index);
                const summary = self.tree.itemSummaryAt(self.index).?;
                if (invokeFilter(filter, summary)) {
                    self.before_start = false;
                    return;
                }
            }
            self.before_start = true;
            self.position = Dimension.zero(self.context);
        }

        pub fn seek(self: *Self, comptime Target: type, target: Target, bias: Bias) bool {
            self.reset();
            return self.seekInternal(Target, target, bias);
        }

        pub fn seekForward(self: *Self, comptime Target: type, target: Target, bias: Bias) bool {
            std.debug.assert(self.did_seek);
            std.debug.assert(Target.compare(target, &self.position, self.context) != .lt);
            return self.seekInternal(Target, target, bias);
        }

        pub fn slice(self: *Self, comptime Target: type, target: Target, bias: Bias) !Tree {
            if (!self.did_seek) {
                self.did_seek = true;
                self.before_start = false;
                self.index = 0;
                self.position = Dimension.zero(self.context);
            }
            const begin = self.index;
            _ = self.seekInternal(Target, target, bias);
            return self.tree.copyRange(begin, self.index, self.context);
        }

        pub fn suffix(self: *Self) !Tree {
            if (!self.did_seek) {
                self.did_seek = true;
                self.index = 0;
                self.position = Dimension.zero(self.context);
            }
            const begin = self.index;
            self.index = self.tree.itemCount();
            self.position = self.tree.extent(Dimension, self.context);
            self.before_start = false;
            return self.tree.copyRange(begin, self.index, self.context);
        }

        pub fn rangeSummary(self: *Self, comptime Target: type, target: Target, bias: Bias, comptime Output: type) Output.Value {
            comptime {
                requireDecl(Output, "Value");
                requireDecl(Output, "zero");
                requireDecl(Output, "addSummary");
            }
            if (!self.did_seek) {
                self.did_seek = true;
                self.index = 0;
                self.position = Dimension.zero(self.context);
            }
            const begin = self.index;
            _ = self.seekInternal(Target, target, bias);
            var result = Output.zero(self.context);
            var i = begin;
            while (i < self.index) : (i += 1) Output.addSummary(&result, self.tree.itemSummaryAt(i).?, self.context);
            return result;
        }

        fn seekInternal(self: *Self, comptime Target: type, target: Target, bias: Bias) bool {
            self.did_seek = true;
            self.before_start = false;
            var matched_boundary = false;
            while (self.index < self.tree.itemCount()) {
                const summary = self.tree.itemSummaryAt(self.index).?;
                var item_end = self.position;
                Dimension.addSummary(&item_end, summary, self.context);
                const comparison = Target.compare(target, &item_end, self.context);
                if (comparison == .eq) matched_boundary = true;
                if (comparison == .gt or (comparison == .eq and bias == .right)) {
                    self.position = item_end;
                    self.index += 1;
                } else break;
            }
            return matched_boundary or Target.compare(target, &self.end(), self.context) == .eq;
        }

        fn positionAt(self: *const Self, wanted: usize) Value {
            var position = Dimension.zero(self.context);
            var i: usize = 0;
            while (i < wanted) : (i += 1) Dimension.addSummary(&position, self.tree.itemSummaryAt(i).?, self.context);
            return position;
        }

        fn assertDidSeek(self: *const Self) void {
            std.debug.assert(self.did_seek);
        }
    };
}

pub fn FilterCursor(comptime Tree: type, comptime Dimension: type, comptime Filter: type) type {
    const Base = Cursor(Tree, Dimension);
    return struct {
        const Self = @This();
        cursor: Base,
        filter: Filter,

        pub fn init(tree: *const Tree, context: Tree.ContextType, filter: Filter) Self {
            return .{ .cursor = Base.init(tree, context), .filter = filter };
        }

        pub fn start(self: *const Self) *const Dimension.Value {
            return self.cursor.start();
        }

        pub fn end(self: *const Self) Dimension.Value {
            return self.cursor.end();
        }

        pub fn item(self: *const Self) ?*const Tree.ItemType {
            return self.cursor.item();
        }

        pub fn itemSummary(self: *const Self) ?*const Tree.SummaryType {
            return self.cursor.itemSummary();
        }

        pub fn next(self: *Self) void {
            self.cursor.searchForward(self.filter);
        }

        pub fn prev(self: *Self) void {
            self.cursor.searchBackward(self.filter);
        }
    };
}

fn invokeFilter(filter: anytype, summary: anytype) bool {
    return @call(.auto, filter, .{summary});
}

fn requireDecl(comptime T: type, comptime name: []const u8) void {
    if (!@hasDecl(T, name)) @compileError(@typeName(T) ++ " must declare " ++ name);
}
