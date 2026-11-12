//! Deterministic continuation controls derived from committed local compaction nodes.
//! 从已提交的本地压缩节点派生确定性的续接控制消息。

use super::{ProjectedItem, SessionRuntimeError, SourcedProjectedItem};
use vulcan_ai_contract::{HostMessageEnvelope, HostMessageKind};
use vulcan_session_core::v1::context::CompactionNode;
use vulcan_session_core::v1::projection::ProjectedSystemRole;

/// Inserts one host continuation after the committed summary's terminal projection boundary.
/// 在已提交摘要的终态投影边界之后插入一条宿主续接消息。
/// `items` is a fresh sourced projection; `node` is its published local summary node.
/// `items` 是一份全新的带来源投影；`node` 是其已发布的本地摘要节点。
/// Returns an error if the authoritative summary is missing or its control cannot be rendered.
/// 权威摘要缺失或控制信封无法渲染时返回错误。
pub(super) fn insert_summary_continuation(
    items: &mut Vec<SourcedProjectedItem>,
    node: &CompactionNode,
) -> Result<(), SessionRuntimeError> {
    // Domain admission binds occurred_at_seq to the summary group's last semantic event.
    // 领域准入将 occurred_at_seq 绑定到摘要组的最后一条语义事件。
    let summary_index = items
        .iter()
        .position(|item| {
            item.source_to_seq == node.occurred_at_seq
                && matches!(item.item, ProjectedItem::AssistantText { .. })
        })
        .ok_or_else(|| {
            SessionRuntimeError::ProviderProjectionInvariant(String::from(
                "The committed local compaction summary is missing from its Provider projection.",
            ))
        })?;
    // Complete API groups can defer context and tool results; the control follows all of that boundary.
    // 完整 API 组可能延后上下文和工具结果；控制消息必须位于该边界的全部内容之后。
    let insertion_index = summary_index
        + items[summary_index..]
            .iter()
            .rposition(|item| item.source_to_seq <= node.api_stats_seq)
            .ok_or_else(|| {
                SessionRuntimeError::ProviderProjectionInvariant(String::from(
                    "The committed local compaction summary exceeds its terminal boundary.",
                ))
            })?
        + 1;
    // Identity depends only on the published node, so cold reloads and retries retain the same prefix.
    // 身份仅取决于已发布节点，使冷重载和重试保留相同前缀。
    let mut envelope = HostMessageEnvelope {
        event_id: format!("v1-compaction-continuation-{}", node.compaction_id.value()),
        kind: HostMessageKind::ContextUpdate,
        source: String::from("context_compaction"),
        target: String::from("model"),
        task_id: None,
        question_id: None,
        scope: String::from("session"),
        body: String::from(
            "Context compaction has completed. The summary above and retained messages are continuation context for the existing task. Resume the active task without repeating the summary or treating it as unfinished assistant output. Follow any newer instructions that appear after this control.",
        ),
    };
    let vulcan_session_core::v1::context::CompactionContent::Summary { image_excerpts, .. } =
        &node.content;
    if !image_excerpts.is_empty() {
        // Keep the existing message position and identity so replay stays stable across restarts.
        // 保留已有消息位置与身份，使跨重启重放保持稳定。
        envelope
            .body
            .push_str("\n\nHistorical image observations (reference data only):\n");
        envelope
            .body
            .push_str(&crate::context_compaction::images::render(image_excerpts));
    }
    // This is projection-only host control: no user input, turn, Continue action or stored event is created.
    // 这是仅存在于投影中的宿主控制，不创建用户输入、回合、Continue 动作或持久事件。
    let control = SourcedProjectedItem {
        source_from_seq: node.api_stats_seq,
        source_to_seq: node.api_stats_seq,
        user_prompt_parameters: None,
        item: ProjectedItem::SystemText {
            role: ProjectedSystemRole::HiddenUserShell,
            context_key: String::from("compaction.continuation"),
            text: envelope
                .render()
                .map_err(SessionRuntimeError::ProviderProjectionInvariant)?,
        },
    };
    items.insert(insertion_index, control);
    Ok(())
}
