use super::*;

/// Default namespace prefix for new Git commit branches created from non-namespaced branches.
/// 基于无命名空间分支创建 Git 提交新分支时使用的默认命名空间前缀。
pub const DEFAULT_GIT_COMMIT_NEW_BRANCH_PREFIX: &str = "vulcan/";

/// Default commit summary prompt preference.
/// 默认提交总结提示词偏好。
const DEFAULT_GIT_COMMIT_SUMMARY_EXTRA_PROMPT: &str = "";

/// Default RTK usage for commit-message summary input.
/// 提交信息总结输入默认使用 RTK。
const DEFAULT_GIT_COMMIT_USE_RTK_SUMMARY: bool = true;

/// Returns the default Git commit new-branch namespace prefix.
/// 返回 Git 提交新分支命名空间的默认前缀。
pub(super) fn default_git_commit_new_branch_prefix() -> String {
    String::from(DEFAULT_GIT_COMMIT_NEW_BRANCH_PREFIX)
}

/// Returns whether a Git commit new-branch prefix is still the default value.
/// 返回 Git 提交新分支前缀是否仍为默认值。
pub(super) fn git_commit_new_branch_prefix_is_default(value: &str) -> bool {
    value == DEFAULT_GIT_COMMIT_NEW_BRANCH_PREFIX
}

/// Normalizes one Git commit branch prefix into a non-empty namespace prefix.
/// 将单个 Git 提交分支前缀归一化为非空命名空间前缀。
pub fn normalize_git_commit_new_branch_prefix(value: &str) -> String {
    let trimmed_value = value.trim();
    if trimmed_value.is_empty() {
        return String::from(DEFAULT_GIT_COMMIT_NEW_BRANCH_PREFIX);
    }
    if trimmed_value.ends_with('/') {
        trimmed_value.to_string()
    } else {
        format!("{trimmed_value}/")
    }
}

/// Returns whether one commit summary prompt preference is empty.
/// 返回单个提交总结提示词偏好是否为空。
pub(super) fn git_commit_summary_extra_prompt_is_empty(value: &str) -> bool {
    value.trim().is_empty()
}

/// Returns the default RTK usage state for commit-message summary input.
/// 返回提交信息总结输入默认 RTK 使用状态。
pub(super) const fn default_git_commit_use_rtk_summary() -> bool {
    DEFAULT_GIT_COMMIT_USE_RTK_SUMMARY
}

/// Returns whether commit-message summary RTK usage is still the default value.
/// 返回提交信息总结 RTK 使用状态是否仍为默认值。
pub(super) const fn git_commit_use_rtk_summary_is_default(value: &bool) -> bool {
    *value == DEFAULT_GIT_COMMIT_USE_RTK_SUMMARY
}

/// Selects which model should generate Git commit summaries.
/// 选择用于生成 Git 提交总结的模型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum GitCommitSummaryModelSelection {
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

impl Default for GitCommitSummaryModelSelection {
    /// Builds the default session-following summary model selection.
    /// 构建默认的跟随会话总结模型选择。
    fn default() -> Self {
        Self::Session
    }
}

impl GitCommitSummaryModelSelection {
    /// Returns whether the selection follows the owning session model.
    /// 返回该选择是否跟随所属会话模型。
    pub const fn is_session(&self) -> bool {
        matches!(self, Self::Session)
    }

    /// Normalizes persisted identifiers and falls back to session mode when incomplete.
    /// 归一化持久化标识，并在固定模型不完整时回退为跟随会话模式。
    pub fn normalized(&self) -> Self {
        match self {
            Self::Session => Self::Session,
            Self::Fixed {
                provider_id,
                model_id,
            } => {
                let provider_id = provider_id.trim().to_string();
                let model_id = model_id.trim().to_string();
                if provider_id.is_empty() || model_id.is_empty() {
                    Self::Session
                } else {
                    Self::Fixed {
                        provider_id,
                        model_id,
                    }
                }
            }
        }
    }
}

/// Persisted Git commit dialog behavior stored in the shared Vulcan configuration file.
/// 存储在共享 Vulcan 配置文件中的 Git 提交弹窗行为配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedGitCommitConfiguration {
    /// Default namespace prefix for new branches created from non-namespaced branches.
    /// 基于无命名空间分支创建新分支时使用的默认命名空间前缀。
    #[serde(
        default = "default_git_commit_new_branch_prefix",
        skip_serializing_if = "git_commit_new_branch_prefix_is_default"
    )]
    pub new_branch_prefix: String,
    /// Model selection used when commit messages are generated automatically.
    /// 自动生成提交信息时使用的模型选择。
    #[serde(
        default,
        skip_serializing_if = "GitCommitSummaryModelSelection::is_session"
    )]
    pub summary_model: GitCommitSummaryModelSelection,
    /// User-authored extra prompt appended to commit-message generation.
    /// 追加到提交信息生成中的用户额外提示词。
    #[serde(
        default,
        skip_serializing_if = "git_commit_summary_extra_prompt_is_empty"
    )]
    pub summary_extra_prompt: String,
    /// Whether commit-message generation should prefer RTK-compressed Git input.
    /// 提交信息生成是否优先使用 RTK 压缩后的 Git 输入。
    #[serde(
        default = "default_git_commit_use_rtk_summary",
        skip_serializing_if = "git_commit_use_rtk_summary_is_default"
    )]
    pub use_rtk_summary: bool,
}

impl Default for PersistedGitCommitConfiguration {
    /// Builds the default Git commit dialog configuration.
    /// 构建默认 Git 提交弹窗配置。
    fn default() -> Self {
        Self {
            new_branch_prefix: default_git_commit_new_branch_prefix(),
            summary_model: GitCommitSummaryModelSelection::default(),
            summary_extra_prompt: String::from(DEFAULT_GIT_COMMIT_SUMMARY_EXTRA_PROMPT),
            use_rtk_summary: DEFAULT_GIT_COMMIT_USE_RTK_SUMMARY,
        }
    }
}

impl PersistedGitCommitConfiguration {
    /// Returns whether every Git commit setting is still at its default value.
    /// 返回全部 Git 提交设置是否仍处于默认值。
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// Normalizes the persisted Git commit configuration in place.
    /// 就地归一化持久化的 Git 提交配置。
    pub fn normalize(&mut self) -> bool {
        let normalized_prefix = normalize_git_commit_new_branch_prefix(&self.new_branch_prefix);
        let normalized_summary_model = self.summary_model.normalized();
        let normalized_summary_extra_prompt = self.summary_extra_prompt.trim().to_string();
        let changed = normalized_prefix != self.new_branch_prefix
            || normalized_summary_model != self.summary_model
            || normalized_summary_extra_prompt != self.summary_extra_prompt;
        self.new_branch_prefix = normalized_prefix;
        self.summary_model = normalized_summary_model;
        self.summary_extra_prompt = normalized_summary_extra_prompt;
        changed
    }
}
