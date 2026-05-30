//! VMM settings view helpers shared by global and project configuration surfaces.
//! 全局与项目配置界面共享的 VMM 设置视图辅助。

use super::*;
use iced::Length;
use iced::widget::column;
use std::fmt;

impl fmt::Display for VmmManagementScopeChoice {
    /// Formats one VMM management scope for pick-list display.
    /// 将一个 VMM 管理作用域格式化为下拉选择展示文本。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

impl fmt::Display for VmmManagementSection {
    /// Formats one VMM management section for pick-list display.
    /// 将一个 VMM 管理分区格式化为下拉选择展示文本。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

impl fmt::Display for VmmProfileTargetChoice {
    /// Formats one VMM profile target for pick-list display.
    /// 将一个 VMM 画像目标格式化为下拉选择展示文本。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

impl VulcanWorkbench {
    /// Renders the top-level VulcanMemoryMesh configuration panel.
    /// 渲染顶层 VulcanMemoryMesh 配置面板。
    pub(super) fn view_vulcan_memory_mesh_panel(&self) -> Element<'_, Message> {
        let selected_session = self.selected_session();
        let selected_session_id = selected_session.map(|session| session.id);
        let selected_project_id = selected_session.and_then(|session| session.project_id.clone());
        let selected_project = selected_project_id
            .as_deref()
            .and_then(|project_id| self.task_project_by_id(project_id));
        let session_layer = selected_session_id
            .and_then(|session_id| self.session_runtime_state.get(&session_id))
            .map(|state| &state.vmm);
        let project_layer = self.current_project_vmm_configuration(self.selected_session);
        let effective = vulcan_vmm::VmmEffectiveConfig::resolve(
            &self.provider_configuration.vmm,
            Some(&project_layer),
            session_layer,
        );
        let diagnostic = effective.diagnose_memory_flow(!effective.grpc_target.trim().is_empty());
        let project_button: Element<'_, Message> = if let Some(project) = selected_project {
            button("打开当前项目设置")
                .style(button::secondary)
                .on_press(Message::OpenTaskProjectSettings(project.id.clone()))
                .into()
        } else {
            button("打开当前项目设置").style(button::secondary).into()
        };
        let effective_card = container(
            column![
                row![
                    column![
                        text("当前会话有效配置").size(16),
                        text("这里展示全局、项目和会话覆盖合并后的实际 VMM 状态。").size(12),
                    ]
                    .spacing(3)
                    .width(Fill),
                    project_button,
                ]
                .spacing(10)
                .align_y(Center),
                row![
                    text(format!(
                        "gRPC：{}",
                        if effective.grpc_target.trim().is_empty() {
                            "未配置"
                        } else {
                            effective.grpc_target.as_str()
                        }
                    ))
                    .size(12),
                    text(format!(
                        "user/project：{}/{}",
                        effective
                            .user_id
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| String::from("-")),
                        effective
                            .project_id
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| String::from("-")),
                    ))
                    .size(12),
                    text(format!(
                        "记忆：{}",
                        if effective.enabled {
                            "启用"
                        } else {
                            "关闭"
                        }
                    ))
                    .size(12),
                    text(format!(
                        "AI 工具：{}",
                        if effective.ai_tools_enabled {
                            "启用"
                        } else {
                            "关闭"
                        }
                    ))
                    .size(12),
                ]
                .spacing(12)
                .wrap(),
                text(format!("诊断：{diagnostic:?}")).size(12),
                text(format!(
                    "当前项目：{}",
                    selected_project
                        .map(|project| project.title.as_str())
                        .unwrap_or("未绑定项目")
                ))
                .size(12),
            ]
            .spacing(10),
        )
        .padding(12)
        .style(container::bordered_box);

