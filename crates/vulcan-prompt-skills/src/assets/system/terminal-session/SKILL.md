---
name: terminal-session
description: Required operating guide for the terminal_session tool. Use before any terminal_session call to understand persistent terminal sessions, incremental output, command transactions with exit codes, shortcuts, TUI screen output, and safe control of long-running processes.
---

# Terminal Session

Before using `terminal_session`, read this skill and follow it. Do not use `terminal_session` from memory.

`terminal_session` manages named persistent terminals. It is not a one-shot command runner. Use `shell_command` for isolated commands that do not need inherited shell state.

## Quick Choice

- Use `shell_command` for an isolated command with no inherited terminal state.
- Use `terminal_session` when state matters: activated environments, changed directories, REPLs, long-running processes, interactive programs, or TUI screens.
- Use `terminal_session` command transactions with `until_exit_code` for command sequences that must inherit the same shell state and still return per-command exit codes.
- If you are unsure which terminal exists or you forgot an id, call `terminal_session` with `action: "list"` first.

## Basic Workflow

1. `list` existing terminals when you need to recover names and ids.
2. `start` a named terminal with a shell command such as `pwsh -NoLogo`, `cmd`, `bash`, `python`, or a TUI command.
3. `send` ordered text, paste, keys, or chords.
4. `read` new output after the last delivered cursor.
5. `snapshot` current terminal state without advancing the delivered cursor.
6. `stop` the terminal when it is no longer needed, or use `Ctrl+C` first if preserving the shell matters.

## Actions

### list

Lists terminal sessions owned by the current controller session. Use this before operating if the terminal id or purpose is unclear.

```json
{
  "action": "list"
}
```

The result includes each terminal's `name`, `terminal_session_id`, status, command, shell, working directory, and output counters.

### start

Starts one named persistent terminal. Always provide a short task-oriented `name` when starting more than one terminal. `cwd` only applies when the session starts. Later `send` calls inherit the running process state and do not change `cwd` unless you send a shell command such as `cd`.

```json
{
  "action": "start",
  "name": "install shell",
  "command": "pwsh -NoLogo",
  "shell": "auto"
}
```

### send

Sends ordered inputs to an existing terminal. Use this for commands, REPL input, shortcuts, and TUI navigation.

```json
{
  "action": "send",
  "terminal_session_id": "...",
  "inputs": [{ "kind": "text", "text": "npm test\n" }],
  "wait_policy": { "mode": "until_exit_code", "max_wait_ms": 120000 }
}
```

### read

Reads incremental output after the last delivered cursor. Use this for polling a long-running process or interactive output.

```json
{
  "action": "read",
  "terminal_session_id": "...",
  "wait_policy": { "mode": "until_quiet", "quiet_ms": 300, "max_wait_ms": 3000 }
}
```

### snapshot

Inspects current terminal state without advancing the delivered cursor. Prefer this for TUI or screen-like programs.

```json
{
  "action": "snapshot",
  "terminal_session_id": "...",
  "wait_policy": { "mode": "screen_stable", "quiet_ms": 150, "max_wait_ms": 1000 }
}
```

### stop

Stops the terminal. Use `Ctrl+C` first when you want to interrupt a foreground process but keep the shell available.

```json
{
  "action": "stop",
  "terminal_session_id": "..."
}
```

## Wait Policies

- Use `until_exit_code` for a normal shell command transaction that should return this command's exit code while preserving the session state.
- Use `until_quiet` when output has a natural pause but no reliable prompt.
- Use `until_pattern` for servers or tools that print readiness text.
- Use `screen_stable` for TUI or screen-like output.
- Use `return_immediately` after sending input that should not block.

### return_immediately

Use when input should be sent and control should return right away. Good for starting a long interaction or sending a key that will be followed by a later `read` or `snapshot`.

### until_quiet

Use when output is append-style and then becomes quiet. Set `quiet_ms` around 200-500ms and always set a bounded `max_wait_ms`.

### until_prompt

