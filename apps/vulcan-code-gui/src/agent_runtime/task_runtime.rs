//! Resumable task-session runtime for host-managed task_id chaining and nested subtask execution.
//! 供宿主管理 task_id 续跑与嵌套子任务执行使用的可恢复任务会话运行时模块。

use crate::agent_runtime::events::{RuntimeEvent, RuntimeEventKind};
use crate::agent_runtime::provider_request_bridge_conversation::core_snapshot_to_session_conversation_state;
use crate::agent_runtime::provider_stream::ProviderStreamDelta;
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::provider_transport_stream_executor::StreamingRuntimeToolExecutor;
use crate::agent_runtime::runtime_stream_delta::{
    RuntimeStreamDeltaState, process_provider_stream_delta,
};
use crate::agent_runtime::tool_runtime_bridge::VulcanManagedToolRuntimeBridge;
use crate::conversation_protocol::SessionConversationState;
use crate::model::SessionId;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc::unbounded_channel;
use uuid::Uuid;
use vulcan_core::controller::VulcanCoreController;
use vulcan_core::tool::AllowAllToolPolicy;
use vulcan_core::types::{
    ControllerConfig as CoreControllerConfig, ConversationSnapshot as CoreConversationSnapshot,
};
use vulcan_tools_manager::{
    ResolvedToolDefinition, VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor,
    VulcanToolHostResult, VulcanToolInvocationResult, VulcanToolSourceKind, VulcanToolsManager,
};

/// Default nested-task depth limit used to prevent unbounded recursive task spawning.
/// 用于防止无限递归任务派生的默认嵌套任务深度上限。
const DEFAULT_TASK_NESTING_LIMIT: usize = 8;

/// Windows filesystem path guidance injected into child task prompts to stabilize tool calls.
/// 注入到子任务提示词中的 Windows 文件系统路径指引，用于稳定工具调用。
const WINDOWS_TASK_FILESYSTEM_PATH_GUIDANCE: &str = "[Filesystem Paths]\nWhen calling filesystem-oriented tools on Windows, prefer absolute paths written with forward slashes like D:/repo/path. Do not wrap tool arguments in Markdown code fences or extra quotes, and avoid escaped backslashes unless a tool explicitly requires them.\n在 Windows 上调用面向文件系统的工具时，优先使用类似 D:/repo/path 的正斜杠绝对路径。不要给工具参数额外包裹 Markdown 代码块或多余引号，也不要使用转义反斜杠，除非工具明确要求。";

/// Boxed asynchronous task execution future.
/// 装箱后的异步任务执行 Future。
pub type TaskExecutionFuture =
    Pin<Box<dyn Future<Output = Result<TaskExecutionOutcome, String>> + Send>>;

/// Shared asynchronous task execution handler.
/// 共享的异步任务执行处理器。
pub type TaskExecutionHandler =
    Arc<dyn Fn(TaskExecutionRequest) -> TaskExecutionFuture + Send + Sync>;

/// Structured task tool arguments accepted from the model-visible `task` tool surface.
/// 模型可见 `task` 工具表面接受的结构化任务参数。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
struct TaskToolInvocationArgs {
    /// Required prompt used to start or resume the task session.
    /// 用于启动或续跑任务会话的必填提示词。
    prompt: String,
    /// Optional human-readable task description.
    /// 可选的人类可读任务描述。
    description: Option<String>,
    /// Optional stable task identifier supplied by the caller.
    /// 调用方提供的可选稳定任务标识。
    task_id: Option<String>,
    /// Optional subagent label used for display and system-prompt augmentation.
    /// 用于展示和系统提示词增强的可选子代理标签。
    subagent: Option<String>,
}

/// Canonical task-session state stored by the host runtime between task invocations.
/// 宿主运行时在多次任务调用之间持久保存的规范任务会话状态。
#[derive(Debug, Clone)]
struct TaskSessionRecord {
    /// Stable task identifier that owns this record.
    /// 持有该记录的稳定任务标识。
    task_id: SessionId,
    /// The top-level GUI or task session that originated this task creation.
    /// 发起该任务创建的顶层 GUI 或任务会话标识。
    origin_session_id: SessionId,
    /// Optional parent task identifier for the nested task chain.
    /// 用于嵌套任务链的可选父任务标识。
    parent_task_id: Option<SessionId>,
    /// Stable nesting depth used for recursion guardrails.
    /// 用于递归防护的稳定嵌套深度。
    depth: usize,
    /// Optional human-readable task description.
    /// 可选的人类可读任务描述。
    description: Option<String>,
    /// Optional subagent label selected for the task.
    /// 任务选用的可选子代理标签。
    subagent: Option<String>,
    /// Fully resolved controller configuration reused when this task is resumed.
    /// 任务续跑时复用的完整控制器配置。
    controller_config: Option<CoreControllerConfig>,
    /// Canonical conversation snapshot retained for later resumption.
    /// 为后续续跑保留的规范会话快照。
    snapshot: CoreConversationSnapshot,
    /// Latest prompt text that was executed for this task.
    /// 该任务最近一次执行的提示词文本。
    last_prompt: String,
    /// Latest result text emitted by the task backend.
    /// 任务后端最近一次产出的结果文本。
    last_result_text: String,
    /// Unix-millis timestamp captured when the record was created.
    /// 记录创建时捕获的 Unix 毫秒时间戳文本。
    created_at: String,
    /// Unix-millis timestamp captured when the record was last updated.
    /// 记录最近一次更新时捕获的 Unix 毫秒时间戳文本。
    updated_at: String,
    /// Last observed execution status for the task.
    /// 任务最近一次观测到的执行状态。
    last_status: String,
}

