const std = @import("std");
const rope_mod = @import("rope.zig");
const chunk_mod = @import("chunk.zig");

const Rope = rope_mod.Rope;
const Chunk = chunk_mod.Chunk;
const ChunkTreeCursor = @TypeOf(@as(*const rope_mod.ChunkTree, undefined).cursor(rope_mod.Dimension(usize), {}));

pub const ChunkBitmaps = struct {
    text: []const u8,
    chars: chunk_mod.Bitmap,
    tabs: chunk_mod.Bitmap,
    newlines: chunk_mod.Bitmap,
};

pub const Cursor = struct {
    rope: *const Rope,
    chunks: ChunkTreeCursor,
    offset_value: usize,

    pub fn init(rope: *const Rope, initial_offset: usize) Cursor {
        std.debug.assert(initial_offset <= rope.len());
        var chunks = rope.chunks.cursor(rope_mod.Dimension(usize), {});
        _ = chunks.seek(rope_mod.ScalarTarget(usize), .{ .value = initial_offset }, .right);
        return .{ .rope = rope, .chunks = chunks, .offset_value = initial_offset };
    }

    pub fn offset(self: *const Cursor) usize {
        return self.offset_value;
    }

    pub fn seekForward(self: *Cursor, end_offset: usize) void {
        std.debug.assert(end_offset >= self.offset_value and end_offset <= self.rope.len());
        _ = self.chunks.seekForward(rope_mod.ScalarTarget(usize), .{ .value = end_offset }, .right);
        self.offset_value = end_offset;
    }

    pub fn slice(self: *Cursor, end_offset: usize) !Rope {
        std.debug.assert(end_offset >= self.offset_value and end_offset <= self.rope.len());
        const result = try self.rope.sliceBytes(.{ .start = self.offset_value, .end = end_offset });
        self.seekForward(end_offset);
        return result;
    }

    pub fn suffix(self: *Cursor) !Rope {
        return self.slice(self.rope.len());
    }

    pub fn summary(self: *Cursor, comptime ValueType: type, end_offset: usize) ValueType {
        std.debug.assert(end_offset >= self.offset_value and end_offset <= self.rope.len());
        const start = self.offset_value;
        const text = rangeSummary(self.rope, start, end_offset);
        self.seekForward(end_offset);
        if (ValueType == rope_mod.TextSummary) return text;
        if (ValueType == usize) return text.len;
        if (ValueType == rope_mod.OffsetUtf16) return text.len_utf16;
        if (ValueType == rope_mod.Point) return text.lines;
        if (ValueType == rope_mod.PointUtf16) return text.linesUtf16();
        @compileError("unsupported rope cursor summary dimension");
    }
};

