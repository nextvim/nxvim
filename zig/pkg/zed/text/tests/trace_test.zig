const std = @import("std");
const text = @import("text");
const trace = text.trace;

test "version-one parser and emit behavior remain unchanged" {
    try std.testing.expectEqual(trace.Command.emit, (try trace.parseLine("emit")).?);
    try std.testing.expectEqual(@as(?trace.Command, null), try trace.parseLine("  # comment"));
    try std.testing.expectEqual(@as(?trace.Command, null), try trace.parseLine(" \t\r\n"));
    try std.testing.expectError(error.MalformedTrace, trace.parseLine("unknown"));
    try std.testing.expectError(error.MalformedTrace, trace.parseLine("emit extra"));
    try std.testing.expectEqualStrings(
        "state version=1 text=- version-vector=- operations=0 deferred=0 history=0",
        trace.initial_state,
    );
}

test "v2 parser accepts every command form and borrows fields" {
    var parser = trace.V2Parser.init();
    try std.testing.expectEqual(@as(?trace.V2Command, null), try parser.parseLine("\xef\xbb\xbf # preface\r\n"));
    try expectTag(.trace, (try parser.parseLine("trace\t2\r\n")).?);

    const replica_line = "replica 8 42 68c3a9\r\n";
    const replica = (try parser.parseLine(replica_line)).?.replica;
    try std.testing.expectEqual(@as(u16, 8), replica.replica);
    try std.testing.expectEqual(@as(u64, 42), replica.buffer);
    try std.testing.expectEqualStrings("68c3a9", replica.bytes);
    try std.testing.expect(replica.bytes.ptr == replica_line.ptr + 13);

    const edit = (try parser.parseLine("edit 8 0 2 -\r\n")).?.edit;
    try std.testing.expectEqual(@as(u64, 0), edit.start);
    try std.testing.expectEqual(@as(u64, 2), edit.end);
    try std.testing.expectEqualStrings("-", edit.bytes);

    const capture = (try parser.parseLine("capture 8 op_1-x\r\n")).?.capture;
    try std.testing.expectEqualStrings("op_1-x", capture.operation_name);
    const deliver = (try parser.parseLine("deliver op_1-x 9\r\n")).?.deliver;
    try std.testing.expectEqualStrings("op_1-x", deliver.operation_name);
    try std.testing.expectEqual(@as(u16, 9), deliver.replica);
    try std.testing.expectEqual(@as(u16, 8), (try parser.parseLine("undo 8\r\n")).?.undo);
    try std.testing.expectEqual(@as(u16, 8), (try parser.parseLine("redo 8\r\n")).?.redo);

    const anchor = (try parser.parseLine("anchor 8 cursor-A 7 left\r\n")).?.anchor;
    try std.testing.expectEqualStrings("cursor-A", anchor.anchor_name);
    try std.testing.expectEqual(trace.Bias.left, anchor.bias);
    try std.testing.expectEqual(@as(u64, 7), anchor.offset);
    try std.testing.expectEqualStrings("cursor-A", (try parser.parseLine("resolve 9 cursor-A\r\n")).?.resolve.anchor_name);
    try std.testing.expectEqualStrings("Version_1", (try parser.parseLine("mark 8 Version_1\r\n")).?.mark.version_name);
    try std.testing.expectEqualStrings("Version_1", (try parser.parseLine("patch 9 Version_1\r\n")).?.patch.version_name);
    try std.testing.expectEqual(trace.LineEnding.lf, (try parser.parseLine("line-ending 8 lf\r\n")).?.line_ending.ending);
    try std.testing.expectEqual(trace.LineEnding.crlf, (try parser.parseLine("line-ending 8 crlf\r\n")).?.line_ending.ending);
    try std.testing.expectEqual(@as(u16, 8), (try parser.parseLine("emit 8\r\n")).?.emit.replica);
    try expectTag(.all, (try parser.parseLine("emit all")).?.emit);
    try parser.finish();
}

