use super::*;

/// Builds the one-shot shell-command host-owned native tool.
/// 构建一次性 shell 命令宿主自有原生工具。
pub(crate) fn build_shell_command_tool(
    shell_support: HostCommandShellSupport,
) -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from("shell_command"),
        name: String::from("shell_command"),
        description: shell_command_tool_description(),
        help_text: None,
        ai_visible: true,
        available_in_plan: true,
    };

    build_native_tool_definition(
        descriptor,
        shell_command_args_schema(&shell_support),
        move |request: NativeToolExecutionRequest| {
            let shell_support = shell_support.clone();
            Box::pin(async move { execute_shell_command_tool(request, &shell_support).await })
        },
    )
}

/// One typed argument payload accepted by the host-owned `shell_command` native tool.
/// 宿主自有 `shell_command` 原生工具接受的一份强类型参数载荷。
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ShellCommandToolArgs {
    /// Raw command string that should be executed by the selected shell.
    /// 应由所选 shell 执行的原始命令字符串。
    command: String,
    /// Optional working directory override resolved relative to the workspace root when needed.
    /// 需要时相对工作区根目录解析的可选工作目录覆盖值。
    cwd: Option<String>,
    /// Optional timeout budget in milliseconds applied to the command run.
    /// 应用于命令执行的可选毫秒级超时预算。
    timeout_ms: Option<u64>,
    /// Optional stdin text written to the child process before waiting for output.
    /// 在等待输出前写入子进程的可选标准输入文本。
    stdin: Option<String>,
    /// Optional shell family selector such as `auto`, `powershell`, `cmd`, or `bash`.
    /// 可选 shell 类型选择器，例如 `auto`、`powershell`、`cmd` 或 `bash`。
    shell: Option<String>,
}

/// One internal execution status returned by the host-owned shell-command runner.
/// 宿主自有 shell 命令执行器返回的一种内部执行状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShellCommandExecutionStatus {
    Completed,
    Failed,
    TimedOut,
}

impl ShellCommandExecutionStatus {
    /// Render the execution status into one stable host-result label.
    /// 将执行状态渲染为稳定的宿主结果标签。
    fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::TimedOut => "timed_out",
        }
    }
}

/// Maximum inline byte budget used before shell tool output switches into page mode spill rendering.
/// shell 工具输出在切换到分页落盘渲染前使用的最大内联字节预算。
pub(crate) const SHELL_INLINE_OUTPUT_LIMIT_BYTES: u64 = 200_000;

/// One normalized shell-command execution report used by both model text and host metadata.
/// 同时供模型文本与宿主元数据复用的一份规范化 shell 命令执行报告。
#[derive(Debug, Clone)]
pub(crate) struct ShellCommandExecutionReport {
    /// Final execution status observed by the host.
    /// 宿主观察到的最终执行状态。
    status: ShellCommandExecutionStatus,
    /// Stable shell label chosen by the host.
    /// 由宿主选定的稳定 shell 标签。
    shell_label: String,
    /// Effective working directory used for the command run.
    /// 本次命令执行使用的实际工作目录。
    cwd: PathBuf,
    /// Optional process exit code when the child exited normally.
    /// 子进程正常退出时的可选进程退出码。
    exit_code: Option<i32>,
    /// Total wall-clock duration captured by the host in milliseconds.
    /// 由宿主记录的总耗时，单位为毫秒。
    duration_ms: u64,
    /// Decoded standard-output text captured from the child process.
    /// 从子进程捕获并解码得到的标准输出文本。
    stdout_text: String,
    /// Decoded standard-error text captured from the child process.
    /// 从子进程捕获并解码得到的标准错误文本。
    stderr_text: String,
    /// Optional failure reason synthesized by the host for spawn or timeout failures.
    /// 宿主为启动失败或超时失败综合生成的可选失败原因。
    failure_reason: Option<String>,
}

