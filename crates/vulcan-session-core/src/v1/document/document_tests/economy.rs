//! Economy settlement through real root, API, compaction and projection transitions.
//! 使用真实根输入、API、压缩和投影状态转换验证节约结算。

use super::*;
use crate::v1::economy::EconomyCandidate;
use crate::v1::projection::{ProjectedItem, project_context_generation_items_with_sources};
use vulcan_ai_contract::economy_window::EconomyWindow;

/// Closes one ordinary question and publishes its optional handoff under the current owner.
/// 关闭一个普通问题，并在当前所有者下发布可选交接摘要。
fn finish(
    doc: &mut SessionDocument,
    turn: TurnId,
    summary: Option<&str>,
) -> Result<(), DomainError> {
    finish_on_route(
        doc,
        turn,
        summary,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        None,
    )
}

/// Closes the named turn with its exact request, optional replay and handoff, returning any domain error.
/// 使用指定回合的精确请求、可选回放封套与交接摘要闭合回合，返回域错误。
fn finish_on_route(
    doc: &mut SessionDocument,
    turn: TurnId,
    summary: Option<&str>,
    request: ApiRequestSnapshot,
    source: Option<ProviderSourceInput>,
) -> Result<(), DomainError> {
    // Bind the terminal source to the same physical request used during projection.
    // 将终态来源绑定到投影所使用的同一个物理请求。
    let api = doc.start_api(ApiPurposeKind::MainGeneration, request.clone(), T0)?;
    doc.append_assistant_text(api, "final", AssistantSemanticStatus::Completed, T0)?;
    complete_ok_for_request(doc, api, request, source, T0)?;
    let revision = doc.metadata.config.economy.revision;
    doc.settle_economy_turn(
        turn,
        &EconomyCandidate {
            api,
            revision,
            summary: summary.map(String::from),
        },
        &[],
        |_| true,
    )?;
    doc.complete_turn_and_select_successor(turn, T0)?;
    Ok(())
}

/// Replays the same event projection used by Core and returns its provider-visible items.
/// 重放 Core 使用的事件投影并返回供应商可见项目。
fn projected(doc: &SessionDocument) -> Result<Vec<ProjectedItem>, DomainError> {
    let mut items =
        project_context_generation_items_with_sources(doc.events(), Seq::try_new(1)?, None)?;
    doc.metadata
        .config
        .economy
        .project(&mut items, None, doc.events());
    Ok(items.into_iter().map(|i| i.item).collect())
}

