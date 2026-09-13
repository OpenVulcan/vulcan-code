import type {
  ConversationProcessRenderItem,
  ConversationRenderItem,
  ConversationTimelineItem,
} from "@/components/conversation/conversation-model";

// ConversationTurnProjection is the immutable row snapshot consumed by one Turn Seat.
// ConversationTurnProjection 是单个 Turn Seat 消费的不可变行快照。
export interface ConversationTurnProjection {
  // Stable Turn identity inside one conversation surface.
  // 单个会话界面内的稳定 Turn 身份。
  key: string;
  // Ordered timeline items owned by this Turn.
  // 当前 Turn 拥有的有序时间线条目。
  items: ConversationTimelineItem[];
}

// ConversationTimelineProjectionStore retains unchanged Turn snapshots and notifies only changed row subscribers.
// ConversationTimelineProjectionStore 保留未变化的 Turn 快照，并且只通知发生变化的行订阅者。
export class ConversationTimelineProjectionStore {
  // rows stores the latest immutable snapshot for every live Turn key.
  // rows 保存每个存活 Turn 键对应的最新不可变快照。
  private readonly rows = new Map<string, ConversationTurnProjection>();
  // listeners stores exact row subscriptions without introducing a global render subscription.
  // listeners 保存精确行订阅，避免引入全局渲染订阅。
  private readonly listeners = new Map<string, Set<() => void>>();
  // pendingNotificationKeys collects changed rows until React commits the parent projection.
  // pendingNotificationKeys 收集发生变化的行，等待 React 提交父级投影后统一通知。
  private readonly pendingNotificationKeys = new Set<string>();
  // orderSnapshot stores the stable ordered row-key array exposed to the parent.
  // orderSnapshot 保存向父级暴露的稳定有序行键数组。
  private orderSnapshot: readonly string[] = [];

  // reconcile applies one authoritative ordered projection while preserving equal row references.
  // reconcile 应用一份权威有序投影，同时保留相同行的引用。
  // Parameters: nextRows contains every rendered Turn; replaceAll explicitly invalidates projection-wide context such as locale.
  // 参数：nextRows 包含全部已渲染 Turn；replaceAll 显式使语言等投影级上下文失效。
  // Returns: the stable ordered row-key snapshot after reconciliation.
  // 返回值：协调完成后的稳定有序行键快照。
  reconcile(
    nextRows: readonly ConversationTurnProjection[],
    replaceAll: boolean = false,
  ): readonly string[] {
    // nextKeys is the authoritative live-key set used to remove stale row snapshots.
    // nextKeys 是用于移除过期行快照的权威存活键集合。
    const nextKeys = new Set<string>();
    // nextOrder is the exact visual ordering represented by the incoming projection.
    // nextOrder 是传入投影所表达的精确视觉顺序。
    const nextOrder: string[] = [];
    for (const nextRow of nextRows) {
      nextKeys.add(nextRow.key);
      nextOrder.push(nextRow.key);
      // currentRow is the previously retained snapshot for the same stable key.
      // currentRow 是此前为相同稳定键保留的快照。
      const currentRow = this.rows.get(nextRow.key);
      if (!replaceAll && currentRow && conversationTurnProjectionEqual(currentRow, nextRow)) {
        continue;
      }
      this.rows.set(nextRow.key, nextRow);
      this.pendingNotificationKeys.add(nextRow.key);
    }
    for (const currentKey of this.rows.keys()) {
      if (nextKeys.has(currentKey)) {
        continue;
      }
      this.rows.delete(currentKey);
      this.pendingNotificationKeys.add(currentKey);
    }
    if (!stringArrayEqual(this.orderSnapshot, nextOrder)) {
      this.orderSnapshot = nextOrder;
    }
    return this.orderSnapshot;
  }

  // getOrderSnapshot returns the current stable visual row order.
  // getOrderSnapshot 返回当前稳定的视觉行顺序。
  // Parameters: none.
  // 参数：无。
  // Returns: the immutable row-key sequence retained by the store.
  // 返回值：仓库保留的不可变行键序列。
  getOrderSnapshot(): readonly string[] {
    return this.orderSnapshot;
  }

  // getRowSnapshot returns the exact immutable projection for one live Turn.
  // getRowSnapshot 返回单个存活 Turn 的精确不可变投影。
  // Parameters: rowKey is the stable Turn key already present in the current order snapshot.
  // 参数：rowKey 是当前顺序快照中已存在的稳定 Turn 键。
  // Returns: the retained row snapshot; a missing authoritative key is reported immediately.
  // 返回值：已保留的行快照；权威键缺失时立即显式报错。
  getRowSnapshot(rowKey: string): ConversationTurnProjection {
    // row is the authoritative projection registered for the requested key.
    // row 是为请求键注册的权威投影。
    const row = this.rows.get(rowKey);
    if (!row) {
      throw new Error(`Conversation timeline row is not registered: ${rowKey}`);
    }
    return row;
  }

