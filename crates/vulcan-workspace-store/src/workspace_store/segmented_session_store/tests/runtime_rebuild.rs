use super::*;

/// Builds a canonical assistant call and ToolResult conversation for runtime schema tests.
/// 为运行时结构测试构建规范助手调用与 ToolResult 会话。
fn runtime_tool_result_conversation_state() -> vulcan_context::protocol::SessionConversationState {
    vulcan_context::protocol::SessionConversationState {
        messages: vec![
            vulcan_context::protocol::SessionConversationMessage {
                id: String::from("assistant-1"),
                payload: vulcan_context::protocol::SessionConversationPayload::Assistant {
                    text: String::new(),
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: vec![vulcan_context::protocol::SessionConversationToolCall {
                        id: String::from("call-1"),
                        tool_call_id: Some(String::from("call-1")),
                        tool_name: String::from("demo"),
                        arguments: json!({}),
                    }],
                },
            },
            vulcan_context::protocol::SessionConversationMessage {
                id: String::from("message-result-1"),
                payload: vulcan_context::protocol::SessionConversationPayload::ToolResult {
                    tool_call_id: Some(String::from("call-1")),
                    is_error: false,
                    text: String::from("done"),
                },
            },
        ],
        records: Vec::new(),
        continuation: Default::default(),
    }
}

/// Rewrites one current ToolResult into the deliberately abandoned legacy field shape.
/// 将一条当前 ToolResult 重写为已明确放弃的旧字段形态。
///
/// # Parameters
/// 参数
/// `session_dir` owns an already rebuilt current runtime log.
/// `session_dir` 拥有一份已经重建的当前运行时日志。
/// # Returns
/// 返回值
/// Exact invalid JSONL text after metadata byte length is synchronized.
/// 在元数据字节长度同步后返回精确的无效 JSONL 文本。
fn rewrite_runtime_log_as_schema_fixture(session_dir: &Path) -> String {
    // segment_path is the sole runtime segment declared by current metadata.
    // segment_path 是当前元数据声明的唯一运行时分段。
    let segment_path = session_dir.join("runtime").join("r001.jsonl");
    // line_values retain schema v1 while replacing only the abandoned ToolResult identity field.
    // line_values 保持 schema v1，同时只替换已放弃的 ToolResult 身份字段。
    let mut line_values = read_json_lines::<Value>(&segment_path).expect("read runtime fixture");
    for line_value in &mut line_values {
        // event is the tagged current payload inspected for the exact ToolResult variant.
        // event 是为精确 ToolResult 变体检查的标签当前载荷。
        let event = line_value
            .as_object_mut()
            .and_then(|root| root.get_mut("event"))
            .and_then(Value::as_object_mut)
            .expect("runtime fixture event object");
        if event.get("kind").and_then(Value::as_str) != Some("tool_result") {
            continue;
        }
        // message_id is moved into the removed field to construct an unambiguously invalid row.
        // message_id 被移动到已删除字段中，以构造明确无效的日志行。
        let message_id = event
            .remove("message_id")
            .expect("current ToolResult message id");
        event.insert(String::from("tool_result_id"), message_id);
    }
    write_json_lines_file_atomic(&segment_path, &line_values)
        .expect("write legacy-shape fixture atomically");
    // rewritten_text is retained for non-destructive rejection assertions.
    // rewritten_text 被保留用于非破坏性拒绝断言。
    let rewritten_text = fs::read_to_string(&segment_path).expect("read rewritten fixture");
    // metadata must describe the exact fixture bytes so only payload validation is under test.
    // metadata 必须描述精确夹具字节，使测试只关注载荷验证。
    let mut metadata = read_runtime_metadata(session_dir).expect("read runtime metadata");
    // segment is unique under the validated single-segment runtime contract.
    // segment 在已验证单分段运行时契约下具有唯一性。
    let segment = metadata
        .segments
        .first_mut()
        .expect("runtime metadata segment");
    segment.byte_len = u64::try_from(rewritten_text.len()).expect("fixture byte length");
    write_runtime_metadata(session_dir, &metadata).expect("write fixture metadata");
    rewritten_text
}

/// Verifies runtime JSONL can be rebuilt from canonical conversation state and read back.
/// 验证 runtime JSONL 可从规范会话状态重建并读取还原。
#[test]
fn runtime_log_rebuilds_and_restores_conversation_state() {
    let root = test_root("runtime_log_rebuilds_and_restores_conversation_state");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    let messages = vec![
        test_message(
            "m-user-1",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "first prompt",
        ),
        test_message(
            "m-assistant-1",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "first answer",
        ),
    ];
    let conversation_state = conversation_state_from_session_messages(&messages)
        .expect("valid replayed conversation identities");

    let metadata = rebuild_runtime_log_from_conversation_state(
        &session_dir,
        &conversation_state,
        "2026-06-09T10:00:00Z",
    )
    .expect("rebuild runtime log");
    let restored = read_runtime_conversation_state(&session_dir)
        .expect("read runtime conversation")
        .expect("runtime conversation exists");

    assert_eq!(metadata.active_segment, "runtime/r001.jsonl");
    assert!(session_dir.join(SEGMENTED_RUNTIME_METADATA_FILE).exists());
    assert!(session_dir.join(SEGMENTED_RUNTIME_INDEX_FILE).exists());
    assert!(session_dir.join("runtime").join("r001.jsonl").exists());
    assert_eq!(restored.messages.len(), conversation_state.messages.len());
}

