// Job log overlay of the session activity: the bounded log of one command job, mirroring the
// WebUI `shell-job-log-view.tsx`. It shows the newest lines first and follows a running job while
// the view stays at the bottom; scrolling to the top reads one earlier page at a time.
// 会话活动的任务日志浮层：一个命令任务的有界日志，对应 WebUI 的 `shell-job-log-view.tsx`。先显示
// 最新的行，视图停留在底部时跟随运行中的任务；滚动到顶部时每次读取更早的一页。

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use super::chrome::ButtonKind;
use super::overlays::{button_row, frame, modal};
use super::text::{spinner, truncate, wrap_verbatim};
use crate::app::App;
use crate::app::action::UiAction;
use crate::app::activity::{ActivityAction, ShellJobLog};
use crate::app::hit::Target;
use crate::tr;

/// Draws the log of `log` over the whole screen but a small margin.
/// 在除少量边距外的整个屏幕上绘制 `log` 的日志。
pub fn render(app: &App, log: &mut ShellJobLog, area: Rect, buf: &mut Buffer) {
    let theme = &app.theme;
    let rect = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(2),
    };
    let footer = "↑↓ · PgUp/PgDn · Home/End · s · Esc".to_string();
    let inner = modal(app, rect, frame(theme, log.title.clone(), footer), buf);
    if inner.height < 4 || inner.width < 8 {
        return;
    }
    let width = inner.width.saturating_sub(2) as usize;
    let x = inner.x + 1;
    buf.set_string(
        x,
        inner.y,
        truncate(&format!("$ {}", log.command), width),
        theme.muted_text(),
    );

    // The last inner row holds the buttons.
    // 最后一行为按钮。
    let bar_y = inner.bottom() - 1;
    let mut buttons = Vec::new();
    if log.running {
        buttons.push((
            tr!("sessionActivity.stop"),
            ButtonKind::Danger,
            UiAction::SessionActivity(ActivityAction::StopJob(log.job_id.clone())),
        ));
    }
    button_row(app, buf, inner, bar_y, buttons);
    if log.running {
        buf.set_string(
            x,
            bar_y,
            format!(
                "{} {}",
                spinner(app.phase()),
                tr!("sessionActivity.status.running")
            ),
            theme.primary_text(),
        );
    }

    let mut top = inner.y + 1;

    if log.has_earlier || log.earlier_pending() {
        let text = if log.earlier_pending() {
            format!("{} {}", spinner(app.phase()), tr!("tui.page.loading"))
        } else {
            format!("↑ {}", tr!("sessionActivity.scrollForEarlier"))
        };
        let line = Rect {
            x,
            y: top,
            width: width as u16,
            height: 1,
        };
        buf.set_string(x, top, truncate(&text, width), theme.primary_text());
        app.hits.add(
            line,
            Target::Do(UiAction::SessionActivity(ActivityAction::LogEarlier)),
        );
        top += 1;
    }
    if let Some(error) = &log.error {
        buf.set_string(
            x,
            top,
            truncate(error, width),
            Style::new().fg(theme.danger),
        );
        top += 1;
    }
    let bottom = bar_y.max(top);
    let height = (bottom - top) as usize;
    if height == 0 {
        return;
    }
    if log.rows.is_empty() {
        let text = if log.loaded {
            tr!("sessionActivity.noOutput")
        } else {
            tr!("tui.page.loading")
        };
        if log.error.is_none() {
            buf.set_string(x, top, truncate(&text, width), theme.muted_text());
        }
        log.top = 0;
        log.max_top = 0;
        return;
    }

    // Each row wraps to the width; gap rows mark lines the service no longer keeps.
    // 每行按宽度换行；间隔行标记服务不再保留的行。
    let faint = Style::new().fg(theme.faint_fg(theme.popover));
    let gap = Style::new()
        .fg(theme.warning)
        .add_modifier(Modifier::ITALIC);
    let text = Style::new().fg(theme.fg);
    let wrapped: Vec<Vec<Line<'static>>> = log
        .rows
        .iter()
        .map(|row| {
            let style = if row.kind == "text" { text } else { gap };
            let prefix = if row.continuation { "  " } else { "" };
            let mut lines =
                wrap_verbatim(&row.text, width.saturating_sub(prefix.len()).max(1), style);
            if lines.is_empty() {
                lines.push(Line::default());
            }
            if row.continuation {
                for line in &mut lines {
                    line.spans.insert(0, Span::styled(prefix, faint));
                }
            }
            lines
        })
        .collect();

    // The largest first row that still fills the view from the newest row upward.
    // 从最新的行向上仍能填满视图的最大首行。
    let mut filled = 0;
    log.max_top = 0;
    for (index, lines) in wrapped.iter().enumerate().rev() {
        filled += lines.len();
        if filled >= height {
            log.max_top = index;
            break;
        }
    }
    if log.follow || log.top > log.max_top {
        log.top = log.max_top;
    }
    let shown: Vec<&Line<'static>> = if log.follow {
        // Bottom-aligned, so the newest line is always in view.
        // 底部对齐，使最新的行始终可见。
        let mut lines: Vec<&Line<'static>> = wrapped.iter().flatten().collect();
        let skip = lines.len().saturating_sub(height);
        lines.drain(..skip);
        lines
    } else {
        wrapped[log.top..].iter().flatten().take(height).collect()
    };
    for (offset, line) in shown.into_iter().enumerate() {
        let row = Rect {
            x,
            y: top + offset as u16,
            width: width as u16,
            height: 1,
        };
        line.clone().render(row, buf);
    }
}
