const std = @import("std");

pub const RealSystemClock = struct {
    pub fn now(_: RealSystemClock) std.time.Instant {
        return std.time.Instant.now() catch unreachable;
    }
};

pub const FakeSystemClock = struct {
    nanoseconds: std.atomic.Value(u64),

    pub fn init(nanoseconds: u64) FakeSystemClock {
        return .{ .nanoseconds = .init(nanoseconds) };
    }

    pub fn now(self: *const FakeSystemClock) u64 {
        return self.nanoseconds.load(.acquire);
    }

    pub fn setNow(self: *FakeSystemClock, nanoseconds: u64) void {
        self.nanoseconds.store(nanoseconds, .release);
    }

    pub fn advance(self: *FakeSystemClock, nanoseconds: u64) void {
        _ = self.nanoseconds.fetchAdd(nanoseconds, .acq_rel);
    }
};
