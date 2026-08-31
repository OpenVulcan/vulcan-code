# 06：最终完整节点与测试映射

## 范围与比较规则

2026-08-31，以冻结提交 `111540689b777ddc1229a6f17293a42818f2155e` 的9716行源码直接核对当前26产物。原218节点均唯一保留：186生产、23测试、9辅助。209节点忽略注释、空白及仅声明开头的内部可见性后严格等价；其余9项精确差异如下。没有全局忽略逗号、花括号或字符串的规则。

旧[生产映射](06-PRODUCTION-MOVES.md)及[测试映射](06-TEST-MOVES.md)保留第四阶段快照，当前定位以本表和[机器可读节点表](06-AUDIT-NODES.json)为准。

## 实名差异

| 节点 | 精确差异 | 判定 |
| --- | --- | --- |
| `canonical_bridge_workspace_root` | 仅增加函数参数尾逗号 | 已核实；完整前后词元与摘要见[差异数据](06-LEXICAL-EXCEPTIONS.json) |
| `canonical_plugin_file` | 仅增加函数参数尾逗号 | 已核实；完整前后词元与摘要见[差异数据](06-LEXICAL-EXCEPTIONS.json) |
| `read_plugin_json_state` | 仅增加函数参数尾逗号 | 已核实；完整前后词元与摘要见[差异数据](06-LEXICAL-EXCEPTIONS.json) |
| `test_manager` | 固定桥接include_str多上一层，仍解析同一真实bridge.lua | 已核实；完整前后词元与摘要见[差异数据](06-LEXICAL-EXCEPTIONS.json) |
| `bundled_context7_plugin_loads_with_typed_json_surface` | 无Hook预热空通过改为真实启用探测，并断言Lua清单 | 已核实；完整前后词元与摘要见[差异数据](06-LEXICAL-EXCEPTIONS.json) |
| `content_plugin_operates_without_lua_runtime_or_execution_environment` | 仅移除block_on调用尾逗号 | 已核实；完整前后词元与摘要见[差异数据](06-LEXICAL-EXCEPTIONS.json) |
| `user_plugin_install_binds_authorization_to_exact_content_fingerprint` | 修复真实授权记录路径；卸载事务增加删除前存在断言 | 已核实；完整前后词元与摘要见[差异数据](06-LEXICAL-EXCEPTIONS.json) |
| `user_plugin_install_environment_and_uninstall_are_transactional` | 修复真实授权记录路径；卸载事务增加删除前存在断言 | 已核实；完整前后词元与摘要见[差异数据](06-LEXICAL-EXCEPTIONS.json) |
| `parse_engine_response` | 仅增加函数参数尾逗号 | 已核实；完整前后词元与摘要见[差异数据](06-LEXICAL-EXCEPTIONS.json) |

## 全部生产节点

