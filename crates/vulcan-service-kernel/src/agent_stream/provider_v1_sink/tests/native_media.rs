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
    let snapshot = SessionProviderProjectionSnapshot {
        current_compaction_id: None,
        open_turn_id: None,
        child_business_execution_id: None,
        preparing_user_prompt_media_seqs: vec![],
        compaction_frontier_seq: None,
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
    let prepared =
        rebuild_provider_transport_candidate(snapshot, &facts, candidate, Default::default())?;
    // Media must be an input resource, and durable controller history must remain lossless.
    // 媒体必须成为输入资源，且持久控制器历史必须保持无损。
    assert!(prepared.transport.exact_request.messages.iter().flat_map(|message| &message.content).any(|item| matches!(item, ContentItem::Resource { resource } if resource.id == "native-media-v1")));
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
    snapshot.sourced_items = snapshot
        .items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let seq = Seq::try_new(index as u64 + 1)?;
            Ok(vulcan_session_core::v1::projection::SourcedProjectedItem {
                user_prompt_parameters: None,
                source_from_seq: seq,
                source_to_seq: seq,
                item: item.clone(),
            })
        })
        .collect::<Result<_, Box<dyn std::error::Error>>>()?;
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
