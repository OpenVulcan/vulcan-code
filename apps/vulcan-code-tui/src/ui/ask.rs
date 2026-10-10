// The non-modal `ask` panel shown above the composer while a question waits for the user.
// 有问题等待用户时显示在输入框上方的非模态 `ask` 面板。
//
// Layout: a stepper with one chip per question (✓ answered, ● current, ○ open, ! missing), the
// current question with its options and the free-form answer, and a fixed button row at the bottom
// (‹ previous, next › or submit, ignore). Moving between questions is always an explicit button or
// key, never a side effect of choosing an option.
// 布局：步进条（每题一个标记：✓ 已答、● 当前、○ 未答、! 缺失）、当前问题及其选项与自由填写行，
// 以及底部固定的按钮行（‹ 上一题、下一题 › 或提交、忽略）。切换题目始终是显式的按钮或按键，
// 从不是选择选项的副作用。

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Widget};

use super::chrome::{ButtonKind, button, button_style};
use super::text::{truncate, width_of, wrap};
use super::theme::mix;
use crate::app::action::{AskStep, UiAction};
use crate::app::hit::Target;
use crate::app::{App, Focus};
use crate::model::{
    ApprovalJudge, ApprovalReview, ApprovalSubject, PendingApproval, PendingQuestion,
};
use crate::tr;

/// One body row and what clicking it (or parts of it) does.
/// 一行正文，以及点击该行（或其中部分）的效果。
struct Row {
    line: Line<'static>,
    target: Option<Target>,
    /// Clickable segments `(x offset, width, target)`.
    /// 可点击片段 `(x 偏移, 宽度, 目标)`。
    chips: Vec<(u16, u16, Target)>,
}

impl Row {
    fn plain(line: Line<'static>) -> Self {
        Row {
            line,
            target: None,
            chips: Vec::new(),
        }
    }
}

fn answered(app: &App, question: usize) -> bool {
    app.ask
        .selections
        .get(&question)
        .is_some_and(|s| !s.is_empty())
        || !app.ask.other_text(question).trim().is_empty()
}

fn stepper(app: &App, pending: &PendingQuestion, width: usize) -> Vec<Row> {
    let theme = &app.theme;
    let total = pending.questions.len();
    let index = app.ask.index.min(total.saturating_sub(1));
    let lead = format!(
        "{}  ",
        tr!("tui.ask.step", current = index + 1, total = total)
    );
    let mut rows = Vec::new();
    let mut spans = vec![Span::styled(lead.clone(), theme.muted_text())];
    let mut chips = Vec::new();
    let mut x = width_of(&lead);
    for (i, question) in pending.questions.iter().enumerate() {
        let (mark, style) = if i == index {
            (
                "●",
                Style::new()
                    .fg(theme.primary_fg)
                    .bg(theme.primary)
                    .add_modifier(Modifier::BOLD),
            )
        } else if app.ask.missing == Some(i) {
            ("!", Style::new().fg(theme.primary_fg).bg(theme.danger))
        } else if answered(app, i) {
            (
                "✓",
                Style::new()
                    .fg(theme.success)
                    .bg(mix(theme.success, theme.card, 0.15)),
            )
        } else {
            ("○", Style::new().fg(theme.muted_fg).bg(theme.muted))
        };
        let text = format!(" {mark} {} ", truncate(&question.header, 16));
        let w = width_of(&text);
        if x + w > width && !spans.is_empty() {
            rows.push(Row {
                line: Line::from(std::mem::take(&mut spans)),
                target: None,
                chips: std::mem::take(&mut chips),
            });
            x = 0;
        }
        chips.push((x as u16, w as u16, Target::AskQuestion(i)));
        spans.push(Span::styled(text, style));
        spans.push(Span::raw(" "));
        x += w + 1;
    }
    rows.push(Row {
        line: Line::from(spans),
        target: None,
        chips,
    });
    rows
}

