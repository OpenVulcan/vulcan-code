//! Conversation-only projections; internal task controls retain their structured results.
//! 仅用于会话的投影；内部任务控制保留结构化结果。

use super::*;
use vulcan_tools_manager::result_markdown::{fenced_code, inline_code};

/// Wraps a registered conversation candidate after its runtime operation has completed.
/// 在运行时操作完成后包装已注册会话工具。
/// Takes the existing candidate and returns the same contract with a reading projection.
/// 接受原工具，返回输入契约不变且增加阅读投影的工具。
pub(super) fn conversation_candidate(mut candidate: VulcanToolCandidate) -> VulcanToolCandidate {
    let invoke = candidate.invoke.clone();
    let tool = candidate.descriptor.id.clone();
    candidate.invoke = Arc::new(move |args, context| {
        let invoke = invoke.clone();
        let tool = tool.clone();
        Box::pin(async move {
            let result = invoke(args, context).await?;
            project(&tool, result).map_err(|error| format!("The {tool} operation already returned but its conversation projection failed: {error}. Do not repeat the operation automatically."))
        })
    });
    candidate
}

/// Projects the six registered subagent tools without changing machine callers or error contracts.
/// 投影六个已注册子代理工具，不改变机器调用方或错误契约。
/// Takes a known tool and its completed result; returns Markdown plus actual UI/lifecycle metadata.
/// 接受已知工具及完成结果；返回 Markdown 和实际界面、生命周期所需元数据。
fn project(
    tool: &str,
    mut result: VulcanToolInvocationResult,
) -> Result<VulcanToolInvocationResult, String> {
    if result
        .host_result
        .as_ref()
        .is_some_and(|host| host.kind == vulcan_tools_manager::VULCAN_TOOL_ERROR_DETAIL_KIND)
    {
        return Ok(result);
    }
    let model = &result.model_output;
    let host = result.host_result.as_ref().ok_or_else(|| {
        format!("{tool} completed without its host identity record; do not repeat the operation.")
    })?;
    let mut payload = json!({"format":"markdown"});
    let body = match tool {
        SPAWN_AGENT_TOOL => {
            // Identity and execution correlation are consumed by the existing live child card.
            // 既有活动子代理卡消费身份与执行关联字段。
            copy_fields(
                &host.payload,
                &mut payload,
                &[
                    "task_id",
                    "subagent_task_id",
                    "display_title",
                    "execution_id",
                    "execution_status",
                ],
            );
            format!(
                "# Agent scheduled\n\nName: {}\nSession ID: {}\nExecution: {}\n\nScheduling is not completion. Use `wait_agents_result` for subsequent results.",
                code(model, "task_name")?,
                code(model, "subagent_task_id")?,
                code(model, "execution_status")?
            )
        }
        SEND_MESSAGE_TOOL => {
            copy_fields(
                &host.payload,
                &mut payload,
                &["target", "subagent_task_id", "queued", "delivery_mode"],
            );
            format!(
                "# Agent message accepted\n\nTarget: {}\nDelivery mode: {}\n\nThe message is queued; this does not confirm task completion. Use `wait_agents_result` for feedback.",
                code(model, "target")?,
                code(model, "delivery_mode")?
            )
        }
        LIST_AGENTS_TOOL => {
            // Only a count is needed by the header; the shared body owns names, states and results.
            // 标题只需要数量；共享正文拥有名称、状态和结果。
            let agents = array(model, "agents")?;
            payload["agent_count"] = json!(agents.len());
            let mut text = String::from("# Agent status\n");
            if agents.is_empty() {
                text.push_str("\nNo agents match this request.\n");
            }
            for agent in agents {
                text.push_str(&format!("\n## {}\n\nTree path: {}\nStatus: {}; lifecycle: {}; run state: {}.\nQueued messages: {}.\n",
                    code(agent,"agent_name")?,code(agent,"tree_path")?,code(agent,"status")?,code(agent,"lifecycle_state")?,code(agent,"run_state")?,required(agent,"queued_message_count")?));
                for (label, key) in [
                    ("Stop reason", "stop_reason"),
                    ("Last task", "last_task_message"),
                    ("Last result", "last_result_summary"),
                ] {
                    if let Some(value) = optional_text(agent, key)? {
                        text.push_str(&format!("\n{label}:\n{}\n", fenced_code(value)));
                    }
                }
                text.push_str(&format!("\nFinal result available: {}. Controls: send {}, interrupt {}, close {}, wait {}.\n",required(agent,"has_final_result")?,required(agent,"can_send")?,required(agent,"can_interrupt")?,required(agent,"can_close")?,required(agent,"can_wait")?));
            }
            text
        }
        AGENT_ORCHESTRATION_INFO_TOOL => {
            payload["agent_count"] = json!(array(model, "agents")?.len());
            orchestration(model)?
        }
        CLOSE_AGENT_TOOL | INTERRUPT_AGENT_TOOL => {
            // Close projection is also a lifecycle consumer, so exact affected session IDs remain.
            // 关闭投影还服务生命周期消费者，因此保留精确受影响会话身份。
            copy_fields(
                &host.payload,
                &mut payload,
                &[
                    "tool",
                    "closed_agents",
                    "closed_count",
                    "closed",
                    "cleared_message_count",
                    "stopped_subagent_task_ids",
                    "stopped_count",
                ],
            );
            payload["matched_agents"] = Value::Array(
                array(&host.payload, "matched_agents")?
                    .iter()
                    .map(|agent| {
                        Ok(json!({"subagent_task_id": required(agent,"subagent_task_id")?}))
                    })
                    .collect::<Result<Vec<_>, String>>()?,
            );
            let mut text = if tool == CLOSE_AGENT_TOOL {
                String::from("# Agent closed\n")
            } else {
                String::from("# Agent interrupted\n\nThe agent remains open.\n")
            };
            text.push_str(&format!(
                "\nPrevious status: {}.\nStopped runs: {}.\n",
                required(model, "previous_status")?,
                required(&host.payload, "stopped_count")?
            ));
            if tool == CLOSE_AGENT_TOOL {
                text.push_str(&format!(
                    "\nClosed agents: {}.\nCleared queued messages: {}.\n",
                    array(model, "closed_agents")?
                        .iter()
                        .map(|value| value
                            .as_str()
                            .map(inline_code)
                            .ok_or_else(|| String::from("Invalid closed agent name")))
                        .collect::<Result<Vec<_>, _>>()?
                        .join(", "),
                    required(model, "cleared_message_count")?
                ));
            }
            text
        }
        _ => return Err(format!("No conversation result projection for {tool}")),
    };
    result.model_output = Value::String(body);
    // Only close uses the existing control kind for event-driven panel removal; other results use
    // an explicit reading marker, leaving legacy mailbox projection records untouched.
    // 只有关闭操作使用既有控制类型驱动面板移除；其他结果使用明确阅读标记，不修改旧邮箱投影记录。
    result.host_result = Some(VulcanToolHostResult {
        kind: if tool == CLOSE_AGENT_TOOL {
            String::from("subagent_control")
        } else {
            String::from(vulcan_subagent_core::SUBAGENT_RECEIPT_KIND)
        },
        payload,
    });
    Ok(result)
}

