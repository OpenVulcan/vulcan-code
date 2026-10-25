use super::{
    COMMAND_SESSION_CONTROL_QUEUE_LIMIT, COMMAND_SESSION_OUTPUT_CHUNK_COALESCE_SOFT_LIMIT_BYTES,
    COMMAND_SESSION_RETAINED_BYTE_LIMIT, COMMAND_SESSION_RETAINED_CHUNK_LIMIT,
    COMMAND_SESSION_RETAINED_FINISHED_LIMIT, CommandSessionBackend, CommandSessionDisplayCursor,
    CommandSessionInputEvent, CommandSessionMutableState, CommandSessionOutputChunk,
    CommandSessionOutputStream, CommandSessionRecord, CommandSessionRuntime,
    CommandSessionSharedState, CommandSessionStatus, DEFAULT_PTY_SCREEN_COLS,
    DEFAULT_PTY_SCREEN_ROWS, MAX_COMMAND_SESSION_READ_CHUNKS, MAX_COMMAND_SESSION_READ_WAIT_MS,
    PtyScreenFrameState, append_output_chunk_to_state, bounded_command_session_read_chunks,
    bounded_command_session_read_wait_ms, can_coalesce_output_chunk,
    encode_command_session_input_events, output_chunk_ends_with_line_boundary,
};
use crate::model::SessionId;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc::channel;
use tokio::time::{Duration, timeout};

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

/// Retained command-session output should obey both fragment and byte limits and expose one cursor gap.
/// 命令会话保留输出应同时遵守分片与字节上限，并暴露一个游标缺口。
#[tokio::test]
async fn retained_command_session_output_is_bounded_and_reports_a_gap() {
    let mut state = CommandSessionMutableState::default();
    for _ in 0..(COMMAND_SESSION_RETAINED_CHUNK_LIMIT + 2) {
        append_output_chunk_to_state(
            &mut state,
            CommandSessionOutputStream::Stdout,
            String::from("x\n"),
        );
    }
    assert_eq!(state.chunks.len(), COMMAND_SESSION_RETAINED_CHUNK_LIMIT);
    assert_eq!(
        state.retained_output_bytes,
        COMMAND_SESSION_RETAINED_CHUNK_LIMIT * 2
    );
    assert_eq!(state.dropped_before_seq, 2);

    let mut byte_bounded_state = CommandSessionMutableState::default();
    append_output_chunk_to_state(
        &mut byte_bounded_state,
        CommandSessionOutputStream::Stdout,
        "x".repeat(COMMAND_SESSION_RETAINED_BYTE_LIMIT),
    );
    append_output_chunk_to_state(
        &mut byte_bounded_state,
        CommandSessionOutputStream::Stdout,
        String::from("\n"),
    );
    assert_eq!(byte_bounded_state.chunks.len(), 1);
    assert_eq!(byte_bounded_state.retained_output_bytes, 1);
    assert_eq!(byte_bounded_state.dropped_before_seq, 1);

    let shared_state = Arc::new(CommandSessionSharedState::default());
    *shared_state.mutable_state.lock().await = state;
    let (control_sender, _control_receiver) = channel(COMMAND_SESSION_CONTROL_QUEUE_LIMIT);
    let record = CommandSessionRecord {
        command_session_id: SessionId::from_u128(0xCA15),
        admission_seq: 0,
        owner_session_id: SessionId::from_u128(0x0A16),
        display_name: Some(String::from("bounded output test")),
        command: String::from("echo bounded"),
        shell_label: String::from("pwsh"),
        launcher_shell_label: String::from("pwsh"),
        interactive_newline: String::from("\r\n"),
        cwd: PathBuf::from(r"D:\openvulcan\vulcan-code"),
        backend: CommandSessionBackend::Pty,
        environment_fingerprint: String::new(),
        control_sender,
        shared_state,
    };
    let outcome = record.read_since(0, 2, false).await;
    assert_eq!(outcome.snapshot.dropped_before_seq, 2);
    assert_eq!(outcome.returned_chunks[0].seq, 2);
    assert!(outcome.returned_chunks[0].text.contains("was evicted"));
    assert_eq!(outcome.returned_chunks[1].seq, 3);
    assert!(outcome.has_more);
}

