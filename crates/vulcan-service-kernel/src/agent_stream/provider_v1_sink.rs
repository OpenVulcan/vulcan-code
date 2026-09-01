//! Provider stream projection into the unique V1 session owner.
//! Provider 流到唯一 V1 会话所有者的投影。

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use sha2::{Digest, Sha256};
use vulcan_agent_runtime::agent_runtime::events::{
    PreparedPhysicalApi, ProviderRequestPreparationCandidate, RuntimeEvent,
    ToolExecutionRecoveryBoundary,
};
use vulcan_agent_runtime::agent_runtime::provider_v1_bridge::{
    ProviderV1ApiTerminalFacts, ProviderV1ExternalResourceError, ProviderV1GeneratedResourceError,
    ProviderV1RequestFacts, ProviderV1TransportObservedFacts,
    provider_v1_external_refs_from_tool_result, provider_v1_generated_attachments_from_tool_result,
};
use vulcan_agent_runtime::agent_runtime::tool_runtime_bridge::{
    PreparedToolInvocationIdentity, persistent_tool_output_from_result,
};
use vulcan_ai_contract::{
    AiMessage, ContentItem, MessageRole, ResourceKind, ResourceReference, ResourceSourceKind,
    ToolCall as AiToolCall, ToolResult as AiToolResult,
};
use vulcan_change_set::{PersistentChangeSetError, persistent_change_set_from_host_result};
use vulcan_core::types::{
    ConversationAttachment, ConversationAttachmentKind, ConversationMessage, ConversationPayload,
    ConversationSnapshot, ConversationToolCall, ProviderContinuation, ProviderPromptLayers,
    ProviderRequestSnapshot as CoreProviderRequestSnapshot, ToolInvocationRequest,
    ToolInvocationResult,
};
use vulcan_session_core::v1::context::{
    LockedBaselineProjection, LockedPromptTarget, LockedSource,
};
#[cfg(test)]
use vulcan_session_core::v1::document::TurnSuccessor;
use vulcan_session_core::v1::document::{
    ApiCompletion, ApiGroupCompletionReceipt, ToolCompletion, ToolPreparation,
    TurnCompletionTransition,
};
use vulcan_session_core::v1::errors::DomainError;
#[cfg(test)]
use vulcan_session_core::v1::event::ContinuationEligibility;
use vulcan_session_core::v1::event::{
    ApiCallStatus, AssistantSemanticStatus, GeneratedResourceKind, GeneratedResourceRef,
    ProviderProjectionProof, ToolCallStatus, ToolPayload,
};
use vulcan_session_core::v1::ids::{ApiCallId, RequestStreamId, ResourceId, Seq, ToolCallId};
use vulcan_session_core::v1::projection::{ProjectedItem, ProjectedSystemRole};
use vulcan_workbench_model::SessionId;
use vulcan_workspace_store::workspace_store::{
    ProjectResourceCatalogError, ProjectResourceCatalogRegistry, load_project_resource_catalog,
};
#[cfg(test)]
use vulcan_workspace_store::workspace_store::{
    ProjectResourceResolution, resolve_project_resource,
};

use crate::session_runtime_registry_v1::{
    SessionFlushReason, SessionHandle, SessionMutationAdmission, SessionMutationChange,
    SessionMutationReceipt, SessionPreparedPhysicalRequest, SessionProviderProjectionSnapshot,
    SessionRuntimeError, SuccessfulTurnSettlement,
};
use crate::state::SharedServiceBridgeState;

mod media;
pub(crate) use media::frozen_prompt_media_projection;

/// Acquires the optional V1 sink once for the complete lifetime of one stream task.
/// 为一个流任务的完整生命周期一次性获取可选 V1 sink。
///
/// # Parameters
/// 参数
/// `state` owns the process-local registry and `session_id` selects the exact registered owner.
/// `state` 持有进程内注册表，`session_id` 选择精确的已注册所有者。
///
/// # Returns
/// 返回值
/// Returns `None` only when the session has not entered V1 admission yet; every registry failure
/// is explicit so an admitted V1 session never falls back to the legacy transcript.
/// 仅当会话尚未进入 V1 准入时返回 `None`；全部注册表失败均显式返回，使已准入 V1 会话绝不降级到旧转录。
pub(crate) fn provider_v1_stream_sink_for_session(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<Option<ProviderV1StreamSink>, String> {
    let (registry, project_resource_catalog_registry) = {
        let guard = state.lock().map_err(|_| {
            String::from("Service state lock poisoned while acquiring the V1 session.")
        })?;
        (
            guard.session_runtime_registry.clone(),
            Arc::clone(&guard.project_resource_catalog_registry),
        )
    };
    match registry.get(session_id) {
        Ok(handle) => Ok(Some(ProviderV1StreamSink::new(
            handle,
            project_resource_catalog_registry,
        ))),
        Err(SessionRuntimeError::SessionNotRegistered(_)) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

/// Stream-local V1 projection state that retains one exact session handle for its whole lifetime.
/// 在完整生命周期内保留同一精确会话句柄的流局部 V1 投影状态。
pub(crate) struct ProviderV1StreamSink {
    /// Unique V1 owner retained by this exact stream task.
    /// 当前精确流任务保留的唯一 V1 所有者。
    handle: Arc<SessionHandle>,
    /// Project-level owner used to register generated resources before tool completion is durable.
    /// 在工具终态可靠落盘前注册生成资源的项目级 owner。
    project_resource_catalog_registry: Arc<ProjectResourceCatalogRegistry>,
    /// Sole active physical Provider request projected by this stream.
    /// 当前流投影的唯一活动物理 Provider 请求。
    active: Option<ActiveProviderV1Projection>,
    /// Durable terminal receipt of the last physical API completed by this stream.
    /// 当前流最后一次已完成物理 API 的可靠终态回执。
    api_terminal_completion: Option<SessionMutationReceipt<ApiGroupCompletionReceipt>>,
    /// Durable logical-Turn completion produced by the final tool-free API of this stream.
    /// 当前流最终无工具 API 产生的可靠逻辑回合完成凭据。
    turn_completion: Option<SessionMutationReceipt<TurnCompletionTransition>>,
    /// Exact completed assistant semantic of the last physical API.
    /// 最后一次物理 API 的精确已完成 assistant 语义。
    completed_assistant_text: Option<ProviderV1CompletedAssistantText>,
    /// Exact locked baseline used by the last completed physical API.
    /// 最后一次已完成物理 API 使用的精确锁定基线。
    completed_locked_baseline: Option<LockedBaselineProjection>,
}

/// Exact persisted assistant semantic emitted by one completed physical API.
/// 一次已完成物理 API 产生的精确持久化 assistant 语义。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProviderV1CompletedAssistantText {
    /// Session-local physical API identity that owns the semantic.
    /// 拥有该语义的会话本地物理 API 身份。
    pub(crate) api_call_id: ApiCallId,
    /// Exact main-ledger seq of the completed assistant event.
    /// 已完成 assistant 事件在主账本中的精确 seq。
    pub(crate) seq: Seq,
    /// Exact persisted assistant body.
    /// 精确持久化的 assistant 正文。
    pub(crate) text: String,
}

/// Durable V1 facts transferred exactly once from the stream sink to terminal handling.
/// 从流 sink 向终态处理精确转移一次的可靠 V1 事实。
#[derive(Debug, Default)]
pub(crate) struct ProviderV1FinishedDurability {
    /// Exact ApiStats mutation proving that the final physical API is durable.
    /// 证明最终物理 API 已可靠落盘的精确 ApiStats 修改。
    pub(crate) api_terminal: Option<SessionMutationReceipt<ApiGroupCompletionReceipt>>,
    /// Exact logical-Turn completion and successor selection, when this API closed the Turn.
    /// 当本次 API 闭合逻辑回合时产生的精确回合完成与后继选择。
    pub(super) turn_completion: Option<SessionMutationReceipt<TurnCompletionTransition>>,
    /// Exact completed assistant semantic of the final physical API, when present.
    /// 最终物理 API 的精确已完成 assistant 语义（如存在）。
    pub(crate) completed_assistant_text: Option<ProviderV1CompletedAssistantText>,
    /// Exact locked baseline used by the final physical API, when present.
    /// 最终物理 API 使用的精确锁定基线（如存在）。
    pub(crate) locked_baseline: Option<LockedBaselineProjection>,
}

/// Exact durable result of applying one tool execution boundary.
/// 应用一个工具执行边界后的精确可靠结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProviderV1ToolBoundaryOutcome {
    /// A non-batch boundary was durably applied without a new execution identity payload.
    /// 非批次边界已可靠应用，且没有新的执行身份载荷。
    Applied,
    /// The prepared batch allocated exact local identities for every request stream.
    /// 已准备批次为每个请求流分配了精确本地身份。
    BatchPrepared(Vec<PreparedToolInvocationIdentity>),
}

/// Mutable semantic state for one active physical Provider request.
/// 单次活动物理 Provider 请求的可变语义状态。
struct ActiveProviderV1Projection {
    /// Durable V1 API identity allocated by the session document.
    /// 会话文档分配的可靠 V1 API 身份。
    api_call_id: ApiCallId,
    /// Immutable typed Provider request facts used for terminal identity validation.
    /// 用于终态身份校验的不可变强类型 Provider 请求事实。
    request_facts: ProviderV1RequestFacts,
    /// Exact validated locked baseline used to build this physical request.
    /// 构建当前物理请求所使用的精确已校验锁定基线。
    locked_baseline: LockedBaselineProjection,
    /// Exact project resource directory derived from the immutable session locator.
    /// 从不可变会话定位器派生的精确项目资源目录。
    project_directory: PathBuf,
    /// Complete reasoning text accumulated from ordered deltas.
    /// 从有序增量累积的完整思考正文。
    reasoning_text: String,
    /// Main-ledger seq of the reasoning semantic, once allocated.
    /// 推理语义分配后的主账本 seq。
    reasoning_seq: Option<Seq>,
    /// Whether the reasoning semantic already reached a terminal display status.
    /// 思考语义是否已经到达展示终态。
    reasoning_sealed: bool,
    /// Complete assistant text accumulated from ordered deltas.
    /// 从有序增量累积的完整 assistant 正文。
    assistant_text: String,
    /// Main-ledger seq of the assistant semantic, once allocated.
    /// assistant 语义分配后的主账本 seq。
    assistant_text_seq: Option<Seq>,
    /// Provider terminal retained while a real tool batch remains pending.
    /// 真实工具批次仍待处理时保留的 Provider 终态。
    pending_terminal: Option<ProviderV1ApiTerminalFacts>,
    /// Provider-native tool identities durably prepared for this API in original batch order.
    /// 按原始批次顺序为当前 API 可靠准备的 Provider 原生工具身份。
    prepared_tools: BTreeMap<String, ActiveProviderV1Tool>,
    /// Whether the one declared Provider tool batch has crossed its durable Prepared boundary.
    /// 已声明的唯一 Provider 工具批次是否已经跨过可靠 Prepared 边界。
    tool_batch_prepared: bool,
}

/// Stream-local validation state for one durably represented V1 tool event.
/// 一条已可靠表达的 V1 工具事件的流局部校验状态。
struct ActiveProviderV1Tool {
    /// Exact Provider request retained for Started-boundary equality checks.
    /// 为 Started 边界相等性校验保留的精确 Provider 请求。
    request: ToolInvocationRequest,
    /// Exact resolved tool binding fingerprint retained at batch preparation.
    /// 在批次准备时保留的精确已解析工具绑定指纹。
    binding_fingerprint: String,
    /// Session-local identity allocated on the durable Prepared event.
    /// 在可靠 Prepared 事件上分配的会话本地身份。
    tool_call_id: ToolCallId,
    /// Current lifecycle state mirrored from the authoritative session event.
    /// 从权威会话事件镜像的当前生命周期状态。
    state: ProviderV1ToolState,
}

/// Valid stream-local transitions for one V1 tool event.
/// 一条 V1 工具事件允许的流局部转换。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderV1ToolState {
    /// The request is durable and its body has not started.
    /// 请求已经可靠落盘且主体尚未启动。
    Prepared,
    /// The tool body has started and no terminal result is durable yet.
    /// 工具主体已经启动且尚无可靠终态结果。
    Running,
    /// The exact terminal result is durable.
    /// 精确终态结果已经可靠落盘。
    Finished,
}

/// Outcome of accepting one typed Provider terminal boundary.
/// 接纳一个强类型 Provider 终态边界的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderV1TerminalOutcome {
    /// The physical API group was durably closed.
    /// 物理 API 组已经可靠闭合。
    Completed,
    /// API statistics are deferred until the declared tool batch completes.
    /// API 统计延迟到已声明工具批次完成后闭合。
    AwaitingTools,
}

/// Explicit failures emitted by the Provider-to-V1 projection boundary.
/// Provider 到 V1 投影边界发出的显式失败。
#[derive(Debug)]
pub(crate) enum ProviderV1SinkError {
    /// The V1 session owner rejected a mutation or durable flush.
    /// V1 会话所有者拒绝了一次修改或可靠 flush。
    Runtime(SessionRuntimeError),
    /// The host clock could not provide a V1 event timestamp.
    /// 宿主时钟无法提供 V1 事件时间戳。
    Timestamp(String),
    /// The stream adapter received an event without the required typed tool boundary.
    /// 流适配器收到的事件缺少必需的强类型工具边界。
    MissingToolExecutionBoundary,
    /// Durable tool completion could not update its non-transcript lifecycle surfaces.
    /// 可靠工具完成后无法更新其非转录生命周期界面。
    ToolLifecycleProjection(String),
    /// A new request arrived while another physical request remained active.
    /// 另一物理请求仍活动时到达了新请求。
    RequestAlreadyActive,
    /// A semantic or terminal event arrived without an active physical request.
    /// 没有活动物理请求时到达了语义或终态事件。
    RequestNotActive,
    /// Started telemetry did not carry the required typed V1 request facts.
    /// Started 遥测没有携带必需的强类型 V1 请求事实。
    MissingTypedRequestFacts,
    /// A legacy Started durability boundary attempted to create a second request-start owner.
    /// 旧 Started 可靠边界试图创建第二个请求启动所有者。
    UnexpectedRequestStartDurabilityBoundary,
    /// Completed telemetry did not carry the required typed V1 terminal facts.
    /// Completed 遥测没有携带必需的强类型 V1 终态事实。
    MissingTypedTerminalFacts,
    /// A terminal fact did not identify the active physical request exactly.
    /// 终态事实没有精确标识当前活动物理请求。
    RequestIdentityMismatch,
    /// A second terminal fact arrived while a tool-bearing terminal was already retained.
    /// 已保留含工具终态时又到达了第二个终态事实。
    TerminalAlreadyPending,
    /// A tool boundary arrived before the successful Provider terminal declared its batch.
    /// 成功 Provider 终态声明工具批次前到达了工具边界。
    ToolTerminalNotPending,
    /// A tool-bearing successful terminal omitted its typed Provider replay source.
    /// 含工具的成功终态缺少强类型 Provider 重放来源。
    ToolProviderSourceMissing,
    /// The Prepared batch size differs from the exact Provider terminal declaration.
    /// Prepared 批次数量与精确 Provider 终态声明不一致。
    ToolBatchSizeMismatch {
        /// Tool count declared by the Provider terminal.
        /// Provider 终态声明的工具数量。
        declared: usize,
        /// Tool count carried by the Prepared boundary.
        /// Prepared 边界携带的工具数量。
        actual: usize,
    },
    /// A Prepared batch repeats one Provider-native tool-call identity.
    /// Prepared 批次重复了一个 Provider 原生工具调用身份。
    DuplicateProviderToolCallId(String),
    /// One Provider stream identity could not enter the strict V1 local-id domain.
    /// 一个 Provider 流身份无法进入严格的 V1 本地身份域。
    InvalidRequestStreamIdentity {
        /// Exact rejected Provider stream identity.
        /// 被拒绝的精确 Provider 流身份。
        request_stream_id: String,
        /// Exact Core validation failure.
        /// Core 返回的精确校验失败。
        error: DomainError,
    },
    /// One prepared tool payload failed strict Core validation before mutation.
    /// 一个待准备工具载荷在修改前未通过严格 Core 校验。
    InvalidToolPayload {
        /// Exact tool name owning the rejected payload.
        /// 拥有被拒载荷的精确工具名。
        tool_name: String,
        /// Exact Core validation failure.
        /// Core 返回的精确校验失败。
        error: DomainError,
    },
    /// A Started or Finished boundary references no durably prepared tool.
    /// Started 或 Finished 边界引用了不存在可靠 Prepared 前驱的工具。
    ToolNotPrepared(String),
    /// A tool boundary does not match the exact request or legal lifecycle state.
    /// 工具边界与精确请求或合法生命周期状态不一致。
    ToolBoundaryMismatch(String),
    /// The canonical ChangeSet converter rejected a declared change-set.
    /// canonical ChangeSet 转换器拒绝了一份已声明的 change-set。
    ChangeSet(PersistentChangeSetError),
    /// A stateful tool returned an invalid navigation-resource contract.
    /// 有状态工具返回了无效的导航资源契约。
    ExternalResource(ProviderV1ExternalResourceError),
    /// A provider-generated attachment host contract is malformed.
    /// Provider 生成附件宿主契约畸形。
    GeneratedResource(ProviderV1GeneratedResourceError),
    /// The project resource owner rejected generated-attachment registration.
    /// 项目资源 owner 拒绝了生成附件注册。
    ResourceCatalog(ProjectResourceCatalogError),
    /// Blocking generated-resource registration could not return to the stream task.
    /// 阻塞式生成资源注册无法返回流任务。
    GeneratedResourceWorker(String),
    /// A generated attachment exposed a preview path without a typed preview resource contract.
    /// 生成附件暴露了预览路径，但缺少强类型预览资源契约。
    UntypedGeneratedPreview {
        /// Exact attachment position in producer order.
        /// 生产者顺序中的精确附件位置。
        attachment_index: usize,
        /// Stable display file name.
        /// 稳定显示文件名。
        file_name: String,
    },
    /// Project registration returned a count different from the producer attachment count.
    /// 项目注册返回的数量与生产者附件数量不同。
    GeneratedResourceCountMismatch {
        /// Number of canonical generated attachments.
        /// 规范生成附件数量。
        attachments: usize,
        /// Number of project resource ids returned by the owner.
        /// 项目 owner 返回的资源 ID 数量。
        resource_ids: usize,
    },
}

