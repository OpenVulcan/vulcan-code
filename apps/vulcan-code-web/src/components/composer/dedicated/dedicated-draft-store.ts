import React from "react";
import type { AnswerMode } from "@/service/answer-mode";
import type { DedicatedToolAccess } from "@/service/conversation-event-window-client";

/** Session kind chosen for one draft; only drafts may switch.
 * 单个草稿选择的会话种类；只有草稿可以切换。 */
export type DraftSessionKind = "work" | "dedicated";

/** Ordered steps of the dedicated creation wizard.
 * 专职创建向导的有序步骤。 */
export const DEDICATED_WIZARD_STEPS = [
  "knowledge",
  "answerMode",
  "outputStyle",
  "requirement",
  "prompt",
  "memory",
] as const;

/** One dedicated creation wizard step.
 * 单个专职创建向导步骤。 */
export type DedicatedWizardStep = typeof DEDICATED_WIZARD_STEPS[number];

/** Local-only choices of one draft, kept until it is submitted or discarded.
 * 单个草稿的仅本地选择，保留到提交或丢弃为止。 */
export interface DedicatedDraftState {
  kind: DraftSessionKind;
  toolAccess: DedicatedToolAccess;
  step: DedicatedWizardStep;
  /** Explicit knowledge libraries enabled at creation; empty disables the knowledge base.
   * 创建时启用的显式知识库；为空表示不使用知识库。 */
  knowledgeLibraryIds: string[];
  /** Explicit answer mode; null copies the global default.
   * 显式回答模式；null 表示复制全局默认值。 */
  answerMode: AnswerMode | null;
  /** Explicit output style; null follows the global selection.
   * 显式输出风格；null 表示跟随全局选择。 */
  outputStyleId: string | null;
  requirement: string;
  prompt: string;
  revisionRequest: string;
}

/** Initial state of a draft that has not been touched.
 * 尚未改动的草稿初始状态。 */
export const INITIAL_DEDICATED_DRAFT: DedicatedDraftState = {
  kind: "work",
  toolAccess: "none",
  step: "knowledge",
  knowledgeLibraryIds: [],
  answerMode: null,
  outputStyleId: null,
  requirement: "",
  prompt: "",
  revisionRequest: "",
};

// drafts stores per-draft choices so switching conversations does not lose wizard progress.
// drafts 按草稿保存选择，切换会话时不丢失向导进度。
const drafts = new Map<string, DedicatedDraftState>();
// listeners are notified after any draft entry changes.
// listeners 在任一草稿条目变化后收到通知。
const listeners = new Set<() => void>();

/** Replaces one draft entry; `null` discards it.
 * 替换单个草稿条目；`null` 表示丢弃。 */
export function writeDedicatedDraft(draftId: string, state: DedicatedDraftState | null): void {
  if (state === null) drafts.delete(draftId);
  else drafts.set(draftId, state);
  listeners.forEach((listener) => listener());
}

/** Subscribes to draft-store changes.
 * 订阅草稿存储变化。 */
function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Binds one draft's choices; a null id yields the initial state and ignores updates.
 * 绑定单个草稿的选择；id 为 null 时返回初始状态并忽略更新。
 *
 * @returns the current state and a functional updater scoped to the same draft.
 *          当前状态与作用于同一草稿的函数式更新器。 */
export function useDedicatedDraft(
  draftId: string | null,
): [DedicatedDraftState, (update: (current: DedicatedDraftState) => DedicatedDraftState) => void] {
  const read = React.useCallback(
    () => (draftId === null ? INITIAL_DEDICATED_DRAFT : drafts.get(draftId) ?? INITIAL_DEDICATED_DRAFT),
    [draftId],
  );
  const state = React.useSyncExternalStore(subscribe, read, read);
  const update = React.useCallback((next: (current: DedicatedDraftState) => DedicatedDraftState) => {
    if (draftId === null) return;
    writeDedicatedDraft(draftId, next(drafts.get(draftId) ?? INITIAL_DEDICATED_DRAFT));
  }, [draftId]);
  return [state, update];
}
