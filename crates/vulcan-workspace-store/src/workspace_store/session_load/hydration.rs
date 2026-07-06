//! Session hydration entry points and snapshot assembly.
//! 会话水合入口与快照组装逻辑。

use super::*;
use std::collections::BTreeSet;

/// Lazily loads one session by its deterministic project, created-at, and session id.
/// 按确定性的项目、创建时间与会话标识懒加载单个会话。
///
/// # Parameters
/// 参数
/// `controller` owns the workspace-store runtime bridge.
/// `controller` 持有工作区存储运行时桥。
/// `project_id` is the known owning project identifier used as the segmented project tag.
/// `project_id` 是已知的所属项目标识，用作分段项目标签。
/// `created_at` is the raw Unix-milliseconds creation timestamp from the session summary.
/// `created_at` 是会话摘要中的原始 Unix 毫秒创建时间戳。
/// `session_id` selects the session to hydrate.
/// `session_id` 选择要水合的会话。
/// `requested_limit` is the requested recent-turn window size before clamping.
/// `requested_limit` 是裁剪前请求的最近轮次窗口大小。
///
/// # Returns
/// 返回值
/// A hydration snapshot for the target session, or an empty snapshot if no segmented directory exists.
/// 返回目标会话的水合快照；若分段目录不存在则返回空快照。
pub async fn load_session_hydration_snapshot_for_project_created_at(
    controller: ControllerRuntimeHandle,
    project_id: &str,
    created_at: &str,
    session_id: SessionId,
    requested_limit: usize,
) -> Result<SessionHydrationSnapshot, String> {
    let locator = query_session_locator_for_project_created_at(
        &controller,
        project_id,
        created_at,
        session_id,
    )
    .await?;
    load_session_hydration_snapshot_from_locator(controller, session_id, requested_limit, locator)
        .await
}

/// Lazily loads a display-complete transcript window by deterministic session identity.
/// 通过确定性会话身份懒加载展示完整的转录窗口。
///
/// # Parameters
/// 参数
/// `controller` owns the workspace-store runtime bridge.
/// `controller` 持有工作区存储运行时桥。
/// `project_id` is the known owning project identifier used as the segmented project tag.
/// `project_id` 是已知的所属项目标识，用作分段项目标签。
/// `created_at` is the raw Unix-milliseconds creation timestamp from the session summary.
/// `created_at` 是会话摘要中的原始 Unix 毫秒创建时间戳。
/// `session_id` selects the session to hydrate.
/// `session_id` 选择要水合的会话。
/// `requested_limit` is the requested recent-turn window size before clamping.
/// `requested_limit` 是裁剪前请求的最近轮次窗口大小。
///
/// # Returns
/// 返回值
/// A display-complete hydration snapshot for the target session.
/// 返回目标会话的展示完整水合快照。
pub async fn load_session_display_hydration_snapshot_for_project_created_at(
    controller: ControllerRuntimeHandle,
    project_id: &str,
    created_at: &str,
    session_id: SessionId,
    requested_limit: usize,
) -> Result<SessionHydrationSnapshot, String> {
    let locator = query_session_locator_for_project_created_at(
        &controller,
        project_id,
        created_at,
        session_id,
    )
    .await?;
    load_session_hydration_snapshot_from_locator(controller, session_id, requested_limit, locator)
        .await
}

/// Builds a session hydration snapshot from an already resolved segmented locator.
/// 根据已经解析出的分段定位信息构建会话水合快照。
///
/// # Parameters
/// 参数
/// `controller` owns the workspace-store runtime bridge.
/// `controller` 持有工作区存储运行时桥。
/// `session_id` selects the session to hydrate.
/// `session_id` 选择要水合的会话。
/// `requested_limit` is the requested recent-turn window size before clamping.
/// `requested_limit` 是裁剪前请求的最近轮次窗口大小。
/// `locator` points at the segmented session directory when one was found.
/// `locator` 在找到分段会话目录时指向该目录。
///
/// # Returns
/// 返回值
/// A hydration snapshot assembled from the locator and segmented state.
/// 返回基于定位信息与分段状态组装出的水合快照。
async fn load_session_hydration_snapshot_from_locator(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
    requested_limit: usize,
    locator: Option<StoredSessionLocatorRow>,
) -> Result<SessionHydrationSnapshot, String> {
    let requested_limit = requested_limit.clamp(1, SESSION_TRANSCRIPT_WINDOW_MAX_TURN_LIMIT);
    let compactions = Vec::new();
    let target_turn_count =
        resolve_initial_hydration_turn_count(requested_limit, &compactions, locator.as_ref());
    let segmented_window = load_segmented_window_messages(
        &controller,
        locator.as_ref(),
        session_id,
        None,
        target_turn_count,
    )?;
    let (
        has_more_history,
        mut messages,
        mut tool_detail_messages,
        earliest_ordinal,
        full_messages,
        boundary_conversation_state,
    ) = segmented_window
        .map(|window| {
            (
                window.has_more_history,
                window.messages,
                window.tool_detail_messages,
                window.earliest_ordinal,
                window.full_messages,
                window.boundary_conversation_state,
            )
        })
        .unwrap_or_else(|| (false, Vec::new(), Vec::new(), None, Vec::new(), None));
    let runtime_state =
        match load_segmented_runtime_state(&controller, locator.as_ref(), session_id)? {
            Some(runtime_state) => Some(runtime_state),
            None => hydrate_session_runtime_state(session_id, &full_messages).await?,
        }
        .map(|mut runtime_state| {
            if let Some(boundary_conversation_state) = boundary_conversation_state {
                apply_boundary_conversation_projection_to_runtime_state(
                    &mut runtime_state,
                    boundary_conversation_state,
                );
            }
            runtime_state
        });
    restore_hydration_window_from_runtime_state_if_empty(
        &mut messages,
        &mut tool_detail_messages,
        runtime_state.as_ref(),
    );

    Ok(SessionHydrationSnapshot {
        session_id,
        messages,
        tool_detail_messages,
        compactions,
        runtime_state,
        earliest_ordinal,
        has_more_history,
    })
}

/// Repairs duplicate canonical record ids inside one loaded conversation state so older broken sessions can recover during hydration.
/// 修复单份已加载会话状态中的重复规范记录标识，确保旧的损坏会话能在水合阶段自动恢复。
pub(super) fn repair_duplicate_conversation_record_ids(
    session_id: SessionId,
    conversation_state: &mut SessionConversationState,
) -> bool {
    let mut seen_ids = BTreeSet::new();
    let mut repaired = false;

    for (record_index, record) in conversation_state.records.iter_mut().enumerate() {
        if seen_ids.insert(record.id.clone()) {
            continue;
        }

        let created_at = non_empty_timestamp_text(&record.created_at);
        let mut candidate_id = format!("session-{session_id}-message-{created_at}-{record_index}");
        let mut collision_suffix = 1usize;
        while !seen_ids.insert(candidate_id.clone()) {
            candidate_id = format!(
                "session-{session_id}-message-{created_at}-{record_index}-{collision_suffix}"
            );
            collision_suffix = collision_suffix.saturating_add(1);
        }
        record.id = candidate_id;
        repaired = true;
    }

    repaired
}
