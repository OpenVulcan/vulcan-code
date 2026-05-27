//! Diff view state extracted from the main workbench module.
//! 从主工作台模块中抽取的 Diff 视图状态。

use super::*;

/// Normalized diff visual-line slice range clipped to the total available line count.
/// 按总可用行数裁剪后的标准化 Diff 可视行切片范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DiffVisualSliceRange {
    /// Inclusive start index of the normalized slice.
    /// 标准化切片的起始索引（包含）。
    pub(super) start: usize,
    /// Exclusive end index of the normalized slice.
    /// 标准化切片的结束索引（不包含）。
    pub(super) end: usize,
}

impl DiffVisualSliceRange {
    /// Builds one normalized slice range constrained by the total visual-line count.
    /// 根据总可视行数量构建一个规范化切片范围。
    pub(super) fn new(start: usize, end: usize, total_lines: usize) -> Self {
        let bounded_start = start.min(total_lines);
        let bounded_end = end.max(bounded_start).min(total_lines);

        Self {
            start: bounded_start,
            end: bounded_end,
        }
    }

    /// Returns the number of visual lines covered by the slice.
    /// 返回该切片覆盖的可视行数量。
    pub(super) fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }
}

/// Visible virtualized range of diff file cards inside the current review viewport.
/// 当前审查视口内可见的虚拟化 Diff 文件卡片范围。
#[derive(Debug, Clone, Copy)]
pub(super) struct VisibleDiffFileRange {
    /// Inclusive start index of the visible file-card slice.
    /// 可见文件卡片切片的起始索引（包含）。
    pub(super) start: usize,
    /// Exclusive end index of the visible file-card slice.
    /// 可见文件卡片切片的结束索引（不包含）。
    pub(super) end: usize,
    /// Height spacer inserted before the first rendered file card.
    /// 插入在首个渲染文件卡片之前的高度占位。
    pub(super) top_spacer_height: f32,
    /// Height spacer inserted after the last rendered file card.
    /// 插入在最后一个渲染文件卡片之后的高度占位。
    pub(super) bottom_spacer_height: f32,
}

/// One visible hunk slice together with the hidden-height spacers surrounding it.
/// 单个可见 hunk 切片及其上下隐藏高度占位描述。
#[derive(Debug, Clone, Copy)]
pub(super) struct VisibleDiffHunkSlice {
    /// Visual-line slice range currently rendered inside the hunk editor.
    /// 当前在 hunk 编辑器中渲染的可视行切片范围。
    pub(super) range: DiffVisualSliceRange,
    /// Hidden height above the visible slice inside the full hunk block.
    /// 整个 hunk 区块内位于可视切片上方的隐藏高度。
    pub(super) top_hidden_height: f32,
    /// Hidden height below the visible slice inside the full hunk block.
    /// 整个 hunk 区块内位于可视切片下方的隐藏高度。
    pub(super) bottom_hidden_height: f32,
}

/// Loaded diff preview payload for the testing region.
/// 用于测试区域的已加载 diff 预览载荷。
#[derive(Debug, Clone)]
pub(super) struct DiffPreviewPayload {
    /// Footer and toast-like status message after the scan finishes.
    /// 扫描完成后展示在底栏中的状态消息。
    pub(super) status_message: String,
    /// Human-readable summary for the diff preview header.
    /// 供 diff 预览头部展示的人类可读摘要。
    pub(super) summary: String,
    /// Structured diff panel payload used by the workbench.
    /// 提供给工作台使用的结构化 diff 面板载荷。
    pub(super) panel: WorkspaceDiffPanel,
    /// Number of changed entries detected in the workspace.
    /// 在工作区中检测到的变更条目数量。
    pub(super) changed_files: usize,
    /// Activity level used to push the scan result into the runtime log.
    /// 用于将扫描结果写入运行日志的活动级别。
    pub(super) activity_level: ActivityLevel,
}

/// Active diff file descriptor derived from the current review scroll position.
/// 根据当前审查滚动位置推导出的激活 diff 文件描述。
#[derive(Debug, Clone, Copy)]
pub(super) struct ActiveDiffFile<'a> {
    /// Active file payload currently closest to the top of the review viewport.
    /// 当前最接近审查视口顶部的激活文件载荷。
    pub(super) file: &'a DiffFile,
    /// Whether the active file card is currently collapsed.
    /// 当前激活文件卡片是否处于折叠状态。
    pub(super) is_collapsed: bool,
}