/// Verifies the runtime message index keys tool results by local protocol-message identity.
/// 验证运行时消息索引会按本地协议消息身份索引工具结果。
#[test]
fn runtime_log_index_preserves_tool_result_message_identifier() {
    // session_dir owns an isolated runtime-log persistence transaction.
    // session_dir 拥有隔离的运行时日志持久化事务。
    let root = test_root("runtime_log_index_preserves_tool_result_message_identifier");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    // conversation_state gives the result one local entity id and one separate call relationship.
    // conversation_state 为结果提供一个本地实体标识与一个独立调用关系。
    let conversation_state = runtime_tool_result_conversation_state();

    rebuild_runtime_log_from_conversation_state(
        &session_dir,
        &conversation_state,
        "2026-07-11T12:00:00Z",
    )
    .expect("rebuild runtime log");
    // index is the persisted lookup contract consumed independently from JSONL replay.
    // index 是独立于 JSONL 重放被消费的持久化查找契约。
    let index = read_required_json::<RuntimeLogIndex>(&runtime_index_path(&session_dir))
        .expect("read runtime index");
    // restored proves clean v1 preserves the complete call/result conversation during replay.
    // restored 证明干净 v1 会在重放中保留完整调用/结果会话。
    let restored = read_runtime_conversation_state(&session_dir)
        .expect("read clean v1 runtime conversation")
        .expect("runtime conversation exists");
    // persisted_values expose the exact clean v1 JSONL contract written to disk.
    // persisted_values 暴露写入磁盘的精确干净 v1 JSONL 契约。
    let persisted_values =
        read_json_lines::<Value>(&session_dir.join("runtime").join("r001.jsonl"))
            .expect("read clean v1 runtime lines");
    // result_event is the persisted ToolResult whose identity must have one owner.
    // result_event 是持久化的 ToolResult，其身份必须只有一个所有者。
    let result_event = persisted_values
        .iter()
        .filter_map(|line| line.get("event"))
        .find(|event| event.get("kind").and_then(Value::as_str) == Some("tool_result"))
        .expect("clean v1 ToolResult event");

    assert_eq!(RUNTIME_LOG_SCHEMA_VERSION, 1);
    assert!(index.message_offsets.contains_key("assistant-1"));
    assert!(index.message_offsets.contains_key("message-result-1"));
    assert_eq!(index.message_offsets.len(), 2);
    assert_eq!(restored.messages.len(), 2);
    assert!(
        persisted_values
            .iter()
            .all(|line| { line.get("schema_version").and_then(Value::as_u64) == Some(1) })
    );
    assert_eq!(
        result_event.get("message_id").and_then(Value::as_str),
        Some("message-result-1")
    );
    assert!(result_event.get("tool_result_id").is_none());
}

/// Verifies the abandoned ToolResult shape is rejected without source mutation.
/// 验证已放弃 ToolResult 形态会被拒绝且源文件不被修改。
#[test]
fn runtime_log_read_rejects_legacy_v1_tool_result_shape_without_rewriting_source() {
    // root isolates the deliberately invalid legacy ToolResult fixture.
    // root 隔离故意构造的无效旧 ToolResult 夹具。
    let root = test_root("runtime_log_read_rejects_legacy_tool_result_shape");
    // session_dir owns the source whose exact bytes must remain unchanged on rejection.
    // session_dir 拥有拒绝时必须保持精确字节不变的源。
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create legacy-shape session");
    // conversation_state supplies a complete current v1 ToolResult before invalid mutation.
    // conversation_state 在无效变更前提供完整当前 v1 ToolResult。
    let conversation_state = runtime_tool_result_conversation_state();
    rebuild_runtime_log_from_conversation_state(
        &session_dir,
        &conversation_state,
        "2026-07-11T12:00:00Z",
    )
    .expect("build current v1 runtime fixture");
    // original_segment captures the exact abandoned field shape before the failed read.
    // original_segment 捕获失败读取前已放弃字段形态的精确字节。
    let original_segment = rewrite_runtime_log_as_schema_fixture(&session_dir);

    // error must be a hard payload failure rather than migration or corrupt-suffix repair.
    // error 必须是载荷硬错误，而不是迁移或损坏尾部修复。
    let error =
        read_runtime_log_lines(&session_dir).expect_err("legacy ToolResult shape must fail");
    // persisted_segment proves strict rejection is non-destructive.
    // persisted_segment 证明严格拒绝不会破坏源文件。
    let persisted_segment = fs::read_to_string(session_dir.join("runtime").join("r001.jsonl"))
        .expect("read retained legacy-shape segment");

    assert!(error.contains("Invalid runtime log v1 payload"));
    assert_eq!(persisted_segment, original_segment);
}

