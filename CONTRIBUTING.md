# Contributing to Vulcan Code

Thank you for helping. Bug reports, suggestions and pull requests are all welcome.

## Report a problem or suggest an idea

- **From inside Vulcan Code** (recommended): open the account menu and choose *Send feedback*, or press
  *Report this problem* on a session error card. Everything is filtered on your device first: keys, tokens, paths,
  your user name and other personal data are replaced, and you see the exact content before it is sent. Replies
  from the team arrive in the message center. Sending needs a signed-in OpenVulcan account.
- **On GitHub**: open an [issue](https://github.com/OpenVulcan/vulcan-code/issues). Please remove secrets and
  personal paths from logs yourself.

## Fix it yourself

Vulcan Code can help: ask it to use the `vulcan-self-fix` skill. It clones this repository into a directory you
choose, builds a Debug version, reproduces the problem, fixes and verifies it, and opens a pull request only after
you confirm. Doing it by hand works the same way:

1. Fork and clone the repository, then prepare the toolchain once:
   ```bash
   make init-system
   ```
2. Sync runtime resources and build the Debug distribution:
   ```bash
   make sync
   ```
   ```bash
   make build
   ```
3. Run it with `make run`, reproduce the problem, fix it and add a test that fails without your fix.
4. Before committing, format and run the checks:
   ```bash
   cargo fmt --all
   ```
   ```bash
   node scripts/check.mjs rustfmt repository
   ```
   Add the `web` or `desktop` group when you changed the WebUI or the desktop shell. Prefer targeted tests
   (`cargo nextest run -p <crate>`, `npx vitest run <dir>`) over the whole workspace.
5. Open a pull request with the template. If it fixes feedback you sent from Vulcan Code, put the feedback id
   (`fb_…`, shown in the message center) in the template so the team can link the two.

## Project rules

[`AGENTS.md`](AGENTS.md) holds the rules every change follows, with links to the detailed contracts in
`docs/contracts/`. The most common ones:

- Text for the AI and backend messages is English; interface text lives only in `crates/vulcan-i18n/catalogs/`.
- Do not edit `output/` or `output-release/`; change the sources under `crates/vulcan-runtime-assets/resources/`
  and run `make sync`.
- Tests constrain behavior and formats, not exact wording.

By contributing you agree that your contribution is licensed under the [MIT License](LICENSE).
