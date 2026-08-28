import type { SessionDraftRpcV1 } from "@/service/conversation-session-drafts-v1";
import type { ComposerAttachmentSummary } from "@/types";

/** Immutable payload queued for one authoritative session-draft replacement.
 * 排队等待权威会话草稿替换的一份不可变载荷。
 */
interface PendingSessionDraftWriteV1 {
  sessionId: string;
  text: string;
  attachments: ComposerAttachmentSummary[];
}

/** Mutable per-session drain state owned only by the coordinator.
 * 仅由协调器持有的单会话可变排空状态。
 */
interface SessionDraftWriteEntryV1 {
  pending: PendingSessionDraftWriteV1 | null;
  running: Promise<void> | null;
  lastError: unknown;
}

/** Coalesces typing bursts while preserving one serialized writer per materialized session.
 * 合并连续输入突发，同时为每个已物化会话保留一个串行 writer。
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
  constructor(private readonly client: SessionDraftRpcV1) {}

  /** Replaces the pending payload with the newest complete draft and starts draining immediately.
   * 用最新完整草稿替换待写载荷，并立即开始排空。
   */
  enqueue(
    sessionId: string,
    text: string,
    attachments: readonly ComposerAttachmentSummary[],
  ): void {
    if (this.disposed) {
      throw new Error("Session draft writer has been disposed.");
    }
    const entry = this.entry(sessionId);
    entry.pending = {
      sessionId,
      text,
      attachments: attachments.map((attachment) => ({ ...attachment })),
    };
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
    while (entry.running !== null) {
      await entry.running;
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
        const pending = entry.pending;
        entry.pending = null;
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
