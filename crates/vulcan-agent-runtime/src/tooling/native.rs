//! Built-in native tools exposed to the host runtime foundation.
//! 暴露给宿主运行时基础架构的内建原生工具。

use crate::agent_runtime::config::{AgentRuntimeConfig, RuntimeAgentKind, RuntimeSkillKind};
use crate::model::SessionId;
use crate::project_temp_maintenance;
use crate::tool_result_format::{
    EffectiveBudgetScope, HostRenderOptions, HostTextRenderInput, HostToolOverflowMode,
    ToolResultBudgetSnapshot, render_tool_result_text,
};
#[cfg(test)]
use crate::tooling::command_session_runtime::DEFAULT_COMMAND_SESSION_READ_WAIT_MS;
use crate::tooling::command_session_runtime::{
    CommandSessionBackend, CommandSessionInputEvent, CommandSessionOutputChunk,
    CommandSessionReadOutcome, CommandSessionRuntime, CommandSessionSnapshot,
    CommandSessionStartRequest, DEFAULT_COMMAND_SESSION_READ_MAX_CHUNKS,
    DEFAULT_COMMAND_SESSION_STOP_WAIT_MS,
};
use crate::tooling::output_decode::decode_shell_output_bytes;
use crate::tooling::shell_host::{HostCommandShell, HostCommandShellSupport};
use crate::workspace_path::normalized_workspace_path_text_from_path;
use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::fs as tokio_fs;
use tokio::io::AsyncWriteExt;
use tokio::process::Command as TokioCommand;
use tokio::time::{Duration, Instant, timeout};
use uuid::Uuid;
use vulcan_prompt_skills::{
    PromptSkillState, PromptSkillsGlobalConfig, PromptSkillsProjectConfig,
    PromptSkillsSessionConfig, render_skill_tool_output, resolve_prompt_skills,
};
use vulcan_tools_manager::{VulcanToolHostResult, VulcanToolInvocationResult};
use vulcan_vmm::{
    DeleteMemoriesRequest, GetTurnDetailsRequest, SearchMemoryEventsRequest, TonicVmmClient,
    VmmClient, VmmConfigDiagnostic, VmmConfigLayer, VmmEffectiveConfig, VmmTransportConfig,
    WriteMemoriesRequest, WriteMemoryItem,
};
use vulcan_workspace_store::{
    data_plane::ControllerRuntimeHandle,
    workspace_store::{
        GoalAccountingMode, GoalUpdate, ThreadGoal, ThreadGoalStatus, VulcanGoalStore,
    },
};

/// Stable identifier for the universal session todolist update tool.
/// 通用会话 todolist 更新工具的稳定标识。
pub const TODOLIST_TOOL_ID: &str = "todolist";

/// Stable identifier for the Plan-mode plan artifact submission tool.
/// 计划模式计划产物提交工具的稳定标识。
pub const SUBMIT_PLAN_TOOL_ID: &str = "submit_plan";

/// Stable identifier for the current-thread goal read tool.
/// 当前线程目标读取工具的稳定标识。
pub const GET_GOAL_TOOL_ID: &str = "get_goal";

/// Stable identifier for the current-thread goal creation tool.
/// 当前线程目标创建工具的稳定标识。
pub const CREATE_GOAL_TOOL_ID: &str = "create_goal";

/// Stable identifier for the current-thread goal status update tool.
/// 当前线程目标状态更新工具的稳定标识。
pub const UPDATE_GOAL_TOOL_ID: &str = "update_goal";

/// Quiet window used after Ctrl+C before the terminal is considered drained.
/// Ctrl+C 后认为终端排空所需的静默窗口。
const TERMINAL_INTERRUPT_DRAIN_QUIET_MS: u64 = 250;

/// Maximum total wait used to consume residual Ctrl+C terminal output.
/// 用于消费 Ctrl+C 残留终端输出的最大总等待时间。
const TERMINAL_INTERRUPT_DRAIN_MAX_MS: u64 = 1_500;

/// Soft byte limit for coalescing adjacent model-visible terminal chunks.
/// 合并相邻模型可见终端分片时使用的软字节限制。
const TERMINAL_OUTPUT_CHUNK_COALESCE_SOFT_LIMIT_BYTES: usize = 2_048;

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
    /// Whether the tool may execute while Plan mode is active.
    /// 当前工具是否允许在计划模式激活时执行。
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
    /// Optional controller runtime used by native tools that persist workspace state.
    /// 持久化工作区状态的原生工具使用的可选控制器运行时。
    pub controller_runtime: Option<ControllerRuntimeHandle>,
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
            .map(|path| normalized_workspace_path_text_from_path(&path))
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
        build_todolist_tool(),
        build_submit_plan_tool(),
        build_get_goal_tool(),
        build_create_goal_tool(),
        build_update_goal_tool(),
        build_prompt_skill_detail_tool(),
        build_shell_command_tool(shell_support.clone()),
        build_terminal_session_tool(command_session_runtime, shell_support),
        build_mcp_server_list_tool(snapshot.clone()),
        build_vmm_memory_search_tool(snapshot.clone()),
        build_vmm_turn_details_tool(snapshot.clone()),
        build_vmm_memory_write_tool(snapshot.clone()),
        build_vmm_memory_delete_tool(snapshot),
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

/// Builds the AI-visible session todolist update tool.
/// 构建 AI 可见的会话 todolist更新工具。
fn build_todolist_tool() -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from(TODOLIST_TOOL_ID),
        name: String::from(TODOLIST_TOOL_ID),
        description: String::from(
            "Updates the session todolist/checklist progress shown to the user.\nThis is a todolist progress tool, not Plan Mode and not a design-plan document tool.\nProvide an optional explanation and a list of todolist items, each with a step and status.\nAt most one step can be in_progress at a time.",
        ),
        help_text: None,
        ai_visible: true,
        available_in_plan: false,
    };

    build_native_tool_definition(
        descriptor,
        todolist_args_schema(),
        move |request: NativeToolExecutionRequest| {
            Box::pin(async move { execute_todolist_tool(request).await })
        },
    )
}

/// One parsed step status accepted by the `todolist` tool.
/// `todolist` 工具接受的单个已解析步骤状态。
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TodolistStepStatusArg {
    /// The step has not started yet.
    /// 该步骤尚未开始。
    Pending,
    /// The step is currently being worked on.
    /// 该步骤正在进行中。
    InProgress,
    /// The step has been completed.
    /// 该步骤已经完成。
    Completed,
}

impl TodolistStepStatusArg {
    /// Returns the stable public status key.
    /// 返回稳定的公开状态键。
    ///
    /// # Returns
    /// 返回值
    /// The Codex-compatible snake_case status string.
    /// 与 Codex 兼容的 snake_case 状态字符串。
    fn as_key(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
        }
    }
}

/// One parsed todolist item accepted by the `todolist` tool.
/// `todolist` 工具接受的单个已解析条目。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct TodolistItemArg {
    /// Task step text.
    /// 任务步骤文本。
    step: String,
    /// Step status.
    /// 步骤状态。
    status: TodolistStepStatusArg,
}

/// Parsed argument payload accepted by the `todolist` tool.
/// `todolist` 工具接受的已解析参数载荷。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct TodolistArgs {
    /// Optional explanation for this todolist/checklist update.
    /// 本次 todolist/检查清单更新的可选说明。
    explanation: Option<String>,
    /// The list of todolist/checklist steps.
    /// todolist/检查清单步骤列表。
    items: Vec<TodolistItemArg>,
}

/// Builds the Codex-compatible JSON Schema for `todolist`.
/// 构建与 Codex 兼容的 `todolist` JSON Schema。
///
/// # Returns
/// 返回值
/// The model-facing argument schema for the todolist/checklist progress tool.
/// 面向模型的 todolist/检查清单进程工具参数结构。
fn todolist_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "explanation": {
                "type": "string",
                "description": "Optional explanation for this todolist/checklist progress update."
            },
            "items": {
                "type": "array",
                "description": "The ordered todolist/checklist steps to show as session progress.",
                "items": {
                    "type": "object",
                    "properties": {
                        "step": {
                            "type": "string",
                            "description": "Task step text.",
                            "minLength": 1
                        },
                        "status": {
                            "type": "string",
                            "description": "Step status.",
                            "enum": ["pending", "in_progress", "completed"]
                        }
                    },
                    "required": ["step", "status"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["items"],
        "additionalProperties": false
    })
}

/// Executes one `todolist` request and returns a host projection payload.
/// 执行单次 `todolist` 请求并返回宿主投影载荷。
///
/// # Parameters
/// 参数
/// `request` carries the raw model-supplied tool arguments.
/// `request` 携带模型提供的原始工具参数。
///
/// # Returns
/// 返回值
/// A Codex-compatible model response plus a Vulcan host result for session projection.
/// 与 Codex 兼容的模型响应以及用于会话投影的 Vulcan 宿主结果。
async fn execute_todolist_tool(
    request: NativeToolExecutionRequest,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let args: TodolistArgs = serde_json::from_value(request.arguments).map_err(|error| {
        NativeToolError::new(format!("failed to parse function arguments: {error}"))
    })?;
    let in_progress_count = args
        .items
        .iter()
        .filter(|item| matches!(item.status, TodolistStepStatusArg::InProgress))
        .count();
    if in_progress_count > 1 {
        return Err(NativeToolError::new(
            "At most one todolist item can be in_progress at a time.",
        ));
    }
    let mut items = Vec::with_capacity(args.items.len());
    for item in &args.items {
        // Trim here so the host projection never accepts a todolist item that the WebUI persistence layer would later discard.
        // 此处进行裁剪，避免宿主投影接受后又被 WebUI 持久化层丢弃。
        let step = item.step.trim();
        if step.is_empty() {
            return Err(NativeToolError::new(
                "Todolist item step cannot be empty after trimming.",
            ));
        }
        items.push(json!({
            "step": step,
            "status": item.status.as_key(),
        }));
    }
    Ok(
        VulcanToolInvocationResult::from_model_output(Value::String(String::from(
            "Todolist updated",
        )))
        .with_host_result(VulcanToolHostResult {
            kind: String::from("todolist"),
            payload: json!({
                "explanation": args.explanation,
                "items": items,
            }),
        }),
    )
}

/// Hidden marker proving a Markdown plan was created by the submit_plan tool.
/// 证明 Markdown 计划由 submit_plan 工具创建的隐藏标记。
const SUBMIT_PLAN_MARKER: &str = "<!-- generated-by: vulcan-submit-plan -->";

/// Parsed argument payload accepted by the `submit_plan` tool.
/// `submit_plan` 工具接受的已解析参数载荷。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct SubmitPlanArgs {
    /// Human-readable plan name used for the generated filename.
    /// 用于生成文件名的人类可读计划名称。
    name: String,
    /// Complete Markdown plan content to write.
    /// 需要写入的完整 Markdown 计划内容。
    content: String,
    /// Optional target directory under the workspace root.
    /// 工作区根目录下的可选目标目录。
    directory: Option<String>,
    /// Whether to replace a previous tool-created plan file.
    /// 是否替换此前由工具创建的计划文件。
    replace: Option<bool>,
    /// Exact previous plan path required when replacing.
    /// 替换时必需的此前计划完整路径。
    path: Option<String>,
}

/// Builds the Plan-mode plan artifact submission tool.
/// 构建计划模式计划产物提交工具。
fn build_submit_plan_tool() -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from(SUBMIT_PLAN_TOOL_ID),
        name: String::from(SUBMIT_PLAN_TOOL_ID),
        description: String::from(
            "Submit a complete Plan-mode Markdown plan artifact. Use this only in Plan mode after the implementation plan is ready. For revisions, set replace=true and pass the exact path returned by the previous submit_plan call.",
        ),
        help_text: None,
        ai_visible: true,
        available_in_plan: true,
    };

    build_native_tool_definition(
        descriptor,
        submit_plan_args_schema(),
        move |request: NativeToolExecutionRequest| {
            Box::pin(async move { execute_submit_plan_tool(request).await })
        },
    )
}

/// Builds the JSON Schema for `submit_plan`.
/// 构建 `submit_plan` 的 JSON Schema。
///
/// # Returns
/// 返回值
/// The model-facing argument schema for plan artifact submission.
/// 面向模型的计划产物提交参数结构。
fn submit_plan_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": {
                "type": "string",
                "description": "Short human-readable plan name used for the generated filename.",
                "minLength": 1
            },
            "content": {
                "type": "string",
                "description": "Complete self-contained Markdown plan content.",
                "minLength": 1
            },
            "directory": {
                "type": "string",
                "description": "Optional target directory under the current workspace root. Defaults to docs/plan."
            },
            "replace": {
                "type": "boolean",
                "description": "Set true only when replacing an existing tool-created plan artifact."
            },
            "path": {
                "type": "string",
                "description": "Exact previous plan file path returned by submit_plan. Required when replace=true."
            }
        },
        "required": ["name", "content"],
        "additionalProperties": false
    })
}

/// Executes one `submit_plan` request and writes the Markdown artifact.
/// 执行单次 `submit_plan` 请求并写入 Markdown 产物。
///
/// # Parameters
/// 参数
/// `request` carries raw model arguments and the workspace root.
/// `request` 携带模型原始参数与工作区根目录。
///
/// # Returns
/// 返回值
/// A model response and host projection pointing at the generated plan artifact.
/// 返回模型响应以及指向已生成计划产物的宿主投影。
async fn execute_submit_plan_tool(
    request: NativeToolExecutionRequest,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let args: SubmitPlanArgs =
        serde_json::from_value(request.arguments.clone()).map_err(|error| {
            NativeToolError::new(format!("failed to parse function arguments: {error}"))
        })?;
    let name = args.name.trim();
    let content = args.content.trim();
    if name.is_empty() {
        return Err(NativeToolError::new("Plan name cannot be empty."));
    }
    if content.is_empty() {
        return Err(NativeToolError::new("Plan content cannot be empty."));
    }
    let workspace_root = request
        .workspace_root
        .as_deref()
        .ok_or_else(|| NativeToolError::new("submit_plan requires a workspace root."))?;
    let workspace_root = canonicalize_existing_path(workspace_root).await?;
    let replace = args.replace.unwrap_or(false);
    let target_path = if replace {
        let path = args
            .path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| NativeToolError::new("replace=true requires the original plan path."))?;
        let path = PathBuf::from(path);
        ensure_workspace_child_path(&workspace_root, &path).await?;
        let existing = tokio_fs::read_to_string(&path).await.map_err(|error| {
            NativeToolError::new(format!("failed to read original plan: {error}"))
        })?;
        if !existing.contains(SUBMIT_PLAN_MARKER) {
            return Err(NativeToolError::new(
                "Refusing to replace a file that was not created by submit_plan.",
            ));
        }
        path
    } else {
        if args
            .path
            .as_deref()
            .is_some_and(|path| !path.trim().is_empty())
        {
            return Err(NativeToolError::new(
                "Pass path only with replace=true; new plans choose a fresh filename automatically.",
            ));
        }
        let directory = args
            .directory
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("docs/plan");
        let directory_path = resolve_new_plan_directory(&workspace_root, directory).await?;
        tokio_fs::create_dir_all(&directory_path)
            .await
            .map_err(|error| {
                NativeToolError::new(format!("failed to create plan directory: {error}"))
            })?;
        unique_plan_path(&directory_path, name).await?
    };
    let plan_text = format!("{SUBMIT_PLAN_MARKER}\n\n{content}\n");
    tokio_fs::write(&target_path, plan_text)
        .await
        .map_err(|error| NativeToolError::new(format!("failed to write plan file: {error}")))?;
    let path_text = display_submit_plan_path(&target_path);
    let updated_at = unix_millis_now().to_string();
    Ok(VulcanToolInvocationResult::from_model_output(json!({
        "path": path_text,
        "name": name,
        "message": "Plan artifact saved."
    }))
    .with_host_result(VulcanToolHostResult {
        kind: String::from("plan_artifact"),
        payload: json!({
            "name": name,
            "path": path_text,
            "content": content,
            "updatedAt": updated_at,
        }),
    }))
}

/// Formats a submitted Plan path for model-visible and UI-visible tool feedback.
/// 将已提交计划路径格式化为模型可见与界面可见的工具反馈文本。
///
/// # Parameters
/// 参数
/// `path` is the actual filesystem path used for writing the artifact.
/// `path` 是实际用于写入计划产物的文件系统路径。
///
/// # Returns
/// 返回值
/// A display path with Windows extended-path prefixes removed.
/// 返回移除 Windows 扩展路径前缀后的展示路径。
fn display_submit_plan_path(path: &Path) -> String {
    normalized_workspace_path_text_from_path(path)
}

/// Returns the current Unix epoch milliseconds.
/// 返回当前 Unix 纪元毫秒数。
///
/// # Returns
/// 返回值
/// The current timestamp in milliseconds, or zero if the system clock is before epoch.
/// 当前毫秒时间戳；如果系统时间早于纪元则返回零。
fn unix_millis_now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

/// Canonicalizes an existing path and converts failures into native-tool errors.
/// 规范化已存在路径，并将失败转换为原生工具错误。
///
/// # Parameters
/// 参数
/// `path` is the filesystem path to canonicalize.
/// `path` 是需要规范化的文件系统路径。
///
/// # Returns
/// 返回值
/// The canonical absolute path.
/// 规范化后的绝对路径。
async fn canonicalize_existing_path(path: &Path) -> Result<PathBuf, NativeToolError> {
    tokio_fs::canonicalize(path)
        .await
        .map_err(|error| NativeToolError::new(format!("failed to resolve path: {error}")))
}

/// Ensures a target path remains inside the workspace root.
/// 确保目标路径仍位于工作区根目录内。
///
/// # Parameters
/// 参数
/// `workspace_root` is the canonical workspace root.
/// `workspace_root` 是规范化后的工作区根目录。
/// `path` is the target path to validate.
/// `path` 是需要校验的目标路径。
///
/// # Returns
/// 返回值
/// `Ok(())` when the path is accepted.
/// 路径通过校验时返回 `Ok(())`。
async fn ensure_workspace_child_path(
    workspace_root: &Path,
    path: &Path,
) -> Result<(), NativeToolError> {
    let candidate = if path.exists() {
        canonicalize_existing_path(path).await?
    } else {
        let parent = path
            .parent()
            .ok_or_else(|| NativeToolError::new("target path has no parent directory."))?;
        let canonical_parent = canonicalize_existing_path(parent).await?;
        let file_name = path
            .file_name()
            .ok_or_else(|| NativeToolError::new("target path has no filename."))?;
        canonical_parent.join(file_name)
    };
    if !candidate.starts_with(workspace_root) {
        return Err(NativeToolError::new(
            "Plan path must stay inside the current workspace.",
        ));
    }
    Ok(())
}

