// Session bar: one fixed row above the composer with the conversation's live numbers — context
// usage, the turn in progress (or the last one), cache hit rate, session totals, subagents, goal,
// background commands, memory and output speed. Each segment has a full and a compact form and a
// priority; when the row is too narrow the lowest priorities shrink first, then disappear.
// Clicking a segment opens the matching panel tab.
// 会话栏：输入框上方固定的一行，显示会话的实时数据——上下文用量、进行中（或上一）轮次、缓存命中率、
// 会话合计、子代理、目标、后台命令、记忆与输出速度。每段有完整与紧凑两种形式及优先级；行宽不足时
// 最低优先级的段先收缩、再隐藏。点击某段打开对应的面板标签页。

use std::time::Instant;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use super::chrome::usage_gauge;
use super::text::{compact_count, compact_duration, spinner, width_of};
use crate::app::action::UiAction;
use crate::app::activity::{ActivityAction, ActivityPage};
use crate::app::hit::Target;
use crate::app::{App, PanelTab, SessionState, insight};
use crate::model::{AgentState, SessionGoalStatus};
use crate::tr;

/// The conversation column must be at least this tall for the bar; shorter terminals keep the
/// rows for the transcript and show the context gauge in the status bar instead.
/// 对话列至少需要这么高才显示会话栏；更矮的终端把行留给会话记录，改在状态栏显示上下文进度条。
pub const MIN_COLUMN_HEIGHT: u16 = 14;

const SEPARATOR: &str = " │ ";

/// True when the bar is drawn for the open conversation in a column `column_height` rows tall.
/// 在高 `column_height` 行的列中为当前会话绘制会话栏时为真。
pub fn visible(app: &App, column_height: u16) -> bool {
    app.conversation().is_some() && app.session().is_some() && column_height >= MIN_COLUMN_HEIGHT
}

struct Segment {
    /// Higher stays longer; the lowest shrinks, then hides, first.
    /// 越高保留越久；最低的先收缩、再隐藏。
    priority: u8,
    full: Vec<Span<'static>>,
    compact: Option<Vec<Span<'static>>>,
    /// What a click does: opens the matching panel tab unless set otherwise.
    /// 点击的效果：除非另行设置，否则打开对应的面板标签页。
    action: UiAction,
    shrunk: bool,
}

impl Segment {
    fn new(priority: u8, tab: PanelTab, full: Vec<Span<'static>>) -> Self {
        Self {
            priority,
            full,
            compact: None,
            action: UiAction::ShowPanel(tab),
            shrunk: false,
        }
    }

    fn on_click(mut self, action: UiAction) -> Self {
        self.action = action;
        self
    }

    fn compact(mut self, spans: Vec<Span<'static>>) -> Self {
        self.compact = Some(spans);
        self
    }

    fn spans(&self) -> &[Span<'static>] {
        match &self.compact {
            Some(compact) if self.shrunk => compact,
            _ => &self.full,
        }
    }

    fn width(&self) -> usize {
        self.spans().iter().map(|s| width_of(&s.content)).sum()
    }
}

pub fn render(app: &App, area: Rect, buf: &mut Buffer) {
    let Some(session) = app.session() else {
        return;
    };
    if area.height == 0 {
        return;
    }
    let mut segments = segments(app, session);
    fit(&mut segments, area.width.saturating_sub(1) as usize);

    let theme = &app.theme;
    let separator = Style::new().fg(theme.border);
    let mut x = area.x + 1;
    for (i, segment) in segments.iter().enumerate() {
        if i > 0 {
            buf.set_string(x, area.y, SEPARATOR, separator);
            x += width_of(SEPARATOR) as u16;
        }
        let width = segment.width() as u16;
        let rect = Rect {
            x,
            y: area.y,
            width: width.min(area.right().saturating_sub(x)),
            height: 1,
        };
        Line::from(segment.spans().to_vec()).render(rect, buf);
        app.hits.add(rect, Target::Do(segment.action.clone()));
        x += width;
    }
}

/// Shrinks, then drops, the lowest-priority segments until the row fits in `width` columns, then
/// gives the room left back to the highest-priority shrunk segments that fit in full again.
/// 依次收缩、再移除最低优先级的段，直到该行能放进 `width` 列；随后把剩余空间还给能重新完整放下的
/// 最高优先级收缩段。
fn fit(segments: &mut Vec<Segment>, width: usize) {
    let total = |segments: &[Segment]| -> usize {
        segments.iter().map(Segment::width).sum::<usize>()
            + width_of(SEPARATOR) * segments.len().saturating_sub(1)
    };
    while total(segments) > width {
        let shrink = segments
            .iter_mut()
            .filter(|s| s.compact.is_some() && !s.shrunk)
            .min_by_key(|s| s.priority);
        if let Some(segment) = shrink {
            segment.shrunk = true;
            continue;
        }
        let Some(lowest) = segments
            .iter()
            .enumerate()
            .min_by_key(|(_, s)| s.priority)
            .map(|(i, _)| i)
        else {
            return;
        };
        segments.remove(lowest);
    }
    let mut order: Vec<usize> = (0..segments.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(segments[i].priority));
    for i in order {
        if !segments[i].shrunk {
            continue;
        }
        segments[i].shrunk = false;
        if total(segments) > width {
            segments[i].shrunk = true;
        }
    }
}

