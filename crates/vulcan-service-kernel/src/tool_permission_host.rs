//! Service-kernel side of the tool permission gate.
//! 工具权限闸门的服务内核侧实现。

use std::sync::{Arc, Mutex, Weak};

use serde_json::Value;
use vulcan_agent_runtime::agent_runtime::question_runtime::{QuestionRuntime, ToolApprovalPrompt};
use vulcan_agent_runtime::agent_runtime::tool_permission::{
    JudgeCall, JudgeLabel, LlmReview, ReviewCall, ToolPermissionHost,
};
use vulcan_agent_runtime::model::SessionId;
use vulcan_session_core::v1::config::PermissionMode;
use vulcan_tools_manager::VulcanToolInvocationContext;

use crate::ServiceBridgeState;
use crate::session_runtime_registry_v1::SessionRuntimeRegistry;

/// Reads each session's live permission mode from its registered V1 document and asks the user
/// through the shared question runtime.
/// 从已注册的 V1 文档读取每个会话的实时权限模式，并通过共享问题运行时询问用户。
pub(crate) struct KernelToolPermissionHost {
    /// Process session registry; every session that runs tools is registered here.
    /// 进程会话注册表；所有运行工具的会话都在此注册。
    registry: Arc<SessionRuntimeRegistry>,
    /// Shared question runtime whose Ask panel presents approvals.
    /// 以 Ask 面板展示审批的共享问题运行时。
    question_runtime: Arc<QuestionRuntime>,
    /// Service state holding the judge settings and worker; weak, because the state owns the
    /// runtime that owns this host.
    /// 持有判定设置与 worker 的服务状态；使用弱引用，因为该状态拥有持有本宿主的运行时。
    state: Weak<Mutex<ServiceBridgeState>>,
}

impl KernelToolPermissionHost {
    /// Builds the host over the process session registry and question runtime.
    /// 基于进程会话注册表与问题运行时构建宿主。
    pub(crate) fn new(
        registry: Arc<SessionRuntimeRegistry>,
        question_runtime: Arc<QuestionRuntime>,
        state: Weak<Mutex<ServiceBridgeState>>,
    ) -> Self {
        Self {
            registry,
            question_runtime,
            state,
        }
    }
}

#[async_trait::async_trait]
impl ToolPermissionHost for KernelToolPermissionHost {
    /// Child agent sessions follow their root user session, so a child can never run with more
    /// permission than the session the user controls; a missing root fails closed.
    /// 子代理会话跟随其根用户会话，因此子会话的权限绝不会超过用户控制的会话；根会话缺失时失败即关闭。
    fn permission_mode(&self, session_id: SessionId) -> Result<PermissionMode, String> {
        let (own_mode, root) = self.read(session_id, |document| {
            (
                document.metadata.mode.access.permission,
                document
                    .metadata
                    .child_execution_profile
                    .as_ref()
                    .map(|profile| profile.root_session_id.clone()),
            )
        })?;
        let Some(root) = root else {
            return Ok(own_mode);
        };
        let root_id = SessionId::parse_str(&root)
            .map_err(|error| format!("invalid root session `{root}`: {error}"))?;
        self.read(root_id, |document| document.metadata.mode.access.permission)
    }

    async fn request_approval(
        &self,
        session_id: SessionId,
        context: VulcanToolInvocationContext,
        tool_name: String,
        arguments: Value,
        prompt: ToolApprovalPrompt,
    ) -> Result<bool, String> {
        self.question_runtime
            .request_approval(session_id, context, tool_name, arguments, prompt)
            .await
    }

    async fn judge(&self, call: JudgeCall) -> Result<Option<JudgeLabel>, String> {
        let state = self
            .state
            .upgrade()
            .ok_or_else(|| String::from("The service is shutting down."))?;
        crate::permission_judge::judge_call(&state, call).await
    }

    async fn review(&self, call: ReviewCall) -> Result<Option<LlmReview>, String> {
        let state = self
            .state
            .upgrade()
            .ok_or_else(|| String::from("The service is shutting down."))?;
        crate::permission_judge::review_call(&state, call).await
    }
}

impl KernelToolPermissionHost {
    /// Reads one value from a registered session document.
    /// 从已注册会话文档读取一个值。
    fn read<T>(
        &self,
        session_id: SessionId,
        reader: impl FnOnce(&vulcan_session_core::v1::document::SessionDocument) -> T,
    ) -> Result<T, String> {
        crate::session_goal_lifecycle::registered_session_v1_handle(&self.registry, session_id)?
            .read(reader)
            .map_err(|error| error.to_string())
    }
}
