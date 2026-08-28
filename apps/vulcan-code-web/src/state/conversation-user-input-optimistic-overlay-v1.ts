import type {
  ClientActionId,
  ConversationEventBody,
  ConversationEventSummary,
  ConversationEventWindowOwner,
} from "@/service/conversation-event-window-store";

/** Process-local attachment information rendered by an optimistic user-input row.
 * 乐观用户输入行渲染的进程内附件信息。 */
export interface UserInputOptimisticAttachmentDisplayV1 {
  /** Stable process-local display key; it is not a durable ResourceId.
   * 稳定的进程内展示键；它不是持久化 ResourceId。 */
  display_id: string;
  /** Attachment kind shown by the current composer UI.
   * 当前输入区界面显示的附件类型。 */
  kind: string;
  /** Human-readable attachment name.
   * 人类可读的附件名称。 */
  file_name: string;
  /** MIME type used only for process-local presentation.
   * 仅供进程内展示使用的 MIME 类型。 */
  mime_type: string;
}

/** Exact owner and visible payload captured before one user-input RPC starts.
 * 一次用户输入 RPC 开始前捕获的精确归属与可见载荷。 */
export interface UserInputOptimisticSubmissionV1 {
  /** Exact selected V1 event-window owner.
   * 精确的选中 V1 事件窗口归属。 */
  owner: ConversationEventWindowOwner;
  /** Browser-generated idempotency key and the only reconciliation identity.
   * 浏览器生成的幂等键，也是唯一对账身份。 */
  client_action_id: ClientActionId;
  /** User-visible input text.
   * 用户可见输入文本。 */
  text: string;
  /** Process-local attachment presentation copied from the draft.
   * 从草稿复制的进程内附件展示信息。 */
  attachments: readonly UserInputOptimisticAttachmentDisplayV1[];
  /** Local start timestamp used only for display ordering and status.
   * 仅用于展示顺序与状态的本地开始时间。 */
  started_at_ms: number;
}

/** Visible optimistic phase before the canonical V1 user_input event takes over.
 * canonical V1 user_input 事件接管前的可见乐观阶段。 */
export type UserInputOptimisticVisiblePhaseV1 = "submitting" | "acknowledged";

/** One process-local optimistic row exposed to the future V1 renderer.
 * 暴露给未来 V1 renderer 的一条进程内乐观行。 */
export interface UserInputOptimisticOverlayItemV1 extends UserInputOptimisticSubmissionV1 {
  /** Current visible submission phase; ACK never removes the row.
   * 当前可见提交阶段；ACK 绝不移除该行。 */
  phase: UserInputOptimisticVisiblePhaseV1;
}

/** Immutable display snapshot of one owner-bound optimistic overlay store.
 * 一份按 owner 绑定的乐观 overlay store 不可变展示快照。 */
export interface ConversationUserInputOptimisticOverlaySnapshotV1 {
  /** Current owner, or null after disposal and before initial ownership.
   * 当前归属；释放后及初始归属前为空。 */
  owner: ConversationEventWindowOwner | null;
  /** Active rows in browser submission order; these rows never own canonical seq values.
   * 按浏览器提交顺序排列的活动行；这些行绝不拥有 canonical seq。 */
  overlays: readonly UserInputOptimisticOverlayItemV1[];
  /** Whether the store has permanently released its process-local state.
   * store 是否已永久释放其进程内状态。 */
  disposed: boolean;
}

/** Exact V1 user_input body used for optimistic reconciliation.
 * 用于乐观对账的精确 V1 user_input 事件体。 */
export type ConversationUserInputBodyV1 = Extract<ConversationEventBody, { kind: "user_input" }>;

/** Canonical V1 event accepted by the optimistic overlay reducer.
 * 乐观 overlay reducer 接受的 canonical V1 事件。 */
export type ConversationUserInputSummaryV1 = Omit<ConversationEventSummary, "event"> & {
  event: ConversationUserInputBodyV1;
};

/** Terminal lifecycle retained only to make duplicate browser actions deterministic.
 * 仅为使重复浏览器动作保持确定性而保留的终态生命周期。 */
type UserInputOptimisticTerminalPhaseV1 = "reconciled" | "rpc_failed" | "retired";

/** Complete process-local lifecycle retained for one ClientActionId.
 * 为一个 ClientActionId 保留的完整进程内生命周期。 */
export type UserInputOptimisticRecordPhaseV1 =
  | UserInputOptimisticVisiblePhaseV1
  | UserInputOptimisticTerminalPhaseV1;

