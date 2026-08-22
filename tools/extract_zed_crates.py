#!/usr/bin/env python3
"""Extract a minimal, buildable subset of Zed's Rust crates.

The generated tree is a nested Cargo workspace under ``DEST``. By default,
dev-dependency sections are removed because Cargo treats path dependencies under
a workspace root as workspace members and would otherwise pull in much of Zed's
test/UI graph. Pass --include-dev to retain root-crate dev dependencies; support
crate dev dependencies are still removed.
"""

from __future__ import annotations

import argparse
import re
import shutil
import subprocess
import sys
import tomllib
from collections import deque
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

DEFAULT_ROOTS = ("clock", "rope", "sum_tree", "text")
DEV_SECTION_RE = re.compile(
    r"(?ms)^\[(?:target\.[^\n]+\.)?dev-dependencies\]\s*\n.*?(?=^\[|\Z)"
)
MEMBERS_RE = re.compile(
    r'(?ms)^members\s*=\s*\[.*?^\]\s*',
)
DEFAULT_MEMBERS_RE = re.compile(r"(?m)^default-members\s*=.*\n?")
PATCH_CRATES_IO_RE = re.compile(r"(?ms)^\[patch\.crates-io\]\s*\n.*?(?=^\[|\Z)")
PROFILE_RE = re.compile(r"(?ms)^\[profile\.[^\]]+\]\s*\n.*?(?=^\[|\Z)")
WORKSPACE_METADATA_RE = re.compile(
    r"(?ms)^\[workspace\.metadata\.[^\]]+\]\s*\n.*?(?=^\[|\Z)"
)


@dataclass(frozen=True)
class LocalDependency:
    name: str
    path: Path
    kind: str


def load_toml(path: Path) -> dict:
    with path.open("rb") as file:
        return tomllib.load(file)


def git_revision(source: Path) -> str:
    result = subprocess.run(
        ["git", "-C", str(source), "rev-parse", "HEAD"],
        check=True,
        capture_output=True,
        text=True,
    )
    return result.stdout.strip()


def workspace_path_dependencies(source: Path) -> dict[str, Path]:
    workspace = load_toml(source / "Cargo.toml")
    result: dict[str, Path] = {}
    for name, value in workspace.get("workspace", {}).get("dependencies", {}).items():
        if isinstance(value, dict) and "path" in value:
            result[name] = (source / value["path"]).resolve()
    return result


def dependency_tables(manifest: dict, include_dev: bool) -> Iterable[tuple[str, dict]]:
    for kind in ("dependencies", "build-dependencies"):
        yield kind, manifest.get(kind, {})
    if include_dev:
        yield "dev-dependencies", manifest.get("dev-dependencies", {})

    for target in manifest.get("target", {}).values():
        if not isinstance(target, dict):
            continue
        for kind in ("dependencies", "build-dependencies"):
            yield kind, target.get(kind, {})
        if include_dev:
            yield "dev-dependencies", target.get("dev-dependencies", {})


def local_dependencies(
    crate_dir: Path,
    workspace_paths: dict[str, Path],
    include_dev: bool,
) -> list[LocalDependency]:
    manifest = load_toml(crate_dir / "Cargo.toml")
    result: list[LocalDependency] = []
    for kind, table in dependency_tables(manifest, include_dev):
        for dependency_name, value in table.items():
            package_name = dependency_name
            path: Path | None = None
            if isinstance(value, dict):
                package_name = value.get("package", dependency_name)
                if "path" in value:
                    path = (crate_dir / value["path"]).resolve()
                elif value.get("workspace") is True:
                    path = workspace_paths.get(package_name)
            if path is not None:
                result.append(LocalDependency(package_name, path, kind))
    return result


def crate_name(crate_dir: Path) -> str:
    return load_toml(crate_dir / "Cargo.toml")["package"]["name"]


def dependency_closure(
    source: Path,
    roots: list[Path],
    include_root_dev: bool,
) -> dict[str, Path]:
    workspace_paths = workspace_path_dependencies(source)
    root_paths = {path.resolve() for path in roots}
    queue = deque(path.resolve() for path in roots)
    crates: dict[str, Path] = {}

    while queue:
        crate_dir = queue.popleft()
        name = crate_name(crate_dir)
        previous = crates.get(name)
        if previous is not None:
            if previous != crate_dir:
                raise RuntimeError(f"crate {name!r} resolves to both {previous} and {crate_dir}")
            continue
        crates[name] = crate_dir

        # Cargo does not use dev dependencies of ordinary dependencies. Keeping
        # that distinction avoids importing Zed's editor/UI test infrastructure.
        include_dev = include_root_dev and crate_dir in root_paths
        for dependency in local_dependencies(crate_dir, workspace_paths, include_dev):
            manifest = dependency.path / "Cargo.toml"
            if not manifest.is_file():
                raise RuntimeError(
                    f"local dependency {dependency.name!r} from {crate_dir} has no {manifest}"
                )
            queue.append(dependency.path)

    return crates


