//! Host-local database runtime: logical resources bound to physical files, opened in process.
//! 宿主本地数据库运行时：逻辑资源绑定到物理文件，在进程内打开。
//!
//! This module is the Phase Four replacement for the controller's process, endpoint and client
//! responsibilities. It owns, for the whole host process:
//!
//! 本模块是阶段四对控制器进程、端点与客户端职责的替代。它在整个宿主进程内持有：
//!
//! - the two in-process engines ([`SqliteRuntime`] and [`LanceDbRuntime`]);
//! - a **read-only** binding from each [`LogicalDatabase`] to one physical path;
//! - the startup health check (open, schema version, lock ownership, minimal read/write);
//! - the shutdown drain (stop accepting, drain, release engines and file locks).
//!
//! - 两个进程内引擎（[`SqliteRuntime`] 与 [`LanceDbRuntime`]）；
//! - 每个 [`LogicalDatabase`] 到单个物理路径的**只读**绑定；
//! - 启动健康检查（打开、Schema 版本、锁所有权、最小读写）；
//! - 退出排空（停止接收、排空、释放引擎与文件锁）。
//!
//! Why the binding is read-only / 为何绑定是只读:
//!
//! The plan is explicit that directory derivation must happen in **one** authoritative host entry
//! point and that engines must not re-guess directories. A binding is therefore built once from
//! caller-supplied paths and never derived from a logical name, so no code path can invent a
//! storage location from a string.
//!
//! 方案明确指出目录派生必须发生在宿主**唯一**权威入口，且引擎不得重新推测目录。因此绑定由调用方提供的
//! 路径一次性建立，绝不由逻辑名派生，使任何代码路径都无法从一个字符串凭空造出存储位置。
//!
//! What replaced the old lifecycle / 取代了旧生命周期的东西:
//!
//! | Old controller call | Now |
//! | --- | --- |
//! | `attach_space` | deleted — a space only existed to route across processes / 删除：空间仅为跨进程路由而存在 |
//! | `enable_sqlite` / `enable_lancedb` | [`LocalDatabaseRuntime::open`] opens the file directly / 直接打开文件 |
//! | `get_status` polling | [`LocalDatabaseRuntime::health_report`] reads the engines, no network / 读取引擎，无网络 |
//! | `shutdown` over RPC | [`LocalDatabaseRuntime::shutdown`] drops engines and releases locks locally / 本地释放引擎与锁 |

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};

pub use vulcan_lancedb_engine::engine::{LanceDbEngine, LanceDbEngineOptions};
use vulcan_lancedb_engine::error::LanceDbEngineError;
use vulcan_lancedb_engine::manager::DatabaseRuntimeConfig;
use vulcan_lancedb_engine::runtime::LanceDbRuntime;
pub use vulcan_lancedb_engine::types::{
    LanceDbColumnDef, LanceDbColumnType, LanceDbCreateTableInput, LanceDbCreateTableResult,
    LanceDbDeleteInput, LanceDbDeleteResult, LanceDbDropTableInput, LanceDbDropTableResult,
    LanceDbInputFormat, LanceDbOutputFormat, LanceDbSearchInput, LanceDbSearchResult,
    LanceDbUpsertInput, LanceDbUpsertResult,
};
use vulcan_sqlite_engine::error::SqliteEngineError;
use vulcan_sqlite_engine::runtime::{SqliteDatabaseHandle, SqliteRuntime};
pub use vulcan_sqlite_engine::runtime::{
    SqliteHardeningOptions, SqliteOpenOptions, SqlitePragmaOptions,
};

use crate::data_plane::APP_SCHEMA_VERSION;

/// A logical database resource owned by the host.
/// 宿主持有的一个逻辑数据库资源。
///
/// The set covers every storage surface the plan names for Phase Four: application, system
/// statistics, VCE, VMM, knowledge base, plugin and skill.
///
/// 该集合覆盖方案为阶段四列出的全部存储面：应用、系统统计、VCE、VMM、知识库、插件与技能。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LogicalDatabase {
    /// Application state (projects, sessions, preferences, credentials).
    /// 应用状态（项目、会话、偏好、凭据）。
    AppState,
    /// Host-owned provider usage and system statistics.
    /// 宿主持有的 Provider 用量与系统统计。
    SystemStatistics,
    /// VCE metadata and vector tables.
    /// VCE 元数据与向量表。
    VceMetadata,
    /// VMM metadata and lexical index.
    /// VMM 元数据与词法索引。
    VmmMetadata,
    /// Knowledge base documents, FTS index and dictionaries.
    /// 知识库文档、FTS 索引与词典。
    KnowledgeBase,
    /// Plugin-owned storage.
    /// 插件持有的存储。
    PluginStorage,
    /// Skill-owned storage.
    /// 技能持有的存储。
    SkillStorage,
}

impl LogicalDatabase {
    /// Returns every logical database, so callers can iterate the whole set without drifting.
    /// 返回全部逻辑数据库，使调用方无需自行维护而遍历完整集合。
    pub fn all() -> [Self; 7] {
        [
            Self::AppState,
            Self::SystemStatistics,
            Self::VceMetadata,
            Self::VmmMetadata,
            Self::KnowledgeBase,
            Self::PluginStorage,
            Self::SkillStorage,
        ]
    }

    /// Returns a stable name used in diagnostics.
    /// 返回用于诊断的稳定名称。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AppState => "app-state",
            Self::SystemStatistics => "system-statistics",
            Self::VceMetadata => "vce-metadata",
            Self::VmmMetadata => "vmm-metadata",
            Self::KnowledgeBase => "knowledge-base",
            Self::PluginStorage => "plugin-storage",
            Self::SkillStorage => "skill-storage",
        }
    }
}

/// One LanceDB directory together with the limits its owning subsystem declared.
/// 一个 LanceDB 目录，以及其所属子系统声明的各项限制。
///
/// Why a directory rather than a logical name / 为何是目录而不是逻辑名:
///
/// LanceDB databases are not a fixed set. The application has one, every VCE project has its own,
/// and every plugin has its own, so the number of databases is unbounded and cannot be enumerated by
/// a `LogicalDatabase` variant. What is stable is the rule the controller used and that the plan
/// keeps: one subsystem derives one directory and the engine holds its tables directly in it. The
/// directory itself is therefore the identity, and it is the cache key.
///
/// LanceDB 数据库不是固定集合。应用有一个，每个 VCE 项目各有一个，每个插件也各有一个，因此数据库
/// 数量无界，无法用 `LogicalDatabase` 变体枚举。稳定的是控制器曾使用且方案保留的规则：一个子系统派生
/// 一个目录，引擎把表直接放在其中。因此目录本身就是身份，也是缓存键。
///
/// Why the limits travel with the binding / 为何限制随绑定一起传递:
///
/// The controller applied per-binding limits (`max_upsert_payload`, `max_search_limit`,
/// `max_concurrent_requests`), and those are real bounds that callers depend on rather than
/// decoration. They are carried here so the owning subsystem keeps declaring them instead of the
/// runtime inventing one global value for every database.
///
/// 控制器按绑定施加限制（`max_upsert_payload`、`max_search_limit`、`max_concurrent_requests`），
/// 这些是调用方真正依赖的边界而非装饰。它们随绑定传递，使所属子系统继续声明它们，而不是由运行时为
/// 每个数据库臆造一个全局值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanceDbDirectoryBinding {
    /// Directory whose tables belong to this database.
    /// 其表属于该数据库的目录。
    pub directory: PathBuf,
    /// Read-consistency window in milliseconds; `None` disables the window.
    /// 读一致性窗口，单位为毫秒；`None` 表示关闭该窗口。
    pub read_consistency_interval_ms: Option<u64>,
    /// Maximum accepted write payload in bytes.
    /// 可接受的最大写入载荷字节数。
    pub max_upsert_payload: usize,
    /// Maximum rows one search may return.
    /// 单次检索可返回的最大行数。
    pub max_search_limit: usize,
    /// Maximum concurrent in-flight requests.
    /// 最大并发在途请求数。
    pub max_concurrent_requests: usize,
}

impl LanceDbDirectoryBinding {
    /// Returns the engine options this binding declares.
    /// 返回本绑定声明的引擎选项。
    pub fn engine_options(&self) -> LanceDbEngineOptions {
        LanceDbEngineOptions {
            max_upsert_payload: self.max_upsert_payload,
            max_search_limit: self.max_search_limit,
            max_concurrent_requests: self.max_concurrent_requests,
        }
    }

    /// Builds the manager configuration that resolves to exactly this directory.
    /// 构建恰好解析到本目录的管理器配置。
    ///
    /// `default_db_path` and `db_root` are both the directory: the host installs every LanceDB
    /// binding that way, and the engine keeps a database's tables directly in its path
    /// rather than in a child of it.
    ///
    /// `default_db_path` 与 `db_root` 都是该目录：宿主对所有 LanceDB 绑定都是这样安装的，而引擎把
    /// 数据库的表直接放在其路径中，而不是其子目录里。
    fn runtime_config(&self) -> DatabaseRuntimeConfig {
        DatabaseRuntimeConfig {
            default_db_path: self.directory.clone(),
            db_root: Some(self.directory.clone()),
            read_consistency_interval_ms: self.read_consistency_interval_ms,
        }
    }
}

/// One SQLite database file together with the open options its owning subsystem declared.
/// 一个 SQLite 数据库文件，以及其所属子系统声明的打开选项。
///
/// Why the file, not only a logical name / 为何是文件，而不只是逻辑名:
///
/// [`LogicalDatabase`] enumerates the host's **singular** databases, and that is all it can do. Two
/// of the seven surfaces the plan names are not singular: the knowledge base keeps one relational
/// database and one display database **per library**, VCE keeps one per project, and plugin and
/// skill storage keep one per instance. There is no fixed set to enumerate, so those bindings are
/// keyed by the file the subsystem derived.
///
/// [`LogicalDatabase`] 枚举宿主**单例**数据库，这也是它唯一能做的事。方案点名的七个存储面中有两个并非
/// 单例：知识库**每个资料库**各有一个关系库与一个显示库，VCE 每个项目一个，插件与技能存储每个实例一个。
/// 没有固定集合可枚举，因此这些绑定以子系统派生的文件为键。
///
/// Why the options travel with the binding / 为何选项随绑定一起传递:
///
/// The controller's `enable_sqlite` applied per-binding `busy_timeout_ms`, `cache_size_kib` and
/// `read_only`. Those are real settings that differ per subsystem, so they are declared here by the
/// owner rather than defaulted uniformly by the runtime.
///
/// 控制器的 `enable_sqlite` 按绑定施加 `busy_timeout_ms`、`cache_size_kib` 与 `read_only`。这些是
/// 确实按子系统不同的设置，因此由所有者在此声明，而不是由运行时统一取默认值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqliteFileBinding {
    /// Physical file that holds this database.
    /// 保存该数据库的物理文件。
    pub path: PathBuf,
    /// Programmatic open options: pragmas, hardening and busy timeout.
    /// 程序化打开选项：PRAGMA、硬化与忙碌超时。
    pub options: SqliteOpenOptions,
}

impl SqliteFileBinding {
    /// Creates a binding for one file with the engine's v1 baseline options.
    /// 为单个文件创建使用引擎 v1 基线选项的绑定。
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            options: SqliteOpenOptions::default(),
        }
    }
}

/// The outcome of one database's startup health check.
/// 单个数据库启动健康检查的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseHealth {
    /// Logical resource that was checked.
    /// 被检查的逻辑资源。
    logical: LogicalDatabase,
    /// Physical path that was opened.
    /// 被打开的物理路径。
    path: PathBuf,
    /// Schema version read back from the database after opening.
    /// 打开后从数据库读回的结构版本号。
    schema_version: i64,
    /// Whether the exclusive file lock is owned by this process.
    /// 独占文件锁是否由本进程持有。
    lock_owned: bool,
    /// Whether a minimal write-then-read round trip succeeded.
    /// 最小"写入后读回"往返是否成功。
    read_write_verified: bool,
}

impl DatabaseHealth {
    /// Returns the logical resource that was checked.
    /// 返回被检查的逻辑资源。
    pub fn logical(&self) -> LogicalDatabase {
        self.logical
    }

    /// Returns the physical path that was opened.
    /// 返回被打开的物理路径。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the schema version read back after opening.
    /// 返回打开后读回的结构版本号。
    pub fn schema_version(&self) -> i64 {
        self.schema_version
    }

    /// Returns whether this process owns the database file lock.
    /// 返回本进程是否持有该数据库文件锁。
    pub fn lock_owned(&self) -> bool {
        self.lock_owned
    }

    /// Returns whether the minimal read/write round trip succeeded.
    /// 返回最小读写往返是否成功。
    pub fn read_write_verified(&self) -> bool {
        self.read_write_verified
    }
}

/// A startup health report for every bound database.
/// 每个已绑定数据库的启动健康报告。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalDatabaseHealthReport {
    /// One entry per bound database, ordered by [`LogicalDatabase`] for stable output.
    /// 每个已绑定数据库一项，按 [`LogicalDatabase`] 排序以获得稳定输出。
    pub databases: Vec<DatabaseHealth>,
}

impl LocalDatabaseHealthReport {
    /// Returns whether every bound database passed its health check.
    /// 返回是否每个已绑定数据库都通过了健康检查。
    pub fn is_healthy(&self) -> bool {
        self.databases
            .iter()
            .all(|health| health.lock_owned && health.read_write_verified)
    }

    /// Returns the health entry for one logical database, if it was bound.
    /// 返回某个逻辑数据库的健康条目（若已绑定）。
    pub fn get(&self, logical: LogicalDatabase) -> Option<&DatabaseHealth> {
        self.databases
            .iter()
            .find(|health| health.logical == logical)
    }
}

/// Startup failure for the host-local database runtime.
/// 宿主本地数据库运行时的启动失败。
#[derive(Debug, thiserror::Error)]
pub enum LocalDatabaseError {
    /// A logical database was requested without a binding.
    /// 请求了一个未绑定的逻辑数据库。
    #[error("logical database '{logical}' has no physical binding")]
    Unbound {
        /// Logical resource name that was not bound.
        /// 未绑定的逻辑资源名称。
        logical: &'static str,
    },

    /// Opening a SQLite database failed.
    /// 打开 SQLite 数据库失败。
    #[error("failed to open the '{logical}' database: {source}")]
    Sqlite {
        /// Logical resource name involved in the failure.
        /// 失败涉及的逻辑资源名称。
        logical: &'static str,
        /// Underlying engine failure.
        /// 底层引擎失败。
        #[source]
        source: SqliteEngineError,
    },

