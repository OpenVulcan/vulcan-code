// Modal overlays: pickers, menus, dialogs, the folder picker, history, the diff viewer, the commit
// dialog and help.
// 模态浮层：选择器、菜单、对话框、文件夹选择器、历史、diff 查看器、提交对话框与帮助。

use ratatui_textarea::TextArea;

use super::action::{ConfirmDialog, Edit, PromptDialog, UiAction};
use super::commands::AppCommand;
use super::pages::Area;
use crate::model::DirectoryListing;
use crate::tr;

/// What a picker row does when confirmed.
/// 选择器条目确认后的动作。
#[derive(Debug, Clone)]
pub enum PickerAction {
    Command(AppCommand),
    Model(String),
    Locale(&'static str),
    Branch(String),
    CreateBranch,
    Area(Area),
    /// Sets a page choice row to the option id.
    /// 将页面选项行设为该选项 ID。
    Choice(Edit, String),
    /// Requests one session permission mode.
    /// 请求一个会话权限模式。
    PermissionMode(crate::model::PermissionMode),
    /// Opens one system page.
    /// 打开一个系统页面。
    Page(super::pages::PageId),
}

#[derive(Debug, Clone)]
pub struct PickerItem {
    pub label: String,
    pub detail: String,
    pub hint: String,
    pub current: bool,
    pub action: PickerAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerKind {
    Palette,
    Model,
    Language,
    Branch,
    Area,
    Choice,
}

/// A filterable list overlay.
/// 可过滤的列表浮层。
#[derive(Debug, Clone)]
pub struct Picker {
    pub kind: PickerKind,
    pub title: String,
    pub placeholder: String,
    pub filter: String,
    pub items: Vec<PickerItem>,
    pub cursor: usize,
}

impl Picker {
    pub fn new(
        kind: PickerKind,
        title: String,
        placeholder: String,
        items: Vec<PickerItem>,
    ) -> Self {
        let cursor = items.iter().position(|i| i.current).unwrap_or(0);
        Picker {
            kind,
            title,
            placeholder,
            filter: String::new(),
            items,
            cursor,
        }
    }

    /// Items matching the filter (case-insensitive substring on label, detail and hint).
    /// 匹配过滤条件的条目（对标签、说明与提示做不区分大小写的子串匹配）。
    pub fn visible(&self) -> Vec<&PickerItem> {
        let needle = self.filter.to_lowercase();
        self.items
            .iter()
            .filter(|item| {
                needle.is_empty()
                    || matches!(item.action, PickerAction::CreateBranch)
                    || item.label.to_lowercase().contains(&needle)
                    || item.detail.to_lowercase().contains(&needle)
                    || item.hint.to_lowercase().contains(&needle)
            })
            .collect()
    }

    pub fn clamp(&mut self) {
        let len = self.visible().len();
        if len == 0 {
            self.cursor = 0;
        } else if self.cursor >= len {
            self.cursor = len - 1;
        }
    }

