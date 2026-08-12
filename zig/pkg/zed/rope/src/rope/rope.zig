const std = @import("std");
const sum_tree = @import("sum_tree");
const chunk_mod = @import("chunk.zig");

pub const Chunk = chunk_mod.Chunk;
pub const ChunkSlice = chunk_mod.ChunkSlice;
pub const Point = @import("point.zig").Point;
pub const PointUtf16 = @import("point_utf16.zig").PointUtf16;
pub const OffsetUtf16 = @import("offset_utf16.zig").OffsetUtf16;
pub const Unclipped = @import("unclipped.zig").Unclipped;
pub const TextSummary = @import("text_summary.zig").TextSummary;
pub const Bias = sum_tree.Bias;
pub const iterators = @import("iterators.zig");

pub const Error = anyerror;
pub const parallel_build_threshold: usize = 1024 * 1024;

pub const ByteRange = struct {
    start: usize,
    end: usize,
};

pub const RowRange = struct {
    start: u32,
    end: u32,
};

pub const ChunkSummary = struct {
    text: TextSummary = .{},
};

pub const ChunkOps = struct {
    pub const Summary = ChunkSummary;
    pub const Context = void;

    pub fn summary(chunk: *const Chunk, _: void) ChunkSummary {
        return .{ .text = chunk.asSlice().textSummary() };
    }

    pub fn zero(_: void) ChunkSummary {
        return .{};
    }

    pub fn addSummary(total: *ChunkSummary, value: *const ChunkSummary, _: void) void {
        total.text.addAssign(value.text);
    }

    pub fn cloneItem(value: *const Chunk, _: std.mem.Allocator) !Chunk {
        return value.*;
    }

    pub fn deinitItem(_: *Chunk, _: std.mem.Allocator) void {}

    pub fn cloneSummary(value: *const ChunkSummary, _: std.mem.Allocator) !ChunkSummary {
        return value.*;
    }

    pub fn deinitSummary(_: *ChunkSummary, _: std.mem.Allocator) void {}

    pub fn eqlSummary(a: *const ChunkSummary, b: *const ChunkSummary) bool {
        return std.meta.eql(a.*, b.*);
    }
};

pub const ChunkTree = sum_tree.SumTree(Chunk, ChunkOps, sum_tree.DefaultTreeBase);

pub fn Dimension(comptime ValueType: type) type {
    return struct {
        pub const Value = ValueType;

        pub fn zero(_: void) ValueType {
            return if (ValueType == usize) 0 else .{};
        }

        pub fn addSummary(value: *ValueType, summary: *const ChunkSummary, _: void) void {
            if (ValueType == usize) {
                value.* += summary.text.len;
            } else if (ValueType == OffsetUtf16) {
                value.addAssign(summary.text.len_utf16);
            } else if (ValueType == Point) {
                value.addAssign(summary.text.lines);
            } else if (ValueType == PointUtf16) {
                value.addAssign(summary.text.linesUtf16());
            } else if (ValueType == TextSummary) {
                value.addAssign(summary.text);
            } else {
                @compileError("unsupported rope dimension");
            }
        }
    };
}

pub fn ProductDimension(comptime First: type, comptime Second: type) type {
    const FirstDimension = Dimension(First);
    const SecondDimension = Dimension(Second);
    return struct {
        pub const Value = struct { first: First, second: Second };

        pub fn zero(context: void) Value {
            return .{ .first = FirstDimension.zero(context), .second = SecondDimension.zero(context) };
        }

        pub fn addSummary(value: *Value, summary: *const ChunkSummary, context: void) void {
            FirstDimension.addSummary(&value.first, summary, context);
            SecondDimension.addSummary(&value.second, summary, context);
        }
    };
}

pub fn ScalarTarget(comptime ValueType: type) type {
    return struct {
        value: ValueType,

        pub fn compare(self: @This(), position: *const ValueType, _: void) std.math.Order {
            if (ValueType == usize) return std.math.order(self.value, position.*);
            return self.value.order(position.*);
        }
    };
}

