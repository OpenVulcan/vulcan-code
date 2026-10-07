//! Observed pre-dispatch conversation uploads bound to one frozen execution and total deadline.
//! 绑定单份冻结执行及总截止时间的可观测分派前会话上传。

use super::ResolvedPublicAiExecution;
use super::media::{RuntimeMediaUploadProgressContext, RuntimeMediaUploadTerminalGuard};
use crate::{
    AdmittedLocalMediaSource, ConversationMediaUploadContract, LocalMediaMaterialization,
    MediaUploadCancellation, MediaUploadDeadline, MediaUploadFailure, MediaUploadProgressCallback,
    PreparedConversationMedia, RuntimeMediaUploadPhase, RuntimeMediaUploadProgressEvent,
    RuntimeMediaUploadProgressObserver,
};
use std::future::Future;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
#[cfg(test)]
use std::time::Duration;
use vulcan_ai_contract::ResourceKind;

/// Captures only telemetry facts absent from the route object; it does not own media origins or mutable selection.
/// 仅捕获路线对象缺失的遥测事实；不持有媒体原身份或可变选择。
pub(super) struct FrozenMediaUploadTelemetry {
    /// Existing service-installed observer, cloned at execution resolution.
    /// 在执行解析时克隆的既有服务安装观察器。
    pub(super) observer: Option<RuntimeMediaUploadProgressObserver>,
    /// Exact selection generation used to construct this execution.
    /// 构建此执行使用的精确选择代际。
    pub(super) manifest_generation: u64,
    /// Exact frozen selection route revision, never recomputed after an await.
    /// 精确冻结选择路线修订，绝不在等待之后重新计算。
    pub(super) route_revision: String,
}

