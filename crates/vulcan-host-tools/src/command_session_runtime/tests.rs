use super::{
    COMMAND_SESSION_OUTPUT_CHUNK_COALESCE_SOFT_LIMIT_BYTES, CommandSessionBackend,
    CommandSessionOutputChunk, CommandSessionOutputStream, CommandSessionRecord,
    CommandSessionRuntime, CommandSessionSharedState, CommandSessionStatus, PtyScreenFrameState,
    can_coalesce_output_chunk, output_chunk_ends_with_line_boundary,
};
#[cfg(windows)]
use super::{CommandSessionInputEvent, CommandSessionStartRequest};
use crate::model::SessionId;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc::unbounded_channel;

/// Output coalescing should keep appending to one undelivered line fragment until the line is completed.
/// 输出合并应持续追加到同一条未投递行碎片，直到该行被完整结束。
#[test]
fn output_chunk_coalescing_prefers_undelivered_partial_lines() {
    let last_chunk = CommandSessionOutputChunk {
        seq: 3,
        stream: CommandSessionOutputStream::Stdout,
        text: String::from("partial"),
    };
    assert!(can_coalesce_output_chunk(
        Some(&last_chunk),
        2,
        CommandSessionOutputStream::Stdout,
        8,
    ));
    assert!(!can_coalesce_output_chunk(
        Some(&last_chunk),
        3,
        CommandSessionOutputStream::Stdout,
        8,
    ));
    assert!(!can_coalesce_output_chunk(
        Some(&last_chunk),
        2,
        CommandSessionOutputStream::Stderr,
        8,
    ));
}

/// Output coalescing should stop once the prior chunk already ended with a line boundary or the soft size limit would be exceeded.
/// 一旦前一个分片已经结束在行边界，或继续合并会超过软大小阈值，输出合并就应停止。
#[test]
fn output_chunk_coalescing_respects_line_boundaries_and_soft_limit() {
    let line_chunk = CommandSessionOutputChunk {
        seq: 4,
        stream: CommandSessionOutputStream::Stdout,
        text: String::from("line complete\n"),
    };
    assert!(output_chunk_ends_with_line_boundary(&line_chunk.text));
    assert!(!can_coalesce_output_chunk(
        Some(&line_chunk),
        0,
        CommandSessionOutputStream::Stdout,
        1,
    ));

    let oversized_chunk = CommandSessionOutputChunk {
        seq: 5,
        stream: CommandSessionOutputStream::Stdout,
        text: "x".repeat(COMMAND_SESSION_OUTPUT_CHUNK_COALESCE_SOFT_LIMIT_BYTES),
    };
    assert!(!can_coalesce_output_chunk(
        Some(&oversized_chunk),
        0,
        CommandSessionOutputStream::Stdout,
        1,
    ));
}

/// PTY frame synthesis should keep a rolling live screen for WebUI display.
/// PTY 帧综合应为 WebUI 展示保留滚动实时屏幕。
#[test]
fn pty_screen_frame_state_builds_rolling_live_frame() {
    let mut state = PtyScreenFrameState::new(3, 80);
    let first = state
        .push_output("one\n\x1b[31mtwo\x1b[0m\nthree")
        .expect("first frame");
    assert_eq!(first.0, 1);
    assert_eq!(first.1, 3);
    assert_eq!(first.2, 80);
    assert!(first.3.contains("\x1b[31mtwo\x1b[0m"));
    assert!(first.3.contains("three"));

    let second = state.push_output("\rTHREE\nfour").expect("second frame");
    assert_eq!(second.0, 2);
    assert!(!second.3.contains("one"));
    assert!(second.3.contains("THREE"));
    assert!(second.3.contains("four"));
}

