//! Built-in native tools exposed to the host runtime foundation.
//! 暴露给宿主运行时基础架构的内建原生工具。

use crate::agent_runtime::ExecutionKnowledgeAccessContext;
use crate::agent_runtime::config::{AgentRuntimeConfig, RuntimeAgentKind, RuntimeSkillKind};
use crate::model::SessionId;
use crate::project_temp_maintenance;
use crate::tool_result_format::{
    HostRenderOptions, HostTextRenderInput, HostToolOverflowMode, ToolResultBudgetSnapshot,
    render_tool_result_text,
};
use crate::tooling::command_session_runtime::{
    CommandSessionBackend, CommandSessionInputEvent, CommandSessionOutputChunk,
    CommandSessionReadOutcome, CommandSessionRuntime, CommandSessionSnapshot,
    CommandSessionStartRequest, DEFAULT_COMMAND_SESSION_READ_MAX_CHUNKS,
    DEFAULT_COMMAND_SESSION_STOP_WAIT_MS, MAX_COMMAND_SESSION_READ_CHUNKS,
    MAX_COMMAND_SESSION_READ_WAIT_MS,
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
use vulcan_prompt_skills::{PromptSkill, render_skill_tool_output};
use vulcan_tools_manager::{VulcanToolHostResult, VulcanToolInvocationResult};
use vulcan_vmm::{
    DeleteMemoriesRequest, GetTurnDetailsRequest, SearchMemoryEventsRequest, TonicVmmClient,
    VmmBackendMode, VmmClient, VmmConfigDiagnostic, VmmConfigLayer, VmmEffectiveConfig,
    VmmTransportConfig, WriteMemoriesRequest, WriteMemoryItem, process_native_vmm_client,
};
use vulcan_workspace_store::data_plane::HostRuntimeHandle;

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

/// Stable identifier for the tool that explains the next tool call to the permission reviewer.
/// 向权限复核器说明下一次工具调用的工具的稳定标识。
pub const DECLARE_INTENT_TOOL_ID: &str = "declare_intent";

/// Longest reason the permission reviewer reads, whether a command `purpose` or a declared
/// intent, in Unicode scalar values.
/// 权限复核器读取的最长理由（命令 `purpose` 或已声明意图），按 Unicode 标量值计。
pub const MAX_PERMISSION_REASON_CHARS: usize = 2_000;

/// Model-facing description of the optional `purpose` argument of the command tools.
/// 命令工具可选 `purpose` 参数面向模型的说明。
const PURPOSE_ARGUMENT_DESCRIPTION: &str = "Optional; usually omit it. Why this call is needed, read only by the permission reviewer when the call needs approval, such as in an unattended session. Give it when the reviewer denied the same call before or the reason is not obvious from the call itself. It is never passed to the command.";

/// Stable lifecycle state exposed by the native Goal domain port.
/// 原生 Goal 领域端口暴露的稳定生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeGoalStatus {
    /// Goal is actively pursued by automatic continuation.
    /// Goal 正由自动续跑流程推进。
    Active,
    /// Goal is paused and does not auto-continue.
    /// Goal 已暂停且不会自动续跑。
    Paused,
    /// Goal is blocked and needs external progress.
    /// Goal 已受阻并需要外部进展。
    Blocked,
    /// Goal stopped because Provider or runtime usage is limited.
    /// Goal 因 Provider 或运行时用量受限而停止。
    UsageLimited,
    /// Goal exhausted its explicit token budget.
    /// Goal 已耗尽显式 token 预算。
    BudgetLimited,
    /// Goal completed its objective.
    /// Goal 已完成目标。
    Complete,
}

impl NativeGoalStatus {
    /// Returns the stable protocol key used in tool errors and responses.
    /// 返回工具错误与响应使用的稳定协议键。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Blocked => "blocked",
            Self::UsageLimited => "usage_limited",
            Self::BudgetLimited => "budget_limited",
            Self::Complete => "complete",
        }
    }
}

/// Terminal lifecycle transition accepted from the native update-goal tool.
/// 原生 update-goal 工具接受的终态生命周期转换。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeGoalTerminalStatus {
    /// Mark the active Goal as completed.
    /// 将活动 Goal 标记为已完成。
    Complete,
    /// Mark the active Goal as blocked.
    /// 将活动 Goal 标记为受阻。
    Blocked,
}

