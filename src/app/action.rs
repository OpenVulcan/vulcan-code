// UI actions: one vocabulary shared by buttons, menus, page rows, dialogs and mouse clicks.
// 界面动作：按钮、菜单、页面行、对话框与鼠标点击共用的一套词汇。
//
// Views attach a `UiAction` to whatever can be activated; keyboard and mouse input both end up in
// `perform`. Keeping the effect in one place means a click and its keyboard equivalent can never
// drift apart.
// 视图为每个可激活元素附加一个 `UiAction`；键盘与鼠标输入最终都进入 `perform`。效果集中在一处，
// 点击与其键盘等价操作就不会产生分歧。

use super::commands::AppCommand;
use super::input;
use super::overlay::{
    DetailSheet, FolderMode, FolderPicker, History, Inspector, Overlay, Picker, PickerAction,
    PickerItem, PickerKind,
};
use super::pages::{Area, PageId};
use super::{App, Focus, PanelTab, SidebarMode, SidebarRow, ThinkingPlacement};
use crate::backend::Command;
use crate::i18n;
use crate::model::*;
use crate::tr;
use crate::ui::theme::{Theme, ThemeMode};

/// Settings that live only in this TUI process and are never written to the service.
/// 只存在于本 TUI 进程、从不写回服务的设置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalSetting {
    /// The explicit language choice is the one local setting that is also reported to the
    /// service (`SetUiLocale`), because the user picked it on purpose.
    /// 显式选择语言是唯一会同时上报服务（`SetUiLocale`）的本地设置，因为这是用户的主动选择。
    Locale,
    Theme,
    Thinking,
    Sidebar,
    Panel,
    /// The knowledge library shown on the library details page.
    /// 知识库详情页显示的知识库。
    Library,
}

/// Where an edited value goes.
/// 编辑后的值写往何处。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    /// Saved immediately (`SetSetting`).
    /// 立即保存。
    Setting(String),
    /// Held as an unsaved draft until the page is saved.
    /// 作为未保存草稿保留，直到页面保存。
    Draft(String),
    Enabled(EntityKind, String),
    /// Model route; an empty value turns the route off.
    /// 模型路由；空值表示关闭。
    Route(EntityKind, String),
    /// One field of a catalog entity (`SetField`), saved immediately.
    /// 目录实体的某个字段（`SetField`），立即保存。
    Field(EntityKind, String, String),
    /// A secret setting: sent once, never kept in drafts or in the local snapshot.
    /// 密钥类设置：只发送一次，从不保存在草稿或本地快照中。
    Secret(String),
    Local(LocalSetting),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskStep {
    Prev,
    Next,
    Submit,
    Reject,
    RejectConfirm,
    RejectCancel,
    EditOther,
}

#[derive(Debug, Clone)]
pub struct ConfirmDialog {
    pub title: String,
    pub body: String,
    pub confirm_label: String,
    pub danger: bool,
    pub then: UiAction,
    /// Keyboard focus sits on the confirm button (else on cancel, the safe default).
    /// 键盘焦点位于确认按钮（否则位于取消，即安全默认）。
    pub focus_confirm: bool,
}

#[derive(Debug, Clone)]
pub enum PromptThen {
    RenameProject(String),
    RenameConversation(ConversationId),
    AddCredential(String),
    Edit(Edit),
    MemorySearch,
    MemoryCorrect,
    KnowledgeSearch,
    SetGoal(ConversationId),
    /// Types a message for a subagent; `steering` interrupts its current step.
    /// 为子代理输入消息；`steering` 会中断其当前步骤。
    AgentMessage {
        conversation_id: ConversationId,
        agent_id: String,
        steering: bool,
    },
    /// A typed number, validated against the row's range before it is applied.
    /// 输入的数字，在应用前按该行的范围校验。
    Number {
        edit: Edit,
        min: i64,
        max: i64,
    },
    KnowledgeCreate,
    MemoryProjectCreate,
    /// Moves every item of the memory project with this id into the typed project.
    /// 将此 ID 的记忆项目中的全部条目迁移到输入的项目。
    MemoryMigrate(String),
    LuaInstall,
}

