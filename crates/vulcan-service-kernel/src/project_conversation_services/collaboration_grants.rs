//! User-owned collaboration grant management over the canonical V1 session owner.
//! 基于规范 V1 会话 owner 的用户协作授权管理。

use super::*;

use serde_json::{Value, json};
use vulcan_session_core::v1::{
    IdempotentMutation,
    collaboration::{
        AddCollaborationGrantInput, CollaborationGrantAction, CollaborationGrantAuthority,
        CollaborationGrantId, CollaborationOperationId, RemoveCollaborationGrantInput,
    },
};

use crate::session_runtime_registry_v1::{
    SessionFlushReason, SessionHandle, SessionMutationAdmission, SessionMutationChange,
};
use crate::session_v1_lifecycle::get_or_load_session_v1;

/// Returns the active user grants owned by one exact source session.
/// 返回一个精确来源会话拥有的有效用户授权。
///
/// The source session is the sole durable authority for its grants. The result is read from the
/// canonical in-process owner so an already loaded session cannot race a separate disk reader.
///
/// # Parameters / 参数
///
/// `state` owns the controller and canonical runtime registry; `source_session_id` selects the
/// exact source session whose active grants are requested.
/// `state` 持有控制器与规范运行时注册表；`source_session_id` 选择要查询有效授权的精确来源会话。
///
/// # Returns / 返回值
///
/// Returns a JSON service response containing active source-owned grants.
/// 返回包含来源拥有有效授权的 JSON 服务响应。
/// 来源会话是其授权的唯一可靠权威。结果从规范进程内 owner 读取，避免已加载会话与独立磁盘读取器竞态。
pub(crate) async fn service_list_collaboration_grants(
    state: SharedServiceBridgeState,
    source_session_id: String,
) -> Result<Value, String> {
    let source_id = parse_session_id(&source_session_id, "sourceSessionId")?;
    let (controller, registry) = lifecycle_handles(&state)?;
    let source = get_or_load_session_v1(&controller, registry.as_ref(), source_id)
        .await
        .map_err(|error| format!("无法加载授权来源会话 `{source_session_id}`：{error}"))?;
    let grants = source
        .read(|document| {
            document
                .collaboration_state()
                .grants
                .values()
                .filter(|grant| grant.revoked_at_ms.is_none())
                .cloned()
                .collect::<Vec<_>>()
        })
        .map_err(|error| format!("读取会话协作授权失败：{error}"))?;
    Ok(json!({
        "status": "ok",
        "sourceSessionId": source_session_id,
        "grants": grants,
    }))
}

