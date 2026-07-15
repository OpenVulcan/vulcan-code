use super::*;

/// The terminal-session tool should stay AI-visible in Plan mode so interactive command workflows are not silently hidden from the model.
/// terminal-session 工具应在 Plan 模式下继续对 AI 可见，避免交互式命令工作流被模型静默隐藏。
#[test]
fn terminal_session_tool_stays_visible_in_plan_mode() {
    let tool =
        build_terminal_session_tool(Arc::new(CommandSessionRuntime::new()), test_shell_support());
    assert!(tool.descriptor.ai_visible);
    assert!(tool.descriptor.available_in_plan);
    assert!(
        tool.descriptor
            .description
            .contains("skill(\"terminal-session\")")
    );
}

/// Screen-like terminal chunks should expose a plain screen text view for AI navigation.
/// 屏幕型终端分片应暴露用于 AI 导航的纯文本屏幕视图。
#[test]
fn terminal_screen_text_from_chunks_strips_ansi_sequences() {
    let chunks = vec![CommandSessionOutputChunk {
        seq: 1,
        stream: CommandSessionOutputStream::Stdout,
        text: String::from("\x1b[?1049h\x1b[2J\x1b[HMenu\n> Item"),
    }];

    let screen_text = terminal_screen_text_from_chunks("screen", &chunks)
        .expect("screen mode should expose screen text");

    assert!(screen_text.contains("Menu"));
    assert!(screen_text.contains("> Item"));
    assert!(!screen_text.contains("\x1b"));
    assert!(terminal_screen_text_from_chunks("log", &chunks).is_none());
}

/// Ctrl+C chord input should be recognized as an interrupt so send can drain immediate output.
/// Ctrl+C 组合键输入应被识别为中断，以便 send 可以消化即时输出。
#[test]
fn terminal_inputs_request_interrupt_detects_ctrl_c() {
    assert!(terminal_inputs_request_interrupt(&[
        TerminalSessionInput::Chord {
            key: String::from("c"),
            ctrl: Some(true),
            alt: None,
            shift: None,
            meta: None,
        }
    ]));
    assert!(!terminal_inputs_request_interrupt(&[
        TerminalSessionInput::Key {
            key: String::from("c"),
        }
    ]));
}

/// Terminal key encoding should keep ANSI VT as the default while allowing explicit Windows getch bytes.
/// 终端按键编码应保持 ANSI VT 为默认，同时允许显式 Windows getch 字节。
#[test]
fn terminal_key_encoding_supports_ansi_and_windows_getch_extended_keys() {
    let ansi = encode_terminal_inputs(
        &[
            TerminalSessionInput::Key {
                key: String::from("ArrowDown"),
            },
            TerminalSessionInput::Key {
                key: String::from("Delete"),
            },
            TerminalSessionInput::Key {
                key: String::from("F12"),
            },
        ],
        "\r\n",
        TerminalSessionKeyEncoding::AnsiVt,
    )
    .expect("encode ansi keys");
    assert_eq!(ansi, b"\x1b[B\x1b[3~\x1b[24~");

    let windows_getch_single_byte = encode_terminal_inputs(
        &[TerminalSessionInput::Key {
            key: String::from("Esc"),
        }],
        "\r\n",
        TerminalSessionKeyEncoding::WindowsConsoleGetch,
    )
    .expect("encode windows getch single-byte key");
    assert_eq!(windows_getch_single_byte, vec![0x1b]);

    let windows_getch_arrow = encode_terminal_inputs(
        &[TerminalSessionInput::Key {
            key: String::from("ArrowDown"),
        }],
        "\r\n",
        TerminalSessionKeyEncoding::WindowsConsoleGetch,
    );
    if cfg!(windows) {
        assert!(
            windows_getch_arrow
                .expect_err("byte-oriented backend should reject getch extended keys")
                .message
                .contains("cannot synthesize console extended key")
        );
    } else {
        assert_eq!(windows_getch_arrow.expect("encode arrow"), vec![0xe0, b'P']);
    }
}