fn segments(app: &App, session: &SessionState) -> Vec<Segment> {
    let theme = &app.theme;
    let label = theme.muted_text();
    let value = Style::new().fg(theme.fg);
    let strong = value.add_modifier(Modifier::BOLD);
    let mut out = Vec::new();

    // Context usage.
    // 上下文用量。
    let usage = session.usage;
    if usage.max_tokens > 0 {
        let percent = usage.percent();
        let color = Style::new().fg(theme.usage_color(percent));
        let mut full = vec![Span::styled(
            format!("{} ", tr!("contextUsage.title")),
            label,
        )];
        full.extend(usage_gauge(theme, percent));
        full.push(Span::styled(format!(" {percent}%"), color));
        full.push(Span::styled(
            format!(
                " {}/{}",
                compact_count(usage.used_tokens),
                compact_count(usage.max_tokens)
            ),
            label,
        ));
        let mut compact = usage_gauge(theme, percent);
        compact.push(Span::styled(format!(" {percent}%"), color));
        out.push(Segment::new(8, PanelTab::Stats, full).compact(compact));
    }

    // The turn in progress, else the last finished one.
    // 进行中的轮次，否则为上一个已结束的轮次。
    if let Some(turn) = session.live_turn(Instant::now()) {
        let spin = Span::styled(
            format!("{} ", spinner(app.phase())),
            theme.primary_text().add_modifier(Modifier::BOLD),
        );
        let elapsed = Span::styled(compact_duration(turn.elapsed_ms), value);
        let approx = if turn.streaming > 0 { "~" } else { "" };
        let output = Span::styled(
            format!(
                " ↓{approx}{}",
                compact_count(turn.usage.output + turn.streaming)
            ),
            strong,
        );
        let full = vec![
            spin.clone(),
            Span::styled(format!("{} ", tr!("tui.bar.turn")), label),
            elapsed.clone(),
            Span::styled(format!(" ↑{}", compact_count(turn.usage.input)), value),
            output.clone(),
        ];
        out.push(Segment::new(7, PanelTab::Stats, full).compact(vec![spin, elapsed, output]));
    } else if let Some((duration_ms, total_tokens, _)) = session.last_turn() {
        let full = vec![
            Span::styled(format!("{} ", tr!("tui.bar.lastTurn")), label),
            Span::styled(compact_count(total_tokens), value),
            Span::styled(format!(" · {}", compact_duration(duration_ms)), label),
        ];
        let compact = vec![
            Span::styled("↺ ", label),
            Span::styled(compact_count(total_tokens), value),
        ];
        out.push(Segment::new(7, PanelTab::Stats, full).compact(compact));
    }

    let lifetime = session.lifetime_total();

    // Cache hit rate over the whole session.
    // 整个会话的缓存命中率。
    if let Some(rate) = lifetime.cache_hit_rate() {
        let percent = Span::styled(
            format!("{:.0}%", rate * 100.0),
            Style::new().fg(theme.success),
        );
        out.push(
            Segment::new(
                6,
                PanelTab::Stats,
                vec![
                    Span::styled(format!("{} ", tr!("composer.goal.cacheHit")), label),
                    percent.clone(),
                ],
            )
            .compact(vec![
                Span::styled(format!("{} ", tr!("tui.bar.cache")), label),
                percent,
            ]),
        );
    }

    // Session totals, main session plus subagents.
    // 会话合计，主会话加上子代理。
    if lifetime.requests > 0 {
        let bound = if lifetime.partial() { "≥" } else { "" };
        let total = Span::styled(
            format!("Σ {bound}{}", compact_count(lifetime.total())),
            value,
        );
        out.push(
            Segment::new(
                5,
                PanelTab::Stats,
                vec![
                    total.clone(),
                    Span::styled(
                        format!(" · {}", tr!("tui.bar.requests", count = lifetime.requests)),
                        label,
                    ),
                ],
            )
            .compact(vec![total]),
        );
    }

    // Subagents: running / all.
    // 子代理：运行中 / 全部。
    if !session.agents.is_empty() {
        let running = session
            .agents
            .iter()
            .filter(|a| a.state == AgentState::Running)
            .count();
        let count = Span::styled(
            format!("{running}/{}", session.agents.len()),
            if running > 0 {
                theme.primary_text()
            } else {
                value
            },
        );
        out.push(
            Segment::new(
                4,
                PanelTab::Agents,
                vec![
                    Span::styled(format!("⧉ {} ", tr!("tui.bar.agents")), label),
                    count.clone(),
                ],
            )
            .compact(vec![Span::styled("⧉ ", label), count])
            .on_click(UiAction::SessionActivity(ActivityAction::Show(
                ActivityPage::Subagents,
            ))),
        );
    }

    // Background commands still running in this session.
    // 本会话中仍在运行的后台命令。
    let background = app
        .selected
        .as_deref()
        .map_or(0, |id| app.activity.background_count(id));
    if background > 0 {
        let count = Span::styled(background.to_string(), theme.primary_text());
        out.push(
            Segment::new(
                4,
                PanelTab::Agents,
                vec![
                    Span::styled(
                        format!("$ {} ", tr!("sessionActivity.backgroundCommands")),
                        label,
                    ),
                    count.clone(),
                ],
            )
            .compact(vec![Span::styled("$ ", label), count])
            .on_click(UiAction::SessionActivity(ActivityAction::Show(
                ActivityPage::Commands,
            ))),
        );
    }

    // Goal: share of the token budget, else its status.
    // 目标：token 预算的使用比例，否则为其状态。
    if let Some(goal) = &session.insight.goal {
        let color = match goal.status {
            SessionGoalStatus::Active => theme.primary,
            SessionGoalStatus::Complete => theme.success,
            SessionGoalStatus::Paused => theme.muted_fg,
            _ => theme.warning,
        };
        let style = Style::new().fg(color);
        let budget = goal
            .budget
            .filter(|b| *b > 0)
            .map(|b| format!("{:.0}%", goal.usage.total() as f32 * 100.0 / b as f32));
        let full = vec![
            Span::styled("◎ ", style),
            Span::styled(insight::goal_status_label(goal.status), style),
            Span::styled(
                budget.as_ref().map_or(String::new(), |b| format!(" {b}")),
                value,
            ),
        ];
        let compact = vec![
            Span::styled("◎", style),
            Span::styled(budget.map_or(String::new(), |b| format!(" {b}")), value),
        ];
        out.push(Segment::new(3, PanelTab::Stats, full).compact(compact));
    }

    // Memory injected into the context: dynamic · static.
    // 注入上下文的记忆：动态 · 静态。
    let memory = &session.insight.memory;
    let (dynamic, fixed) = (memory.dynamic_count(), memory.static_count());
    if dynamic + fixed > 0 {
        let counts = Span::styled(format!("{dynamic}·{fixed}"), value);
        out.push(
            Segment::new(
                2,
                PanelTab::Memory,
                vec![
                    Span::styled(format!("◈ {} ", tr!("contextPanel.memory")), label),
                    counts.clone(),
                ],
            )
            .compact(vec![Span::styled("◈ ", label), counts]),
        );
    }

    // Average output speed.
    // 平均输出速度。
    if let Some(speed) = lifetime.average_speed() {
        out.push(Segment::new(
            1,
            PanelTab::Stats,
            vec![Span::styled(format!("{speed:.0} tok/s"), label)],
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(priority: u8, full: &str, compact: Option<&str>) -> Segment {
        let spans = |text: &str| vec![Span::raw(text.to_string())];
        let segment = Segment::new(priority, PanelTab::Stats, spans(full));
        match compact {
            Some(text) => segment.compact(spans(text)),
            None => segment,
        }
    }

    fn widths(segments: &[Segment]) -> Vec<(u8, usize)> {
        segments.iter().map(|s| (s.priority, s.width())).collect()
    }

    #[test]
    fn fit_shrinks_lowest_priority_first_then_drops() {
        let build = || {
            vec![
                segment(3, "aaaaaaaaaa", Some("aa")),
                segment(2, "bbbbbbbbbb", Some("bb")),
                segment(1, "cccccccccc", None),
            ]
        };
        // Everything fits: 3 × 10 + 2 separators.
        // 全部放得下：3 × 10 加 2 个分隔符。
        let mut all = build();
        fit(&mut all, 36);
        assert_eq!(widths(&all), vec![(3, 10), (2, 10), (1, 10)]);

        // Priority 2 shrinks before priority 3; priority 1 has no compact form and stays whole.
        // 优先级 2 先于优先级 3 收缩；优先级 1 没有紧凑形式，保持完整。
        let mut shrunk = build();
        fit(&mut shrunk, 30);
        assert_eq!(widths(&shrunk), vec![(3, 10), (2, 2), (1, 10)]);

        // With every compact form used, the lowest priority is dropped.
        // 所有紧凑形式都用上后，移除最低优先级。
        let mut dropped = build();
        fit(&mut dropped, 10);
        assert_eq!(widths(&dropped), vec![(3, 2), (2, 2)]);
    }

    #[test]
    fn fit_gives_spare_room_back_to_segments_that_fit_in_full() {
        // Both shrink to reach 20 columns; 2 spare columns let priority 2 (+2) expand again,
        // while priority 3 (+8) stays compact.
        // 两段都收缩后才能放进 20 列；剩余 2 列足以让优先级 2（+2）重新展开，优先级 3（+8）保持紧凑。
        let mut segments = vec![
            segment(3, "aaaaaaaaaa", Some("aa")),
            segment(2, "bbbb", Some("bb")),
            segment(1, "cccccccccc", None),
        ];
        fit(&mut segments, 22);
        assert_eq!(widths(&segments), vec![(3, 2), (2, 4), (1, 10)]);
    }
}