pub fn ProductTarget(comptime First: type, comptime Second: type) type {
    const Product = ProductDimension(First, Second).Value;
    return struct {
        value: First,

        pub fn compare(self: @This(), position: *const Product, _: void) std.math.Order {
            if (First == usize) return std.math.order(self.value, position.first);
            return self.value.order(position.first);
        }
    };
}

pub const Rope = struct {
    chunks: ChunkTree,

    pub fn init(allocator_value: std.mem.Allocator) !Rope {
        return .{ .chunks = try ChunkTree.init(allocator_value, {}) };
    }

    pub fn initText(allocator_value: std.mem.Allocator, text: []const u8) Error!Rope {
        if (!std.unicode.utf8ValidateSlice(text)) return error.InvalidUtf8;
        if (text.len == 0) return init(allocator_value);

        var chunks: std.ArrayList(Chunk) = .empty;
        defer chunks.deinit(allocator_value);
        try chunks.ensureTotalCapacity(allocator_value, std.math.divCeil(usize, text.len, chunk_mod.MAX_BASE - 3) catch unreachable);

        var offset: usize = 0;
        while (offset < text.len) {
            var end = @min(offset + chunk_mod.MAX_BASE, text.len);
            while (end > offset and !isUtf8Boundary(text, end)) end -= 1;
            std.debug.assert(end > offset);
            try chunks.append(allocator_value, Chunk.init(text[offset..end]) catch unreachable);
            offset = end;
        }

        return .{ .chunks = if (text.len >= parallel_build_threshold)
            try ChunkTree.fromParallel(allocator_value, chunks.items, {})
        else
            try ChunkTree.fromSlice(allocator_value, chunks.items, {}) };
    }

    /// Appends UTF-8 text while preserving snapshots and repairing the shared
    /// boundary chunk when it has room. The mutation commits only after every
    /// fallible operation has succeeded.
    pub fn push(self: *Rope, text: []const u8) Error!void {
        if (!std.unicode.utf8ValidateSlice(text)) return error.InvalidUtf8;
        if (text.len == 0) return;

        var suffix = try Rope.initText(self.allocator(), text);
        defer suffix.deinit();
        try self.append(&suffix);
    }

    pub fn pushFront(self: *Rope, text: []const u8) Error!void {
        if (!std.unicode.utf8ValidateSlice(text)) return error.InvalidUtf8;
        if (text.len == 0) return;

        var prefix = try Rope.initText(self.allocator(), text);
        errdefer prefix.deinit();
        try prefix.append(self);
        self.deinit();
        self.* = prefix;
    }

    /// Appends another rope without consuming it. Adjacent endpoint chunks are
    /// combined when they fit, avoiding underfull chunks at the join.
    pub fn append(self: *Rope, other: *const Rope) Error!void {
        if (other.isEmpty()) return;
        if (self.isEmpty()) {
            var replacement = other.clone();
            self.deinit();
            self.* = replacement;
            replacement = undefined;
            return;
        }

        var replacement = self.clone();
        errdefer replacement.deinit();
        var suffix = other.clone();
        defer suffix.deinit();
        try repairJoin(&replacement.chunks, &suffix.chunks);
        try replacement.chunks.append(&suffix.chunks, {});
        self.deinit();
        self.* = replacement;
    }

    /// Returns a persistent half-open byte slice. Boundary chunks are copied;
    /// complete interior chunks and subtrees remain shared.
    pub fn sliceBytes(self: *const Rope, range: ByteRange) Error!Rope {
        std.debug.assert(range.start <= range.end and range.end <= self.len());
        std.debug.assert(self.isCharBoundary(range.start) and self.isCharBoundary(range.end));
        if (range.start == range.end) return Rope.init(self.allocator());

        const first = self.locate(usize, range.start, .left);
        const last = if (range.end == self.len()) blk: {
            const item = self.chunks.itemAt(self.chunkCount() - 1).?;
            break :blk @TypeOf(first){ .start = self.len() - item.len(), .end = self.len(), .item = item };
        } else self.locate(usize, range.end, .right);
        std.debug.assert(first.item != null and last.item != null);

        const first_index = chunkIndexAtOffset(self, range.start, .left);
        const last_index = if (range.end == self.len()) self.chunkCount() - 1 else chunkIndexAtOffset(self, range.end, .right);
        var result_tree = try ChunkTree.init(self.allocator(), {});
        errdefer result_tree.deinit();
        if (first_index == last_index) {
            const item = first.item.?;
            const boundary = try Chunk.init(item.text()[range.start - first.start .. range.end - first.start]);
            try result_tree.push(boundary, {});
        } else {
            const first_boundary = try Chunk.init(first.item.?.text()[range.start - first.start ..]);
            try result_tree.push(first_boundary, {});
            var interior = try self.chunks.copyRange(first_index + 1, last_index, {});
            defer interior.deinit();
            try result_tree.append(&interior, {});
            const last_boundary = try Chunk.init(last.item.?.text()[0 .. range.end - last.start]);
            if (!last_boundary.isEmpty()) try result_tree.push(last_boundary, {});
        }
        try normalizeChunkBoundaries(&result_tree);
        return .{ .chunks = result_tree };
    }

    /// Slices complete rows. The end row is exclusive; the newline terminating
    /// the final included row is retained when present.
    pub fn sliceRows(self: *const Rope, range: RowRange) Error!Rope {
        std.debug.assert(range.start <= range.end and range.end <= self.maxPoint().row + 1);
        const start = self.pointToOffset(.new(range.start, 0));
        const end = if (range.end > self.maxPoint().row) self.len() else self.pointToOffset(.new(range.end, 0));
        return self.sliceBytes(.{ .start = start, .end = end });
    }

    /// Replaces one UTF-8-boundary byte range as a single transaction.
    pub fn replace(self: *Rope, range: ByteRange, text: []const u8) Error!void {
        std.debug.assert(range.start <= range.end and range.end <= self.len());
        std.debug.assert(self.isCharBoundary(range.start) and self.isCharBoundary(range.end));
        if (!std.unicode.utf8ValidateSlice(text)) return error.InvalidUtf8;

        var replacement = try self.sliceBytes(.{ .start = 0, .end = range.start });
        errdefer replacement.deinit();
        try replacement.push(text);
        var suffix = try self.sliceBytes(.{ .start = range.end, .end = self.len() });
        defer suffix.deinit();
        try replacement.append(&suffix);
        try normalizeChunkBoundaries(&replacement.chunks);
        self.deinit();
        self.* = replacement;
    }

    pub fn clone(self: Rope) Rope {
        return .{ .chunks = self.chunks.clone() };
    }

    pub fn deinit(self: *Rope) void {
        self.chunks.deinit();
        self.* = undefined;
    }

    pub fn allocator(self: *const Rope) std.mem.Allocator {
        return self.chunks.getAllocator();
    }

    pub fn summary(self: *const Rope) TextSummary {
        return self.chunks.summary().text;
    }

    pub fn len(self: *const Rope) usize {
        return self.summary().len;
    }

    pub fn isEmpty(self: *const Rope) bool {
        return self.len() == 0;
    }

    pub fn maxPoint(self: *const Rope) Point {
        return self.summary().lines;
    }

    pub fn maxPointUtf16(self: *const Rope) PointUtf16 {
        return self.summary().linesUtf16();
    }

    pub fn chunkCount(self: *const Rope) usize {
        return self.chunks.itemCount();
    }

    pub fn cursor(self: *const Rope, offset: usize) iterators.Cursor {
        return iterators.Cursor.init(self, offset);
    }

    pub fn chunksIterator(self: *const Rope) iterators.Chunks {
        return self.chunksInRange(.{ .start = 0, .end = self.len() });
    }

    pub fn chunksInRange(self: *const Rope, range: ByteRange) iterators.Chunks {
        return iterators.Chunks.init(self, range, false);
    }

    pub fn reversedChunksInRange(self: *const Rope, range: ByteRange) iterators.Chunks {
        return iterators.Chunks.init(self, range, true);
    }

    pub fn bytesInRange(self: *const Rope, range: ByteRange) iterators.Bytes {
        return iterators.Bytes.init(self, range, false);
    }

    pub fn reversedBytesInRange(self: *const Rope, range: ByteRange) iterators.Bytes {
        return iterators.Bytes.init(self, range, true);
    }

    pub fn scalars(self: *const Rope) iterators.Scalars {
        return iterators.Scalars.init(self, .{ .start = 0, .end = self.len() }, false);
    }

    pub fn scalarsAt(self: *const Rope, start: usize) iterators.Scalars {
        return iterators.Scalars.init(self, .{ .start = start, .end = self.len() }, false);
    }

    pub fn reversedScalarsAt(self: *const Rope, end: usize) iterators.Scalars {
        return iterators.Scalars.init(self, .{ .start = 0, .end = end }, true);
    }

    pub fn lines(self: *const Rope, allocator_value: std.mem.Allocator) iterators.Lines {
        return self.chunksIterator().lines(allocator_value);
    }

    pub fn isCharBoundary(self: *const Rope, offset: usize) bool {
        if (offset > self.len()) return false;
        if (offset == self.len()) return true;
        const located = self.locate(usize, offset, .left);
        return located.item != null and located.item.?.isCharBoundary(offset - located.start);
    }

    pub fn floorCharBoundary(self: *const Rope, index: usize) usize {
        if (index >= self.len()) return self.len();
        const located = self.locate(usize, index, .left);
        return if (located.item) |item| located.start + item.floorCharBoundary(index - located.start) else self.len();
    }

    pub fn ceilCharBoundary(self: *const Rope, index: usize) usize {
        if (index >= self.len()) return self.len();
        const located = self.locate(usize, index, .left);
        return if (located.item) |item| located.start + item.asSlice().ceilCharBoundary(index - located.start) else self.len();
    }

    pub fn clipOffset(self: *const Rope, offset: usize, bias: Bias) usize {
        return switch (bias) {
            .left => self.floorCharBoundary(offset),
            .right => self.ceilCharBoundary(offset),
        };
    }

    pub fn offsetToOffsetUtf16(self: *const Rope, offset: usize) OffsetUtf16 {
        if (offset >= self.len()) return self.summary().len_utf16;
        const located = self.locateProduct(usize, OffsetUtf16, offset, .left);
        const local = located.item.?.asSlice().offsetToOffsetUtf16(offset - located.start.first) catch return located.start.second;
        return located.start.second.add(local);
    }

    pub fn offsetUtf16ToOffset(self: *const Rope, offset: OffsetUtf16) usize {
        if (offset.order(self.summary().len_utf16) != .lt) return self.len();
        const located = self.locateProduct(OffsetUtf16, usize, offset, .left);
        const local = located.item.?.asSlice().offsetUtf16ToOffset(offset.sub(located.start.first)) catch return located.start.second;
        return located.start.second + local;
    }

    pub fn offsetToPoint(self: *const Rope, offset: usize) Point {
        if (offset >= self.len()) return self.maxPoint();
        const located = self.locateProduct(usize, Point, offset, .left);
        const local = located.item.?.asSlice().offsetToPoint(offset - located.start.first) catch return located.start.second;
        return located.start.second.add(local);
    }

    pub fn offsetToPointUtf16(self: *const Rope, offset: usize) PointUtf16 {
        if (offset >= self.len()) return self.maxPointUtf16();
        const located = self.locateProduct(usize, PointUtf16, offset, .left);
        const local = located.item.?.asSlice().offsetToPointUtf16(offset - located.start.first) catch return located.start.second;
        return located.start.second.add(local);
    }

    pub fn pointToOffset(self: *const Rope, point: Point) usize {
        if (point.order(self.maxPoint()) != .lt) return self.len();
        const located = self.locateProduct(Point, usize, point, .left);
        const chunk_slice = located.item.?.asSlice();
        const local_point = point.sub(located.start.first);
        const row_range = chunk_slice.offsetRangeForRow(local_point.row) catch return located.end.second;
        const local = row_range.start + @min(@as(usize, local_point.column), row_range.end - row_range.start);
        return located.start.second + local;
    }

    pub fn pointToPointUtf16(self: *const Rope, point: Point) PointUtf16 {
        if (point.order(self.maxPoint()) != .lt) return self.maxPointUtf16();
        const located = self.locateProduct(Point, PointUtf16, point, .left);
        const chunk_slice = located.item.?.asSlice();
        const local_point = point.sub(located.start.first);
        const row_range = chunk_slice.offsetRangeForRow(local_point.row) catch return located.start.second;
        const local_offset = row_range.start + @min(@as(usize, local_point.column), row_range.end - row_range.start);
        var row_start: usize = 0;
        var scan: usize = 0;
        while (scan < local_offset) : (scan += 1) {
            if (chunk_slice.text[scan] == '\n') row_start = scan + 1;
        }
        var utf16_column: u32 = 0;
        scan = row_start;
        while (scan < local_offset) {
            const byte = chunk_slice.text[scan];
            if ((byte & 0xc0) != 0x80) utf16_column += if (byte >= 0xf0) 2 else 1;
            scan += 1;
        }
        return located.start.second.add(.new(local_point.row, utf16_column));
    }

    pub fn pointUtf16ToOffset(self: *const Rope, point: PointUtf16) usize {
        return self.pointUtf16ToOffsetImpl(point, false);
    }

    pub fn unclippedPointUtf16ToOffset(self: *const Rope, point: Unclipped(PointUtf16)) usize {
        return self.pointUtf16ToOffsetImpl(point.value, true);
    }

    pub fn pointUtf16ToPoint(self: *const Rope, point: PointUtf16) Point {
        if (point.order(self.maxPointUtf16()) != .lt) return self.maxPoint();
        const located = self.locateProduct(PointUtf16, Point, point, .left);
        const local_point = point.sub(located.start.first);
        const local_offset = located.item.?.asSlice().pointUtf16ToOffset(local_point, false) catch return located.start.second;
        const local = located.item.?.asSlice().offsetToPoint(local_offset) catch return located.start.second;
        return located.start.second.add(local);
    }

    pub fn unclippedPointUtf16ToPoint(self: *const Rope, point: Unclipped(PointUtf16)) Point {
        if (point.value.order(self.maxPointUtf16()) != .lt) return self.maxPoint();
        const located = self.locateProduct(PointUtf16, Point, point.value, .left);
        const local = located.item.?.asSlice().unclippedPointUtf16ToPoint(.init(point.value.sub(located.start.first)));
        return located.start.second.add(local);
    }

    pub fn clipOffsetUtf16(self: *const Rope, offset: OffsetUtf16, bias: Bias) OffsetUtf16 {
        if (offset.order(self.summary().len_utf16) != .lt) return self.summary().len_utf16;
        const located = self.locate(OffsetUtf16, offset, .right);
        if (located.item) |item| return located.start.add(item.asSlice().clipOffsetUtf16(offset.sub(located.start), bias));
        return self.summary().len_utf16;
    }

    pub fn clipPoint(self: *const Rope, point: Point, bias: Bias) Point {
        if (point.order(self.maxPoint()) != .lt) return self.maxPoint();
        const located = self.locate(Point, point, .right);
        if (located.item) |item| return located.start.add(item.asSlice().clipPoint(point.sub(located.start), bias));
        return self.maxPoint();
    }

    pub fn clipPointUtf16(self: *const Rope, point: Unclipped(PointUtf16), bias: Bias) PointUtf16 {
        if (point.value.order(self.maxPointUtf16()) != .lt) return self.maxPointUtf16();
        const located = self.locate(PointUtf16, point.value, .right);
        if (located.item) |item| return located.start.add(item.asSlice().clipPointUtf16(.init(point.value.sub(located.start)), bias));
        return self.maxPointUtf16();
    }

    pub fn startsWith(self: *const Rope, pattern: []const u8) bool {
        if (pattern.len > self.len()) return false;
        var remaining = pattern;
        var iterator = self.chunks.iterator();
        while (iterator.next()) |item| {
            const take = @min(remaining.len, item.len());
            if (!std.mem.eql(u8, remaining[0..take], item.text()[0..take])) return false;
            remaining = remaining[take..];
            if (remaining.len == 0) return true;
        }
        return remaining.len == 0;
    }

    pub fn endsWith(self: *const Rope, pattern: []const u8) bool {
        if (pattern.len > self.len()) return false;
        var remaining = pattern;
        var index = self.chunkCount();
        while (index > 0) {
            index -= 1;
            const item = self.chunks.itemAt(index).?;
            const take = @min(remaining.len, item.len());
            if (!std.mem.eql(u8, remaining[remaining.len - take ..], item.text()[item.len() - take ..])) return false;
            remaining = remaining[0 .. remaining.len - take];
            if (remaining.len == 0) return true;
        }
        return remaining.len == 0;
    }

    pub fn lineLen(self: *const Rope, row: u32) u32 {
        return self.clipPoint(.new(row, std.math.maxInt(u32)), .left).column;
    }

    pub fn write(self: *const Rope, writer: *std.Io.Writer) !void {
        var iterator = self.chunks.iterator();
        while (iterator.next()) |item| try writer.writeAll(item.text());
    }

    pub fn toOwnedSlice(self: *const Rope, allocator_value: std.mem.Allocator) ![]u8 {
        const result = try allocator_value.alloc(u8, self.len());
        errdefer allocator_value.free(result);
        var offset: usize = 0;
        var iterator = self.chunks.iterator();
        while (iterator.next()) |item| {
            @memcpy(result[offset .. offset + item.len()], item.text());
            offset += item.len();
        }
        return result;
    }

    pub fn validate(self: *const Rope) !void {
        try self.chunks.validate({});
        var iterator = self.chunks.iterator();
        var computed = TextSummary.zero();
        var index: usize = 0;
        while (iterator.next()) |item| : (index += 1) {
            item.checkInvariants() catch return error.InvalidChunkInvariant;
            if (index + 1 < self.chunkCount() and item.len() + 3 < chunk_mod.MIN_BASE) return error.ChunkUnderflow;
            computed.addAssign(item.asSlice().textSummary());
        }
        if (!std.meta.eql(computed, self.summary())) return error.InvalidChunkInvariant;
    }

    const LocateResult = struct { start: usize, end: usize, item: ?*const Chunk };

    fn locate(self: *const Rope, comptime ValueType: type, target: ValueType, bias: Bias) struct { start: ValueType, end: ValueType, item: ?*const Chunk } {
        var tree_cursor = self.chunks.cursor(Dimension(ValueType), {});
        _ = tree_cursor.seek(ScalarTarget(ValueType), .{ .value = target }, bias);
        return .{ .start = tree_cursor.start().*, .end = tree_cursor.end(), .item = tree_cursor.item() };
    }

    fn locateProduct(self: *const Rope, comptime First: type, comptime Second: type, target: First, bias: Bias) struct {
        start: ProductDimension(First, Second).Value,
        end: ProductDimension(First, Second).Value,
        item: ?*const Chunk,
    } {
        const D = ProductDimension(First, Second);
        var tree_cursor = self.chunks.cursor(D, {});
        _ = tree_cursor.seek(ProductTarget(First, Second), .{ .value = target }, bias);
        return .{ .start = tree_cursor.start().*, .end = tree_cursor.end(), .item = tree_cursor.item() };
    }

    fn chunkIndexAtOffset(self: *const Rope, offset: usize, bias: Bias) usize {
        var tree_cursor = self.chunks.cursor(Dimension(usize), {});
        _ = tree_cursor.seek(ScalarTarget(usize), .{ .value = offset }, bias);
        return if (tree_cursor.item() == null) self.chunkCount() - 1 else tree_cursor.index;
    }

    fn pointUtf16ToOffsetImpl(self: *const Rope, point: PointUtf16, clip: bool) usize {
        if (point.order(self.maxPointUtf16()) != .lt) return self.len();
        const located = self.locateProduct(PointUtf16, usize, point, .left);
        const local = located.item.?.asSlice().pointUtf16ToOffset(point.sub(located.start.first), clip) catch return located.end.second;
        return located.start.second + local;
    }
};

