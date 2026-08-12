const std = @import("std");
const sum_tree = @import("sum_tree");

const Ops = struct {
    pub const Summary = usize;
    pub const Context = void;
    pub fn summary(_: *const u32, _: void) usize {
        return 1;
    }
    pub fn zero(_: void) usize {
        return 0;
    }
    pub fn addSummary(a: *usize, b: *const usize, _: void) void {
        a.* += b.*;
    }
    pub fn cloneItem(v: *const u32, _: std.mem.Allocator) !u32 {
        return v.*;
    }
    pub fn deinitItem(_: *u32, _: std.mem.Allocator) void {}
    pub fn cloneSummary(v: *const usize, _: std.mem.Allocator) !usize {
        return v.*;
    }
    pub fn deinitSummary(_: *usize, _: std.mem.Allocator) void {}
    pub fn eqlSummary(a: *const usize, b: *const usize) bool {
        return a.* == b.*;
    }
};
const Dimension = struct {
    pub const Value = usize;
    pub fn zero(_: void) usize {
        return 0;
    }
    pub fn addSummary(a: *usize, b: *const usize, _: void) void {
        a.* += b.*;
    }
};
const Target = struct {
    value: usize,
    pub fn compare(self: Target, value: *const usize, _: void) std.math.Order {
        return std.math.order(self.value, value.*);
    }
};
const Tree = sum_tree.SumTree(u32, Ops, 6);

pub fn main(init: std.process.Init) !void {
    const allocator = init.gpa;
    const count = 20_000;
    const values = try allocator.alloc(u32, count);
    defer allocator.free(values);
    for (values, 0..) |*v, i| v.* = @intCast(i);
    var mark = std.Io.Clock.Timestamp.now(init.io, .awake);
    var tree = try Tree.fromSlice(allocator, values, {});
    defer tree.deinit();
    const build_ns = mark.untilNow(init.io).raw.toNanoseconds();
    mark = std.Io.Clock.Timestamp.now(init.io, .awake);
    var parallel = try Tree.fromParallel(allocator, values, {});
    defer parallel.deinit();
    const parallel_ns = mark.untilNow(init.io).raw.toNanoseconds();
    mark = std.Io.Clock.Timestamp.now(init.io, .awake);
    var iterator = tree.iterator();
    var checksum: u64 = 0;
    while (iterator.next()) |v| checksum += v.*;
    const iter_ns = mark.untilNow(init.io).raw.toNanoseconds();
    mark = std.Io.Clock.Timestamp.now(init.io, .awake);
    var cursor = tree.cursor(Dimension, {});
    var i: usize = 0;
    while (i < count) : (i += 97) _ = cursor.seek(Target, .{ .value = i }, .right);
    const seek_ns = mark.untilNow(init.io).raw.toNanoseconds();
    mark = std.Io.Clock.Timestamp.now(init.io, .awake);
    var snapshot = tree.clone();
    defer snapshot.deinit();
    try tree.push(count, {});
    const mutation_ns = mark.untilNow(init.io).raw.toNanoseconds();
    std.debug.print("count={d} build_ns={d} parallel_ns={d} iter_ns={d} seek_ns={d} mutation_ns={d} checksum={d}\n", .{ count, build_ns, parallel_ns, iter_ns, seek_ns, mutation_ns, checksum });
}
