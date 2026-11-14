//! Shared host-managed tool runtime bridge for the host executor and `vulcan-core` controller.
//! 供宿主执行器与 `vulcan-core` 控制器共享的宿主管理工具运行时桥接层。

use crate::agent_runtime::events::{RuntimeEvent, RuntimeEventKind};
use crate::agent_runtime::tool_permission::{
    PermissionTexts, ToolPermissionHost, ToolPermissionVerdict, admit_tool_call,
};
use crate::model::SessionId;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::sync::Arc;
use std::sync::{Mutex, OnceLock, RwLock};
use std::time::Duration;
use tokio::sync::oneshot;
use uuid::Uuid;
use vulcan_core::tool::VulcanToolExecutor;
use vulcan_core::types::{
    CoreError as VulcanCoreError, ToolInvocationRequest as CoreToolInvocationRequest,
    ToolInvocationResult as CoreToolInvocationResult,
};
use vulcan_session_core::v1::document::PreparedToolReceipt;
use vulcan_session_core::v1::event::ToolPayload;
use vulcan_session_core::v1::ids::RequestStreamId;
use vulcan_tools_manager::{
    ResolvedToolDefinition, VULCAN_TOOL_ERROR_DETAIL_KIND, VulcanToolInvocationContext,
    VulcanToolInvocationResult, VulcanToolsManager,
};

/// One exact value removed from tool results, with the text that replaces it.
/// 从工具结果中移除的一个精确值，以及替换它的文本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SensitiveToolOutputValue {
    /// Exact value to remove.
    /// 需要移除的精确值。
    pub(crate) value: String,
    /// Text written in its place.
    /// 写在其位置上的文本。
    pub(crate) replacement: String,
}

/// Live registry of values removed from every tool result, longest value first so a value that
/// contains another is replaced whole.
/// 从每个工具结果中移除的值的实时注册表，按值从长到短排列，使包含另一值的值被整体替换。
pub(crate) type SensitiveToolOutputValues = Arc<RwLock<Vec<SensitiveToolOutputValue>>>;

/// Shared host-managed tool bridge that backs the host executor and `vulcan-core` host-side tool execution.
/// 同时支撑宿主执行器与 `vulcan-core` 宿主侧工具执行的共享宿主管理工具桥接层。
#[derive(Clone)]
pub struct VulcanManagedToolRuntimeBridge {
    /// Shared host-owned unified tools manager used for the final dispatch.
    /// 用于最终调度的共享宿主自有统一工具管理器。
    manager: Arc<VulcanToolsManager>,
    /// Stable resolved tool definitions indexed by canonical tool identifier.
    /// 以规范工具标识建立索引的稳定已解析工具定义集合。
    definitions_by_id: HashMap<String, ResolvedToolDefinition>,
    /// Workbench session identifier attached to every host-managed tool invocation.
    /// 附着到每一次宿主管理工具调用上的工作台会话标识。
    session_id: SessionId,
    /// Live exact-value filter for host credentials before tool outputs reach Core persistence.
    /// 工具输出进入 Core 持久化前用于宿主凭据的实时精确值过滤器。
    sensitive_tool_output_values: Option<SensitiveToolOutputValues>,
    /// Execution mode label used by invocation-time Plan-mode guards.
    /// 调用期计划模式守卫使用的执行模式标签。
    execution_mode: String,
    /// Durable V1 identities awaiting exact tool-body execution, keyed only by request stream id.
    /// 等待精确工具主体执行的可靠 V1 身份，仅以请求流身份为键。
    prepared_invocation_identities:
        Arc<Mutex<HashMap<RequestStreamId, PreparedToolInvocationIdentity>>>,
    /// Permission host consulted before every dispatch; `None` when no policy is configured.
    /// 每次调度前咨询的权限宿主；未配置策略时为 `None`。
    permission_host: Option<Arc<dyn ToolPermissionHost>>,
    /// Source of live `tool_chain` settings; `None` uses the defaults.
    /// 实时 `tool_chain` 设置来源；为 `None` 时使用默认值。
    tool_chain_host: Option<Arc<dyn ToolChainHost>>,
}

/// Metadata key used to connect one runtime-event projection to its tool-execution barrier.
/// 用于把单条运行时事件投影与工具执行屏障关联起来的元数据键。
pub const RUNTIME_EVENT_PROJECTION_BARRIER_ID: &str = "webui_projection_barrier_id";

/// Exact Core receipt returned by the durable V1 tool-batch owner.
/// 由可靠 V1 工具批次所有者返回的精确 Core 回执。
pub type PreparedToolInvocationIdentity = PreparedToolReceipt;

/// Payload released by one completed runtime-event projection barrier.
/// 单个运行时事件投影屏障完成后释放的载荷。
#[derive(Debug, Clone, PartialEq, Eq)]
enum RuntimeEventProjectionReceipt {
    /// The event was projected and requires no execution identity payload.
    /// 事件已完成投影，且不需要执行身份载荷。
    Projected,
    /// The durable tool batch returned the exact local identities for execution.
    /// 可靠工具批次返回了供执行使用的精确本地身份。
    ToolBatchPrepared(Vec<PreparedToolInvocationIdentity>),
    /// The host rejected the barrier with the original durability failure detail.
    /// 宿主使用原始持久化失败详情拒绝了该屏障。
    Rejected(String),
}

/// One structured projection-barrier failure that must remain distinct from a normal tool error.
/// 一种必须与普通工具错误保持区分的结构化投影屏障失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RuntimeEventProjectionError {
    /// The host explicitly rejected persistence or projection and retained the originating detail.
    /// 宿主明确拒绝了持久化或投影，并保留原始失败详情。
    Rejected {
        /// Original host-side durability or projection failure.
        /// 原始宿主持久化或投影失败详情。
        detail: String,
    },
    /// A mandatory execution barrier exceeded its bounded acknowledgement window.
    /// 强制执行屏障超过了有界确认时间。
    TimedOut,
    /// The host acknowledged a tool-batch barrier with the wrong receipt shape.
    /// 宿主使用错误回执形状确认了工具批次屏障。
    UnexpectedReceipt,
    /// The process-wide barrier registry could not be accessed consistently.
    /// 无法一致访问进程级屏障注册表。
    Registry {
        /// Exact registry operation failure.
        /// 精确的注册表操作失败。
        detail: String,
    },
}

impl Display for RuntimeEventProjectionError {
    /// Formats one stable diagnostic without discarding the original rejection detail.
    /// 格式化一条稳定诊断，且不丢弃原始拒绝详情。
    ///
    /// `formatter` receives the user-diagnosable projection failure text. The function
    /// returns the formatter result and performs no state mutation.
    /// `formatter` 接收用户可诊断的投影失败文本。此函数返回格式化结果且不修改状态。
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rejected { detail } => write!(
                formatter,
                "Tool recovery durability barrier was rejected before execution could advance: {detail}"
            ),
            Self::TimedOut => write!(
                formatter,
                "Tool recovery durability barrier timed out before execution could advance."
            ),
            Self::UnexpectedReceipt => write!(
                formatter,
                "Tool batch durability barrier completed without local V1 invocation identities."
            ),
            Self::Registry { detail } => write!(
                formatter,
                "Tool recovery durability barrier registry failed: {detail}"
            ),
        }
    }
}