/// Verifies unsupported schemas remain visible and never trigger destructive rewrite.
/// 验证不受支持结构保持可观察且绝不会触发破坏性重写。
#[test]
fn runtime_log_read_rejects_unknown_schema_without_rewriting_source() {
    // root isolates the unsupported-schema fixture.
    // root 隔离不受支持结构夹具。
    let root = test_root("runtime_log_read_rejects_unknown_schema");
    // session_dir owns the source whose exact bytes must remain unchanged on failure.
    // session_dir 拥有失败时必须保持精确字节不变的源。
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create unsupported-schema session");
    // conversation_state supplies a valid current log before only its version selector is changed.
    // conversation_state 在仅修改版本选择器前提供有效当前日志。
    let conversation_state = runtime_tool_result_conversation_state();
    rebuild_runtime_log_from_conversation_state(
        &session_dir,
        &conversation_state,
        "2026-07-11T12:00:00Z",
    )
    .expect("build current runtime fixture");
    // unsupported_version is strictly outside the single supported clean v1 schema.
    // unsupported_version 严格位于唯一受支持干净 v1 结构之外。
    let unsupported_version = RUNTIME_LOG_SCHEMA_VERSION + 1;
    // segment_path identifies the source whose selector is changed without payload mutation.
    // segment_path 标识只修改选择器而不修改载荷的源。
    let segment_path = session_dir.join("runtime").join("r001.jsonl");
    // line_values preserve every event while assigning the unsupported selector.
    // line_values 在分配不受支持选择器时保留全部事件。
    let mut line_values =
        read_json_lines::<Value>(&segment_path).expect("read current runtime fixture");
    for line_value in &mut line_values {
        // root owns the line-level schema selector.
        // root 持有日志行级结构选择器。
        let root = line_value
            .as_object_mut()
            .expect("runtime fixture line object");
        root.insert(
            String::from("schema_version"),
            Value::from(unsupported_version),
        );
    }
    write_json_lines_file_atomic(&segment_path, &line_values)
        .expect("write unsupported-schema fixture");
    // original_segment captures the exact unsupported source bytes before the failed read.
    // original_segment 捕获失败读取前不受支持源的精确字节。
    let original_segment = fs::read_to_string(&segment_path).expect("read unsupported fixture");
    // metadata must match fixture bytes so only version validation is under test.
    // metadata 必须匹配夹具字节，使测试只关注版本验证。
    let mut metadata = read_runtime_metadata(&session_dir).expect("read runtime metadata");
    // segment is unique under the validated single-segment runtime contract.
    // segment 在已验证单分段运行时契约下具有唯一性。
    let segment = metadata
        .segments
        .first_mut()
        .expect("runtime metadata segment");
    segment.byte_len = u64::try_from(original_segment.len()).expect("fixture byte length");
    write_runtime_metadata(&session_dir, &metadata).expect("write fixture metadata");

    // error must remain a version failure rather than a corrupt-suffix repair.
    // error 必须保持为版本错误，而不是损坏尾部修复。
    let error =
        read_runtime_log_lines(&session_dir).expect_err("unsupported runtime schema must fail");
    // persisted_segment proves no automatic rewrite occurred for an unknown schema.
    // persisted_segment 证明未知结构不会触发自动重写。
    let persisted_segment =
        fs::read_to_string(&segment_path).expect("read retained unsupported segment");

    assert!(error.contains(&format!(
        "Unsupported runtime log schema version {unsupported_version}"
    )));
    assert_eq!(persisted_segment, original_segment);
}

