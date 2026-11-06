//! The interactive installer after the wizard: the run works on its own thread while the pages
//! show its steps, ask for the review, follow the download and end with what was done and how to
//! go on. Text comes from the `setup` catalog surface; errors and integration notes stay English.
//! 向导之后的交互式安装界面：运行在独立线程中工作，各页显示其步骤、请求审阅、跟随下载进度，最后说明
//! 做了什么以及之后如何继续。文案来自 `setup` 目录分面；错误与集成说明保持英文。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use vulcan_i18n::Translator;
use vulcan_install_core::names::Channel;

use crate::prompt::megabytes;
use crate::report::{
    Action, BACK, BROWSER_DEPENDENCIES_COMMAND, Decision, DesktopWay, Outcome, Privileged,
    Reporter, Review, Stage,
};
use crate::screen::{
    ACCENT, DANGER, MUTED, Screen, component_names, fields, filled, location_lines, page, text,
};

/// How often the pages are drawn again while the run works.
/// 运行工作期间各页重绘的间隔。
const TICK: Duration = Duration::from_millis(100);

/// Width of the download bar, in cells.
/// 下载进度条的宽度，以单元格计。
const BAR_WIDTH: usize = 30;

/// What the run's thread tells the pages.
/// 运行线程告知各页的内容。
enum Message {
    /// A step began.
    /// 某步骤开始。
    Stage(Stage),
    /// Download progress.
    /// 下载进度。
    Progress(u64, u64),
    /// The flag that cancels the run's transfers.
    /// 取消运行传输的标志。
    Cancel(Arc<AtomicBool>),
    /// A review waiting for its answer.
    /// 等待回答的审阅。
    Review(Box<Review>, Sender<Decision>),
    /// Commands waiting to run with administrator rights, and where their result goes.
    /// 等待以管理员权限运行的命令，及其结果的去向。
    Privileged(Box<Privileged>, Sender<bool>),
}

/// The reporter of the run's thread.
/// 运行线程的报告器。
struct Tui {
    /// Where the messages go.
    /// 消息的去向。
    sender: Sender<Message>,
}

impl Reporter for Tui {
    fn stage(&self, stage: Stage) {
        let _ = self.sender.send(Message::Stage(stage));
    }

    fn progress(&self, received: u64, total: u64) {
        let _ = self.sender.send(Message::Progress(received, total));
    }

    fn cancel_with(&self, flag: Arc<AtomicBool>) {
        let _ = self.sender.send(Message::Cancel(flag));
    }

    fn review(&self, review: &Review) -> Result<Decision, String> {
        let (answer, answered) = mpsc::channel();
        self.sender
            .send(Message::Review(Box::new(review.clone()), answer))
            .map_err(|_| String::from("the interactive installer closed"))?;
        answered
            .recv()
            .map_err(|_| String::from("the interactive installer closed"))
    }

    fn privileged(&self, request: &Privileged) -> bool {
        let (result, done) = mpsc::channel();
        self.sender
            .send(Message::Privileged(Box::new(request.clone()), result))
            .is_ok()
            && done.recv().unwrap_or(false)
    }
}

/// The page being shown.
/// 正在显示的页。
enum Page {
    /// The run's steps.
    /// 运行的各步骤。
    Working,
    /// The review and where its answer goes.
    /// 审阅及其回答的去向。
    Review(Box<Review>, Sender<Decision>),
    /// What a finished run did.
    /// 已完成运行所做的事。
    Done(Box<Outcome>),
    /// Why the run stopped.
    /// 运行停止的原因。
    Failed(String),
}

/// What a key press leads to.
/// 按键导致的结果。
#[derive(Debug, PartialEq, Eq)]
enum Flow {
    /// Stay on the pages.
    /// 留在各页中。
    Stay,
    /// Close the pages.
    /// 关闭各页。
    Close,
}

