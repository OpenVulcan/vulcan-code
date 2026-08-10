// QueuedComposerPersistence contains one ordered persistence result and its non-rejecting queue tail.
// QueuedComposerPersistence 包含一项有序持久化结果及其不会拒绝的队列尾部。
export interface QueuedComposerPersistence<T> {
  // Exact result returned by the queued persistence operation.
  // 排队持久化操作返回的精确结果。
  result: Promise<T>;
  // Settled queue tail that allows later writes to run even when this operation rejects.
  // 无论本次操作是否拒绝都会完成的队列尾部，使后续写入仍可执行。
  tail: Promise<void>;
}

// confirmedComposerPersistenceValue advances a rollback baseline only after persistence succeeds.
// confirmedComposerPersistenceValue 仅在持久化成功后推进回滚基线。
// Parameters: confirmed is the former durable value, attempted is the queued intent, and saved is its result.
// 参数：confirmed 是先前持久值，attempted 是排队意图，saved 是其结果。
// Returns: the attempted value on success or the former confirmed value on failure.
// 返回值：成功时返回尝试值，失败时返回先前确认值。
export function confirmedComposerPersistenceValue<T>(
  confirmed: T,
  attempted: T,
  saved: boolean,
): T {
  return saved ? attempted : confirmed;
}

// enqueueComposerPersistence serializes one persistence operation after the former queue tail.
// enqueueComposerPersistence 将一项持久化操作串行安排在前一队列尾部之后。
// Parameters: previousTail is the settled ordering boundary and persist performs one exact write.
// 参数：previousTail 是既有排序边界，persist 执行一次精确写入。
// Returns: the exact operation result plus a non-rejecting tail for the next operation.
// 返回值：精确操作结果，以及供下一项操作使用的不会拒绝的队列尾部。
export function enqueueComposerPersistence<T>(
  previousTail: Promise<void>,
  persist: () => Promise<T>,
): QueuedComposerPersistence<T> {
  // result runs after both successful and failed former operations so a transient write failure
  // cannot permanently stall later user intent.
  // result 在前一操作成功或失败后都会运行，因此一次临时写入失败不会永久阻塞后续用户意图。
  const result = previousTail.then(persist, persist);
  // tail deliberately absorbs this result while preserving the exact rejection on `result`.
  // tail 有意吸收本次结果，同时在 result 上保留精确拒绝。
  const tail = result.then(
    () => undefined,
    () => undefined,
  );
  return { result, tail };
}

// flushLatestComposerPersistence waits until the latest observable boolean persistence result settles.
// flushLatestComposerPersistence 持续等待，直到可观察到的最新布尔持久化结果结束。
// Parameters: readPending returns the current persistence result or null when no write remains.
// 参数：readPending 返回当前持久化结果；没有剩余写入时返回 null。
// Returns: the latest intent result, or true when no persistence write is pending.
// 返回值：返回最新意图的结果；没有待处理持久化写入时返回 true。
export async function flushLatestComposerPersistence(
  readPending: () => Promise<boolean> | null,
): Promise<boolean> {
  while (true) {
    // pending freezes one observable queue member for this wait iteration.
    // pending 为本次等待迭代冻结一个可观察队列成员。
    const pending = readPending();
    if (!pending) {
      return true;
    }
    // saved retains the exact result even when a newer intent appears while this promise settles.
    // saved 保留精确结果，即使本 Promise 结束期间出现了更新意图。
    const saved = await pending;
    // latest detects whether the user appended a newer persistence intent during the wait.
    // latest 检测用户是否在等待期间追加了更新的持久化意图。
    const latest = readPending();
    if (latest && latest !== pending) {
      continue;
    }
    return saved;
  }
}