/** Internal immutable record kept after visibility ends to reject identity drift.
 * 可见性结束后仍保留、用于拒绝身份漂移的内部不可变记录。 */
export interface UserInputOptimisticRecordV1 extends UserInputOptimisticSubmissionV1 {
  /** Complete process-local lifecycle phase.
   * 完整的进程内生命周期阶段。 */
  phase: UserInputOptimisticRecordPhaseV1;
}

/** Pure reducer state for owner-bound optimistic input reconciliation.
 * 按 owner 绑定的乐观输入对账纯 reducer 状态。 */
export interface ConversationUserInputOptimisticOverlayStateV1 {
  /** Current exact owner.
   * 当前精确归属。 */
  owner: ConversationEventWindowOwner | null;
  /** Browser action ledger retained only in process memory.
   * 仅保留于进程内存的浏览器动作账本。 */
  records: ReadonlyMap<ClientActionId, UserInputOptimisticRecordV1>;
  /** Permanent lifecycle flag.
   * 永久生命周期标记。 */
  disposed: boolean;
}

/** Pure state-machine input accepted by the optimistic overlay reducer.
 * 乐观 overlay reducer 接受的纯状态机输入。 */
export type ConversationUserInputOptimisticOverlayActionV1 =
  | { kind: "owner_rebased"; owner: ConversationEventWindowOwner }
  | { kind: "submission_started"; submission: UserInputOptimisticSubmissionV1 }
  | { kind: "ack_received"; owner: ConversationEventWindowOwner; client_action_id: ClientActionId }
  | { kind: "rpc_failed"; owner: ConversationEventWindowOwner; client_action_id: ClientActionId }
  | { kind: "user_input_observed"; owner: ConversationEventWindowOwner; event: ConversationUserInputSummaryV1 }
  | { kind: "disposed" };

/** Listener notified after one atomic overlay state publication.
 * 一次 overlay 状态原子发布后收到通知的监听器。 */
export type ConversationUserInputOptimisticOverlayListenerV1 = () => void;

/** Explicit error raised when one ClientActionId drifts to another owner or payload.
 * 一个 ClientActionId 漂移到其他归属或载荷时抛出的显式错误。 */
export class ConversationUserInputOptimisticConflictError extends Error {
  /** Conflicting browser action identity.
   * 发生冲突的浏览器动作身份。 */
  readonly clientActionId: ClientActionId;

  /** Creates one exact ClientActionId conflict diagnostic.
   * 创建一条精确的 ClientActionId 冲突诊断。 */
  constructor(clientActionId: ClientActionId) {
    super(`Optimistic user input conflict for client action '${clientActionId}'.`);
    this.name = "ConversationUserInputOptimisticConflictError";
    this.clientActionId = clientActionId;
  }
}

/** Explicit error raised when a submission targets a non-current owner.
 * 提交指向非当前归属时抛出的显式错误。 */
export class ConversationUserInputOptimisticOwnerMismatchError extends Error {
  /** Creates one owner mismatch diagnostic without redirecting the submission.
   * 创建一条归属不匹配诊断，且不会重定向提交。 */
  constructor() {
    super("Optimistic user input submission does not belong to the current event-window owner.");
    this.name = "ConversationUserInputOptimisticOwnerMismatchError";
  }
}

/** Explicit error raised when a disposed process-local store is mutated.
 * 修改已释放进程内 store 时抛出的显式错误。 */
export class ConversationUserInputOptimisticDisposedError extends Error {
  /** Creates one permanent-disposal diagnostic.
   * 创建一条永久释放诊断。 */
  constructor() {
    super("Optimistic user input overlay store has been disposed.");
    this.name = "ConversationUserInputOptimisticDisposedError";
  }
}

/** Creates the empty process-local reducer state.
 * 创建空的进程内 reducer 状态。 */
export function createConversationUserInputOptimisticOverlayStateV1(): ConversationUserInputOptimisticOverlayStateV1 {
  return {
    owner: null,
    records: new Map(),
    disposed: false,
  };
}

/** Reduces one exact optimistic lifecycle action without side effects.
 * 无副作用地归约一条精确乐观生命周期动作。 */