  // getRowSnapshotOr returns committed row state or an explicit first-mount fallback.
  // getRowSnapshotOr 返回已提交行状态，或返回显式的首次挂载回退。
  // Parameters: rowKey is the stable Turn key and fallback is the parent projection for a not-yet-committed new Seat.
  // 参数：rowKey 是稳定 Turn 键；fallback 是尚未提交的新 Seat 对应的父级投影。
  // Returns: the committed row when present, otherwise the exact fallback supplied by the current render.
  // 返回值：存在时返回已提交行，否则返回当前渲染提供的精确回退。
  getRowSnapshotOr(
    rowKey: string,
    fallback: ConversationTurnProjection,
  ): ConversationTurnProjection {
    return this.rows.get(rowKey) ?? fallback;
  }

  // subscribe registers one listener for one exact Turn key.
  // subscribe 为一个精确 Turn 键注册一个监听器。
  // Parameters: rowKey selects the Turn; listener is invoked only when that Turn changes or disappears.
  // 参数：rowKey 选择 Turn；listener 仅在该 Turn 变化或消失时调用。
  // Returns: an unsubscribe callback that releases the exact listener.
  // 返回值：释放该精确监听器的退订回调。
  subscribe(rowKey: string, listener: () => void): () => void {
    // rowListeners is the existing listener set or a new set for the requested key.
    // rowListeners 是请求键已有的监听器集合，若不存在则创建新集合。
    const rowListeners = this.listeners.get(rowKey) ?? new Set<() => void>();
    rowListeners.add(listener);
    this.listeners.set(rowKey, rowListeners);
    return () => {
      rowListeners.delete(listener);
      if (rowListeners.size === 0) {
        this.listeners.delete(rowKey);
      }
    };
  }

  // flushNotifications publishes staged row changes after the parent projection commits.
  // flushNotifications 在父级投影提交后发布已暂存的行变化。
  // Parameters: none.
  // 参数：无。
  // Returns: nothing; each changed live row listener is invoked at most once.
  // 返回值：无；每个已变化存活行的监听器最多调用一次。
  flushNotifications(): void {
    // notificationKeys freezes this flush so listener work may safely stage later changes.
    // notificationKeys 固定本轮通知集合，使监听器工作可以安全暂存后续变化。
    const notificationKeys = Array.from(this.pendingNotificationKeys);
    this.pendingNotificationKeys.clear();
    for (const rowKey of notificationKeys) {
      for (const listener of this.listeners.get(rowKey) ?? []) {
        listener();
      }
    }
  }

  // invalidateRows refreshes only live Turns whose external rendering dependencies changed.
  // invalidateRows 仅刷新外部渲染依赖发生变化的存活 Turn。
  // Parameters: predicate receives each retained immutable Turn projection.
  // 参数：predicate 接收每个已保留的不可变 Turn 投影。
  // Returns: the number of row snapshots staged for notification.
  // 返回值：已暂存通知的行快照数量。
  invalidateRows(predicate: (row: ConversationTurnProjection) => boolean): number {
    let invalidated = 0;
    for (const [rowKey, row] of this.rows) {
      if (!predicate(row)) {
        continue;
      }
      this.rows.set(rowKey, { ...row });
      this.pendingNotificationKeys.add(rowKey);
      invalidated += 1;
    }
    return invalidated;
  }
}

// conversationTurnProjectionEqual compares one Turn through exact projection sources and process state.
// conversationTurnProjectionEqual 通过精确投影来源与过程状态比较单个 Turn。
// Parameters: left and right are projections sharing the same stable row key.
// 参数：left 与 right 是共享相同稳定行键的投影。
// Returns: true when rendering either projection produces the same business row content.
// 返回值：当两个投影会生成相同业务行内容时返回 true。
export function conversationTurnProjectionEqual(
  left: ConversationTurnProjection,
  right: ConversationTurnProjection,
): boolean {
  if (left === right) {
    return true;
  }
  if (left.key !== right.key || left.items.length !== right.items.length) {
    return false;
  }
  return left.items.every((item, index) => conversationTimelineItemEqual(item, right.items[index]));
}

