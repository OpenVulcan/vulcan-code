// Conversation transcript: events folded into blocks, rendered bottom-anchored with scrolling.
// 会话记录：事件折叠为块，以底部为锚点渲染并支持滚动。
//
// Block rules mirror the WebUI message stream: consecutive tool calls form one group (expanded
// while any call is open, collapsed after), reasoning collapses into a "Thinking process" row, and
// each turn ends with a stats line. `SessionState::toggled` flips a block's default fold state.
// 块规则对应 WebUI 消息流：连续的工具调用组成一组（有调用进行中时展开，结束后折叠），思考过程折叠
// 为“思考过程”行，每轮以统计行结束。`SessionState::toggled` 翻转块的默认折叠状态。

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;
use serde_json::Value;

use super::answer_components::CodeSnapshots;
use super::brand;
use super::markdown;
use super::text::{compact_count, compact_duration, shimmer, spinner, truncate, width_of, wrap};
use super::theme::{Theme, mix};
use crate::app::hit::{ScrollTarget, Target};
use crate::app::{App, Focus, SessionState};
use crate::model::*;
use crate::tr;

enum Block<'a> {
    User(&'a ConversationEvent),
    Reasoning(&'a ConversationEvent),
    Text(&'a ConversationEvent),
    Tools(Vec<&'a ConversationEvent>),
    Note(&'a ConversationEvent),
    Stats(&'a ConversationEvent),
}

impl Block<'_> {
    fn seq(&self) -> u64 {
        match self {
            Block::Tools(events) => events[0].seq,
            Block::User(e)
            | Block::Reasoning(e)
            | Block::Text(e)
            | Block::Note(e)
            | Block::Stats(e) => e.seq,
        }
    }
}

fn blocks(events: &[ConversationEvent]) -> Vec<Block<'_>> {
    let mut out: Vec<Block> = Vec::new();
    for event in events {
        match &event.body {
            EventBody::ToolCall { .. } => match out.last_mut() {
                Some(Block::Tools(group)) => group.push(event),
                _ => out.push(Block::Tools(vec![event])),
            },
            EventBody::UserInput { .. } => out.push(Block::User(event)),
            EventBody::AssistantReasoning { .. } => out.push(Block::Reasoning(event)),
            EventBody::AssistantText { .. } => out.push(Block::Text(event)),
            EventBody::DisplayNote { .. } => out.push(Block::Note(event)),
            EventBody::TurnStats { .. } => out.push(Block::Stats(event)),
        }
    }
    out
}

/// Laid-out transcript: lines plus selectable items `(seq, first line, line count)`.
/// 排版后的会话记录：行与可选项 `(seq, 首行, 行数)`。
struct Layout {
    lines: Vec<Line<'static>>,
    items: Vec<(u64, usize, usize)>,
}

struct Ctx<'a> {
    theme: &'a Theme,
    width: usize,
    phase: f32,
    session: &'a SessionState,
}

pub fn render(app: &mut App, area: Rect, buf: &mut Buffer) {
    let theme = app.theme;
    buf.set_style(area, theme.base());
    app.hits.transcript = area;
    app.hits.transcript_top = 0;
    app.hits.transcript_content = Rect::default();
    app.hits.transcript_text.clear();
    app.hits.add(area, Target::Transcript);
    app.hits.add_scroll(area, ScrollTarget::Transcript);
    let Some(session) = app.session() else {
        app.hits.transcript_items.clear();
        app.hits.transcript_lines = 0;
        welcome(&theme, area, buf, false);
        return;
    };
    if !session.hydrated {
        let line = Line::from(
            [
                vec![Span::styled(
                    format!("{} ", spinner(app.phase())),
                    theme.primary_text(),
                )],
                shimmer(
                    &tr!("conversation.loading.title"),
                    theme.muted_fg,
                    theme.fg,
                    app.phase(),
                ),
            ]
            .concat(),
        );
        line.render(
            Rect {
                y: area.y + 1,
                height: 1,
                x: area.x + 2,
                width: area.width.saturating_sub(2),
            },
            buf,
        );
        return;
    }
    if session.events.is_empty() {
        app.hits.transcript_items.clear();
        app.hits.transcript_lines = 0;
        welcome(&theme, area, buf, true);
        return;
    }
    let gutter = 2u16;
    let content_width = area.width.saturating_sub(gutter + 1).max(8) as usize;
    let ctx = Ctx {
        theme: &theme,
        width: content_width,
        phase: app.phase(),
        session,
    };
    let content = Rect {
        x: area.x + gutter,
        width: content_width as u16,
        ..area
    };
    let mut layout = lay_out(&ctx);
    if app.run_phase().is_active() && session.live_reasoning().is_none() && !streaming_text(session)
    {
        layout.lines.push(Line::default());
        let label = if app.run_phase() == RunPhase::Queued {
            tr!("pendingPrompt.queue")
        } else {
            tr!("conversation.process.working")
        };
        layout.lines.push(Line::from(
            [
                vec![Span::styled(
                    format!("{} ", spinner(app.phase())),
                    theme.primary_text(),
                )],
                shimmer(&label, theme.muted_fg, theme.fg, app.phase()),
            ]
            .concat(),
        ));
    }
    layout.lines.push(Line::default());

    let total = layout.lines.len();
    let height = area.height as usize;
    let selected = session.selected;
    let mut scroll = session.scroll_from_bottom.min(total.saturating_sub(height));
    if let Some(find) = app.transcript_find.as_mut()
        && let Some(line) =
            find.update(&layout.lines, total.saturating_sub(height + scroll), height)
    {
        let top = line.saturating_sub(height / 2);
        scroll = total.saturating_sub(height + top);
    }
    let top = total.saturating_sub(height + scroll);
    let found = app.transcript_find.as_ref();
    let selected_range = selected
        .and_then(|seq| layout.items.iter().find(|(s, _, _)| *s == seq))
        .map(|(_, start, len)| *start..*start + *len);
    let focused = app.focus == Focus::Transcript;
    let conversation = app.selected.as_ref();
    let dragged = app.clipboard.selection.as_ref().filter(|selection| {
        Some(&selection.conversation) == conversation && selection.width == content_width
    });
    let highlight = Style::new().fg(theme.primary_fg).bg(theme.primary);

    for (row, line) in layout.lines.iter().skip(top).take(height).enumerate() {
        let index = top + row;
        let y = area.y + row as u16;
        if selected_range.as_ref().is_some_and(|r| r.contains(&index)) {
            let color = if focused { theme.primary } else { theme.border };
            buf.set_string(area.x, y, "▎", Style::new().fg(color));
        }
        line.render(
            Rect {
                y,
                height: 1,
                ..content
            },
            buf,
        );
        if let Some(find) = found {
            for (number, hit) in find
                .matches
                .iter()
                .enumerate()
                .filter(|(_, m)| m.line == index)
            {
                let style = if number == find.current {
                    highlight
                } else {
                    Style::new().fg(theme.fg).bg(theme.accent)
                };
                let to = hit.to.min(content_width);
                let cells = Rect {
                    x: content.x + hit.from as u16,
                    y,
                    width: to.saturating_sub(hit.from) as u16,
                    height: 1,
                };
                buf.set_style(cells, style);
            }
        }
        let columns = dragged.and_then(|selection| selection.columns(index, line.width()));
        if let Some((from, to)) = columns {
            let to = to.min(content_width);
            let cells = Rect {
                x: content.x + from as u16,
                y,
                width: to.saturating_sub(from) as u16,
                height: 1,
            };
            buf.set_style(cells, highlight);
        }
    }
    if scroll > 0 {
        let hint = format!(" ↓ {} · End ", tr!("composer.scrollToBottom"));
        let w = width_of(&hint) as u16;
        if area.width > w + 2 && area.height > 0 {
            buf.set_string(
                area.right() - w - 1,
                area.bottom() - 1,
                hint,
                Style::new().fg(theme.primary_fg).bg(theme.primary),
            );
        }
    }
    if let Some(find) = found {
        find_bar(&theme, find, area, buf);
    }
    let items = layout.items;
    app.hits.transcript_top = top;
    app.hits.transcript_items = items;
    app.hits.transcript_lines = total;
    app.hits.transcript_content = content;
    app.hits.transcript_text = layout.lines;
    if let Some(session) = app.session_mut() {
        session.scroll_from_bottom = scroll;
    }
}

/// Draws the find line over the bottom row: the query (with a caret while typing) and the match count.
/// 在底行绘制查找行：查询（输入时带光标）与匹配计数。
fn find_bar(theme: &Theme, find: &crate::app::find::TranscriptFind, area: Rect, buf: &mut Buffer) {
    if area.height == 0 {
        return;
    }
    let row = Rect {
        y: area.bottom() - 1,
        height: 1,
        ..area
    };
    buf.set_style(row, Style::new().fg(theme.fg).bg(theme.popover));
    for x in row.left()..row.right() {
        buf[(x, row.y)].set_symbol(" ");
    }
    let status = if find.query.is_empty() {
        String::new()
    } else if find.matches.is_empty() {
        tr!("conversation.find.none")
    } else {
        tr!(
            "conversation.find.count",
            current = find.current + 1,
            total = find.matches.len()
        )
    };
    let right = format!("{status}  n/N · Esc ");
    let room = (area.width as usize).saturating_sub(width_of(&right) + 1);
    let query = if find.query.is_empty() && find.editing {
        Span::styled(
            tr!("conversation.find.label"),
            Style::new().fg(theme.muted_fg),
        )
    } else {
        Span::raw(find.query.clone())
    };
    let left = Line::from(vec![
        Span::styled(" / ", theme.primary_text()),
        query,
        Span::styled(if find.editing { "▏" } else { "" }, theme.primary_text()),
    ]);
    left.render(
        Rect {
            width: room as u16,
            ..row
        },
        buf,
    );
    if room > 0 {
        buf.set_string(
            row.right().saturating_sub(width_of(&right) as u16),
            row.y,
            right,
            Style::new().fg(theme.muted_fg).bg(theme.popover),
        );
    }
}

fn streaming_text(session: &SessionState) -> bool {
    session.events.last().is_some_and(|e| {
        matches!(
            e.body,
            EventBody::AssistantText {
                status: AssistantSemanticStatus::Streaming,
                ..
            }
        )
    })
}

fn lay_out(ctx: &Ctx) -> Layout {
    let mut layout = Layout {
        lines: Vec::new(),
        items: Vec::new(),
    };
    let all = blocks(&ctx.session.events);
    for (index, block) in all.iter().enumerate() {
        let tight = index > 0
            && matches!(
                (&all[index - 1], block),
                (Block::Tools(_), Block::Reasoning(_)) | (Block::Reasoning(_), Block::Tools(_))
            );
        if index > 0 && !tight {
            layout.lines.push(Line::default());
        }
        let start = layout.lines.len();
        let mut lines = match block {
            Block::User(event) => user(ctx, event),
            Block::Reasoning(event) => reasoning(ctx, event),
            Block::Text(event) => text(ctx, event),
            Block::Tools(group) => tools(ctx, group),
            Block::Note(event) => note(ctx, event),
            Block::Stats(event) => stats(ctx, event),
        };
        let len = lines.len();
        layout.lines.append(&mut lines);
        layout.items.push((block.seq(), start, len));
    }
    layout
}

/// Fold state of a block: its default flipped by the user, or everything open while revealing.
/// 块的折叠状态：默认状态经用户翻转；展开全部模式下一律展开。
fn expanded(ctx: &Ctx, seq: u64, default: bool) -> bool {
    ctx.session.reveal || default ^ ctx.session.toggled.contains(&seq)
}

fn fold_marker(open: bool) -> &'static str {
    if open { "▾ " } else { "▸ " }
}

/// User messages longer than this many wrapped lines fold to a preview of
/// [`USER_PREVIEW_LINES`] lines until toggled open.
/// 超过此折行数的用户消息折叠为 [`USER_PREVIEW_LINES`] 行的预览，切换后展开。
const USER_FOLD_LINES: usize = 12;
const USER_PREVIEW_LINES: usize = 8;

fn user(ctx: &Ctx, event: &ConversationEvent) -> Vec<Line<'static>> {
    let EventBody::UserInput {
        text,
        input_source,
        attachments,
    } = &event.body
    else {
        return Vec::new();
    };
    let theme = ctx.theme;
    let bg = theme.card;
    let tag = match input_source {
        UserInputSource::Root => None,
        UserInputSource::PendingInTurn => Some(tr!("pendingPrompt.insertedShort")),
        UserInputSource::QueuedScheduled => Some(tr!("pendingPrompt.queue")),
        UserInputSource::AgentMessage => Some(tr!("conversation.subagent.detail.feedback")),
    };
    let mut lines = Vec::new();
    if let Some(tag) = tag {
        lines.push(
            Line::from(Span::styled(
                format!("  {tag}"),
                Style::new().fg(theme.violet),
            ))
            .style(Style::new().bg(bg)),
        );
    }
    let first = [Span::styled(
        "› ".to_string(),
        theme.primary_text().add_modifier(Modifier::BOLD),
    )];
    let rest = [Span::raw("  ".to_string())];
    let body = [Span::styled(text.clone(), theme.strong())];
    let mut wrapped = wrap(&body, ctx.width, &first, &rest);
    let foldable = wrapped.len() > USER_FOLD_LINES;
    let open = expanded(ctx, event.seq, false);
    let hidden = wrapped.len().saturating_sub(USER_PREVIEW_LINES);
    if foldable && !open {
        wrapped.truncate(USER_PREVIEW_LINES);
    }
    for line in wrapped {
        lines.push(line.style(Style::new().bg(bg)));
    }
    if foldable {
        let label = if open {
            tr!("tui.transcript.foldMessage")
        } else {
            tr!("tui.transcript.moreLines", count = hidden)
        };
        lines.push(
            Line::from(Span::styled(
                format!("  {}{label}", fold_marker(open)),
                theme.muted_text(),
            ))
            .style(Style::new().bg(bg)),
        );
    }
    if !attachments.is_empty() {
        let chips: Vec<Span<'static>> = attachments
            .iter()
            .map(|path| {
                let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
                Span::styled(format!("⊕ {name}  "), Style::new().fg(theme.info))
            })
            .collect();
        let rest = [Span::raw("  ".to_string())];
        for line in wrap(&chips, ctx.width, &rest, &rest) {
            lines.push(line.style(Style::new().bg(bg)));
        }
    }
    lines
}

