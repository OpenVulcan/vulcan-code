//! Prompt Skills management panel view composition.
//! Prompt Skills 管理面板视图组合逻辑。

use super::*;
use iced::widget::column;

impl VulcanWorkbench {
    /// Builds the Prompt Skills management panel backed by system and user skill directories.
    /// 构建由系统与用户技能目录驱动的 Prompt Skills 管理面板。
    pub(super) fn view_prompt_skills_panel(&self) -> Element<'_, Message> {
        let header = container(
            column![
                text(MainDisplayPanel::PromptSkills.localized_section_title(self.current_locale()))
                    .size(22),
                text(MainDisplayPanel::PromptSkills.localized_description(self.current_locale()))
                    .size(13),
            ]
            .spacing(4),
        )
        .padding([4, 2]);

        let global_enabled = self.provider_configuration.prompt_skills.enabled;
        let mut refresh_button = button("刷新").padding([8, 14]).style(button::secondary);
        refresh_button = refresh_button.on_press(Message::RefreshPromptSkillsCatalog);
        let install_system_button = button("同步系统技能")
            .padding([8, 14])
            .style(button::secondary)
            .on_press(Message::InstallSystemPromptSkills);

        let toggle_label = if global_enabled {
            "关闭全局技能"
        } else {
            "启用全局技能"
        };
        let toggle_button = button(toggle_label)
            .padding([8, 14])
            .on_press(Message::SetPromptSkillsEnabled(!global_enabled));

