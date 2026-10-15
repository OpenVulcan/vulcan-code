use super::*;

/// Terminal wait policies should never exceed the host-enforced command-session read budget.
/// 终端等待策略不应超过宿主强制执行的命令会话读取预算。
#[test]
fn terminal_wait_policy_is_hard_bounded() {
    // waitPolicy carries an intentionally excessive budget to verify the host-side cap.
    // waitPolicy 携带故意过大的预算，用于验证宿主侧硬上限。
    let wait_policy = serde_json::from_value::<TerminalSessionWaitPolicy>(json!({
        "mode": "until_pattern",
        "max_wait_ms": u64::MAX,
        "pattern": "ready"
    }))
    .expect("wait policy should deserialize");

    assert_eq!(
        terminal_wait_budget_ms(Some(&wait_policy)),
        Some(MAX_COMMAND_SESSION_READ_WAIT_MS)
    );
}

/// Undeclared fields are rejected at every level instead of being silently ignored.
/// 未声明字段在每一层都会被拒绝，而不是被静默忽略。
#[test]
fn terminal_session_arguments_reject_unknown_fields() {
    for arguments in [
        json!({"action": "list", "timeout_ms": 1000}),
        json!({"action": "read", "terminal_session_id": "t", "wait_policy": {"mode": "until_quiet", "timeout_ms": 1}}),
        json!({"action": "send", "terminal_session_id": "t", "inputs": [{"kind": "key", "key": "Enter", "ctrl": true}]}),
    ] {
        assert!(
            serde_json::from_value::<TerminalSessionToolArgs>(arguments.clone()).is_err(),
            "accepted undeclared field: {arguments}"
        );
    }
    serde_json::from_value::<TerminalSessionToolArgs>(json!({
        "action": "send",
        "terminal_session_id": "t",
        "inputs": [{"kind": "chord", "key": "c", "ctrl": true}],
        "wait_policy": {"mode": "until_quiet", "max_wait_ms": 1}
    }))
    .expect("declared fields stay accepted");
}

/// Terminal read chunk budgets should apply one host-bounded total across every poll.
/// 终端读取分片预算应当作为所有轮询共同使用的一份宿主有界总量。
#[test]
fn terminal_read_chunk_budget_is_total_and_hard_bounded() {
    assert_eq!(
        terminal_read_chunk_budget(None),
        DEFAULT_COMMAND_SESSION_READ_MAX_CHUNKS
    );
    assert_eq!(terminal_read_chunk_budget(Some(0)), 1);
    assert_eq!(terminal_read_chunk_budget(Some(17)), 17);
    assert_eq!(
        terminal_read_chunk_budget(Some(usize::MAX)),
        MAX_COMMAND_SESSION_READ_CHUNKS
    );
}

/// Terminal-session schema should publish the same wait and output limits enforced by the host.
/// terminal-session Schema 应发布与宿主执行一致的等待和输出上限。
#[test]
fn terminal_session_schema_publishes_hard_read_limits() {
    // schema is the exact model-visible contract for bounded terminal reads.
    // schema 是有界终端读取对应的精确模型可见合同。
    let schema = terminal_session_args_schema(&test_shell_support());

    assert_eq!(
        schema["properties"]["wait_policy"]["properties"]["max_wait_ms"]["maximum"],
        json!(MAX_COMMAND_SESSION_READ_WAIT_MS)
    );
    assert_eq!(
        schema["properties"]["max_chunks"]["maximum"],
        json!(MAX_COMMAND_SESSION_READ_CHUNKS)
    );
}

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

/// Removed host-owned exit-code waits should fail parsing instead of degrading to another wait mode.
/// 已移除的宿主退出码等待应解析失败，而不是降级为其他等待模式。
#[test]
fn terminal_session_rejects_removed_exit_code_wait_mode() {
    // error proves the public typed contract has no hidden compatibility branch for the removed mode.
    // error 证明公共强类型契约没有为已移除模式保留隐藏兼容分支。
    let error = serde_json::from_value::<TerminalSessionToolArgs>(json!({
        "action": "send",
        "terminal_session_id": Uuid::nil().to_string(),
        "inputs": [{ "kind": "text", "text": "echo visible\n" }],
        "wait_policy": { "mode": "until_exit_code" }
    }))
    .expect_err("removed wait mode should be rejected");

    assert!(
        error
            .to_string()
            .contains("unknown variant `until_exit_code`")
    );
}

