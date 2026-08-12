const std = @import("std");
const clock = @import("clock");

test "replica constants, ordering, and remote classification" {
    try std.testing.expectEqual(@as(u16, 0), clock.ReplicaId.LOCAL.asU16());
    try std.testing.expectEqual(@as(u16, 1), clock.ReplicaId.REMOTE_SERVER.asU16());
    try std.testing.expectEqual(@as(u16, 2), clock.ReplicaId.AGENT.asU16());
    try std.testing.expectEqual(@as(u16, 3), clock.ReplicaId.LOCAL_BRANCH.asU16());
    try std.testing.expectEqual(@as(u16, 8), clock.ReplicaId.FIRST_COLLAB_ID.asU16());
    try std.testing.expect(!clock.ReplicaId.LOCAL.isRemote());
    try std.testing.expect(clock.ReplicaId.REMOTE_SERVER.isRemote());
    try std.testing.expect(clock.ReplicaId.new(8).isRemote());
    try std.testing.expectEqual(std.math.Order.lt, clock.ReplicaId.new(8).order(.new(65535)));
}

test "lamport tick observe ordering and packed representation" {
    var local = clock.Lamport.new(.LOCAL);
    try std.testing.expectEqual(@as(u32, 1), local.value);
    try std.testing.expectEqual(@as(u64, 1) << 32, local.asU64());
    try std.testing.expect(local.tick().eql(.{ .value = 1, .replica_id = .LOCAL }));
    try std.testing.expectEqual(@as(u32, 2), local.value);
    local.observe(.{ .value = 9, .replica_id = .REMOTE_SERVER });
    try std.testing.expectEqual(@as(u32, 10), local.value);
    const local_four = clock.Lamport{ .value = 4, .replica_id = .LOCAL };
    try std.testing.expectEqual(std.math.Order.lt, local_four.order(.{ .value = 4, .replica_id = .AGENT }));
    try std.testing.expectEqual(@as(u64, 0xffff_ffff_0000_ffff), clock.Lamport.MAX.asU64());
}
