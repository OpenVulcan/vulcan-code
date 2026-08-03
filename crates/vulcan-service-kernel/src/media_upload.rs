//! Pre-dispatch media preparation and cancellation for composer submissions.
//! 输入区提交的分派前媒体准备与取消。

use super::*;
use crate::conversation_submission::SubmissionModelSelection;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};
use vulcan_provider_adapters::{MediaUploadCancellation, MediaUploadProgressCallback};
use vulcan_service_api::{
    MediaUploadPhase, MediaUploadProgressChangedDelta, ServiceEvent, WorkbenchDelta,
    WorkbenchDeltaEnvelope,
};

/// Process-local pending media submissions keyed by the client submission id.
/// 按客户端提交标识索引的进程内待处理媒体提交。
static PENDING_MEDIA_UPLOADS: OnceLock<StdMutex<BTreeMap<String, MediaUploadCancellation>>> =
    OnceLock::new();

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

/// Prepares every local attachment before session materialization and provider dispatch.
/// 在会话物化与供应商分派前准备所有本地附件。
///
/// # Parameters
/// 参数
/// `agent_runtime` resolves the exact selected account and provider route.
/// `agent_runtime` 解析精确选定的账号与供应商路由。
/// `attachments` are the already validated core attachments from the composer submission.
/// `attachments` 是输入区提交中已经校验的核心附件。
/// `app` and `state` carry ordered progress events without persisting ephemeral byte counters.
/// `app` 与 `state` 发送有序进度事件，但不持久化临时字节计数。
pub(crate) async fn prepare_submission_media(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    client_submission_id: &str,
    conversation_id: Option<&str>,
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
        conversation_id,
        model_selection,
        attachments,
        &cancellation,
    )
    .await;
    remove_media_upload(client_submission_id);
    result
}