def relative_to_source(source: Path, path: Path) -> Path:
    try:
        return path.resolve().relative_to(source.resolve())
    except ValueError as error:
        raise RuntimeError(f"local crate {path} is outside Zed source {source}") from error


def strip_dev_dependencies(manifest_path: Path) -> None:
    text = manifest_path.read_text(encoding="utf-8")
    manifest_path.write_text(DEV_SECTION_RE.sub("", text), encoding="utf-8")


def write_workspace_manifest(source: Path, destination: Path, members: list[Path]) -> None:
    text = (source / "Cargo.toml").read_text(encoding="utf-8")
    member_lines = "members = [\n" + "".join(
        f'    "{member.as_posix()}",\n' for member in sorted(members)
    ) + "]\n"
    text, count = MEMBERS_RE.subn(member_lines, text, count=1)
    if count != 1:
        raise RuntimeError("could not replace Zed workspace members list")
    text = DEFAULT_MEMBERS_RE.sub("", text, count=1)
    # Zed's application-wide patches include unrelated Git dependencies. The
    # extracted foundation crates build against their declared registry versions.
    text = PATCH_CRATES_IO_RE.sub("", text, count=1)
    text = PROFILE_RE.sub("", text)
    text = WORKSPACE_METADATA_RE.sub("", text)
    (destination / "Cargo.toml").write_text(text, encoding="utf-8")


def copy_license_files(source: Path, destination: Path) -> None:
    for path in source.iterdir():
        if path.is_file() and (path.name.startswith("LICENSE") or path.name.startswith("NOTICE")):
            shutil.copy2(path, destination / path.name)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path, help="path to a Zed git checkout")
    parser.add_argument(
        "--destination",
        type=Path,
        default=Path("crates/zed"),
        help="generated nested workspace (default: crates/zed)",
    )
    parser.add_argument(
        "--roots",
        nargs="+",
        default=list(DEFAULT_ROOTS),
        help="root crate names under Zed's crates/ directory",
    )
    parser.add_argument(
        "--include-dev",
        action="store_true",
        help="include dev dependencies of root crates (larger extraction)",
    )
    parser.add_argument("--force", action="store_true", help="replace destination if it exists")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    source = args.source.resolve()
    destination = args.destination.resolve()
    if not (source / "Cargo.toml").is_file():
        raise RuntimeError(f"{source} is not a Zed checkout")

    roots = [(source / "crates" / name).resolve() for name in args.roots]
    missing = [path for path in roots if not (path / "Cargo.toml").is_file()]
    if missing:
        raise RuntimeError(f"root crates not found: {', '.join(map(str, missing))}")

    crates = dependency_closure(source, roots, args.include_dev)
    if destination.exists():
        if not args.force:
            raise RuntimeError(f"destination exists: {destination}; pass --force to replace it")
        shutil.rmtree(destination)
    destination.mkdir(parents=True)

    members: list[Path] = []
    root_paths = {path.resolve() for path in roots}
    for name, source_path in sorted(crates.items()):
        relative = relative_to_source(source, source_path)
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copytree(source_path, target)
        if not args.include_dev or source_path not in root_paths:
            strip_dev_dependencies(target / "Cargo.toml")
        members.append(relative)
        print(f"copied {name:<24} {relative}")

    copy_license_files(source, destination)
    write_workspace_manifest(source, destination, members)
    revision = git_revision(source)
    (destination / "ZED_REVISION").write_text(revision + "\n", encoding="utf-8")
    (destination / "README.md").write_text(
        "# Extracted Zed crates\n\n"
        f"Generated from `zed-industries/zed` revision `{revision}` by "
        "`scripts/extract_zed_crates.py`. Do not edit generated files directly.\n\n"
        "Regenerate from the repository root with:\n\n"
        "```sh\n"
        f"python3 scripts/extract_zed_crates.py /path/to/zed --force\n"
        "```\n",
        encoding="utf-8",
    )

    names = ", ".join(sorted(crates))
    print(f"\nextracted {len(crates)} crates to {destination}")
    print(f"revision: {revision}")
    print(f"crates: {names}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1)
