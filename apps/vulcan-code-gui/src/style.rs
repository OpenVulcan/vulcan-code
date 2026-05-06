//! Shared visual styles for the GUI workbench.
//! GUI 工作台共享的视觉样式定义。

use crate::diff::DiffLineKind;
use iced::widget::container;
use iced::{Color, Theme};

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
