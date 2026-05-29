//! AI capability configuration and modal view composition methods extracted from the main workbench views module.
//! 从主工作台视图模块中抽取的 AI 能力配置与弹窗界面组合方法。

use super::*;
use crate::provider_config::ResolvedAiCapabilityRouteTarget;
use iced::widget::{column, opaque};

impl VulcanWorkbench {
    /// Builds the blocking quick AI capability modal opened from the composer control row.
    /// 构建从输入区控制行打开的阻塞式快捷 AI 能力面板弹窗。
    pub(super) fn view_workbench_ai_capability_panel_modal(&self) -> Option<Element<'_, Message>> {
        if !self.workbench_ai_capability_panel_open {
            return None;
        }

        let palette = self.app_theme.extended_palette();
        let selected_section = self.workbench_ai_capability_section;
        let close_button = button(
            container(
                codicon::chrome_close()
                    .size(14)
                    .line_height(1.0)
                    .color(palette.background.base.text.scale_alpha(0.72)),
            )
            .width(Fill)
            .height(Fill)
            .center(Fill),
        )
        .width(30)
        .height(30)
        .padding(0)
        .style(style::workbench_model_trigger_button_style)
        .on_press(Message::CloseWorkbenchAiCapabilityPanel);
        let left_navigation = scrollable(
            WorkbenchAiCapabilitySection::ALL
                .into_iter()
                .fold(column![].spacing(1).width(Fill), |list, section| {
                    list.push(self.view_workbench_ai_capability_section_row(section))
                }),
        )
        .height(420);
        let right_panel: Element<'_, Message> =
            if selected_section == WorkbenchAiCapabilitySection::PromptSkills {
                self.view_workbench_prompt_skills_quick_panel()
            } else if selected_section == WorkbenchAiCapabilitySection::VmmMemory {
                self.view_workbench_vmm_quick_panel()
            } else if let Some(capability_kind) = selected_section.capability_kind() {
                self.view_ai_capability_management_panel(capability_kind, false, false, true)
            } else if selected_section.supports_routing_strategy() {
                self.view_ai_capability_unavailable_page(
                    selected_section.label(),
                    "当前能力入口已预留；后续接通后，这里会提供原生优先与工具理解等路由策略选项。",
                )
                .into()
            } else {
                self.view_ai_capability_unavailable_page(
                    selected_section.label(),
                    "当前能力入口已预留，但还没有接通可配置的目标与执行工具。",
                )
                .into()
            };
        let modal_card = container(
            column![
                container(
                    row![
                        column![
                            text("能力扩展设置").size(20),
                            text("这里用于高频调整已配置能力的启用状态与顺序；新增供应商仍需前往设置页。")
                                .size(12)
                                .color(palette.background.base.text.scale_alpha(0.62)),
                        ]
                        .spacing(4),
                        space::horizontal(),
                        close_button,
                    ]
                    .align_y(Center),
                )
                .padding(iced::Padding {
                    top: 0.0,
                    right: 0.0,
                    bottom: 14.0,
                    left: 0.0,
                }),
                rule::horizontal(1),
                row![
                    container(left_navigation)
                        .padding(iced::Padding {
                            top: 14.0,
                            right: 14.0,
                            bottom: 0.0,
                            left: 0.0,
                        })
                        .width(200)
                        .height(Fill),
                    rule::vertical(1),
                    scrollable(
                        container(right_panel)
                            .padding(iced::Padding {
                                top: 14.0,
                                right: 0.0,
                                bottom: 0.0,
                                left: 14.0,
                            })
                            .width(Fill),
                    )
                        .width(Fill)
                        .height(410),
                ]
                .spacing(0)
                .height(420),
            ]
            .spacing(0),
        )
        .padding([22, 24])
        .width(920)
        .style(style::startup_overlay_card_style);
        let backdrop = mouse_area(container(space()).width(Fill).height(Fill))
            .on_press(Message::CloseWorkbenchAiCapabilityPanel);