impl fmt::Display for ProviderV1SinkError {
    /// Formats one stable service-boundary failure.
    /// 格式化一条稳定的服务边界失败。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Runtime(error) => write!(formatter, "{error}"),
            Self::Timestamp(error) => write!(formatter, "V1 timestamp acquisition failed: {error}"),
            Self::MissingToolExecutionBoundary => formatter.write_str("The V1 tool event has no typed execution boundary."),
            Self::ToolLifecycleProjection(error) => write!(formatter, "V1 tool lifecycle projection failed: {error}"),
            Self::RequestAlreadyActive => {
                formatter.write_str("A V1 Provider request is already active.")
            }
            Self::RequestNotActive => formatter.write_str("No V1 Provider request is active."),
            Self::MissingTypedRequestFacts => {
                formatter.write_str("The Provider Started boundary has no typed V1 request facts.")
            }
            Self::UnexpectedRequestStartDurabilityBoundary => formatter.write_str(
                "A Provider Started durability boundary cannot replace the committed V1 preparation permit.",
            ),
            Self::MissingTypedTerminalFacts => formatter
                .write_str("The Provider Completed boundary has no typed V1 terminal facts."),
            Self::RequestIdentityMismatch => formatter
                .write_str("The V1 Provider terminal does not match the active physical request."),
            Self::TerminalAlreadyPending => {
                formatter.write_str("A V1 Provider terminal is already awaiting tools.")
            }
            Self::ToolTerminalNotPending => formatter
                .write_str("The V1 tool boundary arrived before its successful Provider terminal."),
            Self::ToolProviderSourceMissing => formatter
                .write_str("The V1 tool-bearing Provider terminal has no typed replay source."),
            Self::ToolBatchSizeMismatch { declared, actual } => write!(
                formatter,
                "The V1 Prepared tool count `{actual}` does not match Provider terminal count `{declared}`."
            ),
            Self::DuplicateProviderToolCallId(tool_call_id) => write!(
                formatter,
                "The V1 Prepared batch repeats Provider tool call `{tool_call_id}`."
            ),
            Self::InvalidRequestStreamIdentity {
                request_stream_id,
                error,
            } => write!(
                formatter,
                "The V1 request stream identity `{request_stream_id}` is invalid: {error}"
            ),
            Self::InvalidToolPayload { tool_name, error } => write!(
                formatter,
                "The V1 prepared payload for tool `{tool_name}` is invalid: {error}"
            ),
            Self::ToolNotPrepared(tool_call_id) => write!(
                formatter,
                "The V1 tool boundary references unprepared Provider tool call `{tool_call_id}`."
            ),
            Self::ToolBoundaryMismatch(tool_call_id) => write!(
                formatter,
                "The V1 tool boundary does not match the prepared lifecycle for `{tool_call_id}`."
            ),
            Self::ChangeSet(error) => {
                write!(formatter, "The V1 tool ChangeSet is invalid: {error}")
            }
            Self::ExternalResource(error) => {
                write!(
                    formatter,
                    "The V1 tool external resource is invalid: {error}"
                )
            }
            Self::GeneratedResource(error) => {
                write!(formatter, "The V1 generated-resource contract is invalid: {error}")
            }
            Self::ResourceCatalog(error) => {
                write!(formatter, "The V1 project resource registration failed: {error}")
            }
            Self::GeneratedResourceWorker(error) => write!(
                formatter,
                "The V1 generated-resource registration worker failed: {error}"
            ),
            Self::UntypedGeneratedPreview {
                attachment_index,
                file_name,
            } => write!(
                formatter,
                "Generated attachment {attachment_index} (`{file_name}`) has an untyped preview path"
            ),
            Self::GeneratedResourceCountMismatch {
                attachments,
                resource_ids,
            } => write!(
                formatter,
                "Generated attachment count `{attachments}` does not match registered resource count `{resource_ids}`"
            ),
        }
    }
}

impl std::error::Error for ProviderV1SinkError {}

impl From<SessionRuntimeError> for ProviderV1SinkError {
    /// Preserves one structured runtime failure at the stream boundary.
    /// 在流边界保留一条结构化运行时失败。
    fn from(error: SessionRuntimeError) -> Self {
        Self::Runtime(error)
    }
}

impl From<PersistentChangeSetError> for ProviderV1SinkError {
    /// Preserves one canonical ChangeSet conversion failure without fallback.
    /// 在不降级的情况下保留一条 canonical ChangeSet 转换失败。
    fn from(error: PersistentChangeSetError) -> Self {
        Self::ChangeSet(error)
    }
}

impl From<ProviderV1ExternalResourceError> for ProviderV1SinkError {
    /// Preserves one exact stateful-tool resource failure without parsing display output.
    /// 在不解析展示输出的情况下保留一条精确有状态工具资源失败。
    fn from(error: ProviderV1ExternalResourceError) -> Self {
        Self::ExternalResource(error)
    }
}

impl From<ProviderV1GeneratedResourceError> for ProviderV1SinkError {
    /// Preserves one exact generated-resource payload failure without output-text inference.
    /// 在不推断输出文本的情况下保留一条精确生成资源载荷失败。
    fn from(error: ProviderV1GeneratedResourceError) -> Self {
        Self::GeneratedResource(error)
    }
}

impl From<ProjectResourceCatalogError> for ProviderV1SinkError {
    /// Preserves one project resource registration failure at the stream boundary.
    /// 在流边界保留一条项目资源注册失败。
    fn from(error: ProjectResourceCatalogError) -> Self {
        Self::ResourceCatalog(error)
    }
}

/// Registers exact generated attachments and builds their ordered durable V1 references.
/// 注册精确生成附件，并构建其有序可靠 V1 引用。
///
/// # Parameters / 参数
///
/// - `registry`: Unique process-local owner of project resource catalogs.
///   `registry`：项目资源目录的唯一进程内 owner。
/// - `project_directory`: Exact project directory from the immutable session locator.
///   `project_directory`：来自不可变会话定位器的精确项目目录。
/// - `attachments`: Producer-owned canonical attachments in output order.
///   `attachments`：按输出顺序排列的生产者自有规范附件。
///
/// # Returns / 返回值
///
/// Ordered project resource references, or one explicit producer/catalog failure.
/// 有序项目资源引用，或一条显式生产者/目录失败。
async fn register_provider_generated_resources(
    registry: Arc<ProjectResourceCatalogRegistry>,
    project_directory: PathBuf,
    attachments: Vec<ConversationAttachment>,
) -> Result<Vec<GeneratedResourceRef>, ProviderV1SinkError> {
    if attachments.is_empty() {
        return Ok(Vec::new());
    }
    let mut kinds = Vec::with_capacity(attachments.len());
    for (attachment_index, attachment) in attachments.iter().enumerate() {
        if attachment.preview_local_path.is_some() {
            return Err(ProviderV1SinkError::UntypedGeneratedPreview {
                attachment_index,
                file_name: attachment.file_name.clone(),
            });
        }
        kinds.push(match attachment.kind {
            ConversationAttachmentKind::Audio => GeneratedResourceKind::Audio,
            ConversationAttachmentKind::Image => GeneratedResourceKind::Image,
            ConversationAttachmentKind::Video => GeneratedResourceKind::Video,
            ConversationAttachmentKind::Pdf => GeneratedResourceKind::Pdf,
            ConversationAttachmentKind::File => GeneratedResourceKind::File,
        });
    }
    let attachment_count = attachments.len();
    let resource_ids = tokio::task::spawn_blocking(move || {
        registry.resolve_or_register_managed_attachments(&project_directory, &attachments)
    })
    .await
    .map_err(|error| ProviderV1SinkError::GeneratedResourceWorker(error.to_string()))??;
    if resource_ids.len() != attachment_count {
        return Err(ProviderV1SinkError::GeneratedResourceCountMismatch {
            attachments: attachment_count,
            resource_ids: resource_ids.len(),
        });
    }
    Ok(resource_ids
        .into_iter()
        .zip(kinds)
        .map(|(resource_id, kind)| GeneratedResourceRef {
            resource_id,
            kind,
            preview_resource_id: None,
        })
        .collect())
}

/// Rebuilds the only transport candidate that may leave the V1 preparation owner.
/// 重建唯一允许离开 V1 准备 owner 的传输候选。
///
/// # Parameters
/// 参数
/// `snapshot` is the immutable durable ledger projection, `facts` is the selected non-secret
/// request route, and `candidate` contributes only transport capabilities that are not ledger facts.
/// `snapshot` 是不可变可靠账本投影，`facts` 是已选非秘密请求路由，`candidate` 仅贡献不属于
/// 账本事实的传输能力。
///
/// # Returns
/// 返回值
/// Returns the rebuilt candidate and a SHA-256 proof over the exact serialized request.
/// 返回重建候选以及对精确序列化请求计算的 SHA-256 证明。
fn rebuild_provider_transport_candidate(
    snapshot: SessionProviderProjectionSnapshot,
    facts: &ProviderV1RequestFacts,
    mut candidate: ProviderRequestPreparationCandidate,
    mut prepared_media: media::PreparedV1UserMedia,
) -> Result<SessionPreparedPhysicalRequest<ProviderRequestPreparationCandidate>, SessionRuntimeError>
{
    validate_provider_transport_candidate(facts, &candidate)?;
    validate_prompt_media_preparation_frontier(&snapshot)?;
    let mut request_messages = Vec::new();
    let mut controller_messages = Vec::new();
    let mut system_prompt_parts = Vec::new();
    let mut latest_user_prompt = String::new();
    let mut continuation = ProviderContinuation::default();

    append_dynamic_prompt_messages(
        &candidate.dynamic_prompt_layers,
        &mut request_messages,
        &mut system_prompt_parts,
    );

    for source in candidate.locked_baseline.locked_sources() {
        let role = locked_prompt_message_role(source)?;
        if matches!(role, MessageRole::System | MessageRole::Developer) {
            system_prompt_parts.push(source.content.clone());
        }
        request_messages.push(text_ai_message(role, source.content.clone()));
    }

    for (index, item) in snapshot.items.iter().enumerate() {
        let message_id = format!("v1-provider-message-{}", index + 1);
        match item {
            ProjectedItem::SystemText { role, text, .. } => {
                let message_role = projected_system_message_role(*role);
                if matches!(message_role, MessageRole::System | MessageRole::Developer) {
                    system_prompt_parts.push(text.clone());
                } else {
                    controller_messages.push(ConversationMessage {
                        id: message_id,
                        payload: ConversationPayload::UserText {
                            text: text.clone(),
                            attachments: Vec::new(),
                        },
                    });
                }
                request_messages.push(text_ai_message(message_role, text.clone()));
            }
            ProjectedItem::UserInput { text, resource_ids } => {
                // Resource-bearing prompts must use the exact prevalidated result, never an unconditional Base64 fallback.
                // 带资源提示词必须使用精确预校验结果，绝不使用无条件Base64回退。
                let (content, attachments) = if resource_ids.is_empty() {
                    (vec![ContentItem::Text { text: text.clone() }], Vec::new())
                } else {
                    let prepared = prepared_media.prompts.remove(&index).ok_or_else(|| {
                        provider_projection_runtime_error(
                            "V1 user resources have no exact prepared prompt result",
                        )
                    })?;
                    (prepared.content, prepared.attachments)
                };
                latest_user_prompt = text.clone();
                request_messages.push(AiMessage {
                    role: MessageRole::User,
                    content,
                    provider_context_items: Vec::new(),
                });
                controller_messages.push(ConversationMessage {
                    id: message_id,
                    payload: ConversationPayload::UserText {
                        text: text.clone(),
                        attachments,
                    },
                });
            }
            ProjectedItem::AssistantText {
                text,
                reasoning,
                tool_calls,
                provider_source,
            } => {
                let mut content = Vec::new();
                if let Some(reasoning) = reasoning.as_ref().filter(|value| !value.is_empty()) {
                    content.push(ContentItem::Reasoning {
                        text: reasoning.clone(),
                        signature: None,
                    });
                }
                if !text.is_empty() {
                    content.push(ContentItem::Text { text: text.clone() });
                }
                let mut controller_tool_calls = Vec::with_capacity(tool_calls.len());
                for tool_call in tool_calls {
                    let call_id = provider_wire_tool_call_id(
                        tool_call.tool_call_id,
                        tool_call.provider_tool_call_id.as_deref(),
                    );
                    let arguments = serde_json::to_value(&tool_call.input)
                        .map_err(provider_projection_runtime_error)?;
                    content.push(ContentItem::ToolCall {
                        call: AiToolCall {
                            id: call_id.clone(),
                            name: tool_call.tool_name.clone(),
                            arguments: arguments.clone(),
                            signature: None,
                        },
                    });
                    controller_tool_calls.push(ConversationToolCall {
                        id: format!("v1-tool-item-{}", tool_call.tool_call_id.value()),
                        tool_call_id: Some(call_id),
                        tool_name: tool_call.tool_name.clone(),
                        arguments,
                        argument_parse_error: None,
                    });
                }
                request_messages.push(AiMessage {
                    role: MessageRole::Assistant,
                    content,
                    provider_context_items: Vec::new(),
                });
                let provider_message_state = provider_source
                    .as_ref()
                    .and_then(|source| source.replay.as_ref())
                    .map(serde_json::to_value)
                    .transpose()
                    .map_err(provider_projection_runtime_error)?;
                controller_messages.push(ConversationMessage {
                    id: message_id.clone(),
                    payload: ConversationPayload::Assistant {
                        text: text.clone(),
                        reasoning: reasoning.clone().unwrap_or_default(),
                        provider_message_id: provider_source.as_ref().and_then(|source| {
                            source
                                .message_id
                                .clone()
                                .or_else(|| source.response_id.clone())
                        }),
                        provider_message_state,
                        usage: None,
                        tool_calls: controller_tool_calls,
                    },
                });
                if let Some(source) = provider_source {
                    continuation.provider_id = Some(source.provider.clone());
                    continuation.protocol_profile_id = source.profile.clone();
                    continuation.model_id = Some(source.model.clone());
                    continuation.last_provider_message_id = source
                        .message_id
                        .clone()
                        .or_else(|| source.response_id.clone());
                    continuation.last_local_message_id = Some(message_id);
                    continuation.context_rewrite_fingerprint =
                        Some(source.fingerprint_sha256.clone());
                }
            }
            ProjectedItem::ToolResult {
                tool_call_id,
                provider_tool_call_id,
                status,
                output,
                ..
            } => {
                let call_id =
                    provider_wire_tool_call_id(*tool_call_id, provider_tool_call_id.as_deref());
                let output =
                    serde_json::to_value(output).map_err(provider_projection_runtime_error)?;
                request_messages.push(AiMessage {
                    role: MessageRole::Tool,
                    content: vec![ContentItem::ToolResult {
                        result: AiToolResult {
                            call_id: call_id.clone(),
                            is_error: *status != ToolCallStatus::Completed,
                            content: output.clone(),
                        },
                    }],
                    provider_context_items: Vec::new(),
                });
                controller_messages.push(ConversationMessage {
                    id: message_id,
                    payload: ConversationPayload::ToolResult {
                        tool_call_id: Some(call_id),
                        is_error: *status != ToolCallStatus::Completed,
                        text: serde_json::to_string(&output)
                            .map_err(provider_projection_runtime_error)?,
                    },
                });
            }
            ProjectedItem::SubagentReturnText { text } => {
                latest_user_prompt = text.clone();
                request_messages.push(text_ai_message(MessageRole::User, text.clone()));
                controller_messages.push(ConversationMessage {
                    id: message_id,
                    payload: ConversationPayload::UserText {
                        text: text.clone(),
                        attachments: Vec::new(),
                    },
                });
            }
        }
    }

    candidate.exact_request.model_id = facts.request.model.clone();
    candidate.exact_request.messages = request_messages;
    candidate.exact_request.resources.clear();
    candidate.exact_request.continuation = None;
    candidate.controller_snapshot = ConversationSnapshot {
        messages: controller_messages.clone(),
        records: Vec::new(),
        continuation: continuation.clone(),
    };
    candidate.request_snapshot = CoreProviderRequestSnapshot {
        provider_id: facts.request.provider.clone(),
        model_id: facts.request.model.clone(),
        system_prompt: system_prompt_parts.join("\n\n"),
        prompt_layers: candidate.dynamic_prompt_layers.clone(),
        prompt: latest_user_prompt,
        prompt_in_history: true,
        history: controller_messages,
        temperature: facts.request.temperature,
        max_turns: candidate.request_snapshot.max_turns,
        tool_schemas: candidate.request_snapshot.tool_schemas.clone(),
        additional_params: candidate.request_snapshot.additional_params.clone(),
        request_headers: candidate.request_snapshot.request_headers.clone(),
        continuation,
    };
    let request_bytes =
        serde_json::to_vec(&candidate.exact_request).map_err(provider_projection_runtime_error)?;
    let fingerprint_sha256 = sha256_hex(&request_bytes);
    let projection = ProviderProjectionProof {
        segment_generation: snapshot.segment_generation,
        document_revision: snapshot.document_revision,
        retained_from_seq: snapshot.retained_from_seq,
        through_seq: snapshot.through_seq,
        fingerprint_sha256,
    };
    let request_budget_evidence = candidate
        .finalize_v1_request_budget(&projection, snapshot.provider_usage_budget_anchor.as_ref())
        .map_err(provider_projection_runtime_error)?;
    Ok(SessionPreparedPhysicalRequest {
        user_prompt_media: prepared_media.bindings,
        transport: candidate,
        projection,
        request_budget_evidence,
    })
}