/// Complete V1 Goal snapshot returned through the native-tool domain port.
/// 通过原生工具领域端口返回的完整 V1 Goal 快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeGoalSnapshot {
    /// Whether the current Goal budget usage is known.
    /// 当前 Goal 预算用量是否已知。
    pub usage_known: bool,
    /// Session identifier that owns the Goal.
    /// 拥有该 Goal 的会话标识。
    pub session_id: SessionId,
    /// Stable Goal generation identifier.
    /// 稳定的 Goal 代际标识。
    pub goal_id: String,
    /// Human-authored Goal objective.
    /// 用户编写的 Goal 目标内容。
    pub objective: String,
    /// Current Goal lifecycle state.
    /// 当前 Goal 生命周期状态。
    pub status: NativeGoalStatus,
    /// Optional explicit token budget.
    /// 可选的显式 token 预算。
    pub token_budget: Option<i64>,
    /// Accounted uncached-input-plus-output token usage.
    /// 已记账的非缓存输入加输出 token 用量。
    pub tokens_used: i64,
    /// Cumulative uncached input tokens.
    /// 累计非缓存输入 token。
    pub uncached_input_tokens: i64,
    /// Cumulative cache-hit input tokens.
    /// 累计缓存命中输入 token。
    pub cached_input_tokens: i64,
    /// Cumulative output tokens.
    /// 累计输出 token。
    pub output_tokens: i64,
    /// Cumulative reasoning tokens.
    /// 累计思考 token。
    pub reasoning_tokens: i64,
    /// Cumulative Provider total tokens.
    /// 累计 Provider 总 token。
    pub total_tokens: i64,
    /// Accounted wall-clock usage in seconds.
    /// 已记账的墙钟用量秒数。
    pub time_used_seconds: i64,
    /// Goal generation creation time in Unix milliseconds.
    /// Goal 代际创建时间的 Unix 毫秒值。
    pub created_at_ms: i64,
    /// Goal generation last-update time in Unix milliseconds.
    /// Goal 代际最近更新时间的 Unix 毫秒值。
    pub updated_at_ms: i64,
}

/// Host-injected V1 Goal domain port used by native Goal tools.
/// 原生 Goal 工具使用的宿主注入 V1 Goal 领域端口。
#[async_trait::async_trait]
pub trait NativeGoalRuntime: fmt::Debug + Send + Sync {
    /// Reads the authoritative Goal snapshot for one exact session.
    /// 读取一个精确会话的权威 Goal 快照。
    async fn get_goal(&self, session_id: SessionId) -> Result<Option<NativeGoalSnapshot>, String>;

    /// Creates a Goal or replaces a completed generation through one atomic V1 transition.
    /// 通过单个原子 V1 转换创建 Goal 或替换已完成代际。
    async fn create_goal(
        &self,
        session_id: SessionId,
        objective: String,
        token_budget: Option<u64>,
    ) -> Result<NativeGoalSnapshot, String>;

    /// Applies one terminal Goal transition through the authoritative V1 document.
    /// 通过权威 V1 文档应用一个 Goal 终态转换。
    async fn update_goal_status(
        &self,
        session_id: SessionId,
        status: NativeGoalTerminalStatus,
    ) -> Result<NativeGoalSnapshot, String>;
}
/// Stable identifier for project knowledge search.
/// 项目资料检索的稳定标识。
pub const KNOWLEDGE_SEARCH_TOOL_ID: &str = "knowledge_search";
/// Stable identifier for project knowledge browsing.
/// 项目资料浏览的稳定标识。
pub const KNOWLEDGE_BROWSE_TOOL_ID: &str = "knowledge_browse";
/// Stable identifier for project knowledge reading.
/// 项目资料读取的稳定标识。
pub const KNOWLEDGE_READ_TOOL_ID: &str = "knowledge_read";
/// Stable identifier for read-only LuaSkills package configuration discovery.
/// 只读 LuaSkills 技能包配置发现的稳定标识。
pub const RUNTIME_CONFIG_TOOL_ID: &str = "runtime-config";

/// Quiet window used after Ctrl+C before the terminal is considered drained.
/// Ctrl+C 后认为终端排空所需的静默窗口。
const TERMINAL_INTERRUPT_DRAIN_QUIET_MS: u64 = 250;

/// Maximum total wait used to consume residual Ctrl+C terminal output.
/// 用于消费 Ctrl+C 残留终端输出的最大总等待时间。
const TERMINAL_INTERRUPT_DRAIN_MAX_MS: u64 = 1_500;

/// Soft byte limit for coalescing adjacent model-visible terminal chunks.
/// 合并相邻模型可见终端分片时使用的软字节限制。
const TERMINAL_OUTPUT_CHUNK_COALESCE_SOFT_LIMIT_BYTES: usize = 2_048;
mod apply_patch;
mod file;
mod file_edit;
mod file_read;
mod file_result;
mod file_write;
mod knowledge_base;
mod prompt_skill;
mod registry;
mod shell_command;
pub(crate) use apply_patch::*;
pub(crate) use file::*;
pub(crate) use file_edit::*;
pub(crate) use file_read::*;
pub(crate) use file_write::*;
pub(crate) use knowledge_base::*;
pub(crate) use prompt_skill::*;
pub(crate) use registry::*;
pub(crate) use shell_command::*;
mod browser;
mod declare_intent;
mod goal;
mod runtime_config;
mod submit_plan;
mod todolist;

