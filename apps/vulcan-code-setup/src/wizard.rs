//! The interactive installer: one screen per choice, then a summary of every location it writes
//! (plan section 18.6). Text comes from the `setup` catalog surface; errors stay English.
//! 交互式安装界面：每个选择一屏，最后汇总将写入的每个位置（方案 18.6 节）。文案来自 `setup` 目录分面，
//! 错误保持英文。

use std::path::PathBuf;

use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Margin};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use vulcan_i18n::{SETUP, Translator, format_template};
use vulcan_install_core::names::{Channel, Platform};

use crate::install::resolve_root;
use crate::integrate::{Location, Plan};
use crate::root;
use crate::state::Mode;

/// Accent colour, the primary colour of `vulcan-code-tui`'s themes; only foreground colours are
/// set, so the terminal's own background shows through in light and dark terminals alike.
/// 强调色，即 `vulcan-code-tui` 主题的主色；只设置前景色，因此无论浅色还是深色终端都透出终端自身的背景。
const ACCENT: Color = Color::Rgb(0xf8, 0x73, 0x31);

/// Secondary text colour.
/// 次要文字颜色。
const MUTED: Color = Color::Rgb(0x9a, 0xa0, 0xa8);

/// Error text colour.
/// 错误文字颜色。
const DANGER: Color = Color::Rgb(0xf8, 0x51, 0x49);

/// The choices being edited.
/// 正在编辑的选择。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Draft {
    /// Installation mode.
    /// 安装模式。
    pub mode: Mode,
    /// Whether the browser component is wanted.
    /// 是否需要浏览器组件。
    pub browser: bool,
    /// Installation directory as typed.
    /// 输入的安装目录。
    pub root: String,
    /// Whether a menu entry or application bundle is wanted.
    /// 是否需要菜单项或应用程序包。
    pub shortcut: bool,
    /// Whether commands are wanted on PATH.
    /// 是否需要把命令加入 PATH。
    pub path: bool,
    /// Release channel.
    /// 发布通道。
    pub channel: Channel,
}

/// The modes `platform` offers, in display order.
/// `platform` 提供的模式，按显示顺序排列。
pub fn modes(platform: Platform) -> Vec<Mode> {
    if platform.is_windows() {
        vec![Mode::Terminal]
    } else if platform.is_macos() {
        vec![Mode::Desktop, Mode::Terminal]
    } else {
        vec![Mode::Desktop, Mode::Terminal, Mode::Service]
    }
}

/// The mode preselected when none was recorded: desktop where a graphical session runs.
/// 未记录模式时预选的模式：有图形会话时为桌面模式。
pub fn default_mode(platform: Platform) -> Mode {
    let graphical = platform.is_macos()
        || std::env::var_os("DISPLAY").is_some()
        || std::env::var_os("WAYLAND_DISPLAY").is_some();
    if graphical && modes(platform).contains(&Mode::Desktop) {
        Mode::Desktop
    } else {
        Mode::Terminal
    }
}

/// One screen of the wizard.
/// 向导的一屏。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    /// Installation mode.
    /// 安装模式。
    Mode,
    /// Optional components.
    /// 可选组件。
    Components,
    /// Installation directory.
    /// 安装目录。
    Location,
    /// Shortcut and PATH.
    /// 快捷方式与 PATH。
    Integration,
    /// Release channel.
    /// 发布通道。
    Channel,
    /// Everything chosen, and every location written.
    /// 全部选择以及将写入的每个位置。
    Summary,
}

/// One toggle of the integration screen.
/// 集成屏中的一个开关。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Toggle {
    /// The menu entry or application bundle.
    /// 菜单项或应用程序包。
    Shortcut,
    /// Commands on PATH.
    /// 命令加入 PATH。
    Path,
}

/// What a key press leads to.
/// 按键导致的结果。
#[derive(Clone, Debug, PartialEq, Eq)]
enum Flow {
    /// Stay in the wizard.
    /// 留在向导中。
    Stay,
    /// Install with the draft.
    /// 按草稿安装。
    Finish,
    /// Leave without installing.
    /// 不安装直接离开。
    Cancel,
}

