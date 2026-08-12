const std = @import("std");
const sum_tree = @import("sum_tree");

const Point = struct {
    row: usize = 0,
    column: usize = 0,

    fn add(self: *Point, other: Point) void {
        if (other.row == 0) {
            self.column += other.column;
        } else {
            self.row += other.row;
            self.column = other.column;
        }
    }
};

const Chunk = struct {
    bytes: []const u8,
    utf16: usize,
    point: Point,
};
const TextSummary = struct {
    bytes: usize,
    utf16: usize,
    point: Point,
};
const Ops = struct {
    pub const Summary = TextSummary;
    pub const Context = void;
    pub fn summary(chunk: *const Chunk, _: void) TextSummary {
        return .{ .bytes = chunk.bytes.len, .utf16 = chunk.utf16, .point = chunk.point };
    }
    pub fn zero(_: void) TextSummary {
        return .{ .bytes = 0, .utf16 = 0, .point = .{} };
    }
    pub fn addSummary(a: *TextSummary, b: *const TextSummary, _: void) void {
        a.bytes += b.bytes;
        a.utf16 += b.utf16;
        a.point.add(b.point);
    }
    pub fn cloneItem(v: *const Chunk, _: std.mem.Allocator) !Chunk {
        return v.*;
    }
    pub fn deinitItem(_: *Chunk, _: std.mem.Allocator) void {}
    pub fn cloneSummary(v: *const TextSummary, _: std.mem.Allocator) !TextSummary {
        return v.*;
    }
    pub fn deinitSummary(_: *TextSummary, _: std.mem.Allocator) void {}
    pub fn eqlSummary(a: *const TextSummary, b: *const TextSummary) bool {
        return a.bytes == b.bytes and a.utf16 == b.utf16 and std.meta.eql(a.point, b.point);
    }
};
const Bytes = struct {
    pub const Value = usize;
    pub fn zero(_: void) usize {
        return 0;
    }
    pub fn addSummary(a: *usize, b: *const TextSummary, _: void) void {
        a.* += b.bytes;
    }
};
const Utf16 = struct {
    pub const Value = usize;
    pub fn zero(_: void) usize {
        return 0;
    }
    pub fn addSummary(a: *usize, b: *const TextSummary, _: void) void {
        a.* += b.utf16;
    }
};
const Points = struct {
    pub const Value = Point;
    pub fn zero(_: void) Point {
        return .{};
    }
    pub fn addSummary(a: *Point, b: *const TextSummary, _: void) void {
        a.add(b.point);
    }
};
fn Product(comptime A: type, comptime B: type) type {
    return struct {
        pub const Value = struct { first: A.Value, second: B.Value };
        pub fn zero(context: void) Value {
            return .{ .first = A.zero(context), .second = B.zero(context) };
        }
        pub fn addSummary(value: *Value, summary: *const TextSummary, context: void) void {
            A.addSummary(&value.first, summary, context);
            B.addSummary(&value.second, summary, context);
        }
    };
}
fn ScalarTarget(comptime V: type) type {
    return struct {
        value: V,
        pub fn compare(self: @This(), value: *const V, _: void) std.math.Order {
            return std.math.order(self.value, value.*);
        }
    };
}
const ByteTarget = ScalarTarget(usize);
const ProductByteTarget = struct {
    value: usize,
    pub fn compare(self: ProductByteTarget, value: *const Product(Bytes, Utf16).Value, _: void) std.math.Order {
        return std.math.order(self.value, value.first);
    }
};

fn makeChunk(bytes: []const u8, utf16: usize, row: usize, column: usize) Chunk {
    return .{ .bytes = bytes, .utf16 = utf16, .point = .{ .row = row, .column = column } };
}

test "rope-like byte utf16 point dimensions persistence and extraction" {
    const Tree = sum_tree.SumTree(Chunk, Ops, 2);
    var tree = try Tree.fromSlice(std.testing.allocator, &.{
        makeChunk("hello\n", 6, 1, 0),
        makeChunk("😀", 2, 0, 4),
        makeChunk("world", 5, 0, 5),
        makeChunk("\n", 1, 1, 0),
        makeChunk("tail", 4, 0, 4),
    }, {});
    defer tree.deinit();
    var snapshot = tree.clone();
    defer snapshot.deinit();

    try std.testing.expectEqual(@as(usize, 20), tree.summary().bytes);
    try std.testing.expectEqual(@as(usize, 18), tree.summary().utf16);
    try std.testing.expectEqual(Point{ .row = 2, .column = 4 }, tree.summary().point);

    var cursor = tree.cursor(Product(Bytes, Utf16), {});
    try std.testing.expect(cursor.seek(ProductByteTarget, .{ .value = 6 }, .right));
    try std.testing.expectEqualStrings("😀", cursor.item().?.bytes);
    try std.testing.expectEqual(@as(usize, 6), cursor.start().first);
    try std.testing.expectEqual(@as(usize, 6), cursor.start().second);

    var slice = try cursor.slice(ProductByteTarget, .{ .value = 15 }, .right);
    defer slice.deinit();
    try std.testing.expectEqual(@as(usize, 2), slice.itemCount());
    try std.testing.expectEqualStrings("😀", slice.first().?.bytes);
    try std.testing.expectEqualStrings("world", slice.last().?.bytes);

    var suffix_cursor = tree.cursor(Bytes, {});
    _ = suffix_cursor.seek(ByteTarget, .{ .value = 15 }, .right);
    var suffix = try suffix_cursor.suffix();
    defer suffix.deinit();
    try std.testing.expectEqualStrings("\n", suffix.first().?.bytes);
    try std.testing.expectEqualStrings("tail", suffix.last().?.bytes);

    try tree.updateLast({}, struct {
        fn update(value: *Chunk) void {
            value.bytes = "TAIL";
        }
    }.update);
    try std.testing.expectEqualStrings("TAIL", tree.last().?.bytes);
    try std.testing.expectEqualStrings("tail", snapshot.last().?.bytes);

    var point_cursor = tree.cursor(Points, {});
    point_cursor.next();
    try std.testing.expectEqual(Point{}, point_cursor.start().*);
    point_cursor.next();
    try std.testing.expectEqual(Point{ .row = 1, .column = 0 }, point_cursor.start().*);

    try tree.validate({});
    try snapshot.validate({});
}
