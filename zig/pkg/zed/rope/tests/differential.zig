const std = @import("std");
const rope = @import("rope");

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
        const operation = fields.next() orelse continue;
        if (std.mem.eql(u8, operation, "emit")) {
            try noMore(&fields);
            try output.writeAll("state phase=4 unicode-segmentation=1.13.3 chunk-max=128\n");
        } else if (std.mem.eql(u8, operation, "grapheme")) {
            const encoded = try next(&fields);
            const offset = try parse(usize, try next(&fields));
            try noMore(&fields);
            const text = try decodeHex(allocator, encoded);
            defer allocator.free(text);
            const boundary = try rope.grapheme.isBoundary(text, offset);
            const previous = try rope.grapheme.previousBoundary(text, offset);
            const following = try rope.grapheme.nextBoundary(text, offset);
            try output.print("grapheme {d} {d} {d} {d}\n", .{ offset, @intFromBool(boundary), previous, following });
        } else if (std.mem.eql(u8, operation, "chunk")) {
            const text = try decodeHex(allocator, try next(&fields));
            defer allocator.free(text);
            try noMore(&fields);
            var chunk = try rope.Chunk.init(text);
            const slice = chunk.asSlice();
            const summary = slice.textSummary();
            try output.print("chunk {d} {d} {d} {d} {d} {d} {d} {d} {d} {d} {d} {d} {d} {d}\n", .{
                summary.len,              summary.chars,            summary.len_utf16.value,     summary.lines.row,   summary.lines.column,
                summary.first_line_chars, summary.last_line_chars,  summary.last_line_len_utf16, summary.longest_row, summary.longest_row_chars,
                slice.chars_bitmap,       slice.chars_utf16_bitmap, slice.newlines_bitmap,       slice.tabs_bitmap,
            });
        } else if (std.mem.eql(u8, operation, "chunk_byte")) {
            const encoded = try next(&fields);
            const offset = try parse(usize, try next(&fields));
            try noMore(&fields);
            const text = try decodeHex(allocator, encoded);
            defer allocator.free(text);
            var chunk = try rope.Chunk.init(text);
            const slice = chunk.asSlice();
            const point = try slice.offsetToPoint(offset);
            const utf16 = try slice.offsetToOffsetUtf16(offset);
            const point_utf16 = try slice.offsetToPointUtf16(offset);
            try output.print("chunk_byte {d} {d} {d} {d} {d} {d}\n", .{ offset, point.row, point.column, utf16.value, point_utf16.row, point_utf16.column });
        } else if (std.mem.eql(u8, operation, "chunk_point")) {
            const encoded = try next(&fields);
            const row = try parse(u32, try next(&fields));
            const column = try parse(u32, try next(&fields));
            try noMore(&fields);
            const text = try decodeHex(allocator, encoded);
            defer allocator.free(text);
            var chunk = try rope.Chunk.init(text);
            const slice = chunk.asSlice();
            const point = rope.Point.new(row, column);
            const offset = try slice.pointToOffset(point);
            const utf16 = try slice.pointToPointUtf16(point);
            try output.print("chunk_point {d} {d} {d} {d} {d}\n", .{ row, column, offset, utf16.row, utf16.column });
        } else if (std.mem.eql(u8, operation, "chunk_utf16")) {
            const encoded = try next(&fields);
            const offset = try parse(usize, try next(&fields));
            try noMore(&fields);
            const text = try decodeHex(allocator, encoded);
            defer allocator.free(text);
            var chunk = try rope.Chunk.init(text);
            const byte = try chunk.asSlice().offsetUtf16ToOffset(rope.OffsetUtf16.new(offset));
            try output.print("chunk_utf16 {d} {d}\n", .{ offset, byte });
        } else if (std.mem.eql(u8, operation, "chunk_point_utf16")) {
            const encoded = try next(&fields);
            const row = try parse(u32, try next(&fields));
            const column = try parse(u32, try next(&fields));
            const clip_value = try next(&fields);
            try noMore(&fields);
            const clip = if (std.mem.eql(u8, clip_value, "0")) false else if (std.mem.eql(u8, clip_value, "1")) true else return error.MalformedTrace;
            const text = try decodeHex(allocator, encoded);
            defer allocator.free(text);
            var chunk = try rope.Chunk.init(text);
            const byte = try chunk.asSlice().pointUtf16ToOffset(rope.PointUtf16.new(row, column), clip);
            try output.print("chunk_point_utf16 {d} {d} {d} {d}\n", .{ row, column, @intFromBool(clip), byte });
        } else if (std.mem.eql(u8, operation, "chunk_clip")) {
            const encoded = try next(&fields);
            const row = try parse(u32, try next(&fields));
            const column = try parse(u32, try next(&fields));
            const bias_text = try next(&fields);
            try noMore(&fields);
            const bias: rope.sum_tree.Bias = if (std.mem.eql(u8, bias_text, "left")) .left else if (std.mem.eql(u8, bias_text, "right")) .right else return error.MalformedTrace;
            const text = try decodeHex(allocator, encoded);
            defer allocator.free(text);
            var chunk = try rope.Chunk.init(text);
            const point = chunk.asSlice().clipPoint(rope.Point.new(row, column), bias);
            try output.print("chunk_clip {d} {d} {s} {d} {d}\n", .{ row, column, bias_text, point.row, point.column });
        } else if (std.mem.eql(u8, operation, "rope")) {
            const text = try decodeHex(allocator, try next(&fields));
            defer allocator.free(text);
            try noMore(&fields);
            var value = try rope.Rope.initText(allocator, text);
            defer value.deinit();
            const summary = value.summary();
            const materialized = try value.toOwnedSlice(allocator);
            defer allocator.free(materialized);
            try output.print("rope {d} {d} {d} {d} {d} {d} {d} {d} {d} {d} ", .{ value.len(), summary.chars, summary.len_utf16.value, summary.lines.row, summary.lines.column, summary.first_line_chars, summary.last_line_chars, summary.last_line_len_utf16, summary.longest_row, summary.longest_row_chars });
            try writeHex(output, materialized);
            try output.writeByte('\n');
        } else if (std.mem.eql(u8, operation, "rope_byte")) {
            const encoded = try next(&fields);
            const offset = try parse(usize, try next(&fields));
            try noMore(&fields);
            const text = try decodeHex(allocator, encoded);
            defer allocator.free(text);
            var value = try rope.Rope.initText(allocator, text);
            defer value.deinit();
            const point = value.offsetToPoint(offset);
            const point16 = value.offsetToPointUtf16(offset);
            try output.print("rope_byte {d} {d} {d} {d} {d} {d} {d}\n", .{ offset, @intFromBool(value.isCharBoundary(offset)), value.offsetToOffsetUtf16(offset).value, point.row, point.column, point16.row, point16.column });
        } else if (std.mem.eql(u8, operation, "rope_point")) {
            const encoded = try next(&fields);
            const row = try parse(u32, try next(&fields));
            const column = try parse(u32, try next(&fields));
            try noMore(&fields);
            const text = try decodeHex(allocator, encoded);
            defer allocator.free(text);
            var value = try rope.Rope.initText(allocator, text);
            defer value.deinit();
            const point = rope.Point.new(row, column);
            const point16 = value.pointToPointUtf16(point);
            try output.print("rope_point {d} {d} {d} {d} {d}\n", .{ row, column, value.pointToOffset(point), point16.row, point16.column });
        } else if (std.mem.eql(u8, operation, "rope_clip")) {
            const encoded = try next(&fields);
            const row = try parse(u32, try next(&fields));
            const column = try parse(u32, try next(&fields));
            const bias_text = try next(&fields);
            try noMore(&fields);
            const bias: rope.sum_tree.Bias = if (std.mem.eql(u8, bias_text, "left")) .left else if (std.mem.eql(u8, bias_text, "right")) .right else return error.MalformedTrace;
            const text = try decodeHex(allocator, encoded);
            defer allocator.free(text);
            var value = try rope.Rope.initText(allocator, text);
            defer value.deinit();
            const point = value.clipPoint(.new(row, column), bias);
            try output.print("rope_clip {d} {d} {s} {d} {d}\n", .{ row, column, bias_text, point.row, point.column });
        } else return error.MalformedTrace;
    }
    try output.flush();
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

fn writeHex(output: *std.Io.Writer, bytes: []const u8) !void {
    for (bytes) |byte| try output.print("{x:0>2}", .{byte});
}

fn decodeHex(allocator: std.mem.Allocator, encoded: []const u8) ![]u8 {
    if (std.mem.eql(u8, encoded, "-")) return allocator.alloc(u8, 0);
    if (encoded.len % 2 != 0) return error.MalformedTrace;
    const result = try allocator.alloc(u8, encoded.len / 2);
    errdefer allocator.free(result);
    _ = std.fmt.hexToBytes(result, encoded) catch return error.MalformedTrace;
    return result;
}
