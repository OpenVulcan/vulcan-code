//! Local-media preparation, provider-tool upload preflight, and progress lifecycles.
//! 本地媒体准备、供应商工具上传预检与进度生命周期。

use super::tool_targets::ResolvedProviderToolTarget;
use super::{PublicAiRuntime, ResolvedPublicAiSelection, current_unix_millis};
use crate::{
    PersistedAiConfiguration, ProviderExecutionClient, ProviderToolCapabilityKind,
    ProviderToolTargetSelection,
};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use uuid::Uuid;
use vulcan_ai_contract::ResourceKind;

/// Reports whether one media-understanding source must use an available provider upload channel.
/// 判断一个媒体理解源是否必须使用可用的 Provider 上传通道。
///
/// # Parameters
/// 参数
/// `kind` is the exact media modality, `upload_available` reflects the frozen route contract, and
/// `source_byte_len` is the immutable source length observed before preparation.
/// `kind` 是精确媒体模态，`upload_available` 反映冻结路由契约，`source_byte_len` 是准备前观察到的不可变源长度。
///
/// # Returns
/// 返回值
/// `true` for every upload-capable video and for upload-capable image/audio sources above the shared inline ceiling.
/// 对全部具备上传能力的视频，以及超过共享内联上限且具备上传能力的图片/音频返回 `true`。
fn media_understanding_uses_provider_upload(
    kind: ResourceKind,
    upload_available: bool,
    source_byte_len: u64,
) -> bool {
    upload_available
        && (kind == ResourceKind::Video
            || source_byte_len > crate::PREFERRED_INLINE_MEDIA_MAX_BYTES)
}

/// One immutable media reference prepared for a model-replacement understanding request.
/// 为模型代替理解请求准备的一条不可变媒体引用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedLocalMediaReference {
    /// Exact request reference returned by inline preparation or the Provider upload channel.
    /// 由内联准备或供应商上传通道返回的精确请求引用。
    pub reference: String,
    /// Provider-declared encoding for uploaded references; `None` identifies an inline Data URL.
    /// 已上传引用使用供应商声明的编码；`None` 标识内联 Data URL。
    pub reference_encoding: Option<crate::UploadReferenceEncoding>,
}

/// Carries the complete immutable admission for one local-media preparation.
/// 携带一次本地媒体准备所需的完整不可变准入信息。
pub struct PrepareLocalMediaRequest<'a> {
    /// Exact resolved Provider selection used for preparation and caching.
    /// 用于准备与缓存的精确已解析供应商选择。
    pub selection: &'a ResolvedPublicAiSelection,
    /// Optional owning session required by session-scoped upload channels.
    /// 会话作用域上传通道要求的可选所有会话。
    pub owner_session_id: Option<&'a str>,
    /// Stable host operation identity used for progress projection.
    /// 用于进度投影的稳定宿主操作身份。
    pub operation_id: &'a str,
    /// Exact local source path read during preparation.
    /// 准备期间读取的精确本地源路径。
    pub local_path: &'a Path,
    /// Closed source resource kind.
    /// 封闭源资源类型。
    pub kind: ResourceKind,
    /// Canonical source MIME type.
    /// 规范源 MIME 类型。
    pub mime_type: &'a str,
    /// Optional transfer-progress callback.
    /// 可选传输进度回调。
    pub progress: Option<crate::MediaUploadProgressCallback>,
    /// Cooperative cancellation owner for the complete preparation.
    /// 完整准备过程使用的协作取消所有者。
    pub cancellation: &'a crate::MediaUploadCancellation,
}

/// Immutable emitter context for one media upload performed after request dispatch.
/// 请求分派后执行单次媒体上传时使用的不可变发射上下文。
#[derive(Clone)]
pub(super) struct RuntimeMediaUploadProgressContext {
    /// Host callback installed by the service boundary.
    /// 服务边界安装的宿主回调。
    observer: crate::RuntimeMediaUploadProgressObserver,
    /// Stable non-secret event fields reused by every lifecycle phase.
    /// 每个生命周期阶段复用的稳定非秘密事件字段。
    pub(super) base: crate::RuntimeMediaUploadProgressEvent,
    /// Latest transferred byte count retained for cancellation and failure terminal events.
    /// 为取消与失败终态事件保留的最近已传输字节数。
    pub(super) last_bytes_sent: Arc<AtomicU64>,
}

impl RuntimeMediaUploadProgressContext {
    /// Emits one phase replacement for this exact transfer.
    /// 为当前精确传输发出一次阶段替换。
    ///
    /// # Parameters
    /// 参数
    /// `phase`, `bytes_sent`, `total_bytes`, and `error` are the current transfer facts.
    /// `phase`、`bytes_sent`、`total_bytes` 与 `error` 是当前传输事实。
    pub(super) fn emit(
        &self,
        phase: crate::RuntimeMediaUploadPhase,
        bytes_sent: u64,
        total_bytes: Option<u64>,
        error: Option<String>,
    ) {
        self.last_bytes_sent.store(bytes_sent, Ordering::Release);
        // event preserves stable route identity while replacing only lifecycle facts.
        // event 保留稳定路由身份，仅替换生命周期事实。
        let mut event = self.base.clone();
        event.phase = phase;
        event.bytes_sent = bytes_sent;
        event.total_bytes = total_bytes;
        event.error = error;
        (self.observer)(event);
    }