/// Verifies structurally invalid runtime relationships remain observable after storage validation.
/// 验证结构无效的运行时关系在存储校验后仍保持可观察。
#[test]
fn runtime_log_read_rejects_tool_call_with_missing_assistant() {
    // root owns the isolated semantic-corruption fixture.
    // root 拥有隔离的语义损坏测试夹具。
    let root = test_root("runtime_log_read_rejects_tool_call_with_missing_assistant");
    // session_dir owns the otherwise valid runtime artifacts.
    // session_dir 拥有其他方面均有效的运行时工件。
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    // state creates one assistant and one valid tool call through the production projection path.
    // state 通过生产投影路径创建一个助手和一个有效工具调用。
    let state = vulcan_context::protocol::SessionConversationState {
        messages: vec![vulcan_context::protocol::SessionConversationMessage {
            id: String::from("assistant-1"),
            payload: vulcan_context::protocol::SessionConversationPayload::Assistant {
                text: String::new(),
                reasoning: String::new(),
                provider_message_id: None,
                provider_message_state: None,
                usage: None,
                tool_calls: vec![vulcan_context::protocol::SessionConversationToolCall {
                    id: String::from("call-a"),
                    tool_call_id: Some(String::from("call-a")),
                    tool_name: String::from("demo"),
                    arguments: json!({}),
                }],
            },
        }],
        records: Vec::new(),
        continuation: Default::default(),
    };
    rebuild_runtime_log_from_conversation_state(&session_dir, &state, "t1")
        .expect("rebuild valid runtime log");
    // lines are modified only at the semantic owner reference while sequence/schema remain valid.
    // lines 仅修改语义所属引用，同时保持序号与结构版本有效。
    let mut lines = read_runtime_log_lines(&session_dir)
        .expect("read valid runtime lines")
        .expect("runtime lines exist");
    let tool_call = lines
        .iter_mut()
        .find_map(|line| match &mut line.event {
            RuntimeLogEvent::ToolCall(event) => Some(event),
            _ => None,
        })
        .expect("runtime tool call");
    tool_call.assistant_message_id = String::from("assistant-missing");
    // segment_path receives a fully encoded semantically invalid but physically valid document.
    // segment_path 接收完整编码、语义无效但物理有效的文档。
    let segment_path = session_dir.join(runtime_segment_file_name());
    write_json_lines_file_atomic(&segment_path, &lines).expect("write semantic corruption");
    // metadata is refreshed so physical validation succeeds and replay owns the semantic diagnosis.
    // metadata 会被刷新，使物理校验成功并由重放层负责语义诊断。
    let mut metadata = read_runtime_metadata(&session_dir).expect("read runtime metadata");
    metadata.segments[0].byte_len = fs::metadata(&segment_path)
        .expect("read corrupted segment metadata")
        .len();
    write_runtime_metadata(&session_dir, &metadata).expect("refresh runtime metadata");
    write_runtime_index(&session_dir, &rebuild_runtime_log_index(&lines))
        .expect("refresh runtime index");

    // error proves the store reader propagates replay failure rather than dropping the call.
    // error 证明存储读取器会传播重放失败，而不是丢弃该调用。
    let error = read_runtime_conversation_state(&session_dir)
        .expect_err("missing tool-call assistant must fail runtime replay");

    assert!(
        error.contains("references missing assistant message `assistant-missing`"),
        "unexpected error: {error}"
    );
}

/// Verifies a physically consistent but false batch completion summary fails semantic replay.
/// 验证物理一致但内容虚假的批次完成汇总会在语义重放时失败。
#[test]
fn runtime_log_read_rejects_incorrect_tool_batch_completion() {
    // root owns the isolated batch-summary corruption fixture.
    // root 拥有隔离的批次汇总损坏测试夹具。
    let root = test_root("runtime_log_read_rejects_incorrect_tool_batch_completion");
    // session_dir owns the valid runtime projection that will be semantically altered.
    // session_dir 拥有随后会被语义修改的有效运行时投影。
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    // state creates one completed call whose production summary must declare one call and one result.
    // state 创建一个已完成调用，其生产汇总必须声明一个调用和一个结果。
    let state = vulcan_context::protocol::SessionConversationState {
        messages: vec![
            vulcan_context::protocol::SessionConversationMessage {
                id: String::from("assistant-1"),
                payload: vulcan_context::protocol::SessionConversationPayload::Assistant {
                    text: String::new(),
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: vec![vulcan_context::protocol::SessionConversationToolCall {
                        id: String::from("call-a"),
                        tool_call_id: Some(String::from("call-a")),
                        tool_name: String::from("demo"),
                        arguments: json!({}),
                    }],
                },
            },
            vulcan_context::protocol::SessionConversationMessage {
                id: String::from("result-a"),
                payload: vulcan_context::protocol::SessionConversationPayload::ToolResult {
                    tool_call_id: Some(String::from("call-a")),
                    is_error: false,
                    text: String::from("done"),
                },
            },
        ],
        records: Vec::new(),
        continuation: Default::default(),
    };
    rebuild_runtime_log_from_conversation_state(&session_dir, &state, "t1")
        .expect("rebuild valid runtime log");
    // lines retain valid schema and sequence while only the completion count is falsified.
    // lines 保持有效结构版本与序号，仅篡改完成事件计数。
    let mut lines = read_runtime_log_lines(&session_dir)
        .expect("read valid runtime lines")
        .expect("runtime lines exist");
    let completion = lines
        .iter_mut()
        .find_map(|line| match &mut line.event {
            RuntimeLogEvent::ToolBatchCompleted(event) => Some(event),
            _ => None,
        })
        .expect("runtime tool-batch completion");
    completion.tool_call_count = 2;
    // segment_path receives a complete semantically false document.
    // segment_path 接收完整但语义虚假的文档。
    let segment_path = session_dir.join(runtime_segment_file_name());
    write_json_lines_file_atomic(&segment_path, &lines).expect("write incorrect batch completion");
    // metadata and index are refreshed so replay exclusively owns the semantic diagnosis.
    // metadata 与 index 会被刷新，使语义诊断完全由重放层负责。
    let mut metadata = read_runtime_metadata(&session_dir).expect("read runtime metadata");
    metadata.segments[0].byte_len = fs::metadata(&segment_path)
        .expect("read corrupted segment metadata")
        .len();
    write_runtime_metadata(&session_dir, &metadata).expect("refresh runtime metadata");
    write_runtime_index(&session_dir, &rebuild_runtime_log_index(&lines))
        .expect("refresh runtime index");

    // error proves physical consistency cannot authorize a false control-event summary.
    // error 证明物理一致性不能授权虚假的控制事件汇总。
    let error = read_runtime_conversation_state(&session_dir)
        .expect_err("incorrect batch completion must fail runtime replay");

    assert!(
        error.contains("summary mismatch: calls expected 1, found 2"),
        "unexpected error: {error}"
    );
}

