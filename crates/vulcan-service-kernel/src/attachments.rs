//! WebUI attachment cleanup and composer attachment helpers.
//! WebUI 附件清理与输入区附件辅助逻辑。

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use serde::{Deserialize, Serialize};
use vulcan_agent_runtime::{
    attachment_asset_index::{
        AttachmentCleanupPreview, AttachmentCleanupReport, AttachmentCleanupSelection,
        execute_attachment_cleanup, preview_attachment_cleanup,
    },
    attachment_assets::{
        attachment_descriptor_from_mime_type, attachment_descriptor_from_path,
        stage_attachment_bytes, stage_attachment_file,
    },
};
use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};
use vulcan_provider_config::config::{PersistedAiToolConfiguration, ProviderConfigurationFile};
use vulcan_service_api::{ComposerAttachmentCapabilitySummary, ComposerAttachmentSummary};

use crate::{
    SharedServiceBridgeState, ai_capabilities::active_workbench_model_capabilities,
    normalized_workspace_path_text_from_path, parse_session_id, spawn_service_blocking_task,
    vmm_project_scope_path, workspace_root,
};

/// WebUI request body for project-scoped attachment cleanup operations.
/// WebUI 项目级附件清理操作的请求体。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WebuiProjectAttachmentCleanupRequest {
    /// Stable project identifier selected in the WebUI project settings dialog.
    /// WebUI 项目设置弹窗选中的稳定项目标识。
    project_id: String,
    /// Local cutoff date in `YYYY-MM-DD` format.
    /// `YYYY-MM-DD` 格式的本地截止日期。
    cutoff_date: String,
    /// Attachment-kind filters selected by the user.
    /// 用户选择的附件类型筛选。
    selection: WebuiAttachmentCleanupSelection,
}

/// WebUI attachment-kind filter state.
/// WebUI 附件类型筛选状态。
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WebuiAttachmentCleanupSelection {
    /// Whether image assets should participate in cleanup.
    /// 图片资产是否参与清理。
    image: bool,
    /// Whether audio assets should participate in cleanup.
    /// 音频资产是否参与清理。
    audio: bool,
    /// Whether video assets should participate in cleanup.
    /// 视频资产是否参与清理。
    video: bool,
    /// Whether PDF assets should participate in cleanup.
    /// PDF 资产是否参与清理。
    pdf: bool,
}

/// WebUI summary returned by attachment cleanup preview.
/// 附件清理预览返回给 WebUI 的摘要。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WebuiAttachmentCleanupPreviewSummary {
    /// Cutoff date applied by the preview.
    /// 预览使用的截止日期。
    cutoff_date: String,
    /// Total matched attachment asset count.
    /// 匹配的附件资产总数。
    matched_asset_count: usize,
    /// Human-readable matched byte count.
    /// 匹配字节数的人类可读文本。
    matched_byte_len_label: String,
    /// Unique session count impacted by matched assets.
    /// 受匹配资产影响的唯一会话数量。
    impacted_session_count: usize,
    /// Per-kind matched asset counts.
    /// 按类型统计的匹配资产数。
    per_kind_counts: BTreeMap<String, usize>,
    /// One-line user-facing summary.
    /// 面向用户的一行摘要。
    summary: String,
}

/// WebUI summary returned after attachment cleanup execution.
/// 附件清理执行后返回给 WebUI 的摘要。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WebuiAttachmentCleanupReportSummary {
    /// Cutoff date applied by the execution.
    /// 执行使用的截止日期。
    cutoff_date: String,
    /// Total purged attachment asset count.
    /// 被清理的附件资产总数。
    purged_asset_count: usize,
    /// Total deleted managed file count.
    /// 已删除的托管文件总数。
    deleted_file_count: usize,
    /// Total managed files already missing before cleanup.
    /// 清理前已经缺失的托管文件总数。
    already_missing_file_count: usize,
    /// Total managed files that could not be deleted.
    /// 未能删除的托管文件总数。
    failed_delete_count: usize,
    /// Human-readable purged byte count.
    /// 清理字节数的人类可读文本。
    purged_byte_len_label: String,
    /// Unique session count impacted by purged assets.
    /// 受清理资产影响的唯一会话数量。
    impacted_session_count: usize,
    /// One-line user-facing summary.
    /// 面向用户的一行摘要。
    summary: String,
}

