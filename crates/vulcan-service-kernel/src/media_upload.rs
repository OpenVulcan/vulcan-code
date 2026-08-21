//! Pre-dispatch preparation plus post-dispatch media-understanding upload progress.
//! 分派前媒体准备与分派后媒体理解上传进度。

use super::*;
use crate::conversation_submission::SubmissionModelSelection;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};
use vulcan_provider_adapters::{
    CONVERSATION_MEDIA_INPUT_OPERATION_ID, MediaUploadCancellation, MediaUploadProgressCallback,
    ProviderOperationalProfileRevision, ProviderOperationalTelemetryEvent,
    ProviderOperationalTelemetryEventName, record_provider_operational_telemetry,
};
use vulcan_service_api::{
    MediaUploadPhase, MediaUploadProgressChangedDelta, ServiceEvent, WorkbenchDelta,
    WorkbenchDeltaEnvelope,
};
use vulcan_workbench_model::{SessionMediaReferenceEntry, SessionMediaReferenceScope};

/// Process-local pending media submissions keyed by the client submission id.
/// 按客户端提交标识索引的进程内待处理媒体提交。
static PENDING_MEDIA_UPLOADS: OnceLock<StdMutex<BTreeMap<String, MediaUploadCancellation>>> =
    OnceLock::new();

/// Immutable non-secret route identity copied into every progress and telemetry event for one file.
/// 为单个文件复制到每条进度与遥测事件的不可变非秘密路由身份。
#[derive(Clone)]
struct MediaUploadTelemetryIdentity {
    /// Exact provider identifier.
    /// 精确供应商标识。
    provider_id: String,
    /// Exact service-entry identifier.
    /// 精确服务入口标识。
    service_entry_id: String,
    /// Exact credential connection identifier.
    /// 精确凭据连接标识。
    connection_id: String,
    /// Exact upstream model identifier.
    /// 精确上游模型标识。
    model_id: String,
    /// Exact provider-neutral protocol profile.
    /// 精确供应商无关协议 Profile。
    protocol_profile_id: String,
    /// Frozen manifest revision.
    /// 冻结 Manifest 修订号。
    manifest_revision: u64,
    /// Frozen runtime manifest generation.
    /// 冻结运行时 Manifest 代际。
    manifest_generation: u64,
    /// Content-addressed complete route revision.
    /// 内容寻址的完整路由修订号。
    route_revision: String,
    /// Exact manifest or host-inline upload channel.
    /// 精确 Manifest 或宿主内联上传通道。
    upload_channel_id: String,
    /// Frozen request projection profile identities.
    /// 冻结请求投影 Profile 身份。
    request_profiles: Vec<ProviderOperationalProfileRevision>,
    /// Frozen response projection profile identities.
    /// 冻结响应投影 Profile 身份。
    response_profiles: Vec<ProviderOperationalProfileRevision>,
}

/// Carries provider-native video references prepared for one accepted prompt.
/// 携带一条已接纳提示词所准备的供应商原生视频引用。
#[derive(Debug, Clone, Default)]
pub(crate) struct PreparedSubmissionMedia {
    /// Provider-native references that must be attached to the canonical user-message owner.
    /// 必须附加到规范用户消息所有者上的供应商原生引用。
    pub(crate) references: Vec<PreparedSubmissionMediaReference>,
}

/// Carries one durable media-ledger candidate without exposing provider secrets.
/// 携带一个不暴露供应商秘密的可持久化媒体账本候选项。
#[derive(Debug, Clone)]
pub(crate) struct PreparedSubmissionMediaReference {
    /// SHA-256 digest of the exact source bytes.
    /// 精确源字节的 SHA-256 摘要。
    pub(crate) source_sha256: String,
    /// Byte length covered by the source digest.
    /// 源摘要覆盖的字节长度。
    pub(crate) source_byte_len: u64,
    /// Provider identity owning the uploaded object.
    /// 拥有已上传对象的供应商标识。
    pub(crate) provider_id: String,
    /// Service/API entry owning the upload channel.
    /// 拥有上传通道的服务/API 入口。
    pub(crate) service_entry_id: String,
    /// Manifest upload channel that created the object.
    /// 创建该对象的 Manifest 上传通道。
    pub(crate) upload_channel_id: String,
    /// Content fingerprint of the upload contract semantics.
    /// 上传契约语义的内容指纹。
    pub(crate) upload_contract_fingerprint: String,
    /// Explicit reuse boundary declared by the upload channel.
    /// 上传通道声明的显式复用边界。
    pub(crate) reference_scope: SessionMediaReferenceScope,
    /// Credential identity required only by credential-scoped references.
    /// 仅凭据作用域引用需要的凭据身份。
    pub(crate) connection_id: Option<String>,
    /// Session identity required only by session-scoped references.
    /// 仅会话作用域引用需要的会话身份。
    pub(crate) session_id: Option<String>,
    /// Canonical resource kind retained by the ledger.
    /// 账本保留的规范资源类型。
    pub(crate) resource_kind: String,
    /// Exact normalized source MIME type.
    /// 精确归一化源 MIME 类型。
    pub(crate) mime_type: String,
    /// Provider-native URI or URL used by request projection.
    /// 请求投影使用的供应商原生 URI 或 URL。
    pub(crate) provider_reference: String,
    /// Provider reference creation time in Unix milliseconds.
    /// 供应商引用创建时间的 Unix 毫秒值。
    pub(crate) created_at_unix_millis: u64,
    /// Provider reference expiry time in Unix milliseconds.
    /// 供应商引用过期时间的 Unix 毫秒值。
    pub(crate) expires_at_unix_millis: u64,
    /// Historical canonical user-message owners that still retain this source.
    /// 仍保留该源文件的历史规范用户消息所有者。
    pub(crate) owner_message_ids: BTreeSet<String>,
    /// Whether the current submission owner should be attached after turn admission.
    /// 是否应在本轮回合准入后附加当前提交所有者。
    pub(crate) attach_current_owner: bool,
}

impl PreparedSubmissionMediaReference {
    /// Converts the preparation result into a durable session-ledger entry.
    /// 将准备结果转换为可持久化的会话账本条目。
    pub(crate) fn into_ledger_entry(
        self,
        owner_message_id: Option<&str>,
    ) -> SessionMediaReferenceEntry {
        let owner_message_ids = self.owner_message_ids;
        let attach_current_owner = self.attach_current_owner;
        let mut entry = SessionMediaReferenceEntry {
            source_sha256: self.source_sha256,
            source_byte_len: self.source_byte_len,
            provider_id: self.provider_id,
            service_entry_id: self.service_entry_id,
            upload_channel_id: self.upload_channel_id,
            upload_contract_fingerprint: self.upload_contract_fingerprint,
            reference_scope: self.reference_scope,
            connection_id: self.connection_id,
            session_id: self.session_id,
            resource_kind: self.resource_kind,
            mime_type: self.mime_type,
            provider_reference: self.provider_reference,
            created_at_unix_millis: self.created_at_unix_millis,
            expires_at_unix_millis: self.expires_at_unix_millis,
            owner_message_ids,
        };
        if attach_current_owner {
            if let Some(owner_message_id) = owner_message_id {
                entry.owner_message_ids.insert(owner_message_id.to_string());
            }
        }
        entry
    }
}

impl PreparedSubmissionMedia {
    /// Retains a durable reference only for video, whose provider-native upload must survive model switches.
    /// 仅为视频保留可持久化引用，因为视频供应商原生上传必须跨模型切换存活。
    fn push_video_reference(
        &mut self,
        resource_kind: vulcan_ai_contract::ResourceKind,
        reference: PreparedSubmissionMediaReference,
    ) {
        if resource_kind == vulcan_ai_contract::ResourceKind::Video {
            self.references.push(reference);
        }
    }
}

/// Returns the process-local pending media cancellation registry.
/// 返回进程内待处理媒体取消注册表。
fn pending_media_uploads() -> &'static StdMutex<BTreeMap<String, MediaUploadCancellation>> {
    PENDING_MEDIA_UPLOADS.get_or_init(|| StdMutex::new(BTreeMap::new()))
}

