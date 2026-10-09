//! Terminal-session native tool implementation.
//! 终端会话原生工具实现。

use super::*;

mod input;
mod interaction;
mod lifecycle;
mod output;
mod render;
mod shell;
mod wait;

pub(crate) use input::*;
pub(crate) use interaction::*;
pub(crate) use lifecycle::*;
pub(crate) use output::*;
pub(crate) use render::*;
pub(crate) use shell::*;
pub(crate) use wait::*;

/// Stable identifier of the interactive terminal-session native tool.
/// 交互式终端会话原生工具的稳定标识。
pub const TERMINAL_SESSION_TOOL_ID: &str = "terminal_session";

pub(crate) fn build_terminal_session_tool(
    command_session_runtime: Arc<CommandSessionRuntime>,
    shell_support: HostCommandShellSupport,
) -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from(TERMINAL_SESSION_TOOL_ID),
        name: String::from(TERMINAL_SESSION_TOOL_ID),
        description: terminal_session_tool_description(),
        help_text: None,
        ai_visible: true,
        available_in_plan: true,
    };

    build_native_tool_definition(
        descriptor,
        terminal_session_args_schema(&shell_support),
        move |request: NativeToolExecutionRequest| {
            let command_session_runtime = Arc::clone(&command_session_runtime);
            let shell_support = shell_support.clone();
            Box::pin(async move {
                execute_terminal_session_tool(request, command_session_runtime, &shell_support)
                    .await
            })
        },
    )
}

/// One action accepted by the host-owned `terminal_session` tool.
/// 宿主自有 `terminal_session` 工具接受的一种动作。
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TerminalSessionToolAction {
    /// List persistent terminal sessions owned by the current controller session.
    /// 列出当前控制器会话拥有的持久终端会话。
    List,
    /// Start one persistent terminal session.
    /// 启动一条持久终端会话。
    Start,
    /// Send ordered input events into an existing terminal session.
    /// 向已有终端会话发送保序输入事件。
    Send,
    /// Read new output from the AI-facing incremental cursor.
    /// 从面向 AI 的增量游标读取新输出。
    Read,
    /// Inspect current terminal state without advancing the read cursor.
    /// 检查当前终端状态但不推进读取游标。
    Snapshot,
    /// Stop an existing terminal session.
    /// 停止已有终端会话。
    Stop,
}

impl TerminalSessionToolAction {
    /// Renders one stable lowercase action label for model output and host metadata.
    /// 为模型输出与宿主元数据渲染稳定的小写动作标签。
    fn as_str(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Start => "start",
            Self::Send => "send",
            Self::Read => "read",
            Self::Snapshot => "snapshot",
            Self::Stop => "stop",
        }
    }
}

/// One ordered terminal input item accepted by `terminal_session`.
/// `terminal_session` 接受的一条保序终端输入项。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum TerminalSessionInput {
    /// Raw text typed into the terminal.
    /// 输入到终端的原始文本。
    Text {
        /// Text payload to write.
        /// 要写入的文本载荷。
        text: String,
    },
    /// Text pasted into the terminal as one payload.
    /// 作为一份载荷粘贴到终端的文本。
    Paste {
        /// Paste payload to write.
        /// 要写入的粘贴载荷。
        text: String,
    },
    /// One standardized terminal key.
    /// 一个标准化终端按键。
    Key {
        /// Stable key name such as Enter or ArrowDown.
        /// 稳定按键名称，例如 Enter 或 ArrowDown。
        key: String,
    },
    /// One standardized key chord.
    /// 一个标准化组合键。
    Chord {
        /// Base key for the chord.
        /// 组合键的基础按键。
        key: String,
        /// Whether Ctrl is held.
        /// 是否按住 Ctrl。
        ctrl: Option<bool>,
        /// Whether Alt is held.
        /// 是否按住 Alt。
        alt: Option<bool>,
        /// Whether Shift is held.
        /// 是否按住 Shift。
        shift: Option<bool>,
        /// Whether Meta is held.
        /// 是否按住 Meta。
        meta: Option<bool>,
    },
}

