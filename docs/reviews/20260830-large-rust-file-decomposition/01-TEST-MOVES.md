# 01：运行时测试与夹具迁移映射

## 保全结果

- 原始发现 125 项测试，当前发现 125 项；与冻结的 `01-RUNTIME_TEST_BASELINE.md` 对照。无删除、无新增忽略、无意外新增。
- 123 项按实际被测职责迁入子模块；根模块仅保留运行时构造和共用截止预算两项。目录覆盖／来源／账号策略测试归目录策略，模型选择／绑定／代际测试归选择层，不按数量硬凑配对。
- 原始 125 个测试函数与 25 个测试／夹具节点完整保全，函数体及前置属性全部等价，断言没有弱化。共享静态存储、连接、目录、协议变体、自定义供应商、账号用量及双账号运行时夹具只向父模块开放，生产类型内部字段未因测试扩大可见性。
- 原生产区域内的 `apply_model_catalog_overlays` 仍实际调用当前生产目录投影函数，是有效的测试适配夹具，保留并迁入 `catalog_policy/test_support.rs`；其记录同时出现在生产区域迁移表中，不重复计为新增函数。
- `cargo test -p vulcan-provider-adapters --lib -- --test-threads=1`：644 项全部通过，包含此处全部 125 项。
- 01 文件逐文件终审已通过；全部测试及产物仍纳入后续全局五轮审核，本次不计为全局轮次。

## 全部测试路径

每个用例的原始完整路径均为 `runtime::tests::<用例名>`。当前文件相对于 `crates/vulcan-provider-adapters/src/`。