/// Registers one submission before any workspace or transcript mutation occurs.
/// 在任何工作区或转录变更前注册一条提交。
pub(crate) fn register_media_upload(
    client_submission_id: &str,
) -> Result<MediaUploadCancellation, String> {
    if client_submission_id.trim().is_empty() {
        return Err(String::from("client submission id must not be empty"));
    }
    let cancellation = MediaUploadCancellation::new();
    let mut pending = pending_media_uploads()
        .lock()
        .map_err(|error| format!("failed to lock pending media uploads: {error}"))?;
    if pending.contains_key(client_submission_id) {
        return Err(format!(
            "client submission `{client_submission_id}` is already preparing media"
        ));
    }
    pending.insert(client_submission_id.to_string(), cancellation.clone());
    Ok(cancellation)
}

/// Cancels one pending submission without touching persisted conversation state.
/// 取消一条待处理提交，但不触碰持久化会话状态。
pub(crate) fn cancel_media_upload(client_submission_id: &str) -> Result<bool, String> {
    let cancellation = pending_media_uploads()
        .lock()
        .map_err(|error| format!("failed to lock pending media uploads: {error}"))?
        .get(client_submission_id)
        .cloned();
    if let Some(cancellation) = cancellation {
        cancellation.cancel();
        return Ok(true);
    }
    Ok(false)
}

/// Removes the exact submission registration after upload preparation reaches a terminal state.
/// 媒体准备到达终态后移除精确提交注册。
fn remove_media_upload(client_submission_id: &str) {
    if let Ok(mut pending) = pending_media_uploads().lock() {
        pending.remove(client_submission_id);
    }
}

/// Builds one complete progress payload from immutable route identity and current byte facts.
/// 从不可变路由身份与当前字节事实构建一条完整进度载荷。
///
/// # Parameters
/// 参数
/// `identity` is the frozen route; the remaining arguments are exact submission and file state.
/// `identity` 是冻结路由；其余参数是精确提交与文件状态。
///
/// # Returns
/// 返回值
/// One browser-safe progress delta with the same trace identity as operational telemetry.
/// 返回一条浏览器安全进度增量，其追踪身份与运行遥测完全一致。
#[allow(clippy::too_many_arguments)]
fn media_upload_progress_delta(
    identity: &MediaUploadTelemetryIdentity,
    client_submission_id: &str,
    conversation_id: Option<&str>,
    file_name: &str,
    media_kind: &str,
    file_index: usize,
    file_count: usize,
    phase: MediaUploadPhase,
    bytes_sent: u64,
    total_bytes: Option<u64>,
    error: Option<String>,
) -> MediaUploadProgressChangedDelta {
    MediaUploadProgressChangedDelta {
        client_submission_id: client_submission_id.to_string(),
        conversation_id: conversation_id.map(str::to_string),
        provider_id: identity.provider_id.clone(),
        service_entry_id: identity.service_entry_id.clone(),
        connection_id: identity.connection_id.clone(),
        model_id: identity.model_id.clone(),
        protocol_profile_id: identity.protocol_profile_id.clone(),
        manifest_revision: identity.manifest_revision,
        manifest_generation: identity.manifest_generation,
        route_revision: identity.route_revision.clone(),
        operation_id: CONVERSATION_MEDIA_INPUT_OPERATION_ID.to_string(),
        upload_channel_id: identity.upload_channel_id.clone(),
        file_name: file_name.to_string(),
        media_kind: media_kind.to_string(),
        file_index,
        file_count,
        phase,
        bytes_sent,
        total_bytes,
        error,
        cancellable: true,
    }
}

/// Installs the service bridge for media uploads started by post-dispatch understanding tools.
/// 为分派后理解工具启动的媒体上传安装服务桥接。
///
/// # Parameters
/// 参数
/// `runtime` owns the process-local observer slot; `app` and `state` emit ordered workbench deltas.
/// `runtime` 持有进程内观察器槽位；`app` 与 `state` 发出有序工作台增量。
///
/// # Returns
/// 返回值
/// Success after installing the observer, or a controlled runtime observer error.
/// 安装观察器后返回成功，或返回受控运行时观察器错误。
pub(crate) fn install_runtime_media_upload_progress_bridge(
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
) -> Result<(), String> {
    runtime.set_runtime_media_upload_progress_observer(Some(Arc::new(move |progress| {
        // phase maps the provider-neutral runtime lifecycle into the public service contract.
        // phase 将供应商中立运行时生命周期映射到公开服务契约。
        let phase = match progress.phase {
            vulcan_provider_adapters::RuntimeMediaUploadPhase::Preparing => {
                MediaUploadPhase::Preparing
            }
            vulcan_provider_adapters::RuntimeMediaUploadPhase::Uploading => {
                MediaUploadPhase::Uploading
            }
            vulcan_provider_adapters::RuntimeMediaUploadPhase::Completed => {
                MediaUploadPhase::Completed
            }
            vulcan_provider_adapters::RuntimeMediaUploadPhase::Failed => MediaUploadPhase::Failed,
            vulcan_provider_adapters::RuntimeMediaUploadPhase::Cancelled => {
                MediaUploadPhase::Cancelled
            }
        };
        emit_media_upload_progress(
            &app,
            &state,
            MediaUploadProgressChangedDelta {
                client_submission_id: progress.transfer_id,
                conversation_id: Some(progress.owner_session_id),
                provider_id: progress.provider_id,
                service_entry_id: progress.service_entry_id,
                connection_id: progress.connection_id,
                model_id: progress.model_id,
                protocol_profile_id: progress.protocol_profile_id,
                manifest_revision: progress.manifest_revision,
                manifest_generation: progress.manifest_generation,
                route_revision: progress.route_revision,
                operation_id: progress.operation_id,
                upload_channel_id: progress.upload_channel_id,
                file_name: progress.file_name,
                media_kind: progress.media_kind,
                file_index: 0,
                file_count: 1,
                phase,
                bytes_sent: progress.bytes_sent,
                total_bytes: progress.total_bytes,
                error: progress.error,
                cancellable: false,
            },
        );
    })))
}

/// Emits one low-frequency upload lifecycle event without file names, paths, URLs, or bodies.
/// 发出一条不含文件名、路径、URL 或正文的低频上传生命周期事件。
///
/// # Parameters
/// 参数
/// `identity` is the exact frozen route; `event_name` is a closed lifecycle point and
/// `failure_kind` is a stable host category for terminal failures.
/// `identity` 是精确冻结路由；`event_name` 是封闭生命周期点，`failure_kind` 是终态失败的
/// 稳定宿主分类。
///
/// # Returns
/// 返回值
/// Returns no value because telemetry diagnostics cannot change provider execution outcome.
/// 不返回值，因为遥测诊断不能改变供应商执行结果。
fn emit_media_operational_telemetry(
    identity: &MediaUploadTelemetryIdentity,
    event_name: ProviderOperationalTelemetryEventName,
    failure_kind: Option<&str>,
) {
    let mut event = ProviderOperationalTelemetryEvent::new(event_name);
    event.provider_id = Some(identity.provider_id.clone());
    event.service_entry_id = Some(identity.service_entry_id.clone());
    event.connection_id = Some(identity.connection_id.clone());
    event.model_id = Some(identity.model_id.clone());
    event.protocol_profile_id = Some(identity.protocol_profile_id.clone());
    event.route_revision = Some(identity.route_revision.clone());
    event.manifest_revision = Some(identity.manifest_revision);
    event.manifest_generation = Some(identity.manifest_generation);
    event.request_profiles = identity.request_profiles.clone();
    event.response_profiles = identity.response_profiles.clone();
    event.operation_id = Some(CONVERSATION_MEDIA_INPUT_OPERATION_ID.to_string());
    event.upload_channel_id = Some(identity.upload_channel_id.clone());
    event.failure_kind = failure_kind.map(str::to_string);
    if let Err(error) = record_provider_operational_telemetry(event) {
        eprintln!("[media-upload] structured telemetry was rejected: {error}");
    }
}