fn reasoning(ctx: &Ctx, event: &ConversationEvent) -> Vec<Line<'static>> {
    let EventBody::AssistantReasoning { text, status, .. } = &event.body else {
        return Vec::new();
    };
    let theme = ctx.theme;
    if *status == AssistantSemanticStatus::Streaming {
        let mut spans = vec![Span::styled(
            format!("{} ", spinner(ctx.phase)),
            theme.primary_text(),
        )];
        spans.extend(shimmer(
            &tr!("conversation.block.thinkingActive"),
            theme.muted_fg,
            theme.fg,
            ctx.phase,
        ));
        spans.push(Span::styled(
            format!("  {}", compact_count(text.chars().count() as u64)),
            theme.muted_text(),
        ));
        return vec![Line::from(spans)];
    }
    let open = expanded(ctx, event.seq, false);
    let mut lines = vec![Line::from(vec![
        Span::styled(fold_marker(open), theme.muted_text()),
        Span::styled(
            tr!("conversation.block.thinking"),
            Style::new().fg(theme.violet),
        ),
        Span::styled(
            format!(
                " · {} {}",
                compact_count(text.chars().count() as u64),
                tr!("conversation.toolRequest.characters")
            ),
            theme.muted_text(),
        ),
    ])];
    if open {
        let bar = Span::styled("  │ ".to_string(), Style::new().fg(theme.border));
        let style = theme.muted_text().add_modifier(Modifier::ITALIC);
        for paragraph in text.split('\n') {
            let prefix = std::slice::from_ref(&bar);
            lines.extend(wrap(
                &[Span::styled(paragraph.to_string(), style)],
                ctx.width,
                prefix,
                prefix,
            ));
        }
    }
    lines
}

