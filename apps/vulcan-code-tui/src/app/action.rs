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
    DetailSheet, FolderMode, FolderPicker, History, Inspector, Menu, Overlay, Picker, PickerAction,
    PickerItem, PickerKind,
};
use super::pages::{Area, PageId};
use super::{App, Focus, PanelTab, SidebarMode, SidebarRow, ThinkingPlacement};
use crate::backend::Command;
use crate::i18n;
use crate::model::*;
use crate::service_api::setter::Setter;
use crate::tr;
use crate::ui::theme::Theme;

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
    /// Selected knowledge view, independent from the library list.
    /// 所选知识视图，独立于知识库列表。
    KnowledgeView,
    /// Exact selected managed-document folder.
    /// 准确的托管文档目录选择。
    KnowledgeFolder,
    /// Raw/display corpus for the selected document.
    /// 所选文档的原始或展示语料。
    KnowledgeCorpus,
    /// Local agent enabled-state filter.
    /// 本地智能体启用状态筛选。
    AgentStatus,
    /// Local prompt-skill state filter.
    /// 本地提示词技能状态筛选。
    SkillStatus,
    /// WebUI effective/source override filter for prompt skills.
    /// 提示词技能的 WebUI 生效、来源覆盖筛选。
    SkillSource,
    /// Selected native VMM project; changing it reloads the visible records.
    /// 选中的原生 VMM 项目；变更后重新加载可见记录。
    VmmProject,
    /// Selected host project for the VCE overview.
    /// VCE 概览选中的宿主项目。
    VceProject,
    /// Native profile target, as defined by `native_profile_records`.
    /// 原生画像目标，由 `native_profile_records` 定义。
    ProfileTarget,
    /// Shown tab of the open plugin settings layout; switching loads its unread data.
    /// 当前插件设置版面显示的标签；切换时加载其尚未读取的数据。
    PluginTab,
    /// Shown period of one plugin chart; the value is the chart key and the period id joined
    /// by `pages::PERIOD_SEPARATOR`.
    /// 单个插件图表显示的周期；值为以 `pages::PERIOD_SEPARATOR` 连接的图表键与周期标识。
    PluginChartPeriod,
}

/// Where an edited value goes.
/// 编辑后的值写往何处。
#[derive(Debug, Clone)]
pub enum Edit {
    /// A confirmed WebUI request builder; values are persisted by the service.
    /// 已确认的 WebUI 请求构建器；值由服务持久化。
    Call(Setter),
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
    Edit(Edit),
    MemorySearch,
    KnowledgeSearch,
    /// Filters agent definitions using the catalog's searchable metadata.
    /// 使用目录可搜索元数据筛选智能体定义。
    AgentSearch,
    /// Filters prompt-skill records using their searchable metadata.
    /// 使用可搜索元数据筛选提示词技能记录。
    SkillSearch,
    SetGoal(ConversationId),
    /// A typed number, validated against the row's range before it is applied.
    /// 输入的数字，在应用前按该行的范围校验。
    Number {
        edit: Edit,
        min: i64,
        max: i64,
    },
    KnowledgeCreate,
    /// Collects replacement text after confirming the native resource's purpose.
    /// 确认原生资源用途后收集替换正文。
    ResourceWrite(String),
    /// Continuation of transient multi-field request input.
    /// 临时多字段请求输入的后续步骤。
    Form(Box<super::form::Form>),
}

impl PromptThen {
    /// Search prompts accept an empty value, which clears the search.
    /// 搜索输入框接受空值，表示清除搜索。
    fn allows_empty(&self) -> bool {
        matches!(
            self,
            PromptThen::MemorySearch
                | PromptThen::KnowledgeSearch
                | PromptThen::AgentSearch
                | PromptThen::SkillSearch
                | PromptThen::Edit(_)
                | PromptThen::ResourceWrite(_)
        ) || matches!(self, PromptThen::Form(form) if form.fields[form.index].optional)
    }
}

/// Transient text input preserving multiline service fields and masking credential values.
/// 保留多行服务字段并遮蔽凭据值的临时文本输入框。
#[derive(Clone)]
pub struct PromptDialog {
    /// Localized purpose of this input step.
    /// 此输入步骤的本地化用途。
    pub title: String,
    /// Localized field instructions or request-step context.
    /// 本地化字段说明或请求步骤上下文。
    pub description: String,
    /// Exact transient input; form and resource submission preserve its whitespace.
    /// 准确临时输入；表单及资源提交保留其空白。
    pub input: String,
    /// Masks the input on screen; the value is still sent once, never stored by the TUI.
    /// 在屏幕上遮蔽输入；值只发送一次，TUI 从不保存。
    pub secret: bool,
    /// Exact continuation that determines parsing, validation and request ownership.
    /// 决定解析、校验及请求归属的准确后续步骤。
    pub then: PromptThen,
    /// Localized validation failure cleared on the next input change.
    /// 下一次输入变化时清除的本地化校验失败。
    pub error: Option<String>,
}