/// Prepares every local attachment before session materialization and provider dispatch.
/// 在会话物化与供应商分派前准备所有本地附件。
///
/// # Parameters
/// 参数
/// `app` and `state` carry ordered progress events without persisting ephemeral byte counters.
/// `app` 与 `state` 发送有序进度事件，但不持久化临时字节计数。
/// `agent_runtime` resolves the exact selected account and provider route; `client_submission_id`
/// owns cancellation and progress; `session_id` scopes the immutable route lease;
/// `session_runtime_state` supplies the persisted lease identity; `model_selection` is the exact
/// composer snapshot; and `attachments` are the already validated core attachments.
/// `agent_runtime` 解析精确选定的账号与供应商路由；`client_submission_id` 持有取消与进度；
/// `session_id` 约束不可变路由租约；`session_runtime_state` 提供持久化租约身份；
/// `model_selection` 是精确输入区快照；`attachments` 是已经校验的核心附件。
///
/// # Returns
/// 返回值
/// Success after every attachment is prepared and remembered, or an explicit lease, capability,
/// credential, upload, cancellation, or projection error.
/// 全部附件完成准备并记忆后返回成功；否则返回明确的租约、能力、凭据、上传、取消或投影错误。
pub(crate) async fn prepare_submission_media(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    client_submission_id: &str,
    session_id: Option<SessionId>,
    session_runtime_state: Option<&vulcan_workbench_model::SessionRuntimeStateEntry>,
    model_selection: Option<&SubmissionModelSelection>,
    attachments: &[ConversationAttachment],
) -> Result<PreparedSubmissionMedia, String> {
    if attachments.is_empty()
        && !session_runtime_state.is_some_and(session_runtime_state_has_video_attachment)
    {
        return Ok(PreparedSubmissionMedia::default());
    }
    let cancellation = register_media_upload(client_submission_id)?;
    let result = prepare_submission_media_inner(
        app,
        state,
        agent_runtime,
        client_submission_id,
        session_id,
        session_runtime_state,
        model_selection,
        attachments,
        &cancellation,
    )
    .await;
    remove_media_upload(client_submission_id);
    result
}

/// Commits prepared video references together with their canonical user-message owner.
/// 将已准备视频引用与其规范用户消息所有者一并提交。
pub(crate) fn commit_prepared_submission_media_ledger(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    owner_message_id: Option<&str>,
    prepared: PreparedSubmissionMedia,
) -> Result<(), String> {
    if prepared.references.is_empty() {
        return Ok(());
    }
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
        .ok_or_else(|| String::from("Media reference ledger session runtime state is missing."))?;
    for reference in prepared.references {
        // A reference without a historical owner and without a newly admitted canonical owner
        // is request-scoped only and must not become an orphaned durable ledger entry.
        // 没有历史所有者且没有新准入规范所有者的引用仅属于请求作用域，不得成为孤立持久账本条目。
        if reference.owner_message_ids.is_empty()
            && (!reference.attach_current_owner || owner_message_id.is_none())
        {
            continue;
        }
        runtime_state
            .media_reference_ledger
            .upsert(reference.into_ledger_entry(owner_message_id));
    }
    runtime_state.updated_at = current_unix_millis_text();
    Ok(())
}

/// Resolves media preparation through the same immutable session route as provider dispatch.
/// 通过与供应商分派相同的不可变会话路由解析媒体准备执行。
///
/// # Parameters
/// 参数
/// `agent_runtime` owns current catalogs and secrets; session inputs identify the lease owner and
/// persisted identity; `model_selection` is the exact per-submit UI snapshot.
/// `agent_runtime` 持有当前目录与秘密；会话输入标识租约所有者与持久化身份；
/// `model_selection` 是精确的逐次提交界面快照。
///
/// # Returns
/// 返回值
/// A credential-bearing execution built from the frozen session selection, or from current
/// configuration only for a draft without a session owner.
/// 返回从冻结会话选择构建的携带凭据执行；仅无会话所有者的草稿从当前配置构建。
async fn resolve_submission_media_execution(
    agent_runtime: &AgentRuntimeBridge,
    session_id: Option<SessionId>,
    session_runtime_state: Option<&vulcan_workbench_model::SessionRuntimeStateEntry>,
    model_selection: Option<&SubmissionModelSelection>,
) -> Result<vulcan_provider_adapters::ResolvedPublicAiExecution, String> {
    // requestTarget follows the explicit submission snapshot, then queued-runtime overrides,
    // then the persisted session target. Partial targets remain explicit errors.
    // requestTarget 依次采用显式提交快照、排队运行时覆盖和持久化会话目标；不完整目标保持显式错误。
    let request_target = if let Some(selection) = model_selection {
        (
            Some(selection.provider_id.as_str()),
            Some(selection.model_id.as_str()),
            Some(selection.connection_id.as_str()),
        )
    } else if agent_runtime.provider_id_override.is_some()
        || agent_runtime.model_id_override.is_some()
        || agent_runtime.connection_id_override.is_some()
    {
        (
            agent_runtime.provider_id_override.as_deref(),
            agent_runtime.model_id_override.as_deref(),
            agent_runtime.connection_id_override.as_deref(),
        )
    } else {
        (
            session_runtime_state.and_then(|state| state.active_provider_id.as_deref()),
            session_runtime_state.and_then(|state| state.active_model_id.as_deref()),
            session_runtime_state.and_then(|state| state.active_connection_id.as_deref()),
        )
    };
    if let Some(session_id) = session_id {
        // blueprint installs or restores exactly one session lease and validates persisted identity
        // before any source file is read or uploaded.
        // blueprint 在读取或上传任何源文件前安装或恢复唯一会话租约，并校验持久化身份。
        let blueprint =
            vulcan_agent_runtime::agent_runtime::provider::resolve_public_ai_provider_blueprint(
                agent_runtime.public_ai_runtime.as_ref(),
                session_id.to_string().as_str(),
                session_runtime_state.and_then(|state| state.provider_route_lease.as_ref()),
                ReasoningEffortLevel::Off,
                request_target.0,
                request_target.1,
                request_target.2,
            )?;
        let selection = blueprint.frozen_selection.ok_or_else(|| {
            String::from("Media preparation did not freeze a session provider route.")
        })?;
        return agent_runtime
            .public_ai_runtime
            .resolve_execution_from_frozen_selection(selection)
            .await;
    }

    let configuration = agent_runtime.public_ai_runtime.configuration_snapshot()?;
    match request_target {
        (Some(provider_id), Some(model_id), Some(connection_id)) => {
            agent_runtime
                .public_ai_runtime
                .resolve_connection_model_execution(
                    &configuration,
                    connection_id,
                    provider_id,
                    model_id,
                )
                .await
        }
        (Some(provider_id), Some(model_id), None) => {
            agent_runtime
                .public_ai_runtime
                .resolve_model_execution(&configuration, provider_id, model_id)
                .await
        }
        (None, None, None) => {
            agent_runtime
                .public_ai_runtime
                .resolve_active_execution(&configuration)
                .await
        }
        _ => Err(String::from(
            "Media preparation requires provider_id and model_id together; connection_id is optional only with both.",
        )),
    }
}

