// Keyboard and mouse handling.
// 键盘与鼠标处理。
//
// Order: global shortcuts, then the open overlay (modal), then the focused region. Only key
// presses are handled; Windows also reports releases, which are ignored. Mouse input resolves
// against the hit map recorded by the last frame (`mouse.rs`) and ends in the same functions.
// 顺序：全局快捷键，然后是打开的浮层（模态），最后是获得焦点的区域。只处理按下事件；Windows 还会
// 报告释放事件，这些事件被忽略。鼠标输入根据上一帧记录的命中表解析（`mouse.rs`），最终调用相同函数。

mod mouse;
mod overlay;
pub mod pages;
mod regions;

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui_textarea::TextArea;

use super::action::{self, UiAction};
use super::commands::AppCommand;
use super::composer;
use super::overlay::{
    CommitDialog, CommitField, DiffView, Overlay, Picker, PickerAction, PickerItem, PickerKind,
};
use super::pages::{Area, PageId};
use super::{App, Focus, PanelTab, SidebarMode, ThinkingPlacement};
use crate::backend::Command;
use crate::i18n;
use crate::model::*;
use crate::tr;

pub use regions::ask_step;

pub fn handle_event(app: &mut App, event: Event) {
    match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => on_key(app, key),
        Event::Mouse(event) => mouse::on_mouse(app, event),
        Event::Paste(text) => paste(app, &text),
        _ => {}
    }
}

/// Pasted text goes to the text field of the open dialog, else to the composer (where long
/// pastes collapse into a token).
/// 粘贴的文本进入打开的对话框的文本框，否则进入输入框（长粘贴在其中折叠为标记）。
fn paste(app: &mut App, text: &str) {
    // Normalize ordinary text line endings; opaque credential bytes retain their original input.
    // 规范普通文本换行符；不透明凭据字节保留其原始输入。
    let normalized = composer::normalize(text);
    // Single-line continuations intentionally retain only their first source line.
    // 单行后续步骤明确仅保留首条来源行。
    let single_line = || normalized.lines().next().unwrap_or_default().to_string();
    match &mut app.overlay {
        Some(Overlay::Prompt(dialog)) => {
            // The continuation declares multiline semantics; preserve full normalized form text.
            // 后续步骤声明多行语义；保留完整的规范化表单文本。
            if dialog.secret {
                dialog.input.push_str(text);
            } else if dialog.accepts_multiline() {
                dialog.input.push_str(&normalized);
            } else {
                dialog.input.push_str(&single_line());
            }
            dialog.error = None;
        }
        Some(Overlay::Folder(picker)) => {
            picker
                .typing
                .get_or_insert_with(String::new)
                .push_str(&single_line());
        }
        Some(Overlay::Picker(picker)) => {
            picker.filter.push_str(&single_line());
            picker.clamp();
        }
        Some(Overlay::Commit(dialog)) if dialog.field == CommitField::Message => {
            dialog.message.insert_str(&normalized);
        }
        Some(_) => {}
        None => {
            app.set_area(Area::Chat);
            app.focus = Focus::Composer;
            app.paste_into_composer(&normalized);
        }
    }
}

pub(super) fn ctrl(key: &KeyEvent, ch: char) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char(ch)
}

pub(super) fn alt(key: &KeyEvent, ch: char) -> bool {
    key.modifiers.contains(KeyModifiers::ALT) && key.code == KeyCode::Char(ch)
}

/// A printable character typed without Ctrl or Alt.
/// 未按 Ctrl 或 Alt 时输入的可打印字符。
pub(super) fn typed(key: &KeyEvent) -> Option<char> {
    match key.code {
        KeyCode::Char(ch)
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            Some(ch)
        }
        _ => None,
    }
}

