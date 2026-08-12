const std = @import("std");

/// Extended grapheme cluster boundaries from UAX #29 for Unicode 17.0.0.
pub const unicode_version = "17.0.0";

pub const Error = error{ InvalidUtf8, OffsetOutOfBounds };

const data = @import("unicode_grapheme_data.zig");
const Property = data.Property;

pub fn isBoundary(text: []const u8, offset: usize) Error!bool {
    if (offset > text.len) return error.OffsetOutOfBounds;
    try validate(text);
    if (offset == 0 or offset == text.len) return true;
    if (!isCodepointBoundary(text, offset)) return false;

    const right = try decodeAt(text, offset);
    const left_start = previousCodepointStart(text, offset);
    const left = try decodeAt(text, left_start);
    const a = property(left.codepoint);
    const b = property(right.codepoint);

    if (a == .cr and b == .lf) return false; // GB3
    if (isBreakControl(a) or isBreakControl(b)) return true; // GB4/GB5
    if (a == .l and (b == .l or b == .v or b == .lv or b == .lvt)) return false; // GB6
    if ((a == .lv or a == .v) and (b == .v or b == .t)) return false; // GB7
    if ((a == .lvt or a == .t) and b == .t) return false; // GB8
    if (b == .extend or b == .zwj) return false; // GB9
    if (b == .spacing_mark) return false; // GB9a
    if (a == .prepend) return false; // GB9b
    if (b == .incb_consonant and hasIndicConjunctBefore(text, left_start)) return false; // GB9c
    if (a == .zwj and b == .extended_pictographic and hasExtendedPictographicBeforeZwj(text, left_start)) return false; // GB11
    if (a == .regional_indicator and b == .regional_indicator and precedingRegionalIndicatorCount(text, left_start) % 2 == 1) return false; // GB12/13
    return true; // GB999
}

pub fn previousBoundary(text: []const u8, offset: usize) Error!usize {
    if (offset > text.len) return error.OffsetOutOfBounds;
    try validate(text);
    var cursor = offset;
    while (true) {
        if (try isBoundary(text, cursor)) return cursor;
        cursor -= 1;
    }
}

pub fn nextBoundary(text: []const u8, offset: usize) Error!usize {
    if (offset > text.len) return error.OffsetOutOfBounds;
    try validate(text);
    var cursor = offset;
    while (cursor <= text.len) : (cursor += 1) {
        if (try isBoundary(text, cursor)) return cursor;
    }
    unreachable;
}

fn validate(text: []const u8) Error!void {
    if (!std.unicode.utf8ValidateSlice(text)) return error.InvalidUtf8;
}

fn isCodepointBoundary(text: []const u8, offset: usize) bool {
    return offset == 0 or offset == text.len or (text[offset] & 0xc0) != 0x80;
}

const Decoded = struct { codepoint: u21, len: usize };

fn decodeAt(text: []const u8, offset: usize) Error!Decoded {
    const length = std.unicode.utf8ByteSequenceLength(text[offset]) catch return error.InvalidUtf8;
    const len: usize = @intCast(length);
    if (offset + len > text.len) return error.InvalidUtf8;
    const codepoint = std.unicode.utf8Decode(text[offset .. offset + len]) catch return error.InvalidUtf8;
    return .{ .codepoint = codepoint, .len = len };
}

fn previousCodepointStart(text: []const u8, offset: usize) usize {
    var start = offset - 1;
    while ((text[start] & 0xc0) == 0x80) start -= 1;
    return start;
}

fn hasIndicConjunctBefore(text: []const u8, left_start: usize) bool {
    var cursor = left_start;
    var saw_linker = false;
    while (true) {
        const decoded = decodeAt(text, cursor) catch return false;
        switch (property(decoded.codepoint)) {
            .extend, .zwj => {},
            .incb_consonant => return saw_linker,
            else => return false,
        }
        if (isIncbLinker(decoded.codepoint)) saw_linker = true;
        if (cursor == 0) return false;
        cursor = previousCodepointStart(text, cursor);
    }
}

fn isIncbLinker(codepoint: u21) bool {
    return switch (codepoint) {
        0x094d, 0x09cd, 0x0acd, 0x0b4d, 0x0c4d, 0x0d4d, 0x1039, 0x17d2, 0x1a60, 0x1b44, 0x1baa, 0xa9c0, 0xaaf6, 0x10a3f, 0x11133, 0x113d0, 0x1193e, 0x11a47, 0x11a99, 0x11f42 => true,
        else => false,
    };
}

fn hasExtendedPictographicBeforeZwj(text: []const u8, zwj_start: usize) bool {
    var cursor = zwj_start;
    while (cursor > 0) {
        cursor = previousCodepointStart(text, cursor);
        const decoded = decodeAt(text, cursor) catch return false;
        switch (property(decoded.codepoint)) {
            .extend => continue,
            .extended_pictographic => return true,
            else => return false,
        }
    }
    return false;
}

fn precedingRegionalIndicatorCount(text: []const u8, rightmost_start: usize) usize {
    var count: usize = 0;
    var cursor = rightmost_start;
    while (true) {
        const decoded = decodeAt(text, cursor) catch break;
        if (property(decoded.codepoint) != .regional_indicator) break;
        count += 1;
        if (cursor == 0) break;
        cursor = previousCodepointStart(text, cursor);
    }
    return count;
}

fn isBreakControl(value: Property) bool {
    return value == .control or value == .cr or value == .lf;
}

fn property(codepoint: u21) Property {
    var low: usize = 0;
    var high: usize = data.ranges.len;
    while (low < high) {
        const mid = low + (high - low) / 2;
        const range = data.ranges[mid];
        if (codepoint < range.first) high = mid else if (codepoint > range.last) low = mid + 1 else return range.property;
    }
    return .other;
}
