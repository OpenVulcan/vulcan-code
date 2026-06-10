//! GUI-owned VMM hook snapshot assembly and legacy cleanup helpers.
//! GUI 负责的 VMM hook 快照组装与旧数据清理辅助逻辑。

use super::*;
use crate::model::{VmmProfileSyncState, VmmWritebackOutboxEntry};
use crate::provider_config;
use std::collections::BTreeMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
pub(super) use vulcan_agent_runtime::agent_runtime::vmm_hooks::{
    VmmHookConfigSnapshot, VmmHookRuntimeHandle,
};
use vulcan_agent_runtime::agent_runtime::vmm_hooks::{
    register_builtin_vmm_hooks as register_shared_builtin_vmm_hooks, vmm_profile_placement_phase,
};
use vulcan_hooks::SystemPromptHookPhase;

/// Stable hidden-user injection tag used for VMM profile delta updates.
/// VMM 画像增量更新使用的稳定隐藏用户注入标签。
const VMM_PROFILE_DELTA_INJECTION_TAG: &str = "vmm_profile_delta";
/// Legacy hidden-user injection tag removed from older session state.
/// 需要从旧会话状态中移除的旧版隐藏用户注入标签。
const LEGACY_VMM_PROFILE_CONTEXT_INJECTION_TAG: &str = "vmm_profile";

