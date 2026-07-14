//! Generated asset staging and attachment selector helpers.
//! 生成资产暂存与附件选择器辅助能力。

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};
use vulcan_tools_manager::{VulcanToolHostResult, VulcanToolInvocationResult};

use crate::agent_runtime::provider_attachment::{
    build_missing_attachment_replay_text, read_provider_attachment,
};
use crate::attachment_assets::{
    attachment_descriptor_from_mime_type, attachment_descriptor_from_path, stage_attachment_bytes,
    stage_attachment_file, write_attachment_bytes_to_directory,
};
use crate::provider_config::AiToolKind;
use crate::workspace_path::{canonicalize_workspace_path, normalize_workspace_path_text};

use super::types::{GeneratedAssetOutputOptions, StagedGeneratedAsset};

/// Stages one generated binary asset into the managed attachment store.
/// 将单个生成得到的二进制资产暂存到托管附件存储中。
pub(super) fn stage_generated_asset_bytes(
    workspace_path: &str,
    mime_type: &str,
    suggested_file_name: &str,
    bytes: &[u8],
    output_options: &GeneratedAssetOutputOptions,
) -> Result<StagedGeneratedAsset, String> {
    let descriptor = attachment_descriptor_from_mime_type(mime_type).ok_or_else(|| {
        format!("Generated assets with MIME type `{mime_type}` are not currently supported.")
    })?;
    // Caller-visible preferred file name whose extension is normalized from the resolved media descriptor.
    // 面向调用方的首选文件名，其扩展名会根据已解析媒体描述符完成规范化。
    let requested_file_name =
        resolve_requested_output_file_name(suggested_file_name, output_options);
    if let Some(output_dir) = output_options.output_dir.as_deref() {
        // Filesystem directory chosen for the caller-visible exported copy.
        // 为调用方可见导出副本选定的文件系统目录。
        let export_directory = resolve_generated_output_directory(workspace_path, output_dir)?;
        // Exported file path returned to the model for immediate follow-up work.
        // 回传给模型、供其立即执行后续工作的导出文件路径。
        let exported_path = write_attachment_bytes_to_directory(
            &descriptor,
            &export_directory,
            &requested_file_name,
            bytes,
        )?;
        // Managed attachment copy retained for transcript rendering, replay, and attachment indexing.
        // 为转录渲染、回放与附件索引保留的托管附件副本。
        let attachment = stage_attachment_file(workspace_path, &exported_path)?;
        return Ok(StagedGeneratedAsset {
            attachment,
            model_local_path: normalize_workspace_path_text(&exported_path.to_string_lossy()),
        });
    }

    // Managed attachment copy used both as the durable host asset and as the model-visible path when no export directory is requested.
    // 在未请求导出目录时，同时作为宿主持久资产与模型可见路径的托管附件副本。
    let attachment = stage_attachment_bytes(
        workspace_path,
        &descriptor,
        &requested_file_name,
        None,
        bytes,
    )?;
    Ok(StagedGeneratedAsset {
        model_local_path: normalize_workspace_path_text(&attachment.local_path),
        attachment,
    })
}

/// Resolves one caller-visible requested file name by favoring the optional caller-supplied stem while still letting the host control the final extension.
/// 解析一条面向调用方的请求文件名：优先采用可选的调用方名称主体，同时仍由宿主控制最终扩展名。
pub(super) fn resolve_requested_output_file_name(
    suggested_file_name: &str,
    output_options: &GeneratedAssetOutputOptions,
) -> String {
    output_options
        .output_file_stem
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| suggested_file_name.to_string())
}