/// Translated label of one backend high-risk key; unknown keys show as sent.
/// 单个后端高风险键的翻译标签；未知键按原样显示。
fn risk_label(key: &str) -> String {
    match key {
        "recursive_delete" => tr!("ask.approval.risk.recursiveDelete"),
        "destructive_git" => tr!("ask.approval.risk.destructiveGit"),
        "disk_format" => tr!("ask.approval.risk.diskFormat"),
        "system_configuration" => tr!("ask.approval.risk.systemConfiguration"),
        "opaque_script" => tr!("ask.approval.risk.opaqueScript"),
        "database_drop" => tr!("ask.approval.risk.databaseDrop"),
        "permission_change" => tr!("ask.approval.risk.permissionChange"),
        "publish" => tr!("ask.approval.risk.publish"),
        "infrastructure_destroy" => tr!("ask.approval.risk.infrastructureDestroy"),
        other => other.to_string(),
    }
}

/// Body rows of a tool-call approval: what would run, the mode, and any high risk.
/// 工具调用审批的正文行：将执行的内容、模式及高风险（如有）。
fn approval_rows(app: &App, approval: &PendingApproval, width: usize) -> Vec<Row> {
    let theme = &app.theme;
    let mut rows = Vec::new();
    let push_wrapped = |rows: &mut Vec<Row>, text: String, style: Style| {
        for line in wrap(&[Span::styled(text, style)], width, &[], &[]) {
            rows.push(Row::plain(line));
        }
    };
    if app.ask.confirm_reject {
        push_wrapped(
            &mut rows,
            tr!("ask.confirm.rejectTitle"),
            Style::new().fg(theme.danger).add_modifier(Modifier::BOLD),
        );
        push_wrapped(&mut rows, tr!("ask.approval.stopBody"), theme.muted_text());
        return rows;
    }
    let tool = approval.tool_name.clone();
    let (headline, lines) = match &approval.subject {
        ApprovalSubject::Command(text) if text.trim().is_empty() => {
            (tr!("ask.approval.shell", tool = tool), Vec::new())
        }
        ApprovalSubject::Command(text) => (
            tr!("ask.approval.command", tool = tool),
            text.lines().map(str::to_string).collect(),
        ),
        ApprovalSubject::FileWrite(paths) => {
            (tr!("ask.approval.fileWrite", tool = tool), paths.clone())
        }
        ApprovalSubject::Opaque => (tr!("ask.approval.opaque", tool = tool), Vec::new()),
    };
    push_wrapped(&mut rows, headline, theme.strong());
    for line in lines {
        push_wrapped(&mut rows, format!("  {line}"), theme.primary_text());
    }
    push_wrapped(
        &mut rows,
        tr!(
            "ask.approval.mode",
            mode = crate::app::permission_mode::label(approval.permission_mode)
        ),
        theme.muted_text(),
    );
    if let Some(key) = &approval.high_risk {
        push_wrapped(
            &mut rows,
            tr!("ask.approval.highRisk", category = risk_label(key)),
            Style::new().fg(theme.danger).add_modifier(Modifier::BOLD),
        );
    }
    match &approval.judge {
        Some(ApprovalJudge::Scored(label)) => {
            let (text, style) = match label.as_str() {
                "allow" => (tr!("ask.approval.judge.allow"), theme.muted_text()),
                "review" => (tr!("ask.approval.judge.review"), theme.muted_text()),
                "block" => (
                    tr!("ask.approval.judge.block"),
                    Style::new().fg(theme.danger),
                ),
                other => (other.to_string(), theme.muted_text()),
            };
            push_wrapped(&mut rows, text, style);
        }
        Some(ApprovalJudge::Unavailable(reason)) => {
            push_wrapped(
                &mut rows,
                format!("{} · {reason}", tr!("ask.approval.judge.unavailable")),
                theme.muted_text(),
            );
        }
        None => {}
    }
    match &approval.review {
        Some(ApprovalReview::Reviewed { verdict, reason }) => {
            let label = match verdict.as_str() {
                "allow" => tr!("ask.approval.review.allow"),
                "ask_user" => tr!("ask.approval.review.askUser"),
                "deny" => tr!("ask.approval.review.deny"),
                other => other.to_string(),
            };
            push_wrapped(&mut rows, label, theme.muted_text());
            if !reason.is_empty() {
                push_wrapped(&mut rows, reason.clone(), theme.muted_text());
            }
        }
        Some(ApprovalReview::Unavailable(reason)) => {
            push_wrapped(
                &mut rows,
                format!("{} · {reason}", tr!("ask.approval.review.unavailable")),
                theme.muted_text(),
            );
        }
        None => {}
    }
    rows
}