        container(
            column![
                container(
                    column![
                        text(
                            MainDisplayPanel::VulcanMemoryMesh
                                .localized_section_title(self.current_locale())
                        )
                        .size(22),
                        text(
                            MainDisplayPanel::VulcanMemoryMesh
                                .localized_description(self.current_locale())
                        )
                        .size(13),
                    ]
                    .spacing(4),
                )
                .padding([4, 2]),
                scrollable(
                    column![
                        effective_card,
                        self.view_vmm_management_workbench(),
                        self.view_global_vmm_settings_card(),
                    ]
                    .spacing(12)
                )
                .height(Fill),
            ]
            .spacing(12)
            .height(Fill),
        )
        .padding(16)
        .height(Fill)
        .width(Fill)
        .into()
    }

    /// Renders the dedicated VMM management workbench.
    /// 渲染专用 VMM 管理工作台。
    fn view_vmm_management_workbench(&self) -> Element<'_, Message> {
        let state = &self.vmm_management_workbench;
        let scope_choices = [
            VmmManagementScopeChoice::Global,
            VmmManagementScopeChoice::Project,
        ];
        let running_suffix = if state.in_flight { " · 执行中" } else { "" };
        let project_choices = self.vmm_project_scope_choices();
        let selected_project = state
            .selected_project_id
            .as_deref()
            .and_then(|project_id| {
                project_choices
                    .iter()
                    .find(|choice| choice.project_id == project_id)
            })
            .cloned();
        let project_picker: Element<'_, Message> =
            if state.scope == VmmManagementScopeChoice::Project {
                pick_list(
                    project_choices,
                    selected_project,
                    Message::VmmWorkbenchProjectScopeChanged,
                )
                .placeholder("选择项目")
                .width(Length::Fixed(260.0))
                .into()
            } else {
                container(text("项目选择仅在“设置到项目”时启用。").size(12))
                    .padding([6, 0])
                    .into()
            };
        let section_filter = state.section_filter.trim().to_lowercase();
        let mut visible_section_count = 0usize;
        let mut menu = column![].spacing(6);
        for section in VmmManagementSection::all().iter().copied() {
            let matches_filter = section_filter.is_empty()
                || section.label().to_lowercase().contains(&section_filter)
                || section.key().to_lowercase().contains(&section_filter)
                || section
                    .description()
                    .to_lowercase()
                    .contains(&section_filter);
            if !matches_filter {
                continue;
            }
            visible_section_count += 1;
            let action = Message::VmmWorkbenchSectionChanged(section);
            let button_style = if section == state.section {
                button::primary
            } else {
                button::secondary
            };
            menu = menu.push(
                button(
                    column![
                        text(section.key()).size(12),
                        text(section.label()).size(13),
                        text(section.description()).size(11),
                    ]
                    .spacing(2),
                )
                .style(button_style)
                .width(Fill)
                .on_press(action),
            );
        }
        if visible_section_count == 0 {
            menu = menu.push(text("没有匹配的管理入口。").size(12));
        }
        let active_draft = match state.scope {
            VmmManagementScopeChoice::Global => &self.vmm_settings_draft,
            VmmManagementScopeChoice::Project => &self.project_vmm_settings_draft.settings,
        };
        let global_grpc = self
            .provider_configuration
            .vmm
            .grpc_target
            .as_deref()
            .filter(|target| !target.trim().is_empty())
            .unwrap_or("未配置")
            .to_string();
        let active_user = if active_draft.user_id.trim().is_empty() {
            "未设置"
        } else {
            active_draft.user_id.as_str()
        };
        let active_project = if active_draft.project_id.trim().is_empty() {
            "未设置"
        } else {
            active_draft.project_id.as_str()
        };
        let memory_mode = if active_draft.enabled {
            "隐藏记忆"
        } else {
            "关闭"
        };
        let detail = match state.section {
            VmmManagementSection::UserManager => self.view_vmm_user_manager_panel(),
            VmmManagementSection::ProjectManager => self.view_vmm_project_manager_panel(),
            VmmManagementSection::ProfileCenter => {
                self.view_vmm_profile_nodes_panel(VmmProfileTargetChoice::all())
            }
            VmmManagementSection::ProfileBundleTest => self.view_vmm_profile_bundle_panel(),
            VmmManagementSection::ToolsDebug => self.view_vmm_tools_debug_panel(),
            VmmManagementSection::MemorySettings => self.view_vmm_memory_settings_panel(),
            VmmManagementSection::GrpcTransport => self.view_vmm_grpc_transport_panel(),
        };
        container(
            column![
                row![
                    column![
                        text("VMM 管理工作台").size(16),
                        text(
                            "对应 OpenCode 插件 TUI 管理面：用户、项目、画像、工具调试、记忆设置与 gRPC 传输。"
                        )
                        .size(12),
                    ]
                    .spacing(3)
                    .width(Fill),
                    pick_list(
                        scope_choices,
                        Some(state.scope),
                        Message::VmmWorkbenchScopeChanged,
                    )
                    .width(Length::Fixed(130.0)),
                    project_picker,
                ]
                .spacing(10)
                .align_y(Center),
                container(
                    row![
                        text(format!("gRPC：{global_grpc}")).size(12),
                        text(format!("用户：{active_user}")).size(12),
                        text(format!("项目：{active_project}")).size(12),
                        text(format!("模式：{memory_mode}")).size(12),
                        text(format!("TTL：{}", active_draft.implicit_memory_turns)).size(12),
                        text("压缩感知：自动").size(12),
                    ]
                    .spacing(12)
                    .wrap(),
                )
                .padding(8)
                .style(container::rounded_box)
                .width(Fill),
                row![
                    button("测试连接")
                        .style(button::secondary)
                        .on_press(Message::RunVmmWorkbenchAction(VmmWorkbenchAction::Healthz)),
                    button("刷新用户/项目").style(button::secondary).on_press(
                        Message::RunVmmWorkbenchAction(VmmWorkbenchAction::LoadUsersAndProjects,)
                    ),
                    button("解析绑定").style(button::secondary).on_press(
                        Message::RunVmmWorkbenchAction(VmmWorkbenchAction::ResolveUserAndProject,)
                    ),
                    button("预检项目").style(button::secondary).on_press(
                        Message::RunVmmWorkbenchAction(VmmWorkbenchAction::EnsureProjectPreview,)
                    ),
                    button("读取完整画像").style(button::secondary).on_press(
                        Message::RunVmmWorkbenchAction(VmmWorkbenchAction::LoadProfileBundle,)
                    ),
                ]
                .spacing(8)
                .wrap(),
                text(format!("{}{}", state.notice, running_suffix)).size(12),
                row![
                    container(
                        column![
                            text("管理入口").size(13),
                            text_input("筛选入口", &state.section_filter)
                                .on_input(Message::VmmWorkbenchSectionFilterChanged)
                                .padding(8),
                            menu,
                            container(
                                column![
                                    text(state.section.key()).size(11),
                                    text(state.section.label()).size(12),
                                    text(state.section.description()).size(11),
                                ]
                                .spacing(4),
                            )
                            .padding(8)
                            .style(container::rounded_box),
                        ]
                        .spacing(8),
                    )
                    .padding(10)
                    .style(container::rounded_box)
                    .width(Length::Fixed(260.0)),
                    container(detail)
                        .padding(10)
                        .style(container::rounded_box)
                        .width(Fill),
                ]
                .spacing(12),
            ]
            .spacing(12),
        )
        .padding(12)
        .style(container::bordered_box)
        .into()
    }

    /// Builds project-scope picker choices from the current task-project list.
    /// 基于当前任务项目列表构建项目作用域下拉选项。
    fn vmm_project_scope_choices(&self) -> Vec<VmmProjectScopeChoice> {
        self.task_projects
            .iter()
            .map(|project| VmmProjectScopeChoice {
                project_id: project.id.clone(),
                workspace_path: project.path.clone(),
                label: format!("{} · {}", project.title, project.path),
            })
            .collect()
    }

    /// Renders the OpenCode-style user manager panel.
    /// 渲染 OpenCode 风格的用户管理面板。
    fn view_vmm_user_manager_panel(&self) -> Element<'_, Message> {
        let state = &self.vmm_management_workbench;
        let resolve_action = VmmWorkbenchAction::ResolveUser {
            user_ref: state.user_ref.clone(),
            confirm_create: false,
        };
        let create_action = VmmWorkbenchAction::ResolveUser {
            user_ref: state.user_ref.clone(),
            confirm_create: true,
        };
        let delete_action = VmmWorkbenchAction::DeleteUser {
            user_ref: state.user_ref.clone(),
            confirmation_code: state.user_delete_confirmation_code.clone(),
        };
        column![
            text("用户管理").size(15),
            text("长期用户列表、解析用户引用、创建确认和删除确认。").size(12),
            container(
                column![
                    text("操作").size(13),
                    row![
                        self.view_vmm_text_field(
                            "用户引用",
                            "ID 或用户名",
                            &state.user_ref,
                            Message::VmmWorkbenchUserRefChanged,
                        ),
                        self.view_vmm_text_field(
                            "删除确认码",
                            "首次删除后返回",
                            &state.user_delete_confirmation_code,
                            Message::VmmWorkbenchUserDeleteConfirmationCodeChanged,
                        ),
                    ]
                    .spacing(10),
                    row![
                        button("解析用户")
                            .style(button::secondary)
                            .on_press(Message::RunVmmWorkbenchAction(resolve_action)),
                        button("确认创建")
                            .style(button::secondary)
                            .on_press(Message::RunVmmWorkbenchAction(create_action)),
                        button("删除用户")
                            .style(button::danger)
                            .on_press(Message::RunVmmWorkbenchAction(delete_action)),
                        button("刷新用户").style(button::secondary).on_press(
                            Message::RunVmmWorkbenchAction(
                                VmmWorkbenchAction::LoadUsersAndProjects,
                            )
                        ),
                        button("清空当前用户绑定")
                            .style(button::secondary)
                            .on_press(Message::ClearVmmWorkbenchUserBinding),
                    ]
                    .spacing(8)
                    .wrap(),
                ]
                .spacing(8),
            )
            .padding(10)
            .style(container::rounded_box),
            self.view_vmm_user_list_panel(),
        ]
        .spacing(10)
        .into()
    }

    /// Renders the filtered OpenCode-style user list and binding summary.
    /// 渲染带筛选与绑定摘要的 OpenCode 风格用户列表。
    fn view_vmm_user_list_panel(&self) -> Element<'_, Message> {
        let state = &self.vmm_management_workbench;
        let user_filter = state.user_filter.trim().to_lowercase();
        let filtered_users = state
            .users
            .iter()
            .filter(|user| {
                user_filter.is_empty()
                    || user.user_name.to_lowercase().contains(&user_filter)
                    || user.user_id.to_string().contains(&user_filter)
            })
            .collect::<Vec<_>>();
        let user_rows = if filtered_users.is_empty() {
            column![text("没有匹配的用户。").size(12)].spacing(6)
        } else {
            filtered_users
                .iter()
                .take(12)
                .fold(column![].spacing(6), |column, user| {
                    column.push(
                        container(
                            column![
                                row![
                                    text(&user.user_name).size(13).width(Fill),
                                    text(format!("ID：{}", user.user_id)).size(11),
                                ]
                                .spacing(8)
                                .align_y(Center),
                                row![
                                    text("选择后会写入当前全局或项目作用域。").size(11),
                                    space::horizontal(),
                                    button("绑定到当前范围")
                                        .style(button::secondary)
                                        .on_press(Message::BindVmmWorkbenchUser(user.user_id)),
                                ]
                                .spacing(8)
                                .align_y(Center),
                            ]
                            .spacing(4),
                        )
                        .padding(8)
                        .style(container::rounded_box),
                    )
                })
        };
        let project_label = self
            .vmm_management_workbench
            .selected_project_id
            .as_deref()
            .and_then(|project_id| self.task_project_by_id(project_id))
            .map(|project| project.title.as_str())
            .unwrap_or("未选择项目");
        container(
            column![
                row![
                    column![
                        text(format!(
                            "用户 {} / {}",
                            filtered_users.len(),
                            state.users.len()
                        ))
                        .size(13),
                        text(format!("当前范围：{}", state.scope.label())).size(11),
                    ]
                    .spacing(3)
                    .width(Fill),
                    text_input("筛选用户", &state.user_filter)
                        .on_input(Message::VmmWorkbenchUserFilterChanged)
                        .padding(8)
                        .width(Length::Fixed(220.0)),
                ]
                .spacing(10)
                .align_y(Center),
                row![
                    container(
                        column![
                            text("绑定摘要").size(12),
                            text(format!(
                                "全局用户：{}",
                                self.vmm_binding_value_label(&self.vmm_settings_draft.user_id)
                            ))
                            .size(11),
                            text(format!(
                                "{} 用户：{}",
                                project_label,
                                self.vmm_binding_value_label(
                                    &self.project_vmm_settings_draft.settings.user_id
                                )
                            ))
                            .size(11),
                        ]
                        .spacing(4),
                    )
                    .padding(8)
                    .style(container::rounded_box)
                    .width(Length::Fixed(220.0)),
                    scrollable(user_rows)
                        .height(Length::Fixed(260.0))
                        .width(Fill),
                ]
                .spacing(10),
            ]
            .spacing(8),
        )
        .padding(10)
        .style(container::rounded_box)
        .width(Fill)
        .into()
    }

    /// Renders the OpenCode-style project manager panel.
    /// 渲染 OpenCode 风格的项目管理面板。
    fn view_vmm_project_manager_panel(&self) -> Element<'_, Message> {
        let state = &self.vmm_management_workbench;
        let ensure_preview = VmmWorkbenchAction::EnsureProject {
            project_path: state.project_path.clone(),
            confirm_create: false,
        };
        let ensure_create = VmmWorkbenchAction::EnsureProject {
            project_path: state.project_path.clone(),
            confirm_create: true,
        };
        let delete_action = VmmWorkbenchAction::DeleteProject {
            project_path: state.project_path.clone(),
            confirm_delete: state.project_delete_confirmed,
        };
        let migrate_action = VmmWorkbenchAction::MigrateProject {
            source_project_path: state.migrate_source_project_path.clone(),
            target_project_path: state.migrate_target_project_path.clone(),
            confirm_migrate: state.migrate_confirmed,
        };
        column![
            text("项目管理").size(15),
            text("项目路径预检、创建确认、删除确认和迁移确认。").size(12),
            container(
                column![
                    text("操作").size(13),
                    self.view_vmm_text_field(
                        "项目路径",
                        "Team/Space/Project",
                        &state.project_path,
                        Message::VmmWorkbenchProjectPathChanged,
                    ),
                    row![
                        button("预检项目")
                            .style(button::secondary)
                            .on_press(Message::RunVmmWorkbenchAction(ensure_preview)),
                        button("确认创建")
                            .style(button::secondary)
                            .on_press(Message::RunVmmWorkbenchAction(ensure_create)),
                        checkbox(state.project_delete_confirmed)
                            .on_toggle(Message::VmmWorkbenchProjectDeleteConfirmedChanged),
                        text("确认删除").size(13),
                        button("删除项目")
                            .style(button::danger)
                            .on_press(Message::RunVmmWorkbenchAction(delete_action)),
                    ]
                    .spacing(8)
                    .wrap(),
                    row![
                        self.view_vmm_text_field(
                            "迁移源项目",
                            "OldTeam/OldSpace/OldProject",
                            &state.migrate_source_project_path,
                            Message::VmmWorkbenchMigrateSourceProjectPathChanged,
                        ),
                        self.view_vmm_text_field(
                            "迁移目标项目",
                            "NewTeam/NewSpace/NewProject",
                            &state.migrate_target_project_path,
                            Message::VmmWorkbenchMigrateTargetProjectPathChanged,
                        ),
                    ]
                    .spacing(10),
                    row![
                        checkbox(state.migrate_confirmed)
                            .on_toggle(Message::VmmWorkbenchMigrateConfirmedChanged),
                        text("确认迁移").size(13),
                        button("执行迁移")
                            .style(button::danger)
                            .on_press(Message::RunVmmWorkbenchAction(migrate_action)),
                        button("刷新项目").style(button::secondary).on_press(
                            Message::RunVmmWorkbenchAction(
                                VmmWorkbenchAction::LoadUsersAndProjects,
                            )
                        ),
                        button("清空当前项目绑定")
                            .style(button::secondary)
                            .on_press(Message::ClearVmmWorkbenchProjectBinding),
                    ]
                    .spacing(8)
                    .align_y(Center),
                ]
                .spacing(8),
            )
            .padding(10)
            .style(container::rounded_box),
            self.view_vmm_project_list_panel(),
        ]
        .spacing(10)
        .into()
    }

    /// Renders the filtered OpenCode-style project list and binding summary.
    /// 渲染带筛选与绑定摘要的 OpenCode 风格项目列表。
    fn view_vmm_project_list_panel(&self) -> Element<'_, Message> {
        let state = &self.vmm_management_workbench;
        let project_filter = state.project_filter.trim().to_lowercase();
        let filtered_projects = state
            .projects
            .iter()
            .filter(|project| {
                project_filter.is_empty()
                    || project
                        .display_path
                        .to_lowercase()
                        .contains(&project_filter)
                    || project
                        .project_name
                        .to_lowercase()
                        .contains(&project_filter)
                    || project.space_name.to_lowercase().contains(&project_filter)
                    || project.team_name.to_lowercase().contains(&project_filter)
                    || project.project_id.to_string().contains(&project_filter)
            })
            .collect::<Vec<_>>();
        let project_rows = if filtered_projects.is_empty() {
            column![text("没有匹配的项目。").size(12)].spacing(6)
        } else {
            filtered_projects
                .iter()
                .take(12)
                .fold(column![].spacing(6), |column, project| {
                    column.push(
                        container(
                            column![
                                row![
                                    text(&project.display_path).size(13).width(Fill),
                                    text(format!("ID：{}", project.project_id)).size(11),
                                ]
                                .spacing(8)
                                .align_y(Center),
                                row![
                                    text(format!(
                                        "team={} · space={} · project={}",
                                        project.team_id, project.space_id, project.project_id
                                    ))
                                    .size(11),
                                    space::horizontal(),
                                    button("绑定到当前范围").style(button::secondary).on_press(
                                        Message::BindVmmWorkbenchProject(project.project_id),
                                    ),
                                ]
                                .spacing(8)
                                .align_y(Center),
                            ]
                            .spacing(4),
                        )
                        .padding(8)
                        .style(container::rounded_box),
                    )
                })
        };
        let project_label = self
            .vmm_management_workbench
            .selected_project_id
            .as_deref()
            .and_then(|project_id| self.task_project_by_id(project_id))
            .map(|project| project.title.as_str())
            .unwrap_or("未选择项目");
        container(
            column![
                row![
                    column![
                        text(format!(
                            "项目 {} / {}",
                            filtered_projects.len(),
                            state.projects.len()
                        ))
                        .size(13),
                        text(format!("当前范围：{}", state.scope.label())).size(11),
                    ]
                    .spacing(3)
                    .width(Fill),
                    text_input("筛选项目", &state.project_filter)
                        .on_input(Message::VmmWorkbenchProjectFilterChanged)
                        .padding(8)
                        .width(Length::Fixed(220.0)),
                ]
                .spacing(10)
                .align_y(Center),
                row![
                    container(
                        column![
                            text("绑定摘要").size(12),
                            text(format!(
                                "全局项目：{}",
                                self.vmm_binding_value_label(&self.vmm_settings_draft.project_id)
                            ))
                            .size(11),
                            text(format!(
                                "{} 项目：{}",
                                project_label,
                                self.vmm_binding_value_label(
                                    &self.project_vmm_settings_draft.settings.project_id
                                )
                            ))
                            .size(11),
                        ]
                        .spacing(4),
                    )
                    .padding(8)
                    .style(container::rounded_box)
                    .width(Length::Fixed(220.0)),
                    scrollable(project_rows)
                        .height(Length::Fixed(260.0))
                        .width(Fill),
                ]
                .spacing(10),
            ]
            .spacing(8),
        )
        .padding(10)
        .style(container::rounded_box)
        .width(Fill)
        .into()
    }

    /// Formats one optional binding value for compact summaries.
    /// 将一个可选绑定值格式化为紧凑摘要文本。
    fn vmm_binding_value_label(&self, value: &str) -> String {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            String::from("继承/未设置")
        } else {
            trimmed.to_string()
        }
    }

    /// Renders the VMM profile-node workspace.
    /// 渲染 VMM 画像节点工作区。
    fn view_vmm_profile_nodes_panel(
        &self,
        target_choices: &'static [VmmProfileTargetChoice],
    ) -> Element<'_, Message> {
        let state = &self.vmm_management_workbench;
        let target = state.profile_target;
        let profile_filter = state.profile_filter.trim().to_lowercase();
        let filtered_nodes = state
            .profile_nodes
            .iter()
            .filter(|node| {
                profile_filter.is_empty()
                    || node.content.to_lowercase().contains(&profile_filter)
                    || node.level.to_lowercase().contains(&profile_filter)
                    || node.priority.to_lowercase().contains(&profile_filter)
                    || node.level_reason.to_lowercase().contains(&profile_filter)
                    || node.profile_node_id.to_string().contains(&profile_filter)
                    || node.bind_id.to_string().contains(&profile_filter)
            })
            .collect::<Vec<_>>();
        let nodes = if filtered_nodes.is_empty() {
            column![text("没有匹配的画像节点。").size(12)].spacing(6)
        } else {
            filtered_nodes
                .iter()
                .take(12)
                .fold(column![].spacing(6), |column, node| {
                    column.push(
                        container(
                            column![
                                row![
                                    text(format!("#{}", node.profile_node_id)).size(11),
                                    text(format!(
                                        "{} · {} · bind={}",
                                        node.priority, node.level, node.bind_id
                                    ))
                                    .size(11),
                                ]
                                .spacing(8),
                                text(&node.content).size(12),
                                text(format!(
                                    "refresh={} · date={} · source={:?}:{}",
                                    node.refresh_weight,
                                    node.profile_date,
                                    node.source_kind,
                                    node.source_id
                                ))
                                .size(11),
                                text(&node.level_reason).size(11),
                            ]
                            .spacing(4),
                        )
                        .padding(8)
                        .style(container::rounded_box),
                    )
                })
        };
        let instruction_action = VmmWorkbenchAction::ApplyProfileInstruction {
            target: target.into_protocol(),
            instruction: state.profile_instruction.clone(),
        };
        container(
            column![
                row![
                    column![
                        text("画像中心").size(13),
                        text(format!(
                            "节点 {} / {}",
                            filtered_nodes.len(),
                            state.profile_nodes.len()
                        ))
                        .size(11),
                    ]
                    .spacing(3)
                    .width(Fill),
                    pick_list(
                        target_choices,
                        Some(target),
                        Message::VmmWorkbenchProfileTargetChanged,
                    )
                    .width(Length::Fixed(130.0)),
                    button("刷新").style(button::secondary).on_press(
                        Message::RunVmmWorkbenchAction(VmmWorkbenchAction::LoadProfileNodes {
                            target: target.into_protocol(),
                        },)
                    ),
                ]
                .spacing(8)
                .align_y(Center),
                text_input("筛选画像节点", &state.profile_filter)
                    .on_input(Message::VmmWorkbenchProfileFilterChanged)
                    .padding(8)
                    .width(Fill),
                row![
                    container(
                        column![
                            text("操作").size(12),
                            text(
                                "手工画像指令会提交给当前目标，由 VMM 返回 accepted/retired 节点。"
                            )
                            .size(11),
                            text_input("输入一条手工画像指令", &state.profile_instruction)
                                .on_input(Message::VmmWorkbenchProfileInstructionChanged)
                                .padding(8),
                            button("应用手工画像指令")
                                .style(button::secondary)
                                .on_press(Message::RunVmmWorkbenchAction(instruction_action)),
                        ]
                        .spacing(8),
                    )
                    .padding(8)
                    .style(container::rounded_box)
                    .width(Length::Fixed(260.0)),
                    scrollable(nodes).height(Length::Fixed(300.0)).width(Fill),
                ]
                .spacing(10),
            ]
            .spacing(8),
        )
        .padding(10)
        .style(container::rounded_box)
        .width(FillPortion(1))
        .into()
    }

    /// Renders the latest full profile bundle.
    /// 渲染最新完整画像 Bundle。
    fn view_vmm_profile_bundle_panel(&self) -> Element<'_, Message> {
        let bundle_text = self
            .vmm_management_workbench
            .profile_bundle
            .as_ref()
            .map(|bundle| {
                [
                    ("环境优先级", bundle.environment_priority_text.as_str()),
                    ("完整画像", bundle.combined_text.as_str()),
                    ("团队画像", bundle.team_profile.as_str()),
                    ("空间画像", bundle.space_profile.as_str()),
                    ("项目画像", bundle.project_profile.as_str()),
                    ("用户画像", bundle.user_profile.as_str()),
                ]
                .into_iter()
                .filter(|(_, value)| !value.trim().is_empty())
                .map(|(label, value)| format!("## {label}\n{value}"))
                .collect::<Vec<_>>()
                .join("\n\n")
            })
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| String::from("尚未读取完整画像 Bundle。"));
        container(
            column![
                row![
                    column![
                        text("完整画像 Bundle").size(13),
                        text("读取当前 user/project 的 FULL bundle，用于检查画像组合结果。")
                            .size(11),
                    ]
                    .spacing(3)
                    .width(Fill),
                    button("刷新 Bundle").style(button::secondary).on_press(
                        Message::RunVmmWorkbenchAction(VmmWorkbenchAction::LoadProfileBundle,)
                    ),
                ]
                .spacing(10)
                .align_y(Center),
                row![
                    container(
                        column![
                            text("操作").size(12),
                            text("对应 OpenCode Profile Bundle Test 的单一 refresh action。")
                                .size(11),
                            button("读取完整画像").style(button::secondary).on_press(
                                Message::RunVmmWorkbenchAction(
                                    VmmWorkbenchAction::LoadProfileBundle,
                                )
                            ),
                        ]
                        .spacing(8),
                    )
                    .padding(8)
                    .style(container::rounded_box)
                    .width(Length::Fixed(260.0)),
                    container(text(bundle_text).size(12))
                        .padding(10)
                        .style(container::rounded_box)
                        .width(Fill),
                ]
                .spacing(10),
            ]
            .spacing(8),
        )
        .padding(10)
        .style(container::rounded_box)
        .into()
    }

    /// Renders the OpenCode-style tools-debug panel.
    /// 渲染 OpenCode 风格的工具调试面板。
    fn view_vmm_tools_debug_panel(&self) -> Element<'_, Message> {
        let state = &self.vmm_management_workbench;
        column![
            text("工具调试").size(15),
            text("模拟 VMM AI 专属工具：检索记忆、展开 turn、主动写入记忆。").size(12),
            row![
                container(
                    column![
                        text("动作").size(13),
                        button("SearchMemoryEvents")
                            .style(button::secondary)
                            .width(Fill)
                            .on_press(Message::RunVmmWorkbenchSearchMemoryEvents),
                        button("GetTurnDetails")
                            .style(button::secondary)
                            .width(Fill)
                            .on_press(Message::RunVmmWorkbenchGetTurnDetails),
                        button("WriteMemories")
                            .style(button::secondary)
                            .width(Fill)
                            .on_press(Message::RunVmmWorkbenchWriteMemories),
                    ]
                    .spacing(8),
                )
                .padding(8)
                .style(container::rounded_box)
                .width(Length::Fixed(220.0)),
                container(
                    column![
                        text("请求参数").size(13),
                        row![
                            self.view_vmm_text_field(
                                "检索查询",
                                "多条用 | 分隔",
                                &state.tool_search_queries,
                                Message::VmmWorkbenchToolSearchQueriesChanged,
                            ),
                            self.view_vmm_text_field(
                                "Top K",
                                "5",
                                &state.tool_search_top_k,
                                Message::VmmWorkbenchToolSearchTopKChanged,
                            ),
                        ]
                        .spacing(10),
                        row![
                            self.view_vmm_text_field(
                                "Turn IDs",
                                "用逗号或 | 分隔",
                                &state.tool_turn_ids,
                                Message::VmmWorkbenchToolTurnIdsChanged,
                            ),
                            self.view_vmm_text_field(
                                "摘要",
                                "记忆摘要",
                                &state.tool_write_abstract,
                                Message::VmmWorkbenchToolWriteAbstractChanged,
                            ),
                            self.view_vmm_text_field(
                                "详情",
                                "完整内容",
                                &state.tool_write_details,
                                Message::VmmWorkbenchToolWriteDetailsChanged,
                            ),
                        ]
                        .spacing(10),
                        row![
                            self.view_vmm_text_field(
                                "Scope",
                                "2",
                                &state.tool_write_scope_level,
                                Message::VmmWorkbenchToolWriteScopeLevelChanged,
                            ),
                            self.view_vmm_text_field(
                                "Category",
                                "1",
                                &state.tool_write_category,
                                Message::VmmWorkbenchToolWriteCategoryChanged,
                            ),
                            self.view_vmm_text_field(
                                "Priority",
                                "5",
                                &state.tool_write_priority,
                                Message::VmmWorkbenchToolWritePriorityChanged,
                            ),
                            self.view_vmm_text_field(
                                "Level",
                                "2",
                                &state.tool_write_memory_level,
                                Message::VmmWorkbenchToolWriteMemoryLevelChanged,
                            ),
                        ]
                        .spacing(10),
                    ]
                    .spacing(8),
                )
                .padding(8)
                .style(container::rounded_box)
                .width(Fill),
            ]
            .spacing(10),
            row![
                container(
                    column![
                        text("SearchMemoryEvents 结果").size(12),
                        self.view_vmm_memory_search_result(),
                    ]
                    .spacing(6),
                )
                .padding(8)
                .style(container::rounded_box)
                .width(Fill),
                container(
                    column![
                        text("GetTurnDetails 结果").size(12),
                        self.view_vmm_turn_details_result(),
                    ]
                    .spacing(6),
                )
                .padding(8)
                .style(container::rounded_box)
                .width(Fill),
            ]
            .spacing(10),
            container(
                column![
                    text("WriteMemories 结果").size(12),
                    self.view_vmm_write_memories_result(),
                ]
                .spacing(6),
            )
            .padding(8)
            .style(container::rounded_box)
            .width(Fill),
            self.view_vmm_tools_debug_payloads(),
        ]
        .spacing(10)
        .into()
    }

    /// Renders the latest tools-debug request and response JSON panes.
    /// 渲染最近一次工具调试请求与响应 JSON 面板。
    fn view_vmm_tools_debug_payloads(&self) -> Element<'_, Message> {
        let request_text = if self
            .vmm_management_workbench
            .tool_request_preview
            .trim()
            .is_empty()
        {
            "尚未生成请求。"
        } else {
            self.vmm_management_workbench.tool_request_preview.as_str()
        };
        let response_text = if self
            .vmm_management_workbench
            .tool_response_preview
            .trim()
            .is_empty()
        {
            "尚未收到响应。"
        } else {
            self.vmm_management_workbench.tool_response_preview.as_str()
        };
        row![
            container(column![text("请求").size(12), text(request_text).size(11)].spacing(6))
                .padding(8)
                .style(container::rounded_box)
                .width(Fill),
            container(column![text("响应").size(12), text(response_text).size(11)].spacing(6))
                .padding(8)
                .style(container::rounded_box)
                .width(Fill),
        ]
        .spacing(10)
        .into()
    }

    /// Renders the latest memory-search result preview.
    /// 渲染最近一次记忆检索结果预览。
    fn view_vmm_memory_search_result(&self) -> Element<'_, Message> {
        let text_value = self
            .vmm_management_workbench
            .memory_search
            .as_ref()
            .map(|response| {
                response
                    .results
                    .iter()
                    .flat_map(|group| {
                        group.hits.iter().map(move |hit| {
                            format!(
                                "{} · #{} · turn={} · {}",
                                group.query, hit.memory_id, hit.source_turn_id, hit.abstract_text
                            )
                        })
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| String::from("尚未执行 SearchMemoryEvents。"));
        container(text(text_value).size(12))
            .padding(8)
            .style(container::rounded_box)
            .width(Fill)
            .into()
    }

    /// Renders the latest turn-detail result preview.
    /// 渲染最近一次 turn 详情结果预览。
    fn view_vmm_turn_details_result(&self) -> Element<'_, Message> {
        let text_value = self
            .vmm_management_workbench
            .turn_details
            .as_ref()
            .map(|response| {
                response
                    .turns
                    .iter()
                    .map(|turn| {
                        format!(
                            "turn={} · user={} · timeline={} · assistant={}",
                            turn.turn_id,
                            turn.user_question,
                            turn.timeline.len(),
                            turn.assistant_answer
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| String::from("尚未执行 GetTurnDetails。"));
        container(text(text_value).size(12))
            .padding(8)
            .style(container::rounded_box)
            .width(Fill)
            .into()
    }

    /// Renders the latest direct-write result preview.
    /// 渲染最近一次主动写入结果预览。
    fn view_vmm_write_memories_result(&self) -> Element<'_, Message> {
        let text_value = self
            .vmm_management_workbench
            .write_memories
            .as_ref()
            .map(|response| {
                response
                    .items
                    .iter()
                    .map(|item| format!("memory_id={} · deduped={}", item.memory_id, item.deduped))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| String::from("尚未执行 WriteMemories。"));
        container(text(text_value).size(12))
            .padding(8)
            .style(container::rounded_box)
            .width(Fill)
            .into()
    }

    /// Renders VMM memory settings for the current management scope.
    /// 渲染当前管理作用域的 VMM 记忆设置。
    fn view_vmm_memory_settings_panel(&self) -> Element<'_, Message> {
        let draft = match self.vmm_management_workbench.scope {
            VmmManagementScopeChoice::Global => &self.vmm_settings_draft,
            VmmManagementScopeChoice::Project => &self.project_vmm_settings_draft.settings,
        };
        let save_button: Element<'_, Message> = match self.vmm_management_workbench.scope {
            VmmManagementScopeChoice::Global => button("保存到全局")
                .on_press(Message::SaveGlobalVmmConfiguration)
                .into(),
            VmmManagementScopeChoice::Project => button("保存到选中项目")
                .on_press(Message::SaveVmmWorkbenchProjectConfiguration)
                .into(),
        };
        column![
            text("记忆设置").size(15),
            text("配置记忆开关、AI 工具、临时 TTL、画像刷新、压缩召回和注入说明。").size(12),
            self.view_vmm_settings_fields(draft, self.vmm_settings_message_map_for_scope(), false),
            save_button,
        ]
        .spacing(10)
        .into()
    }

    /// Renders global gRPC transport settings.
    /// 渲染全局 gRPC 传输设置。
    fn view_vmm_grpc_transport_panel(&self) -> Element<'_, Message> {
        column![
            text("gRPC 传输").size(15),
            text("连接地址、API key、超时、最大接收字节数与 keepalive 策略只允许在全局配置。")
                .size(12),
            self.view_vmm_settings_fields(
                &self.vmm_settings_draft,
                self.global_vmm_settings_message_map(),
                true
            ),
            row![
                button("重载")
                    .style(button::secondary)
                    .on_press(Message::ReloadGlobalVmmConfiguration),
                button("保存").on_press(Message::SaveGlobalVmmConfiguration),
            ]
            .spacing(8),
        ]
        .spacing(10)
        .into()
    }

    /// Renders the global VMM configuration card inside the AI extension settings surface.
    /// 在 AI 扩展设置界面中渲染全局 VMM 配置卡片。
    pub(super) fn view_global_vmm_settings_card(&self) -> Element<'_, Message> {
        container(
            column![
                row![
                    column![
                        text("VulcanMemoryMesh").size(16),
                        text("配置 gRPC 地址、业务标识与注入策略；项目与会话可继续覆盖。").size(12),
                    ]
                    .spacing(3)
                    .width(Fill),
                    button("重载")
                        .style(button::secondary)
                        .on_press(Message::ReloadGlobalVmmConfiguration),
                    button("保存").on_press(Message::SaveGlobalVmmConfiguration),
                ]
                .spacing(10)
                .align_y(Center),
                self.view_vmm_settings_fields(
                    &self.vmm_settings_draft,
                    self.global_vmm_settings_message_map(),
                    true,
                ),
                self.view_vmm_management_buttons(Message::RunGlobalVmmManagement),
            ]
            .spacing(12),
        )
        .padding(12)
        .style(container::bordered_box)
        .into()
    }

    /// Renders the project VMM override card inside the project settings modal.
    /// 在项目设置弹窗中渲染项目级 VMM 覆盖卡片。
    pub(super) fn view_project_vmm_settings_card(&self) -> Element<'_, Message> {
        let draft = &self.project_vmm_settings_draft;
        let fields = if draft.project_override_enabled {
            self.view_vmm_settings_fields(
                &draft.settings,
                self.project_vmm_settings_message_map(),
                false,
            )
        } else {
            container(text("当前继承全局 VMM 配置。开启项目覆盖后可为此项目设置独立 user/project id、记忆策略或关闭记忆；gRPC 连接仍只使用全局配置。").size(12))
                .padding(10)
                .style(container::rounded_box)
                .into()
        };
        container(
            column![
                row![
                    column![
                        text("项目级 VMM 记忆").size(14),
                        text(
                            "未开启覆盖时继承全局配置；开启后当前项目优先，但不允许修改 gRPC 连接。"
                        )
                        .size(12),
                    ]
                    .spacing(3)
                    .width(Fill),
                    checkbox(draft.project_override_enabled)
                        .on_toggle(Message::ProjectVmmOverrideToggled),
                    button("重载")
                        .style(button::secondary)
                        .on_press(Message::ReloadProjectVmmConfiguration),
                    button("保存").on_press(Message::SaveProjectVmmConfiguration),
                ]
                .spacing(10)
                .align_y(Center),
                fields,
                self.view_vmm_management_buttons(Message::RunProjectVmmManagement),
            ]
            .spacing(12),
        )
        .padding(12)
        .style(container::bordered_box)
        .into()
    }

    /// Renders the common VMM field grid.
    /// 渲染通用 VMM 字段网格。
    fn view_vmm_settings_fields<'a>(
        &'a self,
        draft: &'a VmmSettingsDraft,
        messages: VmmSettingsMessageMap,
        include_transport: bool,
    ) -> Element<'a, Message> {
        let mut fields = column![
            row![
                checkbox(draft.enabled).on_toggle(messages.enabled),
                text("启用记忆连接").size(13),
                space::horizontal(),
                checkbox(draft.ai_tools_enabled).on_toggle(messages.ai_tools),
                text("启用 AI 专属工具").size(13),
                checkbox(draft.precheck_enabled).on_toggle(messages.precheck),
                text("启用前置预注入").size(13),
            ]
            .spacing(8)
            .align_y(Center),
        ]
        .spacing(10);
        if include_transport {
            fields = fields.push(
                row![
                    self.view_vmm_text_field(
                        "gRPC 地址",
                        "127.0.0.1:50051",
                        &draft.grpc_target,
                        messages.grpc_target,
                    ),
                    self.view_vmm_text_field(
                        "API Key",
                        "可留空",
                        &draft.grpc_api_key,
                        messages.api_key
                    ),
                ]
                .spacing(10),
            );
            fields = fields.push(
                row![
                    self.view_vmm_text_field(
                        "握手超时 ms",
                        "15000",
                        &draft.handshake_timeout_ms,
                        messages.handshake_timeout,
                    ),
                    self.view_vmm_text_field(
                        "请求超时 ms",
                        "15000",
                        &draft.receive_timeout_ms,
                        messages.receive_timeout,
                    ),
                    self.view_vmm_text_field(
                        "最大接收字节",
                        "可留空",
                        &draft.max_receive_message_bytes,
                        messages.max_receive_message_bytes,
                    ),
                ]
                .spacing(10),
            );
            fields = fields.push(
                row![
                    self.view_vmm_text_field(
                        "Keepalive 间隔 ms",
                        "300000",
                        &draft.grpc_keepalive_time_ms,
                        messages.grpc_keepalive_time,
                    ),
                    self.view_vmm_text_field(
                        "Keepalive 超时 ms",
                        "20000",
                        &draft.grpc_keepalive_timeout_ms,
                        messages.grpc_keepalive_timeout,
                    ),
                    column![
                        text("空闲 Keepalive").size(12),
                        row![
                            checkbox(draft.grpc_keepalive_permit_without_calls)
                                .on_toggle(messages.grpc_keepalive_permit_without_calls),
                            text("无调用时也发送").size(12),
                        ]
                        .spacing(6)
                        .align_y(Center),
                    ]
                    .spacing(5)
                    .width(Fill),
                ]
                .spacing(10),
            );
        } else {
            fields = fields.push(
                container(
                    text("项目级配置继承全局 gRPC 连接；这里只保存用户、项目和记忆策略覆盖。")
                        .size(12),
                )
                .padding(8)
                .style(container::rounded_box),
            );
        }
        fields = fields.push(
            row![
                self.view_vmm_text_field("用户 ID", "数字", &draft.user_id, messages.user_id),
                self.view_vmm_text_field("项目 ID", "数字", &draft.project_id, messages.project_id),
            ]
            .spacing(10),
        );
        fields = fields.push(
            row![
                self.view_vmm_text_field(
                    "管理用户引用",
                    "ID 或用户名",
                    &draft.management_user_ref,
                    messages.management_user_ref,
                ),
                self.view_vmm_text_field(
                    "管理项目引用",
                    "ID 或 Team/Space/Project",
                    &draft.management_project_ref,
                    messages.management_project_ref,
                ),
            ]
            .spacing(10),
        );
        fields = fields.push(
            row![
                self.view_vmm_text_field(
                    "临时记忆 TTL",
                    "3",
                    &draft.implicit_memory_turns,
                    messages.implicit_turns,
                ),
                self.view_vmm_text_field(
                    "画像刷新间隔",
                    "10",
                    &draft.profile_refresh_turns,
                    messages.profile_refresh_turns,
                ),
            ]
            .spacing(10),
        );
        fields = fields.push(
            container(
                text("记忆注入固定为隐藏模式；压缩感知自动开启，不再需要单独配置。").size(12),
            )
            .padding(8)
            .style(container::rounded_box),
        );
        fields = fields.push(self.view_vmm_text_field(
            "注入标签",
            "vmm_inject",
            &draft.injection_tag,
            messages.injection_tag,
        ));
        fields = fields.push(
            column![
                text("注入说明").size(12),
                text_input("用于包裹召回内容的说明文本", &draft.injection_description)
                    .on_input(messages.injection_description)
                    .padding(8)
                    .width(Fill),
            ]
            .spacing(5),
        );
        fields = fields.push(text(&draft.notice).size(12));
        fields.into()
    }

    /// Renders one labeled VMM text input field.
    /// 渲染一个带标签的 VMM 文本输入字段。
    fn view_vmm_text_field<'a>(
        &'a self,
        label: &'a str,
        placeholder: &'a str,
        value: &'a str,
        message: fn(String) -> Message,
    ) -> Element<'a, Message> {
        column![
            text(label).size(12),
            text_input(placeholder, value)
                .on_input(message)
                .padding(8)
                .width(Fill),
        ]
        .spacing(5)
        .width(Fill)
        .into()
    }

    /// Returns message constructors for the global VMM settings draft.
    /// 返回全局 VMM 设置草稿使用的消息构造器。
    fn global_vmm_settings_message_map(&self) -> VmmSettingsMessageMap {
        VmmSettingsMessageMap {
            enabled: Message::GlobalVmmEnabledChanged,
            grpc_target: Message::GlobalVmmGrpcTargetChanged,
            api_key: Message::GlobalVmmApiKeyChanged,
            user_id: Message::GlobalVmmUserIdChanged,
            project_id: Message::GlobalVmmProjectIdChanged,
            receive_timeout: Message::GlobalVmmReceiveTimeoutChanged,
            handshake_timeout: Message::GlobalVmmHandshakeTimeoutChanged,
            max_receive_message_bytes: Message::GlobalVmmMaxReceiveMessageBytesChanged,
            grpc_keepalive_time: Message::GlobalVmmKeepaliveTimeChanged,
            grpc_keepalive_timeout: Message::GlobalVmmKeepaliveTimeoutChanged,
            grpc_keepalive_permit_without_calls:
                Message::GlobalVmmKeepalivePermitWithoutCallsChanged,
            management_user_ref: Message::GlobalVmmManagementUserRefChanged,
            management_project_ref: Message::GlobalVmmManagementProjectRefChanged,
            implicit_turns: Message::GlobalVmmImplicitTurnsChanged,
            profile_refresh_turns: Message::GlobalVmmProfileRefreshTurnsChanged,
            ai_tools: Message::GlobalVmmAiToolsChanged,
            precheck: Message::GlobalVmmPrecheckChanged,
            injection_tag: Message::GlobalVmmInjectionTagChanged,
            injection_description: Message::GlobalVmmInjectionDescriptionChanged,
        }
    }

    /// Returns message constructors for the project VMM settings draft.
    /// 返回项目 VMM 设置草稿使用的消息构造器。
    fn project_vmm_settings_message_map(&self) -> VmmSettingsMessageMap {
        VmmSettingsMessageMap {
            enabled: Message::ProjectVmmEnabledChanged,
            grpc_target: Message::ProjectVmmGrpcTargetChanged,
            api_key: Message::ProjectVmmApiKeyChanged,
            user_id: Message::ProjectVmmUserIdChanged,
            project_id: Message::ProjectVmmProjectIdChanged,
            receive_timeout: Message::ProjectVmmReceiveTimeoutChanged,
            handshake_timeout: Message::ProjectVmmHandshakeTimeoutChanged,
            max_receive_message_bytes: Message::ProjectVmmMaxReceiveMessageBytesChanged,
            grpc_keepalive_time: Message::ProjectVmmKeepaliveTimeChanged,
            grpc_keepalive_timeout: Message::ProjectVmmKeepaliveTimeoutChanged,
            grpc_keepalive_permit_without_calls:
                Message::ProjectVmmKeepalivePermitWithoutCallsChanged,
            management_user_ref: Message::ProjectVmmManagementUserRefChanged,
            management_project_ref: Message::ProjectVmmManagementProjectRefChanged,
            implicit_turns: Message::ProjectVmmImplicitTurnsChanged,
            profile_refresh_turns: Message::ProjectVmmProfileRefreshTurnsChanged,
            ai_tools: Message::ProjectVmmAiToolsChanged,
            precheck: Message::ProjectVmmPrecheckChanged,
            injection_tag: Message::ProjectVmmInjectionTagChanged,
            injection_description: Message::ProjectVmmInjectionDescriptionChanged,
        }
    }

    /// Returns message constructors for the currently selected VMM management scope.
    /// 返回当前 VMM 管理作用域使用的消息构造器。
    fn vmm_settings_message_map_for_scope(&self) -> VmmSettingsMessageMap {
        match self.vmm_management_workbench.scope {
            VmmManagementScopeChoice::Global => self.global_vmm_settings_message_map(),
            VmmManagementScopeChoice::Project => self.project_vmm_settings_message_map(),
        }
    }

    /// Renders VMM management action buttons for one settings surface.
    /// 为一个设置界面渲染 VMM 管理动作按钮。
    fn view_vmm_management_buttons(
        &self,
        message: fn(VmmManagementAction) -> Message,
    ) -> Element<'_, Message> {
        row![
            button(VmmManagementAction::Healthz.label())
                .style(button::secondary)
                .on_press(message(VmmManagementAction::Healthz)),
            button(VmmManagementAction::ListUsersAndProjects.label())
                .style(button::secondary)
                .on_press(message(VmmManagementAction::ListUsersAndProjects)),
            button(VmmManagementAction::ResolveUserAndProject.label())
                .style(button::secondary)
                .on_press(message(VmmManagementAction::ResolveUserAndProject)),
            button(VmmManagementAction::EnsureProjectPreview.label())
                .style(button::secondary)
                .on_press(message(VmmManagementAction::EnsureProjectPreview)),
            button(VmmManagementAction::ProfileBundlePreview.label())
                .style(button::secondary)
                .on_press(message(VmmManagementAction::ProfileBundlePreview)),
        ]
        .spacing(8)
        .wrap()
        .into()
    }
}

