# 03：基础执行层测试与结构基线

## 冻结信息

- 核验时间：2026-08-31；原始文件 14353 行，SHA-256：`6165dc24fbdb253f282503f346cec7860c69b4ed21fe9a30233860e133d088d1`。
- 103 个本文件测试、11 个辅助节点、250 个生产区域节点；18 个顶层常量另行核对。
- 实测 119 通过（本文件 103，额外工具执行 16），0 失败、0 忽略。共享资源使用单线程。
- 测试名包含“旧等价”只表示比较契约，不能据此删除；有效性以当前生产调用和断言内容为准。
- 以下行号为冻结前源码定位，不随迁移覆盖。

## 原始测试清单

| 测试 | 原始行 | 属性 |
| --- | --- | --- |
| `unfinished_wire_response_capture_records_cancelled_stage` | 9388–9405 | `#[tokio::test]` |
| `every_builtin_provider_protocol_routes_through_responses_core` | 9410–9435 | `#[test]` |
| `bundled_media_upload_routes_are_manifest_driven` | 9509–9609 | `#[test]` |
| `prepared_media_key_isolates_frozen_route_identity` | 9614–9667 | `#[test]` |
| `prepared_media_key_honors_explicit_upload_reference_scopes` | 9672–9745 | `#[test]` |
| `prepared_media_cache_pruning_is_bounded` | 9750–9799 | `#[test]` |
| `prepared_media_cache_keeps_durable_conversation_reference` | 9804–9825 | `#[test]` |
| `provider_tool_reference_reuse_uses_first_use_and_provider_expiry` | 9830–9857 | `#[test]` |
| `prepared_media_fingerprint_detects_equal_length_content_replacement` | 9862–9888 | `#[tokio::test]` |
| `prepared_inline_media_read_binds_retained_bytes_to_fingerprint` | 9893–9924 | `#[tokio::test]` |
| `exact_provider_routes_install_specialized_usage_profiles` | 9929–10000 | `#[test]` |
| `profiled_usage_gate_releases_usage_after_response_start` | 10005–10028 | `#[test]` |
| `profiled_usage_gate_rejects_missing_response_start` | 10033–10044 | `#[test]` |
| `profiled_usage_gate_rejects_unbounded_pre_start_events` | 10049–10077 | `#[test]` |
| `error_json_body_profile_extracts_billable_usage` | 10082–10118 | `#[test]` |
| `unregistered_error_usage_kind_does_not_parse_plain_text` | 10123–10147 | `#[test]` |
| `usage_output_marks_subsequent_transport_failure_retry_unsafe` | 10152–10163 | `#[test]` |
| `stream_idle_timeout_remains_retryable_after_semantic_output` | 10168–10182 | `#[test]` |
| `http_context_length_code_is_typed` | 10187–10202 | `#[test]` |
| `execution_failure_boundary_redacts_exact_secret_and_provider_urls` | 10207–10249 | `#[test]` |
| `provider_request_id_uses_declared_order` | 10254–10276 | `#[test]` |
| `provider_diagnostic_header_values_are_bounded` | 10281–10325 | `#[test]` |
| `http_send_failure_uses_reqwest_connection_evidence` | 10330–10380 | `#[tokio::test]` |
| `http_stream_failure_retains_http_transport` | 10385–10390 | `#[test]` |
| `websocket_protocol_failure_retains_websocket_transport` | 10395–10406 | `#[test]` |
| `empty_http_error_response_reports_status` | 10411–10419 | `#[test]` |
| `http_server_overloaded_code_is_retryable` | 10424–10439 | `#[test]` |
| `http_server_overloaded_plain_text_is_retryable` | 10444–10457 | `#[test]` |
| `compact_shape_trace_labels_are_bounded_and_sanitized` | 10462–10472 | `#[test]` |
| `apply_patch_standard_function_survives_production_protocol_projection` | 10509–10664 | `#[test]` |
| `protocol_roles_preserve_instruction_and_tool_identity_across_switches` | 10669–10860 | `#[test]` |
| `protocol_roles_reject_missing_conversion_before_dispatch` | 10865–10897 | `#[test]` |
| `gemini_generate_content_route_uses_native_model_path_and_controls` | 10902–10936 | `#[test]` |
| `apikeyfun_gemini_route_separates_execution_and_discovery_origins` | 10941–10968 | `#[test]` |
| `responses_websocket_cache_key_is_bounded_and_endpoint_bound` | 10973–10991 | `#[test]` |
| `responses_websocket_breakers_are_bounded_and_lru` | 10996–11021 | `#[test]` |
| `gemini_interactions_route_uses_responses_baseline_and_preserves_replay` | 11026–11074 | `#[test]` |
| `http_wire_request_capture_matches_dispatched_api_body` | 11145–11307 | `#[tokio::test]` |
| `http_trace_persistence_failure_does_not_prevent_network_dispatch` | 11312–11389 | `#[tokio::test]` |
| `http_wire_response_capture_classifies_dispatch_failure` | 11394–11459 | `#[tokio::test]` |
| `http_wire_response_capture_survives_protocol_failure` | 11464–11586 | `#[tokio::test]` |
| `http_wire_response_capture_survives_tool_argument_abort` | 11591–11725 | `#[tokio::test]` |
| `nonpersistent_responses_continuation_is_scoped_to_the_active_websocket` | 11730–11766 | `#[tokio::test]` |
| `websocket_retry_controls_preserve_code_and_semantic_safety` | 11771–11805 | `#[test]` |
| `websocket_1013_failure_falls_back_to_full_history_http_attempt` | 11810–11909 | `#[tokio::test]` |
| `websocket_previous_response_not_found_retries_without_stale_cursor` | 11914–12053 | `#[tokio::test]` |
| `codex_compact_body_removes_stream_only_and_unknown_fields` | 12058–12088 | `#[test]` |
| `codex_compact_body_preserves_nonempty_base_instructions` | 12093–12108 | `#[test]` |
| `codex_compact_response_requires_a_compaction_item` | 12113–12176 | `#[test]` |
| `codex_protocol_projection_preserves_socket_proven_previous_response_id` | 12181–12210 | `#[test]` |
| `non_codex_routes_replay_history_without_projecting_codex_continuation` | 12215–12244 | `#[test]` |
| `responses_compact_transport_failure_is_dispatched_and_retryable` | 12249–12257 | `#[test]` |
| `responses_http_completion_failure_preserves_retry_safety` | 12262–12294 | `#[test]` |
| `alibaba_workspace_origin_is_protocol_scoped` | 12379–12390 | `#[test]` |
| `sse_parser_handles_arbitrary_chunks` | 12395–12415 | `#[test]` |
| `sse_parser_accepts_cr_only_and_mixed_line_endings` | 12420–12441 | `#[test]` |
| `bounded_parser_supports_json_lines_ndjson_and_single_json` | 12446–12487 | `#[test]` |
| `bounded_parser_rejects_oversized_frames_and_buffers` | 12492–12510 | `#[test]` |
| `response_content_type_is_strict_and_parameter_tolerant` | 12515–12537 | `#[test]` |
| `retry_after_numeric_values_preserve_milliseconds` | 12542–12549 | `#[test]` |
| `retry_after_http_date_preserves_source` | 12554–12567 | `#[test]` |
| `retry_after_malformed_value_is_rejected` | 12572–12579 | `#[test]` |
| `encode_request_projects_pinned_openrouter_route_for_every_protocol` | 12584–12603 | `#[test]` |
| `encode_request_preserves_structured_output_schema_for_supported_protocols` | 12608–12647 | `#[test]` |
| `encode_request_rejects_structured_schema_loss_after_custom_projection` | 12652–12679 | `#[test]` |
| `kimi_coding_projection_uses_thinking_and_device_identity` | 12684–12728 | `#[test]` |
| `minimax_openai_chat_projects_adaptive_thinking` | 12733–12754 | `#[test]` |
| `minimax_anthropic_projects_disabled_thinking` | 12759–12773 | `#[test]` |
| `minimax_anthropic_preserves_structured_output_format` | 12778–12811 | `#[test]` |
| `minimax_rejects_undocumented_reasoning_effort` | 12816–12834 | `#[test]` |
| `deepseek_openai_chat_projects_disabled_thinking` | 12839–12858 | `#[test]` |
| `deepseek_thinking_omits_ineffective_temperature` | 12863–12881 | `#[test]` |
| `deepseek_anthropic_projects_disabled_and_enabled_thinking` | 12886–12914 | `#[test]` |
| `commandcode_projects_native_chat_and_anthropic_reasoning` | 12919–12952 | `#[test]` |
| `deepseek_anthropic_preserves_native_web_search` | 12957–12972 | `#[test]` |
| `deepseek_responses_projects_native_web_search` | 12977–13005 | `#[test]` |
| `encode_request_normalizes_openrouter_reasoning_for_every_protocol` | 13010–13033 | `#[test]` |
| `encode_request_projects_openrouter_adaptive_reasoning_as_enabled` | 13038–13057 | `#[test]` |
| `encode_request_rejects_conflicting_openrouter_reasoning_controls` | 13062–13082 | `#[test]` |
| `encode_request_rejects_openrouter_explicit_prompt_cache_from_manifest` | 13087–13107 | `#[test]` |
| `execution_headers_merge_protocol_headers_and_reject_credential_overrides` | 13112–13175 | `#[test]` |
| `native_codex_protocol_owns_exact_codex_user_agent` | 13180–13236 | `#[test]` |
| `minimax_protocol_selection_controls_wire_route_and_authentication` | 13241–13287 | `#[test]` |
| `custom_model_projection_applies_registered_request_mutations` | 13292–13335 | `#[test]` |
| `encode_request_applies_custom_reasoning_projection_to_final_wire_body` | 13340–13368 | `#[test]` |
| `required_disabled_reasoning_validation_rejects_dropped_or_contradictory_projection` | 13373–13586 | `#[test]` |
| `responses_failure_never_touches_other_protocol_endpoints` | 13592–13679 | `#[tokio::test]` |
| `bundled_usage_profiles_match_legacy_provider_functions` | 13684–13735 | `#[test]` |
| `deepseek_manifest_profiles_produce_legacy_equivalent_wire` | 13749–13863 | `#[test]` |
| `alibaba_manifest_profiles_produce_legacy_equivalent_wire` | 13868–13902 | `#[test]` |
| `openrouter_manifest_profiles_produce_legacy_equivalent_wire` | 13907–13941 | `#[test]` |
| `openrouter_reasoning_max_tokens_projects_via_profile` | 13946–13960 | `#[test]` |
| `kimi_manifest_default_profile_matches_legacy_wire` | 13965–13987 | `#[test]` |
| `minimax_manifest_request_profiles_produce_legacy_equivalent_wire` | 13992–14045 | `#[test]` |
| `minimax_profile_rejects_unsupported_effort` | 14050–14067 | `#[test]` |
| `response_profile_projects_stream_record_before_decode` | 14072–14091 | `#[test]` |
| `error_profile_projects_vendor_error_before_decode` | 14098–14117 | `#[test]` |
| `response_profile_rejects_non_object_target_ancestor_without_panic` | 14122–14136 | `#[test]` |
| `request_profile_rejects_non_object_target_ancestor_without_panic` | 14141–14162 | `#[test]` |
| `bounded_json_template_renders_nested_registered_forms` | 14167–14260 | `#[test]` |
| `bounded_json_template_rejects_ambiguous_forms_and_array_overflow` | 14265–14299 | `#[test]` |
| `bounded_json_template_rejects_depth_and_output_overflow` | 14304–14332 | `#[test]` |
| `projection_scalar_keys_cover_string_number_and_boolean_values` | 14337–14352 | `#[test]` |

## 原始辅助节点

- `ProtocolRuntimeSource for Arc<ProtocolRegistry>::into_protocol_runtime`：原 L9380–9382。
- `bundled_manifest_route`：原 L9450–9504。
- `minimal_request`：原 L10476–10502。
- `request_with_codex_continuation`：原 L11078–11089。
- `read_test_http_request`：原 L11103–11140。
- `pinned_openrouter_route`：原 L12308–12316。
- `minimax_route`：原 L12330–12332。
- `deepseek_route`：原 L12346–12348。
- `kimi_coding_route`：原 L12352–12354。
- `alibaba_workspace_route`：原 L12368–12374。
- `projection_test_client`：原 L13739–13744。
