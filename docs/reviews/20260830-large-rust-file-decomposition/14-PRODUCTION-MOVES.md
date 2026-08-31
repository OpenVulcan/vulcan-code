# 14 号生产符号迁移与可见性记录

源文件：`crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_vulcan.rs`，基线提交 `111540689b777ddc1229a6f17293a42818f2155e`。

## 等价核对

72 个有效生产类型／函数／方法均保留。连同 36 个有效测试和 3 个测试夹具符号，共 111 个符号与原文词法等价：比较保留字符串字面量，忽略注释、布局、可选尾逗号及下述有依据的内部可见性变化。新增符号 0，缺失仅为已说明移除的测试专用编码函数与其无效测试。

## 可见性与依赖

- `VulcanPublicContractAdapter` 保留根类型及全部私有字段；请求构建和流实现仍为同一类型的原 trait 实现，不引入第二持有者。
- `VulcanPublicContractRequest`、工具投影、原生工具和压缩消息入口均由原根路径重导出，原 `pub(crate)` 可见范围不变。
- `public_usage_to_core` 从子模块使用 `pub(in crate::agent_runtime)`，根仍以原 `pub(super)` 重导出，因此有效范围不扩大。
- 追踪跨模块类型、两个适配器追踪方法、失败发布器的构造／观察方法、追踪阶段分类与排空函数仅限 `pub(super)`，消费者是根适配器或 `streaming`。
- 累积器只开放类型、构造、应用事件、终态完成、未结束工具查询与被流执行读取的 `usage` 字段到父模块范围；其他内部状态及方法保持私有。测试位于其子模块，不为测试放宽状态访问。
- 输入资源只开放预算类型与资源解析入口；预算字段和方法保持私有。生成资源只开放事件落盘入口。历史投影只开放请求构建所需的规范消息／文本转换，重放解析保持私有。
- 兄弟模块依赖为 `streaming → accumulator / trace / generated_resources`、`request → message_projection / input_resources`、`message_projection → input_resources`，无兄弟模块循环。

## 生产符号映射

