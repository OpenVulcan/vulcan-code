//! Host-authorized configured inference orchestration.
//! 宿主授权的已配置推理编排。

use futures_util::StreamExt;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    future::Future,
    path::PathBuf,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use time::OffsetDateTime;
use tokio::sync::{Mutex as AsyncMutex, Notify, broadcast, oneshot, watch};
use uuid::Uuid;
use vulcan_agent_runtime::agent_runtime::provider_reasoning::resolve_effective_public_reasoning_effort;
use vulcan_agent_runtime::agent_runtime::provider_request_telemetry::{
    ProviderRequestCompletedUsage, ProviderRequestTelemetryEvent, ProviderRequestTelemetryEventKind,
};
use vulcan_agent_runtime::agent_runtime::provider_transport_retry::{
    ProviderRetryDecision, ProviderRetryPolicy,
};
use vulcan_agent_runtime::provider_input_normalizer::protocol_profile_supports_structured_output;
use vulcan_ai_contract::{
    AiErrorCategory, AiExecutionError, AiExecutionRequest, AiOperation, AiStreamEvent, ContentItem,
    DispatchOutcome, EmbeddingBatchCollectedResult, EmbeddingBatchPlan, EmbeddingBatchSnapshot,
    EmbeddingBatchSubmission, EmbeddingBatchTaskPlan, EmbeddingBatchUploadedInput,
    EmbeddingInputContent, EmbeddingRequest, EmbeddingResponse, ExecutionRouteSnapshot,
    InferenceCallerContext, InferenceCapabilitySnapshot, InferenceOperationCapability,
    InferencePermission, InferenceUsageScope, ModelCapabilities, ModelParameterValue,
    NativeToolKind, PromptCachePolicy, ReasoningControlMode, ReasoningEffort, RequestUsage,
    RerankRequest, RerankResponse, ResourceKind, ResponsesExecutionOutputItem,
    ResponsesExecutionRequest, ResponsesExecutionResult, StructuredOutputCapability,
    StructuredOutputPolicy, ToolCall, ToolChoicePolicy, UsageValue,
};
use vulcan_hooks::{HookContext, HookScope};
use vulcan_llm_core::{
    ANTHROPIC_MESSAGES_PROFILE_ID, ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
    CLAUDE_CODE_MESSAGES_PROFILE_ID, COHERE_CHAT_PROFILE_ID, GEMINI_GENERATE_CONTENT_PROFILE_ID,
    GEMINI_INTERACTIONS_PROFILE_ID, GOOGLE_CODE_ASSIST_PROFILE_ID, OPENAI_CHAT_PROFILE_ID,
    OPENAI_CODEX_RESPONSES_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,
};
use vulcan_plugin_runtime::VulcanPluginRuntimeManager;
use vulcan_provider_adapters::{
    EmbeddingBatchArtifactKind, EmbeddingBatchArtifactSink, InferenceRouteConfiguration,
    InternalModelOverrides, PersistedAiConfiguration, ProviderExecutionFailure,
    ProviderWireRequestCapture, PublicAiRuntime, ResolvedPublicAiSelection,
    registered_native_embedding_batch_capabilities,
};
use vulcan_provider_config::config::PersistedProviderRetryConfiguration;
use vulcan_workspace_store::data_plane::HostRuntimeHandle;

use crate::provider_usage_runtime::persist_native_provider_usage_attempt;

/// Bounds process-local event buffering for one restricted LLM execution.
/// 限制一项受限 LLM 执行的进程内事件缓冲。
const LLM_EVENT_BUFFER_CAPACITY: usize = 128;
/// Bounds retained normalized event payloads, including image previews, per execution.
/// 限制每项执行保留的归一化事件载荷（包括图片预览）总量。
const LLM_EVENT_BUFFER_MAX_BYTES: usize = 64 * 1024 * 1024;
/// Versioned disk contract for VMM configured-inference transport request traces.
/// VMM 已配置推理传输请求追踪的版本化磁盘契约。
const CONFIGURED_INFERENCE_PROVIDER_REQUEST_TRACE_SCHEMA_VERSION: &str =
    "vulcan.configured-inference-provider-request-trace.v1";
/// Versioned disk contract for VCE native embedding request diagnostics.
/// VCE 原生向量请求诊断使用的版本化磁盘契约。
const VCE_EMBEDDING_PROVIDER_REQUEST_TRACE_SCHEMA_VERSION: &str =
    "vulcan.vce-embedding-provider-request-trace.v1";

/// Owns opt-in VCE Batch artifacts for one host-authenticated project.
/// 持有一个经宿主认证项目的自愿开启的 VCE Batch 产物。
struct VceEmbeddingBatchArtifactSink {
    /// Existing application data root whose `vce` directory is the artifact sibling.
    /// 现有应用数据根目录，其 `vce` 目录与产物目录同级。
    data_root: PathBuf,
    /// Exact project identity supplied by the VCE capability registration.
    /// 由 VCE 能力注册提供的精确项目身份。
    project_id: String,
}

#[async_trait::async_trait]
impl EmbeddingBatchArtifactSink for VceEmbeddingBatchArtifactSink {
    /// Writes exact request or response bytes off the async executor.
    /// 在异步执行器之外写入精确请求或返回字节。
    ///
    /// `task_id` identifies one immutable Batch task, `kind` chooses its file, and `bytes` stay exact.
    /// `task_id` 标识一个不可变 Batch 任务，`kind` 选择文件，`bytes` 保持精确。
    ///
    /// Returns a storage error without changing the Provider operation.
    /// 返回存储错误而不改变供应商操作。
    async fn persist(
        &self,
        task_id: &str,
        kind: EmbeddingBatchArtifactKind,
        bytes: &[u8],
    ) -> Result<(), String> {
        // Map the typed artifact to its fixed diagnostic file name.
        // 将强类型产物映射为固定诊断文件名。
        let file_name = match kind {
            EmbeddingBatchArtifactKind::RequestJsonl => String::from("request.jsonl"),
            EmbeddingBatchArtifactKind::RequestJson => String::from("request.json"),
            EmbeddingBatchArtifactKind::OutputJsonl => String::from("output.jsonl"),
            EmbeddingBatchArtifactKind::OutputJson => String::from("output.json"),
            EmbeddingBatchArtifactKind::ErrorJsonl => String::from("error.jsonl"),
            EmbeddingBatchArtifactKind::TerminalStatusJson => {
                format!("status-{}.json", hex::encode(Sha256::digest(bytes)))
            }
        };
        // Move only owned values into the blocking filesystem worker.
        // 仅将拥有所有权的值移入阻塞式文件系统工作线程。
        let data_root = self.data_root.clone();
        let project_id = self.project_id.clone();
        let task_id = task_id.to_owned();
        let bytes = bytes.to_vec();
        tokio::task::spawn_blocking(move || {
            vulcan_workspace_store::workspace_store::persist_vce_batch_artifact(
                &data_root,
                project_id.as_str(),
                task_id.as_str(),
                file_name.as_str(),
                &bytes,
            )
        })
        .await
        .map_err(|error| format!("VCE Batch artifact writer failed: {error}"))??;
        Ok(())
    }
}
/// Process-local tie breaker for configured-inference traces captured in one millisecond.
/// 同一毫秒内捕获的已配置推理追踪使用的进程内消歧序号。
static CONFIGURED_INFERENCE_PROVIDER_REQUEST_TRACE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Identifies the VMM internal-inference consumer profile.
/// 标识 VMM 内部推理消费者配置档。
pub(crate) const VMM_CONSUMER_PROFILE_ID: &str = "vmm";

/// Identifies billable provider/model validation requests in the native usage ledger.
/// 标识原生用量账本中的可计费供应商/模型验证请求。
const PROVIDER_MODEL_VALIDATION_CONSUMER_ID: &str = "vulcan-code-provider-validation";

/// Retains terminal LLM status briefly for polling and reconnect diagnostics.
/// 短暂保留 LLM 终态，供轮询与重连诊断使用。
const LLM_TERMINAL_RETENTION: Duration = Duration::from_secs(15 * 60);

/// Bounds all configured-inference work admitted through the shared facade.
/// 限制通过共享门面受理的全部已配置推理工作。
const INFERENCE_SCHEDULER_TOTAL_CAPACITY: u32 = 6;

/// Reserves configured-inference capacity from external-service saturation.
/// 为已配置推理保留不被外部服务耗尽的容量。
const INFERENCE_SCHEDULER_EXTERNAL_CAPACITY: u32 = 3;

/// Prevents background indexing and batch work from occupying multiple shared slots.
/// 防止后台索引与批处理占用多个共享槽位。
const INFERENCE_SCHEDULER_BACKGROUND_CAPACITY: u32 = 1;

/// Allows bounded priority bypass before one waiting lower-priority class must progress.
/// 允许有限次数优先级越过，之后必须推进一个等待中的较低优先级类别。
const INFERENCE_SCHEDULER_MAX_PRIORITY_BYPASS: u32 = 8;

/// Defines how a caller deadline is classified after one Batch provider future starts polling.
/// 定义一个 Batch 供应商 Future 开始轮询后调用方截止时间的分派分类方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EmbeddingBatchDeadlineDispatchPolicy {
    /// The operation is safely repeatable and does not require uncertain-dispatch recovery.
    /// 操作可安全重复，不需要不确定分派恢复。
    Repeatable,
    /// The operation can create a remote file or billable task, so delivery is indeterminate.
    /// 操作可能创建远端文件或计费任务，因此送达情况不确定。
    NonIdempotent,
}

/// Identifies one stable configured-inference failure category.
/// 标识一种稳定的已配置推理失败类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfiguredInferenceErrorCategory {
    /// The versioned request contract is invalid.
    /// 版本化请求契约无效。
    Contract,
    /// The trusted caller lacks permission.
    /// 可信调用方缺少权限。
    Authorization,
    /// The configured profile or route cannot execute.
    /// 已配置 Profile 或路由无法执行。
    Configuration,
    /// A host budget or deadline was exceeded.
    /// 超过宿主预算或截止时间。
    Limit,
    /// Provider dispatch or stream processing failed.
    /// 供应商分派或流处理失败。
    Execution,
    /// The owning caller cancelled the execution.
    /// 所属调用方取消了执行。
    Cancelled,
}

/// Carries one controlled configured-inference failure.
/// 承载一项受控的已配置推理失败。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConfiguredInferenceError {
    /// Stable public category.
    /// 稳定公共类别。
    pub category: ConfiguredInferenceErrorCategory,
    /// Controlled diagnostic without credential material.
    /// 不含凭据材料的受控诊断。
    pub message: String,
    /// Optional normalized provider error.
    /// 可选归一化供应商错误。
    pub provider_error: Option<Box<AiExecutionError>>,
    /// Exact non-secret route when provider selection completed before the failure.
    /// 失败前已完成 Provider 选择时的准确非敏感路由。
    pub route: Option<Box<ExecutionRouteSnapshot>>,
}

impl ConfiguredInferenceError {
    /// Returns the stable public category label used by restricted stream events.
    /// 返回受限流事件使用的稳定公共类别标签。
    ///
    /// # Returns
    /// 返回值
    /// A lower-case category identifier.
    /// 小写类别标识。
    fn category_label(&self) -> &'static str {
        match self.category {
            ConfiguredInferenceErrorCategory::Contract => "contract",
            ConfiguredInferenceErrorCategory::Authorization => "authorization",
            ConfiguredInferenceErrorCategory::Configuration => "configuration",
            ConfiguredInferenceErrorCategory::Limit => "limit",
            ConfiguredInferenceErrorCategory::Execution => "execution",
            ConfiguredInferenceErrorCategory::Cancelled => "cancelled",
        }
    }

    /// Builds one controlled failure without provider detail.
    /// 构建一项不含供应商详情的受控失败。
    ///
    /// # Parameters
    /// 参数
    /// `category` and `message` are stable host-selected diagnostics.
    /// `category` 与 `message` 是宿主选择的稳定诊断。
    ///
    /// # Returns
    /// 返回值
    /// A structured configured-inference error.
    /// 返回结构化已配置推理错误。
    fn new(category: ConfiguredInferenceErrorCategory, message: impl Into<String>) -> Self {
        Self {
            category,
            message: message.into(),
            provider_error: None,
            route: None,
        }
    }

    /// Converts one adapter failure without losing normalized provider semantics.
    /// 转换一项适配器失败且不丢失归一化供应商语义。
    ///
    /// # Parameters
    /// 参数
    /// `failure` is returned by the exact frozen adapter route.
    /// `failure` 由精确冻结的适配器路由返回。
    ///
    /// # Returns
    /// 返回值
    /// An execution-category failure retaining the provider error.
    /// 返回保留供应商错误的执行类别失败。
    fn from_provider_failure(failure: ProviderExecutionFailure) -> Self {
        let message = failure.error.message.clone();
        Self {
            category: ConfiguredInferenceErrorCategory::Execution,
            message,
            provider_error: Some(failure.error),
            route: None,
        }
    }

    /// Preserves an indeterminate dispatch outcome when a non-idempotent Batch call reaches its deadline.
    /// 在非幂等 Batch 调用达到截止时间时保留不确定分派结果。
    ///
    /// # Parameters
    /// 参数
    /// `self` is the caller-deadline failure produced after the provider future started polling.
    /// `self` 是供应商 Future 开始轮询后产生的调用方截止时间失败。
    ///
    /// # Returns
    /// 返回值
    /// The same controlled failure carrying a normalized timeout with unknown dispatch semantics.
    /// 返回携带规范化超时及不确定分派语义的同一受控失败。
    fn with_unknown_batch_dispatch(mut self) -> Self {
        self.provider_error = Some(Box::new(AiExecutionError {
            category: AiErrorCategory::Timeout,
            subcategory: Some(String::from("caller_deadline")),
            stage: String::from("dispatch"),
            http_status: None,
            transport: None,
            websocket_close_code: None,
            upstream_code: None,
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: None,
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message: self.message.clone(),
            dispatch_outcome: DispatchOutcome::Unknown,
            semantic_output_observed: false,
            side_effect_observed: false,
            terminal: true,
            retryable: false,
            retry_after: None,
        }));
        self
    }

    /// Converts one provider terminal event into a controlled failure.
    /// 将一个供应商终态事件转换为受控失败。
    ///
    /// # Parameters
    /// 参数
    /// `error` is the normalized public execution error emitted by the stream.
    /// `error` 是流输出的归一化公共执行错误。
    ///
    /// # Returns
    /// 返回值
    /// An execution-category failure retaining the exact terminal error.
    /// 返回保留精确终态错误的执行类别失败。
    fn from_terminal_error(error: AiExecutionError) -> Self {
        Self {
            category: ConfiguredInferenceErrorCategory::Execution,
            message: error.message.clone(),
            provider_error: Some(Box::new(error)),
            route: None,
        }
    }

    /// Attaches the exact resolved route to a failure that occurred after selection.
    /// 为发生在选择完成后的失败附加准确解析路由。
    ///
    /// # Parameters
    /// 参数
    /// `route` is the non-secret immutable route used by the physical dispatch.
    /// `route` 是本次物理分派使用的非敏感不可变路由。
    ///
    /// # Returns
    /// 返回值
    /// The same failure enriched with route diagnostics.
    /// 返回补充了路由诊断的同一失败。
    fn with_route(mut self, route: ExecutionRouteSnapshot) -> Self {
        self.route = Some(Box::new(route));
        self
    }
}

impl std::fmt::Display for ConfiguredInferenceError {
    /// Formats the controlled user-safe diagnostic.
    /// 格式化受控且用户安全的诊断。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message.as_str())
    }
}

impl std::error::Error for ConfiguredInferenceError {}

/// Identifies one restricted LLM execution phase.
/// 标识一项受限 LLM 执行阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfiguredLlmExecutionPhase {
    /// Provider execution is active.
    /// 供应商执行正在进行。
    Running,
    /// Aggregation completed successfully.
    /// 聚合已成功完成。
    Completed,
    /// Execution failed terminally.
    /// 执行已终态失败。
    Failed,
    /// The owning caller cancelled execution.
    /// 所属调用方取消了执行。
    Cancelled,
}

/// Exposes one caller-owned LLM execution snapshot.
/// 暴露一项调用方所有的 LLM 执行快照。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConfiguredLlmExecutionSnapshot {
    /// Stable host execution identifier.
    /// 稳定宿主执行标识。
    pub execution_id: String,
    /// Current terminal or running phase.
    /// 当前终态或运行阶段。
    pub phase: ConfiguredLlmExecutionPhase,
    /// Immutable non-secret execution route.
    /// 不可变且不含秘密的执行路由。
    pub route: ExecutionRouteSnapshot,
    /// Completed aggregate result when successful.
    /// 成功时的已完成聚合结果。
    pub response: Option<ResponsesExecutionResult>,
    /// Controlled terminal failure when unsuccessful.
    /// 未成功时的受控终态失败。
    pub error: Option<ConfiguredInferenceError>,
}

/// Tracks mutable state for one active or retained LLM execution.
/// 跟踪一项活动或保留 LLM 执行的可变状态。
struct ConfiguredLlmExecutionState {
    /// Stable owning caller identifier.
    /// 稳定所属调用方标识。
    owner_caller_id: String,
    /// Exact request fingerprint used for in-flight idempotency.
    /// 用于运行中幂等的精确请求指纹。
    request_fingerprint: String,
    /// In-flight de-duplication key.
    /// 运行中去重键。
    in_flight_key: String,
    /// Mutable public snapshot.
    /// 可变公共快照。
    snapshot: Mutex<ConfiguredLlmExecutionSnapshot>,
    /// Restricted normalized event sender.
    /// 受限归一化事件发送器。
    event_sender: broadcast::Sender<AiStreamEvent>,
    /// Ordered bounded replay history for subscribers that attach after dispatch.
    /// 为分派后接入订阅者保留的有序有界重放历史。
    event_history: Mutex<Vec<AiStreamEvent>>,
    /// Approximate serialized byte size of retained replay events.
    /// 已保留重放事件的近似序列化字节大小。
    event_history_bytes: Mutex<usize>,
    /// Records whether replay history discarded an older event.
    /// 记录重放历史是否已经丢弃过较早事件。
    event_history_truncated: Mutex<bool>,
    /// Cooperative cancellation sender.
    /// 协作式取消发送器。
    cancellation_sender: watch::Sender<bool>,
    /// Number of active non-streaming waiters that still own the provider execution.
    /// 仍持有供应商执行的活动非流式等待者数量。
    waiter_count: AtomicUsize,
    /// Terminal-state notification.
    /// 终态通知。
    terminal_notify: Notify,
    /// Monotonic terminal timestamp used for bounded process-local retention.
    /// 用于限制进程内保留时间的单调终态时间戳。
    terminal_at: Mutex<Option<Instant>>,
}

/// Cancels one running restricted LLM execution when its last non-streaming waiter is dropped.
/// 当最后一个非流式等待者被丢弃时取消一项仍在运行的受限 LLM 执行。
struct ConfiguredLlmWaiterGuard {
    /// Shared execution state whose waiter ownership is tracked.
    /// 跟踪等待者所有权的共享执行状态。
    state: Arc<ConfiguredLlmExecutionState>,
}

impl ConfiguredLlmWaiterGuard {
    /// Registers one active non-streaming waiter.
    /// 注册一个活动非流式等待者。
    ///
    /// # Parameters
    /// 参数
    /// `state` is the exact retained execution returned to the authenticated caller.
    /// `state` 是返回给已认证调用方的精确保留执行。
    ///
    /// # Returns
    /// 返回值
    /// An RAII guard that releases ownership on normal return or Future cancellation.
    /// 返回一个会在正常返回或 Future 取消时释放所有权的 RAII 守卫。
    fn new(state: Arc<ConfiguredLlmExecutionState>) -> Self {
        state.waiter_count.fetch_add(1, Ordering::AcqRel);
        Self { state }
    }
}

impl Drop for ConfiguredLlmWaiterGuard {
    /// Releases one waiter and signals cooperative cancellation when no running owner remains.
    /// 释放一个等待者，并在没有运行中所有者时发送协作取消信号。
    fn drop(&mut self) {
        if self.state.waiter_count.fetch_sub(1, Ordering::AcqRel) != 1 {
            return;
        }
        let running = self
            .state
            .snapshot
            .lock()
            .map(|snapshot| snapshot.phase == ConfiguredLlmExecutionPhase::Running)
            .unwrap_or(true);
        if running {
            let _ = self.state.cancellation_sender.send(true);
        }
    }
}

/// Shares one non-streaming in-flight result across exact concurrent duplicates.
/// 在精确并发重复请求之间共享一项非流式运行中结果。
struct SharedInferenceResult<T> {
    /// Exact serialized request used to reject conflicting key reuse.
    /// 用于拒绝冲突键复用的精确序列化请求。
    request_fingerprint: String,
    /// Terminal result retained only while current waiters hold this state.
    /// 仅在当前等待方持有该状态期间保留的终态结果。
    result: Mutex<Option<Result<T, ConfiguredInferenceError>>>,
    /// Wakes every concurrent duplicate after the leader reaches a terminal result.
    /// 领导请求到达终态后唤醒全部并发重复请求。
    terminal_notify: Notify,
}

/// Guarantees that cancelled leader futures release their in-flight key and wake followers.
/// 保证被取消的领导 Future 会释放运行中键并唤醒跟随请求。
struct SharedInferenceLeaderGuard<'a, T> {
    /// Operation-specific in-flight registry.
    /// 操作专属运行中注册表。
    registry: &'a Mutex<BTreeMap<String, Arc<SharedInferenceResult<T>>>>,
    /// Exact caller-operation-request identity.
    /// 精确调用方、操作、请求身份。
    key: String,
    /// Shared result state owned by the current leader.
    /// 当前领导请求拥有的共享结果状态。
    state: Arc<SharedInferenceResult<T>>,
    /// Whether a terminal result was published normally.
    /// 是否已经正常发布终态结果。
    completed: bool,
}

impl<T: Clone> SharedInferenceLeaderGuard<'_, T> {
    /// Publishes one terminal result, removes the running key, and wakes all followers.
    /// 发布一项终态结果、移除运行中键并唤醒全部跟随请求。
    ///
    /// # Parameters
    /// 参数
    /// `result` is the exact leader response or controlled failure.
    /// `result` 是精确领导响应或受控失败。
    ///
    /// # Returns
    /// 返回值
    /// The same result after it becomes visible to current followers.
    /// 结果对当前跟随请求可见后返回同一结果。
    fn complete(
        mut self,
        result: Result<T, ConfiguredInferenceError>,
    ) -> Result<T, ConfiguredInferenceError> {
        *self.state.result.lock().map_err(lock_error)? = Some(result.clone());
        remove_shared_inference_state(self.registry, self.key.as_str(), &self.state)?;
        self.completed = true;
        self.state.terminal_notify.notify_waiters();
        result
    }
}

impl<T> Drop for SharedInferenceLeaderGuard<'_, T> {
    /// Converts a dropped leader into one controlled failure instead of stranding followers.
    /// 把被丢弃的领导请求转换成受控失败，避免跟随请求永久等待。
    fn drop(&mut self) {
        if self.completed {
            return;
        }
        if let Ok(mut result) = self.state.result.lock()
            && result.is_none()
        {
            *result = Some(Err(ConfiguredInferenceError::new(
                ConfiguredInferenceErrorCategory::Execution,
                "The leading inference request ended before publishing a terminal result.",
            )));
        }
        if let Ok(mut registry) = self.registry.lock()
            && registry
                .get(self.key.as_str())
                .is_some_and(|state| Arc::ptr_eq(state, &self.state))
        {
            registry.remove(self.key.as_str());
        }
        self.state.terminal_notify.notify_waiters();
    }
}

/// Defines the closed scheduling priority derived only from trusted usage scope.
/// 定义仅从可信用途范围派生的封闭调度优先级。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InferenceWorkloadClass {
    /// Host-proven interactive agent work.
    /// 宿主证明的交互式智能体工作。
    Interactive,
    /// Separately authenticated local-service work.
    /// 独立鉴权的本地服务工作。
    External,
    /// Vulcan-owned background indexing or batch work.
    /// Vulcan 自有后台索引或批处理工作。
    Background,
}

impl InferenceWorkloadClass {
    /// Resolve a queue from host-issued scheduling authority after caller authorization.
    /// 在调用方授权后，从宿主签发的调度权限解析队列。
    /// `caller` is frozen registration context; returns no new model-route permissions.
    /// `caller` 是冻结的注册上下文；返回值不授予新的模型路由权限。
    fn from_caller(caller: &InferenceCallerContext) -> Self {
        match caller.scheduling_class {
            vulcan_ai_contract::InferenceSchedulingClass::ByUsageScope => {
                Self::from_usage_scope(caller.usage_scope)
            }
            vulcan_ai_contract::InferenceSchedulingClass::InternalQuery => Self::Interactive,
        }
    }

    /// Derives one scheduler class from a host-injected usage scope.
    /// 从宿主注入的用途范围派生一项调度类别。
    ///
    /// # Parameters
    /// 参数
    /// `usage_scope` is trusted caller context and never request-body metadata.
    /// `usage_scope` 是可信调用方上下文，绝不是请求正文元数据。
    ///
    /// # Returns
    /// 返回值
    /// The exact closed scheduling class.
    /// 返回精确封闭调度类别。
    fn from_usage_scope(usage_scope: InferenceUsageScope) -> Self {
        match usage_scope {
            InferenceUsageScope::InteractiveAgent => Self::Interactive,
            InferenceUsageScope::ExternalService => Self::External,
            InferenceUsageScope::InternalBackground => Self::Background,
        }
    }
}

/// Retains bounded FIFO queues and active counts for configured inference.
/// 保留已配置推理的有界 FIFO 队列与活动计数。
#[derive(Default)]
struct InferenceSchedulerState {
    /// Next monotonically increasing queue ticket.
    /// 下一个单调递增队列票据。
    next_ticket: u64,
    /// Active work across all scheduling classes.
    /// 全部调度类别的活动工作数。
    active_total: u32,
    /// Active externally authenticated service work.
    /// 活动的外部鉴权服务工作数。
    active_external: u32,
    /// Active background work.
    /// 活动的后台工作数。
    active_background: u32,
    /// FIFO interactive queue.
    /// 交互式 FIFO 队列。
    interactive: VecDeque<u64>,
    /// FIFO external-service queue.
    /// 外部服务 FIFO 队列。
    external: VecDeque<u64>,
    /// FIFO background queue.
    /// 后台 FIFO 队列。
    background: VecDeque<u64>,
    /// Interactive admissions while external work has waited.
    /// 外部工作等待期间的交互式受理次数。
    interactive_bypass_count: u32,
    /// Higher-priority admissions while background work has waited.
    /// 后台工作等待期间的较高优先级受理次数。
    background_bypass_count: u32,
}

impl InferenceSchedulerState {
    /// Returns the exact queue for one workload class.
    /// 返回一个工作负载类别的精确队列。
    ///
    /// # Parameters
    /// 参数
    /// `class` selects one closed scheduler lane.
    /// `class` 选择一条封闭调度通道。
    ///
    /// # Returns
    /// 返回值
    /// A shared view of the matching FIFO queue.
    /// 返回匹配 FIFO 队列的共享视图。
    fn queue(&self, class: InferenceWorkloadClass) -> &VecDeque<u64> {
        match class {
            InferenceWorkloadClass::Interactive => &self.interactive,
            InferenceWorkloadClass::External => &self.external,
            InferenceWorkloadClass::Background => &self.background,
        }
    }

    /// Returns mutable access to the exact queue for one workload class.
    /// 返回一个工作负载类别精确队列的可变访问。
    ///
    /// # Parameters
    /// 参数
    /// `class` selects one closed scheduler lane.
    /// `class` 选择一条封闭调度通道。
    ///
    /// # Returns
    /// 返回值
    /// A mutable view of the matching FIFO queue.
    /// 返回匹配 FIFO 队列的可变视图。
    fn queue_mut(&mut self, class: InferenceWorkloadClass) -> &mut VecDeque<u64> {
        match class {
            InferenceWorkloadClass::Interactive => &mut self.interactive,
            InferenceWorkloadClass::External => &mut self.external,
            InferenceWorkloadClass::Background => &mut self.background,
        }
    }

    /// Reports whether one queue-head ticket can enter its reserved capacity.
    /// 报告一个队首票据能否进入其保留容量。
    ///
    /// # Parameters
    /// 参数
    /// `class` and `ticket` identify one exact waiting request.
    /// `class` 与 `ticket` 标识一项精确等待请求。
    ///
    /// # Returns
    /// 返回值
    /// Whether admission preserves priority, fairness, and reserved capacity.
    /// 返回受理是否保持优先级、公平性与保留容量。
    fn can_admit(&self, class: InferenceWorkloadClass, ticket: u64) -> bool {
        if self.active_total >= INFERENCE_SCHEDULER_TOTAL_CAPACITY
            || self.queue(class).front().copied() != Some(ticket)
        {
            return false;
        }
        // Yield priority only when the waiting background head can actually use its own capacity.
        // 仅当等待中的后台队首确实能使用自身容量时让出优先级。
        // A busy background slot must not stall foreground queries after repeated priority admissions.
        // 后台槽位已占用时，不得在多次优先受理后阻塞前台查询。
        let background_must_progress = self.active_background
            < INFERENCE_SCHEDULER_BACKGROUND_CAPACITY
            && !self.background.is_empty()
            && self.background_bypass_count >= INFERENCE_SCHEDULER_MAX_PRIORITY_BYPASS;
        match class {
            InferenceWorkloadClass::Interactive => !background_must_progress,
            InferenceWorkloadClass::External => {
                self.active_external < INFERENCE_SCHEDULER_EXTERNAL_CAPACITY
                    && !background_must_progress
                    && (self.interactive.is_empty()
                        || self.interactive_bypass_count >= INFERENCE_SCHEDULER_MAX_PRIORITY_BYPASS)
            }
            InferenceWorkloadClass::Background => {
                self.active_background < INFERENCE_SCHEDULER_BACKGROUND_CAPACITY
                    && ((self.interactive.is_empty() && self.external.is_empty())
                        || background_must_progress)
            }
        }
    }

    /// Commits one already-authorized queue-head admission.
    /// 提交一项已经获准的队首受理。
    ///
    /// # Parameters
    /// 参数
    /// `class` and `ticket` identify the admitted queue head.
    /// `class` 与 `ticket` 标识已受理队首。
    fn admit(&mut self, class: InferenceWorkloadClass, ticket: u64) {
        let removed = self.queue_mut(class).pop_front();
        debug_assert_eq!(removed, Some(ticket));
        self.active_total = self.active_total.saturating_add(1);
        if !self.external.is_empty() && class == InferenceWorkloadClass::Interactive {
            self.interactive_bypass_count = self.interactive_bypass_count.saturating_add(1);
        } else if class == InferenceWorkloadClass::External || self.external.is_empty() {
            self.interactive_bypass_count = 0;
        }
        if !self.background.is_empty() && class != InferenceWorkloadClass::Background {
            self.background_bypass_count = self.background_bypass_count.saturating_add(1);
        } else if class == InferenceWorkloadClass::Background || self.background.is_empty() {
            self.background_bypass_count = 0;
        }
        match class {
            InferenceWorkloadClass::Interactive => {}
            InferenceWorkloadClass::External => {
                self.active_external = self.active_external.saturating_add(1);
            }
            InferenceWorkloadClass::Background => {
                self.active_background = self.active_background.saturating_add(1);
            }
        }
    }

    /// Releases one exact class admission.
    /// 释放一个精确类别的受理。
    ///
    /// # Parameters
    /// 参数
    /// `class` identifies the counters owned by the completed permit.
    /// `class` 标识已完成许可拥有的计数器。
    fn release(&mut self, class: InferenceWorkloadClass) {
        self.active_total = self.active_total.saturating_sub(1);
        match class {
            InferenceWorkloadClass::Interactive => {}
            InferenceWorkloadClass::External => {
                self.active_external = self.active_external.saturating_sub(1);
            }
            InferenceWorkloadClass::Background => {
                self.active_background = self.active_background.saturating_sub(1);
            }
        }
    }
}

/// Owns process-local priority scheduling for configured inference.
/// 管理已配置推理的进程内优先级调度。
#[derive(Default)]
struct InferenceWorkloadScheduler {
    /// Mutable queue and capacity state.
    /// 可变队列与容量状态。
    state: Mutex<InferenceSchedulerState>,
    /// Wakes queued requests after admission, cancellation, or release.
    /// 在受理、取消或释放后唤醒排队请求。
    changed: Notify,
}

/// Removes one queued ticket when its waiting future is cancelled or expires.
/// 在等待 Future 被取消或过期时移除一个排队票据。
struct InferenceQueueRegistration {
    /// Shared scheduler that owns the ticket.
    /// 拥有票据的共享调度器。
    scheduler: Arc<InferenceWorkloadScheduler>,
    /// Exact workload class.
    /// 精确工作负载类别。
    class: InferenceWorkloadClass,
    /// Monotonic queue ticket.
    /// 单调队列票据。
    ticket: u64,
    /// Whether admission already removed the ticket.
    /// 受理是否已经移除票据。
    admitted: bool,
}

impl Drop for InferenceQueueRegistration {
    /// Removes an abandoned queue ticket and wakes the next eligible request.
    /// 移除被放弃的队列票据并唤醒下一项符合条件的请求。
    fn drop(&mut self) {
        if self.admitted {
            return;
        }
        if let Ok(mut state) = self.scheduler.state.lock()
            && let Some(position) = state
                .queue(self.class)
                .iter()
                .position(|ticket| *ticket == self.ticket)
        {
            state.queue_mut(self.class).remove(position);
        }
        self.scheduler.changed.notify_waiters();
    }
}

/// Owns one admitted workload slot until execution terminates.
/// 在执行终止前拥有一个已受理工作负载槽位。
struct InferenceSchedulerPermit {
    /// Shared scheduler that owns capacity.
    /// 拥有容量的共享调度器。
    scheduler: Arc<InferenceWorkloadScheduler>,
    /// Exact admitted workload class.
    /// 精确已受理工作负载类别。
    class: InferenceWorkloadClass,
}

impl Drop for InferenceSchedulerPermit {
    /// Releases one workload slot and wakes queued requests.
    /// 释放一个工作负载槽位并唤醒排队请求。
    fn drop(&mut self) {
        if let Ok(mut state) = self.scheduler.state.lock() {
            state.release(self.class);
        }
        self.scheduler.changed.notify_waiters();
    }
}

impl InferenceWorkloadScheduler {
    /// Acquires one priority-scheduled workload slot before provider dispatch.
    /// 在供应商分派前获取一个按优先级调度的工作负载槽位。
    ///
    /// # Parameters
    /// 参数
    /// `self` retains queue ownership, `class` selects the host-authorized lane, and
    /// `deadline_unix_ms` bounds queue waiting.
    /// `self` 保留队列所有权，`class` 选择宿主已授权通道，`deadline_unix_ms`
    /// 限制排队等待时间。
    ///
    /// # Returns
    /// 返回值
    /// An RAII capacity permit or a stable deadline/lock failure.
    /// 返回 RAII 容量许可，或稳定截止时间/锁失败。
    async fn acquire(
        self: &Arc<Self>,
        class: InferenceWorkloadClass,
        deadline_unix_ms: Option<u64>,
    ) -> Result<InferenceSchedulerPermit, ConfiguredInferenceError> {
        let ticket = {
            let mut state = self.state.lock().map_err(lock_error)?;
            let ticket = state.next_ticket;
            state.next_ticket = state.next_ticket.wrapping_add(1);
            state.queue_mut(class).push_back(ticket);
            ticket
        };
        let mut registration = InferenceQueueRegistration {
            scheduler: Arc::clone(self),
            class,
            ticket,
            admitted: false,
        };
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            // Register before checking scheduler capacity because releases use notify_waiters.
            // 在检查调度容量前完成注册，因为容量释放使用 notify_waiters。
            changed.as_mut().enable();
            {
                let mut state = self.state.lock().map_err(lock_error)?;
                if state.can_admit(class, ticket) {
                    state.admit(class, ticket);
                    registration.admitted = true;
                    self.changed.notify_waiters();
                    return Ok(InferenceSchedulerPermit {
                        scheduler: Arc::clone(self),
                        class,
                    });
                }
            }
            match scheduler_wait_duration(deadline_unix_ms)? {
                None => changed.as_mut().await,
                Some(duration) => {
                    tokio::time::timeout(duration, changed.as_mut())
                        .await
                        .map_err(|_| {
                        ConfiguredInferenceError::new(
                            ConfiguredInferenceErrorCategory::Limit,
                            "Configured inference expired while waiting for scheduled capacity.",
                        )
                    })?;
                }
            }
        }
    }
}

/// Owns one caller reservation from queue admission through execution termination.
/// 从排队准入直到执行终止持有一项调用方预留。
struct CallerConcurrencyReservation {
    /// Stable caller identifier.
    /// 稳定调用方标识。
    caller_id: String,
    /// Shared active-execution counters.
    /// 共享活动执行计数器。
    counters: Arc<Mutex<BTreeMap<String, u32>>>,
}

/// Retains both caller and scheduler ownership after successful admission.
/// 成功准入后同时保留调用方和调度器所有权。
struct CallerExecutionPermit {
    /// The same reservation created before queue waiting, never a second counter increment.
    /// 排队等待前创建的同一预留，绝不再次递增计数。
    _caller_reservation: CallerConcurrencyReservation,
    /// Priority-scheduled shared workload slot.
    /// 按优先级调度的共享工作负载槽位。
    _scheduler_permit: InferenceSchedulerPermit,
}

/// Retains one caller's fixed rolling-minute accounting window.
/// 保留一个调用方的固定滚动分钟计量窗口。
struct CallerRateWindow {
    /// Window start timestamp in Unix milliseconds.
    /// 窗口起始 Unix 毫秒时间戳。
    started_at_unix_ms: u64,
    /// Accepted requests in the current window.
    /// 当前窗口内已受理请求数。
    request_count: u32,
    /// Accepted input units in the current window.
    /// 当前窗口内已受理输入单位数。
    input_units: u64,
}

impl CallerRateWindow {
    /// Creates one empty rate window at the exact host timestamp.
    /// 在精确宿主时间戳创建一个空速率窗口。
    ///
    /// # Parameters
    /// 参数
    /// `started_at_unix_ms` is the trusted current Unix timestamp.
    /// `started_at_unix_ms` 是可信当前 Unix 时间戳。
    ///
    /// # Returns
    /// 返回值
    /// An empty request and input-unit accounting window.
    /// 返回请求数与输入单位均为空的计量窗口。
    fn new(started_at_unix_ms: u64) -> Self {
        Self {
            started_at_unix_ms,
            request_count: 0,
            input_units: 0,
        }
    }
}

/// Contains only non-secret lifecycle facts accepted by the usage-statistics hooks.
/// 仅包含用量统计 Hook 可接收的非秘密生命周期事实。
#[derive(Clone)]
struct InternalInferenceTelemetryFacts {
    /// Stable request identity.
    /// 稳定请求身份。
    request_id: String,
    /// Synthetic execution/session identity for non-conversation inference.
    /// 非会话推理使用的合成执行/会话身份。
    execution_id: String,
    /// Caller identity issued by the host.
    /// 宿主签发的调用方身份。
    caller_id: String,
    /// Consumer profile selected through authorization.
    /// 通过授权选择的消费者配置档。
    consumer_profile_id: String,
    /// Trusted execution usage scope selected by the host.
    /// 由宿主选择的可信执行用途范围。
    usage_scope: InferenceUsageScope,
    /// Closed operation-specific statistics owner.
    /// 封闭的操作专属统计归属。
    owner_kind: &'static str,
    /// Provider identity and display label.
    /// 供应商身份与显示标签。
    provider_id: String,
    provider_name: String,
    /// Exact product, connection, protocol, and provider-internal route identities.
    /// 精确产品、连接、协议及供应商内部路由身份。
    service_entry_id: String,
    connection_id: String,
    protocol_profile_id: Option<String>,
    action_binding_id: Option<String>,
    secondary_route_id: Option<String>,
    secondary_provider_name: Option<String>,
    secondary_endpoint_name: Option<String>,
    /// Model identity and display label.
    /// 模型身份与显示标签。
    model_id: String,
    model_name: String,
    /// Exact normalized operation and frozen evidence revisions.
    /// 精确归一化操作及冻结证据修订。
    operation: String,
    capability_revision: String,
    catalog_revision: String,
    route_revision: String,
    /// Exact terminal timing.
    /// 精确终态计时。
    started_at_unix_ms: u64,
    first_response_at_unix_ms: Option<u64>,
    /// First nonempty text or reasoning delta, distinct from transport and item lifecycle events.
    /// 首个非空文本或推理增量，与传输和响应项生命周期事件分开记录。
    first_token_at_unix_ms: Option<u64>,
    duration_ms: u64,
    /// Number of provider retries completed after the initial attempt.
    /// 初始尝试后已完成的供应商重试次数。
    retry_count: u32,
    /// Exact upstream transport observed for this physical attempt.
    /// 当前真实尝试观察到的精确上游传输。
    transport: Option<vulcan_ai_contract::AiTransportKind>,
    /// Exact transport dispatch and response-header timestamps.
    /// 精确传输分派与响应头时间戳。
    dispatched_at_ms: Option<i64>,
    headers_at_ms: Option<i64>,
    /// Exact HTTP and WebSocket terminal transport facts.
    /// 精确 HTTP 与 WebSocket 终态传输事实。
    transport_http_status: Option<u16>,
    websocket_close_code: Option<u16>,
    /// Allowlisted upstream request and rate-limit facts.
    /// 允许列表内的上游请求与限流事实。
    provider_request_id: Option<String>,
    rate_limit_headers: BTreeMap<String, String>,
    /// Stable terminal status.
    /// 稳定终态状态。
    status: &'static str,
    /// Normalized provider usage without content.
    /// 不含内容的归一化供应商用量。
    usage: RequestUsage,
    /// Optional content-free operation cardinality metrics.
    /// 可选的不含内容的操作基数指标。
    input_item_count: Option<u64>,
    output_item_count: Option<u64>,
    vector_dimensions: Option<u64>,
    /// Optional stable failure category.
    /// 可选稳定失败类别。
    error_category: Option<String>,
    /// Optional exact normalized provider failure.
    /// 可选精确归一化供应商失败。
    provider_error: Option<Box<AiExecutionError>>,
}

impl InternalInferenceTelemetryFacts {
    /// Adds exact content-free operation cardinality to one lifecycle record.
    /// 为一项生命周期记录增加精确且不含内容的操作基数。
    ///
    /// # Parameters
    /// 参数
    /// Counts describe accepted inputs, returned outputs, and optional vector dimensions.
    /// 这些计数描述已接收输入、已返回输出及可选向量维度。
    ///
    /// # Returns
    /// 返回值
    /// The same lifecycle record with immutable operation metrics.
    /// 返回带有不可变操作指标的同一生命周期记录。
    fn with_metrics(
        mut self,
        input_item_count: Option<u64>,
        output_item_count: Option<u64>,
        vector_dimensions: Option<u64>,
    ) -> Self {
        self.input_item_count = input_item_count;
        self.output_item_count = output_item_count;
        self.vector_dimensions = vector_dimensions;
        self
    }

    /// Rebinds telemetry to the exact frozen execution route used by a supplemental action.
    /// 将遥测重新绑定到补充动作实际使用的精确冻结执行路由。
    ///
    /// # Parameters
    /// 参数
    /// `route` is the content-free route snapshot returned by the same identity resolution as dispatch.
    /// `route` 是由与分派相同的身份解析返回且不含内容的路由快照。
    ///
    /// # Returns
    /// 返回值
    /// The same lifecycle record with every route identifier replaced atomically.
    /// 返回以原子方式替换全部路由标识后的同一生命周期记录。
    fn with_execution_route(mut self, route: &ExecutionRouteSnapshot) -> Self {
        self.usage_scope = route.usage_scope;
        self.provider_id = route.provider_id.clone();
        self.service_entry_id = route.service_entry_id.clone();
        self.connection_id = route.connection_id.clone();
        self.protocol_profile_id = route.protocol_binding_id.clone();
        self.action_binding_id = route.action_binding_id.clone();
        self.secondary_route_id = route.secondary_route_id.clone();
        self.secondary_provider_name = route.secondary_provider_name.clone();
        self.secondary_endpoint_name = route.secondary_endpoint_name.clone();
        self.model_id = route.model_id.clone();
        self.capability_revision = route.capability_revision.clone();
        self.catalog_revision = route.catalog_revision.clone();
        self.route_revision = route.route_revision.clone();
        self
    }

    /// Adds the first semantic provider-response timestamp.
    /// 增加首个供应商语义响应时间戳。
    ///
    /// # Parameters
    /// 参数
    /// `first_response_at_unix_ms` is captured exactly once by the execution driver.
    /// `first_response_at_unix_ms` 由执行驱动精确捕获一次。
    ///
    /// # Returns
    /// 返回值
    /// The same lifecycle record with first-response timing.
    /// 返回带有首响应计时的同一生命周期记录。
    fn with_first_response(mut self, first_response_at_unix_ms: Option<u64>) -> Self {
        self.first_response_at_unix_ms = first_response_at_unix_ms;
        self
    }

    /// Adds the exact number of provider retries completed by one logical request.
    /// 增加一个逻辑请求已完成的精确供应商重试次数。
    ///
    /// # Parameters
    /// 参数
    /// `retry_count` excludes the initial provider attempt.
    /// `retry_count` 不包含初始供应商尝试。
    ///
    /// # Returns
    /// 返回值
    /// The same lifecycle record with attempt accounting.
    /// 返回带有尝试计量的同一生命周期记录。
    fn with_retry_count(mut self, retry_count: u32) -> Self {
        self.retry_count = retry_count;
        self.request_id = format!("{}:attempt:{retry_count}", self.execution_id);
        self
    }

    /// Copies exact transport facts from the active physical-attempt record.
    /// 从活动真实尝试记录复制精确传输事实。
    fn with_transport_from(mut self, source: &Self) -> Self {
        self.first_token_at_unix_ms = source.first_token_at_unix_ms;
        self.transport = source.transport;
        self.dispatched_at_ms = source.dispatched_at_ms;
        self.headers_at_ms = source.headers_at_ms;
        self.transport_http_status = source.transport_http_status;
        self.websocket_close_code = source.websocket_close_code;
        self.provider_request_id = source.provider_request_id.clone();
        self.rate_limit_headers = source.rate_limit_headers.clone();
        self
    }

    /// Records exact transport facts emitted by the physical provider dispatcher.
    /// 记录真实供应商分派器发出的精确传输事实。
    fn observe_transport_event(&mut self, event: &AiStreamEvent) {
        // Empty deltas and response/tool start events cannot establish token generation.
        // 空增量及响应或工具开始事件不能证明 Token 已开始生成。
        if self.first_token_at_unix_ms.is_none()
            && matches!(event, AiStreamEvent::ResponseTextDelta { delta, .. } | AiStreamEvent::ReasoningDelta { delta, .. } if !delta.is_empty())
        {
            self.first_token_at_unix_ms = Some(current_unix_millis());
        }
        let AiStreamEvent::TransportObserved {
            transport,
            dispatched_at_ms,
            headers_at_ms,
            http_status,
            provider_request_id,
            rate_limit_headers,
        } = event
        else {
            return;
        };
        self.transport = Some(*transport);
        self.dispatched_at_ms = Some(*dispatched_at_ms);
        self.headers_at_ms = *headers_at_ms;
        self.transport_http_status = *http_status;
        self.provider_request_id = provider_request_id.clone();
        self.rate_limit_headers = rate_limit_headers.clone();
    }

    /// Builds successful facts from one resolved provider selection.
    /// 从一项已解析供应商选择构建成功事实。
    ///
    /// # Parameters
    /// 参数
    /// All inputs are immutable authorization, route, timing, and usage facts.
    /// 所有输入均为不可变授权、路由、计时与用量事实。
    /// `request_id` is caller-defined; `execution_id` is the canonical UUID that owns usage accounting.
    /// `request_id` 由调用方定义；`execution_id` 是持有用量统计的规范 UUID。
    ///
    /// # Returns
    /// 返回值
    /// One privacy-safe terminal telemetry record.
    /// 返回一项隐私安全终态遥测记录。
    // Frozen route identity, caller identity and execution accounting are separate authorities.
    // 冻结路由身份、调用方身份及执行计量属于不同权威。
    #[expect(
        clippy::too_many_arguments,
        reason = "Frozen route identity, caller identity and execution accounting are separate authorities."
    )]
    fn succeeded(
        caller: &InferenceCallerContext,
        consumer_profile_id: &str,
        request_id: &str,
        execution_id: &str,
        owner_kind: &'static str,
        selection: &ResolvedPublicAiSelection,
        started_at_unix_ms: u64,
        duration_ms: u64,
        usage: &RequestUsage,
    ) -> Self {
        Self::from_selection(
            caller,
            consumer_profile_id,
            request_id,
            execution_id,
            owner_kind,
            selection,
            started_at_unix_ms,
            duration_ms,
            "succeeded",
            usage,
            None,
        )
    }

    /// Builds failed facts from one resolved provider selection.
    /// 从一项已解析供应商选择构建失败事实。
    ///
    /// # Parameters
    /// 参数
    /// All inputs are immutable authorization, route, timing, and normalized failure facts.
    /// 所有输入均为不可变授权、路由、计时与归一化失败事实。
    /// `request_id` is caller-defined; `execution_id` is the canonical UUID that owns usage accounting.
    /// `request_id` 由调用方定义；`execution_id` 是持有用量统计的规范 UUID。
    ///
    /// # Returns
    /// 返回值
    /// One privacy-safe failed terminal telemetry record.
    /// 返回一项隐私安全失败终态遥测记录。
    // Frozen route identity, caller identity and execution accounting are separate authorities.
    // 冻结路由身份、调用方身份及执行计量属于不同权威。
    #[expect(
        clippy::too_many_arguments,
        reason = "Frozen route identity, caller identity and execution accounting are separate authorities."
    )]
    fn failed(
        caller: &InferenceCallerContext,
        consumer_profile_id: &str,
        request_id: &str,
        execution_id: &str,
        owner_kind: &'static str,
        selection: &ResolvedPublicAiSelection,
        started_at_unix_ms: u64,
        error: &ConfiguredInferenceError,
    ) -> Self {
        Self::from_selection(
            caller,
            consumer_profile_id,
            request_id,
            execution_id,
            owner_kind,
            selection,
            started_at_unix_ms,
            current_unix_millis().saturating_sub(started_at_unix_ms),
            if error.category == ConfiguredInferenceErrorCategory::Cancelled {
                "cancelled"
            } else {
                "failed"
            },
            &RequestUsage::default(),
            Some(error),
        )
    }

    /// Projects one selection into immutable terminal facts.
    /// 将一项选择投影为不可变终态事实。
    ///
    /// # Parameters
    /// 参数
    /// Inputs carry host authorization, exact route identity, terminal timing, and optional error.
    /// 输入承载宿主授权、精确路由身份、终态计时与可选错误。
    /// `request_id` and `execution_id` remain independent identities throughout the lifecycle.
    /// `request_id` 与 `execution_id` 在整个生命周期中保持为相互独立的身份。
    ///
    /// # Returns
    /// 返回值
    /// One content-free telemetry record.
    /// 返回一项不含内容的遥测记录。
    #[allow(clippy::too_many_arguments)]
    fn from_selection(
        caller: &InferenceCallerContext,
        consumer_profile_id: &str,
        request_id: &str,
        execution_id: &str,
        owner_kind: &'static str,
        selection: &ResolvedPublicAiSelection,
        started_at_unix_ms: u64,
        duration_ms: u64,
        status: &'static str,
        usage: &RequestUsage,
        error: Option<&ConfiguredInferenceError>,
    ) -> Self {
        let (secondary_route_id, secondary_provider_name, secondary_endpoint_name) =
            secondary_route_snapshot(&selection.route.secondary_route);
        Self {
            request_id: request_id.to_string(),
            execution_id: execution_id.to_string(),
            caller_id: caller.caller_id.clone(),
            consumer_profile_id: consumer_profile_id.to_string(),
            usage_scope: caller.usage_scope,
            owner_kind,
            provider_id: selection.route.provider.id.clone(),
            provider_name: selection.route.provider.display_name.clone(),
            service_entry_id: selection.route.entry.id.clone(),
            connection_id: selection.route.connection.id.clone(),
            protocol_profile_id: Some(selection.route.protocol_route.protocol_profile_id.clone()),
            action_binding_id: None,
            secondary_route_id,
            secondary_provider_name,
            secondary_endpoint_name,
            model_id: selection.model.id.clone(),
            model_name: selection.model.display_name.clone(),
            operation: match owner_kind {
                "internal_embedding" => AiOperation::EmbeddingCreate.as_str().to_string(),
                "internal_rerank" => AiOperation::RerankDocuments.as_str().to_string(),
                "internal_llm" => AiOperation::ConversationRespond.as_str().to_string(),
                _ => owner_kind.to_string(),
            },
            capability_revision: selection.model.capabilities.evidence_revision.clone(),
            catalog_revision: selection.catalog_revision.clone(),
            route_revision: selection.route_revision.clone(),
            started_at_unix_ms,
            first_response_at_unix_ms: None,
            first_token_at_unix_ms: None,
            duration_ms,
            retry_count: 0,
            transport: None,
            dispatched_at_ms: None,
            headers_at_ms: None,
            transport_http_status: None,
            websocket_close_code: None,
            provider_request_id: None,
            rate_limit_headers: BTreeMap::new(),
            status,
            usage: usage.clone(),
            input_item_count: None,
            output_item_count: None,
            vector_dimensions: None,
            error_category: error.map(|error| error.category_label().to_string()),
            provider_error: error.and_then(|error| error.provider_error.clone()),
        }
    }

    /// Projects one frozen LLM route into immutable terminal facts.
    /// 将一条冻结 LLM 路由投影为不可变终态事实。
    ///
    /// # Parameters
    /// 参数
    /// Inputs carry caller/profile identity, exact route, terminal timing, usage, and optional error.
    /// 输入承载调用方/配置档身份、精确路由、终态计时、用量与可选错误。
    /// `execution_id` must be the UUID allocated by the owning LLM execution.
    /// `execution_id` 必须是所属 LLM 执行分配的 UUID。
    ///
    /// # Returns
    /// 返回值
    /// One content-free telemetry record.
    /// 返回一项不含内容的遥测记录。
    #[allow(clippy::too_many_arguments)]
    fn from_route(
        caller_id: &str,
        consumer_profile_id: &str,
        request_id: &str,
        execution_id: &str,
        owner_kind: &'static str,
        route: &ExecutionRouteSnapshot,
        started_at_unix_ms: u64,
        duration_ms: u64,
        status: &'static str,
        usage: &RequestUsage,
        error: Option<&ConfiguredInferenceError>,
    ) -> Self {
        Self {
            request_id: request_id.to_string(),
            execution_id: execution_id.to_string(),
            caller_id: caller_id.to_string(),
            consumer_profile_id: consumer_profile_id.to_string(),
            usage_scope: route.usage_scope,
            owner_kind,
            provider_id: route.provider_id.clone(),
            provider_name: route.provider_id.clone(),
            service_entry_id: route.service_entry_id.clone(),
            connection_id: route.connection_id.clone(),
            protocol_profile_id: route.protocol_binding_id.clone(),
            action_binding_id: route.action_binding_id.clone(),
            secondary_route_id: route.secondary_route_id.clone(),
            secondary_provider_name: route.secondary_provider_name.clone(),
            secondary_endpoint_name: route.secondary_endpoint_name.clone(),
            model_id: route.model_id.clone(),
            model_name: route.model_id.clone(),
            operation: AiOperation::ConversationRespond.as_str().to_string(),
            capability_revision: route.capability_revision.clone(),
            catalog_revision: route.catalog_revision.clone(),
            route_revision: route.route_revision.clone(),
            started_at_unix_ms,
            first_response_at_unix_ms: None,
            first_token_at_unix_ms: None,
            duration_ms,
            retry_count: 0,
            transport: None,
            dispatched_at_ms: None,
            headers_at_ms: None,
            transport_http_status: None,
            websocket_close_code: None,
            provider_request_id: None,
            rate_limit_headers: BTreeMap::new(),
            status,
            usage: usage.clone(),
            input_item_count: None,
            output_item_count: None,
            vector_dimensions: None,
            error_category: error.map(|error| error.category_label().to_string()),
            provider_error: error.and_then(|error| error.provider_error.clone()),
        }
    }
}

impl Drop for CallerConcurrencyReservation {
    /// Releases only this reservation on queue cancellation, rejection, or execution completion.
    /// 在排队取消、拒绝或执行完成时仅释放此项预留。
    fn drop(&mut self) {
        release_caller_counter(self.counters.as_ref(), self.caller_id.as_str());
    }
}

/// Provides one host-authorized inference boundary over configured provider routes.
/// 在已配置供应商路由之上提供一个宿主授权推理边界。
pub struct ConfiguredInferenceService {
    /// Shared provider runtime and process-local catalogs.
    /// 共享供应商运行时与进程内目录。
    runtime: Arc<PublicAiRuntime>,
    /// Shared priority scheduler with reserved workload-class capacity.
    /// 具有工作负载类别保留容量的共享优先级调度器。
    workload_scheduler: Arc<InferenceWorkloadScheduler>,
    /// Caller-scoped active execution counts.
    /// 调用方作用域活动执行计数。
    caller_execution_counts: Arc<Mutex<BTreeMap<String, u32>>>,
    /// Caller-scoped rolling request and input-unit accounting.
    /// 调用方作用域的滚动请求与输入单位计量。
    caller_rate_windows: Mutex<BTreeMap<String, CallerRateWindow>>,
    /// Weak per-request-key locks that serialize idempotency reservation before provider dispatch.
    /// 在供应商分派前串行化幂等预留的按请求键弱引用锁。
    llm_start_locks: Mutex<BTreeMap<String, Weak<AsyncMutex<()>>>>,
    /// In-flight embedding requests available for exact concurrent coalescing.
    /// 可供精确并发合并的运行中向量请求。
    in_flight_embeddings: Mutex<BTreeMap<String, Arc<SharedInferenceResult<EmbeddingResponse>>>>,
    /// In-flight rerank requests available for exact concurrent coalescing.
    /// 可供精确并发合并的运行中重排请求。
    in_flight_reranks: Mutex<BTreeMap<String, Arc<SharedInferenceResult<RerankResponse>>>>,
    /// Optional host plugin runtime used for privacy-safe inference accounting.
    /// 用于隐私安全推理计量的可选宿主插件运行时。
    telemetry_runtime: Option<VulcanPluginRuntimeManager>,
    /// Optional host storage runtime used by native provider accounting.
    /// 原生供应商计量使用的可选宿主存储运行时。
    usage_storage: Option<HostRuntimeHandle>,
    /// Live host retry policy shared with the ordinary provider settings page.
    /// 与普通供应商设置页共享的实时宿主重试策略。
    provider_retry_policy: std::sync::RwLock<ProviderRetryPolicy>,
    /// Live opt-in switch for complete secret-safe configured-inference request traces.
    /// 完整且不含秘密的已配置推理请求追踪实时显式开关。
    provider_request_trace_enabled: AtomicBool,
    /// Retained LLM execution states keyed by execution identifier.
    /// 按执行标识索引的保留 LLM 执行状态。
    llm_executions: Mutex<BTreeMap<String, Arc<ConfiguredLlmExecutionState>>>,
    /// In-flight request keys mapped to their shared execution.
    /// 运行中请求键到共享执行的映射。
    in_flight_llm_requests: Mutex<BTreeMap<String, String>>,
}

impl ConfiguredInferenceService {
    /// Builds one configured inference service around the authoritative provider runtime.
    /// 围绕权威供应商运行时构建一项已配置推理服务。
    ///
    /// # Parameters
    /// 参数
    /// `runtime` owns current configuration, catalogs, and secure credential access.
    /// `runtime` 拥有当前配置、目录与安全凭据访问。
    ///
    /// # Returns
    /// 返回值
    /// A service with empty process-local execution state.
    /// 返回进程内执行状态为空的服务。
    pub fn new(runtime: Arc<PublicAiRuntime>) -> Self {
        Self {
            runtime,
            workload_scheduler: Arc::new(InferenceWorkloadScheduler::default()),
            caller_execution_counts: Arc::new(Mutex::new(BTreeMap::new())),
            caller_rate_windows: Mutex::new(BTreeMap::new()),
            llm_start_locks: Mutex::new(BTreeMap::new()),
            in_flight_embeddings: Mutex::new(BTreeMap::new()),
            in_flight_reranks: Mutex::new(BTreeMap::new()),
            telemetry_runtime: None,
            usage_storage: None,
            provider_retry_policy: std::sync::RwLock::new(ProviderRetryPolicy::from_config(
                &PersistedProviderRetryConfiguration::default(),
            )),
            provider_request_trace_enabled: AtomicBool::new(false),
            llm_executions: Mutex::new(BTreeMap::new()),
            in_flight_llm_requests: Mutex::new(BTreeMap::new()),
        }
    }

    /// Enables privacy-safe plugin telemetry for configured inference.
    /// 为已配置推理启用隐私安全的插件遥测。
    ///
    /// # Parameters
    /// 参数
    /// `runtime` is the already initialized host plugin runtime manager.
    /// `runtime` 是已经初始化的宿主插件运行时管理器。
    ///
    /// # Returns
    /// 返回值
    /// The same service with non-secret terminal-event dispatch enabled.
    /// 返回已启用非秘密终态事件分派的同一服务。
    pub fn with_plugin_telemetry(mut self, runtime: VulcanPluginRuntimeManager) -> Self {
        self.telemetry_runtime = Some(runtime);
        self
    }

    /// Enables native provider accounting through the existing application storage runtime.
    /// 通过现有应用存储运行时启用原生供应商计量。
    pub fn with_native_usage_storage(mut self, storage: HostRuntimeHandle) -> Self {
        self.usage_storage = Some(storage);
        self
    }

    /// Applies the startup retry configuration before the service is shared.
    /// 在服务被共享前应用启动期重试配置。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` is the same persisted host policy used by ordinary provider requests.
    /// `configuration` 与普通供应商请求使用同一份持久化宿主策略。
    ///
    /// # Returns
    /// 返回值
    /// The configured service ready to be wrapped in its shared `Arc`.
    /// 返回可被共享 `Arc` 包装的已配置服务。
    pub fn with_retry_configuration(
        mut self,
        configuration: &PersistedProviderRetryConfiguration,
    ) -> Self {
        self.provider_retry_policy =
            std::sync::RwLock::new(ProviderRetryPolicy::from_config(configuration));
        self
    }

    /// Applies the startup configured-inference request-trace switch before the service is shared.
    /// 在服务共享前应用启动期已配置推理请求追踪开关。
    ///
    /// # Parameters
    /// 参数
    /// `enabled` is the exact persisted Provider diagnostic setting.
    /// `enabled` 是持久化 Provider 诊断设置中的精确值。
    ///
    /// # Returns
    /// 返回值
    /// The same service ready to be wrapped in its shared `Arc`.
    /// 返回可被共享 `Arc` 包装的同一服务。
    pub fn with_provider_request_trace_enabled(self, enabled: bool) -> Self {
        self.provider_request_trace_enabled
            .store(enabled, Ordering::Release);
        self
    }

    /// Atomically replaces the live retry policy after a settings save.
    /// 在设置保存后原子替换实时重试策略。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` is the newly committed persisted host policy.
    /// `configuration` 是新提交的持久化宿主策略。
    ///
    /// # Returns
    /// 返回值
    /// This method returns no value after replacing the complete policy.
    /// 完整替换策略后本方法不返回值。
    pub fn update_retry_configuration(&self, configuration: &PersistedProviderRetryConfiguration) {
        let mut policy = self.provider_retry_policy.write().unwrap_or_else(|poisoned| {
            tracing::warn!(
                "[webui-service] recovering poisoned configured-inference retry policy during full replacement"
            );
            poisoned.into_inner()
        });
        *policy = ProviderRetryPolicy::from_config(configuration);
    }

    /// Atomically replaces the live configured-inference request-trace decision after settings save.
    /// 在设置保存后原子替换实时已配置推理请求追踪决定。
    ///
    /// # Parameters
    /// 参数
    /// `enabled` is the newly committed bounded Provider wire-request trace switch.
    /// `enabled` 是新提交的有界 Provider 线上请求追踪开关。
    ///
    /// # Returns
    /// 返回值
    /// This method returns no value after the atomic publication.
    /// 原子发布完成后本方法不返回值。
    pub fn update_provider_request_trace_enabled(&self, enabled: bool) {
        self.provider_request_trace_enabled
            .store(enabled, Ordering::Release);
    }

    /// Executes one exact settings-owned model validation with native attempt accounting.
    /// 执行一次由设置页持有的精确模型验证，并写入原生真实尝试计量。
    ///
    /// # Parameters
    /// 参数
    /// `selection` freezes the user-selected provider route, `request` is the minimal validation
    /// request, and `execution_id` is the stable logical request identity.
    /// `selection` 冻结用户选择的供应商路由，`request` 是最小验证请求，`execution_id`
    /// 是稳定逻辑请求标识。
    ///
    /// # Returns
    /// 返回值
    /// Returns visible terminal response text, or a normalized failure after persisting the exact
    /// physical attempt and any partial usage.
    /// 返回可见终态响应文本；失败时先持久化精确真实尝试及可能存在的部分用量，再返回规范错误。
    pub(crate) async fn execute_model_validation(
        &self,
        selection: &ResolvedPublicAiSelection,
        request: AiExecutionRequest,
        execution_id: String,
    ) -> Result<String, ConfiguredInferenceError> {
        let caller = provider_model_validation_caller();
        let started_at_unix_ms = current_unix_millis();
        let input_item_count = u64::try_from(request.messages.len()).unwrap_or(u64::MAX);
        let mut active_facts = InternalInferenceTelemetryFacts::from_selection(
            &caller,
            PROVIDER_MODEL_VALIDATION_CONSUMER_ID,
            execution_id.as_str(),
            execution_id.as_str(),
            "internal_llm",
            selection,
            started_at_unix_ms,
            0,
            "succeeded",
            &RequestUsage::default(),
            None,
        )
        .with_metrics(Some(input_item_count), None, None);
        self.emit_internal_inference_started(&active_facts);

        let stream = self
            .runtime
            .execute_internal_llm(selection, request, execution_id.clone())
            .await;
        let mut stream = match stream {
            Ok(stream) => stream,
            Err(failure) => {
                let error = ConfiguredInferenceError::from_provider_failure(failure);
                self.emit_model_validation_terminal(
                    &caller,
                    selection,
                    execution_id.as_str(),
                    started_at_unix_ms,
                    input_item_count,
                    None,
                    &RequestUsage::default(),
                    &active_facts,
                    Err(&error),
                );
                return Err(error);
            }
        };
        let mut output_text = String::new();
        let mut reasoning_text = String::new();
        let mut structured_output = None;
        let mut output_items = Vec::new();
        let mut usage = RequestUsage::default();
        let mut first_response_at_unix_ms = None;
        while let Some(next_event) = stream.next().await {
            let event = match next_event {
                Ok(event) => event,
                Err(failure) => {
                    let error = ConfiguredInferenceError::from_provider_failure(failure);
                    self.emit_model_validation_terminal(
                        &caller,
                        selection,
                        execution_id.as_str(),
                        started_at_unix_ms,
                        input_item_count,
                        first_response_at_unix_ms,
                        &usage,
                        &active_facts,
                        Err(&error),
                    );
                    return Err(error);
                }
            };
            active_facts.observe_transport_event(&event);
            if first_response_at_unix_ms.is_none() && is_semantic_inference_response(&event) {
                first_response_at_unix_ms = Some(current_unix_millis());
            }
            match aggregate_restricted_llm_event(
                event,
                &mut output_text,
                &mut reasoning_text,
                &mut structured_output,
                &mut output_items,
                &mut usage,
            ) {
                Ok(Some(_)) => {
                    let output_text = output_text.trim().to_string();
                    if output_text.is_empty() {
                        let error = ConfiguredInferenceError::new(
                            ConfiguredInferenceErrorCategory::Execution,
                            "Custom model validation completed without visible text output.",
                        );
                        self.emit_model_validation_terminal(
                            &caller,
                            selection,
                            execution_id.as_str(),
                            started_at_unix_ms,
                            input_item_count,
                            first_response_at_unix_ms,
                            &usage,
                            &active_facts,
                            Err(&error),
                        );
                        return Err(error);
                    }
                    self.emit_model_validation_terminal(
                        &caller,
                        selection,
                        execution_id.as_str(),
                        started_at_unix_ms,
                        input_item_count,
                        first_response_at_unix_ms,
                        &usage,
                        &active_facts,
                        Ok(1),
                    );
                    return Ok(output_text);
                }
                Ok(None) => {}
                Err(error) => {
                    self.emit_model_validation_terminal(
                        &caller,
                        selection,
                        execution_id.as_str(),
                        started_at_unix_ms,
                        input_item_count,
                        first_response_at_unix_ms,
                        &usage,
                        &active_facts,
                        Err(&error),
                    );
                    return Err(error);
                }
            }
        }
        let error = ConfiguredInferenceError::new(
            ConfiguredInferenceErrorCategory::Execution,
            "Custom model validation stream ended before a terminal completion event.",
        );
        self.emit_model_validation_terminal(
            &caller,
            selection,
            execution_id.as_str(),
            started_at_unix_ms,
            input_item_count,
            first_response_at_unix_ms,
            &usage,
            &active_facts,
            Err(&error),
        );
        Err(error)
    }

    /// Persists one settings-owned model-validation terminal attempt.
    /// 持久化一次由设置页持有的模型验证终态尝试。
    #[allow(clippy::too_many_arguments)]
    fn emit_model_validation_terminal(
        &self,
        caller: &InferenceCallerContext,
        selection: &ResolvedPublicAiSelection,
        execution_id: &str,
        started_at_unix_ms: u64,
        input_item_count: u64,
        first_response_at_unix_ms: Option<u64>,
        usage: &RequestUsage,
        active_facts: &InternalInferenceTelemetryFacts,
        result: Result<u64, &ConfiguredInferenceError>,
    ) {
        let duration_ms = current_unix_millis().saturating_sub(started_at_unix_ms);
        let (status, output_item_count, error) = match result {
            Ok(output_item_count) => ("succeeded", Some(output_item_count), None),
            Err(error) => (
                if error.category == ConfiguredInferenceErrorCategory::Cancelled {
                    "cancelled"
                } else {
                    "failed"
                },
                None,
                Some(error),
            ),
        };
        let facts = InternalInferenceTelemetryFacts::from_selection(
            caller,
            PROVIDER_MODEL_VALIDATION_CONSUMER_ID,
            execution_id,
            execution_id,
            "internal_llm",
            selection,
            started_at_unix_ms,
            duration_ms,
            status,
            usage,
            error,
        )
        .with_metrics(Some(input_item_count), output_item_count, None)
        .with_first_response(first_response_at_unix_ms)
        .with_transport_from(active_facts);
        if first_response_at_unix_ms.is_some() {
            self.emit_internal_inference_first_response(&facts);
        }
        self.emit_internal_inference_telemetry(facts);
    }

    /// Dispatches one content-free internal inference start event without blocking the caller.
    /// 在不阻塞调用方的情况下分派一条不含内容的内部推理开始事件。
    ///
    /// # Parameters
    /// 参数
    /// `facts` contains only host identity, immutable route, timing, and operation cardinality.
    /// `facts` 只包含宿主身份、不可变路由、计时与操作基数。
    ///
    /// # Returns
    /// 返回值
    /// Returns no value; disabled plugins and hook failures are diagnostic-only.
    /// 不返回值；插件禁用与 Hook 失败仅作为诊断。
    fn emit_internal_inference_started(&self, facts: &InternalInferenceTelemetryFacts) {
        let Some(runtime) = self.telemetry_runtime.clone() else {
            return;
        };
        let session_id = facts.execution_id.clone();
        let mut payload = internal_inference_identity_payload(facts);
        let object = payload
            .as_object_mut()
            .expect("Internal inference telemetry identity must be an object.");
        object.insert(
            String::from("started_at_ms"),
            json!(facts.started_at_unix_ms),
        );
        dispatch_internal_inference_hook(runtime, "provider.request.started", session_id, payload);
    }

    /// Dispatches one content-free retry wait event for a failed physical provider attempt.
    /// 为一次失败的真实供应商尝试分派一条不含内容的重试等待事件。
    ///
    /// # Parameters
    /// 参数
    /// `facts` binds the logical request, `decision` supplies the exact next attempt, and
    /// `error_category` is the normalized stable failure category.
    /// `facts` 绑定逻辑请求，`decision` 提供精确下一次尝试，`error_category` 是归一化稳定失败类别。
    ///
    /// # Returns
    /// 返回值
    /// Returns no value; disabled plugins and hook failures cannot change retry execution.
    /// 不返回值；插件禁用与 Hook 失败不能改变重试执行。
    fn emit_internal_inference_retrying(
        &self,
        facts: &InternalInferenceTelemetryFacts,
        decision: &ProviderRetryDecision,
        error_category: &str,
    ) {
        let Some(runtime) = self.telemetry_runtime.clone() else {
            return;
        };
        let session_id = facts.execution_id.clone();
        let mut payload = internal_inference_identity_payload(facts);
        let object = payload
            .as_object_mut()
            .expect("Internal inference telemetry identity must be an object.");
        object.insert(String::from("retry_number"), json!(decision.retry_number));
        object.insert(String::from("max_retries"), json!(decision.max_retries));
        object.insert(
            String::from("delay_ms"),
            json!(u64::try_from(decision.delay.as_millis()).unwrap_or(u64::MAX)),
        );
        object.insert(String::from("error_category"), json!(error_category));
        object.insert(String::from("retrying_at_ms"), json!(current_unix_millis()));
        dispatch_internal_inference_hook(runtime, "provider.request.retrying", session_id, payload);
    }

    /// Dispatches the first semantic provider response for one internal inference execution.
    /// 分派一次内部推理执行的首个供应商语义响应。
    ///
    /// # Parameters
    /// 参数
    /// `facts` must contain the execution driver's exact first-response timestamp.
    /// `facts` 必须包含执行驱动捕获的精确首响应时间戳。
    ///
    /// # Returns
    /// 返回值
    /// Returns no value; a missing first response produces no fabricated event.
    /// 不返回值；缺少首响应时不会虚构事件。
    fn emit_internal_inference_first_response(&self, facts: &InternalInferenceTelemetryFacts) {
        let Some(first_response_at_unix_ms) = facts.first_token_at_unix_ms else {
            return;
        };
        let Some(runtime) = self.telemetry_runtime.clone() else {
            return;
        };
        let session_id = facts.execution_id.clone();
        let mut payload = internal_inference_identity_payload(facts);
        let object = payload
            .as_object_mut()
            .expect("Internal inference telemetry identity must be an object.");
        object.insert(
            String::from("started_at_ms"),
            json!(facts.started_at_unix_ms),
        );
        object.insert(
            String::from("first_token_at_ms"),
            json!(first_response_at_unix_ms),
        );
        object.insert(
            String::from("time_to_first_token_ms"),
            json!(first_response_at_unix_ms.saturating_sub(facts.started_at_unix_ms)),
        );
        dispatch_internal_inference_hook(
            runtime,
            "provider.response.first_token",
            session_id,
            payload,
        );
    }

    /// Dispatches one content-free internal inference terminal event without blocking the caller.
    /// 在不阻塞调用方的情况下分派一条不含内容的内部推理终态事件。
    ///
    /// # Parameters
    /// 参数
    /// `facts` contains host identity, immutable route, timing, status, metrics, and usage.
    /// `facts` 包含宿主身份、不可变路由、计时、状态、指标与用量。
    ///
    /// # Returns
    /// 返回值
    /// Returns no value; disabled plugins and hook failures are diagnostic-only.
    /// 不返回值；插件禁用与 Hook 失败仅作为诊断。
    fn emit_internal_inference_telemetry(&self, facts: InternalInferenceTelemetryFacts) {
        let completed_at_unix_ms = current_unix_millis();
        let session_id = facts.execution_id.clone();
        let payload = internal_inference_terminal_payload(&facts, completed_at_unix_ms);
        if let Some(runtime) = self.telemetry_runtime.clone() {
            dispatch_internal_inference_hook(
                runtime,
                "provider.request.completed",
                session_id,
                payload.clone(),
            );
        }
        if let Some(storage) = self.usage_storage.clone() {
            match internal_inference_native_usage_event(&facts, payload) {
                Ok(event) => {
                    tokio::spawn(async move {
                        if let Err(error) =
                            persist_native_provider_usage_attempt(&storage, &event).await
                        {
                            tracing::warn!(
                                "[webui-service] native configured-inference usage persistence failed: {error}"
                            );
                        }
                    });
                }
                Err(error) => tracing::warn!(
                    "[webui-service] native configured-inference usage event rejected: {error}"
                ),
            }
        }
    }

    /// Returns the exact configured capabilities visible to one authorized caller.
    /// 返回一个已授权调用方可见的精确已配置能力。
    ///
    /// # Parameters
    /// 参数
    /// `caller` supplies host-trusted status permission and `consumer_profile_id`
    /// selects one isolated inference profile.
    /// `caller` 提供宿主可信状态权限，`consumer_profile_id` 选择一个隔离推理配置档。
    ///
    /// # Returns
    /// 返回值
    /// A complete snapshot that preserves unavailable configured routes as diagnostics.
    /// 返回保留不可用已配置路由诊断的完整快照。
    pub fn capability_snapshot(
        &self,
        caller: &InferenceCallerContext,
        consumer_profile_id: &str,
    ) -> Result<InferenceCapabilitySnapshot, ConfiguredInferenceError> {
        authorize_caller(caller, consumer_profile_id, InferencePermission::Status)?;
        let configuration = self.configuration_snapshot()?;
        let profile = configuration
            .inference_profiles
            .get(consumer_profile_id)
            .ok_or_else(|| {
                ConfiguredInferenceError::new(
                    ConfiguredInferenceErrorCategory::Configuration,
                    format!(
                        "Inference consumer profile `{consumer_profile_id}` is not configured."
                    ),
                )
            })?;
        let mut operations = Vec::new();
        for (purpose_id, route) in &profile.llm_routes {
            operations.push(self.operation_capability(
                &configuration,
                route,
                Some(purpose_id.clone()),
                caller.usage_scope,
            ));
        }
        operations.push(match profile.embedding.as_ref() {
            Some(route) => {
                self.operation_capability(&configuration, route, None, caller.usage_scope)
            }
            None => unavailable_operation_capability(
                AiOperation::EmbeddingCreate,
                None,
                "No embedding route is configured for this consumer profile.",
            ),
        });
        operations.push(match profile.rerank.as_ref() {
            Some(route) => {
                self.operation_capability(&configuration, route, None, caller.usage_scope)
            }
            None => unavailable_operation_capability(
                AiOperation::RerankDocuments,
                None,
                "No rerank route is configured for this consumer profile.",
            ),
        });
        Ok(InferenceCapabilitySnapshot {
            contract_version: vulcan_ai_contract::INFERENCE_CONTRACT_VERSION,
            consumer_profile_id: consumer_profile_id.to_string(),
            operations,
        })
    }

    /// Resolves one configured route into an available or diagnostic capability entry.
    /// 将一条已配置路由解析为可用或诊断能力条目。
    ///
    /// # Parameters
    /// 参数
    /// `configuration`, `route`, `purpose_id`, and `usage_scope` are one immutable
    /// authorization and catalog snapshot input.
    /// `configuration`、`route`、`purpose_id` 与 `usage_scope`
    /// 是一组不可变授权及目录快照输入。
    ///
    /// # Returns
    /// 返回值
    /// One capability entry that never silently substitutes another route.
    /// 返回一项绝不静默替换其他路由的能力条目。
    fn operation_capability(
        &self,
        configuration: &PersistedAiConfiguration,
        route: &InferenceRouteConfiguration,
        purpose_id: Option<String>,
        usage_scope: InferenceUsageScope,
    ) -> InferenceOperationCapability {
        if route.usage_scope != usage_scope {
            return unavailable_operation_capability(
                route.operation,
                purpose_id,
                format!(
                    "Configured inference route usage scope `{:?}` does not match caller scope `{:?}`.",
                    route.usage_scope, usage_scope
                )
                .as_str(),
            );
        }
        match self
            .runtime
            .resolve_internal_inference_selection(configuration, route)
        {
            Ok(selection) => InferenceOperationCapability {
                operation: route.operation,
                purpose_id,
                available: true,
                unavailable_reason: None,
                route: Some(configured_route_snapshot(&selection, route)),
                embedding: selection.model.capabilities.embedding.clone(),
                embedding_batch: (route.operation == AiOperation::EmbeddingCreate)
                    .then(|| {
                        registered_native_embedding_batch_capabilities(
                            &selection.route.entry,
                            selection.model.id.as_str(),
                        )
                        .cloned()
                    })
                    .flatten(),
                rerank: selection.model.capabilities.rerank.clone(),
            },
            Err(error) => {
                unavailable_operation_capability(route.operation, purpose_id, error.as_str())
            }
        }
    }

    /// Coalesces one exact non-streaming request only while its leader remains active.
    /// 仅在领导请求仍活动期间合并一项精确非流式请求。
    ///
    /// # Parameters
    /// 参数
    /// `registry`, `key`, and `request_fingerprint` define the closed running identity;
    /// `operation` performs exactly one provider dispatch for the elected leader.
    /// `registry`、`key` 与 `request_fingerprint` 定义封闭运行中身份；
    /// `operation` 仅由选出的领导请求执行一次供应商分派。
    ///
    /// # Returns
    /// 返回值
    /// The leader result shared with concurrent exact duplicates.
    /// 返回与精确并发重复请求共享的领导结果。
    async fn execute_shared_inference<T, F>(
        &self,
        registry: &Mutex<BTreeMap<String, Arc<SharedInferenceResult<T>>>>,
        key: String,
        request_fingerprint: String,
        operation: F,
    ) -> Result<T, ConfiguredInferenceError>
    where
        T: Clone,
        F: Future<Output = Result<T, ConfiguredInferenceError>>,
    {
        let (state, is_leader) = {
            let mut registry = registry.lock().map_err(lock_error)?;
            match registry.get(key.as_str()) {
                Some(state) => {
                    if state.request_fingerprint != request_fingerprint {
                        return Err(ConfiguredInferenceError::new(
                            ConfiguredInferenceErrorCategory::Contract,
                            "An in-flight inference request reused its identity with different content.",
                        ));
                    }
                    (Arc::clone(state), false)
                }
                None => {
                    let state = Arc::new(SharedInferenceResult {
                        request_fingerprint,
                        result: Mutex::new(None),
                        terminal_notify: Notify::new(),
                    });
                    registry.insert(key.clone(), Arc::clone(&state));
                    (state, true)
                }
            }
        };
        if !is_leader {
            loop {
                let notified = state.terminal_notify.notified();
                tokio::pin!(notified);
                // Register before reading the shared terminal result because all leaders notify waiters.
                // 在读取共享终态结果前完成注册，因为所有领导请求都使用 notify_waiters。
                notified.as_mut().enable();
                if let Some(result) = state.result.lock().map_err(lock_error)?.clone() {
                    return result;
                }
                notified.as_mut().await;
            }
        }
        let guard = SharedInferenceLeaderGuard {
            registry,
            key,
            state,
            completed: false,
        };
        guard.complete(operation.await)
    }

    /// Executes one replay-safe non-streaming provider operation under the live retry policy.
    /// 在实时重试策略下执行一项可安全重放的非流式供应商操作。
    ///
    /// # Parameters
    /// 参数
    /// `caller` supplies the absolute deadline, `facts` binds privacy-safe telemetry, and
    /// `operation` creates one fresh provider future for each physical attempt.
    /// `caller` 提供绝对截止时间，`facts` 绑定隐私安全遥测，`operation` 为每次真实尝试创建新供应商 Future。
    ///
    /// # Returns
    /// 返回值
    /// The normalized result or terminal controlled failure plus the exact executed retry count.
    /// 返回归一化结果或终态受控失败，并携带实际执行的精确重试次数。
    async fn execute_non_streaming_provider_with_retry<T, Factory, Attempt>(
        &self,
        caller: &InferenceCallerContext,
        facts: &InternalInferenceTelemetryFacts,
        mut operation: Factory,
    ) -> (Result<T, ConfiguredInferenceError>, u32)
    where
        Factory: FnMut() -> Attempt,
        Attempt: Future<Output = Result<T, ProviderExecutionFailure>>,
    {
        let mut completed_retries = 0_u32;
        loop {
            let failure =
                match await_provider_attempt_with_deadline(operation(), caller.deadline_unix_ms)
                    .await
                {
                    Ok(Ok(value)) => return (Ok(value), completed_retries),
                    Ok(Err(failure)) => failure,
                    Err(error) => return (Err(error), completed_retries),
                };
            let policy = self
                .provider_retry_policy
                .read()
                .unwrap_or_else(|poisoned| {
                    tracing::warn!(
                        "[webui-service] recovering poisoned configured-inference retry policy for read"
                    );
                    poisoned.into_inner()
                })
                .clone();
            let Some(decision) =
                policy.next_structured_retry(failure.error.as_ref(), completed_retries)
            else {
                return (
                    Err(ConfiguredInferenceError::from_provider_failure(failure)),
                    completed_retries,
                );
            };
            let normalized_failure =
                ConfiguredInferenceError::from_provider_failure(failure.clone());
            self.emit_internal_inference_retrying(
                facts,
                &decision,
                normalized_failure.category_label(),
            );
            let retry_wait = tokio::time::sleep(decision.delay);
            let deadline_wait = wait_until_deadline(caller.deadline_unix_ms);
            tokio::pin!(retry_wait);
            tokio::pin!(deadline_wait);
            tokio::select! {
                _ = &mut retry_wait => {
                    completed_retries = decision.retry_number;
                }
                _ = &mut deadline_wait => {
                    return (
                        Err(ConfiguredInferenceError::new(
                            ConfiguredInferenceErrorCategory::Limit,
                            "Configured inference retry wait exceeded its caller deadline.",
                        )),
                        completed_retries,
                    );
                }
            }
        }
    }

    /// Waits for one safe restricted-LLM retry or returns the exact terminal reason.
    /// 等待一次安全的受限 LLM 重试，或返回精确终止原因。
    ///
    /// # Parameters
    /// 参数
    /// `facts` binds the retry hook to the frozen route, `failure` preserves typed
    /// dispatch safety, `completed_retries` counts executed retries, and cancellation
    /// plus the absolute deadline remain authoritative while waiting.
    /// `facts` 将重试 Hook 绑定到冻结路由，`failure` 保留强类型分派安全事实，
    /// `completed_retries` 统计已执行重试，等待期间取消与绝对截止时间仍具权威性。
    ///
    /// # Returns
    /// 返回值
    /// The next completed-retry count after the delay, or a controlled provider,
    /// cancellation, or deadline failure.
    /// 等待结束后返回下一项已完成重试计数，或返回受控供应商、取消或截止时间失败。
    async fn wait_for_restricted_llm_retry(
        &self,
        facts: &InternalInferenceTelemetryFacts,
        failure: ProviderExecutionFailure,
        completed_retries: u32,
        cancellation: &mut watch::Receiver<bool>,
        deadline_unix_ms: Option<u64>,
    ) -> Result<u32, ConfiguredInferenceError> {
        let policy = self
            .provider_retry_policy
            .read()
            .unwrap_or_else(|poisoned| {
                tracing::warn!(
                    "[webui-service] recovering poisoned configured-inference retry policy for read"
                );
                poisoned.into_inner()
            })
            .clone();
        let Some(decision) =
            policy.next_structured_retry(failure.error.as_ref(), completed_retries)
        else {
            return Err(ConfiguredInferenceError::from_provider_failure(failure));
        };
        let normalized_failure = ConfiguredInferenceError::from_provider_failure(failure);
        self.emit_internal_inference_retrying(
            facts,
            &decision,
            normalized_failure.category_label(),
        );
        let retry_wait = tokio::time::sleep(decision.delay);
        let deadline_wait = wait_until_deadline(deadline_unix_ms);
        tokio::pin!(retry_wait);
        tokio::pin!(deadline_wait);
        tokio::select! {
            _ = &mut retry_wait => Ok(decision.retry_number),
            cancellation_result = cancellation.changed() => {
                if cancellation_result.is_ok() && *cancellation.borrow() {
                    Err(ConfiguredInferenceError::new(
                        ConfiguredInferenceErrorCategory::Cancelled,
                        "Restricted LLM execution was cancelled by its owning caller.",
                    ))
                } else {
                    Err(ConfiguredInferenceError::new(
                        ConfiguredInferenceErrorCategory::Execution,
                        "Restricted LLM cancellation channel closed during provider retry wait.",
                    ))
                }
            }
            _ = &mut deadline_wait => Err(ConfiguredInferenceError::new(
                ConfiguredInferenceErrorCategory::Limit,
                "Restricted LLM retry wait exceeded its caller deadline.",
            )),
        }
    }

    /// Executes one configured embedding request.
    /// 执行一次已配置向量请求。
    ///
    /// # Parameters
    /// 参数
    /// `caller` is trusted host identity and `request` cannot select a provider route.
    /// `caller` 是可信宿主身份，`request` 无法选择供应商路由。
    ///
    /// # Returns
    /// 返回值
    /// A complete normalized embedding response.
    /// 返回完整归一化向量响应。
    pub async fn create_embeddings(
        &self,
        caller: &InferenceCallerContext,
        request: EmbeddingRequest,
    ) -> Result<EmbeddingResponse, ConfiguredInferenceError> {
        request.validate().map_err(|message| {
            ConfiguredInferenceError::new(ConfiguredInferenceErrorCategory::Contract, message)
        })?;
        authorize_caller(
            caller,
            request.consumer_profile_id.as_str(),
            InferencePermission::Embedding,
        )?;
        enforce_input_budget(caller, embedding_input_units(&request)?)?;
        let request_fingerprint = serde_json::to_string(&request).map_err(|error| {
            ConfiguredInferenceError::new(
                ConfiguredInferenceErrorCategory::Contract,
                format!("Embedding request could not be fingerprinted: {error}"),
            )
        })?;
        let in_flight_key = inference_in_flight_key(
            caller,
            request.consumer_profile_id.as_str(),
            AiOperation::EmbeddingCreate,
            request.request_id.as_str(),
            request.idempotency_key.as_deref(),
        );
        self.execute_shared_inference(
            &self.in_flight_embeddings,
            in_flight_key,
            request_fingerprint,
            async { self.create_embeddings_once(caller, request).await },
        )
        .await
    }

    /// Plans deterministic asynchronous embedding Batch tasks through the configured route.
    /// 通过已配置路由规划确定性的异步向量 Batch 任务。
    pub fn plan_embedding_batch(
        &self,
        caller: &InferenceCallerContext,
        request: EmbeddingRequest,
    ) -> Result<EmbeddingBatchPlan, ConfiguredInferenceError> {
        request.validate().map_err(|message| {
            ConfiguredInferenceError::new(ConfiguredInferenceErrorCategory::Contract, message)
        })?;
        authorize_caller(
            caller,
            request.consumer_profile_id.as_str(),
            InferencePermission::Embedding,
        )?;
        // Batch aggregate size is governed by the frozen per-task item, payload, and task-count limits.
        // Batch 聚合大小由冻结的单任务条目、负载字节及任务数量上限约束。
        let configuration = self.configuration_snapshot()?;
        let route = configured_operation_route(
            &configuration,
            request.consumer_profile_id.as_str(),
            AiOperation::EmbeddingCreate,
            None,
        )?;
        validate_route_scope(route, caller)?;
        let selection = self
            .runtime
            .resolve_internal_inference_selection(&configuration, route)
            .map_err(configuration_error)?;
        let request = embedding_request_with_internal_defaults(&configuration, &selection, request);
        self.runtime
            .plan_internal_embedding_batch(&selection, &request, caller.usage_scope)
            .map_err(ConfiguredInferenceError::from_provider_failure)
    }

    /// Uploads one exact file-backed embedding Batch input before task creation.
    /// 在创建任务前上传一个精确的文件型向量 Batch 输入。
    pub async fn upload_embedding_batch_input(
        &self,
        caller: &InferenceCallerContext,
        request: EmbeddingRequest,
        request_fingerprint: String,
        task: EmbeddingBatchTaskPlan,
    ) -> Result<EmbeddingBatchUploadedInput, ConfiguredInferenceError> {
        let (selection, request, route_snapshot) =
            self.resolve_embedding_batch_call(caller, request, request_fingerprint.as_str())?;
        validate_embedding_batch_task(&task)?;
        // Freeze the enabled switch for this upload operation.
        // 为本次上传操作冻结开关状态。
        let artifact_sink = self.vce_batch_artifact_sink(caller);
        let _permit = self.acquire_caller_permit(caller, 1).await?;
        self.execute_embedding_batch_provider_operation(
            caller,
            &request,
            &selection,
            &route_snapshot,
            "internal_embedding_batch_upload",
            Some(u64::from(task.input_count)),
            EmbeddingBatchDeadlineDispatchPolicy::NonIdempotent,
            self.runtime.upload_internal_embedding_batch_input(
                &selection,
                &request,
                &task,
                artifact_sink
                    .as_ref()
                    .map(|sink| sink as &dyn EmbeddingBatchArtifactSink),
            ),
            |_| (RequestUsage::default(), None),
        )
        .await
    }

    /// Submits one planned asynchronous embedding Batch task without automatic replay.
    /// 提交一个已规划异步向量 Batch 任务且不执行自动重放。
    pub async fn submit_embedding_batch(
        &self,
        caller: &InferenceCallerContext,
        request: EmbeddingRequest,
        request_fingerprint: String,
        task: EmbeddingBatchTaskPlan,
        uploaded_input: Option<EmbeddingBatchUploadedInput>,
    ) -> Result<EmbeddingBatchSubmission, ConfiguredInferenceError> {
        let (selection, request, route_snapshot) =
            self.resolve_embedding_batch_call(caller, request, request_fingerprint.as_str())?;
        validate_embedding_batch_task(&task)?;
        // Freeze the enabled switch for this submission operation.
        // 为本次提交操作冻结开关状态。
        let artifact_sink = self.vce_batch_artifact_sink(caller);
        let _permit = self.acquire_caller_permit(caller, 1).await?;
        self.execute_embedding_batch_provider_operation(
            caller,
            &request,
            &selection,
            &route_snapshot,
            "internal_embedding_batch_submit",
            Some(u64::from(task.input_count)),
            EmbeddingBatchDeadlineDispatchPolicy::NonIdempotent,
            self.runtime.submit_internal_embedding_batch(
                &selection,
                &request,
                &task,
                uploaded_input.as_ref(),
                artifact_sink
                    .as_ref()
                    .map(|sink| sink as &dyn EmbeddingBatchArtifactSink),
            ),
            |_| (RequestUsage::default(), None),
        )
        .await
    }

    /// Reconciles one file-backed task whose creation response was indeterminate.
    /// 对账一个创建响应不确定的文件型任务。
    pub async fn reconcile_embedding_batch_submission(
        &self,
        caller: &InferenceCallerContext,
        request: EmbeddingRequest,
        request_fingerprint: String,
        task: EmbeddingBatchTaskPlan,
        uploaded_input: EmbeddingBatchUploadedInput,
    ) -> Result<Option<EmbeddingBatchSubmission>, ConfiguredInferenceError> {
        let (selection, request, route_snapshot) =
            self.resolve_embedding_batch_call(caller, request, request_fingerprint.as_str())?;
        validate_embedding_batch_task(&task)?;
        let _permit = self.acquire_caller_permit(caller, 1).await?;
        self.execute_embedding_batch_provider_operation(
            caller,
            &request,
            &selection,
            &route_snapshot,
            "internal_embedding_batch_reconcile",
            Some(u64::from(task.input_count)),
            EmbeddingBatchDeadlineDispatchPolicy::Repeatable,
            self.runtime.reconcile_internal_embedding_batch_submission(
                &selection,
                &task,
                &uploaded_input,
            ),
            |_| (RequestUsage::default(), None),
        )
        .await
    }

    /// Polls one durable asynchronous embedding Batch submission without resubmission.
    /// 在不重新提交的情况下轮询一个持久异步向量 Batch 提交。
    pub async fn embedding_batch_status(
        &self,
        caller: &InferenceCallerContext,
        request: EmbeddingRequest,
        request_fingerprint: String,
        submission: EmbeddingBatchSubmission,
    ) -> Result<EmbeddingBatchSnapshot, ConfiguredInferenceError> {
        let (selection, request, route_snapshot) =
            self.resolve_embedding_batch_call(caller, request, request_fingerprint.as_str())?;
        validate_embedding_batch_submission(&submission)?;
        // Freeze the enabled switch for this status observation.
        // 为本次状态观测冻结开关状态。
        let artifact_sink = self.vce_batch_artifact_sink(caller);
        let _permit = self.acquire_caller_permit(caller, 1).await?;
        self.execute_embedding_batch_provider_operation(
            caller,
            &request,
            &selection,
            &route_snapshot,
            "internal_embedding_batch_status",
            None,
            EmbeddingBatchDeadlineDispatchPolicy::Repeatable,
            self.runtime.internal_embedding_batch_status(
                &selection,
                &submission,
                artifact_sink
                    .as_ref()
                    .map(|sink| sink as &dyn EmbeddingBatchArtifactSink),
            ),
            |snapshot| {
                (
                    RequestUsage::default(),
                    snapshot.completed_requests.map(u64::from),
                )
            },
        )
        .await
    }

    /// Collects one completed asynchronous embedding Batch task.
    /// 收集一个已完成异步向量 Batch 任务。
    pub async fn collect_embedding_batch(
        &self,
        caller: &InferenceCallerContext,
        request: EmbeddingRequest,
        request_fingerprint: String,
        task: EmbeddingBatchTaskPlan,
        snapshot: EmbeddingBatchSnapshot,
    ) -> Result<EmbeddingBatchCollectedResult, ConfiguredInferenceError> {
        let (selection, request, route_snapshot) =
            self.resolve_embedding_batch_call(caller, request, request_fingerprint.as_str())?;
        validate_embedding_batch_task(&task)?;
        validate_embedding_batch_submission(&snapshot.submission)?;
        if snapshot.submission.task_id != task.task_id {
            return Err(ConfiguredInferenceError::new(
                ConfiguredInferenceErrorCategory::Contract,
                "Embedding Batch snapshot does not belong to the collected task.",
            ));
        }
        snapshot.validate().map_err(|message| {
            ConfiguredInferenceError::new(ConfiguredInferenceErrorCategory::Contract, message)
        })?;
        // Freeze the enabled switch for this result collection.
        // 为本次结果收集冻结开关状态。
        let artifact_sink = self.vce_batch_artifact_sink(caller);
        let _permit = self.acquire_caller_permit(caller, 1).await?;
        self.execute_embedding_batch_provider_operation(
            caller,
            &request,
            &selection,
            &route_snapshot,
            "internal_embedding_batch_collect",
            Some(u64::from(task.input_count)),
            EmbeddingBatchDeadlineDispatchPolicy::Repeatable,
            self.runtime.collect_internal_embedding_batch(
                &selection,
                &request,
                &task,
                &snapshot,
                caller.usage_scope,
                artifact_sink
                    .as_ref()
                    .map(|sink| sink as &dyn EmbeddingBatchArtifactSink),
            ),
            |collected| {
                (
                    collected.response.usage.clone(),
                    Some(u64::try_from(collected.response.results.len()).unwrap_or(u64::MAX)),
                )
            },
        )
        .await
    }

    /// Deletes one temporary embedding Batch input file after durable consumer ingestion.
    /// 在调用方持久入库后删除一个临时向量 Batch 输入文件。
    pub async fn cleanup_embedding_batch_input(
        &self,
        caller: &InferenceCallerContext,
        request: EmbeddingRequest,
        request_fingerprint: String,
        submission: EmbeddingBatchSubmission,
    ) -> Result<(), ConfiguredInferenceError> {
        let (selection, request, route_snapshot) =
            self.resolve_embedding_batch_call(caller, request, request_fingerprint.as_str())?;
        validate_embedding_batch_submission(&submission)?;
        let _permit = self.acquire_caller_permit(caller, 1).await?;
        self.execute_embedding_batch_provider_operation(
            caller,
            &request,
            &selection,
            &route_snapshot,
            "internal_embedding_batch_cleanup",
            None,
            EmbeddingBatchDeadlineDispatchPolicy::Repeatable,
            self.runtime
                .cleanup_internal_embedding_batch_input(&selection, &submission),
            |_| (RequestUsage::default(), None),
        )
        .await
    }

    /// Cancels one durable asynchronous embedding Batch submission when supported.
    /// 在供应商支持时取消一个持久异步向量 Batch 提交。
    pub async fn cancel_embedding_batch(
        &self,
        caller: &InferenceCallerContext,
        request: EmbeddingRequest,
        request_fingerprint: String,
        submission: EmbeddingBatchSubmission,
    ) -> Result<EmbeddingBatchSnapshot, ConfiguredInferenceError> {
        authorize_caller(
            caller,
            request.consumer_profile_id.as_str(),
            InferencePermission::Cancel,
        )?;
        let (selection, request, route_snapshot) =
            self.resolve_embedding_batch_call(caller, request, request_fingerprint.as_str())?;
        validate_embedding_batch_submission(&submission)?;
        let _permit = self.acquire_caller_permit(caller, 1).await?;
        self.execute_embedding_batch_provider_operation(
            caller,
            &request,
            &selection,
            &route_snapshot,
            "internal_embedding_batch_cancel",
            None,
            EmbeddingBatchDeadlineDispatchPolicy::Repeatable,
            self.runtime
                .cancel_internal_embedding_batch(&selection, &submission),
            |snapshot| {
                (
                    RequestUsage::default(),
                    snapshot.completed_requests.map(u64::from),
                )
            },
        )
        .await
    }

    /// Resolves one authorized asynchronous embedding Batch call to immutable route facts.
    /// 将一次已授权异步向量 Batch 调用解析为不可变路由事实。
    /// Selects VCE file capture only for the authenticated VCE caller and enabled global switch.
    /// 仅为经认证的 VCE 调用方且全局开关已开启时选择 VCE 文件捕获。
    ///
    /// `caller` is the host-issued inference identity; the result is absent when capture is disabled.
    /// `caller` 是宿主签发的推理身份；捕获关闭时返回空值。
    fn vce_batch_artifact_sink(
        &self,
        caller: &InferenceCallerContext,
    ) -> Option<VceEmbeddingBatchArtifactSink> {
        if !self.provider_request_trace_enabled.load(Ordering::Acquire) {
            return None;
        }
        // The host VCE registration owns this exact project-scoped caller prefix.
        // 宿主 VCE 注册持有这一精确的项目调用方前缀。
        let project_id = caller.caller_id.strip_prefix("vulcan-code-vce:")?;
        if project_id.is_empty() {
            return None;
        }
        Some(VceEmbeddingBatchArtifactSink {
            data_root: vulcan_workspace_store::data_plane::application_data_root(),
            project_id: project_id.to_owned(),
        })
    }

    fn resolve_embedding_batch_call(
        &self,
        caller: &InferenceCallerContext,
        request: EmbeddingRequest,
        request_fingerprint: &str,
    ) -> Result<
        (
            ResolvedPublicAiSelection,
            EmbeddingRequest,
            ExecutionRouteSnapshot,
        ),
        ConfiguredInferenceError,
    > {
        request.validate().map_err(|message| {
            ConfiguredInferenceError::new(ConfiguredInferenceErrorCategory::Contract, message)
        })?;
        authorize_caller(
            caller,
            request.consumer_profile_id.as_str(),
            InferencePermission::Embedding,
        )?;
        // Lifecycle calls revalidate the frozen Batch partition instead of applying the synchronous request budget.
        // 生命周期调用重新校验冻结 Batch 分区，而不套用同步请求预算。
        let configuration = self.configuration_snapshot()?;
        let route = configured_operation_route(
            &configuration,
            request.consumer_profile_id.as_str(),
            AiOperation::EmbeddingCreate,
            None,
        )?;
        validate_route_scope(route, caller)?;
        let selection = self
            .runtime
            .resolve_internal_inference_selection(&configuration, route)
            .map_err(configuration_error)?;
        let request = embedding_request_with_internal_defaults(&configuration, &selection, request);
        let (route_snapshot, current_request_fingerprint) = self
            .runtime
            .internal_embedding_batch_request_identity(&selection, &request, caller.usage_scope)
            .map_err(ConfiguredInferenceError::from_provider_failure)?;
        validate_embedding_batch_request_fingerprint(
            current_request_fingerprint.as_str(),
            request_fingerprint,
        )?;
        Ok((selection, request, route_snapshot))
    }

    /// Executes one non-replayed embedding Batch lifecycle request with deadline and telemetry.
    /// 在截止时间与遥测约束下执行一次不重放的向量 Batch 生命周期请求。
    ///
    /// # Parameters
    /// 参数
    /// `caller`, `request`, and `selection` are the authorized frozen call facts;
    /// `owner_kind` identifies the lifecycle operation, `input_item_count` is content-free
    /// cardinality, `deadline_dispatch_policy` declares whether a timed-out started call can be
    /// replayed, `attempt` is exactly one provider request, and `success_metrics` extracts
    /// normalized usage and output count without retaining payload content.
    /// `caller`、`request` 与 `selection` 是已授权的冻结调用事实；`owner_kind` 标识生命周期操作，
    /// `input_item_count` 是不含内容的基数，`deadline_dispatch_policy` 声明已启动调用超时后
    /// 能否重放，`attempt` 是恰好一次供应商请求，`success_metrics` 在不保留载荷内容的前提下
    /// 提取归一化用量与输出数量。
    ///
    /// # Returns
    /// 返回值
    /// The provider result, or a route-bound controlled failure after emitting terminal telemetry.
    /// 返回供应商结果；失败时在发出终态遥测后返回绑定路由的受控错误。
    // Frozen route identity, caller identity and execution accounting are separate authorities.
    // 冻结路由身份、调用方身份及执行计量属于不同权威。
    #[expect(
        clippy::too_many_arguments,
        reason = "Frozen route identity, caller identity and execution accounting are separate authorities."
    )]
    async fn execute_embedding_batch_provider_operation<T, Attempt, SuccessMetrics>(
        &self,
        caller: &InferenceCallerContext,
        request: &EmbeddingRequest,
        selection: &ResolvedPublicAiSelection,
        route_snapshot: &ExecutionRouteSnapshot,
        owner_kind: &'static str,
        input_item_count: Option<u64>,
        deadline_dispatch_policy: EmbeddingBatchDeadlineDispatchPolicy,
        attempt: Attempt,
        success_metrics: SuccessMetrics,
    ) -> Result<T, ConfiguredInferenceError>
    where
        Attempt: Future<Output = Result<T, ProviderExecutionFailure>>,
        SuccessMetrics: FnOnce(&T) -> (RequestUsage, Option<u64>),
    {
        let telemetry_execution_id = Uuid::new_v4().to_string();
        let started_at_unix_ms = current_unix_millis();
        let started_facts = InternalInferenceTelemetryFacts::from_selection(
            caller,
            request.consumer_profile_id.as_str(),
            request.request_id.as_str(),
            telemetry_execution_id.as_str(),
            owner_kind,
            selection,
            started_at_unix_ms,
            0,
            "succeeded",
            &RequestUsage::default(),
            None,
        )
        .with_execution_route(route_snapshot)
        .with_metrics(input_item_count, None, request.dimensions.map(u64::from));
        self.emit_internal_inference_started(&started_facts);
        let result =
            match await_provider_attempt_with_deadline(attempt, caller.deadline_unix_ms).await {
                Ok(Ok(value)) => Ok(value),
                Ok(Err(failure)) => Err(ConfiguredInferenceError::from_provider_failure(failure)),
                Err(error) => Err(match deadline_dispatch_policy {
                    EmbeddingBatchDeadlineDispatchPolicy::Repeatable => error,
                    EmbeddingBatchDeadlineDispatchPolicy::NonIdempotent => {
                        error.with_unknown_batch_dispatch()
                    }
                }),
            }
            .map_err(|error| error.with_route(route_snapshot.clone()));
        match result {
            Ok(value) => {
                let first_response_at_unix_ms = current_unix_millis();
                let (usage, output_item_count) = success_metrics(&value);
                let facts = InternalInferenceTelemetryFacts::succeeded(
                    caller,
                    request.consumer_profile_id.as_str(),
                    request.request_id.as_str(),
                    telemetry_execution_id.as_str(),
                    owner_kind,
                    selection,
                    started_at_unix_ms,
                    first_response_at_unix_ms.saturating_sub(started_at_unix_ms),
                    &usage,
                )
                .with_execution_route(route_snapshot)
                .with_metrics(
                    input_item_count,
                    output_item_count,
                    request.dimensions.map(u64::from),
                )
                .with_first_response(Some(first_response_at_unix_ms));
                self.emit_internal_inference_first_response(&facts);
                self.emit_internal_inference_telemetry(facts);
                Ok(value)
            }
            Err(error) => {
                self.emit_internal_inference_telemetry(
                    InternalInferenceTelemetryFacts::failed(
                        caller,
                        request.consumer_profile_id.as_str(),
                        request.request_id.as_str(),
                        telemetry_execution_id.as_str(),
                        owner_kind,
                        selection,
                        started_at_unix_ms,
                        &error,
                    )
                    .with_execution_route(route_snapshot)
                    .with_metrics(
                        input_item_count,
                        None,
                        request.dimensions.map(u64::from),
                    ),
                );
                Err(error)
            }
        }
    }

    /// Executes the elected embedding leader after shared request reservation.
    /// 在共享请求预留后执行选出的向量领导请求。
    ///
    /// # Parameters
    /// 参数
    /// `caller` and `request` have already passed the public contract gate.
    /// `caller` 与 `request` 已通过公共契约入口校验。
    ///
    /// # Returns
    /// 返回值
    /// One complete provider result or controlled failure for all current followers.
    /// 返回供全部当前跟随请求共享的完整供应商结果或受控失败。
    async fn create_embeddings_once(
        &self,
        caller: &InferenceCallerContext,
        request: EmbeddingRequest,
    ) -> Result<EmbeddingResponse, ConfiguredInferenceError> {
        request.validate().map_err(|message| {
            ConfiguredInferenceError::new(ConfiguredInferenceErrorCategory::Contract, message)
        })?;
        authorize_caller(
            caller,
            request.consumer_profile_id.as_str(),
            InferencePermission::Embedding,
        )?;
        let input_units = embedding_input_units(&request)?;
        enforce_input_budget(caller, input_units)?;
        let _permit = self.acquire_caller_permit(caller, input_units).await?;
        let configuration = self.configuration_snapshot()?;
        let route = configured_operation_route(
            &configuration,
            request.consumer_profile_id.as_str(),
            AiOperation::EmbeddingCreate,
            None,
        )?;
        validate_route_scope(route, caller)?;
        let selection = self
            .runtime
            .resolve_internal_inference_selection(&configuration, route)
            .map_err(configuration_error)?;
        let request = embedding_request_with_internal_defaults(&configuration, &selection, request);
        let route_snapshot = configured_route_snapshot(&selection, route);
        // telemetry_execution_id separates the provider request identity from the UUID-owned usage lifecycle.
        // telemetry_execution_id 将供应商请求身份与由 UUID 持有的用量生命周期分离。
        let telemetry_execution_id = Uuid::new_v4().to_string();
        let started_at_unix_ms = current_unix_millis();
        let input_item_count = u64::try_from(request.inputs.len()).unwrap_or(u64::MAX);
        let started_facts = InternalInferenceTelemetryFacts::from_selection(
            caller,
            request.consumer_profile_id.as_str(),
            request.request_id.as_str(),
            telemetry_execution_id.as_str(),
            "internal_embedding",
            &selection,
            started_at_unix_ms,
            0,
            "succeeded",
            &RequestUsage::default(),
            None,
        )
        .with_metrics(
            Some(input_item_count),
            None,
            request.dimensions.map(u64::from),
        );
        self.emit_internal_inference_started(&started_facts);
        let captured_wire_request = Arc::new(Mutex::new(None::<ProviderWireRequestCapture>));
        let captured_wire_request_for_attempt = Arc::clone(&captured_wire_request);
        let runtime_for_attempt = Arc::clone(&self.runtime);
        let selection_for_attempt = selection.clone();
        let request_for_attempt = request.clone();
        let usage_scope_for_attempt = caller.usage_scope;
        let (result, retry_count) = self
            .execute_non_streaming_provider_with_retry(caller, &started_facts, || {
                let captured_wire_request = Arc::clone(&captured_wire_request_for_attempt);
                let runtime = Arc::clone(&runtime_for_attempt);
                let selection = selection_for_attempt.clone();
                let request = request_for_attempt.clone();
                async move {
                    let (capture_sender, capture_receiver) = oneshot::channel();
                    let result = runtime
                        .execute_internal_embedding_with_wire_request_capture(
                            &selection,
                            &request,
                            usage_scope_for_attempt,
                            Some(capture_sender),
                        )
                        .await;
                    if let Ok(captured) = capture_receiver.await
                        && let Ok(mut retained) = captured_wire_request.lock()
                    {
                        *retained = Some(captured);
                    }
                    result
                }
            })
            .await;
        // result keeps malformed successful responses on the same terminal telemetry path as provider failures.
        // result 让格式错误的成功响应与 Provider 失败进入同一终态遥测路径。
        let diagnostic_route = route_snapshot.clone();
        let result = result
            .and_then(|response| {
                response.validate_for_request(&request).map_err(|message| {
                    ConfiguredInferenceError::new(
                        ConfiguredInferenceErrorCategory::Execution,
                        message,
                    )
                })?;
                Ok(response)
            })
            .map_err(|error| error.with_route(route_snapshot));
        if let Err(error) = &result
            && let Ok(mut retained) = captured_wire_request.lock()
            && let Some(captured) = retained.take()
        {
            Self::persist_vce_embedding_provider_request_trace(
                &request,
                &diagnostic_route,
                error,
                retry_count,
                captured,
            );
        }
        match result {
            Ok(response) => {
                let first_response_at_unix_ms = current_unix_millis();
                let facts = InternalInferenceTelemetryFacts::succeeded(
                    caller,
                    request.consumer_profile_id.as_str(),
                    request.request_id.as_str(),
                    telemetry_execution_id.as_str(),
                    "internal_embedding",
                    &selection,
                    started_at_unix_ms,
                    response.duration_ms,
                    &response.usage,
                )
                .with_retry_count(retry_count)
                .with_metrics(
                    Some(input_item_count),
                    Some(u64::try_from(response.results.len()).unwrap_or(u64::MAX)),
                    Some(u64::from(response.fingerprint.dimensions)),
                )
                .with_first_response(Some(first_response_at_unix_ms));
                self.emit_internal_inference_first_response(&facts);
                self.emit_internal_inference_telemetry(facts);
                Ok(response)
            }
            Err(error) => {
                self.emit_internal_inference_telemetry(
                    InternalInferenceTelemetryFacts::failed(
                        caller,
                        request.consumer_profile_id.as_str(),
                        request.request_id.as_str(),
                        telemetry_execution_id.as_str(),
                        "internal_embedding",
                        &selection,
                        started_at_unix_ms,
                        &error,
                    )
                    .with_retry_count(retry_count)
                    .with_metrics(
                        Some(input_item_count),
                        None,
                        request.dimensions.map(u64::from),
                    ),
                );
                Err(error)
            }
        }
    }

    /// Executes one configured document rerank request.
    /// 执行一次已配置文档重排请求。
    ///
    /// # Parameters
    /// 参数
    /// `caller` is trusted host identity and `request` cannot select a provider route.
    /// `caller` 是可信宿主身份，`request` 无法选择供应商路由。
    ///
    /// # Returns
    /// 返回值
    /// A complete normalized rerank response.
    /// 返回完整归一化重排响应。
    pub async fn rerank_documents(
        &self,
        caller: &InferenceCallerContext,
        request: RerankRequest,
    ) -> Result<RerankResponse, ConfiguredInferenceError> {
        request.validate().map_err(|message| {
            ConfiguredInferenceError::new(ConfiguredInferenceErrorCategory::Contract, message)
        })?;
        authorize_caller(
            caller,
            request.consumer_profile_id.as_str(),
            InferencePermission::Rerank,
        )?;
        enforce_input_budget(caller, rerank_input_units(&request)?)?;
        let request_fingerprint = serde_json::to_string(&request).map_err(|error| {
            ConfiguredInferenceError::new(
                ConfiguredInferenceErrorCategory::Contract,
                format!("Rerank request could not be fingerprinted: {error}"),
            )
        })?;
        let in_flight_key = inference_in_flight_key(
            caller,
            request.consumer_profile_id.as_str(),
            AiOperation::RerankDocuments,
            request.request_id.as_str(),
            request.idempotency_key.as_deref(),
        );
        self.execute_shared_inference(
            &self.in_flight_reranks,
            in_flight_key,
            request_fingerprint,
            async { self.rerank_documents_once(caller, request).await },
        )
        .await
    }

    /// Executes the elected rerank leader after shared request reservation.
    /// 在共享请求预留后执行选出的重排领导请求。
    ///
    /// # Parameters
    /// 参数
    /// `caller` and `request` have already passed the public contract gate.
    /// `caller` 与 `request` 已通过公共契约入口校验。
    ///
    /// # Returns
    /// 返回值
    /// One complete provider result or controlled failure for all current followers.
    /// 返回供全部当前跟随请求共享的完整供应商结果或受控失败。
    async fn rerank_documents_once(
        &self,
        caller: &InferenceCallerContext,
        request: RerankRequest,
    ) -> Result<RerankResponse, ConfiguredInferenceError> {
        request.validate().map_err(|message| {
            ConfiguredInferenceError::new(ConfiguredInferenceErrorCategory::Contract, message)
        })?;
        authorize_caller(
            caller,
            request.consumer_profile_id.as_str(),
            InferencePermission::Rerank,
        )?;
        let input_units = rerank_input_units(&request)?;
        enforce_input_budget(caller, input_units)?;
        let _permit = self.acquire_caller_permit(caller, input_units).await?;
        let configuration = self.configuration_snapshot()?;
        let route = configured_operation_route(
            &configuration,
            request.consumer_profile_id.as_str(),
            AiOperation::RerankDocuments,
            None,
        )?;
        validate_route_scope(route, caller)?;
        let selection = self
            .runtime
            .resolve_internal_inference_selection(&configuration, route)
            .map_err(configuration_error)?;
        let request = rerank_request_with_internal_defaults(&configuration, &selection, request);
        let route_snapshot = configured_route_snapshot(&selection, route);
        // telemetry_execution_id separates the provider request identity from the UUID-owned usage lifecycle.
        // telemetry_execution_id 将供应商请求身份与由 UUID 持有的用量生命周期分离。
        let telemetry_execution_id = Uuid::new_v4().to_string();
        let started_at_unix_ms = current_unix_millis();
        let input_item_count = u64::try_from(request.candidates.len()).unwrap_or(u64::MAX);
        let started_facts = InternalInferenceTelemetryFacts::from_selection(
            caller,
            request.consumer_profile_id.as_str(),
            request.request_id.as_str(),
            telemetry_execution_id.as_str(),
            "internal_rerank",
            &selection,
            started_at_unix_ms,
            0,
            "succeeded",
            &RequestUsage::default(),
            None,
        )
        .with_metrics(Some(input_item_count), None, None);
        self.emit_internal_inference_started(&started_facts);
        let (result, retry_count) = self
            .execute_non_streaming_provider_with_retry(caller, &started_facts, || {
                self.runtime
                    .execute_internal_rerank(&selection, &request, caller.usage_scope)
            })
            .await;
        // result rejects provider output that loses candidate identity or declared rank semantics.
        // result 拒绝丢失候选身份或违反声明排序语义的供应商输出。
        let result = result
            .and_then(|response| {
                response.validate_for_request(&request).map_err(|message| {
                    ConfiguredInferenceError::new(
                        ConfiguredInferenceErrorCategory::Execution,
                        message,
                    )
                })?;
                Ok(response)
            })
            .map_err(|error| error.with_route(route_snapshot));
        match result {
            Ok(response) => {
                let first_response_at_unix_ms = current_unix_millis();
                let facts = InternalInferenceTelemetryFacts::succeeded(
                    caller,
                    request.consumer_profile_id.as_str(),
                    request.request_id.as_str(),
                    telemetry_execution_id.as_str(),
                    "internal_rerank",
                    &selection,
                    started_at_unix_ms,
                    response.duration_ms,
                    &response.usage,
                )
                .with_retry_count(retry_count)
                .with_metrics(
                    Some(input_item_count),
                    Some(u64::try_from(response.results.len()).unwrap_or(u64::MAX)),
                    None,
                )
                .with_first_response(Some(first_response_at_unix_ms));
                self.emit_internal_inference_first_response(&facts);
                self.emit_internal_inference_telemetry(facts);
                Ok(response)
            }
            Err(error) => {
                self.emit_internal_inference_telemetry(
                    InternalInferenceTelemetryFacts::failed(
                        caller,
                        request.consumer_profile_id.as_str(),
                        request.request_id.as_str(),
                        telemetry_execution_id.as_str(),
                        "internal_rerank",
                        &selection,
                        started_at_unix_ms,
                        &error,
                    )
                    .with_retry_count(retry_count)
                    .with_metrics(Some(input_item_count), None, None),
                );
                Err(error)
            }
        }
    }

    /// Starts one configured restricted LLM execution and returns its stable identifier.
    /// 启动一项已配置受限 LLM 执行并返回其稳定标识。
    ///
    /// # Parameters
    /// 参数
    /// `self` must be shared so the execution worker retains the same service state.
    /// `caller` and `request` are validated before any credential is loaded.
    /// `self` 必须共享以便执行工作器保留同一服务状态。
    /// `caller` 与 `request` 在读取任何凭据前完成校验。
    ///
    /// # Returns
    /// 返回值
    /// A new or coalesced in-flight execution identifier.
    /// 返回新建或合并的运行中执行标识。
    pub async fn start_response_execution(
        self: &Arc<Self>,
        caller: InferenceCallerContext,
        request: ResponsesExecutionRequest,
    ) -> Result<String, ConfiguredInferenceError> {
        self.prune_terminal_llm_executions()?;
        request.validate().map_err(|message| {
            ConfiguredInferenceError::new(ConfiguredInferenceErrorCategory::Contract, message)
        })?;
        validate_restricted_structured_output_policy(&request.structured_output)?;
        authorize_caller(
            &caller,
            request.consumer_profile_id.as_str(),
            InferencePermission::Llm,
        )?;
        let input_units = llm_input_units(&request)?;
        enforce_input_budget(&caller, input_units)?;
        enforce_llm_output_budget(&caller, request.output_token_limit)?;
        let request_fingerprint = serde_json::to_string(&request).map_err(|error| {
            ConfiguredInferenceError::new(
                ConfiguredInferenceErrorCategory::Contract,
                format!("Restricted LLM request could not be fingerprinted: {error}"),
            )
        })?;
        let in_flight_key = llm_in_flight_key(&caller, &request);
        let start_lock = self.llm_start_lock(in_flight_key.as_str())?;
        let _start_guard = start_lock.lock().await;
        if let Some(execution_id) =
            self.coalesced_llm_execution(in_flight_key.as_str(), request_fingerprint.as_str())?
        {
            return Ok(execution_id);
        }
        let permit = self.acquire_caller_permit(&caller, input_units).await?;
        let configuration = self.configuration_snapshot()?;
        let route = configured_operation_route(
            &configuration,
            request.consumer_profile_id.as_str(),
            AiOperation::ConversationRespond,
            Some(request.purpose_id.as_str()),
        )?;
        validate_route_scope(route, &caller)?;
        let selection = self
            .runtime
            .resolve_internal_inference_selection(&configuration, route)
            .map_err(configuration_error)?;
        let provider_request =
            responses_provider_request(&configuration, &selection, &request, &caller)?;
        if vmm_purpose_requires_disabled_reasoning(
            request.consumer_profile_id.as_str(),
            request.purpose_id.as_str(),
        ) {
            self.runtime
                .validate_disabled_reasoning_request_projection(&selection, &provider_request)
                .map_err(|message| {
                    ConfiguredInferenceError::new(
                        ConfiguredInferenceErrorCategory::Contract,
                        format!(
                            "VMM request failed final disabled-reasoning projection validation: {message}"
                        ),
                    )
                })?;
        }
        let execution_id = Uuid::new_v4().to_string();
        let route_snapshot = llm_route_snapshot(&selection, caller.usage_scope);
        let (event_sender, _) = broadcast::channel(LLM_EVENT_BUFFER_CAPACITY);
        let (cancellation_sender, cancellation_receiver) = watch::channel(false);
        let state = Arc::new(ConfiguredLlmExecutionState {
            owner_caller_id: caller.caller_id.clone(),
            request_fingerprint,
            in_flight_key: in_flight_key.clone(),
            snapshot: Mutex::new(ConfiguredLlmExecutionSnapshot {
                execution_id: execution_id.clone(),
                phase: ConfiguredLlmExecutionPhase::Running,
                route: route_snapshot.clone(),
                response: None,
                error: None,
            }),
            event_sender,
            event_history: Mutex::new(Vec::new()),
            event_history_bytes: Mutex::new(0),
            event_history_truncated: Mutex::new(false),
            cancellation_sender,
            waiter_count: AtomicUsize::new(0),
            terminal_notify: Notify::new(),
            terminal_at: Mutex::new(None),
        });
        {
            let mut executions = self.llm_executions.lock().map_err(lock_error)?;
            executions.insert(execution_id.clone(), Arc::clone(&state));
        }
        {
            let mut requests = self.in_flight_llm_requests.lock().map_err(lock_error)?;
            requests.insert(in_flight_key, execution_id.clone());
        }
        let service = Arc::clone(self);
        let execution_id_for_task = execution_id.clone();
        tokio::spawn(async move {
            let _permit = permit;
            service
                .drive_llm_execution(
                    state,
                    request,
                    route_snapshot,
                    selection,
                    provider_request,
                    execution_id_for_task.clone(),
                    cancellation_receiver,
                    caller.deadline_unix_ms,
                    caller.caller_id,
                )
                .await;
            service.release_in_flight_llm_request(execution_id_for_task.as_str());
        });
        Ok(execution_id)
    }

    /// Executes and aggregates one restricted LLM request through the shared execution record.
    /// 通过共享执行记录执行并聚合一次受限 LLM 请求。
    ///
    /// # Parameters
    /// 参数
    /// `self`, `caller`, and `request` use the same start/status lifecycle as streaming clients.
    /// `self`、`caller` 与 `request` 使用和流式客户端相同的启动与状态生命周期。
    ///
    /// # Returns
    /// 返回值
    /// The completed response or the exact retained terminal failure.
    /// 返回已完成响应或精确保留的终态失败。
    pub async fn execute_response(
        self: &Arc<Self>,
        caller: InferenceCallerContext,
        request: ResponsesExecutionRequest,
    ) -> Result<ResponsesExecutionResult, ConfiguredInferenceError> {
        let caller_id = caller.caller_id.clone();
        let execution_id = self.start_response_execution(caller, request).await?;
        self.wait_response(caller_id.as_str(), execution_id.as_str())
            .await
    }

    /// Waits for one already-started restricted LLM execution without starting a duplicate request.
    /// 等待一项已经启动的受限 LLM 执行，且不会重新发起重复请求。
    ///
    /// # Parameters
    /// 参数
    /// `caller_id` must own `execution_id`; the execution identifier must have been returned by
    /// `start_response_execution` for the same caller.
    /// `caller_id` 必须拥有 `execution_id`；执行标识必须由同一调用方的 `start_response_execution` 返回。
    ///
    /// # Returns
    /// 返回值
    /// The retained completed aggregate response or its exact terminal failure.
    /// 保留的完整聚合响应或其精确终态失败。
    pub async fn wait_response(
        &self,
        caller_id: &str,
        execution_id: &str,
    ) -> Result<ResponsesExecutionResult, ConfiguredInferenceError> {
        let state = self.owned_llm_execution(caller_id, execution_id)?;
        let _waiter_guard = ConfiguredLlmWaiterGuard::new(Arc::clone(&state));
        loop {
            // Register the notification future before reading the phase so a terminal
            // transition between the read and the await cannot be missed.
            // 在读取阶段前注册通知 Future，避免终态转换发生在读取与等待之间而丢失唤醒。
            let terminal_notification = state.terminal_notify.notified();
            tokio::pin!(terminal_notification);
            // Register before reading the execution phase because terminal publishers use notify_waiters.
            // 在读取执行阶段前完成注册，因为终态发布者使用 notify_waiters。
            terminal_notification.as_mut().enable();
            let snapshot = state.snapshot.lock().map_err(lock_error)?.clone();
            match snapshot.phase {
                ConfiguredLlmExecutionPhase::Running => terminal_notification.as_mut().await,
                ConfiguredLlmExecutionPhase::Completed => {
                    return snapshot.response.ok_or_else(|| {
                        ConfiguredInferenceError::new(
                            ConfiguredInferenceErrorCategory::Execution,
                            "Completed restricted LLM execution has no aggregate response.",
                        )
                    });
                }
                ConfiguredLlmExecutionPhase::Failed | ConfiguredLlmExecutionPhase::Cancelled => {
                    return Err(snapshot.error.unwrap_or_else(|| {
                        ConfiguredInferenceError::new(
                            ConfiguredInferenceErrorCategory::Execution,
                            "Restricted LLM execution ended without a terminal diagnostic.",
                        )
                    }));
                }
            }
        }
    }

    /// Subscribes the owning caller to normalized events from one LLM execution.
    /// 让所属调用方订阅一项 LLM 执行的归一化事件。
    ///
    /// # Parameters
    /// 参数
    /// `caller_id` and `execution_id` must identify the same retained execution.
    /// `caller_id` 与 `execution_id` 必须标识同一项保留执行。
    ///
    /// # Returns
    /// 返回值
    /// A bounded broadcast receiver; lag is reported by the receiver.
    /// 返回有界广播接收器；滞后由接收器报告。
    pub fn stream_response(
        &self,
        caller_id: &str,
        execution_id: &str,
    ) -> Result<(Vec<AiStreamEvent>, broadcast::Receiver<AiStreamEvent>), ConfiguredInferenceError>
    {
        self.prune_terminal_llm_executions()?;
        let state = self.owned_llm_execution(caller_id, execution_id)?;
        let history = state.event_history.lock().map_err(lock_error)?;
        if *state.event_history_truncated.lock().map_err(lock_error)? {
            return Err(ConfiguredInferenceError::new(
                ConfiguredInferenceErrorCategory::Limit,
                format!(
                    "Restricted LLM execution `{execution_id}` replay history exceeded its bounded capacity."
                ),
            ));
        }
        let receiver = state.event_sender.subscribe();
        let replay = history.clone();
        drop(history);
        Ok((replay, receiver))
    }

    /// Returns one caller-owned LLM execution snapshot.
    /// 返回一项调用方所有的 LLM 执行快照。
    ///
    /// # Parameters
    /// 参数
    /// `caller_id` and `execution_id` must identify the same retained execution.
    /// `caller_id` 与 `execution_id` 必须标识同一项保留执行。
    ///
    /// # Returns
    /// 返回值
    /// A stable non-secret status snapshot.
    /// 返回稳定且不含秘密的状态快照。
    pub fn execution_status(
        &self,
        caller_id: &str,
        execution_id: &str,
    ) -> Result<ConfiguredLlmExecutionSnapshot, ConfiguredInferenceError> {
        self.prune_terminal_llm_executions()?;
        self.owned_llm_execution(caller_id, execution_id)?
            .snapshot
            .lock()
            .map_err(lock_error)
            .map(|snapshot| snapshot.clone())
    }

    /// Cancels one active LLM execution owned by the caller.
    /// 取消一项由调用方拥有的活动 LLM 执行。
    ///
    /// # Parameters
    /// 参数
    /// `caller_id` and `execution_id` must identify the same active execution.
    /// `caller_id` 与 `execution_id` 必须标识同一项活动执行。
    ///
    /// # Returns
    /// 返回值
    /// Success after cooperative cancellation has been signalled.
    /// 协作式取消信号发出后返回成功。
    pub fn cancel_execution(
        &self,
        caller_id: &str,
        execution_id: &str,
    ) -> Result<(), ConfiguredInferenceError> {
        self.prune_terminal_llm_executions()?;
        let state = self.owned_llm_execution(caller_id, execution_id)?;
        if state.snapshot.lock().map_err(lock_error)?.phase != ConfiguredLlmExecutionPhase::Running
        {
            return Err(ConfiguredInferenceError::new(
                ConfiguredInferenceErrorCategory::Configuration,
                format!("Restricted LLM execution `{execution_id}` is already terminal."),
            ));
        }
        state.cancellation_sender.send(true).map_err(|_| {
            ConfiguredInferenceError::new(
                ConfiguredInferenceErrorCategory::Execution,
                format!("Restricted LLM execution `{execution_id}` cannot receive cancellation."),
            )
        })
    }

    /// Returns the authoritative persisted inference configuration snapshot.
    /// 返回权威持久化推理配置快照。
    ///
    /// # Returns
    /// 返回值
    /// One immutable configuration or a controlled lock failure.
    /// 返回一份不可变配置或受控锁失败。
    fn configuration_snapshot(&self) -> Result<PersistedAiConfiguration, ConfiguredInferenceError> {
        self.runtime
            .configuration_snapshot()
            .map_err(configuration_error)
    }

    /// Acquires one caller-scoped concurrency reservation.
    /// 获取一项调用方作用域并发预留。
    ///
    /// # Parameters
    /// 参数
    /// `caller` supplies host-authorized ceilings and `input_units` is the exact accepted payload size.
    /// `caller` 提供宿主授权上限，`input_units` 是精确受理载荷大小。
    ///
    /// # Returns
    /// 返回值
    /// An RAII permit or a stable limit failure.
    /// 返回 RAII 许可或稳定限额失败。
    async fn acquire_caller_permit(
        &self,
        caller: &InferenceCallerContext,
        input_units: u64,
    ) -> Result<CallerExecutionPermit, ConfiguredInferenceError> {
        {
            let mut counters = self.caller_execution_counts.lock().map_err(lock_error)?;
            let current = counters
                .get(caller.caller_id.as_str())
                .copied()
                .unwrap_or(0);
            if current >= caller.budget.max_concurrent_executions {
                return Err(ConfiguredInferenceError::new(
                    ConfiguredInferenceErrorCategory::Limit,
                    format!(
                        "Inference caller `{}` reached its concurrent execution limit {}.",
                        caller.caller_id, caller.budget.max_concurrent_executions
                    ),
                ));
            }
            counters.insert(caller.caller_id.clone(), current + 1);
        }
        // Establish cancellation-safe ownership before the first await can suspend this reservation.
        // 在首个 await 可能挂起此预留之前，建立能够安全处理取消的所有权。
        let caller_reservation = CallerConcurrencyReservation {
            caller_id: caller.caller_id.clone(),
            counters: Arc::clone(&self.caller_execution_counts),
        };
        // Queue failure or future cancellation drops the same reservation without manual rollback.
        // 队列失败或协程取消会丢弃同一预留，不需要手动回滚。
        let scheduler_permit = self
            .workload_scheduler
            .acquire(
                InferenceWorkloadClass::from_caller(caller),
                caller.deadline_unix_ms,
            )
            .await?;
        self.reserve_caller_rate_budget(caller, input_units)?;
        Ok(CallerExecutionPermit {
            _caller_reservation: caller_reservation,
            _scheduler_permit: scheduler_permit,
        })
    }

    /// Reserves one request and its exact input units inside the caller rolling minute.
    /// 在调用方滚动分钟窗口中预留一次请求及其精确输入单位。
    ///
    /// # Parameters
    /// 参数
    /// `caller` owns the immutable rate ceilings and `input_units` is measured before dispatch.
    /// `caller` 持有不可变速率上限，`input_units` 在分派前完成计量。
    ///
    /// # Returns
    /// 返回值
    /// Success after an atomic reservation, or a stable limit error without upstream dispatch.
    /// 原子预留后返回成功；超限时在未分派上游请求前返回稳定限额错误。
    fn reserve_caller_rate_budget(
        &self,
        caller: &InferenceCallerContext,
        input_units: u64,
    ) -> Result<(), ConfiguredInferenceError> {
        let now = current_unix_millis();
        let mut windows = self.caller_rate_windows.lock().map_err(lock_error)?;
        // Expired caller windows no longer carry rate state, so remove them before
        // inserting the active caller and keep this process-local map bounded.
        // 已过期的调用方窗口不再承载速率状态，因此在插入活动调用方前清理，
        // 以限制该进程内映射的大小。
        windows.retain(|_, window| now.saturating_sub(window.started_at_unix_ms) < 60_000);
        let window = windows
            .entry(caller.caller_id.clone())
            .or_insert_with(|| CallerRateWindow::new(now));
        if caller
            .budget
            .max_requests_per_minute
            .is_some_and(|limit| window.request_count >= limit)
        {
            return Err(ConfiguredInferenceError::new(
                ConfiguredInferenceErrorCategory::Limit,
                format!(
                    "Inference caller `{}` reached its per-minute request limit.",
                    caller.caller_id
                ),
            ));
        }
        let next_input_units = window.input_units.saturating_add(input_units);
        if caller
            .budget
            .max_input_units_per_minute
            .is_some_and(|limit| next_input_units > limit)
        {
            return Err(ConfiguredInferenceError::new(
                ConfiguredInferenceErrorCategory::Limit,
                format!(
                    "Inference caller `{}` reached its per-minute input-unit limit.",
                    caller.caller_id
                ),
            ));
        }
        window.request_count = window.request_count.saturating_add(1);
        window.input_units = next_input_units;
        Ok(())
    }

    /// Resolves one request-key lock used to serialize idempotency reservation and dispatch start.
    /// 解析一项请求键锁，用于串行化幂等预留与分派启动。
    ///
    /// # Parameters
    /// 参数
    /// `in_flight_key` is the exact caller-scoped request identity.
    /// `in_flight_key` 是精确的调用方作用域请求身份。
    ///
    /// # Returns
    /// 返回值
    /// A shared asynchronous lock while pruning inactive weak entries.
    /// 返回共享异步锁，并清理已失活的弱引用条目。
    fn llm_start_lock(
        &self,
        in_flight_key: &str,
    ) -> Result<Arc<AsyncMutex<()>>, ConfiguredInferenceError> {
        let mut locks = self.llm_start_locks.lock().map_err(lock_error)?;
        locks.retain(|_, lock| lock.strong_count() > 0);
        if let Some(lock) = locks.get(in_flight_key).and_then(Weak::upgrade) {
            return Ok(lock);
        }
        let lock = Arc::new(AsyncMutex::new(()));
        locks.insert(in_flight_key.to_string(), Arc::downgrade(&lock));
        Ok(lock)
    }

    /// Finds one matching active idempotent execution.
    /// 查找一项匹配的活动幂等执行。
    ///
    /// # Parameters
    /// 参数
    /// `in_flight_key` and `request_fingerprint` identify an exact concurrent duplicate.
    /// `in_flight_key` 与 `request_fingerprint` 标识一项精确并发重复请求。
    ///
    /// # Returns
    /// 返回值
    /// The shared execution identifier, no match, or a conflicting-request error.
    /// 返回共享执行标识、无匹配或请求冲突错误。
    fn coalesced_llm_execution(
        &self,
        in_flight_key: &str,
        request_fingerprint: &str,
    ) -> Result<Option<String>, ConfiguredInferenceError> {
        let execution_id = self
            .in_flight_llm_requests
            .lock()
            .map_err(lock_error)?
            .get(in_flight_key)
            .cloned();
        let Some(execution_id) = execution_id else {
            return Ok(None);
        };
        let executions = self.llm_executions.lock().map_err(lock_error)?;
        let state = executions.get(execution_id.as_str()).ok_or_else(|| {
            ConfiguredInferenceError::new(
                ConfiguredInferenceErrorCategory::Execution,
                "In-flight restricted LLM index references a missing execution.",
            )
        })?;
        if state.request_fingerprint != request_fingerprint {
            return Err(ConfiguredInferenceError::new(
                ConfiguredInferenceErrorCategory::Contract,
                "An in-flight restricted LLM request reused its request identity with different content.",
            ));
        }
        Ok(Some(execution_id))
    }

    /// Resolves one execution and enforces caller ownership.
    /// 解析一项执行并强制调用方所有权。
    ///
    /// # Parameters
    /// 参数
    /// `caller_id` is trusted host identity and `execution_id` is caller supplied.
    /// `caller_id` 是可信宿主身份，`execution_id` 由调用方提供。
    ///
    /// # Returns
    /// 返回值
    /// The retained state only when ownership matches.
    /// 仅在所有权匹配时返回保留状态。
    fn owned_llm_execution(
        &self,
        caller_id: &str,
        execution_id: &str,
    ) -> Result<Arc<ConfiguredLlmExecutionState>, ConfiguredInferenceError> {
        let state = self
            .llm_executions
            .lock()
            .map_err(lock_error)?
            .get(execution_id)
            .cloned()
            .ok_or_else(|| {
                ConfiguredInferenceError::new(
                    ConfiguredInferenceErrorCategory::Configuration,
                    format!("Restricted LLM execution `{execution_id}` was not found."),
                )
            })?;
        if state.owner_caller_id != caller_id {
            return Err(ConfiguredInferenceError::new(
                ConfiguredInferenceErrorCategory::Authorization,
                "Inference caller does not own the requested execution.",
            ));
        }
        Ok(state)
    }

    /// Starts one detached writer for the exact VMM transport request captured at dispatch.
    /// 为分派时捕获的精确 VMM 传输请求启动一个独立写入任务。
    ///
    /// # Parameters
    /// 参数
    /// `request` owns the authenticated consumer, purpose, and correlation identities; `route`
    /// is the immutable physical route; `execution_id` identifies the logical execution; and
    /// `attempt_index` distinguishes native retries without changing their shared execution id.
    /// `request` 持有已认证消费者、用途与关联身份；`route` 是不可变真实路由；
    /// `execution_id` 标识逻辑执行；`attempt_index` 用于区分共享同一执行标识的原生重试。
    ///
    /// # Returns
    /// 返回值
    /// A one-shot sender only for opted-in authenticated VMM requests; all other calls return `None`.
    /// 仅对已显式启用的已认证 VMM 请求返回一次性发送器；其余调用返回 `None`。
    fn start_configured_inference_provider_request_trace(
        &self,
        request: &ResponsesExecutionRequest,
        route: &ExecutionRouteSnapshot,
        execution_id: &str,
        attempt_index: u32,
    ) -> Option<tokio::sync::oneshot::Sender<ProviderWireRequestCapture>> {
        if request.consumer_profile_id != VMM_CONSUMER_PROFILE_ID
            || !self.provider_request_trace_enabled.load(Ordering::Acquire)
        {
            return None;
        }
        let (capture_sender, capture_receiver) =
            tokio::sync::oneshot::channel::<ProviderWireRequestCapture>();
        // requestIdentity and routeIdentity freeze non-secret facts before the background task starts.
        // requestIdentity 与 routeIdentity 在后台任务启动前冻结不含秘密的事实。
        let request_identity = json!({
            "consumer_profile_id": request.consumer_profile_id,
            "purpose_id": request.purpose_id,
            "request_id": request.request_id,
            "execution_id": execution_id,
            "attempt_index": attempt_index,
        });
        let route_identity = json!({
            "provider_id": route.provider_id,
            "service_entry_id": route.service_entry_id,
            "connection_id": route.connection_id,
            "model_id": route.model_id,
            "protocol_binding_id": route.protocol_binding_id,
            "action_binding_id": route.action_binding_id,
            "secondary_route_id": route.secondary_route_id,
            "capability_revision": route.capability_revision,
            "catalog_revision": route.catalog_revision,
            "route_revision": route.route_revision,
        });
        let consumer_profile_id = request.consumer_profile_id.clone();
        let execution_id = execution_id.to_string();
        tokio::spawn(async move {
            let Ok(captured) = capture_receiver.await else {
                // A closed channel proves provider encoding or cancellation ended before a transport request existed.
                // 通道关闭证明 Provider 编码或取消在传输请求形成前已结束。
                return;
            };
            let captured_at_unix_ms = current_unix_millis();
            let sequence = CONFIGURED_INFERENCE_PROVIDER_REQUEST_TRACE_SEQUENCE
                .fetch_add(1, Ordering::Relaxed)
                .saturating_add(1);
            let slot_index = sequence.saturating_sub(1)
                % vulcan_workspace_store::workspace_store::CONFIGURED_INFERENCE_PROVIDER_REQUEST_TRACE_SLOT_COUNT;
            let trace = json!({
                "schema_version": CONFIGURED_INFERENCE_PROVIDER_REQUEST_TRACE_SCHEMA_VERSION,
                "captured_at_unix_ms": captured_at_unix_ms,
                "process_sequence": sequence,
                "request": request_identity,
                "route": route_identity,
                "api_request": {
                    "capture_point": "transport_dispatch",
                    "transport": captured.transport,
                    "method": captured.method,
                    "path": captured.path,
                    "resolved_url": captured.resolved_url,
                    "headers": captured.headers,
                    "credential_values_redacted": true,
                    "body": captured.body,
                },
            });
            let data_root = vulcan_workspace_store::data_plane::application_data_root();
            let persisted = tokio::task::spawn_blocking(move || {
                vulcan_workspace_store::workspace_store::persist_configured_inference_provider_request_trace(
                    &data_root,
                    consumer_profile_id.as_str(),
                    slot_index,
                    &trace,
                )
            })
            .await;
            match persisted {
                Ok(Ok(path)) => tracing::info!(
                    "[configured-inference-provider-request-trace] execution={} attempt={} path={}",
                    execution_id,
                    attempt_index,
                    path.display()
                ),
                Ok(Err(error)) => tracing::warn!(
                    "[configured-inference-provider-request-trace] execution={} attempt={} persist_failed={}",
                    execution_id,
                    attempt_index,
                    error
                ),
                Err(error) => tracing::warn!(
                    "[configured-inference-provider-request-trace] execution={} attempt={} writer_failed={}",
                    execution_id,
                    attempt_index,
                    error
                ),
            }
        });
        Some(capture_sender)
    }

    /// Persists the exact native embedding request when its terminal attempt fails.
    /// 原生向量终态尝试失败时持久化精确请求。
    ///
    /// `captured` is produced immediately before provider dispatch with credential-shaped header
    /// values redacted; the logical request and normalized error remain alongside it.
    /// `captured` 在供应商分派前立即生成且已脱敏凭据形态请求头；逻辑请求与规范化错误一并保存。
    fn persist_vce_embedding_provider_request_trace(
        request: &EmbeddingRequest,
        route: &ExecutionRouteSnapshot,
        error: &ConfiguredInferenceError,
        retry_count: u32,
        captured: ProviderWireRequestCapture,
    ) {
        let sequence = CONFIGURED_INFERENCE_PROVIDER_REQUEST_TRACE_SEQUENCE
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1);
        let slot_index = sequence.saturating_sub(1)
            % vulcan_workspace_store::workspace_store::CONFIGURED_INFERENCE_PROVIDER_REQUEST_TRACE_SLOT_COUNT;
        let consumer_profile_id = request.consumer_profile_id.clone();
        let trace = json!({
            "schema_version": VCE_EMBEDDING_PROVIDER_REQUEST_TRACE_SCHEMA_VERSION,
            "captured_at_unix_ms": current_unix_millis(),
            "process_sequence": sequence,
            "request_stage": "embedding",
            "request": request,
            "route": route,
            "retry_count": retry_count,
            "error": error,
            "api_request": {
                "capture_point": "native_embedding_transport_dispatch",
                "transport": captured.transport,
                "method": captured.method,
                "path": captured.path,
                "resolved_url": captured.resolved_url,
                "headers": captured.headers,
                "credential_values_redacted": true,
                "body": captured.body,
            },
        });
        let data_root = vulcan_workspace_store::data_plane::application_data_root();
        tokio::spawn(async move {
            let persisted = tokio::task::spawn_blocking(move || {
                vulcan_workspace_store::workspace_store::persist_configured_inference_provider_request_trace(
                    &data_root,
                    consumer_profile_id.as_str(),
                    slot_index,
                    &trace,
                )
            })
            .await;
            match persisted {
                Ok(Ok(path)) => tracing::info!(
                    "[vce-embedding-provider-request-trace] sequence={} path={}",
                    sequence,
                    path.display()
                ),
                Ok(Err(error)) => tracing::warn!(
                    "[vce-embedding-provider-request-trace] sequence={} persist_failed={}",
                    sequence,
                    error
                ),
                Err(error) => tracing::warn!(
                    "[vce-embedding-provider-request-trace] sequence={} writer_failed={}",
                    sequence,
                    error
                ),
            }
        });
    }

    /// Drives one provider stream into the shared restricted execution record.
    /// 将一条供应商流驱动进共享受限执行记录。
    ///
    /// # Parameters
    /// 参数
    /// All arguments are frozen before the worker starts and contain no secret material.
    /// 全部参数在工作器启动前冻结且不包含秘密材料。
    // Frozen route identity, caller identity and execution accounting are separate authorities.
    // 冻结路由身份、调用方身份及执行计量属于不同权威。
    #[expect(
        clippy::too_many_arguments,
        reason = "Frozen route identity, caller identity and execution accounting are separate authorities."
    )]
    async fn drive_llm_execution(
        &self,
        state: Arc<ConfiguredLlmExecutionState>,
        request: ResponsesExecutionRequest,
        route: ExecutionRouteSnapshot,
        selection: ResolvedPublicAiSelection,
        provider_request: AiExecutionRequest,
        execution_id: String,
        mut cancellation: watch::Receiver<bool>,
        deadline_unix_ms: Option<u64>,
        caller_id: String,
    ) {
        let started_at = Instant::now();
        let input_item_count = u64::try_from(request.messages.len()).unwrap_or(u64::MAX);
        let mut first_response_at_unix_ms = None;
        let mut attempt_first_response_at_unix_ms;
        let mut active_attempt_facts;
        let mut attempt_terminal_emitted;
        let mut output_text = String::new();
        let mut reasoning_text = String::new();
        let mut structured_output = None;
        let mut output_items = Vec::new();
        let mut usage = RequestUsage::default();
        let mut completed_retries = 0_u32;
        let mut response_started_emitted = false;
        let terminal = 'attempts: loop {
            let attempt_started_at_unix_ms = current_unix_millis();
            active_attempt_facts = InternalInferenceTelemetryFacts::from_route(
                caller_id.as_str(),
                request.consumer_profile_id.as_str(),
                request.request_id.as_str(),
                execution_id.as_str(),
                "internal_llm",
                &route,
                attempt_started_at_unix_ms,
                0,
                "succeeded",
                &RequestUsage::default(),
                None,
            )
            .with_retry_count(completed_retries)
            .with_metrics(Some(input_item_count), None, None);
            attempt_first_response_at_unix_ms = None;
            attempt_terminal_emitted = false;
            self.emit_internal_inference_started(&active_attempt_facts);
            let wire_request_capture = self.start_configured_inference_provider_request_trace(
                &request,
                &route,
                execution_id.as_str(),
                completed_retries,
            );
            let deadline_wait = wait_until_deadline(deadline_unix_ms);
            tokio::pin!(deadline_wait);
            // Provider startup is part of the retained execution lifecycle. This keeps
            // cancellation and deadlines effective while response headers are pending.
            // 上游启动属于已保留执行的生命周期，确保等待响应头期间取消与截止时间仍然生效。
            let mut stream = tokio::select! {
                cancellation_result = cancellation.changed() => {
                    let error = if cancellation_result.is_ok() && *cancellation.borrow() {
                        ConfiguredInferenceError::new(
                            ConfiguredInferenceErrorCategory::Cancelled,
                            "Restricted LLM execution was cancelled by its owning caller.",
                        )
                    } else {
                        ConfiguredInferenceError::new(
                            ConfiguredInferenceErrorCategory::Execution,
                            "Restricted LLM cancellation channel closed before provider startup.",
                        )
                    };
                    break 'attempts Err(error);
                }
                _ = &mut deadline_wait => {
                    break 'attempts Err(ConfiguredInferenceError::new(
                        ConfiguredInferenceErrorCategory::Limit,
                        "Restricted LLM execution exceeded its caller deadline.",
                    ));
                }
                stream = self.runtime.execute_internal_llm_with_wire_request_capture(
                    &selection,
                    provider_request.clone(),
                    execution_id.clone(),
                    wire_request_capture,
                ) => {
                    match stream {
                        Ok(stream) => stream,
                        Err(failure) => {
                            let normalized_failure =
                                ConfiguredInferenceError::from_provider_failure(failure.clone());
                            let failed_facts = InternalInferenceTelemetryFacts::from_route(
                                caller_id.as_str(),
                                request.consumer_profile_id.as_str(),
                                request.request_id.as_str(),
                                execution_id.as_str(),
                                "internal_llm",
                                &route,
                                attempt_started_at_unix_ms,
                                current_unix_millis().saturating_sub(attempt_started_at_unix_ms),
                                "failed",
                                &usage,
                                Some(&normalized_failure),
                            )
                            .with_retry_count(completed_retries)
                            .with_metrics(Some(input_item_count), None, None)
                            .with_first_response(attempt_first_response_at_unix_ms)
                            .with_transport_from(&active_attempt_facts);
                            self.emit_internal_inference_telemetry(failed_facts);
                            attempt_terminal_emitted = true;
                            match self
                                .wait_for_restricted_llm_retry(
                                    &active_attempt_facts,
                                    failure,
                                    completed_retries,
                                    &mut cancellation,
                                    deadline_unix_ms,
                                )
                                .await
                            {
                                Ok(retry_count) => {
                                    completed_retries = retry_count;
                                    usage = RequestUsage::default();
                                    continue 'attempts;
                                }
                                Err(error) => break 'attempts Err(error),
                            }
                        }
                    }
                }
            };
            let mut retry_unsafe_event_seen = false;
            loop {
                tokio::select! {
                    cancellation_result = cancellation.changed() => {
                        if cancellation_result.is_ok() && *cancellation.borrow() {
                            break 'attempts Err(ConfiguredInferenceError::new(
                                ConfiguredInferenceErrorCategory::Cancelled,
                                "Restricted LLM execution was cancelled by its owning caller.",
                            ));
                        }
                        if cancellation_result.is_err() {
                            break 'attempts Err(ConfiguredInferenceError::new(
                                ConfiguredInferenceErrorCategory::Execution,
                                "Restricted LLM cancellation channel closed during provider streaming.",
                            ));
                        }
                    }
                    _ = &mut deadline_wait => {
                        break 'attempts Err(ConfiguredInferenceError::new(
                            ConfiguredInferenceErrorCategory::Limit,
                            "Restricted LLM execution exceeded its caller deadline.",
                        ));
                    }
                    next_event = stream.next() => {
                        let Some(next_event) = next_event else {
                            break 'attempts Err(ConfiguredInferenceError::new(
                                ConfiguredInferenceErrorCategory::Execution,
                                "Restricted LLM provider stream ended without a terminal event.",
                            ));
                        };
                        let event = match next_event {
                            Ok(event) => event,
                            Err(failure) => {
                                if retry_unsafe_event_seen {
                                    break 'attempts Err(
                                        ConfiguredInferenceError::from_provider_failure(failure),
                                    );
                                }
                                let normalized_failure =
                                    ConfiguredInferenceError::from_provider_failure(failure.clone());
                                let failed_facts = InternalInferenceTelemetryFacts::from_route(
                                    caller_id.as_str(),
                                    request.consumer_profile_id.as_str(),
                                    request.request_id.as_str(),
                                    execution_id.as_str(),
                                    "internal_llm",
                                    &route,
                                    attempt_started_at_unix_ms,
                                    current_unix_millis()
                                        .saturating_sub(attempt_started_at_unix_ms),
                                    "failed",
                                    &usage,
                                    Some(&normalized_failure),
                                )
                                .with_retry_count(completed_retries)
                                .with_metrics(Some(input_item_count), None, None)
                                .with_first_response(attempt_first_response_at_unix_ms)
                                .with_transport_from(&active_attempt_facts);
                                self.emit_internal_inference_telemetry(failed_facts);
                                attempt_terminal_emitted = true;
                                match self
                                    .wait_for_restricted_llm_retry(
                                        &active_attempt_facts,
                                        failure,
                                        completed_retries,
                                        &mut cancellation,
                                        deadline_unix_ms,
                                    )
                                    .await
                                {
                                    Ok(retry_count) => {
                                        completed_retries = retry_count;
                                        usage = RequestUsage::default();
                                        continue 'attempts;
                                    }
                                    Err(error) => break 'attempts Err(error),
                                }
                            }
                        };
                        // Capture the first-token transition independently of the first semantic response.
                        // 独立于首个语义响应捕获首 Token 状态变化。
                        let token_was_observed = active_attempt_facts.first_token_at_unix_ms.is_some();
                        active_attempt_facts.observe_transport_event(&event);
                        if !token_was_observed && active_attempt_facts.first_token_at_unix_ms.is_some() {
                            self.emit_internal_inference_first_response(&active_attempt_facts);
                        }
                        let public_event = Some(event.clone());
                        if public_event.as_ref().is_some_and(|event| {
                            !matches!(
                                event,
                                AiStreamEvent::TransportObserved { .. }
                                    | AiStreamEvent::ResponseStarted { .. }
                                    | AiStreamEvent::ResponseMetadataUpdated { .. }
                            )
                        }) {
                            retry_unsafe_event_seen = true;
                        }
                        if first_response_at_unix_ms.is_none()
                            && public_event
                                .as_ref()
                                .is_some_and(is_semantic_inference_response)
                        {
                            let timestamp = current_unix_millis();
                            first_response_at_unix_ms = Some(timestamp);
                            attempt_first_response_at_unix_ms = Some(timestamp);
                        }
                        match aggregate_restricted_llm_event(
                            event,
                            &mut output_text,
                            &mut reasoning_text,
                            &mut structured_output,
                            &mut output_items,
                            &mut usage,
                        ) {
                            Ok(Some(termination_reason)) => {
                                // The provider terminal event is deferred until structured output
                                // has passed the host-owned terminal contract check.
                                // 供应商终态事件延后到结构化输出通过宿主持有的终态契约校验后再发布。
                                break 'attempts Ok(termination_reason);
                            }
                            Ok(None) => {
                                if let Some(public_event) = public_event {
                                    emit_restricted_llm_event(
                                        &state,
                                        public_event,
                                        &mut response_started_emitted,
                                    );
                                }
                            }
                            Err(error) => {
                                break 'attempts Err(error);
                            }
                        }
                    }
                }
            }
        };
        let terminal = terminal
            .and_then(|termination_reason| {
                finalize_restricted_structured_output(
                    &request.structured_output,
                    output_text.as_str(),
                    &mut structured_output,
                    request.consumer_profile_id == VMM_CONSUMER_PROFILE_ID,
                )?;
                Ok(termination_reason)
            })
            .map_err(|error| error.with_route(route.clone()));
        let duration_ms = duration_millis(started_at.elapsed());
        let attempt_duration_ms =
            current_unix_millis().saturating_sub(active_attempt_facts.started_at_unix_ms);
        let telemetry = match &terminal {
            Ok(_) => InternalInferenceTelemetryFacts::from_route(
                caller_id.as_str(),
                request.consumer_profile_id.as_str(),
                request.request_id.as_str(),
                execution_id.as_str(),
                "internal_llm",
                &route,
                active_attempt_facts.started_at_unix_ms,
                attempt_duration_ms,
                "succeeded",
                &usage,
                None,
            )
            .with_retry_count(completed_retries)
            .with_metrics(Some(input_item_count), Some(1), None)
            .with_first_response(attempt_first_response_at_unix_ms)
            .with_transport_from(&active_attempt_facts),
            Err(error) => InternalInferenceTelemetryFacts::from_route(
                caller_id.as_str(),
                request.consumer_profile_id.as_str(),
                request.request_id.as_str(),
                execution_id.as_str(),
                "internal_llm",
                &route,
                active_attempt_facts.started_at_unix_ms,
                attempt_duration_ms,
                if error.category == ConfiguredInferenceErrorCategory::Cancelled {
                    "cancelled"
                } else {
                    "failed"
                },
                &usage,
                Some(error),
            )
            .with_retry_count(completed_retries)
            .with_metrics(Some(input_item_count), None, None)
            .with_first_response(attempt_first_response_at_unix_ms)
            .with_transport_from(&active_attempt_facts),
        };
        let mut snapshot = match state.snapshot.lock() {
            Ok(snapshot) => snapshot,
            Err(_) => {
                state.terminal_notify.notify_waiters();
                return;
            }
        };
        match terminal {
            Ok(termination_reason) => {
                emit_restricted_llm_event(
                    &state,
                    AiStreamEvent::ResponseCompleted,
                    &mut response_started_emitted,
                );
                snapshot.phase = ConfiguredLlmExecutionPhase::Completed;
                snapshot.response = Some(ResponsesExecutionResult {
                    request_id: request.request_id,
                    route,
                    output_text,
                    reasoning_text: (!reasoning_text.is_empty()).then_some(reasoning_text),
                    include_reasoning_encrypted_content: request
                        .include_reasoning_encrypted_content,
                    structured_output,
                    output_items,
                    termination_reason,
                    usage,
                    duration_ms,
                });
            }
            Err(error) => {
                snapshot.phase = if error.category == ConfiguredInferenceErrorCategory::Cancelled {
                    ConfiguredLlmExecutionPhase::Cancelled
                } else {
                    ConfiguredLlmExecutionPhase::Failed
                };
                if error.category == ConfiguredInferenceErrorCategory::Cancelled {
                    emit_restricted_llm_event(
                        &state,
                        AiStreamEvent::ResponseCancelled,
                        &mut response_started_emitted,
                    );
                } else {
                    emit_restricted_llm_event(
                        &state,
                        AiStreamEvent::ResponseFailed {
                            error: configured_inference_stream_error(&error),
                        },
                        &mut response_started_emitted,
                    );
                }
                snapshot.error = Some(error);
            }
        }
        drop(snapshot);
        if !attempt_terminal_emitted {
            self.emit_internal_inference_telemetry(telemetry);
        }
        if let Ok(mut terminal_at) = state.terminal_at.lock() {
            *terminal_at = Some(Instant::now());
        }
        state.terminal_notify.notify_waiters();
    }

    /// Removes the in-flight index after an execution becomes terminal.
    /// 在执行进入终态后删除运行中索引。
    ///
    /// # Parameters
    /// 参数
    /// `execution_id` identifies the exact retained execution.
    /// `execution_id` 标识精确保留执行。
    fn release_in_flight_llm_request(&self, execution_id: &str) {
        let Ok(executions) = self.llm_executions.lock() else {
            return;
        };
        let Some(state) = executions.get(execution_id) else {
            return;
        };
        let in_flight_key = state.in_flight_key.clone();
        drop(executions);
        let Ok(mut requests) = self.in_flight_llm_requests.lock() else {
            return;
        };
        if requests.get(in_flight_key.as_str()).map(String::as_str) == Some(execution_id) {
            requests.remove(in_flight_key.as_str());
        }
    }

    /// Removes terminal LLM executions after the bounded polling retention period.
    /// 在有限轮询保留期后删除 LLM 终态执行。
    ///
    /// # Returns
    /// 返回值
    /// Success after pruning, or a controlled lock diagnostic.
    /// 完成清理时返回成功，否则返回受控锁诊断。
    fn prune_terminal_llm_executions(&self) -> Result<(), ConfiguredInferenceError> {
        let now = Instant::now();
        let mut executions = self.llm_executions.lock().map_err(lock_error)?;
        executions.retain(|_, state| {
            let terminal_at = match state.terminal_at.lock() {
                Ok(terminal_at) => *terminal_at,
                Err(_) => return true,
            };
            terminal_at.is_none_or(|terminal_at| {
                now.saturating_duration_since(terminal_at) < LLM_TERMINAL_RETENTION
            })
        });
        Ok(())
    }
}

/// Projects one provider event into the closed external LLM stream contract.
/// 将一项供应商事件投影到封闭的外部 LLM 流契约。
///
/// # Parameters
/// 参数
/// `event` is the already normalized provider event before restricted aggregation.
/// `event` 是受限聚合前已经归一化的供应商事件。
///
/// # Returns
/// 返回值
/// A safe public event for semantic output, usage, and terminal state, or no event
/// for provider metadata and lifecycle-only records.
/// 返回安全的语义输出、用量或终态公共事件；供应商元数据和纯生命周期记录不返回事件。
/// Identifies the first user-consumable semantic response for telemetry timing.
/// 标识用于遥测计时的首个用户可消费语义响应。
///
/// # Parameters
/// 参数
/// `event` is a normalized restricted event that contains no provider-only metadata.
/// `event` 是不含供应商专属元数据的归一化受限事件。
///
/// # Returns
/// 返回值
/// True only for text, reasoning, or structured output content.
/// 仅对文本、推理或结构化输出内容返回真。
fn is_semantic_inference_response(event: &AiStreamEvent) -> bool {
    matches!(
        event,
        AiStreamEvent::ResponseItemStarted { .. }
            | AiStreamEvent::ResponseTextDelta { .. }
            | AiStreamEvent::ReasoningDelta { .. }
            | AiStreamEvent::ToolCallStarted { .. }
            | AiStreamEvent::ToolCallArgumentsDelta { .. }
            | AiStreamEvent::ToolCallCompleted { .. }
            | AiStreamEvent::ImageGenerationPartial { .. }
    )
}

/// Converts one host-controlled configured-inference failure into the shared normalized stream error.
/// 将一项宿主控制的已配置推理失败转换为共享归一化流错误。
fn configured_inference_stream_error(error: &ConfiguredInferenceError) -> AiExecutionError {
    if let Some(provider_error) = error.provider_error.as_deref() {
        return provider_error.clone();
    }
    let category = match error.category {
        ConfiguredInferenceErrorCategory::Contract => AiErrorCategory::InvalidRequest,
        ConfiguredInferenceErrorCategory::Authorization => AiErrorCategory::PermissionDenied,
        ConfiguredInferenceErrorCategory::Configuration
        | ConfiguredInferenceErrorCategory::Execution => AiErrorCategory::Unclassified,
        ConfiguredInferenceErrorCategory::Limit => AiErrorCategory::Timeout,
        ConfiguredInferenceErrorCategory::Cancelled => AiErrorCategory::Cancelled,
    };
    AiExecutionError {
        category,
        subcategory: Some(error.category_label().to_string()),
        stage: String::from("configured_inference"),
        http_status: None,
        transport: None,
        websocket_close_code: None,
        upstream_code: Some(error.category_label().to_string()),
        upstream_type: None,
        upstream_parameter: None,
        upstream_message: None,
        request_id: None,
        provider_id: None,
        entry_id: None,
        model_id: None,
        message: error.message.clone(),
        dispatch_outcome: DispatchOutcome::Unknown,
        semantic_output_observed: false,
        side_effect_observed: false,
        terminal: true,
        retryable: false,
        retry_after: None,
    }
}

/// Appends and broadcasts one normalized Responses event under bounded replay limits.
/// 在有界重放限制下追加并广播一项归一化 Responses 事件。
///
/// # Parameters
/// 参数
/// `state` owns replay and live delivery, while `response_started_emitted` prevents
/// transparent retries from exposing duplicate start records.
/// `state` 拥有重放与实时投递，`response_started_emitted` 防止透明重试暴露重复启动记录。
fn emit_restricted_llm_event(
    state: &ConfiguredLlmExecutionState,
    event: AiStreamEvent,
    response_started_emitted: &mut bool,
) {
    if matches!(&event, AiStreamEvent::ResponseStarted { .. }) {
        if *response_started_emitted {
            return;
        }
        *response_started_emitted = true;
    }
    let event_size = serde_json::to_vec(&event).ok().map(|value| value.len());
    if let (Ok(mut history), Ok(mut history_bytes), Some(event_size)) = (
        state.event_history.lock(),
        state.event_history_bytes.lock(),
        event_size,
    ) {
        if event_size > LLM_EVENT_BUFFER_MAX_BYTES {
            history.clear();
            *history_bytes = 0;
            if let Ok(mut truncated) = state.event_history_truncated.lock() {
                *truncated = true;
            }
        } else {
            while history.len() >= LLM_EVENT_BUFFER_CAPACITY
                || history_bytes.saturating_add(event_size) > LLM_EVENT_BUFFER_MAX_BYTES
            {
                let removed = history.remove(0);
                let removed_size = serde_json::to_vec(&removed)
                    .ok()
                    .map_or(0, |value| value.len());
                *history_bytes = history_bytes.saturating_sub(removed_size);
                if let Ok(mut truncated) = state.event_history_truncated.lock() {
                    *truncated = true;
                }
            }
            history.push(event.clone());
            *history_bytes = history_bytes.saturating_add(event_size);
        }
    } else if let Ok(mut truncated) = state.event_history_truncated.lock() {
        *truncated = true;
    }
    let _ = state.event_sender.send(event);
}

/// Authorizes one operation and rejects an already expired deadline.
/// 授权一项操作并拒绝已过期截止时间。
///
/// # Parameters
/// 参数
/// `caller`, `profile_id`, and `permission` are trusted host-selected facts.
/// `caller`、`profile_id` 与 `permission` 是宿主选择的可信事实。
///
/// # Returns
/// 返回值
/// Success when permission and deadline are valid.
/// 权限与截止时间有效时返回成功。
fn authorize_caller(
    caller: &InferenceCallerContext,
    profile_id: &str,
    permission: InferencePermission,
) -> Result<(), ConfiguredInferenceError> {
    caller
        .authorize(profile_id, permission)
        .map_err(|message| {
            ConfiguredInferenceError::new(ConfiguredInferenceErrorCategory::Authorization, message)
        })?;
    if caller
        .deadline_unix_ms
        .is_some_and(|deadline| deadline <= current_unix_millis())
    {
        return Err(ConfiguredInferenceError::new(
            ConfiguredInferenceErrorCategory::Limit,
            "Inference caller deadline has already expired.",
        ));
    }
    Ok(())
}

/// Builds the host-owned identity used only for provider/model validation accounting.
/// 构建仅用于供应商/模型验证计量的宿主持有身份。
///
/// # Returns
/// 返回值
/// A bounded internal-background caller whose identity becomes the native ledger consumer.
/// 返回一个有界后台调用方，其身份会成为原生账本消费者。
fn provider_model_validation_caller() -> InferenceCallerContext {
    InferenceCallerContext {
        // Model validation keeps its existing scheduling behavior.
        // 模型验证保留既有调度行为。
        scheduling_class: vulcan_ai_contract::InferenceSchedulingClass::ByUsageScope,
        caller_id: PROVIDER_MODEL_VALIDATION_CONSUMER_ID.to_string(),
        allowed_consumer_profile_ids: BTreeSet::from([String::from(
            PROVIDER_MODEL_VALIDATION_CONSUMER_ID,
        )]),
        permissions: BTreeSet::from([InferencePermission::Llm]),
        usage_scope: InferenceUsageScope::InternalBackground,
        interactive_session_id: None,
        deadline_unix_ms: None,
        budget: vulcan_ai_contract::InferenceCallerBudget {
            max_concurrent_executions: 1,
            max_input_units_per_request: Some(16 * 1024),
            max_output_tokens_per_request: Some(256),
            max_requests_per_minute: Some(60),
            max_input_units_per_minute: Some(64 * 1024),
            max_execution_duration_ms: Some(60_000),
        },
    }
}

/// Resolves one exact configured operation route without fallback.
/// 在不回退的情况下解析一条精确已配置操作路由。
///
/// # Parameters
/// 参数
/// `configuration`, `profile_id`, `operation`, and optional `purpose_id` select one route.
/// `configuration`、`profile_id`、`operation` 与可选 `purpose_id` 选择一条路由。
///
/// # Returns
/// 返回值
/// The exact stored route or an explicit unconfigured error.
/// 返回精确存储路由或明确未配置错误。
fn configured_operation_route<'a>(
    configuration: &'a PersistedAiConfiguration,
    profile_id: &str,
    operation: AiOperation,
    purpose_id: Option<&str>,
) -> Result<&'a InferenceRouteConfiguration, ConfiguredInferenceError> {
    let profile = configuration
        .inference_profiles
        .get(profile_id)
        .ok_or_else(|| {
            ConfiguredInferenceError::new(
                ConfiguredInferenceErrorCategory::Configuration,
                format!("Inference consumer profile `{profile_id}` is not configured."),
            )
        })?;
    let route = match operation {
        AiOperation::ConversationRespond => {
            let purpose_id = purpose_id.ok_or_else(|| {
                ConfiguredInferenceError::new(
                    ConfiguredInferenceErrorCategory::Contract,
                    "Restricted LLM inference requires one exact purpose_id.",
                )
            })?;
            profile.llm_routes.get(purpose_id).ok_or_else(|| {
                ConfiguredInferenceError::new(
                    ConfiguredInferenceErrorCategory::Configuration,
                    format!(
                        "Inference consumer profile `{profile_id}` has no LLM route for purpose `{purpose_id}`."
                    ),
                )
            })?
        }
        AiOperation::EmbeddingCreate => profile.embedding.as_ref().ok_or_else(|| {
            ConfiguredInferenceError::new(
                ConfiguredInferenceErrorCategory::Configuration,
                format!("Inference consumer profile `{profile_id}` has no embedding route."),
            )
        })?,
        AiOperation::RerankDocuments => profile.rerank.as_ref().ok_or_else(|| {
            ConfiguredInferenceError::new(
                ConfiguredInferenceErrorCategory::Configuration,
                format!("Inference consumer profile `{profile_id}` has no rerank route."),
            )
        })?,
        _ => {
            return Err(ConfiguredInferenceError::new(
                ConfiguredInferenceErrorCategory::Configuration,
                format!(
                    "Operation `{}` is not part of configured internal inference.",
                    operation.as_str()
                ),
            ));
        }
    };
    if route.operation != operation {
        return Err(ConfiguredInferenceError::new(
            ConfiguredInferenceErrorCategory::Configuration,
            format!(
                "Configured inference route operation `{}` does not match requested operation `{}`.",
                route.operation.as_str(),
                operation.as_str()
            ),
        ));
    }
    Ok(route)
}

/// Validates that one configured route serves the caller's actual usage scope.
/// 校验一条已配置路由服务调用方的实际用途范围。
///
/// # Parameters
/// 参数
/// `route` is persisted configuration and `caller` is host-trusted context.
/// `route` 是持久化配置，`caller` 是宿主可信上下文。
///
/// # Returns
/// 返回值
/// Success only for an exact scope match.
/// 仅在用途范围精确匹配时返回成功。
fn validate_route_scope(
    route: &InferenceRouteConfiguration,
    caller: &InferenceCallerContext,
) -> Result<(), ConfiguredInferenceError> {
    if route.usage_scope != caller.usage_scope {
        return Err(ConfiguredInferenceError::new(
            ConfiguredInferenceErrorCategory::Authorization,
            format!(
                "Configured inference route usage scope `{:?}` does not match caller scope `{:?}`.",
                route.usage_scope, caller.usage_scope
            ),
        ));
    }
    Ok(())
}

/// Validates one caller-supplied structured-output policy before provider dispatch.
/// 在供应商分派前校验一项调用方提供的结构化输出策略。
///
/// # Parameters
/// 参数
/// `policy` is the exact closed policy retained in the restricted inference request.
/// `policy` 是受限推理请求中保留的精确封闭策略。
///
/// # Returns
/// 返回值
/// Success for text, JSON-object, or a compilable Draft 2020-12 JSON Schema policy.
/// 对文本、JSON 对象或可编译的 Draft 2020-12 JSON Schema 策略返回成功。
fn validate_restricted_structured_output_policy(
    policy: &StructuredOutputPolicy,
) -> Result<(), ConfiguredInferenceError> {
    let StructuredOutputPolicy::JsonSchema { name, schema, .. } = policy else {
        return Ok(());
    };
    if name.trim().is_empty() {
        return Err(ConfiguredInferenceError::new(
            ConfiguredInferenceErrorCategory::Contract,
            "Restricted LLM JSON Schema name must not be empty.",
        ));
    }
    jsonschema::draft202012::meta::validate(schema).map_err(|_| {
        ConfiguredInferenceError::new(
            ConfiguredInferenceErrorCategory::Contract,
            "Restricted LLM structured-output schema is not valid Draft 2020-12 JSON Schema.",
        )
    })?;
    jsonschema::draft202012::new(schema).map_err(|_| {
        ConfiguredInferenceError::new(
            ConfiguredInferenceErrorCategory::Contract,
            "Restricted LLM structured-output schema could not be compiled.",
        )
    })?;
    Ok(())
}

/// Finalizes and independently verifies one restricted structured LLM result.
/// 完成并独立校验一项受限结构化 LLM 结果。
///
/// # Parameters
/// 参数
/// `policy` is the accepted request constraint, `output_text` is the accumulated provider text,
/// `structured_output` is the optional provider-native value, and `ignore_additional_properties`
/// enables VMM-only removal of unknown object keys before validation.
/// `policy` 是已接受的请求约束，`output_text` 是累计供应商文本，
/// `structured_output` 是可选的供应商原生结构化值，`ignore_additional_properties`
/// 仅为 VMM 开启校验前的未知对象键移除。
///
/// # Returns
/// 返回值
/// Success after the terminal value is materialized and verified, or a controlled execution error.
/// 终态值完成物化与校验后返回成功，否则返回受控执行错误。
fn finalize_restricted_structured_output(
    policy: &StructuredOutputPolicy,
    output_text: &str,
    structured_output: &mut Option<serde_json::Value>,
    ignore_additional_properties: bool,
) -> Result<(), ConfiguredInferenceError> {
    match policy {
        StructuredOutputPolicy::None => {
            if structured_output.is_some() {
                return Err(ConfiguredInferenceError::new(
                    ConfiguredInferenceErrorCategory::Execution,
                    "Restricted text LLM response emitted unexpected structured output.",
                ));
            }
            Ok(())
        }
        StructuredOutputPolicy::JsonObject => {
            let value = materialize_restricted_structured_output(output_text, structured_output)?;
            if !value.is_object() {
                return Err(ConfiguredInferenceError::new(
                    ConfiguredInferenceErrorCategory::Execution,
                    "Restricted LLM response did not produce the requested JSON object.",
                ));
            }
            Ok(())
        }
        StructuredOutputPolicy::JsonSchema { schema, .. } => {
            let value = materialize_restricted_structured_output(output_text, structured_output)?;
            let validator = jsonschema::draft202012::new(schema).map_err(|_| {
                ConfiguredInferenceError::new(
                    ConfiguredInferenceErrorCategory::Contract,
                    "Restricted LLM structured-output schema could not be compiled.",
                )
            })?;
            // VMM accepts provider-added object keys only after removing them from the result
            // that its deny-unknown-fields decoder will consume. All other Schema rules remain active.
            // VMM 仅在从交给拒绝未知字段的解码器的结果中移除模型附加键后接纳它们；
            // 其他 Schema 规则继续生效。
            if ignore_additional_properties {
                discard_additional_object_fields(schema, value);
            }
            if let Err(error) = validator.validate(value) {
                return Err(ConfiguredInferenceError::new(
                    ConfiguredInferenceErrorCategory::Execution,
                    format!(
                        "Restricted LLM response did not satisfy the requested JSON Schema at `{}`: {}.",
                        error.instance_path(),
                        error
                    ),
                ));
            }
            Ok(())
        }
    }
}

/// Removes only unknown object keys from a VMM result according to its projected Core schema.
/// 仅按 Core 投影的 Schema 移除 VMM 结果中的未知对象键。
/// `schema` is the authoritative object/array/nullable projection; `value` is the parsed result.
/// `schema` 是权威对象、数组与可空投影；`value` 是已解析结果。
fn discard_additional_object_fields(schema: &serde_json::Value, value: &mut serde_json::Value) {
    // Core projects Nullable as exactly a value branch and a null branch.
    // Core 将 Nullable 精确投影为值分支与 null 分支。
    if let Some(branches) = schema.get("anyOf").and_then(serde_json::Value::as_array)
        && branches.len() == 2
        && branches
            .iter()
            .any(|branch| branch.get("type").and_then(serde_json::Value::as_str) == Some("null"))
        && !value.is_null()
    {
        if let Some(branch) = branches
            .iter()
            .find(|branch| branch.get("type").and_then(serde_json::Value::as_str) != Some("null"))
        {
            discard_additional_object_fields(branch, value);
        }
        return;
    }
    if let Some(object) = value.as_object_mut() {
        // Only producer-declared properties may survive a closed-object projection.
        // 封闭对象投影仅允许保留产出方声明的属性。
        let properties = schema
            .get("properties")
            .and_then(serde_json::Value::as_object);
        if schema.get("additionalProperties") == Some(&serde_json::Value::Bool(false))
            && let Some(properties) = properties
        {
            object.retain(|name, _| properties.contains_key(name));
        }
        if let Some(properties) = properties {
            for (name, child) in object {
                if let Some(child_schema) = properties.get(name) {
                    discard_additional_object_fields(child_schema, child);
                }
            }
        }
    } else if let Some(items) = schema.get("items")
        && let Some(array) = value.as_array_mut()
    {
        for child in array {
            discard_additional_object_fields(items, child);
        }
    }
}

/// Materializes one provider-native or text-encoded JSON terminal value.
/// 物化一个供应商原生或文本编码的 JSON 终态值。
///
/// # Parameters
/// 参数
/// `output_text` is used only when the provider did not emit a native structured item;
/// `structured_output` receives the parsed value so all callers observe one result shape.
/// 仅当供应商未发出原生结构化项时使用 `output_text`；
/// `structured_output` 接收解析值，使所有调用方观察到同一种结果形态。
///
/// # Returns
/// 返回值
/// A mutable borrowed parsed JSON value or a controlled execution error.
/// 返回可变借用的已解析 JSON 值或受控执行错误。
fn materialize_restricted_structured_output<'a>(
    output_text: &str,
    structured_output: &'a mut Option<serde_json::Value>,
) -> Result<&'a mut serde_json::Value, ConfiguredInferenceError> {
    if structured_output.is_none() {
        let value = parse_restricted_json_text(output_text).ok_or_else(|| {
            ConfiguredInferenceError::new(
                ConfiguredInferenceErrorCategory::Execution,
                "Restricted LLM response was not valid JSON.",
            )
        })?;
        *structured_output = Some(value);
    }
    structured_output.as_mut().ok_or_else(|| {
        ConfiguredInferenceError::new(
            ConfiguredInferenceErrorCategory::Execution,
            "Restricted LLM response did not contain structured output.",
        )
    })
}

/// Parses one complete JSON response, fenced document, or uniquely embedded JSON value.
/// 解析一份完整 JSON 响应、代码围栏文档或唯一嵌入的 JSON 值。
///
/// # Parameters
/// 参数
/// `output_text` is the provider text accumulated for a restricted structured-output request.
/// `output_text` 是受限结构化输出请求累计的供应商文本。
///
/// # Returns
/// 返回值
/// The parsed JSON value only when exactly one complete object or array can be identified.
/// 仅当能够识别唯一完整对象或数组时返回解析值。
fn parse_restricted_json_text(output_text: &str) -> Option<serde_json::Value> {
    let trimmed = output_text.trim();
    serde_json::from_str(trimmed)
        .ok()
        .or_else(|| {
            let lines = trimmed.lines().collect::<Vec<_>>();
            if lines.len() < 3 {
                return None;
            }
            let opening = lines.first()?.trim().to_ascii_lowercase();
            if opening != "```" && opening != "```json" {
                return None;
            }
            if lines.last()?.trim() != "```" {
                return None;
            }
            let body = lines[1..lines.len() - 1].join("\n");
            serde_json::from_str(body.trim()).ok()
        })
        .or_else(|| first_unique_embedded_json_value(trimmed))
}

/// Extracts one unique complete top-level JSON value from surrounding provider text.
/// 从供应商前后说明文字中提取唯一一份完整的顶层 JSON 值。
fn first_unique_embedded_json_value(text: &str) -> Option<serde_json::Value> {
    let bytes = text.as_bytes();
    let mut start = 0_usize;
    while start < bytes.len() {
        if (bytes[start] == b'{' || bytes[start] == b'[')
            && let Some((value, end)) = complete_json_value_at(text, start)
        {
            if first_complete_json_value(text.get(end.saturating_add(1)..)?).is_some() {
                return None;
            }
            return Some(value);
        }
        start = start.saturating_add(1);
    }
    None
}

/// Finds one balanced JSON object or array while respecting strings and escapes.
/// 在尊重字符串与转义的前提下查找一份平衡的 JSON 对象或数组。
fn complete_json_value_at(text: &str, start: usize) -> Option<(serde_json::Value, usize)> {
    let bytes = text.as_bytes();
    let mut stack = Vec::new();
    let mut in_string = false;
    let mut escaped = false;
    let mut index = start;
    while index < bytes.len() {
        let byte = bytes[index];
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            index = index.saturating_add(1);
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' => stack.push(b'}'),
            b'[' => stack.push(b']'),
            b'}' | b']' => {
                if stack.pop() != Some(byte) {
                    return None;
                }
                if stack.is_empty() {
                    let value = serde_json::from_str(&text[start..=index]).ok()?;
                    return Some((value, index));
                }
            }
            _ => {}
        }
        index = index.saturating_add(1);
    }
    None
}

/// Finds whether the trailing text contains another complete JSON object or array.
/// 查找尾部文本中是否还存在另一份完整 JSON 对象或数组。
fn first_complete_json_value(text: &str) -> Option<serde_json::Value> {
    let bytes = text.as_bytes();
    let mut start = 0_usize;
    while start < bytes.len() {
        if (bytes[start] == b'{' || bytes[start] == b'[')
            && let Some((value, _)) = complete_json_value_at(text, start)
        {
            return Some(value);
        }
        start = start.saturating_add(1);
    }
    None
}

/// Builds one restricted provider request from an authorized LLM contract.
/// 根据已授权 LLM 契约构建一项受限供应商请求。
///
/// # Parameters
/// 参数
/// Configuration and selection supply defaults; caller request values may only narrow them.
/// 配置与选择提供默认值；调用方请求值只能收窄这些默认值。
///
/// # Returns
/// 返回值
/// A provider-neutral Responses request that satisfies the frozen model capability contract.
/// 返回满足冻结模型能力契约的供应商无关 Responses 请求。
fn responses_provider_request(
    configuration: &PersistedAiConfiguration,
    selection: &ResolvedPublicAiSelection,
    request: &ResponsesExecutionRequest,
    caller: &InferenceCallerContext,
) -> Result<AiExecutionRequest, ConfiguredInferenceError> {
    let internal_overrides =
        internal_model_overrides(configuration, selection, AiOperation::ConversationRespond);
    let output_token_limit = narrow_output_limit(
        request.output_token_limit,
        [
            selection.model.capabilities.output_limit,
            selection.overrides.output_limit,
            internal_overrides.and_then(|overrides| overrides.output_limit),
            caller.budget.max_output_tokens_per_request,
        ],
    );
    let prompt_cache = if request.prompt_cache == Default::default() {
        selection.overrides.prompt_cache.clone().unwrap_or_default()
    } else {
        request.prompt_cache.clone()
    };
    let mut model_parameters = selection.overrides.model_parameters.clone();
    if let Some(overrides) = internal_overrides {
        model_parameters.extend(overrides.model_parameters.clone());
    }
    model_parameters.extend(request.model_parameters.clone());
    // Resolve the same host-default capability contract used by ordinary conversation dispatch.
    // 解析与普通会话分发使用的同一宿主默认能力契约。
    let mut effective_capabilities = selection.model.capabilities.clone();
    vulcan_provider_adapters::apply_host_default_reasoning_policy(&mut effective_capabilities);
    validate_responses_model_capabilities(request, &effective_capabilities)?;
    validate_restricted_structured_output_route(
        selection.route.protocol_route.protocol_profile_id.as_str(),
        &request.structured_output,
        &effective_capabilities,
    )?;
    validate_prompt_cache_policy(&effective_capabilities, &prompt_cache)?;
    let requested_reasoning_effort = request
        .reasoning_effort
        .or_else(|| {
            configured_purpose_reasoning_effort(
                configuration,
                request.consumer_profile_id.as_str(),
                request.purpose_id.as_str(),
            )
        })
        .or_else(|| internal_overrides.and_then(|overrides| overrides.reasoning_effort));
    let reasoning_effort = apply_restricted_reasoning_policy(
        request.consumer_profile_id.as_str(),
        request.purpose_id.as_str(),
        &selection.model.capabilities,
        &selection
            .route
            .model_parameters_conflicting_with_request_field("reasoning_effort"),
        &mut model_parameters,
        requested_reasoning_effort,
    )?;
    // A missing restricted-request preference means provider default, which must still resolve to
    // an enabled host default for ordinary text models; explicit `none` remains authoritative.
    // 受限请求缺少思考偏好表示使用供应商默认，但普通文本模型仍必须解析为宿主启用默认；
    // 显式 `none` 仍然具有最高优先级。
    let reasoning_effort = reasoning_effort
        .or_else(|| resolve_effective_public_reasoning_effort(&effective_capabilities, None));
    validate_reasoning_effort(
        effective_capabilities.reasoning_control,
        effective_capabilities.reasoning_efforts.as_slice(),
        reasoning_effort,
    )?;
    let messages = request.messages.clone();
    let structured_output = structured_output_policy_for_protocol(
        selection.route.protocol_route.protocol_profile_id.as_str(),
        &request.structured_output,
    );
    Ok(AiExecutionRequest {
        operation: AiOperation::ConversationRespond,
        model_id: selection.model.id.clone(),
        cache_scope_id: Some(responses_cache_scope_id(caller, request)),
        continuation: None,
        messages,
        tools: request.tools.clone(),
        native_tools: request.native_tools.clone(),
        tool_choice: request.tool_choice.clone(),
        parallel_tool_calls: request.parallel_tool_calls,
        structured_output,
        stop_sequences: request.stop_sequences.clone(),
        prompt_cache,
        reasoning_effort,
        reasoning_output: request.reasoning_output,
        model_parameters,
        resources: request.resources.clone(),
        output_token_limit,
    })
}

/// Builds one caller-isolated provider cache scope for a Responses execution.
/// 为一项 Responses 执行构建调用方隔离的供应商缓存作用域。
///
/// # Parameters
/// 参数
/// `caller` is trusted host identity and `request` carries the optional untrusted public key.
/// `caller` 是可信宿主身份，`request` 承载可选的不可信公共缓存键。
///
/// # Returns
/// 返回值
/// A bounded opaque cache identity that cannot collide across callers or expose caller text upstream.
/// 返回不会跨调用方碰撞且不会向上游暴露调用方文本的有界不透明缓存身份。
fn responses_cache_scope_id(
    caller: &InferenceCallerContext,
    request: &ResponsesExecutionRequest,
) -> String {
    let caller_key = request.prompt_cache_key.as_deref().unwrap_or("default");
    let mut hasher = Sha256::new();
    for value in [
        caller.caller_id.as_str(),
        request.consumer_profile_id.as_str(),
        request.purpose_id.as_str(),
        caller_key,
    ] {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }
    format!("vulcan-responses:sha256:{}", hex::encode(hasher.finalize()))
}

/// Validates one Responses execution request against the exact selected-model capabilities.
/// 根据所选模型的精确能力校验一项 Responses 执行请求。
///
/// # Parameters
/// 参数
/// `request` contains caller-visible tools and resources; `capabilities` is the frozen route
/// snapshot resolved for this execution.
/// `request` 包含调用方可见工具与资源；`capabilities` 是本次执行解析出的冻结路由快照。
///
/// # Returns
/// 返回值
/// Success only when every requested optional semantic is explicitly supported by the route.
/// 仅当路由显式支持每项请求的可选语义时返回成功。
fn validate_responses_model_capabilities(
    request: &ResponsesExecutionRequest,
    capabilities: &ModelCapabilities,
) -> Result<(), ConfiguredInferenceError> {
    let mut tool_names = BTreeSet::new();
    for tool in &request.tools {
        if !tool_names.insert(tool.name.as_str()) {
            return Err(ConfiguredInferenceError::new(
                ConfiguredInferenceErrorCategory::Contract,
                format!("Responses tool name `{}` is duplicated.", tool.name),
            ));
        }
    }

    if let ToolChoicePolicy::Specific { tool_name } = &request.tool_choice
        && !tool_names.contains(tool_name.as_str())
    {
        return Err(ConfiguredInferenceError::new(
            ConfiguredInferenceErrorCategory::Contract,
            format!("Responses tool_choice references unknown function `{tool_name}`."),
        ));
    }

    let mut native_tools = Vec::new();
    for native_tool in request.native_tools.iter().copied() {
        if native_tools.contains(&native_tool) {
            return Err(ConfiguredInferenceError::new(
                ConfiguredInferenceErrorCategory::Contract,
                format!("Responses native tool `{native_tool:?}` is duplicated."),
            ));
        }
        native_tools.push(native_tool);
        if !capabilities.provider_native_tools.contains(&native_tool) {
            return Err(ConfiguredInferenceError::new(
                ConfiguredInferenceErrorCategory::Contract,
                format!("The selected model route does not declare native tool `{native_tool:?}`."),
            ));
        }
    }

    if request
        .native_tools
        .contains(&NativeToolKind::ImageGeneration)
        && !capabilities.output_resources.contains(&ResourceKind::Image)
    {
        return Err(ConfiguredInferenceError::new(
            ConfiguredInferenceErrorCategory::Contract,
            "Image generation requires an explicitly declared image output capability.",
        ));
    }

    for resource in request
        .resources
        .iter()
        .chain(request.messages.iter().flat_map(|message| {
            message.content.iter().filter_map(|content| match content {
                ContentItem::Resource { resource } => Some(resource),
                _ => None,
            })
        }))
    {
        if !capabilities.input_resources.contains(&resource.kind) {
            return Err(ConfiguredInferenceError::new(
                ConfiguredInferenceErrorCategory::Contract,
                format!(
                    "The selected model route does not declare `{}` input resources.",
                    resource_kind_label(resource.kind)
                ),
            ));
        }
    }
    Ok(())
}

/// Validates the resolved prompt-cache policy against the frozen selected-model capability.
/// 根据冻结的所选模型能力校验解析后的提示词缓存策略。
///
/// # Parameters
/// 参数
/// `capabilities` is the exact frozen model capability snapshot and `policy` is the effective
/// policy after applying caller and configured overrides.
/// `capabilities` 是精确冻结的模型能力快照，`policy` 是应用调用方与配置覆盖后的有效策略。
///
/// # Returns
/// 返回值
/// Success only when the selected route explicitly supports the resolved policy.
/// 仅当所选路由显式支持解析后的策略时返回成功。
fn validate_prompt_cache_policy(
    capabilities: &ModelCapabilities,
    policy: &PromptCachePolicy,
) -> Result<(), ConfiguredInferenceError> {
    capabilities
        .prompt_cache
        .validate_policy(policy)
        .map_err(|message| {
            ConfiguredInferenceError::new(ConfiguredInferenceErrorCategory::Contract, message)
        })
}

/// Returns the stable public label for one resource kind.
/// 返回一个资源类别的稳定公共标签。
///
/// # Parameters
/// 参数
/// `kind` is the closed resource kind from the AI contract.
/// `kind` 是 AI 契约中的封闭资源类别。
///
/// # Returns
/// 返回值
/// The snake-case label used in controlled contract diagnostics.
/// 返回受控契约诊断使用的蛇形标签。
fn resource_kind_label(kind: ResourceKind) -> &'static str {
    match kind {
        ResourceKind::Image => "image",
        ResourceKind::Audio => "audio",
        ResourceKind::Video => "video",
        ResourceKind::File => "file",
    }
}

/// Selects the provider-wire structured-output projection while retaining host-side validation.
/// 选择供应商 Wire 结构化输出投影，同时保留宿主侧的结果校验。
///
/// # Parameters
/// 参数
/// `protocol_id` is the exact frozen protocol profile and `policy` is the caller-owned JSON contract.
/// `protocol_id` 是冻结的精确协议 Profile，`policy` 是调用方拥有的 JSON 契约。
///
/// # Returns
/// 返回值
/// Anthropic keeps the exact JSON Schema contract. Protocols whose provider APIs expose a
/// JSON-object switch receive `JsonObject`, while the shared host finalizer still enforces the
/// caller-owned schema after decoding. Unknown profiles receive no native projection.
/// Anthropic 保留准确 JSON Schema 契约；提供方 API 暴露 JSON-object 开关的协议接收
/// `JsonObject`，共享宿主终结器仍在解码后强制校验调用方拥有的 Schema；未知协议不投影。
fn structured_output_policy_for_protocol(
    protocol_id: &str,
    policy: &StructuredOutputPolicy,
) -> StructuredOutputPolicy {
    if matches!(policy, StructuredOutputPolicy::None) {
        return StructuredOutputPolicy::None;
    }
    if matches!(
        protocol_id,
        ANTHROPIC_MESSAGES_PROFILE_ID
            | ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID
            | CLAUDE_CODE_MESSAGES_PROFILE_ID
    ) {
        return policy.clone();
    }
    if matches!(
        protocol_id,
        OPENAI_CHAT_PROFILE_ID
            | OPENAI_RESPONSES_PROFILE_ID
            | OPENAI_CODEX_RESPONSES_PROFILE_ID
            | GEMINI_GENERATE_CONTENT_PROFILE_ID
            | GOOGLE_CODE_ASSIST_PROFILE_ID
            | GEMINI_INTERACTIONS_PROFILE_ID
            | COHERE_CHAT_PROFILE_ID
    ) {
        return StructuredOutputPolicy::JsonObject;
    }
    StructuredOutputPolicy::None
}

/// Verifies that one exact model and protocol route can carry the requested structured contract.
/// 验证一条精确模型与协议路由能够承载请求的结构化契约。
///
/// # Parameters
/// 参数
/// `protocol_id` selects the frozen encoder, `policy` is the caller contract, and `capabilities`
/// is the exact selected-model declaration.
/// `protocol_id` 选择冻结编码器，`policy` 是调用方契约，`capabilities` 是所选模型的准确声明。
///
/// # Returns
/// 返回值
/// Success only when both the model declaration and protocol encoder preserve structured output.
/// 仅当模型声明与协议编码器都能保留结构化输出时返回成功。
fn validate_restricted_structured_output_route(
    protocol_id: &str,
    policy: &StructuredOutputPolicy,
    capabilities: &ModelCapabilities,
) -> Result<(), ConfiguredInferenceError> {
    let requested = match policy {
        StructuredOutputPolicy::None => return Ok(()),
        StructuredOutputPolicy::JsonObject => StructuredOutputCapability::JsonObject,
        StructuredOutputPolicy::JsonSchema { .. } => StructuredOutputCapability::JsonSchema,
    };
    if model_route_supports_structured_output(protocol_id, requested, capabilities) {
        return Ok(());
    }
    Err(ConfiguredInferenceError::new(
        ConfiguredInferenceErrorCategory::Contract,
        "The selected model and protocol route cannot preserve the requested structured-output contract.",
    ))
}

/// Reports whether one exact route can preserve a caller-owned JSON Schema contract.
/// 判断一条精确路由能否保留调用方拥有的 JSON Schema 契约。
///
/// # Parameters
/// 参数
/// `protocol_id` identifies the frozen encoder and `capabilities` is the selected model contract.
/// `protocol_id` 标识冻结编码器，`capabilities` 是所选模型契约。
///
/// # Returns
/// 返回值
/// `true` when provider-side JSON control remains active and host-side Schema validation is valid.
/// 当提供方 JSON 控制保持启用且宿主侧 Schema 校验有效时返回 `true`。
pub(crate) fn model_route_supports_json_schema_contract(
    protocol_id: &str,
    capabilities: &ModelCapabilities,
) -> bool {
    model_route_supports_structured_output(
        protocol_id,
        StructuredOutputCapability::JsonSchema,
        capabilities,
    )
}

/// Intersects a caller structured-output requirement with model and protocol capabilities.
/// 将调用方结构化输出要求与模型及协议能力取交集。
///
/// # Parameters
/// 参数
/// `protocol_id`, `requested`, and `capabilities` are the three frozen authorities for one route.
/// `protocol_id`、`requested` 与 `capabilities` 是一条路由的三项冻结权威。
///
/// # Returns
/// 返回值
/// `true` only when the provider wire can enforce JSON and the model declares sufficient support.
/// 仅当提供方 Wire 可强制 JSON 且模型声明具备足够支持时返回 `true`。
fn model_route_supports_structured_output(
    protocol_id: &str,
    requested: StructuredOutputCapability,
    capabilities: &ModelCapabilities,
) -> bool {
    let provider_requirement = if matches!(
        protocol_id,
        ANTHROPIC_MESSAGES_PROFILE_ID
            | ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID
            | CLAUDE_CODE_MESSAGES_PROFILE_ID
    ) {
        requested
    } else if matches!(
        protocol_id,
        OPENAI_CHAT_PROFILE_ID
            | OPENAI_RESPONSES_PROFILE_ID
            | OPENAI_CODEX_RESPONSES_PROFILE_ID
            | GEMINI_GENERATE_CONTENT_PROFILE_ID
            | GOOGLE_CODE_ASSIST_PROFILE_ID
            | GEMINI_INTERACTIONS_PROFILE_ID
            | COHERE_CHAT_PROFILE_ID
    ) {
        StructuredOutputCapability::JsonObject
    } else {
        return false;
    };
    if !protocol_profile_supports_structured_output(protocol_id, provider_requirement) {
        return false;
    }
    match provider_requirement {
        StructuredOutputCapability::JsonObject => {
            capabilities.structured_output != StructuredOutputCapability::Unsupported
        }
        StructuredOutputCapability::JsonSchema => {
            capabilities.structured_output == StructuredOutputCapability::JsonSchema
        }
        StructuredOutputCapability::Unsupported => false,
    }
}

/// Applies the shared restricted-request reasoning policy and removes conflicting provider controls.
/// 应用共享受限请求思考策略，并移除相互冲突的供应商控制参数。
///
/// # Parameters
/// 参数
/// `consumer_profile_id` and `purpose_id` identify the configured-inference slot; `capabilities` describes
/// the selected model; `conflicting_model_parameter_ids` comes from the frozen route's generic
/// manifest guards; `model_parameters` contains merged provider parameters; and
/// `requested_effort` is the caller or configured override.
/// `consumer_profile_id` 与 `purpose_id` 标识已配置推理槽位；`capabilities` 描述所选模型；
/// `conflicting_model_parameter_ids` 来自冻结路由的通用 Manifest 守卫；
/// `model_parameters` 包含合并后的供应商参数；`requested_effort` 是调用方或配置覆盖值。
///
/// # Returns
/// 返回值
/// The explicit caller selection for ordinary consumers, forced disabled reasoning for VMM, or a
/// controlled contract error. Any explicit effective selection owns the request field and removes
/// every Manifest-declared conflicting provider parameter before validation.
/// 普通消费者返回调用方显式选择，VMM 返回强制关闭思考，或返回受控契约错误。任何显式有效选择
/// 都拥有请求字段，并在校验前移除 Manifest 声明的全部冲突型供应商参数。
fn apply_restricted_reasoning_policy(
    consumer_profile_id: &str,
    purpose_id: &str,
    capabilities: &ModelCapabilities,
    conflicting_model_parameter_ids: &BTreeSet<String>,
    model_parameters: &mut BTreeMap<String, ModelParameterValue>,
    requested_effort: Option<ReasoningEffort>,
) -> Result<Option<ReasoningEffort>, ConfiguredInferenceError> {
    let requires_disabled_reasoning =
        vmm_purpose_requires_disabled_reasoning(consumer_profile_id, purpose_id);
    if requires_disabled_reasoning && !model_supports_disabled_reasoning(capabilities) {
        return Err(ConfiguredInferenceError::new(
            ConfiguredInferenceErrorCategory::Contract,
            "VMM inference requires a model that explicitly supports disabled reasoning.",
        ));
    }
    let effective_effort = if requires_disabled_reasoning {
        Some(ReasoningEffort::None)
    } else {
        requested_effort
    };
    if effective_effort.is_some() {
        // The frozen manifest route declares every model parameter that conflicts with the
        // host-owned reasoning field; remove exactly those declarations before applying the
        // purpose-scoped selection.
        // 冻结 Manifest 路由声明与宿主持有思考字段冲突的每个模型参数；在应用用途级选择前
        // 仅移除这些声明。
        for parameter_id in conflicting_model_parameter_ids {
            model_parameters.remove(parameter_id);
        }
    }
    Ok(effective_effort)
}

/// Resolves a purpose-specific configured reasoning effort with backward-compatible VMM defaults.
/// 使用向后兼容的 VMM 默认值解析用途专属已配置思考档位。
///
/// # Parameters
/// 参数
/// `configuration`, `consumer_profile_id`, and `purpose_id` identify one restricted LLM slot.
/// `configuration`、`consumer_profile_id` 与 `purpose_id` 标识一个受限 LLM 槽位。
///
/// # Returns
/// 返回值
/// The persisted effort, or explicit disabled reasoning for an older VMM profile without the field.
/// 返回已持久化档位；旧 VMM 配置档缺少该字段时返回显式关闭思考。
fn configured_purpose_reasoning_effort(
    configuration: &PersistedAiConfiguration,
    consumer_profile_id: &str,
    purpose_id: &str,
) -> Option<ReasoningEffort> {
    configuration
        .inference_profiles
        .get(consumer_profile_id)
        .and_then(|profile| profile.llm_reasoning_efforts.get(purpose_id))
        .copied()
        .or_else(|| {
            (consumer_profile_id == VMM_CONSUMER_PROFILE_ID).then_some(ReasoningEffort::None)
        })
}

/// Reports whether one configured-inference slot must run with reasoning disabled.
/// 判断一个已配置推理槽位是否必须关闭思考运行。
///
/// # Parameters
/// 参数
/// `consumer_profile_id` and `purpose_id` identify one exact restricted LLM slot.
/// `consumer_profile_id` 与 `purpose_id` 标识一个准确受限 LLM 槽位。
///
/// # Returns
/// 返回值
/// `true` for every VMM LLM purpose because native structured output uses the proven no-reasoning route.
/// 对每个 VMM LLM 用途返回 `true`，因为原生结构化输出使用已验证的无思考路线。
pub(crate) fn vmm_purpose_requires_disabled_reasoning(
    consumer_profile_id: &str,
    _purpose_id: &str,
) -> bool {
    consumer_profile_id == VMM_CONSUMER_PROFILE_ID
}

/// Validates one explicit reasoning selection against the frozen model contract.
/// 根据冻结模型契约校验一项显式推理选择。
///
/// # Parameters
/// 参数
/// `control` declares fixed or selectable behavior, `supported` is the exact closed
/// effort set, and `requested` is the effective caller or internal override.
/// `control` 声明固定或可选行为，`supported` 是精确封闭档位集合，
/// `requested` 是调用方或内部覆盖后的有效选择。
///
/// # Returns
/// 返回值
/// Success only when the request can be represented by the selected model route.
/// 仅当所选模型路由能够表达该请求时返回成功。
fn validate_reasoning_effort(
    control: ReasoningControlMode,
    supported: &[ReasoningEffort],
    requested: Option<ReasoningEffort>,
) -> Result<(), ConfiguredInferenceError> {
    let Some(requested) = requested else {
        return Ok(());
    };
    let valid = match control {
        ReasoningControlMode::FixedOff => requested == ReasoningEffort::None,
        ReasoningControlMode::FixedOn => {
            requested != ReasoningEffort::None && supported.contains(&requested)
        }
        ReasoningControlMode::Toggle
        | ReasoningControlMode::EffortSelection
        | ReasoningControlMode::ProviderAdaptive => supported.contains(&requested),
        ReasoningControlMode::Unknown => false,
    };
    if valid {
        return Ok(());
    }
    Err(ConfiguredInferenceError::new(
        ConfiguredInferenceErrorCategory::Contract,
        format!(
            "Reasoning effort `{}` is not declared by the selected model route with control mode `{:?}`.",
            requested.as_str(),
            control
        ),
    ))
}

/// Reports whether one exact model contract can execute without reasoning.
/// 判断一份精确模型契约能否在关闭思考的状态下执行。
///
/// # Parameters
/// 参数
/// `capabilities` is the credential-scoped model capability snapshot.
/// `capabilities` 是凭据作用域的模型能力快照。
///
/// # Returns
/// 返回值
/// `true` only for fixed-off models or adjustable contracts that explicitly contain `none`.
/// 仅对固定关闭模型或显式包含 `none` 的可调契约返回 `true`。
pub(crate) fn model_supports_disabled_reasoning(capabilities: &ModelCapabilities) -> bool {
    match capabilities.reasoning_control {
        ReasoningControlMode::FixedOff => true,
        ReasoningControlMode::Toggle
        | ReasoningControlMode::EffortSelection
        | ReasoningControlMode::ProviderAdaptive => capabilities
            .reasoning_efforts
            .contains(&ReasoningEffort::None),
        ReasoningControlMode::FixedOn | ReasoningControlMode::Unknown => false,
    }
}

/// Returns operation-scoped internal overrides for one frozen selection.
/// 返回一项冻结选择的操作作用域内部覆盖。
///
/// # Parameters
/// 参数
/// `configuration`, `selection`, and `operation` identify one exact override key.
/// `configuration`、`selection` 与 `operation` 标识一个精确覆盖键。
///
/// # Returns
/// 返回值
/// The stored override when configured.
/// 配置时返回已存储覆盖。
fn internal_model_overrides<'a>(
    configuration: &'a PersistedAiConfiguration,
    selection: &ResolvedPublicAiSelection,
    operation: AiOperation,
) -> Option<&'a InternalModelOverrides> {
    configuration.internal_model_overrides.get(
        PersistedAiConfiguration::internal_model_operation_key(
            selection.route.provider.id.as_str(),
            selection.model.id.as_str(),
            operation,
        )
        .as_str(),
    )
}

/// Applies host-owned embedding defaults without changing caller identity or input ordering.
/// 应用宿主持有的向量默认值且不改变调用方身份或输入顺序。
///
/// # Parameters
/// 参数
/// `configuration`, `selection`, and `request` provide the exact override scope and request.
/// `configuration`、`selection` 与 `request` 提供精确覆盖作用域与请求。
///
/// # Returns
/// 返回值
/// A request whose configurable route defaults have been applied.
/// 返回已应用可配置路由默认值的请求。
fn embedding_request_with_internal_defaults(
    configuration: &PersistedAiConfiguration,
    selection: &ResolvedPublicAiSelection,
    mut request: EmbeddingRequest,
) -> EmbeddingRequest {
    // Preserve admitted request dimensions before consulting this consumer's optional choice.
    // 优先保留已准入请求的维度，再读取该消费者自己的可选维度选择。
    request.dimensions = consumer_embedding_request_dimensions(
        configuration,
        &request.consumer_profile_id,
        request.dimensions,
    );
    let Some(overrides) =
        internal_model_overrides(configuration, selection, AiOperation::EmbeddingCreate)
    else {
        return request;
    };
    if let Some(task) = overrides.embedding_task {
        request.task = task;
    }
    if request.dimensions.is_none() {
        request.dimensions = overrides.embedding_dimensions;
    }
    if let Some(output_kind) = overrides.embedding_output_kind {
        request.output_kind = output_kind;
    }
    if let Some(encoding) = overrides.embedding_encoding {
        request.encoding = encoding;
    }
    request
}

/// Resolve dimensions from the admitted request and its exact consumer profile.
/// 从已准入请求及其精确消费者配置档解析维度。
///
/// `configuration` owns saved profiles; `consumer_profile_id` selects one profile;
/// `explicit_dimensions` preserves an already frozen request. Returns no value when
/// neither declares dimensions, allowing the existing model-level default policy.
/// `configuration` 持有保存的配置；`consumer_profile_id` 选择唯一配置档；
/// `explicit_dimensions` 保留已冻结请求。两者均未声明时返回空值，沿用模型级默认策略。
fn consumer_embedding_request_dimensions(
    configuration: &PersistedAiConfiguration,
    consumer_profile_id: &str,
    explicit_dimensions: Option<u32>,
) -> Option<u32> {
    explicit_dimensions.or_else(|| {
        configuration
            .inference_profiles
            .get(consumer_profile_id)
            .and_then(|profile| profile.embedding_dimensions)
    })
}

/// Applies host-owned rerank defaults without changing query or candidate identity.
/// 应用宿主持有的重排默认值且不改变查询或候选身份。
///
/// # Parameters
/// 参数
/// `configuration`, `selection`, and `request` provide the exact override scope and request.
/// `configuration`、`selection` 与 `request` 提供精确覆盖作用域与请求。
///
/// # Returns
/// 返回值
/// A request whose configurable route defaults have been applied.
/// 返回已应用可配置路由默认值的请求。
fn rerank_request_with_internal_defaults(
    configuration: &PersistedAiConfiguration,
    selection: &ResolvedPublicAiSelection,
    mut request: RerankRequest,
) -> RerankRequest {
    let Some(overrides) =
        internal_model_overrides(configuration, selection, AiOperation::RerankDocuments)
    else {
        return request;
    };
    if let Some(top_n) = overrides.rerank_top_n {
        request.top_n = top_n;
    }
    if let Some(truncation) = overrides.rerank_truncation {
        request.truncation = truncation;
    }
    if let Some(return_content) = overrides.rerank_return_content {
        request.return_content = return_content;
    }
    request
}

/// Narrows one optional output limit through every configured hard ceiling.
/// 通过全部已配置硬上限收窄一个可选输出限制。
///
/// # Parameters
/// 参数
/// `requested` is caller intent and `ceilings` are trusted host-owned limits.
/// `requested` 是调用方意图，`ceilings` 是宿主拥有的可信限制。
///
/// # Returns
/// 返回值
/// The smallest known positive limit.
/// 返回最小已知正数限制。
fn narrow_output_limit(requested: Option<u64>, ceilings: [Option<u64>; 4]) -> Option<u64> {
    ceilings
        .into_iter()
        .flatten()
        .fold(requested, |current, ceiling| {
            Some(current.map_or(ceiling, |value| value.min(ceiling)))
        })
}

/// Creates one non-secret route snapshot for a restricted LLM selection.
/// 为一项受限 LLM 选择创建不含秘密的路由快照。
///
/// # Parameters
/// 参数
/// `selection` and `usage_scope` are frozen authorization results.
/// `selection` 与 `usage_scope` 是冻结授权结果。
///
/// # Returns
/// 返回值
/// One immutable execution route snapshot.
/// 返回一份不可变执行路由快照。
fn llm_route_snapshot(
    selection: &ResolvedPublicAiSelection,
    usage_scope: vulcan_ai_contract::InferenceUsageScope,
) -> ExecutionRouteSnapshot {
    let (secondary_route_id, secondary_provider_name, secondary_endpoint_name) =
        secondary_route_snapshot(&selection.route.secondary_route);
    ExecutionRouteSnapshot {
        provider_id: selection.route.provider.id.clone(),
        service_entry_id: selection.route.entry.id.clone(),
        connection_id: selection.route.connection.id.clone(),
        model_id: selection.model.id.clone(),
        protocol_binding_id: Some(selection.route.protocol_route.protocol_profile_id.clone()),
        action_binding_id: None,
        secondary_route_id,
        secondary_provider_name,
        secondary_endpoint_name,
        capability_revision: selection.model.capabilities.evidence_revision.clone(),
        catalog_revision: selection.catalog_revision.clone(),
        route_revision: selection.route_revision.clone(),
        usage_scope,
    }
}

/// Creates one route snapshot from an exact configured protocol or action binding.
/// 根据精确已配置协议或动作绑定创建一份路由快照。
///
/// # Parameters
/// 参数
/// `selection` is the resolved catalog snapshot and `route` owns the persisted binding.
/// `selection` 是已解析目录快照，`route` 拥有持久化绑定。
///
/// # Returns
/// 返回值
/// A non-secret route snapshot whose binding identity matches the configured operation.
/// 返回绑定身份与已配置操作一致且不含秘密的路由快照。
pub(crate) fn configured_route_snapshot(
    selection: &ResolvedPublicAiSelection,
    route: &InferenceRouteConfiguration,
) -> ExecutionRouteSnapshot {
    let (protocol_binding_id, action_binding_id) = match &route.binding {
        vulcan_provider_adapters::InferenceRouteBinding::Protocol {
            protocol_profile_id,
        } => (Some(protocol_profile_id.clone()), None),
        vulcan_provider_adapters::InferenceRouteBinding::Action { action_binding_id } => {
            (None, Some(action_binding_id.clone()))
        }
    };
    let (secondary_route_id, secondary_provider_name, secondary_endpoint_name) =
        secondary_route_snapshot(&selection.route.secondary_route);
    ExecutionRouteSnapshot {
        provider_id: selection.route.provider.id.clone(),
        service_entry_id: selection.route.entry.id.clone(),
        connection_id: selection.route.connection.id.clone(),
        model_id: selection.model.id.clone(),
        protocol_binding_id,
        action_binding_id,
        secondary_route_id,
        secondary_provider_name,
        secondary_endpoint_name,
        capability_revision: selection.model.capabilities.evidence_revision.clone(),
        catalog_revision: selection.catalog_revision.clone(),
        route_revision: selection.route_revision.clone(),
        usage_scope: route.usage_scope,
    }
}

/// Validates the immutable shape of one lifecycle task before provider dispatch.
/// 在供应商分派前校验一项生命周期任务的不可变结构。
///
/// # Parameters
/// 参数
/// `task` is the exact durable task selected from the caller-validated frozen plan.
/// `task` 是从调用方已校验冻结计划中选出的精确持久任务。
///
/// # Returns
/// 返回值
/// Unit for a well-formed immutable task, or a contract error before provider I/O.
/// 不可变任务结构正确时返回空值，否则在供应商 I/O 前返回契约错误。
fn validate_embedding_batch_task(
    task: &EmbeddingBatchTaskPlan,
) -> Result<(), ConfiguredInferenceError> {
    task.validate().map_err(|message| {
        ConfiguredInferenceError::new(ConfiguredInferenceErrorCategory::Contract, message)
    })
}

/// Compares a lifecycle call with the opaque request-and-route fingerprint frozen at planning.
/// 将生命周期调用与规划时冻结的不透明请求及路由指纹进行比较。
///
/// # Parameters
/// 参数
/// `current` is reconstructed from one current immutable configuration snapshot and `expected` is durable caller state.
/// `current` 从一份当前不可变配置快照重建，`expected` 是调用方持久状态。
///
/// # Returns
/// 返回值
/// Unit for an exact match, or a contract error before any provider request.
/// 精确匹配时返回空值，否则在任何供应商请求前返回契约错误。
fn validate_embedding_batch_request_fingerprint(
    current: &str,
    expected: &str,
) -> Result<(), ConfiguredInferenceError> {
    if current == expected {
        return Ok(());
    }
    Err(ConfiguredInferenceError::new(
        ConfiguredInferenceErrorCategory::Contract,
        "Embedding Batch lifecycle request does not match its dispatch-frozen route and payload.",
    ))
}

/// Validates one durable provider submission before provider lifecycle I/O.
/// 在供应商生命周期 I/O 前校验一项持久供应商提交。
///
/// # Parameters
/// 参数
/// `submission` carries durable task and provider identities from the caller-validated plan.
/// `submission` 携带来自调用方已校验计划的持久任务及供应商身份。
///
/// # Returns
/// 返回值
/// Unit for a complete durable identity, or a contract error before provider I/O.
/// 持久身份完整时返回空值，否则在供应商 I/O 前返回契约错误。
fn validate_embedding_batch_submission(
    submission: &EmbeddingBatchSubmission,
) -> Result<(), ConfiguredInferenceError> {
    submission.validate().map_err(|message| {
        ConfiguredInferenceError::new(ConfiguredInferenceErrorCategory::Contract, message)
    })
}

/// Projects one frozen provider-internal selection into non-secret route facts.
/// 将一项冻结的供应商内部选择投影为不含秘密的路由事实。
///
/// # Parameters
/// 参数
/// `selection` is the exact validated secondary route selected before dispatch.
/// `selection` 是分派前选择并校验的精确二级路由。
///
/// # Returns
/// 返回值
/// Route identifier and display labels, or three empty values for provider-managed routing.
/// 返回路由标识和显示标签；供应商管理路由时返回三个空值。
fn secondary_route_snapshot(
    selection: &vulcan_provider_adapters::SecondaryRouteSelection,
) -> (Option<String>, Option<String>, Option<String>) {
    match selection {
        vulcan_provider_adapters::SecondaryRouteSelection::Automatic => (None, None, None),
        vulcan_provider_adapters::SecondaryRouteSelection::PinnedEndpoint {
            route_id,
            provider_display_name,
            endpoint_display_name,
        } => (
            Some(route_id.clone()),
            Some(provider_display_name.clone()),
            Some(endpoint_display_name.clone()),
        ),
    }
}

/// Builds one unavailable capability entry without inventing a replacement route.
/// 构建一项不可用能力条目且不虚构替代路由。
///
/// # Parameters
/// 参数
/// `operation`, `purpose_id`, and `reason` preserve the exact failed configuration identity.
/// `operation`、`purpose_id` 与 `reason` 保留精确失败配置身份。
///
/// # Returns
/// 返回值
/// One diagnostic capability entry with no route or model capability.
/// 返回不含路由或模型能力的一项诊断能力条目。
fn unavailable_operation_capability(
    operation: AiOperation,
    purpose_id: Option<String>,
    reason: &str,
) -> InferenceOperationCapability {
    InferenceOperationCapability {
        operation,
        purpose_id,
        available: false,
        unavailable_reason: Some(reason.to_string()),
        route: None,
        embedding: None,
        embedding_batch: None,
        rerank: None,
    }
}

/// Aggregates one restricted provider event and identifies successful termination.
/// 聚合一项受限供应商事件并标识成功终止。
///
/// # Parameters
/// 参数
/// Mutable accumulators retain text, reasoning, structured output, non-text items, and usage.
/// 可变累加器保留文本、推理、结构化输出、非文本项目与用量。
///
/// # Returns
/// 返回值
/// A termination reason, no terminal state, or an explicit restricted-contract failure.
/// 返回终止原因、无终态或明确受限契约失败。
fn aggregate_restricted_llm_event(
    event: AiStreamEvent,
    output_text: &mut String,
    reasoning_text: &mut String,
    structured_output: &mut Option<serde_json::Value>,
    output_items: &mut Vec<ResponsesExecutionOutputItem>,
    usage: &mut RequestUsage,
) -> Result<Option<String>, ConfiguredInferenceError> {
    match event {
        AiStreamEvent::ResponseItemStarted { item_id, item } => match item {
            ContentItem::Text { text } => {
                reject_text_refusal_mix(output_items)?;
                output_text.push_str(text.as_str());
            }
            ContentItem::StructuredJson { value } => {
                if structured_output.replace(value).is_some() {
                    return Err(ConfiguredInferenceError::new(
                        ConfiguredInferenceErrorCategory::Execution,
                        "Restricted LLM stream emitted more than one structured output item.",
                    ));
                }
            }
            ContentItem::Refusal { text } => {
                if !output_text.is_empty() {
                    return Err(ConfiguredInferenceError::new(
                        ConfiguredInferenceErrorCategory::Execution,
                        "Responses execution mixed output text and refusal content.",
                    ));
                }
                output_items.push(ResponsesExecutionOutputItem {
                    item_id,
                    content: ContentItem::Refusal { text },
                });
            }
            ContentItem::Reasoning { text, signature } => {
                reasoning_text.push_str(text.as_str());
                upsert_reasoning_output(output_items, item_id, text, signature);
            }
            ContentItem::Resource { resource } => {
                if resource.kind != ResourceKind::Image {
                    return Err(ConfiguredInferenceError::new(
                        ConfiguredInferenceErrorCategory::Execution,
                        format!(
                            "Responses LLM execution produced unsupported `{:?}` resource output `{}`.",
                            resource.kind, resource.id
                        ),
                    ));
                }
                output_items.push(ResponsesExecutionOutputItem {
                    item_id,
                    content: ContentItem::Resource { resource },
                });
            }
            ContentItem::ToolCall { call } => {
                upsert_tool_call_output(output_items, item_id, call);
            }
            ContentItem::ToolResult { result } => {
                output_items.push(ResponsesExecutionOutputItem {
                    item_id,
                    content: ContentItem::ToolResult { result },
                });
            }
            ContentItem::Citation { title, url, text } => {
                output_items.push(ResponsesExecutionOutputItem {
                    item_id,
                    content: ContentItem::Citation { title, url, text },
                });
            }
        },
        AiStreamEvent::ResponseTextDelta { delta, .. } => {
            reject_text_refusal_mix(output_items)?;
            output_text.push_str(delta.as_str());
        }
        AiStreamEvent::ReasoningDelta { item_id, delta } => {
            reasoning_text.push_str(delta.as_str());
            append_reasoning_output_delta(output_items, item_id, delta);
        }
        AiStreamEvent::UsageUpdated {
            usage: usage_update,
        } => usage.merge_update(*usage_update),
        AiStreamEvent::ResponseCompleted => return Ok(Some(String::from("completed"))),
        AiStreamEvent::ResponseFailed { error } => {
            return Err(ConfiguredInferenceError::from_terminal_error(error));
        }
        AiStreamEvent::ResponseCancelled => {
            return Err(ConfiguredInferenceError::new(
                ConfiguredInferenceErrorCategory::Cancelled,
                "Restricted LLM provider execution was cancelled.",
            ));
        }
        AiStreamEvent::ToolCallCompleted { item_id, call } => {
            upsert_tool_call_output(output_items, item_id, call);
        }
        AiStreamEvent::ToolCallAborted { message, .. } => {
            return Err(ConfiguredInferenceError::new(
                ConfiguredInferenceErrorCategory::Execution,
                format!("Provider aborted a streamed tool call: {message}"),
            ));
        }
        AiStreamEvent::ToolCallStarted { .. }
        | AiStreamEvent::ToolCallArgumentsDelta { .. }
        | AiStreamEvent::ResourceProgress { .. }
        | AiStreamEvent::ReasoningSummaryUpdated { .. }
        | AiStreamEvent::ImageGenerationPartial { .. } => {}
        AiStreamEvent::ReasoningCompleted { item_id, signature } => {
            complete_reasoning_output(output_items, item_id, signature);
        }
        AiStreamEvent::TransportObserved { .. }
        | AiStreamEvent::ResponseStarted { .. }
        | AiStreamEvent::ResponseMetadataUpdated { .. }
        | AiStreamEvent::ResponseMessagePhaseUpdated { .. }
        | AiStreamEvent::ResponseTurnBoundary { .. }
        | AiStreamEvent::ReasoningStarted { .. }
        | AiStreamEvent::ProviderContextItemCaptured { .. }
        | AiStreamEvent::ResponseItemCompleted { .. } => {}
    }
    Ok(None)
}

/// Inserts or replaces one normalized reasoning output while retaining its continuation signature.
/// 插入或替换一项归一化推理输出，同时保留其延续签名。
///
/// # Parameters
/// 参数
/// `output_items` owns ordered output, while the remaining values identify the authoritative item.
/// `output_items` 持有有序输出，其余值标识权威推理项目。
fn upsert_reasoning_output(
    output_items: &mut Vec<ResponsesExecutionOutputItem>,
    item_id: String,
    text: String,
    signature: Option<String>,
) {
    if let Some(existing) = output_items.iter_mut().find(|item| item.item_id == item_id) {
        *existing = ResponsesExecutionOutputItem {
            item_id,
            content: ContentItem::Reasoning { text, signature },
        };
        return;
    }
    output_items.push(ResponsesExecutionOutputItem {
        item_id,
        content: ContentItem::Reasoning { text, signature },
    });
}

/// Appends one reasoning delta to its exact normalized output item.
/// 将一项推理增量追加到其精确归一化输出项目。
///
/// # Parameters
/// 参数
/// `output_items`, `item_id`, and `delta` identify the ordered item and new text fragment.
/// `output_items`、`item_id` 与 `delta` 标识有序项目和新文本分片。
fn append_reasoning_output_delta(
    output_items: &mut Vec<ResponsesExecutionOutputItem>,
    item_id: String,
    delta: String,
) {
    if let Some(ContentItem::Reasoning { text, .. }) = output_items
        .iter_mut()
        .find(|item| item.item_id == item_id)
        .map(|item| &mut item.content)
    {
        text.push_str(delta.as_str());
        return;
    }
    output_items.push(ResponsesExecutionOutputItem {
        item_id,
        content: ContentItem::Reasoning {
            text: delta,
            signature: None,
        },
    });
}

/// Applies one terminal continuation signature to its exact reasoning output item.
/// 将一项终态延续签名应用到其精确推理输出项目。
///
/// # Parameters
/// 参数
/// `output_items`, `item_id`, and `signature` identify the terminal normalized reasoning state.
/// `output_items`、`item_id` 与 `signature` 标识终态归一化推理状态。
fn complete_reasoning_output(
    output_items: &mut Vec<ResponsesExecutionOutputItem>,
    item_id: String,
    signature: Option<String>,
) {
    if let Some(ContentItem::Reasoning {
        signature: current, ..
    }) = output_items
        .iter_mut()
        .find(|item| item.item_id == item_id)
        .map(|item| &mut item.content)
    {
        *current = signature;
        return;
    }
    output_items.push(ResponsesExecutionOutputItem {
        item_id,
        content: ContentItem::Reasoning {
            text: String::new(),
            signature,
        },
    });
}

/// Rejects provider output that mixes assistant text with refusal content.
/// 拒绝将助手文本与拒绝内容混合的供应商输出。
fn reject_text_refusal_mix(
    output_items: &[ResponsesExecutionOutputItem],
) -> Result<(), ConfiguredInferenceError> {
    if output_items
        .iter()
        .any(|item| matches!(&item.content, ContentItem::Refusal { .. }))
    {
        return Err(ConfiguredInferenceError::new(
            ConfiguredInferenceErrorCategory::Execution,
            "Responses execution mixed output text and refusal content.",
        ));
    }
    Ok(())
}

/// Inserts or replaces one completed tool call while preserving provider output order.
/// 插入或替换一个已完成工具调用，同时保留供应商输出顺序。
///
/// # Parameters
/// 参数
/// `output_items` owns retained non-text output and `call` is the authoritative call payload.
/// `output_items` 持有已保留非文本输出，`call` 是权威调用载荷。
fn upsert_tool_call_output(
    output_items: &mut Vec<ResponsesExecutionOutputItem>,
    item_id: String,
    call: ToolCall,
) {
    if let Some(existing) = output_items.iter_mut().find(|item| {
        matches!(&item.content, ContentItem::ToolCall { call: existing } if existing.id == call.id)
    }) {
        *existing = ResponsesExecutionOutputItem {
            item_id,
            content: ContentItem::ToolCall { call },
        };
        return;
    }
    output_items.push(ResponsesExecutionOutputItem {
        item_id,
        content: ContentItem::ToolCall { call },
    });
}

/// Builds one caller-scoped in-flight identity.
/// 构建一个调用方作用域运行中身份。
///
/// # Parameters
/// 参数
/// The caller and request supply stable host and consumer identities.
/// 调用方与请求提供稳定宿主和消费者身份。
///
/// # Returns
/// 返回值
/// A length-prefixed de-duplication key.
/// 返回长度前缀去重键。
fn llm_in_flight_key(
    caller: &InferenceCallerContext,
    request: &ResponsesExecutionRequest,
) -> String {
    inference_in_flight_key(
        caller,
        request.consumer_profile_id.as_str(),
        AiOperation::ConversationRespond,
        request.request_id.as_str(),
        request.idempotency_key.as_deref(),
    )
}

/// Builds one operation-scoped running identity for stream and non-stream requests.
/// 为流式与非流式请求构建一个操作作用域运行中身份。
///
/// # Parameters
/// 参数
/// Trusted caller identity, exact profile/operation, request ID, and optional idempotency key
/// form the closed identity without parsing concatenated values.
/// 可信调用方身份、精确配置档/操作、请求 ID 与可选幂等键组成无需解析拼接值的封闭身份。
///
/// # Returns
/// 返回值
/// A deterministic length-prefixed de-duplication key.
/// 返回确定性的长度前缀去重键。
fn inference_in_flight_key(
    caller: &InferenceCallerContext,
    consumer_profile_id: &str,
    operation: AiOperation,
    request_id: &str,
    idempotency_key: Option<&str>,
) -> String {
    let request_identity = idempotency_key.unwrap_or(request_id);
    format!(
        "{}:{}:{}:{}:{}:{}:{}:{}",
        caller.caller_id.len(),
        caller.caller_id,
        consumer_profile_id.len(),
        consumer_profile_id,
        operation.as_str().len(),
        operation.as_str(),
        request_identity.len(),
        request_identity
    )
}

/// Enforces one caller input-unit ceiling.
/// 执行一项调用方输入单位上限。
///
/// # Parameters
/// 参数
/// `input_units` is the exact aggregate UTF-8 byte count accepted by this boundary.
/// `input_units` 是当前边界接受的精确 UTF-8 字节合计。
///
/// # Returns
/// 返回值
/// Success when no ceiling is configured or the request is within it.
/// 未配置上限或请求未超限时返回成功。
fn enforce_input_budget(
    caller: &InferenceCallerContext,
    input_units: u64,
) -> Result<(), ConfiguredInferenceError> {
    if caller
        .budget
        .max_input_units_per_request
        .is_some_and(|limit| input_units > limit)
    {
        return Err(ConfiguredInferenceError::new(
            ConfiguredInferenceErrorCategory::Limit,
            format!(
                "Inference request input size {input_units} exceeds the caller limit {}.",
                caller
                    .budget
                    .max_input_units_per_request
                    .unwrap_or_default()
            ),
        ));
    }
    Ok(())
}

/// Enforces an explicitly requested LLM output-token ceiling.
/// 执行显式请求的 LLM 输出 Token 上限。
///
/// # Parameters
/// 参数
/// `requested` is caller input and the budget is host-owned.
/// `requested` 是调用方输入，预算由宿主拥有。
///
/// # Returns
/// 返回值
/// Success when the explicit request does not exceed its budget.
/// 显式请求未超过预算时返回成功。
fn enforce_llm_output_budget(
    caller: &InferenceCallerContext,
    requested: Option<u64>,
) -> Result<(), ConfiguredInferenceError> {
    if let (Some(requested), Some(limit)) = (requested, caller.budget.max_output_tokens_per_request)
        && requested > limit
    {
        return Err(ConfiguredInferenceError::new(
            ConfiguredInferenceErrorCategory::Limit,
            format!("Restricted LLM output limit {requested} exceeds the caller limit {limit}."),
        ));
    }
    Ok(())
}

/// Counts restricted LLM message input as exact serialized UTF-8 bytes.
/// 将受限 LLM 消息输入计为精确序列化 UTF-8 字节。
///
/// # Parameters
/// 参数
/// `request` is already contract-valid.
/// `request` 已通过契约校验。
///
/// # Returns
/// 返回值
/// A checked aggregate byte count.
/// 返回经过溢出检查的字节合计。
fn llm_input_units(request: &ResponsesExecutionRequest) -> Result<u64, ConfiguredInferenceError> {
    let encoded = serde_json::to_vec(request).map_err(|error| {
        ConfiguredInferenceError::new(
            ConfiguredInferenceErrorCategory::Contract,
            format!("Responses execution input could not be measured: {error}"),
        )
    })?;
    u64::try_from(encoded.len()).map_err(|_| {
        ConfiguredInferenceError::new(
            ConfiguredInferenceErrorCategory::Limit,
            "Responses execution input exceeds the supported byte range.",
        )
    })
}

/// Counts embedding input as exact UTF-8 identifier and payload bytes.
/// 将向量输入计为精确 UTF-8 标识与载荷字节。
///
/// # Parameters
/// 参数
/// `request` is already contract-valid.
/// `request` 已通过契约校验。
///
/// # Returns
/// 返回值
/// A checked aggregate byte count.
/// 返回经过溢出检查的字节合计。
fn embedding_input_units(request: &EmbeddingRequest) -> Result<u64, ConfiguredInferenceError> {
    let mut units = 0_u64;
    for input in &request.inputs {
        units = checked_input_units(units, input.input_id.len())?;
        match &input.content {
            EmbeddingInputContent::Text { text } => {
                units = checked_input_units(units, text.len())?;
            }
            EmbeddingInputContent::Resource { resource } => {
                units = checked_input_units(units, resource.id.len())?;
                if let Some(path) = resource.local_path.as_deref() {
                    units = checked_input_units(units, path.len())?;
                }
                if let Some(url) = resource.url.as_deref() {
                    units = checked_input_units(units, url.len())?;
                }
            }
        }
    }
    Ok(units)
}

/// Counts rerank input as exact UTF-8 identifier and text bytes.
/// 将重排输入计为精确 UTF-8 标识与文本字节。
///
/// # Parameters
/// 参数
/// `request` is already contract-valid.
/// `request` 已通过契约校验。
///
/// # Returns
/// 返回值
/// A checked aggregate byte count.
/// 返回经过溢出检查的字节合计。
fn rerank_input_units(request: &RerankRequest) -> Result<u64, ConfiguredInferenceError> {
    let mut units = checked_input_units(0, request.query.id.len())?;
    units = checked_input_units(units, request.query.text.len())?;
    for candidate in &request.candidates {
        units = checked_input_units(units, candidate.id.len())?;
        units = checked_input_units(units, candidate.text.len())?;
    }
    Ok(units)
}

/// Adds one platform byte length to a stable public input-unit counter.
/// 将一项平台字节长度加入稳定公共输入单位计数器。
///
/// # Parameters
/// 参数
/// `current` and `additional` are exact byte counts.
/// `current` 与 `additional` 是精确字节数。
///
/// # Returns
/// 返回值
/// The checked total or a stable overflow error.
/// 返回经过检查的合计或稳定溢出错误。
fn checked_input_units(current: u64, additional: usize) -> Result<u64, ConfiguredInferenceError> {
    let additional = u64::try_from(additional).map_err(|_| {
        ConfiguredInferenceError::new(
            ConfiguredInferenceErrorCategory::Limit,
            "Inference input size exceeds the supported platform range.",
        )
    })?;
    current.checked_add(additional).ok_or_else(|| {
        ConfiguredInferenceError::new(
            ConfiguredInferenceErrorCategory::Limit,
            "Inference input size exceeds the supported counter range.",
        )
    })
}

/// Waits for an optional absolute deadline and otherwise remains pending.
/// 等待一个可选绝对截止时间，否则保持等待。
///
/// # Parameters
/// 参数
/// `deadline_unix_ms` is trusted host context.
/// `deadline_unix_ms` 是可信宿主上下文。
async fn wait_until_deadline(deadline_unix_ms: Option<u64>) {
    let Some(deadline) = deadline_unix_ms else {
        std::future::pending::<()>().await;
        return;
    };
    let remaining = deadline.saturating_sub(current_unix_millis());
    tokio::time::sleep(Duration::from_millis(remaining)).await;
}

/// Awaits one non-streaming provider attempt under the caller's absolute deadline.
/// 在调用方绝对截止时间约束下等待一项非流式供应商尝试。
///
/// # Parameters
/// 参数
/// `operation` is one freshly-created provider future and `deadline_unix_ms` is host-issued.
/// `operation` 是一次新建的供应商 Future，`deadline_unix_ms` 由宿主签发。
///
/// # Returns
/// 返回值
/// The untouched provider result nested inside a stable deadline result so retry
/// classification retains the typed provider failure.
/// 返回嵌套在稳定截止时间结果内的原始供应商结果，使重试分类保留强类型供应商失败。
async fn await_provider_attempt_with_deadline<T, F>(
    operation: F,
    deadline_unix_ms: Option<u64>,
) -> Result<Result<T, ProviderExecutionFailure>, ConfiguredInferenceError>
where
    F: Future<Output = Result<T, ProviderExecutionFailure>>,
{
    let deadline_wait = wait_until_deadline(deadline_unix_ms);
    tokio::pin!(deadline_wait);
    tokio::pin!(operation);
    tokio::select! {
        result = &mut operation => Ok(result),
        _ = &mut deadline_wait => {
            Err(ConfiguredInferenceError::new(
                ConfiguredInferenceErrorCategory::Limit,
                "Configured inference operation exceeded its caller deadline.",
            ))
        }
    }
}

/// Returns current Unix time in milliseconds, saturating outside the supported range.
/// 返回当前 Unix 毫秒时间，并在超出支持范围时饱和。
///
/// # Returns
/// 返回值
/// A stable monotonic-enough wall-clock value for deadline comparison.
/// 返回用于截止时间比较的稳定墙钟值。
fn current_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

/// Computes the remaining scheduler wait from one trusted absolute deadline.
/// 根据一个可信绝对截止时间计算剩余调度等待时间。
///
/// # Parameters
/// 参数
/// `deadline_unix_ms` is absent for unbounded waiting or contains a host-owned deadline.
/// `deadline_unix_ms` 可缺失以表示无限等待，或包含宿主管理的截止时间。
///
/// # Returns
/// 返回值
/// `None` for unbounded waiting, a positive duration, or a stable expired-deadline failure.
/// 无限等待时返回 `None`，否则返回正时长或稳定的截止时间已过期失败。
fn scheduler_wait_duration(
    deadline_unix_ms: Option<u64>,
) -> Result<Option<Duration>, ConfiguredInferenceError> {
    let Some(deadline_unix_ms) = deadline_unix_ms else {
        return Ok(None);
    };
    let remaining_ms = deadline_unix_ms.saturating_sub(current_unix_millis());
    if remaining_ms == 0 {
        return Err(ConfiguredInferenceError::new(
            ConfiguredInferenceErrorCategory::Limit,
            "Configured inference deadline expired while waiting for scheduled capacity.",
        ));
    }
    Ok(Some(Duration::from_millis(remaining_ms)))
}

/// Releases one caller counter without touching the independent scheduler slot.
/// 释放一个调用方计数器且不触碰独立调度器槽位。
///
/// # Parameters
/// 参数
/// `counters` owns caller counts and `caller_id` identifies the exact reservation.
/// `counters` 拥有调用方计数，`caller_id` 标识精确预留。
fn release_caller_counter(counters: &Mutex<BTreeMap<String, u32>>, caller_id: &str) {
    let Ok(mut counters) = counters.lock() else {
        return;
    };
    let Some(count) = counters.get_mut(caller_id) else {
        return;
    };
    *count = count.saturating_sub(1);
    if *count == 0 {
        counters.remove(caller_id);
    }
}

/// Removes one shared non-streaming state only when the registry still owns that exact instance.
/// 仅当注册表仍持有同一实例时移除一项共享非流式状态。
///
/// # Parameters
/// 参数
/// `registry`, `key`, and `state` identify the exact running request instance.
/// `registry`、`key` 与 `state` 标识精确运行中请求实例。
///
/// # Returns
/// 返回值
/// Success after the matching entry is removed or was already absent.
/// 匹配项被移除或已经不存在时返回成功。
fn remove_shared_inference_state<T>(
    registry: &Mutex<BTreeMap<String, Arc<SharedInferenceResult<T>>>>,
    key: &str,
    state: &Arc<SharedInferenceResult<T>>,
) -> Result<(), ConfiguredInferenceError> {
    let mut registry = registry.lock().map_err(lock_error)?;
    if registry
        .get(key)
        .is_some_and(|current| Arc::ptr_eq(current, state))
    {
        registry.remove(key);
    }
    Ok(())
}

/// Returns the stable telemetry label for one trusted inference usage scope.
/// 返回一个可信推理用途范围的稳定遥测标签。
///
/// # Parameters
/// 参数
/// `usage_scope` is copied from host-owned caller or frozen route state.
/// `usage_scope` 从宿主持有的调用方或冻结路由状态复制。
///
/// # Returns
/// 返回值
/// One closed snake-case label accepted by statistics hook schemas.
/// 返回统计 Hook Schema 接受的封闭蛇形标签。
fn inference_usage_scope_label(usage_scope: InferenceUsageScope) -> &'static str {
    match usage_scope {
        InferenceUsageScope::InteractiveAgent => "interactive_agent",
        InferenceUsageScope::InternalBackground => "internal_background",
        InferenceUsageScope::ExternalService => "external_service",
    }
}

/// Builds the exact shared identity payload accepted by every inference lifecycle hook.
/// 构建每个推理生命周期 Hook 接收的精确共享身份载荷。
///
/// # Parameters
/// 参数
/// `facts` contains only non-secret host, route, model, operation, and cardinality facts.
/// `facts` 仅包含不含秘密的宿主、路由、模型、操作与基数事实。
///
/// # Returns
/// 返回值
/// A JSON object containing required common fields and only present optional route fields.
/// 返回包含必需公共字段且仅写入已存在可选路由字段的 JSON 对象。
fn internal_inference_identity_payload(
    facts: &InternalInferenceTelemetryFacts,
) -> serde_json::Value {
    let mut payload = json!({
        "schema_version": 1,
        "request_id": facts.request_id,
        "execution_id": facts.execution_id,
        "session_id": facts.execution_id,
        "project_id": null,
        "workspace_id": "vulcan-code-internal-inference",
        "owner_kind": facts.owner_kind,
        "owner_id": facts.caller_id,
        "owner_name": facts.consumer_profile_id,
        "usage_scope": inference_usage_scope_label(facts.usage_scope),
        "provider_key": facts.provider_id,
        "provider_name": facts.provider_name,
        "service_entry_id": facts.service_entry_id,
        "connection_id": facts.connection_id,
        "model_key": facts.model_id,
        "model_name": facts.model_name,
        "operation": facts.operation,
        "capability_revision": facts.capability_revision,
        "catalog_revision": facts.catalog_revision,
        "route_revision": facts.route_revision,
        "attempt_index": facts.retry_count,
        "retry_count": facts.retry_count,
        "calendar_date": telemetry_calendar_date(facts.started_at_unix_ms),
        "timezone_offset_minutes": 0
    });
    let object = payload
        .as_object_mut()
        .expect("Internal inference telemetry identity must be an object.");
    insert_optional_telemetry_value(
        object,
        "protocol_profile_id",
        facts.protocol_profile_id.as_ref().map(|value| json!(value)),
    );
    insert_optional_telemetry_value(
        object,
        "action_binding_id",
        facts.action_binding_id.as_ref().map(|value| json!(value)),
    );
    insert_optional_telemetry_value(
        object,
        "secondary_route_id",
        facts.secondary_route_id.as_ref().map(|value| json!(value)),
    );
    insert_optional_telemetry_value(
        object,
        "secondary_provider_name",
        facts
            .secondary_provider_name
            .as_ref()
            .map(|value| json!(value)),
    );
    insert_optional_telemetry_value(
        object,
        "secondary_endpoint_name",
        facts
            .secondary_endpoint_name
            .as_ref()
            .map(|value| json!(value)),
    );
    insert_optional_telemetry_value(
        object,
        "input_item_count",
        facts.input_item_count.map(serde_json::Value::from),
    );
    insert_optional_telemetry_value(
        object,
        "output_item_count",
        facts.output_item_count.map(serde_json::Value::from),
    );
    insert_optional_telemetry_value(
        object,
        "vector_dimensions",
        facts.vector_dimensions.map(serde_json::Value::from),
    );
    payload
}

/// Builds one privacy-safe terminal plugin payload from normalized inference facts.
/// 从归一化推理事实构建一份隐私安全的插件终态载荷。
///
/// # Parameters
/// 参数
/// `facts` contains immutable route, timing, operation, and normalized usage data, while
/// `completed_at_unix_ms` is the host-captured terminal wall-clock timestamp.
/// `facts` 包含不可变路由、计时、操作与归一化用量数据，
/// `completed_at_unix_ms` 是宿主捕获的终态墙钟时间戳。
///
/// # Returns
/// 返回值
/// The exact `provider.request.completed` payload consumed by usage-statistics plugins.
/// 返回用量统计插件消费的精确 `provider.request.completed` 载荷。
fn internal_inference_terminal_payload(
    facts: &InternalInferenceTelemetryFacts,
    completed_at_unix_ms: u64,
) -> serde_json::Value {
    let mut payload = internal_inference_identity_payload(facts);
    let object = payload
        .as_object_mut()
        .expect("Internal inference telemetry identity must be an object.");
    object.extend([
        (String::from("status"), json!(facts.status)),
        (
            String::from("started_at_ms"),
            json!(facts.started_at_unix_ms),
        ),
        (
            String::from("first_event_at_ms"),
            json!(facts.first_response_at_unix_ms),
        ),
        (
            String::from("first_token_at_ms"),
            json!(facts.first_token_at_unix_ms),
        ),
        (String::from("completed_at_ms"), json!(completed_at_unix_ms)),
        (
            String::from("time_to_first_event_ms"),
            json!(
                facts
                    .first_response_at_unix_ms
                    .map(|timestamp| timestamp.saturating_sub(facts.started_at_unix_ms))
            ),
        ),
        (
            String::from("time_to_first_token_ms"),
            json!(
                facts
                    .first_token_at_unix_ms
                    .map(|timestamp| timestamp.saturating_sub(facts.started_at_unix_ms))
            ),
        ),
        (String::from("duration_ms"), json!(facts.duration_ms)),
        (
            String::from("input_tokens"),
            json!(usage_value_or_null(&facts.usage.input_tokens)),
        ),
        (
            String::from("uncached_input_tokens"),
            json!(usage_value_or_null(&facts.usage.uncached_input_tokens)),
        ),
        (
            String::from("output_tokens"),
            json!(usage_value_or_null(&facts.usage.output_tokens)),
        ),
        (
            String::from("reasoning_tokens"),
            json!(usage_value_or_null(&facts.usage.reasoning_tokens)),
        ),
        (
            String::from("total_tokens"),
            json!(usage_value_or_null(&facts.usage.total_tokens)),
        ),
        (
            String::from("cached_input_tokens"),
            json!(usage_value_or_null(&facts.usage.cache_read_tokens)),
        ),
        (
            String::from("cache_creation_input_tokens"),
            json!(usage_value_or_null(&facts.usage.cache_write_tokens)),
        ),
        (String::from("finish_reason"), serde_json::Value::Null),
        (String::from("error_category"), json!(facts.error_category)),
        (String::from("error_code"), serde_json::Value::Null),
    ]);
    let provider_error = facts.provider_error.as_deref();
    object.insert(
        String::from("dispatch_outcome"),
        match provider_error {
            Some(error) => json!(error.dispatch_outcome),
            None if facts.status == "succeeded" => json!(DispatchOutcome::Dispatched),
            None => serde_json::Value::Null,
        },
    );
    object.insert(
        String::from("semantic_output_observed"),
        json!(
            provider_error
                .map(|error| error.semantic_output_observed)
                .unwrap_or(facts.first_response_at_unix_ms.is_some())
        ),
    );
    object.insert(
        String::from("replay_safe"),
        provider_error
            .map(|error| {
                json!(
                    !error.semantic_output_observed
                        && !error.side_effect_observed
                        && error.dispatch_outcome != DispatchOutcome::Unknown
                )
            })
            .unwrap_or(serde_json::Value::Null),
    );
    object.insert(
        String::from("transport"),
        provider_error
            .and_then(|error| error.transport)
            .or(facts.transport)
            .map(|value| json!(value))
            .unwrap_or(serde_json::Value::Null),
    );
    object.insert(
        String::from("dispatched_at_ms"),
        json!(facts.dispatched_at_ms),
    );
    object.insert(String::from("headers_at_ms"), json!(facts.headers_at_ms));
    object.insert(
        String::from("http_status"),
        provider_error
            .and_then(|error| error.http_status)
            .or(facts.transport_http_status)
            .map(serde_json::Value::from)
            .unwrap_or(serde_json::Value::Null),
    );
    object.insert(
        String::from("websocket_close_code"),
        provider_error
            .and_then(|error| error.websocket_close_code)
            .or(facts.websocket_close_code)
            .map(serde_json::Value::from)
            .unwrap_or(serde_json::Value::Null),
    );
    object.insert(
        String::from("provider_request_id"),
        provider_error
            .and_then(|error| error.request_id.as_ref())
            .or(facts.provider_request_id.as_ref())
            .map(|value| json!(value))
            .unwrap_or(serde_json::Value::Null),
    );
    object.insert(
        String::from("rate_limit_headers"),
        json!(facts.rate_limit_headers),
    );
    object.insert(
        String::from("error_code"),
        provider_error
            .and_then(|error| error.upstream_code.as_ref())
            .map(|value| json!(value))
            .unwrap_or(serde_json::Value::Null),
    );
    object.insert(
        String::from("retry_after"),
        provider_error
            .and_then(|error| error.retry_after.as_ref())
            .map(|value| json!(value))
            .unwrap_or(serde_json::Value::Null),
    );
    payload
}

/// Builds one native terminal usage event from the exact configured-inference facts.
/// 根据精确已配置推理事实构建一条原生终态用量事件。
fn internal_inference_native_usage_event(
    facts: &InternalInferenceTelemetryFacts,
    payload: serde_json::Value,
) -> Result<ProviderRequestTelemetryEvent, String> {
    let session_id = Uuid::parse_str(facts.execution_id.as_str()).map_err(|error| {
        format!(
            "configured-inference execution id `{}` is not a UUID: {error}",
            facts.execution_id
        )
    })?;
    Ok(ProviderRequestTelemetryEvent::detached(
        ProviderRequestTelemetryEventKind::Completed,
        session_id,
        payload,
        Some(ProviderRequestCompletedUsage {
            update_kind: Some(facts.usage.update_kind),
            input_tokens: usage_value_usize(&facts.usage.input_tokens)?,
            uncached_input_tokens: usage_value_usize(&facts.usage.uncached_input_tokens)?,
            cached_input_tokens: usage_value_usize(&facts.usage.cache_read_tokens)?,
            cache_creation_input_tokens: usage_value_usize(&facts.usage.cache_write_tokens)?,
            output_tokens: usage_value_usize(&facts.usage.output_tokens)?,
            reasoning_tokens: usage_value_usize(&facts.usage.reasoning_tokens)?,
            total_tokens: usage_value_usize(&facts.usage.total_tokens)?,
            audio_input_tokens: usage_value_usize(&facts.usage.audio_input_tokens)?,
            audio_output_tokens: usage_value_usize(&facts.usage.audio_output_tokens)?,
            extraction_profile_id: facts.usage.extraction_profile_id.clone(),
            extraction_profile_revision: facts.usage.extraction_profile_revision,
            source_paths: facts.usage.source_paths.clone(),
            validation_flags: facts.usage.validation_flags.clone(),
            response_model: facts.usage.response_model.clone(),
            response_id: facts.usage.response_id.clone(),
            billing_details: facts.usage.billing_details.clone(),
        }),
    ))
}

/// Converts one known usage value into the host token-width without saturating.
/// 将一个已知用量值无饱和地转换为宿主 Token 宽度。
fn usage_value_usize(value: &UsageValue) -> Result<Option<usize>, String> {
    match value {
        UsageValue::Unknown => Ok(None),
        UsageValue::Known { value, .. } => usize::try_from(*value)
            .map(Some)
            .map_err(|_| format!("provider usage value {value} exceeds host usize")),
    }
}

/// Inserts one optional telemetry field without serializing absence as JSON null.
/// 插入一项可选遥测字段且不把缺失序列化为 JSON 空值。
///
/// # Parameters
/// 参数
/// `object` is the lifecycle payload, `key` is schema-declared, and `value` is optional.
/// `object` 是生命周期载荷，`key` 已由 Schema 声明，`value` 为可选值。
///
/// # Returns
/// 返回值
/// This function mutates the payload and returns no value.
/// 本函数修改载荷且不返回值。
fn insert_optional_telemetry_value(
    object: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: Option<serde_json::Value>,
) {
    if let Some(value) = value {
        object.insert(key.to_string(), value);
    }
}

/// Converts one Unix timestamp into the stable local telemetry calendar date.
/// 将一个 Unix 时间戳转换为稳定的本地遥测日历日期。
///
/// # Parameters
/// 参数
/// `timestamp_unix_ms` is a host-captured Unix millisecond value.
/// `timestamp_unix_ms` 是宿主捕获的 Unix 毫秒值。
///
/// # Returns
/// 返回值
/// The derived date, or the Unix epoch date only when conversion is impossible.
/// 返回推导日期；仅在无法转换时返回 Unix 纪元日期。
fn telemetry_calendar_date(timestamp_unix_ms: u64) -> String {
    i64::try_from(timestamp_unix_ms / 1_000)
        .ok()
        .and_then(|timestamp| OffsetDateTime::from_unix_timestamp(timestamp).ok())
        .map(|timestamp| timestamp.date().to_string())
        .unwrap_or_else(|| String::from("1970-01-01"))
}

/// Dispatches one content-free inference lifecycle hook in the background.
/// 在后台分派一条不含内容的推理生命周期 Hook。
///
/// # Parameters
/// 参数
/// `runtime`, `hook_point`, `session_id`, and `payload` are fully resolved host facts.
/// `runtime`、`hook_point`、`session_id` 与 `payload` 均为完整解析的宿主事实。
///
/// # Returns
/// 返回值
/// This function schedules diagnostic-only delivery and returns immediately.
/// 本函数调度仅用于诊断的投递并立即返回。
fn dispatch_internal_inference_hook(
    runtime: VulcanPluginRuntimeManager,
    hook_point: &'static str,
    session_id: String,
    payload: serde_json::Value,
) {
    tokio::spawn(async move {
        let mut context = HookContext::new(
            payload
                .get("request_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("internal-inference"),
            hook_point,
        );
        context.scope = HookScope::Session;
        context.workspace_id = Some(String::from("vulcan-code-internal-inference"));
        context.session_id = Some(session_id);
        if let Err(error) =
            vulcan_agent_runtime::agent_runtime::vulcan_plugin_hooks::dispatch_vulcan_plugin_hook(
                runtime, context, payload,
            )
            .await
        {
            tracing::warn!("[inference-service] internal inference telemetry failed: {error}");
        }
    });
}

/// Reads a known normalized usage value while preserving unknown as null.
/// 读取已知归一化用量值，同时将未知保留为空。
///
/// # Parameters
/// 参数
/// `value` is one optional provider-neutral usage field.
/// `value` 是一项可选供应商无关用量字段。
///
/// # Returns
/// 返回值
/// The known count or `None` when the provider did not report it.
/// 返回已知计数；供应商未报告时返回 `None`。
fn usage_value_or_null(value: &UsageValue) -> Option<u64> {
    match value {
        UsageValue::Known { value, .. } => Some(*value),
        UsageValue::Unknown => None,
    }
}

/// Converts one duration to a saturating millisecond counter.
/// 将一项时长转换为饱和毫秒计数。
///
/// # Parameters
/// 参数
/// `duration` is measured by the local monotonic clock.
/// `duration` 由本地单调时钟测量。
///
/// # Returns
/// 返回值
/// A stable public millisecond duration.
/// 返回稳定公共毫秒时长。
fn duration_millis(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}

/// Converts a provider configuration diagnostic into one controlled error.
/// 将供应商配置诊断转换为一项受控错误。
///
/// # Parameters
/// 参数
/// `message` contains no credential material.
/// `message` 不包含凭据材料。
///
/// # Returns
/// 返回值
/// A configuration-category failure.
/// 返回配置类别失败。
fn configuration_error(message: String) -> ConfiguredInferenceError {
    ConfiguredInferenceError::new(ConfiguredInferenceErrorCategory::Configuration, message)
}

/// Converts a poisoned process-local lock into one controlled error.
/// 将中毒的进程内锁转换为一项受控错误。
///
/// # Parameters
/// 参数
/// The concrete poison value is intentionally discarded.
/// 具体中毒值被刻意丢弃。
///
/// # Returns
/// 返回值
/// A stable execution-category failure.
/// 返回稳定执行类别失败。
fn lock_error<T>(_: std::sync::PoisonError<T>) -> ConfiguredInferenceError {
    ConfiguredInferenceError::new(
        ConfiguredInferenceErrorCategory::Execution,
        "Configured inference process-local state lock is poisoned.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    use vulcan_ai_contract::{
        INFERENCE_CONTRACT_VERSION, InferenceCallerBudget, InferenceUsageScope,
        ParallelToolCallPolicy,
    };
    use vulcan_provider_adapters::{
        InferenceProfile, InferenceRouteBinding, InferenceRouteConfiguration,
    };

    /// Verifies every remote lifecycle call remains bound to the planning fingerprint.
    /// 验证每次远端生命周期调用都保持绑定到规划指纹。
    #[test]
    fn embedding_batch_lifecycle_rejects_route_or_payload_drift() {
        assert!(validate_embedding_batch_request_fingerprint("frozen", "frozen").is_ok());
        assert!(validate_embedding_batch_request_fingerprint("current", "frozen").is_err());
    }

    /// Assert dimension precedence without changing another consumer's default policy.
    /// 验证维度优先级且不改变其他消费者的默认策略。
    ///
    /// Takes no parameters and returns nothing; failed isolation or precedence panics.
    /// 无参数、无返回值；隔离或优先级断言不成立时测试失败。
    #[test]
    fn consumer_embedding_dimensions_preserve_admission_and_isolate_peers() {
        // configuration contains one explicit VCE dimension and a legacy VMM profile.
        // configuration 包含 VCE 明确维度与沿用旧策略的 VMM 配置档。
        let mut configuration = PersistedAiConfiguration::default();
        configuration.inference_profiles.insert(
            String::from("vce"),
            InferenceProfile {
                embedding_dimensions: Some(1024),
                ..Default::default()
            },
        );
        configuration
            .inference_profiles
            .insert(String::from("vmm"), InferenceProfile::default());
        assert_eq!(
            consumer_embedding_request_dimensions(&configuration, "vce", None),
            Some(1024)
        );
        assert_eq!(
            consumer_embedding_request_dimensions(&configuration, "vce", Some(2048)),
            Some(2048)
        );
        assert_eq!(
            consumer_embedding_request_dimensions(&configuration, "vmm", None),
            None
        );
        assert_eq!(
            consumer_embedding_request_dimensions(&configuration, "knowledge-test", None),
            None
        );
        assert_eq!(
            consumer_embedding_request_dimensions(&configuration, "vmm", Some(768)),
            Some(768)
        );
    }

    /// Builds one authorized deterministic test caller.
    /// 构建一个已授权确定性测试调用方。
    ///
    /// # Returns
    /// 返回值
    /// A VMM-scoped background caller with all operation permissions.
    /// 返回具有全部操作权限的 VMM 后台调用方。
    fn test_caller() -> InferenceCallerContext {
        InferenceCallerContext {
            // Existing caller fixtures retain usage-scope scheduling.
            // 既有调用方夹具保留用途范围调度。
            scheduling_class: vulcan_ai_contract::InferenceSchedulingClass::ByUsageScope,
            caller_id: String::from("vmm-test"),
            allowed_consumer_profile_ids: BTreeSet::from([String::from("vmm")]),
            permissions: BTreeSet::from([
                InferencePermission::Llm,
                InferencePermission::Embedding,
                InferencePermission::Rerank,
            ]),
            usage_scope: InferenceUsageScope::InternalBackground,
            interactive_session_id: None,
            deadline_unix_ms: None,
            budget: InferenceCallerBudget {
                max_concurrent_executions: 1,
                max_input_units_per_request: Some(1_024),
                max_output_tokens_per_request: Some(128),
                max_requests_per_minute: Some(60),
                max_input_units_per_minute: Some(60_000),
                max_execution_duration_ms: Some(30_000),
            },
        }
    }

    /// Builds one minimal Responses execution request for capability validation tests.
    /// 构建一项用于能力校验测试的最小 Responses 执行请求。
    ///
    /// # Returns
    /// 返回值
    /// A text-only request with provider-default optional semantics.
    /// 返回一项仅含文本且可选语义均采用供应商默认值的请求。
    fn test_responses_request() -> ResponsesExecutionRequest {
        ResponsesExecutionRequest {
            contract_version: INFERENCE_CONTRACT_VERSION,
            request_id: String::from("request-capability-1"),
            idempotency_key: None,
            consumer_profile_id: String::from("vmm"),
            purpose_id: String::from("precheck_l1"),
            messages: vec![vulcan_ai_contract::AiMessage {
                role: vulcan_ai_contract::MessageRole::User,
                content: vec![ContentItem::Text {
                    text: String::from("test"),
                }],
                provider_context_items: Vec::new(),
            }],
            tools: Vec::new(),
            native_tools: Vec::new(),
            tool_choice: ToolChoicePolicy::Auto,
            parallel_tool_calls: ParallelToolCallPolicy::ProviderDefault,
            structured_output: StructuredOutputPolicy::None,
            prompt_cache: Default::default(),
            prompt_cache_key: None,
            include_reasoning_encrypted_content: false,
            reasoning_effort: None,
            reasoning_output: Default::default(),
            model_parameters: BTreeMap::new(),
            stop_sequences: Vec::new(),
            resources: Vec::new(),
            output_token_limit: None,
        }
    }

    /// Builds one retained execution state for deterministic waiter-cancellation tests.
    /// 构建一项用于确定性等待者取消测试的保留执行状态。
    ///
    /// # Parameters
    /// 参数
    /// `phase` selects whether dropping the last waiter should cancel provider work.
    /// `phase` 用于选择丢弃最后一个等待者时是否应取消供应商工作。
    ///
    /// # Returns
    /// 返回值
    /// The shared state and a receiver observing its cooperative cancellation signal.
    /// 返回共享状态以及观察其协作取消信号的接收器。
    fn test_llm_execution_state(
        phase: ConfiguredLlmExecutionPhase,
    ) -> (Arc<ConfiguredLlmExecutionState>, watch::Receiver<bool>) {
        let (event_sender, _) = broadcast::channel(4);
        let (cancellation_sender, cancellation_receiver) = watch::channel(false);
        let state = Arc::new(ConfiguredLlmExecutionState {
            owner_caller_id: String::from("caller"),
            request_fingerprint: String::from("fingerprint"),
            in_flight_key: String::from("caller:request"),
            snapshot: Mutex::new(ConfiguredLlmExecutionSnapshot {
                execution_id: String::from("execution"),
                phase,
                route: ExecutionRouteSnapshot {
                    provider_id: String::from("provider"),
                    service_entry_id: String::from("service"),
                    connection_id: String::from("connection"),
                    model_id: String::from("model"),
                    protocol_binding_id: Some(String::from("openai.responses")),
                    action_binding_id: None,
                    secondary_route_id: None,
                    secondary_provider_name: None,
                    secondary_endpoint_name: None,
                    capability_revision: String::from("capability"),
                    catalog_revision: String::from("catalog"),
                    route_revision: String::from("route"),
                    usage_scope: InferenceUsageScope::ExternalService,
                },
                response: None,
                error: None,
            }),
            event_sender,
            event_history: Mutex::new(Vec::new()),
            event_history_bytes: Mutex::new(0),
            event_history_truncated: Mutex::new(false),
            cancellation_sender,
            waiter_count: AtomicUsize::new(0),
            terminal_notify: Notify::new(),
            terminal_at: Mutex::new(None),
        });
        (state, cancellation_receiver)
    }

    /// Verifies a running provider request is cancelled only after its final non-streaming HTTP waiter disappears.
    /// 验证运行中的供应商请求只会在最后一个非流式 HTTP 等待者离开后被取消。
    #[test]
    fn last_non_streaming_waiter_drop_cancels_running_execution() {
        let (state, cancellation) = test_llm_execution_state(ConfiguredLlmExecutionPhase::Running);
        let first = ConfiguredLlmWaiterGuard::new(Arc::clone(&state));
        let second = ConfiguredLlmWaiterGuard::new(Arc::clone(&state));

        drop(first);
        assert!(!*cancellation.borrow());
        assert_eq!(state.waiter_count.load(Ordering::Acquire), 1);

        drop(second);
        assert!(*cancellation.borrow());
        assert_eq!(state.waiter_count.load(Ordering::Acquire), 0);
    }

    /// Verifies returning one completed response releases waiter ownership without emitting a late cancellation.
    /// 验证返回已完成响应时会释放等待者所有权且不会发送迟到的取消信号。
    #[test]
    fn completed_waiter_drop_does_not_emit_cancellation() {
        let (state, cancellation) =
            test_llm_execution_state(ConfiguredLlmExecutionPhase::Completed);
        let waiter = ConfiguredLlmWaiterGuard::new(Arc::clone(&state));

        drop(waiter);

        assert!(!*cancellation.borrow());
        assert_eq!(state.waiter_count.load(Ordering::Acquire), 0);
    }

    /// Verifies complete transport tracing is both opt-in and restricted to the authenticated VMM consumer.
    /// 验证完整传输追踪既需显式启用，又仅限已认证 VMM 消费者。
    #[tokio::test]
    async fn configured_inference_request_trace_is_opt_in_and_vmm_scoped() {
        let runtime = Arc::new(PublicAiRuntime::unavailable().expect("test public AI runtime"));
        let mut request = test_responses_request();
        let route = ExecutionRouteSnapshot {
            provider_id: String::from("provider"),
            service_entry_id: String::from("service"),
            connection_id: String::from("connection"),
            protocol_binding_id: Some(String::from("openai.responses.v1")),
            action_binding_id: None,
            secondary_route_id: None,
            secondary_provider_name: None,
            secondary_endpoint_name: None,
            model_id: String::from("model"),
            capability_revision: String::from("capability"),
            catalog_revision: String::from("catalog"),
            route_revision: String::from("sha256:route"),
            usage_scope: InferenceUsageScope::InternalBackground,
        };
        let service = ConfiguredInferenceService::new(Arc::clone(&runtime));
        assert!(
            service
                .start_configured_inference_provider_request_trace(
                    &request,
                    &route,
                    "00000000-0000-0000-0000-000000000001",
                    0,
                )
                .is_none(),
            "disabled tracing must not allocate a capture channel"
        );

        let service =
            ConfiguredInferenceService::new(runtime).with_provider_request_trace_enabled(true);
        request.consumer_profile_id = String::from("plugin");
        assert!(
            service
                .start_configured_inference_provider_request_trace(
                    &request,
                    &route,
                    "00000000-0000-0000-0000-000000000002",
                    0,
                )
                .is_none(),
            "non-VMM consumers must not receive bounded wire-request tracing"
        );

        request.consumer_profile_id = String::from(VMM_CONSUMER_PROFILE_ID);
        let capture = service
            .start_configured_inference_provider_request_trace(
                &request,
                &route,
                "00000000-0000-0000-0000-000000000003",
                0,
            )
            .expect("opted-in VMM execution should allocate one capture channel");
        drop(capture);
        tokio::task::yield_now().await;
    }

    /// Verifies Batch file capture follows the existing switch and the exact VCE caller identity.
    /// 验证 Batch 文件捕获遵循现有开关及精确 VCE 调用方身份。
    #[test]
    fn vce_batch_artifact_capture_is_opt_in_and_project_scoped() {
        let runtime = Arc::new(PublicAiRuntime::unavailable().expect("test public AI runtime"));
        let mut caller = test_caller();
        caller.caller_id = String::from("vulcan-code-vce:project-A");
        let disabled = ConfiguredInferenceService::new(Arc::clone(&runtime));
        assert!(disabled.vce_batch_artifact_sink(&caller).is_none());

        let enabled =
            ConfiguredInferenceService::new(runtime).with_provider_request_trace_enabled(true);
        let sink = enabled
            .vce_batch_artifact_sink(&caller)
            .expect("VCE capture enabled");
        assert_eq!(sink.project_id, "project-A");
        caller.caller_id = String::from("vulcan-code-vmm:project-A");
        assert!(enabled.vce_batch_artifact_sink(&caller).is_none());
    }

    /// Verifies cache identities are caller-isolated, bounded, and never contain untrusted key text.
    /// 验证缓存身份按调用方隔离、长度有界且绝不包含不可信键原文。
    #[test]
    fn responses_cache_scope_is_bounded_opaque_and_caller_isolated() {
        let caller = test_caller();
        let mut request = test_responses_request();
        request.prompt_cache_key = Some("secret-cache-key\r\nheader-like-value".repeat(4096));
        let first = responses_cache_scope_id(&caller, &request);
        let repeated = responses_cache_scope_id(&caller, &request);
        assert_eq!(first, repeated);
        assert_eq!(first.len(), "vulcan-responses:sha256:".len() + 64);
        assert!(!first.contains("secret-cache-key"));
        assert!(!first.contains('\r'));
        assert!(!first.contains('\n'));

        let mut another_caller = caller;
        another_caller.caller_id = String::from("another-vmm");
        assert_ne!(first, responses_cache_scope_id(&another_caller, &request));
    }

    /// Verifies function-tool semantics do not depend on a model capability declaration.
    /// 验证函数工具语义不依赖模型能力声明。
    #[test]
    fn responses_capability_validation_accepts_function_tools() {
        let mut request = test_responses_request();
        request.tools.push(vulcan_ai_contract::ToolDefinition {
            name: String::from("lookup"),
            description: String::from("Looks up one value."),
            input_schema: serde_json::json!({"type": "object"}),
            input_format: Default::default(),
            strict: true,
        });

        validate_responses_model_capabilities(&request, &ModelCapabilities::default())
            .expect("conversation models always accept function-tool semantics");
    }

    /// Verifies selected function names must resolve to one unique declared tool.
    /// 验证被选择的函数名必须解析到唯一声明的工具。
    #[test]
    fn responses_capability_validation_rejects_unknown_specific_tool() {
        let mut request = test_responses_request();
        request.tool_choice = ToolChoicePolicy::Specific {
            tool_name: String::from("missing"),
        };
        let capabilities = ModelCapabilities::default();

        let error = validate_responses_model_capabilities(&request, &capabilities)
            .expect_err("an unknown specific tool must be rejected");

        assert!(error.message.contains("unknown function"));
    }

    /// Verifies native image generation requires both the native tool and image-output capability.
    /// 验证原生图片生成同时要求原生工具与图片输出能力。
    #[test]
    fn responses_capability_validation_requires_image_output_for_generation() {
        let mut request = test_responses_request();
        request.native_tools.push(NativeToolKind::ImageGeneration);
        let capabilities = ModelCapabilities {
            provider_native_tools: vec![NativeToolKind::ImageGeneration],
            ..ModelCapabilities::default()
        };

        let error = validate_responses_model_capabilities(&request, &capabilities)
            .expect_err("image generation without image output must be rejected");

        assert!(error.message.contains("image output"));
    }

    /// Verifies every typed resource input is checked against the frozen model declaration.
    /// 验证每项强类型资源输入都会依据冻结模型声明进行校验。
    #[test]
    fn responses_capability_validation_checks_message_resources() {
        let mut request = test_responses_request();
        request.messages[0].content.push(ContentItem::Resource {
            resource: vulcan_ai_contract::ResourceReference {
                id: String::from("image-1"),
                kind: ResourceKind::Image,
                media_type: Some(String::from("image/png")),
                source_kind: vulcan_ai_contract::ResourceSourceKind::RemoteUrl,
                file_name: None,
                image_detail: Some(vulcan_ai_contract::ImageInputDetail::High),
                resource_path: None,
                local_path: None,
                url: Some(String::from("https://example.invalid/image.png")),
            },
        });

        let error = validate_responses_model_capabilities(&request, &ModelCapabilities::default())
            .expect_err("undeclared image input must be rejected");
        assert!(error.message.contains("image"));

        let mut capabilities = ModelCapabilities::default();
        capabilities.input_resources.push(ResourceKind::Image);
        validate_responses_model_capabilities(&request, &capabilities)
            .expect("declared image input should pass");
    }

    /// Verifies caller-explicit prompt caching cannot bypass the selected model capability.
    /// 验证调用方显式提示词缓存不能绕过所选模型能力。
    #[test]
    fn prompt_cache_policy_validation_rejects_unsupported_managed_explicit() {
        let policy = PromptCachePolicy::ManagedExplicit {
            ttl: vulcan_ai_contract::PromptCacheTtlSelection::ProfileDefault,
        };

        let error = validate_prompt_cache_policy(&ModelCapabilities::default(), &policy)
            .expect_err("unsupported managed prompt caching must fail closed");

        assert_eq!(error.category, ConfiguredInferenceErrorCategory::Contract);
        assert!(error.message.contains("not supported"));
    }

    /// Verifies reserved capacities keep interactive work admissible during lower-class saturation.
    /// 验证保留容量会在较低类别饱和时保持交互式工作可受理。
    #[test]
    fn workload_scheduler_preserves_interactive_capacity() {
        let mut state = InferenceSchedulerState {
            active_total: 4,
            active_external: INFERENCE_SCHEDULER_EXTERNAL_CAPACITY,
            active_background: INFERENCE_SCHEDULER_BACKGROUND_CAPACITY,
            ..InferenceSchedulerState::default()
        };
        state.interactive.push_back(1);
        state.external.push_back(2);
        state.background.push_back(3);
        assert!(state.can_admit(InferenceWorkloadClass::Interactive, 1));
        assert!(!state.can_admit(InferenceWorkloadClass::External, 2));
        assert!(!state.can_admit(InferenceWorkloadClass::Background, 3));
    }

    /// Verifies bounded bypass eventually advances a waiting background queue head.
    /// 验证有限越过最终会推进等待中的后台队首。
    #[test]
    fn workload_scheduler_bounds_priority_starvation() {
        let mut state = InferenceSchedulerState {
            background_bypass_count: INFERENCE_SCHEDULER_MAX_PRIORITY_BYPASS,
            ..InferenceSchedulerState::default()
        };
        state.interactive.push_back(1);
        state.external.push_back(2);
        state.background.push_back(3);
        assert!(!state.can_admit(InferenceWorkloadClass::Interactive, 1));
        assert!(!state.can_admit(InferenceWorkloadClass::External, 2));
        assert!(state.can_admit(InferenceWorkloadClass::Background, 3));
    }

    /// Prove foreground query capacity survives a busy background slot and repeated fair-queue bypass.
    /// 证明前台查询容量不会被繁忙后台槽及反复公平队列越过所阻塞。
    /// No inputs; checks actual permits and queue cleanup without model or storage traffic.
    /// 无输入；检查实际许可和队列清理，不产生模型或存储流量。
    #[tokio::test]
    async fn internal_query_scheduler_remains_live_during_background_work() {
        // One shared scheduler reproduces the production background bottleneck.
        // 一个共享调度器复现生产后台瓶颈。
        let scheduler = Arc::new(InferenceWorkloadScheduler::default());
        // Keep the original background task alive throughout all foreground queries.
        // 在全部前台查询期间保持原后台任务存活。
        let background = scheduler
            .acquire(InferenceWorkloadClass::Background, None)
            .await
            .expect("background capacity");
        // A second background ticket tests fairness without being allowed to consume foreground slots.
        // 第二张后台票据测试公平性，不允许其占用前台槽位。
        let mut waiting = Box::pin(scheduler.acquire(InferenceWorkloadClass::Background, None));
        assert!(futures_util::poll!(waiting.as_mut()).is_pending());
        // Only the host-issued scheduling class changes, not the authorized route scope.
        // 仅改变宿主签发调度类别，不改变授权路由用途。
        let mut caller = test_caller();
        caller.scheduling_class = vulcan_ai_contract::InferenceSchedulingClass::InternalQuery;
        caller
            .authorize("vmm", InferencePermission::Embedding)
            .expect("internal authorization");
        for _ in 0..=INFERENCE_SCHEDULER_MAX_PRIORITY_BYPASS {
            // Every request must finish while background capacity is still occupied.
            // 每项请求都必须在后台容量仍被占用时完成。
            let query = tokio::time::timeout(
                std::time::Duration::from_secs(1),
                scheduler.acquire(InferenceWorkloadClass::from_caller(&caller), None),
            )
            .await
            .expect("foreground cannot wait for background")
            .expect("query capacity");
            drop(query);
        }
        drop(background);
        // Once background capacity is free, the retained queue head must also make progress.
        // 后台容量释放后，保留的队首也必须推进。
        let second = tokio::time::timeout(std::time::Duration::from_secs(1), waiting)
            .await
            .expect("background fairness")
            .expect("second background capacity");
        drop(second);
        // Both priority and background permits return to the same bounded scheduler.
        // 优先和后台许可都归还同一个有界调度器。
        let state = scheduler.state.lock().expect("scheduler");
        assert_eq!(state.active_total, 0);
        assert_eq!(state.active_background, 0);
        assert!(state.background.is_empty());
        assert!(state.interactive.is_empty());
    }

    /// Enforce total capacity and remove cancelled foreground tickets without changing background capacity.
    /// 执行总容量约束并移除已取消的前台票据，不改变后台容量。
    /// No inputs; uses real scheduler permits and bounded timeout failures.
    /// 无输入；使用真实调度许可及有界超时失败。
    #[tokio::test]
    async fn internal_query_scheduler_bounds_capacity_and_cleans_cancellation() {
        // The fixture owns every slot so it cannot depend on timing from another test.
        // 夹具持有全部槽位，不依赖其他测试的时序。
        let scheduler = Arc::new(InferenceWorkloadScheduler::default());
        // Retain all admitted foreground work until deadline and cancellation have been checked.
        // 在检查截止时间和取消之前保留全部已准入前台工作。
        let mut permits = Vec::new();
        for _ in 0..INFERENCE_SCHEDULER_TOTAL_CAPACITY {
            permits.push(
                scheduler
                    .acquire(InferenceWorkloadClass::Interactive, None)
                    .await
                    .expect("bounded foreground permit"),
            );
        }
        assert!(
            scheduler
                .acquire(
                    InferenceWorkloadClass::Interactive,
                    Some(current_unix_millis().saturating_add(5))
                )
                .await
                .is_err()
        );
        // Dropping an enqueued request mirrors cooperative cancellation before model dispatch.
        // 丢弃已排队请求对应模型派发前的协作取消。
        let mut pending = Box::pin(scheduler.acquire(InferenceWorkloadClass::Interactive, None));
        assert!(futures_util::poll!(pending.as_mut()).is_pending());
        drop(pending);
        assert!(
            scheduler
                .state
                .lock()
                .expect("cancelled queue")
                .interactive
                .is_empty()
        );
        drop(permits);
        assert_eq!(
            scheduler
                .state
                .lock()
                .expect("released capacity")
                .active_total,
            0
        );
    }

    /// Scheduling grants cannot change route authorization or promote externally authenticated callers.
    /// 调度授权不能改变路由授权或提升外部鉴权调用方。
    /// No inputs; verifies the shared caller authorization and default scheduling contract.
    /// 无输入；验证共同调用方授权及默认调度契约。
    #[test]
    fn internal_query_scheduling_preserves_authorization() {
        // Begin with the same caller used by existing internal inference tests.
        // 从既有内部推理测试使用的同一调用方开始。
        let mut caller = test_caller();
        assert_eq!(
            InferenceWorkloadClass::from_caller(&caller),
            InferenceWorkloadClass::Background
        );
        caller.scheduling_class = vulcan_ai_contract::InferenceSchedulingClass::InternalQuery;
        assert_eq!(
            InferenceWorkloadClass::from_caller(&caller),
            InferenceWorkloadClass::Interactive
        );
        assert!(
            caller
                .authorize("vmm", InferencePermission::Embedding)
                .is_ok()
        );
        assert!(
            caller
                .authorize("unowned", InferencePermission::Embedding)
                .is_err()
        );
        for scope in [
            InferenceUsageScope::ExternalService,
            InferenceUsageScope::InteractiveAgent,
        ] {
            caller.usage_scope = scope;
            assert!(
                caller
                    .authorize("vmm", InferencePermission::Embedding)
                    .is_err()
            );
        }
    }

    /// Reject forged scheduling grants while allowing separately registered query authority over one profile.
    /// 拒绝伪造调度权限，同时允许在同一配置档上单独注册查询权限。
    /// No inputs; exercises the real host registry without credentials, model requests or databases.
    /// 无输入；执行真实宿主注册表，不访问凭据、模型或数据库。
    #[test]
    fn internal_query_scheduling_rejects_forged_capability_handles() {
        use vulcan_ai_contract::{
            HostAiCapabilityRegistration, HostAiPort, InferenceSchedulingClass,
        };
        // An empty private profile admits status inspection without enabling any operation.
        // 空私有配置档允许状态检查，不启用任何操作。
        let runtime = Arc::new(PublicAiRuntime::unavailable().expect("private runtime"));
        // No persisted configuration or model profile is changed by this fixture.
        // 此夹具不改变任何持久化配置或模型配置档。
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .inference_profiles
            .insert("vmm".into(), InferenceProfile::default());
        runtime
            .replace_configuration(configuration)
            .expect("private profile");
        // The production host owns the exact registry used to validate returned handles.
        // 生产宿主持有用于验证返回句柄的精确注册表。
        let port = crate::internal_ai::ConfiguredInferenceHostAiPort::new(Arc::new(
            ConfiguredInferenceService::new(runtime),
        ));
        // Keep the existing authorization scope and explicitly grant status inspection.
        // 保持既有授权用途并明确授予状态检查权限。
        let mut caller = test_caller();
        caller.permissions.insert(InferencePermission::Status);
        // The background registration freezes its scheduling class with the caller identity.
        // 后台注册将调度类别与调用方身份一起冻结。
        let registration = HostAiCapabilityRegistration {
            registration_id: "scheduling-index".into(),
            owner_id: "scheduling-fixture".into(),
            consumer_profile_id: "vmm".into(),
            caller,
            requirements: Vec::new(),
        };
        // Own a second trusted registration rather than mutating a previously issued handle.
        // 持有第二份受信注册，而不是篡改已签发句柄。
        let mut query_registration = registration.clone();
        query_registration.registration_id = "scheduling-query".into();
        query_registration.caller.scheduling_class = InferenceSchedulingClass::InternalQuery;
        // This exact handle is registered as background and must not be promoted by its holder.
        // 此精确句柄注册为后台，持有方不得将其提升。
        let mut forged = port
            .register_capability(registration)
            .expect("index registration");
        forged.caller.scheduling_class = InferenceSchedulingClass::InternalQuery;
        assert_eq!(
            port.capability_snapshot(&forged)
                .expect_err("forged promotion rejected")
                .code,
            vulcan_ai_contract::HostAiErrorCode::PermissionDenied
        );
        // The query capability is independently revocable and cannot revoke its background peer.
        // 查询能力可独立撤销，不得撤销其后台同伴。
        let query = port
            .register_capability(query_registration)
            .expect("query registration");
        assert!(port.capability_snapshot(&query).is_ok());
        port.unregister_capability(&query.registration_id)
            .expect("query revocation");
        assert!(port.capability_snapshot(&query).is_err());
        assert_eq!(
            port.capability_handle("scheduling-index")
                .expect("index survives")
                .caller
                .scheduling_class,
            InferenceSchedulingClass::ByUsageScope
        );
    }

    /// Verifies queue deadlines remove abandoned tickets without leaking capacity.
    /// 验证队列截止时间会移除被放弃票据且不会泄漏容量。
    #[tokio::test]
    async fn workload_scheduler_deadline_cleans_waiting_ticket() {
        let scheduler = Arc::new(InferenceWorkloadScheduler::default());
        let first = scheduler
            .acquire(InferenceWorkloadClass::Background, None)
            .await
            .expect("first background slot");
        let error = match scheduler
            .acquire(
                InferenceWorkloadClass::Background,
                Some(current_unix_millis().saturating_add(5)),
            )
            .await
        {
            Ok(_) => panic!("second background slot must time out"),
            Err(error) => error,
        };
        assert_eq!(error.category, ConfiguredInferenceErrorCategory::Limit);
        {
            let state = scheduler.state.lock().expect("scheduler state");
            assert!(state.background.is_empty());
            assert_eq!(state.active_background, 1);
        }
        drop(first);
        let state = scheduler.state.lock().expect("scheduler state");
        assert_eq!(state.active_total, 0);
        assert_eq!(state.active_background, 0);
    }

    #[test]
    fn route_resolution_never_falls_back_between_llm_purposes() {
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .inference_profiles
            .insert(String::from("vmm"), InferenceProfile::default());
        let error = configured_operation_route(
            &configuration,
            "vmm",
            AiOperation::ConversationRespond,
            Some("precheck_l1"),
        )
        .expect_err("an unconfigured purpose must fail");
        assert_eq!(
            error.category,
            ConfiguredInferenceErrorCategory::Configuration
        );
        assert!(error.message.contains("precheck_l1"));
    }

    /// Verifies VMM eligibility accepts only exact contracts that can disable reasoning.
    /// 验证 VMM 资格仅接受能够关闭思考的精确契约。
    #[test]
    fn vmm_reasoning_policy_requires_explicit_disabled_support() {
        let fixed_off = ModelCapabilities {
            reasoning_control: ReasoningControlMode::FixedOff,
            ..ModelCapabilities::default()
        };
        assert!(model_supports_disabled_reasoning(&fixed_off));
        let mut provider_parameters = BTreeMap::from([(
            String::from("provider.reasoning_budget"),
            ModelParameterValue::Integer(4_096),
        )]);
        let effective_effort = apply_restricted_reasoning_policy(
            VMM_CONSUMER_PROFILE_ID,
            "precheck_l1",
            &fixed_off,
            &BTreeSet::from([String::from("provider.reasoning_budget")]),
            &mut provider_parameters,
            Some(ReasoningEffort::High),
        )
        .expect("VMM should override caller reasoning controls");
        assert_eq!(effective_effort, Some(ReasoningEffort::None));
        assert!(
            !provider_parameters.contains_key("provider.reasoning_budget"),
            "VMM must remove every manifest-declared conflicting reasoning budget"
        );
        let mut postaction_parameters = BTreeMap::new();
        let postaction_effort = apply_restricted_reasoning_policy(
            VMM_CONSUMER_PROFILE_ID,
            "postaction_l2",
            &fixed_off,
            &BTreeSet::new(),
            &mut postaction_parameters,
            Some(ReasoningEffort::High),
        )
        .expect("every VMM purpose should force the proven disabled-reasoning route");
        assert_eq!(postaction_effort, Some(ReasoningEffort::None));

        let toggle = ModelCapabilities {
            reasoning_control: ReasoningControlMode::Toggle,
            reasoning_efforts: vec![ReasoningEffort::None, ReasoningEffort::High],
            ..ModelCapabilities::default()
        };
        assert!(model_supports_disabled_reasoning(&toggle));

        let fixed_on = ModelCapabilities {
            reasoning_control: ReasoningControlMode::FixedOn,
            reasoning_efforts: vec![ReasoningEffort::High],
            ..ModelCapabilities::default()
        };
        assert!(!model_supports_disabled_reasoning(&fixed_on));

        let unknown = ModelCapabilities::default();
        assert!(!model_supports_disabled_reasoning(&unknown));
    }

    /// Verifies every restricted consumer owns Manifest-conflicting provider controls when it
    /// explicitly selects a reasoning effort.
    /// 验证每个受限消费者显式选择思考档位时都会接管与 Manifest 冲突的供应商控制项。
    #[test]
    fn explicit_reasoning_selection_removes_conflicting_parameters_for_every_consumer() {
        let capabilities = ModelCapabilities {
            reasoning_control: ReasoningControlMode::Toggle,
            reasoning_efforts: vec![ReasoningEffort::None, ReasoningEffort::High],
            ..ModelCapabilities::default()
        };
        let mut provider_parameters = BTreeMap::from([
            (
                String::from("provider.reasoning_budget"),
                ModelParameterValue::Integer(4_096),
            ),
            (
                String::from("provider.temperature"),
                ModelParameterValue::Decimal(String::from("0.2")),
            ),
        ]);

        let effective_effort = apply_restricted_reasoning_policy(
            "vce",
            "query_rewrite",
            &capabilities,
            &BTreeSet::from([String::from("provider.reasoning_budget")]),
            &mut provider_parameters,
            Some(ReasoningEffort::None),
        )
        .expect("an explicit shared reasoning selection should own conflicting parameters");

        assert_eq!(effective_effort, Some(ReasoningEffort::None));
        assert!(!provider_parameters.contains_key("provider.reasoning_budget"));
        assert!(provider_parameters.contains_key("provider.temperature"));
    }

    #[test]
    fn caller_authorization_requires_exact_profile_and_scope_identity() {
        let caller = test_caller();
        authorize_caller(&caller, "vmm", InferencePermission::Embedding)
            .expect("configured profile should be authorized");
        let error = authorize_caller(&caller, "plugin", InferencePermission::Embedding)
            .expect_err("another profile must be rejected");
        assert_eq!(
            error.category,
            ConfiguredInferenceErrorCategory::Authorization
        );
    }

    /// Verifies capability negotiation rejects routes outside the caller's trusted usage scope.
    /// 验证能力协商会拒绝超出调用方可信用途范围的路由。
    #[test]
    fn capability_snapshot_marks_usage_scope_mismatch_unavailable() {
        let runtime = PublicAiRuntime::unavailable()
            .expect("unavailable runtime should remain constructible");
        let service = ConfiguredInferenceService::new(Arc::new(runtime));
        let route = InferenceRouteConfiguration {
            provider_id: String::from("provider"),
            service_entry_id: String::from("entry"),
            connection_id: String::from("connection"),
            model_id: String::from("model"),
            binding: InferenceRouteBinding::Action {
                action_binding_id: String::from("action"),
            },
            operation: AiOperation::EmbeddingCreate,
            usage_scope: InferenceUsageScope::InteractiveAgent,
        };

        let capability = service.operation_capability(
            &PersistedAiConfiguration::default(),
            &route,
            None,
            InferenceUsageScope::ExternalService,
        );

        assert!(!capability.available);
        assert!(
            capability
                .unavailable_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("does not match caller scope"))
        );
    }

    #[test]
    fn llm_input_budget_uses_exact_utf8_bytes() {
        let request = ResponsesExecutionRequest {
            contract_version: INFERENCE_CONTRACT_VERSION,
            request_id: String::from("request-1"),
            idempotency_key: None,
            consumer_profile_id: String::from("vmm"),
            purpose_id: String::from("precheck_l1"),
            messages: vec![vulcan_ai_contract::AiMessage {
                role: vulcan_ai_contract::MessageRole::User,
                content: vec![ContentItem::Text {
                    text: String::from("向量"),
                }],
                provider_context_items: Vec::new(),
            }],
            tools: Vec::new(),
            native_tools: Vec::new(),
            tool_choice: Default::default(),
            parallel_tool_calls: Default::default(),
            structured_output: Default::default(),
            prompt_cache: Default::default(),
            prompt_cache_key: None,
            include_reasoning_encrypted_content: false,
            reasoning_effort: None,
            reasoning_output: Default::default(),
            model_parameters: BTreeMap::new(),
            stop_sequences: Vec::new(),
            resources: Vec::new(),
            output_token_limit: None,
        };
        let expected = serde_json::to_vec(&request)
            .expect("request should serialize")
            .len() as u64;
        assert_eq!(
            llm_input_units(&request).expect("input should fit"),
            expected
        );
    }

    /// Verifies internal inference terminal telemetry preserves cache and token accounting.
    /// 验证内部推理终态遥测会保留缓存与 Token 计量。
    #[test]
    fn internal_inference_terminal_payload_preserves_usage_accounting() {
        let known = |value| UsageValue::Known {
            value,
            source: vulcan_ai_contract::UsageValueSource::Reported,
        };
        let facts = InternalInferenceTelemetryFacts {
            request_id: String::from("request-vmm-1"),
            execution_id: String::from("2f322d48-09c6-4b9e-b9a5-342b62a22b92"),
            caller_id: String::from("vulcan-code-native-vmm"),
            consumer_profile_id: String::from("vmm"),
            usage_scope: InferenceUsageScope::InternalBackground,
            owner_kind: "vmm",
            provider_id: String::from("provider-1"),
            provider_name: String::from("Provider One"),
            service_entry_id: String::from("service-1"),
            connection_id: String::from("connection-1"),
            protocol_profile_id: Some(String::from("protocol-1")),
            action_binding_id: Some(String::from("embedding.batch.v1")),
            secondary_route_id: None,
            secondary_provider_name: None,
            secondary_endpoint_name: None,
            model_id: String::from("model-1"),
            model_name: String::from("Model One"),
            operation: String::from("llm.generate"),
            capability_revision: String::from("capability-1"),
            catalog_revision: String::from("catalog-1"),
            route_revision: String::from("sha256:route-1"),
            started_at_unix_ms: 1_000,
            first_response_at_unix_ms: Some(1_125),
            first_token_at_unix_ms: Some(1_125),
            duration_ms: 750,
            retry_count: 1,
            transport: Some(vulcan_ai_contract::AiTransportKind::Http),
            dispatched_at_ms: Some(1_010),
            headers_at_ms: Some(1_050),
            transport_http_status: Some(200),
            websocket_close_code: None,
            provider_request_id: Some(String::from("provider-request-1")),
            rate_limit_headers: BTreeMap::new(),
            status: "succeeded",
            usage: RequestUsage {
                input_tokens: known(100),
                uncached_input_tokens: known(30),
                output_tokens: known(20),
                reasoning_tokens: known(5),
                cache_read_tokens: known(70),
                cache_write_tokens: known(8),
                total_tokens: known(125),
                ..RequestUsage::default()
            },
            input_item_count: Some(2),
            output_item_count: Some(1),
            vector_dimensions: None,
            error_category: None,
            provider_error: None,
        };

        let payload = internal_inference_terminal_payload(&facts, 1_750);

        assert_eq!(payload["usage_scope"], "internal_background");
        assert_eq!(payload["owner_kind"], "vmm");
        assert_eq!(payload["input_tokens"], 100);
        assert_eq!(payload["uncached_input_tokens"], 30);
        assert_eq!(payload["cached_input_tokens"], 70);
        assert_eq!(payload["cache_creation_input_tokens"], 8);
        assert_eq!(payload["output_tokens"], 20);
        assert_eq!(payload["reasoning_tokens"], 5);
        assert_eq!(payload["total_tokens"], 125);
        assert_eq!(payload["time_to_first_token_ms"], 125);
        assert_eq!(payload["retry_count"], 1);
        assert_eq!(payload["transport"], "http");
        assert_eq!(payload["dispatched_at_ms"], 1_010);
        assert_eq!(payload["headers_at_ms"], 1_050);
        assert_eq!(payload["http_status"], 200);
        assert_eq!(payload["provider_request_id"], "provider-request-1");
        assert_eq!(payload["route_revision"], "sha256:route-1");
        assert_eq!(payload["action_binding_id"], "embedding.batch.v1");
        // native_event proves a caller-defined request id no longer has to parse as the owning UUID.
        // native_event 证明调用方自定义请求标识不再需要被解析为所属 UUID。
        let native_event = internal_inference_native_usage_event(&facts, payload)
            .expect("native usage event should use the independent execution UUID");
        assert_eq!(native_event.session_id.to_string(), facts.execution_id);
        assert_eq!(native_event.payload["request_id"], facts.request_id);
    }

    /// Verifies unknown usage dimensions remain null in native and shadow telemetry.
    /// 验证未知用量维度在原生与影子遥测中保持为空。
    #[test]
    fn internal_inference_terminal_payload_preserves_unknown_usage() {
        let facts = InternalInferenceTelemetryFacts::from_route(
            "caller",
            "consumer",
            "request",
            "00000000-0000-0000-0000-000000000000",
            "internal_llm",
            &ExecutionRouteSnapshot {
                provider_id: String::from("provider"),
                service_entry_id: String::from("service"),
                connection_id: String::from("connection"),
                protocol_binding_id: Some(String::from("openai.responses.v1")),
                action_binding_id: None,
                secondary_route_id: None,
                secondary_provider_name: None,
                secondary_endpoint_name: None,
                model_id: String::from("model"),
                capability_revision: String::from("capability"),
                catalog_revision: String::from("catalog"),
                route_revision: String::from("sha256:route"),
                usage_scope: InferenceUsageScope::InternalBackground,
            },
            1_000,
            10,
            "succeeded",
            &RequestUsage::default(),
            None,
        );
        let initial_attempt = facts.clone().with_retry_count(0);
        let retry_attempt = facts.clone().with_retry_count(1);
        assert_eq!(initial_attempt.execution_id, retry_attempt.execution_id);
        assert_eq!(
            initial_attempt.request_id,
            format!("{}:attempt:0", Uuid::nil())
        );
        assert_eq!(
            retry_attempt.request_id,
            format!("{}:attempt:1", Uuid::nil())
        );

        let payload = internal_inference_terminal_payload(&facts, 1_010);

        for field in [
            "input_tokens",
            "uncached_input_tokens",
            "output_tokens",
            "reasoning_tokens",
            "total_tokens",
        ] {
            assert!(payload[field].is_null(), "{field} must remain unknown");
        }
    }

    #[test]
    fn output_limit_is_narrowed_by_every_trusted_ceiling() {
        assert_eq!(
            narrow_output_limit(Some(1_000), [Some(850), Some(800), Some(900), Some(700)]),
            Some(700)
        );
        assert_eq!(
            narrow_output_limit(None, [Some(650), None, Some(900), Some(700)]),
            Some(650)
        );
        assert_eq!(
            narrow_output_limit(None, [None, None, Some(900), Some(700)]),
            Some(700)
        );
    }

    /// Verify cancelling queued work releases its caller reservation in every scheduling lane.
    /// 验证取消排队工作会在每条调度通道归还其调用方预留。
    /// No inputs or output; real permits and explicit future polling avoid provider calls and timing races.
    /// 无输入或输出；真实许可与显式协程轮询避免供应商调用和计时竞争。
    #[tokio::test]
    async fn queued_caller_permit_cancellation_releases_reservation() {
        // Cover background indexing, host-issued foreground queries, and the unchanged external lane.
        // 覆盖后台索引、宿主签发的前台查询及保持原规则的外部通道。
        for (usage_scope, scheduling_class, capacity) in [
            (
                InferenceUsageScope::InternalBackground,
                vulcan_ai_contract::InferenceSchedulingClass::ByUsageScope,
                INFERENCE_SCHEDULER_BACKGROUND_CAPACITY,
            ),
            (
                InferenceUsageScope::InternalBackground,
                vulcan_ai_contract::InferenceSchedulingClass::InternalQuery,
                INFERENCE_SCHEDULER_TOTAL_CAPACITY,
            ),
            (
                InferenceUsageScope::ExternalService,
                vulcan_ai_contract::InferenceSchedulingClass::ByUsageScope,
                INFERENCE_SCHEDULER_EXTERNAL_CAPACITY,
            ),
        ] {
            // This private service cannot dispatch to any configured provider or user storage.
            // 此私有服务不能派发到任何已配置供应商或用户存储。
            let service = ConfiguredInferenceService::new(Arc::new(
                PublicAiRuntime::unavailable().expect("private runtime"),
            ));
            // The exact caller budget remains shared across every reservation of this identity.
            // 此身份的每次预留都共享精确调用方预算。
            let mut caller = test_caller();
            caller.usage_scope = usage_scope;
            caller.scheduling_class = scheduling_class;
            // Freeze the lane from the same host-authorized context used in production.
            // 从生产使用的同一宿主授权上下文冻结通道。
            let class = InferenceWorkloadClass::from_caller(&caller);
            // Retain all lane capacity so each tested request must really enter the queue.
            // 保留全部通道容量，使每个被测请求都必须真实进入队列。
            let mut blockers = Vec::new();
            for _ in 0..capacity {
                blockers.push(
                    service
                        .workload_scheduler
                        .acquire(class, None)
                        .await
                        .expect("blocker"),
                );
            }
            for _ in 0..3 {
                // Polling once establishes the actual reservation before cancellation drops the future.
                // 单次轮询在取消丢弃协程前建立真实预留。
                let mut queued = Box::pin(service.acquire_caller_permit(&caller, 1));
                assert!(futures_util::poll!(queued.as_mut()).is_pending());
                assert_eq!(
                    service
                        .caller_execution_counts
                        .lock()
                        .expect("counts")
                        .get(&caller.caller_id)
                        .copied(),
                    Some(1)
                );
                assert_eq!(
                    service
                        .workload_scheduler
                        .state
                        .lock()
                        .expect("queue")
                        .queue(class)
                        .len(),
                    1
                );
                drop(queued);
                assert!(
                    service
                        .caller_execution_counts
                        .lock()
                        .expect("released counts")
                        .is_empty(),
                    "cancelled queue must release caller reservation: {class:?}"
                );
                assert!(
                    service
                        .workload_scheduler
                        .state
                        .lock()
                        .expect("released queue")
                        .queue(class)
                        .is_empty()
                );
                assert!(
                    service
                        .caller_rate_windows
                        .lock()
                        .expect("uncharged rate")
                        .is_empty()
                );
            }
            drop(blockers);
            // The same identity must remain usable after more cancellations than its concurrency budget.
            // 取消次数超过并发预算后，同一身份仍必须可用。
            let recovered = service
                .acquire_caller_permit(&caller, 1)
                .await
                .expect("recovered caller");
            drop(recovered);
            assert!(
                service
                    .caller_execution_counts
                    .lock()
                    .expect("final counts")
                    .is_empty()
            );
            assert_eq!(
                service
                    .workload_scheduler
                    .state
                    .lock()
                    .expect("final capacity")
                    .active_total,
                0
            );
        }
    }

    /// Verify failed or completed admission releases only its own reservation and scheduler slot.
    /// 验证失败或完成的准入仅释放其自身预留和调度槽位。
    /// No inputs or output; assertions cover deadline, rate, concurrency rejection, and retained peers.
    /// 无输入或输出；断言覆盖截止时间、速率、并发拒绝和保留同伴。
    #[tokio::test]
    async fn caller_permit_failures_and_completion_preserve_peer_reservations() {
        // All counters and scheduler permits belong to this private service.
        // 全部计数器和调度许可属于此私有服务。
        let service = ConfiguredInferenceService::new(Arc::new(
            PublicAiRuntime::unavailable().expect("private runtime"),
        ));
        // Keep the first admitted execution alive while the same caller queues another request.
        // 同一调用方排队另一项请求时，保持首项已准入执行存活。
        let mut caller = test_caller();
        caller.budget.max_concurrent_executions = 2;
        caller.budget.max_requests_per_minute = Some(1);
        // This live owner must survive every rejected or cancelled sibling request below.
        // 此存活所有者必须跨越以下每个被拒绝或取消的同伴请求。
        let active = service
            .acquire_caller_permit(&caller, 1)
            .await
            .expect("active owner");
        // The second reservation cannot acquire the occupied background slot.
        // 第二项预留不能获取已占用的后台槽位。
        let mut queued = Box::pin(service.acquire_caller_permit(&caller, 1));
        assert!(futures_util::poll!(queued.as_mut()).is_pending());
        // A rejected third reservation must not decrement either existing owner.
        // 被拒绝的第三项预留不得递减任何既有所有者。
        let rejected = service
            .acquire_caller_permit(&caller, 1)
            .await
            .err()
            .expect("concurrency rejection");
        assert!(rejected.message.contains("concurrent execution limit"));
        assert_eq!(
            service
                .caller_execution_counts
                .lock()
                .expect("two owners")
                .get(&caller.caller_id)
                .copied(),
            Some(2)
        );
        drop(queued);
        assert_eq!(
            service
                .caller_execution_counts
                .lock()
                .expect("retained owner")
                .get(&caller.caller_id)
                .copied(),
            Some(1)
        );
        // An already-expired deadline fails while waiting without disturbing the running owner.
        // 已过期截止时间在等待时失败，不干扰运行中的所有者。
        caller.deadline_unix_ms = Some(0);
        let expired = service
            .acquire_caller_permit(&caller, 1)
            .await
            .err()
            .expect("deadline rejection");
        assert!(expired.message.contains("deadline expired"));
        assert_eq!(
            service
                .caller_execution_counts
                .lock()
                .expect("owner after deadline")
                .get(&caller.caller_id)
                .copied(),
            Some(1)
        );
        caller.deadline_unix_ms = None;
        drop(active);
        // The rate rejection happens after scheduling, so both independently owned resources must unwind.
        // 速率拒绝发生在调度后，因此两个独立持有的资源都必须释放。
        let rate_limited = service
            .acquire_caller_permit(&caller, 1)
            .await
            .err()
            .expect("rate rejection");
        assert!(rate_limited.message.contains("per-minute request"));
        assert!(
            service
                .caller_execution_counts
                .lock()
                .expect("final counts")
                .is_empty()
        );
        assert_eq!(
            service
                .workload_scheduler
                .state
                .lock()
                .expect("final scheduler")
                .active_total,
            0
        );
        assert_eq!(
            service
                .caller_rate_windows
                .lock()
                .expect("one admitted charge")
                .get(&caller.caller_id)
                .expect("caller rate")
                .request_count,
            1
        );
    }

    /// Verifies caller rolling limits are reserved atomically before provider dispatch.
    /// 验证调用方滚动限额在供应商分派前完成原子预留。
    #[test]
    fn caller_rate_budget_rejects_the_next_request_without_dispatch() {
        let runtime = PublicAiRuntime::unavailable()
            .expect("unavailable runtime should remain constructible");
        let service = ConfiguredInferenceService::new(Arc::new(runtime));
        let mut caller = test_caller();
        caller.budget.max_requests_per_minute = Some(1);
        caller.budget.max_input_units_per_minute = Some(10);

        service
            .reserve_caller_rate_budget(&caller, 6)
            .expect("first request should reserve");
        let error = service
            .reserve_caller_rate_budget(&caller, 1)
            .expect_err("second request must exceed the request count");

        assert_eq!(error.category, ConfiguredInferenceErrorCategory::Limit);
        assert!(error.message.contains("per-minute request"));
    }

    /// Verifies inactive caller rate windows are removed before a new reservation.
    /// 验证新的预留发生前会清除非活动调用方的速率窗口。
    #[test]
    fn caller_rate_budget_prunes_expired_windows() {
        let runtime = PublicAiRuntime::unavailable()
            .expect("unavailable runtime should remain constructible");
        let service = ConfiguredInferenceService::new(Arc::new(runtime));
        let now = current_unix_millis();
        service
            .caller_rate_windows
            .lock()
            .expect("rate windows lock should remain available")
            .insert(
                String::from("expired-caller"),
                CallerRateWindow::new(now.saturating_sub(60_001)),
            );

        service
            .reserve_caller_rate_budget(&test_caller(), 1)
            .expect("active caller should reserve");

        let windows = service
            .caller_rate_windows
            .lock()
            .expect("rate windows lock should remain available");
        assert!(!windows.contains_key("expired-caller"));
        assert!(windows.contains_key("vmm-test"));
    }

    /// Verifies concurrent starts for one idempotency key share one live serialization lock.
    /// 验证同一幂等键的并发启动共享一项存活的串行化锁。
    #[test]
    fn llm_start_lock_is_key_scoped_and_reclaims_inactive_entries() {
        let runtime = PublicAiRuntime::unavailable()
            .expect("unavailable runtime should remain constructible");
        let service = ConfiguredInferenceService::new(Arc::new(runtime));

        let first = service
            .llm_start_lock("caller:request")
            .expect("first lock should resolve");
        let shared = service
            .llm_start_lock("caller:request")
            .expect("same key should resolve");
        let independent = service
            .llm_start_lock("caller:other")
            .expect("different key should resolve");

        assert!(Arc::ptr_eq(&first, &shared));
        assert!(!Arc::ptr_eq(&first, &independent));
        drop(first);
        drop(shared);
        let reclaimed = service
            .llm_start_lock("caller:request")
            .expect("inactive key should be recreated");
        assert!(!Arc::ptr_eq(&reclaimed, &independent));
        assert_eq!(Arc::strong_count(&reclaimed), 1);
    }

    /// Verifies exact concurrent duplicates share one result and terminal requests dispatch again.
    /// 验证精确并发重复请求共享一个结果，而终态后的请求会再次分派。
    #[tokio::test]
    async fn non_streaming_in_flight_coalescing_is_exact_and_running_only() {
        let runtime = PublicAiRuntime::unavailable()
            .expect("unavailable runtime should remain constructible");
        let service = ConfiguredInferenceService::new(Arc::new(runtime));
        let registry = Mutex::new(BTreeMap::new());
        let dispatches = Arc::new(AtomicUsize::new(0));
        let first_dispatches = Arc::clone(&dispatches);
        let second_dispatches = Arc::clone(&dispatches);

        let first = service.execute_shared_inference(
            &registry,
            String::from("embedding/key"),
            String::from("same-request"),
            async move {
                first_dispatches.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(5)).await;
                Ok(7_u32)
            },
        );
        let second = service.execute_shared_inference(
            &registry,
            String::from("embedding/key"),
            String::from("same-request"),
            async move {
                second_dispatches.fetch_add(1, Ordering::SeqCst);
                Ok(8_u32)
            },
        );
        let (first_result, second_result) = tokio::join!(first, second);

        assert_eq!(first_result.expect("leader result"), 7);
        assert_eq!(second_result.expect("follower result"), 7);
        assert_eq!(dispatches.load(Ordering::SeqCst), 1);

        let terminal_result = service
            .execute_shared_inference(
                &registry,
                String::from("embedding/key"),
                String::from("same-request"),
                async {
                    dispatches.fetch_add(1, Ordering::SeqCst);
                    Ok(9_u32)
                },
            )
            .await
            .expect("terminal key should be reusable");
        assert_eq!(terminal_result, 9);
        assert_eq!(dispatches.load(Ordering::SeqCst), 2);
    }

    /// Verifies different content cannot reuse one active non-streaming request identity.
    /// 验证不同内容不能复用同一项活动非流式请求身份。
    #[tokio::test]
    async fn non_streaming_in_flight_coalescing_rejects_conflicting_content() {
        let runtime = PublicAiRuntime::unavailable()
            .expect("unavailable runtime should remain constructible");
        let service = ConfiguredInferenceService::new(Arc::new(runtime));
        let registry = Mutex::new(BTreeMap::new());
        let leader = service.execute_shared_inference(
            &registry,
            String::from("rerank/key"),
            String::from("request-a"),
            async {
                tokio::time::sleep(Duration::from_millis(5)).await;
                Ok(1_u32)
            },
        );
        let conflict = service.execute_shared_inference(
            &registry,
            String::from("rerank/key"),
            String::from("request-b"),
            async { Ok(2_u32) },
        );
        let (leader_result, conflict_result) = tokio::join!(leader, conflict);

        assert_eq!(leader_result.expect("leader result"), 1);
        let error = conflict_result.expect_err("conflicting request must fail");
        assert_eq!(error.category, ConfiguredInferenceErrorCategory::Contract);
    }

    /// Verifies one-shot native inference obeys the same absolute deadline as LLM streaming.
    /// 验证一次性原生推理遵守与 LLM 流式执行相同的绝对截止时间。
    #[tokio::test]
    async fn one_shot_provider_operation_is_cancelled_at_caller_deadline() {
        let operation = std::future::pending::<
            Result<(), vulcan_provider_adapters::ProviderExecutionFailure>,
        >();
        let error =
            await_provider_attempt_with_deadline(operation, Some(current_unix_millis() + 1))
                .await
                .expect_err("pending provider operation must reach its deadline");

        assert_eq!(error.category, ConfiguredInferenceErrorCategory::Limit);
        assert!(error.message.contains("deadline"));
    }

    /// Verifies a non-idempotent Batch deadline retains an unknown dispatch outcome.
    /// 验证非幂等 Batch 截止时间会保留不确定分派结果。
    #[tokio::test]
    async fn non_idempotent_batch_deadline_preserves_unknown_dispatch() {
        let operation = std::future::pending::<
            Result<(), vulcan_provider_adapters::ProviderExecutionFailure>,
        >();
        let error =
            await_provider_attempt_with_deadline(operation, Some(current_unix_millis() + 1))
                .await
                .expect_err("pending provider operation must reach its deadline")
                .with_unknown_batch_dispatch();
        let provider_error = error
            .provider_error
            .expect("non-idempotent deadline must retain provider semantics");

        assert_eq!(provider_error.category, AiErrorCategory::Timeout);
        assert_eq!(provider_error.dispatch_outcome, DispatchOutcome::Unknown);
        assert!(!provider_error.retryable);
    }

    /// Verifies text-encoded JSON is materialized into the strong terminal result.
    /// 验证文本编码 JSON 会物化为强类型终态结果。
    #[test]
    fn structured_output_materializes_text_encoded_json_object() {
        let mut structured_output = None;

        finalize_restricted_structured_output(
            &StructuredOutputPolicy::JsonObject,
            r#"{"accepted":true}"#,
            &mut structured_output,
            false,
        )
        .expect("valid JSON object should materialize");

        assert_eq!(
            structured_output,
            Some(serde_json::json!({ "accepted": true }))
        );
    }

    /// Verifies reasoning deltas remain isolated from text-decoded structured JSON.
    /// 验证思考增量与由文本解码的结构化 JSON 始终隔离。
    #[test]
    fn structured_output_never_merges_reasoning_text_into_json() {
        let mut output_text = String::new();
        let mut reasoning_text = String::new();
        let mut structured_output = None;
        let mut output_items = Vec::new();
        let mut usage = RequestUsage::default();

        aggregate_restricted_llm_event(
            AiStreamEvent::ReasoningDelta {
                item_id: String::from("reasoning-1"),
                delta: String::from("private chain of thought"),
            },
            &mut output_text,
            &mut reasoning_text,
            &mut structured_output,
            &mut output_items,
            &mut usage,
        )
        .expect("reasoning delta should aggregate independently");
        aggregate_restricted_llm_event(
            AiStreamEvent::ResponseTextDelta {
                item_id: String::from("text-1"),
                delta: String::from(r#"{"accepted":true}"#),
            },
            &mut output_text,
            &mut reasoning_text,
            &mut structured_output,
            &mut output_items,
            &mut usage,
        )
        .expect("response text delta should aggregate independently");
        finalize_restricted_structured_output(
            &StructuredOutputPolicy::JsonObject,
            &output_text,
            &mut structured_output,
            false,
        )
        .expect("response JSON should materialize without reasoning text");

        assert_eq!(reasoning_text, "private chain of thought");
        assert_eq!(output_text, r#"{"accepted":true}"#);
        assert_eq!(
            structured_output,
            Some(serde_json::json!({ "accepted": true }))
        );
    }

    /// Verifies a provider JSON code fence remains parseable before schema validation.
    /// 验证供应商返回的 JSON 代码围栏会在 Schema 校验前保持可解析。
    #[test]
    fn structured_output_materializes_fenced_json_object() {
        let mut structured_output = None;

        finalize_restricted_structured_output(
            &StructuredOutputPolicy::JsonObject,
            "```json\n{\"accepted\":true}\n```",
            &mut structured_output,
            false,
        )
        .expect("a complete JSON code fence should materialize");

        assert_eq!(
            structured_output,
            Some(serde_json::json!({ "accepted": true }))
        );
    }

    /// Verifies malformed JSON cannot complete a requested structured execution successfully.
    /// 验证畸形 JSON 无法让已请求结构化输出的执行成功结束。
    #[test]
    fn structured_output_rejects_malformed_json_text() {
        let error = finalize_restricted_structured_output(
            &StructuredOutputPolicy::JsonObject,
            "not-json",
            &mut None,
            false,
        )
        .expect_err("malformed JSON must fail");

        assert_eq!(error.category, ConfiguredInferenceErrorCategory::Execution);
        assert!(error.message.contains("not valid JSON"));
    }

    /// Verifies one JSON object surrounded by provider prose is accepted without weakening schema checks.
    /// 验证供应商说明文字包裹单个 JSON 对象时仍可解析，且不会削弱 Schema 校验。
    #[test]
    fn structured_output_accepts_one_unique_embedded_json_value() {
        let mut structured_output = None;
        finalize_restricted_structured_output(
            &StructuredOutputPolicy::JsonObject,
            "Here is the result: {\"accepted\":true}.",
            &mut structured_output,
            false,
        )
        .expect("one unique embedded JSON object should materialize");
        assert_eq!(
            structured_output,
            Some(serde_json::json!({ "accepted": true }))
        );
    }

    /// Verifies multiple embedded JSON objects are rejected instead of silently choosing one.
    /// 验证多份嵌入 JSON 会被拒绝，而不是静默选择其中一份。
    #[test]
    fn structured_output_rejects_multiple_embedded_json_values() {
        let error = finalize_restricted_structured_output(
            &StructuredOutputPolicy::JsonObject,
            "first {\"accepted\":true}; second {\"accepted\":false}",
            &mut None,
            false,
        )
        .expect_err("multiple embedded JSON objects must fail");
        assert!(error.message.contains("not valid JSON"));
    }

    /// Verifies VMM ignores extra output keys while preserving all other schema checks.
    /// 验证 VMM 忽略多余输出键，同时保留其他全部 Schema 校验。
    #[test]
    fn vmm_schema_ignores_only_additional_output_fields() {
        // This closed schema exercises VMM's object, array, and nullable-object projections.
        // 此封闭 Schema 覆盖 VMM 的对象、数组及可空对象投影。
        let policy = StructuredOutputPolicy::JsonSchema {
            name: String::from("vmm_output"),
            description: None,
            schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "kind": { "type": "string", "enum": ["statement"] },
                    "nodes": { "type": "array", "items": {
                        "type": "object",
                        "properties": { "content": { "type": "string" } },
                        "required": ["content"],
                        "additionalProperties": false
                    } },
                    "context": { "anyOf": [
                        { "type": "object", "properties": { "source": { "type": "string" } },
                          "required": ["source"], "additionalProperties": false },
                        { "type": "null" }
                    ] }
                },
                "required": ["kind", "nodes"],
                "additionalProperties": false
            }),
            strict: true,
        };
        // The provider output carries extra root, nested-array, and nullable-object keys.
        // 模型输出在顶层、嵌套数组及可空对象中都附加了多余键。
        let output = r#"{"kind":"statement","nodes":[{"content":"fact","type":"extra"}],"context":{"source":"user","confidence":1},"type":"extra"}"#;
        // Accepted output is the exact sanitized value passed to VMM's decoder.
        // 接纳的输出是交给 VMM 解码器的准确净化值。
        let mut accepted = None;
        finalize_restricted_structured_output(&policy, output, &mut accepted, true)
            .expect("VMM may discard additional properties");
        assert_eq!(
            accepted,
            Some(serde_json::json!({
                "kind": "statement", "nodes": [{ "content": "fact" }],
                "context": { "source": "user" }
            }))
        );
        // Native provider JSON uses the same sanitizer and produces the same terminal value.
        // 供应商原生 JSON 使用相同的清理逻辑，并产出相同的终态值。
        let mut native = Some(serde_json::from_str(output).expect("fixture JSON"));
        finalize_restricted_structured_output(&policy, "", &mut native, true)
            .expect("native structured output may discard additional properties");
        assert_eq!(native, accepted);
        finalize_restricted_structured_output(&policy, output, &mut None, false)
            .expect_err("other consumers still reject additional properties");

        // Missing required fields and invalid values must still fail after normalization.
        // 规范化后，缺少必填字段或字段值无效仍必须失败。
        for invalid in [
            r#"{"nodes":[],"type":"extra"}"#,
            r#"{"kind":"other","nodes":[],"type":"extra"}"#,
            r#"{"kind":"statement","nodes":[{"content":2,"type":"extra"}],"type":"extra"}"#,
            r#"{"kind":"statement","nodes":[{"type":"extra"}],"type":"extra"}"#,
        ] {
            // Every invalid fixture must still produce a structured execution error.
            // 每个无效样本仍必须产生结构化执行错误。
            let error = finalize_restricted_structured_output(&policy, invalid, &mut None, true)
                .expect_err("normalization must preserve the remaining schema checks");
            assert!(error.message.contains("JSON Schema"));
        }
    }

    /// Verifies JSON Schema output is independently checked after provider completion.
    /// 验证 JSON Schema 输出会在供应商完成后被独立校验。
    #[test]
    fn structured_output_enforces_requested_json_schema() {
        let policy = StructuredOutputPolicy::JsonSchema {
            name: String::from("decision"),
            description: None,
            schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "accepted": { "type": "boolean" }
                },
                "required": ["accepted"],
                "additionalProperties": false
            }),
            strict: true,
        };
        validate_restricted_structured_output_policy(&policy)
            .expect("fixture schema should compile");
        let error = finalize_restricted_structured_output(
            &policy,
            r#"{"accepted":"yes"}"#,
            &mut None,
            false,
        )
        .expect_err("schema-invalid JSON must fail");

        assert_eq!(error.category, ConfiguredInferenceErrorCategory::Execution);
        assert!(error.message.contains("JSON Schema"));
        assert!(error.message.contains("/accepted"));
    }

    /// Verifies protocol selection projects the strongest supported provider-side JSON control.
    /// 验证协议选择会投影提供方侧支持的最强 JSON 控制。
    #[test]
    fn structured_output_projection_preserves_schema_and_requests_json_objects() {
        let policy = StructuredOutputPolicy::JsonSchema {
            name: String::from("decision"),
            description: None,
            schema: serde_json::json!({
                "type": "object",
                "properties": { "accepted": { "type": "boolean" } },
                "required": ["accepted"],
                "additionalProperties": false
            }),
            strict: true,
        };

        assert_eq!(
            structured_output_policy_for_protocol(ANTHROPIC_MESSAGES_PROFILE_ID, &policy),
            policy
        );
        assert_eq!(
            structured_output_policy_for_protocol(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID, &policy),
            StructuredOutputPolicy::JsonObject
        );
        assert_eq!(
            structured_output_policy_for_protocol(
                vulcan_llm_core::GEMINI_GENERATE_CONTENT_PROFILE_ID,
                &policy
            ),
            StructuredOutputPolicy::JsonObject
        );
        assert_eq!(
            structured_output_policy_for_protocol(vulcan_llm_core::COHERE_CHAT_PROFILE_ID, &policy),
            StructuredOutputPolicy::JsonObject
        );
        assert_eq!(
            structured_output_policy_for_protocol("unknown.protocol", &policy),
            StructuredOutputPolicy::None
        );
    }
}
