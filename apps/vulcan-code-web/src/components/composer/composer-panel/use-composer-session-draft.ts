import React from "react";
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
import type { ComposerAttachmentSummary } from "@/types";

// UseComposerSessionDraftOptions contains attachment staging dependencies for one draft slot.
// UseComposerSessionDraftOptions 包含单个草稿槽的附件暂存依赖。
export interface UseComposerSessionDraftOptions extends Omit<UseComposerAttachmentsArgs, "initialAttachments"> {}

// ComposerDraftSubmissionSnapshot captures the exact draft slot content consumed by one send attempt.
// ComposerDraftSubmissionSnapshot 捕获一次发送尝试消费的精确草稿槽内容。
export interface ComposerDraftSubmissionSnapshot {
  // Draft slot that owned the submission when it started.
  // 发送开始时拥有该提交的草稿槽。
  draftSlotKey: string;
  // Exact unsent input consumed by the submission.
  // 本次提交消费的精确未发送文本。
  input: string;
  // Exact staged attachments consumed by the submission.
  // 本次提交消费的精确已暂存附件。
  attachments: ComposerAttachmentSummary[];
}

// UseComposerSessionDraftState exposes one coherent composer draft transaction boundary.
// UseComposerSessionDraftState 暴露一个完整一致的输入区草稿事务边界。
export interface UseComposerSessionDraftState extends Omit<UseComposerAttachmentsState, "setAttachments"> {
  // Stable key identifying the currently active conversation or new-conversation draft slot.
  // 标识当前会话或新建会话草稿槽的稳定键。
  draftSlotKey: string;
  // Current unsent composer text owned by the active draft slot.
  // 当前草稿槽持有的未发送输入区文本。
  draftInput: string;
  // Updates unsent text while keeping the active draft slot synchronized.
  // 更新未发送文本，同时保持当前草稿槽同步。
  setDraftInput: React.Dispatch<React.SetStateAction<string>>;
  // Captures and clears submitted content before an async send begins.
  // 在异步发送开始前捕获并清理已提交内容。
  clearForSubmission: () => ComposerDraftSubmissionSnapshot;
  // Restores a failed submission to its owning slot while preserving newer user edits.
  // 将失败提交恢复到所属槽，同时保留用户较新的编辑。
  restoreFailedSubmission: (snapshot: ComposerDraftSubmissionSnapshot) => void;
  // Restores missing text to one explicit draft slot without contaminating another active slot.
  // 将缺失文本恢复到一个显式草稿槽，且不污染另一个活动槽。
  restoreInputToSlot: (draftSlotKey: string, input: string) => void;
  // Replaces the complete active draft from an explicit queued-prompt edit action.
  // 通过显式的排队提示词编辑操作替换当前完整草稿。
  replaceDraft: (input: string, attachments: ComposerAttachmentSummary[]) => void;
}

