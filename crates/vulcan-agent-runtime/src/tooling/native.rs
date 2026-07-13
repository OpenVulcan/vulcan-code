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
        GoalUpdate, MAX_THREAD_GOAL_OBJECTIVE_CHARS, ThreadGoal, ThreadGoalStatus, VulcanGoalStore,
        normalize_thread_goal_objective, validate_thread_goal_token_budget,
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
mod prompt_skill;
mod registry;
mod shell_command;
#[cfg(test)]
mod shell_session;
pub(crate) use prompt_skill::*;
pub(crate) use registry::*;
pub(crate) use shell_command::*;
#[cfg(test)]
pub(crate) use shell_session::*;
mod goal;
mod submit_plan;
mod todolist;

pub(crate) use goal::*;
pub(crate) use submit_plan::*;
pub(crate) use todolist::*;
mod terminal;
mod vmm;
pub(crate) use terminal::*;
pub(crate) use vmm::*;

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
    plugin_runtime: Option<vulcan_plugin_runtime::SystemPluginRuntimeManager>,
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
        build_shell_command_tool(shell_support.clone(), plugin_runtime),
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

#[cfg(test)]
mod tests;