/// The pages' state, independent of the terminal.
/// 各页的状态，与终端无关。
struct Session {
    /// Whether the review can return to the wizard.
    /// 审阅能否返回向导。
    back: bool,
    /// The steps begun so far; the last one is under way.
    /// 至今已开始的步骤；最后一个正在进行。
    stages: Vec<Stage>,
    /// Bytes received and expected.
    /// 已接收与预期的字节数。
    progress: Option<(u64, u64)>,
    /// The flag that cancels the run's transfers.
    /// 取消运行传输的标志。
    cancel: Option<Arc<AtomicBool>>,
    /// Whether the person asked to cancel.
    /// 用户是否要求取消。
    cancelling: bool,
    /// The confirmed review, which names the work in progress.
    /// 已确认的审阅，用于说明进行中的工作。
    confirmed: Option<Box<Review>>,
    /// The page.
    /// 页面。
    page: Page,
    /// Frames drawn, for the activity dots.
    /// 已绘制的帧数，用于活动圆点。
    tick: usize,
    /// First line shown of the final page.
    /// 最终页显示的第一行。
    scroll: u16,
    /// Commands waiting to run with administrator rights outside the pages.
    /// 等待在各页之外以管理员权限运行的命令。
    privileged: Option<(Box<Privileged>, Sender<bool>)>,
}

impl Session {
    /// Pages that start with the first step.
    /// 从第一步开始的各页。
    fn new(back: bool) -> Self {
        Self {
            back,
            stages: Vec::new(),
            progress: None,
            cancel: None,
            cancelling: false,
            confirmed: None,
            page: Page::Working,
            tick: 0,
            scroll: 0,
            privileged: None,
        }
    }

    /// The step under way.
    /// 正在进行的步骤。
    fn current(&self) -> Option<Stage> {
        self.stages.last().copied()
    }

    /// Takes in one message of the run.
    /// 接收运行的一条消息。
    fn receive(&mut self, message: Message) {
        match message {
            Message::Stage(stage) => {
                if self.current() != Some(stage) {
                    self.stages.push(stage);
                }
            }
            Message::Progress(received, total) => self.progress = Some((received, total)),
            Message::Cancel(flag) => {
                if self.cancelling {
                    flag.store(true, Ordering::Relaxed);
                }
                self.cancel = Some(flag);
            }
            // A cancellation asked for before the review answers it.
            // 在审阅之前提出的取消直接回答审阅。
            Message::Review(review, answer) => {
                if self.cancelling {
                    let _ = answer.send(Decision::Cancel);
                } else {
                    self.page = Page::Review(review, answer);
                }
            }
            // A person who asked to leave is not asked for a password; the step is reported as
            // not done.
            // 已要求离开的用户不会被询问密码；该步骤报告为未完成。
            Message::Privileged(request, result) => {
                if self.cancelling {
                    let _ = result.send(false);
                } else {
                    self.privileged = Some((request, result));
                }
            }
        }
    }

    /// Asks the run to stop while that changes nothing.
    /// 在不会改动任何内容时要求运行停止。
    fn cancel(&mut self) {
        if !self.current().is_none_or(Stage::cancellable) {
            return;
        }
        self.cancelling = true;
        if let Some(flag) = &self.cancel {
            flag.store(true, Ordering::Relaxed);
        }
    }

    /// Answers the review and returns to the steps.
    /// 回答审阅并返回各步骤。
    fn answer(&mut self, decision: Decision) {
        if let Page::Review(review, answer) = std::mem::replace(&mut self.page, Page::Working) {
            let _ = answer.send(decision);
            if decision == Decision::Continue {
                self.confirmed = Some(review);
            }
        }
    }

    /// Handles one key press.
    /// 处理一次按键。
    fn handle(&mut self, key: KeyEvent) -> Flow {
        let interrupt =
            key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c');
        match &self.page {
            Page::Working => {
                if interrupt || matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                    self.cancel();
                }
            }
            Page::Review(..) => match key.code {
                _ if interrupt => self.answer(Decision::Cancel),
                KeyCode::Enter => self.answer(Decision::Continue),
                KeyCode::Esc | KeyCode::Backspace | KeyCode::Left if self.back => {
                    self.answer(Decision::Back)
                }
                KeyCode::Esc | KeyCode::Char('q') => self.answer(Decision::Cancel),
                _ => {}
            },
            Page::Done(_) | Page::Failed(_) => match key.code {
                KeyCode::Up | KeyCode::Char('k') => self.scroll = self.scroll.saturating_sub(1),
                KeyCode::Down | KeyCode::Char('j') => self.scroll = self.scroll.saturating_add(1),
                KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(10),
                KeyCode::PageDown => self.scroll = self.scroll.saturating_add(10),
                KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') => return Flow::Close,
                _ if interrupt => return Flow::Close,
                _ => {}
            },
        }
        Flow::Stay
    }
}

