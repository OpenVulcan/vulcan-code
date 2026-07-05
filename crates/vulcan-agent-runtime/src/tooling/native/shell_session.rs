use super::*;

/// Builds the session-oriented shell-command host-owned native tool.
/// 构建面向会话的 shell 命令宿主自有原生工具。
#[cfg(test)]
pub(crate) fn build_shell_session_tool(
    command_session_runtime: Arc<CommandSessionRuntime>,
    shell_support: HostCommandShellSupport,
) -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from("shell_session"),
        name: String::from("shell_session"),
        description: shell_session_tool_description(),
        help_text: None,
        ai_visible: true,
        available_in_plan: true,
    };

    build_native_tool_definition(
        descriptor,
        shell_session_args_schema(&shell_support),
        move |request: NativeToolExecutionRequest| {
            let command_session_runtime = Arc::clone(&command_session_runtime);
            let shell_support = shell_support.clone();
            Box::pin(async move {
                execute_shell_session_tool(request, command_session_runtime, &shell_support).await
            })
        },
    )
}

/// One action accepted by the host-owned `shell_session` tool.
/// 宿主自有 `shell_session` 工具接受的一种动作。
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ShellSessionToolAction {
    Start,
    Read,
    Write,
    Stop,
}

impl ShellSessionToolAction {
    /// Renders one stable lowercase action label for model output and host metadata.
    /// 为模型输出与宿主元数据渲染稳定的小写动作标签。
    fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Read => "read",
            Self::Write => "write",
            Self::Stop => "stop",
        }
    }
}

/// One typed argument payload accepted by the host-owned `shell_session` native tool.
/// 宿主自有 `shell_session` 原生工具接受的一份强类型参数载荷。
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ShellSessionToolArgs {
    /// Requested shell-session action.
    /// 请求的命令会话动作。
    action: ShellSessionToolAction,
    /// Optional target command-session identifier used by non-start actions.
    /// 供非 start 动作使用的可选目标命令会话标识。
    command_session_id: Option<String>,
    /// Optional raw command string used by the start action.
    /// 供 start 动作使用的可选原始命令字符串。
    command: Option<String>,
    /// Optional working directory override resolved relative to the workspace root when needed.
    /// 需要时相对工作区根目录解析的可选工作目录覆盖值。
    cwd: Option<String>,
    /// Optional shell family selector such as `auto`, `powershell`, `cmd`, or `bash`.
    /// 可选 shell 类型选择器，例如 `auto`、`powershell`、`cmd` 或 `bash`。
    shell: Option<String>,
    /// Optional stdin payload used by the start or write action.
    /// 供 start 或 write 动作使用的可选标准输入载荷。
    stdin: Option<String>,
    /// Optional flag indicating whether the write action should append one newline.
    /// 指示 write 动作是否应追加一个换行符的可选标志。
    append_newline: Option<bool>,
    /// Optional output cursor used by the read action to request only newer chunks.
    /// 供 read 动作仅请求更新分片时使用的可选输出游标。
    after_seq: Option<u64>,
    /// Optional wait budget used by the read action when no fresh output is immediately available.
    /// 当没有即时新输出时供 read 动作使用的可选等待预算。
    wait_for_output_ms: Option<u64>,
    /// Optional maximum chunk count returned by one read action.
    /// 一次 read 动作返回的可选最大分片数量。
    max_chunks: Option<usize>,
}

pub(crate) fn shell_session_tool_description() -> String {
    format!(
        "Start, read, write to, or stop a long-lived host command session. Reads are polling-based, and `cwd` only affects `start`. The `start` result reports the resolved launcher shell used for `auto` and whether stdin is currently left on an open input line. When an inline read result exceeds {SHELL_INLINE_OUTPUT_LIMIT_BYTES} bytes, the host switches to page-mode spill files under the project-local `.vulcan/temp/` directory using names like `tool_output_overflow_page_md_<unix_ms>.md`. After spill, the host preserves the full rendered output instead of truncating it, subject to successful file creation, filesystem limits, and per-line safe-read chunking."
    )
}

