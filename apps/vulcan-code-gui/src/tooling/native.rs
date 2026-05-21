//! Built-in native tools exposed to the host runtime foundation.
//! 暴露给宿主运行时基础架构的内建原生工具。

use crate::agent_runtime::config::AgentRuntimeConfig;
use crate::model::SessionId;
use crate::project_temp_maintenance;
use crate::tool_result_format::{
    EffectiveBudgetScope, HostRenderOptions, HostTextRenderInput, HostToolOverflowMode,
    ToolResultBudgetSnapshot, render_tool_result_text,
};
use crate::tooling::command_session_runtime::{
    CommandSessionOutputChunk, CommandSessionRuntime, CommandSessionSnapshot,
    CommandSessionStartRequest, DEFAULT_COMMAND_SESSION_STOP_WAIT_MS,
};
use crate::tooling::output_decode::decode_shell_output_bytes;
use crate::tooling::shell_host::HostCommandShellSupport;
use futures_util::future::BoxFuture;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::process::Command as TokioCommand;
use tokio::time::{Duration, timeout};
use uuid::Uuid;
use vulcan_tools_manager::{VulcanToolHostResult, VulcanToolInvocationResult};

/// One lightweight descriptor for a registered native tool.
/// 单个已注册原生工具的轻量描述。
#[derive(Debug, Clone)]
pub struct NativeToolDescriptor {
    /// Stable tool identifier.
    /// 稳定工具标识。
    pub id: String,
    /// Human-readable tool name.
    /// 人类可读的工具名称。
    pub name: String,
    /// Human-readable tool description.
    /// 人类可读的工具描述。
    pub description: String,
    /// Whether the tool should be directly visible to AI agents.
    /// 当前工具是否应直接对 AI 代理可见。
    pub ai_visible: bool,
    /// Whether the tool may remain visible in Plan mode.
    /// 当前工具是否允许在 Plan 模式继续可见。
    pub available_in_plan: bool,
}

/// One serialisable runtime snapshot captured by the native tool layer.
/// 原生工具层捕获的一份可序列化运行时快照。
#[derive(Debug, Clone)]
pub struct NativeToolRuntimeSnapshot {
    /// Current workspace directory used by the desktop process.
    /// 当前桌面进程使用的工作区目录。
    pub workspace_directory: String,
    /// Current active model reference.
    /// 当前激活模型引用。
    pub active_model: Option<String>,
    /// Declared agent count.
    /// 已声明代理数量。
    pub agent_count: usize,
    /// Declared skill count.
    /// 已声明技能数量。
    pub skill_count: usize,
    /// Configured MCP server identifiers.
    /// 已配置的 MCP 服务器标识集合。
    pub configured_mcp_server_ids: Vec<String>,
    /// Enabled MCP server identifiers.
    /// 已启用的 MCP 服务器标识集合。
    pub enabled_mcp_server_ids: Vec<String>,
}

/// One lightweight native-tool error surfaced by the host-side tool layer.
/// 宿主侧工具层对外暴露的轻量原生工具错误。
#[derive(Debug, Clone)]
pub struct NativeToolError {
    /// Human-readable error message.
    /// 人类可读错误消息。
    pub message: String,
}

