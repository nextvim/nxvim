const std = @import("std");
const clock = @import("clock");

test "fake system clock is deterministic" {
    var fake = clock.FakeSystemClock.init(100);
    try std.testing.expectEqual(@as(u64, 100), fake.now());
    fake.advance(25);
    try std.testing.expectEqual(@as(u64, 125), fake.now());
    fake.setNow(7);
    try std.testing.expectEqual(@as(u64, 7), fake.now());
}