| 节点 | 原行 | 当前文件与行 |
| --- | ---: | --- |
| `apply_release_no_window_to_command` | 129 | `crates/vulcan-plugin-runtime/src/runtime/environment.rs:59` |
| `type:VulcanPluginBridgeIdentityPayload` | 144 | `crates/vulcan-plugin-runtime/src/runtime/bridge.rs:97` |
| `struct:VulcanPluginInvocationRequest` | 155 | `crates/vulcan-plugin-runtime/src/runtime/sessions.rs:25` |
| `struct:VulcanPluginActionRequest` | 173 | `crates/vulcan-plugin-runtime/src/runtime/actions.rs:20` |
| `struct:VulcanPluginPermissionPreview` | 198 | `crates/vulcan-plugin-runtime/src/runtime/authorization.rs:13` |
| `struct:VulcanPluginMcpPermissionPreview` | 250 | `crates/vulcan-plugin-runtime/src/runtime/authorization.rs:65` |
| `struct:VulcanPluginInstallSecretRequirement` | 269 | `crates/vulcan-plugin-runtime/src/runtime/authorization.rs:84` |
| `struct:VulcanPluginMcpStdioLaunchSpec` | 281 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:28` |
| `struct:VulcanPluginAuthorizationRecord` | 300 | `crates/vulcan-plugin-runtime/src/runtime/authorization.rs:97` |
| `struct:VulcanPluginInstallationRecord` | 322 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:30` |
| `struct:VulcanPluginEnvironmentMarker` | 344 | `crates/vulcan-plugin-runtime/src/runtime/environment.rs:29` |
| `struct:VulcanPluginInstallationResult` | 366 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:52` |
| `struct:VulcanPluginUpdateTransaction` | 379 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:65` |
| `VulcanPluginUpdateTransaction::plugin_id` | 409 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:214` |
| `VulcanPluginUpdateTransaction::permission_preview` | 415 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:228` |
| `enum:VulcanPluginLifecycleEvent` | 423 | `crates/vulcan-plugin-runtime/src/runtime/lifecycle.rs:26` |
| `enum:VulcanPluginBridgeLeaseScope` | 444 | `crates/vulcan-plugin-runtime/src/runtime/bridge.rs:108` |
| `VulcanPluginBridgeLeaseScope::as_str` | 476 | `crates/vulcan-plugin-runtime/src/runtime/bridge.rs:140` |
| `VulcanPluginBridgeLeaseScope::parse` | 500 | `crates/vulcan-plugin-runtime/src/runtime/bridge.rs:164` |
| `VulcanPluginBridgeLeaseScope::required_runtime_state` | 520 | `crates/vulcan-plugin-runtime/src/runtime/bridge.rs:184` |
| `From<VulcanPluginLifecycleEvent> for VulcanPluginBridgeLeaseScope::from` | 545 | `crates/vulcan-plugin-runtime/src/runtime/bridge.rs:209` |
| `struct:VulcanPluginRuntimeError` | 559 | `crates/vulcan-plugin-runtime/src/runtime.rs:67` |
| `VulcanPluginRuntimeError::new` | 578 | `crates/vulcan-plugin-runtime/src/runtime.rs:86` |
| `fmt::Display for VulcanPluginRuntimeError::fmt` | 588 | `crates/vulcan-plugin-runtime/src/runtime.rs:96` |
| `struct:ManagedVulcanPluginSession` | 597 | `crates/vulcan-plugin-runtime/src/runtime.rs:105` |
| `struct:ManagedVulcanPluginHookRuntime` | 638 | `crates/vulcan-plugin-runtime/src/runtime.rs:146` |
| `enum:VulcanPluginPersistentRuntimeKind` | 668 | `crates/vulcan-plugin-runtime/src/runtime.rs:176` |
| `VulcanPluginPersistentRuntimeKind::lease_id` | 680 | `crates/vulcan-plugin-runtime/src/runtime.rs:188` |
| `VulcanPluginPersistentRuntimeKind::label` | 689 | `crates/vulcan-plugin-runtime/src/runtime.rs:197` |
| `struct:RetainedVulcanPluginLease` | 700 | `crates/vulcan-plugin-runtime/src/runtime.rs:208` |
| `struct:VulcanPluginManagerState` | 723 | `crates/vulcan-plugin-runtime/src/runtime.rs:231` |
| `struct:VulcanPluginDrainReport` | 798 | `crates/vulcan-plugin-runtime/src/runtime/drain.rs:19` |
| `VulcanPluginDrainReport::merge` | 819 | `crates/vulcan-plugin-runtime/src/runtime/drain.rs:42` |
| `Drop for VulcanPluginManagerState::drop` | 837 | `crates/vulcan-plugin-runtime/src/runtime.rs:306` |
| `struct:VulcanPluginRuntimeManager` | 892 | `crates/vulcan-plugin-runtime/src/runtime.rs:361` |
| `fmt::Debug for VulcanPluginRuntimeManager::fmt` | 901 | `crates/vulcan-plugin-runtime/src/runtime.rs:370` |
| `VulcanPluginRuntimeManager::new` | 935 | `crates/vulcan-plugin-runtime/src/runtime.rs:404` |
| `VulcanPluginRuntimeManager::without_runtime` | 967 | `crates/vulcan-plugin-runtime/src/runtime.rs:436` |
| `VulcanPluginRuntimeManager::new_with_optional_engine` | 998 | `crates/vulcan-plugin-runtime/src/runtime.rs:467` |
| `VulcanPluginRuntimeManager::attach_runtime_engine` | 1056 | `crates/vulcan-plugin-runtime/src/runtime.rs:525` |
| `VulcanPluginRuntimeManager::runtime_engine` | 1080 | `crates/vulcan-plugin-runtime/src/runtime.rs:549` |
| `VulcanPluginRuntimeManager::install_host_services` | 1105 | `crates/vulcan-plugin-runtime/src/runtime/bridge.rs:233` |
| `VulcanPluginRuntimeManager::install_host_tool_bridge` | 1125 | `crates/vulcan-plugin-runtime/src/runtime/bridge.rs:253` |
| `VulcanPluginRuntimeManager::vulcan_plugin_root` | 1141 | `crates/vulcan-plugin-runtime/src/runtime.rs:569` |
| `VulcanPluginRuntimeManager::user_plugin_root` | 1150 | `crates/vulcan-plugin-runtime/src/runtime.rs:578` |
| `VulcanPluginRuntimeManager::plugin_state_root` | 1161 | `crates/vulcan-plugin-runtime/src/runtime.rs:589` |
| `VulcanPluginRuntimeManager::plugin_config` | 1177 | `crates/vulcan-plugin-runtime/src/runtime/configuration.rs:68` |
| `VulcanPluginRuntimeManager::update_plugin_config` | 1226 | `crates/vulcan-plugin-runtime/src/runtime/configuration.rs:117` |
| `VulcanPluginRuntimeManager::initialize_plugin_config_if_absent` | 1275 | `crates/vulcan-plugin-runtime/src/runtime/configuration.rs:166` |
| `VulcanPluginRuntimeManager::permission_preview` | 1300 | `crates/vulcan-plugin-runtime/src/runtime/authorization.rs:390` |
| `VulcanPluginRuntimeManager::hook_binary_directory` | 1323 | `crates/vulcan-plugin-runtime/src/runtime/persistent.rs:39` |
| `VulcanPluginRuntimeManager::authorize_user_plugin` | 1399 | `crates/vulcan-plugin-runtime/src/runtime/authorization.rs:413` |
| `VulcanPluginRuntimeManager::inspect_local_user_plugin` | 1464 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:246` |
| `VulcanPluginRuntimeManager::install_local_user_plugin` | 1487 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:269` |
| `VulcanPluginRuntimeManager::begin_local_user_plugin_update` | 1683 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:465` |
| `VulcanPluginRuntimeManager::verify_local_user_plugin_update_candidate` | 1916 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:698` |
| `VulcanPluginRuntimeManager::commit_local_user_plugin_update` | 1941 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:728` |
| `VulcanPluginRuntimeManager::rollback_local_user_plugin_update` | 1980 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:772` |
| `VulcanPluginRuntimeManager::uninstall_user_plugin` | 2043 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:840` |
| `VulcanPluginRuntimeManager::refresh_catalog` | 2187 | `crates/vulcan-plugin-runtime/src/runtime/lifecycle.rs:57` |
| `VulcanPluginRuntimeManager::begin_enabling` | 2224 | `crates/vulcan-plugin-runtime/src/runtime/lifecycle.rs:94` |
| `VulcanPluginRuntimeManager::begin_draining` | 2247 | `crates/vulcan-plugin-runtime/src/runtime/lifecycle.rs:117` |
| `VulcanPluginRuntimeManager::rollback_draining_to_enabling` | 2270 | `crates/vulcan-plugin-runtime/src/runtime/lifecycle.rs:140` |
| `VulcanPluginRuntimeManager::finish_lifecycle_transition` | 2293 | `crates/vulcan-plugin-runtime/src/runtime/lifecycle.rs:163` |
| `VulcanPluginRuntimeManager::probe_plugin` | 2330 | `crates/vulcan-plugin-runtime/src/runtime/lifecycle.rs:200` |
| `VulcanPluginRuntimeManager::drain_plugin_sessions` | 2452 | `crates/vulcan-plugin-runtime/src/runtime/drain.rs:70` |
| `VulcanPluginRuntimeManager::drain_persistent_runtime` | 2481 | `crates/vulcan-plugin-runtime/src/runtime/drain.rs:101` |
| `VulcanPluginRuntimeManager::drain_owner_sessions` | 2549 | `crates/vulcan-plugin-runtime/src/runtime/drain.rs:169` |
| `VulcanPluginRuntimeManager::drain_sessions` | 2576 | `crates/vulcan-plugin-runtime/src/runtime/drain.rs:198` |
| `VulcanPluginRuntimeManager::drain_retained_leases` | 2646 | `crates/vulcan-plugin-runtime/src/runtime/drain.rs:270` |
| `VulcanPluginRuntimeManager::take_retained_leases` | 2678 | `crates/vulcan-plugin-runtime/src/runtime/drain.rs:304` |
| `VulcanPluginRuntimeManager::retain_failed_lease` | 2696 | `crates/vulcan-plugin-runtime/src/runtime/drain.rs:324` |
| `VulcanPluginRuntimeManager::restore_retained_lease` | 2716 | `crates/vulcan-plugin-runtime/src/runtime/drain.rs:346` |
| `VulcanPluginRuntimeManager::retain_failed_session_lease` | 2733 | `crates/vulcan-plugin-runtime/src/runtime/drain.rs:365` |
| `VulcanPluginRuntimeManager::record_cleanup_failure` | 2753 | `crates/vulcan-plugin-runtime/src/runtime/drain.rs:387` |
| `VulcanPluginRuntimeManager::publish_cleanup_diagnostics` | 2771 | `crates/vulcan-plugin-runtime/src/runtime/drain.rs:407` |
| `VulcanPluginRuntimeManager::clear_cleanup_diagnostics_if_fully_drained` | 2803 | `crates/vulcan-plugin-runtime/src/runtime/drain.rs:441` |
| `VulcanPluginRuntimeManager::snapshot` | 2876 | `crates/vulcan-plugin-runtime/src/runtime/lifecycle.rs:317` |
| `VulcanPluginRuntimeManager::prepare_plugin_environment` | 2896 | `crates/vulcan-plugin-runtime/src/runtime/environment.rs:710` |
| `VulcanPluginRuntimeManager::plugin_execution_package_root` | 2939 | `crates/vulcan-plugin-runtime/src/runtime/environment.rs:753` |
| `VulcanPluginRuntimeManager::resolve_mcp_stdio_launcher` | 2962 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:997` |
| `VulcanPluginRuntimeManager::resolve_mcp_stdio_launcher_descriptor` | 2992 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:1027` |
| `VulcanPluginRuntimeManager::invoke_lifecycle` | 3039 | `crates/vulcan-plugin-runtime/src/runtime/lifecycle.rs:337` |
| `VulcanPluginRuntimeManager::invoke_action` | 3138 | `crates/vulcan-plugin-runtime/src/runtime/actions.rs:54` |
| `VulcanPluginRuntimeManager::invoke` | 3309 | `crates/vulcan-plugin-runtime/src/runtime/sessions.rs:53` |
| `VulcanPluginRuntimeManager::invoke_stateless` | 3335 | `crates/vulcan-plugin-runtime/src/runtime/sessions.rs:81` |
| `VulcanPluginRuntimeManager::prewarm_enabled_plugin_runtimes` | 3441 | `crates/vulcan-plugin-runtime/src/runtime/persistent.rs:110` |
| `VulcanPluginRuntimeManager::prewarm_plugin_runtime` | 3469 | `crates/vulcan-plugin-runtime/src/runtime/persistent.rs:140` |
| `VulcanPluginRuntimeManager::invoke_hook` | 3498 | `crates/vulcan-plugin-runtime/src/runtime/persistent.rs:171` |
| `VulcanPluginRuntimeManager::ensure_hook_runtime` | 3590 | `crates/vulcan-plugin-runtime/src/runtime/persistent.rs:265` |
| `VulcanPluginRuntimeManager::ensure_enabled_plugin_runtime` | 3605 | `crates/vulcan-plugin-runtime/src/runtime/persistent.rs:282` |
| `VulcanPluginRuntimeManager::close_persistent_runtime` | 3694 | `crates/vulcan-plugin-runtime/src/runtime/persistent.rs:373` |
| `VulcanPluginRuntimeManager::close_raw_lease` | 3762 | `crates/vulcan-plugin-runtime/src/runtime/lease.rs:261` |
| `VulcanPluginRuntimeManager::schedule_session_idle_reaper` | 3789 | `crates/vulcan-plugin-runtime/src/runtime/sessions.rs:186` |
| `VulcanPluginRuntimeManager::open_session` | 3850 | `crates/vulcan-plugin-runtime/src/runtime/sessions.rs:249` |
| `VulcanPluginRuntimeManager::call_session` | 3983 | `crates/vulcan-plugin-runtime/src/runtime/sessions.rs:384` |
| `VulcanPluginRuntimeManager::close_session` | 4012 | `crates/vulcan-plugin-runtime/src/runtime/sessions.rs:415` |
| `VulcanPluginRuntimeManager::create_managed_lease` | 4086 | `crates/vulcan-plugin-runtime/src/runtime/lease.rs:22` |
| `VulcanPluginRuntimeManager::owned_session` | 4277 | `crates/vulcan-plugin-runtime/src/runtime/sessions.rs:491` |
| `VulcanPluginRuntimeManager::ensure_current_tool_generation` | 4307 | `crates/vulcan-plugin-runtime/src/runtime/lifecycle.rs:428` |
| `VulcanPluginRuntimeManager::ensure_current_hook_generation` | 4331 | `crates/vulcan-plugin-runtime/src/runtime/lifecycle.rs:454` |
| `VulcanPluginRuntimeManager::ensure_session_generation` | 4365 | `crates/vulcan-plugin-runtime/src/runtime/sessions.rs:523` |
| `VulcanPluginRuntimeManager::ensure_session_runtime_current` | 4382 | `crates/vulcan-plugin-runtime/src/runtime/sessions.rs:542` |
| `VulcanPluginRuntimeManager::project_runtime_state` | 4406 | `crates/vulcan-plugin-runtime/src/runtime/lifecycle.rs:490` |
| `VulcanPluginRuntimeManager::begin_lifecycle_transition` | 4505 | `crates/vulcan-plugin-runtime/src/runtime/lifecycle.rs:589` |
| `VulcanPluginRuntimeManager::evaluate_bridge_function` | 4581 | `crates/vulcan-plugin-runtime/src/runtime/lease.rs:223` |
| `VulcanPluginRuntimeManager::dispatch` | 4627 | `crates/vulcan-plugin-runtime/src/runtime/sessions.rs:576` |
| `VulcanPluginRuntimeManager::close_lease` | 4673 | `crates/vulcan-plugin-runtime/src/runtime/sessions.rs:624` |
| `VulcanPluginRuntimeManager::reserve_session_slot` | 4699 | `crates/vulcan-plugin-runtime/src/runtime/sessions.rs:652` |
| `VulcanPluginRuntimeManager::release_session_slot` | 4725 | `crates/vulcan-plugin-runtime/src/runtime/sessions.rs:680` |
| `VulcanPluginRuntimeManager::publish_active_session_count` | 4743 | `crates/vulcan-plugin-runtime/src/runtime/sessions.rs:700` |
| `dispatch_registered_plugin_host_tool_request` | 4777 | `crates/vulcan-plugin-runtime/src/runtime/bridge.rs:275` |
| `dispatch_plugin_host_tool_request` | 4814 | `crates/vulcan-plugin-runtime/src/runtime/bridge.rs:312` |
| `plugin_storage_operation` | 4933 | `crates/vulcan-plugin-runtime/src/runtime/bridge.rs:431` |
| `plugin_vldb_operation` | 4977 | `crates/vulcan-plugin-runtime/src/runtime/bridge.rs:475` |
| `dispatch_plugin_storage_request` | 5001 | `crates/vulcan-plugin-runtime/src/runtime/bridge.rs:499` |
| `dispatch_plugin_vldb_request` | 5052 | `crates/vulcan-plugin-runtime/src/runtime/bridge.rs:550` |
| `dispatch_plugin_ui_invalidation` | 5101 | `crates/vulcan-plugin-runtime/src/runtime/bridge.rs:599` |
| `ensure_plugin_capability` | 5161 | `crates/vulcan-plugin-runtime/src/runtime/bridge.rs:659` |
| `trusted_hook_workspace_root` | 5207 | `crates/vulcan-plugin-runtime/src/runtime/persistent.rs:452` |
| `bridge_request_plugin_id` | 5245 | `crates/vulcan-plugin-runtime/src/runtime/bridge.rs:705` |
| `bridge_request_lease_scope` | 5264 | `crates/vulcan-plugin-runtime/src/runtime/bridge.rs:724` |
| `canonical_bridge_workspace_root` | 5285 | `crates/vulcan-plugin-runtime/src/runtime/bridge.rs:745` |
| `bridge_request_identity_and_payload` | 5308 | `crates/vulcan-plugin-runtime/src/runtime/bridge.rs:770` |
| `resolve_plugin_process_spec` | 5336 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:55` |
| `resolve_plugin_mcp_stdio_launcher` | 5474 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:193` |
| `resolve_plugin_managed_node` | 5555 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:288` |
| `resolve_plugin_node_package_bin` | 5584 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:337` |
| `npm_package_relative_path` | 5656 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:419` |
| `resolve_npm_bin_target` | 5676 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:453` |
| `resolve_plugin_binary_spec` | 5716 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:493` |
| `resolve_plugin_node_spec` | 5788 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:565` |
| `resolve_plugin_python_spec` | 5849 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:626` |
| `build_plugin_process_spec` | 5926 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:703` |
| `validate_plugin_environment` | 6049 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:826` |
| `copy_optional_string` | 6074 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:865` |
| `copy_optional_positive_u64` | 6090 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:895` |
| `required_json_string` | 6106 | `crates/vulcan-plugin-runtime/src/runtime/bridge.rs:790` |
| `canonical_plugin_file` | 6119 | `crates/vulcan-plugin-runtime/src/runtime/local_files.rs:14` |
| `canonical_plugin_directory` | 6137 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:925` |
| `current_plugin_target_triple` | 6155 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:951` |
| `plugin_config_state_path` | 6190 | `crates/vulcan-plugin-runtime/src/runtime/configuration.rs:26` |
| `plugin_authorization_state_path` | 6196 | `crates/vulcan-plugin-runtime/src/runtime/authorization.rs:129` |
| `plugin_installation_state_path` | 6202 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:106` |
| `authorization_record_for_descriptor` | 6218 | `crates/vulcan-plugin-runtime/src/runtime/authorization.rs:145` |
| `write_plugin_json_record` | 6243 | `crates/vulcan-plugin-runtime/src/runtime/local_files.rs:43` |
| `copy_plugin_directory_tree` | 6261 | `crates/vulcan-plugin-runtime/src/runtime/local_files.rs:63` |
| `read_optional_plugin_state_file` | 6321 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:122` |
| `remove_plugin_state_file` | 6354 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:160` |
| `current_unix_time_ms` | 6380 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:194` |
| `plugin_execution_package_root` | 6401 | `crates/vulcan-plugin-runtime/src/runtime/environment.rs:84` |
| `ensure_plugin_execution_environment` | 6430 | `crates/vulcan-plugin-runtime/src/runtime/environment.rs:113` |
| `remove_plugin_execution_environment_generation` | 6553 | `crates/vulcan-plugin-runtime/src/runtime/environment.rs:236` |
| `remove_obsolete_plugin_execution_environments` | 6615 | `crates/vulcan-plugin-runtime/src/runtime/environment.rs:298` |
| `validate_plugin_environment_marker` | 6690 | `crates/vulcan-plugin-runtime/src/runtime/environment.rs:385` |
| `install_plugin_registry_dependencies` | 6719 | `crates/vulcan-plugin-runtime/src/runtime/environment.rs:428` |
| `resolve_plugin_package_manager` | 6836 | `crates/vulcan-plugin-runtime/src/runtime/environment.rs:563` |
| `run_plugin_dependency_command` | 6893 | `crates/vulcan-plugin-runtime/src/runtime/environment.rs:632` |
| `plugin_dependency_install_mode_key` | 6915 | `crates/vulcan-plugin-runtime/src/runtime/environment.rs:664` |
| `plugin_python_environment_executable` | 6924 | `crates/vulcan-plugin-runtime/src/runtime/environment.rs:683` |
| `build_permission_preview` | 6950 | `crates/vulcan-plugin-runtime/src/runtime/authorization.rs:175` |
| `required_lua_runtime` | 7073 | `crates/vulcan-plugin-runtime/src/runtime/lease.rs:299` |
| `plugin_permission_key` | 7086 | `crates/vulcan-plugin-runtime/src/runtime/authorization.rs:298` |
| `plugin_ui_mount_point_key` | 7108 | `crates/vulcan-plugin-runtime/src/runtime/authorization.rs:330` |
| `plugin_is_authorized` | 7128 | `crates/vulcan-plugin-runtime/src/runtime/authorization.rs:350` |
| `read_plugin_json_state` | 7167 | `crates/vulcan-plugin-runtime/src/runtime/local_files.rs:123` |
| `validate_plugin_config_value` | 7206 | `crates/vulcan-plugin-runtime/src/runtime/configuration.rs:42` |
| `validate_plugin_action_value` | 7231 | `crates/vulcan-plugin-runtime/src/runtime/actions.rs:226` |
| `require_argument_object` | 7249 | `crates/vulcan-plugin-runtime/src/runtime/sessions.rs:726` |
| `take_session_id` | 7259 | `crates/vulcan-plugin-runtime/src/runtime/sessions.rs:738` |
| `plugin_payload` | 7269 | `crates/vulcan-plugin-runtime/src/runtime/sessions.rs:750` |
| `resolve_host_timeout` | 7287 | `crates/vulcan-plugin-runtime/src/runtime/sessions.rs:770` |
| `wrap_session_result` | 7324 | `crates/vulcan-plugin-runtime/src/runtime/sessions.rs:809` |
| `rebuild_enabled_surface` | 7334 | `crates/vulcan-plugin-runtime/src/runtime/lifecycle.rs:658` |
| `call_engine_blocking` | 7382 | `crates/vulcan-plugin-runtime/src/runtime/lease.rs:314` |
| `parse_engine_response` | 7403 | `crates/vulcan-plugin-runtime/src/runtime/lease.rs:337` |
| `parse_plugin_result` | 7422 | `crates/vulcan-plugin-runtime/src/runtime/lease.rs:361` |
| `render_error_value` | 7448 | `crates/vulcan-plugin-runtime/src/runtime/lease.rs:389` |
| `required_response_string` | 7457 | `crates/vulcan-plugin-runtime/src/runtime/lease.rs:400` |
| `required_response_u64` | 7473 | `crates/vulcan-plugin-runtime/src/runtime/lease.rs:418` |
| `lock_sessions` | 7481 | `crates/vulcan-plugin-runtime/src/runtime.rs:596` |
| `lock_hook_runtimes` | 7494 | `crates/vulcan-plugin-runtime/src/runtime.rs:609` |
| `lock_retained_leases` | 7507 | `crates/vulcan-plugin-runtime/src/runtime.rs:622` |
| `close_system_lease_sync` | 7517 | `crates/vulcan-plugin-runtime/src/runtime/lease.rs:428` |
| `close_raw_system_lease_sync` | 7536 | `crates/vulcan-plugin-runtime/src/runtime/lease.rs:449` |
| `current_unix_millis_u64` | 7557 | `crates/vulcan-plugin-runtime/src/runtime/sessions.rs:821` |

