use super::*;

pub(crate) fn terminal_wait_mode(
    wait_policy: Option<&TerminalSessionWaitPolicy>,
) -> TerminalSessionWaitMode {
    wait_policy
        .map(|policy| policy.mode)
        .unwrap_or(TerminalSessionWaitMode::ReturnImmediately)
}

/// Returns the bounded output wait budget for one wait policy.
/// 返回一份等待策略对应的有界输出等待预算。
pub(crate) fn terminal_wait_budget_ms(
    wait_policy: Option<&TerminalSessionWaitPolicy>,
) -> Option<u64> {
    let Some(policy) = wait_policy else {
        return Some(0);
    };
    Some(match policy.mode {
        TerminalSessionWaitMode::ReturnImmediately => 0,
        TerminalSessionWaitMode::UntilQuiet => policy.max_wait_ms.unwrap_or_else(|| {
            policy
                .quiet_ms
                .unwrap_or(300)
                .saturating_mul(10)
                .clamp(500, 5_000)
        }),
        TerminalSessionWaitMode::UntilPrompt => policy.max_wait_ms.unwrap_or(3_000),
        TerminalSessionWaitMode::UntilExitCode => policy.max_wait_ms.unwrap_or(120_000),
        TerminalSessionWaitMode::UntilPattern => {
            let has_pattern = policy
                .pattern
                .as_deref()
                .map(str::trim)
                .is_some_and(|pattern| !pattern.is_empty());
            policy
                .max_wait_ms
                .unwrap_or(if has_pattern { 30_000 } else { 3_000 })
        }
        TerminalSessionWaitMode::ScreenStable => policy.max_wait_ms.unwrap_or_else(|| {
            policy
                .quiet_ms
                .unwrap_or(150)
                .saturating_mul(6)
                .clamp(300, 2_000)
        }),
    })
}

/// Returns the effective quiet window used by quiet-style terminal waits.
/// 返回静默类终端等待使用的有效静默窗口。
pub(crate) fn terminal_wait_quiet_window_ms(
    wait_policy: Option<&TerminalSessionWaitPolicy>,
) -> u64 {
    wait_policy
        .and_then(|policy| policy.quiet_ms)
        .unwrap_or(300)
        .clamp(50, 5_000)
}

/// Returns the trimmed pattern requested by one wait policy.
/// 返回一份等待策略请求的裁剪后模式。
pub(crate) fn terminal_wait_pattern_text(
    wait_policy: Option<&TerminalSessionWaitPolicy>,
) -> Option<String> {
    wait_policy
        .and_then(|policy| policy.pattern.as_deref())
        .map(str::trim)
        .filter(|pattern| !pattern.is_empty())
        .map(String::from)
}

pub(crate) async fn read_terminal_session_output_for_ai(
    command_session_runtime: &Arc<CommandSessionRuntime>,
    owner_session_id: SessionId,
    terminal_session_id: SessionId,
    after_seq: Option<u64>,
    read_from_start: bool,
    wait_for_output_ms: Option<u64>,
    max_chunks: Option<usize>,
) -> Result<CommandSessionReadOutcome, NativeToolError> {
    command_session_runtime
        .read_session_for_ai(
            owner_session_id,
            terminal_session_id,
            after_seq,
            read_from_start,
            wait_for_output_ms,
            max_chunks,
        )
        .await
        .map_err(NativeToolError::new)
}

/// Peeks terminal output without advancing the AI-facing cursor.
/// 窥视终端输出，但不推进面向 AI 的读取游标。
pub(crate) async fn peek_terminal_session_output_for_ai(
    command_session_runtime: &Arc<CommandSessionRuntime>,
    owner_session_id: SessionId,
    terminal_session_id: SessionId,
    after_seq: Option<u64>,
    read_from_start: bool,
    wait_for_output_ms: Option<u64>,
    max_chunks: Option<usize>,
) -> Result<CommandSessionReadOutcome, NativeToolError> {
    command_session_runtime
        .peek_session_for_ai(
            owner_session_id,
            terminal_session_id,
            after_seq,
            read_from_start,
            wait_for_output_ms,
            max_chunks,
        )
        .await
        .map_err(NativeToolError::new)
}

