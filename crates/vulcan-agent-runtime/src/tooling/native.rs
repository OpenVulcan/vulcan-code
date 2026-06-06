//! Built-in native tools exposed to the host runtime foundation.
//! 暴露给宿主运行时基础架构的内建原生工具。

use crate::agent_runtime::config::{AgentRuntimeConfig, RuntimeAgentKind, RuntimeSkillKind};
use crate::model::SessionId;
use crate::project_temp_maintenance;
use crate::tool_result_format::{
    EffectiveBudgetScope, HostRenderOptions, HostTextRenderInput, HostToolOverflowMode,
    ToolResultBudgetSnapshot, render_tool_result_text,
};
use crate::tooling::command_session_runtime::{
    CommandSessionOutputChunk, CommandSessionRuntime, CommandSessionSnapshot,
    CommandSessionStartRequest, DEFAULT_COMMAND_SESSION_READ_MAX_CHUNKS,
    DEFAULT_COMMAND_SESSION_READ_WAIT_MS, DEFAULT_COMMAND_SESSION_STOP_WAIT_MS,
};
use crate::tooling::output_decode::decode_shell_output_bytes;
use crate::tooling::shell_host::{HostCommandShell, HostCommandShellSupport};
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
use vulcan_prompt_skills::{
    PromptSkillState, PromptSkillsGlobalConfig, PromptSkillsProjectConfig,
    PromptSkillsSessionConfig, render_skill_tool_output, resolve_prompt_skills,
};
use vulcan_tools_manager::{VulcanToolHostResult, VulcanToolInvocationResult};
use vulcan_vmm::{
    GetTurnDetailsRequest, SearchMemoryEventsRequest, TonicVmmClient, VmmClient,
    VmmConfigDiagnostic, VmmConfigLayer, VmmEffectiveConfig, VmmTransportConfig,
    WriteMemoriesRequest, WriteMemoryItem,
};

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
    /// Optional Markdown usage guidance injected into the runtime prompt when the tool is visible.
    /// 工具可见时注入运行时提示词的可选 Markdown 使用说明。
    pub help_text: Option<String>,
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
    /// Global VMM configuration layer.
    /// 全局 VMM 配置层。
    pub vmm_global: VmmConfigLayer,
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
    /// Optional prompt-skill names enabled for the current execution turn.
    /// 当前执行回合启用的可选提示词技能名称集合。
    pub enabled_prompt_skill_names: Vec<String>,
    /// Optional prompt-skill user root captured for the current execution turn.
    /// 当前执行回合捕获的可选提示词技能用户根目录。
    pub prompt_skill_user_root: Option<PathBuf>,
    /// Project-level VMM configuration captured for the current execution turn.
    /// 为当前执行回合捕获的项目级 VMM 配置。
    pub project_vmm: VmmConfigLayer,
    /// Session-level VMM configuration captured for the current execution turn.
    /// 为当前执行回合捕获的会话级 VMM 配置。
    pub session_vmm: VmmConfigLayer,
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
    /// Global VMM configuration layer captured at catalog build time.
    /// 构建目录时捕获的全局 VMM 配置层。
    pub vmm_global: VmmConfigLayer,
}

impl NativeToolCatalog {
    /// Returns the global VMM configuration layer captured when the catalog was built.
    /// 返回构建目录时捕获的全局 VMM 配置层。
    pub fn vmm_global_config(&self) -> &VmmConfigLayer {
        &self.vmm_global
    }

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
                "Native tool `{tool_id}` was not found."
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
    vmm_global: VmmConfigLayer,
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
        vmm_global: vmm_global.clone(),
    };
    let snapshot = Arc::new(snapshot);
    let shell_support =
        HostCommandShellSupport::from_configured_default_id(configured_default_command_shell_id);

    let definitions = vec![
        build_workspace_describe_tool(snapshot.clone()),
        build_agent_list_tool(snapshot.clone(), config),
        build_skill_list_tool(snapshot.clone(), config),
        build_prompt_skill_detail_tool(),
        build_shell_command_tool(shell_support.clone()),
        build_shell_session_tool(command_session_runtime, shell_support),
        build_mcp_server_list_tool(snapshot.clone()),
        build_vmm_memory_search_tool(snapshot.clone()),
        build_vmm_turn_details_tool(snapshot.clone()),
        build_vmm_memory_write_tool(snapshot),
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
        vmm_global,
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
        name: String::from("Workspace Summary"),
        description: String::from(
            "Return the current workspace directory, active model, and basic runtime statistics.",
        ),
        help_text: None,
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
                "kind": match agent.kind {
                    RuntimeAgentKind::Worker => "worker",
                    RuntimeAgentKind::Planner => "planner",
                    RuntimeAgentKind::Explorer => "explorer",
                    RuntimeAgentKind::Specialist => "specialist",
                    RuntimeAgentKind::Coordinator => "coordinator",
                },
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
        name: String::from("List Agents"),
        description: String::from("Return the currently declared agent catalog."),
        help_text: None,
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
                "kind": match skill.kind {
                    RuntimeSkillKind::Regular => "regular",
                    RuntimeSkillKind::LuaSkills => "luaskills",
                    RuntimeSkillKind::Automation => "automation",
                    RuntimeSkillKind::Memory => "memory",
                },
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
        name: String::from("List Skills"),
        description: String::from("Return the currently declared skill catalog."),
        help_text: None,
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

/// Builds the AI-visible prompt-skill detail tool.
/// 构建 AI 可见的提示词技能详情工具。
fn build_prompt_skill_detail_tool() -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from("skill"),
        name: String::from("skill"),
        description: String::from(
            "Load the full SKILL.md instructions for one enabled prompt skill by name. Use this before applying a listed prompt skill.",
        ),
        help_text: None,
        ai_visible: true,
        available_in_plan: true,
    };

    build_native_tool_definition(
        descriptor,
        prompt_skill_detail_args_schema(),
        move |request: NativeToolExecutionRequest| {
            Box::pin(async move { execute_prompt_skill_detail_tool(request).await })
        },
    )
}

/// Builds the one-shot shell-command host-owned native tool.
/// 构建一次性 shell 命令宿主自有原生工具。
fn build_shell_command_tool(shell_support: HostCommandShellSupport) -> Arc<NativeToolDefinition> {
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

/// Builds the session-oriented shell-command host-owned native tool.
/// 构建面向会话的 shell 命令宿主自有原生工具。
fn build_shell_session_tool(
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

/// Builds the MCP-server list host-owned native tool.
/// 构建 MCP 服务器列表宿主自有原生工具。
fn build_mcp_server_list_tool(
    snapshot: Arc<NativeToolRuntimeSnapshot>,
) -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from("native_mcp_list_servers"),
        name: String::from("List MCP Servers"),
        description: String::from("Return the current MCP server configuration and enabled state."),
        help_text: None,
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

/// Builds the VMM memory-search native tool.
/// 构建 VMM 记忆检索原生工具。
fn build_vmm_memory_search_tool(
    snapshot: Arc<NativeToolRuntimeSnapshot>,
) -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from("vmm_memory_search"),
        name: String::from("vmm_memory_search"),
        description: String::from(
            "Search durable VulcanMemoryMesh memories for the current session's configured user and project. Use this when recalled context is insufficient and you need targeted memory lookup.",
        ),
        help_text: None,
        ai_visible: true,
        available_in_plan: true,
    };

    build_native_tool_definition(
        descriptor,
        vmm_memory_search_args_schema(),
        move |request: NativeToolExecutionRequest| {
            let snapshot = snapshot.clone();
            Box::pin(async move { execute_vmm_memory_search_tool(request, snapshot).await })
        },
    )
}

