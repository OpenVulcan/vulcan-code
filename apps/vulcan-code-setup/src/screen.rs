//! The terminal the interactive installer draws on, kept from the wizard to the last page, and the
//! parts every page shares. Text comes from the `setup` catalog surface; errors stay English.
//! 交互式安装界面所绘制的终端（从向导一直保持到最后一页），以及各页共用的部分。文案来自 `setup` 目录分面，
//! 错误保持英文。

use ratatui::layout::{Constraint, Layout, Margin};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};
use vulcan_i18n::{SETUP, Translator, format_template};
use vulcan_install_core::names::{ComponentName, Platform};

use crate::integrate::Location;

/// Accent colour, the primary colour of `vulcan-code-tui`'s themes; only foreground colours are
/// set, so the terminal's own background shows through in light and dark terminals alike.
/// 强调色，即 `vulcan-code-tui` 主题的主色；只设置前景色，因此无论浅色还是深色终端都透出终端自身的背景。
pub const ACCENT: Color = Color::Rgb(0xf8, 0x73, 0x31);

/// Secondary text colour.
/// 次要文字颜色。
pub const MUTED: Color = Color::Rgb(0x9a, 0xa0, 0xa8);

/// Error and warning text colour.
/// 错误与警告文字颜色。
pub const DANGER: Color = Color::Rgb(0xf8, 0x51, 0x49);

/// The terminal in the interactive installer's mode; dropping it gives the terminal back.
/// 处于交互式安装界面模式的终端；丢弃时归还终端。
pub struct Screen {
    /// The terminal.
    /// 终端。
    terminal: DefaultTerminal,
    /// Text of the system language.
    /// 系统语言的文案。
    translator: Translator,
}

impl Screen {
    /// Takes the terminal over.
    /// 接管终端。
    pub fn open() -> Result<Self, String> {
        let mut translator = Translator::new(&[&SETUP]);
        translator.set_locale(vulcan_i18n::system_locale());
        let terminal = ratatui::try_init()
            .map_err(|error| format!("cannot start the interactive installer: {error}"))?;
        Ok(Self {
            terminal,
            translator,
        })
    }

    /// Draws one frame with `render`.
    /// 以 `render` 绘制一帧。
    pub fn draw(&mut self, render: impl FnOnce(&mut Frame<'_>, &Translator)) -> Result<(), String> {
        let Self {
            terminal,
            translator,
        } = self;
        terminal
            .draw(|frame| render(frame, translator))
            .map(|_| ())
            .map_err(|error| format!("cannot draw the interactive installer: {error}"))
    }

    /// Gives the terminal back while `work` runs, so a program such as `sudo` can use it, then
    /// takes it over again.
    /// 在 `work` 运行期间归还终端，使 `sudo` 等程序可以使用它，随后再次接管。
    pub fn suspended<T>(&mut self, work: impl FnOnce(&Translator) -> T) -> Result<T, String> {
        let _ = ratatui::try_restore();
        let result = work(&self.translator);
        self.terminal = ratatui::try_init()
            .map_err(|error| format!("cannot return to the interactive installer: {error}"))?;
        Ok(result)
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        let _ = ratatui::try_restore();
    }
}

/// Text of `key`, or the key itself when the catalog lacks it (a complete surface never does).
/// `key` 的文案；目录缺少该键时返回键本身（完整分面不会缺少）。
pub fn text<'t>(translator: &'t Translator, key: &'t str) -> &'t str {
    translator.text(key).unwrap_or(key)
}

/// Text of `key` with its placeholders filled.
/// 填好占位符的 `key` 文案。
pub fn filled(translator: &Translator, key: &str, values: &[(&str, &str)]) -> String {
    format_template(text(translator, key), values)
}

/// One page: the frame with the installer's title, a muted line and a bold title, the body and the
/// key hint; the body starts `scroll` lines down.
/// 一页：带安装器标题的边框、一行次要文字与一行粗体标题、正文以及按键提示；正文从第 `scroll` 行开始。
pub fn page(
    frame: &mut Frame<'_>,
    translator: &Translator,
    overline: &str,
    title: &str,
    body: Vec<Line<'_>>,
    hint: &str,
    scroll: u16,
) {
    let area = frame.area();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(MUTED))
        .title(Span::styled(
            format!(" {} ", text(translator, "setup.title")),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [header, content, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .areas(inner.inner(Margin::new(2, 1)));
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(overline.to_owned(), Style::default().fg(MUTED)),
            Line::styled(
                title.to_owned(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ])
        .wrap(Wrap { trim: false }),
        header,
    );
    frame.render_widget(
        Paragraph::new(body)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        content,
    );
    frame.render_widget(
        Paragraph::new(Line::styled(hint.to_owned(), Style::default().fg(MUTED)))
            .wrap(Wrap { trim: false }),
        footer,
    );
}

/// Label and value rows whose values line up.
/// 值对齐的标签与值各行。
pub fn fields<'a>(rows: Vec<(&'a str, String)>) -> Vec<Line<'a>> {
    let width = rows
        .iter()
        .map(|(label, _)| Span::raw(*label).width())
        .max()
        .unwrap_or(0);
    rows.into_iter()
        .map(|(label, value)| {
            let padding = width.saturating_sub(Span::raw(label).width()) + 2;
            Line::from(vec![
                Span::styled(
                    format!("{label}{}", " ".repeat(padding)),
                    Style::default().fg(MUTED),
                ),
                Span::raw(value),
            ])
        })
        .collect()
}

/// Display names of `components`, in catalog text where the catalog has one.
/// `components` 的显示名称，目录中有对应文案时使用目录文案。
pub fn component_names(translator: &Translator, components: &[ComponentName]) -> String {
    components
        .iter()
        .map(|component| {
            let key = match component.as_str() {
                "core" => "setup.component.core",
                "tui" => "setup.component.tui",
                "gui" => "setup.component.gui",
                "browser" => "setup.components.browser",
                other => return other.to_owned(),
            };
            text(translator, key).to_owned()
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// One line per location outside the root, or the line that says there is none.
/// 根目录之外每个位置一行；没有时给出说明行。
pub fn location_lines<'a>(
    translator: &'a Translator,
    platform: Platform,
    locations: &[Location],
) -> Vec<Line<'a>> {
    if locations.is_empty() {
        return vec![Line::raw(format!(
            "  {}",
            text(translator, "setup.summary.writesNone")
        ))];
    }
    let service = if platform.is_windows() {
        "setup.writes.serviceLoginRun"
    } else if platform.is_macos() {
        "setup.writes.serviceLaunchAgent"
    } else {
        "setup.writes.userService"
    };
    locations
        .iter()
        .map(|location| {
            let (key, path) = match location {
                Location::Commands(path) => ("setup.writes.commands", Some(path)),
                Location::MenuEntry(path) => ("setup.writes.menuEntry", Some(path)),
                Location::DesktopShortcut(path) => ("setup.writes.desktopShortcut", Some(path)),
                Location::AppBundle(path) => ("setup.writes.appBundle", Some(path)),
                Location::UserPath(path) => ("setup.writes.userPath", Some(path)),
                Location::UninstallEntry => ("setup.writes.uninstallEntry", None),
                Location::UserService => (service, None),
                Location::Linger => ("setup.writes.linger", None),
                Location::AppArmorProfile(path) => ("setup.writes.appArmorProfile", Some(path)),
            };
            let path = path
                .map(|path| path.display().to_string())
                .unwrap_or_default();
            Line::raw(format!("  {}", filled(translator, key, &[("path", &path)])))
        })
        .collect()
}