/// One wait mode accepted by `terminal_session`.
/// `terminal_session` 接受的一种等待模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TerminalSessionWaitMode {
    /// Return immediately after the input or read request.
    /// 在输入或读取请求后立即返回。
    ReturnImmediately,
    /// Wait for at least one output event within the bounded wait budget.
    /// 在有界等待预算内等待至少一条输出事件。
    UntilQuiet,
    /// Wait using the same bounded behavior while prompt detection remains best-effort.
    /// 在提示符检测仍为尽力而为时使用相同有界等待行为。
    UntilPrompt,
    /// Wait for output matching a known readiness pattern.
    /// 等待匹配已知就绪模式的输出。
    UntilPattern,
    /// Wait briefly for screen-like output to stabilize.
    /// 短暂等待屏幕型输出稳定。
    ScreenStable,
}

impl TerminalSessionWaitMode {
    /// Renders one stable lowercase wait-mode label.
    /// 渲染稳定的小写等待模式标签。
    fn as_str(self) -> &'static str {
        match self {
            Self::ReturnImmediately => "return_immediately",
            Self::UntilQuiet => "until_quiet",
            Self::UntilPrompt => "until_prompt",
            Self::UntilPattern => "until_pattern",
            Self::ScreenStable => "screen_stable",
        }
    }
}

/// One wait-policy payload accepted by `terminal_session`.
/// `terminal_session` 接受的一份等待策略载荷。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TerminalSessionWaitPolicy {
    /// Requested wait mode.
    /// 请求的等待模式。
    mode: TerminalSessionWaitMode,
    /// Optional quiet window in milliseconds.
    /// 可选静默窗口毫秒数。
    quiet_ms: Option<u64>,
    /// Optional maximum wait budget in milliseconds.
    /// 可选最大等待预算毫秒数。
    max_wait_ms: Option<u64>,
    /// Optional readiness or output pattern.
    /// 可选就绪或输出模式。
    pattern: Option<String>,
}

/// One typed argument payload accepted by the host-owned `terminal_session` native tool.
/// 宿主自有 `terminal_session` 原生工具接受的一份强类型参数载荷。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TerminalSessionToolArgs {
    /// Requested terminal-session action.
    /// 请求的终端会话动作。
    action: TerminalSessionToolAction,
    /// Optional user-facing name for a newly started terminal session.
    /// 新启动终端会话的可选用户可见名称。
    name: Option<String>,
    /// Optional target terminal-session identifier used by non-start actions.
    /// 供非 start 动作使用的可选目标终端会话标识。
    terminal_session_id: Option<String>,
    /// Optional raw command string used by the start action.
    /// 供 start 动作使用的可选原始命令字符串。
    command: Option<String>,
    /// Optional working directory override resolved relative to the workspace root when needed.
    /// 需要时相对工作区根目录解析的可选工作目录覆盖值。
    cwd: Option<String>,
    /// Optional shell family selector such as `auto`, `powershell`, `cmd`, or `bash`.
    /// 可选 shell 类型选择器，例如 `auto`、`powershell`、`cmd` 或 `bash`。
    shell: Option<String>,
    /// Ordered terminal inputs used by the send action.
    /// 供 send 动作使用的保序终端输入。
    inputs: Option<Vec<TerminalSessionInput>>,
    /// Optional wait policy used by send/read actions.
    /// 供 send/read 动作使用的可选等待策略。
    wait_policy: Option<TerminalSessionWaitPolicy>,
    /// Optional output cursor override.
    /// 可选输出游标覆盖值。
    after_seq: Option<u64>,
    /// Whether read should ignore the AI cursor and read from the beginning.
    /// read 是否应忽略 AI 游标并从开头读取。
    read_from_start: Option<bool>,
    /// Optional maximum chunk count returned by one read-like action.
    /// 一次读取类动作返回的可选最大分片数量。
    max_chunks: Option<usize>,
}

/// Result summary produced by an interrupt drain pass.
/// 一次中断排空过程产生的结果摘要。
#[derive(Debug, Clone)]
pub(crate) struct TerminalInterruptDrainResult {
    /// Latest terminal snapshot observed by the drain pass.
    /// 排空过程中观测到的最新终端快照。
    snapshot: CommandSessionSnapshot,
    /// Model-visible output chunks kept after control-only fragments are discarded.
    /// 丢弃纯控制片段后保留给模型可见的输出分片。
    returned_chunks: Vec<CommandSessionOutputChunk>,
    /// Whether more buffered output remains after the drain pass.
    /// 排空过程结束后是否仍存在更多缓冲输出。
    has_more: bool,
    /// Milliseconds spent in the drain pass.
    /// 排空过程消耗的毫秒数。
    elapsed_ms: u64,
    /// Number of control-only chunks consumed but hidden from model output.
    /// 已消费但从模型输出中隐藏的纯控制分片数量。
    discarded_chunk_count: usize,
}