impl PromptThen {
    /// Search prompts accept an empty value, which clears the search.
    /// 搜索输入框接受空值，表示清除搜索。
    fn allows_empty(&self) -> bool {
        matches!(
            self,
            PromptThen::MemorySearch | PromptThen::KnowledgeSearch | PromptThen::Edit(_)
        )
    }
}

/// Single-line text input dialog.
/// 单行文本输入对话框。
#[derive(Debug, Clone)]
pub struct PromptDialog {
    pub title: String,
    pub description: String,
    pub input: String,
    /// Masks the input on screen; the value is still sent once, never stored by the TUI.
    /// 在屏幕上遮蔽输入；值只发送一次，TUI 从不保存。
    pub secret: bool,
    pub then: PromptThen,
    pub error: Option<String>,
}

impl PromptDialog {
    pub fn new(
        title: String,
        description: String,
        input: String,
        then: PromptThen,
        secret: bool,
    ) -> Self {
        PromptDialog {
            title,
            description,
            input,
            secret,
            then,
            error: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum UiAction {
    Send(Command),
    App(AppCommand),
    Confirm(Box<ConfirmDialog>),
    ConfirmAccept,
    Prompt(Box<PromptDialog>),
    PromptAccept,
    Pick {
        title: String,
        edit: Edit,
        options: Vec<(String, String)>,
        current: String,
    },
    Edit(Edit, SettingValue),
    /// Saves the drafts whose keys start with one of these prefixes.
    /// 保存键以这些前缀之一开头的草稿。
    SaveDrafts(Vec<&'static str>),
    DiscardDrafts(Vec<&'static str>),
    /// Selects the knowledge library shown on the library details page.
    /// 选择知识库详情页显示的知识库。
    OpenLibrary(String),
    /// Opens the folder picker to add a source directory to this library.
    /// 打开文件夹选择器，为此知识库添加来源目录。
    AddKnowledgeSource(String),
    /// Opens the folder picker to install a local plugin directory.
    /// 打开文件夹选择器以安装本地插件目录。
    InstallPlugin,
    Area(Area),
    GoPage(PageId),
    OpenAreaPicker,
    SelectConversation(ConversationId),
    NewConversation(Option<String>),
    ToggleProject(String),
    ProjectMenu(String),
    ConversationMenu(ConversationId),
    AddProject,
    Attach,
    /// Opens the file browser at a directory (attaching whatever is picked).
    /// 在某个目录打开文件浏览器（选中的文件将被附加）。
    Browse(String),
    RemoveAttachment(usize),
    FolderGo(String),
    FolderUp,
    FolderChoose,
    FolderToggleHidden,
    FolderType,
    /// Project mode: add the directory being listed.
    /// 项目模式：添加当前所列的目录。
    FolderUseCurrent,
    Inspect(u64),
    OpenHistory,
    OpenLanguagePicker,
    OpenDiff(usize),
    ToggleStage(usize),
    StageAll(bool),
    DiffStep(isize),
    CommitSubmit(bool),
    CommitGenerate,
    Ask(AskStep),
    /// Opens a detail sheet.
    /// 打开详情面板。
    Detail(Box<DetailSheet>),
    /// Runs one detail-sheet button; the sheet closes first.
    /// 执行详情面板的一个按钮；面板先关闭。
    SheetPick(usize),
    /// Selects a row of a scrolling panel tab.
    /// 选中可滚动面板标签页的一行。
    PanelSelect(PanelTab, usize),
    /// Opens the details of a panel row (memory item, profile node, agent).
    /// 打开面板行（记忆条目、画像节点、代理）的详情。
    PanelOpen(PanelTab, usize),
    /// Memory tab: static profile (`true`) or dynamic injections.
    /// 记忆页：静态画像（`true`）或动态注入。
    MemoryView(bool),
    /// Shows the session panel on this tab and focuses it (a drawer on narrow terminals).
    /// 在此标签页显示会话面板并聚焦（窄终端上为抽屉）。
    ShowPanel(PanelTab),
    SidebarMode(SidebarMode),
    /// Expands the sidebar list as a drawer and focuses it (the rail's expand button).
    /// 以抽屉形式展开侧栏列表并聚焦（窄栏的展开按钮）。
    OpenSidebar,
    /// A project initial on the rail: opens the list on that project.
    /// 窄栏上的项目首字母：打开列表并定位到该项目。
    RailProject(String),
    /// Folds every project when any is open, else unfolds them all.
    /// 有任何项目展开时折叠全部，否则全部展开。
    FoldAllProjects,
    /// Previews a collapsed paste; `None` picks the one next to the cursor, else the latest.
    /// 预览折叠的粘贴；`None` 选择光标旁的粘贴，否则选择最近一次。
    PastePreview(Option<usize>),
    /// Replaces a paste token with its full text.
    /// 用完整文本替换粘贴标记。
    PasteExpand(usize),
    PasteRemove(usize),
    Close,
}

/// Opens a confirmation dialog with keyboard focus on cancel.
/// 打开确认对话框，键盘焦点位于取消。
pub fn confirm(title: String, body: String, confirm_label: String, then: UiAction) -> UiAction {
    UiAction::Confirm(Box::new(ConfirmDialog {
        title,
        body,
        confirm_label,
        danger: true,
        then,
        focus_confirm: false,
    }))
}

pub fn prompt(title: String, description: String, input: String, then: PromptThen) -> UiAction {
    UiAction::Prompt(Box::new(PromptDialog::new(
        title,
        description,
        input,
        then,
        false,
    )))
}

pub fn perform(app: &mut App, action: UiAction) {
    match action {
        UiAction::Send(command) => app.backend.send(command),
        UiAction::App(command) => input::execute(app, command, None),
        UiAction::Confirm(dialog) => app.overlay = Some(Overlay::Confirm(dialog)),
        UiAction::ConfirmAccept => {
            if let Some(Overlay::Confirm(dialog)) = app.overlay.take() {
                perform(app, dialog.then);
            }
        }
        UiAction::Prompt(dialog) => app.overlay = Some(Overlay::Prompt(dialog)),
        UiAction::PromptAccept => accept_prompt(app),
        UiAction::Pick {
            title,
            edit,
            options,
            current,
        } => open_choice(app, title, edit, options, current),
        UiAction::Edit(edit, value) => apply_edit(app, edit, value),
        UiAction::SaveDrafts(prefixes) => {
            let keys = draft_keys(app, &prefixes);
            let values: Vec<(String, SettingValue)> = keys
                .into_iter()
                .filter_map(|key| app.drafts.remove_entry(&key))
                .collect();
            if values.is_empty() {
                return;
            }
            if let Some(system) = app.system.as_mut() {
                system.settings.extend(values.iter().cloned());
            }
            app.backend
                .send(Command::System(SystemCommand::SaveSettings(values)));
        }
        UiAction::DiscardDrafts(prefixes) => {
            for key in draft_keys(app, &prefixes) {
                app.drafts.remove(&key);
            }
        }
        UiAction::OpenLibrary(id) => {
            app.library = Some(id);
            perform(app, UiAction::GoPage(PageId::KnowledgeLibrary));
        }
        UiAction::AddKnowledgeSource(library) => {
            app.library = Some(library);
            open_folder(app, FolderMode::KnowledgeSource, None);
        }
        UiAction::InstallPlugin => open_folder(app, FolderMode::Plugin, None),
        UiAction::Area(area) => app.set_area(area),
        UiAction::GoPage(page) => {
            app.set_area(page.area());
            let index = page
                .area()
                .pages()
                .iter()
                .position(|p| *p == page)
                .unwrap_or(0);
            app.nav.insert(page.area(), index);
            app.focus = Focus::AreaPage;
        }
        UiAction::OpenAreaPicker => open_area_picker(app),
        UiAction::SelectConversation(id) => {
            app.overlay = None;
            app.set_area(Area::Chat);
            app.select_conversation(id);
            app.focus = Focus::Composer;
        }
        UiAction::NewConversation(project_id) => {
            app.overlay = None;
            app.set_area(Area::Chat);
            app.backend.send(Command::NewConversation { project_id });
            app.focus = Focus::Composer;
        }
        UiAction::ToggleProject(id) => {
            if !app.collapsed_projects.remove(&id) {
                app.collapsed_projects.insert(id);
            }
        }
        UiAction::ProjectMenu(id) => {
            let anchor = app.menu_anchor();
            app.overlay = super::menus::project_menu(app, &id).map(|mut menu| {
                menu.anchor = anchor;
                Overlay::Menu(menu)
            });
        }
        UiAction::ConversationMenu(id) => {
            let anchor = app.menu_anchor();
            app.overlay = super::menus::conversation_menu(app, &id).map(|mut menu| {
                menu.anchor = anchor;
                Overlay::Menu(menu)
            });
        }
        UiAction::AddProject => open_folder(app, FolderMode::Project, None),
        UiAction::Attach => {
            let start = app
                .conversation()
                .and_then(|c| app.project_of(c))
                .map(|p| p.path.clone());
            open_folder(app, FolderMode::Attachment, start);
        }
        UiAction::Browse(path) => open_folder(app, FolderMode::Attachment, Some(path)),
        UiAction::RemoveAttachment(index) => {
            if index < app.attachments.len() {
                app.attachments.remove(index);
            }
        }
        UiAction::FolderGo(path) => folder_go(app, Some(path)),
        UiAction::FolderUp => {
            let parent = match &app.overlay {
                Some(Overlay::Folder(picker)) => {
                    picker.listing.as_ref().and_then(|l| l.parent.clone())
                }
                _ => None,
            };
            if let Some(parent) = parent {
                folder_go(app, Some(parent));
            }
        }
        UiAction::FolderChoose => folder_choose(app),
        UiAction::FolderToggleHidden => {
            let path = match &mut app.overlay {
                Some(Overlay::Folder(picker)) => {
                    picker.show_hidden = !picker.show_hidden;
                    picker.listing.as_ref().map(|l| l.path.clone())
                }
                _ => return,
            };
            folder_go(app, path);
        }
        UiAction::FolderType => {
            if let Some(Overlay::Folder(picker)) = &mut app.overlay {
                picker.typing = Some(
                    picker
                        .listing
                        .as_ref()
                        .map(|l| l.path.clone())
                        .unwrap_or_default(),
                );
            }
        }
        UiAction::FolderUseCurrent => folder_use_current(app),
        UiAction::Inspect(seq) => {
            let back = match app.overlay.take() {
                Some(Overlay::History(history)) => Some(Box::new(history)),
                _ => None,
            };
            app.overlay = Some(Overlay::Inspector(Inspector {
                seq,
                scroll: 0,
                back,
            }));
        }
        UiAction::OpenHistory => open_history(app),
        UiAction::OpenLanguagePicker => input::execute(app, AppCommand::Language, None),
        UiAction::OpenDiff(index) => {
            app.git.cursor = index;
            input::open_diff(app, index);
        }
        UiAction::ToggleStage(index) => input::toggle_stage(app, index),
        UiAction::StageAll(stage) => input::stage_all(app, stage),
        UiAction::DiffStep(delta) => input::diff_step(app, delta),
        UiAction::CommitSubmit(push) => input::submit_commit(app, push),
        UiAction::CommitGenerate => input::generate_commit_message(app),
        UiAction::Ask(step) => input::ask_step(app, step),
        UiAction::Detail(sheet) => app.overlay = Some(Overlay::Detail(sheet)),
        UiAction::SheetPick(index) => {
            if let Some(Overlay::Detail(sheet)) = app.overlay.take()
                && let Some(item) = sheet.actions.into_iter().nth(index)
            {
                perform(app, item.action);
            }
        }
        UiAction::PanelSelect(tab, index) => {
            app.panel_tab = tab;
            app.focus = Focus::Panel;
            app.panel.free_scroll = false;
            app.panel.cursor.insert(tab, index);
        }
        UiAction::PanelOpen(tab, index) => {
            perform(app, UiAction::PanelSelect(tab, index));
            if let Some(sheet) = super::insight::sheet_for(app, tab, index) {
                app.overlay = Some(Overlay::Detail(Box::new(sheet)));
            }
        }
        UiAction::MemoryView(static_view) => {
            if app.panel.memory_static != static_view {
                app.panel.memory_static = static_view;
                app.panel.cursor.insert(PanelTab::Memory, 0);
                app.panel.scroll.insert(PanelTab::Memory, 0);
            }
        }
        UiAction::ShowPanel(tab) => {
            app.set_area(Area::Chat);
            app.panel_visible = true;
            app.panel_tab = tab;
            app.focus = Focus::Panel;
        }
        UiAction::SidebarMode(mode) => {
            app.sidebar = mode;
            if mode == SidebarMode::Rail && app.focus == Focus::Sidebar {
                app.set_area(app.area);
            }
        }
        UiAction::OpenSidebar => {
            app.overlay = None;
            app.sidebar = SidebarMode::Expanded;
            app.sidebar_free_scroll = false;
            app.focus = Focus::Sidebar;
        }
        UiAction::RailProject(id) => {
            app.overlay = None;
            app.collapsed_projects.remove(&id);
            app.sidebar_cursor = Some(SidebarRow::Project(id));
            app.sidebar_free_scroll = false;
            app.focus = Focus::Sidebar;
        }
        UiAction::FoldAllProjects => fold_all_projects(app),
        UiAction::PastePreview(index) => {
            let index = index.or_else(|| app.paste_near_cursor());
            if let Some(sheet) = index.and_then(|i| paste_sheet(app, i)) {
                app.overlay = Some(Overlay::Detail(Box::new(sheet)));
            }
        }
        UiAction::PasteExpand(index) => {
            app.expand_paste(index);
            app.focus = Focus::Composer;
        }
        UiAction::PasteRemove(index) => {
            app.remove_paste(index);
            app.focus = Focus::Composer;
        }
        UiAction::Close => app.close_overlay(),
    }
}

/// Folds every project when any is open, else unfolds them all. A cursor left on a hidden
/// conversation moves to its project.
/// 有任何项目展开时折叠全部，否则全部展开。停留在被隐藏会话上的光标移到其项目。
fn fold_all_projects(app: &mut App) {
    let any_open = app
        .projects
        .iter()
        .any(|p| !app.collapsed_projects.contains(&p.id));
    if any_open {
        app.collapsed_projects = app.projects.iter().map(|p| p.id.clone()).collect();
    } else {
        app.collapsed_projects.clear();
    }
    let hidden_project = match &app.sidebar_cursor {
        Some(SidebarRow::Conversation(id)) => app
            .conversations
            .iter()
            .find(|c| &c.id == id && !c.pinned)
            .and_then(|c| c.project_id.clone())
            .filter(|p| app.collapsed_projects.contains(p)),
        _ => None,
    };
    if let Some(project) = hidden_project {
        app.sidebar_cursor = Some(SidebarRow::Project(project));
    }
    app.sidebar_free_scroll = false;
    app.fix_sidebar_cursor();
}

/// Lines of a paste shown in its preview sheet; the rest is summarized.
/// 粘贴预览面板中显示的行数；其余部分以摘要表示。
const PASTE_PREVIEW_LINES: usize = 200;

fn paste_sheet(app: &App, index: usize) -> Option<DetailSheet> {
    let paste = app.pastes.get(index)?;
    let lines: Vec<&str> = paste.text.lines().collect();
    let mut body = lines
        .iter()
        .take(PASTE_PREVIEW_LINES)
        .copied()
        .collect::<Vec<_>>()
        .join("\n");
    if lines.len() > PASTE_PREVIEW_LINES {
        body.push_str(&format!(
            "\n… {}",
            tr!(
                "tui.composer.moreLines",
                count = lines.len() - PASTE_PREVIEW_LINES
            )
        ));
    }
    Some(
        DetailSheet::new(tr!("tui.composer.pasteTitle", index = paste.serial))
            .field(tr!("tui.composer.lines"), paste.lines().to_string())
            .field(tr!("tui.composer.chars"), paste.chars().to_string())
            .section(tr!("tui.composer.pasteContent"), body)
            .action(
                tr!("tui.composer.expandPaste"),
                false,
                UiAction::PasteExpand(index),
            )
            .action(
                tr!("tui.composer.removePaste"),
                true,
                UiAction::PasteRemove(index),
            ),
    )
}

/// Draft keys matching any of the prefixes.
/// 匹配任一前缀的草稿键。
pub fn draft_keys(app: &App, prefixes: &[&str]) -> Vec<String> {
    app.drafts
        .keys()
        .filter(|key| prefixes.iter().any(|p| key.starts_with(p)))
        .cloned()
        .collect()
}

/// Applies one edited value to wherever `edit` points.
/// 将编辑后的值写入 `edit` 指向的位置。
pub fn apply_edit(app: &mut App, edit: Edit, value: SettingValue) {
    match edit {
        Edit::Setting(key) => {
            if let Some(system) = app.system.as_mut() {
                system.settings.insert(key.clone(), value.clone());
            }
            app.backend
                .send(Command::System(SystemCommand::SetSetting { key, value }));
        }
        Edit::Draft(key) => {
            let stored = app.system.as_ref().and_then(|s| s.setting(&key)).cloned();
            if stored.as_ref() == Some(&value) {
                app.drafts.remove(&key);
            } else {
                app.drafts.insert(key, value);
            }
        }
        Edit::Enabled(kind, id) => {
            app.backend.send(Command::System(SystemCommand::SetEnabled {
                kind,
                id,
                enabled: value.as_bool(),
            }));
        }
        Edit::Route(kind, id) => {
            let model = value.as_str().to_string();
            app.backend.send(Command::System(SystemCommand::SetRoute {
                kind,
                id,
                model: (!model.is_empty()).then_some(model),
            }));
        }
        Edit::Field(kind, id, field) => {
            app.backend.send(Command::System(SystemCommand::SetField {
                kind,
                id,
                field,
                value,
            }));
        }
        Edit::Secret(key) => {
            // Blank means "keep the stored secret"; nothing is echoed back locally.
            // 留空表示“保留已存储的密钥”；本地不回显任何内容。
            if !value.as_str().is_empty() {
                app.backend
                    .send(Command::System(SystemCommand::SetSetting { key, value }));
            }
        }
        Edit::Local(setting) => match setting {
            LocalSetting::Locale => input::set_locale(app, value.as_str()),
            LocalSetting::Theme => {
                app.theme = match value.as_str() {
                    "light" => Theme::light(),
                    _ => Theme::dark(),
                };
            }
            LocalSetting::Thinking => {
                app.thinking_placement = match value.as_str() {
                    "docked" => ThinkingPlacement::Docked,
                    _ => ThinkingPlacement::Float,
                };
            }
            LocalSetting::Sidebar => {
                let mode = if value.as_bool() {
                    SidebarMode::Expanded
                } else {
                    SidebarMode::Rail
                };
                perform(app, UiAction::SidebarMode(mode));
            }
            LocalSetting::Panel => app.panel_visible = value.as_bool(),
            LocalSetting::Library => app.library = Some(value.as_str().to_string()),
        },
    }
}

/// Current theme as a choice id.
/// 当前主题对应的选项 ID。
pub fn theme_id(app: &App) -> &'static str {
    match app.theme.mode {
        ThemeMode::Dark => "dark",
        ThemeMode::Light => "light",
    }
}

fn open_choice(
    app: &mut App,
    title: String,
    edit: Edit,
    options: Vec<(String, String)>,
    current: String,
) {
    let items = options
        .into_iter()
        .map(|(id, label)| PickerItem {
            detail: String::new(),
            hint: String::new(),
            current: id == current,
            action: PickerAction::Choice(edit.clone(), id),
            label,
        })
        .collect();
    app.overlay = Some(Overlay::Picker(Picker::new(
        PickerKind::Choice,
        title,
        tr!("knowledge.search"),
        items,
    )));
}

pub fn open_area_picker(app: &mut App) {
    let items = Area::ALL
        .into_iter()
        .map(|area| PickerItem {
            label: area.label(),
            detail: area.description(),
            hint: format!("/{}", super::pages::area_command(area).slash()),
            current: area == app.area,
            action: PickerAction::Area(area),
        })
        .collect();
    app.overlay = Some(Overlay::Picker(Picker::new(
        PickerKind::Area,
        tr!("tui.area.switch"),
        tr!("knowledge.search"),
        items,
    )));
}

pub fn open_history(app: &mut App) {
    if app.session().is_none() {
        app.toast(Severity::Info, tr!("sidebar.waitingWorkspace"));
        return;
    }
    app.overlay = Some(Overlay::History(History::default()));
}

fn accept_prompt(app: &mut App) {
    let Some(Overlay::Prompt(dialog)) = &mut app.overlay else {
        return;
    };
    let value = dialog.input.trim().to_string();
    if value.is_empty() && !dialog.then.allows_empty() {
        dialog.error = Some(tr!("tui.prompt.required"));
        return;
    }
    let Some(Overlay::Prompt(mut dialog)) = app.overlay.take() else {
        return;
    };
    let system = |command: SystemCommand| Command::System(command);
    // The continuation is taken out so the dialog stays reusable for re-prompting.
    // 取出后续动作，使对话框可在重新提示时复用。
    let then = std::mem::replace(&mut dialog.then, PromptThen::KnowledgeCreate);
    match then {
        PromptThen::RenameProject(project_id) => app.backend.send(Command::RenameProject {
            project_id,
            name: value,
        }),
        PromptThen::RenameConversation(conversation_id) => {
            app.backend.send(Command::RenameConversation {
                conversation_id,
                title: value,
            })
        }
        PromptThen::AddCredential(provider) => {
            app.backend.send(system(SystemCommand::Credential {
                provider,
                op: CredentialOp::Add {
                    label: String::new(),
                    secret: value,
                },
            }))
        }
        PromptThen::Edit(edit) => apply_edit(app, edit, SettingValue::Text(value)),
        PromptThen::Number { edit, min, max } => match value.replace([',', '_'], "").parse::<i64>()
        {
            Ok(number) if (min..=max).contains(&number) => {
                apply_edit(app, edit, SettingValue::Number(number))
            }
            _ => {
                // Keep the dialog open with the typed text so the user can correct it.
                // 保留对话框与已输入内容，便于用户修正。
                dialog.error = Some(tr!("tui.prompt.range", min = min, max = max));
                dialog.then = PromptThen::Number { edit, min, max };
                app.overlay = Some(Overlay::Prompt(dialog));
            }
        },
        PromptThen::KnowledgeCreate => app
            .backend
            .send(system(SystemCommand::KnowledgeCreate(value))),
        PromptThen::MemoryProjectCreate => app.backend.send(system(SystemCommand::MemoryProject(
            MemoryProjectOp::Create(value),
        ))),
        PromptThen::MemoryMigrate(from) => app.backend.send(system(SystemCommand::MemoryProject(
            MemoryProjectOp::Migrate { from, to: value },
        ))),
        PromptThen::LuaInstall => app
            .backend
            .send(system(SystemCommand::LuaSkill(CatalogOp::Install(value)))),
        PromptThen::MemorySearch => app.backend.send(system(SystemCommand::MemorySearch(value))),
        PromptThen::MemoryCorrect => app
            .backend
            .send(system(SystemCommand::MemoryCorrectProfile(value))),
        PromptThen::KnowledgeSearch => {
            app.backend
                .send(system(SystemCommand::KnowledgeSearch(value)));
            if app.area == Area::Knowledge {
                perform(app, UiAction::GoPage(PageId::KnowledgeSearch));
            }
        }
        PromptThen::SetGoal(conversation_id) => app.backend.send(Command::Goal {
            conversation_id,
            op: GoalOp::Set {
                objective: value,
                budget: None,
            },
        }),
        PromptThen::AgentMessage {
            conversation_id,
            agent_id,
            steering,
        } => app.backend.send(Command::AgentMessage {
            conversation_id,
            agent_id,
            message: AgentMessage {
                steering,
                text: value,
            },
        }),
    }
}

fn open_folder(app: &mut App, mode: FolderMode, start: Option<String>) {
    app.overlay = Some(Overlay::Folder(Box::new(FolderPicker {
        mode,
        listing: None,
        cursor: 0,
        scroll: 0,
        show_hidden: false,
        typing: None,
        loading: true,
    })));
    folder_go(app, start);
}

fn folder_go(app: &mut App, path: Option<String>) {
    let Some(Overlay::Folder(picker)) = &mut app.overlay else {
        return;
    };
    picker.loading = true;
    picker.typing = None;
    let command = Command::ListDirectory {
        path,
        show_hidden: picker.show_hidden,
        files: !picker.mode.picks_directory(),
    };
    app.backend.send(command);
}

/// Enter in the folder picker: open a directory, attach a file, or pick the current directory.
/// 文件夹选择器中的回车：进入目录、附加文件，或选择当前目录。
fn folder_choose(app: &mut App) {
    let Some(Overlay::Folder(picker)) = &mut app.overlay else {
        return;
    };
    if let Some(typed) = picker.typing.take() {
        folder_go(app, Some(typed));
        return;
    }
    let Some(listing) = picker.listing.as_ref() else {
        return;
    };
    let entry = listing.entries.get(picker.cursor);
    match (picker.mode, entry) {
        (_, Some(entry))
            if entry.is_dir && !(picker.mode == FolderMode::Project && entry.is_repo) =>
        {
            let path = listing.child(&entry.name);
            folder_go(app, Some(path));
        }
        (FolderMode::Attachment, Some(entry)) => {
            let path = listing.child(&entry.name);
            app.overlay = None;
            if !app.attachments.contains(&path) {
                app.attachments.push(path);
            }
            app.focus = Focus::Composer;
        }
        (FolderMode::Attachment, None) => {}
        (mode, entry) => {
            let path = entry.map_or_else(|| listing.path.clone(), |e| listing.child(&e.name));
            use_directory(app, mode, path);
        }
    }
}

/// Finishes a directory-mode picker with the chosen directory.
/// 以选中的目录结束目录模式的选择器。
fn use_directory(app: &mut App, mode: FolderMode, path: String) {
    app.overlay = None;
    let command = match mode {
        FolderMode::Project => Command::AddProject { path },
        FolderMode::KnowledgeSource => {
            let Some(library) = app.library.clone() else {
                return;
            };
            Command::System(SystemCommand::KnowledgeSource {
                library,
                op: SourceOp::Add(path),
            })
        }
        FolderMode::Plugin => Command::System(SystemCommand::Plugin(CatalogOp::Install(path))),
        FolderMode::Attachment => return,
    };
    app.backend.send(command);
}

/// Uses the directory currently listed (the "use this folder" button).
/// 使用当前列出的目录（“使用此文件夹”按钮）。
pub fn folder_use_current(app: &mut App) {
    let Some(Overlay::Folder(picker)) = &app.overlay else {
        return;
    };
    let mode = picker.mode;
    let Some(path) = picker.listing.as_ref().map(|l| l.path.clone()) else {
        return;
    };
    if mode.picks_directory() {
        use_directory(app, mode, path);
    }
}

/// Confirms a picker row.
/// 确认选择器中的一行。
pub fn confirm_picker(app: &mut App, kind: PickerKind, action: PickerAction, filter: String) {
    match action {
        PickerAction::Command(command) => input::execute(app, command, None),
        PickerAction::Model(id) => {
            app.model_id = id;
            if let Some(model) = app.model() {
                let label = model.label.clone();
                app.toast(
                    Severity::Info,
                    tr!("composer.modelPicker.currentModel", model = label),
                );
            }
        }
        PickerAction::Locale(code) => input::set_locale(app, code),
        PickerAction::Branch(name) => {
            let local = name.strip_prefix("origin/").unwrap_or(&name).to_string();
            app.backend.send(Command::GitSwitchBranch {
                name: local,
                create: false,
            });
        }
        PickerAction::CreateBranch => {
            debug_assert_eq!(kind, PickerKind::Branch);
            if filter.is_empty() || filter.contains(char::is_whitespace) {
                app.toast(Severity::Warning, tr!("tui.commit.branchRequired"));
            } else {
                app.backend.send(Command::GitSwitchBranch {
                    name: filter,
                    create: true,
                });
            }
        }
        PickerAction::Area(area) => app.set_area(area),
        PickerAction::Choice(edit, id) => apply_edit(app, edit, SettingValue::Choice(id)),
    }
}

/// The locale code currently in effect, for the status bar button.
/// 当前生效的语言代码，用于状态栏按钮。
pub fn locale_mark() -> &'static str {
    let active = i18n::active_locale();
    i18n::LOCALES
        .iter()
        .find(|l| l.code == active)
        .map_or(active, |l| l.mark)
}
