// Application state and the reducer that folds backend facts into it.
// 应用状态，以及将后端事实折叠进状态的 reducer。
//
// Views read `App` and never talk to the backend. Input handling (`input/`) turns keys and clicks
// into state changes and backend `Command`s; `apply` turns `BackendEvent`s into state. This keeps
// the mock and the future service backend interchangeable.
// 视图只读取 `App`，从不直接访问后端。输入处理（`input/`）把按键与点击转换为状态变化与后端 `Command`；
// `apply` 把 `BackendEvent` 转换为状态。由此模拟后端与未来的服务后端可以互换。

pub mod action;
pub mod activity;
#[cfg(test)]
mod activity_tests;
pub mod answer_mode;
pub mod ask;
#[cfg(test)]
mod catalog_tests;
pub mod commands;
pub mod composer;
pub mod find;
pub mod form;
pub mod history;
pub mod hit;
pub mod input;
pub mod insight;
pub mod knowledge_actions;
pub mod knowledge_state;
#[cfg(test)]
mod markdown_memory_tests;
pub mod menus;
#[cfg(test)]
mod model_list_tests;
pub mod notice;
pub mod overlay;
pub mod pages;
pub mod path_mention;
pub mod permission_mode;
#[cfg(test)]
mod plugin_settings_tests;
mod preview;
pub mod selection;
#[cfg(test)]
mod service_tests;
#[cfg(test)]
mod workbench_git_tests;

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use ratatui_textarea::{TextArea, WrapMode};

use crate::backend::{
    BackendEvent, BackendHandle, Command, ConnectionState, GitBusy, GitNotice, Notice,
};
use crate::model::*;
use crate::tr;
use crate::ui::theme::Theme;

use ask::AskState;
use hit::HitMap;
use overlay::Overlay;
use pages::{Area, PageId, PageState};

/// Poll active knowledge jobs only after their preceding observation has returned.
/// 仅在前一次观察已返回后轮询活动知识库任务。
const KNOWLEDGE_JOB_POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Re-read provider tasks while one can still change phase, as the WebUI tasks tab does.
/// 仍有任务可能改变阶段时重新读取供应商任务，与 WebUI 任务页一致。
const PROVIDER_TASK_POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Re-read an MCP sign-in while its flow waits for the browser or exchanges the code.
/// MCP 登录流程等待浏览器或交换授权码期间重新读取其状态。
const MCP_OAUTH_POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Re-read interval of the luaskills-debug status while its install runs.
/// luaskills-debug 安装运行期间状态的重读间隔。
const LUASKILLS_DEBUG_POLL_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Sidebar,
    Transcript,
    Ask,
    Composer,
    Panel,
    /// Navigation column of a system area.
    /// 系统区域的导航列。
    AreaNav,
    /// Page body of a system area.
    /// 系统区域的页面主体。
    AreaPage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelTab {
    Changes,
    Progress,
    Terminal,
    /// Memory injected into this conversation: dynamic TTL items and the static profile.
    /// 注入本会话的记忆：动态 TTL 条目与静态画像。
    Memory,
    /// Context usage, goal and lifetime usage of the main session and its subagents.
    /// 上下文用量、目标，以及主会话与子代理的生命周期用量。
    Stats,
    Agents,
}

impl PanelTab {
    pub const ALL: [PanelTab; 6] = [
        PanelTab::Changes,
        PanelTab::Progress,
        PanelTab::Terminal,
        PanelTab::Memory,
        PanelTab::Stats,
        PanelTab::Agents,
    ];

    /// Tabs whose content is a free-scrolling list with a selectable cursor row.
    /// 内容为可自由滚动、带可选光标行列表的标签页。
    pub fn scrolls(self) -> bool {
        matches!(self, PanelTab::Memory | PanelTab::Stats | PanelTab::Agents)
    }
}

/// Client-only view state of the scrolling panel tabs.
/// 可滚动面板标签页的纯客户端视图状态。
#[derive(Debug, Default)]
pub struct PanelView {
    /// Memory tab shows the static profile instead of dynamic injections.
    /// 记忆页显示静态画像而非动态注入。
    pub memory_static: bool,
    /// Selected row per tab (memory items, agents); stats has no cursor.
    /// 每个标签页选中的行（记忆条目、代理）；统计页没有光标。
    pub cursor: HashMap<PanelTab, usize>,
    pub scroll: HashMap<PanelTab, usize>,
    /// The wheel moved the list away from the cursor; keyboard movement re-attaches it.
    /// 滚轮使列表脱离光标；键盘移动会重新跟随。
    pub free_scroll: bool,
}

impl PanelView {
    pub fn cursor(&self, tab: PanelTab) -> usize {
        self.cursor.get(&tab).copied().unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidebarRow {
    Project(String),
    Unlinked,
    Conversation(ConversationId),
}

/// Sidebar search, as the WebUI sidebar's search field: a project whose name matches lists all its
/// conversations, otherwise only conversations whose title matches are listed.
/// 侧栏搜索，对应 WebUI 侧栏的搜索框：名称匹配的项目列出其全部会话，否则只列出标题匹配的会话。
#[derive(Debug, Clone, Default)]
pub struct SidebarFilter {
    pub query: String,
    /// Whether keys type into the query.
    /// 按键是否输入查询。
    pub editing: bool,
}

impl SidebarRow {
    /// Headers of fixed sections cannot hold the cursor.
    /// 固定分区的标题不能持有光标。
    pub fn selectable(&self) -> bool {
        !matches!(self, SidebarRow::Unlinked)
    }
}

/// How the project sidebar is shown; toggled at runtime, never persisted.
/// 项目侧栏的显示方式；运行时切换，不持久化。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarMode {
    /// Full list of projects and conversations.
    /// 完整的项目与会话列表。
    Expanded,
    /// A narrow rail of buttons and project initials; the full list opens as a drawer on focus.
    /// 由按钮与项目首字母组成的窄栏；获得焦点时以抽屉形式打开完整列表。
    Rail,
}

/// Snapshot taken when a run starts, so the session bar can show what this turn has used so far.
/// 运行开始时的快照，使会话栏能显示本轮到目前为止的用量。
#[derive(Debug, Clone, Copy)]
pub struct TurnBase {
    pub started: Instant,
    /// Lifetime usage (main session and subagents) before the turn.
    /// 本轮之前的生命周期用量（主会话与子代理）。
    pub usage: UsageBucket,
    /// Highest event seq before the turn; later events belong to it.
    /// 本轮之前的最大事件 seq；之后的事件属于本轮。
    pub seq: u64,
}

/// Usage of the turn in progress.
/// 进行中轮次的用量。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveTurn {
    pub elapsed_ms: u64,
    /// Usage of the requests that finished during this turn.
    /// 本轮中已结束请求的用量。
    pub usage: UsageBucket,
    /// Rough token estimate of text still streaming, not yet reported by the provider.
    /// 仍在流式输出、提供方尚未报告的文本的粗略 token 估算。
    pub streaming: u64,
}

/// Rough token count of streamed text (about four bytes per token), for display only.
/// 流式文本的粗略 token 数（约每 4 字节一个 token），仅用于显示。
pub fn estimate_tokens(text: &str) -> u64 {
    (text.len() as u64).div_ceil(4)
}

/// Where the live thinking box floats; toggled at runtime, never persisted.
/// 实时思考框的位置；运行时切换，不持久化。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThinkingPlacement {
    Float,
    Docked,
}

/// Per-conversation client state.
/// 每个会话的客户端状态。
pub struct SessionState {
    pub events: Vec<ConversationEvent>,
    pub pending_question: Option<PendingQuestion>,
    pub prompts: Vec<PendingPrompt>,
    pub usage: ContextUsage,
    pub todo: Vec<TodoItem>,
    pub agents: Vec<AgentInfo>,
    pub insight: SessionInsight,
    pub mode: ExecutionMode,
    /// Last accepted answer mode.
    /// 最近接纳的回答模式。
    pub answer_mode: AnswerModeSession,
    /// Last saved permission mode; `None` until the session or draft loads.
    /// 最近保存的权限模式；会话或草稿加载前为 `None`。
    pub permission_mode: Option<PermissionMode>,
    /// Last saved unattended mode; unavailable for drafts and before the session loads.
    /// 最近保存的无人值守模式；草稿及会话加载前不可用。
    pub unattended: Unattended,
    /// Blocks whose default fold state the user flipped, keyed by event seq.
    /// 用户翻转过默认折叠状态的块，以事件 seq 为键。
    pub toggled: HashSet<u64>,
    /// Lines scrolled up from the bottom; 0 follows the stream.
    /// 从底部向上滚动的行数；0 表示跟随流式输出。
    pub scroll_from_bottom: usize,
    pub selected: Option<u64>,
    /// Reasoning blocks that just completed, for the fade-out of the thinking box.
    /// 刚完成的思考块，用于思考框淡出。
    pub reasoning_settled: Option<(u64, Instant)>,
    pub hydrated: bool,
    /// Expands every folded block (thinking, tools) at once for reading back through history.
    /// 一次展开所有折叠块（思考、工具），便于回看历史。
    pub reveal: bool,
    /// Set while a run is active; see [`TurnBase`].
    /// 运行进行中时设置；见 [`TurnBase`]。
    pub turn_base: Option<TurnBase>,
}

impl SessionState {
    fn new() -> Self {
        SessionState {
            events: Vec::new(),
            pending_question: None,
            prompts: Vec::new(),
            usage: ContextUsage::default(),
            todo: Vec::new(),
            agents: Vec::new(),
            insight: SessionInsight::default(),
            mode: ExecutionMode::Build,
            answer_mode: AnswerModeSession::default(),
            permission_mode: None,
            unattended: Unattended::Unavailable,
            toggled: HashSet::new(),
            scroll_from_bottom: 0,
            selected: None,
            reasoning_settled: None,
            hydrated: false,
            reveal: false,
            turn_base: None,
        }
    }

