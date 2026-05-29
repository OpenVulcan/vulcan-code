//! Prompt Skills panel message handling.
//! Prompt Skills 面板消息处理。

use super::*;
use vulcan_prompt_skills::PromptSkillCatalog;

/// Loads prompt skills from the persisted global configuration.
/// 按持久化全局配置载入提示词技能。
pub(super) async fn load_prompt_skills_catalog_task(
    config: crate::provider_config::PersistedPromptSkillsConfiguration,
) -> Result<PromptSkillCatalog, String> {
    let runtime_config = crate::skills::prompt::global_config_from_persisted(&config);
    Ok(vulcan_prompt_skills::load_prompt_skills(&runtime_config))
}

/// Installs the embedded VulcanCode system prompt skills into the host-managed `.system` root.
/// 将内置 VulcanCode 系统提示词技能安装到宿主管理的 `.system` 根目录。
pub(super) async fn install_system_prompt_skills_task(
    config: crate::provider_config::PersistedPromptSkillsConfiguration,
) -> Result<vulcan_prompt_skills::SystemPromptSkillsInstallReport, String> {
    let runtime_config = crate::skills::prompt::global_config_from_persisted(&config);
    let packaged_system_skills_root =
        crate::runtime_assets::resolve_system_prompt_skills_resource_directory();
    let install_result = if packaged_system_skills_root.is_dir() {
        vulcan_prompt_skills::install_system_prompt_skills_from_source(
            packaged_system_skills_root,
            vulcan_prompt_skills::configured_system_skill_root(&runtime_config),
        )
    } else {
        vulcan_prompt_skills::install_embedded_system_prompt_skills(&runtime_config)
    };
    install_result.map_err(|error| format!("{}：{}", error.path.display(), error.message))
}

