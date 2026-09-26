//! Database path resolution and connection caching.
//! 数据库路径解析与连接缓存。
//!
//! This module owns the whole of the engine's **path authorization** rule. A named database is not
//! a free-form path: it must be a single path segment, so it cannot contain a separator, a parent
//! reference or a root marker, and it is always joined under the configured database root. The
//! validation is therefore what constrains where the engine may create directories and files, not
//! a convenience check.
//!
//! 本模块承担引擎**路径授权**规则的全部内容。命名数据库不是自由形式的路径：它必须是单个路径段，
//! 因此不能包含分隔符、父级引用或根标记，并且始终拼接在配置的数据库根之下。因此该校验约束的是
//! 引擎可以创建目录与文件的位置，而不是便利性检查。
//!
//! The migrated source returned `Box<dyn Error>`; this port returns the crate's structured
//! [`LanceDbEngineError`] so a caller can distinguish a bad name from a failed open without string
//! matching.
//!
//! 迁入源码返回 `Box<dyn Error>`；本移植返回本 crate 结构化错误 [`LanceDbEngineError`]，使调用方
//! 无需字符串匹配即可区分名称非法与打开失败。

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use lancedb::Connection;
use tokio::sync::Mutex;

use crate::error::{EngineResult, LanceDbEngineError};

/// Strong cache identity separating the default database from every legal named database.
/// 强类型缓存身份，把默认数据库与所有合法命名数据库分隔开。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum DatabaseCacheKey {
    /// The configured default database path.
    /// 已配置的默认数据库路径。
    Default,
    /// One validated child name beneath the configured database root.
    /// 已配置数据库根下的一个已校验子名称。
    Named(String),
}

impl DatabaseCacheKey {
    /// Returns an unambiguous diagnostic label for this cache entry.
    /// 返回此缓存条目的无歧义诊断标签。
    fn diagnostic_label(&self) -> String {
        match self {
            Self::Default => "<default>".to_string(),
            Self::Named(name) => format!("named:{name}"),
        }
    }
}

/// Database runtime configuration limited to what the engine actually needs.
/// 仅保留引擎真正需要的数据库运行时配置。
#[derive(Debug, Clone)]
pub struct DatabaseRuntimeConfig {
    /// Local filesystem path of the default database.
    /// 默认数据库的本地文件系统路径。
    pub default_db_path: PathBuf,
    /// Optional root under which named databases are created.
    /// 可选根目录，命名数据库在其下创建。
    pub db_root: Option<PathBuf>,
    /// Optional read-consistency window in milliseconds.
    /// 可选的读一致性窗口，单位为毫秒。
    pub read_consistency_interval_ms: Option<u64>,
}

impl DatabaseRuntimeConfig {
    /// Returns the configured read-consistency window as a duration.
    /// 以持续时间返回配置的读一致性窗口。
    pub fn read_consistency_interval(&self) -> Option<Duration> {
        self.read_consistency_interval_ms.map(Duration::from_millis)
    }

    /// Resolves the target database path by name; a blank name means the default database.
    /// 按名称解析目标数据库路径；空名称表示默认数据库。
    pub fn database_path_for_name(&self, database_name: Option<&str>) -> EngineResult<PathBuf> {
        let normalized = database_name
            .map(str::trim)
            .filter(|value| !value.is_empty());

        let Some(database_name) = normalized else {
            return Ok(self.default_db_path.clone());
        };
        validate_database_name(database_name)?;

        let Some(db_root) = self.db_root.as_ref() else {
            return Err(LanceDbEngineError::invalid_argument(format!(
                "database `{database_name}` requires a database root to be configured"
            )));
        };

        Ok(db_root.join(database_name))
    }
}

/// Lazily opens and caches connections for the default database and named child databases.
/// 惰性打开并缓存默认数据库与命名子库的连接。
#[derive(Clone)]
pub struct DatabaseManager {
    /// Shared immutable configuration.
    /// 共享的不可变配置。
    config: Arc<DatabaseRuntimeConfig>,
    /// Cached connections keyed by normalized database name.
    /// 以规范化数据库名为键的连接缓存。
    connections: Arc<Mutex<HashMap<DatabaseCacheKey, Arc<Connection>>>>,
}

