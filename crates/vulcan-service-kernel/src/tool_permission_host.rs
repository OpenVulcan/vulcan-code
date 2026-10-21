//! Service-kernel side of the tool permission gate.
//! 工具权限闸门的服务内核侧实现。

use std::sync::{Arc, Mutex, Weak};

use serde_json::Value;
use vulcan_agent_runtime::agent_runtime::question_runtime::{
    ApprovalAnswer, QuestionRuntime, ToolApprovalPrompt,
};
use vulcan_agent_runtime::agent_runtime::tool_permission::{
    JudgeCall, JudgeFailure, JudgeVerdict, LlmReview, PermissionMemory, PermissionRecord,
    ReviewCall, ToolPermissionHost,
};
use vulcan_agent_runtime::model::SessionId;
use vulcan_session_core::v1::config::AccessConfig;
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
    /// Declared intents and repeated unattended denials, per session, in memory only.
    /// 按会话保存、仅在内存中的已声明意图与无人值守重复拒绝。
    memory: PermissionMemory,
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
            memory: PermissionMemory::default(),
        }
    }
}

#[async_trait::async_trait]
impl ToolPermissionHost for KernelToolPermissionHost {
    /// Child agent sessions follow their root user session, including its unattended mode, so a
    /// child can never run with more permission than the session the user controls, nor ask a
    /// user who is not there; a missing root fails closed.
    /// 子代理会话跟随其根用户会话（包括其无人值守模式），因此子会话的权限绝不会超过用户控制的会话，
    /// 也不会询问不在场的用户；根会话缺失时失败即关闭。
    fn access(&self, session_id: SessionId) -> Result<AccessConfig, String> {
        let root_id = self.root_of(session_id)?;
        self.read(root_id, |document| document.metadata.mode.access.clone())
    }

    fn memory(&self) -> Option<&PermissionMemory> {
        Some(&self.memory)
    }

    async fn request_approval(
        &self,
        session_id: SessionId,
        context: VulcanToolInvocationContext,
        tool_name: String,
        arguments: Value,
        prompt: ToolApprovalPrompt,
    ) -> Result<ApprovalAnswer, String> {
        let state = self
            .state
            .upgrade()
            .ok_or_else(|| String::from("The service is shutting down."))?;
        let timeout = crate::permission_judge::approval_timeout(&state)?;
        self.question_runtime
            .request_approval(session_id, context, tool_name, arguments, prompt, timeout)
            .await
    }

    async fn judge(&self, call: JudgeCall) -> Result<Option<JudgeVerdict>, JudgeFailure> {
        let state = self.state.upgrade().ok_or_else(|| JudgeFailure {
            reason: String::from("The service is shutting down."),
            trace: None,
        })?;
        crate::permission_judge::judge_call(&state, call).await
    }

    async fn review(&self, call: ReviewCall) -> Result<Option<LlmReview>, String> {
        let state = self
            .state
            .upgrade()
            .ok_or_else(|| String::from("The service is shutting down."))?;
        // Child sessions read their root session's own prompt, like its access.
        // 子会话像读取访问设置一样读取其根会话的自有提示词。
        let root_id = self.root_of(call.session_id)?;
        let handle =
            crate::session_goal_lifecycle::registered_session_v1_handle(&self.registry, root_id)?;
        let session_prompt = crate::permission_judge::session_review_prompt(handle.as_ref())?;
        crate::permission_judge::review_call(&state, call, session_prompt).await
    }

    fn record(&self, record: PermissionRecord) {
        if let Some(state) = self.state.upgrade() {
            crate::permission_judge::record_decision(&state, record);
        }
    }
}

impl KernelToolPermissionHost {
    /// Returns the root user session of a child agent session, or the session itself.
    /// 返回子代理会话的根用户会话；非子会话返回其自身。
    fn root_of(&self, session_id: SessionId) -> Result<SessionId, String> {
        let root = self.read(session_id, |document| {
            document
                .metadata
                .child_execution_profile
                .as_ref()
                .map(|profile| profile.root_session_id.clone())
        })?;
        match root {
            Some(root) => SessionId::parse_str(&root)
                .map_err(|error| format!("invalid root session `{root}`: {error}")),
            None => Ok(session_id),
        }
    }

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
