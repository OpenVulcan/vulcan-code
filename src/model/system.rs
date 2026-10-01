// System areas: settings, memory, knowledge, skills, agents and the code semantic index.
// 系统区域：设置、记忆、知识库、技能、代理与代码语义索引。
//
// These are native (non-Lua) system features. Like the conversation model, the shapes keep only
// what the TUI renders; the service backend maps its DTOs onto them. Scalar settings travel as
// `(key, SettingValue)` pairs so simple pages need no dedicated command per field.
// 这些是原生（非 Lua）系统功能。与会话模型一样，只保留 TUI 渲染所需的字段；服务后端将其 DTO
// 映射到这些类型。标量设置以 `(键, SettingValue)` 对传递，简单页面无需为每个字段单独定义命令。

use std::collections::BTreeMap;

use super::Severity;

#[derive(Debug, Clone, PartialEq)]
pub enum SettingValue {
    Bool(bool),
    Choice(String),
    Number(i64),
    Text(String),
}

impl SettingValue {
    pub fn as_bool(&self) -> bool {
        matches!(self, SettingValue::Bool(true))
    }

    pub fn as_number(&self) -> i64 {
        match self {
            SettingValue::Number(n) => *n,
            _ => 0,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            SettingValue::Choice(s) | SettingValue::Text(s) => s,
            _ => "",
        }
    }
}

/// Long-running system service state (memory service, index workers).
/// 长生命周期系统服务状态（记忆服务、索引工作线程）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
    Failed,
}

impl ServiceStatus {
    pub fn is_transition(self) -> bool {
        matches!(self, ServiceStatus::Starting | ServiceStatus::Stopping)
    }
}

