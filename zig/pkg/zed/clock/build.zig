const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const module = b.addModule("clock", .{
        .root_source_file = b.path("src/clock/root.zig"),
        .target = target,
        .optimize = optimize,
    });

    const test_step = b.step("test", "Run clock tests");
    for ([_][]const u8{
        "tests/clock_test.zig",
        "tests/global_test.zig",
        "tests/property_test.zig",
        "tests/system_clock_test.zig",
    }) |test_path| {
        const unit_tests = b.addTest(.{
            .root_module = b.createModule(.{
                .root_source_file = b.path(test_path),
                .target = target,
                .optimize = optimize,
                .imports = &.{.{ .name = "clock", .module = module }},
            }),
        });
        test_step.dependOn(&b.addRunArtifact(unit_tests).step);
    }

    const differential_exe = b.addExecutable(.{
        .name = "clock_differential",
        .root_module = b.createModule(.{
            .root_source_file = b.path("tests/differential.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{.{ .name = "clock", .module = module }},
        }),
    });
    const differential_step = b.step("differential", "Run the clock differential trace consumer");
    differential_step.dependOn(&b.addRunArtifact(differential_exe).step);
}
