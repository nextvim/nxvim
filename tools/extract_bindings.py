#!/usr/bin/env python3

import re
import sys
import json

def to_snake_case(name):
    return re.sub(r'(?<!^)(?=[A-Z])', '_', name).lower()


def extract_bindings(path, action):
    target = f"{action}_actions"

    with open(path, "r", encoding="utf-8") as f:
        lines = f.readlines()

    results = []

    in_action = False
    in_bind = False
    key = None

    for line in lines:
        stripped = line.strip()

        if target in stripped:
            in_action = True

        if not in_action:
            continue

        if stripped.endswith("_actions") and target not in stripped:
            in_action = False
            continue

        if ".bind(" in stripped:
            in_bind = True
            key = None

        if not in_bind:
            continue

        if key is None and '"' in line:
            start = line.find('"') + 1
            end = line.find('"', start)

            if end != -1:
                key = line[start:end]

        if "Action::" in line:
            start = line.find("Action::") + len("Action::")

            action_name = ""

            for char in line[start:]:
                if char.isalnum() or char == "_":
                    action_name += char
                else:
                    break

            if key is not None and action_name:
                results.append({
                    "keys": key,
                    "action": action_name,
                    "func": to_snake_case(action_name),
                })

            in_bind = False
            key = None

    return results


def main():
    if len(sys.argv) != 3:
        print(f"Usage: {sys.argv[0]} <action> <path>", file=sys.stderr)
        sys.exit(1)

    results = extract_bindings(sys.argv[2], sys.argv[1])

    # for item in results:
    #     print(item)

    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
