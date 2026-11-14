//! Regression coverage for the authoritative V1 native-tool media rebuild and actual wire encoding.
//! 权威V1原生工具媒体重建与实际线上编码的回归覆盖。

use super::*;

/// Native read fixture: frozen media `context`, request `facts`, route-aligned `candidate`, its V1 `snapshot` and Base64 body.
/// 原生读取夹具：冻结媒体`context`、请求`facts`、路线对齐`candidate`、其V1`snapshot`及Base64正文。
type NativeReadFixture = (
    vulcan_agent_runtime::agent_runtime::events::ProviderRequestMediaContext,
    ProviderV1RequestFacts,
    ProviderRequestPreparationCandidate,
    SessionProviderProjectionSnapshot,
    String,
    tempfile::TempDir,
);

/// Builds one completed `read_media` call and result over the real isolated route; no endpoint is called.
/// 在真实隔离路线上构建一次已完成的`read_media`调用及结果；不调用任何端点。
async fn native_read_fixture() -> Result<NativeReadFixture, Box<dyn std::error::Error>> {
    // Reuse the real isolated route and credential fixture; no inference endpoint is called.
    // 复用真实隔离路线及凭据夹具；不调用推理端点。
    let context = media::tests::native_input_context().await?;
    // The canonical baseline comes from the established production reconstruction fixture.
    // 规范基线来自既有生产重建夹具。
    let (directory, handle) = published_stored_open_turn();
    // Replace all route facts together so V1 validates one exact frozen execution.
    // 一起替换全部路线事实，使V1校验同一精确冻结执行。
    let mut candidate = host_guidance::candidate_for(&handle);
    // Request facts come from the same actual execution as media projection.
    // 请求事实与媒体投影来自同一实际执行。
    let mut facts = request_facts();
    facts.request.provider = context.execution.route.provider.id.clone();
    facts.request.model = context.execution.model.id.clone();
    facts.request.profile = Some(
        context
            .execution
            .route
            .protocol_route
            .protocol_profile_id
            .clone(),
    );
    facts.request.connection_id = Some(context.execution.route.connection.id.clone());
    candidate.exact_request.model_id = facts.request.model.clone();
    candidate.exact_request.cache_scope_id = Some(context.session_scope_id.clone());
    candidate.exact_request.output_token_limit = Some(1024);
    candidate.request_snapshot.provider_id = facts.request.provider.clone();
    candidate.request_snapshot.model_id = facts.request.model.clone();
    candidate.request_snapshot.temperature = facts.request.temperature;
    candidate.controller_config.provider_id = facts.request.provider.clone();
    candidate.controller_config.model_id = facts.request.model.clone();
    candidate.media_context = Some(context.clone());
    candidate.request_budget_guard = Some(ProviderRequestBudgetGuard::new_v1(
        facts.request.provider.clone(),
        facts.request.model.clone(),
        context
            .execution
            .route
            .protocol_route
            .protocol_profile_id
            .clone(),
        context.execution.frozen_route_revision().to_string(),
        ModelCapabilities::default(),
        ProviderRuntimeReasoningResolution::default(),
        None,
        None,
        None,
    ));
    // Large enough to detect accidental Base64-as-text budget accounting while remaining well below media limits.
    // 大小足以检测Base64误按文本记账，同时明显低于媒体限制。
    let base64 = "YWJj".repeat(20_000);
    // Persist the exact documented host-owned model-output envelope.
    // 持久化精确已定义的宿主持有模型输出信封。
    let model_output = json!({"$vulcan_native_media_input":1,"text":"Read media: image.png","byte_len":60_000,"upload":null,
        "resource":{"id":"native-media-v1","kind":"image","media_type":"image/png","source_kind":"inline_data","file_name":"image.png","resource_path":"image.png","url":format!("data:image/png;base64,{base64}")}}).to_string();
    // Tool calls and results use matching local and provider identities.
    // 工具调用与结果使用匹配的本地及供应商身份。
    let tool_id = ToolCallId::try_new(1)?;
    // This projection is the authoritative V1 source, independent of pre-gate candidate messages.
    // 此投影是权威V1来源，独立于闸门前候选消息。
    let mut snapshot = SessionProviderProjectionSnapshot {
        unenveloped_assistant_requests: Default::default(),
        current_compaction_id: None,
        open_turn_id: None,
        child_business_execution_id: None,
        preparing_user_prompt_media_seqs: vec![],
        compaction_frontier_seq: None,
        image_replay_from_seq: None,
        segment_generation: 1,
        document_revision: 1,
        retained_from_seq: Seq::try_new(1)?,
        through_seq: Seq::try_new(2)?,
        items: vec![
            ProjectedItem::AssistantText {
                text: String::new(),
                reasoning: None,
                provider_source: None,
                tool_calls: vec![vulcan_session_core::v1::projection::ProjectedToolCall {
                    tool_call_id: tool_id,
                    provider_tool_call_id: Some("native-read".into()),
                    tool_name: "read_media".into(),
                    input: ToolPayload::try_new(json!({"path":"image.png"}))?,
                }],
            },
            ProjectedItem::ToolResult {
                tool_call_id: tool_id,
                provider_tool_call_id: Some("native-read".into()),
                tool_name: "read_media".into(),
                status: ToolCallStatus::Completed,
                output: ToolPayload::try_new(
                    json!({"tool_call_id":"native-read","tool_name":"read_media","model_output":model_output,"host_result":null,"is_error":false}),
                )?,
            },
        ],
        sourced_items: vec![],
        project_directory: PathBuf::new(),
        provider_usage_budget_anchor: None,
        initialized_locked_baseline: None,
    };
    // Preserve the durable source sequences so the fixture can exercise the bound compaction path.
    // 保留持久来源序列，使夹具能验证已绑定来源的压缩路径。
    snapshot.sourced_items = snapshot
        .items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let seq = Seq::try_new(index as u64 + 1)?;
            Ok(vulcan_session_core::v1::projection::SourcedProjectedItem {
                source_from_seq: seq,
                source_to_seq: seq,
                user_prompt_parameters: None,
                item: item.clone(),
            })
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    Ok((context, facts, candidate, snapshot, base64, directory))
}