/// Builds a project-scoped managed attachment cleanup preview for the WebUI settings dialog.
/// 为 WebUI 设置弹窗构建项目级托管附件清理预览。
///
/// # Parameters
/// 参数
/// `state` owns the current workspace and project selection snapshot.
/// `state` 持有当前工作区与项目选择快照。
/// `request` contains the project id, cutoff date, and attachment-kind selection.
/// `request` 包含项目标识、截止日期和附件类型选择。
///
/// # Returns
/// 返回值
/// A WebUI preview summary for the matching managed attachment assets.
/// 匹配托管附件资产的 WebUI 预览摘要。
pub(super) async fn service_preview_project_attachment_cleanup(
    state: SharedServiceBridgeState,
    request: WebuiProjectAttachmentCleanupRequest,
) -> Result<WebuiAttachmentCleanupPreviewSummary, String> {
    let project_path = vmm_project_scope_path(&state, Some(&request.project_id))?;
    let cutoff_date = request.cutoff_date.trim().to_string();
    let selection = webui_attachment_cleanup_selection(request.selection);
    spawn_service_blocking_task(move || {
        let preview = preview_attachment_cleanup(&project_path, &cutoff_date, selection)?;
        Ok(webui_attachment_cleanup_preview_summary(preview))
    })
    .await
    .map_err(|error| format!("附件清理预览任务失败：{error}"))?
}

/// Executes a project-scoped managed attachment cleanup for the WebUI settings dialog.
/// 为 WebUI 设置弹窗执行项目级托管附件清理。
///
/// # Parameters
/// 参数
/// `state` owns the current workspace and project selection snapshot.
/// `state` 持有当前工作区与项目选择快照。
/// `request` contains the project id, cutoff date, and attachment-kind selection.
/// `request` 包含项目标识、截止日期和附件类型选择。
///
/// # Returns
/// 返回值
/// A WebUI cleanup report after eligible managed attachment assets are purged.
/// 符合条件的托管附件资产清理后的 WebUI 报告。
pub(super) async fn service_execute_project_attachment_cleanup(
    state: SharedServiceBridgeState,
    request: WebuiProjectAttachmentCleanupRequest,
) -> Result<WebuiAttachmentCleanupReportSummary, String> {
    let project_path = vmm_project_scope_path(&state, Some(&request.project_id))?;
    let cutoff_date = request.cutoff_date.trim().to_string();
    let selection = webui_attachment_cleanup_selection(request.selection);
    spawn_service_blocking_task(move || {
        let report = execute_attachment_cleanup(&project_path, &cutoff_date, selection)?;
        Ok(webui_attachment_cleanup_report_summary(report))
    })
    .await
    .map_err(|error| format!("附件清理执行任务失败：{error}"))?
}

/// Rejects the retired native attachment picker RPC.
/// 拒绝已退役的原生附件选择器 RPC。
///
/// # Parameters
/// 参数
/// `_state` is retained for the retired RPC signature.
/// `_state` 为已退役 RPC 签名保留。
/// `_project_id` is retained for the retired RPC signature.
/// `_project_id` 为已退役 RPC 签名保留。
/// `_conversation_id` is retained for the retired RPC signature.
/// `_conversation_id` 为已退役 RPC 签名保留。
/// `_kind` is retained for the retired RPC signature.
/// `_kind` 为已退役 RPC 签名保留。
///
/// # Returns
/// 返回值
/// Always returns an error directing callers to WebUI upload and path staging APIs.
/// 始终返回错误，引导调用方使用 WebUI 上传与路径暂存接口。
pub(super) async fn service_pick_composer_attachments(
    _state: SharedServiceBridgeState,
    _project_id: Option<String>,
    _conversation_id: Option<String>,
    _kind: Option<String>,
) -> Result<Vec<ComposerAttachmentSummary>, String> {
    Err(String::from(
        "原生附件选择器已从服务壳层移除，请使用 WebUI 文件上传或路径暂存接口。",
    ))
}

