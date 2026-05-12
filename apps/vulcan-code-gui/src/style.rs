//! Shared visual styles for the GUI workbench.
//! GUI 工作台共享的视觉样式定义。

use crate::diff::DiffLineKind;
use crate::model::SessionRunPhase;
use crate::theme::ThemePaletteExt;
use iced::overlay::menu;
use iced::widget::{button, container, pick_list, text_editor};
use iced::{Background, Border, Color, Shadow, Theme, Vector, gradient};

/// Returns the gutter background style for a semantic diff line kind.
/// 返回某个语义 diff 行类型对应的 gutter 背景样式。
pub fn diff_gutter_background_style(_theme: &Theme, kind: DiffLineKind) -> container::Style {
    match kind {
        DiffLineKind::Added => container::Style {
            background: Some(iced::color!(0xF0FFF4).into()),
            ..Default::default()
        },
        DiffLineKind::Removed => container::Style {
            background: Some(iced::color!(0xFFF1F0).into()),
            ..Default::default()
        },
        DiffLineKind::Note => container::Style {
            background: Some(iced::color!(0xE3F2FF).into()),
            ..Default::default()
        },
        DiffLineKind::Context => container::Style {
            background: Some(iced::color!(0xFFFFFF).into()),
            ..Default::default()
        },
    }
}

/// Returns the left accent-bar style for a semantic diff line kind.
/// 返回某个语义 diff 行类型对应的左侧强调竖条样式。
pub fn diff_line_bar_style(_theme: &Theme, kind: DiffLineKind) -> container::Style {
    container::Style {
        background: Some(diff_bar_color(kind).into()),
        ..Default::default()
    }
}

/// Returns the accent color used by the left diff bar.
/// 返回左侧 diff 竖条使用的强调颜色。
pub fn diff_bar_color(kind: DiffLineKind) -> Color {
    match kind {
        DiffLineKind::Added => iced::color!(0x2DA44E),
        DiffLineKind::Removed => iced::color!(0xD73A49),
        DiffLineKind::Note => iced::color!(0x0969DA),
        DiffLineKind::Context => iced::color!(0xD8DEE4),
    }
}

/// Returns the gutter line-number color used by one semantic diff line kind.
/// 返回某个语义 diff 行类型使用的 gutter 行号颜色。
pub fn diff_line_number_color(kind: DiffLineKind) -> Color {
    match kind {
        DiffLineKind::Added | DiffLineKind::Removed => diff_bar_color(kind),
        DiffLineKind::Note => iced::color!(0x0969DA),
        DiffLineKind::Context => iced::color!(0x8C959F),
    }
}

/// Returns the separator style between gutter line numbers and code content.
/// 返回 gutter 行号与代码内容之间的分隔线样式。
pub fn diff_gutter_separator_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(iced::color!(0xD0D7DE).into()),
        ..Default::default()
    }
}

/// Returns the square-corner container style used by diff hunk and gap blocks.
/// 返回 diff hunk 与省略块使用的直角容器样式。
pub fn diff_block_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();

    container::Style {
        background: Some(palette.background.base.color.into()),
        border: Border {
            radius: 0.0.into(),
            width: 1.0,
            color: palette.background.strong.color,
        },
        ..Default::default()
    }
}

/// Returns the borderless text editor style used inside diff code blocks.
/// 返回 diff 代码区内部使用的无边框文本编辑器样式。
pub fn diff_code_editor_style(theme: &Theme, _status: text_editor::Status) -> text_editor::Style {
    let palette = theme.extended_palette();

    text_editor::Style {
        background: Background::Color(Color::TRANSPARENT),
        border: Border {
            radius: 0.0.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        placeholder: palette.secondary.base.color,
        value: palette.background.base.text,
        selection: palette.primary.weak.color,
    }
}

/// Returns the shared top chrome style used by custom undecorated window bars.
/// 返回自定义无系统装饰窗口顶部 chrome 区域使用的共享样式。
pub fn window_chrome_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();

    container::Style {
        background: Some(palette.background.base.color.into()),
        shadow: Shadow::default(),
        ..Default::default()
    }
}

