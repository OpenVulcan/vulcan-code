//! User agent management panel view composition.
//! 用户 Agent 管理面板视图组合逻辑。

use super::*;
use iced::widget::column;

impl VulcanWorkbench {
    /// Builds the dedicated user-agent management panel backed by the dedicated user profile directory.
    /// 构建由独立用户 Agent 配置目录驱动的专用用户 Agent 管理面板。
    pub(super) fn view_agents_panel(&self) -> Element<'_, Message> {
        let panel_header = container(
            column![
                text(MainDisplayPanel::Agents.localized_section_title(self.current_locale()))
                    .size(22),
                text(MainDisplayPanel::Agents.localized_description(self.current_locale()))
                    .size(13),
            ]
            .spacing(4),
        )
        .padding([4, 2]);

        let system_default_summary = self
            .runtime_foundation
            .active_agent_definition()
            .filter(|agent| agent.id == WORKSPACE_COORDINATOR_AGENT_ID)
            .map(|agent| {
                format!(
                    "当前系统默认代理：{}（按模型自动路由基础提示词）",
                    agent.name
                )
            })
            .unwrap_or_else(|| String::from("当前系统默认代理会按模型自动路由基础提示词。"));

        let summary_card = container(
            column![
                text(&self.agent_profile_notice).size(12),
                text(format!(
                    "用户目录：{}",
                    agent_profile::agent_profiles_directory().display()
                ))
                .size(12),
                text(system_default_summary).size(12),
            ]
            .spacing(8),
        )
        .padding(16)
        .style(container::bordered_box);

        let profile_list_body = if self.agent_profile_catalog.agents.is_empty() {
            column![
                text("当前还没有用户自定义 Agent。").size(13),
                text("点击“新建 Agent”创建一份覆盖系统默认提示词的主代理。").size(12),
            ]
            .spacing(8)
        } else {
            self.agent_profile_catalog
                .agents
                .values()
                .fold(column![].spacing(8), |column, profile| {
                    column.push(self.view_user_agent_profile_card(profile))
                })
        };

        let profile_list = container(
            column![
                row![
                    column![
                        text("用户 Agent 列表").size(18),
                        text("系统默认代理不在此列表中，它会按模型自动路由。").size(12),
                    ]
                    .spacing(4),
                    space::horizontal(),
                    button("新建 Agent")
                        .padding([10, 14])
                        .style(button::secondary)
                        .on_press(Message::CreateUserAgentProfileDraft),
                ]
                .align_y(Center),
                scrollable(profile_list_body).height(Fill),
            ]
            .spacing(12)
            .height(Fill),
        )
        .padding(16)
        .style(container::bordered_box)
        .width(FillPortion(2))
        .height(Fill);

        let selected_agent_is_current = self
            .selected_user_agent_profile_id
            .as_deref()
            .zip(
                self.agent_profile_catalog
                    .state
                    .selected_agent_id
                    .as_deref(),
            )
            .is_some_and(|(selected_id, active_id)| selected_id == active_id);
        let selected_agent_choice = self
            .selected_user_agent_profile_id
            .as_deref()
            .and_then(|agent_id| self.agent_profile_catalog.agents.get(agent_id))
            .map(|profile| AgentChoice {
                id: profile.id.clone(),
                label: profile.name.clone(),
            });