/// Runs `work` on its own thread with the pages on `screen`; `back` lets the review return to the
/// wizard. The run's result is returned once the person closed the final page; going back or
/// cancelling returns at once.
/// 在独立线程中运行 `work`，并在 `screen` 上显示各页；`back` 允许审阅返回向导。用户关闭最终页后返回运行
/// 结果；返回或取消时立即返回。
pub fn run<F>(screen: &mut Screen, back: bool, work: F) -> Result<Outcome, String>
where
    F: FnOnce(&dyn Reporter) -> Result<Outcome, String> + Send,
{
    let (sender, receiver) = mpsc::channel();
    std::thread::scope(|scope| {
        let mut worker = Some(scope.spawn(move || work(&Tui { sender })));
        let mut session = Session::new(back);
        let result = loop {
            drain(&mut session, &receiver);
            // `sudo` asks for the password in the terminal, so the pages give it back meanwhile.
            // `sudo` 在终端中询问密码，因此期间各页把终端交还。
            if let Some((request, result)) = session.privileged.take() {
                let ran = screen.suspended(|translator| {
                    println!();
                    println!("{}", text(translator, request.purpose.catalog_key()));
                    crate::integrate::run_privileged(&request.commands)
                });
                match ran {
                    Ok(succeeded) => {
                        let _ = result.send(succeeded);
                    }
                    Err(error) => break Err(error),
                }
            }
            if worker.as_ref().is_some_and(|handle| handle.is_finished()) {
                // Everything the run sent before it ended is in the channel now.
                // 运行结束前发送的全部内容此时都已在通道中。
                drain(&mut session, &receiver);
                let finished = worker.take().map_or_else(
                    || Err(String::from(crate::CANCELLED)),
                    |handle| {
                        handle.join().unwrap_or_else(|_| {
                            Err(String::from("the installation stopped unexpectedly"))
                        })
                    },
                );
                match finished {
                    Ok(outcome) => session.page = Page::Done(Box::new(outcome)),
                    Err(error) if error == crate::CANCELLED || error == BACK => break Err(error),
                    Err(error) => session.page = Page::Failed(error),
                }
            }
            if let Err(error) = screen.draw(|frame, translator| render(frame, &session, translator))
            {
                break Err(error);
            }
            session.tick = session.tick.wrapping_add(1);
            match event::poll(TICK) {
                Ok(true) => {}
                Ok(false) => continue,
                Err(error) => break Err(format!("cannot read the keyboard: {error}")),
            }
            let key = match event::read() {
                Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => key,
                Ok(_) => continue,
                Err(error) => break Err(format!("cannot read the keyboard: {error}")),
            };
            if session.handle(key) == Flow::Close {
                break match std::mem::replace(&mut session.page, Page::Working) {
                    Page::Done(outcome) => Ok(*outcome),
                    Page::Failed(error) => Err(error),
                    Page::Working | Page::Review(..) => Err(String::from(crate::CANCELLED)),
                };
            }
        };
        // Leaving while the run works stops it where that changes nothing and answers a waiting
        // review; the scope then waits for the run's thread.
        // 运行仍在工作时离开，会在不改动任何内容的情况下停止它并回答等待中的审阅；随后作用域等待运行线程。
        if worker.is_some() {
            session.cancel();
            session.answer(Decision::Cancel);
        }
        drop(receiver);
        result
    })
}

/// Takes in every message waiting in `receiver`.
/// 接收 `receiver` 中等待的全部消息。
fn drain(session: &mut Session, receiver: &Receiver<Message>) {
    while let Ok(message) = receiver.try_recv() {
        session.receive(message);
    }
}