impl NativeToolError {
    /// Builds one native-tool error from a human-readable message.
    /// 根据一条人类可读消息构建原生工具错误。
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for NativeToolError {
    /// Render the native-tool error as one display string.
    /// 将原生工具错误渲染为一条展示字符串。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl Error for NativeToolError {}

/// One host-owned execution request passed to native tools together with runtime context.
/// 连同运行时上下文一起传给原生工具的单条宿主执行请求。
#[derive(Debug, Clone)]
pub struct NativeToolExecutionRequest {
    /// Raw JSON arguments provided by the model-side tool call.
    /// 由模型侧工具调用提供的原始 JSON 参数。
    pub arguments: Value,
    /// Optional stable controller session identifier that owns this tool invocation.
    /// 拥有当前工具调用的可选稳定控制器会话标识。
    pub session_id: Option<SessionId>,
    /// Optional workspace root resolved for the current execution turn.
    /// 为当前执行回合解析得到的可选工作区根目录。
    pub workspace_root: Option<PathBuf>,
    /// Optional host-managed spill directory used for oversized output paging.
    /// 用于超长输出分页落盘的可选宿主管理超限目录。
    pub spill_root: Option<PathBuf>,
}

/// Shared boxed future returned by one native-tool handler.
/// 单个原生工具处理器返回的共享装箱 future。
type NativeToolFuture = BoxFuture<'static, Result<VulcanToolInvocationResult, NativeToolError>>;

/// Shared async handler signature used by one host-owned native tool.
/// 单个宿主自有原生工具使用的共享异步处理器签名。
type NativeToolHandler = dyn Fn(NativeToolExecutionRequest) -> NativeToolFuture + Send + Sync;

/// One host-owned native tool definition used as the source of truth for AI and host surfaces.
/// 作为 AI 与宿主表面事实标准的单个宿主自有原生工具定义。
#[derive(Clone)]
pub struct NativeToolDefinition {
    /// Stable descriptor metadata.
    /// 稳定描述元数据。
    pub descriptor: NativeToolDescriptor,
    /// JSON Schema describing tool arguments.
    /// 描述工具参数的 JSON Schema。
    pub args_schema: Value,
    /// Shared async handler that executes the tool.
    /// 执行该工具的共享异步处理器。
    handler: Arc<NativeToolHandler>,
}

impl NativeToolDefinition {
    /// Executes the native tool handler with one structured execution request.
    /// 使用单条结构化执行请求执行原生工具处理器。
    pub async fn invoke(
        &self,
        request: NativeToolExecutionRequest,
    ) -> Result<VulcanToolInvocationResult, NativeToolError> {
        (self.handler)(request).await
    }
}

/// Aggregate native tool catalog built for the current runtime configuration.
/// 为当前运行时配置构建的原生工具目录聚合体。
#[derive(Clone)]
pub struct NativeToolCatalog {
    /// Host-owned native tool definitions keyed by stable tool identifier.
    /// 以稳定工具标识为键保存的宿主自有原生工具定义。
    pub definitions: BTreeMap<String, Arc<NativeToolDefinition>>,
    /// Lightweight tool descriptors used by UI and diagnostics.
    /// 供界面与诊断使用的轻量工具描述集合。
    pub descriptors: Vec<NativeToolDescriptor>,
}

impl NativeToolCatalog {
    /// Resolves one native-tool definition by its stable identifier.
    /// 通过稳定标识解析单个原生工具定义。
    pub fn definition(&self, tool_id: &str) -> Option<Arc<NativeToolDefinition>> {
        self.definitions.get(tool_id).cloned()
    }

    /// Invokes one host-owned native tool through the unified definition table.
    /// 通过统一定义表调用单个宿主自有原生工具。
    pub async fn invoke(
        &self,
        tool_id: &str,
        request: NativeToolExecutionRequest,
    ) -> Result<VulcanToolInvocationResult, NativeToolError> {
        let Some(definition) = self.definition(tool_id) else {
            return Err(NativeToolError::new(format!(
                "未找到原生工具 `{tool_id}`。"
            )));
        };
        definition.invoke(request).await
    }
}

/// Builds the built-in native tool catalog from the runtime aggregate configuration.
/// 根据运行时聚合配置构建内建原生工具目录。
pub fn build_native_tool_catalog(
    config: &AgentRuntimeConfig,
    configured_default_command_shell_id: Option<&str>,
    command_session_runtime: Arc<CommandSessionRuntime>,
) -> NativeToolCatalog {
    let snapshot = NativeToolRuntimeSnapshot {
        workspace_directory: std::env::current_dir()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| String::from(".")),
        active_model: config.active_model.clone(),
        agent_count: config.agents.len(),
        skill_count: config.skills.len(),
        configured_mcp_server_ids: config.configured_mcp_server_ids.clone(),
        enabled_mcp_server_ids: config.enabled_mcp_server_ids.clone(),
    };
    let snapshot = Arc::new(snapshot);
    let shell_support =
        HostCommandShellSupport::from_configured_default_id(configured_default_command_shell_id);

    let definitions = vec![
        build_workspace_describe_tool(snapshot.clone()),
        build_agent_list_tool(snapshot.clone(), config),
        build_skill_list_tool(snapshot.clone(), config),
        build_shell_command_tool(shell_support.clone()),
        build_shell_session_tool(command_session_runtime, shell_support),
        build_mcp_server_list_tool(snapshot),
    ];

    let mut definition_map = BTreeMap::new();
    let mut descriptors = Vec::new();
    for definition in definitions {
        descriptors.push(definition.descriptor.clone());
        definition_map.insert(definition.descriptor.id.clone(), definition);
    }

    NativeToolCatalog {
        definitions: definition_map,
        descriptors,
    }
}

/// Builds one host-owned native tool definition from the shared metadata and async handler.
/// 根据共享元数据与异步处理器构建单个宿主自有原生工具定义。
fn build_native_tool_definition(
    descriptor: NativeToolDescriptor,
    args_schema: Value,
    handler: impl Fn(NativeToolExecutionRequest) -> NativeToolFuture + Send + Sync + 'static,
) -> Arc<NativeToolDefinition> {
    Arc::new(NativeToolDefinition {
        descriptor,
        args_schema,
        handler: Arc::new(handler),
    })
}

/// Builds the workspace-describe host-owned native tool.
/// 构建工作区概览宿主自有原生工具。
fn build_workspace_describe_tool(
    snapshot: Arc<NativeToolRuntimeSnapshot>,
) -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from("native_workspace_describe"),
        name: String::from("工作区概览"),
        description: String::from("返回当前工作目录、激活模型以及基础运行时统计。"),
        ai_visible: false,
        available_in_plan: false,
    };

    build_native_tool_definition(
        descriptor,
        json!({"type": "object"}),
        move |_request: NativeToolExecutionRequest| {
            let snapshot = snapshot.clone();
            Box::pin(async move {
                Ok(VulcanToolInvocationResult::from_model_output(json!({
                    "workspaceDirectory": snapshot.workspace_directory,
                    "activeModel": snapshot.active_model,
                    "agentCount": snapshot.agent_count,
                    "skillCount": snapshot.skill_count,
                    "configuredMcpServers": snapshot.configured_mcp_server_ids,
                    "enabledMcpServers": snapshot.enabled_mcp_server_ids,
                })))
            })
        },
    )
}

