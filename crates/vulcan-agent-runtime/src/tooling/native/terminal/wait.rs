use super::*;

/// Quiet window used when an `until_quiet` wait omits `quiet_ms`.
/// `until_quiet` 等待省略 `quiet_ms` 时使用的静默窗口。
const DEFAULT_TERMINAL_WAIT_QUIET_MS: u64 = 300;

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
    // requestedBudgetMs preserves mode-specific defaults before applying the shared host hard limit.
    // requestedBudgetMs 在应用共享宿主硬上限前保留各等待模式的默认值。
    let requested_budget_ms = match policy.mode {
        TerminalSessionWaitMode::ReturnImmediately => 0,
        TerminalSessionWaitMode::UntilQuiet => policy.max_wait_ms.unwrap_or_else(|| {
            policy
                .quiet_ms
                .unwrap_or(DEFAULT_TERMINAL_WAIT_QUIET_MS)
                .saturating_mul(10)
                .clamp(500, 5_000)
        }),
        TerminalSessionWaitMode::UntilPrompt => policy.max_wait_ms.unwrap_or(3_000),
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
    };
    Some(requested_budget_ms.min(MAX_COMMAND_SESSION_READ_WAIT_MS))
}

/// Returns the total output-chunk budget shared by every poll in one terminal read.
/// 返回一次终端读取中所有轮询共同使用的输出分片总预算。
pub(crate) fn terminal_read_chunk_budget(max_chunks: Option<usize>) -> usize {
    max_chunks
        .unwrap_or(DEFAULT_COMMAND_SESSION_READ_MAX_CHUNKS)
        .clamp(1, MAX_COMMAND_SESSION_READ_CHUNKS)
}

/// Returns the unconsumed output-chunk budget for one bounded terminal read.
/// 返回一次有界终端读取尚未消耗的输出分片预算。
fn remaining_terminal_read_chunks(total_budget: usize, consumed_chunks: usize) -> usize {
    total_budget.saturating_sub(consumed_chunks)
}

