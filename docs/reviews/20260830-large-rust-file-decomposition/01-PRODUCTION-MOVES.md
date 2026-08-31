# 01：运行时生产符号迁移映射

## 快照与核验范围

- 更新时间：2026-08-31，十三段职责拆分及逐文件终审通过；全部产物继续纳入最终全局审核。
- 原始文件：`crates/vulcan-provider-adapters/src/runtime.rs`，基线 20951 行；当前根文件 631 行。
- 基线内容摘要：`ed88476f46893a48e1bab18c5b2fedf91dcff796aa5dfd926f1a25da3822e899`。
- CodeKit 原始和当前结构均为 430 个类型／函数节点，其中原生产区域 280 个、原测试／夹具区域 150 个；不存在遗漏、重复或新增符号。
- 已迁出 260 个原生产区域节点；其余 20 个仍在根文件。原生产区域含仍有效的测试专用目录适配函数，现归入策略测试支持文件；节点表包含带实现的特征成员，特征声明本身另覆盖无默认实现的签名。
- 每个节点按完整声明／函数体对比 Rust 词法序列，忽略注释、排版、必要的父模块可见性和可选尾逗号，保留字符串／原始字符串字节与实际表达式；280 个节点全部等价。全部 430 个节点前置属性一致。
- 每一段另外比较扣除精确迁移块和模块导入之后的完整根文件词法序列，均等价；不只依赖单个符号抽样。新增模块入口、导入、可见性及测试接线另行审核。

## 生产类型与函数

原始行号对应冻结基线；当前路径相对于 `crates/vulcan-provider-adapters/src/`。