/// The wizard's state, independent of the terminal.
/// 向导的状态，与终端无关。
struct Wizard {
    /// The platform.
    /// 平台。
    platform: Platform,
    /// The choices.
    /// 选择。
    draft: Draft,
    /// Index into [`Wizard::steps`].
    /// [`Wizard::steps`] 中的下标。
    step: usize,
    /// Highlighted row of a list screen.
    /// 列表屏中高亮的行。
    row: usize,
    /// Cursor of the directory input, in characters.
    /// 目录输入的光标位置，以字符计。
    cursor: usize,
    /// Why the last attempt to continue was refused.
    /// 上一次继续被拒绝的原因。
    error: Option<String>,
}

impl Wizard {
    /// Starts at the first screen.
    /// 从第一屏开始。
    fn new(platform: Platform, draft: Draft) -> Self {
        let cursor = draft.root.chars().count();
        let mut wizard = Self {
            platform,
            draft,
            step: 0,
            row: 0,
            cursor,
            error: None,
        };
        wizard.enter();
        wizard
    }

    /// The screens for the current choices; the mode screen is left out when there is no choice.
    /// 当前选择下的各屏；没有可选模式时省略模式屏。
    fn steps(&self) -> Vec<Step> {
        let mut steps = Vec::new();
        if modes(self.platform).len() > 1 {
            steps.push(Step::Mode);
        }
        steps.extend([
            Step::Components,
            Step::Location,
            Step::Integration,
            Step::Channel,
            Step::Summary,
        ]);
        steps
    }

    /// The current screen.
    /// 当前屏。
    fn current(&self) -> Step {
        let steps = self.steps();
        steps[self.step.min(steps.len() - 1)]
    }

    /// The toggles of the integration screen.
    /// 集成屏的开关。
    fn toggles(&self) -> Vec<Toggle> {
        let mut toggles = Vec::new();
        if self.draft.mode == Mode::Desktop && !self.platform.is_windows() {
            toggles.push(Toggle::Shortcut);
        }
        toggles.push(Toggle::Path);
        toggles
    }

    /// Number of rows of the current list screen.
    /// 当前列表屏的行数。
    fn rows(&self) -> usize {
        match self.current() {
            Step::Mode => modes(self.platform).len(),
            Step::Components => 1,
            Step::Integration => self.toggles().len(),
            Step::Channel => 2,
            Step::Location | Step::Summary => 0,
        }
    }

    /// Highlights the current choice when a screen is entered.
    /// 进入某屏时高亮当前选择。
    fn enter(&mut self) {
        self.error = None;
        self.row = match self.current() {
            Step::Mode => modes(self.platform)
                .iter()
                .position(|mode| *mode == self.draft.mode)
                .unwrap_or(0),
            Step::Channel => usize::from(self.draft.channel == Channel::Preview),
            _ => 0,
        };
    }

    /// Moves the highlight; on a single-choice screen it also selects.
    /// 移动高亮；在单选屏中同时选中。
    fn move_row(&mut self, down: bool) {
        let rows = self.rows();
        if rows == 0 {
            return;
        }
        self.row = if down {
            (self.row + 1) % rows
        } else {
            (self.row + rows - 1) % rows
        };
        match self.current() {
            Step::Mode => self.draft.mode = modes(self.platform)[self.row],
            Step::Channel => {
                self.draft.channel = if self.row == 0 {
                    Channel::Stable
                } else {
                    Channel::Preview
                };
            }
            _ => {}
        }
    }

    /// Flips the highlighted toggle.
    /// 切换高亮的开关。
    fn toggle(&mut self) {
        match self.current() {
            Step::Components => self.draft.browser = !self.draft.browser,
            Step::Integration => match self.toggles().get(self.row) {
                Some(Toggle::Shortcut) => self.draft.shortcut = !self.draft.shortcut,
                Some(Toggle::Path) => self.draft.path = !self.draft.path,
                None => {}
            },
            _ => {}
        }
    }

    /// Continues to the next screen after checking the current one.
    /// 检查当前屏后继续到下一屏。
    fn advance(&mut self) -> Flow {
        if self.current() == Step::Location
            && let Err(error) = check_location(self.platform, &self.draft.root)
        {
            self.error = Some(error);
            return Flow::Stay;
        }
        if self.current() == Step::Summary {
            return Flow::Finish;
        }
        self.step += 1;
        self.enter();
        Flow::Stay
    }

    /// Returns to the previous screen, or leaves from the first one.
    /// 返回上一屏；在第一屏时离开。
    fn back(&mut self) -> Flow {
        if self.step == 0 {
            return Flow::Cancel;
        }
        self.step -= 1;
        self.enter();
        Flow::Stay
    }

