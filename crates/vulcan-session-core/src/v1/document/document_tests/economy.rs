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
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0,
    )?;
    doc.append_assistant_text(api, "final", AssistantSemanticStatus::Completed, T0)?;
    complete_ok(doc, api, None, T0)?;
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
        matches!(&items[prefix.len()..], [ProjectedItem::UserInput {text, ..}, ProjectedItem::SystemText {text: handoff, ..}, ProjectedItem::AssistantText {text: answer, ..}] if text == "long" && handoff.contains("tested &lt;path&gt;") && answer == "final")
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
        trigger_turns: 3,
        batch_turns: 1,
    };
    for (tool_name, handoff) in [
        ("file_read", None),
        ("ask", Some("user chose the first option")),
        ("file_read", Some("verified")),
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
        finish(&mut doc, turn, handoff)?;
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
