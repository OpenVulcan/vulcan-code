//! Semantic projection of an already-authorized bounded session event page.
//! 已授权有界会话事件页的语义投影。

use super::*;
use serde::Serialize;
use std::collections::BTreeSet;
use vulcan_session_core::v1::collaboration::{
    CollaborationAfterState, CollaborationAnswerDeliveryState, CollaborationTaskResult,
};
use vulcan_session_core::v1::event::{ConversationEvent, EventBody};
use vulcan_tools_manager::result_markdown::fenced_code;

/// Exact bounded page returned after the service finishes its permission-guarded read.
/// 服务完成权限保护读取后返回的精确有界页。
#[derive(Deserialize)]
struct SessionPage {
    /// Authorized target session.
    /// 已授权目标会话。
    session_id: String,
    /// Authorized target project.
    /// 已授权目标项目。
    project_id: String,
    /// Requested event window, never expanded by this projection.
    /// 请求的事件窗口，此投影绝不扩大范围。
    events: Vec<ConversationEvent>,
    /// Global exclusive frontier, omitted by the service's zero-limit response.
    /// 全局排他边界；服务的零条数响应不提供此字段。
    next_seq: Option<u64>,
}

/// Converts an authorized page into role-ordered Markdown and exact continuation coordinates.
/// 将已授权页转换为按角色排序的 Markdown 和精确续读坐标。
/// Takes the unchanged service DTO; returns one body without provider replay or host snapshots.
/// 接收未修改的服务对象，返回不含供应商重放或宿主快照的唯一正文。
pub(super) fn project(output: Value) -> Result<VulcanToolInvocationResult, String> {
    let page: SessionPage = serde_json::from_value(output)
        .map_err(|error| format!("read_session result projection failed: {error}."))?;
    let mut body = format!(
        "# Session {}\n\nProject: {}\n",
        inline_code(&page.session_id),
        inline_code(&page.project_id)
    );
    let mut seen = BTreeSet::new();
    if page.events.is_empty() {
        body.push_str("\nNo events in the requested range.\n");
    }
    for event in &page.events {
        let mut section = String::new();
        match &event.event {
            EventBody::UserInput(input) => {
                section.push_str(&format!(
                    "User ({})\n\n{}",
                    state_name(&input.input_source),
                    fenced_code(&input.text)
                ));
                for resource in &input.resource_ids {
                    section.push_str(&format!("\nResource: {}", resource.value()));
                }
            }
            EventBody::AssistantText(text) => {
                section.push_str(&format!("Assistant ({})", state_name(&text.status)));
                append_body(
                    &mut section,
                    &mut seen,
                    format!("assistant:{}:{}", page.session_id, event.seq.value()),
                    &text.text,
                );
            }
            EventBody::AssistantReasoning(reasoning) => section.push_str(&format!(
                "Reasoning ({})\n\n{}",
                state_name(&reasoning.status),
                fenced_code(&reasoning.text)
            )),
            EventBody::ToolCall(tool) => {
                section.push_str(&format!(
                    "Tool {} ({})",
                    inline_code(&tool.tool_name),
                    state_name(&tool.status)
                ));
                if let Some(output) = &tool.output {
                    // Core emits canonical model_output envelopes or explicit synthetic terminal failures.
                    // Core 产出规范 model_output 封套或明确的合成终态失败。
                    let output = serde_json::to_value(output).map_err(|error| error.to_string())?;
                    if let Some(model_output) = output.get("model_output") {
                        let text = match model_output {
                            Value::String(text) => {
                                crate::agent_runtime::tool_result_media::tool_result_text_preview(
                                    text,
                                )?
                            }
                            original => original.to_string(),
                        };
                        section.push_str(&format!("\n\n{}", fenced_code(&text)));
                    } else {
                        section.push_str(&format!("\n\n{}", fenced_code(&output.to_string())));
                    }
                }
                for resource in &tool.generated_resources {
                    section.push_str(&format!(
                        "\nResource {} ({})",
                        resource.resource_id.value(),
                        state_name(&resource.kind)
                    ));
                }
                for reference in &tool.external_refs {
                    section.push_str(&format!(
                        "\nReference {} ({})",
                        inline_code(&reference.id),
                        state_name(&reference.kind)
                    ));
                }
            }
            EventBody::TurnBoundary(boundary) => section.push_str(&format!(
                "Turn ended: {}; reason {}",
                state_name(&boundary.outcome),
                state_name(&boundary.reason)
            )),
            EventBody::SubagentReturn(child) => {
                section.push_str(&format!(
                    "Child result from {}",
                    inline_code(&child.child_session_id)
                ));
                append_body(
                    &mut section,
                    &mut seen,
                    format!(
                        "assistant:{}:{}",
                        child.child_session_id,
                        child.source_assistant_text_seq.value()
                    ),
                    &child.content,
                );
            }
            EventBody::DisplayNote(note) => {
                section.push_str(&format!(
                    "Notice {}: {}",
                    state_name(&note.severity),
                    inline_code(&note.reason_code)
                ));
                if let Some(detail) = &note.redacted_detail {
                    section.push_str(&format!("\n\n{}", fenced_code(detail)));
                }
            }
            // Recalled memory bodies stay out; the outcome and queries say what the turn was given.
            // 召回的记忆正文不输出；结果与检索词说明该回合获得了什么。
            EventBody::MemoryRecall(recall) => {
                section.push_str(&format!(
                    "Memory recall: {}; {} memories",
                    state_name(&recall.outcome),
                    recall.memories.len()
                ));
                for query in &recall.queries {
                    section.push_str(&format!(
                        "
Query: {}",
                        inline_code(query)
                    ));
                }
            }
            EventBody::ExecutionControl(control) => section.push_str(&format!(
                "Execution control: {}",
                state_name(&control.control_kind)
            )),
            EventBody::Collaboration(collaboration) => {
                section.push_str(&format!(
                    "Collaboration: {}",
                    state_name(&collaboration.event_kind)
                ));
                collaboration_section(&mut section, &mut seen, &collaboration.after_state);
            }
            // These are host configuration/accounting facts, not requested dialogue bodies.
            // 这些是宿主配置或统计事实，不属于请求的对话正文。
            EventBody::SystemContext(_)
            | EventBody::ApiStarted(_)
            | EventBody::ApiStats(_)
            | EventBody::TurnStats(_)
            | EventBody::ContextMaintenance(_)
            | EventBody::RequestConfigChanged(_)
            | EventBody::QueuedInputResolution(_) => continue,
        }
        body.push_str(&format!("\n## Event {}", event.seq.value()));
        if let Some(turn) = event.turn_id {
            body.push_str(&format!(" · Turn {}", turn.value()));
        }
        body.push_str(&format!(" · {} ms\n\n{section}\n", event.updated_at_ms));
    }
    if let Some(last) = page.events.last() {
        body.push_str(&format!("\nLast scanned event: {}. Continue with read_session after_seq={} to read later events.\n", last.seq.value(), last.seq.value()));
    }
    if let Some(next_seq) = page.next_seq {
        body.push_str(&format!("\nObserved event frontier (exclusive): {next_seq}. Historical events alone do not prove current execution liveness.\n"));
    }
    Ok(
        VulcanToolInvocationResult::from_model_output(Value::String(body)).with_host_result(
            VulcanToolHostResult {
                kind: "collaboration_receipt_v1".into(),
                payload: json!({"format":"markdown"}),
            },
        ),
    )
}

