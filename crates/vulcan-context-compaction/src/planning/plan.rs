use vulcan_context::{SessionMessageEntry, SessionMessageKind};

use super::tail::{select_compaction_tail, tail_selection_for_explicit_start};
use super::time::current_unix_millis_text;
use super::{CompactionPlan, CompactionPlanInput};
/// Plans the compaction step and returns the source slice plus checkpoint metadata when compaction is needed.
/// 规划压缩步骤，并在需要压缩时返回源消息切片与检查点元数据。
pub fn plan_context_compaction(input: CompactionPlanInput<'_>) -> Option<CompactionPlan> {
    plan_compaction(input, false)
}

/// Plans an image-only retirement even when the complete text tail fits and must be retained.
/// 在完整文字尾部可容纳且必须保留时，仍规划仅退役图片的压缩。
pub fn plan_conversation_image_compaction(
    input: CompactionPlanInput<'_>,
) -> Option<CompactionPlan> {
    plan_compaction(input, true)
}

/// Plans the selected scope while keeping the existing whole-segment tail policy authoritative.
/// 在保持既有完整执行段尾部策略权威的前提下规划选定范围。
fn plan_compaction(
    input: CompactionPlanInput<'_>,
    allow_full_text_tail: bool,
) -> Option<CompactionPlan> {
    let context_start_index = input
        .context_start_message_id
        .and_then(|context_start_message_id| {
            input
                .messages
                .iter()
                .position(|message| message.id == context_start_message_id)
        })
        .unwrap_or(0);
    let context_candidates = input.messages[context_start_index..]
        .iter()
        .filter(|message| message.contributes_to_context())
        .cloned()
        .collect::<Vec<_>>();

    let tail_selection = match input.source_until_message_id {
        Some(source_until_message_id) => {
            let retained_tail_start = context_candidates
                .iter()
                .position(|message| message.id == source_until_message_id)
                .map(|index| index.saturating_add(1))?;
            tail_selection_for_explicit_start(
                &context_candidates,
                retained_tail_start,
                input.policy,
            )?
        }
        None => select_compaction_tail(&context_candidates, input.policy),
    };
    let retained_tail_start = tail_selection.start_index;
    if retained_tail_start == 0 && (!allow_full_text_tail || context_candidates.is_empty()) {
        return None;
    }

    let compaction_source = &context_candidates[..retained_tail_start];
    let source_until_message_id = compaction_source.last().map(|message| message.id.clone());
    let source_until_turn_index = source_until_message_id
        .as_deref()
        .and_then(|message_id| absolute_turn_index_for_message(input.all_messages, message_id));
    let retained_tail_start_turn_index = context_candidates
        .get(retained_tail_start)
        .and_then(|message| absolute_turn_index_for_message(input.all_messages, &message.id));
    let created_at = current_unix_millis_text();

    Some(CompactionPlan {
        source_messages: compaction_source.to_vec(),
        source_until_message_id,
        source_until_turn_index,
        retained_tail_start_turn_index,
        compaction_id: format!("session-{}-compaction-{created_at}", input.session_label),
        created_at,
        policy: input.policy,
        tail_selection: tail_selection.report,
    })
}

fn absolute_turn_index_for_message(
    messages: &[SessionMessageEntry],
    message_id: &str,
) -> Option<i64> {
    let mut current_turn_index = -1_i64;

    for message in messages {
        if matches!(message.kind, SessionMessageKind::User) {
            current_turn_index = current_turn_index.saturating_add(1);
        }
        if message.id == message_id {
            return Some(current_turn_index.max(0));
        }
    }

    None
}