/// Explicit ANSI VT encoding on WindowsConsole should convert keys to text escape sequences instead of native key events.
/// WindowsConsole 上显式 ANSI VT 编码应把按键转换为文本转义序列，而不是原生按键事件。
#[test]
fn windows_console_explicit_ansi_vt_uses_text_escape_sequence() {
    let events = windows_console_input_events_from_inputs(
        &[TerminalSessionInput::Key {
            key: String::from("ArrowDown"),
        }],
        "\r\n",
        Some(TerminalSessionKeyEncoding::AnsiVt),
    )
    .expect("convert ansi vt input");
    assert_eq!(events.len(), 1);
    match &events[0] {
        CommandSessionInputEvent::Text { text } => assert_eq!(text, "\x1b[B"),
        other => panic!("expected text event, got {other:?}"),
    }

    let native_events = windows_console_input_events_from_inputs(
        &[TerminalSessionInput::Key {
            key: String::from("ArrowDown"),
        }],
        "\r\n",
        None,
    )
    .expect("convert native input");
    assert!(matches!(
        &native_events[0],
        CommandSessionInputEvent::Key { key } if key == "ArrowDown"
    ));
}

/// Interrupt drain should hide pure control fragments while keeping visible prompt or output text.
/// 中断排空应隐藏纯控制片段，同时保留可见的提示符或输出文本。
#[test]
fn terminal_interrupt_drain_hides_control_only_chunks() {
    let control_chunk = CommandSessionOutputChunk {
        seq: 1,
        stream: CommandSessionOutputStream::Stdout,
        text: String::from("\x03\r\n\x1b[?25l"),
    };
    let prompt_chunk = CommandSessionOutputChunk {
        seq: 2,
        stream: CommandSessionOutputStream::Stdout,
        text: String::from("PS D:\\repo> "),
    };

    assert!(!terminal_interrupt_drain_chunk_is_visible(&control_chunk));
    assert!(terminal_interrupt_drain_chunk_is_visible(&prompt_chunk));
}

/// Adjacent terminal chunks from the same stream should be coalesced before returning to the model.
/// 返回给模型前应合并同一输出流上的相邻终端分片。
#[test]
fn terminal_output_chunks_coalesce_adjacent_same_stream() {
    let chunks = vec![
        CommandSessionOutputChunk {
            seq: 1,
            stream: CommandSessionOutputStream::Stdout,
            text: String::from("$"),
        },
        CommandSessionOutputChunk {
            seq: 2,
            stream: CommandSessionOutputStream::Stdout,
            text: String::from("global"),
        },
        CommandSessionOutputChunk {
            seq: 3,
            stream: CommandSessionOutputStream::Stderr,
            text: String::from("err"),
        },
    ];

    let coalesced = coalesce_terminal_output_chunks(chunks);

    assert_eq!(coalesced.len(), 2);
    assert_eq!(coalesced[0].seq, 1);
    assert_eq!(coalesced[0].text, "$global");
    assert_eq!(coalesced[1].stream, CommandSessionOutputStream::Stderr);
}

/// Windows console screen frames should parse the transaction marker even when the visible frame loses the leading host prefix.
/// Windows 控制台屏幕帧即使丢失宿主前缀的可见开头，也应解析事务标记。
#[test]
fn terminal_transaction_sentinel_accepts_windows_screen_visible_suffix() {
    let chunks = vec![CommandSessionOutputChunk {
        seq: 3,
        stream: CommandSessionOutputStream::Stdout,
        text: String::from(
            "[screen #3]\n\x1b[37;40m_TERMINAL_TX_EXIT__:tx-screen:0\x1b[0m\nPS D:\\repo>",
        ),
    }];

    let (stripped_chunks, exit_code) = strip_terminal_transaction_sentinel(&chunks, "tx-screen");

    assert_eq!(exit_code, Some(0));
    assert!(stripped_chunks.iter().all(|chunk| {
        !strip_simple_ansi_sequences(&chunk.text).contains("_TERMINAL_TX_EXIT__:tx-screen")
    }));
}

/// Windows console screen frames should still parse transaction markers after deeper left clipping.
/// Windows 控制台屏幕帧在更深的左侧裁剪后仍应解析事务标记。
#[test]
fn terminal_transaction_sentinel_accepts_deeply_clipped_visible_suffix() {
    let chunks = vec![CommandSessionOutputChunk {
        seq: 4,
        stream: CommandSessionOutputStream::Stdout,
        text: String::from(
            "[screen #4]\n\x1b[37;40mINAL_TX_EXIT__:tx-screen:0\x1b[0m\nPS D:\\repo>",
        ),
    }];

    let (stripped_chunks, exit_code) = strip_terminal_transaction_sentinel(&chunks, "tx-screen");

    assert_eq!(exit_code, Some(0));
    assert!(stripped_chunks.iter().all(|chunk| {
        !strip_simple_ansi_sequences(&chunk.text).contains("INAL_TX_EXIT__:tx-screen")
    }));
}