fn on_key(app: &mut App, key: KeyEvent) {
    if ctrl(&key, 'q') {
        app.should_quit = true;
        return;
    }
    if ctrl(&key, 'c') {
        interrupt(app);
        return;
    }
    if app.overlay.is_some() {
        overlay::on_key(app, key);
        return;
    }
    let in_chat = app.area == Area::Chat;
    let shortcut = if key.code == KeyCode::F(1) {
        Some(AppCommand::Help)
    } else if key.code == KeyCode::F(2) {
        Some(AppCommand::AreaPicker)
    } else if ctrl(&key, 'p') {
        Some(AppCommand::Palette)
    } else if ctrl(&key, 'f') {
        Some(AppCommand::History)
    } else if ctrl(&key, 'b') {
        Some(AppCommand::ToggleSidebar)
    } else if ctrl(&key, '\\') || ctrl(&key, '4') {
        Some(AppCommand::TogglePanel)
    } else if ctrl(&key, 'o') {
        Some(AppCommand::SelectModel)
    } else if ctrl(&key, 'r') {
        Some(AppCommand::CycleReasoning)
    } else if ctrl(&key, 't') {
        Some(AppCommand::ThinkingPlacement)
    } else if ctrl(&key, 'n') {
        Some(AppCommand::NewConversation)
    } else if alt(&key, 'a') {
        Some(AppCommand::Attach)
    } else if alt(&key, 'h') && in_chat {
        Some(AppCommand::ToggleHtmlAnswers)
    } else if alt(&key, 'd') {
        Some(AppCommand::OpenDiff)
    } else if alt(&key, 'c') {
        Some(AppCommand::Commit)
    } else if alt(&key, 'b') {
        Some(AppCommand::SwitchBranch)
    } else if key.code == KeyCode::BackTab && in_chat {
        Some(AppCommand::TogglePlan)
    } else {
        None
    };
    if let Some(command) = shortcut {
        execute(app, command, None);
        return;
    }
    if key.modifiers.contains(KeyModifiers::ALT)
        && let KeyCode::Char(ch @ '1'..='6') = key.code
    {
        let tab = PanelTab::ALL[(ch as usize) - ('1' as usize)];
        if app.panel_tabs().contains(&tab) {
            action::perform(app, UiAction::ShowPanel(tab));
        }
        return;
    }
    let tab = matches!(key.code, KeyCode::Tab | KeyCode::BackTab);
    if tab && !(app.focus == Focus::Composer && key.code == KeyCode::Tab && complete_slash(app)) {
        cycle_focus(app, key.code == KeyCode::BackTab);
        return;
    }
    match app.focus {
        Focus::Composer => on_composer_key(app, key),
        Focus::Transcript => regions::on_transcript_key(app, key),
        Focus::Sidebar => regions::on_sidebar_key(app, key),
        Focus::Panel => regions::on_panel_key(app, key),
        Focus::Ask => regions::on_ask_key(app, key),
        Focus::AreaNav => pages::on_nav_key(app, key),
        Focus::AreaPage => pages::on_page_key(app, key),
    }
}

/// Ctrl+C: close the overlay, else stop the run, else clear the composer, else arm quit (second
/// press quits).
/// Ctrl+C：关闭浮层；否则停止运行；否则清空输入框；否则进入待退出状态（再按一次退出）。
fn interrupt(app: &mut App) {
    if app.overlay.is_some() {
        app.overlay = None;
        return;
    }
    if app.area == Area::Chat
        && app.run_phase().is_active()
        && let Some(id) = app.selected.clone()
    {
        app.backend.send(Command::StopConversationRun {
            conversation_id: id,
        });
        return;
    }
    if !app.composer_is_empty() {
        app.reset_composer();
        return;
    }
    if app.quit_armed_until.is_some() {
        app.should_quit = true;
    } else {
        app.quit_armed_until = Some(Instant::now() + Duration::from_secs(2));
        app.toast(NoticeSeverity::Info, tr!("tui.quit.confirm"));
    }
}

