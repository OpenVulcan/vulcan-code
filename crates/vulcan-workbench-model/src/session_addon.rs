//! Session-scoped addon models shared by WebUI, service, and persistence layers.
//! 由 WebUI、服务与持久化层共享的会话级附属模型。

use serde::{Deserialize, Serialize};

use crate::{SessionPlanModeState, SessionRuntimeStateEntry};

/// One stable session addon namespace named by addon change events.
/// 附属状态变更事件中指明的一个稳定会话命名空间。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SessionAddonNamespace {
    /// Session index metadata mirrored for addon-level notifications.
    /// 用于附属状态级通知的会话索引元信息镜像。
    Metadata,
    /// User-selected provider, model, reasoning, and temperature controls.
    /// 用户选择的供应商、模型、推理档位与温度控制。
    RuntimeControls,
    /// Session-scoped user work mode such as planning or building.
    /// 会话级用户工作模式，例如计划或构建。
    WorkMode,
    /// Session-level output-style override and applied prompt snapshot.
    /// 会话级输出风格覆盖与已应用提示词快照。
    OutputStyle,
    /// Session-level prompt skill overrides.
    /// 会话级提示词技能覆盖。
    PromptSkills,
    /// Session-level context-injection state.
    /// 会话级上下文注入状态。
    ContextInjection,
    /// Session-level VMM state and memory settings.
    /// 会话级 VMM 状态与记忆设置。
    Vmm,
    /// Session-level knowledge-library selection, separate from conversational memory.
    /// 与会话记忆分离的会话级知识库选择。
    KnowledgeBases,
    /// Session-level todolist updated by the model-facing todolist tool.
    /// 由模型可见 todolist 工具更新的会话级 todolist。
    Todolist,
    /// Session composer and pending prompt UI state.
    /// 会话输入框与待发送提示词界面状态。
    Composer,
    /// Last provider request preview rendered for the session.
    /// 会话最近一次渲染的 Provider 请求预览。
    RequestPreview,
    /// Context reconstruction anchor and budget fingerprints.
    /// 上下文重建锚点与预算指纹。
    ContextAnchor,
}

impl SessionAddonNamespace {
    /// Returns the stable string key used in events.
    /// 返回事件中使用的稳定字符串键。
    ///
    /// # Returns
    /// 返回值
    /// The stable snake_case namespace key.
    /// 稳定的 snake_case 命名空间键。
    pub fn as_key(self) -> &'static str {
        match self {
            Self::Metadata => "metadata",
            Self::RuntimeControls => "runtime_controls",
            Self::WorkMode => "work_mode",
            Self::OutputStyle => "output_style",
            Self::PromptSkills => "prompt_skills",
            Self::ContextInjection => "context_injection",
            Self::Vmm => "vmm",
            Self::KnowledgeBases => "knowledge_bases",
            Self::Todolist => "todolist",
            Self::Composer => "composer",
            Self::RequestPreview => "request_preview",
            Self::ContextAnchor => "context_anchor",
        }
    }
}

/// Session selection strategy for project-authorized knowledge libraries.
/// 项目已授权知识库的会话选择策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SessionKnowledgeLibrarySelectionMode {
    /// Inherit libraries marked default-enabled for the current project.
    /// 继承当前项目中标记为默认启用的知识库。
    #[default]
    Inherit,
    /// Use only explicitly listed libraries after authorization intersection.
    /// 与授权集合求交后，仅使用显式列出的知识库。
    Explicit,
}

/// Durable per-session knowledge-library selection.
/// 持久化的每会话知识库选择。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionKnowledgeLibrarySelection {
    /// Selection strategy.
    /// 选择策略。
    #[serde(default)]
    pub mode: SessionKnowledgeLibrarySelectionMode,
    /// Explicit library IDs, used only when mode is explicit.
    /// 显式知识库标识符，仅在 explicit 模式下使用。
    #[serde(default)]
    pub library_ids: Vec<String>,
}

impl SessionKnowledgeLibrarySelection {
    /// Returns a canonical selection with deterministic unique identifiers.
    /// 返回具有确定性唯一标识符的规范选择。
    ///
    /// # Returns
    /// 返回值
    /// A sorted and deduplicated explicit selection, or an identifier-free inherited selection.
    /// 返回排序去重后的显式选择，或不含标识符的继承选择。
    pub fn normalized(mut self) -> Self {
        self.library_ids.sort();
        self.library_ids.dedup();
        if self.mode == SessionKnowledgeLibrarySelectionMode::Inherit {
            self.library_ids.clear();
        }
        self
    }

    /// Returns whether the selection uses the implicit project defaults.
    /// 返回该选择是否使用隐式项目默认值。
    ///
    /// # Returns
    /// 返回值
    /// `true` when the selection is the default inherit mode without explicit identifiers.
    /// 当选择为默认继承模式且不含显式标识符时返回 `true`。
    pub fn is_default(&self) -> bool {
        self.mode == SessionKnowledgeLibrarySelectionMode::Inherit && self.library_ids.is_empty()
    }
}

/// Session-scoped user work mode managed outside transcript hydration.
/// 在转录水合之外管理的会话级用户工作模式。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionWorkMode {
    /// Stable work mode key such as `build` or `plan`.
    /// 稳定工作模式键，例如 `build` 或 `plan`。
    pub execution_mode: String,
    /// Session-scoped Plan mode marker and latest generated artifact.
    /// 会话级计划模式标记与最近生成的计划产物。
    #[serde(default, skip_serializing_if = "SessionPlanModeState::is_empty")]
    pub plan_mode: SessionPlanModeState,
}

impl SessionWorkMode {
    /// Applies the work mode to the session runtime state.
    /// 将工作模式写入会话运行态。
    ///
    /// # Parameters
    /// 参数
    /// `runtime_state` is the session runtime state to update.
    /// `runtime_state` 是要更新的会话运行态。
    pub fn apply_to_runtime_state(&self, runtime_state: &mut SessionRuntimeStateEntry) {
        runtime_state.active_work_mode = Some(self.execution_mode.clone());
        runtime_state.plan_mode = self.plan_mode.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies explicit knowledge selections are sorted and deduplicated.
    /// 验证显式知识库选择会排序并去重。
    #[test]
    fn explicit_selection_is_normalized() {
        let selection = SessionKnowledgeLibrarySelection {
            mode: SessionKnowledgeLibrarySelectionMode::Explicit,
            library_ids: vec![String::from("b"), String::from("a"), String::from("b")],
        };

        assert_eq!(selection.normalized().library_ids, vec!["a", "b"]);
    }

    /// Verifies inherited selection never keeps irrelevant explicit identifiers.
    /// 验证继承型选择绝不保留无关的显式标识符。
    #[test]
    fn inherited_selection_clears_explicit_ids() {
        let selection = SessionKnowledgeLibrarySelection {
            mode: SessionKnowledgeLibrarySelectionMode::Inherit,
            library_ids: vec![String::from("ignored")],
        };

        assert!(selection.normalized().library_ids.is_empty());
    }
}
