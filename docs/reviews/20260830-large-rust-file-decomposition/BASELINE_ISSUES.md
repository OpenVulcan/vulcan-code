# 拆分前验证阻塞：内置清单字节与索引不一致

## 状态与范围边界

- 问题编号：基线问题一。
- 发现时间：2026-08-30；尚未修改任何 Rust 生产代码或测试。
- 基线提交：`111540689b777ddc1229a6f17293a42818f2155e`，开始时工作树干净。
- 初次发现时仅记录并请求用户确认；随后用户明确指示“按照实际的名单 测试都是老的”。现按当前仓库真实清单修正资源一致性，并逐项核对测试是否过时；不放宽校验，不删除或跳过测试。
- 仓库规则要求未修改模块的既有问题不得越界修复；因此此项必须与结构性拆分分开确认。

## 实际验证结果

| 验证命令 | 结果 |
| --- | --- |
| `rtk proxy cargo test -p vulcan-provider-adapters --lib runtime:: -- --list` | 成功发现 125 项测试，0 项基准 |
| `rtk proxy cargo test -p vulcan-provider-adapters --lib runtime:: -- --test-threads=1` | 78 通过、47 失败、0 忽略、519 被过滤 |
| `rtk proxy cargo test -p vulcan-provider-adapters --lib runtime::tests::active_selection_uses_exact_configured_connection -- --exact --test-threads=1 --nocapture` | 单独复现，仍报 OpenRouter 未注册；不是并发争用 |
| `rtk proxy cargo run -p vulcan-provider-manifest-cli -- validate-all` | 失败，Alibaba 索引声明 312144 字节，实际 322161 字节 |

编译阶段成功；链接器产生原有库／对象创建提示警告。测试失败不能视为拆分引入，但也不能被当成验收通过。

## 源码证据与根因

1. `crates/vulcan-runtime-assets/src/lib.rs:367` 优先读取解析出的打包资源目录，否则读取编译时源码资源目录；环境中未设置 `VULCAN_CODE_OUTPUT_ROOT`。
2. `crates/vulcan-provider-adapters/src/provider_manifest/compiler.rs:691` 从资源解析器读取内置索引与清单目录。
3. 同文件 `load_indexed_bundled_provider_manifest` 在解析 JSON 前校验实际字节数和 SHA-256；不一致就隔离该供应商。
4. `git ls-files --eol` 显示选查文件为 `i/lf w/crlf`；系统 Git 配置 `C:/Program Files/Git/etc/gitconfig` 设置 `core.autocrlf=true`，资源目录没有专属换行属性。
5. 对索引声明的全部 17 份清单只读比较：工作区原字节、仅在内存中归一 LF 的字节、`git show HEAD:<精确路径>` 原始字节及索引摘要。
6. 全部工作区内容归一 LF 后均与提交中的原字节完全一致。因此工作区没有业务内容漂移。
7. 当前 16 份工作区清单不匹配索引。12 份清单的提交字节匹配索引，但被检出换行转换破坏；另外 5 份连提交字节也不匹配索引，不能仅修复换行就宣告解决。
8. Tavily 是其中的特殊情况：当前 CRLF 工作区字节恰好匹配索引，提交中的 LF 字节不匹配。不得据此对全部文件猜测同一种历史格式。

## 全量字节核查

“匹配”均同时要求正确字节数和 SHA-256。归一过程只在内存中进行，未修改文件。

| 供应商 | 索引字节数 | 工作区字节数 | 提交／归一 LF 字节数 | 工作区匹配 | 提交匹配 |
| --- | ---: | ---: | ---: | --- | --- |
| alibaba | 312144 | 322161 | 312144 | 否 | 是 |
| anthropic | 12803 | 13260 | 12803 | 否 | 是 |
| apikeyfun | 52064 | 53028 | 52064 | 否 | 是 |
| claudeapi | 3084 | 3185 | 3084 | 否 | 是 |
| code0 | 3459 | 3571 | 3459 | 否 | 是 |
| commandcode | 61703 | 63950 | 61703 | 否 | 是 |
| deepseek | 19361 | 19604 | 18950 | 否 | 否 |
| fennoai | 3277 | 3386 | 3277 | 否 | 是 |
| google | 15339 | 15862 | 15339 | 否 | 是 |
| kimi | 43432 | 44037 | 42607 | 否 | 否 |
| minimax | 178588 | 180599 | 174784 | 否 | 否 |
| openai | 11633 | 12045 | 11633 | 否 | 是 |
| opencode-go | 30879 | 31685 | 30879 | 否 | 是 |
| openrouter | 24071 | 24213 | 23405 | 否 | 否 |
| qiniu | 3302 | 3411 | 3302 | 否 | 是 |
| tavily | 9817 | 9817 | 9512 | 是 | 否 |
| tokenrhythm | 30530 | 31260 | 30530 | 否 | 是 |

## 待确认的修复边界

建议另行授权：统一被索引资源的确定性换行规则，保留供应商业务声明，通过现有构建入口重建并验证完整索引，按仓库要求同步资源并复跑基线。不修改安全校验或给测试增加绕过路径。

用户已明确以实际名单为准，并允许先治理旧测试。此项已按当前 17 份清单完成确定性换行和索引重建：新增局部 `.gitattributes`，生成第 26 版索引，未改变供应商／模型业务声明。`validate-all` 全部通过，原始 125 项运行时测试全部通过，未删改任何测试。后续其他失败仍逐项核对，不能先假设无效。拆分目标未完成，连续五轮审核尚未开始，计数仍为 0／5。

## 后续独立验证

