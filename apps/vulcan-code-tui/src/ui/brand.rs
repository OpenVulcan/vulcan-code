// Text wordmark shared by the splash screen, header, empty conversation and About page.
// 启动画面、标题栏、空会话与关于页面共用的文字字标。

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::theme::Theme;

/// `VulcanCode`: "Vulcan" in the strong text color, "Code" in bold primary.
/// `VulcanCode`：“Vulcan”使用强调文字色，“Code”使用加粗主色。
pub fn wordmark(theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled("Vulcan", theme.strong()),
        Span::styled(
            "Code",
            Style::new().fg(theme.primary).add_modifier(Modifier::BOLD),
        ),
    ])
}