/// Returns remaining milliseconds for one bounded terminal wait loop.
/// 返回一次有界终端等待循环的剩余毫秒数。
pub(crate) fn remaining_terminal_wait_ms(started_at: Instant, wait_budget_ms: u64) -> u64 {
    wait_budget_ms.saturating_sub(started_at.elapsed().as_millis() as u64)
}

/// Reads terminal output according to the requested wait policy.
/// 按请求的等待策略读取终端输出。
pub(crate) async fn read_terminal_session_output_with_wait(
    command_session_runtime: &Arc<CommandSessionRuntime>,
    owner_session_id: SessionId,
    terminal_session_id: SessionId,
    after_seq: Option<u64>,
    read_from_start: bool,
    wait_policy: Option<&TerminalSessionWaitPolicy>,
    max_chunks: Option<usize>,
) -> Result<TerminalWaitReadResult, NativeToolError> {
    let wait_mode = terminal_wait_mode(wait_policy);
    let wait_budget_ms = terminal_wait_budget_ms(wait_policy).unwrap_or(0);
    let started_at = Instant::now();

    if wait_mode == TerminalSessionWaitMode::ReturnImmediately {
        let read_outcome = read_terminal_session_output_for_ai(
            command_session_runtime,
            owner_session_id,
            terminal_session_id,
            after_seq,
            read_from_start,
            Some(0),
            max_chunks,
        )
        .await?;
        return Ok(TerminalWaitReadResult {
            snapshot: read_outcome.snapshot,
            returned_chunks: read_outcome.returned_chunks,
            has_more: read_outcome.has_more,
            elapsed_ms: started_at.elapsed().as_millis() as u64,
            pattern_matched: None,
            quiet_observed: None,
        });
    }

    if wait_mode == TerminalSessionWaitMode::UntilPattern {
        let pattern = terminal_wait_pattern_text(wait_policy);
        let mut returned_chunks = Vec::new();
        let mut has_more = false;
        let mut latest_outcome = read_terminal_session_output_for_ai(
            command_session_runtime,
            owner_session_id,
            terminal_session_id,
            after_seq,
            read_from_start,
            Some(wait_budget_ms.min(500)),
            max_chunks,
        )
        .await?;
        returned_chunks.extend(latest_outcome.returned_chunks.clone());
        has_more |= latest_outcome.has_more;
        let mut pattern_matched = pattern.as_deref().is_some_and(|pattern| {
            terminal_output_chunks_contain_pattern(&returned_chunks, pattern)
        });

        while !pattern_matched && remaining_terminal_wait_ms(started_at, wait_budget_ms) > 0 {
            if latest_outcome.snapshot.status.as_str() != "running" {
                break;
            }
            let remaining_ms = remaining_terminal_wait_ms(started_at, wait_budget_ms);
            latest_outcome = read_terminal_session_output_for_ai(
                command_session_runtime,
                owner_session_id,
                terminal_session_id,
                None,
                false,
                Some(remaining_ms.min(500)),
                max_chunks,
            )
            .await?;
            returned_chunks.extend(latest_outcome.returned_chunks.clone());
            has_more |= latest_outcome.has_more;
            pattern_matched = pattern.as_deref().is_some_and(|pattern| {
                terminal_output_chunks_contain_pattern(&returned_chunks, pattern)
            });
            if latest_outcome.returned_chunks.is_empty() && remaining_ms <= 500 {
                break;
            }
        }

        return Ok(TerminalWaitReadResult {
            snapshot: latest_outcome.snapshot,
            returned_chunks,
            has_more,
            elapsed_ms: started_at.elapsed().as_millis() as u64,
            pattern_matched: Some(pattern_matched),
            quiet_observed: None,
        });
    }

    let quiet_ms = terminal_wait_quiet_window_ms(wait_policy);
    let mut returned_chunks = Vec::new();
    let mut has_more = false;
    let mut quiet_observed = false;
    let mut latest_outcome = read_terminal_session_output_for_ai(
        command_session_runtime,
        owner_session_id,
        terminal_session_id,
        after_seq,
        read_from_start,
        Some(quiet_ms.min(wait_budget_ms)),
        max_chunks,
    )
    .await?;
    loop {
        has_more |= latest_outcome.has_more;
        if latest_outcome.returned_chunks.is_empty() {
            quiet_observed = true;
            break;
        }
        returned_chunks.extend(latest_outcome.returned_chunks.clone());
        if latest_outcome.snapshot.status.as_str() != "running" {
            break;
        }
        let remaining_ms = remaining_terminal_wait_ms(started_at, wait_budget_ms);
        if remaining_ms == 0 {
            break;
        }
        latest_outcome = read_terminal_session_output_for_ai(
            command_session_runtime,
            owner_session_id,
            terminal_session_id,
            None,
            false,
            Some(quiet_ms.min(remaining_ms)),
            max_chunks,
        )
        .await?;
    }

    Ok(TerminalWaitReadResult {
        snapshot: latest_outcome.snapshot,
        returned_chunks,
        has_more,
        elapsed_ms: started_at.elapsed().as_millis() as u64,
        pattern_matched: None,
        quiet_observed: Some(quiet_observed),
    })
}