/// Draws the current page.
/// 绘制当前页。
fn render(frame: &mut Frame<'_>, session: &Session, translator: &Translator) {
    let t = |key| text(translator, key);
    match &session.page {
        Page::Working => {
            let title = session.confirmed.as_deref().map_or_else(
                || t("setup.working.title").to_owned(),
                |review| action_title(translator, review, "setup.working"),
            );
            let hint = match session.current() {
                _ if session.cancelling => "",
                Some(stage) if !stage.cancellable() => t("setup.hint.busy"),
                _ => t("setup.hint.working"),
            };
            page(
                frame,
                translator,
                t("setup.working.overline"),
                &title,
                working_lines(translator, session),
                hint,
                0,
            );
        }
        Page::Review(review, _) => {
            let hint = if session.back {
                t("setup.hint.review")
            } else {
                t("setup.hint.reviewOnly")
            };
            page(
                frame,
                translator,
                t("setup.review.overline"),
                &action_title(translator, review, "setup.review"),
                review_lines(translator, review),
                hint,
                0,
            );
        }
        Page::Done(outcome) => {
            let version = outcome.version.as_str();
            let title = match &outcome.action {
                Action::Install => {
                    filled(translator, "setup.done.installed", &[("version", version)])
                }
                Action::Update { from } => filled(
                    translator,
                    "setup.done.updated",
                    &[("from", from), ("version", version)],
                ),
                Action::Refresh => {
                    filled(translator, "setup.done.refreshed", &[("version", version)])
                }
            };
            page(
                frame,
                translator,
                t("setup.done.overline"),
                &title,
                done_lines(translator, outcome),
                t("setup.hint.done"),
                session.scroll,
            );
        }
        Page::Failed(error) => {
            let lines = vec![
                Line::styled(error.clone(), Style::default().fg(DANGER)),
                Line::default(),
                Line::styled(t("setup.failed.detail"), Style::default().fg(MUTED)),
            ];
            page(
                frame,
                translator,
                t("setup.failed.overline"),
                t("setup.failed.title"),
                lines,
                t("setup.hint.done"),
                session.scroll,
            );
        }
    }
}

/// The title naming the work of `review`, from the keys below `prefix` (`.install`, `.update`,
/// `.refresh`).
/// 说明 `review` 所做工作的标题，取自 `prefix` 之下的键（`.install`、`.update`、`.refresh`）。
fn action_title(translator: &Translator, review: &Review, prefix: &str) -> String {
    let version = review.version.as_str();
    match &review.action {
        Action::Install => filled(
            translator,
            &format!("{prefix}.install"),
            &[("version", version)],
        ),
        Action::Update { from } => filled(
            translator,
            &format!("{prefix}.update"),
            &[("from", from), ("version", version)],
        ),
        Action::Refresh => filled(
            translator,
            &format!("{prefix}.refresh"),
            &[("version", version)],
        ),
    }
}

/// Catalog text of a channel.
/// 通道的目录文案。
fn channel_name(translator: &Translator, channel: Channel) -> &str {
    text(
        translator,
        match channel {
            Channel::Stable => "setup.channel.stable",
            Channel::Preview => "setup.channel.preview",
        },
    )
}

/// Lines of the steps page: finished steps, the step under way and the download bar.
/// 步骤页的各行：已完成的步骤、正在进行的步骤以及下载进度条。
fn working_lines<'a>(translator: &'a Translator, session: &Session) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    let last = session.stages.len().saturating_sub(1);
    for (index, stage) in session.stages.iter().enumerate() {
        let label = match stage {
            Stage::Checking(channel) => filled(
                translator,
                "setup.stage.checking",
                &[("channel", channel_name(translator, *channel))],
            ),
            Stage::Preparing => text(translator, "setup.stage.preparing").to_owned(),
            Stage::Downloading => text(translator, "setup.stage.downloading").to_owned(),
            Stage::Switching => text(translator, "setup.stage.switching").to_owned(),
            Stage::Integrating => text(translator, "setup.stage.integrating").to_owned(),
            Stage::Service => text(translator, "setup.stage.service").to_owned(),
            Stage::Cleaning => text(translator, "setup.stage.cleaning").to_owned(),
        };
        if index < last {
            lines.push(Line::styled(
                format!("  √ {label}"),
                Style::default().fg(MUTED),
            ));
            continue;
        }
        let dots = ".".repeat(session.tick / 4 % 4);
        lines.push(Line::from(vec![
            Span::styled("  › ", Style::default().fg(ACCENT)),
            Span::styled(label, Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(dots, Style::default().fg(ACCENT)),
        ]));
        if *stage == Stage::Downloading
            && let Some((received, total)) = session.progress
        {
            lines.push(Line::default());
            lines.push(bar_line(translator, received, total));
        }
    }
    if session.cancelling {
        lines.push(Line::default());
        lines.push(Line::styled(
            text(translator, "setup.working.cancelling"),
            Style::default().fg(DANGER),
        ));
    }
    lines
}

