const std = @import("std");
const rope = @import("rope");

test "phase baseline is pinned" {
    try std.testing.expectEqualStrings("0.16.0", rope.baseline.zig);
    try std.testing.expectEqualStrings("1.13.3", rope.baseline.unicode_segmentation_crate);
    _ = rope.sum_tree.Bias.left;
}