impl std::error::Error for RuntimeEventProjectionError {}

/// Stores pending projection barrier acknowledgements by stable barrier id.
/// 按稳定屏障标识保存等待中的投影确认发送端。
type RuntimeEventProjectionBarrierRegistry =
    HashMap<String, oneshot::Sender<RuntimeEventProjectionReceipt>>;

/// Process-wide pending projection barrier acknowledgements keyed by stable barrier id.
/// 以稳定屏障标识为键的进程级待确认投影屏障集合。
static RUNTIME_EVENT_PROJECTION_BARRIERS: OnceLock<Mutex<RuntimeEventProjectionBarrierRegistry>> =
    OnceLock::new();

/// Schema version of every tool-binding description hashed into the recovery fingerprint.
/// 进入恢复指纹哈希的每份工具绑定描述的结构版本。
const TOOL_BINDING_FINGERPRINT_SCHEMA_VERSION: u32 = 1;

/// Maximum time a tool may wait for the UI projection acknowledgement before continuing.
/// 工具等待界面投影确认后继续执行的最长时间。
const RUNTIME_EVENT_PROJECTION_BARRIER_TIMEOUT: Duration = Duration::from_secs(8);

/// One pending projection barrier created for a runtime event.
/// 为一条运行时事件创建的待确认投影屏障。
pub(crate) struct RuntimeEventProjectionBarrier {
    /// Stable barrier id copied into runtime-event metadata.
    /// 复制到运行时事件元数据中的稳定屏障标识。
    id: String,
    /// Whether acknowledgement is a mandatory safety gate before external execution may begin.
    /// 确认是否属于外部执行开始前必须通过的安全门禁。
    required_for_execution: bool,
    /// Receiver resolved by the host bridge after it emits the projected workbench snapshot.
    /// 宿主桥接层完成投影快照发送后解析的接收端。
    receiver: oneshot::Receiver<RuntimeEventProjectionReceipt>,
}

impl VulcanManagedToolRuntimeBridge {
    /// Builds one shared runtime bridge from the current tools manager, resolved definitions, and workbench session.
    /// 根据当前工具管理器、已解析定义与工作台会话构建共享运行时桥接层。
    pub fn new(
        manager: Arc<VulcanToolsManager>,
        definitions: &[ResolvedToolDefinition],
        session_id: SessionId,
        execution_mode: impl Into<String>,
    ) -> Self {
        let definitions_by_id = definitions
            .iter()
            .cloned()
            .map(|definition| (definition.descriptor.id.clone(), definition))
            .collect::<HashMap<_, _>>();

        Self {
            manager,
            definitions_by_id,
            session_id,
            sensitive_tool_output_values: None,
            execution_mode: execution_mode.into(),
            prepared_invocation_identities: Arc::new(Mutex::new(HashMap::new())),
            permission_host: None,
            tool_chain_host: None,
        }
    }

    /// Installs the source of live `tool_chain` settings read when a chain is called.
    /// 安装在调用链条时读取的实时 `tool_chain` 设置来源。
    pub(crate) fn with_tool_chain_host(mut self, host: Option<Arc<dyn ToolChainHost>>) -> Self {
        self.tool_chain_host = host;
        self
    }

    /// Installs the permission host that admits or denies each call before dispatch.
    /// 安装在调度前放行或拒绝每次调用的权限宿主。
    pub(crate) fn with_tool_permission_host(
        mut self,
        host: Option<Arc<dyn ToolPermissionHost>>,
    ) -> Self {
        self.permission_host = host;
        self
    }

    /// Installs the current runtime's live host-credential filter on this invocation bridge.
    /// 在此调用桥上安装当前运行时的实时宿主凭据过滤器。
    ///
    /// `values` is shared with the service bootstrap; the returned bridge applies it to every result.
    /// `values` 与服务引导共享；返回的桥对每个结果应用该过滤器。
    pub(crate) fn with_sensitive_tool_output_values(
        mut self,
        values: SensitiveToolOutputValues,
    ) -> Self {
        self.sensitive_tool_output_values = Some(values);
        self
    }

    /// Removes exact host credentials from both model text and user-visible structured details.
    /// 从模型文本和用户可见的结构化详情中移除精确宿主凭据。
    ///
    /// `result` is the canonical output; a poisoned filter fails closed before persistence.
    /// `result` 是规范输出；过滤器损坏时在持久化前闭锁失败。
    fn redact_sensitive_tool_result(
        &self,
        mut result: CoreToolInvocationResult,
    ) -> Result<CoreToolInvocationResult, VulcanCoreError> {
        let mut model_output = Value::String(std::mem::take(&mut result.model_output));
        self.redact_sensitive_value(&mut model_output)?;
        if let Value::String(text) = model_output {
            result.model_output = text;
        }
        if let Some(host_result) = result.host_result.as_mut() {
            self.redact_sensitive_value(host_result)?;
        }
        Ok(result)
    }

    /// Removes exact host credentials from every string leaf of `value` in place.
    /// 就地从 `value` 的每个字符串叶节点移除精确宿主凭据。
    ///
    /// A poisoned filter fails closed, so nothing unfiltered is passed on.
    /// 过滤器损坏时闭锁失败，不传出任何未过滤内容。
    fn redact_sensitive_value(&self, value: &mut Value) -> Result<(), VulcanCoreError> {
        let Some(values) = self.sensitive_tool_output_values.as_ref() else {
            return Ok(());
        };
        let values = values.read().map_err(|_| VulcanCoreError::InvalidState {
            message: String::from("Sensitive tool-output filter is unavailable."),
        })?;
        redact_sensitive_json_strings(value, &values);
        Ok(())
    }

    /// Installs one complete durable tool-batch identity receipt before any tool body starts.
    /// 在任何工具主体启动前安装一份完整可靠的工具批次身份回执。
    ///
    /// # Parameters
    /// 参数
    /// `requests` are the exact Core requests admitted by the batch; `identities` are the exact
    /// local ids returned by the authoritative V1 session document.
    /// `requests` 是该批次接纳的精确 Core 请求；`identities` 是权威 V1 会话文档返回的精确本地身份。
    ///
    /// # Returns
    /// 返回值
    /// Success only when every request stream has exactly one matching identity and no older
    /// prepared identity would be overwritten.
    /// 仅当每个请求流都恰好拥有一个匹配身份，且不会覆盖旧的待执行身份时成功。
    pub(crate) fn install_prepared_invocation_identities(
        &self,
        requests: &[CoreToolInvocationRequest],
        identities: Vec<PreparedToolInvocationIdentity>,
    ) -> Result<(), String> {
        if requests.len() != identities.len() {
            return Err(format!(
                "Prepared V1 tool identity count mismatch: expected {}, received {}.",
                requests.len(),
                identities.len()
            ));
        }
        let mut expected = HashMap::with_capacity(requests.len());
        for request in requests {
            let request_stream_id = RequestStreamId::try_new(request.request_stream_id.clone())
                .map_err(|error| error.to_string())?;
            if expected
                .insert(request_stream_id, request.tool_call_id.as_str())
                .is_some()
            {
                return Err(String::from(
                    "Prepared tool batch contains a duplicate request stream identity.",
                ));
            }
        }
        let mut staged = HashMap::with_capacity(identities.len());
        for identity in identities {
            if !expected.contains_key(&identity.request_stream_id) {
                return Err(format!(
                    "Prepared V1 tool identity references unknown request stream `{}`.",
                    identity.request_stream_id.as_str()
                ));
            }
            if staged
                .insert(identity.request_stream_id.clone(), identity)
                .is_some()
            {
                return Err(String::from(
                    "Prepared V1 tool identities contain a duplicate request stream identity.",
                ));
            }
        }
        let mut prepared = self.prepared_invocation_identities.lock().map_err(|_| {
            String::from("Prepared V1 tool identity registry is locked and cannot install a batch.")
        })?;
        if let Some(conflict) = staged.keys().find(|key| prepared.contains_key(*key)) {
            return Err(format!(
                "Prepared V1 tool identity for request stream `{}` is already awaiting execution.",
                conflict.as_str()
            ));
        }
        prepared.extend(staged);
        Ok(())
    }

