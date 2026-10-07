//! Real service generations drive production automatic prewarm triggers and original native receipts.
//! 真实服务代际驱动生产自动预热触发及原原生回执。

use super::*;
use crate::plugin_runtime_services::automatic_prewarm::spawn_for_turn;
use crate::tests::test_support::TestHostRuntimeRoot;

impl Fixture {
    /// Publish an exact visible project for owner with a workspace distinct from session storage.
    /// 为 owner 发布精确可见项目，其工作区与会话存储分离。
    /// Returns the path ordinary plugin tools must receive without loading any session runtime.
    /// 返回普通插件工具必须收到的路径，不加载任何会话运行时。
    fn attach_workspace(&self, owner: uuid::Uuid) -> std::path::PathBuf {
        // Separate physical project files from both native runtime and session persistence.
        // 将物理项目文件与原生运行时及会话持久化分离。
        let path = self.root.path().join("execution-workspace");
        std::fs::create_dir_all(&path).expect("execution workspace");
        // Use the same canonical session fixture shape as the service, changing only project ownership.
        // 使用与服务相同的规范会话夹具形状，仅改变项目归属。
        let mut workspace = test_service_state_with_parent_session(owner)
            .workspace_snapshot
            .take()
            .expect("retained workspace fixture");
        workspace
            .sessions
            .iter_mut()
            .find(|session| session.id == owner)
            .expect("fixture owner identity changed")
            .project_id = Some("v1-test-project".into());
        workspace.task_projects.push(crate::TaskProjectSummary::new(
            "v1-test-project",
            "Prewarm project",
            path.to_string_lossy().as_ref(),
            false,
            0,
            None,
            None,
            Vec::new(),
        ));
        self.state.lock().expect("service state").workspace_snapshot = Some(workspace);
        path
    }