impl ResolvedPublicAiExecution {
    /// Uploads admitted `source` of `kind` for `owner_session_id` through the exact already-selected `contract`.
    /// 为`owner_session_id`通过精确已选`contract`上传已准入`source`及其`kind`。
    /// The caller must finish policy and cache preflight first; `cancellation` belongs to the pending request.
    /// 调用方必须先完成策略及缓存预检；`cancellation`归待处理请求所有。
    /// `deadline` is the same budget that already covered source/cache preparation, not a newly started network timer.
    /// `deadline`是已经覆盖来源/缓存准备的相同预算，不是新启动的网络计时器。
    /// Returns verified prepared media or a safe failure, with credential/read/upload/response work inside one deadline.
    /// 返回已验证准备媒体或安全失败，凭据/读取/上传/响应工作位于同一截止时间内。
    pub async fn prepare_conversation_upload_until(
        &self,
        source: AdmittedLocalMediaSource<'_>,
        kind: ResourceKind,
        owner_session_id: &str,
        contract: &ConversationMediaUploadContract,
        cancellation: &MediaUploadCancellation,
        deadline: &MediaUploadDeadline,
    ) -> Result<PreparedConversationMedia, MediaUploadFailure> {
        if owner_session_id.trim().is_empty() {
            return Err(MediaUploadFailure::InvalidContract);
        }
        // Compare the entire declared contract before emitting progress or accessing credentials.
        // 发出进度或访问凭据之前比较完整声明契约。
        let declared = self
            .client
            .conversation_media_upload_contract(&self.route, kind, source.mime_type)
            .map_err(|_| MediaUploadFailure::InvalidContract)?;
        if declared.as_ref() != Some(contract) {
            return Err(MediaUploadFailure::InvalidContract);
        }
        deadline.validate_contract(contract)?;
        deadline.check(cancellation)?;
        // Each physical transfer has its own progress identity; cache hits never enter this function.
        // 每次物理传输拥有自己的进度身份；缓存命中绝不进入此函数。
        let progress_context = self
            .media_upload_telemetry
            .observer
            .as_ref()
            .map(|observer| RuntimeMediaUploadProgressContext {
                observer: Arc::clone(observer),
                base: RuntimeMediaUploadProgressEvent {
                    transfer_id: format!("media-conversation:{}", uuid::Uuid::new_v4()),
                    owner_session_id: owner_session_id.into(),
                    provider_id: self.route.provider.id.clone(),
                    service_entry_id: self.route.entry.id.clone(),
                    connection_id: self.route.connection.id.clone(),
                    model_id: self.model.id.clone(),
                    protocol_profile_id: self.route.protocol_route.protocol_profile_id.clone(),
                    manifest_revision: self.route.manifest_revision,
                    manifest_generation: self.media_upload_telemetry.manifest_generation,
                    route_revision: self.media_upload_telemetry.route_revision.clone(),
                    operation_id: crate::CONVERSATION_MEDIA_INPUT_OPERATION_ID.into(),
                    upload_channel_id: contract.upload_channel_id.clone(),
                    file_name: source.file_name.into(),
                    media_kind: match kind {
                        ResourceKind::Image => "image",
                        ResourceKind::Audio => "audio",
                        ResourceKind::Video => "video",
                        ResourceKind::File => "file",
                    }
                    .into(),
                    phase: RuntimeMediaUploadPhase::Preparing,
                    bytes_sent: 0,
                    total_bytes: Some(source.identity.byte_len),
                    error: None,
                },
                last_bytes_sent: Arc::new(AtomicU64::new(0)),
                dispatch_state: Arc::default(),
            });
        run_observed_upload_until(
            progress_context,
            cancellation,
            deadline,
            |progress, transfer_cancellation| async move {
                // Credential refresh is part of this same bounded upload, not a separate unbounded pre-step.
                // 凭据刷新属于相同有界上传，而非独立无界前置步骤。
                let secret = self
                    .credential
                    .resolve_for_request()
                    .await
                    .map_err(|_| MediaUploadFailure::CredentialUnavailable)?;
                // Only the caller-admitted provider-upload branch is executable; inline fallback is unavailable.
                // 仅调用方已准入的供应商上传分支可执行；不可回退内联。
                let prepared = self
                    .client
                    .prepare_local_media_typed(
                        &self.route,
                        &self.model.id,
                        &secret,
                        source,
                        kind,
                        false,
                        LocalMediaMaterialization::ProviderUpload {
                            source_byte_len: source.identity.byte_len,
                        },
                        progress,
                        &transfer_cancellation,
                    )
                    .await?;
                if prepared.source_identity() != *source.identity {
                    return Err(MediaUploadFailure::SourceChanged);
                }
                // Missing or contract-invalid references are failures before the Completed event.
                // 缺少引用或引用违反契约时，在Completed事件之前判定失败。
                let reference = prepared
                    .prepared_reference()
                    .ok_or(MediaUploadFailure::MissingReference)?;
                self.client
                    .validate_conversation_media_reference(
                        &self.route,
                        kind,
                        source.mime_type,
                        reference,
                    )
                    .map_err(|_| MediaUploadFailure::InvalidReference)?;
                Ok(prepared)
            },
        )
        .await
    }
}

