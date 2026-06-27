//! VMM hook snapshot and writeback integration for service-kernel runtime paths.
//! service-kernel 运行时路径使用的 VMM hook 快照与写回集成逻辑。

use std::collections::BTreeSet;

use vulcan_agent_runtime::{
    agent_runtime::{conversation_hooks::VmmWritebackHookEffect, vmm_hooks::VmmHookConfigSnapshot},
    provider_config::ProviderConfigurationFile,
};
use vulcan_workbench_model::SessionRuntimeStateEntry;
use vulcan_workspace_store::workspace_store::WorkspaceSnapshot;

use crate::{
    ServiceBridgeState, SharedServiceBridgeState, current_unix_millis_value,
    load_project_configuration, webui_trace,
};

/// Refreshes the retained VMM hook snapshot from shared service state.
/// 从共享服务状态刷新保留的 VMM hook 快照。
///
/// # Parameters
/// 参数
/// `state` is the shared service-kernel state holding runtime and workspace snapshots.
/// `state` 是持有运行时与工作区快照的共享 service-kernel 状态。
///
/// # Returns
/// 返回值
/// `Ok(())` after the hook snapshot has been refreshed or skipped when runtime state is absent.
/// 刷新 hook 快照完成后返回 `Ok(())`；运行态缺失时跳过并返回成功。
pub(super) fn refresh_service_vmm_hook_snapshot(
    state: &SharedServiceBridgeState,
) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let configuration = guard
        .agent_runtime
        .as_ref()
        .map(|runtime| runtime.provider_configuration.clone());
    if let Some(configuration) = configuration {
        refresh_guard_vmm_hook_snapshot(&mut guard, &configuration);
    }
    Ok(())
}

/// Refreshes the VMM hook snapshot for one locked service state.
/// 为单个已加锁服务状态刷新 VMM hook 快照。
///
/// # Parameters
/// 参数
/// `guard` is the locked service state containing the runtime hook handle and workspace snapshot.
/// `guard` 是包含运行时 hook 句柄与工作区快照的已加锁服务状态。
/// `provider_configuration` is the provider configuration used as the global VMM layer.
/// `provider_configuration` 是作为全局 VMM 配置层使用的 Provider 配置。
///
/// # Returns
/// 返回值
/// This function updates the hook runtime in place and returns no value.
/// 该函数会原地更新 hook 运行时且不返回值。
pub(super) fn refresh_guard_vmm_hook_snapshot(
    guard: &mut ServiceBridgeState,
    provider_configuration: &ProviderConfigurationFile,
) {
    if let (Some(agent_runtime), Some(workspace_snapshot)) = (
        guard.agent_runtime.as_ref(),
        guard.workspace_snapshot.as_ref(),
    ) {
        agent_runtime
            .vmm_hook_runtime
            .replace_snapshot(build_vmm_hook_config_snapshot(
                provider_configuration,
                workspace_snapshot,
            ));
    }
}

