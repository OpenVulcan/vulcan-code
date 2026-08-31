# 06：插件运行时测试保全基线

冻结日期2026-08-31；原runtime.rs 9716行，SHA-256为c442f9145589f8e966babe875d7041b3c4e12d4fc6e7a60525901ddd2433b562。Windows实际发现59包级测试，其中runtime23项，catalog36项；59通过、0失败/0忽略，4.97秒。没有为了拆分修正或删除测试。

## 原运行时用例

| 原路径 | 原始行 | 属性 |
| --- | ---: | --- |
| `runtime::tests::bridge_request_requires_object_payload` | 7578 | `#[test]` |
| `runtime::tests::vldb_host_tool_names_map_to_exact_operations` | 7605 | `#[test]` |
| `runtime::tests::content_plugin_operates_without_lua_runtime_or_execution_environment` | 8116 | `#[test]` |
| `runtime::tests::enable_rollback_uses_declared_disable_callback_in_enabling_scope` | 8206 | `#[test]` |
| `runtime::tests::draining_rollback_reenters_enabling_before_probe` | 8248 | `#[test]` |
| `runtime::tests::hook_runtime_reuses_state_and_drains_by_plugin_generation` | 8289 | `#[test]` |
| `runtime::tests::bundled_usage_statistics_plugin_loads_through_system_vldb_bridge` | 8345 | `#[test]` |
| `runtime::tests::bundled_context7_plugin_loads_with_typed_json_surface` | 8613 | `#[test]` |
| `runtime::tests::bundled_rtk_plugin_uses_generic_action_and_hook_boundaries` | 8648 | `#[cfg(target_os = "windows")]` `#[test]` |
| `runtime::tests::user_plugin_permission_preview_includes_sanitized_mcp_ownership` | 8838 | `#[test]` |
| `runtime::tests::user_plugin_install_binds_authorization_to_exact_content_fingerprint` | 8902 | `#[test]` |
| `runtime::tests::user_plugin_update_binds_authorization_to_exact_content_fingerprint` | 8960 | `#[test]` |
| `runtime::tests::user_plugin_install_environment_and_uninstall_are_transactional` | 9056 | `#[test]` |
| `runtime::tests::user_plugin_update_supports_exact_rollback_and_commit` | 9112 | `#[test]` |
| `runtime::tests::user_plugin_environment_rejects_post_authorization_mutation` | 9243 | `#[test]` |
| `runtime::tests::idle_session_timeout_reaps_managed_lease` | 9314 | `#[test]` |
| `runtime::tests::close_tool_retires_stale_generation_session` | 9376 | `#[test]` |
| `runtime::tests::explicit_close_failure_publishes_cleanup_diagnostic` | 9429 | `#[test]` |
| `runtime::tests::concurrent_session_drain_waits_for_close_owner` | 9494 | `#[test]` |
| `runtime::tests::lifecycle_rollback_preserves_cleanup_diagnostic_for_active_session` | 9564 | `#[test]` |
| `runtime::tests::same_version_package_change_advances_plugin_generation` | 9626 | `#[test]` |
| `runtime::tests::retained_lease_retry_claim_is_atomic` | 9651 | `#[test]` |
| `runtime::tests::plugin_process_spec_normalizes_windows_verbatim_paths` | 9695 | `#[cfg(windows)]` `#[test]` |

## 原辅助节点

| 节点 | 原始行 |
| --- | ---: |
| `struct:RecordingVldbService` | 7632 |
| `VulcanPluginVldbService for RecordingVldbService::execute` | 7651 |
| `write_hook_package` | 7815 |
| `write_idle_session_package` | 7888 |
| `write_user_plugin_package` | 7968 |
| `write_content_plugin_package` | 8001 |
| `test_engine` | 8024 |
| `test_manager` | 8056 |
| `hook_invocation` | 8089 |

## 基线命令与边界

- rtk proxy cargo test -p vulcan-plugin-runtime --lib -- --list：59项，无忽略；两项运行时用例有Windows条件，其他平台列表会按原cfg变化。
- rtk proxy cargo test -p vulcan-plugin-runtime --lib -- --test-threads=1 --quiet：59通过，4.97秒；涉及共享Lua回调/运行时，因此串行。
- 编译仅有MSVC创建导入库的linker_messages说明，测试正常终止。
- 环境、安装和桥接测试使用现有隔离夹具；不将其称为真实在线注册表依赖安装、全部平台运行或完整桌面验收。
- 原23运行时测试及9辅助节点将全量逐节点核验；当前不存在可按证据移除的无效测试。
