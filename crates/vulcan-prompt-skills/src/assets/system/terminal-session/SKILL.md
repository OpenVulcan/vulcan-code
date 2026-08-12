---
name: terminal-session
description: Required operating guide for the terminal_session tool. Use before any terminal_session call to understand named persistent terminals, cross-platform PTY input and child-side platform semantics, incremental output, command transactions with exit codes, TUI screen output, and safe control of long-running processes.
---

# Terminal Session

Before using `terminal_session`, read this skill and follow it. Do not use `terminal_session` from memory.

`terminal_session` manages named persistent host terminals. It is not a one-shot command runner. Use `shell_command` for isolated commands that do not need inherited shell state.

## Core Rules

- Use `shell_command` for isolated commands.
- Use `terminal_session` when shell state matters: activated environments, changed directories, REPLs, long-running processes, interactive prompts, or TUI screens.
- Always give `start` a non-empty, task-oriented `name`.
- Use standardized `text`, `paste`, `key`, and `chord` inputs. Do not hand-write ANSI escapes, platform-specific getch bytes, or control characters.
- Check `backend` and `input_capabilities` on tool responses. Interactive terminal sessions use the same isolated PTY and ANSI/VT host-input contract on Windows, Linux, and macOS. On Windows, ConPTY may translate that host input into Console key events before a console-mode child reads it.
- Prefer `read` after append-style output. For TUI or screen-like output, use `send` with `screen_stable` or `snapshot` to inspect `screen_text`.
- Before finalizing a task, clean up temporary terminals you started for tests or diagnostics. Do not announce final completion until this cleanup is done.

## Backend Capabilities

Every terminal response may include backend and capability metadata:

- `backend`: the concrete runtime backend, normally `pty` for `terminal_session` and `pipe` only for command-style sessions.
- `input_capabilities`: whether structured `key` and `chord` inputs are supported and which encoding the host applies.
- `screen_frame_seq`, `screen_size`, and `screen_text`: the latest frame reported by a screen-capable backend.

Backend rules:

- `pty`: the cross-platform `terminal_session` backend on Windows, Linux, and macOS. It runs in the isolated terminal host and converts standardized keys/chords into ANSI/VT input. Windows ConPTY can then expose supported special keys as Console key events to console-mode child programs; this child-facing representation does not change the tool contract.
- `pipe`: command-style process I/O. It is not a full TUI backend.

Do not infer key support from the operating system alone. Use the response's `backend` and `input_capabilities` fields.

## Actions

### list

Lists terminals owned by the current controller session. Use it when you forgot a terminal id or need to recover after context loss.

```json
{
  "action": "list"
}
```

Choose a terminal by `name`, `terminal_session_id`, `status`, command, and working directory. Do not guess ids.

### start

Starts one persistent terminal. `name` and `command` must both be non-empty. `name` is only required for `start`; it is ignored by other actions.

```json
{
  "action": "start",
  "name": "dev shell",
  "command": "pwsh -NoLogo",
  "shell": "auto"
}
```

`cwd` only applies at start time. Later `send` calls inherit the process state; change directories by typing shell commands such as `cd`.

`start` uses the same isolated PTY backend on Windows, Linux, and macOS. The operating system changes the PTY implementation, not the tool contract, executable, protocol, or lifecycle.

On Windows, if `command` directly launches `bash`, `sh`, or `zsh`, do not use `shell: "auto"`. Select the explicit shell so the host can use deterministic interactive newline behavior.

### send

Sends ordered inputs to an existing terminal. `terminal_session_id` is required and `inputs` must be non-empty.

```json
{
  "action": "send",
  "terminal_session_id": "...",
  "inputs": [{ "kind": "text", "text": "npm test\n" }],
  "wait_policy": { "mode": "until_exit_code", "max_wait_ms": 120000 }
}
```

If `wait_policy` is omitted, the call returns immediately after writing input. Use an explicit bounded wait when you need output back from the same call.

### read

Reads output for an existing terminal. `terminal_session_id` is required. By default, `read` starts from the terminal's AI-facing cursor and advances that cursor after returning chunks.

```json
{
  "action": "read",
  "terminal_session_id": "...",
  "wait_policy": { "mode": "until_quiet", "quiet_ms": 300, "max_wait_ms": 3000 }
}
```