/// Runs `prepare` with a byte callback and transfer-local token inside the original `deadline` lifecycle.
/// 在原`deadline`生命周期内使用字节回调及传输局部令牌运行`prepare`。
/// `context` is optional UI telemetry; parent `cancellation` stops this transfer without being cancelled by its timeout.
/// `context`是可选UI遥测；父`cancellation`停止此传输，但不会因本传输超时而被取消。
/// `deadline` retains the exact timeout owner independently of whether a UI observer exists.
/// `deadline`保留精确超时所有者，不依赖是否存在UI观察器。
/// Returns one prepared value or error and emits exactly one terminal phase after Preparing, including future drop.
/// 返回一个准备值或错误；Preparing之后恰好发出一次终态，包括Future丢弃情况。
async fn run_observed_upload_until<T, F, Fut>(
    context: Option<RuntimeMediaUploadProgressContext>,
    cancellation: &MediaUploadCancellation,
    deadline: &MediaUploadDeadline,
    prepare: F,
) -> Result<T, MediaUploadFailure>
where
    F: FnOnce(Option<MediaUploadProgressCallback>, MediaUploadCancellation) -> Fut,
    Fut: Future<Output = Result<T, MediaUploadFailure>>,
{
    deadline.check(cancellation)?;
    // A failed or timed-out file cannot cancel all subsequent files in its parent request.
    // 失败或超时文件不能取消父请求中的全部后续文件。
    let transfer_cancellation = MediaUploadCancellation::new();
    // The existing terminal guard also cancels background source reads when the future is dropped.
    // 既有终态守卫也会在Future丢弃时取消后台来源读取。
    let mut terminal =
        RuntimeMediaUploadTerminalGuard::new_cancellable(context.as_ref(), &transfer_cancellation);
    if let Some(context) = context.as_ref() {
        context.emit(
            RuntimeMediaUploadPhase::Preparing,
            0,
            context.base.total_bytes,
            None,
        );
    }
    // Only the actual upload engine reports byte progress; hashing and cache hits do not use this callback.
    // 只有实际上传引擎报告字节进度；哈希及缓存命中不使用此回调。
    let progress = context
        .as_ref()
        .map(RuntimeMediaUploadProgressContext::byte_callback);
    // Parent cancellation has priority over simultaneous completion, preventing false successful cancellation.
    // 父取消优先于同时完成，防止取消被误报成功。
    let result = deadline
        .wait(cancellation, async {
            prepare(progress, transfer_cancellation.clone()).await
        })
        .await
        .and_then(|result| result);
    transfer_cancellation.cancel();
    if let Some(context) = context.as_ref() {
        match &result {
            Ok(_) => context.emit(
                RuntimeMediaUploadPhase::Completed,
                context.base.total_bytes.unwrap_or_default(),
                context.base.total_bytes,
                None,
            ),
            Err(error) => {
                // Only the actual parent cancellation produces Cancelled; timeout remains an explicit failure.
                // 只有实际父取消产生Cancelled；超时仍是明确失败。
                let phase = if cancellation.is_cancelled() {
                    RuntimeMediaUploadPhase::Cancelled
                } else {
                    RuntimeMediaUploadPhase::Failed
                };
                context.emit(
                    phase,
                    context.last_bytes_sent.load(Ordering::Acquire),
                    context.base.total_bytes,
                    (phase == RuntimeMediaUploadPhase::Failed).then(|| error.to_string()),
                );
            }
        }
    }
    terminal.disarm();
    result
}

/// Preserves the original lifecycle test entrypoint for exact `channel_id`/`timeout_ms` while exercising the shared production deadline.
/// 为精确`channel_id`/`timeout_ms`保留原生命周期测试入口，同时覆盖共享生产截止时间。
/// `context`, `cancellation` and `prepare` retain their production lifecycle semantics and return the same typed result.
/// `context`、`cancellation`及`prepare`保持生产生命周期语义并返回相同强类型结果。
#[cfg(test)]
async fn run_observed_upload<T, F, Fut>(
    context: Option<RuntimeMediaUploadProgressContext>,
    cancellation: &MediaUploadCancellation,
    channel_id: &str,
    timeout_ms: u64,
    prepare: F,
) -> Result<T, MediaUploadFailure>
where
    F: FnOnce(Option<MediaUploadProgressCallback>, MediaUploadCancellation) -> Fut,
    Fut: Future<Output = Result<T, MediaUploadFailure>>,
{
    // Construct the exact same bounded deadline used by the public frozen-contract entrypoint.
    // 构造公开冻结契约入口使用的完全相同有界截止时间。
    let deadline = MediaUploadDeadline::for_channel(channel_id, timeout_ms)?;
    run_observed_upload_until(context, cancellation, &deadline, prepare).await
}

#[cfg(test)]
mod tests;
