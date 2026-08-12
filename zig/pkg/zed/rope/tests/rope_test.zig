const std = @import("std");
const rope = @import("rope");

fn expectMaterialized(expected: []const u8, value: *const rope.Rope) !void {
    const actual = try value.toOwnedSlice(std.testing.allocator);
    defer std.testing.allocator.free(actual);
    try std.testing.expectEqualStrings(expected, actual);
}

fn byteBoundaries(text: []const u8, output: *[1025]usize) []const usize {
    var count: usize = 0;
    var offset: usize = 0;
    while (offset < text.len) {
        output[count] = offset;
        count += 1;
        offset += @intCast(std.unicode.utf8ByteSequenceLength(text[offset]) catch unreachable);
    }
    output[count] = text.len;
    return output[0 .. count + 1];
}

test "empty construction and O(1) snapshots" {
    var value = try rope.Rope.init(std.testing.allocator);
    defer value.deinit();
    var snapshot = value.clone();
    defer snapshot.deinit();
    try std.testing.expect(value.isEmpty());
    try std.testing.expectEqual(@as(usize, 0), value.len());
    try std.testing.expectEqual(rope.Point.zero(), value.maxPoint());
    try value.validate();
    try snapshot.validate();
}

test "large UTF-8 construction is balanced and materializes exactly" {
    var source: std.ArrayList(u8) = .empty;
    defer source.deinit(std.testing.allocator);
    for (0..900) |index| {
        try source.appendSlice(std.testing.allocator, if (index % 5 == 0) "😀\n" else if (index % 3 == 0) "é" else "abc");
    }
    var value = try rope.Rope.initText(std.testing.allocator, source.items);
    defer value.deinit();
    try std.testing.expect(value.chunkCount() > 12);
    try expectMaterialized(source.items, &value);
    try std.testing.expectEqual(try rope.TextSummary.parse(source.items), value.summary());
    try value.validate();
}

test "coordinate conversions match chunk-independent flat model" {
    const text = "ab😀\nβe\u{301}\n👩‍💻 tail";
    var value = try rope.Rope.initText(std.testing.allocator, text);
    defer value.deinit();
    var storage: [1025]usize = undefined;
    for (byteBoundaries(text, &storage)) |offset| {
        const prefix = try rope.TextSummary.parse(text[0..offset]);
        try std.testing.expectEqual(prefix.lines, value.offsetToPoint(offset));
        try std.testing.expectEqual(prefix.linesUtf16(), value.offsetToPointUtf16(offset));
        try std.testing.expectEqual(prefix.len_utf16, value.offsetToOffsetUtf16(offset));
        try std.testing.expectEqual(offset, value.pointToOffset(prefix.lines));
        try std.testing.expectEqual(offset, value.offsetUtf16ToOffset(prefix.len_utf16));
        try std.testing.expectEqual(prefix.linesUtf16(), value.pointToPointUtf16(prefix.lines));
    }
    try std.testing.expectEqual(text.len, value.pointToOffset(.max));
    try std.testing.expectEqual(value.maxPoint(), value.offsetToPoint(text.len + 100));
}

test "boundary and coordinate clipping matches Rust semantics" {
    const text = "a😀e\u{301}\n👩‍💻z";
    var value = try rope.Rope.initText(std.testing.allocator, text);
    defer value.deinit();
    try std.testing.expect(!value.isCharBoundary(2));
    try std.testing.expectEqual(@as(usize, 1), value.floorCharBoundary(2));
    try std.testing.expectEqual(@as(usize, 5), value.ceilCharBoundary(2));
    try std.testing.expectEqual(rope.Point.new(0, 5), value.clipPoint(.new(0, 6), .left));
    try std.testing.expectEqual(rope.Point.new(0, 8), value.clipPoint(.new(0, 6), .right));
    try std.testing.expectEqual(rope.Point.new(1, 0), value.clipPoint(.new(1, 5), .left));
    try std.testing.expectEqual(rope.Point.new(1, 11), value.clipPoint(.new(1, 5), .right));
    try std.testing.expectEqual(value.maxPoint().column, value.lineLen(value.maxPoint().row));
}

