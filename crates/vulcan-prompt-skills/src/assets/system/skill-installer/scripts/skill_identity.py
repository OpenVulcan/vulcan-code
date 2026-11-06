"""Keep an installed skill's SKILL.md `name` equal to its directory name, which the loader requires.
让已安装技能的 SKILL.md `name` 与其目录名一致，这是加载器的要求。
"""

from __future__ import annotations

import re
from pathlib import Path

_NAME_LINE = re.compile(r"^name\s*:")


class SkillIdentityError(Exception):
    pass


def _frontmatter_end(lines: list[str]) -> int:
    """Return the index of the closing `---` line of the leading YAML frontmatter.
    返回开头 YAML frontmatter 结束 `---` 行的下标。
    """
    if not lines or lines[0].rstrip("\r\n").strip() != "---":
        raise SkillIdentityError("SKILL.md does not start with YAML frontmatter.")
    for index in range(1, len(lines)):
        if lines[index].rstrip("\r\n").strip() == "---":
            return index
    raise SkillIdentityError("SKILL.md frontmatter is not closed.")


def align_skill_name(skill_dir: Path, name: str) -> None:
    """Rewrite the frontmatter `name` line to `name` when it differs.
    当 frontmatter 的 `name` 行与 `name` 不同时改写它。
    """
    skill_file = skill_dir / "SKILL.md"
    with open(skill_file, encoding="utf-8", newline="") as handle:
        lines = handle.read().splitlines(keepends=True)
    end = _frontmatter_end(lines)
    for index in range(1, end):
        line = lines[index]
        if not _NAME_LINE.match(line):
            continue
        current = line.split(":", 1)[1].strip().strip("'\"")
        if current == name:
            return
        ending = line[len(line.rstrip("\r\n")):]
        lines[index] = f"name: {name}{ending}"
        with open(skill_file, "w", encoding="utf-8", newline="") as handle:
            handle.write("".join(lines))
        return
    raise SkillIdentityError("SKILL.md frontmatter has no `name` field.")