    fn upsert(&mut self, event: ConversationEvent) {
        match self.events.binary_search_by_key(&event.seq, |e| e.seq) {
            Ok(index) => {
                let was_streaming = matches!(
                    self.events[index].body,
                    EventBody::AssistantReasoning {
                        status: AssistantSemanticStatus::Streaming,
                        ..
                    }
                );
                let now_settled = matches!(
                    event.body,
                    EventBody::AssistantReasoning {
                        status: AssistantSemanticStatus::Completed
                            | AssistantSemanticStatus::Interrupted,
                        ..
                    }
                );
                if was_streaming && now_settled {
                    self.reasoning_settled = Some((event.seq, Instant::now()));
                }
                self.events[index] = event;
            }
            Err(index) => self.events.insert(index, event),
        }
    }

    fn append(&mut self, seq: u64, text: &str) {
        if let Ok(index) = self.events.binary_search_by_key(&seq, |e| e.seq) {
            match &mut self.events[index].body {
                EventBody::AssistantReasoning { text: body, .. }
                | EventBody::AssistantText { text: body, .. } => body.push_str(text),
                _ => {}
            }
        }
    }

    /// Lifetime usage of the main session plus every subagent.
    /// 主会话加上所有子代理的生命周期用量。
    pub fn lifetime_total(&self) -> UsageBucket {
        let mut total = self.insight.lifetime.main;
        total.add(&self.insight.lifetime.children_total());
        total
    }

    /// Usage of the run in progress: finished requests since the run started plus an estimate
    /// of the text still streaming.
    /// 进行中运行的用量：自运行开始以来已结束的请求，加上仍在流式输出文本的估算。
    pub fn live_turn(&self, now: Instant) -> Option<LiveTurn> {
        let base = self.turn_base?;
        let total = self.lifetime_total();
        let usage = UsageBucket {
            requests: total.requests.saturating_sub(base.usage.requests),
            tool_calls: total.tool_calls.saturating_sub(base.usage.tool_calls),
            input: total.input.saturating_sub(base.usage.input),
            cache_read: total.cache_read.saturating_sub(base.usage.cache_read),
            output: total.output.saturating_sub(base.usage.output),
            ..UsageBucket::default()
        };
        let streaming = self
            .events
            .iter()
            .filter(|e| e.seq > base.seq)
            .map(|e| match &e.body {
                EventBody::AssistantReasoning {
                    text,
                    status: AssistantSemanticStatus::Streaming,
                    ..
                }
                | EventBody::AssistantText {
                    text,
                    status: AssistantSemanticStatus::Streaming,
                    ..
                } => estimate_tokens(text),
                _ => 0,
            })
            .sum();
        Some(LiveTurn {
            elapsed_ms: now.saturating_duration_since(base.started).as_millis() as u64,
            usage,
            streaming,
        })
    }

    /// The most recent finished turn: `(duration_ms, total_tokens, requests)`.
    /// 最近一次结束的轮次：`(耗时毫秒, token 合计, 请求数)`。
    pub fn last_turn(&self) -> Option<(u64, u64, u32)> {
        self.events.iter().rev().find_map(|e| match e.body {
            EventBody::TurnStats {
                duration_ms,
                total_tokens,
                requests,
                ..
            } => Some((duration_ms, total_tokens, requests)),
            _ => None,
        })
    }

    /// Starts or clears the turn snapshot when the run phase changes.
    /// 运行阶段变化时建立或清除本轮快照。
    fn phase_changed(&mut self, was_active: bool, active: bool) {
        if active && !was_active {
            self.turn_base = Some(TurnBase {
                started: Instant::now(),
                usage: self.lifetime_total(),
                seq: self.events.last().map_or(0, |e| e.seq),
            });
        } else if !active {
            self.turn_base = None;
        }
    }

    /// The reasoning block currently streaming, if any.
    /// 当前正在流式输出的思考块（如有）。
    pub fn live_reasoning(&self) -> Option<&ConversationEvent> {
        self.events.iter().rev().find(|e| {
            matches!(
                e.body,
                EventBody::AssistantReasoning {
                    status: AssistantSemanticStatus::Streaming,
                    ..
                }
            )
        })
    }
}

/// Git side state mirrored from the backend.
/// 从后端镜像的 Git 侧状态。
#[derive(Default)]
pub struct GitView {
    pub snapshot: Option<GitSnapshot>,
    pub cursor: usize,
    pub busy: Option<GitBusy>,
    pub branches: Vec<GitBranch>,
    /// Successful diffs keyed by their complete workspace, repository, path and mode identity.
    /// 按完整工作区、仓库、路径及模式身份保存的成功差异。
    pub diffs: HashMap<GitDiffKey, FileDiff>,
    /// Files whose diff failed to load, with the service's reason when it gave one.
    /// 加载 diff 失败的文件，及服务给出的原因。
    pub diff_failures: HashMap<GitDiffKey, Option<String>>,
    /// Repository chosen in the panel that the backend has not published yet.
    /// 已在面板中选择、后端尚未发布的仓库。
    pub pending: Option<String>,
}

impl GitView {
    /// Returns the full identity of `file` in the displayed repository, or none during selection.
    /// 返回显示仓库内 `file` 的完整身份；选择切换期间返回空。
    pub fn diff_key(&self, file: &GitFile) -> Option<GitDiffKey> {
        // Only a resolved selection can own a diff request.
        // 只有已经确定的选择才能拥有差异请求。
        let snapshot = self.repository()?;
        Some(GitDiffKey {
            workspace: snapshot.workspace.clone(),
            repository_key: snapshot.selected.clone()?,
            path: file.path.clone(),
            staged: file.staged,
        })
    }

    /// Accepts a result only while its workspace, repository and file mode are still displayed.
    /// 仅当结果的工作区、仓库及文件模式仍在显示时接收结果。
    pub fn accepts_diff(&self, key: &GitDiffKey) -> bool {
        self.repository().is_some_and(|snapshot| {
            snapshot.workspace == key.workspace
                && snapshot.selected.as_ref() == Some(&key.repository_key)
                && snapshot
                    .files
                    .iter()
                    .any(|file| file.path == key.path && file.staged == key.staged)
        })
    }

    /// Path of `file` from the project root: the displayed repository's folder joined with the
    /// repository path, or none during selection.
    /// `file` 相对项目根目录的路径：显示仓库的目录与仓库内路径拼接；选择切换期间返回空。
    pub fn project_relative_path(&self, file: &GitFile) -> Option<String> {
        let snapshot = self.repository()?;
        let selected = snapshot.selected.as_ref()?;
        let repository = snapshot.repositories.iter().find(|r| &r.key == selected)?;
        Some(match repository.relative_path.trim_matches('/') {
            "" | "." => file.path.clone(),
            folder => format!("{folder}/{}", file.path),
        })
    }

    /// Files in panel order: staged first, then unstaged.
    /// 面板顺序的文件：先暂存区，再未暂存。
    pub fn ordered_files(&self) -> Vec<&GitFile> {
        let Some(snapshot) = self.snapshot.as_ref().filter(|_| self.pending.is_none()) else {
            return Vec::new();
        };
        let mut files: Vec<&GitFile> = snapshot.files.iter().filter(|f| f.staged).collect();
        files.extend(snapshot.files.iter().filter(|f| !f.staged));
        files
    }

    /// Whether the selected project has Git, as the WebUI `sessionGitRepositoryAvailable`: the
    /// catalog lists repositories, the first read is still loading, or Git work is running.
    /// 所选项目是否有 Git，对应 WebUI `sessionGitRepositoryAvailable`：目录列出了仓库、首次读取
    /// 仍在加载，或 Git 操作正在执行。
    pub fn available(&self) -> bool {
        self.snapshot
            .as_ref()
            .is_none_or(|snapshot| !snapshot.repositories.is_empty())
            || self.busy.is_some()
    }

    /// The snapshot when a monitored repository is selected; branch and push need one.
    /// 已选受监听仓库时的快照；分支与推送需要它。
    pub fn repository(&self) -> Option<&GitSnapshot> {
        self.snapshot
            .as_ref()
            .filter(|s| s.selected.is_some() && self.pending.is_none())
    }

    /// The repository the panel marks: a pending choice before the backend publishes it.
    /// 面板标记的仓库：后端发布前为待定选择。
    pub fn marked(&self) -> Option<&String> {
        self.pending
            .as_ref()
            .or(self.snapshot.as_ref()?.selected.as_ref())
    }

    /// The monitored repository after the selected one, wrapping around; `None` with fewer
    /// than two monitored repositories.
    /// 所选仓库之后的受监听仓库，循环回绕；受监听仓库少于两个时为 `None`。
    pub fn next_repository(&self) -> Option<String> {
        let snapshot = self.snapshot.as_ref()?;
        let monitored: Vec<&GitRepository> = snapshot
            .repositories
            .iter()
            .filter(|r| r.monitoring)
            .collect();
        if monitored.len() < 2 {
            return None;
        }
        let current = monitored.iter().position(|r| self.marked() == Some(&r.key));
        let next = current.map_or(0, |index| (index + 1) % monitored.len());
        Some(monitored[next].key.clone())
    }

    /// The repository monitoring switches as a menu, as the WebUI Git settings dialog.
    /// 以菜单呈现的仓库监听开关，对应 WebUI 的 Git 设置对话框。
    pub fn monitoring_menu(&self) -> Option<overlay::Menu> {
        let snapshot = self.snapshot.as_ref()?;
        if snapshot.repositories.is_empty() {
            return None;
        }
        Some(overlay::Menu {
            title: tr!("gitCommitDialog.monitoringManager"),
            items: snapshot
                .repositories
                .iter()
                .map(|repository| overlay::MenuItem {
                    label: format!(
                        "{} {}",
                        if repository.monitoring { "☑" } else { "☐" },
                        repository.name
                    ),
                    hint: repository_path(repository).unwrap_or_default().to_string(),
                    action: action::UiAction::Send(Command::SetGitMonitoring {
                        repository_key: repository.key.clone(),
                        enabled: !repository.monitoring,
                    }),
                    danger: false,
                })
                .collect(),
            cursor: 0,
            anchor: None,
        })
    }
}