impl VulcanWorkbench {
    /// Handles prompt-skill catalog and visibility messages outside the main update dispatcher.
    /// 在主更新分发器之外处理提示词技能目录与可见性消息。
    pub(super) fn update_prompt_skills_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::PromptSkillsCatalogLoaded(result) => {
                match result {
                    Ok(catalog) => {
                        let skill_count = catalog.skills.len();
                        let error_count = catalog.errors.len();
                        let system_count = catalog
                            .skills
                            .iter()
                            .filter(|skill| {
                                skill.scope == vulcan_prompt_skills::PromptSkillScope::System
                            })
                            .count();
                        let user_count = skill_count.saturating_sub(system_count);
                        self.prompt_skills_notice = format!(
                            "提示词技能目录已载入：系统根 {} · 用户根 {} · 系统 {} 个 · 用户 {} 个 · 错误 {} 个。",
                            catalog.system_root.display(),
                            catalog.user_root.display(),
                            system_count,
                            user_count,
                            error_count
                        );
                        self.prompt_skills_catalog = Some(catalog);
                    }
                    Err(error) => {
                        self.prompt_skills_notice = format!("提示词技能目录载入失败：{error}");
                        self.push_activity(ActivityLevel::Warning, "提示词技能目录载入失败", error);
                    }
                }
                Task::none()
            }
            Message::RefreshPromptSkillsCatalog => {
                self.prompt_skills_notice = String::from("正在刷新提示词技能目录。");
                Task::perform(
                    load_prompt_skills_catalog_task(
                        self.provider_configuration.prompt_skills.clone(),
                    ),
                    Message::PromptSkillsCatalogLoaded,
                )
            }
            Message::InstallSystemPromptSkills => {
                self.prompt_skills_notice = String::from("正在同步 VulcanCode 系统提示词技能。");
                Task::perform(
                    install_system_prompt_skills_task(
                        self.provider_configuration.prompt_skills.clone(),
                    ),
                    Message::SystemPromptSkillsInstalled,
                )
            }
            Message::SystemPromptSkillsInstalled(result) => match result {
                Ok(report) => {
                    let action = if report.installed {
                        "已同步"
                    } else {
                        "已是最新"
                    };
                    self.prompt_skills_notice = format!(
                        "系统提示词技能{action}：{} · 技能 {} 个。",
                        report.system_root.display(),
                        report.skill_names.len()
                    );
                    Task::done(Message::RefreshPromptSkillsCatalog)
                }
                Err(error) => {
                    self.prompt_skills_notice = format!("同步系统提示词技能失败：{error}");
                    self.push_activity(ActivityLevel::Warning, "同步系统提示词技能失败", error);
                    Task::none()
                }
            },
            Message::SetPromptSkillsEnabled(enabled) => {
                match self.persist_prompt_skills_enabled(enabled) {
                    Ok(true) => {
                        self.prompt_skills_notice = if enabled {
                            String::from("提示词技能全局总开关已启用。")
                        } else {
                            String::from("提示词技能全局总开关已关闭。")
                        };
                    }
                    Ok(false) => {}
                    Err(error) => {
                        self.prompt_skills_notice = format!("保存提示词技能总开关失败：{error}");
                        self.push_activity(
                            ActivityLevel::Warning,
                            "保存提示词技能总开关失败",
                            error,
                        );
                    }
                }
                Task::done(Message::RefreshPromptSkillsCatalog)
            }
            Message::SetPromptSkillGlobalState { name, enabled } => {
                match self.persist_prompt_skill_global_state(&name, enabled) {
                    Ok(true) => {
                        self.prompt_skills_notice = format!(
                            "提示词技能 `{name}` 已{}。",
                            if enabled { "启用" } else { "停用" }
                        );
                    }
                    Ok(false) => {}
                    Err(error) => {
                        self.prompt_skills_notice =
                            format!("保存提示词技能 `{name}` 状态失败：{error}");
                        self.push_activity(ActivityLevel::Warning, "保存提示词技能状态失败", error);
                    }
                }
                Task::done(Message::RefreshPromptSkillsCatalog)
            }
            Message::SetPromptSkillSessionState { name, enabled } => {
                self.set_prompt_skill_session_state(&name, enabled);
                self.prompt_skills_notice = format!(
                    "当前会话中的提示词技能 `{name}` 已{}。",
                    if enabled { "启用" } else { "停用" }
                );
                Task::none()
            }
            Message::SetSessionVmmEnabled(enabled) => {
                self.set_session_vmm_enabled(enabled);
                self.prompt_skills_notice = if enabled {
                    String::from("当前会话已启用 VMM 记忆覆盖。")
                } else {
                    String::from("当前会话已关闭 VMM 记忆覆盖。")
                };
                Task::none()
            }
            Message::SetSessionVmmAiToolsEnabled(enabled) => {
                self.set_session_vmm_ai_tools_enabled(enabled);
                self.prompt_skills_notice = if enabled {
                    String::from("当前会话已启用 VMM AI 专属工具。")
                } else {
                    String::from("当前会话已关闭 VMM AI 专属工具。")
                };
                Task::none()
            }
            Message::GlobalVmmEnabledChanged(value) => {
                self.vmm_settings_draft.enabled = value;
                self.vmm_settings_draft.notice =
                    String::from("全局 VMM 启用状态已修改，点击保存后生效。");
                Task::none()
            }
            Message::GlobalVmmGrpcTargetChanged(value) => {
                self.vmm_settings_draft.grpc_target = value;
                self.vmm_settings_draft.notice =
                    String::from("全局 VMM gRPC 地址已修改，点击保存后生效。");
                Task::none()
            }
            Message::GlobalVmmApiKeyChanged(value) => {
                self.vmm_settings_draft.grpc_api_key = value;
                self.vmm_settings_draft.notice =
                    String::from("全局 VMM API key 已修改，点击保存后生效。");
                Task::none()
            }
            Message::GlobalVmmUserIdChanged(value) => {
                self.vmm_settings_draft.user_id = value;
                self.vmm_settings_draft.notice =
                    String::from("全局 VMM 用户 ID 已修改，点击保存后生效。");
                Task::none()
            }
            Message::GlobalVmmProjectIdChanged(value) => {
                self.vmm_settings_draft.project_id = value;
                self.vmm_settings_draft.notice =
                    String::from("全局 VMM 项目 ID 已修改，点击保存后生效。");
                Task::none()
            }
            Message::GlobalVmmManagementUserRefChanged(value) => {
                self.vmm_settings_draft.management_user_ref = value;
                self.vmm_settings_draft.notice =
                    String::from("全局 VMM 管理用户引用已修改，可直接执行管理动作。");
                Task::none()
            }
            Message::GlobalVmmManagementProjectRefChanged(value) => {
                self.vmm_settings_draft.management_project_ref = value;
                self.vmm_settings_draft.notice =
                    String::from("全局 VMM 管理项目引用已修改，可直接执行管理动作。");
                Task::none()
            }
            Message::GlobalVmmImplicitTurnsChanged(value) => {
                self.vmm_settings_draft.implicit_memory_turns = value;
                self.vmm_settings_draft.notice =
                    String::from("全局 VMM 临时记忆 TTL 已修改，点击保存后生效。");
                Task::none()
            }
            Message::GlobalVmmProfileRefreshTurnsChanged(value) => {
                self.vmm_settings_draft.profile_refresh_turns = value;
                self.vmm_settings_draft.notice =
                    String::from("全局 VMM 画像刷新间隔已修改，点击保存后生效。");
                Task::none()
            }
            Message::GlobalVmmVisibleMemoryChanged(value) => {
                self.vmm_settings_draft.visible_memory_injection = value;
                self.vmm_settings_draft.notice =
                    String::from("全局 VMM 可见记忆模式已修改，点击保存后生效。");
                Task::none()
            }
            Message::GlobalVmmSessionCompactRecallChanged(value) => {
                self.vmm_settings_draft.session_compact_recall = value;
                self.vmm_settings_draft.notice =
                    String::from("全局 VMM 压缩感知召回已修改，点击保存后生效。");
                Task::none()
            }
            Message::GlobalVmmAiToolsChanged(value) => {
                self.vmm_settings_draft.ai_tools_enabled = value;
                self.vmm_settings_draft.notice =
                    String::from("全局 VMM AI 专属工具开关已修改，点击保存后生效。");
                Task::none()
            }
            Message::GlobalVmmInjectionTagChanged(value) => {
                self.vmm_settings_draft.injection_tag = value;
                self.vmm_settings_draft.notice =
                    String::from("全局 VMM 注入标签已修改，点击保存后生效。");
                Task::none()
            }
            Message::GlobalVmmInjectionDescriptionChanged(value) => {
                self.vmm_settings_draft.injection_description = value;
                self.vmm_settings_draft.notice =
                    String::from("全局 VMM 注入说明已修改，点击保存后生效。");
                Task::none()
            }
            Message::ReloadGlobalVmmConfiguration => {
                self.reload_global_vmm_settings_draft();
                self.prompt_skills_notice = self.vmm_settings_draft.notice.clone();
                Task::none()
            }
            Message::SaveGlobalVmmConfiguration => {
                match self.save_global_vmm_settings_draft() {
                    Ok(()) => self.prompt_skills_notice = self.vmm_settings_draft.notice.clone(),
                    Err(error) => {
                        self.vmm_settings_draft.notice = error.clone();
                        self.prompt_skills_notice = error;
                    }
                }
                Task::none()
            }
            Message::RunGlobalVmmManagement(action) => {
                let layer = match self.vmm_settings_draft.to_layer() {
                    Ok(layer) => layer,
                    Err(error) => {
                        self.vmm_settings_draft.notice = error.clone();
                        self.prompt_skills_notice = error;
                        return Task::none();
                    }
                };
                self.vmm_settings_draft.notice =
                    format!("正在执行 VMM 管理动作：{}。", action.label());
                self.prompt_skills_notice = self.vmm_settings_draft.notice.clone();
                let management_user_ref = self.vmm_settings_draft.management_user_ref.clone();
                let management_project_ref = self.vmm_settings_draft.management_project_ref.clone();
                Task::perform(
                    vmm_management::run_vmm_management_action(
                        action,
                        layer,
                        management_user_ref,
                        management_project_ref,
                    ),
                    Message::GlobalVmmManagementFinished,
                )
            }
            Message::GlobalVmmManagementFinished(result) => {
                let notice = match result {
                    Ok(message) => message,
                    Err(error) => format!("VMM 管理调用失败：{error}"),
                };
                self.vmm_settings_draft.notice = notice.clone();
                self.prompt_skills_notice = notice;
                Task::none()
            }
            Message::TogglePromptSkillDetails(name) => {
                self.selected_prompt_skill_name = (self.selected_prompt_skill_name.as_deref()
                    != Some(name.as_str()))
                .then_some(name);
                Task::none()
            }
            _ => Task::none(),
        }
    }

    /// Persists one prompt-skill override into the selected session runtime state.
    /// 将单个提示词技能覆盖持久化到当前选中会话运行时状态。
    fn set_prompt_skill_session_state(&mut self, name: &str, enabled: bool) {
        let session_id = self.selected_session;
        if session_id.is_nil() {
            return;
        }
        let state = if enabled {
            crate::provider_config::PersistedPromptSkillState::Enabled
        } else {
            crate::provider_config::PersistedPromptSkillState::Disabled
        };
        let runtime_state = self.ensure_session_runtime_state_mut(session_id);
        runtime_state
            .prompt_skill_overrides
            .insert(name.trim().to_string(), state);
        runtime_state.updated_at = current_unix_millis_text();
    }

    /// Persists the selected session VMM memory override.
    /// 持久化当前选中会话的 VMM 记忆覆盖。
    fn set_session_vmm_enabled(&mut self, enabled: bool) {
        let session_id = self.selected_session;
        if session_id.is_nil() {
            return;
        }
        let now = current_unix_millis_text();
        let runtime_state = self.ensure_session_runtime_state_mut(session_id);
        runtime_state.vmm.enabled = Some(enabled);
        runtime_state.updated_at = now;
        self.refresh_vmm_hook_config_snapshot();
    }

    /// Persists the selected session VMM AI-tool override.
    /// 持久化当前选中会话的 VMM AI 工具覆盖。
    fn set_session_vmm_ai_tools_enabled(&mut self, enabled: bool) {
        let session_id = self.selected_session;
        if session_id.is_nil() {
            return;
        }
        let now = current_unix_millis_text();
        let runtime_state = self.ensure_session_runtime_state_mut(session_id);
        runtime_state.vmm.ai_tools_enabled = Some(enabled);
        runtime_state.updated_at = now;
        self.refresh_vmm_hook_config_snapshot();
    }
}