    /// Builds the byte callback consumed by the generic upload engine.
    /// 构建通用上传引擎消费的字节回调。
    ///
    /// # Returns
    /// 返回值
    /// A cloneable callback that emits uploading replacements for this transfer.
    /// 返回一个可克隆回调，为当前传输发出上传中替换事件。
    pub(super) fn byte_callback(&self) -> crate::MediaUploadProgressCallback {
        // context remains immutable and can safely cross the asynchronous upload boundary.
        // context 保持不可变，可安全跨越异步上传边界。
        let context = self.clone();
        Arc::new(move |bytes_sent, total_bytes| {
            context.emit(
                crate::RuntimeMediaUploadPhase::Uploading,
                bytes_sent,
                total_bytes,
                None,
            );
        })
    }
}

/// Emits a cancelled terminal event when an in-flight media upload future is dropped.
/// 当进行中的媒体上传 Future 被丢弃时发出取消终态事件。
pub(super) struct RuntimeMediaUploadTerminalGuard {
    /// Exact progress context retained until a normal terminal phase is emitted.
    /// 保留到正常终态发出为止的精确进度上下文。
    context: Option<RuntimeMediaUploadProgressContext>,
}

impl RuntimeMediaUploadTerminalGuard {
    /// Creates one armed terminal guard from an optional user-visible progress context.
    /// 从可选的用户可见进度上下文创建一个已启用终态守卫。
    ///
    /// # Parameters
    /// 参数
    /// `context` is the exact transfer whose future lifetime is guarded.
    /// `context` 是其 Future 生命周期被守护的精确传输。
    ///
    /// # Returns
    /// 返回值
    /// An armed guard when progress is visible, or an inert guard otherwise.
    /// 进度可见时返回已启用守卫，否则返回惰性守卫。
    pub(super) fn new(context: Option<&RuntimeMediaUploadProgressContext>) -> Self {
        Self {
            context: context.cloned(),
        }
    }

    /// Disarms the drop fallback after a normal terminal event has been emitted.
    /// 在正常终态事件发出后解除丢弃回退。
    pub(super) fn disarm(&mut self) {
        self.context = None;
    }
}

impl Drop for RuntimeMediaUploadTerminalGuard {
    /// Emits cancellation with the latest observed byte count when still armed.
    /// 仍启用时使用最近观测字节数发出取消事件。
    fn drop(&mut self) {
        let Some(context) = self.context.take() else {
            return;
        };
        context.emit(
            crate::RuntimeMediaUploadPhase::Cancelled,
            context.last_bytes_sent.load(Ordering::Acquire),
            context.base.total_bytes,
            None,
        );
    }
}

impl PublicAiRuntime {
    /// Installs or clears the host observer for post-dispatch media upload progress.
    /// 安装或清除分派后媒体上传进度的宿主观察器。
    ///
    /// # Parameters
    /// 参数
    /// `observer` is the process-local callback owned by the embedding service.
    /// `observer` 是嵌入服务持有的进程内回调。
    ///
    /// # Returns
    /// 返回值
    /// Success after atomically replacing the observer, or a controlled poisoned-lock error.
    /// 原子替换观察器后返回成功，或返回受控的锁中毒错误。
    pub fn set_runtime_media_upload_progress_observer(
        &self,
        observer: Option<crate::RuntimeMediaUploadProgressObserver>,
    ) -> Result<(), String> {
        let mut current = self
            .runtime_media_upload_progress_observer
            .write()
            .map_err(|_| String::from("The runtime media upload observer lock is poisoned."))?;
        *current = observer;
        Ok(())
    }

