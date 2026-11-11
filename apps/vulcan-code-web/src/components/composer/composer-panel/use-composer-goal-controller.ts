import React from "react";
import { toast } from "@/ui-kit/primitives/toast";
import { useI18n } from "@/i18n";
import type { SessionGoalSummary, SessionGoalUserStatusUpdate } from "@/types";
import { composerGoalToggleTarget } from "./goal-status-action";

// ComposerGoalDialogMode identifies whether the dialog edits the current goal or resumes a stopped goal.
// ComposerGoalDialogMode 标识弹窗是在编辑当前目标，还是继续一个已停止目标。
export type ComposerGoalDialogMode = "edit" | "resume";

// UseComposerGoalControllerOptions defines the persisted operations and composer coordination needed by the goal controller.
// UseComposerGoalControllerOptions 定义目标控制器所需的持久化操作与输入区协作能力。
export interface UseComposerGoalControllerOptions {
  // scope identifies the owner of dialog effects and first-turn submission.
  // scope 标识弹窗副作用和首轮提交的所有者。
  scope: {
    // key identifies the durable session or new-conversation draft slot.
    // key 标识稳定会话或新建会话草稿槽。
    key: string;
    // owner selects draft submission or server-owned Goal dispatch.
    // owner 选择草稿提交或由服务端负责目标派发。
    owner: "draft" | "session";
  };
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
  const activeScopeKeyRef = React.useRef(options.scope.key);
  activeScopeKeyRef.current = options.scope.key;
  // operationGuard owns one synchronous operation for one continuous visit to a conversation.
  // operationGuard 为每次连续停留的会话持有一个同步操作守卫。
  // Replacing the guard on scope changes also rejects stale completion after navigating away and back.
  // 作用域变更时替换守卫，也能拒绝离开后返回同一会话的旧完成事件。
  const operationGuard = React.useRef({ scopeKey: options.scope.key, busy: false });
  if (operationGuard.current.scopeKey !== options.scope.key) {
    operationGuard.current = { scopeKey: options.scope.key, busy: false };
  }
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
  }, [options.scope.key]);

  // openEditor seeds the goal editor after conflicting composer modes have been disabled.
  // openEditor 在禁用冲突的输入区模式后初始化目标编辑器。
  const openEditor = React.useCallback(async () => {
    // guard captures this visit's identity before asynchronous mode preparation starts.
    // guard 在异步模式准备前捕获本次会话访问身份。
    const guard = operationGuard.current;
    if (guard.busy) return;
    guard.busy = true;
    setSaving(true);
    // editorScopeKey binds asynchronous preparation to the visible session that requested it.
    // editorScopeKey 将异步准备绑定到发起请求的可见会话。
    const editorScopeKey = options.scope.key;
    // prepared records whether the caller successfully resolved mutually exclusive state.
    // prepared 记录调用方是否成功解除互斥状态。
    try {
      const prepared = await options.onPrepareEditor();
      if (!prepared || operationGuard.current !== guard || activeScopeKeyRef.current !== editorScopeKey) return;
      setDialogMode("edit");
      setObjectiveDraft(options.goal?.objective ?? "");
      setTokenBudgetDraft(options.goal?.tokenBudget ? String(options.goal.tokenBudget) : "");
      setDialogOpen(true);
    } catch (error) {
      if (operationGuard.current === guard) toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      guard.busy = false;
      if (operationGuard.current === guard) setSaving(false);
    }
  }, [options.goal, options.onPrepareEditor, options.scope.key]);

  // openResumeEditor seeds the next-stage editor from the stopped goal.
  // openResumeEditor 使用已停止目标初始化下一阶段编辑器。
  const openResumeEditor = React.useCallback(() => {
    if (!options.goal || operationGuard.current.busy) {
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
    // guard prevents repeated persistence and excludes stale completion from a different visit.
    // guard 阻止重复持久化，并排除其他会话访问的旧完成事件。
    const guard = operationGuard.current;
    if (guard.busy) return;
    // requestScopeKey binds persistence and auto-submit side effects to the initiating session.
    // requestScopeKey 将持久化与自动提交副作用绑定到发起会话。
    const requestScopeKey = options.scope.key;
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
    if (tokenBudget !== null && (!Number.isSafeInteger(tokenBudget) || tokenBudget <= 0)) {
      toast.error(t("composer.goal.error.budgetInvalid"));
      return;
    }
    // Only a draft needs the composer to submit its first turn. The service owns existing Goal dispatch.
    // 只有草稿需要输入区提交首轮，已有会话的目标派发由服务端负责。
    const shouldAutoSubmit = options.scope.owner === "draft"
      && !options.goal && dialogMode === "edit" && options.canAutoSubmitCreatedGoal;
    guard.busy = true;
    setSaving(true);
    try {
      // saved records whether the backend accepted the requested goal mutation.
      // saved 记录后端是否接受请求的目标修改。
      const saved = dialogMode === "resume"
        ? await options.onResumeGoal(objective, tokenBudget)
        : await options.onSaveGoal(objective, tokenBudget);
      if (!saved || operationGuard.current !== guard || activeScopeKeyRef.current !== requestScopeKey) {
        return;
      }
      setDialogOpen(false);
      if (shouldAutoSubmit) {
        await options.onSubmitCreatedGoal(objective);
      }
    } catch (error) {
      if (operationGuard.current === guard) toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      guard.busy = false;
      if (operationGuard.current === guard) setSaving(false);
    }
  }, [
    dialogMode,
    objectiveDraft,
    options.canAutoSubmitCreatedGoal,
    options.goal,
    options.onResumeGoal,
    options.onSaveGoal,
    options.onSubmitCreatedGoal,
    options.scope.owner,
    options.scope.key,
    t,
    tokenBudgetDraft,
  ]);

  // toggleStatus pauses or reactivates the current visible goal.
  // toggleStatus 暂停或重新激活当前可见目标。
  const toggleStatus = React.useCallback(async () => {
    // guard identifies the accepted mutation and keeps old responses from releasing a newer mutation.
    // guard 标识已接受变更，防止旧响应释放较新的变更。
    const guard = operationGuard.current;
    if (!options.goal || guard.busy) {
      return;
    }
    // nextStatus is the explicit persisted lifecycle transition requested by the user.
    // nextStatus 是用户请求的显式持久化生命周期转换。
    const nextStatus = composerGoalToggleTarget(options.goal.status);
    if (nextStatus === null) {
      return;
    }
    guard.busy = true;
    setSaving(true);
    try {
      await options.onUpdateGoalStatus(nextStatus);
    } catch (error) {
      if (operationGuard.current === guard) toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      guard.busy = false;
      if (operationGuard.current === guard) setSaving(false);
    }
  }, [options.goal, options.onUpdateGoalStatus]);

  // clear removes the current visible goal after a direct user action.
  // clear 在用户直接操作后移除当前可见目标。
  const clear = React.useCallback(async () => {
    // guard serializes clear against save, resume, and status changes for this visit.
    // guard 将清理与本次访问的保存、继续及状态变更串行化。
    const guard = operationGuard.current;
    if (guard.busy) return;
    guard.busy = true;
    setSaving(true);
    try {
      await options.onClearGoal();
    } catch (error) {
      if (operationGuard.current === guard) toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      guard.busy = false;
      if (operationGuard.current === guard) setSaving(false);
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