/// Returns the background style used by the fixed left activity bar.
/// 返回固定左侧活动条使用的背景样式。
pub fn activity_bar_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();

    container::Style {
        background: Some(palette.background.base.color.into()),
        border: Border::default(),
        ..Default::default()
    }
}

/// Returns the top separator style used by the framed main display region.
/// 返回主显示区域顶部独立分隔线使用的样式。
pub fn main_display_top_separator_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();

    container::Style {
        background: Some(palette.background.strong.color.scale_alpha(0.22).into()),
        border: Border {
            radius: iced::border::Radius {
                top_left: 2.0,
                top_right: 0.0,
                bottom_right: 0.0,
                bottom_left: 0.0,
            },
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        ..Default::default()
    }
}

/// Returns the left separator style used by the framed main display region.
/// 返回主显示区域左侧独立分隔线使用的样式。
pub fn main_display_left_separator_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();

    container::Style {
        background: Some(palette.background.strong.color.scale_alpha(0.22).into()),
        border: Border {
            radius: iced::border::Radius {
                top_left: 2.0,
                top_right: 0.0,
                bottom_right: 0.0,
                bottom_left: 2.0,
            },
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        ..Default::default()
    }
}

/// Returns the bottom separator style used by the framed main display region.
/// 返回主显示区域底部独立分隔线使用的样式。
pub fn main_display_bottom_separator_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();

    container::Style {
        background: Some(palette.background.strong.color.scale_alpha(0.22).into()),
        border: Border {
            radius: iced::border::Radius {
                top_left: 0.0,
                top_right: 0.0,
                bottom_right: 0.0,
                bottom_left: 2.0,
            },
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        ..Default::default()
    }
}

/// Returns the translucent full-screen backdrop style used by the startup loading overlay.
/// 返回启动加载遮罩使用的半透明全屏背景样式。
pub fn startup_overlay_backdrop_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color {
            r: 0.06,
            g: 0.07,
            b: 0.10,
            a: 0.78,
        })),
        ..Default::default()
    }
}

/// Returns the compact card style used by the startup loading overlay content.
/// 返回启动加载遮罩内容卡片使用的紧凑样式。
pub fn startup_overlay_card_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();

    container::Style {
        background: Some(palette.background.base.color.into()),
        border: Border {
            radius: 6.0.into(),
            width: 1.0,
            color: palette.background.strong.color.scale_alpha(0.18),
        },
        shadow: Shadow {
            color: Color {
                a: 0.20,
                ..Color::BLACK
            },
            offset: Vector::new(0.0, 10.0),
            blur_radius: 24.0,
        },
        ..Default::default()
    }
}

/// Returns the icon-button style used by one item inside the fixed left activity bar.
/// 返回固定左侧活动条中某个图标按钮使用的样式。
pub fn activity_bar_button_style(
    theme: &Theme,
    status: button::Status,
    is_active: bool,
) -> button::Style {
    let palette = theme.extended_palette();
    let active_style = button::Style {
        background: Some(Background::Color(palette.primary.weak.color)),
        text_color: activity_bar_icon_color(theme, true),
        border: Border {
            radius: 2.0.into(),
            width: 1.0,
            color: palette.primary.strong.color.scale_alpha(0.20),
        },
        shadow: Shadow::default(),
        snap: true,
    };
    let inactive_style = button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: activity_bar_icon_color(theme, false),
        border: Border {
            radius: 2.0.into(),
            width: 1.0,
            color: Color::TRANSPARENT,
        },
        shadow: Shadow::default(),
        snap: true,
    };

    if is_active {
        return match status {
            button::Status::Active => active_style,
            button::Status::Hovered => button::Style {
                background: Some(Background::Color(
                    palette.primary.base.color.scale_alpha(0.22),
                )),
                text_color: palette.primary.strong.text,
                ..active_style
            },
            button::Status::Pressed => button::Style {
                background: Some(Background::Color(
                    palette.primary.base.color.scale_alpha(0.30),
                )),
                text_color: palette.primary.strong.text,
                ..active_style
            },
            button::Status::Disabled => button::Style {
                text_color: palette.background.base.text.scale_alpha(0.40),
                ..active_style
            },
        };
    }

    match status {
        button::Status::Active => inactive_style,
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(
                palette.background.strong.color.scale_alpha(0.10),
            )),
            text_color: palette.background.base.text,
            ..inactive_style
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(
                palette.background.strong.color.scale_alpha(0.18),
            )),
            text_color: palette.background.base.text,
            ..inactive_style
        },
        button::Status::Disabled => button::Style {
            text_color: palette.background.base.text.scale_alpha(0.34),
            ..inactive_style
        },
    }
}

