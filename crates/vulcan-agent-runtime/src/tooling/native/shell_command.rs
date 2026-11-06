use super::shell_command_live::canonical_shell_job_session_id;
use super::*;
use crate::agent_runtime::conversation_hooks::HOOK_TOOL_SHELL_COMMAND_BEFORE_EXECUTE;
use crate::agent_runtime::vulcan_plugin_hooks::dispatch_vulcan_plugin_hook;
use crate::provider_config::PersistedRtkConfiguration;
use std::collections::BTreeMap;
use vulcan_hooks::{HookActor, HookCapabilitySet, HookContext, HookDiagnosticSeverity, HookScope};
use vulcan_host_tools::process_creation::{
    apply_child_path_prefixes, new_background_tokio_command,
};
use vulcan_host_tools::rtk::{
    RTK_PLAIN_OUTPUT_VARIABLE, RtkRewriteOutcome, resolve_rtk_executable, rtk_command_routes,
    rtk_host_environment, rtk_invocation_environment, run_rtk_rewrite,
};
use vulcan_host_tools::shell_job::{
    MAX_RUNNING_BACKGROUND_SHELL_JOBS, SHELL_OUTPUT_LINE_WRAP_BYTES,
    SHELL_OUTPUT_TAIL_BUDGET_BYTES, ShellJobCapturedStreams, ShellJobFinal, ShellJobForeground,
    ShellJobOrigin, ShellJobRuntime, ShellJobSpawn, ShellJobTail, ShellJobWaitOutcome,
    render_plain_rows,
};

/// Stable identifier of the one-shot shell-command native tool.
/// 一次性 shell 命令原生工具的稳定标识。
pub const SHELL_COMMAND_TOOL_ID: &str = "shell_command";

/// Builds the one-shot shell-command host-owned native tool.
/// 构建一次性 shell 命令宿主自有原生工具。
///
/// # Parameters
/// 参数
/// `rtk` is the RTK router configuration captured when the tool catalog is built.
/// `rtk` 是构建工具目录时捕获的 RTK 路由配置。
pub(crate) fn build_shell_command_tool(
    shell_support: HostCommandShellSupport,
    plugin_runtime: Option<vulcan_plugin_runtime::VulcanPluginRuntimeManager>,
    rtk: PersistedRtkConfiguration,
) -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from(SHELL_COMMAND_TOOL_ID),
        name: String::from(SHELL_COMMAND_TOOL_ID),
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
            let rtk = rtk.clone();
            Box::pin(async move {
                execute_shell_command_tool(request, &shell_support, plugin_runtime.as_ref(), &rtk)
                    .await
            })
        },
    )
}

/// One typed argument payload accepted by the host-owned `shell_command` native tool.
/// 宿主自有 `shell_command` 原生工具接受的一份强类型参数载荷。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
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
    /// Optional stdin text written once to the child process, which is then closed.
    /// 一次性写入子进程并随后关闭的可选标准输入文本。
    stdin: Option<String>,
    /// Optional shell family selector such as `auto`, `powershell`, `cmd`, or `bash`.
    /// 可选 shell 类型选择器，例如 `auto`、`powershell`、`cmd` 或 `bash`。
    shell: Option<String>,
    /// What the command is for; shown on the card, read by the permission gate and listed with jobs.
    /// 命令用途；显示在卡片上，由权限闸门读取，并随任务列出。
    purpose: String,
    /// Starts the command as a background job and returns at once.
    /// 以后台任务启动命令并立即返回。
    #[serde(default)]
    background: bool,
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
    /// Host-owned variables such as the RTK warning switch, applied first so session and hook
    /// values win.
    /// 宿主持有的变量（如 RTK 提示开关），最先应用，使会话与 Hook 值优先。
    host_environment: Vec<(String, String)>,
    /// Vulcan environment variables of the session, applied before hook contributions so an
    /// explicit hook value wins.
    /// 会话的 Vulcan 环境变量，先于 Hook 贡献应用，使显式 Hook 值优先。
    vulcan_environment: Vec<(String, String)>,
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

/// Command state after the host-native RTK rewrite, which plugin hooks receive as their input.
/// 宿主原生 RTK 改写后的命令状态，作为插件 Hook 的输入。
#[derive(Debug, Clone)]
struct NativeShellRewrite {
    /// Command after the RTK rewrite, or the model's command when none applied.
    /// RTK 改写后的命令；未改写时为模型命令。
    command: String,
    /// Variables the routed command needs.
    /// 经路由的命令所需的变量。
    environment: BTreeMap<String, String>,
    /// Display contribution describing the applied routes.
    /// 描述已应用路由的展示贡献。
    contributions: Vec<ShellCommandHookContribution>,
    /// Serialized `HookDiagnostic`-shaped notes explaining a skipped rewrite.
    /// 解释跳过改写原因的 `HookDiagnostic` 形状序列化说明。
    diagnostics: Vec<Value>,
}

