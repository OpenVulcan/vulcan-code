# 02：原始结构与测试基线

## 冻结状态

- 日期：2026-08-31；源码 `crates/vulcan-service-kernel/src/public_ai_settings.rs`，16244行。
- SHA-256：`64f893a762edcafb3b2abb7ab2bde2cc8d1a0fee43dfd82ba0c7e2e037c05070`，与目标开始时冻结值一致。
- 314个AST节点：266生产区域、47测试、1个测试夹具；两个顶层常量另行保全。
- 原测试不是统一tests模块：context_compaction_window_node_tests有1项，inference_settings_route_tests有46项。
- 初次使用不存在的 `public_ai_settings::tests::` 过滤器发现0项，不能当成通过；改用实际 `public_ai_settings::` 前缀后精确发现47项。
- 基线运行：`rtk proxy cargo test -p vulcan-service-kernel --lib public_ai_settings:: -- --test-threads=1`，47通过、0失败、0忽略，1.41秒。
- 当前服务包共有788项（本次47执行、741过滤）；最终应重新验证本文件和必要关联域，不能复用旧会话34项测试数量。
- Windows链接器导出库提示保留，不抑制。

## 原始测试

| 原始发现路径 | 原始行 | 属性 |
| --- | ---: | --- |
| `public_ai_settings::context_compaction_window_node_tests::changed_node_values_invalidate_locked_session_budgets_only` | 13151 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::account_membership_cannot_enable_provider_disabled_custom_model` | 14819 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::anthropic_candidate_requires_json_schema_capability` | 13551 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::anthropic_video_candidate_excludes_unregistered_audio_input` | 13516 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::bounded_fixed_headers_survive_prepare_and_registry_revalidation` | 16129 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::cancelled_provider_publication_restores_staged_before_images` | 13671 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::capability_metadata_defaults_keep_inheritance_after_rebuild` | 13261 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::compensated_projection_becomes_bounded_provider_rejection` | 13616 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::connection_contract_comparison_normalizes_every_route` | 16066 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::connection_removal_clears_exact_media_and_extension_references` | 13351 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::connection_requests_reject_user_selected_protocol` | 13876 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::custom_codex_capability_switch_is_not_a_connection_contract_change` | 15280 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::custom_codex_provider_candidate_preserves_explicit_responses_capabilities` | 15234 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::custom_model_route_status_exposes_model_specific_protocol_block` | 14312 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::custom_model_save_request_defaults_missing_template_decisions` | 15773 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::custom_provider_edit_atomically_migrates_every_existing_credential` | 15344 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::custom_provider_observation_acceptance_is_closed_and_explicit` | 15686 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::custom_provider_pending_validation_matches_non_ready_acceptance` | 15729 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::explicit_preferences_require_enabled_protocols` | 16205 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::fixed_off_reasoning_accepts_only_none` | 13577 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::internal_conversation_candidate_uses_model_protocol_binding` | 14101 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::invalid_active_account_rebinds_same_model_before_model_fallback` | 14418 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::isolated_custom_provider_removal_identity_is_namespace_safe` | 13855 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::manifest_rpc_identity_guard_rejects_unknown_and_oversized_values` | 13593 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::media_candidate_capabilities_intersect_model_and_protocol_support` | 13480 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::media_capability_backend_accepts_model_replacement_without_provider_target` | 13292 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::missing_session_models_remain_exact_until_explicit_reselection_or_revocation` | 14629 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::multi_protocol_custom_provider_projects_one_route_per_protocol` | 15953 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::multi_protocol_directory_kind_aggregates_route_discovery` | 15834 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::multi_protocol_migration_preserves_exact_internal_route_binding` | 15901 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::provider_projection_rejects_missing_activation_before_image` | 13646 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::provider_tool_selection_v1_orders_final_persisted_keys` | 14942 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::removed_protocols_reports_only_the_deleted_protocol` | 15996 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::secret_fixed_headers_are_rejected_at_service_save_boundary` | 16167 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::session_media_backend_mode_is_materialized_with_session_defaults` | 15200 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::session_media_direct_targets_are_removed_from_overridden_sessions` | 15148 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::session_media_understanding_settings_reject_missing_backend` | 15055 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::session_model_selection_waits_for_next_provider_request_and_is_idempotent` | 14524 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::session_provider_tool_settings_reconciliation_is_idempotent` | 15079 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::session_provider_tool_settings_reject_missing_selection` | 14918 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::settings_snapshot_excludes_isolated_custom_provider_connection` | 13803 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::settings_snapshot_projects_capability_overrides_without_catalogs` | 14006 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::settings_snapshot_separates_provider_tool_templates_from_conversation_access` | 14041 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::settings_test_route_rejects_operation_slot_mismatch` | 14892 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::settings_test_route_retains_exact_usage_scope` | 13979 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::system_connection_directory_uses_manifest_routes` | 15790 | `#[test]` |
| `public_ai_settings::inference_settings_route_tests::vmm_llm_route_requires_disabled_reasoning_capability` | 14857 | `#[test]` |

## 夹具

- `route_fixture`：原13959–13974行；原测试模块为inference_settings_route_tests；不能复制或丢失其精确usage_scope与operation绑定。

## 顶层常量

- `KNOWLEDGE_CURATION_PURPOSE_ID`：原64行，原pub(super)可见，值为knowledge_display_curation。
- `MAX_CUSTOM_MODEL_TEMPLATE_BATCH_SIZE`：原1417行，私有，值2000。

## 后续核对要求

每阶段逐一核对314节点、字段/序列化属性、两个常量、47条测试及1夹具；模块路径改变必须记录映射。测试含旧名称并非删除理由，以当前断言对应的真实生产合同判断；不修改断言来迁就拆分。
