# 13 号客户端测试迁移映射

旧测试前缀为 `client::tests::`；新前缀为 `client::<模块>::tests::`，表中路径分隔符 `/` 对应 Rust 模块分隔符 `::`。41 项测试完整保留，函数名、异步属性和断言均不变。

6 个 HTTP／模型共享夹具只由模型发现测试使用，统一迁入 `model_discovery/tests.rs`，不复制。测试专用的有界块适配函数仍调用真实生产块追加器和错误分类，属于有效测试桥接，保留在 HTTP 模块。

| 测试名称 | 原函数行 | 新模块 |
| --- | ---: | --- |
| `paginated_model_append_is_atomic_and_unique` | 4296 | `model_discovery` |
| `pagination_cursor_registry_rejects_non_adjacent_cycles` | 4315 | `model_discovery` |
| `pagination_boundary_rejects_unbounded_unique_cursors` | 4332 | `model_discovery` |
| `bounded_model_discovery_chunk_rejects_overflow_before_append` | 4345 | `http` |
| `discovered_catalog_bounds_reject_duplicate_and_oversized_identities` | 4359 | `model_discovery` |
| `controlled_error_summary_is_structured_and_secret_safe` | 4414 | `http` |
| `controlled_error_summary_omits_unstructured_body` | 4434 | `http` |
| `custom_anthropic_discovery_collects_multiple_pages` | 4449 | `model_discovery` |
| `custom_anthropic_discovery_uses_custom_catalog_path` | 4480 | `model_discovery` |
| `custom_anthropic_discovery_rejects_duplicate_models` | 4531 | `model_discovery` |
| `custom_anthropic_discovery_requires_cursor_for_more_pages` | 4557 | `model_discovery` |
| `custom_anthropic_discovery_rejects_cursor_cycles` | 4573 | `model_discovery` |
| `custom_openai_discovery_uses_standard_models_endpoint` | 4599 | `model_discovery` |
| `custom_openai_discovery_preserves_authentication_status` | 4642 | `model_discovery` |
| `custom_discovery_http_statuses_have_exact_typed_failures` | 4674 | `model_discovery` |
| `custom_discovery_distinguishes_invalid_json_from_empty_catalog` | 4727 | `model_discovery` |
| `custom_discovery_transport_failure_is_temporarily_unavailable` | 4790 | `model_discovery` |
| `minimax_key_prefixes_select_exact_profiles` | 4822 | `minimax_registration` |
| `deepseek_balance_preserves_multiple_currencies` | 4838 | `account_usage/deepseek` |
| `kimi_usage_preserves_membership_and_nested_windows` | 4856 | `account_usage/kimi` |
| `kimi_usage_maps_confirmed_membership_names` | 4882 | `account_usage/kimi` |
| `kimi_usage_identifies_unnamed_five_hour_window` | 4904 | `account_usage/kimi` |
| `minimax_region_requires_exactly_one_successful_probe` | 4923 | `minimax_registration` |
| `minimax_zero_allocation_bucket_is_unavailable` | 4945 | `account_usage/minimax` |
| `minimax_weekly_status_three_remains_unlimited` | 4970 | `account_usage/minimax` |
| `openrouter_credit_subtraction_is_exact` | 4995 | `account_usage/decimal` |
| `command_code_usage_normalization_is_exact` | 5010 | `account_usage/command_code` |
| `command_code_unlimited_plan_accepts_absent_request_windows` | 5072 | `account_usage/command_code` |
| `command_code_inactive_subscription_omits_plan_identity` | 5109 | `account_usage/command_code` |
| `command_code_credit_addition_is_exact` | 5149 | `account_usage/decimal` |
| `command_code_usage_url_is_fixed_and_encoded` | 5163 | `account_usage/command_code` |
| `tavily_missing_limit_is_not_zero` | 5185 | `account_usage/tavily` |
| `tavily_paygo_null_limit_is_unlimited` | 5197 | `account_usage/tavily` |
| `discovery_uses_only_active_manifest_routes` | 5208 | `model_discovery` |
| `multi_protocol_discovery_merges_shared_signature_into_per_protocol_variants` | 5236 | `model_discovery` |
| `discovery_projects_all_manifest_verified_protocol_variants` | 5342 | `model_discovery` |
| `manifest_variant_authorization_is_model_and_policy_scoped` | 5421 | `model_discovery` |
| `multi_protocol_discovery_merges_distinct_signatures_separately` | 5457 | `model_discovery` |
| `authoritative_manifest_inventory_replaces_static_catalog_membership` | 5542 | `model_discovery` |
| `manifest_static_catalog_requires_snapshot_and_service_inventory` | 5580 | `model_discovery` |
| `minimax_model_discovery_uses_generic_manifest_contracts` | 5620 | `model_discovery` |
