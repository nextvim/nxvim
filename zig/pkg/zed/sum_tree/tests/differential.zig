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
const Tree = sum_tree.SumTree(u32, Ops, 2);

const MapOps = struct {
    pub fn compareKeys(a: *const u32, b: *const u32) std.math.Order {
        return std.math.order(a.*, b.*);
    }
    pub fn cloneKey(v: *const u32, _: std.mem.Allocator) !u32 {
        return v.*;
    }
    pub fn deinitKey(_: *u32, _: std.mem.Allocator) void {}
    pub fn cloneValue(v: *const u32, _: std.mem.Allocator) !u32 {
        return v.*;
    }
    pub fn deinitValue(_: *u32, _: std.mem.Allocator) void {}
};
const Map = sum_tree.TreeMap(u32, u32, MapOps, 2);
const SetOps = struct {
    pub fn compareKeys(a: *const u32, b: *const u32) std.math.Order {
        return std.math.order(a.*, b.*);
    }
    pub fn cloneKey(v: *const u32, _: std.mem.Allocator) !u32 {
        return v.*;
    }
    pub fn deinitKey(_: *u32, _: std.mem.Allocator) void {}
};
const Set = sum_tree.TreeSet(u32, SetOps, 2);

pub fn main(init: std.process.Init) !void {
    const allocator = init.gpa;
    var tree = try Tree.init(allocator, {});
    defer tree.deinit();
    var map = try Map.init(allocator);
    defer map.deinit();
    var set = try Set.init(allocator);
    defer set.deinit();

    var stdin_buffer: [4096]u8 = undefined;
    var stdin_reader = std.Io.File.stdin().reader(init.io, &stdin_buffer);
    const input = &stdin_reader.interface;
    var stdout_buffer: [4096]u8 = undefined;
    var stdout_writer = std.Io.File.stdout().writer(init.io, &stdout_buffer);
    const output = &stdout_writer.interface;
    defer output.flush() catch {};

    while (try input.takeDelimiter('\n')) |raw| {
        const line = std.mem.trim(u8, raw, " \r\n\t");
        if (line.len == 0) continue;
        var fields = std.mem.tokenizeScalar(u8, line, ' ');
        const operation = fields.next().?;
        if (std.mem.eql(u8, operation, "push")) {
            try tree.push(try number(fields.next().?), {});
        } else if (std.mem.eql(u8, operation, "append")) {
            var values: std.ArrayList(u32) = .empty;
            defer values.deinit(allocator);
            while (fields.next()) |field| try values.append(allocator, try number(field));
            var other = try Tree.fromSlice(allocator, values.items, {});
            defer other.deinit();
            try tree.append(&other, {});
        } else if (std.mem.eql(u8, operation, "seek")) {
            const target = try number(fields.next().?);
            const bias: sum_tree.Bias = if (std.mem.eql(u8, fields.next().?, "L")) .left else .right;
            const result = tree.find(Dimension, Target, {}, .{ .value = target }, bias);
            try output.print("seek {d} {d} {d}\n", .{ result.start, result.end, if (result.item) |v| @as(i64, v.*) else -1 });
        } else if (std.mem.eql(u8, operation, "slice")) {
            const start = try number(fields.next().?);
            const end = try number(fields.next().?);
            var cursor = tree.cursor(Dimension, {});
            _ = cursor.seek(Target, .{ .value = start }, .right);
            var slice = try cursor.slice(Target, .{ .value = end }, .right);
            defer slice.deinit();
            try output.writeAll("slice ");
            try writeTree(output, &slice);
            try output.writeByte('\n');
        } else if (std.mem.eql(u8, operation, "map_put")) {
            try map.insert(try number(fields.next().?), try number(fields.next().?));
        } else if (std.mem.eql(u8, operation, "map_remove")) {
            _ = try map.remove(try number(fields.next().?));
        } else if (std.mem.eql(u8, operation, "set_add")) {
            try set.insert(try number(fields.next().?));
        } else if (std.mem.eql(u8, operation, "set_remove")) {
            _ = try set.remove(try number(fields.next().?));
        } else if (std.mem.eql(u8, operation, "emit")) {
            try output.writeAll("state ");
            try writeTree(output, &tree);
            try output.writeAll(" | ");
            var mi = map.iterator();
            var first = true;
            while (mi.next()) |entry| {
                if (!first) try output.writeByte(',');
                first = false;
                try output.print("{d}:{d}", .{ entry.key, entry.value });
            }
            try output.writeAll(" | ");
            var si = set.iterator();
            first = true;
            while (si.next()) |key| {
                if (!first) try output.writeByte(',');
                first = false;
                try output.print("{d}", .{key.*});
            }
            try output.writeByte('\n');
        } else return error.UnknownOperation;
    }
}

fn number(value: []const u8) !u32 {
    return std.fmt.parseInt(u32, value, 10);
}
fn writeTree(output: anytype, tree: *const Tree) !void {
    var it = tree.iterator();
    var first = true;
    while (it.next()) |v| {
        if (!first) try output.writeByte(',');
        first = false;
        try output.print("{d}", .{v.*});
    }
}