/// Task execution request handed to the backend runner for one concrete task session.
/// 交给后端执行器的单条具体任务会话执行请求。
#[derive(Debug, Clone)]
pub struct TaskExecutionRequest {
    /// Invoking controller session identifier that owns the current tool call.
    /// 拥有当前工具调用的执行中控制器会话标识。
    pub current_session_id: SessionId,
    /// Stable task identifier for the child or resumed task session.
    /// 子任务或续跑任务会话的稳定任务标识。
    pub task_id: SessionId,
    /// User-visible prompt used to drive the task session.
    /// 用于驱动任务会话的用户可见提示词。
    pub prompt: String,
    /// Optional human-readable task description.
    /// 可选的人类可读任务描述。
    pub description: Option<String>,
    /// Optional subagent label used to annotate the task session.
    /// 用于标注任务会话的可选子代理标签。
    pub subagent: Option<String>,
    /// Optional parent task identifier for nested task chains.
    /// 嵌套任务链的可选父任务标识。
    pub parent_task_id: Option<SessionId>,
    /// Stable nesting depth used by the recursion guard.
    /// 递归防护使用的稳定嵌套深度。
    pub depth: usize,
    /// Canonical snapshot used when resuming an existing task session.
    /// 续跑既有任务会话时使用的规范快照。
    pub snapshot: CoreConversationSnapshot,
    /// Controller configuration used when resuming an existing task session.
    /// 续跑既有任务会话时使用的控制器配置。
    pub controller_config: Option<CoreControllerConfig>,
    /// Whether this request resumes a previously stored task session.
    /// 该请求是否在续跑一个已存储的任务会话。
    pub is_resumed: bool,
}

/// Task execution outcome returned by the backend after the child controller finishes.
/// 子控制器完成后由后端返回的任务执行结果。
#[derive(Debug, Clone)]
pub struct TaskExecutionOutcome {
    /// Stable task identifier associated with the outcome.
    /// 与该结果关联的稳定任务标识。
    pub task_id: SessionId,
    /// Final controller configuration used for this task session.
    /// 该任务会话实际使用的最终控制器配置。
    pub controller_config: CoreControllerConfig,
    /// Updated canonical snapshot captured after the task finished.
    /// 任务结束后捕获到的更新后规范快照。
    pub snapshot: CoreConversationSnapshot,
    /// Total provider turns executed by the child controller.
    /// 子控制器实际执行的 Provider 回合总数。
    pub provider_turns_executed: usize,
    /// Total tool calls processed by the child controller.
    /// 子控制器处理过的工具调用总数。
    pub tool_calls_processed: usize,
    /// Final assistant message identifier produced by the child controller.
    /// 子控制器产出的最终助手消息标识。
    pub final_assistant_message_id: Option<String>,
    /// Final text surfaced to the model-visible task result.
    /// 暴露给模型可见任务结果的最终文本。
    pub final_output_text: String,
    /// Whether the child controller ended in an error state.
    /// 子控制器是否以错误状态结束。
    pub is_error: bool,
}

/// Guard that unregisters one backend when it goes out of scope.
/// 作用域结束时自动注销某个后端的守卫对象。
pub struct TaskBackendGuard {
    /// Shared runtime registry that owns the backend map.
    /// 持有后端映射表的共享运行时注册表。
    runtime: Arc<TaskRuntime>,
    /// Session identifier whose backend should be removed on drop.
    /// 在 Drop 时应移除其后端的会话标识。
    session_id: SessionId,
}

impl Drop for TaskBackendGuard {
    /// Removes the installed backend as soon as the guard leaves scope.
    /// 在守卫离开作用域时立即移除已安装的后端。
    fn drop(&mut self) {
        self.runtime.remove_backend(self.session_id);
    }
}

