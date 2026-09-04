use super::*;

/// Returns true when a boolean is false for serde omission helpers.
/// 当布尔值为 false 时返回 true，用于 serde 省略辅助。
fn is_false(value: &bool) -> bool {
    !*value
}

/// Returns true when an unsigned revision is zero for serde omission helpers.
/// 当无符号修订为零时返回 true，用于 serde 省略辅助。
fn is_zero_u64(value: &u64) -> bool {
    *value == 0
}

/// Summarizes one long-running command session.
/// 汇总单个长生命周期命令会话。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommandSessionSummary {
    /// Stable command session identifier.
    /// 稳定命令会话标识。
    pub id: String,
    /// Optional user-facing terminal session name.
    /// 可选用户可见终端会话名称。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Command display name.
    /// 命令显示名称。
    pub command: String,
    /// Live command status.
    /// 实时命令状态。
    pub status: String,
    /// Recent command output preview.
    /// 最近命令输出预览。
    pub preview: String,
    /// Optional owner session id when the runtime reports one.
    /// 运行时报告时的可选归属会话标识。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_session_id: Option<String>,
    /// Effective working directory used by the command session.
    /// 命令会话使用的实际工作目录。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Stable shell label selected by the host.
    /// 宿主选择的稳定 shell 标签。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell_label: Option<String>,
    /// Concrete backend label reported by the host runtime.
    /// 宿主运行时报告的具体后端标签。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    /// Latest screen frame sequence reported by frame-capable terminal sessions.
    /// 具备屏幕帧能力的终端会话报告的最新屏幕帧序号。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen_frame_seq: Option<u64>,
    /// Latest terminal row count reported by frame-capable terminal sessions.
    /// 具备屏幕帧能力的终端会话报告的最新终端行数。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen_rows: Option<u16>,
    /// Latest terminal column count reported by frame-capable terminal sessions.
    /// 具备屏幕帧能力的终端会话报告的最新终端列数。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen_cols: Option<u16>,
    /// Latest renderable screen text reported by frame-capable terminal sessions.
    /// 具备屏幕帧能力的终端会话报告的最新可渲染屏幕内容。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen_text: Option<String>,
    /// Highest output sequence evicted from bounded retained history.
    /// 已从有界保留历史中淘汰的最高输出序号。
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub dropped_before_seq: u64,
    /// Optional process exit code captured by the host.
    /// 宿主捕获到的可选进程退出码。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Optional failure reason synthesized by the host runtime.
    /// 宿主运行时综合生成的可选失败原因。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    /// Whether this summary represents a historical session no longer present in live runtime state.
    /// 该摘要是否代表实时运行时中已不存在的历史会话。
    #[serde(default, skip_serializing_if = "is_false")]
    pub lost: bool,
}

/// Summarizes one diagnostic log entry.
/// 汇总单条诊断日志。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LogEntrySummary {
    /// Stable log entry identifier.
    /// 稳定日志条目标识。
    pub id: String,
    /// Log entry level.
    /// 日志级别。
    pub level: String,
    /// Log entry source.
    /// 日志来源。
    pub source: String,
    /// Log entry message.
    /// 日志消息。
    pub message: String,
}

/// Describes one session addon-state namespace change emitted to connected UI clients.
/// 描述一次发送给已连接 UI 客户端的会话附属状态命名空间变更。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionAddonChangedEvent {
    /// Session identifier whose addon state changed.
    /// 附属状态发生变化的会话标识。
    pub session_id: String,
    /// Changed addon namespace keys.
    /// 已变化的附属状态命名空间键。
    pub namespaces: Vec<String>,
    /// Latest addon document revision after the change.
    /// 变化后的最新附属状态文档版本。
    pub revision: u64,
    /// Unix-millis text representing the latest addon update time.
    /// 表示最近附属状态更新时间的 Unix 毫秒文本。
    pub updated_at: String,
    /// Optional local instance id that originated the change.
    /// 发起该变化的可选本地实例标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_instance_id: Option<String>,
}