    /// Enable one actual reusable package; return its current lossless generation token.
    /// 启用单个真实复用包；返回其当前无损代次令牌。
    fn enable_prewarm(&self) -> String {
        // Real execution requires the repository's authoritative bridge, unlike metadata-only capacity tests.
        // 与仅元数据容量测试不同，真实执行需要仓库权威桥接。
        let bridge = self.root.path().join("bridge");
        std::fs::create_dir_all(bridge.join("runtime")).expect("bridge runtime");
        std::fs::write(bridge.join("dependencies.yaml"), "{}\n").expect("bridge dependencies");
        std::fs::write(
            bridge.join("runtime/bridge.lua"),
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../plugin-bridge/vulcan-plugin-bridge/runtime/bridge.lua"
            )),
        )
        .expect("authoritative bridge source");
        // This installed fixture package remains the sole source of declarations and Lua.
        // 此已安装夹具包仍是声明及 Lua 的唯一来源。
        let package = self.root.path().join("system/control-plugin");
        std::fs::write(
            package.join("parameters.json"),
            r#"{"type":"object","properties":{}}"#,
        )
        .expect("tool schema");
        std::fs::write(
            package.join("runtime/plugin.lua"),
            r#"
-- Keep exports private to each real module instance.
-- 将导出保留在每个真实模块实例内。
local M, calls = {}, 0
-- Count business calls on this original VM and return the version-one result frame.
-- 在此原始 VM 上计数业务调用，并返回版本一结果帧。
function M.dispatch(_envelope)
    calls = calls + 1
    return vulcan.json.encode({ok=true,result={count=calls}})
end
-- Accept a later ordinary call envelope; return successful preparation.
-- 接受后续普通调用信封；返回准备成功。
function M.before_call(_envelope) return true end
-- Accept original envelope and result state; return successful hygiene.
-- 接受原信封及结果状态；返回清理成功。
function M.after_call(_envelope, _succeeded) return true end
-- Report that this private module has no retained request state.
-- 报告此私有模块没有保留请求状态。
function M.reuse_check() return true end
-- Close the original envelope without executing any business operation.
-- 关闭原信封，不执行任何业务操作。
function M.shutdown(_envelope) return '{"ok":true,"result":{}}' end
return M
"#,
        )
        .expect("real reusable module");
        std::fs::write(
            package.join("plugin.yaml"),
            r#"schema_version: 2
id: control-plugin
version: 1.0.0
default_enabled: true
display:
  name: Control fixture
  capability: Explicit native prewarm
execution:
  dependencies:
    manifest: dependencies.yaml
    install_mode: bundled
runtime_requirements:
  protocol_version: 1
  backend: in_process
  core_capabilities: [capacity_groups_v1]
  capacities: []
lua_runtime:
  bundle: runtime/plugin.lua
  invoke_function: dispatch
  shutdown_function: shutdown
  max_active_sessions: 1
  tools:
    - id: control_ping
      group: control_tools
      description: Execute the explicit control fixture.
      parameters_file: parameters.json
      execution:
        kind: stateless
        operation: ping
        timeout_ms: 10000
        reusable:
          before_call: before_call
          after_call: after_call
          reuse_check: reuse_check
          ordering: key_serial
          max_resident_vms: 2
          max_running_calls: 1
          idle_ttl_ms: 60000
          max_uses: 100
"#,
        )
        .expect("reusable manifest");
        self.runtime
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("enable tool");
        // Use the exact enabled descriptor; changed fixture declarations must fail with catalog diagnostics.
        // 使用精确已启用描述符；夹具声明变化必须携带目录诊断失败。
        let snapshot = self.runtime.snapshot();
        snapshot
            .enabled_tools
            .get("control_ping")
            .unwrap_or_else(|| {
                panic!(
                    "control tool missing; fixture declaration may have changed: {:?}",
                    snapshot.plugins["control-plugin"].diagnostics
                )
            })
            .plugin_generation
            .to_string()
    }

    /// Poll only the original automatic receipt until actual retirement within one test deadline.
    /// 在单个测试截止时间内仅轮询原自动回执，直到真实退役。
    /// Returns retained successful or failed evidence without acknowledging it.
    /// 返回保留成功或失败证据，不确认回执。
    async fn wait_prewarm(
        &self,
        owner: uuid::Uuid,
        invocation_id: &str,
    ) -> vulcan_plugin_runtime::VulcanPluginFormalCallStatus {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                // Never replace an unknown receipt with another preparation.
                // 绝不将未知回执替换为其他准备。
                let status = self
                    .runtime
                    .formal_call_status(invocation_id, &owner.to_string())
                    .expect("original status");
                if status.retired {
                    return status;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("prewarm cleanup reached its test deadline")
    }

    /// Publish one exact tool rule through the production RPC; return no execution authority.
    /// 通过生产 RPC 发布单个精确工具规则；不返回执行权威。
    async fn automatic_rule(&self) {
        let status = self
            .rpc("service_plugin_prewarm_policy_status", json!({}))
            .await
            .expect("policy read");
        let receipt = self.rpc("service_begin_plugin_prewarm_policy_update", json!({"request": {
            "updateId": uuid::Uuid::new_v4().to_string(), "runtimeId": status["runtimeId"],
            "expectedRevision": status["revision"], "rules": [{"pluginId":"control-plugin", "toolId":"control_ping", "targetIdle":1}]
        }})).await.expect("explicit rule");
        self.rpc(
            "service_continue_plugin_pending_operation",
            json!({"kind": "prewarmPolicy", "operationId": receipt["request"]["updateId"], "action": "forget"}),
        )
        .await
        .expect("acknowledge policy");
    }

    /// Start a real authoritative kernel turn; return its exact current preparation generation.
    /// 启动真实权威内核回合；返回其精确当前准备代际。
    fn accept_turn(&self, owner: uuid::Uuid, turn: &str) -> u64 {
        let mut state = self.state.lock().expect("service state");
        crate::start_webui_session_kernel_user_turn(&mut state, owner, turn)
            .expect("accepted turn");
        crate::current_webui_prompt_preparation_generation(&state, owner, turn)
            .expect("exact preparation generation")
    }

    /// Run the production admission and await only metadata submission, then return the original invocation.
    /// 运行生产入场并仅等待元数据提交，随后返回原调用标识。
    async fn automatic_target(&self, owner: uuid::Uuid, turn: &str, generation: u64) -> String {
        spawn_for_turn(&self.state, self.runtime.clone(), owner, turn, generation)
            .expect("automatic trigger")
            .expect("first trigger task")
            .await
            .expect("batch observer");
        let targets = self
            .runtime
            .formal_calls(&owner.to_string())
            .expect("original receipts");
        targets
            .into_iter()
            .find(|call| call.prewarm.is_some())
            .expect("automatic target must exist; check real manifest and accepted policy")
            .invocation_id
    }
}

