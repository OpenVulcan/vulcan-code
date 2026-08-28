use super::*;
use crate::agent_runtime::conversation_hooks::HOOK_TOOL_SHELL_COMMAND_BEFORE_EXECUTE;
use crate::agent_runtime::vulcan_plugin_hooks::dispatch_vulcan_plugin_hook;
use std::collections::BTreeMap;
use vulcan_hooks::{HookActor, HookCapabilitySet, HookContext, HookScope};
use vulcan_host_tools::process_creation::{
    apply_child_path_prefixes, new_background_tokio_command,
};

/// Builds the one-shot shell-command host-owned native tool.
/// 构建一次性 shell 命令宿主自有原生工具。
pub(crate) fn build_shell_command_tool(
    shell_support: HostCommandShellSupport,
    plugin_runtime: Option<vulcan_plugin_runtime::VulcanPluginRuntimeManager>,
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
            let plugin_runtime = plugin_runtime.clone();
            Box::pin(async move {
                execute_shell_command_tool(request, &shell_support, plugin_runtime.as_ref()).await
            })
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

/// Process execution options derived from generic plugin hooks and host policy.
/// 根据通用插件 Hook 与宿主策略派生的进程执行选项。
#[derive(Debug, Clone, Default)]
pub(crate) struct ShellCommandProcessOptions {
    /// Ordered directories prepended to PATH for this child process.
    /// 为当前子进程前置到 PATH 的有序目录集合。
    path_prefixes: Vec<PathBuf>,
    /// Exact environment contributions keyed by variable name.
    /// 以变量名为键的精确环境贡献。
    environment: BTreeMap<String, String>,
}

/// One generic user-visible display contribution returned by a shell transform hook.
/// 一个由 shell 转换 Hook 返回的通用用户可见展示贡献。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ShellCommandHookContribution {
    /// Optional compact badge rendered on the command card.
    /// 渲染在命令卡片上的可选紧凑徽标。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    badge: Option<String>,
    /// Short stable contribution label.
    /// 简短稳定的贡献标签。
    label: String,
    /// Human-readable contribution value.
    /// 人类可读的贡献值。
    value: String,
}

/// One request to prepend the directory of a hook-owning logical binary to PATH.
/// 一个把 Hook 所属逻辑二进制目录前置到 PATH 的请求。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ShellCommandBinaryPathRequest {
    /// Exact completed hook registration making the request.
    /// 发出请求的精确已完成 Hook 注册标识。
    registration_id: String,
    /// Exact binary id declared by the owning plugin.
    /// 所属插件声明的精确二进制标识。
    binary_id: String,
}

/// Exact transform payload for `tool.shell_command.before_execute`.
/// `tool.shell_command.before_execute` 使用的精确转换载荷。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ShellCommandHookPayload {
    /// Protocol schema version.
    /// 协议 Schema 版本。
    schema_version: u32,
    /// Effective shell command text.
    /// 生效 shell 命令文本。
    command: String,
    /// Immutable normalized working directory.
    /// 不可变的规范化工作目录。
    cwd: String,
    /// Immutable selected shell identifier.
    /// 不可变的选定 shell 标识。
    shell: String,
    /// Environment variables contributed to the child process.
    /// 贡献给子进程的环境变量。
    environment: BTreeMap<String, String>,
    /// Logical binary-directory requests bound to completed hooks.
    /// 绑定到已完成 Hook 的逻辑二进制目录请求。
    #[serde(default)]
    binary_path_requests: Vec<ShellCommandBinaryPathRequest>,
    /// Generic display contributions rendered by the host.
    /// 由宿主渲染的通用展示贡献。
    #[serde(default)]
    contributions: Vec<ShellCommandHookContribution>,
}