export function reduceConversationUserInputOptimisticOverlayV1(
  state: ConversationUserInputOptimisticOverlayStateV1,
  action: ConversationUserInputOptimisticOverlayActionV1,
): ConversationUserInputOptimisticOverlayStateV1 {
  if (action.kind === "disposed") {
    return disposeState(state);
  }
  if (state.disposed) {
    throw new ConversationUserInputOptimisticDisposedError();
  }

  switch (action.kind) {
    case "owner_rebased":
      return rebaseOwner(state, action.owner);
    case "submission_started":
      return startSubmission(state, action.submission);
    case "ack_received":
      return updateMatchingRecord(state, action.owner, action.client_action_id, "acknowledged");
    case "rpc_failed":
      return updateMatchingRecord(state, action.owner, action.client_action_id, "rpc_failed");
    case "user_input_observed":
      return reconcileUserInput(state, action.owner, action.event);
  }
}

/** Owner-bound process-local store used by the future Stage 11 renderer bridge.
 * 供未来 Stage 11 renderer bridge 使用的按 owner 绑定进程内 store。 */
export class ConversationUserInputOptimisticOverlayStoreV1 {
  /** Current immutable reducer state.
   * 当前不可变 reducer 状态。 */
  private currentState = createConversationUserInputOptimisticOverlayStateV1();
  /** Current immutable display snapshot.
   * 当前不可变展示快照。 */
  private currentSnapshot = snapshotFromState(this.currentState);
  /** Active publication listeners.
   * 活动发布监听器。 */
  private readonly listeners = new Set<ConversationUserInputOptimisticOverlayListenerV1>();

  /** Returns the current immutable process-local snapshot.
   * 返回当前不可变进程内快照。 */
  snapshot(): ConversationUserInputOptimisticOverlaySnapshotV1 {
    return this.currentSnapshot;
  }