/// Semantic icon kind rendered inside one custom window control button.
/// 渲染在单个自定义窗口控制按钮中的语义图标类型。
#[derive(Debug, Clone, Copy)]
pub(super) enum WindowControlIcon {
    /// Minimize-window affordance icon.
    /// 最小化窗口操作图标。
    Minimize,
    /// Maximize-window affordance icon.
    /// 最大化窗口操作图标。
    Maximize,
    /// Restore-window affordance icon displayed while maximized.
    /// 窗口已最大化时显示的还原操作图标。
    Restore,
    /// Close-window affordance icon.
    /// 关闭窗口操作图标。
    Close,
}

impl VulcanWorkbench {
    /// Rebuilds the cached diff hunk payloads and refreshes visible buffers.
    /// 重建缓存 Diff hunk 载荷，并刷新可见缓冲区。
    pub(super) fn rebuild_diff_code_buffers(&mut self) {
        self.diff_wrapped_hunks = rebuild_wrapped_diff_hunks(&self.diff_panel);
        self.refresh_visible_diff_buffers();
    }

    /// Refreshes only the diff editor buffers currently needed by the visible virtualized viewport.
    /// 仅刷新当前虚拟化视口真正需要的 Diff 编辑器缓冲区。
    pub(super) fn refresh_visible_diff_buffers(&mut self) {
        let visible_file_range = self.visible_diff_file_range();
        let mut next_buffers = BTreeMap::new();
        let mut next_slices = BTreeMap::new();

        for file_index in visible_file_range.start..visible_file_range.end {
            let Some(file) = self.diff_panel.files.get(file_index) else {
                continue;
            };

            if self.collapsed_diff_files.contains(&file.key()) {
                continue;
            }

            let file_top_offset = self.diff_file_top_offset(file_index);
            let mut hunk_top_offset =
                file_top_offset + DIFF_FILE_HEADER_HEIGHT + DIFF_FILE_BODY_VERTICAL_INSET;
            let file_key = file.key();

            for (hunk_index, hunk) in file.hunks.iter().enumerate() {
                if hunk_index > 0 {
                    hunk_top_offset += DIFF_HUNK_FLOW_SPACING;
                    hunk_top_offset += DIFF_GAP_BLOCK_HEIGHT;
                    hunk_top_offset += DIFF_HUNK_FLOW_SPACING;
                }

                let wrapped_key = diff_hunk_buffer_key(&file_key, hunk_index);
                let Some(wrapped_hunk) = self.diff_wrapped_hunks.get(&wrapped_key) else {
                    hunk_top_offset += self.diff_hunk_block_height(&file_key, hunk_index, hunk);
                    continue;
                };

                let visible_slice =
                    self.visible_diff_hunk_slice(hunk_top_offset, wrapped_hunk.visual_lines.len());

                if visible_slice.range.len() > 0 {
                    next_slices.insert(wrapped_key.clone(), visible_slice.range);
                    next_buffers.insert(
                        wrapped_key.clone(),
                        build_diff_slice_buffer(wrapped_hunk, visible_slice.range),
                    );
                }

                hunk_top_offset += wrapped_hunk.visual_lines.len() as f32 * CODE_EDITOR_LINE_HEIGHT;
            }
        }

        self.diff_code_buffers = next_buffers;
        self.diff_buffer_slices = next_slices;
    }

    /// Returns the current diff viewport height, preferring the real scroll viewport and falling back to layout estimates.
    /// 返回当前 Diff 视口高度，优先使用真实滚动视口高度，不足时回退到布局估算。
    pub(super) fn current_diff_viewport_height(&self) -> f32 {
        let dedicated_window_height = if self.diff_window_id.is_some() {
            (self.diff_window_size.height - 180.0).max(240.0)
        } else {
            0.0
        };
        let main_panel_height = (self.main_window_size.height * 0.42).max(220.0);

        self.diff_viewport_height
            .max(dedicated_window_height.max(main_panel_height))
    }

    /// Returns the overscanned vertical window used by diff virtualization.
    /// 返回 Diff 虚拟化使用的带预加载区域的纵向窗口。
    pub(super) fn diff_virtual_window(&self) -> (f32, f32) {
        let viewport_height = self.current_diff_viewport_height();
        let start = (self.diff_scroll_offset - DIFF_FILE_VIRTUAL_OVERSCAN).max(0.0);
        let end = self.diff_scroll_offset + viewport_height + DIFF_FILE_VIRTUAL_OVERSCAN;

        (start, end)
    }