/// Auditable generic transform report retained in model text and host metadata.
/// 保留在模型文本与宿主元数据中的可审计通用转换报告。
#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct ShellCommandHookReport {
    /// Original command before plugin hooks.
    /// 插件 Hook 前的原始命令。
    original_command: String,
    /// Effective command after plugin hooks.
    /// 插件 Hook 后的生效命令。
    effective_command: String,
    /// Hook registrations completed successfully.
    /// 成功完成的 Hook 注册集合。
    completed_hooks: Vec<String>,
    /// Generic display contributions.
    /// 通用展示贡献。
    contributions: Vec<ShellCommandHookContribution>,
    /// Serialized non-fatal dispatcher diagnostics.
    /// 序列化的非致命调度诊断。
    diagnostics: Vec<Value>,
    /// Serialized non-fatal handler failures.
    /// 序列化的非致命处理器失败。
    failures: Vec<Value>,
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
    let fallback_directory =
        crate::runtime_assets::native_host_path(&std::env::current_dir().map_err(|error| {
            NativeToolError::new(format!(
                "Failed to read the current working directory: {error}"
            ))
        })?);
    let Some(raw_cwd) = raw_cwd.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(workspace_root
            .map(crate::runtime_assets::native_host_path)
            .unwrap_or(fallback_directory));
    };
    let requested_path = PathBuf::from(raw_cwd);
    if requested_path.is_absolute() {
        return Ok(crate::runtime_assets::native_host_path(&requested_path));
    }
    let base_directory = workspace_root
        .map(crate::runtime_assets::native_host_path)
        .unwrap_or(fallback_directory);
    Ok(crate::runtime_assets::native_host_path(
        &base_directory.join(requested_path),
    ))
}

/// Resolves the spill root used by one shell-command invocation when oversized output must be paged.
/// 为单次 shell 命令调用解析在超长输出需要分页时使用的超限目录。
///
/// # Parameters
/// # 参数
/// - `explicit_spill_root`: Optional host-selected path that takes precedence unchanged.
/// - `explicit_spill_root`：可选的宿主指定路径，保持原值并具有最高优先级。
/// - `workspace_root`: Optional workspace used to derive `.vulcan/temp`.
/// - `workspace_root`：用于派生 `.vulcan/temp` 的可选工作区。
/// - `effective_cwd`: Concrete command directory used when no workspace exists.
/// - `effective_cwd`：不存在工作区时使用的实际命令目录。
///
/// # Returns
/// # 返回值
/// Returns the selected path without touching the filesystem; the page writer creates it lazily.
/// 返回选定路径且不触碰文件系统；分页写入器会延迟创建该路径。
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
        .map(project_temp_maintenance::resolve_project_temp_dir)
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

/// Applies process options that are not part of the model-visible shell arguments.
/// 应用不属于模型可见 shell 参数的进程执行选项。
fn apply_shell_command_process_options(
    command: &mut TokioCommand,
    process_options: &ShellCommandProcessOptions,
) -> Result<(), NativeToolError> {
    // VULCAN_HOME gives bundled skill scripts the same profile-specific user root as the Rust host.
    // VULCAN_HOME 让内置技能脚本与 Rust 宿主使用相同的按构建配置区分的用户根目录。
    if std::env::var_os("VULCAN_HOME").is_none()
        && let Some(user_vulcan_root) = crate::runtime_assets::resolve_user_vulcan_root()
    {
        command.env(
            "VULCAN_HOME",
            crate::runtime_assets::native_host_path(&user_vulcan_root),
        );
    }

    for (name, value) in &process_options.environment {
        command.env(name, value);
    }

    apply_child_path_prefixes(command, &process_options.path_prefixes)
        .map_err(NativeToolError::new)?;
    Ok(())
}

/// Applies the generic shell-command transform hook chain and validates its exact output protocol.
/// 应用通用 shell 命令转换 Hook 链并校验其精确输出协议。
///
/// # Parameters
/// 参数
/// The original arguments and cwd are host-owned; plugin runtime, workspace, and session are context.
/// 原始参数与 cwd 由宿主持有；插件运行时、工作区与会话构成上下文。
///
/// # Returns
/// 返回值
/// Returns effective arguments, bounded process options, and an auditable transform report.
/// 返回生效参数、有界进程选项与可审计转换报告。
async fn apply_shell_command_plugin_hooks(
    parsed_args: &ShellCommandToolArgs,
    effective_cwd: &Path,
    workspace_root: Option<&Path>,
    session_id: Option<SessionId>,
    plugin_runtime: Option<&vulcan_plugin_runtime::VulcanPluginRuntimeManager>,
) -> Result<
    (
        ShellCommandToolArgs,
        ShellCommandProcessOptions,
        ShellCommandHookReport,
    ),
    NativeToolError,
