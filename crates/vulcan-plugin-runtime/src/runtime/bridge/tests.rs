//! Direct tests for fixed host tool names and typed bridge request envelopes.
//! 固定宿主工具名称与强类型桥接请求信封的直接测试。

use super::super::plugin_permission_key;
use super::{
    VULCAN_VLDB_SQLITE_EXECUTE_BATCH_V1_TOOL, VULCAN_VLDB_SQLITE_EXECUTE_V1_TOOL,
    VULCAN_VLDB_SQLITE_FLUSH_V1_TOOL, VULCAN_VLDB_SQLITE_QUERY_V1_TOOL,
    bridge_request_identity_and_payload, plugin_vldb_operation,
};
use crate::{VulcanPluginPermission, VulcanPluginVldbOperation};
use serde_json::json;

/// Verifies fixed bridge requests preserve strict JSON object payload semantics.
/// 验证固定桥请求严格保留 JSON 对象载荷语义。
#[test]
fn bridge_request_requires_object_payload() {
    // objectRequest represents the typed object emitted by LuaSkills 0.5.5.
    // objectRequest 表示 LuaSkills 0.5.5 发出的强类型对象。
    let object_request = json!({"plugin_id": "fixture", "payload": {}});
    // extracted is the exact borrowed identity and object payload.
    // extracted 是精确借用的身份与对象载荷。
    let (plugin_id, payload) =
        bridge_request_identity_and_payload(&object_request).expect("accept object bridge payload");
    assert_eq!(plugin_id, "fixture");
    assert!(payload.is_empty());

    // arrayRequest proves the removed Lua empty-table compatibility path cannot return.
    // arrayRequest 证明已移除的 Lua 空表兼容路径不会重新出现。
    let array_request = json!({"plugin_id": "fixture", "payload": []});
    // error is the stable protocol rejection for a non-object payload.
    // error 是非对象载荷对应的稳定协议拒绝信息。
    let error = bridge_request_identity_and_payload(&array_request)
        .expect_err("reject array bridge payload");
    assert_eq!(
        error,
        "Vulcan plugin host request requires object field `payload`."
    );
}

/// Verifies every public VLDB host name selects one immutable operation and rejects aliases.
/// 验证每个公开 VLDB 宿主名称都选择唯一不可变操作，并拒绝别名。
#[test]
fn vldb_host_tool_names_map_to_exact_operations() {
    assert_eq!(
        plugin_vldb_operation(VULCAN_VLDB_SQLITE_QUERY_V1_TOOL),
        Some(VulcanPluginVldbOperation::SqliteQuery)
    );
    assert_eq!(
        plugin_vldb_operation(VULCAN_VLDB_SQLITE_EXECUTE_V1_TOOL),
        Some(VulcanPluginVldbOperation::SqliteExecute)
    );
    assert_eq!(
        plugin_vldb_operation(VULCAN_VLDB_SQLITE_EXECUTE_BATCH_V1_TOOL),
        Some(VulcanPluginVldbOperation::SqliteExecuteBatch)
    );
    assert_eq!(
        plugin_vldb_operation(VULCAN_VLDB_SQLITE_FLUSH_V1_TOOL),
        Some(VulcanPluginVldbOperation::SqliteFlush)
    );
    assert_eq!(plugin_vldb_operation("vulcan.vldb.sqlite.query"), None);
    assert_eq!(
        plugin_permission_key(VulcanPluginPermission::SystemVldbAccess),
        "system_vldb_access"
    );
}