/// Verifies missing runtime logs are not rebuilt from display-only segments.
/// 验证缺失的 runtime 日志不会从纯展示分段重建。
#[test]
fn runtime_log_missing_does_not_rebuild_from_segmented_display_segments() {
    let root = test_root("runtime_log_missing_does_not_rebuild_from_segmented_display_segments");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    let messages = vec![
        test_message(
            "m-user-1",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "first prompt",
        ),
        test_message(
            "m-assistant-1",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "first answer",
        ),
    ];
    append_segmented_session_message_update(&session_dir, &messages).expect("append messages");

    let restored =
        ensure_runtime_log_from_segmented_context(&session_dir).expect("ensure runtime log");

    assert!(restored.is_none());
    assert!(!session_dir.join(SEGMENTED_RUNTIME_METADATA_FILE).exists());
}

/// Verifies missing runtime logs are not synthesized from rolled display segments.
/// 验证缺失的 runtime 日志不会从滚动后的展示分段合成。
#[test]
fn runtime_log_missing_ignores_non_initial_display_segments() {
    let root = test_root("runtime_log_missing_ignores_non_initial_display_segments");
    let policy = SegmentPolicy {
        max_lines: 2,
        ..SegmentPolicy::default()
    };
    let session_dir =
        create_segmented_session_in_location(&root, false, test_session_request(policy))
            .expect("create session");
    let messages = vec![
        test_message(
            "m-user-1",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "first prompt",
        ),
        test_message(
            "m-assistant-1",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "first answer",
        ),
        test_message(
            "m-user-2",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "second prompt",
        ),
        test_message(
            "m-assistant-2",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "second answer",
        ),
        test_message(
            "m-user-3",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "third prompt",
        ),
    ];
    append_segmented_session_message_update(&session_dir, &messages).expect("append messages");
    let metadata = read_session_metadata(&session_dir).expect("read metadata");

    assert_eq!(metadata.active_segment, "s003.jsonl");
    assert_eq!(metadata.segments.len(), 3);

    let restored =
        ensure_runtime_log_from_segmented_context(&session_dir).expect("ensure runtime log");

    assert!(restored.is_none());
    assert!(!session_dir.join("runtime").join("r001.jsonl").exists());
}