/// Host-managed registry for resumable task sessions and per-session task execution backends.
/// 宿主管理的可恢复任务会话与按会话划分任务执行后端的注册表。
pub struct TaskRuntime {
    /// Stored task-session records keyed by stable task identifier.
    /// 按稳定任务标识索引存储的任务会话记录。
    sessions: RwLock<HashMap<SessionId, TaskSessionRecord>>,
    /// Backend handlers keyed by the invoking controller session identifier.
    /// 按执行中控制器会话标识索引存储的后端处理器。
    backends: RwLock<HashMap<SessionId, TaskExecutionHandler>>,
    /// Maximum supported nested task depth.
    /// 支持的最大嵌套任务深度。
    max_depth: usize,
}

impl Default for TaskRuntime {
    /// Builds one empty task runtime with the default recursion guardrail.
    /// 构建一份带默认递归防护的空任务运行时。
    fn default() -> Self {
        Self::new()
    }
}

impl TaskRuntime {
    /// Builds one empty task runtime with the default recursion guardrail.
    /// 构建一份带默认递归防护的空任务运行时。
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            backends: RwLock::new(HashMap::new()),
            max_depth: DEFAULT_TASK_NESTING_LIMIT,
        }
    }

    /// Returns the maximum supported nested task depth.
    /// 返回支持的最大嵌套任务深度。
    pub fn max_depth(&self) -> usize {
        self.max_depth
    }

    /// Installs one backend handler for the given controller session and returns a guard that unregisters it on drop.
    /// 为给定控制器会话安装一个后端处理器，并返回一个在 Drop 时自动注销它的守卫。
    pub fn install_backend(
        self: &Arc<Self>,
        session_id: SessionId,
        handler: TaskExecutionHandler,
    ) -> TaskBackendGuard {
        self.backends
            .write()
            .expect("task backend registry lock")
            .insert(session_id, handler);
        TaskBackendGuard {
            runtime: Arc::clone(self),
            session_id,
        }
    }

    /// Executes one task tool invocation through the backend registered for the current controller session.
    /// 通过为当前控制器会话注册的后端执行一次任务工具调用。
    pub async fn invoke_task(
        self: &Arc<Self>,
        current_session_id: SessionId,
        args: Value,
    ) -> Result<VulcanToolInvocationResult, String> {
        let backend = self
            .backend_for_session(current_session_id)
            .ok_or_else(|| {
                format!(
                    "当前会话 `{}` 尚未安装 task 后端，无法执行子任务。",
                    current_session_id
                )
            })?;

        let parsed_args = parse_task_tool_arguments(args)?;
        let (task_id, is_resumed) =
            resolve_task_session_id(self, current_session_id, &parsed_args)?;
        let invocation = prepare_task_execution_request(
            self,
            current_session_id,
            task_id,
            parsed_args,
            is_resumed,
        )?;
        let outcome = match backend(invocation.clone()).await {
            Ok(outcome) => outcome,
            Err(error) => {
                self.mark_task_failure(current_session_id, &invocation, &error)?;
                return Err(error);
            }
        };

        self.store_task_outcome(current_session_id, &invocation, &outcome)?;

        Ok(build_task_invocation_result(&invocation, &outcome))
    }

    /// Returns one backend handler for the given controller session identifier when it exists.
    /// 当给定控制器会话标识存在时，返回对应的后端处理器。
    fn backend_for_session(&self, session_id: SessionId) -> Option<TaskExecutionHandler> {
        self.backends
            .read()
            .expect("task backend registry lock")
            .get(&session_id)
            .cloned()
    }

    /// Removes one backend handler from the registry.
    /// 从注册表中移除一个后端处理器。
    fn remove_backend(&self, session_id: SessionId) {
        self.backends
            .write()
            .expect("task backend registry lock")
            .remove(&session_id);
    }

    /// Stores one newly completed task outcome back into the session registry.
    /// 将一条新完成的任务结果回写到会话注册表中。
    fn store_task_outcome(
        &self,
        current_session_id: SessionId,
        invocation: &TaskExecutionRequest,
        outcome: &TaskExecutionOutcome,
    ) -> Result<(), String> {
        let mut sessions = self
            .sessions
            .write()
            .map_err(|_| String::from("任务会话注册表已锁定，无法写入结果。"))?;
        let now = current_unix_millis_text();
        let status = if outcome.is_error {
            String::from("failed")
        } else {
            String::from("completed")
        };
        let record = sessions
            .entry(invocation.task_id.clone())
            .or_insert_with(|| {
                build_task_session_record(
                    invocation.task_id,
                    current_session_id,
                    invocation.parent_task_id,
                    invocation.depth,
                    invocation.description.clone(),
                    invocation.subagent.clone(),
                    invocation
                        .controller_config
                        .clone()
                        .or_else(|| Some(outcome.controller_config.clone())),
                    invocation.snapshot.clone(),
                    invocation.prompt.clone(),
                    String::from("created"),
                )
            });
        debug_assert_eq!(record.task_id, invocation.task_id);
        let _ = record.origin_session_id;
        let _ = &record.created_at;
        record.controller_config = Some(outcome.controller_config.clone());
        record.snapshot = outcome.snapshot.clone();
        record.last_prompt = invocation.prompt.clone();
        record.last_result_text = outcome.final_output_text.clone();
        record.updated_at = now;
        record.last_status = status;
        if record.description.is_none() {
            record.description = invocation.description.clone();
        }
        if record.subagent.is_none() {
            record.subagent = invocation.subagent.clone();
        }
        Ok(())
    }

    /// Stores one failed task outcome while preserving the latest snapshot for later resumption.
    /// 保存一条失败的任务结果，同时保留最新快照供后续续跑使用。
    fn mark_task_failure(
        &self,
        current_session_id: SessionId,
        invocation: &TaskExecutionRequest,
        error: &str,
    ) -> Result<(), String> {
        let mut sessions = self
            .sessions
            .write()
            .map_err(|_| String::from("任务会话注册表已锁定，无法写入失败状态。"))?;
        let record = sessions
            .entry(invocation.task_id.clone())
            .or_insert_with(|| {
                build_task_session_record(
                    invocation.task_id,
                    current_session_id,
                    invocation.parent_task_id,
                    invocation.depth,
                    invocation.description.clone(),
                    invocation.subagent.clone(),
                    invocation.controller_config.clone(),
                    invocation.snapshot.clone(),
                    invocation.prompt.clone(),
                    String::from("created"),
                )
            });
        debug_assert_eq!(record.task_id, invocation.task_id);
        let _ = record.origin_session_id;
        let _ = &record.created_at;
        record.last_result_text = error.to_string();
        record.updated_at = current_unix_millis_text();
        record.last_status = String::from("failed");
        if record.controller_config.is_none() {
            record.controller_config = invocation.controller_config.clone();
        }
        Ok(())
    }
}