    /// Consumes the exact durable local identity assigned to one request stream.
    /// 消费分配给一个请求流的精确可靠本地身份。
    fn take_prepared_invocation_identity(
        &self,
        request: &CoreToolInvocationRequest,
    ) -> Result<PreparedToolInvocationIdentity, VulcanCoreError> {
        let request_stream_id = RequestStreamId::try_new(request.request_stream_id.clone())
            .map_err(|error| VulcanCoreError::InvalidState {
                message: error.to_string(),
            })?;
        self.prepared_invocation_identities
            .lock()
            .map_err(|_| VulcanCoreError::InvalidState {
                message: String::from(
                    "Prepared V1 tool identity registry is locked and cannot dispatch a tool.",
                ),
            })?
            .remove(&request_stream_id)
            .ok_or_else(|| VulcanCoreError::InvalidState {
                message: format!(
                    "Tool request stream `{}` has no durable local V1 invocation identity.",
                    request_stream_id.as_str()
                ),
            })
    }

    /// Dispatches one resolved host-managed tool definition through the shared tools manager.
    /// 通过共享工具管理器调度一条已解析宿主管理工具定义。
    /// `texts` carries the permission-only call text (streamed arguments, latest user request).
    /// `texts` 携带仅供权限使用的调用文本（流式参数、最近用户请求）。
    pub async fn invoke_definition(
        &self,
        definition: &ResolvedToolDefinition,
        args: Value,
        texts: PermissionTexts<'_>,
        context: VulcanToolInvocationContext,
    ) -> Result<VulcanToolInvocationResult, String> {
        if let Some(message) = self
            .admit_definition(definition, &args, texts, &context)
            .await?
        {
            return Ok(VulcanToolInvocationResult::from_model_output(
                Value::String(message),
            ));
        }
        self.manager
            .invoke_tool(definition, args, context, &self.execution_mode)
            .await
    }

    /// Runs the plan-mode and permission gates for one call; `Some` carries the denial text.
    /// 为一次调用运行计划模式与权限闸门；`Some` 携带拒绝文本。
    async fn admit_definition(
        &self,
        definition: &ResolvedToolDefinition,
        args: &Value,
        texts: PermissionTexts<'_>,
        context: &VulcanToolInvocationContext,
    ) -> Result<Option<String>, String> {
        // Plan mode answers first; only calls it lets through reach the permission gate.
        // 计划模式优先作答；只有它放行的调用才进入权限闸门。
        if VulcanToolsManager::plan_mode_blocks(definition, args, &self.execution_mode) {
            return Ok(None);
        }
        match admit_tool_call(
            self.permission_host.as_ref(),
            self.session_id,
            definition,
            args,
            texts,
            context,
            self.manager.workspace_root(),
        )
        .await
        {
            ToolPermissionVerdict::Proceed => Ok(None),
            ToolPermissionVerdict::Deny(message) => Ok(Some(message)),
            ToolPermissionVerdict::Fail(error) => Err(error),
        }
    }

    /// Returns the stable fingerprint of the exact resolved or explicitly unregistered binding used by one request.
    /// 返回单条请求所使用精确已解析绑定或明确未注册绑定的稳定指纹。
    pub(crate) fn binding_fingerprint_for_request(
        &self,
        request: &CoreToolInvocationRequest,
    ) -> Result<String, String> {
        match self.definitions_by_id.get(&request.tool_name) {
            Some(definition) => resolved_tool_binding_fingerprint(definition),
            None => unregistered_tool_binding_fingerprint(&request.tool_name),
        }
    }
}

/// Computes a stable fingerprint for one resolved tool binding without serializing its opaque invoker.
/// 为一条已解析工具绑定计算稳定指纹，同时不序列化其不透明调用器。
pub(crate) fn resolved_tool_binding_fingerprint(
    definition: &ResolvedToolDefinition,
) -> Result<String, String> {
    let binding = json!({
        "schema_version": TOOL_BINDING_FINGERPRINT_SCHEMA_VERSION,
        "descriptor": {
            "id": definition.descriptor.id,
            "name": definition.descriptor.name,
            "description": definition.descriptor.description,
            "help_text": definition.descriptor.help_text,
            "source": format!("{:?}", definition.descriptor.source),
            "origin": {
                "source_kind": format!("{:?}", definition.descriptor.origin().source_kind()),
                "namespace": definition.descriptor.origin().namespace(),
                "original_name": definition.descriptor.origin().original_name(),
            },
        },
        "parameters_schema": definition.parameters_schema,
        "plan_policy": format!("{:?}", definition.plan_policy),
    });
    stable_tool_binding_fingerprint(&binding)
}

/// Computes a stable fingerprint for the explicit absence of one provider-requested tool binding.
/// 为 Provider 请求的单个工具绑定明确缺失状态计算稳定指纹。
fn unregistered_tool_binding_fingerprint(tool_name: &str) -> Result<String, String> {
    let binding = json!({
        "schema_version": TOOL_BINDING_FINGERPRINT_SCHEMA_VERSION,
        "binding_state": "unregistered",
        "tool_name": tool_name,
    });
    stable_tool_binding_fingerprint(&binding)
}

/// Hashes one canonical tool-binding description into the recovery protocol fingerprint.
/// 将一份规范工具绑定描述哈希为恢复协议指纹。
fn stable_tool_binding_fingerprint(binding: &Value) -> Result<String, String> {
    let canonical = canonicalize_tool_binding_json(binding);
    let encoded = serde_json::to_vec(&canonical)
        .map_err(|error| format!("Failed to serialize resolved tool binding: {error}"))?;
    Ok(Sha256::digest(encoded)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// Recursively orders JSON object keys so tool-binding fingerprints are independent of map insertion order.
/// 递归排序 JSON 对象键，使工具绑定指纹不受映射插入顺序影响。
fn canonicalize_tool_binding_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries = map.iter().collect::<Vec<_>>();
            entries.sort_by(|left, right| left.0.cmp(right.0));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key.clone(), canonicalize_tool_binding_json(value)))
                    .collect(),
            )
        }
        Value::Array(items) => {
            Value::Array(items.iter().map(canonicalize_tool_binding_json).collect())
        }
        other => other.clone(),
    }
}