| 原始符号 | 原始行 | 当前文件 | 当前行 | 结果 |
| --- | --- | --- | --- | --- |
| `media_understanding_uses_provider_upload` | 61～69 | `runtime/media.rs` | 32～40 | 等价 |
| `struct:PreparedLocalMediaReference` | 74～81 | `runtime/media.rs` | 45～52 | 等价 |
| `struct:PrepareLocalMediaRequest` | 85～110 | `runtime/media.rs` | 56～81 | 等价 |
| `struct:RuntimeMediaUploadProgressContext` | 115～125 | `runtime/media.rs` | 86～96 | 等价 |
| `RuntimeMediaUploadProgressContext::emit` | 135～151 | `runtime/media.rs` | 106～122 | 等价 |
| `RuntimeMediaUploadProgressContext::byte_callback` | 160～172 | `runtime/media.rs` | 131～143 | 等价 |
| `struct:RuntimeMediaUploadTerminalGuard` | 177～181 | `runtime/media.rs` | 148～152 | 等价 |
| `RuntimeMediaUploadTerminalGuard::new` | 196～200 | `runtime/media.rs` | 167～171 | 等价 |
| `RuntimeMediaUploadTerminalGuard::disarm` | 204～206 | `runtime/media.rs` | 175～177 | 等价 |
| `Drop for RuntimeMediaUploadTerminalGuard::drop` | 212～222 | `runtime/media.rs` | 183～193 | 等价 |
| `struct:ProviderExecutionCredentialLease` | 258～277 | `runtime/credentials.rs` | 21～40 | 等价 |
| `ProviderExecutionCredentialLease::new` | 295～310 | `runtime/credentials.rs` | 58～73 | 等价 |
| `ProviderExecutionCredentialLease::resolve_for_request` | 320～331 | `runtime/credentials.rs` | 83～94 | 等价 |
| `ProviderExecutionCredentialLease::refresh_after_unauthorized` | 349～383 | `runtime/credentials.rs` | 112～146 | 等价 |
| `ProviderExecutionCredentialLease::mark_request_succeeded` | 387～390 | `runtime/credentials.rs` | 150～153 | 等价 |
| `ProviderExecutionCredentialLease::refresh_expiring_secret` | 394～409 | `runtime/credentials.rs` | 157～172 | 等价 |
| `ProviderExecutionCredentialLease::refresh_lock` | 419～431 | `runtime/credentials.rs` | 182～194 | 等价 |
| `ProviderExecutionCredentialLease::refresh_locked_secret` | 447～476 | `runtime/credentials.rs` | 210～239 | 等价 |
| `observe_provider_task_records_bounded` | 492～513 | `runtime/async_tasks.rs` | 45～66 | 等价 |
| `remaining_provider_operation_timeout` | 527～535 | `runtime.rs` | 87～95 | 等价 |
| `install_process_rustls_crypto_provider` | 548～559 | `runtime.rs` | 104～115 | 等价 |
| `enum:CatalogDiscoveryState` | 565～596 | `runtime/catalog.rs` | 31～62 | 等价 |
| `enum:CatalogDiscoveryFailureKind` | 602～621 | `runtime/catalog.rs` | 68～87 | 等价 |
| `struct:CatalogDiscoveryStatus` | 627～648 | `runtime/catalog.rs` | 93～114 | 等价 |
| `struct:IsolatedConnectionCatalogObservation` | 653～663 | `runtime/catalog.rs` | 119～129 | 等价 |
| `struct:StagedConnectionCatalogObservation` | 668～681 | `runtime/catalog.rs` | 134～147 | 等价 |
| `model_supports_text_conversation` | 696～700 | `runtime/catalog_policy/capabilities.rs` | 24～28 | 等价 |
| `model_supports_reasoning` | 715～723 | `runtime/catalog_policy/capabilities.rs` | 43～51 | 等价 |
| `await_bounded_provider_task_operation` | 738～752 | `runtime/async_tasks.rs` | 81～95 | 等价 |
| `provider_task_timeout_diagnostic` | 766～777 | `runtime/async_tasks.rs` | 109～120 | 等价 |
| `clamp_provider_task_observation_delay_ms` | 792～798 | `runtime/async_tasks.rs` | 135～141 | 等价 |
| `struct:SecondaryRouteCacheEntry` | 803～813 | `runtime/secondary_routes.rs` | 24～34 | 等价 |
| `trait:CredentialSecretResolver` | 818～856 | `runtime/credentials.rs` | 245～283 | 等价 |
| `trait CredentialSecretResolver::read_secret` | 848～855 | `runtime/credentials.rs` | 275～282 | 等价 |
| `trait:CredentialSecretStore` | 861～925 | `runtime/credentials.rs` | 288～352 | 等价 |
| `struct:IsolatedCredentialSecretStore` | 929～939 | `runtime/credentials.rs` | 356～366 | 等价 |
| `IsolatedCredentialSecretStore::new` | 955～965 | `runtime/credentials.rs` | 382～392 | 等价 |
| `IsolatedCredentialSecretStore::owns` | 979～981 | `runtime/credentials.rs` | 406～408 | 等价 |
| `CredentialSecretResolver for IsolatedCredentialSecretStore::read_optional_secret` | 988～999 | `runtime/credentials.rs` | 415～426 | 等价 |
| `CredentialSecretStore for IsolatedCredentialSecretStore::create_secret` | 1006～1031 | `runtime/credentials.rs` | 433～458 | 等价 |
| `CredentialSecretStore for IsolatedCredentialSecretStore::replace_secret` | 1035～1073 | `runtime/credentials.rs` | 462～500 | 等价 |
| `CredentialSecretStore for IsolatedCredentialSecretStore::delete_secret` | 1077～1086 | `runtime/credentials.rs` | 504～513 | 等价 |
| `CredentialSecretStore for IsolatedCredentialSecretStore::restore_secret` | 1090～1110 | `runtime/credentials.rs` | 517～537 | 等价 |
| `struct:UnavailableCredentialSecretResolver` | 1116～1116 | `runtime/credentials.rs` | 543～543 | 等价 |
| `CredentialSecretResolver for UnavailableCredentialSecretResolver::read_optional_secret` | 1122～1129 | `runtime/credentials.rs` | 549～556 | 等价 |
| `CredentialSecretStore for UnavailableCredentialSecretResolver::create_secret` | 1136～1144 | `runtime/credentials.rs` | 563～571 | 等价 |
| `CredentialSecretStore for UnavailableCredentialSecretResolver::replace_secret` | 1148～1157 | `runtime/credentials.rs` | 575～584 | 等价 |
| `CredentialSecretStore for UnavailableCredentialSecretResolver::delete_secret` | 1161～1165 | `runtime/credentials.rs` | 588～592 | 等价 |
| `CredentialSecretStore for UnavailableCredentialSecretResolver::restore_secret` | 1169～1177 | `runtime/credentials.rs` | 596～604 | 等价 |
| `struct:PublicAiRuntime` | 1183～1258 | `runtime.rs` | 120～194 | 等价 |
| `struct:ResolvedProviderGeneration` | 1271～1287 | `runtime.rs` | 207～223 | 等价 |
| `struct:PreparedSystemManifestReplacement` | 1292～1302 | `runtime/activation.rs` | 22～32 | 等价 |
| `struct:ProviderStagedManifestCatalogMutation` | 1311～1318 | `runtime/activation.rs` | 41～48 | 等价 |
| `struct:ProviderRuntimeActivationSnapshot` | 1323～1348 | `runtime.rs` | 228～253 | 等价 |
| `struct:ProviderCatalogActivationTransaction` | 1352～1359 | `runtime/activation.rs` | 52～59 | 等价 |
| `struct:SessionSelectionLeaseAbsenceGuard` | 1367～1371 | `runtime/session_leases.rs` | 20～24 | 等价 |
| `struct:SessionSelectionLeaseReconciliationGuard` | 1381～1391 | `runtime/session_leases.rs` | 34～44 | 等价 |
| `SessionSelectionLeaseReconciliationGuard<'_>::commit` | 1401～1403 | `runtime/session_leases.rs` | 54～56 | 等价 |
| `Drop for SessionSelectionLeaseReconciliationGuard<'_>::drop` | 1409～1416 | `runtime/session_leases.rs` | 62～69 | 等价 |
| `struct:PublicAiRuntimeSettingsCatalogSnapshot` | 1421～1449 | `runtime/catalog_views.rs` | 25～53 | 等价 |
| `struct:ProviderManifestRuntimeSnapshot` | 1454～1473 | `runtime/catalog_views.rs` | 58～77 | 等价 |
| `struct:ResponsesCompactExecution` | 1478～1497 | `runtime/model_execution.rs` | 19～38 | 等价 |
| `validate_responses_compact_execution_route` | 1511～1530 | `runtime/model_execution.rs` | 52～71 | 等价 |
| `runtime_model_default_enabled_from_manifest` | 1547～1570 | `runtime/catalog_views.rs` | 94～117 | 等价 |
| `runtime_model_is_enabled_from_manifest` | 1587～1606 | `runtime/catalog_views.rs` | 134～153 | 等价 |
| `build_activated_provider_manifest_catalog` | 1622～1677 | `runtime/activation.rs` | 75～130 | 等价 |
| `runtime_configuration_for_manifest_catalog` | 1697～1713 | `runtime/activation.rs` | 150～166 | 等价 |
| `effective_catalog_snapshot_from_generation` | 1727～1731 | `runtime/catalog_views.rs` | 167～171 | 等价 |
| `resolved_model_source_policy` | 1753～1803 | `runtime/catalog_sources.rs` | 27～77 | 等价 |
| `resolve_custom_model_service_entry` | 1820～1861 | `runtime/catalog_sources.rs` | 94～135 | 等价 |
| `model_discovery_is_remote` | 1875～1886 | `runtime/catalog_sources.rs` | 149～160 | 等价 |
| `empty_model_source_catalog` | 1900～1912 | `runtime/catalog_sources.rs` | 174～186 | 等价 |
| `project_manifest_protocol_variants_for_connection` | 1931～2039 | `runtime/catalog_sources.rs` | 205～313 | 等价 |
| `build_manifest_identity_catalog` | 2055～2113 | `runtime/catalog_sources.rs` | 329～387 | 等价 |
| `merge_manifest_identity_catalog` | 2122～2149 | `runtime/catalog_sources.rs` | 396～423 | 等价 |
| `apply_model_catalog_overlays_from_generation` | 2166～2186 | `runtime/catalog_views.rs` | 188～208 | 等价 |
| `resolve_model_catalog_overlays_from_inputs` | 2203～2273 | `runtime/catalog_views.rs` | 225～295 | 等价 |
| `provider_conversation_model_is_supported_in_generation` | 2289～2325 | `runtime/catalog_views.rs` | 311～347 | 等价 |
| `provider_tool_template_models_in_generation` | 2343～2413 | `runtime/catalog_views.rs` | 365～435 | 等价 |
| `provider_tool_target_allows_credential` | 2429～2437 | `runtime/tool_targets.rs` | 117～125 | 等价 |
| `validate_session_selection_lease_admission` | 2453～2469 | `runtime/session_leases.rs` | 86～102 | 等价 |
| `PublicAiRuntime::first_batch` | 2484～2489 | `runtime.rs` | 268～273 | 等价 |
| `PublicAiRuntime::first_batch_with_http_client_for_testing` | 2506～2513 | `runtime.rs` | 290～297 | 等价 |
| `PublicAiRuntime::first_batch_from_components` | 2529～2589 | `runtime.rs` | 313～371 | 等价 |
| `PublicAiRuntime::set_runtime_media_upload_progress_observer` | 2603～2613 | `runtime/media.rs` | 209～219 | 等价 |
| `PublicAiRuntime::unavailable` | 2622～2624 | `runtime.rs` | 380～382 | 等价 |
| `PublicAiRuntime::providers` | 2635～2641 | `runtime.rs` | 391～397 | 等价 |
| `PublicAiRuntime::effective_providers` | 2659～2669 | `runtime.rs` | 415～425 | 等价 |
| `PublicAiRuntime::configuration_snapshot` | 2678～2681 | `runtime/activation.rs` | 176～179 | 等价 |
| `PublicAiRuntime::runtime_configuration_snapshot` | 2701～2715 | `runtime/activation.rs` | 199～213 | 等价 |
| `PublicAiRuntime::replace_configuration` | 2724～2729 | `runtime/activation.rs` | 222～227 | 等价 |
| `PublicAiRuntime::protocols` | 2733～2735 | `runtime.rs` | 434～436 | 等价 |
| `PublicAiRuntime::metadata_client` | 2739～2741 | `runtime.rs` | 445～447 | 等价 |
| `PublicAiRuntime::catalog_generation` | 2750～2755 | `runtime/activation.rs` | 236～241 | 等价 |
| `PublicAiRuntime::catalog_content_hash` | 2764～2769 | `runtime/activation.rs` | 250～255 | 等价 |
| `PublicAiRuntime::advance_catalog_state_revision_locked` | 2778～2786 | `runtime.rs` | 456～464 | 等价 |
| `PublicAiRuntime::capture_activation_snapshot` | 2795～2834 | `runtime/activation.rs` | 282～321 | 等价 |
| `PublicAiRuntime::restore_activation_snapshot` | 2850～2907 | `runtime/activation.rs` | 337～394 | 等价 |
| `PublicAiRuntime::isolated_catalog_activation_runtime` | 2923～2952 | `runtime/activation.rs` | 410～439 | 等价 |
| `PublicAiRuntime::publish_isolated_catalog_candidate` | 2971～3048 | `runtime/activation.rs` | 458～535 | 等价 |
| `PublicAiRuntime::session_selection_lease` | 3062～3070 | `runtime/session_leases.rs` | 117～125 | 等价 |
| `PublicAiRuntime::lease_session_selection` | 3086～3106 | `runtime/session_leases.rs` | 141～161 | 等价 |
| `PublicAiRuntime::clear_session_selection_lease` | 3121～3129 | `runtime/session_leases.rs` | 176～184 | 等价 |
| `PublicAiRuntime::clear_session_selection_leases` | 3143～3146 | `runtime/session_leases.rs` | 198～201 | 等价 |
| `PublicAiRuntime::acquire_session_selection_lease_absence_guard` | 3162～3181 | `runtime/session_leases.rs` | 217～236 | 等价 |
| `PublicAiRuntime::acquire_session_selection_lease_reconciliation_guard` | 3200～3235 | `runtime/session_leases.rs` | 255～290 | 等价 |
| `PublicAiRuntime::oauth` | 3244～3246 | `runtime.rs` | 473～475 | 等价 |
| `PublicAiRuntime::credential_secret_store` | 3255～3257 | `runtime.rs` | 484～486 | 等价 |
| `PublicAiRuntime::replace_catalog` | 3266～3279 | `runtime/catalog.rs` | 157～170 | 等价 |
| `PublicAiRuntime::replace_catalog_locked` | 3293～3299 | `runtime/catalog.rs` | 184～193 | 等价 |
| `PublicAiRuntime::stage_source_catalog_for_testing` | 3319～3338 | `runtime/catalog.rs` | 213～232 | 等价 |
| `PublicAiRuntime::catalog_snapshot` | 3347～3350 | `runtime/catalog.rs` | 241～244 | 等价 |
| `PublicAiRuntime::settings_catalog_snapshot` | 3368～3526 | `runtime/catalog_views.rs` | 454～612 | 等价 |
| `PublicAiRuntime::model_default_enabled` | 3543～3612 | `runtime/catalog_views.rs` | 629～698 | 等价 |
| `PublicAiRuntime::model_is_enabled_for_connection` | 3629～3654 | `runtime/catalog_views.rs` | 715～740 | 等价 |
| `PublicAiRuntime::model_is_enabled` | 3670～3681 | `runtime/catalog_views.rs` | 756～767 | 等价 |
| `PublicAiRuntime::selection_catalog_snapshot` | 3692～3697 | `runtime/activation.rs` | 266～273 | 等价 |
| `PublicAiRuntime::replace_system_manifest` | 3712～3719 | `runtime/activation.rs` | 549～556 | 等价 |
| `PublicAiRuntime::replace_system_manifest_transactional` | 3733～3748 | `runtime/activation.rs` | 570～585 | 等价 |
| `PublicAiRuntime::prepare_system_manifest_replacement` | 3762～3788 | `runtime/activation.rs` | 599～625 | 等价 |
| `PublicAiRuntime::publish_prepared_system_manifest` | 3803～3809 | `runtime/activation.rs` | 640～646 | 等价 |
| `PublicAiRuntime::publish_prepared_system_manifest_transactional` | 3823～3867 | `runtime/activation.rs` | 660～704 | 等价 |
| `PublicAiRuntime::restore_staged_system_manifest_catalog` | 3882～3902 | `runtime/activation.rs` | 719～739 | 等价 |
| `PublicAiRuntime::system_manifest_revisions` | 3911～3922 | `runtime/activation.rs` | 748～759 | 等价 |
| `PublicAiRuntime::system_manifest_provider_ids` | 3931～3941 | `runtime/activation.rs` | 768～778 | 等价 |
| `PublicAiRuntime::bundled_manifest_failure` | 3955～3968 | `runtime/activation.rs` | 792～805 | 等价 |
| `PublicAiRuntime::system_manifest` | 3982～3987 | `runtime/activation.rs` | 819～824 | 等价 |
| `PublicAiRuntime::provider_manifest_runtime_snapshot` | 4002～4050 | `runtime/activation.rs` | 839～887 | 等价 |
| `PublicAiRuntime::staged_system_manifest` | 4065～4073 | `runtime/activation.rs` | 902～910 | 等价 |
| `PublicAiRuntime::model_is_available` | 4090～4123 | `runtime/catalog_views.rs` | 784～817 | 等价 |
| `PublicAiRuntime::available_model_selectors` | 4139～4178 | `runtime/catalog_views.rs` | 833～872 | 等价 |
| `PublicAiRuntime::catalog_error_snapshot` | 4187～4197 | `runtime/catalog.rs` | 253～263 | 等价 |
| `PublicAiRuntime::source_catalog_snapshot` | 4206～4213 | `runtime/catalog.rs` | 272～279 | 等价 |
| `PublicAiRuntime::staged_source_catalog` | 4232～4246 | `runtime/catalog.rs` | 298～312 | 等价 |
| `PublicAiRuntime::stage_connection_catalog_observation` | 4264～4285 | `runtime/catalog.rs` | 330～351 | 等价 |
| `PublicAiRuntime::stage_connection_catalog_observations_if_current` | 4301～4429 | `runtime/catalog.rs` | 367～495 | 等价 |
| `PublicAiRuntime::catalog_status_snapshot` | 4438～4450 | `runtime/catalog.rs` | 504～516 | 等价 |
| `PublicAiRuntime::remove_connection_catalog` | 4459～4495 | `runtime/catalog.rs` | 525～561 | 等价 |
| `PublicAiRuntime::record_catalog_refresh_failure` | 4511～4557 | `runtime/catalog.rs` | 577～623 | 等价 |
| `PublicAiRuntime::refresh_connection_catalog` | 4571～4603 | `runtime/catalog.rs` | 637～669 | 等价 |
| `PublicAiRuntime::observe_connection_catalog_candidate` | 4621～4683 | `runtime/catalog.rs` | 687～749 | 等价 |
| `PublicAiRuntime::begin_catalog_activation_transaction` | 4694～4703 | `runtime/activation.rs` | 921～930 | 等价 |
| `PublicAiRuntime::activate_staged_manifest_catalogs` | 4719～4731 | `runtime/activation.rs` | 946～958 | 等价 |
| `PublicAiRuntime::activate_staged_manifest_catalogs_in_transaction` | 4747～4848 | `runtime/activation.rs` | 974～1075 | 等价 |
| `PublicAiRuntime::refresh_connection_catalog_from_generation` | 4865～5070 | `runtime/catalog.rs` | 767～972 | 等价 |
| `PublicAiRuntime::rebuild_catalog_after_usage` | 5086～5118 | `runtime/catalog.rs` | 1049～1081 | 等价 |
| `PublicAiRuntime::apply_model_catalog_overlays_with_account_access` | 5135～5152 | `runtime/catalog_views.rs` | 889～906 | 等价 |
| `PublicAiRuntime::reapply_model_catalog_overlays` | 5161～5205 | `runtime/activation.rs` | 1084～1128 | 等价 |
| `PublicAiRuntime::reapply_model_catalog_overlays_with_manifests` | 5221～5297 | `runtime/activation.rs` | 1144～1220 | 等价 |
| `PublicAiRuntime::validate_model_catalog_overlays` | 5312～5318 | `runtime/catalog_views.rs` | 921～927 | 等价 |
| `PublicAiRuntime::resolved_model_catalog_snapshot` | 5332～5339 | `runtime/catalog_views.rs` | 941～948 | 等价 |
| `PublicAiRuntime::preview_staged_model_catalog_overlays` | 5355～5376 | `runtime/catalog_views.rs` | 964～985 | 等价 |
| `PublicAiRuntime::provider_conversation_model_is_supported` | 5390～5405 | `runtime/catalog_views.rs` | 999～1014 | 等价 |
| `PublicAiRuntime::provider_model_is_supported` | 5422～5444 | `runtime/catalog_views.rs` | 1031～1053 | 等价 |
| `PublicAiRuntime::provider_tool_template_models` | 5459～5467 | `runtime/catalog_views.rs` | 1068～1076 | 等价 |
| `PublicAiRuntime::resolve_model_catalog_overlays` | 5481～5498 | `runtime/catalog_views.rs` | 1090～1107 | 等价 |
| `PublicAiRuntime::resolve_model_catalog_overlays_with_manifests` | 5513～5534 | `runtime/catalog_views.rs` | 1122～1143 | 等价 |
| `PublicAiRuntime::connection_catalog_contains_model` | 5552～5606 | `runtime/catalog_views.rs` | 1161～1215 | 等价 |
| `PublicAiRuntime::validate_model_capability_override_candidate` | 5623～5749 | `runtime/catalog_views.rs` | 1232～1358 | 等价 |
| `PublicAiRuntime::validate_provider_capability_override_candidate` | 5765～5805 | `runtime/catalog_views.rs` | 1374～1414 | 等价 |
| `PublicAiRuntime::validate_model_execution_override_candidate` | 5821～5867 | `runtime/catalog_views.rs` | 1430～1476 | 等价 |
| `PublicAiRuntime::install_manual_custom_catalog_without_discovery` | 5882～5915 | `runtime/catalog.rs` | 1096～1129 | 等价 |
| `PublicAiRuntime::query_connection_usage` | 5929～5960 | `runtime/usage.rs` | 57～88 | 等价 |
| `PublicAiRuntime::cache_usage_snapshot` | 5976～6013 | `runtime/usage.rs` | 104～141 | 等价 |
| `PublicAiRuntime::cached_account_usage` | 6027～6035 | `runtime/usage.rs` | 155～163 | 等价 |
| `PublicAiRuntime::usage_cache_snapshot` | 6039～6045 | `runtime/usage.rs` | 172～178 | 等价 |
| `PublicAiRuntime::usage_cache_snapshot_locked` | 6054～6061 | `runtime/usage.rs` | 187～194 | 等价 |
| `PublicAiRuntime::remove_connection_usage` | 6065～6083 | `runtime/usage.rs` | 208～226 | 等价 |
| `PublicAiRuntime::refresh_supported_connection_usage` | 6087～6123 | `runtime/usage.rs` | 240～276 | 等价 |
| `PublicAiRuntime::secondary_route_cache_identity` | 6139～6172 | `runtime/secondary_routes.rs` | 51～84 | 等价 |
| `PublicAiRuntime::insert_secondary_route_cache_entry` | 6188～6226 | `runtime/secondary_routes.rs` | 100～138 | 等价 |
| `PublicAiRuntime::discover_secondary_routes` | 6243～6292 | `runtime/secondary_routes.rs` | 155～204 | 等价 |
| `PublicAiRuntime::cached_secondary_routes` | 6306～6327 | `runtime/secondary_routes.rs` | 220～241 | 等价 |
| `PublicAiRuntime::available_provider_tool_targets` | 6341～6365 | `runtime/tool_targets.rs` | 169～193 | 等价 |
| `PublicAiRuntime::provider_tool_media_input_profile` | 6381～6479 | `runtime/media.rs` | 934～1032 | 等价 |
| `PublicAiRuntime::available_provider_tool_targets_in_generation` | 6495～6580 | `runtime/tool_targets.rs` | 209～294 | 等价 |
| `PublicAiRuntime::resolved_provider_tool_targets` | 6592～6598 | `runtime/tool_targets.rs` | 308～317 | 等价 |
| `PublicAiRuntime::resolved_provider_tool_targets_in_generation` | 6612～6621 | `runtime/tool_targets.rs` | 331～340 | 等价 |
| `PublicAiRuntime::execute_selected_provider_tool` | 6635～6643 | `runtime/tool_execution.rs` | 38～46 | 等价 |
| `PublicAiRuntime::execute_selected_provider_tool_for_owner` | 6659～6674 | `runtime/tool_execution.rs` | 62～77 | 等价 |
| `PublicAiRuntime::execute_selected_provider_tool_for_owner_in_workspace` | 6690～6714 | `runtime/tool_execution.rs` | 93～117 | 等价 |
| `PublicAiRuntime::execute_provider_tool_target_for_owner` | 6731～6748 | `runtime/tool_execution.rs` | 134～151 | 等价 |
| `PublicAiRuntime::execute_provider_tool_target_for_owner_in_workspace` | 6764～7137 | `runtime/tool_execution.rs` | 167～540 | 等价 |
| `PublicAiRuntime::configure_provider_task_recovery` | 7151～7168 | `runtime/async_tasks.rs` | 156～173 | 等价 |
| `PublicAiRuntime::provider_task_status` | 7182～7187 | `runtime/async_tasks.rs` | 187～192 | 等价 |
| `PublicAiRuntime::recover_provider_tasks` | 7201～7227 | `runtime/async_tasks.rs` | 206～232 | 等价 |
| `PublicAiRuntime::recover_provider_task_record` | 7244～7273 | `runtime/async_tasks.rs` | 249～278 | 等价 |
| `PublicAiRuntime::cancel_provider_task` | 7287～7318 | `runtime/async_tasks.rs` | 292～323 | 等价 |
| `PublicAiRuntime::request_provider_task_cancellation_for_owner` | 7332～7368 | `runtime/async_tasks.rs` | 337～373 | 等价 |
| `PublicAiRuntime::start_provider_async_task` | 7386～7511 | `runtime/async_tasks.rs` | 391～516 | 等价 |
| `PublicAiRuntime::await_provider_async_task` | 7515～7575 | `runtime/async_tasks.rs` | 520～580 | 等价 |
| `PublicAiRuntime::poll_provider_async_task_once` | 7579～7735 | `runtime/async_tasks.rs` | 584～740 | 等价 |
| `PublicAiRuntime::cancel_provider_task_internal` | 7739～7804 | `runtime/async_tasks.rs` | 744～809 | 等价 |
| `PublicAiRuntime::resolve_recovered_provider_task` | 7808～7932 | `runtime/async_tasks.rs` | 813～937 | 等价 |
| `PublicAiRuntime::record_provider_task_observation_error` | 7936～7974 | `runtime/async_tasks.rs` | 941～979 | 等价 |
| `PublicAiRuntime::resolve_provider_tool_target` | 7988～8129 | `runtime/tool_targets.rs` | 356～497 | 等价 |
| `PublicAiRuntime::resolve_connection_metadata` | 8143～8157 | `runtime.rs` | 500～514 | 等价 |
| `PublicAiRuntime::resolve_connection_metadata_from_generation` | 8173～8211 | `runtime.rs` | 530～568 | 等价 |
| `PublicAiRuntime::resolve_active_execution` | 8225～8231 | `runtime/model_execution.rs` | 109～115 | 等价 |
| `PublicAiRuntime::resolve_model_execution` | 8248～8256 | `runtime/model_execution.rs` | 133～141 | 等价 |
| `PublicAiRuntime::resolve_connection_model_execution` | 8271～8285 | `runtime/model_execution.rs` | 157～171 | 等价 |
| `PublicAiRuntime::resolve_execution_from_selection` | 8301～8322 | `runtime/model_execution.rs` | 187～208 | 等价 |
| `PublicAiRuntime::resolve_execution_from_frozen_selection` | 8345～8350 | `runtime/model_execution.rs` | 231～236 | 等价 |
| `PublicAiRuntime::resolve_fresh_execution_secret` | 8366～8374 | `runtime/credentials.rs` | 625～633 | 等价 |
| `PublicAiRuntime::execution_credential_lease` | 8390～8402 | `runtime/credentials.rs` | 649～661 | 等价 |
| `PublicAiRuntime::resolve_active_selection` | 8416～8439 | `runtime/selection.rs` | 109～132 | 等价 |
| `PublicAiRuntime::resolve_model_selection` | 8456～8484 | `runtime/selection.rs` | 150～178 | 等价 |
| `PublicAiRuntime::resolve_connection_model_selection` | 8500～8529 | `runtime/selection.rs` | 194～223 | 等价 |
| `PublicAiRuntime::resolve_internal_inference_selection` | 8545～8615 | `runtime/selection.rs` | 239～309 | 等价 |
| `PublicAiRuntime::validate_disabled_reasoning_route_projection` | 8630～8678 | `runtime/model_execution.rs` | 251～299 | 等价 |
| `PublicAiRuntime::validate_disabled_reasoning_request_projection` | 8694～8707 | `runtime/model_execution.rs` | 315～328 | 等价 |
| `PublicAiRuntime::execute_internal_embedding` | 8721～8747 | `runtime/model_execution.rs` | 342～368 | 等价 |
| `PublicAiRuntime::execute_internal_rerank` | 8761～8787 | `runtime/model_execution.rs` | 382～408 | 等价 |
| `PublicAiRuntime::prepare_and_remember_local_media` | 8804～9099 | `runtime/media.rs` | 236～531 | 等价 |
| `PublicAiRuntime::execute_internal_llm` | 9114～9122 | `runtime/model_execution.rs` | 423～431 | 等价 |
| `PublicAiRuntime::execute_internal_llm_with_wire_request_capture` | 9138～9175 | `runtime/model_execution.rs` | 447～484 | 等价 |
| `PublicAiRuntime::execute_responses_compact` | 9191～9205 | `runtime/model_execution.rs` | 500～514 | 等价 |
| `PublicAiRuntime::prepare_responses_compact_execution` | 9219～9243 | `runtime/model_execution.rs` | 528～552 | 等价 |
| `PublicAiRuntime::execute_prepared_responses_compact` | 9259～9353 | `runtime/model_execution.rs` | 568～662 | 等价 |
| `PublicAiRuntime::resolve_selection_for_connection` | 9370～9732 | `runtime/selection.rs` | 326～688 | 等价 |
| `provider_tool_target_availability_catalog_matches` | 9737～9752 | `runtime/tool_targets.rs` | 139～154 | 等价 |
| `struct:EffectiveModelProfileRefs` | 9756～9775 | `runtime/selection/profiles.rs` | 6～25 | 等价 |
| `model_rule_replaces_route_profile_chain` | 9790～9803 | `runtime/selection/profiles.rs` | 40～53 | 等价 |
| `effective_model_profile_refs` | 9819～9870 | `runtime/selection/profiles.rs` | 69～120 | 等价 |
| `resolve_input_resource_constraints` | 9886～9912 | `runtime/selection/profiles.rs` | 136～162 | 等价 |
| `resolve_usage_extraction_chain` | 9930～9967 | `runtime/selection/profiles.rs` | 180～217 | 等价 |
| `resolve_request_projection_chain` | 9978～10005 | `runtime/selection/profiles.rs` | 235～262 | 等价 |
| `resolve_response_projection_chain` | 10012～10039 | `runtime/selection/profiles.rs` | 280～307 | 等价 |
| `resolve_request_projection` | 10053～10091 | `runtime/model_overrides.rs` | 19～57 | 等价 |
| `manual_custom_catalog` | 10107～10118 | `runtime/catalog_policy.rs` | 60～71 | 等价 |
| `manual_custom_catalog_for_provider` | 10136～10185 | `runtime/catalog_policy.rs` | 89～138 | 等价 |
| `apply_model_catalog_overlays` | 10195～10258 | `runtime/catalog_policy/test_support.rs` | 19～82 | 等价 |
| `apply_host_default_reasoning_policy` | 10274～10307 | `runtime/catalog_policy/capabilities.rs` | 67～100 | 等价 |
| `resolve_custom_provider_binding` | 10326～10358 | `runtime/catalog_policy/bindings.rs` | 28～60 | 等价 |
| `custom_provider_system_baseline_for_protocol` | 10373～10383 | `runtime/catalog_policy/capabilities.rs` | 115～125 | 等价 |
| `account_usage_secret_reference` | 10398～10418 | `runtime/usage.rs` | 22～42 | 等价 |
| `normalized_usage_decimal_is_positive` | 10432～10464 | `runtime/catalog_policy/account_access.rs` | 18～50 | 等价 |
| `account_usage_unlocks_catalog` | 10478～10495 | `runtime/catalog_policy/account_access.rs` | 64～81 | 等价 |
| `account_plan_identity` | 10509～10518 | `runtime/catalog_policy/account_access.rs` | 95～104 | 等价 |
| `model_rule_allows_account_access` | 10534～10564 | `runtime/catalog_policy/account_access.rs` | 120～150 | 等价 |
| `apply_model_catalog_overlays_with_account_access` | 10581～11153 | `runtime/catalog_policy.rs` | 155～727 | 等价 |
| `freeze_custom_provider_protocol_bindings` | 11181～11265 | `runtime/catalog_policy/bindings.rs` | 88～172 | 等价 |
| `ensure_manual_custom_bound_variant` | 11276～11309 | `runtime/catalog_policy/bindings.rs` | 183～216 | 等价 |
| `freeze_system_provider_protocol_bindings` | 11325～11351 | `runtime/catalog_policy/bindings.rs` | 232～258 | 等价 |
| `project_frozen_model_binding_compatibility` | 11367～11389 | `runtime/catalog_policy/bindings.rs` | 274～296 | 等价 |
| `frozen_binding_variant_is_missing` | 11403～11412 | `runtime/catalog_policy/bindings.rs` | 310～319 | 等价 |
| `frozen_binding_is_current` | 11416～11442 | `runtime/catalog_policy/bindings.rs` | 323～349 | 等价 |
| `validate_resolved_model_capability_limits` | 11458～11482 | `runtime/catalog_policy/capabilities.rs` | 141～165 | 等价 |
| `validate_resolved_catalog_limits` | 11496～11512 | `runtime/catalog_policy/capabilities.rs` | 179～197 | 等价 |
| `apply_custom_model_advanced_declaration` | 11521～11559 | `runtime/catalog_policy/capabilities.rs` | 206～244 | 等价 |
| `apply_custom_model_defaults` | 11570～11612 | `runtime/catalog_policy/capabilities.rs` | 255～297 | 等价 |
| `manual_model_capability_evidence` | 11626～11641 | `runtime/catalog_policy/capabilities.rs` | 311～326 | 等价 |
| `set_custom_conversation_contract` | 11650～11658 | `runtime/catalog_policy/capabilities.rs` | 335～343 | 等价 |
| `apply_model_parameter_defaults` | 11674～11707 | `runtime/model_overrides.rs` | 73～106 | 等价 |
| `effective_model_catalog_content_revision` | 11721～11744 | `runtime/selection/revision.rs` | 19～42 | 等价 |
| `provider_execution_route_revision` | 11761～11852 | `runtime/selection/revision.rs` | 59～150 | 等价 |
| `struct:ResolvedPublicAiSelection` | 11862～11884 | `runtime/selection.rs` | 36～58 | 等价 |
| `struct:ResolvedPublicAiExecution` | 11888～11907 | `runtime/model_execution.rs` | 75～94 | 等价 |
| `struct:AvailableProviderToolTarget` | 11912～11925 | `runtime/tool_targets.rs` | 14～27 | 等价 |
| `struct:ProviderToolMediaInputRouteConstraint` | 11930～11943 | `runtime/media.rs` | 892～905 | 等价 |
| `struct:ProviderToolMediaInputProfile` | 11948～11955 | `runtime/media.rs` | 910～917 | 等价 |
| `struct:DeclaredProviderToolTemplateModel` | 11966～11982 | `runtime/catalog_views.rs` | 1491～1507 | 等价 |
| `provider_tool_media_resource_kind` | 11997～12006 | `runtime/media.rs` | 547～556 | 等价 |
| `struct:ProviderToolMediaUploadPreflight` | 12024～12040 | `runtime/media.rs` | 561～577 | 等价 |
| `provider_tool_media_upload_binding_is_cacheable` | 12054～12058 | `runtime/media.rs` | 591～595 | 等价 |
| `prepare_provider_tool_media_upload` | 12068～12177 | `runtime/media.rs` | 605～714 | 等价 |
| `provider_tool_materialized_reference` | 12181～12201 | `runtime/media.rs` | 718～738 | 等价 |
| `provider_tool_media_upload_progress_context` | 12206～12336 | `runtime/media.rs` | 754～884 | 等价 |
| `struct:ResolvedProviderToolTarget` | 12340～12371 | `runtime/tool_targets.rs` | 31～62 | 等价 |
| `ResolvedProviderToolTarget::diagnostic_context` | 12376～12404 | `runtime/tool_targets.rs` | 72～100 | 等价 |
| `diagnose_provider_tool_failure` | 12409～12419 | `runtime/tool_diagnostics.rs` | 26～36 | 等价 |
| `truncate_provider_tool_diagnostic_component` | 12433～12443 | `runtime/tool_diagnostics.rs` | 50～60 | 等价 |
| `diagnose_provider_tool_failure_with_secret` | 12459～12465 | `runtime/tool_diagnostics.rs` | 76～82 | 等价 |
| `redact_provider_tool_secret` | 12479～12487 | `runtime/tool_diagnostics.rs` | 96～107 | 等价 |
| `current_unix_millis` | 12496～12503 | `runtime.rs` | 578～585 | 等价 |
| `catalog_discovery_failure_kind` | 12517～12532 | `runtime/catalog.rs` | 987～1002 | 等价 |
| `discovery_state_for_failure` | 12536～12549 | `runtime/catalog.rs` | 1016～1029 | 等价 |
| `exact_active_connection` | 12553～12575 | `runtime/selection.rs` | 72～94 | 等价 |
| `validate_model_overrides` | 12579～12679 | `runtime/model_overrides.rs` | 120～220 | 等价 |
| `build_public_ai_http_client` | 12688～12695 | `runtime.rs` | 594～601 | 等价 |

