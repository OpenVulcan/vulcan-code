// System areas and their pages: settings, memory, knowledge, agents, skills and plugins.
// 系统区域及其页面：设置、记忆、知识库、代理、技能与插件。
//
// Pages are declarative: `rows` turns the current `SystemSnapshot` into a flat list of rows, and
// one generic view plus one generic input handler serve every page. A row only describes what it
// shows and which `Edit` or `UiAction` it triggers; nothing here talks to the backend directly.
// Page builders live in one submodule per area; this module holds the shared row vocabulary.
// 页面是声明式的：`rows` 将当前 `SystemSnapshot` 转换为扁平的行列表，由一个通用视图与一个通用输入
// 处理器服务所有页面。行只描述显示内容与触发的 `Edit` 或 `UiAction`；此处不直接访问后端。
// 页面构建函数按区域拆分为子模块；本模块只保存共用的行词汇。

mod catalog;
mod knowledge;
mod memory;
mod plugins;
mod settings;

use std::collections::HashSet;

use super::App;
use super::action::{ConfirmDialog, Edit, PromptDialog, PromptThen, UiAction};
use super::commands::AppCommand;
use crate::backend::Command;
use crate::model::*;
use crate::tr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Area {
    Chat,
    AgentSkills,
    Plugins,
    Agents,
    Memory,
    Knowledge,
    Settings,
}

impl Area {
    /// Areas in the same order as the WebUI's activity bar.
    /// 与 WebUI 活动栏相同顺序的区域。
    pub const ALL: [Area; 7] = [
        Area::Chat,
        Area::AgentSkills,
        Area::Plugins,
        Area::Agents,
        Area::Memory,
        Area::Knowledge,
        Area::Settings,
    ];

    pub fn label(self) -> String {
        match self {
            Area::Chat => tr!("area.chat"),
            Area::AgentSkills => tr!("area.agentSkills"),
            Area::Plugins => tr!("area.skills"),
            Area::Agents => tr!("area.agents"),
            Area::Memory => tr!("area.memory"),
            Area::Knowledge => tr!("area.knowledge"),
            Area::Settings => tr!("area.settings"),
        }
    }

    pub fn description(self) -> String {
        match self {
            Area::Chat => tr!("area.chat.description"),
            Area::AgentSkills => tr!("area.agentSkills.description"),
            Area::Plugins => tr!("area.skills.description"),
            Area::Agents => tr!("area.agents.description"),
            Area::Memory => tr!("area.memory.description"),
            Area::Knowledge => tr!("area.knowledge.description"),
            Area::Settings => tr!("area.settings.description"),
        }
    }

    /// Navigation column: optional group headers followed by their pages.
    /// 导航列：可选的分组标题及其页面。
    pub fn nav(self) -> Vec<(Option<String>, Vec<PageId>)> {
        use PageId::*;
        match self {
            Area::Chat => Vec::new(),
            Area::AgentSkills => vec![(None, vec![SkillList])],
            Area::Plugins => vec![
                (Some("LuaSkills".into()), vec![LuaSkills, LuaSettings]),
                (
                    Some(tr!("area.contextEngine")),
                    vec![IndexProjects, EngineSettings],
                ),
                (Some(tr!("skills.navigation.plugins")), vec![VulcanPlugins]),
            ],
            Area::Agents => vec![(None, vec![AgentList])],
            Area::Memory => vec![
                (None, vec![MemoryOverview]),
                (
                    Some(tr!("tui.memory.groupData")),
                    vec![
                        MemoryContent,
                        MemoryProfile,
                        MemoryProjects,
                        MemoryMaintenance,
                    ],
                ),
                (
                    Some(tr!("tui.memory.groupConfig")),
                    vec![
                        MemoryRules,
                        MemoryInference,
                        MemoryResources,
                        MemoryAdvanced,
                    ],
                ),
            ],
            Area::Knowledge => vec![(
                None,
                vec![KnowledgeBases, KnowledgeLibrary, KnowledgeSearch],
            )],
            Area::Settings => vec![
                (
                    Some(tr!("settings.group.general")),
                    vec![Appearance, Context, Notifications, GitCommit],
                ),
                (
                    Some(tr!("settings.group.models")),
                    vec![Providers, Credentials, Models],
                ),
                (
                    Some(tr!("settings.group.aiCapabilities")),
                    vec![Capabilities],
                ),
                (Some(tr!("settings.group.extensions")), vec![Mcp]),
                (Some(tr!("settings.group.data")), vec![Archived]),
                (Some(tr!("settings.group.about")), vec![Diagnostics, About]),
            ],
        }
    }