fn cycle_focus(app: &mut App, back: bool) {
    let has_ask = app.session().is_some_and(|s| s.pending_question.is_some());
    let order: &[Focus] = if app.area == Area::Chat {
        &[
            Focus::Composer,
            Focus::Ask,
            Focus::Transcript,
            Focus::Panel,
            Focus::Sidebar,
        ]
    } else {
        &[Focus::AreaNav, Focus::AreaPage]
    };
    let available = |focus: &Focus| match focus {
        Focus::Ask => has_ask,
        Focus::Panel => app.panel_visible,
        // The rail is always there; focusing it opens the full list as a drawer.
        // 窄栏始终存在；获得焦点时以抽屉形式打开完整列表。
        Focus::Sidebar => true,
        _ => true,
    };
    let start = order.iter().position(|f| *f == app.focus).unwrap_or(0);
    let len = order.len();
    for step in 1..=len {
        let index = if back {
            (start + len * 2 - step) % len
        } else {
            (start + step) % len
        };
        let candidate = order[index];
        if available(&candidate) {
            app.focus = candidate;
            return;
        }
    }
}

// ---- Composer ----

/// Keys that insert a line break instead of sending: Alt/Shift/Ctrl+Enter and Ctrl+J. Many
/// terminals report Shift+Enter as plain Enter, so a trailing `\` before Enter also breaks the
/// line (the backslash is consumed).
/// 插入换行而非发送的按键：Alt/Shift/Ctrl+Enter 与 Ctrl+J。许多终端把 Shift+Enter 报告为普通回车，
/// 因此回车前的行尾 `\` 也会换行（反斜杠被吃掉）。
fn newline_key(key: &KeyEvent) -> bool {
    (key.code == KeyCode::Enter
        && key
            .modifiers
            .intersects(KeyModifiers::ALT | KeyModifiers::SHIFT | KeyModifiers::CONTROL))
        || ctrl(key, 'j')
}

fn on_composer_key(app: &mut App, key: KeyEvent) {
    if newline_key(&key) {
        app.composer.insert_newline();
        return;
    }
    if alt(&key, 'v') {
        action::perform(app, UiAction::PastePreview(None));
        return;
    }
    match key.code {
        KeyCode::Enter if char_before_cursor(app) == Some('\\') => {
            app.composer.delete_char();
            app.composer.insert_newline();
        }
        KeyCode::Enter => submit(app),
        KeyCode::Up if app.composer_is_empty() => {
            app.focus = Focus::Transcript;
            regions::select_relative(app, -1);
        }
        // Backspace on an empty composer drops the last attachment chip.
        // 输入框为空时退格移除最后一个附件。
        KeyCode::Backspace if app.composer_is_empty() && !app.attachments.is_empty() => {
            app.attachments.pop();
        }
        KeyCode::Backspace if app.delete_token_before_cursor() => {}
        KeyCode::Delete if app.delete_token_after_cursor() => {}
        KeyCode::Esc => {}
        _ => {
            let moving_left = matches!(key.code, KeyCode::Left | KeyCode::Home | KeyCode::Up);
            app.composer.input(key);
            app.snap_cursor_out_of_token(moving_left);
            app.forget_unused_pastes();
        }
    }
}

fn char_before_cursor(app: &App) -> Option<char> {
    let ratatui_textarea::DataCursor(row, col) = app.composer.cursor();
    let line = app.composer.lines().get(row)?;
    col.checked_sub(1).and_then(|i| line.chars().nth(i))
}

/// Slash commands matching the composer's first word, for the completion popup.
/// 与输入框首个词匹配的斜杠命令，用于补全弹窗。
pub fn slash_matches(app: &App) -> Vec<AppCommand> {
    let lines = app.composer.lines();
    if lines.len() != 1 {
        return Vec::new();
    }
    let Some(rest) = lines[0].strip_prefix('/') else {
        return Vec::new();
    };
    if rest.contains(' ') {
        return Vec::new();
    }
    AppCommand::ALL
        .into_iter()
        .filter(|c| c.slash().starts_with(rest))
        .collect()
}