/// Renders the catalog sections and exact model selectors that the spawn tool accepts.
/// 渲染目录分区和启动工具接受的精确模型选择器。
/// Accepts the internal catalog result and returns readable profiles without account/debug data.
/// 接受内部目录结果，返回不含账号和调试数据的可读说明。
fn orchestration(model: &Value) -> Result<String, String> {
    let mut text = String::from("# Agent orchestration\n");
    for agent in array(model, "agents")? {
        text.push_str(&format!(
            "\n## Profile {} — {}\n\n{}\n",
            code(agent, "agent_id")?,
            code(agent, "name")?,
            text_field(agent, "description")?
        ));
        text.push_str(&selection(required(agent, "model_selection")?)?);
    }
    for (title, key) in [
        ("Session model", "session_model"),
        ("Focused profile model", "agent_fixed_model"),
    ] {
        let value = required(model, key)?;
        if !value.is_null() {
            text.push_str(&format!("\n{title}:\n{}", selection(value)?));
        }
    }
    let models = array(model, "models")?;
    if !models.is_empty() {
        text.push_str(
            "\n## Available model selectors\n\nPass a selector verbatim to `spawn_agent.model`.\n",
        );
        for row in models {
            text.push_str(&format!(
                "- {} — {}\n",
                code(row, "selector")?,
                code(row, "model_name")?
            ));
        }
    }
    let modes = array(model, "modes")?;
    if !modes.is_empty() {
        text.push_str("\n## Creation modes\n");
        for mode in modes {
            text.push_str(&format!(
                "- {}: {}\n",
                code(mode, "id")?,
                text_field(mode, "description")?
            ));
        }
    }
    let runtime = required(model, "runtime")?;
    if !runtime.is_null() {
        text.push_str(&format!(
            "\nOpen agents: {}; running agents: {}; queued messages: {}.\n",
            required(runtime, "open_agent_count")?,
            required(runtime, "running_agent_count")?,
            required(runtime, "queued_message_count")?
        ));
    }
    Ok(text)
}