impl std::fmt::Debug for PromptDialog {
    /// Redacts all prompt input, including credentials in a partial request form.
    /// 遮蔽全部输入框内容，包括部分请求表单内的凭据。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PromptDialog")
            .field("secret", &self.secret)
            .finish_non_exhaustive()
    }
}

impl PromptDialog {
    /// Whether this declared continuation accepts multiline input without first-line truncation.
    /// 此已声明后续步骤是否接受多行输入而不截断到首行。
    /// Returns true for service forms, resource text, editable settings and goal instructions.
    /// 服务表单、资源文本、可编辑设置及目标说明返回 true。
    pub fn accepts_multiline(&self) -> bool {
        matches!(
            self.then,
            PromptThen::Form(_)
                | PromptThen::ResourceWrite(_)
                | PromptThen::Edit(_)
                | PromptThen::SetGoal(_)
        )
    }

    /// Builds a transient input step with its explicit continuation and masking behavior.
    /// 使用明确的后续步骤及遮蔽行为构建临时输入步骤。
    /// `title` and `description` provide localized purpose and field instructions.
    /// `title` 及 `description` 提供本地化用途及字段说明。
    /// `input` contains the authoritative initial value; `then` owns the submission continuation.
    /// `input` 包含权威初始值；`then` 持有提交后续步骤。
    /// `secret` masks display; returns the initialized dialog with no validation error.
    /// `secret` 遮蔽显示；返回没有校验错误的已初始化输入框。
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
    /// Applies one edit directly; production edits go through pickers and forms.
    /// 直接应用一项编辑；生产编辑经由选择器与表单。
    #[cfg(test)]
    Edit(Edit, SettingValue),
    /// Starts a multi-field WebUI request form.
    /// 启动多字段 WebUI 请求表单。
    Form(Box<super::form::Form>),
    /// Reloads the current page using its exact service loaders.
    /// 使用当前页面对应的精确服务加载器重新加载。
    ReloadPage,
    /// Selects the knowledge library shown on the library details page.
    /// 选择知识库详情页显示的知识库。
    OpenLibrary(String),
    /// Activates the exact dynamic navigation row through the shared dispatcher.
    /// 通过共享分发器激活准确的动态导航行。
    Navigate(usize),
    /// Selects a server-owned agent category, or the empty all-categories filter.
    /// 选择服务端智能体分类，或空的全部分类筛选。
    AgentCategory(String),
    /// Selects a server-owned prompt-skill category, or the empty all-categories filter.
    /// 选择服务端提示词技能分类，或空的全部分类筛选。
    SkillCategory(String),
    /// Opens one immutable managed document in the current library.
    /// 打开当前知识库中的单个不可变托管文档。
    OpenKnowledgeDocument(String),
    /// Moves the managed listing to an explicit page offset.
    /// 将托管列表移动到明确分页偏移量。
    KnowledgeOffset(usize),
    /// Opens the folder picker to add a source directory to this library.
    /// 打开文件夹选择器，为此知识库添加来源目录。
    AddKnowledgeSource(String),
    /// Opens the folder picker to install a local plugin directory.
    /// 打开文件夹选择器以安装本地插件目录。
    InstallPlugin,
    /// Opens one plugin detail mount, dropping the state of the previously open one.
    /// 打开一个插件详情挂载，并丢弃之前打开挂载的状态。
    OpenPluginMount {
        plugin: String,
        mount: String,
    },
    /// Drops one slot's result, e.g. closing a document read in place.
    /// 丢弃一个槽位的结果，例如关闭就地阅读的文档。
    ClearSlot(crate::service_api::Slot),
    Area(Area),
    /// An activity bar entry: opens its area; the workbench entry clicked inside the workbench
    /// toggles the conversation sidebar instead, as in the WebUI.
    /// 活动栏条目：打开其区域；在工作台内点击工作台条目则改为切换会话侧栏，与 WebUI 一致。
    Activity(Area),
    GoPage(PageId),
    OpenAreaPicker,
    SelectConversation(ConversationId),
    NewConversation(Option<String>),
    ToggleProject(String),
    ProjectMenu(String),
    ConversationMenu(ConversationId),
    /// Opens a prepared menu at the click, e.g. the items a one-line button strip had to fold.
    /// 在点击处打开预先构造的菜单，例如单行按钮条收起的项目。
    OpenMenu(Box<Menu>),
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
        UiAction::Send(command) => {
            if let Command::Call(call) = &command
                && !matches!(call.kind, crate::service_api::CallKind::Load(_))
            {
                if app.call_pending {
                    app.toast(Severity::Info, tr!("tui.notice.waitForOperation"));
                    return;
                }
                app.call_pending = true;
            }
            if let Command::Call(call) = &command
                && let crate::service_api::CallKind::Load(slot) = call.kind
            {
                app.slot_errors.remove(&slot);
            }
            app.backend.send(command);
        }
        UiAction::Form(form) => form.open(app),
        UiAction::ReloadPage => app.load_page(),
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
        #[cfg(test)]
        UiAction::Edit(edit, value) => apply_edit(app, edit, value),
        UiAction::Navigate(index) => {
            // Resolve a current row before dispatch so changed catalogs never address stale indexes.
            // 分派前解析当前行，避免变化后的目录使用旧下标。
            let destination = super::pages::navigation(app)
                .into_iter()
                .flat_map(|(_, entries)| entries)
                .nth(index);
            if let Some(entry) = destination {
                perform(app, entry.action);
                app.focus = Focus::AreaNav;
            }
        }
        UiAction::AgentCategory(id) => {
            app.agent_category = id;
            app.pages.remove(&PageId::AgentList);
        }
        UiAction::SkillCategory(id) => {
            app.skill_category = id;
            app.pages.remove(&PageId::SkillList);
        }
        UiAction::OpenLibrary(id) => {
            app.select_library(Some(id));
            perform(app, UiAction::GoPage(PageId::KnowledgeLibrary));
        }
        UiAction::OpenKnowledgeDocument(id) => {
            app.knowledge_document = Some(id);
            app.slots.clear(crate::service_api::Slot::KnowledgeDocument);
            app.pages.remove(&PageId::KnowledgeDocument);
            perform(app, UiAction::GoPage(PageId::KnowledgeDocument));
        }
        UiAction::KnowledgeOffset(offset) => {
            app.knowledge_offset = offset;
            app.slots
                .clear(crate::service_api::Slot::KnowledgeDocuments);
            app.pages.remove(&PageId::KnowledgeDocuments);
            app.load_page();
        }
        UiAction::AddKnowledgeSource(library) => {
            app.select_library(Some(library));
            open_folder(app, FolderMode::KnowledgeSource, None);
        }
        UiAction::InstallPlugin => open_folder(app, FolderMode::Plugin, None),
        UiAction::OpenPluginMount { plugin, mount } => {
            let target = Some((plugin, mount));
            if app.plugin_mount != target {
                use crate::service_api::Slot;
                app.plugin_mount = target;
                app.plugin_tab = None;
                app.plugin_periods.clear();
                app.plugin_loaded_at.clear();
                app.slots.clear_where(Slot::is_plugin_mount);
                app.slot_errors.retain(|slot, _| !slot.is_plugin_mount());
                app.pages.remove(&PageId::PluginMount);
            }
            perform(app, UiAction::GoPage(PageId::PluginMount));
        }
        UiAction::ClearSlot(slot) => {
            app.slots.clear(slot);
            app.slot_errors.remove(&slot);
        }
        UiAction::Area(area) => app.set_area(area),
        UiAction::Activity(Area::Chat) if app.area == Area::Chat => {
            input::execute(app, AppCommand::ToggleSidebar, None);
        }
        UiAction::Activity(area) => app.set_area(area),
        UiAction::GoPage(page) => {
            let index = page
                .area()
                .pages()
                .iter()
                .position(|p| *p == page)
                .unwrap_or(0);
            if page.area() == Area::Knowledge {
                app.knowledge_page = page;
            } else {
                app.nav.insert(page.area(), index);
            }
            app.set_area(page.area());
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
        UiAction::OpenMenu(mut menu) => {
            menu.anchor = app.menu_anchor();
            app.overlay = Some(Overlay::Menu(*menu));
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
            app.area = Area::Chat;
            app.sidebar = SidebarMode::Expanded;
            app.sidebar_free_scroll = false;
            app.focus = Focus::Sidebar;
        }
        UiAction::RailProject(id) => {
            app.overlay = None;
            app.area = Area::Chat;
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
            .find(|c| &c.id == id)
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
/// Applies one edited value to wherever `edit` points.
/// 将编辑后的值写入 `edit` 指向的位置。
pub fn apply_edit(app: &mut App, edit: Edit, value: SettingValue) {
    match edit {
        Edit::Call(setter) => match setter.apply(&value) {
            Some(call) => perform(app, UiAction::Send(Command::Call(call))),
            None => app.toast(Severity::Error, tr!("tui.form.invalid")),
        },
        Edit::Local(setting) => {
            match setting {
                LocalSetting::Locale => input::set_locale(app, value.as_str()),
                LocalSetting::Theme => {
                    app.theme = match value.as_str() {
                        "light" => Theme::light(),
                        _ => Theme::dark(),
                    }
                }
                LocalSetting::Thinking => {
                    app.thinking_placement = match value.as_str() {
                        "docked" => ThinkingPlacement::Docked,
                        _ => ThinkingPlacement::Float,
                    }
                }
                LocalSetting::Sidebar => perform(
                    app,
                    UiAction::SidebarMode(if value.as_bool() {
                        SidebarMode::Expanded
                    } else {
                        SidebarMode::Rail
                    }),
                ),
                LocalSetting::Panel => app.panel_visible = value.as_bool(),
                LocalSetting::KnowledgeView => {
                    if let Some(page) = super::pages::knowledge_view(value.as_str()) {
                        app.knowledge_page = page;
                    }
                }
                LocalSetting::KnowledgeFolder => {
                    // JSON option values distinguish all folders (null) from the root folder (empty string).
                    // JSON 选项值区分全部目录（null）与根目录（空字符串）。
                    let Ok(folder) = serde_json::from_str::<Option<String>>(value.as_str()) else {
                        app.toast(Severity::Error, tr!("tui.form.invalid"));
                        return;
                    };
                    app.knowledge_folder = folder;
                    app.knowledge_offset = 0;
                    app.slots
                        .clear(crate::service_api::Slot::KnowledgeDocuments);
                    app.pages.remove(&PageId::KnowledgeDocuments);
                }
                LocalSetting::KnowledgeCorpus => {
                    app.knowledge_corpus = value.as_str().to_string();
                    app.slots.clear(crate::service_api::Slot::KnowledgeDocument);
                    app.pages.remove(&PageId::KnowledgeDocument);
                }
                LocalSetting::AgentStatus => {
                    app.agent_status = value.as_str().to_string();
                    app.pages.remove(&PageId::AgentList);
                }
                LocalSetting::SkillStatus => {
                    app.skill_status = value.as_str().to_string();
                    app.pages.remove(&PageId::SkillList);
                }
                LocalSetting::SkillSource => {
                    app.skill_source = value.as_str().to_string();
                    app.pages.remove(&PageId::SkillList);
                }
                LocalSetting::VmmProject => {
                    app.vmm_project = value.as_str().parse().ok();
                    app.slots.clear(crate::service_api::Slot::VmmRecords);
                    app.slots.clear(crate::service_api::Slot::VmmProfile);
                }
                LocalSetting::VceProject => {
                    app.vce_project = Some(value.as_str().to_string());
                    app.slots.clear(crate::service_api::Slot::VceOverview);
                    app.slots.clear(crate::service_api::Slot::VceJob);
                }
                LocalSetting::ProfileTarget => {
                    app.profile_target = value.as_str().to_string();
                    app.slots.clear(crate::service_api::Slot::VmmProfile);
                }
                // Tabs and chart periods change the view only; a tab loads just its unread data.
                // 标签与图表周期只改变视图；切换标签只加载其尚未读取的数据。
                LocalSetting::PluginTab => {
                    app.plugin_tab = Some(value.as_str().to_string());
                    for call in super::pages::plugin_tab_reads(app) {
                        perform(app, UiAction::Send(Command::Call(call)));
                    }
                    return;
                }
                LocalSetting::PluginChartPeriod => {
                    if let Some((chart, period)) =
                        value.as_str().split_once(super::pages::PERIOD_SEPARATOR)
                    {
                        app.plugin_periods
                            .insert(chart.to_string(), period.to_string());
                    }
                    return;
                }
            }
            app.load_page();
        }
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
    // Prompt values are transient, including credentials; only the matching RPC receives them.
    // 输入值属于临时数据，包括凭据；仅对应 RPC 接收它们。
    let value = if matches!(
        dialog.then,
        PromptThen::Form(_) | PromptThen::ResourceWrite(_)
    ) {
        dialog.input.clone()
    } else {
        dialog.input.trim().to_string()
    };
    if value.is_empty() && !dialog.then.allows_empty() {
        dialog.error = Some(tr!("tui.prompt.required"));
        return;
    }
    let Some(Overlay::Prompt(mut dialog)) = app.overlay.take() else {
        return;
    };
    let then = std::mem::replace(&mut dialog.then, PromptThen::KnowledgeCreate);
    match then {
        PromptThen::ResourceWrite(path) => {
            let request = crate::service_api::memory_actions::native_resource_write(&path, &value);
            perform(
                app,
                confirm(
                    tr!("tui.field.editResource"),
                    format!("{path}\n\n{value}"),
                    tr!("tui.field.confirm"),
                    UiAction::Send(Command::Call(request)),
                ),
            );
        }
        PromptThen::RenameProject(project_id) => perform(
            app,
            UiAction::Send(Command::RenameProject {
                project_id,
                name: value,
            }),
        ),
        PromptThen::RenameConversation(conversation_id) => perform(
            app,
            UiAction::Send(Command::RenameConversation {
                conversation_id,
                title: value,
            }),
        ),
        PromptThen::Edit(edit) => apply_edit(app, edit, SettingValue::Text(value)),
        PromptThen::Form(form) => form.accept(app, value),
        PromptThen::Number { edit, min, max } => match value.replace([',', '_'], "").parse::<i64>()
        {
            Ok(number) if (min..=max).contains(&number) => {
                apply_edit(app, edit, SettingValue::Number(number))
            }
            _ => {
                dialog.error = Some(tr!("tui.prompt.range", min = min, max = max));
                dialog.then = PromptThen::Number { edit, min, max };
                app.overlay = Some(Overlay::Prompt(dialog));
            }
        },
        PromptThen::KnowledgeCreate => perform(
            app,
            UiAction::Send(Command::Call(
                crate::service_api::knowledge_base::create_knowledge_library(
                    &value,
                    "",
                    &[],
                    serde_json::json!({}),
                ),
            )),
        ),
        PromptThen::MemorySearch => app.memory_query = value,
        PromptThen::KnowledgeSearch => {
            app.knowledge_query = value;
            app.slots.clear(crate::service_api::Slot::KnowledgeSearch);
            app.slot_errors
                .remove(&crate::service_api::Slot::KnowledgeSearch);
            app.pages.remove(&PageId::KnowledgeSearch);
            perform(app, UiAction::GoPage(PageId::KnowledgeSearch));
        }
        PromptThen::AgentSearch => {
            app.agent_query = value;
            app.pages.remove(&PageId::AgentList);
        }
        PromptThen::SkillSearch => {
            app.skill_query = value;
            app.pages.remove(&PageId::SkillList);
        }
        PromptThen::SetGoal(conversation_id) => {
            // Editing the objective retains the service-projected token budget.
            // 编辑目标内容时保留服务投影的令牌预算。
            let token_budget = app
                .sessions
                .get(&conversation_id)
                .and_then(|session| session.insight.goal.as_ref())
                .and_then(|goal| goal.budget);
            perform(
                app,
                UiAction::Send(Command::SaveSessionGoal {
                    conversation_id,
                    objective: value,
                    token_budget,
                }),
            );
        }
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
    // Hidden entries are filtered when the listing arrives, so both sources list everything.
    // 隐藏项在列表到达时过滤，因此两种来源都列出全部条目。
    let command = if picker.mode.picks_directory() {
        Command::BrowseProjectDirectories { path }
    } else {
        Command::ListLocalFiles { path }
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
    // Directory selection only constructs requests; the backend owns all service effects.
    // 目录选择仅构建请求；后端负责全部服务副作用。
    let command = match mode {
        FolderMode::Project => Command::AddProjectFromPath { path },
        FolderMode::KnowledgeSource => {
            let Some(library) = &app.library else {
                return;
            };
            Command::Call(
                crate::service_api::knowledge_base::preview_knowledge_import(library, &path, None),
            )
        }
        FolderMode::Plugin => Command::Call(
            crate::service_api::skills_actions::inspect_local_plugin(&path),
        ),
        FolderMode::Attachment => return,
    };
    perform(app, UiAction::Send(command));
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
            app.backend.send(Command::SelectPublicAiModel {
                conversation_id: app.selected.clone(),
                model_id: id.clone(),
            });
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
            app.backend.send(Command::SwitchWorkspaceGitBranch {
                name: local,
                create: false,
            });
        }
        PickerAction::CreateBranch => {
            debug_assert_eq!(kind, PickerKind::Branch);
            if filter.is_empty() || filter.contains(char::is_whitespace) {
                app.toast(Severity::Warning, tr!("tui.commit.branchRequired"));
            } else {
                app.backend.send(Command::SwitchWorkspaceGitBranch {
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