/// Returns the foreground color used by one activity-bar icon across active and inactive states.
/// 返回活动条图标在激活态与非激活态之间共用的前景色。
pub fn activity_bar_icon_color(theme: &Theme, is_active: bool) -> Color {
    let palette = theme.extended_palette();

    if is_active {
        palette.primary.strong.text
    } else {
        palette.background.base.text.scale_alpha(0.94)
    }
}

/// Returns the subtle icon-button style used by fold and restore controls inside the main task drawer.
/// 返回主任务抽屉内部折叠与恢复控件使用的轻量图标按钮样式。
pub fn subtle_panel_button_style(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    let base = button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: palette.background.base.text.scale_alpha(0.72),
        border: Border {
            radius: 9.0.into(),
            width: 1.0,
            color: Color::TRANSPARENT,
        },
        shadow: Shadow::default(),
        snap: true,
    };

    match status {
        button::Status::Active => base,
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(
                palette.background.strong.color.scale_alpha(0.10),
            )),
            text_color: palette.background.base.text,
            ..base
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(
                palette.background.strong.color.scale_alpha(0.18),
            )),
            text_color: palette.background.base.text,
            ..base
        },
        button::Status::Disabled => button::Style {
            text_color: palette.background.base.text.scale_alpha(0.30),
            ..base
        },
    }
}

/// Returns the floating circular button style used by the transcript jump-to-bottom affordance.
/// 返回转录区回到底部浮动圆形按钮使用的样式。
pub fn transcript_jump_to_bottom_button_style(
    theme: &Theme,
    status: button::Status,
) -> button::Style {
    let palette = theme.extended_palette();
    let base = button::Style {
        background: Some(Background::Color(
            palette.background.base.color.scale_alpha(0.96),
        )),
        text_color: palette.background.base.text.scale_alpha(0.90),
        border: Border {
            radius: 999.0.into(),
            width: 1.0,
            color: palette.background.strong.color.scale_alpha(0.20),
        },
        shadow: Shadow {
            color: Color {
                a: 0.18,
                ..Color::BLACK
            },
            offset: Vector::new(0.0, 7.0),
            blur_radius: 18.0,
        },
        snap: true,
    };

    match status {
        button::Status::Active => base,
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(
                palette.background.weak.color.scale_alpha(0.98),
            )),
            text_color: palette.primary.strong.color,
            border: Border {
                color: palette.primary.weak.color.scale_alpha(0.42),
                ..base.border
            },
            ..base
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(
                palette.background.strong.color.scale_alpha(0.94),
            )),
            text_color: palette.primary.strong.color,
            border: Border {
                color: palette.primary.base.color.scale_alpha(0.56),
                ..base.border
            },
            shadow: Shadow {
                offset: Vector::new(0.0, 4.0),
                blur_radius: 12.0,
                ..base.shadow
            },
            ..base
        },
        button::Status::Disabled => button::Style {
            text_color: palette.background.base.text.scale_alpha(0.28),
            shadow: Shadow::default(),
            ..base
        },
    }
}

/// Returns the bottom fade overlay style used by the rendered transcript view so lower content softly dissolves into the panel background.
/// 返回渲染态转录视图底部淡隐遮罩使用的样式，让下半区内容柔和地融入面板背景。
pub fn rendered_transcript_bottom_fade_overlay_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    let gradient = gradient::Linear::new(iced::Degrees(180.0))
        .add_stop(
            0.0,
            Color {
                a: 0.0,
                ..palette.background.base.color
            },
        )
        .add_stop(
            1.0,
            Color {
                a: 0.98,
                ..palette.background.base.color
            },
        );

    container::Style {
        background: Some(gradient.into()),
        border: Border::default(),
        ..Default::default()
    }
}

