//! Global drain evidence survives stale bindings and physical resource ownership.
//! 全局排空证据跨陈旧绑定及实际资源所有权保留。

use super::super::tools::owner_drain_tests::{
    OBSERVE, access, child_holds, child_released, server,
};
use super::*;
use crate::mcp::config::McpServerSource;

/// A ready candidate remains drainable from the old service access before publication.
/// 就绪候选在发布之前，仍可从旧服务访问排空。
/// Both explicit plugin drain and application shutdown must retain this ownership handoff.
/// 显式插件排空与应用关闭都必须保留此所有权交接。
#[tokio::test]
async fn plugin_mcp_drain_keeps_ready_unpublished_candidates() {
    for application_shutdown in [false, true] {
        // This old access intentionally never receives the candidate's live partition.
        // 此旧访问故意始终不接收候选实时分区。
        let current = access();
        // A real child-owned file lock distinguishes readiness from physical resource retirement.
        // 真实子进程自有文件锁区分就绪与实际资源退役。
        let witness = tempfile::NamedTempFile::new().expect("ready candidate witness");
        // Candidate startup completes, but no service generation publishes the returned access.
        // 候选启动完成，但没有服务代次发布返回访问。
        let candidate = replace_plugin_mcp_runtime_partition(
            &current,
            "ready",
            5,
            vec![server("ready", witness.path(), true)],
        )
        .await
        .expect("ready unpublished candidate");
        child_holds(&witness).await;
        // A stale binding and the candidate access both remain retained throughout shutdown.
        // 关闭期间始终保留陈旧绑定与候选访问。
        let bindings =
            candidate.available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default());
        current
            .begin_plugin_owner_drain("ready")
            .expect("close session owner gate only");
        assert!(
            current
                .wait_plugin_mcp_drain("ready", OBSERVE)
                .await
                .expect_err("ready unpublished global is still active")
                .contains("has not entered drain")
        );
        if application_shutdown {
            current.shutdown();
        } else {
            current
                .begin_plugin_mcp_drain("ready")
                .expect("explicit candidate drain");
        }
        current
            .wait_plugin_mcp_drain("ready", OBSERVE)
            .await
            .expect("ready original fully released");
        child_released(&witness);
        // Select by the declared tool identity rather than its public ordering.
        // 按声明的工具身份选择，不依赖公开顺序。
        let probe = bindings
            .iter()
            .find(|binding| binding.origin_tool_name() == Some("probe"))
            .expect("fixture probe declaration may have changed");
        assert!(
            probe
                .invoke(serde_json::json!({}))
                .await
                .expect_err("unpublished candidate binding must be revoked")
                .contains("mcp_partition_draining")
        );
        assert!(
            candidate
                .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
                .is_empty()
        );
    }
}

/// Application shutdown stops an unpublished global child and prevents a subsequent global start.
/// 应用关闭停止未发布全局子进程，并阻止之后的全局启动。
#[tokio::test]
async fn plugin_mcp_drain_global_shutdown_latches_unpublished_starts() {
    // Retain the pre-start access to reproduce shutdown before candidate publication.
    // 保留启动前访问，复现候选发布之前的关闭。
    let current = access();
    // The child's own lock proves initialization really began.
    // 子进程自身锁证明初始化确实开始。
    let witness = tempfile::NamedTempFile::new().expect("global shutdown witness");
    // The real parked initialize remains pending while application shutdown closes admission.
    // 应用关闭入场时，真实驻留 initialize 仍在等待。
    let mut starting = Box::pin(replace_plugin_mcp_runtime_partition(
        &current,
        "pending",
        5,
        vec![server("pending", witness.path(), false)],
    ));
    tokio::select! {
        result = &mut starting => panic!("parked initialization unexpectedly returned: {result:?}"),
        () = child_holds(&witness) => {},
    }
    current.shutdown();
    assert!(
        starting.await.is_err(),
        "shutdown must prevent late publication"
    );
    current
        .wait_plugin_mcp_drain("pending", OBSERVE)
        .await
        .expect("original child released");
    child_released(&witness);
    assert!(
        replace_plugin_mcp_runtime_partition(
            &current,
            "pending",
            5,
            vec![server("pending", witness.path(), true)],
        )
        .await
        .expect_err("globally closed access cannot start")
        .contains("globally closed")
    );
    child_released(&witness);
    assert!(current.finish_plugin_owner_drain("pending").is_err());
}

