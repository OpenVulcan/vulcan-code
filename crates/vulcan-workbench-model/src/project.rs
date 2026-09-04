//! Project summary models shared by the task tree and persistence layers.
//! 供任务树与持久化层共享的项目摘要模型。

use crate::SessionId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Persisted project-owned settings for the native code-semantic index.
/// 原生代码语义索引使用的项目自有持久化设置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectVceSettings {
    /// Whether the native code-semantic index is enabled for this project.
    /// 此项目是否启用原生代码语义索引。
    pub enabled: bool,
    /// Optimistic settings revision incremented by every accepted update.
    /// 每次接受更新时递增的乐观设置修订号。
    pub revision: u64,
    /// Whether the initial enabled state was migrated from the former native CBM switch.
    /// 初始启用状态是否由原生 CBM 旧开关迁移而来。
    pub migrated_from_codebase_memory: bool,
    /// Maximum UTF-8 source file size accepted for semantic indexing.
    /// 语义索引允许接收的 UTF-8 源文件最大字节数。
    pub max_file_bytes: u64,
    /// Additional project-relative gitignore-style patterns applied by the VCE source adapter.
    /// VCE 源码适配器应用的附加项目相对 gitignore 风格模式。
    pub extra_ignore_patterns: Vec<String>,
    /// Whether optional LLM query rewriting may be used during retrieval.
    /// 检索期间是否允许使用可选 LLM 查询改写。
    pub allow_query_rewrite: bool,
    /// Whether optional host reranking may be used during retrieval.
    /// 检索期间是否允许使用可选宿主重排。
    pub allow_rerank: bool,
    /// Maximum concurrent embedding batches inside one project refresh.
    /// 单个项目刷新内允许并发执行的向量批次数量上限。
    pub embedding_max_concurrency: usize,
    /// Maximum concurrent query variants inside one retrieval.
    /// 单次检索内允许并发执行的查询变体数量上限。
    pub query_max_concurrency: usize,
    /// Maximum selected code chunks returned by one retrieval.
    /// 单次检索最多返回的代码块数量。
    pub max_results: usize,
    /// Hard aggregate character budget for one rendered retrieval context.
    /// 单次渲染检索上下文的硬性总字符预算。
    pub max_context_chars: usize,
}

impl Default for ProjectVceSettings {
    /// Builds the conservative OCE-inspired native integration defaults.
    /// 构建保守的 OCE 思路原生集成默认值。
    fn default() -> Self {
        Self {
            enabled: false,
            revision: 1,
            migrated_from_codebase_memory: false,
            max_file_bytes: 1_048_576,
            extra_ignore_patterns: Vec::new(),
            allow_query_rewrite: false,
            allow_rerank: true,
            embedding_max_concurrency: 4,
            query_max_concurrency: 3,
            max_results: 10,
            max_context_chars: 32_000,
        }
    }
}