/// Builds the English AI-facing description for the one-shot shell command tool.
/// 构建一次性 shell 命令工具面向 AI 的英文描述。
pub(crate) fn shell_command_tool_description() -> String {
    format!(
        "Run a one-shot command in the host environment and return the full decoded stdout/stderr plus the exit status, not just a summary. Default `timeout_ms`: unlimited. If `timeout_ms` is omitted, no timeout is applied and the host waits until the process exits. When the inline result exceeds {SHELL_INLINE_OUTPUT_LIMIT_BYTES} bytes, the host switches to page-mode spill files under the project-local `.vulcan/temp/` directory using names like `tool_output_overflow_page_md_<unix_ms>.md`. After spill, the host preserves the full rendered output instead of truncating it, subject to successful file creation, filesystem limits, and per-line safe-read chunking."
    )
}

/// Builds the JSON Schema advertised to the model for the host-owned `shell_command` tool.
/// 为宿主自有 `shell_command` 工具构建对模型声明的 JSON Schema。
pub(crate) fn shell_command_args_schema(shell_support: &HostCommandShellSupport) -> Value {
    json!({
        "type": "object",
        "properties": {
            "command": {
                "type": "string",
                "description": format!(
                    "Raw command text executed by the selected shell. On success, the tool returns the full decoded stdout/stderr plus exit status, not just a summary. If the rendered result exceeds {SHELL_INLINE_OUTPUT_LIMIT_BYTES} bytes, the host spills the full output into `.vulcan/temp/tool_output_overflow_page_md_<unix_ms>.md`."
                )
            },
            "cwd": {
                "type": "string",
                "description": "Optional working directory used only for this one-shot command invocation. When omitted, the host uses the current workspace root when available and otherwise falls back to the host process working directory. Relative paths resolve against the same base directory."
            },
            "timeout_ms": {
                "type": "integer",
                "description": "Optional hard timeout in milliseconds. Default: unlimited. When omitted, no timeout is applied and the host waits until the process exits."
            },
            "stdin": {
                "type": "string",
                "description": "Optional stdin text written once before waiting for process output. No newline is added automatically."
            },
            "shell": {
                "type": "string",
                "enum": shell_support.schema_shell_ids(),
                "description": shell_support.schema_shell_description()
            }
        },
        "required": ["command"],
        "additionalProperties": false
    })
}

pub(crate) fn parse_shell_command_tool_arguments(
    arguments: Value,
) -> Result<ShellCommandToolArgs, NativeToolError> {
    let parsed = serde_json::from_value::<ShellCommandToolArgs>(arguments).map_err(|error| {
        NativeToolError::new(format!("Failed to parse shell_command arguments: {error}"))
    })?;
    if parsed.command.trim().is_empty() {
        return Err(NativeToolError::new(
            "The `command` argument for shell_command cannot be empty.",
        ));
    }
    Ok(parsed)
}

pub(crate) fn resolve_shell_command_working_directory(
    workspace_root: Option<&Path>,
    raw_cwd: Option<&str>,
) -> Result<PathBuf, NativeToolError> {
    let workspace_root = workspace_root.filter(|path| !path.as_os_str().is_empty());
    let fallback_directory = std::env::current_dir().map_err(|error| {
        NativeToolError::new(format!(
            "Failed to read the current working directory: {error}"
        ))
    })?;
    let Some(raw_cwd) = raw_cwd.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(workspace_root
            .map(Path::to_path_buf)
            .unwrap_or(fallback_directory));
    };
    let requested_path = PathBuf::from(raw_cwd);
    if requested_path.is_absolute() {
        return Ok(requested_path);
    }
    let base_directory = workspace_root
        .map(Path::to_path_buf)
        .unwrap_or(fallback_directory);
    Ok(base_directory.join(requested_path))
}