// useComposerSessionDraft owns draft-slot hydration, persistence, attachments, and send rollback semantics.
// useComposerSessionDraft 管理草稿槽恢复、持久化、附件以及发送回滚语义。
// Parameters: options provides project-scoped attachment staging dependencies.
// 参数：options 提供项目范围内的附件暂存依赖。
// Returns: synchronized text and attachment state plus explicit submission transaction operations.
// 返回值：同步的文本与附件状态，以及显式的提交事务操作。
export function useComposerSessionDraft(options: UseComposerSessionDraftOptions): UseComposerSessionDraftState {
  // draftKey selects either the persisted conversation slot or the project's new-conversation slot.
  // draftKey 选择已持久化会话槽或项目的新建会话槽。
  const draftKey = React.useMemo(
    () => composerSessionDraftKey(options.projectId, options.conversationId),
    [options.conversationId, options.projectId],
  );
  // initialDraftRef captures the draft that seeds this component mount exactly once.
  // initialDraftRef 仅捕获一次用于初始化本次组件挂载的草稿。
  const initialDraftRef = React.useRef<ComposerSessionMemoryDraft | null>(null);
  if (initialDraftRef.current === null) {
    initialDraftRef.current = readComposerSessionMemoryDraft(draftKey);
  }
  // initialDraft is the non-null snapshot captured for initial text and attachment state.
  // initialDraft 是为初始文本与附件状态捕获的非空快照。
  const initialDraft = initialDraftRef.current;
  // activeDraftKeyRef tracks which draft slot currently owns local state.
  // activeDraftKeyRef 跟踪当前本地状态所属的草稿槽。
  const activeDraftKeyRef = React.useRef(draftKey);
  // inputRef mirrors draftInput for slot switches and async submission rollback.
  // inputRef 镜像 draftInput，供草稿槽切换与异步提交回滚使用。
  const inputRef = React.useRef(initialDraft.input);
  // attachmentsRef mirrors attachments for slot switches and async submission rollback.
  // attachmentsRef 镜像附件，供草稿槽切换与异步提交回滚使用。
  const attachmentsRef = React.useRef<ComposerAttachmentSummary[]>(initialDraft.attachments);
  // restoredRef records whether the active draft slot has completed hydration.
  // restoredRef 记录当前草稿槽是否已完成恢复。
  const restoredRef = React.useRef(false);
  // draftInput stores the current unsent composer text.
  // draftInput 保存当前尚未发送的输入区文本。
  const [draftInput, setDraftInput] = React.useState(initialDraft.input);
  // attachmentState owns attachment validation, staging, picker, paste, and drag-drop behavior.
  // attachmentState 管理附件校验、暂存、选择器、粘贴与拖拽行为。
  const attachmentState = useComposerAttachments({
    initialAttachments: initialDraft.attachments,
    attachmentCapabilities: options.attachmentCapabilities,
    projectId: options.projectId,
    conversationId: options.conversationId,
    onStageAttachmentBytes: options.onStageAttachmentBytes,
  });
  // Internal attachment state connects attachment ownership to the draft transaction while hiding its raw setter.
  // 内部附件状态将附件所有权连接到草稿事务，同时隐藏原始 setter。
  const { attachments, setAttachments, ...attachmentControls } = attachmentState;

  React.useLayoutEffect(() => {
    // previousKey identifies the slot whose latest state may need to be persisted before switching.
    // previousKey 标识切换前可能需要持久化最新状态的草稿槽。
    const previousKey = activeDraftKeyRef.current;
    if (restoredRef.current && previousKey === draftKey) {
      return;
    }
    if (restoredRef.current) {
      writeComposerSessionMemoryDraft(previousKey, inputRef.current, attachmentsRef.current);
    }
    // restoredDraft is the isolated snapshot that becomes the new active slot state.
    // restoredDraft 是即将成为新活动槽状态的隔离快照。
    const restoredDraft = readComposerSessionMemoryDraft(draftKey);
    activeDraftKeyRef.current = draftKey;
    inputRef.current = restoredDraft.input;
    attachmentsRef.current = restoredDraft.attachments;
    restoredRef.current = true;
    setDraftInput(restoredDraft.input);
    setAttachments(restoredDraft.attachments);
  }, [draftKey, setAttachments]);

  React.useEffect(() => {
    if (!restoredRef.current) {
      return;
    }
    inputRef.current = draftInput;
    writeComposerSessionMemoryDraft(activeDraftKeyRef.current, draftInput, attachmentsRef.current);
  }, [draftInput]);

  React.useEffect(() => {
    if (!restoredRef.current) {
      return;
    }
    attachmentsRef.current = attachments;
    writeComposerSessionMemoryDraft(activeDraftKeyRef.current, inputRef.current, attachments);
  }, [attachments]);

  // clearForSubmission captures ownership and atomically clears the submitted draft before awaiting the backend.
  // clearForSubmission 在等待后端前捕获归属并以事务方式清理已提交草稿。
  const clearForSubmission = React.useCallback(() => {
    // snapshot binds consumed content to the exact draft slot that owned it.
    // snapshot 将已消费内容绑定到拥有它的精确草稿槽。
    const snapshot: ComposerDraftSubmissionSnapshot = {
      draftSlotKey: activeDraftKeyRef.current,
      input: draftInput,
      attachments: attachments.map((attachment) => ({ ...attachment })),
    };
    inputRef.current = "";
    attachmentsRef.current = [];
    writeComposerSessionMemoryDraft(activeDraftKeyRef.current, "", []);
    setDraftInput("");
    setAttachments([]);
    return snapshot;
  }, [attachments, draftInput, setAttachments]);

  // restoreFailedSubmission merges a rejected submission into its original slot without contaminating the active slot.
  // restoreFailedSubmission 将被拒绝的提交合并回原始槽，且不污染当前活动槽。
  const restoreFailedSubmission = React.useCallback((snapshot: ComposerDraftSubmissionSnapshot) => {
    if (activeDraftKeyRef.current !== snapshot.draftSlotKey) {
      // inactiveDraft contains edits saved to the original slot after the submission started.
      // inactiveDraft 包含提交开始后保存到原始槽的编辑。
      const inactiveDraft = readComposerSessionMemoryDraft(snapshot.draftSlotKey);
      writeComposerSessionMemoryDraft(
        snapshot.draftSlotKey,
        inactiveDraft.input || snapshot.input,
        inactiveDraft.attachments.length ? inactiveDraft.attachments : snapshot.attachments,
      );
      return;
    }
    setDraftInput((current) => {
      // restoredInput preserves newer typed text and fills only an empty input.
      // restoredInput 保留较新的输入文本，仅在输入为空时回填。
      const restoredInput = current || snapshot.input;
      inputRef.current = restoredInput;
      writeComposerSessionMemoryDraft(activeDraftKeyRef.current, restoredInput, attachmentsRef.current);
      return restoredInput;
    });
    setAttachments((current) => {
      // restoredAttachments preserves newer staged files and fills only an empty attachment list.
      // restoredAttachments 保留较新的已暂存文件，仅在附件列表为空时回填。
      const restoredAttachments = current.length ? current : snapshot.attachments;
      attachmentsRef.current = restoredAttachments;
      writeComposerSessionMemoryDraft(activeDraftKeyRef.current, inputRef.current, restoredAttachments);
      return restoredAttachments;
    });
  }, [setAttachments]);

  // restoreInputToSlot restores a failed generated submission to the slot that owned it.
  // restoreInputToSlot 将失败的生成式提交恢复到拥有它的草稿槽。
  const restoreInputToSlot = React.useCallback((targetDraftSlotKey: string, input: string) => {
    if (activeDraftKeyRef.current !== targetDraftSlotKey) {
      // inactiveDraft preserves newer text and every staged attachment in the inactive slot.
      // inactiveDraft 保留非活动槽中的较新文本与全部已暂存附件。
      const inactiveDraft = readComposerSessionMemoryDraft(targetDraftSlotKey);
      writeComposerSessionMemoryDraft(
        targetDraftSlotKey,
        inactiveDraft.input || input,
        inactiveDraft.attachments,
      );
      return;
    }
    setDraftInput((current) => {
      // restoredInput fills only an empty active input.
      // restoredInput 仅在当前活动输入为空时回填。
      const restoredInput = current || input;
      inputRef.current = restoredInput;
      writeComposerSessionMemoryDraft(activeDraftKeyRef.current, restoredInput, attachmentsRef.current);
      return restoredInput;
    });
  }, []);

  // replaceDraft synchronously replaces both state dimensions and their persisted mirror.
  // replaceDraft 同步替换两类状态及其持久化镜像。
  const replaceDraft = React.useCallback((input: string, nextAttachments: ComposerAttachmentSummary[]) => {
    inputRef.current = input;
    attachmentsRef.current = nextAttachments;
    writeComposerSessionMemoryDraft(activeDraftKeyRef.current, input, nextAttachments);
    setDraftInput(input);
    setAttachments(nextAttachments);
  }, [setAttachments]);

  return {
    ...attachmentControls,
    draftSlotKey: draftKey,
    attachments,
    draftInput,
    setDraftInput,
    clearForSubmission,
    restoreFailedSubmission,
    restoreInputToSlot,
    replaceDraft,
  };
}