    /// Prepares and caches one local media input for an exact resolved conversation route.
    /// 为一条精确解析后的会话路由准备并缓存一个本地媒体输入。
    ///
    /// # Parameters
    /// 参数
    /// `selection` freezes the route; `owner_session_id` and `operation_id` route host progress;
    /// `local_path`, `kind`, and `mime_type` identify the source; `progress` and `cancellation`
    /// control the observable transfer lifecycle.
    /// `selection` 冻结路由；`owner_session_id` 与 `operation_id` 路由宿主进度；
    /// `local_path`、`kind` 与 `mime_type` 标识来源；`progress` 与 `cancellation` 控制可观测传输生命周期。
    ///
    /// # Returns
    /// 返回值
    /// The immutable reference plus its exact Provider encoding, or an inline Data URL marker.
    /// 返回不可变引用及其精确供应商编码，或内联 Data URL 标记。
    pub async fn prepare_and_remember_local_media(
        &self,
        request: PrepareLocalMediaRequest<'_>,
    ) -> Result<PreparedLocalMediaReference, String> {
        let PrepareLocalMediaRequest {
            selection,
            owner_session_id,
            operation_id,
            local_path,
            kind,
            mime_type,
            progress,
            cancellation,
        } = request;
        if cancellation.is_cancelled() {
            return Err(String::from("media upload cancelled"));
        }
        // uploadContract is resolved for every media kind so future image and audio upload channels
        // use the same cache, progress, cancellation, and route isolation as video.
        // uploadContract 会为每种媒体解析，使未来图片与音频上传通道复用视频相同的缓存、进度、取消和路由隔离。
        let upload_contract = self.execution_client.conversation_media_upload_contract(
            &selection.route,
            kind,
            mime_type,
        )?;
        if kind == ResourceKind::Video && upload_contract.is_none() {
            return Err(format!(
                "Video resource `{}` has no declared provider upload channel; refusing Base64 fallback.",
                local_path.display()
            ));
        }
        // sourceByteLen freezes the policy decision before cache lookup and physical preparation.
        // sourceByteLen 在缓存查询与真实准备前冻结策略判定。
        let source_byte_len = tokio::fs::metadata(local_path)
            .await
            .map_err(|error| {
                format!(
                    "Media-understanding source `{}` could not be inspected: {error}",
                    local_path.display()
                )
            })?
            .len();
        // useProviderUpload keeps small image/audio inputs inline while requiring video uploads and
        // selecting any declared media channel above the shared inline-preference threshold.
        // useProviderUpload 让小型图片/音频保持内联，同时强制视频上传，并在超过共享内联阈值时选择任意已声明媒体通道。
        let use_provider_upload = media_understanding_uses_provider_upload(
            kind,
            upload_contract.is_some(),
            source_byte_len,
        );
        if use_provider_upload {
            let contract = upload_contract.as_ref().ok_or_else(|| {
                String::from("Media-understanding upload policy selected no provider channel.")
            })?;
            if contract.reference_scope == crate::UploadReferenceScope::Session
                && owner_session_id.is_none()
            {
                return Err(format!(
                    "Upload channel `{}` requires a session identity before media preparation.",
                    contract.upload_channel_id
                ));
            }
        }
        // cachedReference proves the exact source bytes already have a live provider reference;
        // cachedReference 证明精确源字节已经拥有仍有效的供应商引用；
        // a cache hit must not create a synthetic upload-progress row.
        // 命中缓存时不得创建伪造的上传进度行。
        if use_provider_upload
            && let Some(cached_reference) = self
                .execution_client
                .prepared_media_reference(
                    &selection.route,
                    local_path,
                    kind,
                    mime_type,
                    owner_session_id,
                    Some(cancellation),
                )
                .await
        {
            return Ok(PreparedLocalMediaReference {
                reference: cached_reference.provider_reference,
                reference_encoding: Some(cached_reference.reference_encoding),
            });
        }
        // observer is cloned before any callback invocation to avoid retaining the registry lock.
        // observer 在任何回调调用前完成克隆，避免占用注册表锁。
        let observer = self
            .runtime_media_upload_progress_observer
            .read()
            .map_err(|_| String::from("The runtime media upload observer lock is poisoned."))?
            .clone();
        // progressContext exists only for a real provider upload; Base64 image and audio
        // materialization must not look like a network transfer in the UI.
        // progressContext 仅在真实供应商上传时存在；图片与音频 Base64 物化不得在界面上伪装成网络传输。
        let progress_context = if use_provider_upload {
            if let (Some(observer), Some(owner_session_id)) = (observer, owner_session_id) {
                // mediaKind is the stable UI-facing category for this provider-neutral resource.
                // mediaKind 是当前供应商中立资源面向界面的稳定类别。
                let media_kind = match kind {
                    ResourceKind::Image => "image",
                    ResourceKind::Video => "video",
                    ResourceKind::Audio => "audio",
                    ResourceKind::File => {
                        return Err(String::from(
                            "Model-replacement media progress does not accept generic files.",
                        ));
                    }
                };
                // uploadChannelId is resolved by the same route function used by media preparation.
                // uploadChannelId 由媒体准备使用的同一路由函数解析。
                let upload_channel_id = upload_contract
                    .as_ref()
                    .map(|contract| contract.upload_channel_id.clone())
                    .ok_or_else(|| {
                        String::from(
                            "Media-understanding upload policy selected no provider channel.",
                        )
                    })?;
                // totalBytes reuses the policy snapshot and is rechecked by physical preparation.
                // totalBytes 复用策略快照，并由真实准备再次校验。
                let total_bytes = source_byte_len;
                // fileName is derived only from the canonical workspace path.
                // fileName 仅从规范工作区路径派生。
                let file_name = local_path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        String::from("Media-understanding source has no valid file name.")
                    })?;
                Some(RuntimeMediaUploadProgressContext {
                    observer,
                    base: crate::RuntimeMediaUploadProgressEvent {
                        transfer_id: format!("media-tool:{}", Uuid::new_v4()),
                        owner_session_id: owner_session_id.to_string(),
                        provider_id: selection.route.provider.id.clone(),
                        service_entry_id: selection.route.entry.id.clone(),
                        connection_id: selection.route.connection.id.clone(),
                        model_id: selection.model.id.clone(),
                        protocol_profile_id: selection
                            .route
                            .protocol_route
                            .protocol_profile_id
                            .clone(),
                        manifest_revision: selection.route.manifest_revision,
                        manifest_generation: selection.manifest_generation,
                        route_revision: selection.route_revision.clone(),
                        operation_id: operation_id.to_string(),
                        upload_channel_id,
                        file_name: file_name.to_string(),
                        media_kind: media_kind.to_string(),
                        phase: crate::RuntimeMediaUploadPhase::Preparing,
                        bytes_sent: 0,
                        total_bytes: Some(total_bytes),
                        error: None,
                    },
                    last_bytes_sent: Arc::new(AtomicU64::new(0)),
                })
            } else {
                None
            }
        } else {
            None
        };
        if let Some(context) = progress_context.as_ref() {
            context.emit(
                crate::RuntimeMediaUploadPhase::Preparing,
                0,
                context.base.total_bytes,
                None,
            );
        }
        // progressTerminalGuard closes the UI row if this preparation future is cancelled.
        // progressTerminalGuard 在当前准备 Future 被取消时关闭界面进度行。
        let mut progress_terminal_guard =
            RuntimeMediaUploadTerminalGuard::new(progress_context.as_ref());
        // observedProgress composes a caller callback with the service-owned progress observer.
        // observedProgress 组合调用方回调与服务持有的进度观察器。
        let observed_progress = if use_provider_upload {
            match (progress, progress_context.as_ref()) {
                (Some(progress), Some(context)) => {
                    let observed = context.byte_callback();
                    Some(Arc::new(move |bytes_sent, total_bytes| {
                        progress(bytes_sent, total_bytes);
                        observed(bytes_sent, total_bytes);
                    }) as crate::MediaUploadProgressCallback)
                }
                (Some(progress), None) => Some(progress),
                (None, Some(context)) => Some(context.byte_callback()),
                (None, None) => None,
            }
        } else {
            None
        };
        // preparationResult keeps all terminal errors available for a failed progress event.
        // preparationResult 保留全部终态错误，以便发出失败进度事件。
        let preparation_result = async {
            // secret is the latest credential revision for the already frozen account route.
            // secret 是已冻结账号路由的最新凭据修订版。
            let secret = self
                .resolve_fresh_execution_secret(
                    selection.route.connection.id.as_str(),
                    selection.route.connection.secret_reference.as_str(),
                )
                .await?;
            // prepared binds any native provider reference to the verified local source fingerprint.
            // prepared 将任意供应商原生引用绑定到已验证本地来源指纹。
            self.execution_client
                .prepare_local_media(
                    &selection.route,
                    selection.model.id.as_str(),
                    &secret,
                    local_path,
                    kind,
                    mime_type,
                    true,
                    use_provider_upload,
                    observed_progress,
                    cancellation,
                )
                .await
        }
        .await;
        let prepared = match preparation_result {
            Ok(prepared) => prepared,
            Err(error) => {
                if let Some(context) = progress_context.as_ref() {
                    // terminalPhase distinguishes cooperative cancellation from provider failure.
                    // terminalPhase 区分协作式取消与供应商失败。
                    let terminal_phase = if cancellation.is_cancelled() {
                        crate::RuntimeMediaUploadPhase::Cancelled
                    } else {
                        crate::RuntimeMediaUploadPhase::Failed
                    };
                    context.emit(
                        terminal_phase,
                        context.last_bytes_sent.load(Ordering::Acquire),
                        context.base.total_bytes,
                        (terminal_phase == crate::RuntimeMediaUploadPhase::Failed)
                            .then(|| error.clone()),
                    );
                }
                progress_terminal_guard.disarm();
                return Err(error);
            }
        };
        // preparedReference binds this physical invocation directly to the verified bytes.
        // preparedReference 将本次物理调用直接绑定到已验证字节。
        let prepared_reference = prepared
            .prepared_reference()
            .ok_or_else(|| {
                String::from("Media-understanding preparation did not produce a request reference.")
            })?
            .to_string();
        // referenceEncoding preserves the exact wire channel for uploaded media and distinguishes
        // host-inline Data URLs without guessing from the reference string.
        // referenceEncoding 为已上传媒体保留精确 Wire 通道，并在不猜测引用字符串的情况下区分宿主内联 Data URL。
        let reference_encoding = if use_provider_upload {
            Some(
                upload_contract
                    .as_ref()
                    .ok_or_else(|| {
                        String::from(
                            "Media-understanding upload completed without its frozen contract.",
                        )
                    })?
                    .reference_encoding,
            )
        } else {
            None
        };
        if use_provider_upload {
            self.execution_client.remember_prepared_media(
                &selection.route,
                local_path,
                kind,
                mime_type,
                prepared,
                owner_session_id,
            );
        }
        if let Some(context) = progress_context.as_ref() {
            context.emit(
                crate::RuntimeMediaUploadPhase::Completed,
                context.base.total_bytes.unwrap_or_default(),
                context.base.total_bytes,
                None,
            );
        }
        progress_terminal_guard.disarm();
        Ok(PreparedLocalMediaReference {
            reference: prepared_reference,
            reference_encoding,
        })
    }
}