/// Inserts one pending projection barrier into the supplied authoritative registry.
/// 向给定权威注册表插入一条待确认投影屏障。
///
/// # Parameters
/// 参数
/// `registry` owns the pending acknowledgement senders.
/// `registry` 持有待确认发送端。
/// `barrier_id` is the stable identity copied into event metadata.
/// `barrier_id` 是复制到事件元数据中的稳定身份。
/// `sender` resolves the exact waiting runtime execution.
/// `sender` 用于恢复精确等待中的运行时执行。
/// `operation` describes the caller operation for diagnostics.
/// `operation` 描述调用方操作以生成诊断。
///
/// # Returns
/// 返回值
/// Success after insertion, or an explicit poisoned-registry error.
/// 插入完成后返回成功，否则返回明确的注册表中毒错误。
fn insert_runtime_event_projection_barrier(
    registry: &Mutex<RuntimeEventProjectionBarrierRegistry>,
    barrier_id: String,
    sender: oneshot::Sender<RuntimeEventProjectionReceipt>,
    operation: &str,
) -> Result<(), String> {
    registry
        .lock()
        .map_err(|_| {
            format!(
                "The runtime-event projection barrier registry is locked and cannot {operation}."
            )
        })?
        .insert(barrier_id, sender);
    Ok(())
}

/// Removes one pending projection barrier from the supplied authoritative registry.
/// 从给定权威注册表移除一条待确认投影屏障。
///
/// # Parameters
/// 参数
/// `registry` owns the pending acknowledgement senders.
/// `registry` 持有待确认发送端。
/// `barrier_id` identifies the exact pending acknowledgement.
/// `barrier_id` 标识精确的待确认项。
/// `operation` describes the caller operation for diagnostics.
/// `operation` 描述调用方操作以生成诊断。
///
/// # Returns
/// 返回值
/// The removed sender when still pending, `None` when legitimately absent, or a lock error.
/// 仍待确认时返回已移除发送端，合法缺席时返回 `None`，否则返回锁错误。
fn remove_runtime_event_projection_barrier(
    registry: &Mutex<RuntimeEventProjectionBarrierRegistry>,
    barrier_id: &str,
    operation: &str,
) -> Result<Option<oneshot::Sender<RuntimeEventProjectionReceipt>>, String> {
    Ok(registry
        .lock()
        .map_err(|_| {
            format!(
                "The runtime-event projection barrier registry is locked and cannot {operation}."
            )
        })?
        .remove(barrier_id))
}

/// Attaches a projection barrier id to one runtime event that the host must acknowledge.
/// 为单条宿主必须确认的运行时事件附加投影屏障标识。
///
/// Barriers are unconditional: tool execution needs the durable identities the host returns
/// through them, so no mode exists in which a host-owned stream may run without its consumer.
/// 屏障无条件启用：工具执行需要宿主经由屏障返回的可靠身份，因此不存在宿主持有流可以脱离
/// 其消费端运行的模式。
///
/// # Parameters
/// 参数
/// `event` is the authoritative runtime event that may require host projection acknowledgement.
/// `event` 是可能需要宿主投影确认的权威运行时事件。
///
/// # Returns
/// 返回值
/// The event and its exact wait handle, or a registry-lock error.
/// 返回事件及其精确等待句柄，注册表锁失败时返回错误。
pub(crate) fn attach_runtime_event_projection_barrier(
    mut event: RuntimeEvent,
) -> Result<(RuntimeEvent, RuntimeEventProjectionBarrier), String> {
    // requiredForExecution distinguishes durable recovery gates from display-only ordering barriers.
    // requiredForExecution 将可靠恢复门禁与仅用于显示排序的屏障区分开来。
    let required_for_execution = matches!(
        event.kind,
        RuntimeEventKind::ToolBatchPrepared
            | RuntimeEventKind::ToolStarted
            | RuntimeEventKind::ToolFinished
            | RuntimeEventKind::DelegateStarted
    );
    let barrier_id = Uuid::new_v4().to_string();
    let (sender, receiver) = oneshot::channel();
    let registry = RUNTIME_EVENT_PROJECTION_BARRIERS.get_or_init(|| Mutex::new(HashMap::new()));
    insert_runtime_event_projection_barrier(
        registry,
        barrier_id.clone(),
        sender,
        "register a runtime event",
    )?;

    let mut metadata = match event.metadata.take() {
        Some(Value::Object(map)) => map,
        Some(other) => {
            let mut map = Map::new();
            map.insert(String::from("payload"), other);
            map
        }
        None => Map::new(),
    };
    metadata.insert(
        String::from(RUNTIME_EVENT_PROJECTION_BARRIER_ID),
        Value::String(barrier_id.clone()),
    );
    event.metadata = Some(Value::Object(metadata));

    Ok((
        event,
        RuntimeEventProjectionBarrier {
            id: barrier_id,
            required_for_execution,
            receiver,
        },
    ))
}

/// Waits until the host bridge confirms the runtime event has been projected.
/// 等待宿主桥接层确认运行时事件已经完成投影。
///
/// # Parameters
/// 参数
/// `barrier` is the exact acknowledgement receiver returned while attaching the event.
/// `barrier` 是附加事件时返回的精确确认接收端。
///
/// # Returns
/// 返回值
/// Success after acknowledgement. Display-only barriers tolerate cancellation and timeout, while a required execution barrier fails closed.
/// 收到确认时返回成功。仅显示屏障容忍取消和超时，执行必需屏障则失败关闭。
pub(crate) async fn wait_runtime_event_projection_barrier(
    barrier: RuntimeEventProjectionBarrier,
) -> Result<(), RuntimeEventProjectionError> {
    wait_runtime_event_projection_receipt(barrier)
        .await
        .map(|_| ())
}

/// Attaches the production barrier and exposes its wait result to cross-crate host verification.
/// 附加生产屏障，并向跨 crate 宿主验证公开等待结果。
/// `event` is the actual host input.
/// `event` 是实际宿主输入。
/// Returns the decorated event and its production waiter, or an explicit registry error.
/// 返回装饰后的事件及生产等待器，或明确的注册表错误。
#[cfg(feature = "test-support")]
pub fn attach_test_runtime_event_projection_barrier(
    event: RuntimeEvent,
) -> Result<
    (
        RuntimeEvent,
        impl std::future::Future<Output = Result<(), String>>,
    ),
    String,
> {
    // Use the real registration and waiting paths, including the event's execution-safety policy.
    // 使用真实注册和等待路径，包含事件自身的执行安全策略。
    let (event, barrier) = attach_runtime_event_projection_barrier(event)?;
    Ok((event, async move {
        wait_runtime_event_projection_barrier(barrier)
            .await
            .map_err(|error| error.to_string())
    }))
}

