// System area view: a navigation column and one generic renderer for every declarative page.
// 系统区域视图：导航列，以及服务所有声明式页面的通用渲染器。
//
// Rows have different heights (wrapped notes, item details, expanded lines), so the page
// is laid out first into blocks of lines, then scrolled by line. Every row registers a `PageRow`
// hit over its whole block; steppers and item actions register `PageRowAction` chips on top.
// 行高度各不相同（换行的说明、条目详情、展开行），因此页面先排版为若干行块，再按行滚动。
// 每行在其整个块上登记 `PageRow` 命中；步进器与条目操作在其上登记 `PageRowAction` 按钮。

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Widget};

use super::brand;
use super::chrome::{ButtonKind, button, button_style, overflow_menu};
use super::text::{OVERFLOW_CHIP, fit_strip, spinner, truncate, width_of, wrap};
use super::theme::{Theme, mix};
use crate::app::action::UiAction;
use crate::app::hit::{ScrollTarget, Target};
use crate::app::overlay::MenuItem;
use crate::app::pages::{self, Row, RowKind, Tone};
use crate::app::{App, Focus};
use crate::tr;

/// Narrowest navigation column; it grows to fit its entries up to a third of the area.
/// 导航列最小宽度；随条目增宽，最多占区域的三分之一。
const NAV_MIN_WIDTH: u16 = 20;
/// Narrowest label column of a page; it grows to fit the page's labels.
/// 页面标签列最小宽度；随页面标签增宽。
const LABEL_MIN_WIDTH: usize = 12;

pub fn render(app: &mut App, area: Rect, buf: &mut Buffer) {
    let theme = app.theme;
    buf.set_style(area, theme.base());
    let nav_width = nav_width(app, area.width);
    let nav = Rect {
        width: nav_width,
        ..area
    };
    let body = Rect {
        x: area.x + nav_width,
        width: area.width - nav_width,
        ..area
    };
    render_nav(app, nav, buf);
    render_page(app, body, buf);
}

pub(super) fn tone_color(theme: &Theme, tone: Tone) -> Color {
    match tone {
        Tone::Normal => theme.fg,
        Tone::Info => theme.info,
        Tone::Muted => theme.muted_fg,
        Tone::Good => theme.success,
        Tone::Warn => theme.warning,
        Tone::Bad => theme.danger,
        Tone::Accent => theme.primary,
    }
}

/// Fits the navigation column to its longest entry, so labels are not cut on wide terminals.
/// 使导航列适配最长条目，宽终端上标签不再被截断。
/// `total` is the area width; returns the column width including its border.
/// `total` 为区域宽度；返回含边框的列宽。
fn nav_width(app: &App, total: u16) -> u16 {
    // Marker, padding, badge gap and border around each label.
    // 每个标签周围的标记、留白、徽标间隔与边框。
    const CHROME: usize = 6;
    let widest = pages::navigation(app)
        .iter()
        .flat_map(|(group, entries)| {
            let group = group.as_deref().map_or(0, width_of);
            entries
                .iter()
                .map(move |entry| {
                    width_of(&entry.label)
                        + entry
                            .badge
                            .as_ref()
                            .map_or(0, |(text, _)| width_of(text) + 1)
                })
                .chain(std::iter::once(group))
        })
        .max()
        .unwrap_or(0);
    let wanted = (widest + CHROME).max(width_of(&app.area.label()) + CHROME) as u16;
    wanted.max(NAV_MIN_WIDTH).min(total / 3)
}

