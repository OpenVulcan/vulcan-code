# 03：测试与辅助节点迁移映射

## 保全结论

2026-08-31，第16阶段快照：原103个测试全部可发现，103个随职责迁移、0个保留原路径；11个辅助节点保持唯一，无删除、忽略、断言弱化或新增占位测试。所有新测试文件通过所属生产模块的 `#[cfg(test)] mod tests;` 接入，共享夹具模块也仅在测试条件启用。

- 最新实际包级运行：644通过，0失败、0忽略，27.93秒；包含以下全部103项。
- 103个测试函数名和属性保持；101个函数体严格词法等价，2个测试仅有精确限定的rustfmt表达式块折叠：Schema测试231→229 token，Apply Patch历史投影测试717→715 token。全部输入、字符串、断言与控制语义保持，详见03-FINAL-REVIEW.md。
- 本文件测试前缀严格为 `execution::`；工具执行的16项不计入本文件103项，但计入包级回归。
- 冻结前清单见 `03-EXECUTION_TEST_BASELINE.md`。

## 完整测试路径

所有旧路径均为 `execution::tests::<测试名>`，以下同时列明未迁移用例。

| 测试名 | 原始行 | 当前发现路径 | 当前行 |
| --- | ---: | --- | ---: |
| `unfinished_wire_response_capture_records_cancelled_stage` | 9388 | `execution::wire_capture::tests::unfinished_wire_response_capture_records_cancelled_stage` | 9 |
| `every_builtin_provider_protocol_routes_through_responses_core` | 9410 | `execution::route::tests::every_builtin_provider_protocol_routes_through_responses_core` | 11 |
| `bundled_media_upload_routes_are_manifest_driven` | 9509 | `execution::prepared_media::tests::bundled_media_upload_routes_are_manifest_driven` | 13 |
| `prepared_media_key_isolates_frozen_route_identity` | 9614 | `execution::prepared_media::tests::prepared_media_key_isolates_frozen_route_identity` | 117 |
| `prepared_media_key_honors_explicit_upload_reference_scopes` | 9672 | `execution::prepared_media::tests::prepared_media_key_honors_explicit_upload_reference_scopes` | 175 |
| `prepared_media_cache_pruning_is_bounded` | 9750 | `execution::prepared_media::tests::prepared_media_cache_pruning_is_bounded` | 253 |
| `prepared_media_cache_keeps_durable_conversation_reference` | 9804 | `execution::prepared_media::tests::prepared_media_cache_keeps_durable_conversation_reference` | 307 |
| `provider_tool_reference_reuse_uses_first_use_and_provider_expiry` | 9830 | `execution::prepared_media::tests::provider_tool_reference_reuse_uses_first_use_and_provider_expiry` | 333 |
| `prepared_media_fingerprint_detects_equal_length_content_replacement` | 9862 | `execution::prepared_media::tests::prepared_media_fingerprint_detects_equal_length_content_replacement` | 365 |
| `prepared_inline_media_read_binds_retained_bytes_to_fingerprint` | 9893 | `execution::prepared_media::tests::prepared_inline_media_read_binds_retained_bytes_to_fingerprint` | 396 |
| `exact_provider_routes_install_specialized_usage_profiles` | 9929 | `execution::route::tests::exact_provider_routes_install_specialized_usage_profiles` | 41 |
| `profiled_usage_gate_releases_usage_after_response_start` | 10005 | `execution::usage::tests::profiled_usage_gate_releases_usage_after_response_start` | 11 |
| `profiled_usage_gate_rejects_missing_response_start` | 10033 | `execution::usage::tests::profiled_usage_gate_rejects_missing_response_start` | 39 |
| `profiled_usage_gate_rejects_unbounded_pre_start_events` | 10049 | `execution::usage::tests::profiled_usage_gate_rejects_unbounded_pre_start_events` | 55 |
| `error_json_body_profile_extracts_billable_usage` | 10082 | `execution::usage::tests::error_json_body_profile_extracts_billable_usage` | 88 |
| `unregistered_error_usage_kind_does_not_parse_plain_text` | 10123 | `execution::usage::tests::unregistered_error_usage_kind_does_not_parse_plain_text` | 129 |
| `usage_output_marks_subsequent_transport_failure_retry_unsafe` | 10152 | `execution::stream_state::tests::usage_output_marks_subsequent_transport_failure_retry_unsafe` | 10 |
| `stream_idle_timeout_remains_retryable_after_semantic_output` | 10168 | `execution::failure::tests::stream_idle_timeout_remains_retryable_after_semantic_output` | 9 |
| `http_context_length_code_is_typed` | 10187 | `execution::failure::tests::http_context_length_code_is_typed` | 28 |
| `execution_failure_boundary_redacts_exact_secret_and_provider_urls` | 10207 | `execution::failure::tests::execution_failure_boundary_redacts_exact_secret_and_provider_urls` | 48 |
| `provider_request_id_uses_declared_order` | 10254 | `execution::response_metadata::tests::provider_request_id_uses_declared_order` | 12 |
| `provider_diagnostic_header_values_are_bounded` | 10281 | `execution::response_metadata::tests::provider_diagnostic_header_values_are_bounded` | 39 |
| `http_send_failure_uses_reqwest_connection_evidence` | 10330 | `execution::failure::tests::http_send_failure_uses_reqwest_connection_evidence` | 95 |
| `http_stream_failure_retains_http_transport` | 10385 | `execution::failure::tests::http_stream_failure_retains_http_transport` | 150 |
| `websocket_protocol_failure_retains_websocket_transport` | 10395 | `execution::failure::tests::websocket_protocol_failure_retains_websocket_transport` | 160 |
| `empty_http_error_response_reports_status` | 10411 | `execution::failure::tests::empty_http_error_response_reports_status` | 176 |
| `http_server_overloaded_code_is_retryable` | 10424 | `execution::failure::tests::http_server_overloaded_code_is_retryable` | 189 |
| `http_server_overloaded_plain_text_is_retryable` | 10444 | `execution::failure::tests::http_server_overloaded_plain_text_is_retryable` | 209 |
| `compact_shape_trace_labels_are_bounded_and_sanitized` | 10462 | `execution::transport_diagnostics::tests::compact_shape_trace_labels_are_bounded_and_sanitized` | 9 |
| `apply_patch_standard_function_survives_production_protocol_projection` | 10509 | `execution::tests::protocol_fidelity::apply_patch_standard_function_survives_production_protocol_projection` | 26 |
| `protocol_roles_preserve_instruction_and_tool_identity_across_switches` | 10669 | `execution::tests::protocol_fidelity::protocol_roles_preserve_instruction_and_tool_identity_across_switches` | 185 |
| `protocol_roles_reject_missing_conversion_before_dispatch` | 10865 | `execution::tests::protocol_fidelity::protocol_roles_reject_missing_conversion_before_dispatch` | 385 |
| `gemini_generate_content_route_uses_native_model_path_and_controls` | 10902 | `execution::tests::protocol_fidelity::gemini_generate_content_route_uses_native_model_path_and_controls` | 422 |
| `apikeyfun_gemini_route_separates_execution_and_discovery_origins` | 10941 | `execution::tests::protocol_fidelity::apikeyfun_gemini_route_separates_execution_and_discovery_origins` | 460 |
| `responses_websocket_cache_key_is_bounded_and_endpoint_bound` | 10973 | `execution::responses_websocket::tests::responses_websocket_cache_key_is_bounded_and_endpoint_bound` | 44 |
| `responses_websocket_breakers_are_bounded_and_lru` | 10996 | `execution::responses_websocket::tests::responses_websocket_breakers_are_bounded_and_lru` | 67 |
| `gemini_interactions_route_uses_responses_baseline_and_preserves_replay` | 11026 | `execution::tests::protocol_fidelity::gemini_interactions_route_uses_responses_baseline_and_preserves_replay` | 492 |
| `http_wire_request_capture_matches_dispatched_api_body` | 11145 | `execution::http_transport::tests::http_wire_request_capture_matches_dispatched_api_body` | 22 |
| `http_trace_persistence_failure_does_not_prevent_network_dispatch` | 11312 | `execution::http_transport::tests::http_trace_persistence_failure_does_not_prevent_network_dispatch` | 189 |
| `http_wire_response_capture_classifies_dispatch_failure` | 11394 | `execution::http_transport::tests::http_wire_response_capture_classifies_dispatch_failure` | 271 |
| `http_wire_response_capture_survives_protocol_failure` | 11464 | `execution::http_transport::tests::http_wire_response_capture_survives_protocol_failure` | 341 |
| `http_wire_response_capture_survives_tool_argument_abort` | 11591 | `execution::http_transport::tests::http_wire_response_capture_survives_tool_argument_abort` | 468 |
| `nonpersistent_responses_continuation_is_scoped_to_the_active_websocket` | 11730 | `execution::responses_websocket::tests::nonpersistent_responses_continuation_is_scoped_to_the_active_websocket` | 97 |
| `websocket_retry_controls_preserve_code_and_semantic_safety` | 11771 | `execution::responses_websocket::tests::websocket_retry_controls_preserve_code_and_semantic_safety` | 138 |
| `websocket_1013_failure_falls_back_to_full_history_http_attempt` | 11810 | `execution::responses_websocket::tests::websocket_1013_failure_falls_back_to_full_history_http_attempt` | 177 |
| `websocket_previous_response_not_found_retries_without_stale_cursor` | 11914 | `execution::responses_websocket::tests::websocket_previous_response_not_found_retries_without_stale_cursor` | 280 |
| `codex_compact_body_removes_stream_only_and_unknown_fields` | 12058 | `execution::responses_compact::tests::codex_compact_body_removes_stream_only_and_unknown_fields` | 10 |
| `codex_compact_body_preserves_nonempty_base_instructions` | 12093 | `execution::responses_compact::tests::codex_compact_body_preserves_nonempty_base_instructions` | 45 |
| `codex_compact_response_requires_a_compaction_item` | 12113 | `execution::responses_compact::tests::codex_compact_response_requires_a_compaction_item` | 65 |
| `codex_protocol_projection_preserves_socket_proven_previous_response_id` | 12181 | `execution::tests::protocol_fidelity::codex_protocol_projection_preserves_socket_proven_previous_response_id` | 545 |
| `non_codex_routes_replay_history_without_projecting_codex_continuation` | 12215 | `execution::tests::protocol_fidelity::non_codex_routes_replay_history_without_projecting_codex_continuation` | 579 |
| `responses_compact_transport_failure_is_dispatched_and_retryable` | 12249 | `execution::failure::tests::responses_compact_transport_failure_is_dispatched_and_retryable` | 227 |
| `responses_http_completion_failure_preserves_retry_safety` | 12262 | `execution::failure::tests::responses_http_completion_failure_preserves_retry_safety` | 240 |
| `alibaba_workspace_origin_is_protocol_scoped` | 12379 | `execution::tests::provider_contracts::alibaba_workspace_origin_is_protocol_scoped` | 65 |
| `sse_parser_handles_arbitrary_chunks` | 12395 | `execution::framing::tests::sse_parser_handles_arbitrary_chunks` | 9 |
| `sse_parser_accepts_cr_only_and_mixed_line_endings` | 12420 | `execution::framing::tests::sse_parser_accepts_cr_only_and_mixed_line_endings` | 34 |
| `bounded_parser_supports_json_lines_ndjson_and_single_json` | 12446 | `execution::framing::tests::bounded_parser_supports_json_lines_ndjson_and_single_json` | 60 |
| `bounded_parser_rejects_oversized_frames_and_buffers` | 12492 | `execution::framing::tests::bounded_parser_rejects_oversized_frames_and_buffers` | 106 |
| `response_content_type_is_strict_and_parameter_tolerant` | 12515 | `execution::response_metadata::tests::response_content_type_is_strict_and_parameter_tolerant` | 87 |
| `retry_after_numeric_values_preserve_milliseconds` | 12542 | `execution::response_metadata::tests::retry_after_numeric_values_preserve_milliseconds` | 114 |
| `retry_after_http_date_preserves_source` | 12554 | `execution::response_metadata::tests::retry_after_http_date_preserves_source` | 126 |
| `retry_after_malformed_value_is_rejected` | 12572 | `execution::response_metadata::tests::retry_after_malformed_value_is_rejected` | 143 |
| `encode_request_projects_pinned_openrouter_route_for_every_protocol` | 12584 | `execution::tests::provider_contracts::encode_request_projects_pinned_openrouter_route_for_every_protocol` | 81 |
| `encode_request_preserves_structured_output_schema_for_supported_protocols` | 12608 | `execution::request_encoding::tests::encode_request_preserves_structured_output_schema_for_supported_protocols` | 12 |
| `encode_request_rejects_structured_schema_loss_after_custom_projection` | 12652 | `execution::request_encoding::tests::encode_request_rejects_structured_schema_loss_after_custom_projection` | 52 |
| `kimi_coding_projection_uses_thinking_and_device_identity` | 12684 | `execution::tests::provider_contracts::kimi_coding_projection_uses_thinking_and_device_identity` | 105 |
| `minimax_openai_chat_projects_adaptive_thinking` | 12733 | `execution::tests::provider_contracts::minimax_openai_chat_projects_adaptive_thinking` | 154 |
| `minimax_anthropic_projects_disabled_thinking` | 12759 | `execution::tests::provider_contracts::minimax_anthropic_projects_disabled_thinking` | 180 |
| `minimax_anthropic_preserves_structured_output_format` | 12778 | `execution::tests::provider_contracts::minimax_anthropic_preserves_structured_output_format` | 199 |
| `minimax_rejects_undocumented_reasoning_effort` | 12816 | `execution::tests::provider_contracts::minimax_rejects_undocumented_reasoning_effort` | 237 |
| `deepseek_openai_chat_projects_disabled_thinking` | 12839 | `execution::tests::provider_contracts::deepseek_openai_chat_projects_disabled_thinking` | 260 |
| `deepseek_thinking_omits_ineffective_temperature` | 12863 | `execution::tests::provider_contracts::deepseek_thinking_omits_ineffective_temperature` | 284 |
| `deepseek_anthropic_projects_disabled_and_enabled_thinking` | 12886 | `execution::tests::provider_contracts::deepseek_anthropic_projects_disabled_and_enabled_thinking` | 307 |
| `commandcode_projects_native_chat_and_anthropic_reasoning` | 12919 | `execution::tests::provider_contracts::commandcode_projects_native_chat_and_anthropic_reasoning` | 340 |
| `deepseek_anthropic_preserves_native_web_search` | 12957 | `execution::tests::provider_contracts::deepseek_anthropic_preserves_native_web_search` | 374 |
| `deepseek_responses_projects_native_web_search` | 12977 | `execution::tests::provider_contracts::deepseek_responses_projects_native_web_search` | 394 |
| `encode_request_normalizes_openrouter_reasoning_for_every_protocol` | 13010 | `execution::tests::provider_contracts::encode_request_normalizes_openrouter_reasoning_for_every_protocol` | 427 |
| `encode_request_projects_openrouter_adaptive_reasoning_as_enabled` | 13038 | `execution::tests::provider_contracts::encode_request_projects_openrouter_adaptive_reasoning_as_enabled` | 455 |
| `encode_request_rejects_conflicting_openrouter_reasoning_controls` | 13062 | `execution::tests::provider_contracts::encode_request_rejects_conflicting_openrouter_reasoning_controls` | 479 |
| `encode_request_rejects_openrouter_explicit_prompt_cache_from_manifest` | 13087 | `execution::tests::provider_contracts::encode_request_rejects_openrouter_explicit_prompt_cache_from_manifest` | 504 |
| `execution_headers_merge_protocol_headers_and_reject_credential_overrides` | 13112 | `execution::request_headers::tests::execution_headers_merge_protocol_headers_and_reject_credential_overrides` | 18 |
| `native_codex_protocol_owns_exact_codex_user_agent` | 13180 | `execution::request_headers::tests::native_codex_protocol_owns_exact_codex_user_agent` | 85 |
| `minimax_protocol_selection_controls_wire_route_and_authentication` | 13241 | `execution::tests::provider_contracts::minimax_protocol_selection_controls_wire_route_and_authentication` | 529 |
| `custom_model_projection_applies_registered_request_mutations` | 13292 | `execution::request_encoding::tests::custom_model_projection_applies_registered_request_mutations` | 84 |
| `encode_request_applies_custom_reasoning_projection_to_final_wire_body` | 13340 | `execution::request_encoding::tests::encode_request_applies_custom_reasoning_projection_to_final_wire_body` | 132 |
| `required_disabled_reasoning_validation_rejects_dropped_or_contradictory_projection` | 13373 | `execution::request_encoding::tests::required_disabled_reasoning_validation_rejects_dropped_or_contradictory_projection` | 165 |
| `responses_failure_never_touches_other_protocol_endpoints` | 13592 | `execution::http_transport::tests::responses_failure_never_touches_other_protocol_endpoints` | 608 |
| `bundled_usage_profiles_match_legacy_provider_functions` | 13684 | `execution::usage::tests::bundled_usage_profiles_match_legacy_provider_functions` | 158 |
| `deepseek_manifest_profiles_produce_legacy_equivalent_wire` | 13749 | `execution::tests::manifest_equivalence::deepseek_manifest_profiles_produce_legacy_equivalent_wire` | 26 |
| `alibaba_manifest_profiles_produce_legacy_equivalent_wire` | 13868 | `execution::tests::manifest_equivalence::alibaba_manifest_profiles_produce_legacy_equivalent_wire` | 145 |
| `openrouter_manifest_profiles_produce_legacy_equivalent_wire` | 13907 | `execution::tests::manifest_equivalence::openrouter_manifest_profiles_produce_legacy_equivalent_wire` | 183 |
| `openrouter_reasoning_max_tokens_projects_via_profile` | 13946 | `execution::tests::manifest_equivalence::openrouter_reasoning_max_tokens_projects_via_profile` | 225 |
| `kimi_manifest_default_profile_matches_legacy_wire` | 13965 | `execution::tests::manifest_equivalence::kimi_manifest_default_profile_matches_legacy_wire` | 244 |
| `minimax_manifest_request_profiles_produce_legacy_equivalent_wire` | 13992 | `execution::tests::manifest_equivalence::minimax_manifest_request_profiles_produce_legacy_equivalent_wire` | 271 |
| `minimax_profile_rejects_unsupported_effort` | 14050 | `execution::tests::manifest_equivalence::minimax_profile_rejects_unsupported_effort` | 332 |
| `response_profile_projects_stream_record_before_decode` | 14072 | `execution::projection_profiles::tests::response_profile_projects_stream_record_before_decode` | 10 |
| `error_profile_projects_vendor_error_before_decode` | 14098 | `execution::projection_profiles::tests::error_profile_projects_vendor_error_before_decode` | 36 |
| `response_profile_rejects_non_object_target_ancestor_without_panic` | 14122 | `execution::projection_profiles::tests::response_profile_rejects_non_object_target_ancestor_without_panic` | 60 |
| `request_profile_rejects_non_object_target_ancestor_without_panic` | 14141 | `execution::projection_profiles::tests::request_profile_rejects_non_object_target_ancestor_without_panic` | 79 |
| `bounded_json_template_renders_nested_registered_forms` | 14167 | `execution::json_template::tests::bounded_json_template_renders_nested_registered_forms` | 10 |
| `bounded_json_template_rejects_ambiguous_forms_and_array_overflow` | 14265 | `execution::json_template::tests::bounded_json_template_rejects_ambiguous_forms_and_array_overflow` | 108 |
| `bounded_json_template_rejects_depth_and_output_overflow` | 14304 | `execution::json_template::tests::bounded_json_template_rejects_depth_and_output_overflow` | 146 |
| `projection_scalar_keys_cover_string_number_and_boolean_values` | 14337 | `execution::projection_profiles::tests::projection_scalar_keys_cover_string_number_and_boolean_values` | 105 |