/// Ordinary Responses messages age out in batches while surviving answers retain their exact native input.
/// 普通 Responses 消息按批移出，窗口中剩余答复保留精确的原生输入。
#[test]
fn economy_responses_window_preserves_native_answers_until_eviction() -> Result<(), DomainError> {
    // Use the real default window and a separately identifiable response for each turn.
    // 使用真实默认窗口，为每个回合构造可区分的响应。
    let window = EconomyWindow::default();
    // Keep every settlement in one session so the configured window applies across responses.
    // 在同一会话中结算所有答复，使窗口阈值跨回合生效。
    let mut doc = new_document();
    doc.set_economy_mode(true, 0, T0)?;
    // Remember provider envelopes before the economy projection touches them.
    // 在节约模式投影前保存供应商原封套。
    let mut expected = Vec::new();
    for index in 0..window.trigger_turns {
        // Use the durable identity and a split output_text to check the complete native representation.
        // 使用持久标识与分段 output_text 验证完整原生表示。
        let id = format!("msg-{index}");
        // The prompt uses the message ID so questions and answers can be paired after eviction.
        // 使用消息标识作为提示词，便于裁剪后配对提问和答复。
        let turn = doc.submit_root_input_fixture(&id, Vec::new(), T0)?;
        doc.prepare_economy_turn(window, T0)?;
        // Reuse the exact Responses route recorded by the request fixture.
        // 复用请求夹具记录的同一 Responses 路由。
        let mut source = replay_source(&id);
        // Populate canonical and native payloads with different text block boundaries.
        // 用不同文本分块边界填充规范与原生载荷。
        let replay = source.replay.as_mut().expect("native source");
        replay.content.push(vulcan_ai_contract::ContentItem::Text {
            text: "final".into(),
        });
        replay.provider_context_items.push(serde_json::json!({
            "id": id, "role": "assistant", "type": "message", "phase": "final_answer",
            "content": [{"type":"output_text", "text":"fi", "annotations":[]}, {"type":"output_text", "text":"nal"}]
        }));
        replay
            .message_phases
            .push(vulcan_ai_contract::ProviderMessagePhaseSnapshot {
                item_id: id,
                phase: vulcan_ai_contract::AiMessagePhase::FinalAnswer,
            });
        expected.push(replay.clone());
        finish_on_route(&mut doc, turn, None, replay_request(), Some(source))?;
    }
    // Compare entire envelopes in order, including phase, message IDs and content-part boundaries.
    // 按顺序比较完整封套，包括阶段、消息标识与内容分块边界。
    let items = projected(&doc)?;
    // Both sides of each evicted exchange must disappear at the same boundary.
    // 已移出问答的两端必须在同一个边界消失。
    let questions: Vec<_> = items
        .iter()
        .filter_map(|item| match item {
            ProjectedItem::UserInput { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        questions,
        (window.batch_turns..window.trigger_turns)
            .map(|index| format!("msg-{index}"))
            .collect::<Vec<_>>()
    );
    // Include missing envelopes in the comparison so reconstruction cannot hide their loss.
    // 比较时保留封套缺失的情况，防止文本重建掩盖数据丢失。
    let remaining: Vec<_> = items
        .into_iter()
        .filter_map(|item| match item {
            ProjectedItem::AssistantText {
                provider_source, ..
            } => Some(provider_source.and_then(|s| s.replay)),
            _ => None,
        })
        .collect();
    assert_eq!(
        remaining,
        expected
            .into_iter()
            .skip(window.batch_turns as usize)
            .map(Some)
            .collect::<Vec<_>>()
    );
    assert!(
        doc.metadata
            .config
            .economy
            .settled
            .iter()
            .all(|r| !r.protected)
    );
    Ok(())
}

/// A Gemini ask keeps its signed function call and matching result across settlement, including older receipts.
/// Gemini 提问结算后保留带签名的函数调用及对应结果，同时覆盖已有回执的投影。
#[test]
fn economy_preserves_gemini_signed_ask_across_settlement() -> Result<(), DomainError> {
    use vulcan_ai_contract::{ContentItem, ToolCall, wrap_provider_continuation};
    // Build the signature and route in exactly the normalized form consumed by Gemini replay.
    // 按 Gemini 回放消费的规范形式构造签名与路由。
    let arguments = serde_json::json!({"question":"choice"});
    // Bind the signature to the same provider call whose answer is projected below.
    // 将签名绑定到下文投影答复所对应的供应商调用。
    let call = ToolCall {
        id: "call-ask".into(),
        name: "ask".into(),
        arguments: arguments.clone(),
        signature: wrap_provider_continuation("gemini", "signed-call"),
    };
    // Freeze the same route on the envelope, the completion and the following request.
    // 在封套、完成事件及后续请求上固定相同路由。
    let replay = ProviderReplayEnvelope::new(
        "google",
        "connection-1",
        "https://provider.invalid/gemini",
        "google.gemini.generate_content",
        "gemini",
        vec![ContentItem::ToolCall { call: call.clone() }],
        vec![],
    );
    // Derive completion identity from the route that issued the signature.
    // 从发出签名的路由派生完成事件的身份。
    let mut request = ApiRequestSnapshot::new(&replay.provider_id, &replay.model_id);
    request.profile = Some(replay.protocol_profile_id.clone());
    request.connection_id = Some(replay.connection_id.clone());
    // Store the envelope on the tool-producing API, before completing the follow-up text API.
    // 在最终文本 API 完成前，将封套存到产生工具调用的 API。
    let source = ProviderSourceInput {
        provider: request.provider.clone(),
        profile: request.profile.clone(),
        model: request.model.clone(),
        connection_id: request.connection_id.clone(),
        message_id: None,
        response_id: None,
        replay: Some(replay.clone()),
    };
    let mut doc = new_document();
    doc.set_economy_mode(true, 0, T0)?;
    let turn = doc.submit_root_input_fixture("choose", Vec::new(), T0)?;
    doc.prepare_economy_turn(Default::default(), T0)?;
    // The tool group and final answer are two physical requests of the same turn.
    // 工具调用组和最终答复属于同一回合中的两个物理请求。
    let api = doc.start_api(ApiPurposeKind::MainGeneration, request.clone(), T0)?;
    // Preserve the call ID through the answer to the question.
    // 在提问返回中保留调用标识。
    let tool = doc.start_tool(
        api,
        "ask",
        ToolPayload::try_new(arguments)?,
        Some(call.id.clone()),
        T0,
    )?;
    doc.finish_tool(
        api,
        tool,
        tool_completion(
            Some(ToolPayload::try_new(serde_json::json!({"answer":"first"}))?),
            ToolCallStatus::Completed,
            T0,
        ),
    )?;
    complete_ok_for_request(&mut doc, api, request.clone(), Some(source), T0)?;
    finish_on_route(&mut doc, turn, Some("first"), request, None)?;
    // New receipts protect this dependency. A previously settled ask still needs its original signature.
    // 新回执保护此依赖，已有提问回执投影时同样需要原始签名。
    let assert_replay = |doc: &SessionDocument| -> Result<(), DomainError> {
        let items = projected(doc)?;
        assert!(items.iter().any(|item| matches!(item,
            ProjectedItem::AssistantText { tool_calls, provider_source: Some(source), .. }
                if tool_calls.iter().any(|tool| tool.tool_name == "ask")
                    && source.replay.as_ref() == Some(&replay)
        )));
        assert!(items.iter().any(|item| matches!(item,
            ProjectedItem::ToolResult { provider_tool_call_id: Some(id), tool_name, .. }
                if id == &call.id && tool_name == "ask"
        )));
        Ok(())
    };
    assert_replay(&doc)?;
    // Exercise the projection of a persisted receipt from before signed calls were protected.
    // 覆盖签名调用受保护前已持久化的回执投影。
    let receipt = doc
        .metadata
        .config
        .economy
        .settled
        .last_mut()
        .expect("settled ask");
    assert!(receipt.protected);
    assert_eq!(receipt.retained_reason.as_deref(), Some("native_replay"));
    receipt.protected = false;
    receipt.retained_reason = None;
    receipt.summary = Some("first".into());
    assert_replay(&doc)
}

/// Completed short exchanges preserve the prefix until an entire batch is due.
/// 完成的短问答保留原前缀，直到达到整批裁剪边界。
#[test]
fn economy_batch_windows_match_projection_after_every_turn() -> Result<(), DomainError> {
    for window in [
        EconomyWindow::default(),
        EconomyWindow {
            trigger_turns: 36,
            batch_turns: 12,
        },
    ] {
        let mut doc = new_document();
        doc.set_economy_mode(true, 0, T0)?;
        let mut expected = Vec::new();
        for index in 1..=61 {
            let prompt = format!("question {index}");
            let turn = doc.submit_root_input_fixture(&prompt, Vec::new(), T0)?;
            doc.prepare_economy_turn(window, T0)?;
            finish(&mut doc, turn, None)?;
            expected.push(prompt);
            if expected.len() == window.trigger_turns as usize {
                expected.drain(..window.batch_turns as usize);
            }
            let questions: Vec<_> = projected(&doc)?
                .into_iter()
                .filter_map(|item| match item {
                    ProjectedItem::UserInput { text, .. } => Some(text),
                    _ => None,
                })
                .collect();
            assert_eq!(questions, expected);
            assert!(
                doc.metadata
                    .config
                    .economy
                    .settled
                    .iter()
                    .all(|r| r.summary.is_none())
            );
        }
    }
    Ok(())
}

/// A pinned attachment survives both default evictions; every other retained turn stays in order.
/// 首轮附件跨两次默认裁剪保留，其余保留回合的相对顺序不变。
#[test]
fn economy_pinned_first_turn_allows_complete_batches_to_retire() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.set_economy_mode(true, 0, T0)?;
    let attachment = ResourceId::try_new(17)?;
    let mut eviction_turns = Vec::new();
    for n in 1..=61 {
        let resources = if n == 1 { vec![attachment] } else { Vec::new() };
        let turn = doc.submit_root_input_fixture(format!("q{n}"), resources, T0)?;
        doc.prepare_economy_turn(EconomyWindow::default(), T0)?;
        let before = doc.metadata.config.economy.anchor_revision();
        finish(&mut doc, turn, None)?;
        if before != doc.metadata.config.economy.anchor_revision() {
            eviction_turns.push(n);
        }
    }
    assert_eq!(eviction_turns, vec![40, 60]);
    let prompts: Vec<_> = projected(&doc)?
        .into_iter()
        .filter_map(|item| match item {
            ProjectedItem::UserInput { text, resource_ids } => Some((text, resource_ids)),
            _ => None,
        })
        .collect();
    let expected: Vec<_> = std::iter::once(("q1".to_owned(), vec![attachment]))
        .chain((42..=61).map(|n| (format!("q{n}"), Vec::new())))
        .collect();
    assert_eq!(prompts, expected);
    doc.metadata.config.economy.validate()?;
    Ok(())
}

/// Even above the threshold, wait for a full batch of older eligible turns and keep the latest answer.
/// 超出阈值后仍须等到凑齐整批合格旧回合，并始终保留最新答复。
#[test]
fn economy_pins_defer_partial_batch_and_keep_current_answer() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.set_economy_mode(true, 0, T0)?;
    let window = EconomyWindow {
        trigger_turns: 4,
        batch_turns: 2,
    };
    for n in 1..=6 {
        let resources = if n <= 3 {
            vec![ResourceId::try_new(n)?]
        } else {
            vec![]
        };
        let turn = doc.submit_root_input_fixture(format!("q{n}"), resources, T0)?;
        doc.prepare_economy_turn(window, T0)?;
        finish(&mut doc, turn, None)?;
        let state = &doc.metadata.config.economy;
        assert_eq!(
            state
                .settled
                .iter()
                .filter(|r| r.retired_at.is_some())
                .count(),
            if n == 6 { 2 } else { 0 }
        );
        assert!(
            state
                .settled
                .iter()
                .any(|r| r.turn.turn_id == turn && r.retired_at.is_none())
        );
    }
    doc.metadata.config.economy.validate()?;
    Ok(())
}