/// Returns the circular trailing-button style used by the workbench composer send or stop affordance.
/// 返回工作台提示词发送或停止触发按钮使用的圆形尾部按钮样式。
pub fn workbench_composer_trigger_button_style(
    theme: &Theme,
    status: button::Status,
    is_running: bool,
) -> button::Style {
    let palette = theme.extended_palette();
    let accent_color = if is_running {
        palette.danger.strong.color
    } else {
        palette.primary.strong.color
    };
    let accent_border = if is_running {
        palette.danger.weak.color.scale_alpha(0.40)
    } else {
        palette.primary.weak.color.scale_alpha(0.42)
    };
    let pressed_background = if is_running {
        palette.danger.base.color.scale_alpha(0.14)
    } else {
        palette.primary.base.color.scale_alpha(0.14)
    };
    let base = button::Style {
        background: Some(Background::Color(
            palette.background.base.color.scale_alpha(0.98),
        )),
        text_color: accent_color,
        border: Border {
            radius: 999.0.into(),
            width: 1.0,
            color: accent_border,
        },
        shadow: Shadow {
            color: Color {
                a: 0.14,
                ..Color::BLACK
            },
            offset: Vector::new(0.0, 6.0),
            blur_radius: 16.0,
        },
        snap: true,
    };

    match status {
        button::Status::Active => base,
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(
                palette.background.weak.color.scale_alpha(0.98),
            )),
            border: Border {
                color: accent_color.scale_alpha(0.55),
                ..base.border
            },
            shadow: Shadow {
                offset: Vector::new(0.0, 7.0),
                blur_radius: 18.0,
                ..base.shadow
            },
            ..base
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(pressed_background)),
            border: Border {
                color: accent_color.scale_alpha(0.62),
                ..base.border
            },
            shadow: Shadow {
                offset: Vector::new(0.0, 4.0),
                blur_radius: 12.0,
                ..base.shadow
            },
            ..base
        },
        button::Status::Disabled => button::Style {
            text_color: palette.background.base.text.scale_alpha(0.28),
            border: Border {
                color: palette.background.strong.color.scale_alpha(0.18),
                ..base.border
            },
            shadow: Shadow::default(),
            ..base
        },
    }
}

/// Returns the compact bordered trigger style used by the inline workbench model selector surface.
/// 返回工作台内联模型选择触发表面使用的紧凑描边样式。
pub fn workbench_model_trigger_button_style(
    theme: &Theme,
    status: button::Status,
) -> button::Style {
    let palette = theme.extended_palette();
    let base = button::Style {
        background: Some(Background::Color(
            palette.background.base.color.scale_alpha(0.98),
        )),
        text_color: palette.background.base.text,
        border: Border {
            radius: 10.0.into(),
            width: 1.0,
            color: palette.background.strong.color.scale_alpha(0.24),
        },
        shadow: Shadow::default(),
        snap: true,
    };

    match status {
        button::Status::Active => base,
        button::Status::Hovered => button::Style {
            border: Border {
                color: palette.primary.base.color.scale_alpha(0.58),
                ..base.border
            },
            background: Some(Background::Color(
                palette.background.weak.color.scale_alpha(0.98),
            )),
            ..base
        },
        button::Status::Pressed => button::Style {
            border: Border {
                color: palette.primary.strong.color.scale_alpha(0.68),
                ..base.border
            },
            background: Some(Background::Color(
                palette.background.strong.color.scale_alpha(0.94),
            )),
            ..base
        },
        button::Status::Disabled => button::Style {
            text_color: palette.background.base.text.scale_alpha(0.34),
            ..base
        },
    }
}