/// Appends the producer-declared request-dynamic prompt layers before durable conversation history.
/// 在可靠会话历史之前追加生产者声明的请求动态提示词层。
///
/// # Parameters
/// # 参数
/// - `layers`: Dynamic-only prompt layers emitted by the unique prompt composer.
/// - `layers`：由唯一提示词组合器生成的仅动态提示词层。
/// - `messages`: Exact Provider-neutral request message buffer.
/// - `messages`：精确的 Provider 中立请求消息缓冲区。
/// - `system_prompt_parts`: Controller snapshot system/developer prompt summary buffer.
/// - `system_prompt_parts`：控制器快照的系统/开发者提示摘要缓冲区。
fn append_dynamic_prompt_messages(
    layers: &ProviderPromptLayers,
    messages: &mut Vec<AiMessage>,
    system_prompt_parts: &mut Vec<String>,
) {
    if !layers.system.trim().is_empty() {
        system_prompt_parts.push(layers.system.clone());
        messages.push(text_ai_message(MessageRole::System, layers.system.clone()));
    }
    if !layers.developer.trim().is_empty() {
        system_prompt_parts.push(layers.developer.clone());
        messages.push(text_ai_message(
            MessageRole::Developer,
            layers.developer.clone(),
        ));
    }
    if !layers.tool_guidance.trim().is_empty() {
        let rendered = ProviderPromptLayers {
            tool_guidance: layers.tool_guidance.clone(),
            ..ProviderPromptLayers::default()
        }
        .render_fallback_system_prompt();
        system_prompt_parts.push(rendered.clone());
        messages.push(text_ai_message(MessageRole::System, rendered));
    }
    if !layers.runtime_context.trim().is_empty() {
        messages.push(text_ai_message(
            MessageRole::User,
            layers.runtime_context.clone(),
        ));
    }
}

/// Validates that pre-gate route facts do not drift from the selected V1 request configuration.
/// 校验闸门前路由事实没有偏离已选 V1 请求配置。
fn validate_provider_transport_candidate(
    facts: &ProviderV1RequestFacts,
    candidate: &ProviderRequestPreparationCandidate,
) -> Result<(), SessionRuntimeError> {
    if candidate.exact_request.model_id != facts.request.model
        || candidate.request_snapshot.provider_id != facts.request.provider
        || candidate.request_snapshot.model_id != facts.request.model
        || candidate.request_snapshot.temperature != facts.request.temperature
    {
        return Err(SessionRuntimeError::ProviderProjectionInvariant(
            String::from(
                "pre-gate transport route differs from the selected V1 request configuration",
            ),
        ));
    }
    if let Some(context) = &candidate.media_context {
        // The carrier must be the same frozen execution as this request, not a new global model selection.
        // 载体必须来自本请求相同的冻结执行，而非新的全局模型选择。
        let execution = &context.execution;
        if execution.model.id != facts.request.model
            || execution.route.provider.id != facts.request.provider
            || Some(execution.route.connection.id.as_str())
                != facts.request.connection_id.as_deref()
            || Some(execution.route.protocol_route.protocol_profile_id.as_str())
                != facts.request.profile.as_deref()
            || candidate.exact_request.cache_scope_id.as_deref()
                != Some(context.session_scope_id.as_str())
        {
            return Err(provider_projection_runtime_error(
                "V1 media execution differs from the frozen request route or session scope",
            ));
        }
    }
    Ok(())
}

/// Validates Core-owned new prompt eligibility in `snapshot` before any resource I/O can begin.
/// 在任何资源IO开始前校验`snapshot`中由Core持有的新提示词资格。
/// Returns an explicit invariant error for reordered, missing or non-pending eligible prompt identities.
/// 对乱序、缺失或并非待准备的合格提示词身份返回明确不变量错误。
fn validate_prompt_media_preparation_frontier(
    snapshot: &SessionProviderProjectionSnapshot,
) -> Result<(), SessionRuntimeError> {
    // Eligibility is chronological and must point to one exact user prompt occurrence, never a resource-cache owner.
    // 资格遵循时序，必须指向一次精确用户提示词出现，而非资源缓存所有者。
    if snapshot.preparing_user_prompt_media_seqs.windows(2).any(|pair| pair[0] >= pair[1])
        || snapshot.preparing_user_prompt_media_seqs.iter().any(|seq| {
            !snapshot.sourced_items.iter().any(|item| {
                item.source_from_seq == *seq && item.source_to_seq == *seq
                    && matches!(&item.item, ProjectedItem::UserInput { resource_ids, .. } if !resource_ids.is_empty())
            && item.user_prompt_parameters.as_ref().is_some_and(|parameters| matches!(parameters.media_delivery, vulcan_session_core::v1::UserPromptMediaDelivery::Pending | vulcan_session_core::v1::UserPromptMediaDelivery::Preparing { .. }))
            })
        })
    {
        return Err(provider_projection_runtime_error("V1 prompt media preparation frontier does not match chronological source projection"));
    }
    Ok(())
}

/// Converts one locked baseline target into its exact Provider-neutral message role.
/// 把一个锁定基线目标转换为精确的 Provider 中立消息角色。
fn locked_prompt_message_role(source: &LockedSource) -> Result<MessageRole, SessionRuntimeError> {
    match source.target {
        LockedPromptTarget::System => Ok(MessageRole::System),
        LockedPromptTarget::Developer => Ok(MessageRole::Developer),
        LockedPromptTarget::User => Ok(MessageRole::User),
        LockedPromptTarget::ToolMetadata => {
            Err(SessionRuntimeError::ProviderProjectionInvariant(format!(
                "locked baseline node `{}` targets tool metadata without a frozen transport mapping",
                source.node_id
            )))
        }
    }
}

/// Converts one dynamic system role into the exact Provider-neutral message role.
/// 把一个动态系统角色转换为精确的 Provider 中立消息角色。
const fn projected_system_message_role(role: ProjectedSystemRole) -> MessageRole {
    match role {
        ProjectedSystemRole::System => MessageRole::System,
        ProjectedSystemRole::Developer => MessageRole::Developer,
        ProjectedSystemRole::HiddenUserShell => MessageRole::User,
    }
}

/// Builds one text-only Provider-neutral message without native replay state.
/// 构建一条不携带原生重放状态的纯文本 Provider 中立消息。
fn text_ai_message(role: MessageRole, text: String) -> AiMessage {
    AiMessage {
        role,
        content: vec![ContentItem::Text { text }],
        provider_context_items: Vec::new(),
    }
}

/// Maps a validated MIME value to the Provider-neutral resource category.
/// 把经过校验的 MIME 值映射到 Provider 中立资源类别。
fn resource_kind_from_mime(mime: &str) -> ResourceKind {
    // MIME classification follows the case-insensitive target registry without changing the persisted source MIME.
    // MIME分类遵循不区分大小写的目标注册表，不改变持久化来源MIME。
    let normalized = mime.to_ascii_lowercase();
    if normalized.starts_with("image/") {
        ResourceKind::Image
    } else if normalized.starts_with("audio/") {
        ResourceKind::Audio
    } else if normalized.starts_with("video/") {
        ResourceKind::Video
    } else {
        ResourceKind::File
    }
}

/// Computes the canonical lowercase SHA-256 hexadecimal digest of exact request bytes.
/// 计算精确请求字节的规范小写 SHA-256 十六进制摘要。
fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .flat_map(|byte| {
            [
                char::from_digit(u32::from(byte >> 4), 16)
                    .expect("a four-bit nibble is always hexadecimal"),
                char::from_digit(u32::from(byte & 0x0f), 16)
                    .expect("a four-bit nibble is always hexadecimal"),
            ]
        })
        .collect()
}

/// Returns the exact wire-visible tool id without relying on Provider order or tool name.
/// 返回精确线路可见工具 id，不依赖 Provider 顺序或工具名。
fn provider_wire_tool_call_id(local_id: ToolCallId, provider_tool_call_id: Option<&str>) -> String {
    provider_tool_call_id
        .map(str::to_owned)
        .unwrap_or_else(|| format!("v1-tool-{}", local_id.value()))
}

/// Wraps one request-projection failure in the structured session-owner error.
/// 把一次请求投影失败包装为结构化会话 owner 错误。
fn provider_projection_runtime_error(error: impl fmt::Display) -> SessionRuntimeError {
    SessionRuntimeError::ProviderProjectionInvariant(error.to_string())
}

impl ProviderV1StreamSink {
    /// Creates one stream-local sink around the exact registered session handle.
    /// 围绕精确注册的会话句柄创建一个流局部 sink。
    ///
    /// # Parameters
    /// 参数
    /// `handle` is the unique session owner retained until the stream task exits;
    /// `project_resource_catalog_registry` owns project-level generated-resource registration.
    /// `handle` 是保留到流任务退出的唯一会话所有者；`project_resource_catalog_registry`
    /// 持有项目级生成资源注册。
    ///
    /// # Returns
    /// 返回值
    /// Returns an idle sink ready for one typed request boundary.
    /// 返回一个等待强类型请求边界的空闲 sink。
    pub(crate) fn new(
        handle: Arc<SessionHandle>,
        project_resource_catalog_registry: Arc<ProjectResourceCatalogRegistry>,
    ) -> Self {
        Self {
            handle,
            project_resource_catalog_registry,
            active: None,
            api_terminal_completion: None,
            turn_completion: None,
            completed_assistant_text: None,
            completed_locked_baseline: None,
        }
    }

    /// Transfers all durable V1 terminal facts to the finished-stream owner exactly once.
    /// 将全部可靠 V1 终态事实向流结束所有者精确转移一次。
    pub(crate) fn take_finished_durability(&mut self) -> ProviderV1FinishedDurability {
        ProviderV1FinishedDurability {
            api_terminal: self.api_terminal_completion.take(),
            turn_completion: self.turn_completion.take(),
            completed_assistant_text: self.completed_assistant_text.take(),
            locked_baseline: self.completed_locked_baseline.take(),
        }
    }

    /// Returns the exact final assistant semantic retained for parent subagent return projection.
    /// 返回为父级子代理回流投影保留的精确最终 assistant 语义。
    ///
    /// # Returns / 返回值
    ///
    /// The latest completed assistant fact without consuming stream-finished ownership.
    /// 最新已完成 assistant 事实，且不消费流终态所有权。
    pub(crate) fn completed_assistant_text(&self) -> Option<&ProviderV1CompletedAssistantText> {
        self.completed_assistant_text.as_ref()
    }

    /// Requests the unique V1 preparation owner to rebuild and commit one physical API.
    /// 请求唯一 V1 准备所有者重建并提交一次物理 API。
    ///
    /// # Parameters
    /// 参数
    /// `facts` owns the typed physical identity and `candidate` is the pre-gate transport input.
    /// `facts` 持有强类型物理身份，`candidate` 是闸门前传输输入。
    ///
    /// # Returns
    /// 返回值
    /// Returns a dispatch permit only after exact V1 projection and durable api_started commit.
    /// 仅在完成精确 V1 投影与可靠 api_started 提交后返回分派许可。
    pub(crate) async fn prepare_request(
        &mut self,
        facts: ProviderV1RequestFacts,
        candidate: ProviderRequestPreparationCandidate,
    ) -> Result<PreparedPhysicalApi, ProviderV1SinkError> {
        if self.active.is_some() {
            return Err(ProviderV1SinkError::RequestAlreadyActive);
        }
        validate_provider_transport_candidate(&facts, &candidate)?;
        let locked_baseline = candidate.locked_baseline.clone();
        let purpose = facts.purpose;
        let request = facts.request.clone();
        let continuation_action_id = facts.continuation_action_id.clone();
        let prepared_at_ms = facts.prepared_at_ms;
        let facts_for_build = facts.clone();
        // Private upload receipts use the existing project owner; prompt identity remains in the session owner.
        // 私有上传回执使用既有项目所有者；提示词身份仍归会话所有者。
        let resource_registry = Arc::clone(&self.project_resource_catalog_registry);
        let project_directory = self.handle.storage_locator()?.project_directory();
        let receipt = self
            .handle
            .prepare_physical_api_durable(
                purpose,
                request,
                locked_baseline.clone(),
                continuation_action_id,
                prepared_at_ms,
                move |snapshot, preparation| async move {
                    // Plan and materialize all resources before the durable API permit, outside both session locks.
                    // 在可靠API许可之前、两把会话锁之外规划和物化全部资源。
                    let prepared_media = media::prepare_v1_user_media_durable(
                        &snapshot,
                        candidate.media_context.as_ref(),
                        resource_registry,
                        Arc::clone(&preparation),
                    )
                    .await?;
                    // Blocking resource reads and encoding run outside the session writer and publisher locks.
                    // 阻塞资源读取与编码在会话写入器、发布器锁之外运行。
                    tokio::task::spawn_blocking(move || {
                        // Rebuild from a real durable recapture after our own prompt writes; external changes still require a fresh request.
                        // 自身提示词写入后从真实可靠重捕获重建；外部变化仍要求全新请求。
                        let Some(snapshot) = preparation.recapture()? else {
                            return Ok(None);
                        };
                        rebuild_provider_transport_candidate(
                            snapshot,
                            &facts_for_build,
                            candidate,
                            prepared_media,
                        )
                        .map(Some)
                    })
                    .await
                    .map_err(|error| SessionRuntimeError::BlockingTaskJoin(error.to_string()))?
                },
            )
            .await?;
        let Some(receipt) = receipt else {
            return Ok(PreparedPhysicalApi::RequestConfigurationChanged);
        };
        let prepared = receipt.value;
        let core_start = prepared.core_start;
        let transport = prepared.transport;
        // A newly prepared API supersedes the previous physical terminal as the stream's final API.
        // 新准备的 API 会取代上一物理终态，成为当前流的最终物理 API。
        self.api_terminal_completion = None;
        self.turn_completion = None;
        self.completed_assistant_text = None;
        self.completed_locked_baseline = None;
        self.active = Some(ActiveProviderV1Projection {
            api_call_id: core_start.api_call_id,
            request_facts: facts,
            locked_baseline,
            project_directory,
            reasoning_text: String::new(),
            reasoning_seq: None,
            reasoning_sealed: false,
            assistant_text: String::new(),
            assistant_text_seq: None,
            pending_terminal: None,
            prepared_tools: BTreeMap::new(),
            tool_batch_prepared: false,
        });
        Ok(PreparedPhysicalApi::SessionV1 {
            exact_request: transport.exact_request,
            controller_config: transport.controller_config,
            controller_snapshot: transport.controller_snapshot,
            request_snapshot: transport.request_snapshot,
            context_budget: transport.context_budget,
            core_start,
            provider_api_trace_store: self.handle.provider_api_trace_store(),
        })
    }

    /// Durably binds one real wire dispatch to the active local V1 API identity.
    /// 把一次真实线路分派可靠绑定到当前本地 V1 API 身份。
    ///
    /// # Parameters
    /// 参数
    /// `facts` carries the exact prepared request identity and typed dispatch timestamp.
    /// `facts` 携带精确已准备请求身份与强类型分派时间戳。
    ///
    /// # Returns
    /// 返回值
    /// Returns only after the transport-observed fact is durable; an exact duplicate remains
    /// idempotent and still confirms any previously dirty revision.
    /// 仅在传输已观测事实可靠落盘后返回；精确重复保持幂等，并仍确认此前任意脏修订。
    pub(crate) async fn mark_transport_observed(
        &mut self,
        facts: ProviderV1TransportObservedFacts,
    ) -> Result<(), ProviderV1SinkError> {
        let active = self
            .active
            .as_ref()
            .ok_or(ProviderV1SinkError::RequestNotActive)?;
        if active.request_facts != facts.request_facts {
            return Err(ProviderV1SinkError::RequestIdentityMismatch);
        }
        self.handle
            .mark_api_transport_observed_durable(active.api_call_id, facts.dispatched_at_ms)
            .await?;
        Ok(())
    }

    /// Appends one ordered reasoning delta to the in-memory V1 semantic event.
    /// 把一条有序思考增量追加到内存 V1 语义事件。
    ///
    /// # Parameters
    /// 参数
    /// `delta` is the exact Provider fragment and `now_ms` is its host observation time.
    /// `delta` 是精确 Provider 片段，`now_ms` 是宿主观测时间。
    ///
    /// # Returns
    /// 返回值
    /// Returns after the authoritative in-memory event contains the complete accumulated text.
    /// 在权威内存事件包含完整累积正文后返回。
    pub(crate) fn append_reasoning_delta(
        &mut self,
        delta: &str,
        now_ms: i64,
    ) -> Result<(), ProviderV1SinkError> {
        let active = self
            .active
            .as_mut()
            .ok_or(ProviderV1SinkError::RequestNotActive)?;
        if active.pending_terminal.is_some() {
            return Err(ProviderV1SinkError::TerminalAlreadyPending);
        }
        let mut next_text = active.reasoning_text.clone();
        next_text.push_str(delta);
        let receipt = self.handle.update_reasoning(
            active.api_call_id,
            next_text.clone(),
            AssistantSemanticStatus::Streaming,
            now_ms,
        )?;
        active.reasoning_seq = Some(receipt.value);
        active.reasoning_text = next_text;
        active.reasoning_sealed = false;
        Ok(())
    }