/// Two intermediate compactions leave the preceding fifteen settled turns untouched at delivery.
/// 交付时两次中间压缩均不改变先前十五个已结算回合。
#[test]
fn economy_restores_fifteen_turn_prefix_after_two_compactions() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.set_economy_mode(true, 0, T0)?;
    for index in 1..=15 {
        let turn = doc.submit_root_input_fixture(format!("q{index}"), Vec::new(), T0)?;
        doc.prepare_economy_turn(EconomyWindow::default(), T0)?;
        finish(&mut doc, turn, None)?;
    }
    let prefix = projected(&doc)?;
    assert!(doc.metadata.config.economy.anchor_revision().is_none());
    let turn = doc.submit_root_input_fixture("long", Vec::new(), T0)?;
    doc.prepare_economy_turn(EconomyWindow::default(), T0)?;
    let tool_api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0,
    )?;
    let tool = doc.start_tool(
        tool_api,
        "file_read",
        ToolPayload::try_new(serde_json::json!({}))?,
        None,
        T0,
    )?;
    doc.finish_tool(
        tool_api,
        tool,
        tool_completion(
            Some(ToolPayload::try_new(
                serde_json::json!({"result":"x".repeat(65536)}),
            )?),
            ToolCallStatus::Completed,
            T0,
        ),
    )?;
    complete_ok(&mut doc, tool_api, None, T0)?;
    let original_bytes = serde_json::to_vec(&projected(&doc)?).unwrap().len();
    for label in ["compaction one", "compaction two"] {
        let api = doc.start_api(
            ApiPurposeKind::Compaction,
            ApiRequestSnapshot::new("openai", "gpt-5"),
            T0,
        )?;
        let seq = doc
            .append_assistant_text(api, label, AssistantSemanticStatus::Completed, T0)?
            .into_value();
        complete_ok(&mut doc, api, None, T0)?;
        let id = doc
            .record_compaction(
                CompactionCommitInput::new(api, seq, seq, label, LockedBaselineProjection::empty()),
                T0,
            )?
            .into_value();
        let persisted = doc.prepare_flush_candidates()?.unwrap();
        doc.advance_durable(persisted.revision)?;
        assert_eq!(
            doc.metadata.config.economy.visible_compaction(Some(id)),
            Some(id)
        );
    }
    finish(&mut doc, turn, Some("tested <path>"))?;
    assert_eq!(
        doc.metadata
            .config
            .economy
            .visible_compaction(doc.runtime_lock.current_compaction_id),
        None
    );
    let items = projected(&doc)?;
    let published_bytes = serde_json::to_vec(&items).unwrap().len();
    assert!(original_bytes > published_bytes + 64000);
    assert!(items.starts_with(&prefix));
    assert!(
        matches!(&items[prefix.len()..], [ProjectedItem::UserInput {text, ..}, ProjectedItem::AssistantHandoff {text: handoff}, ProjectedItem::AssistantText {text: answer, ..}] if text == "long" && handoff.contains("tested &lt;path&gt;") && answer == "final")
    );
    let next = doc.submit_root_input_fixture("q17", Vec::new(), T0)?;
    doc.prepare_economy_turn(EconomyWindow::default(), T0)?;
    finish(&mut doc, next, None)?;
    assert!(projected(&doc)?.starts_with(&items));
    Ok(())
}

