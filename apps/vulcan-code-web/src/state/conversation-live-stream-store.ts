import type {
  ToolRequestProgressSnapshot,
  TransientReasoningStatus,
  WorkbenchDeltaEnvelope,
} from "@/types";

// ConversationLiveStreamListener receives one fine-grained ephemeral-state notification.
// ConversationLiveStreamListener 接收一条细粒度瞬态状态通知。
export type ConversationLiveStreamListener = () => void;

// transientReasoningStatusKey builds the subscription identity for one conversation status line.
// transientReasoningStatusKey 构建单个会话状态行的订阅身份。
// Parameters: conversationId is the exact owning conversation identity.
// 参数：conversationId 是精确的所属会话身份。
// Returns: a namespace-isolated store subscription key.
// 返回值：一个命名空间隔离的存储订阅键。
export function transientReasoningStatusKey(conversationId: string): string {
  return `transient-reasoning-status:${conversationId}`;
}

// toolRequestProgressKey builds the subscription identity for one request-stream card.
// toolRequestProgressKey 构建单个请求流卡片的订阅身份。
// Parameters: conversationId owns the request and requestStreamId is the exact coordinator identity.
// 参数：conversationId 是请求所属会话，requestStreamId 是精确协调器身份。
// Returns: one namespace-isolated store subscription key.
// 返回值：一个命名空间隔离的存储订阅键。
export function toolRequestProgressKey(
  conversationId: string,
  requestStreamId: string,
): string {
  return `tool-request-progress:${conversationId}:${requestStreamId}`;
}

// ConversationLiveStreamStore owns only request-local state that must never enter the durable V1 ledger.
// ConversationLiveStreamStore 仅持有绝不能进入持久化 V1 账本的请求局部状态。
export class ConversationLiveStreamStore {
  // transientReasoningStatuses stores replace-in-place Provider progress outside the transcript.
  // transientReasoningStatuses 在转录之外保存原位替换的 Provider 进度。
  readonly #transientReasoningStatuses = new Map<string, TransientReasoningStatus>();

  // toolRequestProgressSnapshots stores high-frequency argument formation outside retained events.
  // toolRequestProgressSnapshots 在持久化事件之外保存高频参数构造状态。
  readonly #toolRequestProgressSnapshots = new Map<string, ToolRequestProgressSnapshot>();

  // settledToolRequestProgressKeys prevents delayed progress from reviving a terminal draft card.
  // settledToolRequestProgressKeys 防止延迟进度复活已终止的草稿卡片。
  readonly #settledToolRequestProgressKeys = new Set<string>();

  // listeners stores fine-grained subscribers by exact ephemeral identity.
  // listeners 按精确瞬态身份保存细粒度订阅者。
  readonly #listeners = new Map<string, Set<ConversationLiveStreamListener>>();

  // dirtyKeys batches notifications without changing event order.
  // dirtyKeys 批量发送通知且不改变事件顺序。
  readonly #dirtyKeys = new Set<string>();