/// Maps one fixed understanding capability to its exact provider-neutral media modality.
/// 将一个固定理解能力映射为精确的供应商无关媒体模态。
///
/// # Parameters
/// 参数
/// `capability` is the immutable provider-tool capability selected for this invocation.
/// `capability` 是本次调用选定的不可变供应商工具能力。
///
/// # Returns
/// 返回值
/// The resource kind and stable UI media label for image, video, or audio understanding; non-media
/// capabilities return `None` and cannot enter upload preflight.
/// 图片、视频或音频理解返回资源类型与稳定界面媒体标签；非媒体能力返回 `None`，不能进入上传预检。
pub(super) fn provider_tool_media_resource_kind(
    capability: ProviderToolCapabilityKind,
) -> Option<(ResourceKind, &'static str)> {
    match capability {
        ProviderToolCapabilityKind::ImageUnderstand => Some((ResourceKind::Image, "image")),
        ProviderToolCapabilityKind::VideoUnderstand => Some((ResourceKind::Video, "video")),
        ProviderToolCapabilityKind::AudioUnderstand => Some((ResourceKind::Audio, "audio")),
        _ => None,
    }
}

/// Carries one local understanding-media upload preflight for the process-local cache.
/// 携带一次模型扩展进程级理解媒体缓存所需的本地上传预检结果。
#[derive(Debug, Clone)]
pub(super) struct ProviderToolMediaUploadPreflight {
    /// Exact scalar binding that maps the fixed media facade into the provider operation.
    /// 将固定媒体外壳映射到供应商操作的精确标量绑定。
    pub(super) binding: crate::OperationUploadBinding,
    /// Validated resource and upload channel selected from the frozen manifest.
    /// 从冻结 Manifest 选出的已校验资源与上传通道。
    pub(super) inspection: crate::upload_engine::OperationUploadRouteInspection,
    /// Source bytes identity captured before the network upload.
    /// 网络上传前捕获的源字节身份。
    pub(super) source_identity: crate::PreparedMediaSourceIdentity,
    /// Exact media modality used by the provider-reference cache key.
    /// 供应商引用缓存键使用的精确媒体模态。
    pub(super) resource_kind: ResourceKind,
    /// Wall-clock timestamp that anchors the one-day reuse window.
    /// 锚定一天复用窗口的墙钟首次使用时间。
    pub(super) first_used_at_unix_millis: u64,
}

