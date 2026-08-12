const std = @import("std");

/// Zig 0.16's general mutex is I/O-context based. This small critical-section
/// mutex keeps the API independent of an `std.Io` value; operations under it
/// only perform bounded memory work and never wait on external I/O.
const Mutex = struct {
    locked: std.atomic.Value(bool) = .init(false),

    fn lock(self: *Mutex) void {
        while (self.locked.cmpxchgWeak(false, true, .acquire, .monotonic) != null) {
            while (self.locked.load(.monotonic)) std.atomic.spinLoopHint();
        }
    }

    fn unlock(self: *Mutex) void {
        self.locked.store(false, .release);
    }
};

/// Ownership and accumulation contract used by Topic and Subscription.
///
/// Ops must provide:
///   init(allocator) !T
///   clone(value: *const T, allocator) !T
///   deinit(value: *T, allocator) void
///   combine(current: *const T, update: *const T, allocator) !T
///
/// `combine` must not modify either input. This permits publish to provide a
/// no-loss guarantee: if any allocation fails, no subscriber is changed.
fn SharedState(comptime T: type, comptime Ops: type) type {
    return struct {
        const Self = @This();

        allocator: std.mem.Allocator,
        /// Counts the Subscription owner plus every Topic weak entry. The Topic
        /// entries keep the control block allocated, but not the value alive.
        ref_count: std.atomic.Value(usize),
        mutex: Mutex = .{},
        alive: bool = true,
        value: T,

        fn create(allocator: std.mem.Allocator) !*Self {
            const self = try allocator.create(Self);
            errdefer allocator.destroy(self);
            self.* = .{
                .allocator = allocator,
                .ref_count = .init(1),
                .value = try Ops.init(allocator),
            };
            return self;
        }

        fn retain(self: *Self) void {
            _ = self.ref_count.fetchAdd(1, .monotonic);
        }

        fn release(self: *Self) void {
            if (self.ref_count.fetchSub(1, .acq_rel) == 1) {
                self.allocator.destroy(self);
            }
        }
    };
}

pub fn Subscription(comptime T: type, comptime Ops: type) type {
    return struct {
        const Self = @This();
        const State = SharedState(T, Ops);

        state: ?*State,

        /// Returns an owned snapshot without consuming pending updates.
        pub fn read(self: *const Self, allocator: std.mem.Allocator) !T {
            const state = self.state orelse return error.Cancelled;
            state.mutex.lock();
            defer state.mutex.unlock();
            if (!state.alive) return error.Cancelled;
            return Ops.clone(&state.value, allocator);
        }

        /// Atomically takes all accumulated updates and installs a fresh value.
        /// If initialization fails, the old value remains pending and unchanged.
        pub fn consume(self: *const Self) !T {
            const state = self.state orelse return error.Cancelled;
            state.mutex.lock();
            defer state.mutex.unlock();
            if (!state.alive) return error.Cancelled;

            var replacement = try Ops.init(state.allocator);
            const result = state.value;
            state.value = replacement;
            replacement = undefined;
            return result;
        }

        /// Cancels this subscriber. It is idempotent; its pending value is
        /// destroyed immediately, while a publisher may retain a stale control
        /// block until its next publish or deinit.
        pub fn deinit(self: *Self) void {
            const state = self.state orelse return;
            self.state = null;
            state.mutex.lock();
            if (state.alive) {
                state.alive = false;
                Ops.deinit(&state.value, state.allocator);
            }
            state.mutex.unlock();
            state.release();
        }
    };
}

pub fn Topic(comptime T: type, comptime Ops: type) type {
    return struct {
        const Self = @This();
        const State = SharedState(T, Ops);
        const Sub = Subscription(T, Ops);
        const Pending = struct { state: *State, value: T };

        allocator: std.mem.Allocator,
        mutex: Mutex = .{},
        subscribers: std.ArrayList(*State) = .empty,

        pub fn init(allocator: std.mem.Allocator) Self {
            return .{ .allocator = allocator };
        }

        /// The returned subscription owns its pending value. The Topic stores a
        /// weak-equivalent control-block reference and may be destroyed first.
        pub fn subscribe(self: *Self) !Sub {
            const state = try State.create(self.allocator);
            errdefer {
                Ops.deinit(&state.value, state.allocator);
                state.release();
            }

            self.mutex.lock();
            defer self.mutex.unlock();
            try self.subscribers.append(self.allocator, state);
            state.retain();
            return .{ .state = state };
        }

        /// Accumulates `update` into every live subscriber. All live subscriber
        /// mutexes remain held while candidates are built, making publication
        /// atomic across the topic: an error commits none of them.
        pub fn publish(self: *Self, update: *const T) !void {
            self.mutex.lock();
            defer self.mutex.unlock();

            var pending: std.ArrayList(Pending) = .empty;
            defer pending.deinit(self.allocator);
            errdefer {
                for (pending.items) |*item| {
                    Ops.deinit(&item.value, item.state.allocator);
                    item.state.mutex.unlock();
                }
            }

            for (self.subscribers.items) |state| {
                state.mutex.lock();
                if (!state.alive) {
                    state.mutex.unlock();
                    continue;
                }
                const combined = Ops.combine(&state.value, update, state.allocator) catch |err| {
                    state.mutex.unlock();
                    return err;
                };
                pending.append(self.allocator, .{ .state = state, .value = combined }) catch |err| {
                    var owned = combined;
                    Ops.deinit(&owned, state.allocator);
                    state.mutex.unlock();
                    return err;
                };
            }

            for (pending.items) |*item| {
                var old = item.state.value;
                item.state.value = item.value;
                item.value = undefined;
                Ops.deinit(&old, item.state.allocator);
                item.state.mutex.unlock();
            }

            var index: usize = 0;
            while (index < self.subscribers.items.len) {
                const state = self.subscribers.items[index];
                state.mutex.lock();
                const alive = state.alive;
                state.mutex.unlock();
                if (alive) {
                    index += 1;
                } else {
                    _ = self.subscribers.orderedRemove(index);
                    state.release();
                }
            }
        }

        pub fn deinit(self: *Self) void {
            self.mutex.lock();
            for (self.subscribers.items) |state| state.release();
            self.subscribers.deinit(self.allocator);
            self.subscribers = .empty;
            self.mutex.unlock();
            self.* = undefined;
        }
    };
}