pub const Chunks = struct {
    rope: *const Rope,
    cursor: ChunkTreeCursor,
    range: rope_mod.ByteRange,
    offset_value: usize,
    reversed: bool,

    pub fn init(rope: *const Rope, range: rope_mod.ByteRange, reversed: bool) Chunks {
        std.debug.assert(range.start <= range.end and range.end <= rope.len());
        std.debug.assert(rope.isCharBoundary(range.start) and rope.isCharBoundary(range.end));
        var cursor = rope.chunks.cursor(rope_mod.Dimension(usize), {});
        const initial_offset = if (reversed) range.end else range.start;
        _ = cursor.seek(rope_mod.ScalarTarget(usize), .{ .value = initial_offset }, if (reversed) .left else .right);
        return .{ .rope = rope, .cursor = cursor, .range = range, .offset_value = initial_offset, .reversed = reversed };
    }

    pub fn clone(self: Chunks) Chunks {
        return self;
    }

    pub fn offset(self: *const Chunks) usize {
        return self.offset_value;
    }

    pub fn seek(self: *Chunks, offset_value: usize) void {
        const target_offset = std.math.clamp(offset_value, self.range.start, self.range.end);
        _ = self.cursor.seek(rope_mod.ScalarTarget(usize), .{ .value = target_offset }, if (self.reversed) .left else .right);
        self.offset_value = target_offset;
    }

    pub fn setRange(self: *Chunks, range: rope_mod.ByteRange) void {
        std.debug.assert(range.start <= range.end and range.end <= self.rope.len());
        self.range = range;
        self.seek(if (self.reversed) range.end else range.start);
    }

    pub fn peek(self: *const Chunks) ?[]const u8 {
        if (!self.offsetValid()) return null;
        const item = self.cursor.item() orelse return null;
        const chunk_start = self.cursor.start().*;
        if (self.reversed) {
            const start = @max(chunk_start, self.range.start) - chunk_start;
            const end = self.offset_value - chunk_start;
            return item.text()[start..end];
        }
        const start = self.offset_value - chunk_start;
        const end = @min(self.cursor.end(), self.range.end) - chunk_start;
        return item.text()[start..end];
    }

    pub fn peekWithBitmaps(self: *const Chunks) ?ChunkBitmaps {
        const text = self.peek() orelse return null;
        const item = self.cursor.item().?;
        const chunk_start = self.cursor.start().*;
        const slice_start = if (self.reversed) @max(chunk_start, self.range.start) - chunk_start else self.offset_value - chunk_start;
        return .{
            .text = text,
            .chars = item.chars() >> @intCast(slice_start),
            .tabs = item.tabsBitmap() >> @intCast(slice_start),
            .newlines = item.newlines() >> @intCast(slice_start),
        };
    }

    pub fn next(self: *Chunks) ?[]const u8 {
        const result = self.peek() orelse return null;
        self.advance(result.len);
        return result;
    }

    pub fn nextWithBitmaps(self: *Chunks) ?ChunkBitmaps {
        const result = self.peekWithBitmaps() orelse return null;
        self.advance(result.text.len);
        return result;
    }

    pub fn nextLine(self: *Chunks) bool {
        std.debug.assert(!self.reversed);
        while (self.peek()) |text| {
            if (std.mem.indexOfScalar(u8, text, '\n')) |index| {
                self.seek(self.offset_value + index + 1);
                return self.offset_value <= self.range.end;
            }
            _ = self.next();
        }
        return false;
    }

    pub fn prevLine(self: *Chunks) bool {
        std.debug.assert(!self.reversed);
        if (self.offset_value == self.range.start) return false;
        var bytes = Bytes.init(self.rope, .{ .start = self.range.start, .end = self.offset_value }, true);
        var skipped_current_newline = false;
        var cursor_offset = self.offset_value;
        while (bytes.readByte()) |byte| {
            cursor_offset -= 1;
            if (!skipped_current_newline and byte == '\n' and cursor_offset + 1 == self.offset_value) {
                skipped_current_newline = true;
                continue;
            }
            skipped_current_newline = true;
            if (byte == '\n') {
                self.seek(cursor_offset + 1);
                return true;
            }
        }
        self.seek(self.range.start);
        return self.offset_value < cursor_offset or self.offset_value == 0;
    }

    pub fn lines(self: Chunks, allocator_value: std.mem.Allocator) Lines {
        return Lines.init(allocator_value, self);
    }

    fn offsetValid(self: *const Chunks) bool {
        return if (self.reversed)
            self.offset_value > self.range.start and self.offset_value <= self.range.end
        else
            self.offset_value >= self.range.start and self.offset_value < self.range.end;
    }

    fn advance(self: *Chunks, len: usize) void {
        if (self.reversed) {
            self.offset_value -= len;
            if (self.offset_value <= self.cursor.start().*) self.cursor.prev();
        } else {
            self.offset_value += len;
            if (self.offset_value >= self.cursor.end()) self.cursor.next();
        }
    }
};