/// Reports whether one media upload binding can participate in the scalar process-local cache.
/// 报告一条媒体上传绑定是否可以参与进程级标量缓存。
///
/// # Parameters
/// 参数
/// `binding` is the exact frozen operation upload contract.
/// `binding` 是精确冻结的操作上传契约。
///
/// # Returns
/// 返回值
/// `true` only for one scalar resource binding; collection bindings remain on the generic uploader.
/// 仅对单资源标量绑定返回 `true`；集合绑定继续交由通用上传器处理。
fn provider_tool_media_upload_binding_is_cacheable(
    binding: &crate::OperationUploadBinding,
) -> bool {
    binding.item_source_path.is_none() && binding.item_target_path.is_none()
}

/// Preflights one fixed media-understanding call for bounded process-local reference reuse.
/// 为一次固定媒体理解调用预检有界的进程级引用复用。
///
/// The model-extension tool has no conversation media ledger. This cache is deliberately scoped
/// only by the provider upload contract and source bytes, so a later independent tool call can
/// reuse a still-valid provider object without pretending that a session context exists.
/// 模型扩展工具没有会话媒体账本。本缓存只按供应商上传契约与源字节作用域，后续独立工具调用
/// 可以复用仍有效的供应商对象，但不会伪造会话上下文。
pub(super) async fn prepare_provider_tool_media_upload(
    execution_client: &ProviderExecutionClient,
    resolved: &ResolvedProviderToolTarget,
    operation: &crate::OperationDeclaration,
    arguments: &serde_json::Value,
    credential_profile: &crate::CredentialProfile,
    workspace_root: Option<&Path>,
    capability: ProviderToolCapabilityKind,
) -> Result<
    Option<(
        ProviderToolMediaUploadPreflight,
        Option<crate::PreparedMediaReferenceSnapshot>,
    )>,
    String,