/// Result summary produced by a best-effort interrupt input-line cleanup pass.
/// 一次尽力而为中断输入行清理过程产生的结果摘要。
#[derive(Debug, Clone)]
pub(crate) struct TerminalInterruptCleanupResult {
    /// Latest terminal snapshot observed by the cleanup pass.
    /// 清理过程中观测到的最新终端快照。
    snapshot: CommandSessionSnapshot,
    /// Whether more buffered output remains after cleanup.
    /// 清理结束后是否仍存在更多缓冲输出。
    has_more: bool,
    /// Milliseconds spent in the cleanup pass.
    /// 清理过程消耗的毫秒数。
    elapsed_ms: u64,
    /// Number of cleanup output chunks consumed and hidden from model output.
    /// 已消费但从模型输出中隐藏的清理输出分片数量。
    discarded_chunk_count: usize,
    /// Optional cleanup failure reason when the best-effort cleanup could not run.
    /// 尽力清理无法运行时的可选失败原因。
    failure_reason: Option<String>,
}

/// Result summary produced by one terminal wait-mode read pass.
/// 一次终端等待模式读取过程产生的结果摘要。
#[derive(Debug, Clone)]
pub(crate) struct TerminalWaitReadResult {
    /// Latest terminal snapshot observed by the wait pass.
    /// 等待过程中观测到的最新终端快照。
    snapshot: CommandSessionSnapshot,
    /// Output chunks returned across the whole wait pass.
    /// 整个等待过程返回的输出分片。
    returned_chunks: Vec<CommandSessionOutputChunk>,
    /// Whether more buffered output remains after the wait pass.
    /// 等待过程结束后是否仍存在更多缓冲输出。
    has_more: bool,
    /// Milliseconds spent in the wait pass.
    /// 等待过程消耗的毫秒数。
    elapsed_ms: u64,
    /// Whether the requested pattern was actually observed.
    /// 是否实际观测到了请求的模式。
    pattern_matched: Option<bool>,
    /// Whether a quiet window with no new output was observed.
    /// 是否观测到一段没有新输出的静默窗口。
    quiet_observed: Option<bool>,
}

/// One resolved shell behavior snapshot for a command session, separating the outer launcher shell from the inferred interactive shell.
/// 一份命令会话的已解析 shell 行为快照，用于区分外层启动器 shell 与推断得到的交互 shell。
#[derive(Debug, Clone)]
pub(crate) struct ResolvedSessionShellBehavior {
    /// Stable shell label used for newline semantics and user-facing interaction summaries.
    /// 用于换行语义与面向用户交互摘要的稳定 shell 标签。
    pub(crate) interactive_shell_label: String,
    /// Stable outer launcher shell label selected directly by the host shell resolver.
    /// 由宿主 shell 解析器直接选定的稳定外层启动器 shell 标签。
    pub(crate) launcher_shell_label: String,
    /// Whether the runtime inferred that the command enters a nested interactive shell different from the outer launcher.
    /// 运行时是否推断出该命令进入了一个不同于外层启动器的嵌套交互 shell。
    pub(crate) nested_shell_inferred: bool,
}

fn terminal_session_tool_description() -> String {
    String::from(
        "Operate named persistent host terminal sessions. Before using terminal_session, you must call skill(\"terminal-session\") and follow its instructions. Do not use this tool from memory. The start action requires a non-empty name. Use shell_command for isolated one-shot commands and exact process exit codes. terminal_session supports list/start/send/read/snapshot/stop, ordered cross-platform ANSI/VT text/paste/key/chord input, incremental output, and TUI-aware output mode hints. The host never wraps or rewrites commands to infer per-command exit codes.",
    )
}