pub const Bytes = struct {
    chunks: Chunks,

    pub fn init(rope: *const Rope, range: rope_mod.ByteRange, reversed: bool) Bytes {
        return .{ .chunks = Chunks.init(rope, range, reversed) };
    }

    pub fn peek(self: *const Bytes) ?[]const u8 {
        return self.chunks.peek();
    }

    pub fn next(self: *Bytes) ?[]const u8 {
        return self.chunks.next();
    }

    pub fn read(self: *Bytes, buffer: []u8) usize {
        if (buffer.len == 0) return 0;
        const source = self.peek() orelse return 0;
        const len = @min(buffer.len, source.len);
        if (self.chunks.reversed) {
            for (0..len) |index| buffer[index] = source[source.len - 1 - index];
        } else @memcpy(buffer[0..len], source[0..len]);
        self.chunks.advance(len);
        return len;
    }

    pub fn readByte(self: *Bytes) ?u8 {
        var byte: [1]u8 = undefined;
        return if (self.read(&byte) == 1) byte[0] else null;
    }
};

pub const Scalars = struct {
    bytes: Bytes,

    pub fn init(rope: *const Rope, range: rope_mod.ByteRange, reversed: bool) Scalars {
        return .{ .bytes = Bytes.init(rope, range, reversed) };
    }

    pub fn next(self: *Scalars) ?u21 {
        if (self.bytes.chunks.reversed) {
            var encoded: [4]u8 = undefined;
            var count: usize = 0;
            while (count < encoded.len) {
                const byte = self.bytes.readByte() orelse return null;
                encoded[encoded.len - 1 - count] = byte;
                count += 1;
                if ((byte & 0xc0) != 0x80) {
                    const slice = encoded[encoded.len - count ..];
                    return std.unicode.utf8Decode(slice) catch unreachable;
                }
            }
            unreachable;
        }
        const first = self.bytes.readByte() orelse return null;
        const len: usize = @intCast(std.unicode.utf8ByteSequenceLength(first) catch unreachable);
        var encoded: [4]u8 = undefined;
        encoded[0] = first;
        var index: usize = 1;
        while (index < len) : (index += 1) encoded[index] = self.bytes.readByte().?;
        return std.unicode.utf8Decode(encoded[0..len]) catch unreachable;
    }
};

pub const Lines = struct {
    allocator: std.mem.Allocator,
    chunks: Chunks,
    scratch: std.ArrayList(u8) = .empty,
    done: bool = false,

    pub fn init(allocator_value: std.mem.Allocator, chunks: Chunks) Lines {
        return .{ .allocator = allocator_value, .chunks = chunks };
    }

    pub fn deinit(self: *Lines) void {
        self.scratch.deinit(self.allocator);
        self.* = undefined;
    }

    pub fn offset(self: *const Lines) usize {
        return self.chunks.offset();
    }

    pub fn seek(self: *Lines, offset_value: usize) void {
        self.chunks.seek(offset_value);
        self.scratch.clearRetainingCapacity();
        self.done = false;
    }

    pub fn next(self: *Lines) !?[]const u8 {
        if (self.done) return null;
        self.scratch.clearRetainingCapacity();
        while (self.chunks.peek()) |text| {
            if (self.chunks.reversed) {
                if (std.mem.lastIndexOfScalar(u8, text, '\n')) |index| {
                    self.chunks.seek(self.chunks.offset_value - (text.len - index));
                    if (self.scratch.items.len == 0) return text[index + 1 ..];
                    try self.scratch.insertSlice(self.allocator, 0, text[index + 1 ..]);
                    return self.scratch.items;
                }
                try self.scratch.insertSlice(self.allocator, 0, text);
                _ = self.chunks.next();
            } else {
                if (std.mem.indexOfScalar(u8, text, '\n')) |index| {
                    self.chunks.seek(self.chunks.offset_value + index + 1);
                    if (self.scratch.items.len == 0) return text[0..index];
                    try self.scratch.appendSlice(self.allocator, text[0..index]);
                    return self.scratch.items;
                }
                try self.scratch.appendSlice(self.allocator, text);
                _ = self.chunks.next();
            }
        }
        self.done = true;
        return self.scratch.items;
    }
};

fn rangeSummary(rope: *const Rope, start: usize, end: usize) rope_mod.TextSummary {
    if (start == end) return .{};
    var chunks = Chunks.init(rope, .{ .start = start, .end = end }, false);
    var result = rope_mod.TextSummary.zero();
    while (chunks.next()) |text| result.addAssign(rope_mod.TextSummary.parse(text) catch unreachable);
    return result;
}
