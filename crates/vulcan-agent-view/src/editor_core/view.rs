//! Lightweight viewport state adapted from Ferrite (MIT) for transcript scrolling.
//! 改编自 Ferrite（MIT）的轻量视口状态，用于转录滚动控制。

/// Number of extra logical lines rendered above and below the viewport.
/// 在视口上下额外渲染的逻辑行数量。
const OVERSCAN_LINES: usize = 5;

/// Fallback line height used before exact paragraph metrics become available.
/// 在精确段落度量可用前采用的回退行高。
const DEFAULT_LINE_HEIGHT: f32 = 20.0;

/// Minimal viewport state shared by the future selectable transcript widget.
/// 未来可选转录组件共享的最小视口状态。
#[derive(Debug, Clone, PartialEq)]
pub struct ViewState {
    /// First visible logical line at the top of the viewport.
    /// 位于视口顶部的首个可见逻辑行。
    first_visible_line: usize,
    /// Vertical sub-line scroll offset inside the first visible line.
    /// 首个可见逻辑行内部的垂直亚行滚动偏移。
    scroll_offset_y: f32,
    /// Current viewport height in pixels.
    /// 当前视口高度，单位为像素。
    viewport_height: f32,
    /// Current estimated logical line height in pixels.
    /// 当前估算的逻辑行高，单位为像素。
    line_height: f32,
}

impl Default for ViewState {
    /// Creates one default viewport state through the shared constructor.
    /// 通过统一构造函数创建一个默认视口状态。
    fn default() -> Self {
        Self::new()
    }
}

impl ViewState {
    /// Creates one empty viewport state with zero scroll and fallback metrics.
    /// 创建一个滚动为零且使用回退度量值的空视口状态。
    #[must_use]
    pub fn new() -> Self {
        Self {
            first_visible_line: 0,
            scroll_offset_y: 0.0,
            viewport_height: 0.0,
            line_height: DEFAULT_LINE_HEIGHT,
        }
    }

    /// Updates the viewport height after one container resize.
    /// 在容器尺寸变化后更新视口高度。
    pub fn update_viewport(&mut self, height: f32) {
        self.viewport_height = height.max(0.0);
    }

    /// Updates the estimated logical line height used by scrolling math.
    /// 更新滚动计算使用的逻辑行估算高度。
    pub fn set_line_height(&mut self, height: f32) {
        self.line_height = height.max(1.0);
    }

    /// Returns the visible line range expanded by a small overscan window.
    /// 返回向上下扩展了少量 overscan 的可见行区间。
    #[must_use]
    pub fn visible_line_range(&self, total_lines: usize) -> (usize, usize) {
        if total_lines == 0 {
            return (0, 0);
        }

        let visible_lines = (self.viewport_height / self.line_height).ceil() as usize;
        let start = self.first_visible_line.saturating_sub(OVERSCAN_LINES);
        let end = (self.first_visible_line + visible_lines + OVERSCAN_LINES).min(total_lines);
        (start, end)
    }

    /// Scrolls the viewport so the provided logical line becomes the first visible line.
    /// 滚动视口，使给定逻辑行成为首个可见行。
    pub fn scroll_to_line(&mut self, line: usize) {
        self.first_visible_line = line;
        self.scroll_offset_y = 0.0;
    }

    /// Scrolls the viewport by one pixel delta while clamping to the document height.
    /// 按像素增量滚动视口，并将结果钳制在文档高度范围内。
    pub fn scroll_by(&mut self, delta_y: f32, total_lines: usize) {
        if total_lines == 0 {
            self.first_visible_line = 0;
            self.scroll_offset_y = 0.0;
            return;
        }

        let max_scroll = self.max_scroll_offset(total_lines);
        let next_scroll = (self.current_scroll_y() + delta_y).clamp(0.0, max_scroll);
        self.scroll_to_absolute(next_scroll, total_lines);
    }

    /// Scrolls the viewport to the provided absolute vertical offset.
    /// 将视口滚动到给定的绝对垂直偏移位置。
    pub fn scroll_to_absolute(&mut self, absolute_scroll_y: f32, total_lines: usize) {
        if total_lines == 0 {
            self.first_visible_line = 0;
            self.scroll_offset_y = 0.0;
            return;
        }

        let clamped_scroll = absolute_scroll_y.clamp(0.0, self.max_scroll_offset(total_lines));
        self.first_visible_line = (clamped_scroll / self.line_height).floor() as usize;
        self.scroll_offset_y = clamped_scroll - (self.first_visible_line as f32 * self.line_height);
    }

    /// Returns the current absolute vertical scroll position from the document top.
    /// 返回从文档顶部开始计算的当前绝对垂直滚动位置。
    #[must_use]
    pub fn current_scroll_y(&self) -> f32 {
        (self.first_visible_line as f32 * self.line_height) + self.scroll_offset_y
    }

    /// Returns the first visible logical line index.
    /// 返回首个可见逻辑行索引。
    #[must_use]
    pub fn first_visible_line(&self) -> usize {
        self.first_visible_line
    }

    /// Returns the current viewport height in pixels.
    /// 返回当前视口高度，单位为像素。
    #[must_use]
    pub fn viewport_height(&self) -> f32 {
        self.viewport_height
    }

    /// Returns the current logical line height in pixels.
    /// 返回当前逻辑行高，单位为像素。
    #[must_use]
    pub fn line_height(&self) -> f32 {
        self.line_height
    }

    /// Returns the maximum legal absolute scroll offset for the current document.
    /// 返回当前文档允许的最大绝对滚动偏移。
    #[must_use]
    fn max_scroll_offset(&self, total_lines: usize) -> f32 {
        ((total_lines as f32 * self.line_height) - self.viewport_height).max(0.0)
    }
}

#[cfg(test)]
mod tests {
    //! Viewport-state regression tests for transcript scrolling behavior.
    //! 转录滚动行为使用的视口状态回归测试。

    use super::ViewState;

    /// Verifies pixel scrolling updates both visible line and sub-line offset.
    /// 验证像素滚动会同时更新可见行和亚行偏移。
    #[test]
    fn scrolls_by_pixel_delta() {
        let mut view_state = ViewState::new();
        view_state.update_viewport(100.0);
        view_state.set_line_height(20.0);

        view_state.scroll_by(35.0, 20);

        assert_eq!(view_state.first_visible_line(), 1);
        assert!((view_state.current_scroll_y() - 35.0).abs() < 0.001);
    }

    /// Verifies visible line ranges include the expected overscan rows.
    /// 验证可见行区间会包含预期的 overscan 行。
    #[test]
    fn reports_visible_line_range_with_overscan() {
        let mut view_state = ViewState::new();
        view_state.update_viewport(100.0);
        view_state.set_line_height(20.0);
        view_state.scroll_to_line(10);

        assert_eq!(view_state.visible_line_range(40), (5, 20));
    }
}
