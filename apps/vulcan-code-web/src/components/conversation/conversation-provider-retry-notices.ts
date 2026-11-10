import type { ConversationEventBody, ConversationEventSummary } from "@/service/conversation-event-window-store";
import { parseSystemProviderError } from "./conversation-system-error-card";

// RetryApiStats is the durable result of one physical Provider request.
// RetryApiStats 是一次物理供应商请求的持久化结果。
type RetryApiStats = Extract<ConversationEventBody, { kind: "api_stats" }>;

// RetryFailureAttempt links one numbered attempt to its exact terminal facts and notice.
// RetryFailureAttempt 将一次编号尝试关联到其精确终态事实和提示。
export interface RetryFailureAttempt {
  // One-based request number, or null if the loaded window does not establish the first attempt.
  // 从一开始的请求编号；已加载窗口无法确认首次尝试时为空。
  attempt: number | null;
  // Terminal request facts that own the diagnostic.
  // 拥有诊断信息的请求终态事实。
  stats: RetryApiStats;
  // Original Core failure notice, retained at its canonical sequence.
  // 保留规范序号的 Core 原始失败提示。
  notice: ConversationEventSummary & { event: Extract<ConversationEventBody, { kind: "display_note" }> };
}

// projectProviderRetryNotices takes the ordered event window and returns presentation overrides by notice sequence.
// projectProviderRetryNotices 接收有序事件窗口，按提示序号返回展示覆盖项。
// A null override hides the notice; an array supplies every page of a repeated failure.
// 空值覆盖隐藏提示，数组则提供重复失败的全部分页。
export function projectProviderRetryNotices(
  events: readonly ConversationEventSummary[],
): ReadonlyMap<number, readonly RetryFailureAttempt[] | null> {
  // Only notices with a proven retry relationship receive a display override.
  // 只有重试关系已证实的提示才获得展示覆盖项。
  const overrides = new Map<number, readonly RetryFailureAttempt[] | null>();
  // The sequence may contain distinct failures, all of which disappear after a successful retry.
  // 同一重试序列可包含不同的失败，重试成功后隐藏该序列中的全部失败。
  let failures: RetryFailureAttempt[] = [];
  // A page group contains only consecutive failures with the same diagnostic.
  // 分页组仅包含诊断相同的连续失败。
  let repeated: RetryFailureAttempt[] = [];
  // A preceding success proves a fresh sequence even when a resumed execution uses the retry purpose.
  // 即使恢复后的执行采用重试用途，前一次成功也能证明接下来是新序列。
  let previousCompleted = false;
  // A history page can start in the middle of a retry sequence, where the absolute number is unknown.
  // 历史分页可能从重试序列中间开始，此时绝对编号未知。
  let firstAttemptKnown = false;
  for (const [index, summary] of events.entries()) {
    const { event } = summary;
    if ((index > 0 && summary.seq !== events[index - 1].seq + 1)
      || event.kind === "turn_boundary"
      || (event.kind === "execution_control" && (event.control_kind === "turn_continue_requested"
        || event.control_kind === "turn_resumed" || event.control_kind === "execution_retry_requested"))) {
      failures = [];
      repeated = [];
      previousCompleted = false;
    }
    if (event.kind !== "api_stats") continue;

    // The runtime marks automatic business retries as retry_generation. Manual Continue has its own
    // admission boundary and the first physical request is marked continue.
    // 运行时将业务请求的自动重试标为 retry_generation。手动继续有独立的准入边界，且首个物理请求标为 continue。
    const previous = failures.at(-1);
    // A retry follows an unexhausted failure on the same turn and route.
    // 重试接续同一回合及路由中尚未耗尽重试次数的失败。
    const continues = event.purpose === "retry_generation" && previous !== undefined
      && previous.notice.turn_id === summary.turn_id && previous.stats.end_reason !== "retry_exhausted"
      && sameProviderRoute(previous.stats, event);
    if (!continues) {
      failures = [];
      repeated = [];
      firstAttemptKnown = event.purpose !== "retry_generation" || previousCompleted;
    }
    previousCompleted = event.status === "completed";
    if (event.status === "completed") {
      for (const failure of failures) overrides.set(failure.notice.seq, null);
    }
    // Core writes a failure notice immediately after its terminal statistics in the same mutation.
    // Core 在同一次变更中紧接终态统计写入失败提示。
    const notice = events[index + 1];
    if (event.status !== "failed" || event.error === undefined || summary.turn_id === null
      || event.purpose === "compaction" || event.purpose === "question_execution"
      || notice?.seq !== summary.seq + 1 || notice.turn_id !== summary.turn_id
      || notice.event.kind !== "display_note" || notice.event.api_call_id !== event.api_call_id
      || notice.event.redacted_detail !== event.error.message) {
      failures = [];
      repeated = [];
      continue;
    }
    // Preserve attempt numbering across a change in error, even though that error gets a separate card.
    // 即使错误变化需要单独显示卡片，也继续按同一序列为尝试编号。
    const failure: RetryFailureAttempt = {
      attempt: firstAttemptKnown ? failures.length + 1 : null,
      stats: event,
      notice: { ...notice, event: notice.event },
    };
    if (continues && sameFailure(previous.stats, event)) {
      repeated.push(failure);
      overrides.set(repeated[0].notice.seq, repeated);
      overrides.set(notice.seq, null);
    } else {
      repeated = [failure];
    }
    failures.push(failure);
  }
  return overrides;
}