/// Stages existing local paths as composer attachments.
/// 将已有本地路径暂存为输入区附件。
///
/// # Parameters
/// 参数
/// `state` owns the workspace snapshot used to resolve the staging root.
/// `state` 持有用于解析暂存根目录的工作区快照。
/// `project_id` optionally selects the owning project.
/// `project_id` 可选指定所属项目。
/// `conversation_id` optionally selects a conversation whose project owns the attachment.
/// `conversation_id` 可选指定拥有附件项目的会话。
/// `paths` contains local filesystem paths to stage.
/// `paths` 包含要暂存的本地文件系统路径。
///
/// # Returns
/// 返回值
/// Composer attachment summaries for the staged local files.
/// 已暂存本地文件的输入区附件摘要。
pub(super) async fn service_stage_composer_attachment_paths(
    state: SharedServiceBridgeState,
    project_id: Option<String>,
    conversation_id: Option<String>,
    paths: Vec<String>,
) -> Result<Vec<ComposerAttachmentSummary>, String> {
    let workspace_path = composer_attachment_workspace_path(
        &state,
        project_id.as_deref(),
        conversation_id.as_deref(),
    )?;
    let paths = paths.into_iter().map(PathBuf::from).collect::<Vec<_>>();
    stage_composer_attachment_paths_for_workspace(&workspace_path, paths)
}

/// Stages one browser-provided in-memory file as a composer attachment.
/// 将浏览器提供的一份内存文件暂存为输入区附件。
///
/// # Parameters
/// 参数
/// `state` owns the workspace snapshot used to resolve the staging root.
/// `state` 持有用于解析暂存根目录的工作区快照。
/// `project_id` optionally selects the owning project.
/// `project_id` 可选指定所属项目。
/// `conversation_id` optionally selects a conversation whose project owns the attachment.
/// `conversation_id` 可选指定拥有附件项目的会话。
/// `file_name` is the browser-provided display file name.
/// `file_name` 是浏览器提供的展示文件名。
/// `mime_type` is the browser-provided MIME type.
/// `mime_type` 是浏览器提供的 MIME 类型。
/// `data_base64` is the base64-encoded file content.
/// `data_base64` 是 base64 编码的文件内容。
///
/// # Returns
/// 返回值
/// A composer attachment summary for the staged in-memory file.
/// 已暂存内存文件的输入区附件摘要。
pub(super) async fn service_stage_composer_attachment_bytes(
    state: SharedServiceBridgeState,
    project_id: Option<String>,
    conversation_id: Option<String>,
    file_name: String,
    mime_type: String,
    data_base64: String,
) -> Result<ComposerAttachmentSummary, String> {
    let workspace_path = composer_attachment_workspace_path(
        &state,
        project_id.as_deref(),
        conversation_id.as_deref(),
    )?;
    let descriptor = attachment_descriptor_from_mime_type(&mime_type)
        .or_else(|| attachment_descriptor_from_path_fallback(&file_name))
        .ok_or_else(|| format!("文件 `{}` 不是受支持的附件格式。", file_name.trim()))?;
    let bytes = BASE64_STANDARD
        .decode(data_base64.trim())
        .map_err(|error| format!("解码附件 `{}` 失败：{}。", file_name.trim(), error))?;
    let attachment = stage_attachment_bytes(
        &workspace_path,
        &descriptor,
        file_name.trim(),
        None,
        bytes.as_slice(),
    )?;
    Ok(composer_attachment_summary_from_core(&attachment))
}

