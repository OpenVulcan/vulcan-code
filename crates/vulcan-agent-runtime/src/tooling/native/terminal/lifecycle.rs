use super::*;

pub(crate) async fn execute_terminal_session_list_action(
    request: NativeToolExecutionRequest,
    owner_session_id: SessionId,
    command_session_runtime: Arc<CommandSessionRuntime>,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let mut snapshots = command_session_runtime
        .try_snapshot_sessions()
        .into_iter()
        .filter(|snapshot| snapshot.owner_session_id == owner_session_id)
        .collect::<Vec<_>>();
    snapshots.sort_by_key(|snapshot| snapshot.command_session_id);

    let terminal_entries = snapshots
        .iter()
        .map(|snapshot| {
            let name = terminal_session_name_for_snapshot(snapshot);
            json!({
                "terminal_session_id": snapshot.command_session_id.to_string(),
                "command_session_id": snapshot.command_session_id.to_string(),
                "name": name,
                "command": snapshot.command,
                "cwd": normalized_workspace_path_text_from_path(&snapshot.cwd),
                "shell": snapshot.shell_label,
                "launcher_shell": snapshot.launcher_shell_label,
                "status": snapshot.status.as_str(),
                "exit_code": snapshot.exit_code,
                "stdout_bytes": snapshot.stdout_bytes,
                "stderr_bytes": snapshot.stderr_bytes,
                "last_seq": snapshot.last_seq,
                "pending_stdin_line": snapshot.pending_stdin_line,
                "failure_reason": snapshot.failure_reason,
            })
        })
        .collect::<Vec<_>>();

    let mut lines = vec![
        String::from("Action: list"),
        format!("Owner session: {owner_session_id}"),
        format!("Terminal count: {}", terminal_entries.len()),
    ];
    if terminal_entries.is_empty() {
        lines.push(String::from("(no terminal sessions)"));
    } else {
        for entry in &terminal_entries {
            let name = entry["name"].as_str().unwrap_or("terminal");
            let terminal_id = entry["terminal_session_id"].as_str().unwrap_or("-");
            let status = entry["status"].as_str().unwrap_or("unknown");
            let command = entry["command"].as_str().unwrap_or("");
            lines.push(format!(
                "- {name} [{status}] id={terminal_id} command={command}"
            ));
        }
    }

    let render_input = HostTextRenderInput {
        content_bytes: lines.join("\n").len(),
        content_lines: lines.len(),
        content: lines.join("\n"),
        overflow_mode: Some(HostToolOverflowMode::Page),
        template_hint: None,
    };
    let budget = default_shell_command_budget_snapshot();
    let spill_root = resolve_shell_command_spill_root(
        request.spill_root.as_deref(),
        request.workspace_root.as_deref(),
        request
            .workspace_root
            .as_deref()
            .unwrap_or_else(|| Path::new(".")),
    );
    let host_payload = json!({
        "action": "list",
        "owner_session_id": owner_session_id.to_string(),
        "terminal_count": terminal_entries.len(),
        "terminal_sessions": terminal_entries,
        "output_overflowed": shell_command_output_exceeds_inline_budget(&render_input, &budget),
    });
    let model_output = render_tool_result_text(
        &render_input,
        None,
        &budget,
        &HostRenderOptions {
            spill_root,
            template_skill_roots: Vec::new(),
            template_resources_root: None,
        },
    );

    Ok(
        VulcanToolInvocationResult::from_model_output(Value::String(model_output))
            .with_host_result(VulcanToolHostResult {
                kind: String::from("terminal_session"),
                payload: host_payload,
            }),
    )
}

/// Executes one terminal-session start action and returns the unified tool invocation result.
/// 执行一次终端会话 start 动作并返回统一工具调用结果。
pub(crate) async fn execute_terminal_session_start_action(
    request: NativeToolExecutionRequest,
    parsed_args: TerminalSessionToolArgs,
    owner_session_id: SessionId,
    command_session_runtime: Arc<CommandSessionRuntime>,
    shell_support: &HostCommandShellSupport,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let command_text = parsed_args
        .command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            NativeToolError::new(
                "The `start` action for terminal_session requires a non-empty `command`.",
            )
        })?;
    let effective_cwd = resolve_shell_command_working_directory(
        request.workspace_root.as_deref(),
        parsed_args.cwd.as_deref(),
    )?;
    let spill_root = resolve_shell_command_spill_root(
        request.spill_root.as_deref(),
        request.workspace_root.as_deref(),
        &effective_cwd,
    );
    let (launcher_program, launcher_args, launcher_shell_label) =
        resolve_shell_command_launcher(shell_support, parsed_args.shell.as_deref(), command_text)?;
    let shell_behavior = resolve_session_shell_behavior(command_text, &launcher_shell_label);
    let requested_shell_selector = normalize_shell_selector(parsed_args.shell.as_deref());
    validate_terminal_session_start_shell_policy(parsed_args.shell.as_deref(), &shell_behavior)?;
    let session_name = require_terminal_session_start_name(parsed_args.name.as_deref())?;
    let snapshot = command_session_runtime
        .start_session(CommandSessionStartRequest {
            owner_session_id,
            display_name: Some(session_name.clone()),
            command: command_text.to_string(),
            shell_label: shell_behavior.interactive_shell_label.clone(),
            launcher_shell_label: shell_behavior.launcher_shell_label.clone(),
            interactive_newline: terminal_session_pty_newline(
                shell_behavior.interactive_shell_label.as_str(),
            )
            .to_string(),
            cwd: effective_cwd,
            launcher_program,
            launcher_args,
            initial_stdin: None,
            initial_stdin_append_newline: false,
            backend: default_terminal_session_backend(),
        })
        .await
        .map_err(NativeToolError::new)?;
    let returned_chunks = Vec::new();
    let (output_mode, confidence, reason) = detect_terminal_output_mode(&returned_chunks);
    let render_input = build_terminal_session_render_input(
        TerminalSessionToolAction::Start,
        &snapshot,
        &session_name,
        &returned_chunks,
        false,
        output_mode,
        None,
        vec![
            format!("Requested shell selector: {}", requested_shell_selector),
            format!(
                "Resolved launcher shell: {}",
                shell_behavior.launcher_shell_label
            ),
            format!(
                "Interactive shell: {}",
                shell_behavior.interactive_shell_label
            ),
            String::from("Before using this tool, call skill(\"terminal-session\") and follow it."),
        ],
    );
    let budget = default_shell_command_budget_snapshot();
    let host_payload = build_terminal_session_host_payload(
        TerminalSessionToolAction::Start,
        &snapshot,
        &session_name,
        &returned_chunks,
        shell_command_output_exceeds_inline_budget(&render_input, &budget),
        output_mode,
        confidence,
        reason,
        None,
        json!({
            "requested_shell": requested_shell_selector,
            "nested_shell_inferred": shell_behavior.nested_shell_inferred,
        }),
    );
    let model_output = render_tool_result_text(
        &render_input,
        None,
        &budget,
        &HostRenderOptions {
            spill_root,
            template_skill_roots: Vec::new(),
            template_resources_root: None,
        },
    );

    Ok(
        VulcanToolInvocationResult::from_model_output(Value::String(model_output))
            .with_host_result(VulcanToolHostResult {
                kind: String::from("terminal_session"),
                payload: host_payload,
            }),
    )
}