/// Resolves a new plan directory while keeping it inside the workspace root.
/// 解析新的计划目录，并确保其位于工作区根目录内。
///
/// # Parameters
/// 参数
/// `workspace_root` is the canonical workspace root.
/// `workspace_root` 是规范化后的工作区根目录。
/// `directory` is the model-supplied relative or absolute directory.
/// `directory` 是模型提供的相对或绝对目录。
///
/// # Returns
/// 返回值
/// A workspace-contained directory path that may not exist yet.
/// 一个位于工作区内且可能尚不存在的目录路径。
async fn resolve_new_plan_directory(
    workspace_root: &Path,
    directory: &str,
) -> Result<PathBuf, NativeToolError> {
    let requested = PathBuf::from(directory);
    if requested
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(NativeToolError::new(
            "Plan directory cannot contain parent-directory components.",
        ));
    }
    if requested.is_absolute() {
        let parent = requested
            .parent()
            .ok_or_else(|| NativeToolError::new("plan directory has no parent."))?;
        let canonical_parent = canonicalize_existing_path(parent).await?;
        let file_name = requested
            .file_name()
            .ok_or_else(|| NativeToolError::new("plan directory has no final component."))?;
        let resolved = canonical_parent.join(file_name);
        if !resolved.starts_with(workspace_root) {
            return Err(NativeToolError::new(
                "Plan directory must stay inside the current workspace.",
            ));
        }
        return Ok(resolved);
    }
    Ok(workspace_root.join(requested))
}

/// Builds a unique Markdown path for a new plan artifact.
/// 为新的计划产物构建唯一 Markdown 路径。
///
/// # Parameters
/// 参数
/// `directory` is the already-created target directory.
/// `directory` 是已经创建的目标目录。
/// `name` is the human-readable plan name.
/// `name` 是人类可读计划名称。
///
/// # Returns
/// 返回值
/// A non-existing Markdown path.
/// 一个尚不存在的 Markdown 路径。
async fn unique_plan_path(directory: &Path, name: &str) -> Result<PathBuf, NativeToolError> {
    let base = slugify_plan_name(name);
    for suffix in 0..1000 {
        let filename = if suffix == 0 {
            format!("{base}.md")
        } else {
            format!("{base}_{suffix}.md")
        };
        let path = directory.join(filename);
        if tokio_fs::metadata(&path).await.is_err() {
            return Ok(path);
        }
    }
    Err(NativeToolError::new(
        "failed to allocate a unique plan filename after 1000 attempts.",
    ))
}

/// Converts one human-readable plan name into a conservative filename stem.
/// 将单个人类可读计划名称转换为保守文件名主体。
///
/// # Parameters
/// 参数
/// `name` is the model-supplied plan name.
/// `name` 是模型提供的计划名称。
///
/// # Returns
/// 返回值
/// A lowercase ASCII filename stem.
/// 小写 ASCII 文件名主体。
fn slugify_plan_name(name: &str) -> String {
    let mut slug = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        String::from("plan")
    } else {
        slug.chars().take(80).collect()
    }
}

/// Maximum accepted objective length copied from Codex thread-goal validation.
/// 从 Codex 线程目标校验复制的目标文本最大长度。
const MAX_THREAD_GOAL_OBJECTIVE_CHARS: usize = 4_000;

/// Parsed argument payload accepted by the `create_goal` tool.
/// `create_goal` 工具接受的已解析参数载荷。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateGoalArgs {
    /// Required concrete objective for the new goal.
    /// 新目标必需的具体目标内容。
    objective: String,
    /// Optional positive token budget explicitly requested by the user.
    /// 用户明确要求时提供的可选正数 token 预算。
    token_budget: Option<i64>,
}

/// Parsed status accepted by the `update_goal` tool.
/// `update_goal` 工具接受的已解析状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum UpdateGoalStatusArg {
    /// Mark the current goal as complete.
    /// 将当前目标标记为完成。
    Complete,
    /// Mark the current goal as genuinely blocked.
    /// 将当前目标标记为真正受阻。
    Blocked,
}

/// Parsed argument payload accepted by the `update_goal` tool.
/// `update_goal` 工具接受的已解析参数载荷。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateGoalArgs {
    /// Terminal status requested by the model.
    /// 模型请求设置的终态状态。
    status: UpdateGoalStatusArg,
}

/// Model-facing goal view aligned with Codex protocol fields.
/// 与 Codex 协议字段对齐的模型可见目标视图。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GoalModelView {
    /// Session identifier that owns this goal.
    /// 拥有该目标的会话标识。
    thread_id: SessionId,
    /// Human-authored goal objective.
    /// 用户编写的目标内容。
    objective: String,
    /// Current lifecycle status.
    /// 当前生命周期状态。
    status: ThreadGoalStatus,
    /// Optional explicit token budget.
    /// 可选的显式 token 预算。
    #[serde(skip_serializing_if = "Option::is_none")]
    token_budget: Option<i64>,
    /// Accounted token usage for this goal.
    /// 当前目标已记账的 token 用量。
    tokens_used: i64,
    /// Accounted wall-clock time in seconds.
    /// 当前目标已记账的墙钟时间秒数。
    time_used_seconds: i64,
    /// Creation timestamp in Unix epoch seconds.
    /// 创建时间的 Unix 纪元秒值。
    created_at: i64,
    /// Last update timestamp in Unix epoch seconds.
    /// 最近更新时间的 Unix 纪元秒值。
    updated_at: i64,
}

/// Model-facing tool response shared by all Goal native tools.
/// 所有 Goal 原生工具共享的模型可见工具响应。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GoalToolResponse {
    /// Current goal, if one exists.
    /// 当前目标，如果存在。
    goal: Option<GoalModelView>,
    /// Remaining explicit token budget, if a budget exists.
    /// 剩余显式 token 预算，如果存在预算。
    remaining_tokens: Option<i64>,
    /// Final usage-report instruction returned for completed goals with budget or elapsed time.
    /// 在带预算或耗时的目标完成时返回的最终用量汇报指令。
    completion_budget_report: Option<String>,
}

/// Builds the AI-visible goal read tool.
/// 构建 AI 可见的目标读取工具。
fn build_get_goal_tool() -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from(GET_GOAL_TOOL_ID),
        name: String::from(GET_GOAL_TOOL_ID),
        description: String::from(
            "Get the current goal for this thread, including status, budgets, token and elapsed-time usage, and remaining token budget.",
        ),
        help_text: None,
        ai_visible: true,
        available_in_plan: false,
    };

    build_native_tool_definition(
        descriptor,
        json!({
            "type": "object",
            "additionalProperties": false
        }),
        move |request: NativeToolExecutionRequest| {
            Box::pin(async move { execute_get_goal_tool(request).await })
        },
    )
}

/// Builds the AI-visible goal creation tool.
/// 构建 AI 可见的目标创建工具。
fn build_create_goal_tool() -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from(CREATE_GOAL_TOOL_ID),
        name: String::from(CREATE_GOAL_TOOL_ID),
        description: String::from(
            "Create a goal only when explicitly requested by the user or system/developer instructions; do not infer goals from ordinary tasks.\nSet token_budget only when an explicit token budget is requested. Fails if an unfinished goal exists; use update_goal only for status.",
        ),
        help_text: None,
        ai_visible: true,
        available_in_plan: false,
    };

    build_native_tool_definition(
        descriptor,
        create_goal_args_schema(),
        move |request: NativeToolExecutionRequest| {
            Box::pin(async move { execute_create_goal_tool(request).await })
        },
    )
}

/// Builds the AI-visible goal status update tool.
/// 构建 AI 可见的目标状态更新工具。
fn build_update_goal_tool() -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from(UPDATE_GOAL_TOOL_ID),
        name: String::from(UPDATE_GOAL_TOOL_ID),
        description: String::from(
            "Update the existing goal.\nUse this tool only to mark the goal achieved or genuinely blocked.\nSet status to `complete` only when the objective has actually been achieved and no required work remains.\nSet status to `blocked` only when the same blocking condition has repeated for at least three consecutive goal turns, counting the original/user-triggered turn and any automatic continuations, and the agent cannot make meaningful progress without user input or an external-state change.\nIf the user resumes a goal that was previously marked `blocked`, treat the resumed run as a fresh blocked audit. If the same blocking condition then repeats for at least three consecutive resumed goal turns, set status to `blocked` again.\nOnce the blocked threshold is satisfied, do not keep reporting that you are still blocked while leaving the goal active; set status to `blocked`.\nDo not use `blocked` merely because the work is hard, slow, uncertain, incomplete, or would benefit from clarification.\nDo not mark a goal complete merely because its budget is nearly exhausted or because you are stopping work.\nYou cannot use this tool to pause, resume, budget-limit, or usage-limit a goal; those status changes are controlled by the user or system.\nWhen marking a budgeted goal achieved with status `complete`, report the final token usage from the tool result to the user.",
        ),
        help_text: None,
        ai_visible: true,
        available_in_plan: false,
    };

    build_native_tool_definition(
        descriptor,
        update_goal_args_schema(),
        move |request: NativeToolExecutionRequest| {
            Box::pin(async move { execute_update_goal_tool(request).await })
        },
    )
}

/// Builds the Codex-compatible JSON Schema for `create_goal`.
/// 构建与 Codex 兼容的 `create_goal` JSON Schema。
///
/// # Returns
/// 返回值
/// The model-facing argument schema for the goal creation tool.
/// 面向模型的目标创建工具参数结构。
fn create_goal_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "objective": {
                "type": "string",
                "description": "Required. The concrete objective to start pursuing. This starts a new active goal when no goal exists or replaces the current goal when it is complete.",
                "minLength": 1,
                "maxLength": MAX_THREAD_GOAL_OBJECTIVE_CHARS
            },
            "token_budget": {
                "type": "integer",
                "description": "Positive token budget for the new goal. Omit unless explicitly requested.",
                "minimum": 1
            }
        },
        "required": ["objective"],
        "additionalProperties": false
    })
}

/// Builds the Codex-compatible JSON Schema for `update_goal`.
/// 构建与 Codex 兼容的 `update_goal` JSON Schema。
///
/// # Returns
/// 返回值
/// The model-facing argument schema for the goal status update tool.
/// 面向模型的目标状态更新工具参数结构。
fn update_goal_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "status": {
                "type": "string",
                "description": "Required. Set to `complete` only when the objective is achieved and no required work remains. Set to `blocked` only after the same blocking condition has recurred for at least three consecutive goal turns and the agent is at an impasse. After a previously blocked goal is resumed, the resumed run starts a fresh blocked audit.",
                "enum": ["complete", "blocked"]
            }
        },
        "required": ["status"],
        "additionalProperties": false
    })
}

/// Executes one `get_goal` request.
/// 执行单次 `get_goal` 请求。
///
/// # Parameters
/// 参数
/// `request` carries the current session and controller runtime context.
/// `request` 携带当前会话与控制器运行时上下文。
///
/// # Returns
/// 返回值
/// The current goal response formatted for the model.
/// 面向模型格式化的当前目标响应。
async fn execute_get_goal_tool(
    request: NativeToolExecutionRequest,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    ensure_empty_object_arguments(&request.arguments)?;
    let store = goal_store_from_request(&request)?;
    let session_id = goal_session_id_from_request(&request)?;
    let goal = store
        .get_thread_goal(session_id)
        .await
        .map_err(|error| NativeToolError::new(format!("failed to read goal: {error}")))?;
    Ok(goal_tool_result(goal, false))
}

/// Executes one `create_goal` request.
/// 执行单次 `create_goal` 请求。
///
/// # Parameters
/// 参数
/// `request` carries model arguments plus current session persistence context.
/// `request` 携带模型参数与当前会话持久化上下文。
///
/// # Returns
/// 返回值
/// The newly created goal and a host projection payload.
/// 新创建的目标以及宿主投影载荷。
async fn execute_create_goal_tool(
    request: NativeToolExecutionRequest,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let mut args: CreateGoalArgs =
        serde_json::from_value(request.arguments.clone()).map_err(|error| {
            NativeToolError::new(format!("failed to parse function arguments: {error}"))
        })?;
    args.objective = args.objective.trim().to_string();
    validate_goal_objective(&args.objective)?;
    validate_goal_budget(args.token_budget)?;

    let store = goal_store_from_request(&request)?;
    let session_id = goal_session_id_from_request(&request)?;
    let goal = store
        .insert_thread_goal(
            session_id,
            args.objective.as_str(),
            ThreadGoalStatus::Active,
            args.token_budget,
        )
        .await
        .map_err(|error| NativeToolError::new(format!("failed to create goal: {error}")))?
        .ok_or_else(|| {
            NativeToolError::new(
                "cannot create a new goal because this thread has an unfinished goal; complete the existing goal first",
            )
        })?;
    Ok(goal_tool_result_with_host(Some(goal), false))
}

/// Executes one `update_goal` request.
/// 执行单次 `update_goal` 请求。
///
/// # Parameters
/// 参数
/// `request` carries the target terminal status for the current goal.
/// `request` 携带当前目标的目标终态状态。
///
/// # Returns
/// 返回值
/// The updated goal and a host projection payload.
/// 更新后的目标以及宿主投影载荷。
async fn execute_update_goal_tool(
    request: NativeToolExecutionRequest,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let args: UpdateGoalArgs =
        serde_json::from_value(request.arguments.clone()).map_err(|error| {
            NativeToolError::new(format!("failed to parse function arguments: {error}"))
        })?;
    let store = goal_store_from_request(&request)?;
    let session_id = goal_session_id_from_request(&request)?;
    let target_status = match args.status {
        UpdateGoalStatusArg::Complete => ThreadGoalStatus::Complete,
        UpdateGoalStatusArg::Blocked => ThreadGoalStatus::Blocked,
    };
    let accounting_mode = match args.status {
        UpdateGoalStatusArg::Complete => GoalAccountingMode::ActiveOrComplete,
        UpdateGoalStatusArg::Blocked => GoalAccountingMode::ActiveOrStopped,
    };
    let _ = store
        .account_thread_goal_usage(session_id, 0, 0, accounting_mode, None)
        .await
        .map_err(|error| {
            NativeToolError::new(format!("failed to account goal progress: {error}"))
        })?;
    let goal = store
        .update_thread_goal(
            session_id,
            GoalUpdate {
                objective: None,
                status: Some(target_status),
                token_budget: None,
                expected_goal_id: None,
            },
        )
        .await
        .map_err(|error| NativeToolError::new(format!("failed to update goal: {error}")))?
        .ok_or_else(|| {
            NativeToolError::new("cannot update goal because this thread has no goal")
        })?;
    Ok(goal_tool_result_with_host(
        Some(goal),
        args.status == UpdateGoalStatusArg::Complete,
    ))
}

/// Resolves the goal store from one native-tool request.
/// 从单条原生工具请求解析目标存储。
///
/// # Parameters
/// 参数
/// `request` carries the optional controller runtime handle.
/// `request` 携带可选控制器运行时句柄。
///
/// # Returns
/// 返回值
/// A controller-backed goal store or a model-visible error.
/// 控制器承载目标存储或一个模型可见错误。
fn goal_store_from_request(
    request: &NativeToolExecutionRequest,
) -> Result<VulcanGoalStore, NativeToolError> {
    request
        .controller_runtime
        .clone()
        .map(VulcanGoalStore::new)
        .ok_or_else(|| NativeToolError::new("goal tools require a controller runtime context"))
}

/// Resolves the owning session id from one native-tool request.
/// 从单条原生工具请求解析所属会话标识。
///
/// # Parameters
/// 参数
/// `request` carries the optional current session id.
/// `request` 携带可选当前会话标识。
///
/// # Returns
/// 返回值
/// The current session id or a model-visible error.
/// 当前会话标识或一个模型可见错误。
fn goal_session_id_from_request(
    request: &NativeToolExecutionRequest,
) -> Result<SessionId, NativeToolError> {
    request
        .session_id
        .ok_or_else(|| NativeToolError::new("goal tools require a current session id"))
}

/// Validates that an argument payload is an empty object when provided.
/// 校验参数载荷在提供时必须为空对象。
///
/// # Parameters
/// 参数
/// `arguments` is the raw JSON argument value.
/// `arguments` 是原始 JSON 参数值。
///
/// # Returns
/// 返回值
/// Success when the arguments are absent-like or empty.
/// 参数类似缺省或为空时返回成功。
fn ensure_empty_object_arguments(arguments: &Value) -> Result<(), NativeToolError> {
    match arguments {
        Value::Object(map) if map.is_empty() => Ok(()),
        Value::Null => Ok(()),
        _ => Err(NativeToolError::new(
            "get_goal does not accept any arguments.",
        )),
    }
}

/// Validates one goal objective using Codex-compatible constraints.
/// 使用 Codex 兼容约束校验单条目标内容。
///
/// # Parameters
/// 参数
/// `objective` is the trimmed objective text.
/// `objective` 是裁剪后的目标文本。
///
/// # Returns
/// 返回值
/// Success when the objective is non-empty and within the maximum length.
/// 目标非空且未超过最大长度时返回成功。
fn validate_goal_objective(objective: &str) -> Result<(), NativeToolError> {
    if objective.is_empty() {
        return Err(NativeToolError::new("goal objective must not be empty"));
    }
    if objective.chars().count() > MAX_THREAD_GOAL_OBJECTIVE_CHARS {
        return Err(NativeToolError::new(format!(
            "goal objective must be at most {MAX_THREAD_GOAL_OBJECTIVE_CHARS} characters"
        )));
    }
    Ok(())
}

/// Validates one optional explicit token budget.
/// 校验一个可选的显式 token 预算。
///
/// # Parameters
/// 参数
/// `token_budget` is the optional budget supplied by the model.
/// `token_budget` 是模型提供的可选预算。
///
/// # Returns
/// 返回值
/// Success when the budget is absent or strictly positive.
/// 预算缺省或严格为正数时返回成功。
fn validate_goal_budget(token_budget: Option<i64>) -> Result<(), NativeToolError> {
    if let Some(value) = token_budget
        && value <= 0
    {
        return Err(NativeToolError::new(
            "goal token_budget must be a positive integer",
        ));
    }
    Ok(())
}

