//! Diff window and file-rendering view composition methods extracted from the main workbench views module.
//! 从主工作台视图模块中抽取的 Diff 窗口与文件渲染界面组合方法。

use super::*;
use iced::widget::column;

impl VulcanWorkbench {
    /// Builds the dedicated diff preview window so the panel can be iterated independently.
    /// 构建独立的 Diff 预览窗口，以便面板可以脱离主工作台单独迭代。
    pub(super) fn view_diff_window(&self, window_id: window::Id) -> Element<'_, Message> {
        // Build the fixed top chrome so the diff window inherits the same custom frame language as the main workbench.
        // 构建固定在顶部的统一 chrome 区块，使 Diff 窗口继承与主工作台一致的自定义窗口框架语言。
        let top_chrome = container(self.view_window_title_bar(window_id, "Git Diff Review Panel"))
            .style(style::window_chrome_style);

        let diff_header = container(
            row![
                column![
                    text("Git Diff 预览窗口").size(24),
                    text("独立窗口，便于单独调整审查面板结构与信息层级。").size(13),
                ]
                .spacing(4),
                space::horizontal(),
                button("重新扫描当前工作区")
                    .padding([10, 14])
                    .style(button::primary)
                    .on_press(Message::ScanWorkspace),
            ]
            .align_y(Center),
        )
        .padding(14)
        .style(container::bordered_box);

        // Keep workspace and unstaged summary in a single top-level bar so the review area itself stays clean.
        // 将工作区与未暂存摘要收敛到同一条顶部信息栏，保持审查内容区本身更干净。
        let diff_context = container(
            row![
                text(&self.diff_summary).size(15),
                space::horizontal(),
                text(format!("工作区：{}", self.workspace_path)).size(13),
            ]
            .align_y(Center),
        )
        .padding([10, 14])
        .style(container::rounded_box);

