//! LuaSkills management panel view composition.
//! LuaSkills 管理面板视图组合逻辑。

use super::*;
use crate::provider_config::PersistedLuaSkillToolRegistrationState;
use iced::widget::column;

const LUASKILL_TOOL_REGISTRATION_OPTIONS: [PersistedLuaSkillToolRegistrationState; 3] = [
    PersistedLuaSkillToolRegistrationState::Disabled,
    PersistedLuaSkillToolRegistrationState::Explicit,
    PersistedLuaSkillToolRegistrationState::Implicit,
];

impl VulcanWorkbench {
    /// Builds the LuaSkills management panel backed by the embedded host runtime and its live skill catalog.
    /// 构建由嵌入式宿主运行时及其实时技能目录驱动的 LuaSkills 管理面板。
    pub(super) fn view_luaskills_panel(&self) -> Element<'_, Message> {
        let panel_header = container(
            column![
                text(MainDisplayPanel::LuaSkills.localized_section_title(self.current_locale()))
                    .size(22),
                text(MainDisplayPanel::LuaSkills.localized_description(self.current_locale()))
                    .size(13),
            ]
            .spacing(4),
        )
        .padding([4, 2]);

        let summary_lines = if let Some(catalog) = &self.luaskills_catalog {
            let enabled_skill_count = catalog.skills.iter().filter(|skill| skill.enabled).count();
            let managed_skill_count = catalog.skills.iter().filter(|skill| skill.managed).count();
            let (explicit_tool_count, implicit_tool_count, disabled_tool_count) = catalog
                .skills
                .iter()
                .flat_map(|skill| {
                    skill.tools.iter().map(|tool| {
                        self.provider_configuration
                            .luaskills
                            .tool_registration_state(&skill.skill_id, &tool.local_name)
                    })
                })
                .fold((0usize, 0usize, 0usize), |mut counts, state| {
                    match state {
                        PersistedLuaSkillToolRegistrationState::Explicit => counts.0 += 1,
                        PersistedLuaSkillToolRegistrationState::Implicit => counts.1 += 1,
                        PersistedLuaSkillToolRegistrationState::Disabled => counts.2 += 1,
                    }
                    counts
                });
            column![
                text(format!("运行时根：{}", catalog.runtime_root.display())).size(12),
                text(format!(
                    "system_lua_lib：{}",
                    catalog.system_lua_lib_dir.display()
                ))
                .size(12),
                text(format!(
                    "可见技能 {} · 已启用 {} · 受管安装 {} · 运行时入口 {} · 显式 {} · 隐式 {} · 禁用 {}",
                    catalog.skills.len(),
                    enabled_skill_count,
                    managed_skill_count,
                    catalog.entry_count,
                    explicit_tool_count,
                    implicit_tool_count,
                    disabled_tool_count
                ))
                .size(12),
            ]
            .spacing(4)
        } else {
            column![
                text("尚未载入 LuaSkills 技能目录。").size(12),
                text("当嵌入式 LuaSkills 运行时就绪后，这里会显示当前可见技能。").size(12),
            ]
            .spacing(4)
        };

        let mut refresh_button = button("刷新").padding([8, 14]).style(button::secondary);
        if !self.luaskills_operation_in_flight {
            refresh_button = refresh_button.on_press(Message::RefreshLuaSkillsCatalog);
        }

        let source_placeholder = match self.luaskills_install_source_choice {
            LuaSkillsInstallSourceChoice::Github => "例如：LuaSkills/vulcan-codekit",
            LuaSkillsInstallSourceChoice::Url => "例如：https://example.com/skill-source.yaml",
        };

        let mut install_button = button("直接安装").padding([8, 14]);
        if !self.luaskills_operation_in_flight {
            install_button = install_button.on_press(Message::InstallLuaSkill);
        }

