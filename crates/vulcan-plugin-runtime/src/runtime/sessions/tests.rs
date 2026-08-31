//! Regression tests for bounded idle retirement and stale-generation close.
//! 有界空闲退役与陈旧代次关闭的回归测试。

use super::super::tests::{test_manager, write_idle_session_package};
use super::VulcanPluginInvocationRequest;
use serde_json::json;
use std::collections::BTreeSet;
use std::fs;
use std::time::Duration;

/// Verifies the host reaps an idle owner-bound session without another tool invocation.
/// 验证宿主无需额外工具调用即可回收空闲的所有者绑定会话。
#[test]
fn idle_session_timeout_reaps_managed_lease() {
    let temp = tempfile::tempdir().expect("create temp directory");
    let runtime_root = temp.path().join("runtime");
    let vulcan_plugin_root = runtime_root.join("plugins");
    fs::create_dir_all(&vulcan_plugin_root).expect("create Vulcan Plugin root");
    write_idle_session_package(&vulcan_plugin_root);
    let manager = test_manager(&runtime_root, vulcan_plugin_root);
    manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("load idle plugin catalog");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("create Tokio test runtime");
    let stateless = runtime
        .block_on(manager.invoke(VulcanPluginInvocationRequest {
            tool_id: String::from("idle_ping"),
            arguments: json!({}),
            owner_session_id: String::from("owner-1"),
            workspace_root: temp.path().to_path_buf(),
        }))
        .expect("invoke stateless tool");
    assert_eq!(stateless["accepted"], true);
    assert_eq!(
        manager.snapshot().plugins["idle-plugin"].active_session_count,
        0
    );
    runtime
        .block_on(manager.invoke(VulcanPluginInvocationRequest {
            tool_id: String::from("idle_open"),
            arguments: json!({}),
            owner_session_id: String::from("owner-1"),
            workspace_root: temp.path().to_path_buf(),
        }))
        .expect("open idle session");
    assert_eq!(
        manager.snapshot().plugins["idle-plugin"].active_session_count,
        1
    );
    runtime.block_on(async {
        // The reaper starts at the one-second deadline, while asynchronous shutdown may finish later under full-workspace load.
        // 回收器在一秒截止点启动，而异步关闭在全仓负载下可能稍后才完成。
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if manager.snapshot().plugins["idle-plugin"].active_session_count == 0 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("idle session should be reaped within the bounded shutdown window");
    });
    assert_eq!(
        manager.snapshot().plugins["idle-plugin"].active_session_count,
        0
    );
}

/// Verifies a current close tool retires an owned session from an older package generation.
/// 验证当前关闭工具可以回收属于旧插件代次的所有者会话。
#[test]
fn close_tool_retires_stale_generation_session() {
    let temp = tempfile::tempdir().expect("create temp directory");
    let runtime_root = temp.path().join("runtime");
    let vulcan_plugin_root = runtime_root.join("plugins");
    fs::create_dir_all(&vulcan_plugin_root).expect("create Vulcan Plugin root");
    write_idle_session_package(&vulcan_plugin_root);
    let manager = test_manager(&runtime_root, vulcan_plugin_root.clone());
    manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("load idle plugin catalog");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("create Tokio test runtime");
    let opened = runtime
        .block_on(manager.invoke(VulcanPluginInvocationRequest {
            tool_id: String::from("idle_open"),
            arguments: json!({}),
            owner_session_id: String::from("owner-stale"),
            workspace_root: temp.path().to_path_buf(),
        }))
        .expect("open stale fixture session");
    let session_id = opened["session_id"]
        .as_str()
        .expect("read opened session id")
        .to_owned();
    fs::write(
        vulcan_plugin_root.join("idle-plugin/dependencies.yaml"),
        "# generation change\n",
    )
    .expect("change package fingerprint");
    manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("refresh changed package");

    let closed = runtime
        .block_on(manager.invoke(VulcanPluginInvocationRequest {
            tool_id: String::from("idle_close"),
            arguments: json!({ "session_id": session_id }),
            owner_session_id: String::from("owner-stale"),
            workspace_root: temp.path().to_path_buf(),
        }))
        .expect("close stale fixture session");
    assert_eq!(closed["closed"], true);
    assert_eq!(closed["result"]["closed_stale_generation"], true);
    assert_eq!(
        manager.snapshot().plugins["idle-plugin"].active_session_count,
        0
    );
}