/// Converts a persisted goal into the Codex-compatible model view.
/// 将持久化目标转换为 Codex 兼容的模型视图。
///
/// # Parameters
/// 参数
/// `goal` is the persisted goal record.
/// `goal` 是持久化目标记录。
///
/// # Returns
/// 返回值
/// A model-facing goal view with second-level timestamps.
/// 带秒级时间戳的模型可见目标视图。
fn goal_model_view(goal: &ThreadGoal) -> GoalModelView {
    GoalModelView {
        thread_id: goal.thread_id,
        objective: goal.objective.clone(),
        status: goal.status,
        token_budget: goal.token_budget,
        tokens_used: goal.tokens_used,
        time_used_seconds: goal.time_used_seconds,
        created_at: goal.created_at_ms / 1_000,
        updated_at: goal.updated_at_ms / 1_000,
    }
}

/// Renders the shared goal tool result without a host projection.
/// 渲染不携带宿主投影的共享目标工具结果。
///
/// # Parameters
/// 参数
/// `goal` is the optional persisted goal.
/// `goal` 是可选的持久化目标。
/// `include_completion_report` controls final budget report visibility.
/// `include_completion_report` 控制最终预算报告是否可见。
///
/// # Returns
/// 返回值
/// A model-only invocation result.
/// 仅包含模型输出的调用结果。
fn goal_tool_result(
    goal: Option<ThreadGoal>,
    include_completion_report: bool,
) -> VulcanToolInvocationResult {
    let response = goal_response_value(goal.as_ref(), include_completion_report);
    VulcanToolInvocationResult::from_model_output(response)
}

/// Renders the shared goal tool result with a host projection.
/// 渲染携带宿主投影的共享目标工具结果。
///
/// # Parameters
/// 参数
/// `goal` is the optional persisted goal.
/// `goal` 是可选的持久化目标。
/// `include_completion_report` controls final budget report visibility.
/// `include_completion_report` 控制最终预算报告是否可见。
///
/// # Returns
/// 返回值
/// A model invocation result plus a host `session_goal` projection.
/// 模型调用结果以及宿主 `session_goal` 投影。
fn goal_tool_result_with_host(
    goal: Option<ThreadGoal>,
    include_completion_report: bool,
) -> VulcanToolInvocationResult {
    let response = goal_response_value(goal.as_ref(), include_completion_report);
    let host_payload = json!({
        "goal": goal.as_ref().map(goal_host_projection),
    });
    VulcanToolInvocationResult::from_model_output(response).with_host_result(VulcanToolHostResult {
        kind: String::from("session_goal"),
        payload: host_payload,
    })
}

/// Builds the model-facing goal response value.
/// 构建模型可见的目标响应值。
///
/// # Parameters
/// 参数
/// `goal` is the optional persisted goal.
/// `goal` 是可选的持久化目标。
/// `include_completion_report` controls final budget report visibility.
/// `include_completion_report` 控制最终预算报告是否可见。
///
/// # Returns
/// 返回值
/// A serializable JSON value for the tool result.
/// 工具结果使用的可序列化 JSON 值。
fn goal_response_value(goal: Option<&ThreadGoal>, include_completion_report: bool) -> Value {
    let remaining_tokens = goal.and_then(|goal| {
        goal.token_budget
            .map(|budget| (budget - goal.tokens_used).max(0))
    });
    let completion_budget_report = if include_completion_report {
        goal.and_then(goal_completion_budget_report)
    } else {
        None
    };
    json!(GoalToolResponse {
        goal: goal.map(goal_model_view),
        remaining_tokens,
        completion_budget_report,
    })
}

/// Builds the final completion usage-report instruction used by Codex goal tools.
/// 构建 Codex 目标工具使用的最终完成用量汇报指令。
///
/// # Parameters
/// 参数
/// `goal` is the completed persisted goal.
/// `goal` 是已完成的持久化目标。
///
/// # Returns
/// 返回值
/// A usage-report instruction when an explicit budget or elapsed time exists.
/// 存在显式预算或耗时时返回用量汇报指令。
fn goal_completion_budget_report(goal: &ThreadGoal) -> Option<String> {
    if goal.token_budget.is_none() && goal.time_used_seconds <= 0 {
        None
    } else {
        Some(
            "Goal achieved. Report final usage from this tool result's structured goal fields. If `goal.tokenBudget` is present, include token usage from `goal.tokensUsed` and `goal.tokenBudget`. If `goal.timeUsedSeconds` is greater than 0, summarize elapsed time in a concise, human-friendly form appropriate to the response language."
                .to_string(),
        )
    }
}

/// Builds the host-facing goal projection used by workbench snapshots.
/// 构建工作台快照使用的宿主可见目标投影。
///
/// # Parameters
/// 参数
/// `goal` is the persisted goal record.
/// `goal` 是持久化目标记录。
///
/// # Returns
/// 返回值
/// A JSON projection including internal goal id and millisecond timestamps.
/// 包含内部目标标识与毫秒时间戳的 JSON 投影。
fn goal_host_projection(goal: &ThreadGoal) -> Value {
    json!({
        "sessionId": goal.thread_id,
        "goalId": goal.goal_id,
        "objective": goal.objective,
        "status": goal.status,
        "tokenBudget": goal.token_budget,
        "tokensUsed": goal.tokens_used,
        "timeUsedSeconds": goal.time_used_seconds,
        "createdAtMs": goal.created_at_ms,
        "updatedAtMs": goal.updated_at_ms,
    })
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
#[cfg(test)]
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

/// Builds the native persistent terminal-session tool exposed to AI.
/// 构建暴露给 AI 的原生持久终端会话工具。
fn build_terminal_session_tool(
    command_session_runtime: Arc<CommandSessionRuntime>,
    shell_support: HostCommandShellSupport,
) -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from("terminal_session"),
        name: String::from("terminal_session"),
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

/// Builds the VMM memory-delete native tool.
/// 构建 VMM 记忆删除原生工具。
fn build_vmm_memory_delete_tool(
    snapshot: Arc<NativeToolRuntimeSnapshot>,
) -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from("vmm_memory_delete"),
        name: String::from("vmm_memory_delete"),
        description: String::from(
            "Delete explicit VulcanMemoryMesh memory ids for the current configured user and project. Use only when the user asked to remove a memory or a recalled memory is clearly wrong, stale, or harmful.",
        ),
        help_text: Some(vmm_memory_delete_usage_help_markdown().to_string()),
        ai_visible: true,
        available_in_plan: false,
    };

    build_native_tool_definition(
        descriptor,
        vmm_memory_delete_args_schema(),
        move |request: NativeToolExecutionRequest| {
            let snapshot = snapshot.clone();
            Box::pin(async move { execute_vmm_memory_delete_tool(request, snapshot).await })
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

/// One typed argument payload accepted by `vmm_memory_delete`.
/// `vmm_memory_delete` 接受的一份强类型参数载荷。
#[derive(Debug, Clone, Deserialize)]
struct VmmMemoryDeleteToolArgs {
    /// Durable VMM memory ids to delete.
    /// 需要删除的长期 VMM 记忆标识集合。
    #[serde(rename = "memoryIds")]
    memory_ids: Vec<u64>,
    /// Deletion reason used for audit logs.
    /// 用于审计日志的删除原因。
    reason: String,
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
#[cfg(test)]
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ShellSessionToolAction {
    Start,
    Read,
    Write,
    Stop,
}

#[cfg(test)]
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
#[cfg(test)]
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

/// One action accepted by the host-owned `terminal_session` tool.
/// 宿主自有 `terminal_session` 工具接受的一种动作。
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TerminalSessionToolAction {
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
#[serde(tag = "kind", rename_all = "snake_case")]
enum TerminalSessionInput {
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
enum TerminalSessionWaitMode {
    /// Return immediately after the input or read request.
    /// 在输入或读取请求后立即返回。
    ReturnImmediately,
    /// Wait for at least one output event within the bounded wait budget.
    /// 在有界等待预算内等待至少一条输出事件。
    UntilQuiet,
    /// Wait using the same bounded behavior while prompt detection remains best-effort.
    /// 在提示符检测仍为尽力而为时使用相同有界等待行为。
    UntilPrompt,
    /// Wrap one command transaction and wait for the host exit-code sentinel.
    /// 包装一条命令事务并等待宿主退出码哨兵。
    UntilExitCode,
    /// Wait for output matching a known readiness pattern.
    /// 等待匹配已知就绪模式的输出。
    UntilPattern,
    /// Wait briefly for screen-like output to stabilize.
    /// 短暂等待屏幕型输出稳定。
    ScreenStable,
}

/// Key encoding family used when terminal_session converts standardized key names into terminal input bytes.
/// terminal_session 将标准化按键名转换为终端输入字节时使用的按键编码族。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TerminalSessionKeyEncoding {
    /// ANSI/VT escape sequences used by ordinary PTY and terminal applications.
    /// 普通 PTY 与终端应用使用的 ANSI/VT 转义序列。
    AnsiVt,
    /// Windows console `msvcrt.getch()` extended-key byte pairs.
    /// Windows 控制台 `msvcrt.getch()` 扩展键字节对。
    WindowsConsoleGetch,
}

impl TerminalSessionKeyEncoding {
    /// Renders one stable lowercase key-encoding label.
    /// 渲染稳定的小写按键编码标签。
    fn as_str(self) -> &'static str {
        match self {
            Self::AnsiVt => "ansi_vt",
            Self::WindowsConsoleGetch => "windows_console_getch",
        }
    }
}

impl TerminalSessionWaitMode {
    /// Renders one stable lowercase wait-mode label.
    /// 渲染稳定的小写等待模式标签。
    fn as_str(self) -> &'static str {
        match self {
            Self::ReturnImmediately => "return_immediately",
            Self::UntilQuiet => "until_quiet",
            Self::UntilPrompt => "until_prompt",
            Self::UntilExitCode => "until_exit_code",
            Self::UntilPattern => "until_pattern",
            Self::ScreenStable => "screen_stable",
        }
    }
}

/// One wait-policy payload accepted by `terminal_session`.
/// `terminal_session` 接受的一份等待策略载荷。
#[derive(Debug, Clone, Deserialize)]
struct TerminalSessionWaitPolicy {
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
struct TerminalSessionToolArgs {
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
    /// Optional key encoding used when converting key/chord inputs during send.
    /// send 时转换 key/chord 输入所使用的可选按键编码。
    key_encoding: Option<TerminalSessionKeyEncoding>,
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

/// One parsed terminal command transaction sentinel result.
/// 一份已解析的终端命令事务哨兵结果。
#[derive(Debug, Clone)]
struct TerminalTransactionResult {
    /// Stable host-generated transaction identifier.
    /// 由宿主生成的稳定事务标识。
    id: String,
    /// User command wrapped by this transaction.
    /// 本次事务包装的用户命令。
    command: String,
    /// Optional exit code captured from the sentinel line.
    /// 从哨兵行捕获到的可选退出码。
    exit_code: Option<i32>,
    /// Stable transaction status.
    /// 稳定事务状态。
    status: String,
    /// First output sequence observed for the transaction.
    /// 本次事务观测到的首个输出序号。
    started_seq: u64,
    /// Last output sequence observed for the transaction.
    /// 本次事务观测到的最后输出序号。
    ended_seq: Option<u64>,
}

/// Result summary produced by an interrupt drain pass.
/// 一次中断排空过程产生的结果摘要。
#[derive(Debug, Clone)]
struct TerminalInterruptDrainResult {
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
struct TerminalInterruptCleanupResult {
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
struct TerminalWaitReadResult {
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
#[cfg(test)]
fn shell_session_tool_description() -> String {
    format!(
        "Start, read, write to, or stop a long-lived host command session. Reads are polling-based, and `cwd` only affects `start`. The `start` result reports the resolved launcher shell used for `auto` and whether stdin is currently left on an open input line. When an inline read result exceeds {SHELL_INLINE_OUTPUT_LIMIT_BYTES} bytes, the host switches to page-mode spill files under the project-local `.vulcan/temp/` directory using names like `tool_output_overflow_page_md_<unix_ms>.md`. After spill, the host preserves the full rendered output instead of truncating it, subject to successful file creation, filesystem limits, and per-line safe-read chunking."
    )
}

/// Builds the shell-session-specific schema text for the `shell` argument so AI sees the current `auto` target plus nested-shell guidance.
/// 为 `shell_session` 的 `shell` 参数构建专用 schema 文本，使 AI 能看到当前 `auto` 目标及嵌套 shell 指引。
#[cfg(test)]
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

/// Returns Markdown usage guidance for the VMM memory-delete tool.
/// 返回 VMM 删除记忆工具的 Markdown 使用说明。
fn vmm_memory_delete_usage_help_markdown() -> &'static str {
    r#"Use `vmm_memory_delete` only for explicit durable memory ids. Valid reasons include: the user explicitly asked to remove a memory, a recalled memory is contradicted by newer user-approved information, or the memory contains wrong/stale/harmful data.

Prefer deleting by `MEMORY_ID` shown in recalled VMM context or by `memory_id` returned from `vmm_memory_search`. Do not delete memories just because they are irrelevant to the current turn. Always include a concise audit reason."#
}

/// Builds the JSON Schema advertised to the model for `vmm_memory_delete`.
/// 为 `vmm_memory_delete` 构建对模型声明的 JSON Schema。
fn vmm_memory_delete_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "memoryIds": {
                "type": "array",
                "items": { "type": "integer", "minimum": 1 },
                "minItems": 1,
                "maxItems": 256,
                "description": "One or more durable VMM memory ids to delete. Use MEMORY_ID labels from recalled context or memory_id values returned by vmm_memory_search."
            },
            "reason": {
                "type": "string",
                "maxLength": 1024,
                "description": "Concise deletion reason for audit logs."
            }
        },
        "required": ["memoryIds", "reason"],
        "additionalProperties": false
    })
}

/// Builds the JSON Schema advertised to the model for the host-owned `shell_session` tool.
/// 为宿主自有 `shell_session` 工具构建对模型声明的 JSON Schema。
#[cfg(test)]
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

/// Builds the English AI-facing description for the native persistent terminal tool.
/// 构建原生持久终端工具面向 AI 的英文描述。
fn terminal_session_tool_description() -> String {
    String::from(
        "Operate named persistent host terminal sessions. Before using terminal_session, you must call skill(\"terminal-session\") and follow its instructions. Do not use this tool from memory. The start action requires a non-empty name. Use shell_command for isolated one-shot commands. terminal_session supports list/start/send/read/snapshot/stop, ordered text/paste/key/chord input, incremental output, native Windows console key events when available, TUI-aware output mode hints, and command transactions with per-send exit codes via wait_policy.mode=until_exit_code.",
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
fn terminal_session_args_schema(shell_support: &HostCommandShellSupport) -> Value {
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
                                "text": { "type": "string", "description": "Text typed into the terminal. Include a newline when submitting a command." }
                            },
                            "required": ["kind", "text"],
                            "additionalProperties": false
                        },
                        {
                            "type": "object",
                            "properties": {
                                "kind": { "type": "string", "enum": ["paste"] },
                                "text": { "type": "string", "description": "Text pasted into the terminal as one payload." }
                            },
                            "required": ["kind", "text"],
                            "additionalProperties": false
                        },
                        {
                            "type": "object",
                            "properties": {
                                "kind": { "type": "string", "enum": ["key"] },
                                "key": { "type": "string", "description": "Standard key name such as Enter, Tab, Esc, ArrowDown, Home, End, PageUp, PageDown, Insert, Delete, or F1-F12. The key_encoding field controls whether navigation/function keys use ANSI VT bytes or Windows msvcrt.getch byte pairs." }
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
                "description": "Optional bounded wait behavior for send/read. Use until_exit_code only with exactly one text input containing one ordinary shell command transaction, not paste/key/chord, REPL/TUI, or long-running services.",
                "properties": {
                    "mode": {
                        "type": "string",
                        "enum": ["return_immediately", "until_quiet", "until_prompt", "until_exit_code", "until_pattern", "screen_stable"]
                    },
                    "quiet_ms": { "type": "integer" },
                    "max_wait_ms": { "type": "integer" },
                    "pattern": { "type": "string" }
                },
                "required": ["mode"],
                "additionalProperties": false
            },
            "key_encoding": {
                "type": "string",
                "enum": ["ansi_vt", "windows_console_getch"],
                "description": "Optional key encoding for send key/chord inputs. Omit this field on Windows native console sessions to deliver real Console KEY_EVENT input for msvcrt.getch() and Console.ReadKey(). Set ansi_vt explicitly when a Windows native console session must feed stdin/VT readers such as sys.stdin.read(1). Use windows_console_getch only for explicit byte-oriented compatibility cases."
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
                "description": format!("Optional maximum output chunks returned by one read-like action. Default: {DEFAULT_COMMAND_SESSION_READ_MAX_CHUNKS}.")
            }
        },
        "required": ["action"],
        "additionalProperties": false
    })
}

/// Parses one raw model payload into the typed shell-session argument structure.
/// 将一份原始模型载荷解析为强类型 shell 会话参数结构。
#[cfg(test)]
fn parse_shell_session_tool_arguments(
    arguments: Value,
) -> Result<ShellSessionToolArgs, NativeToolError> {
    serde_json::from_value::<ShellSessionToolArgs>(arguments).map_err(|error| {
        NativeToolError::new(format!("Failed to parse shell_session arguments: {error}"))
    })
}

/// Parses one raw model payload into the typed terminal-session argument structure.
/// 将一份原始模型载荷解析为强类型终端会话参数结构。
fn parse_terminal_session_tool_arguments(
    arguments: Value,
) -> Result<TerminalSessionToolArgs, NativeToolError> {
    serde_json::from_value::<TerminalSessionToolArgs>(arguments).map_err(|error| {
        NativeToolError::new(format!(
            "Failed to parse terminal_session arguments: {error}"
        ))
    })
}

/// Resolves the controller session that owns the current shell-session tool invocation.
/// 解析拥有当前 shell 会话工具调用的控制器会话。
#[cfg(test)]
fn require_shell_session_owner_session_id(
    session_id: Option<SessionId>,
) -> Result<SessionId, NativeToolError> {
    session_id.ok_or_else(|| {
        NativeToolError::new(
            "The shell_session tool requires a stable controller session identifier, but no session_id was attached to this invocation.",
        )
    })
}

/// Resolves the controller session that owns the current terminal-session tool invocation.
/// 解析拥有当前终端会话工具调用的控制器会话。
fn require_terminal_session_owner_session_id(
    session_id: Option<SessionId>,
) -> Result<SessionId, NativeToolError> {
    session_id.ok_or_else(|| {
        NativeToolError::new(
            "The terminal_session tool requires a stable controller session identifier, but no session_id was attached to this invocation.",
        )
    })
}

