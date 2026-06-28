use super::*;

/// Describes Git commit dialog settings projected to WebUI clients.
/// 描述投影给 WebUI 客户端的 Git 提交弹窗设置。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GitCommitSettingsSummary {
    /// Default namespace prefix used when the current branch has no namespace.
    /// 当前分支没有命名空间时使用的默认命名空间前缀。
    pub new_branch_prefix: String,
    /// Model selection used when commit messages are generated automatically.
    /// 自动生成提交信息时使用的模型选择。
    #[serde(default)]
    pub summary_model: GitCommitSummaryModelSelectionSummary,
    /// User-authored extra prompt appended to commit-message generation.
    /// 追加到提交信息生成中的用户额外提示词。
    #[serde(default)]
    pub summary_extra_prompt: String,
}

impl Default for GitCommitSettingsSummary {
    /// Builds the WebUI default Git commit settings summary.
    /// 构建 WebUI 默认 Git 提交设置摘要。
    fn default() -> Self {
        Self {
            new_branch_prefix: String::from("vulcan/"),
            summary_model: GitCommitSummaryModelSelectionSummary::default(),
            summary_extra_prompt: String::new(),
        }
    }
}

/// Describes the model selection used for automatic Git commit summaries.
/// 描述自动 Git 提交总结使用的模型选择。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum GitCommitSummaryModelSelectionSummary {
    /// Follow the model selected by the session that owns the commit action.
    /// 跟随发起提交动作的会话当前模型。
    Session,
    /// Always use one fixed provider/model pair.
    /// 始终使用一组固定 Provider/模型。
    Fixed {
        /// Stable provider identifier.
        /// 稳定 Provider 标识。
        provider_id: String,
        /// Stable model identifier.
        /// 稳定模型标识。
        model_id: String,
    },
}

impl Default for GitCommitSummaryModelSelectionSummary {
    /// Builds the default session-following summary model selection.
    /// 构建默认的跟随会话总结模型选择。
    fn default() -> Self {
        Self::Session
    }
}