/// The relative path shown beside a repository name, omitted when it adds nothing.
/// 显示在仓库名称旁的相对路径；不提供额外信息时省略。
pub fn repository_path(repository: &GitRepository) -> Option<&str> {
    let path = repository.relative_path.as_str();
    (!path.is_empty() && path != "." && path != repository.name).then_some(path)
}

pub struct App {
    pub theme: Theme,
    pub backend: BackendHandle,
    pub connection: ConnectionState,
    pub started: Instant,
    pub now: Instant,
    pub projects: Vec<Project>,
    pub conversations: Vec<ConversationSummary>,
    pub models: Vec<ModelInfo>,
    pub model_id: String,
    /// Saved permission judge backend from the workbench; `None` greys out auto-approve.
    /// 来自工作台的已保存权限判定后端；`None` 时自动审批显示为灰色。
    pub permission_judge_backend: Option<JudgeBackend>,
    pub effort: ReasoningEffort,
    /// Efforts the current model accepts; `CycleReasoning` stays within them.
    /// 当前模型接受的推理强度；`CycleReasoning` 只在其中循环。
    pub efforts: Vec<ReasoningEffort>,
    pub base_url: String,
    pub selected: Option<ConversationId>,
    pub sessions: HashMap<ConversationId, SessionState>,
    pub focus: Focus,
    pub sidebar: SidebarMode,
    pub panel_visible: bool,
    pub panel_tab: PanelTab,
    pub panel: PanelView,
    pub sidebar_cursor: Option<SidebarRow>,
    pub sidebar_scroll: usize,
    /// The wheel moved the sidebar away from the cursor; keyboard movement re-attaches it.
    /// 滚轮使侧栏脱离光标；键盘移动会重新跟随。
    pub sidebar_free_scroll: bool,
    pub collapsed_projects: HashSet<String>,
    /// Locally pinned conversations; the service has no conversation pin yet.
    /// 本地置顶的会话；服务尚无会话置顶。
    pub terminal_cursor: usize,
    pub composer: TextArea<'static>,
    /// Long pastes collapsed into tokens of the composer; see `composer.rs`.
    /// 折叠为输入框标记的长粘贴；见 `composer.rs`。
    pub pastes: Vec<composer::PastedText>,
    /// Numbers the paste tokens of the current draft.
    /// 为当前草稿的粘贴标记编号。
    pub paste_serial: usize,
    pub ask: AskState,
    pub git: GitView,
    pub command_sessions: Vec<CommandSession>,
    pub overlay: Option<Overlay>,
    /// Pushed notices, oldest first.
    /// 推送的通知，最早的在前。
    pub toasts: Vec<notice::Notice>,
    pub thinking_placement: ThinkingPlacement,
    pub quit_armed_until: Option<Instant>,
    pub should_quit: bool,
    pub hits: HitMap,
    pub locale_explicit: bool,
    /// Active top-level area; everything except `Chat` shows a navigation column and a page.
    /// 当前顶层区域；除 `Chat` 外都显示导航列与页面。
    pub area: Area,
    /// Selected page index per area.
    /// 每个区域选中的页面序号。
    pub nav: HashMap<Area, usize>,
    pub pages: HashMap<PageId, PageState>,
    /// Sanitized WebUI DTOs and the loader that owns each result.
    /// 已清理机密的 WebUI DTO，以及每个结果所属的加载器。
    pub slots: crate::service_api::Slots,
    /// Blocks another whole-document write until its predecessor's reloads settle.
    /// 前一次刷新结束前阻止再次写入完整文档。
    pub call_pending: bool,
    /// Persistent scoped read errors stay visible until a matching retry or result.
    /// 持久限定范围读取错误保留显示，直到匹配重试或结果到达。
    pub slot_errors: HashMap<crate::service_api::Slot, String>,
    /// Native project and profile selectors shared by the VMM DTO pages.
    /// VMM DTO 页面共用的原生项目与画像选择。
    pub vmm_project: Option<i64>,
    pub profile_target: String,

    /// Project whose permission strategy is shown.
    /// 显示其权限策略的项目。
    pub judge_policy_project: Option<String>,
    /// Saved memory mode from the last settings read; it decides which memory pages exist.
    /// 最近一次设置读取得到的已保存记忆模式；它决定存在哪些记忆页面。
    pub memory_mode: Option<String>,
    /// Host project whose Markdown memory the project page shows.
    /// 项目页显示其 Markdown 记忆的宿主项目。
    pub markdown_memory_project: Option<String>,
    /// Local record filter for the memory page.
    /// 记忆页面的本地记录筛选。
    pub memory_query: String,
    /// Read-only knowledge selectors and the explicit unsaved human policy draft.
    /// 只读知识选择器及明确未保存人工策略草稿。
    pub knowledge: knowledge_state::KnowledgeUiState,
    /// Exact selected knowledge library; None denotes an empty or not-yet-loaded catalog.
    /// 准确所选知识库；None 表示目录为空或尚未加载。
    pub library: Option<String>,
    /// Active library view, independent from the dynamic left-hand library identities.
    /// 当前知识库视图，独立于左侧动态知识库身份。
    pub knowledge_page: PageId,
    /// Open plugin detail mount as `(plugin id, mount id)`.
    /// 当前打开的插件详情挂载，形式为 `(插件标识, 挂载标识)`。
    pub plugin_mount: Option<(String, String)>,
    /// Chosen tab of the open plugin settings layout.
    /// 当前插件设置版面所选的标签。
    pub plugin_tab: Option<String>,
    /// Chosen period per plugin chart, keyed by section id and block position.
    /// 每个插件图表所选的周期，以区块标识与数据块位置为键。
    pub plugin_periods: HashMap<String, String>,
    /// Chosen resolutions of import conflicts by conflict key; unset conflicts take the newer side.
    /// 按冲突键记录的导入冲突解决方式；未选择的冲突取较新一侧。
    pub markdown_memory_conflicts: HashMap<String, String>,
    /// When each plugin data section last received a result, by data slot index.
    /// 每个插件数据区块最近收到结果的时间，以数据槽位下标为键。
    pub plugin_loaded_at: HashMap<u8, std::time::SystemTime>,
    /// Next re-read of a live provider-task or sign-in slot; armed only by a result, so reads never overlap.
    /// 实时供应商任务或登录槽位的下一次重读；只由结果设置，因此读取不会重叠。
    live_poll_due: std::collections::BTreeMap<crate::service_api::Slot, Instant>,
    /// Backend-defined category currently filtering agent definitions.
    /// 当前筛选智能体定义的后端分类。
    pub agent_category: String,
    /// Release shown on the changelog page; `None` opens the running version.
    /// 更新记录页显示的发布；`None` 时打开正在运行的版本。
    pub changelog_release: Option<String>,
    /// Hub skill whose detail currently shows the original instead of the translation.
    /// 当前以原文而非译文显示详情的 Hub 技能。
    pub hub_original_skill: Option<String>,
    /// Backend-defined category currently filtering effective prompt skills.
    /// 当前筛选生效提示词技能的后端分类。
    pub skill_category: String,
    /// Local cross-locale agent search text and enabled-state filter.
    /// 本地跨语言智能体搜索文本及启用状态筛选。
    pub agent_query: String,
    /// Declared all/enabled/disabled agent-state filter.
    /// 已声明的 all/enabled/disabled 智能体状态筛选。
    pub agent_status: String,
    /// Local cross-locale prompt-skill search text.
    /// 本地跨语言提示词技能搜索文本。
    pub skill_query: String,
    /// Declared all/enabled/disabled/issues skill-state filter.
    /// 已声明的 all/enabled/disabled/issues 技能状态筛选。
    pub skill_status: String,
    /// WebUI source filter; all shows runtime-effective records only.
    /// WebUI 来源筛选；all 仅展示运行时生效记录。
    pub skill_source: String,
    /// Provider shown by the model list; empty or unknown falls back to the first group.
    /// 模型清单显示的供应商；为空或未知时回退到第一个分组。
    pub model_provider: String,
    /// Local model-list search text matched against display names and model ids.
    /// 与显示名称及模型标识匹配的本地模型清单搜索文本。
    pub model_query: String,
    /// Declared all/conversation/internal/multimodal model usage filter.
    /// 已声明的 all/conversation/internal/multimodal 模型用途筛选。
    pub model_usage: String,
    /// Declared all or single output-modality model filter.
    /// 已声明的 all 或单一输出模态模型筛选。
    pub model_output: String,
    /// Files attached to the next prompt.
    /// 附加到下一条提示的文件。
    pub attachments: Vec<String>,
    /// Last left click `(time, column, row)`, for double clicks and menu anchoring.
    /// 最近一次左键点击 `(时间, 列, 行)`，用于双击与菜单定位。
    pub last_click: Option<(Instant, u16, u16)>,
    /// Transcript text selection and the copy waiting for the terminal clipboard.
    /// 会话记录文本选择，以及等待写入终端剪贴板的复制内容。
    pub clipboard: selection::Clipboard,
    /// Find in the open conversation's transcript, while open.
    /// 打开期间的当前会话记录查找。
    pub transcript_find: Option<find::TranscriptFind>,
    /// The composer's `@` path mention list.
    /// 输入框的 `@` 路径提及列表。
    pub path_mention_state: path_mention::PathMentionState,
    /// Sidebar search, while open.
    /// 打开期间的侧栏搜索。
    pub sidebar_filter: Option<SidebarFilter>,
    /// Subagent and command activity of the selected session.
    /// 所选会话的子代理与命令活动。
    pub activity: activity::ActivityState,
}

impl App {
    /// Panel tabs in strip order; Changes is left out while the project has no Git.
    /// 按标签条顺序排列的面板标签；项目没有 Git 时不含变更页。
    pub fn panel_tabs(&self) -> Vec<PanelTab> {
        PanelTab::ALL
            .into_iter()
            .filter(|tab| *tab != PanelTab::Changes || self.git.available())
            .collect()
    }