pub use browser::*;
pub(crate) use declare_intent::*;
pub(crate) use goal::*;
pub(crate) use runtime_config::*;
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
    /// Optional host-injected Core browser runtime for direct browser tools.
    /// 供浏览器直接工具使用的可选宿主注入 Core 浏览器运行时。
    pub native_browser_runtime: Option<Arc<dyn NativeBrowserRuntime>>,
    /// Optional stable VulcanCode project identity that owns this tool invocation.
    /// 拥有当前工具调用的可选稳定 VulcanCode 项目身份。
    pub host_project_id: Option<String>,
    /// Optional workspace root resolved for the current execution turn.
    /// 为当前执行回合解析得到的可选工作区根目录。
    pub workspace_root: Option<PathBuf>,
    /// Optional controller runtime used by native tools that persist workspace state.
    /// 持久化工作区状态的原生工具使用的可选控制器运行时。
    pub controller_runtime: Option<HostRuntimeHandle>,
    /// Optional host-injected V1 Goal domain port bound to this execution.
    /// 绑定到本次执行的可选宿主注入 V1 Goal 领域端口。
    pub goal_runtime: Option<Arc<dyn NativeGoalRuntime>>,
    /// Optional project-authorized and session-enabled knowledge-library snapshot.
    /// 可选的项目授权且会话已启用知识库快照。
    pub knowledge_access: Option<ExecutionKnowledgeAccessContext>,
    /// Optional host-managed spill directory used for oversized output paging.
    /// 用于超长输出分页落盘的可选宿主管理超限目录。
    pub spill_root: Option<PathBuf>,
    /// Optional prompt-skill names enabled for the current execution turn.
    /// 当前执行回合启用的可选提示词技能名称集合。
    pub enabled_prompt_skill_names: Vec<String>,
    /// Parsed prompt skills captured for the current execution turn.
    /// 当前执行回合捕获的已解析提示词技能集合。
    pub prepared_prompt_skills: Vec<PromptSkill>,
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
    /// Optional execution-only parser and preview capability for streamed requests.
    /// 可选的流式请求仅执行期解析器与预览能力。
    pub request_streaming: Option<vulcan_tools_manager::ToolRequestStreamingCapability>,
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
    plugin_runtime: Option<vulcan_plugin_runtime::VulcanPluginRuntimeManager>,
    luaskills_runtime: Option<crate::luaskills_runtime::LuaSkillsRuntimeHandle>,
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
    let mut definitions = vec![
        build_workspace_describe_tool(snapshot.clone()),
        build_agent_list_tool(snapshot.clone(), config),
        build_skill_list_tool(snapshot.clone(), config),
        build_todolist_tool(),
        build_submit_plan_tool(),
        build_get_goal_tool(),
        build_create_goal_tool(),
        build_update_goal_tool(),
        build_declare_intent_tool(),
        build_knowledge_search_tool(),
        build_knowledge_browse_tool(),
        build_knowledge_read_tool(),
        build_prompt_skill_detail_tool(),
        build_shell_command_tool(shell_support.clone(), plugin_runtime),
        build_terminal_session_tool(command_session_runtime, shell_support),
        build_mcp_server_list_tool(snapshot.clone()),
        build_vmm_memory_search_tool(snapshot.clone()),
        build_vmm_turn_details_tool(snapshot.clone()),
        build_vmm_memory_write_tool(snapshot.clone()),
        build_vmm_memory_delete_tool(snapshot),
        build_file_tool(),
        build_file_read_tool(),
        build_file_write_tool(),
        build_file_edit_tool(),
        build_apply_patch_tool(),
        build_browser_list_tool(),
        build_browser_open_tool(),
        build_browser_select_tool(),
        build_browser_close_tool(),
    ];
    if let Some(runtime) = luaskills_runtime {
        definitions.push(build_runtime_config_tool(runtime));
    }
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
    build_native_tool_definition_with_request_streaming(descriptor, args_schema, None, handler)
}

/// Builds one native tool definition with an explicit host-owned request-streaming capability.
/// 使用显式宿主自有请求流能力构建单个原生工具定义。
///
/// # Parameters
/// 参数
/// `descriptor` is the stable tool identity and AI-visible description.
/// `descriptor` 是稳定工具身份与 AI 可见描述。
/// `args_schema` is the exact JSON Schema exposed to providers.
/// `args_schema` 是暴露给 Provider 的精确 JSON Schema。
/// `request_streaming` is the optional execution-only parser and preview capability.
/// `request_streaming` 是可选的仅执行期解析器与预览能力。
/// `handler` is the authoritative complete-request execution function.
/// `handler` 是权威完整请求执行函数。
///
/// # Returns
/// 返回值
/// A shared immutable native tool definition.
/// 返回共享不可变原生工具定义。
fn build_native_tool_definition_with_request_streaming(
    descriptor: NativeToolDescriptor,
    args_schema: Value,
    request_streaming: Option<vulcan_tools_manager::ToolRequestStreamingCapability>,
    handler: impl Fn(NativeToolExecutionRequest) -> NativeToolFuture + Send + Sync + 'static,
) -> Arc<NativeToolDefinition> {
    Arc::new(NativeToolDefinition {
        descriptor,
        args_schema,
        request_streaming,
        handler: Arc::new(handler),
    })
}

#[cfg(test)]
mod tests;