/// Builds the shell-session-specific schema text for the `shell` argument so AI sees the current `auto` target plus nested-shell guidance.
/// 为 `shell_session` 的 `shell` 参数构建专用 schema 文本，使 AI 能看到当前 `auto` 目标及嵌套 shell 指引。
#[cfg(test)]
pub(crate) fn shell_session_schema_shell_description(
    shell_support: &HostCommandShellSupport,
) -> String {
    let windows_nested_shell_policy_note = if cfg!(windows) {
        " On Windows, `shell_session` rejects `shell: auto` when the command directly launches `bash`, `sh`, or `zsh`; use an explicit Unix-like shell selector for deterministic interactive behavior."
    } else {
        ""
    };
    format!(
        "{} For interactive nested shells such as `bash`, `sh`, or `zsh`, `auto` selects only the outer launcher and nested-shell inference is best-effort. The `start` result reports both the resolved launcher shell and the effective interactive newline shell. Prefer an explicit Unix-like shell when line-ending semantics matter or when deterministic behavior is required across environments.{}",
        shell_support.schema_shell_description(),
        windows_nested_shell_policy_note
    )
}

/// Builds the JSON Schema advertised to the model for the host-owned `shell_session` tool.
/// 为宿主自有 `shell_session` 工具构建对模型声明的 JSON Schema。
#[cfg(test)]
pub(crate) fn shell_session_args_schema(shell_support: &HostCommandShellSupport) -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["start", "read", "write", "stop"],
                "description": "Session action. `start` launches a new session, `read` polls buffered output, `write` appends stdin, and `stop` terminates the session."
            },
            "command_session_id": {
                "type": "string",
                "description": "Session identifier. Required for `read`, `write`, and `stop`."
            },
            "command": {
                "type": "string",
                "description": "Raw command text used only by `start`."
            },
            "cwd": {
                "type": "string",
                "description": "Optional working directory used only by `start`. When omitted, the host uses the current workspace root when available and otherwise falls back to the host process working directory. Relative paths resolve against the same base directory. Later `write` calls cannot change the working directory."
            },
            "shell": {
                "type": "string",
                "enum": shell_support.schema_shell_ids(),
                "description": shell_session_schema_shell_description(shell_support)
            },
            "stdin": {
                "type": "string",
                "description": "Optional stdin text. For `start`, it is written once after launch. For `write`, it is appended to the running session. No newline is added unless `append_newline` is true. For `start`, omitted `append_newline` defaults to `true` when `stdin` is provided. If `start` sends stdin without a trailing newline, the host rejects the request to avoid hidden concatenation with later writes."
            },
            "append_newline": {
                "type": "boolean",
                "description": "Optional newline submission flag for `start` and `write`. When true, the host appends the session-specific interactive newline (`\\n` for bash/sh/zsh, `\\r\\n` for cmd/PowerShell on Windows). For `start`, the default is `true` when `stdin` is provided and `false` otherwise. For `write`, the default is `false`. For `auto`, newline behavior follows the resolved launcher shell reported by the `start` result unless the host can directly infer a nested interactive shell from the command."
            },
            "after_seq": {
                "type": "integer",
                "description": "Optional output cursor for `read`. Only chunks with a higher sequence number are returned."
            },
            "wait_for_output_ms": {
                "type": "integer",
                "description": format!(
                    "Optional poll wait in milliseconds for `read`. Default: {DEFAULT_COMMAND_SESSION_READ_WAIT_MS} (do not wait; return immediately with whatever is already buffered)."
                )
            },
            "max_chunks": {
                "type": "integer",
                "description": format!(
                    "Optional maximum number of buffered chunks returned by `read`. Default: {DEFAULT_COMMAND_SESSION_READ_MAX_CHUNKS}. A chunk is one decoded stdout/stderr fragment emitted by the runtime; the host opportunistically coalesces adjacent undelivered fragments toward line-sized chunks, but chunk boundaries remain implementation-defined and may still be smaller than full lines or commands."
                )
            }
        },
        "required": ["action"],
        "additionalProperties": false
    })
}

