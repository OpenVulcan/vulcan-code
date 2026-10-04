//! Reusable native ownership differs from retained failure history and fixed-tool counts.
//! 可复用原生所有权与保留失败历史及固定工具计数不同。

use super::*;

/// Diagnose the real reusable domain before and after idle physical retirement without leaking shutdown results.
/// 在空闲物理退役前后诊断真实复用域，不泄露关闭结果。
#[tokio::test]
async fn hot_reuse_resource_diagnostics_distinguish_owned_pool_from_retired_failure_history() {
    // Existing fixture uses its real short manifest idle TTL to retain independent failure evidence.
    // 既有夹具使用真实较短清单空闲期限，保留独立失败证据。
    let fixture = Fixture::new(20, 100);
    fixture
        .invoke("owner", fixture.root.path().to_path_buf(), "shutdown_error")
        .await
        .expect("real reusable business");
    // Actual domain identity comes from the controller's original retained registry.
    // 实际域身份来自控制器原保留注册表。
    let domain = fixture
        .manager
        .reusable_tool_domains("owner")
        .expect("original domain")
        .into_iter()
        .find(|domain| domain.tool_id == "idle_ping")
        .expect("expected reusable declaration may have changed");
    // Poll safe metadata until the exact pool is retired, allowing only the documented short-lock Busy condition.
    // 轮询安全元数据至精确池退役，仅允许已记录短锁忙碌条件。
    let deadline = Instant::now() + WAIT;
    loop {
        // Read the full plugin ownership without enumerating active conversations.
        // 读取完整插件所有权，不枚举活动会话。
        let observation = match fixture
            .manager
            .formal_plugin_resource_diagnostics("idle-plugin", None)
        {
            Ok(Some(observation)) => observation,
            Err(error) if error.message == "Plugin resource diagnostics are busy." => {
                assert!(Instant::now() < deadline, "diagnostic state remained busy");
                tokio::time::sleep(OBSERVATION_INTERVAL).await;
                continue;
            }
            other => panic!("unexpected diagnostic observation: {other:?}"),
        };
        assert_eq!(
            observation.pinned_session_count, 0,
            "native reusable work is not a fixed tool session"
        );
        // Terminal finalization history remains keyed by its actual original domain.
        // 终态关闭历史继续以实际原域为键。
        let row = observation
            .resources
            .iter()
            .find(|row| row.resource_id == domain.domain_id)
            .expect("failed original domain must remain until acknowledgement");
        assert_eq!(row.resource_kind, VulcanPluginResourceKind::ReusableDomain);
        assert_eq!(row.owner_session_id.as_deref(), Some("owner"));
        assert_eq!(row.original_pool_id, domain.pool_id);
        if row.retired && !row.pending_finalizations && !row.finalization_operation_ids.is_empty() {
            assert!(
                !row.is_drain_blocking,
                "retained terminal failure evidence owns no physical VM"
            );
            // Serialization must preserve metadata while excluding the application's closing error marker.
            // 序列化必须保留元数据，同时排除应用关闭错误标记。
            let encoded = serde_json::to_string(row).expect("safe domain JSON");
            assert!(!encoded.contains("shutdown-marker"));
            break;
        }
        assert!(
            Instant::now() < deadline,
            "exact reusable pool failed to retire"
        );
        tokio::time::sleep(OBSERVATION_INTERVAL).await;
    }
    fixture
        .manager
        .forget_reusable_tool_domain(&domain.domain_id, "owner")
        .expect("acknowledge original failure");
    fixture.close().await;
}
