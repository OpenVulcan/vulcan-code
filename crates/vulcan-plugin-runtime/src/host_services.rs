//! Host-owned services exposed to authorized Vulcan Plugins through the fixed bridge.
//! 通过固定桥接层向已授权 Vulcan 插件暴露的宿主持有服务。

use serde_json::Value;
use std::fmt;
use std::sync::Arc;

/// One exact storage operation authorized by a plugin manifest capability.
/// 由插件清单能力授权的一项精确存储操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulcanPluginStorageOperation {
    /// Apply an ordered SQLite schema migration.
    /// 应用一组有序 SQLite Schema 迁移。
    SqliteMigrate,
    /// Execute one parameterized SQLite statement.
    /// 执行一条参数化 SQLite 语句。
    SqliteExecute,
    /// Execute an ordered SQLite statement batch in one transaction.
    /// 在单个事务中执行一组有序 SQLite 语句。
    SqliteExecuteBatch,
    /// Query SQLite rows as JSON values.
    /// 以 JSON 值形式查询 SQLite 行。
    SqliteQuery,
    /// Flush pending SQLite writes and checkpoint WAL state.
    /// 刷新待处理 SQLite 写入并检查点化 WAL 状态。
    SqliteFlush,
    /// Create one typed LanceDB table.
    /// 创建一张类型化 LanceDB 表。
    LanceDbCreateTable,
    /// Upsert rows into one LanceDB table.
    /// 向一张 LanceDB 表写入或更新行。
    LanceDbUpsert,
    /// Search one LanceDB table.
    /// 检索一张 LanceDB 表。
    LanceDbSearch,
    /// Delete rows from one LanceDB table.
    /// 从一张 LanceDB 表删除行。
    LanceDbDelete,
    /// Drop one LanceDB table.
    /// 删除一张 LanceDB 表。
    LanceDbDropTable,
}

impl VulcanPluginStorageOperation {
    /// Reports whether this operation uses the SQLite backend.
    /// 报告当前操作是否使用 SQLite 后端。
    ///
    /// # Returns
    /// 返回值
    /// Returns true only for SQLite operations.
    /// 仅对 SQLite 操作返回 true。
    pub fn is_sqlite(self) -> bool {
        matches!(
            self,
            Self::SqliteMigrate
                | Self::SqliteExecute
                | Self::SqliteExecuteBatch
                | Self::SqliteQuery
                | Self::SqliteFlush
        )
    }
}

/// One authenticated plugin storage request with host-controlled identity.
/// 一条携带宿主控制身份的已认证插件存储请求。
#[derive(Debug, Clone)]
pub struct VulcanPluginStorageRequest {
    /// Exact plugin identity injected by the fixed bridge lease.
    /// 由固定桥接租约注入的精确插件身份。
    pub plugin_id: String,
    /// Exact authorized storage operation.
    /// 精确的已授权存储操作。
    pub operation: VulcanPluginStorageOperation,
    /// Plugin-owned JSON payload without host paths or binding identifiers.
    /// 不含宿主路径或绑定标识的插件自有 JSON 载荷。
    pub payload: Value,
}

/// Host storage boundary implemented outside the plugin runtime crate.
/// 在插件运行时 crate 外部实现的宿主存储边界。
pub trait VulcanPluginStorageService: Send + Sync {
    /// Executes one authenticated storage operation for an isolated plugin data space.
    /// 为隔离的插件数据空间执行一项已认证存储操作。
    ///
    /// # Parameters
    /// 参数
    /// `request` contains the host-injected plugin identity, operation, and validated payload.
    /// `request` 包含宿主注入的插件身份、操作与已校验载荷。
    ///
    /// # Returns
    /// 返回值
    /// Returns the JSON-safe operation result or an explicit host diagnostic.
    /// 返回 JSON 安全的操作结果或明确的宿主诊断。
    fn execute(&self, request: VulcanPluginStorageRequest) -> Result<Value, String>;
}

/// One plugin-scoped UI invalidation event published after durable data changes.
/// 在数据持久变更后发布的一条插件作用域界面失效事件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VulcanPluginUiInvalidation {
    /// Exact plugin identity injected by the fixed bridge lease.
    /// 由固定桥接租约注入的精确插件身份。
    pub plugin_id: String,
    /// Plugin-local invalidation topic.
    /// 插件内失效主题。
    pub topic: String,
    /// Monotonic plugin data revision.
    /// 单调递增的插件数据修订号。
    pub revision: u64,
}

/// Host event boundary used to notify mounted plugin UI without carrying business data.
/// 用于通知已挂载插件界面且不携带业务数据的宿主事件边界。
pub trait VulcanPluginUiEventPublisher: Send + Sync {
    /// Publishes one validated plugin-scoped invalidation event.
    /// 发布一条已校验的插件作用域失效事件。
    ///
    /// # Parameters
    /// 参数
    /// `event` contains only plugin identity, topic, and revision.
    /// `event` 仅包含插件身份、主题与修订号。
    ///
    /// # Returns
    /// 返回值
    /// Returns unit after the host accepted the event or an explicit diagnostic.
    /// 宿主接受事件后返回空值，否则返回明确诊断。
    fn publish(&self, event: VulcanPluginUiInvalidation) -> Result<(), String>;
}

/// Optional host services installed after the surrounding application runtime is ready.
/// 外围应用运行时就绪后安装的可选宿主服务集合。
#[derive(Clone, Default)]
pub struct VulcanPluginHostServices {
    /// Optional isolated plugin storage service.
    /// 可选的隔离插件存储服务。
    pub storage: Option<Arc<dyn VulcanPluginStorageService>>,
    /// Optional plugin UI invalidation publisher.
    /// 可选的插件界面失效发布器。
    pub ui_events: Option<Arc<dyn VulcanPluginUiEventPublisher>>,
}

impl fmt::Debug for VulcanPluginHostServices {
    /// Formats service availability without exposing implementation internals.
    /// 在不暴露实现内部的前提下格式化服务可用性。
    ///
    /// # Parameters
    /// 参数
    /// `formatter` receives the debug representation.
    /// `formatter` 接收调试表示。
    ///
    /// # Returns
    /// 返回值
    /// Returns the formatter result.
    /// 返回格式化器结果。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VulcanPluginHostServices")
            .field("storage_available", &self.storage.is_some())
            .field("ui_events_available", &self.ui_events.is_some())
            .finish()
    }
}
