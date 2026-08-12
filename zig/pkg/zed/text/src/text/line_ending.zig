const std = @import("std");
const builtin = @import("builtin");

/// Allocator-explicit equivalent of Rust's `Cow<str>` normalization result.
/// `.borrowed` aliases the input; `.owned` must be released with `deinit`.
pub const Normalized = union(enum) {
    borrowed: []const u8,
    owned: []u8,

    pub fn slice(self: Normalized) []const u8 {
        return switch (self) {
            .borrowed => |text| text,
            .owned => |text| text,
        };
    }

    pub fn deinit(self: *Normalized, allocator: std.mem.Allocator) void {
        switch (self.*) {
            .borrowed => {},
            .owned => |text| allocator.free(text),
        }
        self.* = .{ .borrowed = "" };
    }
};

pub const LineEnding = enum {
    unix,
    windows,

    pub fn default() LineEnding {
        return if (builtin.os.tag == .windows) .windows else .unix;
    }

    pub fn asStr(self: LineEnding) []const u8 {
        return switch (self) {
            .unix => "\n",
            .windows => "\r\n",
        };
    }

    pub fn label(self: LineEnding) []const u8 {
        return switch (self) {
            .unix => "LF",
            .windows => "CRLF",
        };
    }

    /// Detects the style of the first LF in the Rust-compatible 1000-byte prefix.
    pub fn detect(text: []const u8) LineEnding {
        var limit = @min(text.len, 1000);
        while (limit > 0 and limit < text.len and isUtf8Continuation(text[limit])) limit -= 1;

        if (std.mem.indexOfScalar(u8, text[0..limit], '\n')) |index| {
            return if (index > 0 and text[index - 1] == '\r') .windows else .unix;
        }
        return default();
    }

    /// Replaces CRLF and bare CR in place. The returned slice aliases `text`.
    /// No allocator is used; bytes after the returned slice are unspecified.
    pub fn normalizeInPlace(text: []u8) []u8 {
        var read: usize = 0;
        var write: usize = 0;
        while (read < text.len) {
            if (text[read] == '\r') {
                text[write] = '\n';
                write += 1;
                read += 1;
                if (read < text.len and text[read] == '\n') read += 1;
            } else {
                text[write] = text[read];
                write += 1;
                read += 1;
            }
        }
        return text[0..write];
    }

    /// Normalizes like Rust's `normalize_cow`: normalized input remains borrowed,
    /// while input containing CR is replaced by allocator-owned text. Allocation
    /// happens before any result is published, so failure leaves the input and
    /// caller state untouched.
    pub fn normalize(allocator: std.mem.Allocator, text: []const u8) std.mem.Allocator.Error!Normalized {
        if (!needsNormalization(text)) return .{ .borrowed = text };
        return .{ .owned = try normalizeOwned(allocator, text) };
    }

    /// Returns the input unchanged. This is useful when the caller already knows
    /// that `needsNormalization(text)` is false.
    pub fn normalizeBorrowed(text: []const u8) []const u8 {
        return text;
    }

    /// Produces owned normalized text using only the caller-provided allocator.
    pub fn normalizeOwned(allocator: std.mem.Allocator, text: []const u8) std.mem.Allocator.Error![]u8 {
        const result = try allocator.alloc(u8, normalizedLen(text));
        var write: usize = 0;
        var read: usize = 0;
        while (read < text.len) {
            if (text[read] == '\r') {
                result[write] = '\n';
                write += 1;
                read += 1;
                if (read < text.len and text[read] == '\n') read += 1;
            } else {
                result[write] = text[read];
                write += 1;
                read += 1;
            }
        }
        return result;
    }

    pub fn needsNormalization(text: []const u8) bool {
        return std.mem.indexOfScalar(u8, text, '\r') != null;
    }

    fn normalizedLen(text: []const u8) usize {
        var length = text.len;
        var index: usize = 1;
        while (index < text.len) : (index += 1) {
            if (text[index - 1] == '\r' and text[index] == '\n') length -= 1;
        }
        return length;
    }

    fn isUtf8Continuation(byte: u8) bool {
        return byte & 0xc0 == 0x80;
    }
};
