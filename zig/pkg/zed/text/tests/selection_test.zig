const std = @import("std");
const text = @import("text");
const Selection = text.Selection(usize);

fn double(value: usize) u32 {
    return @intCast(value * 2);
}

test "selection exposes ordered range and directional endpoints" {
    const forward = Selection{ .id = 1, .start = 2, .end = 8 };
    try std.testing.expectEqual(@as(usize, 8), forward.head());
    try std.testing.expectEqual(@as(usize, 2), forward.tail());
    try std.testing.expectEqual(@as(usize, 6), forward.len());
    try std.testing.expectEqual(@as(usize, 2), forward.range().start);
    try std.testing.expectEqual(@as(usize, 8), forward.range().end);
    try std.testing.expect(forward.equals(.{ .start = 2, .end = 8 }));

    const reversed = Selection{ .id = 2, .start = 2, .end = 8, .reversed = true };
    try std.testing.expectEqual(@as(usize, 2), reversed.head());
    try std.testing.expectEqual(@as(usize, 8), reversed.tail());
}

test "selection construction and mapping preserve Rust metadata" {
    const collapsed = Selection.fromOffset(12);
    try std.testing.expectEqual(@as(usize, 0), collapsed.id);
    try std.testing.expect(collapsed.isEmpty());
    try std.testing.expectEqual(text.SelectionGoal.none, collapsed.goal);

    const source = Selection{
        .id = 7,
        .start = 2,
        .end = 5,
        .reversed = true,
        .goal = .{ .horizontal_range = .{ .start = 1.5, .end = 9.25 } },
    };
    const mapped = source.map(u32, double);
    try std.testing.expectEqual(@as(u32, 4), mapped.start);
    try std.testing.expectEqual(@as(u32, 10), mapped.end);
    try std.testing.expect(mapped.reversed);
    try std.testing.expectEqual(source.goal, mapped.goal);
}

test "setHead crosses and meets the tail exactly like Rust" {
    var value = Selection{ .id = 1, .start = 3, .end = 7 };
    value.setHead(1, .{ .horizontal_position = 4.5 });
    try std.testing.expectEqual(Selection{ .id = 1, .start = 1, .end = 3, .reversed = true, .goal = .{ .horizontal_position = 4.5 } }, value);

    value.setHead(3, .none);
    try std.testing.expectEqual(Selection{ .id = 1, .start = 3, .end = 3 }, value);
    value.setHead(5, .none);
    try std.testing.expectEqual(Selection{ .id = 1, .start = 3, .end = 5 }, value);
}

test "setTail equality and crossing match Rust reversal behavior" {
    var value = Selection{ .id = 1, .start = 3, .end = 7 };
    value.setTail(9, .none);
    try std.testing.expectEqual(Selection{ .id = 1, .start = 7, .end = 9, .reversed = true }, value);

    value.setTail(7, .none);
    try std.testing.expectEqual(Selection{ .id = 1, .start = 7, .end = 7 }, value);
    try std.testing.expect(value.isEmpty());
}

test "setHeadTail swap and collapse are deterministic" {
    var value = Selection{ .id = 9, .start = 0, .end = 0 };
    value.setHeadTail(2, 10, .{ .wrapped_horizontal_position = .{ .row = 3, .x = 1.25 } });
    try std.testing.expect(value.reversed);
    try std.testing.expectEqual(@as(usize, 2), value.start);
    try std.testing.expectEqual(@as(usize, 10), value.end);

    value.swapHeadTail();
    try std.testing.expectEqual(Selection{ .id = 9, .start = 2, .end = 10, .goal = value.goal }, value);
    value.swapHeadTail();
    try std.testing.expectEqual(@as(usize, 10), value.start);
    try std.testing.expectEqual(@as(usize, 2), value.end);

    value.collapseTo(4, .{ .horizontal_range = .{ .start = 2.0, .end = 5.0 } });
    try std.testing.expect(value.isEmpty());
    try std.testing.expect(!value.reversed);
}

test "all small ordered ranges retain a fixed tail while the head moves" {
    var start: usize = 0;
    while (start <= 4) : (start += 1) {
        var end = start;
        while (end <= 4) : (end += 1) {
            var head: usize = 0;
            while (head <= 4) : (head += 1) {
                var value = Selection{ .id = 0, .start = start, .end = end };
                const tail = value.tail();
                value.setHead(head, .none);
                try std.testing.expectEqual(head, value.head());
                try std.testing.expectEqual(tail, value.tail());
                try std.testing.expect(value.start <= value.end);
            }
        }
    }
}
