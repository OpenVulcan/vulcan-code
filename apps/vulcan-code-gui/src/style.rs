//! Shared visual styles for the GUI workbench.
//! GUI 工作台共享的视觉样式定义。

use crate::diff::DiffLineKind;
use iced::widget::{button, container, text_editor};
use iced::{Background, Border, Color, Shadow, Theme, Vector};

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
        text_color: palette.primary.strong.text,
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
        text_color: palette.background.base.text.scale_alpha(0.82),
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
