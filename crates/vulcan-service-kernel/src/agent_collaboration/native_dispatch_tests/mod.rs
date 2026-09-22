//! Port-level dispatch fixtures for every native collaboration command arm.
//! 覆盖每条原生协作命令分支的端口级分发夹具。

use super::*;
use crate::agent_collaboration::test_support::fixture;

mod cases;
mod wake;

/// One admitted source API plus the exact ToolCall that a native port dispatch is bound to.
/// 一条已准入来源 API，以及原生端口分发所绑定的精确工具调用。
pub(super) struct NativeDispatch {
    /// Isolated host root retained for the whole test.
    /// 在整个测试期间保留的隔离宿主根。
    pub(super) _root: crate::tests::test_support::TestHostRuntimeRoot,
    /// Real service invocation whose source owns the admitted ToolCall.
    /// 真实服务调用；其来源持有已准入工具调用。
    pub(super) invocation: ValidatedInvocation,
    /// Authorized same-project target session.
    /// 同项目获授权目标会话。
    pub(super) target: Arc<SessionHandle>,
    /// Tool identity this fixture's context was admitted for.
    /// 本夹具上下文获准时使用的工具身份。
    tool_id: String,
    /// Exact ToolCall context validated by the concrete port method.
    /// 由具体端口方法校验的精确工具调用上下文。
    context: VulcanToolInvocationContext,
}

impl NativeDispatch {
    /// Returns the exact durable operation identity derived for the current tool call.
    /// 返回当前工具调用派生的精确可靠操作身份。
    pub(super) fn operation_id(&self) -> String {
        stable_operation_id(&self.context, &self.tool_id)
    }

    /// Returns the canonical source handle that owns the question or message authority.
    /// 返回持有问题或消息权威的规范来源句柄。
    pub(super) fn source(&self) -> &Arc<SessionHandle> {
        &self.invocation.source_handle
    }

    /// Returns the exact admitted ToolCall context bound to this fixture.
    /// 返回绑定到本夹具的精确已准入工具调用上下文。
    pub(super) fn context(&self) -> VulcanToolInvocationContext {
        self.context.clone()
    }

    /// Dispatches one typed command through the real native collaboration runtime.
    /// 通过真实原生协作运行时调度一条类型化命令。
    pub(super) async fn invoke(&self, command: AgentCollaborationCommand) -> Result<Value, String> {
        session_v1_native_agent_collaboration_runtime(
            self.invocation.state.clone(),
            crate::ServiceEventEmitter::new(vec![]),
        )
        .invoke(self.context.clone(), command)
        .await
    }

    /// Rebinds this fixture to a different real ToolCall for an independent command identity.
    /// 将该夹具重新绑定到另一条真实工具调用，以获得独立命令身份。
    pub(super) async fn retool(self, tool_id: &'static str) -> Self {
        // The previous command left a durable active API; interrupt it so a new request boundary opens.
        // 上一条命令留下了可靠活动 API；先中断它，才能开启新的请求边界。
        self.invocation
            .source_handle
            .mutate_durable(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::MetadataChanged,
                |document| {
                    document.apply_atomic_transition(|document| {
                        document.interrupt_active_api(10)?;
                        document.set_collaboration_run_control(
                            vulcan_session_core::v1::collaboration::SetRunControlInput {
                                operation_id: CollaborationOperationId::try_new(
                                    "native-dispatch-running",
                                )?,
                                state: CollaborationRunControl::Running,
                                changed_at_ms: 10,
                            },
                        )?;
                        Ok(())
                    })
                },
            )
            .unwrap();
        let (context, tool_id) = admit_source_tool_call(&self.invocation, tool_id, 20).await;
        let mut invocation = self.invocation;
        invocation.source_api_call_id = Some(context.api_call_id);
        invocation.operation_id = stable_operation_id(&context, tool_id);
        invocation.source_tool_id = tool_id.to_owned();
        Self {
            _root: self._root,
            invocation,
            target: self.target,
            tool_id: tool_id.to_owned(),
            context,
        }
    }
}