test "push front append and join repair preserve text" {
    var value = try rope.Rope.initText(std.testing.allocator, "middle");
    defer value.deinit();
    try value.push(" tail");
    try value.pushFront("head ");

    var other = try rope.Rope.initText(std.testing.allocator, " end");
    defer other.deinit();
    try value.append(&other);

    try expectMaterialized("head middle tail end", &value);
    try std.testing.expectEqual(@as(usize, 1), value.chunkCount());
    try value.validate();
    try expectMaterialized(" end", &other);
}

test "byte and row slices preserve exact UTF-8 ranges" {
    var source: std.ArrayList(u8) = .empty;
    defer source.deinit(std.testing.allocator);
    for (0..90) |_| try source.appendSlice(std.testing.allocator, "αβ\n");
    try source.appendSlice(std.testing.allocator, "tail");

    var value = try rope.Rope.initText(std.testing.allocator, source.items);
    defer value.deinit();
    const start = std.mem.indexOf(u8, source.items, "β\nα") orelse unreachable;
    const end = start + "β\nα".len;
    var bytes = try value.sliceBytes(.{ .start = start, .end = end });
    defer bytes.deinit();
    try expectMaterialized("β\nα", &bytes);
    try bytes.validate();

    var rows = try value.sliceRows(.{ .start = 10, .end = 13 });
    defer rows.deinit();
    try expectMaterialized("αβ\nαβ\nαβ\n", &rows);
    try rows.validate();
}

test "replace is transactional and snapshots remain isolated" {
    var value = try rope.Rope.initText(std.testing.allocator, "one 😀 three");
    defer value.deinit();
    var original = value.clone();
    defer original.deinit();

    try value.replace(.{ .start = 4, .end = 8 }, "two");
    try expectMaterialized("one two three", &value);
    try expectMaterialized("one 😀 three", &original);

    try std.testing.expectError(error.InvalidUtf8, value.replace(.{ .start = 4, .end = 7 }, "\xff"));
    try expectMaterialized("one two three", &value);

    var edit_snapshot = value.clone();
    defer edit_snapshot.deinit();
    for (0..20) |index| {
        try value.replace(.{ .start = 4, .end = 7 }, if (index % 2 == 0) "TWO" else "two");
        try value.validate();
    }
    try expectMaterialized("one two three", &edit_snapshot);
}

fn allocationFailureReplace(allocator_value: std.mem.Allocator) !void {
    var source: std.ArrayList(u8) = .empty;
    defer source.deinit(allocator_value);
    for (0..80) |_| try source.appendSlice(allocator_value, "abcdefgh😀\n");

    var value = try rope.Rope.initText(allocator_value, source.items);
    defer value.deinit();
    var snapshot = value.clone();
    defer snapshot.deinit();
    try value.replace(.{ .start = 8, .end = 12 }, "replacement");
    try expectMaterialized(source.items, &snapshot);
    try value.validate();
    try snapshot.validate();
}

test "replace cleans up every induced allocation failure" {
    try std.testing.checkAllAllocationFailures(std.testing.allocator, allocationFailureReplace, .{});
}

test "prefix suffix and invalid UTF-8 behavior" {
    var value = try rope.Rope.initText(std.testing.allocator, "hello 😀 world");
    defer value.deinit();
    try std.testing.expect(value.startsWith("hello"));
    try std.testing.expect(value.startsWith("hello 😀"));
    try std.testing.expect(value.endsWith("world"));
    try std.testing.expect(value.endsWith("😀 world"));
    try std.testing.expect(!value.startsWith("world"));
    try std.testing.expectError(error.InvalidUtf8, rope.Rope.initText(std.testing.allocator, "\xff"));
}
