import React from "react";
import { AskQuestionDialog } from "@/components/ask/AskQuestionDialog";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  ProjectFolderPickerDialog,
  type ProjectFolderPickerDialogProps,
} from "@/components/project/folder-picker-dialog";
import {
  ArchiveProjectDialog,
  RenameDialog,
  type RenameTarget,
} from "@/components/project/project-action-dialogs";
import type { ProjectIconDialogTarget, ProjectIconSelection } from "@/components/project/project-icon-contract";
import {
  ProjectSettingsDialog,
  projectConversationCount,
  type ProjectSettingsDialogTarget,
} from "@/components/project/project-settings-dialog";
import type {
  PendingQuestionSnapshot,
  ProjectSummary,
  WorkbenchSnapshot,
} from "@/types";
import { useI18n } from "@/i18n";
import type {
  StopConversationRunAgentPromptItem,
  StopConversationRunChoice,
} from "@/components/workspace/workbench-layout/types";

// LazyProjectIconDialog prevents the complete icon catalogs from loading before the picker opens.
// LazyProjectIconDialog 防止完整图标目录在选择器打开前加载。
const LazyProjectIconDialog = React.lazy(() => import("@/components/project/project-icon-dialog").then((module) => ({ default: module.ProjectIconDialog })));

// WorkbenchDialogsProps groups globally mounted workbench dialogs and their callbacks.
// WorkbenchDialogsProps 汇总工作台全局挂载的对话框及其回调。
export interface WorkbenchDialogsProps {
  // Whether the project folder picker is currently open.
  // 项目目录选择器当前是否打开。
  projectFolderPickerOpen: boolean;
  // Loads directory information for the project folder picker.
  // 为项目目录选择器加载目录信息。
  onBrowseProjectDirectories: ProjectFolderPickerDialogProps["onBrowse"];
  // Confirms the selected project directory.
  // 确认选中的项目目录。
  onConfirmProjectFolder: ProjectFolderPickerDialogProps["onConfirm"];
  // Handles project folder picker visibility changes.
  // 处理项目目录选择器可见性变化。
  onProjectFolderPickerOpenChange: ProjectFolderPickerDialogProps["onOpenChange"];
  // Current rename dialog target.
  // 当前重命名对话框目标。
  renameTarget: RenameTarget | null;
  // Cancels the current rename flow.
  // 取消当前重命名流程。
  onCancelRename: () => void;
  // Confirms one project or conversation rename.
  // 确认一次项目或会话重命名。
  onConfirmRename: React.ComponentProps<typeof RenameDialog>["onConfirm"];
  // Current project icon dialog target.
  // 当前项目图标对话框目标。
  projectIconTarget: ProjectIconDialogTarget | null;
  // Cancels the project icon dialog.
  // 取消项目图标对话框。
  onCancelProjectIcon: () => void;
  // Confirms one project icon selection.
  // 确认一次项目图标选择。
  onConfirmProjectIcon: (project: ProjectSummary, icon: ProjectIconSelection) => void;
  // Resets one project icon override.
  // 还原一个项目图标覆盖。
  onResetProjectIcon: (project: ProjectSummary) => void;
  // Current project settings dialog target.
  // 当前项目设置对话框目标。
  projectSettingsTarget: ProjectSettingsDialogTarget | null;
  // Latest accepted settings invalidation revisions for the current project target.
  // 当前项目目标已接收的最新设置失效版本。
  projectSettingsRevision: React.ComponentProps<typeof ProjectSettingsDialog>["settingsRevision"];
  // Closes the project settings dialog.
  // 关闭项目设置对话框。
  onCancelProjectSettings: () => void;
  // Current workbench snapshot used by settings and counters.
  // 设置与计数使用的当前工作台快照。
  workbench: WorkbenchSnapshot;
  // Loads project Agent Skill overrides.
  // 加载项目 Agent Skill 覆盖。
  onLoadAgentSkillOverrides: React.ComponentProps<typeof ProjectSettingsDialog>["onLoadAgentSkillOverrides"];
  // Loads a project VMM draft.
  // 加载项目 VMM 草稿。
  onLoadVmmDraft: React.ComponentProps<typeof ProjectSettingsDialog>["onLoadVmmDraft"];
  // Saves a project VMM draft.
  // 保存项目 VMM 草稿。
  onSaveVmmDraft: React.ComponentProps<typeof ProjectSettingsDialog>["onSaveVmmDraft"];
  // Saves one project Agent Skill override.
  // 保存单个项目 Agent Skill 覆盖。
  onSetAgentSkillState: React.ComponentProps<typeof ProjectSettingsDialog>["onSetAgentSkillState"];
  // Loads a project attachment cleanup preview.
  // 加载项目附件清理预览。
  onPreviewAttachmentCleanup: React.ComponentProps<typeof ProjectSettingsDialog>["onPreviewAttachmentCleanup"];
  // Executes a project attachment cleanup.
  // 执行项目附件清理。
  onExecuteAttachmentCleanup: React.ComponentProps<typeof ProjectSettingsDialog>["onExecuteAttachmentCleanup"];
  // Project targeted by archive-all-conversations.
  // 全部归档对话目标项目。
  archiveProjectTarget: ProjectSummary | null;
  // Whether the archive project action is running.
  // 项目归档动作是否正在执行。
  archiveProjectSubmitting: boolean;
  // Cancels the archive project dialog.
  // 取消项目归档对话框。
  onCancelArchiveProject: () => void;
  // Confirms the archive project action.
  // 确认项目归档动作。
  onConfirmArchiveProject: () => void;
  // Project targeted by archive-and-remove.
  // 归档并移除的目标项目。
  removeProjectTarget: ProjectSummary | null;
  // Whether the remove project action is running.
  // 项目移除动作是否正在执行。
  removeProjectSubmitting: boolean;
  // Cancels the remove project dialog.
  // 取消项目移除对话框。
  onCancelRemoveProject: () => void;
  // Confirms the remove project action.
  // 确认项目移除动作。
  onConfirmRemoveProject: () => void;
  // Current pending ask-tool request.
  // 当前待处理的 ask 工具请求。
  pendingQuestion: PendingQuestionSnapshot | null;
  // Running agents that require a main-stop decision.
  // 需要主会话停止决策的运行中智能体。
  stopConversationAgents: StopConversationRunAgentPromptItem[] | null;
  // Cancels the main conversation stop flow.
  // 取消主会话停止流程。
  onCancelStopConversationRun: () => void;
  // Confirms how the main stop flow should handle running agents.
  // 确认主会话停止流程应如何处理运行中智能体。
  onConfirmStopConversationRun: (choice: StopConversationRunChoice) => void;
  // Rejects the pending ask-tool request.
  // 拒绝当前待处理的 ask 工具请求。
  onRejectPendingQuestion: React.ComponentProps<typeof AskQuestionDialog>["onReject"];
  // Submits answers for the pending ask-tool request.
  // 提交当前待处理 ask 工具请求的答案。
  onSubmitPendingQuestion: React.ComponentProps<typeof AskQuestionDialog>["onSubmit"];
}