/// Builds the shared VMM hook snapshot from the current Tauri workspace state.
/// 根据当前 Tauri 工作区状态构建共享 VMM hook 快照。
///
/// # Parameters
/// 参数
/// `provider_configuration` provides the global VMM configuration layer.
/// `provider_configuration` 提供全局 VMM 配置层。
/// `workspace_snapshot` provides project, session, outbox, profile-sync and compaction state.
/// `workspace_snapshot` 提供项目、会话、outbox、画像同步与压缩状态。
///
/// # Returns
/// 返回值
/// The complete VMM hook configuration snapshot consumed by agent-runtime hooks.
/// 返回 agent-runtime hook 使用的完整 VMM hook 配置快照。
pub(super) fn build_vmm_hook_config_snapshot(
    provider_configuration: &ProviderConfigurationFile,
    workspace_snapshot: &WorkspaceSnapshot,
) -> VmmHookConfigSnapshot {
    let snapshot_started_at = current_unix_millis_value();
    let mut projects_by_workspace = std::collections::BTreeMap::new();
    let mut project_count = 0usize;
    for project in workspace_snapshot
        .task_projects
        .iter()
        .chain(workspace_snapshot.archived_task_projects.iter())
    {
        project_count += 1;
        let project_configuration = load_project_configuration(&project.path);
        if !project_configuration.vmm.is_empty() {
            projects_by_workspace.insert(project.path.clone(), project_configuration.vmm);
        }
    }

    let sessions = workspace_snapshot
        .session_runtime_state
        .iter()
        .chain(workspace_snapshot.archived_session_runtime_state.iter())
        .filter_map(|(session_id, runtime_state)| {
            (!runtime_state.vmm.is_empty())
                .then(|| (session_id.to_string(), runtime_state.vmm.clone()))
        })
        .collect();
    let outboxes = workspace_snapshot
        .session_runtime_state
        .iter()
        .chain(workspace_snapshot.archived_session_runtime_state.iter())
        .filter_map(|(session_id, runtime_state)| {
            (!runtime_state.vmm_writeback_outbox.is_empty()).then(|| {
                (
                    session_id.to_string(),
                    runtime_state.vmm_writeback_outbox.clone(),
                )
            })
        })
        .collect();
    let profile_sync = workspace_snapshot
        .session_runtime_state
        .iter()
        .chain(workspace_snapshot.archived_session_runtime_state.iter())
        .filter_map(|(session_id, runtime_state)| {
            (!runtime_state.vmm_profile_sync.is_empty()).then(|| {
                (
                    session_id.to_string(),
                    runtime_state.vmm_profile_sync.clone(),
                )
            })
        })
        .collect();
    let last_compaction_ids = workspace_snapshot
        .session_runtime_state
        .iter()
        .chain(workspace_snapshot.archived_session_runtime_state.iter())
        .map(|(session_id, runtime_state)| {
            (
                session_id.to_string(),
                runtime_state.last_compaction_id.clone(),
            )
        })
        .collect();
    let snapshot = VmmHookConfigSnapshot {
        global: provider_configuration.vmm.clone(),
        projects_by_workspace,
        sessions,
        outboxes,
        profile_sync,
        last_compaction_ids,
    };
    webui_trace(
        "vmm_hook_snapshot_built",
        format!(
            "projects_scanned={} project_vmm={} session_vmm={} outboxes={} profile_sync={} last_compaction_ids={} elapsed_ms={}",
            project_count,
            snapshot.projects_by_workspace.len(),
            snapshot.sessions.len(),
            snapshot.outboxes.len(),
            snapshot.profile_sync.len(),
            snapshot.last_compaction_ids.len(),
            current_unix_millis_value().saturating_sub(snapshot_started_at)
        ),
    );
    snapshot
}

/// Applies VMM writeback hook effects to one session runtime state.
/// 将 VMM 写回 hook 效果应用到单个会话运行态。
///
/// # Parameters
/// 参数
/// `runtime_state` is the mutable session runtime state receiving outbox changes.
/// `runtime_state` 是接收 outbox 变更的可变会话运行态。
/// `effects` are the hook effects returned by the VMM completion hook.
/// `effects` 是 VMM completion hook 返回的 hook 效果集合。
/// `now` is the timestamp text stored on created or updated outbox entries.
/// `now` 是写入新建或更新 outbox 条目的时间戳文本。
///
/// # Returns
/// 返回值
/// This function mutates the runtime state in place and returns no value.
/// 该函数会原地修改运行态且不返回值。
pub(super) fn apply_vmm_writeback_effects_to_runtime_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    effects: Vec<VmmWritebackHookEffect>,
    now: &str,
) {
    for effect in effects {
        match effect {
            VmmWritebackHookEffect::Flushed { entry_ids } => {
                let ids = entry_ids.into_iter().collect::<BTreeSet<_>>();
                runtime_state
                    .vmm_writeback_outbox
                    .retain(|entry| !ids.contains(&entry.id));
            }
            VmmWritebackHookEffect::Enqueue { request, error } => {
                let id = format!(
                    "vmm-outbox-{}-{}",
                    request.session_id,
                    stable_vmm_outbox_request_hash(&request)
                );
                if let Some(existing) = runtime_state
                    .vmm_writeback_outbox
                    .iter_mut()
                    .find(|entry| entry.id == id)
                {
                    existing.request = request;
                    existing.attempt_count = existing.attempt_count.saturating_add(1);
                    existing.last_error = Some(error);
                    existing.updated_at = now.to_string();
                } else {
                    runtime_state.vmm_writeback_outbox.push(
                        vulcan_workbench_model::VmmWritebackOutboxEntry {
                            id,
                            request,
                            attempt_count: 1,
                            last_error: Some(error),
                            created_at: now.to_string(),
                            updated_at: now.to_string(),
                        },
                    );
                }
            }
            VmmWritebackHookEffect::Failed { entry_id, error } => {
                if let Some(existing) = runtime_state
                    .vmm_writeback_outbox
                    .iter_mut()
                    .find(|entry| entry.id == entry_id)
                {
                    existing.attempt_count = existing.attempt_count.saturating_add(1);
                    existing.last_error = Some(error);
                    existing.updated_at = now.to_string();
                }
            }
        }
    }
}

