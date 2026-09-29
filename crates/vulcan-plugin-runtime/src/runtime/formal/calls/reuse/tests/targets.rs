//! Retained target receipts exercise real scheduler readiness and exact startup ownership.
//! 保留目标回执验证真实调度器就绪及精确启动归属。

use super::*;

/// Native operation quota exhaustion preserves the already successful initialization and its receipt.
/// 原生操作配额耗尽保留已成功初始化及其回执。
#[tokio::test]
async fn hot_reuse_target_admission_failure_retains_prior_success() {
    // Two retained native slots permit one initializer and its mandatory independent finalizer.
    // 两个原生保留槽允许一个初始化器及其必需独立关闭器。
    let mut settings = VulcanPluginRuntimeSettings::default();
    settings.parent.max_operations = 2;
    settings.parent.max_running_calls = 1;
    settings.parent.max_queued_calls = 1;
    // The real enabled fixture owns the authoritative package and formal runtime.
    // 真实已启用夹具拥有权威包及正式运行时。
    let fixture = Fixture::with_settings(10_000, 100, settings);
    fixture.target_capacity(2);
    // Retain the inert original receipt before starting any native initialization.
    // 在启动任何原生初始化前保留原未启动回执。
    let prepared = fixture
        .manager
        .prepare_reusable_tool_prewarm(fixture.target(2))
        .expect("register original prewarm target");
    fixture
        .manager
        .start_reusable_tool_prewarm(&prepared.invocation_id, "owner")
        .expect("start or observe original target");
    // Observe actual retirement while retaining all admitted operation evidence.
    // 观测真实退役，同时保留全部已接纳操作证据。
    let failed = wait_target(&fixture, &prepared.invocation_id).await;
    assert!(failed.error.is_some(), "{failed:?}");
    assert!(
        failed.operation.is_none(),
        "second native admission cannot create an operation"
    );
    // Inspect the target-owned history separately from the current operation.
    // 将目标拥有历史与当前操作分开检查。
    let progress = failed.prewarm.expect("retained target progress");
    assert!(progress.satisfied.is_none());
    assert_eq!(progress.completed.len(), 1);
    // Keep original native evidence for later identity and retention assertions.
    // 保留原生证据，以供后续身份及保留断言。
    let original = progress
        .completed
        .first()
        .expect("earlier completed initialization");
    assert!(
        original
            .value
            .as_ref()
            .expect("successful native or target result")["instance_id"]
            .is_string()
    );
    assert_eq!(
        fixture
            .owner()
            .host
            .runtime()
            .resources()
            .expect("query actual native resources")
            .resident,
        1
    );
    fixture
        .manager
        .forget_formal_call(&prepared.invocation_id, "owner")
        .expect("acknowledge retained target");
    assert!(
        fixture
            .owner()
            .host
            .runtime()
            .operation(&original.operation_id)
            .is_err()
    );
    assert_eq!(
        fixture
            .invoke("owner", fixture.root.path().to_path_buf(), "ok")
            .await
            .expect("invoke first business after prewarming")["count"],
        1
    );
    fixture.close().await;
}

impl Fixture {
    /// Build a trusted target request using this fixture's current exact parent and catalog generation.
    /// 使用此夹具当前精确父级及目录代次构建可信目标请求。
    /// Target selects desired idle instances; return only metadata without starting work.
    /// target 选择期望空闲实例；仅返回元数据，不启动工作。
    pub(super) fn target(&self, target: usize) -> VulcanPluginPrewarmTargetRequest {
        VulcanPluginPrewarmTargetRequest {
            runtime_id: self.owner().host.runtime().id().to_owned(),
            plugin_id: "idle-plugin".into(),
            tool_id: "idle_ping".into(),
            plugin_generation: self.manager.snapshot().enabled_tools["idle_ping"].plugin_generation,
            owner_session_id: "owner".into(),
            workspace_root: self.root.path().to_path_buf(),
            target_idle: target,
        }
    }

    /// Change the authoritative resident limit to maximum before execution and republish the manifest.
    /// 在执行前将权威常驻上限改为 maximum，并重新发布清单。
    pub(super) fn target_capacity(&self, maximum: usize) {
        // Use the authoritative fixture manifest or source path already selected by this test.
        // 使用此测试已选择的权威夹具清单或源码路径。
        let path = self.root.path().join("bundled/idle-plugin/plugin.yaml");
        // Preserve existing declarations while changing only the targeted resident ceiling.
        // 仅改变目标常驻上限，同时保留既有声明。
        let mut manifest: Value =
            serde_yaml::from_str(&std::fs::read_to_string(&path).expect("manifest"))
                .expect("mapping");
        manifest["lua_runtime"]["tools"]
            .as_array_mut()
            .expect("tools")
            .iter_mut()
            .find(|tool| tool["id"] == "idle_ping")
            .expect("fixture ping tool changed")["execution"]["reusable"]["max_resident_vms"] =
            json!(maximum);
        std::fs::write(path, serde_yaml::to_string(&manifest).expect("encode")).expect("write");
        self.manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("publish target capacity");
    }
}

