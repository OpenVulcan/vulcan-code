// Text layout helpers: width-aware wrapping, truncation, shimmer and spinners.
// 文本布局辅助：按显示宽度换行、截断、微光效果与旋转指示器。

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Widget};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::theme::mix;

/// Wraps source text by display columns without trimming indentation or internal spaces.
/// 按显示列对来源文本换行，不裁剪缩进或内部空格。
/// `source` is normalized document text; `width` bounds each line; `style` colors literal content.
/// `source` 为规范化文档文本；`width` 限定每行宽度；`style` 设置字面正文颜色。
/// Returns owned lines; tabs expand to four spaces and unprintable controls are omitted.
/// 返回持有的行；制表符展开为四个空格，不可打印控制字符省略。
pub fn wrap_verbatim(source: &str, width: usize, style: Style) -> Vec<Line<'static>> {
    if width == 0 {
        return Vec::new();
    }
    // Keep empty source lines independently from word wrapping.
    // 保留空来源行，不依赖单词换行。
    let mut lines = Vec::new();
    for raw in source.lines() {
        // One physical display line and its occupied column count.
        // 单个物理显示行及其占用列数。
        let mut text = String::new();
        // Width of printable characters already in this physical line.
        // 此物理行内已有可打印字符的宽度。
        let mut used = 0;
        for character in raw
            .replace('\t', "    ")
            .chars()
            .filter(|character| !character.is_control())
        {
            // Wide characters are indivisible in terminal cells.
            // 宽字符在终端单元格中不可拆分。
            let character_width = character.width().unwrap_or(0);
            if used + character_width > width && !text.is_empty() {
                lines.push(Line::from(Span::styled(std::mem::take(&mut text), style)));
                used = 0;
            }
            if character_width > width {
                // A one-column viewport cannot represent a two-column character.
                // 单列视口无法表示双列字符。
                text.push('�');
                used += 1;
            } else {
                text.push(character);
                used += character_width;
            }
        }
        lines.push(Line::from(Span::styled(text, style)));
    }
    lines
}