/// Returns the stable serialized name of a fieldless domain enum.
/// 返回无字段领域枚举的稳定序列化名称。
fn state_name(state: &impl Serialize) -> String {
    serde_json::to_value(state)
        .expect("domain state serializes")
        .as_str()
        .expect("fieldless domain state")
        .to_owned()
}

/// Includes each immutable source body once per page while retaining every state transition.
/// 每页仅包含一次不可变来源正文，同时保留所有状态转换。
/// The caller supplies a domain identity, never a content hash or guessed equivalence.
/// 调用方提供领域身份，绝不使用正文哈希或推测等价关系。
fn append_body(section: &mut String, seen: &mut BTreeSet<String>, identity: String, text: &str) {
    if seen.insert(identity) {
        section.push_str(&format!("\n\n{}", fenced_code(text)));
    }
}

/// Projects changed collaboration facts without emitting their nested replay snapshots.
/// 投影变更协作事实，不输出嵌套重放快照。
fn collaboration_section(
    section: &mut String,
    seen: &mut BTreeSet<String>,
    state: &CollaborationAfterState,
) {
    match state {
        CollaborationAfterState::Question { question, delivery } => {
            section.push_str(&format!(
                "\nQuestion {}: {}; delivery {}",
                inline_code(question.question_id.as_str()),
                state_name(&question.state),
                state_name(&delivery.state)
            ));
            append_body(
                section,
                seen,
                format!("question:{}", question.question_id.as_str()),
                &question.body,
            );
            if let Some(answer) = &question.answer {
                section.push_str(&format!(
                    "\nAnswer delivery: {}",
                    state_name(&answer.delivery.state)
                ));
                if answer.delivery.state != CollaborationAnswerDeliveryState::Failed {
                    append_body(
                        section,
                        seen,
                        format!("answer:{}", question.question_id.as_str()),
                        &answer.body,
                    );
                }
                if let Some(reason) = &answer.delivery.failure {
                    section.push_str(&format!(
                        "\nAnswer delivery failure: {}",
                        state_name(reason)
                    ));
                }
            }
            if let Some(cancellation) = &question.cancellation {
                section.push_str(&format!(
                    "\nCancellation reason: {}",
                    fenced_code(&cancellation.reason)
                ));
            }
        }
        CollaborationAfterState::OutboundTaskMessage { record } => {
            section.push_str(&format!(
                "\nTo {}; delivery {}; accepted {}",
                inline_code(&record.message.target.session_id),
                inline_code(record.message.delivery_id.as_str()),
                record.acceptance.is_some()
            ));
            append_body(
                section,
                seen,
                format!("message:{}", record.message.delivery_id.as_str()),
                &record.message.body,
            );
        }
        CollaborationAfterState::InboundTaskMessage { acceptance } => {
            section.push_str(&format!(
                "\nFrom {}; delivery {}",
                inline_code(&acceptance.message.source.session_id),
                inline_code(acceptance.message.delivery_id.as_str())
            ));
            append_body(
                section,
                seen,
                format!("message:{}", acceptance.message.delivery_id.as_str()),
                &acceptance.message.body,
            );
        }
        CollaborationAfterState::TaskResult { result } => {
            task_result_section(section, seen, result)
        }
        CollaborationAfterState::ReceivedTaskResult { acceptance } => {
            task_result_section(section, seen, &acceptance.result)
        }
        CollaborationAfterState::ChildTask { task } => {
            section.push_str(&format!(
                "\nChild {}; execution {}; state {}",
                inline_code(&task.child.session_id),
                inline_code(task.execution_id.as_str()),
                state_name(&task.state)
            ));
            if let Some(outcome) = &task.terminal_outcome {
                section.push_str(&format!("; outcome {}", state_name(outcome)));
            }
        }
        CollaborationAfterState::RunControl { transition } => {
            section.push_str(&format!("\nRun control: {}", state_name(&transition.state)))
        }
        CollaborationAfterState::OutboundNotification { notification } => {
            section.push_str(&format!(
                "\nTo {}; notification {}; state {}",
                inline_code(&notification.target.session_id),
                inline_code(notification.notification_id.as_str()),
                state_name(&notification.state)
            ));
            append_body(
                section,
                seen,
                format!("notification:{}", notification.notification_id.as_str()),
                &notification.body,
            );
        }
        CollaborationAfterState::InboundNotification { notification } => {
            section.push_str(&format!(
                "\nFrom {}; notification {}; state {}",
                inline_code(&notification.source.session_id),
                inline_code(notification.notification_id.as_str()),
                state_name(&notification.state)
            ));
            append_body(
                section,
                seen,
                format!("notification:{}", notification.notification_id.as_str()),
                &notification.body,
            );
        }
        CollaborationAfterState::AnswerDelivery {
            question_id,
            delivery,
        } => section.push_str(&format!(
            "\nQuestion {}; delivery {}",
            inline_code(question_id.as_str()),
            state_name(&delivery.state)
        )),
        CollaborationAfterState::OutboundDelivery { delivery } => section.push_str(&format!(
            "\nQuestion {}; delivery {}",
            inline_code(delivery.question_id.as_str()),
            state_name(&delivery.state)
        )),
        CollaborationAfterState::InboundDelivery { delivery } => section.push_str(&format!(
            "\nQuestion {}; delivery {}",
            inline_code(delivery.question_id.as_str()),
            state_name(&delivery.state)
        )),
        CollaborationAfterState::QuestionExecution { execution } => section.push_str(&format!(
            "\nQuestion {}; execution {}",
            inline_code(execution.question_id.as_str()),
            state_name(&execution.state)
        )),
        CollaborationAfterState::TaskMessageEffect { effect } => {
            section.push_str(&format!(
                "\nDelivery {}",
                inline_code(effect.delivery_id.as_str())
            ));
            if let vulcan_session_core::v1::collaboration::CollaborationTaskMessageEffectKind::Failed { reason } = &effect.outcome {
                section.push_str(&format!("; failed {}", state_name(reason)));
            }
        }
        CollaborationAfterState::TaskResultProjection { .. }
        | CollaborationAfterState::TaskResultAcknowledgement { .. }
        | CollaborationAfterState::Grant { .. }
        | CollaborationAfterState::GenerationOwnershipCleared => {}
    }
}