// WorkbenchDialogs renders the globally mounted modal layer for the workbench.
// WorkbenchDialogs 渲染工作台全局挂载的模态层。
export function WorkbenchDialogs(props: WorkbenchDialogsProps) {
  return (
    <>
      <ProjectFolderPickerDialog
        open={props.projectFolderPickerOpen}
        onBrowse={props.onBrowseProjectDirectories}
        onConfirm={props.onConfirmProjectFolder}
        onOpenChange={props.onProjectFolderPickerOpenChange}
      />
      <RenameDialog
        target={props.renameTarget}
        onCancel={props.onCancelRename}
        onConfirm={props.onConfirmRename}
      />
      {props.projectIconTarget ? (
        <React.Suspense fallback={null}>
          <LazyProjectIconDialog
            target={props.projectIconTarget}
            onCancel={props.onCancelProjectIcon}
            onConfirm={props.onConfirmProjectIcon}
            onReset={props.onResetProjectIcon}
          />
        </React.Suspense>
      ) : null}
      <ProjectSettingsDialog
        target={props.projectSettingsTarget}
        settingsRevision={props.projectSettingsRevision}
        workbench={props.workbench}
        onCancel={props.onCancelProjectSettings}
        onExecuteAttachmentCleanup={props.onExecuteAttachmentCleanup}
        onLoadAgentSkillOverrides={props.onLoadAgentSkillOverrides}
        onLoadVmmDraft={props.onLoadVmmDraft}
        onPreviewAttachmentCleanup={props.onPreviewAttachmentCleanup}
        onSaveVmmDraft={props.onSaveVmmDraft}
        onSetAgentSkillState={props.onSetAgentSkillState}
      />
      <ArchiveProjectDialog
        busy={props.archiveProjectSubmitting}
        conversationCount={props.archiveProjectTarget ? projectConversationCount(props.workbench, props.archiveProjectTarget.id) : 0}
        mode="archive"
        project={props.archiveProjectTarget}
        onCancel={props.onCancelArchiveProject}
        onConfirm={props.onConfirmArchiveProject}
      />
      <ArchiveProjectDialog
        busy={props.removeProjectSubmitting}
        conversationCount={props.removeProjectTarget ? projectConversationCount(props.workbench, props.removeProjectTarget.id) : 0}
        mode="remove"
        project={props.removeProjectTarget}
        onCancel={props.onCancelRemoveProject}
        onConfirm={props.onConfirmRemoveProject}
      />
      <AskQuestionDialog
        request={props.pendingQuestion}
        onReject={props.onRejectPendingQuestion}
        onSubmit={props.onSubmitPendingQuestion}
      />
      <StopConversationRunDialog
        agents={props.stopConversationAgents}
        onCancel={props.onCancelStopConversationRun}
        onConfirm={props.onConfirmStopConversationRun}
      />
    </>
  );
}