/// Render assistant Markdown, including closed answer components, with its streaming state.
/// 渲染助手 Markdown（含已闭合的回答组件）及其流式状态。
fn text(ctx: &Ctx, event: &ConversationEvent) -> Vec<Line<'static>> {
    let EventBody::AssistantText {
        text,
        status,
        code_snapshots,
    } = &event.body
    else {
        return Vec::new();
    };
    let code = match (status, code_snapshots) {
        (_, Some(snapshots)) => CodeSnapshots::Sealed(snapshots),
        (AssistantSemanticStatus::Streaming, None) => CodeSnapshots::Pending,
        _ => CodeSnapshots::None,
    };
    let mut lines = markdown::render_with_code(text, ctx.width, ctx.theme, code);
    match status {
        AssistantSemanticStatus::Streaming => {
            let caret = Span::styled(
                "▍",
                Style::new().fg(mix(ctx.theme.primary, ctx.theme.bg, pulse(ctx.phase))),
            );
            match lines.last_mut() {
                Some(last) if width_of(&line_text(last)) < ctx.width => last.spans.push(caret),
                _ => lines.push(Line::from(caret)),
            }
        }
        AssistantSemanticStatus::Interrupted => lines.push(Line::from(Span::styled(
            tr!("conversation.process.stopped"),
            Style::new().fg(ctx.theme.warning),
        ))),
        AssistantSemanticStatus::Completed => {}
    }
    lines
}