impl VulcanWorkbench {
    /// Builds the VMM hook configuration snapshot from global, project, and session state.
    /// 从全局、项目与会话状态构建 VMM hook 配置快照。
    pub(super) fn build_vmm_hook_config_snapshot(&self) -> VmmHookConfigSnapshot {
        let mut projects_by_workspace = BTreeMap::new();
        for project in self
            .task_projects
            .iter()
            .chain(self.archived_task_projects.iter())
        {
            let workspace_path = normalize_workspace_path_text(&project.path);
            let project_config =
                provider_config::load_project_configuration_file(&PathBuf::from(&workspace_path))
                    .unwrap_or_default();
            if !project_config.vmm.is_empty() {
                projects_by_workspace.insert(workspace_path, project_config.vmm);
            }
        }

        let sessions = self
            .session_runtime_state
            .iter()
            .chain(self.archived_session_runtime_state.iter())
            .filter_map(|(session_id, runtime_state)| {
                (!runtime_state.vmm.is_empty())
                    .then(|| (session_id.to_string(), runtime_state.vmm.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        let outboxes = self
            .session_runtime_state
            .iter()
            .chain(self.archived_session_runtime_state.iter())
            .filter_map(|(session_id, runtime_state)| {
                (!runtime_state.vmm_writeback_outbox.is_empty()).then(|| {
                    (
                        session_id.to_string(),
                        runtime_state.vmm_writeback_outbox.clone(),
                    )
                })
            })
            .collect::<BTreeMap<String, Vec<VmmWritebackOutboxEntry>>>();
        let profile_sync = self
            .session_runtime_state
            .iter()
            .chain(self.archived_session_runtime_state.iter())
            .filter_map(|(session_id, runtime_state)| {
                (!runtime_state.vmm_profile_sync.is_empty()).then(|| {
                    (
                        session_id.to_string(),
                        runtime_state.vmm_profile_sync.clone(),
                    )
                })
            })
            .collect::<BTreeMap<String, VmmProfileSyncState>>();
        let last_compaction_ids = self
            .session_runtime_state
            .iter()
            .chain(self.archived_session_runtime_state.iter())
            .map(|(session_id, runtime_state)| {
                (
                    session_id.to_string(),
                    runtime_state.last_compaction_id.clone(),
                )
            })
            .collect::<BTreeMap<_, _>>();

        VmmHookConfigSnapshot {
            global: self.provider_configuration.vmm.clone(),
            projects_by_workspace,
            sessions,
            outboxes,
            profile_sync,
            last_compaction_ids,
        }
    }

    /// Synchronizes the shared VMM hook snapshot from the current workbench state.
    /// 根据当前工作台状态同步共享 VMM hook 快照。
    pub(super) fn refresh_vmm_hook_config_snapshot(&self) {
        self.vmm_hook_runtime
            .replace_snapshot(self.build_vmm_hook_config_snapshot());
    }

    /// Seeds the shared VMM hook profile cache from loaded session runtime state.
    /// 使用已载入会话运行态初始化共享 VMM hook 画像缓存。
    pub(super) fn seed_vmm_hook_profile_cache_from_runtime_state(&self) {
        let profile_sync = self
            .session_runtime_state
            .iter()
            .chain(self.archived_session_runtime_state.iter())
            .filter_map(|(session_id, runtime_state)| {
                (!runtime_state.vmm_profile_sync.is_empty()).then(|| {
                    (
                        session_id.to_string(),
                        runtime_state.vmm_profile_sync.clone(),
                    )
                })
            })
            .collect::<BTreeMap<_, _>>();
        self.vmm_hook_runtime.seed_profile_cache(profile_sync);
    }

    /// Removes legacy hidden-user VMM profile injections from every loaded session runtime state and returns changed sessions.
    /// 从全部已加载会话运行态中移除旧版隐藏用户 VMM 画像注入，并返回发生变化的会话。
    pub(super) fn remove_legacy_vmm_profile_context_injections(&mut self) -> Vec<SessionId> {
        let now = current_unix_millis_text();
        let mut changed_session_ids = Vec::new();
        let mut cancelled_events = Vec::new();
        for (session_id, runtime_state) in self.session_runtime_state.iter_mut() {
            let previous_persistent_fingerprint =
                persistent_injection_fingerprint(&runtime_state.context_injection_state);
            let removed_legacy_profile =
                crate::agent_runtime::context_injection_runtime::remove_legacy_vmm_profile_context_injection_state(
                    &mut runtime_state.context_injection_state,
                );
            let removed_duplicate_delta =
                remove_duplicate_vmm_profile_delta_matching_base(runtime_state);
            if removed_legacy_profile || removed_duplicate_delta {
                cancelled_events.push((
                    *session_id,
                    legacy_vmm_injection_cancelled_event(
                        previous_persistent_fingerprint,
                        removed_legacy_profile,
                        removed_duplicate_delta,
                        "legacy_vmm_profile_global_cleanup",
                    ),
                ));
                runtime_state.updated_at = now.clone();
                changed_session_ids.push(*session_id);
            }
        }
        for (session_id, runtime_state) in self.archived_session_runtime_state.iter_mut() {
            let previous_persistent_fingerprint =
                persistent_injection_fingerprint(&runtime_state.context_injection_state);
            let removed_legacy_profile =
                crate::agent_runtime::context_injection_runtime::remove_legacy_vmm_profile_context_injection_state(
                    &mut runtime_state.context_injection_state,
                );
            let removed_duplicate_delta =
                remove_duplicate_vmm_profile_delta_matching_base(runtime_state);
            if removed_legacy_profile || removed_duplicate_delta {
                cancelled_events.push((
                    *session_id,
                    legacy_vmm_injection_cancelled_event(
                        previous_persistent_fingerprint,
                        removed_legacy_profile,
                        removed_duplicate_delta,
                        "legacy_vmm_profile_global_cleanup",
                    ),
                ));
                runtime_state.updated_at = now.clone();
                changed_session_ids.push(*session_id);
            }
        }
        self.record_legacy_vmm_injection_cancelled_events(cancelled_events, &now);
        changed_session_ids
    }

    /// Removes a legacy hidden-user VMM profile injection from one loaded session.
    /// 从单个已加载会话中移除旧版隐藏用户 VMM 画像注入。
    pub(super) fn remove_legacy_vmm_profile_context_injection_for_session(
        &mut self,
        session_id: SessionId,
    ) -> bool {
        let now = current_unix_millis_text();
        if let Some(runtime_state) = self.session_runtime_state.get_mut(&session_id) {
            let previous_persistent_fingerprint =
                persistent_injection_fingerprint(&runtime_state.context_injection_state);
            let removed_legacy_profile =
                crate::agent_runtime::context_injection_runtime::remove_legacy_vmm_profile_context_injection_state(
                    &mut runtime_state.context_injection_state,
                );
            let removed_duplicate_delta =
                remove_duplicate_vmm_profile_delta_matching_base(runtime_state);
            let changed = removed_legacy_profile || removed_duplicate_delta;
            if changed {
                let event = legacy_vmm_injection_cancelled_event(
                    previous_persistent_fingerprint,
                    removed_legacy_profile,
                    removed_duplicate_delta,
                    "legacy_vmm_profile_session_cleanup",
                );
                runtime_state.updated_at = now.clone();
                self.record_legacy_vmm_injection_cancelled_events(vec![(session_id, event)], &now);
            }
            return changed;
        }
        if let Some(runtime_state) = self.archived_session_runtime_state.get_mut(&session_id) {
            let previous_persistent_fingerprint =
                persistent_injection_fingerprint(&runtime_state.context_injection_state);
            let removed_legacy_profile =
                crate::agent_runtime::context_injection_runtime::remove_legacy_vmm_profile_context_injection_state(
                    &mut runtime_state.context_injection_state,
                );
            let removed_duplicate_delta =
                remove_duplicate_vmm_profile_delta_matching_base(runtime_state);
            let changed = removed_legacy_profile || removed_duplicate_delta;
            if changed {
                let event = legacy_vmm_injection_cancelled_event(
                    previous_persistent_fingerprint,
                    removed_legacy_profile,
                    removed_duplicate_delta,
                    "legacy_vmm_profile_session_cleanup",
                );
                runtime_state.updated_at = now.clone();
                self.record_legacy_vmm_injection_cancelled_events(vec![(session_id, event)], &now);
            }
            return changed;
        }
        false
    }

    /// Records legacy VMM injection cancellation audit events for changed sessions.
    /// 为发生变化的会话记录旧版 VMM 注入取消审计事件。
    fn record_legacy_vmm_injection_cancelled_events(
        &mut self,
        events: Vec<(SessionId, vulcan_context::RuntimeInjectionCancelledEvent)>,
        timestamp: &str,
    ) {
        let Some(controller_runtime) = self.controller_runtime.as_ref() else {
            return;
        };
        let mut failures = Vec::new();
        for (session_id, event) in events {
            if let Err(error) = crate::workspace_store::record_session_runtime_injection_cancelled(
                &controller_runtime.data_root,
                session_id,
                event,
                timestamp,
            ) {
                failures.push(format!("`{session_id}`: {error}"));
            }
        }
        if !failures.is_empty() {
            self.push_activity(
                ActivityLevel::Warning,
                "注入取消审计写入失败",
                failures.join("\n"),
            );
        }
    }

    /// Resolves the VMM profile placement phase for the next user prompt in one session.
    /// 解析某个会话下一条用户提示对应的 VMM 画像放置阶段。
    pub(super) fn vmm_profile_placement_phase_for_next_user_prompt(
        &self,
        session_id: SessionId,
        user_prompt_already_appended: bool,
    ) -> SystemPromptHookPhase {
        let conversation_state = self.session_conversation_state(session_id);
        let runtime_state = self.session_runtime_state(session_id);
        let message_count = if user_prompt_already_appended {
            conversation_state.messages.len()
        } else {
            conversation_state.messages.len().saturating_add(1)
        };
        vmm_profile_placement_phase(
            message_count,
            runtime_state.and_then(|state| state.last_compaction_id.as_deref()),
            runtime_state.and_then(|state| state.vmm_profile_sync.covered_compaction_id.as_deref()),
        )
    }

    /// Registers the built-in VMM hook handlers on the shared conversation hook dispatcher.
    /// 在共享会话 hook 调度器上注册内置 VMM hook 处理器。
    pub(super) fn register_builtin_vmm_hooks(&mut self) {
        let _ = register_shared_builtin_vmm_hooks(
            self.conversation_hook_runtime.dispatcher_mut(),
            self.vmm_hook_runtime.clone(),
        );
    }
}

/// Removes profile-delta items that exactly duplicate the current system baseline.
/// 移除与当前系统基点完全重复的画像增量项。
fn remove_duplicate_vmm_profile_delta_matching_base(
    runtime_state: &mut SessionRuntimeStateEntry,
) -> bool {
    let Some(base_profile_content) = runtime_state
        .vmm_profile_sync
        .base_profile_content
        .as_deref()
        .map(normalize_profile_duplicate_text)
        .filter(|content| !content.is_empty())
    else {
        return false;
    };
    let Some(group) = runtime_state
        .context_injection_state
        .groups
        .get_mut(VMM_PROFILE_DELTA_INJECTION_TAG)
    else {
        return false;
    };
    let before_len = group.items.len();
    group
        .items
        .retain(|_, item| normalize_profile_duplicate_text(&item.content) != base_profile_content);
    let removed_any = group.items.len() != before_len;
    if group.items.is_empty() {
        runtime_state
            .context_injection_state
            .groups
            .remove(VMM_PROFILE_DELTA_INJECTION_TAG);
    }
    removed_any
}

/// Normalizes profile text for duplicate cleanup.
/// 为画像重复清理规范化文本。
fn normalize_profile_duplicate_text(content: &str) -> String {
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Builds one runtime audit event for legacy VMM injection cleanup.
/// 为旧版 VMM 注入清理构建一条运行时审计事件。
fn legacy_vmm_injection_cancelled_event(
    previous_persistent_fingerprint: Option<String>,
    removed_legacy_profile: bool,
    removed_duplicate_delta: bool,
    reason: &str,
) -> vulcan_context::RuntimeInjectionCancelledEvent {
    vulcan_context::RuntimeInjectionCancelledEvent {
        turn_id: None,
        cancellation_id: format!("{reason}-{}", current_unix_millis_text()),
        source_node_ids: legacy_vmm_cancelled_source_node_ids(
            removed_legacy_profile,
            removed_duplicate_delta,
        ),
        previous_persistent_fingerprint,
        previous_transient_fingerprint: None,
        reason: Some(reason.to_string()),
    }
}

/// Returns stable source ids removed by one legacy VMM cleanup pass.
/// 返回单次旧版 VMM 清理移除的稳定来源标识。
fn legacy_vmm_cancelled_source_node_ids(
    removed_legacy_profile: bool,
    removed_duplicate_delta: bool,
) -> Vec<String> {
    let mut source_node_ids = Vec::new();
    if removed_legacy_profile {
        source_node_ids.push(format!(
            "persistent:{LEGACY_VMM_PROFILE_CONTEXT_INJECTION_TAG}"
        ));
    }
    if removed_duplicate_delta {
        source_node_ids.push(format!("persistent:{VMM_PROFILE_DELTA_INJECTION_TAG}"));
    }
    source_node_ids
}

/// Builds a stable fingerprint for persistent injection blocks without storing their content.
/// 为长期注入块构建稳定指纹，同时不保存注入正文。
fn persistent_injection_fingerprint(
    state: &vulcan_context_injection::ContextInjectionState,
) -> Option<String> {
    let view = state.resolve_view();
    if view.persistent_blocks.is_empty() {
        return None;
    }
    let mut hasher = DefaultHasher::new();
    view.persistent_blocks.hash(&mut hasher);
    Some(format!("{:016x}", hasher.finish()))
}
