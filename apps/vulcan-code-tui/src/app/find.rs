// Transcript find: "/" in the transcript searches the laid-out lines of the open conversation, the
// same text the WebUI find bar searches. Keys only record the query and the wanted jump; the next
// frame matches against its own layout, so folding, resizing and streaming never leave stale hits.
// 会话记录查找：在会话记录中按 "/" 搜索当前会话排版后的各行，即 WebUI 查找栏搜索的同一文本。按键只记录
// 查询与所需跳转；下一帧以自身排版进行匹配，因此折叠、调整尺寸与流式输出都不会留下过期结果。

use ratatui::text::Line;
use unicode_width::UnicodeWidthChar;

/// One match: its layout line and the display columns it covers, `from..to`.
/// 一个匹配：其排版行及覆盖的显示列 `from..to`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FindMatch {
    pub line: usize,
    pub from: usize,
    pub to: usize,
}

/// Where the next frame moves the current match.
/// 下一帧如何移动当前匹配。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindJump {
    /// The query changed: go to the first match at or below the top of the view.
    /// 查询已变化：跳到视图顶部及以下的首个匹配。
    Restart,
    /// Step to the next (`1`) or previous (`-1`) match, wrapping at either end.
    /// 移到下一个（`1`）或上一个（`-1`）匹配，在两端循环。
    Step(isize),
}

/// Open transcript find.
/// 打开的会话记录查找。
#[derive(Debug, Default)]
pub struct TranscriptFind {
    pub query: String,
    /// Whether keys type into the query; otherwise n/N step and Esc closes.
    /// 按键是否输入查询；否则 n/N 移动、Esc 关闭。
    pub editing: bool,
    /// Index of the current match in `matches`.
    /// 当前匹配在 `matches` 中的序号。
    pub current: usize,
    /// Matches in the last rendered layout.
    /// 最近一次渲染排版中的匹配。
    pub matches: Vec<FindMatch>,
    pub jump: Option<FindJump>,
}

impl TranscriptFind {
    /// Starts typing a query, keeping the previous one so it can be edited.
    /// 开始输入查询，保留上一次查询以便编辑。
    pub fn edit(find: &mut Option<TranscriptFind>) {
        find.get_or_insert_with(TranscriptFind::default).editing = true;
    }

    /// Matches the query against `lines`, applies the pending jump and returns the line to bring
    /// into view, if the jump calls for it. `top` and `height` describe the current view.
    /// 用查询匹配 `lines`，执行待处理的跳转，并在跳转需要时返回应滚入视图的行。`top` 与 `height`
    /// 描述当前视图。
    pub fn update(&mut self, lines: &[Line<'_>], top: usize, height: usize) -> Option<usize> {
        self.matches = find_matches(lines, &self.query);
        let jump = self.jump.take();
        if self.matches.is_empty() {
            self.current = 0;
            return None;
        }
        let count = self.matches.len();
        match jump {
            None => {
                self.current = self.current.min(count - 1);
                None
            }
            Some(FindJump::Restart) => {
                self.current = self.matches.iter().position(|m| m.line >= top).unwrap_or(0);
                let line = self.matches[self.current].line;
                (!(top..top + height).contains(&line)).then_some(line)
            }
            Some(FindJump::Step(direction)) => {
                let current = self.current.min(count - 1) as isize;
                self.current = (current + direction).rem_euclid(count as isize) as usize;
                Some(self.matches[self.current].line)
            }
        }
    }
}

/// Finds `query` in each line, ignoring case; matches do not overlap and never span lines.
/// 在每一行中查找 `query`（不区分大小写）；匹配互不重叠，也不跨行。
pub fn find_matches(lines: &[Line<'_>], query: &str) -> Vec<FindMatch> {
    let needle: Vec<char> = query.chars().map(fold).collect();
    let mut matches = Vec::new();
    if needle.is_empty() {
        return matches;
    }
    for (index, line) in lines.iter().enumerate() {
        // chars holds the folded characters with their starting column and width.
        // chars 保存折叠后的字符及其起始列与宽度。
        let mut chars: Vec<(char, usize, usize)> = Vec::new();
        let mut column = 0;
        for ch in line.spans.iter().flat_map(|span| span.content.chars()) {
            let width = ch.width().unwrap_or(0);
            chars.push((fold(ch), column, width));
            column += width;
        }
        let mut start = 0;
        while start + needle.len() <= chars.len() {
            let window = &chars[start..start + needle.len()];
            if window
                .iter()
                .zip(&needle)
                .all(|((ch, _, _), wanted)| ch == wanted)
            {
                let (_, from, _) = window[0];
                let (_, last, width) = window[needle.len() - 1];
                matches.push(FindMatch {
                    line: index,
                    from,
                    to: last + width,
                });
                start += needle.len();
            } else {
                start += 1;
            }
        }
    }
    matches
}

fn fold(ch: char) -> char {
    ch.to_lowercase().next().unwrap_or(ch)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(texts: &[&str]) -> Vec<Line<'static>> {
        texts.iter().map(|t| Line::from(t.to_string())).collect()
    }

    #[test]
    fn matches_ignore_case_and_measure_display_columns() {
        let found = find_matches(&lines(&["Alpha alpha", "错误 ALPHA"]), "alpha");
        assert_eq!(
            found,
            vec![
                FindMatch {
                    line: 0,
                    from: 0,
                    to: 5
                },
                FindMatch {
                    line: 0,
                    from: 6,
                    to: 11
                },
                FindMatch {
                    line: 1,
                    from: 5,
                    to: 10
                },
            ]
        );
        assert!(find_matches(&lines(&["abc"]), "").is_empty());
    }

    #[test]
    fn jumps_start_in_view_step_and_wrap() {
        let layout = lines(&["x", "", "x", "", "x"]);
        let mut find = TranscriptFind {
            query: "x".into(),
            jump: Some(FindJump::Restart),
            ..Default::default()
        };
        assert_eq!(
            find.update(&layout, 1, 3),
            None,
            "a visible match needs no scroll"
        );
        assert_eq!(find.current, 1);
        find.jump = Some(FindJump::Step(1));
        assert_eq!(find.update(&layout, 1, 3), Some(4));
        find.jump = Some(FindJump::Step(1));
        assert_eq!(find.update(&layout, 1, 3), Some(0), "stepping wraps");
        find.jump = Some(FindJump::Step(-1));
        assert_eq!(find.update(&layout, 1, 3), Some(4));
        assert_eq!(find.current, 2);
    }
}