/// Describes one plugin-scoped UI data invalidation without carrying business data.
/// 描述一次不携带业务数据的插件作用域界面数据失效。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginUiInvalidatedEvent {
    /// Exact plugin identity owning the invalidation topic.
    /// 拥有失效主题的精确插件身份。
    pub plugin_id: String,
    /// Plugin-local invalidation topic.
    /// 插件内失效主题。
    pub topic: String,
    /// Monotonic plugin data revision.
    /// 单调递增的插件数据修订号。
    pub revision: u64,
}

/// Describes Git repositories whose working-tree state changed on disk.
/// 描述磁盘上的工作树状态发生变化的 Git 仓库集合。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitChangedEvent {
    /// Optional persisted project identity owning the changed repositories.
    /// 发生变化的仓库所属的可选持久化项目标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// Canonical workspace path used to match the active WebUI project.
    /// 用于匹配当前 WebUI 项目的规范化工作区路径。
    pub workspace_path: String,
    /// Repository keys whose watched paths contain at least one changed file.
    /// 被监听路径包含至少一个变更文件的仓库键集合。
    pub repository_keys: Vec<String>,
}

/// Carries one ordered terminal output fragment to an attached display.
/// 向一个已挂接显示层承载一个有序终端输出片段。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalDisplayChunk {
    /// Monotonic output sequence inside the terminal runtime.
    /// 终端运行时内部的单调递增输出序号。
    pub seq: u64,
    /// Stable output stream label such as stdout or stderr.
    /// 稳定的输出流标签，例如 stdout 或 stderr。
    pub stream: String,
    /// Decoded output text delivered to the terminal renderer.
    /// 交付给终端渲染器的已解码输出文本。
    pub text: String,
}

/// Returns the initial state for one newly attached visible command-session display.
/// 返回一个新挂接的可见命令会话显示层的初始状态。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommandSessionDisplayAttachment {
    /// Opaque identity used to route events and detach this display.
    /// 用于路由事件及解除该显示层挂接的不透明标识。
    pub subscription_id: String,
    /// Current live terminal summary at attach time.
    /// 挂接时的当前实时终端摘要。
    pub terminal: CommandSessionSummary,
    /// Retained output required to initialize the renderer once.
    /// 首次初始化渲染器所需的保留输出。
    pub chunks: Vec<TerminalDisplayChunk>,
}

/// Pushes changed output and state only to one attached visible command-session display.
/// 仅向一个已挂接的可见命令会话显示层推送变化后的输出与状态。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommandSessionDisplayChangedEvent {
    /// Browser/WebView lifetime that owns this display event.
    /// 持有该显示事件的浏览器或 WebView 生命周期。
    pub display_client_id: String,
    /// Opaque display subscription receiving this event.
    /// 接收该事件的不透明显示订阅。
    pub subscription_id: String,
    /// Latest live terminal summary.
    /// 最新实时终端摘要。
    pub terminal: CommandSessionSummary,
    /// Ordered output fragments not delivered by the previous event.
    /// 上一事件尚未交付的有序输出片段。
    pub chunks: Vec<TerminalDisplayChunk>,
}

/// Describes one shell launcher available for project terminal creation.
/// 描述一个可用于创建项目终端的 Shell 启动器。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTerminalShellOption {
    /// Stable shell family id accepted by create requests.
    /// 创建请求接受的稳定 Shell 族标识。
    pub id: String,
    /// User-facing shell label.
    /// 用户可见 Shell 标签。
    pub label: String,
    /// Indicates the shell selected by current auto/default policy.
    /// 指示当前自动或默认策略选择的 Shell。
    pub default: bool,
}

