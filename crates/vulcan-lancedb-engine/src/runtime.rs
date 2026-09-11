//! Embedded runtime that owns one database manager and one engine per physical database.
//! 嵌入式运行时：持有一个数据库管理器，并为每个物理数据库持有一个引擎。
//!
//! The runtime is the layer a host embeds. It resolves a database name to a physical key, reuses
//! one [`LanceDbEngine`] per key, and exposes the shared [`DatabaseManager`] so a caller can reuse
//! path resolution without opening a connection.
//!
//! 该运行时是宿主嵌入的层。它把数据库名解析为物理键，按键复用同一个 [`LanceDbEngine`]，并暴露共享的
//! [`DatabaseManager`]，使调用方可复用路径解析而不建立连接。
//!
//! Why the engine cache is keyed separately from the connection cache / 为何引擎缓存与连接缓存分开键控:
//!
//! The manager caches by **normalized database name** (so `None`, `""` and `"  "` share the
//! `default` entry), while the runtime caches by **resolved physical path**. Two spellings that
//! map to one connection therefore also map to one engine, and the engine's per-table
//! synchronization state is never silently split across two instances of the same database.
//!
//! 管理器按**规范化数据库名**缓存（因此 `None`、`""` 与 `"  "` 共用 `default` 条目），而运行时按
//! **解析后的物理路径**缓存。因此映射到同一连接的两种拼写也映射到同一个引擎，引擎的表级同步状态
//! 绝不会被静默拆分到同一数据库的两个实例上。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

use crate::db_lock::DatabaseRootLock;
use crate::engine::{LanceDbEngine, LanceDbEngineOptions};
use crate::error::{EngineResult, LanceDbEngineError};
use crate::manager::{DatabaseManager, DatabaseRuntimeConfig};

/// Result of acquiring one database-root lock.
/// 获取一个库根锁的结果。
enum RootLockOutcome {
    /// The lock was acquired by this call.
    /// 本次调用获取到了锁。
    Acquired(Arc<DatabaseRootLock>),
    /// Another in-process opener published an engine for this root while this call waited.
    /// 本次调用等待期间，另一个进程内打开者为该根发布了引擎。
    AlreadyPublished(LanceDbEngine),
}

/// One cached engine together with the database-root lock that keeps this process its sole writer.
/// 一个已缓存的引擎，连同使本进程成为其唯一写者的库根锁。
///
/// The two are one value on purpose: the baseline requires in-process consumers of one root to share one lock
/// and one engine, and a cache that could hold an engine without its lock would be able to violate that.
/// 二者刻意合成一个值：基线要求同一根的进程内消费者共享同一把锁与同一个引擎，而一个可能持有引擎却无其锁的缓存
/// 就能违反这一点。
#[derive(Clone)]
struct CachedDatabase {
    /// Shared engine.
    /// 共享引擎。
    engine: LanceDbEngine,
    /// Root lock held for as long as this entry exists.
    /// 在本条目存在期间持有的根锁。
    _root_lock: Arc<DatabaseRootLock>,
}

/// How long a concurrent in-process open waits for the first opener to publish its engine.
/// 并发的进程内打开等待首个打开者发布其引擎的时长。
const IN_PROCESS_OPEN_WAIT: Duration = Duration::from_secs(2);

/// Step between cache re-checks while waiting for a concurrent opener.
/// 等待并发打开者期间重新检查缓存的间隔。
const IN_PROCESS_OPEN_POLL: Duration = Duration::from_millis(10);

/// Embedded LanceDB runtime managing database instances and engine construction.
/// 嵌入式 LanceDB 运行时，负责管理数据库实例与引擎构造。
#[derive(Clone)]
pub struct LanceDbRuntime {
    /// Shared database manager.
    /// 共享数据库管理器。
    manager: DatabaseManager,
    /// Engine options applied to every engine this runtime constructs.
    /// 施加于本运行时构造的每个引擎的引擎选项。
    engine_options: LanceDbEngineOptions,
    /// Engines keyed by resolved physical database path, each with its held root lock.
    /// 按解析后的物理数据库路径键控的引擎缓存，每项都携带其持有的根锁。
    engines: Arc<Mutex<HashMap<PathBuf, CachedDatabase>>>,
}

