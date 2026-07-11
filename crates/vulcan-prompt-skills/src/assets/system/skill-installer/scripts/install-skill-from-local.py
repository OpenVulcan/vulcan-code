#!/usr/bin/env python3
"""Install a local VulcanCode prompt skill directory into $VULCAN_HOME/code/skills.
从本地目录安装 VulcanCode 提示词技能到 $VULCAN_HOME/code/skills。
"""

from __future__ import annotations

import argparse
import os
import re
import shutil
import sys
from pathlib import Path


class InstallError(Exception):
    pass


def _vulcan_home() -> Path:
    return Path(os.environ.get("VULCAN_HOME", os.path.expanduser("~/.vulcan")))


def _default_dest() -> Path:
    """Resolve the canonical VulcanCode Agent Skills root.
    解析规范的 VulcanCode Agent Skills 根目录。
    """
    return _vulcan_home() / "code" / "skills"


def _normalize_skill_name(name: str) -> str:
    normalized = name.strip().lower()
    normalized = re.sub(r"[^a-z0-9]+", "-", normalized)
    normalized = re.sub(r"-{2,}", "-", normalized).strip("-")
    return normalized


def _validate_source(source: Path) -> None:
    if not source.is_dir():
        raise InstallError(f"Source skill directory not found: {source}")
    skill_file = source / "SKILL.md"
    if not skill_file.is_file():
        raise InstallError("SKILL.md not found in selected skill directory.")


def _validate_skill_name(name: str) -> None:
    if not name or name in {".", ".."}:
        raise InstallError("Skill name must be a non-empty single path segment.")
    if "/" in name or "\\" in name:
        raise InstallError("Skill name must not contain path separators.")


def _copy_skill(source: Path, dest: Path, overwrite: bool) -> None:
    if dest.exists():
        if not overwrite:
            raise InstallError(f"Destination already exists: {dest}")
        if not dest.is_dir():
            raise InstallError(f"Destination exists but is not a directory: {dest}")
        shutil.rmtree(dest)
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(source, dest)


def _parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Install a local VulcanCode prompt skill.")
    parser.add_argument("source", help="Local skill directory containing SKILL.md")
    parser.add_argument("--dest", help="Destination skills root, defaults to $VULCAN_HOME/code/skills")
    parser.add_argument("--name", help="Destination skill name, defaults to source directory name")
    parser.add_argument("--overwrite", action="store_true", help="Replace an existing skill")
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = _parse_args(argv)
    try:
        source = Path(args.source).expanduser().resolve()
        _validate_source(source)
        skill_name = _normalize_skill_name(args.name or source.name)
        _validate_skill_name(skill_name)
        if skill_name == ".system":
            raise InstallError("Refusing to install into the managed .system skill directory.")
        dest_root = Path(args.dest).expanduser().resolve() if args.dest else _default_dest()
        dest = dest_root / skill_name
        _copy_skill(source, dest, args.overwrite)
        print(f"Installed {skill_name} to {dest}")
        return 0
    except InstallError as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
