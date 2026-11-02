//! Runtime configuration and engine options shared by the client and the native library.
//! 客户端与原生库共用的运行时配置及引擎选项。
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

use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use crate::error::{EngineResult, LanceDbEngineError};

/// Database runtime configuration limited to what the engine actually needs.
/// 仅保留引擎真正需要的数据库运行时配置。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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

/// Engine options controlling safety limits and concurrency for one instance.
/// 控制单实例安全限制与并发行为的引擎选项。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LanceDbEngineOptions {
    /// Maximum accepted upsert payload size in bytes.
    /// 可接受的最大写入载荷字节数。
    pub max_upsert_payload: usize,
    /// Maximum rows a single search may return.
    /// 单次检索可返回的最大行数。
    pub max_search_limit: usize,
    /// Maximum concurrent in-flight requests for this engine.
    /// 本引擎的最大并发在途请求数。
    pub max_concurrent_requests: usize,
}

impl Default for LanceDbEngineOptions {
    /// Provides the migration's stable defaults.
    /// 提供迁移的稳定默认值。
    fn default() -> Self {
        Self {
            max_upsert_payload: 50 * 1024 * 1024,
            max_search_limit: 10_000,
            max_concurrent_requests: 500,
        }
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
