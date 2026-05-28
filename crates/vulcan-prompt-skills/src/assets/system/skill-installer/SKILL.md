---
name: skill-installer
description: Install VulcanCode prompt skills into $VULCAN_HOME/skills from a GitHub repo path or local skill directory. Use when a user asks to list installable skills, install a skill from another repo (including private repos), or install/import a local skill directory.
---

# Skill Installer

Helps install VulcanCode prompt skills. A prompt skill is a directory that contains a `SKILL.md` file with `name` and `description` frontmatter, optional root-level `vulcan.yaml` UI metadata, plus optional `scripts/`, `references/`, and `assets/` resources.

Use the helper scripts based on the task:
- List skills when the user provides a GitHub repo/path and asks what is available.
- Install from another repo when the user provides a GitHub repo/path (including private repos).
- Install from a local directory by copying a validated skill folder into the user skill root.

Install skills with the helper scripts.

## Communication

When listing skills, output approximately as follows:
"""
Skills from {repo}:
1. skill-1
2. skill-2 (already installed)
3. ...
Which ones would you like installed?
"""

After installing a skill, tell the user to refresh the Prompt Skills panel. If the current run already started before installation, explain that the new skill is available on the next AI request.

## Scripts

GitHub scripts use network. Local installation does not use network.

- `scripts/list-skills.py` (prints skills list with installed annotations)
- `scripts/list-skills.py --format json`
- `scripts/install-skill-from-github.py --repo <owner>/<repo> --path <path/to/skill> [<path/to/skill> ...]`
- `scripts/install-skill-from-github.py --url https://github.com/<owner>/<repo>/tree/<ref>/<path>`
- `scripts/install-skill-from-local.py <local-skill-directory> [--name <skill-name>] [--overwrite]`

## Behavior and Options

- Defaults to direct download for public GitHub repos.
- If download fails with auth/permission errors, falls back to git sparse checkout.
- Aborts if the destination skill directory already exists.
- Installs into `$VULCAN_HOME/skills/<skill-name>` (defaults to `~/.vulcan/skills`).
- Multiple `--path` values install multiple skills in one run, each named from the path basename unless `--name` is supplied.
- Options: `--ref <ref>` (default `main`), `--dest <path>`, `--method auto|download|git`.
- Local install validates that the source is a directory with `SKILL.md`, normalizes the destination name, and refuses to write into the managed `.system` directory.

## Notes

- Private GitHub repos can be accessed via existing git credentials or optional `GITHUB_TOKEN`/`GH_TOKEN` for download.
- Git fallback tries HTTPS first, then SSH.
- Installed annotations come from `$VULCAN_HOME/skills`.
- System skills live under `$VULCAN_HOME/skills/.system` and are managed by VulcanCode. Do not overwrite `.system` from this skill.
