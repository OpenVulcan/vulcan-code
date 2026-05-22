//! Generic tool transcript projection used for MCP, unknown, and fallback tool rows.
//! 供 MCP、未知工具以及兜底工具行使用的通用工具转录投影。

use super::AgentViewBuildOptions;
use super::tool_shared::{
    build_base_tool_attributes, build_tool_directive_block, host_result_payload_by_kind,
};
use crate::model::SessionMessageEntry;
use crate::tooling::change_set::{ChangeSetMode, change_set_modal_summary_from_payload};
use vulcan_agent_view::{
    AgentViewDirectiveBlock, AgentViewDirectiveKind, AgentViewToolPresentationKind,
};

/// Builds one generic tool transcript block when no dedicated presentation family applies.
/// 当不存在专属展示家族时，构建一条通用工具转录区块。
pub(crate) fn build_generic_tool_block(
    kind: AgentViewDirectiveKind,
    message: &SessionMessageEntry,
    options: &AgentViewBuildOptions,
) -> AgentViewDirectiveBlock {
    let mut attributes =
        build_base_tool_attributes(message, AgentViewToolPresentationKind::Generic, options);
    if let Some(payload) = host_result_payload_by_kind(message, "change_set") {
        append_change_set_summary_attributes(&mut attributes, payload, options);
    }
    build_tool_directive_block(kind, attributes)
}

/// Appends compact `change_set` summary attributes onto one generic tool block so the transcript renderer can show inline file-change statistics without loading the heavy tool body.
/// 将紧凑 `change_set` 摘要属性追加到通用工具区块上，使转录渲染器无需加载重型工具正文也能展示内联文件变更统计。
fn append_change_set_summary_attributes(
    attributes: &mut vulcan_agent_view::AgentViewAttributes,
    payload: &serde_json::Value,
    options: &AgentViewBuildOptions,
) {
    let Some(summary) = change_set_modal_summary_from_payload(payload) else {
        return;
    };
    attributes.insert(
        String::from("tool-summary-text"),
        localized_change_set_summary_text(&summary, options.locale),
    );
    attributes.insert(
        String::from("change-set-mode"),
        summary.mode.as_str().to_string(),
    );
    attributes.insert(
        String::from("change-set-file-count"),
        summary.stats.file_count.to_string(),
    );
    attributes.insert(
        String::from("change-set-create-count"),
        summary.stats.create_count.to_string(),
    );
    attributes.insert(
        String::from("change-set-modify-count"),
        summary.stats.modify_count.to_string(),
    );
    attributes.insert(
        String::from("change-set-delete-count"),
        summary.stats.delete_count.to_string(),
    );
    attributes.insert(
        String::from("change-set-rename-count"),
        summary.stats.rename_count.to_string(),
    );
    attributes.insert(
        String::from("change-set-inserted-line-count"),
        summary.stats.inserted_line_count.to_string(),
    );
    attributes.insert(
        String::from("change-set-deleted-line-count"),
        summary.stats.deleted_line_count.to_string(),
    );
    if let Some(summary_text) = summary.summary.as_deref() {
        let trimmed = summary_text.trim();
        if !trimmed.is_empty() {
            attributes.insert(String::from("change-set-summary"), trimmed.to_string());
        }
    }
}

/// Builds one locale-aware single-line inline summary for a compact `change_set` tool row.
/// 为紧凑 `change_set` 工具行构建一条带语言感知的单行内联摘要。
fn localized_change_set_summary_text(
    summary: &crate::tooling::change_set::ChangeSetModalSummary,
    locale: crate::AppLocale,
) -> String {
    match locale {
        crate::AppLocale::ZhCn => {
            let mut parts = vec![match summary.mode {
                ChangeSetMode::Preview => String::from("预览文件变更"),
                ChangeSetMode::Applied => String::from("已应用文件变更"),
            }];
            parts.push(format!("{} 文件", summary.stats.file_count));
            if summary.stats.create_count > 0 {
                parts.push(format!("新增 {}", summary.stats.create_count));
            }
            if summary.stats.modify_count > 0 {
                parts.push(format!("修改 {}", summary.stats.modify_count));
            }
            if summary.stats.delete_count > 0 {
                parts.push(format!("删除 {}", summary.stats.delete_count));
            }
            if summary.stats.rename_count > 0 {
                parts.push(format!("重命名 {}", summary.stats.rename_count));
            }
            if summary.stats.inserted_line_count > 0 || summary.stats.deleted_line_count > 0 {
                parts.push(format!(
                    "+{} / -{}",
                    summary.stats.inserted_line_count, summary.stats.deleted_line_count
                ));
            }
            parts.join(" · ")
        }
        crate::AppLocale::EnUs => {
            let mut parts = vec![match summary.mode {
                ChangeSetMode::Preview => String::from("Previewed file changes"),
                ChangeSetMode::Applied => String::from("Applied file changes"),
            }];
            parts.push(format!("{} files", summary.stats.file_count));
            if summary.stats.create_count > 0 {
                parts.push(format!("create {}", summary.stats.create_count));
            }
            if summary.stats.modify_count > 0 {
                parts.push(format!("modify {}", summary.stats.modify_count));
            }
            if summary.stats.delete_count > 0 {
                parts.push(format!("delete {}", summary.stats.delete_count));
            }
            if summary.stats.rename_count > 0 {
                parts.push(format!("rename {}", summary.stats.rename_count));
            }
            if summary.stats.inserted_line_count > 0 || summary.stats.deleted_line_count > 0 {
                parts.push(format!(
                    "+{} / -{}",
                    summary.stats.inserted_line_count, summary.stats.deleted_line_count
                ));
            }
            parts.join(" · ")
        }
    }
}