    /// Handles one key press.
    /// 处理一次按键。
    fn handle(&mut self, key: KeyEvent) -> Flow {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Flow::Cancel;
        }
        if self.current() == Step::Location {
            return self.edit(key);
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.move_row(false),
            KeyCode::Down | KeyCode::Char('j') => self.move_row(true),
            KeyCode::Char(' ') => self.toggle(),
            KeyCode::Enter => return self.advance(),
            KeyCode::Esc | KeyCode::Backspace | KeyCode::Left => return self.back(),
            KeyCode::Char('q') => return Flow::Cancel,
            _ => {}
        }
        Flow::Stay
    }

    /// Edits the directory input.
    /// 编辑目录输入。
    fn edit(&mut self, key: KeyEvent) -> Flow {
        let mut chars: Vec<char> = self.draft.root.chars().collect();
        match key.code {
            KeyCode::Enter => return self.advance(),
            KeyCode::Esc => return self.back(),
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(chars.len()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = chars.len(),
            KeyCode::Backspace if self.cursor > 0 => {
                self.cursor -= 1;
                chars.remove(self.cursor);
            }
            KeyCode::Delete if self.cursor < chars.len() => {
                chars.remove(self.cursor);
            }
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                chars.insert(self.cursor, character);
                self.cursor += 1;
            }
            _ => return Flow::Stay,
        }
        self.draft.root = chars.into_iter().collect();
        self.error = None;
        Flow::Stay
    }
}

/// Checks that `text` names a directory the setup program may install into.
/// 检查 `text` 是否指向安装器可以安装到的目录。
fn check_location(platform: Platform, text: &str) -> Result<PathBuf, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err(String::from("enter a directory"));
    }
    let root = resolve_root(Some(&PathBuf::from(text)), platform)?;
    root::ownership(&root)?;
    Ok(root)
}

/// Runs the wizard on the terminal; `None` when the person left without installing.
/// 在终端上运行向导；用户未安装就离开时为 `None`。
pub fn run(platform: Platform, draft: Draft) -> Result<Option<Draft>, String> {
    let mut translator = Translator::new(&[&SETUP]);
    translator.set_locale(vulcan_i18n::system_locale());
    let mut wizard = Wizard::new(platform, draft);
    let mut terminal = ratatui::try_init()
        .map_err(|error| format!("cannot start the interactive installer: {error}"))?;
    let result = loop {
        if let Err(error) = terminal.draw(|frame| render(frame, &wizard, &translator)) {
            break Err(format!("cannot draw the interactive installer: {error}"));
        }
        let key = match event::read() {
            Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => key,
            Ok(_) => continue,
            Err(error) => break Err(format!("cannot read the keyboard: {error}")),
        };
        match wizard.handle(key) {
            Flow::Stay => {}
            Flow::Finish => break Ok(Some(wizard.draft.clone())),
            Flow::Cancel => break Ok(None),
        }
    };
    let _ = ratatui::try_restore();
    result
}

/// Text of `key`, or the key itself when the catalog lacks it (a complete surface never does).
/// `key` 的文案；目录缺少该键时返回键本身（完整分面不会缺少）。
fn text<'t>(translator: &'t Translator, key: &'t str) -> &'t str {
    translator.text(key).unwrap_or(key)
}