    /// Resolves the currently visible file-card range for virtualized diff rendering.
    /// 解析当前虚拟化 Diff 渲染需要展示的文件卡片范围。
    pub(super) fn visible_diff_file_range(&self) -> VisibleDiffFileRange {
        if self.diff_panel.files.is_empty() {
            return VisibleDiffFileRange {
                start: 0,
                end: 0,
                top_spacer_height: 0.0,
                bottom_spacer_height: 0.0,
            };
        }

        let (window_top, window_bottom) = self.diff_virtual_window();
        let mut running_offset = 0.0;
        let mut first_visible = None;
        let mut last_visible = None;
        let mut last_file_bottom = 0.0;

        for (index, file) in self.diff_panel.files.iter().enumerate() {
            let file_top = running_offset;
            let file_bottom = file_top + self.diff_file_card_height(file);

            if file_bottom >= window_top && file_top <= window_bottom {
                first_visible.get_or_insert(index);
                last_visible = Some(index);
            }

            last_file_bottom = file_bottom;
            running_offset = file_bottom;

            if index + 1 < self.diff_panel.files.len() {
                running_offset += DIFF_FILE_CARD_SPACING;
            }
        }

        let (start, end) = match (first_visible, last_visible) {
            (Some(start), Some(end)) => (start, end + 1),
            _ => {
                let fallback_index = self.diff_panel.files.len().saturating_sub(1);
                (fallback_index, fallback_index + 1)
            }
        };
        let top_spacer_height = self.diff_file_top_offset(start);
        let bottom_spacer_height = (last_file_bottom
            - (self.diff_file_top_offset(end - 1)
                + self.diff_file_card_height(&self.diff_panel.files[end - 1])))
        .max(0.0);

        VisibleDiffFileRange {
            start,
            end,
            top_spacer_height,
            bottom_spacer_height,
        }
    }

    /// Resolves the visible virtualized slice for one wrapped hunk block under the current viewport.
    /// 解析当前视口下单个已折行 hunk 区块的可见虚拟化切片。
    pub(super) fn visible_diff_hunk_slice(
        &self,
        hunk_top_offset: f32,
        total_visual_lines: usize,
    ) -> VisibleDiffHunkSlice {
        let total_height = total_visual_lines as f32 * CODE_EDITOR_LINE_HEIGHT;

        if total_visual_lines == 0 {
            return VisibleDiffHunkSlice {
                range: DiffVisualSliceRange::new(0, 0, 0),
                top_hidden_height: 0.0,
                bottom_hidden_height: 0.0,
            };
        }

        let (window_top, window_bottom) = self.diff_virtual_window();
        let hunk_bottom_offset = hunk_top_offset + total_height;

        if hunk_bottom_offset < window_top || hunk_top_offset > window_bottom {
            return VisibleDiffHunkSlice {
                range: DiffVisualSliceRange::new(0, 0, total_visual_lines),
                top_hidden_height: 0.0,
                bottom_hidden_height: total_height,
            };
        }

        let visible_top = (window_top - hunk_top_offset).max(0.0);
        let visible_bottom = (window_bottom - hunk_top_offset).min(total_height).max(0.0);
        let start_line = ((visible_top / CODE_EDITOR_LINE_HEIGHT).floor() as usize)
            .saturating_sub(DIFF_HUNK_VIRTUAL_OVERSCAN_LINES);
        let end_line = ((visible_bottom / CODE_EDITOR_LINE_HEIGHT).ceil() as usize)
            .saturating_add(DIFF_HUNK_VIRTUAL_OVERSCAN_LINES)
            .min(total_visual_lines);
        let range = DiffVisualSliceRange::new(start_line, end_line, total_visual_lines);
        let top_hidden_height = range.start as f32 * CODE_EDITOR_LINE_HEIGHT;
        let bottom_hidden_height =
            (total_visual_lines.saturating_sub(range.end)) as f32 * CODE_EDITOR_LINE_HEIGHT;

        VisibleDiffHunkSlice {
            range,
            top_hidden_height,
            bottom_hidden_height,
        }
    }

    /// Toggles one diff file between collapsed and expanded state.
    /// 在折叠与展开状态之间切换单个 diff 文件。
    pub(super) fn toggle_diff_file_collapsed(&mut self, file_key: &str) {
        if self.collapsed_diff_files.contains(file_key) {
            self.collapsed_diff_files.remove(file_key);
        } else {
            self.collapsed_diff_files.insert(file_key.to_owned());
        }

        self.hovered_diff_line = None;
        self.refresh_visible_diff_buffers();
    }