    pub fn selected(&self) -> Option<PickerItem> {
        self.visible().get(self.cursor).map(|item| (*item).clone())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitField {
    Target,
    BranchName,
    Message,
    IncludeUnstaged,
}

impl CommitField {
    pub fn next(self, new_branch: bool, back: bool) -> Self {
        let order: &[CommitField] = if new_branch {
            &[
                CommitField::Target,
                CommitField::BranchName,
                CommitField::Message,
                CommitField::IncludeUnstaged,
            ]
        } else {
            &[
                CommitField::Target,
                CommitField::Message,
                CommitField::IncludeUnstaged,
            ]
        };
        let index = order.iter().position(|f| *f == self).unwrap_or(0);
        let next = if back {
            (index + order.len() - 1) % order.len()
        } else {
            (index + 1) % order.len()
        };
        order[next]
    }
}

pub struct CommitDialog {
    pub new_branch: bool,
    pub branch_name: String,
    pub message: TextArea<'static>,
    pub include_unstaged: bool,
    pub field: CommitField,
    pub error: Option<String>,
    /// An empty message was submitted: commit (and push when `true`) once generation finishes.
    /// 提交了空信息：生成完成后提交（为 `true` 时同时推送）。
    pub commit_after_generate: Option<bool>,
}

pub struct DiffView {
    pub file_index: usize,
    pub scroll: usize,
}

#[derive(Debug, Clone)]
pub struct MenuItem {
    pub label: String,
    pub hint: String,
    pub action: UiAction,
    pub danger: bool,
}

/// Context menu, anchored at the click position or centered when opened from the keyboard.
/// 上下文菜单；鼠标打开时锚定在点击位置，键盘打开时居中。
#[derive(Debug, Clone)]
pub struct Menu {
    pub title: String,
    pub items: Vec<MenuItem>,
    pub cursor: usize,
    pub anchor: Option<(u16, u16)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderMode {
    /// Pick a directory to add as a project.
    /// 选择要添加为项目的目录。
    Project,
    /// Pick a file to attach to the prompt.
    /// 选择要附加到提示的文件。
    Attachment,
    /// Pick a source directory for the selected knowledge library.
    /// 为所选知识库选择来源目录。
    KnowledgeSource,
    /// Pick a local plugin directory to install.
    /// 选择要安装的本地插件目录。
    Plugin,
}

impl FolderMode {
    /// The picker chooses a directory rather than a file.
    /// 选择器选择的是目录而非文件。
    pub fn picks_directory(self) -> bool {
        self != FolderMode::Attachment
    }
}

#[derive(Debug, Clone)]
pub struct FolderPicker {
    pub mode: FolderMode,
    pub listing: Option<DirectoryListing>,
    pub cursor: usize,
    pub scroll: usize,
    pub show_hidden: bool,
    /// Path being typed into the location bar.
    /// 正在地址栏中输入的路径。
    pub typing: Option<String>,
    pub loading: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryFilter {
    All,
    Prompts,
    Thinking,
    Tools,
    Replies,
    Notes,
}

impl HistoryFilter {
    pub const ALL: [HistoryFilter; 6] = [
        HistoryFilter::All,
        HistoryFilter::Prompts,
        HistoryFilter::Thinking,
        HistoryFilter::Tools,
        HistoryFilter::Replies,
        HistoryFilter::Notes,
    ];

    pub fn label(self) -> String {
        match self {
            HistoryFilter::All => tr!("tui.history.all"),
            HistoryFilter::Prompts => tr!("tui.history.prompts"),
            HistoryFilter::Thinking => tr!("tui.history.thinking"),
            HistoryFilter::Tools => tr!("tui.history.tools"),
            HistoryFilter::Replies => tr!("tui.history.replies"),
            HistoryFilter::Notes => tr!("tui.history.notes"),
        }
    }
}

/// Conversation history browser: every prompt, thinking block, tool group and reply.
/// 会话历史浏览器：所有提示、思考块、工具组与回复。
#[derive(Debug, Clone)]
pub struct History {
    pub filter: HistoryFilter,
    pub query: String,
    pub cursor: usize,
    pub scroll: usize,
}

impl Default for History {
    fn default() -> Self {
        History {
            filter: HistoryFilter::All,
            query: String::new(),
            cursor: usize::MAX,
            scroll: 0,
        }
    }
}

/// Full view of one history entry; `back` returns to the history list it was opened from.
/// 单个历史条目的完整视图；`back` 返回打开它的历史列表。
#[derive(Debug, Clone)]
pub struct Inspector {
    pub seq: u64,
    pub scroll: usize,
    pub back: Option<Box<History>>,
}

/// One button of a detail sheet.
/// 详情面板的一个按钮。
#[derive(Debug, Clone)]
pub struct SheetAction {
    pub label: String,
    pub danger: bool,
    pub action: UiAction,
}

/// Read-only detail sheet with actions: a memory item, a profile node or a subagent.
/// 带操作的只读详情面板：记忆条目、画像节点或子代理。
#[derive(Clone)]
pub struct DetailSheet {
    pub title: String,
    /// `(label, value)` rows shown as an aligned table.
    /// 以对齐表格显示的 `(标签, 值)` 行。
    pub fields: Vec<(String, String)>,
    /// `(heading, body)` blocks, wrapped to the sheet width.
    /// `(标题, 正文)` 块，按面板宽度换行。
    pub sections: Vec<(String, String)>,
    pub actions: Vec<SheetAction>,
    /// Button under keyboard focus; `None` leaves Enter to close the sheet.
    /// 键盘焦点所在按钮；`None` 时回车关闭面板。
    pub focus: Option<usize>,
    pub scroll: usize,
}

impl std::fmt::Debug for DetailSheet {
    /// Detail text can contain temporary viewer URLs; diagnostics retain shape rather than values.
    /// 详情文本可能包含临时查看地址；诊断仅保留形状，不保留取值。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DetailSheet")
            .field("fields", &self.fields.len())
            .field("sections", &self.sections.len())
            .field("actions", &self.actions.len())
            .finish()
    }
}

impl DetailSheet {
    pub fn new(title: String) -> Self {
        DetailSheet {
            title,
            fields: Vec::new(),
            sections: Vec::new(),
            actions: Vec::new(),
            focus: None,
            scroll: 0,
        }
    }

    pub fn field(mut self, label: String, value: impl Into<String>) -> Self {
        self.fields.push((label, value.into()));
        self
    }

    pub fn section(mut self, heading: String, body: impl Into<String>) -> Self {
        self.sections.push((heading, body.into()));
        self
    }

    pub fn action(mut self, label: String, danger: bool, action: UiAction) -> Self {
        self.actions.push(SheetAction {
            label,
            danger,
            action,
        });
        self
    }
}

pub enum Overlay {
    Picker(Picker),
    Diff(DiffView),
    // Boxed: the dialog owns a TextArea and is far larger than the other variants.
    // 装箱：对话框持有 TextArea，远大于其他变体。
    Commit(Box<CommitDialog>),
    Help { scroll: usize },
    Menu(Menu),
    Confirm(Box<ConfirmDialog>),
    Prompt(Box<PromptDialog>),
    Folder(Box<FolderPicker>),
    History(History),
    Inspector(Inspector),
    Detail(Box<DetailSheet>),
}