/// Best-effort live snapshots should expose the latest shared state to synchronous WebUI callers without crossing ownership boundaries.
/// 尽力而为的实时快照应在不跨越归属边界的前提下，把最新共享状态暴露给同步 WebUI 调用方。
#[test]
fn try_snapshot_owned_session_returns_live_state_without_async_wait() {
    let runtime = CommandSessionRuntime::new();
    let command_session_id = SessionId::from_u128(0xCA11);
    let owner_session_id = SessionId::from_u128(0x0A11);
    let shared_state = Arc::new(CommandSessionSharedState::default());
    {
        let mut state = shared_state
            .mutable_state
            .try_lock()
            .expect("lock shared state");
        state.status = CommandSessionStatus::Stopped;
        state.exit_code = Some(9);
        state.failure_reason = Some(String::from("stopped by test"));
        state.stdout_bytes = 128;
        state.stderr_bytes = 16;
        state.last_seq = 5;
        state.pending_stdin_line = true;
    }
    let (control_sender, _control_receiver) = unbounded_channel();
    runtime
        .sessions
        .write()
        .expect("lock runtime registry")
        .insert(
            command_session_id,
            Arc::new(CommandSessionRecord {
                command_session_id,
                owner_session_id,
                display_name: None,
                command: String::from("echo live"),
                shell_label: String::from("pwsh"),
                launcher_shell_label: String::from("pwsh"),
                interactive_newline: String::from("\r\n"),
                cwd: PathBuf::from(r"D:\openvulcan\vulcan-code"),
                backend: CommandSessionBackend::Pipe,
                control_sender,
                shared_state,
            }),
        );

    let snapshot = runtime
        .try_snapshot_owned_session(owner_session_id, command_session_id)
        .expect("read live snapshot");
    assert_eq!(snapshot.command_session_id, command_session_id);
    assert_eq!(snapshot.owner_session_id, owner_session_id);
    assert_eq!(snapshot.status, CommandSessionStatus::Stopped);
    assert_eq!(snapshot.exit_code, Some(9));
    assert_eq!(snapshot.failure_reason.as_deref(), Some("stopped by test"));
    assert_eq!(snapshot.stdout_bytes, 128);
    assert_eq!(snapshot.stderr_bytes, 16);
    assert_eq!(snapshot.last_seq, 5);
    assert!(snapshot.pending_stdin_line);
    assert!(
        runtime
            .try_snapshot_owned_session(SessionId::from_u128(0x0A12), command_session_id)
            .is_none()
    );
    assert!(
        runtime
            .try_snapshot_session_by_id(command_session_id)
            .is_some()
    );
}

/// Peeking should not advance the AI-facing cursor used by terminal_session reads.
/// 窥视读取不应推进 terminal_session 使用的面向 AI 的读取游标。
#[tokio::test]
async fn peek_session_for_ai_does_not_advance_ai_cursor() {
    let runtime = CommandSessionRuntime::new();
    let command_session_id = SessionId::from_u128(0xCA12);
    let owner_session_id = SessionId::from_u128(0x0A13);
    let shared_state = Arc::new(CommandSessionSharedState::default());
    let (control_sender, _control_receiver) = unbounded_channel();
    runtime
        .sessions
        .write()
        .expect("lock runtime registry")
        .insert(
            command_session_id,
            Arc::new(CommandSessionRecord {
                command_session_id,
                owner_session_id,
                display_name: Some(String::from("peek test")),
                command: String::from("echo visible"),
                shell_label: String::from("pwsh"),
                launcher_shell_label: String::from("pwsh"),
                interactive_newline: String::from("\r\n"),
                cwd: PathBuf::from(r"D:\openvulcan\vulcan-code"),
                backend: CommandSessionBackend::Pipe,
                control_sender,
                shared_state: Arc::clone(&shared_state),
            }),
        );
    shared_state
        .append_output_chunk(
            CommandSessionOutputStream::Stdout,
            String::from("visible\n"),
        )
        .await;

    let peeked = runtime
        .peek_session_for_ai(
            owner_session_id,
            command_session_id,
            None,
            false,
            Some(0),
            None,
        )
        .await
        .expect("peek should read buffered output");
    assert_eq!(peeked.returned_chunks.len(), 1);
    assert_eq!(peeked.returned_chunks[0].text, "visible\n");

    let read = runtime
        .read_session_for_ai(
            owner_session_id,
            command_session_id,
            None,
            false,
            Some(0),
            None,
        )
        .await
        .expect("normal read should still see peeked output");
    assert_eq!(read.returned_chunks.len(), 1);
    assert_eq!(read.returned_chunks[0].text, "visible\n");

    let second_read = runtime
        .read_session_for_ai(
            owner_session_id,
            command_session_id,
            None,
            false,
            Some(0),
            None,
        )
        .await
        .expect("second normal read should consume no duplicate output");
    assert!(second_read.returned_chunks.is_empty());
}