    /// Resolves the currently active diff file from the review scroll offset.
    /// 根据审查滚动偏移量解析当前激活的 diff 文件。
    pub(super) fn active_diff_file(&self) -> Option<ActiveDiffFile<'_>> {
        let mut running_offset = 0.0;
        let target_offset = self.diff_scroll_offset.max(0.0) + 1.0;
        let mut last_candidate = None;

        for (index, file) in self.diff_panel.files.iter().enumerate() {
            if index > 0 {
                running_offset += DIFF_FILE_CARD_SPACING;
            }

            let file_top_offset = running_offset;
            let file_height = self.diff_file_card_height(file);
            let file_key = file.key();
            let is_collapsed = self.collapsed_diff_files.contains(&file_key);

            let candidate = ActiveDiffFile { file, is_collapsed };
            last_candidate = Some(candidate);

            if target_offset < file_top_offset + file_height {
                return Some(candidate);
            }

            running_offset += file_height;
        }

        last_candidate
    }

    /// Resolves the vertical scroll offset pointing at the top edge of one diff file card.
    /// 解析指向单个 diff 文件卡片顶部边缘的纵向滚动偏移量。
    pub(super) fn diff_file_scroll_offset(&self, target_file_key: &str) -> Option<f32> {
        let mut running_offset = 0.0;

        for (index, file) in self.diff_panel.files.iter().enumerate() {
            if index > 0 {
                running_offset += DIFF_FILE_CARD_SPACING;
            }

            if file.key() == target_file_key {
                return Some(running_offset);
            }

            running_offset += self.diff_file_card_height(file);
        }

        None
    }

    /// Resolves the top edge offset of one diff file card inside the virtualized review flow.
    /// 解析虚拟化审查流中单个 Diff 文件卡片的顶部偏移。
    pub(super) fn diff_file_top_offset(&self, target_index: usize) -> f32 {
        let mut running_offset = 0.0;

        for (index, file) in self.diff_panel.files.iter().enumerate() {
            if index == target_index {
                return running_offset;
            }

            running_offset += self.diff_file_card_height(file);

            if index + 1 < self.diff_panel.files.len() {
                running_offset += DIFF_FILE_CARD_SPACING;
            }
        }

        running_offset
    }

    /// Estimates the rendered height of one diff file card in the current layout.
    /// 估算当前布局下单个 diff 文件卡片的渲染高度。
    pub(super) fn diff_file_card_height(&self, file: &DiffFile) -> f32 {
        let is_collapsed = self.collapsed_diff_files.contains(&file.key());

        DIFF_FILE_HEADER_HEIGHT
            + if is_collapsed {
                0.0
            } else {
                self.diff_file_body_height(file)
            }
    }

    /// Estimates the rendered body height of one expanded diff file card.
    /// 估算单个展开 diff 文件卡片主体的渲染高度。
    pub(super) fn diff_file_body_height(&self, file: &DiffFile) -> f32 {
        if file.hunks.is_empty() {
            return DIFF_EMPTY_BODY_HEIGHT;
        }

        let file_key = file.key();
        let hunk_count = file.hunks.len();
        let gap_count = hunk_count.saturating_sub(1);
        let block_count = hunk_count + gap_count;
        let hunk_heights = file
            .hunks
            .iter()
            .enumerate()
            .map(|(hunk_index, hunk)| self.diff_hunk_block_height(&file_key, hunk_index, hunk))
            .sum::<f32>();
        let gap_heights = gap_count as f32 * DIFF_GAP_BLOCK_HEIGHT;
        let inter_block_spacing = block_count.saturating_sub(1) as f32 * DIFF_HUNK_FLOW_SPACING;

        DIFF_FILE_BODY_VERTICAL_PADDING + hunk_heights + gap_heights + inter_block_spacing
    }

    /// Estimates the rendered height of one hunk block after wrapping under the current column budget.
    /// 估算当前列预算下单个 hunk 区块折行后的渲染高度。
    pub(super) fn diff_hunk_block_height(
        &self,
        file_key: &str,
        hunk_index: usize,
        hunk: &crate::diff::DiffHunk,
    ) -> f32 {
        self.diff_wrapped_hunks
            .get(&diff_hunk_buffer_key(file_key, hunk_index))
            .map(|wrapped_hunk| wrapped_hunk.visual_lines.len() as f32 * CODE_EDITOR_LINE_HEIGHT)
            .unwrap_or_else(|| {
                code_view::wrap_diff_hunk(hunk, 0).visual_lines.len() as f32
                    * CODE_EDITOR_LINE_HEIGHT
            })
    }
}
