import React from "react";
import { AskQuestionDialog } from "@/components/ask/AskQuestionDialog";
import {
  ProjectFolderPickerDialog,
  type ProjectFolderPickerDialogProps,
} from "@/components/project/folder-picker-dialog";
import {
  ArchiveProjectDialog,
  RenameDialog,
  type RenameTarget,
} from "@/components/project/project-action-dialogs";
import {
  ProjectIconDialog,
  type ProjectIconDialogTarget,
  type ProjectIconSelection,
} from "@/components/project/project-icons";
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
      <ProjectIconDialog
        target={props.projectIconTarget}
        onCancel={props.onCancelProjectIcon}
        onConfirm={props.onConfirmProjectIcon}
        onReset={props.onResetProjectIcon}
      />
      <ProjectSettingsDialog
        target={props.projectSettingsTarget}
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
    </>
  );
}
