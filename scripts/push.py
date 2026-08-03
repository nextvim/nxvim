#!/usr/bin/env python3
"""Push/sync local changes from nxvim workspace crates to their individual repositories."""

import shutil
import sys
from pathlib import Path

CRATES = ["vim-input", "vim-formatter", "vim-script", "vim-buffer", "vim-regex", "vim-ui"]

def sync_crate(src: Path, dst: Path) -> bool:
    if not src.exists():
        print(f"Error: Source directory {src} does not exist.", file=sys.stderr)
        return False

    print(f"Syncing {src.name} to {dst}...")

    # Ensure target directory exists
    dst.mkdir(parents=True, exist_ok=True)

    # Clean the target directory, preserving `.git`
    for item in dst.iterdir():
        if item.name == '.git':
            continue
        if item.is_dir() and not item.is_symlink():
            shutil.rmtree(item)
        else:
            item.unlink()

    # Copy all items from source to target
    for item in src.iterdir():
        if item.name == '.git':
            continue
        target_item = dst / item.name
        if item.is_dir():
            shutil.copytree(item, target_item)
        else:
            shutil.copy2(item, target_item)

    print(f"Successfully synced {src.name}!\n")
    return True

def main() -> int:
    # Resolve the path of this script and locate the project root
    script_path = Path(__file__).resolve()

    # Support running from either scripts/ or script/
    project_root = script_path.parent.parent
    parent_dir = project_root.parent

    print(f"Project root resolved to: {project_root}")
    print(f"Target parent directory:  {parent_dir}\n")

    success = True
    for crate in CRATES:
        src = project_root / "crates" / crate
        dst = parent_dir / crate
        if not sync_crate(src, dst):
            success = False

    if success:
        print("All crates synced successfully!")
        return 0
    else:
        print("Some crates failed to sync.", file=sys.stderr)
        return 1

if __name__ == "__main__":
    sys.exit(main())
