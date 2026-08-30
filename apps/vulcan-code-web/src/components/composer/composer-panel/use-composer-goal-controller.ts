import React from "react";
import { toast } from "sonner";
import { useI18n } from "@/i18n";
import type { SessionGoalSummary, SessionGoalUserStatusUpdate } from "@/types";
import { composerGoalToggleTarget } from "./goal-status-action";

// ComposerGoalDialogMode identifies whether the dialog edits the current goal or resumes a stopped goal.
// ComposerGoalDialogMode 标识弹窗是在编辑当前目标，还是继续一个已停止目标。
export type ComposerGoalDialogMode = "edit" | "resume";

// UseComposerGoalControllerOptions defines the persisted operations and composer coordination needed by the goal controller.
// UseComposerGoalControllerOptions 定义目标控制器所需的持久化操作与输入区协作能力。
export interface UseComposerGoalControllerOptions {
  // Stable conversation or new-conversation draft slot that owns dialog side effects.
  // 持有弹窗副作用的稳定会话或新建会话草稿槽。
  scopeKey: string;
  // Visible non-completed goal rendered by the composer.
  // 输入区渲染的未完成可见目标。
  goal: SessionGoalSummary | null;
  // Whether a newly created goal should immediately start a backend turn.
  // 新建目标是否应立即启动一个后端轮次。
  canAutoSubmitCreatedGoal: boolean;
  // Prepares mutually exclusive composer state before opening the goal editor.
  // 在打开目标编辑器前准备互斥的输入区状态。
  onPrepareEditor: () => Promise<boolean>;
  // Persists an ordinary goal create or edit request.
  // 持久化普通目标创建或编辑请求。
  onSaveGoal: (objective: string, tokenBudget?: number | null) => Promise<boolean>;
  // Persists a fresh budget stage for a stopped goal.
  // 为已停止目标持久化新的预算阶段。
  onResumeGoal: (objective: string, tokenBudget?: number | null) => Promise<boolean>;
  // Removes the current persisted goal.
  // 移除当前持久化目标。
  onClearGoal: () => Promise<boolean>;
  // Updates the current persisted goal lifecycle status.
  // 更新当前持久化目标的生命周期状态。
  onUpdateGoalStatus: (status: SessionGoalUserStatusUpdate) => Promise<boolean>;
  // Starts the first backend turn after a new goal has been persisted.
  // 在新目标持久化后启动第一个后端轮次。
  onSubmitCreatedGoal: (objective: string) => Promise<void>;
}

// ComposerGoalController exposes goal dialog state and lifecycle actions to the composer view.
// ComposerGoalController 向输入区视图暴露目标弹窗状态与生命周期操作。
export interface ComposerGoalController {
  // Whether the goal editor dialog is open.
  // 目标编辑弹窗是否打开。
  dialogOpen: boolean;
  // Current goal editor operation mode.
  // 当前目标编辑操作模式。
  dialogMode: ComposerGoalDialogMode;
  // Editable goal objective text.
  // 可编辑的目标文本。
  objectiveDraft: string;
  // Editable optional token budget text.
  // 可编辑的可选 token 预算文本。
  tokenBudgetDraft: string;
  // Whether a goal mutation is currently in flight.
  // 当前是否存在飞行中的目标修改。
  saving: boolean;
  // Updates goal editor visibility.
  // 更新目标编辑器可见性。
  setDialogOpen: React.Dispatch<React.SetStateAction<boolean>>;
  // Updates the editable objective text.
  // 更新可编辑的目标文本。
  setObjectiveDraft: React.Dispatch<React.SetStateAction<string>>;
  // Updates the editable token budget text.
  // 更新可编辑的 token 预算文本。
  setTokenBudgetDraft: React.Dispatch<React.SetStateAction<string>>;
  // Opens the ordinary goal editor after preparing mutually exclusive state.
  // 在准备互斥状态后打开普通目标编辑器。
  openEditor: () => Promise<void>;
  // Opens the stopped-goal resume editor.
  // 打开已停止目标的继续编辑器。
  openResumeEditor: () => void;
  // Validates and persists the current dialog draft.
  // 校验并持久化当前弹窗草稿。
  save: () => Promise<void>;
  // Pauses or reactivates the current goal.
  // 暂停或重新激活当前目标。
  toggleStatus: () => Promise<void>;
  // Clears the current goal.
  // 清理当前目标。
  clear: () => Promise<void>;
}

