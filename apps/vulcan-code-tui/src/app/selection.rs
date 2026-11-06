// Text selection in the transcript, made by dragging the left button, and the copies it produces.
// 会话记录中按住左键拖动得到的文本选择，以及由此产生的复制内容。
//
// Mouse capture keeps the terminal from selecting text itself, so the client does it. Positions are
// `(layout line, display column)` of the transcript content, so scrolling keeps a selection on the
// same text; a selection belongs to the conversation and content width it was made at, and any
// other one hides it. A copy is queued here and written by the event loop as OSC 52, which
// terminals such as Windows Terminal, iTerm2, kitty, WezTerm and tmux put on the system clipboard.
// 鼠标捕获使终端无法自行选择文本，因此由客户端完成。位置是会话记录内容中的 `(排版行, 显示列)`，
// 滚动时选择仍停留在同一段文本上；选择属于创建时的会话与内容宽度，换成其他会话或宽度时隐藏。复制
// 内容在此排队，由事件循环以 OSC 52 写出，Windows Terminal、iTerm2、kitty、WezTerm 与 tmux 等终端会
// 将其放入系统剪贴板。

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ratatui::text::Line;
use unicode_width::UnicodeWidthChar;

use crate::model::ConversationId;

/// One cell of the transcript content: layout line and display column.
/// 会话记录内容中的一个单元格：排版行与显示列。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Point {
    pub line: usize,
    pub column: usize,
}

/// A selection from the cell where the button went down to the cell under the pointer.
/// 从按下按键的单元格到指针所在单元格的选择。
#[derive(Debug, Clone)]
pub struct TextSelection {
    pub conversation: ConversationId,
    /// Content width the layout had when the selection was made.
    /// 创建选择时排版的内容宽度。
    pub width: usize,
    pub anchor: Point,
    pub head: Point,
    /// The button is still held.
    /// 按键仍处于按下状态。
    pub dragging: bool,
}

impl TextSelection {
    /// First and last selected cells, both inclusive; `None` while the pointer has not left the
    /// cell it pressed, so a plain click selects nothing.
    /// 首个与末个选中单元格（均包含）；指针尚未离开按下的单元格时为 `None`，因此普通点击不选中任何内容。
    pub fn span(&self) -> Option<(Point, Point)> {
        (self.anchor != self.head).then(|| (self.anchor.min(self.head), self.anchor.max(self.head)))
    }

    /// Columns `[from, to)` selected on `line`, whose text is `line_width` columns wide.
    /// `line` 上被选中的列 `[from, to)`，该行文本宽 `line_width` 列。
    pub fn columns(&self, line: usize, line_width: usize) -> Option<(usize, usize)> {
        let (start, end) = self.span()?;
        if line < start.line || line > end.line {
            return None;
        }
        let from = if line == start.line { start.column } else { 0 };
        let to = if line == end.line {
            (end.column + 1).min(line_width)
        } else {
            line_width
        };
        (from < to).then_some((from, to))
    }

    /// Selected text of the rendered `lines`, one line per row with trailing blanks removed.
    /// 已渲染 `lines` 中选中的文本，每行一段并去掉行尾空白。
    pub fn text(&self, lines: &[Line]) -> String {
        let Some((start, end)) = self.span() else {
            return String::new();
        };
        if lines.is_empty() {
            return String::new();
        }
        (start.line..=end.line.min(lines.len() - 1))
            .map(|index| {
                let text: String = lines[index]
                    .spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect();
                let width = crate::ui::text::width_of(&text);
                match self.columns(index, width) {
                    Some((from, to)) => slice_columns(&text, from, to).trim_end().to_string(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// The selection being shown and the copy waiting to be written to the terminal.
/// 正在显示的选择，以及等待写入终端的复制内容。
#[derive(Default)]
pub struct Clipboard {
    pub selection: Option<TextSelection>,
    outbox: Option<String>,
}

impl std::fmt::Debug for Clipboard {
    /// Clipboard payloads can contain one-use viewer links; never format their bytes.
    /// 剪贴板内容可能包含单次查看链接；绝不格式化其字节。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Clipboard")
            .field("selection", &self.selection)
            .field("pending", &self.outbox.is_some())
            .finish()
    }
}

impl Clipboard {
    /// Queues `text` for the terminal clipboard; a newer copy replaces an unsent one.
    /// 将 `text` 排入终端剪贴板队列；较新的复制替换尚未发送的内容。
    pub fn copy(&mut self, text: String) {
        self.outbox = Some(text);
    }

    /// Takes the queued copy as the OSC 52 sequence that sets the system clipboard.
    /// 取出排队的复制内容，形式为设置系统剪贴板的 OSC 52 序列。
    pub fn take_sequence(&mut self) -> Option<String> {
        self.outbox
            .take()
            .map(|text| format!("\x1b]52;c;{}\x07", STANDARD.encode(text)))
    }
}

/// Characters of `text` that overlap display columns `[from, to)`.
/// `text` 中与显示列 `[from, to)` 重叠的字符。
fn slice_columns(text: &str, from: usize, to: usize) -> String {
    let mut column = 0;
    let mut out = String::new();
    for ch in text.chars() {
        let width = ch.width().unwrap_or(0);
        if column < to && column + width.max(1) > from {
            out.push(ch);
        }
        column += width;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selection(anchor: (usize, usize), head: (usize, usize)) -> TextSelection {
        let point = |(line, column)| Point { line, column };
        TextSelection {
            conversation: "c".into(),
            width: 40,
            anchor: point(anchor),
            head: point(head),
            dragging: false,
        }
    }

    #[test]
    fn selected_text_follows_the_drag_in_either_direction() {
        let lines = [
            Line::from("first line"),
            Line::from("第二行 text   "),
            Line::from("third"),
        ];
        let forward = selection((0, 6), (2, 2));
        assert_eq!(forward.text(&lines), "line\n第二行 text\nthi");
        let backward = selection((2, 2), (0, 6));
        assert_eq!(backward.text(&lines), forward.text(&lines));
        // A wide character is taken whole when either of its cells is selected.
        // 宽字符的任一单元格被选中时整体取出。
        assert_eq!(selection((1, 1), (1, 2)).text(&lines), "第二");
        // A click without movement selects nothing.
        // 未移动的点击不选中任何内容。
        assert_eq!(selection((1, 3), (1, 3)).text(&lines), "");
    }

    #[test]
    fn queued_copies_leave_as_one_osc52_sequence() {
        let mut clipboard = Clipboard::default();
        assert!(clipboard.take_sequence().is_none());
        clipboard.copy("old".into());
        clipboard.copy("新".into());
        let sequence = clipboard.take_sequence().expect("a queued copy");
        let payload = sequence
            .strip_prefix("\x1b]52;c;")
            .and_then(|rest| rest.strip_suffix('\x07'))
            .expect("OSC 52 framing");
        assert_eq!(STANDARD.decode(payload).unwrap(), "新".as_bytes());
        assert!(clipboard.take_sequence().is_none());
    }
}
