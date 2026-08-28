#!/usr/bin/env python3

import os
import re
import sys


def find_actions(text):
    return set(re.findall(r"\bAction::([A-Za-z_][A-Za-z0-9_]*)", text))


def main():
    if len(sys.argv) != 3:
        print(
            f"Usage: {sys.argv[0]} <file_path> <folder_path>",
            file=sys.stderr,
        )
        sys.exit(1)

    file_path = sys.argv[1]
    folder_path = sys.argv[2]

    # Build initial list from file_path.
    with open(file_path, "r", encoding="utf-8") as f:
        text = f.read()

    unimplemented = find_actions(text)

    # Recursively search folder_path and all subdirectories.
    for root, dirs, files in os.walk(folder_path):
        for filename in files:
            path = os.path.join(root, filename)

            try:
                with open(path, "r", encoding="utf-8") as f:
                    text = f.read()
            except (UnicodeDecodeError, OSError):
                continue

            # Remove every Action found in this file.
            unimplemented -= find_actions(text)

    # Print remaining actions.
    for action in sorted(unimplemented):
        print(f"Action::{action}")


if __name__ == "__main__":
    main()