/// Parses the caller-provided command-session identifier into the stable UUID-shaped session id.
/// 将调用方提供的命令会话标识解析为稳定的 UUID 形态会话 id。
#[cfg(test)]
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
fn normalize_terminal_session_name(raw_name: Option<&str>, command_text: &str) -> String {
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

/// Encodes one ordered terminal input list into the bytes written to the child stdin.
/// 将一组保序终端输入编码为写入子进程标准输入的字节文本。
fn encode_terminal_inputs(
    inputs: &[TerminalSessionInput],
    interactive_newline: &str,
    key_encoding: TerminalSessionKeyEncoding,
) -> Result<Vec<u8>, NativeToolError> {
    let mut encoded = Vec::new();
    for input in inputs {
        match input {
            TerminalSessionInput::Text { text } | TerminalSessionInput::Paste { text } => {
                encoded.extend_from_slice(text.as_bytes());
            }
            TerminalSessionInput::Key { key } => {
                encoded.extend_from_slice(&terminal_key_sequence(
                    key,
                    interactive_newline,
                    key_encoding,
                )?);
            }
            TerminalSessionInput::Chord {
                key,
                ctrl,
                alt,
                shift,
                meta,
            } => {
                encoded.extend_from_slice(&terminal_chord_sequence(
                    key,
                    ctrl.unwrap_or(false),
                    alt.unwrap_or(false),
                    shift.unwrap_or(false),
                    meta.unwrap_or(false),
                    interactive_newline,
                    key_encoding,
                )?);
            }
        }
    }
    Ok(encoded)
}

/// Converts terminal-session inputs into backend-neutral structured input events.
/// 将终端会话输入转换为后端无关的结构化输入事件。
fn terminal_input_events_from_inputs(
    inputs: &[TerminalSessionInput],
) -> Vec<CommandSessionInputEvent> {
    inputs
        .iter()
        .map(|input| match input {
            TerminalSessionInput::Text { text } => {
                CommandSessionInputEvent::Text { text: text.clone() }
            }
            TerminalSessionInput::Paste { text } => {
                CommandSessionInputEvent::Paste { text: text.clone() }
            }
            TerminalSessionInput::Key { key } => CommandSessionInputEvent::Key { key: key.clone() },
            TerminalSessionInput::Chord {
                key,
                ctrl,
                alt,
                shift,
                meta,
            } => CommandSessionInputEvent::Chord {
                key: key.clone(),
                ctrl: ctrl.unwrap_or(false),
                alt: alt.unwrap_or(false),
                shift: shift.unwrap_or(false),
                meta: meta.unwrap_or(false),
            },
        })
        .collect()
}

/// Converts terminal-session inputs into WindowsConsole events for the requested key encoding.
/// 根据请求的按键编码将终端会话输入转换为 WindowsConsole 事件。
fn windows_console_input_events_from_inputs(
    inputs: &[TerminalSessionInput],
    interactive_newline: &str,
    requested_key_encoding: Option<TerminalSessionKeyEncoding>,
) -> Result<Vec<CommandSessionInputEvent>, NativeToolError> {
    match requested_key_encoding {
        None => Ok(terminal_input_events_from_inputs(inputs)),
        Some(TerminalSessionKeyEncoding::AnsiVt) => {
            let encoded = encode_terminal_inputs(
                inputs,
                interactive_newline,
                TerminalSessionKeyEncoding::AnsiVt,
            )?;
            let text = String::from_utf8(encoded).map_err(|error| {
                NativeToolError::new(format!(
                    "ANSI VT terminal input contained invalid UTF-8: {error}"
                ))
            })?;
            Ok(vec![CommandSessionInputEvent::Text { text }])
        }
        Some(TerminalSessionKeyEncoding::WindowsConsoleGetch) => Err(NativeToolError::new(
            "`windows_console_getch` is only for byte-oriented compatibility backends. Omit `key_encoding` for Windows native console sessions, or use `ansi_vt` for stdin/VT readers.",
        )),
    }
}

/// Returns the effective key encoding label reported for one send action.
/// 返回一次 send 动作报告的有效按键编码标签。
fn terminal_effective_key_encoding_label(
    backend: CommandSessionBackend,
    requested_key_encoding: Option<TerminalSessionKeyEncoding>,
) -> &'static str {
    if backend == CommandSessionBackend::WindowsConsole && requested_key_encoding.is_none() {
        "native_console"
    } else {
        requested_key_encoding
            .unwrap_or(TerminalSessionKeyEncoding::AnsiVt)
            .as_str()
    }
}

/// Returns the terminal sequence for one standardized key.
/// 返回一个标准化按键对应的终端序列。
fn terminal_key_sequence(
    key: &str,
    interactive_newline: &str,
    key_encoding: TerminalSessionKeyEncoding,
) -> Result<Vec<u8>, NativeToolError> {
    if key_encoding == TerminalSessionKeyEncoding::WindowsConsoleGetch {
        return terminal_windows_getch_key_sequence(key, interactive_newline);
    }
    match key {
        "Enter" => Ok(interactive_newline.as_bytes().to_vec()),
        "Tab" => Ok(b"\t".to_vec()),
        "Esc" | "Escape" => Ok(vec![0x1b]),
        "Backspace" => Ok(vec![0x08]),
        "Delete" => Ok(b"\x1b[3~".to_vec()),
        "ArrowUp" => Ok(b"\x1b[A".to_vec()),
        "ArrowDown" => Ok(b"\x1b[B".to_vec()),
        "ArrowRight" => Ok(b"\x1b[C".to_vec()),
        "ArrowLeft" => Ok(b"\x1b[D".to_vec()),
        "Home" => Ok(b"\x1b[H".to_vec()),
        "End" => Ok(b"\x1b[F".to_vec()),
        "PageUp" => Ok(b"\x1b[5~".to_vec()),
        "PageDown" => Ok(b"\x1b[6~".to_vec()),
        "Insert" => Ok(b"\x1b[2~".to_vec()),
        "F1" => Ok(b"\x1bOP".to_vec()),
        "F2" => Ok(b"\x1bOQ".to_vec()),
        "F3" => Ok(b"\x1bOR".to_vec()),
        "F4" => Ok(b"\x1bOS".to_vec()),
        "F5" => Ok(b"\x1b[15~".to_vec()),
        "F6" => Ok(b"\x1b[17~".to_vec()),
        "F7" => Ok(b"\x1b[18~".to_vec()),
        "F8" => Ok(b"\x1b[19~".to_vec()),
        "F9" => Ok(b"\x1b[20~".to_vec()),
        "F10" => Ok(b"\x1b[21~".to_vec()),
        "F11" => Ok(b"\x1b[23~".to_vec()),
        "F12" => Ok(b"\x1b[24~".to_vec()),
        _ => Err(NativeToolError::new(format!(
            "Unsupported terminal key `{key}`. Call skill(\"terminal-session\") and use a documented key name."
        ))),
    }
}

/// Returns the Windows console getch byte sequence for one standardized key.
/// 返回一个标准化按键对应的 Windows 控制台 getch 字节序列。
fn terminal_windows_getch_key_sequence(
    key: &str,
    interactive_newline: &str,
) -> Result<Vec<u8>, NativeToolError> {
    match key {
        "Enter" => Ok(interactive_newline.as_bytes().to_vec()),
        "Tab" => Ok(vec![b'\t']),
        "Esc" | "Escape" => Ok(vec![0x1b]),
        "Backspace" => Ok(vec![0x08]),
        "ArrowUp" | "ArrowDown" | "ArrowRight" | "ArrowLeft" | "Home" | "End" | "PageUp"
        | "PageDown" | "Insert" | "Delete" | "F1" | "F2" | "F3" | "F4" | "F5" | "F6" | "F7"
        | "F8" | "F9" | "F10" | "F11" | "F12"
            if cfg!(windows) =>
        {
            Err(NativeToolError::new(format!(
                "The current byte-oriented terminal backend cannot synthesize console extended key `{key}` for msvcrt.getch(). Use the Windows native console backend or send a documented text shortcut such as 1/2/3, Enter, Esc, or q."
            )))
        }
        "ArrowUp" => Ok(vec![0xe0, b'H']),
        "ArrowDown" => Ok(vec![0xe0, b'P']),
        "ArrowRight" => Ok(vec![0xe0, b'M']),
        "ArrowLeft" => Ok(vec![0xe0, b'K']),
        "Home" => Ok(vec![0xe0, b'G']),
        "End" => Ok(vec![0xe0, b'O']),
        "PageUp" => Ok(vec![0xe0, b'I']),
        "PageDown" => Ok(vec![0xe0, b'Q']),
        "Insert" => Ok(vec![0xe0, b'R']),
        "Delete" => Ok(vec![0xe0, b'S']),
        "F1" => Ok(vec![0x00, b';']),
        "F2" => Ok(vec![0x00, b'<']),
        "F3" => Ok(vec![0x00, b'=']),
        "F4" => Ok(vec![0x00, b'>']),
        "F5" => Ok(vec![0x00, b'?']),
        "F6" => Ok(vec![0x00, b'@']),
        "F7" => Ok(vec![0x00, b'A']),
        "F8" => Ok(vec![0x00, b'B']),
        "F9" => Ok(vec![0x00, b'C']),
        "F10" => Ok(vec![0x00, b'D']),
        "F11" => Ok(vec![0xe0, 0x85]),
        "F12" => Ok(vec![0xe0, 0x86]),
        _ => Err(NativeToolError::new(format!(
            "Unsupported Windows console getch key `{key}`. Use Enter, Tab, Esc, Backspace, or text shortcuts."
        ))),
    }
}

/// Returns the terminal sequence for one standardized key chord.
/// 返回一个标准化组合键对应的终端序列。
fn terminal_chord_sequence(
    key: &str,
    ctrl: bool,
    alt: bool,
    shift: bool,
    meta: bool,
    interactive_newline: &str,
    key_encoding: TerminalSessionKeyEncoding,
) -> Result<Vec<u8>, NativeToolError> {
    if shift && key == "Tab" && !ctrl && !alt && !meta {
        return Ok(b"\x1b[Z".to_vec());
    }

    let mut sequence = Vec::new();
    if alt || meta {
        sequence.push(0x1b);
    }
    if ctrl {
        let normalized = key.trim().to_ascii_lowercase();
        let mut chars = normalized.chars();
        let Some(ch) = chars.next() else {
            return Err(NativeToolError::new("Chord key cannot be empty."));
        };
        if chars.next().is_none() && ch.is_ascii_alphabetic() {
            sequence.push((ch as u8) & 0x1f);
            return Ok(sequence);
        }
        return Err(NativeToolError::new(format!(
            "Unsupported Ctrl chord `{key}`. Use a single alphabetic key such as c, d, z, l, or r."
        )));
    }
    sequence.extend_from_slice(&terminal_key_sequence(
        key,
        interactive_newline,
        key_encoding,
    )?);
    Ok(sequence)
}

/// Returns whether the input list requests a foreground-process interrupt.
/// 返回输入列表是否请求中断前台进程。
fn terminal_inputs_request_interrupt(inputs: &[TerminalSessionInput]) -> bool {
    inputs.iter().any(|input| {
        matches!(
            input,
            TerminalSessionInput::Chord {
                key,
                ctrl: Some(true),
                alt,
                shift,
                meta,
            } if key.eq_ignore_ascii_case("c")
                && !alt.unwrap_or(false)
                && !shift.unwrap_or(false)
                && !meta.unwrap_or(false)
        )
    })
}

/// Returns the visible label for one terminal input item.
/// 返回一条终端输入项的可见标签。
fn terminal_input_label(input: &TerminalSessionInput) -> String {
    match input {
        TerminalSessionInput::Text { text } => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                String::from("text")
            } else {
                trimmed.chars().take(80).collect()
            }
        }
        TerminalSessionInput::Paste { text } => {
            format!("paste {} chars", text.chars().count())
        }
        TerminalSessionInput::Key { key } => key.clone(),
        TerminalSessionInput::Chord {
            key,
            ctrl,
            alt,
            shift,
            meta,
        } => {
            let mut parts = Vec::new();
            if ctrl.unwrap_or(false) {
                parts.push("Ctrl");
            }
            if alt.unwrap_or(false) {
                parts.push("Alt");
            }
            if shift.unwrap_or(false) {
                parts.push("Shift");
            }
            if meta.unwrap_or(false) {
                parts.push("Meta");
            }
            parts.push(key.as_str());
            parts.join("+")
        }
    }
}

/// Returns the wait mode requested by one terminal-session call.
/// 返回一次终端会话调用请求的等待模式。
fn terminal_wait_mode(wait_policy: Option<&TerminalSessionWaitPolicy>) -> TerminalSessionWaitMode {
    wait_policy
        .map(|policy| policy.mode)
        .unwrap_or(TerminalSessionWaitMode::ReturnImmediately)
}

/// Returns the bounded output wait budget for one wait policy.
/// 返回一份等待策略对应的有界输出等待预算。
fn terminal_wait_budget_ms(wait_policy: Option<&TerminalSessionWaitPolicy>) -> Option<u64> {
    let Some(policy) = wait_policy else {
        return Some(0);
    };
    Some(match policy.mode {
        TerminalSessionWaitMode::ReturnImmediately => 0,
        TerminalSessionWaitMode::UntilQuiet => policy.max_wait_ms.unwrap_or_else(|| {
            policy
                .quiet_ms
                .unwrap_or(300)
                .saturating_mul(10)
                .clamp(500, 5_000)
        }),
        TerminalSessionWaitMode::UntilPrompt => policy.max_wait_ms.unwrap_or(3_000),
        TerminalSessionWaitMode::UntilExitCode => policy.max_wait_ms.unwrap_or(120_000),
        TerminalSessionWaitMode::UntilPattern => {
            let has_pattern = policy
                .pattern
                .as_deref()
                .map(str::trim)
                .is_some_and(|pattern| !pattern.is_empty());
            policy
                .max_wait_ms
                .unwrap_or(if has_pattern { 30_000 } else { 3_000 })
        }
        TerminalSessionWaitMode::ScreenStable => policy.max_wait_ms.unwrap_or_else(|| {
            policy
                .quiet_ms
                .unwrap_or(150)
                .saturating_mul(6)
                .clamp(300, 2_000)
        }),
    })
}

/// Returns the effective quiet window used by quiet-style terminal waits.
/// 返回静默类终端等待使用的有效静默窗口。
fn terminal_wait_quiet_window_ms(wait_policy: Option<&TerminalSessionWaitPolicy>) -> u64 {
    wait_policy
        .and_then(|policy| policy.quiet_ms)
        .unwrap_or(300)
        .clamp(50, 5_000)
}

/// Returns the trimmed pattern requested by one wait policy.
/// 返回一份等待策略请求的裁剪后模式。
fn terminal_wait_pattern_text(wait_policy: Option<&TerminalSessionWaitPolicy>) -> Option<String> {
    wait_policy
        .and_then(|policy| policy.pattern.as_deref())
        .map(str::trim)
        .filter(|pattern| !pattern.is_empty())
        .map(String::from)
}

/// Reports whether the returned terminal chunks contain a literal pattern.
/// 判断返回的终端分片是否包含一个字面量模式。
fn terminal_output_chunks_contain_pattern(
    chunks: &[CommandSessionOutputChunk],
    pattern: &str,
) -> bool {
    chunks.iter().any(|chunk| chunk.text.contains(pattern))
        || chunks
            .iter()
            .map(|chunk| chunk.text.as_str())
            .collect::<String>()
            .contains(pattern)
}

/// Returns the newline sequence used to submit generated terminal commands through PTY input.
/// 返回通过 PTY 输入提交宿主生成终端命令时使用的换行序列。
fn terminal_generated_command_newline(shell_label: &str) -> &'static str {
    terminal_session_pty_newline(shell_label)
}

/// Returns the PTY-facing newline sequence used by generated terminal key/control input.
/// 返回宿主生成的终端按键或控制输入面向 PTY 使用的换行序列。
fn terminal_session_pty_newline(shell_label: &str) -> &'static str {
    let lower_shell = shell_label.to_ascii_lowercase();
    if cfg!(windows)
        && (lower_shell.contains("pwsh")
            || lower_shell.contains("powershell")
            || lower_shell.contains("cmd"))
    {
        "\r"
    } else {
        shell_specific_newline(shell_label)
    }
}

/// Returns the default backend used by terminal_session on the current host.
/// 返回当前宿主上 terminal_session 使用的默认后端。
fn default_terminal_session_backend() -> CommandSessionBackend {
    if cfg!(windows) {
        CommandSessionBackend::WindowsConsole
    } else {
        CommandSessionBackend::Pty
    }
}

/// Reports whether one input list can be safely wrapped as a command transaction.
/// 判断一组输入能否安全包装为命令事务。
fn terminal_transaction_command_text(
    inputs: &[TerminalSessionInput],
) -> Result<String, NativeToolError> {
    if inputs.len() != 1 {
        return Err(NativeToolError::new(
            "`until_exit_code` requires exactly one text input containing one complete shell command.",
        ));
    }
    let text = match &inputs[0] {
        TerminalSessionInput::Text { text } => text,
        TerminalSessionInput::Paste { .. } => {
            return Err(NativeToolError::new(
                "`until_exit_code` cannot be used with paste input. Use text for one shell command transaction or use until_quiet for pasted interactive content.",
            ));
        }
        TerminalSessionInput::Key { .. } | TerminalSessionInput::Chord { .. } => {
            return Err(NativeToolError::new(
                "`until_exit_code` cannot be used with key or chord input.",
            ));
        }
    };
    if !stdin_payload_has_trailing_newline(text) {
        return Err(NativeToolError::new(
            "`until_exit_code` requires a complete command ending with a newline.",
        ));
    }
    let command = text.trim().to_string();
    if command.is_empty() {
        return Err(NativeToolError::new(
            "`until_exit_code` requires a non-empty command.",
        ));
    }
    Ok(command)
}

/// Validates that a terminal is clean enough to start one command transaction.
/// 校验终端状态是否足够干净，可以启动一条命令事务。
fn validate_terminal_transaction_start(
    snapshot: &CommandSessionSnapshot,
) -> Result<(), NativeToolError> {
    if snapshot.pending_stdin_line {
        return Err(NativeToolError::new(
            "`until_exit_code` cannot run while the terminal has a pending input line. Send Ctrl+C or submit/clear the current line before starting a command transaction.",
        ));
    }
    Ok(())
}