/// A handoff cannot cover an insertion that arrived after the final request was frozen.
/// 最终请求冻结后才进入本轮的插话不可被该请求产生的交接摘要覆盖。
#[test]
fn economy_declines_response_that_did_not_see_promoted_input() -> Result<(), DomainError> {
    let mut doc = new_document();
    let turn = doc.submit_root_input_fixture("initial", Vec::new(), T0)?;
    doc.enable_draft_economy(EconomyWindow::default(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0,
    )?;
    doc.add_pending_in_turn_input_fixture("late instruction", Vec::new(), T0)?;
    doc.append_assistant_text(
        api,
        "already generated",
        AssistantSemanticStatus::Completed,
        T0,
    )?;
    complete_ok(&mut doc, api, None, T0)?;
    doc.promote_pending_in_turn_inputs_after_api(T0)?;
    doc.settle_economy_turn(
        turn,
        &EconomyCandidate {
            api,
            revision: 1,
            summary: Some("stale".into()),
        },
        &[],
        |_| true,
    )?;
    assert!(doc.metadata.config.economy.settled.is_empty());
    Ok(())
}

/// A missing handoff keeps the native tool result until the complete turn ages out; Ask remains paired.
/// 漏写摘要时保留原始工具结果直至整轮按龄移出；提问与答复必须始终配对。
#[test]
fn economy_preserves_ask_and_missing_summary_without_freezing_the_window() -> Result<(), DomainError>
{
    let mut doc = new_document();
    doc.set_economy_mode(true, 0, T0)?;
    let window = EconomyWindow {
        trigger_turns: 4,
        batch_turns: 1,
    };
    for (tool_name, handoff) in [
        ("file_read", None),
        ("ask", Some("user chose the first option")),
        ("file_read", Some("verified")),
        ("file_read", Some("rechecked")),
    ] {
        let turn = doc.submit_root_input_fixture(tool_name, Vec::new(), T0)?;
        doc.prepare_economy_turn(window, T0)?;
        let api = doc.start_api(
            ApiPurposeKind::MainGeneration,
            ApiRequestSnapshot::new("openai", "gpt-5"),
            T0,
        )?;
        let tool = doc.start_tool(
            api,
            tool_name,
            ToolPayload::try_new(serde_json::json!({"question": "choice"}))?,
            None,
            T0,
        )?;
        doc.finish_tool(
            api,
            tool,
            tool_completion(
                Some(ToolPayload::try_new(
                    serde_json::json!({"choice": "first"}),
                )?),
                ToolCallStatus::Completed,
                T0,
            ),
        )?;
        complete_ok(&mut doc, api, None, T0)?;
        let old_anchor = doc.metadata.config.economy.anchor_revision();
        let old_budget = doc.metadata.config.economy.position_revision();
        finish(&mut doc, turn, handoff)?;
        let economy = &doc.metadata.config.economy;
        if handoff.is_some() {
            assert_ne!(economy.position_revision(), old_budget);
            match economy.settled.iter().any(|r| r.retired_at.is_some()) {
                true => assert_ne!(economy.anchor_revision(), old_anchor),
                false if old_anchor.is_some() => assert_eq!(economy.anchor_revision(), old_anchor),
                false => assert!(economy.anchor_revision().is_some()),
            }
        }
        if handoff.is_none() {
            assert!(projected(&doc)?.iter().any(|item| matches!(item, ProjectedItem::ToolResult {tool_name, ..} if tool_name=="file_read")));
            assert_eq!(
                doc.metadata
                    .config
                    .economy
                    .settled
                    .last()
                    .unwrap()
                    .retained_reason
                    .as_deref(),
                Some("missing_summary")
            );
        }
    }
    let items = projected(&doc)?;
    assert!(items.windows(2).any(|pair| matches!(pair, [ProjectedItem::AssistantText {tool_calls,..}, ProjectedItem::ToolResult {tool_name,..}] if tool_calls.iter().any(|tool|tool.tool_name=="ask") && tool_name=="ask")));
    assert!(!items.iter().any(
        |item| matches!(item, ProjectedItem::ToolResult {tool_name,..} if tool_name=="file_read")
    ));
    doc.metadata.config.economy.validate()?;
    Ok(())
}

/// A preference revision cannot adopt an already running turn or publish a stale candidate.
/// 偏好修订不能接管已运行回合，也不能发布旧修订候选。
#[test]
fn economy_switch_respects_running_turn_and_expected_revision() -> Result<(), DomainError> {
    let mut doc = new_document();
    let first = doc.submit_root_input_fixture("active", Vec::new(), T0)?;
    doc.set_economy_mode(true, 0, T0)?;
    assert!(!doc.prepare_economy_turn(EconomyWindow::default(), T0)?);
    assert!(doc.set_economy_mode(false, 0, T0).is_err());
    finish(&mut doc, first, None)?;
    assert!(doc.metadata.config.economy.settled.is_empty());
    let next = doc.submit_root_input_fixture("second", Vec::new(), T0)?;
    doc.prepare_economy_turn(EconomyWindow::default(), T0)?;
    assert!(doc.metadata.config.economy.initialize);
    doc.set_economy_mode(false, 1, T0)?;
    finish(&mut doc, next, None)?;
    assert!(doc.metadata.config.economy.settled.is_empty());
    Ok(())
}
