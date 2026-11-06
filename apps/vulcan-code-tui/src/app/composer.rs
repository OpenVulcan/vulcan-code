// Composer editing: multi-line input, long pastes collapsed into tokens, and paste bursts.
// 输入框编辑：多行输入、长粘贴折叠为标记，以及粘贴突发识别。
//
// A long paste is not inserted verbatim: the composer holds a short token such as
// `[Pasted #1 · 42 lines]` and the full text lives in `App::pastes` until the prompt is sent, when
// every token still present is expanded. Tokens are edited as one unit (Backspace/Delete remove the
// whole token, the cursor never rests inside one).
// 长粘贴不会原样插入：输入框只保存如 `[Pasted #1 · 42 lines]` 的短标记，完整文本保存在
// `App::pastes` 中，发送时展开所有仍存在的标记。标记作为整体编辑（退格/删除移除整个标记，光标不会
// 停留在标记内部）。
//
// Terminals without bracketed paste (the Windows console) deliver a paste as individual key
// presses, so a newline inside it would submit the prompt. `coalesce` turns such bursts back into
// one `Event::Paste` before input handling sees them.
// 不支持括号粘贴的终端（Windows 控制台）会把粘贴拆成逐个按键，其中的换行会直接提交提示。
// `coalesce` 在输入处理之前把这类突发按键还原为一个 `Event::Paste`。

use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui_textarea::{CursorMove, DataCursor};

use super::App;
use crate::tr;

/// Pastes with at least this many lines are collapsed into a token.
/// 行数达到此值的粘贴会折叠为标记。
pub const PASTE_COLLAPSE_LINES: usize = 4;
/// Pastes with at least this many characters are collapsed into a token.
/// 字符数达到此值的粘贴会折叠为标记。
pub const PASTE_COLLAPSE_CHARS: usize = 800;
/// A run of plain keys read in one batch that ends with Enter counts as a paste from this length
/// on; nobody types this many keys between two frames.
/// 同一批读取、以回车结尾的普通按键达到此长度即视为粘贴；人工输入无法在两帧之间敲出这么多键。
pub const BURST_MIN_CHARS: usize = 16;

/// One collapsed paste: the token shown in the composer and the text it stands for.
/// 一次折叠的粘贴：输入框中显示的标记及其代表的文本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PastedText {
    /// Number shown in the token, unique within the current draft.
    /// 标记中显示的编号，在当前草稿内唯一。
    pub serial: usize,
    pub token: String,
    pub text: String,
}

impl PastedText {
    pub fn lines(&self) -> usize {
        self.text.lines().count().max(1)
    }

    pub fn chars(&self) -> usize {
        self.text.chars().count()
    }
}

/// Unifies line endings: `\r\n` and lone `\r` become `\n`.
/// 统一换行符：`\r\n` 与单独的 `\r` 都转换为 `\n`。
pub fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// True when a paste is long enough to be shown as a token.
/// 粘贴足够长、应显示为标记时为真。
pub fn should_collapse(text: &str) -> bool {
    text.lines().count() >= PASTE_COLLAPSE_LINES || text.chars().count() >= PASTE_COLLAPSE_CHARS
}

/// A token found in one composer line: paste index and its char range `[start, end)`.
/// 在输入框某行中找到的标记：粘贴序号及其字符范围 `[start, end)`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenSpan {
    pub paste: usize,
    pub start: usize,
    pub end: usize,
}

/// Every token occurrence in `line`, in order.
/// `line` 中所有标记的出现位置，按顺序排列。
pub fn tokens_in(line: &str, pastes: &[PastedText]) -> Vec<TokenSpan> {
    let mut spans = Vec::new();
    for (paste, item) in pastes.iter().enumerate() {
        let mut from = 0;
        while let Some(offset) = line[from..].find(&item.token) {
            let byte = from + offset;
            let start = line[..byte].chars().count();
            spans.push(TokenSpan {
                paste,
                start,
                end: start + item.token.chars().count(),
            });
            from = byte + item.token.len();
        }
    }
    spans.sort_by_key(|s| s.start);
    spans
}