/// Wraps one shell command so the host can observe the transaction exit code.
/// 包装一条 shell 命令，使宿主可以观测该事务退出码。
fn wrap_terminal_transaction_command(
    command: &str,
    shell_label: &str,
    transaction_id: &str,
    interactive_newline: &str,
) -> Result<String, NativeToolError> {
    let sentinel = format!("__VULCAN_TERMINAL_TX_EXIT__:{transaction_id}:");
    let lower_shell = shell_label.to_ascii_lowercase();
    if lower_shell.contains("pwsh") || lower_shell.contains("powershell") {
        return Ok(format!(
            "$global:LASTEXITCODE = $null; $vulcanErrorCount = $Error.Count; & {{ {command} }}; $vulcanSuccess = $?; $vulcanNativeCode = $LASTEXITCODE; $vulcanNewErrors = $Error.Count - $vulcanErrorCount; if ($null -ne $vulcanNativeCode) {{ $vulcanCode = $vulcanNativeCode }} elseif ($vulcanSuccess -and $vulcanNewErrors -le 0) {{ $vulcanCode = 0 }} else {{ $vulcanCode = 1 }}; Write-Output \"{sentinel}$vulcanCode\"{interactive_newline}"
        ));
    }
    if lower_shell.contains("cmd") {
        return Ok(format!(
            "{command}{interactive_newline}echo {sentinel}%ERRORLEVEL%{interactive_newline}"
        ));
    }
    if lower_shell.contains("bash") || lower_shell.contains("sh") || lower_shell.contains("zsh") {
        return Ok(format!(
            "{{ {command}; vulcan_code=$?; printf '\\n{sentinel}%s\\n' \"$vulcan_code\"; }}{interactive_newline}"
        ));
    }
    Err(NativeToolError::new(format!(
        "`until_exit_code` is not supported for shell `{shell_label}`. Use until_quiet or an explicit supported shell."
    )))
}

/// Resolves the effective newline-submission default for the `start` action.
/// 解析 `start` 动作的有效换行提交默认值。
#[cfg(test)]
fn effective_start_append_newline(
    requested_append_newline: Option<bool>,
    initial_stdin: Option<&str>,
) -> bool {
    requested_append_newline.unwrap_or(initial_stdin.is_some())
}

/// Validates whether `start` stdin is safe to submit without hidden concatenation across later writes.
/// 校验 `start` 的标准输入是否能安全提交，避免与后续写入发生隐式拼接。
#[cfg(test)]
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
fn validate_terminal_session_start_shell_policy(
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
            "terminal_session start requires an explicit `shell` when the command directly launches `bash`, `sh`, or `zsh` on Windows. `auto` resolves only the outer launcher and is rejected here to keep interactive newline behavior deterministic.",
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
#[cfg(test)]
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

/// Parses and validates VMM memory-delete arguments.
/// 解析并校验 VMM 记忆删除参数。
fn parse_vmm_memory_delete_arguments(
    arguments: Value,
) -> Result<VmmMemoryDeleteToolArgs, NativeToolError> {
    let mut parsed =
        serde_json::from_value::<VmmMemoryDeleteToolArgs>(arguments).map_err(|error| {
            NativeToolError::new(format!(
                "Failed to parse vmm_memory_delete arguments: {error}"
            ))
        })?;
    parsed.memory_ids = normalized_vmm_memory_delete_ids(parsed.memory_ids)?;
    parsed.reason = parsed.reason.trim().to_string();
    if parsed.reason.is_empty() {
        return Err(NativeToolError::new(
            "The `reason` argument for vmm_memory_delete cannot be empty.",
        ));
    }
    parsed.reason = parsed.reason.chars().take(1024).collect();
    Ok(parsed)
}

/// Normalizes memory-delete ids by removing zeroes and duplicates.
/// 通过移除零值和重复项归一化记忆删除标识。
fn normalized_vmm_memory_delete_ids(memory_ids: Vec<u64>) -> Result<Vec<u64>, NativeToolError> {
    let mut normalized = Vec::new();
    for memory_id in memory_ids {
        if memory_id == 0 || normalized.contains(&memory_id) {
            continue;
        }
        normalized.push(memory_id);
        if normalized.len() > 256 {
            return Err(NativeToolError::new(
                "The `memoryIds` argument for vmm_memory_delete cannot contain more than 256 ids.",
            ));
        }
    }
    if normalized.is_empty() {
        return Err(NativeToolError::new(
            "The `memoryIds` argument for vmm_memory_delete must contain positive memory ids.",
        ));
    }
    Ok(normalized)
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

/// Builds the host-only structured payload returned for one shell-session tool action.
/// 为单次 shell 会话工具动作构建宿主专用结构化返回载荷。
#[cfg(test)]
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

/// Converts command-session chunks into terminal-session event metadata.
/// 将命令会话分片转换为终端会话事件元数据。
fn terminal_events_from_chunks(chunks: &[CommandSessionOutputChunk]) -> Value {
    Value::Array(
        chunks
            .iter()
            .map(|chunk| {
                json!({
                    "seq": chunk.seq,
                    "kind": "output",
                    "stream": chunk.stream.as_str(),
                    "text": chunk.text,
                })
            })
            .collect(),
    )
}

/// Coalesces adjacent output chunks from the same stream for compact model-visible output.
/// 将同一输出流的相邻分片合并为更紧凑的模型可见输出。
fn coalesce_terminal_output_chunks(
    chunks: Vec<CommandSessionOutputChunk>,
) -> Vec<CommandSessionOutputChunk> {
    let mut coalesced: Vec<CommandSessionOutputChunk> = Vec::new();
    for chunk in chunks {
        if let Some(last_chunk) = coalesced.last_mut() {
            if last_chunk.stream == chunk.stream
                && last_chunk.text.len().saturating_add(chunk.text.len())
                    <= TERMINAL_OUTPUT_CHUNK_COALESCE_SOFT_LIMIT_BYTES
            {
                last_chunk.text.push_str(&chunk.text);
                continue;
            }
        }
        coalesced.push(chunk);
    }
    coalesced
}

/// Removes host transaction sentinel lines from chunks and returns the parsed exit code if present.
/// 从分片中移除宿主事务哨兵行，并在存在时返回解析出的退出码。
fn strip_terminal_transaction_sentinel(
    chunks: &[CommandSessionOutputChunk],
    transaction_id: &str,
) -> (Vec<CommandSessionOutputChunk>, Option<i32>) {
    let sentinel_prefixes = terminal_transaction_sentinel_prefixes(transaction_id);
    let mut exit_code = None;
    let mut stripped_chunks = Vec::new();

    for chunk in chunks {
        let mut stripped_text = String::new();
        for line in chunk.text.split_inclusive('\n') {
            let line_without_newline = line.trim_end_matches(['\r', '\n']);
            let detection_line = strip_simple_ansi_sequences(line_without_newline);
            if let Some((sentinel_start, sentinel_len)) =
                find_terminal_transaction_sentinel(&detection_line, &sentinel_prefixes)
            {
                let raw_code = &detection_line[sentinel_start + sentinel_len..];
                let raw_code = raw_code
                    .trim_start()
                    .chars()
                    .take_while(|ch| ch.is_ascii_digit() || *ch == '-' || *ch == '+')
                    .collect::<String>();
                if let Ok(parsed_code) = raw_code.parse::<i32>() {
                    exit_code = Some(parsed_code);
                }
                continue;
            }
            stripped_text.push_str(line);
        }
        if !stripped_text.is_empty() {
            let mut stripped_chunk = chunk.clone();
            stripped_chunk.text = stripped_text;
            stripped_chunks.push(stripped_chunk);
        }
    }

    (stripped_chunks, exit_code)
}

/// Returns exact transaction sentinel prefixes accepted for append logs and screen frames.
/// 返回追加日志与屏幕帧中可接受的精确事务哨兵前缀。
fn terminal_transaction_sentinel_prefixes(transaction_id: &str) -> [String; 2] {
    [
        format!("__VULCAN_TERMINAL_TX_EXIT__:{transaction_id}:"),
        format!("_TERMINAL_TX_EXIT__:{transaction_id}:"),
    ]
}

/// Finds one exact transaction sentinel prefix inside a decoded terminal line.
/// 在已解码终端行中查找一个精确事务哨兵前缀。
fn find_terminal_transaction_sentinel(
    line: &str,
    sentinel_prefixes: &[String],
) -> Option<(usize, usize)> {
    sentinel_prefixes
        .iter()
        .filter_map(|prefix| line.find(prefix).map(|start| (start, prefix.len())))
        .min_by_key(|(start, _)| *start)
}

/// Detects whether terminal output appears log-like, screen-like, or mixed.
/// 检测终端输出看起来是日志型、屏幕型还是混合型。
fn detect_terminal_output_mode(
    chunks: &[CommandSessionOutputChunk],
) -> (&'static str, f64, String) {
    let combined = chunks
        .iter()
        .map(|chunk| chunk.text.as_str())
        .collect::<Vec<_>>()
        .join("");
    if combined.contains("\x1b[?1049h")
        || combined.contains("\x1b[?47h")
        || combined.contains("\x1b[2J")
        || combined.contains("\x1b[H")
    {
        return (
            "screen",
            0.82,
            String::from("ANSI alternate-screen or cursor-control sequence detected."),
        );
    }
    let escape_count = combined.matches('\x1b').count();
    if escape_count >= 8 {
        return (
            "mixed",
            0.64,
            String::from("Multiple ANSI control sequences detected."),
        );
    }
    (
        "log",
        0.9,
        String::from("Output appears to be append-style text."),
    )
}

/// Removes simple ANSI control sequences for screen-text fallback rendering.
/// 移除简单 ANSI 控制序列以便生成屏幕文本回退内容。
fn strip_simple_ansi_sequences(text: &str) -> String {
    let mut output = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\x1b' {
            output.push(ch);
            continue;
        }
        if chars.peek() == Some(&'[') {
            let _ = chars.next();
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() || matches!(next, '~' | '@') {
                    break;
                }
            }
        }
    }
    output
}

/// Returns whether a drained interrupt chunk contains model-visible content.
/// 返回中断排空分片是否包含模型应看见的内容。
fn terminal_interrupt_drain_chunk_is_visible(chunk: &CommandSessionOutputChunk) -> bool {
    let text_without_ansi = strip_simple_ansi_sequences(&chunk.text);
    let visible_text =
        text_without_ansi.trim_matches(|ch: char| ch.is_control() || ch.is_whitespace());
    !visible_text.is_empty()
}

/// Builds plain screen text for screen-like terminal output when available.
/// 在存在屏幕型终端输出时构建纯文本屏幕内容。
fn terminal_screen_text_from_chunks(
    output_mode: &str,
    chunks: &[CommandSessionOutputChunk],
) -> Option<String> {
    if output_mode != "screen" && output_mode != "mixed" {
        return None;
    }
    let screen_text = strip_simple_ansi_sequences(
        &chunks
            .iter()
            .map(|chunk| chunk.text.as_str())
            .collect::<Vec<_>>()
            .join(""),
    );
    (!screen_text.trim().is_empty()).then_some(screen_text)
}

/// Builds a compact controls hint for terminal-session host metadata.
/// 为终端会话宿主元数据构建紧凑控制提示。
fn terminal_controls_hint(output_mode: &str) -> Value {
    if output_mode == "screen" || output_mode == "mixed" {
        json!({
            "preferred_input": "keys",
            "text_input_supported": true,
            "keys_supported": true,
            "paste_supported": true,
            "common_keys": ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Tab", "Enter", "Esc"],
            "common_chords": [{ "key": "c", "ctrl": true }],
            "note": "Screen-like output detected. Prefer keys/chords and snapshots for navigation."
        })
    } else {
        json!({
            "preferred_input": "text",
            "text_input_supported": true,
            "keys_supported": true,
            "paste_supported": true,
            "common_keys": ["Enter", "Tab", "ArrowUp", "ArrowDown"],
            "common_chords": [{ "key": "c", "ctrl": true }]
        })
    }
}

/// Returns a compact model-visible controls hint line for screen-like output.
/// 返回一条面向模型的紧凑屏幕控制提示。
fn terminal_controls_hint_line(output_mode: &str) -> Option<&'static str> {
    if output_mode == "screen" || output_mode == "mixed" {
        Some(
            "Prefer snapshot/screen_stable with key or chord inputs; avoid treating ANSI output as an append-only log.",
        )
    } else {
        None
    }
}

/// Builds backend-specific terminal input capability metadata.
/// 构建后端特定的终端输入能力元数据。
fn terminal_input_capabilities(snapshot: &CommandSessionSnapshot) -> Value {
    match snapshot.backend {
        CommandSessionBackend::WindowsConsole => json!({
            "text": true,
            "paste": true,
            "key": true,
            "chord": true,
            "native_console_events": true,
            "common_keys": ["Enter", "Tab", "Esc", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Delete", "F1"],
        }),
        CommandSessionBackend::Pty => json!({
            "text": true,
            "paste": true,
            "key": true,
            "chord": true,
            "native_console_events": false,
            "common_keys": ["Enter", "Tab", "Esc", "ArrowUp", "ArrowDown"],
        }),
        CommandSessionBackend::Pipe => json!({
            "text": true,
            "paste": true,
            "key": false,
            "chord": false,
            "native_console_events": false,
        }),
    }
}

/// Builds one model-visible text render input for terminal-session actions.
/// 为终端会话动作构建一份模型可见文本渲染输入。
fn build_terminal_session_render_input(
    action: TerminalSessionToolAction,
    snapshot: &CommandSessionSnapshot,
    session_name: &str,
    returned_chunks: &[CommandSessionOutputChunk],
    has_more: bool,
    output_mode: &str,
    transaction: Option<&TerminalTransactionResult>,
    extra_lines: Vec<String>,
) -> HostTextRenderInput {
    let mut lines = vec![
        format!("Action: {}", action.as_str()),
        format!("Terminal session: {}", snapshot.command_session_id),
        format!("Name: {session_name}"),
        format!("Owner session: {}", snapshot.owner_session_id),
        format!("Command: {}", snapshot.command),
        format!("Shell: {}", snapshot.shell_label),
        format!("Backend: {}", snapshot.backend.as_str()),
        format!(
            "Working directory: {}",
            normalized_workspace_path_text_from_path(&snapshot.cwd)
        ),
        format!("Status: {}", snapshot.status.as_str()),
        format!("Output mode: {output_mode}"),
        format!("Last sequence: {}", snapshot.last_seq),
        format!("Returned chunks: {}", returned_chunks.len()),
        format!(
            "Has more buffered output: {}",
            if has_more { "yes" } else { "no" }
        ),
    ];
    if let Some(exit_code) = snapshot.exit_code {
        lines.push(format!("Session exit code: {exit_code}"));
    }
    if let Some(reason) = snapshot.failure_reason.as_deref() {
        lines.push(format!("Reason: {reason}"));
    }
    if let Some(transaction) = transaction {
        lines.push(format!("Transaction: {}", transaction.id));
        lines.push(format!("Transaction command: {}", transaction.command));
        lines.push(format!("Transaction status: {}", transaction.status));
        lines.push(format!(
            "Transaction exit code: {}",
            transaction
                .exit_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| String::from("unknown"))
        ));
    }
    let screen_text = snapshot
        .screen_text
        .clone()
        .or_else(|| terminal_screen_text_from_chunks(output_mode, returned_chunks));
    if screen_text.is_some() {
        lines.push(String::from("Screen text: present"));
    }
    if let Some(hint) = terminal_controls_hint_line(output_mode) {
        lines.push(format!("Controls hint: {hint}"));
    }
    lines.extend(extra_lines);
    let mut content = lines.join("\n");
    if let Some(screen_text) = screen_text {
        content.push_str("\n\n[screen_text]\n");
        content.push_str(&screen_text);
    }
    if output_mode == "screen" || output_mode == "mixed" {
        let controls_hint = serde_json::to_string_pretty(&terminal_controls_hint(output_mode))
            .unwrap_or_else(|_| terminal_controls_hint(output_mode).to_string());
        content.push_str("\n\n[controls_hint]\n");
        content.push_str(&controls_hint);
    }
    if returned_chunks.is_empty() {
        content.push_str("\n\n(no new output)");
    } else {
        for chunk in returned_chunks {
            content.push_str(&format!(
                "\n\n[{} #{}]\n{}",
                chunk.stream.as_str(),
                chunk.seq,
                chunk.text
            ));
        }
    }
    HostTextRenderInput {
        content_bytes: content.len(),
        content_lines: content.lines().count(),
        content,
        overflow_mode: Some(HostToolOverflowMode::Page),
        template_hint: None,
    }
}

/// Builds the host-only structured payload returned for one terminal-session tool action.
/// 为单次终端会话工具动作构建宿主专用结构化返回载荷。
fn build_terminal_session_host_payload(
    action: TerminalSessionToolAction,
    snapshot: &CommandSessionSnapshot,
    session_name: &str,
    returned_chunks: &[CommandSessionOutputChunk],
    output_overflowed: bool,
    output_mode: &str,
    output_mode_confidence: f64,
    output_mode_reason: String,
    transaction: Option<&TerminalTransactionResult>,
    extra_fields: Value,
) -> Value {
    let screen_text = snapshot
        .screen_text
        .clone()
        .or_else(|| terminal_screen_text_from_chunks(output_mode, returned_chunks));
    let mut payload = json!({
        "action": action.as_str(),
        "owner_session_id": snapshot.owner_session_id.to_string(),
        "terminal_session_id": snapshot.command_session_id.to_string(),
        "command_session_id": snapshot.command_session_id.to_string(),
        "name": session_name,
        "command": snapshot.command.clone(),
        "cwd": normalized_workspace_path_text_from_path(&snapshot.cwd),
        "shell": snapshot.shell_label.clone(),
        "launcher_shell": snapshot.launcher_shell_label.clone(),
        "backend": snapshot.backend.as_str(),
        "input_capabilities": terminal_input_capabilities(snapshot),
        "status": snapshot.status.as_str(),
        "exit_code": snapshot.exit_code,
        "stdout_bytes": snapshot.stdout_bytes,
        "stderr_bytes": snapshot.stderr_bytes,
        "last_seq": snapshot.last_seq,
        "pending_stdin_line": snapshot.pending_stdin_line,
        "screen_frame_seq": snapshot.screen_frame_seq,
        "screen_size": {
            "rows": snapshot.screen_rows,
            "cols": snapshot.screen_cols,
        },
        "failure_reason": snapshot.failure_reason.clone(),
        "output_overflowed": output_overflowed,
        "output_mode": output_mode,
        "output_mode_confidence": output_mode_confidence,
        "output_mode_reason": output_mode_reason,
        "events": terminal_events_from_chunks(returned_chunks),
        "screen_text": screen_text,
        "controls_hint": terminal_controls_hint(output_mode),
    });
    if let Some(transaction) = transaction {
        if let Some(payload_object) = payload.as_object_mut() {
            payload_object.insert(
                String::from("transaction"),
                json!({
                    "id": transaction.id,
                    "command": transaction.command,
                    "exit_code": transaction.exit_code,
                    "status": transaction.status,
                    "started_seq": transaction.started_seq,
                    "ended_seq": transaction.ended_seq,
                }),
            );
        }
    }
    if let (Some(payload_object), Value::Object(extra_object)) =
        (payload.as_object_mut(), extra_fields)
    {
        payload_object.extend(extra_object);
    }
    payload
}

