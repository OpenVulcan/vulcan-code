//! Settings-navigation view composition methods extracted from the main workbench views module.
//! 从主工作台视图模块中抽取的设置导航相关界面组合方法。

use super::*;
use iced::widget::column;

impl VulcanWorkbench {
    /// Builds the left settings drawer using the same visual structure as the main task panel.
    /// 构建与主任务面板使用相同视觉结构的左侧设置抽屉。
    pub(super) fn view_settings_navigation_panel(&self) -> Element<'_, Message> {
        let navigation_tree = scrollable(self.view_settings_navigation_tree())
            .height(Fill)
            .direction(scrollable::Direction::Vertical(
                scrollable::Scrollbar::new()
                    .width(6)
                    .margin(0)
                    .scroller_width(6),
            ));

        container(column![navigation_tree].spacing(10).height(Fill))
            .padding([12, 2])
            .width(Fill)
            .height(Fill)
            .into()
    }

    /// Builds the collapsible left navigation tree used by the dedicated settings workspace.
    /// 构建独立设置工作区使用的可折叠左侧导航树。
    pub(super) fn view_settings_navigation_tree(&self) -> Element<'_, Message> {
        let mut navigation =
            column![self.view_main_task_group_label(self.t(I18nKey::SettingsRootTitle), false)]
            .spacing(1)
            .width(Fill);
        navigation = navigation.push(self.view_settings_navigation_model_group_button());
        if self.settings_model_tree_expanded {
            navigation = navigation
                .push(self.view_settings_navigation_child_button(SettingsSection::ModelProviders))
                .push(self.view_settings_navigation_child_button(SettingsSection::ModelList));
        }
        navigation = navigation.push(self.view_settings_navigation_ai_group_button());
        if self.settings_ai_capability_tree_expanded {
            navigation =
                navigation
                    .push(self.view_settings_navigation_child_button(
                        SettingsSection::AiCapabilityVendors,
                    ))
                    .push(self.view_settings_navigation_child_button(
                        SettingsSection::AiCapabilityWebSearch,
                    ))
                    .push(self.view_settings_navigation_child_button(
                        SettingsSection::AiCapabilityUnderstandImage,
                    ))
                    .push(self.view_settings_navigation_child_button(
                        SettingsSection::AiCapabilityUnderstandVideo,
                    ))
                    .push(self.view_settings_navigation_child_button(
                        SettingsSection::AiCapabilityUnderstandAudio,
                    ))
                    .push(self.view_settings_navigation_child_button(
                        SettingsSection::AiCapabilityImageGeneration,
                    ))
                    .push(self.view_settings_navigation_child_button(
                        SettingsSection::AiCapabilityVideoGeneration,
                    ))
                    .push(self.view_settings_navigation_child_button(
                        SettingsSection::AiCapabilitySpeechSynthesis,
                    ))
                    .push(self.view_settings_navigation_child_button(
                        SettingsSection::AiCapabilityMusicGeneration,
                    ))
                    .push(self.view_settings_navigation_child_button(
                        SettingsSection::AiCapabilityMusicCover,
                    ));
        }
        navigation = navigation
            .push(self.view_settings_navigation_top_level_button(SettingsSection::McpServers))
            .push(self.view_settings_navigation_top_level_button(SettingsSection::ArchivedRestore))
            .push(
                self.view_settings_navigation_top_level_button(SettingsSection::RuntimeDiagnostics),
            );

        container(navigation).width(Fill).into()
    }

    /// Builds the collapsible model-settings parent row inside the left settings navigation drawer.
    /// 构建左侧设置导航抽屉中的可折叠模型设置父级树节点行。
    pub(super) fn view_settings_navigation_model_group_button(&self) -> Element<'_, Message> {
        let folder_icon: Element<'_, Message> = if self.settings_model_tree_expanded {
            codicon::folder_opened().size(16).line_height(1.0).into()
        } else {
            codicon::folder().size(16).line_height(1.0).into()
        };
        let chevron_icon: Element<'_, Message> = if self.settings_model_tree_expanded {
            codicon::chevron_down().size(14).line_height(1.0).into()
        } else {
            codicon::chevron_right().size(14).line_height(1.0).into()
        };

        button(
            container(
                row![
                    container(folder_icon).width(16).center_y(Fill),
                    text(self.t(I18nKey::SettingsModelGroupTitle))
                        .size(14)
                        .wrapping(iced::advanced::text::Wrapping::None),
                    space::horizontal(),
                    container(chevron_icon).width(16).center_y(Fill),
                ]
                .spacing(8)
                .align_y(Center),
            )
            .padding(iced::Padding {
                top: 3.0,
                right: MAIN_TASK_TREE_SECTION_LEFT_PADDING,
                bottom: 3.0,
                left: MAIN_TASK_TREE_SECTION_LEFT_PADDING,
            })
            .height(MAIN_TASK_PROJECT_ROW_HEIGHT)
            .width(Fill),
        )
        .width(Fill)
        .padding(0)
        .style(move |theme, status| {
            style::main_task_tree_session_button_style(theme, status, false, false)
        })
        .on_press(Message::ToggleModelSettingsTree)
        .into()
    }

    /// Builds one reusable navigation-tree row shared by settings, target pickers, and quick capability sidebars.
    /// 构建由设置页、目标选择器与快捷能力侧栏复用的通用导航树行。
    pub(super) fn view_tree_navigation_button_row<'a>(
        &self,
        icon: Element<'a, Message>,
        label: String,
        secondary: Option<String>,
        trailing: Option<Element<'a, Message>>,
        is_selected: bool,
        left_padding: f32,
        on_press: Message,
    ) -> Element<'a, Message> {
        self.view_tree_navigation_button_row_with_leading(
            None,
            icon,
            label,
            secondary,
            trailing,
            is_selected,
            left_padding,
            on_press,
        )
    }

    /// Builds one reusable tree-navigation row that optionally includes one extra leading control before the icon.
    /// 构建一个可复用树形导航行，并可选地在图标前插入额外前导控件。
    pub(super) fn view_tree_navigation_button_row_with_leading<'a>(
        &self,
        leading: Option<Element<'a, Message>>,
        icon: Element<'a, Message>,
        label: String,
        secondary: Option<String>,
        trailing: Option<Element<'a, Message>>,
        is_selected: bool,
        left_padding: f32,
        on_press: Message,
    ) -> Element<'a, Message> {
        let palette = self.app_theme.extended_palette();
        let text_column = if let Some(secondary) = secondary {
            column![
                text(label)
                    .size(13)
                    .wrapping(iced::advanced::text::Wrapping::None),
                text(secondary)
                    .size(11)
                    .color(palette.background.base.text.scale_alpha(0.56))
                    .wrapping(iced::advanced::text::Wrapping::None),
            ]
            .spacing(3)
            .width(Fill)
        } else {
            column![
                text(label)
                    .size(13)
                    .wrapping(iced::advanced::text::Wrapping::None)
            ]
            .spacing(0)
            .width(Fill)
        };

        button(
            container(
                row![
                    leading.unwrap_or_else(|| space::horizontal().width(0).into()),
                    icon,
                    text_column,
                    space::horizontal(),
                    trailing.unwrap_or_else(|| space::horizontal().width(0).into()),
                ]
                .spacing(8)
                .align_y(Center),
            )
            .padding(iced::Padding {
                top: 3.0,
                right: MAIN_TASK_TREE_SECTION_LEFT_PADDING,
                bottom: 3.0,
                left: left_padding,
            })
            .width(Fill)
            .height(MAIN_TASK_PROJECT_ROW_HEIGHT),
        )
        .width(Fill)
        .padding(0)
        .style(move |theme, status| {
            style::main_task_tree_session_button_style(theme, status, is_selected, false)
        })
        .on_press(on_press)
        .into()
    }

    /// Builds one top-level tree row inside the left settings navigation drawer.
    /// 构建左侧设置导航抽屉中的顶层树节点行。
    pub(super) fn view_settings_navigation_top_level_button(
        &self,
        section: SettingsSection,
    ) -> Element<'_, Message> {
        let is_selected = self.selected_settings_section == section;
        let icon = self.view_settings_navigation_icon(section, false);
        self.view_tree_navigation_button_row(
            icon,
            section.localized_label(self.current_locale()),
            None,
            None,
            is_selected,
            MAIN_TASK_TREE_SECTION_LEFT_PADDING,
            Message::SettingsSectionSelected(section),
        )
    }

    /// Builds the collapsible AI-capability parent row inside the left settings navigation drawer.
    /// 构建左侧设置导航抽屉中的可折叠 AI 能力父级树节点行。
    pub(super) fn view_settings_navigation_ai_group_button(&self) -> Element<'_, Message> {
        let folder_icon: Element<'_, Message> = if self.settings_ai_capability_tree_expanded {
            codicon::folder_opened().size(16).line_height(1.0).into()
        } else {
            codicon::folder().size(16).line_height(1.0).into()
        };
        let chevron_icon: Element<'_, Message> = if self.settings_ai_capability_tree_expanded {
            codicon::chevron_down().size(14).line_height(1.0).into()
        } else {
            codicon::chevron_right().size(14).line_height(1.0).into()
        };

        button(
            container(
                row![
                    container(folder_icon).width(16).center_y(Fill),
                    text(self.t(I18nKey::SettingsAiCapabilityGroupTitle))
                        .size(14)
                        .wrapping(iced::advanced::text::Wrapping::None),
                    space::horizontal(),
                    container(chevron_icon).width(16).center_y(Fill),
                ]
                .spacing(8)
                .align_y(Center),
            )
            .padding(iced::Padding {
                top: 3.0,
                right: MAIN_TASK_TREE_SECTION_LEFT_PADDING,
                bottom: 3.0,
                left: MAIN_TASK_TREE_SECTION_LEFT_PADDING,
            })
            .height(MAIN_TASK_PROJECT_ROW_HEIGHT)
            .width(Fill),
        )
        .width(Fill)
        .padding(0)
        .style(move |theme, status| {
            style::main_task_tree_session_button_style(theme, status, false, false)
        })
        .on_press(Message::ToggleAiCapabilitySettingsTree)
        .into()
    }

    /// Builds one child row shown under the AI-capability parent inside the settings navigation drawer.
    /// 构建显示在设置导航抽屉中 AI 能力父节点下方的子级树节点行。
    pub(super) fn view_settings_navigation_child_button(
        &self,
        section: SettingsSection,
    ) -> Element<'_, Message> {
        let is_selected = self.selected_settings_section == section;
        let icon = self.view_settings_navigation_icon(section, true);
        self.view_tree_navigation_button_row(
            icon,
            section.localized_label(self.current_locale()),
            None,
            None,
            is_selected,
            35.0,
            Message::SettingsSectionSelected(section),
        )
    }

    /// Builds the semantic icon surface used by one settings navigation row.
    /// 构建单条设置导航行使用的语义图标表面。
    pub(super) fn view_settings_navigation_icon(
        &self,
        section: SettingsSection,
        is_child: bool,
    ) -> Element<'_, Message> {
        let palette = self.app_theme.extended_palette();
        let color = palette
            .background
            .base
            .text
            .scale_alpha(if is_child { 0.72 } else { 0.82 });
        let icon: Element<'_, Message> = match section {
            SettingsSection::ModelProviders => lucide::file_text()
                .size(15)
                .line_height(1.0)
                .color(color)
                .into(),
            SettingsSection::ModelList => lucide::file_text()
                .size(15)
                .line_height(1.0)
                .color(color)
                .into(),
            SettingsSection::RuntimeDiagnostics => codicon::sync()
                .size(15)
                .line_height(1.0)
                .color(color)
                .into(),
            SettingsSection::AiCapabilityVendors => lucide::file_text()
                .size(15)
                .line_height(1.0)
                .color(color)
                .into(),
            SettingsSection::AiCapabilityWebSearch => lucide::search()
                .size(15)
                .line_height(1.0)
                .color(color)
                .into(),
            SettingsSection::AiCapabilityUnderstandImage => lucide::image()
                .size(15)
                .line_height(1.0)
                .color(color)
                .into(),
            SettingsSection::AiCapabilityUnderstandVideo => lucide::video()
                .size(15)
                .line_height(1.0)
                .color(color)
                .into(),
            SettingsSection::AiCapabilityUnderstandAudio => lucide::volume_two()
                .size(15)
                .line_height(1.0)
                .color(color)
                .into(),
            SettingsSection::AiCapabilityImageGeneration => lucide::image()
                .size(15)
                .line_height(1.0)
                .color(color)
                .into(),
            SettingsSection::AiCapabilityVideoGeneration => lucide::video()
                .size(15)
                .line_height(1.0)
                .color(color)
                .into(),
            SettingsSection::AiCapabilitySpeechSynthesis => lucide::volume_two()
                .size(15)
                .line_height(1.0)
                .color(color)
                .into(),
            SettingsSection::AiCapabilityMusicGeneration => lucide::volume_two()
                .size(15)
                .line_height(1.0)
                .color(color)
                .into(),
            SettingsSection::AiCapabilityMusicCover => codicon::sync()
                .size(15)
                .line_height(1.0)
                .color(color)
                .into(),
            SettingsSection::McpServers => codicon::folder()
                .size(15)
                .line_height(1.0)
                .color(color)
                .into(),
            SettingsSection::ArchivedRestore => codicon::archive()
                .size(15)
                .line_height(1.0)
                .color(color)
                .into(),
        };

        container(icon).width(18).center_y(Fill).into()
    }

    /// Builds the dedicated archived-item restore card shown inside settings instead of the main session tree.
    /// 构建展示在设置页面中的独立归档恢复卡片，而不是继续显示在主会话树里。
    pub(super) fn view_archived_restore_card(&self) -> Element<'_, Message> {
        let archived_project_session_ids = self
            .archived_task_projects
            .iter()
            .flat_map(|project| project.session_ids.iter().copied())
            .collect::<HashSet<_>>();
        let standalone_archived_sessions = self
            .archived_sessions
            .iter()
            .filter(|session| !archived_project_session_ids.contains(&session.id))
            .collect::<Vec<_>>();

        let mut content = column![
            row![
                text(self.t(I18nKey::SettingsArchivedRestoreTitle)).size(18),
                space::horizontal(),
                text(self.t(I18nKey::SettingsArchivedRestoreHint)).size(12),
            ]
            .align_y(Center)
        ]
        .spacing(12);

        if self.archived_task_projects.is_empty() && standalone_archived_sessions.is_empty() {
            content = content.push(text(self.t(I18nKey::SettingsArchivedRestoreEmpty)).size(13));
        } else {
            if !self.archived_task_projects.is_empty() {
                content = content.push(text(self.t(I18nKey::SettingsArchivedProjectsTitle)).size(14)).push(
                    self.archived_task_projects
                        .iter()
                        .fold(column![].spacing(1).width(Fill), |column, project| {
                            column.push(self.view_archived_task_project_row(project))
                        }),
                );
            }

            if !standalone_archived_sessions.is_empty() {
                content = content.push(text(self.t(I18nKey::SettingsArchivedStandaloneSessionsTitle)).size(14)).push(
                    standalone_archived_sessions.into_iter().fold(
                        column![].spacing(1).width(Fill),
                        |column, session| {
                            column.push(self.view_archived_task_session_tree_row(session))
                        },
                    ),
                );
            }
        }

        if let Some(confirmation) = self.pending_archived_delete_confirmation.as_ref() {
            content = content
                .push(space::vertical().height(6))
                .push(self.view_archived_delete_confirmation_card(confirmation));
        }

        container(content)
            .padding(16)
            .style(container::bordered_box)
            .into()
    }
}
