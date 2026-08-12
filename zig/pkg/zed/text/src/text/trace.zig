const std = @import("std");

pub const version: u16 = 1;
pub const v2_version: u16 = 2;
pub const initial_state = "state version=1 text=- version-vector=- operations=0 deferred=0 history=0";

/// The version-one command set. This API intentionally remains unchanged.
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

pub const V2ParseError = error{
    MalformedTrace,
    UnsupportedVersion,
    InvalidUtf8,
    InvalidNumber,
    NumberOverflow,
    InvalidHex,
    InvalidLineEnding,
    UnknownCommand,
    MissingField,
    ExtraField,
};

pub const Bias = enum { left, right };
pub const LineEnding = enum { lf, crlf };
pub const EmitTarget = union(enum) { replica: u16, all };

/// Tokens represented by slices borrow from the line passed to `V2Parser.parseLine`.
pub const V2Command = union(enum) {
    trace: u16,
    replica: struct { replica: u16, buffer: u64, bytes: []const u8 },
    edit: struct { replica: u16, start: u64, end: u64, bytes: []const u8 },
    capture: struct { replica: u16, operation_name: []const u8 },
    deliver: struct { operation_name: []const u8, replica: u16 },
    undo: u16,
    redo: u16,
    anchor: struct { replica: u16, anchor_name: []const u8, offset: u64, bias: Bias },
    resolve: struct { replica: u16, anchor_name: []const u8 },
    mark: struct { replica: u16, version_name: []const u8 },
    patch: struct { replica: u16, version_name: []const u8 },
    line_ending: struct { replica: u16, ending: LineEnding },
    emit: EmitTarget,
};

/// Stateful parser for a complete v2 stream. Pass each physical line including its
/// LF/CRLF terminator; the final line may be unterminated. Call `finish` at EOF.
pub const V2Parser = struct {
    expected_line_ending: ?LineEnding = null,
    saw_input: bool = false,
    saw_header: bool = false,

    pub fn init() V2Parser {
        return .{};
    }

    pub fn parseLine(self: *V2Parser, raw: []const u8) V2ParseError!?V2Command {
        var line = raw;
        if (std.mem.indexOfScalar(u8, line, 0) != null) return error.MalformedTrace;
        if (!std.unicode.utf8ValidateSlice(line)) return error.InvalidUtf8;

        if (!self.saw_input) {
            self.saw_input = true;
            if (std.mem.startsWith(u8, line, "\xef\xbb\xbf")) line = line[3..];
        }
        if (std.mem.indexOf(u8, line, "\xef\xbb\xbf") != null) return error.MalformedTrace;

        var ending: ?LineEnding = null;
        if (std.mem.endsWith(u8, line, "\r\n")) {
            ending = .crlf;
            line = line[0 .. line.len - 2];
        } else if (std.mem.endsWith(u8, line, "\n")) {
            ending = .lf;
            line = line[0 .. line.len - 1];
        }
        if (std.mem.indexOfScalar(u8, line, '\r') != null or std.mem.indexOfScalar(u8, line, '\n') != null)
            return error.InvalidLineEnding;
        if (ending) |actual| {
            if (self.expected_line_ending) |expected| {
                if (actual != expected) return error.InvalidLineEnding;
            } else self.expected_line_ending = actual;
        }

        const command = try parseV2Content(line);
        if (!self.saw_header) {
            if (command == null) return null;
            switch (command.?) {
                .trace => |found| {
                    if (found != v2_version) return error.UnsupportedVersion;
                    self.saw_header = true;
                    return command;
                },
                else => return error.UnsupportedVersion,
            }
        }
        if (command) |parsed| switch (parsed) {
            .trace => return error.MalformedTrace,
            else => {},
        };
        return command;
    }

    pub fn finish(self: *const V2Parser) V2ParseError!void {
        if (!self.saw_header) return error.MissingField;
    }
};