fn complete_slash(app: &mut App) -> bool {
    let matches = slash_matches(app);
    let Some(first) = matches.first() else {
        return false;
    };
    let mut composer = super::new_composer();
    composer.insert_str(format!("/{} ", first.slash()));
    app.composer = composer;
    true
}

/// Runs a command picked from the slash popup.
/// 执行从斜杠弹窗中选中的命令。
pub fn run_slash(app: &mut App, command: AppCommand) {
    app.reset_composer();
    execute(app, command, None);
}

fn submit(app: &mut App) {
    let text = app.composer_prompt();
    let prompt = text.trim();
    if prompt.is_empty() {
        app.toast(NoticeSeverity::Warning, tr!("composer.error.promptEmpty"));
        return;
    }
    if let Some(rest) = prompt.strip_prefix('/') {
        let mut parts = rest.split_whitespace();
        let name = parts.next().unwrap_or_default().to_string();
        let arg = parts.next().map(str::to_string);
        match AppCommand::from_slash(&name) {
            Some(command) => {
                app.reset_composer();
                execute(app, command, arg.as_deref());
            }
            None => app.toast(
                NoticeSeverity::Warning,
                tr!("tui.command.unknown", name = name),
            ),
        }
        return;
    }
    let Some(conversation_id) = app.selected.clone() else {
        app.toast(NoticeSeverity::Warning, tr!("sidebar.waitingWorkspace"));
        return;
    };
    app.backend.send(Command::SubmitConversationPrompt {
        conversation_id,
        prompt: prompt.to_string(),
        mode: app.mode(),
        effort: app.effort,
        model_id: app.model_id.clone(),
        attachments: std::mem::take(&mut app.attachments),
    });
    app.reset_composer();
    if let Some(session) = app.session_mut() {
        session.scroll_from_bottom = 0;
        session.selected = None;
    }
}

// ---- Commands ----

