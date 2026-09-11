//! SQLite runtime: open options, connection pragmas, handle reuse and path normalization.
//! SQLite 运行时：打开选项、连接 PRAGMA、句柄复用与路径规范化。
//!
//! Responsibilities / 职责:
//!
//! - Own one handle set per normalized database path, so the same path is never opened twice.
//! - Apply the v1 PRAGMA and security option set at the single open entry point.
//! - Normalize paths and refuse URI forms, so callers cannot smuggle a non-local target in.
//!
//! 负责：按规范化路径持有唯一句柄集合，使同一路径不会被重复打开；在唯一打开入口应用 v1 PRAGMA
//! 与安全选项；规范化路径并拒绝 URI 形式，防止调用方夹带非本地目标。
//!
//! Boundary note / 边界说明:
//!
//! This module decides *how* a database is opened, never *where* it lives. It receives an
//! already-authorized concrete path from the host registry and accepts no "space root plus
//! relative path" style input. Read-only mode is part of the opening policy, not a permission
//! grant.
//!
//! 本模块决定数据库**如何**被打开，绝不决定它**位于何处**。它从宿主注册表接收已经授权的具体
// 路径，不接受“空间根目录 + 相对路径”式输入。只读模式属于打开策略，而不是权限授予。

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use rusqlite::{Connection, OpenFlags};

use crate::db_lock::DatabaseFileLock;
use crate::error::{EngineResult, SqliteEngineError};

/// SQLite connection-level PRAGMA configuration.
/// SQLite 连接层 PRAGMA 配置。
///
/// These defaults are the v1 baseline values carried over from the migrated source. They are
/// recorded in the v1 baseline ledger; changing them after release becomes a compatibility
/// constraint.
///
/// 以下默认值是从迁入源码继承的 v1 基线取值，记录在 v1 基线台账中；发布后修改它们会成为兼容约束。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlitePragmaOptions {
    /// Requested journal mode.
    /// 期望的 journal 模式。
    pub journal_mode: String,
    /// Requested synchronous mode.
    /// 期望的 synchronous 模式。
    pub synchronous: String,
    /// Whether foreign keys are enforced.
    /// 是否强制外键。
    pub foreign_keys: bool,
    /// Temporary storage mode.
    /// 临时存储模式。
    pub temp_store: String,
    /// WAL auto-checkpoint page count.
    /// WAL 自动 checkpoint 页数。
    pub wal_autocheckpoint_pages: u32,
    /// SQLite page cache size in KiB.
    /// SQLite 页缓存大小（KiB）。
    pub cache_size_kib: i64,
    /// Memory-mapped I/O size in bytes.
    /// 内存映射 I/O 大小（字节）。
    pub mmap_size_bytes: u64,
}

/// SQLite security hardening options.
/// SQLite 安全硬化选项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqliteHardeningOptions {
    /// Whether a database file ownership lock must be enforced.
    /// 是否强制数据库文件所有权锁。
    pub enforce_db_file_lock: bool,
    /// Whether the database is opened read-only.
    /// 是否以只读方式打开数据库。
    pub read_only: bool,
    /// Whether the `trusted_schema` pragma is enabled.
    /// 是否启用 `trusted_schema` pragma。
    pub trusted_schema: bool,
    /// Whether SQLite defensive mode is enabled.
    /// 是否启用 SQLite defensive 模式。
    pub defensive: bool,
}

/// Programmatic open options for one SQLite database.
/// 单个 SQLite 数据库的程序化打开选项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqliteOpenOptions {
    /// `busy_timeout` in milliseconds. This exists only for external and brief contention; it
    /// must never be used to mask the host's own concurrency errors.
    /// `busy_timeout`（毫秒）。它只用于外部占用与短暂竞争，绝不能用来掩盖宿主自身的并发错误。
    pub busy_timeout_ms: u64,
    /// SQLite PRAGMA options.
    /// SQLite PRAGMA 选项。
    pub pragmas: SqlitePragmaOptions,
    /// SQLite hardening options.
    /// SQLite 安全硬化选项。
    pub hardening: SqliteHardeningOptions,
}

impl Default for SqlitePragmaOptions {
    /// Returns the v1 baseline PRAGMA defaults.
    /// 返回 v1 基线 PRAGMA 默认值。
    fn default() -> Self {
        Self {
            journal_mode: "WAL".to_string(),
            synchronous: "NORMAL".to_string(),
            foreign_keys: true,
            temp_store: "MEMORY".to_string(),
            wal_autocheckpoint_pages: 1_000,
            cache_size_kib: 65_536,
            mmap_size_bytes: 268_435_456,
        }
    }
}

impl Default for SqliteHardeningOptions {
    /// Returns the v1 baseline hardening defaults: locking on, writable, restricted schema and
    /// defensive mode on. URI filenames are not part of the public contract and are always refused.
    /// 返回 v1 基线硬化默认值：启用锁、可写、限制 schema 并启用 defensive 模式。URI 文件名不属于
    /// 公共契约，并始终被拒绝。
    fn default() -> Self {
        Self {
            enforce_db_file_lock: true,
            read_only: false,
            trusted_schema: false,
            defensive: true,
        }
    }
}

