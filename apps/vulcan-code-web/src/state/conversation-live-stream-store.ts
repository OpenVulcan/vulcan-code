import type {
  AppendMessageBodyDelta,
  ConversationMessage,
  ConversationMessageStreamState,
  ToolRequestProgressSnapshot,
  TransientReasoningStatus,
  WorkbenchDeltaEnvelope,
  WorkbenchSnapshot,
} from "@/types";

// ConversationLiveStreamKind is the exact transcript record kind accepted by the live stream cache.
// ConversationLiveStreamKind 是实时流缓存接受的精确转录记录类型。
export type ConversationLiveStreamKind = "thinking-output" | "assistant-output";

// ConversationLiveStreamRevision is the immutable subscription snapshot for one cached stream.
// ConversationLiveStreamRevision 是单条缓存流使用的不可变订阅快照。
export interface ConversationLiveStreamRevision {
  // Stable compound identity for this cached stream.
  // 当前缓存流的稳定复合身份。
  key: string;
  // Owning conversation identity.
  // 所属会话身份。
  conversationId: string;
  // Stable message identity targeted by body deltas.
  // 正文增量定位的稳定消息身份。
  messageId: string;
  // Exact streamed transcript record kind.
  // 精确的流式转录记录类型。
  recordKind: ConversationLiveStreamKind;
  // Explicit backend-owned stream lifecycle.
  // 后端持有的显式流生命周期。
  lifecycle: ConversationMessageStreamState;
  // Generation represented by the authoritative full base body.
  // 权威完整基线正文对应的 generation。
  baseGeneration: number;
  // Latest generation applied to this stream.
  // 当前流已应用的最新 generation。
  lastGeneration: number;
  // Monotonic local version used by fine-grained React subscriptions.
  // 细粒度 React 订阅使用的本地单调版本。
  version: number;
  // Total JavaScript UTF-16 code-unit length without materializing the full body.
  // 无需物化完整正文即可获得的 JavaScript UTF-16 码元总长度。
  utf16Length: number;
  // Total UTF-8 byte length used by Markdown segmentation thresholds.
  // Markdown 分段阈值使用的 UTF-8 总字节长度。
  utf8ByteLength: number;
}

// ConversationLiveStreamBodySnapshot is one atomic full-body read used only at cold mount or reconciliation.
// ConversationLiveStreamBodySnapshot 是仅在冷挂载或对账时使用的一次原子完整正文读取。
export interface ConversationLiveStreamBodySnapshot extends ConversationLiveStreamRevision {
  // Materialized authoritative body at lastGeneration.
  // 在 lastGeneration 时物化的权威正文。
  body: string;
}

// ConversationLiveStreamDelta is one ordered fragment read after a mounted render cursor.
// ConversationLiveStreamDelta 是挂载渲染游标之后读取的一条有序片段。
export interface ConversationLiveStreamDelta {
  // Backend generation that owns this fragment.
  // 当前片段所属的后端 generation。
  generation: number;
  // Exact appended provider text.
  // 精确追加的 Provider 文本。
  text: string;
}

// ConversationLiveStreamListener receives one fine-grained stream revision notification.
// ConversationLiveStreamListener 接收单条细粒度流版本通知。
export type ConversationLiveStreamListener = () => void;

// transientReasoningStatusKey builds the subscription identity for one conversation status line.
// transientReasoningStatusKey 构建单个会话状态行的订阅身份。
// Parameters: conversationId is the exact owning conversation identity.
// 参数：conversationId 是精确所属会话身份。
// Returns: a namespace-isolated Store subscription key.
// 返回值：一个命名空间隔离的 Store 订阅键。
export function transientReasoningStatusKey(conversationId: string): string {
  return `transient-reasoning-status:${conversationId}`;
}

// toolRequestProgressKey builds the subscription identity for one request-stream card.
// toolRequestProgressKey 构建单个请求流卡片的订阅身份。
// Parameters: conversationId is the owner and requestStreamId is the exact coordinator identity.
// 参数：conversationId 是所属会话，requestStreamId 是精确协调器身份。
// Returns: one namespace-isolated Store subscription key.
// 返回值：一个命名空间隔离的 Store 订阅键。
export function toolRequestProgressKey(
  conversationId: string,
  requestStreamId: string,
): string {
  return `tool-request-progress:${conversationId}:${requestStreamId}`;
}

// LiveStreamFragmentNode stores append history as a reverse linked list for constant-time ingestion.
// LiveStreamFragmentNode 使用反向链表保存追加历史，以实现常量时间写入。
interface LiveStreamFragmentNode {
  // Backend generation that owns this fragment.
  // 当前片段所属的后端 generation。
  generation: number;
  // Exact appended text.
  // 精确追加文本。
  text: string;
  // UTF-8 byte length computed once during ingestion.
  // 写入时仅计算一次的 UTF-8 字节长度。
  utf8ByteLength: number;
  // Previous fragment in reverse chronological order.
  // 反向时间顺序中的前一片段。
  previous: LiveStreamFragmentNode | null;
}

