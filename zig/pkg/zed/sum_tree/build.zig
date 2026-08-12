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
    for ([_][]const u8{
        "tests/sum_tree_test.zig",
        "tests/tree_map_test.zig",
        "tests/compatibility_test.zig",
        "tests/text_compatibility_test.zig",
        "tests/cursor_gate_test.zig",
        "tests/parallel_gate_test.zig",
        "tests/persistence_gate_test.zig",
    }) |test_path| {
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

    const differential_exe = b.addExecutable(.{
        .name = "sum_tree_differential",
        .root_module = b.createModule(.{
            .root_source_file = b.path("tests/differential.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{.{ .name = "sum_tree", .module = module }},
        }),
    });
    const differential_step = b.step("differential", "Run the Zig differential trace consumer");
    differential_step.dependOn(&b.addRunArtifact(differential_exe).step);

    const benchmark_exe = b.addExecutable(.{
        .name = "sum_tree_bench",
        .root_module = b.createModule(.{
            .root_source_file = b.path("bench.zig"),
            .target = target,
            .optimize = .ReleaseFast,
            .imports = &.{.{ .name = "sum_tree", .module = module }},
        }),
    });
    const benchmark_step = b.step("bench", "Run sum_tree release benchmarks");
    benchmark_step.dependOn(&b.addRunArtifact(benchmark_exe).step);
}