        let user_root = self
            .prompt_skills_catalog
            .as_ref()
            .map(|catalog| catalog.user_root.display().to_string())
            .unwrap_or_else(|| {
                self.provider_configuration
                    .prompt_skills
                    .user_skill_root
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| {
                        vulcan_prompt_skills::default_user_skill_root()
                            .display()
                            .to_string()
                    })
            });
        let system_root = self
            .prompt_skills_catalog
            .as_ref()
            .map(|catalog| catalog.system_root.display().to_string())
            .unwrap_or_else(|| {
                vulcan_prompt_skills::configured_system_skill_root(
                    &crate::skills::prompt::global_config_from_persisted(
                        &self.provider_configuration.prompt_skills,
                    ),
                )
                .display()
                .to_string()
            });

        let summary = container(
            column![
                row![
                    text("提示词技能").size(18),
                    space::horizontal().width(Fill),
                    toggle_button,
                    install_system_button,
                    refresh_button,
                ]
                .spacing(10)
                .align_y(Center),
                text(format!("系统技能根：{system_root}")).size(12),
                text(format!("用户技能根：{user_root}")).size(12),
                text(format!(
                    "全局状态：{}",
                    if global_enabled {
                        "已启用"
                    } else {
                        "已关闭"
                    }
                ))
                .size(12),
                text(&self.prompt_skills_notice).size(12),
            ]
            .spacing(10),
        )
        .padding(16)
        .style(container::bordered_box);

        let skill_list = match &self.prompt_skills_catalog {
            Some(catalog) if catalog.skills.is_empty() => {
                column![text("当前没有可用提示词技能，可先点击“同步系统技能”。").size(13)]
                    .spacing(8)
            }
            Some(catalog) => catalog
                .skills
                .iter()
                .fold(column![].spacing(12), |column, skill| {
                    column.push(self.view_prompt_skill_card(skill))
                }),
            None => column![
                text("提示词技能目录尚未载入。").size(13),
                text("打开面板或点击刷新后会扫描系统目录与用户目录下的 SKILL.md。").size(12),
            ]
            .spacing(8),
        };

        let errors: Option<Element<'_, Message>> = self
            .prompt_skills_catalog
            .as_ref()
            .filter(|catalog| !catalog.errors.is_empty())
            .map(|catalog| {
                let mut rows: iced::widget::Column<'_, Message> =
                    column![text("载入警告").size(16)].spacing(6);
                for error in &catalog.errors {
                    rows = rows.push(
                        text(format!("{}：{}", error.path.display(), error.message)).size(12),
                    );
                }
                container(rows)
                    .padding(12)
                    .style(container::rounded_box)
                    .width(Fill)
                    .into()
            });

        let mut list_body = column![
            text("可用提示词技能").size(18),
            scrollable(skill_list).height(Fill)
        ]
        .spacing(10)
        .height(Fill);
        if let Some(errors) = errors {
            list_body = list_body.push(errors);
        }

        let list_card = container(list_body)
            .padding(16)
            .style(container::bordered_box)
            .height(Fill);

        column![header, summary, list_card]
            .spacing(12)
            .height(Fill)
            .into()
    }

    /// Builds one Prompt Skills card row with global visibility controls and optional details.
    /// 构建带全局可见性控制与可选详情的单条 Prompt Skills 卡片。
    fn view_prompt_skill_card<'a>(
        &'a self,
        skill: &'a vulcan_prompt_skills::PromptSkill,
    ) -> Element<'a, Message> {
        let override_state = self
            .provider_configuration
            .prompt_skills
            .skill_overrides
            .get(&skill.name)
            .copied();
        let enabled = override_state
            .map(|state| state.is_enabled())
            .unwrap_or_else(|| {
                self.provider_configuration
                    .prompt_skills
                    .default_skill_state
                    .is_enabled()
            });
        let detail_open = self.selected_prompt_skill_name.as_deref() == Some(skill.name.as_str());
        let state_label = if enabled { "已启用" } else { "已停用" };
        let scope_label = skill.scope.zh_label();
        let display_name = prompt_skill_display_name(skill);
        let description = prompt_skill_short_description(skill);
        let icon = view_prompt_skill_icon(skill);
        let mut body = column![
            row![
                icon,
                column![
                    text(display_name).size(16),
                    text(format!("id: {}", skill.name)).size(11),
                ]
                .spacing(2),
                text(scope_label).size(11),
                space::horizontal().width(Fill),
                text(state_label).size(11),
                button(if enabled { "停用" } else { "启用" })
                    .padding([6, 10])
                    .on_press(Message::SetPromptSkillGlobalState {
                        name: skill.name.clone(),
                        enabled: !enabled,
                    }),
                button(if detail_open { "收起" } else { "详情" })
                    .padding([6, 10])
                    .style(button::secondary)
                    .on_press(Message::TogglePromptSkillDetails(skill.name.clone())),
            ]
            .spacing(10)
            .align_y(Center),
            text(description).size(12),
            text(format!("文件：{}", skill.skill_file.display())).size(12),
        ]
        .spacing(6);

        if detail_open {
            body = body.push(
                container(scrollable(text(skill.body.trim()).size(12).width(Fill)).height(220))
                    .padding(10)
                    .style(container::rounded_box),
            );
        }

        container(body)
            .padding([12, 14])
            .style(container::rounded_box)
            .width(Fill)
            .into()
    }
}

/// Builds one optional prompt-skill icon preview with a text fallback.
/// 构建一个带文本回退的可选提示词技能图标预览。
fn view_prompt_skill_icon<'a>(
    skill: &'a vulcan_prompt_skills::PromptSkill,
) -> Element<'a, Message> {
    if let Some(icon_path) = skill
        .interface
        .as_ref()
        .and_then(|interface| interface.icon_small.as_ref())
        .map(|path| skill.base_dir.join(path))
        .filter(|path| path.is_file())
    {
        let extension = icon_path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if extension == "svg" {
            return container(
                svg(svg::Handle::from_path(icon_path))
                    .width(24)
                    .height(24)
                    .content_fit(ContentFit::Contain),
            )
            .width(32)
            .height(32)
            .center(Fill)
            .into();
        }
        if matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "webp") {
            return container(
                iced_image(icon_path.display().to_string())
                    .width(24)
                    .height(24)
                    .content_fit(ContentFit::Contain),
            )
            .width(32)
            .height(32)
            .center(Fill)
            .into();
        }
    }

    let fallback = skill
        .name
        .chars()
        .find(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_uppercase().to_string())
        .unwrap_or_else(|| String::from("*"));
    container(text(fallback).size(13))
        .width(32)
        .height(32)
        .center(Fill)
        .style(container::rounded_box)
        .into()
}