/// Parses one v2 line's content without stream/header state. Newline bytes, CR,
/// BOM, NUL, and invalid UTF-8 are still rejected.
pub fn parseV2Line(line: []const u8) V2ParseError!?V2Command {
    if (std.mem.indexOfScalar(u8, line, 0) != null) return error.MalformedTrace;
    if (!std.unicode.utf8ValidateSlice(line)) return error.InvalidUtf8;
    if (std.mem.indexOfScalar(u8, line, '\r') != null or std.mem.indexOfScalar(u8, line, '\n') != null)
        return error.InvalidLineEnding;
    if (std.mem.indexOf(u8, line, "\xef\xbb\xbf") != null) return error.MalformedTrace;
    return parseV2Content(line);
}

fn parseV2Content(line: []const u8) V2ParseError!?V2Command {
    var fields = std.mem.tokenizeAny(u8, line, " \t");
    const operation = fields.next() orelse return null;
    if (operation[0] == '#') return null;

    if (std.mem.eql(u8, operation, "trace")) {
        const found = try parseNumber(u16, try next(&fields));
        try noMore(&fields);
        return .{ .trace = found };
    } else if (std.mem.eql(u8, operation, "replica")) {
        const replica = try parseReplica(try next(&fields));
        const buffer = try parseNumber(u64, try next(&fields));
        if (buffer == 0) return error.InvalidNumber;
        const bytes = try parseBytes(try next(&fields));
        try noMore(&fields);
        return .{ .replica = .{ .replica = replica, .buffer = buffer, .bytes = bytes } };
    } else if (std.mem.eql(u8, operation, "edit")) {
        const replica = try parseReplica(try next(&fields));
        const start = try parseNumber(u64, try next(&fields));
        const end = try parseNumber(u64, try next(&fields));
        const bytes = try parseBytes(try next(&fields));
        try noMore(&fields);
        return .{ .edit = .{ .replica = replica, .start = start, .end = end, .bytes = bytes } };
    } else if (std.mem.eql(u8, operation, "capture")) {
        const replica = try parseReplica(try next(&fields));
        const name = try parseName(try next(&fields));
        try noMore(&fields);
        return .{ .capture = .{ .replica = replica, .operation_name = name } };
    } else if (std.mem.eql(u8, operation, "deliver")) {
        const name = try parseName(try next(&fields));
        const replica = try parseReplica(try next(&fields));
        try noMore(&fields);
        return .{ .deliver = .{ .operation_name = name, .replica = replica } };
    } else if (std.mem.eql(u8, operation, "undo") or std.mem.eql(u8, operation, "redo")) {
        const replica = try parseReplica(try next(&fields));
        try noMore(&fields);
        return if (operation[0] == 'u') .{ .undo = replica } else .{ .redo = replica };
    } else if (std.mem.eql(u8, operation, "anchor")) {
        const replica = try parseReplica(try next(&fields));
        const name = try parseName(try next(&fields));
        const offset = try parseNumber(u64, try next(&fields));
        const bias_token = try next(&fields);
        const bias: Bias = if (std.mem.eql(u8, bias_token, "left")) .left else if (std.mem.eql(u8, bias_token, "right")) .right else return error.MalformedTrace;
        try noMore(&fields);
        return .{ .anchor = .{ .replica = replica, .anchor_name = name, .offset = offset, .bias = bias } };
    } else if (std.mem.eql(u8, operation, "resolve")) {
        const replica = try parseReplica(try next(&fields));
        const name = try parseName(try next(&fields));
        try noMore(&fields);
        return .{ .resolve = .{ .replica = replica, .anchor_name = name } };
    } else if (std.mem.eql(u8, operation, "mark") or std.mem.eql(u8, operation, "patch")) {
        const replica = try parseReplica(try next(&fields));
        const name = try parseName(try next(&fields));
        try noMore(&fields);
        return if (operation[0] == 'm')
            .{ .mark = .{ .replica = replica, .version_name = name } }
        else
            .{ .patch = .{ .replica = replica, .version_name = name } };
    } else if (std.mem.eql(u8, operation, "line-ending")) {
        const replica = try parseReplica(try next(&fields));
        const token = try next(&fields);
        const value: LineEnding = if (std.mem.eql(u8, token, "lf")) .lf else if (std.mem.eql(u8, token, "crlf")) .crlf else return error.MalformedTrace;
        try noMore(&fields);
        return .{ .line_ending = .{ .replica = replica, .ending = value } };
    } else if (std.mem.eql(u8, operation, "emit")) {
        const token = try next(&fields);
        const target: EmitTarget = if (std.mem.eql(u8, token, "all")) .all else .{ .replica = try parseReplica(token) };
        try noMore(&fields);
        return .{ .emit = target };
    }
    return error.UnknownCommand;
}

