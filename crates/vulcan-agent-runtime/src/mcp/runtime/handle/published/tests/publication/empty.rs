//! An explicit empty original producer has a full source without claiming that a conversation process was started.
//! 显式空原生产者具有完整来源，不宣称会话进程已启动。

use super::*;
use crate::mcp::plugin::tests::credential_binding::candidates::runtime::published_fixture::published_fixture_with_scope;
use vulcan_plugin_runtime::VulcanPluginMcpScopeManifest;

/// An actual conversation-only source publishes an explicitly empty global factory without fabricating a native launch.
/// 实际仅会话来源发布显式空全局工厂，不伪造原生启动。
/// No inputs or return value; real sealed materialization, registry source and unchanged marker establish this empty case.
/// 无输入及返回值；真实密封物化、注册表来源及不变标记确立此空分支。
#[tokio::test]
async fn published_full_pointer_empty_global_factory_keeps_explicit_original_source() {
    // The actual immutable manifest declares a conversation server before full selection and resource preparation.
    // 实际不可变清单在完整选择及资源准备之前声明会话服务器。
    let PublishedFixture {
        manager,
        configs,
        authority,
        marker,
        _temp,
        ..
    } = published_fixture_with_scope("ready", VulcanPluginMcpScopeManifest::Conversation);
    assert!(
        configs.servers().is_empty(),
        "conversation-only declaration must have no global materialization"
    );
    // Source metadata comes from the actual owner even though it has no first global server.
    // 即使没有首个全局服务器，来源元数据也来自实际所有者。
    let catalog = manager.snapshot();
    // The original empty bootstrap still supplies real shared admission and original observation tables.
    // 原空引导仍提供真实共享入场及原观察表。
    let access = bootstrap_mcp_runtime_access(Vec::new())
        .await
        .expect("actual empty host lineage");
    // The full original slot is a genuine immutable extension rather than a guessed NeverStarted state.
    // 完整原槽为真实不可变扩展，而非猜测的 NeverStarted 状态。
    let expected = full_extension(&manager, &catalog, catalog.revision, access.clone());
    // The same sealed source producer allocates and registers this explicitly empty actual global instance.
    // 同一密封来源生产者分配并登记此显式空实际全局实例。
    let native = PublishedPluginMcpRuntime::start(&access, &manager, configs, &authority)
        .expect("actual empty global producer");
    native
        .observer()
        .wait_until_ready(OBSERVE)
        .await
        .expect("explicit empty producer ready");
    assert!(
        native
            .observation
            .runtime
            .inner
            .registry
            .enabled_servers()
            .is_empty()
    );
    assert!(
        !marker.exists(),
        "explicit empty global factory must not start a conversation child"
    );
    // Same empty actual lease and full handlers are prepared before native publication exclusions.
    // 相同空实际租约及完整处理器在原生发布互斥之前准备。
    let next = full_extension(
        &manager,
        &catalog,
        expected.revision() + 1,
        native
            .publication_access(&access)
            .expect("same explicit empty lease"),
    );
    // The actual pointer replacement confirms only this producer's original source and empty global lease.
    // 实际指针替换仅确认此生产者的原来源及空全局租约。
    let mut slot = Arc::clone(&expected);
    native
        .prepare_publication(&manager, &authority, &catalog)
        .expect("same actual empty producer source")
        .prepare_extension(&expected, next)
        .expect("complete empty global successor")
        .publish_into(&mut slot)
        .expect("actual empty full publication");
    assert!(native.matches_original_publication(&slot));
    assert!(
        slot.mcp_runtime()
            .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
            .is_empty()
    );
    assert!(
        native
            .observer()
            .verify_source("bound-plugin:local", &native.observer().profile())
            .is_err()
    );
    assert!(
        !marker.exists(),
        "full empty publication must not fabricate or replay a native start"
    );
    native.shutdown();
    native
        .observer()
        .wait_until_released(OBSERVE)
        .await
        .expect("actual empty original cleanup");
}
