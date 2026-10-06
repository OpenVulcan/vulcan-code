// Activity bar: the vertical area switcher on the left edge, mirroring the WebUI activity rail.
// 活动栏：左侧边缘的纵向区域切换栏，对应 WebUI 的活动栏。
//
// Top to bottom: the workbench, a rule, the feature areas (Knowledge sits under Plugins, as the
// WebUI shows plugin pages beneath their parent), and Settings pinned to the bottom. Wide terminals
// show labels beside the icons; narrower ones only the icons; the narrowest hide the bar and
// switch areas through the header button or F2.
// 自上而下：工作台、分隔线、功能区域（知识库位于插件之下，与 WebUI 将插件页面置于其父项之下一致），
// 设置固定在底部。宽终端在图标旁显示名称；较窄终端只显示图标；最窄时隐藏活动栏，经顶栏按钮或 F2
// 切换区域。

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Widget};

use super::text::{spinner, truncate, width_of};
use crate::app::App;
use crate::app::hit::Target;
use crate::app::pages::Area;

/// Width of the icon-only bar, its right border included.
/// 仅图标形式活动栏的宽度，包括右边框。
pub const ICON_WIDTH: u16 = 4;

/// Feature areas between the workbench and Settings, in `Area::ALL` order.
/// 工作台与设置之间的功能区域，按 `Area::ALL` 顺序排列。
fn features() -> impl Iterator<Item = Area> {
    Area::ALL
        .into_iter()
        .filter(|area| !matches!(area, Area::Chat | Area::Settings))
}

/// Icon of one area; single-column glyphs so both bar forms keep their width.
/// 区域图标；使用单列字形，使两种形式的活动栏宽度保持不变。
fn icon(area: Area) -> &'static str {
    match area {
        Area::Chat => "▤",
        Area::Plugins => "◫",
        Area::Knowledge => "▦",
        Area::AgentSkills => "✦",
        Area::Agents => "◉",
        Area::Memory => "◍",
        Area::Settings => "⚙",
    }
}

/// Columns a child entry is indented by in the labeled form.
/// 带名称形式中子项的缩进列数。
const CHILD_INDENT: usize = 2;

/// Indent of an entry in the labeled form: nested entries move right under their parent.
/// 带名称形式中条目的缩进：嵌套条目在其父项之下右移。
fn indent(area: Area) -> usize {
    if area.parent().is_some() {
        CHILD_INDENT
    } else {
        0
    }
}

/// Width of the labeled bar: the widest entry plus its icon, padding and right border.
/// 带名称活动栏的宽度：最宽条目加图标、留白与右边框。
fn label_width() -> u16 {
    let widest = Area::ALL
        .into_iter()
        .map(|area| width_of(&area.label()) + indent(area))
        .max()
        .unwrap_or(0);
    // Marker, icon and a space before the label; a space and the border after it.
    // 名称前为标记、图标与一个空格；名称后为一个空格与边框。
    (widest + 5) as u16
}

/// Bar width for a body `width`: labeled when the full workbench (sidebar, conversation and panel
/// at their own widths) still fits beside it, icons while `center_min` fits, else hidden.
/// 主体宽度为 `width` 时的活动栏宽度：完整工作台（侧栏、对话与面板各按自身宽度）仍能并排放下时显示
/// 名称；能放下 `center_min` 时只显示图标；否则隐藏。
/// `workbench` is the width the expanded workbench needs; returns zero when the bar is hidden.
/// `workbench` 为展开工作台所需的宽度；隐藏时返回零。
pub fn width(width: u16, workbench: u16, center_min: u16) -> u16 {
    let labeled = label_width();
    if width >= labeled + workbench {
        labeled
    } else if width >= ICON_WIDTH + center_min {
        ICON_WIDTH
    } else {
        0
    }
}

pub fn render(app: &mut App, area: Rect, buf: &mut Buffer) {
    let theme = app.theme;
    let block = Block::new()
        .borders(Borders::RIGHT)
        .border_style(Style::new().fg(theme.sidebar_border))
        .style(Style::new().bg(theme.sidebar).fg(theme.sidebar_fg));
    let inner = block.inner(area);
    block.render(area, buf);
    app.hits.add(inner, Target::Inert);
    if inner.width < 2 || inner.height < 3 {
        return;
    }
    let labeled = area.width > ICON_WIDTH;
    let entry = |app: &App, area_id: Area, y: u16, buf: &mut Buffer| {
        let rect = Rect {
            y,
            height: 1,
            ..inner
        };
        let active = app.area == area_id;
        let bg = if active {
            theme.sidebar_accent
        } else {
            theme.sidebar
        };
        let fg = if active { theme.fg } else { theme.sidebar_fg };
        let mut spans = vec![Span::styled(
            if active { "▌" } else { " " },
            Style::new().fg(theme.primary),
        )];
        if labeled {
            spans.push(Span::raw(" ".repeat(indent(area_id))));
        }
        spans.push(Span::styled(
            icon(area_id),
            Style::new().fg(if active { theme.primary } else { fg }),
        ));
        if labeled {
            let used = width_of(icon(area_id)) + 2 + indent(area_id);
            let room = (inner.width as usize).saturating_sub(used);
            let style = Style::new().fg(fg);
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                truncate(&area_id.label(), room),
                if active {
                    style.add_modifier(Modifier::BOLD)
                } else {
                    style
                },
            ));
        }
        Line::from(spans)
            .style(Style::new().bg(bg))
            .render(rect, buf);
        // The workbench shows live work in its last column while another area is open.
        // 打开其他区域时，工作台在其最后一列显示正在进行的工作。
        if area_id == Area::Chat && !active {
            let running = app.conversations.iter().any(|c| c.run_phase.is_active());
            let attention = app.conversations.iter().any(|c| c.attention);
            let marker = if running {
                Some((spinner(app.phase()), theme.primary))
            } else if attention {
                Some(("!", theme.warning))
            } else {
                None
            };
            if let Some((glyph, color)) = marker {
                buf.set_string(
                    rect.right().saturating_sub(1),
                    rect.y,
                    glyph,
                    Style::new().fg(color).bg(bg).add_modifier(Modifier::BOLD),
                );
            }
        }
        app.hits.add(rect, Target::AreaTab(area_id));
    };

    entry(app, Area::Chat, inner.y, buf);
    buf.set_string(
        inner.x,
        inner.y + 1,
        "─".repeat(inner.width as usize),
        Style::new().fg(theme.sidebar_border),
    );
    // Settings keeps the last row; feature entries fill the rows between.
    // 设置占据最后一行；功能条目填充其间的行。
    let settings_y = inner.bottom() - 1;
    for (y, area_id) in (inner.y + 2..settings_y).zip(features()) {
        entry(app, area_id, y, buf);
    }
    entry(app, Area::Settings, settings_y, buf);
}