fn render_nav(app: &mut App, area: Rect, buf: &mut Buffer) {
    let theme = app.theme;
    let focused = app.focus == Focus::AreaNav;
    let block = Block::new()
        .borders(Borders::RIGHT)
        .border_style(Style::new().fg(if focused {
            theme.primary
        } else {
            theme.sidebar_border
        }))
        .style(Style::new().bg(theme.sidebar).fg(theme.sidebar_fg));
    let inner = block.inner(area);
    block.render(area, buf);
    app.hits.add(inner, Target::Focus(Focus::AreaNav));
    app.hits.add_scroll(inner, ScrollTarget::Nav);
    if inner.width < 8 || inner.height < 3 {
        return;
    }
    let width = inner.width.saturating_sub(2) as usize;
    let mut lines: Vec<(Line<'static>, Option<usize>)> = Vec::new();
    lines.push((
        Line::from(Span::styled(
            truncate(&app.area.label(), width),
            Style::new().fg(theme.primary).add_modifier(Modifier::BOLD),
        )),
        None,
    ));
    for line in wrap(
        &[Span::styled(app.area.description(), theme.muted_text())],
        width,
        &[],
        &[],
    )
    .into_iter()
    .take(3)
    {
        lines.push((line, None));
    }
    lines.push((Line::default(), None));
    let current = app.navigation_index();
    let mut index = 0;
    for (group, entries) in pages::navigation(app) {
        if let Some(group) = group {
            lines.push((
                Line::from(Span::styled(
                    truncate(&group, width),
                    theme.muted_text().add_modifier(Modifier::BOLD),
                )),
                None,
            ));
        }
        for entry in entries {
            let selected = entry.selected;
            let marker = if selected && focused { "▌" } else { " " };
            let style = if selected {
                Style::new().fg(theme.fg).add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(theme.sidebar_fg)
            };
            // Counts and unsaved-draft markers sit right-aligned after the label.
            // 计数与未保存草稿标记右对齐显示在标签之后。
            let badge = entry.badge.map(|(text, tone)| (format!(" {text}"), tone));
            let badge_width = badge.as_ref().map_or(0, |(text, _)| width_of(text));
            let label = truncate(&entry.label, width.saturating_sub(2 + badge_width));
            let pad = width.saturating_sub(1 + width_of(&label) + badge_width);
            let mut spans = vec![
                Span::styled(marker, Style::new().fg(theme.primary)),
                Span::styled(format!(" {label}"), style),
            ];
            if let Some((text, tone)) = badge {
                let color = match tone {
                    Tone::Normal => theme.muted_fg,
                    tone => tone_color(&theme, tone),
                };
                spans.push(Span::raw(" ".repeat(pad)));
                spans.push(Span::styled(text, Style::new().fg(color)));
            }
            let mut line = Line::from(spans);
            if selected {
                line = line.style(Style::new().bg(theme.sidebar_accent));
            }
            lines.push((line, Some(index)));
            index += 1;
        }
    }
    // Keep the selected page visible.
    // 保持选中页面可见。
    let height = inner.height as usize;
    let selected_line = lines
        .iter()
        .position(|(_, i)| *i == Some(current))
        .unwrap_or(0);
    let offset = (selected_line + 1).saturating_sub(height);
    for (row, (line, target)) in lines.iter().skip(offset).take(height).enumerate() {
        let rect = Rect {
            x: inner.x + 1,
            y: inner.y + row as u16,
            width: inner.width.saturating_sub(1),
            height: 1,
        };
        line.render(rect, buf);
        if let Some(index) = target {
            app.hits.add(
                Rect {
                    x: inner.x,
                    width: inner.width,
                    ..rect
                },
                Target::NavItem(*index),
            );
        }
    }
}

/// One laid-out row: its lines and clickable chips `(line, x offset, width, target)`.
/// 一个已排版的行：其各行文本与可点击按钮 `(行, x 偏移, 宽度, 目标)`。
struct Laid {
    lines: Vec<Line<'static>>,
    chips: Vec<(usize, u16, u16, Target)>,
}

fn render_page(app: &mut App, area: Rect, buf: &mut Buffer) {
    let theme = app.theme;
    app.hits.add(area, Target::Focus(Focus::AreaPage));
    let Some(page) = app.current_page() else {
        return;
    };
    let content_width = area.width.saturating_sub(4);
    let content = Rect {
        x: area.x + 2,
        y: area.y,
        width: content_width,
        height: area.height,
    };
    if content.width < 16 || content.height < 4 {
        return;
    }

    // Title with the page's search button.
    // 标题及页面搜索按钮。
    let title = page.title(app);
    buf.set_string(
        content.x,
        content.y,
        truncate(&title, content.width as usize),
        theme.strong().add_modifier(Modifier::BOLD),
    );
    if let Some(search) = page.search(app) {
        let label = format!("⌕ {}  /", tr!("tui.page.search"));
        let x = content.right().saturating_sub(width_of(&label) as u16 + 2);
        button(
            app,
            buf,
            x,
            content.y,
            &label,
            button_style(app, ButtonKind::Normal, theme.bg),
            search,
        );
    }
    buf.set_string(
        content.x,
        content.y + 1,
        "─".repeat(content.width as usize),
        Style::new().fg(theme.border),
    );
    let list = Rect {
        y: content.y + 2,
        height: content.height - 2,
        ..content
    };
    app.hits.page_height = list.height;
    app.hits.add_scroll(list, ScrollTarget::Page);

    let rows = pages::rows(app, page);
    let mut state = app.page_state(page);
    let focused = app.focus == Focus::AreaPage;
    // Keep the cursor on a selectable row.
    // 保持光标位于可选行。
    if !rows.get(state.cursor).is_some_and(Row::selectable)
        && let Some(first) = rows.iter().position(Row::selectable)
    {
        state.cursor = first;
    }
    let label_width = label_width(&rows, list.width as usize);
    let laid: Vec<Laid> = rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            lay_out(
                &theme,
                row,
                i,
                &state,
                list.width as usize,
                label_width,
                i == 0,
            )
        })
        .collect();
    let tops: Vec<usize> = laid
        .iter()
        .scan(0, |top, l| {
            let this = *top;
            *top += l.lines.len();
            Some(this)
        })
        .collect();
    let total: usize = laid.iter().map(|l| l.lines.len()).sum();
    let height = list.height as usize;
    if !state.free_scroll
        && let (Some(top), Some(block)) = (tops.get(state.cursor), laid.get(state.cursor))
    {
        let bottom = top + block.lines.len();
        if *top < state.scroll {
            state.scroll = *top;
        } else if bottom > state.scroll + height {
            state.scroll = bottom.saturating_sub(height).min(*top);
        }
    }
    state.scroll = state.scroll.min(total.saturating_sub(height));
    let scroll = state.scroll;
    *app.page_state_mut(page) = state.clone();

    for (index, block) in laid.iter().enumerate() {
        let top = tops[index];
        let bottom = top + block.lines.len();
        if bottom <= scroll || top >= scroll + height {
            continue;
        }
        let selected = index == state.cursor && rows[index].selectable();
        let visible_top = top.max(scroll);
        let visible_bottom = bottom.min(scroll + height);
        let rect = Rect {
            x: list.x,
            y: list.y + (visible_top - scroll) as u16,
            width: list.width,
            height: (visible_bottom - visible_top) as u16,
        };
        if selected {
            let bg = if focused {
                theme.selection_bg()
            } else {
                mix(theme.selection_bg(), theme.bg, 0.5)
            };
            buf.set_style(
                Rect {
                    x: rect.x.saturating_sub(1),
                    width: rect.width + 1,
                    ..rect
                },
                Style::new().bg(bg),
            );
            if focused {
                for y in rect.top()..rect.bottom() {
                    buf.set_string(
                        rect.x.saturating_sub(2),
                        y,
                        "▌",
                        Style::new().fg(theme.primary),
                    );
                }
            }
        }
        if rows[index].selectable() {
            app.hits.add(rect, Target::PageRow(index));
        }
        for (line_index, line) in block.lines.iter().enumerate() {
            let at = top + line_index;
            if at < scroll || at >= scroll + height {
                continue;
            }
            let y = list.y + (at - scroll) as u16;
            line.render(
                Rect {
                    x: list.x,
                    y,
                    width: list.width,
                    height: 1,
                },
                buf,
            );
        }
        for (line_index, x, w, target) in &block.chips {
            let at = top + line_index;
            if at < scroll || at >= scroll + height {
                continue;
            }
            let y = list.y + (at - scroll) as u16;
            app.hits.add(
                Rect {
                    x: list.x + x,
                    y,
                    width: *w,
                    height: 1,
                },
                target.clone(),
            );
        }
    }

    // Scroll indicator.
    // 滚动指示。
    if total > height && height > 0 {
        let thumb = (height * height / total).max(1);
        let top = (height - thumb) * scroll / total.saturating_sub(height).max(1);
        let x = area.right().saturating_sub(1);
        for i in 0..thumb {
            if let Some(cell) = buf.cell_mut((x, list.y + (top + i) as u16)) {
                cell.set_char('▐')
                    .set_fg(mix(theme.muted_fg, theme.bg, 0.5));
            }
        }
    }
}