/// Executes the cancellable attachment loop while the outer registration remains visible to RPC callers.
/// 在外层注册仍对 RPC 调用方可见期间执行可取消的附件循环。
///
/// # Parameters
/// 参数
/// `app`, `state`, and `client_submission_id` publish ordered progress; `agent_runtime`,
/// `session_id`, `session_runtime_state`, and `model_selection` resolve the frozen execution;
/// `attachments` are exact source resources; and `cancellation` is the registered submission token.
/// `app`、`state` 与 `client_submission_id` 发布有序进度；`agent_runtime`、`session_id`、
/// `session_runtime_state` 与 `model_selection` 解析冻结执行；`attachments` 是精确源资源；
/// `cancellation` 是已注册的提交取消令牌。
///
/// # Returns
/// 返回值
/// Success after every attachment crosses its required preparation path, or the first explicit error.
/// 全部附件越过其必需准备路径后返回成功；否则返回首个明确错误。
async fn prepare_submission_media_inner(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    client_submission_id: &str,
    session_id: Option<SessionId>,
    session_runtime_state: Option<&vulcan_workbench_model::SessionRuntimeStateEntry>,
    model_selection: Option<&SubmissionModelSelection>,
    attachments: &[ConversationAttachment],
    cancellation: &MediaUploadCancellation,
) -> Result<PreparedSubmissionMedia, String> {
    let execution = resolve_submission_media_execution(
        agent_runtime,
        session_id,
        session_runtime_state,
        model_selection,
    )
    .await?;
    // conversationId is derived from the parsed session owner and cannot diverge from lease scope.
    // conversationId 从已解析会话所有者派生，无法与租约作用域产生偏离。
    let conversation_id = session_id.map(|session_id| session_id.to_string());
    let provider_id = execution.route.provider.id.clone();
    let model_id = execution.model.id.clone();
    // bridgedMediaKinds freezes valid host-tool alternatives before any source file is read.
    // bridgedMediaKinds 在读取任何源文件前冻结有效宿主工具替代路径。
    let configuration = agent_runtime.public_ai_runtime.configuration_snapshot()?;
    let bridged_media_kinds =
        crate::public_ai_settings::public_ai_session_media_understanding_bridge_kinds(
            agent_runtime.public_ai_runtime.as_ref(),
            &configuration,
            session_runtime_state,
        )?;
    validate_submission_media_support(
        model_id.as_str(),
        execution.route.protocol_route.protocol_profile_id.as_str(),
        &execution.model.capabilities,
        &bridged_media_kinds,
        attachments,
    )?;
    let preparation_attachments =
        collect_media_preparation_attachments(session_runtime_state, attachments);
    let file_count = preparation_attachments.len();
    let mut prepared_submission_media = PreparedSubmissionMedia::default();
    for (file_index, preparation_attachment) in preparation_attachments.iter().enumerate() {
        let attachment = &preparation_attachment.attachment;
        let media_kind = attachment_kind_text(attachment.kind.clone());
        let total_bytes = attachment.byte_len;
        let resource_kind = attachment_kind(attachment.kind.clone());
        // Bridged attachments, including MIME formats unsupported by the selected wire encoder,
        // remain local until the fixed understanding tool is actually invoked.
        // 桥接附件（包括所选 Wire 编码器不支持的 MIME 格式）保持本地，直到固定理解工具被实际调用。
        let native_media = submission_attachment_is_native(
            execution.route.protocol_route.protocol_profile_id.as_str(),
            &execution.model.capabilities,
            attachment,
        );
        if !native_media {
            continue;
        }
        // Images and audio are intentionally request-scoped Base64 projections; only videos
        // create provider-owned references that must survive a model or provider switch.
        // 图片与音频有意保持请求作用域 Base64 投影；只有视频创建必须跨模型或供应商切换存活的
        // 供应商引用。
        if resource_kind != vulcan_ai_contract::ResourceKind::Video {
            continue;
        }
        let upload_contract = execution.client.conversation_media_upload_contract(
            &execution.route,
            resource_kind,
            attachment.mime_type.as_str(),
        )?;
        let Some(upload_contract) = upload_contract else {
            return Err(format!(
                "Video attachment `{}` has no declared provider upload channel; refusing Base64 fallback.",
                attachment.file_name
            ));
        };
        let session_scope_id = session_id.map(|session_id| session_id.to_string());
        if resource_kind == vulcan_ai_contract::ResourceKind::Video
            && upload_contract.reference_scope
                == vulcan_provider_adapters::UploadReferenceScope::Session
            && session_scope_id.is_none()
        {
            return Err(format!(
                "Upload channel `{}` requires a session identity before media preparation.",
                upload_contract.upload_channel_id
            ));
        }
        let source_identity = execution
            .client
            .read_media_source_identity(
                std::path::Path::new(attachment.local_path.as_str()),
                Some(cancellation),
            )
            .await?;
        let now_unix_millis = current_unix_millis_u64();
        let source_ledger_reference = (resource_kind == vulcan_ai_contract::ResourceKind::Video)
            .then(|| session_runtime_state)
            .flatten()
            .and_then(|runtime_state| {
                runtime_state.media_reference_ledger.find_usable(
                    source_identity.content_sha256.as_str(),
                    source_identity.byte_len,
                    provider_id.as_str(),
                    execution.route.entry.id.as_str(),
                    upload_contract.upload_channel_id.as_str(),
                    upload_contract.contract_fingerprint.as_str(),
                    session_media_reference_scope(upload_contract.reference_scope),
                    execution.route.connection.id.as_str(),
                    session_scope_id.as_deref(),
                    resource_kind_text(resource_kind),
                    attachment.mime_type.as_str(),
                    current_unix_millis_u64(),
                )
            })
            .filter(|reference| {
                // Historical attachments keep their original provider reference even after provider TTL metadata expires.
                // 历史附件即使供应商 TTL 元数据已过期，也继续保留并使用原供应商引用。
                submission_media_reference_is_usable_for_current_attachment(
                    preparation_attachment.attach_current_owner,
                    reference.created_at_unix_millis,
                    reference.expires_at_unix_millis,
                    now_unix_millis,
                )
            })
            .cloned();
        if let Some(source_ledger_reference) = source_ledger_reference {
            execution
                .client
                .remember_persisted_media_reference(
                    &execution.route,
                    std::path::Path::new(attachment.local_path.as_str()),
                    resource_kind,
                    attachment.mime_type.as_str(),
                    source_ledger_reference.provider_reference.clone(),
                    source_ledger_reference.source_sha256.as_str(),
                    source_ledger_reference.source_byte_len,
                    session_scope_id.as_deref(),
                    source_ledger_reference.created_at_unix_millis,
                    source_ledger_reference.expires_at_unix_millis,
                )
                .await?;
            prepared_submission_media.push_video_reference(
                resource_kind,
                prepared_media_reference_from_ledger(
                    &source_ledger_reference,
                    preparation_attachment.attach_current_owner,
                ),
            );
            continue;
        }
        if let Some(prepared_reference) = execution
            .client
            .prepared_media_reference(
                &execution.route,
                std::path::Path::new(attachment.local_path.as_str()),
                resource_kind,
                attachment.mime_type.as_str(),
                session_scope_id.as_deref(),
                Some(cancellation),
            )
            .await
            .filter(|reference| {
                // Re-attaching an expired local video is a new upload intent; an old history replay is not.
                // 重新附加已过期本地视频代表新的上传意图；历史回放不代表新的上传意图。
                submission_media_reference_is_usable_for_current_attachment(
                    preparation_attachment.attach_current_owner,
                    reference.created_at_unix_millis,
                    reference.expires_at_unix_millis,
                    now_unix_millis,
                )
            })
        {
            prepared_submission_media.push_video_reference(
                resource_kind,
                prepared_media_reference_from_source(
                    &execution,
                    &upload_contract,
                    &source_identity,
                    resource_kind,
                    attachment.mime_type.as_str(),
                    prepared_reference.provider_reference,
                    prepared_reference.created_at_unix_millis,
                    prepared_reference.expires_at_unix_millis,
                    session_scope_id.as_deref(),
                    preparation_attachment.owner_message_ids.clone(),
                    preparation_attachment.attach_current_owner,
                ),
            );
            continue;
        }
        let upload_channel_id = upload_contract.upload_channel_id.clone();
        // identity is captured once before any source read so progress, telemetry, cache, and
        // dispatch cannot disagree after a manifest hot replacement.
        // identity 在任何源读取前只捕获一次，确保 Manifest 热替换后进度、遥测、缓存与分派
        // 不会产生分歧。
        let identity = MediaUploadTelemetryIdentity {
            provider_id: provider_id.clone(),
            service_entry_id: execution.route.entry.id.clone(),
            connection_id: execution.route.connection.id.clone(),
            model_id: model_id.clone(),
            protocol_profile_id: execution.route.protocol_route.protocol_profile_id.clone(),
            manifest_revision: execution.route.manifest_revision,
            manifest_generation: execution.route.manifest_generation,
            route_revision: execution.route.route_revision.clone(),
            upload_channel_id,
            request_profiles: execution
                .route
                .request_projection_profiles
                .iter()
                .map(|profile| ProviderOperationalProfileRevision {
                    profile_id: profile.profile_id.clone(),
                    revision: profile.revision,
                })
                .collect(),
            response_profiles: execution
                .route
                .response_projection_profiles
                .iter()
                .map(|profile| ProviderOperationalProfileRevision {
                    profile_id: profile.profile_id.clone(),
                    revision: profile.revision,
                })
                .collect(),
        };
        emit_media_operational_telemetry(
            &identity,
            ProviderOperationalTelemetryEventName::ProviderUploadStarted,
            None,
        );
        emit_media_upload_progress(
            app,
            state,
            media_upload_progress_delta(
                &identity,
                client_submission_id,
                conversation_id.as_deref(),
                attachment.file_name.as_str(),
                media_kind,
                file_index,
                file_count,
                MediaUploadPhase::Preparing,
                0,
                total_bytes,
                None,
            ),
        );
        let progress_attachment = attachment.clone();
        let progress_identity = identity.clone();
        let progress_conversation_id = conversation_id.clone();
        let progress_client_submission_id = client_submission_id.to_string();
        let progress_media_kind = media_kind.to_string();
        let progress_state = state.clone();
        let progress_app = app.clone();
        let progress: MediaUploadProgressCallback = Arc::new(move |bytes_sent, total_bytes| {
            emit_media_upload_progress(
                &progress_app,
                &progress_state,
                media_upload_progress_delta(
                    &progress_identity,
                    progress_client_submission_id.as_str(),
                    progress_conversation_id.as_deref(),
                    progress_attachment.file_name.as_str(),
                    progress_media_kind.as_str(),
                    file_index,
                    file_count,
                    MediaUploadPhase::Uploading,
                    bytes_sent,
                    total_bytes,
                    None,
                ),
            );
        });
        // firstUsedAt anchors both the process cache and durable ledger to the beginning of this upload attempt.
        // firstUsedAt 将进程缓存与持久账本同时锚定到本次上传尝试开始时。
        let first_used_at_unix_millis = current_unix_millis_u64();
        // Physical media upload reads the same latest credential revision as conversation requests.
        // 媒体物理上传与会话请求一样读取同一份最新凭据修订版。
        let prepared = async {
            let secret = execution.credential.resolve_for_request().await?;
            execution
                .client
                .prepare_local_media(
                    &execution.route,
                    model_id.as_str(),
                    &secret,
                    std::path::Path::new(attachment.local_path.as_str()),
                    resource_kind,
                    attachment.mime_type.as_str(),
                    false,
                    true,
                    Some(progress),
                    cancellation,
                )
                .await
        }
        .await;
        let prepared = match prepared {
            Ok(prepared) => prepared,
            Err(error) if cancellation.is_cancelled() || error == "media upload cancelled" => {
                emit_media_operational_telemetry(
                    &identity,
                    ProviderOperationalTelemetryEventName::ProviderUploadCancelled,
                    Some("host_cancelled"),
                );
                emit_media_upload_progress(
                    app,
                    state,
                    media_upload_progress_delta(
                        &identity,
                        client_submission_id,
                        conversation_id.as_deref(),
                        attachment.file_name.as_str(),
                        media_kind,
                        file_index,
                        file_count,
                        MediaUploadPhase::Cancelled,
                        0,
                        total_bytes,
                        None,
                    ),
                );
                return Err(String::from("submission_cancelled: media upload cancelled"));
            }
            Err(error) => {
                emit_media_operational_telemetry(
                    &identity,
                    ProviderOperationalTelemetryEventName::ProviderUploadFailed,
                    Some("materialization_failed"),
                );
                emit_media_upload_progress(
                    app,
                    state,
                    media_upload_progress_delta(
                        &identity,
                        client_submission_id,
                        conversation_id.as_deref(),
                        attachment.file_name.as_str(),
                        media_kind,
                        file_index,
                        file_count,
                        MediaUploadPhase::Failed,
                        0,
                        total_bytes,
                        Some(error.clone()),
                    ),
                );
                return Err(error);
            }
        };
        if cancellation.is_cancelled() {
            emit_media_operational_telemetry(
                &identity,
                ProviderOperationalTelemetryEventName::ProviderUploadCancelled,
                Some("host_cancelled"),
            );
            emit_media_upload_progress(
                app,
                state,
                media_upload_progress_delta(
                    &identity,
                    client_submission_id,
                    conversation_id.as_deref(),
                    attachment.file_name.as_str(),
                    media_kind,
                    file_index,
                    file_count,
                    MediaUploadPhase::Cancelled,
                    total_bytes.unwrap_or_default(),
                    total_bytes,
                    None,
                ),
            );
            return Err(String::from("submission_cancelled: media upload cancelled"));
        }
        let prepared_reference = prepared
            .prepared_reference()
            .ok_or_else(|| String::from("Provider media upload did not return a reference."))?
            .to_string();
        let prepared_source_identity = prepared.source_identity();
        let prepared_created_at_unix_millis = first_used_at_unix_millis;
        let prepared_expires_at_unix_millis = upload_contract
            .cache_duration_ms
            .map(|cache_duration_ms| {
                prepared_created_at_unix_millis.saturating_add(cache_duration_ms)
            })
            .unwrap_or(u64::MAX);
        execution.client.remember_conversation_prepared_media(
            &execution.route,
            std::path::Path::new(attachment.local_path.as_str()),
            attachment_kind(attachment.kind.clone()),
            attachment.mime_type.as_str(),
            prepared,
            session_scope_id.as_deref(),
            Some((
                prepared_created_at_unix_millis,
                prepared_expires_at_unix_millis,
            )),
        );
        prepared_submission_media.push_video_reference(
            resource_kind,
            prepared_media_reference_from_source(
                &execution,
                &upload_contract,
                &prepared_source_identity,
                resource_kind,
                attachment.mime_type.as_str(),
                prepared_reference,
                prepared_created_at_unix_millis,
                prepared_expires_at_unix_millis,
                session_scope_id.as_deref(),
                preparation_attachment.owner_message_ids.clone(),
                preparation_attachment.attach_current_owner,
            ),
        );
        emit_media_operational_telemetry(
            &identity,
            ProviderOperationalTelemetryEventName::ProviderUploadCompleted,
            None,
        );
        emit_media_upload_progress(
            app,
            state,
            media_upload_progress_delta(
                &identity,
                client_submission_id,
                conversation_id.as_deref(),
                attachment.file_name.as_str(),
                media_kind,
                file_index,
                file_count,
                MediaUploadPhase::Completed,
                total_bytes.unwrap_or_default(),
                total_bytes,
                None,
            ),
        );
    }
    Ok(prepared_submission_media)
}

