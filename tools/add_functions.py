#!/usr/bin/env python3

import json
import re
import sys


FUNCTION_TEMPLATE = """pub fn {func}(editor: &mut Editor, window: WindowId, select: bool) -> Outcome {{
    super::marks_and_jumps::record_jump(editor, window);
    let (win, buffer) = editor.window_and_buffer_mut(window);
    // TODO
    moved(editor, window, select)
}}
"""


def function_exists(text, func):
    pattern = rf"\bpub\s+fn\s+{re.escape(func)}\s*\("
    return re.search(pattern, text) is not None

def filter_duplicate_funcs(results):
    seen = set()
    filtered = []

    for entry in results:
        func = entry["func"]

        if func in seen:
            continue

        seen.add(func)
        filtered.append(entry)

    return filtered

def filter_actions(results, mod_path):
    # Work on a copy; don't modify the loaded JSON.
    remaining = results[:]

    with open(mod_path, "r", encoding="utf-8") as f:
        for line in f:
            # Only process lines containing Action::
            if "Action::" not in line:
                continue

            for entry in remaining[:]:
                action = entry["action"].removeprefix("Action::")

                if f"Action::{action}" in line:
                    remaining.remove(entry)

    return remaining


def main():
    if len(sys.argv) != 4:
        print(
            f"Usage: {sys.argv[0]} <json_path> <file_path> <mod_path>",
            file=sys.stderr,
        )
        sys.exit(1)

    json_path = sys.argv[1]
    file_path = sys.argv[2]
    mod_path = sys.argv[3]

    with open(json_path, "r", encoding="utf-8") as f:
        results = json.load(f)

    # Remove actions already handled by the dispatch module.
    results = filter_actions(results, mod_path)
    results = filter_duplicate_funcs(results)

    with open(file_path, "r", encoding="utf-8") as f:
        text = f.read()

    funcs = ""

    for entry in results:
        func = entry["func"]

        if not function_exists(text, func):
            if not text.endswith("\n\n"):
                text = text.rstrip("\n") + "\n\n"

            funcs += FUNCTION_TEMPLATE.format(func=func)

    print(funcs, end="")


if __name__ == "__main__":
    main()