    /// The shown panel tab: the chosen one, or the first shown tab while the chosen one is hidden,
    /// so the choice returns with a project that has Git.
    /// 显示的面板标签：所选标签，或在其隐藏期间显示第一个标签，使选择随有 Git 的项目恢复。
    pub fn active_panel_tab(&self) -> PanelTab {
        let tabs = self.panel_tabs();
        if tabs.contains(&self.panel_tab) {
            self.panel_tab
        } else {
            tabs.first().copied().unwrap_or(self.panel_tab)
        }
    }

    pub fn new(backend: BackendHandle, theme: Theme, locale_explicit: bool) -> Self {
        let now = Instant::now();
        App {
            theme,
            backend,
            connection: ConnectionState::Connecting,
            started: now,
            now,
            projects: Vec::new(),
            conversations: Vec::new(),
            models: Vec::new(),
            model_id: String::new(),
            permission_judge_backend: None,
            effort: ReasoningEffort::High,
            efforts: ReasoningEffort::DEFAULT_CHOICES.to_vec(),
            base_url: String::new(),
            selected: None,
            sessions: HashMap::new(),
            focus: Focus::Composer,
            sidebar: SidebarMode::Expanded,
            panel_visible: true,
            panel_tab: PanelTab::Changes,
            panel: PanelView::default(),
            sidebar_cursor: None,
            sidebar_scroll: 0,
            sidebar_free_scroll: false,
            collapsed_projects: HashSet::new(),
            terminal_cursor: 0,
            composer: new_composer(),
            pastes: Vec::new(),
            paste_serial: 0,
            ask: AskState::default(),
            git: GitView::default(),
            command_sessions: Vec::new(),
            overlay: None,
            toasts: Vec::new(),
            thinking_placement: ThinkingPlacement::Float,
            quit_armed_until: None,
            should_quit: false,
            hits: HitMap::default(),
            locale_explicit,
            area: Area::Chat,
            nav: HashMap::new(),
            pages: HashMap::new(),
            slots: crate::service_api::Slots::default(),
            call_pending: false,
            slot_errors: HashMap::new(),
            vmm_project: None,
            profile_target: "user".into(),

            judge_policy_project: None,
            memory_mode: None,
            markdown_memory_project: None,
            memory_query: String::new(),
            knowledge: knowledge_state::KnowledgeUiState::default(),
            library: None,
            knowledge_page: PageId::KnowledgeLibrary,
            plugin_mount: None,
            plugin_tab: None,
            plugin_periods: HashMap::new(),
            markdown_memory_conflicts: HashMap::new(),
            plugin_loaded_at: HashMap::new(),
            live_poll_due: std::collections::BTreeMap::new(),
            agent_category: String::new(),
            changelog_release: None,
            hub_original_skill: None,
            skill_category: String::new(),
            agent_query: String::new(),
            agent_status: "all".into(),
            skill_query: String::new(),
            skill_status: "all".into(),
            skill_source: "all".into(),
            model_provider: String::new(),
            model_query: String::new(),
            model_usage: "all".into(),
            model_output: "all".into(),
            attachments: Vec::new(),
            last_click: None,
            clipboard: selection::Clipboard::default(),
            transcript_find: None,
            path_mention_state: Default::default(),
            sidebar_filter: None,
            activity: activity::ActivityState::default(),
        }
    }

    /// Switches the top-level area and moves focus into it.
    /// 切换顶层区域并将焦点移入。
    pub fn set_area(&mut self, area: Area) {
        self.area = area;
        self.load_page();
        self.focus = match area {
            Area::Chat => Focus::Composer,
            _ => Focus::AreaNav,
        };
    }

    /// Loads the visible page through its WebUI service functions; chat needs no system loaders.
    /// 经对应 WebUI 服务函数加载可见页面；会话区域不需要系统加载器。
    pub fn load_page(&mut self) {
        if let Some(page) = self.current_page() {
            // An explicit null workbench slot records a failed refresh, so page reload retries it.
            // 明确为空的工作台槽位记录刷新失败，因此重新加载页面时重试该读取。
            if self
                .slots
                .value(crate::service_api::Slot::Workbench)
                .is_null()
                && let Some(loader) = self
                    .slots
                    .loader(crate::service_api::Slot::Workbench)
                    .cloned()
            {
                action::perform(self, action::UiAction::Send(Command::Call(loader)));
            }
            // Route page effects through the same dispatcher used by mouse and keyboard.
            // 页面副作用经过鼠标与键盘共用的分发入口。
            for call in page.loaders(self) {
                action::perform(self, action::UiAction::Send(Command::Call(call)));
            }
        }
    }

    /// Selects the first available catalog entry when selection is absent or was removed.
    /// 当前没有选择或所选条目已删除时，选中目录中第一个可用条目。
    /// Selects an optional catalog library and invalidates all state owned by the previous library.
    /// 选择可选目录知识库，并使旧知识库持有的全部状态失效。
    /// `library` is the catalog identity; unchanged selection preserves paging and queries.
    /// `library` 为目录身份；选择未变化时保留分页及查询。
    pub fn select_library(&mut self, library: Option<String>) {
        if self.library == library {
            return;
        }
        self.library = library;
        self.knowledge = knowledge_state::KnowledgeUiState::default();
        for slot in [
            crate::service_api::Slot::KnowledgeWords,
            crate::service_api::Slot::KnowledgeBindings,
            crate::service_api::Slot::KnowledgeIntegrity,
            crate::service_api::Slot::KnowledgeJob,
            crate::service_api::Slot::KnowledgeSearch,
            crate::service_api::Slot::KnowledgeFolders,
            crate::service_api::Slot::KnowledgeDocuments,
            crate::service_api::Slot::KnowledgeDocument,
            crate::service_api::Slot::KnowledgeEmbeddings,
            crate::service_api::Slot::KnowledgeChanges,
            crate::service_api::Slot::KnowledgeChange,
        ] {
            self.slots.clear(slot);
            self.slot_errors.remove(&slot);
        }
        for page in Area::Knowledge.pages() {
            self.pages.remove(&page);
        }
        if self.knowledge_page == PageId::KnowledgeDocument {
            self.knowledge_page = PageId::KnowledgeDocuments;
        }
    }

    /// Returns the selected row in the current server-projected navigation, or its first row.
    /// 返回当前服务投影导航中已选行，未选择时返回首行。
    pub fn navigation_index(&self) -> usize {
        pages::navigation(self)
            .into_iter()
            .flat_map(|(_, entries)| entries)
            .position(|entry| entry.selected)
            .unwrap_or(0)
    }

    fn accept_catalog(&mut self, slot: crate::service_api::Slot) {
        use crate::service_api::Slot;
        let mut changed = false;
        match slot {
            Slot::KnowledgeLibraries => {
                if let Some(catalog) = self.slots.value(slot).as_array()
                    && !catalog
                        .iter()
                        .any(|entry| entry["id"].as_str() == self.library.as_deref())
                {
                    let next = catalog
                        .first()
                        .and_then(|entry| entry["id"].as_str())
                        .map(str::to_string);
                    changed = next != self.library;
                    self.select_library(next);
                }
            }
            Slot::VmmProjects => {
                if let Some(catalog) = self.slots.at(slot, "/data/value/projects").as_array()
                    && !catalog
                        .iter()
                        .any(|entry| entry["projectId"].as_i64() == self.vmm_project)
                {
                    let next = catalog
                        .first()
                        .and_then(|entry| entry["projectId"].as_i64());
                    changed = next != self.vmm_project;
                    self.vmm_project = next;
                    for slot in [Slot::VmmRecords, Slot::VmmProfile] {
                        self.slots.clear(slot);
                    }
                }
            }
            Slot::Workbench => {
                // Both category catalogs may change independently from the host project catalog.
                // 两种分类目录可能独立于宿主项目目录变化。
                let workbench = self.slots.value(slot);
                if !self.agent_category.is_empty()
                    && !workbench
                        .pointer("/subagentCatalog/categories")
                        .and_then(serde_json::Value::as_array)
                        .is_some_and(|categories| {
                            categories.iter().any(|category| {
                                category["categoryId"].as_str()
                                    == Some(self.agent_category.as_str())
                            })
                        })
                {
                    self.agent_category.clear();
                    self.pages.remove(&PageId::AgentList);
                }
                if !self.skill_category.is_empty()
                    && !workbench
                        .pointer("/agentSkills/categories")
                        .and_then(serde_json::Value::as_array)
                        .is_some_and(|categories| {
                            categories.iter().any(|category| {
                                category["categoryId"].as_str()
                                    == Some(self.skill_category.as_str())
                            })
                        })
                {
                    self.skill_category.clear();
                    self.pages.remove(&PageId::SkillList);
                }

                if !self
                    .projects
                    .iter()
                    .any(|project| Some(&project.id) == self.judge_policy_project.as_ref())
                {
                    let next = self.projects.first().map(|project| project.id.clone());
                    changed |= next != self.judge_policy_project;
                    self.judge_policy_project = next;
                    self.slots.clear(Slot::ProjectPermissionJudgePolicy);
                    self.slots.clear(Slot::ProjectUnattendedDefault);
                    self.slots.clear(Slot::ProjectReviewRules);
                }
                if !self
                    .projects
                    .iter()
                    .any(|project| Some(&project.id) == self.markdown_memory_project.as_ref())
                {
                    let next = self.projects.first().map(|project| project.id.clone());
                    changed |= next != self.markdown_memory_project
                        && self.current_page() == Some(PageId::MarkdownProject);
                    self.markdown_memory_project = next;
                }
            }
            // A mode change moves the memory area onto the pages of the new mode.
            // 模式变化会把记忆区域切换到新模式的页面。
            Slot::MemorySettings => {
                let mode = self.slots.at(slot, "/mode").as_str().map(str::to_string);
                if mode.is_some() && mode != self.memory_mode {
                    self.memory_mode = mode;
                    changed = self.area == Area::Memory;
                }
            }
            _ => {}
        }
        if changed {
            self.load_page();
        }
    }