/// Builds the VMM turn-detail native tool.
/// 构建 VMM turn 详情原生工具。
fn build_vmm_turn_details_tool(
    snapshot: Arc<NativeToolRuntimeSnapshot>,
) -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from("vmm_turn_details"),
        name: String::from("vmm_turn_details"),
        description: String::from(
            "Load structured details for one or more VMM turn ids returned by memory search. Use this before relying on a memory that needs original-dialogue verification.",
        ),
        help_text: None,
        ai_visible: true,
        available_in_plan: true,
    };

    build_native_tool_definition(
        descriptor,
        vmm_turn_details_args_schema(),
        move |request: NativeToolExecutionRequest| {
            let snapshot = snapshot.clone();
            Box::pin(async move { execute_vmm_turn_details_tool(request, snapshot).await })
        },
    )
}

/// Builds the VMM direct memory-write native tool.
/// 构建 VMM 主动写记忆原生工具。
fn build_vmm_memory_write_tool(
    snapshot: Arc<NativeToolRuntimeSnapshot>,
) -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from("vmm_memory_write"),
        name: String::from("vmm_memory_write"),
        description: String::from(
            "Write explicit durable memories into VulcanMemoryMesh for the current configured user and project. Use only for stable facts, durable preferences, or user-approved memory candidates.",
        ),
        help_text: Some(vmm_memory_write_usage_help_markdown().to_string()),
        ai_visible: true,
        available_in_plan: false,
    };

    build_native_tool_definition(
        descriptor,
        vmm_memory_write_args_schema(),
        move |request: NativeToolExecutionRequest| {
            let snapshot = snapshot.clone();
            Box::pin(async move { execute_vmm_memory_write_tool(request, snapshot).await })
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

/// One typed argument payload accepted by the host-owned `skill` native tool.
/// 宿主自有 `skill` 原生工具接受的一份强类型参数载荷。
#[derive(Debug, Clone, Deserialize)]
struct PromptSkillDetailToolArgs {
    /// Prompt skill name listed in the runtime prompt-skill catalog.
    /// 运行时技能目录中列出的技能名称。
    name: String,
}

/// One typed argument payload accepted by `vmm_memory_search`.
/// `vmm_memory_search` 接受的一份强类型参数载荷。
#[derive(Debug, Clone, Deserialize)]
struct VmmMemorySearchToolArgs {
    /// Natural-language memory lookup queries.
    /// 自然语言记忆检索查询集合。
    queries: Vec<String>,
    /// Optional maximum hits per query.
    /// 每条查询可选的最大命中数量。
    top_k: Option<u32>,
}

/// One typed argument payload accepted by `vmm_turn_details`.
/// `vmm_turn_details` 接受的一份强类型参数载荷。
#[derive(Debug, Clone, Deserialize)]
struct VmmTurnDetailsToolArgs {
    /// Turn identifiers returned by VMM memory search.
    /// VMM 记忆检索返回的 turn 标识集合。
    #[serde(rename = "turnIds")]
    turn_ids: Vec<u64>,
}

/// One typed direct-memory item accepted by `vmm_memory_write`.
/// `vmm_memory_write` 接收的一条强类型主动记忆项。
#[derive(Debug, Clone, Deserialize)]
struct VmmMemoryWriteToolItemArgs {
    /// Compact memory abstract.
    /// 精简记忆摘要。
    #[serde(rename = "abstract")]
    abstract_text: String,
    /// Full memory details.
    /// 完整记忆详情。
    details: String,
    /// Numeric memory category.
    /// 数字记忆分类。
    category: i32,
    /// Optional VMM scope level.
    /// 可选 VMM 作用域级别。
    #[serde(rename = "scopeLevel")]
    scope_level: Option<i32>,
    /// Optional VMM priority.
    /// 可选 VMM 优先级。
    priority: Option<i32>,
    /// Optional VMM memory level.
    /// 可选 VMM 记忆层级。
    #[serde(rename = "memoryLevel")]
    memory_level: Option<i32>,
}

/// One typed argument payload accepted by `vmm_memory_write`.
/// `vmm_memory_write` 接受的一份强类型参数载荷。
#[derive(Debug, Clone, Deserialize)]
struct VmmMemoryWriteToolArgs {
    /// Direct memory items to persist.
    /// 需要持久化的主动记忆项集合。
    items: Vec<VmmMemoryWriteToolItemArgs>,
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

/// Maximum inline byte budget used before shell tool output switches into page mode spill rendering.
/// shell 工具输出在切换到分页落盘渲染前使用的最大内联字节预算。
const SHELL_INLINE_OUTPUT_LIMIT_BYTES: u64 = 200_000;

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

/// One resolved shell behavior snapshot for a command session, separating the outer launcher shell from the inferred interactive shell.
/// 一份命令会话的已解析 shell 行为快照，用于区分外层启动器 shell 与推断得到的交互 shell。
#[derive(Debug, Clone)]
struct ResolvedSessionShellBehavior {
    /// Stable shell label used for newline semantics and user-facing interaction summaries.
    /// 用于换行语义与面向用户交互摘要的稳定 shell 标签。
    interactive_shell_label: String,
    /// Stable outer launcher shell label selected directly by the host shell resolver.
    /// 由宿主 shell 解析器直接选定的稳定外层启动器 shell 标签。
    launcher_shell_label: String,
    /// Whether the runtime inferred that the command enters a nested interactive shell different from the outer launcher.
    /// 运行时是否推断出该命令进入了一个不同于外层启动器的嵌套交互 shell。
    nested_shell_inferred: bool,
}

/// Builds the English AI-facing description for the one-shot shell command tool.
/// 构建一次性 shell 命令工具面向 AI 的英文描述。
fn shell_command_tool_description() -> String {
    format!(
        "Run a one-shot command in the host environment and return the full decoded stdout/stderr plus the exit status, not just a summary. Default `timeout_ms`: unlimited. If `timeout_ms` is omitted, no timeout is applied and the host waits until the process exits. When the inline result exceeds {SHELL_INLINE_OUTPUT_LIMIT_BYTES} bytes, the host switches to page-mode spill files under the project-local `.vulcan/temp/` directory using names like `tool_output_overflow_page_md_<unix_ms>.md`. After spill, the host preserves the full rendered output instead of truncating it, subject to successful file creation, filesystem limits, and per-line safe-read chunking."
    )
}

/// Builds the English AI-facing description for the session-oriented shell tool.
/// 构建面向会话 shell 工具面向 AI 的英文描述。
fn shell_session_tool_description() -> String {
    format!(
        "Start, read, write to, or stop a long-lived host command session. Reads are polling-based, and `cwd` only affects `start`. The `start` result reports the resolved launcher shell used for `auto` and whether stdin is currently left on an open input line. When an inline read result exceeds {SHELL_INLINE_OUTPUT_LIMIT_BYTES} bytes, the host switches to page-mode spill files under the project-local `.vulcan/temp/` directory using names like `tool_output_overflow_page_md_<unix_ms>.md`. After spill, the host preserves the full rendered output instead of truncating it, subject to successful file creation, filesystem limits, and per-line safe-read chunking."
    )
}

/// Builds the shell-session-specific schema text for the `shell` argument so AI sees the current `auto` target plus nested-shell guidance.
/// 为 `shell_session` 的 `shell` 参数构建专用 schema 文本，使 AI 能看到当前 `auto` 目标及嵌套 shell 指引。
fn shell_session_schema_shell_description(shell_support: &HostCommandShellSupport) -> String {
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

/// Builds the JSON Schema advertised to the model for the host-owned `shell_command` tool.
/// 为宿主自有 `shell_command` 工具构建对模型声明的 JSON Schema。
fn shell_command_args_schema(shell_support: &HostCommandShellSupport) -> Value {
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

/// Builds the JSON Schema advertised to the model for the host-owned `skill` tool.
/// 为宿主自有 `skill` 工具构建对模型声明的 JSON Schema。
fn prompt_skill_detail_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": {
                "type": "string",
                "description": "Exact prompt skill name from the enabled skills list."
            }
        },
        "required": ["name"],
        "additionalProperties": false
    })
}