    /// Seals the accumulated reasoning semantic without forcing a disk flush.
    /// 封口已累积思考语义，但不强制磁盘 flush。
    ///
    /// # Parameters
    /// 参数
    /// `now_ms` is the exact completion observation time.
    /// `now_ms` 是精确完成观测时间。
    ///
    /// # Returns
    /// 返回值
    /// Returns after a non-empty reasoning event reaches Completed.
    /// 在非空思考事件到达 Completed 后返回。
    pub(crate) fn complete_reasoning(&mut self, now_ms: i64) -> Result<(), ProviderV1SinkError> {
        let active = self
            .active
            .as_mut()
            .ok_or(ProviderV1SinkError::RequestNotActive)?;
        if active.reasoning_text.is_empty() || active.reasoning_sealed {
            return Ok(());
        }
        let receipt = self.handle.update_reasoning(
            active.api_call_id,
            active.reasoning_text.clone(),
            AssistantSemanticStatus::Completed,
            now_ms,
        )?;
        active.reasoning_seq = Some(receipt.value);
        active.reasoning_sealed = true;
        Ok(())
    }

    /// Appends one ordered assistant delta to the in-memory V1 semantic event.
    /// 把一条有序 assistant 增量追加到内存 V1 语义事件。
    ///
    /// # Parameters
    /// 参数
    /// `delta` is the exact Provider fragment and `now_ms` is its host observation time.
    /// `delta` 是精确 Provider 片段，`now_ms` 是宿主观测时间。
    ///
    /// # Returns
    /// 返回值
    /// Returns after reasoning is sealed and the complete assistant text is updated.
    /// 在思考封口且完整 assistant 正文更新后返回。
    pub(crate) fn append_assistant_delta(
        &mut self,
        delta: &str,
        now_ms: i64,
    ) -> Result<(), ProviderV1SinkError> {
        self.complete_reasoning(now_ms)?;
        let active = self
            .active
            .as_mut()
            .ok_or(ProviderV1SinkError::RequestNotActive)?;
        if active.pending_terminal.is_some() {
            return Err(ProviderV1SinkError::TerminalAlreadyPending);
        }
        let mut next_text = active.assistant_text.clone();
        next_text.push_str(delta);
        let receipt = self.handle.update_assistant_text(
            active.api_call_id,
            next_text.clone(),
            AssistantSemanticStatus::Streaming,
            now_ms,
        )?;
        active.assistant_text_seq = Some(receipt.value);
        active.assistant_text = next_text;
        Ok(())
    }

    /// Accepts one typed API terminal and durably closes a tool-free group.
    /// 接纳一个强类型 API 终态，并可靠闭合无工具调用组。
    ///
    /// # Parameters
    /// 参数
    /// `terminal` is the exact terminal fact attached to the existing durability boundary.
    /// `terminal` 是附着到既有可靠性边界的精确终态事实。
    ///
    /// # Returns
    /// 返回值
    /// Returns `AwaitingTools` without writing `api_stats` when tool calls were declared.
    /// 声明了工具调用时返回 `AwaitingTools`，且不写入 `api_stats`。
    pub(crate) async fn accept_terminal(
        &mut self,
        terminal: ProviderV1ApiTerminalFacts,
    ) -> Result<ProviderV1TerminalOutcome, ProviderV1SinkError> {
        let active = self
            .active
            .as_ref()
            .ok_or(ProviderV1SinkError::RequestNotActive)?;
        if active.request_facts != terminal.request_facts {
            return Err(ProviderV1SinkError::RequestIdentityMismatch);
        }
        if active.pending_terminal.is_some() {
            return Err(ProviderV1SinkError::TerminalAlreadyPending);
        }
        self.seal_semantics_for_terminal(terminal.status, terminal.completed_at_ms)?;
        if terminal.tool_call_count > 0 {
            self.active
                .as_mut()
                .ok_or(ProviderV1SinkError::RequestNotActive)?
                .pending_terminal = Some(terminal);
            return Ok(ProviderV1TerminalOutcome::AwaitingTools);
        }
        let closes_logical_turn = terminal.status == ApiCallStatus::Completed
            && terminal.request_facts.purpose
                != vulcan_session_core::v1::event::ApiPurposeKind::Compaction;
        let completed_at_ms = terminal.completed_at_ms;
        let api_terminal = self.complete_api(terminal).await?;
        let has_promoted_in_turn_inputs = !api_terminal.value.promoted_input_seqs.is_empty();
        self.api_terminal_completion = Some(api_terminal);
        if closes_logical_turn && !has_promoted_in_turn_inputs {
            // Settlement atomically chooses closure or child-return continuation, without a read/write gap.
            // 结算原子选择关闭或子任务回流续跑，不留下读写间隙。
            let receipt = self
                .handle
                .complete_successful_turn(completed_at_ms)
                .await?;
            match receipt.value {
                SuccessfulTurnSettlement::Completed(value) => {
                    self.turn_completion = Some(SessionMutationReceipt {
                        value,
                        revision: receipt.revision,
                        changed_event_seqs: receipt.changed_event_seqs,
                        flush_reason: receipt.flush_reason,
                    });
                }
                // Terminal handling reads pending returns from the owner after releasing admission.
                // 终态处理释放准入后从所有者读取待回流结果，不保留可能过期的快照。
                SuccessfulTurnSettlement::SubagentReturn => {}
            }
        }
        Ok(ProviderV1TerminalOutcome::Completed)
    }

    /// Commits one V1 tool event, then updates lifecycle surfaces before its caller acknowledges it.
    /// 提交一个 V1 工具事件，然后在调用方确认前更新生命周期界面。
    ///
    /// Parameters: app emits lifecycle deltas, state owns live surfaces, event is the executor's exact event,
    /// and now_ms timestamps the V1 mutation. Returns the durable boundary outcome or an explicit error.
    /// 参数：app 发射生命周期增量，state 持有实时界面，event 是执行器的精确事件，
    /// now_ms 为 V1 修改提供时间戳。返回可靠边界结果或显式错误。
    pub(crate) async fn apply_tool_event(
        &mut self,
        app: &vulcan_service_api::ServiceEventEmitter,
        state: &SharedServiceBridgeState,
        event: &RuntimeEvent,
        now_ms: i64,
    ) -> Result<ProviderV1ToolBoundaryOutcome, ProviderV1SinkError> {
        // boundary is the sole typed durability source, not reconstructed display metadata.
        // boundary 是唯一的强类型可靠性来源，不是由显示元数据重建的内容。
        let boundary = event
            .tool_execution_boundary
            .as_ref()
            .ok_or(ProviderV1SinkError::MissingToolExecutionBoundary)?;
        // outcome is available only after persistence succeeds; failed boundaries must not publish lifecycle changes.
        // outcome 仅在持久化成功后可用；失败边界不得发布生命周期变更。
        let outcome = self.apply_tool_boundary(boundary, now_ms).await?;
        // This projector owns only live resources and request progress, never legacy transcript persistence.
        // 此投影器仅拥有实时资源和请求进度，不负责旧转录持久化。
        // Keeping it inside the public V1 entry prevents callers' early returns from skipping close/spawn deltas.
        // 将其放在公开 V1 入口内，防止调用方提前返回时跳过关闭或创建增量。
        super::append_runtime_event_projection(app, state, self.handle.session_id(), event)
            .map_err(ProviderV1SinkError::ToolLifecycleProjection)?;
        Ok(outcome)
    }

    /// Applies one exact tool boundary through Prepared, Running, and terminal on one event seq.
    /// 在同一事件 seq 上按 Prepared、Running 与终态应用一个精确工具边界。
    ///
    /// # Parameters
    /// 参数
    /// `boundary` is the strongly typed runtime boundary emitted by the tool executor.
    /// `boundary` 是工具执行器发出的强类型运行时边界。
    ///
    /// # Returns
    /// 返回值
    /// Returns only after the exact existing execution barrier is represented by a durable V1 revision.
    /// 仅在精确的既有执行边界被一个可靠 V1 修订表达后返回。
    async fn apply_tool_boundary(
        &mut self,
        boundary: &ToolExecutionRecoveryBoundary,
        now_ms: i64,
    ) -> Result<ProviderV1ToolBoundaryOutcome, ProviderV1SinkError> {
        match boundary {
            ToolExecutionRecoveryBoundary::BatchPrepared(requests) => self
                .prepare_tool_batch(requests, now_ms)
                .await
                .map(ProviderV1ToolBoundaryOutcome::BatchPrepared),
            ToolExecutionRecoveryBoundary::Started {
                request,
                binding_fingerprint,
            } => {
                self.start_prepared_tool(request, binding_fingerprint, now_ms)
                    .await?;
                Ok(ProviderV1ToolBoundaryOutcome::Applied)
            }
            ToolExecutionRecoveryBoundary::Finished(result) => {
                self.finish_tool(result, now_ms).await?;
                Ok(ProviderV1ToolBoundaryOutcome::Applied)
            }
        }
    }

