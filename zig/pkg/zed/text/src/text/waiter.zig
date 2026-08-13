const std = @import("std");
const clock = @import("clock");

const Mutex = struct {
    locked: std.atomic.Value(bool) = .init(false),
    fn lock(self: *Mutex) void {
        while (self.locked.cmpxchgWeak(false, true, .acquire, .monotonic) != null)
            while (self.locked.load(.monotonic)) std.atomic.spinLoopHint();
    }
    fn unlock(self: *Mutex) void {
        self.locked.store(false, .release);
    }
};

pub const State = struct {
    allocator: std.mem.Allocator,
    refs: std.atomic.Value(usize) = std.atomic.Value(usize).init(2),
    mutex: Mutex = .{},
    target: clock.Global,
    ready: bool = false,
    cancelled: bool = false,

    pub fn create(allocator: std.mem.Allocator, target: *const clock.Global) !*State {
        const state = try allocator.create(State);
        errdefer allocator.destroy(state);
        state.* = .{ .allocator = allocator, .target = try target.clone(allocator) };
        return state;
    }

    pub fn release(self: *State) void {
        if (self.refs.fetchSub(1, .acq_rel) == 1) {
            self.target.deinit();
            self.allocator.destroy(self);
        }
    }

    pub fn resolve(self: *State, version: *const clock.Global) bool {
        self.mutex.lock();
        defer self.mutex.unlock();
        if (self.cancelled or self.ready) return self.cancelled;
        if (version.observedAll(&self.target)) self.ready = true;
        return false;
    }

    pub fn isFinished(self: *State) bool {
        self.mutex.lock();
        defer self.mutex.unlock();
        return self.ready or self.cancelled;
    }

    pub fn cancelFromBuffer(self: *State) void {
        self.mutex.lock();
        self.cancelled = true;
        self.mutex.unlock();
    }
};

pub const WaitHandle = struct {
    state: ?*State,

    pub fn isReady(self: *const WaitHandle) bool {
        const state = self.state orelse return false;
        state.mutex.lock();
        defer state.mutex.unlock();
        return state.ready;
    }

    pub fn isCancelled(self: *const WaitHandle) bool {
        const state = self.state orelse return true;
        state.mutex.lock();
        defer state.mutex.unlock();
        return state.cancelled;
    }

    pub fn cancel(self: *WaitHandle) void {
        const state = self.state orelse return;
        state.mutex.lock();
        state.cancelled = true;
        state.mutex.unlock();
    }

    pub fn deinit(self: *WaitHandle) void {
        const state = self.state orelse return;
        self.state = null;
        state.release();
    }
};
