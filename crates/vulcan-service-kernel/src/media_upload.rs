//! Pre-dispatch preparation plus post-dispatch media-understanding upload progress.
//! 分派前媒体准备与分派后媒体理解上传进度。

use super::*;
use crate::conversation_submission::SubmissionModelSelection;
use std::collections::BTreeMap;
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
) -> Result<(), String> {
    if attachments.is_empty() {
        return Ok(());
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
) -> Result<(), String> {
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
    let file_count = attachments.len();
    for (file_index, attachment) in attachments.iter().enumerate() {
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
        let upload_channel_id = execution
            .client
            .conversation_media_upload_channel_id(
                &execution.route,
                resource_kind,
                attachment.mime_type.as_str(),
            )?
            .unwrap_or_else(|| String::from("host.inline_payload"));
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
        execution.client.remember_prepared_media(
            &execution.route,
            model_id.as_str(),
            std::path::Path::new(attachment.local_path.as_str()),
            attachment_kind(attachment.kind.clone()),
            attachment.mime_type.as_str(),
            prepared,
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
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::{
        register_media_upload, remove_media_upload, submission_attachment_is_native,
        validate_submission_media_support,
    };
    use std::collections::BTreeSet;
    use vulcan_ai_contract::{ModelCapabilities, ResourceKind};
    use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};

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
}
