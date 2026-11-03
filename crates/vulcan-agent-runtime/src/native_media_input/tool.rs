//! Frozen native reader: materialize media without executing an additional AI request.
//! 冻结原生读取器：物化媒体，不执行额外AI请求。

use super::*;
use base64::Engine;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{path::PathBuf, sync::Arc};
use tokio::io::AsyncReadExt;
use vulcan_ai_contract::ResourceSourceKind;
use vulcan_provider_adapters::{
    AdmittedLocalMediaSource, MediaDeliveryDecision, MediaDeliveryOrigin, MediaDeliveryPlanner,
    MediaDeliveryPurpose, MediaDeliveryTarget, MediaUploadCancellation, MediaUploadDeadline,
    PreparedMediaSourceIdentity, ProviderToolMediaSourceResolver, PublicAiRuntime,
};
use vulcan_tools_manager::{
    VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor, VulcanToolInvocationResult,
    VulcanToolOrigin, VulcanToolSourceKind,
};

/// Cancels pending transfer work when its explicit read invocation is dropped.
/// 显式读取调用被释放时取消待处理传输工作。
struct CancelRead(MediaUploadCancellation);

impl Drop for CancelRead {
    /// Cancels only this read's transfer; returns no value and never cancels other session tools.
    /// 仅取消本次读取的传输；不返回值，绝不取消其他会话工具。
    fn drop(&mut self) {
        self.0.cancel();
    }
}

/// Describes exact admitted `mimes` per modality and directs unsupported input to its existing understanding tool.
/// 按模态说明精确准入的`mimes`，并将不支持的输入指向既有理解工具。
/// Returns AI-facing guidance without promising that an external tool survived the final exposure filters.
/// 返回面向AI的指引，不承诺外部工具一定通过了最终可见性过滤。
fn media_type_guidance(mimes: &[&str]) -> String {
    // Only the four categories produced by supported_mimes participate; PDF has one canonical MIME.
    // 仅处理supported_mimes产出的四类；PDF具有唯一规范MIME。
    let mut lines = Vec::new();
    for (label, prefix, external_capability) in [
        (
            "Images",
            "image/",
            Some(ProviderToolCapabilityKind::ImageUnderstand),
        ),
        (
            "Audio",
            "audio/",
            Some(ProviderToolCapabilityKind::AudioUnderstand),
        ),
        (
            "Video",
            "video/",
            Some(ProviderToolCapabilityKind::VideoUnderstand),
        ),
        ("PDF", "application/pdf", None),
    ] {
        // List only MIME types admitted by the same frozen selection used during execution.
        // 仅列出执行时使用的同一冻结选择所准入的MIME类型。
        let accepted = mimes
            .iter()
            .copied()
            .filter(|mime| mime.starts_with(prefix))
            .collect::<Vec<_>>();
        if !accepted.is_empty() {
            lines.push(format!("{label}: SUPPORTED only for {}. These are native input formats. Other image formats are accepted only if listed under automatic PNG conversion below; audio and video accept only their listed formats without transcoding.", accepted.join(", ")));
        } else if let Some(capability) = external_capability {
            // Reuse the canonical external tool identity, but require it to appear in the actual tool list.
            // 复用外部工具的规范身份，但要求它出现在实际工具列表中。
            let tool = crate::media_understanding::media_understanding_tool_id(capability)
                .expect("explicit media-understanding capability");
            lines.push(format!("{label}: NOT SUPPORTED by read_media. Do not pass this media type to read_media. For understanding, call {tool} if it is in your available tools; otherwise report that no suitable understanding tool is available."));
        } else {
            lines.push(format!("{label}: NOT SUPPORTED by read_media. Do not pass PDF files to read_media. To use a PDF's content, extract its text with another available tool, such as a shell command; a PDF attachment listing gives its stored file `path` for this. Report that the PDF cannot be read only when no such tool works."));
        }
    }
    lines.push(image_conversion::guidance(mimes));
    if mimes.iter().any(|mime| mime.starts_with("image/")) {
        lines.push(format!("Image compression: images whose longest edge exceeds {} px are downscaled to it, and large images may be re-encoded as JPEG (opaque) or PNG (transparent); the result states any conversion and the original and delivered dimensions.", image_conversion::COMPRESSION_MAX_EDGE_PX));
    }
    lines.join("\n")
}

