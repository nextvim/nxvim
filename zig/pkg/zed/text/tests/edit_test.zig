const std = @import("std");
const text = @import("text");

const Edit = text.Edit(usize);

test "Edit exposes generic ranges, lengths, and Rust empty semantics" {
    const replacement: Edit = .{
        .old = .{ .start = 2, .end = 5 },
        .new = .{ .start = 2, .end = 7 },
    };
    try std.testing.expectEqual(@as(usize, 3), replacement.oldLen());
    try std.testing.expectEqual(@as(usize, 5), replacement.newLen());
    try std.testing.expect(!replacement.isEmpty());

    try std.testing.expect((Edit{
        .old = .{ .start = 4, .end = 4 },
        .new = .{ .start = 9, .end = 9 },
    }).isEmpty());
    try std.testing.expect(!(Edit{
        .old = .{ .start = 4, .end = 4 },
        .new = .{ .start = 9, .end = 10 },
    }).isEmpty());
}

test "Edit is generic over a non-usize arithmetic coordinate" {
    const SignedEdit = text.Edit(i32);
    const item: SignedEdit = .{
        .old = .{ .start = -3, .end = 4 },
        .new = .{ .start = -3, .end = 1 },
    };
    try std.testing.expectEqual(@as(i32, 7), item.oldLen());
    try std.testing.expectEqual(@as(i32, 4), item.newLen());
}