/// Waits for the exact durable local identities assigned to one prepared tool batch.
/// 等待分配给一个已准备工具批次的精确可靠本地身份。
///
/// # Parameters
/// 参数
/// `barrier` is the mandatory ToolBatchPrepared projection barrier.
/// `barrier` 是强制的 ToolBatchPrepared 投影屏障。
///
/// # Returns
/// 返回值
/// Returns the exact ordered local identities, or fails when the host acknowledges the wrong
/// barrier shape.
/// 返回精确有序的本地身份；宿主确认了错误屏障形状时失败。
pub(crate) async fn wait_tool_batch_projection_barrier(
    barrier: RuntimeEventProjectionBarrier,
) -> Result<Vec<PreparedToolInvocationIdentity>, RuntimeEventProjectionError> {
    match wait_runtime_event_projection_receipt(barrier).await? {
        RuntimeEventProjectionReceipt::ToolBatchPrepared(identities) => Ok(identities),
        RuntimeEventProjectionReceipt::Projected => {
            Err(RuntimeEventProjectionError::UnexpectedReceipt)
        }
        RuntimeEventProjectionReceipt::Rejected(detail) => {
            Err(RuntimeEventProjectionError::Rejected { detail })
        }
    }
}

/// Removes a barrier's registry entry when its waiter ends, including when the waiter is dropped.
/// 等待方结束时（包括被丢弃时）移除屏障的注册表条目。
struct AbandonedProjectionBarrierGuard {
    /// Stable id of the barrier being waited on.
    /// 正在等待的屏障稳定标识。
    barrier_id: String,
}

impl Drop for AbandonedProjectionBarrierGuard {
    fn drop(&mut self) {
        // Removal is idempotent; an acknowledged barrier is already gone, and a destructor cannot
        // report a poisoned registry.
        // 移除是幂等的；已确认的屏障早已不在，析构函数也无法上报中毒的注册表。
        if let Some(registry) = RUNTIME_EVENT_PROJECTION_BARRIERS.get() {
            let _ = remove_runtime_event_projection_barrier(
                registry,
                &self.barrier_id,
                "remove an abandoned barrier",
            );
        }
    }
}

/// Waits for one internal runtime-event projection receipt.
/// 等待一个内部运行时事件投影回执。
async fn wait_runtime_event_projection_receipt(
    barrier: RuntimeEventProjectionBarrier,
) -> Result<RuntimeEventProjectionReceipt, RuntimeEventProjectionError> {
    let barrier_id = barrier.id.clone();
    let required_for_execution = barrier.required_for_execution;
    // A cancelled stream drops this future; the guard removes the pending sender so it never leaks.
    // 流被取消时本 future 会被丢弃；守卫移除待确认发送端，避免残留。
    let _abandoned_barrier_guard = AbandonedProjectionBarrierGuard {
        barrier_id: barrier_id.clone(),
    };
    match tokio::time::timeout(RUNTIME_EVENT_PROJECTION_BARRIER_TIMEOUT, barrier.receiver).await {
        Ok(Ok(RuntimeEventProjectionReceipt::Rejected(detail))) if required_for_execution => {
            Err(RuntimeEventProjectionError::Rejected { detail })
        }
        Ok(Ok(receipt)) => Ok(receipt),
        Ok(Err(_)) if required_for_execution => Err(RuntimeEventProjectionError::Rejected {
            detail: String::from(
                "The host closed the mandatory projection barrier without an acknowledgement.",
            ),
        }),
        Ok(Err(_)) => Ok(RuntimeEventProjectionReceipt::Projected),
        Err(_) => {
            if let Some(registry) = RUNTIME_EVENT_PROJECTION_BARRIERS.get() {
                remove_runtime_event_projection_barrier(
                    registry,
                    &barrier_id,
                    "remove a timed-out barrier",
                )
                .map_err(|detail| RuntimeEventProjectionError::Registry { detail })?;
            }
            if required_for_execution {
                return Err(RuntimeEventProjectionError::TimedOut);
            }
            Ok(RuntimeEventProjectionReceipt::Projected)
        }
    }
}

/// Removes a projection barrier for a runtime event that will never reach the host bridge.
/// 移除一条永远不会到达宿主桥接层的运行时事件投影屏障。
///
/// # Parameters
/// 参数
/// `barrier` is the exact wait handle whose registry entry must no longer remain pending.
/// `barrier` 是其注册表条目不应继续等待的精确等待句柄。
///
/// # Returns
/// 返回值
/// Success when cancellation completes, otherwise an explicit registry-lock error.
/// 取消完成时返回成功，否则返回明确的注册表锁错误。
pub(crate) fn cancel_runtime_event_projection_barrier(
    barrier: RuntimeEventProjectionBarrier,
) -> Result<(), String> {
    let Some(registry) = RUNTIME_EVENT_PROJECTION_BARRIERS.get() else {
        return Ok(());
    };
    remove_runtime_event_projection_barrier(registry, &barrier.id, "cancel a barrier")?;
    Ok(())
}

/// Acknowledges one projection barrier from runtime-event metadata after the service emits it.
/// 服务发出运行时事件投影后，根据事件元数据确认对应投影屏障。
///
/// # Parameters
/// 参数
/// `event` carries the authoritative optional barrier identity attached by the Agent runtime.
/// `event` 携带 Agent 运行时附加的权威可选屏障标识。
///
/// # Returns
/// 返回值
/// `Ok(true)` for a delivered acknowledgement, `Ok(false)` for legitimate absence or an abandoned waiter, or a registry-lock error.
/// 确认已送达时返回 `Ok(true)`，合法缺席或等待方已放弃时返回 `Ok(false)`，注册表锁失败时返回错误。
pub fn acknowledge_runtime_event_projection_barrier_from_event(
    event: &RuntimeEvent,
) -> Result<bool, String> {
    let Some(barrier_id) = runtime_event_projection_barrier_id(event) else {
        return Ok(false);
    };
    let Some(registry) = RUNTIME_EVENT_PROJECTION_BARRIERS.get() else {
        return Ok(false);
    };
    let sender = remove_runtime_event_projection_barrier(
        registry,
        barrier_id,
        "acknowledge a projected event",
    )?;
    if let Some(sender) = sender {
        Ok(sender
            .send(RuntimeEventProjectionReceipt::Projected)
            .is_ok())
    } else {
        Ok(false)
    }
}

/// Acknowledges one ToolBatchPrepared barrier with its exact durable local identities.
/// 使用精确可靠的本地身份确认一个 ToolBatchPrepared 屏障。
///
/// # Parameters
/// 参数
/// `event` carries the exact barrier identity and must be ToolBatchPrepared; `identities` are the
/// ordered receipts returned by the authoritative session owner.
/// `event` 携带精确屏障身份且必须是 ToolBatchPrepared；`identities` 是权威会话所有者返回的有序回执。
///
/// # Returns
/// 返回值
/// `Ok(true)` when the waiting executor received the identities, `Ok(false)` when the waiter was
/// already abandoned, or an explicit shape/registry error.
/// 等待执行器收到身份时返回 `Ok(true)`，等待方已放弃时返回 `Ok(false)`，否则返回明确形状或注册表错误。
pub fn acknowledge_tool_batch_projection_barrier_from_event(
    event: &RuntimeEvent,
    identities: Vec<PreparedToolInvocationIdentity>,
) -> Result<bool, String> {
    if event.kind != RuntimeEventKind::ToolBatchPrepared {
        return Err(String::from(
            "Only a ToolBatchPrepared event may acknowledge local V1 tool identities.",
        ));
    }
    let Some(barrier_id) = runtime_event_projection_barrier_id(event) else {
        return Ok(false);
    };
    let Some(registry) = RUNTIME_EVENT_PROJECTION_BARRIERS.get() else {
        return Ok(false);
    };
    let sender = remove_runtime_event_projection_barrier(
        registry,
        barrier_id,
        "acknowledge a prepared tool batch",
    )?;
    if let Some(sender) = sender {
        Ok(sender
            .send(RuntimeEventProjectionReceipt::ToolBatchPrepared(identities))
            .is_ok())
    } else {
        Ok(false)
    }
}