/// Executes the cancellable attachment loop while the outer registration remains visible to RPC callers.
/// 在外层注册仍对 RPC 调用方可见期间执行可取消的附件循环。
async fn prepare_submission_media_inner(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    client_submission_id: &str,
    conversation_id: Option<&str>,
    model_selection: Option<&SubmissionModelSelection>,
    attachments: &[ConversationAttachment],
    cancellation: &MediaUploadCancellation,
) -> Result<(), String> {
    let configuration = agent_runtime.public_ai_runtime.configuration_snapshot()?;
    let execution = if let Some(selection) = model_selection {
        agent_runtime
            .public_ai_runtime
            .resolve_connection_model_execution(
                &configuration,
                selection.connection_id.as_str(),
                selection.provider_id.as_str(),
                selection.model_id.as_str(),
            )
            .await?
    } else {
        match (
            agent_runtime.provider_id_override.as_deref(),
            agent_runtime.model_id_override.as_deref(),
            agent_runtime.connection_id_override.as_deref(),
        ) {
            (Some(provider_id), Some(model_id), Some(connection_id)) => {
                agent_runtime
                    .public_ai_runtime
                    .resolve_connection_model_execution(
                        &configuration,
                        connection_id,
                        provider_id,
                        model_id,
                    )
                    .await?
            }
            (Some(provider_id), Some(model_id), None) => {
                agent_runtime
                    .public_ai_runtime
                    .resolve_model_execution(&configuration, provider_id, model_id)
                    .await?
            }
            (None, None, None) => {
                agent_runtime
                    .public_ai_runtime
                    .resolve_active_execution(&configuration)
                    .await?
            }
            _ => {
                return Err(String::from(
                    "Media preparation requires provider_id and model_id together; connection_id is optional only with both.",
                ));
            }
        }
    };
    let provider_id = execution.route.provider.id.clone();
    let model_id = execution.model.id.clone();
    validate_submission_media_support(
        model_id.as_str(),
        execution.route.protocol_route.protocol_profile_id.as_str(),
        &execution.model.capabilities,
        attachments,
    )?;
    let file_count = attachments.len();
    for (file_index, attachment) in attachments.iter().enumerate() {
        let media_kind = attachment_kind_text(attachment.kind.clone());
        let total_bytes = attachment.byte_len;
        emit_media_upload_progress(
            app,
            state,
            MediaUploadProgressChangedDelta {
                client_submission_id: client_submission_id.to_string(),
                conversation_id: conversation_id.map(str::to_string),
                provider_id: provider_id.clone(),
                model_id: model_id.clone(),
                file_name: attachment.file_name.clone(),
                media_kind: media_kind.to_string(),
                file_index,
                file_count,
                phase: MediaUploadPhase::Preparing,
                bytes_sent: 0,
                total_bytes,
                error: None,
            },
        );
        let progress_attachment = attachment.clone();
        let progress_provider_id = provider_id.clone();
        let progress_model_id = model_id.clone();
        let progress_conversation_id = conversation_id.map(str::to_string);
        let progress_client_submission_id = client_submission_id.to_string();
        let progress_media_kind = media_kind.to_string();
        let progress_state = state.clone();
        let progress_app = app.clone();
        let progress: MediaUploadProgressCallback = Arc::new(move |bytes_sent, total_bytes| {
            emit_media_upload_progress(
                &progress_app,
                &progress_state,
                MediaUploadProgressChangedDelta {
                    client_submission_id: progress_client_submission_id.clone(),
                    conversation_id: progress_conversation_id.clone(),
                    provider_id: progress_provider_id.clone(),
                    model_id: progress_model_id.clone(),
                    file_name: progress_attachment.file_name.clone(),
                    media_kind: progress_media_kind.clone(),
                    file_index,
                    file_count,
                    phase: MediaUploadPhase::Uploading,
                    bytes_sent,
                    total_bytes,
                    error: None,
                },
            );
        });
        let prepared = execution
            .client
            .prepare_local_media(
                &execution.route,
                model_id.as_str(),
                &execution.secret,
                std::path::Path::new(attachment.local_path.as_str()),
                attachment_kind(attachment.kind.clone()),
                attachment.mime_type.as_str(),
                Some(progress),
                cancellation,
            )
            .await;
        let prepared = match prepared {
            Ok(prepared) => prepared,
            Err(error) if cancellation.is_cancelled() || error == "media upload cancelled" => {
                emit_media_upload_progress(
                    app,
                    state,
                    MediaUploadProgressChangedDelta {
                        client_submission_id: client_submission_id.to_string(),
                        conversation_id: conversation_id.map(str::to_string),
                        provider_id: provider_id.clone(),
                        model_id: model_id.clone(),
                        file_name: attachment.file_name.clone(),
                        media_kind: media_kind.to_string(),
                        file_index,
                        file_count,
                        phase: MediaUploadPhase::Cancelled,
                        bytes_sent: 0,
                        total_bytes,
                        error: None,
                    },
                );
                return Err(String::from("submission_cancelled: media upload cancelled"));
            }
            Err(error) => {
                emit_media_upload_progress(
                    app,
                    state,
                    MediaUploadProgressChangedDelta {
                        client_submission_id: client_submission_id.to_string(),
                        conversation_id: conversation_id.map(str::to_string),
                        provider_id: provider_id.clone(),
                        model_id: model_id.clone(),
                        file_name: attachment.file_name.clone(),
                        media_kind: media_kind.to_string(),
                        file_index,
                        file_count,
                        phase: MediaUploadPhase::Failed,
                        bytes_sent: 0,
                        total_bytes,
                        error: Some(error.clone()),
                    },
                );
                return Err(error);
            }
        };
        if cancellation.is_cancelled() {
            emit_media_upload_progress(
                app,
                state,
                MediaUploadProgressChangedDelta {
                    client_submission_id: client_submission_id.to_string(),
                    conversation_id: conversation_id.map(str::to_string),
                    provider_id: provider_id.clone(),
                    model_id: model_id.clone(),
                    file_name: attachment.file_name.clone(),
                    media_kind: media_kind.to_string(),
                    file_index,
                    file_count,
                    phase: MediaUploadPhase::Cancelled,
                    bytes_sent: total_bytes.unwrap_or_default(),
                    total_bytes,
                    error: None,
                },
            );
            return Err(String::from("submission_cancelled: media upload cancelled"));
        }
        if let Some(prepared_url) = prepared {
            execution.client.remember_prepared_media(
                &execution.route,
                model_id.as_str(),
                std::path::Path::new(attachment.local_path.as_str()),
                attachment_kind(attachment.kind.clone()),
                attachment.mime_type.as_str(),
                prepared_url,
            );
        }
        emit_media_upload_progress(
            app,
            state,
            MediaUploadProgressChangedDelta {
                client_submission_id: client_submission_id.to_string(),
                conversation_id: conversation_id.map(str::to_string),
                provider_id: provider_id.clone(),
                model_id: model_id.clone(),
                file_name: attachment.file_name.clone(),
                media_kind: media_kind.to_string(),
                file_index,
                file_count,
                phase: MediaUploadPhase::Completed,
                bytes_sent: total_bytes.unwrap_or_default(),
                total_bytes,
                error: None,
            },
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
    attachments: &[ConversationAttachment],
) -> Result<(), String> {
    for attachment in attachments {
        let resource_kind = attachment_kind(attachment.kind.clone());
        let media_kind = attachment_kind_text(attachment.kind.clone());
        if !capabilities.input_resources.contains(&resource_kind) {
            return Err(format!(
                "Model `{model_id}` does not support `{media_kind}` attachments; media upload was not started."
            ));
        }
        if !vulcan_agent_runtime::provider_input_normalizer::protocol_profile_supports_attachment_kind(
            protocol_profile_id,
            attachment.kind.clone(),
        ) {
            return Err(format!(
                "Protocol profile `{protocol_profile_id}` does not support `{media_kind}` attachments; media upload was not started."
            ));
        }
    }
    Ok(())
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
    use super::{register_media_upload, remove_media_upload, validate_submission_media_support};
    use vulcan_ai_contract::{ModelCapabilities, ResourceKind};
    use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};

    /// Builds one attachment fixture for pre-upload capability validation.
    /// 构建一个用于上传前能力校验的附件夹具。
    fn attachment(kind: ConversationAttachmentKind) -> ConversationAttachment {
        ConversationAttachment {
            kind,
            file_name: String::from("media.bin"),
            local_path: String::from("media.bin"),
            source_path: None,
            asset_id: None,
            byte_len: Some(1),
            preview_local_path: None,
            mime_type: String::from("application/octet-stream"),
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
            &[attachment(ConversationAttachmentKind::Video)],
        )
        .expect("dedicated video profile should accept video");
    }
}