/// A visible display cursor should prevent later fragments from mutating an already rendered sequence.
/// 可见显示游标应阻止后续片段修改已经渲染的序号。
#[tokio::test]
async fn display_bootstrap_freezes_rendered_chunk_sequence() {
    let runtime = CommandSessionRuntime::new();
    let command_session_id = SessionId::from_u128(0xCA17);
    let owner_session_id = SessionId::from_u128(0x0A18);
    let shared_state = Arc::new(CommandSessionSharedState::default());
    let (control_sender, _control_receiver) = channel(COMMAND_SESSION_CONTROL_QUEUE_LIMIT);
    runtime
        .sessions
        .write()
        .expect("lock runtime registry")
        .insert(
            command_session_id,
            Arc::new(CommandSessionRecord {
                command_session_id,
                admission_seq: 0,
                owner_session_id,
                display_name: Some(String::from("display cursor test")),
                command: String::from("echo partial"),
                shell_label: String::from("pwsh"),
                launcher_shell_label: String::from("pwsh"),
                interactive_newline: String::from("\r\n"),
                cwd: PathBuf::from(r"D:\openvulcan\vulcan-code"),
                backend: CommandSessionBackend::Pty,
                environment_fingerprint: String::new(),
                control_sender,
                shared_state: Arc::clone(&shared_state),
            }),
        );
    shared_state
        .append_output_chunk(CommandSessionOutputStream::Stdout, String::from("partial"))
        .await;
    let attachment = runtime
        .read_recent_session_for_display(owner_session_id, command_session_id, 8)
        .await
        .expect("read display bootstrap");
    assert_eq!(attachment.returned_chunks[0].seq, 1);

    shared_state
        .append_output_chunk(CommandSessionOutputStream::Stdout, String::from(" tail"))
        .await;
    let changed = runtime
        .wait_for_session_display_change(
            owner_session_id,
            command_session_id,
            &CommandSessionDisplayCursor {
                after_seq: 1,
                running_was_observed: true,
                screen_frame_seq: None,
                failure_reason: None,
            },
            8,
        )
        .await
        .expect("read display increment");
    assert_eq!(changed.returned_chunks.len(), 1);
    assert_eq!(changed.returned_chunks[0].seq, 2);
    assert_eq!(changed.returned_chunks[0].text, " tail");
    assert_eq!(shared_state.mutable_state.lock().await.ai_delivered_seq, 0);
}