/// Drains residual output after Ctrl+C until the terminal is quiet or the bounded budget expires.
/// 在 Ctrl+C 后持续排空残留输出，直到终端静默或有界预算耗尽。
pub(crate) async fn drain_terminal_interrupt_output(
    command_session_runtime: &Arc<CommandSessionRuntime>,
    owner_session_id: SessionId,
    terminal_session_id: SessionId,
    after_seq: u64,
    max_chunks: Option<usize>,
) -> Result<TerminalInterruptDrainResult, NativeToolError> {
    let started_at = Instant::now();
    let mut next_after_seq = Some(after_seq);
    let mut returned_chunks = Vec::new();
    let mut discarded_chunk_count = 0usize;
    let mut latest_outcome = read_terminal_session_output_for_ai(
        command_session_runtime,
        owner_session_id,
        terminal_session_id,
        next_after_seq,
        false,
        Some(TERMINAL_INTERRUPT_DRAIN_QUIET_MS),
        max_chunks,
    )
    .await?;
    loop {
        for chunk in latest_outcome.returned_chunks.iter().cloned() {
            if terminal_interrupt_drain_chunk_is_visible(&chunk) {
                returned_chunks.push(chunk);
            } else {
                discarded_chunk_count = discarded_chunk_count.saturating_add(1);
            }
        }

        let quiet_after_wait = latest_outcome.returned_chunks.is_empty();
        if quiet_after_wait {
            break;
        }
        if latest_outcome.snapshot.status.as_str() != "running" {
            break;
        }
        if started_at.elapsed() >= Duration::from_millis(TERMINAL_INTERRUPT_DRAIN_MAX_MS) {
            break;
        }

        next_after_seq = None;
        latest_outcome = read_terminal_session_output_for_ai(
            command_session_runtime,
            owner_session_id,
            terminal_session_id,
            next_after_seq,
            false,
            Some(TERMINAL_INTERRUPT_DRAIN_QUIET_MS),
            max_chunks,
        )
        .await?;
    }

    Ok(TerminalInterruptDrainResult {
        snapshot: latest_outcome.snapshot,
        returned_chunks,
        has_more: latest_outcome.has_more,
        elapsed_ms: started_at.elapsed().as_millis() as u64,
        discarded_chunk_count,
    })
}

