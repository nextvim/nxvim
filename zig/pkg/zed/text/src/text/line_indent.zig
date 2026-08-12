pub const LineIndent = struct {
    tabs: u32,
    spaces: u32,
    line_blank: bool,

    /// Parses indentation exactly like Rust's `From<&str>` implementation.
    pub fn parse(text: []const u8) LineIndent {
        var result = LineIndent{ .tabs = 0, .spaces = 0, .line_blank = true };
        for (text) |byte| {
            switch (byte) {
                '\t' => result.tabs +|= 1,
                ' ' => result.spaces +|= 1,
                '\n' => break,
                else => {
                    result.line_blank = false;
                    break;
                },
            }
        }
        return result;
    }

    pub fn onlySpaces(count: u32) LineIndent {
        return .{ .tabs = 0, .spaces = count, .line_blank = true };
    }

    pub fn onlyTabs(count: u32) LineIndent {
        return .{ .tabs = count, .spaces = 0, .line_blank = true };
    }

    pub fn isLineEmpty(self: LineIndent) bool {
        return self.tabs == 0 and self.spaces == 0 and self.line_blank;
    }

    pub fn isLineBlank(self: LineIndent) bool {
        return self.line_blank;
    }

    pub fn rawLen(self: LineIndent) u32 {
        return self.tabs +| self.spaces;
    }

    /// Rust counts each tab as exactly `tab_size` columns (not tab-stop alignment).
    pub fn len(self: LineIndent, tab_size: u32) u32 {
        return (self.tabs *| tab_size) +| self.spaces;
    }
};