Use only when a stable shell prompt is expected. Prompt detection can vary across shells and themes, so prefer `until_exit_code` for ordinary command success/failure.

### until_exit_code

Use for a single command transaction inside an existing shell state. It only supports exactly one `text` input containing one complete shell command with a trailing newline.

Good:

```json
{
  "action": "send",
  "terminal_session_id": "...",
  "inputs": [{ "kind": "text", "text": "npm run build\n" }],
  "wait_policy": { "mode": "until_exit_code", "max_wait_ms": 120000 }
}
```

This preserves session state and returns `transaction.exit_code`.

Do not use `until_exit_code` for:

- `paste`, `key`, or `chord` inputs.
- REPL input.
- Interactive prompts that ask follow-up questions.
- Long-running servers.
- TUI or screen-mode programs.
- Half-written command lines.

Use `until_quiet`, `return_immediately`, or `screen_stable` for pasted content, shortcuts, and TUI interaction. The host rejects `paste/key/chord + until_exit_code` because wrapping those inputs as an exit-code transaction can corrupt an interactive shell line.

PowerShell note: the host treats native `$LASTEXITCODE` as authoritative when present. For cmdlet-only commands, the transaction also checks whether new PowerShell errors were added during the command and reports exit code `1` for those non-terminating errors.

Timeout does not stop the session. If timeout occurs, decide whether to `read`, send `Ctrl+C`, or `stop`.

### until_pattern

Use for long-running processes that print a known readiness message, such as `Local:`, `ready`, `listening`, or `Compiled successfully`.

### screen_stable

Use for TUI or screen-like output. It waits for the terminal screen to stop changing briefly, then returns a snapshot-oriented response.

## Input Rules

Use ordered `inputs`; order matters.

- Use `{ "kind": "text", "text": "npm test\n" }` for typed text.
- Use `{ "kind": "paste", "text": "large text" }` for pasted content.
- Use `{ "kind": "key", "key": "ArrowDown" }` for navigation keys.
- Use `{ "kind": "chord", "key": "c", "ctrl": true }` for shortcuts.

Do not hand-write ANSI escape sequences or control characters. The host encodes keys for the current terminal.

### Text

```json
{ "kind": "text", "text": "Get-Date\n" }
```

Use `\n` to submit a command unless the tool response says a different interactive newline is needed.

### Paste

```json
{ "kind": "paste", "text": "multi-line content" }
```

Use paste for larger text blocks. Prefer paste over many tiny text inputs.

Do not combine `paste` with `until_exit_code`. Use `until_quiet` for pasted interactive content, or convert a single ordinary command into one `text` input when you need an exit code.

### Keys

Supported keys include:

```text
Enter, Tab, Esc, Backspace, Delete,
ArrowUp, ArrowDown, ArrowLeft, ArrowRight,
Home, End, PageUp, PageDown,
Insert,
F1 ... F24
```

Enter is encoded by the host for the current interactive shell, but PTY layers and child programs may normalize it. Programs that read raw characters should not assume Enter arrives as `\r`; many stdin readers observe `\n`. Prefer normal line input such as `readline()` or treat both `\r` and `\n` as submit keys.

### Chords

```json
{ "kind": "chord", "key": "c", "ctrl": true }
```

Common chords:

```text
Ctrl+C, Ctrl+D, Ctrl+Z, Ctrl+L, Ctrl+R,
Shift+Tab,
Alt+F, Alt+B
```

Use `Ctrl+C` to interrupt a stuck foreground process before `stop` when preserving the shell matters.

Ctrl+C sends use the host interrupt drain/cleanup path for every wait policy, including `return_immediately`, `until_quiet`, `until_prompt`, and `screen_stable`. Check `interrupt_drain_*` and `interrupt_cleanup_*` fields when diagnosing whether the host consumed post-interrupt output or input-line noise.

After `Ctrl+C`, prefer one stabilizing step before the next tracked command: call `read` with `until_quiet` or `snapshot` with `screen_stable` and verify that the shell prompt or expected application state has returned. The host cleanup path is designed to prevent the interrupt byte from polluting the next `until_exit_code` wrapper, but Windows PowerShell can still surface `\x03` as a standalone echoed/control line.

