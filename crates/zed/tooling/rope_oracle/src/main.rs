use rope::{Chunk, OffsetUtf16, Point, PointUtf16, Rope};
use std::io::{self, BufRead};
use sum_tree::Bias;
use unicode_segmentation::UnicodeSegmentation;

fn main() {
    for (line_number, line) in io::stdin().lock().lines().enumerate() {
        let line = line.unwrap_or_else(|error| fail(line_number, &error.to_string()));
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<_> = line.split_ascii_whitespace().collect();
        match fields.as_slice() {
            ["grapheme", encoded, offset] => {
                let text = text(encoded, line_number);
                let offset = usize_field(offset, line_number, "offset");
                if offset > text.len() {
                    fail(line_number, "offset past end of text");
                }
                let mut boundaries: Vec<_> = text.grapheme_indices(true).map(|(i, _)| i).collect();
                boundaries.push(text.len());
                boundaries.sort_unstable();
                boundaries.dedup();
                let previous = boundaries
                    .iter()
                    .copied()
                    .take_while(|i| *i <= offset)
                    .last()
                    .unwrap_or(0);
                let next = boundaries
                    .iter()
                    .copied()
                    .find(|i| *i >= offset)
                    .unwrap_or(text.len());
                println!(
                    "grapheme {offset} {} {previous} {next}",
                    usize::from(previous == offset && next == offset)
                );
            }
            ["chunk", encoded] => {
                let text = text(encoded, line_number);
                let chunk = chunk(&text, line_number);
                let slice = chunk.as_slice();
                let summary = slice.text_summary();
                let (chars, chars_utf16, newlines, tabs) = canonical_bitmaps(&text);
                println!(
                    "chunk {} {} {} {} {} {} {} {} {} {} {} {} {} {}",
                    summary.len,
                    summary.chars,
                    summary.len_utf16.0,
                    summary.lines.row,
                    summary.lines.column,
                    summary.first_line_chars,
                    summary.last_line_chars,
                    summary.last_line_len_utf16,
                    summary.longest_row,
                    summary.longest_row_chars,
                    chars,
                    chars_utf16,
                    newlines,
                    tabs,
                );
            }
            ["chunk_byte", encoded, offset] => {
                let text = text(encoded, line_number);
                let offset = usize_field(offset, line_number, "offset");
                require_boundary(&text, offset, line_number);
                let slice = chunk(&text, line_number);
                let slice = slice.as_slice();
                let point = slice.offset_to_point(offset);
                let utf16 = slice.offset_to_offset_utf16(offset);
                let point_utf16 = slice.offset_to_point_utf16(offset);
                println!(
                    "chunk_byte {offset} {} {} {} {} {}",
                    point.row, point.column, utf16.0, point_utf16.row, point_utf16.column
                );
            }
            ["chunk_point", encoded, row, column] => {
                let text = text(encoded, line_number);
                let row = u32_field(row, line_number, "row");
                let column = u32_field(column, line_number, "column");
                let slice = chunk(&text, line_number);
                let slice = slice.as_slice();
                let point = Point::new(row, column);
                let offset = slice.point_to_offset(point);
                require_boundary(&text, offset, line_number);
                let utf16 = slice.point_to_point_utf16(point);
                println!(
                    "chunk_point {row} {column} {offset} {} {}",
                    utf16.row, utf16.column
                );
            }
            ["chunk_utf16", encoded, offset] => {
                let text = text(encoded, line_number);
                let offset = usize_field(offset, line_number, "UTF-16 offset");
                let slice = chunk(&text, line_number);
                let byte = slice.as_slice().offset_utf16_to_offset(OffsetUtf16(offset));
                println!("chunk_utf16 {offset} {byte}");
            }
            ["chunk_point_utf16", encoded, row, column, clip] => {
                let text = text(encoded, line_number);
                let row = u32_field(row, line_number, "row");
                let column = u32_field(column, line_number, "column");
                let clip = bool_field(clip, line_number);
                let slice = chunk(&text, line_number);
                let byte = slice
                    .as_slice()
                    .point_utf16_to_offset(PointUtf16::new(row, column), clip);
                println!(
                    "chunk_point_utf16 {row} {column} {} {byte}",
                    usize::from(clip)
                );
            }
            ["chunk_clip", encoded, row, column, bias] => {
                let text = text(encoded, line_number);
                let row = u32_field(row, line_number, "row");
                let column = u32_field(column, line_number, "column");
                let bias = bias_field(bias, line_number);
                let slice = chunk(&text, line_number);
                let point = slice.as_slice().clip_point(Point::new(row, column), bias);
                println!(
                    "chunk_clip {row} {column} {} {} {}",
                    bias_name(bias),
                    point.row,
                    point.column
                );
            }
            ["rope", encoded] => {
                let text = text(encoded, line_number);
                let rope = Rope::from(text.as_str());
                let summary = rope.summary();
                println!(
                    "rope {} {} {} {} {} {} {} {} {} {} {}",
                    rope.len(),
                    summary.chars,
                    summary.len_utf16.0,
                    summary.lines.row,
                    summary.lines.column,
                    summary.first_line_chars,
                    summary.last_line_chars,
                    summary.last_line_len_utf16,
                    summary.longest_row,
                    summary.longest_row_chars,
                    hex(rope.to_string().as_bytes())
                );
            }
            ["rope_byte", encoded, offset] => {
                let text = text(encoded, line_number);
                let offset = usize_field(offset, line_number, "offset");
                let rope = Rope::from(text.as_str());
                let point = rope.offset_to_point(offset);
                let point16 = rope.offset_to_point_utf16(offset);
                println!(
                    "rope_byte {offset} {} {} {} {} {} {}",
                    usize::from(rope.is_char_boundary(offset)),
                    rope.offset_to_offset_utf16(offset).0,
                    point.row,
                    point.column,
                    point16.row,
                    point16.column
                );
            }
            ["rope_point", encoded, row, column] => {
                let text = text(encoded, line_number);
                let row = u32_field(row, line_number, "row");
                let column = u32_field(column, line_number, "column");
                let rope = Rope::from(text.as_str());
                let point = Point::new(row, column);
                let point16 = rope.point_to_point_utf16(point);
                println!(
                    "rope_point {row} {column} {} {} {}",
                    rope.point_to_offset(point),
                    point16.row,
                    point16.column
                );
            }
            ["rope_clip", encoded, row, column, bias] => {
                let text = text(encoded, line_number);
                let row = u32_field(row, line_number, "row");
                let column = u32_field(column, line_number, "column");
                let bias = bias_field(bias, line_number);
                let rope = Rope::from(text.as_str());
                let point = rope.clip_point(Point::new(row, column), bias);
                println!(
                    "rope_clip {row} {column} {} {} {}",
                    bias_name(bias),
                    point.row,
                    point.column
                );
            }
            ["emit"] => println!("state phase=4 unicode-segmentation=1.13.3 chunk-max=128"),
            _ => fail(line_number, "unknown operation or wrong field count"),
        }
    }
}

