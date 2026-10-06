// System areas and their pages: settings, memory, knowledge, agents, skills and plugins.
// 系统区域及其页面：设置、记忆、知识库、代理、技能与插件。
//
// Pages are declarative: `rows` turns the current WebUI DTO slots into a flat list of rows, and
// one generic view plus one generic input handler serve every page. A row only describes what it
// shows and which `Edit` or `UiAction` it triggers; nothing here talks to the backend directly.
// Page builders live in one submodule per area; this module holds the shared row vocabulary.
// 页面是声明式的：`rows` 将当前 WebUI DTO slots 转换为扁平的行列表，由一个通用视图与一个通用输入
// 处理器服务所有页面。行只描述显示内容与触发的 `Edit` 或 `UiAction`；此处不直接访问后端。
// 页面构建函数按区域拆分为子模块；本模块只保存共用的行词汇。

mod catalog;
mod inference;
mod knowledge;
mod memory;
mod navigation;
mod plugin_settings;
mod plugins;
mod settings;

use std::collections::HashSet;

use super::App;
use super::action::{ConfirmDialog, Edit, LocalSetting, PromptDialog, PromptThen, UiAction};
use super::commands::AppCommand;
use crate::backend::Command;
use crate::model::*;
use crate::service_api::{self as api, ServiceCall, Slot, setter::Setter};
use crate::tr;
use serde_json::{Value, json};

pub use knowledge::knowledge_view;
pub use navigation::navigation;
pub use plugin_settings::{PERIOD_SEPARATOR, reads as plugin_reads, tab_reads as plugin_tab_reads};

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
    /// Areas in the same order as the WebUI's activity bar; Knowledge is a plugin page there, so it
    /// follows Plugins.
    /// 与 WebUI 活动栏相同顺序的区域；知识库在其中是插件页面，因此紧随插件之后。
    pub const ALL: [Area; 7] = [
        Area::Chat,
        Area::Plugins,
        Area::Knowledge,
        Area::AgentSkills,
        Area::Agents,
        Area::Memory,
        Area::Settings,
    ];

    /// Area an entry is nested under in the activity bar: plugin-provided areas sit beneath
    /// Plugins, as plugin detail pages do in the WebUI.
    /// 活动栏中该条目所嵌套的区域：插件提供的区域位于插件之下，与 WebUI 的插件详情页一致。
    pub fn parent(self) -> Option<Area> {
        match self {
            Area::Knowledge => Some(Area::Plugins),
            _ => None,
        }
    }

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

    /// Static page inventory and fixed-area navigation; dynamic catalogs use navigation(app).
    /// 静态页面清单及固定区域导航；动态目录使用 navigation(app)。
    pub fn nav(self) -> Vec<(Option<String>, Vec<PageId>)> {
        use PageId::*;
        match self {
            Area::Chat => Vec::new(),
            Area::AgentSkills => vec![(None, vec![SkillList])],
            Area::Plugins => vec![
                (Some("LuaSkills".into()), vec![LuaSkills, LuaSettings]),
                (Some(tr!("settings.mcpServers")), vec![Mcp]),
                (
                    Some(tr!("area.contextEngine")),
                    vec![IndexProjects, EngineSettings],
                ),
                (
                    Some(tr!("skills.navigation.plugins")),
                    vec![VulcanPlugins, PluginMount],
                ),
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
                vec![
                    KnowledgeBases,
                    KnowledgeLibrary,
                    KnowledgeDocuments,
                    KnowledgeDocument,
                    KnowledgeSearch,
                    KnowledgeSettings,
                    KnowledgeSources,
                    KnowledgeBindings,
                    KnowledgeMaintenance,
                ],
            )],
            Area::Settings => vec![
                (
                    Some(tr!("settings.group.general")),
                    vec![Appearance, Context, GitCommit],
                ),
                (
                    Some(tr!("settings.group.models")),
                    vec![Providers, Credentials, Models, Capabilities],
                ),
                (Some(tr!("settings.group.data")), vec![Archived]),
                (Some(tr!("settings.group.about")), vec![Diagnostics, About]),
            ],
        }
    }

    /// Semantic page inventory, independent from category and library navigation row counts.
    /// 语义页面清单，独立于分类及知识库导航行数。
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
    /// Browses paged managed documents in the selected library.
    /// 浏览所选知识库中分页的托管文档。
    KnowledgeDocuments,
    /// Reads one exact document in its raw or display corpus.
    /// 阅读原始或展示语料中的单个准确文档。
    KnowledgeDocument,
    /// Edits the selected library's retrieval and inference policy.
    /// 编辑所选知识库的检索及推理策略。
    KnowledgeSettings,
    /// Manages directory sources for the selected library.
    /// 管理所选知识库的目录来源。
    KnowledgeSources,
    /// Manages project authorization and default inclusion.
    /// 管理项目授权及默认使用状态。
    KnowledgeBindings,
    /// Observes and maintains indexes and the retained operation job.
    /// 观察及维护索引和保留的操作任务。
    KnowledgeMaintenance,
    AgentList,
    SkillList,
    LuaSkills,
    LuaSettings,
    IndexProjects,
    EngineSettings,
    VulcanPlugins,
    /// The open plugin detail mount; navigation lists one row per runtime-ready mount.
    /// 当前打开的插件详情挂载；导航为每个运行就绪的挂载列出一行。
    PluginMount,
}

