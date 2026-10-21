import type { SessionDraftRpcV1 } from "@/service/conversation-session-drafts-v1";
import type { ComposerAttachmentSummary } from "@/types";

/** SESSION_DRAFT_WRITE_INTERVAL_MS is the least time between two writes of one session's draft while typing. The
 * service answers within milliseconds, so coalescing only behind a running request still sent one write per
 * keystroke, each carrying the whole draft. A flush, such as before sending or switching, never waits for it.
 * SESSION_DRAFT_WRITE_INTERVAL_MS 是输入期间同一会话草稿两次写入之间的最短间隔。服务在数毫秒内应答，仅在请求进行中
 * 合并仍会每次按键写入一次且每次携带整份草稿。flush（例如发送或切换之前）从不等待该间隔。
 */
export const SESSION_DRAFT_WRITE_INTERVAL_MS = 500;

/** Immutable payload queued for one authoritative session-draft replacement.
 * 排队等待权威会话草稿替换的一份不可变载荷。
 */
interface PendingSessionDraftWriteV1 {
  sessionId: string;
  text: string;
  attachments: ComposerAttachmentSummary[];
  /** Whether this payload may wait for the interval, as typing may; any other change is written at once.
   * 该载荷是否可以等待间隔（输入即可）；其他任何修改都立即写入。
   */
  coalesce: boolean;
}

/** Mutable per-session drain state owned only by the coordinator.
 * 仅由协调器持有的单会话可变排空状态。
 */
interface SessionDraftWriteEntryV1 {
  pending: PendingSessionDraftWriteV1 | null;
  running: Promise<void> | null;
  lastError: unknown;
  /** When the last write started; the next one waits until the interval has passed.
   * 上一次写入开始的时间；下一次写入等到间隔过去为止。
   */
  lastWriteAt: number;
  /** Ends a wait for the interval early, set only while the writer waits.
   * 提前结束间隔等待；仅在 writer 等待期间设置。
   */
  wake: (() => void) | null;
  /** Number of flushes waiting; while any wait, writes do not wait for the interval.
   * 正在等待的 flush 数量；只要存在，写入就不等待间隔。
   */
  flushes: number;
}

/** Coalesces typing bursts while preserving one serialized writer per materialized session: the first change
 * after a quiet moment is written at once, and later ones at most once per interval, always the newest draft.
 * 合并连续输入突发，同时为每个已物化会话保留一个串行 writer：安静片刻后的首次修改立即写入，之后每个间隔至多写入一次，
 * 且总是最新草稿。
 */
export class SessionDraftWriteCoordinatorV1 {
  /** Per-session writer state; sessions never share ordering authority.
   * 单会话 writer 状态；不同会话绝不共享顺序权威。
   */
  private readonly entries = new Map<string, SessionDraftWriteEntryV1>();
  /** Disposal fence preventing late UI owners from scheduling new writes.
   * 阻止迟到 UI owner 再次排入写入的释放栅栏。
   */
  private disposed = false;

  /** Creates one coordinator over the exact V1 draft transport.
   * 基于精确 V1 草稿传输创建一个协调器。
   */
  constructor(
    private readonly client: SessionDraftRpcV1,
    private readonly intervalMs: number = SESSION_DRAFT_WRITE_INTERVAL_MS,
  ) {}

  /** Replaces the pending payload with the newest complete draft and starts draining. A typing payload
   * (`coalesce`) waits while a write started within the interval; any other payload is written at once.
   * 用最新完整草稿替换待写载荷并开始排空。输入载荷（`coalesce`）在间隔内已开始过写入时等待；其他载荷立即写入。
   */
  enqueue(
    sessionId: string,
    text: string,
    attachments: readonly ComposerAttachmentSummary[],
    coalesce = false,
  ): void {
    if (this.disposed) {
      throw new Error("Session draft writer has been disposed.");
    }
    const entry = this.entry(sessionId);
    entry.pending = {
      sessionId,
      text,
      attachments: attachments.map((attachment) => ({ ...attachment })),
      coalesce,
    };
    if (!coalesce) {
      entry.wake?.();
    }
    if (entry.running === null) {
      entry.running = this.drain(entry);
    }
  }

  /** Waits until every payload queued for one session is durably acknowledged.
   * 等待一个会话排入的全部载荷获得可靠确认。
   */
  async flush(sessionId: string): Promise<void> {
    const entry = this.entries.get(sessionId);
    if (!entry) {
      return;
    }
    entry.flushes += 1;
    entry.wake?.();
    try {
      while (entry.running !== null) {
        await entry.running;
      }
    } finally {
      entry.flushes -= 1;
    }
    if (entry.lastError !== null) {
      throw entry.lastError;
    }
  }

  /** Prevents future writes and drops only payloads that have not started network admission.
   * 阻止后续写入，并仅丢弃尚未开始网络准入的载荷。
   */
  dispose(): void {
    this.disposed = true;
    for (const entry of this.entries.values()) {
      entry.pending = null;
      entry.wake?.();
    }
  }

  /** Starts every waiting write now, for a page that is going away.
   * 立即开始所有等待中的写入，用于即将离开的页面。
   */
  writeAllNow(): void {
    for (const entry of this.entries.values()) {
      entry.lastWriteAt = Number.NEGATIVE_INFINITY;
      entry.wake?.();
    }
  }

  /** Returns or creates the writer state for one exact session.
   * 返回或创建一个精确会话的 writer 状态。
   */
  private entry(sessionId: string): SessionDraftWriteEntryV1 {
    const existing = this.entries.get(sessionId);
    if (existing) {
      return existing;
    }
    const created: SessionDraftWriteEntryV1 = {
      pending: null,
      running: null,
      lastError: null,
      lastWriteAt: Number.NEGATIVE_INFINITY,
      wake: null,
      flushes: 0,
    };
    this.entries.set(sessionId, created);
    return created;
  }

  /** Serially commits the current payload and every newest replacement observed during the request.
   * 串行提交当前载荷，以及请求期间观察到的每一份最新替换载荷。
   */
  private async drain(entry: SessionDraftWriteEntryV1): Promise<void> {
    try {
      while (!this.disposed && entry.pending !== null) {
        // Waiting only when needed keeps the first write of a burst synchronous with its enqueue.
        // 仅在需要时等待，使突发中的首次写入与其入队同步发生。
        const remaining = entry.lastWriteAt + this.intervalMs - performance.now();
        if (remaining > 0 && entry.flushes === 0 && entry.pending.coalesce) {
          await new Promise<void>((resolve) => {
            const done = () => {
              clearTimeout(timer);
              entry.wake = null;
              resolve();
            };
            const timer = setTimeout(done, remaining);
            entry.wake = done;
          });
          if (this.disposed || entry.pending === null) {
            break;
          }
        }
        const pending = entry.pending;
        entry.pending = null;
        entry.lastWriteAt = performance.now();
        try {
          await this.client.write(pending.sessionId, pending.text, pending.attachments);
          entry.lastError = null;
        } catch (error) {
          entry.lastError = error;
        }
      }
    } finally {
      entry.running = null;
      if (!this.disposed && entry.pending !== null) {
        entry.running = this.drain(entry);
      }
    }
  }
}