/// Builds one task tool candidate backed by the shared task runtime registry.
/// 根据共享任务运行时注册表构建一条任务工具候选项。
pub fn build_task_tool_candidate(task_runtime: Arc<TaskRuntime>) -> VulcanToolCandidate {
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: String::from("task"),
            name: String::from("task"),
            description: String::from("创建或续跑一个可恢复的子任务会话。"),
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: task_tool_args_schema(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Hidden,
        invoke: Arc::new(move |args, session_id| {
            let task_runtime = Arc::clone(&task_runtime);
            Box::pin(async move {
                let Some(session_id) = session_id.as_deref() else {
                    return Err(String::from("task 工具缺少会话标识，无法执行。"));
                };
                let session_id = Uuid::parse_str(session_id)
                    .map_err(|error| format!("task 工具会话标识解析失败：{error}"))?;
                task_runtime.invoke_task(session_id, args).await
            })
        }),
    }
}

/// Builds the JSON Schema advertised to the model for the `task` tool.
/// 为模型端 `task` 工具构建对外声明的 JSON Schema。
fn task_tool_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "prompt": {
                "type": "string",
                "description": "用于启动或续跑子任务的提示词。"
            },
            "description": {
                "type": "string",
                "description": "可选的任务描述，用于展示和系统提示词增强。"
            },
            "task_id": {
                "type": "string",
                "description": "可选的稳定任务标识，传入后可继续同一子任务。"
            },
            "subagent": {
                "type": "string",
                "description": "可选的子代理标签，仅用于展示与提示词增强。"
            }
        },
        "required": ["prompt"],
        "additionalProperties": false
    })
}

/// Parses one raw JSON payload into the strongly typed task invocation arguments.
/// 将一份原始 JSON 载荷解析为强类型任务调用参数。
fn parse_task_tool_arguments(args: Value) -> Result<TaskToolInvocationArgs, String> {
    serde_json::from_value(args).map_err(|error| format!("task 工具参数解析失败：{error}"))
}

/// Resolves the stable task identifier and resume flag for the current invocation.
/// 为当前调用解析稳定任务标识与续跑标志。
fn resolve_task_session_id(
    task_runtime: &Arc<TaskRuntime>,
    current_session_id: SessionId,
    args: &TaskToolInvocationArgs,
) -> Result<(SessionId, bool), String> {
    if let Some(task_id_text) = args.task_id.as_ref() {
        let task_id = Uuid::parse_str(task_id_text)
            .map_err(|error| format!("task_id `{task_id_text}` 解析失败：{error}"))?;
        let resumed = task_runtime
            .sessions
            .read()
            .map_err(|_| String::from("任务会话注册表已锁定，无法检查既有任务。"))?
            .contains_key(&task_id);
        return Ok((task_id, resumed));
    }

    let current_depth = task_runtime
        .sessions
        .read()
        .map_err(|_| String::from("任务会话注册表已锁定，无法计算任务深度。"))?
        .get(&current_session_id)
        .map(|record| record.depth)
        .unwrap_or(0);
    if current_depth + 1 > task_runtime.max_depth() {
        return Err(format!(
            "当前任务链已达到最大深度 `{}`，无法再创建新的子任务。",
            task_runtime.max_depth()
        ));
    }

    Ok((Uuid::new_v4(), false))
}

