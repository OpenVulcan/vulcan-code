//! Transcript-oriented view composition methods extracted from the main workbench views module.
//! 从主工作台视图模块中抽取的转录相关界面组合方法。

use super::super::helpers::*;
use super::*;
use iced::widget::column;

/// Resolves one bounded image preview size that preserves the intrinsic aspect ratio while capping the preview height.
/// 解析一个保留原始宽高比且限制最大高度的图片预览尺寸。
fn bounded_image_preview_size(
    aspect_ratio: Option<f32>,
    max_height: f32,
    max_width: f32,
) -> iced::Size {
    let safe_aspect_ratio = aspect_ratio
        .filter(|ratio| ratio.is_finite() && *ratio > 0.0)
        .unwrap_or(16.0 / 10.0);
    let resolved_max_height = max_height.max(1.0);
    let resolved_max_width = max_width.max(1.0);
    let unconstrained_width = resolved_max_height * safe_aspect_ratio;
    let resolved_width = unconstrained_width.min(resolved_max_width).max(1.0);
    let resolved_height = (resolved_width / safe_aspect_ratio)
        .min(resolved_max_height)
        .max(1.0);
    iced::Size::new(resolved_width, resolved_height)
}

/// Maps one selectable transcript interaction into the corresponding workbench message while letting each caller supply its own thinking and tool modal routes.
/// 将一条可选择转录交互映射为对应的工作台消息，同时允许各调用方提供自己的思考与工具弹层路由。
fn map_selectable_transcript_interaction(
    interaction: SelectableTranscriptInteraction,
    open_thinking: impl Fn(String) -> Message,
    open_tool: impl Fn(String) -> Message,
) -> Message {
    match interaction {
        SelectableTranscriptInteraction::ToggleBlockInline(_) => Message::Noop,
        SelectableTranscriptInteraction::OpenThinkingModal(block_id) => open_thinking(block_id),
        SelectableTranscriptInteraction::OpenToolModal(block_id) => open_tool(block_id),
        SelectableTranscriptInteraction::OpenImagePreview {
            file_name,
            local_path,
        } => Message::OpenComposerImageAttachmentPreview {
            file_name,
            local_path,
        },
        SelectableTranscriptInteraction::ToggleAudioPlayback {
            file_name,
            local_path,
        } => Message::ToggleGeneratedAudioPlayback {
            file_name,
            local_path,
        },
        SelectableTranscriptInteraction::OpenMediaWithDefaultApp(local_path) => {
            Message::OpenGeneratedAttachmentWithDefaultApp(local_path)
        }
        SelectableTranscriptInteraction::RevealMediaInFolder(local_path) => {
            Message::RevealGeneratedAttachmentInFolder(local_path)
        }
        SelectableTranscriptInteraction::CopyMediaPath(local_path) => {
            Message::CopyGeneratedAttachmentPath(local_path)
        }
        SelectableTranscriptInteraction::CopyImageToClipboard {
            file_name,
            local_path,
        } => Message::CopyGeneratedImageToClipboard {
            file_name,
            local_path,
        },
        SelectableTranscriptInteraction::OpenContextMenu(request) => {
            Message::OpenTranscriptContextMenu(request)
        }
    }
}

/// Builds the stable surface identifier used by a selectable session transcript widget.
/// 构建可选择会话转录组件使用的稳定表面标识。
fn session_transcript_surface_id(session_id: SessionId) -> SelectableTranscriptSurfaceId {
    SelectableTranscriptSurfaceId(format!("session:{session_id}"))
}

/// Builds the stable surface identifier used by a centered thinking-modal transcript widget.
/// 构建居中思考弹层转录组件使用的稳定表面标识。
fn thinking_modal_transcript_surface_id(session_id: SessionId) -> SelectableTranscriptSurfaceId {
    SelectableTranscriptSurfaceId(format!("thinking-modal:{session_id}"))
}

/// Builds the stable surface identifier used by a streaming thinking-overlay transcript widget.
/// 构建流式思考浮层转录组件使用的稳定表面标识。
fn thinking_overlay_transcript_surface_id(session_id: SessionId) -> SelectableTranscriptSurfaceId {
    SelectableTranscriptSurfaceId(format!("thinking-overlay:{session_id}"))
}

/// Builds the stable surface identifier used by a selectable tool-modal transcript widget.
/// 构建可选择工具弹层转录组件使用的稳定表面标识。
fn tool_modal_transcript_surface_id(session_id: SessionId) -> SelectableTranscriptSurfaceId {
    SelectableTranscriptSurfaceId(format!("tool-modal:{session_id}"))
}

impl VulcanWorkbench {
    /// Builds the transcript content surface shown in the center of the main workbench canvas.
    /// 构建展示在主工作台中央画布中的会话内容表面。
    pub(super) fn view_main_canvas_transcript_stream(&self) -> Element<'_, Message> {
        if let Some(state) = self.selected_subagent_task_panel_state() {
            return self.view_selected_subagent_task_panel_surface(state);
        }

