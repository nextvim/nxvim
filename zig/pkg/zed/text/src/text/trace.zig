const std = @import("std");

pub const version: u16 = 1;
pub const initial_state = "state version=1 text=- version-vector=- operations=0 deferred=0 history=0";

pub const Command = enum {
    emit,
};

pub const ParseError = error{MalformedTrace};

pub fn parseLine(raw_line: []const u8) ParseError!?Command {
    const line = std.mem.trim(u8, raw_line, " \t\r\n");
    if (line.len == 0 or line[0] == '#') return null;

    var fields = std.mem.tokenizeAny(u8, line, " \t");
    const operation = fields.next() orelse return null;
    if (!std.mem.eql(u8, operation, "emit")) return error.MalformedTrace;
    if (fields.next() != null) return error.MalformedTrace;
    return .emit;
}

pub fn execute(command: Command, output: *std.Io.Writer) !void {
    switch (command) {
        .emit => try output.print("{s}\n", .{initial_state}),
    }
}