/// Label on the left, padded to `label_width`, then `right` spans.
/// 左侧为补齐到 `label_width` 的标签，其后为 `right` 片段。
fn labeled(
    theme: &Theme,
    label: &str,
    label_width: usize,
    right: Vec<Span<'static>>,
) -> Line<'static> {
    let label = truncate(label, label_width.saturating_sub(1));
    let pad = label_width.saturating_sub(width_of(&label));
    let mut spans = vec![
        Span::styled(label, Style::new().fg(theme.fg)),
        Span::raw(" ".repeat(pad)),
    ];
    spans.extend(right);
    Line::from(spans)
}

/// Like [`labeled`], but joins label and value with a faint dot leader for read-only facts.
/// 与 [`labeled`] 相同，但以浅色点引导线连接只读信息的标签与值。
fn leadered(
    theme: &Theme,
    label: &str,
    label_width: usize,
    right: Vec<Span<'static>>,
) -> Line<'static> {
    let label = truncate(label, label_width.saturating_sub(3));
    let used = width_of(&label);
    let mut spans = vec![Span::styled(label, Style::new().fg(theme.fg))];
    let dots = label_width.saturating_sub(used + 2);
    if dots > 0 {
        spans.push(Span::styled(
            format!(" {} ", "·".repeat(dots)),
            Style::new().fg(mix(theme.border, theme.bg, 0.2)),
        ));
    } else {
        spans.push(Span::raw(" ".repeat(label_width.saturating_sub(used))));
    }
    spans.extend(right);
    Line::from(spans)
}