/// Resolves the project workspace path used to stage one composer attachment.
/// 解析暂存输入区附件时使用的项目工作区路径。
///
/// # Parameters
/// 参数
/// `state` owns the workspace snapshot used for project lookup.
/// `state` 持有用于项目查找的工作区快照。
/// `project_id` optionally provides an explicit project id.
/// `project_id` 可选提供显式项目标识。
/// `conversation_id` optionally provides a conversation id whose project should be used.
/// `conversation_id` 可选提供需要使用其项目的会话标识。
///
/// # Returns
/// 返回值
/// The resolved workspace path text used by attachment staging.
/// 附件暂存使用的已解析工作区路径文本。
fn composer_attachment_workspace_path(
    state: &SharedServiceBridgeState,
    project_id: Option<&str>,
    conversation_id: Option<&str>,
) -> Result<String, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let Some(workspace_snapshot) = guard.workspace_snapshot.as_ref() else {
        return Ok(normalized_workspace_path_text_from_path(&workspace_root()));
    };
    let owner_project_id = conversation_id
        .and_then(parse_session_id)
        .and_then(|session_id| {
            workspace_snapshot
                .sessions
                .iter()
                .find(|session| session.id == session_id)
                .and_then(|session| session.project_id.as_deref())
        })
        .or(project_id)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let Some(owner_project_id) = owner_project_id else {
        return Ok(normalized_workspace_path_text_from_path(&workspace_root()));
    };
    Ok(workspace_snapshot
        .task_projects
        .iter()
        .find(|project| project.id == owner_project_id)
        .map(|project| project.path.clone())
        .unwrap_or_else(|| normalized_workspace_path_text_from_path(&workspace_root())))
}

/// Stages local paths for a resolved workspace path and converts them to service DTOs.
/// 为已解析的工作区路径暂存本地路径，并转换为 service DTO。
///
/// # Parameters
/// 参数
/// `workspace_path` is the resolved project workspace path.
/// `workspace_path` 是已解析的项目工作区路径。
/// `paths` contains local filesystem paths to stage.
/// `paths` 包含要暂存的本地文件系统路径。
///
/// # Returns
/// 返回值
/// Composer attachment summaries for every staged path.
/// 每个已暂存路径对应的输入区附件摘要。
fn stage_composer_attachment_paths_for_workspace(
    workspace_path: &str,
    paths: Vec<PathBuf>,
) -> Result<Vec<ComposerAttachmentSummary>, String> {
    paths
        .iter()
        .map(|path| stage_attachment_file(workspace_path, path))
        .map(|result| result.map(|attachment| composer_attachment_summary_from_core(&attachment)))
        .collect()
}

/// Converts a core conversation attachment into the WebUI service DTO.
/// 将核心会话附件转换为 WebUI service DTO。
///
/// # Parameters
/// 参数
/// `attachment` is the core conversation attachment to project.
/// `attachment` 是要投影的核心会话附件。
///
/// # Returns
/// 返回值
/// The WebUI service attachment summary.
/// WebUI 服务附件摘要。
pub(super) fn composer_attachment_summary_from_core(
    attachment: &ConversationAttachment,
) -> ComposerAttachmentSummary {
    ComposerAttachmentSummary {
        kind: attachment.kind.modality_name().to_string(),
        file_name: attachment.file_name.clone(),
        local_path: attachment.local_path.clone(),
        source_path: attachment.source_path.clone(),
        asset_id: attachment.asset_id.clone(),
        byte_len: attachment.byte_len,
        preview_local_path: attachment.preview_local_path.clone(),
        mime_type: attachment.mime_type.clone(),
    }
}

/// Converts a WebUI service attachment DTO back into the core conversation attachment.
/// 将 WebUI service 附件 DTO 转回核心会话附件。
///
/// # Parameters
/// 参数
/// `summary` is the WebUI service attachment summary to convert.
/// `summary` 是要转换的 WebUI 服务附件摘要。
///
/// # Returns
/// 返回值
/// The core conversation attachment reconstructed from the summary.
/// 从摘要重建的核心会话附件。
pub(super) fn composer_attachment_summary_to_core(
    summary: ComposerAttachmentSummary,
) -> Result<ConversationAttachment, String> {
    Ok(ConversationAttachment {
        kind: parse_composer_attachment_kind(&summary.kind)?,
        file_name: summary.file_name,
        local_path: summary.local_path,
        source_path: summary.source_path,
        asset_id: summary.asset_id,
        byte_len: summary.byte_len,
        preview_local_path: summary.preview_local_path,
        mime_type: summary.mime_type,
    })
}

