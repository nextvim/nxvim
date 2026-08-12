#!/usr/bin/env python3
import random
import sys

seed = int(sys.argv[1]) if len(sys.argv) > 1 else 0
count = int(sys.argv[2]) if len(sys.argv) > 2 else 250
rng = random.Random(seed)
atoms = ["a", "\t", "\r", "\n", "é", "e\u0301", "क", "\u0600a", "🇺🇸", "👩‍💻", "👍🏽", "가", "가"]


def encoded(text):
    return text.encode("utf-8").hex() or "-"


def byte_boundaries(text):
    result = [0]
    total = 0
    for char in text:
        total += len(char.encode("utf-8"))
        result.append(total)
    return result


def rows(text):
    return text.split("\n")


def utf16_len(text):
    return len(text.encode("utf-16-le")) // 2


def emit_rope_cases(text, exhaustive):
    data = encoded(text)
    print("rope", data)
    boundaries = byte_boundaries(text)
    selected = boundaries if exhaustive else sorted({0, len(text.encode("utf-8")), rng.choice(boundaries)})
    for offset in selected:
        print("rope_byte", data, offset)
    for row, line in enumerate(rows(text)):
        columns = byte_boundaries(line)
        chosen = columns if exhaustive else sorted({0, len(line.encode("utf-8")), rng.choice(columns)})
        for column in chosen:
            print("rope_point", data, row, column)
        if len(line.encode("utf-8")) > 0:
            column = rng.randrange(len(line.encode("utf-8")) + 1)
            print("rope_clip", data, row, column, "left")
            print("rope_clip", data, row, column, "right")


def emit_chunk_cases(text, exhaustive):
    data = encoded(text)
    print("chunk", data)
    boundaries = byte_boundaries(text)
    selected = boundaries if exhaustive else sorted({0, len(text.encode("utf-8")), rng.choice(boundaries)})
    for offset in selected:
        print("chunk_byte", data, offset)

    points = []
    for row, line in enumerate(rows(text)):
        columns = byte_boundaries(line)
        chosen = columns if exhaustive else sorted({0, len(line.encode("utf-8")), rng.choice(columns)})
        points.extend((row, column) for column in chosen)
    for row, column in points:
        print("chunk_point", data, row, column)

    total_utf16 = utf16_len(text)
    utf16_offsets = range(total_utf16 + 1) if exhaustive else sorted({0, total_utf16, rng.randrange(total_utf16 + 1)})
    for offset in utf16_offsets:
        print("chunk_utf16", data, offset)

    line_values = rows(text)
    for row in range(len(line_values) + 2):
        line_utf16 = utf16_len(line_values[row]) if row < len(line_values) else 0
        columns = range(line_utf16 + 3) if exhaustive else sorted({0, line_utf16, line_utf16 + 1})
        for column in columns:
            print("chunk_point_utf16", data, row, column, 1)

    for row in range(len(line_values) + 2):
        line_bytes = len(line_values[row].encode("utf-8")) if row < len(line_values) else 0
        columns = range(line_bytes + 3) if exhaustive else sorted({0, line_bytes, line_bytes + 1})
        for column in columns:
            print("chunk_clip", data, row, column, "left")
            print("chunk_clip", data, row, column, "right")


print("emit")
fixed = [
    "",
    "abc",
    "a\tb\nc",
    "é😀z",
    "e\u0301x",
    "👩‍💻\n🇺🇸",
    "a" * 128,
    "😀" * 32,
]
for text in fixed:
    emit_chunk_cases(text, True)
    emit_rope_cases(text, True)

large = "ab😀\nβe\u0301\n👩‍💻 tail" * 20
emit_rope_cases(large, False)

for _ in range(count):
    text = ""
    while True:
        atom = rng.choice(atoms)
        if len((text + atom).encode("utf-8")) > 128:
            break
        text += atom
        if rng.randrange(5) == 0:
            break
    emit_chunk_cases(text, False)
    emit_rope_cases(text, False)
    data = text.encode("utf-8")
    print("grapheme", data.hex() or "-", rng.randrange(len(data) + 1))