        let Some(session_id) = self.selected_session().map(|session| session.id) else {
            return container(
                column![
                    text("尚未选择会话").size(18),
                    text("先在左侧会话列表中选中一个会话，会话内容会显示在这里。").size(13),
                ]
                .spacing(8)
                .align_x(iced::Alignment::Center),
            )
            .width(Fill)
            .height(Fill)
            .center(Fill)
            .into();
        };

        if let Some((progress_value, progress_detail)) =
            self.session_agent_view_loading_progress(session_id)
        {
            return container(
                column![
                    text("正在载入会话").size(18),
                    text(progress_detail).size(13),
                    container(progress_bar(0.0..=1.0, progress_value)).width(Fill),
                ]
                .spacing(10)
                .max_width(420)
                .align_x(iced::Alignment::Center),
            )
            .width(Fill)
            .height(Fill)
            .center(Fill)
            .into();
        }

        let has_any_document_content = self
            .selected_session()
            .and_then(|session| self.session_agent_view_documents.get(&session.id))
            .map(|document| !document.blocks.is_empty())
            .unwrap_or(false);

        if !has_any_document_content {
            return container(
                column![
                    text("尚未开始执行").size(18),
                    text("输入任务并开始执行后，当前会话的输出会显示在这里。").size(13),
                ]
                .spacing(8)
                .align_x(iced::Alignment::Center),
            )
            .width(Fill)
            .height(Fill)
            .center(Fill)
            .into();
        }

        let base_surface: Element<'_, Message> =
            if self.agent_transcript_surface_mode == AgentTranscriptSurfaceMode::DetailCards {
                self.view_main_canvas_segmented_transcript_stream(session_id)
            } else {
                self.view_selected_session_agent_document_surface(session_id)
            };

        let mut layered_surfaces = vec![base_surface];

        if let Some(overlay) = self.view_selected_session_subagent_navigation_overlay() {
            layered_surfaces.push(overlay);
        }

        if let Some(overlay) = self.view_selected_session_rendered_transcript_fade_overlay() {
            layered_surfaces.push(overlay);
        }

        if let Some(overlay) = self.view_selected_session_streaming_thinking_overlay(session_id) {
            layered_surfaces.push(overlay);
        }

        if let Some(overlay) = self.view_selected_session_jump_to_bottom_overlay(session_id) {
            layered_surfaces.push(overlay);
        }

        if let Some(overlay) = self.view_selected_session_thinking_modal(session_id) {
            layered_surfaces.push(overlay);
        }

        if let Some(overlay) = self.view_selected_session_tool_modal(session_id) {
            layered_surfaces.push(overlay);
        }