/// Verifies corrupt runtime metadata is surfaced and never treated as an absent rebuild trigger.
/// 验证损坏的运行时元数据会被上报，且绝不会被当作缺失后的重建触发器。
#[test]
fn runtime_log_corrupt_metadata_remains_observable_across_entrypoints() {
    // root isolates the corrupt metadata scenario from other runtime-log tests.
    // root 将损坏元数据场景与其他运行时日志测试隔离。
    let root = test_root("runtime_log_corrupt_metadata_remains_observable");
    // session_dir owns both the valid initial runtime log and the later damaged metadata.
    // session_dir 同时持有初始有效运行时日志与后续损坏元数据。
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    // messages provide a canonical conversation that could otherwise overwrite the damage.
    // messages 提供一个可能覆盖损坏状态的规范会话。
    let messages = vec![test_message(
        "m-user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "first prompt",
    )];
    // conversation_state is the rebuild input used to verify sync does not silently replace evidence.
    // conversation_state 是用于验证同步不会静默替换证据的重建输入。
    let conversation_state = conversation_state_from_session_messages(&messages)
        .expect("valid replayed conversation identities");
    rebuild_runtime_log_from_conversation_state(
        &session_dir,
        &conversation_state,
        "2026-06-09T10:00:00Z",
    )
    .expect("build initial runtime log");
    // metadata_path identifies the authoritative runtime metadata document.
    // metadata_path 标识权威运行时元数据文档。
    let metadata_path = runtime_metadata_path(&session_dir);
    // corrupt_metadata is retained verbatim to prove failed reads do not trigger replacement.
    // corrupt_metadata 会保持原样，用于证明读取失败不会触发替换。
    let corrupt_metadata = "{ broken runtime metadata\n";
    fs::write(&metadata_path, corrupt_metadata).expect("damage runtime metadata");

    // ensure_error captures the strict read failure from the hydration path.
    // ensure_error 捕获 hydration 路径返回的严格读取失败。
    let ensure_error = ensure_runtime_log_from_segmented_context(&session_dir)
        .expect_err("corrupt runtime metadata must fail ensure");
    // sync_error proves canonical state synchronization does not rebuild over corrupt metadata.
    // sync_error 证明规范状态同步不会覆盖损坏元数据并重建。
    let sync_error = sync_runtime_log_from_conversation_state(
        &session_dir,
        &conversation_state,
        "2026-06-09T10:01:00Z",
    )
    .expect_err("corrupt runtime metadata must fail sync");
    // compacted_sync_error covers the independent post-compaction synchronization entrypoint.
    // compacted_sync_error 覆盖独立的压缩后同步入口。
    let compacted_sync_error = sync_runtime_log_from_compacted_conversation_state(
        &session_dir,
        &conversation_state,
        "2026-06-09T10:02:00Z",
        "compaction-corrupt-metadata",
    )
    .expect_err("corrupt runtime metadata must fail compacted sync");
    // audit_error proves best-effort callers still receive persisted integrity failures.
    // audit_error 证明尽力而为调用方仍会收到已持久化完整性失败。
    let audit_error = record_runtime_audit_event(
        &session_dir,
        "2026-06-09T10:03:00Z",
        RuntimeLogEvent::SystemUpdate(RuntimeSystemUpdateEvent {
            turn_id: Some(String::from("corrupt-metadata-turn")),
            update_id: String::from("corrupt-metadata-audit"),
            effective_after_event_id: None,
            text: String::from("must not append"),
            trust: RuntimeSystemUpdateTrust::TrustedHost,
            lowering_hint: RuntimeSystemUpdateLoweringHint::Auto,
            fingerprint: Some(String::from("corrupt-metadata-audit-fingerprint")),
            source: Some(String::from("runtime-rebuild-test")),
        }),
    )
    .expect_err("corrupt runtime metadata must fail audit append");
    // persisted_metadata captures the document after every failed operation.
    // persisted_metadata 捕获全部失败操作后的文档内容。
    let persisted_metadata = fs::read_to_string(&metadata_path).expect("read corrupt metadata");

    assert!(ensure_error.contains("解析 JSON 文件失败"));
    assert!(sync_error.contains("解析 JSON 文件失败"));
    assert!(compacted_sync_error.contains("解析 JSON 文件失败"));
    assert!(audit_error.contains("解析 JSON 文件失败"));
    assert_eq!(persisted_metadata, corrupt_metadata);
}