fn pulse(phase: f32) -> f32 {
    0.55 + 0.45 * (phase * 5.0).sin().abs()
}

fn line_text(line: &Line) -> String {
    line.spans.iter().map(|s| s.content.as_ref()).collect()
}

/// `(tool name, input, output, changeset, status, duration ms)` of one tool-call event.
/// 单个工具调用事件的 `(工具名, 输入, 输出, 变更集, 状态, 耗时毫秒)`。
type ToolParts<'a> = (
    &'a str,
    &'a Value,
    Option<&'a str>,
    &'a [ChangesetFile],
    ToolCallStatus,
    Option<u64>,
);

fn tool_parts(event: &ConversationEvent) -> Option<ToolParts<'_>> {
    match &event.body {
        EventBody::ToolCall {
            tool_name,
            input,
            output,
            changeset,
            status,
            started_at_ms,
            finished_at_ms,
            user_wait_ms,
            ..
        } => {
            let duration = started_at_ms
                .zip(*finished_at_ms)
                .map(|(s, f)| f.saturating_sub(s).saturating_sub(*user_wait_ms));
            Some((
                tool_name,
                input,
                output.as_deref(),
                changeset,
                *status,
                duration,
            ))
        }
        _ => None,
    }
}

/// The most telling input field of a tool call, shown next to its name.
/// 工具调用最具代表性的输入字段，显示在工具名旁。
fn tool_subject(input: &Value) -> String {
    for key in [
        "command",
        "path",
        "file_path",
        "pattern",
        "query",
        "url",
        "action",
        "description",
    ] {
        if let Some(value) = input.get(key).and_then(Value::as_str) {
            return value.lines().next().unwrap_or_default().to_string();
        }
    }
    String::new()
}

