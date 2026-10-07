//! Automatic prewarm targets share ordinary reusable routing and controller/workspace partitions.
//! 自动预热目标共享普通复用路由及控制器／工作区分区。

use super::*;

/// Target admission rejects non-reusable, stale, foreign and ownerless requests before creating any VM.
/// 目标入场在创建任何 VM 前拒绝非复用、陈旧、外部及无所有者请求。
#[tokio::test]
async fn hot_reuse_target_validates_authority_and_preserves_partitions() {
    // Use one real reusable package and the production enabled catalog.
    // 使用单个真实可复用包及生产启用目录。
    let fixture = Fixture::new(10_000, 100);
    // Capture the declared generation before issuing either invalid or valid work.
    // 在发出无效或有效工作前捕获声明代次。
    let generation = fixture.target(1).plugin_generation;
    for (plugin, tool, selected, controller) in [
        ("idle-plugin", "idle_open", generation, "owner"),
        ("idle-plugin", "missing", generation, "owner"),
        (
            "idle-plugin",
            "idle_ping",
            generation.checked_add(1).expect("different generation"),
            "owner",
        ),
        ("idle-plugin", "idle_ping", generation, " "),
        ("foreign-plugin", "idle_ping", generation, "owner"),
    ] {
        // Every request keeps the actual existing workspace, never a fallback directory.
        // 每个请求保留真实现有工作区，绝不使用回退目录。
        let mut request = fixture.target(1);
        request.plugin_id = plugin.into();
        request.tool_id = tool.into();
        request.plugin_generation = selected;
        request.owner_session_id = controller.into();
        fixture
            .prepare_target(request)
            .expect_err("invalid authority");
    }
    assert_eq!(
        fixture
            .owner()
            .host
            .runtime()
            .resources()
            .expect("no VM")
            .resident,
        0
    );
    assert!(
        fixture
            .manager
            .formal_calls("owner")
            .expect("no admitted calls")
            .is_empty()
    );
    let workspace = fixture.root.path().to_path_buf();
    fixture.converge_target(fixture.target(1)).await;
    assert_eq!(
        fixture
            .invoke("other", workspace.clone(), "ok")
            .await
            .expect("other controller")["count"],
        json!(1)
    );
    assert_eq!(
        fixture
            .invoke("owner", workspace.clone(), "ok")
            .await
            .expect("warm controller")["count"],
        json!(1)
    );
    // A different real directory creates a distinct domain even for the same controller and tool.
    // 即使控制器及工具相同，不同真实目录仍创建不同域。
    let other = workspace.join("other");
    std::fs::create_dir(&other).expect("other workspace");
    let mut request = fixture.target(1);
    request.workspace_root = other.clone();
    fixture.converge_target(request).await;
    assert_eq!(
        fixture
            .invoke("owner", other, "ok")
            .await
            .expect("other workspace business")["count"],
        json!(1)
    );
    assert_eq!(
        fixture
            .invoke("owner", workspace, "ok")
            .await
            .expect("original state")["count"],
        json!(2)
    );
    fixture.close().await;
}