/// Verifies orphaned runtime artifacts prevent metadata-absence rebuilds.
/// 验证孤儿运行时产物会阻止按元数据缺失执行重建。
#[test]
fn runtime_log_orphaned_artifacts_remain_observable_when_metadata_is_missing() {
    // root isolates the interrupted-persistence layout.
    // root 隔离持久化中断布局。
    let root = test_root("runtime_log_orphaned_artifacts_remain_observable");
    // session_dir owns the runtime artifacts whose metadata will be removed.
    // session_dir 持有元数据即将被删除的运行时产物。
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    // messages provide canonical state that must not overwrite orphaned evidence.
    // messages 提供不得覆盖孤儿证据的规范状态。
    let messages = vec![test_message(
        "m-user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "first prompt",
    )];
    // conversation_state initializes a complete runtime log before the interruption.
    // conversation_state 在中断前初始化完整运行时日志。
    let conversation_state = conversation_state_from_session_messages(&messages)
        .expect("valid replayed conversation identities");
    sync_runtime_log_from_conversation_state(&session_dir, &conversation_state, "t1")
        .expect("sync initial runtime log");
    // metadata identifies the active segment retained after metadata deletion.
    // metadata 标识删除元数据后仍保留的活动分段。
    let metadata = read_runtime_metadata(&session_dir).expect("read runtime metadata");
    // metadata_path is removed to simulate an interrupted multi-file publication.
    // metadata_path 会被删除，以模拟多文件发布中断。
    let metadata_path = runtime_metadata_path(&session_dir);
    // active_segment_path retains authoritative runtime bytes without metadata.
    // active_segment_path 在缺少元数据时保留权威运行时字节。
    let active_segment_path = session_dir.join(&metadata.active_segment);
    // original_segment captures evidence that both failed operations must preserve.
    // original_segment 捕获两次失败操作都必须保留的证据。
    let original_segment = fs::read_to_string(&active_segment_path).expect("read runtime segment");
    // index_path identifies the contradictory rebuildable artifact retained without metadata.
    // index_path 标识缺少元数据时保留的矛盾可重建产物。
    let index_path = runtime_index_path(&session_dir);
    // original_index captures exact sidecar bytes that failed classification must preserve.
    // original_index 捕获失败分类必须保留的精确附属索引字节。
    let original_index = fs::read(&index_path).expect("read runtime index");
    fs::remove_file(&metadata_path).expect("remove runtime metadata");

    // ensure_error proves hydration rejects the inconsistent artifact set.
    // ensure_error 证明 hydration 会拒绝不一致的产物集合。
    let ensure_error = ensure_runtime_log_from_segmented_context(&session_dir)
        .expect_err("orphaned runtime artifacts must fail ensure");
    // sync_error proves canonical synchronization cannot rebuild over orphaned evidence.
    // sync_error 证明规范同步不能覆盖孤儿证据并重建。
    let sync_error =
        sync_runtime_log_from_conversation_state(&session_dir, &conversation_state, "t2")
            .expect_err("orphaned runtime artifacts must fail sync");
    // persisted_segment verifies the failed classification is non-destructive.
    // persisted_segment 验证失败分类不会破坏原文件。
    let persisted_segment =
        fs::read_to_string(&active_segment_path).expect("read retained segment");
    // persisted_index verifies neither failed entrypoint refreshed the rebuildable sidecar.
    // persisted_index 验证两个失败入口都没有刷新可重建附属索引。
    let persisted_index = fs::read(&index_path).expect("read retained runtime index");
    // metadata_absence distinguishes true NotFound from metadata inspection failures.
    // metadata_absence 区分真正的 NotFound 与元数据检查失败。
    let metadata_absence = fs::symlink_metadata(&metadata_path);

    assert!(ensure_error.contains("Runtime metadata is missing"));
    assert!(sync_error.contains("Runtime metadata is missing"));
    assert_eq!(persisted_segment, original_segment);
    assert_eq!(persisted_index, original_index);
    assert!(matches!(
        metadata_absence,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    ));
}

/// Verifies a corrupt runtime JSONL row is backed up and removed without losing valid rows.
/// 验证损坏 runtime JSONL 行会被备份并移除，同时不丢失有效行。
#[test]
fn runtime_log_tolerates_and_repairs_corrupt_jsonl_tail() {
    let root = test_root("runtime_log_tolerates_and_repairs_corrupt_jsonl_tail");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    let messages = vec![
        test_message(
            "m-user-1",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "first prompt",
        ),
        test_message(
            "m-assistant-1",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "first answer",
        ),
    ];
    let conversation_state = conversation_state_from_session_messages(&messages)
        .expect("valid replayed conversation identities");
    rebuild_runtime_log_from_conversation_state(
        &session_dir,
        &conversation_state,
        "2026-06-09T10:00:00Z",
    )
    .expect("rebuild runtime log");
    let runtime_segment_path = session_dir.join("runtime").join("r001.jsonl");
    {
        let mut file = OpenOptions::new()
            .append(true)
            .open(&runtime_segment_path)
            .expect("open runtime segment");
        writeln!(file, "{{ broken").expect("append corrupt line");
    }

    let lines = repair_runtime_log_lines(&session_dir)
        .expect("read repaired runtime lines")
        .expect("runtime lines exist");
    let cleaned = read_json_lines::<RuntimeLogEventLine>(&runtime_segment_path)
        .expect("read cleaned runtime segment");
    let corrupt_backups = fs::read_dir(session_dir.join("runtime"))
        .expect("read runtime dir")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains(".corrupt-"))
        .count();

    assert_eq!(lines.len(), cleaned.len());
    assert_eq!(lines.len(), 3);
    let repair_event = match lines.last().map(|line| &line.event) {
        Some(RuntimeLogEvent::RuntimeRepair(event)) => event,
        _ => panic!("expected runtime repair event"),
    };
    assert!(repair_event.detail.contains("runtime/r001.jsonl lines 3"));
    assert_eq!(corrupt_backups, 1);
    assert!(session_dir.join(SEGMENTED_RUNTIME_INDEX_FILE).exists());
}