/// A horizontal gauge of `width` cells filled to `ratio`.
/// 宽度为 `width` 个单元、按 `ratio` 填充的水平进度条。
fn gauge(theme: &Theme, ratio: f64, width: usize, color: Color) -> Vec<Span<'static>> {
    let filled = ((width as f64) * ratio.clamp(0.0, 1.0)).round() as usize;
    vec![
        Span::styled("━".repeat(filled), Style::new().fg(color)),
        Span::styled(
            "━".repeat(width.saturating_sub(filled)),
            Style::new().fg(mix(theme.border, theme.bg, 0.5)),
        ),
    ]
}

/// `text` padded with spaces to exactly `width` display cells, truncated to leave a one-cell gap.
/// 将 `text` 截断（保留一格间隔）后以空格补齐到恰好 `width` 个显示单元。
fn padded(text: &str, width: usize) -> String {
    let text = truncate(text, width.saturating_sub(1));
    let pad = width.saturating_sub(width_of(&text));
    format!("{text}{}", " ".repeat(pad))
}

/// Aligns every labelled value of a page in one column sized to its longest label, leaving at
/// least three fifths of the width to the values.
/// 将页面所有带标签的值对齐到同一列，列宽取最长标签，并至少为值保留五分之三的宽度。
/// `rows` are the page rows and `width` the content width; returns the label column width.
/// `rows` 为页面行，`width` 为内容宽度；返回标签列宽度。
fn label_width(rows: &[Row], width: usize) -> usize {
    let widest = rows
        .iter()
        .filter(|row| {
            matches!(
                row.kind,
                RowKind::Info { .. }
                    | RowKind::Toggle { .. }
                    | RowKind::Choice { .. }
                    | RowKind::Number { .. }
                    | RowKind::Text { .. }
                    | RowKind::Meter { .. }
                    | RowKind::Spark { .. }
            )
        })
        .map(|row| width_of(&row.label) + 3)
        .max()
        .unwrap_or(0);
    widest.max(LABEL_MIN_WIDTH).min(width * 2 / 5)
}