/// An accepted turn warms the ordinary workspace once, and receipt forgetting cannot re-arm that turn.
/// 已接纳回合对普通工作区只预热一次，遗忘回执不能重新激活此回合。
#[tokio::test]
async fn plugin_automatic_turn_reuses_workspace_and_survives_receipt_forgetting() {
    let fixture = Fixture::new();
    let owner = uuid::Uuid::new_v4();
    let workspace = fixture.attach_workspace(owner);
    let storage = TestHostRuntimeRoot::new(owner);
    fixture.state.lock().expect("state").controller_runtime =
        Some(storage.attached_host_runtime_handle().await);
    fixture.enable_prewarm();
    fixture.automatic_rule().await;
    assert!(
        spawn_for_turn(&fixture.state, fixture.runtime.clone(), owner, "first", 0)
            .expect("unaccepted trigger ignored")
            .is_none()
    );
    let generation = fixture.accept_turn(owner, "first");
    let invocation = fixture.automatic_target(owner, "first", generation).await;
    let completed = fixture.wait_prewarm(owner, &invocation).await;
    assert_eq!(completed.error, None, "{completed:?}");
    let target = completed.prewarm.as_ref().expect("prewarm target");
    assert_eq!(target.request.workspace_root, workspace);
    assert_eq!(target.satisfied.as_ref().expect("satisfied").ready, 1);
    assert!(
        spawn_for_turn(
            &fixture.state,
            fixture.runtime.clone(),
            owner,
            "first",
            generation
        )
        .expect("duplicate")
        .is_none()
    );
    fixture
        .runtime
        .forget_formal_call(&invocation, &owner.to_string())
        .expect("explicit original acknowledgement");
    assert!(
        spawn_for_turn(
            &fixture.state,
            fixture.runtime.clone(),
            owner,
            "first",
            generation
        )
        .expect("forgotten original cannot rearm turn")
        .is_none()
    );
    assert!(
        fixture
            .runtime
            .formal_calls(&owner.to_string())
            .expect("no replay")
            .is_empty()
    );
    let business = fixture
        .runtime
        .invoke(vulcan_plugin_runtime::VulcanPluginInvocationRequest {
            tool_id: "control_ping".into(),
            arguments: json!({}),
            owner_session_id: owner.to_string(),
            workspace_root: workspace,
        })
        .await
        .expect("ordinary same-domain business");
    assert_eq!(
        business["count"], 1,
        "initialization cannot run the business export"
    );
    assert_eq!(
        fixture
            .runtime
            .reusable_tool_domains(&owner.to_string())
            .expect("same domain")
            .len(),
        1
    );
    assert_eq!(
        fixture
            .runtime
            .formal_runtime_status()
            .expect("physical state")
            .expect("parent")
            .resources
            .resident,
        1
    );
    fixture.close().await;
}

/// Stop closes queued admission before its worker can create a receipt, and a later generation may warm normally.
/// 停止在工作者创建回执前关闭排队入场，后续代际可以正常预热。
#[tokio::test]
async fn plugin_automatic_stopped_batch_cannot_initialize_or_replay_into_new_generation() {
    let fixture = Fixture::new();
    let owner = uuid::Uuid::new_v4();
    fixture.attach_workspace(owner);
    fixture.enable_prewarm();
    fixture.automatic_rule().await;
    let original = fixture.accept_turn(owner, "same-logical-turn");
    // The current-thread runtime cannot poll this task before the synchronous stop below.
    // 当前线程运行时不能在下方同步停止前轮询此任务。
    let queued = spawn_for_turn(
        &fixture.state,
        fixture.runtime.clone(),
        owner,
        "same-logical-turn",
        original,
    )
    .expect("queue original")
    .expect("task");
    crate::stop_webui_session_kernel_generation(&mut fixture.state.lock().expect("state"), owner);
    queued.await.expect("stopped batch observer");
    assert!(
        fixture
            .runtime
            .formal_calls(&owner.to_string())
            .expect("no late registration")
            .is_empty()
    );
    assert_eq!(
        fixture
            .runtime
            .formal_runtime_status()
            .expect("physical")
            .expect("parent")
            .resources
            .resident,
        0
    );
    let replacement = fixture.accept_turn(owner, "same-logical-turn");
    assert_ne!(
        replacement, original,
        "ABA restart must change the authoritative generation"
    );
    assert!(
        spawn_for_turn(
            &fixture.state,
            fixture.runtime.clone(),
            owner,
            "same-logical-turn",
            original
        )
        .expect("old generation ignored")
        .is_none()
    );
    let invocation = fixture
        .automatic_target(owner, "same-logical-turn", replacement)
        .await;
    let completed = fixture.wait_prewarm(owner, &invocation).await;
    assert!(completed.value.is_some(), "{completed:?}");
    fixture.close().await;
}