/// Shows an immutable task outcome and the exact authorized final text when supplied.
/// 展示不可变任务结果及提供时的精确获授权最终正文。
fn task_result_section(
    section: &mut String,
    seen: &mut BTreeSet<String>,
    result: &CollaborationTaskResult,
) {
    section.push_str(&format!(
        "\nSession {}; delivery {}",
        inline_code(&result.acceptance.message.target.session_id),
        inline_code(result.acceptance.message.delivery_id.as_str())
    ));
    if let Some(terminal) = &result.terminal {
        section.push_str(&format!("; outcome {}", state_name(&terminal.outcome)));
        if let Some(assistant) = &terminal.final_assistant {
            append_body(
                section,
                seen,
                format!(
                    "assistant:{}:{}",
                    result.acceptance.message.target.session_id,
                    assistant.source_text.event_seq.value()
                ),
                &assistant.source_text.text,
            );
        }
    } else {
        section.push_str("; delivery failed before task execution");
        if let vulcan_session_core::v1::collaboration::CollaborationTaskMessageEffectKind::Failed { reason } = &result.effect.outcome {
            section.push_str(&format!("; reason {}", state_name(reason)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a typed event envelope around the supplied semantic fixture, returning its JSON DTO.
    /// 围绕语义夹具构建类型化事件封套，返回 JSON 传输对象。
    fn event(seq: u64, body: Value) -> Value {
        json!({"seq":seq,"turn_id":1,"created_at_ms":1,"updated_at_ms":2,"event_revision":1,
            "visibility":"provider","event":body})
    }

    /// Verifies pagination uses the scanned page, while tool metadata and private context stay omitted.
    /// 验证分页使用已扫描页位置，同时省略工具元数据及私有上下文。
    #[test]
    fn session_read_keeps_semantics_and_cursor_without_internal_snapshots() {
        let result = project(json!({"session_id":"session-a","project_id":"p","next_seq":20,"events":[
            event(5,json!({"kind":"assistant_text","api_call_id":2,"text":"unique-answer","status":"completed"})),
            event(6,json!({"kind":"tool_call","api_call_id":2,"tool_call_id":3,"request_stream_id":"stream",
                "tool_name":"file","input":{"action":"write","content":"private-input-copy"},"status":"completed",
                "output":{"model_output":"File written.","host_result":{"kind":"internal","payload":{"secret":"private-host-copy"}}}})),
            event(7,json!({"kind":"system_context","context_key":"private","operation":"upsert","scope":"global","content":"private-context-copy"}))
        ]})).unwrap();
        let body = result.model_output.as_str().unwrap();
        assert_eq!(body.matches("unique-answer").count(), 1);
        assert!(body.contains("File written."));
        assert!(body.contains("after_seq=7"));
        assert!(body.contains("(exclusive): 20"));
        for omitted in [
            "private-input-copy",
            "private-host-copy",
            "private-context-copy",
            "event_revision",
        ] {
            assert!(!body.contains(omitted), "Internal field leaked: {omitted}");
        }
    }

    /// Source identities suppress repeated receipt bodies without equating unrelated identical text.
    /// 来源身份抑制重复回执正文，不将内容相同但无关的消息视为等价。
    #[test]
    fn immutable_source_identity_controls_body_deduplication() {
        let mut body = String::new();
        let mut seen = BTreeSet::new();
        append_body(&mut body, &mut seen, "question:q1".into(), "same-text");
        append_body(&mut body, &mut seen, "question:q1".into(), "same-text");
        append_body(&mut body, &mut seen, "question:q2".into(), "same-text");
        assert_eq!(body.matches("same-text").count(), 2);
    }
}
