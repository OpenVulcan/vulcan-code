# 显式测试入口

`#[ignore]` 不等于测试无价值；应按依赖、费用和进程隔离要求选择执行入口。不得直接对工作区执行无过滤的 `--ignored`：其中有计费模型、下载任务和长期运行的浏览器宿主。

## 离线全局回调门禁

```powershell
node scripts/test-isolated-callbacks.mjs
```

该入口已接入 Quality gates。白名单定义在脚本中，每项使用 Nextest 独立进程并串行执行，空匹配直接失败。以下六项保留忽略标记，避免普通 `cargo test` 在同一进程内与其他测试争用全局回调：

- `real_skill_runlua_and_system_calls_overlap_inside_host_callbacks`
- `pending_and_committed_provider_installations_have_exact_owners`
- `pending_callbacks_roll_back_before_service_publication`
- `stale_in_flight_projection_cannot_regress_terminal_state`
- `operation_finalization_cannot_return_or_retain_in_flight_state`
- `operation_progress_production_projector_exposes_live_start`

LanceDB 的 `creates_upserts_and_searches_through_the_host_channel` 已改为正常测试：夹具显式启用受限多线程调度器，持续驱动同步通道等待期间的异步任务。它与同模块其他数据库测试一起进入 CI。

回调并发测试已纠正过时的 System 串行断言：当前锁定的 LuaSkills 0.5.9 使用 System 逻辑目录，允许它与持有进程 cwd 锁的文件型 runLua 重叠。契约归属为上游 `runtime/engine/logical_cwd.rs` 和 `system_runtime_lease_ignores_process_cwd_guard` 测试；升级依赖时必须回读，而不能恢复旧的全局串行假设。

`parent_question_to_owned_embedded_child_binds_the_question_to_the_child` 原本就是正常测试；其注释中提到的 `#[ignore]` 不是属性，不应被扫描结果误报为禁用。

## 其余显式测试分类

本清单记录审查时的名称和前置条件；真实属性和代码仍是执行权威。新增或改名时同步更新清单。配置好对应前置条件后按**单个测试名**执行：

```powershell
cargo nextest run --locked --lib -p <crate> --run-ignored only --test-threads=1 --no-tests=fail -E 'test(/::<完整函数名>$/)'
```

性能测试按需增加 `--release`；子进程专用夹具由其父测试调用，不能使用此模板独立启动。

### 性能与测量

| crate | 测试 | 前置条件 |
| --- | --- | --- |
| vulcan-context-engine | `records_offline_performance_baseline` | 显式记录性能基线 |
| vulcan-lancedb-engine | `performance_lancedb_operations_report_p50_p95` | 显式性能测量 |
| vulcan-sqlite-engine | `performance_batch_write_and_query_stream_report_p50_p95`、`performance_concurrent_reads_report_p50_p95`、`performance_connection_open_cost_breakdown`、`performance_fts_search_reports_p50_p95` | 显式性能测量 |
| vulcan-workspace-store | `in_process_native_compaction_meets_vce_performance_targets`、`shared_storage_scale_baseline`、`performance_active_tail_sizes_report_p50_p95`、`performance_completed_turn_phase_costs` | 完整性能矩阵或 Release 基线 |
| vulcan-agent-runtime | `vm_pool_startup_resource_probe` | `VULCAN_VM_PROBE_PROFILE` 选择 `old` 或 `new`，分别使用独立进程 |
| vulcan-service-kernel | `real_plugin_empty_database_stage_measurements` | `VULCAN_PLUGIN_TEST_LUA_RUNTIME` 指向已安装的原生 Lua 包 |

### 外部运行时与本地夹具

| crate | 测试 | 前置条件 |
| --- | --- | --- |
| vulcan-service-runtime | `node_version_check_accepts_verbatim_executable_path` | `VULCAN_MANAGED_NODE_EXECUTABLE` |
| vulcan-service-runtime | `native_installer_downloads_complete_runtime_layout` | 网络与独立 `VULCAN_MANAGED_RUNTIME_SMOKE_ROOT`，会下载完整运行时 |
| vulcan-service-runtime | `native_bootstrap_is_idempotent` | 上述完整运行时布局 |
| vulcan-agent-runtime | `session_owner_hooks_reach_two_real_mcp_processes` | `VULCAN_W14_NODE_EXE` |
| vulcan-workspace-store | `in_process_session_v1_lifecycle_and_failures` | 受管 Python，用于外部 SQLite 锁持有者 |
| vulcan-workspace-store | `real_goal_failure_snapshot_recovers_without_losing_events` | `VULCAN_GOAL_RECOVERY_METADATA` 指定失败快照 |
| vulcan-workspace-store | `metadata_migration_external_session_copy` | `VULCAN_METADATA_MIGRATION_FIXTURE` 指定会话夹具 |
| vulcan-service-kernel | `real_hidden_node_retains_diagnostics_without_console_handles` | 打包的 Node 运行时 |
| vulcan-service-kernel | `real_core_host_plugin_mcp_browser_switch_revokes_old_grant` | 打包 Chromium、编译宿主和注册表安装的 Playwright MCP |
| vulcan-service-kernel | `real_startup_and_restart_leave_uninitialized_projects_cold` | 插件、LuaSkills 运行时以及两个服务子进程 |

### 真实模型、人工宿主与子进程

| crate | 测试 | 执行限制 |
| --- | --- | --- |
| vulcan-service-kernel | `live_saved_fixed_model_generates_git_summary_without_mutating_user_settings` | 明确允许计费调用并设置 `GIT_SUMMARY_LIVE_ACCEPTANCE=1` |
| vulcan-service-kernel | `live_saved_models_index_and_retrieve_temporary_project` | 明确允许计费调用，设置 `VCE_LIVE_ACCEPTANCE=1` 与 `VCE_LIVE_DIMENSION` |
| vulcan-service-kernel | `live_saved_routes_execute_native_vmm_contract_and_lifecycle` | 明确允许计费调用并设置 `NATIVE_VMM_LIVE_ACCEPTANCE=1` |
| vulcan-service-kernel | `real_browser_vce_lifecycle_host` | 人工浏览器宿主，必须设置隔离 `VULCAN_VCE_BROWSER_TEST_ROOT` |
| vulcan-service-kernel | `real_controller_policy_components_over_http`、`real_controller_native_workbench_over_http` | 使用各自的私有浏览器验收入口，不作为自动结束的普通测试 |
| vulcan-workspace-store | `writer_lease_child` | 仅由有界父测试以专用环境启动 |
