//! Exact Hook authority for a newly admitted explicit Continue action.
//! 新准入显式 Continue 动作的精确 Hook 权威。

use super::*;
use vulcan_agent_runtime::agent_runtime::conversation_hooks::CanonicalInTurnHookInput;
use vulcan_session_core::v1::ClientActionId;
use vulcan_session_core::v1::business_hook::{BusinessHookInputKind, BusinessHookMetadata};
use vulcan_session_core::v1::document::ContinueTurnReceipt;
use vulcan_session_core::v1::event::{
    ApiPurposeKind, EventBody, ExecutionControlKind, UserInputSource,
};

/// Original published Hook plus the optional exact human input of one explicit continuation.
/// 原已发布 Hook 及一次显式继续的可选精确人类输入。
pub(crate) struct ContinuationHookAuthority {
    /// Original immutable Hook binding and chronological observations.
    /// 原不可变 Hook 绑定及按时序排列的观察。
    pub(crate) state: BusinessHookMetadata,
    /// Empty for pure Continue; otherwise exactly one canonical supplement.
    /// 纯继续时为空；否则恰好包含一条规范补充。
    pub(crate) inputs: Vec<CanonicalInTurnHookInput>,
}

/// Resolves `receipt` for `action` against the same published document revision owned by `handle`.
/// 对照 `handle` 持有的同一已发布文档修订解析 `action` 的 `receipt`。
/// Returns original Hook ownership and optional input; stale actions, mixed coordinates and missing bindings fail explicitly.
/// 返回原 Hook 归属和可选输入；过期动作、混合坐标和缺失绑定明确失败。
pub(crate) fn continuation_hook_authority(
    handle: &SessionHandle,
    action: &ClientActionId,
    receipt: &ContinueTurnReceipt,
) -> Result<ContinuationHookAuthority, String> {
    handle.read(|document| {
        // The admission receipt must still own the open business and no newer physical execution.
        // 准入回执必须仍持有开放业务，且没有更新的物理执行占用。
        let turn = document.active_turn().filter(|turn| !turn.closed)
            .ok_or_else(|| String::from("Continue Hook authority has no open business turn."))?;
        if document.memory_revision() != document.durable_revision()
            || turn.turn_id != receipt.turn_id
            || turn.origin == vulcan_session_core::v1::document::TurnOrigin::QuestionExecution
            || document.active_api().is_some()
            || receipt.requested_seq >= receipt.resumed_seq {
            return Err(String::from("Continue Hook authority no longer owns the published business boundary."));
        }
        for (seq, kind) in [(receipt.requested_seq, ExecutionControlKind::TurnContinueRequested),
            (receipt.resumed_seq, ExecutionControlKind::TurnResumed)] {
            if !document.events().iter().any(|event| event.seq == seq && event.turn_id == Some(receipt.turn_id)
                && matches!(&event.event, EventBody::ExecutionControl(control)
                    if control.control_kind == kind && control.client_action_id.as_ref() == Some(action))) {
                return Err(String::from("Continue Hook receipt does not match its canonical control events."));
            }
        }
        if document.events().iter().any(|event| event.turn_id == Some(receipt.turn_id)
            && event.seq > receipt.resumed_seq
            && (matches!(&event.event, EventBody::ApiStarted(started)
                    if !matches!(started.purpose, ApiPurposeKind::Compaction | ApiPurposeKind::QuestionExecution))
                || matches!(&event.event, EventBody::ExecutionControl(control)
                    if control.control_kind == ExecutionControlKind::TurnResumed))) {
            return Err(String::from("Continue Hook authority was superseded by a later business boundary."));
        }
        // The durable binding is original admission authority; recent User text is never a substitute.
        // 可靠绑定是原始准入权威；最近 User 正文绝不能替代它。
        let state = &document.metadata.business_hook;
        state.validate(document.segment_generation()).map_err(|error| error.to_string())?;
        let binding = state.binding.as_ref()
            .ok_or_else(|| String::from("Continue requires its original published business Hook."))?;
        if binding.session_id != handle.session_id().to_string()
            || binding.turn_id != receipt.turn_id || binding.start_seq != turn.start_seq
            || state.observed_inputs.iter().any(|entry| entry.turn_id != receipt.turn_id) {
            return Err(String::from("Continue Hook binding belongs to another business owner."));
        }
        let inputs = match (receipt.input_id, receipt.input_seq) {
            (None, None) => {
                if document.events().iter().any(|event| matches!(&event.event,
                    EventBody::UserInput(input) if &input.client_action_id == action)) {
                    return Err(String::from("Pure Continue receipt omitted its canonical human input."));
                }
                Vec::new()
            }
            (Some(input_id), Some(input_seq)) => {
                let event = document.events().iter().find(|event| event.seq == input_seq)
                    .ok_or_else(|| String::from("Continue input event is absent."))?;
                let EventBody::UserInput(input) = &event.event else {
                    return Err(String::from("Continue input receipt points to another event kind."));
                };
                if event.turn_id != Some(receipt.turn_id) || input_seq >= receipt.requested_seq
                    || input.input_id != input_id || &input.client_action_id != action
                    || input.input_source != UserInputSource::PendingInTurn
                    || !state.observed_inputs.iter().any(|entry| entry.client_action_id == *action
                        && entry.input_id == input_id && entry.content == input.text
                        && entry.kind == BusinessHookInputKind::Interrupt) {
                    return Err(String::from("Continue input differs from its original admitted observation."));
                }
                vec![CanonicalInTurnHookInput { client_action_id: action.clone(), input_id, input_seq, text: input.text.clone() }]
            }
            _ => return Err(String::from("Continue receipt has incomplete human input coordinates.")),
        };
        Ok(ContinuationHookAuthority { state: state.clone(), inputs })
    }).map_err(|error| error.to_string())?
}