/// Verifies the Windows console backend delivers extended keys to `msvcrt.getch`.
/// 验证 Windows 控制台后端能把扩展按键传递给 `msvcrt.getch`。
#[cfg(windows)]
#[tokio::test]
async fn windows_console_backend_delivers_msvcrt_arrow_keys() {
    if super::resolve_windows_console_helper_path().is_err() {
        return;
    }
    let python = match resolve_test_python_executable() {
        Some(python) => python,
        None => return,
    };
    let runtime = Arc::new(CommandSessionRuntime::new());
    let owner_session_id = SessionId::from_u128(0x0BEE);
    let script = concat!(
        "import msvcrt, sys\n",
        "items=['one','two','three']\n",
        "idx=0\n",
        "sys.stdout.write('ready\\n')\n",
        "sys.stdout.flush()\n",
        "while True:\n",
        "    ch=msvcrt.getch()\n",
        "    if ch in (b'\\x00', b'\\xe0'):\n",
        "        ext=msvcrt.getch()\n",
        "        if ext == b'P':\n",
        "            idx=(idx+1)%len(items)\n",
        "            sys.stdout.write('index=%d\\n' % idx)\n",
        "            sys.stdout.flush()\n",
        "        elif ext == b'H':\n",
        "            idx=(idx-1)%len(items)\n",
        "            sys.stdout.write('index=%d\\n' % idx)\n",
        "            sys.stdout.flush()\n",
        "    elif ch in (b'\\r', b'\\n'):\n",
        "        sys.stdout.write('selected=%s\\n' % items[idx])\n",
        "        sys.stdout.flush()\n",
        "        break\n",
    );
    let snapshot = runtime
        .start_session(CommandSessionStartRequest {
            owner_session_id,
            display_name: Some(String::from("msvcrt test")),
            command: format!("{python} -c <msvcrt getch>"),
            shell_label: String::from("python"),
            launcher_shell_label: String::from("python"),
            cwd: std::env::current_dir().expect("current dir"),
            launcher_program: python,
            launcher_args: vec![String::from("-c"), String::from(script)],
            initial_stdin: None,
            initial_stdin_append_newline: false,
            interactive_newline: String::from("\r\n"),
            backend: CommandSessionBackend::WindowsConsole,
        })
        .await
        .expect("start windows console session");

    let (ready_output, ready_seq) = read_test_session_until_text(
        &runtime,
        owner_session_id,
        snapshot.command_session_id,
        0,
        "ready",
    )
    .await
    .expect("read ready");
    assert!(
        ready_output.contains("ready"),
        "expected ready output, got {ready_output:?}"
    );
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    runtime
        .write_session_input_events(
            owner_session_id,
            snapshot.command_session_id,
            vec![
                CommandSessionInputEvent::Key {
                    key: String::from("ArrowDown"),
                },
                CommandSessionInputEvent::Key {
                    key: String::from("Enter"),
                },
            ],
        )
        .await
        .expect("write getch input events");
    let (output, _) = read_test_session_until_text(
        &runtime,
        owner_session_id,
        snapshot.command_session_id,
        ready_seq,
        "selected=two",
    )
    .await
    .expect("read getch result");
    assert!(
        output.contains("selected=two"),
        "unexpected output: {output:?}"
    );
}