/// Draws the current screen.
/// 绘制当前屏。
fn render(frame: &mut Frame<'_>, wizard: &Wizard, translator: &Translator) {
    let t = |key| text(translator, key);
    let area = frame.area();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(MUTED))
        .title(Span::styled(
            format!(" {} ", t("setup.title")),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .areas(inner.inner(Margin::new(2, 1)));

    let steps = wizard.steps();
    let step = wizard.current();
    let title = match step {
        Step::Mode => "setup.mode.title",
        Step::Components => "setup.components.title",
        Step::Location => "setup.location.title",
        Step::Integration => "setup.integration.title",
        Step::Channel => "setup.channel.title",
        Step::Summary => "setup.summary.title",
    };
    let counter = format_template(
        t("setup.step"),
        &[
            ("current", &(wizard.step + 1).to_string()),
            ("total", &steps.len().to_string()),
        ],
    );
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(counter, Style::default().fg(MUTED)),
            Line::styled(t(title), Style::default().add_modifier(Modifier::BOLD)),
        ]),
        header,
    );

    let mut lines = match step {
        Step::Mode => {
            let options = modes(wizard.platform)
                .into_iter()
                .map(|mode| match mode {
                    Mode::Desktop => ("setup.mode.desktop", "setup.mode.desktopDetail"),
                    Mode::Terminal => ("setup.mode.terminal", "setup.mode.terminalDetail"),
                    Mode::Service => ("setup.mode.service", "setup.mode.serviceDetail"),
                })
                .collect::<Vec<_>>();
            choice_lines(translator, &options, wizard.row, None)
        }
        Step::Components => choice_lines(
            translator,
            &[("setup.components.browser", "setup.components.browserDetail")],
            wizard.row,
            Some(&[wizard.draft.browser]),
        ),
        Step::Integration => {
            let toggles = wizard.toggles();
            let options: Vec<_> = toggles
                .iter()
                .map(|toggle| match toggle {
                    Toggle::Shortcut => (
                        "setup.integration.shortcut",
                        "setup.integration.shortcutDetail",
                    ),
                    Toggle::Path => ("setup.integration.path", "setup.integration.pathDetail"),
                })
                .collect();
            let states: Vec<bool> = toggles
                .iter()
                .map(|toggle| match toggle {
                    Toggle::Shortcut => wizard.draft.shortcut,
                    Toggle::Path => wizard.draft.path,
                })
                .collect();
            choice_lines(translator, &options, wizard.row, Some(&states))
        }
        Step::Channel => choice_lines(
            translator,
            &[
                ("setup.channel.stable", "setup.channel.stableDetail"),
                ("setup.channel.preview", "setup.channel.previewDetail"),
            ],
            wizard.row,
            None,
        ),
        Step::Location => location_lines(translator, wizard),
        Step::Summary => summary_lines(translator, wizard),
    };
    if let Some(error) = &wizard.error {
        lines.push(Line::default());
        lines.push(Line::styled(error.clone(), Style::default().fg(DANGER)));
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), body);

    let hint = match step {
        Step::Mode | Step::Channel => "setup.hint.choose",
        Step::Components | Step::Integration => "setup.hint.toggle",
        Step::Location => "setup.hint.input",
        Step::Summary => "setup.hint.summary",
    };
    frame.render_widget(
        Paragraph::new(Line::styled(t(hint), Style::default().fg(MUTED)))
            .wrap(Wrap { trim: false }),
        footer,
    );
}

/// Lines of a list screen: radio buttons when `states` is `None`, check boxes otherwise.
/// 列表屏的各行：`states` 为 `None` 时为单选按钮，否则为复选框。
fn choice_lines<'a>(
    translator: &'a Translator,
    options: &[(&'a str, &'a str)],
    row: usize,
    states: Option<&[bool]>,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    for (index, (label, detail)) in options.iter().enumerate() {
        let highlighted = index == row;
        let mark = match states {
            None if highlighted => "(•)",
            None => "( )",
            Some(states) if states.get(index).copied().unwrap_or(false) => "[x]",
            Some(_) => "[ ]",
        };
        let style = if highlighted {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let pointer = if highlighted { "›" } else { " " };
        lines.push(Line::from(vec![
            Span::styled(format!("{pointer} {mark} "), style),
            Span::styled(text(translator, label), style),
        ]));
        lines.push(Line::styled(
            format!("      {}", text(translator, detail)),
            Style::default().fg(MUTED),
        ));
        lines.push(Line::default());
    }
    lines
}

/// Lines of the directory screen, with the cursor shown as a reversed cell.
/// 目录屏的各行，光标以反色单元显示。
fn location_lines<'a>(translator: &'a Translator, wizard: &Wizard) -> Vec<Line<'a>> {
    let chars: Vec<char> = wizard.draft.root.chars().collect();
    let before: String = chars[..wizard.cursor.min(chars.len())].iter().collect();
    let at: String = chars
        .get(wizard.cursor)
        .map_or_else(|| String::from(" "), |character| character.to_string());
    let after: String = chars.iter().skip(wizard.cursor + 1).collect();
    vec![
        Line::styled(
            text(translator, "setup.location.detail"),
            Style::default().fg(MUTED),
        ),
        Line::default(),
        Line::from(vec![
            Span::styled("› ", Style::default().fg(ACCENT)),
            Span::raw(before),
            Span::styled(at, Style::default().add_modifier(Modifier::REVERSED)),
            Span::raw(after),
        ]),
    ]
}