## 辅助节点

| 节点 | 原始行 | 当前相对源码路径 | 当前行 |
| --- | --- | --- | --- |
| `ProtocolRuntimeSource for Arc<ProtocolRegistry>::into_protocol_runtime` | 9380–9382 | `execution/test_support.rs` | 207–209 |
| `bundled_manifest_route` | 9450–9504 | `execution/test_support.rs` | 29–83 |
| `minimal_request` | 10476–10502 | `execution/test_support.rs` | 87–113 |
| `request_with_codex_continuation` | 11078–11089 | `execution/responses_websocket/tests.rs` | 28–39 |
| `read_test_http_request` | 11103–11140 | `execution/test_support.rs` | 165–202 |
| `pinned_openrouter_route` | 12308–12316 | `execution/test_support.rs` | 127–135 |
| `minimax_route` | 12330–12332 | `execution/tests/provider_contracts.rs` | 32–34 |
| `deepseek_route` | 12346–12348 | `execution/test_support.rs` | 149–151 |
| `kimi_coding_route` | 12352–12354 | `execution/tests/provider_contracts.rs` | 38–40 |
| `alibaba_workspace_route` | 12368–12374 | `execution/tests/provider_contracts.rs` | 54–60 |
| `projection_test_client` | 13739–13744 | `execution/tests/manifest_equivalence.rs` | 16–21 |