impl App {
    /// Inserts pasted text at the cursor, collapsing long pastes into a token.
    /// 在光标处插入粘贴文本，长粘贴折叠为标记。
    pub fn paste_into_composer(&mut self, text: &str) {
        let text = normalize(text);
        if !should_collapse(&text) {
            self.composer.insert_str(&text);
            return;
        }
        self.paste_serial += 1;
        let index = self.paste_serial;
        let lines = text.lines().count();
        let token = if lines > 1 {
            tr!("tui.composer.pasted", index = index, lines = lines)
        } else {
            tr!(
                "tui.composer.pastedChars",
                index = index,
                chars = text.chars().count()
            )
        };
        self.composer.insert_str(&token);
        self.pastes.push(PastedText {
            serial: index,
            token,
            text,
        });
    }

    /// The composer text with every remaining token expanded to its paste.
    /// 输入框文本，其中所有剩余标记都展开为对应的粘贴内容。
    pub fn composer_prompt(&self) -> String {
        let mut text = self.composer_text();
        for paste in &self.pastes {
            if let Some(at) = text.find(&paste.token) {
                text.replace_range(at..at + paste.token.len(), &paste.text);
            }
        }
        text
    }

    /// The token under or next to the cursor on the cursor's line.
    /// 光标所在行中，位于光标处或紧邻光标的标记。
    fn token_near_cursor(&self, pick: impl Fn(&TokenSpan, usize) -> bool) -> Option<TokenSpan> {
        let DataCursor(row, col) = self.composer.cursor();
        let line = self.composer.lines().get(row)?;
        tokens_in(line, &self.pastes)
            .into_iter()
            .find(|span| pick(span, col))
    }

    /// Backspace right after a token removes the whole token; returns true when it did.
    /// 在标记之后退格会移除整个标记；移除时返回真。
    pub fn delete_token_before_cursor(&mut self) -> bool {
        let Some(span) = self.token_near_cursor(|s, col| s.end == col) else {
            return false;
        };
        for _ in span.start..span.end {
            self.composer.delete_char();
        }
        self.forget_unused_pastes();
        true
    }

    /// Delete right before a token removes the whole token; returns true when it did.
    /// 在标记之前按删除键会移除整个标记；移除时返回真。
    pub fn delete_token_after_cursor(&mut self) -> bool {
        let Some(span) = self.token_near_cursor(|s, col| s.start == col) else {
            return false;
        };
        self.composer.delete_str(span.end - span.start);
        self.forget_unused_pastes();
        true
    }

    /// Moves a cursor that landed inside a token to its edge: the start when moving left, the
    /// end otherwise.
    /// 将落在标记内部的光标移到边缘：向左移动时到开头，否则到结尾。
    pub fn snap_cursor_out_of_token(&mut self, moving_left: bool) {
        let Some(span) = self.token_near_cursor(|s, col| s.start < col && col < s.end) else {
            return;
        };
        let DataCursor(row, _) = self.composer.cursor();
        let col = if moving_left { span.start } else { span.end };
        self.composer
            .move_cursor(CursorMove::Jump(row as u16, col as u16));
    }

    /// The paste whose token touches the cursor, else the most recent one.
    /// 标记与光标相邻的粘贴，否则为最近一次粘贴。
    pub fn paste_near_cursor(&self) -> Option<usize> {
        self.token_near_cursor(|s, col| s.start <= col && col <= s.end)
            .map(|span| span.paste)
            .or_else(|| self.pastes.len().checked_sub(1))
    }

    /// Replaces the token of paste `index` with its full text.
    /// 用完整文本替换第 `index` 个粘贴的标记。
    pub fn expand_paste(&mut self, index: usize) {
        let Some(paste) = self.pastes.get(index).cloned() else {
            return;
        };
        self.replace_token(&paste.token, &paste.text);
        self.pastes.remove(index);
    }

    /// Removes paste `index` and its token from the composer.
    /// 从输入框中移除第 `index` 个粘贴及其标记。
    pub fn remove_paste(&mut self, index: usize) {
        let Some(paste) = self.pastes.get(index).cloned() else {
            return;
        };
        self.replace_token(&paste.token, "");
        self.pastes.remove(index);
    }