/// Cancels a real unpublished global start, then drains it from the unchanged published access.
/// 取消真实未发布全局启动，再从未改变的发布访问排空它。
#[tokio::test]
async fn plugin_mcp_drain_keeps_cancelled_unpublished_global_start() {
    // The original access remains authoritative although readiness never publishes the candidate.
    // 即使就绪从未发布候选，原访问仍然保持权威。
    let current = access();
    // This OS lock proves the actual parked child, rather than only a future, was started.
    // 此操作系统锁证明启动了实际驻留子进程，而不只是 future。
    let witness = tempfile::NamedTempFile::new().expect("unpublished global witness");
    // Pin the actual production startup so its caller can disappear during initialize.
    // 固定真实生产启动，使其调用者可在 initialize 期间消失。
    let mut starting = Box::pin(replace_plugin_mcp_runtime_partition(
        &current,
        "pending",
        5,
        vec![server("pending", witness.path(), false)],
    ));
    tokio::select! {
        result = &mut starting => panic!("parked initialization unexpectedly returned: {result:?}"),
        () = child_holds(&witness) => {},
    }
    drop(starting);
    current
        .begin_plugin_mcp_drain("pending")
        .expect("close original unpublished instance");
    current
        .wait_plugin_mcp_drain("pending", OBSERVE)
        .await
        .expect("unpublished original released");
    child_released(&witness);
    assert!(!current.has_usable_retired_plugin_partitions("pending"));
    current
        .finish_plugin_owner_drain("pending")
        .expect("reopen after physical release");
}