/// Rejects one projection barrier from runtime-event metadata without acknowledging execution safety.
/// 根据运行时事件元数据拒绝一条投影屏障，且不确认执行安全性。
///
/// # Parameters
/// 参数
/// `event` carries the authoritative optional barrier identity attached by the Agent runtime.
/// `event` 携带 Agent 运行时附加的权威可选屏障标识。
///
/// # Returns
/// 返回值
/// `Ok(true)` when one pending waiter was rejected, `Ok(false)` for legitimate absence, or a registry-lock error.
/// 拒绝了一条待处理等待时返回 `Ok(true)`，合法缺席时返回 `Ok(false)`，注册表锁失败时返回错误。
pub fn reject_runtime_event_projection_barrier_from_event(
    event: &RuntimeEvent,
) -> Result<bool, String> {
    reject_runtime_event_projection_barrier_from_event_with_reason(
        event,
        "The host rejected the runtime-event projection without a more specific diagnostic.",
    )
}

/// Rejects one projection barrier and delivers the originating host failure to the waiter.
/// 拒绝一条投影屏障，并把宿主原始失败传递给等待方。
///
/// `event` owns the exact barrier identity and `reason` is the controlled failure emitted
/// by the persistence or projection boundary. The function returns whether a waiter received
/// the rejection, while a missing barrier remains a legitimate `Ok(false)`.
/// `event` 持有精确屏障身份，`reason` 是持久化或投影边界发出的受控失败。函数返回
/// 是否有等待方收到拒绝；屏障不存在仍是合法的 `Ok(false)`。
pub fn reject_runtime_event_projection_barrier_from_event_with_reason(
    event: &RuntimeEvent,
    reason: impl Into<String>,
) -> Result<bool, String> {
    let Some(barrier_id) = runtime_event_projection_barrier_id(event) else {
        return Ok(false);
    };
    let Some(registry) = RUNTIME_EVENT_PROJECTION_BARRIERS.get() else {
        return Ok(false);
    };
    // sender is the exact one-shot owner removed before the rejection is delivered.
    // sender 是在发送拒绝回执前移除的精确一次性 owner。
    let sender =
        remove_runtime_event_projection_barrier(registry, barrier_id, "reject a projected event")?;
    if let Some(sender) = sender {
        Ok(sender
            .send(RuntimeEventProjectionReceipt::Rejected(reason.into()))
            .is_ok())
    } else {
        Ok(false)
    }
}

/// Returns whether one runtime event is the exact mandatory gate before a real tool body may start.
/// 返回单条运行时事件是否是真实工具主体开始前的精确强制门禁。
///
/// # Parameters
/// 参数
/// `event` is the runtime event whose kind and runtime-owned barrier identity must both match.
/// `event` 是其类型与运行时持有的屏障身份都必须匹配的运行时事件。
///
/// # Returns
/// 返回值
/// `true` only for an actual tool-start event emitted by the streaming tool executor with a projection barrier.
/// 仅对流式工具执行器发出且携带投影屏障的真实工具开始事件返回 `true`。
pub fn runtime_event_requires_tool_execution_safety_barrier(event: &RuntimeEvent) -> bool {
    event.kind == RuntimeEventKind::ToolStarted
        && runtime_event_projection_barrier_id(event).is_some()
}

/// Reads the exact optional projection-barrier identity carried by one runtime event.
/// 读取单条运行时事件携带的精确可选投影屏障身份。
///
/// # Parameters
/// 参数
/// `event` is the authoritative metadata owner.
/// `event` 是权威元数据持有者。
///
/// # Returns
/// 返回值
/// The borrowed non-empty barrier id when present, otherwise `None`.
/// 存在时返回借用的非空屏障标识，否则返回 `None`。
fn runtime_event_projection_barrier_id(event: &RuntimeEvent) -> Option<&str> {
    event
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get(RUNTIME_EVENT_PROJECTION_BARRIER_ID))
        .and_then(Value::as_str)
        .filter(|barrier_id| !barrier_id.is_empty())
}

/// Builds the canonical controller-visible tool-error host result with trusted invocation identity.
/// 使用可信调用身份构建规范的控制器可见工具错误宿主结果。
///
/// # Parameters
/// 参数
/// - `request`: Core-owned invocation identity and arguments.
/// - `request`：Core 持有的调用身份与参数。
/// - `error`: Human-readable error text exposed at the canonical payload root.
/// - `error`：暴露在规范载荷根部的人类可读错误文本。
/// - `details`: Optional tool-owned structured failure details.
/// - `details`：可选的工具自有结构化失败详情。
///
/// # Returns
/// 返回值
/// Canonical `tool_error` host result consumed by runtime and WebUI projections.
/// 由运行时与 WebUI 投影消费的规范 `tool_error` 宿主结果。
fn build_canonical_tool_error_host_result(
    request: &CoreToolInvocationRequest,
    error: &str,
    details: Option<Value>,
) -> Value {
    let mut payload = json!({
        "tool_call_id": request.tool_call_id,
        "tool_name": request.tool_name,
        "status": "failed",
        "error": error,
    });
    if let Some(details) = details {
        payload["details"] = details;
    }
    json!({
        "kind": "tool_error",
        "payload": payload,
    })
}

/// Canonicalizes one tool-owned structured error detail at the invocation-identity boundary.
/// 在调用身份边界规范化一条工具自有结构化错误详情。
///
/// # Parameters
/// 参数
/// - `request`: Core-owned invocation identity and arguments.
/// - `request`：Core 持有的调用身份与参数。
/// - `details`: Tool-owned envelope containing failed status, non-empty error, and domain details.
/// - `details`：包含失败状态、非空错误与领域详情的工具自有信封。
///
/// # Returns
/// 返回值
/// Canonical host result, or an explicit protocol violation.
/// 规范宿主结果，或显式协议违规错误。
fn canonicalize_tool_error_details(
    request: &CoreToolInvocationRequest,
    details: &Value,
) -> Result<Value, String> {
    if details.get("status").and_then(Value::as_str) != Some("failed") {
        return Err(format!(
            "Tool `{}` returned `{VULCAN_TOOL_ERROR_DETAIL_KIND}` without `status: failed`.",
            request.tool_name
        ));
    }
    let error = details
        .get("error")
        .and_then(Value::as_str)
        .filter(|error| !error.trim().is_empty())
        .ok_or_else(|| {
            format!(
                "Tool `{}` returned `{VULCAN_TOOL_ERROR_DETAIL_KIND}` without a non-empty `error` string.",
                request.tool_name
            )
        })?;
    let domain_details = details.get("details").cloned().ok_or_else(|| {
        format!(
            "Tool `{}` returned `{VULCAN_TOOL_ERROR_DETAIL_KIND}` without `details`.",
            request.tool_name
        )
    })?;
    Ok(build_canonical_tool_error_host_result(
        request,
        error,
        Some(domain_details),
    ))
}