/// Rebuilds a durable native read through production V1 preparation, retaining binary input and bounded accounting.
/// 通过生产V1准备重建持久原生读取，保留二进制输入及有界记账。
#[tokio::test]
async fn native_media_v1_rebuild_preserves_media_instead_of_serializing_host_json()
-> Result<(), Box<dyn std::error::Error>> {
    let (context, facts, candidate, snapshot, base64, _directory) = native_read_fixture().await?;
    // Execute the actual rebuild and budget finalization used before production provider dispatch.
    // 执行生产供应商分派前使用的实际重建及预算完成逻辑。
    let prepared = rebuild_provider_transport_candidate(
        snapshot.clone(),
        &facts,
        candidate.clone(),
        Default::default(),
    )?;
    // Media must be an input resource, and durable controller history must remain lossless.
    // 媒体必须成为输入资源，且持久控制器历史必须保持无损。
    assert!(
        prepared
            .transport
            .exact_request
            .messages
            .iter()
            .any(|message| {
                message.role == MessageRole::User && message.content.iter().any(|item| {
            matches!(item, ContentItem::Resource { resource } if resource.id == "native-media-v1")
        })
            })
    );
    assert!(
        serde_json::to_string(&prepared.transport.controller_snapshot)?
            .contains("$vulcan_native_media_input")
    );
    assert!(
        prepared
            .request_budget_evidence
            .request_text_measurement
            .character_count
            < base64.len() as u64
    );
    // Invoke the actual frozen profile encoder after V1 has replaced the provisional request.
    // V1替换临时请求后调用实际冻结Profile编码器。
    let wire = vulcan_llm_core::ProtocolRegistry::first_batch()?
        .require(&context.execution.route.protocol_route.protocol_profile_id)?
        .encode(&prepared.transport.exact_request)?;
    // Wire body carries original image content without the internal result envelope.
    // 线上正文携带原图片内容，不包含内部结果信封。
    let body = serde_json::to_string(&wire.body)?;
    assert!(body.contains("input_image"));
    assert!(body.contains(&base64));
    assert!(!body.contains("$vulcan_native_media_input"));

    // summary_facts and summary_candidate select the same frozen route and add only the summary instruction.
    // summary_facts 与 summary_candidate 选择同一冻结路由，仅追加摘要指令。
    let mut summary_facts = facts;
    summary_facts.purpose = ApiPurposeKind::Compaction;
    let mut summary_candidate = candidate;
    summary_candidate.request_snapshot.prompt = String::from("Summarize the existing context.");
    // rows and state mirror the planner's projection without using it as the physical request history.
    // rows 和 state 模拟规划器的投影，但不将其用作物理请求历史。
    let rows = snapshot
        .sourced_items
        .iter()
        .filter_map(crate::context_compaction::compaction_message_from_v1_projection)
        .map(|entry| entry.map(|(message, _)| message))
        .collect::<Result<Vec<_>, _>>()?;
    let state = vulcan_context::conversation_state_from_session_messages(&rows)?;
    // source binds the planner identity to the durable snapshot that owns the original media.
    // source 将规划器身份绑定到持有原生媒体的持久快照。
    let mut source = compaction_source::CompactionSourceWindow::capture(snapshot.clone(), &state)?;
    source.select(&vulcan_context::conversation_state_to_compaction_source_messages(&state)?)?;
    // summary uses the same native media projection as the ordinary physical request.
    // summary 与普通物理请求使用同一原生媒体投影。
    let summary = rebuild_provider_transport_candidate_with_compaction(
        snapshot,
        &summary_facts,
        summary_candidate,
        Default::default(),
        Some(&source),
    )?;
    // prefix and summary_messages compare every physical message without depending on a particular history size.
    // prefix 与 summary_messages 逐条比较物理消息，不依赖固定的历史数量。
    let prefix = &prepared.transport.exact_request.messages;
    let summary_messages = &summary.transport.exact_request.messages;
    assert_eq!(summary_messages.len(), prefix.len() + 1);
    assert_eq!(
        serde_json::to_value(&summary_messages[..prefix.len()])?,
        serde_json::to_value(prefix)?
    );
    assert!(
        matches!(summary_messages.last(), Some(message) if message.role == MessageRole::User
        && message.content.iter().any(|item| matches!(item, ContentItem::Text { text }
            if text.contains("Summarize the existing context."))))
    );
    Ok(())
}