> {
    let (resource_kind, _media_label) = provider_tool_media_resource_kind(capability)
        .ok_or_else(|| format!("Capability `{capability:?}` is not media understanding."))?;
    let first_used_at_unix_millis = current_unix_millis()?;
    let projection = resolved
        .target
        .contract
        .media_input_projection
        .as_ref()
        .ok_or_else(|| {
            format!(
                "Media-understanding target `{}` has no input projection.",
                resolved.target.target_id
            )
        })?;
    let source_pointer = format!("/{}", projection.path_field);
    let mut matching_bindings = operation
        .upload_bindings
        .iter()
        .filter(|binding| binding.source_path == source_pointer);
    let binding = matching_bindings.next().ok_or_else(|| {
        format!(
            "Media-understanding operation `{}` has no upload binding for `{source_pointer}`.",
            operation.operation_id
        )
    })?;
    if matching_bindings.next().is_some() {
        return Err(format!(
            "Media-understanding operation `{}` has ambiguous upload bindings for `{source_pointer}`.",
            operation.operation_id
        ));
    }
    // Collection bindings remain owned by the generic upload materializer. The process-local
    // understanding cache has a one-resource scalar key and must not narrow valid array contracts.
    // 集合绑定继续由通用上传物化器持有。进程级理解缓存使用单资源标量键，不得收窄有效数组契约。
    if !provider_tool_media_upload_binding_is_cacheable(binding) {
        return Ok(None);
    }
    let source_reference = arguments
        .pointer(source_pointer.as_str())
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            format!(
                "Media-understanding upload source `{source_pointer}` must be one non-empty local path."
            )
        })?;
    let inspection = crate::upload_engine::inspect_operation_upload_route(
        binding,
        source_reference,
        1,
        workspace_root,
        &resolved.upload_channels,
        credential_profile.credential_role.as_deref(),
    )
    .await?;
    if matches!(
        inspection.channel.primitive,
        Some(crate::UploadPrimitiveKind::InlineDataUrl | crate::UploadPrimitiveKind::InlineBase64)
    ) {
        if resource_kind == ResourceKind::Video {
            return Err(String::from(
                "Video-understanding tool uploads must use a provider resource channel; Base64 is forbidden.",
            ));
        }
        return Ok(None);
    }
    let Some(local_path) = inspection.resource.local_path.as_ref() else {
        return Ok(None);
    };
    let source_identity = execution_client
        .read_media_source_identity(local_path, None)
        .await?;
    let cached_reference = execution_client
        .prepared_provider_tool_media_reference(
            resolved.connection.provider_id.as_str(),
            resolved.entry.id.as_str(),
            resolved.connection.id.as_str(),
            &inspection.channel,
            local_path,
            resource_kind,
            inspection.resource.mime_type.as_str(),
            None,
        )
        .await?;
    Ok(Some((
        ProviderToolMediaUploadPreflight {
            binding: binding.clone(),
            inspection,
            source_identity,
            resource_kind,
            first_used_at_unix_millis,
        },
        cached_reference,
    )))
}

/// Extracts one scalar provider reference written by a fixed operation upload binding.
/// 提取固定操作上传绑定写入的一个标量供应商引用。
pub(super) fn provider_tool_materialized_reference(
    arguments: &serde_json::Value,
    binding: &crate::OperationUploadBinding,
) -> Result<String, String> {
    if binding.item_source_path.is_some() || binding.item_target_path.is_some() {
        return Err(String::from(
            "Media-understanding upload cache requires a scalar upload binding.",
        ));
    }
    arguments
        .pointer(&binding.target_path)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            format!(
                "Media-understanding upload target `{}` did not contain one scalar provider reference.",
                binding.target_path
            )
        })
}

