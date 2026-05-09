//! Iced container styles owned by the agent-view presentation layer.
//! 由 agent-view 展示层持有的 Iced 容器样式。

use iced::widget::container;
use iced::{Background, Border, Color, Shadow, Theme, Vector};

/// Returns the translucent backdrop style used by the centered thinking modal.
/// 返回居中思考弹层使用的半透明背景遮罩样式。
pub fn thinking_modal_backdrop_style(_theme: &Theme, progress: f32) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color {
            r: 0.05,
            g: 0.06,
            b: 0.09,
            a: 0.18 + progress.clamp(0.0, 1.0) * 0.56,
        })),
        ..Default::default()
    }
}

/// Returns the centered modal card style used by the explicit thinking-detail surface.
/// 返回显式思考详情表面使用的居中弹层卡片样式。
pub fn thinking_modal_card_style(theme: &Theme, progress: f32) -> container::Style {
    let palette = theme.extended_palette();
    let visible_ratio = progress.clamp(0.0, 1.0);

    container::Style {
        background: Some(
            iced::color!(0x121824)
                .scale_alpha(0.90 + visible_ratio * 0.08)
                .into(),
        ),
        border: Border {
            radius: 6.0.into(),
            width: 1.0,
            color: palette
                .secondary
                .strong
                .color
                .scale_alpha(0.20 + visible_ratio * 0.12),
        },
        shadow: Shadow {
            color: Color::BLACK.scale_alpha(0.08 + visible_ratio * 0.16),
            offset: Vector::new(0.0, 10.0 + visible_ratio * 10.0),
            blur_radius: 18.0 + visible_ratio * 16.0,
        },
        ..Default::default()
    }
}