/// Parses one stable composer attachment kind key.
/// 解析单个稳定输入区附件类型键。
///
/// # Parameters
/// 参数
/// `value` is the raw attachment kind key from a WebUI summary.
/// `value` 是来自 WebUI 摘要的原始附件类型键。
///
/// # Returns
/// 返回值
/// The parsed core attachment kind.
/// 解析后的核心附件类型。
fn parse_composer_attachment_kind(value: &str) -> Result<ConversationAttachmentKind, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "image" => Ok(ConversationAttachmentKind::Image),
        "audio" => Ok(ConversationAttachmentKind::Audio),
        "video" => Ok(ConversationAttachmentKind::Video),
        "pdf" | "document" => Ok(ConversationAttachmentKind::Pdf),
        other => Err(format!("不支持的附件类型：{other}")),
    }
}

/// Resolves an attachment descriptor from a browser file name when the MIME type is missing.
/// 当浏览器未提供 MIME 类型时，从文件名解析附件描述。
///
/// # Parameters
/// 参数
/// `file_name` is the browser-provided file name used for extension inference.
/// `file_name` 是用于扩展名推断的浏览器文件名。
///
/// # Returns
/// 返回值
/// The inferred attachment descriptor when the file extension is supported.
/// 文件扩展名受支持时返回推断出的附件描述。
fn attachment_descriptor_from_path_fallback(
    file_name: &str,
) -> Option<vulcan_agent_runtime::attachment_assets::AttachmentDescriptor> {
    attachment_descriptor_from_path(Path::new(file_name))
}

/// Returns the localized label used by the native file picker filter.
/// 返回原生文件选择器过滤器使用的本地化名称。
///
/// # Parameters
/// 参数
/// `kind` is the core attachment kind whose label should be rendered.
/// `kind` 是需要渲染标签的核心附件类型。
///
/// # Returns
/// 返回值
/// A localized attachment kind label.
/// 本地化的附件类型标签。
fn composer_attachment_kind_label(kind: &ConversationAttachmentKind) -> &'static str {
    match kind {
        ConversationAttachmentKind::Image => "图片",
        ConversationAttachmentKind::Audio => "音频",
        ConversationAttachmentKind::Video => "视频",
        ConversationAttachmentKind::Pdf => "PDF",
    }
}

/// Returns the browser file picker accept list for one attachment kind.
/// 返回单种附件类型对应的浏览器文件选择 accept 列表。
///
/// # Parameters
/// 参数
/// `kind` is the core attachment kind whose browser accept list should be rendered.
/// `kind` 是需要渲染浏览器 accept 列表的核心附件类型。
///
/// # Returns
/// 返回值
/// A comma-separated browser file accept list.
/// 逗号分隔的浏览器文件 accept 列表。
fn composer_attachment_kind_accept(kind: &ConversationAttachmentKind) -> &'static str {
    match kind {
        ConversationAttachmentKind::Image => "image/png,image/jpeg,image/webp,image/gif,image/bmp",
        ConversationAttachmentKind::Audio => {
            "audio/mpeg,audio/wav,audio/mp4,audio/aac,audio/flac,audio/ogg"
        }
        ConversationAttachmentKind::Video => {
            "video/mp4,video/quicktime,video/x-matroska,video/webm,video/x-msvideo"
        }
        ConversationAttachmentKind::Pdf => "application/pdf",
    }
}

/// Converts the WebUI cleanup-kind selection into the runtime attachment cleanup selection.
/// 将 WebUI 清理类型选择转换为运行时附件清理选择。
///
/// # Parameters
/// 参数
/// `selection` is the WebUI cleanup-kind selection state.
/// `selection` 是 WebUI 的清理类型选择状态。
///
/// # Returns
/// 返回值
/// The runtime attachment cleanup selection.
/// 运行时附件清理选择。
fn webui_attachment_cleanup_selection(
    selection: WebuiAttachmentCleanupSelection,
) -> AttachmentCleanupSelection {
    AttachmentCleanupSelection {
        image: selection.image,
        audio: selection.audio,
        video: selection.video,
        pdf: selection.pdf,
    }
}