If a post-interrupt response contains a standalone `\x03`, `^C`, or a PowerShell "term is not recognized" message for that control character, treat it as benign interrupt cleanup noise when the next `until_exit_code` transaction reports the expected exit code. Do not retry destructive commands only because of that cleanup noise.

Windows PTY note: Ctrl+C may not reliably terminate every foreground program under PowerShell/ConPTY. If the foreground process remains alive after drain/cleanup, use `stop` for the terminal session or another host-level termination path instead of assuming the interrupt succeeded.

## Output Modes

Check `output_mode` on every response.

- `log`: append-style terminal output; read and summarize normally.
- `screen`: TUI or alternate-screen output; inspect `screen_text` and use keys/chords.
- `mixed`: treat as partly screen-like; avoid assuming raw output is a clean log.
- `unknown`: proceed conservatively and use short waits or snapshots.

If the mode is `screen` or `mixed`, drive the program with keys/chords instead of text commands unless the screen clearly shows a command prompt.

### log

Append-style output. Treat returned events as normal terminal text. This is the usual mode for shells, builds, tests, and installs.

### screen

Screen-style output, usually TUI or alternate screen. Do not summarize raw ANSI logs as if they were normal text. Inspect `screen_text`, cursor information, and `controls_hint`.

For `screen` and `mixed` responses, the model-visible result includes a `[screen_text]` section when text can be extracted. The structured host payload also carries `screen_text` and `controls_hint`. Prefer those fields over raw `[stdout]` chunks when deciding where the cursor is or which key to press next.

### mixed

The terminal has both log-like and screen-like behavior. Prefer snapshots and conservative key inputs.

### unknown

The host cannot confidently classify the output yet. Use bounded waits, `snapshot`, or a small `read`.

## Command Transactions

Use `until_exit_code` when you need a command's result in an existing session, for example after activating a virtual environment or changing directories. The host returns `transaction.exit_code` for that send.

Do not use `until_exit_code` for paste/key/chord inputs, REPL prompts, interactive questions, long-running services, or TUI programs.

Before using `send` on an existing terminal after context loss, call `list` and choose by `name` plus `status`, then use that entry's `terminal_session_id`.

## TUI Control

TUI output represents a screen, not a log. Use `snapshot` or `screen_stable` to inspect current state.

Control strategy:

1. Read `screen_text`.
2. Check `cursor` and `controls_hint`.
3. Use `key` or `chord` inputs for navigation.
4. Prefer small steps: one or two keys, then snapshot again.
5. Use `Esc` or `Ctrl+C` to exit or interrupt when appropriate.

Cursor position can be approximate when output is complex. If the screen is unclear, use conservative navigation keys and verify with `snapshot`.

Do not send ANSI escapes directly. Use standardized keys and chords.

Windows PTY limitation: PowerShell `[Console]::ReadKey()` and Python `msvcrt.getch()` can fail when stdin is redirected through the host PTY. Prefer TUIs and scripts that read from standard input using `sys.stdin.read*`, normal line input, or supported key/chord events. If a program requires a real console keyboard API, treat it as unsupported in `terminal_session` unless a wrapper can translate stdin into the program's expected input.

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

### List Existing Terminals

```json
{
  "action": "list"
}
```

### Run A Command Transaction

```json
{
  "action": "send",
  "terminal_session_id": "...",
  "inputs": [{ "kind": "text", "text": "npm test\n" }],
  "wait_policy": { "mode": "until_exit_code", "max_wait_ms": 120000 }
}
```

### Read Incremental Output

```json
{
  "action": "read",
  "terminal_session_id": "...",
  "wait_policy": { "mode": "until_quiet", "quiet_ms": 300, "max_wait_ms": 3000 }
}
```

### Stop A Session

```json
{
  "action": "stop",
  "terminal_session_id": "..."
}
```

### TUI Navigation

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