impl NativeShellRewrite {
    /// Returns the unchanged command state for one model command.
    /// 返回一条模型命令的未改写状态。
    fn unchanged(command: &str) -> Self {
        Self {
            command: command.to_string(),
            environment: BTreeMap::new(),
            contributions: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    /// Returns the unchanged command state with one diagnostic.
    /// 返回带一条诊断的未改写状态。
    fn skipped(command: &str, severity: HookDiagnosticSeverity, message: String) -> Self {
        let mut rewrite = Self::unchanged(command);
        rewrite
            .diagnostics
            .push(json!({ "severity": severity, "message": message }));
        rewrite
    }
}

/// Routes one model command through the bundled RTK executable when the settings allow it.
/// 设置允许时，经随包 RTK 可执行文件路由一条模型命令。
///
/// # Parameters
/// 参数
/// `command` is the model's command, `cwd` its working directory, and `rtk` the router settings.
/// `command` 是模型命令，`cwd` 是其工作目录，`rtk` 是路由设置。
///
/// # Returns
/// 返回值
/// The rewritten command only when RTK produced one and every route is allowed; otherwise the
/// original command, never partial output.
/// 仅当 RTK 产生改写且全部路由均被允许时返回改写命令；否则返回原命令，绝不返回部分输出。
async fn apply_native_rtk_rewrite(
    command: &str,
    cwd: &Path,
    rtk: &PersistedRtkConfiguration,
) -> NativeShellRewrite {
    if !rtk.routing_enabled() {
        return NativeShellRewrite::unchanged(command);
    }
    let Some(executable) = resolve_rtk_executable() else {
        return NativeShellRewrite::skipped(
            command,
            HookDiagnosticSeverity::Info,
            String::from("RTK is not bundled for this host; command routing was skipped."),
        );
    };
    let environment = rtk_invocation_environment(rtk.telemetry_disabled);
    let rewritten = match run_rtk_rewrite(&executable, command, cwd, &environment).await {
        RtkRewriteOutcome::Rewritten(rewritten) => rewritten,
        RtkRewriteOutcome::NoRewrite => return NativeShellRewrite::unchanged(command),
        RtkRewriteOutcome::Denied => {
            return NativeShellRewrite::skipped(
                command,
                HookDiagnosticSeverity::Info,
                String::from("An RTK deny rule matched; the original command runs unchanged."),
            );
        }
        RtkRewriteOutcome::Failed(reason) => {
            return NativeShellRewrite::skipped(
                command,
                HookDiagnosticSeverity::Warning,
                format!("RTK rewrite was not applied: {reason}"),
            );
        }
    };
    if rewritten == command {
        return NativeShellRewrite::unchanged(command);
    }
    let routes = rtk_command_routes(&rewritten);
    if !rtk.allows_routes(&routes) {
        return NativeShellRewrite::skipped(
            command,
            HookDiagnosticSeverity::Info,
            format!(
                "RTK routes are not enabled in the RTK settings: {}",
                routes.join(", ")
            ),
        );
    }
    NativeShellRewrite {
        command: rewritten,
        environment: BTreeMap::from([(String::from(RTK_PLAIN_OUTPUT_VARIABLE), String::from("1"))]),
        contributions: vec![ShellCommandHookContribution {
            badge: Some(String::from("RTK")),
            label: String::from("Command router"),
            value: format!("rtk ({})", routes.join(", ")),
        }],
        diagnostics: Vec::new(),
    }
}

/// Status label of a command that moved to the background and keeps running.
/// 已转到后台并继续运行的命令的状态标签。
pub(crate) const SHELL_COMMAND_BACKGROUND_STATUS: &str = "running_in_background";

/// Status label of a command that could not be started.
/// 无法启动的命令的状态标签。
const SHELL_COMMAND_NOT_STARTED_STATUS: &str = "failed";

/// Maximum inline byte budget used before shell tool output switches into page mode spill rendering.
/// shell 工具输出在切换到分页落盘渲染前使用的最大内联字节预算。
pub(crate) const SHELL_INLINE_OUTPUT_LIMIT_BYTES: u64 =
    crate::tool_result_format::DEFAULT_INLINE_TOOL_RESULT_BYTES;

/// How one `shell_command` call ended for its caller.
/// 一次 `shell_command` 调用对调用方而言的结束方式。
#[derive(Debug, Clone)]
pub(crate) enum ShellCommandCallEnd {
    /// The job finished while the call waited.
    /// 调用等待期间任务已结束。
    Finished(ShellJobFinal),
    /// The job keeps running in the background; `reason` says why the call returned first.
    /// 任务在后台继续运行；`reason` 说明调用为何先返回。
    Background {
        /// Why the call returned while the job runs.
        /// 任务运行期间调用返回的原因。
        reason: String,
    },
    /// The command never started.
    /// 命令从未启动。
    NotStarted {
        /// Why it could not start.
        /// 无法启动的原因。
        reason: String,
    },
}

/// One normalized shell-command execution report used by both model text and host metadata.
/// 同时供模型文本与宿主元数据复用的一份规范化 shell 命令执行报告。
#[derive(Debug, Clone)]
pub(crate) struct ShellCommandExecutionReport {
    /// How the call ended.
    /// 调用的结束方式。
    end: ShellCommandCallEnd,
    /// Job identity, once the command started.
    /// 命令启动后的任务身份。
    job_id: Option<String>,
    /// Stable shell label chosen by the host.
    /// 由宿主选定的稳定 shell 标签。
    shell_label: String,
    /// Effective working directory used for the command run.
    /// 本次命令执行使用的实际工作目录。
    cwd: PathBuf,
    /// Run time in milliseconds: total for a finished job, so far for a background one.
    /// 运行时间（毫秒）：已结束任务为总时长，后台任务为迄今时长。
    duration_ms: u64,
    /// Bounded end of the output, when the log could be read.
    /// 输出的有界末尾（日志可读时）。
    tail: Option<ShellJobTail>,
    /// Raw stdout and stderr kept for tool-chain steps.
    /// 为工具链步骤保留的原始 stdout 与 stderr。
    streams: ShellJobCapturedStreams,
}

impl ShellCommandExecutionReport {
    /// Stable status label of the call.
    /// 调用的稳定状态标签。
    fn status_label(&self) -> &'static str {
        match &self.end {
            ShellCommandCallEnd::Finished(outcome) => outcome.status.as_str(),
            ShellCommandCallEnd::Background { .. } => SHELL_COMMAND_BACKGROUND_STATUS,
            ShellCommandCallEnd::NotStarted { .. } => SHELL_COMMAND_NOT_STARTED_STATUS,
        }
    }

    /// Process exit code, when the process exited by itself.
    /// 进程自行退出时的退出码。
    fn exit_code(&self) -> Option<i32> {
        match &self.end {
            ShellCommandCallEnd::Finished(outcome) => outcome.exit_code,
            _ => None,
        }
    }

    /// Host note on failures, stops, timeouts and early returns.
    /// 宿主关于失败、停止、超时与提前返回的说明。
    fn failure_reason(&self) -> Option<&str> {
        match &self.end {
            ShellCommandCallEnd::Finished(outcome) => outcome.failure_reason.as_deref(),
            ShellCommandCallEnd::Background { reason }
            | ShellCommandCallEnd::NotStarted { reason } => Some(reason),
        }
    }

    /// True while the job keeps running in the background.
    /// 任务在后台继续运行时为 true。
    fn is_background(&self) -> bool {
        matches!(self.end, ShellCommandCallEnd::Background { .. })
    }
}

/// Builds the English AI-facing description for the one-shot shell command tool.
/// 构建一次性 shell 命令工具面向 AI 的英文描述。
pub(crate) fn shell_command_tool_description() -> String {
    format!(
        "Run a one-shot command in the host environment. Every call runs as a shell job: stdout and stderr are recorded together, in arrival order, in a rolling log, and the result gives the exit status plus the end of the output, the newest whole lines within {SHELL_OUTPUT_TAIL_BUDGET_BYTES} bytes; a line longer than {SHELL_OUTPUT_LINE_WRAP_BYTES} bytes is split into several rows. Shorter output returns in full. When earlier output was left out, read it by line range or search it with `shell_job`. Default `timeout_ms`: unlimited; if omitted the host waits until the process exits. Set `background` to true for commands that run long or never exit (servers, watchers, long builds): the call returns the `job_id` at once and the command keeps running; a session runs at most {MAX_RUNNING_BACKGROUND_SHELL_JOBS} background jobs at a time. If the user sends a message while a command runs in the foreground, the host moves it to the background so you can answer, and it keeps running as a job. You are told when a background job finishes; meanwhile use `shell_job` to list, wait for, read, search or stop jobs. A result that still exceeds {SHELL_INLINE_OUTPUT_LIMIT_BYTES} bytes, such as one with a very long command text, is paged into an exclusively created `.vulcan/temp/tool_output_<random_id>.md` file whose exact path is reported."
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
                "description": "Raw command text executed by the selected shell."
            },
            "purpose": {
                "type": "string",
                "minLength": 1,
                "maxLength": MAX_PERMISSION_REASON_CHARS,
                "description": SHELL_COMMAND_PURPOSE_DESCRIPTION
            },
            "cwd": {
                "type": "string",
                "description": "Optional working directory used only for this one-shot command invocation. When omitted, the host uses the current workspace root when available and otherwise falls back to the host process working directory. Relative paths resolve against the same base directory."
            },
            "timeout_ms": {
                "type": "integer",
                "minimum": 1,
                "description": "Optional hard timeout in milliseconds; must be at least 1 when provided. Default: unlimited. When omitted, no timeout is applied and the host waits until the process exits. With `background` true it bounds the whole job; a foreground command moved to the background keeps running without it."
            },
            "background": {
                "type": "boolean",
                "default": false,
                "description": "Optional, default false. True starts the command as a background job and returns its `job_id` immediately; follow it with `shell_job`."
            },
            "stdin": {
                "type": "string",
                "description": "Optional stdin text written once to the process as UTF-8; stdin is closed afterwards. When omitted, stdin is empty. No newline is added automatically. In PowerShell read it with `[Console]::In.ReadToEnd()`, not `$input`, which garbles non-ASCII text on Windows."
            },
            "shell": {
                "type": "string",
                "enum": shell_support.schema_shell_ids(),
                "default": shell_support.auto_shell().map(|entry| entry.shell.id()),
                "description": shell_support.schema_shell_description()
            }
        },
        "required": ["command", "purpose"],
        "additionalProperties": false
    })
}

