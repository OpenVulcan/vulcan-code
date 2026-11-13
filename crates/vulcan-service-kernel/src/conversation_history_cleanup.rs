//! User-facing operations on the independent session history.
//! 面向用户的独立会话历史操作。

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use vulcan_session_core::v1::{
    event::{ConversationEvent, EventBody, TurnStatsEvent, UserInputSource},
    history_cleanup::{CleanedEventRange, CleanedHistory, eligible_turns, protected_start},
    ids::Seq,
};
use vulcan_workspace_store::workspace_store::load_session_v1_full;

use crate::{
    conversation_events_v1::resolve_exact_locator_and_registry,
    runtime_workspace_helpers::runtime_parts_from_state,
    session_v1_lifecycle::get_or_load_session_v1, state::SharedServiceBridgeState,
};

/// A strictly parsed request whose operation chooses the required arguments.
/// 严格解析的请求，操作类型决定必填参数。
#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ConversationHistoryRequest {
    /// Reads the timeline replacement and its coverage without opening the execution stream.
    /// 读取替换时间线及其覆盖信息，不打开执行事件流。
    Coverage {
        /// Exact indexed session identity.
        /// 精确的索引会话标识。
        session_id: String,
    },
    /// Read independent entries and the eligible source turn endpoints.
    /// 读取独立条目和可清理的来源回合端点。
    Read {
        /// Exact indexed session identity.
        /// 精确的索引会话标识。
        session_id: String,
    },
    /// Clean the oldest remaining turns through the previewed endpoint.
    /// 从最早剩余回合清理到预览选定的终点。
    Clean {
        /// Exact indexed session identity.
        /// 精确的索引会话标识。
        session_id: String,
        /// Last selected turn's end.
        /// 末个所选回合的终点。
        through: Seq,
        /// Previewed runtime generation.
        /// 预览时的运行时代次。
        generation: u64,
        /// Previewed runtime revision.
        /// 预览时的运行时修订。
        revision: u64,
    },
    /// Remove one or several independent display entries.
    /// 移除一条或多条独立展示记录。
    Delete {
        /// Exact indexed session identity.
        /// 精确的索引会话标识。
        session_id: String,
        /// Selected independent IDs.
        /// 选定的独立标识。
        ids: BTreeSet<Uuid>,
    },
}

/// The selectable source boundary and time of one old turn.
/// 一个旧回合可选的来源边界及时间。
#[derive(Serialize)]
struct HistorySourceTurn {
    /// Inclusive terminal position in the source.
    /// 来源中的包含式结束位置。
    through: Seq,
    /// Beginning of the root or scheduled prompt, or empty for a turn with no prompt text.
    /// 根提示或调度提示的开头，无提示正文时为空。
    prompt_preview: String,
    /// Prompt time, or the turn start time for a turn without an initiating user input.
    /// 提示词时间，无发起用户输入时取回合开始时间。
    started_at_ms: i64,
}

/// Builds a bounded label for the selected closure from its own initiating input.
/// 从回合自身的发起输入为所选闭合记录构建限长标签。
///
/// The stream supplies original events. Returns a plain-text label and a stable cutoff.
/// 事件流提供原始事件，返回纯文本标签与稳定的截止位置。
fn source_turn_preview(
    events: &[ConversationEvent],
    closure: &ConversationEvent,
    stats: &TurnStatsEvent,
) -> HistorySourceTurn {
    // The closure defines the inclusive source span, even when its first event is a host fact.
    // 闭合记录界定包含式来源区间，首事件可能是宿主事实。
    let start = events.partition_point(|event| event.seq < stats.start_seq);
    // Find the actual initiating prompt instead of using a supplementary input as the label.
    // 定位实际发起提示词，避免把追加输入当作标签。
    let prompt = events[start..]
        .iter()
        .take_while(|event| event.seq <= stats.end_seq)
        .find_map(|event| match &event.event {
            EventBody::UserInput(input)
                if event.turn_id == closure.turn_id
                    && matches!(
                        input.input_source,
                        UserInputSource::Root | UserInputSource::QueuedScheduled
                    ) =>
            {
                Some((event.created_at_ms, input.text.as_str()))
            }
            _ => None,
        });
    // Keep the UI label brief and collapse newlines without copying the full prompt.
    // 折叠换行并限制界面标签长度，无需复制整段提示词。
    const PREVIEW_CHARS: usize = 72;
    // A turn without a user prompt still has a source timestamp and an empty preview.
    // 没有用户提示词的回合仍有来源时间，预览为空。
    let (started_at_ms, text) = prompt.unwrap_or((events[start].created_at_ms, ""));
    // Normalize whitespace in the prefix and use an ellipsis only when more text follows.
    // 归一化开头的空白，仅在后面还有正文时加省略号。
    let mut chars = text
        .split_whitespace()
        .flat_map(|word| word.chars().chain(std::iter::once(' ')));
    // Trim the trailing separator inserted after each word.
    // 移除每个词后插入的尾部分隔符。
    let prefix: String = chars.by_ref().take(PREVIEW_CHARS).collect();
    // Skip remaining separators before deciding whether the label was truncated.
    // 判断是否截断前先跳过剩余分隔符。
    let mut prompt_preview = prefix.trim_end().to_owned();
    if chars.any(|ch| !ch.is_whitespace()) {
        prompt_preview.push('…');
    }
    HistorySourceTurn {
        through: stats.end_seq,
        prompt_preview,
        started_at_ms,
    }
}

