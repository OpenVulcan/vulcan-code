//! Streaming response candidate processing for Google Gemini.
//! Google Gemini 流式响应候选处理逻辑。

use super::tool_conversion::gemini_function_call_key;
use crate::agent_runtime::provider_protocol_bridge::core_provider_turn_delta_to_runtime_delta;
use crate::agent_runtime::provider_protocol_gemini::{GeminiCandidate, GeminiPart};
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use std::collections::HashMap;
use vulcan_core::provider::VulcanProviderDeltaSink;
use vulcan_core::types::{
    CoreError as VulcanCoreError, ProviderTurnDelta as CoreProviderTurnDelta,
};

/// Processes one Gemini candidate payload, forwards live deltas, and mutates the accumulated native part list used for the final provider-state replay.
/// 处理单个 Gemini 候选载荷，转发实时增量，并更新用于最终 Provider 状态回放的原生 Part 累积列表。
pub(super) fn process_gemini_candidate<S>(
    provider_id: &str,
    candidate: GeminiCandidate,
    live_sender: &ProviderLiveDeltaSender,
    sink: &mut S,
    assistant_text: &mut String,
    finalized_parts: &mut Vec<GeminiPart>,
    pending_reasoning_index: &mut Option<usize>,
    function_part_indices: &mut HashMap<String, usize>,
) -> Result<(), VulcanCoreError>
where
    S: VulcanProviderDeltaSink,
{
    let Some(content) = candidate.content else {
        return Ok(());
    };

    for part in content.parts {
        if let Some(function_call) = part.function_call.clone() {
            let key = gemini_function_call_key(&function_call);
            if let Some(index) = function_part_indices.get(&key).copied() {
                finalized_parts[index] = part;
            } else {
                function_part_indices.insert(key, finalized_parts.len());
                finalized_parts.push(part);
            }
            continue;
        }

        if part.is_reasoning() {
            let text = part.text.clone().unwrap_or_default();
            if !text.is_empty() {
                forward_google_gemini_delta(
                    live_sender,
                    sink,
                    CoreProviderTurnDelta::Thinking {
                        delta: text.clone(),
                    },
                );
            }

            match *pending_reasoning_index {
                Some(index) => {
                    let existing_text = finalized_parts[index].text.clone().unwrap_or_default();
                    finalized_parts[index].text =
                        Some(reconcile_reasoning_text(&existing_text, &text));
                    if part.thought_signature.is_some() {
                        finalized_parts[index].thought_signature = part.thought_signature.clone();
                        *pending_reasoning_index = None;
                    }
                }
                None => {
                    let index = finalized_parts.len();
                    finalized_parts.push(part.clone());
                    if part.thought_signature.is_none() {
                        *pending_reasoning_index = Some(index);
                    }
                }
            }
            continue;
        }

        if let Some(text) = part.text.clone().filter(|value| !value.is_empty()) {
            assistant_text.push_str(&text);
            forward_google_gemini_delta(
                live_sender,
                sink,
                CoreProviderTurnDelta::Output { delta: text },
            );
            finalized_parts.push(part);
            continue;
        }

        finalized_parts.push(part);
    }

    if let Some(finish_reason) = candidate.finish_reason
        && finish_reason.eq_ignore_ascii_case("malformed_function_call")
    {
        return Err(VulcanCoreError::ProviderFailure {
            provider_id: provider_id.to_string(),
            message: String::from("Google Gemini 返回了无效函数调用，无法继续当前回合。"),
        });
    }

    Ok(())
}

/// Reconciles one finalized Gemini reasoning text with the latest streamed chunk while avoiding duplicated prefixes when the provider replays the full block on the signature chunk.
/// 在避免 Provider 在签名块中重放完整文本时产生重复前缀的前提下，协调一段 Gemini 最终思考文本与最新流式块。
fn reconcile_reasoning_text(existing_text: &str, latest_text: &str) -> String {
    if latest_text.is_empty() {
        return existing_text.to_string();
    }
    if existing_text.is_empty() {
        return latest_text.to_string();
    }
    if latest_text.starts_with(existing_text) {
        return latest_text.to_string();
    }
    if existing_text.starts_with(latest_text) {
        return existing_text.to_string();
    }
    format!("{existing_text}{latest_text}")
}

fn forward_google_gemini_delta<S>(
    live_sender: &ProviderLiveDeltaSender,
    sink: &mut S,
    delta: CoreProviderTurnDelta,
) where
    S: VulcanProviderDeltaSink,
{
    let runtime_delta = core_provider_turn_delta_to_runtime_delta(delta.clone());
    let _ = live_sender.send(Ok(runtime_delta));
    sink.push_delta(delta);
}
