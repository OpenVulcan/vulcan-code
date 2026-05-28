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
}
