use super::*;

pub(crate) async fn execute_terminal_session_read_like_action(
    request: NativeToolExecutionRequest,
    parsed_args: TerminalSessionToolArgs,
    owner_session_id: SessionId,
    command_session_runtime: Arc<CommandSessionRuntime>,
    action: TerminalSessionToolAction,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let terminal_session_id =
        parse_terminal_session_id(parsed_args.terminal_session_id.as_deref())?;
    let spill_root = resolve_shell_command_spill_root(
        request.spill_root.as_deref(),
        request.workspace_root.as_deref(),
        request
            .workspace_root
            .as_deref()
            .unwrap_or_else(|| Path::new(".")),
    );
    let read_from_start = parsed_args.read_from_start.unwrap_or(false);
    let wait_result = if matches!(action, TerminalSessionToolAction::Snapshot) {
        let read_outcome = peek_terminal_session_output_for_ai(
            &command_session_runtime,
            owner_session_id,
            terminal_session_id,
            parsed_args.after_seq,
            true,
            Some(0),
            parsed_args.max_chunks,
        )
        .await?;
        TerminalWaitReadResult {
            snapshot: read_outcome.snapshot,
            returned_chunks: Vec::new(),
            has_more: read_outcome.has_more,
            elapsed_ms: 0,
            pattern_matched: None,
            quiet_observed: None,
        }
    } else {
        read_terminal_session_output_with_wait(
            &command_session_runtime,
            owner_session_id,
            terminal_session_id,
            parsed_args.after_seq,
            read_from_start,
            parsed_args.wait_policy.as_ref(),
            parsed_args.max_chunks,
        )
        .await?
    };
    let session_name = terminal_session_name_for_snapshot(&wait_result.snapshot);
    let returned_chunks = coalesce_terminal_output_chunks(wait_result.returned_chunks.clone());
    let (output_mode, confidence, reason) = detect_terminal_output_mode(&returned_chunks);
    let render_input = build_terminal_session_render_input(
        action,
        &wait_result.snapshot,
        &session_name,
        &returned_chunks,
        wait_result.has_more,
        output_mode,
        None,
        vec![
            format!(
                "Wait mode: {}",
                terminal_wait_mode(parsed_args.wait_policy.as_ref()).as_str()
            ),
            format!("Wait elapsed: {} ms", wait_result.elapsed_ms),
            format!(
                "Pattern matched: {}",
                wait_result
                    .pattern_matched
                    .map(|matched| matched.to_string())
                    .unwrap_or_else(|| String::from("n/a"))
            ),
            format!(
                "Quiet observed: {}",
                wait_result
                    .quiet_observed
                    .map(|observed| observed.to_string())
                    .unwrap_or_else(|| String::from("n/a"))
            ),
        ],
    );
    let budget = default_shell_command_budget_snapshot();
    let host_payload = build_terminal_session_host_payload(
        action,
        &wait_result.snapshot,
        &session_name,
        &returned_chunks,
        shell_command_output_exceeds_inline_budget(&render_input, &budget),
        output_mode,
        confidence,
        reason,
        None,
        json!({
            "requested_after_seq": parsed_args.after_seq.unwrap_or(0),
            "read_from_start": read_from_start,
            "returned_chunk_count": returned_chunks.len(),
            "has_more": wait_result.has_more,
            "wait_elapsed_ms": wait_result.elapsed_ms,
            "pattern_matched": wait_result.pattern_matched,
            "quiet_observed": wait_result.quiet_observed,
            "snapshot_advanced_ai_cursor": !matches!(action, TerminalSessionToolAction::Snapshot),
        }),
    );
    let model_output = render_tool_result_text(
        &render_input,
        None,
        &budget,
        &HostRenderOptions {
            spill_root,
            template_skill_roots: Vec::new(),
            template_resources_root: None,
        },
    )
    .map_err(NativeToolError::new)?;
    Ok(
        VulcanToolInvocationResult::from_model_output(Value::String(model_output))
            .with_host_result(VulcanToolHostResult {
                kind: String::from("terminal_session"),
                payload: host_payload,
            }),
    )
}

