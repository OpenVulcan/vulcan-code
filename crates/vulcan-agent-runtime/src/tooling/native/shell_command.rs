use super::*;
use crate::provider_config::{PersistedRtkRouteMode, PersistedRtkToolConfiguration};
use crate::runtime_assets;
use std::collections::BTreeSet;

/// Hard timeout for the lightweight RTK rewrite probe.
/// 轻量 RTK rewrite 探测使用的硬超时。
const RTK_REWRITE_TIMEOUT_MS: u64 = 1_500;

/// Builds the one-shot shell-command host-owned native tool.
/// 构建一次性 shell 命令宿主自有原生工具。
pub(crate) fn build_shell_command_tool(
    shell_support: HostCommandShellSupport,
    rtk_support: RtkRoutingSupport,
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
        shell_command_args_schema(&shell_support, &rtk_support),
        move |request: NativeToolExecutionRequest| {
            let shell_support = shell_support.clone();
            let rtk_support = rtk_support.clone();
            Box::pin(async move {
                execute_shell_command_tool(request, &shell_support, &rtk_support).await
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
    /// Optional per-call bypass for RTK routing when RTK support is exposed.
    /// RTK 支持已暴露时用于单次调用绕过 RTK 路由的可选开关。
    #[serde(default)]
    disabled_rtk: bool,
}

/// RTK routing support captured when the shell-command tool is registered.
/// 注册 shell-command 工具时捕获的 RTK 路由支持信息。
#[derive(Debug, Clone)]
pub(crate) struct RtkRoutingSupport {
    /// Persisted RTK route configuration.
    /// 持久化 RTK 路由配置。
    configuration: PersistedRtkToolConfiguration,
    /// Bundled RTK executable resolved from output root.
    /// 从 output root 解析得到的随包 RTK 可执行文件。
    executable_path: Option<PathBuf>,
}

impl RtkRoutingSupport {
    /// Builds RTK support from persisted host-tool configuration and output-root assets.
    /// 根据持久化宿主工具配置与 output-root 资产构建 RTK 支持信息。
    pub(crate) fn from_configuration(configuration: &PersistedRtkToolConfiguration) -> Self {
        let executable_path = if configuration.enabled
            && configuration.route_mode != PersistedRtkRouteMode::Disabled
        {
            runtime_assets::resolve_bundled_rtk_executable_path()
        } else {
            None
        };
        Self {
            configuration: configuration.clone(),
            executable_path,
        }
    }

    /// Builds disabled RTK support for tests and fallback construction.
    /// 为测试与回退构建禁用态 RTK 支持信息。
    #[cfg(test)]
    pub(crate) fn disabled() -> Self {
        Self {
            configuration: PersistedRtkToolConfiguration {
                enabled: false,
                ..PersistedRtkToolConfiguration::default()
            },
            executable_path: None,
        }
    }

    /// Builds RTK support from explicit test parts.
    /// 根据显式测试部件构建 RTK 支持信息。
    #[cfg(test)]
    pub(crate) fn from_test_parts(
        configuration: PersistedRtkToolConfiguration,
        executable_path: Option<PathBuf>,
    ) -> Self {
        Self {
            configuration,
            executable_path,
        }
    }

    /// Returns whether RTK routing can be used by shell_command.
    /// 返回 shell_command 当前是否可以使用 RTK 路由。
    pub(crate) fn is_available(&self) -> bool {
        self.configuration.enabled
            && self.configuration.route_mode != PersistedRtkRouteMode::Disabled
            && self
                .executable_path
                .as_ref()
                .is_some_and(|path| path.is_file())
    }

    /// Returns the directory containing the bundled RTK executable.
    /// 返回随包 RTK 可执行文件所在目录。
    fn executable_directory(&self) -> Option<PathBuf> {
        self.executable_path
            .as_ref()
            .and_then(|path| path.parent())
            .map(Path::to_path_buf)
    }

    /// Returns whether every RTK route in one rewritten command is allowed.
    /// 返回某个改写命令中的全部 RTK 路由是否都被允许。
    pub(crate) fn allows_routes(&self, routes: &BTreeSet<String>) -> bool {
        if routes.is_empty() {
            return false;
        }

        routes.iter().all(|route| {
            if self.configuration.disabled_routes.contains(route) {
                return false;
            }
            match self.configuration.route_mode {
                PersistedRtkRouteMode::Disabled => false,
                PersistedRtkRouteMode::Selected => {
                    self.configuration.enabled_routes.contains(route)
                }
                PersistedRtkRouteMode::All => true,
            }
        })
    }
}

/// One RTK routing decision captured for command execution and host audit payloads.
/// 为命令执行与宿主审计载荷捕获的单条 RTK 路由决策。
#[derive(Debug, Clone)]
pub(crate) struct RtkRoutingDecision {
    /// Whether the original command was replaced by an RTK rewrite result.
    /// 原始命令是否已被 RTK rewrite 结果替换。
    applied: bool,
    /// Original command provided by the model.
    /// 模型提供的原始命令。
    original_command: String,
    /// Rewritten command returned by RTK when available.
    /// RTK 返回的可用改写命令。
    rewritten_command: Option<String>,
    /// RTK route names detected in the rewritten command.
    /// 在改写命令中检测到的 RTK 路由名称集合。
    routes: Vec<String>,
    /// Bypass or failure reason when RTK was not applied.
    /// 未应用 RTK 时的绕过或失败原因。
    bypass_reason: Option<String>,
}

impl RtkRoutingDecision {
    /// Builds an applied RTK routing decision.
    /// 构建已应用的 RTK 路由决策。
    fn applied(
        original_command: &str,
        rewritten_command: String,
        routes: BTreeSet<String>,
    ) -> Self {
        Self {
            applied: true,
            original_command: original_command.to_string(),
            rewritten_command: Some(rewritten_command),
            routes: routes.into_iter().collect(),
            bypass_reason: None,
        }
    }

    /// Builds a bypassed RTK routing decision.
    /// 构建已绕过的 RTK 路由决策。
    fn bypassed(
        original_command: &str,
        rewritten_command: Option<String>,
        routes: BTreeSet<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            applied: false,
            original_command: original_command.to_string(),
            rewritten_command,
            routes: routes.into_iter().collect(),
            bypass_reason: Some(reason.into()),
        }
    }
}

/// Process execution options derived from RTK routing and host policy.
/// 根据 RTK 路由与宿主策略派生的进程执行选项。
#[derive(Debug, Clone, Default)]
pub(crate) struct ShellCommandProcessOptions {
    /// Directory prepended to PATH for this child process.
    /// 为当前子进程前置到 PATH 的目录。
    path_prefix: Option<PathBuf>,
    /// Whether RTK telemetry should be disabled for this child process.
    /// 当前子进程是否应禁用 RTK 遥测。
    rtk_telemetry_disabled: bool,
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
pub(crate) fn shell_command_args_schema(
    shell_support: &HostCommandShellSupport,
    rtk_support: &RtkRoutingSupport,
) -> Value {
    let mut schema = json!({
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
    });

    if rtk_support.is_available() {
        if let Some(properties) = schema
            .get_mut("properties")
            .and_then(serde_json::Value::as_object_mut)
        {
            properties.insert(
                String::from("disabled_rtk"),
                json!({
                    "type": "boolean",
                    "description": "Optional per-call RTK bypass. Set true to run the raw command exactly as provided without RTK rewrite routing. Default: false."
                }),
            );
        }
    }

    schema
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

/// Applies process options that are not part of the model-visible shell arguments.
/// 应用不属于模型可见 shell 参数的进程执行选项。
fn apply_shell_command_process_options(
    command: &mut TokioCommand,
    process_options: &ShellCommandProcessOptions,
) {
    if let Some(path_prefix) = process_options.path_prefix.as_deref() {
        let mut paths = vec![path_prefix.to_path_buf()];
        if let Some(existing_path) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&existing_path));
        }
        if let Ok(joined_path) = std::env::join_paths(paths) {
            command.env("PATH", joined_path);
        }
    }

    if process_options.rtk_telemetry_disabled {
        command.env("RTK_TELEMETRY_DISABLED", "1");
    }
}

/// Resolves whether RTK should rewrite one shell command before execution.
/// 解析 RTK 是否应在执行前改写某条 shell 命令。
async fn resolve_rtk_routing_decision(
    parsed_args: &ShellCommandToolArgs,
    effective_cwd: &Path,
    rtk_support: &RtkRoutingSupport,
) -> Option<RtkRoutingDecision> {
    if parsed_args.disabled_rtk {
        return Some(RtkRoutingDecision::bypassed(
            &parsed_args.command,
            None,
            BTreeSet::new(),
            "disabled_by_invocation",
        ));
    }

    if !rtk_support.is_available() {
        return None;
    }

    let rewritten_command = match rewrite_shell_command_with_rtk(
        &parsed_args.command,
        effective_cwd,
        rtk_support,
    )
    .await
    {
        Ok(command) => command,
        Err(reason) => {
            return Some(RtkRoutingDecision::bypassed(
                &parsed_args.command,
                None,
                BTreeSet::new(),
                reason,
            ));
        }
    };
    let routes = collect_rtk_route_names(&rewritten_command);
    if !rtk_support.allows_routes(&routes) {
        return Some(RtkRoutingDecision::bypassed(
            &parsed_args.command,
            Some(rewritten_command),
            routes,
            "route_not_allowed",
        ));
    }

    Some(RtkRoutingDecision::applied(
        &parsed_args.command,
        rewritten_command,
        routes,
    ))
}

/// Invokes `rtk rewrite` and returns the rewritten command text from stdout.
/// 调用 `rtk rewrite` 并从 stdout 返回改写后的命令文本。
async fn rewrite_shell_command_with_rtk(
    command_text: &str,
    effective_cwd: &Path,
    rtk_support: &RtkRoutingSupport,
) -> Result<String, String> {
    let Some(executable_path) = rtk_support.executable_path.as_deref() else {
        return Err(String::from("rtk_executable_missing"));
    };

    let mut command = TokioCommand::new(executable_path);
    command.arg("rewrite");
    command.arg(command_text);
    command.current_dir(effective_cwd);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command.kill_on_drop(true);
    if rtk_support.configuration.telemetry_disabled {
        command.env("RTK_TELEMETRY_DISABLED", "1");
    }

    let output = match timeout(
        Duration::from_millis(RTK_REWRITE_TIMEOUT_MS),
        command.output(),
    )
    .await
    {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => return Err(format!("rtk_rewrite_spawn_failed: {error}")),
        Err(_) => return Err(String::from("rtk_rewrite_timed_out")),
    };

    let stdout_text = decode_shell_output_bytes(&output.stdout);
    if let Some(rewritten_command) = stdout_text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
    {
        return Ok(rewritten_command.to_string());
    }

    let stderr_text = decode_shell_output_bytes(&output.stderr);
    let status = output
        .status
        .code()
        .map_or_else(|| String::from("signal"), |code| code.to_string());
    let stderr_suffix = stderr_text.trim();
    if stderr_suffix.is_empty() {
        Err(format!("rtk_rewrite_no_match_or_empty_output:{status}"))
    } else {
        Err(format!(
            "rtk_rewrite_no_match_or_empty_output:{status}:{stderr_suffix}"
        ))
    }
}

/// Collects RTK route names from a rewritten command.
/// 从改写后的命令中收集 RTK 路由名称。
pub(crate) fn collect_rtk_route_names(rewritten_command: &str) -> BTreeSet<String> {
    let mut routes = BTreeSet::new();
    let mut previous_token_was_rtk = false;

    for raw_token in tokenize_rewritten_shell_command(rewritten_command) {
        let token = raw_token.trim_matches(|value| value == '"' || value == '\'');
        if previous_token_was_rtk {
            if !token.is_empty() && !shell_operator_token(token) && !token.starts_with('-') {
                routes.insert(token.to_ascii_lowercase());
            }
            previous_token_was_rtk = false;
        }
        if rtk_executable_token(token) {
            previous_token_was_rtk = true;
        }
    }

    routes
}

/// Tokenizes the RTK rewrite output enough to inspect shell segment boundaries.
/// 对 RTK rewrite 输出做足够用于检查 shell 片段边界的分词。
fn tokenize_rewritten_shell_command(rewritten_command: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = rewritten_command.chars().peekable();
    while let Some(value) = chars.next() {
        match value {
            value if value.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            '&' => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
                if chars.peek().is_some_and(|next| *next == '&') {
                    let _ = chars.next();
                    tokens.push(String::from("&&"));
                } else {
                    tokens.push(String::from("&"));
                }
            }
            '|' => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
                if chars.peek().is_some_and(|next| *next == '|') {
                    let _ = chars.next();
                    tokens.push(String::from("||"));
                } else {
                    tokens.push(String::from("|"));
                }
            }
            ';' => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
                tokens.push(String::from(";"));
            }
            _ => current.push(value),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Returns whether a token names the RTK executable.
/// 返回某个 token 是否指向 RTK 可执行文件。
fn rtk_executable_token(token: &str) -> bool {
    let normalized = token.replace('\\', "/");
    let file_name = normalized.rsplit('/').next().unwrap_or(token);
    file_name == "rtk" || file_name.eq_ignore_ascii_case("rtk.exe")
}

/// Returns whether a token is a shell composition operator.
/// 返回某个 token 是否为 shell 组合操作符。
fn shell_operator_token(token: &str) -> bool {
    matches!(token, "&&" | "||" | "|" | ";" | "&")
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

    let mut command = TokioCommand::new(&program);
    command.args(&program_args);
    command.current_dir(effective_cwd);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command.kill_on_drop(true);
    apply_shell_command_process_options(&mut command, process_options);
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
    rtk_decision: Option<&RtkRoutingDecision>,
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
    if let Some(decision) = rtk_decision {
        if decision.applied {
            lines.push(format!("RTK: applied ({})", decision.routes.join(", ")));
            lines.push(format!("Original command: {}", decision.original_command));
        } else if let Some(reason) = decision.bypass_reason.as_deref() {
            lines.push(format!("RTK: bypassed ({reason})"));
        }
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
    rtk_decision: Option<&RtkRoutingDecision>,
) -> Value {
    let mut payload = json!({
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
    });

    if let Some(decision) = rtk_decision {
        payload["rtk"] = json!({
            "applied": decision.applied,
            "original_command": decision.original_command,
            "rewritten_command": decision.rewritten_command,
            "routes": decision.routes,
            "bypass_reason": decision.bypass_reason,
        });
    }

    payload
}

pub(crate) async fn execute_shell_command_tool(
    request: NativeToolExecutionRequest,
    shell_support: &HostCommandShellSupport,
    rtk_support: &RtkRoutingSupport,
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
    let rtk_decision =
        resolve_rtk_routing_decision(&parsed_args, &effective_cwd, rtk_support).await;
    let mut effective_args = parsed_args.clone();
    let mut process_options = ShellCommandProcessOptions::default();
    if let Some(decision) = rtk_decision.as_ref().filter(|decision| decision.applied) {
        if let Some(rewritten_command) = decision.rewritten_command.as_ref() {
            effective_args.command = rewritten_command.clone();
        }
        process_options.path_prefix = rtk_support.executable_directory();
        process_options.rtk_telemetry_disabled = rtk_support.configuration.telemetry_disabled;
    }

    let execution_report = execute_shell_command_process(
        &effective_args,
        &effective_cwd,
        shell_support,
        &process_options,
    )
    .await;
    let render_input = build_shell_command_text_render_input(
        &effective_args,
        &execution_report,
        rtk_decision.as_ref(),
    );
    let budget = default_shell_command_budget_snapshot();
    let host_payload = build_shell_command_host_payload(
        request.session_id,
        &effective_args,
        &execution_report,
        shell_command_output_exceeds_inline_budget(&render_input, &budget),
        rtk_decision.as_ref(),
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