pub fn execute(app: &mut App, command: AppCommand, arg: Option<&str>) {
    match command {
        AppCommand::NewConversation => {
            let project_id = app.conversation().and_then(|c| c.project_id.clone());
            action::perform(app, UiAction::NewConversation(project_id));
        }
        AppCommand::TogglePlan => {
            if let Some(id) = app.selected.clone() {
                let enabled = app.mode() == ExecutionMode::Build;
                if let Some(session) = app.session_mut() {
                    session.mode = if enabled {
                        ExecutionMode::Plan
                    } else {
                        ExecutionMode::Build
                    };
                }
                app.backend.send(Command::SetSessionPlanMode {
                    conversation_id: id,
                    enabled,
                });
            }
        }
        AppCommand::ToggleHtmlAnswers => super::html_answers::toggle(app),
        AppCommand::HtmlResults => {
            let seq = app.session().and_then(|session| {
                session
                    .selected
                    .filter(|seq| {
                        session
                            .html_answers
                            .projections
                            .get(seq)
                            .is_some_and(|projection| !projection.answers.is_empty())
                    })
                    .or_else(|| {
                        session
                            .html_answers
                            .projections
                            .iter()
                            .rev()
                            .find(|(_, projection)| !projection.answers.is_empty())
                            .map(|(seq, _)| *seq)
                    })
            });
            if let Some(seq) = seq {
                super::html_answers::inspect(app, seq);
            } else {
                app.toast(NoticeSeverity::Info, tr!("htmlAnswers.unavailable"));
            }
        }
        AppCommand::SelectModel => {
            let items = app
                .models
                .iter()
                .map(|m| PickerItem {
                    label: m.label.clone(),
                    detail: m.provider.clone(),
                    hint: crate::ui::text::compact_count(m.context_window),
                    current: m.id == app.model_id,
                    action: PickerAction::Model(m.id.clone()),
                })
                .collect();
            app.overlay = Some(Overlay::Picker(Picker::new(
                PickerKind::Model,
                tr!("composer.modelPicker.placeholder"),
                tr!("composer.modelPicker.filter"),
                items,
            )));
        }
        AppCommand::CycleReasoning => {
            let next = app
                .efforts
                .iter()
                .position(|e| *e == app.effort)
                .map_or(0, |at| (at + 1) % app.efforts.len());
            let Some(effort) = app.efforts.get(next).copied() else {
                return;
            };
            app.effort = effort;
            app.backend.send(Command::SelectWorkbenchReasoningEffort {
                conversation_id: app.selected.clone(),
                effort,
            });
            let label = crate::ui::effort_label(app.effort);
            app.toast(
                NoticeSeverity::Info,
                format!("{}: {label}", tr!("composer.reasoning.subtitle")),
            );
        }
        AppCommand::ToggleTheme => app.theme = app.theme.toggled(),
        AppCommand::Language => {
            if let Some(code) = arg {
                set_locale(app, code);
                return;
            }
            let active = i18n::active_locale();
            let items = i18n::LOCALES
                .iter()
                .map(|l| PickerItem {
                    label: l.native_name.to_string(),
                    detail: l.code.to_string(),
                    hint: l.mark.to_string(),
                    current: l.code == active,
                    action: PickerAction::Locale(l.code),
                })
                .collect();
            app.overlay = Some(Overlay::Picker(Picker::new(
                PickerKind::Language,
                tr!("global.language"),
                tr!("knowledge.search"),
                items,
            )));
        }
        // Outside the workbench the list is hidden, so the shortcut brings it back.
        // 工作台之外列表处于隐藏状态，因此该快捷键会将其带回。
        AppCommand::ToggleSidebar if app.area != Area::Chat => {
            action::perform(app, UiAction::OpenSidebar);
        }
        AppCommand::ToggleSidebar => {
            let mode = match app.sidebar {
                SidebarMode::Expanded => SidebarMode::Rail,
                SidebarMode::Rail => SidebarMode::Expanded,
            };
            action::perform(app, UiAction::SidebarMode(mode));
        }
        AppCommand::TogglePanel => {
            app.panel_visible = !app.panel_visible;
            if !app.panel_visible && app.focus == Focus::Panel {
                app.focus = Focus::Composer;
            }
        }
        AppCommand::ThinkingPlacement => {
            app.thinking_placement = match app.thinking_placement {
                ThinkingPlacement::Float => ThinkingPlacement::Docked,
                ThinkingPlacement::Docked => ThinkingPlacement::Float,
            };
        }
        AppCommand::OpenDiff => open_diff(app, app.git.cursor),
        AppCommand::Commit => open_commit(app),
        AppCommand::SwitchBranch | AppCommand::Push if app.git.repository().is_none() => {
            app.toast(NoticeSeverity::Info, tr!("gitRepository.unavailable"));
        }
        AppCommand::SwitchBranch => {
            app.backend.send(Command::LoadWorkspaceGitBranches);
            app.overlay = Some(Overlay::Picker(Picker::new(
                PickerKind::Branch,
                tr!("gitBranchMenu.label"),
                tr!("gitBranchMenu.search"),
                Vec::new(),
            )));
            refresh_branch_picker(app);
        }
        // `runPushAction`: pushes the current branch without a commit.
        // 推送当前分支且不创建提交。
        AppCommand::Push => app.backend.send(Command::RunGitCommitAction {
            message: String::new(),
            new_branch: None,
            include_unstaged: false,
            push: true,
        }),
        AppCommand::OpenWeb => {
            let url = app.base_url.clone();
            if app.backend.is_mock {
                app.toast(NoticeSeverity::Info, tr!("tui.web.mock", url = url));
            } else {
                app.toast(NoticeSeverity::Info, url);
            }
        }
        AppCommand::Help => app.overlay = Some(Overlay::Help { scroll: 0 }),
        AppCommand::Quit => app.should_quit = true,
        AppCommand::Chat => app.set_area(Area::Chat),
        AppCommand::Settings => app.set_area(Area::Settings),
        AppCommand::Memory => app.set_area(Area::Memory),
        AppCommand::Knowledge => app.set_area(Area::Knowledge),
        AppCommand::AgentsArea => app.set_area(Area::Agents),
        AppCommand::Skills => app.set_area(Area::AgentSkills),
        AppCommand::Plugins => app.set_area(Area::Plugins),
        AppCommand::ContextIndex => action::perform(app, UiAction::GoPage(PageId::IndexProjects)),
        AppCommand::AreaPicker => action::open_area_picker(app),
        AppCommand::History => action::open_history(app),
        AppCommand::Attach => {
            app.set_area(Area::Chat);
            action::perform(app, UiAction::Attach);
        }
        AppCommand::AddProject => action::perform(app, UiAction::AddProject),
        AppCommand::Palette => open_palette(app),
    }
}

