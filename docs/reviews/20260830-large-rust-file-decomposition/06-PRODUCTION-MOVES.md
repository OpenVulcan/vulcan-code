# 06：插件运行时生产节点迁移映射

本文件保留第四阶段历史快照。当前根631行、26产物；最新定位见[最终完整映射](06-FINAL-MOVES.md)，结论见[完整终审](06-FINAL-REVIEW.md)。

## 第4阶段检查点

原runtime.rs 9716行，SHA-256为c442f9145589f8e966babe875d7041b3c4e12d4fc6e7a60525901ddd2433b562。当前根6175行，10个Rust产物合计10159行；说明、导入和接线计入物理行数。

218原节点完整：186生产区域、23测试、9辅助节点；23类型及195函数/方法。65生产节点迁移、121保留根；55原可见项声明保持，15处累计父域范围精确核验，无字段变化。22常量与1静态完整；216节点严格词法等价，2项旧测试按实名记录修正，全部218节点通过精确预期核验。06尚未充分、未逐文件闭环；当前审核见06-STAGE-04-REVIEW.md。

## 全量生产节点

| 原节点 | 原始行 | 当前产物行 | 核验 | 原范围→当前范围 |
| --- | ---: | --- | --- | --- |
| `apply_release_no_window_to_command` | 129 | `crates/vulcan-plugin-runtime/src/runtime/environment.rs:59` | 严格等价 | `private`→`private` |
| `type:VulcanPluginBridgeIdentityPayload` | 144 | `crates/vulcan-plugin-runtime/src/runtime.rs:137` | 严格等价 | `private`→`private` |
| `struct:VulcanPluginInvocationRequest` | 155 | `crates/vulcan-plugin-runtime/src/runtime.rs:148` | 严格等价 | `pub`→`pub` |
| `struct:VulcanPluginActionRequest` | 173 | `crates/vulcan-plugin-runtime/src/runtime.rs:166` | 严格等价 | `pub`→`pub` |
| `struct:VulcanPluginPermissionPreview` | 198 | `crates/vulcan-plugin-runtime/src/runtime/authorization.rs:13` | 严格等价 | `pub`→`pub` |
| `struct:VulcanPluginMcpPermissionPreview` | 250 | `crates/vulcan-plugin-runtime/src/runtime/authorization.rs:65` | 严格等价 | `pub`→`pub` |
| `struct:VulcanPluginInstallSecretRequirement` | 269 | `crates/vulcan-plugin-runtime/src/runtime/authorization.rs:84` | 严格等价 | `pub`→`pub` |
| `struct:VulcanPluginMcpStdioLaunchSpec` | 281 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:28` | 严格等价 | `pub`→`pub` |
| `struct:VulcanPluginAuthorizationRecord` | 300 | `crates/vulcan-plugin-runtime/src/runtime/authorization.rs:97` | 严格等价 | `private`→`pub(super)` |
| `struct:VulcanPluginInstallationRecord` | 322 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:30` | 严格等价 | `private`→`private` |
| `struct:VulcanPluginEnvironmentMarker` | 344 | `crates/vulcan-plugin-runtime/src/runtime/environment.rs:29` | 严格等价 | `private`→`private` |
| `struct:VulcanPluginInstallationResult` | 366 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:52` | 严格等价 | `pub`→`pub` |
| `struct:VulcanPluginUpdateTransaction` | 379 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:65` | 严格等价 | `pub`→`pub` |
| `VulcanPluginUpdateTransaction::plugin_id` | 409 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:214` | 严格等价 | `pub`→`pub` |
| `VulcanPluginUpdateTransaction::permission_preview` | 415 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:228` | 严格等价 | `pub`→`pub` |
| `enum:VulcanPluginLifecycleEvent` | 423 | `crates/vulcan-plugin-runtime/src/runtime.rs:190` | 严格等价 | `pub`→`pub` |
| `enum:VulcanPluginBridgeLeaseScope` | 444 | `crates/vulcan-plugin-runtime/src/runtime.rs:211` | 严格等价 | `private`→`private` |
| `VulcanPluginBridgeLeaseScope::as_str` | 476 | `crates/vulcan-plugin-runtime/src/runtime.rs:243` | 严格等价 | `private`→`private` |
| `VulcanPluginBridgeLeaseScope::parse` | 500 | `crates/vulcan-plugin-runtime/src/runtime.rs:267` | 严格等价 | `private`→`private` |
| `VulcanPluginBridgeLeaseScope::required_runtime_state` | 520 | `crates/vulcan-plugin-runtime/src/runtime.rs:287` | 严格等价 | `private`→`private` |
| `From<VulcanPluginLifecycleEvent> for VulcanPluginBridgeLeaseScope::from` | 545 | `crates/vulcan-plugin-runtime/src/runtime.rs:312` | 严格等价 | `private`→`private` |
| `struct:VulcanPluginRuntimeError` | 559 | `crates/vulcan-plugin-runtime/src/runtime.rs:326` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeError::new` | 578 | `crates/vulcan-plugin-runtime/src/runtime.rs:345` | 严格等价 | `pub`→`pub` |
| `fmt::Display for VulcanPluginRuntimeError::fmt` | 588 | `crates/vulcan-plugin-runtime/src/runtime.rs:355` | 严格等价 | `private`→`private` |
| `struct:ManagedVulcanPluginSession` | 597 | `crates/vulcan-plugin-runtime/src/runtime.rs:364` | 严格等价 | `private`→`private` |
| `struct:ManagedVulcanPluginHookRuntime` | 638 | `crates/vulcan-plugin-runtime/src/runtime.rs:405` | 严格等价 | `private`→`private` |
| `enum:VulcanPluginPersistentRuntimeKind` | 668 | `crates/vulcan-plugin-runtime/src/runtime.rs:435` | 严格等价 | `private`→`private` |
| `VulcanPluginPersistentRuntimeKind::lease_id` | 680 | `crates/vulcan-plugin-runtime/src/runtime.rs:447` | 严格等价 | `private`→`private` |
| `VulcanPluginPersistentRuntimeKind::label` | 689 | `crates/vulcan-plugin-runtime/src/runtime.rs:456` | 严格等价 | `private`→`private` |
| `struct:RetainedVulcanPluginLease` | 700 | `crates/vulcan-plugin-runtime/src/runtime.rs:467` | 严格等价 | `private`→`private` |
| `struct:VulcanPluginManagerState` | 723 | `crates/vulcan-plugin-runtime/src/runtime.rs:490` | 严格等价 | `private`→`private` |
| `struct:VulcanPluginDrainReport` | 798 | `crates/vulcan-plugin-runtime/src/runtime.rs:565` | 严格等价 | `pub`→`pub` |
| `VulcanPluginDrainReport::merge` | 819 | `crates/vulcan-plugin-runtime/src/runtime.rs:586` | 严格等价 | `private`→`private` |
| `Drop for VulcanPluginManagerState::drop` | 837 | `crates/vulcan-plugin-runtime/src/runtime.rs:604` | 严格等价 | `private`→`private` |
| `struct:VulcanPluginRuntimeManager` | 892 | `crates/vulcan-plugin-runtime/src/runtime.rs:659` | 严格等价 | `pub`→`pub` |
| `fmt::Debug for VulcanPluginRuntimeManager::fmt` | 901 | `crates/vulcan-plugin-runtime/src/runtime.rs:668` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::new` | 935 | `crates/vulcan-plugin-runtime/src/runtime.rs:702` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::without_runtime` | 967 | `crates/vulcan-plugin-runtime/src/runtime.rs:734` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::new_with_optional_engine` | 998 | `crates/vulcan-plugin-runtime/src/runtime.rs:765` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::attach_runtime_engine` | 1056 | `crates/vulcan-plugin-runtime/src/runtime.rs:823` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::runtime_engine` | 1080 | `crates/vulcan-plugin-runtime/src/runtime.rs:847` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::install_host_services` | 1105 | `crates/vulcan-plugin-runtime/src/runtime.rs:872` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::install_host_tool_bridge` | 1125 | `crates/vulcan-plugin-runtime/src/runtime.rs:892` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::vulcan_plugin_root` | 1141 | `crates/vulcan-plugin-runtime/src/runtime.rs:908` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::user_plugin_root` | 1150 | `crates/vulcan-plugin-runtime/src/runtime.rs:917` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::plugin_state_root` | 1161 | `crates/vulcan-plugin-runtime/src/runtime.rs:928` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::plugin_config` | 1177 | `crates/vulcan-plugin-runtime/src/runtime/configuration.rs:68` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::update_plugin_config` | 1226 | `crates/vulcan-plugin-runtime/src/runtime/configuration.rs:117` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::initialize_plugin_config_if_absent` | 1275 | `crates/vulcan-plugin-runtime/src/runtime/configuration.rs:166` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::permission_preview` | 1300 | `crates/vulcan-plugin-runtime/src/runtime/authorization.rs:390` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::hook_binary_directory` | 1323 | `crates/vulcan-plugin-runtime/src/runtime.rs:944` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::authorize_user_plugin` | 1399 | `crates/vulcan-plugin-runtime/src/runtime/authorization.rs:413` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::inspect_local_user_plugin` | 1464 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:246` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::install_local_user_plugin` | 1487 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:269` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::begin_local_user_plugin_update` | 1683 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:465` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::verify_local_user_plugin_update_candidate` | 1916 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:698` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::commit_local_user_plugin_update` | 1941 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:728` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::rollback_local_user_plugin_update` | 1980 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:772` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::uninstall_user_plugin` | 2043 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:840` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::refresh_catalog` | 2187 | `crates/vulcan-plugin-runtime/src/runtime.rs:1020` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::begin_enabling` | 2224 | `crates/vulcan-plugin-runtime/src/runtime.rs:1057` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::begin_draining` | 2247 | `crates/vulcan-plugin-runtime/src/runtime.rs:1080` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::rollback_draining_to_enabling` | 2270 | `crates/vulcan-plugin-runtime/src/runtime.rs:1103` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::finish_lifecycle_transition` | 2293 | `crates/vulcan-plugin-runtime/src/runtime.rs:1126` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::probe_plugin` | 2330 | `crates/vulcan-plugin-runtime/src/runtime.rs:1163` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::drain_plugin_sessions` | 2452 | `crates/vulcan-plugin-runtime/src/runtime.rs:1285` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::drain_persistent_runtime` | 2481 | `crates/vulcan-plugin-runtime/src/runtime.rs:1314` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::drain_owner_sessions` | 2549 | `crates/vulcan-plugin-runtime/src/runtime.rs:1382` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::drain_sessions` | 2576 | `crates/vulcan-plugin-runtime/src/runtime.rs:1409` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::drain_retained_leases` | 2646 | `crates/vulcan-plugin-runtime/src/runtime.rs:1479` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::take_retained_leases` | 2678 | `crates/vulcan-plugin-runtime/src/runtime.rs:1511` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::retain_failed_lease` | 2696 | `crates/vulcan-plugin-runtime/src/runtime.rs:1529` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::restore_retained_lease` | 2716 | `crates/vulcan-plugin-runtime/src/runtime.rs:1549` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::retain_failed_session_lease` | 2733 | `crates/vulcan-plugin-runtime/src/runtime.rs:1566` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::record_cleanup_failure` | 2753 | `crates/vulcan-plugin-runtime/src/runtime.rs:1586` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::publish_cleanup_diagnostics` | 2771 | `crates/vulcan-plugin-runtime/src/runtime.rs:1604` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::clear_cleanup_diagnostics_if_fully_drained` | 2803 | `crates/vulcan-plugin-runtime/src/runtime.rs:1636` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::snapshot` | 2876 | `crates/vulcan-plugin-runtime/src/runtime.rs:1709` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::prepare_plugin_environment` | 2896 | `crates/vulcan-plugin-runtime/src/runtime/environment.rs:710` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::plugin_execution_package_root` | 2939 | `crates/vulcan-plugin-runtime/src/runtime/environment.rs:753` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::resolve_mcp_stdio_launcher` | 2962 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:997` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::resolve_mcp_stdio_launcher_descriptor` | 2992 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:1027` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::invoke_lifecycle` | 3039 | `crates/vulcan-plugin-runtime/src/runtime.rs:1729` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::invoke_action` | 3138 | `crates/vulcan-plugin-runtime/src/runtime.rs:1828` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::invoke` | 3309 | `crates/vulcan-plugin-runtime/src/runtime.rs:1999` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::invoke_stateless` | 3335 | `crates/vulcan-plugin-runtime/src/runtime.rs:2025` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::prewarm_enabled_plugin_runtimes` | 3441 | `crates/vulcan-plugin-runtime/src/runtime.rs:2131` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::prewarm_plugin_runtime` | 3469 | `crates/vulcan-plugin-runtime/src/runtime.rs:2159` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::invoke_hook` | 3498 | `crates/vulcan-plugin-runtime/src/runtime.rs:2188` | 严格等价 | `pub`→`pub` |
| `VulcanPluginRuntimeManager::ensure_hook_runtime` | 3590 | `crates/vulcan-plugin-runtime/src/runtime.rs:2280` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::ensure_enabled_plugin_runtime` | 3605 | `crates/vulcan-plugin-runtime/src/runtime.rs:2295` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::close_persistent_runtime` | 3694 | `crates/vulcan-plugin-runtime/src/runtime.rs:2384` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::close_raw_lease` | 3762 | `crates/vulcan-plugin-runtime/src/runtime.rs:2452` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::schedule_session_idle_reaper` | 3789 | `crates/vulcan-plugin-runtime/src/runtime.rs:2479` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::open_session` | 3850 | `crates/vulcan-plugin-runtime/src/runtime.rs:2540` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::call_session` | 3983 | `crates/vulcan-plugin-runtime/src/runtime.rs:2673` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::close_session` | 4012 | `crates/vulcan-plugin-runtime/src/runtime.rs:2702` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::create_managed_lease` | 4086 | `crates/vulcan-plugin-runtime/src/runtime.rs:2776` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::owned_session` | 4277 | `crates/vulcan-plugin-runtime/src/runtime.rs:2967` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::ensure_current_tool_generation` | 4307 | `crates/vulcan-plugin-runtime/src/runtime.rs:2997` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::ensure_current_hook_generation` | 4331 | `crates/vulcan-plugin-runtime/src/runtime.rs:3021` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::ensure_session_generation` | 4365 | `crates/vulcan-plugin-runtime/src/runtime.rs:3055` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::ensure_session_runtime_current` | 4382 | `crates/vulcan-plugin-runtime/src/runtime.rs:3072` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::project_runtime_state` | 4406 | `crates/vulcan-plugin-runtime/src/runtime.rs:3096` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::begin_lifecycle_transition` | 4505 | `crates/vulcan-plugin-runtime/src/runtime.rs:3195` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::evaluate_bridge_function` | 4581 | `crates/vulcan-plugin-runtime/src/runtime.rs:3271` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::dispatch` | 4627 | `crates/vulcan-plugin-runtime/src/runtime.rs:3317` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::close_lease` | 4673 | `crates/vulcan-plugin-runtime/src/runtime.rs:3363` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::reserve_session_slot` | 4699 | `crates/vulcan-plugin-runtime/src/runtime.rs:3389` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::release_session_slot` | 4725 | `crates/vulcan-plugin-runtime/src/runtime.rs:3415` | 严格等价 | `private`→`private` |
| `VulcanPluginRuntimeManager::publish_active_session_count` | 4743 | `crates/vulcan-plugin-runtime/src/runtime.rs:3433` | 严格等价 | `private`→`private` |
| `dispatch_registered_plugin_host_tool_request` | 4777 | `crates/vulcan-plugin-runtime/src/runtime.rs:3467` | 严格等价 | `private`→`private` |
| `dispatch_plugin_host_tool_request` | 4814 | `crates/vulcan-plugin-runtime/src/runtime.rs:3504` | 严格等价 | `private`→`private` |
| `plugin_storage_operation` | 4933 | `crates/vulcan-plugin-runtime/src/runtime.rs:3623` | 严格等价 | `private`→`private` |
| `plugin_vldb_operation` | 4977 | `crates/vulcan-plugin-runtime/src/runtime.rs:3667` | 严格等价 | `private`→`private` |
| `dispatch_plugin_storage_request` | 5001 | `crates/vulcan-plugin-runtime/src/runtime.rs:3691` | 严格等价 | `private`→`private` |
| `dispatch_plugin_vldb_request` | 5052 | `crates/vulcan-plugin-runtime/src/runtime.rs:3742` | 严格等价 | `private`→`private` |
| `dispatch_plugin_ui_invalidation` | 5101 | `crates/vulcan-plugin-runtime/src/runtime.rs:3791` | 严格等价 | `private`→`private` |
| `ensure_plugin_capability` | 5161 | `crates/vulcan-plugin-runtime/src/runtime.rs:3851` | 严格等价 | `private`→`private` |
| `trusted_hook_workspace_root` | 5207 | `crates/vulcan-plugin-runtime/src/runtime.rs:3897` | 严格等价 | `private`→`private` |
| `bridge_request_plugin_id` | 5245 | `crates/vulcan-plugin-runtime/src/runtime.rs:3935` | 严格等价 | `private`→`private` |
| `bridge_request_lease_scope` | 5264 | `crates/vulcan-plugin-runtime/src/runtime.rs:3954` | 严格等价 | `private`→`private` |
| `canonical_bridge_workspace_root` | 5285 | `crates/vulcan-plugin-runtime/src/runtime.rs:3975` | 严格等价 | `private`→`private` |
| `bridge_request_identity_and_payload` | 5308 | `crates/vulcan-plugin-runtime/src/runtime.rs:3998` | 严格等价 | `private`→`private` |
| `resolve_plugin_process_spec` | 5336 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:55` | 严格等价 | `private`→`pub(super)` |
| `resolve_plugin_mcp_stdio_launcher` | 5474 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:193` | 严格等价 | `private`→`private` |
| `resolve_plugin_managed_node` | 5555 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:288` | 严格等价 | `private`→`private` |
| `resolve_plugin_node_package_bin` | 5584 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:337` | 严格等价 | `private`→`private` |
| `npm_package_relative_path` | 5656 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:419` | 严格等价 | `private`→`private` |
| `resolve_npm_bin_target` | 5676 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:453` | 严格等价 | `private`→`private` |
| `resolve_plugin_binary_spec` | 5716 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:493` | 严格等价 | `private`→`private` |
| `resolve_plugin_node_spec` | 5788 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:565` | 严格等价 | `private`→`private` |
| `resolve_plugin_python_spec` | 5849 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:626` | 严格等价 | `private`→`private` |
| `build_plugin_process_spec` | 5926 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:703` | 严格等价 | `private`→`private` |
| `validate_plugin_environment` | 6049 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:826` | 严格等价 | `private`→`private` |
| `copy_optional_string` | 6074 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:865` | 严格等价 | `private`→`private` |
| `copy_optional_positive_u64` | 6090 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:895` | 严格等价 | `private`→`private` |
| `required_json_string` | 6106 | `crates/vulcan-plugin-runtime/src/runtime.rs:4016` | 严格等价 | `private`→`private` |
| `canonical_plugin_file` | 6119 | `crates/vulcan-plugin-runtime/src/runtime.rs:4029` | 严格等价 | `private`→`private` |
| `canonical_plugin_directory` | 6137 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:925` | 严格等价 | `private`→`private` |
| `current_plugin_target_triple` | 6155 | `crates/vulcan-plugin-runtime/src/runtime/process.rs:951` | 严格等价 | `private`→`pub(super)` |
| `plugin_config_state_path` | 6190 | `crates/vulcan-plugin-runtime/src/runtime/configuration.rs:26` | 严格等价 | `private`→`pub(super)` |
| `plugin_authorization_state_path` | 6196 | `crates/vulcan-plugin-runtime/src/runtime/authorization.rs:129` | 严格等价 | `private`→`pub(super)` |
| `plugin_installation_state_path` | 6202 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:106` | 严格等价 | `private`→`private` |
| `authorization_record_for_descriptor` | 6218 | `crates/vulcan-plugin-runtime/src/runtime/authorization.rs:145` | 严格等价 | `private`→`pub(super)` |
| `write_plugin_json_record` | 6243 | `crates/vulcan-plugin-runtime/src/runtime.rs:4052` | 严格等价 | `private`→`private` |
| `copy_plugin_directory_tree` | 6261 | `crates/vulcan-plugin-runtime/src/runtime.rs:4070` | 严格等价 | `private`→`private` |
| `read_optional_plugin_state_file` | 6321 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:122` | 严格等价 | `private`→`private` |
| `remove_plugin_state_file` | 6354 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:160` | 严格等价 | `private`→`private` |
| `current_unix_time_ms` | 6380 | `crates/vulcan-plugin-runtime/src/runtime/installation.rs:194` | 严格等价 | `private`→`private` |
| `plugin_execution_package_root` | 6401 | `crates/vulcan-plugin-runtime/src/runtime/environment.rs:84` | 严格等价 | `private`→`pub(super)` |
| `ensure_plugin_execution_environment` | 6430 | `crates/vulcan-plugin-runtime/src/runtime/environment.rs:113` | 严格等价 | `private`→`pub(super)` |
| `remove_plugin_execution_environment_generation` | 6553 | `crates/vulcan-plugin-runtime/src/runtime/environment.rs:236` | 严格等价 | `private`→`pub(super)` |
| `remove_obsolete_plugin_execution_environments` | 6615 | `crates/vulcan-plugin-runtime/src/runtime/environment.rs:298` | 严格等价 | `private`→`pub(super)` |
| `validate_plugin_environment_marker` | 6690 | `crates/vulcan-plugin-runtime/src/runtime/environment.rs:385` | 严格等价 | `private`→`private` |
| `install_plugin_registry_dependencies` | 6719 | `crates/vulcan-plugin-runtime/src/runtime/environment.rs:428` | 严格等价 | `private`→`private` |
| `resolve_plugin_package_manager` | 6836 | `crates/vulcan-plugin-runtime/src/runtime/environment.rs:563` | 严格等价 | `private`→`private` |
| `run_plugin_dependency_command` | 6893 | `crates/vulcan-plugin-runtime/src/runtime/environment.rs:632` | 严格等价 | `private`→`private` |
| `plugin_dependency_install_mode_key` | 6915 | `crates/vulcan-plugin-runtime/src/runtime/environment.rs:664` | 严格等价 | `private`→`private` |
| `plugin_python_environment_executable` | 6924 | `crates/vulcan-plugin-runtime/src/runtime/environment.rs:683` | 严格等价 | `private`→`pub(super)` |
| `build_permission_preview` | 6950 | `crates/vulcan-plugin-runtime/src/runtime/authorization.rs:175` | 严格等价 | `private`→`pub(super)` |
| `required_lua_runtime` | 7073 | `crates/vulcan-plugin-runtime/src/runtime.rs:4130` | 严格等价 | `private`→`private` |
| `plugin_permission_key` | 7086 | `crates/vulcan-plugin-runtime/src/runtime/authorization.rs:298` | 严格等价 | `pub`→`pub` |
| `plugin_ui_mount_point_key` | 7108 | `crates/vulcan-plugin-runtime/src/runtime/authorization.rs:330` | 严格等价 | `private`→`private` |
| `plugin_is_authorized` | 7128 | `crates/vulcan-plugin-runtime/src/runtime/authorization.rs:350` | 严格等价 | `private`→`pub(super)` |
| `read_plugin_json_state` | 7167 | `crates/vulcan-plugin-runtime/src/runtime.rs:4153` | 严格等价 | `private`→`private` |
| `validate_plugin_config_value` | 7206 | `crates/vulcan-plugin-runtime/src/runtime/configuration.rs:42` | 严格等价 | `private`→`private` |
| `validate_plugin_action_value` | 7231 | `crates/vulcan-plugin-runtime/src/runtime.rs:4192` | 严格等价 | `private`→`private` |
| `require_argument_object` | 7249 | `crates/vulcan-plugin-runtime/src/runtime.rs:4210` | 严格等价 | `private`→`private` |
| `take_session_id` | 7259 | `crates/vulcan-plugin-runtime/src/runtime.rs:4220` | 严格等价 | `private`→`private` |
| `plugin_payload` | 7269 | `crates/vulcan-plugin-runtime/src/runtime.rs:4230` | 严格等价 | `private`→`private` |
| `resolve_host_timeout` | 7287 | `crates/vulcan-plugin-runtime/src/runtime.rs:4248` | 严格等价 | `private`→`private` |
| `wrap_session_result` | 7324 | `crates/vulcan-plugin-runtime/src/runtime.rs:4285` | 严格等价 | `private`→`private` |
| `rebuild_enabled_surface` | 7334 | `crates/vulcan-plugin-runtime/src/runtime.rs:4295` | 严格等价 | `private`→`private` |
| `call_engine_blocking` | 7382 | `crates/vulcan-plugin-runtime/src/runtime.rs:4343` | 严格等价 | `private`→`private` |
| `parse_engine_response` | 7403 | `crates/vulcan-plugin-runtime/src/runtime.rs:4364` | 严格等价 | `private`→`private` |
| `parse_plugin_result` | 7422 | `crates/vulcan-plugin-runtime/src/runtime.rs:4383` | 严格等价 | `private`→`private` |
| `render_error_value` | 7448 | `crates/vulcan-plugin-runtime/src/runtime.rs:4409` | 严格等价 | `private`→`private` |
| `required_response_string` | 7457 | `crates/vulcan-plugin-runtime/src/runtime.rs:4418` | 严格等价 | `private`→`private` |
| `required_response_u64` | 7473 | `crates/vulcan-plugin-runtime/src/runtime.rs:4434` | 严格等价 | `private`→`private` |
| `lock_sessions` | 7481 | `crates/vulcan-plugin-runtime/src/runtime.rs:4442` | 严格等价 | `private`→`private` |
| `lock_hook_runtimes` | 7494 | `crates/vulcan-plugin-runtime/src/runtime.rs:4455` | 严格等价 | `private`→`private` |
| `lock_retained_leases` | 7507 | `crates/vulcan-plugin-runtime/src/runtime.rs:4468` | 严格等价 | `private`→`private` |
| `close_system_lease_sync` | 7517 | `crates/vulcan-plugin-runtime/src/runtime.rs:4478` | 严格等价 | `private`→`private` |
| `close_raw_system_lease_sync` | 7536 | `crates/vulcan-plugin-runtime/src/runtime.rs:4497` | 严格等价 | `private`→`private` |
| `current_unix_millis_u64` | 7557 | `crates/vulcan-plugin-runtime/src/runtime.rs:4518` | 严格等价 | `private`→`private` |