/// Prepares one task execution request from the current invocation and the stored registry state.
/// 根据当前调用与已存储注册表状态准备一份任务执行请求。
fn prepare_task_execution_request(
    task_runtime: &Arc<TaskRuntime>,
    current_session_id: SessionId,
    task_id: SessionId,
    args: TaskToolInvocationArgs,
    is_resumed: bool,
) -> Result<TaskExecutionRequest, String> {
    let (parent_task_id, depth, snapshot, controller_config) = {
        let mut sessions = task_runtime
            .sessions
            .write()
            .map_err(|_| String::from("任务会话注册表已锁定，无法准备执行请求。"))?;
        if let Some(record) = sessions.get_mut(&task_id) {
            if is_resumed {
                record.updated_at = current_unix_millis_text();
                if record.description.is_none() {
                    record.description = args.description.clone();
                }
                if record.subagent.is_none() {
                    record.subagent = args.subagent.clone();
                }
            }
            (
                record.parent_task_id,
                record.depth,
                record.snapshot.clone(),
                record.controller_config.clone(),
            )
        } else {
            let current_depth = sessions
                .get(&current_session_id)
                .map(|record| record.depth)
                .unwrap_or(0);
            let depth = if current_depth == 0 {
                1
            } else {
                current_depth + 1
            };
            if depth > task_runtime.max_depth() {
                return Err(format!(
                    "当前任务链已达到最大深度 `{}`，无法再创建新的子任务。",
                    task_runtime.max_depth()
                ));
            }
            let parent_task_id = sessions
                .get(&current_session_id)
                .map(|record| record.task_id);
            let timestamp = current_unix_millis_text();
            sessions.insert(
                task_id,
                TaskSessionRecord {
                    task_id,
                    origin_session_id: current_session_id,
                    parent_task_id,
                    depth,
                    description: args.description.clone(),
                    subagent: args.subagent.clone(),
                    controller_config: None,
                    snapshot: CoreConversationSnapshot::default(),
                    last_prompt: args.prompt.clone(),
                    last_result_text: String::new(),
                    created_at: timestamp.clone(),
                    updated_at: timestamp,
                    last_status: String::from("created"),
                },
            );
            (
                parent_task_id,
                depth,
                CoreConversationSnapshot::default(),
                None,
            )
        }
    };

    Ok(TaskExecutionRequest {
        current_session_id,
        task_id,
        prompt: args.prompt,
        description: args.description,
        subagent: args.subagent,
        parent_task_id,
        depth,
        snapshot,
        controller_config,
        is_resumed,
    })
}

/// Builds one task-session record with the provided metadata and execution state.
/// 使用给定元数据与执行状态构建一条任务会话记录。
fn build_task_session_record(
    task_id: SessionId,
    origin_session_id: SessionId,
    parent_task_id: Option<SessionId>,
    depth: usize,
    description: Option<String>,
    subagent: Option<String>,
    controller_config: Option<CoreControllerConfig>,
    snapshot: CoreConversationSnapshot,
    last_prompt: String,
    last_status: String,
) -> TaskSessionRecord {
    let timestamp = current_unix_millis_text();
    TaskSessionRecord {
        task_id,
        origin_session_id,
        parent_task_id,
        depth,
        description,
        subagent,
        controller_config,
        snapshot,
        last_prompt,
        last_result_text: String::new(),
        created_at: timestamp.clone(),
        updated_at: timestamp,
        last_status,
    }
}

/// Builds one model-visible task invocation result from the execution outcome.
/// 根据执行结果构建一条模型可见的任务调用结果。
fn build_task_invocation_result(
    invocation: &TaskExecutionRequest,
    outcome: &TaskExecutionOutcome,
) -> VulcanToolInvocationResult {
    let task_status = task_invocation_status_text(invocation.is_resumed);
    let execution_status = if outcome.is_error {
        "failed"
    } else {
        "completed"
    };
    let model_output = json!({
        "task_id": outcome.task_id.to_string(),
        "status": task_status,
        "execution_status": execution_status,
        "origin_session_id": invocation.current_session_id.to_string(),
        "parent_task_id": invocation.parent_task_id.map(|value| value.to_string()),
        "depth": invocation.depth,
        "description": invocation.description.clone(),
        "subagent": invocation.subagent.clone(),
        "provider_turns_executed": outcome.provider_turns_executed,
        "tool_calls_processed": outcome.tool_calls_processed,
        "final_assistant_message_id": outcome.final_assistant_message_id.clone(),
        "result": outcome.final_output_text.clone(),
    });
    let conversation_state = core_snapshot_to_session_conversation_state(&outcome.snapshot);
    let payload = build_task_host_payload(
        invocation,
        execution_status,
        outcome.provider_turns_executed,
        outcome.tool_calls_processed,
        outcome.final_assistant_message_id.as_deref(),
        outcome.final_output_text.as_str(),
        Some(&conversation_state),
        current_unix_millis_text(),
    );
    VulcanToolInvocationResult::from_model_output(model_output).with_host_result(
        VulcanToolHostResult {
            kind: String::from("task_session"),
            payload,
        },
    )
}