/// Description of the required `purpose` argument of `shell_command`.
/// `shell_command` 必填参数 `purpose` 的说明。
const SHELL_COMMAND_PURPOSE_DESCRIPTION: &str = "Required. One short sentence saying what this command is for, such as \"Run the parser unit tests\". It is shown to the user on the command card, given to the permission reviewer, and listed with background jobs so you can tell them apart. It is never passed to the command.";

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
    if parsed.purpose.trim().is_empty() {
        return Err(NativeToolError::new(
            "The `purpose` argument for shell_command cannot be empty; say in one short sentence what the command is for.",
        ));
    }
    // Only omission means unlimited; an explicit zero is rejected instead of silently becoming unlimited.
    // 只有省略表示无限等待；显式传入 0 直接拒绝，而不是静默当作无限。
    if parsed.timeout_ms == Some(0) {
        return Err(NativeToolError::new(
            "The `timeout_ms` argument for shell_command must be at least 1; omit it to wait without a timeout.",
        ));
    }
    Ok(parsed)
}

pub(crate) fn resolve_shell_command_working_directory(
    workspace_root: Option<&Path>,
    raw_cwd: Option<&str>,
) -> Result<PathBuf, NativeToolError> {
    let workspace_root = workspace_root.filter(|path| !path.as_os_str().is_empty());
    let requested_path = raw_cwd
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    if let Some(requested_path) = requested_path.as_ref()
        && requested_path.is_absolute()
    {
        return Ok(crate::runtime_assets::native_host_path(requested_path));
    }
    // Projectless executions intentionally fall back to the host process directory; it is read only when needed
    // so an unreadable process directory cannot break workspace-bound or absolute-cwd commands.
    // 无项目执行按设计回退到宿主进程目录；仅在需要时读取，避免进程目录不可读时连带破坏
    // 绑定工作区或使用绝对 cwd 的命令。
    let base_directory = match workspace_root {
        Some(workspace_root) => crate::runtime_assets::native_host_path(workspace_root),
        None => {
            crate::runtime_assets::native_host_path(&std::env::current_dir().map_err(|error| {
                NativeToolError::new(format!(
                    "Failed to read the current working directory: {error}"
                ))
            })?)
        }
    };
    Ok(match requested_path {
        Some(requested_path) => {
            crate::runtime_assets::native_host_path(&base_directory.join(requested_path))
        }
        None => base_directory,
    })
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
///
/// `utf8_stdin` marks a command whose stdin the host fills with UTF-8 text.
/// `utf8_stdin` 标记宿主会以 UTF-8 文本填充其 stdin 的命令。
pub(crate) fn resolve_shell_command_launcher(
    shell_support: &HostCommandShellSupport,
    shell_name: Option<&str>,
    command_text: &str,
    utf8_stdin: bool,
) -> Result<(String, Vec<String>, String), NativeToolError> {
    let resolved_shell = shell_support
        .resolve_requested_shell(shell_name)
        .map_err(NativeToolError::new)?;
    if utf8_stdin {
        let command_text = resolved_shell
            .shell
            .command_text_for_utf8_stdin(command_text);
        return Ok(resolved_shell.build_launcher(&command_text));
    }
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

    for (name, value) in &process_options.host_environment {
        command.env(name, value);
    }
    for (name, value) in &process_options.vulcan_environment {
        command.env(name, value);
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
/// The original arguments and cwd are host-owned; `native` is the RTK-routed command state the
/// hooks start from; plugin runtime, workspace, and session are context.
/// 原始参数与 cwd 由宿主持有；`native` 是 Hook 起始的 RTK 路由后命令状态；插件运行时、工作区与会话构成上下文。
///
/// # Returns
/// 返回值
/// Returns effective arguments, bounded process options, and an auditable transform report.
/// 返回生效参数、有界进程选项与可审计转换报告。
async fn apply_shell_command_plugin_hooks(
    parsed_args: &ShellCommandToolArgs,
    effective_cwd: &Path,
    native: NativeShellRewrite,
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
        command: native.command.clone(),
        cwd: original_cwd.clone(),
        shell: shell.clone(),
        environment: native.environment.clone(),
        binary_path_requests: Vec::new(),
        contributions: native.contributions.clone(),
    };
    let Some(plugin_runtime) = plugin_runtime else {
        let mut effective_args = parsed_args.clone();
        effective_args.command = native.command.clone();
        return Ok((
            effective_args,
            ShellCommandProcessOptions {
                path_prefixes: host_path_prefixes,
                environment: native.environment,
                host_environment: Vec::new(),
                vulcan_environment: Vec::new(),
            },
            ShellCommandHookReport {
                original_command: parsed_args.command.clone(),
                effective_command: native.command,
                completed_hooks: Vec::new(),
                contributions: native.contributions,
                diagnostics: native.diagnostics,
                failures: Vec::new(),
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
    let mut diagnostics = native.diagnostics;
    for diagnostic in &report.diagnostics {
        diagnostics.push(serde_json::to_value(diagnostic).map_err(|error| {
            NativeToolError::new(format!(
                "Failed to encode shell_command hook diagnostics: {error}"
            ))
        })?);
    }
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
            host_environment: Vec::new(),
            vulcan_environment: Vec::new(),
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

/// Session and tool call a `shell_command` job belongs to.
/// `shell_command` 任务所属的会话与工具调用。
#[derive(Debug, Clone)]
pub(crate) struct ShellCommandJobContext {
    /// Owning session in canonical text form.
    /// 规范文本形式的所属会话。
    owner_session_id: String,
    /// Tool call that starts the job, when a model call does.
    /// 启动任务的工具调用（由模型调用启动时）。
    origin: Option<ShellJobOrigin>,
}

impl ShellCommandJobContext {
    /// Owning session in canonical text form.
    /// 规范文本形式的所属会话。
    pub(crate) fn owner_session_id(&self) -> &str {
        &self.owner_session_id
    }

    /// Context of one native tool request; direct calls without a session share one fallback owner.
    /// 单个原生工具请求的上下文；没有会话的直接调用共用一个回退所有者。
    pub(crate) fn of_request(request: &NativeToolExecutionRequest) -> Self {
        let owner_session_id = request
            .invocation_context
            .as_ref()
            .map(|context| canonical_shell_job_session_id(&context.session_id))
            .or_else(|| request.session_id.map(|session_id| session_id.to_string()))
            .unwrap_or_else(|| String::from("direct"));
        let origin = request
            .invocation_context
            .as_ref()
            .map(|context| ShellJobOrigin {
                api_call_id: context.api_call_id.value(),
                tool_call_id: context.tool_call_id.value(),
            });
        Self {
            owner_session_id,
            origin,
        }
    }
}

/// Executes one shell command as a job and returns how the call ended.
/// 以任务方式执行单条 shell 命令，并返回调用的结束方式。
///
/// A foreground call waits until the job finishes, the user interjects a message, or the job is moved to the
/// background from elsewhere; dropping the future while it waits stops the job, as a cancelled call did before.
/// 前台调用等待任务结束、用户插队发送消息，或任务被其他地方转到后台；等待期间丢弃 future 会停止任务，
/// 与以往取消调用的行为一致。
pub(crate) async fn execute_shell_command_process(
    parsed_args: &ShellCommandToolArgs,
    effective_cwd: &Path,
    shell_support: &HostCommandShellSupport,
    process_options: &ShellCommandProcessOptions,
    job_context: &ShellCommandJobContext,
) -> ShellCommandExecutionReport {
    let not_started = |shell_label: String, reason: String| ShellCommandExecutionReport {
        end: ShellCommandCallEnd::NotStarted { reason },
        job_id: None,
        shell_label,
        cwd: effective_cwd.to_path_buf(),
        duration_ms: 0,
        tail: None,
        streams: ShellJobCapturedStreams::default(),
    };
    let (program, program_args, shell_label) = match resolve_shell_command_launcher(
        shell_support,
        parsed_args.shell.as_deref(),
        &parsed_args.command,
        parsed_args.stdin.is_some(),
    ) {
        Ok(resolved) => resolved,
        Err(error) => {
            return not_started(
                parsed_args
                    .shell
                    .clone()
                    .unwrap_or_else(|| String::from("auto")),
                error.to_string(),
            );
        }
    };

    let mut command = new_background_tokio_command(&program);
    command.args(&program_args);
    command.current_dir(effective_cwd);
    if let Err(error) = apply_shell_command_process_options(&mut command, process_options) {
        return not_started(shell_label, error.to_string());
    }
    let runtime = ShellJobRuntime::global();
    let owner = &job_context.owner_session_id;
    let background = parsed_args.background;
    let job = match runtime.spawn(
        ShellJobSpawn {
            owner_session_id: owner.clone(),
            origin: job_context.origin,
            command: parsed_args.command.clone(),
            purpose: parsed_args.purpose.trim().to_string(),
            shell_label: shell_label.clone(),
            cwd: effective_cwd.to_path_buf(),
            jobs_dir: runtime.jobs_directory(owner),
            stdin: parsed_args.stdin.clone(),
            background,
            timeout: parsed_args.timeout_ms.map(Duration::from_millis),
            capture_streams: !background,
        },
        command,
    ) {
        Ok(job) => job,
        Err(error) => return not_started(shell_label, error.to_string()),
    };

    let end = if background {
        ShellCommandCallEnd::Background {
            reason: String::from("Started in the background as requested."),
        }
    } else {
        // The attachment stops the job if this future is dropped before the job finishes or detaches.
        // 若本 future 在任务结束或转后台前被丢弃，附着会停止任务。
        let foreground = ShellJobForeground::attach(Arc::clone(&job));
        let mut interjection = runtime.subscribe_interjection(owner);
        let end = match job.wait_interruptible(None, &mut interjection).await {
            ShellJobWaitOutcome::Finished(outcome) => ShellCommandCallEnd::Finished(outcome),
            ShellJobWaitOutcome::Interjected => {
                if job.detach(false) || (job.is_background() && job.outcome().is_none()) {
                    ShellCommandCallEnd::Background {
                        reason: String::from(
                            "The user sent a message while this command was running, so the host moved it to the background for you to answer. It keeps running without a timeout.",
                        ),
                    }
                } else {
                    ShellCommandCallEnd::Finished(job.wait().await)
                }
            }
            ShellJobWaitOutcome::Detached | ShellJobWaitOutcome::StillRunning => {
                ShellCommandCallEnd::Background {
                    reason: String::from(
                        "The user moved this command to the background. It keeps running without a timeout.",
                    ),
                }
            }
        };
        foreground.release();
        end
    };
    let duration_ms = match &end {
        ShellCommandCallEnd::Finished(outcome) => outcome.duration_ms,
        _ => job.elapsed_ms(),
    };
    let tail = job
        .log_reader()
        .and_then(|reader| reader.tail(SHELL_OUTPUT_TAIL_BUDGET_BYTES).ok());
    ShellCommandExecutionReport {
        end,
        job_id: Some(job.id().to_string()),
        shell_label,
        cwd: effective_cwd.to_path_buf(),
        duration_ms,
        tail,
        streams: job.captured_streams().unwrap_or_default(),
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
        format!("Purpose: {}", parsed_args.purpose.trim()),
        format!("Shell: {}", execution_report.shell_label),
        format!(
            "Working directory: {}",
            normalized_workspace_path_text_from_path(&execution_report.cwd)
        ),
    ];
    if let Some(job_id) = &execution_report.job_id {
        lines.push(format!("Job: {job_id}"));
    }
    lines.push(format!("Status: {}", execution_report.status_label()));
    if execution_report.is_background() {
        lines.push(format!("Running for: {} ms", execution_report.duration_ms));
    } else {
        lines.push(format!("Duration: {} ms", execution_report.duration_ms));
    }
    if let Some(exit_code) = execution_report.exit_code() {
        lines.push(format!("Exit code: {exit_code}"));
    }
    if let Some(reason) = execution_report.failure_reason() {
        lines.push(format!("Reason: {reason}"));
    }
    if !hook_report.completed_hooks.is_empty()
        && hook_report.original_command != hook_report.effective_command
    {
        lines.push(format!(
            "Plugin hooks: transformed by {} completed hook(s)",
            hook_report.completed_hooks.len()
        ));
    }
    if hook_report.original_command != hook_report.effective_command {
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
    let shown = execution_report
        .tail
        .as_ref()
        .filter(|tail| !tail.rows.is_empty());
    if let (Some(tail), Some(job_id)) = (shown, &execution_report.job_id)
        && tail.truncated
    {
        lines.push(format!(
            "Output: {} lines, {} bytes in total; only the end is shown. Read earlier lines or search with `shell_job` (`read` or `search`, job_id `{job_id}`).",
            tail.total_lines, tail.total_bytes
        ));
    }

    let mut content = lines.join("\n");
    match shown {
        Some(tail) => {
            content.push_str("\n\n[output]\n");
            content.push_str(render_plain_rows(&tail.rows).trim_end());
        }
        None if !execution_report.is_background()
            && execution_report.failure_reason().is_none() =>
        {
            content.push_str("\n\n(no output)");
        }
        None => {}
    }
    if let (true, Some(job_id)) = (execution_report.is_background(), &execution_report.job_id) {
        content.push_str(&format!(
            "\n\nThe command keeps running as background job `{job_id}`; you will be told when it finishes. Meanwhile use `shell_job` to wait for it, read or search its output, check its processes, or stop it."
        ));
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
    ToolResultBudgetSnapshot::uniform_bytes(SHELL_INLINE_OUTPUT_LIMIT_BYTES)
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
    let tail = execution_report.tail.as_ref();
    json!({
        "session_id": session_id.map(|value| value.to_string()),
        "command": parsed_args.command,
        "purpose": parsed_args.purpose.trim(),
        "job_id": execution_report.job_id,
        "background": execution_report.is_background(),
        "cwd": normalized_workspace_path_text_from_path(&execution_report.cwd),
        "shell": execution_report.shell_label,
        "status": execution_report.status_label(),
        "exit_code": execution_report.exit_code(),
        "duration_ms": execution_report.duration_ms,
        "output_lines": tail.map(|tail| tail.total_lines),
        "output_bytes": tail.map(|tail| tail.total_bytes),
        "output_truncated": tail.is_some_and(|tail| tail.truncated),
        "combined_output_overflowed": combined_output_overflowed,
        "failure_reason": execution_report.failure_reason(),
        "hook_transform": hook_report,
    })
}

/// Executes one `shell_command` invocation after applying the generic plugin transform chain.
/// 在应用通用插件转换链后执行一次 `shell_command` 调用。
pub(crate) async fn execute_shell_command_tool(
    request: NativeToolExecutionRequest,
    shell_support: &HostCommandShellSupport,
    plugin_runtime: Option<&vulcan_plugin_runtime::VulcanPluginRuntimeManager>,
    rtk: &PersistedRtkConfiguration,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let vulcan_environment = child_process_environment(&request).await?;
    let job_context = ShellCommandJobContext::of_request(&request);
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
    let native_rewrite = apply_native_rtk_rewrite(&parsed_args.command, &effective_cwd, rtk).await;
    let (effective_args, mut process_options, hook_report) = apply_shell_command_plugin_hooks(
        &parsed_args,
        &effective_cwd,
        native_rewrite,
        request.workspace_root.as_deref(),
        session_id,
        plugin_runtime,
    )
    .await?;
    process_options.host_environment = rtk_host_environment(rtk.telemetry_disabled);
    process_options.vulcan_environment = vulcan_environment;

    let execution_report = execute_shell_command_process(
        &effective_args,
        &effective_cwd,
        shell_support,
        &process_options,
        &job_context,
    )
    .await;
    let render_input =
        build_shell_command_text_render_input(&effective_args, &execution_report, &hook_report);
    let budget = default_shell_command_budget_snapshot();
    let exceeds_inline_budget = shell_command_output_exceeds_inline_budget(&render_input, &budget);
    let host_payload = build_shell_command_host_payload(
        session_id,
        &effective_args,
        &execution_report,
        exceeds_inline_budget,
        &hook_report,
    );
    let render_options = HostRenderOptions {
        spill_root,
        template_skill_roots: Vec::new(),
        template_resources_root: None,
    };
    // Only overflowing output writes a spill page; keep that filesystem work off the async worker.
    // 只有超限输出会写入分页文件；该文件系统操作不占用异步工作线程。
    let model_output = if exceeds_inline_budget {
        tokio::task::spawn_blocking(move || {
            render_tool_result_text(&render_input, None, &budget, &render_options)
        })
        .await
        .map_err(|error| {
            NativeToolError::new(format!("shell_command output rendering failed: {error}"))
        })?
    } else {
        render_tool_result_text(&render_input, None, &budget, &render_options)
    }
    .map_err(NativeToolError::new)?;

    // The raw streams stay in memory so a chain step can read them even when the text spilled.
    // 原始流仅保留在内存中，使链条步骤在文本已溢出时仍可读取。
    let streams = execution_report.streams;
    Ok(
        VulcanToolInvocationResult::from_model_output(Value::String(model_output))
            .with_host_result(VulcanToolHostResult {
                kind: String::from("command_run"),
                payload: host_payload,
            })
            .with_raw_streams(VulcanToolRawStreams {
                stdout: streams.stdout,
                stderr: streams.stderr,
            }),
    )
}

#[cfg(test)]
mod process_option_tests {
    use super::*;

    /// Arguments of one test command.
    /// 一条测试命令的参数。
    fn args(command: &str, shell: Option<&str>, timeout_ms: Option<u64>) -> ShellCommandToolArgs {
        ShellCommandToolArgs {
            command: command.to_string(),
            cwd: None,
            timeout_ms,
            stdin: None,
            shell: shell.map(str::to_string),
            purpose: String::from("Test command"),
            background: false,
        }
    }

    /// Job context with a unique owner, so tests never share interjection signals.
    /// 具有唯一所有者的任务上下文，使测试之间从不共享插队信号。
    fn job_context() -> ShellCommandJobContext {
        ShellCommandJobContext {
            owner_session_id: format!("shell-command-test-{}", Uuid::new_v4()),
            origin: None,
        }
    }

    /// Runs one command with default options.
    /// 以默认选项运行一条命令。
    async fn run(
        parsed_args: &ShellCommandToolArgs,
        cwd: &Path,
        context: &ShellCommandJobContext,
    ) -> ShellCommandExecutionReport {
        tokio::time::timeout(
            Duration::from_secs(60),
            execute_shell_command_process(
                parsed_args,
                cwd,
                &HostCommandShellSupport::from_configured_default_id(None),
                &ShellCommandProcessOptions::default(),
                context,
            ),
        )
        .await
        .expect("test command must return in time")
    }

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

    /// An explicit zero timeout is rejected because only omission means unlimited.
    /// 显式的 0 超时会被拒绝，因为只有省略才表示无限等待。
    #[test]
    fn zero_timeout_is_rejected_instead_of_meaning_unlimited() {
        let error = parse_shell_command_tool_arguments(serde_json::json!({
            "command": "echo hi",
            "purpose": "Say hi",
            "timeout_ms": 0,
        }))
        .expect_err("zero timeout must be rejected");
        assert!(error.message.contains("at least 1"));
        let parsed = parse_shell_command_tool_arguments(
            serde_json::json!({ "command": "echo hi", "purpose": "Say hi" }),
        )
        .expect("omitted timeout stays valid");
        assert_eq!(parsed.timeout_ms, None);
        assert!(!parsed.background);
    }

    /// The purpose is required and may not be blank.
    /// 用途必填且不得为空白。
    #[test]
    fn purpose_is_required() {
        parse_shell_command_tool_arguments(serde_json::json!({ "command": "echo hi" }))
            .expect_err("a missing purpose must be rejected");
        parse_shell_command_tool_arguments(serde_json::json!({
            "command": "echo hi",
            "purpose": "  ",
        }))
        .expect_err("a blank purpose must be rejected");
    }

    /// A misspelled field is rejected instead of silently running without the intended option.
    /// 拼错的字段会被拒绝，而不是在缺少预期选项的情况下静默执行。
    #[test]
    fn unknown_argument_is_rejected() {
        parse_shell_command_tool_arguments(serde_json::json!({
            "command": "echo hi",
            "purpose": "Say hi",
            "timeout": 1000,
        }))
        .expect_err("an undeclared field must be rejected");
    }

    /// Large stdin must be written while output is drained so echoing commands cannot deadlock on full pipes.
    /// 大体量 stdin 必须在排空输出的同时写入，避免回显命令因管道写满而死锁。
    #[cfg(unix)]
    #[tokio::test]
    async fn large_stdin_is_written_while_output_is_drained() {
        let temp = tempfile::tempdir().expect("create shell stdin test directory");
        // stdinText exceeds typical pipe buffers in both directions but fits the raw stream capture.
        // stdinText 在两个方向上都超过常见管道缓冲区容量，但不超过原始流保留大小。
        let stdin_text = "x".repeat(2 * 1024 * 1024);
        let mut parsed_args = args("cat", Some("sh"), Some(60_000));
        parsed_args.stdin = Some(stdin_text.clone());
        let report = run(&parsed_args, temp.path(), &job_context()).await;
        assert_eq!(report.status_label(), "completed");
        // Login profiles may print banners first, so only the echoed tail is asserted.
        // 登录配置可能先输出横幅，因此只断言回显的尾部内容。
        assert!(report.streams.stdout.ends_with(&stdin_text));
    }

    /// Omitted stdin must be empty instead of inheriting the service input stream.
    /// 省略 stdin 时必须为空输入，而不是继承服务自身的输入流。
    #[cfg(unix)]
    #[tokio::test]
    async fn omitted_stdin_reads_end_of_input() {
        let temp = tempfile::tempdir().expect("create shell stdin test directory");
        let report = run(
            &args("cat; echo done", Some("sh"), Some(60_000)),
            temp.path(),
            &job_context(),
        )
        .await;
        assert_eq!(report.status_label(), "completed");
        assert!(report.streams.stdout.trim_end().ends_with("done"));
    }

    /// The card's live view finds the job by its tool call and reads the same output the result holds.
    /// 卡片实时视图按工具调用找到任务，并读取与结果相同的输出。
    #[tokio::test]
    async fn live_view_reads_the_output_of_the_call_job() {
        let temp = tempfile::tempdir().expect("create shell live tail test directory");
        let mut context = job_context();
        context.origin = Some(ShellJobOrigin {
            api_call_id: 1,
            tool_call_id: 1,
        });
        let report = run(
            &args("echo live-tail-check", None, Some(60_000)),
            temp.path(),
            &context,
        )
        .await;
        assert_eq!(report.status_label(), "completed");
        assert!(report.streams.stdout.contains("live-tail-check"));
        let live = super::super::shell_command_live::shell_command_live_run(
            &super::super::shell_command_live::ShellCommandLiveKey::new(
                &context.owner_session_id,
                1,
                1,
            ),
        )
        .expect("the finished job is still kept in memory");
        assert!(!live.is_running());
        let lines = live.recent_lines();
        assert!(
            lines.iter().any(|line| line.contains("live-tail-check")),
            "{lines:?}"
        );
    }

    /// Long output is no longer cut off: the call completes and returns a bounded tail of whole lines.
    /// 长输出不再被截断：调用照常完成，并返回由完整行组成的有界尾部。
    #[tokio::test]
    async fn long_output_completes_with_a_bounded_tail() {
        let temp = tempfile::tempdir().expect("create shell long output test directory");
        #[cfg(windows)]
        let command = args(
            "for /L %i in (1,1,6000) do @echo output line number %i",
            Some("cmd"),
            Some(120_000),
        );
        #[cfg(not(windows))]
        let command = args(
            "i=1; while [ $i -le 6000 ]; do echo output line number $i; i=$((i+1)); done",
            Some("sh"),
            Some(120_000),
        );
        let report = run(&command, temp.path(), &job_context()).await;
        assert_eq!(report.status_label(), "completed");
        let tail = report.tail.as_ref().expect("tail");
        assert!(tail.truncated);
        assert!(tail.total_lines >= 6000);
        let rendered = render_plain_rows(&tail.rows);
        assert!(
            rendered.len() <= SHELL_OUTPUT_TAIL_BUDGET_BYTES,
            "{}",
            rendered.len()
        );
        assert!(rendered.trim_end().ends_with("output line number 6000"));
        let render_input = build_shell_command_text_render_input(
            &command,
            &report,
            &ShellCommandHookReport::default(),
        );
        assert!(render_input.content.contains("shell_job"));
    }

    /// A background call returns its job at once and the job keeps running.
    /// 后台调用立即返回其任务，任务继续运行。
    #[tokio::test]
    async fn background_call_returns_the_job_at_once() {
        let temp = tempfile::tempdir().expect("create shell background test directory");
        let context = job_context();
        #[cfg(windows)]
        let mut command = args("ping -n 31 127.0.0.1 >NUL", Some("cmd"), None);
        #[cfg(not(windows))]
        let mut command = args("sleep 30", Some("sh"), None);
        command.background = true;
        let report = run(&command, temp.path(), &context).await;
        assert_eq!(report.status_label(), SHELL_COMMAND_BACKGROUND_STATUS);
        let job_id = report.job_id.clone().expect("job id");
        let runtime = ShellJobRuntime::global();
        let job = runtime
            .job(&context.owner_session_id, &job_id)
            .expect("job is registered");
        assert!(job.outcome().is_none() && job.is_background());
        assert_eq!(
            runtime.stop_session_jobs(&context.owner_session_id, "cleanup"),
            1
        );
        job.wait().await;
    }

    /// An interjection moves a running foreground command to the background instead of stopping it.
    /// 插队把运行中的前台命令转到后台，而不是停止它。
    #[tokio::test]
    async fn interjection_moves_a_foreground_command_to_the_background() {
        let temp = tempfile::tempdir().expect("create shell interjection test directory");
        let context = job_context();
        #[cfg(windows)]
        let command = args("ping -n 31 127.0.0.1 >NUL", Some("cmd"), Some(600_000));
        #[cfg(not(windows))]
        let command = args("sleep 30", Some("sh"), Some(600_000));
        let owner = context.owner_session_id.clone();
        let signal = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            ShellJobRuntime::global().notify_interjection(&owner);
        });
        let report = run(&command, temp.path(), &context).await;
        signal.await.expect("signal task");
        assert_eq!(report.status_label(), SHELL_COMMAND_BACKGROUND_STATUS);
        let runtime = ShellJobRuntime::global();
        let job = runtime
            .job(
                &context.owner_session_id,
                report.job_id.as_deref().expect("job id"),
            )
            .expect("job is registered");
        assert!(job.outcome().is_none());
        runtime.stop_session_jobs(&context.owner_session_id, "cleanup");
        job.wait().await;
    }

    /// PowerShell reads non-ASCII stdin unchanged through `[Console]::In`, whatever the console code
    /// page; the command prints the UTF-8 bytes it decoded as hex so output decoding cannot interfere.
    /// PowerShell 无论控制台代码页如何都能通过 `[Console]::In` 原样读取非 ASCII 的 stdin；命令以十六进制
    /// 打印其解码得到的 UTF-8 字节，使输出解码无法干扰结果。
    #[cfg(windows)]
    #[tokio::test]
    async fn powershell_reads_utf8_stdin_unchanged() {
        let temp = tempfile::tempdir().expect("create shell stdin test directory");
        let stdin_text = "\u{4e2d}\u{6587} \u{2713} \u{e9}\nline two\n";
        let expected = stdin_text
            .bytes()
            .map(|byte| format!("{byte:02X}"))
            .collect::<Vec<_>>()
            .join("-");
        let shell_support = HostCommandShellSupport::from_configured_default_id(None);
        let powershells = shell_support
            .supported_shells()
            .iter()
            .filter(|shell| {
                matches!(
                    shell.shell,
                    HostCommandShell::Pwsh | HostCommandShell::Powershell
                )
            })
            .map(|shell| shell.shell.id())
            .collect::<Vec<_>>();
        for shell in powershells {
            let mut parsed_args = args(
                "[BitConverter]::ToString([Text.Encoding]::UTF8.GetBytes([Console]::In.ReadToEnd()))",
                Some(shell),
                Some(60_000),
            );
            parsed_args.stdin = Some(stdin_text.to_string());
            let report = run(&parsed_args, temp.path(), &job_context()).await;
            assert_eq!(report.status_label(), "completed", "{shell}");
            assert_eq!(report.streams.stdout.trim(), expected, "{shell}");
        }
    }

    /// Timeouts must kill descendants holding the pipes and still return output produced so far.
    /// 超时必须终止持有管道的后代进程，并仍返回已产生的输出。
    #[cfg(unix)]
    #[tokio::test]
    async fn timeout_kills_descendants_and_keeps_partial_output() {
        let temp = tempfile::tempdir().expect("create shell timeout test directory");
        let report = run(
            &args("echo partial; sleep 600 & wait", Some("sh"), Some(500)),
            temp.path(),
            &job_context(),
        )
        .await;
        assert_eq!(report.status_label(), "timed_out");
        assert!(report.streams.stdout.contains("partial"));
    }

    /// Background processes inheriting the pipes must not keep a naturally exited command running.
    /// 继承管道的后台进程不得让已自然退出的命令持续挂起。
    #[cfg(unix)]
    #[tokio::test]
    async fn natural_exit_stops_waiting_for_background_pipe_holders() {
        let temp = tempfile::tempdir().expect("create shell background test directory");
        let report = run(
            &args("sleep 30 & echo done", Some("sh"), None),
            temp.path(),
            &job_context(),
        )
        .await;
        assert_eq!(report.status_label(), "completed");
        assert!(report.streams.stdout.trim_end().ends_with("done"));
        assert!(
            report
                .failure_reason()
                .is_some_and(|reason| reason.contains("background processes"))
        );
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
        // processOptions models the canonical directory resolved from registration_id plus binary_id.
        // processOptions 模拟通过 registration_id 与 binary_id 解析出的规范目录。
        let process_options = ShellCommandProcessOptions {
            path_prefixes: vec![temp.path().to_path_buf()],
            environment: BTreeMap::new(),
            host_environment: Vec::new(),
            vulcan_environment: Vec::new(),
        };
        // report captures the real child-shell lookup result using only the injected PATH prefix.
        // report 捕获仅使用注入 PATH 前缀时真实子 Shell 的查找结果。
        let report = execute_shell_command_process(
            &args("rtk", Some("cmd"), Some(5_000)),
            temp.path(),
            &HostCommandShellSupport::from_configured_default_id(None),
            &process_options,
            &job_context(),
        )
        .await;
        assert_eq!(report.status_label(), "completed");
        assert!(report.streams.stdout.contains("plugin-path-ok"));
    }
}
