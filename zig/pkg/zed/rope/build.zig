const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});
    const sum_tree_dependency = b.dependency("sum_tree", .{
        .target = target,
        .optimize = optimize,
    });

    const module = b.addModule("rope", .{
        .root_source_file = b.path("src/rope/root.zig"),
        .target = target,
        .optimize = optimize,
        .imports = &.{.{
            .name = "sum_tree",
            .module = sum_tree_dependency.module("sum_tree"),
        }},
    });

    const test_step = b.step("test", "Run rope tests");
    for ([_][]const u8{
        "tests/grapheme_test.zig",
        "tests/contract_test.zig",
        "tests/point_test.zig",
        "tests/text_summary_test.zig",
        "tests/chunk_test.zig",
        "tests/rope_test.zig",
        "tests/iterator_test.zig",
        "tests/compatibility_test.zig",
    }) |test_path| {
        const unit_tests = b.addTest(.{
            .root_module = b.createModule(.{
                .root_source_file = b.path(test_path),
                .target = target,
                .optimize = optimize,
                .imports = &.{.{ .name = "rope", .module = module }},
            }),
        });
        test_step.dependOn(&b.addRunArtifact(unit_tests).step);
    }

    const bench_exe = b.addExecutable(.{
        .name = "rope_bench",
        .root_module = b.createModule(.{
            .root_source_file = b.path("bench.zig"),
            .target = target,
            .optimize = if (optimize == .Debug) .ReleaseFast else optimize,
            .imports = &.{.{ .name = "rope", .module = module }},
        }),
    });
    const bench_step = b.step("bench", "Run rope benchmarks");
    bench_step.dependOn(&b.addRunArtifact(bench_exe).step);

    const differential_exe = b.addExecutable(.{
        .name = "rope_differential",
        .root_module = b.createModule(.{
            .root_source_file = b.path("tests/differential.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{.{ .name = "rope", .module = module }},
        }),
    });
    const run_differential = b.addRunArtifact(differential_exe);
    const differential_step = b.step("differential", "Run the rope differential trace consumer");
    differential_step.dependOn(&run_differential.step);
}