/// Flushes a possible leftover interrupt control character from the shell input line.
/// 从 shell 输入行中冲刷可能残留的中断控制字符。
pub(crate) async fn cleanup_terminal_interrupt_input_line(
    command_session_runtime: &Arc<CommandSessionRuntime>,
    owner_session_id: SessionId,
    terminal_session_id: SessionId,
    snapshot: CommandSessionSnapshot,
    interactive_newline: &str,
    max_chunks: Option<usize>,
) -> TerminalInterruptCleanupResult {
    if snapshot.status.as_str() != "running" || !snapshot.pending_stdin_line {
        return TerminalInterruptCleanupResult {
            snapshot,
            has_more: false,
            elapsed_ms: 0,
            discarded_chunk_count: 0,
            failure_reason: None,
        };
    }

    let started_at = Instant::now();
    let after_seq = snapshot.last_seq;
    let write_outcome = match command_session_runtime
        .write_session(
            owner_session_id,
            terminal_session_id,
            interactive_newline.to_string(),
            false,
        )
        .await
    {
        Ok(outcome) => outcome,
        Err(error) => {
            return TerminalInterruptCleanupResult {
                snapshot,
                has_more: false,
                elapsed_ms: started_at.elapsed().as_millis() as u64,
                discarded_chunk_count: 0,
                failure_reason: Some(error),
            };
        }
    };

    let mut discarded_chunk_count = 0usize;
    let mut has_more = false;
    let mut latest_snapshot = write_outcome.snapshot;
    let mut next_after_seq = Some(after_seq);
    loop {
        let read_outcome = match read_terminal_session_output_for_ai(
            command_session_runtime,
            owner_session_id,
            terminal_session_id,
            next_after_seq,
            false,
            Some(TERMINAL_INTERRUPT_DRAIN_QUIET_MS),
            max_chunks,
        )
        .await
        {
            Ok(outcome) => outcome,
            Err(error) => {
                return TerminalInterruptCleanupResult {
                    snapshot: latest_snapshot,
                    has_more,
                    elapsed_ms: started_at.elapsed().as_millis() as u64,
                    discarded_chunk_count,
                    failure_reason: Some(error.message),
                };
            }
        };
        let chunk_count = read_outcome.returned_chunks.len();
        discarded_chunk_count = discarded_chunk_count.saturating_add(chunk_count);
        has_more |= read_outcome.has_more;
        latest_snapshot = read_outcome.snapshot;
        if chunk_count == 0 {
            break;
        }
        if latest_snapshot.status.as_str() != "running" {
            break;
        }
        if started_at.elapsed() >= Duration::from_millis(TERMINAL_INTERRUPT_DRAIN_MAX_MS) {
            break;
        }
        next_after_seq = None;
    }

    TerminalInterruptCleanupResult {
        snapshot: latest_snapshot,
        has_more,
        elapsed_ms: started_at.elapsed().as_millis() as u64,
        discarded_chunk_count,
        failure_reason: None,
    }
}

/// Reads terminal output until a command transaction sentinel is observed or the wait budget expires.
/// 读取终端输出直到观测到命令事务哨兵或等待预算耗尽。
pub(crate) async fn read_terminal_transaction_output(
    command_session_runtime: &Arc<CommandSessionRuntime>,
    owner_session_id: SessionId,
    terminal_session_id: SessionId,
    after_seq: u64,
    wait_budget_ms: u64,
    max_chunks: Option<usize>,
    transaction_id: &str,
) -> Result<
    (
        CommandSessionReadOutcome,
        Vec<CommandSessionOutputChunk>,
        Option<i32>,
    ),
    NativeToolError,
> {
    let started_at = Instant::now();
    let mut all_chunks = Vec::new();
    let mut latest_outcome = read_terminal_session_output_for_ai(
        command_session_runtime,
        owner_session_id,
        terminal_session_id,
        Some(after_seq),
        false,
        Some(wait_budget_ms.min(500)),
        max_chunks,
    )
    .await?;
    all_chunks.extend(latest_outcome.returned_chunks.clone());
    let coalesced_chunks = coalesce_terminal_output_chunks(all_chunks.clone());
    let (stripped_chunks, exit_code) =
        strip_terminal_transaction_sentinel(&coalesced_chunks, transaction_id);
    if exit_code.is_some() {
        return Ok((latest_outcome, stripped_chunks, exit_code));
    }

    while started_at.elapsed().as_millis() < wait_budget_ms as u128 {
        let elapsed_ms = started_at.elapsed().as_millis() as u64;
        let remaining_ms = wait_budget_ms.saturating_sub(elapsed_ms);
        if remaining_ms == 0 {
            break;
        }
        latest_outcome = read_terminal_session_output_for_ai(
            command_session_runtime,
            owner_session_id,
            terminal_session_id,
            None,
            false,
            Some(remaining_ms.min(500)),
            max_chunks,
        )
        .await?;
        if latest_outcome.returned_chunks.is_empty() {
            if latest_outcome.snapshot.status.as_str() != "running" {
                break;
            }
            continue;
        }
        all_chunks.extend(latest_outcome.returned_chunks.clone());
        let coalesced_chunks = coalesce_terminal_output_chunks(all_chunks.clone());
        let (stripped_chunks, exit_code) =
            strip_terminal_transaction_sentinel(&coalesced_chunks, transaction_id);
        if exit_code.is_some() {
            return Ok((latest_outcome, stripped_chunks, exit_code));
        }
    }

    let coalesced_chunks = coalesce_terminal_output_chunks(all_chunks);
    let (stripped_chunks, exit_code) =
        strip_terminal_transaction_sentinel(&coalesced_chunks, transaction_id);
    Ok((latest_outcome, stripped_chunks, exit_code))
}