// StopConversationRunDialog asks how to handle running agents while stopping the main conversation.
// StopConversationRunDialog 询问停止主会话时如何处理运行中的智能体。
function StopConversationRunDialog(props: {
  // Running agents reported by the backend preflight.
  // 后端预检报告的运行中智能体。
  agents: StopConversationRunAgentPromptItem[] | null;
  // Cancels the stop flow without touching runtime state.
  // 取消停止流程且不修改运行态。
  onCancel: () => void;
  // Confirms the chosen stop behavior.
  // 确认选中的停止行为。
  onConfirm: (choice: StopConversationRunChoice) => void;
}) {
  const { t } = useI18n();
  const agents = props.agents ?? [];
  const open = Boolean(props.agents);
  return (
    <Dialog open={open} onOpenChange={(nextOpen) => {
      if (!nextOpen) {
        props.onCancel();
      }
    }}>
      <DialogContent size="sm">
        <DialogHeader>
          <DialogTitle>{t("conversation.stopWithAgents.title")}</DialogTitle>
          <DialogDescription>
            {t("conversation.stopWithAgents.description").replace("{count}", String(agents.length))}
          </DialogDescription>
        </DialogHeader>
        <div className="grid gap-2 rounded-lg border bg-muted/30 p-3 text-sm">
          {agents.map((agent) => (
            <div key={agent.taskId} className="flex min-w-0 items-center justify-between gap-3">
              <span className="min-w-0 truncate font-medium">{agent.label}</span>
              <code className="shrink-0 rounded bg-background px-1.5 py-0.5 text-[11px] text-muted-foreground">{agent.taskId.slice(0, 8)}</code>
            </div>
          ))}
        </div>
        <DialogFooter>
          <Button type="button" variant="ghost" onClick={props.onCancel}>
            {t("conversation.action.cancel")}
          </Button>
          <Button type="button" variant="outline" onClick={() => props.onConfirm("main_only")}>
            {t("conversation.stopWithAgents.mainOnly")}
          </Button>
          <Button type="button" variant="destructive" onClick={() => props.onConfirm("main_and_subagents")}>
            {t("conversation.stopWithAgents.all")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
