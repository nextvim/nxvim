const std = @import("std");
const grapheme = @import("unicode_grapheme.zig");
const sum_tree = @import("sum_tree");

pub const Point = @import("point.zig").Point;
pub const PointUtf16 = @import("point_utf16.zig").PointUtf16;
pub const OffsetUtf16 = @import("offset_utf16.zig").OffsetUtf16;
pub const Unclipped = @import("unclipped.zig").Unclipped;
pub const TextSummary = @import("text_summary.zig").TextSummary;

pub const Bitmap = u128;
pub const max_base: usize = @bitSizeOf(Bitmap);
pub const min_base: usize = max_base / 2;
pub const MAX_BASE = max_base;
pub const MIN_BASE = min_base;

pub const Error = error{
    InvalidUtf8,
    ChunkTooLong,
    CapacityExceeded,
    OffsetOutOfBounds,
    NotCodepointBoundary,
    InvalidRange,
    PointOutOfBounds,
    PointInsideCodepoint,
};

pub const Bias = sum_tree.Bias;

pub const Range = struct {
    start: usize,
    end: usize,
};

pub const Chunk = struct {
    chars_bitmap: Bitmap = 0,
    chars_utf16_bitmap: Bitmap = 0,
    newlines_bitmap: Bitmap = 0,
    tabs_bitmap: Bitmap = 0,
    text_buffer: [max_base]u8 = undefined,
    text_len: u8 = 0,

    pub fn init(bytes: []const u8) Error!Chunk {
        if (bytes.len > max_base) return error.ChunkTooLong;
        if (!std.unicode.utf8ValidateSlice(bytes)) return error.InvalidUtf8;

        var result = Chunk{};
        result.text_len = @intCast(bytes.len);
        @memcpy(result.text_buffer[0..bytes.len], bytes);
        result.rebuildBitmaps();
        return result;
    }

    pub fn new(bytes: []const u8) Error!Chunk {
        return init(bytes);
    }

    pub fn empty() Chunk {
        return .{};
    }

    pub fn text(self: *const Chunk) []const u8 {
        return self.text_buffer[0..self.text_len];
    }

    pub fn len(self: *const Chunk) usize {
        return self.text_len;
    }

    pub fn isEmpty(self: *const Chunk) bool {
        return self.text_len == 0;
    }

    pub fn chars(self: *const Chunk) Bitmap {
        return self.chars_bitmap;
    }

    pub fn newlines(self: *const Chunk) Bitmap {
        return self.newlines_bitmap;
    }

    pub fn tabsBitmap(self: *const Chunk) Bitmap {
        return self.tabs_bitmap;
    }

    pub fn asSlice(self: *const Chunk) ChunkSlice {
        return .{
            .chars_bitmap = self.chars_bitmap,
            .chars_utf16_bitmap = self.chars_utf16_bitmap,
            .newlines_bitmap = self.newlines_bitmap,
            .tabs_bitmap = self.tabs_bitmap,
            .text = self.text(),
        };
    }

    pub fn slice(self: *const Chunk, range: Range) Error!ChunkSlice {
        return self.asSlice().slice(range);
    }

    pub fn splitAt(self: *const Chunk, mid: usize) Error!Split {
        return self.asSlice().splitAt(mid);
    }

    pub fn isCharBoundary(self: *const Chunk, offset: usize) bool {
        return self.asSlice().isCharBoundary(offset);
    }

    pub fn floorCharBoundary(self: *const Chunk, index: usize) usize {
        return self.asSlice().floorCharBoundary(index);
    }

    pub fn pushStr(self: *Chunk, value: []const u8) Error!void {
        const other = try Chunk.init(value);
        try self.append(other.asSlice());
    }

    pub fn prependStr(self: *Chunk, value: []const u8) Error!void {
        const other = try Chunk.init(value);
        try self.prepend(other.asSlice());
    }

    pub fn append(self: *Chunk, value: ChunkSlice) Error!void {
        try value.checkInvariants();
        if (self.len() + value.len() > max_base) return error.CapacityExceeded;
        if (value.len() == 0) return;
        @memcpy(self.text_buffer[self.len() .. self.len() + value.len()], value.text);
        self.text_len = @intCast(self.len() + value.len());
        self.rebuildBitmaps();
    }

    pub fn prepend(self: *Chunk, value: ChunkSlice) Error!void {
        try value.checkInvariants();
        const old_len = self.len();
        if (old_len + value.len() > max_base) return error.CapacityExceeded;
        if (value.len() == 0) return;
        std.mem.copyBackwards(u8, self.text_buffer[value.len() .. value.len() + old_len], self.text_buffer[0..old_len]);
        @memcpy(self.text_buffer[0..value.len()], value.text);
        self.text_len = @intCast(old_len + value.len());
        self.rebuildBitmaps();
    }

    pub fn checkInvariants(self: *const Chunk) Error!void {
        if (self.len() > max_base) return error.ChunkTooLong;
        try self.asSlice().checkInvariants();
    }

    fn rebuildBitmaps(self: *Chunk) void {
        self.chars_bitmap = 0;
        self.chars_utf16_bitmap = 0;
        self.newlines_bitmap = 0;
        self.tabs_bitmap = 0;
        for (self.text(), 0..) |byte, index| {
            const bit = bitAt(index);
            if (isCodepointStart(byte)) {
                self.chars_bitmap |= bit;
                self.chars_utf16_bitmap |= bit;
                if (byte >= 0xf0 and index + 1 < max_base) self.chars_utf16_bitmap |= bitAt(index + 1);
            }
            if (byte == '\n') self.newlines_bitmap |= bit;
            if (byte == '\t') self.tabs_bitmap |= bit;
        }
    }
};