impl DatabaseManager {
    /// Creates a manager from resolved configuration.
    /// 基于解析后的配置创建管理器。
    pub fn new(config: DatabaseRuntimeConfig) -> Self {
        Self {
            config: Arc::new(config),
            connections: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Returns the shared configuration.
    /// 返回共享配置。
    pub fn config(&self) -> &DatabaseRuntimeConfig {
        self.config.as_ref()
    }

    /// Opens or retrieves the default database connection.
    /// 打开或获取默认数据库连接。
    pub async fn open_default(&self) -> EngineResult<Arc<Connection>> {
        self.open_named(None).await
    }

    /// Opens or retrieves a connection by name; `None` means the default database.
    /// 按名称打开或获取连接；`None` 表示默认数据库。
    ///
    /// The cache is consulted under a short lock, but the connection is established **outside** the
    /// lock so a slow open cannot block unrelated databases. Two concurrent opens of the same name
    /// may therefore both connect; the `entry` insert resolves the race in favour of the first
    /// stored connection and the duplicate is dropped.
    ///
    /// 缓存在短暂持锁下查询，但连接建立发生在**锁之外**，使缓慢的打开不会阻塞无关数据库。因此
    /// 同名并发的两次打开都可能真正连接；`entry` 插入会让先存入的连接胜出，重复者被丢弃。
    pub async fn open_named(&self, database_name: Option<&str>) -> EngineResult<Arc<Connection>> {
        let database_key = normalize_database_key(database_name);
        let target_path = self.config.database_path_for_name(database_name)?;

        {
            let guard = self.connections.lock().await;
            if let Some(existing) = guard.get(&database_key) {
                return Ok(Arc::clone(existing));
            }
        }

        let target_uri = target_path.to_str().ok_or_else(|| {
            LanceDbEngineError::path_not_authorized(format!(
                "LanceDB requires a Unicode local path and cannot open '{}' without changing its identity",
                target_path.display()
            ))
        })?;

        tokio::fs::create_dir_all(&target_path)
            .await
            .map_err(|error| {
                LanceDbEngineError::open(
                    target_path.clone(),
                    format!("failed to create the database directory: {error}"),
                )
            })?;

        let mut builder = lancedb::connect(target_uri)
            // Pin the v1 Lance file format for every table this connection creates. The option is
            // applied here because `CreateTableBuilder` exposes no storage-version method; see
            // `crate::storage_format` for the precedence analysis and the read-back verification
            // that turns this from a request into a guarantee.
            // 为本次连接创建的所有表固定 v1 Lance 文件格式。该选项在此施加，因为
            // `CreateTableBuilder` 不暴露存储版本方法；优先级分析与使其从请求变为保证的读回校验
            // 见 `crate::storage_format`。
            .storage_option(
                crate::storage_format::PINNED_STORAGE_VERSION_OPTION,
                crate::storage_format::PINNED_STORAGE_VERSION_VALUE,
            );
        if let Some(interval) = self.config.read_consistency_interval() {
            builder = builder.read_consistency_interval(interval);
        }

        let connection = Arc::new(builder.execute().await.map_err(|error| {
            LanceDbEngineError::open(target_path, format!("failed to connect: {error}"))
        })?);

        let mut guard = self.connections.lock().await;
        let entry = guard
            .entry(database_key)
            .or_insert_with(|| Arc::clone(&connection));
        Ok(Arc::clone(entry))
    }

    /// Resolves the concrete path for a database name without opening a connection.
    /// 解析某个库名对应的实际路径，不触发连接创建。
    pub fn database_path_for_name(&self, database_name: Option<&str>) -> EngineResult<PathBuf> {
        self.config.database_path_for_name(database_name)
    }

    /// Returns cached database keys in sorted order, for diagnostics.
    /// 按排序返回已缓存的库键，用于诊断。
    pub async fn cached_database_keys(&self) -> Vec<String> {
        let guard = self.connections.lock().await;
        let mut keys = guard
            .keys()
            .map(DatabaseCacheKey::diagnostic_label)
            .collect::<Vec<_>>();
        keys.sort();
        keys
    }
}

/// Normalizes an external database name into the internal cache key.
/// 将外部输入的库名统一规整成内部缓存键。
fn normalize_database_key(database_name: Option<&str>) -> DatabaseCacheKey {
    match database_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(name) => DatabaseCacheKey::Named(name.to_string()),
        None => DatabaseCacheKey::Default,
    }
}

/// Validates that a named database identifier is a single safe path segment.
/// 校验命名数据库标识为安全的单个路径段。
///
/// Separators are rejected before the component walk so the refusal message names the actual
/// problem, and because on Windows a backslash is a separator that `Path::components` would treat
/// as one only after platform-specific normalization.
///
/// 在组件遍历之前先拒绝分隔符，使拒绝消息指出真正的问题；同时因为在 Windows 上反斜杠是分隔符，而
/// `Path::components` 只在平台相关规范化之后才会如此处理。
fn validate_database_name(database_name: &str) -> EngineResult<()> {
    if database_name.contains('/') || database_name.contains('\\') {
        return Err(LanceDbEngineError::path_not_authorized(format!(
            "database `{database_name}` must not contain path separators"
        )));
    }

    let mut components = Path::new(database_name).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(()),
        _ => Err(LanceDbEngineError::path_not_authorized(format!(
            "database `{database_name}` must be a single path segment"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::{DatabaseManager, DatabaseRuntimeConfig};
    use std::path::PathBuf;

    /// Builds a configuration with a root so named databases can resolve.
    /// 构造一个带根的配置，使命名数据库可解析。
    fn sample_config() -> DatabaseRuntimeConfig {
        DatabaseRuntimeConfig {
            default_db_path: PathBuf::from("/srv/vldb/default"),
            db_root: Some(PathBuf::from("/srv/vldb/databases")),
            read_consistency_interval_ms: Some(0),
        }
    }

    /// Verifies the default database path comes from the default path field.
    /// 验证默认数据库路径取自默认路径字段。
    #[test]
    fn default_database_path_uses_default_db_path() {
        let manager = DatabaseManager::new(sample_config());
        assert_eq!(
            manager
                .database_path_for_name(None)
                .expect("default database path should resolve"),
            PathBuf::from("/srv/vldb/default")
        );
    }

    /// Verifies a named database resolves beneath the configured root.
    /// 验证命名数据库解析到配置根之下。
    #[test]
    fn named_database_path_uses_database_root() {
        let manager = DatabaseManager::new(sample_config());
        assert_eq!(
            manager
                .database_path_for_name(Some("memory"))
                .expect("named database path should resolve"),
            PathBuf::from("/srv/vldb/databases").join("memory")
        );
    }

    /// Verifies a traversal name is refused.
    /// 验证路径穿越名称被拒绝。
    #[test]
    fn named_database_path_rejects_traversal_segments() {
        let manager = DatabaseManager::new(sample_config());
        let error = manager
            .database_path_for_name(Some("../escape"))
            .expect_err("traversal name should be rejected");
        assert!(error.to_string().contains("path separators"));
    }

    /// Verifies a nested name is refused.
    /// 验证嵌套名称被拒绝。
    #[test]
    fn named_database_path_rejects_path_separators() {
        let manager = DatabaseManager::new(sample_config());
        let error = manager
            .database_path_for_name(Some("nested/name"))
            .expect_err("nested path should be rejected");
        assert!(error.to_string().contains("path separators"));
    }

    /// Verifies authorization refusals are reported as such rather than as plain bad arguments, so
    /// a caller can tell an escape attempt from a typo.
    /// 验证授权拒绝按其自身类别上报而不是普通参数错误，使调用方能把越权尝试与拼写错误区分开。
    #[test]
    fn path_escape_is_reported_as_authorization_failure() {
        let manager = DatabaseManager::new(sample_config());

        for name in ["../escape", "nested/name", "..\\escape", "/absolute", ".."] {
            let error = manager
                .database_path_for_name(Some(name))
                .expect_err("unsafe database name must be refused");
            assert!(
                matches!(
                    error,
                    crate::error::LanceDbEngineError::PathNotAuthorized { .. }
                ),
                "database name {name:?} must be reported as PathNotAuthorized, got {error:?}"
            );
        }
    }

    /// Verifies a named database without a configured root is refused.
    /// 验证未配置根时命名数据库被拒绝。
    #[test]
    fn named_database_without_root_is_refused() {
        let manager = DatabaseManager::new(DatabaseRuntimeConfig {
            default_db_path: PathBuf::from("/srv/vldb/default"),
            db_root: None,
            read_consistency_interval_ms: None,
        });

        let error = manager
            .database_path_for_name(Some("memory"))
            .expect_err("named database without a root must be refused");
        assert!(error.to_string().contains("requires a database root"));
    }

    /// Verifies blank and whitespace names fall back to the default database.
    /// 验证空白名称回落为默认数据库。
    #[test]
    fn blank_name_falls_back_to_default_database() {
        let manager = DatabaseManager::new(sample_config());

        for name in ["", "   ", "\t"] {
            assert_eq!(
                manager
                    .database_path_for_name(Some(name))
                    .expect("blank name should resolve to the default"),
                PathBuf::from("/srv/vldb/default")
            );
        }
    }

    /// Verifies conversion of the declared read-consistency duration.
    /// 验证声明的读一致性持续时间转换。
    #[test]
    fn config_reports_read_consistency_duration() {
        let local = sample_config();
        assert_eq!(
            local.read_consistency_interval(),
            Some(std::time::Duration::from_millis(0))
        );
    }

    /// Verifies a non-Unicode path is refused before a lossy path could target another database.
    /// 验证非 Unicode 路径会在有损转换可能指向另一数据库之前被拒绝。
    #[cfg(unix)]
    #[tokio::test]
    async fn non_unicode_default_path_is_refused_without_creating_it() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let directory = tempfile::tempdir().expect("temporary directory should exist");
        let target = directory
            .path()
            .join(OsString::from_vec(vec![b'l', b'a', b'n', b'c', b'e', 0xff]));
        let manager = DatabaseManager::new(DatabaseRuntimeConfig {
            default_db_path: target.clone(),
            db_root: None,
            read_consistency_interval_ms: None,
        });

        // The success type does not implement Debug; match the Result without weakening the assertion.
        // 成功类型没有实现 Debug；显式匹配结果，同时保留拒绝非法路径的断言。
        let error = match manager.open_default().await {
            Ok(_) => panic!("non-Unicode path must be refused"),
            Err(error) => error,
        };
        assert!(matches!(
            error,
            crate::error::LanceDbEngineError::PathNotAuthorized { .. }
        ));
        assert!(!target.exists(), "a refused path must not be created");
    }
}
