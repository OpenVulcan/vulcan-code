import React from "react";
import type { AnswerMode } from "@/service/answer-mode";
import type { DedicatedToolAccess } from "@/service/conversation-event-window-client";

/** Session kind chosen for one draft; only drafts may switch.
 * 单个草稿选择的会话种类；只有草稿可以切换。 */
export type DraftSessionKind = "work" | "dedicated";

/** Ordered steps of the dedicated creation wizard: basic settings, the base prompt, then memory.
 * 专职创建向导的有序步骤：基础设置、基础提示词、记忆。 */
export const DEDICATED_WIZARD_STEPS = ["settings", "prompt", "memory"] as const;

/** Recommended answer mode of a new assistant: it answers by its own prompt, so interactive components rarely fit.
 * 新助手的推荐回答模式：助手按自身提示词作答，交互式组件通常并不适合。 */
export const DEDICATED_RECOMMENDED_ANSWER_MODE: AnswerMode = "standard";

/** Recommended output style of a new assistant, so only its own prompt shapes the answer; the wire id is the
 * service's `OUTPUT_STYLE_UNRESTRICTED_ID` (`vulcan-prompt-composer::output_style`).
 * 新助手的推荐输出风格，使回答只由其自身提示词塑造；线上标识即服务端
 * `OUTPUT_STYLE_UNRESTRICTED_ID`（`vulcan-prompt-composer::output_style`）。 */
export const DEDICATED_RECOMMENDED_OUTPUT_STYLE_ID = "unrestricted";

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
  /** Answer mode picked in the wizard; null shows and submits the recommended mode.
   * 向导中选择的回答模式；null 表示显示并提交推荐模式。 */
  answerMode: AnswerMode | null;
  /** Output style picked in the wizard; null shows and submits the recommended style.
   * 向导中选择的输出风格；null 表示显示并提交推荐风格。 */
  outputStyleId: string | null;
  /** Description the prompt generator drafts from; kept so the dialog reopens with it.
   * 提示词生成器据以起草的描述；保留以便弹窗重新打开时沿用。 */
  requirement: string;
  /** Base prompt typed by the user or written back from the generator.
   * 用户撰写或由生成器写回的基础提示词。 */
  prompt: string;
}

/** Initial state of a draft that has not been touched.
 * 尚未改动的草稿初始状态。 */
export const INITIAL_DEDICATED_DRAFT: DedicatedDraftState = {
  kind: "work",
  toolAccess: "none",
  step: "settings",
  knowledgeLibraryIds: [],
  answerMode: null,
  outputStyleId: null,
  requirement: "",
  prompt: "",
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
