//! Retained routing metadata must share admission capacity with real calls and sessions.
//! 保留路由元数据必须与真实调用和会话共享入场容量。

use super::*;

/// Fill real idle domains, reject over-budget calls and sessions, then reclaim only through actual drain.
/// 填满真实空闲域，拒绝超预算调用及会话，仅通过实际排空回收。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hot_reuse_retained_routing_bytes_bound_all_admission_paths() {
    // Keep enough physical slots so this test reaches bytes before VM or metadata count limits.
    // 保留足够物理槽，使此测试先触及字节而非 VM 或元数据数量限制。
    let mut settings = VulcanPluginRuntimeSettings::default();
    settings.parent.max_queued_bytes = 32 * 1024;
    settings.parent.max_value_bytes = 8 * 1024;
    // A long idle lifetime prevents eviction from making an invalid admission appear safe.
    // 长空闲寿命防止驱逐使无效入场看起来安全。
    let fixture = Fixture::with_settings(60_000, 100, settings);
    let owner = fixture.owner();
    let workspace = fixture.root.path().to_path_buf();
    let mut controllers = Vec::new();
    let mut rejected = None;
    for sequence in 0..owner.settings.parent.max_registered_pools {
        // Distinct long caller identities are real retained input, not inflated synthetic counters.
        // 不同长调用者身份是真实保留输入，而非膨胀的合成计数。
        let controller = format!("controller-{sequence}-{}", "x".repeat(2048));
        match fixture.invoke(&controller, workspace.clone(), "ok").await {
            Ok(result) => {
                assert_eq!(result["count"], json!(1));
                assert!(
                    fixture
                        .manager
                        .formal_calls(&controller)
                        .expect("call query")
                        .is_empty()
                );
                controllers.push(controller);
            }
            Err(error) => {
                assert!(error.to_string().contains("input byte capacity"), "{error}");
                rejected = Some(controller);
                break;
            }
        }
    }
    let rejected = rejected.expect("retained bytes must reject before the domain count limit");
    assert!(
        controllers.len() > 1,
        "multiple real idle domains must consume the shared budget"
    );
    let retained = owner
        .reusable
        .lock()
        .expect("domain registry")
        .retained_input_bytes()
        .expect("checked routing bytes");
    assert!(retained > 0 && retained <= owner.settings.parent.max_queued_bytes);
    assert!(
        fixture
            .manager
            .reusable_tool_domains(&rejected)
            .expect("rejected domain query")
            .is_empty()
    );
    // Any preparation failure still owns its result until the real call finishes and is acknowledged.
    // 任何准备失败仍拥有其结果，直到真实调用结束并被确认。
    for call in fixture
        .manager
        .formal_calls(&rejected)
        .expect("failed call query")
    {
        assert!(
            call.retired,
            "byte rejection must precede native registration"
        );
        fixture
            .manager
            .forget_formal_call(&call.invocation_id, &rejected)
            .expect("acknowledge original rejected call");
    }
    let projected = controllers
        .iter()
        .map(|controller| {
            fixture
                .manager
                .reusable_tool_domains(controller)
                .expect("domain projection")
                .into_iter()
                .map(|domain| {
                    assert!(!domain.preparing && !domain.closing && domain.borrowers == 0);
                    domain.retained_input_bytes
                })
                .sum::<usize>()
        })
        .sum::<usize>();
    assert_eq!(
        retained, projected,
        "public numeric projection must cover all live domains"
    );
    // This request fits an empty parent but exceeds the remaining space occupied by actual cached routing.
    // 此请求可放入空父级，但会超过真实缓存路由占用后的剩余空间。
    let large_controller = "y".repeat(owner.settings.parent.max_queued_bytes / 3);
    let pool_count = owner
        .reusable
        .lock()
        .expect("domain registry")
        .domains
        .len();
    let error = fixture
        .invoke(&large_controller, workspace.clone(), "ok")
        .await
        .expect_err("ordinary admission must include retained routing");
    assert!(error.to_string().contains("input byte capacity"), "{error}");
    let error = fixture
        .manager
        .invoke(VulcanPluginInvocationRequest {
            tool_id: "idle_open".to_owned(),
            arguments: json!({}),
            owner_session_id: large_controller.clone(),
            workspace_root: workspace.clone(),
        })
        .await
        .expect_err("pinned admission must include retained routing");
    assert!(error.to_string().contains("input byte capacity"), "{error}");
    assert!(owner.calls.lock().expect("call registry").is_empty());
    assert!(owner.pinned.lock().expect("session registry").is_empty());
    assert_eq!(
        owner
            .reusable
            .lock()
            .expect("domain registry")
            .domains
            .len(),
        pool_count
    );
    // Draining uses the original pool identities and actual finalizers before releasing cached input.
    // 排空使用原池身份及实际关闭器，之后才释放缓存输入。
    assert!(
        fixture
            .manager
            .drain_reusable_domains(Some("idle-plugin"), None)
            .await
            .expect("actual domain drain")
            .failures
            .is_empty()
    );
    assert_eq!(
        owner
            .reusable
            .lock()
            .expect("domain registry")
            .retained_input_bytes()
            .expect("empty checked routing bytes"),
        0
    );
    assert_eq!(
        fixture
            .invoke("fresh-owner", workspace, "ok")
            .await
            .expect("released budget admits new work")["count"],
        json!(1)
    );
    fixture.close().await;
}