pub const Split = struct { left: ChunkSlice, right: ChunkSlice };
pub const LongestRow = struct { row: u32, chars: u32, total_chars: usize };

pub const ChunkSlice = struct {
    chars_bitmap: Bitmap,
    chars_utf16_bitmap: Bitmap,
    newlines_bitmap: Bitmap,
    tabs_bitmap: Bitmap,
    text: []const u8,

    pub fn init(text: []const u8) Error!ChunkSlice {
        const owned = try Chunk.init(text);
        _ = owned;
        // A slice cannot safely borrow a temporary Chunk. Construct masks directly.
        var char_starts: Bitmap = 0;
        var utf16: Bitmap = 0;
        var newline_bits: Bitmap = 0;
        var tab_bits: Bitmap = 0;
        for (text, 0..) |byte, index| {
            const bit = bitAt(index);
            if (isCodepointStart(byte)) {
                char_starts |= bit;
                utf16 |= bit;
                if (byte >= 0xf0 and index + 1 < max_base) utf16 |= bitAt(index + 1);
            }
            if (byte == '\n') newline_bits |= bit;
            if (byte == '\t') tab_bits |= bit;
        }
        return .{ .chars_bitmap = char_starts, .chars_utf16_bitmap = utf16, .newlines_bitmap = newline_bits, .tabs_bitmap = tab_bits, .text = text };
    }

    pub fn toChunk(self: ChunkSlice) Error!Chunk {
        return Chunk.init(self.text);
    }

    pub fn len(self: ChunkSlice) usize {
        return self.text.len;
    }

    pub fn isEmpty(self: ChunkSlice) bool {
        return self.text.len == 0;
    }

    pub fn chars(self: ChunkSlice) Bitmap {
        return self.chars_bitmap;
    }

    pub fn newlines(self: ChunkSlice) Bitmap {
        return self.newlines_bitmap;
    }

    pub fn tabsBitmap(self: ChunkSlice) Bitmap {
        return self.tabs_bitmap;
    }

    pub fn isCharBoundary(self: ChunkSlice, offset: usize) bool {
        if (offset > self.len()) return false;
        return offset == self.len() or (self.chars_bitmap & bitAt(offset)) != 0;
    }

    pub fn floorCharBoundary(self: ChunkSlice, index: usize) usize {
        var cursor = @min(index, self.len());
        while (cursor > 0 and !self.isCharBoundary(cursor)) cursor -= 1;
        return cursor;
    }

    pub fn ceilCharBoundary(self: ChunkSlice, index: usize) usize {
        var cursor = @min(index, self.len());
        while (cursor < self.len() and !self.isCharBoundary(cursor)) cursor += 1;
        return cursor;
    }

    pub fn splitAt(self: ChunkSlice, mid: usize) Error!Split {
        if (mid > self.len()) return error.OffsetOutOfBounds;
        if (!self.isCharBoundary(mid)) return error.NotCodepointBoundary;
        return .{ .left = try self.slice(.{ .start = 0, .end = mid }), .right = try self.slice(.{ .start = mid, .end = self.len() }) };
    }

    pub fn slice(self: ChunkSlice, range: Range) Error!ChunkSlice {
        if (range.start > range.end) return error.InvalidRange;
        if (range.end > self.len()) return error.OffsetOutOfBounds;
        if (!self.isCharBoundary(range.start) or !self.isCharBoundary(range.end)) return error.NotCodepointBoundary;
        const width = range.end - range.start;
        return .{
            .chars_bitmap = shiftRange(self.chars_bitmap, range.start, width),
            .chars_utf16_bitmap = shiftRange(self.chars_utf16_bitmap, range.start, width),
            .newlines_bitmap = shiftRange(self.newlines_bitmap, range.start, width),
            .tabs_bitmap = shiftRange(self.tabs_bitmap, range.start, width),
            .text = self.text[range.start..range.end],
        };
    }

    pub fn lenUtf16(self: ChunkSlice) OffsetUtf16 {
        return .{ .value = @popCount(self.chars_utf16_bitmap) };
    }

    pub fn lines(self: ChunkSlice) Point {
        const row: u32 = @intCast(@popCount(self.newlines_bitmap));
        const start = self.lastRowStart();
        return .{ .row = row, .column = @intCast(self.len() - start) };
    }

    pub fn firstLineChars(self: ChunkSlice) u32 {
        const end = self.firstNewline() orelse self.len();
        return @intCast(@popCount(self.chars_bitmap & lowMask(end)));
    }

    pub fn lastLineChars(self: ChunkSlice) u32 {
        const start = self.lastRowStart();
        return @intCast(@popCount(self.chars_bitmap & highMask(start)));
    }

    pub fn lastLineLenUtf16(self: ChunkSlice) u32 {
        const start = self.lastRowStart();
        return @intCast(@popCount(self.chars_utf16_bitmap & highMask(start)));
    }

    pub fn longestRow(self: ChunkSlice) LongestRow {
        var row: u32 = 0;
        var row_chars: u32 = 0;
        var best_row: u32 = 0;
        var best_chars: u32 = 0;
        var total: usize = 0;
        for (self.text) |byte| {
            if (isCodepointStart(byte)) {
                total += 1;
                if (byte == '\n') {
                    if (row_chars > best_chars) {
                        best_row = row;
                        best_chars = row_chars;
                    }
                    row += 1;
                    row_chars = 0;
                } else row_chars += 1;
            }
        }
        if (row_chars > best_chars) {
            best_row = row;
            best_chars = row_chars;
        }
        return .{ .row = best_row, .chars = best_chars, .total_chars = total };
    }

    pub fn textSummary(self: ChunkSlice) TextSummary {
        const longest = self.longestRow();
        return .{
            .len = self.len(),
            .chars = longest.total_chars,
            .len_utf16 = self.lenUtf16(),
            .lines = self.lines(),
            .first_line_chars = self.firstLineChars(),
            .last_line_chars = self.lastLineChars(),
            .last_line_len_utf16 = self.lastLineLenUtf16(),
            .longest_row = longest.row,
            .longest_row_chars = longest.chars,
        };
    }

    pub fn offsetToPoint(self: ChunkSlice, offset: usize) Error!Point {
        if (offset > self.len()) return error.OffsetOutOfBounds;
        if (!self.isCharBoundary(offset)) return error.NotCodepointBoundary;
        const before = lowMask(offset);
        const row: u32 = @intCast(@popCount(self.newlines_bitmap & before));
        const preceding_newlines = self.newlines_bitmap & before;
        const row_start = if (preceding_newlines == 0) 0 else lastSetBit(preceding_newlines) + 1;
        return .{ .row = row, .column = @intCast(offset - row_start) };
    }

    pub fn pointToOffset(self: ChunkSlice, point: Point) Error!usize {
        const range = try self.offsetRangeForRow(point.row);
        if (point.column > range.end - range.start) return error.PointOutOfBounds;
        const result = range.start + point.column;
        if (!self.isCharBoundary(result)) return error.PointInsideCodepoint;
        return result;
    }

    pub fn offsetToOffsetUtf16(self: ChunkSlice, offset: usize) Error!OffsetUtf16 {
        if (offset > self.len()) return error.OffsetOutOfBounds;
        if (!self.isCharBoundary(offset)) return error.NotCodepointBoundary;
        return .{ .value = @popCount(self.chars_utf16_bitmap & lowMask(offset)) };
    }

    pub fn offsetUtf16ToOffset(self: ChunkSlice, target: OffsetUtf16) Error!usize {
        if (target.value > self.lenUtf16().value) return error.OffsetOutOfBounds;
        if (target.value == 0) return 0;
        if (target.value == self.lenUtf16().value) return self.len();
        const marker = nthSetBit(self.chars_utf16_bitmap, target.value);
        var offset = marker + 1;
        if (offset < self.len() and (self.chars_utf16_bitmap & bitAt(offset)) == 0) {
            while (offset < self.len() and !self.isCharBoundary(offset)) offset += 1;
        }
        return offset;
    }

    pub fn offsetToPointUtf16(self: ChunkSlice, offset: usize) Error!PointUtf16 {
        const point = try self.offsetToPoint(offset);
        const row_range = try self.offsetRangeForRow(point.row);
        const prefix = try self.offsetToOffsetUtf16(offset);
        const row_prefix = try self.offsetToOffsetUtf16(row_range.start);
        return .{ .row = point.row, .column = @intCast(prefix.value - row_prefix.value) };
    }

    pub fn pointToPointUtf16(self: ChunkSlice, point: Point) Error!PointUtf16 {
        return self.offsetToPointUtf16(try self.pointToOffset(point));
    }

    pub fn pointUtf16ToOffset(self: ChunkSlice, point: PointUtf16, clip: bool) Error!usize {
        const extent = self.lines();
        if (point.row > extent.row) {
            if (clip) return self.len();
            return error.PointOutOfBounds;
        }
        const range = try self.offsetRangeForRow(point.row);
        const line = try self.slice(range);
        const target = OffsetUtf16.new(point.column);
        if (target.value > line.lenUtf16().value) {
            if (clip) return range.end;
            return error.PointOutOfBounds;
        }
        const relative = try line.offsetUtf16ToOffset(target);
        const absolute = range.start + relative;
        if (!self.isCharBoundary(absolute)) {
            if (!clip) return error.PointInsideCodepoint;
            return self.floorCharBoundary(absolute);
        }
        return absolute;
    }

    pub fn unclippedPointUtf16ToPoint(self: ChunkSlice, point: Unclipped(PointUtf16)) Point {
        const extent = self.lines();
        if (point.value.row > extent.row) return extent;
        const range = self.offsetRangeForRow(point.value.row) catch return extent;
        const line = self.slice(range) catch return extent;
        if (point.value.column == 0) return .{ .row = point.value.row, .column = 0 };
        if (point.value.column >= line.lenUtf16().value) return .{ .row = point.value.row, .column = @intCast(line.len()) };
        var column = line.offsetUtf16ToOffset(.{ .value = point.value.column }) catch line.len();
        column = line.floorCharBoundary(column);
        return .{ .row = point.value.row, .column = @intCast(column) };
    }

    pub fn clipPoint(self: ChunkSlice, point: Point, bias: Bias) Point {
        const extent = self.lines();
        if (point.row > extent.row) return extent;
        const range = self.offsetRangeForRow(point.row) catch return extent;
        const line = self.text[range.start..range.end];
        if (point.column == 0) return point;
        if (point.column >= line.len) return .{ .row = point.row, .column = @intCast(line.len) };
        const column: usize = point.column;
        const clipped = switch (bias) {
            .left => grapheme.previousBoundary(line, column) catch self.floorBoundaryIn(line, column),
            .right => grapheme.nextBoundary(line, column) catch self.ceilBoundaryIn(line, column),
        };
        return .{ .row = point.row, .column = @intCast(clipped) };
    }

    pub fn clipPointUtf16(self: ChunkSlice, point: Unclipped(PointUtf16), bias: Bias) PointUtf16 {
        const extent = self.lines();
        if (point.value.row > extent.row) return .{ .row = extent.row, .column = self.lastLineLenUtf16() };
        const range = self.offsetRangeForRow(point.value.row) catch return .{ .row = extent.row, .column = self.lastLineLenUtf16() };
        const line = self.slice(range) catch unreachable;
        const column = line.clipOffsetUtf16(.{ .value = point.value.column }, bias);
        return .{ .row = point.value.row, .column = @intCast(column.value) };
    }

    pub fn clipOffsetUtf16(self: ChunkSlice, target: OffsetUtf16, bias: Bias) OffsetUtf16 {
        const extent = self.lenUtf16();
        if (target.value == 0) return .{};
        if (target.value >= extent.value) return extent;
        var offset = self.offsetUtf16ToOffset(target) catch return extent;
        while (!self.isCharBoundary(offset)) {
            switch (bias) {
                .left => offset -= 1,
                .right => offset += 1,
            }
        }
        return self.offsetToOffsetUtf16(offset) catch extent;
    }

    pub fn offsetRangeForRow(self: ChunkSlice, row: u32) Error!Range {
        const row_count: u32 = @intCast(@popCount(self.newlines_bitmap));
        if (row > row_count) return error.PointOutOfBounds;
        const start = if (row == 0) 0 else nthSetBit(self.newlines_bitmap, row) + 1;
        const remaining = self.newlines_bitmap & highMask(start);
        const end = if (remaining == 0) self.len() else @min(firstSetBit(remaining), self.len());
        return .{ .start = start, .end = end };
    }

    pub fn tabs(self: ChunkSlice) Tabs {
        return .{ .remaining = self.tabs_bitmap, .chars_bitmap = self.chars_bitmap };
    }

    pub fn checkInvariants(self: ChunkSlice) Error!void {
        if (self.len() > max_base) return error.ChunkTooLong;
        if (!std.unicode.utf8ValidateSlice(self.text)) return error.InvalidUtf8;
        const expected = try ChunkSlice.init(self.text);
        if (expected.chars_bitmap != self.chars_bitmap or expected.chars_utf16_bitmap != self.chars_utf16_bitmap or expected.newlines_bitmap != self.newlines_bitmap or expected.tabs_bitmap != self.tabs_bitmap) return error.InvalidUtf8;
        const used = lowMask(self.len());
        if ((self.chars_bitmap | self.chars_utf16_bitmap | self.newlines_bitmap | self.tabs_bitmap) & ~used != 0) return error.InvalidUtf8;
    }

    fn firstNewline(self: ChunkSlice) ?usize {
        if (self.newlines_bitmap == 0) return null;
        return firstSetBit(self.newlines_bitmap);
    }

    fn lastRowStart(self: ChunkSlice) usize {
        if (self.newlines_bitmap == 0) return 0;
        return lastSetBit(self.newlines_bitmap) + 1;
    }

    fn floorBoundaryIn(_: ChunkSlice, text: []const u8, index: usize) usize {
        var cursor = @min(index, text.len);
        while (cursor > 0 and cursor < text.len and !isCodepointStart(text[cursor])) cursor -= 1;
        return cursor;
    }

    fn ceilBoundaryIn(_: ChunkSlice, text: []const u8, index: usize) usize {
        var cursor = @min(index, text.len);
        while (cursor < text.len and !isCodepointStart(text[cursor])) cursor += 1;
        return cursor;
    }
};