/// Verifies the runtime-only permit sends matched media as a User resource.
/// 验证纯运行时许可将已配对媒体作为 User 资源发送。
/// Takes no arguments and returns any fixture or preparation failure.
/// 不接收参数，返回夹具或准备失败。
#[tokio::test]
async fn native_media_non_session_permit_projects_the_tool_result()
-> Result<(), Box<dyn std::error::Error>> {
    // candidate and snapshot describe the same frozen media execution.
    // candidate 和 snapshot 描述同一冻结媒体执行。
    let (_, _, mut candidate, snapshot, base64, _directory) = native_read_fixture().await?;
    // output is the exact persisted result wrapper, as emitted by the controller bridge.
    // output 是控制器桥接发出的完整持久结果包装。
    let output = snapshot
        .items
        .iter()
        .find_map(|item| match item {
            ProjectedItem::ToolResult { output, .. } => Some(output),
            _ => None,
        })
        .expect("native media fixture tool result");
    candidate.exact_request.messages = vec![
        AiMessage {
            role: MessageRole::Assistant,
            content: vec![ContentItem::ToolCall {
                call: AiToolCall {
                    id: String::from("native-read"),
                    name: String::from("read_media"),
                    arguments: json!({"path":"image.png"}),
                    signature: None,
                },
            }],
            provider_context_items: Vec::new(),
        },
        AiMessage {
            role: MessageRole::Tool,
            content: vec![ContentItem::ToolResult {
                result: AiToolResult {
                    call_id: String::from("native-read"),
                    is_error: false,
                    content: serde_json::Value::String(serde_json::to_string(output)?),
                },
            }],
            provider_context_items: Vec::new(),
        },
    ];
    // exact_request is the only request released to the provider adapter.
    // exact_request 是唯一放行给供应商适配器的请求。
    let PreparedPhysicalApi::ExcludedNonSession { exact_request, .. } =
        candidate.into_excluded_non_session()?
    else {
        panic!("expected a standalone physical request");
    };
    assert!(exact_request.messages.iter().any(|message| {
        message.role == MessageRole::User && message.content.iter().any(|item| {
            matches!(item, ContentItem::Resource { resource } if resource.id == "native-media-v1")
        })
    }));
    for item in exact_request
        .messages
        .iter()
        .flat_map(|message| &message.content)
    {
        if let ContentItem::ToolResult { result } = item {
            assert!(!serde_json::to_string(&result.content)?.contains(&base64));
        }
    }
    Ok(())
}

