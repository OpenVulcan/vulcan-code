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

/// Terminal input schema should promise host-normalized logical newlines without exposing platform bytes.
/// 终端输入 Schema 应承诺由宿主归一化逻辑换行，且不暴露平台字节。
#[test]
fn terminal_session_schema_documents_logical_newline_normalization() {
    // schemaText captures the complete model-visible terminal contract for exact wording assertions.
    // schemaText 捕获完整的模型可见终端合同，用于精确文案断言。
    let schema_text = terminal_session_args_schema(&test_shell_support()).to_string();
    assert!(schema_text.contains("normalizes CRLF, CR, and LF"));
    assert!(schema_text.contains("bracketed-paste mode is not implied"));
    assert!(!schema_text.contains("send manual carriage return"));
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

/// Persistent AI terminals should use the same isolated PTY backend on every supported platform.
/// 持久 AI 终端应在所有受支持平台上使用同一个隔离 PTY 后端。
#[test]
fn terminal_session_backend_is_cross_platform_pty() {
    assert_eq!(
        default_terminal_session_backend(),
        CommandSessionBackend::Pty
    );
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
        dropped_before_seq: 0,
        pending_stdin_line: true,
        screen_frame_seq: None,
        screen_rows: None,
        screen_cols: None,
        screen_text: None,
    };

    let error = validate_terminal_transaction_start(&snapshot)
        .expect_err("pending input line should reject command transactions");
    assert!(error.message.contains("pending input line"));

    // payload keeps the original field while exposing the clearer pending_input_line alias.
    // payload 保留原字段，同时暴露更清晰的 pending_input_line 别名。
    let payload = build_terminal_session_host_payload(
        TerminalSessionToolAction::Snapshot,
        &snapshot,
        "dirty shell",
        &[],
        false,
        "log",
        1.0,
        String::from("test"),
        None,
        json!({}),
    );
    assert_eq!(payload["pending_stdin_line"], json!(true));
    assert_eq!(payload["pending_input_line"], json!(true));
    assert_eq!(payload["screen_frame_confirmed"], json!(false));
}