/// Message constructors used by the reusable VMM field renderer.
/// 可复用 VMM 字段渲染器使用的消息构造器集合。
#[derive(Debug, Clone, Copy)]
struct VmmSettingsMessageMap {
    /// Enabled-toggle message constructor.
    /// 启用开关消息构造器。
    enabled: fn(bool) -> Message,
    /// gRPC target message constructor.
    /// gRPC 目标消息构造器。
    grpc_target: fn(String) -> Message,
    /// API-key message constructor.
    /// API key 消息构造器。
    api_key: fn(String) -> Message,
    /// User-id message constructor.
    /// 用户标识消息构造器。
    user_id: fn(String) -> Message,
    /// Project-id message constructor.
    /// 项目标识消息构造器。
    project_id: fn(String) -> Message,
    /// Request-timeout message constructor.
    /// 请求超时时间消息构造器。
    receive_timeout: fn(String) -> Message,
    /// Handshake-timeout message constructor.
    /// 握手超时时间消息构造器。
    handshake_timeout: fn(String) -> Message,
    /// Max receive-message-bytes message constructor.
    /// 最大接收消息字节数消息构造器。
    max_receive_message_bytes: fn(String) -> Message,
    /// gRPC keepalive interval message constructor.
    /// gRPC keepalive 间隔消息构造器。
    grpc_keepalive_time: fn(String) -> Message,
    /// gRPC keepalive timeout message constructor.
    /// gRPC keepalive 超时消息构造器。
    grpc_keepalive_timeout: fn(String) -> Message,
    /// gRPC idle keepalive toggle message constructor.
    /// gRPC 空闲 keepalive 开关消息构造器。
    grpc_keepalive_permit_without_calls: fn(bool) -> Message,
    /// Management user-reference message constructor.
    /// 管理用户引用消息构造器。
    management_user_ref: fn(String) -> Message,
    /// Management project-reference message constructor.
    /// 管理项目引用消息构造器。
    management_project_ref: fn(String) -> Message,
    /// Transient TTL message constructor.
    /// 临时 TTL 消息构造器。
    implicit_turns: fn(String) -> Message,
    /// Profile refresh interval message constructor.
    /// 画像刷新间隔消息构造器。
    profile_refresh_turns: fn(String) -> Message,
    /// AI-tool exposure toggle message constructor.
    /// AI 工具暴露开关消息构造器。
    ai_tools: fn(bool) -> Message,
    /// Automatic precheck recall toggle message constructor.
    /// 自动前置召回开关消息构造器。
    precheck: fn(bool) -> Message,
    /// Injection-tag message constructor.
    /// 注入标签消息构造器。
    injection_tag: fn(String) -> Message,
    /// Injection-description message constructor.
    /// 注入说明消息构造器。
    injection_description: fn(String) -> Message,
}