/// Reads terminal output once through the AI-facing cursor and returns detected output mode data.
/// 通过面向 AI 的游标读取一次终端输出，并返回检测到的输出模式数据。
async fn read_terminal_session_output_for_ai(
    command_session_runtime: &Arc<CommandSessionRuntime>,
    owner_session_id: SessionId,
    terminal_session_id: SessionId,
    after_seq: Option<u64>,
    read_from_start: bool,
    wait_for_output_ms: Option<u64>,
    max_chunks: Option<usize>,
) -> Result<CommandSessionReadOutcome, NativeToolError> {
    command_session_runtime
        .read_session_for_ai(
            owner_session_id,
            terminal_session_id,
            after_seq,
            read_from_start,
            wait_for_output_ms,
            max_chunks,
        )
        .await
        .map_err(NativeToolError::new)
}

/// Peeks terminal output without advancing the AI-facing cursor.
/// 窥视终端输出，但不推进面向 AI 的读取游标。
async fn peek_terminal_session_output_for_ai(
    command_session_runtime: &Arc<CommandSessionRuntime>,
    owner_session_id: SessionId,
    terminal_session_id: SessionId,
    after_seq: Option<u64>,
    read_from_start: bool,
    wait_for_output_ms: Option<u64>,
    max_chunks: Option<usize>,
) -> Result<CommandSessionReadOutcome, NativeToolError> {
    command_session_runtime
        .peek_session_for_ai(
            owner_session_id,
            terminal_session_id,
            after_seq,
            read_from_start,
            wait_for_output_ms,
            max_chunks,
        )
        .await
        .map_err(NativeToolError::new)
}

/// Returns remaining milliseconds for one bounded terminal wait loop.
/// 返回一次有界终端等待循环的剩余毫秒数。
fn remaining_terminal_wait_ms(started_at: Instant, wait_budget_ms: u64) -> u64 {
    wait_budget_ms.saturating_sub(started_at.elapsed().as_millis() as u64)
}

/// Reads terminal output according to the requested wait policy.
/// 按请求的等待策略读取终端输出。
async fn read_terminal_session_output_with_wait(
    command_session_runtime: &Arc<CommandSessionRuntime>,
    owner_session_id: SessionId,
    terminal_session_id: SessionId,
    after_seq: Option<u64>,
    read_from_start: bool,
    wait_policy: Option<&TerminalSessionWaitPolicy>,
    max_chunks: Option<usize>,
) -> Result<TerminalWaitReadResult, NativeToolError> {
    let wait_mode = terminal_wait_mode(wait_policy);
    let wait_budget_ms = terminal_wait_budget_ms(wait_policy).unwrap_or(0);
    let started_at = Instant::now();

    if wait_mode == TerminalSessionWaitMode::ReturnImmediately {
        let read_outcome = read_terminal_session_output_for_ai(
            command_session_runtime,
            owner_session_id,
            terminal_session_id,
            after_seq,
            read_from_start,
            Some(0),
            max_chunks,
        )
        .await?;
        return Ok(TerminalWaitReadResult {
            snapshot: read_outcome.snapshot,
            returned_chunks: read_outcome.returned_chunks,
            has_more: read_outcome.has_more,
            elapsed_ms: started_at.elapsed().as_millis() as u64,
            pattern_matched: None,
            quiet_observed: None,
        });
    }

    if wait_mode == TerminalSessionWaitMode::UntilPattern {
        let pattern = terminal_wait_pattern_text(wait_policy);
        let mut returned_chunks = Vec::new();
        let mut has_more = false;
        let mut latest_outcome = read_terminal_session_output_for_ai(
            command_session_runtime,
            owner_session_id,
            terminal_session_id,
            after_seq,
            read_from_start,
            Some(wait_budget_ms.min(500)),
            max_chunks,
        )
        .await?;
        returned_chunks.extend(latest_outcome.returned_chunks.clone());
        has_more |= latest_outcome.has_more;
        let mut pattern_matched = pattern.as_deref().is_some_and(|pattern| {
            terminal_output_chunks_contain_pattern(&returned_chunks, pattern)
        });

        while !pattern_matched && remaining_terminal_wait_ms(started_at, wait_budget_ms) > 0 {
            if latest_outcome.snapshot.status.as_str() != "running" {
                break;
            }
            let remaining_ms = remaining_terminal_wait_ms(started_at, wait_budget_ms);
            latest_outcome = read_terminal_session_output_for_ai(
                command_session_runtime,
                owner_session_id,
                terminal_session_id,
                None,
                false,
                Some(remaining_ms.min(500)),
                max_chunks,
            )
            .await?;
            returned_chunks.extend(latest_outcome.returned_chunks.clone());
            has_more |= latest_outcome.has_more;
            pattern_matched = pattern.as_deref().is_some_and(|pattern| {
                terminal_output_chunks_contain_pattern(&returned_chunks, pattern)
            });
            if latest_outcome.returned_chunks.is_empty() && remaining_ms <= 500 {
                break;
            }
        }

        return Ok(TerminalWaitReadResult {
            snapshot: latest_outcome.snapshot,
            returned_chunks,
            has_more,
            elapsed_ms: started_at.elapsed().as_millis() as u64,
            pattern_matched: Some(pattern_matched),
            quiet_observed: None,
        });
    }

    let quiet_ms = terminal_wait_quiet_window_ms(wait_policy);
    let mut returned_chunks = Vec::new();
    let mut has_more = false;
    let mut quiet_observed = false;
    let mut latest_outcome = read_terminal_session_output_for_ai(
        command_session_runtime,
        owner_session_id,
        terminal_session_id,
        after_seq,
        read_from_start,
        Some(quiet_ms.min(wait_budget_ms)),
        max_chunks,
    )
    .await?;
    loop {
        has_more |= latest_outcome.has_more;
        if latest_outcome.returned_chunks.is_empty() {
            quiet_observed = true;
            break;
        }
        returned_chunks.extend(latest_outcome.returned_chunks.clone());
        if latest_outcome.snapshot.status.as_str() != "running" {
            break;
        }
        let remaining_ms = remaining_terminal_wait_ms(started_at, wait_budget_ms);
        if remaining_ms == 0 {
            break;
        }
        latest_outcome = read_terminal_session_output_for_ai(
            command_session_runtime,
            owner_session_id,
            terminal_session_id,
            None,
            false,
            Some(quiet_ms.min(remaining_ms)),
            max_chunks,
        )
        .await?;
    }

    Ok(TerminalWaitReadResult {
        snapshot: latest_outcome.snapshot,
        returned_chunks,
        has_more,
        elapsed_ms: started_at.elapsed().as_millis() as u64,
        pattern_matched: None,
        quiet_observed: Some(quiet_observed),
    })
}

/// Drains residual output after Ctrl+C until the terminal is quiet or the bounded budget expires.
/// 在 Ctrl+C 后持续排空残留输出，直到终端静默或有界预算耗尽。
async fn drain_terminal_interrupt_output(
    command_session_runtime: &Arc<CommandSessionRuntime>,
    owner_session_id: SessionId,
    terminal_session_id: SessionId,
    after_seq: u64,
    max_chunks: Option<usize>,
) -> Result<TerminalInterruptDrainResult, NativeToolError> {
    let started_at = Instant::now();
    let mut next_after_seq = Some(after_seq);
    let mut returned_chunks = Vec::new();
    let mut discarded_chunk_count = 0usize;
    let mut latest_outcome = read_terminal_session_output_for_ai(
        command_session_runtime,
        owner_session_id,
        terminal_session_id,
        next_after_seq,
        false,
        Some(TERMINAL_INTERRUPT_DRAIN_QUIET_MS),
        max_chunks,
    )
    .await?;
    loop {
        for chunk in latest_outcome.returned_chunks.iter().cloned() {
            if terminal_interrupt_drain_chunk_is_visible(&chunk) {
                returned_chunks.push(chunk);
            } else {
                discarded_chunk_count = discarded_chunk_count.saturating_add(1);
            }
        }

        let quiet_after_wait = latest_outcome.returned_chunks.is_empty();
        if quiet_after_wait {
            break;
        }
        if latest_outcome.snapshot.status.as_str() != "running" {
            break;
        }
        if started_at.elapsed() >= Duration::from_millis(TERMINAL_INTERRUPT_DRAIN_MAX_MS) {
            break;
        }

        next_after_seq = None;
        latest_outcome = read_terminal_session_output_for_ai(
            command_session_runtime,
            owner_session_id,
            terminal_session_id,
            next_after_seq,
            false,
            Some(TERMINAL_INTERRUPT_DRAIN_QUIET_MS),
            max_chunks,
        )
        .await?;
    }

    Ok(TerminalInterruptDrainResult {
        snapshot: latest_outcome.snapshot,
        returned_chunks,
        has_more: latest_outcome.has_more,
        elapsed_ms: started_at.elapsed().as_millis() as u64,
        discarded_chunk_count,
    })
}

/// Flushes a possible leftover interrupt control character from the shell input line.
/// 从 shell 输入行中冲刷可能残留的中断控制字符。
async fn cleanup_terminal_interrupt_input_line(
    command_session_runtime: &Arc<CommandSessionRuntime>,
    owner_session_id: SessionId,
    terminal_session_id: SessionId,
    snapshot: CommandSessionSnapshot,
    interactive_newline: &str,
    max_chunks: Option<usize>,
) -> TerminalInterruptCleanupResult {
    if snapshot.status.as_str() != "running" || !snapshot.pending_stdin_line {
        return TerminalInterruptCleanupResult {
            snapshot,
            has_more: false,
            elapsed_ms: 0,
            discarded_chunk_count: 0,
            failure_reason: None,
        };
    }

    let started_at = Instant::now();
    let after_seq = snapshot.last_seq;
    let write_outcome = match command_session_runtime
        .write_session(
            owner_session_id,
            terminal_session_id,
            interactive_newline.to_string(),
            false,
        )
        .await
    {
        Ok(outcome) => outcome,
        Err(error) => {
            return TerminalInterruptCleanupResult {
                snapshot,
                has_more: false,
                elapsed_ms: started_at.elapsed().as_millis() as u64,
                discarded_chunk_count: 0,
                failure_reason: Some(error),
            };
        }
    };

    let mut discarded_chunk_count = 0usize;
    let mut has_more = false;
    let mut latest_snapshot = write_outcome.snapshot;
    let mut next_after_seq = Some(after_seq);
    loop {
        let read_outcome = match read_terminal_session_output_for_ai(
            command_session_runtime,
            owner_session_id,
            terminal_session_id,
            next_after_seq,
            false,
            Some(TERMINAL_INTERRUPT_DRAIN_QUIET_MS),
            max_chunks,
        )
        .await
        {
            Ok(outcome) => outcome,
            Err(error) => {
                return TerminalInterruptCleanupResult {
                    snapshot: latest_snapshot,
                    has_more,
                    elapsed_ms: started_at.elapsed().as_millis() as u64,
                    discarded_chunk_count,
                    failure_reason: Some(error.message),
                };
            }
        };
        let chunk_count = read_outcome.returned_chunks.len();
        discarded_chunk_count = discarded_chunk_count.saturating_add(chunk_count);
        has_more |= read_outcome.has_more;
        latest_snapshot = read_outcome.snapshot;
        if chunk_count == 0 {
            break;
        }
        if latest_snapshot.status.as_str() != "running" {
            break;
        }
        if started_at.elapsed() >= Duration::from_millis(TERMINAL_INTERRUPT_DRAIN_MAX_MS) {
            break;
        }
        next_after_seq = None;
    }

    TerminalInterruptCleanupResult {
        snapshot: latest_snapshot,
        has_more,
        elapsed_ms: started_at.elapsed().as_millis() as u64,
        discarded_chunk_count,
        failure_reason: None,
    }
}

/// Reads terminal output until a command transaction sentinel is observed or the wait budget expires.
/// 读取终端输出直到观测到命令事务哨兵或等待预算耗尽。
async fn read_terminal_transaction_output(
    command_session_runtime: &Arc<CommandSessionRuntime>,
    owner_session_id: SessionId,
    terminal_session_id: SessionId,
    after_seq: u64,
    wait_budget_ms: u64,
    max_chunks: Option<usize>,
    transaction_id: &str,
) -> Result<
    (
        CommandSessionReadOutcome,
        Vec<CommandSessionOutputChunk>,
        Option<i32>,
    ),
    NativeToolError,
> {
    let started_at = Instant::now();
    let mut all_chunks = Vec::new();
    let mut latest_outcome = read_terminal_session_output_for_ai(
        command_session_runtime,
        owner_session_id,
        terminal_session_id,
        Some(after_seq),
        false,
        Some(wait_budget_ms.min(500)),
        max_chunks,
    )
    .await?;
    all_chunks.extend(latest_outcome.returned_chunks.clone());
    let coalesced_chunks = coalesce_terminal_output_chunks(all_chunks.clone());
    let (stripped_chunks, exit_code) =
        strip_terminal_transaction_sentinel(&coalesced_chunks, transaction_id);
    if exit_code.is_some() {
        return Ok((latest_outcome, stripped_chunks, exit_code));
    }

    while started_at.elapsed().as_millis() < wait_budget_ms as u128 {
        let elapsed_ms = started_at.elapsed().as_millis() as u64;
        let remaining_ms = wait_budget_ms.saturating_sub(elapsed_ms);
        if remaining_ms == 0 {
            break;
        }
        latest_outcome = read_terminal_session_output_for_ai(
            command_session_runtime,
            owner_session_id,
            terminal_session_id,
            None,
            false,
            Some(remaining_ms.min(500)),
            max_chunks,
        )
        .await?;
        if latest_outcome.returned_chunks.is_empty() {
            if latest_outcome.snapshot.status.as_str() != "running" {
                break;
            }
            continue;
        }
        all_chunks.extend(latest_outcome.returned_chunks.clone());
        let coalesced_chunks = coalesce_terminal_output_chunks(all_chunks.clone());
        let (stripped_chunks, exit_code) =
            strip_terminal_transaction_sentinel(&coalesced_chunks, transaction_id);
        if exit_code.is_some() {
            return Ok((latest_outcome, stripped_chunks, exit_code));
        }
    }

    let coalesced_chunks = coalesce_terminal_output_chunks(all_chunks);
    let (stripped_chunks, exit_code) =
        strip_terminal_transaction_sentinel(&coalesced_chunks, transaction_id);
    Ok((latest_outcome, stripped_chunks, exit_code))
}