/// Resolves the spill root used by one shell-command invocation when oversized output must be paged.
/// 为单次 shell 命令调用解析在超长输出需要分页时使用的超限目录。
pub(crate) fn resolve_shell_command_spill_root(
    explicit_spill_root: Option<&Path>,
    workspace_root: Option<&Path>,
    effective_cwd: &Path,
) -> Option<PathBuf> {
    if let Some(spill_root) = explicit_spill_root.filter(|path| !path.as_os_str().is_empty()) {
        return Some(spill_root.to_path_buf());
    }
    workspace_root
        .filter(|path| !path.as_os_str().is_empty())
        .or(Some(effective_cwd))
        .and_then(|path| project_temp_maintenance::ensure_project_temp_dir(path).ok())
}

/// Resolves the concrete shell executable, argument vector, and host-visible shell label.
/// 解析具体 shell 可执行文件、参数向量以及宿主可见 shell 标签。
pub(crate) fn resolve_shell_command_launcher(
    shell_support: &HostCommandShellSupport,
    shell_name: Option<&str>,
    command_text: &str,
) -> Result<(String, Vec<String>, String), NativeToolError> {
    let resolved_shell = shell_support
        .resolve_requested_shell(shell_name)
        .map_err(NativeToolError::new)?;
    Ok(resolved_shell.build_launcher(command_text))
}

/// Executes one shell command through the selected launcher and returns a normalized execution report.
/// 通过选定启动器执行单条 shell 命令，并返回规范化执行报告。
pub(crate) async fn execute_shell_command_process(
    parsed_args: &ShellCommandToolArgs,
    effective_cwd: &Path,
    shell_support: &HostCommandShellSupport,
) -> ShellCommandExecutionReport {
    let started_at = std::time::Instant::now();
    let (program, program_args, shell_label) = match resolve_shell_command_launcher(
        shell_support,
        parsed_args.shell.as_deref(),
        &parsed_args.command,
    ) {
        Ok(resolved) => resolved,
        Err(error) => {
            return ShellCommandExecutionReport {
                status: ShellCommandExecutionStatus::Failed,
                shell_label: parsed_args
                    .shell
                    .clone()
                    .unwrap_or_else(|| String::from("auto")),
                cwd: effective_cwd.to_path_buf(),
                exit_code: None,
                duration_ms: 0,
                stdout_text: String::new(),
                stderr_text: String::new(),
                failure_reason: Some(error.to_string()),
            };
        }
    };

    let mut command = TokioCommand::new(&program);
    command.args(&program_args);
    command.current_dir(effective_cwd);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command.kill_on_drop(true);
    if parsed_args.stdin.is_some() {
        command.stdin(Stdio::piped());
    }

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return ShellCommandExecutionReport {
                status: ShellCommandExecutionStatus::Failed,
                shell_label,
                cwd: effective_cwd.to_path_buf(),
                exit_code: None,
                duration_ms: started_at.elapsed().as_millis() as u64,
                stdout_text: String::new(),
                stderr_text: String::new(),
                failure_reason: Some(format!("Failed to start the command: {error}")),
            };
        }
    };

    if let Some(stdin_text) = parsed_args.stdin.as_deref() {
        match child.stdin.take() {
            Some(mut child_stdin) => {
                if let Err(error) = child_stdin.write_all(stdin_text.as_bytes()).await {
                    return ShellCommandExecutionReport {
                        status: ShellCommandExecutionStatus::Failed,
                        shell_label,
                        cwd: effective_cwd.to_path_buf(),
                        exit_code: None,
                        duration_ms: started_at.elapsed().as_millis() as u64,
                        stdout_text: String::new(),
                        stderr_text: String::new(),
                        failure_reason: Some(format!("Failed to write command stdin: {error}")),
                    };
                }
            }
            None => {
                return ShellCommandExecutionReport {
                    status: ShellCommandExecutionStatus::Failed,
                    shell_label,
                    cwd: effective_cwd.to_path_buf(),
                    exit_code: None,
                    duration_ms: started_at.elapsed().as_millis() as u64,
                    stdout_text: String::new(),
                    stderr_text: String::new(),
                    failure_reason: Some(String::from(
                        "The command did not expose a writable stdin channel.",
                    )),
                };
            }
        }
    }

    let wait_future = child.wait_with_output();
    let waited_output = match parsed_args.timeout_ms.filter(|value| *value > 0) {
        Some(timeout_ms) => match timeout(Duration::from_millis(timeout_ms), wait_future).await {
            Ok(output_result) => match output_result {
                Ok(output) => Some(output),
                Err(error) => {
                    return ShellCommandExecutionReport {
                        status: ShellCommandExecutionStatus::Failed,
                        shell_label,
                        cwd: effective_cwd.to_path_buf(),
                        exit_code: None,
                        duration_ms: started_at.elapsed().as_millis() as u64,
                        stdout_text: String::new(),
                        stderr_text: String::new(),
                        failure_reason: Some(format!(
                            "Failed while waiting for command output: {error}"
                        )),
                    };
                }
            },
            Err(_) => {
                return ShellCommandExecutionReport {
                    status: ShellCommandExecutionStatus::TimedOut,
                    shell_label,
                    cwd: effective_cwd.to_path_buf(),
                    exit_code: None,
                    duration_ms: started_at.elapsed().as_millis() as u64,
                    stdout_text: String::new(),
                    stderr_text: String::new(),
                    failure_reason: Some(format!(
                        "The command exceeded the timeout limit ({} ms).",
                        timeout_ms
                    )),
                };
            }
        },
        None => match wait_future.await {
            Ok(output) => Some(output),
            Err(error) => {
                return ShellCommandExecutionReport {
                    status: ShellCommandExecutionStatus::Failed,
                    shell_label,
                    cwd: effective_cwd.to_path_buf(),
                    exit_code: None,
                    duration_ms: started_at.elapsed().as_millis() as u64,
                    stdout_text: String::new(),
                    stderr_text: String::new(),
                    failure_reason: Some(format!(
                        "Failed while waiting for command output: {error}"
                    )),
                };
            }
        },
    };

    let output = waited_output.expect("shell command output should exist after successful wait");
    let exit_code = output.status.code();
    let stdout_text = decode_shell_output_bytes(&output.stdout);
    let stderr_text = decode_shell_output_bytes(&output.stderr);
    let status = if output.status.success() {
        ShellCommandExecutionStatus::Completed
    } else {
        ShellCommandExecutionStatus::Failed
    };

    ShellCommandExecutionReport {
        status,
        shell_label,
        cwd: effective_cwd.to_path_buf(),
        exit_code,
        duration_ms: started_at.elapsed().as_millis() as u64,
        stdout_text,
        stderr_text,
        failure_reason: None,
    }
}