/// Adds one numbered suffix to the optional caller-supplied output stem when one request emits multiple files.
/// 当一次请求会产出多个文件时，为可选的调用方输出名称主体追加编号后缀。
pub(super) fn with_optional_output_sequence(
    output_options: &GeneratedAssetOutputOptions,
    output_count: usize,
    output_index: usize,
) -> GeneratedAssetOutputOptions {
    if output_count <= 1 {
        return output_options.clone();
    }

    // Cloned output options updated only when a caller-supplied file-name stem exists.
    // 仅在存在调用方文件名主体时才会被更新的克隆输出选项。
    let mut sequenced_output_options = output_options.clone();
    if let Some(output_file_stem) = output_options
        .output_file_stem
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        sequenced_output_options.output_file_stem =
            Some(format!("{output_file_stem}-{:03}", output_index + 1));
    }
    sequenced_output_options
}

/// Resolves one caller-selected output directory into an absolute filesystem path by treating relative paths as workspace-relative.
/// 将调用方选定的输出目录解析为绝对文件系统路径，并把相对路径视作相对工作区路径。
pub(super) fn resolve_generated_output_directory(
    workspace_path: &str,
    output_dir: &str,
) -> Result<PathBuf, String> {
    // Trimmed output-directory text used for path resolution.
    // 用于路径解析的裁剪后输出目录文本。
    let trimmed_output_dir = output_dir.trim();
    // Raw output-directory path supplied by the caller.
    // 调用方提供的原始输出目录路径。
    let requested_path = PathBuf::from(trimmed_output_dir);
    if requested_path.is_absolute() {
        return Ok(canonicalize_workspace_path(&requested_path));
    }

    // Workspace anchor used for resolving caller-relative output directories.
    // 用于解析调用方相对输出目录的工作区锚点路径。
    let workspace_anchor = resolve_workspace_anchor_directory(workspace_path).ok_or_else(|| {
        String::from("Relative generated output directories require a non-empty workspace path.")
    })?;
    Ok(canonicalize_workspace_path(
        &workspace_anchor.join(requested_path),
    ))
}

/// Resolves the directory anchor used for workspace-relative generated-output paths.
/// 解析工作区相对生成输出路径使用的目录锚点。
fn resolve_workspace_anchor_directory(workspace_path: &str) -> Option<PathBuf> {
    // Trimmed workspace-path text captured from the active execution preparation.
    // 从当前执行准备中捕获的裁剪后工作区路径文本。
    let trimmed_workspace_path = workspace_path.trim();
    if trimmed_workspace_path.is_empty() {
        return None;
    }

    // Canonical workspace candidate that may point either to the directory itself or to one file inside it.
    // 规范化后的工作区候选路径，它既可能直接指向目录，也可能指向目录内部的某个文件。
    let workspace_candidate = canonicalize_workspace_path(Path::new(trimmed_workspace_path));
    if workspace_candidate.is_dir() {
        return Some(workspace_candidate);
    }
    Some(
        workspace_candidate
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or(workspace_candidate),
    )
}

/// Builds one structured tool result that includes both model-visible output and richer host metadata for generated assets.
/// 构建一条同时包含模型可见输出与更丰富宿主元数据的生成资产工具结果。
pub(super) fn build_generated_result(
    capability_kind: AiToolKind,
    target_selector: &str,
    extra: Option<Value>,
    outputs: Vec<StagedGeneratedAsset>,
) -> VulcanToolInvocationResult {
    let host_outputs = outputs
        .iter()
        .map(|output| attachment_to_json(&output.attachment))
        .collect::<Vec<_>>();
    let model_outputs = outputs
        .iter()
        .map(generated_asset_to_model_output_json)
        .collect::<Vec<_>>();
    let model_output = json!({
        "ok": true,
        "capability": capability_kind.key(),
        "target": target_selector,
        "outputs": model_outputs,
        "extra": extra.clone().unwrap_or(Value::Null),
    });
    VulcanToolInvocationResult::from_model_output(model_output).with_host_result(
        VulcanToolHostResult {
            kind: String::from("generated_attachments"),
            payload: json!({
                "capability": capability_kind.key(),
                "target": target_selector,
                "outputs": host_outputs,
                "extra": extra.unwrap_or(Value::Null),
            }),
        },
    )
}