pub const TabPosition = struct { byte_offset: usize, char_offset: usize };

pub const Tabs = struct {
    remaining: Bitmap,
    chars_bitmap: Bitmap,

    pub fn next(self: *Tabs) ?TabPosition {
        if (self.remaining == 0) return null;
        const byte_offset = firstSetBit(self.remaining);
        self.remaining &= ~bitAt(byte_offset);
        return .{ .byte_offset = byte_offset, .char_offset = @popCount(self.chars_bitmap & lowMask(byte_offset)) };
    }
};

fn isCodepointStart(byte: u8) bool {
    return (byte & 0xc0) != 0x80;
}

fn bitAt(index: usize) Bitmap {
    if (index >= max_base) return 0;
    return @as(Bitmap, 1) << @intCast(index);
}

fn lowMask(width: usize) Bitmap {
    if (width == 0) return 0;
    if (width >= max_base) return std.math.maxInt(Bitmap);
    return bitAt(width) - 1;
}

fn highMask(start: usize) Bitmap {
    return ~lowMask(start);
}

fn shiftRange(value: Bitmap, start: usize, width: usize) Bitmap {
    if (start >= max_base or width == 0) return 0;
    return (value >> @intCast(start)) & lowMask(width);
}

fn firstSetBit(value: Bitmap) usize {
    std.debug.assert(value != 0);
    return @intCast(@ctz(value));
}

fn lastSetBit(value: Bitmap) usize {
    std.debug.assert(value != 0);
    return max_base - 1 - @as(usize, @intCast(@clz(value)));
}

/// One-based: `n == 1` returns the first set bit. With `n == popCount(value) + 1`
/// this returns 128, which is useful as the logical end of a full chunk.
fn nthSetBit(value: Bitmap, n: usize) usize {
    if (n == 0) return 0;
    var remaining = value;
    var count = n;
    while (remaining != 0) {
        const index = firstSetBit(remaining);
        count -= 1;
        if (count == 0) return index;
        remaining &= remaining - 1;
    }
    return max_base;
}