| 符号 | 原行 | 新文件 | 新行 |
| --- | ---: | --- | ---: |
| `enum:ProviderApiTraceResponseState` | 82 | `provider_transport_vulcan/trace.rs` | 24 |
| `struct:ProviderApiTraceProcessingFailure` | 94 | `provider_transport_vulcan/trace.rs` | 36 |
| `struct:ProviderApiTraceProcessingFailureSender` | 105 | `provider_transport_vulcan/trace.rs` | 47 |
| `ProviderApiTraceProcessingFailureSender::new` | 124 | `provider_transport_vulcan/trace.rs` | 66 |
| `ProviderApiTraceProcessingFailureSender::observe_failure` | 137 | `provider_transport_vulcan/trace.rs` | 79 |
| `struct:ProviderApiTraceAttemptState` | 151 | `provider_transport_vulcan/trace.rs` | 93 |
| `struct:InlineResourceProjectionBudget` | 163 | `provider_transport_vulcan/input_resources.rs` | 23 |
| `InlineResourceProjectionBudget::reserve` | 187 | `provider_transport_vulcan/input_resources.rs` | 47 |
| `InlineResourceProjectionBudget::count_limit_reached` | 236 | `provider_transport_vulcan/input_resources.rs` | 96 |
| `transient_reasoning_status_kind` | 259 | `provider_transport_vulcan/accumulator.rs` | 36 |
| `provider_request_trace_timestamp_ms` | 292 | `provider_transport_vulcan/trace.rs` | 109 |
| `provider_request_trace_headers` | 311 | `provider_transport_vulcan/trace.rs` | 128 |
| `select_provider_api_trace_failure` | 355 | `provider_transport_vulcan/trace.rs` | 172 |
| `resolved_public_output_token_limit` | 422 | `provider_transport_vulcan.rs` | 63 |
| `execute_vulcan_public_stream` | 444 | `provider_transport_vulcan.rs` | 85 |
| `capture_vulcan_public_stream` | 506 | `provider_transport_vulcan.rs` | 147 |
| `struct:VulcanPublicContractAdapter` | 573 | `provider_transport_vulcan.rs` | 214 |
| `struct:VulcanPublicContractRequest` | 640 | `provider_transport_vulcan/request.rs` | 27 |
| `VulcanPublicContractAdapter::new` | 682 | `provider_transport_vulcan.rs` | 296 |
| `VulcanPublicContractAdapter::claim_provider_api_trace_attempt` | 744 | `provider_transport_vulcan/trace.rs` | 195 |
| `VulcanPublicContractAdapter::persist_provider_api_trace_snapshot` | 797 | `provider_transport_vulcan/trace.rs` | 248 |
| `configured_tool_schema_projection_profile` | 1021 | `provider_transport_vulcan/request.rs` | 63 |
| `project_public_tool_definitions` | 1046 | `provider_transport_vulcan/request.rs` | 88 |
| `resolve_public_native_tools` | 1082 | `provider_transport_vulcan/request.rs` | 124 |
| `VulcanProviderAdapter for VulcanPublicContractAdapter::provider_id` | 1101 | `provider_transport_vulcan/request.rs` | 143 |
| `VulcanProviderAdapter for VulcanPublicContractAdapter::model_id` | 1105 | `provider_transport_vulcan/request.rs` | 147 |
| `VulcanProviderAdapter for VulcanPublicContractAdapter::build_request` | 1109 | `provider_transport_vulcan/request.rs` | 151 |
| `VulcanProviderAdapter for VulcanPublicContractAdapter::execute_request` | 1234 | `provider_transport_vulcan/request.rs` | 276 |
| `build_public_continuation` | 1259 | `provider_transport_vulcan/request.rs` | 301 |
| `public_stream_event_is_terminal` | 1322 | `provider_transport_vulcan/streaming.rs` | 43 |
| `provider_trace_processing_stage_for_event` | 1343 | `provider_transport_vulcan/trace.rs` | 472 |
| `drain_provider_stream_for_trace` | 1364 | `provider_transport_vulcan/trace.rs` | 493 |
| `recover_rejected_oauth_credential` | 1380 | `provider_transport_vulcan/streaming.rs` | 62 |
| `VulcanStreamingProviderAdapter for VulcanPublicContractAdapter::execute_request_streaming` | 1412 | `provider_transport_vulcan/streaming.rs` | 94 |
| `attach_execution_route_context` | 1859 | `provider_transport_vulcan/streaming.rs` | 541 |
| `public_execution_failure` | 1882 | `provider_transport_vulcan.rs` | 364 |
| `public_execution_failure_message` | 1895 | `provider_transport_vulcan.rs` | 377 |
| `struct:PublicTurnAccumulator` | 1904 | `provider_transport_vulcan/accumulator.rs` | 65 |
| `struct:ReplayAccumulatorItem` | 1958 | `provider_transport_vulcan/accumulator.rs` | 119 |
| `enum:ReplayAccumulatorItemState` | 1970 | `provider_transport_vulcan/accumulator.rs` | 131 |
| `provider_context_items_cover_replay_content` | 2012 | `provider_transport_vulcan/accumulator.rs` | 173 |
| `PublicTurnAccumulator::new` | 2097 | `provider_transport_vulcan/accumulator.rs` | 258 |
| `PublicTurnAccumulator::emit_transient_reasoning_status` | 2138 | `provider_transport_vulcan/accumulator.rs` | 299 |
| `PublicTurnAccumulator::clear_transient_reasoning_status` | 2151 | `provider_transport_vulcan/accumulator.rs` | 312 |
| `PublicTurnAccumulator::register_replay_item` | 2171 | `provider_transport_vulcan/accumulator.rs` | 332 |
| `PublicTurnAccumulator::replay_item_state_mut` | 2201 | `provider_transport_vulcan/accumulator.rs` | 362 |
| `PublicTurnAccumulator::replay_content` | 2226 | `provider_transport_vulcan/accumulator.rs` | 387 |
| `PublicTurnAccumulator::apply_event` | 2254 | `provider_transport_vulcan/accumulator.rs` | 415 |
| `PublicTurnAccumulator::has_pending_tool_request` | 2583 | `provider_transport_vulcan/accumulator.rs` | 744 |
| `PublicTurnAccumulator::finish` | 2591 | `provider_transport_vulcan/accumulator.rs` | 752 |
| `materialize_provider_resource_event` | 2663 | `provider_transport_vulcan/generated_resources.rs` | 27 |
| `parse_generated_data_url` | 2766 | `provider_transport_vulcan/generated_resources.rs` | 130 |
| `generated_image_descriptor` | 2809 | `provider_transport_vulcan/generated_resources.rs` | 173 |
| `safe_generated_resource_stem` | 2848 | `provider_transport_vulcan/generated_resources.rs` | 212 |
| `resolve_public_message_resources` | 2882 | `provider_transport_vulcan/input_resources.rs` | 122 |
| `apply_prepared_media_reference` | 3092 | `provider_transport_vulcan/input_resources.rs` | 332 |
| `inline_data_resource_raw_byte_len` | 3121 | `provider_transport_vulcan/input_resources.rs` | 361 |
| `resource_projection_budget_fallback_text` | 3157 | `provider_transport_vulcan/input_resources.rs` | 397 |
| `resource_projection_unavailability_reason` | 3224 | `provider_transport_vulcan/input_resources.rs` | 464 |
| `protocol_profile_supports_resource_kind` | 3277 | `provider_transport_vulcan/input_resources.rs` | 517 |
| `frozen_resource_constraint_unavailability_reason` | 3312 | `provider_transport_vulcan/input_resources.rs` | 552 |
| `input_resource_constraint` | 3361 | `provider_transport_vulcan/input_resources.rs` | 601 |
| `build_responses_compaction_messages` | 3389 | `provider_transport_vulcan/message_projection.rs` | 40 |
| `core_message_to_public` | 3463 | `provider_transport_vulcan/message_projection.rs` | 114 |
| `replay_content_for_route` | 3652 | `provider_transport_vulcan/message_projection.rs` | 303 |
| `replay_provider_context_items_for_route` | 3697 | `provider_transport_vulcan/message_projection.rs` | 348 |
| `responses_compaction_items_for_route` | 3756 | `provider_transport_vulcan/message_projection.rs` | 407 |
| `core_message_text` | 3818 | `provider_transport_vulcan/message_projection.rs` | 469 |
| `public_usage_to_core` | 3834 | `provider_transport_vulcan/accumulator.rs` | 809 |
| `known_usage` | 3858 | `provider_transport_vulcan/accumulator.rs` | 835 |
| `known_usage_u64` | 3877 | `provider_transport_vulcan/accumulator.rs` | 854 |
| `provider_failure` | 3886 | `provider_transport_vulcan.rs` | 385 |