impl PageId {
    pub fn label(self) -> String {
        match self {
            PageId::Appearance => tr!("tui.settings.appearance"),
            PageId::Context => tr!("tui.settings.context"),
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
            PageId::KnowledgeLibrary => tr!("tui.knowledge.overview"),
            PageId::KnowledgeSearch => tr!("knowledge.search"),
            PageId::KnowledgeDocuments => tr!("tui.knowledge.documents"),
            PageId::KnowledgeDocument => tr!("tui.knowledge.document"),
            PageId::KnowledgeSettings => tr!("tui.knowledge.settings"),
            PageId::KnowledgeSources => tr!("tui.field.sources"),
            PageId::KnowledgeBindings => tr!("tui.field.projectBindings"),
            PageId::KnowledgeMaintenance => tr!("tui.knowledge.maintenance"),
            PageId::AgentList => tr!("area.agents"),
            PageId::SkillList => tr!("area.agentSkills"),
            PageId::LuaSkills => tr!("skills.navigation.list"),
            PageId::LuaSettings => tr!("skills.navigation.luaSettings"),
            PageId::IndexProjects => tr!("codeContext.project.projectSemanticIndex"),
            PageId::EngineSettings => tr!("tui.index.engineSettings"),
            PageId::VulcanPlugins => tr!("skills.navigation.externalPlugins"),
            PageId::PluginMount => tr!("tui.plugin.page"),
        }
    }

    /// Title shown above the page: the open mount's own label for plugin pages.
    /// 页面上方显示的标题：插件页面使用当前挂载自身的名称。
    pub fn title(self, app: &App) -> String {
        match self {
            PageId::PluginMount => plugin_settings::title(app).unwrap_or_else(|| self.label()),
            _ => self.label(),
        }
    }

    pub fn area(self) -> Area {
        Area::ALL
            .into_iter()
            .find(|area| area.pages().contains(&self))
            .unwrap_or(Area::Settings)
    }

    /// Counts entries from the DTO slot owned by this page; returns no badge while loading.
    /// 统计本页所属 DTO 槽位的条目；加载中不显示计数。
    pub fn badge(self, app: &App) -> Option<(String, Tone)> {
        // Each pointer comes from its corresponding WebUI catalog type.
        // 每个指针均来自对应的 WebUI 目录类型。
        let entries = match self {
            Self::Credentials => a(app.slots.value(Slot::PublicAiSettings), "/connections"),
            Self::Models => a(
                app.slots.value(Slot::PublicAiSettings),
                "/modelAccessRecords",
            ),
            Self::KnowledgeBases => a(app.slots.value(Slot::KnowledgeLibraries), ""),
            Self::MemoryProjects => a(app.slots.value(Slot::VmmProjects), "/data/value/projects"),
            Self::AgentList => a(
                app.slots.value(Slot::Workbench),
                "/subagentCatalog/definitions",
            ),
            Self::SkillList => a(app.slots.value(Slot::Workbench), "/agentSkills/records"),
            Self::LuaSkills => a(app.slots.value(Slot::Workbench), "/luaSkills/skills"),
            Self::VulcanPlugins => a(app.slots.value(Slot::Workbench), "/plugins"),
            Self::Archived => a(app.slots.value(Slot::Workbench), "/archivedConversations"),
            Self::Mcp => a(app.slots.value(Slot::McpRuntime), ""),
            _ => &[],
        };
        (!entries.is_empty()).then(|| (entries.len().to_string(), Tone::Muted))
    }