impl Default for SqliteOpenOptions {
    /// Returns the v1 baseline open options.
    /// 返回 v1 基线打开选项。
    fn default() -> Self {
        Self {
            busy_timeout_ms: 5_000,
            pragmas: SqlitePragmaOptions::default(),
            hardening: SqliteHardeningOptions::default(),
        }
    }
}

/// Multi-database runtime owning one handle per normalized database path.
/// 多数据库运行时：每个规范化数据库路径持有一个句柄。
///
/// The registry mutex guards only map lookup and insertion. Database I/O must never run while it
/// is held.
///
/// 注册表互斥量只保护映射查找与插入；持有它时绝不允许执行数据库 I/O。
#[derive(Debug)]
pub struct SqliteRuntime {
    /// Options applied when a caller does not supply explicit options.
    /// 调用方未显式提供选项时应用的选项。
    default_options: SqliteOpenOptions,
    /// Registered handles keyed by normalized database path.
    /// 以规范化数据库路径为键的已注册句柄。
    databases: Mutex<HashMap<PathBuf, Arc<SqliteDatabaseHandle>>>,
}

/// A single database handle managed by a [`SqliteRuntime`].
/// 由 [`SqliteRuntime`] 管理的单库句柄。
#[derive(Debug)]
pub struct SqliteDatabaseHandle {
    /// Normalized absolute database path.
    /// 规范化后的绝对数据库路径。
    db_path: PathBuf,
    /// Options this handle was opened with.
    /// 打开本句柄时使用的选项。
    options: SqliteOpenOptions,
    /// Held ownership lock, absent for in-memory databases or when locking is disabled.
    /// 持有的所有权锁；内存数据库或禁用锁时为空。
    file_lock: Option<DatabaseFileLock>,
}

impl Default for SqliteRuntime {
    /// Creates a runtime with default options.
    /// 使用默认选项创建运行时。
    fn default() -> Self {
        Self::new()
    }
}

impl SqliteRuntime {
    /// Creates a multi-database runtime with default options.
    /// 使用默认选项创建多数据库运行时。
    pub fn new() -> Self {
        Self::with_default_options(SqliteOpenOptions::default())
    }

    /// Creates a multi-database runtime with caller-provided default options.
    /// 使用调用方提供的默认选项创建多数据库运行时。
    pub fn with_default_options(default_options: SqliteOpenOptions) -> Self {
        Self {
            default_options,
            databases: Mutex::new(HashMap::new()),
        }
    }

    /// Returns the default open options.
    /// 返回默认打开选项。
    pub fn default_options(&self) -> &SqliteOpenOptions {
        &self.default_options
    }

    /// Opens or reuses the handle for `db_path` using the runtime default options.
    /// 使用运行时默认选项打开或复用 `db_path` 的句柄。
    pub fn open_database(
        &self,
        db_path: impl AsRef<Path>,
    ) -> EngineResult<Arc<SqliteDatabaseHandle>> {
        self.open_database_with_options(db_path, self.default_options.clone())
    }

    /// Opens or reuses the handle for `db_path` using explicit options.
    /// 使用显式选项打开或复用 `db_path` 的句柄。
    ///
    /// A path that is already registered is returned as-is. Requesting the same path with
    /// different options is a resource conflict rather than a silent reuse, because silently
    /// ignoring the caller's options would make the effective PRAGMA set depend on call order.
    ///
    /// 已注册的路径会原样返回。以不同选项请求同一路径属于资源冲突，而不是静默复用——静默忽略
    /// 调用方选项会让实际 PRAGMA 集合取决于调用顺序。
    pub fn open_database_with_options(
        &self,
        db_path: impl AsRef<Path>,
        options: SqliteOpenOptions,
    ) -> EngineResult<Arc<SqliteDatabaseHandle>> {
        validated_pragma_scalars(&options)?;
        let normalized = normalize_db_path(db_path.as_ref())?;

        let mut guard = lock_unpoisoned(&self.databases);

        if let Some(existing) = guard.get(&normalized) {
            if !existing.options_match(&options) {
                return Err(SqliteEngineError::ResourceConflict {
                    path: normalized,
                    detail: "database path is already open with different options".to_string(),
                });
            }
            return Ok(Arc::clone(existing));
        }

        let handle = Arc::new(SqliteDatabaseHandle::open(normalized.clone(), options)?);
        guard.insert(normalized, Arc::clone(&handle));
        Ok(handle)
    }

    /// Returns an already-registered handle for `db_path`, if any.
    /// 返回 `db_path` 已注册的句柄（若有）。
    pub fn get_database(&self, db_path: impl AsRef<Path>) -> Option<Arc<SqliteDatabaseHandle>> {
        let normalized = normalize_db_path(db_path.as_ref()).ok()?;
        lock_unpoisoned(&self.databases).get(&normalized).cloned()
    }

    /// Closes and removes the handle for `db_path`.
    /// 关闭并移除 `db_path` 的句柄。
    ///
    /// Returns whether a handle was registered. Dropping the returned entry releases the file
    /// ownership lock.
    ///
    /// 返回此前是否注册过句柄。移除该条目即释放文件所有权锁。
    pub fn close_database(&self, db_path: impl AsRef<Path>) -> bool {
        let Ok(normalized) = normalize_db_path(db_path.as_ref()) else {
            return false;
        };
        lock_unpoisoned(&self.databases)
            .remove(&normalized)
            .is_some()
    }

