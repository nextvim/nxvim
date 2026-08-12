const std = @import("std");
const text = @import("text");

const Patch = text.Patch(usize);
const Edit = Patch.Edit;

fn edit(old_start: usize, old_end: usize, new_start: usize, new_end: usize) Edit {
    return .{
        .old = .{ .start = old_start, .end = old_end },
        .new = .{ .start = new_start, .end = new_end },
    };
}

fn expectEdits(expected: []const Edit, actual: *const Patch) !void {
    try std.testing.expectEqualDeep(expected, actual.edits());
}

test "new clone clear invert and push variants own their storage" {
    const source = [_]Edit{edit(1, 3, 1, 4)};
    var patch = try Patch.new(std.testing.allocator, &source);
    errdefer patch.deinit();
    var copy = try patch.clone(std.testing.allocator);
    defer copy.deinit();

    patch.invert();
    try expectEdits(&.{edit(1, 4, 1, 3)}, &patch);
    try expectEdits(&source, &copy);

    try copy.push(edit(8, 8, 9, 9)); // Empty in both coordinate spaces.
    try std.testing.expectEqual(@as(usize, 1), copy.edits().len);
    try copy.pushMaybeEmpty(edit(4, 4, 5, 5));
    try std.testing.expectEqual(@as(usize, 2), copy.edits().len);
    try copy.pushMaybeEmpty(edit(4, 6, 5, 8));
    try expectEdits(&.{ edit(1, 3, 1, 4), edit(4, 6, 5, 8) }, &copy);

    copy.clear();
    try std.testing.expect(copy.isEmpty());

    var owned = patch.into();
    defer owned.deinit(std.testing.allocator);
    try std.testing.expectEqualDeep(&.{edit(1, 4, 1, 3)}, owned.items);
}

test "oldToNew and editForOldPosition preserve boundary semantics" {
    var patch = try Patch.new(std.testing.allocator, &.{
        edit(2, 5, 2, 7),
        edit(9, 10, 11, 11),
    });
    defer patch.deinit();

    const expected = [_]usize{ 0, 1, 2, 2, 2, 7, 8, 9, 10, 11, 11, 12 };
    for (expected, 0..) |translated, old| try std.testing.expectEqual(translated, patch.oldToNew(old));

    try std.testing.expectEqualDeep(edit(1, 1, 1, 1), patch.editForOldPosition(1));
    try std.testing.expectEqualDeep(edit(2, 5, 2, 7), patch.editForOldPosition(5));
    try std.testing.expectEqualDeep(edit(7, 7, 9, 9), patch.editForOldPosition(7));
    try std.testing.expectEqualDeep(edit(9, 10, 11, 11), patch.editForOldPosition(9));
}

fn applyFlat(allocator: std.mem.Allocator, original: []const u8, patch: *const Patch, new_text: []const u8) ![]u8 {
    var result: std.ArrayList(u8) = .empty;
    errdefer result.deinit(allocator);
    var cursor: usize = 0;
    for (patch.edits()) |item| {
        try result.appendSlice(allocator, original[cursor..item.old.start]);
        try result.appendSlice(allocator, new_text[item.new.start..item.new.end]);
        cursor = item.old.end;
    }
    try result.appendSlice(allocator, original[cursor..]);
    return result.toOwnedSlice(allocator);
}

fn expectCompositionFlat(first_edits: []const Edit, second_edits: []const Edit, expected_edits: []const Edit) !void {
    const original = "abcdefghijklmnopqrstuvwxyz";
    const inserted = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    var first = try Patch.new(std.testing.allocator, first_edits);
    defer first.deinit();
    var second = try Patch.new(std.testing.allocator, second_edits);
    defer second.deinit();
    var composed = try first.compose(second.edits());
    defer composed.deinit();
    try expectEdits(expected_edits, &composed);

    const middle = try applyFlat(std.testing.allocator, original, &first, inserted);
    defer std.testing.allocator.free(middle);
    const expected = try applyFlat(std.testing.allocator, middle, &second, inserted);
    defer std.testing.allocator.free(expected);
    const actual = try applyFlat(std.testing.allocator, original, &composed, expected);
    defer std.testing.allocator.free(actual);
    try std.testing.expectEqualStrings(expected, actual);
}

