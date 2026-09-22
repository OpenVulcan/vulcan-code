//! Durable child execution configuration for auxiliary question recovery.
//! 用于辅助答疑恢复的可靠子执行配置。

use super::{DomainError, SubagentTaskId, TurnId};
use crate::{ExecutionRuntimePolicy, SessionEffectiveConfig};
use serde::{Deserialize, Serialize};

mod creation;
mod preparation;
mod request_overrides;
pub use creation::{ChildCreationIntent, ChildCreationOrigin};
pub use preparation::ChildExecutionPreparation;
pub use request_overrides::{
    ChildExecutionRequestOverrides, ChildKnowledgeAccess, ChildMediaUnderstandingSelection,
    ChildOutputStyleSnapshot,
};

/// Captures resolved child configuration without serializing executable runtime handles.
/// 捕获已解析子配置，不序列化可执行运行时句柄。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChildExecutionProfile {
    /// Exact supported profile contract version.
    /// 精确支持的配置记录契约版本。
    pub version: u32,
    /// Root session owning the embedded task directory.
    /// 拥有嵌入任务目录的根会话。
    pub root_session_id: String,
    /// Direct parent session, which may itself be embedded.
    /// 直接父会话，其本身也可以是嵌入会话。
    pub parent_session_id: String,
    /// Stable task identity matching the child session directory.
    /// 与子会话目录一致的稳定任务身份。
    pub task_id: SubagentTaskId,
    /// Child document generation in which configuration was captured.
    /// 捕获配置时的子文档代际。
    pub segment_generation: u64,
    /// Business turn that produced this configuration; QA cannot replace it.
    /// 产生此配置的业务回合；答疑不能覆盖它。
    pub business_turn_id: TurnId,
    /// Exact business execution attempt whose preparation supplied the configuration.
    /// 准备过程提供配置的精确业务执行尝试。
    pub business_execution_id: String,
    /// Original runtime-facing identity, preserved independently of editable session titles.
    /// 原运行时身份，独立于可编辑会话标题保留。
    pub runtime_identity: ChildExecutionRuntimeIdentity,
    /// Exact original invocation, retaining optional overrides before inheritance resolution.
    /// 精确原调用，保留继承解析前的可选覆盖。
    pub invocation_config: crate::TaskInvocationConfig,
    /// Original resolver output, independent of subsequent catalog or global default changes.
    /// 原解析器输出，独立于后续目录或全局默认值变化。
    pub resolved_config: crate::TaskSessionResolvedConfig,
    /// Resolved child model, tools, skills and compaction configuration.
    /// 已解析子模型、工具、技能及压缩配置。
    pub effective_config: SessionEffectiveConfig,
    /// Runtime feature policy enforced for the child scope.
    /// 子范围执行的运行时功能策略。
    pub runtime_policy: ExecutionRuntimePolicy,
    /// Agent selection frozen by the actual child request.
    /// 真实子请求冻结的代理选择。
    pub execution_agent_id: Option<String>,
    /// Exact non-secret connection used by the prepared child model route.
    /// 已准备子模型路由使用的精确非敏感连接标识。
    pub connection_id: String,
    /// Exact request temperature; absence preserves the provider default.
    /// 精确请求温度；缺省时保留供应商默认值。
    pub temperature: Option<f32>,
    /// Callable public tool IDs and their immutable source identities at original preparation.
    /// 原准备阶段可调用的公开工具 ID 及其不可变来源身份。
    pub admitted_tools: std::collections::BTreeMap<String, ChildToolOrigin>,
    /// Actual non-secret session overrides inherited by the child request.
    /// 子请求实际继承的非敏感会话覆盖。
    pub request_overrides: ChildExecutionRequestOverrides,
    /// Fixed parent agent prompt captured for the child, if present.
    /// 为子代理捕获的固定父代理提示词（若存在）。
    pub fixed_agent_prompt: Option<String>,
    /// Exact child-specific prompt fields used by the prompt composer.
    /// 提示词组合器使用的精确子专属提示字段。
    pub prompt_context: ChildExecutionPromptContext,
}

/// Immutable task lookup and presentation identity captured from the original runtime invocation.
/// 从原运行时调用捕获的不可变任务查找及展示身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChildExecutionRuntimeIdentity {
    /// Exact public task name used by subsequent orchestration tools, which may differ from the UUID.
    /// 后续编排工具使用的精确公开任务名称，可与 UUID 不同。
    pub public_task_id: String,
    /// Original generated display title, unaffected by later edits to the session title.
    /// 原生成展示标题，不受后续会话标题编辑影响。
    pub display_title: String,
    /// Original normalized optional task description.
    /// 原始规范化可选任务描述。
    pub description: Option<String>,
    /// Original normalized optional subagent label.
    /// 原始规范化可选子代理标记。
    pub subagent: Option<String>,
    /// Exact nesting depth at admission; the direct child of a root has depth one.
    /// 准入时精确嵌套深度；根的直接子代理深度为一。
    pub depth: u32,
}

