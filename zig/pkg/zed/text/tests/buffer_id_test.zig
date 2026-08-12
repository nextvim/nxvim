const std = @import("std");
const BufferId = @import("text").BufferId;

test "BufferId rejects zero and round-trips protocol values" {
    try std.testing.expectError(error.ZeroBufferId, BufferId.new(0));
    try std.testing.expectError(error.ZeroBufferId, BufferId.fromProto(0));

    const id = try BufferId.fromProto(42);
    try std.testing.expectEqual(@as(u64, 42), id.get());
    try std.testing.expectEqual(@as(u64, 42), id.toProto());
    try std.testing.expect(id.eql(try BufferId.new(42)));
    try std.testing.expectEqual(std.math.Order.lt, id.order(try BufferId.new(43)));
}

test "BufferId next is saturating post-increment" {
    var id = try BufferId.new(7);
    try std.testing.expectEqual(@as(u64, 7), id.next().get());
    try std.testing.expectEqual(@as(u64, 8), id.get());

    var maximum = try BufferId.new(std.math.maxInt(u64));
    try std.testing.expectEqual(std.math.maxInt(u64), maximum.next().get());
    try std.testing.expectEqual(std.math.maxInt(u64), maximum.get());
}

test "BufferId checked increment reports overflow transactionally" {
    var id = try BufferId.new(7);
    try std.testing.expectEqual(@as(u64, 7), (try id.checkedNext()).get());
    try std.testing.expectEqual(@as(u64, 8), id.get());

    var maximum = try BufferId.new(std.math.maxInt(u64));
    try std.testing.expectError(error.BufferIdOverflow, maximum.checkedNext());
    try std.testing.expectEqual(std.math.maxInt(u64), maximum.get());
}