/// Lines of the summary screen.
/// 汇总屏的各行。
fn summary_lines<'a>(translator: &'a Translator, wizard: &Wizard) -> Vec<Line<'a>> {
    let t = |key| text(translator, key);
    let draft = &wizard.draft;
    let mut components = vec![t("setup.component.core"), t("setup.component.tui")];
    if draft.mode == Mode::Desktop {
        components.push(t("setup.component.gui"));
    }
    if draft.browser {
        components.push(t("setup.components.browser"));
    }
    let mode = match draft.mode {
        Mode::Desktop => t("setup.mode.desktop"),
        Mode::Terminal => t("setup.mode.terminal"),
        Mode::Service => t("setup.mode.service"),
    };
    let channel = match draft.channel {
        Channel::Stable => t("setup.channel.stable"),
        Channel::Preview => t("setup.channel.preview"),
    };
    let root = check_location(wizard.platform, &draft.root)
        .unwrap_or_else(|_| PathBuf::from(draft.root.trim()));
    let labels = [
        "setup.summary.mode",
        "setup.summary.components",
        "setup.summary.directory",
        "setup.summary.channel",
    ];
    let label_width = labels
        .iter()
        .map(|key| Span::raw(t(key)).width())
        .max()
        .unwrap_or(0);
    let field = |label: &'a str, value: String| {
        let padding = label_width.saturating_sub(Span::raw(label).width()) + 2;
        Line::from(vec![
            Span::styled(
                format!("{label}{}", " ".repeat(padding)),
                Style::default().fg(MUTED),
            ),
            Span::raw(value),
        ])
    };
    let mut lines = vec![
        field(t("setup.summary.mode"), mode.to_owned()),
        field(t("setup.summary.components"), components.join(", ")),
        field(t("setup.summary.directory"), root.display().to_string()),
        field(t("setup.summary.channel"), channel.to_owned()),
        Line::default(),
        Line::styled(t("setup.summary.writes"), Style::default().fg(MUTED)),
    ];
    let version = String::new();
    let plan = Plan {
        platform: wizard.platform,
        root: &root,
        version: &version,
        mode: draft.mode,
        shortcut: draft.shortcut,
        path: draft.path,
    };
    let locations = plan.locations().unwrap_or_default();
    if locations.is_empty() {
        lines.push(Line::raw(format!("  {}", t("setup.summary.writesNone"))));
    }
    for location in locations {
        let (key, path) = match &location {
            Location::Commands(path) => ("setup.writes.commands", Some(path)),
            Location::MenuEntry(path) => ("setup.writes.menuEntry", Some(path)),
            Location::AppBundle(path) => ("setup.writes.appBundle", Some(path)),
            Location::UserPath(path) => ("setup.writes.userPath", Some(path)),
            Location::UninstallEntry => ("setup.writes.uninstallEntry", None),
            Location::UserService => ("setup.writes.userService", None),
        };
        let path = path
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        lines.push(Line::raw(format!(
            "  {}",
            format_template(t(key), &[("path", &path)])
        )));
    }
    lines.push(Line::default());
    lines.push(Line::styled(
        t("setup.summary.detail"),
        Style::default().fg(MUTED),
    ));
    lines
}

