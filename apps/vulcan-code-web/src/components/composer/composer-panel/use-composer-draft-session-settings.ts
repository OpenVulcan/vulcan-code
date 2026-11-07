import React from "react";

import type {
  ComposerInitialGoalSnapshot,
  ComposerKnowledgeSelectionSnapshot,
  ComposerPromptSkillOverrides,
  ComposerProviderToolSettingsSnapshot,
  ComposerSessionSettingsSnapshot,
  ComposerVmmPolicySnapshot,
} from "@/components/composer/composer-model";
import type { AnswerMode } from "@/service/answer-mode";
import type { PermissionMode } from "@/service/permission-mode";
import type { SessionGoalSummary } from "@/types";

// DraftSessionSettingsEntry contains only overrides explicitly authored for one new-conversation slot.
// DraftSessionSettingsEntry 仅包含为一个新会话草稿槽显式编写的覆盖项。
interface DraftSessionSettingsEntry {
  // Explicit answer mode captured with all other first-turn settings.
  // 与其他首轮设置一同捕获的显式回答模式。
  answerMode?: AnswerMode;
  // Explicit permission mode captured with all other first-turn settings.
  // 与其他首轮设置一同捕获的显式权限模式。
  permissionMode?: PermissionMode;
  // Explicit output-style override for the new conversation slot.
  // 新会话草稿槽的显式输出风格覆盖。
  outputStyleId?: string;
  // Explicit VMM override pair.
  // 显式 VMM 覆盖对。
  vmmPolicy?: ComposerVmmPolicySnapshot;
  // Explicit complete provider-tool override.
  // 显式完整供应商工具覆盖。
  providerToolSettings?: ComposerProviderToolSettingsSnapshot;
  // Explicit knowledge selection.
  // 显式知识库选择。
  knowledgeSelection?: ComposerKnowledgeSelectionSnapshot;
  // Optional initial goal.
  // 可选初始目标。
  initialGoal?: ComposerInitialGoalSnapshot;
  // Session skill switches set by another surface together with the draft text.
  // 其他界面随草稿文本一并设置的会话技能开关。
  promptSkillOverrides?: ComposerPromptSkillOverrides;
}

// captureDraftSessionSettingsSnapshot returns only the exact unmaterialized slot override.
// captureDraftSessionSettingsSnapshot 仅返回精确未物化草稿槽的覆盖项。
// Parameters: entries owns isolated draft slots, scopeKey selects one slot, and conversationId identifies persisted conversations.
// 参数：entries 持有隔离草稿槽，scopeKey 选择一个槽，conversationId 标识已持久化会话。
// Returns: a detached settings snapshot for a draft, or null for persisted/empty slots.
// 返回值：草稿的分离设置快照；已持久化或空槽返回 null。
export function captureDraftSessionSettingsSnapshot(
  entries: Readonly<Record<string, DraftSessionSettingsEntry>>,
  scopeKey: string,
  conversationId: string | null,
): ComposerSessionSettingsSnapshot | null {
  if (conversationId) return null;
  const entry = entries[scopeKey] ?? {};
  return Object.keys(entry).length > 0 ? { ...entry } : null;
}

// clearDraftSessionSettingsEntry removes only one accepted new-conversation slot.
// clearDraftSessionSettingsEntry 仅移除一个已接受的新会话草稿槽。
// Parameters: entries owns isolated slots, scopeKey selects the consumed slot, and conversationId blocks persisted-session mutation.
// 参数：entries 持有隔离槽，scopeKey 选择已消费槽，conversationId 阻止修改持久化会话。
// Returns: the original map when no draft may be cleared, otherwise a map without the consumed slot.
// 返回值：无法清理草稿时返回原映射，否则返回移除已消费槽后的映射。
export function clearDraftSessionSettingsEntry(
  entries: Readonly<Record<string, DraftSessionSettingsEntry>>,
  scopeKey: string,
  conversationId: string | null,
): Record<string, DraftSessionSettingsEntry> {
  if (conversationId || !Object.hasOwn(entries, scopeKey)) return entries;
  const nextEntries = { ...entries };
  delete nextEntries[scopeKey];
  return nextEntries;
}