/// Creates one persistent user grant after validating both sessions through host-owned locators.
/// 通过宿主持有的定位器校验双方会话后创建一条持久用户授权。
///
/// Only this user RPC can create the grant. Model text, model tool arguments, and collaboration
/// envelopes never create authorization records.
///
/// # Parameters / 参数
///
/// `state` owns host validation and the canonical source owner; the remaining values select the
/// source, target, and user-selected action set.
/// `state` 持有宿主校验与规范来源 owner；其余参数选择来源、目标以及用户选择的动作集合。
///
/// # Returns / 返回值
///
/// Returns the source-owned committed grant and the host-verified participant facts.
/// 返回来源拥有的已提交授权以及宿主已校验的参与者事实。
/// 只有本用户 RPC 可以创建授权。模型文本、模型工具参数和协作信封都不能创建授权记录。
pub(crate) async fn service_add_collaboration_grant(
    state: SharedServiceBridgeState,
    source_session_id: String,
    target_session_id: String,
    operation_id: String,
    actions: std::collections::BTreeSet<CollaborationGrantAction>,
) -> Result<Value, String> {
    let source_id = parse_session_id(&source_session_id, "sourceSessionId")?;
    let target_id = parse_session_id(&target_session_id, "targetSessionId")?;
    if source_id == target_id {
        return Err(String::from("协作授权的来源会话与目标会话必须不同。"));
    }
    if actions.is_empty() {
        return Err(String::from("协作授权至少需要一个动作。"));
    }

    // The exclusion closes new-turn admission for both participants while the host validates
    // generation and durably publishes the source-owned grant.
    // 该排除在宿主校验代际并可靠发布来源授权期间关闭双方新回合准入。
    let _admission =
        begin_session_turn_start_exclusion(&state, [source_id, target_id], "协作授权添加")?;
    let (controller, registry) = lifecycle_handles(&state)?;
    let source = get_or_load_session_v1(&controller, registry.as_ref(), source_id)
        .await
        .map_err(|error| format!("无法加载授权来源会话 `{source_session_id}`：{error}"))?;
    let target = get_or_load_session_v1(&controller, registry.as_ref(), target_id)
        .await
        .map_err(|error| format!("无法加载授权目标会话 `{target_session_id}`：{error}"))?;
    let source_locator = source
        .storage_locator()
        .map_err(|error| format!("读取授权来源定位失败：{error}"))?;
    let target_locator = target
        .storage_locator()
        .map_err(|error| format!("读取授权目标定位失败：{error}"))?;
    let source_project_id = source_locator.project_id().to_string();
    let target_project_id = target_locator.project_id().to_string();
    let (source_generation, target_generation) = participant_generations(&source, &target)?;

    let now_ms = current_session_lifecycle_unix_millis()?;
    let operation_id =
        parse_user_grant_operation_id(operation_id, USER_GRANT_ADD_OPERATION_PREFIX)?;
    // The grant identity is derived from the host-validated operation identity so an exact WebUI
    // retry reuses the same durable record instead of allocating a second authorization.
    // 授权标识由宿主校验过的操作标识派生，确保 WebUI 精确重试复用同一持久记录，而不是重复授权。
    let grant_id = CollaborationGrantId::try_new(format!("grant:v1:{}", operation_id.as_str()))
        .map_err(|error| format!("failed to derive collaboration grant identity: {error}"))?;
    let input = AddCollaborationGrantInput {
        operation_id,
        grant_id,
        authority: CollaborationGrantAuthority::LocalUser,
        source_session_id: source_id.to_string(),
        target_session_id: target_id.to_string(),
        actions,
        created_at_ms: now_ms,
    };
    // Share source ownership with delivery and retain admission through actual disk publication.
    // 与投递共享来源所有权，并在实际磁盘发布期间保留准入。
    let source_for_write = Arc::clone(&source);
    let receipt = crate::agent_collaboration::lifecycle_write::with_canonical_handles(
        &registry,
        vec![Arc::clone(&source), Arc::clone(&target)],
        move || {
            let _admission = _admission;
            source_for_write
                .mutate_durable_if_changed(
                    SessionMutationAdmission::ControlPlane,
                    SessionFlushReason::MetadataChanged,
                    move |document| {
                        let mutation = document.add_collaboration_grant(input)?;
                        Ok(match mutation {
                            IdempotentMutation::Committed(value) => {
                                SessionMutationChange::Changed(value)
                            }
                            IdempotentMutation::Idempotent(value) => {
                                SessionMutationChange::Unchanged(value)
                            }
                        })
                    },
                )
                .map_err(|error| format!("可靠保存协作授权失败：{error}"))
        },
    )
    .await?;
    let grant = receipt.value.grant;
    Ok(json!({
        "status": "ok",
        "changed": receipt.changed,
        "grant": grant,
        "sourceSessionId": source_id.to_string(),
        "targetSessionId": target_id.to_string(),
        "sourceProjectId": source_project_id,
        "targetProjectId": target_project_id,
        "sourceSegmentGeneration": source_generation,
        "targetSegmentGeneration": target_generation,
        "eventSeq": receipt.value.event_seq,
        "revision": receipt.revision,
    }))
}

/// Revokes one source-owned grant while retaining its durable audit record.
/// 撤销一条来源拥有的授权，同时保留可靠审计记录。
///
/// # Parameters / 参数
///
/// `state` owns the canonical source owner; `source_session_id` and `grant_id` identify the
/// source-owned record to revoke.
/// `state` 持有规范来源 owner；`source_session_id` 与 `grant_id` 标识要撤销的来源授权记录。
///
/// # Returns / 返回值
///
/// Returns the revoked grant after durable publication.
/// 返回可靠发布后的已撤销授权。
pub(crate) async fn service_remove_collaboration_grant(
    state: SharedServiceBridgeState,
    source_session_id: String,
    grant_id: String,
    operation_id: String,
) -> Result<Value, String> {
    let source_id = parse_session_id(&source_session_id, "sourceSessionId")?;
    let _admission = begin_session_turn_start_exclusion(&state, [source_id], "协作授权撤销")?;
    let (controller, registry) = lifecycle_handles(&state)?;
    let source = get_or_load_session_v1(&controller, registry.as_ref(), source_id)
        .await
        .map_err(|error| format!("无法加载授权来源会话 `{source_session_id}`：{error}"))?;
    let grant_id = CollaborationGrantId::try_new(grant_id)
        .map_err(|error| format!("协作授权标识无效：{error}"))?;
    let operation_id =
        parse_user_grant_operation_id(operation_id, USER_GRANT_REMOVE_OPERATION_PREFIX)?;
    let revoked_at_ms = current_session_lifecycle_unix_millis()?;
    let input = RemoveCollaborationGrantInput {
        operation_id,
        grant_id,
        revoked_at_ms,
    };
    // Revocation uses the same source lifecycle gate as authorized message publication.
    // 撤销与已授权消息发布使用同一个来源生命周期锁。
    let source_for_write = Arc::clone(&source);
    let receipt = crate::agent_collaboration::lifecycle_write::with_canonical_handles(
        &registry,
        vec![Arc::clone(&source)],
        move || {
            let _admission = _admission;
            source_for_write
                .mutate_durable_if_changed(
                    SessionMutationAdmission::ControlPlane,
                    SessionFlushReason::MetadataChanged,
                    move |document| {
                        let mutation = document.remove_collaboration_grant(input)?;
                        Ok(match mutation {
                            IdempotentMutation::Committed(value) => {
                                SessionMutationChange::Changed(value)
                            }
                            IdempotentMutation::Idempotent(value) => {
                                SessionMutationChange::Unchanged(value)
                            }
                        })
                    },
                )
                .map_err(|error| format!("可靠撤销协作授权失败：{error}"))
        },
    )
    .await?;
    let grant = receipt.value.grant;
    Ok(json!({
        "status": "ok",
        "changed": receipt.changed,
        "grant": grant,
        "sourceSessionId": source_id.to_string(),
        "eventSeq": receipt.value.event_seq,
        "revision": receipt.revision,
    }))
}