/// Whether the wizard should run: no `--yes`, and both ends are a terminal.
/// 是否运行向导：没有 `--yes`，且输入输出都是终端。
pub fn wanted(yes: bool) -> bool {
    use std::io::IsTerminal;
    !yes && std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn platform(name: &str) -> Platform {
        Platform::ALL
            .into_iter()
            .find(|platform| platform.as_str() == name)
            .expect("platform")
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn draft(root: &str) -> Draft {
        Draft {
            mode: Mode::Desktop,
            browser: false,
            root: root.to_owned(),
            shortcut: true,
            path: true,
            channel: Channel::Stable,
        }
    }

    #[test]
    fn moving_on_the_mode_screen_selects_and_wraps() {
        let mut wizard = Wizard::new(platform("linux-x64"), draft("/opt/vulcan"));
        assert_eq!(wizard.current(), Step::Mode);
        wizard.handle(key(KeyCode::Down));
        assert_eq!(wizard.draft.mode, Mode::Terminal);
        wizard.handle(key(KeyCode::Down));
        assert_eq!(wizard.draft.mode, Mode::Service);
        wizard.handle(key(KeyCode::Down));
        assert_eq!(wizard.draft.mode, Mode::Desktop);
        assert_eq!(wizard.toggles(), [Toggle::Shortcut, Toggle::Path]);
        let windows = Wizard::new(platform("windows-x64"), draft("C:\\Vulcan"));
        assert_eq!(windows.current(), Step::Components);
    }

    #[test]
    fn keys_walk_the_screens_and_edit_the_draft() {
        // The directory is checked by the rules of the machine running the test.
        // 目录按运行测试的机器的规则检查。
        let host = Platform::current().expect("supported host");
        let temp = tempfile::tempdir().expect("temp dir");
        let root = temp.path().join("vulcan").display().to_string();
        let mut start = draft(&root);
        start.mode = Mode::Terminal;
        let mut wizard = Wizard::new(host, start);
        if wizard.current() == Step::Mode {
            assert_eq!(wizard.handle(key(KeyCode::Enter)), Flow::Stay);
        }
        assert_eq!(wizard.current(), Step::Components);
        wizard.handle(key(KeyCode::Char(' ')));
        assert!(wizard.draft.browser);
        wizard.handle(key(KeyCode::Enter));
        assert_eq!(wizard.current(), Step::Location);
        // Typing `q` edits the path instead of quitting.
        // 输入 `q` 编辑路径而不是退出。
        wizard.handle(key(KeyCode::Char('q')));
        assert!(wizard.draft.root.ends_with('q'));
        wizard.handle(key(KeyCode::Backspace));
        assert_eq!(wizard.draft.root, root);
        wizard.handle(key(KeyCode::Enter));
        // Terminal mode has no shortcut, so the only toggle is PATH.
        // 终端模式没有快捷方式，因此唯一的开关是 PATH。
        assert_eq!(wizard.toggles(), [Toggle::Path]);
        wizard.handle(key(KeyCode::Char(' ')));
        assert!(!wizard.draft.path);
        wizard.handle(key(KeyCode::Enter));
        wizard.handle(key(KeyCode::Down));
        assert_eq!(wizard.draft.channel, Channel::Preview);
        wizard.handle(key(KeyCode::Enter));
        assert_eq!(wizard.current(), Step::Summary);
        assert_eq!(wizard.handle(key(KeyCode::Enter)), Flow::Finish);
    }

    #[test]
    fn a_refused_directory_keeps_the_location_screen() {
        let temp = tempfile::tempdir().expect("temp dir");
        std::fs::write(temp.path().join("notes.txt"), "mine").expect("foreign file");
        let mut wizard = Wizard::new(
            platform("windows-x64"),
            draft(&temp.path().display().to_string()),
        );
        assert_eq!(wizard.current(), Step::Components);
        wizard.handle(key(KeyCode::Enter));
        assert_eq!(wizard.handle(key(KeyCode::Enter)), Flow::Stay);
        assert_eq!(wizard.current(), Step::Location);
        assert!(wizard.error.is_some());
        wizard.handle(key(KeyCode::Esc));
        assert_eq!(wizard.current(), Step::Components);
        assert_eq!(wizard.handle(key(KeyCode::Esc)), Flow::Cancel);
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(wizard.handle(ctrl_c), Flow::Cancel);
    }

    #[test]
    fn every_screen_renders_catalog_text_in_every_language() {
        let temp = tempfile::tempdir().expect("temp dir");
        let root = temp.path().join("vulcan").display().to_string();
        for name in ["linux-x64", "macos-arm64", "windows-x64"] {
            for locale in vulcan_i18n::LOCALES {
                let mut translator = Translator::new(&[&SETUP]);
                assert!(translator.set_locale(locale.code));
                let mut start = draft(&root);
                if name.starts_with("windows") {
                    start.mode = Mode::Terminal;
                }
                let mut wizard = Wizard::new(platform(name), start);
                let mut terminal = Terminal::new(TestBackend::new(60, 30)).expect("terminal");
                for _ in 0..wizard.steps().len() {
                    terminal
                        .draw(|frame| render(frame, &wizard, &translator))
                        .expect("draw");
                    // A key missing from the catalog would be drawn as the key itself.
                    // 目录中缺少的键会以键本身绘制出来。
                    let screen: String = terminal
                        .backend()
                        .buffer()
                        .content()
                        .iter()
                        .map(|cell| cell.symbol())
                        .collect();
                    assert!(
                        !screen.contains("setup."),
                        "{name} {} {:?} shows a raw key",
                        locale.code,
                        wizard.current()
                    );
                    wizard.advance();
                }
            }
        }
    }
}
