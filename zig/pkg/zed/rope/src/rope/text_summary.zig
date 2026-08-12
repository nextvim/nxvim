const std = @import("std");
pub const Point = @import("point.zig").Point;
pub const PointUtf16 = @import("point_utf16.zig").PointUtf16;
pub const OffsetUtf16 = @import("offset_utf16.zig").OffsetUtf16;

/// Allocation-free metrics for a valid UTF-8 string.
pub const TextSummary = struct {
    len: usize = 0,
    chars: usize = 0,
    len_utf16: OffsetUtf16 = .{},
    lines: Point = .{},
    first_line_chars: u32 = 0,
    last_line_chars: u32 = 0,
    last_line_len_utf16: u32 = 0,
    longest_row: u32 = 0,
    longest_row_chars: u32 = 0,

    pub fn zero() TextSummary {
        return .{};
    }

    pub fn linesUtf16(self: TextSummary) PointUtf16 {
        return .new(self.lines.row, self.last_line_len_utf16);
    }

    pub fn newline() TextSummary {
        return .{
            .len = 1,
            .chars = 1,
            .len_utf16 = .new(1),
            .lines = .new(1, 0),
        };
    }

    pub fn addNewline(self: *TextSummary) void {
        self.addAssign(newline());
    }

    /// Parse valid UTF-8. Invalid input returns `error.InvalidUtf8`.
    pub fn parse(text: []const u8) error{InvalidUtf8}!TextSummary {
        var result = TextSummary.zero();
        var view = std.unicode.Utf8View.init(text) catch return error.InvalidUtf8;
        var iterator = view.iterator();
        while (iterator.nextCodepointSlice()) |bytes| {
            const codepoint = std.unicode.utf8Decode(bytes) catch unreachable;
            result.chars += 1;
            const utf16_len: u32 = if (codepoint <= 0xffff) 1 else 2;
            result.len_utf16.value += utf16_len;

            if (codepoint == '\n') {
                result.lines = result.lines.add(.new(1, 0));
                result.last_line_chars = 0;
                result.last_line_len_utf16 = 0;
            } else {
                result.lines.column += @intCast(bytes.len);
                result.last_line_chars += 1;
                result.last_line_len_utf16 += utf16_len;
            }

            if (result.lines.row == 0) result.first_line_chars = result.last_line_chars;
            if (result.last_line_chars > result.longest_row_chars) {
                result.longest_row = result.lines.row;
                result.longest_row_chars = result.last_line_chars;
            }
        }
        result.len = text.len;
        return result;
    }

    pub fn add(self: TextSummary, other: TextSummary) TextSummary {
        var result = self;
        result.addAssign(other);
        return result;
    }

    pub fn addAssign(self: *TextSummary, other: TextSummary) void {
        const joined_chars = self.last_line_chars + other.first_line_chars;
        if (joined_chars > self.longest_row_chars) {
            self.longest_row = self.lines.row;
            self.longest_row_chars = joined_chars;
        }
        if (other.longest_row_chars > self.longest_row_chars) {
            self.longest_row = self.lines.row + other.longest_row;
            self.longest_row_chars = other.longest_row_chars;
        }

        if (self.lines.row == 0) self.first_line_chars += other.first_line_chars;
        if (other.lines.row == 0) {
            self.last_line_chars += other.first_line_chars;
            self.last_line_len_utf16 += other.last_line_len_utf16;
        } else {
            self.last_line_chars = other.last_line_chars;
            self.last_line_len_utf16 = other.last_line_len_utf16;
        }

        self.chars += other.chars;
        self.len += other.len;
        self.len_utf16.addAssign(other.len_utf16);
        self.lines.addAssign(other.lines);
    }
};

/// Compile-time extraction and composition of supported text dimensions.
pub fn TextDimension(comptime T: type) type {
    return struct {
        pub fn zero() T {
            return if (T == TextSummary or T == Point or T == PointUtf16 or T == OffsetUtf16) .{} else if (T == usize) 0 else @compileError("unsupported text dimension");
        }

        pub fn fromTextSummary(summary: TextSummary) T {
            if (T == TextSummary) return summary;
            if (T == usize) return summary.len;
            if (T == OffsetUtf16) return summary.len_utf16;
            if (T == Point) return summary.lines;
            if (T == PointUtf16) return summary.linesUtf16();
            @compileError("unsupported text dimension");
        }

        pub fn addAssign(value: *T, other: T) void {
            if (T == usize) value.* += other else value.addAssign(other);
        }
    };
}

/// Two dimensions whose ordering/equality is determined only by `key`.
pub fn DimensionPair(comptime K: type, comptime V: type) type {
    return struct {
        const Self = @This();

        key: K = TextDimension(K).zero(),
        value: ?V = TextDimension(V).zero(),

        pub fn fromTextSummary(summary: TextSummary) Self {
            return .{
                .key = TextDimension(K).fromTextSummary(summary),
                .value = TextDimension(V).fromTextSummary(summary),
            };
        }

        pub fn order(self: Self, other: Self) std.math.Order {
            if (K == usize) return std.math.order(self.key, other.key);
            return self.key.order(other.key);
        }

        pub fn eql(self: Self, other: Self) bool {
            return self.order(other) == .eq;
        }

        pub fn addAssign(self: *Self, other: Self) void {
            TextDimension(K).addAssign(&self.key, other.key);
            if (self.value) |*value| {
                if (other.value) |other_value| TextDimension(V).addAssign(value, other_value) else self.value = null;
            }
        }

        pub fn sub(self: Self, other: Self) Self {
            return .{
                .key = subtract(K, self.key, other.key),
                .value = if (self.value != null and other.value != null) subtract(V, self.value.?, other.value.?) else null,
            };
        }

        fn subtract(comptime T: type, left: T, right: T) T {
            if (T == usize) return left - right;
            return left.sub(right);
        }
    };
}