/// Resolves the controller and unique session registry from one service state snapshot.
/// 从一次服务状态快照解析控制器与唯一会话注册表。
///
/// # Parameters / 参数
///
/// `state` is the shared service bridge state containing both owners.
/// `state` 是同时持有两个 owner 的共享服务桥接状态。
///
/// # Returns / 返回值
///
/// Returns cloned handles that can be used after releasing the state lock.
/// 返回可在释放状态锁后使用的 owner 克隆句柄。
fn lifecycle_handles(
    state: &SharedServiceBridgeState,
) -> Result<
    (
        HostRuntimeHandle,
        Arc<crate::session_runtime_registry_v1::SessionRuntimeRegistry>,
    ),
    String,
> {
    let controller = controller_runtime_from_state(state)?;
    let registry = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .session_runtime_registry
        .clone();
    Ok((controller, registry))
}

/// Reads both current segment generations from their canonical session owners.
/// 从双方规范会话 owner 读取当前分片代际。
///
/// # Parameters / 参数
///
/// `source` and `target` are the canonical session handles already resolved by the host.
/// `source` 与 `target` 是宿主已经解析出的规范会话句柄。
///
/// # Returns / 返回值
///
/// Returns the source and target segment generations in that order.
/// 按来源、目标顺序返回两方分片代际。
fn participant_generations(
    source: &Arc<SessionHandle>,
    target: &Arc<SessionHandle>,
) -> Result<(u64, u64), String> {
    let source_generation = source
        .read(|document| document.segment_generation())
        .map_err(|error| format!("读取来源会话代际失败：{error}"))?;
    let target_generation = target
        .read(|document| document.segment_generation())
        .map_err(|error| format!("读取目标会话代际失败：{error}"))?;
    Ok((source_generation, target_generation))
}

/// Parses one exact WebUI session identifier without accepting inferred fallbacks.
///
/// # Parameters / 参数
///
/// `value` is the untrusted wire value and `field` names it in diagnostics.
/// `value` 是不可信线值，`field` 用于诊断中标识该参数。
///
/// # Returns / 返回值
///
/// Returns the parsed application session identity or an explicit parse error.
/// 返回解析后的应用会话身份，或显式解析错误。
/// 解析一个精确 WebUI 会话标识，不接受推测式回退。
fn parse_session_id(value: &str, field: &str) -> Result<SessionId, String> {
    value
        .parse::<SessionId>()
        .map_err(|error| format!("{field} 无效：{error}"))
}

/// Returns a checked signed lifecycle timestamp for collaboration metadata.
///
/// # Returns / 返回值
///
/// Returns the current Unix timestamp in signed milliseconds.
/// 返回当前 Unix 有符号毫秒时间戳。
/// 返回经过检查的协作 metadata 生命周期有符号时间戳。
fn current_session_lifecycle_unix_millis() -> Result<i64, String> {
    i64::try_from(current_unix_millis_value())
        .map_err(|_| String::from("当前时间超出 V1 会话生命周期毫秒范围。"))
}

/// Prefixes accepted for user-originated grant mutation identities.
/// 仅接受用户入口生成的授权变更操作标识前缀。
const USER_GRANT_ADD_OPERATION_PREFIX: &str = "user-grant-add:v1:";
/// Prefixes accepted for user-originated grant revocation identities.
/// 仅接受用户入口生成的授权撤销操作标识前缀。
const USER_GRANT_REMOVE_OPERATION_PREFIX: &str = "user-grant-remove:v1:";

/// Parses one stable user request identity and rejects model-shaped or ambiguous values.
/// 解析稳定的用户请求标识，并拒绝模型伪造或含义不明确的值。
fn parse_user_grant_operation_id(
    value: String,
    expected_prefix: &str,
) -> Result<CollaborationOperationId, String> {
    let operation_id = CollaborationOperationId::try_new(value)
        .map_err(|error| format!("协作授权操作标识无效：{error}"))?;
    if !operation_id.as_str().starts_with(expected_prefix)
        || operation_id.as_str().len() == expected_prefix.len()
    {
        return Err(format!(
            "协作授权操作标识必须以 `{expected_prefix}` 开始，并且包含请求 ID"
        ));
    }
    Ok(operation_id)
}