/// PTY frame synthesis should keep a complete formatted VT checkpoint for WebUI display.
/// PTY 帧综合应为 WebUI 展示保留完整的格式化 VT 检查点。
#[test]
fn pty_screen_frame_state_builds_formatted_vt_checkpoint() {
    let mut state = PtyScreenFrameState::new(3, 80);
    let first = state
        .push_output("one\n\x1b[31mtwo\x1b[0m\nthree")
        .expect("first frame");
    assert_eq!(first.0, 1);
    assert_eq!(first.1, 3);
    assert_eq!(first.2, 80);
    assert!(first.3.contains("\x1b["));
    assert!(first.3.contains("two"));
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
    let (control_sender, _control_receiver) = channel(COMMAND_SESSION_CONTROL_QUEUE_LIMIT);
    runtime
        .sessions
        .write()
        .expect("lock runtime registry")
        .insert(
            command_session_id,
            Arc::new(CommandSessionRecord {
                command_session_id,
                admission_seq: 0,
                owner_session_id,
                display_name: None,
                command: String::from("echo live"),
                shell_label: String::from("pwsh"),
                launcher_shell_label: String::from("pwsh"),
                interactive_newline: String::from("\r\n"),
                cwd: PathBuf::from(r"D:\openvulcan\vulcan-code"),
                backend: CommandSessionBackend::Pty,
                environment_fingerprint: String::new(),
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

/// Display waits should react to screen-frame and failure metadata without advancing the AI output cursor.
/// 显示等待应响应屏幕帧与故障元数据，同时不推进 AI 输出游标。
#[tokio::test]
async fn display_wait_observes_frame_and_failure_metadata_without_output() {
    // runtime owns one running fixture through the same registry used by visible display subscriptions.
    // runtime 通过可见显示订阅使用的同一注册表持有一个运行中夹具。
    let runtime = CommandSessionRuntime::new();
    // command_session_id and owner_session_id define the exact ownership boundary under test.
    // command_session_id 与 owner_session_id 定义本测试的精确归属边界。
    let command_session_id = SessionId::from_u128(0xCA13);
    let owner_session_id = SessionId::from_u128(0x0A14);
    // shared_state receives metadata without receiving any output chunk.
    // shared_state 接收元数据，但不接收任何输出分片。
    let shared_state = Arc::new(CommandSessionSharedState::default());
    let (control_sender, _control_receiver) = channel(COMMAND_SESSION_CONTROL_QUEUE_LIMIT);
    runtime
        .sessions
        .write()
        .expect("lock runtime registry")
        .insert(
            command_session_id,
            Arc::new(CommandSessionRecord {
                command_session_id,
                admission_seq: 0,
                owner_session_id,
                display_name: Some(String::from("display metadata test")),
                command: String::from("echo metadata"),
                shell_label: String::from("pwsh"),
                launcher_shell_label: String::from("pwsh"),
                interactive_newline: String::from("\r\n"),
                cwd: PathBuf::from(r"D:\openvulcan\vulcan-code"),
                backend: CommandSessionBackend::Pty,
                environment_fingerprint: String::new(),
                control_sender,
                shared_state: Arc::clone(&shared_state),
            }),
        );
    shared_state
        .update_screen_frame(7, 24, 80, String::from("formatted screen"))
        .await;
    shared_state
        .remember_failure_reason(String::from("terminal host metadata failure"))
        .await;

    // outcome must return from metadata comparison even though status and output cursor are unchanged.
    // outcome 必须通过元数据比较返回，即使状态与输出游标都未变化。
    let outcome = timeout(
        Duration::from_millis(100),
        runtime.wait_for_session_display_change(
            owner_session_id,
            command_session_id,
            &CommandSessionDisplayCursor {
                after_seq: 0,
                running_was_observed: true,
                screen_frame_seq: None,
                failure_reason: None,
            },
            1,
        ),
    )
    .await
    .expect("display metadata must satisfy the wait")
    .expect("display change outcome");
    assert!(outcome.returned_chunks.is_empty());
    assert_eq!(outcome.snapshot.screen_frame_seq, Some(7));
    assert_eq!(
        outcome.snapshot.failure_reason.as_deref(),
        Some("terminal host metadata failure")
    );
    assert_eq!(shared_state.mutable_state.lock().await.ai_delivered_seq, 0);
}

/// Peeking should not advance the AI-facing cursor used by terminal_session reads.
/// 窥视读取不应推进 terminal_session 使用的面向 AI 的读取游标。
#[tokio::test]
async fn peek_session_for_ai_does_not_advance_ai_cursor() {
    let runtime = CommandSessionRuntime::new();
    let command_session_id = SessionId::from_u128(0xCA12);
    let owner_session_id = SessionId::from_u128(0x0A13);
    let shared_state = Arc::new(CommandSessionSharedState::default());
    let (control_sender, _control_receiver) = channel(COMMAND_SESSION_CONTROL_QUEUE_LIMIT);
    runtime
        .sessions
        .write()
        .expect("lock runtime registry")
        .insert(
            command_session_id,
            Arc::new(CommandSessionRecord {
                command_session_id,
                admission_seq: 0,
                owner_session_id,
                display_name: Some(String::from("peek test")),
                command: String::from("echo visible"),
                shell_label: String::from("pwsh"),
                launcher_shell_label: String::from("pwsh"),
                interactive_newline: String::from("\r\n"),
                cwd: PathBuf::from(r"D:\openvulcan\vulcan-code"),
                backend: CommandSessionBackend::Pty,
                environment_fingerprint: String::new(),
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

/// Command-session read parameters should remain bounded even when a caller supplies unreasonably large values.
/// 即使调用方提供异常大的值，命令会话读取参数也应保持有界。
#[test]
fn command_session_read_parameters_are_hard_bounded() {
    assert_eq!(bounded_command_session_read_wait_ms(None), 0);
    assert_eq!(bounded_command_session_read_chunks(None), 128);
    assert_eq!(bounded_command_session_read_chunks(Some(0)), 1);
    assert_eq!(
        bounded_command_session_read_wait_ms(Some(u64::MAX)),
        MAX_COMMAND_SESSION_READ_WAIT_MS
    );
    assert_eq!(
        bounded_command_session_read_chunks(Some(usize::MAX)),
        MAX_COMMAND_SESSION_READ_CHUNKS
    );
}

/// Structured terminal events should encode to one deterministic cross-platform ANSI/VT byte stream.
/// 结构化终端事件应编码为确定且跨平台一致的 ANSI/VT 字节流。
#[test]
fn structured_terminal_input_events_encode_cross_platform_ansi_vt() {
    let encoded = encode_command_session_input_events(
        &[
            CommandSessionInputEvent::Text {
                text: String::from("run"),
            },
            CommandSessionInputEvent::Paste {
                text: String::from(" pasted"),
            },
            CommandSessionInputEvent::Key {
                key: String::from("Enter"),
            },
            CommandSessionInputEvent::Key {
                key: String::from("ArrowDown"),
            },
            CommandSessionInputEvent::Key {
                key: String::from("Backspace"),
            },
            CommandSessionInputEvent::Chord {
                key: String::from("h"),
                ctrl: true,
                alt: false,
                shift: false,
                meta: false,
            },
            CommandSessionInputEvent::Chord {
                key: String::from("c"),
                ctrl: true,
                alt: false,
                shift: false,
                meta: false,
            },
            CommandSessionInputEvent::Chord {
                key: String::from("Tab"),
                ctrl: false,
                alt: false,
                shift: true,
                meta: false,
            },
        ],
        "\r\n",
    )
    .expect("encode terminal inputs");

    assert_eq!(encoded, b"run pasted\r\n\x1b[B\x7f\x08\x03\x1b[Z");
}

/// Text and paste events should preserve ordering while normalizing every source newline convention.
/// 文本与粘贴事件应保留顺序，同时统一所有来源换行约定。
#[test]
fn structured_terminal_text_and_paste_normalize_shell_newlines_in_order() {
    // events mixes LF, CR, and CRLF across adjacent text and paste boundaries.
    // events 在相邻文本与粘贴边界中混合 LF、CR 与 CRLF。
    let events = [
        CommandSessionInputEvent::Text {
            text: String::from("text-a\ntext-b\rtext-c\r\n"),
        },
        CommandSessionInputEvent::Paste {
            text: String::from("paste-a\r\npaste-b\n"),
        },
        CommandSessionInputEvent::Text {
            text: String::from("tail"),
        },
    ];

    // windowsEncoded proves the single-CR Windows PTY contract without changing event order.
    // windowsEncoded 验证单 CR 的 Windows PTY 合同，同时不改变事件顺序。
    let windows_encoded =
        encode_command_session_input_events(&events, "\r").expect("encode Windows terminal inputs");
    assert_eq!(
        windows_encoded,
        b"text-a\rtext-b\rtext-c\rpaste-a\rpaste-b\rtail",
    );

    // posixEncoded proves the LF POSIX PTY contract over the identical structured events.
    // posixEncoded 验证相同结构化事件上的 LF POSIX PTY 合同。
    let posix_encoded =
        encode_command_session_input_events(&events, "\n").expect("encode POSIX terminal inputs");
    assert_eq!(
        posix_encoded,
        b"text-a\ntext-b\ntext-c\npaste-a\npaste-b\ntail",
    );

    // crlfEncoded proves every source convention produces exactly one two-byte target newline.
    // crlfEncoded 验证每种来源约定都只生成一个双字节目标换行。
    let crlf_encoded =
        encode_command_session_input_events(&events, "\r\n").expect("encode CRLF terminal inputs");
    assert_eq!(
        crlf_encoded,
        b"text-a\r\ntext-b\r\ntext-c\r\npaste-a\r\npaste-b\r\ntail",
    );
}

/// Newline normalization should preserve Unicode, consecutive empty lines, and empty payloads exactly.
/// 换行归一化应精确保留 Unicode、连续空行与空载荷。
#[test]
fn structured_terminal_text_normalization_preserves_unicode_and_empty_lines() {
    // encoded contains Chinese, Emoji, two logical empty lines, and a trailing empty event.
    // encoded 包含中文、Emoji、两个逻辑空行以及一个末尾空事件。
    let encoded = encode_command_session_input_events(
        &[
            CommandSessionInputEvent::Text {
                text: String::from("中文🙂\n\r\n\rend"),
            },
            CommandSessionInputEvent::Paste {
                text: String::new(),
            },
        ],
        "\r",
    )
    .expect("encode Unicode terminal inputs");
    assert_eq!(encoded, "中文🙂\r\r\rend".as_bytes());
}

/// PTY state should expose its authoritative grid before output without claiming a confirmed screen frame.
/// PTY 状态应在输出前暴露权威网格，同时不得声称已经存在确认屏幕帧。
#[tokio::test]
async fn pty_initial_state_exposes_fixed_grid_before_first_frame() {
    // sharedState uses the same initial dimensions passed to the isolated terminal host.
    // sharedState 使用传给隔离终端宿主的相同初始尺寸。
    let shared_state =
        CommandSessionSharedState::new_pty(DEFAULT_PTY_SCREEN_ROWS, DEFAULT_PTY_SCREEN_COLS);
    // state is read atomically exactly like a command-session snapshot.
    // state 像命令会话快照一样被原子读取。
    let state = shared_state.mutable_state.lock().await;
    assert_eq!(state.screen_rows, Some(DEFAULT_PTY_SCREEN_ROWS));
    assert_eq!(state.screen_cols, Some(DEFAULT_PTY_SCREEN_COLS));
    assert_eq!(state.screen_frame_seq, None);
    assert_eq!(state.screen_text, None);
}

/// Unsupported structured keys should fail explicitly instead of guessing a platform-specific byte sequence.
/// 不受支持的结构化按键应明确失败，而不是猜测平台特定字节序列。
#[test]
fn structured_terminal_input_events_reject_unknown_keys() {
    let error = encode_command_session_input_events(
        &[CommandSessionInputEvent::Key {
            key: String::from("VolumeUp"),
        }],
        "\n",
    )
    .expect_err("unknown key must fail");

    assert!(error.contains("Unsupported terminal key `VolumeUp`"));
}

/// A host stop marks the status immediately, while only process finalization records reaping.
/// 宿主停止会立即标记状态，而只有进程收尾才会记录回收。
#[test]
fn stop_marking_does_not_claim_process_reaping() {
    let mut state = CommandSessionMutableState::default();
    super::mark_stop_requested_state(&mut state);
    assert_eq!(state.status, CommandSessionStatus::Stopped);
    assert!(!state.process_reaped);
    super::finalize_process_exit_state(&mut state, true, None, None, false);
    assert_eq!(state.status, CommandSessionStatus::Stopped);
    assert!(state.process_reaped);
}

/// Builds one registry record with the given admission order and lifecycle status.
/// 构建一条带指定准入顺序与生命周期状态的注册表记录。
fn retention_test_record(
    admission_seq: u64,
    status: CommandSessionStatus,
) -> Arc<CommandSessionRecord> {
    let shared_state = Arc::new(CommandSessionSharedState::default());
    shared_state
        .mutable_state
        .try_lock()
        .expect("lock shared state")
        .status = status;
    let (control_sender, _control_receiver) = channel(COMMAND_SESSION_CONTROL_QUEUE_LIMIT);
    Arc::new(CommandSessionRecord {
        command_session_id: SessionId::from_u128(0x5E55_0000 + u128::from(admission_seq)),
        admission_seq,
        owner_session_id: SessionId::from_u128(0x0A17),
        display_name: None,
        command: String::from("retention test"),
        shell_label: String::from("sh"),
        launcher_shell_label: String::from("sh"),
        interactive_newline: String::from("\n"),
        cwd: PathBuf::from("."),
        backend: CommandSessionBackend::Pty,
        environment_fingerprint: String::new(),
        control_sender,
        shared_state,
    })
}

/// Registering sessions keeps every running session and only the newest finished ones within the limit.
/// 注册会话时保留全部运行中会话，且已结束会话只保留上限内最新的部分。
#[test]
fn register_session_evicts_oldest_finished_sessions_beyond_limit() {
    let runtime = CommandSessionRuntime::new();
    let finished_total = COMMAND_SESSION_RETAINED_FINISHED_LIMIT as u64 + 3;
    let mut admission_seq = 0;
    let mut running_ids = Vec::new();
    let mut finished_ids = Vec::new();
    for index in 0..finished_total {
        // Interleave running sessions so eviction must skip them rather than trim by position.
        // 穿插运行中会话，使淘汰必须跳过它们而不是按位置截断。
        if index % 4 == 0 {
            let running = retention_test_record(admission_seq, CommandSessionStatus::Running);
            admission_seq += 1;
            running_ids.push(running.command_session_id);
            runtime
                .register_session(&running)
                .expect("register running session");
        }
        let status = match index % 3 {
            0 => CommandSessionStatus::Completed,
            1 => CommandSessionStatus::Failed,
            _ => CommandSessionStatus::Stopped,
        };
        let finished = retention_test_record(admission_seq, status);
        admission_seq += 1;
        finished_ids.push(finished.command_session_id);
        runtime
            .register_session(&finished)
            .expect("register finished session");
    }

    let sessions = runtime.sessions.read().expect("lock runtime registry");
    assert!(running_ids.iter().all(|id| sessions.contains_key(id)));
    let evicted = finished_ids.len() - COMMAND_SESSION_RETAINED_FINISHED_LIMIT;
    assert!(
        finished_ids[..evicted]
            .iter()
            .all(|id| !sessions.contains_key(id))
    );
    assert!(
        finished_ids[evicted..]
            .iter()
            .all(|id| sessions.contains_key(id))
    );
    assert_eq!(
        sessions.len(),
        running_ids.len() + COMMAND_SESSION_RETAINED_FINISHED_LIMIT
    );
}
