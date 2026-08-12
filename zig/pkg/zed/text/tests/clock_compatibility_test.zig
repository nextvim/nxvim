const std = @import("std");
const text = @import("text");

test "text consumes the standalone clock package" {
    try std.testing.expect(text.ReplicaId == text.clock.ReplicaId);
    var timestamp = text.clock.Lamport.new(text.ReplicaId.LOCAL);
    try std.testing.expect(timestamp.tick().eql(.{ .value = 1, .replica_id = .LOCAL }));

    var version = text.clock.Global.init(std.testing.allocator);
    defer version.deinit();
    try version.observe(.{ .value = 7, .replica_id = .new(1024) });
    try std.testing.expect(version.observed(.{ .value = 7, .replica_id = .new(1024) }));
}
