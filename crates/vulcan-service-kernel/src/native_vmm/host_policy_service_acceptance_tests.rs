//! Actual policy service requests for the real queued-analysis acceptance fixture.
//! 为真实队列分析验收夹具提供实际策略服务请求。

use super::*;
use vulcan_service_api::{ServiceEventEmitter, VmmMemoryActionRequest, VmmMemoryActionResponse};

/// Builds isolated service state around the supplied real runtime, storage and shared operation gate.
/// 围绕传入的真实运行时、存储及共享操作锁构建隔离服务状态。
/// Returns the normal management dispatch state without starting unrelated background workers.
/// 返回普通管理派发状态，不启动无关后台工作器。
pub(super) fn service(
    runtime: Arc<NativeVmmCoreRuntime>,
    data_plane: VmmDataPlaneHandle,
    operation_gate: Arc<RwLock<()>>,
) -> crate::state::SharedServiceBridgeState {
    // The dispatcher reads its backend from the actual agent configuration.
    // 派发器从实际智能体配置读取后端模式。
    let mut agent = crate::tests::test_support::test_agent_runtime_bridge();
    agent.provider_configuration.vmm.enabled = Some(true);
    agent.provider_configuration.vmm.backend_mode = Some(vulcan_vmm::VmmBackendMode::Native);
    // This service owns no user session or production storage destination.
    // 本服务不持有用户会话或生产存储目标。
    let mut state =
        crate::tests::test_support::test_service_state_with_parent_session(uuid::Uuid::new_v4());
    state.agent_runtime = Some(agent);
    state.native_vmm_maintenance = Some(crate::native_vmm::NativeVmmMaintenance::new(
        runtime,
        data_plane,
        operation_gate,
    ));
    Arc::new(StdMutex::new(state))
}

/// Executes an exact action and JSON payload through the public management service on isolated state.
/// 在隔离状态上，通过公开管理服务执行准确动作及 JSON 参数。
/// Returns the production typed response or rejection, without substituting either result.
/// 返回生产类型化响应或拒绝，不替换任何结果。
pub(super) async fn action(
    state: &crate::state::SharedServiceBridgeState,
    name: &str,
    payload: serde_json::Value,
) -> Result<VmmMemoryActionResponse, String> {
    // Use the same independent task boundary as the existing real lifecycle service acceptance.
    // 使用与既有真实生命周期服务验收相同的独立任务边界。
    let result = tokio::spawn(crate::service_run_vmm_memory_action(
        ServiceEventEmitter::new(Vec::new()),
        state.clone(),
        VmmMemoryActionRequest {
            command_id: uuid::Uuid::new_v4().to_string(),
            action: name.to_owned(),
            scope: "global".to_owned(),
            project_id: None,
            conversation_id: None,
            payload,
        },
    ))
    .await
    .expect("policy service task");
    result.map(|result| result.data)
}

/// Rejects malformed and unbound edits through the real service while preserving both saved documents.
/// 通过真实服务拒绝非法及未绑定编辑，同时保留两份已保存文档。
/// Takes the isolated service, storage root and valid recall template; returns after byte-exact checks.
/// 接收隔离服务、存储根及有效召回模板；完成逐字节检查后返回。
pub(super) async fn verify_rejections(
    state: &crate::state::SharedServiceBridgeState,
    data_plane: &VmmDataPlaneHandle,
    recall: &vulcan_service_api::VmmNativeRecallPolicySummary,
) {
    // These are the sole paths declared by runtime_policy and host_project_policy persistence.
    // 这些是运行时策略与宿主项目策略持久化声明的唯一路径。
    let paths = [
        data_plane.data_root.join("config/runtime-policy.json"),
        data_plane
            .data_root
            .join("config/host-project-policies.json"),
    ];
    // Retain exact on-disk bytes rather than only the currently effective runtime values.
    // 保留准确磁盘字节，而不只保存当前运行时有效值。
    let before = paths
        .each_ref()
        .map(|path| std::fs::read(path).expect("saved policy"));
    // A valid policy still requires an existing exact host binding at the maintenance boundary.
    // 即使策略有效，维护边界仍要求存在准确宿主绑定。
    let unbound = action(
        state,
        "native_host_project_policy_update",
        serde_json::json!({
            "hostProjectId": "queue-host-unbound",
            "overrides": {"recall": recall}
        }),
    )
    .await
    .expect_err("unbound caller must be rejected");
    assert!(unbound.contains("not bound"), "{unbound}");
    // Unknown ownership fields must fail strict decoding instead of being ignored.
    // 未知归属字段必须被严格解码拒绝，不能被忽略。
    let forbidden = action(
        state,
        "native_host_project_policy_update",
        serde_json::json!({
            "hostProjectId": "queue-host-a",
            "overrides": {"recall": recall, "userId": 2}
        }),
    )
    .await
    .expect_err("forbidden ownership must be rejected");
    assert!(forbidden.contains("userId"), "{forbidden}");
    // An invalid documented numeric value reaches business validation, not unknown-field rejection.
    // 非法已定义数值进入业务校验，而不是未知字段拒绝。
    let mut invalid = recall.clone();
    invalid.postaction_max_input_tokens = 0;
    assert!(
        action(
            state,
            "native_runtime_policy_update",
            serde_json::to_value(invalid).expect("invalid policy JSON")
        )
        .await
        .is_err()
    );
    assert_eq!(
        paths
            .each_ref()
            .map(|path| std::fs::read(path).expect("unchanged policy")),
        before
    );
}