fn normalizeChunkBoundaries(tree: *ChunkTree) !void {
    if (tree.itemCount() < 2) return;
    const allocator_value = tree.getAllocator();
    var normalized: std.ArrayList(Chunk) = .empty;
    defer normalized.deinit(allocator_value);
    try normalized.ensureTotalCapacity(allocator_value, tree.itemCount() + 1);

    var iterator = tree.iterator();
    while (iterator.next()) |item| {
        try normalized.append(allocator_value, item.*);
        while (normalized.items.len >= 2 and normalized.items[normalized.items.len - 2].len() + 3 < chunk_mod.MIN_BASE) {
            const right = normalized.pop().?;
            const left = normalized.pop().?;
            var boundary: [chunk_mod.MAX_BASE * 2]u8 = undefined;
            const total = left.len() + right.len();
            @memcpy(boundary[0..left.len()], left.text());
            @memcpy(boundary[left.len()..total], right.text());
            if (total <= chunk_mod.MAX_BASE) {
                try normalized.append(allocator_value, try Chunk.init(boundary[0..total]));
            } else {
                var split = total / 2;
                while (!isUtf8Boundary(boundary[0..total], split)) split -= 1;
                try normalized.append(allocator_value, try Chunk.init(boundary[0..split]));
                try normalized.append(allocator_value, try Chunk.init(boundary[split..total]));
            }
        }
    }
    var replacement = try ChunkTree.fromParallel(allocator_value, normalized.items, {});
    errdefer replacement.deinit();
    tree.deinit();
    tree.* = replacement;
}