/// Returns one RFC3339-like Unix-millis text stamp for task-session records.
/// 为任务会话记录返回一条类似 RFC3339 的 Unix 毫秒时间戳文本。
fn current_unix_millis_text() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|_| String::from("0"))
}

/// Builds one backend handler that can execute a concrete task session using the provided host execution context.
/// 根据给定宿主执行上下文构建一条可执行具体任务会话的后端处理器。
pub fn build_task_execution_handler<A>(
    task_runtime: Arc<TaskRuntime>,
    provider_factory: Arc<impl Fn(ProviderLiveDeltaSender) -> A + Send + Sync + 'static>,
    base_controller_config: CoreControllerConfig,
    tools_manager: Arc<VulcanToolsManager>,
    resolved_tool_definitions: Vec<ResolvedToolDefinition>,
    parent_live_sender: ProviderLiveDeltaSender,
    provider_label: String,
) -> TaskExecutionHandler
where
    A: Clone
        + Send
        + Sync
        + 'static
        + vulcan_core::provider::VulcanStreamingProviderAdapter<
            Response = vulcan_core::types::ProviderTurnResponse,
        >,
{
    Arc::new(move |request: TaskExecutionRequest| {
        let task_runtime = Arc::clone(&task_runtime);
        let provider_factory = Arc::clone(&provider_factory);
        let base_controller_config = base_controller_config.clone();
        let tools_manager = Arc::clone(&tools_manager);
        let resolved_tool_definitions = resolved_tool_definitions.clone();
        let parent_live_sender = parent_live_sender.clone();
        let provider_label = provider_label.clone();
        Box::pin(async move {
            let task_id = request.task_id.clone();
            let started_payload = build_task_host_payload(
                &request,
                "running",
                0,
                0,
                None,
                "",
                None,
                current_unix_millis_text(),
            );
            let _ = parent_live_sender.send(Ok(ProviderStreamDelta::RuntimeEvent(
                build_task_delegate_runtime_event(
                    RuntimeEventKind::DelegateStarted,
                    &request,
                    "运行中，等待子代理返回结果。",
                    started_payload,
                ),
            )));
            let child_handler = build_task_execution_handler(
                Arc::clone(&task_runtime),
                Arc::clone(&provider_factory),
                base_controller_config.clone(),
                Arc::clone(&tools_manager),
                resolved_tool_definitions.clone(),
                parent_live_sender.clone(),
                provider_label.clone(),
            );
            let _child_backend_guard = task_runtime.install_backend(task_id, child_handler);
            let (child_live_sender, mut child_live_receiver) =
                unbounded_channel::<Result<ProviderStreamDelta, String>>();
            let child_provider_adapter = provider_factory(child_live_sender.clone());

            let task_controller_config = build_task_controller_config(
                &base_controller_config,
                request.controller_config.clone(),
                &request.description,
                &request.subagent,
            );
            let child_stream_forward_sender = parent_live_sender.clone();
            let child_stream_session_id = task_id;
            let child_stream_forward_task = tokio::spawn({
                let streaming_controller = VulcanCoreController::new(
                    task_controller_config.clone(),
                    request.snapshot.clone(),
                );
                async move {
                    let mut child_stream_state = RuntimeStreamDeltaState::new(streaming_controller);
                    while let Some(item) = child_live_receiver.recv().await {
                        for event in process_provider_stream_delta(
                            child_stream_session_id,
                            &mut child_stream_state,
                            item,
                        ) {
                            let _ = child_stream_forward_sender
                                .send(Ok(ProviderStreamDelta::ExecutionEvent(event)));
                        }
                        if child_stream_state.has_terminal_error() {
                            break;
                        }
                    }
                    if let Some(event) =
                        child_stream_state.complete_pending_thinking(child_stream_session_id)
                    {
                        let _ = child_stream_forward_sender
                            .send(Ok(ProviderStreamDelta::ExecutionEvent(event)));
                    }
                }
            });
            let child_runtime_bridge = Arc::new(VulcanManagedToolRuntimeBridge::new(
                Arc::clone(&tools_manager),
                &resolved_tool_definitions,
                task_id,
            ));
            let child_tool_executor =
                StreamingRuntimeToolExecutor::new(child_runtime_bridge, child_live_sender.clone());
            let mut controller =
                VulcanCoreController::new(task_controller_config.clone(), request.snapshot.clone());
            let task_policy = AllowAllToolPolicy;
            let controller_result = controller
                .run_user_turn_streaming(
                    &child_provider_adapter,
                    &child_tool_executor,
                    &task_policy,
                    request.prompt.clone(),
                )
                .await;
            drop(child_tool_executor);
            drop(child_provider_adapter);
            drop(child_live_sender);
            let _ = child_stream_forward_task.await;
            let controller_result =
                controller_result.map_err(|error| format!("任务 `{}` 执行失败：{error}", task_id));
            let snapshot = controller.snapshot().clone();
            let child_conversation_state = core_snapshot_to_session_conversation_state(&snapshot);
            let controller_result = match controller_result {
                Ok(controller_result) => controller_result,
                Err(error) => {
                    let execution_status = if task_execution_error_is_stopped(error.as_str()) {
                        "stopped"
                    } else {
                        "failed"
                    };
                    let finished_payload = build_task_host_payload(
                        &request,
                        execution_status,
                        0,
                        0,
                        None,
                        error.as_str(),
                        Some(&child_conversation_state),
                        current_unix_millis_text(),
                    );
                    let _ = parent_live_sender.send(Ok(ProviderStreamDelta::RuntimeEvent(
                        build_task_delegate_runtime_event(
                            RuntimeEventKind::DelegateFinished,
                            &request,
                            error.as_str(),
                            finished_payload,
                        ),
                    )));
                    return Err(error);
                }
            };
            let final_output_text = extract_final_assistant_text(&snapshot).unwrap_or_else(|| {
                if let Some(message_id) = controller_result.final_assistant_message_id.as_ref() {
                    format!("任务已完成，但未能直接定位最终助手消息 `{message_id}`。")
                } else {
                    String::from("任务已完成，但未产出可复用的最终助手文本。")
                }
            });
            let finished_payload = build_task_host_payload(
                &request,
                "completed",
                controller_result.provider_turns_executed,
                controller_result.tool_calls_processed,
                controller_result.final_assistant_message_id.as_deref(),
                final_output_text.as_str(),
                Some(&child_conversation_state),
                current_unix_millis_text(),
            );
            let _ = parent_live_sender.send(Ok(ProviderStreamDelta::RuntimeEvent(
                build_task_delegate_runtime_event(
                    RuntimeEventKind::DelegateFinished,
                    &request,
                    "子代理任务已完成。",
                    finished_payload,
                ),
            )));

            Ok(TaskExecutionOutcome {
                task_id,
                controller_config: task_controller_config,
                snapshot,
                provider_turns_executed: controller_result.provider_turns_executed,
                tool_calls_processed: controller_result.tool_calls_processed,
                final_assistant_message_id: controller_result.final_assistant_message_id,
                final_output_text,
                is_error: false,
            })
        })
    })
}