// ComposerDraftSessionSettingsController exposes draft-local state and an atomic submission capture.
// ComposerDraftSessionSettingsController 暴露草稿本地状态与原子提交捕获。
export interface ComposerDraftSessionSettingsController {
  // Explicit draft answer mode, or null while the draft follows the global default.
  // 显式草稿回答模式；草稿跟随全局默认值时为空。
  answerMode: AnswerMode | null;
  // Capture a new draft answer mode synchronously for the next submission.
  // 同步捕获新草稿回答模式供下一次提交使用。
  setAnswerMode: (mode: AnswerMode) => void;
  // Explicit draft permission mode, or null while the draft keeps the session default.
  // 显式草稿权限模式；草稿保留会话默认值时为空。
  permissionMode: PermissionMode | null;
  // Capture a new draft permission mode synchronously for the next submission.
  // 同步捕获新草稿权限模式供下一次提交使用。
  setPermissionMode: (mode: PermissionMode) => void;
  // Output-style override shown for the current draft, or null when inheriting global settings.
  // 当前草稿显示的输出风格覆盖；继承全局设置时为空。
  outputStyleId: string | null;
  // VMM policy shown for the current draft.
  // 当前草稿展示的 VMM 策略。
  vmmPolicy: ComposerVmmPolicySnapshot | null;
  // Provider-tool override shown for the current draft.
  // 当前草稿展示的供应商工具覆盖。
  providerToolSettings: DraftSessionSettingsEntry["providerToolSettings"];
  // Knowledge selection shown for the current draft.
  // 当前草稿展示的知识库选择。
  knowledgeSelection: ComposerKnowledgeSelectionSnapshot | null;
  // Synthetic goal summary shown before materialization.
  // 物化前展示的合成目标摘要。
  initialGoalSummary: SessionGoalSummary | null;
  // Updates one VMM policy without losing the other tri-state value.
  // 更新单项 VMM 策略且不丢失另一项三态值。
  setVmmPolicy: (policy: string, mode: string) => void;
  // Sets one explicit output-style override for the new conversation slot.
  // 为新会话草稿槽设置一个显式输出风格覆盖。
  setOutputStyle: (styleId: string) => void;
  // Clears the draft output-style override so the new conversation inherits the global choice.
  // 清除草稿输出风格覆盖，使新会话继承全局选择。
  clearOutputStyle: () => void;
  // Replaces the complete provider-tool override.
  // 替换完整供应商工具覆盖。
  setProviderToolSettings: (
    settings: NonNullable<DraftSessionSettingsEntry["providerToolSettings"]>,
  ) => void;
  // Replaces the draft knowledge selection.
  // 替换草稿知识库选择。
  setKnowledgeSelection: (selection: ComposerKnowledgeSelectionSnapshot) => void;
  // Creates or replaces the draft initial goal.
  // 创建或替换草稿初始目标。
  setInitialGoal: (objective: string, tokenBudget: number | null) => void;
  // Clears the draft initial goal.
  // 清理草稿初始目标。
  clearInitialGoal: () => void;
  // Replaces the draft's session skill switches.
  // 替换草稿的会话技能开关。
  setPromptSkillOverrides: (overrides: ComposerPromptSkillOverrides) => void;
  // Captures the latest settings synchronously at submission time.
  // 在提交时同步捕获最新设置。
  captureSubmissionSettings: () => ComposerSessionSettingsSnapshot | null;
  // Clears an accepted snapshot only when its immutable source entry is still current.
  // 仅在不可变来源条目仍为当前版本时清理已接受的快照。
  clearSubmissionSettings: (snapshot: ComposerSessionSettingsSnapshot | null) => void;
}