/// Builds one controller-visible error result so host-managed tool failures can be returned to the model instead of aborting the whole turn.
/// 构建一条控制器可见的错误结果，使宿主管理工具失败能够回传给模型，而不是直接中断整轮执行。
pub fn build_tool_failure_invocation_result(
    request: &CoreToolInvocationRequest,
    message: impl Into<String>,
) -> CoreToolInvocationResult {
    let message = message.into();
    let model_output = format!(
        "Tool `{}` failed.\nReason: {}\nAdjust the arguments, narrow the scope, or switch to a more appropriate tool before continuing.",
        request.tool_name, message
    );
    CoreToolInvocationResult {
        tool_call_id: request.tool_call_id.clone(),
        tool_name: request.tool_name.clone(),
        model_output,
        host_result: Some(build_canonical_tool_error_host_result(
            request, &message, None,
        )),
        is_error: true,
    }
}

/// Converts one host-managed invocation result into the canonical controller result.
/// 将一条宿主管理调用结果转换为规范控制器结果。
///
/// # Parameters
/// 参数
/// - `request`: Exact controller-owned tool request and invocation identity.
/// - `request`：控制器持有的精确工具请求与调用身份。
/// - `result`: Tool-owned model output and optional structured host result.
/// - `result`：工具持有的模型输出与可选结构化宿主结果。
///
/// # Returns
/// 返回值
/// Canonical controller result, including trusted identity and normalized errors.
/// 包含可信身份与规范化错误的控制器结果。
pub fn canonicalize_tool_invocation_result(
    request: &CoreToolInvocationRequest,
    result: VulcanToolInvocationResult,
) -> CoreToolInvocationResult {
    let (host_result, is_error) = match result.host_result.as_ref() {
        Some(host_result) if host_result.kind == VULCAN_TOOL_ERROR_DETAIL_KIND => {
            let canonical = match canonicalize_tool_error_details(request, &host_result.payload) {
                Ok(canonical) => canonical,
                Err(message) => return build_tool_failure_invocation_result(request, message),
            };
            (Some(canonical), true)
        }
        Some(host_result) if host_result.kind == "tool_error" => {
            return build_tool_failure_invocation_result(
                request,
                format!(
                    "Tool `{}` returned reserved host-result kind `tool_error`; return `{VULCAN_TOOL_ERROR_DETAIL_KIND}` details instead.",
                    request.tool_name
                ),
            );
        }
        Some(host_result) => (
            Some(json!({
                "kind": host_result.kind.clone(),
                "payload": host_result.payload.clone(),
            })),
            false,
        ),
        None => (None, false),
    };
    let model_output = if is_error {
        // Error details were validated above; input arguments already belong to the tool event.
        // 错误详情已在上方验证；输入参数已归工具事件持有。
        let details = &result
            .host_result
            .as_ref()
            .expect("validated tool-owned error")
            .payload;
        result_markdown::error_markdown(&request.tool_name, details)
    } else {
        match result.model_output {
            Value::String(text) => text,
            // Images are saved before canonicalization; raw Base64 must never reach the model as text.
            // 图片在规范化之前保存；原始 Base64 绝不能以文本形式进入模型。
            other if crate::agent_runtime::tool_result_media::has_image_blocks(&other) => {
                return build_tool_failure_invocation_result(
                    request,
                    format!(
                        "Tool `{}` returned unsaved image blocks.",
                        request.tool_name
                    ),
                );
            }
            other => serde_json::to_string(&other).unwrap_or_else(|_| other.to_string()),
        }
    };
    CoreToolInvocationResult {
        tool_call_id: request.tool_call_id.clone(),
        tool_name: request.tool_name.clone(),
        model_output,
        host_result,
        is_error,
    }
}

/// Builds the exact V1 durable tool-output payload from one controller result.
/// 根据一条控制器结果构建精确的 V1 可靠工具输出载荷。
///
/// # Parameters
/// 参数
/// - `result`: Canonical controller result emitted to the Provider runtime.
/// - `result`：发送给 Provider 运行时的规范控制器结果。
///
/// # Returns
/// 返回值
/// Strict V1 tool payload, or an explicit payload-validation error.
/// 严格 V1 工具载荷；校验失败时返回显式错误。
pub fn persistent_tool_output_from_result(
    result: &CoreToolInvocationResult,
) -> Result<ToolPayload, vulcan_session_core::v1::errors::DomainError> {
    ToolPayload::try_new(json!({
        "tool_call_id": result.tool_call_id,
        "tool_name": result.tool_name,
        "model_output": result.model_output,
        "host_result": result.host_result,
        "is_error": result.is_error,
    }))
}

/// Redacts exact host credentials from every string leaf of a structured tool result.
/// 从结构化工具结果的每个字符串叶节点遮蔽精确宿主凭据。
///
/// `value` is mutated in place and `secrets` contains only bootstrap-registered host values.
/// `value` 就地修改，`secrets` 仅包含引导阶段注册的宿主值。
fn redact_sensitive_json_strings(value: &mut Value, secrets: &[SensitiveToolOutputValue]) {
    match value {
        Value::String(text) => {
            for secret in secrets {
                *text = text.replace(&secret.value, &secret.replacement);
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_sensitive_json_strings(item, secrets);
            }
        }
        Value::Object(fields) => {
            for field in fields.values_mut() {
                redact_sensitive_json_strings(field, secrets);
            }
        }
        _ => {}
    }
}

/// Builds one controller-visible policy rejection result without failing the provider turn.
/// 构建一条控制器可见的策略拒绝结果，同时不使 Provider 回合失败。
pub(crate) fn build_tool_rejection_invocation_result(
    request: &CoreToolInvocationRequest,
    message: impl Into<String>,
) -> CoreToolInvocationResult {
    let message = message.into();
    let model_output = format!(
        "Tool request rejected by runtime policy.\nTool: `{}`\nReason: {}\nUse another available tool or report this limitation to the parent agent.",
        request.tool_name, message
    );
    CoreToolInvocationResult {
        tool_call_id: request.tool_call_id.clone(),
        tool_name: request.tool_name.clone(),
        model_output,
        host_result: Some(json!({
            "kind": "tool_result",
            "payload": {
                "tool_call_id": request.tool_call_id,
                "tool_name": request.tool_name,
                "status": "rejected",
                "rejected": true,
                "reason": message,
                "arguments": request.arguments,
            }
        })),
        is_error: false,
    }
}

/// Produces the actual policy rejection for cross-crate host boundary verification.
/// 为跨 crate 宿主边界验证生成真实策略拒绝结果。
/// `request` identifies the rejected invocation and `message` explains the policy; returns the production result.
/// `request` 标识被拒绝调用，`message` 说明策略；返回生产结果。
#[cfg(feature = "test-support")]
pub fn test_tool_rejection_invocation_result(
    request: &CoreToolInvocationRequest,
    message: &str,
) -> CoreToolInvocationResult {
    build_tool_rejection_invocation_result(request, message)
}

