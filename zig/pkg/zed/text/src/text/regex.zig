const std = @import("std");

pub const Match = struct {
    start: usize,
    end: usize,
};

/// Engine-neutral borrowed regex adapter. The callback and context must outlive
/// each call; `text` never stores either value and never assumes regex syntax.
pub const RegexMatcher = struct {
    context: *anyopaque,
    find_fn: *const fn (context: *anyopaque, text: []const u8, start: usize) ?Match,

    pub fn find(self: RegexMatcher, text: []const u8, start: usize) ?Match {
        if (start > text.len) return null;
        const result = self.find_fn(self.context, text, start) orelse return null;
        if (result.start < start or result.start > result.end or result.end > text.len) return null;
        return result;
    }
};

pub const MatchIterator = struct {
    matcher: RegexMatcher,
    text: []const u8,
    offset: usize,
    done: bool = false,

    pub fn next(self: *MatchIterator) ?Match {
        if (self.done) return null;
        const result = self.matcher.find(self.text, self.offset) orelse {
            self.done = true;
            return null;
        };
        if (result.end > result.start) {
            self.offset = result.end;
        } else if (result.end < self.text.len) {
            self.offset = nextUtf8Boundary(self.text, result.end);
        } else {
            self.done = true;
        }
        return result;
    }

    fn nextUtf8Boundary(text: []const u8, offset: usize) usize {
        var result = offset + 1;
        while (result < text.len and text[result] & 0xc0 == 0x80) result += 1;
        return result;
    }
};

pub fn matches(matcher: RegexMatcher, text: []const u8, start: usize) MatchIterator {
    return .{ .matcher = matcher, .text = text, .offset = @min(start, text.len) };
}