/// Parses one raw model payload into the typed shell-session argument structure.
/// 将一份原始模型载荷解析为强类型 shell 会话参数结构。
#[cfg(test)]
pub(crate) fn parse_shell_session_tool_arguments(
    arguments: Value,
) -> Result<ShellSessionToolArgs, NativeToolError> {
    serde_json::from_value::<ShellSessionToolArgs>(arguments).map_err(|error| {
        NativeToolError::new(format!("Failed to parse shell_session arguments: {error}"))
    })
}

/// Resolves the controller session that owns the current shell-session tool invocation.
/// 解析拥有当前 shell 会话工具调用的控制器会话。
#[cfg(test)]
pub(crate) fn require_shell_session_owner_session_id(
    session_id: Option<SessionId>,
) -> Result<SessionId, NativeToolError> {
    session_id.ok_or_else(|| {
        NativeToolError::new(
            "The shell_session tool requires a stable controller session identifier, but no session_id was attached to this invocation.",
        )
    })
}

/// Builds one model-visible text render input for shell-session actions that do not need to include buffered output chunks.
/// 为不需要包含缓冲输出分片的 shell 会话动作构建一份模型可见文本渲染输入。
#[cfg(test)]
pub(crate) fn build_shell_session_summary_render_input(
    action: ShellSessionToolAction,
    snapshot: &CommandSessionSnapshot,
    extra_lines: Vec<String>,
) -> HostTextRenderInput {
    let mut lines = vec![
        format!("Action: {}", action.as_str()),
        format!("Command session: {}", snapshot.command_session_id),
        format!("Owner session: {}", snapshot.owner_session_id),
        format!("Command: {}", snapshot.command),
        format!("Shell: {}", snapshot.shell_label),
    ];
    if snapshot.launcher_shell_label != snapshot.shell_label {
        lines.push(format!("Launcher shell: {}", snapshot.launcher_shell_label));
    }
    lines.extend([
        format!(
            "Working directory: {}",
            normalized_workspace_path_text_from_path(&snapshot.cwd)
        ),
        format!("Status: {}", snapshot.status.as_str()),
        format!("Stdout bytes: {}", snapshot.stdout_bytes),
        format!("Stderr bytes: {}", snapshot.stderr_bytes),
        format!("Last sequence: {}", snapshot.last_seq),
        format!(
            "Pending stdin line: {}",
            if snapshot.pending_stdin_line {
                "yes"
            } else {
                "no"
            }
        ),
    ]);
    if let Some(exit_code) = snapshot.exit_code {
        lines.push(format!("Exit code: {exit_code}"));
    }
    if let Some(reason) = snapshot.failure_reason.as_deref() {
        lines.push(format!("Reason: {reason}"));
    }
    lines.extend(extra_lines);
    let content = lines.join("\n");
    HostTextRenderInput {
        content_bytes: content.len(),
        content_lines: content.lines().count(),
        content,
        overflow_mode: Some(HostToolOverflowMode::Page),
        template_hint: None,
    }
}

/// Builds one model-visible text render input for shell-session read actions that should include returned output chunks.
/// 为需要包含返回输出分片的 shell 会话 read 动作构建一份模型可见文本渲染输入。
#[cfg(test)]
pub(crate) fn build_shell_session_read_render_input(
    snapshot: &CommandSessionSnapshot,
    returned_chunks: &[CommandSessionOutputChunk],
    has_more: bool,
    requested_after_seq: Option<u64>,
) -> HostTextRenderInput {
    let extra_lines = vec![
        format!(
            "Requested after sequence: {}",
            requested_after_seq.unwrap_or(0)
        ),
        format!("Returned chunks: {}", returned_chunks.len()),
        format!(
            "Has more buffered output: {}",
            if has_more { "yes" } else { "no" }
        ),
        String::from("Read model: polling"),
        String::from(
            "Chunk semantics: implementation-defined decoded stdout/stderr fragments; boundaries may be smaller than full lines or commands.",
        ),
    ];
    let mut render_input = build_shell_session_summary_render_input(
        ShellSessionToolAction::Read,
        snapshot,
        extra_lines,
    );
    if returned_chunks.is_empty() {
        render_input.content.push_str("\n\n(no new output)");
    } else {
        for chunk in returned_chunks {
            render_input.content.push_str(&format!(
                "\n\n[{} #{}]\n{}",
                chunk.stream.as_str(),
                chunk.seq,
                chunk.text
            ));
        }
    }
    render_input.content_bytes = render_input.content.len();
    render_input.content_lines = render_input.content.lines().count();
    render_input
}