  // applyEnvelope ingests one generation-ordered ephemeral Workbench delta.
  // applyEnvelope 写入一条已按 generation 排序的瞬态 Workbench 增量。
  // Parameters: envelope is one transport event admitted by the global ordering gate.
  // 参数：envelope 是一条已通过全局顺序门禁的传输事件。
  // Returns: true only when one ephemeral value changed.
  // 返回值：仅在某个瞬态值发生变化时返回 true。
  applyEnvelope(envelope: WorkbenchDeltaEnvelope): boolean {
    if (envelope.delta.kind === "toolRequestProgressChanged") {
      const { conversationId, requestStreamId, progress } = envelope.delta.payload;
      const key = toolRequestProgressKey(conversationId, requestStreamId);
      const existing = this.#toolRequestProgressSnapshots.get(key);
      if (!progress) {
        this.#settledToolRequestProgressKeys.add(key);
        return this.#deleteToolRequestProgress(key);
      }
      if (
        progress.requestStreamId !== requestStreamId
        || !Number.isSafeInteger(progress.requestRevision)
        || progress.requestRevision < 1
        || this.#settledToolRequestProgressKeys.has(key)
      ) {
        return false;
      }
      if (
        existing
        && (
          progress.toolCallId !== existing.toolCallId
          || progress.toolName !== existing.toolName
          || progress.requestRevision <= existing.requestRevision
        )
      ) {
        return false;
      }
      this.#toolRequestProgressSnapshots.set(key, {
        ...progress,
        preview: progress.preview ? { ...progress.preview } : progress.preview,
      });
      this.#dirtyKeys.add(key);
      return true;
    }
    if (envelope.delta.kind === "transientReasoningStatusChanged") {
      const { conversationId, status } = envelope.delta.payload;
      const key = transientReasoningStatusKey(conversationId);
      if (status) {
        this.#transientReasoningStatuses.set(conversationId, {
          ...status,
          summaries: status.summaries?.map((summary) => ({ ...summary })),
        });
      } else {
        this.#transientReasoningStatuses.delete(conversationId);
      }
      this.#dirtyKeys.add(key);
      return true;
    }
    return false;
  }

  // resetForWorkbenchSnapshot clears request-local values after an authoritative snapshot replacement.
  // resetForWorkbenchSnapshot 在权威快照替换后清除请求局部值。
  // Returns: nothing; subscribers are notified by the next flushNotifications call.
  // 返回值：无；订阅者由下一次 flushNotifications 调用通知。
  resetForWorkbenchSnapshot(): void {
    for (const conversationId of this.#transientReasoningStatuses.keys()) {
      this.#dirtyKeys.add(transientReasoningStatusKey(conversationId));
    }
    this.#transientReasoningStatuses.clear();
    for (const key of this.#toolRequestProgressSnapshots.keys()) {
      this.#settledToolRequestProgressKeys.add(key);
      this.#dirtyKeys.add(key);
    }
    this.#toolRequestProgressSnapshots.clear();
  }

  // getTransientReasoningStatus returns the stable request-local reasoning status.
  // getTransientReasoningStatus 返回稳定的请求局部思考状态。
  // Parameters: conversationId is the exact owner.
  // 参数：conversationId 是精确所有者。
  // Returns: the current status or null when absent.
  // 返回值：当前状态；不存在时返回 null。
  getTransientReasoningStatus(conversationId: string): TransientReasoningStatus | null {
    return this.#transientReasoningStatuses.get(conversationId) ?? null;
  }

  // getToolRequestProgress returns the stable high-frequency snapshot for one request stream.
  // getToolRequestProgress 返回单个请求流的稳定高频快照。
  // Parameters: conversationId and requestStreamId form the exact identity.
  // 参数：conversationId 与 requestStreamId 组成精确身份。
  // Returns: the current progress or null after terminal cleanup.
  // 返回值：当前进度；终态清理后返回 null。
  getToolRequestProgress(
    conversationId: string,
    requestStreamId: string,
  ): ToolRequestProgressSnapshot | null {
    return this.#toolRequestProgressSnapshots.get(
      toolRequestProgressKey(conversationId, requestStreamId),
    ) ?? null;
  }

  // settleToolRequestProgress permanently retires one request-local draft after a durable V1 tool event takes ownership.
  // settleToolRequestProgress 在持久化 V1 工具事件接管后永久淘汰一条请求局部草稿。
  // Parameters: conversationId and requestStreamId form the exact durable handoff identity.
  // 参数：conversationId 与 requestStreamId 组成精确的持久化交接身份。
  // Returns: true only when an in-memory progress snapshot was removed.
  // 返回值：仅当内存进度快照被删除时返回 true。
  settleToolRequestProgress(conversationId: string, requestStreamId: string): boolean {
    const key = toolRequestProgressKey(conversationId, requestStreamId);
    this.#settledToolRequestProgressKeys.add(key);
    return this.#deleteToolRequestProgress(key);
  }

  // subscribe registers one fine-grained listener for an exact ephemeral identity.
  // subscribe 为一个精确瞬态身份注册细粒度监听器。
  // Parameters: key is the subscription identity and listener receives committed notifications.
  // 参数：key 是订阅身份，listener 接收已提交通知。
  // Returns: an idempotent unsubscribe callback.
  // 返回值：一个幂等取消订阅回调。
  subscribe(key: string, listener: ConversationLiveStreamListener): () => void {
    let listeners = this.#listeners.get(key);
    if (!listeners) {
      listeners = new Set();
      this.#listeners.set(key, listeners);
    }
    listeners.add(listener);
    return () => {
      const current = this.#listeners.get(key);
      current?.delete(listener);
      if (current?.size === 0) {
        this.#listeners.delete(key);
      }
    };
  }

  // flushNotifications publishes every dirty ephemeral identity exactly once for the current batch.
  // flushNotifications 为当前批次精确发布每个脏瞬态身份一次。
  // Returns: the number of identities notified.
  // 返回值：已通知的身份数量。
  flushNotifications(): number {
    const dirtyKeys = [...this.#dirtyKeys];
    this.#dirtyKeys.clear();
    for (const key of dirtyKeys) {
      for (const listener of this.#listeners.get(key) ?? []) {
        listener();
      }
    }
    return dirtyKeys.length;
  }

  // #deleteToolRequestProgress removes one exact progress value and schedules a notification.
  // #deleteToolRequestProgress 删除一个精确进度值并安排通知。
  // Parameters: key is the exact request progress identity.
  // 参数：key 是精确请求进度身份。
  // Returns: true only when a value existed.
  // 返回值：仅在原值存在时返回 true。
  #deleteToolRequestProgress(key: string): boolean {
    const removed = this.#toolRequestProgressSnapshots.delete(key);
    if (removed) {
      this.#dirtyKeys.add(key);
    }
    return removed;
  }
}