/// Executes one terminal-session send action.
/// 执行一次终端会话 send 动作。
pub(crate) async fn execute_terminal_session_send_action(
    request: NativeToolExecutionRequest,
    parsed_args: TerminalSessionToolArgs,
    owner_session_id: SessionId,
    command_session_runtime: Arc<CommandSessionRuntime>,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let terminal_session_id =
        parse_terminal_session_id(parsed_args.terminal_session_id.as_deref())?;
    let before_snapshot = command_session_runtime
        .try_snapshot_owned_session(owner_session_id, terminal_session_id)
        .ok_or_else(|| {
            NativeToolError::new(format!(
                "Terminal session `{terminal_session_id}` was not found."
            ))
        })?;
    let session_name = terminal_session_name_for_snapshot(&before_snapshot);
    let inputs = parsed_args.inputs.clone().ok_or_else(|| {
        NativeToolError::new("The `send` action for terminal_session requires `inputs`.")
    })?;
    if inputs.is_empty() {
        return Err(NativeToolError::new(
            "The `inputs` array for terminal_session send cannot be empty.",
        ));
    }
    let wait_mode = terminal_wait_mode(parsed_args.wait_policy.as_ref());
    let interactive_newline = terminal_session_pty_newline(before_snapshot.shell_label.as_str());
    let key_encoding_label = "ansi_vt";
    let mut structured_input = terminal_input_events_from_inputs(&inputs);
    let interrupt_requested = terminal_inputs_request_interrupt(&inputs);
    let mut transaction = None;
    if wait_mode == TerminalSessionWaitMode::UntilExitCode {
        validate_terminal_transaction_start(&before_snapshot)?;
        let command = terminal_transaction_command_text(&inputs)?;
        let transaction_id = Uuid::new_v4().to_string();
        let wrapped_command = wrap_terminal_transaction_command(
            &command,
            before_snapshot.shell_label.as_str(),
            transaction_id.as_str(),
            terminal_generated_command_newline(before_snapshot.shell_label.as_str()),
        )?;
        structured_input = vec![CommandSessionInputEvent::Text {
            text: wrapped_command,
        }];
        transaction = Some(TerminalTransactionResult {
            id: transaction_id,
            command,
            exit_code: None,
            status: String::from("pending"),
            started_seq: before_snapshot.last_seq,
            ended_seq: None,
        });
    }
    let write_outcome = command_session_runtime
        .write_session_input_events(owner_session_id, terminal_session_id, structured_input)
        .await
        .map_err(NativeToolError::new)?;
    let requested_wait_budget =
        terminal_wait_budget_ms(parsed_args.wait_policy.as_ref()).unwrap_or(0);
    let wait_budget = requested_wait_budget;
    let mut interrupt_drain_elapsed_ms = 0u64;
    let mut interrupt_drain_discarded_chunks = 0usize;
    let mut interrupt_cleanup_elapsed_ms = 0u64;
    let mut interrupt_cleanup_discarded_chunks = 0usize;
    let mut interrupt_cleanup_failure: Option<String> = None;
    let response_wait_started_at = Instant::now();
    let (
        read_snapshot,
        returned_chunks,
        has_more,
        wait_elapsed_ms,
        pattern_matched,
        quiet_observed,
    ) = if let Some(transaction_ref) = transaction.as_mut() {
        let (read_outcome, stripped_chunks, exit_code) = read_terminal_transaction_output(
            &command_session_runtime,
            owner_session_id,
            terminal_session_id,
            before_snapshot.last_seq,
            wait_budget,
            parsed_args.max_chunks,
            transaction_ref.id.as_str(),
        )
        .await?;
        transaction_ref.exit_code = exit_code;
        transaction_ref.status = if exit_code.is_some() {
            String::from("completed")
        } else {
            String::from("timeout")
        };
        transaction_ref.ended_seq = read_outcome
            .returned_chunks
            .last()
            .map(|chunk| chunk.seq)
            .or(Some(read_outcome.snapshot.last_seq));
        (
            read_outcome.snapshot,
            stripped_chunks,
            read_outcome.has_more,
            response_wait_started_at.elapsed().as_millis() as u64,
            None,
            None,
        )
    } else if interrupt_requested {
        let drain_result = drain_terminal_interrupt_output(
            &command_session_runtime,
            owner_session_id,
            terminal_session_id,
            before_snapshot.last_seq,
            parsed_args.max_chunks,
        )
        .await?;
        interrupt_drain_elapsed_ms = drain_result.elapsed_ms;
        interrupt_drain_discarded_chunks = drain_result.discarded_chunk_count;
        let cleanup_result = cleanup_terminal_interrupt_input_line(
            &command_session_runtime,
            owner_session_id,
            terminal_session_id,
            drain_result.snapshot.clone(),
            interactive_newline,
            parsed_args.max_chunks,
        )
        .await;
        interrupt_cleanup_elapsed_ms = cleanup_result.elapsed_ms;
        interrupt_cleanup_discarded_chunks = cleanup_result.discarded_chunk_count;
        interrupt_cleanup_failure = cleanup_result.failure_reason.clone();
        (
            cleanup_result.snapshot,
            drain_result.returned_chunks,
            drain_result.has_more || cleanup_result.has_more,
            interrupt_drain_elapsed_ms.saturating_add(interrupt_cleanup_elapsed_ms),
            None,
            None,
        )
    } else {
        let wait_result = read_terminal_session_output_with_wait(
            &command_session_runtime,
            owner_session_id,
            terminal_session_id,
            Some(before_snapshot.last_seq),
            false,
            parsed_args.wait_policy.as_ref(),
            parsed_args.max_chunks,
        )
        .await?;
        (
            wait_result.snapshot,
            wait_result.returned_chunks,
            wait_result.has_more,
            wait_result.elapsed_ms,
            wait_result.pattern_matched,
            wait_result.quiet_observed,
        )
    };
    let returned_chunks = coalesce_terminal_output_chunks(returned_chunks);
    let (output_mode, confidence, reason) = detect_terminal_output_mode(&returned_chunks);
    let spill_root = resolve_shell_command_spill_root(
        request.spill_root.as_deref(),
        request.workspace_root.as_deref(),
        request
            .workspace_root
            .as_deref()
            .unwrap_or_else(|| Path::new(".")),
    );
    let input_labels = inputs
        .iter()
        .map(terminal_input_label)
        .collect::<Vec<_>>()
        .join(", ");
    let render_input = build_terminal_session_render_input(
        TerminalSessionToolAction::Send,
        &read_snapshot,
        &session_name,
        &returned_chunks,
        has_more,
        output_mode,
        transaction.as_ref(),
        vec![
            format!("Inputs: {input_labels}"),
            format!("Key encoding: {key_encoding_label}"),
            format!("Bytes written: {}", write_outcome.bytes_written),
            format!("Wait mode: {}", wait_mode.as_str()),
            format!("Wait elapsed: {wait_elapsed_ms} ms"),
            format!(
                "Pattern matched: {}",
                pattern_matched
                    .map(|matched| matched.to_string())
                    .unwrap_or_else(|| String::from("n/a"))
            ),
            format!(
                "Quiet observed: {}",
                quiet_observed
                    .map(|observed| observed.to_string())
                    .unwrap_or_else(|| String::from("n/a"))
            ),
            format!(
                "Interrupt drain: {}",
                if interrupt_requested { "yes" } else { "no" }
            ),
            format!("Interrupt drain elapsed ms: {interrupt_drain_elapsed_ms}"),
            format!("Interrupt drain discarded chunks: {interrupt_drain_discarded_chunks}"),
            format!("Interrupt cleanup elapsed ms: {interrupt_cleanup_elapsed_ms}"),
            format!("Interrupt cleanup discarded chunks: {interrupt_cleanup_discarded_chunks}"),
            format!(
                "Interrupt cleanup failure: {}",
                interrupt_cleanup_failure.as_deref().unwrap_or("none")
            ),
        ],
    );
    let budget = default_shell_command_budget_snapshot();
    let host_payload = build_terminal_session_host_payload(
        TerminalSessionToolAction::Send,
        &read_snapshot,
        &session_name,
        &returned_chunks,
        shell_command_output_exceeds_inline_budget(&render_input, &budget),
        output_mode,
        confidence,
        reason,
        transaction.as_ref(),
        json!({
            "input_labels": input_labels,
            "key_encoding": key_encoding_label,
            "bytes_written": write_outcome.bytes_written,
            "wait_mode": wait_mode.as_str(),
            "wait_elapsed_ms": wait_elapsed_ms,
            "pattern_matched": pattern_matched,
            "quiet_observed": quiet_observed,
            "interrupt_drain_ms": interrupt_drain_elapsed_ms,
            "interrupt_drain_discarded_chunks": interrupt_drain_discarded_chunks,
            "interrupt_cleanup_ms": interrupt_cleanup_elapsed_ms,
            "interrupt_cleanup_discarded_chunks": interrupt_cleanup_discarded_chunks,
            "interrupt_cleanup_failure": interrupt_cleanup_failure,
            "returned_chunk_count": returned_chunks.len(),
            "has_more": has_more,
        }),
    );
    let model_output = render_tool_result_text(
        &render_input,
        None,
        &budget,
        &HostRenderOptions {
            spill_root,
            template_skill_roots: Vec::new(),
            template_resources_root: None,
        },
    )
    .map_err(NativeToolError::new)?;
    Ok(
        VulcanToolInvocationResult::from_model_output(Value::String(model_output))
            .with_host_result(VulcanToolHostResult {
                kind: String::from("terminal_session"),
                payload: host_payload,
            }),
    )
}
