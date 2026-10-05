//! Real owner-process status stays observable without joining global tool routing.
//! 真实所有者进程状态保持可观察，且不进入全局工具路由。

use super::*;
use owner_drain_tests::{OBSERVE, access, child_released, server};

/// No inputs; verifies startup, live catalogs, multiple owners and cleanup using real stdio children.
/// 无输入；使用真实 stdio 子进程验证启动、实时目录、多所有者及清理。
#[tokio::test]
async fn plugin_status_observes_conversation_instances_without_global_routing() {
    // The original admission record exists before readiness or owner publication.
    // 原准入记录早于就绪与所有者发布存在。
    let access = access();
    let witness = tempfile::NamedTempFile::new().expect("child witness");
    let key = McpRuntimeOwnedPartitionKey::try_new("status", 5, SessionId::new_v4())
        .expect("valid owner");
    let (runtime, resources) = access
        .start_owned_stdio_runtime(&key, vec![server("status", witness.path(), true)])
        .expect("registered start");
    assert!(access.plugin_server_states().iter().any(|state| {
        state.id == "status:owner" && state.status == McpRuntimeServerStatus::Connecting
    }));
    runtime
        .wait_until_ready(OBSERVE)
        .await
        .expect("real MCP ready");
    // The second session owns its own published connection to the same manifest server.
    // 第二个会话拥有同一清单服务器的独立已发布连接。
    let second_witness = tempfile::NamedTempFile::new().expect("second child witness");
    let second = access
        .open_owned_stdio_partition(
            "status",
            5,
            SessionId::new_v4(),
            vec![server("status", second_witness.path(), true)],
        )
        .await
        .expect("second owner ready");
    // Management sees both instances while the ordinary global catalog still sees neither.
    // 管理端看到两个实例，而普通全局目录仍不包含任何一个。
    let states = access.plugin_server_states();
    assert_eq!(states.len(), 2);
    assert!(states.iter().all(|state| {
        state.id == "status:owner"
            && state.generation == Some(5)
            && state.status == McpRuntimeServerStatus::Connected
            && state.tool_count == 1
    }));
    assert!(access.server_states().is_empty());
    access
        .begin_plugin_owner_drain("status")
        .expect("begin drain");
    assert!(
        access
            .plugin_server_states()
            .iter()
            .all(|state| { state.status == McpRuntimeServerStatus::Stopping })
    );
    drop(resources);
    drop(second);
    access
        .wait_plugin_owner_drain("status", OBSERVE)
        .await
        .expect("drain originals");
    assert!(access.plugin_server_states().is_empty());
    child_released(&witness);
    child_released(&second_witness);
}
