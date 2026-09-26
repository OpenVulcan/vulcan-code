//! Conversation projection of LuaSkills 0.5.9 lease replies.
//! LuaSkills 0.5.9 租约回执的会话投影。

use super::{
    RunLuaSessionAction, lua_runtime_response_error_text, required_response_string,
    runtime_response_ok,
};
use serde_json::Value;
use vulcan_tools_manager::result_markdown::{fenced_code, inline_code};

/// Renders one public action without changing the engine's machine-readable lease contract.
/// 渲染一次公开动作，不改变引擎的机器可读租约契约。
/// Takes the executed action and exact response; returns Markdown or an explicit shape error.
/// 接收已执行动作与准确响应，返回 Markdown 或明确的结构错误。
pub(super) fn render(action: RunLuaSessionAction, response: &Value) -> Result<String, String> {
    // Lifecycle metadata is host-owned; arbitrary eval values remain exact business data.
    // 生命周期元数据归宿主管理，任意求值内容保留准确业务数据。
    let mut output = format!("# Lua session: {}\n", action.as_str());
    if !runtime_response_ok(response)? {
        output.push_str(&format!(
            "\nFailed: {}\n",
            lua_runtime_response_error_text(response)?
        ));
        return Ok(output);
    }
    if matches!(action, RunLuaSessionAction::List) {
        // The list producer decorates upstream statuses with exact owner-bound names and IDs.
        // 列表生产者为上游状态补充准确的所有者绑定名称与标识。
        let sessions = response
            .get("sessions")
            .and_then(Value::as_array)
            .ok_or_else(|| String::from("Lua session list is missing sessions."))?;
        for session in sessions {
            output.push_str(&format!(
                "\n## {}\n\nSession ID: {}\n",
                inline_code(&required_response_string(session, "name")?),
                inline_code(&required_response_string(session, "session_id")?)
            ));
            if runtime_response_ok(session)? {
                append_status(&mut output, session)?;
            } else {
                output.push_str(&format!(
                    "\nStatus check failed: {}\n",
                    lua_runtime_response_error_text(session)?
                ));
            }
        }
        if sessions.is_empty() {
            output.push_str("\nNo active Lua sessions.\n");
        }
        return Ok(output);
    }
    output.push_str(&format!(
        "\nSession ID: {}\n",
        inline_code(&required_response_string(response, "lease_id")?)
    ));
    match action {
        RunLuaSessionAction::Eval => {
            output.push_str("\nEvaluation completed.\n");
            // A null result is meaningful; a missing result is a contract failure.
            // 空值结果有业务意义，缺失结果则是契约失败。
            let result = response
                .get("result")
                .ok_or_else(|| String::from("Lua evaluation is missing its result."))?;
            // String results are readable source; nonstrings preserve their original JSON value.
            // 字符串结果作为可读正文，非字符串保留原始 JSON 值。
            let body = match result {
                Value::String(text) => text.clone(),
                value => serde_json::to_string_pretty(value)
                    .map_err(|error| format!("Failed to encode Lua evaluation result: {error}"))?,
            };
            output.push_str(&format!("\n## Result\n\n{}\n", fenced_code(&body)));
            append_expiry(&mut output, response);
        }
        RunLuaSessionAction::Start => {
            output.push_str("\nSession started.\n");
            append_expiry(&mut output, response);
        }
        RunLuaSessionAction::Status | RunLuaSessionAction::Close => {
            append_status(&mut output, response)?
        }
        RunLuaSessionAction::List => unreachable!("list handled above"),
    }
    Ok(output)
}

/// Appends observed lifecycle flags, never inferring active status from an absent flag.
/// 追加已观察生命周期标记，不依据缺失标记推断活跃状态。
/// Takes a successful status/close snapshot and mutable output; returns a shape error if invalid.
/// 接收成功的状态或关闭快照与可变输出，结构无效时返回错误。
fn append_status(output: &mut String, response: &Value) -> Result<(), String> {
    // LuaSkills status_payload and close_payload both require these exact booleans.
    // LuaSkills 的 status_payload 与 close_payload 都要求这两个准确布尔字段。
    let closed = response
        .get("closed")
        .and_then(Value::as_bool)
        .ok_or_else(|| String::from("Lua session status is missing closed."))?;
    // Expiration is independent of the explicit close operation.
    // 过期独立于显式关闭操作。
    let expired = response
        .get("expired")
        .and_then(Value::as_bool)
        .ok_or_else(|| String::from("Lua session status is missing expired."))?;
    output.push_str(&format!("\nClosed: {closed}; expired: {expired}\n"));
    append_expiry(output, response);
    Ok(())
}

/// Appends supplied lifetime facts shared by start, eval and status responses.
/// 追加启动、求值和状态响应共享的已提供有效期信息。
/// Takes output and the exact response; returns no value and omits absent/null optional fields.
/// 接收输出与准确响应，无返回值，省略缺失或为空的可选字段。
fn append_expiry(output: &mut String, response: &Value) {
    for (field, label) in [
        ("ttl_sec", "Idle TTL (seconds)"),
        ("expires_at_unix_ms", "Expires at (Unix ms)"),
    ] {
        if let Some(value) = response.get(field).filter(|value| !value.is_null()) {
            output.push_str(&format!("\n{label}: {value}\n"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Keeps arbitrary eval JSON intact while removing repeated engine configuration.
    /// 完整保留任意求值 JSON，同时移除重复引擎配置。
    #[test]
    fn eval_preserves_raw_result_once() {
        // Engine internals must not escape merely because the result is an object.
        // 不能因为结果是对象就把引擎内部信息输出。
        let response = serde_json::json!({
            "ok": true, "lease_id": "lease-1", "sid": "private-sid", "generation": 7,
            "cwd": "private-root", "expires_at_unix_ms": 42,
            "result": { "answer": "独有正文", "valid": false, "empty": null }
        });
        // The result is source data, not a second outer protocol envelope.
        // 结果为源数据，不是第二层外部协议信封。
        let body = render(RunLuaSessionAction::Eval, &response).expect("eval projection");
        assert_eq!(body.matches("独有正文").count(), 1);
        assert!(body.contains("\"valid\": false"));
        assert!(body.contains("\"empty\": null"));
        assert!(body.contains("lease-1"));
        assert!(!body.contains("private-sid"));
        assert!(!body.contains("private-root"));
    }

    /// Distinguishes an empty list, a failed check and an actual closed session.
    /// 区分空列表、检查失败和实际关闭的会话。
    #[test]
    fn lifecycle_outcomes_remain_explicit() {
        assert!(
            render(
                RunLuaSessionAction::List,
                &serde_json::json!({"ok": true, "sessions": []})
            )
            .expect("list")
            .contains("No active Lua sessions")
        );
        assert!(
            render(
                RunLuaSessionAction::Status,
                &serde_json::json!({
                    "ok": false, "error_code": "lease_expired", "message": "Lease expired."
                })
            )
            .expect("failure")
            .contains("Failed: lease_expired")
        );
        assert!(
            render(
                RunLuaSessionAction::Close,
                &serde_json::json!({
                    "ok": true, "lease_id": "lease-1", "closed": true, "expired": false
                })
            )
            .expect("closed")
            .contains("Closed: true; expired: false")
        );
    }
}
