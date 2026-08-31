//! Tests for persistent category isolation and plugin-generation retirement.
//! 持久运行时类别隔离与插件代次退役测试。

use super::super::VulcanPluginActionRequest;
use super::super::tests::{test_manager, write_hook_package};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use uuid::Uuid;
use vulcan_hooks::{
    HookActor, HookCapabilitySet, HookContext, HookInvocation, HookMode, HookPayload,
    HookRegistration, HookScope,
};

/// Builds one exact host invocation for the fixture hook.
/// 为夹具 hook 构建一次精确宿主调用。
/// registration_id selects the fixture registration; returns one complete host-owned invocation.
/// registration_id选择夹具注册项；返回一份完整的宿主调用。
fn hook_invocation(registration_id: &str) -> HookInvocation {
    HookInvocation {
        registration: HookRegistration::new(
            registration_id,
            "conversation.turn.precheck",
            HookMode::Transform,
        ),
        context: HookContext {
            run_id: Uuid::new_v4().to_string(),
            point: String::from("conversation.turn.precheck"),
            scope: HookScope::Session,
            actor: HookActor::new("runtime-test"),
            workspace_id: None,
            project_id: None,
            session_id: Some(String::from("session-1")),
            conversation_id: Some(String::from("session-1")),
            user_id: None,
            capabilities: HookCapabilitySet::default(),
            metadata: BTreeMap::new(),
        },
        payload: HookPayload::empty_object(),
    }
}

/// Verifies hook calls reuse persistent Lua state and draining rejects stale generations.
/// 验证 hook 调用会复用持久 Lua 状态，且 draining 会拒绝陈旧代次。
#[test]
fn hook_runtime_reuses_state_and_drains_by_plugin_generation() {
    let temp = tempfile::tempdir().expect("create temp directory");
    let runtime_root = temp.path().join("runtime");
    let vulcan_plugin_root = runtime_root.join("plugins");
    fs::create_dir_all(&vulcan_plugin_root).expect("create Vulcan Plugin root");
    write_hook_package(&vulcan_plugin_root);
    let manager = test_manager(&runtime_root, vulcan_plugin_root);
    let catalog = manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("load hook catalog");
    let hook = catalog
        .enabled_hooks
        .values()
        .next()
        .cloned()
        .expect("resolve enabled hook");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("create Tokio test runtime");
    let first = runtime
        .block_on(manager.invoke_hook(&hook, hook_invocation(&hook.registration_id)))
        .expect("invoke first hook");
    let ui_action = runtime
        .block_on(manager.invoke_action(VulcanPluginActionRequest {
            plugin_id: String::from("hook-plugin"),
            action_id: String::from("ui-action"),
            arguments: json!({}),
            caller: crate::VulcanPluginActionCaller::PluginUi,
            workspace_root: runtime_root.clone(),
            reuse_enabled_runtime: true,
        }))
        .expect("invoke persistent UI action");
    let second = runtime
        .block_on(manager.invoke_hook(&hook, hook_invocation(&hook.registration_id)))
        .expect("invoke second hook");
    assert_eq!(first.payload.expect("first payload").value["count"], 1);
    assert_eq!(ui_action["count"], 100);
    assert_eq!(second.payload.expect("second payload").value["count"], 2);
    manager
        .begin_draining("hook-plugin")
        .expect("begin plugin draining");
    let stale_error = runtime
        .block_on(manager.invoke_hook(&hook, hook_invocation(&hook.registration_id)))
        .expect_err("stale hook must be rejected");
    assert!(stale_error.message.contains("not enabled"));
    let report = runtime
        .block_on(manager.drain_plugin_sessions("hook-plugin"))
        .expect("drain hook runtime");
    assert_eq!(report.closed_hook_runtimes, 1);
    assert_eq!(report.closed_ui_action_runtimes, 1);
    assert!(report.failures.is_empty());
}