    /// Rebuilds the composer with the first occurrence of `token` replaced, keeping the cursor
    /// where the replacement ends.
    /// 重建输入框并替换 `token` 的首次出现，光标置于替换内容末尾。
    fn replace_token(&mut self, token: &str, with: &str) {
        let text = self.composer_text();
        let Some(at) = text.find(token) else {
            return;
        };
        let mut composer = super::new_composer();
        composer.insert_str(&text[..at]);
        composer.insert_str(with);
        let DataCursor(row, col) = composer.cursor();
        composer.insert_str(&text[at + token.len()..]);
        composer.move_cursor(CursorMove::Jump(row as u16, col as u16));
        self.composer = composer;
    }

    /// Puts a refused submission back: its text goes in front of whatever was typed since, and
    /// its files rejoin the attachment list. Only the conversation it was sent from receives it,
    /// so text never moves to another conversation.
    /// 交还被拒绝的提交：其文本放在之后输入内容的前面，其文件重新加入附件列表。只有发送它的会话
    /// 才会接收，文本绝不会转到其他会话。
    pub fn restore_rejected_submission(
        &mut self,
        conversation_id: &str,
        prompt: &str,
        attachments: Vec<String>,
    ) {
        if self.selected.as_deref() != Some(conversation_id) {
            return;
        }
        let typed = self.composer_text();
        let mut composer = super::new_composer();
        composer.insert_str(prompt);
        if !typed.trim().is_empty() {
            composer.insert_newline();
            composer.insert_str(&typed);
        }
        self.composer = composer;
        let mut restored = attachments;
        restored.retain(|path| !self.attachments.contains(path));
        restored.append(&mut self.attachments);
        self.attachments = restored;
    }

    /// Drops pastes whose token no longer appears in the composer.
    /// 丢弃标记已不在输入框中的粘贴。
    pub fn forget_unused_pastes(&mut self) {
        let text = self.composer_text();
        self.pastes.retain(|p| text.contains(&p.token));
    }
}

/// A key press that inserts text as typed: a character without Ctrl/Alt, a bare Enter or Tab.
/// 按原样插入文本的按键：未按 Ctrl/Alt 的字符、单独的回车或 Tab。
fn plain_char(key: &KeyEvent) -> Option<char> {
    if key.kind == KeyEventKind::Release {
        return None;
    }
    let bare = !key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
    match key.code {
        KeyCode::Char(ch) if bare => Some(ch),
        KeyCode::Enter if key.modifiers.is_empty() => Some('\n'),
        KeyCode::Tab if key.modifiers.is_empty() => Some('\t'),
        _ => None,
    }
}

fn is_release(event: &Event) -> bool {
    matches!(event, Event::Key(key) if key.kind == KeyEventKind::Release)
}

/// True while the tail of a read batch looks like a paste still arriving, so the reader should
/// wait a moment for the rest instead of handling half of it.
/// 读取批次的末尾看起来像仍在到达的粘贴时为真，此时读取方应稍等片刻取完剩余部分，而不是只处理一半。
pub fn burst_pending(batch: &[Event]) -> bool {
    batch
        .iter()
        .rev()
        .filter(|e| !is_release(e))
        .take(2)
        .filter(|e| matches!(e, Event::Key(key) if plain_char(key).is_some()))
        .count()
        == 2
}