> {
    // hostPathPrefixes gives packaged sg and rg precedence over plugin and inherited PATH entries.
    // hostPathPrefixes 让已打包 sg 与 rg 优先于插件及继承的 PATH 条目。
    let host_path_prefixes = vulcan_runtime_assets::resolve_bundled_tool_bin_directory()
        .into_iter()
        .collect::<Vec<_>>();
    let original_cwd = normalized_workspace_path_text_from_path(effective_cwd);
    let shell = parsed_args
        .shell
        .clone()
        .unwrap_or_else(|| String::from("auto"));
    let initial_payload = ShellCommandHookPayload {
        schema_version: 1,
        command: parsed_args.command.clone(),
        cwd: original_cwd.clone(),
        shell: shell.clone(),
        environment: BTreeMap::new(),
        binary_path_requests: Vec::new(),
        contributions: Vec::new(),
    };
    let Some(plugin_runtime) = plugin_runtime else {
        return Ok((
            parsed_args.clone(),
            ShellCommandProcessOptions {
                path_prefixes: host_path_prefixes,
                environment: BTreeMap::new(),
            },
            ShellCommandHookReport {
                original_command: parsed_args.command.clone(),
                effective_command: parsed_args.command.clone(),
                ..ShellCommandHookReport::default()
            },
        ));
    };
    let mut context = HookContext::new(
        Uuid::new_v4().to_string(),
        HOOK_TOOL_SHELL_COMMAND_BEFORE_EXECUTE,
    );
    context.scope = HookScope::Workspace;
    context.actor = HookActor::new("native-tool/shell_command");
    context.workspace_id = workspace_root.map(normalized_workspace_path_text_from_path);
    context.metadata.insert(
        String::from("workspace_root"),
        Value::String(original_cwd.clone()),
    );
    context.session_id = session_id.map(|value| value.to_string());
    context.conversation_id = context.session_id.clone();
    context.capabilities = HookCapabilitySet::default();
    let report = dispatch_vulcan_plugin_hook(
        plugin_runtime.clone(),
        context,
        serde_json::to_value(&initial_payload).map_err(|error| {
            NativeToolError::new(format!(
                "Failed to encode shell_command hook payload: {error}"
            ))
        })?,
    )
    .await
    .map_err(|error| {
        NativeToolError::new(format!("shell_command hook dispatch failed: {error}"))
    })?;
    let transformed: ShellCommandHookPayload = serde_json::from_value(report.payload.value)
        .map_err(|error| {
            NativeToolError::new(format!(
                "shell_command hook returned an invalid transform payload: {error}"
            ))
        })?;
    if transformed.schema_version != 1 {
        return Err(NativeToolError::new(format!(
            "shell_command hook returned unsupported schema_version `{}`.",
            transformed.schema_version
        )));
    }
    if transformed.command.trim().is_empty() {
        return Err(NativeToolError::new(
            "shell_command hook returned an empty command.",
        ));
    }
    if transformed.cwd != original_cwd || transformed.shell != shell {
        return Err(NativeToolError::new(
            "shell_command hook must preserve host-owned cwd and shell fields.",
        ));
    }
    validate_shell_command_hook_environment(&transformed.environment)?;
    let completed_hooks = report
        .completed_hooks
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    // pathPrefixes starts with host-owned tools before accepting validated plugin binary directories.
    // pathPrefixes 首先放入宿主自有工具，再接收已校验的插件二进制目录。
    let mut path_prefixes = host_path_prefixes;
    for request in &transformed.binary_path_requests {
        if !completed_hooks.contains(&request.registration_id) {
            return Err(NativeToolError::new(format!(
                "shell_command hook binary path request references incomplete registration `{}`.",
                request.registration_id
            )));
        }
        let directory = plugin_runtime
            .hook_binary_directory(&request.registration_id, &request.binary_id)
            .map_err(|error| NativeToolError::new(error.to_string()))?;
        if !path_prefixes.contains(&directory) {
            path_prefixes.push(directory);
        }
    }
    let diagnostics = report
        .diagnostics
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            NativeToolError::new(format!(
                "Failed to encode shell_command hook diagnostics: {error}"
            ))
        })?;
    let failures = report
        .failures
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            NativeToolError::new(format!(
                "Failed to encode shell_command hook failures: {error}"
            ))
        })?;
    let mut effective_args = parsed_args.clone();
    effective_args.command = transformed.command.clone();
    Ok((
        effective_args,
        ShellCommandProcessOptions {
            path_prefixes,
            environment: transformed.environment,
        },
        ShellCommandHookReport {
            original_command: parsed_args.command.clone(),
            effective_command: transformed.command,
            completed_hooks,
            contributions: transformed.contributions,
            diagnostics,
            failures,
        },
    ))
}