/// Reports whether the active canonical conversation still contains a video attachment.
/// 判断活动规范会话中是否仍包含视频附件。
fn session_runtime_state_has_video_attachment(
    runtime_state: &vulcan_workbench_model::SessionRuntimeStateEntry,
) -> bool {
    runtime_state
        .conversation_state
        .messages
        .iter()
        .any(|message| {
            let vulcan_context::SessionConversationPayload::UserText { attachments, .. } =
                &message.payload
            else {
                return false;
            };
            attachments
                .iter()
                .any(|attachment| attachment.kind == ConversationAttachmentKind::Video)
        })
}

/// Decides whether one prepared reference may satisfy the current attachment intent.
/// 判断一条已准备引用是否可以满足当前附件意图。
///
/// Historical replay deliberately ignores provider TTL metadata; an explicit current attachment
/// reuses only a reference whose known lifetime is still valid, while `u64::MAX` means unknown.
/// 历史回放有意忽略供应商 TTL 元数据；本轮明确附件仅复用已知生命周期仍有效的引用，
/// `u64::MAX` 表示没有已知的过期时间。
fn submission_media_reference_is_usable_for_current_attachment(
    attach_current_owner: bool,
    created_at_unix_millis: u64,
    expires_at_unix_millis: u64,
    now_unix_millis: u64,
) -> bool {
    !attach_current_owner
        || expires_at_unix_millis == u64::MAX
        || (created_at_unix_millis <= now_unix_millis && expires_at_unix_millis > now_unix_millis)
}