/// The download bar with its share and sizes.
/// 带比例与大小的下载进度条。
fn bar_line<'a>(translator: &'a Translator, received: u64, total: u64) -> Line<'a> {
    let received = received.min(total);
    let share = if total == 0 {
        1.0
    } else {
        received as f64 / total as f64
    };
    let full = ((share * BAR_WIDTH as f64).round() as usize).min(BAR_WIDTH);
    let sizes = filled(
        translator,
        "setup.progress",
        &[
            ("received", &megabytes(received)),
            ("total", &megabytes(total)),
        ],
    );
    Line::from(vec![
        Span::raw("    "),
        Span::styled("█".repeat(full), Style::default().fg(ACCENT)),
        Span::styled("░".repeat(BAR_WIDTH - full), Style::default().fg(MUTED)),
        Span::raw(format!("  {:>3}%  {sizes}", (share * 100.0).floor() as u64)),
    ])
}

/// Lines of the review page.
/// 审阅页的各行。
fn review_lines<'a>(translator: &'a Translator, review: &Review) -> Vec<Line<'a>> {
    let t = |key| text(translator, key);
    let download = match review.download {
        0 => t("setup.review.downloadNone").to_owned(),
        bytes => megabytes(bytes),
    };
    let mut rows = vec![
        (
            t("setup.summary.components"),
            component_names(translator, &review.components),
        ),
        (
            t("setup.summary.directory"),
            review.directory.display().to_string(),
        ),
        (
            t("setup.summary.channel"),
            channel_name(translator, review.channel).to_owned(),
        ),
        (t("setup.review.download"), download),
    ];
    if let Some(size) = review.installed_size {
        rows.push((t("setup.review.size"), megabytes(size)));
    }
    if let Some(range) = &review.port_range {
        rows.push((t("setup.summary.ports"), range.clone()));
    }
    let mut lines = fields(rows);
    lines.push(Line::default());
    lines.push(Line::styled(
        t("setup.summary.writes"),
        Style::default().fg(MUTED),
    ));
    lines.extend(location_lines(
        translator,
        review.platform,
        &review.locations,
    ));
    lines.push(Line::default());
    let detail = if review.action == Action::Refresh {
        "setup.review.refreshDetail"
    } else {
        "setup.review.detail"
    };
    lines.push(Line::styled(t(detail), Style::default().fg(MUTED)));
    lines
}

