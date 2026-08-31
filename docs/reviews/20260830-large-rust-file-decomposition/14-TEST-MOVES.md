# 14 号 Provider 传输测试迁移映射

旧名称前缀：`agent_runtime::provider_transport_vulcan::tests::`。
新名称：根组维持旧前缀；其余组使用 `agent_runtime::provider_transport_vulcan::<模块>::tests::`，测试函数名不变。

36 项保留，1 项经当前生产调用证据确认无效后移除；详见逐文件记录。原异步测试属性保留。

| 测试名称 | 原函数行 | 新归属 |
| --- | ---: | --- |
| `provider_projection_rejects_same_length_mutated_managed_attachment` | 3903 | `message_projection` |
| `provider_request_trace_headers_redact_access_material` | 3966 | `trace` |
| `provider_api_trace_failure_selection_preserves_the_earliest_boundary` | 3998 | `trace` |
| `provider_api_trace_classifies_tool_request_coordinator_events` | 4023 | `trace` |
| `provider_api_trace_processing_failure_sender_publishes_once` | 4044 | `trace` |
| `provider_response_trace_body_uses_the_smaller_exact_encoding` | 4062 | 已移除：仅测试已弃用的测试专用实现 |
| `configured_tool_schema_projection_modes_map_without_heuristics` | 4088 | `request` |
| `successful_empty_or_transient_only_response_is_rejected` | 4153 | `accumulator` |
| `successful_response_requires_one_normalized_durable_semantic_item` | 4189 | `accumulator` |
| `response_completion_is_a_protocol_terminal_boundary` | 4248 | `streaming` |
| `provider_trace_drain_consumes_the_remaining_stream` | 4265 | `trace` |
| `partial_usage_updates_merge_without_erasing_known_fields` | 4292 | `accumulator` |
| `transient_reasoning_mode_fails_closed_on_multiple_profile_claims` | 4320 | `accumulator` |
| `responses_compaction_context_requires_exact_route_identity` | 4354 | `message_projection` |
| `ordinary_replay_content_requires_exact_connection_route` | 4433 | `message_projection` |
| `public_turn_persists_exact_responses_provider_context` | 4575 | `accumulator` |
| `public_tool_abort_becomes_non_executable_tool_call` | 4652 | `accumulator` |
| `partial_provider_context_falls_back_to_normalized_replay` | 4733 | `accumulator` |
| `public_turn_deltas_are_forwarded_to_live_workbench_stream` | 4763 | `accumulator` |
| `transient_reasoning_summary_is_live_only_and_clears_at_terminal` | 4843 | `accumulator` |
| `durable_reasoning_supersedes_transient_fallbacks` | 4955 | `accumulator` |
| `generated_image_event_materializes_inside_workspace` | 5040 | `generated_resources` |
| `remote_generated_image_url_remains_remote` | 5099 | `generated_resources` |
| `generated_resource_stem_removes_path_syntax` | 5132 | `generated_resources` |
| `public_output_limit_uses_default_below_catalog_maximum` | 5142 | `root` |
| `public_output_limit_honors_user_narrowing_and_catalog_cap` | 5152 | `root` |
| `continuation_rejects_protocol_profile_switch` | 5166 | `request` |
| `public_execution_failure_message_is_json` | 5213 | `root` |
| `frozen_resource_constraints_are_provider_neutral` | 5249 | `input_resources` |
| `resource_projection_fallback_uses_the_safe_resource_path` | 5350 | `input_resources` |
| `prepared_provider_file_id_preserves_reference_encoding` | 5380 | `input_resources` |
| `provider_file_id_fallback_without_resource_path_is_redacted` | 5422 | `input_resources` |
| `prepared_response_url_preserves_canonical_resource_identity` | 5449 | `input_resources` |
| `inline_resource_budget_accounts_for_exact_base64_expansion` | 5483 | `input_resources` |
| `inline_resource_budget_enforces_request_wide_resource_count` | 5508 | `input_resources` |
| `inline_data_resource_reports_exact_raw_byte_length` | 5522 | `input_resources` |
| `protocol_resource_kind_gate_keeps_generic_files_metadata_only` | 5541 | `input_resources` |