// ConversationLiveStreamRecord owns canonical Web replica state for one streamed message.
// ConversationLiveStreamRecord 持有单条流式消息的规范 Web 副本状态。
interface ConversationLiveStreamRecord {
  // Immutable public subscription snapshot.
  // 不可变的公开订阅快照。
  revision: ConversationLiveStreamRevision;
  // Latest authoritative full body supplied by a snapshot, initial row, or sealed event.
  // 快照、初始行或封口事件提供的最近权威完整正文。
  baseBody: string;
  // Reverse-linked fragments newer than baseGeneration.
  // 比 baseGeneration 更新的反向链式片段。
  fragmentTail: LiveStreamFragmentNode | null;
  // Stable workbench generation known to contain this exact sealed body.
  // 已知包含当前精确封口正文的稳定工作台 generation。
  stableCoverageGeneration: number | null;
}

// EMPTY_LISTENERS is the stable empty listener set used for missing stream identities.
// EMPTY_LISTENERS 是缺失流身份使用的稳定空监听器集合。
const EMPTY_LISTENERS: ReadonlySet<ConversationLiveStreamListener> = new Set();

// UTF8_ENCODER computes byte thresholds incrementally instead of rescanning full bodies.
// UTF8_ENCODER 增量计算字节阈值，避免重新扫描完整正文。
const UTF8_ENCODER = new TextEncoder();

// settledToolRequestStreamId returns the exact request identity after durable execution owns the card.
// settledToolRequestStreamId 在持久执行状态接管卡片后返回精确请求身份。
function settledToolRequestStreamId(message: ConversationMessage): string | null {
  if (!message.metadata || typeof message.metadata !== "object" || Array.isArray(message.metadata)) {
    return null;
  }
  // metadata is the exact root projected from the durable tool card.
  // metadata 是从持久工具卡片投影得到的精确根对象。
  const metadata = message.metadata as Record<string, unknown>;
  // requestStreamId preserves the coordinator identity after ToolStarted metadata merges in place.
  // requestStreamId 在 ToolStarted 元数据原地合并后继续保留协调器身份。
  const requestStreamId = typeof metadata.request_stream_id === "string"
    ? metadata.request_stream_id.trim()
    : "";
  // displayState decides whether request formation or actual execution owns the card.
  // displayState 决定请求构造或真实执行中的哪一方持有卡片。
  const displayState = typeof metadata.tool_display_state === "string"
    ? metadata.tool_display_state
    : "";
  if (!requestStreamId || ["receiving", "arguments_assembled", "ready"].includes(displayState)) {
    return null;
  }
  return requestStreamId;
}

// ConversationLiveStreamStore keeps every active conversation stream outside global React state.
// ConversationLiveStreamStore 在全局 React 状态之外保留所有活跃会话流。
export class ConversationLiveStreamStore {
  // records stores canonical Web replicas by conversation and message identity.
  // records 按会话与消息身份保存规范 Web 副本。
  readonly #records = new Map<string, ConversationLiveStreamRecord>();
  // transientReasoningStatuses stores replace-in-place request status outside transcript replicas.
  // transientReasoningStatuses 在转录副本之外保存原地替换式请求状态。
  readonly #transientReasoningStatuses = new Map<string, TransientReasoningStatus>();
  // toolRequestProgressSnapshots stores high-frequency argument formation outside retained messages.
  // toolRequestProgressSnapshots 在保留消息之外保存高频参数构造快照。
  readonly #toolRequestProgressSnapshots = new Map<string, ToolRequestProgressSnapshot>();
  // listeners stores fine-grained subscribers for one exact stream.
  // listeners 保存单条精确流的细粒度订阅者。
  readonly #listeners = new Map<string, Set<ConversationLiveStreamListener>>();
  // dirtyKeys batches notifications without coalescing away original generations.
  // dirtyKeys 批处理通知，但不会合并掉原始 generation。
  readonly #dirtyKeys = new Set<string>();