/// Converts a runtime cleanup preview into the WebUI response shape.
/// 将运行时清理预览转换为 WebUI 响应结构。
///
/// # Parameters
/// 参数
/// `preview` is the runtime cleanup preview report.
/// `preview` 是运行时清理预览报告。
///
/// # Returns
/// 返回值
/// The WebUI cleanup preview summary.
/// WebUI 清理预览摘要。
fn webui_attachment_cleanup_preview_summary(
    preview: AttachmentCleanupPreview,
) -> WebuiAttachmentCleanupPreviewSummary {
    let matched_byte_len_label = webui_attachment_cleanup_byte_len_label(preview.matched_byte_len);
    let summary = format!(
        "截止 {cutoff} 将匹配 {count} 个附件、约 {byte_len}、影响 {sessions} 个会话（{kind_summary}）",
        cutoff = preview.cutoff_date_text,
        count = preview.matched_asset_count,
        byte_len = matched_byte_len_label,
        sessions = preview.impacted_session_count,
        kind_summary = webui_attachment_cleanup_kind_summary(&preview.per_kind_counts),
    );
    WebuiAttachmentCleanupPreviewSummary {
        cutoff_date: preview.cutoff_date_text,
        matched_asset_count: preview.matched_asset_count,
        matched_byte_len_label,
        impacted_session_count: preview.impacted_session_count,
        per_kind_counts: preview.per_kind_counts,
        summary,
    }
}

/// Converts a runtime cleanup report into the WebUI response shape.
/// 将运行时清理报告转换为 WebUI 响应结构。
///
/// # Parameters
/// 参数
/// `report` is the runtime cleanup execution report.
/// `report` 是运行时清理执行报告。
///
/// # Returns
/// 返回值
/// The WebUI cleanup execution summary.
/// WebUI 清理执行摘要。
fn webui_attachment_cleanup_report_summary(
    report: AttachmentCleanupReport,
) -> WebuiAttachmentCleanupReportSummary {
    let purged_byte_len_label = webui_attachment_cleanup_byte_len_label(report.purged_byte_len);
    let failed_delete_suffix = if report.failed_delete_count == 0 {
        String::new()
    } else {
        format!(
            "，{} 个文件删除失败但已撤销回放",
            report.failed_delete_count
        )
    };
    let summary = format!(
        "已按 {cutoff} 清理 {purged} 个附件，删除 {deleted} 个托管文件，{missing} 个文件已缺失，共释放约 {byte_len}，影响 {sessions} 个会话{failed_delete_suffix}",
        cutoff = report.cutoff_date_text,
        purged = report.purged_asset_count,
        deleted = report.deleted_file_count,
        missing = report.already_missing_file_count,
        byte_len = purged_byte_len_label,
        sessions = report.impacted_session_count,
    );
    WebuiAttachmentCleanupReportSummary {
        cutoff_date: report.cutoff_date_text,
        purged_asset_count: report.purged_asset_count,
        deleted_file_count: report.deleted_file_count,
        already_missing_file_count: report.already_missing_file_count,
        failed_delete_count: report.failed_delete_count,
        purged_byte_len_label,
        impacted_session_count: report.impacted_session_count,
        summary,
    }
}

/// Formats attachment cleanup byte counts for the WebUI.
/// 为 WebUI 格式化附件清理字节数。
///
/// # Parameters
/// 参数
/// `byte_len` is the raw byte count to display.
/// `byte_len` 是要展示的原始字节数。
///
/// # Returns
/// 返回值
/// A human-readable binary byte-size label.
/// 人类可读的二进制字节大小标签。
fn webui_attachment_cleanup_byte_len_label(byte_len: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let byte_len_f64 = byte_len as f64;
    if byte_len_f64 >= GIB {
        format!("{:.2} GiB", byte_len_f64 / GIB)
    } else if byte_len_f64 >= MIB {
        format!("{:.2} MiB", byte_len_f64 / MIB)
    } else if byte_len_f64 >= KIB {
        format!("{:.2} KiB", byte_len_f64 / KIB)
    } else {
        format!("{byte_len} B")
    }
}