/// Rebuilds a native stop receipt from the verified job ID and terminal status only.
/// 仅使用已校验的任务 ID 与终态重建原生停止回执。
///
/// The projected receipt contains no stored output and can be returned if the registry fails.
/// 此投影回执不含存储输出，可在注册表故障时返回。
fn project_shell_job_stop_receipt(
    definition: &ResolvedToolDefinition,
    request: &CoreToolInvocationRequest,
    result: &VulcanToolInvocationResult,
) -> Option<CoreToolInvocationResult> {
    use crate::tooling::native::shell_job::{SHELL_JOB_TOOL_ID, shell_job_stop_state};
    use vulcan_host_tools::shell_job::{ShellJobStatus, valid_job_id};
    use vulcan_tools_manager::VulcanToolSourceKind;

    if definition.descriptor.id != SHELL_JOB_TOOL_ID
        || definition.descriptor.source != VulcanToolSourceKind::Native
        || request.arguments.get("action")?.as_str()? != "stop"
    {
        return None;
    }
    let host = result.host_result.as_ref()?;
    let job_id = host.payload.get("job_id")?.as_str()?;
    if host.kind != SHELL_JOB_TOOL_ID
        || host.payload.get("action")?.as_str()? != "stop"
        || host.payload.get("running")?.as_bool()?
        || request.arguments.get("job_id")?.as_str()?.trim() != job_id
        || !valid_job_id(job_id)
    {
        return None;
    }
    let status =
        serde_json::from_value::<ShellJobStatus>(host.payload.get("status")?.clone()).ok()?;
    Some(canonicalize_tool_invocation_result(
        request,
        shell_job_stop_state(job_id, status),
    ))
}

#[async_trait::async_trait]
impl VulcanToolExecutor for VulcanManagedToolRuntimeBridge {
    /// Executes one controller-owned tool request through the shared host-managed tool bridge.
    /// 通过共享宿主管理工具桥接层执行一次控制器自有工具请求。
    async fn execute_tool(
        &self,
        request: &CoreToolInvocationRequest,
    ) -> Result<CoreToolInvocationResult, VulcanCoreError> {
        let identity = self.take_prepared_invocation_identity(request)?;
        let Some(definition) = self.definitions_by_id.get(&request.tool_name) else {
            return self.redact_sensitive_tool_result(build_tool_failure_invocation_result(
                request,
                format!(
                    "Tool `{}` is not registered in the resolved definition set.",
                    request.tool_name
                ),
            ));
        };
        let context = VulcanToolInvocationContext {
            turn_id: identity.turn_id,
            session_id: self.session_id.to_string(),
            request_stream_id: RequestStreamId::try_new(request.request_stream_id.clone())
                .map_err(|error| VulcanCoreError::InvalidState {
                    message: error.to_string(),
                })?,
            api_call_id: identity.api_call_id,
            tool_call_id: identity.tool_call_id,
            tool_seq: identity.tool_seq,
            controller_tool_call_id: request.tool_call_id.clone(),
            provider_tool_call_id: Some(request.tool_call_id.clone()),
        };
        if definition.descriptor.id == vulcan_tool_chain::TOOL_CHAIN_TOOL_ID {
            return self
                .redact_sensitive_tool_result(self.execute_tool_chain(request, context).await);
        }
        let mut result = match self
            .invoke_definition(
                definition,
                request.arguments.clone(),
                PermissionTexts {
                    raw_arguments: request.raw_arguments.text(),
                    user_request: request.user_request.text(),
                },
                context,
            )
            .await
        {
            Ok(result) => result,
            Err(message) => {
                return self.redact_sensitive_tool_result(build_tool_failure_invocation_result(
                    request, message,
                ));
            }
        };
        // Image blocks become session-owned generated files; the model receives only their locators.
        // 图片块成为会话自有的生成文件；模型只收到其定位符。
        if crate::agent_runtime::tool_result_media::has_image_blocks(&result.model_output) {
            if result.host_result.is_some() {
                return self.redact_sensitive_tool_result(build_tool_failure_invocation_result(
                    request,
                    format!(
                        "Tool `{}` returned images together with a host result; images can only be saved from a result without one.",
                        request.tool_name
                    ),
                ));
            }
            match crate::agent_runtime::tool_result_media::save_tool_result_images(
                std::mem::take(&mut result.model_output),
                self.manager.workspace_root(),
                &self.session_id.to_string(),
            )
            .await
            {
                Ok((model_output, attachments)) => {
                    result.model_output = model_output;
                    result.host_result = Some(vulcan_tools_manager::VulcanToolHostResult {
                        kind: String::from(
                            crate::provider_tool_bridge::GENERATED_ATTACHMENTS_HOST_RESULT_KIND,
                        ),
                        payload: json!({ "outputs": attachments }),
                    });
                }
                Err(message) => {
                    return self.redact_sensitive_tool_result(
                        build_tool_failure_invocation_result(
                            request,
                            format!("Tool result images could not be saved: {message}"),
                        ),
                    );
                }
            }
        }

        let stop_receipt = project_shell_job_stop_receipt(definition, request, &result);
        match self
            .redact_sensitive_tool_result(canonicalize_tool_invocation_result(request, result))
        {
            Err(_) if let Some(receipt) = stop_receipt => Ok(receipt),
            result => result,
        }
    }
}

#[cfg(test)]
mod tests;

mod result_markdown;
mod tool_chain;

pub use tool_chain::{ToolChainHost, ToolChainSettings};

/// Measures canonical payload bytes for a frozen pre-change fixture and current producer output.
/// 对冻结的修改前夹具和当前生产者输出计量规范载荷字节数。
/// Inputs identify the tool and unique body; asserts one persisted body and lower total size.
/// 输入标识工具及唯一正文；断言持久化正文唯一且总大小减少。
#[cfg(test)]
pub(crate) fn assert_result_payload_cost(
    tool: &str,
    before: VulcanToolInvocationResult,
    after: VulcanToolInvocationResult,
    unique_body: &str,
) {
    let request = CoreToolInvocationRequest {
        request_stream_id: "cost-fixture".into(),
        tool_call_id: "cost-call".into(),
        tool_name: tool.into(),
        arguments: json!({}),
        argument_parse_error: None,
        raw_arguments: Default::default(),
        user_request: Default::default(),
    };
    let before = canonicalize_tool_invocation_result(&request, before);
    let after = canonicalize_tool_invocation_result(&request, after);
    let sizes = |result: &CoreToolInvocationResult| {
        (
            result.model_output.len(),
            result
                .host_result
                .as_ref()
                .map(|host| serde_json::to_vec(host).unwrap().len())
                .unwrap_or(0),
            serde_json::to_vec(result).unwrap().len(),
        )
    };
    let old = sizes(&before);
    let new = sizes(&after);
    assert!(
        new.2 < old.2,
        "Expected payload reduction for {tool}: {old:?} -> {new:?}"
    );
    assert_eq!(
        serde_json::to_string(&after)
            .unwrap()
            .matches(unique_body)
            .count(),
        1
    );
    eprintln!("PAYLOAD_COST {tool}: model/host/envelope {old:?} -> {new:?} bytes");
}
