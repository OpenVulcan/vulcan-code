//! Service publication tests for frozen plugin conversation routes.
//! 冻结插件会话路由的服务发布测试。

use super::*;
use vulcan_hooks::{FnHookHandler, HookInvocation, HookMode, HookRegistration, HookResult};

pub(in crate::skills_services) mod published;
mod shutdown;

/// Verifies real service publication, settings preservation, and failed preparation isolation.
/// 验证真实服务发布、设置更新保留及准备失败隔离。
#[tokio::test]
async fn service_hook_generation_publication_preserves_captured_routes() {
    // The fixture provides ordinary service dependencies without connecting any provider.
    // 夹具提供普通服务依赖，不连接任何供应商。
    let mut bridge = crate::tests::test_support::test_agent_runtime_bridge();
    bridge.extension_generation = bridge
        .prepare_plugin_extension_generation(
            bridge.vulcan_plugin_runtime.snapshot(),
            bridge.extension_generation.mcp_runtime(),
        )
        .expect("hook generation fixture operation should succeed");
    // Old executions retain this exact bridge before the later service publication.
    // 旧执行在后续服务发布前保留本精确桥。
    let old = bridge.clone();
    // Base registration represents an unpublished candidate that must never leak into captured routes.
    // 基础注册表示未发布候选，不得泄漏到捕获的路由。
    let staged_id = "vulcan-plugin/unpublished-test";
    bridge
        .conversation_hook_runtime
        .lock()
        .expect("hook generation fixture operation should succeed")
        .dispatcher_mut()
        .register(
            HookRegistration::new(staged_id, "conversation.turn.precheck", HookMode::Observe),
            Arc::new(FnHookHandler::new(|_: HookInvocation| async {
                Ok(HookResult::default())
            })),
        )
        .expect("hook generation fixture operation should succeed");
    // A live host registration must remain visible despite the frozen empty plugin partition.
    // 即使插件分区冻结为空，实时宿主注册仍须可见。
    let host_id = "host/published-test";
    bridge
        .conversation_hook_runtime
        .lock()
        .expect("hook generation fixture operation should succeed")
        .dispatcher_mut()
        .register(
            HookRegistration::new(host_id, "conversation.turn.precheck", HookMode::Observe),
            Arc::new(FnHookHandler::new(|_: HookInvocation| async {
                Ok(HookResult::default())
            })),
        )
        .expect("hook generation fixture operation should succeed");
    // This state is the actual owner used by the production publisher.
    // 此状态是真实生产发布器使用的所有者。
    let mut service =
        crate::tests::test_support::test_service_state_with_parent_session(SessionId::new_v4());
    service.agent_runtime = Some(bridge);
    // Shared state serializes the one authoritative extension pointer.
    // 共享状态串行化唯一权威扩展指针。
    let state = Arc::new(Mutex::new(service));
    // The explicit catalog refresh creates a new catalog identity without executing Lua.
    // 显式目录刷新创建新目录身份，不执行 Lua。
    let catalog = old
        .vulcan_plugin_runtime
        .refresh_catalog(&Default::default(), &Default::default())
        .expect("hook generation fixture operation should succeed");
    publish_extension_generation(
        &state,
        "unpublished-test",
        Arc::clone(&catalog),
        old.extension_generation.mcp_runtime(),
    )
    .expect("hook generation fixture operation should succeed");
    // New readers capture the published pointer, while old readers keep their previous Arc.
    // 新读者捕获已发布指针，旧读者保留先前 Arc。
    let published = state
        .lock()
        .expect("hook generation fixture operation should succeed")
        .agent_runtime
        .as_ref()
        .expect("hook generation fixture operation should succeed")
        .clone();
    assert_eq!(
        published.extension_generation.revision(),
        old.extension_generation.revision() + 1
    );
    assert!(Arc::ptr_eq(
        &catalog,
        &published
            .extension_generation
            .plugin_catalog()
            .expect("hook generation fixture operation should succeed")
    ));
    assert!(!Arc::ptr_eq(
        &old.extension_generation,
        &published.extension_generation
    ));
    for captured in [&old, &published] {
        // Metadata comes from the same scoped dispatcher later cloned for actual execution.
        // 元数据来自稍后为真实执行克隆的同一作用域调度器。
        let snapshots = captured
            .lock_conversation_hooks()
            .expect("hook generation fixture operation should succeed")
            .precheck_hook_snapshots();
        assert!(
            snapshots
                .iter()
                .any(|entry| entry.registration.id == host_id)
        );
        assert!(
            !snapshots
                .iter()
                .any(|entry| entry.registration.id == staged_id)
        );
    }
    assert!(
        published
            .conversation_hook_runtime
            .lock()
            .expect("hook generation fixture operation should succeed")
            .dispatcher_mut()
            .registry()
            .registration_snapshots(None)
            .iter()
            .any(|entry| entry.registration.id == staged_id)
    );
    crate::settings_runtime_projection::store_provider_and_mcp_configuration_after_change(
        &state,
        published.provider_configuration.clone(),
        published.extension_generation.mcp_runtime(),
    )
    .expect("hook generation fixture operation should succeed");
    // A user MCP settings replacement must not drop or recreate plugin hook ownership.
    // 用户 MCP 设置替换不得丢弃或重建插件 Hook 所有权。
    let settings = state
        .lock()
        .expect("hook generation fixture operation should succeed")
        .agent_runtime
        .as_ref()
        .expect("hook generation fixture operation should succeed")
        .clone();
    assert!(std::ptr::eq(
        settings
            .extension_generation
            .plugin_conversation_hooks()
            .expect("hook generation fixture operation should succeed"),
        published
            .extension_generation
            .plugin_conversation_hooks()
            .expect("hook generation fixture operation should succeed")
    ));
    // Poison the actual hook mutex to make publication preparation fail before pointer replacement.
    // 使真实 Hook 互斥锁中毒，让发布准备在指针替换前失败。
    let poisoned = Arc::clone(&settings.conversation_hook_runtime);
    assert!(
        std::thread::spawn(move || {
            // Keep ownership across the intentional panic so the mutex records the failure.
            // 在故意恐慌期间保持所有权，使互斥锁记录失败。
            let _guard = poisoned
                .lock()
                .expect("fixture runtime or observation lock should remain healthy");
            panic!("intentional hook publication fixture failure");
        })
        .join()
        .is_err()
    );
    assert!(
        publish_extension_generation(
            &state,
            "unpublished-test",
            catalog,
            settings.extension_generation.mcp_runtime()
        )
        .is_err()
    );
    assert!(Arc::ptr_eq(
        &settings.extension_generation,
        &state
            .lock()
            .expect("hook generation fixture operation should succeed")
            .agent_runtime
            .as_ref()
            .expect("hook generation fixture operation should succeed")
            .extension_generation
    ));
}