    /// The page shown in the current system area.
    /// 当前系统区域显示的页面。
    /// Returns the exact active content page independently from dynamic sidebar identities.
    /// 返回准确活动内容页面，与动态侧栏身份独立。
    pub fn current_page(&self) -> Option<PageId> {
        if self.area == Area::Knowledge {
            return Some(self.knowledge_page);
        }
        // Static areas retain their page index; category catalogs select within a single page.
        // 静态区域保留页面下标；分类目录在单页内选择。
        let pages = self.area.pages();
        // Clamp only view state, never entity identities.
        // 只限制视图状态，不处理实体身份。
        let index = self.nav.get(&self.area).copied().unwrap_or(0);
        let page = pages
            .get(index.min(pages.len().saturating_sub(1)))
            .copied()?;
        // The memory mode decides which memory pages exist.
        // 记忆模式决定存在哪些记忆页面。
        Some(if self.area == Area::Memory {
            pages::memory_page_for_mode(page, self.memory_mode.as_deref())
        } else {
            page
        })
    }

    pub fn page_state(&self, page: PageId) -> PageState {
        self.pages.get(&page).cloned().unwrap_or_default()
    }

    pub fn page_state_mut(&mut self, page: PageId) -> &mut PageState {
        self.pages.entry(page).or_default()
    }

    /// Closes the top overlay; the inspector returns to the history list it came from.
    /// 关闭顶层浮层；检视器返回其来源的历史列表。
    pub fn close_overlay(&mut self) {
        self.overlay = match self.overlay.take() {
            Some(Overlay::Inspector(inspector)) => inspector.back.map(|h| Overlay::History(*h)),
            _ => None,
        };
    }

    /// Screen position of a click in the last half second, where a context menu should open.
    /// 最近半秒内的点击位置，即上下文菜单应出现之处。
    pub fn menu_anchor(&self) -> Option<(u16, u16)> {
        self.last_click
            .filter(|(at, _, _)| at.elapsed() < Duration::from_millis(500))
            .map(|(_, x, y)| (x, y))
    }

    /// Seconds since start, driving shimmer and spinners.
    /// 自启动以来的秒数，驱动微光与旋转指示器。
    pub fn phase(&self) -> f32 {
        self.now.duration_since(self.started).as_secs_f32()
    }

    pub fn session(&self) -> Option<&SessionState> {
        self.selected.as_ref().and_then(|id| self.sessions.get(id))
    }

    pub fn session_mut(&mut self) -> Option<&mut SessionState> {
        let id = self.selected.clone()?;
        Some(self.sessions.entry(id).or_insert_with(SessionState::new))
    }

    pub fn conversation(&self) -> Option<&ConversationSummary> {
        let id = self.selected.as_ref()?;
        self.conversations.iter().find(|c| &c.id == id)
    }

    pub fn project_of(&self, conversation: &ConversationSummary) -> Option<&Project> {
        let id = conversation.project_id.as_ref()?;
        self.projects.iter().find(|p| &p.id == id)
    }

    pub fn model(&self) -> Option<&ModelInfo> {
        self.models.iter().find(|m| m.id == self.model_id)
    }

    pub fn run_phase(&self) -> RunPhase {
        self.conversation().map_or(RunPhase::Idle, |c| c.run_phase)
    }

    pub fn mode(&self) -> ExecutionMode {
        self.session().map_or(ExecutionMode::Build, |s| s.mode)
    }

    /// True while something on screen animates, so the loop keeps redrawing.
    /// 屏幕上有动画时为真，使主循环持续重绘。
    pub fn animating(&self) -> bool {
        self.connection != ConnectionState::Connected
            || self.conversations.iter().any(|c| c.run_phase.is_active())
            || !self.toasts.is_empty()
            || self.git.busy.is_some()
            || matches!(&self.overlay, Some(Overlay::Folder(picker)) if picker.loading)
            // A counting-down approval redraws every frame so its remaining seconds stay current.
            // 正在倒计时的审批每帧重绘，使剩余秒数保持最新。
            || self
                .session()
                .and_then(|s| s.pending_question.as_ref())
                .and_then(|question| question.approval.as_ref())
                .is_some_and(|approval| approval.deadline_unix_ms.is_some())
            || self
                .session()
                .and_then(|s| s.reasoning_settled)
                .is_some_and(|(_, at)| {
                    at.elapsed() < Duration::from_millis(crate::ui::thinking::FADE_MS)
                })
    }

    pub fn tick(&mut self, now: Instant) {
        self.now = now;
        self.toasts.retain(|t| t.until > now);
        if self.quit_armed_until.is_some_and(|until| until <= now) {
            self.quit_armed_until = None;
        }
        // A due slot is re-read only while its page is open; leaving the page drops the schedule.
        // 到期槽位仅在其页面打开时重读；离开页面即放弃该计划。
        let page = self.current_page();
        let due = self
            .live_poll_due
            .iter()
            .filter(|(_, due)| **due <= now)
            .map(|(slot, _)| *slot)
            .collect::<Vec<_>>();
        for slot in due {
            self.live_poll_due.remove(&slot);
            if (page.is_some_and(|page| page.live_slots().contains(&slot))
                || activity::live_slot_visible(self, slot))
                && let Some(loader) = self.slots.loader(slot).map(|loader| loader.renewed())
            {
                action::perform(self, action::UiAction::Send(Command::Call(loader)));
            }
        }
        activity::tick(self);
    }

    /// Schedules the next read of a provider-task or sign-in slot while its content can still change.
    /// 在供应商任务或登录槽位内容仍可能变化时安排下一次读取。
    /// `slot` is the slot whose result just arrived.
    /// `slot` 为刚收到结果的槽位。
    fn arm_live_poll(&mut self, slot: crate::service_api::Slot) {
        use crate::service_api::Slot;
        let value = self.slots.value(slot);
        let interval = match slot {
            Slot::KnowledgeJob => matches!(
                value.get("state").and_then(serde_json::Value::as_str),
                Some("queued" | "running")
            )
            .then_some(KNOWLEDGE_JOB_POLL_INTERVAL),
            Slot::KnowledgeEmbeddings => (value
                .get("pending")
                .and_then(serde_json::Value::as_u64)
                .is_some_and(|count| count > 0)
                && value.get("paused").and_then(serde_json::Value::as_bool) == Some(false))
            .then_some(KNOWLEDGE_JOB_POLL_INTERVAL),
            Slot::ProviderTasks => value
                .as_array()
                .is_some_and(|tasks| {
                    tasks
                        .iter()
                        .any(|task| task.get("terminal").and_then(|t| t.as_bool()) == Some(false))
                })
                .then_some(PROVIDER_TASK_POLL_INTERVAL),
            Slot::McpOAuth => matches!(
                value
                    .pointer("/flow/phase")
                    .and_then(|phase| phase.as_str()),
                Some("awaiting_callback" | "exchanging")
            )
            .then_some(MCP_OAUTH_POLL_INTERVAL),
            // The status carries the latest install task, so it is read while that task runs.
            // 状态携带最近安装任务，因此在该任务运行期间持续读取。
            Slot::LuaSkillsDebugStatus => (value
                .pointer("/task/state")
                .and_then(serde_json::Value::as_str)
                == Some("running"))
            .then_some(LUASKILLS_DEBUG_POLL_INTERVAL),
            // Memory, GPU use and latency keep changing, so the status is read while shown, at
            // the interval the service publishes in it.
            // 内存、GPU 占用与延迟持续变化，因此状态在显示期间按服务在其中发布的间隔持续读取。
            Slot::PermissionJudgeStatus => value
                .pointer("/pollIntervalMs")
                .and_then(|interval| interval.as_u64())
                .map(Duration::from_millis),
            // The endpoint table follows the background prober at the status interval.
            // 端点表按状态间隔跟随后台探测。
            Slot::PermissionJudgeEdgeEndpoints => self
                .slots
                .at(Slot::PermissionJudgeStatus, "/pollIntervalMs")
                .as_u64()
                .map(Duration::from_millis),
            // Activity lists refresh while an entry runs and their page is shown.
            // 活动列表在有条目运行且其页面显示期间刷新。
            Slot::ShellJobs | Slot::SessionActivity => activity::live_interval(self, slot),
            _ => return,
        };
        match interval {
            Some(interval) => {
                self.live_poll_due.insert(slot, self.now + interval);
                // Endpoints that arrived before the status had no interval; the status re-arms them.
                // 先于状态到达的端点结果没有间隔；由状态重新安排其读取。
                if slot == Slot::PermissionJudgeStatus
                    && self
                        .slots
                        .loader(Slot::PermissionJudgeEdgeEndpoints)
                        .is_some()
                {
                    self.live_poll_due
                        .entry(Slot::PermissionJudgeEdgeEndpoints)
                        .or_insert(self.now + interval);
                }
            }
            None => {
                self.live_poll_due.remove(&slot);
            }
        }
    }

