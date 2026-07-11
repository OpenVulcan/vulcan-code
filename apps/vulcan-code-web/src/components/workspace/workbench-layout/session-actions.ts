import React from "react";
import type { ComposerAiCapabilityConfigurationSave } from "@/components/composer/composer-model";
import type { DraftConversationSelection } from "@/components/workspace/workspace-resizable-body";
import type {
  ConversationSummary,
  PendingPromptSummary,
  SessionGoalUserStatusUpdate,
  WorkbenchSnapshot,
} from "@/types";
import {
  clearSessionGoal,
  editPendingPrompt,
  resumeSessionGoal,
  saveSessionAiCapabilityConfiguration,
  saveSessionGoal,
  setSessionPlanMode,
  setSessionVmmPolicy,
  updateSessionGoalStatus,
} from "@/service/conversation-actions-service";
import { applySessionAddonsChangedResponse } from "@/state/workbench-state";
import { loadWorkbenchSnapshot } from "@/service/workbench-snapshot-service";
import { useI18n } from "@/i18n";
import { toast } from "sonner";

// WorkbenchSessionActionsOptions groups active session state and snapshot update bridges.
// WorkbenchSessionActionsOptions 归组当前会话状态与快照更新桥接。
interface WorkbenchSessionActionsOptions {
  // Currently selected persisted conversation, if any.
  // 当前选中的持久化会话。
  selectedConversation: ConversationSummary | undefined;
  // Currently selected draft conversation, if any.
  // 当前选中的草稿会话。
  selectedDraftConversation: DraftConversationSelection | null;
  // Updates the local workbench snapshot.
  // 更新本地工作台快照。
  setWorkbenchState: React.Dispatch<React.SetStateAction<WorkbenchSnapshot>>;
  // Merges a service snapshot with local caches.
  // 将服务快照与本地缓存合并。
  mergeWorkbenchWithLocalCaches: (current: WorkbenchSnapshot, incoming: WorkbenchSnapshot) => WorkbenchSnapshot;
}