fn chunk<'a>(text: &'a str, line_number: usize) -> Chunk {
    if text.len() > Chunk::MASK_BITS {
        fail(line_number, "chunk text exceeds 128 bytes");
    }
    Chunk::new(text)
}

fn canonical_bitmaps(text: &str) -> (u128, u128, u128, u128) {
    let mut chars = 0u128;
    let mut chars_utf16 = 0u128;
    let mut newlines = 0u128;
    let mut tabs = 0u128;
    for (index, byte) in text.bytes().enumerate() {
        let bit = 1u128 << index;
        if byte & 0xc0 != 0x80 {
            chars |= bit;
            chars_utf16 |= bit;
            if byte >= 0xf0 {
                chars_utf16 |= bit << 1;
            }
        }
        if byte == b'\n' {
            newlines |= bit;
        }
        if byte == b'\t' {
            tabs |= bit;
        }
    }
    (chars, chars_utf16, newlines, tabs)
}

fn text(value: &str, line_number: usize) -> String {
    let bytes = decode(value).unwrap_or_else(|error| fail(line_number, &error));
    String::from_utf8(bytes).unwrap_or_else(|error| fail(line_number, &error.to_string()))
}

fn require_boundary(text: &str, offset: usize, line_number: usize) {
    if offset > text.len() || !text.is_char_boundary(offset) {
        fail(line_number, "offset is not a UTF-8 boundary");
    }
}

fn usize_field(value: &str, line_number: usize, name: &str) -> usize {
    value
        .parse()
        .unwrap_or_else(|_| fail(line_number, &format!("invalid {name}")))
}

fn u32_field(value: &str, line_number: usize, name: &str) -> u32 {
    value
        .parse()
        .unwrap_or_else(|_| fail(line_number, &format!("invalid {name}")))
}

fn bool_field(value: &str, line_number: usize) -> bool {
    match value {
        "0" => false,
        "1" => true,
        _ => fail(line_number, "invalid boolean"),
    }
}

fn bias_field(value: &str, line_number: usize) -> Bias {
    match value {
        "left" => Bias::Left,
        "right" => Bias::Right,
        _ => fail(line_number, "invalid bias"),
    }
}

fn bias_name(bias: Bias) -> &'static str {
    match bias {
        Bias::Left => "left",
        Bias::Right => "right",
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode(value: &str) -> Result<Vec<u8>, String> {
    if value == "-" {
        return Ok(Vec::new());
    }
    if value.len() % 2 != 0 {
        return Err("hex text has odd length".into());
    }
    (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).map_err(|_| "invalid hex text".into()))
        .collect()
}

fn fail(line_number: usize, message: &str) -> ! {
    eprintln!("trace line {}: {message}", line_number + 1);
    std::process::exit(2)
}
