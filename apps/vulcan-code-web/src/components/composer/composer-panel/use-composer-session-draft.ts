import React from "react";
import { toast } from "sonner";
import {
  useComposerAttachments,
  type UseComposerAttachmentsArgs,
  type UseComposerAttachmentsState,
} from "@/components/composer/composer-attachments";
import {
  composerSessionDraftKey,
  readComposerSessionMemoryDraft,
  writeComposerSessionMemoryDraft,
  type ComposerSessionMemoryDraft,
} from "@/components/composer/composer-panel/helpers";
import { useI18n } from "@/i18n";
import { SessionDraftWriteCoordinatorV1 } from "@/service/conversation-session-draft-write-coordinator-v1";
import { sessionDraftClientV1 } from "@/service/conversation-session-drafts-v1";
import type { ComposerAttachmentSummary } from "@/types";

/** One exact owner of the composer state currently rendered by the hook.
 * Hook 当前渲染的输入区状态所对应的精确 owner。
 */
interface ComposerDraftOwnerV1 {
  /** Stable UI slot used by submission rollback.
   * 提交回滚使用的稳定界面槽位。
   */
  draftSlotKey: string;
  /** Materialized session identity, or null for the new-session browser draft.
   * 已物化会话标识；新建会话的浏览器草稿为 null。
   */
  sessionId: string | null;
}

/** One complete draft state expected from a programmatic owner transition.
 * owner 程序化切换后预期出现的一份完整草稿状态。
 */
interface ExpectedComposerDraftStateV1 {
  /** Exact owner slot receiving the state.
   * 接收状态的精确 owner 槽位。
   */
  draftSlotKey: string;
  /** Expected raw user text.
   * 预期的原始用户文本。
   */
  input: string;
  /** Expected ordered attachment projection.
   * 预期的有序附件投影。
   */
  attachments: ComposerAttachmentSummary[];
}

/** UseComposerSessionDraftOptions contains attachment staging dependencies for one draft owner.
 * UseComposerSessionDraftOptions 包含单个草稿 owner 的附件暂存依赖。
 */
export interface UseComposerSessionDraftOptions extends Omit<UseComposerAttachmentsArgs, "initialAttachments"> {}

/** ComposerDraftSubmissionSnapshot captures the exact owner and content consumed by one send attempt.
 * ComposerDraftSubmissionSnapshot 捕获一次发送尝试消费的精确 owner 与内容。
 */
export interface ComposerDraftSubmissionSnapshot {
  /** Draft slot that owned the submission when it started.
   * 发送开始时拥有该提交的草稿槽位。
   */
  draftSlotKey: string;
  /** Materialized session that owned the draft, or null for a new session.
   * 拥有该草稿的已物化会话；新建会话为 null。
   */
  sessionId: string | null;
  /** Exact unsent input consumed by the submission.
   * 本次提交消费的精确未发送文本。
   */
  input: string;
  /** Exact ordered attachments consumed by the submission.
   * 本次提交消费的精确有序附件。
   */
  attachments: ComposerAttachmentSummary[];
}

/** Signals that preflight outlived its exact draft owner or content; no draft was consumed.
 * 表示预检已超出精确草稿所有者或内容的有效期；没有消费任何草稿。
 */
export class ComposerDraftChangedBeforeSubmissionError extends Error {
  /** Creates a typed local cancellation without treating it as a persistence failure.
   * 创建类型明确的本地取消原因，避免误报持久化失败；无参数，返回错误实例。
   */
  constructor() {
    super("The composer draft changed before submission and was preserved.");
    this.name = "ComposerDraftChangedBeforeSubmissionError";
  }
}

/** ComposerQueuedDraftReplacementToken binds one queue edit to the owner that initiated it.
 * ComposerQueuedDraftReplacementToken 把一次队列编辑绑定到发起它的 owner。
 */
export interface ComposerQueuedDraftReplacementToken {
  /** Exact owner captured before the queue-to-draft RPC begins.
   * 队列转草稿 RPC 开始前捕获的精确 owner。
   */
  owner: ComposerDraftOwnerV1;
  /** Monotonic operation generation rejecting overlapping replacement completion.
   * 拒绝重叠替换完成的单调操作代际。
   */
  generation: number;
}

/** UseComposerSessionDraftState exposes one coherent composer-draft transaction boundary.
 * UseComposerSessionDraftState 暴露一个完整一致的输入区草稿事务边界。
 */