  // applyEnvelope ingests one already generation-ordered workbench event.
  // applyEnvelope 写入一条已经按 generation 排序的工作台事件。
  // Parameters: envelope is one transport event already admitted by the global ordering gate.
  // 参数：envelope 是一条已经通过全局顺序门控的传输事件。
  // Returns: true when the event changed one live stream replica.
  // 返回值：事件改变某条实时流副本时返回 true。
  applyEnvelope(envelope: WorkbenchDeltaEnvelope): boolean {
    if (envelope.delta.kind === "toolRequestProgressChanged") {
      // payload replaces or clears one exact request identity without touching transcript replicas.
      // payload 替换或清除单个精确请求身份，且不会修改转录副本。
      const { conversationId, requestStreamId, progress } = envelope.delta.payload;
      const key = toolRequestProgressKey(conversationId, requestStreamId);
      const existing = this.#toolRequestProgressSnapshots.get(key);
      if (!progress) {
        if (!existing) {
          return false;
        }
        this.#toolRequestProgressSnapshots.delete(key);
        this.#dirtyKeys.add(key);
        return true;
      }
      if (progress.requestStreamId !== requestStreamId) {
        return false;
      }
      if (existing && progress.requestRevision <= existing.requestRevision) {
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
      // conversationId and status are the complete replacement contract for one request.
      // conversationId 与 status 构成单个请求的完整替换契约。
      const { conversationId, status } = envelope.delta.payload;
      // key routes notifications without admitting the status into transcript records.
      // key 路由通知，且不会让状态进入转录记录。
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
    if (envelope.delta.kind !== "appendMessageBody") {
      return false;
    }
    return this.#applyAppendMessageBody(envelope.generation, envelope.delta.payload);
  }

  // rebaseWorkbenchSnapshot applies one authoritative full snapshot without discarding newer fragments.
  // rebaseWorkbenchSnapshot 应用一份权威完整快照，同时保留更新的片段。
  // Parameters: snapshot contains one exact workbench revision and its selected messages.
  // 参数：snapshot 包含一个精确工作台 revision 及其当前消息。
  // Returns: nothing; matching cached streams are updated in place.
  // 返回值：无；匹配的缓存流会原地更新。
  rebaseWorkbenchSnapshot(snapshot: Pick<WorkbenchSnapshot, "revision" | "messages">): void {
    // Authoritative snapshots intentionally exclude ephemeral state, so every older transient value is stale.
    // 权威快照有意排除瞬态状态，因此此前的所有瞬态值均已过期。
    for (const conversationId of this.#transientReasoningStatuses.keys()) {
      this.#dirtyKeys.add(transientReasoningStatusKey(conversationId));
    }
    this.#transientReasoningStatuses.clear();
    for (const key of this.#toolRequestProgressSnapshots.keys()) {
      this.#dirtyKeys.add(key);
    }
    this.#toolRequestProgressSnapshots.clear();
    // snapshotGeneration is the exact cutover revision represented by snapshot.messages.
    // snapshotGeneration 是 snapshot.messages 所表示的精确切入版本。
    const snapshotGeneration = snapshot.revision ?? 0;
    for (const message of snapshot.messages) {
      // lifecycle comes only from explicit stream metadata produced by the backend.
      // lifecycle 仅来自后端生成的显式流元数据。
      const lifecycle = conversationMessageStreamState(message);
      // recordKind rejects ordinary assistant transcript rows from the live cache.
      // recordKind 防止普通助手转录行进入实时缓存。
      const recordKind = conversationLiveStreamKind(message.recordKind);
      if (!lifecycle || !recordKind) {
        continue;
      }
      this.#rebaseMessage(message, recordKind, lifecycle, snapshotGeneration);
    }
  }

  // reconcileStableCoverage releases sealed replicas after the committed React workbench contains their exact body.
  // reconcileStableCoverage 在已提交 React 工作台包含精确正文后释放封口副本。
  // Parameters: snapshot is the workbench state already committed by React.
  // 参数：snapshot 是 React 已提交的工作台状态。
  // Returns: the number of settled request snapshots and unobserved terminal replicas collected during this pass.
  // 返回值：当前轮次回收的已结束请求快照与无订阅终态副本数量。
  reconcileStableCoverage(snapshot: Pick<WorkbenchSnapshot, "revision" | "messages">): number {
    // snapshotGeneration proves the stable projection is not older than the terminal stream event.
    // snapshotGeneration 证明稳定投影不早于终态流事件。
    const snapshotGeneration = snapshot.revision ?? 0;
    // messagesByKey avoids repeatedly scanning the stable message list for each cached stream.
    // messagesByKey 避免为每条缓存流重复扫描稳定消息列表。
    const messagesByKey = new Map(
      snapshot.messages.map((message) => [
        conversationLiveStreamKey(message.conversationId, message.id),
        message,
      ]),
    );
    let collectedCount = 0;
    // settledRequestKeys are cleared only after React has committed the ToolStarted or terminal card state.
    // settledRequestKeys 仅在 React 已提交 ToolStarted 或终态卡片状态后清理。
    for (const message of snapshot.messages) {
      const requestStreamId = settledToolRequestStreamId(message);
      if (!requestStreamId) {
        continue;
      }
      const key = toolRequestProgressKey(message.conversationId, requestStreamId);
      if (this.#toolRequestProgressSnapshots.delete(key)) {
        this.#dirtyKeys.add(key);
        collectedCount += 1;
      }
    }
    for (const [key, record] of this.#records) {
      if (record.revision.lifecycle !== "sealed" || snapshotGeneration < record.revision.lastGeneration) {
        continue;
      }
      const stableMessage = messagesByKey.get(key);
      if (!stableMessage || stableMessage.body !== materializeRecordBody(record)) {
        continue;
      }
      record.stableCoverageGeneration = snapshotGeneration;
      if (this.#collectSealedRecord(key)) {
        collectedCount += 1;
      }
    }
    return collectedCount;
  }

  // flushNotifications publishes every changed key once after the transport batching window.
  // flushNotifications 在传输批处理窗口结束后为每个变化身份发布一次通知。
  // Returns: nothing; registered listeners are invoked synchronously once per dirty key.
  // 返回值：无；每个脏键的已注册监听器会被同步调用一次。
  flushNotifications(): void {
    // dirtyKeys is copied before callbacks so reentrant writes belong to the next flush.
    // dirtyKeys 在回调前复制，使重入写入归入下一次刷新。
    const dirtyKeys = Array.from(this.#dirtyKeys);
    this.#dirtyKeys.clear();
    for (const key of dirtyKeys) {
      // listeners is the stable subscriber set for this exact stream.
      // listeners 是当前精确流的稳定订阅者集合。
      const listeners = this.#listeners.get(key) ?? EMPTY_LISTENERS;
      for (const listener of listeners) {
        listener();
      }
    }
  }

  // subscribe registers one listener for an exact conversation stream identity.
  // subscribe 为一个精确会话流身份注册监听器。
  // Parameters: key identifies one cached stream and listener receives revision invalidations.
  // 参数：key 标识一条缓存流，listener 接收 revision 失效通知。
  // Returns: an idempotent unsubscription callback for this registration.
  // 返回值：当前注册对应的幂等取消订阅回调。
  subscribe(key: string, listener: ConversationLiveStreamListener): () => void {
    // listeners is created only when the first subscriber mounts.
    // listeners 仅在首个订阅者挂载时创建。
    const listeners = this.#listeners.get(key) ?? new Set<ConversationLiveStreamListener>();
    listeners.add(listener);
    this.#listeners.set(key, listeners);
    return () => {
      listeners.delete(listener);
      if (listeners.size === 0) {
        this.#listeners.delete(key);
        this.#collectSealedRecord(key);
      }
    };
  }

  // getRevision returns the stable immutable subscription snapshot for one stream.
  // getRevision 返回单条流稳定且不可变的订阅快照。
  // Parameters: key identifies the exact cached stream.
  // 参数：key 标识精确缓存流。
  // Returns: its immutable revision, or null when no replica exists.
  // 返回值：对应不可变 revision；副本不存在时返回 null。
  getRevision(key: string): ConversationLiveStreamRevision | null {
    return this.#records.get(key)?.revision ?? null;
  }

  // getTransientReasoningStatus returns the current request-local status for one conversation.
  // getTransientReasoningStatus 返回单个会话当前请求局部状态。
  // Parameters: conversationId is the exact owning conversation identity.
  // 参数：conversationId 是精确所属会话身份。
  // Returns: the immutable latest status, or null after completion or clearing.
  // 返回值：最新不可变状态；请求完成或清除后返回 null。
  getTransientReasoningStatus(conversationId: string): TransientReasoningStatus | null {
    return this.#transientReasoningStatuses.get(conversationId) ?? null;
  }

  // getToolRequestProgress returns the latest leaf-only snapshot for one exact request stream.
  // getToolRequestProgress 返回单个精确请求流最新的仅叶子级快照。
  // Parameters: conversationId owns the request and requestStreamId is its coordinator identity.
  // 参数：conversationId 持有请求，requestStreamId 是其协调器身份。
  // Returns: the stable immutable snapshot, or null after terminal clearing.
  // 返回值：稳定不可变快照；终态清除后返回空。
  getToolRequestProgress(
    conversationId: string,
    requestStreamId: string,
  ): ToolRequestProgressSnapshot | null {
    return this.#toolRequestProgressSnapshots.get(
      toolRequestProgressKey(conversationId, requestStreamId),
    ) ?? null;
  }

  // readBody materializes one full stream body for cold mount or authoritative reconciliation.
  // readBody 为冷挂载或权威对账物化一份完整流正文。
  // Parameters: key identifies the exact cached stream.
  // 参数：key 标识精确缓存流。
  // Returns: one atomic body snapshot, or null when no replica exists.
  // 返回值：一份原子正文快照；副本不存在时返回 null。
  readBody(key: string): ConversationLiveStreamBodySnapshot | null {
    // record is read once so body and revision describe the same synchronous Store state.
    // record 只读取一次，使正文与版本描述同一个同步 Store 状态。
    const record = this.#records.get(key);
    if (!record) {
      return null;
    }
    // fragments converts the reverse linked list into chronological append order once.
    // fragments 将反向链表一次性转换为按时间排序的追加顺序。
    const fragments = chronologicalFragmentsAfter(record.fragmentTail, record.revision.baseGeneration);
    return {
      ...record.revision,
      body: record.baseBody + fragments.map((fragment) => fragment.text).join(""),
    };
  }

  // readDeltasAfter returns ordered fragments newer than one mounted render cursor.
  // readDeltasAfter 返回比挂载渲染游标更新的有序片段。
  // Parameters: key identifies the stream and generation is the caller's consumed cursor.
  // 参数：key 标识流，generation 是调用方已消费的游标。
  // Returns: chronological fragments strictly newer than the cursor.
  // 返回值：严格晚于游标的按时间排序片段。
  readDeltasAfter(key: string, generation: number): ConversationLiveStreamDelta[] {
    // record owns every fragment not already covered by its full base body.
    // record 持有尚未被完整基线正文覆盖的所有片段。
    const record = this.#records.get(key);
    if (!record) {
      return [];
    }
    return chronologicalFragmentsAfter(record.fragmentTail, generation).map((fragment) => ({
      generation: fragment.generation,
      text: fragment.text,
    }));
  }

  // #applyAppendMessageBody applies one raw append event while preserving its original generation.
  // #applyAppendMessageBody 应用一条原始追加事件，同时保留其原始 generation。
  // Parameters: generation orders the event and delta carries its exact message mutation.
  // 参数：generation 对事件排序，delta 携带精确消息变更。
  // Returns: true when the event changed the canonical replica.
  // 返回值：事件改变规范副本时返回 true。
  #applyAppendMessageBody(generation: number, delta: AppendMessageBodyDelta): boolean {
    // key is stable even when the target conversation is not currently mounted.
    // key 即使目标会话当前未挂载也保持稳定。
    const key = conversationLiveStreamKey(delta.conversationId, delta.messageId);
    // existing is the current canonical Web replica when the stream has already materialized.
    // existing 是流已经物化时的当前规范 Web 副本。
    const existing = this.#records.get(key);
    // recordKind comes from the exact initial row or the already-established stream identity.
    // recordKind 来自精确初始行或已经建立的流身份。
    const recordKind = conversationLiveStreamKind(delta.initialMessage?.recordKind)
      ?? existing?.revision.recordKind
      ?? null;
    // lifecycle must be explicit for new protocol events; an existing stream may accept old-compatible continuations.
    // lifecycle 对新协议事件必须显式；既有流可接受旧兼容续传事件。
    const lifecycle = delta.streamState
      ?? existing?.revision.lifecycle
      ?? conversationMessageStreamState(delta.initialMessage)
      ?? null;
    if (!recordKind || !lifecycle) {
      return false;
    }
    if (existing && generation <= existing.revision.lastGeneration) {
      return false;
    }
    if (existing?.revision.lifecycle === "sealed") {
      // A sealed stream accepts only an explicit terminal correction and never an inferred legacy continuation.
      // 已封口流仅接受显式终态校正，绝不接受通过继承状态得到的旧式续传。
      if (delta.streamState !== "sealed" || delta.bodyDelta.length > 0) {
        return false;
      }
    }
    if (lifecycle === "sealed" && typeof delta.currentBody === "string") {
      if (existing && materializeRecordBody(existing) === delta.currentBody) {
        this.#updateLifecycle(existing, lifecycle, generation, null);
        this.#dirtyKeys.add(key);
        return true;
      }
      this.#replaceRecord(
        key,
        delta.conversationId,
        delta.messageId,
        recordKind,
        lifecycle,
        generation,
        delta.currentBody,
        null,
      );
      return true;
    }
    if (!existing) {
      // initialBody is authoritative only when the backend materializes the stable stream row.
      // initialBody 仅在后端物化稳定流式行时具有权威性。
      const initialBody = delta.initialMessage?.body ?? delta.currentBody;
      if (typeof initialBody !== "string") {
        return false;
      }
      this.#replaceRecord(
        key,
        delta.conversationId,
        delta.messageId,
        recordKind,
        lifecycle,
        generation,
        initialBody,
        null,
      );
      return true;
    }
    if (typeof delta.currentBody === "string") {
      this.#replaceRecord(
        key,
        delta.conversationId,
        delta.messageId,
        recordKind,
        lifecycle,
        generation,
        delta.currentBody,
        null,
      );
      return true;
    }
    if (delta.bodyDelta.length === 0) {
      this.#updateLifecycle(existing, lifecycle, generation, null);
      this.#dirtyKeys.add(key);
      return true;
    }
    // fragment stores one original generation without copying the historical body.
    // fragment 保存一条原始 generation，且不会复制历史正文。
    const fragment: LiveStreamFragmentNode = {
      generation,
      text: delta.bodyDelta,
      utf8ByteLength: utf8ByteLength(delta.bodyDelta),
      previous: existing.fragmentTail,
    };
    existing.fragmentTail = fragment;
    existing.revision = {
      ...existing.revision,
      lifecycle,
      lastGeneration: generation,
      version: existing.revision.version + 1,
      utf16Length: existing.revision.utf16Length + delta.bodyDelta.length,
      utf8ByteLength: existing.revision.utf8ByteLength + fragment.utf8ByteLength,
    };
    this.#dirtyKeys.add(key);
    return true;
  }

  // #rebaseMessage establishes one snapshot body while retaining already-received newer fragments.
  // #rebaseMessage 建立一份快照正文，同时保留已经收到的更新片段。
  // Parameters: message is authoritative at generation; recordKind and lifecycle are explicit protocol fields.
  // 参数：message 在 generation 对应版本上权威；recordKind 与 lifecycle 是显式协议字段。
  // Returns: nothing; the keyed replica is rebased in place.
  // 返回值：无；对应键副本会原地重基线。
  #rebaseMessage(
    message: ConversationMessage,
    recordKind: ConversationLiveStreamKind,
    lifecycle: ConversationMessageStreamState,
    generation: number,
  ): void {
    // key locates the exact cached stream represented by this snapshot row.
    // key 定位当前快照行表示的精确缓存流。
    const key = conversationLiveStreamKey(message.conversationId, message.id);
    // existing may already contain transport events newer than this RPC snapshot.
    // existing 可能已经包含比当前 RPC 快照更新的传输事件。
    const existing = this.#records.get(key);
    if (existing && existing.revision.baseGeneration > generation) {
      return;
    }
    if (existing?.revision.lifecycle === "sealed" && lifecycle === "active") {
      // A terminal replica remains authoritative even when a structurally newer snapshot carries stale active metadata.
      // 即使结构上更新的快照携带过期 active 元数据，终态副本仍保持权威。
      return;
    }
    if (existing && lifecycle === "active" && generation >= existing.revision.lastGeneration) {
      // currentBody is materialized only on structural snapshot boundaries, never on ordinary deltas.
      // currentBody 仅在结构快照边界物化，普通增量不会触发。
      const currentBody = materializeRecordBody(existing);
      // Active streams are append-only; a newer structural snapshot carrying a strict prefix has stale hot text.
      // 活跃流只允许追加；较新结构快照若仅携带严格前缀，说明其热正文已经过期。
      if (currentBody.length > message.body.length && currentBody.startsWith(message.body)) {
        return;
      }
    }
    // retainedFragments keeps only events that the snapshot revision cannot already contain.
    // retainedFragments 仅保留快照版本尚不可能包含的事件。
    const retainedFragments = existing
      ? chronologicalFragmentsAfter(existing.fragmentTail, generation)
      : [];
    // retainedTail rebuilds the reverse list without merging away fragment generations.
    // retainedTail 重建反向链表，同时不合并片段 generation。
    const retainedTail = reverseFragmentList(retainedFragments);
    // retainedUtf16Length is computed from only the small post-snapshot fragment list.
    // retainedUtf16Length 仅根据较小的快照后片段列表计算。
    const retainedUtf16Length = retainedFragments.reduce((total, fragment) => total + fragment.text.length, 0);
    // retainedUtf8ByteLength reuses ingestion-time byte counts.
    // retainedUtf8ByteLength 复用写入时计算的字节数。
    const retainedUtf8ByteLength = retainedFragments.reduce(
      (total, fragment) => total + fragment.utf8ByteLength,
      0,
    );
    // lastGeneration remains newer when transport events raced ahead of the selection snapshot.
    // lastGeneration 在传输事件领先会话选择快照时保持更新值。
    const lastGeneration = retainedFragments.at(-1)?.generation ?? generation;
    // effectiveLifecycle preserves a newer terminal state over an older active snapshot.
    // effectiveLifecycle 防止较旧活跃快照覆盖更新的终态。
    const effectiveLifecycle = existing && existing.revision.lastGeneration > generation
      ? existing.revision.lifecycle
      : lifecycle;
    const stableCoverageGeneration = lifecycle === "sealed" && retainedFragments.length === 0
      ? generation
      : existing?.stableCoverageGeneration ?? null;
    this.#records.set(key, {
      baseBody: message.body,
      fragmentTail: retainedTail,
      stableCoverageGeneration,
      revision: {
        key,
        conversationId: message.conversationId,
        messageId: message.id,
        recordKind,
        lifecycle: effectiveLifecycle,
        baseGeneration: generation,
        lastGeneration,
        version: (existing?.revision.version ?? 0) + 1,
        utf16Length: message.body.length + retainedUtf16Length,
        utf8ByteLength: utf8ByteLength(message.body) + retainedUtf8ByteLength,
      },
    });
    this.#dirtyKeys.add(key);
  }

  // #replaceRecord replaces one stream with a complete authoritative body at an exact generation.
  // #replaceRecord 使用精确 generation 的完整权威正文替换一条流。
  // Parameters: identity fields select the replica; lifecycle, generation, body, and coverage define its state.
  // 参数：身份字段选择副本；lifecycle、generation、body 与 coverage 定义其状态。
  // Returns: nothing; the record map receives one complete replacement.
  // 返回值：无；记录映射会收到一份完整替换。
  #replaceRecord(
    key: string,
    conversationId: string,
    messageId: string,
    recordKind: ConversationLiveStreamKind,
    lifecycle: ConversationMessageStreamState,
    generation: number,
    body: string,
    stableCoverageGeneration: number | null,
  ): void {
    // previousVersion preserves subscriber monotonicity across authoritative rebases.
    // previousVersion 在权威重基线期间保持订阅版本单调递增。
    const previousVersion = this.#records.get(key)?.revision.version ?? 0;
    this.#records.set(key, {
      baseBody: body,
      fragmentTail: null,
      stableCoverageGeneration,
      revision: {
        key,
        conversationId,
        messageId,
        recordKind,
        lifecycle,
        baseGeneration: generation,
        lastGeneration: generation,
        version: previousVersion + 1,
        utf16Length: body.length,
        utf8ByteLength: utf8ByteLength(body),
      },
    });
    this.#dirtyKeys.add(key);
  }

  // #updateLifecycle changes terminal state without rebuilding the cached body.
  // #updateLifecycle 在不重建缓存正文的情况下修改终态。
  // Parameters: record is mutable canonical state and remaining values define the lifecycle revision.
  // 参数：record 是可变规范状态，其余值定义生命周期 revision。
  // Returns: nothing; revision and optional coverage are updated in place.
  // 返回值：无；revision 与可选 coverage 会原地更新。
  #updateLifecycle(
    record: ConversationLiveStreamRecord,
    lifecycle: ConversationMessageStreamState,
    generation: number,
    stableCoverageGeneration: number | null,
  ): void {
    record.revision = {
      ...record.revision,
      lifecycle,
      lastGeneration: generation,
      version: record.revision.version + 1,
    };
    if (stableCoverageGeneration !== null) {
      record.stableCoverageGeneration = stableCoverageGeneration;
    }
  }

  // #collectSealedRecord deterministically releases a terminal replica only after stable coverage and unmount.
  // #collectSealedRecord 仅在稳定状态已覆盖且订阅卸载后确定性释放终态副本。
  // Parameters: key identifies the terminal candidate.
  // 参数：key 标识终态候选副本。
  // Returns: true only when the candidate was removed during this call.
  // 返回值：仅当候选副本在当前调用中被移除时返回 true。
  #collectSealedRecord(key: string): boolean {
    // record may have been replaced or already collected by snapshot reconciliation.
    // record 可能已被替换，或已由快照对账提前回收。
    const record = this.#records.get(key);
    if (!record || record.revision.lifecycle !== "sealed") {
      return false;
    }
    if ((record.stableCoverageGeneration ?? -1) < record.revision.lastGeneration) {
      return false;
    }
    if ((this.#listeners.get(key)?.size ?? 0) > 0) {
      return false;
    }
    this.#records.delete(key);
    this.#dirtyKeys.delete(key);
    return true;
  }
}

