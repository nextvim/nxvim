const std = @import("std");
const text = @import("text");

pub fn main(init: std.process.Init) !void {
    var stdin_buffer: [4096]u8 = undefined;
    var stdout_buffer: [4096]u8 = undefined;
    var stdin_reader = std.Io.File.stdin().reader(init.io, &stdin_buffer);
    var stdout_writer = std.Io.File.stdout().writer(init.io, &stdout_buffer);
    const input = &stdin_reader.interface;
    const output = &stdout_writer.interface;

    while (try input.takeDelimiter('\n')) |line| {
        const command = try text.trace.parseLine(line) orelse continue;
        try text.trace.execute(command, output);
    }
    try output.flush();
}