    /// Closes every registered database handle and returns how many registrations were removed.
    /// 关闭全部已注册数据库句柄，并返回移除的注册数量。
    ///
    /// This is the runtime-wide lifecycle boundary used by an embedding host. It includes dynamically
    /// registered skill, plugin, VCE and knowledge-base databases, not only the host's fixed logical files.
    /// 这是嵌入宿主使用的运行时级生命周期边界。它覆盖动态注册的技能、插件、VCE 与知识库数据库，而不只
    /// 包含宿主的固定逻辑文件。
    pub fn close_all_databases(&self) -> usize {
        let mut databases = lock_unpoisoned(&self.databases);
        let closed = databases.len();
        databases.clear();
        closed
    }

    /// Lists normalized database paths currently registered.
    /// 列出当前已注册的规范化数据库路径。
    pub fn list_databases(&self) -> Vec<PathBuf> {
        lock_unpoisoned(&self.databases).keys().cloned().collect()
    }

    /// Returns how many databases are currently registered.
    /// 返回当前已注册的数据库数量。
    pub fn database_count(&self) -> usize {
        lock_unpoisoned(&self.databases).len()
    }
}

impl SqliteDatabaseHandle {
    /// Opens one database handle, creating the parent directory and acquiring the ownership lock.
    /// 打开单个数据库句柄：创建父目录并获取所有权锁。
    fn open(db_path: PathBuf, options: SqliteOpenOptions) -> EngineResult<Self> {
        let special = is_special_db_path(&db_path);

        if !special
            && let Some(parent) = db_path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|source| SqliteEngineError::Open {
                path: db_path.clone(),
                source: Box::new(source),
            })?;
        }

        let file_lock = if options.hardening.enforce_db_file_lock && !special {
            Some(DatabaseFileLock::acquire(&db_path)?)
        } else {
            None
        };

        Ok(Self {
            db_path,
            options,
            file_lock,
        })
    }

    /// Returns the normalized database path.
    /// 返回规范化数据库路径。
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// Returns the options this handle was opened with.
    /// 返回打开本句柄时使用的选项。
    pub fn options(&self) -> &SqliteOpenOptions {
        &self.options
    }

    /// Returns the held lock file path, if this handle holds a lock.
    /// 返回本句柄持有的锁文件路径（若持有）。
    pub fn lock_path(&self) -> Option<&Path> {
        self.file_lock.as_ref().map(DatabaseFileLock::lock_path)
    }

    /// Returns the lock owner record path, if this handle holds a lock.
    /// 返回锁持有者记录路径（若持有锁）。
    pub fn lock_owner_path(&self) -> Option<&Path> {
        self.file_lock.as_ref().map(DatabaseFileLock::owner_path)
    }

    /// Returns the recorded lock owner identity, if this handle holds a lock.
    /// 返回记录的锁持有者身份（若持有锁）。
    pub fn lock_owner(&self) -> Option<&str> {
        self.file_lock.as_ref().map(DatabaseFileLock::owner)
    }

    /// Compares the options that affect the effective connection configuration.
    /// 比较会影响实际连接配置的选项。
    fn options_match(&self, other: &SqliteOpenOptions) -> bool {
        self.options.hardening.read_only == other.hardening.read_only
            && self.options.hardening.trusted_schema == other.hardening.trusted_schema
            && self.options.hardening.defensive == other.hardening.defensive
            && self.options.hardening.enforce_db_file_lock == other.hardening.enforce_db_file_lock
            && self.options.busy_timeout_ms == other.busy_timeout_ms
            && self.options.pragmas.journal_mode == other.pragmas.journal_mode
            && self.options.pragmas.synchronous == other.pragmas.synchronous
            && self.options.pragmas.foreign_keys == other.pragmas.foreign_keys
            && self.options.pragmas.temp_store == other.pragmas.temp_store
            && self.options.pragmas.wal_autocheckpoint_pages
                == other.pragmas.wal_autocheckpoint_pages
            && self.options.pragmas.cache_size_kib == other.pragmas.cache_size_kib
            && self.options.pragmas.mmap_size_bytes == other.pragmas.mmap_size_bytes
    }

    /// Opens one connection initialized with this handle's runtime rules.
    /// 打开一个按本句柄运行时规则初始化的连接。
    ///
    /// The connection is independent of the handle's lifetime and the caller owns it.
    ///
    /// 连接独立于句柄生命周期，由调用方持有。
    pub fn open_connection(&self) -> EngineResult<Connection> {
        open_sqlite_connection(&self.db_path, &self.options)
    }
}

/// Reports whether a path denotes a special (non-file) SQLite database.
/// 判断路径是否表示特殊（非文件）SQLite 数据库。
pub fn is_special_db_path(value: &Path) -> bool {
    value == Path::new(IN_MEMORY_DB_PATH)
}

