//! Text editor compatibility exports and future override seam for Vulcan widgets.
//! Vulcan 自定义组件使用的文本编辑器兼容导出与后续重写接缝。

use iced::advanced::text;

pub use iced::widget::text_editor::{
    Action, Binding, Catalog, Content, Cursor, Edit, KeyPress, Line, LineEnding, Motion, Position,
    Selection, Status, Style,
};

/// Compatibility text editor type alias that preserves the iced widget shape.
/// 保持 iced 组件形态的兼容文本编辑器类型别名。
pub type TextEditor<'a, Highlighter, Message, Theme = iced::Theme, Renderer = iced::Renderer> =
    iced::widget::text_editor::TextEditor<'a, Highlighter, Message, Theme, Renderer>;

/// Builds a Vulcan-compatible text editor using the iced builder format.
/// 使用 iced 构建器格式创建兼容 Vulcan 的文本编辑器。
pub fn text_editor<'a, Message, Theme, Renderer>(
    content: &'a Content<Renderer>,
) -> TextEditor<'a, text::highlighter::PlainText, Message, Theme, Renderer>
where
    Message: Clone,
    Theme: Catalog + 'a,
    Renderer: text::Renderer,
{
    iced::widget::text_editor(content)
}

#[cfg(test)]
mod tests {
    use super::{Content, TextEditor, text_editor};
    use iced::advanced::text;

    /// Compatibility builder smoke test for the default iced text editor shape.
    /// 默认 iced 文本编辑器形态的兼容构建冒烟测试。
    #[test]
    fn text_editor_builder_matches_iced_shape() {
        let content = Content::<iced::Renderer>::with_text("compat");
        let _editor: TextEditor<'_, text::highlighter::PlainText, (), iced::Theme, iced::Renderer> =
            text_editor(&content);
    }
}