/// Renders a documented fixed selector or session inheritance, retaining unknown availability.
/// 渲染已声明固定选择器或会话继承，保留未知可用性。
/// Accepts a catalog selection and returns its exact selector and unavailability explanation.
/// 接受目录选择，返回精确选择器及不可用说明。
fn selection(value: &Value) -> Result<String, String> {
    if optional_text(value, "mode")? == Some("session") {
        return Ok(String::from("Inherits the session model.\n"));
    }
    let mut text = format!(
        "Model: {}; available: {}.\n",
        code(value, "selector")?,
        required(value, "available")?
    );
    if let Some(reason) = optional_text(value, "unavailable_reason")? {
        text.push_str(&format!("{reason}\n"));
    }
    Ok(text)
}

/// Copies only known optional UI fields; absent producer fields remain absent.
/// 仅复制已知可选界面字段；生产者未提供的字段保持缺失。
/// Takes source, destination and an audited allowlist; mutates only the destination.
/// 接受来源、目标及已审核白名单；只修改目标。
fn copy_fields(source: &Value, destination: &mut Value, keys: &[&str]) {
    for key in keys {
        if let Some(value) = source.get(*key) {
            destination[*key] = value.clone();
        }
    }
}

/// Reads a required producer field or reports its missing contract explicitly.
/// 读取生产者必需字段，或明确报告契约缺失。
/// Takes an object and key; returns the borrowed value without candidate paths.
/// 接受对象与键；返回借用值，不探测候选路径。
fn required<'a>(value: &'a Value, key: &str) -> Result<&'a Value, String> {
    value.get(key).ok_or_else(|| {
        format!("Subagent result is missing {key}; operation already completed, do not repeat it.")
    })
}

/// Reads a required text field from the established producer contract.
/// 从既有生产者契约读取必需文本字段。
/// Takes object/key and returns text or a shape error.
/// 接受对象及键，返回文本或形状错误。
fn text_field<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    required(value, key)?
        .as_str()
        .ok_or_else(|| format!("Invalid subagent result field {key}: expected text"))
}

/// Formats an exact required string without altering its identifier contents.
/// 格式化精确必需字符串，不改变其标识内容。
/// Takes object/key and returns safe inline Markdown or a shape error.
/// 接受对象及键，返回安全行内 Markdown 或形状错误。
fn code(value: &Value, key: &str) -> Result<String, String> {
    Ok(inline_code(text_field(value, key)?))
}