/// Verifies the compaction text projection keeps the media address, including for orphaned results.
/// 验证压缩文本投影保留媒体地址，包括缺少可配对调用的工具结果。
/// Takes no arguments and returns any fixture or projection failure.
/// 不接收参数，返回夹具或投影失败。
#[tokio::test]
async fn compaction_transcript_keeps_read_media_as_an_address()
-> Result<(), Box<dyn std::error::Error>> {
    // snapshot and base64 come from the same native-media fixture used by physical request preparation.
    // snapshot 与 base64 来自物理请求准备使用的同一原生媒体夹具。
    let (_, _, _, snapshot, base64, _directory) = native_read_fixture().await?;
    // item is the persisted tool result that can become historical text without a structured call.
    // item 是可能在缺少结构化调用时转成历史文本的持久工具结果。
    let item = snapshot
        .items
        .into_iter()
        .find(|item| matches!(item, ProjectedItem::ToolResult { .. }))
        .expect("native media fixture tool result");
    // projected retains the same identity and sequence when the compaction planner projects it.
    // projected 在压缩规划器投影时保留相同身份和序列。
    let projected = vulcan_session_core::v1::projection::SourcedProjectedItem {
        item,
        source_from_seq: Seq::try_new(2)?,
        source_to_seq: Seq::try_new(2)?,
        user_prompt_parameters: None,
    };
    // transcript contains the textual copy used by the compaction source and budget.
    // transcript 包含压缩来源和预算使用的文本副本。
    let (transcript, _) =
        crate::context_compaction::compaction_message_from_v1_projection(&projected)
            .expect("tool result belongs in compaction source")?;
    // state runs the orphaned result through the same conversation normalization as the summary.
    // state 使缺少调用的工具结果经过摘要使用的同一会话规范化。
    let state = vulcan_context::conversation_state_from_session_messages(&[transcript])?;
    // text is the serialized conversation after historical fallback has been applied.
    // text 是历史文本回退应用后的序列化会话。
    let text = serde_json::to_string(&state)?;
    assert!(text.contains("image.png"));
    assert!(!text.contains(&base64));
    assert!(!text.contains("data:image/png;base64,"));
    Ok(())
}

