import React from "react";
import type { ComposerAiCapabilityConfigurationSave } from "@/components/composer/composer-model";
import type { DraftConversationSelection } from "@/components/workspace/workspace-resizable-body";
import type { ConversationSummary, PendingPromptSummary, WorkbenchSnapshot } from "@/types";
import {
  clearSessionGoal,
  editPendingPrompt,
  resumeSessionGoal,
  saveSessionAiCapabilityConfiguration,
  saveSessionGoal,
  setSessionPlanMode,
  updateSessionGoalStatus,
} from "@/service/conversation-actions-service";
import { loadWorkbenchSnapshot } from "@/service/workbench-snapshot-service";
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
      toast.error("当前会话没有明确关联项目，无法保存 AI 能力设置。");
      return false;
    }
    if (!activeConversationId) {
      toast.error("请先创建或选择一个会话，再保存 AI 能力设置。");
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
        toast.info("浏览器预览模式下无法保存会话 AI 能力设置。");
        return false;
      }
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      toast.success("已保存当前会话 AI 能力设置。");
      return true;
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      return false;
    }
  }, [mergeWorkbenchWithLocalCaches, selectedConversation, selectedDraftConversation]);

  // handleSaveSessionGoal persists one explicit session goal from the composer dialog.
  // handleSaveSessionGoal 从输入区弹窗持久化一个显式会话目标。
  const handleSaveSessionGoal = React.useCallback(async (objective: string, tokenBudget?: number | null): Promise<boolean> => {
    const activeConversationId = selectedDraftConversation ? null : selectedConversation?.id ?? null;
    if (!activeConversationId) {
      toast.error("请先创建或选择一个会话，再设置目标。");
      return false;
    }
    try {
      const snapshot = await saveSessionGoal(activeConversationId, objective, tokenBudget);
      if (!snapshot) {
        toast.info("浏览器预览模式下无法保存目标。");
        return false;
      }
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      toast.success("目标已保存。");
      return true;
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      return false;
    }
  }, [mergeWorkbenchWithLocalCaches, selectedConversation, selectedDraftConversation]);

  // handleResumeSessionGoal resumes a stopped goal as a fresh budget stage.
  // handleResumeSessionGoal 将已停止目标恢复为新的预算阶段。
  const handleResumeSessionGoal = React.useCallback(async (objective: string, tokenBudget?: number | null): Promise<boolean> => {
    const activeConversationId = selectedDraftConversation ? null : selectedConversation?.id ?? null;
    if (!activeConversationId) {
      toast.error("请先选择一个已有会话。");
      return false;
    }
    try {
      const snapshot = await resumeSessionGoal(activeConversationId, objective, tokenBudget);
      if (!snapshot) {
        toast.info("浏览器预览模式下无法继续目标。");
        return false;
      }
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      toast.success("目标已继续。");
      return true;
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      return false;
    }
  }, [mergeWorkbenchWithLocalCaches, selectedConversation, selectedDraftConversation]);

  // handleClearSessionGoal removes the current explicit session goal.
  // handleClearSessionGoal 移除当前显式会话目标。
  const handleClearSessionGoal = React.useCallback(async (): Promise<boolean> => {
    const activeConversationId = selectedDraftConversation ? null : selectedConversation?.id ?? null;
    if (!activeConversationId) {
      toast.error("请先选择一个已有会话。");
      return false;
    }
    try {
      const snapshot = await clearSessionGoal(activeConversationId);
      if (!snapshot) {
        toast.info("浏览器预览模式下无法清理目标。");
        return false;
      }
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      toast.success("目标已清理。");
      return true;
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      return false;
    }
  }, [mergeWorkbenchWithLocalCaches, selectedConversation, selectedDraftConversation]);

  // handleUpdateSessionGoalStatus updates the current goal lifecycle state.
  // handleUpdateSessionGoalStatus 更新当前目标生命周期状态。
  const handleUpdateSessionGoalStatus = React.useCallback(async (status: string): Promise<boolean> => {
    const activeConversationId = selectedDraftConversation ? null : selectedConversation?.id ?? null;
    if (!activeConversationId) {
      toast.error("请先选择一个已有会话。");
      return false;
    }
    try {
      const snapshot = await updateSessionGoalStatus(activeConversationId, status);
      if (!snapshot) {
        toast.info("浏览器预览模式下无法更新目标状态。");
        return false;
      }
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      return true;
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      return false;
    }
  }, [mergeWorkbenchWithLocalCaches, selectedConversation, selectedDraftConversation]);

  // handleSetSessionPlanMode persists the current conversation Plan-mode toggle.
  // handleSetSessionPlanMode 持久化当前会话的计划模式开关。
  const handleSetSessionPlanMode = React.useCallback(async (enabled: boolean): Promise<boolean> => {
    const activeConversationId = selectedDraftConversation ? null : selectedConversation?.id ?? null;
    if (!activeConversationId) {
      toast.error("请先创建或选择一个会话，再切换计划模式。");
      return false;
    }
    try {
      const snapshot = await setSessionPlanMode(activeConversationId, enabled);
      if (!snapshot) {
        toast.info("浏览器预览模式下无法切换计划模式。");
        return false;
      }
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      toast.success(enabled ? "计划模式已开启。" : "计划模式已关闭。");
      return true;
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      return false;
    }
  }, [mergeWorkbenchWithLocalCaches, selectedConversation, selectedDraftConversation]);

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
    handleEditPendingPrompt,
  };
}