/// Switches the UI language. An explicit choice is reported to the service so it survives
/// restarts; nothing else in the user's configuration is touched.
/// 切换界面语言。显式选择会上报服务以便重启后保留；不改动用户配置中的其他内容。
pub fn set_locale(app: &mut App, code: &str) {
    let resolved = i18n::LOCALES
        .iter()
        .find(|l| l.code == code)
        .map(|l| l.code)
        .or_else(|| i18n::match_locale_tag(code));
    match resolved {
        Some(code) => {
            i18n::set_active_locale(code);
            app.locale_explicit = true;
            app.composer
                .set_placeholder_text(tr!("composer.placeholder", shortcut = "Enter"));
            action::perform(
                app,
                UiAction::Send(Command::Call(
                    crate::service_api::settings_actions::select_webui_locale(code),
                )),
            );
        }
        None => app.toast(
            NoticeSeverity::Warning,
            tr!("tui.command.unknown", name = code),
        ),
    }
}

fn open_palette(app: &mut App) {
    let items = AppCommand::ALL
        .into_iter()
        .map(|c| PickerItem {
            label: c.label(),
            detail: format!("/{}", c.slash()),
            hint: c.shortcut().to_string(),
            current: false,
            action: PickerAction::Command(c),
        })
        .collect();
    let mut picker = Picker::new(
        PickerKind::Palette,
        tr!("tui.palette.title"),
        tr!("knowledge.search"),
        items,
    );
    picker.cursor = 0;
    app.overlay = Some(Overlay::Picker(picker));
}

pub fn refresh_branch_picker(app: &mut App) {
    let dirty = app.git.snapshot.as_ref().map_or(0, |s| s.files.len());
    let Some(Overlay::Picker(picker)) = &mut app.overlay else {
        return;
    };
    if picker.kind != PickerKind::Branch {
        return;
    }
    let mut items: Vec<PickerItem> = app
        .git
        .branches
        .iter()
        .map(|b| PickerItem {
            label: b.name.clone(),
            detail: b.last_commit.clone(),
            hint: if b.remote {
                "remote".into()
            } else {
                String::new()
            },
            current: b.current,
            action: PickerAction::Branch(b.name.clone()),
        })
        .collect();
    items.push(PickerItem {
        label: tr!("gitBranchMenu.create"),
        detail: if dirty > 0 {
            tr!("gitBranchMenu.uncommitted", count = dirty)
        } else {
            String::new()
        },
        hint: String::new(),
        current: false,
        action: PickerAction::CreateBranch,
    });
    let had_items = !picker.items.is_empty();
    picker.items = items;
    if !had_items {
        picker.cursor = picker.items.iter().position(|i| i.current).unwrap_or(0);
    }
    picker.clamp();
}

// ---- Git ----