    /// Opening the LanceDB database failed.
    /// 打开 LanceDB 数据库失败。
    #[error("failed to open the LanceDB database: {0}")]
    LanceDb(#[from] LanceDbEngineError),

    /// The startup health check failed.
    /// 启动健康检查失败。
    #[error("startup health check failed for '{logical}': {detail}")]
    HealthCheck {
        /// Logical resource name that failed its check.
        /// 未通过检查的逻辑资源名称。
        logical: &'static str,
        /// Human-readable failure description.
        /// 失败的可读描述。
        detail: String,
    },

    /// The application database is not at the compiled v1 schema version.
    /// 应用数据库不处于编译期 v1 结构版本。
    #[error(
        "application schema version mismatch: expected {expected}, found {observed}; v1 has no migration path"
    )]
    SchemaVersionMismatch {
        /// Schema version this build expects.
        /// 本构建期望的结构版本。
        expected: i64,
        /// Schema version actually present in the database.
        /// 数据库中实际存在的结构版本。
        observed: i64,
    },

    /// A logical resource was already bound to a different physical path.
    /// 某个逻辑资源已绑定到不同的物理路径。
    #[error(
        "logical database '{logical}' is already bound to {existing}; refusing to rebind it to {requested}"
    )]
    BindingConflict {
        /// Logical resource name that was already bound.
        /// 已绑定的逻辑资源名称。
        logical: &'static str,
        /// Path the resource is already bound to.
        /// 该资源已绑定的路径。
        existing: PathBuf,
        /// Path that was requested instead.
        /// 被请求改绑的路径。
        requested: PathBuf,
    },

    /// One LanceDB directory was opened with two different sets of limits.
    /// 同一个 LanceDB 目录被以两套不同的限制打开。
    #[error(
        "LanceDB directory '{}' is already open with different limits",
        directory.display()
    )]
    LanceDbLimitsConflict {
        /// Directory that was already opened.
        /// 已被打开的目录。
        directory: PathBuf,
        /// Binding that opened it first.
        /// 首次打开它的绑定。
        existing: Box<LanceDbDirectoryBinding>,
        /// Binding that was requested afterwards.
        /// 随后请求的绑定。
        requested: Box<LanceDbDirectoryBinding>,
    },

    /// Opening one declared database file failed.
    /// 打开一个已声明的数据库文件失败。
    ///
    /// This is separate from [`Self::Sqlite`] because the subject is a path rather than one of the
    /// enumerated logical resources, and the path is what the caller has to act on.
    ///
    /// 它与 [`Self::Sqlite`] 分开，因为主体是一个路径而不是枚举出的逻辑资源之一，而路径正是调用方需要
    /// 据以行动的信息。
    #[error("failed to open SQLite database '{}': {source}", path.display())]
    SqliteFile {
        /// File that could not be opened.
        /// 无法打开的文件。
        path: PathBuf,
        /// Underlying engine failure.
        /// 底层引擎失败。
        #[source]
        source: SqliteEngineError,
    },

    /// The runtime has been shut down and no longer opens databases.
    /// 运行时已关闭，不再打开数据库。
    #[error("database runtime is shut down and no longer opens databases")]
    ShutDown,

    /// A named binding was resolved before it was registered, or registered inconsistently.
    /// 具名绑定在登记之前被解析，或被不一致地登记。
    ///
    /// Unregistered and conflicting are distinct causes but share one reporting shape because both are
    /// host-side programming errors rather than recoverable database conditions, and both must expose
    /// the identity and the physical path involved.
    /// 未登记与冲突是不同成因，但共用一种报告形态，因为二者都是宿主侧编程错误而非可恢复的数据库状况，
    /// 且二者都必须暴露所涉及的身份与物理路径。
    #[error("named binding '{name}' {detail}")]
    NamedBinding {
        /// Caller-owned identity that failed to resolve or conflicted.
        /// 解析失败或发生冲突的调用方拥有身份。
        name: String,
        /// What went wrong, including the physical path involved.
        /// 出错的原因，含所涉及的物理路径。
        detail: String,
    },
}

/// How many blocking database operations the host runs at once.
/// 宿主一次运行多少个阻塞数据库操作。
///
/// The host already declares four as its parallel-work ceiling for building and testing, so database work
/// follows the same ceiling instead of quietly outgrowing the resource the host sized for it.
/// 宿主已为构建与测试声明四个并行工作上限，因此数据库工作遵循同一上限，而不是悄悄超出宿主为它设定的资源规模。
pub const MAX_CONCURRENT_BLOCKING_DATABASE_OPERATIONS: usize = 4;

/// One physical database as the blocking boundary names it.
/// 阻塞边界用于命名一个物理数据库的键。
///
/// The boundary serializes by physical resource rather than by subsystem, because two subsystems writing one
/// file is a property of the file.
/// 边界按物理资源而非子系统串行化，因为两个子系统写同一个文件是该文件的属性。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DatabaseKey {
    /// One of the host's singular logical databases.
    /// 宿主单例逻辑数据库之一。
    Logical(LogicalDatabase),
    /// One declared database file, named by its resolved path.
    /// 一个已声明数据库文件，由其解析后的路径命名。
    File(PathBuf),
}

/// Why a blocking database operation did not produce a value.
/// 阻塞数据库操作未产生值的原因。
#[derive(Debug)]
pub enum BlockingBoundaryError<E> {
    /// Admission or pre-execution preparation exhausted the caller's deadline.
    /// 准入或执行前准备已耗尽调用方截止时间。
    DeadlineExceeded,
    /// The operation ran and failed.
    /// 操作已运行并失败。
    Operation(E),
    /// The operation could not be run to completion, so its blocking task ended abnormally.
    /// 操作未能运行至完成，因此其阻塞任务异常结束。
    Task {
        /// What the runtime reported about the ended task.
        /// 运行时关于该已结束任务的报告。
        detail: String,
    },
}

/// The host's controlled blocking execution resource and per-database access gate.
/// 宿主的受控阻塞执行资源与每数据库访问门。
///
/// Two requirements live here so a subsystem cannot satisfy one and forget the other. SQLite's blocking
/// calls must run in a host-controlled blocking resource rather than on Tokio core threads, because a
/// blocked core thread is a stalled executor rather than a slow query. And access to one database must
/// serialize, so a write, a schema change, an FTS maintenance pass and a job receipt against the same file
/// cannot interleave.
/// 两项要求在此共存，使子系统无法只满足其一而遗忘其二。SQLite 的阻塞调用必须在宿主受控的阻塞资源中运行而
/// 非在 Tokio 核心线程上，因为被阻塞的核心线程是执行器被卡住，而不是查询变慢。而对同一数据库的访问必须
/// 串行化，使针对同一文件的写入、结构变更、FTS 维护与作业回执无法交错。
///
/// Reads share the same gate as writes rather than running freely. That is deliberate: a read holds a
/// SQLite snapshot for its duration, and a concurrent close or layout change of the same file would
/// invalidate it. Different databases still proceed independently, which is the granularity that matters.
/// 读取与写入共享同一道门，而不是自由并行。这是刻意的：读取在其持续期间持有 SQLite 快照，而同一文件上并发的
/// 关闭或布局变更会使其失效。不同数据库仍相互独立推进，这才是重要的粒度。
///
/// A gated call must not nest another gated call for the same database: the gate is not reentrant, and
/// nesting would deadlock rather than wait. An operation needing several statements on one database takes
/// the gate once and runs the sequence inside a single call.
/// 已取门的调用不得为同一数据库嵌套另一个取门调用：该门不可重入，嵌套会死锁而非等待。需要在同一数据库上执行
/// 多条语句的操作应只取一次门，并在单次调用内运行整个序列。
#[derive(Clone)]
pub struct DatabaseBlockingBoundary {
    /// Configured concurrency ceiling, independent of current permit usage.
    /// 已配置的并发上限，不随当前许可占用变化。
    max_concurrent: usize,
    /// Caps how many blocking database operations are in flight; the rest queue.
    /// 限制同时在途的阻塞数据库操作数量；其余排队。
    permits: Arc<tokio::sync::Semaphore>,
    /// One gate per physical database.
    /// 每个物理数据库一道门。
    gates: Arc<Mutex<HashMap<DatabaseKey, Arc<tokio::sync::Mutex<()>>>>>,
}

impl DatabaseBlockingBoundary {
    /// Creates one boundary that admits the given number of concurrent blocking operations.
    /// 创建一个允许给定数量并发阻塞操作的边界。
    ///
    /// # Parameters / 参数
    /// `max_concurrent` is the concurrency ceiling; zero is raised to one so the boundary cannot deadlock.
    /// `max_concurrent` 是并发上限；零会被提升为一，使边界不会死锁。
    ///
    /// # Returns / 返回值
    /// Returns a boundary with a fresh set of gates.
    /// 返回带有全新一套门的边界。
    pub fn new(max_concurrent: usize) -> Self {
        let max_concurrent = max_concurrent.max(1);
        Self {
            max_concurrent,
            permits: Arc::new(tokio::sync::Semaphore::new(max_concurrent)),
            gates: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Returns the concurrency ceiling this boundary was created with.
    /// 返回创建本边界时所用的并发上限。
    pub fn max_concurrent(&self) -> usize {
        self.max_concurrent
    }

    /// Returns the gate for one database, creating it on first use.
    /// 返回某个数据库的门，首次使用时创建。
    async fn gate_for(&self, key: &DatabaseKey) -> Arc<tokio::sync::Mutex<()>> {
        let mut gates = self
            .gates
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Arc::clone(
            gates
                .entry(key.clone())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))),
        )
    }

    /// Runs one blocking database operation off the core threads, gated by its database.
    /// 在核心线程之外运行一次阻塞数据库操作，并受其数据库的门约束。
    ///
    /// # Parameters / 参数
    /// `key` names the physical database whose gate must be held for the whole operation.
    /// `key` 命名必须为整个操作持有门的物理数据库。
    /// `operation` is the blocking work; it runs on the blocking pool and must not await.
    /// `operation` 是阻塞工作；它运行在阻塞池上且不得 await。
    ///
    /// # Returns / 返回值
    /// Returns the operation's value, its own error, or a task-level failure when the blocking task ended
    /// abnormally.
    /// 返回操作的值、其自身错误，或阻塞任务异常结束时的任务级失败。
    pub async fn run<T, E, F>(
        &self,
        key: DatabaseKey,
        operation: F,
    ) -> Result<T, BlockingBoundaryError<E>>
    where
        F: FnOnce() -> Result<T, E> + Send + 'static,
        T: Send + 'static,
        E: Send + 'static,
    {
        self.run_until(key, None, operation).await
    }

    /// Runs identity preparation on the same bounded blocking budget before a database gate is known.
    /// 在数据库闸门身份确定之前，使用同一有界阻塞预算执行身份准备。
    /// `deadline` bounds admission; `operation` owns its inputs and must not acquire a database gate.
    /// `deadline` 限制准入等待；`operation` 持有输入且不得获取数据库闸门。
    /// Returns the prepared value or the original operation/task failure after real work ends.
    /// 在实际工作结束后返回准备值或原始操作／任务失败。
    pub async fn prepare_until<T, E, F>(
        &self,
        deadline: Option<std::time::Instant>,
        operation: F,
    ) -> Result<T, BlockingBoundaryError<E>>
    where
        F: FnOnce() -> Result<T, E> + Send + 'static,
        T: Send + 'static,
        E: Send + 'static,
    {
        // Preparation never holds a database gate; release this permit before ordinary gate admission.
        // 准备阶段不持有数据库闸门；普通闸门准入前释放本许可，避免锁顺序反转。
        let admission = Arc::clone(&self.permits).acquire_owned();
        let permit = if let Some(deadline) = deadline {
            tokio::time::timeout_at(deadline.into(), admission)
                .await
                .map_err(|_| BlockingBoundaryError::DeadlineExceeded)?
        } else {
            admission.await
        }
        .map_err(|error| BlockingBoundaryError::Task {
            detail: error.to_string(),
        })?;
        tokio::task::spawn_blocking(move || {
            // The actual worker retains capacity even if the asynchronous caller is cancelled.
            // 即使异步调用方取消，实际工作线程仍保留容量占用。
            let _permit = permit;
            if deadline.is_some_and(|limit| std::time::Instant::now() >= limit) {
                return Err(BlockingBoundaryError::DeadlineExceeded);
            }
            operation().map_err(BlockingBoundaryError::Operation)
        })
        .await
        .map_err(|error| BlockingBoundaryError::Task {
            detail: format!("database preparation task ended abnormally: {error}"),
        })?
    }

    /// Waits within an optional caller deadline, then owns gates until blocking work actually returns.
    /// 在可选调用方截止时间内等待，随后持有同步门直至阻塞任务实际返回。
    /// Expiry removes queued work; running operations enforce their own interrupt contract.
    /// 到期移除排队任务；运行中的操作遵循自身中断契约。
    pub async fn run_until<T, E, F>(
        &self,
        key: DatabaseKey,
        deadline: Option<std::time::Instant>,
        operation: F,
    ) -> Result<T, BlockingBoundaryError<E>>
    where
        F: FnOnce() -> Result<T, E> + Send + 'static,
        T: Send + 'static,
        E: Send + 'static,
    {
        // Both queue stages share one deadline, without changing ownership after dispatch.
        // 两个排队阶段共用一个截止时间，调度后不改变所有权。
        let admission = async {
            let gate = self.gate_for(&key).await;
            let guard = gate.lock_owned().await;
            // Acquire the per-database gate before a global permit. Same-database waiters must not consume every
            // global slot and starve unrelated databases while they wait for work that is intentionally serialized.
            // 先获取每数据库门，再获取全局许可。同库等待者不得在等待刻意串行化的工作时占满全部全局槽位并饿死其他数据库。
            let permit = Arc::clone(&self.permits)
                .acquire_owned()
                .await
                .map_err(|_| BlockingBoundaryError::Task {
                    detail: String::from("the boundary semaphore was closed"),
                })?;
            Ok::<_, BlockingBoundaryError<E>>((guard, permit))
        };
        let (guard, permit) = if let Some(deadline) = deadline {
            tokio::time::timeout_at(deadline.into(), admission)
                .await
                .map_err(|_| BlockingBoundaryError::DeadlineExceeded)??
        } else {
            admission.await?
        };
        // The permit and the gate guard move into the blocking task so both are held for exactly as long as
        // the work runs, and released even if the closure panics.
        // 许可与门守卫被移入阻塞任务，因此二者恰好在工作运行期间被持有，且即使闭包 panic 也会被释放。
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let _guard = guard;
            operation()
        })
        .await
        .map_err(|error| BlockingBoundaryError::Task {
            detail: format!("blocking database task ended abnormally: {error}"),
        })?
        .map_err(BlockingBoundaryError::Operation)
    }
}
/// One cached LanceDB directory: its binding and the runtime that owns both the engine and the root lock.
/// 一个已缓存的 LanceDB 目录：其绑定，以及同时拥有引擎与库根锁的运行时。
type CachedLanceDirectory = (LanceDbDirectoryBinding, Arc<LanceDbRuntime>);