    /// Folds one backend fact into the state.
    /// 将一条后端事实折叠进状态。
    pub fn apply(&mut self, event: BackendEvent) {
        match event {
            BackendEvent::AnswerMode {
                conversation_id,
                state,
            } => self.session_for(&conversation_id).answer_mode = state,
            BackendEvent::PermissionMode {
                conversation_id,
                mode,
                unattended,
            } => {
                let session = self.session_for(&conversation_id);
                session.permission_mode = Some(mode);
                session.unattended = unattended;
            }
            BackendEvent::UnattendedDefault {
                conversation_id,
                default,
            } => permission_mode::open_unattended_prompt(self, &conversation_id, default),
            BackendEvent::Connection(state) => {
                self.connection = state;
                if state != ConnectionState::Connected {
                    self.call_pending = false;
                    // Results of the lost connection are discarded, so no reply will clear it.
                    // 断开连接的结果会被丢弃，因此不会有回复来清除它。
                    self.git.busy = None;
                    for session in self.sessions.values_mut() {
                        session.answer_mode = AnswerModeSession::default();
                        session.permission_mode = None;
                        session.unattended = Unattended::Unavailable;
                    }
                }
                if state == ConnectionState::Connected {
                    activity::reconnected(self);
                    self.load_page();
                }
            }
            BackendEvent::Workbench(snapshot) => {
                self.projects = snapshot.projects;
                self.conversations = snapshot.conversations;
                self.models = snapshot.models;
                self.model_id = snapshot.selected_model_id;
                self.permission_judge_backend = snapshot.permission_judge_backend;
                self.base_url = snapshot.base_url;
                if !self.backend.is_mock && !self.base_url.is_empty() {
                    self.backend.label = self.base_url.clone();
                }
                if let Some(locale) = snapshot.ui_locale.as_deref()
                    && !self.locale_explicit
                {
                    crate::i18n::set_active_locale(crate::i18n::resolve_locale(None, Some(locale)));
                    if self.composer_is_empty() {
                        self.reset_composer();
                    }
                }
                // The worker serves selections in order, so a snapshot can still carry an earlier
                // one; the user's latest listed choice stands, and the snapshot only fills in a
                // missing or removed selection. The sidebar cursor follows real changes only.
                // 工作线程按顺序处理选择，快照可能仍带有较早的选择；用户最近且仍在列表中的选择
                // 保持不变，快照只补上缺失或已移除的选择。侧栏光标只随实际变化移动。
                let listed = self
                    .selected
                    .as_ref()
                    .is_some_and(|id| self.conversations.iter().any(|row| &row.id == id));
                if !listed && self.selected != snapshot.selected_conversation_id {
                    self.selected = snapshot.selected_conversation_id;
                    self.sidebar_cursor = self.selected.clone().map(SidebarRow::Conversation);
                }
            }
            BackendEvent::Selected(id) => self.select_conversation(id),
            BackendEvent::SubmitRejected {
                conversation_id,
                prompt,
                attachments,
            } => self.restore_rejected_submission(&conversation_id, &prompt, attachments),
            BackendEvent::ModelSelection {
                model_id,
                effort,
                efforts,
            } => {
                if !model_id.is_empty() {
                    self.model_id = model_id;
                }
                if !efforts.is_empty() {
                    self.efforts = efforts;
                }
                if let Some(effort) = effort {
                    self.effort = effort;
                }
            }
            BackendEvent::ConversationSummary(summary) => {
                let was_active = self
                    .conversations
                    .iter()
                    .find(|c| c.id == summary.id)
                    .is_some_and(|c| c.run_phase.is_active());
                let active = summary.run_phase.is_active();
                let id = summary.id.clone();
                match self.conversations.iter_mut().find(|c| c.id == summary.id) {
                    Some(row) => *row = summary,
                    None => self.conversations.insert(0, summary),
                }
                if let Some(session) = self.sessions.get_mut(&id) {
                    session.phase_changed(was_active, active);
                }
            }
            BackendEvent::ConversationHydrated {
                conversation_id,
                events,
            } => {
                let session = self.session_for(&conversation_id);
                session.events = events;
                session.answer_mode = AnswerModeSession::default();
                session.permission_mode = None;
                session.unattended = Unattended::Unavailable;
                session.events.sort_by_key(|e| e.seq);
                session.hydrated = true;
            }
            BackendEvent::EventUpsert {
                conversation_id,
                event,
            } => self.session_for(&conversation_id).upsert(event),
            BackendEvent::TextAppend {
                conversation_id,
                seq,
                text,
            } => self.session_for(&conversation_id).append(seq, &text),
            BackendEvent::RunPhase {
                conversation_id,
                phase,
            } => {
                if let Some(row) = self
                    .conversations
                    .iter_mut()
                    .find(|c| c.id == conversation_id)
                {
                    let was_active = row.run_phase.is_active();
                    row.run_phase = phase;
                    if let Some(session) = self.sessions.get_mut(&conversation_id) {
                        session.phase_changed(was_active, phase.is_active());
                    }
                }
            }
            BackendEvent::PendingQuestion {
                conversation_id,
                question,
            } => {
                let arrived = question.is_some();
                let is_selected = self.selected.as_deref() == Some(conversation_id.as_str());
                self.session_for(&conversation_id).pending_question = question;
                if is_selected {
                    self.ask = AskState::default();
                    if arrived && self.focus == Focus::Composer && self.composer_is_empty() {
                        self.focus = Focus::Ask;
                    } else if !arrived && self.focus == Focus::Ask {
                        self.focus = Focus::Composer;
                    }
                }
            }
            BackendEvent::PendingPrompts {
                conversation_id,
                prompts,
            } => self.session_for(&conversation_id).prompts = prompts,
            BackendEvent::ContextUsage {
                conversation_id,
                usage,
            } => self.session_for(&conversation_id).usage = usage,
            BackendEvent::Todo {
                conversation_id,
                items,
            } => self.session_for(&conversation_id).todo = items,
            BackendEvent::PlanMode {
                conversation_id,
                mode,
            } => self.session_for(&conversation_id).mode = mode,
            BackendEvent::CommandSessions(sessions) => {
                self.command_sessions = sessions;
                self.terminal_cursor = self
                    .terminal_cursor
                    .min(self.command_sessions.len().saturating_sub(1));
            }
            BackendEvent::Agents {
                conversation_id,
                agents,
            } => {
                self.session_for(&conversation_id).agents = agents;
                activity::subagents_changed(self, &conversation_id);
            }
            BackendEvent::ShellJobChanged(changed) => activity::job_changed(self, changed),
            BackendEvent::SessionInsight {
                conversation_id,
                insight,
            } => self.session_for(&conversation_id).insight = *insight,
            BackendEvent::Git(snapshot) => {
                // The backend serves repository choices in order, so snapshots published before the
                // latest choice are skipped instead of pulling the panel back to them.
                // 后端按顺序处理仓库选择，因此最新选择之前发布的快照会被跳过，而不是把面板拉回去。
                if let Some(pending) = &self.git.pending {
                    let monitored = snapshot
                        .repositories
                        .iter()
                        .any(|r| r.monitoring && &r.key == pending);
                    let earlier = snapshot.selected.as_ref().is_some_and(|key| key != pending);
                    if monitored && earlier {
                        return;
                    }
                    self.git.pending = None;
                }
                self.git.diffs.clear();
                self.git.diff_failures.clear();
                self.git.snapshot = Some(snapshot);
                let len = self.git.ordered_files().len();
                self.git.cursor = self.git.cursor.min(len.saturating_sub(1));
                self.request_visible_diff();
            }
            BackendEvent::GitCleared => {
                self.git.pending = None;
                self.git.diffs.clear();
                self.git.diff_failures.clear();
                self.git.snapshot = None;
                self.git.cursor = 0;
            }
            BackendEvent::GitDiff { key, diff } => {
                if self.git.accepts_diff(&key) {
                    self.git.diff_failures.remove(&key);
                    self.git.diffs.insert(key, diff);
                }
            }
            BackendEvent::GitDiffFailed { key, message } => {
                if self.git.accepts_diff(&key) {
                    self.git.diff_failures.insert(key, message);
                }
            }
            BackendEvent::GitBranches(branches) => {
                self.git.branches = branches;
                input::refresh_branch_picker(self);
            }
            BackendEvent::CommitMessage {
                request,
                text,
                done,
            } => {
                let mut commit_now = None;
                if let Some(Overlay::Commit(dialog)) = &mut self.overlay
                    && dialog.generation == Some(request)
                {
                    if !text.is_empty() {
                        dialog.message.insert_str(&text);
                    }
                    if done {
                        dialog.generation = None;
                        dialog.field = overlay::CommitField::Message;
                        commit_now = dialog.commit_after_generate.take();
                    }
                }
                if let Some(push) = commit_now {
                    input::submit_commit(self, push);
                }
            }
            BackendEvent::GitBusy(busy) => self.git.busy = busy,
            BackendEvent::GitNotice(notice) => {
                let message = match notice {
                    GitNotice::Committed => tr!("gitCommitDialog.actionDone"),
                    GitNotice::Pushed => tr!("gitCommitDialog.pushDone"),
                    GitNotice::Pulled { updated: true } => tr!("gitCommitDialog.pullDone"),
                    GitNotice::Pulled { updated: false } => tr!("gitCommitDialog.pullUpToDate"),
                    GitNotice::Switched(name) => tr!("gitBranchMenu.switched", name = name),
                    GitNotice::Created(name) => tr!("gitBranchCreate.created", name = name),
                };
                if matches!(self.overlay, Some(Overlay::Commit(_))) {
                    self.overlay = None;
                }
                self.toast(NoticeSeverity::Info, message);
            }
            BackendEvent::Toast { severity, message } => self.toast(severity, message),
            BackendEvent::Projects(projects) => {
                self.projects = projects;
                self.accept_catalog(crate::service_api::Slot::Workbench);
                self.fix_sidebar_cursor();
            }
            BackendEvent::UnsupportedConversation {
                conversation_id,
                title,
            } => {
                self.overlay = Some(Overlay::Confirm(Box::new(action::ConfirmDialog {
                    title: tr!("workbenchConversation.unsupportedDelete.title"),
                    body: tr!(
                        "workbenchConversation.unsupportedDelete.description",
                        conversation = title
                    ),
                    confirm_label: tr!("workbenchConversation.unsupportedDelete.confirm"),
                    danger: true,
                    then: action::UiAction::Send(Command::DeleteUnsupportedConversation {
                        conversation_id,
                    }),
                    focus_confirm: false,
                })));
            }
            BackendEvent::ConversationRemoved(id) => {
                self.conversations.retain(|c| c.id != id);
                self.sessions.remove(&id);
                if self.selected.as_ref() == Some(&id) {
                    self.selected = None;
                    self.ask = AskState::default();
                    if let Some(next) = self.conversations.first().map(|c| c.id.clone()) {
                        self.select_conversation(next);
                    }
                }
                self.fix_sidebar_cursor();
            }
            BackendEvent::Directory(mut listing) => {
                if let Some(Overlay::Folder(picker)) = &mut self.overlay {
                    if !picker.show_hidden {
                        listing.entries.retain(|e| !e.hidden);
                    }
                    picker.cursor = 0;
                    picker.scroll = 0;
                    picker.loading = false;
                    picker.listing = Some(listing);
                }
            }
            BackendEvent::Slot { slot, call, value } => {
                if !self.slot_is_current(slot, &call) {
                    return;
                }
                // Initial or recovered registry reads may unlock dependent library reads for the same selection.
                // 初始或已恢复注册表读取可为相同选择解锁依赖知识库读取。
                let needs_knowledge_reads = slot == crate::service_api::Slot::KnowledgeLibraries
                    && !self.slots.loaded(slot);
                // Selection reconciliation may already load the newly selected owner's page.
                // 选择校正可能已加载新选定所有者页面。
                let previous_library = self.library.clone();
                // Content heads and settings revoke paid query results; refreshing metadata never replays a query.
                // 内容头及设置撤销付费查询结果；刷新元信息绝不重放查询。
                let knowledge_changed = slot == crate::service_api::Slot::KnowledgeLibraries && self.library.as_deref().is_some_and(|id| {
                    // Compare the same immutable library identity in two complete registry snapshots.
                    // 对比两个完整注册表快照中相同不可变知识库身份。
                    let old = self.slots.value(slot).as_array().and_then(|rows| rows.iter().find(|row| row["id"] == id));
                    // A missing new row is handled by normal catalog removal reconciliation below.
                    // 缺少新条目由下方正常目录移除校正处理。
                    let new = value.as_array().and_then(|rows| rows.iter().find(|row| row["id"] == id));
                    matches!((old, new), (Some(old), Some(new)) if old["headChangeSeq"] != new["headChangeSeq"] || old["settings"] != new["settings"])
                });
                if knowledge_changed {
                    self.knowledge.query.clear();
                    self.knowledge.cursors = vec![None];
                    for stale in [
                        crate::service_api::Slot::KnowledgeSearch,
                        crate::service_api::Slot::KnowledgeDocuments,
                        crate::service_api::Slot::KnowledgeFolders,
                    ] {
                        self.slots.clear(stale);
                        self.slot_errors.remove(&stale);
                    }
                }
                // Terminal job transitions refresh the currently visible library content exactly once.
                // 任务进入终态时，只刷新一次当前可见知识库内容。
                let was_active_job = slot == crate::service_api::Slot::KnowledgeJob
                    && matches!(
                        self.slots.at(slot, "/state").as_str(),
                        Some("queued" | "running")
                    );
                // A finished CUDA installation changes detection and a finished model download adds a
                // registration, so the settings are re-read.
                // CUDA 安装结束会改变探测结果，模型下载结束会新增登记，因此重新读取设置。
                const JOBS: [&str; 2] = ["/cudaInstall/running", "/modelDownload/running"];
                let running_jobs = |app: &Self| {
                    JOBS.map(|job| {
                        slot == crate::service_api::Slot::PermissionJudgeStatus
                            && app.slots.at(slot, job) == &serde_json::Value::Bool(true)
                    })
                };
                let was_running = running_jobs(self);
                self.slot_errors.remove(&slot);
                if slot == crate::service_api::Slot::PathMentionDirectory {
                    self.path_mention_loaded(Some(&call));
                }
                self.slots.set(slot, call.clone(), value);
                activity::slot_arrived(self, slot, &call);
                let still_running = running_jobs(self);
                if was_running
                    .iter()
                    .zip(still_running)
                    .any(|(was, still)| *was && !still)
                {
                    action::perform(
                        self,
                        action::UiAction::Send(Command::Call(
                            crate::service_api::permission_judge::load_permission_judge_settings(),
                        )),
                    );
                }
                match slot {
                    crate::service_api::Slot::PluginData(index) => {
                        self.plugin_loaded_at
                            .insert(index, std::time::SystemTime::now());
                    }
                    // A layout's reads depend on its content, so they follow its arrival.
                    // 版面的读取取决于其内容，因此在其到达后发出。
                    crate::service_api::Slot::PluginUi => {
                        for call in pages::plugin_reads(self) {
                            action::perform(self, action::UiAction::Send(Command::Call(call)));
                        }
                    }
                    _ => {}
                }
                if slot == crate::service_api::Slot::KnowledgeJob {
                    // Only queued/running states need polling; a successful result arms the next read.
                    // 只有排队、运行状态需要轮询；成功结果设置下一次读取。
                    let active = matches!(
                        self.slots.at(slot, "/state").as_str(),
                        Some("queued" | "running")
                    );
                    if was_active_job
                        && !active
                        && self.current_page().is_some_and(|page| {
                            page.live_slots()
                                .contains(&crate::service_api::Slot::KnowledgeJob)
                        })
                    {
                        self.load_page();
                    }
                }
                self.arm_live_poll(slot);
                self.accept_catalog(slot);
                if (needs_knowledge_reads || knowledge_changed)
                    && self.library == previous_library
                    && self
                        .current_page()
                        .is_some_and(|page| page.area() == Area::Knowledge)
                {
                    self.load_page();
                }
            }
            BackendEvent::SlotFailed {
                slot,
                call,
                message,
            } => {
                if !self.slot_is_current(slot, &call) {
                    return;
                }
                if slot == crate::service_api::Slot::PathMentionDirectory {
                    self.path_mention_loaded(None);
                }
                activity::slot_failed(self, slot, &message);
                self.slots.clear(slot);
                self.slot_errors.insert(slot, message);
                self.live_poll_due.remove(&slot);
            }
            BackendEvent::Preview { call, result } => preview::open(self, call, result),
            BackendEvent::CallFinished => self.call_pending = false,
            BackendEvent::Notice(notice) => {
                // A committed policy/create request ends its draft even if the following registry read failed.
                // 已提交策略或创建请求结束其草稿，即使后续注册表读取失败。
                if matches!(
                    &notice,
                    Notice::Completed("create_knowledge_library" | "set_knowledge_index_mode")
                ) {
                    self.knowledge.draft = None;
                    if self.current_page() == Some(PageId::KnowledgePolicy) {
                        self.knowledge_page = PageId::KnowledgeBases;
                        self.load_page();
                    }
                }
                self.notice(notice);
            }
        }
    }