/// Builds the terminal-session-specific schema text for the `shell` argument.
/// 为 `terminal_session` 的 `shell` 参数构建专用 schema 文本。
fn terminal_session_schema_shell_description(shell_support: &HostCommandShellSupport) -> String {
    format!(
        "{} Start launches a persistent terminal process. Use an explicit shell when deterministic interactive behavior or newline semantics matter.",
        shell_support.schema_shell_description()
    )
}

/// Builds the JSON Schema advertised to the model for the host-owned `terminal_session` tool.
/// 为宿主自有 `terminal_session` 工具构建对模型声明的 JSON Schema。
pub(crate) fn terminal_session_args_schema(shell_support: &HostCommandShellSupport) -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["list", "start", "send", "read", "snapshot", "stop"],
                "description": "Terminal action. `list` returns current terminal sessions owned by this controller session, `start` launches a named persistent terminal, `send` writes ordered inputs, `read` returns new output from the AI cursor, `snapshot` inspects current state without advancing the cursor, and `stop` terminates the session."
            },
            "name": {
                "type": "string",
                "description": "Required for `start`; ignored by other actions. Provide a non-empty short task-oriented terminal name such as `dev server`, `python repl`, or `install shell`, so UI rows and later `list` calls are easy to understand."
            },
            "terminal_session_id": {
                "type": "string",
                "description": "Persistent terminal session identifier returned by `start` or `list`. Required for `send`, `read`, `snapshot`, and `stop`."
            },
            "command": {
                "type": "string",
                "description": "Raw command text used only by `start`, such as `pwsh -NoLogo`, `cmd`, `bash`, `python`, or a TUI command."
            },
            "cwd": {
                "type": "string",
                "description": "Optional working directory used only by `start`. Relative paths resolve against the workspace root when available. Later sends inherit terminal state and cannot change host-side cwd except by typing shell commands."
            },
            "shell": {
                "type": "string",
                "enum": shell_support.schema_shell_ids(),
                "description": terminal_session_schema_shell_description(shell_support)
            },
            "inputs": {
                "type": "array",
                "description": "Ordered terminal inputs used by `send`. Do not hand-write ANSI escapes or control characters; use key/chord items.",
                "items": {
                    "anyOf": [
                        {
                            "type": "object",
                            "properties": {
                                "kind": { "type": "string", "enum": ["text"] },
                                "text": { "type": "string", "description": "Text typed into the terminal. Use ordinary newline characters for logical submissions; the host normalizes CRLF, CR, and LF to the active interactive shell newline." }
                            },
                            "required": ["kind", "text"],
                            "additionalProperties": false
                        },
                        {
                            "type": "object",
                            "properties": {
                                "kind": { "type": "string", "enum": ["paste"] },
                                "text": { "type": "string", "description": "Text pasted into the terminal as one ordered payload. The host normalizes CRLF, CR, and LF to the active interactive shell newline; bracketed-paste mode is not implied." }
                            },
                            "required": ["kind", "text"],
                            "additionalProperties": false
                        },
                        {
                            "type": "object",
                            "properties": {
                                "kind": { "type": "string", "enum": ["key"] },
                                "key": { "type": "string", "description": "Standard key name such as Enter, Tab, Esc, ArrowDown, Home, End, PageUp, PageDown, Insert, Delete, or F1-F12. The cross-platform terminal host encodes navigation and function keys as ANSI/VT input." }
                            },
                            "required": ["kind", "key"],
                            "additionalProperties": false
                        },
                        {
                            "type": "object",
                            "properties": {
                                "kind": { "type": "string", "enum": ["chord"] },
                                "key": { "type": "string", "description": "Base key for the shortcut, such as c, d, z, l, r, Tab, ArrowLeft, or ArrowRight." },
                                "ctrl": { "type": "boolean" },
                                "alt": { "type": "boolean" },
                                "shift": { "type": "boolean" },
                                "meta": { "type": "boolean" }
                            },
                            "required": ["kind", "key"],
                            "additionalProperties": false
                        }
                    ]
                }
            },
            "wait_policy": {
                "type": "object",
                "description": "Optional bounded wait behavior for send/read. Use until_pattern for a caller-chosen visible completion or readiness marker, until_prompt for best-effort prompt detection, until_quiet for bounded output settling, screen_stable for TUI frames, or return_immediately for asynchronous work.",
                "properties": {
                    "mode": {
                        "type": "string",
                        "enum": ["return_immediately", "until_quiet", "until_prompt", "until_pattern", "screen_stable"]
                    },
                    "quiet_ms": { "type": "integer", "minimum": 0, "maximum": MAX_COMMAND_SESSION_READ_WAIT_MS },
                    "max_wait_ms": { "type": "integer", "minimum": 0, "maximum": MAX_COMMAND_SESSION_READ_WAIT_MS },
                    "pattern": { "type": "string" }
                },
                "required": ["mode"],
                "additionalProperties": false
            },
            "after_seq": {
                "type": "integer",
                "description": "Optional output cursor override. When omitted, read/send uses the terminal_session AI cursor."
            },
            "read_from_start": {
                "type": "boolean",
                "description": "When true, read starts from sequence 0 instead of the AI cursor. Use only for explicit history review."
            },
            "max_chunks": {
                "type": "integer",
                "minimum": 1,
                "maximum": MAX_COMMAND_SESSION_READ_CHUNKS,
                "description": format!("Optional maximum output chunks returned by one read-like action. Default: {DEFAULT_COMMAND_SESSION_READ_MAX_CHUNKS}; host maximum: {MAX_COMMAND_SESSION_READ_CHUNKS}.")
            }
        },
        "required": ["action"],
        "additionalProperties": false
    })
}