/// Verifies Windows console text input can deliver ANSI VT escape bytes to stdin readers.
/// 验证 Windows 控制台文本输入可向 stdin 读取器传递 ANSI VT 转义字节。
#[cfg(windows)]
#[tokio::test]
async fn windows_console_backend_delivers_ansi_vt_text_to_stdin_readers() {
    if super::resolve_windows_console_helper_path().is_err() {
        return;
    }
    let python = match resolve_test_python_executable() {
        Some(python) => python,
        None => return,
    };
    let runtime = Arc::new(CommandSessionRuntime::new());
    let owner_session_id = SessionId::from_u128(0x0BEF);
    let script = concat!(
        "import sys\n",
        "sys.stdout.write('ready\\n')\n",
        "sys.stdout.flush()\n",
        "values=[]\n",
        "while len(values) < 3:\n",
        "    ch=sys.stdin.read(1)\n",
        "    if not ch:\n",
        "        break\n",
        "    values.append(ord(ch))\n",
        "sys.stdout.write('codes=' + ','.join(str(v) for v in values) + '\\n')\n",
        "sys.stdout.flush()\n",
    );
    let snapshot = runtime
        .start_session(CommandSessionStartRequest {
            owner_session_id,
            display_name: Some(String::from("stdin vt test")),
            command: format!("{python} -c <stdin vt>"),
            shell_label: String::from("python"),
            launcher_shell_label: String::from("python"),
            cwd: std::env::current_dir().expect("current dir"),
            launcher_program: python,
            launcher_args: vec![String::from("-c"), String::from(script)],
            initial_stdin: None,
            initial_stdin_append_newline: false,
            interactive_newline: String::from("\r\n"),
            backend: CommandSessionBackend::WindowsConsole,
        })
        .await
        .expect("start windows console session");
    let (_, ready_seq) = read_test_session_until_text(
        &runtime,
        owner_session_id,
        snapshot.command_session_id,
        0,
        "ready",
    )
    .await
    .expect("read ready");
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    runtime
        .write_session_input_events(
            owner_session_id,
            snapshot.command_session_id,
            vec![CommandSessionInputEvent::Text {
                text: String::from("\x1b[B\n"),
            }],
        )
        .await
        .expect("write ansi vt text input");
    let (output, _) = read_test_session_until_text(
        &runtime,
        owner_session_id,
        snapshot.command_session_id,
        ready_seq,
        "codes=27,91,66",
    )
    .await
    .expect("read stdin result");
    assert!(
        output.contains("codes=27,91,66"),
        "unexpected output: {output:?}"
    );
}

/// Reads one test command session until the expected text appears or the retry budget is exhausted.
/// 读取测试命令会话，直到出现预期文本或耗尽重试预算。
#[cfg(windows)]
async fn read_test_session_until_text(
    runtime: &Arc<CommandSessionRuntime>,
    owner_session_id: SessionId,
    command_session_id: SessionId,
    mut after_seq: u64,
    expected_text: &str,
) -> Option<(String, u64)> {
    let mut output = String::new();
    for _ in 0..20 {
        let result = runtime
            .read_session(
                owner_session_id,
                command_session_id,
                Some(after_seq),
                Some(250),
                Some(32),
            )
            .await
            .ok()?;
        for chunk in &result.returned_chunks {
            after_seq = after_seq.max(chunk.seq);
            output.push_str(&chunk.text);
        }
        if output.contains(expected_text) || result.snapshot.status != CommandSessionStatus::Running
        {
            return Some((output, after_seq));
        }
    }
    Some((output, after_seq))
}

/// Resolves a Python executable for Windows console-input regression tests.
/// 为 Windows 控制台输入回归测试解析 Python 可执行程序。
#[cfg(windows)]
fn resolve_test_python_executable() -> Option<String> {
    for candidate in ["python", "py"] {
        let output = std::process::Command::new(candidate)
            .arg("--version")
            .output();
        if output.is_ok_and(|output| output.status.success()) {
            return Some(candidate.to_string());
        }
    }
    None
}