fn repairJoin(left: *ChunkTree, right: *ChunkTree) !void {
    if (left.isEmpty() or right.isEmpty()) return;
    const left_last = left.itemAt(left.itemCount() - 1).?.*;
    const right_first = right.itemAt(0).?.*;
    const total = left_last.len() + right_first.len();
    if (total >= chunk_mod.MIN_BASE and total > chunk_mod.MAX_BASE) return;

    var boundary: [chunk_mod.MAX_BASE * 2]u8 = undefined;
    @memcpy(boundary[0..left_last.len()], left_last.text());
    @memcpy(boundary[left_last.len()..total], right_first.text());

    var left_prefix = try left.copyRange(0, left.itemCount() - 1, {});
    errdefer left_prefix.deinit();
    if (total <= chunk_mod.MAX_BASE) {
        try left_prefix.push(try Chunk.init(boundary[0..total]), {});
    } else {
        var split = total / 2;
        while (!isUtf8Boundary(boundary[0..total], split)) split -= 1;
        try left_prefix.push(try Chunk.init(boundary[0..split]), {});
        try left_prefix.push(try Chunk.init(boundary[split..total]), {});
    }
    left.deinit();
    left.* = left_prefix;

    var remainder = try right.copyRange(1, right.itemCount(), {});
    errdefer remainder.deinit();
    right.deinit();
    right.* = remainder;
}

fn isUtf8Boundary(text: []const u8, offset: usize) bool {
    return offset == 0 or offset == text.len or (text[offset] & 0xc0) != 0x80;
}