pub fn open_diff(app: &mut App, index: usize) {
    if app.git.ordered_files().is_empty() {
        app.toast(NoticeSeverity::Info, tr!("diff.emptyBody"));
        return;
    }
    app.overlay = Some(Overlay::Diff(DiffView {
        file_index: index.min(app.git.ordered_files().len() - 1),
        scroll: 0,
    }));
    app.request_visible_diff();
}

/// Moves the diff viewer to the next (`1`) or previous (`-1`) changed file.
/// 将 diff 查看器移到下一个（`1`）或上一个（`-1`）变更文件。
pub fn diff_step(app: &mut App, delta: isize) {
    let count = app.git.ordered_files().len() as isize;
    let Some(Overlay::Diff(view)) = &mut app.overlay else {
        return;
    };
    if count == 0 {
        return;
    }
    view.file_index = (view.file_index as isize + delta).rem_euclid(count) as usize;
    view.scroll = 0;
    app.git.cursor = view.file_index;
    app.request_visible_diff();
}

pub fn open_commit(app: &mut App) {
    let has_changes = app
        .git
        .snapshot
        .as_ref()
        .is_some_and(|s| !s.files.is_empty());
    if !has_changes {
        app.toast(NoticeSeverity::Info, tr!("gitCommitDialog.noChanges"));
        return;
    }
    let mut message = TextArea::default();
    message.set_placeholder_text(tr!("gitCommitDialog.messagePlaceholder"));
    let has_staged = app
        .git
        .snapshot
        .as_ref()
        .is_some_and(|s| s.files.iter().any(|f| f.staged));
    app.overlay = Some(Overlay::Commit(Box::new(CommitDialog {
        new_branch: false,
        branch_name: String::new(),
        message,
        include_unstaged: !has_staged,
        field: CommitField::Message,
        error: None,
        commit_after_generate: None,
    })));
}

/// Sends the commit from the open dialog, generating a message first when it is empty.
/// 从打开的对话框发送提交；信息为空时先生成信息。
pub fn submit_commit(app: &mut App, push: bool) {
    let Some(Overlay::Commit(dialog)) = &mut app.overlay else {
        return;
    };
    if app.git.busy.is_some() {
        return;
    }
    if dialog.new_branch && dialog.branch_name.trim().is_empty() {
        dialog.error = Some(tr!("tui.commit.branchRequired"));
        dialog.field = CommitField::BranchName;
        return;
    }
    let message = dialog.message.lines().join("\n").trim().to_string();
    if message.is_empty() {
        dialog.commit_after_generate = Some(push);
        app.backend.send(Command::GenerateGitCommitMessage);
        return;
    }
    let command = Command::RunGitCommitAction {
        message,
        new_branch: dialog
            .new_branch
            .then(|| dialog.branch_name.trim().to_string()),
        include_unstaged: dialog.include_unstaged,
        push,
    };
    dialog.error = None;
    app.backend.send(command);
}

/// Replaces the commit message with a freshly generated one.
/// 用新生成的提交信息替换当前信息。
pub fn generate_commit_message(app: &mut App) {
    if app.git.busy.is_some() {
        return;
    }
    if let Some(Overlay::Commit(dialog)) = &mut app.overlay {
        dialog.message = TextArea::default();
        app.backend.send(Command::GenerateGitCommitMessage);
    }
}

pub fn toggle_stage(app: &mut App, index: usize) {
    if let Some(file) = app.git.ordered_files().get(index) {
        let command = Command::RunWorkspaceGitFileAction {
            paths: vec![file.path.clone()],
            stage: !file.staged,
        };
        app.backend.send(command);
    }
}

pub fn stage_all(app: &mut App, stage: bool) {
    let paths: Vec<String> = app
        .git
        .ordered_files()
        .iter()
        .filter(|f| f.staged != stage)
        .map(|f| f.path.clone())
        .collect();
    if !paths.is_empty() {
        app.backend
            .send(Command::RunWorkspaceGitFileAction { paths, stage });
    }
}