Use `read_from_start: true` only for explicit history review. Use `after_seq` only when you have a known sequence number from a prior response. `max_chunks` caps returned chunks; the default is `128`.

### snapshot

Returns current terminal metadata and status for an existing terminal. Use it when you need `status`, `backend`, `input_capabilities`, `last_seq`, byte counts, exit code, or the latest screen frame. It does not advance the AI-facing read cursor.

```json
{
  "action": "snapshot",
  "terminal_session_id": "..."
}
```

For PTY sessions, `snapshot` can expose the latest `screen_text` checkpoint without consuming incremental output. Use `send` or `read` with `screen_stable` when you need fresh visible output.

### stop

Stops an existing terminal.

```json
{
  "action": "stop",
  "terminal_session_id": "..."
}
```

If preserving the shell matters, try `Ctrl+C` first and then `read` to verify the shell is usable. Use `stop` when the terminal should be terminated or foreground interruption did not work.

## Wait Policies

Every wait policy is bounded. Timeouts do not stop the terminal.

- `return_immediately`: return after sending input or checking for available output.
- `until_exit_code`: wrap exactly one ordinary shell command and return a per-command exit code.
- `until_quiet`: wait until a real quiet window is observed or the bounded budget expires; inspect `quiet_observed`.
- `until_prompt`: best-effort prompt-oriented bounded wait; prefer `until_exit_code` for ordinary command success/failure.
- `until_pattern`: bounded wait intended for readiness output. Treat readiness as true only when `pattern_matched` is `true`.
- `screen_stable`: bounded screen-oriented wait for TUI or alternate-screen output; inspect `quiet_observed`.

For readiness-sensitive flows, use a target-specific literal pattern such as a prompt, menu title, or `ready` line. Do not send navigation keys merely because a timeout elapsed.

### Command Transactions

Use `until_exit_code` when you need the result of one ordinary shell command inside an existing terminal state.

Good:

```json
{
  "action": "send",
  "terminal_session_id": "...",
  "inputs": [{ "kind": "text", "text": "npm run build\n" }],
  "wait_policy": { "mode": "until_exit_code", "max_wait_ms": 120000 }
}
```

`until_exit_code` requires exactly one `text` input containing one complete non-empty command ending with a newline.

The terminal must be on a clean command line. If the tool reports a pending input line, send `Ctrl+C` or otherwise clear/submit that line, then verify the shell is usable before retrying `until_exit_code`.

Do not use `until_exit_code` for:

- `paste`, `key`, or `chord` inputs.
- REPL input.
- Interactive prompts that ask follow-up questions.
- Long-running servers.
- TUI or screen-mode programs.
- Half-written command lines.

PowerShell note: native `$LASTEXITCODE` is authoritative when present. For cmdlet-only commands, the host also checks whether new PowerShell errors appeared during the transaction.

## Inputs

`inputs` are ordered and written in order.

```json
{ "kind": "text", "text": "Get-Date\n" }
```

Use `text` for typed command lines and REPL input. Include `\n` when submitting a command.

```json
{ "kind": "paste", "text": "multi-line content" }
```

Use `paste` for larger text blocks. Do not combine `paste` with `until_exit_code`.

```json
{ "kind": "key", "key": "ArrowDown" }
```

Use `key` for standardized single-key input.

Supported key names:

```text
Enter, Tab, Esc, Escape, Backspace, Delete,
ArrowUp, ArrowDown, ArrowLeft, ArrowRight,
Home, End, PageUp, PageDown, Insert,
F1 ... F12
```

Enter is encoded for the current backend and interactive shell, but child programs may observe `\n` or `\r` depending on how they read input. Programs that read raw characters should treat both as submit keys.

On Windows PTY sessions, the terminal host writes ANSI/VT input to ConPTY. ConPTY may present supported special keys to console-mode child programs as Windows Console key events. For example, Python `msvcrt.getch()` normally reports an arrow as an extended prefix (`b"\xe0"` or `b"\x00"`) followed by `b"H"`, `b"P"`, `b"K"`, or `b"M"` for Up, Down, Left, or Right. A Python Windows-console TUI must use `msvcrt.getch()` or an equivalent Console-input API for these keys; `sys.stdin.read()` does not expose the arrow-key Console events as VT bytes. The AI must still send standardized `key` inputs such as `ArrowDown` and must never send those scan-code bytes manually.