| 用例名 | 原始行 | 当前完整测试路径 | 当前文件及起始行 |
| --- | ---: | --- | --- |
| `media_understanding_upload_policy_is_modality_and_threshold_exact` | 12718 | `runtime::media::tests::media_understanding_upload_policy_is_modality_and_threshold_exact` | `runtime/media/tests.rs:10` |
| `media_understanding_collection_upload_binding_bypasses_scalar_cache` | 12749 | `runtime::media::tests::media_understanding_collection_upload_binding_bypasses_scalar_cache` | `runtime/media/tests.rs:41` |
| `media_upload_terminal_guard_emits_cancelled_on_drop` | 12781 | `runtime::media::tests::media_upload_terminal_guard_emits_cancelled_on_drop` | `runtime/media/tests.rs:73` |
| `provider_tool_diagnostics_redact_secret_and_preserve_route_context` | 12832 | `runtime::tool_diagnostics::tests::provider_tool_diagnostics_redact_secret_and_preserve_route_context` | `runtime/tool_diagnostics/tests.rs:10` |
| `session_selection_lease_admission_is_bounded_and_idempotent` | 12859 | `runtime::session_leases::tests::session_selection_lease_admission_is_bounded_and_idempotent` | `runtime/session_leases/tests.rs:10` |
| `session_selection_lease_absence_guard_excludes_writers` | 12884 | `runtime::session_leases::tests::session_selection_lease_absence_guard_excludes_writers` | `runtime/session_leases/tests.rs:35` |
| `session_selection_lease_reconciliation_guard_restores_on_drop` | 12917 | `runtime::session_leases::tests::session_selection_lease_reconciliation_guard_restores_on_drop` | `runtime/session_leases/tests.rs:68` |
| `session_selection_lease_reconciliation_guard_commits_removal` | 12960 | `runtime::session_leases::tests::session_selection_lease_reconciliation_guard_commits_removal` | `runtime/session_leases/tests.rs:111` |
| `session_selection_lease_reconciliation_guard_is_atomic_on_conflict` | 12998 | `runtime::session_leases::tests::session_selection_lease_reconciliation_guard_is_atomic_on_conflict` | `runtime/session_leases/tests.rs:149` |
| `provider_task_recovery_scheduler_is_concurrent_and_bounded` | 13086 | `runtime::async_tasks::tests::provider_task_recovery_scheduler_is_concurrent_and_bounded` | `runtime/async_tasks/tests.rs:62` |
| `completed_task_result_recovery_retains_transient_diagnostic` | 13133 | `runtime::async_tasks::tests::completed_task_result_recovery_retains_transient_diagnostic` | `runtime/async_tasks/tests.rs:109` |
| `expired_task_observation_error_persists_timeout_terminal_state` | 13194 | `runtime::async_tasks::tests::expired_task_observation_error_persists_timeout_terminal_state` | `runtime/async_tasks/tests.rs:170` |
| `execution_credential_lease_observes_latest_revision_after_refresh` | 13296 | `runtime::credentials::tests::execution_credential_lease_observes_latest_revision_after_refresh` | `runtime/credentials/tests.rs:48` |
| `execution_credential_lease_does_not_refresh_static_api_key` | 13464 | `runtime::credentials::tests::execution_credential_lease_does_not_refresh_static_api_key` | `runtime/credentials/tests.rs:134` |
| `isolated_candidate_secret_never_reaches_durable_store` | 13576 | `runtime::credentials::tests::isolated_candidate_secret_never_reaches_durable_store` | `runtime/credentials/tests.rs:246` |
| `staged_connection_observation_batch_is_atomic_and_cas_guarded` | 13635 | `runtime::catalog::tests::staged_connection_observation_batch_is_atomic_and_cas_guarded` | `runtime/catalog/tests.rs:10` |
| `first_batch_installs_process_rustls_provider` | 13715 | `runtime::tests::first_batch_installs_process_rustls_provider` | `runtime.rs:612` |
| `confirmed_template_builds_manual_catalog_fallback_without_discovery` | 13723 | `runtime::catalog_policy::tests::confirmed_template_builds_manual_catalog_fallback_without_discovery` | `runtime/catalog_policy/tests.rs:9` |
| `rejected_template_does_not_build_manual_catalog_fallback` | 13772 | `runtime::catalog_policy::tests::rejected_template_does_not_build_manual_catalog_fallback` | `runtime/catalog_policy/tests.rs:58` |
| `effective_catalog_revision_is_content_addressed` | 13893 | `runtime::selection::revision::tests::effective_catalog_revision_is_content_addressed` | `runtime/selection/revision/tests.rs:10` |
| `manual_model_discovery_signature_excludes_observation_generation` | 13930 | `runtime::catalog_policy::bindings::tests::manual_model_discovery_signature_excludes_observation_generation` | `runtime/catalog_policy/bindings/tests.rs:11` |
| `isolated_catalog_candidate_publishes_as_one_runtime_image` | 13998 | `runtime::activation::tests::isolated_catalog_candidate_publishes_as_one_runtime_image` | `runtime/activation/tests.rs:16` |
| `activation_compensation_is_complete_and_compare_and_swap_guarded` | 14048 | `runtime::activation::tests::activation_compensation_is_complete_and_compare_and_swap_guarded` | `runtime/activation/tests.rs:66` |
| `transient_usage_failure_preserves_account_access_evidence` | 14103 | `runtime::usage::tests::transient_usage_failure_preserves_account_access_evidence` | `runtime/usage/tests.rs:10` |
| `catalog_failure_mapping_preserves_every_typed_category` | 14148 | `runtime::catalog::tests::catalog_failure_mapping_preserves_every_typed_category` | `runtime/catalog/tests.rs:90` |
| `unknown_text_model_reasoning_is_enabled_by_default` | 14222 | `runtime::catalog_policy::capabilities::tests::unknown_text_model_reasoning_is_enabled_by_default` | `runtime/catalog_policy/capabilities/tests.rs:11` |
| `explicit_fixed_off_reasoning_remains_disabled` | 14247 | `runtime::catalog_policy::capabilities::tests::explicit_fixed_off_reasoning_remains_disabled` | `runtime/catalog_policy/capabilities/tests.rs:36` |
| `explicit_fixed_off_without_efforts_remains_disabled` | 14264 | `runtime::catalog_policy::capabilities::tests::explicit_fixed_off_without_efforts_remains_disabled` | `runtime/catalog_policy/capabilities/tests.rs:53` |
| `selection_uses_persisted_reasoning_override_when_runtime_catalog_is_stale` | 14278 | `runtime::selection::tests::accounts::selection_uses_persisted_reasoning_override_when_runtime_catalog_is_stale` | `runtime/selection/tests/accounts.rs:23` |
| `model_overlay_rebuild_preserves_source_catalog` | 14364 | `runtime::catalog_views::tests::model_overlay_rebuild_preserves_source_catalog` | `runtime/catalog_views/tests.rs:21` |
| `connection_model_access_isolated_between_accounts` | 14425 | `runtime::catalog_views::tests::connection_model_access_isolated_between_accounts` | `runtime/catalog_views/tests.rs:82` |
| `disabled_connection_default_hides_future_discovery` | 14472 | `runtime::catalog_views::tests::disabled_connection_default_hides_future_discovery` | `runtime/catalog_views/tests.rs:129` |
| `capability_override_candidate_validates_final_resolved_limits` | 14520 | `runtime::catalog_views::tests::capability_override_candidate_validates_final_resolved_limits` | `runtime/catalog_views/tests.rs:177` |
| `provider_model_management_uses_support_catalog_when_account_is_disabled` | 14588 | `runtime::catalog_views::tests::provider_model_management_uses_support_catalog_when_account_is_disabled` | `runtime/catalog_views/tests.rs:245` |
| `credential_free_manual_model_accepts_direct_capability_settings` | 14640 | `runtime::catalog_views::tests::credential_free_manual_model_accepts_direct_capability_settings` | `runtime/catalog_views/tests.rs:297` |
| `capability_override_candidate_validates_every_account` | 14686 | `runtime::catalog_views::tests::capability_override_candidate_validates_every_account` | `runtime/catalog_views/tests.rs:343` |
| `model_overlay_rebuild_rejects_invalid_final_limits` | 14726 | `runtime::catalog_views::tests::model_overlay_rebuild_rejects_invalid_final_limits` | `runtime/catalog_views/tests.rs:383` |
| `manual_custom_model_builds_catalog_without_provider_discovery` | 14757 | `runtime::catalog::tests::manual_custom_model_builds_catalog_without_provider_discovery` | `runtime/catalog/tests.rs:131` |
| `activated_custom_catalog_publishes_discovery_and_manual_union` | 14819 | `runtime::catalog::tests::activated_custom_catalog_publishes_discovery_and_manual_union` | `runtime/catalog/tests.rs:193` |
| `connection_catalog_membership_tracks_manual_definition_removal` | 14908 | `runtime::catalog_views::tests::connection_catalog_membership_tracks_manual_definition_removal` | `runtime/catalog_views/tests.rs:414` |
| `connection_catalog_membership_retains_discovered_model_after_overlay_removal` | 14960 | `runtime::catalog_views::tests::connection_catalog_membership_retains_discovered_model_after_overlay_removal` | `runtime/catalog_views/tests.rs:466` |
| `custom_catalog_uses_two_hundred_thousand_and_thirty_two_thousand_defaults` | 15013 | `runtime::catalog_policy::tests::model_layers::custom_catalog_uses_two_hundred_thousand_and_thirty_two_thousand_defaults` | `runtime/catalog_policy/tests/model_layers.rs:12` |
| `unknown_compatible_conversation_defaults_to_enabled_reasoning` | 15059 | `runtime::catalog_policy::capabilities::tests::unknown_compatible_conversation_defaults_to_enabled_reasoning` | `runtime/catalog_policy/capabilities/tests.rs:82` |
| `native_only_conversation_declaration_defaults_to_enabled_reasoning` | 15092 | `runtime::catalog_policy::tests::model_layers::native_only_conversation_declaration_defaults_to_enabled_reasoning` | `runtime/catalog_policy/tests/model_layers.rs:58` |
| `explicit_model_reasoning_disablement_outranks_host_default` | 15124 | `runtime::catalog_policy::tests::model_layers::explicit_model_reasoning_disablement_outranks_host_default` | `runtime/catalog_policy/tests/model_layers.rs:90` |
| `explicit_model_reasoning_enablement_outranks_disabled_discovery` | 15160 | `runtime::catalog_policy::tests::model_layers::explicit_model_reasoning_enablement_outranks_disabled_discovery` | `runtime/catalog_policy/tests/model_layers.rs:126` |
| `provider_settings_outrank_catalog_values_within_text_contract` | 15205 | `runtime::catalog_policy::tests::model_layers::provider_settings_outrank_catalog_values_within_text_contract` | `runtime/catalog_policy/tests/model_layers.rs:171` |
| `custom_provider_filters_explicit_non_text_output_models` | 15259 | `runtime::catalog_policy::tests::model_layers::custom_provider_filters_explicit_non_text_output_models` | `runtime/catalog_policy/tests/model_layers.rs:225` |
| `custom_provider_system_baseline_enables_reasoning` | 15284 | `runtime::catalog_policy::tests::model_layers::custom_provider_system_baseline_enables_reasoning` | `runtime/catalog_policy/tests/model_layers.rs:250` |
| `custom_gemini_system_baseline_preserves_protocol_reasoning_evidence` | 15333 | `runtime::catalog_policy::capabilities::tests::custom_gemini_system_baseline_preserves_protocol_reasoning_evidence` | `runtime/catalog_policy/capabilities/tests.rs:67` |
| `explicit_provider_settings_can_disable_system_reasoning` | 15348 | `runtime::catalog_policy::tests::model_layers::explicit_provider_settings_can_disable_system_reasoning` | `runtime/catalog_policy/tests/model_layers.rs:299` |
| `direct_model_settings_outrank_provider_reasoning_settings` | 15391 | `runtime::catalog_policy::tests::model_layers::direct_model_settings_outrank_provider_reasoning_settings` | `runtime/catalog_policy/tests/model_layers.rs:342` |
| `custom_model_user_override_outranks_credential_evidence` | 15471 | `runtime::catalog_policy::tests::model_layers::custom_model_user_override_outranks_credential_evidence` | `runtime/catalog_policy/tests/model_layers.rs:422` |
| `discovered_custom_model_applies_only_manual_advanced_declarations` | 15506 | `runtime::catalog_policy::tests::model_layers::discovered_custom_model_applies_only_manual_advanced_declarations` | `runtime/catalog_policy/tests/model_layers.rs:457` |
| `provider_capability_defaults_are_overridden_by_model_capabilities` | 15591 | `runtime::catalog_policy::tests::model_layers::provider_capability_defaults_are_overridden_by_model_capabilities` | `runtime/catalog_policy/tests/model_layers.rs:541` |
| `custom_discovery_catalog_unions_absent_manual_model_unless_explicitly_disabled` | 15646 | `runtime::catalog_policy::tests::source_membership::custom_discovery_catalog_unions_absent_manual_model_unless_explicitly_disabled` | `runtime/catalog_policy/tests/source_membership.rs:18` |
| `empty_custom_discovery_catalog_retains_manual_model_unless_explicitly_disabled` | 15703 | `runtime::catalog_policy::tests::source_membership::empty_custom_discovery_catalog_retains_manual_model_unless_explicitly_disabled` | `runtime/catalog_policy/tests/source_membership.rs:75` |
| `provider_task_network_operation_has_a_controlled_timeout` | 15745 | `runtime::async_tasks::tests::provider_task_network_operation_has_a_controlled_timeout` | `runtime/async_tasks/tests.rs:236` |
| `provider_operation_remaining_budget_fails_after_deadline` | 15761 | `runtime::tests::provider_operation_remaining_budget_fails_after_deadline` | `runtime.rs:620` |
| `model_selection_prefers_remembered_account_over_provider_default` | 15776 | `runtime::selection::tests::accounts::model_selection_prefers_remembered_account_over_provider_default` | `runtime/selection/tests/accounts.rs:108` |
| `model_selection_uses_first_stable_account_without_preference` | 15799 | `runtime::selection::tests::accounts::model_selection_uses_first_stable_account_without_preference` | `runtime/selection/tests/accounts.rs:131` |
| `pending_validation_connection_is_isolated_from_normal_routing` | 15818 | `runtime::selection::tests::accounts::pending_validation_connection_is_isolated_from_normal_routing` | `runtime/selection/tests/accounts.rs:150` |
| `workbench_models_require_bidirectional_text_conversation` | 15870 | `runtime::selection::tests::accounts::workbench_models_require_bidirectional_text_conversation` | `runtime/selection/tests/accounts.rs:202` |
| `model_selection_degrades_after_remembered_account_is_removed` | 15948 | `runtime::selection::tests::accounts::model_selection_degrades_after_remembered_account_is_removed` | `runtime/selection/tests/accounts.rs:280` |
| `connection_model_selection_uses_exact_session_account` | 15968 | `runtime::selection::tests::accounts::connection_model_selection_uses_exact_session_account` | `runtime/selection/tests/accounts.rs:300` |
| `route_revision_tracks_only_exact_execution_semantics` | 16000 | `runtime::selection::revision::tests::route_revision_tracks_only_exact_execution_semantics` | `runtime/selection/revision/tests.rs:47` |
| `secondary_route_cache_honors_scope_ttl_and_forced_refresh` | 16088 | `runtime::secondary_routes::tests::secondary_route_cache_honors_scope_ttl_and_forced_refresh` | `runtime/secondary_routes/tests.rs:10` |
| `secondary_route_cache_evicts_the_oldest_partition_at_capacity` | 16173 | `runtime::secondary_routes::tests::secondary_route_cache_evicts_the_oldest_partition_at_capacity` | `runtime/secondary_routes/tests.rs:95` |
| `active_selection_uses_exact_configured_connection` | 16215 | `runtime::selection::tests::accounts::active_selection_uses_exact_configured_connection` | `runtime/selection/tests/accounts.rs:332` |
| `model_overrides_are_shared_across_matching_accounts` | 16234 | `runtime::selection::tests::accounts::model_overrides_are_shared_across_matching_accounts` | `runtime/selection/tests/accounts.rs:351` |
| `undeclared_model_limits_accept_explicit_overrides` | 16268 | `runtime::model_overrides::tests::undeclared_model_limits_accept_explicit_overrides` | `runtime/model_overrides/tests.rs:15` |
| `declared_model_limits_reject_expansive_overrides` | 16289 | `runtime::model_overrides::tests::declared_model_limits_reject_expansive_overrides` | `runtime/model_overrides/tests.rs:36` |
| `explicit_model_parameter_defaults_are_applied_before_validation` | 16308 | `runtime::model_overrides::tests::explicit_model_parameter_defaults_are_applied_before_validation` | `runtime/model_overrides/tests.rs:55` |
| `runtime_model_is_enabled_follows_manifest_first_precedence` | 16343 | `runtime::catalog_views::tests::runtime_model_is_enabled_follows_manifest_first_precedence` | `runtime/catalog_views/tests.rs:520` |
| `model_level_protocol_routing_uses_declared_bindings_per_model` | 16387 | `runtime::selection::tests::system_bindings::model_level_protocol_routing_uses_declared_bindings_per_model` | `runtime/selection/tests/system_bindings.rs:21` |
| `apikeyfun_manifest_uses_model_protocol_binding` | 16507 | `runtime::selection::tests::system_bindings::apikeyfun_manifest_uses_model_protocol_binding` | `runtime/selection/tests/system_bindings.rs:141` |
| `apikeyfun_manifest_authorizes_discovery_before_credential_access` | 16561 | `runtime::catalog::tests::apikeyfun_manifest_authorizes_discovery_before_credential_access` | `runtime/catalog/tests.rs:282` |
| `apikeyfun_rejects_discovered_model_outside_declared_whitelist` | 16594 | `runtime::selection::tests::system_bindings::apikeyfun_rejects_discovered_model_outside_declared_whitelist` | `runtime/selection/tests/system_bindings.rs:195` |
| `undeclared_model_on_declared_rule_required_provider_is_not_executable` | 16646 | `runtime::selection::tests::system_bindings::undeclared_model_on_declared_rule_required_provider_is_not_executable` | `runtime/selection/tests/system_bindings.rs:247` |
| `mixed_custom_protocol_discovery_aggregates_every_verified_route` | 16793 | `runtime::catalog_sources::tests::mixed_custom_protocol_discovery_aggregates_every_verified_route` | `runtime/catalog_sources/tests.rs:12` |
| `three_custom_providers_stay_isolated_from_each_other` | 16849 | `runtime::selection::tests::accounts::three_custom_providers_stay_isolated_from_each_other` | `runtime/selection/tests/accounts.rs:385` |
| `isolated_custom_provider_connection_is_excluded_from_atomic_activation` | 16953 | `runtime::activation::tests::isolated_custom_provider_connection_is_excluded_from_atomic_activation` | `runtime/activation/tests.rs:740` |
| `custom_manifest_descriptor_match_excludes_only_manifest_owned_profiles` | 17042 | `runtime::activation::tests::custom_manifest_descriptor_match_excludes_only_manifest_owned_profiles` | `runtime/activation/tests.rs:829` |
| `custom_manifest_tool_targets_publish_as_one_frozen_generation` | 17078 | `runtime::tool_targets::tests::custom_manifest_tool_targets_publish_as_one_frozen_generation` | `runtime/tool_targets/tests.rs:16` |
| `custom_multi_protocol_auto_resolves_by_fixed_priority` | 17247 | `runtime::selection::tests::custom_bindings::custom_multi_protocol_auto_resolves_by_fixed_priority` | `runtime/selection/tests/custom_bindings.rs:20` |
| `custom_multi_protocol_auto_uses_exact_model_available_protocols` | 17343 | `runtime::selection::tests::custom_bindings::custom_multi_protocol_auto_uses_exact_model_available_protocols` | `runtime/selection/tests/custom_bindings.rs:116` |
| `custom_auto_binding_rebinds_when_protocol_priority_changes` | 17415 | `runtime::catalog_policy::bindings::tests::custom_auto_binding_rebinds_when_protocol_priority_changes` | `runtime/catalog_policy/bindings/tests.rs:59` |
| `custom_protocol_variant_policy_follows_effective_binding` | 17462 | `runtime::selection::tests::custom_bindings::custom_protocol_variant_policy_follows_effective_binding` | `runtime/selection/tests/custom_bindings.rs:188` |
| `frozen_custom_binding_only_rebinds_at_activation_boundary` | 17545 | `runtime::selection::tests::custom_bindings::frozen_custom_binding_only_rebinds_at_activation_boundary` | `runtime/selection/tests/custom_bindings.rs:271` |
| `custom_explicit_preference_survives_and_requires_reselection_after_protocol_removal` | 17779 | `runtime::selection::tests::custom_bindings::custom_explicit_preference_survives_and_requires_reselection_after_protocol_removal` | `runtime/selection/tests/custom_bindings.rs:502` |
| `deepseek_manifest_identity_supplies_current_protocol_variants` | 17920 | `runtime::selection::tests::system_bindings::deepseek_manifest_identity_supplies_current_protocol_variants` | `runtime/selection/tests/system_bindings.rs:306` |
| `hot_replaced_manifest_drives_new_bindings` | 18123 | `runtime::selection::tests::system_bindings::hot_replaced_manifest_drives_new_bindings` | `runtime/selection/tests/system_bindings.rs:509` |
| `atomic_manifest_activation_skips_provider_tool_connections` | 18232 | `runtime::activation::tests::atomic_manifest_activation_skips_provider_tool_connections` | `runtime/activation/tests.rs:548` |
| `staged_manifest_catalogs_publish_only_through_activation_batch` | 18267 | `runtime::activation::tests::staged_manifest_catalogs_publish_only_through_activation_batch` | `runtime/activation/tests.rs:583` |
| `manifest_account_plan_allowlist_filters_catalog_models` | 18414 | `runtime::catalog_policy::account_access::tests::manifest_account_plan_allowlist_filters_catalog_models` | `runtime/catalog_policy/account_access/tests.rs:32` |
| `manifest_account_model_access_projects_commandcode_catalog` | 18499 | `runtime::catalog_policy::account_access::tests::manifest_account_model_access_projects_commandcode_catalog` | `runtime/catalog_policy/account_access/tests.rs:117` |
| `normalized_usage_decimal_positive_check_is_strict` | 18662 | `runtime::catalog_policy::account_access::tests::normalized_usage_decimal_positive_check_is_strict` | `runtime/catalog_policy/account_access/tests.rs:9` |
| `apikeyfun_catalog_does_not_inherit_account_plan_filtering` | 18674 | `runtime::catalog_policy::account_access::tests::apikeyfun_catalog_does_not_inherit_account_plan_filtering` | `runtime/catalog_policy/account_access/tests.rs:280` |
| `provider_tool_template_models_are_independent_from_conversation_access` | 18728 | `runtime::catalog_views::tests::provider_tool_template_models_are_independent_from_conversation_access` | `runtime/catalog_views/tests.rs:719` |
| `provider_tool_targets_obey_account_model_access` | 18846 | `runtime::tool_targets::tests::provider_tool_targets_obey_account_model_access` | `runtime/tool_targets/tests.rs:185` |
| `manifest_owned_tool_targets_drive_availability_and_resolution` | 18935 | `runtime::tool_targets::tests::manifest_owned_tool_targets_drive_availability_and_resolution` | `runtime/tool_targets/tests.rs:274` |
| `recovered_async_task_uses_frozen_target_snapshot` | 19078 | `runtime::async_tasks::tests::recovered_async_task_uses_frozen_target_snapshot` | `runtime/async_tasks/tests.rs:254` |
| `unactivated_configuration_mutation_is_invisible_to_selection` | 19300 | `runtime::selection::tests::generations::unactivated_configuration_mutation_is_invisible_to_selection` | `runtime/selection/tests/generations.rs:27` |
| `missing_custom_catalog_binding_never_resolves_live_at_request_time` | 19388 | `runtime::selection::tests::custom_bindings::missing_custom_catalog_binding_never_resolves_live_at_request_time` | `runtime/selection/tests/custom_bindings.rs:648` |
| `system_manifest_binding_without_variant_is_model_level_blocked` | 19457 | `runtime::selection::tests::system_bindings::system_manifest_binding_without_variant_is_model_level_blocked` | `runtime/selection/tests/system_bindings.rs:609` |
| `hot_replaced_catalog_swaps_whole_generation_with_content_hash` | 19552 | `runtime::activation::tests::hot_replaced_catalog_swaps_whole_generation_with_content_hash` | `runtime/activation/tests.rs:479` |
| `runtime_settings_projection_isolates_one_failed_system_provider` | 19622 | `runtime::catalog_views::tests::runtime_settings_projection_isolates_one_failed_system_provider` | `runtime/catalog_views/tests.rs:563` |
| `runtime_settings_projection_isolates_invalid_apikeyfun_manifest` | 19707 | `runtime::catalog_views::tests::runtime_settings_projection_isolates_invalid_apikeyfun_manifest` | `runtime/catalog_views/tests.rs:647` |
| `catalog_build_is_deterministic_for_identical_content` | 19780 | `runtime::activation::tests::catalog_build_is_deterministic_for_identical_content` | `runtime/activation/tests.rs:865` |
| `identical_system_manifest_replacement_is_a_generation_noop` | 19805 | `runtime::activation::tests::identical_system_manifest_replacement_is_a_generation_noop` | `runtime/activation/tests.rs:121` |
| `concurrent_configuration_activation_never_mixes_catalog_generations` | 19834 | `runtime::activation::tests::concurrent_configuration_activation_never_mixes_catalog_generations` | `runtime/activation/tests.rs:315` |
| `concurrent_selection_observes_whole_generations` | 19929 | `runtime::selection::tests::generations::concurrent_selection_observes_whole_generations` | `runtime/selection/tests/generations.rs:116` |
| `concurrent_distinct_provider_replacements_preserve_both_updates` | 20046 | `runtime::activation::tests::concurrent_distinct_provider_replacements_preserve_both_updates` | `runtime/activation/tests.rs:404` |
| `provider_manifest_generation_build_failure_preserves_current_catalog` | 20115 | `runtime::activation::tests::provider_manifest_generation_build_failure_preserves_current_catalog` | `runtime/activation/tests.rs:150` |
| `provider_manifest_generation_publish_conflict_preserves_newer_update` | 20136 | `runtime::activation::tests::provider_manifest_generation_publish_conflict_preserves_newer_update` | `runtime/activation/tests.rs:171` |
| `staged_manifest_compensation_restores_exact_before_image` | 20187 | `runtime::activation::tests::staged_manifest_compensation_restores_exact_before_image` | `runtime/activation/tests.rs:222` |
| `staged_manifest_compensation_rejects_newer_mutation` | 20232 | `runtime::activation::tests::staged_manifest_compensation_rejects_newer_mutation` | `runtime/activation/tests.rs:266` |
| `provider_task_timeout_uses_frozen_operation_deadline` | 20284 | `runtime::async_tasks::tests::provider_task_timeout_uses_frozen_operation_deadline` | `runtime/async_tasks/tests.rs:467` |
| `provider_task_observation_delay_never_crosses_deadline` | 20295 | `runtime::async_tasks::tests::provider_task_observation_delay_never_crosses_deadline` | `runtime/async_tasks/tests.rs:478` |
| `bundled_resource_constraints_resolve_without_provider_hardcoding` | 20313 | `runtime::selection::profiles::tests::bundled_resource_constraints_resolve_without_provider_hardcoding` | `runtime/selection/profiles/tests.rs:13` |
| `model_scoped_profiles_freeze_into_the_execution_route` | 20359 | `runtime::selection::profiles::tests::model_scoped_profiles_freeze_into_the_execution_route` | `runtime/selection/profiles/tests.rs:59` |
| `explicit_empty_model_profiles_clear_route_special_cases` | 20473 | `runtime::selection::profiles::tests::explicit_empty_model_profiles_clear_route_special_cases` | `runtime/selection/profiles/tests.rs:173` |
| `model_source_policies_control_runtime_identity_union` | 20580 | `runtime::catalog_policy::tests::source_membership::model_source_policies_control_runtime_identity_union` | `runtime/catalog_policy/tests/source_membership.rs:117` |
| `manifest_only_refresh_performs_no_credential_or_remote_discovery` | 20806 | `runtime::catalog_sources::tests::manifest_only_refresh_performs_no_credential_or_remote_discovery` | `runtime/catalog_sources/tests.rs:68` |
| `custom_manual_only_refresh_performs_no_credential_or_remote_discovery` | 20868 | `runtime::catalog_sources::tests::custom_manual_only_refresh_performs_no_credential_or_remote_discovery` | `runtime/catalog_sources/tests.rs:130` |