// conversationLiveStreamKey builds the stable compound identity used by cache and React subscriptions.
// conversationLiveStreamKey 构建缓存与 React 订阅使用的稳定复合身份。
// Parameters: conversationId and messageId are exact backend-owned identities.
// 参数：conversationId 与 messageId 是后端持有的精确身份。
// Returns: one collision-safe compound key local to this Store.
// 返回值：当前 Store 内无冲突的复合键。
export function conversationLiveStreamKey(conversationId: string, messageId: string): string {
  return `${conversationId}\u0000${messageId}`;
}

// conversationMessageStreamState reads only the explicit backend-owned lifecycle metadata.
// conversationMessageStreamState 仅读取后端持有的显式生命周期元数据。
// Parameters: message is the transcript row whose metadata is inspected.
// 参数：message 是需要检查元数据的转录行。
// Returns: active or sealed when explicitly present, otherwise null.
// 返回值：显式存在时返回 active 或 sealed，否则返回 null。
export function conversationMessageStreamState(
  message: ConversationMessage | null | undefined,
): ConversationMessageStreamState | null {
  if (!message || !isUnknownRecord(message.metadata)) {
    return null;
  }
  // value is the exact metadata field written by the runtime streaming reducer.
  // value 是运行时流归约器写入的精确元数据字段。
  const value = message.metadata.stream_state;
  return value === "active" || value === "sealed" ? value : null;
}