fn body_rows(app: &App, pending: &PendingQuestion, width: usize) -> Vec<Row> {
    if let Some(approval) = &pending.approval {
        return approval_rows(app, approval, width);
    }
    let theme = &app.theme;
    let ask = &app.ask;
    let focused = app.focus == Focus::Ask;
    let total = pending.questions.len();
    let index = ask.index.min(total.saturating_sub(1));
    let question = &pending.questions[index];
    let mut rows = stepper(app, pending, width);

    if ask.confirm_reject {
        rows.push(Row::plain(Line::from(Span::styled(
            tr!("ask.confirm.rejectTitle"),
            Style::new().fg(theme.danger).add_modifier(Modifier::BOLD),
        ))));
        for line in wrap(
            &[Span::styled(
                tr!("ask.confirm.rejectBody"),
                theme.muted_text(),
            )],
            width,
            &[],
            &[],
        ) {
            rows.push(Row::plain(line));
        }
        return rows;
    }

    for line in wrap(
        &[Span::styled(question.question.clone(), theme.strong())],
        width,
        &[],
        &[],
    ) {
        rows.push(Row::plain(line));
    }
    let hint = if question.multiple {
        tr!("tui.ask.multiple")
    } else {
        tr!("tui.ask.single")
    };
    rows.push(Row::plain(Line::from(Span::styled(
        hint,
        Style::new().fg(theme.faint_fg(theme.card)),
    ))));
    for (i, option) in question.options.iter().enumerate() {
        let selected = ask.is_selected(index, i);
        let cursor = focused && ask.cursor == i && !ask.editing_other;
        let mark = match (question.multiple, selected) {
            (true, true) => "[x]",
            (true, false) => "[ ]",
            (false, true) => "(•)",
            (false, false) => "( )",
        };
        let row_style = if cursor {
            Style::new().bg(theme.selection_bg())
        } else {
            Style::new()
        };
        let mut spans = vec![
            Span::styled(if cursor { "› " } else { "  " }, theme.primary_text()),
            Span::styled(
                format!("{mark} "),
                if selected {
                    theme.primary_text()
                } else {
                    theme.muted_text()
                },
            ),
            Span::styled(format!("{}. ", i + 1), theme.muted_text()),
            Span::styled(
                option.label.clone(),
                if selected {
                    theme.strong()
                } else {
                    theme.base()
                },
            ),
        ];
        let used = 9 + width_of(&option.label);
        if !option.description.is_empty() && width > used + 6 {
            spans.push(Span::styled(
                format!("  {}", truncate(&option.description, width - used - 2)),
                theme.muted_text(),
            ));
        }
        rows.push(Row {
            line: Line::from(spans).style(row_style),
            target: Some(Target::AskOption(i)),
            chips: Vec::new(),
        });
    }
    let other_row = question.options.len();
    let other_cursor = focused && ask.cursor == other_row;
    let other = ask.other_text(index);
    let other_value = if other.is_empty() && !ask.editing_other {
        Span::styled(
            tr!("ask.custom.textPlaceholder"),
            Style::new().fg(theme.faint_fg(theme.bg)),
        )
    } else {
        Span::styled(
            format!("{other}{}", if ask.editing_other { "▏" } else { "" }),
            theme.base(),
        )
    };
    rows.push(Row {
        line: Line::from(vec![
            Span::styled(if other_cursor { "› " } else { "  " }, theme.primary_text()),
            Span::styled("✎ ", theme.muted_text()),
            Span::styled(
                format!("{}: ", tr!("ask.custom.adjustCodex")),
                theme.muted_text(),
            ),
            other_value,
        ])
        .style(if other_cursor || ask.editing_other {
            Style::new().bg(theme.selection_bg())
        } else {
            Style::new()
        }),
        target: Some(Target::AskOther),
        chips: Vec::new(),
    });
    if let Some(missing) = ask.missing {
        let header = pending
            .questions
            .get(missing)
            .map(|q| q.header.clone())
            .unwrap_or_default();
        rows.push(Row::plain(Line::from(Span::styled(
            tr!("ask.validation.requiredAnswer", header = header),
            Style::new().fg(theme.danger),
        ))));
    }
    rows
}