/// Executes one terminal-session list action and returns all terminals owned by this controller session.
/// 执行一次终端会话 list 动作，并返回当前控制器会话拥有的全部终端。
async fn execute_terminal_session_list_action(
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
async fn execute_terminal_session_start_action(
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

/// Executes one terminal-session read or snapshot action.
/// 执行一次终端会话 read 或 snapshot 动作。
async fn execute_terminal_session_read_like_action(
    request: NativeToolExecutionRequest,
    parsed_args: TerminalSessionToolArgs,
    owner_session_id: SessionId,
    command_session_runtime: Arc<CommandSessionRuntime>,
    action: TerminalSessionToolAction,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let terminal_session_id =
        parse_terminal_session_id(parsed_args.terminal_session_id.as_deref())?;
    let spill_root = resolve_shell_command_spill_root(
        request.spill_root.as_deref(),
        request.workspace_root.as_deref(),
        request
            .workspace_root
            .as_deref()
            .unwrap_or_else(|| Path::new(".")),
    );
    let read_from_start = parsed_args.read_from_start.unwrap_or(false);
    let wait_result = if matches!(action, TerminalSessionToolAction::Snapshot) {
        let read_outcome = peek_terminal_session_output_for_ai(
            &command_session_runtime,
            owner_session_id,
            terminal_session_id,
            parsed_args.after_seq,
            true,
            Some(0),
            parsed_args.max_chunks,
        )
        .await?;
        TerminalWaitReadResult {
            snapshot: read_outcome.snapshot,
            returned_chunks: Vec::new(),
            has_more: read_outcome.has_more,
            elapsed_ms: 0,
            pattern_matched: None,
            quiet_observed: None,
        }
    } else {
        read_terminal_session_output_with_wait(
            &command_session_runtime,
            owner_session_id,
            terminal_session_id,
            parsed_args.after_seq,
            read_from_start,
            parsed_args.wait_policy.as_ref(),
            parsed_args.max_chunks,
        )
        .await?
    };
    let session_name = terminal_session_name_for_snapshot(&wait_result.snapshot);
    let returned_chunks = coalesce_terminal_output_chunks(wait_result.returned_chunks.clone());
    let (output_mode, confidence, reason) = detect_terminal_output_mode(&returned_chunks);
    let render_input = build_terminal_session_render_input(
        action,
        &wait_result.snapshot,
        &session_name,
        &returned_chunks,
        wait_result.has_more,
        output_mode,
        None,
        vec![
            format!(
                "Wait mode: {}",
                terminal_wait_mode(parsed_args.wait_policy.as_ref()).as_str()
            ),
            format!("Wait elapsed: {} ms", wait_result.elapsed_ms),
            format!(
                "Pattern matched: {}",
                wait_result
                    .pattern_matched
                    .map(|matched| matched.to_string())
                    .unwrap_or_else(|| String::from("n/a"))
            ),
            format!(
                "Quiet observed: {}",
                wait_result
                    .quiet_observed
                    .map(|observed| observed.to_string())
                    .unwrap_or_else(|| String::from("n/a"))
            ),
        ],
    );
    let budget = default_shell_command_budget_snapshot();
    let host_payload = build_terminal_session_host_payload(
        action,
        &wait_result.snapshot,
        &session_name,
        &returned_chunks,
        shell_command_output_exceeds_inline_budget(&render_input, &budget),
        output_mode,
        confidence,
        reason,
        None,
        json!({
            "requested_after_seq": parsed_args.after_seq.unwrap_or(0),
            "read_from_start": read_from_start,
            "returned_chunk_count": returned_chunks.len(),
            "has_more": wait_result.has_more,
            "wait_elapsed_ms": wait_result.elapsed_ms,
            "pattern_matched": wait_result.pattern_matched,
            "quiet_observed": wait_result.quiet_observed,
            "snapshot_advanced_ai_cursor": !matches!(action, TerminalSessionToolAction::Snapshot),
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

/// Executes one terminal-session send action.
/// 执行一次终端会话 send 动作。
async fn execute_terminal_session_send_action(
    request: NativeToolExecutionRequest,
    parsed_args: TerminalSessionToolArgs,
    owner_session_id: SessionId,
    command_session_runtime: Arc<CommandSessionRuntime>,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let terminal_session_id =
        parse_terminal_session_id(parsed_args.terminal_session_id.as_deref())?;
    let before_snapshot = command_session_runtime
        .try_snapshot_owned_session(owner_session_id, terminal_session_id)
        .ok_or_else(|| {
            NativeToolError::new(format!(
                "Terminal session `{terminal_session_id}` was not found."
            ))
        })?;
    let session_name = terminal_session_name_for_snapshot(&before_snapshot);
    let inputs = parsed_args.inputs.clone().ok_or_else(|| {
        NativeToolError::new("The `send` action for terminal_session requires `inputs`.")
    })?;
    if inputs.is_empty() {
        return Err(NativeToolError::new(
            "The `inputs` array for terminal_session send cannot be empty.",
        ));
    }
    let wait_mode = terminal_wait_mode(parsed_args.wait_policy.as_ref());
    let interactive_newline = shell_specific_newline(before_snapshot.shell_label.as_str());
    let requested_key_encoding = parsed_args.key_encoding;
    let key_encoding = requested_key_encoding.unwrap_or(TerminalSessionKeyEncoding::AnsiVt);
    let key_encoding_label =
        terminal_effective_key_encoding_label(before_snapshot.backend, requested_key_encoding);
    let uses_structured_input = before_snapshot.backend == CommandSessionBackend::WindowsConsole;
    let mut structured_input = if uses_structured_input {
        Some(windows_console_input_events_from_inputs(
            &inputs,
            interactive_newline,
            requested_key_encoding,
        )?)
    } else {
        None
    };
    let mut encoded_input = if uses_structured_input {
        Vec::new()
    } else {
        encode_terminal_inputs(&inputs, interactive_newline, key_encoding)?
    };
    let interrupt_requested = terminal_inputs_request_interrupt(&inputs);
    let mut transaction = None;
    if wait_mode == TerminalSessionWaitMode::UntilExitCode {
        validate_terminal_transaction_start(&before_snapshot)?;
        let command = terminal_transaction_command_text(&inputs)?;
        let transaction_id = Uuid::new_v4().to_string();
        let wrapped_command = wrap_terminal_transaction_command(
            &command,
            before_snapshot.shell_label.as_str(),
            transaction_id.as_str(),
            terminal_generated_command_newline(before_snapshot.shell_label.as_str()),
        )?;
        if uses_structured_input {
            structured_input = Some(vec![CommandSessionInputEvent::Text {
                text: wrapped_command,
            }]);
        } else {
            encoded_input = wrapped_command.into_bytes();
        }
        transaction = Some(TerminalTransactionResult {
            id: transaction_id,
            command,
            exit_code: None,
            status: String::from("pending"),
            started_seq: before_snapshot.last_seq,
            ended_seq: None,
        });
    }
    let write_outcome = if uses_structured_input {
        command_session_runtime
            .write_session_input_events(
                owner_session_id,
                terminal_session_id,
                structured_input.unwrap_or_default(),
            )
            .await
            .map_err(NativeToolError::new)?
    } else {
        command_session_runtime
            .write_session_bytes(owner_session_id, terminal_session_id, encoded_input)
            .await
            .map_err(NativeToolError::new)?
    };
    let requested_wait_budget =
        terminal_wait_budget_ms(parsed_args.wait_policy.as_ref()).unwrap_or(0);
    let wait_budget = requested_wait_budget;
    let mut interrupt_drain_elapsed_ms = 0u64;
    let mut interrupt_drain_discarded_chunks = 0usize;
    let mut interrupt_cleanup_elapsed_ms = 0u64;
    let mut interrupt_cleanup_discarded_chunks = 0usize;
    let mut interrupt_cleanup_failure: Option<String> = None;
    let response_wait_started_at = Instant::now();
    let (
        read_snapshot,
        returned_chunks,
        has_more,
        wait_elapsed_ms,
        pattern_matched,
        quiet_observed,
    ) = if let Some(transaction_ref) = transaction.as_mut() {
        let (read_outcome, stripped_chunks, exit_code) = read_terminal_transaction_output(
            &command_session_runtime,
            owner_session_id,
            terminal_session_id,
            before_snapshot.last_seq,
            wait_budget,
            parsed_args.max_chunks,
            transaction_ref.id.as_str(),
        )
        .await?;
        transaction_ref.exit_code = exit_code;
        transaction_ref.status = if exit_code.is_some() {
            String::from("completed")
        } else {
            String::from("timeout")
        };
        transaction_ref.ended_seq = read_outcome
            .returned_chunks
            .last()
            .map(|chunk| chunk.seq)
            .or(Some(read_outcome.snapshot.last_seq));
        (
            read_outcome.snapshot,
            stripped_chunks,
            read_outcome.has_more,
            response_wait_started_at.elapsed().as_millis() as u64,
            None,
            None,
        )
    } else if interrupt_requested {
        let drain_result = drain_terminal_interrupt_output(
            &command_session_runtime,
            owner_session_id,
            terminal_session_id,
            before_snapshot.last_seq,
            parsed_args.max_chunks,
        )
        .await?;
        interrupt_drain_elapsed_ms = drain_result.elapsed_ms;
        interrupt_drain_discarded_chunks = drain_result.discarded_chunk_count;
        let cleanup_result = cleanup_terminal_interrupt_input_line(
            &command_session_runtime,
            owner_session_id,
            terminal_session_id,
            drain_result.snapshot.clone(),
            interactive_newline,
            parsed_args.max_chunks,
        )
        .await;
        interrupt_cleanup_elapsed_ms = cleanup_result.elapsed_ms;
        interrupt_cleanup_discarded_chunks = cleanup_result.discarded_chunk_count;
        interrupt_cleanup_failure = cleanup_result.failure_reason.clone();
        (
            cleanup_result.snapshot,
            drain_result.returned_chunks,
            drain_result.has_more || cleanup_result.has_more,
            interrupt_drain_elapsed_ms.saturating_add(interrupt_cleanup_elapsed_ms),
            None,
            None,
        )
    } else {
        let wait_result = read_terminal_session_output_with_wait(
            &command_session_runtime,
            owner_session_id,
            terminal_session_id,
            Some(before_snapshot.last_seq),
            false,
            parsed_args.wait_policy.as_ref(),
            parsed_args.max_chunks,
        )
        .await?;
        (
            wait_result.snapshot,
            wait_result.returned_chunks,
            wait_result.has_more,
            wait_result.elapsed_ms,
            wait_result.pattern_matched,
            wait_result.quiet_observed,
        )
    };
    let returned_chunks = coalesce_terminal_output_chunks(returned_chunks);
    let (output_mode, confidence, reason) = detect_terminal_output_mode(&returned_chunks);
    let spill_root = resolve_shell_command_spill_root(
        request.spill_root.as_deref(),
        request.workspace_root.as_deref(),
        request
            .workspace_root
            .as_deref()
            .unwrap_or_else(|| Path::new(".")),
    );
    let input_labels = inputs
        .iter()
        .map(terminal_input_label)
        .collect::<Vec<_>>()
        .join(", ");
    let render_input = build_terminal_session_render_input(
        TerminalSessionToolAction::Send,
        &read_snapshot,
        &session_name,
        &returned_chunks,
        has_more,
        output_mode,
        transaction.as_ref(),
        vec![
            format!("Inputs: {input_labels}"),
            format!("Key encoding: {key_encoding_label}"),
            format!("Bytes written: {}", write_outcome.bytes_written),
            format!("Wait mode: {}", wait_mode.as_str()),
            format!("Wait elapsed: {wait_elapsed_ms} ms"),
            format!(
                "Pattern matched: {}",
                pattern_matched
                    .map(|matched| matched.to_string())
                    .unwrap_or_else(|| String::from("n/a"))
            ),
            format!(
                "Quiet observed: {}",
                quiet_observed
                    .map(|observed| observed.to_string())
                    .unwrap_or_else(|| String::from("n/a"))
            ),
            format!(
                "Interrupt drain: {}",
                if interrupt_requested { "yes" } else { "no" }
            ),
            format!("Interrupt drain elapsed ms: {interrupt_drain_elapsed_ms}"),
            format!("Interrupt drain discarded chunks: {interrupt_drain_discarded_chunks}"),
            format!("Interrupt cleanup elapsed ms: {interrupt_cleanup_elapsed_ms}"),
            format!("Interrupt cleanup discarded chunks: {interrupt_cleanup_discarded_chunks}"),
            format!(
                "Interrupt cleanup failure: {}",
                interrupt_cleanup_failure.as_deref().unwrap_or("none")
            ),
        ],
    );
    let budget = default_shell_command_budget_snapshot();
    let host_payload = build_terminal_session_host_payload(
        TerminalSessionToolAction::Send,
        &read_snapshot,
        &session_name,
        &returned_chunks,
        shell_command_output_exceeds_inline_budget(&render_input, &budget),
        output_mode,
        confidence,
        reason,
        transaction.as_ref(),
        json!({
            "input_labels": input_labels,
            "key_encoding": key_encoding_label,
            "bytes_written": write_outcome.bytes_written,
            "wait_mode": wait_mode.as_str(),
            "wait_elapsed_ms": wait_elapsed_ms,
            "pattern_matched": pattern_matched,
            "quiet_observed": quiet_observed,
            "interrupt_drain_ms": interrupt_drain_elapsed_ms,
            "interrupt_drain_discarded_chunks": interrupt_drain_discarded_chunks,
            "interrupt_cleanup_ms": interrupt_cleanup_elapsed_ms,
            "interrupt_cleanup_discarded_chunks": interrupt_cleanup_discarded_chunks,
            "interrupt_cleanup_failure": interrupt_cleanup_failure,
            "returned_chunk_count": returned_chunks.len(),
            "has_more": has_more,
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

/// Executes one terminal-session stop action.
/// 执行一次终端会话 stop 动作。
async fn execute_terminal_session_stop_action(
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
async fn execute_terminal_session_tool(
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

/// Executes one shell-session start action and returns the unified tool invocation result.
/// 执行一次 shell 会话 start 动作并返回统一工具调用结果。
#[cfg(test)]
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
#[cfg(test)]
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
#[cfg(test)]
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
#[cfg(test)]
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
        system_skill_root: Some(
            vulcan_runtime_assets::resolve_system_prompt_skills_resource_directory(),
        ),
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

/// Executes the host-owned `vmm_memory_delete` tool.
/// 执行宿主自有 `vmm_memory_delete` 工具。
async fn execute_vmm_memory_delete_tool(
    request: NativeToolExecutionRequest,
    snapshot: Arc<NativeToolRuntimeSnapshot>,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let parsed_args = parse_vmm_memory_delete_arguments(request.arguments.clone())?;
    let config = effective_vmm_config_for_native_request(&request, &snapshot)?;
    let client = connect_vmm_native_client(&config).await?;
    let response = client
        .delete_memories(DeleteMemoriesRequest {
            user_id: config.user_id.unwrap_or_default(),
            project_id: config.project_id.unwrap_or_default(),
            memory_ids: parsed_args.memory_ids,
            reason: parsed_args.reason,
        })
        .await
        .map_err(vmm_memory_delete_native_error)?;

    Ok(VulcanToolInvocationResult::from_model_output(json!({
        "trace_id": response.trace_id,
        "deleted_count": response.deleted_memory_ids.len(),
        "not_found_count": response.not_found_memory_ids.len(),
        "deleted_vector_rows": response.deleted_vector_rows,
        "deleted_memory_ids": response.deleted_memory_ids,
        "not_found_memory_ids": response.not_found_memory_ids,
    })))
}

/// Converts VMM memory-delete errors into model-facing native tool errors.
/// 将 VMM 记忆删除错误转换为面向模型的原生工具错误。
fn vmm_memory_delete_native_error(error: vulcan_vmm::VmmClientError) -> NativeToolError {
    if error.grpc_code == Some(12)
        || error.kind.eq_ignore_ascii_case("unimplemented")
        || error.message.to_ascii_lowercase().contains("unimplemented")
    {
        NativeToolError::new(
            "The connected VMM service does not support DeleteMemories. Update VulcanMemoryMesh before using vmm_memory_delete.",
        )
    } else {
        NativeToolError::new(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CommandSessionBackend, CommandSessionInputEvent, CommandSessionOutputChunk,
        CommandSessionSnapshot, NativeToolExecutionRequest, SUBMIT_PLAN_MARKER,
        TerminalSessionInput, TerminalSessionKeyEncoding, ThreadGoal, ThreadGoalStatus,
        build_agent_list_tool, build_mcp_server_list_tool, build_shell_command_tool,
        build_shell_session_tool, build_skill_list_tool, build_submit_plan_tool,
        build_terminal_session_tool, build_todolist_tool, build_workspace_describe_tool,
        coalesce_terminal_output_chunks, display_submit_plan_path, effective_start_append_newline,
        encode_terminal_inputs, goal_response_value, normalize_terminal_session_name,
        resolve_session_shell_behavior, resolve_shell_command_launcher,
        resolve_shell_command_working_directory, shell_command_args_schema, shell_specific_newline,
        strip_simple_ansi_sequences, strip_terminal_transaction_sentinel,
        terminal_inputs_request_interrupt, terminal_interrupt_drain_chunk_is_visible,
        terminal_output_chunks_contain_pattern, terminal_screen_text_from_chunks,
        terminal_session_args_schema, terminal_transaction_command_text,
        validate_shell_session_start_stdin_policy, validate_terminal_session_start_shell_policy,
        validate_terminal_transaction_start, windows_console_input_events_from_inputs,
        wrap_terminal_transaction_command,
    };
    use crate::agent_runtime::config::{
        AgentRuntimeConfig, RuntimeAgentDefinition, RuntimeAgentKind, RuntimeAgentRoute,
        RuntimeSkillDefinition, RuntimeSkillKind,
    };
    use crate::tooling::CommandSessionRuntime;
    use crate::tooling::command_session_runtime::{
        CommandSessionOutputStream, CommandSessionStatus,
    };
    use crate::tooling::shell_host::{
        DetectedHostCommandShell, HostCommandShell, HostCommandShellSupport,
    };
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use tokio::fs as tokio_fs;
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

    /// Builds one native-tool execution request with only arguments varied.
    /// 构建一条仅变更参数的原生工具执行请求。
    fn test_native_request(arguments: serde_json::Value) -> NativeToolExecutionRequest {
        NativeToolExecutionRequest {
            arguments,
            session_id: None,
            workspace_root: None,
            controller_runtime: None,
            spill_root: None,
            enabled_prompt_skill_names: Vec::new(),
            prompt_skill_user_root: None,
            project_vmm: VmmConfigLayer::default(),
            session_vmm: VmmConfigLayer::default(),
        }
    }

    /// Builds one native-tool execution request with a workspace root.
    /// 构建一条带工作区根目录的原生工具执行请求。
    fn test_native_request_with_workspace(
        arguments: serde_json::Value,
        workspace_root: PathBuf,
    ) -> NativeToolExecutionRequest {
        NativeToolExecutionRequest {
            workspace_root: Some(workspace_root),
            ..test_native_request(arguments)
        }
    }

    /// Verifies todolist returns the Codex-compatible model text and a host todolist payload.
    /// 验证 todolist 返回与 Codex 兼容的模型文本以及宿主 todolist 载荷。
    #[tokio::test]
    async fn todolist_tool_returns_todo_host_payload() {
        let tool = build_todolist_tool();
        let result = tool
            .invoke(test_native_request(json!({
                "explanation": "next pass",
                "items": [
                    { "step": "Read source", "status": "completed" },
                    { "step": "Wire UI", "status": "in_progress" },
                    { "step": "Verify", "status": "pending" }
                ]
            })))
            .await
            .expect("todolist should execute");
        let host_result = result.host_result.expect("host result");

        assert_eq!(result.model_output, json!("Todolist updated"));
        assert_eq!(host_result.kind, "todolist");
        assert_eq!(host_result.payload["explanation"], "next pass");
        assert_eq!(host_result.payload["items"][1]["status"], "in_progress");
    }

    /// Verifies todolist rejects multiple in-progress items.
    /// 验证 todolist 会拒绝多个进行中条目。
    #[tokio::test]
    async fn todolist_tool_rejects_multiple_in_progress_items() {
        let tool = build_todolist_tool();
        let error = tool
            .invoke(test_native_request(json!({
                "items": [
                    { "step": "A", "status": "in_progress" },
                    { "step": "B", "status": "in_progress" }
                ]
            })))
            .await
            .expect_err("todolist should reject duplicate in-progress items");

        assert!(
            error
                .message
                .contains("At most one todolist item can be in_progress")
        );
    }

    /// Verifies todolist stays AI-visible but is not allowed to execute in Plan mode.
    /// 验证 todolist 保持 AI 可见，但不允许在计划模式中执行。
    #[test]
    fn todolist_tool_is_visible_but_blocked_in_plan_mode() {
        let tool = build_todolist_tool();
        assert!(tool.descriptor.ai_visible);
        assert!(!tool.descriptor.available_in_plan);
    }

    /// Verifies todolist is described to models as a todolist/checklist progress tool.
    /// 验证 todolist 面向模型时会被描述为 todolist/检查清单进程工具。
    #[test]
    fn todolist_tool_description_names_todo_semantics() {
        let tool = build_todolist_tool();
        assert!(tool.descriptor.description.contains("todolist/checklist"));
        assert!(tool.descriptor.description.contains("not Plan Mode"));
        assert!(tool.args_schema.to_string().contains("todolist/checklist"));
    }

    /// Verifies todolist rejects blank step text before emitting host projection data.
    /// 验证 todolist 会在发出宿主投影数据前拒绝空白步骤文本。
    #[tokio::test]
    async fn todolist_tool_rejects_blank_step_text() {
        let tool = build_todolist_tool();
        let error = tool
            .invoke(test_native_request(json!({
                "items": [
                    { "step": "   ", "status": "pending" }
                ]
            })))
            .await
            .expect_err("todolist should reject blank step text");

        assert!(error.message.contains("Todolist item step cannot be empty"));
    }

    /// Verifies submit_plan writes a marked Markdown file and returns a host artifact payload.
    /// 验证 submit_plan 会写入带标记的 Markdown 文件并返回宿主产物载荷。
    #[tokio::test]
    async fn submit_plan_tool_writes_marked_plan_artifact() {
        let workspace_root =
            std::env::temp_dir().join(format!("vulcan-submit-plan-{}", Uuid::new_v4()));
        tokio_fs::create_dir_all(&workspace_root)
            .await
            .expect("workspace directory");
        let tool = build_submit_plan_tool();
        let result = tool
            .invoke(test_native_request_with_workspace(
                json!({
                    "name": "Plan Mode Test",
                    "content": "# Plan\n\n- Step one"
                }),
                workspace_root.clone(),
            ))
            .await
            .expect("submit_plan should execute");
        let host_result = result.host_result.expect("host result");
        let path = host_result.payload["path"].as_str().expect("path");
        let content = tokio_fs::read_to_string(path).await.expect("plan content");

        assert_eq!(host_result.kind, "plan_artifact");
        assert_eq!(
            host_result.payload["content"].as_str(),
            Some("# Plan\n\n- Step one")
        );
        assert!(content.contains(SUBMIT_PLAN_MARKER));
        assert!(content.contains("# Plan"));

        let _ = tokio_fs::remove_dir_all(workspace_root).await;
    }

    /// Verifies submit_plan refuses to replace files that it did not create.
    /// 验证 submit_plan 会拒绝替换非工具创建的文件。
    #[tokio::test]
    async fn submit_plan_tool_rejects_unmarked_replace_target() {
        let workspace_root =
            std::env::temp_dir().join(format!("vulcan-submit-plan-{}", Uuid::new_v4()));
        tokio_fs::create_dir_all(&workspace_root)
            .await
            .expect("workspace directory");
        let target = workspace_root.join("manual.md");
        tokio_fs::write(&target, "# Manual\n")
            .await
            .expect("manual plan");
        let tool = build_submit_plan_tool();
        let error = tool
            .invoke(test_native_request_with_workspace(
                json!({
                    "name": "Manual",
                    "content": "# Replacement",
                    "replace": true,
                    "path": target.display().to_string()
                }),
                workspace_root.clone(),
            ))
            .await
            .expect_err("submit_plan should reject unmarked replacement");

        assert!(error.message.contains("not created by submit_plan"));

        let _ = tokio_fs::remove_dir_all(workspace_root).await;
    }

    /// Verifies submit_plan replacement accepts marked artifacts without an in-memory session record.
    /// 验证 submit_plan 替换允许没有内存会话记录但带工具标记的产物。
    #[tokio::test]
    async fn submit_plan_tool_replaces_marked_target_without_session_record() {
        let workspace_root =
            std::env::temp_dir().join(format!("vulcan-submit-plan-{}", Uuid::new_v4()));
        tokio_fs::create_dir_all(&workspace_root)
            .await
            .expect("workspace directory");
        let target = workspace_root.join("generated.md");
        tokio_fs::write(&target, format!("{SUBMIT_PLAN_MARKER}\n\n# Plan\n"))
            .await
            .expect("tool plan");
        let tool = build_submit_plan_tool();
        let result = tool
            .invoke(test_native_request_with_workspace(
                json!({
                    "name": "Generated",
                    "content": "# Replacement",
                    "replace": true,
                    "path": target.display().to_string()
                }),
                workspace_root.clone(),
            ))
            .await
            .expect("submit_plan should replace a marked target");
        let host_result = result.host_result.expect("host result");
        let replaced = tokio_fs::read_to_string(&target)
            .await
            .expect("replacement content");
        let expected_path = target.display().to_string();

        assert_eq!(host_result.kind, "plan_artifact");
        assert_eq!(
            host_result.payload["path"].as_str(),
            Some(expected_path.as_str())
        );
        assert!(replaced.contains(SUBMIT_PLAN_MARKER));
        assert!(replaced.contains("# Replacement"));

        let _ = tokio_fs::remove_dir_all(workspace_root).await;
    }

    /// Verifies submit_plan feedback paths hide Windows extended-path prefixes.
    /// 验证 submit_plan 反馈路径会隐藏 Windows 扩展路径前缀。
    #[test]
    fn submit_plan_feedback_path_hides_windows_extended_prefix() {
        assert_eq!(
            display_submit_plan_path(Path::new(
                "\\\\?\\D:\\openvulcan\\test-project\\docs\\plan\\mock.md"
            )),
            "D:\\openvulcan\\test-project\\docs\\plan\\mock.md"
        );
        assert_eq!(
            display_submit_plan_path(Path::new("\\\\?\\UNC\\server\\share\\mock.md")),
            "\\\\server\\share\\mock.md"
        );
    }

    /// Verifies completed goals return the Codex-compatible structured usage-report instruction.
    /// 验证已完成目标会返回与 Codex 兼容的结构化用量汇报指令。
    #[test]
    fn completed_goal_response_reports_structured_usage_instruction() {
        let goal = ThreadGoal {
            thread_id: Uuid::new_v4(),
            goal_id: String::from("goal-1"),
            objective: String::from("完成目标模式"),
            status: ThreadGoalStatus::Complete,
            token_budget: Some(10_000),
            tokens_used: 1_234,
            time_used_seconds: 56,
            created_at_ms: 1_000,
            updated_at_ms: 2_000,
        };

        let response = goal_response_value(Some(&goal), true);

        assert_eq!(response["remainingTokens"], json!(8_766));
        assert!(
            response["completionBudgetReport"]
                .as_str()
                .is_some_and(|value| value.contains("Report final usage"))
        );
        assert_eq!(response["goal"]["tokensUsed"], json!(1_234));
        assert_eq!(response["goal"]["tokenBudget"], json!(10_000));
        assert_eq!(response["goal"]["timeUsedSeconds"], json!(56));
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
                controller_runtime: None,
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
                controller_runtime: None,
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
        assert!(error.message.to_ascii_lowercase().contains("support"));
    }

    /// The dynamic shell schemas should expose `auto` together with only the shells detected on the current host snapshot.
    /// 动态 shell schema 应暴露 `auto` 以及当前宿主快照检测到的 shell。
    #[test]
    fn shell_tool_schemas_expose_dynamic_detected_shells() {
        let shell_support = test_shell_support();
        let command_schema = shell_command_args_schema(&shell_support);
        let session_schema = terminal_session_args_schema(&shell_support);

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
        let session_tool = build_terminal_session_tool(
            Arc::new(CommandSessionRuntime::new()),
            shell_support.clone(),
        );
        let command_schema = shell_command_args_schema(&shell_support);
        let session_schema = terminal_session_args_schema(&shell_support);

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
                .contains("call skill(\"terminal-session\")")
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
                .contains("until_exit_code")
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
            session_schema["properties"]["action"]["enum"]
                .as_array()
                .unwrap_or(&Vec::new())
                .contains(&json!("send"))
        );
        assert!(
            session_schema["properties"]["max_chunks"]["description"]
                .as_str()
                .unwrap_or_default()
                .contains("Default: 128")
        );
        assert!(
            session_schema["properties"]["inputs"]["description"]
                .as_str()
                .unwrap_or_default()
                .contains("Do not hand-write ANSI")
        );
        assert!(
            session_schema["properties"]["wait_policy"]["description"]
                .as_str()
                .unwrap_or_default()
                .contains("until_exit_code")
        );
        assert!(
            session_schema["properties"]["cwd"]["description"]
                .as_str()
                .unwrap_or_default()
                .contains("workspace root")
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
            let error = validate_terminal_session_start_shell_policy(Some("auto"), &bash_behavior)
                .expect_err("windows auto nested bash should be rejected");
            assert!(error.message.contains("requires an explicit `shell`"));
        } else {
            validate_terminal_session_start_shell_policy(Some("auto"), &bash_behavior)
                .expect("non-windows hosts should keep current auto behavior");
        }
        validate_terminal_session_start_shell_policy(Some("bash"), &bash_behavior)
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

    /// The terminal-session tool should stay AI-visible in Plan mode so interactive command workflows are not silently hidden from the model.
    /// terminal-session 工具应在 Plan 模式下继续对 AI 可见，避免交互式命令工作流被模型静默隐藏。
    #[test]
    fn terminal_session_tool_stays_visible_in_plan_mode() {
        let tool = build_terminal_session_tool(
            Arc::new(CommandSessionRuntime::new()),
            test_shell_support(),
        );
        assert!(tool.descriptor.ai_visible);
        assert!(tool.descriptor.available_in_plan);
        assert!(
            tool.descriptor
                .description
                .contains("skill(\"terminal-session\")")
        );
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

    /// Terminal key encoding should keep ANSI VT as the default while allowing explicit Windows getch bytes.
    /// 终端按键编码应保持 ANSI VT 为默认，同时允许显式 Windows getch 字节。
    #[test]
    fn terminal_key_encoding_supports_ansi_and_windows_getch_extended_keys() {
        let ansi = encode_terminal_inputs(
            &[
                TerminalSessionInput::Key {
                    key: String::from("ArrowDown"),
                },
                TerminalSessionInput::Key {
                    key: String::from("Delete"),
                },
                TerminalSessionInput::Key {
                    key: String::from("F12"),
                },
            ],
            "\r\n",
            TerminalSessionKeyEncoding::AnsiVt,
        )
        .expect("encode ansi keys");
        assert_eq!(ansi, b"\x1b[B\x1b[3~\x1b[24~");

        let windows_getch_single_byte = encode_terminal_inputs(
            &[TerminalSessionInput::Key {
                key: String::from("Esc"),
            }],
            "\r\n",
            TerminalSessionKeyEncoding::WindowsConsoleGetch,
        )
        .expect("encode windows getch single-byte key");
        assert_eq!(windows_getch_single_byte, vec![0x1b]);

        let windows_getch_arrow = encode_terminal_inputs(
            &[TerminalSessionInput::Key {
                key: String::from("ArrowDown"),
            }],
            "\r\n",
            TerminalSessionKeyEncoding::WindowsConsoleGetch,
        );
        if cfg!(windows) {
            assert!(
                windows_getch_arrow
                    .expect_err("byte-oriented backend should reject getch extended keys")
                    .message
                    .contains("cannot synthesize console extended key")
            );
        } else {
            assert_eq!(windows_getch_arrow.expect("encode arrow"), vec![0xe0, b'P']);
        }
    }

    /// Explicit ANSI VT encoding on WindowsConsole should convert keys to text escape sequences instead of native key events.
    /// WindowsConsole 上显式 ANSI VT 编码应把按键转换为文本转义序列，而不是原生按键事件。
    #[test]
    fn windows_console_explicit_ansi_vt_uses_text_escape_sequence() {
        let events = windows_console_input_events_from_inputs(
            &[TerminalSessionInput::Key {
                key: String::from("ArrowDown"),
            }],
            "\r\n",
            Some(TerminalSessionKeyEncoding::AnsiVt),
        )
        .expect("convert ansi vt input");
        assert_eq!(events.len(), 1);
        match &events[0] {
            CommandSessionInputEvent::Text { text } => assert_eq!(text, "\x1b[B"),
            other => panic!("expected text event, got {other:?}"),
        }

        let native_events = windows_console_input_events_from_inputs(
            &[TerminalSessionInput::Key {
                key: String::from("ArrowDown"),
            }],
            "\r\n",
            None,
        )
        .expect("convert native input");
        assert!(matches!(
            &native_events[0],
            CommandSessionInputEvent::Key { key } if key == "ArrowDown"
        ));
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

    /// Windows console screen frames should parse the transaction marker even when the visible frame loses the leading host prefix.
    /// Windows 控制台屏幕帧即使丢失宿主前缀的可见开头，也应解析事务标记。
    #[test]
    fn terminal_transaction_sentinel_accepts_windows_screen_visible_suffix() {
        let chunks = vec![CommandSessionOutputChunk {
            seq: 3,
            stream: CommandSessionOutputStream::Stdout,
            text: String::from(
                "[screen #3]\n\x1b[37;40m_TERMINAL_TX_EXIT__:tx-screen:0\x1b[0m\nPS D:\\repo>",
            ),
        }];

        let (stripped_chunks, exit_code) =
            strip_terminal_transaction_sentinel(&chunks, "tx-screen");

        assert_eq!(exit_code, Some(0));
        assert!(stripped_chunks.iter().all(|chunk| {
            !strip_simple_ansi_sequences(&chunk.text).contains("_TERMINAL_TX_EXIT__:tx-screen")
        }));
    }

    /// Later echoed transaction wrapper text should not erase a previously parsed transaction exit code.
    /// 后续回显的事务包装文本不应清空先前已解析到的事务退出码。
    #[test]
    fn terminal_transaction_sentinel_keeps_parsed_code_when_echo_later_matches() {
        let chunks = vec![CommandSessionOutputChunk {
            seq: 4,
            stream: CommandSessionOutputStream::Stdout,
            text: String::from(
                "__VULCAN_TERMINAL_TX_EXIT__:tx-keep:0\nWrite-Output \"__VULCAN_TERMINAL_TX_EXIT__:tx-keep:$vulcanCode\"\n",
            ),
        }];

        let (_stripped_chunks, exit_code) = strip_terminal_transaction_sentinel(&chunks, "tx-keep");

        assert_eq!(exit_code, Some(0));
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

    /// Terminal command transactions must reject paste input because wrapping pasted payloads can corrupt interactive shell state.
    /// 终端命令事务必须拒绝粘贴输入，因为包装粘贴载荷可能破坏交互式 shell 状态。
    #[test]
    fn terminal_transaction_rejects_paste_input() {
        let error = terminal_transaction_command_text(&[TerminalSessionInput::Paste {
            text: String::from("echo pasted\n"),
        }])
        .expect_err("paste input should not be accepted for exit-code transactions");

        assert!(error.message.contains("cannot be used with paste"));
    }

    /// Terminal command transactions should reject dirty interactive state before wrapper injection.
    /// 终端命令事务应在注入包装命令前拒绝脏交互状态。
    #[test]
    fn terminal_transaction_rejects_pending_input_line() {
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
            pending_stdin_line: true,
            screen_frame_seq: None,
            screen_rows: None,
            screen_cols: None,
            screen_text: None,
        };

        let error = validate_terminal_transaction_start(&snapshot)
            .expect_err("pending input line should reject command transactions");
        assert!(error.message.contains("pending input line"));
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

    /// PowerShell terminal transactions should not leave the interactive shell in continuation mode.
    /// PowerShell 终端事务不应让交互式 shell 停留在续行模式。
    #[cfg(windows)]
    #[tokio::test]
    async fn terminal_powershell_transaction_leaves_shell_usable() {
        let runtime = Arc::new(CommandSessionRuntime::new());
        let tool = build_terminal_session_tool(Arc::clone(&runtime), test_shell_support());
        let owner_session_id = Uuid::from_u128(0x7001);
        let start_result = tool
            .invoke(NativeToolExecutionRequest {
                arguments: json!({
                    "action": "start",
                    "name": "transaction regression",
                    "command": "pwsh -NoLogo -NoProfile",
                    "shell": "pwsh"
                }),
                session_id: Some(owner_session_id),
                workspace_root: None,
                controller_runtime: None,
                spill_root: None,
                enabled_prompt_skill_names: Vec::new(),
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

        let transaction_result = tool
            .invoke(NativeToolExecutionRequest {
                arguments: json!({
                    "action": "send",
                    "terminal_session_id": terminal_session_id,
                    "inputs": [{ "kind": "text", "text": "Write-Output 'txn_test'\n" }],
                    "wait_policy": { "mode": "until_exit_code", "max_wait_ms": 10000 }
                }),
                session_id: Some(owner_session_id),
                workspace_root: None,
                controller_runtime: None,
                spill_root: None,
                enabled_prompt_skill_names: Vec::new(),
                prompt_skill_user_root: None,
                project_vmm: VmmConfigLayer::default(),
                session_vmm: VmmConfigLayer::default(),
            })
            .await
            .expect("transaction send should execute");
        let transaction_output = transaction_result
            .model_output
            .as_str()
            .expect("transaction output should render");
        let transaction_payload = &transaction_result
            .host_result
            .as_ref()
            .expect("host result")
            .payload;
        assert_eq!(
            transaction_payload["transaction"]["exit_code"],
            json!(0),
            "transaction output was: {transaction_output}\npayload was: {transaction_payload}"
        );

        let followup_result = tool
            .invoke(NativeToolExecutionRequest {
                arguments: json!({
                    "action": "send",
                    "terminal_session_id": terminal_session_id,
                    "inputs": [{ "kind": "text", "text": "Write-Output 'after_tx'\n" }],
                    "wait_policy": {
                        "mode": "until_pattern",
                        "pattern": "after_tx",
                        "max_wait_ms": 10000
                    }
                }),
                session_id: Some(owner_session_id),
                workspace_root: None,
                controller_runtime: None,
                spill_root: None,
                enabled_prompt_skill_names: Vec::new(),
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
            followup_output.contains("after_tx"),
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
                arguments: json!({
                    "action": "stop",
                    "terminal_session_id": terminal_session_id
                }),
                session_id: Some(owner_session_id),
                workspace_root: None,
                controller_runtime: None,
                spill_root: None,
                enabled_prompt_skill_names: Vec::new(),
                prompt_skill_user_root: None,
                project_vmm: VmmConfigLayer::default(),
                session_vmm: VmmConfigLayer::default(),
            })
            .await;
    }

    /// PowerShell transaction wrapping should report non-terminating cmdlet errors as a failed transaction.
    /// PowerShell 事务包装应将 cmdlet 非终止错误报告为失败事务。
    #[test]
    fn terminal_transaction_powershell_wrapper_tracks_non_terminating_errors() {
        let wrapped =
            wrap_terminal_transaction_command("Get-Item missing", "pwsh", "tx-id", "\r\n")
                .expect("pwsh wrapper should be supported");

        assert!(wrapped.contains("$vulcanErrorCount = $Error.Count"));
        assert!(wrapped.contains("$vulcanNewErrors = $Error.Count - $vulcanErrorCount"));
        assert!(wrapped.contains("$vulcanSuccess -and $vulcanNewErrors -le 0"));
        assert!(wrapped.contains("__VULCAN_TERMINAL_TX_EXIT__:tx-id:"));
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
                controller_runtime: None,
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
                controller_runtime: None,
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
                controller_runtime: None,
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
                controller_runtime: None,
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
                controller_runtime: None,
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
                controller_runtime: None,
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
                controller_runtime: None,
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
                controller_runtime: None,
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
                controller_runtime: None,
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