// useComposerGoalController owns goal editor state and orchestrates goal lifecycle mutations.
// useComposerGoalController 持有目标编辑器状态并编排目标生命周期修改。
// Parameters: options provides the visible goal, persistence callbacks, and composer coordination hooks.
// 参数：options 提供可见目标、持久化回调与输入区协作钩子。
// Returns: dialog state and stable lifecycle actions consumed by ComposerPanel.
// 返回值：ComposerPanel 使用的弹窗状态与稳定生命周期操作。
export function useComposerGoalController(options: UseComposerGoalControllerOptions): ComposerGoalController {
  // t resolves stable goal validation messages for the current locale.
  // t 为当前语言解析稳定的目标校验消息。
  const { t } = useI18n();
  // activeScopeKeyRef identifies the scope allowed to receive in-flight dialog side effects.
  // activeScopeKeyRef 标识允许接收飞行中弹窗副作用的作用域。
  const activeScopeKeyRef = React.useRef(options.scopeKey);
  activeScopeKeyRef.current = options.scopeKey;
  // dialogOpen controls the user-facing session goal editor.
  // dialogOpen 控制面向用户的会话目标编辑器。
  const [dialogOpen, setDialogOpen] = React.useState(false);
  // dialogMode distinguishes ordinary edits from budget-stage resumes.
  // dialogMode 区分普通编辑与预算阶段继续。
  const [dialogMode, setDialogMode] = React.useState<ComposerGoalDialogMode>("edit");
  // objectiveDraft stores the editable goal objective.
  // objectiveDraft 保存可编辑的目标文本。
  const [objectiveDraft, setObjectiveDraft] = React.useState("");
  // tokenBudgetDraft stores the optional token budget as input text.
  // tokenBudgetDraft 以输入文本保存可选 token 预算。
  const [tokenBudgetDraft, setTokenBudgetDraft] = React.useState("");
  // saving stores whether a goal mutation is currently in flight.
  // saving 保存当前是否存在飞行中的目标修改。
  const [saving, setSaving] = React.useState(false);

  React.useEffect(() => {
    setDialogOpen(false);
    setDialogMode("edit");
    setSaving(false);
  }, [options.scopeKey]);

  // openEditor seeds the goal editor after conflicting composer modes have been disabled.
  // openEditor 在禁用冲突的输入区模式后初始化目标编辑器。
  const openEditor = React.useCallback(async () => {
    // editorScopeKey binds asynchronous preparation to the visible session that requested it.
    // editorScopeKey 将异步准备绑定到发起请求的可见会话。
    const editorScopeKey = options.scopeKey;
    // prepared records whether the caller successfully resolved mutually exclusive state.
    // prepared 记录调用方是否成功解除互斥状态。
    const prepared = await options.onPrepareEditor();
    if (!prepared || activeScopeKeyRef.current !== editorScopeKey) {
      return;
    }
    setDialogMode("edit");
    setObjectiveDraft(options.goal?.objective ?? "");
    setTokenBudgetDraft(options.goal?.tokenBudget ? String(options.goal.tokenBudget) : "");
    setDialogOpen(true);
  }, [options.goal, options.onPrepareEditor, options.scopeKey]);

  // openResumeEditor seeds the next-stage editor from the stopped goal.
  // openResumeEditor 使用已停止目标初始化下一阶段编辑器。
  const openResumeEditor = React.useCallback(() => {
    if (!options.goal) {
      return;
    }
    setDialogMode("resume");
    setObjectiveDraft(options.goal.objective);
    setTokenBudgetDraft(options.goal.tokenBudget ? String(options.goal.tokenBudget) : "");
    setDialogOpen(true);
  }, [options.goal]);

  // save validates and persists the current goal dialog draft.
  // save 校验并持久化当前目标弹窗草稿。
  const save = React.useCallback(async () => {
    // requestScopeKey binds persistence and auto-submit side effects to the initiating session.
    // requestScopeKey 将持久化与自动提交副作用绑定到发起会话。
    const requestScopeKey = options.scopeKey;
    // objective is the normalized non-empty goal text sent to persistence.
    // objective 是发送给持久化层的规范化非空目标文本。
    const objective = objectiveDraft.trim();
    // tokenBudgetText is the normalized optional budget input.
    // tokenBudgetText 是规范化后的可选预算输入。
    const tokenBudgetText = tokenBudgetDraft.trim();
    // tokenBudget is the parsed positive integer budget or null when omitted.
    // tokenBudget 是解析后的正整数预算，省略时为 null。
    const tokenBudget = tokenBudgetText ? Number(tokenBudgetText) : null;
    if (!objective) {
      toast.error(t("composer.goal.error.empty"));
      return;
    }
    if (tokenBudget !== null && (!Number.isInteger(tokenBudget) || tokenBudget <= 0)) {
      toast.error(t("composer.goal.error.budgetInvalid"));
      return;
    }
    // shouldAutoSubmit captures pre-save state so the first goal turn starts exactly once.
    // shouldAutoSubmit 捕获保存前状态，确保目标首轮只启动一次。
    const shouldAutoSubmit = !options.goal && dialogMode === "edit" && options.canAutoSubmitCreatedGoal;
    setSaving(true);
    try {
      // saved records whether the backend accepted the requested goal mutation.
      // saved 记录后端是否接受请求的目标修改。
      const saved = dialogMode === "resume"
        ? await options.onResumeGoal(objective, tokenBudget)
        : await options.onSaveGoal(objective, tokenBudget);
      if (!saved || activeScopeKeyRef.current !== requestScopeKey) {
        return;
      }
      setDialogOpen(false);
      if (shouldAutoSubmit) {
        await options.onSubmitCreatedGoal(objective);
      }
    } finally {
      setSaving(false);
    }
  }, [
    dialogMode,
    objectiveDraft,
    options.canAutoSubmitCreatedGoal,
    options.goal,
    options.onResumeGoal,
    options.onSaveGoal,
    options.onSubmitCreatedGoal,
    options.scopeKey,
    t,
    tokenBudgetDraft,
  ]);

  // toggleStatus pauses or reactivates the current visible goal.
  // toggleStatus 暂停或重新激活当前可见目标。
  const toggleStatus = React.useCallback(async () => {
    if (!options.goal) {
      return;
    }
    // nextStatus is the explicit persisted lifecycle transition requested by the user.
    // nextStatus 是用户请求的显式持久化生命周期转换。
    const nextStatus = composerGoalToggleTarget(options.goal.status);
    if (nextStatus === null) {
      return;
    }
    setSaving(true);
    try {
      await options.onUpdateGoalStatus(nextStatus);
    } finally {
      setSaving(false);
    }
  }, [options.goal, options.onUpdateGoalStatus]);

  // clear removes the current visible goal after a direct user action.
  // clear 在用户直接操作后移除当前可见目标。
  const clear = React.useCallback(async () => {
    setSaving(true);
    try {
      await options.onClearGoal();
    } finally {
      setSaving(false);
    }
  }, [options.onClearGoal]);

  return {
    dialogOpen,
    dialogMode,
    objectiveDraft,
    tokenBudgetDraft,
    saving,
    setDialogOpen,
    setObjectiveDraft,
    setTokenBudgetDraft,
    openEditor,
    openResumeEditor,
    save,
    toggleStatus,
    clear,
  };
}
