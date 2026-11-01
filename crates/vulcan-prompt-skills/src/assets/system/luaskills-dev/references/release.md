# Releasing a LuaSkill

LuaSkills installs skills from GitHub releases, and LuaSkills Hub lists skills by syncing their GitHub
repository. Hub does not accept zip uploads.

## Assets

| Layout | Assets |
| --- | --- |
| standard (default) | `<id>-v<version>-skill.zip`, `<id>-v<version>-checksums.txt` |
| platform | `<id>-v<version>-<platform>-skill.zip` per system with bundled payloads, `<id>-v<version>-skill.zip` (all systems, omit with `--no-universal`), `<id>-v<version>-packages.json`, checksums |

Each zip holds exactly one top-level directory named `<id>`. LuaSkills 0.8.0 or newer reads `packages.json` and
installs the package for the current system; older hosts use the all-systems package, so keep it unless every
user runs 0.8.0 or newer. The repository name should equal the `skill_id`.

## Workflow

`luaskills-debug new` writes `.github/workflows/release.yml`. The core of it:

```yaml
on:
  push:
    tags: ["v*"]
jobs:
  release:
    runs-on: ubuntu-24.04
    permissions:
      contents: write
    steps:
      - uses: actions/checkout@v4
      - id: package
        uses: LuaSkills/luaskills-debug@v0.8.0
        with:
          layout: standard
          expect-tag: ${{ github.ref_name }}
      - run: gh release create "$GITHUB_REF_NAME" --generate-notes "${{ steps.package.outputs.out-dir }}"/*
        env:
          GH_TOKEN: ${{ github.token }}
```

Action inputs: `skill-path`, `layout`, `include-universal`, `out-dir`, `skill-id`, `include` (one entry per
line), `expect-tag`, `version`. The generated workflow reads the layout from the repository variable
`LUASKILLS_PACKAGE_LAYOUT` (default: the layout given to `new`), so switching needs no file edit. A platform release must build or download every bundled payload in earlier steps
before the package step.

## Steps for the user

1. Bump `version` in `skill.yaml`; run `validate` and `package` locally and check the report.
2. Commit and push to GitHub.
3. Tag `v<version>` and push the tag; the workflow publishes the release.
4. On LuaSkills Hub, submit the repository. The first GitHub repository that publishes a skill id owns it;
   later versions sync from the same repository.

Hub limits: 100 MiB per package, 600 MiB per version, 1 GiB expanded. Larger releases still work from GitHub
but cannot be listed on Hub.

Pushing, tagging and publishing are the user's decisions: prepare the commands, run them only when asked.