/// Returns the effective quiet window used by quiet-style terminal waits.
/// 返回静默类终端等待使用的有效静默窗口。
pub(crate) fn terminal_wait_quiet_window_ms(
    wait_policy: Option<&TerminalSessionWaitPolicy>,
) -> u64 {
    wait_policy
        .and_then(|policy| policy.quiet_ms)
        .unwrap_or(DEFAULT_TERMINAL_WAIT_QUIET_MS)
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
    let chunk_budget = terminal_read_chunk_budget(max_chunks);
    let started_at = Instant::now();

    if wait_mode == TerminalSessionWaitMode::ReturnImmediately {
        let read_outcome = read_terminal_session_output_for_ai(
            command_session_runtime,
            owner_session_id,
            terminal_session_id,
            after_seq,
            read_from_start,
            Some(0),
            Some(chunk_budget),
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
            Some(chunk_budget),
        )
        .await?;
        returned_chunks.append(&mut latest_outcome.returned_chunks);
        has_more |= latest_outcome.has_more;
        let mut pattern_matched = pattern.as_deref().is_some_and(|pattern| {
            terminal_output_chunks_contain_pattern(&returned_chunks, pattern)
        });

        while !pattern_matched
            && returned_chunks.len() < chunk_budget
            && remaining_terminal_wait_ms(started_at, wait_budget_ms) > 0
        {
            if latest_outcome.snapshot.status.as_str() != "running" {
                break;
            }
            let remaining_ms = remaining_terminal_wait_ms(started_at, wait_budget_ms);
            let remaining_chunks =
                remaining_terminal_read_chunks(chunk_budget, returned_chunks.len());
            if remaining_chunks == 0 {
                break;
            }
            latest_outcome = read_terminal_session_output_for_ai(
                command_session_runtime,
                owner_session_id,
                terminal_session_id,
                None,
                false,
                Some(remaining_ms.min(500)),
                Some(remaining_chunks),
            )
            .await?;
            let returned_chunk_count = latest_outcome.returned_chunks.len();
            returned_chunks.append(&mut latest_outcome.returned_chunks);
            has_more |= latest_outcome.has_more;
            pattern_matched = pattern.as_deref().is_some_and(|pattern| {
                terminal_output_chunks_contain_pattern(&returned_chunks, pattern)
            });
            if returned_chunk_count == 0 && remaining_ms <= 500 {
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
    // read_wait_ms is how long the latest read listened; only a full quiet window proves quiet.
    // read_wait_ms 是最近一次读取的监听时长；只有完整的静默窗口才能证明已静默。
    let mut read_wait_ms = quiet_ms.min(wait_budget_ms);
    let mut latest_outcome = read_terminal_session_output_for_ai(
        command_session_runtime,
        owner_session_id,
        terminal_session_id,
        after_seq,
        read_from_start,
        Some(read_wait_ms),
        Some(chunk_budget),
    )
    .await?;
    loop {
        has_more |= latest_outcome.has_more;
        let returned_chunk_count = latest_outcome.returned_chunks.len();
        if returned_chunk_count == 0 {
            quiet_observed = read_wait_ms >= quiet_ms;
            break;
        }
        returned_chunks.append(&mut latest_outcome.returned_chunks);
        if returned_chunks.len() >= chunk_budget {
            break;
        }
        if latest_outcome.snapshot.status.as_str() != "running" {
            break;
        }
        let remaining_ms = remaining_terminal_wait_ms(started_at, wait_budget_ms);
        if remaining_ms == 0 {
            break;
        }
        let remaining_chunks = remaining_terminal_read_chunks(chunk_budget, returned_chunks.len());
        if remaining_chunks == 0 {
            break;
        }
        read_wait_ms = quiet_ms.min(remaining_ms);
        latest_outcome = read_terminal_session_output_for_ai(
            command_session_runtime,
            owner_session_id,
            terminal_session_id,
            None,
            false,
            Some(read_wait_ms),
            Some(remaining_chunks),
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
    let chunk_budget = terminal_read_chunk_budget(max_chunks);
    let mut consumed_chunk_count = 0usize;
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
        Some(chunk_budget),
    )
    .await?;
    loop {
        let returned_chunk_count = latest_outcome.returned_chunks.len();
        consumed_chunk_count = consumed_chunk_count.saturating_add(returned_chunk_count);
        for chunk in latest_outcome.returned_chunks.drain(..) {
            if terminal_interrupt_drain_chunk_is_visible(&chunk) {
                returned_chunks.push(chunk);
            } else {
                discarded_chunk_count = discarded_chunk_count.saturating_add(1);
            }
        }

        let quiet_after_wait = returned_chunk_count == 0;
        if quiet_after_wait {
            break;
        }
        if consumed_chunk_count >= chunk_budget {
            break;
        }
        if latest_outcome.snapshot.status.as_str() != "running" {
            break;
        }
        if started_at.elapsed() >= Duration::from_millis(TERMINAL_INTERRUPT_DRAIN_MAX_MS) {
            break;
        }

        next_after_seq = None;
        let remaining_chunks = remaining_terminal_read_chunks(chunk_budget, consumed_chunk_count);
        if remaining_chunks == 0 {
            break;
        }
        latest_outcome = read_terminal_session_output_for_ai(
            command_session_runtime,
            owner_session_id,
            terminal_session_id,
            next_after_seq,
            false,
            Some(TERMINAL_INTERRUPT_DRAIN_QUIET_MS),
            Some(remaining_chunks),
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
    let chunk_budget = terminal_read_chunk_budget(max_chunks);
    let mut consumed_chunk_count = 0usize;
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
        let remaining_chunks = remaining_terminal_read_chunks(chunk_budget, consumed_chunk_count);
        if remaining_chunks == 0 {
            break;
        }
        let read_outcome = match read_terminal_session_output_for_ai(
            command_session_runtime,
            owner_session_id,
            terminal_session_id,
            next_after_seq,
            false,
            Some(TERMINAL_INTERRUPT_DRAIN_QUIET_MS),
            Some(remaining_chunks),
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
        consumed_chunk_count = consumed_chunk_count.saturating_add(chunk_count);
        discarded_chunk_count = discarded_chunk_count.saturating_add(chunk_count);
        has_more |= read_outcome.has_more;
        latest_snapshot = read_outcome.snapshot;
        if chunk_count == 0 {
            break;
        }
        if consumed_chunk_count >= chunk_budget {
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