/// Opens a SQLite connection from a path and options, applying pragmas and tokenizer setup.
/// 按路径与选项打开 SQLite 连接，并应用 PRAGMA 与分词器初始化。
pub fn open_sqlite_connection(
    db_path: &Path,
    options: &SqliteOpenOptions,
) -> EngineResult<Connection> {
    validated_pragma_scalars(options)?;
    let db_path = normalize_db_path(db_path)?;
    let flags = build_sqlite_open_flags(options);
    let conn = if is_special_db_path(&db_path) {
        Connection::open_in_memory_with_flags(flags)
    } else {
        Connection::open_with_flags(&db_path, flags)
    }
    .map_err(|source| SqliteEngineError::Open {
        path: db_path.clone(),
        source: Box::new(source),
    })?;

    apply_sqlite_connection_pragmas(&conn, &db_path, options)?;
    Ok(conn)
}

/// Applies runtime-level pragmas and tokenizer initialization to an opened connection.
/// 对已打开连接应用运行时级 PRAGMA 与分词器初始化。
///
/// Write-affecting pragmas are skipped in read-only mode, and `query_only` is set instead so a
/// read-only handle cannot be talked into writing.
///
/// 只读模式下跳过影响写入的 PRAGMA，改为设置 `query_only`，使只读句柄无法被诱导写入。
pub fn apply_sqlite_connection_pragmas(
    conn: &Connection,
    db_path: &Path,
    options: &SqliteOpenOptions,
) -> EngineResult<()> {
    let (busy_timeout_ms, cache_size_pragma, mmap_size_bytes) = validated_pragma_scalars(options)?;
    let mut effective_journal_mode = None;

    if !options.hardening.read_only {
        effective_journal_mode = Some(
            conn.pragma_update_and_check(
                None,
                "journal_mode",
                options.pragmas.journal_mode.as_str(),
                |row| row.get::<_, String>(0),
            )
            .map_err(SqliteEngineError::sql)?,
        );
        conn.pragma_update(None, "synchronous", options.pragmas.synchronous.as_str())
            .map_err(SqliteEngineError::sql)?;
        conn.pragma_update(
            None,
            "wal_autocheckpoint",
            options.pragmas.wal_autocheckpoint_pages,
        )
        .map_err(SqliteEngineError::sql)?;
    }

    conn.pragma_update(None, "busy_timeout", busy_timeout_ms)
        .map_err(SqliteEngineError::sql)?;
    conn.pragma_update(None, "foreign_keys", options.pragmas.foreign_keys)
        .map_err(SqliteEngineError::sql)?;
    conn.pragma_update(None, "temp_store", options.pragmas.temp_store.as_str())
        .map_err(SqliteEngineError::sql)?;
    conn.pragma_update(None, "trusted_schema", options.hardening.trusted_schema)
        .map_err(SqliteEngineError::sql)?;
    conn.pragma_update(None, "defensive", options.hardening.defensive)
        .map_err(SqliteEngineError::sql)?;
    conn.pragma_update(None, "cache_size", cache_size_pragma)
        .map_err(SqliteEngineError::sql)?;
    conn.pragma_update(None, "mmap_size", mmap_size_bytes)
        .map_err(SqliteEngineError::sql)?;

    if options.hardening.read_only {
        conn.pragma_update(None, "query_only", true)
            .map_err(SqliteEngineError::sql)?;
    }

    ensure_requested_wal_mode(conn, db_path, options, effective_journal_mode.as_deref())?;

    // The Jieba tokenizer is registered here because this is the single permitted registration
    // point. FTS5 tokenizer registration is per connection, so doing it at open time guarantees
    // every connection that can create an FTS5 table has the tokenizer available.
    // Jieba 分词器在此注册，因为这里是唯一允许的注册点。FTS5 分词器注册是按连接的，
    // 因此在打开时注册可保证每个能创建 FTS5 表的连接都已具备该分词器。
    if options.hardening.read_only {
        crate::tokenizer::ensure_jieba_tokenizer_registered_read_only(conn)?;
    } else {
        crate::tokenizer::ensure_jieba_tokenizer_registered(conn)?;
    }

    Ok(())
}

/// Validates and converts numeric PRAGMA options without truncation or semantic reinterpretation.
/// 校验并转换数值型 PRAGMA 选项，不做截断或语义重解释。
fn validated_pragma_scalars(options: &SqliteOpenOptions) -> EngineResult<(i64, i64, i64)> {
    let busy_timeout_ms = i64::try_from(options.busy_timeout_ms).map_err(|_| {
        SqliteEngineError::invalid_argument("busy_timeout_ms exceeds SQLite's signed range")
    })?;
    if options.pragmas.cache_size_kib <= 0 {
        return Err(SqliteEngineError::invalid_argument(
            "cache_size_kib must be greater than zero",
        ));
    }
    let cache_size_pragma = options
        .pragmas
        .cache_size_kib
        .checked_neg()
        .ok_or_else(|| {
            SqliteEngineError::invalid_argument("cache_size_kib exceeds SQLite's signed range")
        })?;
    let mmap_size_bytes = i64::try_from(options.pragmas.mmap_size_bytes).map_err(|_| {
        SqliteEngineError::invalid_argument("mmap_size_bytes exceeds SQLite's signed range")
    })?;
    Ok((busy_timeout_ms, cache_size_pragma, mmap_size_bytes))
}

