//! LuaSkills panel management message handling for the workbench update loop.
//! 工作台更新循环中的 LuaSkills 面板管理消息处理。

use super::*;

impl VulcanWorkbench {
    /// Handles LuaSkills catalog and management messages outside the main update dispatcher.
    /// 在主更新分发器之外处理 LuaSkills 目录与管理操作消息。
    pub(super) fn update_luaskills_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::LuaSkillsCatalogLoaded(result) => {
                self.luaskills_operation_in_flight = false;
                self.luaskills_operation_progress = None;
                if let Some(handle) = self.luaskills_runtime.as_ref() {
                    let _ = luaskills_runtime::drain_luaskills_operation_progress_events(handle);
                }

                match result {
                    Ok(catalog) => {
                        let visible_skill_count = catalog.skills.len();
                        let enabled_skill_count =
                            catalog.skills.iter().filter(|skill| skill.enabled).count();
                        self.luaskills_notice = format!(
                            "LuaSkills 目录已载入：可见技能 {} 个，启用 {} 个，运行时入口 {} 个。",
                            visible_skill_count, enabled_skill_count, catalog.entry_count
                        );
                        self.status_message = self.luaskills_notice.clone();
                        self.luaskills_catalog = Some(catalog);
                    }
                    Err(error) => {
                        self.luaskills_catalog = None;
                        self.luaskills_notice = format!("LuaSkills 技能目录载入失败：{error}");
                        self.status_message = self.luaskills_notice.clone();
                        self.push_activity(
                            ActivityLevel::Warning,
                            "LuaSkills 技能目录载入失败",
                            error,
                        );
                    }
                }
                self.refresh_runtime_foundation();

                Task::none()
            }
            Message::PollLuaSkillsOperationProgressTick => {
                self.poll_luaskills_operation_progress();
                Task::none()
            }
            Message::RefreshLuaSkillsCatalog => {
                let Some(handle) = self.luaskills_runtime.clone() else {
                    self.luaskills_notice =
                        String::from("LuaSkills 运行时尚未就绪，暂时无法刷新。");
                    self.status_message = self.luaskills_notice.clone();
                    return Task::none();
                };

                if self.luaskills_operation_in_flight {
                    return Task::none();
                }

                self.luaskills_operation_in_flight = true;
                self.begin_luaskills_panel_operation(
                    LuaSkillsOperationVisualKind::Refresh,
                    None,
                    None,
                    String::from("正在刷新 LuaSkills 技能目录。"),
                );
                Task::perform(
                    luaskills_runtime::perform_luaskills_management_action(
                        handle,
                        LuaSkillsManagementAction::Refresh,
                    ),
                    Message::LuaSkillsManagementFinished,
                )
            }
            Message::LuaSkillsInstallSourceChoiceSelected(choice) => {
                self.luaskills_install_source_choice = choice;
                Task::none()
            }
            Message::LuaSkillsInstallSkillIdChanged(value) => {
                self.luaskills_install_skill_id = value;
                Task::none()
            }
            Message::LuaSkillsInstallSourceChanged(value) => {
                self.luaskills_install_source = value;
                Task::none()
            }
            Message::InstallLuaSkill => {
                let Some(handle) = self.luaskills_runtime.clone() else {
                    self.luaskills_notice =
                        String::from("LuaSkills 运行时尚未就绪，暂时无法安装技能。");
                    self.status_message = self.luaskills_notice.clone();
                    return Task::none();
                };

                if self.luaskills_operation_in_flight {
                    return Task::none();
                }

                if self.luaskills_install_source.trim().is_empty() {
                    self.luaskills_notice =
                        String::from("请先填写安装来源，例如 GitHub 仓库或包地址。");
                    self.status_message = self.luaskills_notice.clone();
                    return Task::none();
                }

                let install_skill_id = if self.luaskills_install_skill_id.trim().is_empty() {
                    None
                } else {
                    Some(self.luaskills_install_skill_id.clone())
                };
                let action = LuaSkillsManagementAction::Install {
                    skill_id: install_skill_id.clone(),
                    source: self.luaskills_install_source.clone(),
                    source_type: self.luaskills_install_source_choice,
                };
                self.luaskills_operation_in_flight = true;
                self.begin_luaskills_panel_operation(
                    LuaSkillsOperationVisualKind::Install,
                    install_skill_id,
                    Some(String::from("USER")),
                    String::from("正在安装 LuaSkills 技能。"),
                );
                Task::perform(
                    luaskills_runtime::perform_luaskills_management_action(handle, action),
                    Message::LuaSkillsManagementFinished,
                )
            }
            Message::UpdateLuaSkill(skill_id) => {
                let Some(handle) = self.luaskills_runtime.clone() else {
                    self.luaskills_notice =
                        String::from("LuaSkills 运行时尚未就绪，暂时无法更新技能。");
                    self.status_message = self.luaskills_notice.clone();
                    return Task::none();
                };

                if self.luaskills_operation_in_flight {
                    return Task::none();
                }

                self.luaskills_operation_in_flight = true;
                self.begin_luaskills_panel_operation(
                    LuaSkillsOperationVisualKind::Update,
                    Some(skill_id.clone()),
                    None,
                    format!("正在更新技能 `{skill_id}`。"),
                );
                Task::perform(
                    luaskills_runtime::perform_luaskills_management_action(
                        handle,
                        LuaSkillsManagementAction::Update { skill_id },
                    ),
                    Message::LuaSkillsManagementFinished,
                )
            }
            Message::EnableLuaSkill(skill_id) => {
                let Some(handle) = self.luaskills_runtime.clone() else {
                    self.luaskills_notice =
                        String::from("LuaSkills 运行时尚未就绪，暂时无法启用技能。");
                    self.status_message = self.luaskills_notice.clone();
                    return Task::none();
                };

                if self.luaskills_operation_in_flight {
                    return Task::none();
                }

                self.luaskills_operation_in_flight = true;
                self.begin_luaskills_panel_operation(
                    LuaSkillsOperationVisualKind::Enable,
                    Some(skill_id.clone()),
                    None,
                    format!("正在启用技能 `{skill_id}`。"),
                );
                Task::perform(
                    luaskills_runtime::perform_luaskills_management_action(
                        handle,
                        LuaSkillsManagementAction::Enable { skill_id },
                    ),
                    Message::LuaSkillsManagementFinished,
                )
            }
            Message::DisableLuaSkill(skill_id) => {
                let Some(handle) = self.luaskills_runtime.clone() else {
                    self.luaskills_notice =
                        String::from("LuaSkills 运行时尚未就绪，暂时无法停用技能。");
                    self.status_message = self.luaskills_notice.clone();
                    return Task::none();
                };

                if self.luaskills_operation_in_flight {
                    return Task::none();
                }

                self.luaskills_operation_in_flight = true;
                self.begin_luaskills_panel_operation(
                    LuaSkillsOperationVisualKind::Disable,
                    Some(skill_id.clone()),
                    None,
                    format!("正在停用技能 `{skill_id}`。"),
                );
                Task::perform(
                    luaskills_runtime::perform_luaskills_management_action(
                        handle,
                        LuaSkillsManagementAction::Disable { skill_id },
                    ),
                    Message::LuaSkillsManagementFinished,
                )
            }
            Message::UninstallLuaSkill(skill_id) => {
                let Some(handle) = self.luaskills_runtime.clone() else {
                    self.luaskills_notice =
                        String::from("LuaSkills 运行时尚未就绪，暂时无法卸载技能。");
                    self.status_message = self.luaskills_notice.clone();
                    return Task::none();
                };

                if self.luaskills_operation_in_flight {
                    return Task::none();
                }

                self.luaskills_operation_in_flight = true;
                self.begin_luaskills_panel_operation(
                    LuaSkillsOperationVisualKind::Uninstall,
                    Some(skill_id.clone()),
                    None,
                    format!("正在卸载技能 `{skill_id}`。"),
                );
                Task::perform(
                    luaskills_runtime::perform_luaskills_management_action(
                        handle,
                        LuaSkillsManagementAction::Uninstall { skill_id },
                    ),
                    Message::LuaSkillsManagementFinished,
                )
            }
            Message::SetLuaSkillToolVisibility {
                skill_id,
                local_name,
                hidden,
            } => {
                match self.persist_luaskills_tool_visibility(&skill_id, &local_name, hidden) {
                    Ok(true) => {
                        self.luaskills_notice = if hidden {
                            format!(
                                "已隐藏技能 `{skill_id}` 的工具 `{local_name}`，仅影响宿主对模型暴露。"
                            )
                        } else {
                            format!(
                                "已恢复技能 `{skill_id}` 的工具 `{local_name}`，宿主会重新向模型暴露它。"
                            )
                        };
                        self.status_message = self.luaskills_notice.clone();
                    }
                    Ok(false) => {}
                    Err(error) => {
                        self.luaskills_notice = format!("保存 LuaSkills 工具可见性失败：{error}");
                        self.status_message = self.luaskills_notice.clone();
                        self.push_activity(
                            ActivityLevel::Warning,
                            "保存 LuaSkills 工具可见性失败",
                            error,
                        );
                    }
                }

                Task::none()
            }
            Message::LuaSkillsManagementFinished(result) => {
                self.luaskills_operation_in_flight = false;
                self.luaskills_operation_progress = None;
                if let Some(handle) = self.luaskills_runtime.as_ref() {
                    let _ = luaskills_runtime::drain_luaskills_operation_progress_events(handle);
                }

                match result {
                    Ok(report) => {
                        self.luaskills_notice = report.message.clone();
                        self.status_message = report.message.clone();
                        self.luaskills_catalog = Some(report.catalog);
                        self.push_activity(
                            ActivityLevel::Success,
                            "LuaSkills 管理操作已完成",
                            report.message,
                        );
                    }
                    Err(error) => {
                        self.luaskills_notice = format!("LuaSkills 管理操作失败：{error}");
                        self.status_message = self.luaskills_notice.clone();
                        self.push_activity(ActivityLevel::Warning, "LuaSkills 管理操作失败", error);
                    }
                }
                self.refresh_runtime_foundation();

                Task::none()
            }
            _ => Task::none(),
        }
    }
}
