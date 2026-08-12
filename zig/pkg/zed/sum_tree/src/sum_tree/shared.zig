const std = @import("std");

pub fn Shared(comptime T: type, comptime Hooks: type) type {
    return struct {
        const Self = @This();
        const Allocation = struct {
            ref_count: std.atomic.Value(usize),
            allocator: std.mem.Allocator,
            value: T,
        };

        allocation: *Allocation,

        pub fn init(allocator: std.mem.Allocator, value: T) std.mem.Allocator.Error!Self {
            const allocation = try allocator.create(Allocation);
            allocation.* = .{
                .ref_count = std.atomic.Value(usize).init(1),
                .allocator = allocator,
                .value = value,
            };
            return .{ .allocation = allocation };
        }

        pub fn clone(self: Self) Self {
            _ = self.allocation.ref_count.fetchAdd(1, .monotonic);
            return self;
        }

        pub fn deinit(self: *Self) void {
            const allocation = self.allocation;
            if (allocation.ref_count.fetchSub(1, .acq_rel) == 1) {
                Hooks.deinit(&allocation.value, allocation.allocator);
                allocation.allocator.destroy(allocation);
            }
            self.* = undefined;
        }

        pub fn get(self: *const Self) *const T {
            return &self.allocation.value;
        }

        pub fn isUnique(self: *const Self) bool {
            return self.allocation.ref_count.load(.acquire) == 1;
        }

        pub fn makeUnique(self: *Self) !*T {
            if (!self.isUnique()) {
                const old = self.allocation;
                const copied = try Hooks.clone(&old.value, old.allocator);
                errdefer {
                    var value = copied;
                    Hooks.deinit(&value, old.allocator);
                }
                const replacement = try Self.init(old.allocator, copied);
                self.deinit();
                self.* = replacement;
            }
            return &self.allocation.value;
        }
    };
}