/// Validates plugin-contributed process environment without allowing PATH ownership to escape the host.
/// 校验插件贡献的进程环境，同时禁止 PATH 所有权逃离宿主。
///
/// # Parameters
/// 参数
/// `environment` contains the exact merged variables returned by the completed Hook chain.
/// `environment` 包含已完成 Hook 链返回的精确合并变量。
///
/// # Returns
/// 返回值
/// Returns success for portable non-PATH names or an explicit policy error.
/// 对可移植且非 PATH 的名称返回成功，否则返回明确策略错误。
fn validate_shell_command_hook_environment(
    environment: &BTreeMap<String, String>,
) -> Result<(), NativeToolError> {
    for name in environment.keys() {
        if !valid_process_environment_name(name) {
            return Err(NativeToolError::new(format!(
                "shell_command hook returned invalid environment variable name `{name}`."
            )));
        }
        if name.eq_ignore_ascii_case("PATH") {
            return Err(NativeToolError::new(
                "shell_command hooks may not override the host-owned PATH variable.",
            ));
        }
    }
    Ok(())
}

/// Returns whether one environment variable name is portable and path-safe for process launch.
/// 返回一个环境变量名是否可移植且可安全用于进程启动。
fn valid_process_environment_name(name: &str) -> bool {
    let mut characters = name.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

/// Executes one shell command through the selected launcher and returns a normalized execution report.
/// 通过选定启动器执行单条 shell 命令，并返回规范化执行报告。
pub(crate) async fn execute_shell_command_process(
    parsed_args: &ShellCommandToolArgs,
    effective_cwd: &Path,
    shell_support: &HostCommandShellSupport,
    process_options: &ShellCommandProcessOptions,
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

    let mut command = new_background_tokio_command(&program);
    command.args(&program_args);
    command.current_dir(effective_cwd);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command.kill_on_drop(true);
    if let Err(error) = apply_shell_command_process_options(&mut command, process_options) {
        return ShellCommandExecutionReport {
            status: ShellCommandExecutionStatus::Failed,
            shell_label,
            cwd: effective_cwd.to_path_buf(),
            exit_code: None,
            duration_ms: started_at.elapsed().as_millis() as u64,
            stdout_text: String::new(),
            stderr_text: String::new(),
            failure_reason: Some(error.to_string()),
        };
    }
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
    hook_report: &ShellCommandHookReport,
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
    if hook_report.original_command != hook_report.effective_command {
        lines.push(format!(
            "Plugin hooks: transformed by {} completed hook(s)",
            hook_report.completed_hooks.len()
        ));
        lines.push(format!(
            "Original command: {}",
            hook_report.original_command
        ));
    }
    for contribution in &hook_report.contributions {
        lines.push(format!("{}: {}", contribution.label, contribution.value));
    }
    if !hook_report.failures.is_empty() {
        lines.push(format!(
            "Plugin hook failures: {} non-fatal failure(s)",
            hook_report.failures.len()
        ));
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
    hook_report: &ShellCommandHookReport,
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
        "hook_transform": hook_report,
    })
}

/// Executes one `shell_command` invocation after applying the generic plugin transform chain.
/// 在应用通用插件转换链后执行一次 `shell_command` 调用。
pub(crate) async fn execute_shell_command_tool(
    request: NativeToolExecutionRequest,
    shell_support: &HostCommandShellSupport,
    plugin_runtime: Option<&vulcan_plugin_runtime::VulcanPluginRuntimeManager>,
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
    let session_id = request.session_id;
    let (effective_args, process_options, hook_report) = apply_shell_command_plugin_hooks(
        &parsed_args,
        &effective_cwd,
        request.workspace_root.as_deref(),
        session_id,
        plugin_runtime,
    )
    .await?;

    let execution_report = execute_shell_command_process(
        &effective_args,
        &effective_cwd,
        shell_support,
        &process_options,
    )
    .await;
    let render_input =
        build_shell_command_text_render_input(&effective_args, &execution_report, &hook_report);
    let budget = default_shell_command_budget_snapshot();
    let host_payload = build_shell_command_host_payload(
        session_id,
        &effective_args,
        &execution_report,
        shell_command_output_exceeds_inline_budget(&render_input, &budget),
        &hook_report,
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
    )
    .map_err(NativeToolError::new)?;

    Ok(
        VulcanToolInvocationResult::from_model_output(Value::String(model_output))
            .with_host_result(VulcanToolHostResult {
                kind: String::from("command_run"),
                payload: host_payload,
            }),
    )
}

