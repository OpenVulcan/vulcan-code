//! Serializable prepared child settings before the host allocates a canonical child turn.
//! 宿主分配规范子回合之前，可序列化的子准备设置。

use super::{
    ChildExecutionProfile, ChildExecutionPromptContext, ChildExecutionRequestOverrides,
    ChildExecutionRuntimeIdentity, ChildToolOrigin,
};
use crate::v1::{DomainError, SubagentTaskId, TurnId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Complete non-secret preparation independent of a child document's unallocated coordinates.
/// 完整非敏感准备，独立于子文档尚未分配的坐标。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChildExecutionPreparation {
    /// Exact lookup and presentation fields from the original runtime invocation.
    /// 来自原运行时调用的精确查找及展示字段。
    pub runtime_identity: ChildExecutionRuntimeIdentity,
    /// Original invocation before defaults and expert configuration are resolved.
    /// 解析默认值及专家配置之前的原调用。
    pub invocation_config: crate::TaskInvocationConfig,
    /// Exact resolver output used by this business execution.
    /// 本业务执行实际使用的精确解析器输出。
    pub resolved_config: crate::TaskSessionResolvedConfig,
    /// Child physical identity allocated by the original tool invocation.
    /// 原工具调用分配的子物理身份。
    pub task_id: Uuid,
    /// Original business execution identity checked against the active runtime owner.
    /// 与活动运行时所有者核对的原业务执行身份。
    pub execution_id: String,
    /// Root session owning the embedded storage directory.
    /// 拥有嵌入存储目录的根会话。
    pub root_session_id: Uuid,
    /// Direct parent session, which may itself be embedded.
    /// 直接父会话，其本身也可以是嵌入会话。
    pub parent_session_id: Uuid,
    /// Actual resolved configuration used for this child launch.
    /// 此次子启动实际使用的已解析配置。
    pub effective_config: crate::SessionEffectiveConfig,
    /// Actual runtime feature policy used for this child.
    /// 此子代理实际使用的运行时功能策略。
    pub runtime_policy: crate::ExecutionRuntimePolicy,
    /// Exact agent selection used by the constructed child request.
    /// 构造完成的子请求使用的精确代理选择。
    pub execution_agent_id: Option<String>,
    /// Prepared connection identity without credentials.
    /// 不含凭据的已准备连接身份。
    pub connection_id: String,
    /// Actual request temperature after inheritance and overrides.
    /// 继承与覆盖后的真实请求温度。
    pub temperature: Option<f32>,
    /// Callable public IDs and immutable source identities from the prepared tool snapshot.
    /// 已准备工具快照中的可调用公开 ID 及不可变来源身份。
    pub admitted_tools: BTreeMap<String, ChildToolOrigin>,
    /// Exact non-secret session overrides from the constructed child request.
    /// 来自构造完成子请求的精确非敏感会话覆盖。
    pub request_overrides: ChildExecutionRequestOverrides,
    /// Fixed parent agent prompt already captured by request construction.
    /// 请求构造阶段已捕获的固定父代理提示词。
    pub fixed_agent_prompt: Option<String>,
    /// Exact child-specific inputs to the prompt composer.
    /// 提示词组合器的精确子专属输入。
    pub prompt_context: ChildExecutionPromptContext,
}

impl ChildExecutionPreparation {
    /// Binds these prepared settings to the host's actual child generation and business turn.
    /// 将这些已准备设置绑定到宿主实际子代次及业务回合。
    /// `segment_generation` and `business_turn_id` must come from the canonical child document.
    /// `segment_generation` 和 `business_turn_id` 必须来自规范子文档。
    /// Returns the validated profile without resolving defaults or modifying a document.
    /// 返回经过校验的配置，不解析默认值或修改文档。
    pub fn bind_child_turn(
        self,
        segment_generation: u64,
        business_turn_id: TurnId,
    ) -> Result<ChildExecutionProfile, DomainError> {
        // Keep the allocated physical identity independent of mutable display or prompt fields.
        // 将已分配物理身份与可变展示或提示字段区分。
        let session_id = self.task_id.to_string();
        // Binding only supplies document-owned coordinates; every executable setting is moved unchanged.
        // 绑定仅提供文档自有坐标；每个可执行设置均原样移入。
        let profile = ChildExecutionProfile {
            version: 1,
            root_session_id: self.root_session_id.to_string(),
            parent_session_id: self.parent_session_id.to_string(),
            task_id: SubagentTaskId::try_new(session_id.clone())?,
            segment_generation,
            business_turn_id,
            business_execution_id: self.execution_id,
            runtime_identity: self.runtime_identity,
            invocation_config: self.invocation_config,
            resolved_config: self.resolved_config,
            effective_config: self.effective_config,
            runtime_policy: self.runtime_policy,
            execution_agent_id: self.execution_agent_id,
            connection_id: self.connection_id,
            temperature: self.temperature,
            admitted_tools: self.admitted_tools,
            request_overrides: self.request_overrides,
            fixed_agent_prompt: self.fixed_agent_prompt,
            prompt_context: self.prompt_context,
        };
        profile.validate_owner(&session_id, segment_generation)?;
        Ok(profile)
    }
}