/// Describes one deduplicated media source and its canonical message ownership.
/// 描述一个去重后的媒体源及其规范消息所有权。
#[derive(Debug, Clone)]
struct MediaPreparationAttachment {
    /// Exact attachment source selected for provider preparation.
    /// 用于供应商准备的精确附件源。
    attachment: ConversationAttachment,
    /// Historical canonical user-message owners of this source.
    /// 该源文件对应的历史规范用户消息所有者。
    owner_message_ids: BTreeSet<String>,
    /// Whether this source is also attached to the current submission.
    /// 该源文件是否同时属于当前提交。
    attach_current_owner: bool,
}

/// Returns whether two attachment sources identify the same local media input.
/// 判断两个附件源是否标识同一个本地媒体输入。
fn same_media_preparation_attachment(
    left: &ConversationAttachment,
    right: &ConversationAttachment,
) -> bool {
    left.kind == right.kind
        && left.local_path == right.local_path
        && left.mime_type == right.mime_type
}

/// Collects current attachments plus every historical video that must be projected for a new route.
/// 收集当前附件以及新路由必须投影的全部历史视频。
fn collect_media_preparation_attachments(
    session_runtime_state: Option<&vulcan_workbench_model::SessionRuntimeStateEntry>,
    attachments: &[ConversationAttachment],
) -> Vec<MediaPreparationAttachment> {
    let mut collected: Vec<MediaPreparationAttachment> = Vec::new();
    for attachment in session_runtime_state
        .into_iter()
        .flat_map(|runtime_state| runtime_state.conversation_state.messages.iter())
        .flat_map(|message| {
            let vulcan_context::SessionConversationPayload::UserText { attachments, .. } =
                &message.payload
            else {
                return Vec::new();
            };
            attachments
                .iter()
                .filter(|attachment| attachment.kind == ConversationAttachmentKind::Video)
                .map(|attachment| (message.id.as_str(), attachment))
                .collect::<Vec<_>>()
        })
    {
        if let Some(existing) = collected
            .iter_mut()
            .find(|existing| same_media_preparation_attachment(&existing.attachment, attachment.1))
        {
            existing.owner_message_ids.insert(attachment.0.to_string());
        } else {
            let mut owner_message_ids = BTreeSet::new();
            owner_message_ids.insert(attachment.0.to_string());
            collected.push(MediaPreparationAttachment {
                attachment: attachment.1.clone(),
                owner_message_ids,
                attach_current_owner: false,
            });
        }
    }
    for attachment in attachments {
        if let Some(existing) = collected
            .iter_mut()
            .find(|existing| same_media_preparation_attachment(&existing.attachment, attachment))
        {
            existing.attach_current_owner = true;
        } else {
            collected.push(MediaPreparationAttachment {
                attachment: attachment.clone(),
                owner_message_ids: BTreeSet::new(),
                attach_current_owner: true,
            });
        }
    }
    collected
}

/// Converts a persisted ledger entry into the current submission result shape.
/// 将已持久化账本条目转换为当前提交结果形状。
fn prepared_media_reference_from_ledger(
    entry: &SessionMediaReferenceEntry,
    attach_current_owner: bool,
) -> PreparedSubmissionMediaReference {
    PreparedSubmissionMediaReference {
        source_sha256: entry.source_sha256.clone(),
        source_byte_len: entry.source_byte_len,
        provider_id: entry.provider_id.clone(),
        service_entry_id: entry.service_entry_id.clone(),
        upload_channel_id: entry.upload_channel_id.clone(),
        upload_contract_fingerprint: entry.upload_contract_fingerprint.clone(),
        reference_scope: entry.reference_scope,
        connection_id: entry.connection_id.clone(),
        session_id: entry.session_id.clone(),
        resource_kind: entry.resource_kind.clone(),
        mime_type: entry.mime_type.clone(),
        provider_reference: entry.provider_reference.clone(),
        created_at_unix_millis: entry.created_at_unix_millis,
        expires_at_unix_millis: entry.expires_at_unix_millis,
        owner_message_ids: entry.owner_message_ids.clone(),
        attach_current_owner,
    }
}

/// Converts one freshly verified source and upload contract into a ledger candidate.
/// 将一个刚验证的源与上传契约转换为账本候选项。
fn prepared_media_reference_from_source(
    execution: &vulcan_provider_adapters::ResolvedPublicAiExecution,
    upload_contract: &vulcan_provider_adapters::ConversationMediaUploadContract,
    source_identity: &vulcan_provider_adapters::PreparedMediaSourceIdentity,
    resource_kind: vulcan_ai_contract::ResourceKind,
    mime_type: &str,
    provider_reference: String,
    created_at_unix_millis: u64,
    expires_at_unix_millis: u64,
    session_scope_id: Option<&str>,
    owner_message_ids: BTreeSet<String>,
    attach_current_owner: bool,
) -> PreparedSubmissionMediaReference {
    let reference_scope = match upload_contract.reference_scope {
        vulcan_provider_adapters::UploadReferenceScope::ProviderProtocol => {
            SessionMediaReferenceScope::ProviderProtocol
        }
        vulcan_provider_adapters::UploadReferenceScope::Credential => {
            SessionMediaReferenceScope::Credential
        }
        vulcan_provider_adapters::UploadReferenceScope::Session => {
            SessionMediaReferenceScope::Session
        }
    };
    PreparedSubmissionMediaReference {
        source_sha256: source_identity.content_sha256.clone(),
        source_byte_len: source_identity.byte_len,
        provider_id: execution.route.provider.id.clone(),
        service_entry_id: execution.route.entry.id.clone(),
        upload_channel_id: upload_contract.upload_channel_id.clone(),
        upload_contract_fingerprint: upload_contract.contract_fingerprint.clone(),
        reference_scope,
        connection_id: matches!(reference_scope, SessionMediaReferenceScope::Credential)
            .then(|| execution.route.connection.id.clone()),
        session_id: matches!(reference_scope, SessionMediaReferenceScope::Session)
            .then(|| session_scope_id.map(str::to_owned))
            .flatten(),
        resource_kind: resource_kind_text(resource_kind).to_string(),
        mime_type: mime_type.to_string(),
        provider_reference,
        created_at_unix_millis,
        expires_at_unix_millis,
        owner_message_ids,
        attach_current_owner,
    }
}

/// Converts the shared signed millisecond clock into the non-negative ledger time domain.
/// 将共享的有符号毫秒时钟转换为非负账本时间域。
fn current_unix_millis_u64() -> u64 {
    u64::try_from(current_unix_millis_value()).unwrap_or(u64::MAX)
}

/// Converts the Manifest scope enum into the durable workbench scope enum.
/// 将 Manifest 作用域枚举转换为可持久化工作台作用域枚举。
fn session_media_reference_scope(
    scope: vulcan_provider_adapters::UploadReferenceScope,
) -> SessionMediaReferenceScope {
    match scope {
        vulcan_provider_adapters::UploadReferenceScope::ProviderProtocol => {
            SessionMediaReferenceScope::ProviderProtocol
        }
        vulcan_provider_adapters::UploadReferenceScope::Credential => {
            SessionMediaReferenceScope::Credential
        }
        vulcan_provider_adapters::UploadReferenceScope::Session => {
            SessionMediaReferenceScope::Session
        }
    }
}