/// Verifies that WAL was actually granted when it was requested for a file database.
/// 当文件数据库请求 WAL 时，验证 WAL 是否确实生效。
///
/// A silently non-WAL file database changes durability and locking behaviour, so this is a hard
/// failure rather than a warning.
///
/// 文件数据库静默未启用 WAL 会改变持久性与加锁行为，因此这里按硬失败处理而不是告警。
fn ensure_requested_wal_mode(
    conn: &Connection,
    db_path: &Path,
    options: &SqliteOpenOptions,
    effective_journal_mode: Option<&str>,
) -> EngineResult<()> {
    if !options.pragmas.journal_mode.eq_ignore_ascii_case("WAL") || is_special_db_path(db_path) {
        return Ok(());
    }

    let effective_mode = match effective_journal_mode {
        Some(mode) => mode.trim().to_ascii_uppercase(),
        None => conn
            .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
            .map_err(SqliteEngineError::sql)?
            .trim()
            .to_ascii_uppercase(),
    };

    if effective_mode == "WAL" {
        return Ok(());
    }

    Err(SqliteEngineError::Sql {
        source: rusqlite::Error::InvalidParameterName(format!(
            "journal_mode=WAL was requested for file database {} but SQLite reported journal_mode={effective_mode}",
            db_path.display()
        )),
    })
}

/// Builds SQLite open flags from runtime open options.
/// 由运行时打开选项构造 SQLite open flags。
pub fn build_sqlite_open_flags(options: &SqliteOpenOptions) -> OpenFlags {
    let mut flags = OpenFlags::SQLITE_OPEN_NO_MUTEX;
    if options.hardening.read_only {
        flags |= OpenFlags::SQLITE_OPEN_READ_ONLY;
    } else {
        flags |= OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE;
    }
    flags
}

/// Normalizes one local database path and always refuses SQLite URI filenames.
/// 规范化一个本地数据库路径，并始终拒绝 SQLite URI 文件名。
///
/// Normalization resolves relative paths against the process working directory and then removes
/// `.` and `..` components lexically, so the same logical target cannot register twice under
/// different spellings.
///
/// 规范化会以进程工作目录解析相对路径，再按字面移除 `.` 与 `..` 段，使同一逻辑目标不会以不同
/// 写法注册两次。
pub fn normalize_db_path(raw: &Path) -> EngineResult<PathBuf> {
    if raw.as_os_str().is_empty() || raw.to_str().is_some_and(|value| value.trim().is_empty()) {
        return Err(SqliteEngineError::PathNotAuthorized {
            detail: "database path must not be empty".to_string(),
        });
    }

    if raw == Path::new(IN_MEMORY_DB_PATH) {
        return Ok(PathBuf::from(IN_MEMORY_DB_PATH));
    }

    if raw.to_str().is_some_and(looks_like_sqlite_uri) {
        return Err(SqliteEngineError::PathNotAuthorized {
            detail: format!(
                "SQLite URI filenames are not part of the local engine contract; refusing '{}'",
                raw.display()
            ),
        });
    }

    let absolute = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|source| SqliteEngineError::PathNotAuthorized {
                detail: format!(
                    "cannot resolve relative database path against the working directory: {source}"
                ),
            })?
            .join(raw)
    };

    Ok(remove_lexical_dot_segments(&absolute))
}

/// Removes `.` and `..` components lexically without touching the filesystem.
/// 按字面移除 `.` 与 `..` 段，不访问文件系统。
///
/// This is deliberately lexical: canonicalization would resolve symlinks and require the target
/// to exist, neither of which is desirable while registering a database that is about to be
/// created. The caller passes an absolute path, so a parent segment above the filesystem root is
/// clamped there by the platform's path-component semantics.
///
/// 这里刻意只做字面处理：规范化会解析符号链接并要求目标存在，而注册一个即将创建的数据库时二者
/// 都不可取。调用方传入绝对路径，因此越过文件系统根目录的父级段会按平台路径组件语义钳制在根目录。
fn remove_lexical_dot_segments(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
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
    normalized
}

/// Reports whether a raw string looks like a SQLite URI filename.
/// 判断原始字符串是否形如 SQLite URI 文件名。
fn looks_like_sqlite_uri(value: &str) -> bool {
    value.starts_with("file:")
}

/// The in-memory database path literal.
/// 内存数据库路径字面量。
const IN_MEMORY_DB_PATH: &str = ":memory:";