/// LanceDB directories cached by physical path.
/// 按物理路径缓存的 LanceDB 目录。
type LanceDirectoryCache = HashMap<PathBuf, CachedLanceDirectory>;

/// The in-process database runtime every subsystem shares.
/// 每个子系统共享的进程内数据库运行时。
///
/// Opening is **lazy**, and that is deliberate. Every open takes an exclusive file lock, so a runtime that eagerly
/// opened every binding would lock databases the host never touches — and would collide with controller-era paths
/// while both still exist. Bindings are therefore recorded at construction and a database is opened on first use,
/// with [`Self::health_report`] as the explicit point that opens and verifies every bound database.
///
/// 打开是**惰性**的，且这是刻意的。每次打开都会取得独占文件锁，因此急切打开全部绑定的运行时会锁住宿主
/// 从不触碰的数据库——并在新旧路径并存期间与控制器时代的路径冲突。因此绑定在构造时记录，数据库在首次
/// 使用时打开，而 [`Self::health_report`] 是显式打开并校验每个已绑定数据库的位置。
#[derive(Clone)]
pub struct LocalDatabaseRuntime {
    /// Shared SQLite engine.
    /// 共享的 SQLite 引擎。
    sqlite: Arc<SqliteRuntime>,
    /// Logical-to-physical bindings.
    /// 逻辑到物理绑定。
    ///
    /// Mutable behind a lock so a subsystem's bootstrap can register its own already-derived paths
    /// into the one shared registry — see [`Self::bind`]. It is write-once per logical resource:
    /// re-registering the same path is a no-op and a different path is a conflict.
    ///
    /// 锁后可变，使子系统的引导可把已派生的路径注册进唯一的共享注册表——见 [`Self::bind`]。每个逻辑
    /// 资源只写一次：重复注册相同路径是无操作，不同路径则是冲突。
    bindings: Arc<Mutex<HashMap<LogicalDatabase, PathBuf>>>,
    /// Lazily opened SQLite handles, keyed by logical resource.
    /// 惰性打开的 SQLite 句柄，按逻辑资源键控。
    opened: Arc<Mutex<HashMap<LogicalDatabase, Arc<SqliteDatabaseHandle>>>>,
    /// One LanceDB engine per physical directory, with the binding that opened it.
    /// 每个物理目录一个 LanceDB 引擎，并记录打开它的绑定。
    ///
    /// The key is the resolved directory rather than a subsystem name because LanceDB databases are
    /// unbounded — one per VCE project and one per plugin. Keying by directory is also what upholds
    /// the engine's own invariant: a second engine over one directory would hold a second set of
    /// per-table locks for the same tables.
    ///
    /// 键是解析后的目录而非子系统名，因为 LanceDB 数据库数量无界——每个 VCE 项目一个、每个插件一个。
    /// 按目录键控也维护了引擎自身的不变量：同一目录上的第二个引擎会为相同的表持有第二套表级锁。
    /// The owning runtime is cached beside the directory, not only the engine it produced.
    /// 与目录一同缓存的是**拥有者运行时**，而不仅是它产出的引擎。
    ///
    /// The runtime owns the database-root lock and the one-engine-per-path cache, so an engine kept without its
    /// runtime would keep working while the process's exclusivity claim on the directory was already released —
    /// another process could then open the same directory as a second writer with no error anywhere.
    /// 运行时拥有库根锁与"每路径一个引擎"的缓存，因此只保留引擎而丢弃运行时会留下"引擎仍可用、而本进程对该目录的
    /// 独占声明已释放"的状态——另一个进程随后就能以第二个写者身份打开同一目录，且任何地方都不会报错。
    lance: Arc<Mutex<LanceDirectoryCache>>,
    /// Per-directory gates that serialize only the first runtime publication for each physical path.
    /// 按目录设置的门，仅串行化每个物理路径的首次运行时发布。
    ///
    /// Weak entries avoid retaining an unbounded set of directories after callers close them. Concurrent
    /// first-open callers still share the same live gate through their strong references.
    /// 弱引用条目避免在调用方关闭目录后永久保留无界目录集合；并发首开调用方仍通过各自强引用共享同一道存活门。
    lance_open_gates: Arc<Mutex<HashMap<PathBuf, Weak<tokio::sync::Mutex<()>>>>>,
    /// Bindings registered under a caller-chosen name.
    /// 以调用方选择的名称登记的绑定。
    ///
    /// Subsystems whose databases are unbounded — one per knowledge base, per VCE project, per plugin
    /// — address them through an identity they generate themselves rather than through a
    /// [`LogicalDatabase`] variant. This registry is what turns such an identity back into the binding,
    /// so a helper that only holds the identity can reach the engine without eagerly opening the
    /// database and taking its lock (see the lock constraint).
    /// 数据库数量无界的子系统——每个知识库、每个 VCE 项目、每个插件各一个——通过它们自行生成的身份而非
    /// [`LogicalDatabase`] 变体来寻址这些数据库。本注册表把这样的身份还原为绑定，因此只持有身份的助手无需
    /// 提前打开数据库并取得其锁即可抵达引擎（参见锁约束）。
    named: Arc<Mutex<HashMap<String, SqliteFileBinding>>>,
    /// LanceDB directory bindings registered under a caller-chosen name, keyed the same way.
    /// 以调用方选择的名称登记的 LanceDB 目录绑定，键控方式相同。
    ///
    /// A separate map rather than one heterogeneous registry, because the two binding types name
    /// different things: a file and a directory. Keeping them apart means a name can never resolve to
    /// the wrong kind of binding.
    /// 使用单独的映射而非一个异构注册表，因为两种绑定类型命名的是不同的事物：一个文件与一个目录。将它们
    /// 分开意味着一个名称绝不会解析到错误类型的绑定。
    named_lance: Arc<Mutex<HashMap<String, LanceDbDirectoryBinding>>>,
    /// The host's controlled blocking execution resource and per-database gate.
    /// 宿主的受控阻塞执行资源与每数据库门。
    boundary: DatabaseBlockingBoundary,
    /// Synchronous SQLite admission gate shared by opens and runtime shutdown.
    /// SQLite 同步打开与运行时关闭共享的准入闸门。
    ///
    /// A read guard spans each synchronous handle/connection open. Shutdown sets the atomic refusal flag first,
    /// then takes the write guard before clearing the engine registry, so an opener can never publish behind a
    /// completed shutdown.
    /// 读锁覆盖每次同步句柄/连接打开。关闭先设置原子拒绝标记，再取得写锁并清空引擎注册表，因此打开者绝不
    /// 会在一次已经完成的关闭之后重新发布句柄。
    sqlite_admission: Arc<std::sync::RwLock<()>>,
    /// Whether this runtime has been shut down.
    /// 本运行时是否已关闭。
    shut_down: Arc<std::sync::atomic::AtomicBool>,
}