## 全部测试及辅助节点

| 原节点 | 类别 | 原行 | 当前路径 | 当前文件与行 |
| --- | --- | ---: | --- | --- |
| `bridge_request_requires_object_payload` | 测试 | 7578 | `runtime::bridge::tests::bridge_request_requires_object_payload` | `crates/vulcan-plugin-runtime/src/runtime/bridge/tests.rs:16` |
| `vldb_host_tool_names_map_to_exact_operations` | 测试 | 7605 | `runtime::bridge::tests::vldb_host_tool_names_map_to_exact_operations` | `crates/vulcan-plugin-runtime/src/runtime/bridge/tests.rs:43` |
| `struct:RecordingVldbService` | 辅助 | 7632 | `runtime::tests::bundled::RecordingVldbService` | `crates/vulcan-plugin-runtime/src/runtime/tests/bundled.rs:28` |
| `VulcanPluginVldbService for RecordingVldbService::execute` | 辅助 | 7651 | `runtime::tests::bundled::execute` | `crates/vulcan-plugin-runtime/src/runtime/tests/bundled.rs:47` |
| `write_hook_package` | 辅助 | 7815 | `runtime::tests::write_hook_package` | `crates/vulcan-plugin-runtime/src/runtime/tests.rs:15` |
| `write_idle_session_package` | 辅助 | 7888 | `runtime::tests::write_idle_session_package` | `crates/vulcan-plugin-runtime/src/runtime/tests.rs:88` |
| `write_user_plugin_package` | 辅助 | 7968 | `runtime::tests::write_user_plugin_package` | `crates/vulcan-plugin-runtime/src/runtime/tests.rs:168` |
| `write_content_plugin_package` | 辅助 | 8001 | `runtime::tests::content::write_content_plugin_package` | `crates/vulcan-plugin-runtime/src/runtime/tests/content.rs:18` |
| `test_engine` | 辅助 | 8024 | `runtime::tests::test_engine` | `crates/vulcan-plugin-runtime/src/runtime/tests.rs:195` |
| `test_manager` | 辅助 | 8056 | `runtime::tests::test_manager` | `crates/vulcan-plugin-runtime/src/runtime/tests.rs:227` |
| `hook_invocation` | 辅助 | 8089 | `runtime::persistent::tests::hook_invocation` | `crates/vulcan-plugin-runtime/src/runtime/persistent/tests.rs:19` |
| `content_plugin_operates_without_lua_runtime_or_execution_environment` | 测试 | 8116 | `runtime::tests::content::content_plugin_operates_without_lua_runtime_or_execution_environment` | `crates/vulcan-plugin-runtime/src/runtime/tests/content.rs:42` |
| `enable_rollback_uses_declared_disable_callback_in_enabling_scope` | 测试 | 8206 | `runtime::lifecycle::tests::enable_rollback_uses_declared_disable_callback_in_enabling_scope` | `crates/vulcan-plugin-runtime/src/runtime/lifecycle/tests.rs:12` |
| `draining_rollback_reenters_enabling_before_probe` | 测试 | 8248 | `runtime::lifecycle::tests::draining_rollback_reenters_enabling_before_probe` | `crates/vulcan-plugin-runtime/src/runtime/lifecycle/tests.rs:54` |
| `hook_runtime_reuses_state_and_drains_by_plugin_generation` | 测试 | 8289 | `runtime::persistent::tests::hook_runtime_reuses_state_and_drains_by_plugin_generation` | `crates/vulcan-plugin-runtime/src/runtime/persistent/tests.rs:46` |
| `bundled_usage_statistics_plugin_loads_through_system_vldb_bridge` | 测试 | 8345 | `runtime::tests::bundled::bundled_usage_statistics_plugin_loads_through_system_vldb_bridge` | `crates/vulcan-plugin-runtime/src/runtime/tests/bundled.rs:212` |
| `bundled_context7_plugin_loads_with_typed_json_surface` | 测试 | 8613 | `runtime::tests::bundled::bundled_context7_plugin_loads_with_typed_json_surface` | `crates/vulcan-plugin-runtime/src/runtime/tests/bundled.rs:480` |
| `bundled_rtk_plugin_uses_generic_action_and_hook_boundaries` | 测试 | 8648 | `runtime::tests::bundled::bundled_rtk_plugin_uses_generic_action_and_hook_boundaries` | `crates/vulcan-plugin-runtime/src/runtime/tests/bundled.rs:536` |
| `user_plugin_permission_preview_includes_sanitized_mcp_ownership` | 测试 | 8838 | `runtime::authorization::tests::user_plugin_permission_preview_includes_sanitized_mcp_ownership` | `crates/vulcan-plugin-runtime/src/runtime/authorization/tests.rs:11` |
| `user_plugin_install_binds_authorization_to_exact_content_fingerprint` | 测试 | 8902 | `runtime::installation::tests::user_plugin_install_binds_authorization_to_exact_content_fingerprint` | `crates/vulcan-plugin-runtime/src/runtime/installation/tests.rs:12` |
| `user_plugin_update_binds_authorization_to_exact_content_fingerprint` | 测试 | 8960 | `runtime::installation::tests::user_plugin_update_binds_authorization_to_exact_content_fingerprint` | `crates/vulcan-plugin-runtime/src/runtime/installation/tests.rs:70` |
| `user_plugin_install_environment_and_uninstall_are_transactional` | 测试 | 9056 | `runtime::installation::tests::user_plugin_install_environment_and_uninstall_are_transactional` | `crates/vulcan-plugin-runtime/src/runtime/installation/tests.rs:166` |
| `user_plugin_update_supports_exact_rollback_and_commit` | 测试 | 9112 | `runtime::installation::tests::user_plugin_update_supports_exact_rollback_and_commit` | `crates/vulcan-plugin-runtime/src/runtime/installation/tests.rs:229` |
| `user_plugin_environment_rejects_post_authorization_mutation` | 测试 | 9243 | `runtime::environment::tests::user_plugin_environment_rejects_post_authorization_mutation` | `crates/vulcan-plugin-runtime/src/runtime/environment/tests.rs:11` |
| `idle_session_timeout_reaps_managed_lease` | 测试 | 9314 | `runtime::sessions::tests::idle_session_timeout_reaps_managed_lease` | `crates/vulcan-plugin-runtime/src/runtime/sessions/tests.rs:14` |
| `close_tool_retires_stale_generation_session` | 测试 | 9376 | `runtime::sessions::tests::close_tool_retires_stale_generation_session` | `crates/vulcan-plugin-runtime/src/runtime/sessions/tests.rs:76` |
| `explicit_close_failure_publishes_cleanup_diagnostic` | 测试 | 9429 | `runtime::drain::tests::explicit_close_failure_publishes_cleanup_diagnostic` | `crates/vulcan-plugin-runtime/src/runtime/drain/tests.rs:16` |
| `concurrent_session_drain_waits_for_close_owner` | 测试 | 9494 | `runtime::drain::tests::concurrent_session_drain_waits_for_close_owner` | `crates/vulcan-plugin-runtime/src/runtime/drain/tests.rs:82` |
| `lifecycle_rollback_preserves_cleanup_diagnostic_for_active_session` | 测试 | 9564 | `runtime::drain::tests::lifecycle_rollback_preserves_cleanup_diagnostic_for_active_session` | `crates/vulcan-plugin-runtime/src/runtime/drain/tests.rs:152` |
| `same_version_package_change_advances_plugin_generation` | 测试 | 9626 | `runtime::lifecycle::tests::same_version_package_change_advances_plugin_generation` | `crates/vulcan-plugin-runtime/src/runtime/lifecycle/tests.rs:95` |
| `retained_lease_retry_claim_is_atomic` | 测试 | 9651 | `runtime::drain::tests::retained_lease_retry_claim_is_atomic` | `crates/vulcan-plugin-runtime/src/runtime/drain/tests.rs:214` |
| `plugin_process_spec_normalizes_windows_verbatim_paths` | 测试 | 9695 | `runtime::process::tests::plugin_process_spec_normalizes_windows_verbatim_paths` | `crates/vulcan-plugin-runtime/src/runtime/process/tests.rs:13` |

全部原前置属性一致，包括Windows条件、test、derive和序列化属性。原公开55项保留；51处累计内部声明可见性变化逐一限定于既有父域消费者，无字段可见性变化。22常量与1静态唯一且值完全相同。空Error实现保留。