/// Builds one stable hash for a VMM writeback request.
/// 为一条 VMM 写回请求构建稳定哈希。
///
/// # Parameters
/// 参数
/// `request` is the VMM PostAction request that needs a deterministic local outbox key.
/// `request` 是需要确定性本地 outbox 键的 VMM PostAction 请求。
///
/// # Returns
/// 返回值
/// A deterministic non-cryptographic hash used only for persisted outbox identity.
/// 返回仅用于持久化 outbox 身份的确定性非加密哈希。
fn stable_vmm_outbox_request_hash(request: &vulcan_vmm::PostActionRequest) -> u64 {
    let mut hasher = StableVmmOutboxHasher::default();
    hasher.write_str(&request.session_id);
    hasher.write_u64(request.user_id);
    hasher.write_u64(request.project_id);
    hasher.write_str(&request.user_content);
    hasher.write_str(&request.assistant_content);
    for item in &request.timeline {
        hasher.write_str(item.kind.as_wire_type());
        hasher.write_str(&item.content);
    }
    hasher.finish()
}

/// Tiny deterministic FNV-1a helper used for persisted VMM outbox ids.
/// 用于持久化 VMM outbox 标识的小型确定性 FNV-1a 辅助器。
struct StableVmmOutboxHasher {
    /// Current FNV-1a accumulator.
    /// 当前 FNV-1a 累加值。
    value: u64,
}

impl Default for StableVmmOutboxHasher {
    /// Creates one FNV-1a hasher with the canonical 64-bit offset basis.
    /// 使用标准 64 位 offset basis 创建一个 FNV-1a 哈希器。
    fn default() -> Self {
        Self {
            value: 0xcbf29ce484222325,
        }
    }
}

impl StableVmmOutboxHasher {
    /// Writes a length-prefixed string into the hash stream.
    /// 将带长度前缀的字符串写入哈希流。
    fn write_str(&mut self, value: &str) {
        self.write_u64(value.len() as u64);
        self.write_bytes(value.as_bytes());
    }

    /// Writes one integer into the hash stream in little-endian form.
    /// 以小端形式将一个整数写入哈希流。
    fn write_u64(&mut self, value: u64) {
        self.write_bytes(&value.to_le_bytes());
    }