/// Returns the compact pick-list field style used by the workbench composer control row.
/// 返回工作台提示词控制行使用的紧凑下拉框字段样式。
pub fn workbench_compact_pick_list_style(
    theme: &Theme,
    status: pick_list::Status,
) -> pick_list::Style {
    let palette = theme.extended_palette();
    let base = pick_list::Style {
        text_color: palette.background.base.text,
        placeholder_color: palette.background.base.text.scale_alpha(0.56),
        handle_color: palette.background.base.text.scale_alpha(0.56),
        background: Background::Color(palette.background.base.color.scale_alpha(0.98)),
        border: Border {
            radius: 10.0.into(),
            width: 1.0,
            color: palette.background.strong.color.scale_alpha(0.24),
        },
    };

    match status {
        pick_list::Status::Active => base,
        pick_list::Status::Hovered => pick_list::Style {
            background: Background::Color(palette.background.weak.color.scale_alpha(0.98)),
            border: Border {
                color: palette.primary.base.color.scale_alpha(0.58),
                ..base.border
            },
            ..base
        },
        pick_list::Status::Opened { .. } => pick_list::Style {
            background: Background::Color(palette.background.weak.color.scale_alpha(0.98)),
            border: Border {
                color: palette.primary.strong.color.scale_alpha(0.68),
                ..base.border
            },
            ..base
        },
        pick_list::Status::Disabled => pick_list::Style {
            text_color: palette.background.base.text.scale_alpha(0.34),
            placeholder_color: palette.background.base.text.scale_alpha(0.34),
            handle_color: palette.background.base.text.scale_alpha(0.34),
            ..base
        },
    }
}

/// Returns the dropdown-menu style paired with the compact workbench composer pick lists.
/// 返回与紧凑工作台提示词下拉框配套的下拉菜单样式。
pub fn workbench_compact_pick_list_menu_style(theme: &Theme) -> menu::Style {
    let palette = theme.extended_palette();

    menu::Style {
        background: Background::Color(palette.background.base.color.scale_alpha(0.98)),
        border: Border {
            radius: 10.0.into(),
            width: 1.0,
            color: palette.background.strong.color.scale_alpha(0.24),
        },
        text_color: palette.background.base.text,
        selected_text_color: palette.primary.strong.text,
        selected_background: Background::Color(palette.primary.base.color.scale_alpha(0.22)),
        shadow: Shadow {
            color: Color {
                a: 0.10,
                ..Color::BLACK
            },
            offset: Vector::new(0.0, 6.0),
            blur_radius: 16.0,
        },
    }
}

/// Returns the rounded panel style used by the centered workbench composer surface.
/// 返回居中工作台提示词面板使用的圆角样式。
pub fn workbench_composer_panel_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();

    container::Style {
        background: Some(Background::Color(
            palette.background.weak.color.scale_alpha(0.96),
        )),
        border: Border {
            radius: 20.0.into(),
            width: 1.0,
            color: palette.background.strong.color.scale_alpha(0.30),
        },
        shadow: Shadow {
            color: Color {
                a: 0.25,
                ..Color::BLACK
            },
            offset: Vector::new(8.0, 10.0),
            blur_radius: 12.0,
        },
        ..Default::default()
    }
}

/// Returns the rounded editable text-editor style used inside the floating workbench composer panel.
/// 返回浮动工作台提示词面板内部使用的圆角可编辑文本框样式。
pub fn workbench_composer_editor_style(
    theme: &Theme,
    _status: text_editor::Status,
) -> text_editor::Style {
    let palette = theme.extended_palette();

    text_editor::Style {
        background: Background::Color(palette.background.base.color.scale_alpha(0.82)),
        border: Border {
            radius: 20.0.into(),
            width: 1.0,
            color: palette.background.strong.color.scale_alpha(0.20),
        },
        placeholder: palette.background.base.text.scale_alpha(0.42),
        value: palette.background.base.text,
        selection: palette.primary.weak.color.scale_alpha(0.88),
    }
}

/// Returns the scrollbar-thumb style rendered inside the workbench composer editor surface.
/// 返回渲染在工作台提示词编辑器表面内部的滚动条滑块样式。
pub fn workbench_composer_editor_scrollbar_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();

    container::Style {
        background: Some(Background::Color(
            palette.background.strong.color.scale_alpha(0.42),
        )),
        border: Border {
            radius: 999.0.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        ..Default::default()
    }
}