/// Reads a documented nullable or omitted text field without accepting other shapes.
/// 读取已声明可空或省略的文本字段，不接受其他形状。
/// Takes object/key and returns optional text or an explicit shape error.
/// 接受对象及键，返回可选文本或明确形状错误。
fn optional_text<'a>(value: &'a Value, key: &str) -> Result<Option<&'a str>, String> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text)),
        Some(_) => Err(format!("Invalid optional subagent field {key}")),
    }
}

/// Reads a required list from the exact producer object.
/// 从精确生产者对象读取必需列表。
/// Takes object/key and returns the borrowed entries or a shape error.
/// 接受对象及键，返回借用条目或形状错误。
fn array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>, String> {
    required(value, key)?
        .as_array()
        .ok_or_else(|| format!("Invalid subagent result array {key}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exercises real runtime producers while preserving their machine-facing objects.
    /// 覆盖真实运行时生产者，同时保留其机器侧对象。
    #[tokio::test]
    async fn reading_projection_accepts_real_runtime_catalog_and_status() {
        let runtime = TaskRuntime::new();
        let session = SessionId::new_v4();
        let catalog = runtime
            .invoke_agent_orchestration_info(session, json!({}))
            .await
            .expect("runtime catalog");
        assert!(catalog.model_output.is_object());
        let projected =
            project(AGENT_ORCHESTRATION_INFO_TOOL, catalog.clone()).expect("catalog projection");
        assert!(
            projected
                .model_output
                .as_str()
                .unwrap()
                .contains("Agent orchestration")
        );
        assert!(
            catalog.model_output.is_object(),
            "Machine contract remains structured"
        );
        let status = runtime
            .invoke_list_agents(session, json!({}))
            .await
            .expect("runtime status");
        let projected = project(LIST_AGENTS_TOOL, status).expect("status projection");
        assert!(
            projected
                .model_output
                .as_str()
                .unwrap()
                .contains("No agents match")
        );
    }

    /// Verifies close projection retains lifecycle identities without copying registry bodies.
    /// 验证关闭投影保留生命周期身份且不复制注册表正文。
    #[test]
    fn close_receipt_preserves_panel_removal_contract() {
        let id = SessionId::new_v4();
        let result = VulcanToolInvocationResult::from_model_output(json!({"previous_status":"completed","closed_agents":["worker"],"cleared_message_count":2})).with_host_result(VulcanToolHostResult{kind:"subagent_control".into(),payload:json!({"tool":"close_agent","matched_agents":[{"subagent_task_id":id.to_string(),"last_result_summary":"PRIVATE_RESULT"}],"stopped_subagent_task_ids":[],"stopped_count":0,"closed":true,"closed_agents":["worker"],"closed_count":1,"cleared_message_count":2})});
        let projected = project(CLOSE_AGENT_TOOL, result).expect("project close");
        let host = projected.host_result.expect("control metadata");
        let closed = vulcan_workbench_model::subagent_close_agent_projection_from_host_result(
            &host.kind,
            &host.payload,
        )
        .expect("lifecycle decoder")
        .expect("close result");
        assert_eq!(closed.matched_task_ids, vec![id]);
        assert!(!host.payload.to_string().contains("PRIVATE_RESULT"));
    }

    /// Verifies catalog model selection retains unknown availability and exact selectors.
    /// 验证目录模型选择保留未知可用性与精确选择器。
    #[test]
    fn catalog_selection_preserves_unavailable_and_unknown_states() {
        let output = selection(&json!({"mode":"fixed","selector":"provider/model","available":null,"unavailable_reason":null})).expect("selection");
        assert!(output.contains("available: null"));
        assert!(output.contains("provider/model"));
        assert!(selection(&json!({"mode":"fixed"})).is_err());
    }
}