/// Observe id under its original controller until real retirement; fail on the single fixture deadline.
/// 按原控制器观测 id 直到真实退役；唯一夹具截止时间到期则失败。
/// Return retained evidence without acknowledging or replaying any initialization.
/// 返回保留证据，不确认或重放任何初始化。
pub(super) async fn wait_target(fixture: &Fixture, id: &str) -> VulcanPluginFormalCallStatus {
    // Bound observation with the shared fixture timeout without completing native work artificially.
    // 用共享夹具超时约束观测，不人为完成原生工作。
    let deadline = Instant::now() + WAIT;
    loop {
        // Query the same retained receipt under its original controller.
        // 在原控制器下查询同一保留回执。
        let status = fixture
            .manager
            .formal_call_status(id, "owner")
            .expect("query original target");
        if status.retired && (status.value.is_some() || status.error.is_some()) {
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "target did not retire: {status:?}"
        );
        tokio::time::sleep(OBSERVATION_INTERVAL).await;
    }
}

/// Zero, additional and already-satisfied targets execute only the necessary real initializations.
/// 零目标、增加目标及已满足目标仅执行必要真实初始化。
#[tokio::test]
async fn hot_reuse_target_receipts_are_repeatable_and_keep_distinct_operations() {
    // The real enabled fixture owns the authoritative package and formal runtime.
    // 真实已启用夹具拥有权威包及正式运行时。
    let fixture = Fixture::new(10_000, 100);
    fixture.target_capacity(2);
    // Retain the zero-initialization target receipt for resource and acknowledgement assertions.
    // 保留零初始化目标回执，用于资源及确认断言。
    let zero = fixture
        .manager
        .prepare_reusable_tool_prewarm(fixture.target(0))
        .expect("prepare zero");
    assert!(
        !zero
            .prewarm
            .as_ref()
            .expect("retained target progress")
            .started
    );
    assert!(!zero.requires_cleanup_recovery);
    assert_eq!(
        fixture
            .owner()
            .host
            .runtime()
            .resources()
            .expect("query actual native resources")
            .resident,
        0
    );
    fixture
        .manager
        .start_reusable_tool_prewarm(&zero.invocation_id, "owner")
        .expect("start zero");
    // Retain the zero-initialization target receipt for resource and acknowledgement assertions.
    // 保留零初始化目标回执，用于资源及确认断言。
    let zero = wait_target(&fixture, &zero.invocation_id).await;
    assert_eq!(
        zero.value
            .as_ref()
            .expect("successful native or target result")["initializations"],
        0
    );
    assert!(zero.operation.is_none());
    assert_eq!(
        fixture
            .owner()
            .host
            .runtime()
            .resources()
            .expect("query actual native resources")
            .resident,
        0
    );
    // Freeze the exact parent, plugin generation, controller, workspace and desired count.
    // 冻结精确父级、插件代次、控制器、工作区及期望数量。
    let request = fixture.target(2);
    // Retain the inert original receipt before starting any native initialization.
    // 在启动任何原生初始化前保留原未启动回执。
    let prepared = fixture
        .manager
        .prepare_reusable_tool_prewarm(request.clone())
        .expect("prepare two");
    fixture
        .manager
        .start_reusable_tool_prewarm(&prepared.invocation_id, "owner")
        .expect("start two");
    fixture
        .manager
        .start_reusable_tool_prewarm(&prepared.invocation_id, "owner")
        .expect("repeat start");
    // Wait for actual target retirement before inspecting confirmed native identities.
    // 等待目标真实退役后再检查已确认原生身份。
    let completed = wait_target(&fixture, &prepared.invocation_id).await;
    assert!(completed.error.is_none(), "{completed:?}");
    assert_eq!(
        completed
            .value
            .as_ref()
            .expect("successful native or target result")["initializations"],
        2
    );
    // Inspect the target-owned history separately from the current operation.
    // 将目标拥有历史与当前操作分开检查。
    let progress = completed
        .prewarm
        .as_ref()
        .expect("retained target progress");
    assert_eq!(progress.request.runtime_id, request.runtime_id);
    assert_eq!(progress.request.workspace_root, request.workspace_root);
    assert_eq!(
        progress
            .satisfied
            .as_ref()
            .expect("confirmed readiness observation")
            .ready,
        2
    );
    assert_eq!(progress.completed.len(), 1);
    // Keep the earlier original operation distinct from the latest initializer.
    // 将先前原操作与最新初始化器区分保留。
    let previous = progress
        .completed
        .first()
        .expect("previous original operation");
    // Inspect the latest native initializer retained by the enclosing call.
    // 检查外层调用保留的最新原生初始化器。
    let latest = completed
        .operation
        .as_ref()
        .expect("last original operation");
    assert_ne!(previous.operation_id, latest.operation_id);
    assert_ne!(
        previous
            .value
            .as_ref()
            .expect("successful native or target result")["instance_id"],
        latest
            .value
            .as_ref()
            .expect("successful native or target result")["instance_id"]
    );
    // A repeated start must observe the same receipt without new work.
    // 重复启动必须观测同一回执，且不产生新工作。
    let repeated = fixture
        .manager
        .start_reusable_tool_prewarm(&prepared.invocation_id, "owner")
        .expect("completed start is observation");
    assert_eq!(
        repeated
            .operation
            .expect("original native operation")
            .operation_id,
        latest.operation_id
    );
    // An already-satisfied target must retain its own zero-work completion receipt.
    // 已满足目标必须保留自身零工作完成回执。
    let hot = fixture
        .manager
        .prepare_reusable_tool_prewarm(fixture.target(2))
        .expect("already hot target");
    fixture
        .manager
        .start_reusable_tool_prewarm(&hot.invocation_id, "owner")
        .expect("start or observe original target");
    // An already-satisfied target must retain its own zero-work completion receipt.
    // 已满足目标必须保留自身零工作完成回执。
    let hot = wait_target(&fixture, &hot.invocation_id).await;
    assert_eq!(
        hot.value
            .as_ref()
            .expect("successful native or target result")["initializations"],
        0
    );
    assert!(hot.operation.is_none());
    assert_eq!(
        fixture
            .invoke("owner", fixture.root.path().to_path_buf(), "ok")
            .await
            .expect("invoke first business after prewarming")["count"],
        1
    );
    for id in [
        &zero.invocation_id,
        &completed.invocation_id,
        &hot.invocation_id,
    ] {
        fixture
            .manager
            .forget_formal_call(id, "owner")
            .expect("acknowledge all checkpoints");
        assert!(
            fixture
                .manager
                .start_reusable_tool_prewarm(id, "owner")
                .is_err()
        );
    }
    assert!(
        fixture
            .owner()
            .host
            .runtime()
            .operation(&previous.operation_id)
            .is_err()
    );
    assert!(
        fixture
            .owner()
            .host
            .runtime()
            .operation(&latest.operation_id)
            .is_err()
    );
    fixture.close().await;
}