/// Rejects unsupported media before any provider upload or inline pre-read starts.
/// 在任何供应商上传或内联预读取开始前拒绝不支持的媒体。
fn validate_submission_media_support(
    model_id: &str,
    protocol_profile_id: &str,
    capabilities: &vulcan_ai_contract::ModelCapabilities,
    bridged_media_kinds: &BTreeSet<vulcan_ai_contract::ResourceKind>,
    attachments: &[ConversationAttachment],
) -> Result<(), String> {
    for attachment in attachments {
        let resource_kind = attachment_kind(attachment.kind.clone());
        let media_kind = attachment_kind_text(attachment.kind.clone());
        // modelSupported distinguishes a missing model declaration in the final error.
        // modelSupported 用于在最终错误中区分模型声明缺失。
        let model_supported = capabilities.input_resources.contains(&resource_kind);
        // nativeMedia uses the same exact predicate as preparation so bridge admission cannot
        // accidentally fall back into provider upload for an unsupported MIME format.
        // nativeMedia 与准备流程使用完全相同的判定，避免桥接准入后的不受支持 MIME 格式误入供应商上传。
        let native_media =
            submission_attachment_is_native(protocol_profile_id, capabilities, attachment);
        if native_media {
            continue;
        }
        if bridged_media_kinds.contains(&resource_kind)
            && matches!(
                attachment.kind,
                ConversationAttachmentKind::Image
                    | ConversationAttachmentKind::Video
                    | ConversationAttachmentKind::Audio
            )
        {
            continue;
        }
        if !model_supported {
            return Err(format!(
                "Model `{model_id}` does not support `{media_kind}` attachments; media upload was not started."
            ));
        }
        return Err(format!(
            "Protocol profile `{protocol_profile_id}` does not support `{media_kind}` attachments; media upload was not started."
        ));
    }
    Ok(())
}

/// Reports whether one attachment can use the selected model's exact native request route.
/// 判断一个附件是否可使用所选模型的精确原生请求路由。
///
/// # Parameters
/// 参数
/// `protocol_profile_id` identifies the frozen encoder, `capabilities` contains the effective
/// model contract, and `attachment` contains the actual media kind and MIME type.
/// `protocol_profile_id` 标识冻结编码器，`capabilities` 包含生效模型契约，`attachment` 包含实际媒体类型与 MIME。
///
/// # Returns
/// 返回值
/// `true` only when model declaration, modality projector, and format-sensitive encoder agree.
/// 仅当模型声明、模态投影器与格式敏感编码器全部一致时返回 `true`。
fn submission_attachment_is_native(
    protocol_profile_id: &str,
    capabilities: &vulcan_ai_contract::ModelCapabilities,
    attachment: &ConversationAttachment,
) -> bool {
    // resourceKind normalizes the core attachment category for model and protocol comparison.
    // resourceKind 归一化核心附件类别，用于模型与协议比较。
    let resource_kind = attachment_kind(attachment.kind.clone());
    // protocolSupported reuses the request normalizer's exact kind-and-MIME predicate so
    // pre-dispatch preparation and final message projection cannot diverge.
    // protocolSupported 复用请求归一化器的精确类型与 MIME 判定，确保分派前准备与最终消息投影不会分叉。
    let protocol_supported =
        vulcan_agent_runtime::provider_input_normalizer::protocol_profile_supports_attachment(
            protocol_profile_id,
            attachment,
        );
    capabilities.input_resources.contains(&resource_kind) && protocol_supported
}

/// Emits one ordered ephemeral progress delta and advances the shared workbench revision.
/// 发送一条有序临时进度增量，并推进共享工作台版本。
fn emit_media_upload_progress(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    progress: MediaUploadProgressChangedDelta,
) {
    let event = state.lock().ok().map(|mut guard| {
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let generation = guard.workbench_generation;
        guard.workbench.revision = generation;
        WorkbenchDeltaEnvelope {
            generation,
            delta: WorkbenchDelta::MediaUploadProgressChanged(progress),
        }
    });
    if let Some(event) = event {
        emit_service_event_with_trace(
            app,
            "media_upload_progress",
            ServiceEvent::WorkbenchDelta(Box::new(event)),
        );
    }
}

/// Converts a core attachment kind into the provider-neutral media contract.
/// 将核心附件类型转换为供应商无关的媒体契约。
fn attachment_kind(kind: ConversationAttachmentKind) -> vulcan_ai_contract::ResourceKind {
    match kind {
        ConversationAttachmentKind::Image => vulcan_ai_contract::ResourceKind::Image,
        ConversationAttachmentKind::Audio => vulcan_ai_contract::ResourceKind::Audio,
        ConversationAttachmentKind::Video => vulcan_ai_contract::ResourceKind::Video,
        ConversationAttachmentKind::Pdf | ConversationAttachmentKind::File => {
            vulcan_ai_contract::ResourceKind::File
        }
    }
}

/// Converts a core attachment kind into the stable UI label.
/// 将核心附件类型转换为稳定的界面标签。
fn attachment_kind_text(kind: ConversationAttachmentKind) -> &'static str {
    match kind {
        ConversationAttachmentKind::Image => "image",
        ConversationAttachmentKind::Audio => "audio",
        ConversationAttachmentKind::Video => "video",
        ConversationAttachmentKind::Pdf => "pdf",
        ConversationAttachmentKind::File => "file",
    }
}