/// Later echoed transaction wrapper text should not erase a previously parsed transaction exit code.
/// 后续回显的事务包装文本不应清空先前已解析到的事务退出码。
#[test]
fn terminal_transaction_sentinel_keeps_parsed_code_when_echo_later_matches() {
    let chunks = vec![CommandSessionOutputChunk {
        seq: 4,
        stream: CommandSessionOutputStream::Stdout,
        text: String::from(
            "__VULCAN_TERMINAL_TX_EXIT__:tx-keep:0\nWrite-Output \"__VULCAN_TERMINAL_TX_EXIT__:tx-keep:$vulcanCode\"\n",
        ),
    }];

    let (_stripped_chunks, exit_code) = strip_terminal_transaction_sentinel(&chunks, "tx-keep");

    assert_eq!(exit_code, Some(0));
}

/// Terminal names should preserve explicit labels and fall back to compact command text when omitted.
/// 终端名称应保留显式标签，并在省略时回退到紧凑命令文本。
#[test]
fn terminal_session_name_normalization_uses_explicit_or_command_fallback() {
    assert_eq!(
        normalize_terminal_session_name(Some("  dev server  "), "pwsh -NoLogo"),
        "dev server"
    );
    assert_eq!(
        normalize_terminal_session_name(None, "pwsh   -NoLogo"),
        "pwsh -NoLogo"
    );
    assert_eq!(normalize_terminal_session_name(Some("   "), ""), "terminal");
}

/// Terminal command transactions must reject paste input because wrapping pasted payloads can corrupt interactive shell state.
/// 终端命令事务必须拒绝粘贴输入，因为包装粘贴载荷可能破坏交互式 shell 状态。
#[test]
fn terminal_transaction_rejects_paste_input() {
    let error = terminal_transaction_command_text(&[TerminalSessionInput::Paste {
        text: String::from("echo pasted\n"),
    }])
    .expect_err("paste input should not be accepted for exit-code transactions");

    assert!(error.message.contains("cannot be used with paste"));
}

/// Terminal command transactions should reject dirty interactive state before wrapper injection.
/// 终端命令事务应在注入包装命令前拒绝脏交互状态。
#[test]
fn terminal_transaction_rejects_pending_input_line() {
    let snapshot = CommandSessionSnapshot {
        command_session_id: Uuid::from_u128(0x51),
        display_name: Some(String::from("dirty shell")),
        owner_session_id: Uuid::from_u128(0x52),
        command: String::from("pwsh -NoLogo"),
        shell_label: String::from("pwsh"),
        launcher_shell_label: String::from("pwsh"),
        cwd: PathBuf::from(r"D:\openvulcan\vulcan-code"),
        backend: CommandSessionBackend::Pty,
        status: CommandSessionStatus::Running,
        exit_code: None,
        failure_reason: None,
        stdout_bytes: 0,
        stderr_bytes: 0,
        last_seq: 7,
        pending_stdin_line: true,
        screen_frame_seq: None,
        screen_rows: None,
        screen_cols: None,
        screen_text: None,
    };

    let error = validate_terminal_transaction_start(&snapshot)
        .expect_err("pending input line should reject command transactions");
    assert!(error.message.contains("pending input line"));
}

/// Literal readiness matching should work within one chunk and across chunk boundaries.
/// 字面量就绪匹配应同时支持单个分片内与跨分片边界的内容。
#[test]
fn terminal_pattern_matching_checks_joined_output_chunks() {
    let chunks = vec![
        CommandSessionOutputChunk {
            seq: 1,
            stream: CommandSessionOutputStream::Stdout,
            text: String::from("REA"),
        },
        CommandSessionOutputChunk {
            seq: 2,
            stream: CommandSessionOutputStream::Stdout,
            text: String::from("DY\n"),
        },
    ];

    assert!(terminal_output_chunks_contain_pattern(&chunks, "READY"));
    assert!(!terminal_output_chunks_contain_pattern(&chunks, "MISSING"));
}