    /// Pages in navigation order.
    /// 按导航顺序排列的页面。
    pub fn pages(self) -> Vec<PageId> {
        self.nav()
            .into_iter()
            .flat_map(|(_, pages)| pages)
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PageId {
    Appearance,
    Context,
    Notifications,
    GitCommit,
    Providers,
    Credentials,
    Models,
    Capabilities,
    Mcp,
    Archived,
    Diagnostics,
    About,
    MemoryOverview,
    MemoryContent,
    MemoryProfile,
    MemoryProjects,
    MemoryMaintenance,
    MemoryRules,
    MemoryInference,
    MemoryResources,
    MemoryAdvanced,
    KnowledgeBases,
    KnowledgeLibrary,
    KnowledgeSearch,
    AgentList,
    SkillList,
    LuaSkills,
    LuaSettings,
    IndexProjects,
    EngineSettings,
    VulcanPlugins,
}

impl PageId {
    pub fn label(self) -> String {
        match self {
            PageId::Appearance => tr!("tui.settings.appearance"),
            PageId::Context => tr!("tui.settings.context"),
            PageId::Notifications => tr!("settings.notifications"),
            PageId::GitCommit => tr!("settings.gitCommit"),
            PageId::Providers => tr!("tui.settings.providers"),
            PageId::Credentials => tr!("tui.settings.credentials"),
            PageId::Models => tr!("tui.settings.modelList"),
            PageId::Capabilities => tr!("settings.aiCapabilities"),
            PageId::Mcp => tr!("tui.settings.mcp"),
            PageId::Archived => tr!("settings.misc.archivedConversations"),
            PageId::Diagnostics => tr!("settings.diagnostics"),
            PageId::About => tr!("settings.group.about"),
            PageId::MemoryOverview => tr!("tui.memory.overview"),
            PageId::MemoryContent => tr!("memory.content.title"),
            PageId::MemoryProfile => tr!("memory.profile.title"),
            PageId::MemoryProjects => tr!("memory.identity.nativeTitle"),
            PageId::MemoryMaintenance => tr!("memory.maintenance.title"),
            PageId::MemoryRules => tr!("memory.rules.title"),
            PageId::MemoryInference => tr!("settings.inference.title"),
            PageId::MemoryResources => tr!("memory.resources.title"),
            PageId::MemoryAdvanced => tr!("memory.advanced.title"),
            PageId::KnowledgeBases => tr!("area.knowledge"),
            PageId::KnowledgeLibrary => tr!("tui.knowledge.library"),
            PageId::KnowledgeSearch => tr!("knowledge.search"),
            PageId::AgentList => tr!("area.agents"),
            PageId::SkillList => tr!("area.agentSkills"),
            PageId::LuaSkills => tr!("skills.navigation.list"),
            PageId::LuaSettings => tr!("skills.navigation.luaSettings"),
            PageId::IndexProjects => tr!("codeContext.project.projectSemanticIndex"),
            PageId::EngineSettings => tr!("tui.index.engineSettings"),
            PageId::VulcanPlugins => tr!("skills.navigation.externalPlugins"),
        }
    }

    pub fn area(self) -> Area {
        Area::ALL
            .into_iter()
            .find(|area| area.pages().contains(&self))
            .unwrap_or(Area::Settings)
    }

    /// Setting-key prefixes whose drafts this page owns; its save bar only counts these.
    /// 本页拥有的草稿设置键前缀；其保存栏只统计这些草稿。
    pub fn draft_prefixes(self) -> &'static [&'static str] {
        match self {
            PageId::Context => &["context."],
            PageId::MemoryRules => &["memory.rules.", "memory.policy.", "memory.ranking."],
            PageId::MemoryResources => &["memory.resources."],
            PageId::MemoryAdvanced => &["memory.advanced."],
            PageId::LuaSettings => &["lua."],
            PageId::EngineSettings => &["engine."],
            _ => &[],
        }
    }

    /// Small count shown beside the page in the navigation column.
    /// 导航列中页面旁显示的小计数。
    pub fn badge(self, app: &App) -> Option<(String, Tone)> {
        let prefixes = self.draft_prefixes();
        if !prefixes.is_empty() {
            let unsaved = super::action::draft_keys(app, prefixes).len();
            return (unsaved > 0).then(|| (format!("● {unsaved}"), Tone::Warn));
        }
        let system = app.system.as_ref()?;
        let count = |n: usize, tone: Tone| (n > 0).then(|| (n.to_string(), tone));
        match self {
            PageId::MemoryContent => count(system.memory.items.len(), Tone::Muted),
            PageId::MemoryProjects => count(system.memory.projects.len(), Tone::Muted),
            PageId::KnowledgeBases => count(system.knowledge.len(), Tone::Muted),
            PageId::Archived => count(system.archived.len(), Tone::Muted),
            PageId::Mcp => count(system.mcp_servers.len(), Tone::Muted),
            PageId::AgentList => count(system.agents.len(), Tone::Muted),
            PageId::SkillList => {
                let issues = system.skills.iter().filter(|s| s.issue.is_some()).count();
                count(issues, Tone::Bad).or_else(|| count(system.skills.len(), Tone::Muted))
            }
            PageId::LuaSkills => {
                let updates = system
                    .lua_skills
                    .iter()
                    .filter(|s| s.update.is_some())
                    .count();
                count(updates, Tone::Warn).or_else(|| count(system.lua_skills.len(), Tone::Muted))
            }
            PageId::IndexProjects => {
                let stale = system.index.iter().filter(|p| p.status != "ready").count();
                count(stale, Tone::Warn).or_else(|| count(system.index.len(), Tone::Muted))
            }
            PageId::VulcanPlugins => count(system.plugins.len(), Tone::Muted),
            _ => None,
        }
    }

    /// The page's search prompt, reached with `/`.
    /// 页面的搜索输入框，通过 `/` 打开。
    pub fn search(self) -> Option<UiAction> {
        match self {
            PageId::MemoryContent => Some(prompt(
                tr!("memory.content.search"),
                String::new(),
                PromptThen::MemorySearch,
                false,
            )),
            PageId::KnowledgeSearch | PageId::KnowledgeBases | PageId::KnowledgeLibrary => {
                Some(prompt(
                    tr!("knowledge.search"),
                    String::new(),
                    PromptThen::KnowledgeSearch,
                    false,
                ))
            }
            _ => None,
        }
    }
}

/// Per-page cursor and scroll state.
/// 每个页面的光标与滚动状态。
#[derive(Debug, Default, Clone)]
pub struct PageState {
    /// Index into `rows`; always a selectable row when any exists.
    /// `rows` 中的下标；存在可选行时总指向可选行。
    pub cursor: usize,
    pub scroll: usize,
    /// Highlighted action chip of an `Item` row.
    /// `Item` 行中高亮的操作按钮。
    pub action: usize,
    pub expanded: HashSet<String>,
    /// The wheel moved the view away from the cursor; keyboard movement re-attaches it.
    /// 滚轮使视图脱离光标；键盘移动会重新跟随。
    pub free_scroll: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Normal,
    Muted,
    Good,
    Warn,
    Bad,
    Accent,
}

impl Tone {
    pub fn of(severity: Severity) -> Tone {
        match severity {
            Severity::Info => Tone::Good,
            Severity::Warning => Tone::Warn,
            Severity::Error => Tone::Bad,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RowAction {
    pub label: String,
    pub action: UiAction,
    pub tone: Tone,
}

#[derive(Debug, Clone)]
pub enum RowKind {
    Section,
    Info {
        value: String,
        tone: Tone,
    },
    /// Wrapped paragraph; `label` is the text.
    /// 自动换行的段落；`label` 即正文。
    Note {
        tone: Tone,
    },
    Toggle {
        on: bool,
        edit: Edit,
    },
    Choice {
        options: Vec<(String, String)>,
        current: String,
        edit: Edit,
    },
    Number {
        value: i64,
        min: i64,
        max: i64,
        step: i64,
        edit: Edit,
    },
    Text {
        value: String,
        edit: Edit,
        secret: bool,
    },
    Button {
        action: UiAction,
        tone: Tone,
    },
    Item {
        id: String,
        badge: Option<(String, Tone)>,
        actions: Vec<RowAction>,
        lines: Vec<String>,
        job: Option<Job>,
    },
    /// Labelled figures laid out as tiles.
    /// 以卡片排列的带标签数值。
    Stats(Vec<(String, String, Tone)>),
    /// A gauge after the label.
    /// 标签后的进度条。
    Meter {
        ratio: f64,
        text: String,
        tone: Tone,
    },
    Logo,
    Spacer,
}

#[derive(Debug, Clone)]
pub struct Row {
    pub kind: RowKind,
    pub label: String,
    pub detail: String,
}

impl Row {
    fn new(kind: RowKind, label: impl Into<String>) -> Row {
        Row {
            kind,
            label: label.into(),
            detail: String::new(),
        }
    }

    fn detail(mut self, detail: impl Into<String>) -> Row {
        self.detail = detail.into();
        self
    }

    pub fn selectable(&self) -> bool {
        match &self.kind {
            RowKind::Toggle { .. }
            | RowKind::Choice { .. }
            | RowKind::Number { .. }
            | RowKind::Text { .. }
            | RowKind::Button { .. } => true,
            RowKind::Item { actions, lines, .. } => !actions.is_empty() || !lines.is_empty(),
            _ => false,
        }
    }
}

fn section(label: String) -> Row {
    Row::new(RowKind::Section, label)
}

fn info(label: String, value: impl Into<String>, tone: Tone) -> Row {
    Row::new(
        RowKind::Info {
            value: value.into(),
            tone,
        },
        label,
    )
}

fn note(text: String, tone: Tone) -> Row {
    Row::new(RowKind::Note { tone }, text)
}

fn toggle(label: String, on: bool, edit: Edit) -> Row {
    Row::new(RowKind::Toggle { on, edit }, label)
}

fn button(label: String, action: UiAction, tone: Tone) -> Row {
    Row::new(RowKind::Button { action, tone }, label)
}

fn action(label: String, action: UiAction, tone: Tone) -> RowAction {
    RowAction {
        label,
        action,
        tone,
    }
}

fn item(id: &str, label: impl Into<String>, detail: impl Into<String>) -> Row {
    Row::new(
        RowKind::Item {
            id: id.to_string(),
            badge: None,
            actions: Vec::new(),
            lines: Vec::new(),
            job: None,
        },
        label,
    )
    .detail(detail)
}

impl Row {
    fn badge(mut self, text: impl Into<String>, tone: Tone) -> Row {
        if let RowKind::Item { badge, .. } = &mut self.kind {
            *badge = Some((text.into(), tone));
        }
        self
    }

    fn actions(mut self, list: Vec<RowAction>) -> Row {
        if let RowKind::Item { actions, .. } = &mut self.kind {
            *actions = list;
        }
        self
    }

    fn lines(mut self, list: Vec<String>) -> Row {
        if let RowKind::Item { lines, .. } = &mut self.kind {
            *lines = list;
        }
        self
    }

    fn job(mut self, value: Option<Job>) -> Row {
        if let RowKind::Item { job, .. } = &mut self.kind {
            *job = value;
        }
        self
    }
}

fn prompt(title: String, description: String, then: PromptThen, secret: bool) -> UiAction {
    UiAction::Prompt(Box::new(PromptDialog::new(
        title,
        description,
        String::new(),
        then,
        secret,
    )))
}

fn confirm(title: String, body: String, confirm_label: String, then: UiAction) -> UiAction {
    UiAction::Confirm(Box::new(ConfirmDialog {
        title,
        body,
        confirm_label,
        danger: true,
        then,
        focus_confirm: false,
    }))
}

fn sys_action(command: SystemCommand) -> UiAction {
    UiAction::Send(Command::System(command))
}

fn on_off(on: bool) -> &'static str {
    if on { "●" } else { "○" }
}

fn switch(label: String, on: bool, edit: Edit) -> RowAction {
    action(
        format!("{} {label}", on_off(on)),
        UiAction::Edit(edit, SettingValue::Bool(!on)),
        if on { Tone::Good } else { Tone::Muted },
    )
}

pub fn service_status(status: ServiceStatus) -> (String, Tone) {
    match status {
        ServiceStatus::Running => (tr!("tui.status.running"), Tone::Good),
        ServiceStatus::Starting => (tr!("tui.status.starting"), Tone::Warn),
        ServiceStatus::Stopping => (tr!("tui.status.stopping"), Tone::Warn),
        ServiceStatus::Stopped => (tr!("tui.status.stopped"), Tone::Muted),
        ServiceStatus::Failed => (tr!("tui.status.failed"), Tone::Bad),
    }
}

pub fn capability_label(key: &str) -> String {
    match key {
        "aiWebSearch" => tr!("settings.aiWebSearch"),
        "aiWebExtract" => tr!("settings.aiWebExtract"),
        "aiUnderstandImage" => tr!("settings.aiUnderstandImage"),
        "aiUnderstandVideo" => tr!("settings.aiUnderstandVideo"),
        "aiUnderstandAudio" => tr!("settings.aiUnderstandAudio"),
        "aiImageGeneration" => tr!("settings.aiImageGeneration"),
        "aiVideoGeneration" => tr!("settings.aiVideoGeneration"),
        "aiSpeechSynthesis" => tr!("settings.aiSpeechSynthesis"),
        "aiMusicGeneration" => tr!("settings.aiMusicGeneration"),
        "aiMusicCover" => tr!("settings.aiMusicCover"),
        other => other.to_string(),
    }
}

/// Value shown for a setting: the unsaved draft when present, else the stored value.
/// 设置项显示的值：存在未保存草稿时取草稿，否则取已存储的值。
pub fn setting_value(app: &App, key: &str) -> Option<SettingValue> {
    app.drafts
        .get(key)
        .cloned()
        .or_else(|| app.system.as_ref()?.setting(key).cloned())
}

fn spacer() -> Row {
    Row::new(RowKind::Spacer, "")
}

/// A strip of labelled figures, e.g. document, chunk and status counts.
/// 一排带标签的数值，例如文档、分块与状态计数。
fn stats(tiles: Vec<(String, String, Tone)>) -> Row {
    Row::new(RowKind::Stats(tiles), "")
}

/// A labelled gauge; `ratio` is clamped to `0..=1`.
/// 带标签的进度条；`ratio` 被限制在 `0..=1`。
fn meter(label: String, ratio: f64, text: String, tone: Tone) -> Row {
    Row::new(
        RowKind::Meter {
            ratio: ratio.clamp(0.0, 1.0),
            text,
            tone,
        },
        label,
    )
}

fn choice(label: String, options: Vec<(String, String)>, current: String, edit: Edit) -> Row {
    Row::new(
        RowKind::Choice {
            options,
            current,
            edit,
        },
        label,
    )
}

/// A number row; `range` is `(min, max, step)`.
/// 数值行；`range` 为 `(最小值, 最大值, 步长)`。
fn number(label: String, value: i64, range: (i64, i64, i64), edit: Edit) -> Row {
    Row::new(
        RowKind::Number {
            value,
            min: range.0,
            max: range.1,
            step: range.2,
            edit,
        },
        label,
    )
}

fn text(label: String, value: String, edit: Edit) -> Row {
    Row::new(
        RowKind::Text {
            value,
            edit,
            secret: false,
        },
        label,
    )
}

fn flag(app: &App, key: &str) -> bool {
    setting_value(app, key).is_some_and(|v| v.as_bool())
}

fn count(app: &App, key: &str) -> i64 {
    setting_value(app, key).map_or(0, |v| v.as_number())
}

fn string(app: &App, key: &str) -> String {
    setting_value(app, key)
        .map(|v| v.as_str().to_string())
        .unwrap_or_default()
}

fn draft(key: &str) -> Edit {
    Edit::Draft(key.to_string())
}

fn immediate(key: &str) -> Edit {
    Edit::Setting(key.to_string())
}

/// Save and discard buttons for the page's own drafts; nothing when it has none. A `problem`
/// (failed cross-field validation) is shown instead of the save button.
/// 本页草稿的保存与放弃按钮；没有草稿时不显示。存在 `problem`（跨字段校验失败）时以其替代保存按钮。
fn draft_footer(app: &App, page: PageId, rows: &mut Vec<Row>, problem: Option<String>) {
    let prefixes = page.draft_prefixes();
    let unsaved = super::action::draft_keys(app, prefixes).len();
    if unsaved == 0 {
        return;
    }
    rows.push(spacer());
    rows.push(note(
        tr!("tui.settings.unsaved", count = unsaved),
        Tone::Warn,
    ));
    if let Some(problem) = problem {
        rows.push(note(problem, Tone::Bad));
    } else {
        rows.push(button(
            tr!("memory.common.save"),
            UiAction::SaveDrafts(prefixes.to_vec()),
            Tone::Accent,
        ));
    }
    rows.push(button(
        tr!("memory.common.cancel"),
        UiAction::DiscardDrafts(prefixes.to_vec()),
        Tone::Muted,
    ));
}

/// Model choices for internal purposes: an empty "not configured" entry plus internal models.
/// 内部用途的模型选项：一个空的“未配置”项加上内部模型。
fn internal_models(system: &SystemSnapshot, empty: String) -> Vec<(String, String)> {
    let mut options = vec![(String::new(), empty)];
    options.extend(
        system
            .models
            .iter()
            .filter(|m| m.internal_enabled)
            .map(|m| (m.id.clone(), m.label.clone())),
    );
    options
}

/// Builds the rows of one page from the current state.
/// 根据当前状态构建某个页面的行。
pub fn rows(app: &App, page: PageId) -> Vec<Row> {
    if page == PageId::Appearance {
        return settings::appearance(app);
    }
    let Some(system) = app.system.as_ref() else {
        return vec![note(tr!("tui.page.loading"), Tone::Muted)];
    };
    match page {
        PageId::Appearance => Vec::new(),
        PageId::Context => settings::context(app),
        PageId::Notifications => settings::notifications(app),
        PageId::GitCommit => settings::git_commit(app, system),
        PageId::Providers => settings::providers(system),
        PageId::Credentials => settings::credentials(system),
        PageId::Models => settings::models(system),
        PageId::Capabilities => settings::capabilities(system),
        PageId::Mcp => settings::mcp(system),
        PageId::Archived => settings::archived(system),
        PageId::Diagnostics => settings::diagnostics(app, system),
        PageId::About => settings::about(app, system),
        PageId::MemoryOverview => memory::overview(system),
        PageId::MemoryContent => memory::content(system),
        PageId::MemoryProfile => memory::profile(system),
        PageId::MemoryProjects => memory::projects(system),
        PageId::MemoryMaintenance => memory::maintenance(system),
        PageId::MemoryRules => memory::rules(app, system),
        PageId::MemoryInference => memory::inference(app, system),
        PageId::MemoryResources => memory::resources(app, system),
        PageId::MemoryAdvanced => memory::advanced(app, system),
        PageId::KnowledgeBases => knowledge::libraries(app, system),
        PageId::KnowledgeLibrary => knowledge::library(app, system),
        PageId::KnowledgeSearch => knowledge::search(system),
        PageId::AgentList => catalog::agents(system),
        PageId::SkillList => catalog::skills(system),
        PageId::LuaSkills => plugins::lua_skills(system),
        PageId::LuaSettings => plugins::lua_settings(app, system),
        PageId::IndexProjects => plugins::index(app, system),
        PageId::EngineSettings => plugins::engine(app, system),
        PageId::VulcanPlugins => plugins::plugins(system),
    }
}

fn job_rows(rows: &mut Vec<Row>, jobs: &[Job]) {
    if jobs.is_empty() {
        return;
    }
    rows.push(section(tr!("tui.jobs.title")));
    for job in jobs {
        let (badge, tone) = if job.failed {
            (tr!("tui.status.failed"), Tone::Bad)
        } else if job.done {
            (tr!("tui.jobs.done"), Tone::Good)
        } else {
            (job.detail.clone(), Tone::Warn)
        };
        rows.push(
            item(&job.id, job.label.clone(), String::new())
                .badge(badge, tone)
                .job((!job.done).then(|| job.clone())),
        );
    }
}

/// Commands that open a system area.
/// 打开系统区域的命令。
pub fn area_command(area: Area) -> AppCommand {
    match area {
        Area::Chat => AppCommand::Chat,
        Area::AgentSkills => AppCommand::Skills,
        Area::Plugins => AppCommand::Plugins,
        Area::Agents => AppCommand::AgentsArea,
        Area::Memory => AppCommand::Memory,
        Area::Knowledge => AppCommand::Knowledge,
        Area::Settings => AppCommand::Settings,
    }
}