pub fn width_of(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// Clears a popup's area. A wide character that starts just left of `area` would otherwise spill
/// into its first column and hide the popup's left border, so that character is blanked too.
/// 清空浮层区域。起始于 `area` 左侧紧邻列的宽字符会溢入其首列并遮住浮层左边框，因此一并置空。
pub fn clear_area(area: Rect, buf: &mut Buffer) {
    Clear.render(area, buf);
    let Some(left) = area.x.checked_sub(1) else {
        return;
    };
    for y in area.top()..area.bottom() {
        if let Some(cell) = buf.cell_mut((left, y))
            && width_of(cell.symbol()) > 1
        {
            cell.set_symbol(" ");
        }
    }
}

/// Truncates to `max` display columns, appending `…` when cut.
/// 截断到 `max` 显示列，被截断时追加 `…`。
pub fn truncate(text: &str, max: usize) -> String {
    if width_of(text) <= max {
        return text.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let w = ch.width().unwrap_or(0);
        if used + w > max.saturating_sub(1) {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}

/// Pads `text` with spaces to exactly `width` columns (truncating when longer).
/// 用空格将 `text` 补齐到恰好 `width` 列（过长时截断）。
pub fn fit(text: &str, width: usize) -> String {
    let cut = truncate(text, width);
    let pad = width.saturating_sub(width_of(&cut));
    format!("{cut}{}", " ".repeat(pad))
}

/// Label of the chip that collects the items a one-line strip has no room for.
/// 收纳单行按钮条放不下的项目的按钮标签。
pub const OVERFLOW_CHIP: &str = " ⋯ ";

/// Chooses which items of a one-line strip are shown; the rest fold into an overflow chip.
/// 选择单行按钮条中显示哪些项目；其余项目收进溢出按钮。
/// `widths` are item widths in order, `gap` the space between items, `room` the line width,
/// `more` the overflow chip width and `keep` an item that must stay visible (the active one).
/// Returns the shown indices in their original order; all of them when everything fits.
/// `widths` 为按顺序排列的项目宽度，`gap` 为项目间距，`room` 为行宽，`more` 为溢出按钮宽度，
/// `keep` 为必须保持可见的项目（当前项）。返回按原顺序排列的显示序号；全部放得下时返回全部。
pub fn fit_strip(
    widths: &[usize],
    gap: usize,
    room: usize,
    more: usize,
    keep: Option<usize>,
) -> Vec<usize> {
    let total = widths.iter().sum::<usize>() + gap * widths.len().saturating_sub(1);
    if total <= room {
        return (0..widths.len()).collect();
    }
    let keep = keep.filter(|k| *k < widths.len());
    let mut used = more + keep.map_or(0, |k| widths[k] + gap);
    let mut shown: Vec<usize> = keep.into_iter().collect();
    for (index, width) in widths.iter().enumerate() {
        if Some(index) == keep {
            continue;
        }
        if used + width + gap > room {
            break;
        }
        used += width + gap;
        shown.push(index);
    }
    shown.sort_unstable();
    shown
}

struct Token {
    text: String,
    style: Style,
    width: usize,
    space: bool,
    newline: bool,
}

fn tokenize(spans: &[Span<'_>]) -> Vec<Token> {
    let mut tokens: Vec<Token> = Vec::new();
    for span in spans {
        for ch in span.content.chars() {
            if ch == '\n' {
                tokens.push(Token {
                    text: String::new(),
                    style: span.style,
                    width: 0,
                    space: false,
                    newline: true,
                });
                continue;
            }
            let w = ch.width().unwrap_or(0);
            let space = ch == ' ' || ch == '\t';
            let ch = if ch == '\t' { ' ' } else { ch };
            let joinable = w == 1
                && tokens.last().is_some_and(|last| {
                    !last.newline
                        && last.space == space
                        && last.style == span.style
                        && last.width > 0
                        && last
                            .text
                            .chars()
                            .last()
                            .is_some_and(|c| c.width() == Some(1))
                });
            if joinable {
                let last = tokens.last_mut().expect("checked above");
                last.text.push(ch);
                last.width += w;
            } else {
                tokens.push(Token {
                    text: ch.to_string(),
                    style: span.style,
                    width: w,
                    space,
                    newline: false,
                });
            }
        }
    }
    tokens
}

/// Wraps styled spans to `width` columns. `first` prefixes the first line and `rest` every
/// continuation line (both count toward the width).
/// 将带样式的片段按 `width` 列换行。`first` 作为首行前缀，`rest` 作为后续行前缀（均计入宽度）。
pub fn wrap(
    spans: &[Span<'_>],
    width: usize,
    first: &[Span<'static>],
    rest: &[Span<'static>],
) -> Vec<Line<'static>> {
    let mut builder = LineBuilder::new(width, first, rest);
    for token in tokenize(spans) {
        if token.newline {
            builder.flush();
            continue;
        }
        if token.space && !builder.has_content {
            continue;
        }
        if builder.used + token.width > builder.limit && builder.has_content {
            builder.flush();
            if token.space {
                continue;
            }
        }
        if builder.used + token.width > builder.limit {
            // A single token wider than the line: hard-split by characters.
            // 单个词宽于整行：按字符硬切分。
            for ch in token.text.chars() {
                let w = ch.width().unwrap_or(0);
                if builder.used + w > builder.limit && builder.has_content {
                    builder.flush();
                }
                builder.push(ch.to_string(), token.style, w);
            }
            continue;
        }
        builder.push(token.text, token.style, token.width);
    }
    builder.finish()
}

struct LineBuilder<'p> {
    width: usize,
    rest: &'p [Span<'static>],
    lines: Vec<Line<'static>>,
    current: Vec<Span<'static>>,
    prefix_len: usize,
    used: usize,
    limit: usize,
    has_content: bool,
}

impl<'p> LineBuilder<'p> {
    fn new(width: usize, first: &[Span<'static>], rest: &'p [Span<'static>]) -> Self {
        let used = first.iter().map(|s| width_of(&s.content)).sum::<usize>();
        LineBuilder {
            width,
            rest,
            lines: Vec::new(),
            current: first.to_vec(),
            prefix_len: first.len(),
            used,
            limit: width.max(used + 1),
            has_content: false,
        }
    }

    fn push(&mut self, text: String, style: Style, width: usize) {
        self.used += width;
        self.has_content = true;
        let own_spans = self.current.len() > self.prefix_len;
        match self.current.last_mut() {
            Some(last) if own_spans && last.style == style => {
                last.content.to_mut().push_str(&text);
            }
            _ => self.current.push(Span::styled(text, style)),
        }
    }

    fn flush(&mut self) {
        if self.current.len() > self.prefix_len
            && let Some(last) = self.current.last_mut()
        {
            let trimmed = last.content.trim_end().to_string();
            *last.content.to_mut() = trimmed;
        }
        self.lines
            .push(Line::from(std::mem::take(&mut self.current)));
        self.current = self.rest.to_vec();
        self.prefix_len = self.rest.len();
        self.used = self.rest.iter().map(|s| width_of(&s.content)).sum();
        self.limit = self.width.max(self.used + 1);
        self.has_content = false;
    }

    fn finish(mut self) -> Vec<Line<'static>> {
        if self.has_content || self.lines.is_empty() {
            self.flush();
        }
        self.lines
    }
}

/// Per-character shimmer: a bright band sweeps across `text` as `phase` advances.
/// 逐字符微光：随 `phase` 推进，一条亮带扫过 `text`。
pub fn shimmer(text: &str, base: Color, highlight: Color, phase: f32) -> Vec<Span<'static>> {
    let count = text.chars().count().max(1) as f32;
    let span = count + 8.0;
    let center = (phase * 14.0) % span - 4.0;
    text.chars()
        .enumerate()
        .map(|(i, ch)| {
            let distance = (i as f32 - center).abs();
            let intensity = (1.0 - distance / 3.5).clamp(0.0, 1.0);
            Span::styled(
                ch.to_string(),
                Style::new().fg(mix(highlight, base, intensity)),
            )
        })
        .collect()
}

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn spinner(phase: f32) -> &'static str {
    SPINNER[((phase * 12.0) as usize) % SPINNER.len()]
}

/// Formats a token count as `950`, `12.4k`, `1.2M`.
/// 将 token 数格式化为 `950`、`12.4k`、`1.2M`。
pub fn compact_count(value: u64) -> String {
    match value {
        0..=999 => value.to_string(),
        1_000..=999_999 => format!("{:.1}k", value as f64 / 1_000.0),
        _ => format!("{:.1}M", value as f64 / 1_000_000.0),
    }
}

/// Formats a duration in milliseconds as `850ms`, `12s`, `3m 05s`.
/// 将毫秒时长格式化为 `850ms`、`12s`、`3m 05s`。
pub fn compact_duration(ms: u64) -> String {
    match ms {
        0..=999 => format!("{ms}ms"),
        1_000..=59_999 => format!("{:.0}s", ms as f64 / 1000.0),
        _ => format!("{}m {:02}s", ms / 60_000, (ms / 1000) % 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_of(line: &Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    /// Literal documents retain indentation, blank lines and consecutive spaces under wrapping.
    /// 字面文档换行后保留缩进、空行及连续空格。
    #[test]
    fn document_wrapping_preserves_whitespace_and_display_width() {
        // Printable source text includes a wide character and a tab-indented code row.
        // 可打印来源文本包含宽字符及制表符缩进代码行。
        let source = "  ab  中文\n\n\tcode";
        for width in [40, 7] {
            // Rendered physical rows preserve the original sequence apart from explicit wrapping.
            // 渲染物理行除明确换行外保留原始序列。
            let lines = wrap_verbatim(source, width, Style::default());
            assert!(lines.iter().all(|line| width_of(&text_of(line)) <= width));
            assert_eq!(
                lines.iter().map(text_of).collect::<String>(),
                "  ab  中文    code"
            );
            assert!(lines.iter().any(|line| text_of(line).is_empty()));
        }
    }

    #[test]
    fn wraps_words_and_cjk_within_width() {
        let spans = [Span::raw("hello world 你好世界 again")];
        let lines = wrap(&spans, 10, &[], &[]);
        for line in &lines {
            assert!(width_of(&text_of(line)) <= 10, "{:?}", text_of(line));
        }
        let joined: String = lines.iter().map(text_of).collect::<Vec<_>>().join("|");
        assert_eq!(joined, "hello|world 你好|世界 again");
    }

    #[test]
    fn continuation_lines_use_rest_prefix() {
        let spans = [Span::raw("aaa bbb ccc")];
        let lines = wrap(&spans, 6, &[Span::raw("> ")], &[Span::raw("  ")]);
        let texts: Vec<String> = lines.iter().map(text_of).collect();
        assert_eq!(texts, vec!["> aaa", "  bbb", "  ccc"]);
    }

    /// A strip shows everything when it fits; otherwise it keeps a prefix plus the kept item and
    /// leaves room for the overflow chip, never exceeding the line.
    /// 放得下时按钮条全部显示；否则保留前缀与必须可见项，并为溢出按钮留出空间，不超出行宽。
    #[test]
    fn strip_folds_what_does_not_fit_and_keeps_the_active_item() {
        let widths = [6, 6, 6, 6];
        assert_eq!(fit_strip(&widths, 1, 27, 3, None), vec![0, 1, 2, 3]);
        let shown = fit_strip(&widths, 1, 20, 3, None);
        assert_eq!(shown, vec![0, 1]);
        let shown = fit_strip(&widths, 1, 20, 3, Some(3));
        assert_eq!(shown, vec![0, 3]);
        let used: usize = shown.iter().map(|i| widths[*i] + 1).sum::<usize>() + 3;
        assert!(used <= 20);
    }

    #[test]
    fn truncates_by_display_width() {
        assert_eq!(truncate("你好世界", 5), "你好…");
        assert_eq!(fit("ab", 4), "ab  ");
    }
}