/// Lines of the final page: where it is, how to open it, and how to use the setup program next
/// time.
/// 最终页的各行：安装位置、打开方式，以及下次如何使用安装器。
fn done_lines<'a>(translator: &'a Translator, outcome: &Outcome) -> Vec<Line<'a>> {
    let t = |key| text(translator, key);
    let heading = |key| Line::styled(t(key), Style::default().add_modifier(Modifier::BOLD));
    let item = |line: String| Line::raw(format!("  {line}"));
    let mut lines = fields(vec![(
        t("setup.summary.directory"),
        outcome.directory.display().to_string(),
    )]);
    lines.push(Line::default());
    lines.push(heading("setup.done.open"));
    lines.push(item(filled(
        translator,
        "setup.done.terminal",
        &[("command", &outcome.terminal)],
    )));
    if let Some(way) = &outcome.desktop {
        lines.push(item(match way {
            DesktopWay::StartMenu => t("setup.done.startMenu").to_owned(),
            DesktopWay::AppMenu => t("setup.done.appMenu").to_owned(),
            DesktopWay::Shortcut => t("setup.done.shortcut").to_owned(),
            DesktopWay::Bundle(path) => filled(
                translator,
                "setup.done.bundle",
                &[("path", &path.display().to_string())],
            ),
            DesktopWay::Program(command) => {
                filled(translator, "setup.done.program", &[("command", command)])
            }
        }));
    }
    lines.push(Line::default());
    lines.push(heading("setup.done.next"));
    let setup = [("command", outcome.setup.as_str())];
    for key in [
        "setup.done.modify",
        "setup.done.update",
        "setup.done.uninstall",
    ] {
        lines.push(item(filled(translator, key, &setup)));
    }
    if outcome.platform.is_windows() {
        lines.push(item(t("setup.done.uninstallWindows").to_owned()));
    }
    lines.push(Line::styled(
        format!("  {}", t("setup.done.selfUpdate")),
        Style::default().fg(MUTED),
    ));
    if !outcome.removed.is_empty() || outcome.browser_dependencies {
        lines.push(Line::default());
    }
    if !outcome.removed.is_empty() {
        lines.push(Line::raw(filled(
            translator,
            "setup.done.removed",
            &[("versions", &outcome.removed.join(", "))],
        )));
    }
    if outcome.browser_dependencies {
        lines.push(Line::raw(filled(
            translator,
            "setup.done.browserDependencies",
            &[("command", BROWSER_DEPENDENCIES_COMMAND)],
        )));
    }
    if !outcome.notes.is_empty() {
        lines.push(Line::default());
        lines.push(heading("setup.done.notes"));
        for note in &outcome.notes {
            lines.push(Line::styled(
                format!("  {note}"),
                Style::default().fg(MUTED),
            ));
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::integrate::Location;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use vulcan_i18n::SETUP;
    use vulcan_install_core::names::{ComponentName, Platform};

    fn platform(name: &str) -> Platform {
        Platform::ALL
            .into_iter()
            .find(|platform| platform.as_str() == name)
            .expect("platform")
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn review(platform: Platform, action: Action) -> Review {
        Review {
            platform,
            action,
            version: String::from("1.2.3"),
            channel: Channel::Preview,
            components: ["core", "tui", "gui", "browser"]
                .into_iter()
                .map(|name| ComponentName::new(name).expect("name"))
                .collect(),
            directory: PathBuf::from("/opt/vulcan"),
            download: 96_000_000,
            installed_size: Some(2_400_000_000),
            locations: vec![
                Location::Commands(PathBuf::from("/home/me/.local/bin")),
                Location::MenuEntry(PathBuf::from("/home/me/vulcan-code.desktop")),
                Location::UninstallEntry,
                Location::UserService,
                Location::Linger,
            ],
            port_range: Some(String::from("18000-18029")),
        }
    }

    fn outcome(platform: Platform, desktop: DesktopWay) -> Outcome {
        Outcome {
            platform,
            action: Action::Update {
                from: String::from("1.2.2"),
            },
            version: String::from("1.2.3"),
            directory: PathBuf::from("/opt/vulcan"),
            terminal: String::from("vulcan-tui"),
            desktop: Some(desktop),
            setup: String::from("vulcan-code-setup"),
            removed: vec![String::from("1.2.1")],
            browser_dependencies: true,
            notes: vec![String::from("A note.")],
        }
    }

    /// Feeds `stage` and answers what the run would see.
    /// 输入 `stage` 并回答运行将看到的内容。
    fn reviewed(session: &mut Session, platform: Platform) -> Receiver<Decision> {
        let (answer, answered) = mpsc::channel();
        session.receive(Message::Review(
            Box::new(review(platform, Action::Install)),
            answer,
        ));
        answered
    }

    #[test]
    fn a_cancellation_before_the_review_answers_it() {
        let host = platform("linux-x64");
        let mut session = Session::new(true);
        session.receive(Message::Stage(Stage::Checking(Channel::Stable)));
        // Cancelling before the run handed over its flag sets the flag once it arrives.
        // 运行交出标志之前的取消，会在标志到达时设置它。
        session.handle(key(KeyCode::Char('q')));
        let flag = Arc::new(AtomicBool::new(false));
        session.receive(Message::Cancel(Arc::clone(&flag)));
        assert!(flag.load(Ordering::Relaxed));
        let answered = reviewed(&mut session, host);
        assert_eq!(answered.try_recv(), Ok(Decision::Cancel));
        assert!(matches!(session.page, Page::Working));
    }

    #[test]
    fn administrator_commands_wait_for_the_terminal_unless_the_person_is_leaving() {
        let request = || {
            Box::new(Privileged {
                purpose: crate::report::Purpose::InstallAppArmorProfile,
                commands: vec![vec![String::from("true")]],
            })
        };
        let mut session = Session::new(false);
        let (result, done) = mpsc::channel();
        session.receive(Message::Privileged(request(), result));
        assert!(session.privileged.is_some());
        assert!(done.try_recv().is_err());

        let mut leaving = Session::new(false);
        leaving.handle(key(KeyCode::Char('q')));
        let (result, done) = mpsc::channel();
        leaving.receive(Message::Privileged(request(), result));
        assert!(leaving.privileged.is_none());
        assert_eq!(done.try_recv(), Ok(false));
    }

    #[test]
    fn steps_that_change_the_installation_are_not_cancelled() {
        let mut session = Session::new(true);
        let flag = Arc::new(AtomicBool::new(false));
        session.receive(Message::Cancel(Arc::clone(&flag)));
        session.receive(Message::Stage(Stage::Switching));
        session.handle(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(!session.cancelling && !flag.load(Ordering::Relaxed));
    }

    #[test]
    fn review_keys_continue_go_back_or_cancel() {
        let host = platform("windows-x64");
        let mut wizard_run = Session::new(true);
        let answered = reviewed(&mut wizard_run, host);
        wizard_run.handle(key(KeyCode::Esc));
        assert_eq!(answered.try_recv(), Ok(Decision::Back));
        let answered = reviewed(&mut wizard_run, host);
        wizard_run.handle(key(KeyCode::Enter));
        assert_eq!(answered.try_recv(), Ok(Decision::Continue));
        assert!(wizard_run.confirmed.is_some());
        // Without a wizard to return to, Esc leaves.
        // 没有可返回的向导时，Esc 直接离开。
        let mut update_run = Session::new(false);
        let answered = reviewed(&mut update_run, host);
        update_run.handle(key(KeyCode::Esc));
        assert_eq!(answered.try_recv(), Ok(Decision::Cancel));
    }

    #[test]
    fn the_final_page_closes_on_enter_and_scrolls() {
        let mut session = Session::new(false);
        session.page = Page::Done(Box::new(outcome(
            platform("linux-x64"),
            DesktopWay::AppMenu,
        )));
        assert_eq!(session.handle(key(KeyCode::Down)), Flow::Stay);
        assert_eq!(session.scroll, 1);
        assert_eq!(session.handle(key(KeyCode::Enter)), Flow::Close);
    }

    #[test]
    fn the_download_bar_stays_within_its_width() {
        let translator = Translator::new(&[&SETUP]);
        for (received, total) in [(0, 0), (0, 10), (5, 10), (10, 10), (20, 10)] {
            let line = bar_line(&translator, received, total);
            let cells: usize = line
                .spans
                .iter()
                .map(|span| {
                    span.content
                        .chars()
                        .filter(|c| matches!(c, '█' | '░'))
                        .count()
                })
                .sum();
            assert_eq!(cells, BAR_WIDTH, "{received} of {total}");
        }
    }

    #[test]
    fn every_page_renders_catalog_text_in_every_language() {
        let ways = [
            DesktopWay::StartMenu,
            DesktopWay::AppMenu,
            DesktopWay::Shortcut,
            DesktopWay::Bundle(PathBuf::from("/Users/me/Applications/Vulcan Code.app")),
            DesktopWay::Program(String::from("vulcan-code")),
        ];
        for name in ["linux-x64", "macos-arm64", "windows-x64"] {
            let host = platform(name);
            let mut pages = Vec::new();
            let mut working = Session::new(true);
            for stage in [
                Stage::Checking(Channel::Stable),
                Stage::Preparing,
                Stage::Downloading,
            ] {
                working.receive(Message::Stage(stage));
            }
            working.receive(Message::Progress(40_000_000, 96_000_000));
            working.cancelling = true;
            pages.push(working);
            let mut busy = Session::new(true);
            busy.confirmed = Some(Box::new(review(host, Action::Refresh)));
            for stage in [
                Stage::Switching,
                Stage::Integrating,
                Stage::Service,
                Stage::Cleaning,
            ] {
                busy.receive(Message::Stage(stage));
            }
            pages.push(busy);
            for action in [
                Action::Install,
                Action::Update {
                    from: String::from("1.2.2"),
                },
                Action::Refresh,
            ] {
                for back in [true, false] {
                    let mut session = Session::new(back);
                    let (answer, _answered) = mpsc::channel();
                    session.page = Page::Review(Box::new(review(host, action.clone())), answer);
                    pages.push(session);
                }
            }
            for way in &ways {
                let mut session = Session::new(true);
                session.page = Page::Done(Box::new(outcome(host, way.clone())));
                pages.push(session);
            }
            let mut failed = Session::new(true);
            failed.page = Page::Failed(String::from("cannot download"));
            pages.push(failed);
            for locale in vulcan_i18n::LOCALES {
                let mut translator = Translator::new(&[&SETUP]);
                assert!(translator.set_locale(locale.code));
                for (index, session) in pages.iter().enumerate() {
                    let mut terminal = Terminal::new(TestBackend::new(80, 60)).expect("terminal");
                    terminal
                        .draw(|frame| render(frame, session, &translator))
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
                        "{name} {} page {index} shows a raw key",
                        locale.code
                    );
                }
            }
        }
    }
}