/// Height the panel needs (body, button row and borders), capped by `max`.
/// 面板所需高度（正文、按钮行与边框），不超过 `max`。
pub fn height(app: &App, pending: &PendingQuestion, width: u16, max: u16) -> u16 {
    let rows = body_rows(app, pending, width.saturating_sub(4) as usize).len() as u16;
    (rows + 4).min(max)
}

pub fn render(app: &App, pending: &PendingQuestion, area: Rect, buf: &mut Buffer) {
    let theme = &app.theme;
    let focused = app.focus == Focus::Ask;
    let border = if focused {
        theme.warning
    } else {
        mix(theme.warning, theme.border, 0.4)
    };
    let approval = pending.approval.as_ref();
    let title = if approval.is_some() {
        tr!("ask.approval.title")
    } else {
        tr!("ask.title")
    };
    let hints = if approval.is_some() && focused && app.ask.confirm_reject {
        format!(
            " Enter {} · Esc {} ",
            tr!("ask.action.confirmReject"),
            tr!("ask.approval.back")
        )
    } else if approval.is_some() && focused {
        format!(
            " a {} · d {} · x {} ",
            tr!("ask.approval.allow"),
            tr!("ask.approval.deny"),
            tr!("ask.approval.stop")
        )
    } else if app.ask.editing_other {
        format!(" Enter {} · Esc ", tr!("ask.action.continue"))
    } else if focused {
        format!(
            " ↑↓ · 1-9 · Space · Enter · ←→ · x {} ",
            tr!("ask.action.ignore")
        )
    } else {
        format!(" Tab → {} ", title)
    };
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(border))
        .style(Style::new().bg(theme.card))
        .title(Line::from(vec![
            Span::styled(
                " ? ",
                Style::new()
                    .fg(theme.primary_fg)
                    .bg(theme.warning)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {} ", title),
                Style::new().fg(theme.warning).add_modifier(Modifier::BOLD),
            ),
        ]))
        .title_bottom(Line::from(Span::styled(hints, theme.muted_text())).right_aligned());
    let inner = block.inner(area);
    block.render(area, buf);
    app.hits.add(area, Target::Focus(Focus::Ask));
    if inner.height < 2 {
        return;
    }
    let x = inner.x + 1;
    let width = inner.width.saturating_sub(1);
    // The last two inner rows hold a gap and the buttons so they stay visible when space is short.
    // 最后两行内区放置间隔与按钮，空间不足时按钮也保持可见。
    let body_height = inner.height.saturating_sub(2) as usize;
    let rows = body_rows(app, pending, width.saturating_sub(1) as usize);
    // Keep the option cursor visible when the body is taller than the panel.
    // 正文高于面板时保持选项光标可见。
    let cursor_row = rows
        .iter()
        .position(|r| matches!(r.target, Some(Target::AskOption(i)) if i == app.ask.cursor))
        .or_else(|| {
            rows.iter()
                .position(|r| matches!(r.target, Some(Target::AskOther)))
                .filter(|_| app.ask.editing_other)
        })
        .unwrap_or(0);
    let skip = if rows.len() > body_height && cursor_row >= body_height {
        (cursor_row + 1 - body_height).min(rows.len() - body_height)
    } else {
        0
    };
    // The stepper always stays on the first row.
    // 步进条始终位于首行。
    let mut visible: Vec<&Row> = Vec::new();
    if skip > 0 {
        visible.push(&rows[0]);
        visible.extend(
            rows.iter()
                .skip(skip + 1)
                .take(body_height.saturating_sub(1)),
        );
    } else {
        visible.extend(rows.iter().take(body_height));
    }
    for (i, row) in visible.into_iter().enumerate() {
        let rect = Rect {
            x,
            y: inner.y + i as u16,
            width,
            height: 1,
        };
        row.line.clone().render(rect, buf);
        if let Some(target) = &row.target {
            app.hits.add(rect, target.clone());
        }
        for (offset, w, target) in &row.chips {
            app.hits.add(
                Rect {
                    x: x + offset,
                    width: *w,
                    ..rect
                },
                target.clone(),
            );
        }
    }

    // Button row.
    // 按钮行。
    let y = inner.bottom() - 1;
    let bg = theme.card;
    if app.ask.confirm_reject {
        let mut bx = x;
        bx += button(
            app,
            buf,
            bx,
            y,
            &format!("{} ⏎", tr!("ask.action.confirmReject")),
            button_style(app, ButtonKind::Danger, bg),
            UiAction::Ask(AskStep::RejectConfirm),
        ) + 2;
        button(
            app,
            buf,
            bx,
            y,
            &format!(
                "{} Esc",
                if approval.is_some() {
                    tr!("ask.approval.back")
                } else {
                    tr!("ask.action.backToEdit")
                }
            ),
            button_style(app, ButtonKind::Normal, bg),
            UiAction::Ask(AskStep::RejectCancel),
        );
        return;
    }
    if approval.is_some() {
        // Allow once and Deny on the left, Stop session on the right; none is bound to Enter.
        // 左侧为允许本次与拒绝，右侧为停止会话；均不绑定 Enter。
        let mut bx = x;
        bx += button(
            app,
            buf,
            bx,
            y,
            &format!("{} a", tr!("ask.approval.allow")),
            button_style(app, ButtonKind::Primary, bg),
            UiAction::Ask(AskStep::Allow),
        ) + 1;
        bx += button(
            app,
            buf,
            bx,
            y,
            &format!("{} d", tr!("ask.approval.deny")),
            button_style(app, ButtonKind::Normal, bg),
            UiAction::Ask(AskStep::Deny),
        ) + 1;
        let stop = format!(" {} ", tr!("ask.approval.stop"));
        let sw = width_of(&stop) as u16 + 2;
        if inner.right() > bx + sw {
            button(
                app,
                buf,
                inner.right() - sw,
                y,
                &tr!("ask.approval.stop"),
                Style::new()
                    .fg(theme.danger)
                    .bg(mix(theme.danger, bg, 0.12)),
                UiAction::Ask(AskStep::Reject),
            );
        }
        return;
    }
    let total = pending.questions.len();
    let index = app.ask.index.min(total.saturating_sub(1));
    let last = index + 1 >= total;
    let mut bx = x;
    if total > 1 {
        let prev_label = format!("‹ {}", tr!("tui.ask.prev"));
        if index > 0 {
            bx += button(
                app,
                buf,
                bx,
                y,
                &prev_label,
                button_style(app, ButtonKind::Normal, bg),
                UiAction::Ask(AskStep::Prev),
            ) + 1;
        } else {
            let text = format!(" {prev_label} ");
            let surface = mix(theme.accent, bg, 0.5);
            buf.set_string(
                bx,
                y,
                &text,
                Style::new().fg(theme.faint_fg(surface)).bg(surface),
            );
            bx += width_of(&text) as u16 + 1;
        }
    }
    if last {
        bx += button(
            app,
            buf,
            bx,
            y,
            &format!("✓ {}", tr!("ask.action.submit")),
            button_style(app, ButtonKind::Primary, bg),
            UiAction::Ask(AskStep::Submit),
        ) + 1;
    } else {
        bx += button(
            app,
            buf,
            bx,
            y,
            &format!("{} ›", tr!("tui.ask.next")),
            button_style(app, ButtonKind::Primary, bg),
            UiAction::Ask(AskStep::Next),
        ) + 1;
        bx += button(
            app,
            buf,
            bx,
            y,
            &tr!("ask.action.submit"),
            button_style(app, ButtonKind::Normal, bg),
            UiAction::Ask(AskStep::Submit),
        ) + 1;
    }
    let ignore = format!(" {} ", tr!("ask.action.ignore"));
    let iw = width_of(&ignore) as u16 + 2;
    if inner.right() > bx + iw {
        button(
            app,
            buf,
            inner.right() - iw,
            y,
            &tr!("ask.action.ignore"),
            Style::new()
                .fg(theme.danger)
                .bg(mix(theme.danger, bg, 0.12)),
            UiAction::Ask(AskStep::Reject),
        );
    }
}