        let activation_button: Element<'_, Message> = if let Some(choice) = selected_agent_choice {
            if selected_agent_is_current {
                container(text("该 Agent 当前已激活。").size(12))
                    .padding([10, 12])
                    .style(container::rounded_box)
                    .into()
            } else {
                button("设为当前代理")
                    .padding([10, 14])
                    .style(button::secondary)
                    .on_press(Message::ActiveAgentSelected(choice))
                    .into()
            }
        } else {
            container(text("新建草稿保存后即可设为当前代理。").size(12))
                .padding([10, 12])
                .style(container::rounded_box)
                .into()
        };

        let delete_button: Element<'_, Message> = if self.selected_user_agent_profile_id.is_some() {
            button("删除")
                .padding([10, 14])
                .style(button::danger)
                .on_press(Message::DeleteUserAgentProfile)
                .into()
        } else {
            button("删除")
                .padding([10, 14])
                .style(button::danger)
                .into()
        };

        let editor_panel = container(
            scrollable(
                column![
                    row![
                        column![
                            text("编辑用户 Agent").size(18),
                            text("用户 Agent 会覆盖系统默认主提示词，并可按需追加模型与参数覆盖。")
                                .size(12),
                        ]
                        .spacing(4),
                        space::horizontal(),
                        activation_button,
                        delete_button,
                        button("保存")
                            .padding([10, 14])
                            .style(button::primary)
                            .on_press(Message::SaveUserAgentProfile),
                    ]
                    .align_y(Center),
                    column![
                        text("稳定标识").size(13),
                        text_input("留空时会根据名称自动生成", &self.agent_profile_form.id,)
                            .on_input(Message::UserAgentProfileIdChanged)
                            .padding([10, 12]),
                    ]
                    .spacing(6),
                    column![
                        text("显示名称").size(13),
                        text_input("例如：审慎代码评审代理", &self.agent_profile_form.name)
                            .on_input(Message::UserAgentProfileNameChanged)
                            .padding([10, 12]),
                    ]
                    .spacing(6),
                    column![
                        text("简短描述").size(13),
                        text_input(
                            "说明这个 Agent 更适合处理什么任务",
                            &self.agent_profile_form.description,
                        )
                        .on_input(Message::UserAgentProfileDescriptionChanged)
                        .padding([10, 12]),
                    ]
                    .spacing(6),
                    row![
                        column![
                            text("模型覆盖").size(13),
                            text_input(
                                "可选，例如 openai/gpt-5.5",
                                &self.agent_profile_form.model,
                            )
                            .on_input(Message::UserAgentProfileModelChanged)
                            .padding([10, 12]),
                        ]
                        .spacing(6)
                        .width(FillPortion(3)),
                        column![
                            text("温度覆盖").size(13),
                            text_input(
                                "可选，例如 0.20",
                                &self.agent_profile_form.temperature_text,
                            )
                            .on_input(Message::UserAgentProfileTemperatureChanged)
                            .padding([10, 12]),
                        ]
                        .spacing(6)
                        .width(FillPortion(1)),
                        column![
                            text("最大轮次").size(13),
                            text_input("可选，例如 12", &self.agent_profile_form.max_turns_text,)
                                .on_input(Message::UserAgentProfileMaxTurnsChanged)
                                .padding([10, 12]),
                        ]
                        .spacing(6)
                        .width(FillPortion(1)),
                    ]
                    .spacing(10),
                    column![
                        text("主提示词 Markdown").size(13),
                        container(
                            text_editor(&self.agent_profile_form.prompt_buffer)
                                .height(320)
                                .padding(12)
                                .size(14)
                                .wrapping(iced::widget::text::Wrapping::Word)
                                .on_action(Message::UserAgentProfilePromptAction),
                        )
                        .style(container::bordered_box),
                    ]
                    .spacing(6),
                ]
                .spacing(12),
            )
            .height(Fill),
        )
        .padding(16)
        .style(container::bordered_box)
        .width(FillPortion(3))
        .height(Fill);

        column![
            panel_header,
            summary_card,
            row![profile_list, editor_panel].spacing(12).height(Fill),
        ]
        .spacing(12)
        .height(Fill)
        .into()
    }

    /// Builds one selectable user-agent summary card shown in the left list of the Agent panel.
    /// 构建显示在 Agent 面板左侧列表中的单个可选用户 Agent 摘要卡片。
    pub(super) fn view_user_agent_profile_card<'a>(
        &self,
        profile: &'a agent_profile::UserAgentProfile,
    ) -> Element<'a, Message> {
        let is_selected =
            self.selected_user_agent_profile_id.as_deref() == Some(profile.id.as_str());
        let is_active = self
            .agent_profile_catalog
            .state
            .selected_agent_id
            .as_deref()
            == Some(profile.id.as_str());
        let active_badge: Element<'_, Message> = if is_active {
            container(text("当前激活").size(11))
                .padding([4, 8])
                .style(container::rounded_box)
                .into()
        } else {
            space::horizontal().into()
        };
        let description_text = if profile.description.trim().is_empty() {
            String::from("未填写描述")
        } else {
            profile.description.clone()
        };
        let meta_segments = [
            Some(format!("ID：{}", profile.id)),
            profile.model.as_ref().map(|model| format!("模型：{model}")),
            profile
                .temperature
                .map(|temperature| format!("温度：{temperature:.2}")),
            profile.max_turns.map(|turns| format!("最大轮次：{turns}")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

        button(
            column![
                row![
                    text(&profile.name).size(15),
                    space::horizontal(),
                    active_badge,
                ]
                .align_y(Center),
                text(description_text).size(12),
                text(meta_segments.join(" · ")).size(11),
            ]
            .spacing(6),
        )
        .padding([12, 14])
        .width(Fill)
        .style(if is_selected {
            button::primary
        } else {
            button::secondary
        })
        .on_press(Message::UserAgentProfileSelected(profile.id.clone()))
        .into()
    }
}
