# 会话文档迁移清单

## 不变量

- 单一状态持有者保持为 `SessionDocument`，公开类型保留原路径重导出。
- 原始方法及函数主体原样搬移；仅在已有父／兄弟调用要求时调整到文档内部可见性。
- `validate_provider_tool_selection_config` 原本供 `v1/config.rs` 使用，迁移后保持同一 `v1` 可见域及原入口。
- 保留全部现有测试文件、测试名称、属性和断言；迁移不触碰测试内容。
- 输入、回合、模式、API、工具、来源、选择、压缩、补水、持久化和子任务返回分别归属明确模块；直接兄弟模块导入图已检查无环。

## 完整迁移项

| 原始种类 | 名称 | 原始行号 | 新模块 |
| --- | --- | ---: | --- |
| 方法 | `submit_root_input` | 1348 | `document/input.rs` |
| 方法 | `enqueue_input` | 1451 | `document/input.rs` |
| 方法 | `set_draft` | 1582 | `document/input.rs` |
| 方法 | `clear_draft` | 1621 | `document/input.rs` |
| 方法 | `move_queued_input_to_draft` | 1651 | `document/input.rs` |
| 方法 | `remove_queued_input` | 1726 | `document/input.rs` |
| 方法 | `promote_queued_input_to_pending_turn` | 1786 | `document/input.rs` |
| 方法 | `queued_input_identity_for_action` | 1887 | `document/input.rs` |
| 方法 | `reorder_queued_inputs` | 1942 | `document/input.rs` |
| 方法 | `add_pending_in_turn_input` | 2008 | `document/input.rs` |
| 方法 | `input_id_already_streamed` | 7377 | `document/input.rs` |
| 方法 | `existing_root_submission` | 7682 | `document/input.rs` |
| 方法 | `existing_enqueue_receipt` | 7713 | `document/input.rs` |
| 方法 | `enqueue_disposition_from_scheduled_event` | 7790 | `document/input.rs` |
| 方法 | `is_goal_continuation_turn` | 7843 | `document/input.rs` |
| 方法 | `require_queued_input_index` | 7860 | `document/input.rs` |
| 方法 | `existing_queued_input_resolution` | 7886 | `document/input.rs` |
| 方法 | `existing_pending_submission` | 7912 | `document/input.rs` |
| 方法 | `require_client_action_unused` | 7947 | `document/input.rs` |
| 函数 | `validate_user_input` | 8648 | `document/input.rs` |
| 函数 | `event_client_action_id` | 8657 | `document/input.rs` |
| 函数 | `queued_input_identity` | 8667 | `document/input.rs` |
| 函数 | `merge_queued_input_identity` | 8676 | `document/input.rs` |
| 函数 | `consume_matching_draft` | 8694 | `document/input.rs` |
| 函数 | `validate_client_action_history` | 8877 | `document/input.rs` |
| 函数 | `validate_queued_input_resolution_history` | 9003 | `document/input.rs` |
| 类型 | `ClientInputSubmission` | 121 | `document/input.rs` |
| 类型实现 | `ClientInputSubmission` | 133 | `document/input.rs` |
| 类型 | `EnqueueInputDisposition` | 163 | `document/input.rs` |
| 类型 | `EnqueueInputReceipt` | 188 | `document/input.rs` |
| 类型 | `DraftMutationReceipt` | 203 | `document/input.rs` |
| 类型 | `QueuedInputDraftReceipt` | 212 | `document/input.rs` |
| 类型 | `QueuedInputRemovalReceipt` | 224 | `document/input.rs` |
| 类型 | `QueuedInputPendingTurnReceipt` | 233 | `document/input.rs` |
| 类型 | `QueuedInputOrderReceipt` | 248 | `document/input.rs` |
| 方法 | `prepare_and_start_api` | 2050 | `document/api.rs` |
| 方法 | `start_api` | 2173 | `document/api.rs` |
| 方法 | `mark_api_transport_observed` | 2236 | `document/api.rs` |
| 方法 | `api_transport_observed_for_action` | 2318 | `document/api.rs` |
| 方法 | `continuation_dispatch_resolution` | 2363 | `document/api.rs` |
| 方法 | `continuation_dispatch_resolution_through` | 2387 | `document/api.rs` |
| 方法 | `existing_api_start_for_action` | 2483 | `document/api.rs` |
| 方法 | `validate_continuation_action_for_api` | 2512 | `document/api.rs` |
| 方法 | `append_reasoning` | 2566 | `document/api.rs` |
| 方法 | `append_assistant_text` | 2614 | `document/api.rs` |
| 方法 | `complete_api_group` | 3353 | `document/api.rs` |
| 方法 | `interrupt_active_api` | 3593 | `document/api.rs` |
| 方法 | `interrupt_active_api_for_process` | 3614 | `document/api.rs` |
| 方法 | `interrupt_active_api_state` | 6964 | `document/api.rs` |
| 方法 | `stage_active_event_interruption` | 7111 | `document/api.rs` |
| 方法 | `stage_provider_source_attachment` | 7178 | `document/api.rs` |
| 方法 | `ensure_staged_active_api_tools_terminal` | 7230 | `document/api.rs` |
| 方法 | `validate_active_api_events` | 7279 | `document/api.rs` |
| 方法 | `validate_active_event` | 7294 | `document/api.rs` |
| 方法 | `update_provider_semantic` | 8073 | `document/api.rs` |
| 方法 | `api_transport_was_observed` | 6524 | `document/api.rs` |
| 函数 | `validate_provider_semantic_snapshot_transition` | 8614 | `document/api.rs` |
| 函数 | `validate_api_continuation_binding_shape` | 8724 | `document/api.rs` |
| 函数 | `validate_api_dispatch_history` | 8744 | `document/api.rs` |
| 函数 | `validate_execution_control_event` | 8848 | `document/api.rs` |
| 函数 | `validate_provider_request_budget_evidence` | 9189 | `document/api.rs` |
| 函数 | `validate_provider_budget_fingerprint` | 9254 | `document/api.rs` |
| 函数 | `validate_api_terminal_facts` | 9863 | `document/api.rs` |
| 函数 | `api_failure_notice` | 10080 | `document/api.rs` |
| 类型 | `ApiCompletion` | 496 | `document/api.rs` |
| 类型 | `ApiGroupCompletionReceipt` | 529 | `document/api.rs` |
| 类型 | `PrepareAndStartApiInput` | 547 | `document/api.rs` |
| 类型 | `PreparedApiStart` | 574 | `document/api.rs` |
| 类型 | `ApiTransportObservedReceipt` | 592 | `document/api.rs` |
| 类型 | `ContinuationDispatchResolution` | 610 | `document/api.rs` |
| 方法 | `prepare_tool` | 2662 | `document/tools.rs` |
| 方法 | `prepare_tool_batch` | 2693 | `document/tools.rs` |
| 方法 | `prepare_tool_batch_with_provider_source` | 2725 | `document/tools.rs` |
| 方法 | `prepare_tool_batch_internal` | 2737 | `document/tools.rs` |
| 方法 | `existing_prepared_tool_batch` | 2873 | `document/tools.rs` |
| 方法 | `start_prepared_tool` | 2943 | `document/tools.rs` |
| 方法 | `start_tool` | 2973 | `document/tools.rs` |
| 方法 | `tool_status` | 3012 | `document/tools.rs` |
| 方法 | `mark_tool_waiting_user` | 3023 | `document/tools.rs` |
| 方法 | `update_running_tool_output` | 3057 | `document/tools.rs` |
| 方法 | `finish_tool` | 3083 | `document/tools.rs` |
| 方法 | `answer_waiting_tool` | 3226 | `document/tools.rs` |
| 方法 | `cancel_waiting_tool` | 3290 | `document/tools.rs` |
| 方法 | `update_tool_in_place` | 8139 | `document/tools.rs` |
| 方法 | `require_tool_for_api` | 8186 | `document/tools.rs` |
| 函数 | `validate_tool_preparations` | 9487 | `document/tools.rs` |
| 函数 | `tool_status_name` | 9545 | `document/tools.rs` |
| 函数 | `is_terminal_tool_status` | 9560 | `document/tools.rs` |
| 函数 | `canonical_terminal_tool_output` | 9970 | `document/tools.rs` |
| 函数 | `tool_completion_receipt` | 10002 | `document/tools.rs` |
| 函数 | `validate_tool_lifecycle_shape` | 10023 | `document/tools.rs` |
| 函数 | `validate_tool_completion_changeset` | 10064 | `document/tools.rs` |
| 类型 | `ToolCompletion` | 403 | `document/tools.rs` |
| 类型 | `ToolCompletionReceipt` | 427 | `document/tools.rs` |
| 类型 | `ToolPreparation` | 448 | `document/tools.rs` |
| 类型 | `PreparedToolReceipt` | 466 | `document/tools.rs` |
| 类型 | `PreparedToolBatchReceipt` | 484 | `document/tools.rs` |
| 方法 | `continue_open_turn` | 3645 | `document/turns.rs` |
| 方法 | `existing_continuation_receipt` | 3848 | `document/turns.rs` |
| 方法 | `validate_continuation_eligibility` | 3974 | `document/turns.rs` |
| 方法 | `close_turn` | 4784 | `document/turns.rs` |
| 方法 | `stage_turn_closure` | 4834 | `document/turns.rs` |
| 方法 | `complete_turn_and_select_successor` | 4887 | `document/turns.rs` |
| 方法 | `require_final_assistant` | 7386 | `document/turns.rs` |
| 方法 | `aggregate_turn_buckets` | 8273 | `document/turns.rs` |
| 函数 | `continuation_guidance_key` | 8718 | `document/turns.rs` |
| 类型 | `ContinuationRequest` | 257 | `document/turns.rs` |
| 类型实现 | `ContinuationRequest` | 266 | `document/turns.rs` |
| 类型 | `ContinueTurnReceipt` | 312 | `document/turns.rs` |
| 类型 | `TurnSuccessor` | 642 | `document/turns.rs` |
| 类型 | `TurnCompletionTransition` | 677 | `document/turns.rs` |
| 方法 | `start_goal_continuation_turn` | 4042 | `document/modes.rs` |
| 方法 | `create_goal` | 4104 | `document/modes.rs` |
| 方法 | `save_goal` | 4215 | `document/modes.rs` |
| 方法 | `replace_completed_goal` | 4240 | `document/modes.rs` |
| 方法 | `update_goal_definition` | 4336 | `document/modes.rs` |
| 方法 | `complete_goal` | 4423 | `document/modes.rs` |
| 方法 | `clear_goal` | 4516 | `document/modes.rs` |
| 方法 | `pause_goal` | 4573 | `document/modes.rs` |
| 方法 | `resume_goal` | 4631 | `document/modes.rs` |
| 方法 | `set_plan_mode` | 4727 | `document/modes.rs` |
| 方法 | `block_goal` | 6926 | `document/modes.rs` |
| 方法 | `aggregate_goal_budget` | 7427 | `document/modes.rs` |
| 方法 | `require_goal_creation_boundary` | 7988 | `document/modes.rs` |
| 方法 | `require_no_active_api_for_mode_transition` | 8009 | `document/modes.rs` |
| 方法 | `require_goal_enabled` | 8020 | `document/modes.rs` |
| 方法 | `require_goal_state` | 8033 | `document/modes.rs` |
| 方法 | `commit_goal_transition` | 8050 | `document/modes.rs` |
| 函数 | `validate_goal_definition` | 8492 | `document/modes.rs` |
| 函数 | `goal_context_event` | 9295 | `document/modes.rs` |
| 函数 | `goal_notice_event` | 9312 | `document/modes.rs` |
| 函数 | `goal_state_name` | 9326 | `document/modes.rs` |
| 函数 | `budget_tokens_or_zero` | 10114 | `document/modes.rs` |
| 类型 | `GoalModeTransition` | 745 | `document/modes.rs` |
| 类型 | `PlanModeTransitionInput` | 801 | `document/modes.rs` |
| 类型实现 | `PlanModeTransitionInput` | 828 | `document/modes.rs` |
| 方法 | `build_subagent_return_input` | 5162 | `document/subagent.rs` |
| 方法 | `append_subagent_return_with_stats` | 5278 | `document/subagent.rs` |
| 方法 | `pending_subagent_return_dispatch` | 5362 | `document/subagent.rs` |
| 方法 | `existing_subagent_return` | 7503 | `document/subagent.rs` |
| 函数 | `validate_stat_bucket` | 8378 | `document/subagent.rs` |
| 函数 | `validate_turn_stat_buckets` | 8389 | `document/subagent.rs` |
| 函数 | `validate_subagent_return_pair` | 8404 | `document/subagent.rs` |
| 函数 | `validate_subagent_return_history` | 8438 | `document/subagent.rs` |
| 类型 | `SubagentReturnBuildRequest` | 689 | `document/subagent.rs` |
| 类型 | `SubagentReturnInput` | 704 | `document/subagent.rs` |
| 类型 | `SubagentReturnReceipt` | 718 | `document/subagent.rs` |
| 方法 | `record_compaction` | 5432 | `document/compaction.rs` |
| 方法 | `initialize_runtime_baseline` | 5591 | `document/compaction.rs` |
| 方法 | `validate_runtime_initialization_state` | 6147 | `document/compaction.rs` |
| 方法 | `validate_compaction_state` | 6184 | `document/compaction.rs` |
| 方法 | `set_context_compaction_window_node_id` | 6538 | `document/compaction.rs` |
| 函数 | `validate_provider_projection_proof` | 8514 | `document/compaction.rs` |
| 函数 | `validate_provider_projection_proof_shape` | 8553 | `document/compaction.rs` |
| 类型 | `CompactionCommitInput` | 760 | `document/compaction.rs` |
| 类型实现 | `CompactionCommitInput` | 778 | `document/compaction.rs` |
| 方法 | `set_pending_request` | 6665 | `document/selection.rs` |
| 方法 | `set_output_style` | 6702 | `document/selection.rs` |
| 方法 | `set_session_selection_bundle` | 6731 | `document/selection.rs` |
| 方法 | `set_provider_tool_selection` | 6767 | `document/selection.rs` |
| 方法 | `set_knowledge_library_selection` | 6795 | `document/selection.rs` |
| 方法 | `set_vmm_session_policy` | 6823 | `document/selection.rs` |
| 方法 | `promote_pending_request` | 6848 | `document/selection.rs` |
| 函数 | `normalize_optional_selection` | 9582 | `document/selection.rs` |
| 函数 | `validate_required_selection_component` | 9591 | `document/selection.rs` |
| 函数 | `validate_provider_tool_target_config` | 9603 | `document/selection.rs` |
| 函数 | `validate_provider_tool_capability_selections` | 9615 | `document/selection.rs` |
| 函数 | `validate_provider_media_understanding_selections` | 9638 | `document/selection.rs` |
| 函数 | `validate_provider_extension_target_sets` | 9660 | `document/selection.rs` |
| 函数 | `validate_provider_tool_selection_config` | 9695 | `document/selection.rs` |
| 函数 | `validate_knowledge_library_selection_config` | 9722 | `document/selection.rs` |
| 函数 | `validate_required_request_selection` | 9743 | `document/selection.rs` |
| 函数 | `validate_optional_request_selection` | 9755 | `document/selection.rs` |
| 函数 | `validate_request_temperature` | 9767 | `document/selection.rs` |
| 函数 | `validate_api_request_snapshot` | 9778 | `document/selection.rs` |
| 函数 | `validate_active_request_config` | 9789 | `document/selection.rs` |
| 函数 | `validate_pending_request_config` | 9799 | `document/selection.rs` |
| 函数 | `active_request_from_snapshot` | 9812 | `document/selection.rs` |
| 函数 | `validate_request_against_active_config` | 9824 | `document/selection.rs` |
| 函数 | `pending_request_effectively_equal` | 9957 | `document/selection.rs` |
| 类型 | `SessionSelectionBundleInput` | 813 | `document/selection.rs` |
| 方法 | `rebuild_derived_state` | 5639 | `document/hydration.rs` |
| 方法 | `validate_loaded_metadata` | 6282 | `document/hydration.rs` |
| 方法 | `project_persisted_interruptions` | 6431 | `document/hydration.rs` |
| 函数 | `validate_fork_origin` | 8343 | `document/hydration.rs` |
| 方法 | `apply_published_storage_metadata` | 1056 | `document/persistence.rs` |
| 方法 | `memory_revision_receipt` | 1240 | `document/persistence.rs` |
| 方法 | `advance_durable` | 1288 | `document/persistence.rs` |
| 方法 | `truncate_after` | 6388 | `document/persistence.rs` |
| 方法 | `prepare_flush_candidates` | 6569 | `document/persistence.rs` |
| 方法 | `active_tail_start` | 6626 | `document/persistence.rs` |
| 方法 | `clone_resident_events_at_seqs` | 6635 | `document/persistence.rs` |
| 类型 | `DirtyFile` | 55 | `document/persistence.rs` |
| 类型 | `FlushCandidate` | 861 | `document/persistence.rs` |
| 类型 | `MemoryRevisionReceipt` | 892 | `document/persistence.rs` |
| 函数 | `validate_provider_source_input` | 9338 | `document/provider_source.rs` |
| 函数 | `build_provider_source` | 9389 | `document/provider_source.rs` |
| 函数 | `provider_source_matches_input` | 9410 | `document/provider_source.rs` |
| 函数 | `provider_source_at_seq` | 9422 | `document/provider_source.rs` |
| 函数 | `ensure_provider_source_matches` | 9442 | `document/provider_source.rs` |

