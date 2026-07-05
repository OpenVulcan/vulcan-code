use super::*;

pub(crate) fn build_terminal_session_render_input(
    action: TerminalSessionToolAction,
    snapshot: &CommandSessionSnapshot,
    session_name: &str,
    returned_chunks: &[CommandSessionOutputChunk],
    has_more: bool,
    output_mode: &str,
    transaction: Option<&TerminalTransactionResult>,
    extra_lines: Vec<String>,
) -> HostTextRenderInput {
    let mut lines = vec![
        format!("Action: {}", action.as_str()),
        format!("Terminal session: {}", snapshot.command_session_id),
        format!("Name: {session_name}"),
        format!("Owner session: {}", snapshot.owner_session_id),
        format!("Command: {}", snapshot.command),
        format!("Shell: {}", snapshot.shell_label),
        format!("Backend: {}", snapshot.backend.as_str()),
        format!(
            "Working directory: {}",
            normalized_workspace_path_text_from_path(&snapshot.cwd)
        ),
        format!("Status: {}", snapshot.status.as_str()),
        format!("Output mode: {output_mode}"),
        format!("Last sequence: {}", snapshot.last_seq),
        format!("Returned chunks: {}", returned_chunks.len()),
        format!(
            "Has more buffered output: {}",
            if has_more { "yes" } else { "no" }
        ),
    ];
    if let Some(exit_code) = snapshot.exit_code {
        lines.push(format!("Session exit code: {exit_code}"));
    }
    if let Some(reason) = snapshot.failure_reason.as_deref() {
        lines.push(format!("Reason: {reason}"));
    }
    if let Some(transaction) = transaction {
        lines.push(format!("Transaction: {}", transaction.id));
        lines.push(format!("Transaction command: {}", transaction.command));
        lines.push(format!("Transaction status: {}", transaction.status));
        lines.push(format!(
            "Transaction exit code: {}",
            transaction
                .exit_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| String::from("unknown"))
        ));
    }
    let screen_text = snapshot
        .screen_text
        .clone()
        .or_else(|| terminal_screen_text_from_chunks(output_mode, returned_chunks));
    if screen_text.is_some() {
        lines.push(String::from("Screen text: present"));
    }
    if let Some(hint) = terminal_controls_hint_line(output_mode) {
        lines.push(format!("Controls hint: {hint}"));
    }
    lines.extend(extra_lines);
    let mut content = lines.join("\n");
    if let Some(screen_text) = screen_text {
        content.push_str("\n\n[screen_text]\n");
        content.push_str(&screen_text);
    }
    if output_mode == "screen" || output_mode == "mixed" {
        let controls_hint = serde_json::to_string_pretty(&terminal_controls_hint(output_mode))
            .unwrap_or_else(|_| terminal_controls_hint(output_mode).to_string());
        content.push_str("\n\n[controls_hint]\n");
        content.push_str(&controls_hint);
    }
    if returned_chunks.is_empty() {
        content.push_str("\n\n(no new output)");
    } else {
        for chunk in returned_chunks {
            content.push_str(&format!(
                "\n\n[{} #{}]\n{}",
                chunk.stream.as_str(),
                chunk.seq,
                chunk.text
            ));
        }
    }
    HostTextRenderInput {
        content_bytes: content.len(),
        content_lines: content.lines().count(),
        content,
        overflow_mode: Some(HostToolOverflowMode::Page),
        template_hint: None,
    }
}

/// Builds the host-only structured payload returned for one terminal-session tool action.
/// 为单次终端会话工具动作构建宿主专用结构化返回载荷。
pub(crate) fn build_terminal_session_host_payload(
    action: TerminalSessionToolAction,
    snapshot: &CommandSessionSnapshot,
    session_name: &str,
    returned_chunks: &[CommandSessionOutputChunk],
    output_overflowed: bool,
    output_mode: &str,
    output_mode_confidence: f64,
    output_mode_reason: String,
    transaction: Option<&TerminalTransactionResult>,
    extra_fields: Value,
) -> Value {
    let screen_text = snapshot
        .screen_text
        .clone()
        .or_else(|| terminal_screen_text_from_chunks(output_mode, returned_chunks));
    let mut payload = json!({
        "action": action.as_str(),
        "owner_session_id": snapshot.owner_session_id.to_string(),
        "terminal_session_id": snapshot.command_session_id.to_string(),
        "command_session_id": snapshot.command_session_id.to_string(),
        "name": session_name,
        "command": snapshot.command.clone(),
        "cwd": normalized_workspace_path_text_from_path(&snapshot.cwd),
        "shell": snapshot.shell_label.clone(),
        "launcher_shell": snapshot.launcher_shell_label.clone(),
        "backend": snapshot.backend.as_str(),
        "input_capabilities": terminal_input_capabilities(snapshot),
        "status": snapshot.status.as_str(),
        "exit_code": snapshot.exit_code,
        "stdout_bytes": snapshot.stdout_bytes,
        "stderr_bytes": snapshot.stderr_bytes,
        "last_seq": snapshot.last_seq,
        "pending_stdin_line": snapshot.pending_stdin_line,
        "screen_frame_seq": snapshot.screen_frame_seq,
        "screen_size": {
            "rows": snapshot.screen_rows,
            "cols": snapshot.screen_cols,
        },
        "failure_reason": snapshot.failure_reason.clone(),
        "output_overflowed": output_overflowed,
        "output_mode": output_mode,
        "output_mode_confidence": output_mode_confidence,
        "output_mode_reason": output_mode_reason,
        "events": terminal_events_from_chunks(returned_chunks),
        "screen_text": screen_text,
        "controls_hint": terminal_controls_hint(output_mode),
    });
    if let Some(transaction) = transaction
        && let Some(payload_object) = payload.as_object_mut()
    {
        payload_object.insert(
            String::from("transaction"),
            json!({
                "id": transaction.id,
                "command": transaction.command,
                "exit_code": transaction.exit_code,
                "status": transaction.status,
                "started_seq": transaction.started_seq,
                "ended_seq": transaction.ended_seq,
            }),
        );
    }
    if let (Some(payload_object), Value::Object(extra_object)) =
        (payload.as_object_mut(), extra_fields)
    {
        payload_object.extend(extra_object);
    }
    payload
}