        self.wrap_with_window_resize_handles(
            window_id,
            column![
                top_chrome,
                container(
                    column![
                        diff_header,
                        diff_context,
                        container(self.view_diff_panel())
                            .height(Fill)
                            .style(container::bordered_box),
                    ]
                    .spacing(12)
                )
                .padding(16)
                .height(Fill),
            ]
            .spacing(0)
            .height(Fill)
            .into(),
        )
    }
    /// Renders the structured Git diff panel inside the center workbench.
    /// 在中央工作台中渲染结构化 Git diff 面板。
    pub(super) fn view_diff_panel(&self) -> Element<'_, Message> {
        // Render the fallback notice path when no structured diff files are currently available.
        // 当当前没有结构化 diff 文件时，渲染回退提示路径。
        if !self.diff_panel.has_files() {
            return scrollable(
                container(
                    column![
                        text(&self.diff_panel.notice).size(16),
                        rule::horizontal(1),
                        text(&self.diff_panel.fallback_text).size(13).width(Fill),
                    ]
                    .spacing(12),
                )
                .padding(14)
                .height(Fill)
                .style(container::rounded_box),
            )
            .height(Fill)
            .into();
        }

        // Build the file review flow so users can scan changed files top-to-bottom like a code review panel.
        // 构建文件级审查流，使用户可以像代码审查面板一样自上而下浏览改动。
        let visible_range = self.visible_diff_file_range();
        let mut review_flow = column![].spacing(0);

        if visible_range.top_spacer_height > 0.0 {
            review_flow = review_flow.push(space().height(visible_range.top_spacer_height));
        }

        for file_index in visible_range.start..visible_range.end {
            if file_index > visible_range.start {
                review_flow = review_flow.push(space().height(DIFF_FILE_CARD_SPACING));
            }

            if let Some(file) = self.diff_panel.files.get(file_index) {
                review_flow = review_flow
                    .push(self.view_diff_file_card(file, self.diff_file_top_offset(file_index)));
            }
        }

        if visible_range.bottom_spacer_height > 0.0 {
            review_flow = review_flow.push(space().height(visible_range.bottom_spacer_height));
        }

        let current_file_bar = self.view_current_diff_file_bar();

        column![
            current_file_bar,
            scrollable(review_flow)
                .id(DIFF_PANEL_SCROLLABLE_ID)
                .on_scroll(Message::DiffPanelScrolled)
                .height(Fill)
                .spacing(8)
        ]
        .spacing(8)
        .height(Fill)
        .into()
    }
    /// Renders the external current-file bar that mirrors the review scroll position.
    /// 渲染镜像审查滚动位置的外置当前文件提示条。
    pub(super) fn view_current_diff_file_bar(&self) -> Element<'_, Message> {
        let Some(active_file) = self.active_diff_file() else {
            return space().height(0).into();
        };

        let file_key = active_file.file.key();
        let toggle_file_key = file_key.clone();
        let new_file_marker: Element<'_, Message> = if active_file.file.is_new_file() {
            text("●").size(11).color(iced::color!(0x218BFF)).into()
        } else {
            space().width(11).into()
        };

        container(
            button(
                row![
                    text("当前文件").size(11),
                    row![
                        new_file_marker,
                        column![
                            text(active_file.file.header_title()).size(14),
                            text(
                                active_file
                                    .file
                                    .full_path_label(&self.diff_panel.repository_root)
                            )
                            .size(10),
                        ]
                        .spacing(0),
                    ]
                    .spacing(8)
                    .align_y(Center),
                    space::horizontal(),
                    self.view_diff_file_header_metrics(
                        active_file.file.additions,
                        active_file.file.deletions,
                        active_file.is_collapsed,
                    ),
                ]
                .spacing(10)
                .align_y(Center),
            )
            .width(Fill)
            .height(DIFF_CURRENT_FILE_BAR_HEIGHT)
            .padding([4, 12])
            .style(button::text)
            .on_press(Message::ToggleAndFocusDiffFile(toggle_file_key)),
        )
        .style(container::bordered_box)
        .width(Fill)
        .into()
    }
    /// Renders one file card in the Git diff review flow.
    /// 在 Git diff 审查流中渲染单个文件卡片。
    pub(super) fn view_diff_file_card<'a>(
        &'a self,
        file: &'a DiffFile,
        file_top_offset: f32,
    ) -> Element<'a, Message> {
        // Resolve the stable file key so collapse state survives rerenders and repeat scans.
        // 解析稳定文件键，保证折叠状态在重渲染和重复扫描后依然可追踪。
        let file_key = file.key();
        let is_collapsed = self.collapsed_diff_files.contains(&file_key);

        let file_header = self.view_diff_file_header(file, is_collapsed);

        let file_body: Element<'a, Message> = if is_collapsed {
            column![].into()
        } else {
            container(self.view_diff_file_code(file, file_top_offset))
                .width(Fill)
                .into()
        };

        container(column![file_header, file_body,].spacing(0))
            .style(container::bordered_box)
            .clip(true)
            .width(Fill)
            .into()
    }
    /// Renders the clickable header row shared by normal and sticky diff file cards.
    /// 渲染普通与吸顶 diff 文件卡片共用的可点击头部行。
    pub(super) fn view_diff_file_header<'a>(
        &'a self,
        file: &'a DiffFile,
        is_collapsed: bool,
    ) -> Element<'a, Message> {
        button(self.view_diff_file_header_content(file, is_collapsed))
            .padding([8, 12])
            .height(DIFF_FILE_HEADER_HEIGHT)
            .width(Fill)
            .style(button::text)
            .on_press(Message::ToggleDiffFile(file.key()))
            .into()
    }
    /// Builds the shared visual content of one diff file header.
    /// 构建单个 diff 文件头部的共享视觉内容。
    pub(super) fn view_diff_file_header_content<'a>(
        &'a self,
        file: &'a DiffFile,
        is_collapsed: bool,
    ) -> Element<'a, Message> {
        // Render a blue marker only for new files so freshly discovered entries stand out immediately.
        // 仅为新增文件渲染蓝色标记，以便新发现的条目可以被立即识别。
        let new_file_marker: Element<'a, Message> = if file.is_new_file() {
            text("●").size(11).color(iced::color!(0x218BFF)).into()
        } else {
            space().width(11).into()
        };

        // Keep the header compact while exposing both title and full path inside the same row height budget.
        // 在相同行高预算内同时显示标题与完整路径，并保持头部紧凑。
        row![
            row![
                new_file_marker,
                column![
                    text(file.header_title()).size(15),
                    text(file.full_path_label(&self.diff_panel.repository_root)).size(11),
                ]
                .spacing(0),
            ]
            .spacing(8)
            .align_y(Center),
            space::horizontal(),
            self.view_diff_file_header_metrics(file.additions, file.deletions, is_collapsed),
        ]
        .align_y(Center)
        .into()
    }
    /// Renders the shared trailing metrics cluster used by diff file headers and the current-file bar.
    /// 渲染 diff 文件头与当前文件条共用的尾部统计区块。
    pub(super) fn view_diff_file_header_metrics(
        &self,
        additions: usize,
        deletions: usize,
        is_collapsed: bool,
    ) -> Element<'_, Message> {
        row![
            text(format!("+{additions}"))
                .size(15)
                .color(style::diff_bar_color(crate::diff::DiffLineKind::Added)),
            text(format!("-{deletions}"))
                .size(15)
                .color(style::diff_bar_color(crate::diff::DiffLineKind::Removed)),
            text(if is_collapsed { "▸" } else { "▾" }).size(DIFF_FOLD_ARROW_SIZE),
        ]
        .spacing(10)
        .align_y(Center)
        .into()
    }
    /// Renders one file-level diff body using the same editor foundation as the home page.
    /// 使用与首页相同的编辑器基础渲染文件级 diff 主体。
    pub(super) fn view_diff_file_code<'a>(
        &'a self,
        file: &'a DiffFile,
        file_top_offset: f32,
    ) -> Element<'a, Message> {
        // Render a compact fallback when the file has no visible hunk payload.
        // 当文件没有可见 hunk 载荷时渲染紧凑回退提示。
        if file.hunks.is_empty() {
            return container(text("当前文件暂无可展示的文本 Diff。").size(13))
                .padding([10, 12])
                .height(DIFF_EMPTY_BODY_HEIGHT)
                .width(Fill)
                .into();
        }

        // Split the file into hunk cards and omission cards so discontinuous change regions are visually stable.
        // 将文件拆成 hunk 卡片与省略卡片，使不连续的变更区在视觉上更稳定。
        let file_key = file.key();
        let mut hunk_flow = column![].spacing(1);
        let mut hunk_top_offset =
            file_top_offset + DIFF_FILE_HEADER_HEIGHT + DIFF_FILE_BODY_VERTICAL_INSET;

        for (hunk_index, hunk) in file.hunks.iter().enumerate() {
            if hunk_index > 0 {
                let omitted_lines = omitted_context_line_count(&file.hunks[hunk_index - 1], hunk);
                hunk_flow = hunk_flow.push(self.view_diff_gap_block(omitted_lines));
                hunk_top_offset += DIFF_HUNK_FLOW_SPACING;
                hunk_top_offset += DIFF_GAP_BLOCK_HEIGHT;
                hunk_top_offset += DIFF_HUNK_FLOW_SPACING;
            }

            hunk_flow = hunk_flow.push(self.view_diff_hunk_block(
                file,
                &file_key,
                hunk_index,
                hunk,
                hunk_top_offset,
            ));

            let wrapped_key = diff_hunk_buffer_key(&file_key, hunk_index);
            if let Some(wrapped_hunk) = self.diff_wrapped_hunks.get(&wrapped_key) {
                hunk_top_offset += wrapped_hunk.visual_lines.len() as f32 * CODE_EDITOR_LINE_HEIGHT;
            } else {
                hunk_top_offset += self.diff_hunk_block_height(&file_key, hunk_index, hunk);
            }
        }

        container(hunk_flow).padding([8, 10]).width(Fill).into()
    }
    /// Renders one omission block inserted between separated hunk editors.
    /// 渲染插入在分离 hunk 编辑器之间的省略块。
    /// Renders one omission block inserted between separated hunk editors.
    /// 渲染插入在分离 hunk 编辑器之间的省略块。
    pub(super) fn view_diff_gap_block(&self, omitted_lines: usize) -> Element<'_, Message> {
        let label = if omitted_lines > 0 {
            format!("▴ ▾   省略 {omitted_lines} 行未变化的内容")
        } else {
            String::from("▴ ▾   分隔下一个变更片区")
        };

        container(
            container(text(label).size(11))
                .center_x(Fill)
                .center_y(DIFF_GAP_BLOCK_HEIGHT)
                .height(DIFF_GAP_BLOCK_HEIGHT),
        )
        .width(Fill)
        .height(DIFF_GAP_BLOCK_HEIGHT)
        .style(style::diff_gap_block_style)
        .into()
    }
    /// Renders one hunk-level diff block with its own editor and gutter.
    /// 渲染带有独立编辑器与 gutter 的单个 hunk 级 diff 区块。
    pub(super) fn view_diff_hunk_block<'a>(
        &'a self,
        file: &'a DiffFile,
        file_key: &str,
        hunk_index: usize,
        _hunk: &'a crate::diff::DiffHunk,
        _hunk_top_offset: f32,
    ) -> Element<'a, Message> {
        let buffer_key = diff_hunk_buffer_key(file_key, hunk_index);
        let Some(wrapped_hunk) = self.diff_wrapped_hunks.get(&buffer_key) else {
            return space().height(0).into();
        };
        let total_line_count = wrapped_hunk.visual_lines.len();
        let block_height = total_line_count as f32 * CODE_EDITOR_LINE_HEIGHT;
        let Some(slice_range) = self.diff_buffer_slices.get(&buffer_key).copied() else {
            return space().height(block_height).into();
        };
        let line_count = slice_range.len();
        let visible_slice = VisibleDiffHunkSlice {
            range: slice_range,
            top_hidden_height: slice_range.start as f32 * CODE_EDITOR_LINE_HEIGHT,
            bottom_hidden_height: (total_line_count.saturating_sub(slice_range.end)) as f32
                * CODE_EDITOR_LINE_HEIGHT,
        };

        if line_count == 0 {
            return space().height(block_height).into();
        }

        let content = self
            .diff_code_buffers
            .get(&buffer_key)
            .expect("Diff hunk buffer must exist before rendering");
        let syntax_token = code_view::syntax_token(&file.path);
        let action_buffer_key = buffer_key.clone();
        let hover_buffer_key = buffer_key.clone();
        let editor_font = theme::code_font();
        let visible_height = line_count as f32 * CODE_EDITOR_LINE_HEIGHT;
        let gutter_block_width = diff_gutter_block_width();
        let slice_contains_truncated_lines = wrapped_hunk.truncated_line_count > 0
            && wrapped_hunk
                .visual_lines
                .iter()
                .skip(visible_slice.range.start)
                .take(line_count)
                .any(|line| line.is_truncated);
        let estimated_editor_width = ((wrapped_hunk.max_line_columns.max(1) as f32)
            * CODE_EDITOR_TEXT_SIZE
            * DIFF_CODE_GLYPH_WIDTH_RATIO)
            .clamp(
                DIFF_CODE_EDITOR_MIN_CONTENT_WIDTH,
                DIFF_CODE_EDITOR_MAX_CONTENT_WIDTH,
            );
        let mut line_bars = column![].spacing(0);
        let mut line_arrow_column = column![].spacing(0);
        let mut line_number_backgrounds = column![].spacing(0);
        let mut line_number_column = column![].spacing(0);
        let mut current_bar_kind = None;
        let mut current_bar_count = 0usize;
        let mut current_gutter_kind = None;
        let mut current_gutter_count = 0usize;
        let hover_anchor_indices = wrapped_hunk
            .visual_lines
            .iter()
            .skip(visible_slice.range.start)
            .take(line_count)
            .map(|line| line.anchor_index)
            .collect::<Vec<_>>();
        let hovered_line_index = self.hovered_diff_line.as_ref().and_then(|hovered| {
            if hovered.file_key == buffer_key {
                Some(hovered.line_index)
            } else {
                None
            }
        });
        for (visible_line_offset, visual_line) in wrapped_hunk
            .visual_lines
            .iter()
            .enumerate()
            .skip(visible_slice.range.start)
            .take(line_count)
        {
            let absolute_visual_line_index = visible_line_offset;
            if current_bar_kind == Some(visual_line.kind) {
                current_bar_count += 1;
            } else {
                if let Some(previous_kind) = current_bar_kind {
                    let run_height = current_bar_count as f32 * CODE_EDITOR_LINE_HEIGHT;

                    line_bars = line_bars.push(
                        container(space())
                            .width(3)
                            .height(run_height)
                            .style(move |theme| style::diff_line_bar_style(theme, previous_kind)),
                    );
                }

                current_bar_kind = Some(visual_line.kind);
                current_bar_count = 1;
            }

            if current_gutter_kind == Some(visual_line.kind) {
                current_gutter_count += 1;
            } else {
                if let Some(previous_kind) = current_gutter_kind {
                    let run_height = current_gutter_count as f32 * CODE_EDITOR_LINE_HEIGHT;

                    line_number_backgrounds = line_number_backgrounds.push(
                        container(space())
                            .width(gutter_block_width)
                            .height(run_height)
                            .style(move |theme| {
                                style::diff_gutter_background_style(theme, previous_kind)
                            }),
                    );
                }

                current_gutter_kind = Some(visual_line.kind);
                current_gutter_count = 1;
            }

            line_arrow_column = line_arrow_column.push(
                container(
                    text(
                        if hovered_line_index == Some(visual_line.anchor_index)
                            && visual_line.anchor_index == absolute_visual_line_index
                        {
                            "▶"
                        } else {
                            ""
                        },
                    )
                    .size(10)
                    .line_height(Pixels(CODE_EDITOR_LINE_HEIGHT))
                    .color(iced::color!(0x24292F)),
                )
                .center_x(DIFF_GUTTER_ARROW_WIDTH)
                .height(CODE_EDITOR_LINE_HEIGHT),
            );

            line_number_column = line_number_column.push(
                container(
                    text(format_diff_line_number(
                        visual_line.old_number,
                        visual_line.new_number,
                    ))
                    .size(CODE_EDITOR_TEXT_SIZE)
                    .line_height(Pixels(CODE_EDITOR_LINE_HEIGHT))
                    .font(editor_font)
                    .color(style::diff_line_number_color(visual_line.kind)),
                )
                .width(DIFF_GUTTER_WIDTH)
                .height(CODE_EDITOR_LINE_HEIGHT)
                .align_right(DIFF_GUTTER_WIDTH),
            );
        }

        if let Some(previous_kind) = current_bar_kind {
            let run_height = current_bar_count as f32 * CODE_EDITOR_LINE_HEIGHT;

            line_bars = line_bars.push(
                container(space())
                    .width(3)
                    .height(run_height)
                    .style(move |theme| style::diff_line_bar_style(theme, previous_kind)),
            );
        }

        if let Some(previous_kind) = current_gutter_kind {
            let run_height = current_gutter_count as f32 * CODE_EDITOR_LINE_HEIGHT;

            line_number_backgrounds = line_number_backgrounds.push(
                container(space())
                    .width(gutter_block_width)
                    .height(run_height)
                    .style(move |theme| style::diff_gutter_background_style(theme, previous_kind)),
            );
        }

        let file_editor = if slice_contains_truncated_lines {
            WheelPassthrough::new(
                text_editor(content)
                    .size(CODE_EDITOR_TEXT_SIZE)
                    .line_height(Pixels(CODE_EDITOR_LINE_HEIGHT))
                    .wrapping(iced::advanced::text::Wrapping::None)
                    .width(Pixels(estimated_editor_width))
                    .min_height(visible_height)
                    .max_height(visible_height)
                    .height(visible_height)
                    .padding(0)
                    .font(editor_font)
                    .style(style::diff_code_editor_style)
                    .on_action(move |action| {
                        Message::DiffCodeAction(action_buffer_key.clone(), action)
                    }),
            )
        } else {
            WheelPassthrough::new(
                text_editor(content)
                    .size(CODE_EDITOR_TEXT_SIZE)
                    .line_height(Pixels(CODE_EDITOR_LINE_HEIGHT))
                    .wrapping(iced::advanced::text::Wrapping::None)
                    .width(Pixels(estimated_editor_width))
                    .min_height(visible_height)
                    .max_height(visible_height)
                    .height(visible_height)
                    .padding(0)
                    .font(editor_font)
                    .style(style::diff_code_editor_style)
                    .highlight_with::<iced::highlighter::Highlighter>(
                        iced::highlighter::Settings {
                            theme: theme::code_theme(&self.app_theme),
                            token: syntax_token.clone(),
                        },
                        theme::code_highlight_format,
                    )
                    .on_action(move |action| {
                        Message::DiffCodeAction(action_buffer_key.clone(), action)
                    }),
            )
        };

        let gutter_layer = container(stack([
            container(line_number_backgrounds)
                .width(gutter_block_width)
                .height(visible_height)
                .into(),
            row![
                line_bars,
                line_number_column,
                line_arrow_column,
                container(space())
                    .width(2)
                    .height(Fill)
                    .style(style::diff_gutter_separator_style),
            ]
            .spacing(0)
            .width(gutter_block_width)
            .height(visible_height)
            .into(),
        ]))
        .width(gutter_block_width)
        .height(visible_height);

        let visible_row = mouse_area(
            row![
                gutter_layer,
                scrollable(
                    container(file_editor)
                        .padding([0, DIFF_CODE_EDITOR_HORIZONTAL_PADDING])
                        .height(visible_height)
                        .width(Shrink)
                )
                .direction(scrollable::Direction::Horizontal(
                    scrollable::Scrollbar::default(),
                ))
                .height(visible_height)
                .width(Fill),
            ]
            .spacing(0)
            .height(visible_height)
            .width(Fill),
        )
        .on_move({
            let buffer_key = hover_buffer_key.clone();
            let hover_anchor_indices = hover_anchor_indices.clone();

            move |position| {
                let raw_line_index =
                    hovered_diff_line_index_from_cursor(position.y, hover_anchor_indices.len());
                let anchor_line_index = hover_anchor_indices
                    .get(raw_line_index)
                    .copied()
                    .unwrap_or(raw_line_index);

                Message::DiffCodeHoverMoved(buffer_key.clone(), anchor_line_index)
            }
        })
        .on_exit(Message::DiffCodeHoverExited(hover_buffer_key));

        container(
            column![
                space().height(visible_slice.top_hidden_height),
                visible_row,
                space().height(visible_slice.bottom_hidden_height),
            ]
            .spacing(0)
            .width(Fill),
        )
        .height(block_height)
        .style(style::diff_block_style)
        .clip(true)
        .width(Fill)
        .into()
    }
}
