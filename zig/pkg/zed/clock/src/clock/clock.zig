const std = @import("std");

pub const Seq = u32;

pub const ReplicaId = enum(u16) {
    local = 0,
    remote_server = 1,
    agent = 2,
    local_branch = 3,
    _,

    pub const LOCAL: ReplicaId = .local;
    pub const REMOTE_SERVER: ReplicaId = .remote_server;
    pub const AGENT: ReplicaId = .agent;
    pub const LOCAL_BRANCH: ReplicaId = .local_branch;
    pub const FIRST_COLLAB_ID: ReplicaId = @enumFromInt(8);

    pub fn new(id: u16) ReplicaId {
        return @enumFromInt(id);
    }

    pub fn asU16(self: ReplicaId) u16 {
        return @intFromEnum(self);
    }

    pub fn isRemote(self: ReplicaId) bool {
        return self == REMOTE_SERVER or self.asU16() >= FIRST_COLLAB_ID.asU16();
    }

    pub fn order(self: ReplicaId, other: ReplicaId) std.math.Order {
        return std.math.order(self.asU16(), other.asU16());
    }
};

pub const Lamport = struct {
    value: Seq,
    replica_id: ReplicaId,

    pub const MIN: Lamport = .{ .value = std.math.minInt(Seq), .replica_id = ReplicaId.new(std.math.minInt(u16)) };
    pub const MAX: Lamport = .{ .value = std.math.maxInt(Seq), .replica_id = ReplicaId.new(std.math.maxInt(u16)) };

    pub fn new(replica_id: ReplicaId) Lamport {
        return .{ .value = 1, .replica_id = replica_id };
    }

    pub fn asU64(self: Lamport) u64 {
        return (@as(u64, self.value) << 32) | @as(u64, self.replica_id.asU16());
    }

    pub fn tick(self: *Lamport) Lamport {
        const timestamp = self.*;
        self.value += 1;
        return timestamp;
    }

    pub fn observe(self: *Lamport, timestamp: Lamport) void {
        self.value = @max(self.value, timestamp.value) + 1;
    }

    pub fn order(self: Lamport, other: Lamport) std.math.Order {
        const value_order = std.math.order(self.value, other.value);
        return if (value_order == .eq)
            self.replica_id.order(other.replica_id)
        else
            value_order;
    }

    pub fn eql(self: Lamport, other: Lamport) bool {
        return self.value == other.value and self.replica_id == other.replica_id;
    }
};

pub const Global = struct {
    allocator: std.mem.Allocator,
    values: std.ArrayList(Seq),

    pub fn init(allocator: std.mem.Allocator) Global {
        return .{ .allocator = allocator, .values = .empty };
    }

    pub fn deinit(self: *Global) void {
        self.values.deinit(self.allocator);
        self.* = undefined;
    }

    pub fn clone(self: *const Global, allocator: std.mem.Allocator) !Global {
        var result = Global.init(allocator);
        errdefer result.deinit();
        try result.values.appendSlice(allocator, self.values.items);
        return result;
    }

    pub fn assign(self: *Global, other: *const Global) !void {
        const replacement = try other.clone(self.allocator);
        self.deinit();
        self.* = replacement;
    }

    pub fn get(self: *const Global, replica_id: ReplicaId) Seq {
        const index = @as(usize, replica_id.asU16());
        return if (index < self.values.items.len) self.values.items[index] else 0;
    }

    pub fn observe(self: *Global, timestamp: Lamport) !void {
        std.debug.assert(timestamp.replica_id != Lamport.MAX.replica_id);
        if (timestamp.value == 0) return;
        const index = @as(usize, timestamp.replica_id.asU16());
        try self.ensureLen(index + 1);
        self.values.items[index] = @max(self.values.items[index], timestamp.value);
    }

    pub fn join(self: *Global, other: *const Global) !void {
        try self.ensureLen(other.values.items.len);
        for (self.values.items[0..other.values.items.len], other.values.items) |*left, right| {
            left.* = @max(left.*, right);
        }
    }

    pub fn meet(self: *Global, other: *const Global) !void {
        const original_len = self.values.items.len;
        try self.ensureLen(other.values.items.len);
        var new_len: usize = 0;
        for (self.values.items[0..other.values.items.len], other.values.items, 0..) |*left, right, index| {
            if (left.* == 0) {
                left.* = right;
            } else if (right != 0) {
                left.* = @min(left.*, right);
            }
            if (left.* != 0) new_len = index + 1;
        }
        if (other.values.items.len >= original_len) self.values.shrinkRetainingCapacity(new_len);
    }

    pub fn observed(self: *const Global, timestamp: Lamport) bool {
        return self.get(timestamp.replica_id) >= timestamp.value;
    }

    pub fn observedAny(self: *const Global, other: *const Global) bool {
        const count = @min(self.values.items.len, other.values.items.len);
        for (self.values.items[0..count], other.values.items[0..count]) |left, right| {
            if (right > 0 and left >= right) return true;
        }
        return false;
    }

    pub fn observedAll(self: *const Global, other: *const Global) bool {
        if (self.values.items.len < other.values.items.len) return false;
        for (self.values.items[0..other.values.items.len], other.values.items) |left, right| {
            if (left < right) return false;
        }
        return true;
    }

    pub fn changedSince(self: *const Global, other: *const Global) bool {
        if (self.values.items.len > other.values.items.len) return true;
        const count = @min(self.values.items.len, other.values.items.len);
        for (self.values.items[0..count], other.values.items[0..count]) |left, right| {
            if (left > right) return true;
        }
        return false;
    }

    pub fn mostRecent(self: *const Global) ?Lamport {
        var result: ?Lamport = null;
        var entries = self.iterator();
        while (entries.next()) |timestamp| {
            if (result == null or timestamp.value > result.?.value) result = timestamp;
        }
        return result;
    }

    pub fn iterator(self: *const Global) Iterator {
        return .{ .values = self.values.items };
    }

    pub fn eql(self: *const Global, other: *const Global) bool {
        return std.mem.eql(Seq, self.values.items, other.values.items);
    }

    pub const Iterator = struct {
        values: []const Seq,
        index: usize = 0,

        pub fn next(self: *Iterator) ?Lamport {
            if (self.index >= self.values.len) return null;
            defer self.index += 1;
            return .{ .replica_id = ReplicaId.new(@intCast(self.index)), .value = self.values[self.index] };
        }
    };

    fn ensureLen(self: *Global, len: usize) !void {
        if (len <= self.values.items.len) return;
        const old_len = self.values.items.len;
        try self.values.resize(self.allocator, len);
        @memset(self.values.items[old_len..], 0);
    }
};