// conversationLiveStreamKind narrows persisted transcript kinds accepted by the live cache.
// conversationLiveStreamKind 收窄实时缓存接受的持久化转录类型。
// Parameters: value is one persisted record-kind candidate.
// 参数：value 是一个持久化记录类型候选值。
// Returns: the supported live kind, or null for every other value.
// 返回值：支持的实时类型；其他值返回 null。
function conversationLiveStreamKind(value: string | null | undefined): ConversationLiveStreamKind | null {
  return value === "thinking-output" || value === "assistant-output" ? value : null;
}

// chronologicalFragmentsAfter returns linked fragments newer than one generation in append order.
// chronologicalFragmentsAfter 按追加顺序返回比指定 generation 更新的链式片段。
// Parameters: tail is the newest linked fragment and generation is the exclusive lower bound.
// 参数：tail 是最新链式片段，generation 是不包含自身的下界。
// Returns: matching nodes in chronological order.
// 返回值：按时间顺序排列的匹配节点。
function chronologicalFragmentsAfter(
  tail: LiveStreamFragmentNode | null,
  generation: number,
): LiveStreamFragmentNode[] {
  // fragments is collected newest-first before one final reversal.
  // fragments 先按最新优先收集，最后统一反转一次。
  const fragments: LiveStreamFragmentNode[] = [];
  // cursor traverses immutable historical fragment nodes.
  // cursor 遍历不可变历史片段节点。
  let cursor = tail;
  while (cursor && cursor.generation > generation) {
    fragments.push(cursor);
    cursor = cursor.previous;
  }
  fragments.reverse();
  return fragments;
}

