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
    const Node = Tree.CursorNode;
    const max_height = Tree.CursorMaxHeight;

    return struct {
        const Self = @This();
        const Frame = struct {
            node: Node,
            index: usize,
            start: Value,
        };

        tree: *const Tree,
        context: Context,
        position: Value,
        index: usize,
        did_seek: bool,
        before_start: bool,
        stack: [max_height]Frame,
        stack_len: usize,

        pub fn init(tree: *const Tree, context: Context) Self {
            return .{
                .tree = tree,
                .context = context,
                .position = Dimension.zero(context),
                .index = 0,
                .did_seek = false,
                .before_start = false,
                .stack = undefined,
                .stack_len = 0,
            };
        }

        pub fn reset(self: *Self) void {
            self.position = Dimension.zero(self.context);
            self.index = 0;
            self.did_seek = false;
            self.before_start = false;
            self.stack_len = 0;
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
            if (self.before_start or self.stack_len == 0) return null;
            const frame = self.stack[self.stack_len - 1];
            if (!frame.node.isLeaf() or frame.index >= frame.node.len()) return null;
            return frame.node.item(frame.index);
        }

        pub fn itemSummary(self: *const Self) ?*const Summary {
            self.assertDidSeek();
            if (self.before_start or self.stack_len == 0) return null;
            const frame = self.stack[self.stack_len - 1];
            if (!frame.node.isLeaf() or frame.index >= frame.node.len()) return null;
            return frame.node.itemSummary(frame.index);
        }

        pub fn nextItem(self: *const Self) ?*const Item {
            self.assertDidSeek();
            if (self.before_start) return self.tree.first();
            var copy = self.*;
            if (!copy.advanceRaw()) return null;
            return copy.item();
        }

        pub fn prevItem(self: *const Self) ?*const Item {
            self.assertDidSeek();
            if (self.before_start or self.index == 0) return null;
            var copy = self.*;
            if (!copy.retreatRaw()) return null;
            return copy.item();
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
                self.before_start = false;
                self.position = Dimension.zero(self.context);
                self.index = 0;
                self.descendFirst(self.tree.cursorRoot());
            } else if (self.before_start) {
                self.before_start = false;
                self.position = Dimension.zero(self.context);
                self.index = 0;
                self.stack_len = 0;
                self.descendFirst(self.tree.cursorRoot());
            } else if (self.itemSummary()) |summary| {
                Dimension.addSummary(&self.position, summary, self.context);
                self.index += 1;
                _ = self.advancePath();
            }

            while (self.stack_len > 0) {
                const frame = self.stack[self.stack_len - 1];
                if (frame.node.isLeaf()) {
                    if (invokeFilter(filter, frame.node.itemSummary(frame.index))) return;
                    Dimension.addSummary(&self.position, frame.node.itemSummary(frame.index), self.context);
                    self.index += 1;
                    _ = self.advancePath();
                    continue;
                }

                const child_summary = frame.node.childSummary(frame.index);
                if (!invokeFilter(filter, child_summary)) {
                    Dimension.addSummary(&self.position, child_summary, self.context);
                    self.index += frame.node.child(frame.index).itemCount();
                    _ = self.advancePath();
                } else {
                    self.descendFirst(frame.node.child(frame.index));
                }
            }
        }

        pub fn searchBackward(self: *Self, filter: anytype) void {
            if (!self.did_seek) {
                self.did_seek = true;
                self.before_start = false;
                self.position = self.tree.extent(Dimension, self.context);
                self.index = self.tree.itemCount();
                self.stack_len = 0;
            }

            while (self.index > 0) {
                if (!self.retreatRaw()) break;
                if (invokeFilter(filter, self.itemSummary().?)) return;
            }
            self.setBeforeStart();
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
            self.ensureStart();
            const begin = self.index;
            _ = self.seekInternal(Target, target, bias);
            return self.tree.copyRange(begin, self.index, self.context);
        }

        pub fn suffix(self: *Self) !Tree {
            self.ensureStart();
            const begin = self.index;
            self.setEnd();
            return self.tree.copyRange(begin, self.index, self.context);
        }

        pub fn rangeSummary(self: *Self, comptime Target: type, target: Target, bias: Bias, comptime Output: type) Output.Value {
            comptime {
                requireDecl(Output, "Value");
                requireDecl(Output, "zero");
                requireDecl(Output, "addSummary");
            }
            self.ensureStart();
            var result = Output.zero(self.context);
            _ = self.seekAccumulating(Target, target, bias, Output, &result);
            return result;
        }

        fn seekInternal(self: *Self, comptime Target: type, target: Target, bias: Bias) bool {
            return self.seekAccumulating(Target, target, bias, void, null);
        }

        fn seekAccumulating(self: *Self, comptime Target: type, target: Target, bias: Bias, comptime Output: type, output: if (Output == void) ?void else ?*Output.Value) bool {
            comptime requireDecl(Target, "compare");
            self.ensureStart();
            self.before_start = false;
            var matched_boundary = Target.compare(target, &self.position, self.context) == .eq;

            while (self.stack_len > 0) {
                const frame = self.stack[self.stack_len - 1];
                if (frame.node.isLeaf()) {
                    const summary = frame.node.itemSummary(frame.index);
                    var item_end = self.position;
                    Dimension.addSummary(&item_end, summary, self.context);
                    const comparison = Target.compare(target, &item_end, self.context);
                    if (comparison == .eq) matched_boundary = true;
                    if (comparison == .gt or (comparison == .eq and bias == .right)) {
                        if (Output != void) Output.addSummary(output.?, summary, self.context);
                        self.position = item_end;
                        self.index += 1;
                        _ = self.advanceSeekPath();
                    } else break;
                    continue;
                }

                const summary = frame.node.childSummary(frame.index);
                var child_end = self.position;
                Dimension.addSummary(&child_end, summary, self.context);
                const comparison = Target.compare(target, &child_end, self.context);
                if (comparison == .eq) matched_boundary = true;
                if (comparison == .gt or (comparison == .eq and bias == .right)) {
                    if (Output != void) Output.addSummary(output.?, summary, self.context);
                    self.position = child_end;
                    self.index += frame.node.child(frame.index).itemCount();
                    _ = self.advanceSeekPath();
                } else {
                    self.descendFirst(frame.node.child(frame.index));
                }
            }

            if (self.itemSummary()) |summary| {
                var item_end = self.position;
                Dimension.addSummary(&item_end, summary, self.context);
                return matched_boundary or Target.compare(target, &item_end, self.context) == .eq;
            }
            return matched_boundary or Target.compare(target, &self.position, self.context) == .eq;
        }

        fn ensureStart(self: *Self) void {
            if (self.did_seek and !self.before_start) return;
            self.did_seek = true;
            self.before_start = false;
            self.position = Dimension.zero(self.context);
            self.index = 0;
            self.stack_len = 0;
            const root = self.tree.cursorRoot();
            self.pushFrame(.{ .node = root, .index = 0, .start = self.position });
        }

        fn setEnd(self: *Self) void {
            self.did_seek = true;
            self.before_start = false;
            self.position = self.tree.extent(Dimension, self.context);
            self.index = self.tree.itemCount();
            self.stack_len = 0;
        }

        fn setBeforeStart(self: *Self) void {
            self.did_seek = true;
            self.before_start = true;
            self.position = Dimension.zero(self.context);
            self.index = 0;
            self.stack_len = 0;
        }

        fn pushFrame(self: *Self, frame: Frame) void {
            if (self.stack_len == max_height) @panic("SumTree cursor exceeded maximum height");
            self.stack[self.stack_len] = frame;
            self.stack_len += 1;
        }

        fn descendFirst(self: *Self, start_node: Node) void {
            var node = start_node;
            while (true) {
                self.pushFrame(.{ .node = node, .index = 0, .start = self.position });
                if (node.isLeaf()) {
                    if (node.len() == 0) self.stack_len = 0;
                    return;
                }
                node = node.child(0);
            }
        }

        fn descendLast(self: *Self, start_node: Node, node_start: Value) void {
            var node = start_node;
            var subtree_start = node_start;
            while (true) {
                const last = node.len() - 1;
                var entry_start = subtree_start;
                for (0..last) |index| Dimension.addSummary(&entry_start, if (node.isLeaf()) node.itemSummary(index) else node.childSummary(index), self.context);
                self.pushFrame(.{ .node = node, .index = last, .start = entry_start });
                if (node.isLeaf()) return;
                node = node.child(last);
                subtree_start = entry_start;
            }
        }

        fn descendEnd(self: *Self, start_node: Node) void {
            self.descendEndFrom(start_node, Dimension.zero(self.context));
        }

        fn descendEndFrom(self: *Self, start_node: Node, node_start: Value) void {
            var node = start_node;
            var subtree_start = node_start;
            while (true) {
                var node_end = subtree_start;
                for (0..node.len()) |index| Dimension.addSummary(&node_end, if (node.isLeaf()) node.itemSummary(index) else node.childSummary(index), self.context);
                self.pushFrame(.{ .node = node, .index = node.len(), .start = node_end });
                if (node.isLeaf()) return;
                var child_start = subtree_start;
                for (0..node.len() - 1) |index| Dimension.addSummary(&child_start, node.childSummary(index), self.context);
                subtree_start = child_start;
                node = node.child(node.len() - 1);
            }
        }

        fn advanceRaw(self: *Self) bool {
            if (self.before_start) {
                self.before_start = false;
                self.stack_len = 0;
                self.descendFirst(self.tree.cursorRoot());
                return self.stack_len > 0;
            }
            if (self.itemSummary()) |summary| {
                Dimension.addSummary(&self.position, summary, self.context);
                self.index += 1;
            }
            return self.advancePath();
        }

        fn retreatRaw(self: *Self) bool {
            if (self.before_start or self.index == 0) return false;
            if (!self.positionAtIndex(self.index - 1)) return false;
            self.before_start = false;
            return true;
        }

        fn advanceSeekPath(self: *Self) bool {
            while (self.stack_len > 0) {
                var frame = &self.stack[self.stack_len - 1];
                frame.index += 1;
                if (frame.index < frame.node.len()) {
                    frame.start = self.position;
                    return true;
                }
                self.stack_len -= 1;
            }
            return false;
        }

        fn advancePath(self: *Self) bool {
            while (self.stack_len > 0) {
                var frame = &self.stack[self.stack_len - 1];
                frame.index += 1;
                if (frame.index < frame.node.len()) {
                    frame.start = self.position;
                    if (frame.node.isLeaf()) return true;
                    self.descendFirst(frame.node.child(frame.index));
                    return self.stack_len > 0;
                }
                self.stack_len -= 1;
            }
            return false;
        }

        fn retreatPath(self: *Self) bool {
            while (self.stack_len > 0) {
                var frame = &self.stack[self.stack_len - 1];
                if (frame.node.isLeaf()) {
                    if (frame.index > 0) {
                        frame.index -= 1;
                        frame.start = positionBeforeEntry(frame.node, frame.index, frame.start, self.context);
                        return true;
                    }
                    self.stack_len -= 1;
                    continue;
                }
                if (frame.index >= frame.node.len()) {
                    frame.index = frame.node.len() - 1;
                    frame.start = positionBeforeEntry(frame.node, frame.index, frame.start, self.context);
                    self.descendLast(frame.node.child(frame.index), frame.start);
                    return true;
                }
                if (frame.index > 0) {
                    const node_start = self.stack[self.stack_len - 2].start;
                    frame.index -= 1;
                    frame.start = positionBeforeEntry(frame.node, frame.index, node_start, self.context);
                    self.descendLast(frame.node.child(frame.index), frame.start);
                    return true;
                }
                self.stack_len -= 1;
            }
            return false;
        }

        fn retreatToCandidate(self: *Self, filter: anytype) bool {
            if (self.stack_len == 0) self.descendEnd(self.tree.cursorRoot());
            while (self.stack_len > 0) {
                var frame = &self.stack[self.stack_len - 1];
                if (frame.index == 0) {
                    self.stack_len -= 1;
                    continue;
                }
                frame.index -= 1;
                frame.start = positionBeforeEntry(frame.node, frame.index, frame.start, self.context);
                self.position = frame.start;
                if (frame.node.isLeaf()) {
                    self.index -= 1;
                    return true;
                }
                const summary = frame.node.childSummary(frame.index);
                if (invokeFilter(filter, summary)) {
                    var child_end = frame.start;
                    Dimension.addSummary(&child_end, summary, self.context);
                    self.position = child_end;
                    self.descendEndFrom(frame.node.child(frame.index), frame.start);
                } else {
                    self.index -= frame.node.child(frame.index).itemCount();
                }
            }
            return false;
        }

        fn positionBeforeEntry(node: Node, index: usize, node_start: Value, context: Context) Value {
            var result = node_start;
            for (0..index) |entry| Dimension.addSummary(&result, if (node.isLeaf()) node.itemSummary(entry) else node.childSummary(entry), context);
            return result;
        }

        fn positionAtIndex(self: *Self, wanted: usize) bool {
            self.position = Dimension.zero(self.context);
            self.index = 0;
            self.stack_len = 0;
            var node = self.tree.cursorRoot();
            while (!node.isLeaf()) {
                var child_index: usize = 0;
                while (child_index < node.len()) : (child_index += 1) {
                    const child = node.child(child_index);
                    const child_count = child.itemCount();
                    if (wanted < self.index + child_count) break;
                    Dimension.addSummary(&self.position, node.childSummary(child_index), self.context);
                    self.index += child_count;
                }
                self.pushFrame(.{ .node = node, .index = child_index, .start = self.position });
                node = node.child(child_index);
            }
            const leaf_index = wanted - self.index;
            for (0..leaf_index) |entry| Dimension.addSummary(&self.position, node.itemSummary(entry), self.context);
            self.index = wanted;
            self.pushFrame(.{ .node = node, .index = leaf_index, .start = self.position });
            return true;
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
