//! Attachment cleanup service helpers.
//! 附件清理服务辅助逻辑。

use super::*;

/// WebUI request body for project-scoped attachment cleanup operations.
/// WebUI 项目级附件清理操作的请求体。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WebuiProjectAttachmentCleanupRequest {
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
pub(crate) struct WebuiAttachmentCleanupSelection {
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
pub(crate) struct WebuiAttachmentCleanupPreviewSummary {
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
pub(crate) struct WebuiAttachmentCleanupReportSummary {
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

/// Shared resolved context for one attachment cleanup operation.
/// 单次附件清理操作的共享解析上下文。
struct WebuiAttachmentCleanupOperationContext {
    /// Project root path text that owns the managed attachment index.
    /// 拥有托管附件索引的项目根路径文本。
    project_path: String,
    /// Normalized cutoff date text passed to the runtime cleanup layer.
    /// 传递给运行时清理层的规范化截止日期文本。
    cutoff_date: String,
    /// Runtime attachment-kind selection derived from the WebUI request.
    /// 从 WebUI 请求派生的运行时附件类型选择。
    selection: AttachmentCleanupSelection,
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
pub(crate) async fn service_preview_project_attachment_cleanup(
    state: SharedServiceBridgeState,
    request: WebuiProjectAttachmentCleanupRequest,
) -> Result<WebuiAttachmentCleanupPreviewSummary, String> {
    run_webui_attachment_cleanup_operation(
        state,
        request,
        "附件清理预览任务失败",
        |context| {
            let preview = preview_attachment_cleanup(
                &context.project_path,
                &context.cutoff_date,
                context.selection,
            )?;
            Ok(webui_attachment_cleanup_preview_summary(preview))
        },
    )
    .await
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
pub(crate) async fn service_execute_project_attachment_cleanup(
    state: SharedServiceBridgeState,
    request: WebuiProjectAttachmentCleanupRequest,
) -> Result<WebuiAttachmentCleanupReportSummary, String> {
    run_webui_attachment_cleanup_operation(
        state,
        request,
        "附件清理执行任务失败",
        |context| {
            let report = execute_attachment_cleanup(
                &context.project_path,
                &context.cutoff_date,
                context.selection,
            )?;
            Ok(webui_attachment_cleanup_report_summary(report))
        },
    )
    .await
}

/// Runs one project-scoped attachment cleanup operation on the blocking pool.
/// 在 blocking 线程池运行单个项目级附件清理操作。
/// Parameters: state and request resolve shared cleanup context, task_error_label names join failures, and operation performs runtime cleanup work.
/// 参数：state 和 request 解析共享清理上下文，task_error_label 命名 join 失败，operation 执行运行时清理工作。
/// Returns: the operation result after context resolution and blocking task completion.
/// 返回：完成上下文解析和 blocking 任务后的操作结果。
async fn run_webui_attachment_cleanup_operation<T, F>(
    state: SharedServiceBridgeState,
    request: WebuiProjectAttachmentCleanupRequest,
    task_error_label: &'static str,
    operation: F,
) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(WebuiAttachmentCleanupOperationContext) -> Result<T, String> + Send + 'static,
{
    let context = webui_attachment_cleanup_operation_context(&state, request)?;
    spawn_service_blocking_task(move || operation(context))
        .await
        .map_err(|error| format!("{task_error_label}：{error}"))?
}

/// Resolves a WebUI attachment cleanup request into shared runtime inputs.
/// 将 WebUI 附件清理请求解析为共享运行时输入。
fn webui_attachment_cleanup_operation_context(
    state: &SharedServiceBridgeState,
    request: WebuiProjectAttachmentCleanupRequest,
) -> Result<WebuiAttachmentCleanupOperationContext, String> {
    Ok(WebuiAttachmentCleanupOperationContext {
        project_path: vmm_project_scope_path(state, Some(&request.project_id))?,
        cutoff_date: webui_attachment_cleanup_cutoff_date(&request.cutoff_date),
        selection: webui_attachment_cleanup_selection(request.selection),
    })
}

/// Normalizes cleanup cutoff date text from the WebUI request.
/// 规范化 WebUI 请求中的清理截止日期文本。
fn webui_attachment_cleanup_cutoff_date(cutoff_date: &str) -> String {
    cutoff_date.trim().to_string()
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
        // Generic generated documents share the current WebUI document cleanup switch.
        // 通用生成文档共享当前 WebUI 文档清理开关。
        file: selection.pdf,
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
        let mut summary = String::new();
        for (kind, count) in per_kind_counts {
            if !summary.is_empty() {
                summary.push_str(" / ");
            }
            summary.push_str(kind);
            summary.push(':');
            summary.push_str(&count.to_string());
        }
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies cleanup cutoff date text is trimmed before runtime use.
    /// 验证清理截止日期文本在传给运行时前会被裁剪。
    #[test]
    fn webui_attachment_cleanup_cutoff_date_trims_text() {
        assert_eq!(
            webui_attachment_cleanup_cutoff_date("  2026-07-06  "),
            "2026-07-06"
        );
    }

    /// Verifies WebUI cleanup kind switches are mapped without inversion.
    /// 验证 WebUI 清理类型开关映射时不会反转。
    #[test]
    fn webui_attachment_cleanup_selection_maps_kind_flags() {
        let selection = webui_attachment_cleanup_selection(WebuiAttachmentCleanupSelection {
            image: true,
            audio: false,
            video: true,
            pdf: false,
        });

        assert!(selection.image);
        assert!(!selection.audio);
        assert!(selection.video);
        assert!(!selection.pdf);
        assert!(!selection.file);
    }

    /// Verifies cleanup kind summaries are formatted in stable key order.
    /// 验证清理类型摘要会按稳定键顺序格式化。
    #[test]
    fn webui_attachment_cleanup_kind_summary_formats_counts_in_key_order() {
        let mut per_kind_counts = BTreeMap::new();
        per_kind_counts.insert(String::from("video"), 2);
        per_kind_counts.insert(String::from("image"), 3);
        per_kind_counts.insert(String::from("audio"), 1);

        assert_eq!(
            webui_attachment_cleanup_kind_summary(&per_kind_counts),
            "audio:1 / image:3 / video:2"
        );
    }

    /// Verifies cleanup kind summaries keep the empty preview label.
    /// 验证清理类型摘要会保留空预览文案。
    #[test]
    fn webui_attachment_cleanup_kind_summary_keeps_empty_label() {
        assert_eq!(
            webui_attachment_cleanup_kind_summary(&BTreeMap::new()),
            "未匹配任何类型"
        );
    }
}