fn lay_out(
    theme: &Theme,
    row: &Row,
    index: usize,
    state: &pages::PageState,
    width: usize,
    label_width: usize,
    first: bool,
) -> Laid {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut chips = Vec::new();
    let value_width = width.saturating_sub(label_width + 1);
    let muted = theme.muted_text();
    let detail_lines = |lines: &mut Vec<Line<'static>>| {
        if !row.detail.is_empty() {
            lines.extend(wrap(
                &[Span::styled(
                    row.detail.clone(),
                    Style::new().fg(theme.faint_fg(theme.bg)),
                )],
                width,
                &[],
                &[],
            ));
        }
    };
    let chip_style = Style::new().fg(theme.primary).add_modifier(Modifier::BOLD);
    match &row.kind {
        RowKind::Section => {
            if !first {
                lines.push(Line::default());
            }
            let title = truncate(&row.label, width.saturating_sub(6));
            let rest = width.saturating_sub(width_of(&title) + 4);
            let rule = Style::new().fg(mix(theme.border, theme.bg, 0.1));
            lines.push(Line::from(vec![
                Span::styled("── ", rule),
                Span::styled(
                    title,
                    Style::new().fg(theme.primary).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(" {}", "─".repeat(rest)), rule),
            ]));
        }
        RowKind::Info { value, tone } => {
            lines.push(leadered(
                theme,
                &row.label,
                label_width,
                vec![Span::styled(
                    truncate(value, value_width),
                    Style::new().fg(tone_color(theme, *tone)),
                )],
            ));
            detail_lines(&mut lines);
        }
        RowKind::Note { tone } => {
            let style = if *tone == Tone::Normal {
                theme.base()
            } else {
                Style::new().fg(tone_color(theme, *tone))
            };
            lines.extend(wrap(
                &[Span::styled(row.label.clone(), style)],
                width,
                &[],
                &[],
            ));
        }
        RowKind::Document { content } => {
            // Document whitespace carries code/list structure and must survive the generic row renderer.
            // 文档空白承载代码及列表结构，必须通过通用行渲染器保留。
            lines.extend(super::text::wrap_verbatim(content, width, theme.base()));
        }
        RowKind::Toggle { on, .. } => {
            let switch = theme.switch(*on, (&tr!("tui.page.on"), &tr!("tui.page.off")));
            lines.push(labeled(theme, &row.label, label_width, switch));
            detail_lines(&mut lines);
        }
        RowKind::Choice {
            options, current, ..
        } => {
            let shown = options
                .iter()
                .find(|(id, _)| id == current)
                .map_or(current.clone(), |(_, label)| label.clone());
            let shown = truncate(&shown, value_width.saturating_sub(8));
            let w = width_of(&shown) as u16;
            lines.push(labeled(
                theme,
                &row.label,
                label_width,
                vec![
                    Span::styled(" ‹ ", chip_style),
                    Span::styled(shown, Style::new().fg(theme.fg)),
                    Span::styled(" › ", chip_style),
                    Span::styled(" ▾", muted),
                ],
            ));
            let x = label_width as u16;
            chips.push((0, x, 3, Target::PageRowAction(index, 0)));
            chips.push((0, x + 3 + w, 3, Target::PageRowAction(index, 1)));
            detail_lines(&mut lines);
        }
        RowKind::Number {
            value, min, max, ..
        } => {
            let shown = value.to_string();
            let w = width_of(&shown) as u16;
            let dim = |active: bool| if active { chip_style } else { muted };
            lines.push(labeled(
                theme,
                &row.label,
                label_width,
                vec![
                    Span::styled(" ‹ ", dim(value > min)),
                    Span::styled(
                        shown,
                        Style::new().fg(theme.fg).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(" › ", dim(value < max)),
                    Span::styled(
                        format!(" {min}–{max}"),
                        Style::new().fg(theme.faint_fg(theme.bg)),
                    ),
                ],
            ));
            let x = label_width as u16;
            chips.push((0, x, 3, Target::PageRowAction(index, 0)));
            chips.push((0, x + 3 + w, 3, Target::PageRowAction(index, 1)));
            detail_lines(&mut lines);
        }
        RowKind::Text { value, secret, .. } => {
            let shown = if value.is_empty() {
                Span::styled(
                    tr!("tui.page.notSet"),
                    Style::new().fg(theme.faint_fg(theme.bg)),
                )
            } else if *secret {
                Span::styled(
                    format!("⚿ {}", truncate(value, value_width.saturating_sub(5))),
                    muted,
                )
            } else {
                Span::styled(
                    truncate(value, value_width.saturating_sub(3)),
                    Style::new().fg(theme.fg),
                )
            };
            lines.push(labeled(
                theme,
                &row.label,
                label_width,
                vec![Span::raw(" "), shown, Span::styled(" ✎", chip_style)],
            ));
            detail_lines(&mut lines);
        }
        RowKind::Button { tone, .. } => {
            let style = match tone {
                Tone::Accent => Style::new()
                    .fg(theme.primary_fg)
                    .bg(theme.primary)
                    .add_modifier(Modifier::BOLD),
                Tone::Bad => Style::new()
                    .fg(theme.primary_fg)
                    .bg(theme.danger)
                    .add_modifier(Modifier::BOLD),
                _ => Style::new().fg(theme.fg).bg(theme.control_bg()),
            };
            lines.push(Line::from(Span::styled(
                format!(" {} ", truncate(&row.label, width.saturating_sub(2))),
                style,
            )));
            detail_lines(&mut lines);
        }
        RowKind::Item {
            id,
            badge,
            actions,
            lines: extra,
            job,
        } => {
            let expanded = state.expanded.contains(id);
            let fold = if extra.is_empty() {
                "  "
            } else if expanded {
                "▾ "
            } else {
                "▸ "
            };
            let badge_text = badge
                .as_ref()
                .map(|(text, _)| format!(" {text} "))
                .unwrap_or_default();
            let title_width = width.saturating_sub(width_of(&badge_text) + 3);
            let title = truncate(&row.label, title_width);
            let pad = width.saturating_sub(width_of(&title) + width_of(&badge_text) + 2);
            let mut spans = vec![
                Span::styled(fold, muted),
                Span::styled(title, theme.strong().add_modifier(Modifier::BOLD)),
                Span::raw(" ".repeat(pad)),
            ];
            if let Some((text, tone)) = badge {
                spans.push(Span::styled(
                    format!(" {text} "),
                    theme.badge(tone_color(theme, *tone)),
                ));
            }
            lines.push(Line::from(spans));
            if !row.detail.is_empty() {
                let indent = [Span::raw("  ".to_string())];
                lines.extend(wrap(
                    &[Span::styled(row.detail.clone(), muted)],
                    width,
                    &indent,
                    &indent,
                ));
            }
            if let Some(job) = job {
                let color = if job.failed {
                    theme.danger
                } else if job.done {
                    theme.success
                } else {
                    theme.primary
                };
                let bar_width = width.saturating_sub(12).min(40);
                let mut spans = vec![Span::raw("  ")];
                match job.progress {
                    Some(progress) => {
                        let filled = bar_width * progress as usize / 100;
                        spans.push(Span::styled("━".repeat(filled), Style::new().fg(color)));
                        spans.push(Span::styled(
                            "━".repeat(bar_width - filled),
                            Style::new().fg(mix(theme.border, theme.bg, 0.5)),
                        ));
                        spans.push(Span::styled(
                            format!(" {progress}%"),
                            Style::new().fg(color),
                        ));
                    }
                    None => spans.push(Span::styled(
                        format!("{} ", spinner(0.0)),
                        Style::new().fg(color),
                    )),
                }
                spans.push(Span::styled(
                    format!(
                        "  {}",
                        truncate(&job.label, width.saturating_sub(bar_width + 10))
                    ),
                    muted,
                ));
                lines.push(Line::from(spans));
                if !job.detail.is_empty() {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", truncate(&job.detail, width.saturating_sub(2))),
                        muted,
                    )));
                }
            }
            if expanded {
                for text in extra {
                    let indent = [Span::styled(
                        "  │ ".to_string(),
                        Style::new().fg(theme.border),
                    )];
                    lines.extend(wrap(
                        &[Span::styled(text.clone(), theme.base())],
                        width,
                        &indent,
                        &indent,
                    ));
                }
            }
            if !actions.is_empty() {
                // Chips stay on one line; those without room fold into an overflow menu. The
                // keyboard-chosen chip stays visible so Left / Right never move onto a hidden one.
                // 按钮保持单行；放不下的收进溢出菜单。键盘选中的按钮始终可见，左右键不会移到隐藏按钮。
                let line_index = lines.len();
                let chosen = state.action.min(actions.len() - 1);
                let texts: Vec<String> = actions
                    .iter()
                    .map(|action| format!(" {} ", truncate(&action.label, width.saturating_sub(6))))
                    .collect();
                let widths: Vec<usize> = texts.iter().map(|text| width_of(text)).collect();
                let shown = fit_strip(
                    &widths,
                    1,
                    width.saturating_sub(2),
                    width_of(OVERFLOW_CHIP),
                    (state.cursor == index).then_some(chosen),
                );
                let mut spans = vec![Span::raw("  ")];
                let mut x = 2u16;
                let mut folded = Vec::new();
                for (chip, (action, text)) in actions.iter().zip(texts).enumerate() {
                    if !shown.contains(&chip) {
                        folded.push(MenuItem {
                            label: action.label.clone(),
                            hint: String::new(),
                            action: action.action.clone(),
                            danger: action.tone == Tone::Bad,
                        });
                        continue;
                    }
                    let w = width_of(&text) as u16;
                    let color = match action.tone {
                        Tone::Normal | Tone::Muted => theme.fg,
                        tone => tone_color(theme, tone),
                    };
                    let active = chip == chosen && state.cursor == index;
                    let style = if active {
                        Style::new()
                            .fg(theme.primary_fg)
                            .bg(if color == theme.fg {
                                theme.primary
                            } else {
                                color
                            })
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::new().fg(color).bg(theme.control_bg())
                    };
                    spans.push(Span::styled(text, style));
                    spans.push(Span::raw(" "));
                    chips.push((line_index, x, w, Target::PageRowAction(index, chip)));
                    x += w + 1;
                }
                if !folded.is_empty() {
                    spans.push(Span::styled(
                        OVERFLOW_CHIP,
                        Style::new().fg(theme.fg).bg(theme.control_bg()),
                    ));
                    chips.push((
                        line_index,
                        x,
                        width_of(OVERFLOW_CHIP) as u16,
                        Target::Do(UiAction::OpenMenu(Box::new(overflow_menu(folded)))),
                    ));
                }
                lines.push(Line::from(spans));
            }
            lines.push(Line::default());
        }
        RowKind::Stats(tiles) => {
            // Tiles share the width evenly; each shows its value above its label.
            // 卡片平分宽度；每张卡片上方为数值、下方为标签。
            let per_line = tiles.len().clamp(1, (width / 16).max(1));
            let cell = (width / per_line).saturating_sub(1);
            for chunk in tiles.chunks(per_line) {
                let mut values = Vec::new();
                let mut labels = Vec::new();
                for (label, value, tone) in chunk {
                    let color = tone_color(theme, *tone);
                    let bar = Style::new().fg(if *tone == Tone::Normal {
                        theme.border
                    } else {
                        color
                    });
                    values.push(Span::styled("▎", bar));
                    values.push(Span::styled(
                        padded(value, cell),
                        Style::new().fg(color).add_modifier(Modifier::BOLD),
                    ));
                    labels.push(Span::styled("▎", bar));
                    labels.push(Span::styled(padded(label, cell), muted));
                }
                lines.push(Line::from(values));
                lines.push(Line::from(labels));
            }
            lines.push(Line::default());
        }
        RowKind::Meter { ratio, text, tone } => {
            let color = tone_color(theme, *tone);
            let text = truncate(text, value_width / 2);
            let bar = value_width.saturating_sub(width_of(&text) + 2).clamp(4, 40);
            let mut right = gauge(theme, *ratio, bar, color);
            right.push(Span::styled(format!("  {text}"), Style::new().fg(color)));
            lines.push(leadered(theme, &row.label, label_width, right));
            detail_lines(&mut lines);
        }
        RowKind::Spark { points, text } => {
            // The latest points that fit, each scaled between zero and the shown maximum.
            // 能放下的最近数据点，各自按零到所显示最大值缩放。
            const LEVELS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
            let text = truncate(text, value_width / 2);
            let room = value_width.saturating_sub(width_of(&text) + 2).max(1);
            let shown = &points[points.len().saturating_sub(room)..];
            let max = shown.iter().copied().fold(0.0, f64::max);
            let bars: String = shown
                .iter()
                .map(|point| {
                    let level = if max > 0.0 {
                        (point.max(0.0) / max * (LEVELS.len() - 1) as f64).round() as usize
                    } else {
                        0
                    };
                    LEVELS[level.min(LEVELS.len() - 1)]
                })
                .collect();
            let right = vec![
                Span::styled(bars, Style::new().fg(theme.primary)),
                Span::styled(format!("  {text}"), theme.base()),
            ];
            lines.push(leadered(theme, &row.label, label_width, right));
            detail_lines(&mut lines);
        }
        RowKind::Brand => {
            lines.push(Line::default());
            lines.push(brand::wordmark(theme).centered());
            lines.push(Line::default());
        }
        RowKind::Spacer => lines.push(Line::default()),
    }
    Laid { lines, chips }
}