  /** Subscribes to atomic overlay publications.
   * 订阅 overlay 原子发布。 */
  subscribe(listener: ConversationUserInputOptimisticOverlayListenerV1): () => void {
    if (this.currentState.disposed) {
      return () => undefined;
    }
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /** Establishes or rebases the exact selected-session owner.
   * 建立或重建精确的选中会话归属。 */
  rebaseOwner(owner: ConversationEventWindowOwner): void {
    this.dispatch({ kind: "owner_rebased", owner });
  }

  /** Registers one browser action before its RPC starts.
   * 在 RPC 开始前登记一个浏览器动作。 */
  startSubmission(submission: UserInputOptimisticSubmissionV1): void {
    this.dispatch({ kind: "submission_started", submission });
  }

  /** Records an ACK without removing the optimistic row.
   * 记录 ACK，但不移除乐观行。 */
  acknowledge(owner: ConversationEventWindowOwner, clientActionId: ClientActionId): void {
    this.dispatch({ kind: "ack_received", owner, client_action_id: clientActionId });
  }

  /** Applies the existing product failure policy by removing the failed optimistic row.
   * 按现有产品失败策略移除失败的乐观行。 */
  failRpc(owner: ConversationEventWindowOwner, clientActionId: ClientActionId): void {
    this.dispatch({ kind: "rpc_failed", owner, client_action_id: clientActionId });
  }

  /** Reconciles one canonical V1 user_input event by exact owner and ClientActionId.
   * 按精确 owner 与 ClientActionId 对账一条 canonical V1 user_input 事件。 */
  observeUserInput(owner: ConversationEventWindowOwner, event: ConversationUserInputSummaryV1): void {
    this.dispatch({ kind: "user_input_observed", owner, event });
  }

  /** Permanently releases the process-local overlay and listeners.
   * 永久释放进程内 overlay 与监听器。 */
  dispose(): void {
    if (this.currentState.disposed) {
      return;
    }
    this.dispatch({ kind: "disposed" });
    this.listeners.clear();
  }

  /** Applies one reducer action and publishes only real state changes.
   * 应用一条 reducer 动作，并且仅发布真实状态变化。 */
  private dispatch(action: ConversationUserInputOptimisticOverlayActionV1): void {
    // nextState is fully validated before the store swaps either state or snapshot.
    // nextState 在 store 替换状态或快照前已完成完整校验。
    const nextState = reduceConversationUserInputOptimisticOverlayV1(this.currentState, action);
    if (nextState === this.currentState) {
      return;
    }
    this.currentState = nextState;
    this.currentSnapshot = snapshotFromState(nextState);
    for (const listener of this.listeners) {
      listener();
    }
  }
}

/** Rebases ownership and permanently retires every still-visible row from the prior generation.
 * 重建归属，并永久退役上一代仍可见的每一行。 */
function rebaseOwner(
  state: ConversationUserInputOptimisticOverlayStateV1,
  owner: ConversationEventWindowOwner,
): ConversationUserInputOptimisticOverlayStateV1 {
  assertOwner(owner);
  if (sameOwner(state.owner, owner)) {
    return state;
  }
  // records retains terminal identities while preventing a later switch-back from reviving old rows.
  // records 保留终态身份，同时防止后续切回时复活旧行。
  const records = new Map(state.records);
  for (const [clientActionId, record] of records) {
    if (isVisiblePhase(record.phase)) {
      records.set(clientActionId, { ...record, phase: "retired" });
    }
  }
  return { owner: copyOwner(owner), records, disposed: false };
}

/** Starts one idempotent browser action or rejects payload and ownership drift.
 * 启动一个幂等浏览器动作，或拒绝载荷及归属漂移。 */
function startSubmission(
  state: ConversationUserInputOptimisticOverlayStateV1,
  submission: UserInputOptimisticSubmissionV1,
): ConversationUserInputOptimisticOverlayStateV1 {
  assertOwner(submission.owner);
  assertStartedAt(submission.started_at_ms);
  if (!sameOwner(state.owner, submission.owner)) {
    throw new ConversationUserInputOptimisticOwnerMismatchError();
  }
  // existing binds the browser action globally inside this process-local store lifetime.
  // existing 在当前进程内 store 生命周期中全局绑定该浏览器动作。
  const existing = state.records.get(submission.client_action_id);
  if (existing) {
    if (!sameSubmission(existing, submission)) {
      throw new ConversationUserInputOptimisticConflictError(submission.client_action_id);
    }
    return state;
  }
  // record copies caller-owned attachment objects before publication.
  // record 在发布前复制调用方持有的附件对象。
  const record: UserInputOptimisticRecordV1 = {
    ...submission,
    owner: copyOwner(submission.owner),
    attachments: submission.attachments.map(copyAttachment),
    phase: "submitting",
  };
  const records = new Map(state.records);
  records.set(record.client_action_id, record);
  return { ...state, records };
}

/** Updates one matching active record without accepting stale-owner redirection.
 * 更新一条匹配的活动记录，且不接受旧 owner 重定向。 */
function updateMatchingRecord(
  state: ConversationUserInputOptimisticOverlayStateV1,
  owner: ConversationEventWindowOwner,
  clientActionId: ClientActionId,
  nextPhase: "acknowledged" | "rpc_failed",
): ConversationUserInputOptimisticOverlayStateV1 {
  assertOwner(owner);
  if (!sameOwner(state.owner, owner)) {
    return state;
  }
  // existing may be absent when the controller was created after the browser action began.
  // 当控制器在浏览器动作开始后才创建时，existing 可能不存在。
  const existing = state.records.get(clientActionId);
  if (!existing || !sameOwner(existing.owner, owner) || !isVisiblePhase(existing.phase)) {
    return state;
  }
  if (existing.phase === nextPhase) {
    return state;
  }
  const records = new Map(state.records);
  records.set(clientActionId, { ...existing, phase: nextPhase });
  return { ...state, records };
}

/** Reconciles an authoritative event and rejects same-action text drift.
 * 对账权威事件，并拒绝同一动作的文本漂移。 */
function reconcileUserInput(
  state: ConversationUserInputOptimisticOverlayStateV1,
  owner: ConversationEventWindowOwner,
  event: ConversationUserInputSummaryV1,
): ConversationUserInputOptimisticOverlayStateV1 {
  assertOwner(owner);
  if (!sameOwner(state.owner, owner)) {
    return state;
  }
  // existing identifies only a browser action started by this process-local store.
  // existing 仅标识由当前进程内 store 启动的浏览器动作。
  const existing = state.records.get(event.event.client_action_id);
  if (!existing || !sameOwner(existing.owner, owner)) {
    return state;
  }
  if (existing.text !== event.event.text) {
    throw new ConversationUserInputOptimisticConflictError(event.event.client_action_id);
  }
  if (existing.phase === "reconciled" || existing.phase === "retired") {
    return state;
  }
  const records = new Map(state.records);
  records.set(event.event.client_action_id, { ...existing, phase: "reconciled" });
  return { ...state, records };
}

/** Permanently disposes one state while retaining tombstones only inside the unreachable state object.
 * 永久释放一份状态，同时仅在不可再访问的状态对象内保留墓碑。 */
function disposeState(
  state: ConversationUserInputOptimisticOverlayStateV1,
): ConversationUserInputOptimisticOverlayStateV1 {
  if (state.disposed) {
    return state;
  }
  return {
    owner: null,
    records: new Map(),
    disposed: true,
  };
}

/** Builds one immutable display snapshot without exposing terminal action records.
 * 构建一份不暴露终态动作记录的不可变展示快照。 */
function snapshotFromState(
  state: ConversationUserInputOptimisticOverlayStateV1,
): ConversationUserInputOptimisticOverlaySnapshotV1 {
  // overlays follows Map insertion order and never synthesizes an event seq.
  // overlays 遵循 Map 插入顺序，且绝不合成事件 seq。
  const overlays = Array.from(state.records.values())
    .filter((record) => isVisiblePhase(record.phase) && sameOwner(record.owner, state.owner))
    .map((record): UserInputOptimisticOverlayItemV1 => ({
      owner: copyOwner(record.owner),
      client_action_id: record.client_action_id,
      text: record.text,
      attachments: record.attachments.map(copyAttachment),
      started_at_ms: record.started_at_ms,
      phase: record.phase as UserInputOptimisticVisiblePhaseV1,
    }));
  return {
    owner: state.owner ? copyOwner(state.owner) : null,
    overlays,
    disposed: state.disposed,
  };
}

/** Compares exact session and generation ownership.
 * 比较精确的会话与代际归属。 */
function sameOwner(
  left: ConversationEventWindowOwner | null,
  right: ConversationEventWindowOwner | null,
): boolean {
  if (!left || !right) {
    return left === right;
  }
  return left.session_id === right.session_id && left.window_generation === right.window_generation;
}

/** Validates one exact V1 event-window owner.
 * 校验一个精确 V1 事件窗口归属。 */
function assertOwner(owner: ConversationEventWindowOwner): void {
  if (
    owner.session_id.trim().length === 0
    || !Number.isSafeInteger(owner.window_generation)
    || owner.window_generation <= 0
  ) {
    throw new Error("Optimistic user input owner must contain a session id and a positive safe generation.");
  }
}

/** Validates one process-local display timestamp.
 * 校验一个进程内展示时间戳。 */
function assertStartedAt(startedAtMs: number): void {
  if (!Number.isSafeInteger(startedAtMs) || startedAtMs < 0) {
    throw new Error("Optimistic user input started_at_ms must be a non-negative safe integer.");
  }
}

/** Compares one immutable browser action payload and owner.
 * 比较一份不可变浏览器动作载荷与归属。 */
function sameSubmission(
  existing: UserInputOptimisticRecordV1,
  incoming: UserInputOptimisticSubmissionV1,
): boolean {
  return sameOwner(existing.owner, incoming.owner)
    && existing.text === incoming.text
    && sameAttachments(existing.attachments, incoming.attachments);
}

/** Compares ordered process-local attachment presentation without path fallback.
 * 不使用路径 fallback，比较有序进程内附件展示信息。 */
function sameAttachments(
  left: readonly UserInputOptimisticAttachmentDisplayV1[],
  right: readonly UserInputOptimisticAttachmentDisplayV1[],
): boolean {
  return left.length === right.length && left.every((attachment, index) => {
    // candidate occupies the same user-visible attachment position.
    // candidate 占据相同的用户可见附件位置。
    const candidate = right[index];
    return candidate !== undefined
      && attachment.display_id === candidate.display_id
      && attachment.kind === candidate.kind
      && attachment.file_name === candidate.file_name
      && attachment.mime_type === candidate.mime_type;
  });
}

/** Reports whether one record remains visible before authoritative event reconciliation.
 * 判断一条记录在权威事件对账前是否仍可见。 */
function isVisiblePhase(phase: UserInputOptimisticRecordPhaseV1): phase is UserInputOptimisticVisiblePhaseV1 {
  return phase === "submitting" || phase === "acknowledged";
}

/** Copies one owner so caller mutation cannot redirect an existing overlay.
 * 复制一个 owner，防止调用方修改重定向已有 overlay。 */
function copyOwner(owner: ConversationEventWindowOwner): ConversationEventWindowOwner {
  return {
    session_id: owner.session_id,
    window_generation: owner.window_generation,
  };
}

/** Copies one process-local attachment display record without adding durable path fields.
 * 复制一条进程内附件展示记录，且不增加持久化路径字段。 */
function copyAttachment(
  attachment: UserInputOptimisticAttachmentDisplayV1,
): UserInputOptimisticAttachmentDisplayV1 {
  return {
    display_id: attachment.display_id,
    kind: attachment.kind,
    file_name: attachment.file_name,
    mime_type: attachment.mime_type,
  };
}
