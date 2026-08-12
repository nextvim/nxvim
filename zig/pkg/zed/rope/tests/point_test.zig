const std = @import("std");
const rope = @import("rope");
const Point = rope.Point;
const PointUtf16 = rope.PointUtf16;
const OffsetUtf16 = rope.OffsetUtf16;
const Unclipped = rope.Unclipped;

fn checkPointType(comptime P: type) !void {
    const zero = P.zero();
    try std.testing.expect(zero.isZero());
    try std.testing.expectEqual(std.math.Order.lt, P.new(1, 99).order(P.new(2, 0)));
    try std.testing.expectEqual(std.math.Order.lt, P.new(2, 3).order(P.new(2, 4)));
    try std.testing.expectEqual(P.new(3, 7), P.new(3, 5).add(P.new(0, 2)));
    try std.testing.expectEqual(P.new(5, 4), P.new(3, 5).add(P.new(2, 4)));
    try std.testing.expectEqual(P.new(0, 3), P.new(7, 8).sub(P.new(7, 5)));
    try std.testing.expectEqual(P.new(4, 8), P.new(7, 8).sub(P.new(3, 5)));
    try std.testing.expectEqual(P.zero(), P.new(1, 2).saturatingSub(P.new(1, 3)));
}

test "Point parses bytes and implements pinned point arithmetic" {
    try checkPointType(Point);
    try std.testing.expectEqual(Point.new(0, 0), Point.parse(""));
    try std.testing.expectEqual(Point.new(0, 6), Point.parse("é🙂"));
    try std.testing.expectEqual(Point.new(2, 0), Point.parse("a\nβ\n"));
    const range = Point.rowRange(2, 5);
    try std.testing.expectEqual(Point.new(2, 0), range.start);
    try std.testing.expectEqual(Point.new(5, 0), range.end);
}

test "PointUtf16 mirrors point ordering and arithmetic" {
    try checkPointType(PointUtf16);
}

test "OffsetUtf16 and Unclipped are composable value types" {
    const a = OffsetUtf16.new(7);
    const b = OffsetUtf16.new(3);
    try std.testing.expectEqual(OffsetUtf16.new(10), a.add(b));
    try std.testing.expectEqual(OffsetUtf16.new(4), a.sub(b));
    try std.testing.expectEqual(OffsetUtf16.zero(), b.saturatingSub(a));

    const U = Unclipped(Point);
    var value = U.init(Point.new(1, 3));
    value.addAssign(U.init(Point.new(2, 4)));
    try std.testing.expectEqual(Point.new(3, 4), value.value);
    try std.testing.expectEqual(Point.new(2, 4), value.sub(U.init(Point.new(1, 0))).value);
}
