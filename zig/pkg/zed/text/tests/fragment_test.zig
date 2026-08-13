const std = @import("std");
const text = @import("text");

fn timestamp(value: u32, replica: u16) text.clock.Lamport {
    return .{ .value = value, .replica_id = text.clock.ReplicaId.new(replica) };
}

fn makeFragment(allocator: std.mem.Allocator, component: u64, ts: text.clock.Lamport, offset: u32, len: u32, visible: bool) !text.Fragment {
    var id = try text.Locator.init(allocator, &.{component});
    defer id.deinit();
    return text.Fragment.init(allocator, &id, ts, offset, len, visible);
}

test "fragment split preserves insertion span and deep ownership" {
    const allocator = std.testing.allocator;
    var fragment = try makeFragment(allocator, 10, timestamp(1, 1), 4, 7, true);
    defer fragment.deinit();
    try fragment.addDeletion(timestamp(2, 1));
    var right_id = try text.Locator.init(allocator, &.{ 10, 20 });
    defer right_id.deinit();
    var right = try fragment.split(3, &right_id);
    defer right.deinit();

    try std.testing.expectEqual(@as(u32, 3), fragment.len);
    try std.testing.expectEqual(@as(u32, 4), right.len);
    try std.testing.expectEqual(@as(u32, 7), right.insertion_offset);
    try std.testing.expect(right.id.eql(&right_id));
    try std.testing.expectEqual(@as(usize, 1), right.deletions.items.len);
}

test "fragment tree summaries and retained snapshots stay isolated" {
    const allocator = std.testing.allocator;
    var tree = try text.FragmentTree.init(allocator, null);
    defer tree.deinit();
    var first = try makeFragment(allocator, 1, timestamp(1, 1), 0, 3, true);
    defer first.deinit();
    try tree.push(first, null);
    var snapshot = tree.clone();
    defer snapshot.deinit();
    var second = try makeFragment(allocator, 2, timestamp(1, 2), 0, 2, false);
    defer second.deinit();
    try tree.push(second, null);

    try tree.validate(null);
    try snapshot.validate(null);
    try std.testing.expectEqual(@as(usize, 3), snapshot.summary().text.visible);
    try std.testing.expectEqual(@as(usize, 0), snapshot.summary().text.deleted);
    try std.testing.expectEqual(@as(usize, 3), tree.summary().text.visible);
    try std.testing.expectEqual(@as(usize, 2), tree.summary().text.deleted);
    try std.testing.expectEqual(@as(usize, 5), tree.extent(text.FullOffsetDimension, null).value);
}

test "versioned full offsets detect partially observed subtrees" {
    const allocator = std.testing.allocator;
    var tree = try text.FragmentTree.init(allocator, null);
    defer tree.deinit();
    var first = try makeFragment(allocator, 1, timestamp(1, 1), 0, 2, true);
    defer first.deinit();
    var second = try makeFragment(allocator, 2, timestamp(1, 2), 0, 3, true);
    defer second.deinit();
    try tree.push(first, null);
    try tree.push(second, null);

    var none = text.clock.Global.init(allocator);
    defer none.deinit();
    const none_extent = tree.extent(text.VersionedFullOffsetDimension, &none);
    try std.testing.expectEqual(@as(usize, 0), none_extent.fullOffset().?.value);

    var partial = text.clock.Global.init(allocator);
    defer partial.deinit();
    try partial.observe(first.timestamp);
    try std.testing.expect(tree.extent(text.VersionedFullOffsetDimension, &partial).fullOffset() == null);

    try partial.observe(second.timestamp);
    try std.testing.expectEqual(@as(usize, 5), tree.extent(text.VersionedFullOffsetDimension, &partial).fullOffset().?.value);
}

test "insertion keys and slices use Rust ordering" {
    const allocator = std.testing.allocator;
    var fragment = try makeFragment(allocator, 4, timestamp(3, 2), 5, 2, true);
    defer fragment.deinit();
    var insertion = try text.InsertionFragment.init(allocator, &fragment);
    defer insertion.deinit();
    try std.testing.expectEqual(@as(u32, 5), text.InsertionKeyOps.key(&insertion).split_offset);
    const slice = text.InsertionSlice.fromFragment(timestamp(9, 1), &fragment);
    try std.testing.expectEqual(@as(u32, 5), slice.range_start);
    try std.testing.expectEqual(@as(u32, 7), slice.range_end);
}