fn next(fields: anytype) V2ParseError![]const u8 {
    return fields.next() orelse error.MissingField;
}

fn noMore(fields: anytype) V2ParseError!void {
    if (fields.next() != null) return error.ExtraField;
}

fn parseNumber(comptime T: type, token: []const u8) V2ParseError!T {
    if (token.len == 0) return error.InvalidNumber;
    if (token.len > 1 and token[0] == '0') return error.InvalidNumber;
    for (token) |byte| if (byte < '0' or byte > '9') return error.InvalidNumber;
    return std.fmt.parseUnsigned(T, token, 10) catch |err| switch (err) {
        error.Overflow => error.NumberOverflow,
        else => error.InvalidNumber,
    };
}

fn parseReplica(token: []const u8) V2ParseError!u16 {
    const replica = try parseNumber(u16, token);
    if (replica < 8 or replica == std.math.maxInt(u16)) return error.InvalidNumber;
    return replica;
}

fn parseName(token: []const u8) V2ParseError![]const u8 {
    if (token.len == 0 or !isNameStart(token[0])) return error.MalformedTrace;
    for (token[1..]) |byte| if (!isNameContinue(byte)) return error.MalformedTrace;
    return token;
}

fn isNameStart(byte: u8) bool {
    return (byte >= 'A' and byte <= 'Z') or (byte >= 'a' and byte <= 'z') or byte == '_';
}

fn isNameContinue(byte: u8) bool {
    return isNameStart(byte) or (byte >= '0' and byte <= '9') or byte == '-';
}

fn parseBytes(token: []const u8) V2ParseError![]const u8 {
    if (std.mem.eql(u8, token, "-")) return token;
    if (token.len == 0 or token.len % 2 != 0) return error.InvalidHex;
    for (token) |byte| if (!isLowerHex(byte)) return error.InvalidHex;
    try validateHexUtf8(token);
    return token;
}

fn isLowerHex(byte: u8) bool {
    return (byte >= '0' and byte <= '9') or (byte >= 'a' and byte <= 'f');
}

fn hexByte(token: []const u8, byte_index: usize) u8 {
    const index = byte_index * 2;
    return (hexNibble(token[index]) << 4) | hexNibble(token[index + 1]);
}

fn hexNibble(byte: u8) u8 {
    return if (byte <= '9') byte - '0' else byte - 'a' + 10;
}

fn validateHexUtf8(token: []const u8) V2ParseError!void {
    const byte_len = token.len / 2;
    var index: usize = 0;
    while (index < byte_len) {
        var encoded: [4]u8 = undefined;
        encoded[0] = hexByte(token, index);
        const sequence_len = std.unicode.utf8ByteSequenceLength(encoded[0]) catch return error.InvalidUtf8;
        if (index + sequence_len > byte_len) return error.InvalidUtf8;
        for (1..sequence_len) |offset| encoded[offset] = hexByte(token, index + offset);
        _ = std.unicode.utf8Decode(encoded[0..sequence_len]) catch return error.InvalidUtf8;
        index += sequence_len;
    }
}