impl LanceDbRuntime {
    /// Creates the embedded runtime from database configuration and engine options.
    /// 基于数据库配置与引擎选项创建嵌入式运行时。
    pub fn new(config: DatabaseRuntimeConfig, engine_options: LanceDbEngineOptions) -> Self {
        Self {
            manager: DatabaseManager::new(config),
            engine_options,
            engines: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Returns the shared database manager.
    /// 返回共享数据库管理器。
    pub fn manager(&self) -> &DatabaseManager {
        &self.manager
    }

    /// Returns the engine options used by this runtime.
    /// 返回本运行时使用的引擎选项。
    pub fn engine_options(&self) -> &LanceDbEngineOptions {
        &self.engine_options
    }

    /// Opens the default database and its engine.
    /// 打开默认数据库及其引擎。
    pub async fn open_default_engine(&self) -> EngineResult<LanceDbEngine> {
        self.open_named_engine(None).await
    }

    /// Opens a named database and its engine; `None` means the default database.
    /// 按名称打开数据库及其引擎；`None` 表示默认数据库。
    ///
    /// As in the manager, the cache is checked under a short lock but the connection and engine are
    /// built outside it, so a slow open cannot block unrelated databases. Two concurrent opens of
    /// one path may both construct an engine; the `entry` insert keeps the first and the duplicate
    /// is dropped, which matters because a discarded engine would otherwise hold a second set of
    /// per-table locks for the same database.
    ///
    /// 与管理器一致，缓存在短暂持锁下检查，而连接与引擎在锁外构建，使缓慢的打开不会阻塞无关数据库。
    /// 同一路径的两次并发打开可能都构造出引擎；`entry` 插入保留第一个、丢弃重复者——这很重要，因为被
    /// 丢弃的引擎否则会为同一数据库持有一套额外的表级锁。
    pub async fn open_named_engine(
        &self,
        database_name: Option<&str>,
    ) -> EngineResult<LanceDbEngine> {
        let database_key = self.manager.database_path_for_name(database_name)?;
        if let Some(engine) = self.cached_engine(&database_key).await {
            return Ok(engine);
        }

        // The root lock is taken under the cache lock so exactly one lock exists per physical path, and the
        // cache lock is released again before the slow connection open so unrelated databases are not blocked.
        // 根锁在缓存锁之下获取，使每个物理路径恰好存在一把锁；随后在缓慢的连接建立之前释放缓存锁，使无关数据库
        // 不被阻塞。
        let root_lock = match self.acquire_root_lock(&database_key).await? {
            RootLockOutcome::Acquired(lock) => lock,
            RootLockOutcome::AlreadyPublished(engine) => return Ok(engine),
        };

        let connection = self.manager.open_named(database_name).await?;
        let engine = LanceDbEngine::new(connection, self.engine_options.clone());

        let mut engines = self.engines.lock().await;
        if let Some(existing) = engines.get(&database_key) {
            // Another task published an engine for this path while this one opened. Keep the first entry and
            // release this lock by dropping it, so the path still has exactly one lock and one engine.
            // 本任务打开期间另一任务为该路径发布了引擎。保留首条目并丢弃本锁以释放它，使该路径仍恰好一把锁、
            // 一个引擎。
            return Ok(existing.engine.clone());
        }
        engines.insert(
            database_key,
            CachedDatabase {
                engine: engine.clone(),
                _root_lock: root_lock,
            },
        );
        Ok(engine)
    }

    /// Returns the cached engine for one resolved physical path, if one exists.
    /// 返回某个已解析物理路径的已缓存引擎（若存在）。
    ///
    /// # Parameters / 参数
    /// `database_key` is the resolved physical path used as the cache key.
    /// `database_key` 是用作缓存键的已解析物理路径。
    ///
    /// # Returns / 返回值
    /// Returns the engine, or `None` when this runtime has not published one yet.
    /// 返回引擎；本运行时尚未发布时返回 `None`。
    async fn cached_engine(&self, database_key: &Path) -> Option<LanceDbEngine> {
        let engines = self.engines.lock().await;
        engines.get(database_key).map(|entry| entry.engine.clone())
    }

    /// Acquires the database-root lock for one resolved physical path.
    /// 为某个已解析物理路径获取库根锁。
    ///
    /// # Parameters / 参数
    /// `database_key` is the resolved physical path used as the cache key.
    /// `database_key` 是用作缓存键的已解析物理路径。
    ///
    /// # Returns / 返回值
    /// Returns the acquired lock, or the engine another in-process opener published while this call waited.
    /// Contention from another **process** is returned as [`LanceDbEngineError::LockHeld`] once the bounded wait
    /// expires, which is what keeps a second process from becoming a second writer.
    /// 返回已获取的锁，或本调用等待期间另一个进程内打开者发布的引擎。来自另一**进程**的竞争在有界等待结束后以
    /// [`LanceDbEngineError::LockHeld`] 返回，这正是阻止第二个进程成为第二个写者的机制。
    async fn acquire_root_lock(&self, database_key: &Path) -> EngineResult<RootLockOutcome> {
        match DatabaseRootLock::acquire(database_key) {
            Ok(lock) => Ok(RootLockOutcome::Acquired(Arc::new(lock))),
            Err(LanceDbEngineError::LockHeld { lock_path, owner }) => {
                // This process already holds the lock, so a concurrent opener is publishing an engine for the
                // same root. The baseline requires in-process consumers to share it rather than to fail.
                // 本进程已持有该锁，因此并发的打开者正在为同一个根发布引擎。基线要求进程内消费者共享它而不是失败。
                let deadline = IN_PROCESS_OPEN_WAIT;
                let mut waited = Duration::ZERO;
                while waited < deadline {
                    tokio::time::sleep(IN_PROCESS_OPEN_POLL).await;
                    waited += IN_PROCESS_OPEN_POLL;
                    if let Some(engine) = self.cached_engine(database_key).await {
                        return Ok(RootLockOutcome::AlreadyPublished(engine));
                    }
                }
                Err(LanceDbEngineError::LockHeld { lock_path, owner })
            }
            Err(other) => Err(other),
        }
    }

    /// Resolves the physical path for a database name without opening a connection.
    /// 解析某个数据库名的物理路径，不建立连接。
    pub fn database_path_for_name(&self, database_name: Option<&str>) -> EngineResult<PathBuf> {
        self.manager.database_path_for_name(database_name)
    }

    /// Returns the cached database keys, for diagnostics.
    /// 返回已缓存的数据库键，用于诊断。
    pub async fn cached_database_keys(&self) -> Vec<String> {
        self.manager.cached_database_keys().await
    }

    /// Returns the number of engines currently cached by this runtime.
    /// 返回本运行时当前缓存的引擎数量。
    pub async fn cached_engine_count(&self) -> usize {
        self.engines.lock().await.len()
    }

    /// Returns the number of database-root locks this runtime currently holds.
    /// 返回本运行时当前持有的库根锁数量。
    ///
    /// It exists so a test can assert that one physical path yields one lock, which is the baseline's
    /// requirement that in-process consumers share a single lock.
    /// 它存在的目的是让测试能断言一个物理路径只产生一把锁，即基线对进程内消费者共享唯一锁的要求。
    pub async fn held_root_lock_count(&self) -> usize {
        self.engines.lock().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::LanceDbRuntime;
    use crate::engine::LanceDbEngineOptions;
    use crate::manager::DatabaseRuntimeConfig;
    use std::sync::Arc;

    /// Builds a configuration rooted in a temporary directory.
    /// 构造一个以临时目录为根的配置。
    ///
    /// The root is temporary rather than an absolute literal so the test does not depend on a path
    /// that only exists on one platform; a literal such as `/srv/...` would resolve against the
    /// current drive on Windows and make the assertion environment-dependent.
    ///
    /// 根使用临时目录而非绝对字面量，使测试不依赖只在某个平台上存在的路径；诸如 `/srv/...` 的字面量
    /// 在 Windows 上会相对当前盘符解析，使断言依赖环境。
    fn sample_runtime() -> (tempfile::TempDir, LanceDbRuntime) {
        let directory = tempfile::tempdir().expect("temporary directory should exist");
        let root = directory.path().join("databases");

        let runtime = LanceDbRuntime::new(
            DatabaseRuntimeConfig {
                default_db_path: directory.path().join("default"),
                db_root: Some(root),
                read_consistency_interval_ms: Some(250),
            },
            LanceDbEngineOptions {
                max_upsert_payload: 1024,
                max_search_limit: 64,
                max_concurrent_requests: 8,
            },
        );

        (directory, runtime)
    }

    #[test]
    fn runtime_exposes_engine_options() {
        let (_directory, runtime) = sample_runtime();
        assert_eq!(runtime.engine_options().max_upsert_payload, 1024);
        assert_eq!(runtime.engine_options().max_search_limit, 64);
        assert_eq!(runtime.engine_options().max_concurrent_requests, 8);
    }

    #[test]
    fn runtime_resolves_named_database_path_via_manager() {
        let (_directory, runtime) = sample_runtime();

        let resolved = runtime
            .database_path_for_name(Some("memory"))
            .expect("named database path should resolve");

        assert!(
            resolved.ends_with("memory"),
            "resolved path must end with the database name: {}",
            resolved.display()
        );
        assert_eq!(
            resolved,
            runtime
                .manager()
                .config()
                .db_root
                .as_ref()
                .expect("sample runtime configures a root")
                .join("memory")
        );
    }

    /// Verifies repeated opens of one physical database reuse a single engine cache entry.
    /// 验证重复打开同一物理数据库会复用唯一的引擎缓存条目。
    #[tokio::test]
    async fn runtime_reuses_engine_for_the_same_database_path() {
        let (_directory, runtime) = tempdir_runtime();

        let first = runtime
            .open_default_engine()
            .await
            .expect("first engine open should succeed");
        // A blank name falls back to the default database, so it must resolve to the same path and
        // therefore the same engine.
        // 空名称回落为默认数据库，因此必须解析到同一路径，从而得到同一个引擎。
        let second = runtime
            .open_named_engine(Some(" "))
            .await
            .expect("blank database name should reuse the default engine");

        assert!(Arc::ptr_eq(&first.connection(), &second.connection()));
        assert_eq!(
            runtime.cached_engine_count().await,
            1,
            "one physical database must yield exactly one cached engine"
        );
    }

    /// Builds a runtime whose default database lives in a temporary directory.
    /// 构造一个默认数据库位于临时目录的运行时。
    fn tempdir_runtime() -> (tempfile::TempDir, LanceDbRuntime) {
        let directory = tempfile::tempdir().expect("temporary runtime directory should exist");
        let database_path = directory.path().join("default");
        let runtime = LanceDbRuntime::new(
            DatabaseRuntimeConfig {
                default_db_path: database_path,
                db_root: None,
                read_consistency_interval_ms: None,
            },
            LanceDbEngineOptions::default(),
        );

        (directory, runtime)
    }

    /// Verifies two distinct named databases receive two distinct engines.
    /// 验证两个不同的命名数据库得到两个不同的引擎。
    #[tokio::test]
    async fn distinct_databases_receive_distinct_engines() {
        let (_directory, runtime) = sample_runtime();

        let first = runtime
            .open_named_engine(Some("alpha"))
            .await
            .expect("alpha should open");
        let second = runtime
            .open_named_engine(Some("beta"))
            .await
            .expect("beta should open");

        assert!(!Arc::ptr_eq(&first.connection(), &second.connection()));
        assert_eq!(runtime.cached_engine_count().await, 2);
    }

    /// Verifies cached database keys are sorted and reflect opened databases.
    /// 验证已缓存数据库键有序且反映已打开的数据库。
    #[tokio::test]
    async fn cached_database_keys_are_sorted() {
        let (_directory, runtime) = sample_runtime();

        runtime
            .open_named_engine(Some("zulu"))
            .await
            .expect("zulu should open");
        runtime
            .open_named_engine(Some("alpha"))
            .await
            .expect("alpha should open");

        assert_eq!(
            runtime.cached_database_keys().await,
            vec!["named:alpha".to_string(), "named:zulu".to_string()]
        );
    }

    /// Verifies the default database and a named database literally called `default` never share a connection.
    /// 验证默认数据库与字面名称为 `default` 的命名数据库绝不会共享连接。
    #[tokio::test]
    async fn named_default_does_not_collide_with_the_default_database() {
        let (_directory, runtime) = sample_runtime();

        let default_engine = runtime
            .open_default_engine()
            .await
            .expect("default database should open");
        let named_engine = runtime
            .open_named_engine(Some("default"))
            .await
            .expect("named default database should open");

        assert!(
            !Arc::ptr_eq(&default_engine.connection(), &named_engine.connection()),
            "two distinct physical paths must never share one cached connection"
        );
        assert_eq!(
            runtime.cached_database_keys().await,
            vec!["<default>".to_string(), "named:default".to_string()]
        );
        assert_eq!(runtime.cached_engine_count().await, 2);
        assert_eq!(runtime.held_root_lock_count().await, 2);
    }

    /// Verifies concurrent in-process opens of one root converge on one lock and one engine.
    /// 验证同一根的并发进程内打开收敛为一把锁与一个引擎。
    ///
    /// This is the baseline's in-process requirement expressed as a test: a second logical consumer must share
    /// the first one's lock and engine rather than fail, and a failure here would mean the runtime had become
    /// able to hold two locks for one physical root.
    /// 这是把基线的进程内要求表述为测试：第二个逻辑消费者必须共享第一个的锁与引擎而不是失败，此处失败就意味着
    /// 运行时能够为同一个物理根持有两把锁。
    #[tokio::test]
    async fn concurrent_opens_converge_on_one_lock_and_one_engine() {
        let (_directory, runtime) = tempdir_runtime();

        let (first, second) =
            tokio::join!(runtime.open_default_engine(), runtime.open_default_engine());
        let first = first.expect("first concurrent open");
        let second = second.expect("second concurrent open");

        assert!(
            Arc::ptr_eq(&first.connection(), &second.connection()),
            "both openers must receive the same shared connection"
        );
        assert_eq!(runtime.cached_engine_count().await, 1);
        assert_eq!(
            runtime.held_root_lock_count().await,
            1,
            "one physical root must yield exactly one held lock"
        );
    }

    /// Verifies a named database is refused when no root is configured, without opening anything.
    /// 验证未配置根时命名数据库被拒绝，且不打开任何连接。
    #[tokio::test]
    async fn named_database_without_root_is_refused() {
        let (_directory, runtime) = tempdir_runtime();

        let result = runtime.open_named_engine(Some("orphan")).await;
        assert!(result.is_err());
        assert_eq!(
            runtime.cached_engine_count().await,
            0,
            "a refused open must not populate the engine cache"
        );
    }
}
