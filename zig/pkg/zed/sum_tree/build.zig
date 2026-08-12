const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const module = b.addModule("sum_tree", .{
        .root_source_file = b.path("src/sum_tree/root.zig"),
        .target = target,
        .optimize = optimize,
    });

    const test_step = b.step("test", "Run sum_tree tests");
    for ([_][]const u8{ "tests/sum_tree_test.zig", "tests/tree_map_test.zig" }) |test_path| {
        const unit_tests = b.addTest(.{
            .root_module = b.createModule(.{
                .root_source_file = b.path(test_path),
                .target = target,
                .optimize = optimize,
                .imports = &.{.{ .name = "sum_tree", .module = module }},
            }),
        });
        const run_tests = b.addRunArtifact(unit_tests);
        test_step.dependOn(&run_tests.step);
    }
}