        let progress_card: Option<Element<'_, Message>> =
            self.luaskills_operation_progress.as_ref().map(|progress| {
                let mut progress_body: iced::widget::Column<'_, Message> = column![
                    row![
                        text(progress.title_text()).size(14),
                        space::horizontal().width(Fill),
                        text(progress.status_line()).size(12),
                    ]
                    .align_y(Center)
                ]
                .spacing(8);
                if let Some(progress_value) = progress.progress_fraction() {
                    progress_body = progress_body.push(progress_bar(0.0..=1.0, progress_value));
                }
                if let Some(source_line) = progress.source_line() {
                    progress_body = progress_body.push(text(source_line).size(12));
                }
                if let Some(message) = progress
                    .message
                    .as_ref()
                    .map(|message| message.trim())
                    .filter(|message| !message.is_empty())
                {
                    progress_body = progress_body.push(text(message).size(12));
                }
                container(progress_body)
                    .padding(12)
                    .style(container::rounded_box)
                    .width(Fill)
                    .into()
            });

        let mut install_controls_body = column![
            row![
                text("技能目录").size(18),
                space::horizontal().width(Fill),
                refresh_button,
            ]
            .align_y(Center),
            summary_lines,
            text(&self.luaskills_notice).size(12),
        ]
        .spacing(10);
        if let Some(progress_card) = progress_card {
            install_controls_body = install_controls_body.push(progress_card);
        }
        install_controls_body = install_controls_body
            .push(
                row([
                    LuaSkillsInstallSourceChoice::Github,
                    LuaSkillsInstallSourceChoice::Url,
                ]
                .into_iter()
                .map(|choice| {
                    radio(
                        choice.label(),
                        choice,
                        Some(self.luaskills_install_source_choice),
                        Message::LuaSkillsInstallSourceChoiceSelected,
                    )
                    .into()
                }))
                .spacing(18)
                .align_y(Center),
            )
            .push(
                text_input(
                    "可选：显式技能标识符，例如 vulcan-codekit",
                    &self.luaskills_install_skill_id,
                )
                .on_input(Message::LuaSkillsInstallSkillIdChanged)
                .padding(10),
            )
            .push(
                text_input(source_placeholder, &self.luaskills_install_source)
                    .on_input(Message::LuaSkillsInstallSourceChanged)
                    .padding(10),
            )
            .push(
                row![
                    install_button,
                    text("说明：GitHub 与包地址都走 LuaSkills 0.4.6 的受管安装链，并会回传细粒度进度。").size(12),
                ]
                .spacing(10)
                .align_y(Center),
            );

        let install_controls = container(install_controls_body)
            .padding(16)
            .style(container::bordered_box);

        let skills_column = match &self.luaskills_catalog {
            Some(catalog) if catalog.skills.is_empty() => {
                column![text("当前没有可见 LuaSkills 技能。").size(13)].spacing(8)
            }
            Some(catalog) => catalog
                .skills
                .iter()
                .fold(column![].spacing(12), |column, skill| {
                    column.push(self.view_luaskills_skill_card(skill))
                }),
            None => column![
                text("LuaSkills 技能目录尚未可用。").size(13),
                text("请先等待启动引导完成，或点击上方“刷新”重新拉取目录。").size(12),
            ]
            .spacing(8),
        };

        let skill_list = container(
            column![
                text("可见技能列表").size(18),
                scrollable(skills_column).height(Fill),
            ]
            .spacing(10)
            .height(Fill),
        )
        .padding(16)
        .style(container::bordered_box)
        .height(Fill);

        column![panel_header, install_controls, skill_list]
            .spacing(12)
            .height(Fill)
            .into()
    }

    /// Builds one LuaSkills runtime tool row with host-managed registration controls while keeping package lifecycle state unchanged.
    /// 构建带宿主管理注册控件的单条 LuaSkills 运行时工具行，同时保持技能包生命周期状态不变。
    pub(super) fn view_luaskills_tool_row<'a>(
        &'a self,
        skill: &'a LuaSkillsCatalogSkill,
        tool: &'a crate::luaskills_runtime::LuaSkillsCatalogTool,
    ) -> Element<'a, Message> {
        let registration_state = self
            .provider_configuration
            .luaskills
            .tool_registration_state(&skill.skill_id, &tool.local_name);
        let state_help_text = luaskills_tool_registration_state_help(registration_state);
        let state_picker = pick_list(
            LUASKILL_TOOL_REGISTRATION_OPTIONS,
            Some(registration_state),
            {
                let skill_id = skill.skill_id.clone();
                let local_name = tool.local_name.clone();
                move |state| Message::SetLuaSkillToolRegistrationState {
                    skill_id: skill_id.clone(),
                    local_name: local_name.clone(),
                    state,
                }
            },
        )
        .placeholder("注册状态");

        let description = tool.description.trim();
        let mut body = column![
            row![
                text(&tool.local_name).size(14),
                space::horizontal().width(Fill),
                state_picker,
            ]
            .spacing(10)
            .align_y(Center),
            text(format!("规范名：{}", tool.canonical_name)).size(12),
            text(state_help_text).size(12),
        ]
        .spacing(4);
        if !description.is_empty() {
            body = body.push(text(description).size(12));
        }

        container(body)
            .padding([10, 12])
            .style(container::rounded_box)
            .width(Fill)
            .into()
    }

    /// Builds one LuaSkills skill card row used by the desktop management panel.
    /// 构建桌面管理面板使用的单个 LuaSkills 技能卡片行。
    pub(super) fn view_luaskills_skill_card<'a>(
        &'a self,
        skill: &'a LuaSkillsCatalogSkill,
    ) -> Element<'a, Message> {
        let source_text = if let Some(source_locator) = &skill.source_locator {
            let source_type = match skill.source_type {
                Some(::luaskills::SkillInstallSourceType::Github) => "GitHub",
                Some(::luaskills::SkillInstallSourceType::Url) => "包地址",
                Some(::luaskills::SkillInstallSourceType::OfficialHub) => "Official Hub",
                Some(::luaskills::SkillInstallSourceType::PrivateUrlManifest) => {
                    "Private URL Manifest"
                }
                None => "未知来源",
            };
            format!("{source_type} · {source_locator}")
        } else if skill.system_owned {
            String::from("ROOT 宿主内建技能")
        } else {
            String::from("未记录来源")
        };

        let status_text = if skill.enabled {
            "已启用"
        } else {
            "已停用"
        };
        let manage_text = if skill.managed { "受管" } else { "非受管" };
        let (explicit_tool_count, implicit_tool_count, disabled_tool_count) = skill
            .tools
            .iter()
            .map(|tool| {
                self.provider_configuration
                    .luaskills
                    .tool_registration_state(&skill.skill_id, &tool.local_name)
            })
            .fold((0usize, 0usize, 0usize), |mut counts, state| {
                match state {
                    PersistedLuaSkillToolRegistrationState::Explicit => counts.0 += 1,
                    PersistedLuaSkillToolRegistrationState::Implicit => counts.1 += 1,
                    PersistedLuaSkillToolRegistrationState::Disabled => counts.2 += 1,
                }
                counts
            });
        let active_progress = self
            .luaskills_operation_progress
            .as_ref()
            .filter(|progress| progress.matches_skill(&skill.skill_id));
        let progress_badge: Option<Element<'_, Message>> = active_progress.map(|progress| {
            container(text(progress.badge_text()).size(11))
                .padding([4, 8])
                .style(container::rounded_box)
                .into()
        });

        let mut action_row = row![].spacing(8).align_y(Center);
        if skill.enabled {
            let mut disable_button = button("停用").padding([6, 10]).style(button::secondary);
            if !self.luaskills_operation_in_flight {
                disable_button =
                    disable_button.on_press(Message::DisableLuaSkill(skill.skill_id.clone()));
            }
            action_row = action_row.push(disable_button);
        } else {
            let mut enable_button = button("启用").padding([6, 10]);
            if !self.luaskills_operation_in_flight {
                enable_button =
                    enable_button.on_press(Message::EnableLuaSkill(skill.skill_id.clone()));
            }
            action_row = action_row.push(enable_button);
        }

        if skill.managed {
            let mut update_button = button("更新").padding([6, 10]).style(button::secondary);
            if !self.luaskills_operation_in_flight {
                update_button =
                    update_button.on_press(Message::UpdateLuaSkill(skill.skill_id.clone()));
            }
            action_row = action_row.push(update_button);
        }

        if skill.system_owned {
            action_row = action_row.push(text("ROOT 系统技能，不允许卸载").size(12));
        } else {
            let mut uninstall_button = button("卸载").padding([6, 10]).style(button::secondary);
            if !self.luaskills_operation_in_flight {
                uninstall_button =
                    uninstall_button.on_press(Message::UninstallLuaSkill(skill.skill_id.clone()));
            }
            action_row = action_row.push(uninstall_button);
        }

        let mut detail_rows = column![
            text(format!(
                "{} · {} · 根 {} · 运行时入口 {} · 显式 {} · 隐式 {} · 禁用 {}",
                status_text,
                manage_text,
                skill.root_name,
                skill.entry_count,
                explicit_tool_count,
                implicit_tool_count,
                disabled_tool_count
            ))
            .size(12),
            text(format!("来源：{source_text}")).size(12),
        ]
        .spacing(4);
        if let Some(disabled_reason) = &skill.disabled_reason {
            detail_rows = detail_rows.push(text(format!("停用原因：{disabled_reason}")).size(12));
        }
        if let Some(progress) = active_progress {
            detail_rows =
                detail_rows.push(text(format!("当前进度：{}", progress.status_line())).size(12));
            if let Some(message) = progress
                .message
                .as_ref()
                .map(|message| message.trim())
                .filter(|message| !message.is_empty())
            {
                detail_rows = detail_rows.push(text(message).size(12));
            }
        }
        detail_rows =
            detail_rows.push(text(format!("路径：{}", skill.skill_dir.display())).size(12));
        let tool_section: Element<'_, Message> = if skill.tools.is_empty() {
            let empty_text = if skill.enabled {
                "当前技能尚未解析到可管理的运行时工具。"
            } else {
                "技能停用后不会加载运行时工具；重新启用后可在这里逐项管理。"
            };
            container(column![text("工具列表").size(13), text(empty_text).size(12),].spacing(6))
                .padding(12)
                .style(container::bordered_box)
                .width(Fill)
                .into()
        } else {
            let tool_rows = skill
                .tools
                .iter()
                .fold(column![].spacing(8), |column, tool| {
                    column.push(self.view_luaskills_tool_row(skill, tool))
                });
            container(
                column![
                    text("工具列表").size(13),
                    text("显式启用会注册到模型工具 Schema；隐式启用保留宿主调用能力但不暴露独立 Schema；禁用会关闭该工具调用。").size(12),
                    tool_rows,
                ]
                .spacing(8),
            )
            .padding(12)
            .style(container::bordered_box)
            .width(Fill)
            .into()
        };

        container(
            column![
                {
                    let mut title_row =
                        row![text(format!("{}  v{}", skill.display_name, skill.version)).size(16)]
                            .spacing(8)
                            .align_y(Center);
                    if let Some(progress_badge) = progress_badge {
                        title_row = title_row.push(progress_badge);
                    }
                    title_row
                        .push(space::horizontal().width(Fill))
                        .push(text(format!("ID: {}", skill.skill_id)).size(12))
                },
                detail_rows,
                action_row,
                tool_section,
            ]
            .spacing(10),
        )
        .padding(14)
        .style(container::rounded_box)
        .width(Fill)
        .into()
    }
}

/// Returns the short explanatory text for one LuaSkills tool registration state.
/// 返回单个 LuaSkills 工具注册状态的简短说明文本。
fn luaskills_tool_registration_state_help(
    state: PersistedLuaSkillToolRegistrationState,
) -> &'static str {
    match state {
        PersistedLuaSkillToolRegistrationState::Explicit => {
            "显式启用：工具会作为独立函数 Schema 暴露给模型，适合高频或参数复杂的核心工具。"
        }
        PersistedLuaSkillToolRegistrationState::Implicit => {
            "隐式启用：工具不进入独立 Schema，后续可通过 LuaSkills 调度工具按需调用。"
        }
        PersistedLuaSkillToolRegistrationState::Disabled => {
            "禁用：工具不会注册给模型，也不应被宿主调度调用。"
        }
    }
}
