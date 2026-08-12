//! Zig port of Zed's foundational rope value, summary, Unicode, and chunk types.

pub const sum_tree = @import("sum_tree");
pub const grapheme = @import("unicode_grapheme.zig");

pub const Point = @import("point.zig").Point;
pub const PointUtf16 = @import("point_utf16.zig").PointUtf16;
pub const OffsetUtf16 = @import("offset_utf16.zig").OffsetUtf16;
pub const Unclipped = @import("unclipped.zig").Unclipped;
pub const TextSummary = @import("text_summary.zig").TextSummary;
pub const TextDimension = @import("text_summary.zig").TextDimension;
pub const DimensionPair = @import("text_summary.zig").DimensionPair;
pub const Chunk = @import("chunk.zig").Chunk;
pub const ChunkSlice = @import("chunk.zig").ChunkSlice;
pub const Tabs = @import("chunk.zig").Tabs;
pub const TabPosition = @import("chunk.zig").TabPosition;
pub const ChunkRange = @import("chunk.zig").Range;
pub const chunk = @import("chunk.zig");
pub const rope = @import("rope.zig");
pub const Rope = rope.Rope;
pub const ByteRange = rope.ByteRange;
pub const RowRange = rope.RowRange;
pub const ChunkSummary = rope.ChunkSummary;
pub const ChunkOps = rope.ChunkOps;
pub const ChunkTree = rope.ChunkTree;
pub const Dimension = rope.Dimension;
pub const ProductDimension = rope.ProductDimension;
pub const parallel_build_threshold = rope.parallel_build_threshold;
pub const Cursor = rope.iterators.Cursor;
pub const Chunks = rope.iterators.Chunks;
pub const Bytes = rope.iterators.Bytes;
pub const Scalars = rope.iterators.Scalars;
pub const Lines = rope.iterators.Lines;
pub const ChunkBitmaps = rope.iterators.ChunkBitmaps;

pub const baseline = struct {
    pub const zig = "0.16.0";
    pub const zed_revision = "90d024b88abc91264d9a0ad260eb4f365fa695c3";
    pub const unicode_segmentation_crate = "1.13.3";
};