Generated Enter/key-submit input uses a single carriage return for PowerShell and cmd-style shells on Windows PTY sessions. Avoid sending manual `\r\n` as a key substitute; the extra line-feed can leave PowerShell in continuation mode.

```json
{ "kind": "chord", "key": "c", "ctrl": true }
```

Use `chord` for supported shortcuts:

- `Ctrl` with one alphabetic key, such as `Ctrl+C`, `Ctrl+D`, `Ctrl+Z`, `Ctrl+L`, or `Ctrl+R`.
- `Shift+Tab`.
- `Alt` or `Meta` combined with a supported standard key, such as `Alt+ArrowLeft`.

Do not assume arbitrary application shortcuts are supported. If a chord is rejected, use a supported key sequence or normal text input instead.

## Input Encoding

The terminal host always converts standardized keys and chords to the cross-platform ANSI/VT host-input contract. There is no per-platform `key_encoding` selector. The reported `encoding: "ansi_vt"` describes bytes written by the host to the PTY; it does not guarantee that every child-side input API returns those same bytes after the platform terminal driver processes them.

```json
{
  "action": "send",
  "terminal_session_id": "...",
  "inputs": [{ "kind": "key", "key": "ArrowDown" }],
  "wait_policy": { "mode": "screen_stable", "quiet_ms": 150, "max_wait_ms": 1000 }
}
```

Windows ConPTY translates supported terminal keys into Console `KEY_EVENT` records for console-mode child programs, which supports normal `msvcrt.getch()` menus. Arbitrary scan-code injection and application-specific raw `KEY_EVENT` construction remain outside this PTY contract. Prefer standardized `key` and `chord` inputs, ANSI/VT-aware application modes, or documented text shortcuts.

### Child Input Semantics By Platform

The tool-facing API is identical on every platform, but a child program observes input through its operating system's terminal API:

| Platform | PTY path | What a Python TUI should use | Arrow-key behavior |
| --- | --- | --- | --- |
| Windows | Terminal host writes ANSI/VT to ConPTY; ConPTY can translate it to Console input events | `msvcrt.getch()` for a Windows console menu, or a cross-platform TUI library that supports ConPTY | `msvcrt.getch()` commonly returns `b"\xe0"` or `b"\x00"`, then `b"H"`/`b"P"`/`b"K"`/`b"M"` |
| Linux | Terminal host writes ANSI/VT to a POSIX PTY; the terminal line discipline applies | `curses`, `termios` plus `tty`, or another PTY-aware TUI library | In raw/cbreak mode, simple arrows are ANSI escape sequences such as `ESC [ A` through `ESC [ D`; canonical mode may buffer until newline |
| macOS | Terminal host writes ANSI/VT to a POSIX PTY; the terminal line discipline applies | `curses`, `termios` plus `tty`, or another PTY-aware TUI library | The same POSIX raw/cbreak rules apply; escape-sequence bytes may arrive across multiple reads |

Do not use plain canonical `sys.stdin.read()` as a universal cross-platform raw-key reader. On Windows it does not expose `msvcrt` extended-key events as VT bytes; on Linux and macOS canonical mode can wait for a newline. Use a platform terminal API or a maintained cross-platform TUI library, and let that library decode key sequences.

This distinction is only for code running inside the child terminal. AI callers always use the same structured inputs:

```json
{ "kind": "key", "key": "ArrowDown" }
```

Never branch AI tool calls by operating system and never send `\x1b[B`, `\xe0P`, or other platform bytes manually.

## Output

Check `output_mode` on every response.

- `log`: append-style output. Treat returned chunks as normal terminal text.
- `screen`: screen-like or alternate-screen output. Use `screen_text` when present and drive with keys/chords.
- `mixed`: both log-like and screen-like behavior. Prefer small steps and verify each result.
- `unknown`: not enough evidence yet. Use bounded waits and conservative reads.

For `screen` and `mixed` responses, prefer `[screen_text]`, `screen_frame_seq`, `screen_size`, and `[controls_hint]` over raw ANSI chunks when they are present.

If `has_more` is true, call `read` again with the same terminal id before making conclusions from partial output.

## TUI Control

TUI output represents a screen, not a plain append-only log.

Control strategy:

1. Use `read` or `send` with `screen_stable` to get visible output.
2. Inspect `backend`, `input_capabilities`, `screen_text`, and `controls_hint` when present.
3. Send one or two `key` or `chord` inputs.
4. Read again and verify the screen changed as expected.
5. Use `Esc`, `q`, or `Ctrl+C` only when the screen or program convention indicates they are appropriate.

Do not send ANSI escapes directly. Use standardized keys; the terminal host owns their ANSI/VT encoding on every platform.

If a key does not work, inspect `backend` and `input_capabilities`, verify how the target application reads terminal input, and prefer documented text shortcuts where available. In particular, a Windows Python console TUI should use `msvcrt.getch()` rather than `sys.stdin.read()` for arrows and other extended keys.

For Linux and macOS Python TUIs, use `curses` or place stdin in raw/cbreak mode with `termios` and `tty`; do not expect canonical `sys.stdin.read()` to return an arrow immediately or in one read. Cross-platform applications should prefer a library that owns both the Windows Console and POSIX terminal input modes.

## Interrupts

Use `Ctrl+C` to interrupt a stuck foreground process before `stop` when preserving the shell matters.

```json
{
  "action": "send",
  "terminal_session_id": "...",
  "inputs": [{ "kind": "chord", "key": "c", "ctrl": true }],
  "wait_policy": { "mode": "until_quiet", "quiet_ms": 300, "max_wait_ms": 3000 }
}
```

After `Ctrl+C`, perform one stabilizing `read` and verify the shell prompt or expected application state returned before the next `until_exit_code` command.

If the response contains a standalone `\x03`, `^C`, or a PowerShell message for that control character, treat it as interrupt cleanup noise when the next command behaves normally. Do not retry destructive commands only because of that cleanup noise.

Ctrl+C may not terminate every foreground process under Windows PowerShell/ConPTY. If the process remains alive, use `stop`.

## Finalization

Before giving the final answer for a task that used `terminal_session`:

1. Decide which terminals you created only for this task.
2. Stop temporary test or diagnostic terminals that are no longer needed.
3. Keep long-lived user-intended terminals running only when the user explicitly needs them.
4. If you are unsure which terminals remain, call `list` and identify them by `name`, `status`, command, and working directory.
5. Only after cleanup, give the final summary.

Avoid saying the work is final, finished, stopped, or ended while you still need to inspect or stop terminals.

## Examples

### Start PowerShell

```json
{
  "action": "start",
  "name": "powershell workspace",
  "command": "pwsh -NoLogo",
  "shell": "auto"
}
```

### Recover An Existing Terminal

```json
{
  "action": "list"
}
```

### Run One Command With Exit Code

```json
{
  "action": "send",
  "terminal_session_id": "...",
  "inputs": [{ "kind": "text", "text": "cargo test -p vulcan-agent-runtime\n" }],
  "wait_policy": { "mode": "until_exit_code", "max_wait_ms": 120000 }
}
```

### Read More Output

```json
{
  "action": "read",
  "terminal_session_id": "...",
  "wait_policy": { "mode": "until_quiet", "quiet_ms": 300, "max_wait_ms": 3000 }
}
```

### Drive An ANSI TUI

```json
{
  "action": "send",
  "terminal_session_id": "...",
  "inputs": [
    { "kind": "key", "key": "ArrowDown" },
    { "kind": "key", "key": "Enter" }
  ],
  "wait_policy": { "mode": "screen_stable", "quiet_ms": 150, "max_wait_ms": 1000 }
}
```

### Drive A Windows Python Getch Menu Through ConPTY

Inside the Python program, read extended keys with `msvcrt.getch()`:

```python
first = msvcrt.getch()
if first in (b"\xe0", b"\x00"):
    scan = msvcrt.getch()
    key = {
        b"H": "up",
        b"P": "down",
        b"K": "left",
        b"M": "right",
    }.get(scan)
```

Continue to drive that program with standardized terminal inputs. `ArrowDown` reaches the Python program as the corresponding ConPTY Console key event; do not send `b"\xe0P"` yourself.

```json
{
  "action": "send",
  "terminal_session_id": "...",
  "inputs": [
    { "kind": "key", "key": "ArrowDown" },
    { "kind": "key", "key": "Enter" }
  ],
  "wait_policy": { "mode": "screen_stable", "quiet_ms": 150, "max_wait_ms": 1000 }
}
```

### Stop A Terminal

```json
{
  "action": "stop",
  "terminal_session_id": "..."
}
```