impl ProjectVceSettings {
    /// Validates persisted VCE budgets and ignore patterns before they reach host adapters.
    /// 在 VCE 持久化预算与忽略规则进入宿主适配器前进行校验。
    ///
    /// # Returns
    /// 返回值
    /// Success for a bounded internally consistent settings record, otherwise a stable error.
    /// 设置记录有界且内部一致时返回成功，否则返回稳定错误。
    pub fn validate(&self) -> Result<(), String> {
        if self.revision == 0 {
            return Err(String::from("VCE 设置修订号必须大于零。"));
        }
        if !(1..=16 * 1_048_576).contains(&self.max_file_bytes) {
            return Err(String::from("VCE 最大文件字节数必须位于 1 到 16777216。"));
        }
        if self.extra_ignore_patterns.len() > 64 {
            return Err(String::from("VCE 附加忽略规则不能超过 64 条。"));
        }
        if let Some(pattern) = self.extra_ignore_patterns.iter().find(|pattern| {
            pattern.trim().is_empty()
                || pattern.chars().count() > 256
                || pattern
                    .chars()
                    .any(|character| matches!(character, '\r' | '\n' | '\0'))
        }) {
            return Err(format!("VCE 附加忽略规则无效：`{pattern}`。"));
        }
        if !(1..=16).contains(&self.embedding_max_concurrency) {
            return Err(String::from("VCE 向量并发数必须位于 1 到 16。"));
        }
        if !(1..=8).contains(&self.query_max_concurrency) {
            return Err(String::from("VCE 查询并发数必须位于 1 到 8。"));
        }
        if !(1..=50).contains(&self.max_results) {
            return Err(String::from("VCE 结果数量必须位于 1 到 50。"));
        }
        if !(1_024..=200_000).contains(&self.max_context_chars) {
            return Err(String::from("VCE 上下文字符数必须位于 1024 到 200000。"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::ProjectVceSettings;

    /// Verifies the persisted VCE defaults remain bounded and internally valid.
    /// 验证持久化 VCE 默认值保持有界且内部有效。
    #[test]
    fn project_vce_defaults_are_valid() {
        let settings = ProjectVceSettings::default();

        assert!(settings.validate().is_ok());
        assert!(!settings.enabled);
        assert_eq!(settings.revision, 1);
        assert!(settings.allow_rerank);
    }

    /// Verifies malformed ignore patterns and out-of-range budgets fail before persistence.
    /// 验证畸形忽略规则与越界预算会在持久化前失败。
    #[test]
    fn project_vce_validation_rejects_invalid_bounds() {
        let mut settings = ProjectVceSettings::default();
        settings.revision = 0;
        assert!(settings.validate().is_err());

        let mut settings = ProjectVceSettings::default();
        settings.extra_ignore_patterns = vec![String::from("src\nprivate")];
        assert!(settings.validate().is_err());

        let mut settings = ProjectVceSettings::default();
        settings.embedding_max_concurrency = 17;
        assert!(settings.validate().is_err());

        let mut settings = ProjectVceSettings::default();
        settings.max_context_chars = 1_023;
        assert!(settings.validate().is_err());
    }
}

/// Lightweight project node shown by the main task-drawer tree.
/// 主任务抽屉树中展示的轻量级项目节点。
#[derive(Debug, Clone)]
pub struct TaskProjectSummary {
    /// Stable project identifier used by the tree state.
    /// 供树状态使用的稳定项目标识。
    pub id: String,
    /// Project display title.
    /// 项目展示标题。
    pub title: String,
    /// Absolute project path used when opening the directory.
    /// 用于打开目录的项目绝对路径。
    pub path: String,
    /// Whether the project is currently pinned in the tree.
    /// 当前项目是否已在树中置顶。
    pub pinned: bool,
    /// Stable order used inside the current project section.
    /// 当前项目所在分区内部使用的稳定顺序值。
    pub order: usize,
    /// Optional project icon pack selected by the user.
    /// 用户选择的可选项目图标包。
    pub icon_pack: Option<String>,
    /// Optional project icon key selected by the user.
    /// 用户选择的可选项目图标键。
    pub icon_key: Option<String>,
    /// Legacy Codebase Memory enablement retained only as one-time VCE migration input.
    /// 仅作为 VCE 一次性迁移输入保留的旧 Codebase Memory 启用状态。
    pub codebase_memory_enabled: bool,
    /// Native code-semantic settings owned by this project.
    /// 此项目持有的原生代码语义设置。
    pub vce: ProjectVceSettings,
    /// Session identifiers nested under the project node.
    /// 挂载在项目节点下的会话标识集合。
    pub session_ids: Vec<SessionId>,
}

impl TaskProjectSummary {
    /// Creates one project node with explicit metadata and child-session bindings.
    /// 使用明确元数据与子会话绑定创建一个项目节点。
    #[expect(
        clippy::too_many_arguments,
        reason = "The constructor mirrors the persisted project summary shape used by seed and store callers."
    )]
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        path: impl Into<String>,
        pinned: bool,
        order: usize,
        icon_pack: Option<String>,
        icon_key: Option<String>,
        session_ids: Vec<SessionId>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            path: path.into(),
            pinned,
            order,
            icon_pack,
            icon_key,
            codebase_memory_enabled: false,
            vce: ProjectVceSettings::default(),
            session_ids,
        }
    }

    /// Applies the persisted legacy Codebase Memory flag used by VCE migration.
    /// 应用供 VCE 迁移使用的持久化旧 Codebase Memory 标记。
    ///
    /// # Parameters
    /// 参数
    /// `enabled` is the exact project-owned persisted enablement flag.
    /// `enabled` 是项目实体持有的精确持久化启用标记。
    ///
    /// # Returns
    /// 返回值
    /// Returns the updated project summary for deterministic projection construction.
    /// 返回更新后的项目摘要，以便确定性构建投影。
    pub fn with_codebase_memory_enabled(mut self, enabled: bool) -> Self {
        self.codebase_memory_enabled = enabled;
        self
    }

    /// Applies the exact persisted native code-semantic settings.
    /// 应用精确的持久化原生代码语义设置。
    ///
    /// # Parameters
    /// 参数
    /// `settings` is the validated project-owned VCE settings row.
    /// `settings` 是经过校验的项目自有 VCE 设置记录。
    ///
    /// # Returns
    /// 返回值
    /// Returns the updated project summary for deterministic runtime projection.
    /// 返回更新后的项目摘要，以便确定性构建运行时投影。
    pub fn with_vce_settings(mut self, settings: ProjectVceSettings) -> Self {
        self.vce = settings;
        self
    }

    /// Builds seed project-tree data for the workbench task drawer prototype.
    /// 为工作台任务抽屉原型构建种子项目树数据。
    pub fn seeded() -> Vec<Self> {
        vec![
            Self::new(
                "vulcan-luaskills",
                "LuaSkills",
                r"D:\projects\LuaSkills",
                false,
                0,
                None,
                None,
                vec![Uuid::from_u128(1), Uuid::from_u128(2)],
            ),
            Self::new(
                "vulcan-memory-mesh",
                "VulcanMemoryMesh",
                r"D:\projects\VulcanMemoryMesh",
                false,
                1,
                None,
                None,
                vec![Uuid::from_u128(3), Uuid::from_u128(4)],
            ),
            Self::new(
                "vulcan-plugins-opencode",
                "vulcan-plugins-opencode",
                r"D:\projects\vulcan-plugins-opencode",
                false,
                2,
                None,
                None,
                vec![],
            ),
        ]
    }
}