export interface UseComposerSessionDraftState extends Omit<UseComposerAttachmentsState, "setAttachments"> {
  /** Stable key identifying the active conversation or new-conversation draft slot.
   * 标识活动会话或新建会话草稿槽位的稳定键。
   */
  draftSlotKey: string;
  /** Current unsent composer text owned by the active draft slot.
   * 当前草稿槽持有的未发送输入区文本。
   */
  draftInput: string;
  /** Updates unsent text while preserving the exact active owner.
   * 在保持精确活动 owner 的同时更新未发送文本。
   */
  setDraftInput: React.Dispatch<React.SetStateAction<string>>;
  /** Flushes and captures the exact durable draft before clearing its UI projection.
   * 在清空界面投影前可靠刷新并捕获精确草稿。
   */
  clearForSubmission: () => Promise<ComposerDraftSubmissionSnapshot>;
  /** Restores a failed submission while preserving newer user edits.
   * 恢复失败提交，同时保留较新的用户编辑。
   */
  restoreFailedSubmission: (snapshot: ComposerDraftSubmissionSnapshot) => void;
  /** Restores missing generated text to one explicit draft slot.
   * 将缺失的生成文本恢复到一个显式草稿槽位。
   */
  restoreInputToSlot: (draftSlotKey: string, input: string) => void;
  /** Replaces the complete active draft from an explicit queued-input edit action.
   * 通过显式排队输入编辑操作替换当前完整草稿。
   */
  replaceDraft: (input: string, attachments: ComposerAttachmentSummary[]) => void;
  /** Flushes the current owner and returns its identity before a queued-input replacement RPC.
   * 在队列输入替换 RPC 前刷新当前 owner 并返回其身份。
   */
  prepareQueuedDraftReplacement: () => Promise<ComposerQueuedDraftReplacementToken>;
  /** Applies a committed queued-input replacement to its captured owner in exact order.
   * 按精确顺序把已提交的队列输入替换应用到其捕获 owner。
   */
  completeQueuedDraftReplacement: (
    token: ComposerQueuedDraftReplacementToken,
    input: string,
    attachments: ComposerAttachmentSummary[],
  ) => boolean;
  /** Reloads the authoritative owner after an ambiguous queue-edit acknowledgement.
   * 在队列编辑确认不明确后重新加载权威 owner。
   */
  recoverQueuedDraftReplacement: (
    token: ComposerQueuedDraftReplacementToken,
  ) => Promise<void>;
}

/** Process-wide serialized writer preserving one ordering authority per materialized session.
 * 进程级串行 writer，为每个已物化会话保留唯一写入顺序权威。
 */
const sessionDraftWriteCoordinatorV1 = new SessionDraftWriteCoordinatorV1(sessionDraftClientV1);

/** Creates a detached attachment array for refs, state, and transport ownership.
 * 为 ref、状态与传输所有权创建脱离引用的附件数组。
 */
function cloneAttachments(
  attachments: readonly ComposerAttachmentSummary[],
): ComposerAttachmentSummary[] {
  return attachments.map((attachment) => ({ ...attachment }));
}

/** Compares one complete rendered draft with a programmatic transition target.
 * 比较一份完整渲染草稿与程序化切换目标。
 */
function draftStateMatches(
  expected: ExpectedComposerDraftStateV1,
  input: string,
  attachments: readonly ComposerAttachmentSummary[],
): boolean {
  return expected.input === input
    && JSON.stringify(expected.attachments) === JSON.stringify(attachments);
}

/** Derives a materialized session only from the canonical slot format emitted by this module.
 * 仅从本模块生成的规范槽位格式推导已物化会话。
 */
function materializedSessionIdFromDraftSlotKey(draftSlotKey: string): string | null {
  const prefix = "conversation:";
  if (!draftSlotKey.startsWith(prefix)) {
    return null;
  }
  const sessionId = draftSlotKey.slice(prefix.length);
  if (sessionId.length === 0 || sessionId.trim() !== sessionId) {
    throw new Error("Materialized composer draft slot contains an invalid session id.");
  }
  return sessionId;
}

/** Persists one owner without redirecting an inactive slot to the current selection.
 * 持久化一个 owner，且不会把非活动槽位重定向到当前选择。
 */
function persistDraftOwner(
  owner: ComposerDraftOwnerV1,
  input: string,
  attachments: readonly ComposerAttachmentSummary[],
): void {
  if (owner.sessionId) {
    sessionDraftWriteCoordinatorV1.enqueue(owner.sessionId, input, attachments);
    return;
  }
  writeComposerSessionMemoryDraft(owner.draftSlotKey, input, cloneAttachments(attachments));
}