pub(crate) fn build_shell_session_host_payload(
    action: ShellSessionToolAction,
    snapshot: &CommandSessionSnapshot,
    output_overflowed: bool,
    extra_fields: Value,
) -> Value {
    let mut payload = json!({
        "action": action.as_str(),
        "owner_session_id": snapshot.owner_session_id.to_string(),
        "command_session_id": snapshot.command_session_id.to_string(),
        "command": snapshot.command.clone(),
        "cwd": normalized_workspace_path_text_from_path(&snapshot.cwd),
        "shell": snapshot.shell_label.clone(),
        "launcher_shell": snapshot.launcher_shell_label.clone(),
        "status": snapshot.status.as_str(),
        "exit_code": snapshot.exit_code,
        "stdout_bytes": snapshot.stdout_bytes,
        "stderr_bytes": snapshot.stderr_bytes,
        "last_seq": snapshot.last_seq,
        "pending_stdin_line": snapshot.pending_stdin_line,
        "failure_reason": snapshot.failure_reason.clone(),
        "output_overflowed": output_overflowed,
    });
    if let (Some(payload_object), Value::Object(extra_object)) =
        (payload.as_object_mut(), extra_fields)
    {
        payload_object.extend(extra_object);
    }
    payload
}

/// Executes one shell-session start action and returns the unified tool invocation result.
/// 执行一次 shell 会话 start 动作并返回统一工具调用结果。
#[cfg(test)]
pub(crate) async fn execute_shell_session_start_action(
    request: NativeToolExecutionRequest,
    parsed_args: ShellSessionToolArgs,
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
                "The `start` action for shell_session requires a non-empty `command`.",
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
    let append_newline =
        effective_start_append_newline(parsed_args.append_newline, parsed_args.stdin.as_deref());
    let initial_stdin = parsed_args.stdin.clone();
    validate_shell_session_start_stdin_policy(initial_stdin.as_deref(), append_newline)?;
    let snapshot = command_session_runtime
        .start_session(CommandSessionStartRequest {
            owner_session_id,
            display_name: None,
            command: command_text.to_string(),
            shell_label: shell_behavior.interactive_shell_label.clone(),
            launcher_shell_label: shell_behavior.launcher_shell_label.clone(),
            interactive_newline: shell_specific_newline(
                shell_behavior.interactive_shell_label.as_str(),
            )
            .to_string(),
            cwd: effective_cwd,
            launcher_program,
            launcher_args,
            initial_stdin,
            initial_stdin_append_newline: append_newline,
            backend: CommandSessionBackend::Pipe,
        })
        .await
        .map_err(NativeToolError::new)?;
    let mut extra_lines = vec![
        format!(
            "Initial stdin provided: {}",
            if parsed_args.stdin.is_some() {
                "yes"
            } else {
                "no"
            }
        ),
        format!(
            "Automatic newline submission: {}",
            if append_newline { "yes" } else { "no" }
        ),
        format!(
            "Pending stdin line after start: {}",
            if snapshot.pending_stdin_line {
                "yes"
            } else {
                "no"
            }
        ),
        format!(
            "Resolved launcher shell: {}",
            shell_behavior.launcher_shell_label
        ),
        format!("Requested shell selector: {}", requested_shell_selector),
        format!(
            "Interactive newline shell: {}",
            shell_behavior.interactive_shell_label
        ),
        format!(
            "Nested shell inference: {}",
            if shell_behavior.nested_shell_inferred {
                "yes"
            } else {
                "no"
            }
        ),
        String::from("Read model: polling"),
    ];
    if shell_behavior.nested_shell_inferred {
        extra_lines.push(String::from(
            "Warning: the command appears to enter a nested interactive shell. `auto` still resolved the outer launcher shell shown above, and nested-shell inference remains best-effort. Prefer an explicit shell selector when newline semantics must be deterministic.",
        ));
    }
    if shell_selector_uses_auto(parsed_args.shell.as_deref()) {
        extra_lines.push(String::from(
            "Auto-shell note: `auto` is resolved by the host at session start. Different hosts, installed shells, or nested-shell handoff can change the effective behavior. Use an explicit `shell` value for deterministic interactive sessions.",
        ));
    }
    let render_input = build_shell_session_summary_render_input(
        ShellSessionToolAction::Start,
        &snapshot,
        extra_lines,
    );
    let budget = default_shell_command_budget_snapshot();
    let host_payload = build_shell_session_host_payload(
        ShellSessionToolAction::Start,
        &snapshot,
        shell_command_output_exceeds_inline_budget(&render_input, &budget),
        json!({
            "requested_shell": requested_shell_selector,
            "nested_shell_inferred": shell_behavior.nested_shell_inferred,
            "append_newline_effective": append_newline,
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
                kind: String::from("command_session"),
                payload: host_payload,
            }),
    )
}