fn parse_terminal_session_tool_arguments(
    arguments: Value,
) -> Result<TerminalSessionToolArgs, NativeToolError> {
    serde_json::from_value::<TerminalSessionToolArgs>(arguments).map_err(|error| {
        NativeToolError::new(format!(
            "Failed to parse terminal_session arguments: {error}"
        ))
    })
}

fn require_terminal_session_owner_session_id(
    session_id: Option<SessionId>,
) -> Result<SessionId, NativeToolError> {
    session_id.ok_or_else(|| {
        NativeToolError::new(
            "The terminal_session tool requires a stable controller session identifier, but no session_id was attached to this invocation.",
        )
    })
}

/// Parses the caller-provided terminal-session identifier into the stable UUID-shaped session id.
/// 将调用方提供的终端会话标识解析为稳定的 UUID 形态会话 id。
fn parse_terminal_session_id(
    raw_terminal_session_id: Option<&str>,
) -> Result<SessionId, NativeToolError> {
    let raw_terminal_session_id = raw_terminal_session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            NativeToolError::new(
                "The terminal_session tool is missing the required `terminal_session_id` argument.",
            )
        })?;
    Uuid::parse_str(raw_terminal_session_id).map_err(|error| {
        NativeToolError::new(format!(
            "The `terminal_session_id` provided to terminal_session is not a valid UUID: {error}"
        ))
    })
}

/// Builds a stable fallback terminal name from a start command.
/// 根据启动命令构建稳定的终端回退名称。
fn terminal_session_default_name(command_text: &str) -> String {
    let compact = command_text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if compact.is_empty() {
        return String::from("terminal");
    }
    compact.chars().take(60).collect()
}

/// Normalizes a user-provided terminal name, falling back to the start command when empty.
/// 规范化用户提供的终端名称，并在为空时回退到启动命令。
#[cfg(test)]
pub(crate) fn normalize_terminal_session_name(
    raw_name: Option<&str>,
    command_text: &str,
) -> String {
    let candidate = raw_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| terminal_session_default_name(command_text));
    candidate.chars().take(80).collect()
}

/// Requires and normalizes the explicit terminal name provided by a start request.
/// 要求并规范化 start 请求显式提供的终端名称。
fn require_terminal_session_start_name(raw_name: Option<&str>) -> Result<String, NativeToolError> {
    let candidate = raw_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            NativeToolError::new(
                "The `start` action for terminal_session requires a non-empty `name`.",
            )
        })?;
    Ok(candidate.chars().take(80).collect())
}

/// Returns the runtime-stored terminal name or a stable fallback for historical sessions.
/// 返回运行时保存的终端名称，或为历史会话生成稳定回退名称。
fn terminal_session_name_for_snapshot(snapshot: &CommandSessionSnapshot) -> String {
    snapshot
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| terminal_session_default_name(&snapshot.command))
}
