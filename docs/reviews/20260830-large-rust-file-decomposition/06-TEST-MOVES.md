# 06：插件运行时测试与辅助节点映射

本文件保留第四阶段历史快照。23原测试及9辅助节点最终归属见[最终完整映射](06-FINAL-MOVES.md)；Context7真实加载修复和59包级回归见[完整终审](06-FINAL-REVIEW.md)。

## 第4阶段保全

23原运行时测试在Windows仍全部可发现，7项随实际职责迁移，16项跨域用例留根；9辅助节点唯一保留，原Windows条件和全部测试属性不变。

21测试体严格等价；安装指纹拒绝和安装/卸载事务两项修正旧授权路径，后一项新增删除前存在性断言，详见06-TEST-REPAIRS.md。没有删除、复制或弱化有效测试，不将这两个有意修复说成原内容完全不变。

## 全量测试路径

| 原路径 | 当前路径 | 当前产物行 | 内容核验 |
| --- | --- | --- | --- |
| `runtime::tests::bridge_request_requires_object_payload` | `runtime::tests::bridge_request_requires_object_payload` | `crates/vulcan-plugin-runtime/src/runtime.rs:4539` | 严格等价 |
| `runtime::tests::vldb_host_tool_names_map_to_exact_operations` | `runtime::tests::vldb_host_tool_names_map_to_exact_operations` | `crates/vulcan-plugin-runtime/src/runtime.rs:4566` | 严格等价 |
| `runtime::tests::content_plugin_operates_without_lua_runtime_or_execution_environment` | `runtime::tests::content_plugin_operates_without_lua_runtime_or_execution_environment` | `crates/vulcan-plugin-runtime/src/runtime.rs:5077` | 严格等价 |
| `runtime::tests::enable_rollback_uses_declared_disable_callback_in_enabling_scope` | `runtime::tests::enable_rollback_uses_declared_disable_callback_in_enabling_scope` | `crates/vulcan-plugin-runtime/src/runtime.rs:5167` | 严格等价 |
| `runtime::tests::draining_rollback_reenters_enabling_before_probe` | `runtime::tests::draining_rollback_reenters_enabling_before_probe` | `crates/vulcan-plugin-runtime/src/runtime.rs:5209` | 严格等价 |
| `runtime::tests::hook_runtime_reuses_state_and_drains_by_plugin_generation` | `runtime::tests::hook_runtime_reuses_state_and_drains_by_plugin_generation` | `crates/vulcan-plugin-runtime/src/runtime.rs:5250` | 严格等价 |
| `runtime::tests::bundled_usage_statistics_plugin_loads_through_system_vldb_bridge` | `runtime::tests::bundled_usage_statistics_plugin_loads_through_system_vldb_bridge` | `crates/vulcan-plugin-runtime/src/runtime.rs:5306` | 严格等价 |
| `runtime::tests::bundled_context7_plugin_loads_with_typed_json_surface` | `runtime::tests::bundled_context7_plugin_loads_with_typed_json_surface` | `crates/vulcan-plugin-runtime/src/runtime.rs:5574` | 严格等价 |
| `runtime::tests::bundled_rtk_plugin_uses_generic_action_and_hook_boundaries` | `runtime::tests::bundled_rtk_plugin_uses_generic_action_and_hook_boundaries` | `crates/vulcan-plugin-runtime/src/runtime.rs:5609` | 严格等价 |
| `runtime::tests::user_plugin_permission_preview_includes_sanitized_mcp_ownership` | `runtime::authorization::tests::user_plugin_permission_preview_includes_sanitized_mcp_ownership` | `crates/vulcan-plugin-runtime/src/runtime/authorization/tests.rs:11` | 严格等价 |
| `runtime::tests::user_plugin_install_binds_authorization_to_exact_content_fingerprint` | `runtime::installation::tests::user_plugin_install_binds_authorization_to_exact_content_fingerprint` | `crates/vulcan-plugin-runtime/src/runtime/installation/tests.rs:12` | 实名旧路径修复；其余内容保全 |
| `runtime::tests::user_plugin_update_binds_authorization_to_exact_content_fingerprint` | `runtime::installation::tests::user_plugin_update_binds_authorization_to_exact_content_fingerprint` | `crates/vulcan-plugin-runtime/src/runtime/installation/tests.rs:70` | 严格等价 |
| `runtime::tests::user_plugin_install_environment_and_uninstall_are_transactional` | `runtime::installation::tests::user_plugin_install_environment_and_uninstall_are_transactional` | `crates/vulcan-plugin-runtime/src/runtime/installation/tests.rs:166` | 实名旧路径修复；其余内容保全 |
| `runtime::tests::user_plugin_update_supports_exact_rollback_and_commit` | `runtime::installation::tests::user_plugin_update_supports_exact_rollback_and_commit` | `crates/vulcan-plugin-runtime/src/runtime/installation/tests.rs:229` | 严格等价 |
| `runtime::tests::user_plugin_environment_rejects_post_authorization_mutation` | `runtime::environment::tests::user_plugin_environment_rejects_post_authorization_mutation` | `crates/vulcan-plugin-runtime/src/runtime/environment/tests.rs:11` | 严格等价 |
| `runtime::tests::idle_session_timeout_reaps_managed_lease` | `runtime::tests::idle_session_timeout_reaps_managed_lease` | `crates/vulcan-plugin-runtime/src/runtime.rs:5799` | 严格等价 |
| `runtime::tests::close_tool_retires_stale_generation_session` | `runtime::tests::close_tool_retires_stale_generation_session` | `crates/vulcan-plugin-runtime/src/runtime.rs:5861` | 严格等价 |
| `runtime::tests::explicit_close_failure_publishes_cleanup_diagnostic` | `runtime::tests::explicit_close_failure_publishes_cleanup_diagnostic` | `crates/vulcan-plugin-runtime/src/runtime.rs:5914` | 严格等价 |
| `runtime::tests::concurrent_session_drain_waits_for_close_owner` | `runtime::tests::concurrent_session_drain_waits_for_close_owner` | `crates/vulcan-plugin-runtime/src/runtime.rs:5979` | 严格等价 |
| `runtime::tests::lifecycle_rollback_preserves_cleanup_diagnostic_for_active_session` | `runtime::tests::lifecycle_rollback_preserves_cleanup_diagnostic_for_active_session` | `crates/vulcan-plugin-runtime/src/runtime.rs:6049` | 严格等价 |
| `runtime::tests::same_version_package_change_advances_plugin_generation` | `runtime::tests::same_version_package_change_advances_plugin_generation` | `crates/vulcan-plugin-runtime/src/runtime.rs:6111` | 严格等价 |
| `runtime::tests::retained_lease_retry_claim_is_atomic` | `runtime::tests::retained_lease_retry_claim_is_atomic` | `crates/vulcan-plugin-runtime/src/runtime.rs:6136` | 严格等价 |
| `runtime::tests::plugin_process_spec_normalizes_windows_verbatim_paths` | `runtime::process::tests::plugin_process_spec_normalizes_windows_verbatim_paths` | `crates/vulcan-plugin-runtime/src/runtime/process/tests.rs:13` | 严格等价 |