/// Executes one shell-session read action and returns the unified tool invocation result.
/// 执行一次 shell 会话 read 动作并返回统一工具调用结果。
#[cfg(test)]
pub(crate) async fn execute_shell_session_read_action(
    request: NativeToolExecutionRequest,
    parsed_args: ShellSessionToolArgs,
    owner_session_id: SessionId,
    command_session_runtime: Arc<CommandSessionRuntime>,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let command_session_id = parse_command_session_id(parsed_args.command_session_id.as_deref())?;
    let spill_root = resolve_shell_command_spill_root(
        request.spill_root.as_deref(),
        request.workspace_root.as_deref(),
        request
            .workspace_root
            .as_deref()
            .unwrap_or_else(|| Path::new(".")),
    );
    let read_outcome = command_session_runtime
        .read_session(
            owner_session_id,
            command_session_id,
            parsed_args.after_seq,
            parsed_args.wait_for_output_ms,
            parsed_args.max_chunks,
        )
        .await
        .map_err(NativeToolError::new)?;
    let render_input = build_shell_session_read_render_input(
        &read_outcome.snapshot,
        &read_outcome.returned_chunks,
        read_outcome.has_more,
        parsed_args.after_seq,
    );
    let budget = default_shell_command_budget_snapshot();
    let host_payload = build_shell_session_host_payload(
        ShellSessionToolAction::Read,
        &read_outcome.snapshot,
        shell_command_output_exceeds_inline_budget(&render_input, &budget),
        json!({
            "requested_after_seq": parsed_args.after_seq.unwrap_or(0),
            "returned_chunk_count": read_outcome.returned_chunks.len(),
            "has_more": read_outcome.has_more,
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
                kind: String::from("command_session"),
                payload: host_payload,
            }),
    )
}