/// Builds one successful-but-degraded result for attachments whose managed replay file has already disappeared.
/// 为托管回放文件已消失的附件构建一条成功但降级的结果。
pub(super) fn missing_attachment_result(
    attachment: &ConversationAttachment,
) -> VulcanToolInvocationResult {
    VulcanToolInvocationResult::from_model_output(Value::String(
        build_missing_attachment_replay_text(attachment),
    ))
}

/// Serializes one staged attachment into a compact JSON object carried by model-visible and host-only tool results.
/// 将单条已暂存附件序列化为紧凑 JSON 对象，供模型可见与宿主专用工具结果共同复用。
pub(super) fn attachment_to_json(attachment: &ConversationAttachment) -> Value {
    json!({
        "kind": attachment.kind.modality_name(),
        "file_name": attachment.file_name,
        "local_path": attachment.local_path,
        "source_path": attachment.source_path,
        "mime_type": attachment.mime_type,
        "asset_id": attachment.asset_id,
        "byte_len": attachment.byte_len,
        "preview_local_path": attachment.preview_local_path,
    })
}

/// Serializes one generated output into a model-visible JSON object that includes the preferred local filesystem path for follow-up tool calls.
/// 将单条生成输出序列化为模型可见 JSON 对象，并携带供后续工具调用使用的首选本地文件系统路径。
pub(super) fn generated_asset_to_model_output_json(output: &StagedGeneratedAsset) -> Value {
    json!({
        "kind": output.attachment.kind.modality_name(),
        "file_name": output.attachment.file_name,
        "mime_type": output.attachment.mime_type,
        "asset_id": output.attachment.asset_id,
        "byte_len": output.attachment.byte_len,
        "local_path": output.model_local_path,
    })
}

/// Resolves one optional image selector into a Data URL accepted by MiniMax multimodal generation endpoints.
/// 将单个可选图片选择器解析为 MiniMax 多模态生成端点接受的 Data URL。
pub(super) fn resolve_optional_image_data_url(
    workspace_path: &str,
    attachments: &[ConversationAttachment],
    image_selector: Option<&str>,
) -> Result<Option<String>, String> {
    let Some(image_selector) = image_selector
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let attachment = resolve_attachment_by_name_or_local_path(
        attachments,
        ConversationAttachmentKind::Image,
        image_selector,
        workspace_path,
    )
    .ok_or_else(|| format!("The image selector `{image_selector}` does not exist."))?;
    let payload = match read_provider_attachment(&attachment).map_err(|error| error.to_string())? {
        Some(payload) => payload,
        None => return Ok(None),
    };
    Ok(Some(payload.data_url))
}

/// Resolves one staged attachment by user-visible file name, asset id, or managed local-path file name.
/// 根据用户可见文件名、资产标识或托管本地文件名解析单条已暂存附件。
fn resolve_attachment_by_name<'a>(
    attachments: &'a [ConversationAttachment],
    expected_kind: &ConversationAttachmentKind,
    selector: &str,
) -> Option<&'a ConversationAttachment> {
    let normalized_selector = selector.trim().to_ascii_lowercase();
    if normalized_selector.is_empty() {
        return None;
    }

    attachments.iter().find(|attachment| {
        attachment.kind == *expected_kind
            && (attachment
                .local_path
                .trim()
                .eq_ignore_ascii_case(selector.trim())
                || attachment
                    .source_path
                    .as_deref()
                    .is_some_and(|source_path| {
                        source_path.trim().eq_ignore_ascii_case(selector.trim())
                    })
                || attachment
                    .preview_local_path
                    .as_deref()
                    .is_some_and(|preview_path| {
                        preview_path.trim().eq_ignore_ascii_case(selector.trim())
                    })
                || attachment
                    .file_name
                    .trim()
                    .eq_ignore_ascii_case(&normalized_selector)
                || attachment.asset_id.as_deref().is_some_and(|asset_id| {
                    asset_id.trim().eq_ignore_ascii_case(&normalized_selector)
                })
                || Path::new(&attachment.local_path)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .is_some_and(|file_name| {
                        file_name.trim().eq_ignore_ascii_case(&normalized_selector)
                    }))
    })
}