test "generated fragment and insertion indexes match flat summaries" {
    const allocator = std.testing.allocator;
    var fragments = try text.FragmentTree.init(allocator, null);
    defer fragments.deinit();
    var insertions = try text.InsertionTree.init(allocator, {});
    defer insertions.deinit();

    var expected_visible: usize = 0;
    var expected_deleted: usize = 0;
    var index: u32 = 0;
    while (index < 160) : (index += 1) {
        const len: u32 = index % 7 + 1;
        const visible = index % 3 != 0;
        var fragment = try makeFragment(allocator, index + 1, timestamp(index + 1, @intCast(index % 4 + 1)), index * 8, len, visible);
        defer fragment.deinit();
        try fragments.push(fragment, null);
        var insertion = try text.InsertionFragment.init(allocator, &fragment);
        defer insertion.deinit();
        try insertions.push(insertion, {});
        if (visible) expected_visible += len else expected_deleted += len;
    }

    try fragments.validate(null);
    try insertions.validate({});
    try std.testing.expectEqual(expected_visible, fragments.summary().text.visible);
    try std.testing.expectEqual(expected_deleted, fragments.summary().text.deleted);
    try std.testing.expectEqual(expected_visible + expected_deleted, fragments.extent(text.FullOffsetDimension, null).value);

    var snapshot = fragments.clone();
    defer snapshot.deinit();
    var extra = try makeFragment(allocator, 1000, timestamp(1000, 1), 0, 5, true);
    defer extra.deinit();
    try fragments.push(extra, null);
    try fragments.validate(null);
    try snapshot.validate(null);
    try std.testing.expectEqual(expected_visible, snapshot.summary().text.visible);
    try std.testing.expectEqual(expected_visible + 5, fragments.summary().text.visible);
}

test "fragment builder appends persistent subtrees" {
    const allocator = std.testing.allocator;
    var prefix = try text.FragmentTree.init(allocator, null);
    defer prefix.deinit();
    var first = try makeFragment(allocator, 1, timestamp(1, 1), 0, 2, true);
    defer first.deinit();
    try prefix.push(first, null);

    var builder = try text.FragmentBuilder.init(allocator, null);
    errdefer builder.deinit();
    try builder.append(&prefix, null);
    var second = try makeFragment(allocator, 2, timestamp(2, 1), 2, 3, false);
    defer second.deinit();
    try builder.push(second, null);
    var built = builder.finish();
    defer built.deinit();

    try built.validate(null);
    try std.testing.expectEqual(@as(usize, 2), built.summary().text.visible);
    try std.testing.expectEqual(@as(usize, 3), built.summary().text.deleted);
    try std.testing.expectEqual(@as(usize, 2), prefix.summary().text.visible);
}

test "visible and deleted ropes reconstruct from fragment visibility" {
    const allocator = std.testing.allocator;
    var tree = try text.FragmentTree.init(allocator, null);
    defer tree.deinit();
    var first = try makeFragment(allocator, 1, timestamp(1, 1), 0, 2, false);
    defer first.deinit();
    var second = try makeFragment(allocator, 2, timestamp(2, 1), 2, 2, true);
    defer second.deinit();
    try tree.push(first, null);
    try tree.push(second, null);
    var old_visible = try text.Rope.initText(allocator, "abcd");
    defer old_visible.deinit();
    var old_deleted = try text.Rope.init(allocator);
    defer old_deleted.deinit();
    var rebuilt = try text.rebuildFragmentRopes(allocator, &tree, &old_visible, &old_deleted, &.{ true, true });
    defer rebuilt.visible.deinit();
    defer rebuilt.deleted.deinit();
    const visible = try rebuilt.visible.toOwnedSlice(allocator);
    defer allocator.free(visible);
    try std.testing.expectEqualStrings("cd", visible);
    const deleted = try rebuilt.deleted.toOwnedSlice(allocator);
    defer allocator.free(deleted);
    try std.testing.expectEqualStrings("ab", deleted);
}