/// Rewrites runs of plain key presses that can only come from a paste into `Event::Paste`.
/// A run qualifies when a newline or tab is followed by more text, or when it ends with a newline
/// after at least [`BURST_MIN_CHARS`] characters. Everything else passes through unchanged, so
/// typing, IME commits and a final Enter keep their meaning.
/// 将只可能来自粘贴的普通按键序列改写为 `Event::Paste`。当换行或 Tab 之后还有文本，或以换行结尾且
/// 至少有 [`BURST_MIN_CHARS`] 个字符时，该序列被视为粘贴。其余事件原样通过，因此普通输入、输入法
/// 上屏与最后的回车都保持原意。
pub fn coalesce(events: Vec<Event>) -> Vec<Event> {
    let mut out = Vec::with_capacity(events.len());
    let mut run: Vec<Event> = Vec::new();
    let flush = |run: &mut Vec<Event>, out: &mut Vec<Event>| {
        let text: String = run
            .iter()
            .filter_map(|e| match e {
                Event::Key(key) => plain_char(key),
                _ => None,
            })
            .collect();
        let chars: Vec<char> = text.chars().collect();
        let inner_break = chars
            .iter()
            .enumerate()
            .any(|(i, ch)| matches!(ch, '\n' | '\t') && i + 1 < chars.len());
        let long_line = chars.last() == Some(&'\n') && chars.len() >= BURST_MIN_CHARS;
        if inner_break || long_line {
            out.push(Event::Paste(text));
        } else {
            out.append(run);
        }
        run.clear();
    };
    for event in events {
        let plain = match &event {
            Event::Key(key) => plain_char(key).is_some() || key.kind == KeyEventKind::Release,
            _ => false,
        };
        if plain {
            run.push(event);
        } else {
            flush(&mut run, &mut out);
            out.push(event);
        }
    }
    flush(&mut run, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn typed(text: &str) -> Vec<Event> {
        text.chars()
            .map(|ch| match ch {
                '\n' => key(KeyCode::Enter),
                '\t' => key(KeyCode::Tab),
                ch => key(KeyCode::Char(ch)),
            })
            .collect()
    }

    #[test]
    fn multi_line_burst_becomes_one_paste() {
        let out = coalesce(typed("fn main() {\n    run();\n}"));
        assert_eq!(out, vec![Event::Paste("fn main() {\n    run();\n}".into())]);
    }

    #[test]
    fn short_typing_and_enter_are_kept() {
        let events = typed("ok\n");
        assert_eq!(coalesce(events.clone()), events);
    }

    #[test]
    fn long_single_line_with_trailing_newline_is_a_paste() {
        let text = "https://example.com/some/long/path\n";
        assert_eq!(coalesce(typed(text)), vec![Event::Paste(text.into())]);
    }

    #[test]
    fn modified_keys_split_runs() {
        let mut events = typed("ab");
        events.push(Event::Key(KeyEvent::new(
            KeyCode::Char('p'),
            KeyModifiers::CONTROL,
        )));
        events.extend(typed("c\nd"));
        let out = coalesce(events.clone());
        assert_eq!(out[..3], events[..3]);
        assert_eq!(out[3], Event::Paste("c\nd".into()));
    }

    /// A refused prompt returns to its own conversation's composer ahead of newer typing, and
    /// never to another conversation.
    /// 被拒绝的提示回到其所属会话的输入框，位于新输入之前，绝不进入其他会话。
    #[test]
    fn refused_submission_returns_to_its_own_conversation() {
        let (commands, _receiver) = std::sync::mpsc::channel();
        let (_, events) = std::sync::mpsc::channel();
        let mut app = super::super::App::new(
            crate::backend::BackendHandle {
                commands,
                events,
                label: "test".into(),
                is_mock: true,
            },
            crate::ui::theme::Theme::dark(),
            true,
        );
        app.selected = Some("a".into());
        app.composer.insert_str("newer");
        app.attachments = vec!["kept.png".into()];
        app.restore_rejected_submission("b", "lost?", vec!["other.png".into()]);
        assert_eq!(app.composer_text(), "newer", "other conversation untouched");
        app.restore_rejected_submission(
            "a",
            "first line",
            vec!["sent.png".into(), "kept.png".into()],
        );
        assert_eq!(app.composer_text(), "first line\nnewer");
        assert_eq!(app.attachments, ["sent.png", "kept.png"]);
    }

    #[test]
    fn line_endings_are_normalized() {
        assert_eq!(normalize("a\r\nb\rc"), "a\nb\nc");
    }

    #[test]
    fn tokens_are_found_by_char_position() {
        let pastes = vec![PastedText {
            serial: 1,
            token: "[貼上 #1]".into(),
            text: "x".into(),
        }];
        let spans = tokens_in("前[貼上 #1]後", &pastes);
        assert_eq!(
            spans,
            vec![TokenSpan {
                paste: 0,
                start: 1,
                end: 8
            }]
        );
    }
}
