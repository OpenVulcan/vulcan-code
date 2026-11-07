// Conversation history index: groups the event log into entries the history browser lists.
// 会话历史索引：把事件日志分组为历史浏览器列出的条目。
//
// Consecutive tool calls collapse into one entry so a long agent turn stays scannable; the
// inspector then shows every event from the entry's first seq up to the next entry.
// 连续的工具调用合并为一个条目，使长代理回合仍便于浏览；检视器显示从条目首个 seq 到下一个条目
// 之前的全部事件。

use super::overlay::HistoryFilter;
use crate::model::*;
use crate::tr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Prompt,
    Thinking,
    Tools,
    Reply,
    Note,
    Stats,
}

impl EntryKind {
    pub fn matches(self, filter: HistoryFilter) -> bool {
        match filter {
            HistoryFilter::All => true,
            HistoryFilter::Prompts => self == EntryKind::Prompt,
            HistoryFilter::Thinking => self == EntryKind::Thinking,
            HistoryFilter::Tools => self == EntryKind::Tools,
            HistoryFilter::Replies => self == EntryKind::Reply,
            HistoryFilter::Notes => matches!(self, EntryKind::Note | EntryKind::Stats),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub seq: u64,
    pub kind: EntryKind,
    pub title: String,
    /// Searchable text: the full prompt, reasoning, reply or tool names.
    /// 可搜索文本：完整提示、思考、回复或工具名称。
    pub text: String,
    pub count: usize,
    pub at_ms: u64,
}

/// First non-empty line without leading Markdown heading, quote and bullet markers.
/// 第一个非空行，去掉行首的 Markdown 标题、引用与列表标记。
fn first_line(text: &str) -> String {
    text.lines()
        .map(|line| {
            let line = line.trim().trim_start_matches(['#', '>']).trim_start();
            line.strip_prefix("- ")
                .or_else(|| line.strip_prefix("* "))
                .unwrap_or(line)
                .trim()
        })
        .find(|l| !l.is_empty())
        .unwrap_or_default()
        .to_string()
}

pub fn entries(events: &[ConversationEvent]) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    for event in events {
        let (kind, title, text) = match &event.body {
            EventBody::UserInput { text, .. } => {
                (EntryKind::Prompt, first_line(text), text.clone())
            }
            EventBody::AssistantReasoning { text, .. } => {
                (EntryKind::Thinking, first_line(text), text.clone())
            }
            EventBody::AssistantText { text, .. } => {
                (EntryKind::Reply, first_line(text), text.clone())
            }
            EventBody::ToolCall { tool_name, .. } => {
                if let Some(last) = out.last_mut()
                    && last.kind == EntryKind::Tools
                {
                    last.count += 1;
                    last.text.push(' ');
                    last.text.push_str(tool_name);
                    last.title = tr!("tui.history.toolGroup", count = last.count);
                    continue;
                }
                (EntryKind::Tools, tool_name.clone(), tool_name.clone())
            }
            EventBody::DisplayNote { message, .. } => {
                (EntryKind::Note, first_line(message), message.clone())
            }
            EventBody::TurnStats {
                total_tokens,
                tool_calls,
                ..
            } => (
                EntryKind::Stats,
                tr!(
                    "tui.history.stats",
                    tokens = crate::ui::text::compact_count(*total_tokens),
                    tools = tool_calls
                ),
                String::new(),
            ),
        };
        out.push(Entry {
            seq: event.seq,
            kind,
            title,
            text,
            count: 1,
            at_ms: event.created_at_ms,
        });
    }
    out
}

/// Entries visible under `filter` and a case-insensitive `query`.
/// 在 `filter` 与不区分大小写的 `query` 下可见的条目。
pub fn visible(events: &[ConversationEvent], filter: HistoryFilter, query: &str) -> Vec<Entry> {
    let needle = query.trim().to_lowercase();
    entries(events)
        .into_iter()
        .filter(|e| e.kind.matches(filter))
        .filter(|e| {
            needle.is_empty()
                || e.text.to_lowercase().contains(&needle)
                || e.title.to_lowercase().contains(&needle)
        })
        .collect()
}

/// Events belonging to the entry starting at `seq`.
/// 属于以 `seq` 开头的条目的事件。
pub fn events_of(events: &[ConversationEvent], seq: u64) -> Vec<&ConversationEvent> {
    let all = entries(events);
    let end = all
        .iter()
        .position(|e| e.seq == seq)
        .and_then(|i| all.get(i + 1))
        .map_or(u64::MAX, |e| e.seq);
    events
        .iter()
        .filter(|e| e.seq >= seq && e.seq < end)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(seq: u64, body: EventBody) -> ConversationEvent {
        ConversationEvent {
            seq,
            created_at_ms: seq,
            updated_at_ms: seq,
            body,
        }
    }

    fn tool(seq: u64, name: &str) -> ConversationEvent {
        event(
            seq,
            EventBody::ToolCall {
                tool_name: name.into(),
                input: serde_json::Value::Null,
                output: None,
                changeset: Vec::new(),
                status: ToolCallStatus::Completed,
                started_at_ms: None,
                finished_at_ms: None,
                user_wait_ms: 0,
                host_purpose: None,
            },
        )
    }

    #[test]
    fn groups_consecutive_tools_and_slices_events() {
        let events = vec![
            event(
                1,
                EventBody::UserInput {
                    text: "\n  fix the bug\nplease".into(),
                    input_source: UserInputSource::Root,
                    attachments: Vec::new(),
                },
            ),
            tool(2, "read_file"),
            tool(3, "grep"),
            event(
                4,
                EventBody::AssistantText {
                    text: "done".into(),
                    status: AssistantSemanticStatus::Completed,
                    code_snapshots: None,
                },
            ),
        ];
        let list = entries(&events);
        let kinds: Vec<EntryKind> = list.iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            [EntryKind::Prompt, EntryKind::Tools, EntryKind::Reply]
        );
        assert_eq!(list[0].title, "fix the bug");
        assert_eq!(list[1].count, 2);
        let seqs: Vec<u64> = events_of(&events, 2).iter().map(|e| e.seq).collect();
        assert_eq!(seqs, [2, 3]);
        assert_eq!(visible(&events, HistoryFilter::All, "GREP").len(), 1);
        assert_eq!(visible(&events, HistoryFilter::Prompts, "").len(), 1);
    }
}