fn status_icon(theme: &Theme, status: ToolCallStatus, phase: f32) -> Span<'static> {
    match status {
        ToolCallStatus::Prepared | ToolCallStatus::Running => {
            Span::styled(spinner(phase).to_string(), theme.primary_text())
        }
        ToolCallStatus::WaitingUser => Span::styled(
            "?".to_string(),
            Style::new().fg(theme.warning).add_modifier(Modifier::BOLD),
        ),
        ToolCallStatus::Completed => Span::styled("✓".to_string(), Style::new().fg(theme.success)),
        ToolCallStatus::Failed => Span::styled("✗".to_string(), Style::new().fg(theme.danger)),
        ToolCallStatus::Cancelled | ToolCallStatus::Stopped | ToolCallStatus::Interrupted => {
            Span::styled("■".to_string(), Style::new().fg(theme.warning))
        }
    }
}

fn tools(ctx: &Ctx, group: &[&ConversationEvent]) -> Vec<Line<'static>> {
    let theme = ctx.theme;
    let parts: Vec<_> = group.iter().filter_map(|e| tool_parts(e)).collect();
    let running = parts.iter().filter(|p| p.4.is_open()).count();
    let completed = parts.len() - running;
    let open = expanded(ctx, group[0].seq, running > 0);
    let failed = parts
        .iter()
        .filter(|p| p.4 == ToolCallStatus::Failed)
        .count();
    let total_ms: u64 = parts.iter().filter_map(|p| p.5).sum();

    let mut header = vec![Span::styled(fold_marker(open), theme.muted_text())];
    if running > 0 {
        header.push(Span::styled(
            format!("{} ", spinner(ctx.phase)),
            theme.primary_text(),
        ));
        header.extend(shimmer(
            &tr!(
                "conversation.toolSequence.runningSummary",
                completed = completed,
                running = running
            ),
            theme.muted_fg,
            theme.fg,
            ctx.phase,
        ));
    } else {
        header.push(Span::styled(
            tr!("conversation.toolSequence.summary", count = parts.len()),
            Style::new().fg(theme.info),
        ));
        if total_ms > 0 {
            header.push(Span::styled(
                format!(" · {}", compact_duration(total_ms)),
                theme.muted_text(),
            ));
        }
        if failed > 0 {
            header.push(Span::styled(
                format!(" · ✗ {failed}"),
                Style::new().fg(theme.danger),
            ));
        }
    }
    let mut lines = vec![Line::from(header)];
    if !open {
        return lines;
    }
    for (name, input, output, changeset, status, duration) in parts {
        let mut spans = vec![
            Span::raw("  ".to_string()),
            status_icon(theme, status, ctx.phase),
            Span::raw(" ".to_string()),
        ];
        spans.push(Span::styled(name.to_string(), theme.strong()));
        let subject = tool_subject(input);
        let tail = duration.map(compact_duration).unwrap_or_default();
        let budget = ctx
            .width
            .saturating_sub(width_of(name) + width_of(&tail) + 7);
        if !subject.is_empty() && budget > 4 {
            spans.push(Span::styled(
                format!("  {}", truncate(&subject, budget)),
                theme.muted_text(),
            ));
        }
        if !tail.is_empty() {
            spans.push(Span::styled(
                format!("  {tail}"),
                Style::new().fg(theme.faint_fg(theme.bg)),
            ));
        }
        lines.push(Line::from(spans));
        for file in changeset {
            lines.push(Line::from(vec![
                Span::styled("      ".to_string(), theme.muted_text()),
                Span::styled(
                    truncate(&file.path, ctx.width.saturating_sub(20)),
                    Style::new().fg(theme.fg),
                ),
                Span::styled(
                    format!("  +{}", file.additions),
                    Style::new().fg(theme.success),
                ),
                Span::styled(
                    format!(" -{}", file.deletions),
                    Style::new().fg(theme.danger),
                ),
            ]));
        }
        let show_output = ctx.session.reveal
            || matches!(status, ToolCallStatus::Failed | ToolCallStatus::WaitingUser)
            || name == "shell_command";
        if show_output && let Some(output) = output {
            let bar = Span::styled("      │ ".to_string(), Style::new().fg(theme.border));
            let color = if status == ToolCallStatus::Failed {
                theme.danger
            } else {
                theme.muted_fg
            };
            let out_lines: Vec<&str> = output.lines().collect();
            let skip = out_lines.len().saturating_sub(4);
            if skip > 0 {
                lines.push(Line::from(vec![
                    bar.clone(),
                    Span::styled(format!("… +{skip}"), theme.muted_text()),
                ]));
            }
            for line in &out_lines[skip..] {
                lines.push(Line::from(vec![
                    bar.clone(),
                    Span::styled(
                        truncate(line, ctx.width.saturating_sub(8)),
                        Style::new().fg(color),
                    ),
                ]));
            }
        }
    }
    lines
}