/// Verifies a corrupt middle row never permits later valid runtime events to be stitched together.
/// 验证损坏的中间行绝不会允许拼接其后的有效运行时事件。
#[test]
fn runtime_log_rejects_internal_corruption_without_rewriting_source() {
    // root isolates the internal-corruption fixture.
    // root 隔离内部损坏夹具。
    let root = test_root("runtime_log_rejects_internal_corruption");
    // session_dir owns a two-line runtime log used to create a valid-corrupt-valid sequence.
    // session_dir 持有用于构造有效-损坏-有效序列的两行运行时日志。
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    // messages produce two independently valid runtime rows.
    // messages 生成两条彼此独立的有效运行时记录。
    let messages = vec![
        test_message(
            "m-user-1",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "first prompt",
        ),
        test_message(
            "m-assistant-1",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "first answer",
        ),
    ];
    // conversation_state initializes the valid runtime source.
    // conversation_state 初始化有效运行时源文件。
    let conversation_state = conversation_state_from_session_messages(&messages)
        .expect("valid replayed conversation identities");
    rebuild_runtime_log_from_conversation_state(
        &session_dir,
        &conversation_state,
        "2026-06-09T10:00:00Z",
    )
    .expect("build initial runtime log");
    // segment_path identifies the active runtime JSONL file.
    // segment_path 标识活动运行时 JSONL 文件。
    let segment_path = session_dir.join("runtime").join("r001.jsonl");
    // valid_rows retain the two original JSON rows around the inserted corruption.
    // valid_rows 保留插入损坏行前后的两条原始 JSON 记录。
    let valid_rows = fs::read_to_string(&segment_path)
        .expect("read runtime segment")
        .lines()
        .map(str::to_string)
        .collect::<Vec<_>>();
    assert_eq!(valid_rows.len(), 2);
    // damaged_segment inserts an invalid middle row without changing either valid row.
    // damaged_segment 插入一条无效中间行，同时不改变两条有效记录。
    let damaged_segment = format!("{}\n{{ broken\n{}\n", valid_rows[0], valid_rows[1]);
    // decode_error proves the immutable byte decoder stops at the exact corrupt physical row.
    // decode_error 证明不可变字节解码器会在精确的损坏物理行处停止。
    let decode_error =
        decode_runtime_segment_lines_strict(&segment_path, damaged_segment.as_bytes())
            .expect_err("strict decoder must reject the corrupt middle row");
    assert!(decode_error.contains("line 2"));
    assert!(decode_error.contains("Failed to decode runtime JSONL segment"));
    fs::write(&segment_path, &damaged_segment).expect("write internally corrupt segment");

    // error proves the ordinary reader rejects a changed durable byte boundary before hydration.
    // error 证明普通读取器会在水合前拒绝发生变化的耐久字节边界。
    let error = read_runtime_log_lines(&session_dir)
        .expect_err("internal runtime corruption must fail without repair");
    // persisted_segment proves no prefix rewrite occurred.
    // persisted_segment 证明没有发生前缀重写。
    let persisted_segment = fs::read_to_string(&segment_path).expect("read retained segment");
    // corrupt_backups proves validation failed before any repair publication.
    // corrupt_backups 证明验证在任何修复发布前失败。
    let corrupt_backups = fs::read_dir(session_dir.join("runtime"))
        .expect("read runtime directory")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains(".corrupt-"))
        .count();

    assert!(error.contains("Runtime segment byte length mismatch"));
    assert_eq!(persisted_segment, damaged_segment);
    assert_eq!(corrupt_backups, 0);
}

/// Verifies fully damaged runtime logs are not regenerated from display transcript segments.
/// 验证完全损坏的 runtime 日志不会从展示转录分段重新生成。
#[test]
fn runtime_log_full_corruption_does_not_fall_back_to_segmented_replay() {
    let root = test_root("runtime_log_full_corruption_does_not_fall_back_to_segmented_replay");
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        test_session_request(SegmentPolicy::default()),
    )
    .expect("create session");
    let messages = vec![
        test_message(
            "m-user-1",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "first prompt",
        ),
        test_message(
            "m-assistant-1",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "first answer",
        ),
    ];
    append_segmented_session_message_update(&session_dir, &messages).expect("append messages");
    let conversation_state = conversation_state_from_session_messages(&messages)
        .expect("valid replayed conversation identities");
    rebuild_runtime_log_from_conversation_state(
        &session_dir,
        &conversation_state,
        "2026-06-09T10:00:00Z",
    )
    .expect("rebuild runtime log");
    fs::write(session_dir.join("runtime").join("r001.jsonl"), "{ broken\n")
        .expect("damage runtime segment");

    // first_error captures the first strict durable-boundary diagnosis.
    // first_error 捕获第一次严格的耐久边界诊断。
    let first_error = ensure_runtime_log_from_segmented_context(&session_dir)
        .expect_err("fully damaged runtime log must remain observable");
    // second_error proves the original corruption remains observable on repeated reads.
    // second_error 证明原始损坏在重复读取时仍保持可观察。
    let second_error = read_runtime_log_lines(&session_dir)
        .expect_err("fully damaged runtime log must fail every read");
    // persisted_segment proves the repair planner did not rewrite the only evidence.
    // persisted_segment 证明修复计划器没有重写唯一证据。
    let persisted_segment = fs::read_to_string(session_dir.join("runtime").join("r001.jsonl"))
        .expect("read damaged runtime segment");

    assert!(first_error.contains("Runtime segment byte length mismatch"));
    assert_eq!(second_error, first_error);
    assert_eq!(persisted_segment, "{ broken\n");
}