/// Builds the JSON Schema advertised to the model for `vmm_memory_search`.
/// 为 `vmm_memory_search` 构建对模型声明的 JSON Schema。
fn vmm_memory_search_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "queries": {
                "type": "array",
                "items": { "type": "string" },
                "minItems": 1,
                "description": "One or more concise natural-language memory lookup queries."
            },
            "top_k": {
                "type": "integer",
                "minimum": 1,
                "maximum": 20,
                "description": "Optional maximum hit count per query. Default: 5."
            }
        },
        "required": ["queries"],
        "additionalProperties": false
    })
}

/// Builds the JSON Schema advertised to the model for `vmm_turn_details`.
/// 为 `vmm_turn_details` 构建对模型声明的 JSON Schema。
fn vmm_turn_details_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "turnIds": {
                "type": "array",
                "items": { "type": "integer", "minimum": 1 },
                "minItems": 1,
                "description": "One or more VMM turn ids returned by memory search."
            }
        },
        "required": ["turnIds"],
        "additionalProperties": false
    })
}

/// Returns Markdown usage guidance for the VMM direct memory-write tool.
/// 返回 VMM 主动写记忆工具的 Markdown 使用说明。
fn vmm_memory_write_usage_help_markdown() -> &'static str {
    r#"Use `vmm_memory_write` only for explicit durable memories: stable user preferences, project facts, architecture/API decisions, long-lived requirements, verified bug/debt notes, and user-approved memory candidates. It writes memory rows only; it does not create a turn, timeline, or profile update by itself.

Avoid writing transient current-turn context, duplicate recalled memories, secrets, credentials, or speculative claims. When a memory was retrieved from VMM, do not write it back unless the user corrects, confirms, or materially extends it.

Each `items[]` entry requires:
- `abstract`: compact search-facing summary.
- `details`: complete durable memory body with enough context to stand alone.
- `category`: numeric category: `0 = general`, `1 = architecture_decision`, `2 = tech_spec_api`, `3 = business_logic`, `4 = requirement_todo`, `5 = project_context`, `6 = logical_bug_debt`, `7 = security_policy`.

Optional numeric fields:
- `scopeLevel`: `0 = VMM default`, `1 = session`, `2 = project`, `3 = user`. Prefer `2` for project facts and `3` for stable user preferences.
- `priority`: `0 = VMM default`, `1 = P0/highest`, `2 = P1/ordinary durable`, `3 = P2/weak or tentative`.
- `memoryLevel`: `0 = VMM infer/default`, `1 = L0`, `2 = L1`, `3 = L2`, `4 = L3`. Prefer `2` for ordinary durable project/user memories unless the user or VMM policy indicates otherwise."#
}

/// Builds the JSON Schema advertised to the model for `vmm_memory_write`.
/// 为 `vmm_memory_write` 构建对模型声明的 JSON Schema。
fn vmm_memory_write_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "items": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "properties": {
                        "abstract": {
                            "type": "string",
                            "description": "Compact durable memory abstract."
                        },
                        "details": {
                            "type": "string",
                            "description": "Full durable memory details."
                        },
                        "category": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 7,
                            "description": "Numeric VMM memory category: 0=general, 1=architecture_decision, 2=tech_spec_api, 3=business_logic, 4=requirement_todo, 5=project_context, 6=logical_bug_debt, 7=security_policy."
                        },
                        "scopeLevel": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 3,
                            "description": "Optional VMM scope level: 0=VMM default, 1=session, 2=project, 3=user. Prefer 2 for project facts and 3 for stable user preferences."
                        },
                        "priority": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 3,
                            "description": "Optional VMM priority: 0=VMM default, 1=P0/highest, 2=P1/ordinary durable, 3=P2/weak or tentative."
                        },
                        "memoryLevel": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 4,
                            "description": "Optional VMM memory level: 0=VMM infer/default, 1=L0, 2=L1, 3=L2, 4=L3."
                        }
                    },
                    "required": ["abstract", "details", "category"],
                    "additionalProperties": false
                },
                "description": "Direct memory items to persist."
            }
        },
        "required": ["items"],
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
fn parse_shell_session_tool_arguments(
    arguments: Value,
) -> Result<ShellSessionToolArgs, NativeToolError> {
    serde_json::from_value::<ShellSessionToolArgs>(arguments).map_err(|error| {
        NativeToolError::new(format!("Failed to parse shell_session arguments: {error}"))
    })
}