## 常量归属

全部 12 个原始常量均唯一保留，类型和值的词法序列逐项一致；仅共享手工来源标记增加父模块可见性。

| 常量 | 原始行 | 当前文件 | 当前行 |
| --- | ---: | --- | ---: |
| `OAUTH_REFRESH_SKEW_MS` | 46 | `runtime/credentials.rs` | 16 |
| `SECONDARY_ROUTE_CACHE_TTL` | 227 | `runtime/secondary_routes.rs` | 15 |
| `SECONDARY_ROUTE_CACHE_MAX_ENTRIES` | 231 | `runtime/secondary_routes.rs` | 19 |
| `SESSION_SELECTION_LEASE_MAX_ENTRIES` | 234 | `runtime/session_leases.rs` | 9 |
| `SESSION_SELECTION_LEASE_ID_MAX_BYTES` | 237 | `runtime/session_leases.rs` | 12 |
| `PROVIDER_ASYNC_TASK_OBSERVATION_TIMEOUT` | 241 | `runtime/async_tasks.rs` | 27 |
| `PROVIDER_TASK_RECOVERY_MAX_CONCURRENCY` | 244 | `runtime/async_tasks.rs` | 30 |
| `LEGACY_PROVIDER_TOOL_OPERATION_TIMEOUT` | 247 | `runtime/tool_execution.rs` | 23 |
| `PROVIDER_TOOL_ERROR_DIAGNOSTIC_MAX_CHARACTERS` | 250 | `runtime/tool_diagnostics.rs` | 9 |
| `PROVIDER_TOOL_ROUTE_DIAGNOSTIC_MAX_CHARACTERS` | 253 | `runtime/tool_diagnostics.rs` | 12 |
| `HOST_DEFAULT_REASONING_EVIDENCE_REVISION` | 539 | `runtime/catalog_policy/capabilities.rs` | 9 |
| `MANUAL_MODEL_CATALOG_SCHEMA_REVISION` | 1735 | `runtime/catalog_policy.rs` | 44 |