fn note(ctx: &Ctx, event: &ConversationEvent) -> Vec<Line<'static>> {
    let EventBody::DisplayNote { severity, message } = &event.body else {
        return Vec::new();
    };
    let theme = ctx.theme;
    let (icon, color) = severity_icon(theme, *severity);
    let first = [Span::styled(format!("{icon} "), Style::new().fg(color))];
    let rest = [Span::raw("  ".to_string())];
    wrap(
        &[Span::styled(message.clone(), Style::new().fg(color))],
        ctx.width,
        &first,
        &rest,
    )
}

pub fn severity_icon(
    theme: &Theme,
    severity: NoticeSeverity,
) -> (&'static str, ratatui::style::Color) {
    match severity {
        NoticeSeverity::Info => ("ℹ", theme.info),
        NoticeSeverity::Warning => ("⚠", theme.warning),
        NoticeSeverity::Error => ("✗", theme.danger),
    }
}

fn stats(ctx: &Ctx, event: &ConversationEvent) -> Vec<Line<'static>> {
    let EventBody::TurnStats {
        outcome,
        duration_ms,
        total_tokens,
        requests,
        tool_calls,
    } = &event.body
    else {
        return Vec::new();
    };
    let theme = ctx.theme;
    let duration = compact_duration(*duration_ms);
    let lead = match outcome {
        TurnOutcome::Completed => format!("{} {duration}", tr!("conversation.process.worked")),
        TurnOutcome::Stopped => tr!("conversation.process.stoppedAfter", duration = duration),
        TurnOutcome::Abandoned => tr!("tui.turn.abandoned"),
    };
    let parts = [
        lead,
        tr!("conversation.stats.providerCalls", count = requests),
        tr!("conversation.stats.toolCalls", count = tool_calls),
        tr!(
            "conversation.stats.totalTokens",
            count = compact_count(*total_tokens)
        ),
    ];
    let body = parts.join(" · ");
    let rule_color = mix(theme.border, theme.bg, 0.9);
    let fill = ctx.width.saturating_sub(width_of(&body) + 4);
    let color = if *outcome == TurnOutcome::Completed {
        theme.muted_fg
    } else {
        theme.warning
    };
    vec![Line::from(vec![
        Span::styled("── ".to_string(), Style::new().fg(rule_color)),
        Span::styled(body, Style::new().fg(color)),
        Span::styled(
            format!(" {}", "─".repeat(fill)),
            Style::new().fg(rule_color),
        ),
    ])]
}

/// Empty-conversation hero: the text wordmark with a start hint.
/// 空会话主视觉：文字字标与开始提示。
fn welcome(theme: &Theme, area: Rect, buf: &mut Buffer, has_conversation: bool) {
    if area.height < 3 {
        return;
    }
    let y = area.y + (area.height - 3) / 2;
    let centered = |text_width: usize| area.x + area.width.saturating_sub(text_width as u16) / 2;
    let wordmark = brand::wordmark(theme);
    let w = wordmark.width();
    wordmark.render(
        Rect {
            x: centered(w),
            y,
            width: (w as u16).min(area.width),
            height: 1,
        },
        buf,
    );
    let hint = if has_conversation {
        tr!("tui.empty.hint")
    } else {
        tr!("sidebar.waitingWorkspace")
    };
    let hint = truncate(&hint, area.width.saturating_sub(4) as usize);
    buf.set_string(centered(width_of(&hint)), y + 2, &hint, theme.muted_text());
}
