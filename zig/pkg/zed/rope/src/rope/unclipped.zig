/// Marks a coordinate that must not be clipped to a valid text boundary.
pub fn Unclipped(comptime T: type) type {
    return struct {
        const Self = @This();

        value: T = defaultValue(T),

        pub fn init(value: T) Self {
            return .{ .value = value };
        }

        pub fn add(self: Self, other: Self) Self {
            return .init(self.value.add(other.value));
        }

        pub fn addAssign(self: *Self, other: Self) void {
            if (@hasDecl(T, "addAssign")) self.value.addAssign(other.value) else self.value = self.value.add(other.value);
        }

        pub fn sub(self: Self, other: Self) Self {
            return .init(self.value.sub(other.value));
        }

        pub fn subAssign(self: *Self, other: Self) void {
            self.value = self.value.sub(other.value);
        }

        pub fn order(self: Self, other: Self) @TypeOf(self.value.order(other.value)) {
            return self.value.order(other.value);
        }

        fn defaultValue(comptime U: type) U {
            return .{};
        }
    };
}
