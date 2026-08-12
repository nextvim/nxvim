const std = @import("std");

/// A lexicographically ordered identifier for a position in a collection.
///
/// Locators of up to `inline_capacity` components are values with no allocation.
/// Larger locators own their heap storage and must be copied with `clone` or
/// `assign`, then released with `deinit`.
pub const Locator = struct {
    pub const inline_capacity: usize = 2;

    allocator: std.mem.Allocator,
    storage: Storage,

    const Storage = union(enum) {
        small: Inline,
        heap: []u64,
    };

    const Inline = struct {
        len: u2,
        components: [inline_capacity]u64,
    };

    pub const BetweenError = std.mem.Allocator.Error || error{
        InvalidBounds,
        NoSpace,
    };

    pub fn init(allocator: std.mem.Allocator, components: []const u64) std.mem.Allocator.Error!Locator {
        if (components.len <= inline_capacity) {
            var values: [inline_capacity]u64 = .{0} ** inline_capacity;
            @memcpy(values[0..components.len], components);
            return .{
                .allocator = allocator,
                .storage = .{ .small = .{
                    .len = @intCast(components.len),
                    .components = values,
                } },
            };
        }

        return .{
            .allocator = allocator,
            .storage = .{ .heap = try allocator.dupe(u64, components) },
        };
    }

    pub fn min(allocator: std.mem.Allocator) std.mem.Allocator.Error!Locator {
        return init(allocator, &.{0});
    }

    pub fn max(allocator: std.mem.Allocator) std.mem.Allocator.Error!Locator {
        return init(allocator, &.{std.math.maxInt(u64)});
    }

    pub fn deinit(self: *Locator) void {
        switch (self.storage) {
            .small => {},
            .heap => |components| self.allocator.free(components),
        }
        self.* = undefined;
    }

    pub fn clone(self: *const Locator, allocator: std.mem.Allocator) std.mem.Allocator.Error!Locator {
        return init(allocator, self.slice());
    }

    /// Replaces this locator only after the complete deep copy succeeds.
    pub fn assign(self: *Locator, other: *const Locator) std.mem.Allocator.Error!void {
        if (self == other) return;
        const replacement = try other.clone(self.allocator);
        self.deinit();
        self.* = replacement;
    }

    /// Returns components borrowed from this locator. The slice is invalidated
    /// by mutation, assignment, deinitialization, or moving an inline locator.
    pub fn slice(self: *const Locator) []const u64 {
        return switch (self.storage) {
            .small => |*value| value.components[0..value.len],
            .heap => |components| components,
        };
    }

    pub fn len(self: *const Locator) usize {
        return self.slice().len;
    }

    pub fn isEmpty(self: *const Locator) bool {
        return self.len() == 0;
    }

    pub fn eql(self: *const Locator, other: *const Locator) bool {
        return std.mem.eql(u64, self.slice(), other.slice());
    }

    /// Strict total ordering matching Rust's derived `Ord` for `SmallVec<u64>`.
    pub fn order(self: *const Locator, other: *const Locator) std.math.Order {
        const left = self.slice();
        const right = other.slice();
        const common_len = @min(left.len, right.len);
        for (left[0..common_len], right[0..common_len]) |left_component, right_component| {
            if (left_component < right_component) return .lt;
            if (left_component > right_component) return .gt;
        }
        return std.math.order(left.len, right.len);
    }

    /// Produces a locator strictly inside `(lhs, rhs)` using Zed's `>> 48`
    /// right-biased midpoint. Bounds must be strictly ordered.
    pub fn between(
        allocator: std.mem.Allocator,
        lhs: *const Locator,
        rhs: *const Locator,
    ) BetweenError!Locator {
        if (lhs.order(rhs) != .lt) return error.InvalidBounds;

        const left_components = lhs.slice();
        const right_components = rhs.slice();
        const max_input_len = @max(left_components.len, right_components.len);
        const depth_limit = std.math.add(usize, max_input_len, 1) catch return error.NoSpace;
        var depth: usize = 0;
        var found_space = false;
        while (depth < depth_limit) : (depth += 1) {
            const left = componentOr(left_components, depth, 0);
            const right = componentOr(right_components, depth, std.math.maxInt(u64));
            const middle = left + ((right -| left) >> 48);
            if (middle > left) {
                depth += 1;
                found_space = true;
                break;
            }
        }
        if (!found_space) return error.NoSpace;

        if (depth <= inline_capacity) {
            var result = try init(allocator, &.{});
            var small = &result.storage.small;
            small.len = @intCast(depth);
            for (small.components[0..depth], 0..) |*component, index| {
                const left = componentOr(left_components, index, 0);
                const right = componentOr(right_components, index, std.math.maxInt(u64));
                component.* = left + ((right -| left) >> 48);
            }
            if (lhs.order(&result) != .lt or result.order(rhs) != .lt) return error.NoSpace;
            return result;
        }

        const components = try allocator.alloc(u64, depth);
        errdefer allocator.free(components);
        for (components, 0..) |*component, index| {
            const left = componentOr(left_components, index, 0);
            const right = componentOr(right_components, index, std.math.maxInt(u64));
            component.* = left + ((right -| left) >> 48);
        }
        const result = Locator{ .allocator = allocator, .storage = .{ .heap = components } };
        if (lhs.order(&result) != .lt or result.order(rhs) != .lt) return error.NoSpace;
        return result;
    }

    fn componentOr(components: []const u64, index: usize, fallback: u64) u64 {
        return if (index < components.len) components[index] else fallback;
    }
};