/// Terminal snapshots should expose pending input state without interpreting or rewriting it.
/// 终端快照应公开待提交输入状态，而不解释或改写该输入。
#[test]
fn terminal_snapshot_exposes_pending_input_state() {
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

/// PowerShell terminal text should reach the PTY literally and leave the interactive shell usable.
/// PowerShell 终端文本应原样到达 PTY，并保持交互式 shell 可继续使用。
#[cfg(windows)]
#[tokio::test]
async fn terminal_powershell_text_is_literal_and_shell_remains_usable() {
    let runtime = Arc::new(CommandSessionRuntime::new());
    let tool = build_terminal_session_tool(Arc::clone(&runtime), test_shell_support());
    let owner_session_id = Uuid::from_u128(0x7001);
    let start_result = tool
        .invoke(NativeToolExecutionRequest {
            invocation_context: None,
            arguments: json!({
                "action": "start",
                "name": "literal input regression",
                "command": "pwsh -NoLogo -NoProfile",
                "shell": "pwsh"
            }),
            session_id: Some(owner_session_id),
            native_browser_runtime: None,
            workspace_root: None,
            host_project_id: None,
            controller_runtime: None,
            goal_runtime: None,
            knowledge_access: None,
            vce_retrieval_handler: None,
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

    // immediateSend proves an ordinary LF text submission is accepted as caller-authored input.
    // immediateSend 验证普通 LF 文本提交会作为调用方编写的输入被接受。
    let immediate_send = tool
        .invoke(NativeToolExecutionRequest {
            invocation_context: None,
            arguments: json!({
                "action": "send",
                "terminal_session_id": terminal_session_id,
                "inputs": [{ "kind": "text", "text": "Write-Output 'immediate_submit'\n" }],
                "wait_policy": { "mode": "return_immediately" }
            }),
            session_id: Some(owner_session_id),
            native_browser_runtime: None,
            workspace_root: None,
            host_project_id: None,
            controller_runtime: None,
            goal_runtime: None,
            knowledge_access: None,
            vce_retrieval_handler: None,
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
            invocation_context: None,
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
            native_browser_runtime: None,
            workspace_root: None,
            host_project_id: None,
            controller_runtime: None,
            goal_runtime: None,
            knowledge_access: None,
            vce_retrieval_handler: None,
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

    // literalResult waits for real output without asking the host to generate or hide shell syntax.
    // literalResult 等待真实输出，且不要求宿主生成或隐藏 shell 语法。
    let literal_result = tool
        .invoke(NativeToolExecutionRequest {
            invocation_context: None,
            arguments: json!({
                "action": "send",
                "terminal_session_id": terminal_session_id,
                "inputs": [{ "kind": "text", "text": "Write-Output 'literal_test'\n" }],
                "wait_policy": {
                    "mode": "until_pattern",
                    "pattern": "literal_test",
                    "max_wait_ms": 10000
                }
            }),
            session_id: Some(owner_session_id),
            native_browser_runtime: None,
            workspace_root: None,
            host_project_id: None,
            controller_runtime: None,
            goal_runtime: None,
            knowledge_access: None,
            vce_retrieval_handler: None,
            spill_root: None,
            enabled_prompt_skill_names: Vec::new(),
            prepared_prompt_skills: Vec::new(),
            prompt_skill_user_root: None,
            project_vmm: VmmConfigLayer::default(),
            session_vmm: VmmConfigLayer::default(),
        })
        .await
        .expect("literal send should execute");
    let literal_output = literal_result
        .model_output
        .as_str()
        .expect("literal output should render");
    let literal_payload = &literal_result
        .host_result
        .as_ref()
        .expect("host result")
        .payload;
    assert_eq!(
        literal_payload["pattern_matched"],
        json!(true),
        "literal output was: {literal_output}\npayload was: {literal_payload}"
    );
    // literalPublicText proves AI text and host payload expose only the command actually supplied by the caller.
    // literalPublicText 验证 AI 文本与宿主载荷仅公开调用方实际提供的命令。
    let literal_public_text = format!("{literal_output}\n{literal_payload}");
    assert!(
        literal_public_text.contains("Write-Output 'literal_test'"),
        "original command was missing: {literal_public_text}"
    );
    assert!(
        literal_public_text.contains("literal_test"),
        "real command output was missing: {literal_public_text}"
    );
    for forbidden_fragment in ["__VULCAN_TERMINAL_TX_", "PSConsoleReadLine]::AddToHistory"] {
        assert!(
            !literal_public_text.contains(forbidden_fragment),
            "host-generated shell syntax was present: {forbidden_fragment}\n{literal_public_text}"
        );
    }
    assert!(literal_payload.get("transaction").is_none());

    let followup_result = tool
        .invoke(NativeToolExecutionRequest {
            invocation_context: None,
            arguments: json!({
                "action": "send",
                "terminal_session_id": terminal_session_id,
                "inputs": [{ "kind": "text", "text": "Write-Output 'after_literal'\n" }],
                "wait_policy": {
                    "mode": "until_pattern",
                    "pattern": "after_literal",
                    "max_wait_ms": 10000
                }
            }),
            session_id: Some(owner_session_id),
            native_browser_runtime: None,
            workspace_root: None,
            host_project_id: None,
            controller_runtime: None,
            goal_runtime: None,
            knowledge_access: None,
            vce_retrieval_handler: None,
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
        followup_output.contains("after_literal"),
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
            invocation_context: None,
            arguments: json!({
                "action": "stop",
                "terminal_session_id": terminal_session_id
            }),
            session_id: Some(owner_session_id),
            native_browser_runtime: None,
            workspace_root: None,
            host_project_id: None,
            controller_runtime: None,
            goal_runtime: None,
            knowledge_access: None,
            vce_retrieval_handler: None,
            spill_root: None,
            enabled_prompt_skill_names: Vec::new(),
            prepared_prompt_skills: Vec::new(),
            prompt_skill_user_root: None,
            project_vmm: VmmConfigLayer::default(),
            session_vmm: VmmConfigLayer::default(),
        })
        .await;
}

/// The terminal-session tool should reject cross-session control and let the owner stop its own session.
/// 终端会话工具应拒绝跨会话控制，并允许拥有者停止自己的会话。
#[tokio::test]
async fn terminal_session_tool_enforces_owner_isolation_and_stop() {
    let tool = build_terminal_session_tool(
        Arc::new(CommandSessionRuntime::new()),
        HostCommandShellSupport::from_configured_default_id(None),
    );
    let owner_session_id = Uuid::from_u128(0x7101);
    let foreign_session_id = Uuid::from_u128(0x7102);
    let start_result = tool
        .invoke(NativeToolExecutionRequest {
            invocation_context: None,
            session_id: Some(owner_session_id),
            ..test_native_request(json!({
                "action": "start",
                "name": "owner isolation",
                "command": long_lived_terminal_test_command()
            }))
        })
        .await
        .expect("owner should start a terminal session");
    let terminal_session_id = start_result
        .host_result
        .as_ref()
        .and_then(|result| result.payload["terminal_session_id"].as_str())
        .expect("terminal id should exist")
        .to_string();

    for action in ["read", "snapshot", "stop"] {
        tool.invoke(NativeToolExecutionRequest {
            invocation_context: None,
            session_id: Some(foreign_session_id),
            ..test_native_request(json!({
                "action": action,
                "terminal_session_id": terminal_session_id
            }))
        })
        .await
        .expect_err("a foreign controller session must not access another session's terminal");
    }

    // ownerSnapshot proves the rejected foreign stop left the owner's session running.
    // ownerSnapshot 证明被拒绝的外部停止没有影响拥有者会话的运行状态。
    let owner_snapshot = tool
        .invoke(NativeToolExecutionRequest {
            invocation_context: None,
            session_id: Some(owner_session_id),
            ..test_native_request(json!({
                "action": "snapshot",
                "terminal_session_id": terminal_session_id
            }))
        })
        .await
        .expect("owner should inspect its terminal session");
    assert_eq!(
        owner_snapshot
            .host_result
            .expect("snapshot action should expose host metadata")
            .payload["status"],
        "running"
    );

    let stop_result = tool
        .invoke(NativeToolExecutionRequest {
            invocation_context: None,
            session_id: Some(owner_session_id),
            ..test_native_request(json!({
                "action": "stop",
                "terminal_session_id": terminal_session_id
            }))
        })
        .await
        .expect("owner should stop its terminal session");
    let stop_host_result = stop_result
        .host_result
        .expect("stop action should expose host metadata");
    assert_eq!(stop_host_result.kind, "terminal_session");
    assert_eq!(stop_host_result.payload["status"], "stopped");
}