/// Resolves one attachment selector either from the current host-visible attachment list or from one direct local file path returned by a previous tool call.
/// 从当前宿主可见附件列表或前序工具调用返回的直接本地文件路径中解析单条附件选择器。
pub(super) fn resolve_attachment_by_name_or_local_path(
    attachments: &[ConversationAttachment],
    expected_kind: ConversationAttachmentKind,
    selector: &str,
    workspace_path: &str,
) -> Option<ConversationAttachment> {
    if let Some(attachment) = resolve_attachment_by_name(attachments, &expected_kind, selector) {
        return Some(attachment.clone());
    }
    resolve_attachment_from_local_path(expected_kind, selector, workspace_path)
}

/// Resolves one direct local file path into a synthetic attachment descriptor so follow-up media tools can reuse generated outputs inside the same turn.
/// 将一条直接本地文件路径解析为合成附件描述信息，从而让后续媒体工具能够在同一轮内复用生成结果。
pub(super) fn resolve_attachment_from_local_path(
    expected_kind: ConversationAttachmentKind,
    selector: &str,
    workspace_path: &str,
) -> Option<ConversationAttachment> {
    // Candidate direct-path text supplied by the model.
    // 模型提供的候选直接路径文本。
    let trimmed_selector = selector.trim();
    if trimmed_selector.is_empty() {
        return None;
    }

    // Absolute candidate path resolved from the selector and the current workspace root when the selector is relative.
    // 当选择器是相对路径时，基于当前工作区根目录解析得到的绝对候选路径。
    let candidate_path = resolve_local_selector_path(workspace_path, trimmed_selector)?;
    if !candidate_path.is_file() {
        return None;
    }

    // Host attachment descriptor inferred from the local file-path extension.
    // 根据本地文件路径扩展名推断出的宿主附件描述信息。
    let descriptor = attachment_descriptor_from_path(&candidate_path)?;
    if descriptor.kind != expected_kind {
        return None;
    }

    // Metadata snapshot reused for host-side byte-length hints.
    // 为宿主侧字节长度提示复用的元数据快照。
    let metadata = fs::metadata(&candidate_path).ok();
    Some(ConversationAttachment {
        kind: descriptor.kind,
        file_name: candidate_path
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| {
                vulcan_runtime_assets::native_host_path_text_from_path(&candidate_path)
            }),
        local_path: vulcan_runtime_assets::native_host_path_text_from_path(&candidate_path),
        source_path: None,
        asset_id: None,
        byte_len: metadata.map(|value| value.len()),
        preview_local_path: None,
        mime_type: String::from(descriptor.mime_type),
    })
}

/// Resolves one local selector path by treating relative paths as workspace-relative and then canonicalizing any existing filesystem target.
/// 解析一条本地选择器路径：把相对路径视作相对工作区路径，并对已存在的文件系统目标做规范化。
pub(super) fn resolve_local_selector_path(workspace_path: &str, selector: &str) -> Option<PathBuf> {
    // Raw selector path supplied by the model for one follow-up media input.
    // 模型为某个后续媒体输入提供的原始选择器路径。
    let selector_path = PathBuf::from(selector.trim());
    if selector_path.is_absolute() {
        return Some(canonicalize_workspace_path(&selector_path));
    }
    let workspace_anchor = resolve_workspace_anchor_directory(workspace_path)?;
    Some(canonicalize_workspace_path(
        &workspace_anchor.join(selector_path),
    ))
}

/// Builds one stable selector description that accepts resource ids or explicit local paths.
/// 构建一条同时接受资源标识与显式本地路径的稳定选择器说明。
pub(super) fn attachment_selector_description(
    base_description: &str,
    modality_name: &str,
) -> String {
    format!(
        "{base_description} Use the resource id provided for pasted or uploaded {modality_name} attachments, or use a full local path when the {modality_name} was provided as a path or returned by another host tool. Legacy file names are still accepted for compatibility."
    )
}