/// Builds an exact progress emitter for one provider-tool media upload.
/// 为一次供应商工具媒体上传构建精确进度发射器。
///
/// # Parameters
/// 参数
/// The observer and owner opt into reporting; capability, route, operation, arguments, credential
/// profile, and workspace are the same frozen inputs consumed by upload materialization.
/// 观察器与所有者用于启用报告；能力、路由、操作、参数、凭据配置档与工作区均与上传物化消费的冻结输入一致。
///
/// # Returns
/// 返回值
/// `None` when the observer, owner, or reporting upload channel is absent; otherwise the preflighted context.
/// 观察器、所有者或可报告进度的上传通道缺失时返回 `None`；否则返回已完成预检的上下文。
#[allow(clippy::too_many_arguments)]
pub(super) async fn provider_tool_media_upload_progress_context(
    observer: Option<crate::RuntimeMediaUploadProgressObserver>,
    owner_session_id: Option<&str>,
    capability: ProviderToolCapabilityKind,
    resolved: &ResolvedProviderToolTarget,
    operation: &crate::OperationDeclaration,
    arguments: &serde_json::Value,
    credential_profile: &crate::CredentialProfile,
    workspace_root: Option<&Path>,
) -> Result<Option<RuntimeMediaUploadProgressContext>, String> {
    // observer and owner are both required to route a user-visible progress replacement.
    // observer 与 owner 必须同时存在，才能路由面向用户的进度替换事件。
    let (Some(observer), Some(owner_session_id)) = (observer, owner_session_id) else {
        return Ok(None);
    };
    let Some((_resource_kind, media_kind)) = provider_tool_media_resource_kind(capability) else {
        return Ok(None);
    };
    // projection identifies the exact provider-native source field generated from the facade.
    // projection 标识由固定外壳生成的精确供应商原生来源字段。
    let projection = resolved
        .target
        .contract
        .media_input_projection
        .as_ref()
        .ok_or_else(|| {
            format!(
                "Media-understanding target `{}` has no input projection for upload progress.",
                resolved.target.target_id
            )
        })?;
    // sourcePointer is the authoritative operation binding path for the projected media field.
    // sourcePointer 是投影媒体字段的权威操作绑定路径。
    let source_pointer = format!("/{}", projection.path_field);
    // binding must be unique because one fixed media invocation owns exactly one input file.
    // binding 必须唯一，因为一次固定媒体调用只持有一个输入文件。
    let mut matching_bindings = operation
        .upload_bindings
        .iter()
        .filter(|binding| binding.source_path == source_pointer);
    let binding = matching_bindings.next().ok_or_else(|| {
        format!(
            "Media-understanding operation `{}` has no upload binding for `{source_pointer}`.",
            operation.operation_id
        )
    })?;
    if matching_bindings.next().is_some() {
        return Err(format!(
            "Media-understanding operation `{}` has ambiguous upload bindings for `{source_pointer}`.",
            operation.operation_id
        ));
    }
    // sourceValue is the exact native argument preserved until upload materialization.
    // sourceValue 是保留到上传物化阶段的精确原生参数。
    let source_value = arguments.pointer(source_pointer.as_str()).ok_or_else(|| {
        format!(
            "Media-understanding upload source `{source_pointer}` is missing after argument projection."
        )
    })?;
    // sourceReference and itemCount retain the scalar versus explicit one-item-array contract.
    // sourceReference 与 itemCount 保留标量与显式单项数组契约。
    let (source_reference, item_count) = match source_value {
        serde_json::Value::String(reference) if !reference.trim().is_empty() => {
            (reference.as_str(), 1_usize)
        }
        serde_json::Value::Array(values) if values.len() == 1 => {
            let reference = values[0]
                .as_str()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    String::from(
                        "Media-understanding upload array must contain one non-empty local path.",
                    )
                })?;
            (reference, values.len())
        }
        _ => {
            return Err(String::from(
                "Media-understanding upload source must be one non-empty local path.",
            ));
        }
    };
    // uploadRoute repeats the generic engine preflight and suppresses progress for non-reporting
    // channels such as inline data conversion.
    // uploadRoute 重复通用引擎预检，并对不报告进度的通道（例如内联数据转换）抑制进度。
    let inspection = crate::upload_engine::inspect_operation_upload_route(
        binding,
        source_reference,
        item_count,
        workspace_root,
        &resolved.upload_channels,
        credential_profile.credential_role.as_deref(),
    )
    .await?;
    if !inspection.channel.progress_supported {
        return Ok(None);
    }
    let upload_channel_id = inspection.channel.upload_channel_id;
    let total_bytes = inspection.resource.byte_len;
    // fileName is derived only from the already validated local path argument.
    // fileName 仅从已校验的本地路径参数派生。
    let file_name = Path::new(source_reference)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| String::from("Media-understanding upload path has no valid file name."))?;
    Ok(Some(RuntimeMediaUploadProgressContext {
        observer,
        base: crate::RuntimeMediaUploadProgressEvent {
            transfer_id: format!("media-tool:{}", Uuid::new_v4()),
            owner_session_id: owner_session_id.to_string(),
            provider_id: resolved.connection.provider_id.clone(),
            service_entry_id: resolved.entry.id.clone(),
            connection_id: resolved.connection.id.clone(),
            model_id: resolved.target.upstream_target_id.clone(),
            protocol_profile_id: String::from("provider-tool"),
            manifest_revision: resolved.manifest_revision,
            manifest_generation: resolved.manifest_generation,
            route_revision: resolved.route_revision.clone(),
            operation_id: operation.operation_id.clone(),
            upload_channel_id,
            file_name: file_name.to_string(),
            media_kind: media_kind.to_string(),
            phase: crate::RuntimeMediaUploadPhase::Preparing,
            bytes_sent: 0,
            total_bytes: Some(total_bytes),
            error: None,
        },
        last_bytes_sent: Arc::new(AtomicU64::new(0)),
    }))
}

#[cfg(test)]
mod tests;

/// Describes one exact provider-tool media input route without exposing credentials or references.
/// 描述一条精确的供应商工具媒体输入路线，且不暴露凭据或引用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderToolMediaInputRouteConstraint {
    /// Stable upload channel identifier owning this route.
    /// 持有当前路线的稳定上传通道标识。
    pub upload_channel_id: String,
    /// Exact MIME types accepted by both the operation binding and upload channel.
    /// 操作绑定与上传通道共同接受的精确 MIME 类型。
    pub accepted_mime_types: Vec<String>,
    /// Optional maximum source bytes accepted by this route.
    /// 当前路线可接受的可选最大源字节数。
    pub max_bytes: Option<u64>,
    /// Optional maximum source count accepted by this route.
    /// 当前路线可接受的可选最大源数量。
    pub max_count: Option<u64>,
}

