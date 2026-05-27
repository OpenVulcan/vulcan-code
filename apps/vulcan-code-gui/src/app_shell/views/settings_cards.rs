//! Settings page configuration cards and editor modal view composition.
//! 设置页配置卡片与编辑弹层的视图组合逻辑。

use super::*;

impl VulcanWorkbench {
    /// Renders the standalone provider-configuration card used by the dedicated settings workspace.
    /// 渲染独立设置工作区使用的供应商配置卡片。
    pub(super) fn view_provider_configuration_card(&self) -> Element<'_, Message> {
        let configured_providers = self.configured_provider_choices();
        let configured_provider_list: Element<'_, Message> = if configured_providers.is_empty() {
            container(
                iced::widget::column![
                    text("还没有模型供应商。").size(13),
                    text("先点击右上角“新增供应商”完成接入配置。保存后会进入“调整模型列表”草稿页，默认勾选供应商声明的推荐模型，其余模型需要你显式保存后才会生效。").size(12),
                ]
                .spacing(6),
            )
            .padding(12)
            .style(container::bordered_box)
            .into()
        } else {
            configured_providers
                .into_iter()
                .fold(iced::widget::column![].spacing(10), |list, provider| {
                    let provider_id = provider.id.clone();
                    let provider_name = provider.name.clone();
                    let definition = provider_config::supported_provider_by_id(&provider_id);
                    let entry = self.provider_configuration.provider.get(&provider_id);
                    let catalog_models = provider_config::catalog_model_choices(
                        &provider_id,
                        &self.provider_configuration,
                    );
                    let endpoint_summary = definition
                        .and_then(|definition| {
                            entry.and_then(|entry| definition.resolved_base_url(&entry.options))
                        })
                        .unwrap_or_else(|| String::from("由运行时自动解析"));
                    let api_key_summary = if entry
                        .map(|entry| {
                            entry
                                .options
                                .api_key
                                .as_deref()
                                .is_some_and(|api_key| !api_key.trim().is_empty())
                        })
                        .unwrap_or(false)
                    {
                        String::from("API Key：已填写")
                    } else {
                        String::from("API Key：未填写")
                    };
                    let enabled_model_count = provider_config::effective_model_choices(
                        &provider_id,
                        &self.provider_configuration,
                    )
                    .len();
                    let source_summary = definition
                        .map(|_| {
                            format!(
                                "{} · 内置目录 {} 个 · 当前启用 {} 个",
                                api_key_summary,
                                catalog_models.len(),
                                enabled_model_count
                            )
                        })
                        .unwrap_or_else(|| String::from("未找到供应商定义"));

                    let mut action_row = row![
                        button("编辑")
                            .padding([4, 10])
                            .style(|theme, status| {
                                style::workbench_label_action_button_style(theme, status, false)
                            })
                            .on_press(Message::OpenProviderEditorForEdit(provider_id.clone())),
                        button("调整模型列表")
                            .padding([4, 10])
                            .style(|theme, status| {
                                style::workbench_label_action_button_style(theme, status, false)
                            })
                            .on_press(Message::OpenProviderModelPicker(provider_id.clone())),
                    ]
                    .spacing(10);
                    action_row = action_row.push(
                        button("删除")
                            .padding([4, 10])
                            .style(|theme, status| {
                                style::workbench_label_action_button_style(theme, status, true)
                            })
                            .on_press(Message::RemoveConfiguredProvider(provider_id.clone())),
                    );

                    list.push(
                        container(
                            iced::widget::column![
                                row![
                                    iced::widget::column![
                                        text(provider_name).size(16),
                                        text(format!(
                                            "{source_summary} · 接口地址：{endpoint_summary}"
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
                                    action_row,
                                ]
                                .align_y(Center)
                                .spacing(10),
                            ]
                            .spacing(10),
                        )
                        .padding(12)
                        .style(container::bordered_box),
                    )
                })
                .into()
        };

        scrollable(container(configured_provider_list).width(Fill))
            .height(Fill)
            .spacing(8)
            .width(Fill)
            .into()
    }

    /// Renders the standalone enabled-model management page grouped by provider.
    /// 渲染按供应商分组展示的独立已启用模型管理页面。
    pub(super) fn view_provider_model_list_card(&self) -> Element<'_, Message> {
        let configured_providers = self.configured_provider_choices();
        let Some(active_provider) = self.selected_model_list_provider_choice() else {
            return iced::widget::column![text("当前还没有可展示的供应商。请先在“模型供应商”里新增至少一个供应商，然后再配置启用模型。").size(12)]
                .spacing(12)
                .width(Fill)
                .height(Fill)
                .into();
        };

        let provider_list = scrollable(configured_providers.into_iter().fold(
            iced::widget::column![].spacing(1).width(Fill),
            |list, provider| {
                let enabled_count = provider_config::effective_model_choices(
                    &provider.id,
                    &self.provider_configuration,
                )
                .len();
                let trailing_badge: Element<'_, Message> = text(format!("[{}]", enabled_count))
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
                    self.view_settings_navigation_icon(SettingsSection::ModelProviders, true),
                    provider.name.clone(),
                    None,
                    Some(trailing_badge),
                    active_provider.id == provider.id,
                    MAIN_TASK_TREE_SECTION_LEFT_PADDING,
                    Message::ProviderCatalogSelected(provider),
                ))
            },
        ))
        .height(Fill);

        let enabled_models = provider_config::effective_model_choices(
            &active_provider.id,
            &self.provider_configuration,
        );
        let enabled_model_list: Element<'_, Message> = if enabled_models.is_empty() {
            container(
                iced::widget::column![
                    text("当前供应商还没有启用任何模型。").size(13),
                    text(
                        "点击右上角“调整模型列表”进入完整目录草稿页，勾选后点击保存才会真正生效。"
                    )
                    .size(12),
                ]
                .spacing(6),
            )
            .width(Fill)
            .height(Fill)
            .center(Fill)
            .into()
        } else {
            scrollable(enabled_models.into_iter().fold(
                iced::widget::column![].spacing(10),
                |list, model| {
                    let modality_summary = if model.input_modalities.is_empty() {
                        String::from("text")
                    } else {
                        model.input_modalities.join(" / ")
                    };
                    let summary = format!(
                        "{} · 上下文 {} · 输出 {}",
                        modality_summary,
                        model
                            .context_limit
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| String::from("-")),
                        model
                            .output_limit
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| String::from("-")),
                    );

                    list.push(
                        container(
                            row![
                                iced::widget::column![
                                    text(model.name.clone()).size(13),
                                    text(format!("{} · {}", model.id, summary)).size(11).color(
                                        self.app_theme
                                            .extended_palette()
                                            .background
                                            .base
                                            .text
                                            .scale_alpha(0.58),
                                    ),
                                ]
                                .spacing(4)
                                .width(Fill),
                                button("编辑能力").style(button::secondary).on_press(
                                    Message::OpenProviderModelOverrideEditor {
                                        provider_id: active_provider.id.clone(),
                                        model_id: model.id.clone(),
                                    }
                                ),
                            ]
                            .align_y(Center)
                            .spacing(10),
                        )
                        .padding(10)
                        .style(container::bordered_box),
                    )
                },
            ))
            .height(Fill)
            .into()
        };

        row![
            iced::widget::column![text("供应商").size(13), provider_list]
                .spacing(8)
                .width(FillPortion(2))
                .height(Fill),
            iced::widget::column![
                row![
                    text("已启用模型").size(13),
                    space::horizontal(),
                    text(provider_config::provider_summary(&active_provider.id)).size(12),
                ]
                .align_y(Center),
                enabled_model_list,
            ]
            .spacing(10)
            .width(FillPortion(5))
            .height(Fill),
        ]
        .spacing(16)
        .width(Fill)
        .height(Fill)
        .into()
    }

    /// Renders the blocking provider editor modal used for creating or updating one provider entry.
    /// 渲染用于创建或更新单条供应商配置的阻塞式供应商编辑弹窗。
    pub(super) fn view_provider_editor_modal(&self) -> Option<Element<'_, Message>> {
        let editor = self.provider_editor.as_ref()?;
        let palette = self.app_theme.extended_palette();
        let provider_choices = provider_config::supported_provider_choices();
        let selected_definition = self.selected_provider_definition();
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
        .on_press(Message::CloseProviderEditor);
        let provider_picker: Element<'_, Message> = if editor.is_new {
            pick_list(
                provider_choices,
                Some(self.selected_provider.clone()),
                Message::ProviderCatalogSelected,
            )
            .width(Fill)
            .placeholder("选择供应商")
            .into()
        } else {
            container(text(self.selected_provider.name.clone()).size(14))
                .padding(10)
                .style(container::bordered_box)
                .width(Fill)
                .into()
        };
        let provider_metadata: Element<'_, Message> = selected_definition.map_or_else(
            || text("当前没有可用的供应商定义。").size(12).into(),
            |definition| {
                iced::widget::column![
                    text(format!("运行时：{}", definition.runtime.label())).size(12),
                    text(format!(
                        "接口族：{} · {}",
                        definition.runtime.api_family().label(),
                        definition.runtime.api_family().key()
                    ))
                    .size(12),
                    text(format!(
                        "接口地址：{}",
                        self.selected_provider_endpoint_summary()
                    ))
                    .size(12),
                    text(format!(
                        "模型目录：{}",
                        self.selected_provider_model_source_summary()
                    ))
                    .size(12),
                ]
                .spacing(4)
                .into()
            },
        );
        let provider_field_list = selected_definition.map_or_else(
            || iced::widget::column![text("当前没有可用的供应商定义。").size(13)].spacing(8),
            |definition| {
                definition.input_fields().into_iter().fold(
                    iced::widget::column![].spacing(10),
                    |form, field| {
                        let label = definition.field_label(field);
                        let placeholder = definition.field_placeholder(field);
                        let input = text_input(&placeholder, self.provider_form.get(field))
                            .on_input(move |value| Message::ProviderFieldChanged(field, value))
                            .padding(10)
                            .width(Fill);
                        let input = if field.is_secret() {
                            input.secure(true)
                        } else {
                            input
                        };
                        form.push(iced::widget::column![text(label).size(13), input].spacing(6))
                    },
                )
            },
        );
        let modal_card =
            container(
                iced::widget::column![
                row![
                    text(if editor.is_new {
                        "新增模型供应商"
                    } else {
                        "编辑模型供应商"
                    })
                    .size(20),
                    space::horizontal(),
                    close_button,
                ]
                .align_y(Center),
                text("这里只保存供应商接入配置；启用模型与停用模型请进入“调整模型列表”弹窗后显式保存。")
                    .size(12),
                text("供应商").size(13),
                provider_picker,
                provider_metadata,
                provider_field_list,
                row![
                    text(if editor.is_new {
                        "保存后会打开“调整模型列表”弹窗，并按该供应商内置目录里的默认勾选初始化草稿。"
                    } else {
                        "保存只更新当前供应商接入配置，不会自动改动已启用模型集合。"
                    })
                    .size(12),
                    space::horizontal(),
                    button("保存")
                        .style(button::primary)
                        .on_press(Message::SaveProviderConfiguration),
                ]
                .align_y(Center),
            ]
                .spacing(14),
            )
            .padding([22, 24])
            .width(760)
            .style(style::startup_overlay_card_style);
        let backdrop = mouse_area(container(space()).width(Fill).height(Fill))
            .on_press(Message::CloseProviderEditor);

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

    /// Renders the blocking provider model-picker modal used to edit one provider's enabled-model draft before explicit save.
    /// 渲染用于在显式保存前编辑某个供应商启用模型草稿的阻塞式供应商模型配置弹窗。
    pub(super) fn view_provider_model_picker_modal(&self) -> Option<Element<'_, Message>> {
        let picker = self.provider_model_picker.as_ref()?;
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
        .on_press(Message::CloseProviderModelPicker);
        let current_draft_enabled_ids = picker
            .draft_enabled_model_ids_by_provider
            .get(&picker.selected_provider_id)
            .cloned()
            .unwrap_or_default();
        let total_draft_enabled_count = picker
            .draft_enabled_model_ids_by_provider
            .values()
            .map(BTreeSet::len)
            .sum::<usize>();
        let provider_list = scrollable(
            self.provider_model_picker_provider_choices()
                .into_iter()
                .fold(
                    iced::widget::column![].spacing(1).width(Fill),
                    |list, provider| {
                        let enabled_count = picker
                            .draft_enabled_model_ids_by_provider
                            .get(&provider.id)
                            .map(BTreeSet::len)
                            .unwrap_or_else(|| {
                                provider_config::effective_model_choices(
                                    &provider.id,
                                    &self.provider_configuration,
                                )
                                .len()
                            });
                        let trailing_badge: Element<'_, Message> =
                            text(format!("[{}]", enabled_count))
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
                            self.view_settings_navigation_icon(
                                SettingsSection::ModelProviders,
                                true,
                            ),
                            provider.name.clone(),
                            None,
                            Some(trailing_badge),
                            picker.selected_provider_id == provider.id,
                            MAIN_TASK_TREE_SECTION_LEFT_PADDING,
                            Message::ProviderModelPickerProviderSelected(provider.id),
                        ))
                    },
                ),
        )
        .height(320);
        let filtered_catalog = self.filtered_provider_model_picker_catalog();
        let catalog_list: Element<'_, Message> = if filtered_catalog.is_empty() {
            container(text("当前过滤条件下没有匹配模型。").size(13))
                .width(Fill)
                .height(260)
                .center(Fill)
                .into()
        } else {
            scrollable(filtered_catalog.into_iter().fold(
                iced::widget::column![].spacing(10),
                |list, model| {
                    let model_id = model.id.clone();
                    let provider_id = picker.selected_provider_id.clone();
                    let checked = current_draft_enabled_ids.contains(&model_id);
                    let mut subtitle_parts = Vec::new();
                    if let Some(family) = model.family.as_deref() {
                        subtitle_parts.push(family.to_string());
                    }
                    if !model.input_modalities.is_empty() {
                        subtitle_parts.push(model.input_modalities.join(" / "));
                    } else {
                        subtitle_parts.push(String::from("text"));
                    }
                    if let Some(context_limit) = model.context_limit {
                        subtitle_parts.push(format!("上下文 {context_limit}"));
                    }
                    if model.default_enabled {
                        subtitle_parts.push(String::from("默认勾选"));
                    }
                    list.push(
                        container(
                            iced::widget::column![
                                row![
                                    checkbox(checked)
                                        .label(model.name.clone())
                                        .on_toggle(move |enabled| {
                                            Message::ProviderModelPickerModelToggled {
                                                provider_id: provider_id.clone(),
                                                model_id: model_id.clone(),
                                                enabled,
                                            }
                                        })
                                        .size(13)
                                        .width(Fill),
                                    button("编辑能力").style(button::secondary).on_press(
                                        Message::OpenProviderModelOverrideEditor {
                                            provider_id: picker.selected_provider_id.clone(),
                                            model_id: model.id.clone(),
                                        }
                                    ),
                                ]
                                .align_y(Center)
                                .spacing(10),
                                text(format!("{} · {}", model.id, subtitle_parts.join(" · ")))
                                    .size(11)
                                    .color(
                                        self.app_theme
                                            .extended_palette()
                                            .background
                                            .base
                                            .text
                                            .scale_alpha(0.56),
                                    ),
                            ]
                            .spacing(6),
                        )
                        .padding(10)
                        .style(container::bordered_box),
                    )
                },
            ))
            .height(260)
            .into()
        };
        let modal_card = container(
            iced::widget::column![
                row![
                    text("配置启用模型").size(20),
                    space::horizontal(),
                    text(format!("当前草稿共勾选 {} 项", total_draft_enabled_count)).size(12),
                    close_button,
                ]
                .align_y(Center),
                text("左侧切换供应商，右侧编辑供应商内置完整模型目录。全选、取消等操作都只写入草稿，只有点击保存才会真正落盘。")
                    .size(12),
                row![
                    container(iced::widget::column![text("供应商").size(13), provider_list].spacing(8))
                        .width(FillPortion(2)),
                    container(
                        iced::widget::column![
                            row![
                                text("模型目录").size(13),
                                space::horizontal(),
                                text_input("输入模型名称或 ID 过滤", &picker.filter)
                                    .on_input(Message::ProviderModelPickerFilterChanged)
                                    .padding(8)
                                    .width(260),
                            ]
                            .align_y(Center),
                            row![
                                button("全选当前结果")
                                    .style(button::secondary)
                                    .on_press(Message::ProviderModelPickerSelectAllVisible),
                                button("取消当前结果")
                                    .style(button::secondary)
                                    .on_press(Message::ProviderModelPickerClearAllVisible),
                            ]
                            .spacing(10),
                            catalog_list,
                        ]
                        .spacing(10),
                    )
                    .width(FillPortion(5)),
                ]
                .spacing(12)
                .height(360),
                row![
                    text("取消会丢弃当前草稿，不会污染已保存结果。").size(12),
                    space::horizontal(),
                    button("保存")
                        .style(button::primary)
                        .on_press(Message::SaveProviderModelPicker),
                ]
                .align_y(Center),
            ]
            .spacing(14),
        )
        .padding([22, 24])
        .width(920)
        .style(style::startup_overlay_card_style);
        let backdrop = mouse_area(container(space()).width(Fill).height(Fill))
            .on_press(Message::CloseProviderModelPicker);

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

    /// Renders the blocking per-model metadata editor used to correct one enabled model's capability declaration.
    /// 渲染用于修正单个已启用模型能力声明的阻塞式逐模型元数据编辑弹窗。
    pub(super) fn view_provider_model_override_editor_modal(&self) -> Option<Element<'_, Message>> {
        let editor = self.provider_model_override_editor.as_ref()?;
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
        .on_press(Message::CloseProviderModelOverrideEditor);
        let modality_toggles = iced::widget::column![
            checkbox(editor.supports_text)
                .label("文本输入")
                .on_toggle(
                    |enabled| Message::ProviderModelOverrideInputModalityToggled {
                        modality: String::from("text"),
                        enabled,
                    }
                ),
            checkbox(editor.supports_image)
                .label("图片输入")
                .on_toggle(
                    |enabled| Message::ProviderModelOverrideInputModalityToggled {
                        modality: String::from("image"),
                        enabled,
                    }
                ),
            checkbox(editor.supports_audio)
                .label("音频输入")
                .on_toggle(
                    |enabled| Message::ProviderModelOverrideInputModalityToggled {
                        modality: String::from("audio"),
                        enabled,
                    }
                ),
            checkbox(editor.supports_video)
                .label("视频输入")
                .on_toggle(
                    |enabled| Message::ProviderModelOverrideInputModalityToggled {
                        modality: String::from("video"),
                        enabled,
                    }
                ),
            checkbox(editor.supports_pdf)
                .label("PDF / 文件输入")
                .on_toggle(
                    |enabled| Message::ProviderModelOverrideInputModalityToggled {
                        modality: String::from("pdf"),
                        enabled,
                    }
                ),
        ]
        .spacing(8);
        let modal_card = container(
            iced::widget::column![
                row![
                    text("编辑模型能力").size(20),
                    space::horizontal(),
                    close_button,
                ]
                .align_y(Center),
                text(format!(
                    "{} · {}",
                    provider_config::provider_summary(&editor.provider_id),
                    editor.model_name
                ))
                .size(12),
                row![
                    checkbox(editor.reasoning)
                        .label("声明支持思考 / 推理")
                        .on_toggle(Message::ProviderModelOverrideReasoningChanged),
                    tooltip(
                        checkbox(editor.attachment)
                            .label("声明附件标记")
                            .on_toggle(Message::ProviderModelOverrideAttachmentChanged),
                        "用于记录目录级附件声明，主要服务于诊断摘要与元数据对齐；真正决定图片、音频、视频、PDF 能否直接进入模型输入的是下方“输入能力”开关。",
                        tooltip::Position::Bottom,
                    ),
                ]
                .spacing(18),
                container(iced::widget::column![text("输入能力").size(13), modality_toggles].spacing(8))
                    .padding(12)
                    .style(container::bordered_box),
                row![
                    iced::widget::column![
                        row![
                            text("上下文长度").size(12),
                            tooltip(
                                container(
                                    text("?").size(11).color(
                                        self.app_theme
                                            .extended_palette()
                                            .background
                                            .base
                                            .text
                                            .scale_alpha(0.56),
                                    ),
                                )
                                .padding([0, 4]),
                                "表示模型声明的总上下文窗口上限，通常覆盖系统提示词、历史消息、当前输入以及部分运行时预留空间。",
                                tooltip::Position::Bottom,
                            ),
                        ]
                        .spacing(4)
                        .align_y(Center),
                        text_input("例如 131072", &editor.context_limit_text)
                            .on_input(Message::ProviderModelOverrideContextLimitChanged)
                            .padding(8)
                            .width(Fill),
                    ]
                    .spacing(6)
                    .width(Fill),
                    iced::widget::column![
                        row![
                            text("输入长度").size(12),
                            tooltip(
                                container(
                                    text("?").size(11).color(
                                        self.app_theme
                                            .extended_palette()
                                            .background
                                            .base
                                            .text
                                            .scale_alpha(0.56),
                                    ),
                                )
                                .padding([0, 4]),
                                "表示宿主优先采用的输入预算上限，会直接影响上下文占用、预警与压缩判断；如果留空，则回退到“上下文长度”。它不一定等于“上下文长度 - 输出长度”，因为有些供应商会单独限制最大输入预算。",
                                tooltip::Position::Bottom,
                            ),
                        ]
                        .spacing(4)
                        .align_y(Center),
                        text_input("例如 65536", &editor.input_limit_text)
                            .on_input(Message::ProviderModelOverrideInputLimitChanged)
                            .padding(8)
                            .width(Fill),
                    ]
                    .spacing(6)
                    .width(Fill),
                    iced::widget::column![
                        text("输出长度").size(12),
                        text_input("例如 8192", &editor.output_limit_text)
                            .on_input(Message::ProviderModelOverrideOutputLimitChanged)
                            .padding(8)
                            .width(Fill),
                    ]
                    .spacing(6)
                    .width(Fill),
                ]
                .spacing(10),
                iced::widget::column![
                    text("思考模式附加配置（可选 JSON）").size(12),
                    container(
                        text_editor(&editor.reasoning_profile_buffer)
                            .height(140)
                            .padding(10)
                            .size(13)
                            .line_height(Pixels(18.0))
                            .font(theme::code_font())
                            .wrapping(iced::widget::text::Wrapping::None)
                            .on_action(Message::ProviderModelOverrideReasoningProfileAction),
                    )
                    .style(container::bordered_box)
                    .width(Fill),
                    text("支持多行 JSON；留空则不写入额外思考配置。")
                        .size(11)
                        .color(
                            self.app_theme
                                .extended_palette()
                                .background
                                .base
                                .text
                                .scale_alpha(0.56),
                        ),
                ]
                .spacing(6),
                row![
                    text("修改只会写入该模型的用户覆盖配置；点击保存前不会影响正式结果。").size(12),
                    space::horizontal(),
                    button("保存")
                        .style(button::primary)
                        .on_press(Message::SaveProviderModelOverrideEditor),
                ]
                .align_y(Center),
            ]
            .spacing(14),
        )
        .padding([22, 24])
        .width(760)
        .style(style::startup_overlay_card_style);
        let backdrop = mouse_area(container(space()).width(Fill).height(Fill))
            .on_press(Message::CloseProviderModelOverrideEditor);

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

    /// Renders the runtime selector card used by both the provider page and the runtime diagnostics page.
    /// 渲染供供应商页与运行诊断页复用的运行参数选择卡片。
    pub(super) fn view_runtime_selector_settings_card(&self) -> Element<'_, Message> {
        let temperature_control: Element<'_, Message> = if self.temperature_enabled {
            slider(0.0..=1.0, self.temperature, Message::TemperatureChanged)
                .step(0.05)
                .into()
        } else {
            text(self.t(I18nKey::SettingsTemperatureDisabledNotice))
                .size(12)
                .into()
        };

        container(
            iced::widget::column![
                text(self.t(I18nKey::SettingsRuntimeParametersTitle)).size(18),
                text(self.t(I18nKey::SettingsRuntimeParametersDescription)).size(12),
                text(self.t(I18nKey::CommonLanguageLabel)).size(12),
                pick_list(
                    self.language_manager.available_locales(),
                    Some(self.current_locale()),
                    Message::LocaleSelected
                )
                .width(Fill)
                .placeholder(self.t(I18nKey::SettingsLanguagePlaceholder)),
                text(self.t(I18nKey::SettingsSessionTranscriptTurnPageSizeLabel)).size(12),
                pick_list(
                    SESSION_TRANSCRIPT_TURN_PAGE_SIZE_OPTIONS,
                    Some(self.session_transcript_turn_page_size),
                    Message::SessionTranscriptTurnPageSizeSelected
                )
                .width(Fill)
                .placeholder(self.t(I18nKey::SettingsSessionTranscriptTurnPageSizePlaceholder)),
                text(match self.current_locale() {
                    AppLocale::ZhCn => "默认命令 Shell",
                    AppLocale::EnUs => "Default Command Shell",
                })
                .size(12),
                pick_list(
                    self.command_shell_picker_choices(),
                    self.selected_command_shell_picker_choice(),
                    Message::CommandShellPreferenceSelected
                )
                .width(Fill)
                .placeholder(match self.current_locale() {
                    AppLocale::ZhCn => "选择默认命令 Shell",
                    AppLocale::EnUs => "Choose a default command shell",
                }),
                text(self.command_shell_support_summary()).size(12),
                text(self.t(I18nKey::CommonThemeLabel)).size(12),
                pick_list(
                    Theme::ALL,
                    Some(self.app_theme.clone()),
                    Message::ThemeSelected
                )
                .width(Fill)
                .placeholder(self.t(I18nKey::SettingsThemePlaceholder)),
                checkbox(self.temperature_enabled)
                    .label(self.t(I18nKey::SettingsTemperatureToggle))
                    .on_toggle(Message::TemperatureEnabledToggled),
                text(if self.temperature_enabled {
                    self.tf(
                        I18nKey::SettingsTemperatureValueEnabled,
                        &[I18nArg::named(
                            "temperature",
                            format!("{:.2}", self.temperature),
                        )],
                    )
                } else {
                    self.t(I18nKey::SettingsTemperatureValueDefault)
                }),
                temperature_control,
            ]
            .spacing(12),
        )
        .padding(16)
        .style(container::bordered_box)
        .into()
    }

    /// Renders the prompt-composer settings card.
    /// 渲染提示词组合设置卡片。
    pub(super) fn view_prompt_composer_settings_card(&self) -> Element<'_, Message> {
        let prompt_config = &self.provider_configuration.prompt;
        let project_prompt_path = prompt_config.project_prompt_path.clone();
        let personality_summary = prompt_config.personality.label();
        let project_draft = &self.project_prompt_settings_draft;
        let project_config_path = self
            .current_project_prompt_workspace_path()
            .map(|workspace| provider_config::project_configuration_file_path(&workspace))
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| String::from("当前没有可用工作区"));
        let project_prompt_full_path = if project_draft.workspace_path.trim().is_empty()
            || project_draft.path.trim().is_empty()
        {
            String::from("当前没有可用项目提示词文件")
        } else {
            std::path::PathBuf::from(&project_draft.workspace_path)
                .join(project_draft.path.trim().trim_start_matches(['/', '\\']))
                .display()
                .to_string()
        };
        let override_summary = if project_draft.has_project_override {
            "当前项目：已写入覆盖配置"
        } else {
            "当前项目：未写入覆盖配置，使用全局默认"
        };

        container(
            iced::widget::column![
                text("提示词组合").size(18),
                text(
                    "这里控制实际请求前的提示词组合方式；人格、项目级提示词会进入下一次 API 请求。"
                )
                .size(12),
                rule::horizontal(1),
                text("全局默认").size(14),
                text("人格").size(12),
                pick_list(
                    self.prompt_personality_picker_choices(),
                    self.selected_prompt_personality_choice(),
                    Message::PromptPersonalitySelected,
                )
                .width(Fill)
                .placeholder("选择人格"),
                text(format!("当前人格：{personality_summary}")).size(12),
                checkbox(prompt_config.project_prompt_enabled)
                    .label("全局默认启用项目级用户提示词")
                    .on_toggle(Message::ProjectPromptInjectionToggled),
                text(format!("项目提示词文件：{project_prompt_path}")).size(12),
                text(
                    "当项目没有自己的覆盖配置时，会使用这里的全局默认开关与路径。"
                )
                .size(12),
                rule::horizontal(1),
                text("当前项目覆盖").size(14),
                text(format!("工作区：{}", project_draft.workspace_path)).size(12),
                text(format!("配置文件：{project_config_path}")).size(12),
                text(override_summary).size(12),
                checkbox(project_draft.enabled)
                    .label("当前项目启用用户提示词")
                    .on_toggle(Message::ProjectPromptOverrideToggled),
                text_input("例如 .vulcan/code/prompt.md", &project_draft.path)
                    .on_input(Message::ProjectPromptPathChanged)
                    .width(Fill),
                text(format!("实际提示词文件：{project_prompt_full_path}")).size(12),
                row![
                    button(text("保存当前项目配置")).on_press(Message::SaveProjectPromptConfiguration),
                    button(text("重新载入项目配置")).on_press(Message::ReloadProjectPromptConfiguration),
                ]
                .spacing(8),
                text(&project_draft.notice).size(12),
                text(
                    "项目配置保存后会写入工作区 `.vulcan/code/config.json`，运行时只覆盖项目提示词开关与路径；人格仍使用全局设置。"
                )
                .size(12),
                rule::horizontal(1),
                text("自定义人格文件放在：~/.vulcan/code/prompt/personalities/{id}.md").size(12),
                text("工具帮助文件放在：~/.vulcan/code/prompt/tool-help/{tool_id}.md").size(12),
            ]
            .spacing(12),
        )
        .padding(16)
        .style(container::bordered_box)
        .into()
    }

    /// Renders the engineering policy control card.
    /// 渲染工程策略控制卡片。
    pub(super) fn view_policy_controls_card(&self) -> Element<'_, Message> {
        container(
            iced::widget::column![
                text(self.t(I18nKey::SettingsPolicyTitle)).size(18),
                checkbox(self.generate_plan)
                    .label(self.t(I18nKey::SettingsPolicyGeneratePlan))
                    .on_toggle(Message::GeneratePlanToggled),
                checkbox(self.run_cargo_check)
                    .label(self.t(I18nKey::SettingsPolicyRunCargoCheck))
                    .on_toggle(Message::RunCargoCheckToggled),
                checkbox(self.persist_memory)
                    .label(self.t(I18nKey::SettingsPolicyPersistMemory))
                    .on_toggle(Message::PersistMemoryToggled),
                toggler(self.auto_approve)
                    .label(self.t(I18nKey::SettingsPolicyAutoApprove))
                    .on_toggle(Message::AutoApproveToggled),
            ]
            .spacing(12),
        )
        .padding(16)
        .style(container::bordered_box)
        .into()
    }

    /// Renders the integration guide card shown in runtime diagnostics.
    /// 渲染显示在运行诊断页中的集成指引卡片。
    pub(super) fn view_integration_guide_card(&self) -> Element<'_, Message> {
        container(
            iced::widget::column![
                text(self.t(I18nKey::SettingsIntegrationGuideTitle)).size(18),
                self.view_workbench_markdown_body(
                    Some(&self.quick_guide_markdown_buffer),
                    self.localized_quick_guide_markdown(),
                    Message::QuickGuideMarkdownAction,
                ),
            ]
            .spacing(12),
        )
        .padding(16)
        .style(container::bordered_box)
        .into()
    }

    /// Renders the activity-log card shown in runtime diagnostics.
    /// 渲染显示在运行诊断页中的活动日志卡片。
    pub(super) fn view_activity_log_card(&self) -> Element<'_, Message> {
        container(
            self.activities.iter().enumerate().fold(
                iced::widget::column![
                    text(self.t(I18nKey::SettingsActivityLogTitle)).size(18),
                    text(self.t(I18nKey::SettingsActivityLogHint)).size(12),
                ]
                .spacing(8),
                |list, (index, entry)| list.push(self.view_activity_card(index, entry)),
            ),
        )
        .padding(16)
        .style(container::bordered_box)
        .into()
    }

    /// Renders the runtime diagnostics page with execution status, policy controls, and engineering breadcrumbs.
    /// 渲染包含执行状态、策略控制与工程轨迹的运行诊断页面。
    pub(super) fn view_inspector(&self) -> Element<'_, Message> {
        iced::widget::column![
            self.view_runtime_selector_settings_card(),
            self.view_runtime_execution_card(),
            self.view_prompt_composer_inspector_card(),
            self.view_policy_controls_card(),
            self.view_integration_guide_card(),
            self.view_activity_log_card(),
        ]
        .spacing(14)
        .into()
    }

    /// Renders the latest prompt-composer diagnostic report.
    /// 渲染最近一次提示词组合诊断报告。
    pub(super) fn view_prompt_composer_inspector_card(&self) -> Element<'_, Message> {
        let palette = self.app_theme.extended_palette();
        let Some(report) =
            crate::agent_runtime::prompt_composer_runtime::latest_runtime_prompt_composer_report()
        else {
            return container(
                iced::widget::column![
                    text("提示词组合检查器").size(18),
                    text("尚未捕获提示词组合报告。发送一次消息后，这里会显示系统提示词、人格式样、项目规则、工具说明和运行时上下文的组合结果。")
                        .size(12)
                        .color(palette.background.base.text.scale_alpha(0.62)),
                ]
                .spacing(8),
            )
            .padding(16)
            .style(container::bordered_box)
            .into();
        };

        let cache_summary = format!(
            "节点命中 {} / 未命中 {} · 组合命中 {} / 未命中 {}",
            report.cache_report.node_hits,
            report.cache_report.node_misses,
            report.cache_report.bundle_hits,
            report.cache_report.bundle_misses
        );
        let total_estimated_tokens = report
            .layer_summaries
            .iter()
            .map(|layer| layer.estimated_tokens)
            .sum::<usize>();
        let layer_summary_list = report.layer_summaries.iter().fold(
            iced::widget::column![].spacing(6),
            |column, layer| {
                column.push(
                    text(format!(
                        "• {} · {} chars · ~{} tokens · {} 节点 · cache hit {} / miss {} · fp {}",
                        layer.title,
                        layer.character_count,
                        layer.estimated_tokens,
                        layer.segment_count,
                        layer.cache_hits,
                        layer.cache_misses,
                        prompt_composer_inspector_fingerprint(layer.fingerprint)
                    ))
                    .size(12),
                )
            },
        );
        let cache_event_list = report.cache_events.iter().take(8).fold(
            iced::widget::column![].spacing(4),
            |column, event| {
                column.push(
                    text(format!(
                        "• {} · {} · {}",
                        event.scope,
                        if event.hit { "hit" } else { "miss" },
                        event.cache_key
                    ))
                    .size(12)
                    .color(palette.background.base.text.scale_alpha(0.62)),
                )
            },
        );
        let hidden_cache_event_count = report.cache_events.len().saturating_sub(8);
        let cache_event_list = if hidden_cache_event_count > 0 {
            cache_event_list.push(
                text(format!(
                    "另有 {hidden_cache_event_count} 条缓存事件，复制完整报告可查看。"
                ))
                .size(12)
                .color(palette.background.base.text.scale_alpha(0.56)),
            )
        } else if report.cache_events.is_empty() {
            cache_event_list.push(
                text("暂无缓存事件。")
                    .size(12)
                    .color(palette.background.base.text.scale_alpha(0.56)),
            )
        } else {
            cache_event_list
        };
        let segment_list = report.segments.iter().take(12).fold(
            iced::widget::column![].spacing(6),
            |column, segment| {
                column.push(
                    text(format!(
                        "• {} · {} · {} · {} chars · ~{} tokens · cache={} · source={} · {}",
                        segment.node_id,
                        segment.target_label,
                        segment.title,
                        segment.character_count,
                        segment.estimated_tokens,
                        if segment.from_cache { "hit" } else { "miss" },
                        segment.source_label,
                        segment.preview
                    ))
                    .size(12),
                )
            },
        );
        let hidden_count = report.segments.len().saturating_sub(12);
        let segment_list = if hidden_count > 0 {
            segment_list.push(
                text(format!("另有 {hidden_count} 个片段，详见下方完整报告。"))
                    .size(12)
                    .color(palette.background.base.text.scale_alpha(0.56)),
            )
        } else {
            segment_list
        };
        let layer_cards = PromptComposerInspectorLayer::ALL.iter().copied().fold(
            iced::widget::column![].spacing(8),
            |column, layer| {
                column.push(self.view_prompt_composer_inspector_layer_card(layer, &report))
            },
        );

        container(
            iced::widget::column![
                text("提示词组合检查器").size(18),
                row![
                    button(text("复制完整报告").size(12))
                        .padding([6, 10])
                        .style(button::secondary)
                        .on_press(Message::CopyPromptComposerInspectorReport),
                ]
                .spacing(8),
                text(format!(
                    "会话：{} · 模式：{} · 模型：{} · 策略：{} · 人格：{} · 装配指纹：{}",
                    report.session_title,
                    report.mode_label,
                    report.model_id,
                    report.strategy_id,
                    report.personality_id.as_deref().unwrap_or("none"),
                    prompt_composer_inspector_fingerprint(report.assembly_fingerprint)
                ))
                .size(12)
                .color(palette.background.base.text.scale_alpha(0.68)),
                text(format!("估算提示词总量：~{total_estimated_tokens} tokens"))
                    .size(12)
                    .color(palette.background.base.text.scale_alpha(0.68)),
                text(cache_summary)
                    .size(12)
                    .color(palette.background.base.text.scale_alpha(0.68)),
                container(layer_summary_list)
                    .padding(10)
                    .width(Fill)
                    .style(container::bordered_box),
                container(cache_event_list)
                    .padding(10)
                    .width(Fill)
                    .style(container::bordered_box),
                container(segment_list)
                    .padding(10)
                    .width(Fill)
                    .style(container::bordered_box),
                layer_cards,
            ]
            .spacing(10),
        )
        .padding(16)
        .style(container::bordered_box)
        .into()
    }

    /// Renders one prompt-composer inspector layer with fold and copy controls.
    /// 渲染一个带折叠与复制控制的提示词组合检查器分层。
    fn view_prompt_composer_inspector_layer_card(
        &self,
        layer: PromptComposerInspectorLayer,
        report: &crate::agent_runtime::prompt_composer_runtime::RuntimePromptComposerReport,
    ) -> Element<'_, Message> {
        let palette = self.app_theme.extended_palette();
        let layer_text = self.prompt_composer_inspector_layer_text(layer);
        let layer_report = report
            .layer_summaries
            .iter()
            .find(|summary| summary.target_label == layer.target_label());
        let is_expanded = self
            .prompt_composer_inspector_expanded_layers
            .contains(&layer);
        let layer_buffer = self.prompt_composer_inspector_layer_buffers.get(&layer);
        let layer_summary = layer_report
            .map(|summary| {
                format!(
                    "{} chars · ~{} tokens · {} 个片段 · cache hit {} / miss {} · fp {}",
                    summary.character_count,
                    summary.estimated_tokens,
                    summary.segment_count,
                    summary.cache_hits,
                    summary.cache_misses,
                    prompt_composer_inspector_fingerprint(summary.fingerprint)
                )
            })
            .unwrap_or_else(|| format!("{} chars · 暂无分层诊断", layer_text.chars().count()));
        let mut content = iced::widget::column![
            row![
                text(layer.label()).size(14),
                space::horizontal(),
                button(text(if is_expanded { "收起" } else { "展开" }).size(12))
                    .padding([5, 9])
                    .style(button::secondary)
                    .on_press(Message::TogglePromptComposerInspectorLayer(layer)),
                button(text("复制本层").size(12))
                    .padding([5, 9])
                    .style(button::secondary)
                    .on_press(Message::CopyPromptComposerInspectorLayer(layer)),
            ]
            .spacing(8)
            .align_y(Center),
            text(layer_summary)
                .size(12)
                .color(palette.background.base.text.scale_alpha(0.62)),
        ]
        .spacing(8);

        if is_expanded {
            content = content
                .push(self.view_prompt_composer_inspector_layer_segment_details(layer, report));
            content = content.push(
                scrollable(
                    container(self.view_workbench_code_preview_body(
                        layer_buffer,
                        prompt_composer_inspector_display_text(layer_text),
                        "text",
                        move |action| Message::PromptComposerInspectorLayerAction(layer, action),
                    ))
                    .padding(8)
                    .width(Fill),
                )
                .height(220)
                .width(Fill),
            );
        } else {
            content = content.push(
                text(prompt_composer_inspector_layer_preview(layer_text))
                    .size(12)
                    .color(palette.background.base.text.scale_alpha(0.58)),
            );
        }

        container(content)
            .padding(10)
            .width(Fill)
            .style(container::bordered_box)
            .into()
    }

    /// Renders node-level details for one prompt-composer inspector layer.
    /// 渲染单个提示词组合检查器分层的节点级明细。
    fn view_prompt_composer_inspector_layer_segment_details(
        &self,
        layer: PromptComposerInspectorLayer,
        report: &crate::agent_runtime::prompt_composer_runtime::RuntimePromptComposerReport,
    ) -> Element<'_, Message> {
        let palette = self.app_theme.extended_palette();
        let layer_segments = report
            .segments
            .iter()
            .enumerate()
            .filter(|(_, segment)| segment.target_label == layer.target_label())
            .collect::<Vec<_>>();
        let detail_rows = if layer_segments.is_empty() {
            iced::widget::column![
                text("节点明细").size(13),
                text("本层暂无节点。")
                    .size(12)
                    .color(palette.background.base.text.scale_alpha(0.58)),
            ]
            .spacing(6)
        } else {
            layer_segments.iter().fold(
                iced::widget::column![
                    text(format!("节点明细 · {} 个节点", layer_segments.len())).size(13),
                    rule::horizontal(1),
                ]
                .spacing(6),
                |column, (segment_index, segment)| {
                    let segment_key = prompt_composer_inspector_segment_key(
                        *segment_index,
                        &segment.target_label,
                        &segment.node_id,
                    );
                    let is_segment_expanded = self
                        .prompt_composer_inspector_expanded_segments
                        .contains(&segment_key);
                    let segment_buffer = self
                        .prompt_composer_inspector_segment_buffers
                        .get(&segment_key);
                    let mut segment_content = iced::widget::column![
                        row![
                            text(format!("`{}`", segment.node_id)).size(12),
                            text(format!("· {}", segment.title))
                                .size(12)
                                .color(palette.background.base.text.scale_alpha(0.76)),
                            space::horizontal(),
                            text(if segment.from_cache {
                                "cache hit"
                            } else {
                                "cache miss"
                            })
                            .size(11)
                            .color(if segment.from_cache {
                                palette.success.base.color
                            } else {
                                palette.warning.base.color
                            }),
                            button(
                                text(if is_segment_expanded {
                                    "收起"
                                } else {
                                    "展开"
                                })
                                .size(11)
                            )
                            .padding([4, 8])
                            .style(button::secondary)
                            .on_press(
                                Message::TogglePromptComposerInspectorSegment(segment_key.clone(),)
                            ),
                            button(text("复制节点").size(11))
                                .padding([4, 8])
                                .style(button::secondary)
                                .on_press(Message::CopyPromptComposerInspectorSegment(
                                    segment_key.clone(),
                                )),
                        ]
                        .spacing(6)
                        .align_y(Center),
                        text(format!(
                            "{} · {} chars · ~{} tokens · key {} · {}",
                            segment.kind_label,
                            segment.character_count,
                            segment.estimated_tokens,
                            segment.cache_key,
                            segment.preview
                        ))
                        .size(12)
                        .color(palette.background.base.text.scale_alpha(0.58)),
                        text(format!("来源：{}", segment.source_label))
                            .size(12)
                            .color(palette.background.base.text.scale_alpha(0.58)),
                    ]
                    .spacing(4);

                    if is_segment_expanded {
                        segment_content = segment_content.push(
                            scrollable(
                                container(self.view_workbench_code_preview_body(
                                    segment_buffer,
                                    prompt_composer_inspector_display_text(&segment.body),
                                    "text",
                                    {
                                        let segment_key = segment_key.clone();
                                        move |action| {
                                            Message::PromptComposerInspectorSegmentAction(
                                                segment_key.clone(),
                                                action,
                                            )
                                        }
                                    },
                                ))
                                .padding(6)
                                .width(Fill),
                            )
                            .height(180)
                            .width(Fill),
                        );
                    }

                    column.push(
                        container(segment_content)
                            .padding(8)
                            .width(Fill)
                            .style(container::bordered_box),
                    )
                },
            )
        };

        container(detail_rows)
            .padding(8)
            .width(Fill)
            .style(container::bordered_box)
            .into()
    }

    /// Renders the execution runtime card that exposes active-agent selection and the latest preparation snapshot.
    /// 渲染执行运行时卡片，以暴露激活代理选择与最近一次执行准备快照。
    pub(super) fn view_runtime_execution_card(&self) -> Element<'_, Message> {
        let summary = self.runtime_foundation.summary();
        let active_agent_choices = self.configured_agent_choices();
        let selected_active_agent = self.active_agent_choice();
        let preparation_rows = self
            .last_execution_preparation
            .as_ref()
            .map(|preparation| preparation.summary_rows())
            .unwrap_or_else(|| {
                vec![String::from(
                    "尚未触发执行，可点击工作台输入框右侧的发送按钮发起第一次 AI 调用。",
                )]
            });

        let preparation_list = preparation_rows
            .into_iter()
            .fold(iced::widget::column![].spacing(6), |column, row_text| {
                column.push(text(format!("• {row_text}")).size(12))
            });

        let active_agent_picker: Element<'_, Message> = if active_agent_choices.is_empty() {
            container(text("当前尚未声明任何代理定义。").size(13))
                .padding(10)
                .style(container::bordered_box)
                .into()
        } else {
            pick_list(
                active_agent_choices,
                selected_active_agent,
                Message::ActiveAgentSelected,
            )
            .width(Fill)
            .placeholder("选择激活代理")
            .into()
        };

        container(
            iced::widget::column![
                text("执行运行时").size(18),
                active_agent_picker,
                text(format!(
                    "当前激活代理：{} ({})",
                    summary.active_agent_name, summary.active_agent_id
                ))
                .size(12),
                text(format!(
                    "已构建代理 {} / 技能 {} / 原生工具 {} / 委派 {}",
                    summary.built_agent_count,
                    summary.skill_count,
                    summary.native_tool_count,
                    summary.delegate_tool_count
                ))
                .size(12),
                text(summary.token_budget_status).size(12),
                preparation_list,
            ]
            .spacing(12),
        )
        .padding(16)
        .style(container::bordered_box)
        .into()
    }

    /// Renders the MCP configuration page as a pure server list with compact edit and remove actions.
    /// 将 MCP 配置页渲染为纯服务器列表，并提供紧凑的编辑与移除动作。
    pub(super) fn view_mcp_configuration_card(&self) -> Element<'_, Message> {
        let palette = self.app_theme.extended_palette();
        let server_list: Element<'_, Message> = if self
            .provider_configuration
            .mcp
            .servers
            .is_empty()
        {
            container(
                iced::widget::column![
                    text("还没有 MCP。").size(13),
                    text("点击右上角“新增 MCP”添加一个自定义服务器。").size(12),
                ]
                .spacing(6),
            )
            .width(Fill)
            .height(Fill)
            .center(Fill)
            .into()
        } else {
            scrollable(self.provider_configuration.mcp.servers.iter().fold(
                iced::widget::column![].spacing(10),
                |list, (server_id, entry)| {
                    let transport_label = match entry.transport {
                        PersistedMcpTransportKind::Stdio => "STDIO",
                        PersistedMcpTransportKind::Http => "流式 HTTP",
                        PersistedMcpTransportKind::Sse => "兼容 SSE",
                    };
                    let status_label = if entry.enabled {
                        "已启用"
                    } else {
                        "已停用"
                    };
                    let endpoint_summary = match entry.transport {
                        PersistedMcpTransportKind::Stdio => {
                            let argument_suffix = if entry.args.is_empty() {
                                String::new()
                            } else {
                                format!(" {}", entry.args.join(" "))
                            };
                            format!("{}{}", entry.command, argument_suffix)
                        }
                        PersistedMcpTransportKind::Http | PersistedMcpTransportKind::Sse => entry
                            .url
                            .clone()
                            .unwrap_or_else(|| String::from("未配置 URL")),
                    };
                    let metadata_summary = format!(
                        "{status_label} · {transport_label}{}",
                        if entry.oauth.is_some()
                            && !matches!(entry.transport, PersistedMcpTransportKind::Stdio)
                        {
                            " · OAuth"
                        } else {
                            ""
                        }
                    );
                    let edit_choice = McpServerChoice {
                        id: server_id.clone(),
                        label: entry.name.clone(),
                    };

                    list.push(
                        container(
                            row![
                                iced::widget::column![
                                    text(&entry.name).size(16),
                                    text(metadata_summary)
                                        .size(12)
                                        .color(palette.background.base.text.scale_alpha(0.62)),
                                    text(endpoint_summary)
                                        .size(12)
                                        .color(palette.background.base.text.scale_alpha(0.52)),
                                ]
                                .spacing(6)
                                .width(Fill),
                                row![
                                    button("编辑")
                                        .padding([4, 10])
                                        .style(|theme, status| {
                                            style::workbench_label_action_button_style(
                                                theme, status, false,
                                            )
                                        })
                                        .on_press(Message::McpServerSelected(edit_choice)),
                                    button("删除")
                                        .padding([4, 10])
                                        .style(|theme, status| {
                                            style::workbench_label_action_button_style(
                                                theme, status, true,
                                            )
                                        })
                                        .on_press(Message::RequestRemoveMcpServerConfiguration(
                                            server_id.clone(),
                                        ),),
                                ]
                                .spacing(10),
                            ]
                            .align_y(Center)
                            .spacing(12),
                        )
                        .padding(12)
                        .style(container::bordered_box),
                    )
                },
            ))
            .height(Fill)
            .width(Fill)
            .into()
        };

        container(server_list).width(Fill).height(Fill).into()
    }

    /// Renders the blocking MCP editor overlay used for creating or updating one custom MCP server entry.
    /// 渲染用于创建或更新单条自定义 MCP 服务器配置的阻塞式编辑遮罩层。
    pub(super) fn view_mcp_server_editor_modal(&self) -> Option<Element<'_, Message>> {
        if !self.mcp_server_editor_open {
            return None;
        }

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
        .on_press(Message::CloseMcpServerEditor);
        let transport_selector = row(mcp_transport_choices()
            .into_iter()
            .map(|transport| {
                let label = match transport {
                    PersistedMcpTransportKind::Stdio => "STDIO",
                    PersistedMcpTransportKind::Http => "流式 HTTP",
                    PersistedMcpTransportKind::Sse => "兼容 SSE",
                };
                button(container(text(label).size(13)).width(Fill).center_x(Fill))
                    .width(Fill)
                    .padding([10, 12])
                    .style(move |theme, status| {
                        style::workbench_transcript_mode_button_style(
                            theme,
                            status,
                            self.mcp_server_form.transport == transport,
                        )
                    })
                    .on_press(Message::McpServerTransportSelected(transport))
                    .into()
            })
            .collect::<Vec<Element<'_, Message>>>())
        .spacing(8)
        .width(Fill);
        let build_row_remove_button = |enabled: bool, on_press: Message| -> Element<'_, Message> {
            let button = button(
                container(lucide::trash().size(14).line_height(1.0))
                    .width(Fill)
                    .height(Fill)
                    .center(Fill),
            )
            .width(32)
            .height(32)
            .padding(0)
            .style(style::workbench_model_trigger_button_style);
            if enabled {
                button.on_press(on_press).into()
            } else {
                button.into()
            }
        };
        let transport_fields: Element<'_, Message> = match self.mcp_server_form.transport {
            PersistedMcpTransportKind::Stdio => {
                let allow_remove_args = self.mcp_server_form.args_rows.len() > 1;
                let argument_rows = self.mcp_server_form.args_rows.iter().enumerate().fold(
                    iced::widget::column![].spacing(8),
                    |list, (index, row_state)| {
                        list.push(
                            row![
                                text_input("参数", &row_state.value)
                                    .on_input(move |value| Message::McpServerArgRowChanged(
                                        index, value
                                    ))
                                    .padding(10)
                                    .width(Fill),
                                build_row_remove_button(
                                    allow_remove_args,
                                    Message::RemoveMcpServerArgRow(index),
                                ),
                            ]
                            .spacing(8)
                            .align_y(Center),
                        )
                    },
                );
                let allow_remove_env_rows = self.mcp_server_form.env_rows.len() > 1;
                let environment_rows = self.mcp_server_form.env_rows.iter().enumerate().fold(
                    iced::widget::column![].spacing(8),
                    |list, (index, row_state)| {
                        list.push(
                            row![
                                text_input("键", &row_state.key)
                                    .on_input(move |value| Message::McpServerEnvKeyChanged(
                                        index, value
                                    ))
                                    .padding(10)
                                    .width(FillPortion(2)),
                                text_input("值", &row_state.value)
                                    .on_input(move |value| Message::McpServerEnvValueChanged(
                                        index, value
                                    ))
                                    .padding(10)
                                    .width(FillPortion(3)),
                                build_row_remove_button(
                                    allow_remove_env_rows,
                                    Message::RemoveMcpServerEnvRow(index),
                                ),
                            ]
                            .spacing(8)
                            .align_y(Center),
                        )
                    },
                );
                let allow_remove_passthrough_rows =
                    self.mcp_server_form.env_passthrough_rows.len() > 1;
                let passthrough_rows = self
                    .mcp_server_form
                    .env_passthrough_rows
                    .iter()
                    .enumerate()
                    .fold(
                        iced::widget::column![].spacing(8),
                        |list, (index, row_state)| {
                            list.push(
                                row![
                                    text_input("环境变量名，例如 TEST_TOKEN", &row_state.value)
                                        .on_input(move |value| {
                                            Message::McpServerEnvPassthroughChanged(index, value)
                                        })
                                        .padding(10)
                                        .width(Fill),
                                    build_row_remove_button(
                                        allow_remove_passthrough_rows,
                                        Message::RemoveMcpServerEnvPassthroughRow(index),
                                    ),
                                ]
                                .spacing(8)
                                .align_y(Center),
                            )
                        },
                    );

                iced::widget::column![
                    iced::widget::column![
                        text("启动命令").size(13),
                        text_input(
                            "例如 openai-dev-mcp serve-sqlite",
                            &self.mcp_server_form.command
                        )
                        .on_input(Message::McpServerCommandChanged)
                        .padding(10)
                        .width(Fill),
                    ]
                    .spacing(6),
                    container(
                        iced::widget::column![
                            text("参数").size(13),
                            argument_rows,
                            button("+ 添加参数")
                                .style(button::secondary)
                                .on_press(Message::AddMcpServerArgRow),
                        ]
                        .spacing(8),
                    )
                    .padding(14)
                    .style(container::bordered_box)
                    .width(Fill),
                    container(
                        iced::widget::column![
                            text("环境变量").size(13),
                            environment_rows,
                            button("+ 添加环境变量")
                                .style(button::secondary)
                                .on_press(Message::AddMcpServerEnvRow),
                        ]
                        .spacing(8),
                    )
                    .padding(14)
                    .style(container::bordered_box)
                    .width(Fill),
                    container(
                        iced::widget::column![
                            text("环境变量传递").size(13),
                            passthrough_rows,
                            button("+ 添加变量")
                                .style(button::secondary)
                                .on_press(Message::AddMcpServerEnvPassthroughRow),
                        ]
                        .spacing(8),
                    )
                    .padding(14)
                    .style(container::bordered_box)
                    .width(Fill),
                    iced::widget::column![
                        text("工作目录").size(13),
                        text_input(DEFAULT_MCP_WORKING_DIRECTORY, &self.mcp_server_form.cwd)
                            .on_input(Message::McpServerCwdChanged)
                            .padding(10)
                            .width(Fill),
                    ]
                    .spacing(6),
                ]
                .spacing(14)
                .into()
            }
            PersistedMcpTransportKind::Http | PersistedMcpTransportKind::Sse => {
                let endpoint_label = if matches!(
                    self.mcp_server_form.transport,
                    PersistedMcpTransportKind::Http
                ) {
                    "接口地址"
                } else {
                    "SSE 地址"
                };
                let mut start_oauth_button = button(if self.mcp_oauth_flow_in_flight {
                    "OAuth 进行中"
                } else {
                    "发起 OAuth"
                })
                .style(button::secondary);
                if self.selected_mcp_server_id.is_some() && !self.mcp_oauth_flow_in_flight {
                    start_oauth_button =
                        start_oauth_button.on_press(Message::StartSelectedMcpServerOAuth);
                }
                let mut complete_oauth_button = button("完成授权").style(button::secondary);
                if !self.mcp_oauth_callback_input.trim().is_empty() {
                    complete_oauth_button = complete_oauth_button
                        .on_press(Message::CompleteSelectedMcpServerOAuthFromInput);
                }
                let mut clear_oauth_button = button("清理凭据").style(button::secondary);
                if self.selected_mcp_server_id.is_some() {
                    clear_oauth_button = clear_oauth_button
                        .on_press(Message::RemoveSelectedMcpServerOAuthAuthorization);
                }
                let authorization_url_summary: Element<'_, Message> =
                    if self.mcp_oauth_authorization_url.trim().is_empty() {
                        text("最近一次授权链接：尚未生成").size(12).into()
                    } else {
                        text(format!(
                            "最近一次授权链接：{}",
                            self.mcp_oauth_authorization_url
                        ))
                        .size(12)
                        .into()
                    };
                let allow_remove_headers = self.mcp_server_form.header_rows.len() > 1;
                let header_rows = self.mcp_server_form.header_rows.iter().enumerate().fold(
                    iced::widget::column![].spacing(8),
                    |list, (index, row_state)| {
                        list.push(
                            row![
                                text_input("键", &row_state.key)
                                    .on_input(move |value| Message::McpServerHeaderKeyChanged(
                                        index, value
                                    ))
                                    .padding(10)
                                    .width(FillPortion(2)),
                                text_input("值", &row_state.value)
                                    .on_input(move |value| Message::McpServerHeaderValueChanged(
                                        index, value
                                    ))
                                    .padding(10)
                                    .width(FillPortion(3)),
                                build_row_remove_button(
                                    allow_remove_headers,
                                    Message::RemoveMcpServerHeaderRow(index),
                                ),
                            ]
                            .spacing(8)
                            .align_y(Center),
                        )
                    },
                );
                let allow_remove_env_headers = self.mcp_server_form.header_from_env_rows.len() > 1;
                let header_from_env_rows = self
                    .mcp_server_form
                    .header_from_env_rows
                    .iter()
                    .enumerate()
                    .fold(
                        iced::widget::column![].spacing(8),
                        |list, (index, row_state)| {
                            list.push(
                                row![
                                    text_input("Header 键", &row_state.key)
                                        .on_input(move |value| {
                                            Message::McpServerHeaderFromEnvKeyChanged(index, value)
                                        })
                                        .padding(10)
                                        .width(FillPortion(2)),
                                    text_input("环境变量名", &row_state.value)
                                        .on_input(move |value| {
                                            Message::McpServerHeaderFromEnvValueChanged(
                                                index, value,
                                            )
                                        })
                                        .padding(10)
                                        .width(FillPortion(3)),
                                    build_row_remove_button(
                                        allow_remove_env_headers,
                                        Message::RemoveMcpServerHeaderFromEnvRow(index),
                                    ),
                                ]
                                .spacing(8)
                                .align_y(Center),
                            )
                        },
                    );

                iced::widget::column![
                    iced::widget::column![
                        text(endpoint_label).size(13),
                        text_input("例如 https://example.com/mcp", &self.mcp_server_form.url,)
                            .on_input(Message::McpServerUrlChanged)
                            .padding(10)
                            .width(Fill),
                    ]
                    .spacing(6),
                    iced::widget::column![
                        text("Bearer Token").size(13),
                        text_input(
                            "可选，仅填写 Token 内容",
                            &self.mcp_server_form.bearer_token,
                        )
                        .on_input(Message::McpServerBearerTokenChanged)
                        .padding(10)
                        .secure(true)
                        .width(Fill),
                    ]
                    .spacing(6),
                    container(
                        iced::widget::column![
                            text("请求头").size(13),
                            header_rows,
                            button("+ 添加请求头")
                                .style(button::secondary)
                                .on_press(Message::AddMcpServerHeaderRow),
                        ]
                        .spacing(8),
                    )
                    .padding(14)
                    .style(container::bordered_box)
                    .width(Fill),
                    container(
                        iced::widget::column![
                            text("来自环境变量的请求头").size(13),
                            header_from_env_rows,
                            button("+ 添加环境变量请求头")
                                .style(button::secondary)
                                .on_press(Message::AddMcpServerHeaderFromEnvRow),
                        ]
                        .spacing(8),
                    )
                    .padding(14)
                    .style(container::bordered_box)
                    .width(Fill),
                    container(
                        iced::widget::column![
                            text("OAuth（可选）").size(14),
                            text(format!("当前认证状态：{}", self.mcp_oauth_status)).size(12),
                            iced::widget::column![
                                text("Client ID").size(13),
                                text_input(
                                    "留空则尝试动态注册",
                                    &self.mcp_server_form.oauth_client_id,
                                )
                                .on_input(Message::McpServerOAuthClientIdChanged)
                                .padding(10)
                                .width(Fill),
                            ]
                            .spacing(6),
                            iced::widget::column![
                                text("Client Secret").size(13),
                                text_input("可选", &self.mcp_server_form.oauth_client_secret,)
                                    .on_input(Message::McpServerOAuthClientSecretChanged)
                                    .padding(10)
                                    .secure(true)
                                    .width(Fill),
                            ]
                            .spacing(6),
                            iced::widget::column![
                                text("Scopes").size(13),
                                text_input(
                                    "使用分号、逗号或空格分隔",
                                    &self.mcp_server_form.oauth_scopes_text,
                                )
                                .on_input(Message::McpServerOAuthScopesChanged)
                                .padding(10)
                                .width(Fill),
                            ]
                            .spacing(6),
                            iced::widget::column![
                                text("回调地址").size(13),
                                text_input(
                                    "建议填写 localhost 回调地址",
                                    &self.mcp_server_form.oauth_redirect_uri,
                                )
                                .on_input(Message::McpServerOAuthRedirectUriChanged)
                                .padding(10)
                                .width(Fill),
                            ]
                            .spacing(6),
                            authorization_url_summary,
                            iced::widget::column![
                                text("手动回调").size(13),
                                text_input(
                                    "浏览器最终跳转 URL、/callback?... 路径或 query string",
                                    &self.mcp_oauth_callback_input,
                                )
                                .on_input(Message::McpServerOAuthCallbackInputChanged)
                                .padding(10)
                                .width(Fill),
                            ]
                            .spacing(6),
                            row![
                                start_oauth_button,
                                complete_oauth_button,
                                clear_oauth_button,
                            ]
                            .spacing(10),
                        ]
                        .spacing(12),
                    )
                    .padding(14)
                    .style(container::bordered_box)
                    .width(Fill),
                ]
                .spacing(14)
                .into()
            }
        };
        let notice_text: Element<'_, Message> = if self.mcp_notice.trim().is_empty() {
            container(space()).height(0).into()
        } else {
            text(&self.mcp_notice)
                .size(12)
                .color(palette.background.base.text.scale_alpha(0.68))
                .into()
        };
        let modal_body = scrollable(
            iced::widget::column![
                iced::widget::column![
                    text("名称").size(13),
                    text_input("MCP server name", &self.mcp_server_form.name)
                        .on_input(Message::McpServerNameChanged)
                        .padding(10)
                        .width(Fill),
                ]
                .spacing(6),
                iced::widget::column![text("连接方式").size(13), transport_selector].spacing(6),
                transport_fields,
                notice_text,
            ]
            .spacing(14),
        )
        .height(560)
        .spacing(8);
        let modal_card = container(
            iced::widget::column![
                row![
                    text(if self.selected_mcp_server_id.is_some() {
                        "编辑自定义 MCP"
                    } else {
                        "连接至自定义 MCP"
                    })
                    .size(20),
                    space::horizontal(),
                    close_button,
                ]
                .align_y(Center),
                modal_body,
                row![
                    checkbox(self.mcp_server_form.enabled)
                        .label("启用该服务器")
                        .on_toggle(Message::McpServerEnabledToggled),
                    space::horizontal(),
                    button("保存")
                        .style(button::primary)
                        .on_press(Message::SaveMcpServerConfiguration),
                ]
                .align_y(Center),
            ]
            .spacing(16),
        )
        .padding([22, 24])
        .width(820)
        .style(style::startup_overlay_card_style);
        let backdrop = mouse_area(container(space()).width(Fill).height(Fill))
            .on_press(Message::CloseMcpServerEditor);

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