// useComposerDraftSessionSettings isolates every new-conversation slot from persisted sessions and sibling drafts.
// useComposerDraftSessionSettings 将每个新会话草稿槽与持久会话及其他草稿隔离。
export function useComposerDraftSessionSettings(
  scopeKey: string,
  conversationId: string | null,
): ComposerDraftSessionSettingsController {
  // entries stores explicit overrides by stable draft slot.
  // entries 按稳定草稿槽保存显式覆盖。
  const [entries, setEntries] = React.useState<Record<string, DraftSessionSettingsEntry>>({});
  // entriesRef provides synchronous capture after goal-save and before React commits a render.
  // entriesRef 在目标保存后、React 提交渲染前提供同步捕获。
  const entriesRef = React.useRef(entries);
  entriesRef.current = entries;
  // capturedEntriesRef binds detached delivery snapshots to the immutable entries produced by updateCurrent.
  // capturedEntriesRef 将分离的传输快照绑定到 updateCurrent 生成的不可变条目。
  // Identity preserves later edits, including reselecting the same value, without retaining completed request snapshots.
  // 身份比较保护后续编辑，包括重新选择同值；弱引用不会保留已结束请求的快照。
  const capturedEntriesRef = React.useRef(new WeakMap<ComposerSessionSettingsSnapshot, DraftSessionSettingsEntry>());

  // updateCurrent commits one immutable entry and synchronously updates the capture ref.
  // updateCurrent 提交一个不可变条目并同步更新捕获引用。
  const updateCurrent = React.useCallback((
    updater: (current: DraftSessionSettingsEntry) => DraftSessionSettingsEntry,
  ) => {
    if (conversationId) return;
    // The ref is the synchronous submission authority because React may defer state updaters.
    // ref 是同步提交权威来源，因为 React 可能延迟执行状态 updater。
    const currentEntries = entriesRef.current;
    const nextEntries = {
      ...currentEntries,
      [scopeKey]: updater(currentEntries[scopeKey] ?? {}),
    };
    entriesRef.current = nextEntries;
    setEntries(nextEntries);
  }, [conversationId, scopeKey]);

  const current = conversationId ? {} : entries[scopeKey] ?? {};
  // setAnswerMode shares the exact draft capture and acknowledgement ownership.
  // setAnswerMode 复用精确草稿捕获及确认归属。
  const setAnswerMode = React.useCallback((mode: AnswerMode) => {
    updateCurrent((entry) => ({ ...entry, answerMode: mode }));
  }, [updateCurrent]);
  const setPermissionMode = React.useCallback((mode: PermissionMode) => {
    updateCurrent((entry) => ({ ...entry, permissionMode: mode }));
  }, [updateCurrent]);
  const setVmmPolicy = React.useCallback((policy: string, mode: string) => {
    if (!["inherit", "enabled", "disabled"].includes(mode)) return;
    updateCurrent((entry) => {
      const currentPolicy = entry.vmmPolicy ?? {
        precheckMode: "inherit",
        memoryWritebackMode: "inherit",
      };
      if (policy === "precheck") {
        return {
          ...entry,
          vmmPolicy: { ...currentPolicy, precheckMode: mode as ComposerVmmPolicySnapshot["precheckMode"] },
        };
      }
      if (policy === "writeback") {
        return {
          ...entry,
          vmmPolicy: {
            ...currentPolicy,
            memoryWritebackMode: mode as ComposerVmmPolicySnapshot["memoryWritebackMode"],
          },
        };
      }
      return entry;
    });
  }, [updateCurrent]);
  const setProviderToolSettings = React.useCallback((
    settings: NonNullable<DraftSessionSettingsEntry["providerToolSettings"]>,
  ) => {
    updateCurrent((entry) => ({ ...entry, providerToolSettings: settings }));
  }, [updateCurrent]);
  const setOutputStyle = React.useCallback((styleId: string) => {
    const normalizedStyleId = styleId.trim();
    if (!normalizedStyleId) return;
    updateCurrent((entry) => ({ ...entry, outputStyleId: normalizedStyleId }));
  }, [updateCurrent]);
  const clearOutputStyle = React.useCallback(() => {
    updateCurrent((entry) => {
      if (!entry.outputStyleId) return entry;
      const next = { ...entry };
      delete next.outputStyleId;
      return next;
    });
  }, [updateCurrent]);
  const setKnowledgeSelection = React.useCallback((selection: ComposerKnowledgeSelectionSnapshot) => {
    updateCurrent((entry) => ({ ...entry, knowledgeSelection: selection }));
  }, [updateCurrent]);
  const setInitialGoal = React.useCallback((objective: string, tokenBudget: number | null) => {
    updateCurrent((entry) => ({
      ...entry,
      initialGoal: { objective, tokenBudget },
    }));
  }, [updateCurrent]);
  const clearInitialGoal = React.useCallback(() => {
    updateCurrent((entry) => {
      const next = { ...entry };
      delete next.initialGoal;
      return next;
    });
  }, [updateCurrent]);
  const setPromptSkillOverrides = React.useCallback((overrides: ComposerPromptSkillOverrides) => {
    updateCurrent((entry) => ({ ...entry, promptSkillOverrides: { ...overrides } }));
  }, [updateCurrent]);
  const captureSubmissionSettings = React.useCallback(() => {
    // snapshot is the exact object forwarded by the send controller and acknowledged on success.
    // snapshot 是发送控制器转发并在成功时确认的精确对象。
    const snapshot = captureDraftSessionSettingsSnapshot(
      entriesRef.current,
      scopeKey,
      conversationId,
    );
    if (snapshot !== null) capturedEntriesRef.current.set(snapshot, entriesRef.current[scopeKey]);
    return snapshot;
  }, [conversationId, scopeKey]);
  // clearSubmissionSettings consumes only the acknowledged immutable version, preserving edits made during the request.
  // clearSubmissionSettings 仅消费已确认的不可变版本，保留请求期间发生的编辑。
  // Parameter: snapshot is the accepted delivery object; returns without changing newer or uncaptured entries.
  // 参数：snapshot 是已接受的传输对象；存在更新或未捕获的条目时不作修改并返回。
  const clearSubmissionSettings = React.useCallback((snapshot: ComposerSessionSettingsSnapshot | null) => {
    if (snapshot === null) return;
    // capturedEntry is absent for an already-consumed or foreign snapshot, neither of which may authorize deletion.
    // capturedEntry 对已消费或其他实例的快照为空，这两类快照均不能授权删除。
    const capturedEntry = capturedEntriesRef.current.get(snapshot);
    capturedEntriesRef.current.delete(snapshot);
    if (!capturedEntry || entriesRef.current[scopeKey] !== capturedEntry) return;
    const nextEntries = clearDraftSessionSettingsEntry(
      entriesRef.current,
      scopeKey,
      conversationId,
    );
    if (nextEntries === entriesRef.current) return;
    entriesRef.current = nextEntries;
    setEntries(nextEntries);
  }, [conversationId, scopeKey]);
  const initialGoalSummary = current.initialGoal
    ? syntheticDraftGoal(scopeKey, current.initialGoal)
    : null;
  return {
    answerMode: current.answerMode ?? null,
    setAnswerMode,
    permissionMode: current.permissionMode ?? null,
    setPermissionMode,
    outputStyleId: current.outputStyleId ?? null,
    vmmPolicy: current.vmmPolicy ?? null,
    providerToolSettings: current.providerToolSettings,
    knowledgeSelection: current.knowledgeSelection ?? null,
    initialGoalSummary,
    setVmmPolicy,
    setOutputStyle,
    clearOutputStyle,
    setProviderToolSettings,
    setKnowledgeSelection,
    setInitialGoal,
    clearInitialGoal,
    setPromptSkillOverrides,
    captureSubmissionSettings,
    clearSubmissionSettings,
  };
}

// syntheticDraftGoal projects a draft goal through the existing goal-strip component before persistence.
// syntheticDraftGoal 在持久化前通过现有目标条组件投影草稿目标。
function syntheticDraftGoal(
  scopeKey: string,
  goal: ComposerInitialGoalSnapshot,
): SessionGoalSummary {
  const now = Date.now();
  return {
    sessionId: scopeKey,
    goalId: `${scopeKey}:draft-goal`,
    objective: goal.objective,
    status: "active",
    tokenBudget: goal.tokenBudget,
    tokensUsed: 0,
    usageTokens: 0,
    apiCallCount: 0,
    toolCallCount: 0,
    inputTokens: 0,
    cachedInputTokens: 0,
    outputTokens: 0,
    reasoningTokens: 0,
    totalTokens: 0,
    timeUsedSeconds: 0,
    createdAtMs: now,
    updatedAtMs: now,
  };
}