impl LocalDatabaseRuntime {
    /// Creates a runtime over caller-supplied bindings without opening any database.
    /// 在调用方提供的绑定上创建运行时，不打开任何数据库。
    ///
    /// Bindings come from the caller, which is the single authoritative place allowed to derive
    /// storage paths. A resource without a binding is simply unavailable, so the host can run with
    /// a subset of surfaces while a migration is in progress without a silent fallback.
    ///
    /// 绑定由调用方提供，那是唯一被允许派生存储路径的权威位置。没有绑定的资源就是不可用，因此宿主可以在
    /// 迁移进行中只启用部分存储面，而不产生静默回退。
    pub fn open(bindings: HashMap<LogicalDatabase, PathBuf>) -> Result<Self, LocalDatabaseError> {
        for (logical, path) in &bindings {
            if path.as_os_str().is_empty() {
                return Err(LocalDatabaseError::HealthCheck {
                    logical: logical.as_str(),
                    detail: "bound database path must not be empty".to_string(),
                });
            }
        }

        Ok(Self {
            sqlite: Arc::new(SqliteRuntime::new()),
            bindings: Arc::new(Mutex::new(bindings)),
            opened: Arc::new(Mutex::new(HashMap::new())),
            lance: Arc::new(Mutex::new(HashMap::new())),
            lance_open_gates: Arc::new(Mutex::new(HashMap::new())),
            named: Arc::new(Mutex::new(HashMap::new())),
            named_lance: Arc::new(Mutex::new(HashMap::new())),
            boundary: DatabaseBlockingBoundary::new(MAX_CONCURRENT_BLOCKING_DATABASE_OPERATIONS),
            sqlite_admission: Arc::new(std::sync::RwLock::new(())),
            shut_down: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }

    /// Registers one logical-to-physical binding on an already-created runtime.
    /// 在已创建的运行时上注册一个逻辑到物理的绑定。
    ///
    /// A subsystem's bootstrap is the authoritative place for its own paths — the VMM knows where
    /// its state lives and the application bootstrap does not. This lets such a bootstrap register
    /// its already-derived path into the one shared runtime rather than creating a second runtime,
    /// so the host keeps a single registry and a single health check.
    ///
    /// 子系统的引导是其自身路径的权威位置——VMM 知道自己的状态在哪里，而应用引导不知道。本方法让这类引导
    /// 把已派生的路径注册进唯一的共享运行时，而不是另建运行时，使宿主保持单一注册表与单次健康检查。
    ///
    /// Re-registering the same path is a no-op; a different path is a conflict, because two physical
    /// files claiming one logical identity would make later reads depend on registration order.
    ///
    /// 重复注册相同路径是无操作；不同路径是冲突，因为两个物理文件争夺同一逻辑身份会使后续读取取决于注册顺序。
    pub fn bind(
        &self,
        logical: LogicalDatabase,
        path: impl Into<PathBuf>,
    ) -> Result<(), LocalDatabaseError> {
        let path = path.into();
        if path.as_os_str().is_empty() {
            return Err(LocalDatabaseError::HealthCheck {
                logical: logical.as_str(),
                detail: "bound database path must not be empty".to_string(),
            });
        }

        let mut bindings = self
            .bindings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        match bindings.get(&logical) {
            None => {
                bindings.insert(logical, path);
                Ok(())
            }
            Some(existing) if existing == &path => Ok(()),
            Some(existing) => Err(LocalDatabaseError::BindingConflict {
                logical: logical.as_str(),
                existing: existing.clone(),
                requested: path,
            }),
        }
    }

    /// Returns the shared SQLite engine.
    /// 返回共享的 SQLite 引擎。
    pub fn sqlite(&self) -> &Arc<SqliteRuntime> {
        &self.sqlite
    }

    /// Opens or reuses the SQLite handle for one declared database file.
    /// 打开或复用某个已声明数据库文件的 SQLite 句柄。
    ///
    /// The engine owns the "one handle per normalized path" invariant and refuses to reuse a path
    /// that is already open with **different** options, so a subsystem cannot silently get a
    /// database whose PRAGMA set was decided by whichever caller arrived first. That refusal is
    /// surfaced unchanged rather than converted, because it is already the right error.
    ///
    /// 引擎持有"每个规范化路径一个句柄"的不变量，并拒绝复用已以**不同**选项打开的路径，因此子系统不会
    /// 静默得到一个其 PRAGMA 集合由先到调用方决定的数据库。该拒绝原样呈现而不转换，因为它本身已是正确的
    /// 错误。
    ///
    /// # Errors
    /// 错误
    /// Returns [`LocalDatabaseError::ShutDown`] after shutdown, [`LocalDatabaseError::SqliteFile`]
    /// when the file cannot be opened, or the engine's own resource conflict.
    /// 关闭后返回 [`LocalDatabaseError::ShutDown`]，文件无法打开时返回 [`LocalDatabaseError::SqliteFile`]，
    /// 否则返回引擎自身的资源冲突。
    pub fn open_sqlite_file(
        &self,
        binding: &SqliteFileBinding,
    ) -> Result<Arc<SqliteDatabaseHandle>, LocalDatabaseError> {
        if binding.path.as_os_str().is_empty() {
            return Err(LocalDatabaseError::HealthCheck {
                logical: "sqlite-file",
                detail: "declared database file path must not be empty".to_string(),
            });
        }
        let _admission = self
            .sqlite_admission
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.ensure_running()?;
        self.open_sqlite_file_admitted(binding)
    }

    /// Opens one declared SQLite file while the caller holds the synchronous admission guard.
    /// 在调用方持有同步准入锁期间打开一个已声明 SQLite 文件。
    fn open_sqlite_file_admitted(
        &self,
        binding: &SqliteFileBinding,
    ) -> Result<Arc<SqliteDatabaseHandle>, LocalDatabaseError> {
        self.sqlite
            .open_database_with_options(&binding.path, binding.options.clone())
            .map_err(|source| LocalDatabaseError::SqliteFile {
                path: binding.path.clone(),
                source,
            })
    }

    /// Closes the engine handle for one declared database file, releasing its lock.
    /// 关闭某个已声明数据库文件的引擎句柄，释放其锁。
    ///
    /// The engine keeps every opened database alive until it is told to close it, so a subsystem that
    /// replaces or deletes a database directory must close the handle first. On Windows an open file
    /// makes its containing directory undeletable, so without this the replacement fails with
    /// "access denied" rather than with a lock error.
    /// 引擎会保持每个已打开的数据库存活，直到被告知关闭它，因此替换或删除数据库目录的子系统必须先关闭句柄。
    /// 在 Windows 上，打开的文件会使其所在目录无法删除，因此缺少这一步时替换会以"拒绝访问"而非锁错误失败。
    ///
    /// # Parameters / 参数
    /// `binding` is the declared binding whose file should be closed.
    /// `binding` 是应当关闭其文件的已声明绑定。
    ///
    /// # Returns / 返回值
    /// Returns whether a handle was open and is now closed.
    /// 返回是否曾有句柄打开并现已关闭。
    pub fn close_sqlite_file(&self, binding: &SqliteFileBinding) -> bool {
        self.sqlite.close_database(&binding.path)
    }
    /// Opens one connection to a declared database file.
    /// 打开到某个已声明数据库文件的一个连接。
    pub fn connect_sqlite_file(
        &self,
        binding: &SqliteFileBinding,
    ) -> Result<rusqlite::Connection, LocalDatabaseError> {
        let _admission = self
            .sqlite_admission
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.ensure_running()?;
        self.open_sqlite_file_admitted(binding)?
            .open_connection()
            .map_err(|source| LocalDatabaseError::SqliteFile {
                path: binding.path.clone(),
                source,
            })
    }

    /// Opens or reuses the LanceDB engine whose tables live directly in one directory.
    /// 打开或复用其表直接位于某个目录中的 LanceDB 引擎。
    ///
    /// The first caller for a directory decides that database's limits, because one directory must
    /// have exactly one engine: a second engine would hold a second set of per-table locks and the
    /// engine's own mutation barrier would no longer be process-wide. A later caller that declares
    /// different limits for the same directory therefore gets a conflict rather than a silently
    /// different database.
    ///
    /// 某个目录的第一个调用方决定该数据库的限制，因为一个目录必须恰好只有一个引擎：第二个引擎会持有
    /// 第二套表级锁，引擎自身的变更屏障将不再是进程级的。因此后来者为同一目录声明不同限制时会得到冲突，
    /// 而不是一个静默不同的数据库。
    ///
    /// # Errors
    /// 错误
    /// Returns [`LocalDatabaseError::LanceDbLimitsConflict`] for a directory already open with
    /// different limits, a shutdown error, or the engine's own open failure.
    /// 目录已以不同限制打开时返回 [`LocalDatabaseError::LanceDbLimitsConflict`]，否则返回关闭错误或
    /// 引擎自身的打开失败。
    pub async fn lance_engine(
        &self,
        binding: LanceDbDirectoryBinding,
    ) -> Result<LanceDbEngine, LocalDatabaseError> {
        if binding.directory.as_os_str().is_empty() {
            return Err(LocalDatabaseError::HealthCheck {
                logical: "lancedb",
                detail: "LanceDB database directory must not be empty".to_string(),
            });
        }
        if self.shut_down.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(LocalDatabaseError::ShutDown);
        }
        // Normalization happens before either cache lookup or engine construction, so lexical aliases cannot
        // publish two runtimes and two independent table-lock sets for one physical directory.
        // 在缓存查询或引擎构造之前完成规范化，使字面别名不能为同一物理目录发布两个运行时及两套独立表锁。
        let binding = normalize_lance_binding(&binding)?;

        // Both cache accesses happen in synchronous helpers. A standard-library `MutexGuard` is `!Send`, so a guard
        // that lived in this async body — even one dropped before an await — would make the future non-`Send` and
        // break every caller that submits it to the host channel.
        // 两次缓存访问都在同步辅助函数中完成。标准库 `MutexGuard` 是 `!Send`，因此任何出现在本异步函数体内的守卫
        // ——即使它会在 await 之前被丢弃——都会使该 future 非 `Send`，并让每个把它提交给宿主通道的调用方编译失败。
        if let Some(runtime) = self.cached_lance_runtime(&binding)? {
            return Ok(runtime.open_default_engine().await?);
        }

        let open_gate = self.lance_open_gate(&binding.directory);
        let _open_guard = open_gate.lock_owned().await;
        // A first opener may have published while this call waited. Rechecking under the per-directory gate is
        // load-bearing: constructing a second runtime would make it wait on a root lock whose engine is published
        // in the first runtime's private cache, where the second runtime can never observe it.
        // 本调用等待期间首开者可能已经发布。取得每目录门后重新检查是承重逻辑：若构造第二个运行时，它会等待一把根锁，
        // 而对应引擎发布在首个运行时的私有缓存中，第二个运行时永远无法观察到它。
        if let Some(runtime) = self.cached_lance_runtime(&binding)? {
            return Ok(runtime.open_default_engine().await?);
        }

        // The open runs outside the cache lock so a slow connect cannot block unrelated directories. The
        // per-directory gate above prevents duplicate runtimes for this directory without serializing other paths.
        // The cached runtime owns the root lock for exactly as long as the store serves the directory.
        // 打开发生在缓存锁之外，使缓慢连接不会阻塞无关目录。上方每目录门避免本目录产生重复运行时，同时不串行化其他
        // 路径。缓存运行时在存储服务该目录的整个期间持有根锁。
        let runtime = Arc::new(LanceDbRuntime::new(
            binding.runtime_config(),
            binding.engine_options(),
        ));
        // Opening happens before publishing, so a directory that cannot be opened leaves no entry behind and still
        // takes the root lock here rather than leaving it to a later caller.
        // 打开发生在发布之前，故无法打开的目录不会留下条目，且根锁仍在此处取得而不留给后续调用方。
        let opened = runtime.open_default_engine().await?;

        let selected = self.publish_lance_runtime(binding, &runtime)?;
        if Arc::ptr_eq(&selected, &runtime) {
            return Ok(opened);
        }
        // A racing call published first, so this call's runtime — and with it this call's root lock — is released
        // when this function returns. Serving the entry's runtime is therefore the only correct answer.
        // 竞争的调用先发布了，故本次调用的运行时（连同其根锁）在函数返回时释放。因此服务条目中的运行时是唯一
        // 正确的答案。
        Ok(selected.open_default_engine().await?)
    }

    /// Returns the live first-open gate for one normalized LanceDB directory.
    /// 返回一个已规范化 LanceDB 目录的存活首开门。
    ///
    /// # Parameters / 参数
    /// `directory` is the normalized physical path used by the runtime cache.
    /// `directory` 是运行时缓存所用的规范化物理路径。
    ///
    /// # Returns / 返回值
    /// Returns a shared asynchronous gate. The registry stores only a weak reference so closed directories do not
    /// accumulate permanently.
    /// 返回共享异步门。注册表仅保存弱引用，因此已关闭目录不会永久累积。
    fn lance_open_gate(&self, directory: &Path) -> Arc<tokio::sync::Mutex<()>> {
        let mut gates = self
            .lance_open_gates
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        gates.retain(|_, gate| gate.strong_count() > 0);
        if let Some(gate) = gates.get(directory).and_then(Weak::upgrade) {
            return gate;
        }

        let gate = Arc::new(tokio::sync::Mutex::new(()));
        gates.insert(directory.to_path_buf(), Arc::downgrade(&gate));
        gate
    }

    /// Closes one exact LanceDB directory after its owner has drained all external operation leases.
    /// 在所属方排空全部外部操作租约后关闭一个精确的 LanceDB 目录。
    ///
    /// # Parameters / 参数
    /// `binding` is the exact directory and limit contract that was used to open the engine.
    /// `binding` 是打开引擎时使用的精确目录与限额契约。
    ///
    /// # Returns / 返回值
    /// Returns whether the directory was cached, or an explicit conflict when the path is open under a
    /// different contract. Callers must serialize this lifecycle boundary with their own operation leases;
    /// removing the cache entry releases the root lock but cannot revoke engine clones already handed out.
    /// 返回目录是否曾被缓存；若路径以另一份契约打开则返回明确冲突。调用方必须用自身操作租约串行化该生命周期
    /// 边界；移除缓存条目会释放根锁，但无法撤销已经交出的引擎克隆。
    pub fn close_lance_directory(
        &self,
        binding: &LanceDbDirectoryBinding,
    ) -> Result<bool, LocalDatabaseError> {
        let binding = normalize_lance_binding(binding)?;
        let mut open = self
            .lance
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match open.get(&binding.directory) {
            Some((existing, _)) if existing != &binding => {
                Err(LocalDatabaseError::LanceDbLimitsConflict {
                    directory: binding.directory.clone(),
                    existing: Box::new(existing.clone()),
                    requested: Box::new(binding.clone()),
                })
            }
            Some(_) => {
                open.remove(&binding.directory);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Returns the runtime cached for one directory, or the limits conflict that forbids reusing it.
    /// 返回为某目录缓存的运行时，或禁止复用它的一致性冲突。
    ///
    /// # Parameters / 参数
    /// `binding` supplies the directory and the limits being requested.
    /// `binding` 提供目录与本次请求的限制。
    ///
    /// # Returns / 返回值
    /// Returns `Ok(None)` when the directory has no entry yet, `Ok(Some(_))` for a matching entry, and
    /// [`LocalDatabaseError::LanceDbLimitsConflict`] when the directory is open with different limits.
    /// 目录尚无条目时返回 `Ok(None)`，条目匹配时返回 `Ok(Some(_))`，目录已以不同限制打开时返回
    /// [`LocalDatabaseError::LanceDbLimitsConflict`]。
    fn cached_lance_runtime(
        &self,
        binding: &LanceDbDirectoryBinding,
    ) -> Result<Option<Arc<LanceDbRuntime>>, LocalDatabaseError> {
        let open = self
            .lance
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match open.get(&binding.directory) {
            Some((existing, runtime)) if existing == binding => Ok(Some(Arc::clone(runtime))),
            Some((existing, _)) => Err(LocalDatabaseError::LanceDbLimitsConflict {
                directory: binding.directory.clone(),
                existing: Box::new(existing.clone()),
                requested: Box::new(binding.clone()),
            }),
            None => Ok(None),
        }
    }

    /// Publishes one runtime for a directory and returns the runtime its entry holds.
    /// 为一个目录发布运行时，并返回其条目所持有的运行时。
    ///
    /// # Parameters / 参数
    /// `binding` is the directory and limits being published; `runtime` is this call's runtime.
    /// `binding` 是正在发布的目录与限制；`runtime` 是本次调用的运行时。
    ///
    /// # Returns / 返回值
    /// Returns the entry's runtime, which is this call's unless a racing call published first.
    /// 返回条目中的运行时；除非竞争的调用先发布，否则它就是本次调用的运行时。
    fn publish_lance_runtime(
        &self,
        binding: LanceDbDirectoryBinding,
        runtime: &Arc<LanceDbRuntime>,
    ) -> Result<Arc<LanceDbRuntime>, LocalDatabaseError> {
        let mut open = self
            .lance
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Shutdown sets the refusal flag before taking this same cache lock. An opener that completed its slow
        // connection after that boundary must release its private runtime instead of republishing it.
        // 关闭会在取得同一缓存锁之前设置拒绝标记。若打开者在该边界之后才完成缓慢连接，它必须释放私有运行时，
        // 而不能重新发布。
        self.ensure_running()?;
        let entry = open
            .entry(binding.directory.clone())
            .or_insert_with(|| (binding, Arc::clone(runtime)));
        Ok(Arc::clone(&entry.1))
    }

    /// Returns a LanceDB database's directory when it has been opened, for diagnostics.
    /// 返回某个 LanceDB 数据库的目录（若已打开），用于诊断。
    pub fn lance_directories(&self) -> Vec<PathBuf> {
        self.lance
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .keys()
            .cloned()
            .collect()
    }

    /// Returns the physical path bound to one logical database.
    /// 返回绑定到某个逻辑数据库的物理路径。
    pub fn path_for(&self, logical: LogicalDatabase) -> Result<PathBuf, LocalDatabaseError> {
        self.bindings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&logical)
            .cloned()
            .ok_or(LocalDatabaseError::Unbound {
                logical: logical.as_str(),
            })
    }

    /// Returns the opened handle for one logical database, opening it on first use.
    /// 返回某个逻辑数据库的句柄，首次使用时打开。
    pub fn handle_for(
        &self,
        logical: LogicalDatabase,
    ) -> Result<Arc<SqliteDatabaseHandle>, LocalDatabaseError> {
        let _admission = self
            .sqlite_admission
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.ensure_running()?;
        self.handle_for_admitted(logical)
    }

    /// Opens or returns one logical SQLite handle while the caller owns synchronous admission.
    /// 在调用方持有同步准入期间打开或返回一个逻辑 SQLite 句柄。
    fn handle_for_admitted(
        &self,
        logical: LogicalDatabase,
    ) -> Result<Arc<SqliteDatabaseHandle>, LocalDatabaseError> {
        let path = self.path_for(logical)?;

        let mut opened = self
            .opened
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(handle) = opened.get(&logical) {
            return Ok(Arc::clone(handle));
        }

        // Open outside the registry's own lock is unnecessary here: `SqliteRuntime` already
        // serializes per-path registration, and holding this lock across the open is what prevents
        // two threads from double-opening the same logical resource.
        // 此处无需在注册表锁之外执行打开：`SqliteRuntime` 本身已对按路径注册做串行化，而持有本锁跨越
        // 打开过程正是防止两个线程重复打开同一逻辑资源的手段。
        let handle =
            self.sqlite
                .open_database(path)
                .map_err(|source| LocalDatabaseError::Sqlite {
                    logical: logical.as_str(),
                    source,
                })?;
        opened.insert(logical, Arc::clone(&handle));
        Ok(handle)
    }

    /// Opens one connection to a bound logical database.
    /// 打开到某个已绑定逻辑数据库的一个连接。
    ///
    /// Shutdown is checked here rather than only at the entry points being migrated, so a caller
    /// that races a shutdown receives an error instead of a connection to a released database.
    ///
    /// 关闭状态在此处检查，而不是仅在正被迁移的入口处检查，使与关闭竞争的调用方收到错误，而不是拿到一个
    /// 指向已释放数据库的连接。
    pub fn connect(
        &self,
        logical: LogicalDatabase,
    ) -> Result<rusqlite::Connection, LocalDatabaseError> {
        let _admission = self
            .sqlite_admission
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.ensure_running()?;
        let handle = self.handle_for_admitted(logical)?;
        handle
            .open_connection()
            .map_err(|source| LocalDatabaseError::Sqlite {
                logical: logical.as_str(),
                source,
            })
    }

    /// Opens and verifies every bound database: schema version, lock ownership and a read/write
    /// round trip.
    /// 打开并校验每个已绑定数据库：结构版本、锁所有权与读写往返。
    ///
    /// This is the explicit point at which every bound database is opened, so a host that calls it
    /// during startup both takes the exclusive locks and learns whether the data is intact. A
    /// resource without a binding is skipped rather than reported as unhealthy.
    ///
    /// 这是显式打开每个已绑定数据库的位置，因此在启动时调用它的宿主既取得了独占锁，也得知数据是否完整。
    /// 没有绑定的资源会被跳过，而不会报告为不健康。
    pub fn health_report(&self) -> Result<LocalDatabaseHealthReport, LocalDatabaseError> {
        let _admission = self
            .sqlite_admission
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.ensure_running()?;
        let mut databases = Vec::new();

        for logical in LogicalDatabase::all() {
            if !self
                .bindings
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .contains_key(&logical)
            {
                continue;
            }

            let handle = self.handle_for_admitted(logical)?;
            let connection =
                handle
                    .open_connection()
                    .map_err(|source| LocalDatabaseError::Sqlite {
                        logical: logical.as_str(),
                        source,
                    })?;

            let schema_version: i64 = connection
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .map_err(|error| LocalDatabaseError::HealthCheck {
                    logical: logical.as_str(),
                    detail: format!("failed to read user_version: {error}"),
                })?;

            // A write-then-read against a temporary table proves the file is genuinely writable and
            // not merely openable, which is the difference between an intact database and a
            // read-only mount or a half-initialized file.
            // 针对临时表的"写入后读回"证明文件确实可写，而不只是可打开——这正是完整数据库与只读挂载或
            // 半初始化文件的区别。
            connection
                .execute_batch("CREATE TEMP TABLE IF NOT EXISTS _vulcan_health (value INTEGER);")
                .and_then(|()| {
                    connection.execute("INSERT INTO _vulcan_health (value) VALUES (1)", [])
                })
                .map_err(|error| LocalDatabaseError::HealthCheck {
                    logical: logical.as_str(),
                    detail: format!("minimal write failed: {error}"),
                })?;

            let read_back: i64 = connection
                .query_row("SELECT COUNT(*) FROM _vulcan_health", [], |row| row.get(0))
                .map_err(|error| LocalDatabaseError::HealthCheck {
                    logical: logical.as_str(),
                    detail: format!("minimal read failed: {error}"),
                })?;

            databases.push(DatabaseHealth {
                logical,
                path: handle.db_path().to_path_buf(),
                schema_version,
                lock_owned: handle.lock_path().is_some() && handle.lock_owner().is_some(),
                read_write_verified: read_back == 1,
            });
        }

        Ok(LocalDatabaseHealthReport { databases })
    }

    /// Reads the application database's schema version.
    /// 读取应用数据库的结构版本。
    pub fn app_schema_version(&self) -> Result<i64, LocalDatabaseError> {
        let _admission = self
            .sqlite_admission
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.ensure_running()?;
        let handle = self.handle_for_admitted(LogicalDatabase::AppState)?;
        let connection = handle
            .open_connection()
            .map_err(|source| LocalDatabaseError::Sqlite {
                logical: LogicalDatabase::AppState.as_str(),
                source,
            })?;

        connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(|error| LocalDatabaseError::HealthCheck {
                logical: LogicalDatabase::AppState.as_str(),
                detail: format!("failed to read user_version: {error}"),
            })
    }

    /// Verifies the application database is at the compiled v1 schema version.
    /// 校验应用数据库处于编译期 v1 结构版本。
    ///
    /// A mismatch is an error, never a repair. The plan forbids compatibility branches and migration
    /// logic, so a database at another version must fail loudly rather than be silently adopted or
    /// silently upgraded.
    ///
    /// 不匹配是错误，绝不修复。方案禁止兼容分支与迁移逻辑，因此版本不同的数据库必须响亮失败，而不是被
    /// 静默接受或静默升级。
    pub fn verify_app_schema_version(&self) -> Result<i64, LocalDatabaseError> {
        let observed = self.app_schema_version()?;
        if observed != APP_SCHEMA_VERSION {
            return Err(LocalDatabaseError::SchemaVersionMismatch {
                expected: APP_SCHEMA_VERSION,
                observed,
            });
        }

        Ok(observed)
    }

    /// Registers a binding under a caller-chosen name, or confirms an identical existing one.
    /// 以调用方选择的名称登记绑定，或确认已存在的相同绑定。
    ///
    /// Re-registering the same name with the same directory is a no-op, so a subsystem that resolves
    /// its binding on every operation does not accumulate state. Registering the same name with a
    /// different directory is a conflict rather than a silent replacement, because that would mean two
    /// parts of the host disagree about which file one identity names.
    /// 以相同目录重复登记同名绑定是无操作，因此在每次操作时解析绑定的子系统不会累积状态。以不同目录登记
    /// 同名绑定是冲突而非静默替换，因为那意味着宿主的两部分对同一身份指向哪个文件存在分歧。
    ///
    /// # Parameters / 参数
    /// `name` is the caller-owned identity.
    /// `name` 是调用方拥有的身份。
    /// `binding` is the physical binding that identity resolves to.
    /// `binding` 是该身份解析到的物理绑定。
    ///
    /// # Returns / 返回值
    /// Returns success, or the naming conflict.
    /// 返回成功，或命名冲突。
    pub fn register_named_binding(
        &self,
        name: &str,
        binding: &SqliteFileBinding,
    ) -> Result<(), LocalDatabaseError> {
        let mut named = self
            .named
            .lock()
            .map_err(|_| LocalDatabaseError::NamedBinding {
                name: String::from(name),
                detail: String::from(
                    "could not be registered because the registry lock was poisoned",
                ),
            })?;
        match named.get(name) {
            Some(existing) if existing != binding => Err(LocalDatabaseError::NamedBinding {
                name: String::from(name),
                detail: format!(
                    "already names SQLite binding {:?}, not {:?}",
                    existing, binding
                ),
            }),
            Some(_) => Ok(()),
            None => {
                named.insert(String::from(name), binding.clone());
                Ok(())
            }
        }
    }

    /// Removes one exact caller-named SQLite binding after its database handle has closed.
    /// 在数据库句柄关闭后移除一个精确的调用方命名 SQLite 绑定。
    ///
    /// # Parameters / 参数
    /// `name` is the caller-owned identity and `binding` is the exact contract expected under that name.
    /// `name` 是调用方拥有的身份，`binding` 是该名称下预期的精确契约。
    ///
    /// # Returns / 返回值
    /// Returns whether the binding existed, or a conflict when the name currently identifies another binding.
    /// 返回绑定是否存在；若名称当前标识另一绑定则返回冲突。
    pub fn unregister_named_binding(
        &self,
        name: &str,
        binding: &SqliteFileBinding,
    ) -> Result<bool, LocalDatabaseError> {
        let mut named = self
            .named
            .lock()
            .map_err(|_| LocalDatabaseError::NamedBinding {
                name: String::from(name),
                detail: String::from(
                    "could not be unregistered because the registry lock was poisoned",
                ),
            })?;
        match named.get(name) {
            Some(existing) if existing != binding => Err(LocalDatabaseError::NamedBinding {
                name: String::from(name),
                detail: format!(
                    "currently names SQLite binding {:?}, not {:?}",
                    existing, binding
                ),
            }),
            Some(_) => Ok(named.remove(name).is_some()),
            None => Ok(false),
        }
    }

    /// Returns the host's blocking boundary for database work.
    /// 返回宿主用于数据库工作的阻塞边界。
    ///
    /// Subsystems must route their SQLite calls through this rather than calling the synchronous
    /// [`crate::local_sql`] entry points directly: those run on whichever Tokio worker called them.
    /// 子系统必须让 SQLite 调用经由它，而不是直接调用同步的 [`crate::local_sql`] 入口：后者会在碰巧调用它们
    /// 的那个 Tokio 工作线程上运行。
    pub fn boundary(&self) -> &DatabaseBlockingBoundary {
        &self.boundary
    }

    /// Registers a LanceDB directory binding under a caller-chosen name.
    /// 以调用方选择的名称登记 LanceDB 目录绑定。
    ///
    /// # Parameters / 参数
    /// `name` is the caller-owned identity.
    /// `name` 是调用方拥有的身份。
    /// `binding` is the directory binding that identity resolves to.
    /// `binding` 是该身份解析到的目录绑定。
    ///
    /// # Returns / 返回值
    /// Returns success, or the naming conflict.
    /// 返回成功，或命名冲突。
    pub fn register_named_lance_binding(
        &self,
        name: &str,
        binding: &LanceDbDirectoryBinding,
    ) -> Result<(), LocalDatabaseError> {
        let binding = normalize_lance_binding(binding)?;
        let mut named = self
            .named_lance
            .lock()
            .map_err(|_| LocalDatabaseError::NamedBinding {
                name: String::from(name),
                detail: String::from(
                    "could not be registered because the registry lock was poisoned",
                ),
            })?;
        match named.get(name) {
            Some(existing) if existing != &binding => Err(LocalDatabaseError::NamedBinding {
                name: String::from(name),
                detail: format!(
                    "already names LanceDB binding {:?}, not {:?}",
                    existing, binding
                ),
            }),
            Some(_) => Ok(()),
            None => {
                named.insert(String::from(name), binding);
                Ok(())
            }
        }
    }

    /// Removes one exact caller-named LanceDB binding after its directory engine has closed.
    /// 在目录引擎关闭后移除一个精确的调用方命名 LanceDB 绑定。
    ///
    /// # Parameters / 参数
    /// `name` is the caller-owned identity and `binding` is the exact contract expected under that name.
    /// `name` 是调用方拥有的身份，`binding` 是该名称下预期的精确契约。
    ///
    /// # Returns / 返回值
    /// Returns whether the binding existed, or a conflict when the name currently identifies another binding.
    /// 返回绑定是否存在；若名称当前标识另一绑定则返回冲突。
    pub fn unregister_named_lance_binding(
        &self,
        name: &str,
        binding: &LanceDbDirectoryBinding,
    ) -> Result<bool, LocalDatabaseError> {
        let binding = normalize_lance_binding(binding)?;
        let mut named = self
            .named_lance
            .lock()
            .map_err(|_| LocalDatabaseError::NamedBinding {
                name: String::from(name),
                detail: String::from(
                    "could not be unregistered because the registry lock was poisoned",
                ),
            })?;
        match named.get(name) {
            Some(existing) if existing != &binding => Err(LocalDatabaseError::NamedBinding {
                name: String::from(name),
                detail: format!(
                    "currently names LanceDB binding {:?}, not {:?}",
                    existing, binding
                ),
            }),
            Some(_) => Ok(named.remove(name).is_some()),
            None => Ok(false),
        }
    }

    /// Resolves a previously registered name to its LanceDB directory binding.
    /// 把先前登记的名称解析为其 LanceDB 目录绑定。
    ///
    /// # Parameters / 参数
    /// `name` is the caller-owned identity handed to the helper.
    /// `name` 是交给助手的调用方拥有身份。
    ///
    /// # Returns / 返回值
    /// Returns a copy of the registered binding, or an invalid-argument error when unregistered.
    /// 返回已登记绑定的副本；未登记时返回非法参数错误。
    pub fn named_lance_binding(
        &self,
        name: &str,
    ) -> Result<LanceDbDirectoryBinding, LocalDatabaseError> {
        let named = self
            .named_lance
            .lock()
            .map_err(|_| LocalDatabaseError::NamedBinding {
                name: String::from(name),
                detail: String::from(
                    "could not be resolved because the registry lock was poisoned",
                ),
            })?;
        named
            .get(name)
            .cloned()
            .ok_or_else(|| LocalDatabaseError::NamedBinding {
                name: String::from(name),
                detail: String::from("is not registered"),
            })
    }
    /// Resolves a previously registered name to its binding.
    /// 把先前登记的名称解析为其绑定。
    ///
    /// # Parameters / 参数
    /// `name` is the caller-owned identity handed to the helper.
    /// `name` 是交给助手的调用方拥有身份。
    ///
    /// # Returns / 返回值
    /// Returns a copy of the registered binding, or an invalid-argument error when the name was never
    /// registered — an unregistered identity is a programming error, never a fallback.
    /// 返回已登记绑定的副本；当名称从未被登记时返回非法参数错误——未登记的身份是编程错误，绝不是回退。
    pub fn named_binding(&self, name: &str) -> Result<SqliteFileBinding, LocalDatabaseError> {
        let named = self
            .named
            .lock()
            .map_err(|_| LocalDatabaseError::NamedBinding {
                name: String::from(name),
                detail: String::from(
                    "could not be resolved because the registry lock was poisoned",
                ),
            })?;
        named
            .get(name)
            .cloned()
            .ok_or_else(|| LocalDatabaseError::NamedBinding {
                name: String::from(name),
                detail: String::from("is not registered"),
            })
    }
    /// Shuts the runtime down: stops accepting connections and releases every database handle.
    /// 关闭运行时：停止接受连接并释放每个数据库句柄。
    ///
    /// The order matters. The shut-down flag is set **first** so a concurrent caller cannot acquire
    /// a connection that is about to be released, and only then are the handles dropped. Dropping a
    /// handle releases its exclusive file lock, which is what lets the next process open the
    /// database.
    ///
    /// 顺序很重要。先设置关闭标记，使并发调用方无法取得一个即将被释放的连接；随后才释放句柄。释放句柄会
    /// 解除其独占文件锁，这正是下一个进程能够打开该数据库的前提。
    pub fn shutdown(&self) {
        if self
            .shut_down
            .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            return;
        }

        // Every synchronous opener that observed the accepting state finishes before registries are cleared.
        // Any later opener waits here and then observes `ShutDown`.
        // 所有观察到接纳状态的同步打开者都会在注册表清空前完成；更晚的打开者在此等待，随后观察到
        // `ShutDown`。
        let _sqlite_shutdown = self
            .sqlite_admission
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        {
            let mut opened = self
                .opened
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            opened.clear();
        }

        // The engine registry also contains unbounded named databases for skills, plugins, VCE and knowledge
        // bases. Clearing only fixed logical paths would retain their ownership locks after shutdown.
        // 引擎注册表还包含技能、插件、VCE 与知识库的无界命名数据库。若只清理固定逻辑路径，关闭后仍会保留
        // 它们的所有权锁。
        self.sqlite.close_all_databases();

        // Clearing the cached runtimes releases their database-root ownership locks together with the
        // connections and per-table locks, so a later host process can open each directory cleanly.
        // 清空缓存运行时会同时释放库根所有权锁、连接与表级锁，使后续宿主进程能够干净地打开各目录。
        self.lance
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        self.lance_open_gates
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        self.named
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        self.named_lance
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }

    /// Refuses an operation after the runtime-wide shutdown boundary.
    /// 在运行时级关闭边界之后拒绝操作。
    fn ensure_running(&self) -> Result<(), LocalDatabaseError> {
        if self.shut_down.load(std::sync::atomic::Ordering::SeqCst) {
            Err(LocalDatabaseError::ShutDown)
        } else {
            Ok(())
        }
    }

    /// Returns whether this runtime has been shut down.
    /// 返回本运行时是否已关闭。
    pub fn is_shut_down(&self) -> bool {
        self.shut_down.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Normalizes one LanceDB directory binding without requiring the directory to exist.
/// 在不要求目录已经存在的情况下规范化一个 LanceDB 目录绑定。
///
/// # Parameters / 参数
/// `binding` is the caller-declared directory and engine limit contract.
/// `binding` 是调用方声明的目录与引擎限额契约。
///
/// # Returns / 返回值
/// Returns the same contract with an absolute, lexically normalized directory, or an explicit path error.
/// 返回目录已转为绝对字面规范路径的同一契约，或明确的路径错误。
fn normalize_lance_binding(
    binding: &LanceDbDirectoryBinding,
) -> Result<LanceDbDirectoryBinding, LocalDatabaseError> {
    let absolute = if binding.directory.is_absolute() {
        binding.directory.clone()
    } else {
        std::env::current_dir()
            .map_err(|error| LocalDatabaseError::HealthCheck {
                logical: "lancedb",
                detail: format!(
                    "cannot resolve relative LanceDB directory against the working directory: {error}"
                ),
            })?
            .join(&binding.directory)
    };
    // normalized removes only lexical dot segments; canonicalization would require a directory that this open
    // operation is itself responsible for creating.
    // normalized 只移除字面点号路径段；规范化文件系统路径会要求目录预先存在，而该目录本应由本次打开创建。
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(
                    normalized.components().next_back(),
                    Some(Component::Normal(_))
                ) {
                    normalized.pop();
                }
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    let mut normalized_binding = binding.clone();
    normalized_binding.directory = normalized;
    Ok(normalized_binding)
}

#[cfg(test)]
mod tests {
    use super::{
        LanceDbDirectoryBinding, LocalDatabaseError, LocalDatabaseRuntime, LogicalDatabase,
        SqliteFileBinding,
    };
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    /// Builds bindings for a temporary directory covering the requested resources.
    /// 为临时目录构建覆盖所请求资源的绑定。
    fn bindings_for(
        directory: &std::path::Path,
        resources: &[LogicalDatabase],
    ) -> HashMap<LogicalDatabase, PathBuf> {
        resources
            .iter()
            .map(|logical| {
                (
                    *logical,
                    directory.join(format!("{}.sqlite3", logical.as_str())),
                )
            })
            .collect()
    }

    /// Builds a LanceDB directory binding for a temporary directory.
    /// 为临时目录构建一个 LanceDB 目录绑定。
    ///
    /// The limits are the migration's stable defaults; a production caller declares its own.
    /// 限制采用迁移的稳定默认值；生产调用方声明自己的限制。
    fn lance_binding(directory: &std::path::Path) -> LanceDbDirectoryBinding {
        LanceDbDirectoryBinding {
            directory: directory.join("lance"),
            read_consistency_interval_ms: None,
            max_upsert_payload: 50 * 1024 * 1024,
            max_search_limit: 10_000,
            max_concurrent_requests: 500,
        }
    }

    /// Builds a runtime over a fresh temporary directory with every resource bound.
    /// 在全新临时目录上构建绑定了全部资源的运行时。
    fn runtime_with_all_resources() -> (tempfile::TempDir, LocalDatabaseRuntime) {
        let directory = tempfile::tempdir().expect("temporary directory should exist");
        let runtime =
            LocalDatabaseRuntime::open(bindings_for(directory.path(), &LogicalDatabase::all()))
                .expect("runtime should open every bound database");

        (directory, runtime)
    }

    #[test]
    fn logical_database_names_are_stable_and_unique() {
        let all = LogicalDatabase::all();
        assert_eq!(all.len(), 7, "the plan names seven storage surfaces");

        let names = all
            .iter()
            .map(|logical| logical.as_str())
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(names.len(), all.len(), "names must be unique");
    }

    #[test]
    fn opening_binds_every_requested_resource() {
        let (_directory, runtime) = runtime_with_all_resources();

        for logical in LogicalDatabase::all() {
            assert!(
                runtime.path_for(logical).is_ok(),
                "{logical:?} should be bound"
            );
            assert!(
                runtime.handle_for(logical).is_ok(),
                "{logical:?} should be opened"
            );
        }
    }

    #[test]
    fn unbound_resource_is_reported_rather_than_invented() {
        let directory = tempfile::tempdir().expect("temporary directory should exist");
        let runtime = LocalDatabaseRuntime::open(bindings_for(
            directory.path(),
            &[LogicalDatabase::AppState],
        ))
        .expect("runtime should open");

        assert!(runtime.path_for(LogicalDatabase::AppState).is_ok());

        let missing = runtime.path_for(LogicalDatabase::KnowledgeBase);
        assert!(
            matches!(missing, Err(LocalDatabaseError::Unbound { .. })),
            "an unbound resource must be reported, not derived, got {missing:?}"
        );
    }

    #[test]
    fn health_report_covers_every_bound_database_and_passes() {
        let (_directory, runtime) = runtime_with_all_resources();
        let report = runtime.health_report().expect("health check should run");

        assert_eq!(report.databases.len(), LogicalDatabase::all().len());
        assert!(
            report.is_healthy(),
            "a freshly created database must pass every health check: {report:?}"
        );

        for health in &report.databases {
            assert!(
                health.lock_owned(),
                "{:?} must be lock-owned by this process",
                health.logical()
            );
            assert!(
                health.read_write_verified(),
                "{:?} must pass a write-then-read round trip",
                health.logical()
            );
            assert!(health.path().exists(), "the database file must exist");
        }
    }

    #[test]
    fn health_report_entries_are_ordered_by_logical_resource() {
        let (_directory, runtime) = runtime_with_all_resources();
        let report = runtime.health_report().expect("health check should run");

        let observed = report
            .databases
            .iter()
            .map(|health| health.logical())
            .collect::<Vec<_>>();
        let mut expected = LogicalDatabase::all().to_vec();
        expected.sort();
        assert_eq!(
            observed, expected,
            "health entries must be in stable logical order"
        );
    }

    #[test]
    fn a_fresh_database_reports_schema_version_zero() {
        let (_directory, runtime) = runtime_with_all_resources();

        // Nothing has initialized the v1 schema yet, so `user_version` is still the SQLite default.
        // This pins that the accessor reads the real database rather than a cached assumption.
        // v1 结构尚未初始化，因此 `user_version` 仍是 SQLite 默认值。这固定了该访问器读取的是真实
        // 数据库而非缓存的假设。
        assert_eq!(
            runtime
                .app_schema_version()
                .expect("schema version should be readable"),
            0
        );
    }

    #[test]
    fn schema_version_mismatch_is_an_error_not_a_repair() {
        let (_directory, runtime) = runtime_with_all_resources();

        // A fresh database is at version 0, which is not the compiled v1 version. The verifier must
        // refuse it, and must not have written a version of its own.
        // 全新数据库处于版本 0，不是编译期 v1 版本。校验器必须拒绝它，且不得自行写入任何版本号。
        let result = runtime.verify_app_schema_version();
        assert!(
            matches!(
                result,
                Err(LocalDatabaseError::SchemaVersionMismatch { expected, observed })
                    if expected == crate::data_plane::APP_SCHEMA_VERSION && observed == 0
            ),
            "a version mismatch must be reported, got {result:?}"
        );

        assert_eq!(
            runtime
                .app_schema_version()
                .expect("schema version should still be readable"),
            0,
            "the verifier must not have rewritten the version"
        );
    }

    #[test]
    fn schema_version_at_the_compiled_version_verifies() {
        let (_directory, runtime) = runtime_with_all_resources();

        runtime
            .connect(LogicalDatabase::AppState)
            .expect("connect")
            .execute_batch(&format!(
                "PRAGMA user_version = {};",
                crate::data_plane::APP_SCHEMA_VERSION
            ))
            .expect("set schema version");

        assert_eq!(
            runtime
                .verify_app_schema_version()
                .expect("matching version must verify"),
            crate::data_plane::APP_SCHEMA_VERSION
        );
    }

    #[test]
    fn connect_reuses_the_bound_handle_and_sees_written_data() {
        let (_directory, runtime) = runtime_with_all_resources();

        {
            let connection = runtime
                .connect(LogicalDatabase::AppState)
                .expect("connect should succeed");
            connection
                .execute_batch("CREATE TABLE probe (value INTEGER); INSERT INTO probe VALUES (7);")
                .expect("write should succeed");
        }

        // A second connection must observe the first connection's committed write, proving both
        // come from the same database file rather than separate opens.
        // 第二个连接必须能看到第一个连接已提交的写入，证明二者来自同一个数据库文件而非各自打开。
        let connection = runtime
            .connect(LogicalDatabase::AppState)
            .expect("second connect should succeed");
        let value: i64 = connection
            .query_row("SELECT value FROM probe", [], |row| row.get(0))
            .expect("read should succeed");
        assert_eq!(value, 7);
    }

    #[test]
    fn connect_to_an_unbound_resource_is_refused() {
        let directory = tempfile::tempdir().expect("temporary directory should exist");
        let runtime = LocalDatabaseRuntime::open(bindings_for(
            directory.path(),
            &[LogicalDatabase::AppState],
        ))
        .expect("runtime should open");

        assert!(runtime.connect(LogicalDatabase::VceMetadata).is_err());
    }

    #[test]
    fn bind_adds_a_binding_that_then_works() {
        let directory = tempfile::tempdir().expect("temporary directory should exist");
        let runtime = LocalDatabaseRuntime::open(bindings_for(
            directory.path(),
            &[LogicalDatabase::AppState],
        ))
        .expect("runtime should open");

        // A subsystem bootstrap registers its own already-derived path into the one shared runtime.
        // 子系统引导把其已派生的路径注册进唯一的共享运行时。
        let vmm_path = directory.path().join("vmm-state.sqlite3");
        runtime
            .bind(LogicalDatabase::VmmMetadata, vmm_path.clone())
            .expect("first bind should succeed");

        assert_eq!(
            runtime
                .path_for(LogicalDatabase::VmmMetadata)
                .expect("bound path"),
            vmm_path
        );

        runtime
            .connect(LogicalDatabase::VmmMetadata)
            .expect("a newly bound database must be connectable")
            .execute_batch("CREATE TABLE vmm_probe (value INTEGER);")
            .expect("write to the newly bound database");

        assert!(vmm_path.exists(), "connecting must create the bound file");
    }

    #[test]
    fn bind_is_idempotent_for_the_same_path_and_a_conflict_for_a_different_one() {
        let directory = tempfile::tempdir().expect("temporary directory should exist");
        let runtime = LocalDatabaseRuntime::open(bindings_for(
            directory.path(),
            &[LogicalDatabase::AppState],
        ))
        .expect("runtime should open");

        let path = directory.path().join("vce.sqlite3");
        runtime
            .bind(LogicalDatabase::VceMetadata, path.clone())
            .expect("first bind");
        runtime
            .bind(LogicalDatabase::VceMetadata, path.clone())
            .expect("re-binding the same path must be a no-op");

        let other = directory.path().join("other.sqlite3");
        match runtime.bind(LogicalDatabase::VceMetadata, other.clone()) {
            Err(LocalDatabaseError::BindingConflict {
                existing,
                requested,
                ..
            }) => {
                assert_eq!(existing, path);
                assert_eq!(requested, other);
            }
            other => panic!("expected BindingConflict, got {other:?}"),
        }

        // The refused rebind must not have taken effect.
        // 被拒绝的改绑不得生效。
        assert_eq!(
            runtime
                .path_for(LogicalDatabase::VceMetadata)
                .expect("path"),
            path
        );
    }

    #[test]
    fn a_binding_conflict_is_not_an_unavailability_failure() {
        let directory = tempfile::tempdir().expect("temporary directory should exist");
        let runtime = LocalDatabaseRuntime::open(bindings_for(
            directory.path(),
            &[LogicalDatabase::AppState],
        ))
        .expect("runtime should open");

        runtime
            .bind(
                LogicalDatabase::KnowledgeBase,
                directory.path().join("kb.sqlite3"),
            )
            .expect("bind");

        let conflict = runtime
            .bind(
                LogicalDatabase::KnowledgeBase,
                directory.path().join("elsewhere.sqlite3"),
            )
            .expect_err("a second different path must be refused");

        // A binding conflict is a programming error, not a database that is unavailable, so
        // classifying it as unavailable would tell a caller to retry something that cannot succeed.
        // 绑定冲突是编程错误，而不是数据库不可用；把它归类为不可用会告诉调用方重试一件不可能成功的事。
        assert!(!crate::local_sql::is_unavailable(&conflict));
    }

    #[test]
    fn a_declared_file_binding_opens_and_reuses_one_handle() {
        let directory = tempfile::tempdir().expect("temporary directory should exist");
        let runtime = LocalDatabaseRuntime::open(bindings_for(
            directory.path(),
            &[LogicalDatabase::AppState],
        ))
        .expect("runtime should open");

        // A subsystem's own derived file, such as one knowledge library's database. It is not one of
        // the enumerated logical resources, which is exactly why the file is the binding.
        // 子系统自行派生的文件，例如某个知识库的资料库数据库。它不是枚举出的逻辑资源之一，而这正是以文件
        // 作为绑定的原因。
        let binding =
            SqliteFileBinding::new(directory.path().join("library").join("library.sqlite3"));

        let first = runtime
            .open_sqlite_file(&binding)
            .expect("first open should succeed");
        let second = runtime
            .open_sqlite_file(&binding)
            .expect("re-opening the same file with the same options must reuse the handle");
        assert!(
            std::sync::Arc::ptr_eq(&first, &second),
            "one file must yield exactly one handle, otherwise two callers would hold two locks"
        );
        assert_eq!(runtime.sqlite().database_count(), 1);

        runtime
            .connect_sqlite_file(&binding)
            .expect("connect")
            .execute_batch("CREATE TABLE probe (value INTEGER); INSERT INTO probe VALUES (11);")
            .expect("write");

        let read_back: i64 = runtime
            .connect_sqlite_file(&binding)
            .expect("reconnect")
            .query_row("SELECT value FROM probe", [], |row| row.get(0))
            .expect("read");
        assert_eq!(read_back, 11);
    }

    #[test]
    fn a_declared_file_binding_refuses_a_second_set_of_options() {
        let directory = tempfile::tempdir().expect("temporary directory should exist");
        let runtime = LocalDatabaseRuntime::open(bindings_for(
            directory.path(),
            &[LogicalDatabase::AppState],
        ))
        .expect("runtime should open");

        let binding = SqliteFileBinding::new(directory.path().join("library.sqlite3"));
        runtime.open_sqlite_file(&binding).expect("first open");

        let mut narrower = binding.clone();
        narrower.options.busy_timeout_ms = 1;
        let conflict = runtime
            .open_sqlite_file(&narrower)
            .expect_err("a second set of options for one file must be refused");

        // Silently keeping the first set would make the effective PRAGMA set depend on call order,
        // which is precisely what the engine's resource conflict prevents.
        // 静默保留第一套选项会让实际 PRAGMA 集合取决于调用顺序，而这正是引擎资源冲突所防止的。
        assert!(
            matches!(conflict, LocalDatabaseError::SqliteFile { .. }),
            "expected a file resource conflict, got {conflict:?}"
        );

        assert!(
            !crate::local_sql::is_unavailable(&conflict),
            "an options conflict is a programming error, not an unavailable database"
        );
    }

    #[test]
    fn a_shut_down_runtime_refuses_declared_file_bindings() {
        let directory = tempfile::tempdir().expect("temporary directory should exist");
        let runtime = LocalDatabaseRuntime::open(bindings_for(
            directory.path(),
            &[LogicalDatabase::AppState],
        ))
        .expect("runtime should open");

        runtime.shutdown();

        let binding = SqliteFileBinding::new(directory.path().join("library.sqlite3"));
        assert!(matches!(
            runtime.open_sqlite_file(&binding),
            Err(LocalDatabaseError::ShutDown)
        ));
        assert!(
            !binding.path.exists(),
            "a refused open must not create the file"
        );
    }

    #[tokio::test]
    async fn lance_engine_opens_one_engine_per_directory_and_reuses_it() {
        let directory = tempfile::tempdir().expect("temporary directory should exist");
        let runtime = LocalDatabaseRuntime::open(bindings_for(
            directory.path(),
            &[LogicalDatabase::AppState],
        ))
        .expect("runtime should open");

        let binding = lance_binding(directory.path());
        let first = runtime
            .lance_engine(binding.clone())
            .await
            .expect("first open should succeed");
        let second = runtime
            .lance_engine(binding.clone())
            .await
            .expect("a repeated open with the same limits should reuse the engine");

        // Reuse is the point: two engines over one directory would hold two sets of per-table locks.
        // 复用是关键：同一目录上的两个引擎会持有两套表级锁。
        assert!(
            std::sync::Arc::ptr_eq(&first.connection(), &second.connection()),
            "one directory must yield exactly one engine"
        );
        assert_eq!(runtime.lance_directories(), vec![binding.directory.clone()]);
        assert!(
            binding.directory.is_dir(),
            "opening must have created the database directory"
        );
    }

    /// Verifies concurrent first opens of one directory share the published runtime instead of failing on its lock.
    /// 验证同一目录的并发首开会共享已发布运行时，而不是在其根锁上失败。
    #[tokio::test]
    async fn concurrent_first_lance_opens_share_one_runtime() {
        const OPENERS: usize = 8;

        let directory = tempfile::tempdir().expect("temporary directory should exist");
        let runtime = Arc::new(
            LocalDatabaseRuntime::open(bindings_for(
                directory.path(),
                &[LogicalDatabase::AppState],
            ))
            .expect("runtime should open"),
        );
        let binding = lance_binding(directory.path());
        let barrier = Arc::new(tokio::sync::Barrier::new(OPENERS + 1));
        let mut tasks = Vec::with_capacity(OPENERS);

        for _ in 0..OPENERS {
            let runtime = Arc::clone(&runtime);
            let binding = binding.clone();
            let barrier = Arc::clone(&barrier);
            tasks.push(tokio::spawn(async move {
                barrier.wait().await;
                runtime.lance_engine(binding).await
            }));
        }
        barrier.wait().await;

        let mut engines = Vec::with_capacity(OPENERS);
        for task in tasks {
            engines.push(
                task.await
                    .expect("opener task should finish")
                    .expect("concurrent first open should succeed"),
            );
        }

        let first_connection = engines[0].connection();
        assert!(
            engines
                .iter()
                .all(|engine| Arc::ptr_eq(&first_connection, &engine.connection())),
            "all concurrent first opens must share one connection and table-lock set"
        );
        assert_eq!(runtime.lance_directories(), vec![binding.directory]);
    }

    /// Verifies closing one LanceDB directory removes its cache entry and releases its root lock.
    /// 验证关闭单个 LanceDB 目录会移除缓存条目并释放其根锁。
    #[tokio::test]
    async fn close_lance_directory_releases_the_exact_cached_runtime() {
        let directory = tempfile::tempdir().expect("temporary directory should exist");
        let runtime = LocalDatabaseRuntime::open(bindings_for(
            directory.path(),
            &[LogicalDatabase::AppState],
        ))
        .expect("runtime should open");
        let binding = lance_binding(directory.path());
        let engine = runtime
            .lance_engine(binding.clone())
            .await
            .expect("open LanceDB directory");

        assert!(
            runtime
                .close_lance_directory(&binding)
                .expect("close exact directory"),
            "an opened directory must report that it was closed"
        );
        assert!(runtime.lance_directories().is_empty());
        let _root_lock =
            vulcan_lancedb_engine::db_lock::DatabaseRootLock::acquire(&binding.directory)
                .expect("closing the cached runtime must release the root lock");
        // The held engine proves the close contract releases runtime ownership rather than depending on every
        // engine clone being dropped; callers are responsible for draining those clones before close.
        // 保留的引擎证明关闭契约释放的是运行时所有权，而非依赖所有引擎克隆先被丢弃；调用方负责在关闭前排空克隆。
        drop(engine);
    }

    /// Verifies lexical aliases resolve to one cached LanceDB runtime and one close identity.
    /// 验证字面路径别名解析到同一个 LanceDB 缓存运行时及同一个关闭身份。
    #[tokio::test]
    async fn lance_directory_aliases_share_one_normalized_runtime() {
        let directory = tempfile::tempdir().expect("temporary directory should exist");
        let runtime = LocalDatabaseRuntime::open(HashMap::new()).expect("runtime should open");
        let physical_binding = lance_binding(directory.path());
        let mut alias_binding = physical_binding.clone();
        alias_binding.directory = directory.path().join("unused").join("..").join("lance");

        let physical_engine = runtime
            .lance_engine(physical_binding.clone())
            .await
            .expect("physical path should open");
        let alias_engine = runtime
            .lance_engine(alias_binding.clone())
            .await
            .expect("lexical alias should reuse the open directory");

        assert!(
            Arc::ptr_eq(&physical_engine.connection(), &alias_engine.connection()),
            "lexical aliases must share one connection and one table-lock set"
        );
        assert_eq!(
            runtime.lance_directories(),
            vec![physical_binding.directory.clone()],
            "the runtime must publish only the normalized physical identity"
        );
        assert!(
            runtime
                .close_lance_directory(&alias_binding)
                .expect("alias close should resolve the same cached runtime"),
            "closing through an alias must remove the normalized runtime"
        );
        assert!(runtime.lance_directories().is_empty());
    }

    /// Verifies named bindings reject changed runtime options even when their physical paths match.
    /// 验证命名绑定即使物理路径相同，也会拒绝变化后的运行参数。
    #[test]
    fn named_bindings_compare_the_complete_contract() {
        let directory = tempfile::tempdir().expect("temporary directory should exist");
        let runtime = LocalDatabaseRuntime::open(HashMap::new()).expect("runtime should open");

        let sqlite = SqliteFileBinding::new(directory.path().join("named.sqlite3"));
        runtime
            .register_named_binding("sqlite", &sqlite)
            .expect("register SQLite binding");
        let mut changed_sqlite = sqlite.clone();
        changed_sqlite.options.busy_timeout_ms += 1;
        assert!(
            runtime
                .register_named_binding("sqlite", &changed_sqlite)
                .is_err(),
            "a changed SQLite option must not silently reuse the old binding"
        );

        let lance = lance_binding(&directory.path().join("named-lance"));
        runtime
            .register_named_lance_binding("lance", &lance)
            .expect("register LanceDB binding");
        let mut changed_lance = lance.clone();
        changed_lance.max_search_limit += 1;
        assert!(
            runtime
                .register_named_lance_binding("lance", &changed_lance)
                .is_err(),
            "a changed LanceDB limit must not silently reuse the old binding"
        );

        assert!(
            runtime
                .unregister_named_binding("sqlite", &sqlite)
                .expect("unregister SQLite binding")
        );
        assert!(runtime.named_binding("sqlite").is_err());
        assert!(
            runtime
                .unregister_named_lance_binding("lance", &lance)
                .expect("unregister LanceDB binding")
        );
        assert!(runtime.named_lance_binding("lance").is_err());
    }

    /// Verifies the mechanism that made caching the runtime necessary: an engine outlives its runtime, its lock
    /// does not.
    /// 验证使"必须缓存运行时"成立的机制：引擎比其运行时活得更久，而其锁不会。
    ///
    /// The engine is a clone of the runtime's cached entry, so it keeps its connection and per-table locks while
    /// the runtime's root lock is released with the runtime. A store caching only the engine therefore released the
    /// directory it was still serving, and a second writer could appear with no error anywhere. This test fails if
    /// the lock ever moves into the engine, at which point the store's cache would need revisiting.
    /// 引擎是运行时缓存条目的克隆，因此它保留连接与表级锁，而运行时的根锁随运行时一同释放。只缓存引擎的存储因此
    /// 释放了它仍在服务的目录，第二个写者可以在毫无报错的情况下出现。若锁将来移入引擎，本测试会失败，届时存储的
    /// 缓存需要重新审视。
    #[tokio::test]
    async fn an_engine_outlives_its_runtime_but_its_root_lock_does_not() {
        let directory = tempfile::tempdir().expect("temporary directory should exist");
        let binding = lance_binding(directory.path());

        let engine = {
            let runtime = vulcan_lancedb_engine::runtime::LanceDbRuntime::new(
                binding.runtime_config(),
                binding.engine_options(),
            );
            runtime
                .open_default_engine()
                .await
                .expect("the engine should open")
        };

        // The engine stays alive to the end of this test, so the acquisition below proves that a live engine does
        // not itself keep the directory claimed.
        // 引擎存活到本测试结束，因此下方的获取证明了"存活的引擎本身并不占用该目录的独占声明"。
        let _engine = engine;
        vulcan_lancedb_engine::db_lock::DatabaseRootLock::acquire(&binding.directory).expect(
            "a dropped runtime must have released the root lock, which is why the store caches the runtime",
        );
    }
    /// Verifies the store keeps the database-root lock for as long as it serves a directory.
    /// 验证存储在服务某目录期间持续持有库根锁。
    ///
    /// The runtime that owns that lock is the engine's producer, so caching only the engine would release the lock
    /// while the store still hands that engine out — and another process could then open the same directory as a
    /// second writer with no error anywhere. A second acquisition inside this process is refused exactly when the
    /// store still holds the lock, which is what makes this a discriminator rather than a tautology.
    /// 拥有该锁的运行时正是引擎的产生者，因此只缓存引擎会在存储仍分发该引擎时释放锁——另一个进程随后就能以第二个
    /// 写者身份打开同一目录，且任何地方都不会报错。本进程内的第二次获取恰在存储仍持有锁时被拒绝，这正是本测试成为
    /// 判别条件而非同义反复的原因。
    #[tokio::test]
    async fn lance_engine_keeps_the_database_root_lock_for_the_directory() {
        let directory = tempfile::tempdir().expect("temporary directory should exist");
        let runtime = LocalDatabaseRuntime::open(bindings_for(
            directory.path(),
            &[LogicalDatabase::AppState],
        ))
        .expect("runtime should open");

        let binding = lance_binding(directory.path());
        runtime
            .lance_engine(binding.clone())
            .await
            .expect("open should succeed");

        match vulcan_lancedb_engine::db_lock::DatabaseRootLock::acquire(&binding.directory) {
            Err(vulcan_lancedb_engine::error::LanceDbEngineError::LockHeld { .. }) => {}
            Ok(_) => panic!(
                "the store released the database-root lock while it still serves the directory"
            ),
            Err(other) => panic!("expected LockHeld, got {other}"),
        }
    }
    #[tokio::test]
    async fn lance_engine_refuses_different_limits_for_one_directory() {
        let directory = tempfile::tempdir().expect("temporary directory should exist");
        let runtime = LocalDatabaseRuntime::open(bindings_for(
            directory.path(),
            &[LogicalDatabase::AppState],
        ))
        .expect("runtime should open");

        let binding = lance_binding(directory.path());
        runtime
            .lance_engine(binding.clone())
            .await
            .expect("first open");

        let mut narrower = binding.clone();
        narrower.max_concurrent_requests = 1;
        // `LanceDbEngine` carries a connection and is deliberately not `Debug`, so the failure is
        // taken with `err()` rather than `expect_err`.
        // `LanceDbEngine` 持有连接且刻意不实现 `Debug`，因此用 `err()` 而非 `expect_err` 取出失败。
        let conflict = runtime
            .lance_engine(narrower.clone())
            .await
            .err()
            .expect("a second set of limits for one directory must be refused");

        // The conflict must name both declarations, because the caller's fix depends on knowing
        // which side declared what.
        // 冲突必须指明两边的声明，因为调用方的修复取决于知道哪一侧声明了什么。
        match conflict {
            LocalDatabaseError::LanceDbLimitsConflict {
                directory: conflicted,
                existing,
                requested,
            } => {
                assert_eq!(conflicted, binding.directory);
                assert_eq!(*existing, binding);
                assert_eq!(*requested, narrower);
            }
            other => panic!("expected LanceDbLimitsConflict, got {other:?}"),
        }

        assert!(
            !crate::local_sql::is_unavailable(&LocalDatabaseError::LanceDbLimitsConflict {
                directory: binding.directory.clone(),
                existing: Box::new(binding.clone()),
                requested: Box::new(narrower),
            }),
            "conflicting limits are a programming error, not an unavailable database"
        );
    }

    #[tokio::test]
    async fn shutdown_closes_lance_engines() {
        let directory = tempfile::tempdir().expect("temporary directory should exist");
        let runtime = LocalDatabaseRuntime::open(bindings_for(
            directory.path(),
            &[LogicalDatabase::AppState],
        ))
        .expect("runtime should open");

        runtime
            .lance_engine(lance_binding(directory.path()))
            .await
            .expect("open");
        assert_eq!(runtime.lance_directories().len(), 1);

        runtime.shutdown();

        assert!(
            runtime.lance_directories().is_empty(),
            "shutdown must release every LanceDB engine"
        );
        assert!(
            runtime
                .lance_engine(lance_binding(directory.path()))
                .await
                .is_err(),
            "a shut-down runtime must refuse new LanceDB opens"
        );
    }

    /// Verifies a completed shutdown forbids a slow first opener from publishing a new LanceDB runtime.
    /// 验证关闭完成后，缓慢的首次打开者不能重新发布新的 LanceDB 运行时。
    #[test]
    fn shutdown_refuses_late_lance_runtime_publication() {
        let directory = tempfile::tempdir().expect("temporary directory should exist");
        let runtime = LocalDatabaseRuntime::open(HashMap::new()).expect("runtime should open");
        let binding = lance_binding(directory.path());
        let candidate = Arc::new(vulcan_lancedb_engine::runtime::LanceDbRuntime::new(
            binding.runtime_config(),
            binding.engine_options(),
        ));

        runtime.shutdown();

        assert!(matches!(
            runtime.publish_lance_runtime(binding, &candidate),
            Err(LocalDatabaseError::ShutDown)
        ));
        assert!(runtime.lance_directories().is_empty());
    }

    /// Verifies shutdown closes dynamically named SQLite files and refuses every reopening entry point.
    /// 验证关闭会释放动态命名 SQLite 文件，并拒绝每个重新打开入口。
    #[test]
    fn shutdown_closes_named_sqlite_files_and_refuses_reopen() {
        let directory = tempfile::tempdir().expect("temporary directory should exist");
        let runtime = LocalDatabaseRuntime::open(bindings_for(
            directory.path(),
            &[LogicalDatabase::AppState],
        ))
        .expect("runtime should open");
        let named = SqliteFileBinding::new(directory.path().join("skill.sqlite3"));
        let named_handle = runtime
            .open_sqlite_file(&named)
            .expect("named database should open");
        drop(named_handle);
        assert_eq!(runtime.sqlite().database_count(), 1);

        runtime.shutdown();

        assert_eq!(runtime.sqlite().database_count(), 0);
        assert!(matches!(
            runtime.open_sqlite_file(&named),
            Err(LocalDatabaseError::ShutDown)
        ));
        assert!(matches!(
            runtime.handle_for(LogicalDatabase::AppState),
            Err(LocalDatabaseError::ShutDown)
        ));
        assert!(matches!(
            runtime.health_report(),
            Err(LocalDatabaseError::ShutDown)
        ));
        assert!(matches!(
            runtime.app_schema_version(),
            Err(LocalDatabaseError::ShutDown)
        ));

        let replacement = vulcan_sqlite_engine::runtime::SqliteRuntime::new();
        replacement
            .open_database(&named.path)
            .expect("named database lock must be released");
    }

    #[test]
    fn shutdown_stops_accepting_connections_and_releases_locks() {
        let (_directory, runtime) = runtime_with_all_resources();
        let app_path = runtime
            .path_for(LogicalDatabase::AppState)
            .expect("app state should be bound")
            .to_path_buf();

        // Opening is lazy, so the handle and its exclusive lock only exist after a real use. Without
        // this step the assertions below would pass vacuously.
        // 打开是惰性的，因此句柄与其独占锁只有在真正使用之后才存在。缺少这一步，下方断言会空洞通过。
        runtime
            .connect(LogicalDatabase::AppState)
            .expect("first connect should open the database and take its lock");
        assert!(
            runtime.sqlite().get_database(&app_path).is_some(),
            "using a bound database must register its handle"
        );

        assert!(!runtime.is_shut_down());

        runtime.shutdown();

        assert!(runtime.is_shut_down());
        assert!(
            matches!(
                runtime.connect(LogicalDatabase::AppState),
                Err(LocalDatabaseError::ShutDown)
            ),
            "a shut-down runtime must refuse new connections"
        );

        // The exclusive lock must be gone, so the file is openable again by another runtime.
        // 独占锁必须已释放，使该文件能被另一个运行时再次打开。
        assert!(
            runtime.sqlite().get_database(&app_path).is_none(),
            "shutdown must unregister the database handle"
        );
    }

    #[test]
    fn a_second_runtime_can_open_after_the_first_shuts_down() {
        let directory = tempfile::tempdir().expect("temporary directory should exist");
        let bindings = bindings_for(directory.path(), &[LogicalDatabase::AppState]);

        let first =
            LocalDatabaseRuntime::open(bindings.clone()).expect("first runtime should open");

        // Hold the exclusive lock for real before shutting down, otherwise the second open below
        // would succeed simply because the first never took it.
        // 在关闭之前真正持有独占锁，否则下方第二次打开会因为第一个从未取锁而理所当然地成功。
        first
            .health_report()
            .expect("first health check should open and lock the database");

        // A second runtime must not be able to take the same lock while the first holds it. This is
        // what makes the post-shutdown success below meaningful.
        // 在第一个持有锁期间，第二个运行时不得取得同一把锁。这正是下方关闭后成功之所以有意义的原因。
        let contended =
            LocalDatabaseRuntime::open(bindings.clone()).expect("second runtime should construct");
        assert!(
            matches!(
                contended.connect(LogicalDatabase::AppState),
                Err(LocalDatabaseError::Sqlite { .. })
            ),
            "a held lock must block another runtime from opening the same database"
        );
        drop(contended);

        first.shutdown();
        drop(first);

        // Holding the exclusive lock is what would make this fail, so a successful second open is
        // the evidence that shutdown really released it.
        // 若独占锁未释放，此处会失败，因此第二次成功打开正是关闭确实释放了锁的证据。
        let second = LocalDatabaseRuntime::open(bindings)
            .expect("second runtime must open after the first releases its lock");
        assert!(second.health_report().expect("health").is_healthy());
    }

    #[test]
    fn runtime_is_shareable_across_clones() {
        let (_directory, runtime) = runtime_with_all_resources();
        let clone = runtime.clone();

        // Both clones must reach one engine, so a write through one is visible through the other.
        // 两个克隆必须到达同一个引擎，使经其一写入的数据对另一个可见。
        clone
            .connect(LogicalDatabase::AppState)
            .expect("clone connect")
            .execute_batch("CREATE TABLE shared (value INTEGER); INSERT INTO shared VALUES (1);")
            .expect("write through clone");

        let observed: i64 = runtime
            .connect(LogicalDatabase::AppState)
            .expect("original connect")
            .query_row("SELECT COUNT(*) FROM shared", [], |row| row.get(0))
            .expect("read through original");

        assert_eq!(observed, 1);
    }
}

#[cfg(test)]
mod blocking_boundary_tests {
    //! Proofs that the boundary does what the plan requires of it.
    //! 证明该边界确实做到了方案对它的要求。

    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    /// A cancelled preparation retains capacity while unrelated asynchronous work remains responsive.
    /// 准备调用取消后仍保留容量，同时无关异步工作保持响应。
    #[tokio::test]
    async fn preparation_cancellation_keeps_capacity_until_real_completion() {
        // One slot makes premature release observable without saturating the host thread pool.
        // 单槽位使提前释放可被观察，无需占满宿主线程池。
        let boundary = DatabaseBlockingBoundary::new(1);
        let worker_boundary = boundary.clone();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let task = tokio::spawn(async move {
            worker_boundary
                .prepare_until(None, move || {
                    started_tx.send(()).expect("announce blocking preparation");
                    release_rx
                        .recv_timeout(Duration::from_secs(5))
                        .map_err(|error| error.to_string())
                })
                .await
        });
        started_rx
            .await
            .expect("preparation started off the executor");
        task.abort();
        assert!(task.await.expect_err("cancel waiter").is_cancelled());
        let deadline = Instant::now() + Duration::from_millis(30);
        let queued = boundary
            .prepare_until(Some(deadline), || Ok::<_, String>(()))
            .await;
        release_tx.send(()).expect("release actual worker");
        assert!(
            matches!(queued, Err(BlockingBoundaryError::DeadlineExceeded)),
            "the cancelled caller must not release active preparation capacity"
        );
        boundary
            .prepare_until(None, || Ok::<_, String>(()))
            .await
            .expect("capacity recovered");
    }

    /// Expired preparation never performs filesystem work even when capacity is immediately free.
    /// 即使容量立即可用，已到期的准备也不得执行文件操作。
    #[tokio::test]
    async fn expired_preparation_never_runs_the_operation() {
        let boundary = DatabaseBlockingBoundary::new(1);
        let result = boundary
            .prepare_until(Some(Instant::now()), || -> Result<(), String> {
                panic!("expired preparation must not execute");
            })
            .await;
        assert!(matches!(
            result,
            Err(BlockingBoundaryError::DeadlineExceeded)
        ));
    }

    /// Proves blocking database work leaves the async executor free rather than stalling it.
    /// 证明阻塞数据库工作让异步执行器保持空闲，而不是把它卡住。
    ///
    /// The observation is the *first* tick's latency. A single-threaded runtime polls the two futures in
    /// order, so if the boundary ran its closure inline the ticker could not start until the closure
    /// finished, and its first tick would land after the whole blocking window. Ticking early is therefore
    /// evidence that the work really moved to another thread.
    /// 观测对象是**首次**滴答的延迟。单线程运行时会按顺序轮询两个 future，因此若边界内联运行其闭包，滴答器
    /// 只能等闭包结束才开始，其首次滴答会落在整个阻塞窗口之后。因此较早的滴答就是工作确实转移到其他线程的证据。
    #[tokio::test(flavor = "current_thread")]
    async fn blocking_database_work_does_not_stall_the_async_executor() {
        let boundary = DatabaseBlockingBoundary::new(2);
        let started = Instant::now();
        let first_tick = Arc::new(std::sync::Mutex::new(None));
        let recorder = Arc::clone(&first_tick);
        let (result, ()) = tokio::join!(
            boundary.run::<(), String, _>(
                DatabaseKey::Logical(LogicalDatabase::AppState),
                move || {
                    std::thread::sleep(Duration::from_millis(200));
                    Ok(())
                }
            ),
            async move {
                let deadline = Instant::now() + Duration::from_millis(120);
                let mut first = true;
                while Instant::now() < deadline {
                    if first {
                        *recorder.lock().expect("record first tick") = Some(started.elapsed());
                        first = false;
                    }
                    tokio::task::yield_now().await;
                }
            }
        );
        assert!(result.is_ok(), "the gated operation should succeed");
        let observed = first_tick
            .lock()
            .expect("read first tick")
            .expect("the ticker should have run");
        assert!(
            observed < Duration::from_millis(150),
            "the async executor stayed busy for {observed:?} while a blocking database operation was in \
             flight, which means the work ran on a core thread"
        );
    }

    /// Proves operations on one database serialize even though the boundary admits several at once.
    /// 证明同一数据库上的操作即使边界同时允许多个也会串行化。
    ///
    /// The elapsed assertion is one-sided on purpose: contention can only make the sequence slower, so it
    /// cannot produce a false pass.
    /// 耗时断言刻意是单侧的：竞争只会让序列更慢，因此不会产生假通过。
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn operations_on_one_database_serialize() {
        const TASKS: usize = 4;
        const HOLD: Duration = Duration::from_millis(60);
        let boundary = DatabaseBlockingBoundary::new(TASKS);
        let in_flight = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let started = Instant::now();
        let mut handles = Vec::new();
        for _ in 0..TASKS {
            let boundary = boundary.clone();
            let in_flight = Arc::clone(&in_flight);
            let peak = Arc::clone(&peak);
            handles.push(tokio::spawn(async move {
                boundary
                    .run::<(), String, _>(
                        DatabaseKey::Logical(LogicalDatabase::AppState),
                        move || {
                            let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                            peak.fetch_max(now, Ordering::SeqCst);
                            std::thread::sleep(HOLD);
                            in_flight.fetch_sub(1, Ordering::SeqCst);
                            Ok(())
                        },
                    )
                    .await
            }));
        }
        for handle in handles {
            handle
                .await
                .expect("join the gated operation")
                .expect("the gated operation should succeed");
        }
        assert_eq!(
            peak.load(Ordering::SeqCst),
            1,
            "two operations on one database were in flight together"
        );
        assert!(
            started.elapsed() >= HOLD * (TASKS as u32),
            "the sequence finished faster than its own serialized duration, so it did not serialize"
        );
    }

    /// Proves a waiter on one busy database does not consume the permit needed by another database.
    /// 证明一个繁忙数据库上的等待者不会占用另一个数据库所需的许可。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn same_database_waiter_does_not_starve_an_unrelated_database() {
        let boundary = DatabaseBlockingBoundary::new(1);
        let busy_key = DatabaseKey::File(PathBuf::from("busy.sqlite3"));
        let held_gate = boundary.gate_for(&busy_key).await.lock_owned().await;

        let queued_boundary = boundary.clone();
        let queued = tokio::spawn(async move {
            queued_boundary
                .run::<(), String, _>(busy_key, || Ok(()))
                .await
        });
        for _ in 0..16 {
            tokio::task::yield_now().await;
        }

        tokio::time::timeout(
            Duration::from_secs(1),
            boundary.run::<(), String, _>(
                DatabaseKey::File(PathBuf::from("unrelated.sqlite3")),
                || Ok(()),
            ),
        )
        .await
        .expect("an unrelated database must not be starved")
        .expect("the unrelated operation should succeed");

        drop(held_gate);
        queued
            .await
            .expect("join queued operation")
            .expect("the queued operation should succeed");
        assert_eq!(boundary.max_concurrent(), 1);
    }

    /// Proves one database's gate does not become a global one: different files really do run together.
    /// 证明某个数据库的门没有变成全局门：不同文件确实会同时运行。
    ///
    /// The rendezvous is what makes this deterministic. Each operation announces itself and then waits for
    /// the other, so completing inside the timeout is only possible if both were in flight at once.
    /// 会合机制使本测试具备确定性。每个操作先宣告自身再等待对方，因此只有在二者同时在途时才可能在超时内完成。
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn operations_on_different_databases_run_at_the_same_time() {
        let boundary = DatabaseBlockingBoundary::new(4);
        let (first_tx, first_rx) = mpsc::channel::<()>();
        let (second_tx, second_rx) = mpsc::channel::<()>();
        let started = Instant::now();
        let first = boundary.clone();
        let second = boundary.clone();
        let first_task = tokio::spawn(async move {
            first
                .run::<(), String, _>(
                    DatabaseKey::File(PathBuf::from("first.sqlite3")),
                    move || {
                        let _ = first_tx.send(());
                        let _ = second_rx.recv_timeout(Duration::from_secs(2));
                        Ok(())
                    },
                )
                .await
        });
        let second_task = tokio::spawn(async move {
            second
                .run::<(), String, _>(
                    DatabaseKey::File(PathBuf::from("second.sqlite3")),
                    move || {
                        let _ = second_tx.send(());
                        let _ = first_rx.recv_timeout(Duration::from_secs(2));
                        Ok(())
                    },
                )
                .await
        });
        first_task
            .await
            .expect("join first")
            .expect("first operation");
        second_task
            .await
            .expect("join second")
            .expect("second operation");
        assert!(
            started.elapsed() < Duration::from_millis(1_500),
            "the two databases did not run at the same time, so the gate is coarser than one database"
        );
    }
}
