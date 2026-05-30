//! Prompt Skills panel message handling.
//! Prompt Skills 面板消息处理。

use super::*;
use std::path::PathBuf;
use vulcan_prompt_skills::PromptSkillCatalog;
use vulcan_vmm::{
    DeleteProjectResponse, DeleteUserResponse, MigrateProjectResponse, ProjectEntry, VmmConfigLayer,
};

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
            Message::GlobalVmmReceiveTimeoutChanged(value) => {
                self.vmm_settings_draft.receive_timeout_ms = value;
                self.vmm_settings_draft.notice =
                    String::from("全局 VMM 请求超时时间已修改，点击保存后生效。");
                Task::none()
            }
            Message::GlobalVmmHandshakeTimeoutChanged(value) => {
                self.vmm_settings_draft.handshake_timeout_ms = value;
                self.vmm_settings_draft.notice =
                    String::from("全局 VMM 握手超时时间已修改，点击保存后生效。");
                Task::none()
            }
            Message::GlobalVmmMaxReceiveMessageBytesChanged(value) => {
                self.vmm_settings_draft.max_receive_message_bytes = value;
                self.vmm_settings_draft.notice =
                    String::from("全局 VMM 最大接收消息字节数已修改，点击保存后生效。");
                Task::none()
            }
            Message::GlobalVmmKeepaliveTimeChanged(value) => {
                self.vmm_settings_draft.grpc_keepalive_time_ms = value;
                self.vmm_settings_draft.notice =
                    String::from("全局 VMM keepalive 间隔已修改，点击保存后生效。");
                Task::none()
            }
            Message::GlobalVmmKeepaliveTimeoutChanged(value) => {
                self.vmm_settings_draft.grpc_keepalive_timeout_ms = value;
                self.vmm_settings_draft.notice =
                    String::from("全局 VMM keepalive 超时已修改，点击保存后生效。");
                Task::none()
            }
            Message::GlobalVmmKeepalivePermitWithoutCallsChanged(value) => {
                self.vmm_settings_draft.grpc_keepalive_permit_without_calls = value;
                self.vmm_settings_draft.notice =
                    String::from("全局 VMM 空闲 keepalive 策略已修改，点击保存后生效。");
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
            Message::VmmWorkbenchScopeChanged(scope) => {
                self.vmm_management_workbench.scope = scope;
                self.vmm_management_workbench.notice =
                    format!("VMM 管理作用域已切换为{}。", scope.label());
                Task::none()
            }
            Message::VmmWorkbenchSectionChanged(section) => {
                self.vmm_management_workbench.section = section;
                self.vmm_management_workbench.notice =
                    format!("VMM 管理分区已切换为{}。", section.label());
                Task::none()
            }
            Message::VmmWorkbenchProjectScopeChanged(choice) => {
                self.vmm_management_workbench.selected_project_id = Some(choice.project_id);
                self.reload_project_vmm_settings_draft_for_workspace(PathBuf::from(
                    choice.workspace_path,
                ));
                self.vmm_management_workbench.notice =
                    format!("VMM 项目作用域已切换为{}。", choice.label);
                Task::none()
            }
            Message::VmmWorkbenchSectionFilterChanged(value) => {
                self.vmm_management_workbench.section_filter = value;
                Task::none()
            }
            Message::SaveVmmWorkbenchProjectConfiguration => {
                let project_id = match self.vmm_management_workbench.selected_project_id.clone() {
                    Some(project_id) => project_id,
                    None => {
                        self.vmm_management_workbench.notice =
                            String::from("请先选择要保存 VMM 设置的项目。");
                        return Task::none();
                    }
                };
                let workspace_path = match self.task_project_by_id(&project_id) {
                    Some(project) => PathBuf::from(project.path.clone()),
                    None => {
                        self.vmm_management_workbench.notice =
                            String::from("选中的项目已不存在，请重新选择。");
                        return Task::none();
                    }
                };
                self.project_vmm_settings_draft.project_override_enabled = true;
                match self.save_project_vmm_settings_draft_for_workspace(workspace_path) {
                    Ok(()) => {
                        self.vmm_management_workbench.notice =
                            self.project_vmm_settings_draft.settings.notice.clone();
                    }
                    Err(error) => {
                        self.project_vmm_settings_draft.settings.notice = error.clone();
                        self.vmm_management_workbench.notice = error;
                    }
                }
                Task::none()
            }
            Message::BindVmmWorkbenchUser(user_id) => {
                self.bind_vmm_workbench_user(user_id);
                Task::none()
            }
            Message::ClearVmmWorkbenchUserBinding => {
                self.clear_vmm_workbench_user_binding();
                Task::none()
            }
            Message::BindVmmWorkbenchProject(project_id) => {
                self.bind_vmm_workbench_project(project_id);
                Task::none()
            }
            Message::ClearVmmWorkbenchProjectBinding => {
                self.clear_vmm_workbench_project_binding();
                Task::none()
            }
            Message::VmmWorkbenchProfileTargetChanged(target) => {
                self.vmm_management_workbench.profile_target = target;
                self.vmm_management_workbench.notice =
                    format!("画像目标已切换为{}。", target.label());
                Task::none()
            }
            Message::VmmWorkbenchProfileInstructionChanged(value) => {
                self.vmm_management_workbench.profile_instruction = value;
                Task::none()
            }
            Message::VmmWorkbenchProfileFilterChanged(value) => {
                self.vmm_management_workbench.profile_filter = value;
                Task::none()
            }
            Message::VmmWorkbenchUserRefChanged(value) => {
                self.vmm_management_workbench.user_ref = value;
                Task::none()
            }
            Message::VmmWorkbenchUserFilterChanged(value) => {
                self.vmm_management_workbench.user_filter = value;
                Task::none()
            }
            Message::VmmWorkbenchUserDeleteConfirmationCodeChanged(value) => {
                self.vmm_management_workbench.user_delete_confirmation_code = value;
                Task::none()
            }
            Message::VmmWorkbenchProjectPathChanged(value) => {
                self.vmm_management_workbench.project_path = value;
                Task::none()
            }
            Message::VmmWorkbenchProjectFilterChanged(value) => {
                self.vmm_management_workbench.project_filter = value;
                Task::none()
            }
            Message::VmmWorkbenchProjectDeleteConfirmedChanged(value) => {
                self.vmm_management_workbench.project_delete_confirmed = value;
                Task::none()
            }
            Message::VmmWorkbenchMigrateSourceProjectPathChanged(value) => {
                self.vmm_management_workbench.migrate_source_project_path = value;
                Task::none()
            }
            Message::VmmWorkbenchMigrateTargetProjectPathChanged(value) => {
                self.vmm_management_workbench.migrate_target_project_path = value;
                Task::none()
            }
            Message::VmmWorkbenchMigrateConfirmedChanged(value) => {
                self.vmm_management_workbench.migrate_confirmed = value;
                Task::none()
            }
            Message::VmmWorkbenchToolSearchQueriesChanged(value) => {
                self.vmm_management_workbench.tool_search_queries = value;
                Task::none()
            }
            Message::VmmWorkbenchToolSearchTopKChanged(value) => {
                self.vmm_management_workbench.tool_search_top_k = value;
                Task::none()
            }
            Message::VmmWorkbenchToolTurnIdsChanged(value) => {
                self.vmm_management_workbench.tool_turn_ids = value;
                Task::none()
            }
            Message::VmmWorkbenchToolWriteAbstractChanged(value) => {
                self.vmm_management_workbench.tool_write_abstract = value;
                Task::none()
            }
            Message::VmmWorkbenchToolWriteDetailsChanged(value) => {
                self.vmm_management_workbench.tool_write_details = value;
                Task::none()
            }
            Message::VmmWorkbenchToolWriteScopeLevelChanged(value) => {
                self.vmm_management_workbench.tool_write_scope_level = value;
                Task::none()
            }
            Message::VmmWorkbenchToolWriteCategoryChanged(value) => {
                self.vmm_management_workbench.tool_write_category = value;
                Task::none()
            }
            Message::VmmWorkbenchToolWritePriorityChanged(value) => {
                self.vmm_management_workbench.tool_write_priority = value;
                Task::none()
            }
            Message::VmmWorkbenchToolWriteMemoryLevelChanged(value) => {
                self.vmm_management_workbench.tool_write_memory_level = value;
                Task::none()
            }
            Message::RunVmmWorkbenchAction(action) => self.run_vmm_workbench_action(action),
            Message::RunVmmWorkbenchSearchMemoryEvents => {
                let queries = self
                    .vmm_management_workbench
                    .tool_search_queries
                    .split(['\n', '|'])
                    .map(|query| query.trim().to_string())
                    .filter(|query| !query.is_empty())
                    .collect::<Vec<_>>();
                if queries.is_empty() {
                    self.vmm_management_workbench.notice =
                        String::from("SearchMemoryEvents 查询不能为空。");
                    return Task::none();
                }
                let top_k = match Self::parse_vmm_u32_field(
                    &self.vmm_management_workbench.tool_search_top_k,
                    "Top K",
                ) {
                    Ok(value) => value,
                    Err(error) => {
                        self.vmm_management_workbench.notice = error;
                        return Task::none();
                    }
                };
                self.set_vmm_tool_payload_preview(
                    serde_json::json!({
                        "method": "SearchMemoryEvents",
                        "queries": queries.clone(),
                        "top_k": top_k,
                    }),
                    serde_json::Value::Null,
                );
                self.run_vmm_workbench_action(VmmWorkbenchAction::SearchMemoryEvents {
                    queries,
                    top_k,
                })
            }
            Message::RunVmmWorkbenchGetTurnDetails => {
                let turn_ids = match Self::parse_vmm_u64_list(
                    &self.vmm_management_workbench.tool_turn_ids,
                    "Turn IDs",
                ) {
                    Ok(values) => values,
                    Err(error) => {
                        self.vmm_management_workbench.notice = error;
                        return Task::none();
                    }
                };
                self.set_vmm_tool_payload_preview(
                    serde_json::json!({
                        "method": "GetTurnDetails",
                        "turn_ids": turn_ids.clone(),
                    }),
                    serde_json::Value::Null,
                );
                self.run_vmm_workbench_action(VmmWorkbenchAction::GetTurnDetails { turn_ids })
            }
            Message::RunVmmWorkbenchWriteMemories => {
                let scope_level = match Self::parse_vmm_i32_field(
                    &self.vmm_management_workbench.tool_write_scope_level,
                    "Scope",
                ) {
                    Ok(value) => value,
                    Err(error) => {
                        self.vmm_management_workbench.notice = error;
                        return Task::none();
                    }
                };
                let category = match Self::parse_vmm_i32_field(
                    &self.vmm_management_workbench.tool_write_category,
                    "Category",
                ) {
                    Ok(value) => value,
                    Err(error) => {
                        self.vmm_management_workbench.notice = error;
                        return Task::none();
                    }
                };
                let priority = match Self::parse_vmm_i32_field(
                    &self.vmm_management_workbench.tool_write_priority,
                    "Priority",
                ) {
                    Ok(value) => value,
                    Err(error) => {
                        self.vmm_management_workbench.notice = error;
                        return Task::none();
                    }
                };
                let memory_level = match Self::parse_vmm_i32_field(
                    &self.vmm_management_workbench.tool_write_memory_level,
                    "Level",
                ) {
                    Ok(value) => value,
                    Err(error) => {
                        self.vmm_management_workbench.notice = error;
                        return Task::none();
                    }
                };
                let item = vulcan_vmm::WriteMemoryItem {
                    scope_level,
                    abstract_text: self.vmm_management_workbench.tool_write_abstract.clone(),
                    details: self.vmm_management_workbench.tool_write_details.clone(),
                    category,
                    priority,
                    memory_level,
                };
                self.set_vmm_tool_payload_preview(
                    serde_json::json!({
                        "method": "WriteMemories",
                        "session_id": self.selected_session.to_string(),
                        "items": [item.clone()],
                    }),
                    serde_json::Value::Null,
                );
                self.run_vmm_workbench_action(VmmWorkbenchAction::WriteMemories {
                    session_id: self.selected_session.to_string(),
                    item,
                })
            }
            Message::VmmWorkbenchActionFinished(result) => {
                self.vmm_management_workbench.in_flight = false;
                match result {
                    Ok(outcome) => {
                        self.apply_vmm_management_outcome(outcome);
                    }
                    Err(error) => {
                        self.vmm_management_workbench.notice = format!("VMM 管理调用失败：{error}");
                    }
                }
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

    /// Starts one dedicated VMM management workbench action.
    /// 启动一次专用 VMM 管理工作台动作。
    fn run_vmm_workbench_action(&mut self, action: VmmWorkbenchAction) -> Task<Message> {
        let (layer, management_user_ref, management_project_ref) =
            match self.vmm_workbench_effective_action_layer() {
                Ok(resolved) => resolved,
                Err(error) => {
                    self.vmm_management_workbench.notice = error;
                    return Task::none();
                }
            };
        self.vmm_management_workbench.in_flight = true;
        self.vmm_management_workbench.notice = String::from("正在执行 VMM 管理动作。");
        Task::perform(
            vmm_management::run_vmm_workbench_action(
                action,
                layer,
                management_user_ref,
                management_project_ref,
            ),
            Message::VmmWorkbenchActionFinished,
        )
    }

    /// Resolves the VMM layer and management references used by the workbench scope.
    /// 解析工作台作用域使用的 VMM 配置层与管理引用。
    fn vmm_workbench_effective_action_layer(
        &self,
    ) -> Result<(VmmConfigLayer, String, String), String> {
        match self.vmm_management_workbench.scope {
            VmmManagementScopeChoice::Global => {
                let layer = self.vmm_settings_draft.to_layer()?;
                Ok((
                    layer,
                    self.vmm_settings_draft.management_user_ref.clone(),
                    self.vmm_settings_draft.management_project_ref.clone(),
                ))
            }
            VmmManagementScopeChoice::Project => {
                let project_id = self
                    .vmm_management_workbench
                    .selected_project_id
                    .as_deref()
                    .ok_or_else(|| String::from("请先在 VMM 管理中心选择一个项目。"))?;
                let project = self
                    .task_project_by_id(project_id)
                    .ok_or_else(|| String::from("选中的项目已不存在，请重新选择。"))?;
                let project_draft_matches_selected =
                    self.project_vmm_settings_draft.workspace_path == project.path;
                let project_layer = if project_draft_matches_selected {
                    let mut layer = self.project_vmm_settings_draft.settings.to_layer()?;
                    layer.grpc_target = None;
                    layer.grpc_api_key = None;
                    layer.handshake_timeout_ms = None;
                    layer.receive_timeout_ms = None;
                    layer.max_receive_message_bytes = None;
                    layer.grpc_keepalive_time_ms = None;
                    layer.grpc_keepalive_timeout_ms = None;
                    layer.grpc_keepalive_permit_without_calls = None;
                    layer
                } else {
                    self.load_vmm_project_layer_by_workspace_path(&project.path)
                };
                let effective = vulcan_vmm::VmmEffectiveConfig::resolve(
                    &self.provider_configuration.vmm,
                    Some(&project_layer),
                    None,
                );
                let action_layer = VmmConfigLayer {
                    enabled: Some(effective.enabled),
                    grpc_target: (!effective.grpc_target.trim().is_empty())
                        .then(|| effective.grpc_target.clone()),
                    grpc_api_key: effective.grpc_api_key.clone(),
                    handshake_timeout_ms: Some(effective.handshake_timeout_ms),
                    receive_timeout_ms: Some(effective.receive_timeout_ms),
                    max_receive_message_bytes: effective.max_receive_message_bytes,
                    grpc_keepalive_time_ms: Some(effective.grpc_keepalive_time_ms),
                    grpc_keepalive_timeout_ms: Some(effective.grpc_keepalive_timeout_ms),
                    grpc_keepalive_permit_without_calls: Some(
                        effective.grpc_keepalive_permit_without_calls,
                    ),
                    user_id: effective.user_id,
                    project_id: effective.project_id,
                    visible_memory_injection: Some(effective.visible_memory_injection),
                    implicit_memory_turns: Some(effective.implicit_memory_turns),
                    profile_refresh_turns: Some(effective.profile_refresh_turns),
                    session_compact_recall: Some(effective.session_compact_recall),
                    ai_tools_enabled: Some(effective.ai_tools_enabled),
                    injection_tag: Some(effective.injection_tag),
                    injection_description: Some(effective.injection_description),
                    install: Some(effective.install),
                };
                let management_user_ref = action_layer
                    .user_id
                    .map(|value| value.to_string())
                    .unwrap_or_default();
                let management_project_ref = action_layer
                    .project_id
                    .map(|value| value.to_string())
                    .unwrap_or_default();
                let draft_user_ref = project_draft_matches_selected
                    .then(|| {
                        self.project_vmm_settings_draft
                            .settings
                            .management_user_ref
                            .trim()
                    })
                    .unwrap_or("");
                let draft_project_ref = project_draft_matches_selected
                    .then(|| {
                        self.project_vmm_settings_draft
                            .settings
                            .management_project_ref
                            .trim()
                    })
                    .unwrap_or("");
                Ok((
                    action_layer,
                    if draft_user_ref.is_empty() {
                        project_layer
                            .user_id
                            .map(|value| value.to_string())
                            .unwrap_or(management_user_ref)
                    } else {
                        draft_user_ref.to_string()
                    },
                    if draft_project_ref.is_empty() {
                        project_layer
                            .project_id
                            .map(|value| value.to_string())
                            .unwrap_or(management_project_ref)
                    } else {
                        draft_project_ref.to_string()
                    },
                ))
            }
        }
    }

    /// Loads one project VMM override by absolute workspace path.
    /// 按绝对工作区路径载入一个项目 VMM 覆盖。
    fn load_vmm_project_layer_by_workspace_path(&self, workspace_path: &str) -> VmmConfigLayer {
        crate::provider_config::load_project_configuration_file(&PathBuf::from(workspace_path))
            .map(|config| config.vmm)
            .unwrap_or_default()
    }

    /// Parses an unsigned 32-bit integer field for a VMM debug form.
    /// 为 VMM 调试表单解析 32 位无符号整数字段。
    fn parse_vmm_u32_field(value: &str, label: &str) -> Result<u32, String> {
        value
            .trim()
            .parse::<u32>()
            .map_err(|error| format!("{label} 必须是非负整数：{error}"))
    }

    /// Parses a signed 32-bit integer field for a VMM debug form.
    /// 为 VMM 调试表单解析 32 位有符号整数字段。
    fn parse_vmm_i32_field(value: &str, label: &str) -> Result<i32, String> {
        value
            .trim()
            .parse::<i32>()
            .map_err(|error| format!("{label} 必须是整数：{error}"))
    }

    /// Parses a comma, newline, or pipe separated unsigned identifier list.
    /// 解析逗号、换行或竖线分隔的无符号标识列表。
    fn parse_vmm_u64_list(value: &str, label: &str) -> Result<Vec<u64>, String> {
        let mut parsed = Vec::new();
        for item in value.split([',', '\n', '|']) {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            let value = item
                .parse::<u64>()
                .map_err(|error| format!("{label} 包含无效标识 `{item}`：{error}"))?;
            parsed.push(value);
        }
        if parsed.is_empty() {
            return Err(format!("{label} 不能为空。"));
        }
        Ok(parsed)
    }

    /// Updates tools-debug request and response JSON previews.
    /// 更新工具调试请求与响应 JSON 预览。
    fn set_vmm_tool_payload_preview(
        &mut self,
        request: serde_json::Value,
        response: serde_json::Value,
    ) {
        if !request.is_null() {
            self.vmm_management_workbench.tool_request_preview =
                serde_json::to_string_pretty(&request)
                    .unwrap_or_else(|error| format!("请求预览序列化失败：{error}"));
        }
        if !response.is_null() {
            self.vmm_management_workbench.tool_response_preview =
                serde_json::to_string_pretty(&response)
                    .unwrap_or_else(|error| format!("响应预览序列化失败：{error}"));
        } else if !request.is_null() {
            self.vmm_management_workbench.tool_response_preview.clear();
        }
    }

    /// Binds one VMM user id into the active global or project workbench scope.
    /// 将一个 VMM 用户标识绑定到当前全局或项目工作台作用域。
    fn bind_vmm_workbench_user(&mut self, user_id: u64) {
        match self.vmm_management_workbench.scope {
            VmmManagementScopeChoice::Global => {
                self.vmm_settings_draft.user_id = user_id.to_string();
                self.vmm_settings_draft.management_user_ref = user_id.to_string();
                match self.save_global_vmm_settings_draft() {
                    Ok(()) => {
                        self.vmm_management_workbench.notice =
                            format!("已把用户 #{user_id} 绑定到全局 VMM 配置。");
                    }
                    Err(error) => {
                        self.vmm_management_workbench.notice = format!("绑定全局用户失败：{error}");
                    }
                }
            }
            VmmManagementScopeChoice::Project => {
                if let Err(error) = self.prepare_selected_project_vmm_override() {
                    self.vmm_management_workbench.notice = error;
                    return;
                }
                self.project_vmm_settings_draft.settings.user_id = user_id.to_string();
                self.project_vmm_settings_draft.settings.management_user_ref = user_id.to_string();
                if let Err(error) = self.save_vmm_workbench_selected_project_configuration() {
                    self.vmm_management_workbench.notice = format!("绑定项目用户失败：{error}");
                } else {
                    self.vmm_management_workbench.notice =
                        format!("已把用户 #{user_id} 绑定到选中项目 VMM 配置。");
                }
            }
        }
    }

    /// Clears the VMM user binding in the active global or project workbench scope.
    /// 清空当前全局或项目工作台作用域里的 VMM 用户绑定。
    fn clear_vmm_workbench_user_binding(&mut self) {
        match self.vmm_management_workbench.scope {
            VmmManagementScopeChoice::Global => {
                self.vmm_settings_draft.user_id.clear();
                self.vmm_settings_draft.management_user_ref.clear();
                match self.save_global_vmm_settings_draft() {
                    Ok(()) => {
                        self.vmm_management_workbench.notice =
                            String::from("已清空全局 VMM 用户绑定。");
                    }
                    Err(error) => {
                        self.vmm_management_workbench.notice =
                            format!("清空全局用户绑定失败：{error}");
                    }
                }
            }
            VmmManagementScopeChoice::Project => {
                if let Err(error) = self.prepare_selected_project_vmm_override() {
                    self.vmm_management_workbench.notice = error;
                    return;
                }
                self.project_vmm_settings_draft.settings.user_id.clear();
                self.project_vmm_settings_draft
                    .settings
                    .management_user_ref
                    .clear();
                if let Err(error) = self.save_vmm_workbench_selected_project_configuration() {
                    self.vmm_management_workbench.notice = format!("清空项目用户绑定失败：{error}");
                } else {
                    self.vmm_management_workbench.notice =
                        String::from("已清空选中项目 VMM 用户绑定，后续继承全局。");
                }
            }
        }
    }

    /// Binds one VMM project id into the active global or project workbench scope.
    /// 将一个 VMM 项目标识绑定到当前全局或项目工作台作用域。
    fn bind_vmm_workbench_project(&mut self, project_id: u64) {
        match self.vmm_management_workbench.scope {
            VmmManagementScopeChoice::Global => {
                self.vmm_settings_draft.project_id = project_id.to_string();
                self.vmm_settings_draft.management_project_ref = project_id.to_string();
                match self.save_global_vmm_settings_draft() {
                    Ok(()) => {
                        self.vmm_management_workbench.notice =
                            format!("已把项目 #{project_id} 绑定到全局 VMM 配置。");
                    }
                    Err(error) => {
                        self.vmm_management_workbench.notice = format!("绑定全局项目失败：{error}");
                    }
                }
            }
            VmmManagementScopeChoice::Project => {
                if let Err(error) = self.prepare_selected_project_vmm_override() {
                    self.vmm_management_workbench.notice = error;
                    return;
                }
                self.project_vmm_settings_draft.settings.project_id = project_id.to_string();
                self.project_vmm_settings_draft
                    .settings
                    .management_project_ref = project_id.to_string();
                if let Err(error) = self.save_vmm_workbench_selected_project_configuration() {
                    self.vmm_management_workbench.notice = format!("绑定项目失败：{error}");
                } else {
                    self.vmm_management_workbench.notice =
                        format!("已把项目 #{project_id} 绑定到选中项目 VMM 配置。");
                }
            }
        }
    }

    /// Clears the VMM project binding in the active global or project workbench scope.
    /// 清空当前全局或项目工作台作用域里的 VMM 项目绑定。
    fn clear_vmm_workbench_project_binding(&mut self) {
        match self.vmm_management_workbench.scope {
            VmmManagementScopeChoice::Global => {
                self.vmm_settings_draft.project_id.clear();
                self.vmm_settings_draft.management_project_ref.clear();
                match self.save_global_vmm_settings_draft() {
                    Ok(()) => {
                        self.vmm_management_workbench.notice =
                            String::from("已清空全局 VMM 项目绑定。");
                    }
                    Err(error) => {
                        self.vmm_management_workbench.notice =
                            format!("清空全局项目绑定失败：{error}");
                    }
                }
            }
            VmmManagementScopeChoice::Project => {
                if let Err(error) = self.prepare_selected_project_vmm_override() {
                    self.vmm_management_workbench.notice = error;
                    return;
                }
                self.project_vmm_settings_draft.settings.project_id.clear();
                self.project_vmm_settings_draft
                    .settings
                    .management_project_ref
                    .clear();
                if let Err(error) = self.save_vmm_workbench_selected_project_configuration() {
                    self.vmm_management_workbench.notice = format!("清空项目绑定失败：{error}");
                } else {
                    self.vmm_management_workbench.notice =
                        String::from("已清空选中项目 VMM 项目绑定，后续继承全局。");
                }
            }
        }
    }

    /// Enables a selected project VMM override before applying scoped binding changes.
    /// 在写入作用域绑定前启用选中项目的 VMM 覆盖。
    fn prepare_selected_project_vmm_override(&mut self) -> Result<(), String> {
        let project_id = self
            .vmm_management_workbench
            .selected_project_id
            .clone()
            .ok_or_else(|| String::from("请先选择要设置 VMM 的项目。"))?;
        let project = self
            .task_project_by_id(&project_id)
            .ok_or_else(|| String::from("选中的项目已不存在，请重新选择。"))?;
        self.reload_project_vmm_settings_draft_for_workspace(PathBuf::from(project.path.clone()));
        self.project_vmm_settings_draft.project_override_enabled = true;
        Ok(())
    }

    /// Saves the selected project VMM draft used by the workbench.
    /// 保存工作台当前选中项目的 VMM 草稿。
    fn save_vmm_workbench_selected_project_configuration(&mut self) -> Result<(), String> {
        let project_id = self
            .vmm_management_workbench
            .selected_project_id
            .clone()
            .ok_or_else(|| String::from("请先选择要保存 VMM 设置的项目。"))?;
        let project = self
            .task_project_by_id(&project_id)
            .ok_or_else(|| String::from("选中的项目已不存在，请重新选择。"))?;
        self.save_project_vmm_settings_draft_for_workspace(PathBuf::from(project.path.clone()))
    }

    /// Applies one structured VMM management result to the workbench state.
    /// 将一个结构化 VMM 管理结果应用到工作台状态。
    fn apply_vmm_management_outcome(&mut self, outcome: VmmManagementOutcome) {
        let mut notice = outcome.summary();
        match outcome {
            VmmManagementOutcome::UsersAndProjects { users, projects } => {
                self.vmm_management_workbench.users = users.users;
                self.vmm_management_workbench.projects = projects.projects;
            }
            VmmManagementOutcome::ProfileNodes(response) => {
                self.vmm_management_workbench.profile_nodes = response.nodes;
            }
            VmmManagementOutcome::ProfileBundle(response) => {
                self.vmm_management_workbench.profile_bundle = Some(response);
            }
            VmmManagementOutcome::ProfileInstruction(response) => {
                self.vmm_management_workbench.profile_nodes = response.accepted_nodes.clone();
                self.vmm_management_workbench.profile_instruction.clear();
            }
            VmmManagementOutcome::DeleteUser(response) => {
                if let Some(repair_notice) = self.repair_vmm_user_bindings_after_delete(&response) {
                    notice = append_optional_notice(notice, repair_notice);
                }
            }
            VmmManagementOutcome::DeleteProject(response) => {
                if let Some(repair_notice) =
                    self.repair_vmm_project_bindings_after_delete(&response)
                {
                    notice = append_optional_notice(notice, repair_notice);
                }
            }
            VmmManagementOutcome::MigrateProject(response) => {
                if let Some(repair_notice) =
                    self.repair_vmm_project_bindings_after_migrate(&response)
                {
                    notice = append_optional_notice(notice, repair_notice);
                }
            }
            VmmManagementOutcome::SearchMemoryEvents(response) => {
                self.set_vmm_tool_payload_preview(
                    serde_json::Value::Null,
                    serde_json::to_value(&response).unwrap_or_else(
                        |error| serde_json::json!({ "serialization_error": error.to_string() }),
                    ),
                );
                self.vmm_management_workbench.memory_search = Some(response);
            }
            VmmManagementOutcome::GetTurnDetails(response) => {
                self.set_vmm_tool_payload_preview(
                    serde_json::Value::Null,
                    serde_json::to_value(&response).unwrap_or_else(
                        |error| serde_json::json!({ "serialization_error": error.to_string() }),
                    ),
                );
                self.vmm_management_workbench.turn_details = Some(response);
            }
            VmmManagementOutcome::WriteMemories(response) => {
                self.set_vmm_tool_payload_preview(
                    serde_json::Value::Null,
                    serde_json::to_value(&response).unwrap_or_else(
                        |error| serde_json::json!({ "serialization_error": error.to_string() }),
                    ),
                );
                self.vmm_management_workbench.write_memories = Some(response);
            }
            _ => {}
        }
        self.vmm_management_workbench.notice = notice;
    }

    /// Repairs VMM user bindings after a confirmed backend deletion.
    /// 在后端确认删除用户后修正 VMM 用户绑定。
    fn repair_vmm_user_bindings_after_delete(
        &mut self,
        response: &DeleteUserResponse,
    ) -> Option<String> {
        if response.requires_confirmation {
            return None;
        }
        let Some(deleted_user_id) = response.user.as_ref().map(|user| user.user_id) else {
            return None;
        };
        let mut notice = None;
        let previous_global = self.provider_configuration.vmm.clone();
        let mut global_changed = false;
        if self.provider_configuration.vmm.user_id == Some(deleted_user_id) {
            self.provider_configuration.vmm.user_id = None;
            if self.vmm_settings_draft.user_id.trim() == deleted_user_id.to_string() {
                self.vmm_settings_draft.user_id.clear();
            }
            if self.vmm_settings_draft.management_user_ref.trim() == deleted_user_id.to_string() {
                self.vmm_settings_draft.management_user_ref.clear();
            }
            global_changed = true;
        }
        let project_changed = self
            .repair_project_vmm_bindings(|layer| clear_vmm_user_binding(layer, deleted_user_id));
        let (project_changed, project_notice) = project_changed;
        notice = merge_optional_notice(notice, project_notice);
        if global_changed {
            if let Err(error) =
                crate::provider_config::save_configuration_file(&self.provider_configuration)
            {
                self.provider_configuration.vmm = previous_global;
                self.reload_global_vmm_settings_draft();
                notice = merge_optional_notice(
                    notice,
                    Some(format!("用户已删除，但清理全局 VMM 绑定失败：{error}")),
                );
            }
        }
        if project_changed {
            self.reload_open_project_vmm_draft_if_needed();
        }
        if global_changed || project_changed {
            self.refresh_vmm_hook_config_snapshot();
            self.refresh_runtime_foundation();
        }
        notice
    }

    /// Repairs VMM project bindings after a confirmed backend deletion.
    /// 在后端确认删除项目后修正 VMM 项目绑定。
    fn repair_vmm_project_bindings_after_delete(
        &mut self,
        response: &DeleteProjectResponse,
    ) -> Option<String> {
        if response.needs_confirm {
            return None;
        }
        let Some(deleted_project_id) = response.project.as_ref().map(|project| project.project_id)
        else {
            return None;
        };
        let mut notice = None;
        let previous_global = self.provider_configuration.vmm.clone();
        let mut global_changed = false;
        if self.provider_configuration.vmm.project_id == Some(deleted_project_id) {
            self.provider_configuration.vmm.project_id = None;
            if self.vmm_settings_draft.project_id.trim() == deleted_project_id.to_string() {
                self.vmm_settings_draft.project_id.clear();
            }
            if self.vmm_settings_draft.management_project_ref.trim()
                == deleted_project_id.to_string()
            {
                self.vmm_settings_draft.management_project_ref.clear();
            }
            global_changed = true;
        }
        let project_changed = self.repair_project_vmm_bindings(|layer| {
            clear_vmm_project_binding(layer, deleted_project_id)
        });
        let (project_changed, project_notice) = project_changed;
        notice = merge_optional_notice(notice, project_notice);
        if global_changed {
            if let Err(error) =
                crate::provider_config::save_configuration_file(&self.provider_configuration)
            {
                self.provider_configuration.vmm = previous_global;
                self.reload_global_vmm_settings_draft();
                notice = merge_optional_notice(
                    notice,
                    Some(format!("项目已删除，但清理全局 VMM 绑定失败：{error}")),
                );
            }
        }
        if project_changed {
            self.reload_open_project_vmm_draft_if_needed();
        }
        if global_changed || project_changed {
            self.refresh_vmm_hook_config_snapshot();
            self.refresh_runtime_foundation();
        }
        notice
    }

    /// Repairs VMM project bindings after a confirmed backend migration.
    /// 在后端确认迁移项目后修正 VMM 项目绑定。
    fn repair_vmm_project_bindings_after_migrate(
        &mut self,
        response: &MigrateProjectResponse,
    ) -> Option<String> {
        if response.needs_confirm {
            return None;
        }
        let Some(source_project_id) = response
            .source_project
            .as_ref()
            .map(|project| project.project_id)
        else {
            return None;
        };
        let Some(target_project) = response.target_project.as_ref() else {
            return None;
        };
        let mut notice = None;
        let previous_global = self.provider_configuration.vmm.clone();
        let mut global_changed = false;
        if self.provider_configuration.vmm.project_id == Some(source_project_id) {
            self.provider_configuration.vmm.project_id = Some(target_project.project_id);
            self.vmm_settings_draft.project_id = target_project.project_id.to_string();
            self.vmm_settings_draft.management_project_ref =
                vmm_project_management_ref(target_project);
            global_changed = true;
        }
        let project_changed = self.repair_project_vmm_bindings(|layer| {
            replace_vmm_project_binding(layer, source_project_id, target_project.project_id)
        });
        let (project_changed, project_notice) = project_changed;
        notice = merge_optional_notice(notice, project_notice);
        if global_changed {
            if let Err(error) =
                crate::provider_config::save_configuration_file(&self.provider_configuration)
            {
                self.provider_configuration.vmm = previous_global;
                self.reload_global_vmm_settings_draft();
                notice = merge_optional_notice(
                    notice,
                    Some(format!("项目已迁移，但更新全局 VMM 绑定失败：{error}")),
                );
            }
        }
        if project_changed {
            self.reload_open_project_vmm_draft_if_needed();
        }
        if global_changed || project_changed {
            self.refresh_vmm_hook_config_snapshot();
            self.refresh_runtime_foundation();
        }
        notice
    }

    /// Applies one project VMM binding repair across all known task projects.
    /// 对所有已知任务项目应用一次 VMM 绑定修正。
    fn repair_project_vmm_bindings(
        &mut self,
        mut repair: impl FnMut(&mut VmmConfigLayer) -> bool,
    ) -> (bool, Option<String>) {
        let projects: Vec<_> = self.task_projects.iter().cloned().collect();
        let mut any_changed = false;
        let mut notice = None;
        for project in projects {
            let workspace_path = PathBuf::from(project.path);
            let mut project_config =
                crate::provider_config::load_project_configuration_file(&workspace_path)
                    .unwrap_or_default();
            if !repair(&mut project_config.vmm) {
                continue;
            }
            if let Err(error) = crate::provider_config::save_project_configuration_file(
                &workspace_path,
                &project_config,
            ) {
                notice =
                    merge_optional_notice(notice, Some(format!("修正项目 VMM 绑定失败：{error}")));
                continue;
            }
            any_changed = true;
        }
        (any_changed, notice)
    }

    /// Reloads the currently open project VMM draft after background project repair.
    /// 后台修正项目配置后重载当前打开的项目 VMM 草稿。
    fn reload_open_project_vmm_draft_if_needed(&mut self) {
        if self
            .project_vmm_settings_draft
            .workspace_path
            .trim()
            .is_empty()
        {
            return;
        }
        self.reload_project_vmm_settings_draft_for_workspace(PathBuf::from(
            self.project_vmm_settings_draft.workspace_path.clone(),
        ));
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

/// Clears one VMM user binding when it points at a deleted user.
/// 当 VMM 用户绑定指向已删除用户时清空该绑定。
fn clear_vmm_user_binding(layer: &mut VmmConfigLayer, deleted_user_id: u64) -> bool {
    if layer.user_id != Some(deleted_user_id) {
        return false;
    }
    layer.user_id = None;
    true
}

/// Clears one VMM project binding when it points at a deleted project.
/// 当 VMM 项目绑定指向已删除项目时清空该绑定。
fn clear_vmm_project_binding(layer: &mut VmmConfigLayer, deleted_project_id: u64) -> bool {
    if layer.project_id != Some(deleted_project_id) {
        return false;
    }
    layer.project_id = None;
    true
}

/// Replaces one VMM project binding after a successful project migration.
/// 项目迁移成功后替换一个 VMM 项目绑定。
fn replace_vmm_project_binding(
    layer: &mut VmmConfigLayer,
    source_project_id: u64,
    target_project_id: u64,
) -> bool {
    if layer.project_id != Some(source_project_id) {
        return false;
    }
    layer.project_id = Some(target_project_id);
    true
}

/// Builds the best management reference for one VMM project entry.
/// 为一条 VMM 项目记录构造最合适的管理引用。
fn vmm_project_management_ref(project: &ProjectEntry) -> String {
    if !project.display_path.trim().is_empty() {
        return project.display_path.clone();
    }
    project.project_id.to_string()
}

/// Appends an optional repair notice to the main management summary.
/// 将可选修正提示追加到主管理摘要后。
fn append_optional_notice(mut summary: String, notice: String) -> String {
    if notice.trim().is_empty() {
        return summary;
    }
    if !summary.trim().is_empty() {
        summary.push('\n');
    }
    summary.push_str(notice.trim());
    summary
}

/// Merges two optional notices without losing earlier repair diagnostics.
/// 合并两个可选提示，避免丢失较早的修正诊断。
fn merge_optional_notice(current: Option<String>, next: Option<String>) -> Option<String> {
    match (current, next) {
        (Some(current), Some(next)) => Some(append_optional_notice(current, next)),
        (Some(current), None) => Some(current),
        (None, Some(next)) => Some(next),
        (None, None) => None,
    }
}