pub(crate) async fn execute_terminal_session_stop_action(
    parsed_args: TerminalSessionToolArgs,
    owner_session_id: SessionId,
    command_session_runtime: Arc<CommandSessionRuntime>,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let terminal_session_id =
        parse_terminal_session_id(parsed_args.terminal_session_id.as_deref())?;
    let snapshot = command_session_runtime
        .stop_session(
            owner_session_id,
            terminal_session_id,
            Some(DEFAULT_COMMAND_SESSION_STOP_WAIT_MS),
        )
        .await
        .map_err(NativeToolError::new)?;
    let session_name = terminal_session_name_for_snapshot(&snapshot);
    let returned_chunks = Vec::new();
    let (output_mode, confidence, reason) = detect_terminal_output_mode(&returned_chunks);
    let render_input = build_terminal_session_render_input(
        TerminalSessionToolAction::Stop,
        &snapshot,
        &session_name,
        &returned_chunks,
        false,
        output_mode,
        None,
        vec![format!(
            "Stop wait ms: {DEFAULT_COMMAND_SESSION_STOP_WAIT_MS}"
        )],
    );
    let budget = default_shell_command_budget_snapshot();
    let host_payload = build_terminal_session_host_payload(
        TerminalSessionToolAction::Stop,
        &snapshot,
        &session_name,
        &returned_chunks,
        shell_command_output_exceeds_inline_budget(&render_input, &budget),
        output_mode,
        confidence,
        reason,
        None,
        json!({
            "stop_wait_ms": DEFAULT_COMMAND_SESSION_STOP_WAIT_MS,
        }),
    );
    Ok(
        VulcanToolInvocationResult::from_model_output(Value::String(render_input.content))
            .with_host_result(VulcanToolHostResult {
                kind: String::from("terminal_session"),
                payload: host_payload,
            }),
    )
}

/// Executes the host-owned `terminal_session` tool and returns a unified tool invocation result.
/// 执行宿主自有 `terminal_session` 工具并返回统一工具调用结果。
pub(crate) async fn execute_terminal_session_tool(
    request: NativeToolExecutionRequest,
    command_session_runtime: Arc<CommandSessionRuntime>,
    shell_support: &HostCommandShellSupport,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let owner_session_id = require_terminal_session_owner_session_id(request.session_id)?;
    let parsed_args = parse_terminal_session_tool_arguments(request.arguments.clone())?;
    match parsed_args.action {
        TerminalSessionToolAction::List => {
            execute_terminal_session_list_action(request, owner_session_id, command_session_runtime)
                .await
        }
        TerminalSessionToolAction::Start => {
            execute_terminal_session_start_action(
                request,
                parsed_args,
                owner_session_id,
                command_session_runtime,
                shell_support,
            )
            .await
        }
        TerminalSessionToolAction::Send => {
            execute_terminal_session_send_action(
                request,
                parsed_args,
                owner_session_id,
                command_session_runtime,
            )
            .await
        }
        TerminalSessionToolAction::Read => {
            execute_terminal_session_read_like_action(
                request,
                parsed_args,
                owner_session_id,
                command_session_runtime,
                TerminalSessionToolAction::Read,
            )
            .await
        }
        TerminalSessionToolAction::Snapshot => {
            execute_terminal_session_read_like_action(
                request,
                parsed_args,
                owner_session_id,
                command_session_runtime,
                TerminalSessionToolAction::Snapshot,
            )
            .await
        }
        TerminalSessionToolAction::Stop => {
            execute_terminal_session_stop_action(
                parsed_args,
                owner_session_id,
                command_session_runtime,
            )
            .await
        }
    }
}