/// Admits one real source ToolCall for `tool_id` on the invocation's open business turn.
/// 在调用的开放业务回合上为 `tool_id` 准入一条真实来源工具调用。
/// Returns the exact context plus the tool identity it was admitted for.
/// 返回精确上下文以及其获准的工具身份。
async fn admit_source_tool_call(
    invocation: &ValidatedInvocation,
    tool_id: &'static str,
    now_ms: i64,
) -> (VulcanToolInvocationContext, &'static str) {
    let input = invocation
        .source_handle
        .read(|document| {
            let through_seq = document.events().last().unwrap().seq;
            vulcan_session_core::v1::document::PrepareAndStartApiInput {
                purpose: ApiPurposeKind::MainGeneration,
                request: vulcan_session_core::v1::event::ApiRequestSnapshot::new("openai", "model"),
                request_budget_evidence:
                    crate::session_runtime_registry_v1::test_provider_request_budget_evidence(),
                projection: vulcan_session_core::v1::event::ProviderProjectionProof {
                    segment_generation: document.segment_generation(),
                    document_revision: document.memory_revision(),
                    retained_from_seq: Seq::try_new(1).unwrap(),
                    through_seq,
                    fingerprint_sha256:
                        vulcan_session_core::v1::projection::provider_visible_fingerprint(
                            document.events(),
                            through_seq,
                        )
                        .unwrap(),
                },
                locked_baseline: vulcan_session_core::v1::context::LockedBaselineProjection::empty(
                ),
                continuation_action_id: None,
                now_ms,
            }
        })
        .unwrap();
    let api_call_id = invocation
        .source_handle
        .prepare_and_start_api_durable(input)
        .unwrap()
        .value
        .api_call_id;
    let request_stream_id = vulcan_session_core::v1::ids::RequestStreamId::try_new(format!(
        "native-dispatch-{tool_id}"
    ))
    .unwrap();
    let tool_call_id = invocation
        .source_handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ToolBoundary,
            |document| {
                document.prepare_tool(
                    api_call_id,
                    request_stream_id.clone(),
                    tool_id,
                    vulcan_session_core::v1::event::ToolPayload::empty(),
                    None,
                    now_ms,
                )
            },
        )
        .unwrap()
        .value;
    let tool_seq = invocation
        .source_handle
        .read(|document| {
            document
                .events()
                .iter()
                .find_map(|event| match &event.event {
                    EventBody::ToolCall(tool) if tool.tool_call_id == tool_call_id => {
                        Some(event.seq)
                    }
                    _ => None,
                })
                .unwrap()
        })
        .unwrap();
    (
        VulcanToolInvocationContext {
            session_id: invocation.source_session_id.to_string(),
            request_stream_id,
            api_call_id,
            tool_call_id,
            tool_seq,
            controller_tool_call_id: format!("native-dispatch-{tool_id}"),
            provider_tool_call_id: None,
        },
        tool_id,
    )
}

/// Creates two indexed sessions in one isolated database and admits one real source ToolCall.
/// 在隔离数据库中创建两个索引会话，并准入一条真实来源工具调用。
/// `cross_project` selects the target project scope; `tool_id` selects the admitted tool.
/// `cross_project` 决定目标项目范围；`tool_id` 决定准入的工具。
pub(super) async fn native_dispatch_fixture(
    cross_project: bool,
    tool_id: &'static str,
) -> NativeDispatch {
    let (root, invocation, target) = fixture(cross_project).await;
    invocation.source_handle.initialize_root_usage(true, 1);
    target.initialize_root_usage(true, 1);
    // The fixture owns the exact canonical handles; the root input opens the reviewed business turn.
    // 夹具持有精确规范句柄；根输入开启被测业务回合。
    let source_turn = invocation
        .source_handle
        .submit_root_input(
            vulcan_session_core::v1::document::ClientInputSubmission::new(
                vulcan_session_core::v1::ids::ClientActionId::try_new("native-dispatch-root")
                    .unwrap(),
                "Coordinate with another session",
                vec![],
            ),
            2,
        )
        .unwrap()
        .value
        .value();
    let mut invocation = invocation;
    let (context, tool_id) = admit_source_tool_call(&invocation, tool_id, 3).await;
    invocation.source_turn_id = source_turn;
    invocation.source_api_call_id = Some(context.api_call_id);
    invocation.operation_id = stable_operation_id(&context, tool_id);
    invocation.source_tool_id = tool_id.to_owned();
    NativeDispatch {
        _root: root,
        invocation,
        target,
        tool_id: tool_id.to_owned(),
        context,
    }
}