/// Serializable counterpart of the child's resolved prompt-composer input.
/// 子代理已解析提示词组合输入的可序列化形式。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChildExecutionPromptContext {
    /// Stable model-visible child identity.
    /// 稳定的模型可见子身份。
    pub identity: String,
    /// Original delegated business task description.
    /// 原委派业务任务描述。
    pub delegated_task: String,
    /// Captured parent delegation guidance.
    /// 已捕获的父级委派指导。
    pub delegation: Option<String>,
    /// Captured functional subagent prompt supplement.
    /// 已捕获的功能型子代理提示补充。
    pub functional_system_prompt: Option<String>,
}

/// Durable source identity independent from collision-sensitive public tool names.
/// 独立于可能因冲突而变化的公开工具名的持久来源身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChildToolOrigin {
    /// Source family declared by the original host registry.
    /// 原宿主注册表声明的来源类别。
    pub source_kind: ChildToolSourceKind,
    /// Exact source-owned registry namespace.
    /// 来源自有注册表的精确命名空间。
    pub namespace: String,
    /// Source tool name before public name normalization.
    /// 公开名称规范化之前的来源工具名。
    pub original_name: String,
}

/// Closed durable counterpart of the host tool manager's six source families.
/// 宿主工具管理器六种来源类别的封闭持久对应类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildToolSourceKind {
    /// Native host tool.
    /// 原生宿主工具。
    Native,
    /// Host-managed protocol tool.
    /// 宿主管理的协议工具。
    VulcanManaged,
    /// Manifest-backed plugin tool.
    /// 清单支持的插件工具。
    VulcanPlugin,
    /// LuaSkills runtime tool.
    /// LuaSkills 运行时工具。
    LuaSkills,
    /// MCP server tool.
    /// MCP 服务工具。
    Mcp,
    /// Host-bound AI capability.
    /// 宿主绑定的 AI 能力。
    AiHostBinding,
}

impl ChildExecutionProfile {
    /// Validates the supported version and exact target identity before persistence or recovery.
    /// 在持久化或恢复前验证支持版本及精确目标身份。
    /// `session_id` and `generation` identify the owner; errors never substitute another profile.
    /// `session_id` 和 `generation` 标识所有者；错误不会替换为另一份配置。
    pub fn validate_owner(&self, session_id: &str, generation: u64) -> Result<(), DomainError> {
        self.request_overrides.validate()?;
        // Public lookup identity cannot be reconstructed from a mutable title or physical UUID.
        // 公开查找身份不能从可变标题或物理 UUID 重建。
        let identity = &self.runtime_identity;
        if identity.public_task_id.trim().is_empty()
            || identity.public_task_id.trim() != identity.public_task_id
            || identity.public_task_id.contains('/')
            || identity.display_title.trim().is_empty()
            || identity.depth == 0
            || (identity.depth == 1) != (self.parent_session_id == self.root_session_id)
            || [&identity.description, &identity.subagent]
                .into_iter()
                .flatten()
                .any(|value| value.trim().is_empty() || value.trim() != value)
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "child_execution_profile.runtime_identity",
            });
        }
        if self.temperature.is_some_and(|value| !value.is_finite()) {
            return Err(DomainError::CollaborationStateViolation {
                field: "child_execution_profile.temperature",
            });
        }
        if self.version != 1 {
            return Err(DomainError::CollaborationStateViolation {
                field: "child_execution_profile.version",
            });
        }
        if self.task_id.as_str() != session_id
            || self.segment_generation != generation
            || generation == 0
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "child_execution_profile.owner",
            });
        }
        for identity in [
            &self.root_session_id,
            &self.parent_session_id,
            &self.business_execution_id,
            &self.connection_id,
        ] {
            if identity.trim().is_empty() || identity.trim() != identity {
                return Err(DomainError::CollaborationStateViolation {
                    field: "child_execution_profile.identity",
                });
            }
        }
        if self.root_session_id == session_id
            || self.parent_session_id == session_id
            || self.prompt_context.identity.trim().is_empty()
            || self.prompt_context.delegated_task.trim().is_empty()
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "child_execution_profile.child_scope",
            });
        }
        Ok(())
    }
}
