//! Read-only, authorization-aware waiting on the published session frontier.
//! 对已发布会话前沿执行只读且持续校验授权的等待。

use super::*;
use vulcan_agent_runtime::tooling::collaboration::WaitSessionArgs;
use vulcan_workspace_store::workspace_store::SessionV1EventWindowState;

/// Reads the persisted frontier of `locator` for `invocation`, checking current Read permission.
/// 为 `invocation` 读取 `locator` 的持久前沿，并检查当前读取权限。
/// Returns metadata from one validated publication, never an unflushed in-memory revision.
/// 返回一次已校验发布的元数据，绝不返回尚未刷写的内存修订。
async fn read_frontier(
    invocation: &ValidatedInvocation,
    locator: &SessionV1StorageLocator,
    target_session_id: Uuid,
) -> Result<SessionV1EventWindowState, String> {
    // Cross-project permission is checked on both sides of the blocking disk read so revocation
    // while storage is busy cannot expose another snapshot under an old admission decision.
    // 跨项目权限在阻塞磁盘读取前后均检查，避免存储繁忙期间撤销后仍按旧准入公开新快照。
    require_read_permission(invocation, locator, target_session_id)?;
    let read_locator = locator.clone();
    let frontier = tokio::task::spawn_blocking(move || read_locator.event_window_state())
        .await
        .map_err(|error| format!("session wait storage task failed: {error}"))?
        .map_err(|error| format!("failed to read published session wait frontier: {error}"))?;
    require_read_permission(invocation, locator, target_session_id)?;
    Ok(frontier)
}

/// Checks the source-owned Read grant for the exact target; same-project reads need no grant.
/// 检查精确目标的来源持有读取授权；同项目读取无需额外授权。
/// Returns an explicit denial without changing either session or its grants.
/// 返回明确拒绝，不改变任何会话或授权。
fn require_read_permission(
    invocation: &ValidatedInvocation,
    locator: &SessionV1StorageLocator,
    target_session_id: Uuid,
) -> Result<(), String> {
    // A long wait cannot retain authorization from an owner removed or replaced by lifecycle work.
    // 长等待不能继续使用被生命周期操作移除或替换的旧所有者授权。
    let canonical = invocation
        .registry
        .get(invocation.source_session_id)
        .map_err(|error| format!("session wait source owner is unavailable: {error}"))?;
    if !Arc::ptr_eq(&canonical, &invocation.source_handle) {
        return Err(String::from("session wait source owner has been replaced"));
    }
    if locator.project_id() != invocation.source_locator.project_id() {
        // A pending grant publication is not authority; an in-memory revocation denies immediately.
        // 尚未发布的授权不是权威；内存中的撤销则立即拒绝。
        let allowed = canonical
            .read(|document| {
                document.collaboration_state().grants.values().any(|grant| {
                    grant.source_session_id == invocation.source_session_id.to_string()
                        && grant.target_session_id == target_session_id.to_string()
                        && grant.created_revision <= document.durable_revision()
                        && grant.revoked_at_ms.is_none()
                        && grant.actions.contains(&CollaborationGrantAction::Read)
                })
            })
            .map_err(|error| format!("failed to read session wait grant: {error}"))?;
        if !allowed {
            return Err(String::from(
                "session wait requires an active source-owned cross-project Read grant",
            ));
        }
    }
    Ok(())
}

/// Waits for `args.session_id` after its supplied or newly captured durable revision.
/// 在传入或本次捕获的可靠修订之后等待 `args.session_id`。
/// Returns a changed/reset frontier, an actionable inbound question, or a bounded timeout.
/// 返回变化或重置前沿、可行动入站问题，或有限超时。
/// This observation creates no task, cancels no question and never consumes an inbound delivery.
/// 此观察不创建任务、不撤销问题，也不消费入站投递。
pub(super) async fn wait_session(
    invocation: &ValidatedInvocation,
    args: WaitSessionArgs,
) -> Result<Value, String> {
    // Start the deadline before locator resolution: storage latency counts toward the wait budget.
    // 在解析定位器之前开始计时：存储延迟计入等待预算。
    let deadline = tokio::time::Instant::now()
        .checked_add(std::time::Duration::from_millis(args.timeout_ms))
        .ok_or_else(|| String::from("session wait timeout exceeds runtime instant range"))?;
    let target_session_id = Uuid::parse_str(&args.session_id)
        .map_err(|error| format!("invalid wait target session_id: {error}"))?;
    let locator = resolve_read_target(invocation, target_session_id).await?;
    let initial = read_frontier(invocation, &locator, target_session_id).await?;
    let baseline = args.after_revision.unwrap_or(initial.document_revision);
    let initial_generation = initial.segment_generation;
    let initial_revision = initial.document_revision;
    let mut frontier = initial;
    loop {
        // Preserve the original cursor separately from the incoming question's own revision.
        // 单独保留原等待游标，不与入站问题自身修订混淆。
        if let Some(mut incoming) = incoming_question_snapshot(invocation).await? {
            require_read_permission(invocation, &locator, target_session_id)?;
            let fields = incoming
                .as_object_mut()
                .ok_or_else(|| String::from("incoming question snapshot must be an object"))?;
            fields.insert("waited_session_id".into(), json!(target_session_id));
            fields.insert("after_revision".into(), json!(baseline));
            fields.insert("wait_status".into(), json!("incoming"));
            return Ok(incoming);
        }
        // History generation changes are distinct from a normal forward revision notification.
        // 历史代际变化与普通修订前进通知分别表达。
        let reset = frontier.segment_generation != initial_generation
            || frontier.document_revision < initial_revision;
        let changed = frontier.document_revision > baseline;
        let now = tokio::time::Instant::now();
        if reset || changed || now >= deadline {
            require_read_permission(invocation, &locator, target_session_id)?;
            return Ok(json!({
                "status": if reset || changed { "ok" } else { "timeout" },
                "wait_status": if reset { "reset" } else if changed { "changed" } else { "timeout" },
                "session_id": target_session_id,
                "project_id": locator.project_id(),
                "after_revision": baseline,
                "document_revision": frontier.document_revision,
                "segment_generation": frontier.segment_generation,
                "next_seq": frontier.next_seq,
                "open_turn_start_seq": frontier.open_turn_start_seq.map(Seq::value),
                "operation_id": invocation.operation_id,
            }));
        }
        tokio::time::sleep(
            deadline
                .saturating_duration_since(now)
                .min(COLLABORATION_WAIT_RECHECK_INTERVAL),
        )
        .await;
        frontier = read_frontier(invocation, &locator, target_session_id).await?;
    }
}

#[cfg(test)]
mod tests;