/// Returns the list-row style used by each option inside the in-app workbench model-picker modal.
/// 返回工作台内嵌模型选择弹窗中每个选项行使用的列表样式。
pub fn workbench_model_picker_option_button_style(
    theme: &Theme,
    status: button::Status,
    is_selected: bool,
) -> button::Style {
    let palette = theme.extended_palette();
    let selected_background = palette.primary.base.color.scale_alpha(0.16);
    let selected_border = palette.primary.base.color.scale_alpha(0.48);
    let base = button::Style {
        background: Some(Background::Color(if is_selected {
            selected_background
        } else {
            Color::TRANSPARENT
        })),
        text_color: palette.background.base.text,
        border: Border {
            radius: 10.0.into(),
            width: 1.0,
            color: if is_selected {
                selected_border
            } else {
                Color::TRANSPARENT
            },
        },
        shadow: Shadow::default(),
        snap: true,
    };

    match status {
        button::Status::Active => base,
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(if is_selected {
                selected_background
            } else {
                palette.background.strong.color.scale_alpha(0.12)
            })),
            border: Border {
                color: if is_selected {
                    selected_border
                } else {
                    palette.background.strong.color.scale_alpha(0.28)
                },
                ..base.border
            },
            ..base
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(if is_selected {
                palette.primary.base.color.scale_alpha(0.22)
            } else {
                palette.background.strong.color.scale_alpha(0.18)
            })),
            ..base
        },
        button::Status::Disabled => button::Style {
            text_color: palette.background.base.text.scale_alpha(0.34),
            ..base
        },
    }
}

/// Returns the style of the restore rail shown when the main task drawer is collapsed.
/// 返回主任务抽屉折叠后显示的恢复导轨样式。
pub fn main_task_panel_restore_rail_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();

    container::Style {
        background: Some(palette.background.base.color.into()),
        border: Border {
            radius: 0.0.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        ..Default::default()
    }
}

/// Returns the subtle divider indicator style used inside the main task drawer resize handle.
/// 返回主任务抽屉缩放分隔柄内部使用的轻量分隔指示线样式。
pub fn main_task_panel_resize_indicator_style(
    theme: &Theme,
    is_highlighted: bool,
) -> container::Style {
    let palette = theme.extended_palette();

    container::Style {
        background: Some(
            if is_highlighted {
                palette.primary.base.color.scale_alpha(0.92)
            } else {
                palette.background.strong.color.scale_alpha(0.24)
            }
            .into(),
        ),
        ..Default::default()
    }
}

/// Returns the tree-row button style used by the task drawer session outline.
/// 返回任务抽屉会话树大纲使用的树节点按钮样式。
pub fn main_task_tree_session_button_style(
    theme: &Theme,
    status: button::Status,
    is_selected: bool,
    is_hovered: bool,
) -> button::Style {
    let palette = theme.extended_palette();
    let base = button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: palette.background.base.text,
        border: Border {
            radius: 2.0.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        shadow: Shadow::default(),
        snap: true,
    };
    let selected = button::Style {
        background: Some(Background::Color(
            palette.primary.weak.color.scale_alpha(0.72),
        )),
        text_color: palette.background.base.text,
        border: Border {
            radius: 2.0.into(),
            width: 1.0,
            color: palette.primary.strong.color.scale_alpha(0.16),
        },
        shadow: Shadow::default(),
        snap: true,
    };

    let idle = if is_selected { selected } else { base };

    match status {
        button::Status::Active => {
            if is_hovered && !is_selected {
                button::Style {
                    background: Some(Background::Color(
                        palette.background.strong.color.scale_alpha(0.06),
                    )),
                    text_color: palette.background.base.text,
                    ..idle
                }
            } else {
                idle
            }
        }
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(if is_selected {
                palette.primary.base.color.scale_alpha(0.26)
            } else {
                palette.background.strong.color.scale_alpha(0.10)
            })),
            text_color: palette.background.base.text,
            ..idle
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(if is_selected {
                palette.primary.base.color.scale_alpha(0.34)
            } else {
                palette.background.strong.color.scale_alpha(0.16)
            })),
            text_color: palette.background.base.text,
            ..idle
        },
        button::Status::Disabled => button::Style {
            text_color: palette.background.base.text.scale_alpha(0.40),
            ..idle
        },
    }
}

/// Returns the full-row background style used by one project row in the task drawer tree.
/// 返回任务抽屉树中项目行使用的整行背景样式。
pub fn main_task_tree_project_row_style(
    theme: &Theme,
    is_active: bool,
    is_drag_source: bool,
) -> container::Style {
    let palette = theme.extended_palette();
    let background = if is_drag_source {
        palette.primary.weak.color.scale_alpha(0.14)
    } else if is_active {
        palette.background.strong.color.scale_alpha(0.12)
    } else {
        Color::TRANSPARENT
    };
    let border = if is_drag_source {
        Border {
            radius: 2.0.into(),
            width: 1.0,
            color: palette.primary.base.color.scale_alpha(0.34),
        }
    } else {
        Border {
            radius: 2.0.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        }
    };

    container::Style {
        background: Some(background.into()),
        border,
        ..Default::default()
    }
}