/// Locks a mutex, recovering the guard if a previous holder panicked.
/// 锁定互斥量；若先前持有者 panic 则恢复该守卫。
///
/// Recovering rather than propagating is deliberate: the guarded value is a plain map with no
/// cross-entry invariant, so a poisoned mutex carries no risk of observing a torn map.
///
/// 选择恢复而不是传播是刻意的：被保护的值是没有跨条目不变量的普通映射，因此互斥量中毒不会带来
/// 观测到破损映射的风险。
fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::{
        SqliteHardeningOptions, SqliteOpenOptions, SqliteRuntime, build_sqlite_open_flags,
        is_special_db_path, normalize_db_path, open_sqlite_connection,
    };
    use crate::error::SqliteEngineError;
    use std::path::{Path, PathBuf};

    /// Verifies the v1 baseline PRAGMA defaults are stable.
    /// 验证 v1 基线 PRAGMA 默认值稳定。
    #[test]
    fn default_pragma_values_are_stable() {
        let options = SqliteOpenOptions::default();
        assert_eq!(options.pragmas.journal_mode, "WAL");
        assert_eq!(options.pragmas.synchronous, "NORMAL");
        assert!(options.pragmas.foreign_keys);
        assert_eq!(options.pragmas.temp_store, "MEMORY");
        assert_eq!(options.busy_timeout_ms, 5_000);
    }

    /// Verifies invalid numeric PRAGMA options are rejected instead of panicking or clamping.
    /// 验证非法数值型 PRAGMA 选项会被拒绝，而不是触发 panic 或静默钳制。
    #[test]
    fn invalid_numeric_pragma_options_are_rejected() {
        let invalid_cache = SqliteOpenOptions {
            pragmas: super::SqlitePragmaOptions {
                cache_size_kib: i64::MIN,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(matches!(
            open_sqlite_connection(Path::new(":memory:"), &invalid_cache),
            Err(SqliteEngineError::InvalidArgument { .. })
        ));

        let invalid_timeout = SqliteOpenOptions {
            busy_timeout_ms: u64::MAX,
            ..Default::default()
        };
        assert!(matches!(
            open_sqlite_connection(Path::new(":memory:"), &invalid_timeout),
            Err(SqliteEngineError::InvalidArgument { .. })
        ));

        let invalid_mmap = SqliteOpenOptions {
            pragmas: super::SqlitePragmaOptions {
                mmap_size_bytes: u64::MAX,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(matches!(
            open_sqlite_connection(Path::new(":memory:"), &invalid_mmap),
            Err(SqliteEngineError::InvalidArgument { .. })
        ));
    }

    /// Verifies the v1 baseline hardening defaults are secure by default.
    /// 验证 v1 基线硬化默认值默认安全。
    #[test]
    fn default_hardening_options_are_secure() {
        let hardening = SqliteHardeningOptions::default();
        assert!(hardening.enforce_db_file_lock);
        assert!(!hardening.read_only);
        assert!(!hardening.trusted_schema);
        assert!(hardening.defensive);
    }

    /// Verifies read-only mode sets the read-only open flag and omits create.
    /// 验证只读模式设置只读 flag 且不含创建。
    #[test]
    fn read_only_flags_exclude_create() {
        let options = SqliteOpenOptions {
            hardening: super::SqliteHardeningOptions {
                read_only: true,
                ..Default::default()
            },
            ..SqliteOpenOptions::default()
        };

        let flags = build_sqlite_open_flags(&options);
        assert!(flags.contains(rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY));
        assert!(!flags.contains(rusqlite::OpenFlags::SQLITE_OPEN_CREATE));
        assert!(!flags.contains(rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE));
    }

    /// Verifies URI filenames are always refused by the local-only engine.
    /// 验证仅限本地的引擎始终拒绝 URI 文件名。
    #[test]
    fn uri_filenames_are_always_refused() {
        let uri = Path::new("file:memdb1?mode=memory&cache=shared");

        match normalize_db_path(uri) {
            Err(SqliteEngineError::PathNotAuthorized { detail }) => {
                assert!(detail.contains("URI"), "unexpected detail: {detail}");
            }
            other => panic!("expected PathNotAuthorized, got {other:?}"),
        }

        assert!(matches!(
            open_sqlite_connection(uri, &SqliteOpenOptions::default()),
            Err(SqliteEngineError::PathNotAuthorized { .. })
        ));
    }

    /// Verifies an empty path is refused.
    /// 验证空路径被拒绝。
    #[test]
    fn empty_path_is_refused() {
        assert!(matches!(
            normalize_db_path(Path::new("   ")),
            Err(SqliteEngineError::PathNotAuthorized { .. })
        ));
    }

    /// Verifies normalization does not rewrite significant whitespace in a filesystem path.
    /// 验证规范化不会改写文件系统路径中有意义的首尾空格。
    #[test]
    fn normalization_preserves_significant_surrounding_whitespace() {
        let raw = Path::new(" database.db ");
        let expected = std::env::current_dir()
            .expect("resolve working directory")
            .join(raw);

        assert_eq!(normalize_db_path(raw).expect("normalize"), expected);
    }

    /// Verifies a path resembling the memory literal keeps its distinct filesystem identity.
    /// 验证形似内存字面量的路径仍保持独立的文件系统身份。
    #[test]
    fn spaced_memory_literal_is_not_treated_as_in_memory() {
        assert!(!is_special_db_path(Path::new(" :memory:")));
        assert!(!is_special_db_path(Path::new(":memory: ")));
    }

    /// Verifies the special in-memory path is preserved verbatim.
    /// 验证特殊内存路径原样保留。
    #[test]
    fn in_memory_path_is_preserved() {
        let normalized = normalize_db_path(Path::new(":memory:")).expect("normalize");
        assert_eq!(normalized, PathBuf::from(":memory:"));
        assert!(is_special_db_path(&normalized));
    }

    /// Verifies lexical normalization removes `.` and `..` segments.
    ///
    /// The fixture is built from a relative path so the assertion is platform-neutral: on Windows
    /// a leading `/` is resolved against the current drive, which would make a Unix-style literal
    /// compare against the wrong value.
    ///
    /// 验证字面规范化会移除 `.` 与 `..` 段。
    ///
    /// 夹具用相对路径构造，使断言与平台无关：在 Windows 上前导 `/` 会相对当前驱动器解析，
    /// 直接使用 Unix 风格字面量会比对到错误的值。
    #[test]
    fn normalization_removes_lexical_dot_segments() {
        let raw = Path::new("srv")
            .join("vldb")
            .join(".")
            .join("nested")
            .join("..")
            .join("app.db");
        let expected = std::env::current_dir()
            .expect("resolve working directory")
            .join("srv")
            .join("vldb")
            .join("app.db");

        let normalized = normalize_db_path(&raw).expect("normalize");
        assert_eq!(normalized, expected);
    }

    /// Verifies normalization makes the same logical target normalize identically, so a database
    /// cannot be registered twice under different spellings.
    /// 验证同一逻辑目标规范化结果一致，使数据库不会以不同写法注册两次。
    #[test]
    fn equivalent_spellings_normalize_identically() {
        let first = normalize_db_path(Path::new("srv/vldb/app.db")).expect("first");
        let second = normalize_db_path(Path::new("srv/vldb/./nested/../app.db")).expect("second");
        assert_eq!(first, second);
    }

    /// Verifies parent segments cannot escape above an absolute filesystem root in the cache key.
    /// 验证父级路径段不能在缓存键中越过绝对文件系统根目录。
    #[test]
    fn normalization_clamps_parent_segments_at_the_filesystem_root() {
        let root = std::env::current_dir()
            .expect("resolve working directory")
            .ancestors()
            .last()
            .expect("an absolute working directory has a root")
            .to_path_buf();
        let aliased = root.join("..").join("app.db");

        assert_eq!(
            normalize_db_path(&aliased).expect("normalize root alias"),
            root.join("app.db")
        );
    }

    /// Verifies opening the same path twice reuses one handle.
    /// 验证同一路径打开两次复用一个句柄。
    #[test]
    fn same_path_reuses_one_handle() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let runtime = SqliteRuntime::new();
        let db_path = temp.path().join("app.db");

        let first = runtime.open_database(&db_path).expect("first open");
        let second = runtime.open_database(&db_path).expect("second open");

        assert!(std::sync::Arc::ptr_eq(&first, &second));
        assert_eq!(runtime.database_count(), 1);
    }

    /// Verifies equivalent spellings reuse one handle after normalization.
    /// 验证等价写法在规范化后复用一个句柄。
    #[test]
    fn equivalent_spellings_reuse_one_handle() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let runtime = SqliteRuntime::new();
        let direct = temp.path().join("app.db");
        let indirect = temp.path().join("nested").join("..").join("app.db");

        let first = runtime.open_database(&direct).expect("direct open");
        let second = runtime.open_database(&indirect).expect("indirect open");

        assert!(std::sync::Arc::ptr_eq(&first, &second));
        assert_eq!(runtime.database_count(), 1);
    }

    /// Verifies conflicting options for one path are refused rather than silently ignored.
    /// 验证同一路径的冲突选项被拒绝而不是静默忽略。
    #[test]
    fn conflicting_options_are_refused() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let runtime = SqliteRuntime::new();
        let db_path = temp.path().join("app.db");

        let _first = runtime.open_database(&db_path).expect("first open");

        let conflicting = SqliteOpenOptions {
            busy_timeout_ms: 12_345,
            ..SqliteOpenOptions::default()
        };

        match runtime.open_database_with_options(&db_path, conflicting) {
            Err(SqliteEngineError::ResourceConflict { path, .. }) => {
                assert_eq!(
                    path.file_name().and_then(|name| name.to_str()),
                    Some("app.db")
                );
            }
            other => panic!("expected ResourceConflict, got {other:?}"),
        }
    }

    /// Verifies the handle holds the database ownership lock.
    /// 验证句柄持有数据库所有权锁。
    #[test]
    fn handle_holds_ownership_lock() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let runtime = SqliteRuntime::new();
        let db_path = temp.path().join("app.db");

        let handle = runtime.open_database(&db_path).expect("open");

        let lock_path = handle.lock_path().expect("lock path present");
        assert!(lock_path.exists());
        assert!(handle.lock_owner().expect("owner present").contains("pid="));
    }

    /// Verifies closing a database releases its ownership lock.
    /// 验证关闭数据库释放其所有权锁。
    #[test]
    fn close_releases_ownership_lock() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let runtime = SqliteRuntime::new();
        let db_path = temp.path().join("app.db");

        let handle = runtime.open_database(&db_path).expect("open");
        let lock_path = handle.lock_path().expect("lock path").to_path_buf();
        drop(handle);

        assert!(runtime.close_database(&db_path));
        assert_eq!(runtime.database_count(), 0);

        // The lock is free again, so a fresh acquisition succeeds.
        // 锁已释放，因此新的获取会成功。
        let reacquired = runtime.open_database(&db_path).expect("reopen after close");
        assert_eq!(reacquired.lock_path(), Some(lock_path.as_path()));
    }

    /// Verifies runtime-wide closure releases every dynamically opened database rather than a selected subset.
    /// 验证运行时级关闭会释放每个动态打开的数据库，而不是只释放选定子集。
    #[test]
    fn close_all_releases_every_registered_database() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let runtime = SqliteRuntime::new();
        let first_path = temp.path().join("first.db");
        let second_path = temp.path().join("second.db");

        let first = runtime.open_database(&first_path).expect("open first");
        let second = runtime.open_database(&second_path).expect("open second");
        drop((first, second));

        assert_eq!(runtime.close_all_databases(), 2);
        assert_eq!(runtime.database_count(), 0);

        let replacement = SqliteRuntime::new();
        replacement
            .open_database(&first_path)
            .expect("first ownership lock must be released");
        replacement
            .open_database(&second_path)
            .expect("second ownership lock must be released");
    }

    /// Verifies in-memory databases take no ownership lock.
    /// 验证内存数据库不获取所有权锁。
    #[test]
    fn in_memory_database_takes_no_lock() {
        let runtime = SqliteRuntime::new();
        let handle = runtime
            .open_database(Path::new(":memory:"))
            .expect("open in-memory");
        assert!(handle.lock_path().is_none());
    }

    /// Verifies an opened connection carries the requested PRAGMA state.
    /// 验证打开的连接带有请求的 PRAGMA 状态。
    #[test]
    fn opened_connection_applies_requested_pragmas() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let db_path = temp.path().join("app.db");
        let options = SqliteOpenOptions::default();

        let conn = open_sqlite_connection(&db_path, &options).expect("open connection");

        let journal_mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("read journal_mode");
        assert_eq!(journal_mode.to_ascii_uppercase(), "WAL");

        let foreign_keys: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .expect("read foreign_keys");
        assert_eq!(foreign_keys, 1);

        let busy_timeout: i64 = conn
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
            .expect("read busy_timeout");
        assert_eq!(busy_timeout, 5_000);
    }

    /// Verifies a read-only connection reports `query_only` and cannot write.
    /// 验证只读连接报告 `query_only` 且无法写入。
    #[test]
    fn read_only_connection_enables_query_only() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let db_path = temp.path().join("app.db");

        // Create the database first so the read-only open has a target.
        // 先创建数据库，使只读打开有目标。
        {
            let writable = SqliteOpenOptions::default();
            let conn = open_sqlite_connection(&db_path, &writable).expect("create database");
            conn.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY);")
                .expect("create table");
        }

        let read_only = SqliteOpenOptions {
            hardening: super::SqliteHardeningOptions {
                read_only: true,
                ..Default::default()
            },
            ..SqliteOpenOptions::default()
        };
        // A read-only open must not require the ownership lock to be re-entrant with the writer
        // used above, which has already been dropped.
        // 只读打开不应要求与上方已被丢弃的写者重入同一所有权锁。
        let conn = open_sqlite_connection(&db_path, &read_only).expect("open read-only");

        let query_only: i64 = conn
            .query_row("PRAGMA query_only", [], |row| row.get(0))
            .expect("read query_only");
        assert_eq!(query_only, 1);

        let write_result = conn.execute_batch("CREATE TABLE should_fail (id INTEGER);");
        assert!(
            write_result.is_err(),
            "read-only connection must refuse writes"
        );
    }

    /// Verifies read-only opening does not require or create the companion dictionary table.
    /// 验证只读打开不要求也不创建伴生词典表。
    #[test]
    fn read_only_connection_opens_database_without_dictionary_table() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let db_path = temp.path().join("plain.db");
        {
            let plain = rusqlite::Connection::open(&db_path).expect("create plain database");
            plain
                .execute_batch("CREATE TABLE records (id INTEGER PRIMARY KEY);")
                .expect("create ordinary schema");
        }

        let mut read_only = SqliteOpenOptions {
            hardening: super::SqliteHardeningOptions {
                read_only: true,
                ..Default::default()
            },
            ..SqliteOpenOptions::default()
        };
        read_only.pragmas.journal_mode = String::from("DELETE");
        let connection =
            open_sqlite_connection(&db_path, &read_only).expect("open plain database read-only");

        let dictionary_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = '_vulcan_dict'",
                [],
                |row| row.get(0),
            )
            .expect("inspect schema");
        assert_eq!(dictionary_count, 0, "read-only open must not mutate schema");
    }

    /// Verifies opening a database in a missing directory creates it.
    /// 验证在缺失目录中打开数据库会创建该目录。
    #[test]
    fn open_creates_missing_parent_directory() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let runtime = SqliteRuntime::new();
        let db_path = temp.path().join("a").join("b").join("app.db");

        let handle = runtime
            .open_database(&db_path)
            .expect("open in missing directory");
        assert!(handle.db_path().parent().expect("parent").exists());
    }
}
