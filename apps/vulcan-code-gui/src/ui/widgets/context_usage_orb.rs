//! Reusable circular context-usage orb widget used by composer-like surfaces.
//! 供输入区等表面复用的圆形上下文占用指示盘部件。

use crate::ui::style;
use iced::mouse;
use iced::widget::{Canvas, canvas, column, container, stack, text, tooltip};
use iced::{Color, Element, Length, Pixels, Point, Radians, Rectangle, Renderer, Theme};
use std::f32::consts::PI;

/// Reusable visual tone describing how one context-usage orb should look.
/// 描述单个上下文占用指示盘外观的可复用视觉主题。
#[derive(Debug, Clone, Copy)]
pub struct ContextUsageOrbTone {
    /// Background fill color used by the orb shell.
    /// 指示盘外壳使用的背景填充色。
    pub background: Color,
    /// Border color used by the orb shell.
    /// 指示盘外壳使用的边框颜色。
    pub border: Color,
    /// Track color used by the circular progress ring.
    /// 环形进度轨道使用的颜色。
    pub track: Color,
    /// Progress color used by the circular progress ring.
    /// 环形进度进展段使用的颜色。
    pub progress: Color,
    /// Primary text color shown inside the orb.
    /// 指示盘内部主文本使用的颜色。
    pub text_primary: Color,
}

/// Immutable owned props consumed by the reusable context-usage orb widget.
/// 可复用上下文占用指示盘部件消费的不可变自有属性集合。
#[derive(Debug, Clone)]
pub struct ContextUsageOrbProps {
    /// Percentage displayed and rendered by the orb.
    /// 指示盘展示并渲染的百分比。
    pub percent: f32,
    /// Tooltip title shown above detailed diagnostic rows.
    /// 详细诊断行上方展示的提示标题。
    pub tooltip_title: String,
    /// Tooltip diagnostic rows shown on hover.
    /// 悬停时展示的提示诊断行集合。
    pub tooltip_rows: Vec<String>,
    /// Reusable visual tone used by the orb shell and ring.
    /// 供指示盘外壳与环形进度复用的视觉主题。
    pub tone: ContextUsageOrbTone,
    /// Fixed diameter of the circular orb surface.
    /// 圆形指示盘表面的固定直径。
    pub diameter: f32,
}

/// Builds one reusable circular context-usage orb with hover diagnostics.
/// 构建一个带悬停诊断的可复用环形上下文占用指示盘。
pub fn view_context_usage_orb<Message: 'static>(
    props: ContextUsageOrbProps,
) -> Element<'static, Message> {
    let diameter = props.diameter;
    let percent_font_size = ((diameter * 0.24).round() as u32).max(11);
    let percent_line_height = ((diameter * 0.26).round()).max(12.0);
    let percent_text = format!("{:.0}%", props.percent.clamp(0.0, 100.0));
    let ring = Canvas::new(ContextUsageOrbRingProgram {
        ratio: (props.percent / 100.0).clamp(0.0, 1.0),
        track_color: props.tone.track,
        progress_color: props.tone.progress,
    })
    .width(Length::Fixed(diameter))
    .height(Length::Fixed(diameter));
    let labels = container(
        text(percent_text)
            .size(percent_font_size)
            .color(props.tone.text_primary)
            .line_height(iced::widget::text::LineHeight::Absolute(Pixels(
                percent_line_height,
            ))),
    )
    .width(Length::Fixed(diameter))
    .height(Length::Fixed(diameter))
    .center_x(Length::Fixed(diameter))
    .center_y(Length::Fixed(diameter));
    let orb_surface = container(stack![ring, labels])
        .width(Length::Fixed(diameter))
        .height(Length::Fixed(diameter))
        .style(move |theme| {
            style::workbench_context_usage_orb_style(
                theme,
                props.tone.background,
                props.tone.border,
            )
        });

    tooltip(
        orb_surface,
        build_context_usage_orb_tooltip(props.tooltip_title, props.tooltip_rows),
        tooltip::Position::Top,
    )
    .style(style::workbench_context_usage_tooltip_wrapper_style)
    .into()
}

/// Builds the hover tooltip body paired with one context-usage orb.
/// 构建与单个上下文占用指示盘配对的悬停提示内容。
fn build_context_usage_orb_tooltip<Message: 'static>(
    title: String,
    rows: Vec<String>,
) -> Element<'static, Message> {
    let tooltip_body = rows.into_iter().fold(
        column![
            text(title).size(13),
            container(iced::widget::Space::new().width(Length::Fill).height(1))
                .width(Length::Fill)
                .height(1)
                .style(style::main_display_bottom_separator_style),
        ]
        .spacing(6),
        |column, row| column.push(text(row).size(12)),
    );

    container(tooltip_body)
        .padding([10, 12])
        .max_width(420)
        .style(style::workbench_context_usage_tooltip_style)
        .into()
}

/// Stateless canvas program drawing the circular progress ring used by the orb.
/// 绘制指示盘环形进度圈的无状态画布程序。
#[derive(Debug, Clone, Copy)]
struct ContextUsageOrbRingProgram {
    /// Normalized ring progress ratio in the inclusive 0 to 1 range.
    /// 处于 0 到 1 闭区间内的规范化环形进度比例。
    ratio: f32,
    /// Track color painted for the inactive ring background.
    /// 用于绘制未激活环形轨道背景的颜色。
    track_color: Color,
    /// Progress color painted for the active ring segment.
    /// 用于绘制激活环形进展段的颜色。
    progress_color: Color,
}

impl<Message> canvas::Program<Message> for ContextUsageOrbRingProgram {
    /// No mutable canvas state is needed for the static ring.
    /// 静态环形绘制不需要可变画布状态。
    type State = ();

    /// Draws the track and progress arc geometry for the context-usage ring.
    /// 绘制上下文占用环的轨道与进展弧形几何。
    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let center = frame.center();
        let radius = (bounds.width.min(bounds.height) * 0.5) - 6.0;
        let track = canvas::Path::circle(center, radius.max(0.0));
        let stroke = canvas::Stroke::default()
            .with_width(6.0)
            .with_line_cap(canvas::LineCap::Round);
        frame.stroke(&track, stroke.with_color(self.track_color));

        if self.ratio > 0.0 {
            let start_angle = Radians(-PI * 0.5);
            let sweep_angle = Radians((-PI * 0.5) + (PI * 2.0 * self.ratio));
            let progress_arc = canvas::Path::new(|builder| {
                builder.arc(canvas::path::Arc {
                    center: Point::new(center.x, center.y),
                    radius,
                    start_angle,
                    end_angle: sweep_angle,
                });
            });
            frame.stroke(&progress_arc, stroke.with_color(self.progress_color));
        }

        vec![frame.into_geometry()]
    }
}