/// Converts a provider-neutral resource kind into its stable ledger label.
/// 将供应商无关资源类型转换为稳定的账本标签。
fn resource_kind_text(kind: vulcan_ai_contract::ResourceKind) -> &'static str {
    match kind {
        vulcan_ai_contract::ResourceKind::Image => "image",
        vulcan_ai_contract::ResourceKind::Video => "video",
        vulcan_ai_contract::ResourceKind::Audio => "audio",
        vulcan_ai_contract::ResourceKind::File => "file",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        collect_media_preparation_attachments, register_media_upload, remove_media_upload,
        submission_attachment_is_native,
        submission_media_reference_is_usable_for_current_attachment,
        validate_submission_media_support,
    };
    use std::collections::BTreeSet;
    use vulcan_ai_contract::{ModelCapabilities, ResourceKind};
    use vulcan_context::{SessionConversationMessage, SessionConversationPayload};
    use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};
    use vulcan_workbench_model::SessionRuntimeStateEntry;

    /// Builds one attachment fixture for pre-upload capability validation.
    /// 构建一个用于上传前能力校验的附件夹具。
    fn attachment(kind: ConversationAttachmentKind) -> ConversationAttachment {
        // mimeType matches the exact media fixture kind so format-sensitive validation stays meaningful.
        // mimeType 与精确媒体夹具类型一致，使格式敏感校验保持有效。
        let mime_type = match &kind {
            ConversationAttachmentKind::Image => "image/png",
            ConversationAttachmentKind::Video => "video/mp4",
            ConversationAttachmentKind::Audio => "audio/wav",
            ConversationAttachmentKind::Pdf => "application/pdf",
            ConversationAttachmentKind::File => "application/octet-stream",
        };
        ConversationAttachment {
            kind,
            file_name: String::from("media.bin"),
            local_path: String::from("media.bin"),
            source_path: None,
            asset_id: None,
            byte_len: Some(1),
            preview_local_path: None,
            mime_type: String::from(mime_type),
        }
    }

    /// Verifies duplicate client submission identities cannot replace an active cancellation handle.
    /// 验证重复的客户端提交标识不会替换正在生效的取消句柄。
    #[test]
    fn duplicate_submission_registration_is_rejected() {
        let client_submission_id = "media-upload-duplicate-test";
        let _cancellation = register_media_upload(client_submission_id).expect("first register");
        match register_media_upload(client_submission_id) {
            Err(error) => assert!(error.contains("already preparing media")),
            Ok(_) => panic!("duplicate registration must be rejected"),
        }
        remove_media_upload(client_submission_id);
    }

    /// Verifies every historical video keeps its real owners while current reuse adds only the new owner.
    /// 验证每个历史视频保留真实所有者，而当前复用只追加新的所有者。
    #[test]
    fn historical_video_owners_are_merged_without_rebinding_history() {
        let mut first_video = attachment(ConversationAttachmentKind::Video);
        first_video.local_path = String::from("first.mp4");
        let mut second_video = attachment(ConversationAttachmentKind::Video);
        second_video.local_path = String::from("second.mp4");
        let mut runtime_state = SessionRuntimeStateEntry::default();
        runtime_state.conversation_state.messages = vec![
            SessionConversationMessage {
                id: String::from("old-a"),
                payload: SessionConversationPayload::UserText {
                    text: String::from("old a"),
                    attachments: vec![first_video.clone(), second_video],
                },
            },
            SessionConversationMessage {
                id: String::from("old-b"),
                payload: SessionConversationPayload::UserText {
                    text: String::from("old b"),
                    attachments: vec![first_video.clone()],
                },
            },
        ];

        let collected = collect_media_preparation_attachments(
            Some(&runtime_state),
            std::slice::from_ref(&first_video),
        );
        assert_eq!(collected.len(), 2);
        let first = collected
            .iter()
            .find(|candidate| candidate.attachment.local_path == "first.mp4")
            .expect("first historical video should be collected");
        assert_eq!(
            first.owner_message_ids,
            BTreeSet::from([String::from("old-a"), String::from("old-b")])
        );
        assert!(first.attach_current_owner);
        let second = collected
            .iter()
            .find(|candidate| candidate.attachment.local_path == "second.mp4")
            .expect("second historical video should be collected");
        assert_eq!(
            second.owner_message_ids,
            BTreeSet::from([String::from("old-a")])
        );
        assert!(!second.attach_current_owner);
    }

    /// Verifies a model-declared media restriction rejects before upload preparation.
    /// 验证模型声明的媒体限制会在上传准备前拒绝请求。
    #[test]
    fn unsupported_model_media_is_rejected_before_upload() {
        let capabilities = ModelCapabilities {
            input_resources: vec![ResourceKind::Image],
            ..ModelCapabilities::default()
        };
        let error = validate_submission_media_support(
            "k3-256k",
            vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID,
            &capabilities,
            &BTreeSet::new(),
            &[attachment(ConversationAttachmentKind::Video)],
        )
        .expect_err("video must be rejected by the image-only model");

        assert!(error.contains("k3-256k"));
        assert!(error.contains("media upload was not started"));
    }

    /// Verifies a protocol media restriction rejects before upload preparation.
    /// 验证协议媒体限制会在上传准备前拒绝请求。
    #[test]
    fn unsupported_protocol_media_is_rejected_before_upload() {
        let capabilities = ModelCapabilities {
            input_resources: vec![ResourceKind::Image, ResourceKind::Video],
            ..ModelCapabilities::default()
        };
        let error = validate_submission_media_support(
            "k3",
            vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID,
            &capabilities,
            &BTreeSet::new(),
            &[attachment(ConversationAttachmentKind::Video)],
        )
        .expect_err("standard Anthropic profile must reject video");

        assert!(error.contains("anthropic-messages"));
        assert!(error.contains("media upload was not started"));
    }

    /// Verifies the dedicated Anthropic video profile accepts a declared video resource.
    /// 验证专用 Anthropic 视频档案接受已声明的视频资源。
    #[test]
    fn dedicated_video_protocol_accepts_declared_video() {
        let capabilities = ModelCapabilities {
            input_resources: vec![ResourceKind::Image, ResourceKind::Video],
            ..ModelCapabilities::default()
        };

        validate_submission_media_support(
            "k3",
            vulcan_ai_protocols::ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
            &capabilities,
            &BTreeSet::new(),
            &[attachment(ConversationAttachmentKind::Video)],
        )
        .expect("dedicated video profile should accept video");
    }

    /// Verifies a format-sensitive protocol rejects an unsupported audio MIME before upload.
    /// 验证格式敏感协议会在上传前拒绝不支持的音频 MIME。
    #[test]
    fn unsupported_protocol_media_format_is_rejected_before_upload() {
        // capabilities declares native audio while the exact OpenAI Chat encoder lacks Opus format mapping.
        // capabilities 声明原生音频，但精确 OpenAI Chat 编码器缺少 Opus 格式映射。
        let capabilities = ModelCapabilities {
            input_resources: vec![ResourceKind::Audio],
            ..ModelCapabilities::default()
        };
        // opusAttachment differs from the ordinary fixture only by its exact unsupported MIME type.
        // opusAttachment 仅通过精确的不支持 MIME 类型区别于普通夹具。
        let mut opus_attachment = attachment(ConversationAttachmentKind::Audio);
        opus_attachment.file_name = String::from("voice.opus");
        opus_attachment.mime_type = String::from("audio/opus");

        let error = validate_submission_media_support(
            "audio-model",
            vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID,
            &capabilities,
            &BTreeSet::new(),
            &[opus_attachment],
        )
        .expect_err("unsupported OpenAI Chat audio format must fail before upload");

        assert!(error.contains("openai-chat-completions"));
        assert!(error.contains("media upload was not started"));
    }

    /// Verifies fixed understanding bridges admit video and audio before native upload starts.
    /// 验证固定理解桥接会在原生上传开始前放行视频与音频。
    #[test]
    fn understanding_bridges_admit_non_native_video_and_audio() {
        let capabilities = ModelCapabilities::default();
        // bridges contains only the exact media kinds backed by enabled valid tools.
        // bridges 仅包含由已启用有效工具承载的精确媒体类型。
        let bridges = BTreeSet::from([ResourceKind::Video, ResourceKind::Audio]);

        validate_submission_media_support(
            "text-only-model",
            vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID,
            &capabilities,
            &bridges,
            &[
                attachment(ConversationAttachmentKind::Video),
                attachment(ConversationAttachmentKind::Audio),
            ],
        )
        .expect("valid host bridges should admit non-native media");
    }

    /// Verifies bridge admission never reclassifies a format-incompatible attachment as native.
    /// 验证桥接准入绝不会把格式不兼容的附件重新归类为原生附件。
    #[test]
    fn understanding_bridge_preserves_format_incompatible_audio_as_local() {
        // capabilities declares audio at model level while the frozen encoder cannot encode Opus.
        // capabilities 在模型级声明音频，但冻结编码器无法编码 Opus。
        let capabilities = ModelCapabilities {
            input_resources: vec![ResourceKind::Audio],
            ..ModelCapabilities::default()
        };
        // opusAttachment is bridge-eligible but must not start native preparation.
        // opusAttachment 可由桥接处理，但不得启动原生准备。
        let mut opus_attachment = attachment(ConversationAttachmentKind::Audio);
        opus_attachment.file_name = String::from("voice.opus");
        opus_attachment.mime_type = String::from("audio/opus");
        // bridges proves only the fixed audio-understanding tool owns the fallback route.
        // bridges 证明仅固定音频理解工具持有后备路由。
        let bridges = BTreeSet::from([ResourceKind::Audio]);

        validate_submission_media_support(
            "audio-model",
            vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID,
            &capabilities,
            &bridges,
            std::slice::from_ref(&opus_attachment),
        )
        .expect("the configured understanding bridge should admit Opus");
        assert!(!submission_attachment_is_native(
            vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID,
            &capabilities,
            &opus_attachment,
        ));
    }

    /// Verifies expired history is replayed while an explicit re-attachment requests a new upload.
    /// 验证过期历史会继续回放，而明确重新附加会触发新的上传请求。
    #[test]
    fn media_reference_expiry_only_affects_current_attachment_intent() {
        assert!(submission_media_reference_is_usable_for_current_attachment(
            false, 1, 2, 3
        ));
        assert!(!submission_media_reference_is_usable_for_current_attachment(true, 1, 2, 3));
        assert!(submission_media_reference_is_usable_for_current_attachment(
            true, 1, 10, 3
        ));
        assert!(submission_media_reference_is_usable_for_current_attachment(
            true,
            1,
            u64::MAX,
            u64::MAX,
        ));
    }
}