/// Carries reviewed media input constraints for one fixed provider-understanding facade.
/// 携带一个固定供应商理解外壳的已审核媒体输入约束。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderToolMediaInputProfile {
    /// Exact resource kind consumed by this fixed facade.
    /// 当前固定外壳消费的精确资源类型。
    pub resource_kind: ResourceKind,
    /// Ordered concrete upload routes declared for the projected source field.
    /// 为投影来源字段声明的有序具体上传路线。
    pub routes: Vec<ProviderToolMediaInputRouteConstraint>,
}

impl PublicAiRuntime {
    /// Resolves the exact non-secret media input profile for one selected provider tool.
    /// 解析一个已选供应商工具的精确非秘密媒体输入 Profile。
    ///
    /// # Parameters
    /// 参数
    /// `configuration`, `capability`, and `selection` identify the same fixed route used by tool
    /// execution.
    /// `configuration`、`capability` 与 `selection` 标识工具执行使用的同一条固定路线。
    ///
    /// # Returns
    /// 返回值
    /// `None` for non-understanding capabilities, or the operation binding and channel
    /// intersections that the AI-facing tool description may safely disclose.
    /// 非理解能力返回 `None`；否则返回 AI 可见工具说明可安全披露的操作绑定与通道交集。
    pub fn provider_tool_media_input_profile(
        &self,
        configuration: &PersistedAiConfiguration,
        capability: ProviderToolCapabilityKind,
        selection: &ProviderToolTargetSelection,
    ) -> Result<Option<ProviderToolMediaInputProfile>, String> {
        let Some((resource_kind, _media_label)) = provider_tool_media_resource_kind(capability)
        else {
            return Ok(None);
        };
        let resolved = self.resolve_provider_tool_target(configuration, capability, selection)?;
        let operation = resolved.operation.as_ref().ok_or_else(|| {
            format!(
                "Media-understanding target `{}` has no owning operation.",
                resolved.target.target_id
            )
        })?;
        let projection = resolved
            .target
            .contract
            .media_input_projection
            .as_ref()
            .ok_or_else(|| {
                format!(
                    "Media-understanding target `{}` has no input projection.",
                    resolved.target.target_id
                )
            })?;
        let source_pointer = format!("/{}", projection.path_field);
        let mut matching_bindings = operation
            .upload_bindings
            .iter()
            .filter(|binding| binding.source_path == source_pointer);
        let binding = matching_bindings.next().ok_or_else(|| {
            format!(
                "Media-understanding operation `{}` has no upload binding for `{source_pointer}`.",
                operation.operation_id
            )
        })?;
        if matching_bindings.next().is_some() {
            return Err(format!(
                "Media-understanding operation `{}` has ambiguous upload bindings for `{source_pointer}`.",
                operation.operation_id
            ));
        }
        // bindingMimeTypes is the exact closed file-extension projection declared by the operation.
        // bindingMimeTypes 是操作声明的精确封闭文件扩展名投影。
        let mut binding_mime_types = BTreeSet::new();
        if let Some(mime_type) = binding.mime_type.as_ref() {
            binding_mime_types.insert(mime_type.clone());
        }
        binding_mime_types.extend(binding.mime_types_by_extension.values().cloned());
        let mut routes = Vec::with_capacity(binding.upload_channel_refs.len());
        for channel_id in &binding.upload_channel_refs {
            let channel = resolved
                .upload_channels
                .iter()
                .find(|channel| channel.upload_channel_id == *channel_id)
                .ok_or_else(|| {
                    format!(
                        "Media-understanding operation `{}` references missing upload channel `{channel_id}`.",
                        operation.operation_id
                    )
                })?;
            let channel_mime_types = channel
                .accepted_mime_types
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>();
            // acceptedMimeTypes retains empty-as-unrestricted semantics without inventing a format.
            // acceptedMimeTypes 保留空集合表示未进一步收窄的语义，且不虚构格式。
            let accepted_mime_types = if binding_mime_types.is_empty() {
                channel_mime_types
            } else if channel_mime_types.is_empty() {
                binding_mime_types.clone()
            } else {
                binding_mime_types
                    .intersection(&channel_mime_types)
                    .cloned()
                    .collect()
            };
            routes.push(ProviderToolMediaInputRouteConstraint {
                upload_channel_id: channel.upload_channel_id.clone(),
                accepted_mime_types: accepted_mime_types.into_iter().collect(),
                max_bytes: channel.max_bytes,
                max_count: channel.max_count,
            });
        }
        if routes.is_empty() {
            return Err(format!(
                "Media-understanding operation `{}` has no concrete upload route.",
                operation.operation_id
            ));
        }
        Ok(Some(ProviderToolMediaInputProfile {
            resource_kind,
            routes,
        }))
    }
}