## 全部夹具／辅助节点

表中包含夹具类型、函数及特征实现成员，均与原始词法序列等价。

| 节点 | 原始行 | 当前文件 | 当前行 |
| --- | --- | --- | --- |
| `recovery_scheduler_record` | 13048～13081 | `runtime/async_tasks/tests.rs` | 24～57 |
| `test_kimi_oauth_secret` | 13271～13291 | `runtime/credentials/tests.rs` | 23～43 |
| `struct:TestStaticSecretStore` | 13382～13386 | `runtime/test_support.rs` | 19～23 |
| `TestStaticSecretStore::api_key` | 13396～13406 | `runtime/test_support.rs` | 33～43 |
| `CredentialSecretResolver for TestStaticSecretStore::read_optional_secret` | 13413～13418 | `runtime/test_support.rs` | 50～55 |
| `CredentialSecretStore for TestStaticSecretStore::create_secret` | 13425～13431 | `runtime/test_support.rs` | 62～68 |
| `CredentialSecretStore for TestStaticSecretStore::replace_secret` | 13435～13442 | `runtime/test_support.rs` | 72～79 |
| `CredentialSecretStore for TestStaticSecretStore::delete_secret` | 13446～13448 | `runtime/test_support.rs` | 83～85 |
| `CredentialSecretStore for TestStaticSecretStore::restore_secret` | 13452～13458 | `runtime/test_support.rs` | 89～95 |
| `struct:TestCountingSecretStore` | 13494～13510 | `runtime/credentials/tests.rs` | 164～180 |
| `CredentialSecretResolver for TestCountingSecretStore::read_optional_secret` | 13516～13522 | `runtime/credentials/tests.rs` | 186～192 |
| `CredentialSecretStore for TestCountingSecretStore::create_secret` | 13529～13536 | `runtime/credentials/tests.rs` | 199～206 |
| `CredentialSecretStore for TestCountingSecretStore::replace_secret` | 13540～13550 | `runtime/credentials/tests.rs` | 210～220 |
| `CredentialSecretStore for TestCountingSecretStore::delete_secret` | 13554～13557 | `runtime/credentials/tests.rs` | 224～227 |
| `CredentialSecretStore for TestCountingSecretStore::restore_secret` | 13561～13570 | `runtime/credentials/tests.rs` | 231～240 |
| `connection` | 13791～13805 | `runtime/test_support.rs` | 110～124 |
| `account_usage` | 13821～13840 | `runtime/test_support.rs` | 352～371 |
| `with_protocol_variants` | 13854～13872 | `runtime/test_support.rs` | 140～158 |
| `catalog` | 13874～13888 | `runtime/test_support.rs` | 172～186 |
| `install_source_catalog` | 13987～13993 | `runtime/test_support.rs` | 245～251 |
| `two_account_runtime` | 14181～14217 | `runtime/test_support.rs` | 195～231 |
| `custom_connection` | 16704～16718 | `runtime/test_support.rs` | 265～279 |
| `multi_protocol_custom_definition` | 16722～16731 | `runtime/test_support.rs` | 293～302 |
| `install_current_custom_provider` | 16740～16754 | `runtime/test_support.rs` | 385～399 |
| `custom_definition_with_discovery_mode` | 16770～16788 | `runtime/test_support.rs` | 318～336 |