/// PowerShell terminal transactions should not leave the interactive shell in continuation mode.
/// PowerShell 终端事务不应让交互式 shell 停留在续行模式。
#[cfg(windows)]
#[tokio::test]
async fn terminal_powershell_transaction_leaves_shell_usable() {
    let runtime = Arc::new(CommandSessionRuntime::new());
    let tool = build_terminal_session_tool(Arc::clone(&runtime), test_shell_support());
    let owner_session_id = Uuid::from_u128(0x7001);
    let start_result = tool
        .invoke(NativeToolExecutionRequest {
            arguments: json!({
                "action": "start",
                "name": "transaction regression",
                "command": "pwsh -NoLogo -NoProfile",
                "shell": "pwsh"
            }),
            session_id: Some(owner_session_id),
            workspace_root: None,
            controller_runtime: None,
            spill_root: None,
            enabled_prompt_skill_names: Vec::new(),
            prepared_prompt_skills: Vec::new(),
            prompt_skill_user_root: None,
            project_vmm: VmmConfigLayer::default(),
            session_vmm: VmmConfigLayer::default(),
        })
        .await
        .expect("terminal start should execute");
    let terminal_session_id = start_result
        .host_result
        .as_ref()
        .and_then(|result| result.payload["terminal_session_id"].as_str())
        .expect("terminal id should exist")
        .to_string();

    let transaction_result = tool
        .invoke(NativeToolExecutionRequest {
            arguments: json!({
                "action": "send",
                "terminal_session_id": terminal_session_id,
                "inputs": [{ "kind": "text", "text": "Write-Output 'txn_test'\n" }],
                "wait_policy": { "mode": "until_exit_code", "max_wait_ms": 10000 }
            }),
            session_id: Some(owner_session_id),
            workspace_root: None,
            controller_runtime: None,
            spill_root: None,
            enabled_prompt_skill_names: Vec::new(),
            prepared_prompt_skills: Vec::new(),
            prompt_skill_user_root: None,
            project_vmm: VmmConfigLayer::default(),
            session_vmm: VmmConfigLayer::default(),
        })
        .await
        .expect("transaction send should execute");
    let transaction_output = transaction_result
        .model_output
        .as_str()
        .expect("transaction output should render");
    let transaction_payload = &transaction_result
        .host_result
        .as_ref()
        .expect("host result")
        .payload;
    assert_eq!(
        transaction_payload["transaction"]["exit_code"],
        json!(0),
        "transaction output was: {transaction_output}\npayload was: {transaction_payload}"
    );

    let followup_result = tool
        .invoke(NativeToolExecutionRequest {
            arguments: json!({
                "action": "send",
                "terminal_session_id": terminal_session_id,
                "inputs": [{ "kind": "text", "text": "Write-Output 'after_tx'\n" }],
                "wait_policy": {
                    "mode": "until_pattern",
                    "pattern": "after_tx",
                    "max_wait_ms": 10000
                }
            }),
            session_id: Some(owner_session_id),
            workspace_root: None,
            controller_runtime: None,
            spill_root: None,
            enabled_prompt_skill_names: Vec::new(),
            prepared_prompt_skills: Vec::new(),
            prompt_skill_user_root: None,
            project_vmm: VmmConfigLayer::default(),
            session_vmm: VmmConfigLayer::default(),
        })
        .await
        .expect("follow-up send should execute");
    let followup_output = followup_result
        .model_output
        .as_str()
        .expect("follow-up output should render");
    assert!(
        followup_output.contains("after_tx"),
        "follow-up output was: {followup_output}"
    );
    assert_eq!(
        followup_result
            .host_result
            .as_ref()
            .expect("host result")
            .payload["pattern_matched"],
        json!(true)
    );

    let _ = tool
        .invoke(NativeToolExecutionRequest {
            arguments: json!({
                "action": "stop",
                "terminal_session_id": terminal_session_id
            }),
            session_id: Some(owner_session_id),
            workspace_root: None,
            controller_runtime: None,
            spill_root: None,
            enabled_prompt_skill_names: Vec::new(),
            prepared_prompt_skills: Vec::new(),
            prompt_skill_user_root: None,
            project_vmm: VmmConfigLayer::default(),
            session_vmm: VmmConfigLayer::default(),
        })
        .await;
}

/// PowerShell transaction wrapping should report non-terminating cmdlet errors as a failed transaction.
/// PowerShell 事务包装应将 cmdlet 非终止错误报告为失败事务。
#[test]
fn terminal_transaction_powershell_wrapper_tracks_non_terminating_errors() {
    let wrapped = wrap_terminal_transaction_command("Get-Item missing", "pwsh", "tx-id", "\r\n")
        .expect("pwsh wrapper should be supported");

    assert!(wrapped.contains("$vulcanErrorCount = $Error.Count"));
    assert!(wrapped.contains("$vulcanNewErrors = $Error.Count - $vulcanErrorCount"));
    assert!(wrapped.contains("$vulcanSuccess -and $vulcanNewErrors -le 0"));
    assert!(wrapped.contains("__VULCAN_TERMINAL_TX_EXIT__:tx-id:"));
}