/// Builds the model-visible text render input for one normalized shell-command execution report.
/// 为单条规范化 shell 命令执行报告构建模型可见文本渲染输入。
pub(crate) fn build_shell_command_text_render_input(
    parsed_args: &ShellCommandToolArgs,
    execution_report: &ShellCommandExecutionReport,
) -> HostTextRenderInput {
    let mut lines = vec![
        format!("Command: {}", parsed_args.command),
        format!("Shell: {}", execution_report.shell_label),
        format!(
            "Working directory: {}",
            normalized_workspace_path_text_from_path(&execution_report.cwd)
        ),
        format!("Status: {}", execution_report.status.as_str()),
        format!("Duration: {} ms", execution_report.duration_ms),
    ];
    if let Some(exit_code) = execution_report.exit_code {
        lines.push(format!("Exit code: {exit_code}"));
    }
    if let Some(reason) = execution_report.failure_reason.as_deref() {
        lines.push(format!("Reason: {reason}"));
    }

    let mut content = lines.join("\n");
    let trimmed_stdout = execution_report.stdout_text.trim_end();
    let trimmed_stderr = execution_report.stderr_text.trim_end();
    if !trimmed_stdout.is_empty() {
        content.push_str("\n\n[stdout]\n");
        content.push_str(trimmed_stdout);
    }
    if !trimmed_stderr.is_empty() {
        content.push_str("\n\n[stderr]\n");
        content.push_str(trimmed_stderr);
    }
    if trimmed_stdout.is_empty()
        && trimmed_stderr.is_empty()
        && execution_report.failure_reason.is_none()
    {
        content.push_str("\n\n(no output)");
    }

    HostTextRenderInput {
        content_bytes: content.len(),
        content_lines: content.lines().count(),
        content,
        overflow_mode: Some(HostToolOverflowMode::Page),
        template_hint: None,
    }
}

