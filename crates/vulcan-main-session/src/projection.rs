use serde::{Deserialize, Serialize};

/// Projection policy owned by normal main sessions.
/// 普通主会话拥有的投影策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MainSessionProjectionPolicy {
    /// Whether normal session database projection is enabled.
    /// 是否启用普通会话数据库投影。
    pub database_projection_enabled: bool,
    /// Whether workspace history search projection is enabled.
    /// 是否启用工作区历史搜索投影。
    pub search_projection_enabled: bool,
    /// Whether BM25 indexing is enabled.
    /// 是否启用 BM25 索引。
    pub bm25_enabled: bool,
    /// Whether FTS indexing is enabled.
    /// 是否启用 FTS 索引。
    pub fts_enabled: bool,
}

impl Default for MainSessionProjectionPolicy {
    /// Builds the default main-session projection policy.
    /// 构建默认主会话投影策略。
    fn default() -> Self {
        Self {
            database_projection_enabled: true,
            search_projection_enabled: false,
            bm25_enabled: false,
            fts_enabled: false,
        }
    }
}
