#!/usr/bin/env python3
import sys
import os

filepath = sys.argv[1]
filename = os.path.basename(filepath)

if filename == "git-rebase-todo":
    # Sequence editing mode
    with open(filepath, "r") as f:
        lines = f.readlines()

    # Predefined precise action overrides for vague historical commits
    overrides = {
        "391d1fa": "reword",
        "c49b9d7": "squash",
        "d6aa762": "squash",
        "f8ce4ca": "squash",
        "0af9b27": "reword",
        "a5857b9": "reword",
    }

    parsed_commits = []
    # parse each line
    for line in lines:
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            parsed_commits.append({"raw": line, "is_commit": False})
            continue

        parts = stripped.split()
        if len(parts) >= 3 and parts[0] in ("pick", "reword", "edit", "squash", "fixup", "drop"):
            cmd = parts[0]
            commit_hash = parts[1]
            subject = " ".join(parts[2:])

            # Check if there is an override for this commit hash
            override_action = None
            for key, val in overrides.items():
                if commit_hash.startswith(key) or key.startswith(commit_hash):
                    override_action = val
                    break

            if override_action:
                cmd = override_action

            parsed_commits.append({
                "raw": line,
                "is_commit": True,
                "cmd": cmd,
                "hash": commit_hash,
                "subject": subject
            })
        else:
            parsed_commits.append({"raw": line, "is_commit": False})

    # Auto-squash adjacent commits with identical subjects
    # Also, if we squash subsequent commits into a first one, make the first one a 'reword'
    n = len(parsed_commits)
    i = 0
    while i < n:
        if not parsed_commits[i]["is_commit"]:
            i += 1
            continue

        # Find sequence of identical subjects
        j = i + 1
        sequence = [i]
        while j < n:
            if parsed_commits[j]["is_commit"] and parsed_commits[j]["subject"] == parsed_commits[i]["subject"]:
                sequence.append(j)
                j += 1
            elif not parsed_commits[j]["is_commit"]:
                # skip non-commit lines like comments in between
                j += 1
            else:
                break

        if len(sequence) > 1:
            # We have adjacent identical commits!
            # Mark the first one as reword (if it isn't already reword/squash/etc.)
            if parsed_commits[sequence[0]]["cmd"] == "pick":
                parsed_commits[sequence[0]]["cmd"] = "reword"
            # Mark all others as squash
            for idx in sequence[1:]:
                parsed_commits[idx]["cmd"] = "squash"

        i = j

    # Reconstruct the todo lines
    new_lines = []
    for item in parsed_commits:
        if not item["is_commit"]:
            new_lines.append(item["raw"])
        else:
            new_lines.append(f"{item['cmd']} {item['hash']} {item['subject']}\n")

    with open(filepath, "w") as f:
        f.writelines(new_lines)

else:
    # Message editing mode (e.g. COMMIT_EDITMSG)
    with open(filepath, "r") as f:
        content = f.read()

    # Filter out comments (lines starting with '#')
    non_comment_lines = [line for line in content.splitlines() if not line.strip().startswith('#')]
    non_comment_content = "\n".join(non_comment_lines)

    new_msg = os.environ.get("REBASE_MSG")

    if not new_msg:
        # Predefined templates for known migrations/refactorings
        if "Commit current work changes" in non_comment_content:
            new_msg = """docs: add OPERATIONS.md outlining editor operations and key handlers

- Create `OPERATIONS.md` documenting key configurations, mappings, and core command mechanics.
- Standardize core key operation schemas across the application."""

        elif "Stage 1 & 2" in non_comment_content or "Stage 3 & 4" in non_comment_content:
            new_msg = """refactor(vim-ui): implement Stage 1-4 structural improvements to vim-ui crate

- Stage 1 & 2: Introduce strongly-typed IDs, encapsulate window and view state, and establish separate view and window managers.
- Stage 3 & 4: Add strongly-typed events and commands, define explicit editor execution contexts, and separate view models from rendering logic."""

        elif "Add utility scripts to repository" in non_comment_content or "Update README." in non_comment_content:
            new_msg = """docs: update repository documentation and add push utility script

- Restructure the main README.md and add detailed documentation for the `vim-buffer` crate.
- Introduce `scripts/push.py` utility script to automate repository synchronization."""

        elif "Update example. Begin the Editor" in non_comment_content or "Second attempt (GPT-5.6-sol)" in non_comment_content:
            new_msg = """feat: bootstrap initial editor architecture and integrate example runtimes

- Create initial editor structures, buffer selections, and document models.
- Implement basic crossterm application loops and command-line parsing.
- Add comprehensive interactive interpreter examples in `vim-script` to demonstrate capabilities."""

        elif "Migrate DZed" in non_comment_content and "UI migration" not in non_comment_content:
            new_msg = """refactor: migrate DZed editor core into modular nxvim structure

- Port document model, selections, highlight engines, and tree-sitter syntax providers.
- Setup basic UI components (views, layouts, tabline, statusline) under the new nxvim architecture.
- Establish initial command controllers, event loops, and core application services."""

        elif "UI migration to vim-ui" in non_comment_content:
            new_msg = """refactor: migrate application UI layer to the vim-ui crate

- Refactor window layout, status bar, tabs, and text view rendering to utilize the `vim-ui` crate.
- Standardize window management and view models using `vim-ui`'s manager and model architectures.
- Clean up duplicate layout and UI rendering code from the main application crates."""

        elif "Input migration to vim-input" in non_comment_content:
            new_msg = """refactor: migrate key sequence resolution to vim-input crate

- Decouple low-level crossterm input handling from key sequence resolution.
- Integrate `vim-input` crate to manage interactive key sequence mapping and action queue.
- Add VIM-INPUT.md documenting key event grammar and input translation logic."""

        elif "Disable vim-buffer and vim-ui for now" in non_comment_content:
            new_msg = """chore: temporarily disable vim-buffer and vim-ui modules for local testing

- Temporarily exclude `vim-buffer` and `vim-ui` dependencies in Cargo.toml.
- Adjust view controllers to support compilation with a simplified internal layout/renderer structure."""

        elif "Ingest DZed" in non_comment_content:
            new_msg = """refactor: integrate DZed rendering and textview view components

- Ingest advanced text rendering and textview layout configurations.
- Refactor status bar formatting and text scrolling controllers."""

        elif "Begin scripting" in non_comment_content or "Scripting added" in non_comment_content:
            new_msg = """feat(scripting): integrate vim-script engine with controller runtime

- Embed `vim-script` interpreter into the nxvim application.
- Implement `ScriptRuntime` to manage runtime state, scheduler, and command registry.
- Register and map editor commands, allowing them to execute within the scripting host environment.
- Enable execution of Vimscript Ex commands directly through the controller."""

        elif "Buffer migration to vim-buffer" in non_comment_content:
            new_msg = """refactor: migrate editor buffer management to vim-buffer crate

- Integrate the modular `vim-buffer` crate as the primary buffer and document model management layer.
- Port and refactor document mutations, anchor resolution, and comprehensive selection/multicursor models.
- Implement `DisplayMapAdapter` in the editor display layer to bridge `vim-buffer` snapshots with visual rendering.
- Adjust command execution and text view controllers to synchronize state using `vim-buffer`'s public APIs.
- Remove outdated internal selections and document logic (-5500+ lines of duplicate code), unifying on the new `buffers.rs`, `document.rs`, and `selections.rs` architecture.
- Document buffer mutation APIs and synchronization workflows in VIM-BUFFER.md."""

    if new_msg:
        with open(filepath, "w") as f:
            f.write(new_msg.strip() + "\n")
