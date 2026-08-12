const std = @import("std");
const text = @import("text");

test "trace parser accepts the version-one command" {
    try std.testing.expectEqual(text.trace.Command.emit, (try text.trace.parseLine("emit")).?);
    try std.testing.expectEqual(@as(?text.trace.Command, null), try text.trace.parseLine("  # comment"));
    try std.testing.expectEqual(@as(?text.trace.Command, null), try text.trace.parseLine(" \t\r\n"));
}

test "malformed traces return errors" {
    try std.testing.expectError(error.MalformedTrace, text.trace.parseLine("unknown"));
    try std.testing.expectError(error.MalformedTrace, text.trace.parseLine("emit extra"));
}

test "initial state is canonical and versioned" {
    try std.testing.expectEqualStrings(
        "state version=1 text=- version-vector=- operations=0 deferred=0 history=0",
        text.trace.initial_state,
    );
}