test "composition matches checked-in Rust examples and flat text model" {
    var first = try Patch.new(std.testing.allocator, &.{edit(1, 3, 1, 4)});
    defer first.deinit();
    var composed = try first.compose(&.{edit(0, 0, 0, 4)});
    defer composed.deinit();
    try expectEdits(&.{ edit(0, 0, 0, 4), edit(1, 3, 5, 8) }, &composed);

    var overlap = try Patch.new(std.testing.allocator, &.{edit(2, 6, 2, 5)});
    defer overlap.deinit();
    var overlap_composed = try overlap.compose(&.{edit(3, 4, 3, 7)});
    defer overlap_composed.deinit();
    try expectEdits(&.{edit(2, 6, 2, 8)}, &overlap_composed);

    try expectCompositionFlat(
        &.{ edit(1, 3, 1, 4), edit(8, 12, 9, 11) },
        &.{ edit(0, 0, 0, 4), edit(3, 10, 7, 9) },
        &.{ edit(0, 0, 0, 4), edit(1, 12, 5, 10) },
    );
    try expectCompositionFlat(
        &.{edit(0, 0, 0, 3)},
        &.{ edit(0, 0, 0, 1), edit(1, 2, 2, 2) },
        &.{edit(0, 0, 0, 3)},
    );
}

test "Patch is generic over unsigned coordinate widths" {
    const Patch32 = text.Patch(u32);
    var patch = try Patch32.new(std.testing.allocator, &.{.{
        .old = .{ .start = 2, .end = 4 },
        .new = .{ .start = 2, .end = 7 },
    }});
    defer patch.deinit();
    try std.testing.expectEqual(@as(u32, 8), patch.oldToNew(5));
    try std.testing.expectEqual(@as(u32, 2), patch.edits()[0].oldLen());
}

fn deterministicPatch(allocator: std.mem.Allocator, seed: usize, second_space: bool) !Patch {
    var result = Patch.empty(allocator);
    errdefer result.deinit();
    var old_cursor: usize = 0;
    var new_cursor: usize = 0;
    for (0..4) |index| {
        const gap = 1 + (seed + index * 3) % 3;
        old_cursor += gap;
        new_cursor += gap;
        const old_len = (seed * 3 + index) % 3;
        const new_len = (seed + index * 5 + @intFromBool(second_space)) % 4;
        try result.push(edit(old_cursor, old_cursor + old_len, new_cursor, new_cursor + new_len));
        old_cursor += old_len;
        new_cursor += new_len;
    }
    return result;
}

test "deterministic model: composed coordinate map preserves exterior positions" {
    // Rust intentionally coalesces touching edits and therefore discards some
    // intermediate left-biased mappings. Exterior positions remain unambiguous.
    for (0..32) |seed| {
        var first = try deterministicPatch(std.testing.allocator, seed, false);
        defer first.deinit();
        var second = try deterministicPatch(std.testing.allocator, seed + 11, true);
        defer second.deinit();
        var composed = try first.compose(second.edits());
        defer composed.deinit();

        try std.testing.expectEqual(second.oldToNew(first.oldToNew(0)), composed.oldToNew(0));
        const beyond: usize = 100;
        try std.testing.expectEqual(second.oldToNew(first.oldToNew(beyond)), composed.oldToNew(beyond));
    }
}

fn allocationScenario(allocator: std.mem.Allocator) !void {
    var first = Patch.empty(allocator);
    defer first.deinit();
    for (0..12) |index| try first.push(edit(index * 3, index * 3 + 1, index * 4, index * 4 + 2));
    var cloned = try first.clone(allocator);
    defer cloned.deinit();
    var composed = try first.compose(cloned.edits());
    defer composed.deinit();
}

test "all owning operations are allocation-failure safe" {
    try std.testing.checkAllAllocationFailures(std.testing.allocator, allocationScenario, .{});
}