    /// Returns this page's exact WebUI loaders, scoped to the current library or project.
    /// 返回本页精确的 WebUI 加载器，并限定到当前知识库或项目。
    pub fn loaders(self, app: &App) -> Vec<ServiceCall> {
        use api::{
            knowledge_base as knowledge, memory_actions as memory, settings_actions as settings,
        };
        // Workbench is refreshed separately by its snapshot and event stream.
        // 工作台由其快照与事件流单独刷新。
        let mut calls = Vec::new();
        match self {
            Self::Context => {
                calls.push(settings::load_public_ai_settings());
                calls.push(settings::load_output_style_catalog());
            }
            Self::Providers
            | Self::Credentials
            | Self::Models
            | Self::Capabilities
            | Self::MemoryInference
            | Self::EngineSettings
            | Self::GitCommit => calls.push(settings::load_public_ai_settings()),
            Self::Mcp => calls.push(api::mcp_settings::load_mcp_runtime_snapshot()),
            Self::About => calls.push(api::licenses::load_license_catalog()),
            // The layout's own reads follow its arrival, since they depend on its content.
            // 版面自身的读取在其到达后发出，因为它们取决于版面内容。
            Self::PluginMount => {
                if let Some((plugin, mount)) = &app.plugin_mount {
                    calls.push(api::skills_actions::load_plugin_ui_module(plugin, mount));
                }
            }
            Self::MemoryOverview | Self::MemoryProjects => {
                calls.push(memory::native_project_catalog())
            }
            Self::MemoryContent | Self::MemoryProfile => {
                calls.push(memory::native_project_catalog());
                if let Some(project) = app.vmm_project {
                    calls.push(if self == Self::MemoryContent {
                        memory::native_memory_records(project)
                    } else {
                        memory::native_profile_records(project, &app.profile_target)
                    });
                }
            }
            Self::MemoryMaintenance => calls.extend([
                memory::native_vector_status(),
                memory::native_lexical_status(),
                memory::native_retention_status(),
                memory::native_recycle_bin(),
            ]),
            Self::MemoryRules | Self::MemoryAdvanced => calls.push(memory::native_runtime_policy()),
            Self::MemoryResources => {
                calls.push(memory::native_resource_catalog());
                if let Some(loader) = app.slots.loader(Slot::VmmResourceFile) {
                    calls.push(loader.clone());
                }
            }
            page if page.area() == Area::Knowledge => {
                calls.push(knowledge::list_knowledge_libraries());
                if page == Self::KnowledgeSettings {
                    calls.push(settings::load_public_ai_settings());
                }
                if let Some(library) = &app.library {
                    match page {
                        Self::KnowledgeLibrary | Self::KnowledgeMaintenance => {
                            calls.push(knowledge::get_knowledge_index_integrity(library));
                            calls.push(knowledge::get_knowledge_import_job(library));
                        }
                        Self::KnowledgeSources => {
                            calls.push(knowledge::list_knowledge_sources(library));
                            calls.push(knowledge::get_knowledge_import_job(library));
                        }
                        Self::KnowledgeBindings => {
                            calls.push(knowledge::list_knowledge_project_bindings(library))
                        }
                        Self::KnowledgeDocuments => {
                            calls.push(knowledge::list_managed_knowledge_document_folders(library));
                            // One sentinel record proves whether the next page exists even under a folder filter.
                            // 一个哨兵记录用于确认是否有下一页，目录筛选时同样准确。
                            calls.push(knowledge::list_managed_knowledge_documents(
                                library,
                                app.knowledge_folder.as_deref(),
                                knowledge::DOCUMENT_PAGE_SIZE + 1,
                                app.knowledge_offset,
                            ));
                        }
                        Self::KnowledgeDocument => {
                            if let Some(document) = &app.knowledge_document {
                                calls.push(knowledge::read_managed_knowledge_document(
                                    library,
                                    document,
                                    &app.knowledge_corpus,
                                ));
                            }
                        }
                        Self::KnowledgeSearch if !app.knowledge_query.is_empty() => {
                            calls.push(knowledge::search_managed_knowledge_library(
                                library,
                                &app.knowledge_query,
                                None,
                            ));
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        if matches!(self, Self::IndexProjects | Self::EngineSettings) {
            calls.push(api::vce::load_vce_configuration_status());
        }
        if self == Self::IndexProjects
            && let Some(project) = &app.vce_project
        {
            calls.push(api::vce::load_vce_project_overview(project));
            calls.push(api::vce::load_vce_job(project, None));
        }
        calls
    }

    /// The page's search prompt, reached with `/`.
    /// 页面的搜索输入框，通过 `/` 打开。
    /// `app` owns the retained query; returns None only for a page without search.
    /// `app` 持有保留查询；仅无搜索页面返回 None。
    pub fn search(self, app: &App) -> Option<UiAction> {
        // Retained query belongs to the active catalog/library and is edited in place.
        // 保留查询属于当前目录或知识库，并就地编辑。
        let (title, query, then) = match self {
            PageId::MemoryContent => (
                tr!("memory.content.search"),
                &app.memory_query,
                PromptThen::MemorySearch,
            ),
            PageId::AgentList => (
                tr!("tui.catalog.search"),
                &app.agent_query,
                PromptThen::AgentSearch,
            ),
            PageId::SkillList => (
                tr!("tui.catalog.search"),
                &app.skill_query,
                PromptThen::SkillSearch,
            ),
            page if page.area() == Area::Knowledge => (
                tr!("knowledge.search"),
                &app.knowledge_query,
                PromptThen::KnowledgeSearch,
            ),
            _ => return None,
        };
        Some(UiAction::Prompt(Box::new(PromptDialog::new(
            title,
            String::new(),
            query.clone(),
            then,
            false,
        ))))
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
    /// Literal managed-document content; whitespace stays meaningful and scrolling is view-only.
    /// 托管文档字面正文；空白保持含义，滚动仅改变视图。
    Document {
        /// Unescaped normalized Markdown returned by the document read service.
        /// 文档阅读服务返回的未经转义规范化 Markdown。
        content: String,
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
    /// A sparkline after the label: the latest points that fit, then `text`.
    /// 标签后的迷你走势图：能放下的最近数据点，其后为 `text`。
    Spark {
        points: Vec<f64>,
        text: String,
    },
    /// A gauge after the label.
    /// 标签后的进度条。
    Meter {
        ratio: f64,
        text: String,
        tone: Tone,
    },
    /// The centered text wordmark.
    /// 居中的文字字标。
    Brand,
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

/// Reads a single confirmed JSON pointer; null denotes an optional field or a pending slot.
/// 读取单个已确认的 JSON 指针；空值表示可选字段或尚未加载的槽位。
fn v<'a>(value: &'a Value, pointer: &str) -> &'a Value {
    value.pointer(pointer).unwrap_or(&Value::Null)
}
/// Formats a scalar DTO field for display; compound values are rendered in detail rows.
/// 格式化 DTO 标量字段用于显示；复合值在详情行中呈现。
fn s(value: &Value, pointer: &str) -> String {
    match v(value, pointer) {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        value => value.to_string(),
    }
}
/// Reads an optional boolean DTO field, whose missing value is displayed as disabled.
/// 读取可选的 DTO 布尔字段，缺失值在显示时视为未启用。
fn b(value: &Value, pointer: &str) -> bool {
    v(value, pointer) == &Value::Bool(true)
}
/// Reads an optional count field for display; request builders do not use this default.
/// 读取可选计数字段用于显示；请求构建器不使用此默认值。
fn n(value: &Value, pointer: &str) -> i64 {
    v(value, pointer).as_i64().unwrap_or(0)
}
/// Reads a DTO array at its unique schema pointer; pending optional catalogs have no entries.
/// 在唯一结构指针读取 DTO 数组；尚未加载的可选目录没有条目。
fn a<'a>(value: &'a Value, pointer: &str) -> &'a [Value] {
    v(value, pointer).as_array().map_or(&[], Vec::as_slice)
}
/// Wraps one exact WebUI service call as an action shared by keys and clicks.
/// 将精确 WebUI 服务调用包装为键盘与点击共用的动作。
fn call(request: ServiceCall) -> UiAction {
    UiAction::Send(Command::Call(request))
}
/// Edits a verified pointer in a complete DTO and saves it through its matching API function.
/// 编辑完整 DTO 中已确认的指针，并经对应 API 函数保存。
fn dto(base: Value, pointer: &str, save: fn(Value) -> ServiceCall) -> Edit {
    Edit::Call(Setter::Dto {
        base,
        pointer: pointer.to_string(),
        save,
    })
}
/// Fractional setting input with schema limits, retaining the whole DTO.
/// 带结构限制的小数设置输入，并保留完整 DTO。
fn decimal(
    label: String,
    base: &Value,
    pointer: &str,
    min: f64,
    max: f64,
    save: fn(Value) -> ServiceCall,
) -> Row {
    text(
        label,
        s(base, pointer),
        Edit::Call(Setter::Decimal {
            base: base.clone(),
            pointer: pointer.into(),
            min,
            max,
            save,
        }),
    )
}
/// Multiline string-list input preserving its authoritative array type.
/// 多行字符串列表输入，保留权威数组类型。
fn string_list(label: String, base: &Value, pointer: &str, save: fn(Value) -> ServiceCall) -> Row {
    text(
        label,
        a(base, pointer)
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join("\n"),
        Edit::Call(Setter::StringList {
            base: base.clone(),
            pointer: pointer.into(),
            save,
        }),
    )
}
/// Builds a transient request form; each field is collected before one final service call.
/// 构建临时请求表单；收集全部字段后只发送一次服务调用。
fn form(
    request: Value,
    fields: Vec<super::form::FormField>,
    save: fn(Value) -> ServiceCall,
) -> UiAction {
    UiAction::Form(Box::new(super::form::Form {
        request,
        fields,
        index: 0,
        save: super::form::FormSubmit::Build(save),
    }))
}
/// Creates one request-field prompt using a localized label and an exact API pointer.
/// 使用本地化标签与精确 API 指针创建请求字段输入框。
fn field(
    title: String,
    pointer: &'static str,
    secret: bool,
    optional: bool,
) -> super::form::FormField {
    super::form::FormField {
        title,
        pointer: pointer.into(),
        secret,
        optional,
        parse: |input| Some(Value::String(input.to_string())),
        validate: |_, _| true,
    }
}

/// Parses one string per line without splitting a quoted command argument on spaces.
/// 每行解析一个字符串，不按空格拆分带引号的命令参数。
fn parse_lines(input: &str) -> Option<Value> {
    if input.contains('\0') {
        return None;
    }
    Some(json!(
        input
            .lines()
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
    ))
}

/// Parses KEY=VALUE rows at the first equals sign and rejects empty names or values.
/// 在第一个等号解析 KEY=VALUE 行，并拒绝空名称或空值。
fn parse_pairs(input: &str) -> Option<Value> {
    // Current operation rows.
    // 当前操作的行集合。
    let mut rows = Vec::new();
    // Names.
    // 已经保留的名称集合。
    let mut names = std::collections::HashSet::new();
    for line in input.lines().filter(|line| !line.trim().is_empty()) {
        // Key; Value.
        // 当前结构持有的键；当前协议值。
        let (key, value) = line.split_once('=')?;
        if key.trim().is_empty()
            || value.trim().is_empty()
            || line.contains('\0')
            || !names.insert(key.trim())
        {
            return None;
        }
        rows.push(json!({"key":key.trim(),"value":value}));
    }
    Some(json!(rows))
}

/// Uses a declared parser for list and pair fields in a transient multi-field form.
/// 在临时多字段表单内为列表或键值字段使用明确解析器。
fn parsed_field(
    title: String,
    pointer: &'static str,
    secret: bool,
    parse: fn(&str) -> Option<Value>,
) -> super::form::FormField {
    super::form::FormField {
        title,
        pointer: pointer.into(),
        secret,
        optional: true,
        parse,
        validate: |_, _| true,
    }
}
/// Displays scalar fields of a sanitized DTO in a read-only expandable item.
/// 在只读可展开条目中显示已清理机密的 DTO 标量字段。
fn details(value: &Value) -> Vec<String> {
    match value {
        Value::Object(fields) => fields
            .iter()
            .map(|(key, value)| format!("{key}: {value}"))
            .collect(),
        Value::Array(items) => items.iter().map(Value::to_string).collect(),
        Value::Null => Vec::new(),
        value => vec![value.to_string()],
    }
}
/// Adds a reload button to a DTO-backed page, keeping errors recoverable without reconnecting.
/// 为 DTO 页面追加重新加载按钮，使错误恢复不必依赖重连。
pub fn rows(app: &App, page: PageId) -> Vec<Row> {
    // Each module reads its own service DTOs and constructs only declarative rows.
    // 各模块读取自身的服务 DTO，仅构建声明式行。
    let mut rows = match page {
        PageId::Appearance => settings::appearance(app),
        PageId::Context => settings::context(app),
        PageId::GitCommit => settings::git_commit(app),
        PageId::Providers => settings::providers(app),
        PageId::Credentials => settings::credentials(app),
        PageId::Models => settings::models(app),
        PageId::Capabilities => settings::capabilities(app),
        PageId::Mcp => settings::mcp(app),
        PageId::Archived => settings::archived(app),
        PageId::Diagnostics => settings::diagnostics(app),
        PageId::About => settings::about(app),
        PageId::MemoryOverview => memory::overview(app),
        PageId::MemoryContent => memory::content(app),
        PageId::MemoryProfile => memory::profile(app),
        PageId::MemoryProjects => memory::projects(app),
        PageId::MemoryMaintenance => memory::maintenance(app),
        PageId::MemoryRules => memory::rules(app),
        PageId::MemoryInference => memory::inference(app),
        PageId::MemoryResources => memory::resources(app),
        PageId::MemoryAdvanced => memory::advanced(app),
        PageId::KnowledgeBases => knowledge::libraries(app),
        PageId::KnowledgeLibrary => knowledge::library(app),
        PageId::KnowledgeSearch => knowledge::search(app),
        PageId::KnowledgeDocuments => knowledge::documents(app),
        PageId::KnowledgeDocument => knowledge::document(app),
        PageId::KnowledgeSettings => knowledge::settings(app),
        PageId::KnowledgeSources => knowledge::sources(app),
        PageId::KnowledgeBindings => knowledge::bindings(app),
        PageId::KnowledgeMaintenance => knowledge::maintenance(app),
        PageId::AgentList => catalog::agents(app),
        PageId::SkillList => catalog::skills(app),
        PageId::LuaSkills => plugins::lua_skills(app),
        PageId::LuaSettings => plugins::lua_settings(app),
        PageId::IndexProjects => plugins::index(app),
        PageId::EngineSettings => plugins::engine(app),
        PageId::VulcanPlugins => plugins::plugins(app),
        PageId::PluginMount => plugin_settings::rows(app),
    };
    if rows.is_empty() {
        rows.push(note(tr!("tui.page.loading"), Tone::Muted));
    }
    // Read failures remain visible after toasts expire and disappear only on a matching retry.
    // 读取失败在提示消失后仍保持可见，仅在匹配重试后消失。
    if !app.slot_errors.is_empty() {
        for loader in page.loaders(app) {
            if let api::CallKind::Load(slot) = loader.kind
                && let Some(error) = app.slot_errors.get(&slot)
            {
                rows.push(note(error.clone(), Tone::Bad));
            }
        }
    }
    rows.push(spacer());
    rows.push(button(
        tr!("tui.page.reload"),
        UiAction::ReloadPage,
        Tone::Muted,
    ));
    rows
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