/// Formats per-kind cleanup counts for the WebUI summary line.
/// 为 WebUI 摘要行格式化按类型统计数量。
///
/// # Parameters
/// 参数
/// `per_kind_counts` maps attachment kind keys to matched asset counts.
/// `per_kind_counts` 将附件类型键映射到匹配资产数量。
///
/// # Returns
/// 返回值
/// A compact per-kind count label for the WebUI summary line.
/// WebUI 摘要行使用的紧凑按类型计数字符串。
fn webui_attachment_cleanup_kind_summary(per_kind_counts: &BTreeMap<String, usize>) -> String {
    if per_kind_counts.is_empty() {
        String::from("未匹配任何类型")
    } else {
        per_kind_counts
            .iter()
            .map(|(kind, count)| format!("{kind}:{count}"))
            .collect::<Vec<_>>()
            .join(" / ")
    }
}

/// Extracts composer attachments from durable transcript metadata.
/// 从持久转录元数据中提取输入区附件。
///
/// # Parameters
/// 参数
/// `metadata` is the durable transcript metadata JSON object when available.
/// `metadata` 是可用时的持久转录元数据 JSON 对象。
///
/// # Returns
/// 返回值
/// Composer attachment summaries extracted from supported metadata fields.
/// 从受支持元数据字段中提取的输入区附件摘要。
pub(super) fn composer_message_attachments_from_metadata(
    metadata: Option<&serde_json::Value>,
) -> Vec<ComposerAttachmentSummary> {
    let Some(metadata) = metadata else {
        return Vec::new();
    };
    let Some(attachment_value) = metadata
        .get("attachments")
        .or_else(|| metadata.get("transcript_attachments"))
    else {
        return Vec::new();
    };
    serde_json::from_value::<Vec<ConversationAttachment>>(attachment_value.clone())
        .unwrap_or_default()
        .iter()
        .map(composer_attachment_summary_from_core)
        .collect()
}

/// Creates composer attachment gates using the same capability resolver as runtime execution.
/// 使用与运行时执行相同的能力解析器创建输入区附件门控。
///
/// # Parameters
/// 参数
/// `provider_configuration` contains the active model and provider settings.
/// `provider_configuration` 包含当前模型与 Provider 设置。
/// `ai_tool_configuration` contains the persisted AI tool bridge configuration.
/// `ai_tool_configuration` 包含持久化 AI 工具桥接配置。
///
/// # Returns
/// 返回值
/// Attachment capability summaries for the composer controls.
/// 输入区控件使用的附件能力摘要。
pub(super) fn composer_attachment_capabilities(
    provider_configuration: &ProviderConfigurationFile,
    ai_tool_configuration: &PersistedAiToolConfiguration,
) -> Vec<ComposerAttachmentCapabilitySummary> {
    let capabilities =
        active_workbench_model_capabilities(provider_configuration, ai_tool_configuration);
    [
        ConversationAttachmentKind::Image,
        ConversationAttachmentKind::Video,
        ConversationAttachmentKind::Audio,
        ConversationAttachmentKind::Pdf,
    ]
    .into_iter()
    .map(|kind| {
        let native = capabilities
            .as_ref()
            .is_some_and(|capabilities| capabilities.supports_attachment_kind(kind.clone()));
        let enabled = capabilities
            .as_ref()
            .is_some_and(|capabilities| capabilities.allows_staged_attachment_kind(kind.clone()));
        ComposerAttachmentCapabilitySummary {
            kind: kind.modality_name().to_string(),
            label: composer_attachment_kind_label(&kind).to_string(),
            enabled,
            native,
            bridged: enabled && !native,
            accept: composer_attachment_kind_accept(&kind).to_string(),
        }
    })
    .collect()
}