/// Summarizes one project-owned terminal independently from conversation sessions.
/// 独立于对话会话汇总一个项目级终端。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTerminalSummary {
    /// Stable terminal identity.
    /// 稳定终端标识。
    pub id: String,
    /// Stable owning project identity.
    /// 稳定归属项目标识。
    pub project_id: String,
    /// User-facing terminal name.
    /// 用户可见终端名称。
    pub name: String,
    /// Selected shell family id.
    /// 已选择 Shell 族标识。
    pub shell_id: String,
    /// Exact working directory shown to the user.
    /// 展示给用户的精确工作目录。
    pub cwd: String,
    /// Stable lowercase lifecycle status.
    /// 稳定小写生命周期状态。
    pub status: String,
    /// Optional terminal child exit code.
    /// 可选终端子进程退出码。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Optional terminal-host or child failure reason.
    /// 可选终端宿主或子进程故障原因。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    /// Highest output sequence emitted by the runtime.
    /// 运行时发出的最高输出序号。
    pub last_seq: u64,
    /// Highest output sequence evicted from the bounded display checkpoint.
    /// 已从有界显示检查点淘汰的最高输出序号。
    pub dropped_before_seq: u64,
    /// Monotonic revision for user-visible terminal metadata.
    /// 用户可见终端元数据的单调修订号。
    pub state_revision: u64,
    /// Highest contiguous browser input sequence accepted by the terminal host pipe.
    /// 终端宿主管道已接受的最高连续浏览器输入序号。
    pub last_accepted_input_seq: u64,
    /// Current terminal row count.
    /// 当前终端行数。
    pub rows: u16,
    /// Current terminal column count.
    /// 当前终端列数。
    pub cols: u16,
}

/// Returns available shells and current terminals for one explicit active project.
/// 返回一个显式活动项目的可用 Shell 与当前终端。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTerminalCatalog {
    /// Stable project identity used to build this catalog.
    /// 用于构建本目录的稳定项目标识。
    pub project_id: String,
    /// Host-detected shell launchers in stable preference order.
    /// 按稳定偏好顺序排列的宿主检测 Shell 启动器。
    pub shells: Vec<ProjectTerminalShellOption>,
    /// Project-owned terminal tabs.
    /// 项目级终端标签集合。
    pub terminals: Vec<ProjectTerminalSummary>,
    /// Monotonic revision for this project's running terminal count.
    /// 当前项目运行中终端数量的单调修订号。
    pub running_count_revision: u64,
}

/// Pushes one project's running terminal count without carrying terminal content.
/// 推送一个项目的运行中终端数量，且不承载终端内容。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTerminalRunningCountChangedEvent {
    /// Stable project identity owning the counted terminals.
    /// 被统计终端所属的稳定项目标识。
    pub project_id: String,
    /// Number of project terminals whose lifecycle status is running.
    /// 生命周期状态为运行中的项目终端数量。
    pub running_count: usize,
    /// Monotonic revision used to reject stale initial-query results.
    /// 用于拒绝过期首次查询结果的单调修订号。
    pub revision: u64,
}

/// Returns initial state for one newly attached visible project terminal display.
/// 返回一个新挂接的可见项目终端显示层的初始状态。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTerminalDisplayAttachment {
    /// Opaque identity used to route events and detach this display.
    /// 用于路由事件及解除显示层挂接的不透明标识。
    pub subscription_id: String,
    /// Current project terminal snapshot.
    /// 当前项目终端快照。
    pub terminal: ProjectTerminalSummary,
    /// Bounded retained output tail used to initialize xterm once.
    /// 用于一次性初始化 xterm 的有界保留输出尾部。
    pub chunks: Vec<TerminalDisplayChunk>,
}

/// Pushes changed output and state only to one attached visible project terminal.
/// 仅向一个已挂接的可见项目终端推送变化输出与状态。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTerminalDisplayChangedEvent {
    /// Browser/WebView lifetime that owns this display event.
    /// 持有该显示事件的浏览器或 WebView 生命周期。
    pub display_client_id: String,
    /// Opaque visible-display subscription receiving this event.
    /// 接收本事件的不透明可见显示订阅。
    pub subscription_id: String,
    /// Latest project terminal snapshot.
    /// 最新项目终端快照。
    pub terminal: ProjectTerminalSummary,
    /// Ordered output fragments not delivered by the previous event.
    /// 上一个事件尚未交付的有序输出片段。
    pub chunks: Vec<TerminalDisplayChunk>,
}

