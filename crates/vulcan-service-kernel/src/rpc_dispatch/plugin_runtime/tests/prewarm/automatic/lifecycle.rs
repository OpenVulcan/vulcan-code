//! Real service generations drive production automatic triggers and original native receipts.
//! 真实服务代际驱动生产自动触发及原原生回执。

use super::*;
use crate::plugin_runtime_services::automatic_prewarm::spawn_for_turn;

impl Fixture {
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

    /// Run the production admission and await only metadata submission, then return the original target identity.
    /// 运行生产入场并仅等待元数据提交，随后返回原目标身份。
    async fn automatic_target(&self, owner: uuid::Uuid, turn: &str, generation: u64) -> Value {
        spawn_for_turn(&self.state, self.runtime.clone(), owner, turn, generation)
            .expect("automatic trigger")
            .expect("first trigger task")
            .await
            .expect("batch observer");
        let targets = self
            .runtime
            .formal_calls(&owner.to_string())
            .expect("original receipts");
        let target = targets
            .iter()
            .find(|call| {
                call.prewarm
                    .as_ref()
                    .is_some_and(|target| target.automatic.is_some())
            })
            .expect("automatic target must exist; check real manifest and accepted policy");
        json!({"runtimeId":target.prewarm.as_ref().expect("selected target").request.runtime_id, "ownerSessionId":owner.to_string(), "invocationId":target.invocation_id})
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
    let identity = fixture.automatic_target(owner, "first", generation).await;
    let completed = fixture.wait_prewarm(&identity).await;
    assert_eq!(completed["error"], Value::Null, "{completed}");
    assert_eq!(completed["workspaceRoot"], json!(workspace));
    assert_eq!(completed["satisfied"]["ready"], 1);
    assert!(
        completed["automatic"]["triggerId"].as_str().is_some(),
        "original provenance must cross the production RPC"
    );
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
        .rpc("service_forget_plugin_prewarm", identity)
        .await
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
    let identity = fixture
        .automatic_target(owner, "same-logical-turn", replacement)
        .await;
    let completed = fixture.wait_prewarm(&identity).await;
    assert_eq!(completed["succeeded"], true, "{completed}");
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
        let identity = fixture.automatic_target(owner, turn, generation).await;
        let completed = fixture.wait_prewarm(&identity).await;
        assert_eq!(completed["succeeded"], true, "{completed}");
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
