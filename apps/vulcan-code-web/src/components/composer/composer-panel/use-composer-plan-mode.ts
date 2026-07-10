import React from "react";
import { toast } from "sonner";
import { useI18n } from "@/i18n";
import type { SessionGoalSummary, SessionPlanModeSummary } from "@/types";

// ScopedPlanModeState binds a local or optimistic Plan value to one draft slot.
// ScopedPlanModeState 将本地或乐观 Plan 值绑定到一个草稿槽。
interface ScopedPlanModeState {
  // Draft slot that owns this transient value.
  // 拥有该临时值的草稿槽。
  scopeKey: string;
  // Whether Plan mode is enabled in this scope.
  // 该作用域是否启用 Plan 模式。
  enabled: boolean;
}

// UseComposerPlanModeOptions defines the draft scope, authoritative snapshot, and persistence action.
// UseComposerPlanModeOptions 定义草稿作用域、权威快照与持久化操作。
export interface UseComposerPlanModeOptions {
  // Stable conversation or new-conversation draft slot.
  // 稳定的会话或新建会话草稿槽。
  scopeKey: string;
  // Persisted conversation id, or null for a new-conversation draft.
  // 已持久化会话标识，新建会话草稿时为 null。
  conversationId: string | null;
  // Backend-persisted Plan mode snapshot for the selected conversation.
  // 当前会话的后端持久化 Plan 模式快照。
  sessionPlanMode: SessionPlanModeSummary | null;
  // Visible non-completed goal that is mutually exclusive with Plan mode.
  // 与 Plan 模式互斥的可见未完成目标。
  goal: SessionGoalSummary | null;
  // Persists one Plan mode transition for an existing conversation.
  // 为已存在会话持久化一次 Plan 模式转换。
  onSetSessionPlanMode: (enabled: boolean) => Promise<boolean>;
}

// ComposerPlanModeController exposes one consistent Plan value to UI, submissions, goals, and shortcuts.
// ComposerPlanModeController 向界面、提交、目标与快捷键暴露一个一致的 Plan 值。
export interface ComposerPlanModeController {
  // Effective Plan mode shown by composer controls.
  // 输入区控件展示的生效 Plan 模式。
  planModeEnabled: boolean;
  // Submission execution mode derived directly from the effective Plan value.
  // 直接从生效 Plan 值派生的提交执行模式。
  executionMode: "plan" | "build";
  // Applies one local or persisted Plan mode transition.
  // 应用一次本地或持久化 Plan 模式转换。
  setPlanMode: (enabled: boolean) => Promise<boolean>;
  // Disables Plan mode before the mutually exclusive goal editor opens.
  // 在互斥的目标编辑器打开前禁用 Plan 模式。
  prepareGoalEditor: () => Promise<boolean>;
  // Captures Shift+Tab as the composer work-mode shortcut.
  // 将 Shift+Tab 捕获为输入区工作模式快捷键。
  handleContainerKeyDown: (event: React.KeyboardEvent<HTMLElement>) => void;
}