#[cfg(test)]
mod process_option_tests {
    use super::*;

    /// Empty hook-owned collection fields must remain present in the serialized shell-command payload so Lua plugins can append contributions safely.
    /// 空的 Hook 所有集合字段必须保留在序列化 shell 命令载荷中，确保 Lua 插件能够安全追加贡献项。
    #[test]
    fn shell_command_hook_payload_preserves_empty_plugin_collections() {
        let payload = ShellCommandHookPayload {
            schema_version: 1,
            command: String::from("git diff"),
            cwd: String::from(r"D:\workspace"),
            shell: String::from("auto"),
            environment: BTreeMap::new(),
            binary_path_requests: Vec::new(),
            contributions: Vec::new(),
        };

        let serialized = serde_json::to_value(payload).expect("hook payload should serialize");
        assert_eq!(serialized["binary_path_requests"], json!([]));
        assert_eq!(serialized["contributions"], json!([]));
    }

    /// Plugin Hook environment contributions must not replace the host-owned binary search path.
    /// 插件 Hook 环境变量贡献不得替换宿主持有的二进制搜索路径。
    #[test]
    fn plugin_environment_rejects_case_insensitive_path_override() {
        // environment uses Windows-style casing to prove the reservation is platform-independent.
        // environment 使用 Windows 风格大小写，证明该保留规则不依赖平台。
        let environment = BTreeMap::from([(String::from("Path"), String::from("untrusted"))]);
        // error is the explicit policy diagnostic returned before any process is launched.
        // error 是启动任何进程前返回的明确策略诊断。
        let error = validate_shell_command_hook_environment(&environment)
            .expect_err("plugin PATH override must be rejected");
        assert!(error.message.contains("host-owned PATH"));
    }

    /// A host-resolved plugin binary directory should be prepended before cmd resolves a rewritten command.
    /// 宿主解析出的插件二进制目录应在 cmd 解析改写命令前被前置。
    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn plugin_binary_directory_is_injected_into_child_path() {
        // temp owns an isolated executable directory that is absent from the host process PATH.
        // temp 持有一个不在宿主进程 PATH 中的隔离可执行目录。
        let temp = tempfile::tempdir().expect("create plugin PATH test directory");
        // binaryPath is the command shim resolved only through the injected plugin directory.
        // binaryPath 是只能通过注入插件目录解析的命令垫片。
        let binary_path = temp.path().join("rtk.cmd");
        std::fs::write(&binary_path, "@echo plugin-path-ok\r\n")
            .expect("write plugin PATH command shim");
        // parsedArgs models the command text emitted by the RTK shell transform hook.
        // parsedArgs 模拟 RTK shell 转换 Hook 产生的命令文本。
        let parsed_args = ShellCommandToolArgs {
            command: String::from("rtk"),
            cwd: None,
            timeout_ms: Some(5_000),
            stdin: None,
            shell: Some(String::from("cmd")),
        };
        // processOptions models the canonical directory resolved from registration_id plus binary_id.
        // processOptions 模拟通过 registration_id 与 binary_id 解析出的规范目录。
        let process_options = ShellCommandProcessOptions {
            path_prefixes: vec![temp.path().to_path_buf()],
            environment: BTreeMap::new(),
        };
        // report captures the real child-shell lookup result using only the injected PATH prefix.
        // report 捕获仅使用注入 PATH 前缀时真实子 Shell 的查找结果。
        let report = execute_shell_command_process(
            &parsed_args,
            temp.path(),
            &HostCommandShellSupport::from_configured_default_id(None),
            &process_options,
        )
        .await;
        assert_eq!(report.status, ShellCommandExecutionStatus::Completed);
        assert!(report.stdout_text.contains("plugin-path-ok"));
    }
}