test "v2 stream header comments blank lines BOM and endings" {
    var lf = trace.V2Parser.init();
    try std.testing.expectEqual(@as(?trace.V2Command, null), try lf.parseLine("\xef\xbb\xbf\t# comment\n"));
    try std.testing.expectEqual(@as(?trace.V2Command, null), try lf.parseLine(" \t\n"));
    _ = try lf.parseLine("trace 2\n");
    _ = try lf.parseLine("emit all");
    try lf.finish();

    var wrong_version = trace.V2Parser.init();
    try std.testing.expectError(error.UnsupportedVersion, wrong_version.parseLine("trace 1\n"));
    var missing_header = trace.V2Parser.init();
    try std.testing.expectError(error.UnsupportedVersion, missing_header.parseLine("emit all\n"));
    var empty = trace.V2Parser.init();
    _ = try empty.parseLine("# only a comment\n");
    try std.testing.expectError(error.MissingField, empty.finish());
    var duplicate = trace.V2Parser.init();
    _ = try duplicate.parseLine("trace 2\n");
    try std.testing.expectError(error.MalformedTrace, duplicate.parseLine("trace 2\n"));
}

test "v2 rejects malformed field counts commands and tokens" {
    const cases = [_]struct { expected: trace.V2ParseError, line: []const u8 }{
        .{ .expected = error.UnknownCommand, .line = "wat 8" },
        .{ .expected = error.MissingField, .line = "edit 8 0" },
        .{ .expected = error.ExtraField, .line = "undo 8 extra" },
        .{ .expected = error.InvalidNumber, .line = "edit 8 00 1 -" },
        .{ .expected = error.InvalidNumber, .line = "edit 8 +1 1 -" },
        .{ .expected = error.InvalidNumber, .line = "replica 8 0 -" },
        .{ .expected = error.NumberOverflow, .line = "edit 8 18446744073709551616 1 -" },
        .{ .expected = error.NumberOverflow, .line = "emit 65536" },
        .{ .expected = error.InvalidNumber, .line = "emit 0" },
        .{ .expected = error.InvalidNumber, .line = "emit 7" },
        .{ .expected = error.InvalidNumber, .line = "emit 65535" },
        .{ .expected = error.MalformedTrace, .line = "capture 8 9bad" },
        .{ .expected = error.MalformedTrace, .line = "capture 8 bad.name" },
        .{ .expected = error.MalformedTrace, .line = "anchor 8 a 0 middle" },
        .{ .expected = error.MalformedTrace, .line = "line-ending 8 CRLF" },
        .{ .expected = error.InvalidHex, .line = "replica 8 1 0" },
        .{ .expected = error.InvalidHex, .line = "replica 8 1 0A" },
        .{ .expected = error.InvalidHex, .line = "replica 8 1 gg" },
        .{ .expected = error.InvalidUtf8, .line = "replica 8 1 c0af" },
        .{ .expected = error.InvalidUtf8, .line = "replica 8 1 eda080" },
        .{ .expected = error.InvalidUtf8, .line = "replica 8 1 f4908080" },
    };
    for (cases) |case| try std.testing.expectError(case.expected, trace.parseV2Line(case.line));
}

test "v2 rejects invalid encoding BOM NUL and physical line endings" {
    try std.testing.expectError(error.InvalidUtf8, trace.parseV2Line("emit \xff"));
    try std.testing.expectError(error.MalformedTrace, trace.parseV2Line("emit\x00 all"));
    try std.testing.expectError(error.MalformedTrace, trace.parseV2Line("\xef\xbb\xbftrace 2"));
    try std.testing.expectError(error.InvalidLineEnding, trace.parseV2Line("emit all\r"));
    try std.testing.expectError(error.InvalidLineEnding, trace.parseV2Line("emit all\n"));

    var mixed = trace.V2Parser.init();
    _ = try mixed.parseLine("trace 2\n");
    try std.testing.expectError(error.InvalidLineEnding, mixed.parseLine("emit all\r\n"));
    var bare_cr = trace.V2Parser.init();
    try std.testing.expectError(error.InvalidLineEnding, bare_cr.parseLine("trace 2\r"));
    var late_bom = trace.V2Parser.init();
    _ = try late_bom.parseLine("trace 2\n");
    try std.testing.expectError(error.MalformedTrace, late_bom.parseLine("\xef\xbb\xbfemit all\n"));
    var mid_bom = trace.V2Parser.init();
    try std.testing.expectError(error.MalformedTrace, mid_bom.parseLine("trace \xef\xbb\xbf2\n"));
}

fn expectTag(expected: anytype, value: anytype) !void {
    try std.testing.expectEqual(expected, std.meta.activeTag(value));
}