// reverseFragmentList rebuilds one reverse-linked list from chronological fragments.
// reverseFragmentList 根据按时间排序的片段重建反向链表。
// Parameters: fragments contains nodes in oldest-to-newest order.
// 参数：fragments 包含从最旧到最新排序的节点。
// Returns: the newest tail of the rebuilt reverse list, or null for an empty input.
// 返回值：重建反向链的最新尾节点；输入为空时返回 null。
function reverseFragmentList(fragments: readonly LiveStreamFragmentNode[]): LiveStreamFragmentNode | null {
  // tail becomes the newest node after chronological insertion.
  // tail 在按时间插入后成为最新节点。
  let tail: LiveStreamFragmentNode | null = null;
  for (const fragment of fragments) {
    tail = {
      generation: fragment.generation,
      text: fragment.text,
      utf8ByteLength: fragment.utf8ByteLength,
      previous: tail,
    };
  }
  return tail;
}

// materializeRecordBody joins one cached record only for cold reads or terminal reconciliation.
// materializeRecordBody 仅在冷读取或终态对账时拼接一条缓存记录。
// Parameters: record owns one full base and every newer fragment.
// 参数：record 持有一个完整基线及其全部更新片段。
// Returns: the exact body represented by the current replica.
// 返回值：当前副本表示的精确正文。
function materializeRecordBody(record: ConversationLiveStreamRecord): string {
  // fragments is ordered once before the final body allocation.
  // fragments 在最终正文分配前仅排序一次。
  const fragments = chronologicalFragmentsAfter(
    record.fragmentTail,
    record.revision.baseGeneration,
  );
  return record.baseBody + fragments.map((fragment) => fragment.text).join("");
}

// utf8ByteLength computes one fragment's transport-size threshold exactly once.
// utf8ByteLength 仅计算一次单个片段的传输字节阈值。
// Parameters: value is the text whose encoded length is required.
// 参数：value 是需要计算编码长度的文本。
// Returns: the exact UTF-8 byte length.
// 返回值：精确 UTF-8 字节长度。
function utf8ByteLength(value: string): number {
  return UTF8_ENCODER.encode(value).byteLength;
}

// isUnknownRecord narrows structured transcript metadata without candidate fallback paths.
// isUnknownRecord 收窄结构化转录元数据，不使用候选回退路径。
// Parameters: value is one unknown metadata candidate.
// 参数：value 是一个未知元数据候选值。
// Returns: true only for a non-null, non-array object record.
// 返回值：仅对非空且非数组的对象记录返回 true。
function isUnknownRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