/// Resolves the controller session that owns the current shell-session tool invocation.
/// 解析拥有当前 shell 会话工具调用的控制器会话。
fn require_shell_session_owner_session_id(
    session_id: Option<SessionId>,
) -> Result<SessionId, NativeToolError> {
    session_id.ok_or_else(|| {
        NativeToolError::new(
            "The shell_session tool requires a stable controller session identifier, but no session_id was attached to this invocation.",
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
        .ok_or_else(|| {
            NativeToolError::new(
                "The shell_session tool is missing the required `command_session_id` argument.",
            )
        })?;
    Uuid::parse_str(raw_command_session_id).map_err(|error| {
        NativeToolError::new(format!(
            "The `command_session_id` provided to shell_session is not a valid UUID: {error}"
        ))
    })
}

/// Returns the shell-specific interactive newline sequence used by session input auto-submission.
/// 返回会话输入自动提交时使用的 shell 特定交互换行序列。
fn shell_specific_newline(shell_label: &str) -> &'static str {
    HostCommandShell::parse(shell_label)
        .map(HostCommandShell::interactive_newline)
        .unwrap_or(if cfg!(windows) { "\r\n" } else { "\n" })
}

/// Normalizes one optional shell selector and falls back to `auto` when the caller omitted it.
/// 规范化一份可选 shell 选择器，并在调用方省略时回退到 `auto`。
fn normalize_shell_selector(raw_shell_selector: Option<&str>) -> &str {
    raw_shell_selector
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("auto")
}

/// Reports whether one shell selector still relies on host-side `auto` resolution.
/// 判断一个 shell 选择器是否仍依赖宿主侧 `auto` 解析。
fn shell_selector_uses_auto(raw_shell_selector: Option<&str>) -> bool {
    normalize_shell_selector(raw_shell_selector).eq_ignore_ascii_case("auto")
}

/// Reports whether one stdin payload already ends with a line terminator.
/// 判断一段标准输入载荷是否已经以行终止符结束。
fn stdin_payload_has_trailing_newline(text: &str) -> bool {
    text.ends_with(['\n', '\r'])
}

/// Resolves the effective newline-submission default for the `start` action.
/// 解析 `start` 动作的有效换行提交默认值。
fn effective_start_append_newline(
    requested_append_newline: Option<bool>,
    initial_stdin: Option<&str>,
) -> bool {
    requested_append_newline.unwrap_or(initial_stdin.is_some())
}

/// Validates whether `start` stdin is safe to submit without hidden concatenation across later writes.
/// 校验 `start` 的标准输入是否能安全提交，避免与后续写入发生隐式拼接。
fn validate_shell_session_start_stdin_policy(
    initial_stdin: Option<&str>,
    append_newline: bool,
) -> Result<(), NativeToolError> {
    if let Some(stdin_text) = initial_stdin {
        if !append_newline && !stdin_payload_has_trailing_newline(stdin_text) {
            return Err(NativeToolError::new(
                "shell_session start rejected initial stdin without a trailing newline. Either set `append_newline: true`, include the trailing newline inside `stdin`, or start without `stdin` and use `write` for partial interactive input.",
            ));
        }
    }
    Ok(())
}

/// Validates whether one requested shell selector may safely rely on `auto` for the resolved session behavior.
/// 校验一条请求的 shell 选择器是否可以安全依赖 `auto` 来驱动当前会话行为。
fn validate_shell_session_start_shell_policy(
    raw_shell_selector: Option<&str>,
    shell_behavior: &ResolvedSessionShellBehavior,
) -> Result<(), NativeToolError> {
    if cfg!(windows)
        && shell_selector_uses_auto(raw_shell_selector)
        && shell_behavior.nested_shell_inferred
        && matches!(
            HostCommandShell::parse(shell_behavior.interactive_shell_label.as_str()),
            Some(HostCommandShell::Bash | HostCommandShell::Sh | HostCommandShell::Zsh)
        )
    {
        return Err(NativeToolError::new(
            "shell_session start requires an explicit `shell` when the command directly launches `bash`, `sh`, or `zsh` on Windows. `auto` resolves only the outer launcher and is rejected here to keep interactive newline behavior deterministic.",
        ));
    }
    Ok(())
}

/// Extracts the leading program token from a raw command string for best-effort nested interactive shell detection.
/// 从原始命令字符串中提取起始程序标记，用于尽力检测嵌套交互 shell。
fn leading_command_program_token(command_text: &str) -> Option<String> {
    command_text
        .trim()
        .split_whitespace()
        .next()
        .map(|token| token.trim_matches(|character| character == '"' || character == '\''))
        .filter(|token| !token.is_empty())
        .map(|token| token.to_string())
}

/// Attempts to infer a nested interactive shell directly launched by the command text.
/// 尝试根据命令文本直接推断被启动的嵌套交互 shell。
fn infer_nested_interactive_shell(command_text: &str) -> Option<HostCommandShell> {
    let token = leading_command_program_token(command_text)?;
    let token = token
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(token.as_str())
        .to_ascii_lowercase();
    let token = token
        .strip_suffix(".exe")
        .or_else(|| token.strip_suffix(".cmd"))
        .or_else(|| token.strip_suffix(".bat"))
        .or_else(|| token.strip_suffix(".com"))
        .unwrap_or(token.as_str());
    HostCommandShell::parse(token)
}

/// Resolves the effective interactive shell behavior for a command session, including nested-shell handoff when it can be inferred safely.
/// 解析命令会话的有效交互 shell 行为，并在能够安全推断时处理嵌套 shell 切换。
fn resolve_session_shell_behavior(
    command_text: &str,
    launcher_shell_label: &str,
) -> ResolvedSessionShellBehavior {
    let nested_shell = infer_nested_interactive_shell(command_text);
    let interactive_shell_label = nested_shell
        .map(HostCommandShell::id)
        .unwrap_or(launcher_shell_label)
        .to_string();
    let nested_shell_inferred = interactive_shell_label != launcher_shell_label;

    ResolvedSessionShellBehavior {
        interactive_shell_label,
        launcher_shell_label: launcher_shell_label.to_string(),
        nested_shell_inferred,
    }
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
    ];
    if snapshot.launcher_shell_label != snapshot.shell_label {
        lines.push(format!("Launcher shell: {}", snapshot.launcher_shell_label));
    }
    lines.extend([
        format!("Working directory: {}", snapshot.cwd.display()),
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

/// Parses one raw model payload into the typed shell-command argument structure.
/// 将一份原始模型载荷解析为强类型 shell 命令参数结构。
fn parse_shell_command_tool_arguments(
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

/// Parses one raw model payload into the typed prompt-skill argument structure.
/// 将一份原始模型载荷解析为强类型提示词技能参数结构。
fn parse_prompt_skill_detail_tool_arguments(
    arguments: Value,
) -> Result<PromptSkillDetailToolArgs, NativeToolError> {
    let parsed =
        serde_json::from_value::<PromptSkillDetailToolArgs>(arguments).map_err(|error| {
            NativeToolError::new(format!("Failed to parse skill arguments: {error}"))
        })?;
    if parsed.name.trim().is_empty() {
        return Err(NativeToolError::new(
            "The `name` argument for skill cannot be empty.",
        ));
    }
    Ok(parsed)
}

/// Parses and validates VMM memory-search arguments.
/// 解析并校验 VMM 记忆检索参数。
fn parse_vmm_memory_search_arguments(
    arguments: Value,
) -> Result<VmmMemorySearchToolArgs, NativeToolError> {
    let mut parsed =
        serde_json::from_value::<VmmMemorySearchToolArgs>(arguments).map_err(|error| {
            NativeToolError::new(format!(
                "Failed to parse vmm_memory_search arguments: {error}"
            ))
        })?;
    parsed.queries = parsed
        .queries
        .into_iter()
        .map(|query| query.trim().to_string())
        .filter(|query| !query.is_empty())
        .collect();
    if parsed.queries.is_empty() {
        return Err(NativeToolError::new(
            "The `queries` argument for vmm_memory_search cannot be empty.",
        ));
    }
    parsed.top_k = Some(parsed.top_k.unwrap_or(5).clamp(1, 20));
    Ok(parsed)
}

/// Parses and validates VMM turn-detail arguments.
/// 解析并校验 VMM turn 详情参数。
fn parse_vmm_turn_details_arguments(
    arguments: Value,
) -> Result<VmmTurnDetailsToolArgs, NativeToolError> {
    let parsed = serde_json::from_value::<VmmTurnDetailsToolArgs>(arguments).map_err(|error| {
        NativeToolError::new(format!(
            "Failed to parse vmm_turn_details arguments: {error}"
        ))
    })?;
    if parsed.turn_ids.is_empty() || parsed.turn_ids.iter().any(|turn_id| *turn_id == 0) {
        return Err(NativeToolError::new(
            "The `turnIds` argument for vmm_turn_details must contain positive turn ids.",
        ));
    }
    Ok(parsed)
}

/// Parses and validates VMM direct memory-write arguments.
/// 解析并校验 VMM 主动写记忆参数。
fn parse_vmm_memory_write_arguments(
    arguments: Value,
) -> Result<VmmMemoryWriteToolArgs, NativeToolError> {
    let parsed = serde_json::from_value::<VmmMemoryWriteToolArgs>(arguments).map_err(|error| {
        NativeToolError::new(format!(
            "Failed to parse vmm_memory_write arguments: {error}"
        ))
    })?;
    if parsed.items.is_empty() {
        return Err(NativeToolError::new(
            "The `items` argument for vmm_memory_write cannot be empty.",
        ));
    }
    for item in &parsed.items {
        if item.abstract_text.trim().is_empty() || item.details.trim().is_empty() {
            return Err(NativeToolError::new(
                "`abstract` and `details` are required for every vmm_memory_write item.",
            ));
        }
    }
    Ok(parsed)
}

/// Resolves the effective VMM config for one native tool invocation.
/// 为一次原生工具调用解析最终 VMM 配置。
fn effective_vmm_config_for_native_request(
    request: &NativeToolExecutionRequest,
    snapshot: &NativeToolRuntimeSnapshot,
) -> Result<VmmEffectiveConfig, NativeToolError> {
    let config = VmmEffectiveConfig::resolve(
        &snapshot.vmm_global,
        Some(&request.project_vmm),
        Some(&request.session_vmm),
    );
    if !config.ai_tools_enabled {
        return Err(NativeToolError::new(
            "VMM AI tools are disabled for the current project or session.",
        ));
    }
    match config.diagnose_memory_flow(true) {
        VmmConfigDiagnostic::Ready => Ok(config),
        diagnostic => Err(NativeToolError::new(format!(
            "VMM memory tools are not ready for the current session: {diagnostic:?}"
        ))),
    }
}

/// Connects a VMM client for one native tool invocation.
/// 为一次原生工具调用连接 VMM 客户端。
async fn connect_vmm_native_client(
    config: &VmmEffectiveConfig,
) -> Result<TonicVmmClient, NativeToolError> {
    TonicVmmClient::connect(VmmTransportConfig::from_effective(config))
        .await
        .map_err(|error| NativeToolError::new(error.to_string()))
}

/// Samples support files under one prompt-skill directory without reading their contents.
/// 在单个提示词技能目录下采样支持文件路径但不读取文件内容。
fn sample_prompt_skill_support_files(base_dir: &Path, skill_file: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_prompt_skill_support_files(base_dir, skill_file, base_dir, 0, &mut files);
    files.sort();
    files.truncate(12);
    files
}

/// Recursively collects bounded support-file paths for the prompt-skill detail output.
/// 为提示词技能详情输出递归收集有界支持文件路径。
fn collect_prompt_skill_support_files(
    root: &Path,
    skill_file: &Path,
    current: &Path,
    depth: usize,
    files: &mut Vec<PathBuf>,
) {
    if depth > 2 || files.len() >= 32 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(current) else {
        return;
    };
    for entry in entries.flatten() {
        if files.len() >= 32 {
            return;
        }
        let path = entry.path();
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if file_name.starts_with('.') || file_name.eq_ignore_ascii_case("target") {
            continue;
        }
        if path == skill_file {
            continue;
        }
        if path.is_dir() {
            collect_prompt_skill_support_files(root, skill_file, &path, depth + 1, files);
        } else if path.is_file() {
            files.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
        }
    }
}

/// Resolves the effective working directory for one shell-command invocation.
/// 为单次 shell 命令调用解析实际工作目录。
fn resolve_shell_command_working_directory(
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
    validate_shell_session_start_shell_policy(parsed_args.shell.as_deref(), &shell_behavior)?;
    let append_newline =
        effective_start_append_newline(parsed_args.append_newline, parsed_args.stdin.as_deref());
    let initial_stdin = parsed_args.stdin.clone();
    validate_shell_session_start_stdin_policy(initial_stdin.as_deref(), append_newline)?;
    let snapshot = command_session_runtime
        .start_session(CommandSessionStartRequest {
            owner_session_id,
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

/// Executes the host-owned `skill` tool and returns the full prompt-skill body.
/// 执行宿主自有 `skill` 工具并返回完整提示词技能正文。
async fn execute_prompt_skill_detail_tool(
    request: NativeToolExecutionRequest,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let parsed_args = parse_prompt_skill_detail_tool_arguments(request.arguments)?;
    let requested_name = parsed_args.name.trim();
    // The turn-level enabled list is authoritative, so disabled skills cannot be loaded by name.
    // 本回合启用列表是最终约束，因此已关闭技能不能通过名称绕过加载。
    if !request
        .enabled_prompt_skill_names
        .iter()
        .any(|name| name.eq_ignore_ascii_case(requested_name))
    {
        return Err(NativeToolError::new(format!(
            "Prompt skill `{requested_name}` is not enabled for this execution turn."
        )));
    }
    let config = PromptSkillsGlobalConfig {
        enabled: true,
        user_skill_root: request.prompt_skill_user_root.clone(),
        default_skill_state: PromptSkillState::Enabled,
        skill_overrides: BTreeMap::new(),
    };
    let catalog = vulcan_prompt_skills::load_prompt_skills(&config);
    let resolved = resolve_prompt_skills(
        &catalog,
        &config,
        &PromptSkillsProjectConfig::default(),
        &PromptSkillsSessionConfig::default(),
    );
    let Some(skill) = resolved
        .iter()
        .find(|skill| {
            skill.state == PromptSkillState::Enabled
                && skill.skill.name.eq_ignore_ascii_case(requested_name)
        })
        .map(|resolved| &resolved.skill)
    else {
        let available = resolved
            .iter()
            .filter(|skill| skill.state == PromptSkillState::Enabled)
            .map(|skill| skill.skill.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let suffix = if available.is_empty() {
            String::from("No enabled prompt skills were found.")
        } else {
            format!("Enabled prompt skills: {available}.")
        };
        return Err(NativeToolError::new(format!(
            "Prompt skill `{}` is not enabled or was not found. {suffix}",
            parsed_args.name.trim()
        )));
    };

    let sampled_files = sample_prompt_skill_support_files(&skill.base_dir, &skill.skill_file);
    Ok(VulcanToolInvocationResult::from_model_output(
        Value::String(render_skill_tool_output(skill, &sampled_files)),
    ))
}

/// Executes the host-owned `vmm_memory_search` tool.
/// 执行宿主自有 `vmm_memory_search` 工具。
async fn execute_vmm_memory_search_tool(
    request: NativeToolExecutionRequest,
    snapshot: Arc<NativeToolRuntimeSnapshot>,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let parsed_args = parse_vmm_memory_search_arguments(request.arguments.clone())?;
    let config = effective_vmm_config_for_native_request(&request, &snapshot)?;
    let client = connect_vmm_native_client(&config).await?;
    let response = client
        .search_memory_events(SearchMemoryEventsRequest {
            user_id: config.user_id.unwrap_or_default(),
            project_id: config.project_id.unwrap_or_default(),
            queries: parsed_args.queries,
            top_k: parsed_args.top_k.unwrap_or(5),
        })
        .await
        .map_err(|error| NativeToolError::new(error.to_string()))?;

    Ok(VulcanToolInvocationResult::from_model_output(json!({
        "trace_id": response.trace_id,
        "query_count": response.results.len(),
        "results": response.results.into_iter().map(|group| {
            json!({
                "query_index": group.query_index,
                "query": group.query,
                "hit_count": group.hits.len(),
                "hits": group.hits.into_iter().map(|hit| json!({
                    "memory_id": hit.memory_id,
                    "source_turn_id": hit.source_turn_id,
                    "has_source_turn": hit.source_turn_id != 0,
                    "abstract": hit.abstract_text,
                    "details_preview": hit.details_preview,
                    "category": hit.category,
                    "created_datetime": hit.created_datetime,
                })).collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>(),
    })))
}

/// Executes the host-owned `vmm_turn_details` tool.
/// 执行宿主自有 `vmm_turn_details` 工具。
async fn execute_vmm_turn_details_tool(
    request: NativeToolExecutionRequest,
    snapshot: Arc<NativeToolRuntimeSnapshot>,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let parsed_args = parse_vmm_turn_details_arguments(request.arguments.clone())?;
    let config = effective_vmm_config_for_native_request(&request, &snapshot)?;
    let client = connect_vmm_native_client(&config).await?;
    let response = client
        .get_turn_details(GetTurnDetailsRequest {
            turn_ids: parsed_args.turn_ids,
        })
        .await
        .map_err(|error| NativeToolError::new(error.to_string()))?;

    Ok(VulcanToolInvocationResult::from_model_output(json!({
        "trace_id": response.trace_id,
        "turn_count": response.turns.len(),
        "turns": response.turns.into_iter().map(|turn| json!({
            "turn_id": turn.turn_id,
            "user_question": turn.user_question,
            "timeline": turn.timeline.into_iter().map(|item| json!({
                "type": item.kind.as_wire_type(),
                "content": item.content,
            })).collect::<Vec<_>>(),
            "assistant_answer": turn.assistant_answer,
            "detail": turn.detail,
            "previous_turn_ids": turn.previous_turn_ids,
            "next_turn_ids": turn.next_turn_ids,
        })).collect::<Vec<_>>(),
    })))
}

/// Executes the host-owned `vmm_memory_write` tool.
/// 执行宿主自有 `vmm_memory_write` 工具。
async fn execute_vmm_memory_write_tool(
    request: NativeToolExecutionRequest,
    snapshot: Arc<NativeToolRuntimeSnapshot>,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let parsed_args = parse_vmm_memory_write_arguments(request.arguments.clone())?;
    let session_id = request
        .session_id
        .map(|session_id| session_id.to_string())
        .filter(|session_id| !session_id.trim().is_empty())
        .ok_or_else(|| {
            NativeToolError::new(
                "vmm_memory_write requires a real session id for durable attribution.",
            )
        })?;
    let config = effective_vmm_config_for_native_request(&request, &snapshot)?;
    let client = connect_vmm_native_client(&config).await?;
    let response = client
        .write_memories(WriteMemoriesRequest {
            session_id,
            user_id: config.user_id.unwrap_or_default(),
            project_id: config.project_id.unwrap_or_default(),
            items: parsed_args
                .items
                .into_iter()
                .map(|item| WriteMemoryItem {
                    scope_level: item.scope_level.unwrap_or(0),
                    abstract_text: item.abstract_text.trim().to_string(),
                    details: item.details.trim().to_string(),
                    category: item.category,
                    priority: item.priority.unwrap_or(0),
                    memory_level: item.memory_level.unwrap_or(0),
                })
                .collect(),
        })
        .await
        .map_err(|error| NativeToolError::new(error.to_string()))?;

    Ok(VulcanToolInvocationResult::from_model_output(json!({
        "trace_id": response.trace_id,
        "item_count": response.items.len(),
        "items": response.items.into_iter().map(|item| json!({
            "memory_id": item.memory_id,
            "deduped": item.deduped,
        })).collect::<Vec<_>>(),
    })))
}

#[cfg(test)]
mod tests {
    use super::{
        NativeToolExecutionRequest, build_agent_list_tool, build_mcp_server_list_tool,
        build_shell_command_tool, build_shell_session_tool, build_skill_list_tool,
        build_workspace_describe_tool, effective_start_append_newline,
        resolve_session_shell_behavior, resolve_shell_command_launcher,
        resolve_shell_command_working_directory, shell_command_args_schema,
        shell_session_args_schema, shell_specific_newline,
        validate_shell_session_start_shell_policy, validate_shell_session_start_stdin_policy,
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
    use vulcan_vmm::VmmConfigLayer;

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
            workspace_directory: String::from(r"D:\workspace"),
            active_model: Some(String::from("openai/gpt-5")),
            agent_count: 1,
            skill_count: 1,
            configured_mcp_server_ids: vec![String::from("server-a")],
            enabled_mcp_server_ids: vec![String::from("server-a")],
            vmm_global: VmmConfigLayer::default(),
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
                runtime_policy: vulcan_session_core::ExecutionRuntimePolicyRequest::default(),
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
                enabled_prompt_skill_names: Vec::new(),
                prompt_skill_user_root: None,
                project_vmm: VmmConfigLayer::default(),
                session_vmm: VmmConfigLayer::default(),
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
            workspace_directory: String::from(r"D:\workspace"),
            active_model: Some(String::from("openai/gpt-5")),
            agent_count: 0,
            skill_count: 0,
            configured_mcp_server_ids: vec![String::from("server-a")],
            enabled_mcp_server_ids: vec![String::from("server-a")],
            vmm_global: VmmConfigLayer::default(),
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
            workspace_directory: String::from(r"D:\workspace"),
            active_model: Some(String::from("openai/gpt-5")),
            agent_count: 1,
            skill_count: 1,
            configured_mcp_server_ids: vec![],
            enabled_mcp_server_ids: vec![],
            vmm_global: VmmConfigLayer::default(),
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
                enabled_prompt_skill_names: Vec::new(),
                prompt_skill_user_root: None,
                project_vmm: VmmConfigLayer::default(),
                session_vmm: VmmConfigLayer::default(),
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
            Some(Path::new(r"D:\workspace")),
            Some("subdir/bin"),
        )
        .expect("relative cwd should resolve");
        assert_eq!(resolved, Path::new(r"D:\workspace").join("subdir/bin"));
    }

    /// Omitted shell-command working directories should default to the current workspace root instead of the host process directory.
    /// 省略 shell 命令工作目录时，应默认使用当前工作区根目录，而不是宿主进程目录。
    #[test]
    fn shell_command_working_directory_defaults_to_workspace_root() {
        let resolved =
            resolve_shell_command_working_directory(Some(Path::new(r"D:\workspace")), None)
                .expect("workspace-root default should resolve");
        assert_eq!(resolved, Path::new(r"D:\workspace"));
    }

    /// Shell invocations without a workspace root should still fall back to the host process working directory so non-project commands remain usable.
    /// 缺少工作区根目录的 shell 调用仍应回退到宿主进程工作目录，以便非项目命令继续可用。
    #[test]
    fn shell_command_working_directory_falls_back_without_workspace_root() {
        let resolved = resolve_shell_command_working_directory(None, None)
            .expect("missing workspace root should fall back");
        assert_eq!(
            resolved,
            std::env::current_dir().expect("current directory should resolve")
        );
    }

    /// Relative shell-command working directories should fall back to the host process working directory when no workspace root is attached.
    /// 当没有附着工作区根目录时，相对 shell 命令工作目录应回退到宿主进程工作目录。
    #[test]
    fn shell_command_working_directory_resolves_relative_cwd_without_workspace_root() {
        let resolved = resolve_shell_command_working_directory(None, Some("subdir/bin"))
            .expect("relative cwd without workspace root should resolve");
        assert_eq!(
            resolved,
            std::env::current_dir()
                .expect("current directory should resolve")
                .join("subdir/bin")
        );
    }

    /// Absolute shell-command working directories should remain usable even when no workspace root is attached to the turn.
    /// 即使当前回合没有附着工作区根目录，绝对 shell 命令工作目录也应继续可用。
    #[test]
    fn shell_command_working_directory_accepts_absolute_cwd_without_workspace_root() {
        let absolute_cwd = if cfg!(windows) {
            r"D:\workspace\subdir"
        } else {
            "/tmp/workspace/subdir"
        };
        let resolved = resolve_shell_command_working_directory(None, Some(absolute_cwd))
            .expect("absolute cwd should resolve without workspace root");
        assert_eq!(resolved, Path::new(absolute_cwd));
    }

    /// Unknown shell selectors should fail fast before the host attempts to spawn a process.
    /// 未知 shell 选择器应在宿主尝试启动进程前快速失败。
    #[test]
    fn shell_command_launcher_rejects_unknown_shell() {
        let shell_support = test_shell_support();
        let error = resolve_shell_command_launcher(&shell_support, Some("fish"), "echo hello")
            .expect_err("unsupported shell should fail");
        assert!(error.message.contains("unsupported"));
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

    /// The AI-facing shell schema text should stay in English and should document critical defaults and paging behavior explicitly.
    /// 面向 AI 的 shell schema 文案应保持英文，并明确说明关键默认值与分页行为。
    #[test]
    fn shell_tool_schema_descriptions_document_defaults_in_english() {
        let shell_support = test_shell_support();
        let command_tool = build_shell_command_tool(shell_support.clone());
        let session_tool = build_shell_session_tool(
            Arc::new(CommandSessionRuntime::new()),
            shell_support.clone(),
        );
        let command_schema = shell_command_args_schema(&shell_support);
        let session_schema = shell_session_args_schema(&shell_support);

        assert!(
            command_tool
                .descriptor
                .description
                .starts_with("Run a one-shot command")
        );
        assert!(
            session_tool
                .descriptor
                .description
                .starts_with("Start, read, write to, or stop")
        );
        assert!(
            command_tool
                .descriptor
                .description
                .contains("no timeout is applied")
        );
        assert!(
            command_tool
                .descriptor
                .description
                .contains("not just a summary")
        );
        assert!(
            command_tool
                .descriptor
                .description
                .contains(".vulcan/temp/")
        );
        assert!(
            session_tool
                .descriptor
                .description
                .contains("tool_output_overflow_page_md_")
        );
        assert!(
            command_schema["properties"]["timeout_ms"]["description"]
                .as_str()
                .unwrap_or_default()
                .contains("Default: unlimited")
        );
        assert!(
            command_schema["properties"]["cwd"]["description"]
                .as_str()
                .unwrap_or_default()
                .contains("falls back to the host process working directory")
        );
        assert!(
            command_schema["properties"]["cwd"]["description"]
                .as_str()
                .unwrap_or_default()
                .contains("Relative paths resolve against the same base directory")
        );
        assert!(
            command_schema["properties"]["stdin"]["description"]
                .as_str()
                .unwrap_or_default()
                .contains("No newline is added automatically")
        );
        assert!(
            session_schema["properties"]["wait_for_output_ms"]["description"]
                .as_str()
                .unwrap_or_default()
                .contains("Default: 0")
        );
        assert!(
            session_schema["properties"]["max_chunks"]["description"]
                .as_str()
                .unwrap_or_default()
                .contains("Default: 128")
        );
        assert!(
            session_schema["properties"]["max_chunks"]["description"]
                .as_str()
                .unwrap_or_default()
                .contains("coalesces adjacent undelivered fragments")
        );
        assert!(
            session_schema["properties"]["append_newline"]["description"]
                .as_str()
                .unwrap_or_default()
                .contains("resolved launcher shell")
        );
        assert!(
            session_schema["properties"]["cwd"]["description"]
                .as_str()
                .unwrap_or_default()
                .contains("falls back to the host process working directory")
        );
        if cfg!(windows) {
            assert!(
                session_schema["properties"]["shell"]["description"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("rejects `shell: auto`")
            );
        }
        assert!(
            session_schema["properties"]["stdin"]["description"]
                .as_str()
                .unwrap_or_default()
                .contains("hidden concatenation")
        );
    }

    /// The VMM write tool should expose category mappings through schema text and registration help.
    /// VMM 写入工具应通过 schema 文案与注册说明暴露分类映射。
    #[test]
    fn vmm_memory_write_tool_documents_numeric_mappings() {
        let snapshot = Arc::new(super::NativeToolRuntimeSnapshot {
            workspace_directory: String::from(r"D:\workspace"),
            active_model: Some(String::from("openai/gpt-5")),
            agent_count: 0,
            skill_count: 0,
            configured_mcp_server_ids: vec![],
            enabled_mcp_server_ids: vec![],
            vmm_global: VmmConfigLayer::default(),
        });
        let tool = super::build_vmm_memory_write_tool(snapshot);
        let help_text = tool
            .descriptor
            .help_text
            .as_deref()
            .expect("vmm write tool should expose prompt help");
        let schema = super::vmm_memory_write_args_schema();
        let item_properties = &schema["properties"]["items"]["items"]["properties"];

        assert!(help_text.contains("0 = general"));
        assert!(help_text.contains("5 = project_context"));
        assert!(help_text.contains("7 = security_policy"));
        assert!(help_text.contains("2 = P1/ordinary durable"));
        assert_eq!(item_properties["category"]["maximum"], json!(7));
        assert_eq!(item_properties["priority"]["maximum"], json!(3));
        assert_eq!(item_properties["memoryLevel"]["maximum"], json!(4));
        assert!(
            item_properties["category"]["description"]
                .as_str()
                .unwrap_or_default()
                .contains("1=architecture_decision")
        );
    }

    /// The start action should default newline submission on when initial stdin is provided so complete commands are safer by default.
    /// start 动作在提供初始标准输入时应默认开启换行提交，以便默认情况下更安全地发送完整命令。
    #[test]
    fn shell_session_start_defaults_append_newline_when_stdin_exists() {
        assert!(effective_start_append_newline(None, Some("echo hi")));
        assert!(!effective_start_append_newline(None, None));
        assert!(!effective_start_append_newline(
            Some(false),
            Some("echo hi")
        ));
        assert!(effective_start_append_newline(Some(true), None));
    }

    /// The start action should reject newline-less initial stdin unless the caller explicitly submits a newline in-band or via append_newline.
    /// start 动作应拒绝未闭合的初始标准输入，除非调用方通过文本本身或 append_newline 显式提交换行。
    #[test]
    fn shell_session_start_rejects_hidden_stdin_concatenation() {
        let error = validate_shell_session_start_stdin_policy(Some("echo init"), false)
            .expect_err("newline-less initial stdin should be rejected");
        assert!(error.message.contains("rejected initial stdin"));
        validate_shell_session_start_stdin_policy(Some("echo init\n"), false)
            .expect("trailing newline inside stdin should pass");
        validate_shell_session_start_stdin_policy(Some("echo init"), true)
            .expect("append_newline should make the input safe");
    }

    /// Windows shell sessions should reject `auto` when the command directly enters a nested Unix shell because the outer launcher is not deterministic enough.
    /// Windows shell 会话在命令直接进入嵌套 Unix shell 时应拒绝 `auto`，因为外层启动器行为不够确定。
    #[test]
    fn shell_session_start_rejects_windows_auto_for_nested_unix_shells() {
        let bash_behavior = resolve_session_shell_behavior("bash", "pwsh");
        if cfg!(windows) {
            let error = validate_shell_session_start_shell_policy(Some("auto"), &bash_behavior)
                .expect_err("windows auto nested bash should be rejected");
            assert!(error.message.contains("requires an explicit `shell`"));
        } else {
            validate_shell_session_start_shell_policy(Some("auto"), &bash_behavior)
                .expect("non-windows hosts should keep current auto behavior");
        }
        validate_shell_session_start_shell_policy(Some("bash"), &bash_behavior)
            .expect("explicit bash should remain valid");
    }

    /// Bash-like shells should always receive LF for automatic line submission, even on Windows hosts.
    /// 类 Bash shell 在自动提交输入行时始终应接收 LF，即使宿主是 Windows。
    #[test]
    fn shell_specific_newline_uses_lf_for_unix_shell_labels() {
        assert_eq!(shell_specific_newline("bash"), "\n");
        assert_eq!(shell_specific_newline("sh"), "\n");
        assert_eq!(shell_specific_newline("zsh"), "\n");
        assert_eq!(
            shell_specific_newline("cmd"),
            if cfg!(windows) { "\r\n" } else { "\n" }
        );
    }

    /// Directly launching a nested interactive Unix shell should switch newline semantics away from the outer auto-resolved Windows launcher.
    /// 直接启动嵌套交互式 Unix shell 时，应把换行语义从外层自动解析的 Windows 启动器切换开。
    #[test]
    fn session_shell_behavior_detects_nested_bash_from_command_text() {
        let behavior = resolve_session_shell_behavior("bash", "pwsh");

        assert_eq!(behavior.launcher_shell_label, "pwsh");
        assert_eq!(behavior.interactive_shell_label, "bash");
        assert!(behavior.nested_shell_inferred);
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
                enabled_prompt_skill_names: Vec::new(),
                prompt_skill_user_root: None,
                project_vmm: VmmConfigLayer::default(),
                session_vmm: VmmConfigLayer::default(),
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
                enabled_prompt_skill_names: Vec::new(),
                prompt_skill_user_root: None,
                project_vmm: VmmConfigLayer::default(),
                session_vmm: VmmConfigLayer::default(),
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
                enabled_prompt_skill_names: Vec::new(),
                prompt_skill_user_root: None,
                project_vmm: VmmConfigLayer::default(),
                session_vmm: VmmConfigLayer::default(),
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
                enabled_prompt_skill_names: Vec::new(),
                prompt_skill_user_root: None,
                project_vmm: VmmConfigLayer::default(),
                session_vmm: VmmConfigLayer::default(),
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
                enabled_prompt_skill_names: Vec::new(),
                prompt_skill_user_root: None,
                project_vmm: VmmConfigLayer::default(),
                session_vmm: VmmConfigLayer::default(),
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
                enabled_prompt_skill_names: Vec::new(),
                prompt_skill_user_root: None,
                project_vmm: VmmConfigLayer::default(),
                session_vmm: VmmConfigLayer::default(),
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
                enabled_prompt_skill_names: Vec::new(),
                prompt_skill_user_root: None,
                project_vmm: VmmConfigLayer::default(),
                session_vmm: VmmConfigLayer::default(),
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
                enabled_prompt_skill_names: Vec::new(),
                prompt_skill_user_root: None,
                project_vmm: VmmConfigLayer::default(),
                session_vmm: VmmConfigLayer::default(),
            })
            .await
            .expect_err("foreign session should not control another session");
        assert!(
            foreign_stop_error
                .message
                .contains("does not belong to the current controller session")
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
                enabled_prompt_skill_names: Vec::new(),
                prompt_skill_user_root: None,
                project_vmm: VmmConfigLayer::default(),
                session_vmm: VmmConfigLayer::default(),
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