// useComposerPlanMode unifies draft-local state, persisted snapshots, optimistic transitions, and goal exclusion.
// useComposerPlanMode 统一草稿本地状态、持久化快照、乐观转换与目标互斥。
// Parameters: options provides the active scope, authoritative snapshot, visible goal, and persistence callback.
// 参数：options 提供活动作用域、权威快照、可见目标与持久化回调。
// Returns: one effective mode plus transition, goal-preparation, toggle, and shortcut actions.
// 返回值：一个生效模式，以及转换、目标准备、切换与快捷键操作。
export function useComposerPlanMode(options: UseComposerPlanModeOptions): ComposerPlanModeController {
  // t resolves the stable goal-conflict error for the current locale.
  // t 为当前语言解析稳定的目标冲突错误。
  const { t } = useI18n();
  // draftModeState stores local Plan mode only for the currently identified new-conversation scope.
  // draftModeState 仅为当前标识的新建会话作用域保存本地 Plan 模式。
  const [draftModeState, setDraftModeState] = React.useState<ScopedPlanModeState>({
    scopeKey: options.scopeKey,
    enabled: false,
  });
  // persistedOverride stores the latest successful transition until its authoritative snapshot arrives.
  // persistedOverride 保存最近成功的转换，直到权威快照到达。
  const [persistedOverride, setPersistedOverride] = React.useState<ScopedPlanModeState | null>(null);
  // activeScopeKeyRef prevents an old async transition from mutating a newly selected scope.
  // activeScopeKeyRef 防止旧异步转换修改新选中的作用域。
  const activeScopeKeyRef = React.useRef(options.scopeKey);
  activeScopeKeyRef.current = options.scopeKey;
  // requestVersionRef ensures only the latest overlapping persisted transition may publish an override.
  // requestVersionRef 确保只有最新的重叠持久化转换可以发布乐观覆盖。
  const requestVersionRef = React.useRef(0);
  // persistedPlanModeEnabled is the authoritative backend value for an existing conversation.
  // persistedPlanModeEnabled 是已存在会话的后端权威值。
  const persistedPlanModeEnabled = options.sessionPlanMode?.executionMode?.toLowerCase() === "plan";
  // scopedDraftModeEnabled ignores draft state owned by another new-conversation slot.
  // scopedDraftModeEnabled 忽略属于另一个新建会话槽的草稿状态。
  const scopedDraftModeEnabled = draftModeState.scopeKey === options.scopeKey
    ? draftModeState.enabled
    : false;
  // scopedPersistedOverride ignores optimistic state owned by another conversation slot.
  // scopedPersistedOverride 忽略属于另一个会话槽的乐观状态。
  const scopedPersistedOverride = persistedOverride?.scopeKey === options.scopeKey
    ? persistedOverride.enabled
    : null;
  // planModeEnabled selects draft-local state for new conversations and persisted state for existing ones.
  // planModeEnabled 为新建会话选择草稿本地状态，为已存在会话选择持久化状态。
  const planModeEnabled = options.conversationId
    ? scopedPersistedOverride ?? persistedPlanModeEnabled
    : scopedDraftModeEnabled;
  // executionMode is derived rather than mirrored, preventing UI/submission divergence.
  // executionMode 直接派生而非镜像，防止界面与提交模式分裂。
  const executionMode: "plan" | "build" = planModeEnabled ? "plan" : "build";

  React.useEffect(() => {
    if (!options.conversationId && draftModeState.scopeKey !== options.scopeKey) {
      setDraftModeState({ scopeKey: options.scopeKey, enabled: false });
    }
    setPersistedOverride((current) => {
      if (!current || current.scopeKey !== options.scopeKey) {
        return null;
      }
      if (current.enabled === persistedPlanModeEnabled) {
        return null;
      }
      return current;
    });
  }, [draftModeState.scopeKey, options.conversationId, options.scopeKey, persistedPlanModeEnabled]);

  // setPlanMode applies goal exclusion before routing a scope-correct local or persisted transition.
  // setPlanMode 在路由作用域正确的本地或持久化转换前应用目标互斥。
  const setPlanMode = React.useCallback(async (enabled: boolean) => {
    if (enabled && options.goal) {
      toast.error(t("composer.planMode.goalConflict"));
      return false;
    }
    // requestScopeKey binds this transition to the session visible when it started.
    // requestScopeKey 将本次转换绑定到启动时可见的会话。
    const requestScopeKey = options.scopeKey;
    if (!options.conversationId) {
      setDraftModeState({ scopeKey: requestScopeKey, enabled });
      return true;
    }
    // requestVersion identifies this transition among overlapping persisted requests.
    // requestVersion 在重叠持久化请求中标识本次转换。
    const requestVersion = requestVersionRef.current + 1;
    requestVersionRef.current = requestVersion;
    // saved records whether the session service accepted this transition.
    // saved 记录会话服务是否接受本次转换。
    const saved = await options.onSetSessionPlanMode(enabled);
    if (
      !saved
      || activeScopeKeyRef.current !== requestScopeKey
      || requestVersionRef.current !== requestVersion
    ) {
      return false;
    }
    setPersistedOverride({ scopeKey: requestScopeKey, enabled });
    return true;
  }, [options.conversationId, options.goal, options.onSetSessionPlanMode, options.scopeKey, t]);
  // prepareGoalEditor disables Plan mode only when the effective scope currently requires it.
  // prepareGoalEditor 仅在当前生效作用域需要时禁用 Plan 模式。
  const prepareGoalEditor = React.useCallback(async () => {
    if (!planModeEnabled) {
      return true;
    }
    return setPlanMode(false);
  }, [planModeEnabled, setPlanMode]);
  // togglePlanMode routes the inverse effective value through the same transition boundary.
  // togglePlanMode 通过同一转换边界路由生效值的反值。
  const togglePlanMode = React.useCallback(() => {
    void setPlanMode(!planModeEnabled);
  }, [planModeEnabled, setPlanMode]);
  // handleContainerKeyDown captures Shift+Tab without leaking the shortcut into surrounding surfaces.
  // handleContainerKeyDown 捕获 Shift+Tab，且不让快捷键泄漏到外围界面。
  const handleContainerKeyDown = React.useCallback((event: React.KeyboardEvent<HTMLElement>) => {
    if (event.key !== "Tab" || !event.shiftKey) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    togglePlanMode();
  }, [togglePlanMode]);

  return {
    planModeEnabled,
    executionMode,
    setPlanMode,
    prepareGoalEditor,
    handleContainerKeyDown,
  };
}