/// A read at or before the compaction frontier is replayed as its address only, never as media.
/// 压缩前沿及之前的读取只以地址重放，绝不作为媒体重放。
#[tokio::test]
async fn native_media_before_compaction_frontier_keeps_only_its_address()
-> Result<(), Box<dyn std::error::Error>> {
    let (_context, facts, candidate, mut snapshot, base64, _directory) =
        native_read_fixture().await?;
    // Both items come from seqs 1 and 2; the frontier at 2 covers the whole read.
    // 两项分别来自seq 1与2；前沿位于2覆盖整个读取。
    snapshot.compaction_frontier_seq = Some(Seq::try_new(2)?);
    let prepared =
        rebuild_provider_transport_candidate(snapshot, &facts, candidate, Default::default())?;
    let messages = &prepared.transport.exact_request.messages;
    assert!(
        !messages
            .iter()
            .flat_map(|message| &message.content)
            .any(|item| matches!(item, ContentItem::Resource { .. }))
    );
    let body = serde_json::to_string(messages)?;
    assert!(!body.contains(&base64));
    assert!(body.contains("image.png"));
    Ok(())
}

/// Verifies the real frozen-request rebuild drops foreign reasoning and preserves body and tools.
/// 验证真实冻结请求重建会丢弃外模型思考，并保留正文及工具；无参数，返回测试结果。
#[tokio::test]
async fn model_switch_preserves_visible_history_and_tool_results()
-> Result<(), Box<dyn std::error::Error>> {
    // Reuse an isolated frozen route without dispatching any inference request.
    // 复用隔离的冻结路由，不发送任何推理请求。
    let (_context, facts, candidate, mut snapshot, _base64, _directory) =
        native_read_fixture().await?;
    // Only the archived model differs from the actual target execution.
    // 仅归档模型与实际目标执行不同。
    let target = V1ReplayRouteIdentity::from_candidate(&candidate)
        .expect("the fixture must provide a frozen execution route");
    // Keep the old model identity explicit instead of inferring it from visible text.
    // 显式记录旧模型身份，不从可见文本推断。
    let previous_model = format!("{}-previous", target.model_id);
    // Build durable provenance for the supplied canonical content, retaining all original records.
    // 为传入的规范内容构建持久化来源，保留全部原始记录。
    let source_for = |content: Vec<ContentItem>| vulcan_session_core::v1::event::ProviderSource {
        fingerprint_version:
            vulcan_session_core::v1::event::ProviderFingerprintVersion::ImmutableProjectionV1,
        provider: target.provider_id.clone(),
        profile: Some(target.protocol_profile_id.clone()),
        model: previous_model.clone(),
        connection_id: Some(target.connection_id.clone()),
        message_id: Some(String::from("recorded-message")),
        response_id: None,
        replay: Some(ProviderReplayEnvelope::new(
            target.provider_id.clone(),
            target.connection_id.clone(),
            target.route_fingerprint.clone(),
            target.protocol_profile_id.clone(),
            previous_model.clone(),
            content,
            Vec::new(),
        )),
        covered_until_seq: Seq::try_new(1).expect("the fixture source sequence is valid"),
        fingerprint_sha256: String::from("0").repeat(64),
    };
    // This reasoning must remain visible in canonical history but absent from the target request.
    // 此思考须保留在规范历史中，但不能进入目标请求。
    let old_reasoning = String::from("source-analysis-only");
    // The user's visible assistant body must survive unchanged.
    // 用户可见的助手正文必须原样保留。
    let visible_body = String::from("recorded-visible-answer");
    // Locate the recorded assistant by kind so fixture ordering can evolve safely.
    // 按种类定位记录的助手消息，使夹具排序可以安全变化。
    let assistant = snapshot
        .items
        .iter_mut()
        .find(|item| matches!(item, ProjectedItem::AssistantText { .. }))
        .expect("the fixture must contain an assistant tool-call turn");
    if let ProjectedItem::AssistantText {
        text,
        reasoning,
        tool_calls,
        provider_source,
    } = assistant
    {
        *text = visible_body.clone();
        *reasoning = Some(old_reasoning.clone());
        // Mirror the exact canonical call identities and arguments in the durable envelope.
        // 在持久化封套中保留精确的规范调用标识及参数。
        let mut content = vec![
            ContentItem::Reasoning {
                text: old_reasoning.clone(),
                signature: None,
            },
            ContentItem::Text {
                text: visible_body.clone(),
            },
        ];
        for call in tool_calls {
            content.push(ContentItem::ToolCall {
                call: AiToolCall {
                    id: provider_wire_tool_call_id(
                        call.tool_call_id,
                        call.provider_tool_call_id.as_deref(),
                    ),
                    name: call.tool_name.clone(),
                    arguments: serde_json::to_value(&call.input)?,
                    signature: None,
                },
            });
        }
        *provider_source = Some(Box::new(source_for(content)));
    }
    // A reasoning-only archived turn must disappear instead of becoming an empty assistant message.
    // 纯思考归档回合须被省略，不能变成空的助手消息。
    snapshot.items.insert(
        0,
        ProjectedItem::AssistantText {
            text: String::new(),
            reasoning: Some(old_reasoning.clone()),
            tool_calls: Vec::new(),
            provider_source: Some(Box::new(source_for(vec![ContentItem::Reasoning {
                text: old_reasoning.clone(),
                signature: None,
            }]))),
        },
    );
    // Use an ordinary tool result to verify complete preservation of its structured payload.
    // 使用普通工具结果验证其结构化载荷完整保留。
    let result = snapshot
        .items
        .iter_mut()
        .find(|item| matches!(item, ProjectedItem::ToolResult { .. }))
        .expect("the fixture must contain a recorded tool result");
    // The expected value comes from the actual persisted payload, not from transport assumptions.
    // 期望值来自实际持久化载荷，不依赖传输层假设。
    let expected_output = if let ProjectedItem::ToolResult {
        tool_call_id,
        provider_tool_call_id,
        tool_name,
        output,
        ..
    } = result
    {
        *output = ToolPayload::try_new(json!({
            "tool_call_id":provider_wire_tool_call_id(*tool_call_id, provider_tool_call_id.as_deref()),
            "tool_name":tool_name.clone(), "model_output":"recorded-tool-data",
            "host_result":null, "is_error":false,
        }))?;
        serde_json::to_value(output)?
    } else {
        unreachable!("the preceding kind match selected a tool result")
    };
    // Execute the same authoritative reconstruction used for production dispatch.
    // 执行生产分派使用的同一权威重建流程。
    let prepared =
        rebuild_provider_transport_candidate(snapshot, &facts, candidate, Default::default())?;
    // All assertions inspect the actual final physical request.
    // 所有断言均检查实际最终物理请求。
    let messages = &prepared.transport.exact_request.messages;
    assert!(
        messages
            .iter()
            .flat_map(|message| &message.content)
            .all(|item| !matches!(item, ContentItem::Reasoning { .. }))
    );
    assert!(
        messages
            .iter()
            .filter(|message| message.role == MessageRole::Assistant)
            .all(
                |message| !message.content.is_empty() || !message.provider_context_items.is_empty()
            )
    );
    // The visible assistant turn must contain exactly its body and original tool call.
    // 可见助手回合必须只包含其正文及原工具调用。
    let visible = messages
        .iter()
        .find(|message| {
            message.role == MessageRole::Assistant
                && message
                    .content
                    .iter()
                    .any(|item| matches!(item, ContentItem::Text { text } if text == &visible_body))
        })
        .expect("the recorded visible answer must survive the model switch");
    assert!(visible.content.iter().all(|item| match item {
        ContentItem::Text { text } => text == &visible_body,
        ContentItem::ToolCall { .. } => true,
        _ => false,
    }));
    assert!(
        visible
            .content
            .iter()
            .any(|item| matches!(item, ContentItem::ToolCall { .. }))
    );
    assert!(messages.iter().flat_map(|message| &message.content)
        .any(|item| matches!(item, ContentItem::ToolResult { result } if result.content == expected_output)));
    assert!(
        serde_json::to_string(&prepared.transport.controller_snapshot)?.contains(&old_reasoning)
    );
    Ok(())
}