/// Builds the agent-list host-owned native tool.
/// 构建代理列表宿主自有原生工具。
fn build_agent_list_tool(
    snapshot: Arc<NativeToolRuntimeSnapshot>,
    config: &AgentRuntimeConfig,
) -> Arc<NativeToolDefinition> {
    let items = config
        .agents
        .iter()
        .map(|agent| {
            json!({
                "id": agent.id,
                "name": agent.name,
                "kind": agent.kind.label(),
                "description": agent.description,
                "systemPrompt": agent.system_prompt,
                "skills": agent.skill_ids,
                "nativeTools": agent.native_tools,
                "mcpTools": agent.mcp_tools,
                "delegates": agent.delegates,
                "toolPolicy": agent.tool_policy,
                "fallbackDelegate": agent.fallback_delegate,
                "model": agent.model,
                "temperature": agent.temperature,
                "maxTurns": agent.max_turns,
                "routes": agent
                    .routes
                    .iter()
                    .map(|route| {
                        json!({
                            "signal": route.signal,
                            "delegate": route.delegate,
                        })
                    })
                    .collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();
    let items = Arc::new(items);
    let descriptor = NativeToolDescriptor {
        id: String::from("native_registry_list_agents"),
        name: String::from("列出代理"),
        description: String::from("返回当前已声明的代理清单。"),
        ai_visible: false,
        available_in_plan: false,
    };

    build_native_tool_definition(
        descriptor,
        json!({"type": "object"}),
        move |_request: NativeToolExecutionRequest| {
            let items = items.clone();
            let snapshot = snapshot.clone();
            Box::pin(async move {
                Ok(VulcanToolInvocationResult::from_model_output(json!({
                    "workspaceDirectory": snapshot.workspace_directory,
                    "agents": &*items,
                })))
            })
        },
    )
}

/// Builds the skill-list host-owned native tool.
/// 构建技能列表宿主自有原生工具。
fn build_skill_list_tool(
    snapshot: Arc<NativeToolRuntimeSnapshot>,
    config: &AgentRuntimeConfig,
) -> Arc<NativeToolDefinition> {
    let items = config
        .skills
        .iter()
        .map(|skill| {
            json!({
                "id": skill.id,
                "name": skill.name,
                "kind": skill.kind.label(),
                "description": skill.description,
                "enabled": skill.enabled,
                "triggerSignals": skill.trigger_signals,
                "emittedSignal": skill.emitted_signal,
                "confidenceDelta": skill.confidence_delta,
            })
        })
        .collect::<Vec<_>>();
    let items = Arc::new(items);
    let descriptor = NativeToolDescriptor {
        id: String::from("native_registry_list_skills"),
        name: String::from("列出技能"),
        description: String::from("返回当前已声明的技能清单。"),
        ai_visible: false,
        available_in_plan: false,
    };

    build_native_tool_definition(
        descriptor,
        json!({"type": "object"}),
        move |_request: NativeToolExecutionRequest| {
            let items = items.clone();
            let snapshot = snapshot.clone();
            Box::pin(async move {
                Ok(VulcanToolInvocationResult::from_model_output(json!({
                    "workspaceDirectory": snapshot.workspace_directory,
                    "skills": &*items,
                })))
            })
        },
    )
}

/// Builds the one-shot shell-command host-owned native tool.
/// 构建一次性 shell 命令宿主自有原生工具。
fn build_shell_command_tool(shell_support: HostCommandShellSupport) -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from("shell_command"),
        name: String::from("执行命令"),
        description: String::from(
            "在宿主环境中执行一次命令，返回退出状态、标准输出与错误输出摘要。",
        ),
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

/// Builds the session-oriented shell-command host-owned native tool.
/// 构建面向会话的 shell 命令宿主自有原生工具。
fn build_shell_session_tool(
    command_session_runtime: Arc<CommandSessionRuntime>,
    shell_support: HostCommandShellSupport,
) -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from("shell_session"),
        name: String::from("命令会话"),
        description: String::from("在宿主环境中启动、读取、写入或停止一条可持续交互的命令会话。"),
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

/// Builds the MCP-server list host-owned native tool.
/// 构建 MCP 服务器列表宿主自有原生工具。
fn build_mcp_server_list_tool(
    snapshot: Arc<NativeToolRuntimeSnapshot>,
) -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from("native_mcp_list_servers"),
        name: String::from("列出 MCP 服务器"),
        description: String::from("返回当前 MCP 服务器配置及启用情况。"),
        ai_visible: false,
        available_in_plan: false,
    };

    build_native_tool_definition(
        descriptor,
        json!({"type": "object"}),
        move |_request: NativeToolExecutionRequest| {
            let snapshot = snapshot.clone();
            Box::pin(async move {
                Ok(VulcanToolInvocationResult::from_model_output(json!({
                    "configured": snapshot.configured_mcp_server_ids,
                    "enabled": snapshot.enabled_mcp_server_ids,
                })))
            })
        },
    )
}

/// One typed argument payload accepted by the host-owned `shell_command` native tool.
/// 宿主自有 `shell_command` 原生工具接受的一份强类型参数载荷。
#[derive(Debug, Clone, Deserialize)]
struct ShellCommandToolArgs {
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
enum ShellCommandExecutionStatus {
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

/// One normalized shell-command execution report used by both model text and host metadata.
/// 同时供模型文本与宿主元数据复用的一份规范化 shell 命令执行报告。
#[derive(Debug, Clone)]
struct ShellCommandExecutionReport {
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

/// One action accepted by the host-owned `shell_session` tool.
/// 宿主自有 `shell_session` 工具接受的一种动作。
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ShellSessionToolAction {
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
struct ShellSessionToolArgs {
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

/// Builds the JSON Schema advertised to the model for the host-owned `shell_command` tool.
/// 为宿主自有 `shell_command` 工具构建对模型声明的 JSON Schema。
fn shell_command_args_schema(shell_support: &HostCommandShellSupport) -> Value {
    json!({
        "type": "object",
        "properties": {
            "command": {
                "type": "string",
                "description": "需要执行的原始命令字符串。"
            },
            "cwd": {
                "type": "string",
                "description": "可选工作目录；相对路径会优先基于当前工作区根目录解析。"
            },
            "timeout_ms": {
                "type": "integer",
                "description": "可选超时时间（毫秒）；省略时使用宿主默认等待策略。"
            },
            "stdin": {
                "type": "string",
                "description": "可选标准输入文本，会在等待进程输出前写入子进程。"
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

/// Builds the JSON Schema advertised to the model for the host-owned `shell_session` tool.
/// 为宿主自有 `shell_session` 工具构建对模型声明的 JSON Schema。
fn shell_session_args_schema(shell_support: &HostCommandShellSupport) -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["start", "read", "write", "stop"],
                "description": "命令会话动作类型。"
            },
            "command_session_id": {
                "type": "string",
                "description": "命令会话标识；read/write/stop 时必填。"
            },
            "command": {
                "type": "string",
                "description": "要启动的原始命令字符串；start 时必填。"
            },
            "cwd": {
                "type": "string",
                "description": "可选工作目录；相对路径会优先基于当前工作区根目录解析。"
            },
            "shell": {
                "type": "string",
                "enum": shell_support.schema_shell_ids(),
                "description": shell_support.schema_shell_description()
            },
            "stdin": {
                "type": "string",
                "description": "可选标准输入文本；start 时表示初始输入，write 时表示追加输入。"
            },
            "append_newline": {
                "type": "boolean",
                "description": "write 动作是否在输入末尾追加换行。"
            },
            "after_seq": {
                "type": "integer",
                "description": "read 动作使用的输出游标，只返回更高序号的新分片。"
            },
            "wait_for_output_ms": {
                "type": "integer",
                "description": "read 动作在没有新输出时的可选等待毫秒数。"
            },
            "max_chunks": {
                "type": "integer",
                "description": "read 动作返回的最大输出分片数。"
            }
        },
        "required": ["action"],
        "additionalProperties": false
    })
}

/// Parses one raw model payload into the typed shell-session argument structure.
/// 将一份原始模型载荷解析为强类型 shell 会话参数结构。
fn parse_shell_session_tool_arguments(
    arguments: Value,
) -> Result<ShellSessionToolArgs, NativeToolError> {
    serde_json::from_value::<ShellSessionToolArgs>(arguments)
        .map_err(|error| NativeToolError::new(format!("shell_session 参数解析失败：{error}")))
}

/// Resolves the controller session that owns the current shell-session tool invocation.
/// 解析拥有当前 shell 会话工具调用的控制器会话。
fn require_shell_session_owner_session_id(
    session_id: Option<SessionId>,
) -> Result<SessionId, NativeToolError> {
    session_id.ok_or_else(|| {
        NativeToolError::new(
            "shell_session 需要稳定的控制器会话标识，但当前调用未附带 session_id。",
        )
    })
}

/// Parses the caller-provided command-session identifier into the stable UUID-shaped session id.
/// 将调用方提供的命令会话标识解析为稳定的 UUID 形态会话 id。
fn parse_command_session_id(
    raw_command_session_id: Option<&str>,
) -> Result<SessionId, NativeToolError> {
    let raw_command_session_id = raw_command_session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| NativeToolError::new("shell_session 缺少必填参数 `command_session_id`。"))?;
    Uuid::parse_str(raw_command_session_id).map_err(|error| {
        NativeToolError::new(format!(
            "shell_session 的 `command_session_id` 不是合法 UUID：{error}"
        ))
    })
}

/// Returns the platform-preferred interactive newline sequence used by shell-session write operations.
/// 返回 shell 会话写入操作使用的平台首选交互换行序列。
fn platform_shell_newline() -> &'static str {
    if cfg!(windows) { "\r\n" } else { "\n" }
}

/// Builds one model-visible text render input for shell-session actions that do not need to include buffered output chunks.
/// 为不需要包含缓冲输出分片的 shell 会话动作构建一份模型可见文本渲染输入。
fn build_shell_session_summary_render_input(
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
        format!("Working directory: {}", snapshot.cwd.display()),
        format!("Status: {}", snapshot.status.as_str()),
        format!("Stdout bytes: {}", snapshot.stdout_bytes),
        format!("Stderr bytes: {}", snapshot.stderr_bytes),
        format!("Last sequence: {}", snapshot.last_seq),
    ];
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
fn build_shell_session_read_render_input(
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

/// Parses one raw model payload into the typed shell-command argument structure.
/// 将一份原始模型载荷解析为强类型 shell 命令参数结构。
fn parse_shell_command_tool_arguments(
    arguments: Value,
) -> Result<ShellCommandToolArgs, NativeToolError> {
    let parsed = serde_json::from_value::<ShellCommandToolArgs>(arguments)
        .map_err(|error| NativeToolError::new(format!("shell_command 参数解析失败：{error}")))?;
    if parsed.command.trim().is_empty() {
        return Err(NativeToolError::new(
            "shell_command 的 `command` 不能为空字符串。",
        ));
    }
    Ok(parsed)
}

/// Resolves the effective working directory for one shell-command invocation.
/// 为单次 shell 命令调用解析实际工作目录。
fn resolve_shell_command_working_directory(
    workspace_root: Option<&Path>,
    raw_cwd: Option<&str>,
) -> Result<PathBuf, NativeToolError> {
    let base_directory = match workspace_root {
        Some(root) => root.to_path_buf(),
        None => std::env::current_dir()
            .map_err(|error| NativeToolError::new(format!("读取当前工作目录失败：{error}")))?,
    };
    let Some(raw_cwd) = raw_cwd.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(base_directory);
    };
    let requested_path = PathBuf::from(raw_cwd);
    if requested_path.is_absolute() {
        return Ok(requested_path);
    }
    Ok(base_directory.join(requested_path))
}

/// Resolves the spill root used by one shell-command invocation when oversized output must be paged.
/// 为单次 shell 命令调用解析在超长输出需要分页时使用的超限目录。
fn resolve_shell_command_spill_root(
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
fn resolve_shell_command_launcher(
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
async fn execute_shell_command_process(
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
                failure_reason: Some(format!("启动命令失败：{error}")),
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
                        failure_reason: Some(format!("写入命令标准输入失败：{error}")),
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
                    failure_reason: Some(String::from("命令未提供可写入的标准输入通道。")),
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
                        failure_reason: Some(format!("等待命令输出失败：{error}")),
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
                    failure_reason: Some(format!("命令执行超过超时限制（{} ms）。", timeout_ms)),
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
                    failure_reason: Some(format!("等待命令输出失败：{error}")),
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
fn build_shell_command_text_render_input(
    parsed_args: &ShellCommandToolArgs,
    execution_report: &ShellCommandExecutionReport,
) -> HostTextRenderInput {
    let mut lines = vec![
        format!("Command: {}", parsed_args.command),
        format!("Shell: {}", execution_report.shell_label),
        format!("Working directory: {}", execution_report.cwd.display()),
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
fn default_shell_command_budget_snapshot() -> ToolResultBudgetSnapshot {
    ToolResultBudgetSnapshot {
        tool_result: EffectiveBudgetScope {
            bytes: 200_000,
            lines: -1,
        },
        file_read: EffectiveBudgetScope {
            bytes: 200_000,
            lines: -1,
        },
    }
}

/// Decide whether the combined shell-command text already exceeds the inline tool-result budget.
/// 判断合并后的 shell 命令文本是否已经超出内联工具结果预算。
fn shell_command_output_exceeds_inline_budget(
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
fn build_shell_command_host_payload(
    session_id: Option<SessionId>,
    parsed_args: &ShellCommandToolArgs,
    execution_report: &ShellCommandExecutionReport,
    combined_output_overflowed: bool,
) -> Value {
    json!({
        "session_id": session_id.map(|value| value.to_string()),
        "command": parsed_args.command,
        "cwd": execution_report.cwd.display().to_string(),
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

/// Builds the host-only structured payload returned for one shell-session tool action.
/// 为单次 shell 会话工具动作构建宿主专用结构化返回载荷。
fn build_shell_session_host_payload(
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
        "cwd": snapshot.cwd.display().to_string(),
        "shell": snapshot.shell_label.clone(),
        "status": snapshot.status.as_str(),
        "exit_code": snapshot.exit_code,
        "stdout_bytes": snapshot.stdout_bytes,
        "stderr_bytes": snapshot.stderr_bytes,
        "last_seq": snapshot.last_seq,
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
async fn execute_shell_session_start_action(
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
        .ok_or_else(|| NativeToolError::new("shell_session 的 start 动作需要非空 `command`。"))?;
    let effective_cwd = resolve_shell_command_working_directory(
        request.workspace_root.as_deref(),
        parsed_args.cwd.as_deref(),
    )?;
    let spill_root = resolve_shell_command_spill_root(
        request.spill_root.as_deref(),
        request.workspace_root.as_deref(),
        &effective_cwd,
    );
    let (launcher_program, launcher_args, shell_label) =
        resolve_shell_command_launcher(shell_support, parsed_args.shell.as_deref(), command_text)?;
    let snapshot = command_session_runtime
        .start_session(CommandSessionStartRequest {
            owner_session_id,
            command: command_text.to_string(),
            shell_label,
            cwd: effective_cwd,
            launcher_program,
            launcher_args,
            initial_stdin: parsed_args.stdin,
        })
        .await
        .map_err(NativeToolError::new)?;
    let render_input = build_shell_session_summary_render_input(
        ShellSessionToolAction::Start,
        &snapshot,
        Vec::new(),
    );
    let budget = default_shell_command_budget_snapshot();
    let host_payload = build_shell_session_host_payload(
        ShellSessionToolAction::Start,
        &snapshot,
        shell_command_output_exceeds_inline_budget(&render_input, &budget),
        json!({}),
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
async fn execute_shell_session_read_action(
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
async fn execute_shell_session_write_action(
    parsed_args: ShellSessionToolArgs,
    owner_session_id: SessionId,
    command_session_runtime: Arc<CommandSessionRuntime>,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let command_session_id = parse_command_session_id(parsed_args.command_session_id.as_deref())?;
    let input_text = parsed_args
        .stdin
        .ok_or_else(|| NativeToolError::new("shell_session 的 write 动作需要 `stdin`。"))?;
    let input_text = if parsed_args.append_newline.unwrap_or(false) {
        format!("{input_text}{}", platform_shell_newline())
    } else {
        input_text
    };
    let write_outcome = command_session_runtime
        .write_session(owner_session_id, command_session_id, input_text)
        .await
        .map_err(NativeToolError::new)?;
    let render_input = build_shell_session_summary_render_input(
        ShellSessionToolAction::Write,
        &write_outcome.snapshot,
        vec![
            format!("Bytes written: {}", write_outcome.bytes_written),
            format!(
                "Appended newline: {}",
                if parsed_args.append_newline.unwrap_or(false) {
                    "yes"
                } else {
                    "no"
                }
            ),
        ],
    );
    let budget = default_shell_command_budget_snapshot();
    let host_payload = build_shell_session_host_payload(
        ShellSessionToolAction::Write,
        &write_outcome.snapshot,
        shell_command_output_exceeds_inline_budget(&render_input, &budget),
        json!({
            "bytes_written": write_outcome.bytes_written,
            "appended_newline": parsed_args.append_newline.unwrap_or(false),
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
async fn execute_shell_session_stop_action(
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
async fn execute_shell_session_tool(
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

/// Executes the host-owned `shell_command` tool and returns a unified tool invocation result.
/// 执行宿主自有 `shell_command` 工具并返回统一工具调用结果。
async fn execute_shell_command_tool(
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

#[cfg(test)]
mod tests {
    use super::{
        NativeToolExecutionRequest, build_agent_list_tool, build_mcp_server_list_tool,
        build_shell_command_tool, build_shell_session_tool, build_skill_list_tool,
        build_workspace_describe_tool, resolve_shell_command_launcher,
        resolve_shell_command_working_directory, shell_command_args_schema,
        shell_session_args_schema,
    };
    use crate::agent_runtime::config::{
        AgentRuntimeConfig, RuntimeAgentDefinition, RuntimeAgentKind, RuntimeAgentRoute,
        RuntimeSkillDefinition, RuntimeSkillKind,
    };
    use crate::tooling::CommandSessionRuntime;
    use crate::tooling::shell_host::{
        DetectedHostCommandShell, HostCommandShell, HostCommandShellSupport,
    };
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::path::Path;
    use std::sync::Arc;
    use uuid::Uuid;

    /// Builds one compact detected-shell record for tests that only care about shell identity and dynamic schema exposure.
    /// 为仅关注 shell 身份与动态 schema 暴露的测试构建一条紧凑检测记录。
    fn detected_shell(shell: HostCommandShell) -> DetectedHostCommandShell {
        DetectedHostCommandShell {
            launcher_program: shell.id().to_string(),
            shell,
        }
    }

    /// Builds one test shell-support snapshot that mirrors the runtime-facing resolved-shell structure.
    /// 构建一份镜像运行时解析结构的测试 shell 支持快照。
    fn test_shell_support() -> HostCommandShellSupport {
        HostCommandShellSupport::from_test_parts(
            vec![
                detected_shell(HostCommandShell::Pwsh),
                detected_shell(HostCommandShell::Powershell),
                detected_shell(HostCommandShell::Cmd),
            ],
            Some(HostCommandShell::Pwsh),
        )
    }

    /// Builds one staged-output test command that emits two chunks with a delay between them so the read cursor can be validated across multiple calls.
    /// 构建一条分阶段输出测试命令：带延时地输出两个分片，便于在多次调用之间校验读取游标。
    fn interactive_shell_session_test_command() -> String {
        if cfg!(windows) {
            String::from(
                "echo phase-1 & ping -n 2 127.0.0.1 >nul & echo phase-2 & ping -n 2 127.0.0.1 >nul",
            )
        } else {
            String::from("printf 'phase-1\\n'; sleep 0.6; printf 'phase-2\\n'; sleep 0.3")
        }
    }

    /// Builds one long-running test command that keeps the child process alive until the host explicitly stops it.
    /// 构建一条长时间运行的测试命令，使子进程持续存活直到宿主显式停止它。
    fn long_running_shell_session_test_command() -> String {
        if cfg!(windows) {
            String::from("while ($true) { Start-Sleep -Milliseconds 100 }")
        } else {
            String::from("while true; do sleep 0.1; done")
        }
    }

    /// Verifies that the agent registry tool exposes extended runtime fields for diagnostics and future delegate orchestration.
    /// 验证代理注册表工具会暴露扩展运行时字段，用于诊断与后续委派编排。
    #[tokio::test]
    async fn agent_registry_tool_exposes_extended_runtime_fields() {
        let snapshot = Arc::new(super::NativeToolRuntimeSnapshot {
            workspace_directory: String::from("D:/workspace"),
            active_model: Some(String::from("openai/gpt-5")),
            agent_count: 1,
            skill_count: 1,
            configured_mcp_server_ids: vec![String::from("server-a")],
            enabled_mcp_server_ids: vec![String::from("server-a")],
        });
        let config = AgentRuntimeConfig {
            active_model: Some(String::from("openai/gpt-5")),
            active_agent_id: String::from("agent-a"),
            agents: vec![RuntimeAgentDefinition {
                id: String::from("agent-a"),
                name: String::from("Agent A"),
                kind: RuntimeAgentKind::Coordinator,
                description: String::from("Coordinator agent"),
                system_prompt: String::from("system prompt"),
                skill_ids: vec![String::from("skill-a")],
                native_tools: vec![String::from("native-a")],
                mcp_tools: vec![String::from("mcp-tool-a")],
                delegates: vec![String::from("agent-b")],
                tool_policy: Some(String::from("policy-a")),
                routes: vec![RuntimeAgentRoute {
                    signal: String::from("needs-lua"),
                    delegate: String::from("lua-agent"),
                }],
                fallback_delegate: Some(String::from("agent-b")),
                model: Some(String::from("anthropic/claude")),
                temperature: Some(0.2),
                max_turns: Some(9),
            }],
            skills: vec![RuntimeSkillDefinition {
                id: String::from("skill-a"),
                name: String::from("Skill A"),
                kind: RuntimeSkillKind::Regular,
                description: String::from("Skill description"),
                enabled: true,
                trigger_signals: vec![String::from("signal-a")],
                emitted_signal: Some(String::from("signal-b")),
                confidence_delta: 0.5,
            }],
            tool_policies: BTreeMap::new(),
            configured_mcp_server_ids: vec![String::from("server-a")],
            enabled_mcp_server_ids: vec![String::from("server-a")],
        };

        let tool = build_agent_list_tool(snapshot, &config);
        let payload = tool
            .invoke(NativeToolExecutionRequest {
                arguments: serde_json::Value::Object(Default::default()),
                session_id: None,
                workspace_root: None,
                spill_root: None,
            })
            .await
            .expect("agent registry tool should execute");
        let payload = payload.model_output;

        let agent = &payload["agents"][0];
        assert_eq!(agent["description"], "Coordinator agent");
        assert_eq!(agent["systemPrompt"], "system prompt");
        assert_eq!(agent["fallbackDelegate"], "agent-b");
        assert_eq!(agent["model"], "anthropic/claude");
        let temperature = agent["temperature"]
            .as_f64()
            .expect("temperature should serialize as f64");
        assert!((temperature - 0.2).abs() < 1e-6);
        assert_eq!(agent["maxTurns"], 9);
        assert_eq!(agent["routes"][0]["signal"], "needs-lua");
        assert_eq!(agent["routes"][0]["delegate"], "lua-agent");
    }

    /// Verifies that host-diagnostic discovery tools remain registered but are hidden from AI-facing tool exposure.
    /// 验证宿主诊断发现类工具仍会注册，但不会暴露到 AI 可见工具面中。
    #[test]
    fn host_diagnostic_tools_are_hidden_from_ai() {
        let snapshot = Arc::new(super::NativeToolRuntimeSnapshot {
            workspace_directory: String::from("D:/workspace"),
            active_model: Some(String::from("openai/gpt-5")),
            agent_count: 0,
            skill_count: 0,
            configured_mcp_server_ids: vec![String::from("server-a")],
            enabled_mcp_server_ids: vec![String::from("server-a")],
        });
        let config = AgentRuntimeConfig {
            active_model: Some(String::from("openai/gpt-5")),
            active_agent_id: String::from("agent-a"),
            agents: Vec::new(),
            skills: Vec::new(),
            tool_policies: BTreeMap::new(),
            configured_mcp_server_ids: vec![String::from("server-a")],
            enabled_mcp_server_ids: vec![String::from("server-a")],
        };

        let workspace_tool = build_workspace_describe_tool(Arc::clone(&snapshot));
        let skill_tool = build_skill_list_tool(Arc::clone(&snapshot), &config);
        let mcp_tool = build_mcp_server_list_tool(snapshot);

        assert!(!workspace_tool.descriptor.ai_visible);
        assert!(!workspace_tool.descriptor.available_in_plan);
        assert!(!skill_tool.descriptor.ai_visible);
        assert!(!skill_tool.descriptor.available_in_plan);
        assert!(!mcp_tool.descriptor.ai_visible);
        assert!(!mcp_tool.descriptor.available_in_plan);
    }

    /// Verifies that the skill registry tool exposes skill metadata that used to stay hidden behind dead-code suppressions.
    /// 验证技能注册表工具会暴露此前被 dead-code 抑制隐藏的技能元数据。
    #[tokio::test]
    async fn skill_registry_tool_exposes_extended_runtime_fields() {
        let snapshot = Arc::new(super::NativeToolRuntimeSnapshot {
            workspace_directory: String::from("D:/workspace"),
            active_model: Some(String::from("openai/gpt-5")),
            agent_count: 1,
            skill_count: 1,
            configured_mcp_server_ids: vec![],
            enabled_mcp_server_ids: vec![],
        });
        let config = AgentRuntimeConfig {
            active_model: Some(String::from("openai/gpt-5")),
            active_agent_id: String::from("agent-a"),
            agents: vec![],
            skills: vec![RuntimeSkillDefinition {
                id: String::from("skill-a"),
                name: String::from("Skill A"),
                kind: RuntimeSkillKind::Automation,
                description: String::from("Skill description"),
                enabled: true,
                trigger_signals: vec![String::from("signal-a")],
                emitted_signal: Some(String::from("signal-b")),
                confidence_delta: 0.5,
            }],
            tool_policies: BTreeMap::new(),
            configured_mcp_server_ids: vec![],
            enabled_mcp_server_ids: vec![],
        };

        let tool = build_skill_list_tool(snapshot, &config);
        let payload = tool
            .invoke(NativeToolExecutionRequest {
                arguments: serde_json::Value::Object(Default::default()),
                session_id: None,
                workspace_root: None,
                spill_root: None,
            })
            .await
            .expect("skill registry tool should execute");
        let payload = payload.model_output;

        let skill = &payload["skills"][0];
        assert_eq!(skill["description"], "Skill description");
        assert_eq!(skill["emittedSignal"], "signal-b");
        assert_eq!(skill["confidenceDelta"], 0.5);
    }

    /// Relative shell-command working directories should resolve below the current workspace root.
    /// 相对 shell 命令工作目录应解析到当前工作区根目录之下。
    #[test]
    fn shell_command_working_directory_joins_relative_path_to_workspace_root() {
        let resolved = resolve_shell_command_working_directory(
            Some(Path::new("D:/workspace")),
            Some("subdir/bin"),
        )
        .expect("relative cwd should resolve");
        assert_eq!(resolved, Path::new("D:/workspace").join("subdir/bin"));
    }

    /// Unknown shell selectors should fail fast before the host attempts to spawn a process.
    /// 未知 shell 选择器应在宿主尝试启动进程前快速失败。
    #[test]
    fn shell_command_launcher_rejects_unknown_shell() {
        let shell_support = test_shell_support();
        let error = resolve_shell_command_launcher(&shell_support, Some("fish"), "echo hello")
            .expect_err("unsupported shell should fail");
        assert!(error.message.contains("不支持"));
    }

    /// The dynamic shell schemas should expose `auto` together with only the shells detected on the current host snapshot.
    /// 动态 shell schema 应暴露 `auto` 以及当前宿主快照检测到的 shell。
    #[test]
    fn shell_tool_schemas_expose_dynamic_detected_shells() {
        let shell_support = test_shell_support();
        let command_schema = shell_command_args_schema(&shell_support);
        let session_schema = shell_session_args_schema(&shell_support);

        assert_eq!(
            command_schema["properties"]["shell"]["enum"],
            json!(["auto", "pwsh", "powershell", "cmd"])
        );
        assert_eq!(
            session_schema["properties"]["shell"]["enum"],
            json!(["auto", "pwsh", "powershell", "cmd"])
        );
    }

    /// The shell-command tool should stay AI-visible in Plan mode so planning turns can still discover and select host command execution.
    /// shell-command 工具应在 Plan 模式下继续对 AI 可见，以便规划回合仍能发现并选择宿主命令执行能力。
    #[test]
    fn shell_command_tool_stays_visible_in_plan_mode() {
        let tool = build_shell_command_tool(test_shell_support());
        assert!(tool.descriptor.ai_visible);
        assert!(tool.descriptor.available_in_plan);
    }

    /// The shell-session tool should stay AI-visible in Plan mode so interactive command workflows are not silently hidden from the model.
    /// shell-session 工具应在 Plan 模式下继续对 AI 可见，避免交互式命令工作流被模型静默隐藏。
    #[test]
    fn shell_session_tool_stays_visible_in_plan_mode() {
        let tool =
            build_shell_session_tool(Arc::new(CommandSessionRuntime::new()), test_shell_support());
        assert!(tool.descriptor.ai_visible);
        assert!(tool.descriptor.available_in_plan);
    }

    /// The shell-command tool should execute one simple command and emit the structured `command_run` host result.
    /// shell-command 工具应执行一条简单命令并发出结构化的 `command_run` 宿主结果。
    #[tokio::test]
    async fn shell_command_tool_executes_simple_command() {
        let tool =
            build_shell_command_tool(HostCommandShellSupport::from_configured_default_id(None));
        let result = tool
            .invoke(NativeToolExecutionRequest {
                arguments: json!({
                    "command": "echo hello"
                }),
                session_id: None,
                workspace_root: None,
                spill_root: None,
            })
            .await
            .expect("shell command should execute");

        let model_output = result
            .model_output
            .as_str()
            .expect("shell command model output should be text");
        assert!(model_output.contains("hello"));
        let host_result = result
            .host_result
            .expect("shell command should expose host metadata");
        assert_eq!(host_result.kind, "command_run");
        assert_eq!(host_result.payload["status"], "completed");
    }

    /// The shell-session tool should support a full start-read-write-read-stop round trip and keep the command-session identifier stable across actions.
    /// shell-session 工具应支持完整的 start-read-write-read-stop 往返，并在各动作间保持稳定的命令会话标识。
    #[tokio::test]
    async fn shell_session_tool_round_trips_stdio_and_output_reads() {
        let tool = build_shell_session_tool(
            Arc::new(CommandSessionRuntime::new()),
            HostCommandShellSupport::from_configured_default_id(None),
        );
        let owner_session_id = Uuid::from_u128(1);
        let start_result = tool
            .invoke(NativeToolExecutionRequest {
                arguments: json!({
                    "action": "start",
                    "command": interactive_shell_session_test_command(),
                    "shell": if cfg!(windows) { "cmd" } else { "auto" }
                }),
                session_id: Some(owner_session_id),
                workspace_root: None,
                spill_root: None,
            })
            .await
            .expect("shell session start should execute");
        let start_host_result = start_result
            .host_result
            .expect("shell session start should expose host metadata");
        assert_eq!(start_host_result.kind, "command_session");
        assert_eq!(start_host_result.payload["action"], "start");
        let command_session_id = start_host_result.payload["command_session_id"]
            .as_str()
            .expect("command session id should exist")
            .to_string();

        let first_read_result = tool
            .invoke(NativeToolExecutionRequest {
                arguments: json!({
                    "action": "read",
                    "command_session_id": command_session_id.clone(),
                    "wait_for_output_ms": 300
                }),
                session_id: Some(owner_session_id),
                workspace_root: None,
                spill_root: None,
            })
            .await
            .expect("shell session read should execute");
        let first_read_output = first_read_result
            .model_output
            .as_str()
            .expect("shell session read should render text");
        assert!(first_read_output.contains("phase-1"));
        let first_read_host_result = first_read_result
            .host_result
            .expect("shell session read should expose host metadata");
        assert!(
            first_read_host_result.payload["returned_chunk_count"]
                .as_u64()
                .unwrap_or_default()
                >= 1,
            "first read payload was: {}",
            first_read_host_result.payload
        );
        let first_last_seq = first_read_host_result.payload["last_seq"]
            .as_u64()
            .expect("first read should expose last sequence");

        let write_result = tool
            .invoke(NativeToolExecutionRequest {
                arguments: json!({
                    "action": "write",
                    "command_session_id": command_session_id.clone(),
                    "stdin": "hello",
                    "append_newline": true
                }),
                session_id: Some(owner_session_id),
                workspace_root: None,
                spill_root: None,
            })
            .await
            .expect("shell session write should execute");
        let write_host_result = write_result
            .host_result
            .expect("shell session write should expose host metadata");
        assert_eq!(
            write_host_result.payload["bytes_written"],
            json!(if cfg!(windows) { 7 } else { 6 })
        );

        let second_read_result = tool
            .invoke(NativeToolExecutionRequest {
                arguments: json!({
                    "action": "read",
                    "command_session_id": command_session_id.clone(),
                    "after_seq": first_last_seq,
                    "wait_for_output_ms": 1_500
                }),
                session_id: Some(owner_session_id),
                workspace_root: None,
                spill_root: None,
            })
            .await
            .expect("shell session second read should execute");
        let second_read_host_result = second_read_result
            .host_result
            .expect("shell session second read should expose host metadata");
        assert!(
            second_read_host_result.payload["returned_chunk_count"]
                .as_u64()
                .unwrap_or_default()
                >= 1
        );
        assert!(
            second_read_host_result.payload["last_seq"]
                .as_u64()
                .unwrap_or_default()
                > first_last_seq
        );

        let stop_result = tool
            .invoke(NativeToolExecutionRequest {
                arguments: json!({
                    "action": "stop",
                    "command_session_id": command_session_id.clone()
                }),
                session_id: Some(owner_session_id),
                workspace_root: None,
                spill_root: None,
            })
            .await
            .expect("shell session stop should execute");
        let stop_host_result = stop_result
            .host_result
            .expect("shell session stop should expose host metadata");
        assert_eq!(stop_host_result.payload["action"], "stop");
        assert_eq!(
            stop_host_result.payload["command_session_id"],
            json!(command_session_id)
        );
    }

    /// The shell-session tool should reject cross-session access and should allow the owner to stop one long-running command session.
    /// shell-session 工具应拒绝跨会话访问，并允许拥有者停止一条长时间运行的命令会话。
    #[tokio::test]
    async fn shell_session_tool_enforces_owner_isolation_and_stop() {
        let tool = build_shell_session_tool(
            Arc::new(CommandSessionRuntime::new()),
            HostCommandShellSupport::from_configured_default_id(None),
        );
        let owner_session_id = Uuid::from_u128(11);
        let foreign_session_id = Uuid::from_u128(22);
        let start_result = tool
            .invoke(NativeToolExecutionRequest {
                arguments: json!({
                    "action": "start",
                    "command": long_running_shell_session_test_command()
                }),
                session_id: Some(owner_session_id),
                workspace_root: None,
                spill_root: None,
            })
            .await
            .expect("long running shell session should start");
        let command_session_id = start_result
            .host_result
            .expect("shell session start should expose host metadata")
            .payload["command_session_id"]
            .as_str()
            .expect("command session id should exist")
            .to_string();

        let foreign_stop_error = tool
            .invoke(NativeToolExecutionRequest {
                arguments: json!({
                    "action": "stop",
                    "command_session_id": command_session_id.clone()
                }),
                session_id: Some(foreign_session_id),
                workspace_root: None,
                spill_root: None,
            })
            .await
            .expect_err("foreign session should not control another session");
        assert!(foreign_stop_error.message.contains("不属于当前控制器会话"));

        let stop_result = tool
            .invoke(NativeToolExecutionRequest {
                arguments: json!({
                    "action": "stop",
                    "command_session_id": command_session_id.clone()
                }),
                session_id: Some(owner_session_id),
                workspace_root: None,
                spill_root: None,
            })
            .await
            .expect("owner should stop the command session");
        let stop_host_result = stop_result
            .host_result
            .expect("stop action should expose host metadata");
        assert_eq!(stop_host_result.kind, "command_session");
        assert_eq!(stop_host_result.payload["status"], "stopped");
    }
}