/// Builds one current-model reader using frozen `selection`, host `runtime`, workspace and resource authority.
/// 使用冻结`selection`、宿主`runtime`、工作区及资源权限构建当前模型读取器。
/// Returns no tool when the exact model/protocol accepts none of the registered media formats.
/// 精确模型及协议不接受任何已注册媒体格式时不返回工具。
pub(crate) fn build_read_media_candidate(
    runtime: Arc<PublicAiRuntime>,
    selection: ResolvedPublicAiSelection,
    workspace_root: Option<PathBuf>,
    resources: Option<Arc<dyn crate::media_resource_runtime::MediaResourceRuntime>>,
) -> Option<VulcanToolCandidate> {
    // The advertised formats are exactly those checked again at invocation.
    // 声明格式与调用时再次检查的格式完全一致。
    let mimes = supported_mimes(&selection);
    if mimes.is_empty() {
        return None;
    }
    Some(VulcanToolCandidate {
        descriptor: VulcanToolDescriptor::new(
            READ_MEDIA_TOOL_ID,
            READ_MEDIA_TOOL_ID,
            format!(
                "Read only the media types listed below from a local file or an owned project-resource/attachment locator, and attach content to your next model input for your own understanding. No external AI understanding is called. Current model: {}; provider: {}; protocol: {}. Exact capabilities:\n{}\nImage support does not imply video support. Choose the tool for the source media type before calling it; never use read_media to probe an unsupported type. Subject to configured conversation delivery limits; errors mean the content was not attached. Use file for ordinary text files.",
                selection.model.id,
                selection.route.provider.id,
                selection.route.protocol_route.protocol_profile_id,
                media_type_guidance(&mimes)
            ),
            None,
            VulcanToolOrigin::new(
                VulcanToolSourceKind::AiHostBinding,
                "vulcan",
                READ_MEDIA_TOOL_ID,
            ),
        ),
        parameters_schema: json!({"type":"object","additionalProperties":false,"properties":{"path":{"type":"string","minLength":1,"description":format!("One readable local path or canonical project-resource/attachment locator. Native MIME types ONLY: {}. {} Relative paths use the current workspace.", mimes.join(", "), image_conversion::guidance(&mimes))}},"required":["path"]}),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Visible,
        request_streaming: None,
        invoke: Arc::new(move |arguments, context| {
            // Each invocation retains the same immutable target and host-owned source authority.
            // 每次调用保留相同不可变目标及宿主持有的来源权限。
            let runtime = Arc::clone(&runtime);
            // Clone the frozen route, never resolve mutable active-model settings after an await.
            // 克隆冻结路线，绝不在等待后解析可变的当前模型设置。
            let selection = selection.clone();
            // Relative paths retain the execution's workspace.
            // 相对路径保留执行工作区。
            let workspace = workspace_root.clone();
            // Resource locators retain the exact session-to-project port.
            // 资源定位符保留精确会话到项目的端口。
            let resources = resources.clone();
            Box::pin(async move {
                read_media(
                    runtime,
                    selection,
                    workspace,
                    resources,
                    &context.session_id,
                    arguments,
                )
                .await
            })
        }),
    })
}