/// Builds the default budget snapshot reused by host-owned shell command rendering.
/// 构建供宿主自有 shell 命令结果渲染复用的默认预算快照。
pub(crate) fn default_shell_command_budget_snapshot() -> ToolResultBudgetSnapshot {
    ToolResultBudgetSnapshot {
        tool_result: EffectiveBudgetScope {
            bytes: SHELL_INLINE_OUTPUT_LIMIT_BYTES,
            lines: -1,
        },
        file_read: EffectiveBudgetScope {
            bytes: SHELL_INLINE_OUTPUT_LIMIT_BYTES,
            lines: -1,
        },
    }
}

/// Decide whether the combined shell-command text already exceeds the inline tool-result budget.
/// 判断合并后的 shell 命令文本是否已经超出内联工具结果预算。
pub(crate) fn shell_command_output_exceeds_inline_budget(
    render_input: &HostTextRenderInput,
    budget: &ToolResultBudgetSnapshot,
) -> bool {
    let tool_result_budget = &budget.tool_result;
    let exceeds_bytes = render_input.content_bytes > tool_result_budget.bytes as usize;
    let exceeds_lines = tool_result_budget.lines > 0
        && render_input.content_lines > tool_result_budget.lines as usize;
    exceeds_bytes || exceeds_lines
}

/// Builds the host-only structured payload returned for one shell-command execution.
/// 为单次 shell 命令执行构建宿主专用结构化返回载荷。
pub(crate) fn build_shell_command_host_payload(
    session_id: Option<SessionId>,
    parsed_args: &ShellCommandToolArgs,
    execution_report: &ShellCommandExecutionReport,
    combined_output_overflowed: bool,
) -> Value {
    json!({
        "session_id": session_id.map(|value| value.to_string()),
        "command": parsed_args.command,
        "cwd": normalized_workspace_path_text_from_path(&execution_report.cwd),
        "shell": execution_report.shell_label,
        "status": execution_report.status.as_str(),
        "exit_code": execution_report.exit_code,
        "duration_ms": execution_report.duration_ms,
        "stdout_bytes": execution_report.stdout_text.len(),
        "stderr_bytes": execution_report.stderr_text.len(),
        "combined_output_overflowed": combined_output_overflowed,
        "failure_reason": execution_report.failure_reason,
    })
}

pub(crate) async fn execute_shell_command_tool(
    request: NativeToolExecutionRequest,
    shell_support: &HostCommandShellSupport,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let parsed_args = parse_shell_command_tool_arguments(request.arguments)?;
    let effective_cwd = resolve_shell_command_working_directory(
        request.workspace_root.as_deref(),
        parsed_args.cwd.as_deref(),
    )?;
    let spill_root = resolve_shell_command_spill_root(
        request.spill_root.as_deref(),
        request.workspace_root.as_deref(),
        &effective_cwd,
    );
    let execution_report =
        execute_shell_command_process(&parsed_args, &effective_cwd, shell_support).await;
    let render_input = build_shell_command_text_render_input(&parsed_args, &execution_report);
    let budget = default_shell_command_budget_snapshot();
    let host_payload = build_shell_command_host_payload(
        request.session_id,
        &parsed_args,
        &execution_report,
        shell_command_output_exceeds_inline_budget(&render_input, &budget),
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
                kind: String::from("command_run"),
                payload: host_payload,
            }),
    )
}
