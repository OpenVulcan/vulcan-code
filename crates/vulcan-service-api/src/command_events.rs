use super::*;

/// Returns true when a boolean is false for serde omission helpers.
/// 当布尔值为 false 时返回 true，用于 serde 省略辅助。
fn is_false(value: &bool) -> bool {
    !*value
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

/// Represents one event emitted from the local service bridge.
/// 表示本地服务桥接层发出的单个事件。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
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
    /// Session addon namespace change event.
    /// 会话附属状态命名空间变更事件。
    SessionAddonChanged(Box<SessionAddonChangedEvent>),
    /// Plugin UI data invalidation event.
    /// 插件界面数据失效事件。
    PluginUiInvalidated(Box<PluginUiInvalidatedEvent>),
    /// Git working-tree change event for repository review refreshes.
    /// 用于仓库审查刷新的 Git 工作树变更事件。
    WorkspaceGitChanged(Box<WorkspaceGitChangedEvent>),
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