/// Returns the persisted task-invocation status text used by both runtime metadata and tool results.
/// 返回运行时元数据与工具结果共同使用的持久化任务调用状态文本。
fn task_invocation_status_text(is_resumed: bool) -> &'static str {
    if is_resumed { "resumed" } else { "created" }
}

/// Returns whether one task execution error most likely came from a user-visible stop or transport cancellation rather than a real logical failure.
/// 返回某条任务执行错误是否更可能源于用户可见的停止或传输取消，而不是真正的逻辑失败。
fn task_execution_error_is_stopped(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    normalized.contains("cancelled")
        || normalized.contains("canceled")
        || normalized.contains("aborted")
        || normalized.contains("stopped")
        || error.contains("已停止")
        || error.contains("中止")
}

/// Builds one host-only payload for task-session UI metadata without leaking large child snapshots into the model-visible tool result.
/// 构建一份仅供宿主界面使用的任务会话载荷，避免把庞大的子会话快照泄漏到模型可见工具结果中。
fn build_task_host_payload(
    invocation: &TaskExecutionRequest,
    execution_status: &str,
    provider_turns_executed: usize,
    tool_calls_processed: usize,
    final_assistant_message_id: Option<&str>,
    result_text: &str,
    conversation_state: Option<&SessionConversationState>,
    updated_at: String,
) -> Value {
    let mut payload = json!({
        "task_id": invocation.task_id.to_string(),
        "status": task_invocation_status_text(invocation.is_resumed),
        "execution_status": execution_status,
        "origin_session_id": invocation.current_session_id.to_string(),
        "parent_task_id": invocation.parent_task_id.map(|value| value.to_string()),
        "depth": invocation.depth,
        "prompt": invocation.prompt.clone(),
        "description": invocation.description.clone(),
        "subagent": invocation.subagent.clone(),
        "provider_turns_executed": provider_turns_executed,
        "tool_calls_processed": tool_calls_processed,
        "final_assistant_message_id": final_assistant_message_id,
        "result": result_text,
        "updated_at": updated_at,
    });
    if let Some(conversation_state) = conversation_state {
        payload["conversation_state"] = json!(conversation_state);
    }
    payload
}