    /// Durably reserves every Provider-ordered tool event before any tool body starts.
    /// 在任何工具主体启动前按 Provider 顺序可靠预留全部工具事件。
    async fn prepare_tool_batch(
        &mut self,
        requests: &[vulcan_agent_runtime::agent_runtime::events::ToolExecutionRecoveryRequest],
        now_ms: i64,
    ) -> Result<Vec<PreparedToolInvocationIdentity>, ProviderV1SinkError> {
        let active = self
            .active
            .as_ref()
            .ok_or(ProviderV1SinkError::RequestNotActive)?;
        let terminal = active
            .pending_terminal
            .as_ref()
            .ok_or(ProviderV1SinkError::ToolTerminalNotPending)?;
        let provider_source = terminal
            .provider_source
            .clone()
            .ok_or(ProviderV1SinkError::ToolProviderSourceMissing)?;
        if terminal.tool_call_count != requests.len() {
            return Err(ProviderV1SinkError::ToolBatchSizeMismatch {
                declared: terminal.tool_call_count,
                actual: requests.len(),
            });
        }
        let mut provider_ids = BTreeSet::new();
        for request in requests {
            if !provider_ids.insert(request.request.tool_call_id.clone()) {
                return Err(ProviderV1SinkError::DuplicateProviderToolCallId(
                    request.request.tool_call_id.clone(),
                ));
            }
        }
        let api_call_id = active.api_call_id;
        let durable_requests = requests.to_vec();
        let request_stream_ids = durable_requests
            .iter()
            .map(|request| {
                RequestStreamId::try_new(request.request_stream_id.clone()).map_err(|error| {
                    ProviderV1SinkError::InvalidRequestStreamIdentity {
                        request_stream_id: request.request_stream_id.clone(),
                        error,
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let preparations = durable_requests
            .iter()
            .zip(request_stream_ids.iter().cloned())
            .map(|(request, request_stream_id)| {
                let tool_name = request.request.tool_name.clone();
                Ok(ToolPreparation {
                    request_stream_id,
                    tool_name: tool_name.clone(),
                    input: ToolPayload::try_new(request.request.arguments.clone()).map_err(
                        |error| ProviderV1SinkError::InvalidToolPayload { tool_name, error },
                    )?,
                    provider_tool_call_id: Some(request.request.tool_call_id.clone()),
                })
            })
            .collect::<Result<Vec<_>, ProviderV1SinkError>>()?;
        let prepared = self
            .handle
            .prepare_tool_batch_with_provider_source_durable(
                api_call_id,
                preparations,
                provider_source,
                now_ms,
            )
            .await?
            .value;
        if prepared.api_call_id != api_call_id || prepared.tools.len() != durable_requests.len() {
            return Err(ProviderV1SinkError::ToolBoundaryMismatch(String::from(
                "prepared_tool_batch_receipt",
            )));
        }
        let active = self
            .active
            .as_mut()
            .ok_or(ProviderV1SinkError::RequestNotActive)?;
        let mut identities = Vec::with_capacity(prepared.tools.len());
        for ((request, request_stream_id), receipt) in durable_requests
            .into_iter()
            .zip(request_stream_ids)
            .zip(prepared.tools)
        {
            if receipt.request_stream_id != request_stream_id || receipt.api_call_id != api_call_id
            {
                return Err(ProviderV1SinkError::ToolBoundaryMismatch(
                    request.request.tool_call_id,
                ));
            }
            let provider_tool_call_id = request.request.tool_call_id.clone();
            let next = ActiveProviderV1Tool {
                request: request.request,
                binding_fingerprint: request.binding_fingerprint,
                tool_call_id: receipt.tool_call_id,
                state: ProviderV1ToolState::Prepared,
            };
            if let Some(existing) = active.prepared_tools.get(&provider_tool_call_id) {
                if existing.request != next.request
                    || existing.binding_fingerprint != next.binding_fingerprint
                    || existing.tool_call_id != next.tool_call_id
                {
                    return Err(ProviderV1SinkError::ToolBoundaryMismatch(
                        provider_tool_call_id,
                    ));
                }
            } else {
                active.prepared_tools.insert(provider_tool_call_id, next);
            }
            identities.push(receipt);
        }
        active.tool_batch_prepared = true;
        Ok(identities)
    }

    /// Promotes one durably prepared tool to Running on its existing seq.
    /// 在既有 seq 上把一条可靠 Prepared 工具提升为 Running。
    async fn start_prepared_tool(
        &mut self,
        request: &ToolInvocationRequest,
        binding_fingerprint: &str,
        now_ms: i64,
    ) -> Result<(), ProviderV1SinkError> {
        let active = self
            .active
            .as_ref()
            .ok_or(ProviderV1SinkError::RequestNotActive)?;
        let prepared = active
            .prepared_tools
            .get(&request.tool_call_id)
            .ok_or_else(|| ProviderV1SinkError::ToolNotPrepared(request.tool_call_id.clone()))?;
        if !matches!(
            prepared.state,
            ProviderV1ToolState::Prepared | ProviderV1ToolState::Running
        ) || prepared.request != *request
            || prepared.binding_fingerprint != binding_fingerprint
        {
            return Err(ProviderV1SinkError::ToolBoundaryMismatch(
                request.tool_call_id.clone(),
            ));
        }
        let api_call_id = active.api_call_id;
        let tool_call_id = prepared.tool_call_id;
        Arc::clone(&self.handle)
            .mutate_durable_if_changed_async(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::ToolBoundary,
                move |document| {
                    document
                        .start_prepared_tool(api_call_id, tool_call_id, now_ms)
                        .map(|changed| {
                            if changed {
                                SessionMutationChange::Changed(())
                            } else {
                                SessionMutationChange::Unchanged(())
                            }
                        })
                },
            )
            .await?;
        self.active
            .as_mut()
            .and_then(|active| active.prepared_tools.get_mut(&request.tool_call_id))
            .ok_or_else(|| ProviderV1SinkError::ToolNotPrepared(request.tool_call_id.clone()))?
            .state = ProviderV1ToolState::Running;
        Ok(())
    }

    /// Persists one exact tool result and closes the API exactly once after the final result.
    /// 持久化一个精确工具结果，并在最后一个结果后恰好一次关闭 API。
    async fn finish_tool(
        &mut self,
        result: &ToolInvocationResult,
        now_ms: i64,
    ) -> Result<(), ProviderV1SinkError> {
        let changeset = persistent_change_set_from_tool_result(result)?;
        let external_refs = provider_v1_external_refs_from_tool_result(result)?;
        let generated_attachments = provider_v1_generated_attachments_from_tool_result(result)?;
        let active = self
            .active
            .as_ref()
            .ok_or(ProviderV1SinkError::RequestNotActive)?;
        let prepared = active
            .prepared_tools
            .get(&result.tool_call_id)
            .ok_or_else(|| ProviderV1SinkError::ToolNotPrepared(result.tool_call_id.clone()))?;
        if prepared.state != ProviderV1ToolState::Running
            || prepared.request.tool_name != result.tool_name
        {
            return Err(ProviderV1SinkError::ToolBoundaryMismatch(
                result.tool_call_id.clone(),
            ));
        }
        let closes_api = active.prepared_tools.values().all(|tool| {
            tool.state == ProviderV1ToolState::Finished
                || tool.tool_call_id == prepared.tool_call_id
        });
        let terminal = if closes_api {
            Some(
                active
                    .pending_terminal
                    .clone()
                    .ok_or(ProviderV1SinkError::ToolTerminalNotPending)?,
            )
        } else {
            None
        };
        let api_call_id = active.api_call_id;
        let tool_call_id = prepared.tool_call_id;
        let project_directory = active.project_directory.clone();
        let provider_tool_call_id = result.tool_call_id.clone();
        let output = persistent_tool_output_from_result(result).map_err(|error| {
            ProviderV1SinkError::Runtime(SessionRuntimeError::Domain(Box::new(error)))
        })?;
        let durable_status = self
            .handle
            .read(|document| document.tool_status(api_call_id, tool_call_id))
            .map_err(ProviderV1SinkError::Runtime)?
            .map_err(|error| {
                ProviderV1SinkError::Runtime(SessionRuntimeError::Domain(Box::new(error)))
            })?;
        // A rejected Ask is durably Cancelled before its suspended future returns the canonical
        // error result. The generic ToolFinished boundary must confirm that exact terminal fact
        // instead of attempting a second Failed transition on the same tool event.
        // 被拒绝的 Ask 会在挂起 future 返回规范错误结果前可靠进入 Cancelled；通用 ToolFinished
        // 边界必须确认该精确终态，而不是在同一工具事件上再次尝试 Failed 转换。
        let terminal_status = if durable_status == ToolCallStatus::Cancelled {
            ToolCallStatus::Cancelled
        } else if result.is_error {
            ToolCallStatus::Failed
        } else {
            ToolCallStatus::Completed
        };
        let generated_resources = register_provider_generated_resources(
            Arc::clone(&self.project_resource_catalog_registry),
            project_directory,
            generated_attachments,
        )
        .await?;
        self.handle.finish_tool(
            api_call_id,
            tool_call_id,
            ToolCompletion {
                output: Some(output),
                changeset,
                status: terminal_status,
                finished_at_ms: now_ms,
                external_refs,
                generated_resources,
            },
        )?;
        if let Some(terminal) = terminal {
            let api_terminal = self.complete_api(terminal).await?;
            self.api_terminal_completion = Some(api_terminal);
        } else {
            self.active
                .as_mut()
                .and_then(|active| active.prepared_tools.get_mut(&provider_tool_call_id))
                .ok_or_else(|| ProviderV1SinkError::ToolNotPrepared(provider_tool_call_id.clone()))?
                .state = ProviderV1ToolState::Finished;
        }
        Ok(())
    }

    /// Seals reasoning and assistant semantics according to one exact API terminal.
    /// 根据一个精确 API 终态封口思考与 assistant 语义。
    ///
    /// # Parameters
    /// 参数
    /// `status` selects Completed or Interrupted and `now_ms` is the terminal observation time.
    /// `status` 选择 Completed 或 Interrupted，`now_ms` 是终态观测时间。
    ///
    /// # Returns
    /// 返回值
    /// Returns after every existing semantic event has its terminal display status.
    /// 在每条既有语义事件获得展示终态后返回。
    fn seal_semantics_for_terminal(
        &mut self,
        status: ApiCallStatus,
        now_ms: i64,
    ) -> Result<(), ProviderV1SinkError> {
        let semantic_status = if status == ApiCallStatus::Completed {
            AssistantSemanticStatus::Completed
        } else {
            AssistantSemanticStatus::Interrupted
        };
        let active = self
            .active
            .as_mut()
            .ok_or(ProviderV1SinkError::RequestNotActive)?;
        if !active.reasoning_text.is_empty() && !active.reasoning_sealed {
            let receipt = self.handle.update_reasoning(
                active.api_call_id,
                active.reasoning_text.clone(),
                semantic_status,
                now_ms,
            )?;
            active.reasoning_seq = Some(receipt.value);
            active.reasoning_sealed = true;
        }
        if !active.assistant_text.is_empty() {
            let receipt = self.handle.update_assistant_text(
                active.api_call_id,
                active.assistant_text.clone(),
                semantic_status,
                now_ms,
            )?;
            active.assistant_text_seq = Some(receipt.value);
        }
        Ok(())
    }

    /// Durably appends the unique API statistics event and returns the sink to idle.
    /// 可靠追加唯一 API 统计事件，并把 sink 恢复为空闲状态。
    ///
    /// # Parameters
    /// 参数
    /// `terminal` owns the exact request, usage, status, error, and timing facts.
    /// `terminal` 持有精确请求、用量、状态、错误与时间事实。
    ///
    /// # Returns
    /// 返回值
    /// Returns only after the terminal revision is durable.
    /// 仅在终态修订可靠落盘后返回。
    async fn complete_api(
        &mut self,
        terminal: ProviderV1ApiTerminalFacts,
    ) -> Result<SessionMutationReceipt<ApiGroupCompletionReceipt>, ProviderV1SinkError> {
        let active = self
            .active
            .as_ref()
            .ok_or(ProviderV1SinkError::RequestNotActive)?;
        let api_call_id = active.api_call_id;
        let completed_assistant_text = if terminal.status == ApiCallStatus::Completed {
            active
                .assistant_text_seq
                .map(|seq| ProviderV1CompletedAssistantText {
                    api_call_id,
                    seq,
                    text: active.assistant_text.clone(),
                })
        } else {
            None
        };
        // completedLockedBaseline is the exact baseline admitted with this physical request.
        // completedLockedBaseline 是与当前物理请求一同准入的精确基线。
        let completed_locked_baseline = active.locked_baseline.clone();
        let completion = api_completion_from_terminal(terminal);
        let receipt = Arc::clone(&self.handle)
            .mutate_durable_async(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::ApiCompleted,
                move |document| document.complete_api_group(api_call_id, completion),
            )
            .await?;
        self.completed_assistant_text = completed_assistant_text;
        self.completed_locked_baseline = Some(completed_locked_baseline);
        self.active = None;
        Ok(receipt)
    }
}

/// Converts one typed Provider terminal into the authoritative session completion input.
/// 把一份强类型 Provider 终态转换为权威会话完成输入。
fn api_completion_from_terminal(terminal: ProviderV1ApiTerminalFacts) -> ApiCompletion {
    ApiCompletion {
        request: terminal.request_facts.request,
        usage: terminal.usage,
        status: terminal.status,
        end_reason: terminal.end_reason,
        error: terminal.error,
        continuation_eligibility: terminal.continuation_eligibility,
        timing: terminal.timing,
        provider_source: terminal.provider_source,
        completed_at_ms: terminal.completed_at_ms,
    }
}

/// Converts only the exact host result of a ToolFinished boundary to the persistent ChangeSet.
/// 仅把 ToolFinished 边界的精确宿主结果转换为持久化 ChangeSet。
///
/// # Parameters
/// 参数
/// `result` is the strongly typed tool result emitted by the runtime.
/// `result` 是运行时发出的强类型工具结果。
///
/// # Returns
/// 返回值
/// Returns `None` for non-change-set results and an explicit error for malformed declared payloads.
/// 非 change-set 结果返回 `None`，畸形已声明载荷返回显式错误。
fn persistent_change_set_from_tool_result(
    result: &ToolInvocationResult,
) -> Result<Option<vulcan_change_set::PersistentChangeSet>, ProviderV1SinkError> {
    let Some(host_result) = result.host_result.as_ref() else {
        return Ok(None);
    };
    Ok(persistent_change_set_from_host_result(host_result)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use vulcan_agent_runtime::agent_runtime::events::ToolExecutionRecoveryRequest;
    use vulcan_agent_runtime::agent_runtime::provider_reasoning::ProviderRuntimeReasoningResolution;
    use vulcan_agent_runtime::agent_runtime::provider_transport_request_budget::ProviderRequestBudgetGuard;
    use vulcan_agent_runtime::agent_runtime::tool_runtime_bridge::build_tool_failure_invocation_result;
    use vulcan_ai_contract::{
        AiExecutionRequest, AiOperation, ModelCapabilities, ProviderReplayEnvelope,
    };
    use vulcan_session_core::v1::context::LockedBaselineProjection;
    use vulcan_session_core::v1::document::{
        ClientInputSubmission, FlushCandidate, IdempotentMutation, PrepareAndStartApiInput,
        SessionDocument, SubagentReturnBuildRequest,
    };
    use vulcan_session_core::v1::event::{
        ApiEndReason, ApiPurposeKind, ApiRequestSnapshot, ApiTimings, ConversationEvent, EventBody,
        ExternalResourceKind, ProviderProjectionProof,
    };
    use vulcan_session_core::v1::ids::{ClientActionId, Seq, SubagentTaskId};
    use vulcan_session_core::v1::projection::provider_visible_fingerprint;
    use vulcan_session_core::v1::stats::UsageSnapshot;
    use vulcan_workbench_model::SessionId;
    use vulcan_workspace_store::workspace_store::{SessionV1FlushResult, SessionV1StoreError};

    use crate::session_runtime_registry_v1::{
        SessionFlushReason, SessionMutationAdmission, SessionMutationChange,
        SessionPersistenceBackend,
    };

    /// Deterministic successful backend for sink state-machine tests.
    /// 用于 sink 状态机测试的确定性成功后端。
    struct TestPersistenceBackend;

    impl SessionPersistenceBackend for TestPersistenceBackend {
        /// Accepts one complete candidate and publishes its exact metadata.
        /// 接纳一份完整候选并发布其精确 metadata。
        fn flush(
            &self,
            candidate: &FlushCandidate,
        ) -> Result<SessionV1FlushResult, SessionV1StoreError> {
            Ok(SessionV1FlushResult {
                written_file_count: candidate.dirty.len(),
                published_metadata: Some(candidate.metadata.clone()),
            })
        }
    }

    /// Creates one published V1 session with an open root Turn.
    /// 创建一个已发布且拥有开放根回合的 V1 会话。
    fn published_open_turn() -> Arc<SessionHandle> {
        let session_id = SessionId::new_v4();
        let document = SessionDocument::new(session_id.to_string(), "project-1", "title", 1);
        let handle =
            SessionHandle::with_backend(session_id, document, Arc::new(TestPersistenceBackend));
        handle
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("provider-v1-sink-root").unwrap(),
                    "root",
                    Vec::new(),
                ),
                2,
            )
            .expect("publish root turn");
        handle
    }

    /// Commits one exact prepared API and mirrors its durable receipt into the stream-local sink.
    /// 提交一次精确的已准备 API，并把其可靠回执镜像到流局部 sink。
    fn prepare_sink_request(handle: &SessionHandle, sink: &mut ProviderV1StreamSink) {
        let facts = request_facts();
        let projection = handle
            .read(|document| {
                let through_seq = document.events().last().map(|event| event.seq).unwrap();
                ProviderProjectionProof {
                    segment_generation: document.segment_generation(),
                    document_revision: document.memory_revision(),
                    retained_from_seq: Seq::try_new(1).unwrap(),
                    through_seq,
                    fingerprint_sha256: provider_visible_fingerprint(
                        document.events(),
                        through_seq,
                    )
                    .unwrap(),
                }
            })
            .expect("build exact provider projection proof");
        let prepared = handle
            .prepare_and_start_api_durable(PrepareAndStartApiInput {
                purpose: facts.purpose,
                request: facts.request.clone(),
                request_budget_evidence:
                    crate::session_runtime_registry_v1::test_provider_request_budget_evidence(),
                projection,
                locked_baseline: LockedBaselineProjection::empty(),
                continuation_action_id: None,
                now_ms: facts.prepared_at_ms,
            })
            .expect("durably prepare provider API");
        assert!(
            prepared.changed,
            "first physical API preparation must commit"
        );
        sink.active = Some(ActiveProviderV1Projection {
            api_call_id: prepared.value.api_call_id,
            request_facts: facts,
            locked_baseline: LockedBaselineProjection::empty(),
            project_directory: std::env::temp_dir()
                .join(format!("vulcan-provider-v1-test-{}", handle.session_id())),
            reasoning_text: String::new(),
            reasoning_sealed: false,
            reasoning_seq: None,
            assistant_text: String::new(),
            assistant_text_seq: None,
            pending_terminal: None,
            prepared_tools: BTreeMap::new(),
            tool_batch_prepared: false,
        });
    }

    /// Returns deterministic typed request facts for one sink test.
    /// 为一个 sink 测试返回确定性的强类型请求事实。
    fn request_facts() -> ProviderV1RequestFacts {
        ProviderV1RequestFacts {
            provider_call_id: String::from("provider-call-1"),
            purpose: ApiPurposeKind::MainGeneration,
            continuation_action_id: None,
            request: ApiRequestSnapshot {
                provider: String::from("provider-1"),
                profile: Some(String::from("profile-1")),
                model: String::from("model-1"),
                connection_id: Some(String::from("connection-1")),
                thinking_level: Some(String::from("high")),
                temperature: Some(0.2),
            },
            prepared_at_ms: 3,
        }
    }

    /// Verifies exact pending prompt eligibility is checked before resource access, including missing or ranged identities.
    /// 验证资源访问前检查精确待准备提示词资格，包括缺失或区间身份。
    #[test]
    fn prompt_media_frontier_rejects_non_exact_source_before_io()
    -> Result<(), Box<dyn std::error::Error>> {
        // Only one exact source event owns this resource occurrence and its pending parameters.
        // 只有一个精确来源事件持有此次资源出现及待准备参数。
        let seq = Seq::try_new(2)?;
        let resources = vec![ResourceId::try_new(1)?];
        let item = ProjectedItem::UserInput {
            text: "prompt".into(),
            resource_ids: resources.clone(),
        };
        let mut snapshot = SessionProviderProjectionSnapshot {
            preparing_user_prompt_media_seqs: vec![seq],
            segment_generation: 1,
            document_revision: 1,
            retained_from_seq: Seq::try_new(1)?,
            through_seq: seq,
            items: vec![item.clone()],
            sourced_items: vec![vulcan_session_core::v1::projection::SourcedProjectedItem {
                user_prompt_parameters:
                    vulcan_session_core::v1::UserPromptParameters::for_new_resources(&resources),
                source_from_seq: seq,
                source_to_seq: seq,
                item,
            }],
            project_directory: PathBuf::new(),
            provider_usage_budget_anchor: None,
        };
        validate_prompt_media_preparation_frontier(&snapshot)?;
        snapshot.sourced_items[0].source_to_seq = Seq::try_new(3)?;
        assert!(validate_prompt_media_preparation_frontier(&snapshot).is_err());
        snapshot.sourced_items[0].source_to_seq = seq;
        snapshot.sourced_items[0].user_prompt_parameters = None;
        assert!(validate_prompt_media_preparation_frontier(&snapshot).is_err());
        snapshot.sourced_items[0].user_prompt_parameters =
            vulcan_session_core::v1::UserPromptParameters::for_new_resources(&resources);
        snapshot.preparing_user_prompt_media_seqs = vec![seq, seq];
        assert!(validate_prompt_media_preparation_frontier(&snapshot).is_err());
        snapshot.preparing_user_prompt_media_seqs = vec![Seq::try_new(9)?];
        assert!(validate_prompt_media_preparation_frontier(&snapshot).is_err());
        Ok(())
    }

    /// Returns one successful terminal fact for the supplied tool count.
    /// 为给定工具数量返回一份成功终态事实。
    fn terminal_facts(tool_call_count: usize) -> ProviderV1ApiTerminalFacts {
        ProviderV1ApiTerminalFacts {
            request_facts: request_facts(),
            usage: UsageSnapshot {
                input_tokens: Some(10),
                output_tokens: Some(5),
                total_tokens: Some(15),
                ..UsageSnapshot::default()
            },
            status: ApiCallStatus::Completed,
            end_reason: Some(ApiEndReason::CompletedSuccessfully),
            error: None,
            continuation_eligibility: ContinuationEligibility::Forbidden,
            timing: ApiTimings {
                prepared_at_ms: Some(3),
                completed_at_ms: Some(8),
                ..ApiTimings::default()
            },
            provider_source: Some(vulcan_session_core::v1::event::ProviderSourceInput {
                provider: String::from("provider-1"),
                profile: Some(String::from("profile-1")),
                model: String::from("model-1"),
                connection_id: Some(String::from("connection-1")),
                message_id: Some(String::from("message-1")),
                response_id: None,
                replay: Some(ProviderReplayEnvelope::new(
                    "provider-1",
                    "connection-1",
                    "https://provider.invalid/v1",
                    "profile-1",
                    "model-1",
                    Vec::new(),
                    Vec::new(),
                )),
            }),
            tool_call_count,
            completed_at_ms: 8,
        }
    }

    /// Proves that pre-gate legacy messages cannot reach the physical Provider permit.
    /// 证明闸门前旧消息无法进入物理 Provider 许可。
    #[test]
    fn provider_transport_rebuild_replaces_pre_gate_conversation() {
        let facts = request_facts();
        let legacy_message = text_ai_message(MessageRole::User, String::from("legacy-message"));
        let candidate = ProviderRequestPreparationCandidate {
            media_context: None,
            exact_request: AiExecutionRequest {
                operation: AiOperation::ConversationRespond,
                model_id: facts.request.model.clone(),
                cache_scope_id: None,
                continuation: None,
                messages: vec![legacy_message],
                tools: Vec::new(),
                native_tools: Vec::new(),
                tool_choice: Default::default(),
                parallel_tool_calls: Default::default(),
                structured_output: Default::default(),
                stop_sequences: Vec::new(),
                prompt_cache: Default::default(),
                reasoning_effort: None,
                reasoning_output: Default::default(),
                model_parameters: BTreeMap::new(),
                resources: Vec::new(),
                output_token_limit: None,
            },
            controller_config: vulcan_core::types::ControllerConfig {
                provider_id: facts.request.provider.clone(),
                model_id: facts.request.model.clone(),
                temperature: facts.request.temperature,
                ..Default::default()
            },
            controller_snapshot: ConversationSnapshot {
                messages: vec![ConversationMessage {
                    id: String::from("legacy-controller-message"),
                    payload: ConversationPayload::UserText {
                        text: String::from("legacy-message"),
                        attachments: Vec::new(),
                    },
                }],
                ..ConversationSnapshot::default()
            },
            request_snapshot: CoreProviderRequestSnapshot {
                provider_id: facts.request.provider.clone(),
                model_id: facts.request.model.clone(),
                system_prompt: String::from("legacy-system"),
                prompt_layers: ProviderPromptLayers::default(),
                prompt: String::from("legacy-message"),
                prompt_in_history: true,
                history: Vec::new(),
                temperature: facts.request.temperature,
                max_turns: 8,
                tool_schemas: Vec::new(),
                additional_params: None,
                request_headers: None,
                continuation: ProviderContinuation::default(),
            },
            context_budget: None,
            request_budget_guard: Some(ProviderRequestBudgetGuard::new_v1(
                facts.request.provider.clone(),
                facts.request.model.clone(),
                "test-profile",
                "sha256:test-route",
                ModelCapabilities::default(),
                ProviderRuntimeReasoningResolution::default(),
                None,
                None,
                None,
            )),
            dynamic_prompt_layers: ProviderPromptLayers {
                system: String::from("dynamic-system"),
                developer: String::from("dynamic-developer"),
                tool_guidance: String::from("dynamic-tool-guidance"),
                runtime_context: String::from("dynamic-runtime-context"),
            },
            locked_baseline: LockedBaselineProjection::empty(),
        };
        let snapshot = SessionProviderProjectionSnapshot {
            preparing_user_prompt_media_seqs: Vec::new(),
            segment_generation: 4,
            document_revision: 7,
            retained_from_seq: Seq::try_new(1).expect("valid retained seq"),
            through_seq: Seq::try_new(3).expect("valid through seq"),
            items: vec![ProjectedItem::UserInput {
                text: String::from("durable-v1-message"),
                resource_ids: Vec::new(),
            }],
            sourced_items: Vec::new(),
            project_directory: std::env::temp_dir().join("vulcan-v1-request-rebuild-missing"),
            provider_usage_budget_anchor: None,
        };

        let rebuilt = rebuild_provider_transport_candidate(
            snapshot,
            &facts,
            candidate,
            media::PreparedV1UserMedia::default(),
        )
        .expect("rebuild the exact V1 request");
        let serialized = serde_json::to_string(&rebuilt.transport.exact_request.messages)
            .expect("serialize rebuilt request messages");
        assert!(serialized.contains("durable-v1-message"));
        assert!(serialized.contains("dynamic-system"));
        assert!(serialized.contains("dynamic-developer"));
        assert!(serialized.contains("dynamic-tool-guidance"));
        assert!(serialized.contains("dynamic-runtime-context"));
        assert!(!serialized.contains("legacy-message"));
        assert_eq!(
            rebuilt.transport.request_snapshot.prompt_layers,
            ProviderPromptLayers {
                system: String::from("dynamic-system"),
                developer: String::from("dynamic-developer"),
                tool_guidance: String::from("dynamic-tool-guidance"),
                runtime_context: String::from("dynamic-runtime-context"),
            }
        );
        assert_eq!(
            rebuilt.transport.request_snapshot.prompt,
            "durable-v1-message"
        );
        assert!(
            rebuilt.transport.context_budget.is_some(),
            "the exact V1 transport request must pass the final budget gate"
        );
        assert!(
            rebuilt
                .request_budget_evidence
                .request_text_measurement
                .character_count
                > 0,
            "the persisted evidence must measure the rebuilt request instead of the legacy candidate"
        );
        assert_eq!(
            rebuilt
                .request_budget_evidence
                .request_text_measurement
                .history_message_count,
            u64::try_from(rebuilt.transport.request_snapshot.history.len())
                .expect("fixture history count fits u64")
        );
        assert_eq!(rebuilt.projection.fingerprint_sha256.len(), 64);
        assert_eq!(rebuilt.projection.document_revision, 7);
    }

    /// Returns one exact durable-recovery request for a Provider tool call.
    /// 为一条 Provider 工具调用返回精确的可靠恢复请求。
    fn recovery_request(
        tool_call_id: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> ToolExecutionRecoveryRequest {
        // requestStreamId is the stable Provider item identity paired with this execution identity.
        // requestStreamId 是与该执行身份精确配对的稳定 Provider item 身份。
        let request_stream_id = format!("request-stream-{tool_call_id}");
        ToolExecutionRecoveryRequest {
            request_stream_id: request_stream_id.clone(),
            request: ToolInvocationRequest {
                request_stream_id,
                tool_call_id: tool_call_id.to_string(),
                tool_name: tool_name.to_string(),
                arguments,
                argument_parse_error: None,
            },
            binding_fingerprint: format!("binding-{tool_call_id}"),
        }
    }

    /// Captures navigation pushes and checks they never precede durable V1 tool completion.
    /// 捕获导航推送，并检查推送不会早于可靠的 V1 工具完成。
    struct ToolLifecycleEventSink {
        /// Parent ledger that must already contain the completed tool when closure is published.
        /// 发布关闭状态时必须已包含完成工具的父账本。
        handle: Arc<SessionHandle>,
        /// Navigation states observed without opening or refreshing any child panel.
        /// 未打开或刷新任何子面板时观察到的导航状态。
        states: std::sync::Mutex<Vec<String>>,
    }

    impl vulcan_service_api::ServiceEventSink for ToolLifecycleEventSink {
        /// Records each child status; parameters contain the service event and no value is returned.
        /// 记录每个子任务状态；参数包含服务事件，无返回值。
        fn emit_service_event(&self, event: vulcan_service_api::ServiceEvent) {
            if let vulcan_service_api::ServiceEvent::WorkbenchDelta(envelope) = event
                && let vulcan_service_api::WorkbenchDelta::SubagentNavigationChanged(delta) =
                    envelope.delta
                && let Some(navigation) = delta.subagent_navigation
            {
                for task in navigation.tasks {
                    if task.run_state == "closed" {
                        self.handle.read(|document| {
                            assert_eq!(document.memory_revision(), document.durable_revision());
                            assert!(document.events().iter().any(|event| matches!(
                                &event.event, EventBody::ToolCall(tool) if tool.status == ToolCallStatus::Completed
                            )));
                        }).expect("closed navigation must follow durable tool completion");
                    }
                    self.states
                        .lock()
                        .expect("captured states")
                        .push(task.run_state);
                }
            }
        }
    }

    /// Exercises the real V1 event handler after closing a live child, including pending-peer and rejected-boundary cases.
    /// 关闭实时子任务后执行真实 V1 事件处理入口，包含同批工具未完成与边界被拒绝的情况。
    /// Parameters: tool_call_count selects one or two tools; start_close_tool controls whether completion is admissible.
    /// 参数：tool_call_count 选择一个或两个工具；start_close_tool 控制完成边界是否满足准入条件。
    /// Returns: none; asserts navigation and cache changes only follow successful durable tool completion.
    /// 返回值：无；断言导航与缓存变化只能发生在工具可靠完成后。
    async fn assert_v1_close_agent_navigation(tool_call_count: usize, start_close_tool: bool) {
        use crate::agent_stream::{
            ProviderTelemetryHookGuard, handle_webui_detached_child_execution_event,
        };
        use crate::tests::test_support::{
            seed_test_live_subagent_registry_from_panel_states,
            test_service_state_with_parent_session,
        };
        use vulcan_agent_runtime::agent_runtime::events::{
            ExecutionStreamEvent, RuntimeEvent, RuntimeEventKind,
        };
        use vulcan_agent_runtime::agent_runtime::provider_tool_event_format::{
            attach_tool_name_to_runtime_metadata, build_tool_result_runtime_metadata,
        };
        use vulcan_workbench_model::{SubagentTaskPanelState, SubagentTaskRunState};

        // handle and sink share the real V1 persistence owner for this execution.
        // handle 和 sink 共享本次执行的真实 V1 持久化所有者。
        let handle = published_open_turn();
        let mut sink = ProviderV1StreamSink::new(
            Arc::clone(&handle),
            Arc::new(ProjectResourceCatalogRegistry::default()),
        );
        prepare_sink_request(&handle, &mut sink);
        assert!((1..=2).contains(&tool_call_count));
        sink.accept_terminal(terminal_facts(tool_call_count))
            .await
            .expect("tool-bearing terminal");
        // request is prepared and started before close_agent changes the live registry.
        // request 在 close_agent 改变实时注册表前完成准备和启动。
        let request =
            recovery_request("close-1", "close_agent", json!({"target": "standby-agent"}));
        // requests can include an unfinished sibling, so closure must not wait for the whole API batch.
        // requests 可包含尚未完成的同批工具，因此关闭同步不得等待整批 API 完成。
        let mut requests = vec![request.clone()];
        if tool_call_count == 2 {
            requests.push(recovery_request("list-2", "list_agents", json!({})));
        }
        sink.apply_tool_boundary(&ToolExecutionRecoveryBoundary::BatchPrepared(requests), 9)
            .await
            .expect("prepared close");
        if start_close_tool {
            sink.apply_tool_boundary(
                &ToolExecutionRecoveryBoundary::Started {
                    request: request.request.clone(),
                    binding_fingerprint: request.binding_fingerprint.clone(),
                },
                10,
            )
            .await
            .expect("running close");
        }
        // child is completed but still open, exactly matching the reported standby state.
        // child 已完成但仍开放，与报告中的待命状态一致。
        let child = SubagentTaskPanelState {
            task_id: SessionId::new_v4(),
            parent_session_id: handle.session_id(),
            public_task_id: String::from("standby-agent"),
            subagent_label: String::from("standby-agent"),
            description: None,
            run_state: SubagentTaskRunState::Completed,
            result_text: String::from("online"),
            conversation_state: None,
        };
        // service_state seeds the production runtime registry, not a synthetic close projection.
        // service_state 填充生产运行时注册表，而不是合成关闭投影。
        let mut service_state = test_service_state_with_parent_session(handle.session_id());
        seed_test_live_subagent_registry_from_panel_states(
            &mut service_state,
            handle.session_id(),
            std::slice::from_ref(&child),
        );
        service_state.subagent_surface_detail_cache.insert(
            child.task_id,
            crate::state::WebuiSubagentSurfaceDetailCacheEntry {
                task_state: child.clone(),
                last_access_ms: 0,
            },
        );
        // runtime performs the actual close; state retains the same live cache used by the event handler.
        // runtime 执行真实关闭；state 保留事件处理器使用的同一实时缓存。
        let runtime = service_state.agent_runtime.clone().expect("seeded runtime");
        let state = Arc::new(std::sync::Mutex::new(service_state));
        // capture observes the same service deltas consumed by the frontend list.
        // capture 观察前端列表消费的同一类服务增量。
        let capture = Arc::new(ToolLifecycleEventSink {
            handle: Arc::clone(&handle),
            states: std::sync::Mutex::new(Vec::new()),
        });
        // app forwards service deltas to the test observer without any navigation refresh.
        // app 将服务增量转发给测试观察器，不触发任何导航刷新。
        let app = vulcan_service_api::ServiceEventEmitter::new(vec![capture.clone()]);
        crate::emit_webui_subagent_navigation_changed_delta_for_parent(
            &app,
            &state,
            handle.session_id(),
        )
        .expect("initial standby navigation");
        // closed is the real tool result produced after the registry archive commits.
        // closed 是注册表归档提交后产出的真实工具结果。
        let closed = runtime
            .task_runtime
            .invoke_close_agent(handle.session_id(), json!({"target": "standby-agent"}))
            .await
            .expect("close actual child");
        // host_result retains the runtime producer's exact kind and payload without synthesizing closure fields.
        // host_result 保留运行时生产者的精确 kind 和 payload，不合成关闭字段。
        let host_result = closed.host_result.expect("close host result");
        // result preserves the production executor's typed tool completion envelope.
        // result 保留生产执行器的强类型工具完成信封。
        let result = ToolInvocationResult {
            tool_call_id: String::from("close-1"),
            tool_name: String::from("close_agent"),
            model_output: closed.model_output.to_string(),
            host_result: Some(json!({"kind": host_result.kind, "payload": host_result.payload})),
            is_error: false,
        };
        // metadata is generated with the same formatter as StreamingRuntimeToolExecutor.
        // metadata 由 StreamingRuntimeToolExecutor 使用的同一格式化器生成。
        let metadata = attach_tool_name_to_runtime_metadata(
            Some(build_tool_result_runtime_metadata(
                "close-1",
                result.host_result.as_ref(),
                "completed",
            )),
            "close_agent",
        );
        // event carries both the typed V1 boundary and the executor's canonical display metadata.
        // event 同时携带强类型 V1 边界与执行器的规范显示元数据。
        let event = RuntimeEvent::new(
            RuntimeEventKind::ToolFinished,
            "工具返回 · close-1",
            "closed",
        )
        .with_tool_execution_boundary(ToolExecutionRecoveryBoundary::Finished(result))
        .with_metadata(metadata);
        // telemetry uses the same service state as the production detached-event entry.
        // telemetry 使用与生产独立事件入口相同的服务状态。
        let mut telemetry =
            ProviderTelemetryHookGuard::new_with_persistence(Arc::clone(&state), None);
        // handled must complete all lifecycle projection before returning to the executor.
        // handled 必须在返回执行器前完成全部生命周期投影。
        let handled = handle_webui_detached_child_execution_event(
            &app,
            &state,
            &runtime,
            "close-execution",
            &mut telemetry,
            &mut sink,
            ExecutionStreamEvent::RuntimeEvent {
                session_id: handle.session_id(),
                event,
            },
        )
        .await;
        if !start_close_tool {
            handled.expect_err("an unstarted close cannot be durably completed");
            assert_eq!(
                *capture.states.lock().expect("captured navigation states"),
                vec![String::from("completed")]
            );
            assert!(
                state
                    .lock()
                    .expect("service state")
                    .subagent_surface_detail_cache
                    .contains_key(&child.task_id)
            );
            return;
        }
        handled.expect("real V1 event handler");
        assert_eq!(
            *capture.states.lock().expect("captured navigation states"),
            vec![String::from("completed"), String::from("closed")]
        );
        assert!(
            !state
                .lock()
                .expect("service state")
                .subagent_surface_detail_cache
                .contains_key(&child.task_id)
        );
        assert_eq!(sink.api_terminal_completion.is_none(), tool_call_count == 2);
        assert_eq!(sink.active.is_some(), tool_call_count == 2);
    }

    /// Verifies closure is pushed immediately through the real stream entry without opening the child panel.
    /// 验证关闭通过真实流入口立即推送，无需打开子面板。
    /// Parameters: none. Returns: none; fails if the navigation remains in standby.
    /// 参数：无。返回值：无；若导航仍为待命则失败。
    #[tokio::test]
    async fn v1_close_agent_event_pushes_closed_navigation_without_reopening_panel() {
        assert_v1_close_agent_navigation(1, true).await;
    }

    /// Verifies a closed child is published while another tool in the same physical API remains unfinished.
    /// 验证同一物理 API 中仍有未完成工具时，也会发布子任务关闭状态。
    /// Parameters: none. Returns: none; fails if projection waits for the remaining tool or API completion.
    /// 参数：无。返回值：无；若投影等待剩余工具或 API 完成则失败。
    #[tokio::test]
    async fn v1_close_agent_event_pushes_closed_navigation_before_peer_tool_finishes() {
        assert_v1_close_agent_navigation(2, true).await;
    }

    /// Verifies a rejected V1 completion does not publish closure or evict the child detail cache.
    /// 验证被拒绝的 V1 完成边界不会发布关闭状态或清除子任务详情缓存。
    /// Parameters: none. Returns: none; fails if lifecycle changes precede durable completion.
    /// 参数：无。返回值：无；若生命周期变化早于可靠完成则失败。
    #[tokio::test]
    async fn v1_close_agent_event_rejects_navigation_projection_when_boundary_fails() {
        assert_v1_close_agent_navigation(1, false).await;
    }

    /// Verifies api_started is durable before semantic streaming and terminal closure.
    /// 验证 api_started 在语义流与终态闭合前已经可靠落盘。
    #[tokio::test]
    async fn tool_free_request_persists_ordered_v1_lifecycle() {
        let handle = published_open_turn();
        let mut sink = ProviderV1StreamSink::new(
            Arc::clone(&handle),
            Arc::new(ProjectResourceCatalogRegistry::default()),
        );
        prepare_sink_request(&handle, &mut sink);
        let (memory_revision, durable_revision, bodies) = handle
            .read(|document| {
                (
                    document.memory_revision(),
                    document.durable_revision(),
                    document
                        .events()
                        .iter()
                        .map(|event| event.event.clone())
                        .collect::<Vec<_>>(),
                )
            })
            .expect("read api start");
        assert_eq!(memory_revision, durable_revision);
        assert!(matches!(bodies.last(), Some(EventBody::ApiStarted(_))));

        sink.append_reasoning_delta("reason", 4)
            .expect("reasoning delta");
        sink.complete_reasoning(5).expect("reasoning complete");
        sink.append_assistant_delta("answer", 6)
            .expect("assistant delta");
        assert_eq!(
            sink.accept_terminal(terminal_facts(0))
                .await
                .expect("terminal boundary"),
            ProviderV1TerminalOutcome::Completed
        );
        let bodies = handle
            .read(|document| {
                document
                    .events()
                    .iter()
                    .map(|event| event.event.clone())
                    .collect::<Vec<_>>()
            })
            .expect("read complete lifecycle");
        assert!(matches!(
            &bodies[bodies.len() - 6],
            EventBody::ApiStarted(_)
        ));
        assert!(matches!(
            &bodies[bodies.len() - 5],
            EventBody::AssistantReasoning(_)
        ));
        assert!(matches!(
            &bodies[bodies.len() - 4],
            EventBody::AssistantText(_)
        ));
        assert!(matches!(&bodies[bodies.len() - 3], EventBody::ApiStats(_)));
        assert!(matches!(
            &bodies[bodies.len() - 2],
            EventBody::TurnBoundary(_)
        ));
        assert!(matches!(bodies.last(), Some(EventBody::TurnStats(_))));
        let durability = sink.take_finished_durability();
        assert!(
            durability.api_terminal.is_some(),
            "the physical API terminal must remain available to finished handling"
        );
        let receipt = durability
            .turn_completion
            .expect("final assistant should produce one durable Turn receipt");
        assert_eq!(receipt.value.successor, TurnSuccessor::None);
        let consumed = sink.take_finished_durability();
        assert!(consumed.api_terminal.is_none());
        assert!(consumed.turn_completion.is_none());
    }

    /// Verifies a child return committed during the parent API is transferred to stream-finished dispatch instead of closing the Turn.
    /// 验证父级 API 期间提交的子代理回流会转交流终态分发，而不是关闭回合。
    #[tokio::test]
    async fn child_return_during_parent_api_defers_turn_completion_to_exact_continuation() {
        // childHandle produces the exact terminal child facts required by the parent return contract.
        // childHandle 产生父级回流契约要求的精确子会话终态事实。
        let child_handle = published_open_turn();
        let mut child_sink = ProviderV1StreamSink::new(
            Arc::clone(&child_handle),
            Arc::new(ProjectResourceCatalogRegistry::default()),
        );
        prepare_sink_request(&child_handle, &mut child_sink);
        child_sink
            .append_assistant_delta("durable child result", 5)
            .expect("append child assistant result");
        child_sink
            .accept_terminal(terminal_facts(0))
            .await
            .expect("complete child turn");
        // childDurability retains both the child final Turn and its exact assistant source seq.
        // childDurability 同时保留子会话最终回合及其精确 assistant 来源 seq。
        let child_durability = child_sink.take_finished_durability();
        let child_turn_id = child_durability
            .api_terminal
            .as_ref()
            .expect("child API terminal")
            .value
            .turn_id;
        let child_assistant_seq = child_durability
            .completed_assistant_text
            .as_ref()
            .expect("child assistant semantic")
            .seq;
        let return_input = child_handle
            .read(|document| {
                document.build_subagent_return_input(SubagentReturnBuildRequest {
                    final_turn_id: child_turn_id,
                    child_task_id: SubagentTaskId::try_new("child-return-during-parent-api")?,
                    source_assistant_text_seq: child_assistant_seq,
                })
            })
            .expect("read child document")
            .expect("build exact child return");

        // parentSink remains physically active when the child return enters the same logical Turn.
        // parentSink 在子代理回流进入同一逻辑回合时仍持有活动物理 API。
        let parent_handle = published_open_turn();
        let mut parent_sink = ProviderV1StreamSink::new(
            Arc::clone(&parent_handle),
            Arc::new(ProjectResourceCatalogRegistry::default()),
        );
        prepare_sink_request(&parent_handle, &mut parent_sink);
        parent_sink
            .append_assistant_delta("parent still owns the API", 5)
            .expect("append parent assistant text");
        let appended_return = parent_handle
            .append_subagent_return_with_stats(return_input, 12)
            .expect("append child return to active parent Turn")
            .value;
        // parentTerminal ends after the child return and releases only the physical API.
        // parentTerminal 在子代理回流后结束，且只释放物理 API。
        let mut parent_terminal = terminal_facts(0);
        parent_terminal.completed_at_ms = 20;
        parent_terminal.timing.completed_at_ms = Some(20);
        parent_sink
            .accept_terminal(parent_terminal)
            .await
            .expect("complete parent physical API");

        let durability = parent_sink.take_finished_durability();
        assert!(
            durability.turn_completion.is_none(),
            "a pending child return must keep the parent logical Turn open"
        );
        let pending_dispatch = parent_handle
            .read(|document| document.pending_subagent_return_dispatch())
            .expect("read parent document")
            .expect("validate pending child return");
        assert_eq!(pending_dispatch.as_ref(), Some(&appended_return));
    }

    /// Exercises late completion through the production listener with or without a durable child receipt.
    /// 有或没有可靠子任务回执时，通过生产监听器验证迟到完成事件。
    /// Parameters: durable_child selects successful persistence or an explicit missing-receipt failure.
    /// 参数：durable_child 选择持久化成功或显式缺失回执失败。
    /// Returns: none; verifies immediate terminal pushes, cleanup, and exact continuation input.
    /// 返回值：无；验证立即终态推送、清理与精确续跑输入。
    async fn assert_late_child_completion_lifecycle(durable_child: bool) {
        use crate::agent_stream::{
            ProviderTelemetryHookGuard, handle_webui_detached_subagent_task_event,
        };
        use crate::tests::test_support::{
            TestControllerRuntimeRoot, seed_test_live_subagent_registry_from_panel_states,
            test_service_state_with_parent_session,
        };
        use vulcan_agent_runtime::agent_runtime::events::{RuntimeEvent, RuntimeEventKind};
        use vulcan_agent_runtime::agent_runtime::task_runtime::DetachedTaskRuntimeEvent;
        use vulcan_workbench_model::{
            SessionRunPhase, SubagentTaskPanelState, SubagentTaskRunState,
        };

        // parent_handle has already durably closed the initial user turn.
        // parent_handle 已经可靠关闭最初的用户回合。
        let parent_handle = published_open_turn();
        let mut parent_sink = ProviderV1StreamSink::new(
            Arc::clone(&parent_handle),
            Arc::new(ProjectResourceCatalogRegistry::default()),
        );
        prepare_sink_request(&parent_handle, &mut parent_sink);
        parent_sink
            .append_assistant_delta("child is still working", 5)
            .unwrap();
        parent_sink
            .accept_terminal(terminal_facts(0))
            .await
            .unwrap();
        // child_sink retains the exact source receipt used by the production completion path.
        // child_sink 保留生产完成路径使用的精确来源回执。
        let child_handle = published_open_turn();
        let mut child_sink = ProviderV1StreamSink::new(
            Arc::clone(&child_handle),
            Arc::new(ProjectResourceCatalogRegistry::default()),
        );
        if durable_child {
            prepare_sink_request(&child_handle, &mut child_sink);
            child_sink
                .append_assistant_delta("LATE_CHILD_OK", 5)
                .unwrap();
            child_sink.accept_terminal(terminal_facts(0)).await.unwrap();
        }
        // child is terminal in the real registry while an existing listener still makes the projection active.
        // child 在真实注册表中已终态，但现存监听器仍会让投影显示活动。
        let child = SubagentTaskPanelState {
            task_id: child_handle.session_id(),
            parent_session_id: parent_handle.session_id(),
            public_task_id: String::from("late-child"),
            subagent_label: String::from("late-child"),
            description: None,
            run_state: SubagentTaskRunState::Completed,
            result_text: String::from("LATE_CHILD_OK"),
            conversation_state: None,
        };
        let mut service_state = test_service_state_with_parent_session(parent_handle.session_id());
        seed_test_live_subagent_registry_from_panel_states(
            &mut service_state,
            parent_handle.session_id(),
            std::slice::from_ref(&child),
        );
        service_state
            .session_runtime_registry
            .register(Arc::clone(&parent_handle))
            .unwrap();
        service_state
            .detached_subagent_task_listeners
            .insert(child.task_id, String::from("late-execution"));
        // A stopped parent must retain the result without invoking an external Provider during this test.
        // 测试中的已停止父会话必须保留结果，但不调用外部 Provider。
        service_state
            .workspace_snapshot
            .as_mut()
            .unwrap()
            .session_run_phases
            .insert(parent_handle.session_id(), SessionRunPhase::Stopped);
        let runtime = service_state.agent_runtime.clone().unwrap();
        let state = Arc::new(std::sync::Mutex::new(service_state));
        let capture = Arc::new(ToolLifecycleEventSink {
            handle: Arc::clone(&parent_handle),
            states: Default::default(),
        });
        let app = vulcan_service_api::ServiceEventEmitter::new(vec![capture.clone()]);
        crate::emit_webui_subagent_navigation_changed_delta_for_parent(
            &app,
            &state,
            parent_handle.session_id(),
        )
        .unwrap();
        // controller_root owns isolated offline storage; it never touches the user's session files.
        // controller_root 持有隔离离线存储，不接触用户会话文件。
        let controller_root = TestControllerRuntimeRoot::new(parent_handle.session_id());
        let controller = controller_root.controller_runtime_handle();
        let mut telemetry =
            ProviderTelemetryHookGuard::new_with_persistence(Arc::clone(&state), None);
        let event = RuntimeEvent::new(RuntimeEventKind::DelegateFinished, "Child finished", "Completed").with_metadata(Some(json!({
            "kind": "subagent_task", "payload": {
                "task_id": "late-child", "subagent_task_id": child.task_id.to_string(),
                "execution_id": "late-execution", "execution_status": "completed", "subagent": "late-child", "result": "LATE_CHILD_OK"
            }
        })));
        let handled = handle_webui_detached_subagent_task_event(
            &app,
            &state,
            &controller,
            &runtime,
            child.task_id,
            "late-execution",
            child_handle.as_ref(),
            &mut telemetry,
            &mut child_sink,
            DetachedTaskRuntimeEvent::ParentRuntime {
                parent_session_id: parent_handle.session_id(),
                event,
            },
        )
        .await;
        assert!(
            !state
                .lock()
                .unwrap()
                .detached_subagent_task_listeners
                .contains_key(&child.task_id)
        );
        assert_eq!(
            capture.states.lock().unwrap().last().map(String::as_str),
            Some("completed")
        );
        if durable_child {
            handled.expect("late completion must be persisted without a user prompt");
            let receipt = parent_handle
                .read(|doc| doc.pending_subagent_return_dispatch())
                .unwrap()
                .unwrap()
                .expect("pending return");
            let authority =
                crate::webui_prompt_lifecycle::subagent_return_dispatch_authority_from_v1_handle(
                    &parent_handle,
                    &receipt,
                )
                .unwrap();
            assert_eq!(authority.content, "LATE_CHILD_OK");
            prepare_sink_request(&parent_handle, &mut parent_sink);
            assert!(
                parent_handle
                    .read(|doc| doc.active_api().is_some())
                    .unwrap()
            );
        } else {
            assert!(
                handled
                    .unwrap_err()
                    .contains("no exact durable V1 final assistant receipt")
            );
        }
    }

    /// Verifies late successful completion publishes terminal navigation and retains a dispatchable return.
    /// 验证迟到成功完成会发布终态导航并保留可分发的回流。
    /// Parameters: none. Returns: none; assertions cover the production listener entry.
    /// 参数：无。返回值：无；断言覆盖生产监听器入口。
    #[tokio::test]
    async fn late_child_completion_pushes_state_without_new_user_input() {
        assert_late_child_completion_lifecycle(true).await;
    }

    /// Verifies return persistence errors cannot suppress terminal navigation or leak the listener.
    /// 验证回流持久化错误不能阻断终态导航或泄漏监听器。
    /// Parameters: none. Returns: none; asserts explicit failure and successful lifecycle cleanup.
    /// 参数：无。返回值：无；断言显式失败以及生命周期成功清理。
    #[tokio::test]
    async fn late_child_completion_error_still_pushes_state() {
        assert_late_child_completion_lifecycle(false).await;
    }

    /// Executes native Goal mutations while the production V1 sink owns a running tool boundary.
    /// 在生产 V1 sink 持有运行中工具边界时执行原生 Goal 修改。
    /// Parameters: none. Returns: none; asserts creation, completion, replacement and durable tool closure.
    /// 参数：无。返回值：无；断言创建、完成、替换以及可靠工具闭合。
    #[tokio::test]
    async fn native_goal_creation_inside_v1_tool_api_succeeds() {
        use vulcan_agent_runtime::tooling::native::{NativeGoalStatus, NativeGoalTerminalStatus};
        // handle owns the same active physical API observed by the native Goal runtime.
        // handle 持有原生 Goal 运行时观察到的同一个活动物理 API。
        let handle = published_open_turn();
        let mut sink = ProviderV1StreamSink::new(
            Arc::clone(&handle),
            Arc::new(ProjectResourceCatalogRegistry::default()),
        );
        prepare_sink_request(&handle, &mut sink);
        sink.accept_terminal(terminal_facts(1)).await.unwrap();
        let request = recovery_request(
            "goal-create",
            "create_goal",
            json!({"objective": "test goal"}),
        );
        sink.apply_tool_boundary(
            &ToolExecutionRecoveryBoundary::BatchPrepared(vec![request.clone()]),
            9,
        )
        .await
        .unwrap();
        sink.apply_tool_boundary(
            &ToolExecutionRecoveryBoundary::Started {
                request: request.request,
                binding_fingerprint: request.binding_fingerprint,
            },
            10,
        )
        .await
        .unwrap();
        let service_state =
            crate::tests::test_support::test_service_state_with_parent_session(handle.session_id());
        service_state
            .session_runtime_registry
            .register(Arc::clone(&handle))
            .unwrap();
        let state = Arc::new(std::sync::Mutex::new(service_state));
        let runtime = crate::session_goals::session_v1_native_goal_runtime(
            state,
            vulcan_service_api::ServiceEventEmitter::new(Vec::new()),
        );
        let goal = runtime
            .create_goal(handle.session_id(), String::from("test goal"), Some(2000))
            .await
            .expect("create_goal inside its running V1 tool");
        assert_eq!(goal.status, NativeGoalStatus::Active);
        runtime
            .update_goal_status(handle.session_id(), NativeGoalTerminalStatus::Complete)
            .await
            .unwrap();
        let replacement = runtime
            .create_goal(handle.session_id(), String::from("replacement goal"), None)
            .await
            .expect("replace completed goal inside tool API");
        assert_ne!(goal.goal_id, replacement.goal_id);
        // Tool completion must still target the original active API after Goal metadata changed.
        // Goal 元数据变化后，工具完成仍必须指向原有活动 API。
        sink.apply_tool_boundary(
            &ToolExecutionRecoveryBoundary::Finished(ToolInvocationResult {
                tool_call_id: String::from("goal-create"),
                tool_name: String::from("create_goal"),
                model_output: String::from("{\"ok\":true}"),
                host_result: None,
                is_error: false,
            }),
            12,
        )
        .await
        .unwrap();
        handle
            .read(|document| {
                assert!(document.active_api().is_none());
                assert!(document.active_turn().is_some_and(|turn| !turn.closed));
                assert_eq!(document.memory_revision(), document.durable_revision());
            })
            .unwrap();
    }

    /// Verifies an in-turn supplement closes one physical API without closing the logical Turn.
    /// 验证回合内补充会闭合一次物理 API，但不会闭合逻辑回合。
    #[tokio::test]
    async fn api_terminal_atomically_promotes_in_turn_input_without_turn_completion() {
        let handle = published_open_turn();
        let mut sink = ProviderV1StreamSink::new(
            Arc::clone(&handle),
            Arc::new(ProjectResourceCatalogRegistry::default()),
        );
        prepare_sink_request(&handle, &mut sink);
        sink.append_assistant_delta("partial answer", 4)
            .expect("assistant delta before supplement");
        handle
            .mutate_durable_if_changed(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::InputAccepted,
                |document| {
                    let outcome = document.add_pending_in_turn_input(
                        ClientInputSubmission::new(
                            ClientActionId::try_new("provider-v1-in-turn-supplement")?,
                            "supplement",
                            Vec::new(),
                        ),
                        5,
                    )?;
                    Ok(match outcome {
                        IdempotentMutation::Committed(input_id) => {
                            SessionMutationChange::Changed(input_id)
                        }
                        IdempotentMutation::Idempotent(input_id) => {
                            SessionMutationChange::Unchanged(input_id)
                        }
                    })
                },
            )
            .expect("persist pending in-turn supplement");
        assert_eq!(
            sink.accept_terminal(terminal_facts(0))
                .await
                .expect("terminal boundary with in-turn supplement"),
            ProviderV1TerminalOutcome::Completed
        );

        let durability = sink.take_finished_durability();
        let api_terminal = durability
            .api_terminal
            .expect("physical API terminal receipt");
        assert_eq!(api_terminal.value.promoted_input_seqs.len(), 1);
        assert!(
            durability.turn_completion.is_none(),
            "promoted in-turn work must keep the logical Turn open"
        );
        let authority =
            crate::webui_prompt_lifecycle::pending_in_turn_dispatch_authority_from_v1_handle(
                handle.as_ref(),
                &api_terminal.value,
            )
            .expect("rebuild exact same-Turn dispatch authority");
        assert_eq!(authority.turn_id, api_terminal.value.turn_id);
        assert_eq!(authority.inputs.len(), 1);
        assert_eq!(authority.inputs[0].text, "supplement");
        let (has_open_turn, has_turn_boundary, has_turn_stats) = handle
            .read(|document| {
                (
                    document.active_turn().is_some_and(|turn| !turn.closed),
                    document
                        .events()
                        .iter()
                        .any(|event| matches!(event.event, EventBody::TurnBoundary(_))),
                    document
                        .events()
                        .iter()
                        .any(|event| matches!(event.event, EventBody::TurnStats(_))),
                )
            })
            .expect("read logical Turn after physical terminal");
        assert!(has_open_turn);
        assert!(!has_turn_boundary);
        assert!(!has_turn_stats);
    }

    /// Verifies one Provider-ordered batch keeps each tool seq across all lifecycle updates.
    /// 验证一个 Provider 有序批次在全部生命周期更新中保持每条工具 seq 不变。
    #[tokio::test]
    async fn tool_batch_persists_prepared_running_finished_and_unique_stats() {
        let handle = published_open_turn();
        let mut sink = ProviderV1StreamSink::new(
            Arc::clone(&handle),
            Arc::new(ProjectResourceCatalogRegistry::default()),
        );
        prepare_sink_request(&handle, &mut sink);
        assert_eq!(
            sink.accept_terminal(terminal_facts(2))
                .await
                .expect("defer terminal"),
            ProviderV1TerminalOutcome::AwaitingTools
        );
        let first = recovery_request("tool-1", "shell_command", json!({"command": "echo one"}));
        let second = recovery_request("tool-2", "file_edit", json!({"path": "two.txt"}));
        sink.apply_tool_boundary(
            &ToolExecutionRecoveryBoundary::BatchPrepared(vec![first.clone(), second.clone()]),
            9,
        )
        .await
        .expect("durable Prepared batch");
        let prepared = handle
            .read(|document| {
                document
                    .events()
                    .iter()
                    .filter_map(|event| match &event.event {
                        EventBody::ToolCall(tool) => Some((event.seq, tool.clone())),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            })
            .expect("read Prepared batch");
        assert_eq!(prepared.len(), 2);
        assert!(prepared.iter().all(|(_, tool)| {
            tool.status == ToolCallStatus::Prepared
                && tool.started_at_ms.is_none()
                && tool.finished_at_ms.is_none()
        }));
        assert!(prepared[0].1.provider_source.is_none());
        assert!(prepared[1].1.provider_source.is_some());
        let (memory_revision, durable_revision, stats_count) = handle
            .read(|document| {
                (
                    document.memory_revision(),
                    document.durable_revision(),
                    document
                        .events()
                        .iter()
                        .filter(|event| matches!(&event.event, EventBody::ApiStats(_)))
                        .count(),
                )
            })
            .expect("read durable Prepared source boundary");
        assert_eq!(memory_revision, durable_revision);
        assert_eq!(stats_count, 0);
        let duplicate_batch = sink
            .apply_tool_boundary(
                &ToolExecutionRecoveryBoundary::BatchPrepared(vec![first.clone(), second.clone()]),
                9,
            )
            .await
            .expect("an exact repeated Prepared batch must be idempotent");
        assert_eq!(
            duplicate_batch,
            ProviderV1ToolBoundaryOutcome::BatchPrepared(
                prepared
                    .iter()
                    .map(|(seq, tool)| PreparedToolInvocationIdentity {
                        api_call_id: tool.api_call_id,
                        tool_call_id: tool.tool_call_id,
                        request_stream_id: tool.request_stream_id.clone(),
                        tool_seq: *seq,
                    })
                    .collect(),
            )
        );

        for request in [&first, &second] {
            let started = ToolExecutionRecoveryBoundary::Started {
                request: request.request.clone(),
                binding_fingerprint: request.binding_fingerprint.clone(),
            };
            sink.apply_tool_boundary(&started, 10)
                .await
                .expect("durable Running transition");
            sink.apply_tool_boundary(&started, 10)
                .await
                .expect("exact repeated Started boundary must be idempotent");
        }
        let running = handle
            .read(|document| {
                document
                    .events()
                    .iter()
                    .filter_map(|event| match &event.event {
                        EventBody::ToolCall(tool) => Some((event.seq, tool.clone())),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            })
            .expect("read Running tools");
        assert_eq!(
            prepared.iter().map(|(seq, _)| *seq).collect::<Vec<_>>(),
            running.iter().map(|(seq, _)| *seq).collect::<Vec<_>>()
        );
        assert!(
            running
                .iter()
                .all(|(_, tool)| tool.status == ToolCallStatus::Running)
        );

        sink.apply_tool_boundary(
            &ToolExecutionRecoveryBoundary::Finished(ToolInvocationResult {
                tool_call_id: String::from("tool-1"),
                tool_name: String::from("shell_command"),
                model_output: String::from("one"),
                host_result: Some(json!({"kind": "command_result", "exit_code": 0})),
                is_error: false,
            }),
            11,
        )
        .await
        .expect("finish first tool");
        assert!(
            handle
                .read(|document| document
                    .events()
                    .iter()
                    .all(|event| !matches!(&event.event, EventBody::ApiStats(_))))
                .expect("read deferred stats")
        );

        sink.apply_tool_boundary(
            &ToolExecutionRecoveryBoundary::Finished(ToolInvocationResult {
                tool_call_id: String::from("tool-2"),
                tool_name: String::from("file_edit"),
                model_output: String::from("two"),
                host_result: Some(json!({"kind": "plain_result", "ok": true})),
                is_error: false,
            }),
            12,
        )
        .await
        .expect("finish final tool and API");
        let (finished, stats_count) = handle
            .read(|document| {
                let tools = document
                    .events()
                    .iter()
                    .filter_map(|event| match &event.event {
                        EventBody::ToolCall(tool) => Some((event.seq, tool.clone())),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let stats_count = document
                    .events()
                    .iter()
                    .filter(|event| matches!(&event.event, EventBody::ApiStats(_)))
                    .count();
                (tools, stats_count)
            })
            .expect("read terminal tools");
        assert_eq!(stats_count, 1);
        assert_eq!(
            prepared.iter().map(|(seq, _)| *seq).collect::<Vec<_>>(),
            finished.iter().map(|(seq, _)| *seq).collect::<Vec<_>>()
        );
        assert!(
            finished
                .iter()
                .all(|(_, tool)| tool.status == ToolCallStatus::Completed)
        );
        assert!(finished[0].1.provider_source.is_none());
        assert!(finished[1].1.provider_source.is_some());
        let duplicate_finish_error = sink
            .apply_tool_boundary(
                &ToolExecutionRecoveryBoundary::Finished(ToolInvocationResult {
                    tool_call_id: String::from("tool-2"),
                    tool_name: String::from("file_edit"),
                    model_output: String::from("two"),
                    host_result: Some(json!({"kind": "plain_result", "ok": true})),
                    is_error: false,
                }),
                13,
            )
            .await
            .expect_err("a duplicate final result must fail");
        assert!(matches!(
            duplicate_finish_error,
            ProviderV1SinkError::RequestNotActive
        ));
        assert_eq!(
            handle
                .read(|document| document
                    .events()
                    .iter()
                    .filter(|event| matches!(&event.event, EventBody::ApiStats(_)))
                    .count())
                .expect("read unique terminal statistic"),
            1
        );
        let durability = sink.take_finished_durability();
        assert!(durability.api_terminal.is_some());
        assert!(
            durability.turn_completion.is_none(),
            "a tool-bearing API terminal must not close its logical Turn"
        );
    }

    /// Generated provider attachments are project-registered before their ToolCall terminal is durable.
    /// Provider 生成附件会先注册到项目目录，再可靠提交其 ToolCall 终态。
    #[tokio::test]
    async fn generated_attachment_registration_precedes_tool_terminal() {
        let temporary = tempfile::tempdir().expect("temporary generated-resource root");
        let project_directory = temporary.path().join("project");
        let managed_path = temporary.path().join("generated.png");
        let content = b"generated-image";
        std::fs::write(&managed_path, content).expect("write managed generated attachment");
        let digest = hex::encode(Sha256::digest(content));
        let handle = published_open_turn();
        let mut sink = ProviderV1StreamSink::new(
            Arc::clone(&handle),
            Arc::new(ProjectResourceCatalogRegistry::default()),
        );
        prepare_sink_request(&handle, &mut sink);
        sink.active
            .as_mut()
            .expect("prepared active request")
            .project_directory = project_directory.clone();
        assert_eq!(
            sink.accept_terminal(terminal_facts(1))
                .await
                .expect("defer terminal for generated tool"),
            ProviderV1TerminalOutcome::AwaitingTools
        );
        let request = recovery_request("generated-tool-1", "image_generate", json!({}));
        sink.apply_tool_boundary(
            &ToolExecutionRecoveryBoundary::BatchPrepared(vec![request.clone()]),
            9,
        )
        .await
        .expect("prepare generated tool");
        sink.apply_tool_boundary(
            &ToolExecutionRecoveryBoundary::Started {
                request: request.request.clone(),
                binding_fingerprint: request.binding_fingerprint.clone(),
            },
            10,
        )
        .await
        .expect("start generated tool");

        sink.apply_tool_boundary(
            &ToolExecutionRecoveryBoundary::Finished(ToolInvocationResult {
                tool_call_id: String::from("generated-tool-1"),
                tool_name: String::from("image_generate"),
                model_output: json!({"resources": [{"file_name": "generated.png"}]}).to_string(),
                host_result: Some(json!({
                    "kind": "generated_attachments",
                    "payload": {
                        "outputs": [{
                            "kind": "image",
                            "file_name": "generated.png",
                            "local_path": managed_path.to_string_lossy(),
                            "asset_id": "generated-asset-1",
                            "byte_len": content.len(),
                            "source_sha256": digest,
                            "mime_type": "image/png"
                        }],
                        "upstream_response_id": "provider-response-1"
                    }
                })),
                is_error: false,
            }),
            11,
        )
        .await
        .expect("finish generated tool after project registration");

        let generated_resources = handle
            .read(|document| {
                document
                    .events()
                    .iter()
                    .find_map(|event| match &event.event {
                        EventBody::ToolCall(tool) if tool.tool_name == "image_generate" => {
                            Some(tool.generated_resources.clone())
                        }
                        _ => None,
                    })
            })
            .expect("read generated ToolCall")
            .expect("generated ToolCall event");
        let catalog = load_project_resource_catalog(&project_directory)
            .expect("load generated project resource catalog");

        assert_eq!(generated_resources.len(), 1);
        assert_eq!(generated_resources[0].kind, GeneratedResourceKind::Image);
        assert_eq!(generated_resources[0].resource_id.value(), 1);
        assert!(generated_resources[0].preview_resource_id.is_none());
        assert!(matches!(
            resolve_project_resource(
                &project_directory,
                &catalog,
                generated_resources[0].resource_id
            ),
            Ok(ProjectResourceResolution::Available { bytes, .. }) if bytes == content
        ));
    }

    /// Verifies a durably cancelled Ask is only confirmed by the later generic ToolFinished boundary.
    /// 验证已可靠取消的 Ask 只会由后续通用 ToolFinished 边界进行幂等确认。
    #[tokio::test]
    async fn cancelled_ask_finish_confirms_the_existing_terminal_tool_fact() {
        let handle = published_open_turn();
        let mut sink = ProviderV1StreamSink::new(
            Arc::clone(&handle),
            Arc::new(ProjectResourceCatalogRegistry::default()),
        );
        prepare_sink_request(&handle, &mut sink);
        assert_eq!(
            sink.accept_terminal(terminal_facts(1))
                .await
                .expect("defer terminal"),
            ProviderV1TerminalOutcome::AwaitingTools
        );
        let request = recovery_request("ask-tool", "ask", json!({"questions": []}));
        sink.apply_tool_boundary(
            &ToolExecutionRecoveryBoundary::BatchPrepared(vec![request.clone()]),
            9,
        )
        .await
        .expect("durably prepare Ask");
        sink.apply_tool_boundary(
            &ToolExecutionRecoveryBoundary::Started {
                request: request.request.clone(),
                binding_fingerprint: request.binding_fingerprint.clone(),
            },
            10,
        )
        .await
        .expect("durably start Ask");
        let (api_call_id, tool_call_id) = handle
            .read(|document| {
                document
                    .events()
                    .iter()
                    .find_map(|event| match &event.event {
                        EventBody::ToolCall(tool) => Some((tool.api_call_id, tool.tool_call_id)),
                        _ => None,
                    })
            })
            .expect("read Ask identity")
            .expect("Ask tool event");
        handle
            .mark_tool_waiting_user(api_call_id, tool_call_id, 11)
            .expect("durably mark Ask waiting");
        let result = build_tool_failure_invocation_result(
            &request.request,
            "The user rejected the structured question request.",
        );
        let output = persistent_tool_output_from_result(&result).expect("canonical Ask output");
        handle
            .cancel_waiting_tool(api_call_id, tool_call_id, Some(output), 12)
            .expect("durably cancel Ask");

        sink.apply_tool_boundary(&ToolExecutionRecoveryBoundary::Finished(result), 13)
            .await
            .expect("generic ToolFinished must confirm the durable cancellation");
        let (status, stats_count) = handle
            .read(|document| {
                let status = document
                    .tool_status(api_call_id, tool_call_id)
                    .expect("read durable Ask status");
                let stats_count = document
                    .events()
                    .iter()
                    .filter(|event| matches!(&event.event, EventBody::ApiStats(_)))
                    .count();
                (status, stats_count)
            })
            .expect("read finished Ask facts");
        assert_eq!(status, ToolCallStatus::Cancelled);
        assert_eq!(stats_count, 1);
    }

    /// A stale terminal identity remains a navigation-only durable reference after reload.
    /// 一个已失效的终端身份在重载后仍保持为仅导航用途的可靠引用。
    #[tokio::test]
    async fn terminal_external_reference_round_trips_without_resolution() {
        let handle = published_open_turn();
        let mut sink = ProviderV1StreamSink::new(
            Arc::clone(&handle),
            Arc::new(ProjectResourceCatalogRegistry::default()),
        );
        prepare_sink_request(&handle, &mut sink);
        sink.accept_terminal(terminal_facts(1))
            .await
            .expect("defer terminal");
        let request = recovery_request(
            "tool-terminal-1",
            "terminal_session",
            json!({"action": "read", "terminal_session_id": "missing-terminal"}),
        );
        sink.apply_tool_boundary(
            &ToolExecutionRecoveryBoundary::BatchPrepared(vec![request.clone()]),
            9,
        )
        .await
        .expect("durable terminal Prepared event");
        sink.apply_tool_boundary(
            &ToolExecutionRecoveryBoundary::Started {
                request: request.request.clone(),
                binding_fingerprint: request.binding_fingerprint.clone(),
            },
            10,
        )
        .await
        .expect("durable terminal Running event");
        sink.apply_tool_boundary(
            &ToolExecutionRecoveryBoundary::Finished(ToolInvocationResult {
                tool_call_id: String::from("tool-terminal-1"),
                tool_name: String::from("terminal_session"),
                model_output: String::from("terminal output"),
                host_result: Some(json!({
                    "kind": "terminal_session",
                    "payload": {
                        "action": "read",
                        "terminal_session_id": "missing-terminal"
                    }
                })),
                is_error: false,
            }),
            11,
        )
        .await
        .expect("finish terminal tool and API");

        let tool_event = handle
            .read(|document| {
                document.events().iter().find_map(|event| {
                    matches!(&event.event, EventBody::ToolCall(_)).then(|| event.clone())
                })
            })
            .expect("read terminal event")
            .expect("terminal event exists");
        let serialized = serde_json::to_string(&tool_event).expect("serialize terminal event");
        let restored: ConversationEvent =
            serde_json::from_str(&serialized).expect("reload terminal event");
        let EventBody::ToolCall(restored_tool) = restored.event else {
            panic!("restored event must remain a tool call");
        };
        assert_eq!(restored_tool.external_refs.len(), 1);
        assert_eq!(
            restored_tool.external_refs[0].kind,
            ExternalResourceKind::Terminal
        );
        assert_eq!(restored_tool.external_refs[0].id, "missing-terminal");
    }

    /// Verifies Started cannot advance a tool that lacks a durable Prepared predecessor.
    /// 验证 Started 不能推进缺少可靠 Prepared 前驱的工具。
    #[tokio::test]
    async fn tool_started_without_prepared_predecessor_is_rejected() {
        let handle = published_open_turn();
        let mut sink = ProviderV1StreamSink::new(
            Arc::clone(&handle),
            Arc::new(ProjectResourceCatalogRegistry::default()),
        );
        prepare_sink_request(&handle, &mut sink);
        sink.accept_terminal(terminal_facts(1))
            .await
            .expect("defer terminal");
        let request = recovery_request("tool-1", "shell_command", json!({"command": "echo"}));
        let error = sink
            .apply_tool_boundary(
                &ToolExecutionRecoveryBoundary::Started {
                    request: request.request,
                    binding_fingerprint: request.binding_fingerprint,
                },
                9,
            )
            .await
            .expect_err("Started without Prepared must fail");
        assert!(matches!(error, ProviderV1SinkError::ToolNotPrepared(_)));
    }

    /// Verifies malformed declared ChangeSet data is an explicit ToolFinished boundary error.
    /// 验证畸形已声明 ChangeSet 数据会成为显式 ToolFinished 边界错误。
    #[test]
    fn malformed_tool_finished_changeset_fails_without_fallback() {
        let result = ToolInvocationResult {
            tool_call_id: String::from("tool-1"),
            tool_name: String::from("file_edit"),
            model_output: String::from("done"),
            host_result: Some(json!({
                "kind": "change_set",
                "payload": {
                    "mode": "applied",
                    "files": [{"change": "modify", "path": "bad.txt", "hunks": "invalid"}]
                }
            })),
            is_error: false,
        };
        let error = persistent_change_set_from_tool_result(&result)
            .expect_err("declared malformed change-set must fail");
        assert!(matches!(error, ProviderV1SinkError::ChangeSet(_)));
    }

    /// Verifies an unrelated host result is not interpreted as a ChangeSet.
    /// 验证无关宿主结果不会被解释为 ChangeSet。
    #[test]
    fn non_changeset_tool_finished_returns_none() {
        let result = ToolInvocationResult {
            tool_call_id: String::from("tool-1"),
            tool_name: String::from("shell_command"),
            model_output: String::from("done"),
            host_result: Some(json!({"kind": "command_result", "exit_code": 0})),
            is_error: false,
        };
        assert_eq!(
            persistent_change_set_from_tool_result(&result).expect("non change-set should pass"),
            None
        );
    }
}