// useWorkbenchSessionActions owns active-session AI capability, goal, plan-mode, and prompt-edit callbacks.
// useWorkbenchSessionActions 持有当前会话 AI 能力、目标、计划模式与提示词编辑回调。
export function useWorkbenchSessionActions(options: WorkbenchSessionActionsOptions) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  const {
    selectedConversation,
    selectedDraftConversation,
    setWorkbenchState,
    mergeWorkbenchWithLocalCaches,
  } = options;
  // handleSaveAiCapabilityConfiguration persists the composer capability dialog for the active session.
  // handleSaveAiCapabilityConfiguration 持久化当前会话的输入区能力弹窗配置。
  const handleSaveAiCapabilityConfiguration = React.useCallback(async (request: ComposerAiCapabilityConfigurationSave): Promise<boolean> => {
    const activeProjectId =
      selectedDraftConversation?.projectId
      ?? selectedConversation?.projectId
      ?? null;
    const activeConversationId = selectedDraftConversation
      ? null
      : selectedConversation?.id ?? null;
    if (!activeProjectId) {
      toast.error(t("sessionActions.noProjectForCapability"));
      return false;
    }
    if (!activeConversationId) {
      toast.error(t("sessionActions.noConversationForCapability"));
      return false;
    }
    try {
      const snapshot = await saveSessionAiCapabilityConfiguration(
        activeProjectId,
        activeConversationId,
        request.disabledAiCapabilityKinds,
        request.disabledAiCapabilityTargetSelectors,
        request.aiCapabilityTargetOrderOverrides,
      );
      if (!snapshot) {
        toast.info(t("sessionActions.previewCannotSaveCapability"));
        return false;
      }
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      toast.success(t("sessionActions.capabilitySaved"));
      return true;
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      return false;
    }
  }, [mergeWorkbenchWithLocalCaches, selectedConversation, selectedDraftConversation, t]);

  // handleSaveSessionGoal persists one explicit session goal from the composer dialog.
  // handleSaveSessionGoal 从输入区弹窗持久化一个显式会话目标。
  const handleSaveSessionGoal = React.useCallback(async (objective: string, tokenBudget?: number | null): Promise<boolean> => {
    const activeConversationId = selectedDraftConversation ? null : selectedConversation?.id ?? null;
    if (!activeConversationId) {
      toast.error(t("sessionActions.noConversationForGoalSave"));
      return false;
    }
    try {
      const response = await saveSessionGoal(activeConversationId, objective, tokenBudget);
      if (!response) {
        toast.info(t("sessionActions.previewCannotSaveGoal"));
        return false;
      }
      setWorkbenchState((current) => applySessionAddonsChangedResponse(current, response));
      toast.success(t("sessionActions.goalSaved"));
      return true;
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      return false;
    }
  }, [selectedConversation, selectedDraftConversation, t]);

  // handleResumeSessionGoal resumes a stopped goal as a fresh budget stage.
  // handleResumeSessionGoal 将已停止目标恢复为新的预算阶段。
  const handleResumeSessionGoal = React.useCallback(async (objective: string, tokenBudget?: number | null): Promise<boolean> => {
    const activeConversationId = selectedDraftConversation ? null : selectedConversation?.id ?? null;
    if (!activeConversationId) {
      toast.error(t("sessionActions.noExistingConversation"));
      return false;
    }
    try {
      const response = await resumeSessionGoal(activeConversationId, objective, tokenBudget);
      if (!response) {
        toast.info(t("sessionActions.previewCannotResumeGoal"));
        return false;
      }
      setWorkbenchState((current) => applySessionAddonsChangedResponse(current, response));
      toast.success(t("sessionActions.goalResumed"));
      return true;
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      return false;
    }
  }, [selectedConversation, selectedDraftConversation, t]);

  // handleClearSessionGoal removes the current explicit session goal.
  // handleClearSessionGoal 移除当前显式会话目标。
  const handleClearSessionGoal = React.useCallback(async (): Promise<boolean> => {
    const activeConversationId = selectedDraftConversation ? null : selectedConversation?.id ?? null;
    if (!activeConversationId) {
      toast.error(t("sessionActions.noExistingConversation"));
      return false;
    }
    try {
      const response = await clearSessionGoal(activeConversationId);
      if (!response) {
        toast.info(t("sessionActions.previewCannotClearGoal"));
        return false;
      }
      setWorkbenchState((current) => applySessionAddonsChangedResponse(current, response));
      toast.success(t("sessionActions.goalCleared"));
      return true;
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      return false;
    }
  }, [selectedConversation, selectedDraftConversation, t]);

  // handleUpdateSessionGoalStatus updates the current goal lifecycle state.
  // handleUpdateSessionGoalStatus 更新当前目标生命周期状态。
  const handleUpdateSessionGoalStatus = React.useCallback(async (
    status: SessionGoalUserStatusUpdate,
  ): Promise<boolean> => {
    const activeConversationId = selectedDraftConversation ? null : selectedConversation?.id ?? null;
    if (!activeConversationId) {
      toast.error(t("sessionActions.noExistingConversation"));
      return false;
    }
    try {
      const response = await updateSessionGoalStatus(activeConversationId, status);
      if (!response) {
        toast.info(t("sessionActions.previewCannotUpdateGoal"));
        return false;
      }
      setWorkbenchState((current) => applySessionAddonsChangedResponse(current, response));
      return true;
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      return false;
    }
  }, [selectedConversation, selectedDraftConversation, t]);

  // handleSetSessionPlanMode persists the current conversation Plan-mode toggle.
  // handleSetSessionPlanMode 持久化当前会话的计划模式开关。
  const handleSetSessionPlanMode = React.useCallback(async (enabled: boolean): Promise<boolean> => {
    const activeConversationId = selectedDraftConversation ? null : selectedConversation?.id ?? null;
    if (!activeConversationId) {
      toast.error(t("sessionActions.noConversationForPlanMode"));
      return false;
    }
    try {
      const response = await setSessionPlanMode(activeConversationId, enabled);
      if (!response) {
        toast.info(t("sessionActions.previewCannotTogglePlanMode"));
        return false;
      }
      setWorkbenchState((current) => applySessionAddonsChangedResponse(current, response));
      toast.success(enabled ? t("sessionActions.planModeEnabled") : t("sessionActions.planModeDisabled"));
      return true;
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      return false;
    }
  }, [selectedConversation, selectedDraftConversation, t]);

  // handleSetSessionVmmPolicy persists one current conversation VMM memory policy mode.
  // handleSetSessionVmmPolicy 持久化当前会话的单个 VMM 记忆策略模式。
  const handleSetSessionVmmPolicy = React.useCallback(async (policy: string, mode: string): Promise<boolean> => {
    const activeConversationId = selectedDraftConversation ? null : selectedConversation?.id ?? null;
    if (!activeConversationId) {
      toast.error(t("sessionActions.noConversationForVmmPolicy"));
      return false;
    }
    try {
      const response = await setSessionVmmPolicy(activeConversationId, policy, mode);
      if (!response) {
        toast.info(t("sessionActions.previewCannotSetVmmPolicy"));
        return false;
      }
      setWorkbenchState((current) => applySessionAddonsChangedResponse(current, response));
      toast.success(t("sessionActions.vmmPolicySaved"));
      return true;
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      return false;
    }
  }, [selectedConversation, selectedDraftConversation, t]);

  // handleEditPendingPrompt restores one queued prompt back into the active composer.
  // handleEditPendingPrompt 将单条排队提示词恢复到当前输入区。
  const handleEditPendingPrompt = React.useCallback(async (entryId: string): Promise<PendingPromptSummary | null> => {
    try {
      const pendingPrompt = await editPendingPrompt(entryId);
      if (!pendingPrompt) {
        return null;
      }
      const snapshot = await loadWorkbenchSnapshot();
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      return pendingPrompt;
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      return null;
    }
  }, []);

  return {
    handleSaveAiCapabilityConfiguration,
    handleSaveSessionGoal,
    handleResumeSessionGoal,
    handleClearSessionGoal,
    handleUpdateSessionGoalStatus,
    handleSetSessionPlanMode,
    handleSetSessionVmmPolicy,
    handleEditPendingPrompt,
  };
}