/// Prepared receipts enforce authority and retire without creating any VM when cancelled or closed.
/// 准备回执强制权威校验，并在取消或关闭时退役且不创建 VM。
#[tokio::test]
async fn hot_reuse_target_unstarted_cancel_close_and_stale_identity_are_inert() {
    // The real enabled fixture owns the authoritative package and formal runtime.
    // 真实已启用夹具拥有权威包及正式运行时。
    let fixture = Fixture::new(10_000, 100);
    // Change only the observed parent identity to prove stale admission is rejected.
    // 仅改变已观测父级身份，证明过期入场被拒绝。
    let mut stale = fixture.target(1);
    stale.runtime_id = "different-parent".into();
    assert!(
        fixture
            .manager
            .prepare_reusable_tool_prewarm(stale)
            .is_err()
    );
    assert!(
        fixture
            .manager
            .prepare_reusable_tool_prewarm(fixture.target(2))
            .is_err()
    );
    // Retain the inert original receipt before starting any native initialization.
    // 在启动任何原生初始化前保留原未启动回执。
    let prepared = fixture
        .manager
        .prepare_reusable_tool_prewarm(fixture.target(1))
        .expect("register original prewarm target");
    assert!(
        fixture
            .manager
            .start_reusable_tool_prewarm(&prepared.invocation_id, "other")
            .is_err()
    );
    assert!(
        fixture
            .manager
            .cancel_formal_call(&prepared.invocation_id, "other")
            .is_err()
    );
    fixture
        .manager
        .cancel_formal_call(&prepared.invocation_id, "owner")
        .expect("cancel original target");
    // Observe cancellation of pending input without creating an executor or VM.
    // 观测待处理输入取消，不创建执行器或 VM。
    let cancelled = fixture
        .manager
        .start_reusable_tool_prewarm(&prepared.invocation_id, "owner")
        .expect("start or observe original target");
    assert!(cancelled.retired);
    assert!(!cancelled.prewarm.expect("retained target progress").started);
    assert!(
        cancelled
            .error
            .expect("expected target failure")
            .contains("before startup")
    );
    fixture
        .manager
        .forget_formal_call(&prepared.invocation_id, "owner")
        .expect("acknowledge retained target");
    // Retain original work ownership while testing bounded drain or parent shutdown.
    // 测试有界排空或父级关闭时保留原工作归属。
    let pending = fixture
        .manager
        .prepare_reusable_tool_prewarm(fixture.target(1))
        .expect("register original prewarm target");
    fixture.close().await;
    // Read the original receipt after parent shutdown rather than inferring completion from resources.
    // 父级关闭后读取原回执，不根据资源计数推断完成。
    let closed = fixture
        .manager
        .formal_call_status(&pending.invocation_id, "owner")
        .expect("query original target");
    assert!(closed.retired);
    assert!(!closed.prewarm.expect("retained target progress").started);
    assert!(closed.operation.is_none());
    assert_eq!(
        fixture
            .owner()
            .host
            .runtime()
            .resources()
            .expect("query actual native resources")
            .resident,
        0
    );
    assert_eq!(
        fixture.manager.snapshot().plugins["idle-plugin"].active_session_count,
        0
    );
}