## 匹配与保留理由

- 分帧4、用量6、Wire捕获1、JSON模板3、媒体8、Profile映射5、路由2、请求编码5、失败分类11、响应元数据6、请求头2、诊断1均对应实际职责。
- 远端压缩3、HTTP6、共享流状态1、WebSocket6保留全部请求/响应/网络/重试断言；HTTP和WebSocket真实环回测试均随实现迁移。
- 跨模块测试通过6行入口接入：协议保真8、供应商合同18、当前清单等价7；并非仅测试私有编码助手，保持公开客户端、真实路由与头合并的整条合同。
- 原11个辅助节点全部迁移：test_support中的5个共享函数和1个测试专用trait实现；WebSocket私有游标请求夹具1、供应商合同私有路由夹具3、清单等价私有客户端1。
- 未将私有夹具误泛化为共享接口；未删除、忽略或弱化任何测试。旧名称含legacy的测试仍对照当前真实生产清单与核心注册函数，未因名称陈旧删除有效覆盖。

## 整体目标剩余范围

03号及全部41个Rust产物已完成逐文件终审，根398行、19个生产子模块职责充分；完整依据见03-FINAL-REVIEW.md。整体完成6/15，另9个原始文件及最终连续五轮审核仍未完成，全局0/5，整体目标和计划保持进行中。