/// Lifecycle drain closes an already accepted queued trigger before an empty registry can be declared drained.
/// 生命周期排空在空注册表可被判为已排空之前，关闭已接纳排队触发。
#[tokio::test]
async fn plugin_automatic_drain_fences_queued_trigger_before_owner_removal() {
    let fixture = Fixture::new();
    let owner = uuid::Uuid::new_v4();
    fixture.attach_workspace(owner);
    fixture.enable_prewarm();
    fixture.automatic_rule().await;
    let generation = fixture.accept_turn(owner, "draining");
    let queued = spawn_for_turn(
        &fixture.state,
        fixture.runtime.clone(),
        owner,
        "draining",
        generation,
    )
    .expect("queue original")
    .expect("task");
    let failures = crate::drain_vulcan_plugin_sessions_for_owners(&fixture.state, &[owner]).await;
    assert!(failures.is_empty(), "{failures:?}");
    queued.await.expect("drained batch observer");
    assert!(
        fixture
            .runtime
            .formal_calls(&owner.to_string())
            .expect("no late work after drain")
            .is_empty()
    );
    assert_eq!(
        fixture
            .runtime
            .formal_runtime_status()
            .expect("physical")
            .expect("parent")
            .resources
            .resident,
        0
    );
    assert!(
        spawn_for_turn(
            &fixture.state,
            fixture.runtime.clone(),
            owner,
            "draining",
            generation
        )
        .expect("original remains consumed")
        .is_none()
    );
    fixture.close().await;
}

/// Disabled policy consumes this accepted trigger, and later turns reclaim prior successful automatic history.
/// 关闭策略消费此次已接纳触发，后续回合回收先前成功自动历史。
#[tokio::test]
async fn plugin_automatic_turns_keep_bounded_success_history_without_midturn_policy_replay() {
    let fixture = Fixture::new();
    let owner = uuid::Uuid::new_v4();
    fixture.attach_workspace(owner);
    fixture.enable_prewarm();
    let generation = fixture.accept_turn(owner, "disabled-policy");
    spawn_for_turn(
        &fixture.state,
        fixture.runtime.clone(),
        owner,
        "disabled-policy",
        generation,
    )
    .expect("disabled first trigger")
    .expect("once-only observer")
    .await
    .expect("policy observation");
    assert!(
        fixture
            .runtime
            .formal_calls(&owner.to_string())
            .expect("default disabled")
            .is_empty()
    );
    fixture.automatic_rule().await;
    assert!(
        spawn_for_turn(
            &fixture.state,
            fixture.runtime.clone(),
            owner,
            "disabled-policy",
            generation
        )
        .expect("policy save is not a new turn")
        .is_none()
    );
    for turn in ["first-enabled", "second-enabled", "third-enabled"] {
        crate::finish_webui_session_kernel_turn(
            &mut fixture.state.lock().expect("state"),
            owner,
            vulcan_session_core::TurnTerminalStatus::Completed,
        );
        let generation = fixture.accept_turn(owner, turn);
        let invocation = fixture.automatic_target(owner, turn, generation).await;
        let completed = fixture.wait_prewarm(owner, &invocation).await;
        assert!(completed.value.is_some(), "{completed:?}");
        assert_eq!(
            fixture
                .runtime
                .formal_calls(&owner.to_string())
                .expect("bounded retained success")
                .len(),
            1,
            "only this turn's successful target should remain"
        );
        assert_eq!(
            fixture
                .runtime
                .formal_runtime_status()
                .expect("native state")
                .expect("parent")
                .resources
                .resident,
            1,
            "subsequent turns must reuse the same initialized VM"
        );
    }
    fixture.close().await;
}

/// Normal or failed terminalization revokes queued work just as an explicit stop does.
/// 正常或失败终态与明确停止一样撤销排队工作。
#[tokio::test]
async fn plugin_automatic_terminal_turn_revokes_queued_initialization() {
    let fixture = Fixture::new();
    let owner = uuid::Uuid::new_v4();
    fixture.attach_workspace(owner);
    fixture.enable_prewarm();
    fixture.automatic_rule().await;
    for (turn, terminal) in [
        (
            "completed",
            vulcan_session_core::TurnTerminalStatus::Completed,
        ),
        ("failed", vulcan_session_core::TurnTerminalStatus::Failed),
    ] {
        let generation = fixture.accept_turn(owner, turn);
        let queued = spawn_for_turn(
            &fixture.state,
            fixture.runtime.clone(),
            owner,
            turn,
            generation,
        )
        .expect("queue trigger")
        .expect("first trigger");
        crate::finish_webui_session_kernel_turn(
            &mut fixture.state.lock().expect("state"),
            owner,
            terminal,
        );
        queued.await.expect("terminal batch");
        assert!(
            fixture
                .runtime
                .formal_calls(&owner.to_string())
                .expect("no late work")
                .is_empty()
        );
        assert_eq!(
            fixture
                .runtime
                .formal_runtime_status()
                .expect("physical")
                .expect("parent")
                .resources
                .resident,
            0
        );
    }
    fixture.close().await;
}