    /// Writes raw bytes into the FNV-1a accumulator.
    /// 将原始字节写入 FNV-1a 累加器。
    fn write_bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.value ^= u64::from(*byte);
            self.value = self.value.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    /// Returns the final deterministic hash value.
    /// 返回最终的确定性哈希值。
    fn finish(self) -> u64 {
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    use uuid::Uuid;
    use vulcan_workbench_model::VmmWritebackOutboxEntry;
    use vulcan_workspace_store::workspace_store::WorkspaceSnapshot;

    /// Builds a minimal workspace snapshot for VMM hook integration tests.
    /// 为 VMM hook 集成测试构建最小工作区快照。
    fn test_workspace_snapshot() -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: Uuid::nil(),
            hydrated_session_ids: BTreeSet::new(),
        }
    }

    /// Builds a stable PostAction request for writeback tests.
    /// 为写回测试构建稳定的 PostAction 请求。
    fn test_post_action_request() -> vulcan_vmm::PostActionRequest {
        vulcan_vmm::PostActionRequest {
            session_id: String::from("session-a"),
            user_id: 7,
            project_id: 11,
            user_content: String::from("hello"),
            assistant_content: String::from("world"),
            timeline: vec![vulcan_vmm::PostActionTimelineItem {
                kind: vulcan_vmm::PostActionTimelineType::Assistant,
                content: String::from("middle"),
            }],
        }
    }

    /// Verifies hook snapshots collect session VMM, outbox, profile and compaction state.
    /// 验证 hook 快照会收集会话 VMM、outbox、画像和压缩状态。
    #[test]
    fn build_vmm_hook_config_snapshot_collects_session_runtime_state() {
        let session_id = Uuid::from_u128(42);
        let archived_session_id = Uuid::from_u128(43);
        let mut runtime_state = SessionRuntimeStateEntry::default();
        runtime_state.vmm.enabled = Some(true);
        runtime_state
            .vmm_writeback_outbox
            .push(VmmWritebackOutboxEntry {
                id: String::from("outbox-a"),
                request: test_post_action_request(),
                attempt_count: 1,
                last_error: Some(String::from("offline")),
                created_at: String::from("100"),
                updated_at: String::from("100"),
            });
        runtime_state.vmm_profile_sync.updated_at = Some(String::from("profile-updated"));
        runtime_state.last_compaction_id = Some(String::from("compact-a"));

        let mut workspace = test_workspace_snapshot();
        workspace
            .session_runtime_state
            .insert(session_id, runtime_state);
        let mut archived_runtime_state = SessionRuntimeStateEntry::default();
        archived_runtime_state.vmm.ai_tools_enabled = Some(false);
        archived_runtime_state.last_compaction_id = Some(String::from("archived-compact"));
        workspace
            .archived_session_runtime_state
            .insert(archived_session_id, archived_runtime_state);
        let mut provider_configuration = ProviderConfigurationFile::default();
        provider_configuration.vmm.enabled = Some(true);

        let snapshot = build_vmm_hook_config_snapshot(&provider_configuration, &workspace);

        assert_eq!(snapshot.global.enabled, Some(true));
        assert!(snapshot.sessions.contains_key(&session_id.to_string()));
        assert!(
            snapshot
                .sessions
                .contains_key(&archived_session_id.to_string())
        );
        assert_eq!(
            snapshot.outboxes.get(&session_id.to_string()).map(Vec::len),
            Some(1)
        );
        assert!(snapshot.profile_sync.contains_key(&session_id.to_string()));
        assert_eq!(
            snapshot
                .last_compaction_ids
                .get(&session_id.to_string())
                .and_then(Clone::clone)
                .as_deref(),
            Some("compact-a")
        );
        assert_eq!(
            snapshot
                .last_compaction_ids
                .get(&archived_session_id.to_string())
                .and_then(Clone::clone)
                .as_deref(),
            Some("archived-compact")
        );
    }

    /// Verifies writeback effects enqueue, update, mark failure and flush one outbox entry.
    /// 验证写回效果会入队、更新、标记失败并冲刷单个 outbox 条目。
    #[test]
    fn apply_vmm_writeback_effects_updates_outbox_lifecycle() {
        let request = test_post_action_request();
        let mut runtime_state = SessionRuntimeStateEntry::default();

        apply_vmm_writeback_effects_to_runtime_state(
            &mut runtime_state,
            vec![VmmWritebackHookEffect::Enqueue {
                request: request.clone(),
                error: String::from("offline"),
            }],
            "100",
        );

        assert_eq!(runtime_state.vmm_writeback_outbox.len(), 1);
        let entry_id = runtime_state.vmm_writeback_outbox[0].id.clone();
        assert_eq!(runtime_state.vmm_writeback_outbox[0].attempt_count, 1);
        assert_eq!(
            runtime_state.vmm_writeback_outbox[0].last_error.as_deref(),
            Some("offline")
        );

        apply_vmm_writeback_effects_to_runtime_state(
            &mut runtime_state,
            vec![VmmWritebackHookEffect::Enqueue {
                request,
                error: String::from("still offline"),
            }],
            "200",
        );

        assert_eq!(runtime_state.vmm_writeback_outbox.len(), 1);
        assert_eq!(runtime_state.vmm_writeback_outbox[0].attempt_count, 2);
        assert_eq!(runtime_state.vmm_writeback_outbox[0].updated_at, "200");

        apply_vmm_writeback_effects_to_runtime_state(
            &mut runtime_state,
            vec![VmmWritebackHookEffect::Failed {
                entry_id: entry_id.clone(),
                error: String::from("replay failed"),
            }],
            "300",
        );

        assert_eq!(runtime_state.vmm_writeback_outbox[0].attempt_count, 3);
        assert_eq!(
            runtime_state.vmm_writeback_outbox[0].last_error.as_deref(),
            Some("replay failed")
        );

        apply_vmm_writeback_effects_to_runtime_state(
            &mut runtime_state,
            vec![VmmWritebackHookEffect::Flushed {
                entry_ids: vec![entry_id],
            }],
            "400",
        );

        assert!(runtime_state.vmm_writeback_outbox.is_empty());
    }
}