/// The independent history and its selectable source snapshot.
/// 独立历史及其可选择的来源快照。
#[derive(Serialize)]
pub(crate) struct ConversationHistoryResponse {
    /// Whether an active compaction makes history management applicable.
    /// 是否存在使历史管理适用的生效压缩。
    enabled: bool,
    /// Independent entries available for reading or deletion.
    /// 可读取或删除的独立条目。
    history: CleanedHistory,
    /// Source ranges hidden in the old timeline after replacement.
    /// 替换后在旧时间线中隐藏的来源范围。
    cleaned_ranges: Vec<CleanedEventRange>,
    /// Complete turns outside the protection boundary.
    /// 保护边界以外的完整回合。
    eligible: Vec<HistorySourceTurn>,
    /// Source generation for stale-selection rejection.
    /// 用于拒绝过时选择的来源代次。
    generation: u64,
    /// Source revision for stale-selection rejection.
    /// 用于拒绝过时选择的来源修订。
    revision: u64,
}

/// Executes one local history operation and returns the authoritative display state.
/// 执行一次本地历史操作并返回权威展示状态。
pub(crate) async fn service_conversation_history(
    state: SharedServiceBridgeState,
    request: ConversationHistoryRequest,
) -> Result<ConversationHistoryResponse, String> {
    // Route every operation through the exact indexed session owner.
    // 每个操作都通过精确的索引会话所有者路由。
    let raw_id = match &request {
        ConversationHistoryRequest::Coverage { session_id }
        | ConversationHistoryRequest::Read { session_id }
        | ConversationHistoryRequest::Clean { session_id, .. }
        | ConversationHistoryRequest::Delete { session_id, .. } => session_id,
    };
    // Obtain storage and the registry through the existing V1 conversation entry.
    // 通过既有 V1 会话入口取得存储和注册表。
    let (session_id, locator, registry) =
        resolve_exact_locator_and_registry(&state, raw_id).await?;
    // Timeline reads and deletions need only the display publication.
    // 时间线读取和移除只需要展示历史发布。
    let display_only = matches!(
        request,
        ConversationHistoryRequest::Coverage { .. } | ConversationHistoryRequest::Delete { .. }
    );
    if matches!(
        request,
        ConversationHistoryRequest::Clean { .. } | ConversationHistoryRequest::Delete { .. }
    ) {
        // Only mutations need a live owner and its publication locks.
        // 只有变更需要实时所有者及其发布锁。
        let (controller, _, _, _, _) = runtime_parts_from_state(&state)?;
        // Reuse the existing owner instead of creating a competing in-memory document.
        // 复用既有所有者，避免创建竞争的内存文档。
        let handle = get_or_load_session_v1(&controller, &registry, session_id)
            .await
            .map_err(|error| error.to_string())?;
        tokio::task::spawn_blocking(move || match request {
            ConversationHistoryRequest::Clean {
                through,
                generation,
                revision,
                ..
            } => handle.clean_history(through, generation, revision),
            ConversationHistoryRequest::Delete { ids, .. } => handle.delete_history_entries(&ids),
            ConversationHistoryRequest::Read { .. }
            | ConversationHistoryRequest::Coverage { .. } => Ok(()),
        })
        .await
        .map_err(|error| error.to_string())??;
    }
    if display_only {
        return tokio::task::spawn_blocking(move || {
            // A single publication pairs the replacement rows with their hidden source ranges.
            // 同一次发布将替换条目与需隐藏的来源范围配对。
            let (metadata, enabled, history) = locator
                .read_cleaned_history_snapshot()
                .map_err(|error| error.to_string())?;
            Ok(ConversationHistoryResponse {
                enabled,
                history,
                cleaned_ranges: metadata
                    .cleaned_history
                    .map_or_else(Vec::new, |publication| publication.ranges),
                eligible: Vec::new(),
                generation: metadata.segment_generation,
                revision: metadata.document_revision,
            })
        })
        .await
        .map_err(|error| error.to_string())?;
    }
    // Use the full durable source for protected-boundary and complete-turn inspection.
    // 使用完整持久来源检查保护边界和完整回合。
    tokio::task::spawn_blocking(move || {
        // Read the history file selected by this exact loaded document.
        // 读取这份精确加载文档选定的历史文件。
        let loaded = load_session_v1_full(&locator).map_err(|error| error.to_string())?;
        // The publication revision ties the display data to its source coverage.
        // 发布修订将展示数据绑定到来源覆盖范围。
        let history = loaded
            .metadata
            .cleaned_history
            .as_ref()
            .map(|publication| locator.read_cleaned_history_version(publication))
            .transpose()
            .map_err(|error| error.to_string())?
            .unwrap_or_default();
        // Already-cleaned ranges are internal coverage, not presentation identities.
        // 已清理范围是内部覆盖信息，展示使用独立标识。
        let cleaned_ranges = loaded
            .metadata
            .cleaned_history
            .as_ref()
            .map_or_else(Vec::new, |publication| publication.ranges.clone());
        // No committed compaction means there are no eligible turns.
        // 没有已提交压缩意味着不存在可清理回合。
        let boundary = protected_start(&loaded.document).map_err(|error| error.to_string())?;
        // Display only complete closed turns outside every existing coverage interval.
        // 只展示各已覆盖范围以外的完整已结束回合。
        let eligible = eligible_turns(&loaded.document, boundary)
            .map(|(closure, stats)| source_turn_preview(loaded.document.events(), closure, stats))
            .collect();
        Ok(ConversationHistoryResponse {
            enabled: boundary.is_some(),
            history,
            cleaned_ranges,
            eligible,
            generation: loaded.metadata.segment_generation,
            revision: loaded.metadata.document_revision,
        })
    })
    .await
    .map_err(|error| error.to_string())?
}