// sameProviderRoute checks whether two API results share the exact recorded Provider route.
// sameProviderRoute 检查两个 API 结果是否使用同一条已记录的供应商路由。
function sameProviderRoute(first: RetryApiStats, second: RetryApiStats): boolean {
  return first.provider === second.provider && first.model === second.model
    && first.profile === second.profile && first.connection_id === second.connection_id;
}

// sameFailure compares the two durable error classifications and their diagnostic messages.
// sameFailure 比较两个持久化错误的分类及诊断消息。
function sameFailure(first: RetryApiStats, second: RetryApiStats): boolean {
  // Failed API statistics always carry a redacted error. Other statuses cannot supply a page.
  // 失败的 API 统计始终携带脱敏错误。其他状态不能提供分页。
  const firstError = first.error;
  const secondError = second.error;
  if (!firstError || !secondError || firstError.category !== secondError.category
    || firstError.error_code !== secondError.error_code) return false;
  if (firstError.message === secondError.message) return true;

  // Structured diagnostics may differ only in per-attempt request IDs or retry delays.
  // 结构化诊断可能只在每次尝试的请求编号或重试延时上不同。
  const firstDiagnostic = parseSystemProviderError(firstError.message);
  const secondDiagnostic = parseSystemProviderError(secondError.message);
  return firstDiagnostic !== null && secondDiagnostic !== null
    && firstDiagnostic.category === secondDiagnostic.category && firstDiagnostic.stage === secondDiagnostic.stage
    && firstDiagnostic.dispatchOutcome === secondDiagnostic.dispatchOutcome
    && firstDiagnostic.semanticOutputObserved === secondDiagnostic.semanticOutputObserved
    && firstDiagnostic.sideEffectObserved === secondDiagnostic.sideEffectObserved
    && firstDiagnostic.httpStatus === secondDiagnostic.httpStatus && firstDiagnostic.subcategory === secondDiagnostic.subcategory
    && firstDiagnostic.upstreamCode === secondDiagnostic.upstreamCode && firstDiagnostic.upstreamType === secondDiagnostic.upstreamType
    && firstDiagnostic.upstreamParameter === secondDiagnostic.upstreamParameter
    && firstDiagnostic.message === secondDiagnostic.message && firstDiagnostic.upstreamMessage === secondDiagnostic.upstreamMessage;
}