## 辅助节点

| 节点 | 原始行 | 当前产物行 | 范围 |
| --- | ---: | --- | --- |
| `struct:RecordingVldbService` | 7632 | `crates/vulcan-plugin-runtime/src/runtime.rs:4593` | `private`→`private` |
| `VulcanPluginVldbService for RecordingVldbService::execute` | 7651 | `crates/vulcan-plugin-runtime/src/runtime.rs:4612` | `private`→`private` |
| `write_hook_package` | 7815 | `crates/vulcan-plugin-runtime/src/runtime.rs:4776` | `private`→`private` |
| `write_idle_session_package` | 7888 | `crates/vulcan-plugin-runtime/src/runtime.rs:4849` | `private`→`private` |
| `write_user_plugin_package` | 7968 | `crates/vulcan-plugin-runtime/src/runtime.rs:4929` | `private`→`pub(super)` |
| `write_content_plugin_package` | 8001 | `crates/vulcan-plugin-runtime/src/runtime.rs:4962` | `private`→`private` |
| `test_engine` | 8024 | `crates/vulcan-plugin-runtime/src/runtime.rs:4985` | `private`→`private` |
| `test_manager` | 8056 | `crates/vulcan-plugin-runtime/src/runtime.rs:5017` | `private`→`pub(super)` |
| `hook_invocation` | 8089 | `crates/vulcan-plugin-runtime/src/runtime.rs:5050` | `private`→`private` |

## 匹配与边界

- environment保留授权后篡改不能发布环境；process保留Windows路径规范化；authorization保留单一HTTP/MCP脱敏预览，不冒称全部传输运行验收。
- installation四项保留安装/更新指纹绑定、安装/环境/卸载与更新候选篡改及精确提交回滚；更新测试同域访问私有destination，不开放生产字段。原共享夹具只保留一份。
- 配置没有原独立单元用例；完整bundled插件、桥接与安装场景仍保留，不为凑模块测试数量拆碎跨域断言。
- 基线59通过，第三阶段23运行时通过/4.09秒；第四阶段4安装回归通过/0.21秒。完整最终命令、结果与范围见06-STAGE-04-REVIEW.md，不把未结束进程计为通过。
- 未运行Linux/macOS、Release运行、真实在线依赖安装、真实MCP启动和完整桌面；消费者过滤范围内的元数据/生命周期回归不替代这些验收。