// conversationTimelineItemEqual compares one typed timeline item without serializing Markdown bodies.
// conversationTimelineItemEqual 在不序列化 Markdown 正文的情况下比较单个类型化时间线条目。
// Parameters: left and right occupy the same ordered position in one Turn.
// 参数：left 与 right 位于同一 Turn 的相同有序位置。
// Returns: true when their typed kind, source messages, and process state are unchanged.
// 返回值：当其类型、来源消息与过程状态均未变化时返回 true。
function conversationTimelineItemEqual(
  left: ConversationTimelineItem,
  right: ConversationTimelineItem | undefined,
): boolean {
  if (!right || left.kind !== right.kind) {
    return false;
  }
  if (left.kind === "fork-source") {
    return true;
  }
  if (left.kind === "item" && right.kind === "item") {
    return left.finalAssistantOutput === right.finalAssistantOutput
      && conversationRenderItemEqual(left.item, right.item);
  }
  if (left.kind === "final-assistant-meta" && right.kind === "final-assistant-meta") {
    return conversationRenderItemEqual(left.item, right.item);
  }
  if (left.kind === "process" && right.kind === "process") {
    return conversationProcessRenderItemEqual(left.process, right.process);
  }
  return false;
}

// conversationProcessRenderItemEqual compares exact process state and every source-backed child row.
// conversationProcessRenderItemEqual 比较精确过程状态与每个由来源消息支撑的子行。
// Parameters: left and right are process projections for the same visual position.
// 参数：left 与 right 是相同视觉位置的过程投影。
// Returns: true when header state, timing, and ordered child rows are unchanged.
// 返回值：当标题状态、计时与有序子行均未变化时返回 true。
function conversationProcessRenderItemEqual(
  left: ConversationProcessRenderItem,
  right: ConversationProcessRenderItem,
): boolean {
  return left.id === right.id
    && left.renderKey === right.renderKey
    && left.collapsible === right.collapsible
    && left.defaultCollapsed === right.defaultCollapsed
    && left.running === right.running
    && left.stopped === right.stopped
    && left.startedAtMs === right.startedAtMs
    && left.durationMs === right.durationMs
    && conversationRenderItemArrayEqual(left.items, right.items)
    && conversationRenderItemArrayEqual(left.guidanceItems, right.guidanceItems);
}

// conversationRenderItemArrayEqual compares an ordered child collection through exact row sources.
// conversationRenderItemArrayEqual 通过精确行来源比较一个有序子项集合。
// Parameters: left and right are ordered render-item collections.
// 参数：left 与 right 是有序渲染项集合。
// Returns: true when every position is source-equivalent.
// 返回值：当每个位置的来源均等价时返回 true。
function conversationRenderItemArrayEqual(
  left: readonly ConversationRenderItem[],
  right: readonly ConversationRenderItem[],
): boolean {
  return left.length === right.length
    && left.every((item, index) => conversationRenderItemEqual(item, right[index]));
}

// conversationRenderItemEqual compares deterministic projection identity through its contributing messages.
// conversationRenderItemEqual 通过贡献消息比较确定性的投影身份。
// Parameters: left and right are render items produced by the same transcript projector contract.
// 参数：left 与 right 是由同一转录投影器契约生成的渲染项。
// Returns: true when identity, role, and every contributing immutable message reference match.
// 返回值：当身份、角色及全部贡献不可变消息引用均匹配时返回 true。
function conversationRenderItemEqual(
  left: ConversationRenderItem,
  right: ConversationRenderItem | undefined,
): boolean {
  if (!right) {
    return false;
  }
  if (left === right) {
    return true;
  }
  if (left.id !== right.id || left.renderKey !== right.renderKey || left.role !== right.role) {
    return false;
  }
  return left.reserveThinkingPrefix === right.reserveThinkingPrefix
    && referenceArrayEqual(left.sourceMessages, right.sourceMessages);
}

// referenceArrayEqual compares ordered immutable source arrays by exact reference.
// referenceArrayEqual 按精确引用比较有序不可变来源数组。
// Parameters: left and right are source arrays produced by the transcript projector.
// 参数：left 与 right 是转录投影器生成的来源数组。
// Returns: true when both arrays contain the same references in the same order.
// 返回值：当两个数组以相同顺序包含相同引用时返回 true。
function referenceArrayEqual<T>(left: readonly T[], right: readonly T[]): boolean {
  return left.length === right.length && left.every((value, index) => value === right[index]);
}

// stringArrayEqual compares ordered stable row-key snapshots.
// stringArrayEqual 比较有序稳定行键快照。
// Parameters: left and right are immutable key sequences.
// 参数：left 与 right 是不可变键序列。
// Returns: true when both sequences contain identical keys in identical order.
// 返回值：当两个序列以相同顺序包含相同键时返回 true。
function stringArrayEqual(left: readonly string[], right: readonly string[]): boolean {
  return left.length === right.length && left.every((value, index) => value === right[index]);
}