/// Returns the insertion-line style used by task-tree project drag-and-drop previews.
/// 返回任务树项目拖拽插入预览使用的指示线样式。
pub fn main_task_tree_drop_indicator_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();

    container::Style {
        background: Some(palette.primary.base.color.into()),
        ..Default::default()
    }
}

/// Returns the rounded anchor-dot style used by task-tree drop indicators.
/// 返回任务树插入指示器使用的圆点锚样式。
pub fn main_task_tree_drop_anchor_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();

    container::Style {
        background: Some(palette.primary.base.color.into()),
        border: Border {
            radius: 999.0.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        ..Default::default()
    }
}

/// Returns the floating ghost-card style used while one project is being dragged.
/// 返回拖拽项目期间浮动虚影卡片使用的样式。
pub fn main_task_tree_drag_preview_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();

    container::Style {
        background: Some(palette.background.base.color.scale_alpha(0.84).into()),
        border: Border {
            radius: 2.0.into(),
            width: 1.0,
            color: palette.primary.base.color.scale_alpha(0.72),
        },
        shadow: Shadow {
            color: Color {
                a: 0.20,
                ..Color::BLACK
            },
            offset: Vector::new(0.0, 8.0),
            blur_radius: 18.0,
        },
        ..Default::default()
    }
}

/// Returns the popup style used by the project overflow menu in the task drawer tree.
/// 返回任务抽屉树项目溢出菜单使用的弹出层样式。
pub fn main_task_tree_menu_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();

    container::Style {
        background: Some(palette.background.base.color.into()),
        border: Border {
            radius: 2.0.into(),
            width: 1.0,
            color: palette.background.strong.color.scale_alpha(0.22),
        },
        shadow: Shadow {
            color: Color {
                a: 0.12,
                ..Color::BLACK
            },
            offset: Vector::new(0.0, 4.0),
            blur_radius: 12.0,
        },
        ..Default::default()
    }
}

/// Returns the compact menu-button style used inside the project overflow menu.
/// 返回项目溢出菜单内部使用的紧凑菜单按钮样式。
pub fn main_task_tree_menu_button_style(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    let base = button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: palette.background.base.text.scale_alpha(0.90),
        border: Border {
            radius: 2.0.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        shadow: Shadow::default(),
        snap: true,
    };

    match status {
        button::Status::Active => base,
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(
                palette.background.strong.color.scale_alpha(0.12),
            )),
            text_color: palette.background.base.text,
            ..base
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(
                palette.background.strong.color.scale_alpha(0.18),
            )),
            text_color: palette.background.base.text,
            ..base
        },
        button::Status::Disabled => button::Style {
            text_color: palette.background.base.text.scale_alpha(0.36),
            ..base
        },
    }
}

/// Returns the standard button style used by custom window controls except close.
/// 返回自定义窗口控制按钮中除关闭按钮外使用的标准样式。
pub fn window_control_button_style(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    let base = button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: palette.background.base.text.scale_alpha(0.88),
        border: Border::default(),
        shadow: Shadow::default(),
        snap: true,
    };

    match status {
        button::Status::Active => base,
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(palette.background.weak.color)),
            text_color: palette.background.base.text,
            ..base
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(palette.background.strong.color)),
            text_color: palette.background.base.text,
            ..base
        },
        button::Status::Disabled => button::Style {
            text_color: palette.background.base.text.scale_alpha(0.36),
            ..base
        },
    }
}

/// Returns the close-button style used by the custom window control cluster.
/// 返回自定义窗口控制按钮组中关闭按钮使用的样式。
pub fn window_close_button_style(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    let base = button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: palette.background.base.text.scale_alpha(0.88),
        border: Border::default(),
        shadow: Shadow::default(),
        snap: true,
    };

    match status {
        button::Status::Active => base,
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(iced::color!(0xC42B1C))),
            text_color: Color::WHITE,
            ..base
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(iced::color!(0xA42015))),
            text_color: Color::WHITE,
            ..base
        },
        button::Status::Disabled => button::Style {
            text_color: palette.background.base.text.scale_alpha(0.36),
            ..base
        },
    }
}