/// Builds one delegate runtime event used by the parent session to track child task lifecycle without replaying child transcript output inline.
/// 构建一条供父会话追踪子任务生命周期的委派运行时事件，同时避免把子任务正文直接回放到父会话中。
fn build_task_delegate_runtime_event(
    kind: RuntimeEventKind,
    invocation: &TaskExecutionRequest,
    detail: &str,
    payload: Value,
) -> RuntimeEvent {
    let task_label = invocation
        .subagent
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            invocation
                .description
                .as_deref()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or("子代理");
    let title = match kind {
        RuntimeEventKind::DelegateStarted => {
            format!("子代理启动 · {} · {task_label}", invocation.task_id)
        }
        RuntimeEventKind::DelegateFinished => {
            format!("子代理结束 · {} · {task_label}", invocation.task_id)
        }
        _ => format!("子代理事件 · {} · {task_label}", invocation.task_id),
    };

    RuntimeEvent::new(kind, title, detail).with_metadata(Some(json!({
        "kind": "task_session",
        "payload": payload,
    })))
}

/// Builds one task controller configuration from the base host configuration and the task-specific metadata.
/// 根据基础宿主配置和任务专属元数据构建一份任务控制器配置。
fn build_task_controller_config(
    base: &CoreControllerConfig,
    stored: Option<CoreControllerConfig>,
    description: &Option<String>,
    subagent: &Option<String>,
) -> CoreControllerConfig {
    if let Some(stored) = stored {
        return stored;
    }

    let mut config = base.clone();
    let mut system_prompt = config.system_prompt.clone();
    if let Some(subagent) = subagent.as_ref().filter(|value| !value.trim().is_empty()) {
        system_prompt.push_str(&format!("\n\n[Subagent]\n{subagent}"));
    }
    if let Some(description) = description
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        system_prompt.push_str(&format!("\n\n[Task]\n{description}"));
    }
    append_task_windows_path_guidance(&mut system_prompt);
    if !system_prompt.trim().is_empty() {
        config.system_prompt = system_prompt;
    }
    config
}

/// Appends one Windows-specific filesystem path guidance block to the child task system prompt.
/// 为子任务系统提示词追加一段 Windows 专用文件系统路径指引。
fn append_task_windows_path_guidance(system_prompt: &mut String) {
    if !cfg!(windows) || system_prompt.contains("[Filesystem Paths]") {
        return;
    }
    system_prompt.push_str("\n\n");
    system_prompt.push_str(WINDOWS_TASK_FILESYSTEM_PATH_GUIDANCE);
}

/// Extracts the last visible assistant text from one canonical conversation snapshot.
/// 从一份规范会话快照中提取最后一条可见助手文本。
fn extract_final_assistant_text(snapshot: &CoreConversationSnapshot) -> Option<String> {
    snapshot.messages.iter().rev().find_map(|message| {
        if let vulcan_core::types::ConversationPayload::Assistant { text, .. } = &message.payload {
            if !text.trim().is_empty() {
                return Some(text.clone());
            }
        }
        None
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one minimal controller configuration for task-runtime prompt tests.
    /// 为 task-runtime 提示词测试构建一份最小控制器配置。
    fn build_test_controller_config(system_prompt: &str) -> CoreControllerConfig {
        CoreControllerConfig {
            provider_id: String::from("provider"),
            model_id: String::from("model"),
            system_prompt: system_prompt.to_string(),
            temperature: None,
            max_turns: 8,
            additional_params: None,
        }
    }

    /// Verifies that Windows child task prompts always include normalized filesystem path guidance.
    /// 验证 Windows 子任务提示词始终包含规范化文件系统路径指引。
    #[test]
    #[cfg(windows)]
    fn build_task_controller_config_appends_windows_path_guidance() {
        let base = build_test_controller_config("Base prompt");
        let description = Some(String::from("Analyze the current repository."));
        let subagent = Some(String::from("explorer"));

        let config = build_task_controller_config(&base, None, &description, &subagent);

        assert!(config.system_prompt.contains("[Subagent]\nexplorer"));
        assert!(
            config
                .system_prompt
                .contains("[Task]\nAnalyze the current repository.")
        );
        assert!(config.system_prompt.contains("[Filesystem Paths]"));
        assert!(config.system_prompt.contains("D:/repo/path"));
    }
}
