//! Streaming response candidate processing for Google Gemini.
//! Google Gemini 流式响应候选处理逻辑。

use super::tool_conversion::gemini_function_call_key;
use crate::agent_runtime::provider_protocol_bridge::core_provider_turn_delta_to_runtime_delta;
use crate::agent_runtime::provider_protocol_gemini::{GeminiCandidate, GeminiPart};
use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
use crate::agent_runtime::provider_stream::ProviderStreamDelta;
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::tool_request_argument_assembler::ToolCallDraftAssembler;
use crate::agent_runtime::tool_request_stream::ToolRequestFragment;
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
    provider_tool_definitions: &[ProviderToolDefinition],
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
                forward_complete_google_gemini_tool_request(
                    live_sender,
                    provider_tool_definitions,
                    &function_call,
                );
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

/// Projects one complete Gemini function-call part as an immediately visible and ready tool request.
/// 将一个完整 Gemini 函数调用 Part 投影为立即可见且已就绪的工具请求。
///
/// # Parameters
/// 参数
/// `live_sender` is the host-owned channel for normalized runtime deltas.
/// `live_sender` 是发送规范化运行时增量的宿主通道。
/// `provider_tool_definitions` is the frozen exact tool catalog used for this physical request.
/// `provider_tool_definitions` 是当前物理请求使用的冻结精确工具目录。
/// `function_call` is the complete structured call delivered by Gemini.
/// `function_call` 是 Gemini 交付的完整结构化调用。
///
/// # Returns
/// 返回值
/// This function emits zero or two ordered request fragments and has no return value.
/// 此函数发出零条或两条有序请求碎片，不返回值。
fn forward_complete_google_gemini_tool_request(
    live_sender: &ProviderLiveDeltaSender,
    provider_tool_definitions: &[ProviderToolDefinition],
    function_call: &crate::agent_runtime::provider_protocol_gemini::GeminiFunctionCall,
) {
    if !provider_tool_definitions
        .iter()
        .any(|definition| definition.name == function_call.name)
    {
        return;
    }
    let arguments = serde_json::to_string(&function_call.args)
        .unwrap_or_else(|_| function_call.args.to_string());
    let tool_call_id = function_call
        .id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let argument_assembler = ToolCallDraftAssembler::new(arguments.clone());
    let _ = live_sender.send(Ok(ProviderStreamDelta::ToolRequestFragment(
        ToolRequestFragment::Detected {
            request_stream_id: tool_call_id.clone(),
            tool_call_id: tool_call_id.clone(),
            tool_name: function_call.name.clone(),
            initial_arguments: arguments,
            argument_assembler: Some(argument_assembler),
        },
    )));
    let _ = live_sender.send(Ok(ProviderStreamDelta::ToolRequestFragment(
        ToolRequestFragment::Completed {
            request_stream_id: tool_call_id.clone(),
            canonical_tool_call_id: Some(tool_call_id),
            final_arguments: None,
        },
    )));
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

#[cfg(test)]
mod tests {
    //! Gemini complete function-call request-stream tests.
    //! Gemini 完整函数调用请求流测试。

    use super::*;
    use crate::agent_runtime::provider_protocol_gemini::GeminiFunctionCall;
    use serde_json::json;

    /// Verifies one complete Gemini function part emits ordered detected and ready boundaries with exact identity.
    /// 验证单个完整 Gemini 函数 Part 会以精确身份发出有序的识别与就绪边界。
    #[test]
    fn complete_function_call_emits_detected_then_completed() {
        // live channel captures the same two fragments consumed by the shared coordinator.
        // live 通道捕获共享协调器消费的同两条碎片。
        let (live_sender, mut live_receiver) = tokio::sync::mpsc::unbounded_channel();
        let provider_tools = vec![ProviderToolDefinition {
            name: String::from("submit_plan"),
            description: String::from("Submit a plan"),
            parameters: json!({ "type": "object" }),
        }];
        forward_complete_google_gemini_tool_request(
            &live_sender,
            &provider_tools,
            &GeminiFunctionCall {
                id: Some(String::from("gemini-call-1")),
                name: String::from("submit_plan"),
                args: json!({ "name": "demo" }),
            },
        );

        let first = live_receiver
            .try_recv()
            .expect("detected fragment should exist")
            .expect("detected fragment should be successful");
        let second = live_receiver
            .try_recv()
            .expect("completed fragment should exist")
            .expect("completed fragment should be successful");
        assert!(matches!(
            first,
            ProviderStreamDelta::ToolRequestFragment(ToolRequestFragment::Detected {
                request_stream_id,
                tool_call_id,
                tool_name,
                initial_arguments,
                argument_assembler,
            }) if request_stream_id == "gemini-call-1"
                && tool_call_id == "gemini-call-1"
                && tool_name == "submit_plan"
                && initial_arguments == "{\"name\":\"demo\"}"
                && argument_assembler.as_ref().is_some_and(|assembler| {
                    assembler.read(|arguments| arguments == "{\"name\":\"demo\"}")
                })
        ));
        assert!(matches!(
            second,
            ProviderStreamDelta::ToolRequestFragment(ToolRequestFragment::Completed {
                request_stream_id,
                canonical_tool_call_id: Some(canonical_tool_call_id),
                final_arguments: None,
            }) if request_stream_id == "gemini-call-1"
                && canonical_tool_call_id == "gemini-call-1"
        ));
        assert!(live_receiver.try_recv().is_err());
    }

    /// Verifies an unregistered Gemini function name cannot create a misleading tool card.
    /// 验证未注册的 Gemini 函数名称不能创建误导性的工具卡片。
    #[test]
    fn unknown_function_call_emits_no_request_fragment() {
        // empty catalog proves the provider payload alone cannot authorize request visibility.
        // 空目录证明仅凭 Provider 载荷不能授权请求可见性。
        let (live_sender, mut live_receiver) = tokio::sync::mpsc::unbounded_channel();
        forward_complete_google_gemini_tool_request(
            &live_sender,
            &[],
            &GeminiFunctionCall {
                id: Some(String::from("unknown-call")),
                name: String::from("unknown_tool"),
                args: json!({}),
            },
        );
        assert!(live_receiver.try_recv().is_err());
    }
}