/// Returns the status-chip style used by the workbench transcript header.
/// 返回工作台转录区头部状态标签使用的样式。
pub fn workbench_run_phase_chip_style(theme: &Theme, phase: SessionRunPhase) -> container::Style {
    let palette = theme.extended_palette();
    let (background, border) = match phase {
        SessionRunPhase::Idle => (
            palette.background.weak.color.scale_alpha(0.85),
            palette.background.strong.color.scale_alpha(0.16),
        ),
        SessionRunPhase::Preparing => (
            palette.primary.weak.color.scale_alpha(0.92),
            palette.primary.strong.color.scale_alpha(0.2),
        ),
        SessionRunPhase::Ready => (
            iced::color!(0xF0FFF4),
            iced::color!(0x2DA44E).scale_alpha(0.18),
        ),
        SessionRunPhase::Failed => (
            iced::color!(0xFFF1F0),
            iced::color!(0xD73A49).scale_alpha(0.24),
        ),
        SessionRunPhase::Stopped => (
            iced::color!(0xFFF8E6),
            iced::color!(0x9A6700).scale_alpha(0.2),
        ),
    };

    container::Style {
        background: Some(Background::Color(background)),
        border: Border {
            radius: 2.0.into(),
            width: 1.0,
            color: border,
        },
        ..Default::default()
    }
}

/// Returns the segmented mode-button style used by the transcript surface switcher.
/// 返回转录表面切换器使用的分段模式按钮样式。
pub fn workbench_transcript_mode_button_style(
    theme: &Theme,
    status: button::Status,
    is_active: bool,
) -> button::Style {
    let palette = theme.extended_palette();
    let base = button::Style {
        background: Some(Background::Color(if is_active {
            palette.background.weak.color.scale_alpha(0.92)
        } else {
            Color::TRANSPARENT
        })),
        text_color: palette.background.base.text.scale_alpha(0.88),
        border: Border {
            radius: 6.0.into(),
            width: 1.0,
            color: if is_active {
                palette.primary.strong.color.scale_alpha(0.34)
            } else {
                palette.background.strong.color.scale_alpha(0.22)
            },
        },
        shadow: Shadow::default(),
        snap: true,
    };

    match status {
        button::Status::Active => base,
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(if is_active {
                palette.background.weak.color
            } else {
                palette.background.weak.color.scale_alpha(0.58)
            })),
            text_color: palette.background.base.text,
            border: Border {
                color: if is_active {
                    palette.primary.strong.color.scale_alpha(0.42)
                } else {
                    palette.background.strong.color.scale_alpha(0.30)
                },
                ..base.border
            },
            ..base
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(if is_active {
                palette.background.strong.color.scale_alpha(0.92)
            } else {
                palette.background.weak.color.scale_alpha(0.72)
            })),
            text_color: palette.background.base.text,
            border: Border {
                color: if is_active {
                    palette.primary.strong.color.scale_alpha(0.48)
                } else {
                    palette.background.strong.color.scale_alpha(0.34)
                },
                ..base.border
            },
            ..base
        },
        button::Status::Disabled => button::Style {
            text_color: palette.background.base.text.scale_alpha(0.36),
            border: Border {
                color: palette.background.strong.color.scale_alpha(0.14),
                ..base.border
            },
            ..base
        },
    }
}

/// Returns the borderless read-only text-editor style used by selectable transcript source views.
/// 返回可选取转录原文视图使用的无边框只读文本编辑器样式。
pub fn workbench_transcript_raw_view_style(
    theme: &Theme,
    _status: text_editor::Status,
) -> text_editor::Style {
    let palette = theme.extended_palette();

    text_editor::Style {
        background: Background::Color(Color::TRANSPARENT),
        border: Border {
            radius: 0.0.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        placeholder: palette.secondary.base.color,
        value: palette.background.base.text,
        selection: palette.primary.weak.color.scale_alpha(0.9),
    }
}