资源问题确认期间已验证会话核心包：`rtk proxy cargo test -p vulcan-session-core --lib -- --test-threads=1`，247 项全部通过，0 忽略。已完成 `SessionDocument` 类型／职责迁移规划，尚未修改代码；资源基线修复后继续逐文件闭环。

## 原始失败测试

以下为基线运行的失败名称，用于后续比较；完整发现清单见 [原始测试清单](01-RUNTIME_TEST_BASELINE.md)。

```text
runtime::tests::active_selection_uses_exact_configured_connection
runtime::tests::apikeyfun_catalog_does_not_inherit_account_plan_filtering
runtime::tests::apikeyfun_manifest_authorizes_discovery_before_credential_access
runtime::tests::apikeyfun_manifest_uses_model_protocol_binding
runtime::tests::apikeyfun_rejects_discovered_model_outside_declared_whitelist
runtime::tests::bundled_resource_constraints_resolve_without_provider_hardcoding
runtime::tests::capability_override_candidate_validates_every_account
runtime::tests::capability_override_candidate_validates_final_resolved_limits
runtime::tests::concurrent_configuration_activation_never_mixes_catalog_generations
runtime::tests::concurrent_selection_observes_whole_generations
runtime::tests::connection_model_access_isolated_between_accounts
runtime::tests::connection_model_selection_uses_exact_session_account
runtime::tests::deepseek_manifest_identity_supplies_current_protocol_variants
runtime::tests::disabled_connection_default_hides_future_discovery
runtime::tests::explicit_empty_model_profiles_clear_route_special_cases
runtime::tests::explicit_model_parameter_defaults_are_applied_before_validation
runtime::tests::hot_replaced_manifest_drives_new_bindings
runtime::tests::identical_system_manifest_replacement_is_a_generation_noop
runtime::tests::manifest_account_model_access_projects_commandcode_catalog
runtime::tests::manifest_account_plan_allowlist_filters_catalog_models
runtime::tests::manifest_only_refresh_performs_no_credential_or_remote_discovery
runtime::tests::model_level_protocol_routing_uses_declared_bindings_per_model
runtime::tests::model_overlay_rebuild_preserves_source_catalog
runtime::tests::model_overlay_rebuild_rejects_invalid_final_limits
runtime::tests::model_overrides_are_shared_across_matching_accounts
runtime::tests::model_scoped_profiles_freeze_into_the_execution_route
runtime::tests::model_selection_degrades_after_remembered_account_is_removed
runtime::tests::model_selection_prefers_remembered_account_over_provider_default
runtime::tests::model_selection_uses_first_stable_account_without_preference
runtime::tests::pending_validation_connection_is_isolated_from_normal_routing
runtime::tests::provider_manifest_generation_build_failure_preserves_current_catalog
runtime::tests::provider_manifest_generation_publish_conflict_preserves_newer_update
runtime::tests::provider_model_management_uses_support_catalog_when_account_is_disabled
runtime::tests::provider_tool_targets_obey_account_model_access
runtime::tests::provider_tool_template_models_are_independent_from_conversation_access
runtime::tests::route_revision_tracks_only_exact_execution_semantics
runtime::tests::runtime_model_is_enabled_follows_manifest_first_precedence
runtime::tests::runtime_settings_projection_isolates_one_failed_system_provider
runtime::tests::secondary_route_cache_honors_scope_ttl_and_forced_refresh
runtime::tests::selection_uses_persisted_reasoning_override_when_runtime_catalog_is_stale
runtime::tests::session_selection_lease_reconciliation_guard_commits_removal
runtime::tests::session_selection_lease_reconciliation_guard_is_atomic_on_conflict
runtime::tests::session_selection_lease_reconciliation_guard_restores_on_drop
runtime::tests::staged_manifest_catalogs_publish_only_through_activation_batch
runtime::tests::system_manifest_binding_without_variant_is_model_level_blocked
runtime::tests::undeclared_model_on_declared_rule_required_provider_is_not_executable
runtime::tests::workbench_models_require_bidirectional_text_conversation
```

## 02号拆分阶段发现的既有静态检查边界

2026-08-31，对服务内核生产库执行严格Clippy，唯一失败为 `crates/vulcan-service-kernel/src/workbench_refresh.rs:400` 的 `clippy::collapsible_if`。当前文件Git对象与冻结基线提交均为 `8a61c080ce2818904c2340bf614e4a287ee2d324`，确认不是拆分新增内容；未修改或压制此处警告。默认Clippy退出成功但仍保留该警告，严格Clippy不能记为通过。

02号第一轮包级回归为787通过、1失败、0忽略；失败用例 `tests::capability_metadata_tests::capability_metadata_reopen_and_rebuild_rpc_preserves_conversation` 在 `tests/test_support.rs:469` 建立真实隔离控制器连接时遭到回环端口拒绝，尚未进入被测设置RPC。当前测试与夹具Git对象分别为 `4eeb97a2258da1224dd34e4c14afc8ff373dfcd1`、`f135726022546a8a4a80e3f7a6c4e204253536ce`，均与冻结基线相同。源码表明该测试验证重开/重建后保全会话，仍有实际意义；未删除、忽略或放宽断言。随后相同用例单独重跑1通过。完整包级复跑结果另记于02阶段审核，不用单项复跑掩盖首轮失败或宣称已修复控制器生命周期。

02号前三阶段完整包级复跑最终788通过、0失败、0忽略（142.89秒），未对控制器、夹具或测试断言作修复。首轮连接失败仍作为实测现象保留；后续再次出现时应继续采集控制器启动/租约证据，不能推测已永久解决。
