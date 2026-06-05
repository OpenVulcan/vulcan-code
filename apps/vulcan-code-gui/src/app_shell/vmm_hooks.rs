//! GUI-owned VMM hook snapshot assembly and legacy cleanup helpers.
//! GUI 负责的 VMM hook 快照组装与旧数据清理辅助逻辑。

use super::*;
use crate::model::{VmmProfileSyncState, VmmWritebackOutboxEntry};
use crate::provider_config;
use std::collections::BTreeMap;
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
        for (session_id, runtime_state) in self.session_runtime_state.iter_mut() {
            let removed_legacy_profile =
                crate::agent_runtime::context_injection_runtime::remove_legacy_vmm_profile_context_injection_state(
                    &mut runtime_state.context_injection_state,
                );
            let removed_duplicate_delta =
                remove_duplicate_vmm_profile_delta_matching_base(runtime_state);
            if removed_legacy_profile || removed_duplicate_delta {
                runtime_state.updated_at = now.clone();
                changed_session_ids.push(*session_id);
            }
        }
        for (session_id, runtime_state) in self.archived_session_runtime_state.iter_mut() {
            let removed_legacy_profile =
                crate::agent_runtime::context_injection_runtime::remove_legacy_vmm_profile_context_injection_state(
                    &mut runtime_state.context_injection_state,
                );
            let removed_duplicate_delta =
                remove_duplicate_vmm_profile_delta_matching_base(runtime_state);
            if removed_legacy_profile || removed_duplicate_delta {
                runtime_state.updated_at = now.clone();
                changed_session_ids.push(*session_id);
            }
        }
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
            let removed_legacy_profile =
                crate::agent_runtime::context_injection_runtime::remove_legacy_vmm_profile_context_injection_state(
                    &mut runtime_state.context_injection_state,
                );
            let removed_duplicate_delta =
                remove_duplicate_vmm_profile_delta_matching_base(runtime_state);
            let changed = removed_legacy_profile || removed_duplicate_delta;
            if changed {
                runtime_state.updated_at = now;
            }
            return changed;
        }
        if let Some(runtime_state) = self.archived_session_runtime_state.get_mut(&session_id) {
            let removed_legacy_profile =
                crate::agent_runtime::context_injection_runtime::remove_legacy_vmm_profile_context_injection_state(
                    &mut runtime_state.context_injection_state,
                );
            let removed_duplicate_delta =
                remove_duplicate_vmm_profile_delta_matching_base(runtime_state);
            let changed = removed_legacy_profile || removed_duplicate_delta;
            if changed {
                runtime_state.updated_at = now;
            }
            return changed;
        }
        false
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
