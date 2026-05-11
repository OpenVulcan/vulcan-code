//! Custom widget helpers that keep parent scroll behavior intact.
//! 保持父级滚动行为完整的自定义控件辅助模块。

use iced::advanced::layout::{self, Layout};
use iced::advanced::mouse;
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{Operation, Tree};
use iced::advanced::{Shell, Widget};
use iced::{Element, Event, Length, Rectangle, Size, Vector};

/// Wraps one child widget and lets mouse-wheel events pass through to ancestors.
/// 包装单个子控件，并让鼠标滚轮事件继续向祖先控件透传。
pub struct WheelPassthrough<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    /// Wrapped content that should ignore wheel scrolling while keeping normal interaction.
    /// 在保留常规交互的同时忽略滚轮滚动的被包装内容。
    content: Element<'a, Message, Theme, Renderer>,
}

impl<'a, Message, Theme, Renderer> WheelPassthrough<'a, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    /// Creates a new wheel-passthrough wrapper around the provided content.
    /// 为给定内容创建新的滚轮透传包装层。
    pub fn new(content: impl Into<Element<'a, Message, Theme, Renderer>>) -> Self {
        Self {
            content: content.into(),
        }
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for WheelPassthrough<'_, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    /// Returns the size strategy inherited from the wrapped content.
    /// 返回继承自被包装内容的尺寸策略。
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    /// Returns the child widget tree so runtime state stays aligned.
    /// 返回子控件树，以保持运行时状态对齐。
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    /// Reconciles the wrapped child with the runtime widget tree.
    /// 将被包装子控件与运行时控件树进行协调。
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    /// Delegates layout to the wrapped content.
    /// 将布局计算委托给被包装内容。
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    /// Delegates widget operations to the wrapped content.
    /// 将控件操作委托给被包装内容。
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    /// Skips forwarding wheel events so outer scroll containers can keep scrolling.
    /// 跳过转发滚轮事件，以便外层滚动容器继续正常滚动。
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if cursor.is_over(layout.bounds())
            && matches!(event, Event::Mouse(mouse::Event::WheelScrolled { .. }))
        {
            return;
        }

        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            shell,
            viewport,
        );
    }

    /// Delegates hover interaction lookup to the wrapped content.
    /// 将悬停交互查询委托给被包装内容。
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    /// Delegates drawing to the wrapped content.
    /// 将绘制过程委托给被包装内容。
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        renderer_style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            renderer_style,
            layout,
            cursor,
            viewport,
        );
    }

    /// Delegates overlay creation to the wrapped content.
    /// 将浮层构建委托给被包装内容。
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message, Theme, Renderer> From<WheelPassthrough<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer + 'a,
    Theme: 'a,
    Message: 'a,
{
    /// Converts the wrapper into a regular iced element.
    /// 将包装层转换为常规 iced 元素。
    fn from(
        widget: WheelPassthrough<'a, Message, Theme, Renderer>,
    ) -> Element<'a, Message, Theme, Renderer> {
        Element::new(widget)
    }
}