/// Reads explicit `arguments` under frozen `selection` and host ownership, returning durable native input.
/// 在冻结`selection`及宿主归属下读取显式`arguments`，返回持久原生输入。
/// Workspace resolves local paths; `resources` and `session_id` authorize existing managed locators.
/// 工作区解析本地路径；`resources`及`session_id`授权既有托管定位符。
pub(super) async fn read_media(
    runtime: Arc<PublicAiRuntime>,
    selection: ResolvedPublicAiSelection,
    workspace: Option<PathBuf>,
    resources: Option<Arc<dyn crate::media_resource_runtime::MediaResourceRuntime>>,
    session_id: &str,
    arguments: Value,
) -> Result<VulcanToolInvocationResult, String> {
    // Closed arguments prevent undocumented decoding or transformation options.
    // 封闭参数阻止未定义的解码或转换选项。
    let object = arguments
        .as_object()
        .ok_or("read_media requires an object containing only path.")?;
    if object.len() != 1 {
        return Err("read_media requires exactly one path.".into());
    }
    // The path is an explicit model request, never inferred from another tool result.
    // 路径是模型显式请求，绝不从其他工具结果推断。
    let path = object
        .get("path")
        .and_then(Value::as_str)
        .filter(|path| !path.trim().is_empty())
        .ok_or("read_media requires a nonempty path.")?;
    // Bind cancellation before starting resolution or transfer work.
    // 在开始解析或传输工作前绑定取消。
    let cancellation = CancelRead(MediaUploadCancellation::new());
    // Managed sources have canonical metadata; explicit paths acquire their identity after size admission.
    // 托管来源具有规范元数据；显式路径在大小准入后取得身份。
    let (mut local_path, mut file_name, mut mime, mut byte_len, mut expected_identity) = if path
        .starts_with("project-resource:")
        || path.starts_with("attachment:")
    {
        // Reuse the existing exact locator authority for all four modalities, including PDF.
        // 为包括PDF在内的四种模态复用既有精确定位符权限。
        let source = crate::media_resource_runtime::SessionProviderToolMediaSourceResolver {
            runtime: resources.as_deref(),
            workspace_root: workspace.as_deref(),
            session_id,
        }
        .resolve_source(path)
        .await?;
        (
            source.local_path,
            source.file_name,
            source.mime_type,
            source.source_identity.byte_len,
            Some(source.source_identity),
        )
    } else {
        // The established path resolver owns relative paths and explicit host-file access.
        // 既有路径解析器持有相对路径与显式宿主文件访问语义。
        let local =
            vulcan_provider_adapters::resolve_provider_tool_local_path(workspace.as_deref(), path)
                .await?;
        // Use registered filename semantics, never relabel unsupported binary input as a PDF.
        // 使用已注册文件名语义，绝不把不支持的二进制输入改标为PDF。
        let source_mime = crate::attachment_assets::attachment_descriptor_from_path(&local)
            .ok_or("Unregistered media file format; no image converter is available for this extension.")?
            .mime_type;
        // Inspect size before hashing or allocating source bytes.
        // 在哈希或分配来源字节前检查大小。
        let metadata = tokio::fs::metadata(&local)
            .await
            .map_err(|error| error.to_string())?;
        if !metadata.is_file() {
            return Err("read_media requires a regular file.".into());
        }
        // Retain the actual filename as document metadata.
        // 保留实际文件名作为文档元数据。
        let name = local
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("Media filename is not valid UTF-8.")?
            .to_string();
        (local, name, source_mime.to_string(), metadata.len(), None)
    };
    // Retain compressed files through inline reading or physical upload; the original source remains unchanged.
    // 保留压缩文件直至内联读取或真实上传结束；原始来源保持不变。
    let compressed_file;
    // The receipt discloses local conversion or downscaling without changing durable envelope schema.
    // 回执明确本地转换或缩小，不改变持久信封结构。
    let mut conversion_notice = String::new();
    let native = supported_mimes(&selection).contains(&mime.as_str());
    let targets = image_conversion::CompressionTargets {
        original: native,
        jpeg: supports_mime(&selection, ResourceKind::Image, "image/jpeg"),
        png: supports_mime(&selection, ResourceKind::Image, "image/png"),
    };
    // Images are compressed like composer attachments, converting unsupported formats in the same pass;
    // formats with no converter or output target fall through to the explicit MIME rejection below.
    // 图片按输入区附件方式压缩，并在同一步转换不受支持的格式；无转换器或无输出目标的格式交给下方明确拒绝。
    if mime.starts_with("image/")
        && (native
            || (image_conversion::codec_for_mime(&mime).is_some() && (targets.jpeg || targets.png)))
        && let Some(image) = image_conversion::compress_file(
            &local_path,
            byte_len,
            expected_identity.as_ref(),
            &mime,
            targets,
        )
        .await
        .map_err(|error| {
            format!("Image compression of {mime} failed: {error}. No content was attached.")
        })?
    {
        if image.mime != mime {
            conversion_notice = format!(" Automatically converted from {mime} to {}.", image.mime);
        }
        if image.dimensions != image.source_dimensions {
            let ((from_width, from_height), (width, height)) =
                (image.source_dimensions, image.dimensions);
            conversion_notice.push_str(&format!(
                " Downscaled from {from_width}x{from_height} to {width}x{height} pixels; scale coordinates accordingly."
            ));
        } else if image.mime == mime {
            conversion_notice = format!(" Re-encoded from {byte_len} bytes to reduce size.");
        }
        compressed_file = image.write_temp_file()?;
        local_path = compressed_file.path().to_path_buf();
        file_name = PathBuf::from(&file_name)
            .with_extension(image.extension)
            .to_string_lossy()
            .into_owned();
        mime = image.mime.into();
        byte_len = image.bytes.len() as u64;
        expected_identity = None;
    }
    // MIME is authoritative for managed entities whose physical path may have no extension.
    // 对物理路径可能无扩展名的托管实体，MIME是权威。
    let descriptor = crate::attachment_assets::attachment_descriptor_from_mime_type(&mime)
        .ok_or("Unregistered media MIME type.")?;
    // Generic files remain the ordinary file tool's responsibility.
    // 通用文件仍由普通文件工具负责。
    let kind = resource_kind(descriptor.kind)
        .ok_or("read_media supports images, audio, video and PDF only.")?;
    if !supports_mime(&selection, kind, &mime) {
        return Err(format!(
            "read_media cannot read MIME type `{mime}` with your current model and protocol.\n{}",
            media_type_guidance(&supported_mimes(&selection))
        ));
    }
    // Preserve immutable upload ownership before moving the selection into execution resolution.
    // 将选择移入执行解析前保留不可变上传归属。
    let route_identity = crate::agent_runtime::provider::provider_route_lease_identity(&selection);
    // Resolving execution does not call an understanding model or reselect the active route.
    // 解析执行不调用理解模型，也不重新选择当前路线。
    let execution = runtime
        .resolve_execution_from_frozen_selection(selection)
        .await?;
    // The exact upload contract governs both transfer availability and hard limits.
    // 精确上传契约同时控制传输可用性与硬限制。
    let upload =
        execution
            .client
            .conversation_media_upload_contract(&execution.route, kind, &mime)?;
    // Tool content becomes main-conversation input and therefore uses conversation policy.
    // 工具内容成为主会话输入，因此使用会话策略。
    let mut planner = MediaDeliveryPlanner::new(
        execution.media_delivery_policy,
        MediaDeliveryPurpose::Conversation,
    )?;
    // Size admission occurs before source hashing or transfer.
    // 大小准入发生在来源哈希或传输之前。
    let decision = planner.plan(
        MediaDeliveryTarget {
            profile_id: &execution.route.protocol_route.protocol_profile_id,
            model_inputs: &execution.model.capabilities.input_resources,
            constraint: execution
                .route
                .protocol_route
                .input_resource_constraints
                .iter()
                .find(|constraint| constraint.resource_kind == kind),
            upload: upload.as_ref(),
        }
        .facts(kind, &mime, byte_len, MediaDeliveryOrigin::NewIntent, 1),
    )?;
    if let MediaDeliveryDecision::Attachment { reason } | MediaDeliveryDecision::Reject { reason } =
        decision
    {
        return Err(format!("Media was not attached: {}.", reason.explanation()));
    }
    // An admitted upload shares one declared deadline across source verification and physical transfer.
    // 已准入上传在来源校验及物理传输之间共享一个声明截止时间。
    let upload_deadline = if matches!(decision, MediaDeliveryDecision::Upload) {
        Some(
            MediaUploadDeadline::for_contract(
                upload
                    .as_ref()
                    .ok_or("Native media upload has no contract.")?,
            )
            .map_err(String::from)?,
        )
    } else {
        None
    };
    // The bounded source identity read enforces the preflight size across concurrent file changes.
    // 有界来源身份读取在文件并发变化时仍强制预检大小。
    let identity_read = execution.client.read_admitted_media_source_identity(
        &local_path,
        byte_len,
        Some(&cancellation.0),
    );
    // Inline reads need no upload timer; upload work never obtains a fresh budget after hashing.
    // 内联读取无需上传计时器；上传工作绝不在哈希后重新取得时间预算。
    let identity = match upload_deadline.as_ref() {
        Some(deadline) => deadline
            .wait(&cancellation.0, identity_read)
            .await
            .map_err(String::from)??,
        None => identity_read.await?,
    };
    if expected_identity
        .as_ref()
        .is_some_and(|expected| expected != &identity)
    {
        return Err("Media source changed from its registered identity.".into());
    }
    // Produce exactly the admitted representation; upload failure cannot silently switch to inline.
    // 仅产出准入表示；上传失败不能静默转为内联。
    let (reference, source_kind, origin) = match decision {
        MediaDeliveryDecision::Inline { .. } => {
            // Retain a bounded immutable byte snapshot so later projection never reopens the path.
            // 保留有界不可变字节快照，使后续投影绝不重新打开路径。
            let bytes = read_inline(&local_path, &identity).await?;
            (
                format!(
                    "data:{mime};base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(bytes)
                ),
                ResourceSourceKind::InlineData,
                None,
            )
        }
        MediaDeliveryDecision::Upload => {
            // An upload branch requires the already selected real contract.
            // 上传分支要求已经选择的真实契约。
            let contract = upload
                .as_ref()
                .ok_or("Native media upload has no contract.")?;
            // The lifetime starts before transfer, matching ordinary prompt upload semantics.
            // 生命周期从传输前开始，与普通提示词上传语义一致。
            let created = now_millis()?;
            // This uploads bytes only; the next ordinary model turn performs understanding.
            // 此处仅上传字节；下一次普通模型回合执行理解。
            let prepared = execution
                .prepare_conversation_upload_until(
                    AdmittedLocalMediaSource {
                        path: &local_path,
                        file_name: &file_name,
                        mime_type: &mime,
                        identity: &identity,
                    },
                    kind,
                    session_id,
                    contract,
                    &cancellation.0,
                    upload_deadline
                        .as_ref()
                        .ok_or("Native media upload has no deadline.")?,
                )
                .await
                .map_err(String::from)?;
            // Require a verified provider reference before reporting success.
            // 报告成功前要求具有已验证供应商引用。
            let reference = prepared
                .prepared_reference()
                .ok_or("Native upload returned no reference.")?
                .to_string();
            (
                reference,
                vulcan_provider_adapters::media_upload_source_kind(contract.reference_encoding),
                Some(NativeUploadOrigin {
                    route: route_identity,
                    session_id: session_id.into(),
                    contract_fingerprint: contract.contract_fingerprint.clone(),
                    expires_at_unix_millis: contract
                        .cache_duration_ms
                        .map(|duration| created.saturating_add(duration))
                        .unwrap_or(u64::MAX),
                }),
            )
        }
        _ => return Err("Native media read produced an invalid preparation action.".into()),
    };
    // The envelope owns bytes or a typed reference; host paths never enter encoder source fields.
    // 信封持有字节或带类型引用；宿主路径绝不进入编码器来源字段。
    let envelope = NativeMediaEnvelope {
        version: ENVELOPE_VERSION,
        text: format!(
            "Read media: {file_name} ({mime}, {byte_len} bytes).{conversion_notice} Prepared for your own understanding, subject to the next request's media admission."
        ),
        byte_len,
        upload: origin,
        resource: ResourceReference {
            id: if source_kind == ResourceSourceKind::ProviderFileId {
                reference.clone()
            } else {
                format!("native-media:{}", uuid::Uuid::new_v4())
            },
            kind,
            media_type: Some(mime),
            source_kind,
            file_name: Some(file_name),
            image_detail: None,
            resource_path: Some(path.into()),
            local_path: None,
            url: (source_kind != ResourceSourceKind::ProviderFileId).then_some(reference),
        },
    };
    Ok(VulcanToolInvocationResult::from_model_output(
        Value::String(serde_json::to_string(&envelope).map_err(|error| error.to_string())?),
    ))
}

/// Reads `path` within the admitted `identity` size and digest; returns verified original bytes or a source-change error.
/// 在准入`identity`大小及摘要内读取`path`；返回已验证原字节或来源变化错误。
async fn read_inline(
    path: &std::path::Path,
    identity: &PreparedMediaSourceIdentity,
) -> Result<Vec<u8>, String> {
    // One extra byte detects concurrent growth without an unbounded read.
    // 多读一个字节检测并发增长，避免无界读取。
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|error| error.to_string())?;
    // Allocation was admitted by the shared conversation policy before this function.
    // 分配在进入本函数前已通过共享会话策略准入。
    let mut bytes = Vec::new();
    file.take(
        identity
            .byte_len
            .checked_add(1)
            .ok_or("Media size overflow.")?,
    )
    .read_to_end(&mut bytes)
    .await
    .map_err(|error| error.to_string())?;
    if bytes.len() as u64 != identity.byte_len
        || hex::encode(Sha256::digest(&bytes)) != identity.content_sha256
    {
        return Err("Media source changed during reading.".into());
    }
    Ok(bytes)
}