/// Represents one event emitted from the local service bridge.
/// 表示本地服务桥接层发出的单个事件。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "kind", content = "payload")]
pub enum ServiceEvent {
    /// Startup snapshot event.
    /// 启动快照事件。
    Startup(Box<StartupSnapshot>),
    /// Workbench snapshot event.
    /// 工作台快照事件。
    Workbench(Box<WorkbenchSnapshot>),
    /// Partial workbench update event.
    /// 局部工作台更新事件。
    WorkbenchDelta(Box<WorkbenchDeltaEnvelope>),
    /// Fully settled conversation run lifecycle event.
    /// 完全结束的会话运行生命周期事件。
    ConversationRunSettled(Box<ConversationRunSettledEvent>),
    /// Durable V1 conversation-event delta published from the canonical session writer.
    /// 由规范会话写入器发布的可靠 V1 会话事件增量。
    ConversationEventDeltaV1(Box<ConversationEventLiveDeltaV1>),
    /// Session addon namespace change event.
    /// 会话附属状态命名空间变更事件。
    SessionAddonChanged(Box<SessionAddonChangedEvent>),
    /// Plugin UI data invalidation event.
    /// 插件界面数据失效事件。
    PluginUiInvalidated(Box<PluginUiInvalidatedEvent>),
    /// Git working-tree change event for repository review refreshes.
    /// 用于仓库审查刷新的 Git 工作树变更事件。
    WorkspaceGitChanged(Box<WorkspaceGitChangedEvent>),
    /// Native code-semantic project lifecycle or checkpoint change event.
    /// 原生代码语义项目生命周期或检查点变更事件。
    VceProjectChanged(Box<VceProjectChangedEvent>),
    /// Visible command-session terminal output event.
    /// 可见命令会话终端输出事件。
    CommandSessionDisplayChanged(Box<CommandSessionDisplayChangedEvent>),
    /// Visible project-terminal output event.
    /// 可见项目终端输出事件。
    ProjectTerminalDisplayChanged(Box<ProjectTerminalDisplayChangedEvent>),
    /// Lightweight project-terminal running-count event without terminal content.
    /// 不包含终端内容的轻量项目终端运行数量事件。
    ProjectTerminalRunningCountChanged(Box<ProjectTerminalRunningCountChangedEvent>),
}

/// Receives one service event emitted by the Core Service boundary.
/// 接收 Core Service 边界发出的单个服务事件。
pub trait ServiceEventSink: Send + Sync {
    /// Delivers one service event to the concrete sink implementation.
    /// 将单个服务事件投递给具体 sink 实现。
    fn emit_service_event(&self, event: ServiceEvent);
}

/// Multiplexes service events to all registered client-facing sinks.
/// 将服务事件复用分发到所有已注册的客户端 sink。
#[derive(Clone)]
pub struct ServiceEventEmitter {
    /// Ordered event sinks used by the emitter.
    /// 事件发送器使用的有序 sink 列表。
    sinks: Arc<Vec<Arc<dyn ServiceEventSink>>>,
}

impl ServiceEventEmitter {
    /// Builds an event emitter from an ordered sink list.
    /// 使用有序 sink 列表构建事件发送器。
    pub fn new(sinks: Vec<Arc<dyn ServiceEventSink>>) -> Self {
        Self {
            sinks: Arc::new(sinks),
        }
    }

    /// Emits one event to every registered sink while minimizing event clones.
    /// 向所有已注册 sink 发送单个事件，并尽量减少事件克隆。
    pub fn emit(&self, event: ServiceEvent) {
        let Some((last, leading)) = self.sinks.split_last() else {
            return;
        };
        for sink in leading {
            sink.emit_service_event(event.clone());
        }
        last.emit_service_event(event);
    }
}