## 内部可见性调整

仅下列已有跨边界调用涉及可见性调整；不新增外部公共 API。

| 名称 | 新作用域 | 归属 |
| --- | --- | --- |
| `input_id_already_streamed` | `pub(super)` | input |
| `require_client_action_unused` | `pub(super)` | input |
| `validate_user_input` | `pub(super)` | input |
| `event_client_action_id` | `pub(super)` | input |
| `consume_matching_draft` | `pub(super)` | input |
| `validate_client_action_history` | `pub(super)` | input |
| `validate_queued_input_resolution_history` | `pub(super)` | input |
| `api_transport_was_observed` | `pub(super)` | api |
| `interrupt_active_api_state` | `pub(super)` | api |
| `stage_active_event_interruption` | `pub(super)` | api |
| `validate_active_api_events` | `pub(super)` | api |
| `validate_api_dispatch_history` | `pub(super)` | api |
| `validate_execution_control_event` | `pub(super)` | api |
| `validate_provider_request_budget_evidence` | `pub(super)` | api |
| `validate_api_terminal_facts` | `pub(super)` | api |
| `tool_status_name` | `pub(super)` | tools |
| `validate_tool_lifecycle_shape` | `pub(super)` | tools |
| `stage_turn_closure` | `pub(super)` | turns |
| `aggregate_turn_buckets` | `pub(super)` | turns |
| `aggregate_goal_budget` | `pub(super)` | modes |
| `goal_notice_event` | `pub(super)` | modes |
| `validate_turn_stat_buckets` | `pub(super)` | subagent |
| `validate_subagent_return_history` | `pub(super)` | subagent |
| `validate_runtime_initialization_state` | `pub(super)` | compaction |
| `validate_compaction_state` | `pub(super)` | compaction |
| `validate_provider_projection_proof` | `pub(super)` | compaction |
| `validate_provider_projection_proof_shape` | `pub(super)` | compaction |
| `validate_provider_tool_selection_config` | `pub(in crate::v1)` | selection |
| `validate_knowledge_library_selection_config` | `pub(super)` | selection |
| `validate_api_request_snapshot` | `pub(super)` | selection |
| `validate_active_request_config` | `pub(super)` | selection |
| `validate_pending_request_config` | `pub(super)` | selection |
| `active_request_from_snapshot` | `pub(super)` | selection |
| `validate_request_against_active_config` | `pub(super)` | selection |
| `validate_loaded_metadata` | `pub(super)` | hydration |
| `validate_provider_source_input` | `pub(super)` | provider_source |
| `build_provider_source` | `pub(super)` | provider_source |
| `provider_source_matches_input` | `pub(super)` | provider_source |
| `provider_source_at_seq` | `pub(super)` | provider_source |
| `ensure_provider_source_matches` | `pub(super)` | provider_source |