/// Confirmed screen checkpoints should expose their exact frame sequence in text and host payloads.
/// 已确认屏幕检查点应在文本与宿主载荷中暴露精确帧序号。
#[test]
fn terminal_render_labels_confirmed_screen_frame_sequence() {
    // snapshot models one atomic backend screen checkpoint after output processing.
    // snapshot 模拟输出处理后的一份原子后端屏幕检查点。
    let snapshot = CommandSessionSnapshot {
        command_session_id: Uuid::from_u128(0x61),
        display_name: Some(String::from("screen shell")),
        owner_session_id: Uuid::from_u128(0x62),
        command: String::from("pwsh -NoLogo"),
        shell_label: String::from("pwsh"),
        launcher_shell_label: String::from("pwsh"),
        cwd: PathBuf::from(r"D:\openvulcan\vulcan-code"),
        backend: CommandSessionBackend::Pty,
        status: CommandSessionStatus::Running,
        exit_code: None,
        failure_reason: None,
        stdout_bytes: 12,
        stderr_bytes: 0,
        last_seq: 9,
        dropped_before_seq: 0,
        pending_stdin_line: false,
        screen_frame_seq: Some(7),
        screen_rows: Some(24),
        screen_cols: Some(80),
        screen_text: Some(String::from("PS> ready")),
    };
    // renderInput is the AI-visible text contract for the confirmed frame.
    // renderInput 是已确认帧面向 AI 的文本合同。
    let render_input = build_terminal_session_render_input(
        TerminalSessionToolAction::Snapshot,
        &snapshot,
        "screen shell",
        &[],
        false,
        "screen",
        None,
        Vec::new(),
    );
    assert!(render_input.content.contains("Screen frame sequence: 7"));
    assert!(
        render_input
            .content
            .contains("Screen frame confirmed: true")
    );
    assert!(
        render_input
            .content
            .contains("[screen_text frame_seq=7 confirmed=true]")
    );

    // payload is the host-only structured contract consumed by UI and diagnostics.
    // payload 是界面与诊断使用的宿主专用结构化合同。
    let payload = build_terminal_session_host_payload(
        TerminalSessionToolAction::Snapshot,
        &snapshot,
        "screen shell",
        &[],
        false,
        "screen",
        1.0,
        String::from("test"),
        None,
        json!({}),
    );
    assert_eq!(payload["screen_frame_seq"], json!(7));
    assert_eq!(payload["screen_frame_confirmed"], json!(true));
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
            knowledge_access: None,
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

    // immediateSend proves an ordinary LF text submission is accepted without a transaction wrapper.
    // immediateSend 验证普通 LF 文本提交无需事务包装即可被接受。
    let immediate_send = tool
        .invoke(NativeToolExecutionRequest {
            arguments: json!({
                "action": "send",
                "terminal_session_id": terminal_session_id,
                "inputs": [{ "kind": "text", "text": "Write-Output 'immediate_submit'\n" }],
                "wait_policy": { "mode": "return_immediately" }
            }),
            session_id: Some(owner_session_id),
            workspace_root: None,
            controller_runtime: None,
            knowledge_access: None,
            spill_root: None,
            enabled_prompt_skill_names: Vec::new(),
            prepared_prompt_skills: Vec::new(),
            prompt_skill_user_root: None,
            project_vmm: VmmConfigLayer::default(),
            session_vmm: VmmConfigLayer::default(),
        })
        .await
        .expect("immediate LF submission should execute");
    assert!(
        immediate_send
            .host_result
            .as_ref()
            .expect("host result")
            .payload["bytes_written"]
            .as_u64()
            .is_some_and(|bytes| bytes > 0)
    );

    // immediateRead proves the same logical newline committed the command instead of entering continuation mode.
    // immediateRead 验证相同逻辑换行已提交命令，而不是进入续行模式。
    let immediate_read = tool
        .invoke(NativeToolExecutionRequest {
            arguments: json!({
                "action": "read",
                "terminal_session_id": terminal_session_id,
                "wait_policy": {
                    "mode": "until_pattern",
                    "pattern": "immediate_submit",
                    "max_wait_ms": 10000
                }
            }),
            session_id: Some(owner_session_id),
            workspace_root: None,
            controller_runtime: None,
            knowledge_access: None,
            spill_root: None,
            enabled_prompt_skill_names: Vec::new(),
            prepared_prompt_skills: Vec::new(),
            prompt_skill_user_root: None,
            project_vmm: VmmConfigLayer::default(),
            session_vmm: VmmConfigLayer::default(),
        })
        .await
        .expect("immediate LF output should be readable");
    assert_eq!(
        immediate_read
            .host_result
            .as_ref()
            .expect("host result")
            .payload["pattern_matched"],
        json!(true)
    );

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
            knowledge_access: None,
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
            knowledge_access: None,
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
            knowledge_access: None,
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
    let wrapped = wrap_terminal_transaction_command("Get-Item 'missing'", "pwsh", "tx-id", "\r\n")
        .expect("pwsh wrapper should be supported");

    assert!(wrapped.contains("$vulcanErrorCount = $Error.Count"));
    assert!(wrapped.contains("$vulcanNewErrors = $Error.Count - $vulcanErrorCount"));
    assert!(wrapped.contains("$vulcanSuccess -and $vulcanNewErrors -le 0"));
    assert!(wrapped.contains(
        "[Microsoft.PowerShell.PSConsoleReadLine]::AddToHistory('Get-Item ''missing''')"
    ));
    assert!(wrapped.contains("try { [Microsoft.PowerShell.PSConsoleReadLine]::AddToHistory"));
    assert!(wrapped.contains("catch { }"));
    assert!(wrapped.contains("__VULCAN_TERMINAL_TX_EXIT__:tx-id:"));
}
