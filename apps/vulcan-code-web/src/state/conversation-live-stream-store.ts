import type {
  ToolRequestProgressSnapshot,
  TransientReasoningStatus,
  WorkbenchDeltaEnvelope,
} from "@/types";

// ConversationLiveStreamListener receives one fine-grained ephemeral-state notification.
// ConversationLiveStreamListener 接收一条细粒度瞬态状态通知。
export type ConversationLiveStreamListener = () => void;

// Shared empty membership avoids unstable useSyncExternalStore snapshots.
// 共享空成员集合避免 useSyncExternalStore 快照不稳定。
const emptyToolRequests: readonly ToolRequestProgressSnapshot[] = [];

// Returns the membership subscription key for a conversation; token updates use the leaf key.
// 返回会话成员订阅键；Token 更新使用叶子键。
export function toolRequestMembershipKey(conversationId: string): string {
  return `tool-request-membership:${conversationId}`;
}

// transientReasoningStatusKey builds the subscription identity for one conversation status line.
// transientReasoningStatusKey 构建单个会话状态行的订阅身份。
// Parameters: conversationId is the exact owning conversation identity.
// 参数：conversationId 是精确的所属会话身份。
// Returns: a namespace-isolated store subscription key.
// 返回值：一个命名空间隔离的存储订阅键。
export function transientReasoningStatusKey(conversationId: string): string {
  return `transient-reasoning-status:${conversationId}`;
}

// contextCompactionPreviewKey builds the subscription identity for one compaction card preview.
// contextCompactionPreviewKey 构建单张压缩卡预览的订阅身份。
// Parameters: conversationId owns the compaction and apiCallId is its durable V1 API identity.
// 参数：conversationId 是压缩所属会话，apiCallId 是其持久 V1 API 身份。
// Returns: one namespace-isolated store subscription key.
// 返回值：一个命名空间隔离的存储订阅键。
export function contextCompactionPreviewKey(conversationId: string, apiCallId: number): string {
  return `context-compaction-preview:${conversationId}:${apiCallId}`;
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

  // contextCompactionPreviews stores the live summary text of running compaction APIs outside the transcript.
  // contextCompactionPreviews 在转录之外保存运行中压缩 API 的实时摘要文本。
  readonly #contextCompactionPreviews = new Map<string, string>();

  // toolRequestProgressSnapshots stores high-frequency argument formation outside retained events.
  // toolRequestProgressSnapshots 在持久化事件之外保存高频参数构造状态。
  readonly #toolRequestProgressSnapshots = new Map<string, ToolRequestProgressSnapshot>();

  // Initial snapshots create rows once; subsequent tokens notify only their card.
  // 初始快照仅创建一次行；后续 Token 仅通知对应卡片。
  readonly #toolRequestMembership = new Map<string, readonly ToolRequestProgressSnapshot[]>();

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
        return this.#deleteToolRequestProgress(key, conversationId);
      }
      if (
        progress.requestStreamId !== requestStreamId
        || !Number.isSafeInteger(progress.apiCallId)
        || progress.apiCallId < 1
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
          || progress.apiCallId !== existing.apiCallId
          || progress.toolName !== existing.toolName
          || progress.requestRevision <= existing.requestRevision
        )
      ) {
        return false;
      }
      this.#toolRequestProgressSnapshots.set(key, {
        ...progress,
        // The coordinator emits byte progress separately from semantic previews; absence means no update.
        // 协调器分别发送字节进度和语义预览；预览缺席表示没有更新。
        // Keep the last sanitized preview for this validated identity until replacement or draft removal.
        // 为已校验的当前身份保留最近净化预览，直到新预览替换或草稿移除。
        preview: progress.preview ? { ...progress.preview } : existing?.preview ?? progress.preview,
      });
      if (!existing) {
        this.#toolRequestMembership.set(conversationId, [
          ...this.getToolRequestMembership(conversationId), progress,
        ]);
        this.#dirtyKeys.add(toolRequestMembershipKey(conversationId));
      }
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
    if (envelope.delta.kind === "contextCompactionPreviewChanged") {
      const { conversationId, apiCallId, text } = envelope.delta.payload;
      const key = contextCompactionPreviewKey(conversationId, apiCallId);
      if (typeof text === "string") {
        if (this.#contextCompactionPreviews.get(key) === text) return false;
        this.#contextCompactionPreviews.set(key, text);
      } else if (!this.#contextCompactionPreviews.delete(key)) {
        return false;
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
    for (const key of this.#contextCompactionPreviews.keys()) {
      this.#dirtyKeys.add(key);
    }
    this.#contextCompactionPreviews.clear();
    for (const key of this.#toolRequestProgressSnapshots.keys()) {
      this.#dirtyKeys.add(key);
    }
    this.#toolRequestProgressSnapshots.clear();
    for (const conversationId of this.#toolRequestMembership.keys()) {
      this.#dirtyKeys.add(toolRequestMembershipKey(conversationId));
    }
    this.#toolRequestMembership.clear();
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

  // getContextCompactionPreview returns the live summary preview of one compaction API.
  // getContextCompactionPreview 返回单次压缩 API 的实时摘要预览。
  // Parameters: conversationId and apiCallId form the exact identity.
  // 参数：conversationId 与 apiCallId 组成精确身份。
  // Returns: the current preview text, or null when absent or cleared.
  // 返回值：当前预览文本；不存在或已清除时返回 null。
  getContextCompactionPreview(conversationId: string, apiCallId: number): string | null {
    return this.#contextCompactionPreviews.get(contextCompactionPreviewKey(conversationId, apiCallId)) ?? null;
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

  // Returns stable initial snapshots for the given conversation, updated only on insertion/removal.
  // 返回指定会话的稳定初始快照，仅在插入或移除时更新。
  getToolRequestMembership(conversationId: string): readonly ToolRequestProgressSnapshot[] {
    return this.#toolRequestMembership.get(conversationId) ?? emptyToolRequests;
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
    return this.#deleteToolRequestProgress(key, conversationId);
  }

  // Retires all drafts owned by the completed API; returns whether any draft existed.
  // 淘汰已结束 API 持有的所有草稿；返回是否存在草稿。
  settleToolRequestApi(conversationId: string, apiCallId: number): boolean {
    let removed = false;
    for (const draft of this.getToolRequestMembership(conversationId)) {
      if (draft.apiCallId === apiCallId) {
        removed = this.settleToolRequestProgress(conversationId, draft.requestStreamId) || removed;
      }
    }
    return removed;
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
  #deleteToolRequestProgress(key: string, conversationId: string): boolean {
    const removed = this.#toolRequestProgressSnapshots.delete(key);
    if (removed) {
      this.#dirtyKeys.add(key);
      this.#toolRequestMembership.set(conversationId,
        this.getToolRequestMembership(conversationId).filter(
          (draft) => toolRequestProgressKey(conversationId, draft.requestStreamId) !== key,
        ));
      this.#dirtyKeys.add(toolRequestMembershipKey(conversationId));
    }
    return removed;
  }
}
