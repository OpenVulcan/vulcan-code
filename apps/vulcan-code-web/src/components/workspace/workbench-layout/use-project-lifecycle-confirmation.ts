import React from "react";
import { toast } from "sonner";
import { projectConversationCount } from "@/components/project/project-settings-utils";
import { clearProjectTerminalInputBuffers } from "@/components/project/terminal/project-terminal-input-buffer";
import { formatWorkbenchTranslation } from "@/components/workspace/workbench-layout/types";
import type { TranslationKey } from "@/i18n";
import { archiveProjectConversations, removeProject } from "@/service/project-actions-service";
import type { ProjectSummary, WorkbenchSnapshot } from "@/types";

// ProjectLifecycleConfirmationOptions supplies the dialog targets and snapshot appliers owned by the workbench.
// ProjectLifecycleConfirmationOptions 提供工作台持有的弹窗目标与快照应用器。
export interface ProjectLifecycleConfirmationOptions {
  // Translates fixed workbench messages in the active locale.
  // 以当前语言翻译固定工作台消息。
  translate: (key: TranslationKey) => string;
  // Current workbench used to capture the exact child conversations before the RPC commits.
  // 当前工作台，用于在 RPC 提交前捕获精确的子会话集合。
  workbenchState: WorkbenchSnapshot;
  // Project waiting for bulk archive confirmation.
  // 正在等待批量归档确认的项目。
  archiveProjectTarget: ProjectSummary | null;
  // Opens or closes the bulk archive dialog.
  // 打开或关闭批量归档弹窗。
  setArchiveProjectTarget: React.Dispatch<React.SetStateAction<ProjectSummary | null>>;
  // Project waiting for removal confirmation.
  // 正在等待移除确认的项目。
  removeProjectTarget: ProjectSummary | null;
  // Opens or closes the removal dialog.
  // 打开或关闭移除弹窗。
  setRemoveProjectTarget: React.Dispatch<React.SetStateAction<ProjectSummary | null>>;
  // Merges an accepted service snapshot; returns false when the revision gate rejects it.
  // 合并已接受的服务快照；版本闸门拒绝时返回 false。
  applyProjectActionSnapshot: (snapshot: WorkbenchSnapshot) => boolean;
  // Applies only the lifecycle fields of a rejected snapshot; returns false when nothing converged.
  // 仅应用被拒绝快照中的生命周期字段；未收敛时返回 false。
  applyLifecycleResponseSnapshot: (snapshot: WorkbenchSnapshot, conversationIds: string[], removedProjectId?: string) => boolean;
}

// ProjectLifecycleConfirmation exposes the busy state and the confirm/cancel handlers of both project dialogs.
// ProjectLifecycleConfirmation 暴露两个项目弹窗的忙碌状态与确认、取消处理器。
export interface ProjectLifecycleConfirmation {
  // Whether a bulk archive request is running.
  // 批量归档请求是否正在进行。
  archiveProjectSubmitting: boolean;
  // Whether an archive-then-remove request is running.
  // 先归档再移除的请求是否正在进行。
  removeProjectSubmitting: boolean;
  // Archives every conversation of the archive target once, even when confirmed repeatedly.
  // 对归档目标的全部会话只执行一次归档，即使重复确认。
  handleArchiveProjectConfirm: () => Promise<void>;
  // Archives conversations and removes the removal target once, even when confirmed repeatedly.
  // 对移除目标只执行一次归档并移除，即使重复确认。
  handleRemoveProjectConfirm: () => Promise<void>;
  // Closes the archive dialog unless its request is running.
  // 在请求未进行时关闭归档弹窗。
  handleCancelArchiveProject: () => void;
  // Closes the removal dialog unless its request is running.
  // 在请求未进行时关闭移除弹窗。
  handleCancelRemoveProject: () => void;
}