## 常量与进程级注册表

| 声明 | 原始行 | 当前产物行 | 核验 |
| --- | ---: | --- | --- |
| `VULCAN_PLUGIN_PROBE_TIMEOUT_MS` | 37 | `crates/vulcan-plugin-runtime/src/runtime.rs:50` | 完整声明唯一且等价 |
| `VULCAN_PLUGIN_DEFAULT_HOOK_TIMEOUT_MS` | 41 | `crates/vulcan-plugin-runtime/src/runtime.rs:54` | 完整声明唯一且等价 |
| `VULCAN_PLUGIN_HOOK_SHUTDOWN_TIMEOUT_MS` | 45 | `crates/vulcan-plugin-runtime/src/runtime.rs:58` | 完整声明唯一且等价 |
| `VULCAN_PLUGIN_PROCESS_RESOLVE_TOOL` | 49 | `crates/vulcan-plugin-runtime/src/runtime.rs:62` | 完整声明唯一且等价 |
| `VULCAN_PLUGIN_CONFIG_GET_TOOL` | 53 | `crates/vulcan-plugin-runtime/src/runtime.rs:66` | 完整声明唯一且等价 |
| `VULCAN_PLUGIN_CONFIG_UPDATE_TOOL` | 57 | `crates/vulcan-plugin-runtime/src/runtime.rs:70` | 完整声明唯一且等价 |
| `VULCAN_PLUGIN_STORAGE_SQLITE_MIGRATE_TOOL` | 61 | `crates/vulcan-plugin-runtime/src/runtime.rs:74` | 完整声明唯一且等价 |
| `VULCAN_PLUGIN_STORAGE_SQLITE_EXECUTE_TOOL` | 65 | `crates/vulcan-plugin-runtime/src/runtime.rs:78` | 完整声明唯一且等价 |
| `VULCAN_PLUGIN_STORAGE_SQLITE_EXECUTE_BATCH_TOOL` | 69 | `crates/vulcan-plugin-runtime/src/runtime.rs:82` | 完整声明唯一且等价 |
| `VULCAN_PLUGIN_STORAGE_SQLITE_QUERY_TOOL` | 74 | `crates/vulcan-plugin-runtime/src/runtime.rs:87` | 完整声明唯一且等价 |
| `VULCAN_PLUGIN_STORAGE_SQLITE_FLUSH_TOOL` | 78 | `crates/vulcan-plugin-runtime/src/runtime.rs:91` | 完整声明唯一且等价 |
| `VULCAN_PLUGIN_STORAGE_LANCEDB_CREATE_TABLE_TOOL` | 82 | `crates/vulcan-plugin-runtime/src/runtime.rs:95` | 完整声明唯一且等价 |
| `VULCAN_PLUGIN_STORAGE_LANCEDB_UPSERT_TOOL` | 87 | `crates/vulcan-plugin-runtime/src/runtime.rs:100` | 完整声明唯一且等价 |
| `VULCAN_PLUGIN_STORAGE_LANCEDB_SEARCH_TOOL` | 91 | `crates/vulcan-plugin-runtime/src/runtime.rs:104` | 完整声明唯一且等价 |
| `VULCAN_PLUGIN_STORAGE_LANCEDB_DELETE_TOOL` | 95 | `crates/vulcan-plugin-runtime/src/runtime.rs:108` | 完整声明唯一且等价 |
| `VULCAN_PLUGIN_STORAGE_LANCEDB_DROP_TABLE_TOOL` | 99 | `crates/vulcan-plugin-runtime/src/runtime.rs:112` | 完整声明唯一且等价 |
| `VULCAN_PLUGIN_UI_INVALIDATE_TOOL` | 104 | `crates/vulcan-plugin-runtime/src/runtime.rs:117` | 完整声明唯一且等价 |
| `VULCAN_VLDB_SQLITE_QUERY_V1_TOOL` | 108 | `crates/vulcan-plugin-runtime/src/runtime.rs:121` | 完整声明唯一且等价 |
| `VULCAN_VLDB_SQLITE_EXECUTE_V1_TOOL` | 112 | `crates/vulcan-plugin-runtime/src/runtime.rs:125` | 完整声明唯一且等价 |
| `VULCAN_VLDB_SQLITE_EXECUTE_BATCH_V1_TOOL` | 116 | `crates/vulcan-plugin-runtime/src/runtime.rs:129` | 完整声明唯一且等价 |
| `VULCAN_VLDB_SQLITE_FLUSH_V1_TOOL` | 120 | `crates/vulcan-plugin-runtime/src/runtime.rs:133` | 完整声明唯一且等价 |
| `WINDOWS_CREATE_NO_WINDOW` | 125 | `crates/vulcan-plugin-runtime/src/runtime/environment.rs:23` | 完整声明唯一且等价 |
| `VULCAN_PLUGIN_MANAGER_REGISTRY` | 148 | `crates/vulcan-plugin-runtime/src/runtime.rs:141` | 完整声明唯一且等价 |