/// Preparation retains its original generation through delayed startup and rejects excess metadata before work.
/// 准备在延后启动时保留原代次，并在工作前拒绝过量元数据。
#[tokio::test]
async fn hot_reuse_target_prepared_generation_and_input_budget_cannot_be_reselected() {
    // Use explicit native budgets to test refusal before allocating unbounded metadata.
    // 使用明确原生预算，测试在分配无界元数据前拒绝。
    let mut settings = VulcanPluginRuntimeSettings::default();
    settings.parent.max_queued_bytes = 4096;
    settings.parent.max_value_bytes = settings.parent.max_queued_bytes;
    // The real enabled fixture owns the authoritative package and formal runtime.
    // 真实已启用夹具拥有权威包及正式运行时。
    let fixture = Fixture::with_settings(10_000, 100, settings);
    // Oversized trusted routing metadata must fail before publication or initialization.
    // 过量可信路由元数据必须在发布或初始化前失败。
    let mut excess = fixture.target(1);
    excess.owner_session_id = "x".repeat(fixture.owner().settings.parent.max_queued_bytes);
    assert!(
        fixture
            .manager
            .prepare_reusable_tool_prewarm(excess)
            .is_err()
    );
    assert!(
        fixture
            .manager
            .formal_calls("owner")
            .expect("inspect retained target calls")
            .is_empty()
    );
    // Retain the inert original receipt before starting any native initialization.
    // 在启动任何原生初始化前保留原未启动回执。
    let prepared = fixture
        .manager
        .prepare_reusable_tool_prewarm(fixture.target(1))
        .expect("register original prewarm target");
    // Locate the real declared bundle whose fingerprint owns this generation.
    // 定位其指纹拥有此代次的真实声明 bundle。
    let source_path = fixture
        .root
        .path()
        .join("bundled/idle-plugin/runtime/plugin.bundle.lua");
    // Preserve real bundle behavior while making its source fingerprint change.
    // 保留真实 bundle 行为，同时改变其源码指纹。
    let source = std::fs::read_to_string(&source_path).expect("read real plugin source");
    std::fs::write(source_path, format!("-- new generation\n{source}"))
        .expect("write updated real plugin source");
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("publish updated plugin generation");
    fixture
        .manager
        .start_reusable_tool_prewarm(&prepared.invocation_id, "owner")
        .expect("start or observe original target");
    // Observe actual retirement while retaining all admitted operation evidence.
    // 观测真实退役，同时保留全部已接纳操作证据。
    let failed = wait_target(&fixture, &prepared.invocation_id).await;
    assert_eq!(failed.plugin_generation, prepared.plugin_generation);
    assert!(failed.error.is_some(), "{failed:?}");
    assert!(failed.operation.is_none());
    assert_eq!(
        fixture
            .owner()
            .host
            .runtime()
            .resources()
            .expect("query actual native resources")
            .resident,
        0
    );
    fixture.close().await;
}
