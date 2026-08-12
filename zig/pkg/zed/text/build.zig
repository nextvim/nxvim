const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});
    const clock_dependency = b.dependency("clock", .{
        .target = target,
        .optimize = optimize,
    });
    const rope_dependency = b.dependency("rope", .{
        .target = target,
        .optimize = optimize,
    });
    const sum_tree_dependency = b.dependency("sum_tree", .{
        .target = target,
        .optimize = optimize,
    });

    const module = b.addModule("text", .{
        .root_source_file = b.path("src/text/root.zig"),
        .target = target,
        .optimize = optimize,
        .imports = &.{
            .{ .name = "clock", .module = clock_dependency.module("clock") },
            .{ .name = "rope", .module = rope_dependency.module("rope") },
            .{ .name = "sum_tree", .module = sum_tree_dependency.module("sum_tree") },
        },
    });

    const test_step = b.step("test", "Run text scaffold tests");
    for ([_][]const u8{
        "tests/contract_test.zig",
        "tests/clock_compatibility_test.zig",
        "tests/trace_test.zig",
    }) |test_path| {
        const unit_tests = b.addTest(.{
            .root_module = b.createModule(.{
                .root_source_file = b.path(test_path),
                .target = target,
                .optimize = optimize,
                .imports = &.{.{ .name = "text", .module = module }},
            }),
        });
        test_step.dependOn(&b.addRunArtifact(unit_tests).step);
    }

    const differential_exe = b.addExecutable(.{
        .name = "text_differential",
        .root_module = b.createModule(.{
            .root_source_file = b.path("tests/differential.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{.{ .name = "text", .module = module }},
        }),
    });
    const differential_step = b.step("differential", "Run the text differential trace consumer");
    differential_step.dependOn(&b.addRunArtifact(differential_exe).step);
}