## 当前产物内容摘要

| 文件 | 行数 | SHA-256 |
| --- | ---: | --- |
| `crates/vulcan-provider-adapters/src/runtime.rs` | 631 | `fde2f8c613dd6bc68f319c397fda66a1ca242f4c2a546319969cca64a036901e` |
| `crates/vulcan-provider-adapters/src/runtime/usage.rs` | 280 | `818f9e5a4f6ce51c17644ce5f6db9502e4cb078f7700cd2d8c5d338add603042` |
| `crates/vulcan-provider-adapters/src/runtime/usage/tests.rs` | 49 | `270444ebbd64ee11ffa027fcf2e101d8887d51f3732ebb5b6bfe232527c10831` |
| `crates/vulcan-provider-adapters/src/runtime/tool_targets.rs` | 501 | `f0140bd38a02ca960f92aaf54ca9f7ee2da946bd0e8d1ce07b48a6fe1c74e77f` |
| `crates/vulcan-provider-adapters/src/runtime/tool_targets/tests.rs` | 410 | `f1202b0f8766431f903a83bb8e4814170472a288725bbb9f2541011db4eeb35e` |
| `crates/vulcan-provider-adapters/src/runtime/tool_execution.rs` | 541 | `7c282534e2080f3aaaea7027a8943a1a8859995827b8ca6c263ded2300c0a669` |
| `crates/vulcan-provider-adapters/src/runtime/tool_diagnostics.rs` | 110 | `85228dd649a0cb3163cfb446ef38b1227934fb934c4094b9dcc81be4b613cce6` |
| `crates/vulcan-provider-adapters/src/runtime/tool_diagnostics/tests.rs` | 31 | `0dca41a7ab4e46fd88de858292672f1e3436ac5515015ad8bbf81810255a10c8` |
| `crates/vulcan-provider-adapters/src/runtime/test_support.rs` | 399 | `4f3dbfc3fdb452bc6cebf4107d9797e07c67892571e9a4ffa5937c4cd671d2e5` |
| `crates/vulcan-provider-adapters/src/runtime/session_leases.rs` | 294 | `10fd783f2fc57a9b902f7d57d8a52518bd5bbbacb6c73924a7d4b530efaa9707` |
| `crates/vulcan-provider-adapters/src/runtime/session_leases/tests.rs` | 184 | `74c74b0140602218d54ff7f5eaec660f5d514b3f1149778d6fc5f4a339a15b7d` |
| `crates/vulcan-provider-adapters/src/runtime/selection.rs` | 692 | `17a34dadb7131888d9d1f43d5e97d54925c8e8d80091da20b6ee969ddf838b8f` |
| `crates/vulcan-provider-adapters/src/runtime/selection/tests.rs` | 7 | `ad888cd7e341ef83835fdcd62b74373b044eb5cbbfbd209aea688db15b1d7b18` |
| `crates/vulcan-provider-adapters/src/runtime/selection/tests/system_bindings.rs` | 687 | `f061a606e57bce16934f5aa92cdf83018bf3f908d6b8ad6bca742054bda96101` |
| `crates/vulcan-provider-adapters/src/runtime/selection/tests/generations.rs` | 228 | `18815ef20f80a0c59acb52559fa3617296fc0443806d9e76043c3d6c22183340` |
| `crates/vulcan-provider-adapters/src/runtime/selection/tests/custom_bindings.rs` | 705 | `c28756dc370ddcd504cc4a54a0e241e86245ac4a2eaa7125a4332b04a13a17a8` |
| `crates/vulcan-provider-adapters/src/runtime/selection/tests/accounts.rs` | 484 | `e4eacced0db17009f472e93329214fcb09f50932ad730e1da8dc8a032d8ab3ae` |
| `crates/vulcan-provider-adapters/src/runtime/selection/revision.rs` | 153 | `ebedd50cf657bb2f65b593c530b2e9913b24ba516b7433c44fd92eea38368f37` |
| `crates/vulcan-provider-adapters/src/runtime/selection/revision/tests.rs` | 129 | `3349a144a2db53708b4e4bcf4d5299e5a081f5c0896372144d24297dc2b8239d` |
| `crates/vulcan-provider-adapters/src/runtime/selection/profiles.rs` | 310 | `e45f370c75c53098ed0c9653b8b8c7033e81f08550928444ff62963abf5a82b6` |
| `crates/vulcan-provider-adapters/src/runtime/selection/profiles/tests.rs` | 275 | `bd5e7a5964567bc1c66ad1d96a004df4fca23075742430ebd59ced43989c4591` |
| `crates/vulcan-provider-adapters/src/runtime/secondary_routes.rs` | 245 | `da52512fa5b1d4d99f8c53c19eedf5745f4d41c845a6266d2b6bdfc5bd60f5b6` |
| `crates/vulcan-provider-adapters/src/runtime/secondary_routes/tests.rs` | 131 | `42bf5d38d0e43cd6f32d8053b139f6f79771da00eff68db546565056392591cf` |
| `crates/vulcan-provider-adapters/src/runtime/model_overrides.rs` | 223 | `8e540040712f1d234ee870a809bac7bff5ccd47fb1767dd0ce0dad1ad8206c5e` |
| `crates/vulcan-provider-adapters/src/runtime/activation.rs` | 1224 | `8d14f6ba98b4d23cf1babb8de9771be0e46f24302bc88f11de886f5ed085825f` |
| `crates/vulcan-provider-adapters/src/runtime/catalog_sources.rs` | 426 | `6e6efd3ef190b9135209235202614c1a407b3c87cb70288e7eceb64d26d045df` |
| `crates/vulcan-provider-adapters/src/runtime/catalog_views.rs` | 1507 | `3dfad45dbf124494705a33edd35a63ba1524ca5080012f250d0dd2088c1beb52` |
| `crates/vulcan-provider-adapters/src/runtime/model_overrides/tests.rs` | 84 | `9753e31cbb288fddcde830ada75ffef76288d783cd32b62bb222ebfa982153c8` |
| `crates/vulcan-provider-adapters/src/runtime/model_execution.rs` | 663 | `02fe9c3812fd73249bd69d4cbb14860f780da97986135770072882152c18cdb4` |
| `crates/vulcan-provider-adapters/src/runtime/media.rs` | 1033 | `41f77a2d0aaa86665800ae5463c418c09f8508a4251b00824459ea9c4ae60e88` |
| `crates/vulcan-provider-adapters/src/runtime/catalog.rs` | 1130 | `23e5cb6e3f087f9a58b0e7239da650a25872b0383ac2ecd29a39c75b0bfd6804` |
| `crates/vulcan-provider-adapters/src/runtime/async_tasks/tests.rs` | 491 | `e08b587c5a1065052b0ba3e8bace3b291288550bd390308edd9bf634cd9f4133` |
| `crates/vulcan-provider-adapters/src/runtime/async_tasks.rs` | 983 | `a410742179dcdb82ad6de2429988cf5183dca2892c283798272a38a180612b50` |
| `crates/vulcan-provider-adapters/src/runtime/catalog_policy.rs` | 730 | `cb7487a66ea3b6fb06fb87453653634e64d2d9419c46b406603d5f06c60ff0d2` |
| `crates/vulcan-provider-adapters/src/runtime/credentials.rs` | 662 | `b0838d7420390461e09a7e3778e4893e82b3ea9a405ef052177f934d9d7f4827` |
| `crates/vulcan-provider-adapters/src/runtime/catalog/tests.rs` | 310 | `0bdc3e154670c04a7d4aeafb6bf6771335e3fc35872404a6cd71db1af9c5c631` |
| `crates/vulcan-provider-adapters/src/runtime/credentials/tests.rs` | 300 | `51c308579e3e5466c50c1b6d54b6bcd3b4083e15a6417da04fc3e2aefbbde9c9` |
| `crates/vulcan-provider-adapters/src/runtime/activation/tests.rs` | 885 | `fd84c16263a4cab301ddb4bb3433c58c765b2ebe1466f4a05638d5afa2b8f5e3` |
| `crates/vulcan-provider-adapters/src/runtime/media/tests.rs` | 119 | `f44480ee207b5e12dd85238b0ad26ab4d16ae1593b536bad2c09eb892dbc340a` |
| `crates/vulcan-provider-adapters/src/runtime/catalog_views/tests.rs` | 834 | `c398f11ce37c7bbcfc2ac4cabbc58e02a054edc52605640f84eaebf53e13a27c` |
| `crates/vulcan-provider-adapters/src/runtime/catalog_sources/tests.rs` | 212 | `aedfa53abe8bc37685f1f06b995fd650ae8532c1ed940cb7d7127a14edfd798a` |
| `crates/vulcan-provider-adapters/src/runtime/catalog_policy/test_support.rs` | 82 | `6c84c5be85215f49a4ece433c62e35b18a664f47b3d2506f459f8e2329ec93ec` |
| `crates/vulcan-provider-adapters/src/runtime/catalog_policy/tests.rs` | 76 | `4675ee48171be99983c6b594b21c423814290d260138d9abccb67e985a2a1444` |
| `crates/vulcan-provider-adapters/src/runtime/catalog_policy/tests/source_membership.rs` | 335 | `c7dbb448571a25d99e548e6428c808bb2c48c00e0970fc84790673518467896e` |
| `crates/vulcan-provider-adapters/src/runtime/catalog_policy/tests/model_layers.rs` | 591 | `53e7fe09ea7c58ac96602ad72b4b544d2e38d70e3cb227dc358a40edf2bffe3b` |
| `crates/vulcan-provider-adapters/src/runtime/catalog_policy/capabilities.rs` | 346 | `8122776925e2e19a4de96013d8c0f80d030bde7717b3b85a3df8dc3aada8f2ae` |
| `crates/vulcan-provider-adapters/src/runtime/catalog_policy/capabilities/tests.rs` | 110 | `0d060ad178b3246abee346efb85855a1355e941556e6c31c1867553ded2a735a` |
| `crates/vulcan-provider-adapters/src/runtime/catalog_policy/bindings.rs` | 352 | `de73621259a4d29c7803b36cc40e6b2d3b014f93192efc78d6f37644dda97846` |
| `crates/vulcan-provider-adapters/src/runtime/catalog_policy/bindings/tests.rs` | 101 | `e5c1cb09036003c6c312ae4696b1f3a99a8bc85b489a8aaedcc9172b7516d900` |
| `crates/vulcan-provider-adapters/src/runtime/catalog_policy/account_access.rs` | 153 | `6f226b7f909d29349e6113abedbb42a0e0fdce961bb70f4e95c2fac379cd1c7d` |
| `crates/vulcan-provider-adapters/src/runtime/catalog_policy/account_access/tests.rs` | 329 | `326162372416a336ec461bb8cd45106e8dcb73aeffce3a877cb5925c663e7345` |