## 当前产物摘要

| 路径 | 物理行 | SHA-256 |
| --- | ---: | --- |
| `crates/vulcan-plugin-runtime/src/runtime.rs` | 6175 | `5e9f1ffdcacb925017b82cbb14d692ac0eb82880132ec0ae2c5ba3f97fb56c04` |
| `crates/vulcan-plugin-runtime/src/runtime/process.rs` | 1064 | `a36815f624d03bb5ef070cc7666a59b2fc802fdd8d877832b9cc80e9a52d952b` |
| `crates/vulcan-plugin-runtime/src/runtime/process/tests.rs` | 33 | `e15c81242686d73a5bab63526171788eaeba015a61b40122781d06014cbf676f` |
| `crates/vulcan-plugin-runtime/src/runtime/installation.rs` | 974 | `e8aa8c4056464749e85dfb18d0b75ee72f9238ab9dc400e99fcc9ed9a2dbbb91` |
| `crates/vulcan-plugin-runtime/src/runtime/installation/tests.rs` | 355 | `7eef8d052a872abf213c12e2ac647f5f92325d2be87cf784931c12be3fa15ef5` |
| `crates/vulcan-plugin-runtime/src/runtime/environment.rs` | 766 | `bfdbfcb1b6d731fdcdbca8f81c9b4e6fb514caada5f82c032ab6a2121ca868b9` |
| `crates/vulcan-plugin-runtime/src/runtime/environment/tests.rs` | 76 | `1f6e99817e64c35757dc9c69c46c28cfee45d04af366e3764f84a6bf1454dec4` |
| `crates/vulcan-plugin-runtime/src/runtime/configuration.rs` | 178 | `1be8d30af88d104b137fd25713fe981cd69a02476ba9075a4b8ef71da991c5af` |
| `crates/vulcan-plugin-runtime/src/runtime/authorization.rs` | 468 | `9f6ea810e53c6e02663309b18882651ca5d3014e3e62d7b019490f43c8343745` |
| `crates/vulcan-plugin-runtime/src/runtime/authorization/tests.rs` | 70 | `926101b73f8c2dc67e5e8a5e204b4bb0ccbc5b772c76e3373478311716aa38a6` |

## 所有权与范围

- environment持有不可变环境，process持有清单启动规格；configuration拥有配置文档，authorization拥有精确持久授权与脱敏预览，installation拥有完整安装/更新/卸载文件事务。
- 公开预览类型、权限键、LaunchSpec、安装结果与must_use更新事务保持根显式重导出，lib原导出不变。授权记录字段及更新事务字段仍私有；更新测试随事务迁入，不靠扩大destination可见性通过。
- 当前15范围变换为13生产声明（含授权记录类型）和2共享测试夹具；没有复制共享Arc状态、注册表、引擎、锁或测试夹具。
- Windows常量/条件、空Error实现及根include_str和3个CARGO_MANIFEST_DIR宏保持。旧授权测试修复不改变任何生产逻辑，见06-TEST-REPAIRS.md。
- 根仍121生产节点及16跨域测试/9辅助节点；桥接、生命周期、排空、Lua租约/会话等继续。阶段映射完整不代表整个06已完成。