/** Restores one inactive materialized owner by reading its latest durable state first.
 * 先读取最新可靠状态，再恢复一个非活动的已物化 owner。
 */
async function restoreInactiveMaterializedDraft(
  sessionId: string,
  fallbackInput: string,
  fallbackAttachments: readonly ComposerAttachmentSummary[],
): Promise<void> {
  const current = await sessionDraftClientV1.read(sessionId);
  const restoredInput = current.text || fallbackInput;
  const restoredAttachments = current.attachments.length
    ? current.attachments
    : cloneAttachments(fallbackAttachments);
  sessionDraftWriteCoordinatorV1.enqueue(
    sessionId,
    restoredInput,
    restoredAttachments,
  );
  await sessionDraftWriteCoordinatorV1.flush(sessionId);
}

/** Owns draft hydration, persistence, attachment state, and submission rollback semantics.
 * 管理草稿恢复、持久化、附件状态与提交回滚语义。
 */
// Parameters: options binds one conversation/project draft and persistence callbacks; returns draft handlers and submission capture/rollback operations.
// 参数：options 绑定一份会话或项目草稿及持久化回调；返回草稿处理器和提交捕获、回滚操作。
export function useComposerSessionDraft(
  options: UseComposerSessionDraftOptions,
): UseComposerSessionDraftState {
  const { t } = useI18n();
  const draftKey = React.useMemo(
    () => composerSessionDraftKey(options.projectId, options.conversationId),
    [options.conversationId, options.projectId],
  );
  const initialDraftRef = React.useRef<ComposerSessionMemoryDraft | null>(null);
  if (initialDraftRef.current === null) {
    initialDraftRef.current = options.conversationId
      ? { input: "", attachments: [] }
      : readComposerSessionMemoryDraft(draftKey);
  }
  const initialDraft = initialDraftRef.current;
  const activeOwnerRef = React.useRef<ComposerDraftOwnerV1>({
    draftSlotKey: draftKey,
    sessionId: options.conversationId,
  });
  const ownerInitializedRef = React.useRef(false);
  const hydrationGenerationRef = React.useRef(0);
  const hydratingOwnerKeyRef = React.useRef<string | null>(null);
  const ownerApplyBlockRef = React.useRef<ExpectedComposerDraftStateV1 | null>(null);
  const suppressedUiStateRef = React.useRef<ExpectedComposerDraftStateV1 | null>(null);
  const queuedDraftReplacementGenerationRef = React.useRef(0);
  const inputRef = React.useRef(initialDraft.input);
  const attachmentsRef = React.useRef<ComposerAttachmentSummary[]>(initialDraft.attachments);
  const [draftInput, setDraftInput] = React.useState(initialDraft.input);
  const attachmentState = useComposerAttachments({
    initialAttachments: initialDraft.attachments,
    attachmentCapabilities: options.attachmentCapabilities,
    projectId: options.projectId,
    conversationId: options.conversationId,
    onStageAttachmentBytes: options.onStageAttachmentBytes,
  });
  const {
    attachments,
    setAttachments,
    invalidatePendingAttachmentStaging,
    ...attachmentControls
  } = attachmentState;

  React.useLayoutEffect(() => {
    inputRef.current = draftInput;
    attachmentsRef.current = attachments;
  }, [attachments, draftInput]);

  React.useLayoutEffect(() => {
    const previousOwner = activeOwnerRef.current;
    if (
      ownerInitializedRef.current
      && previousOwner.draftSlotKey === draftKey
      && previousOwner.sessionId === options.conversationId
    ) {
      return;
    }
    // An unresolved read must never save the temporary empty UI over its authoritative draft.
    // 未完成的读取绝不能把临时空界面保存并覆盖权威草稿。
    if (ownerInitializedRef.current && hydratingOwnerKeyRef.current !== previousOwner.draftSlotKey) {
      persistDraftOwner(previousOwner, inputRef.current, attachmentsRef.current);
    }

    const nextOwner: ComposerDraftOwnerV1 = {
      draftSlotKey: draftKey,
      sessionId: options.conversationId,
    };
    invalidatePendingAttachmentStaging();
    activeOwnerRef.current = nextOwner;
    ownerInitializedRef.current = true;
    suppressedUiStateRef.current = null;
    const generation = hydrationGenerationRef.current + 1;
    hydrationGenerationRef.current = generation;

    if (!nextOwner.sessionId) {
      hydratingOwnerKeyRef.current = null;
      const restoredDraft = readComposerSessionMemoryDraft(nextOwner.draftSlotKey);
      const expected: ExpectedComposerDraftStateV1 = {
        draftSlotKey: nextOwner.draftSlotKey,
        input: restoredDraft.input,
        attachments: cloneAttachments(restoredDraft.attachments),
      };
      ownerApplyBlockRef.current = expected;
      inputRef.current = expected.input;
      attachmentsRef.current = expected.attachments;
      setDraftInput(expected.input);
      setAttachments(expected.attachments);
      return;
    }

    hydratingOwnerKeyRef.current = nextOwner.draftSlotKey;
    ownerApplyBlockRef.current = {
      draftSlotKey: nextOwner.draftSlotKey,
      input: "",
      attachments: [],
    };
    inputRef.current = "";
    attachmentsRef.current = [];
    setDraftInput("");
    setAttachments([]);
    // The earlier new-draft branch returned, so this is an exact materialized session identity.
    // 前面的新草稿分支已经返回，因此这里是精确的已物化会话身份。
    const hydratedSessionId = nextOwner.sessionId;
    // Retry the same owner-bound read; preserve edits made while the draft was unavailable.
    // 重试同一 owner 的读取；保留草稿不可用期间输入的新内容。
    const hydrate = () => {
      if (hydrationGenerationRef.current !== generation || activeOwnerRef.current.sessionId !== nextOwner.sessionId) return;
      void sessionDraftClientV1.read(hydratedSessionId).then((restoredDraft) => {
        const currentOwner = activeOwnerRef.current;
        if (
          hydrationGenerationRef.current !== generation
          || currentOwner.draftSlotKey !== nextOwner.draftSlotKey
          || currentOwner.sessionId !== nextOwner.sessionId
        ) {
          return;
        }
        const expected: ExpectedComposerDraftStateV1 = {
          draftSlotKey: nextOwner.draftSlotKey,
          // Non-empty local values can only be new user edits after the initial hydration reset.
          // 初次加载清空后，非空本地值只可能是用户新输入的内容。
          input: inputRef.current || restoredDraft.text,
          attachments: cloneAttachments(attachmentsRef.current.length ? attachmentsRef.current : restoredDraft.attachments),
        };
        hydratingOwnerKeyRef.current = null;
        ownerApplyBlockRef.current = expected;
        inputRef.current = expected.input;
        attachmentsRef.current = expected.attachments;
        setDraftInput(expected.input);
        setAttachments(expected.attachments);
      }).catch((reason: unknown) => {
        const currentOwner = activeOwnerRef.current;
        if (
          hydrationGenerationRef.current === generation
          && currentOwner.draftSlotKey === nextOwner.draftSlotKey
          && currentOwner.sessionId === nextOwner.sessionId
        ) {
          toast.error(t("composer.error.draftLoadFailed"), {
            description: reason instanceof Error ? reason.message : String(reason),
            action: { label: t("composer.draft.retry"), onClick: hydrate },
          });
        }
      });
    };
    hydrate();
  }, [draftKey, invalidatePendingAttachmentStaging, options.conversationId, setAttachments, t]);

  React.useEffect(() => {
    if (!ownerInitializedRef.current) {
      return;
    }
    const owner = activeOwnerRef.current;
    if (hydratingOwnerKeyRef.current === owner.draftSlotKey) {
      return;
    }
    const ownerApplyBlock = ownerApplyBlockRef.current;
    if (ownerApplyBlock?.draftSlotKey === owner.draftSlotKey) {
      if (draftStateMatches(ownerApplyBlock, draftInput, attachments)) {
        ownerApplyBlockRef.current = null;
      }
      return;
    }
    const suppressedUiState = suppressedUiStateRef.current;
    if (suppressedUiState?.draftSlotKey === owner.draftSlotKey) {
      suppressedUiStateRef.current = null;
      if (draftStateMatches(suppressedUiState, draftInput, attachments)) {
        return;
      }
    }
    persistDraftOwner(owner, draftInput, attachments);
  }, [attachments, draftInput]);

  React.useEffect(() => () => {
    const owner = activeOwnerRef.current;
    if (ownerInitializedRef.current && ownerApplyBlockRef.current === null) {
      persistDraftOwner(owner, inputRef.current, attachmentsRef.current);
    }
  }, []);

  const restoreFailedSubmission = React.useCallback((snapshot: ComposerDraftSubmissionSnapshot) => {
    const currentOwner = activeOwnerRef.current;
    if (currentOwner.draftSlotKey !== snapshot.draftSlotKey) {
      if (snapshot.sessionId) {
        void restoreInactiveMaterializedDraft(
          snapshot.sessionId,
          snapshot.input,
          snapshot.attachments,
        ).catch((reason: unknown) => toast.error(t("composer.error.draftPersistFailed"), {
          description: reason instanceof Error ? reason.message : String(reason),
        }));
        return;
      }
      const inactiveDraft = readComposerSessionMemoryDraft(snapshot.draftSlotKey);
      writeComposerSessionMemoryDraft(
        snapshot.draftSlotKey,
        inactiveDraft.input || snapshot.input,
        inactiveDraft.attachments.length ? inactiveDraft.attachments : snapshot.attachments,
      );
      return;
    }

    const restoredInput = inputRef.current || snapshot.input;
    const restoredAttachments = attachmentsRef.current.length
      ? attachmentsRef.current
      : cloneAttachments(snapshot.attachments);
    inputRef.current = restoredInput;
    attachmentsRef.current = restoredAttachments;
    suppressedUiStateRef.current = null;
    persistDraftOwner(currentOwner, restoredInput, restoredAttachments);
    setDraftInput(restoredInput);
    setAttachments(restoredAttachments);
  }, [setAttachments, t]);

  const clearForSubmission = React.useCallback(async () => {
    // Reject retained callbacks before any write: the active ref can belong to a later conversation.
    // 在任何写入前拒绝过期回调：活动 ref 可能已经属于后来切换的会话。
    const owner = activeOwnerRef.current;
    if (
      owner.draftSlotKey !== draftKey
      || owner.sessionId !== options.conversationId
      || !draftStateMatches({ draftSlotKey: draftKey, input: draftInput, attachments }, inputRef.current, attachmentsRef.current)
    ) {
      throw new ComposerDraftChangedBeforeSubmissionError();
    }
    if (attachmentState.hasPendingAttachmentStaging()) {
      throw new Error(t("composer.attachment.stagingPending"));
    }
    if (
      hydratingOwnerKeyRef.current === owner.draftSlotKey
      || ownerApplyBlockRef.current?.draftSlotKey === owner.draftSlotKey
    ) {
      throw new Error("Materialized session draft has not completed authoritative hydration.");
    }
    const snapshot: ComposerDraftSubmissionSnapshot = {
      draftSlotKey: owner.draftSlotKey,
      sessionId: owner.sessionId,
      input: draftInput,
      attachments: cloneAttachments(attachments),
    };

    if (owner.sessionId) {
      sessionDraftWriteCoordinatorV1.enqueue(
        owner.sessionId,
        snapshot.input,
        snapshot.attachments,
      );
    } else {
      writeComposerSessionMemoryDraft(owner.draftSlotKey, snapshot.input, snapshot.attachments);
    }

    const clearedState: ExpectedComposerDraftStateV1 = {
      draftSlotKey: owner.draftSlotKey,
      input: "",
      attachments: [],
    };
    suppressedUiStateRef.current = clearedState;
    inputRef.current = "";
    attachmentsRef.current = [];
    setDraftInput("");
    setAttachments([]);

    if (owner.sessionId) {
      try {
        await sessionDraftWriteCoordinatorV1.flush(owner.sessionId);
      } catch (error) {
        restoreFailedSubmission(snapshot);
        throw error;
      }
    }
    return snapshot;
  }, [attachmentState.hasPendingAttachmentStaging, attachments, draftInput, draftKey, options.conversationId, restoreFailedSubmission, setAttachments, t]);

  const restoreInputToSlot = React.useCallback((targetDraftSlotKey: string, input: string) => {
    const currentOwner = activeOwnerRef.current;
    if (currentOwner.draftSlotKey !== targetDraftSlotKey) {
      const targetSessionId = materializedSessionIdFromDraftSlotKey(targetDraftSlotKey);
      if (targetSessionId) {
        void restoreInactiveMaterializedDraft(targetSessionId, input, [])
          .catch((reason: unknown) => toast.error(t("composer.error.draftPersistFailed"), {
            description: reason instanceof Error ? reason.message : String(reason),
          }));
        return;
      }
      const inactiveDraft = readComposerSessionMemoryDraft(targetDraftSlotKey);
      writeComposerSessionMemoryDraft(
        targetDraftSlotKey,
        inactiveDraft.input || input,
        inactiveDraft.attachments,
      );
      return;
    }
    if (inputRef.current) {
      return;
    }
    inputRef.current = input;
    persistDraftOwner(currentOwner, input, attachmentsRef.current);
    setDraftInput(input);
  }, [t]);

  const replaceDraft = React.useCallback((
    input: string,
    nextAttachments: ComposerAttachmentSummary[],
  ) => {
    const owner = activeOwnerRef.current;
    const detachedAttachments = cloneAttachments(nextAttachments);
    invalidatePendingAttachmentStaging();
    inputRef.current = input;
    attachmentsRef.current = detachedAttachments;
    persistDraftOwner(owner, input, detachedAttachments);
    suppressedUiStateRef.current = {
      draftSlotKey: owner.draftSlotKey,
      input,
      attachments: detachedAttachments,
    };
    setDraftInput(input);
    setAttachments(detachedAttachments);
  }, [invalidatePendingAttachmentStaging, setAttachments]);

  /** Flushes every earlier write before the durable queue-to-draft mutation is admitted.
   * 在持久队列转草稿变更准入前刷新全部较早写入。
   */
  const prepareQueuedDraftReplacement = React.useCallback(async (): Promise<ComposerQueuedDraftReplacementToken> => {
    const owner = { ...activeOwnerRef.current };
    if (
      hydratingOwnerKeyRef.current === owner.draftSlotKey
      || ownerApplyBlockRef.current?.draftSlotKey === owner.draftSlotKey
    ) {
      throw new Error("Materialized session draft has not completed authoritative hydration.");
    }
    invalidatePendingAttachmentStaging();
    persistDraftOwner(owner, inputRef.current, attachmentsRef.current);
    if (owner.sessionId) {
      await sessionDraftWriteCoordinatorV1.flush(owner.sessionId);
    }
    const generation = queuedDraftReplacementGenerationRef.current + 1;
    queuedDraftReplacementGenerationRef.current = generation;
    return { owner, generation };
  }, [invalidatePendingAttachmentStaging]);

  /** Projects one committed replacement without redirecting it when the user changed sessions mid-flight.
   * 投影一份已提交替换；用户在请求期间切换会话时不会把它重定向到新会话。
   */
  const completeQueuedDraftReplacement = React.useCallback((
    token: ComposerQueuedDraftReplacementToken,
    input: string,
    nextAttachments: ComposerAttachmentSummary[],
  ) => {
    if (token.generation !== queuedDraftReplacementGenerationRef.current) {
      return false;
    }
    const detachedAttachments = cloneAttachments(nextAttachments);
    const currentOwner = activeOwnerRef.current;
    if (
      currentOwner.draftSlotKey !== token.owner.draftSlotKey
      || currentOwner.sessionId !== token.owner.sessionId
    ) {
      persistDraftOwner(token.owner, input, detachedAttachments);
      return false;
    }
    replaceDraft(input, detachedAttachments);
    return true;
  }, [replaceDraft]);

  /** Reconciles an uncertain RPC acknowledgement from the durable session draft without guessing commit state.
   * 从持久会话草稿协调不确定的RPC确认，绝不猜测提交状态。
   */
  const recoverQueuedDraftReplacement = React.useCallback(async (
    token: ComposerQueuedDraftReplacementToken,
  ) => {
    if (token.generation !== queuedDraftReplacementGenerationRef.current) {
      return;
    }
    if (!token.owner.sessionId) {
      throw new Error("Queued prompt recovery requires a materialized session owner.");
    }
    const restoredDraft = await sessionDraftClientV1.read(token.owner.sessionId);
    const currentOwner = activeOwnerRef.current;
    if (
      currentOwner.draftSlotKey !== token.owner.draftSlotKey
      || currentOwner.sessionId !== token.owner.sessionId
    ) {
      return;
    }
    replaceDraft(restoredDraft.text, restoredDraft.attachments);
  }, [replaceDraft]);

  return {
    ...attachmentControls,
    draftSlotKey: draftKey,
    attachments,
    invalidatePendingAttachmentStaging,
    draftInput,
    setDraftInput,
    clearForSubmission,
    restoreFailedSubmission,
    restoreInputToSlot,
    replaceDraft,
    prepareQueuedDraftReplacement,
    completeQueuedDraftReplacement,
    recoverQueuedDraftReplacement,
  };
}