/// A background job with progress, addressed by a stable id so live state can be re-queried.
/// 带进度的后台任务，以稳定 ID 寻址以便重新查询实时状态。
#[derive(Debug, Clone)]
pub struct Job {
    pub id: String,
    pub label: String,
    /// 0..=100; `None` while the job has not reported progress.
    /// 0..=100；任务尚未报告进度时为 `None`。
    pub progress: Option<u8>,
    pub done: bool,
    pub failed: bool,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct Credential {
    pub id: String,
    pub label: String,
    /// Always masked by the backend; the TUI never receives or shows the secret itself.
    /// 始终由后端打码；TUI 从不接收或显示密钥本身。
    pub masked: String,
    pub is_default: bool,
    pub last_test: Option<(Severity, String)>,
}

#[derive(Debug, Clone)]
pub struct Provider {
    pub id: String,
    pub name: String,
    /// `official` | `cloudMaaS` | `relay` | `unclassified`
    pub category: String,
    pub protocols: Vec<String>,
    pub endpoints: u32,
    pub credentials: Vec<Credential>,
}

#[derive(Debug, Clone)]
pub struct ModelEntry {
    pub id: String,
    pub label: String,
    pub provider: String,
    pub context_window: u64,
    pub conversation_enabled: bool,
    pub internal_enabled: bool,
}

/// One AI capability (`aiWebSearch`, `aiImageGeneration`, ...), routed to a model or off.
/// 一项 AI 能力（`aiWebSearch`、`aiImageGeneration` 等），路由到某个模型或关闭。
#[derive(Debug, Clone)]
pub struct Capability {
    pub key: String,
    pub enabled: bool,
    pub model: Option<String>,
    pub candidates: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct McpServer {
    pub id: String,
    pub name: String,
    pub transport: String,
    pub status: ServiceStatus,
    pub tools: u32,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct ArchivedConversation {
    pub id: String,
    pub title: String,
    pub project: String,
    pub archived_label: String,
}

#[derive(Debug, Clone)]
pub struct StartupCheck {
    pub label: String,
    pub severity: Severity,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct MemoryItem {
    pub id: String,
    pub content: String,
    pub project: String,
    pub source: String,
    pub score: Option<f32>,
}

/// One durable memory project; the default one receives memory from unmapped conversations.
/// 一个持久记忆项目；默认项目接收未映射会话的记忆。
#[derive(Debug, Clone)]
pub struct MemoryProject {
    pub id: String,
    pub name: String,
    pub items: u32,
    pub default: bool,
    /// System default project; it can be neither deleted nor migrated.
    /// 系统默认项目；不可删除也不可迁移。
    pub protected: bool,
}

/// One native VMM resource file (prompt, PII or Noise rules).
/// 一个原生 VMM 资源文件（提示词、PII 或 Noise 规则）。
#[derive(Debug, Clone)]
pub struct MemoryResource {
    pub id: String,
    pub file: String,
    /// `prompt` | `pii` | `noise`
    pub category: String,
    /// Prompt purpose id (`precheckL1`, ...); empty for rule files.
    /// 提示词用途 ID（`precheckL1` 等）；规则文件为空。
    pub purpose: String,
    pub overridden: bool,
}

#[derive(Debug, Clone)]
pub struct MemoryState {
    pub status: ServiceStatus,
    pub deployment: String,
    pub health: Severity,
    pub pending_writebacks: u32,
    pub completed_turns: u32,
    pub last_profile_update: String,
    pub items: Vec<MemoryItem>,
    /// Results of the last content search; `None` shows the full list.
    /// 最近一次内容搜索的结果；`None` 时显示完整列表。
    pub search: Option<(String, Vec<MemoryItem>)>,
    pub searching: bool,
    pub profile: Vec<String>,
    pub projects: Vec<MemoryProject>,
    pub resources: Vec<MemoryResource>,
    /// Technical diagnostics as `(label, value)` pairs.
    /// 以 `(标签, 值)` 对表示的技术诊断。
    pub diagnostics: Vec<(String, String)>,
    pub jobs: Vec<Job>,
}

/// One imported directory of a knowledge library.
/// 知识库中的一个导入目录。
#[derive(Debug, Clone)]
pub struct KnowledgeSource {
    pub id: String,
    pub path: String,
    pub documents: u32,
    pub chunks: u32,
    pub last_scanned: String,
}

/// Search-index integrity of a knowledge library.
/// 知识库搜索索引的完整性。
#[derive(Debug, Clone)]
pub struct IndexHealth {
    pub authoritative: u32,
    pub indexed: u32,
    pub missing: u32,
    pub orphaned: u32,
    /// `(indexed, current)` revisions.
    /// `(已索引, 当前)` 修订号。
    pub content_revision: (u32, u32),
    pub dictionary_revision: (u32, u32),
    pub tokenizer: String,
}

impl IndexHealth {
    pub fn searchable(&self) -> bool {
        self.missing == 0
            && self.orphaned == 0
            && self.content_revision.0 == self.content_revision.1
            && self.dictionary_revision.0 == self.dictionary_revision.1
    }
}

#[derive(Debug, Clone)]
pub struct KnowledgeBase {
    pub id: String,
    pub name: String,
    pub documents: u32,
    pub chunks: u32,
    pub enabled: bool,
    /// Enabled by default in sessions of authorized projects.
    /// 在已授权项目的会话中默认启用。
    pub session_default: bool,
    /// `raw` | `display` | `hybrid`
    pub mode: String,
    /// `empty` | `ready` | `dirty` | `rebuilding` | `failed`
    pub status: String,
    pub sources: Vec<KnowledgeSource>,
    pub health: IndexHealth,
    pub job: Option<Job>,
}

#[derive(Debug, Clone)]
pub struct SearchHit {
    pub title: String,
    pub snippet: String,
    pub score: f32,
}

/// One standard Agent Skill.
/// 一个标准 Agent Skill。
#[derive(Debug, Clone)]
pub struct Skill {
    pub id: String,
    pub name: String,
    /// `system` | `user` | `plugin`
    pub source: String,
    pub description: String,
    pub enabled: bool,
    /// A user override shadows the system version.
    /// 用户覆盖遮蔽了系统版本。
    pub overridden: bool,
    pub plugin: Option<String>,
    /// Load problem reported by the catalog.
    /// 目录报告的加载问题。
    pub issue: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AgentProfile {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Fixed model id; empty follows the session model.
    /// 固定模型 ID；为空表示跟随会话模型。
    pub model: String,
    pub enabled: bool,
    /// `system` | `user` | `plugin`
    pub scope: String,
    pub plugin: Option<String>,
    pub overridden: bool,
    /// Exposed to the model as a delegation target.
    /// 作为委派目标向模型公开。
    pub expose: bool,
    pub allowed_tools: u32,
    pub denied_tools: u32,
    pub skills: Vec<String>,
    pub file: String,
}

/// Vector-table fragments at which the service recommends a physical compaction.
/// 服务建议执行物理压缩的向量表碎片数阈值。
pub const COMPACTION_FRAGMENTS: u32 = 32;

#[derive(Debug, Clone)]
pub struct IndexedProject {
    pub project_id: String,
    pub name: String,
    /// `ready` | `indexing` | `rebuildRequired` | `modelsRequired` | `disabled`
    pub status: String,
    pub files: u32,
    pub chunks: u32,
    pub vectors: u32,
    pub symbols: u32,
    /// Automatic updates follow file changes; otherwise updates are manual.
    /// 自动更新跟随文件变化；否则为手动更新。
    pub watching: bool,
    pub pending: u32,
    pub checkpoint: String,
    pub last_update: String,
    /// Physical vector-table fragments; compaction is recommended above a threshold.
    /// 向量表物理碎片数；超过阈值时建议压缩。
    pub fragments: u32,
    pub job: Option<Job>,
}

/// How a LuaSkills tool is exposed to the model.
/// LuaSkills 工具向模型暴露的方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolMode {
    Explicit,
    Implicit,
    Disabled,
}

impl ToolMode {
    pub const ALL: [ToolMode; 3] = [ToolMode::Explicit, ToolMode::Implicit, ToolMode::Disabled];

    pub fn id(self) -> &'static str {
        match self {
            ToolMode::Explicit => "explicit",
            ToolMode::Implicit => "implicit",
            ToolMode::Disabled => "disabled",
        }
    }

    pub fn from_id(id: &str) -> Option<ToolMode> {
        ToolMode::ALL.into_iter().find(|m| m.id() == id)
    }

    /// The next mode in display order, wrapping around.
    /// 按显示顺序的下一个模式，循环。
    pub fn next(self) -> ToolMode {
        let at = ToolMode::ALL.iter().position(|m| *m == self).unwrap_or(0);
        ToolMode::ALL[(at + 1) % ToolMode::ALL.len()]
    }
}

/// One installed LuaSkills package.
/// 一个已安装的 LuaSkills 技能包。
#[derive(Debug, Clone)]
pub struct LuaSkill {
    pub id: String,
    pub name: String,
    pub version: String,
    /// Install source, e.g. a GitHub repository.
    /// 安装来源，例如 GitHub 仓库。
    pub source: String,
    /// `system` | `user`
    pub layer: String,
    pub description: String,
    pub enabled: bool,
    pub tools: Vec<(String, ToolMode)>,
    /// Newer version offered by the source.
    /// 来源提供的新版本。
    pub update: Option<String>,
    pub job: Option<Job>,
}

/// One Vulcan plugin. Its UI mounts are listed but never rendered by the TUI.
/// 一个 Vulcan 插件。其界面挂载点只列出，TUI 从不渲染。
#[derive(Debug, Clone)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    /// `system` | `user` | `builtin`
    pub kind: String,
    pub description: String,
    pub enabled: bool,
    pub status: ServiceStatus,
    pub tools: u32,
    pub hooks: u32,
    pub sessions: u32,
    pub skills: u32,
    pub agents: u32,
    pub permissions: Vec<String>,
    pub mounts: Vec<String>,
    pub diagnostics: Vec<String>,
}

/// Everything the system areas render; replaced as a whole on each `BackendEvent::System`.
/// 系统区域渲染所需的全部数据；每次 `BackendEvent::System` 整体替换。
#[derive(Debug, Clone)]
pub struct SystemSnapshot {
    pub settings: BTreeMap<String, SettingValue>,
    pub providers: Vec<Provider>,
    pub models: Vec<ModelEntry>,
    pub capabilities: Vec<Capability>,
    pub mcp_servers: Vec<McpServer>,
    pub archived: Vec<ArchivedConversation>,
    pub startup: Vec<StartupCheck>,
    pub logs: Vec<String>,
    pub memory: MemoryState,
    pub knowledge: Vec<KnowledgeBase>,
    pub knowledge_search: Option<(String, Vec<SearchHit>)>,
    pub skills: Vec<Skill>,
    pub agents: Vec<AgentProfile>,
    pub index: Vec<IndexedProject>,
    pub lua_skills: Vec<LuaSkill>,
    pub plugins: Vec<Plugin>,
    /// Managed Python / Node runtimes of LuaSkills.
    /// LuaSkills 托管的 Python / Node 运行时。
    pub managed_runtimes: Vec<StartupCheck>,
    /// Embedding models as `(id, label, supported dimensions)`.
    /// 嵌入模型，形如 `(ID, 名称, 支持的维度)`。
    pub embedding_models: Vec<(String, String, Vec<u32>)>,
    pub rerank_models: Vec<(String, String)>,
    /// Last connection test per route key.
    /// 每个路由键最近一次连接测试结果。
    pub route_tests: BTreeMap<String, (Severity, String)>,
    pub version: String,
}

impl SystemSnapshot {
    pub fn setting(&self, key: &str) -> Option<&SettingValue> {
        self.settings.get(key)
    }
}

/// Which kind of entity an enable toggle targets.
/// 启用开关作用的实体类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityKind {
    ConversationModel,
    InternalModel,
    Capability,
    Mcp,
    Skill,
    Agent,
    Knowledge,
    IndexWatch,
    LuaSkill,
    Plugin,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CredentialOp {
    Add { label: String, secret: String },
    Test(String),
    MakeDefault(String),
    Delete(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceOp {
    Start,
    Stop,
    Restart,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceOp {
    Add(String),
    Sync(String),
    Remove(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryProjectOp {
    Create(String),
    SetDefault(String),
    /// Moves all memory of `from` into the existing project named `to`.
    /// 将 `from` 的全部记忆迁入名为 `to` 的现有项目。
    Migrate {
        from: String,
        to: String,
    },
    Delete(String),
}

/// Lifecycle operations shared by skills, agents, plugins and LuaSkills packages.
/// 技能、代理、插件与 LuaSkills 包共用的生命周期操作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogOp {
    Install(String),
    Update(String),
    /// Removes a user override and restores the system version.
    /// 移除用户覆盖并恢复系统版本。
    Restore(String),
    Delete(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaintenanceTask {
    Organize,
    RefreshProfile,
    Cleanup,
}

/// System-area intents; each maps onto one service RPC.
/// 系统区域意图；每个对应一个服务 RPC。
#[derive(Debug, Clone, PartialEq)]
pub enum SystemCommand {
    SetSetting {
        key: String,
        value: SettingValue,
    },
    /// Saves a draft page atomically.
    /// 原子保存草稿页面。
    SaveSettings(Vec<(String, SettingValue)>),
    /// The user picked a language in the TUI; the service stores it as `uiLocale`.
    /// 用户在 TUI 中选择了语言；服务将其保存为 `uiLocale`。
    SetUiLocale(String),
    SetEnabled {
        kind: EntityKind,
        id: String,
        enabled: bool,
    },
    SetRoute {
        kind: EntityKind,
        id: String,
        model: Option<String>,
    },
    /// One field of one entity, e.g. a library's retrieval mode or a tool's mode.
    /// 某个实体的某个字段，例如知识库的检索模式或工具模式。
    SetField {
        kind: EntityKind,
        id: String,
        field: String,
        value: SettingValue,
    },
    /// Runs a real connection test of one model route (`memory.inference.precheckL1`, ...).
    /// 对某个模型路由执行真实连接测试。
    TestRoute(String),
    Credential {
        provider: String,
        op: CredentialOp,
    },
    RestoreArchived(String),
    DeleteArchived(String),
    RestartMcp(String),
    MemoryService(ServiceOp),
    MemorySearch(String),
    MemoryDelete(String),
    MemoryCorrectProfile(String),
    MemoryMaintenance(MaintenanceTask),
    MemoryProject(MemoryProjectOp),
    MemoryResourceRestore(String),
    KnowledgeSearch(String),
    KnowledgeReindex(String),
    KnowledgeRepair(String),
    KnowledgeCreate(String),
    KnowledgeDelete(String),
    KnowledgeSource {
        library: String,
        op: SourceOp,
    },
    IndexRebuild(String),
    IndexCompact(String),
    Skill(CatalogOp),
    Agent(CatalogOp),
    LuaSkill(CatalogOp),
    Plugin(CatalogOp),
    SendTestNotification,
    CleanupLogs,
}