    /// Rejects results for a library, project or profile target that is no longer selected.
    /// 拒绝已不再选中的知识库、项目或画像目标的结果。
    fn slot_is_current(
        &self,
        slot: crate::service_api::Slot,
        call: &crate::service_api::ServiceCall,
    ) -> bool {
        use crate::service_api::Slot;
        if let Some(current) = activity::slot_is_current(self, slot, call) {
            return current;
        }
        match slot {
            Slot::PathMentionDirectory => self.path_mention_listing_is_current(call),
            Slot::KnowledgeWords | Slot::KnowledgeBindings | Slot::KnowledgeIntegrity => {
                call.param("/libraryId").as_str() == self.library.as_deref()
            }
            Slot::KnowledgeJob => {
                call.param("/request/libraryId").as_str() == self.library.as_deref()
            }
            Slot::KnowledgeEmbeddings => {
                call.param("/request/libraryId").as_str() == self.library.as_deref()
                    && call.param("/request/offset").as_u64()
                        == self.knowledge.failure_offsets.last().copied()
            }
            Slot::KnowledgeChanges => {
                call.param("/request/libraryId").as_str() == self.library.as_deref()
                    && call.param("/request/beforeSeq")
                        == &serde_json::json!(
                            self.knowledge
                                .history_before
                                .last()
                                .expect("initial history boundary")
                        )
            }
            Slot::KnowledgeChange => {
                call.param("/request/libraryId").as_str() == self.library.as_deref()
                    && call.param("/request/seq").as_u64() == self.knowledge.change_seq
                    && call.param("/request/offset").as_u64()
                        == self.knowledge.change_offsets.last().copied()
            }
            Slot::KnowledgePackages => {
                call.param("/path") == &serde_json::json!(self.knowledge.package_path)
            }
            Slot::KnowledgeSelection => self.conversation().is_some_and(|conversation| {
                call.param("/conversationId").as_str() == Some(conversation.id.as_str())
            }),
            Slot::KnowledgeAuthorizedLibraries | Slot::ProjectKnowledgeBindings => self
                .conversation()
                .and_then(|conversation| conversation.project_id.as_deref())
                .is_some_and(|project_id| call.param("/projectId").as_str() == Some(project_id)),
            Slot::KnowledgeFolders => self.library.as_ref().is_some_and(|id| {
                call.params == serde_json::json!({"request":self.knowledge.directory_request(id)})
            }),
            Slot::KnowledgeDocuments => self.library.as_ref().is_some_and(|id| {
                call.params == serde_json::json!({"request":self.knowledge.directory_request(id)})
            }),
            Slot::KnowledgeDocument => self
                .library
                .as_deref()
                .and_then(|id| self.knowledge.document_call(id))
                .is_some_and(|expected| {
                    call.method == expected.method && call.params == expected.params
                }),
            Slot::KnowledgeSearch => {
                call.param("/request/libraryId").as_str() == self.library.as_deref()
                    && call.param("/request/query").as_str() == Some(self.knowledge.query.as_str())
            }

            Slot::ProjectPermissionJudgePolicy
            | Slot::ProjectUnattendedDefault
            | Slot::ProjectReviewRules => {
                call.param("/projectId").as_str() == self.judge_policy_project.as_deref()
            }
            Slot::VmmRecords => {
                call.param("/request/payload/vmmProjectId").as_i64() == self.vmm_project
            }
            Slot::VmmProfile => {
                call.param("/request/payload/vmmProjectId").as_i64() == self.vmm_project
                    && call.param("/request/payload/target").as_str()
                        == Some(self.profile_target.as_str())
            }
            // Plugin results belong to the open mount; action results also to its current
            // runtime generation, so a reload never mixes generations.
            // 插件结果属于当前挂载；Action 结果还须属于其当前运行时代际，重新加载时不混用代际。
            Slot::PluginUi => self.plugin_mount.as_ref().is_some_and(|(plugin, mount)| {
                call.param("/pluginId").as_str() == Some(plugin.as_str())
                    && call.param("/mountId").as_str() == Some(mount.as_str())
            }),
            Slot::PluginAsset => self.plugin_mount.as_ref().is_some_and(|(plugin, _)| {
                call.param("/pluginId").as_str() == Some(plugin.as_str())
            }),
            Slot::PluginConfig | Slot::PluginCredential(_) | Slot::PluginData(_) => {
                self.plugin_mount.as_ref().is_some_and(|(plugin, _)| {
                    call.param("/pluginId").as_str() == Some(plugin.as_str())
                }) && call.param("/runtimeIdentity")
                    == self.slots.at(Slot::PluginUi, "/runtimeIdentity")
            }
            _ => true,
        }
    }

