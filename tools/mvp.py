#!/usr/bin/env python3
import os
import re

def main():
    # Resolve the path to MVP.md relative to the script location
    script_dir = os.path.dirname(os.path.abspath(__file__))
    mvp_path = os.path.join(script_dir, "..", "MVP.md")

    if not os.path.exists(mvp_path):
        print(f"Error: MVP.md not found at {mvp_path}")
        return

    with open(mvp_path, "r", encoding="utf-8") as f:
        content = f.read()

    checked = len(re.findall(r"\[x\]", content))
    unchecked = len(re.findall(r"\[ \]", content))
    total = checked + unchecked

    if total == 0:
        print("No checklist items found.")
        return

    percentage = (checked / total) * 100
    print(f"Completed: {checked}")
    print(f"Uncompleted: {unchecked}")
    print(f"Total: {total}")
    print(f"Completion: {percentage:.1f}%")

if __name__ == "__main__":
    main()