// useProjectLifecycleConfirmation owns the duplicate-request locks of the project archive and removal dialogs.
// useProjectLifecycleConfirmation 持有项目归档与移除弹窗的重复请求锁。
// Parameters: options supplies dialog targets, translation and snapshot appliers.
// 参数：options 提供弹窗目标、翻译与快照应用器。
// Returns: busy flags plus confirm and cancel handlers.
// 返回值：忙碌标记以及确认与取消处理器。
export function useProjectLifecycleConfirmation(options: ProjectLifecycleConfirmationOptions): ProjectLifecycleConfirmation {
  const {
    translate,
    workbenchState,
    archiveProjectTarget,
    setArchiveProjectTarget,
    removeProjectTarget,
    setRemoveProjectTarget,
    applyProjectActionSnapshot,
    applyLifecycleResponseSnapshot,
  } = options;
  // archiveProjectSubmitting tracks a bulk archive request in the confirmation dialog.
  // archiveProjectSubmitting 跟踪批量归档确认弹窗中的请求状态。
  const [archiveProjectSubmitting, setArchiveProjectSubmitting] = React.useState(false);
  // archiveProjectInFlightRef excludes synchronous duplicate bulk lifecycle requests.
  // archiveProjectInFlightRef 排除同步重复的批量生命周期请求。
  const archiveProjectInFlightRef = React.useRef(false);
  // removeProjectSubmitting tracks an archive-then-remove project request.
  // removeProjectSubmitting 跟踪先归档再移除项目的请求状态。
  const [removeProjectSubmitting, setRemoveProjectSubmitting] = React.useState(false);
  // removeProjectInFlightRef locks destructive removal before React publishes the busy state.
  // removeProjectInFlightRef 在 React 发布忙碌状态前锁定破坏性移除。
  const removeProjectInFlightRef = React.useRef(false);

  // handleArchiveProjectConfirm archives all conversations under the selected project.
  // handleArchiveProjectConfirm 归档选定项目下的全部对话。
  const handleArchiveProjectConfirm = React.useCallback(async () => {
    if (!archiveProjectTarget || archiveProjectInFlightRef.current) {
      return;
    }
    archiveProjectInFlightRef.current = true;
    setArchiveProjectSubmitting(true);
    try {
      // archivedConversationIds captures the exact visible child lifecycle set before the RPC commits.
      // archivedConversationIds 捕获 RPC 提交前的精确可见子生命周期集合。
      const archivedConversationIds = workbenchState.conversations
        .filter((conversation) => conversation.projectId === archiveProjectTarget.id)
        .map((conversation) => conversation.id);
      const snapshot = await archiveProjectConversations(archiveProjectTarget.id);
      if (snapshot) {
        const snapshotApplied = applyProjectActionSnapshot(snapshot);
        if (!snapshotApplied && !applyLifecycleResponseSnapshot(snapshot, archivedConversationIds)) {
          throw new Error(translate("workspace.lifecycle.batchArchiveNotConverged"));
        }
      } else {
        throw new Error(translate("workspace.lifecycle.batchArchiveNoSnapshot"));
      }
      toast.success(formatWorkbenchTranslation(translate, "workbenchConversation.archivedCount", {
        count: String(projectConversationCount(workbenchState, archiveProjectTarget.id)),
      }));
      setArchiveProjectTarget(null);
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    } finally {
      archiveProjectInFlightRef.current = false;
      setArchiveProjectSubmitting(false);
    }
  }, [applyLifecycleResponseSnapshot, applyProjectActionSnapshot, archiveProjectSubmitting, archiveProjectTarget, setArchiveProjectTarget, translate, workbenchState]);

  // handleRemoveProjectConfirm archives conversations and removes the selected project.
  // handleRemoveProjectConfirm 归档对话并移除选中的项目。
  const handleRemoveProjectConfirm = React.useCallback(async () => {
    if (!removeProjectTarget || removeProjectInFlightRef.current) {
      return;
    }
    removeProjectInFlightRef.current = true;
    setRemoveProjectSubmitting(true);
    try {
      // archivedConversationIds captures the exact visible child lifecycle set before project removal commits.
      // archivedConversationIds 捕获项目移除提交前的精确可见子生命周期集合。
      const archivedConversationIds = workbenchState.conversations
        .filter((conversation) => conversation.projectId === removeProjectTarget.id)
        .map((conversation) => conversation.id);
      const snapshot = await removeProject(removeProjectTarget.id);
      if (snapshot) {
        const snapshotApplied = applyProjectActionSnapshot(snapshot);
        if (!snapshotApplied && !applyLifecycleResponseSnapshot(
          snapshot,
          archivedConversationIds,
          removeProjectTarget.id,
        )) {
          throw new Error(translate("workspace.lifecycle.projectRemoveNotConverged"));
        }
      } else {
        throw new Error(translate("workspace.lifecycle.projectRemoveNoSnapshot"));
      }
      clearProjectTerminalInputBuffers(removeProjectTarget.id);
      toast.success(formatWorkbenchTranslation(translate, "workbenchConversation.archivedAndRemovedProject", { project: removeProjectTarget.name }));
      setRemoveProjectTarget(null);
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    } finally {
      removeProjectInFlightRef.current = false;
      setRemoveProjectSubmitting(false);
    }
  }, [applyLifecycleResponseSnapshot, applyProjectActionSnapshot, removeProjectSubmitting, removeProjectTarget, setRemoveProjectTarget, translate, workbenchState]);

  // handleCancelArchiveProject keeps the dialog open while its request may still commit.
  // handleCancelArchiveProject 在请求仍可能提交时保持弹窗打开。
  const handleCancelArchiveProject = () => {
    if (!archiveProjectSubmitting && !archiveProjectInFlightRef.current) {
      setArchiveProjectTarget(null);
    }
  };
  // handleCancelRemoveProject keeps the dialog open while its destructive request may still commit.
  // handleCancelRemoveProject 在破坏性请求仍可能提交时保持弹窗打开。
  const handleCancelRemoveProject = () => {
    if (!removeProjectSubmitting && !removeProjectInFlightRef.current) {
      setRemoveProjectTarget(null);
    }
  };

  return {
    archiveProjectSubmitting,
    removeProjectSubmitting,
    handleArchiveProjectConfirm,
    handleRemoveProjectConfirm,
    handleCancelArchiveProject,
    handleCancelRemoveProject,
  };
}