/// Verifies current and retired global processes release while unrelated plugin and user tools remain callable.
/// 验证当前及已退役全局进程释放，同时无关插件及用户工具仍可调用。
/// Retained stale controls outlive a cancelled observer without hiding original process release.
/// 保留的陈旧控制比被取消的观察者存活更久，但不隐藏原进程释放。
#[tokio::test]
async fn plugin_mcp_drain_observes_global_resources_and_preserves_other_partitions() {
    // Each witness is held only by its own real child process.
    // 每份见证仅由对应真实子进程持有。
    let first = tempfile::NamedTempFile::new().expect("first global witness");
    // A replacement gets a distinct native process and file lock.
    // 替换版本取得独立原生进程及文件锁。
    let second = tempfile::NamedTempFile::new().expect("second global witness");
    // The unrelated plugin must retain both process ownership and tool admission.
    // 无关插件必须保留进程所有权及工具入场。
    let other = tempfile::NamedTempFile::new().expect("other plugin witness");
    // User configuration must not share the target plugin's drain decision.
    // 用户配置不得共享目标插件的排空决策。
    let user = tempfile::NamedTempFile::new().expect("user witness");
    // Start through the actual global readiness entry, not an owner partition.
    // 通过真实全局就绪入口启动，而不是所有者分区。
    let original = bootstrap_mcp_runtime(vec![server("target", first.path(), true)])
        .await
        .expect("original global runtime");
    original
        .wait_until_ready(OBSERVE)
        .await
        .expect("ready original global");
    // Preserve a direct original control handle alongside the published access.
    // 在发布访问之外保留直接原控制句柄。
    let previous =
        access().with_replaced_plugin_partition("target", 5, original.overview(), original.clone());
    // Retain a stale transport while verifying the actual native resource lifetime separately.
    // 保留陈旧传输，同时独立验证实际原生资源寿命。
    let retained_transport = original
        .transport_for_server("target:owner")
        .expect("original transport");
    // Capture old tool metadata before a catalog replacement.
    // 在目录替换之前捕获旧工具元数据。
    let stale = previous.available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default());
    // The grace covers all bounded fixture startups; only explicit drain should stop the original here.
    // 宽限期覆盖全部有界夹具启动；此处应仅由显式排空停止原实例。
    previous.retire_plugin_partitions_after(
        "target",
        MCP_PARTITION_READINESS_TIMEOUT.saturating_mul(8),
    );
    // The next access shares original retirement receipts with its predecessor.
    // 后继访问与前驱共享原退役回执。
    let current = replace_plugin_mcp_runtime_partition(
        &previous,
        "target",
        5,
        vec![server("target", second.path(), true)],
    )
    .await
    .expect("ready replacement");
    // Build a second plugin using the same real transport path.
    // 使用同一真实传输路径构建第二个插件。
    let current = replace_plugin_mcp_runtime_partition(
        &current,
        "other",
        5,
        vec![server("other", other.path(), true)],
    )
    .await
    .expect("ready unrelated plugin");
    // Only the explicit source authority changes for this user-configured server.
    // 此用户配置服务器仅改变明确来源权威。
    let mut user_server = server("user", user.path(), true);
    user_server.source = McpServerSource::UserConfigured;
    // The production replacement registers this actual user original before native dispatch.
    // 生产替换在原生派发前登记此实际用户原实例。
    let user_lease = current
        .start_user_partition(vec![user_server])
        .expect("registered user replacement");
    // This observation refers to the exact published user execution owner.
    // 此观察指向精确已发布用户执行所有者。
    let user_runtime = user_lease.runtime.clone();
    // Publish the sole registered execution lease while keeping only the independent control for observation.
    // 发布唯一已登记执行租约，同时仅保留独立控制用于观察。
    let current = current.with_replaced_user_partition(user_runtime.overview(), user_lease);
    user_runtime
        .wait_until_ready(OBSERVE)
        .await
        .expect("ready user");
    child_holds(&first).await;
    child_holds(&second).await;
    child_holds(&other).await;
    child_holds(&user).await;
    // Closing only session owners does not satisfy the global barrier.
    // 仅关闭会话所有者不能满足全局屏障。
    current
        .begin_plugin_owner_drain("target")
        .expect("owner closure");
    assert!(
        current
            .wait_plugin_mcp_drain("target", OBSERVE)
            .await
            .expect_err("undrained live global must be rejected")
            .contains("has not entered drain")
    );
    current
        .begin_plugin_mcp_drain("target")
        .expect("close all original partitions");
    // Old and new views immediately stop offering the revoked target.
    // 旧新视图立即停止提供已撤销目标。
    assert!(
        previous
            .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
            .is_empty()
    );
    // The captured old binding must reject without dispatching a transport call.
    // 已捕获旧绑定必须拒绝，且不分派传输调用。
    let old_probe = stale
        .iter()
        .find(|binding| binding.origin_tool_name() == Some("probe"))
        .expect("fixture probe declaration may have changed");
    assert!(
        old_probe
            .invoke(serde_json::json!({}))
            .await
            .expect_err("old global binding revoked")
            .contains("mcp_partition_draining")
    );
    // Cancelling one observer cannot consume or erase the outstanding original receipt.
    // 取消一个观察者不能消费或删除尚未完成的原回执。
    let mut observer = Box::pin(current.wait_plugin_mcp_drain("target", OBSERVE));
    std::future::poll_fn(|context| {
        assert!(std::future::Future::poll(observer.as_mut(), context).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    drop(observer);
    assert!(
        current
            .wait_plugin_mcp_drain("target", Duration::ZERO)
            .await
            .is_err()
    );
    assert!(current.has_usable_retired_plugin_partitions("target"));
    current
        .wait_plugin_mcp_drain("target", OBSERVE)
        .await
        .expect("all originals released");
    // A disconnected transport reference must neither reopen admission nor keep the child alive.
    // 已断开的传输引用不得重新开放入场，也不应继续保留子进程。
    assert!(original.transport_for_server("target:owner").is_none());
    assert!(original.is_fully_stopped());
    drop(retained_transport);
    child_released(&first);
    child_released(&second);
    assert!(!current.has_usable_retired_plugin_partitions("target"));
    // Filter by exact source server, never by unstable ordering of exposed bindings.
    // 按精确来源服务器筛选，不依赖公开绑定的易变顺序。
    let remaining = current.available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default());
    for server_id in ["other:owner", "user:owner"] {
        // Each surviving binding calls its own original real child.
        // 每个保留绑定调用其自身原真实子进程。
        let probe = remaining
            .iter()
            .find(|binding| {
                binding.server_id() == Some(server_id)
                    && binding.origin_tool_name() == Some("probe")
            })
            .expect("unrelated fixture probe may have changed");
        probe
            .invoke(serde_json::json!({}))
            .await
            .expect("unrelated tool remains callable");
    }
    current
        .begin_plugin_mcp_drain("other")
        .expect("fixture cleanup");
    current
        .wait_plugin_mcp_drain("other", OBSERVE)
        .await
        .expect("other fixture released");
    current.shutdown();
    user_runtime
        .wait_until_drained(tokio::time::Instant::now() + OBSERVE)
        .await
        .expect("user fixture released");
    child_released(&other);
    child_released(&user);
}