        stack(layered_surfaces).into()
    }

    /// Builds explicit controls used to search history and load one older transcript page.
    /// 构建用于搜索历史并显式加载一页更早转录历史的控件。
    pub(super) fn view_session_history_load_control(
        &self,
        session_id: SessionId,
    ) -> Element<'_, Message> {
        let is_selected_root_session = self.selected_session().map(|session| session.id)
            == Some(session_id)
            && self.active_subagent_panel.is_none();
        if !is_selected_root_session {
            return space::vertical().height(0).into();
        }
        let mut controls = column![].spacing(8).width(Fill);

        let is_history_loading = self.loading_older_session_history_ids.contains(&session_id);
        let has_more_history = self.session_history_has_more.contains(&session_id);
        let label = if is_history_loading {
            "正在加载更早历史..."
        } else {
            "加载更早 30 轮"
        };
        let mut load_button = button(text(label).size(12))
            .padding([7, 12])
            .style(style::subtle_panel_button_style);
        if has_more_history && !is_history_loading {
            load_button =
                load_button.on_press(Message::LoadOlderSessionHistoryRequested(session_id));
        }
        if has_more_history || is_history_loading {
            controls = controls.push(
                row![
                    space::horizontal().width(Fill),
                    load_button,
                    space::horizontal().width(Fill)
                ]
                .align_y(Center),
            );
        }

        container(controls)
            .padding(iced::Padding {
                top: 0.0,
                right: 0.0,
                bottom: 12.0,
                left: 0.0,
            })
            .width(Fill)
            .into()
    }

    /// Builds the dedicated subagent transcript panel or its lightweight running placeholder inside the main transcript region.
    /// 在主转录区域内部构建独立子代理转录面板，或在运行中显示其轻量占位视图。
    pub(super) fn view_selected_subagent_task_panel_surface(
        &self,
        state: &SubagentTaskPanelState,
    ) -> Element<'_, Message> {
        let state = self.normalized_subagent_task_panel_state(state);
        if let Some(document) = self.rendered_subagent_task_panel_document(&state) {
            let session_id = state.task_id;
            let content_column = column![
                view_selectable_rendered_transcript_with_context_menu::<Message>(
                    &document,
                    &self.app_theme,
                    session_transcript_surface_id(session_id),
                    self.selectable_transcript_command.clone(),
                    move |request| Message::SelectedSessionTranscriptDragAutoScrollChanged {
                        session_id,
                        request,
                    },
                    |interaction| map_selectable_transcript_interaction(
                        interaction,
                        Message::OpenSelectedSessionThinkingModal,
                        Message::OpenSelectedSessionToolModal,
                    ),
                ),
                space::vertical().height(24),
            ]
            .width(Fill);

            let base_surface: Element<'_, Message> = container(
                scrollable(content_column)
                    .id(transcript_scrollable_id(session_id))
                    .height(Fill)
                    .on_scroll(move |viewport| Message::SelectedSessionTranscriptScrolled {
                        session_id,
                        viewport,
                    }),
            )
            .padding(14)
            .height(Fill)
            .into();

            let mut layered_surfaces = vec![base_surface];
            if let Some(overlay) = self.view_selected_session_subagent_navigation_overlay() {
                layered_surfaces.push(overlay);
            }
            if let Some(overlay) = self.view_selected_session_jump_to_bottom_overlay(session_id) {
                layered_surfaces.push(overlay);
            }
            if let Some(overlay) = self.view_selected_session_thinking_modal(session_id) {
                layered_surfaces.push(overlay);
            }
            if let Some(overlay) = self.view_selected_session_tool_modal(session_id) {
                layered_surfaces.push(overlay);
            }

            return stack(layered_surfaces).into();
        }

        let helper_text = match state.run_state {
            SubagentTaskRunState::Running => "子代理正在执行，面板会在任务结束后展示完整转录。",
            SubagentTaskRunState::Stopped => {
                "子代理已停止，当前保留截至停止时的上下文、思考与输出片段。"
            }
            SubagentTaskRunState::Completed => "子代理已结束，但当前没有可回看的转录内容。",
            SubagentTaskRunState::Failed => "子代理已失败，当前仅保留失败摘要。",
        };
        let status_label = super::subagent_run_state_badge_label(state.run_state);
        let title = state.subagent_label.clone();
        let description = state.description.clone();
        let result_text = state.result_text.clone();
        let status_icon: Element<'_, Message> = if state.run_state == SubagentTaskRunState::Running
        {
            codicon::sync().size(14).line_height(1.0).into()
        } else {
            space::horizontal().width(0).into()
        };
        let description_block: Element<'_, Message> = if let Some(description) = description {
            text(description).size(13).into()
        } else {
            space::vertical().height(0).into()
        };
        let result_block: Element<'_, Message> = if result_text.trim().is_empty() {
            space::vertical().height(0).into()
        } else {
            container(text(result_text).size(14).width(Fill))
                .padding(12)
                .style(style::workbench_composer_panel_style)
                .width(Fill)
                .into()
        };

        container(
            container(
                column![
                    row![status_icon, text(status_label).size(13),]
                        .spacing(6)
                        .align_y(Center),
                    text(title).size(20),
                    description_block,
                    text(helper_text).size(13),
                    result_block,
                ]
                .spacing(12)
                .width(Fill),
            )
            .max_width(860),
        )
        .width(Fill)
        .height(Fill)
        .center(Fill)
        .into()
    }

    /// Builds the passive lower-half fade overlay used only by the rendered transcript mode so bottom content gradually blends into the panel background.
    /// 构建仅用于渲染态转录视图的被动下半区淡隐遮罩，让底部内容逐渐融入面板背景。
    pub(super) fn view_selected_session_rendered_transcript_fade_overlay(
        &self,
    ) -> Option<Element<'_, Message>> {
        if self.agent_transcript_surface_mode != AgentTranscriptSurfaceMode::DetailCards {
            return None;
        }

        Some(
            container(
                column![
                    space::vertical().height(FillPortion(1)),
                    container(space())
                        .width(Fill)
                        .height(FillPortion(1))
                        .style(style::rendered_transcript_bottom_fade_overlay_style),
                ]
                .width(Fill)
                .height(Fill),
            )
            .width(Fill)
            .height(Fill)
            .into(),
        )
    }

    /// Builds the floating jump-to-bottom affordance shown after the user scrolls away from the newest transcript tail.
    /// 构建当用户滚离最新转录尾部后显示的浮动回底按钮。
    pub(super) fn view_selected_session_jump_to_bottom_overlay(
        &self,
        session_id: SessionId,
    ) -> Option<Element<'_, Message>> {
        if self.session_agent_view_should_follow_tail(session_id)
            || !self.session_agent_view_viewports.contains_key(&session_id)
        {
            return None;
        }

        let button = tooltip(
            button(
                container(codicon::chevron_down().size(18).line_height(1.0))
                    .width(TRANSCRIPT_JUMP_TO_BOTTOM_BUTTON_SIZE)
                    .height(TRANSCRIPT_JUMP_TO_BOTTOM_BUTTON_SIZE)
                    .center(Fill),
            )
            .padding(0)
            .width(TRANSCRIPT_JUMP_TO_BOTTOM_BUTTON_SIZE)
            .height(TRANSCRIPT_JUMP_TO_BOTTOM_BUTTON_SIZE)
            .style(style::transcript_jump_to_bottom_button_style)
            .on_press(Message::JumpSelectedSessionTranscriptToBottom),
            "回到底部",
            tooltip::Position::Top,
        )
        .style(container::rounded_box);
        let bottom_reserved_height = if self
            .selected_subagent_task_panel_state()
            .is_some_and(|state| state.task_id == session_id)
        {
            0.0
        } else {
            self.floating_composer_footprint_height()
        };

        Some(
            container(
                column![
                    space::vertical().height(Fill),
                    row![
                        space::horizontal().width(Fill),
                        button,
                        space::horizontal().width(Fill)
                    ]
                    .width(Fill)
                    .align_y(Center),
                ]
                .width(Fill)
                .height(Fill),
            )
            .padding(iced::Padding {
                top: 14.0,
                right: 14.0,
                bottom: bottom_reserved_height
                    + f32::from(TRANSCRIPT_JUMP_TO_BOTTOM_OVERLAY_BOTTOM_INSET),
                left: 14.0,
            })
            .width(Fill)
            .height(Fill)
            .into(),
        )
    }

    /// Builds the unified rendered transcript stream used for per-block inspection during the transition period.
    /// 构建过渡期用于逐块检查的统一渲染转录流。
    pub(super) fn view_main_canvas_segmented_transcript_stream(
        &self,
        session_id: SessionId,
    ) -> Element<'_, Message> {
        let Some(document) = self.rendered_session_agent_view_document(session_id) else {
            return container(text("当前没有可渲染的统一输出块。").size(14))
                .padding(14)
                .height(Fill)
                .into();
        };
        let content_column = column![
            self.view_session_history_load_control(session_id),
            view_selectable_rendered_transcript_with_context_menu::<Message>(
                &document,
                &self.app_theme,
                session_transcript_surface_id(session_id),
                self.selectable_transcript_command.clone(),
                move |request| Message::SelectedSessionTranscriptDragAutoScrollChanged {
                    session_id,
                    request,
                },
                |interaction| map_selectable_transcript_interaction(
                    interaction,
                    Message::OpenSelectedSessionThinkingModal,
                    Message::OpenSelectedSessionToolModal,
                ),
            ),
            space::vertical().height(self.transcript_bottom_spacer_height()),
        ]
        .width(Fill);

        container(
            scrollable(content_column)
                .id(transcript_scrollable_id(session_id))
                .height(Fill)
                .spacing(10)
                .on_scroll(move |viewport| Message::SelectedSessionTranscriptScrolled {
                    session_id,
                    viewport,
                }),
        )
        .padding(14)
        .height(Fill)
        .into()
    }

    /// Builds one reusable selectable Markdown body from the provided read-only buffer or fallback text.
    /// 根据提供的只读缓冲区或回退文本构建一块可复用的可选 Markdown 正文。
    pub(super) fn view_workbench_markdown_body<'a>(
        &'a self,
        buffer: Option<&'a text_editor::Content>,
        fallback: String,
        on_action: impl Fn(text_editor::Action) -> Message + 'a,
    ) -> Element<'a, Message> {
        if let Some(buffer) = buffer {
            text_editor(buffer)
                .height(Shrink)
                .wrapping(iced::widget::text::Wrapping::Word)
                .padding(0)
                .size(14)
                .style(style::workbench_transcript_raw_view_style)
                .on_action(on_action)
                .into()
        } else {
            text(fallback).size(14).into()
        }
    }

    /// Builds one reusable selectable raw-text body from the provided read-only buffer or fallback text.
    /// 根据提供的只读缓冲区或回退文本构建一块可复用的可选原文正文。
    pub(super) fn view_workbench_raw_body<'a>(
        &'a self,
        buffer: Option<&'a text_editor::Content>,
        fallback: String,
        on_action: impl Fn(text_editor::Action) -> Message + 'a,
    ) -> Element<'a, Message> {
        if let Some(buffer) = buffer {
            text_editor(buffer)
                .height(Shrink)
                .wrapping(iced::widget::text::Wrapping::Word)
                .padding(0)
                .style(style::workbench_transcript_raw_view_style)
                .on_action(on_action)
                .into()
        } else {
            text(fallback).size(14).into()
        }
    }

    /// Builds one reusable code-preview body that matches the patch/scaffold preview surface while keeping transcript text selectable.
    /// 构建一块与补丁/骨架预览表面一致且仍保留可选能力的可复用代码预览正文。
    pub(super) fn view_workbench_code_preview_body<'a>(
        &'a self,
        buffer: Option<&'a text_editor::Content>,
        fallback: String,
        syntax_token: &'a str,
        on_action: impl Fn(text_editor::Action) -> Message + 'a,
    ) -> Element<'a, Message> {
        if let Some(buffer) = buffer {
            WheelPassthrough::new(
                text_editor(buffer)
                    .height(Shrink)
                    .padding(14)
                    .size(WORKBENCH_TRANSCRIPT_CODE_PREVIEW_TEXT_SIZE)
                    .line_height(Pixels(CODE_EDITOR_LINE_HEIGHT))
                    .wrapping(iced::advanced::text::Wrapping::None)
                    .font(theme::code_font())
                    .style(style::diff_code_editor_style)
                    .highlight_with::<iced::highlighter::Highlighter>(
                        iced::highlighter::Settings {
                            theme: theme::code_theme(&self.app_theme),
                            token: syntax_token.to_string(),
                        },
                        theme::code_highlight_format,
                    )
                    .on_action(on_action),
            )
            .into()
        } else {
            container(
                text(fallback)
                    .size(WORKBENCH_TRANSCRIPT_CODE_PREVIEW_TEXT_SIZE)
                    .line_height(Pixels(CODE_EDITOR_LINE_HEIGHT))
                    .font(theme::code_font())
                    .wrapping(iced::advanced::text::Wrapping::None),
            )
            .padding(14)
            .width(Fill)
            .into()
        }
    }

    /// Builds the selectable unified transcript panel for the selected session.
    /// 构建当前选中会话使用的可选统一转录面板。
    pub(super) fn view_selected_session_agent_document_surface(
        &self,
        session_id: SessionId,
    ) -> Element<'_, Message> {
        let (buffer, on_action, fallback_text) = match self.agent_transcript_surface_mode {
            AgentTranscriptSurfaceMode::DetailCards => {
                let buffer = self.session_agent_view_buffers.get(&session_id);
                let fallback_text = if buffer.is_none() {
                    self.session_agent_view_selectable_documents
                        .get(&session_id)
                        .map(|document| document.buffer().to_string())
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                (
                    buffer,
                    Message::SelectedSessionDocumentStructuredAction
                        as fn(text_editor::Action) -> Message,
                    fallback_text,
                )
            }
            AgentTranscriptSurfaceMode::RawSource => {
                let buffer = self.session_agent_view_raw_buffers.get(&session_id);
                let fallback_text = if buffer.is_none() {
                    self.rendered_session_agent_view_document(session_id)
                        .map(|document| agent_view_document_raw_source(&document))
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                (
                    buffer,
                    Message::SelectedSessionDocumentRawSourceAction
                        as fn(text_editor::Action) -> Message,
                    fallback_text,
                )
            }
            AgentTranscriptSurfaceMode::ApiRequest => (
                self.session_last_api_request_buffers.get(&session_id),
                Message::SelectedSessionDocumentApiRequestAction
                    as fn(text_editor::Action) -> Message,
                String::from("当前还没有真实 API 请求快照。"),
            ),
        };

        let body =
            self.view_workbench_code_preview_body(buffer, fallback_text, "markdown", on_action);
        let body_flow = column![
            self.view_session_history_load_control(session_id),
            body,
            space::vertical().height(self.transcript_bottom_spacer_height()),
        ]
        .width(Fill);

        container(
            scrollable(container(body_flow).width(Fill))
                .id(transcript_scrollable_id(session_id))
                .height(Fill)
                .on_scroll(move |viewport| Message::SelectedSessionTranscriptScrolled {
                    session_id,
                    viewport,
                }),
        )
        .padding([14, 16])
        .height(Fill)
        .into()
    }

    /// Builds the centered thinking modal shown when the user explicitly opens one inline thinking block.
    /// 构建当用户显式打开某个内联思考区块时显示的居中思考弹层。
    pub(super) fn view_selected_session_thinking_modal(
        &self,
        session_id: SessionId,
    ) -> Option<Element<'_, Message>> {
        let modal_state = self
            .active_thinking_modal
            .as_ref()
            .filter(|modal_state| modal_state.session_id == session_id)?;
        let progress = thinking_modal_progress(modal_state.started_at, modal_state.phase);
        Some(view_thinking_modal_surface_with_context_menu(
            modal_state.payload.title.clone(),
            modal_state.payload.helper_text.clone(),
            &modal_state.payload.document,
            modal_state.payload.document_signature,
            &self.app_theme,
            progress,
            thinking_overlay_scrollable_id(session_id),
            thinking_modal_transcript_surface_id(session_id),
            self.selectable_transcript_command.clone(),
            move |request| Message::SelectedSessionThinkingOverlayDragAutoScrollChanged {
                session_id,
                request,
            },
            |interaction| {
                map_selectable_transcript_interaction(
                    interaction,
                    Message::OpenSelectedSessionThinkingModal,
                    Message::OpenSelectedSessionToolModal,
                )
            },
            move |viewport| Message::SelectedSessionThinkingOverlayScrolled {
                session_id,
                viewport,
            },
            Message::CloseSelectedSessionThinkingModal,
        ))
    }

    /// Builds the centered tool modal shown when the user explicitly opens one inline tool block.
    /// 构建当用户显式打开某个内联工具区块时显示的居中工具弹层。
    pub(super) fn view_selected_session_tool_modal(
        &self,
        session_id: SessionId,
    ) -> Option<Element<'_, Message>> {
        let modal_state = self
            .active_tool_modal
            .as_ref()
            .filter(|modal_state| modal_state.session_id == session_id)?;
        let progress = thinking_modal_progress(modal_state.started_at, modal_state.phase);
        if !modal_state.payload.generated_attachments.is_empty() {
            return Some(self.view_generated_tool_modal_surface(session_id, modal_state, progress));
        }
        Some(view_thinking_modal_surface_with_context_menu(
            modal_state.payload.title.clone(),
            modal_state.payload.helper_text.clone(),
            &modal_state.payload.document,
            modal_state.payload.document_signature,
            &self.app_theme,
            progress,
            tool_modal_scrollable_id(session_id),
            tool_modal_transcript_surface_id(session_id),
            self.selectable_transcript_command.clone(),
            |_| Message::Noop,
            |interaction| {
                map_selectable_transcript_interaction(
                    interaction,
                    Message::OpenSelectedSessionThinkingModal,
                    Message::OpenSelectedSessionToolModal,
                )
            },
            |_| Message::Noop,
            Message::CloseSelectedSessionToolModal,
        ))
    }

    /// Builds the richer generated-media tool modal that keeps preview cards and the raw transcript detail inside one shared centered surface.
    /// 构建更丰富的生成媒体工具弹层，在统一居中表面中同时保留预览卡片与原始转录详情。
    fn view_generated_tool_modal_surface(
        &self,
        session_id: SessionId,
        modal_state: &ToolModalState,
        progress: f32,
    ) -> Element<'_, Message> {
        let reveal_progress = progress.clamp(0.0, 1.0);
        let vertical_inset = 22.0 + (1.0 - reveal_progress) * 6.0;
        let header_secondary = self
            .app_theme
            .palette()
            .background
            .base
            .text
            .scale_alpha(0.54 + reveal_progress * 0.24);
        let generated_cards =
            self.view_generated_tool_modal_cards(&modal_state.payload.generated_attachments);
        let transcript_body = view_selectable_rendered_transcript_with_context_menu::<Message>(
            &modal_state.payload.document,
            &self.app_theme,
            tool_modal_transcript_surface_id(session_id),
            self.selectable_transcript_command.clone(),
            |_| Message::Noop,
            |interaction| {
                map_selectable_transcript_interaction(
                    interaction,
                    Message::OpenSelectedSessionThinkingModal,
                    Message::OpenSelectedSessionToolModal,
                )
            },
        );
        let modal_body = scrollable(
            column![
                generated_cards,
                rule::horizontal(1),
                text("原始工具返回").size(12),
                transcript_body,
            ]
            .spacing(14)
            .width(Fill),
        )
        .id(tool_modal_scrollable_id(session_id))
        .height(Fill);
        let modal_panel = container(
            column![
                row![
                    column![
                        text(modal_state.payload.title.clone()).size(18),
                        text(modal_state.payload.helper_text.clone())
                            .size(12)
                            .style(move |_theme: &Theme| iced::widget::text::Style {
                                color: Some(header_secondary),
                            }),
                    ]
                    .spacing(4),
                    space::horizontal(),
                    button(text("✕").size(14))
                        .padding([4, 8])
                        .style(button::text)
                        .on_press(Message::CloseSelectedSessionToolModal),
                ]
                .align_y(Center),
                rule::horizontal(1),
                container(modal_body).padding([4, 0]).height(Fill),
            ]
            .spacing(12),
        )
        .padding([16, 18])
        .width(Fill)
        .height(Fill)
        .style(move |theme| vulcan_agent_view::thinking_modal_card_style(theme, progress));
        let backdrop = mouse_area(
            container(space())
                .width(Fill)
                .height(Fill)
                .style(move |theme| {
                    vulcan_agent_view::thinking_modal_backdrop_style(theme, progress)
                }),
        )
        .on_press(Message::CloseSelectedSessionToolModal);
        let centered_modal = column![
            space::vertical().height(FillPortion(15)),
            row![
                space::horizontal().width(FillPortion(15)),
                opaque(
                    container(modal_panel)
                        .width(FillPortion(70))
                        .height(FillPortion(70))
                ),
                space::horizontal().width(FillPortion(15)),
            ]
            .width(Fill)
            .height(FillPortion(70)),
            space::vertical().height(FillPortion(15)),
        ]
        .width(Fill)
        .height(Fill)
        .padding([vertical_inset, 20.0]);

        opaque(
            stack([
                Element::from(backdrop),
                Element::from(container(centered_modal).width(Fill).height(Fill)),
            ])
            .width(Fill)
            .height(Fill),
        )
        .into()
    }

    /// Builds the generated-media preview section shown above tool transcript details inside the richer tool modal.
    /// 构建显示在更丰富工具弹层中、位于工具转录详情上方的生成媒体预览区。
    fn view_generated_tool_modal_cards(
        &self,
        attachments: &[ConversationAttachment],
    ) -> Element<'_, Message> {
        let cards = attachments.iter().fold(
            column![text("生成结果").size(12)].spacing(12),
            |column, attachment| column.push(self.view_generated_tool_modal_card(attachment)),
        );
        container(cards).padding([4, 0]).width(Fill).into()
    }

    /// Builds one generated-media preview card that exposes direct preview/open/copy/reveal actions without forcing the user to read raw file paths.
    /// 构建单个生成媒体预览卡片，直接暴露预览、打开、复制与定位动作，而不再强迫用户阅读原始文件路径。
    fn view_generated_tool_modal_card(
        &self,
        attachment: &ConversationAttachment,
    ) -> Element<'_, Message> {
        let preview: Element<'_, Message> = match attachment.kind {
            ConversationAttachmentKind::Image if Path::new(&attachment.local_path).is_file() => {
                let preview_size = bounded_image_preview_size(
                    local_raster_image_aspect_ratio(&attachment.local_path),
                    220.0,
                    520.0,
                );
                container(
                    button(
                        iced_image(attachment.local_path.clone())
                            .width(preview_size.width)
                            .height(preview_size.height)
                            .content_fit(ContentFit::Contain),
                    )
                    .padding(0)
                    .style(style::workbench_composer_attachment_preview_button_style)
                    .on_press(Message::OpenComposerImageAttachmentPreview {
                        file_name: attachment.file_name.clone(),
                        local_path: attachment.local_path.clone(),
                    }),
                )
                .width(Fill)
                .align_x(iced::Alignment::Start)
                .into()
            }
            ConversationAttachmentKind::Video
                if resolve_video_thumbnail_preview(
                    &attachment.local_path,
                    Some(attachment.mime_type.as_str()),
                )
                .is_some() =>
            {
                let preview = resolve_video_thumbnail_preview(
                    &attachment.local_path,
                    Some(attachment.mime_type.as_str()),
                )
                .expect("guard above ensures video preview exists");
                let preview_size =
                    bounded_image_preview_size(Some(preview.aspect_ratio), 220.0, 520.0);
                container(
                    button(
                        iced_image(preview.handle)
                            .width(preview_size.width)
                            .height(preview_size.height)
                            .content_fit(ContentFit::Contain),
                    )
                    .padding(0)
                    .style(style::workbench_composer_attachment_preview_button_style)
                    .on_press(Message::OpenGeneratedAttachmentWithDefaultApp(
                        attachment.local_path.clone(),
                    )),
                )
                .width(Fill)
                .align_x(iced::Alignment::Start)
                .into()
            }
            ConversationAttachmentKind::Image => self.view_generated_tool_modal_icon_preview(
                lucide::image().size(28).line_height(1.0).into(),
                "图片文件已生成，可使用下方操作继续查看。",
            ),
            ConversationAttachmentKind::Audio => self.view_generated_tool_modal_icon_preview(
                lucide::volume_two().size(28).line_height(1.0).into(),
                "点击下方播放按钮即可直接播放当前音频，再次点击会停止。",
            ),
            ConversationAttachmentKind::Video => self.view_generated_tool_modal_icon_preview(
                lucide::video().size(28).line_height(1.0).into(),
                "当前视频可使用系统默认播放器打开，或直接定位到文件位置。",
            ),
            ConversationAttachmentKind::Pdf => self.view_generated_tool_modal_icon_preview(
                lucide::file_text().size(28).line_height(1.0).into(),
                "当前文档可直接打开，或定位到文件位置。",
            ),
        };
        let open_label = match attachment.kind {
            ConversationAttachmentKind::Image => "预览图片",
            ConversationAttachmentKind::Audio => {
                if self.is_generated_audio_playing_local_path(&attachment.local_path) {
                    "停止播放"
                } else {
                    "播放音频"
                }
            }
            ConversationAttachmentKind::Video => "打开视频",
            ConversationAttachmentKind::Pdf => "打开文档",
        };
        let meta_line = match attachment.byte_len {
            Some(byte_len) => format!(
                "{} · {}",
                attachment.mime_type,
                Self::format_attachment_cleanup_byte_len(byte_len)
            ),
            None => attachment.mime_type.clone(),
        };
        let primary_action = match attachment.kind {
            ConversationAttachmentKind::Image => Message::OpenComposerImageAttachmentPreview {
                file_name: attachment.file_name.clone(),
                local_path: attachment.local_path.clone(),
            },
            ConversationAttachmentKind::Audio => Message::ToggleGeneratedAudioPlayback {
                file_name: attachment.file_name.clone(),
                local_path: attachment.local_path.clone(),
            },
            _ => Message::OpenGeneratedAttachmentWithDefaultApp(attachment.local_path.clone()),
        };
        let secondary_copy_button: Element<'_, Message> = match attachment.kind {
            ConversationAttachmentKind::Image => button(text("复制图片").size(12))
                .padding([6, 10])
                .style(style::subtle_panel_button_style)
                .on_press(Message::CopyGeneratedImageToClipboard {
                    file_name: attachment.file_name.clone(),
                    local_path: attachment.local_path.clone(),
                })
                .into(),
            _ => button(text("复制路径").size(12))
                .padding([6, 10])
                .style(style::subtle_panel_button_style)
                .on_press(Message::CopyGeneratedAttachmentPath(
                    attachment.local_path.clone(),
                ))
                .into(),
        };

        container(
            column![
                preview,
                column![
                    text(attachment.file_name.clone()).size(14),
                    text(meta_line).size(12),
                ]
                .spacing(4)
                .width(Fill),
                row![
                    button(text(open_label).size(12))
                        .padding([6, 10])
                        .style(style::subtle_panel_button_style)
                        .on_press(primary_action),
                    secondary_copy_button,
                    button(text("定位文件").size(12))
                        .padding([6, 10])
                        .style(style::subtle_panel_button_style)
                        .on_press(Message::RevealGeneratedAttachmentInFolder(
                            attachment.local_path.clone()
                        )),
                ]
                .spacing(8),
            ]
            .spacing(12)
            .width(Fill),
        )
        .padding(14)
        .style(style::workbench_composer_panel_style)
        .width(Fill)
        .into()
    }

    /// Builds one icon-based generated-media preview placeholder for non-image outputs.
    /// 为非图片输出构建基于图标的生成媒体预览占位卡片。
    fn view_generated_tool_modal_icon_preview<'a>(
        &self,
        icon: Element<'a, Message>,
        helper_text: &'a str,
    ) -> Element<'a, Message> {
        container(
            column![icon, text(helper_text).size(12)]
                .spacing(10)
                .width(Fill)
                .align_x(Center),
        )
        .padding([20, 16])
        .width(Fill)
        .style(style::workbench_composer_attachment_icon_tile_style)
        .into()
    }

    /// Builds the floating streaming-thinking overlay shown while one active thinking message is still in progress.
    /// 构建当某条活动思考消息仍在流式进行时显示的浮动思考面板。
    pub(super) fn view_selected_session_streaming_thinking_overlay(
        &self,
        session_id: SessionId,
    ) -> Option<Element<'_, Message>> {
        if self
            .active_thinking_modal
            .as_ref()
            .is_some_and(|modal_state| modal_state.session_id == session_id)
        {
            return None;
        }

        let message = self.streaming_thinking_message(session_id)?.clone();
        let mut document = build_agent_view_document(std::slice::from_ref(&message), &[]);
        let block = document.blocks.get_mut(0)?;
        let block_id = agent_view_block_id(session_id, 0, block);
        self.decorate_agent_view_block_for_render(session_id, &block_id, block);
        let (title, document) = build_overlay_thinking_surface_document(document)?;

        Some(
            container(view_thinking_overlay_surface_with_context_menu(
                title,
                String::from("运行中思考会先停留在这里，完成后再回收到主会话。"),
                &document,
                &self.app_theme,
                thinking_overlay_scrollable_id(session_id),
                thinking_overlay_transcript_surface_id(session_id),
                self.selectable_transcript_command.clone(),
                move |request| Message::SelectedSessionThinkingOverlayDragAutoScrollChanged {
                    session_id,
                    request,
                },
                |interaction| {
                    map_selectable_transcript_interaction(
                        interaction,
                        Message::OpenSelectedSessionThinkingModal,
                        Message::OpenSelectedSessionToolModal,
                    )
                },
                move |viewport| Message::SelectedSessionThinkingOverlayScrolled {
                    session_id,
                    viewport,
                },
            ))
            .padding(iced::Padding {
                top: 0.0,
                right: 0.0,
                bottom: self.floating_composer_footprint_height(),
                left: 0.0,
            })
            .width(Fill)
            .height(Fill)
            .into(),
        )
    }
}