/// Executes one shell-session write action and returns the unified tool invocation result.
/// 执行一次 shell 会话 write 动作并返回统一工具调用结果。
#[cfg(test)]
pub(crate) async fn execute_shell_session_write_action(
    parsed_args: ShellSessionToolArgs,
    owner_session_id: SessionId,
    command_session_runtime: Arc<CommandSessionRuntime>,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let command_session_id = parse_command_session_id(parsed_args.command_session_id.as_deref())?;
    let input_text = parsed_args.stdin.ok_or_else(|| {
        NativeToolError::new("The `write` action for shell_session requires `stdin`.")
    })?;
    let append_newline = parsed_args.append_newline.unwrap_or(false);
    let write_outcome = command_session_runtime
        .write_session(
            owner_session_id,
            command_session_id,
            input_text,
            append_newline,
        )
        .await
        .map_err(NativeToolError::new)?;
    let mut extra_lines = vec![
        format!("Bytes written: {}", write_outcome.bytes_written),
        format!(
            "Appended newline: {}",
            if append_newline { "yes" } else { "no" }
        ),
    ];
    if write_outcome.snapshot.pending_stdin_line {
        extra_lines.push(String::from(
            "Warning: the session stdin is still left on an open input line. Subsequent writes will continue the same line until a newline is sent.",
        ));
    }
    let render_input = build_shell_session_summary_render_input(
        ShellSessionToolAction::Write,
        &write_outcome.snapshot,
        extra_lines,
    );
    let budget = default_shell_command_budget_snapshot();
    let host_payload = build_shell_session_host_payload(
        ShellSessionToolAction::Write,
        &write_outcome.snapshot,
        shell_command_output_exceeds_inline_budget(&render_input, &budget),
        json!({
            "bytes_written": write_outcome.bytes_written,
            "appended_newline": append_newline,
        }),
    );

    Ok(
        VulcanToolInvocationResult::from_model_output(Value::String(render_input.content))
            .with_host_result(VulcanToolHostResult {
                kind: String::from("command_session"),
                payload: host_payload,
            }),
    )
}

/// Executes one shell-session stop action and returns the unified tool invocation result.
/// 执行一次 shell 会话 stop 动作并返回统一工具调用结果。
#[cfg(test)]
pub(crate) async fn execute_shell_session_stop_action(
    parsed_args: ShellSessionToolArgs,
    owner_session_id: SessionId,
    command_session_runtime: Arc<CommandSessionRuntime>,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let command_session_id = parse_command_session_id(parsed_args.command_session_id.as_deref())?;
    let snapshot = command_session_runtime
        .stop_session(
            owner_session_id,
            command_session_id,
            Some(DEFAULT_COMMAND_SESSION_STOP_WAIT_MS),
        )
        .await
        .map_err(NativeToolError::new)?;
    let render_input = build_shell_session_summary_render_input(
        ShellSessionToolAction::Stop,
        &snapshot,
        Vec::new(),
    );
    let budget = default_shell_command_budget_snapshot();
    let host_payload = build_shell_session_host_payload(
        ShellSessionToolAction::Stop,
        &snapshot,
        shell_command_output_exceeds_inline_budget(&render_input, &budget),
        json!({
            "stop_wait_ms": DEFAULT_COMMAND_SESSION_STOP_WAIT_MS,
        }),
    );

    Ok(
        VulcanToolInvocationResult::from_model_output(Value::String(render_input.content))
            .with_host_result(VulcanToolHostResult {
                kind: String::from("command_session"),
                payload: host_payload,
            }),
    )
}

/// Executes the host-owned `shell_session` tool and returns a unified tool invocation result.
/// 执行宿主自有 `shell_session` 工具并返回统一工具调用结果。
#[cfg(test)]
pub(crate) async fn execute_shell_session_tool(
    request: NativeToolExecutionRequest,
    command_session_runtime: Arc<CommandSessionRuntime>,
    shell_support: &HostCommandShellSupport,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let owner_session_id = require_shell_session_owner_session_id(request.session_id)?;
    let parsed_args = parse_shell_session_tool_arguments(request.arguments.clone())?;
    match parsed_args.action {
        ShellSessionToolAction::Start => {
            execute_shell_session_start_action(
                request,
                parsed_args,
                owner_session_id,
                command_session_runtime,
                shell_support,
            )
            .await
        }
        ShellSessionToolAction::Read => {
            execute_shell_session_read_action(
                request,
                parsed_args,
                owner_session_id,
                command_session_runtime,
            )
            .await
        }
        ShellSessionToolAction::Write => {
            execute_shell_session_write_action(
                parsed_args,
                owner_session_id,
                command_session_runtime,
            )
            .await
        }
        ShellSessionToolAction::Stop => {
            execute_shell_session_stop_action(
                parsed_args,
                owner_session_id,
                command_session_runtime,
            )
            .await
        }
    }
}