    fn notice(&mut self, notice: Notice) {
        let (severity, message) = match notice {
            Notice::Completed(function) => (NoticeSeverity::Info, completed_notice(function)),
            Notice::ProjectAdded(name) => (
                NoticeSeverity::Info,
                tr!("tui.notice.projectAdded", name = name),
            ),
            Notice::ProjectRemoved(name) => (
                NoticeSeverity::Info,
                tr!("tui.notice.projectRemoved", name = name),
            ),
            Notice::Archived(title) => (
                NoticeSeverity::Info,
                tr!("tui.notice.archived", title = title),
            ),
            Notice::ArchivedCount(count) => (
                NoticeSeverity::Info,
                tr!("tui.notice.archivedCount", count = count),
            ),
            Notice::UnsupportedDeleted(title) => (
                NoticeSeverity::Info,
                tr!(
                    "workbenchConversation.unsupportedDeleted",
                    conversation = title
                ),
            ),
            Notice::Renamed => (NoticeSeverity::Info, tr!("tui.notice.renamed")),
            Notice::MemorySuppressed => (
                NoticeSeverity::Info,
                tr!("workspaceMemory.suppressedNotice"),
            ),
            Notice::MemoryDeleted => (NoticeSeverity::Info, tr!("workspaceMemory.deletedNotice")),
            Notice::Goal(status) => (
                NoticeSeverity::Info,
                match status {
                    None => tr!("conversation.goalNotice.cleared"),
                    Some(SessionGoalStatus::Active) => tr!("conversation.goalNotice.resumed"),
                    Some(SessionGoalStatus::Paused) => tr!("conversation.goalNotice.paused"),
                    Some(SessionGoalStatus::Blocked) => tr!("conversation.goalNotice.blocked"),
                    Some(SessionGoalStatus::Complete) => tr!("conversation.goalNotice.completed"),
                    Some(SessionGoalStatus::BudgetLimited) => {
                        tr!("conversation.goalNotice.budgetExhausted")
                    }
                    Some(SessionGoalStatus::UsageLimited) => {
                        tr!("conversation.goalNotice.usageLimited")
                    }
                },
            ),
            Notice::AgentStopped(name) => (
                NoticeSeverity::Info,
                tr!("tui.notice.agentStopped", name = name),
            ),
            Notice::AgentClosed(name) => (
                NoticeSeverity::Info,
                tr!("tui.notice.agentClosed", name = name),
            ),
            Notice::ServiceUnavailable(detail) => (
                NoticeSeverity::Warning,
                tr!("tui.notice.serviceUnavailable", detail = detail),
            ),
            Notice::StartingService => (NoticeSeverity::Info, tr!("tui.notice.startingService")),
            Notice::ServiceStartFailed(detail) => (
                NoticeSeverity::Error,
                tr!("tui.notice.serviceStartFailed", detail = detail),
            ),
            Notice::NotConnected => (NoticeSeverity::Warning, tr!("tui.notice.notConnected")),
            Notice::Unreadable { source, detail } => (
                NoticeSeverity::Error,
                tr!("tui.notice.unreadable", source = source, detail = detail),
            ),
        };
        self.toast(severity, message);
    }

    /// Keeps the sidebar cursor on an existing, selectable row.
    /// 使侧栏光标保持在存在且可选的行上。
    pub fn fix_sidebar_cursor(&mut self) {
        let rows = self.sidebar_rows();
        if self
            .sidebar_cursor
            .as_ref()
            .is_some_and(|c| rows.contains(c))
        {
            return;
        }
        self.sidebar_cursor = self
            .selected
            .clone()
            .map(SidebarRow::Conversation)
            .filter(|r| rows.contains(r))
            .or_else(|| rows.into_iter().find(SidebarRow::selectable));
    }

    fn session_for(&mut self, conversation_id: &str) -> &mut SessionState {
        self.sessions
            .entry(conversation_id.to_string())
            .or_insert_with(SessionState::new)
    }

    pub fn composer_is_empty(&self) -> bool {
        self.composer.lines().iter().all(|l| l.trim().is_empty())
    }

    pub fn composer_text(&self) -> String {
        self.composer.lines().join("\n")
    }

    pub fn reset_composer(&mut self) {
        self.composer = new_composer();
        self.pastes.clear();
        self.paste_serial = 0;
    }

    /// Asks the backend for the diff of the file under the Git cursor when it is not cached; a
    /// failed file is asked again, so stepping onto it retries.
    /// 当 Git 光标所在文件的 diff 未缓存时向后端请求；失败的文件会再次请求，因此切到该文件即重试。
    pub fn request_visible_diff(&mut self) {
        let key = match &self.overlay {
            Some(Overlay::Diff(view)) => self
                .git
                .ordered_files()
                .get(view.file_index)
                .and_then(|file| self.git.diff_key(file)),
            _ => None,
        };
        if let Some(key) = key
            && !self.git.diffs.contains_key(&key)
        {
            self.git.diff_failures.remove(&key);
            self.backend
                .send(Command::LoadWorkspaceDiffFileDetail { key });
        }
    }

    /// Lists pinned-first projects with their conversations, then unlinked conversations.
    /// 先列出置顶项目及其会话，再列出未绑定项目的会话。
    pub fn sidebar_rows(&self) -> Vec<SidebarRow> {
        // A search keeps matching projects open and drops projects with neither a name nor a
        // conversation match.
        // 搜索时保持匹配项目展开，并去掉名称与会话均不匹配的项目。
        let query = self
            .sidebar_filter
            .as_ref()
            .map(|filter| filter.query.trim().to_lowercase())
            .filter(|query| !query.is_empty());
        let matches = |text: &str| {
            query
                .as_ref()
                .is_none_or(|query| text.to_lowercase().contains(query))
        };
        let mut rows = Vec::new();
        let mut projects: Vec<&Project> = self.projects.iter().collect();
        projects.sort_by_key(|p| !p.pinned);
        for project in projects {
            let name_matches = matches(&project.name);
            let conversations: Vec<&ConversationSummary> = self
                .conversations
                .iter()
                .filter(|c| c.project_id.as_ref() == Some(&project.id))
                .filter(|c| name_matches || matches(&c.title))
                .collect();
            if !name_matches && conversations.is_empty() {
                continue;
            }
            rows.push(SidebarRow::Project(project.id.clone()));
            if query.is_none() && self.collapsed_projects.contains(&project.id) {
                continue;
            }
            rows.extend(
                conversations
                    .into_iter()
                    .map(|c| SidebarRow::Conversation(c.id.clone())),
            );
        }
        let unlinked: Vec<SidebarRow> = self
            .conversations
            .iter()
            .filter(|c| {
                c.project_id
                    .as_ref()
                    .is_none_or(|id| !self.projects.iter().any(|p| &p.id == id))
                    && matches(&c.title)
            })
            .map(|c| SidebarRow::Conversation(c.id.clone()))
            .collect();
        if !unlinked.is_empty() {
            rows.push(SidebarRow::Unlinked);
            rows.extend(unlinked);
        }
        rows
    }

    /// Marks a monitored repository at once and asks the backend for its details, which it serves
    /// from memory once loaded.
    /// 立即标记受监听仓库并向后端请求其详情；已加载的仓库由后端从内存提供。
    pub fn select_git_repository(&mut self, key: String) {
        if self.git.marked() == Some(&key) {
            return;
        }
        self.git.pending = Some(key.clone());
        self.git.cursor = 0;
        self.backend.send(Command::SelectGitRepository(key));
    }

    pub fn select_conversation(&mut self, id: ConversationId) {
        if self.selected.as_ref() == Some(&id) {
            return;
        }
        self.selected = Some(id.clone());
        self.sidebar_cursor = Some(SidebarRow::Conversation(id.clone()));
        self.ask = AskState::default();
        self.backend.send(Command::SelectConversation(id));
    }
}

/// Maps successful service functions to user-facing copy, without exposing RPC parameters.
/// 将成功的服务函数映射到面向用户的文案，不暴露 RPC 参数。
fn completed_notice(function: &str) -> String {
    match function {
        "add_public_ai_connection" => tr!("tui.notice.credentialAdded"),
        "rename_knowledge_library" => tr!("tui.notice.renamed"),
        "delete_knowledge_library"
        | "remove_public_ai_connection"
        | "uninstall_luaskill"
        | "uninstall_user_plugin"
        | "delete_agent_skill"
        | "delete_subagent_definition"
        | "delete_archived_conversations" => tr!("tui.notice.deleted"),
        "restore_archived_conversations" => tr!("tui.notice.completed"),
        "import_knowledge_directory" => {
            tr!("tui.notice.submitted")
        }
        "test_public_ai_inference_route" => tr!("tui.notice.testPassed"),
        "invoke_plugin_action" => tr!("plugins.settings.actionDone"),
        function if function.starts_with("save_") || function.starts_with("select_") => {
            tr!("tui.notice.settingsSaved")
        }
        _ => tr!("tui.notice.completed"),
    }
}

/// Creates an empty composer with the shared key bindings and user-facing placeholder.
/// 创建空白输入框，使用共用按键绑定与面向用户的占位文案。
pub fn new_composer() -> TextArea<'static> {
    let mut composer = TextArea::default();
    composer.set_wrap_mode(WrapMode::WordOrGlyph);
    composer.set_placeholder_text(tr!("composer.placeholder", shortcut = "Enter"));
    composer
}