        Some(
            opaque(
                stack([
                    container(backdrop)
                        .width(Fill)
                        .height(Fill)
                        .style(style::startup_overlay_backdrop_style)
                        .into(),
                    container(mouse_area(modal_card).on_press(Message::Noop))
                        .width(Fill)
                        .height(Fill)
                        .center(Fill)
                        .into(),
                ])
                .width(Fill)
                .height(Fill),
            )
            .into(),
        )
    }
    /// Builds the compact AI capability quick controls shown beside the workbench model trigger.
    /// 构建显示在工作台模型触发器旁侧的紧凑 AI 能力快捷控制区。
    pub(super) fn view_workbench_ai_capability_quick_controls(&self) -> Element<'_, Message> {
        let icon = lucide::sliders_horizontal()
            .size(14)
            .line_height(1.0)
            .color(self.app_theme.extended_palette().primary.base.color)
            .into();

        self.view_workbench_dual_line_trigger(
            Some(icon),
            String::from("扩展能力"),
            String::from("能力扩展设置"),
            WORKBENCH_AI_CAPABILITY_QUICK_CONTROL_WIDTH,
            true,
            Some(Message::OpenWorkbenchAiCapabilityPanel),
        )
    }
    /// Builds the semantic icon rendered beside one quick capability section row.
    /// 构建渲染在快捷能力分区行左侧的语义图标。
    pub(super) fn view_workbench_ai_capability_section_icon(
        &self,
        section: WorkbenchAiCapabilitySection,
    ) -> Element<'_, Message> {
        let palette = self.app_theme.extended_palette();
        let color = palette.background.base.text.scale_alpha(0.82);
        let icon: Element<'_, Message> = match section {
            WorkbenchAiCapabilitySection::WebSearch => lucide::search()
                .size(15)
                .line_height(1.0)
                .color(color)
                .into(),
            WorkbenchAiCapabilitySection::UnderstandImage => lucide::image()
                .size(15)
                .line_height(1.0)
                .color(color)
                .into(),
            WorkbenchAiCapabilitySection::UnderstandVideo => lucide::video()
                .size(15)
                .line_height(1.0)
                .color(color)
                .into(),
            WorkbenchAiCapabilitySection::UnderstandAudio => lucide::volume_two()
                .size(15)
                .line_height(1.0)
                .color(color)
                .into(),
            WorkbenchAiCapabilitySection::ImageGeneration => lucide::image_plus()
                .size(15)
                .line_height(1.0)
                .color(color)
                .into(),
            WorkbenchAiCapabilitySection::VideoGeneration => lucide::video()
                .size(15)
                .line_height(1.0)
                .color(color)
                .into(),
            WorkbenchAiCapabilitySection::SpeechSynthesis => {
                lucide::mic().size(15).line_height(1.0).color(color).into()
            }
            WorkbenchAiCapabilitySection::MusicGeneration => lucide::music()
                .size(15)
                .line_height(1.0)
                .color(color)
                .into(),
            WorkbenchAiCapabilitySection::MusicCover => codicon::sync()
                .size(15)
                .line_height(1.0)
                .color(color)
                .into(),
            WorkbenchAiCapabilitySection::PromptSkills => lucide::sparkles()
                .size(15)
                .line_height(1.0)
                .color(color)
                .into(),
            WorkbenchAiCapabilitySection::VmmMemory => lucide::database()
                .size(15)
                .line_height(1.0)
                .color(color)
                .into(),
        };

        container(icon).width(18).center_y(Fill).into()
    }
    /// Builds one single-level quick capability section row shown in the left shortcut navigation list.
    /// 构建显示在左侧快捷导航列表中的单级快捷能力分区行。
    pub(super) fn view_workbench_ai_capability_section_row(
        &self,
        section: WorkbenchAiCapabilitySection,
    ) -> Element<'_, Message> {
        let is_selected = section == self.workbench_ai_capability_section;
        let toggle_available = self.workbench_ai_capability_is_available(section);
        let toggle_enabled = self.workbench_ai_capability_is_enabled(section);
        let toggle: Element<'_, Message> =
            if section == WorkbenchAiCapabilitySection::PromptSkills {
                checkbox(self.provider_configuration.prompt_skills.enabled)
                    .spacing(0)
                    .size(16)
                    .on_toggle(Message::SetPromptSkillsEnabled)
                    .into()
            } else if section == WorkbenchAiCapabilitySection::VmmMemory {
                let checkbox = checkbox(toggle_enabled).spacing(0).size(16);
                if toggle_available {
                    checkbox.on_toggle(Message::SetSessionVmmEnabled).into()
                } else {
                    checkbox.into()
                }
            } else if section.capability_kind().is_some() {
                let checkbox = checkbox(toggle_enabled).spacing(0).size(16);
                if toggle_available {
                    checkbox
                        .on_toggle(move |enabled| {
                            Message::WorkbenchAiCapabilitySectionEnabledToggled { section, enabled }
                        })
                        .into()
                } else {
                    checkbox.into()
                }
            } else {
                checkbox(false).spacing(0).size(16).into()
            };
        self.view_tree_navigation_button_row_with_leading(
            Some(toggle),
            self.view_workbench_ai_capability_section_icon(section),
            if section == WorkbenchAiCapabilitySection::PromptSkills
                || section == WorkbenchAiCapabilitySection::VmmMemory
                || section.capability_kind().is_some()
            {
                section.label().to_string()
            } else {
                format!("{}（待开放）", section.label())
            },
            None,
            None,
            is_selected,
            MAIN_TASK_TREE_SECTION_LEFT_PADDING,
            Message::WorkbenchAiCapabilitySectionSelected(section),
        )
    }

    /// Builds the quick Prompt Skills panel shown inside the workbench capability modal.
    /// 构建工作台能力弹窗中的快捷 Prompt Skills 面板。
    pub(super) fn view_workbench_prompt_skills_quick_panel(&self) -> Element<'_, Message> {
        let enabled = self.provider_configuration.prompt_skills.enabled;
        let catalog_summary = self
            .prompt_skills_catalog
            .as_ref()
            .map(|catalog| {
                let system_count = catalog
                    .skills
                    .iter()
                    .filter(|skill| skill.scope == vulcan_prompt_skills::PromptSkillScope::System)
                    .count();
                let user_count = catalog.skills.len().saturating_sub(system_count);
                format!("已扫描系统 {system_count} 个、用户 {user_count} 个")
            })
            .unwrap_or_else(|| String::from("尚未扫描系统与用户技能目录"));
        let mut refresh_button = button("刷新技能目录")
            .padding([8, 12])
            .style(button::secondary);
        refresh_button = refresh_button.on_press(Message::RefreshPromptSkillsCatalog);

        let skill_rows = match &self.prompt_skills_catalog {
            Some(catalog) if catalog.skills.is_empty() => {
                column![text("当前没有可用提示词技能，可先同步系统技能。").size(12)].spacing(6)
            }
            Some(catalog) => catalog
                .skills
                .iter()
                .fold(column![].spacing(6), |list, skill| {
                    list.push(self.view_workbench_prompt_skill_session_row(skill))
                }),
            None => column![text("刷新后可在这里按当前会话启停具体技能。").size(12)].spacing(6),
        };

        container(
            column![
                text("提示词技能").size(18),
                text("启用后，请求会只注入技能名称、描述与路径；AI 需要完整内容时再调用 `skill` 工具加载。")
                    .size(12),
                row![
                    checkbox(enabled)
                        .label("启用全局 Prompt Skills")
                        .on_toggle(Message::SetPromptSkillsEnabled),
                    space::horizontal().width(Fill),
                    refresh_button,
                ]
                .spacing(10)
                .align_y(Center),
                text(catalog_summary).size(12),
                text(&self.prompt_skills_notice).size(12),
                scrollable(skill_rows).height(170),
                button("打开完整技能面板")
                    .padding([8, 12])
                    .on_press(Message::MainDisplayPanelSelected(MainDisplayPanel::PromptSkills)),
            ]
            .spacing(12),
        )
        .padding(16)
        .style(container::bordered_box)
        .width(Fill)
        .into()
    }

    /// Builds the session-scoped VMM quick toggle panel.
    /// 构建会话级 VMM 快捷开关面板。
    pub(super) fn view_workbench_vmm_quick_panel(&self) -> Element<'_, Message> {
        let session_layer = self
            .session_runtime_state
            .get(&self.selected_session)
            .map(|state| &state.vmm);
        let project_layer = self.current_project_vmm_configuration(self.selected_session);
        let effective = vulcan_vmm::VmmEffectiveConfig::resolve(
            &self.provider_configuration.vmm,
            Some(&project_layer),
            session_layer,
        );
        let toggle_available = !effective.grpc_target.trim().is_empty();
        let session_memory_enabled = session_layer
            .and_then(|layer| layer.enabled)
            .unwrap_or(effective.enabled);
        let session_ai_tools_enabled = session_layer
            .and_then(|layer| layer.ai_tools_enabled)
            .unwrap_or(effective.ai_tools_enabled);
        let diagnostic = effective.diagnose_memory_flow(!effective.grpc_target.trim().is_empty());
        let memory_checkbox = checkbox(session_memory_enabled);
        let memory_checkbox: Element<'_, Message> = if toggle_available {
            memory_checkbox
                .on_toggle(Message::SetSessionVmmEnabled)
                .into()
        } else {
            memory_checkbox.into()
        };
        let ai_tools_checkbox = checkbox(session_ai_tools_enabled);
        let ai_tools_checkbox: Element<'_, Message> = if toggle_available {
            ai_tools_checkbox
                .on_toggle(Message::SetSessionVmmAiToolsEnabled)
                .into()
        } else {
            ai_tools_checkbox.into()
        };
        container(
            column![
                text("当前会话 VMM").size(16),
                text(format!(
                    "gRPC：{} · user/project：{}/{}",
                    if effective.grpc_target.trim().is_empty() {
                        "未配置"
                    } else {
                        effective.grpc_target.as_str()
                    },
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
                row![memory_checkbox, text("本会话启用记忆").size(13),]
                    .spacing(8)
                    .align_y(Center),
                row![ai_tools_checkbox, text("本会话启用 AI 专属工具").size(13),]
                    .spacing(8)
                    .align_y(Center),
                text(format!("状态：{diagnostic:?}")).size(12),
                text(
                    "这里只写入当前会话覆盖；全局地址与项目 ID 请在能力供应商页或项目设置中配置。"
                )
                .size(12),
            ]
            .spacing(10),
        )
        .padding(12)
        .style(container::bordered_box)
        .into()
    }

    /// Builds one session-scoped Prompt Skill override row for the quick capability panel.
    /// 为快捷能力面板构建单条会话级 Prompt Skill 覆盖行。
    fn view_workbench_prompt_skill_session_row<'a>(
        &'a self,
        skill: &'a vulcan_prompt_skills::PromptSkill,
    ) -> Element<'a, Message> {
        let session_override = self
            .session_runtime_state(self.selected_session)
            .and_then(|state| state.prompt_skill_overrides.get(&skill.name))
            .copied();
        let global_enabled = self
            .provider_configuration
            .prompt_skills
            .skill_overrides
            .get(&skill.name)
            .copied()
            .unwrap_or(
                self.provider_configuration
                    .prompt_skills
                    .default_skill_state,
            )
            .is_enabled();
        let effective_enabled = session_override
            .map(|state| state.is_enabled())
            .unwrap_or(global_enabled);
        let display_name = prompt_skill_display_name(skill);
        container(
            row![
                column![
                    text(display_name).size(13),
                    text(if session_override.is_some() {
                        "当前会话覆盖"
                    } else {
                        "继承全局/项目规则"
                    })
                    .size(11),
                ]
                .spacing(2)
                .width(Fill),
                checkbox(effective_enabled).spacing(0).size(16).on_toggle({
                    let name = skill.name.clone();
                    move |enabled| Message::SetPromptSkillSessionState {
                        name: name.clone(),
                        enabled,
                    }
                }),
            ]
            .spacing(10)
            .align_y(Center),
        )
        .padding([8, 10])
        .style(container::rounded_box)
        .width(Fill)
        .into()
    }

    /// Renders the shared AI capability vendor page where one provider instance is configured only once and later reused by multiple capability tabs.
    /// 渲染共享 AI 能力供应商页面，使同一供应商实例只配置一次并复用于多个能力分页。
    pub(super) fn view_ai_tool_configuration_card(&self) -> Element<'_, Message> {
        let vendor_entries: Element<'_, Message> = if self.ai_tool_configuration.vendors.is_empty()
        {
            container(
                column![
                    text("还没有共享能力供应商。").size(13),
                    text("先点击右上角“新增供应商”，从目录里选择一个供应商并填写 API Key。若该供应商需要区域，会在独立配置弹窗中提供区域下拉框或输入框。")
                        .size(12),
                ]
                .spacing(6),
            )
            .padding(12)
            .style(container::bordered_box)
            .into()
        } else {
            self.ai_tool_configuration
                .vendors
                .iter()
                .fold(column![].spacing(10), |list, vendor| {
                    let vendor_definitions =
                        crate::ai_capability_provider_catalog::ai_capability_provider_definitions_for_vendor(
                            &vendor.vendor_id,
                        );
                    let capability_summary = vendor_definitions
                        .iter()
                        .map(|definition| definition.capability_kind.label())
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect::<Vec<_>>()
                        .join("、");
                    let region_summary = if vendor.region.trim().is_empty() {
                        String::from("未设置区域")
                    } else {
                        format!("区域：{}", vendor.region.trim())
                    };
                    let api_key_summary = if vendor.api_key.trim().is_empty() {
                        String::from("API Key：未填写")
                    } else {
                        String::from("API Key：已填写")
                    };
                    let remove_vendor_id = vendor.vendor_id.clone();
                    let edit_vendor_id = vendor.vendor_id.clone();

                    list.push(
                        container(
                            column![
                                row![
                                    column![
                                        text(&vendor.name).size(16),
                                        text(format!(
                                            "{} · {} · 支持能力：{}",
                                            api_key_summary,
                                            region_summary,
                                            if capability_summary.is_empty() {
                                                "暂无".to_string()
                                            } else {
                                                capability_summary
                                            }
                                        ))
                                        .size(12)
                                        .color(
                                            self.app_theme
                                                .extended_palette()
                                                .background
                                                .base
                                                .text
                                                .scale_alpha(0.62),
                                        ),
                                    ]
                                    .spacing(6),
                                    space::horizontal(),
                                    button("编辑")
                                        .padding([4, 10])
                                        .style(|theme, status| {
                                            style::workbench_label_action_button_style(
                                                theme, status, false,
                                            )
                                        })
                                        .on_press(Message::OpenAiCapabilityVendorEditor(
                                            edit_vendor_id,
                                        )),
                                    button("移除")
                                        .padding([4, 10])
                                        .style(|theme, status| {
                                            style::workbench_label_action_button_style(
                                                theme, status, true,
                                            )
                                        })
                                        .on_press(Message::RemoveAiCapabilityVendor(
                                            remove_vendor_id,
                                        )),
                                ]
                                .align_y(Center)
                                .spacing(10),
                            ]
                            .spacing(0),
                        )
                        .padding(12)
                        .style(container::bordered_box),
                    )
                })
                .into()
        };

        scrollable(
            column![
                self.view_global_vmm_settings_card(),
                container(vendor_entries).width(Fill)
            ]
            .spacing(12),
        )
        .height(Fill)
        .spacing(8)
        .width(Fill)
        .into()
    }
    /// Builds the insertion indicator shown before one enabled AI capability route row while reordering.
    /// 构建 AI 能力已启用路由行重排期间显示在某条目标行之前的插入指示器。
    pub(super) fn view_ai_capability_route_drop_indicator(
        &self,
        is_visible: bool,
    ) -> Element<'_, Message> {
        let height = if is_visible {
            AI_CAPABILITY_ROUTE_DROP_INDICATOR_HEIGHT
        } else {
            0.0
        };
        let indicator = if is_visible {
            container(space())
                .width(Fill)
                .height(2.0)
                .style(style::workbench_pending_prompt_drop_indicator_style)
        } else {
            container(space()).width(Fill).height(0.0)
        };
        container(indicator)
            .padding(iced::Padding {
                top: 0.0,
                right: 2.0,
                bottom: if is_visible { 2.0 } else { 0.0 },
                left: 2.0,
            })
            .height(height)
            .width(Fill)
            .into()
    }
    /// Builds one draggable AI capability route row rendered inside one capability settings or quick panel.
    /// 构建渲染在单个能力设置页或快捷面板中的单条可拖拽 AI 能力路由行。
    pub(super) fn view_ai_capability_route_row(
        &self,
        capability_kind: AiToolKind,
        index: usize,
        entry: ResolvedAiCapabilityRouteTarget,
        show_remove_action: bool,
    ) -> Element<'_, Message> {
        let is_drag_source = self.dragging_ai_capability_route_target.as_deref()
            == Some(entry.target_selector.as_str());
        let enabled = entry.enabled;
        let vendor_id = entry.vendor_id.clone();
        let target_alias = entry.target_alias.clone();
        let drag_handle = container(
            column![
                text("⋮⋮")
                    .size(11)
                    .wrapping(iced::advanced::text::Wrapping::None),
                text("⋮⋮")
                    .size(11)
                    .wrapping(iced::advanced::text::Wrapping::None),
            ]
            .spacing(1)
            .align_x(Center),
        )
        .center_x(14)
        .center_y(Fill);
        let remove_message = Message::RemoveAiCapabilityRouteTarget {
            capability_kind,
            target_selector: entry.target_selector.clone(),
        };
        let toggle = checkbox(enabled)
            .size(16)
            .spacing(0)
            .on_toggle(
                move |next_enabled| Message::AiCapabilityTargetEnabledToggled {
                    capability_kind,
                    vendor_id: vendor_id.clone(),
                    target_alias: target_alias.clone(),
                    enabled: next_enabled,
                },
            );
        let detail_color = self
            .app_theme
            .extended_palette()
            .background
            .base
            .text
            .scale_alpha(if enabled { 0.62 } else { 0.38 });
        let trailing_action: Element<'_, Message> = if show_remove_action {
            button(text("移除").size(12))
                .padding([3, 10])
                .style(|theme, status| {
                    style::workbench_label_action_button_style(theme, status, true)
                })
                .on_press(remove_message)
                .into()
        } else {
            container(space()).width(0).into()
        };

        mouse_area(
            container(
                row![
                    drag_handle,
                    toggle,
                    container(text(format!("{:02}", index + 1)).size(13))
                        .width(24)
                        .center_y(Fill),
                    container(text("·").size(13)).width(10).center_y(Fill),
                    text(entry.display_label())
                        .size(15)
                        .color(detail_color)
                        .wrapping(iced::advanced::text::Wrapping::None)
                        .width(Fill),
                    trailing_action,
                ]
                .spacing(8)
                .align_y(Center),
            )
            .padding([7, 10])
            .width(Fill)
            .height(AI_CAPABILITY_ROUTE_TARGET_ROW_HEIGHT)
            .style(move |theme| {
                style::workbench_ai_capability_route_row_style(theme, is_drag_source)
            }),
        )
        .interaction(if self.dragging_ai_capability_route_target.is_some() {
            iced::mouse::Interaction::Move
        } else {
            iced::mouse::Interaction::Grab
        })
        .on_press(Message::StartAiCapabilityRouteDrag {
            capability_kind,
            target_selector: entry.target_selector.clone(),
        })
        .on_move({
            let target_selector = entry.target_selector.clone();
            move |position| Message::AiCapabilityRouteDropPreviewMoved {
                capability_kind,
                target_selector: target_selector.clone(),
                position,
            }
        })
        .into()
    }
    /// Builds the tail drop zone shown after one enabled AI capability route list so drag reordering can land at the end.
    /// 构建显示在已启用 AI 能力路由列表末尾的尾部落点，以便拖拽重排落到最后。
    pub(super) fn view_ai_capability_route_tail_drop_zone(
        &self,
        capability_kind: AiToolKind,
    ) -> Element<'_, Message> {
        let Some(pending_drag) = self.pending_ai_capability_route_drag.as_ref() else {
            return space::vertical().height(0.0).into();
        };
        if pending_drag.capability_kind != capability_kind
            || self.dragging_ai_capability_route_target.is_none()
        {
            return space::vertical().height(0.0).into();
        }
        let shows_drop_tail = matches!(
            self.active_ai_capability_route_drop_target.as_ref(),
            Some(AiCapabilityRouteDropTarget::End)
        );
        mouse_area(
            container(if shows_drop_tail {
                container(space())
                    .width(Fill)
                    .height(2.0)
                    .style(style::workbench_pending_prompt_drop_indicator_style)
            } else {
                container(space()).width(Fill).height(0.0)
            })
            .padding(iced::Padding {
                top: 2.0,
                right: 2.0,
                bottom: 0.0,
                left: 2.0,
            })
            .height(AI_CAPABILITY_ROUTE_DROP_INDICATOR_HEIGHT)
            .width(Fill),
        )
        .interaction(iced::mouse::Interaction::Move)
        .on_enter(Message::AiCapabilityRouteTailHovered {
            capability_kind,
            is_hovered: true,
        })
        .on_exit(Message::AiCapabilityRouteTailHovered {
            capability_kind,
            is_hovered: false,
        })
        .into()
    }
    /// Renders one placeholder page for AI capability sections that are not wired yet.
    /// 渲染尚未接通实现的 AI 能力分区占位页面。
    pub(super) fn view_ai_capability_unavailable_page(
        &self,
        title: &'static str,
        description: &'static str,
    ) -> Element<'_, Message> {
        container(
            column![
                text(title).size(18),
                text(description).size(12),
                text(
                    "当前可先在这里预留入口；后续接通供应商与解析链后，再开放真实目标配置与排序。"
                )
                .size(12)
                .color(
                    self.app_theme
                        .extended_palette()
                        .background
                        .base
                        .text
                        .scale_alpha(0.62),
                ),
            ]
            .spacing(10),
        )
        .padding(16)
        .style(container::bordered_box)
        .into()
    }
    /// Builds the shared routing strategy block shown above AI capability target lists when that capability supports native-versus-host routing.
    /// 构建显示在 AI 能力目标列表上方的共享路由策略区块，供支持原生与宿主路由的能力复用。
    pub(super) fn view_ai_capability_routing_strategy_block(
        &self,
        capability_kind: AiToolKind,
    ) -> Option<Element<'_, Message>> {
        match capability_kind {
            AiToolKind::WebSearch => Some(
                column![
                    text("路由策略").size(13),
                    pick_list(
                        WebSearchRoutingMode::ALL.to_vec(),
                        Some(self.ai_tool_configuration.web_search.mode),
                        Message::WebSearchRoutingModeSelected,
                    )
                    .width(Fill)
                    .placeholder("选择联网搜索路由策略"),
                    text(self.ai_tool_configuration.web_search.mode.description()).size(12),
                ]
                .spacing(8)
                .into(),
            ),
            AiToolKind::UnderstandImage => Some(
                column![
                    text("路由策略").size(13),
                    pick_list(
                        NativeBridgeRoutingMode::ALL.to_vec(),
                        Some(self.ai_tool_configuration.understand_image.mode),
                        Message::UnderstandImageRoutingModeSelected,
                    )
                    .width(Fill)
                    .placeholder("选择图像理解路由策略"),
                    text(
                        self.ai_tool_configuration
                            .understand_image
                            .mode
                            .description()
                    )
                    .size(12),
                ]
                .spacing(8)
                .into(),
            ),
            _ => None,
        }
    }
    /// Builds the shared capability-management surface reused by the settings page and the workbench quick capability modal.
    /// 构建由设置页与工作台快捷能力弹窗复用的共享能力管理表面。
    pub(super) fn view_ai_capability_management_panel(
        &self,
        capability_kind: AiToolKind,
        show_configure_button: bool,
        show_remove_action: bool,
        show_runtime_toggle: bool,
    ) -> Element<'_, Message> {
        let available_vendor_ids = self
            .ai_tool_configuration
            .vendors
            .iter()
            .filter(|vendor| {
                provider_config::ai_capability_vendor_target_choices(&vendor.vendor_id)
                    .into_iter()
                    .any(|choice| choice.capability_kind == capability_kind)
            })
            .map(|vendor| vendor.vendor_id.clone())
            .collect::<Vec<_>>();
        let configured_targets = self
            .ai_tool_configuration
            .resolved_configured_targets_for_kind(capability_kind);
        let mut content = column![].spacing(12).width(Fill).height(Fill);
        if show_configure_button && show_runtime_toggle {
            content = content.push(
                row![
                    space::horizontal(),
                    button("配置接口")
                        .style(button::primary)
                        .on_press(Message::OpenAiCapabilityTargetPicker(capability_kind)),
                ]
                .align_y(Center),
            );
        }
        if show_runtime_toggle
            && let Some(strategy_block) =
                self.view_ai_capability_routing_strategy_block(capability_kind)
        {
            content = content.push(strategy_block);
        }
        if available_vendor_ids.is_empty() {
            content = content.push(
                container(
                    column![
                        text("当前还没有可用于这个能力的共享供应商。").size(13),
                        button("前往能力供应商").style(button::secondary).on_press(
                            Message::SettingsSectionSelected(SettingsSection::AiCapabilityVendors,)
                        ),
                    ]
                    .spacing(8),
                )
                .padding(12)
                .style(container::bordered_box),
            );
        } else if configured_targets.is_empty() {
            let empty_text = if show_configure_button {
                "当前还没有配置任何接口。点击“配置接口”后，可在弹层中按供应商勾选要启用的模型或功能。"
            } else {
                "当前还没有已配置接口。请前往设置页中的对应能力分页，先完成接口配置。"
            };
            content = content.push(
                container(text(empty_text).size(13))
                    .padding(12)
                    .style(container::bordered_box),
            );
        } else {
            let target_list = configured_targets.into_iter().enumerate().fold(
                column![].spacing(0).width(Fill),
                |list, (index, entry)| {
                    let shows_drop_before = matches!(
                        self.active_ai_capability_route_drop_target.as_ref(),
                        Some(AiCapabilityRouteDropTarget::Before(target_selector))
                            if target_selector == &entry.target_selector
                    );
                    list.push(self.view_ai_capability_route_drop_indicator(shows_drop_before))
                        .push(self.view_ai_capability_route_row(
                            capability_kind,
                            index,
                            entry,
                            show_remove_action,
                        ))
                },
            );
            content = content.push(
                scrollable(
                    target_list.push(self.view_ai_capability_route_tail_drop_zone(capability_kind)),
                )
                .height(Fill),
            );
        }

        let panel = if show_runtime_toggle {
            container(content).padding(iced::Padding {
                top: 0.0,
                right: 16.0,
                bottom: 16.0,
                left: 16.0,
            })
        } else {
            container(content).padding(16)
        };
        panel.width(Fill).height(Fill).into()
    }
    /// Renders one dedicated AI capability page that manages configured targets and their execution order for a single semantic tool.
    /// 渲染管理单个语义工具已配置目标与执行顺序的独立 AI 能力页面。
    pub(super) fn view_ai_capability_settings_page(
        &self,
        capability_kind: AiToolKind,
    ) -> Element<'_, Message> {
        self.view_ai_capability_management_panel(capability_kind, true, true, false)
    }
    /// Builds the blocking AI capability vendor-picker modal used by the supplier page before one new shared vendor is created.
    /// 构建供应商页在创建新的共享 AI 能力供应商前使用的阻塞式供应商选择弹窗。
    pub(super) fn view_ai_capability_vendor_picker_modal(&self) -> Option<Element<'_, Message>> {
        let picker = self.ai_capability_vendor_picker.as_ref()?;
        let palette = self.app_theme.extended_palette();
        let configured_vendor_ids = self
            .ai_tool_configuration
            .vendors
            .iter()
            .map(|vendor| vendor.vendor_id.clone())
            .collect::<Vec<_>>();
        let filter = picker.filter.trim().to_ascii_lowercase();
        let available_vendor_ids =
            crate::ai_capability_provider_catalog::supported_ai_capability_vendor_ids()
                .into_iter()
                .filter(|vendor_id| {
                    !configured_vendor_ids
                        .iter()
                        .any(|existing| existing.eq_ignore_ascii_case(vendor_id))
                })
                .filter(|vendor_id| {
                    if filter.is_empty() {
                        return true;
                    }
                    let display_name =
                        provider_config::ai_capability_vendor_display_name(vendor_id)
                            .to_ascii_lowercase();
                    let capability_summary =
                        provider_config::ai_capability_vendor_target_choices(vendor_id)
                            .into_iter()
                            .map(|choice| choice.capability_kind.label().to_ascii_lowercase())
                            .collect::<BTreeSet<_>>()
                            .into_iter()
                            .collect::<Vec<_>>()
                            .join(" ");
                    vendor_id.to_ascii_lowercase().contains(&filter)
                        || display_name.contains(&filter)
                        || capability_summary.contains(&filter)
                })
                .collect::<Vec<_>>();
        let close_button = button(
            container(
                codicon::chrome_close()
                    .size(14)
                    .line_height(1.0)
                    .color(palette.background.base.text.scale_alpha(0.72)),
            )
            .width(Fill)
            .height(Fill)
            .center(Fill),
        )
        .width(30)
        .height(30)
        .padding(0)
        .style(style::workbench_model_trigger_button_style)
        .on_press(Message::CloseAiCapabilityVendorPicker);
        let vendor_list: Element<'_, Message> = if available_vendor_ids.is_empty() {
            container(text("当前没有可新增的供应商，或者过滤后没有匹配结果。").size(13))
                .width(Fill)
                .height(320)
                .center(Fill)
                .into()
        } else {
            scrollable(available_vendor_ids.into_iter().fold(
                column![].spacing(8),
                |list, vendor_id| {
                    let display_name =
                        provider_config::ai_capability_vendor_display_name(&vendor_id);
                    let capability_summary =
                        provider_config::ai_capability_vendor_target_choices(&vendor_id)
                            .into_iter()
                            .map(|choice| choice.capability_kind.label())
                            .collect::<BTreeSet<_>>()
                            .into_iter()
                            .collect::<Vec<_>>()
                            .join("、");
                    list.push(
                        button(
                            column![
                                text(display_name).size(14),
                                text(format!(
                                    "{} · 支持：{}",
                                    vendor_id,
                                    if capability_summary.is_empty() {
                                        "暂无".to_string()
                                    } else {
                                        capability_summary
                                    }
                                ))
                                .size(12)
                                .color(
                                    self.app_theme
                                        .extended_palette()
                                        .background
                                        .base
                                        .text
                                        .scale_alpha(0.62),
                                ),
                            ]
                            .spacing(4),
                        )
                        .width(Fill)
                        .padding([10, 12])
                        .style(button::secondary)
                        .on_press(Message::AiCapabilityVendorPickerVendorSelected(vendor_id)),
                    )
                },
            ))
            .height(320)
            .into()
        };
        let modal_card = container(
            column![
                row![
                    text("添加共享供应商").size(20),
                    space::horizontal(),
                    close_button,
                ]
                .align_y(Center),
                text("输入关键字过滤供应商。一个供应商只能配置一次，选择后会进入独立配置弹窗。")
                    .size(12),
                text_input("搜索供应商或能力关键字", &picker.filter)
                    .on_input(Message::AiCapabilityVendorPickerFilterChanged)
                    .padding(10)
                    .width(Fill),
                vendor_list,
            ]
            .spacing(14),
        )
        .padding([22, 24])
        .width(720)
        .style(style::startup_overlay_card_style);
        let backdrop = mouse_area(container(space()).width(Fill).height(Fill))
            .on_press(Message::CloseAiCapabilityVendorPicker);

        Some(
            opaque(
                stack([
                    container(backdrop)
                        .width(Fill)
                        .height(Fill)
                        .style(style::startup_overlay_backdrop_style)
                        .into(),
                    container(mouse_area(modal_card).on_press(Message::Noop))
                        .width(Fill)
                        .height(Fill)
                        .center(Fill)
                        .into(),
                ])
                .width(Fill)
                .height(Fill),
            )
            .into(),
        )
    }
    /// Builds the blocking AI capability vendor-editor modal used to edit one shared supplier credential entry.
    /// 构建用于编辑单条共享供应商凭据的阻塞式 AI 能力供应商编辑弹窗。
    pub(super) fn view_ai_capability_vendor_editor_modal(&self) -> Option<Element<'_, Message>> {
        let editor = self.ai_capability_vendor_editor.as_ref()?;
        let palette = self.app_theme.extended_palette();
        let definitions =
            crate::ai_capability_provider_catalog::ai_capability_provider_definitions_for_vendor(
                &editor.vendor_id,
            );
        let supports_connection_mode = editor.vendor_id.eq_ignore_ascii_case("ollama");
        let supports_api_key_field = definitions.iter().any(|definition| {
            !matches!(
                definition.api_key_requirement,
                crate::ai_capability_provider_catalog::AiCapabilityProviderApiKeyRequirement::None
            )
        });
        let supports_region_field = definitions
            .iter()
            .any(|definition| definition.supports_region_field)
            || !editor.region_options.is_empty();
        let api_key_label = definitions
            .iter()
            .find_map(|definition| {
                (!definition.api_key_label.trim().is_empty())
                    .then_some(definition.api_key_label.as_str())
            })
            .unwrap_or("API Key / Token");
        let api_key_placeholder = definitions
            .iter()
            .find_map(|definition| {
                (!definition.api_key_placeholder.trim().is_empty())
                    .then_some(definition.api_key_placeholder.as_str())
            })
            .unwrap_or("请输入 API Key / Token");
        let region_label = definitions
            .iter()
            .find_map(|definition| {
                (!definition.region_label.trim().is_empty())
                    .then_some(definition.region_label.as_str())
            })
            .unwrap_or("区域");
        let region_placeholder = definitions
            .iter()
            .find_map(|definition| {
                (!definition.region_placeholder.trim().is_empty())
                    .then_some(definition.region_placeholder.as_str())
            })
            .unwrap_or("请输入区域");
        let configuration_note = definitions
            .iter()
            .filter_map(|definition| definition.configuration_note.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join("\n");
        let editor_description = match (supports_api_key_field, supports_region_field) {
            (true, true) => {
                "这里只配置共享凭据。供应商名称固定，Base URL 不对外开放，后续各能力分页直接复用这里的 API Key 与区域。"
            }
            (true, false) => {
                "这里只配置共享凭据。供应商名称固定，Base URL 不对外开放，后续各能力分页直接复用这里的 API Key。"
            }
            (false, true) => {
                "这里只配置共享区域。供应商名称固定，Base URL 不对外开放，后续各能力分页直接复用这里的区域设置。"
            }
            (false, false) => {
                "当前供应商无需额外凭据或区域配置，确认后即可加入共享能力供应商列表。"
            }
        };
        let local_base_url_placeholder = "例如：http://127.0.0.1:11434 或 http://nasbox:11434";
        let close_button = button(
            container(
                codicon::chrome_close()
                    .size(14)
                    .line_height(1.0)
                    .color(palette.background.base.text.scale_alpha(0.72)),
            )
            .width(Fill)
            .height(Fill)
            .center(Fill),
        )
        .width(30)
        .height(30)
        .padding(0)
        .style(style::workbench_model_trigger_button_style)
        .on_press(Message::CloseAiCapabilityVendorEditor);
        let region_field: Element<'_, Message> = if !supports_region_field {
            space().into()
        } else if editor.region_options.is_empty() {
            column![
                text(region_label).size(12),
                text_input(region_placeholder, &editor.region)
                    .on_input(Message::AiCapabilityVendorEditorRegionChanged)
                    .padding(8)
                    .width(Fill),
            ]
            .spacing(5)
            .into()
        } else {
            let selected_region = editor
                .region_options
                .iter()
                .find(|option| option.eq_ignore_ascii_case(editor.region.trim()))
                .cloned();
            column![
                text(region_label).size(12),
                pick_list(
                    editor.region_options.clone(),
                    selected_region,
                    Message::AiCapabilityVendorEditorRegionChanged,
                )
                .width(Fill)
                .placeholder("选择区域"),
            ]
            .spacing(5)
            .into()
        };
        let note_block: Element<'_, Message> = if configuration_note.trim().is_empty() {
            space().into()
        } else {
            container(text(configuration_note).size(12))
                .padding(10)
                .style(container::bordered_box)
                .into()
        };
        let api_key_field: Element<'_, Message> = if !supports_api_key_field {
            space().into()
        } else {
            column![
                text(api_key_label).size(12),
                text_input(api_key_placeholder, &editor.api_key)
                    .on_input(Message::AiCapabilityVendorEditorApiKeyChanged)
                    .secure(true)
                    .padding(8)
                    .width(Fill),
            ]
            .spacing(5)
            .into()
        };
        let connection_mode_field: Element<'_, Message> = if !supports_connection_mode {
            space().into()
        } else {
            column![
                text("连接方式").size(12),
                pick_list(
                    crate::provider_config::AiCapabilityVendorConnectionMode::ALL.to_vec(),
                    Some(editor.connection_mode),
                    Message::AiCapabilityVendorEditorConnectionModeChanged,
                )
                .width(Fill)
                .placeholder("选择连接方式"),
            ]
            .spacing(5)
            .into()
        };
        let local_base_url_field: Element<'_, Message> = if !supports_connection_mode
            || !matches!(
                editor.connection_mode,
                crate::provider_config::AiCapabilityVendorConnectionMode::Local
            ) {
            space().into()
        } else {
            column![
                text("本地 Ollama 地址").size(12),
                text_input(local_base_url_placeholder, &editor.base_url)
                    .on_input(Message::AiCapabilityVendorEditorBaseUrlChanged)
                    .padding(8)
                    .width(Fill),
                text("仅本地模式可编辑；留空时默认使用 http://127.0.0.1:11434。").size(12),
            ]
            .spacing(5)
            .into()
        };
        let modal_card = container(
            column![
                row![
                    text(if editor.is_new {
                        format!("新增 {}", editor.display_name)
                    } else {
                        format!("编辑 {}", editor.display_name)
                    })
                    .size(20),
                    space::horizontal(),
                    close_button,
                ]
                .align_y(Center),
                text(editor_description).size(12),
                note_block,
                connection_mode_field,
                api_key_field,
                local_base_url_field,
                region_field,
                row![
                    space::horizontal(),
                    button("取消")
                        .style(button::secondary)
                        .on_press(Message::CloseAiCapabilityVendorEditor),
                    button("保存")
                        .style(button::primary)
                        .on_press(Message::SaveAiCapabilityVendorEditor),
                ]
                .align_y(Center)
                .spacing(10),
            ]
            .spacing(14),
        )
        .padding([22, 24])
        .width(680)
        .style(style::startup_overlay_card_style);
        let backdrop = mouse_area(container(space()).width(Fill).height(Fill))
            .on_press(Message::CloseAiCapabilityVendorEditor);

        Some(
            opaque(
                stack([
                    container(backdrop)
                        .width(Fill)
                        .height(Fill)
                        .style(style::startup_overlay_backdrop_style)
                        .into(),
                    container(mouse_area(modal_card).on_press(Message::Noop))
                        .width(Fill)
                        .height(Fill)
                        .center(Fill)
                        .into(),
                ])
                .width(Fill)
                .height(Fill),
            )
            .into(),
        )
    }
    /// Builds the blocking routing-strategy picker modal used by settings pages for routed AI capabilities.
    /// 构建供设置页为支持路由策略的 AI 能力选择模式的阻塞式路由策略弹窗。
    pub(super) fn view_ai_capability_routing_picker_modal(&self) -> Option<Element<'_, Message>> {
        let picker = self.ai_capability_routing_picker.as_ref()?;
        let palette = self.app_theme.extended_palette();
        let close_button = button(
            container(
                codicon::chrome_close()
                    .size(14)
                    .line_height(1.0)
                    .color(palette.background.base.text.scale_alpha(0.72)),
            )
            .width(Fill)
            .height(Fill)
            .center(Fill),
        )
        .width(30)
        .height(30)
        .padding(0)
        .style(style::workbench_model_trigger_button_style)
        .on_press(Message::CloseAiCapabilityRoutingPicker);
        let (title, summary, option_list): (&str, &str, Element<'_, Message>) = match picker
            .capability_kind
        {
            AiToolKind::WebSearch => (
                "选择联网搜索路由策略",
                "当前能力会按这里选定的顺序，决定优先使用供应商原生搜索还是共享第三方搜索接口。",
                column(WebSearchRoutingMode::ALL.into_iter().map(|mode| {
                    let is_selected = self.ai_tool_configuration.web_search.mode == mode;
                    button(
                        container(
                            column![
                                text(mode.label()).size(13).width(Fill),
                                text(mode.description())
                                    .size(11)
                                    .color(palette.background.base.text.scale_alpha(0.62))
                                    .width(Fill),
                            ]
                            .spacing(4),
                        )
                        .width(Fill)
                        .center_y(Fill),
                    )
                    .width(Fill)
                    .padding([8, 10])
                    .style(move |theme, status| {
                        style::workbench_model_picker_option_button_style(
                            theme,
                            status,
                            is_selected,
                        )
                    })
                    .on_press(Message::WebSearchRoutingModeSelected(mode))
                    .into()
                }))
                .spacing(6)
                .into(),
            ),
            AiToolKind::UnderstandImage => (
                "选择图像理解路由策略",
                "当前能力会按这里选定的顺序，决定优先使用模型原生视觉，还是优先走宿主理解桥接工具。",
                column(NativeBridgeRoutingMode::ALL.into_iter().map(|mode| {
                    let is_selected = self.ai_tool_configuration.understand_image.mode == mode;
                    button(
                        container(
                            column![
                                text(mode.label()).size(13).width(Fill),
                                text(mode.description())
                                    .size(11)
                                    .color(palette.background.base.text.scale_alpha(0.62))
                                    .width(Fill),
                            ]
                            .spacing(4),
                        )
                        .width(Fill)
                        .center_y(Fill),
                    )
                    .width(Fill)
                    .padding([8, 10])
                    .style(move |theme, status| {
                        style::workbench_model_picker_option_button_style(
                            theme,
                            status,
                            is_selected,
                        )
                    })
                    .on_press(Message::UnderstandImageRoutingModeSelected(mode))
                    .into()
                }))
                .spacing(6)
                .into(),
            ),
            _ => return None,
        };
        let modal_card = container(
            column![
                row![text(title).size(20), space::horizontal(), close_button].align_y(Center),
                text(summary)
                    .size(12)
                    .color(palette.background.base.text.scale_alpha(0.62)),
                scrollable(option_list).height(320),
            ]
            .spacing(14),
        )
        .padding([22, 24])
        .width(640)
        .style(style::startup_overlay_card_style);
        let backdrop = mouse_area(container(space()).width(Fill).height(Fill))
            .on_press(Message::CloseAiCapabilityRoutingPicker);
        Some(
            opaque(
                stack([
                    backdrop.into(),
                    container(mouse_area(modal_card).on_press(Message::Noop))
                        .width(Fill)
                        .height(Fill)
                        .center(Fill)
                        .into(),
                ])
                .width(Fill)
                .height(Fill),
            )
            .into(),
        )
    }

    /// Builds the blocking AI capability target-picker modal used by capability pages to choose targets from configured vendors.
    /// 构建供能力页从已配置供应商中选择目标的阻塞式 AI 能力目标选择弹窗。
    pub(super) fn view_ai_capability_target_picker_modal(&self) -> Option<Element<'_, Message>> {
        let picker = self.ai_capability_target_picker.as_ref()?;
        let capability_kind = picker.capability_kind;
        let palette = self.app_theme.extended_palette();
        let vendor_ids = self
            .ai_tool_configuration
            .vendors
            .iter()
            .filter(|vendor| {
                provider_config::ai_capability_vendor_target_choices(&vendor.vendor_id)
                    .into_iter()
                    .any(|choice| choice.capability_kind == capability_kind)
            })
            .map(|vendor| vendor.vendor_id.clone())
            .collect::<Vec<_>>();
        let selected_vendor_id = picker
            .selected_vendor_id
            .clone()
            .or_else(|| vendor_ids.first().cloned());
        let close_button = button(
            container(
                codicon::chrome_close()
                    .size(14)
                    .line_height(1.0)
                    .color(palette.background.base.text.scale_alpha(0.72)),
            )
            .width(Fill)
            .height(Fill)
            .center(Fill),
        )
        .width(30)
        .height(30)
        .padding(0)
        .style(style::workbench_model_trigger_button_style)
        .on_press(Message::CloseAiCapabilityTargetPicker);
        let vendor_list = scrollable(vendor_ids.iter().fold(
            column![].spacing(1).width(Fill),
            |list, vendor_id| {
                let vendor = self.ai_tool_configuration.vendor_by_id(vendor_id);
                let vendor_name = vendor
                    .map(|vendor| vendor.name.clone())
                    .unwrap_or_else(|| vendor_id.clone());
                let selected_count = picker
                    .selections
                    .get(vendor_id)
                    .map(BTreeMap::len)
                    .unwrap_or(0);
                let is_selected = selected_vendor_id.as_deref() == Some(vendor_id.as_str());
                let trailing_badge: Element<'_, Message> = text(format!("[{}]", selected_count))
                    .size(12)
                    .color(
                        self.app_theme
                            .extended_palette()
                            .background
                            .base
                            .text
                            .scale_alpha(0.56),
                    )
                    .into();
                list.push(self.view_tree_navigation_button_row(
                    self.view_settings_navigation_icon(SettingsSection::AiCapabilityVendors, true),
                    vendor_name,
                    None,
                    Some(trailing_badge),
                    is_selected,
                    MAIN_TASK_TREE_SECTION_LEFT_PADDING,
                    Message::AiCapabilityTargetPickerVendorSelected(vendor_id.clone()),
                ))
            },
        ))
        .height(280);
        let right_panel: Element<'_, Message> = if let Some(vendor_id) = selected_vendor_id {
            let target_choices = provider_config::ai_capability_vendor_target_choices(&vendor_id)
                .into_iter()
                .filter(|choice| choice.capability_kind == capability_kind)
                .collect::<Vec<_>>();
            let selected_targets = picker.selections.get(&vendor_id);
            let target_list = if target_choices.is_empty() {
                column![text("当前供应商没有可用接口。").size(13)].spacing(8)
            } else {
                target_choices
                    .into_iter()
                    .fold(column![].spacing(10), |list, choice| {
                        let is_checked = selected_targets
                            .and_then(|targets| targets.get(&choice.target_alias))
                            .is_some();
                        let current_model = selected_targets
                            .and_then(|targets| targets.get(&choice.target_alias))
                            .cloned()
                            .unwrap_or_else(|| choice.default_model_id.clone());
                        let vendor_id_for_toggle = vendor_id.clone();
                        let vendor_id_for_model = vendor_id.clone();
                        let target_alias_for_toggle = choice.target_alias.clone();
                        let target_alias_for_model = choice.target_alias.clone();
                        let model_editor: Element<'_, Message> =
                            if choice.supports_model_override && is_checked {
                                column![
                                    text("请求模型").size(12),
                                    text_input("模型 ID", &current_model)
                                        .on_input(move |value| {
                                            Message::AiCapabilityTargetPickerTargetModelChanged {
                                                vendor_id: vendor_id_for_model.clone(),
                                                target_alias: target_alias_for_model.clone(),
                                                model_id: value,
                                            }
                                        })
                                        .padding(8)
                                        .width(Fill),
                                ]
                                .spacing(5)
                                .into()
                            } else {
                                space::vertical().height(0).into()
                            };
                        list.push(
                            container(
                                column![
                                    checkbox(is_checked)
                                        .label(choice.label.clone())
                                        .on_toggle(move |enabled| {
                                            Message::AiCapabilityTargetPickerTargetToggled {
                                                vendor_id: vendor_id_for_toggle.clone(),
                                                target_alias: target_alias_for_toggle.clone(),
                                                enabled,
                                            }
                                        })
                                        .size(13),
                                    model_editor,
                                ]
                                .spacing(6),
                            )
                            .padding(10)
                            .style(container::bordered_box),
                        )
                    })
            };
            scrollable(column![target_list].spacing(10))
                .height(280)
                .into()
        } else {
            container(text("当前没有可供选择的供应商。").size(13))
                .width(Fill)
                .height(280)
                .center(Fill)
                .into()
        };
        let modal_card = container(
            column![
                row![
                    text(format!("配置接口 · {}", capability_kind.label())).size(20),
                    space::horizontal(),
                    text(format!("已勾选 {} 项", picker.total_selected_count())).size(12),
                    close_button,
                ]
                .align_y(Center),
                text("左侧选择供应商，右侧勾选要启用的接口。已启用接口会自动勾选，点击保存后会立即写回当前能力页并自动持久化。")
                    .size(12),
                row![
                    container(column![text("供应商").size(13), vendor_list].spacing(8))
                        .width(FillPortion(2)),
                    container(column![text("接口列表").size(13), right_panel].spacing(8))
                        .width(FillPortion(5)),
                ]
                .spacing(12)
                .height(320),
                row![
                    text("保存后会同步更新当前能力页的已启用接口、顺序，并立即写入独立 AI 能力配置。").size(12),
                    space::horizontal(),
                    button("保存")
                        .style(button::primary)
                        .on_press(Message::SaveAiCapabilityTargetPicker),
                ]
                .align_y(Center),
            ]
            .spacing(14),
        )
        .padding([22, 24])
        .width(860)
        .style(style::startup_overlay_card_style);
        let backdrop = mouse_area(container(space()).width(Fill).height(Fill))
            .on_press(Message::CloseAiCapabilityTargetPicker);

        Some(
            opaque(
                stack([
                    container(backdrop)
                        .width(Fill)
                        .height(Fill)
                        .style(style::startup_overlay_backdrop_style)
                        .into(),
                    container(mouse_area(modal_card).on_press(Message::Noop))
                        .width(Fill)
                        .height(Fill)
                        .center(Fill)
                        .into(),
                ])
                .width(Fill)
                .height(Fill),
            )
            .into(),
        )
    }
}
