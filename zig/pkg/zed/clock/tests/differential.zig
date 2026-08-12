const std = @import("std");
const clock = @import("clock");

pub fn main(init: std.process.Init) !void {
    const allocator = init.gpa;
    var stdin_buffer: [4096]u8 = undefined;
    var stdout_buffer: [4096]u8 = undefined;
    var stdin_reader = std.Io.File.stdin().reader(init.io, &stdin_buffer);
    var stdout_writer = std.Io.File.stdout().writer(init.io, &stdout_buffer);
    const input = &stdin_reader.interface;
    const output = &stdout_writer.interface;

    while (try input.takeDelimiter('\n')) |raw_line| {
        const line = std.mem.trim(u8, raw_line, " \t\r");
        if (line.len == 0 or line[0] == '#') continue;
        var fields = std.mem.tokenizeAny(u8, line, " \t");
        const command = fields.next() orelse continue;
        if (std.mem.eql(u8, command, "replica")) {
            const id = clock.ReplicaId.new(try parse(u16, try next(&fields)));
            try noMore(&fields);
            try output.print("replica {d} {d}\n", .{ id.asU16(), @intFromBool(id.isRemote()) });
        } else if (std.mem.eql(u8, command, "lamport")) {
            const left = clock.Lamport{ .replica_id = .new(try parse(u16, try next(&fields))), .value = try parse(u32, try next(&fields)) };
            const right = clock.Lamport{ .replica_id = .new(try parse(u16, try next(&fields))), .value = try parse(u32, try next(&fields)) };
            try noMore(&fields);
            try output.print("lamport {d} {d}\n", .{ left.asU64(), orderInt(left.order(right)) });
        } else if (std.mem.eql(u8, command, "global")) {
            var value = try parseGlobal(allocator, try next(&fields));
            defer value.deinit();
            try noMore(&fields);
            try writeGlobal(output, &value);
        } else if (std.mem.eql(u8, command, "join") or std.mem.eql(u8, command, "meet")) {
            var left = try parseGlobal(allocator, try next(&fields));
            defer left.deinit();
            var right = try parseGlobal(allocator, try next(&fields));
            defer right.deinit();
            try noMore(&fields);
            if (std.mem.eql(u8, command, "join")) try left.join(&right) else try left.meet(&right);
            try writeGlobal(output, &left);
        } else if (std.mem.eql(u8, command, "relations")) {
            var left = try parseGlobal(allocator, try next(&fields));
            defer left.deinit();
            var right = try parseGlobal(allocator, try next(&fields));
            defer right.deinit();
            try noMore(&fields);
            try output.print("relations {d} {d} {d}", .{ @intFromBool(left.observedAny(&right)), @intFromBool(left.observedAll(&right)), @intFromBool(left.changedSince(&right)) });
            if (left.mostRecent()) |recent| try output.print(" {d}:{d}\n", .{ recent.replica_id.asU16(), recent.value }) else try output.writeAll(" -\n");
        } else return error.MalformedTrace;
    }
    try output.flush();
}

fn parseGlobal(allocator: std.mem.Allocator, encoded: []const u8) !clock.Global {
    var result = clock.Global.init(allocator);
    errdefer result.deinit();
    if (std.mem.eql(u8, encoded, "-")) return result;
    var entries = std.mem.splitScalar(u8, encoded, ',');
    while (entries.next()) |entry| {
        var pair = std.mem.splitScalar(u8, entry, ':');
        const id = try parse(u16, pair.next() orelse return error.MalformedTrace);
        const value = try parse(u32, pair.next() orelse return error.MalformedTrace);
        if (pair.next() != null) return error.MalformedTrace;
        try result.observe(.{ .replica_id = .new(id), .value = value });
    }
    return result;
}

fn writeGlobal(output: *std.Io.Writer, value: *const clock.Global) !void {
    try output.writeAll("global ");
    var iterator = value.iterator();
    var first = true;
    while (iterator.next()) |timestamp| {
        if (!first) try output.writeByte(',');
        first = false;
        try output.print("{d}", .{timestamp.value});
    }
    if (first) try output.writeByte('-');
    try output.writeByte('\n');
}

fn next(fields: anytype) ![]const u8 {
    return fields.next() orelse error.MalformedTrace;
}
fn noMore(fields: anytype) !void {
    if (fields.next() != null) return error.MalformedTrace;
}
fn parse(comptime T: type, value: []const u8) !T {
    return std.fmt.parseInt(T, value, 10) catch error.MalformedTrace;
}
fn orderInt(order: std.math.Order) i8 {
    return switch (order) {
        .lt => -1,
        .eq => 0,
        .gt => 1,
    };
}
