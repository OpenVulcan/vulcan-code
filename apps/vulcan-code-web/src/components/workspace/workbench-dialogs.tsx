import React from "react";
import { RenderErrorBoundary } from "@/app/render-error-boundary";
import { AskQuestionDialog } from "@/components/ask/ask-question-dialog";
import { Button } from "@/ui-kit/primitives/button";
import { InlineStreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/ui-kit/primitives/dialog";
import type { ProjectFolderPickerDialogProps } from "@/components/project/folder-picker-dialog";
import {
  ArchiveProjectDialog,
  RenameDialog,
  type RenameTarget,
} from "@/components/project/project-action-dialogs";
import type { ProjectIconDialogTarget, ProjectIconSelection } from "@/components/project/project-icon-contract";
import { projectConversationCount } from "@/components/project/project-settings-utils";
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

/** Loads directory browsing only after opening the picker, retaining a closable recovery dialog on failure.
 * 打开选择器后才加载目录浏览，失败时保留可以关闭及重试的弹窗。
 * Parameters are the folder picker's unchanged callbacks and state. Returns its deferred dialog.
 * 参数为目录选择器原有回调和状态。返回按需加载的弹窗。
 */
function DeferredProjectFolderPicker(props: ProjectFolderPickerDialogProps) {
  // Locale remains available even when the picker module cannot be fetched.
  // 即使无法获取选择器模块，仍可使用当前语言。
  const { t } = useI18n();
  // Each explicit retry discards the rejected lazy instance.
  // 每次明确重试都丢弃已拒绝的延迟实例。
  const [attempt, setAttempt] = React.useState(0);
  // Closing unmounts this owner, so reopening also starts a fresh loading attempt.
  // 关闭后卸载当前所有者，重新打开也会重新尝试加载。
  const LazyPicker = React.useMemo(() => React.lazy(() => import("@/components/project/folder-picker-dialog").then((module) => ({ default: module.ProjectFolderPickerDialog }))), [attempt]);
  // recovery keeps loading and import errors inside the requested dialog, without losing workbench navigation.
  // recovery 将加载及导入错误限制在当前弹窗内，不影响工作台导航。
  const recovery = (error?: Error) => <Dialog open={props.open} onOpenChange={props.onOpenChange}>
    <DialogContent size="md">
      <DialogHeader><DialogTitle>{props.title ?? t("projectFolderPicker.title")}</DialogTitle>
        <DialogDescription>{error ? t("workspace.dialogs.unableToLoadThe") : <InlineStreamPlaceholder label={t("workspaceFiles.loading")} />}</DialogDescription>
      </DialogHeader>
      {error ? <details><summary>{t("toolPresentation.output.errorDetails")}</summary><pre className="vk-wrap-pre">{error.message}</pre></details> : null}
      <DialogFooter>
        <Button variant="outline" onClick={() => props.onOpenChange(false)}>{t("conversation.action.cancel")}</Button>
        {error ? <Button onClick={() => setAttempt((value) => value + 1)}>{t("toolPresentation.output.retryLoad")}</Button> : null}
      </DialogFooter>
    </DialogContent>
  </Dialog>;
  return <RenderErrorBoundary key={attempt} fallback={recovery}>
    <React.Suspense fallback={recovery()}><LazyPicker {...props} /></React.Suspense>
  </RenderErrorBoundary>;
}

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
  onConfirmProjectIcon: (project: ProjectSummary, icon: ProjectIconSelection) => Promise<void>;
  // Resets one project icon override.
  // 还原一个项目图标覆盖。
  onResetProjectIcon: (project: ProjectSummary) => Promise<void>;
  // Current workbench snapshot used by settings and counters.
  // 设置与计数使用的当前工作台快照。
  workbench: WorkbenchSnapshot;
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
  // Child agent label of the pending request, when it is not the conversation's own.
  // 待处理请求不属于会话自身时的子代理标签。
  pendingQuestionRequester?: string | null;
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
  // Stores or declines the values of a pending environment value request.
  // 存储或拒绝待处理环境变量取值请求的值。
  onSubmitPendingEnvironment: React.ComponentProps<typeof AskQuestionDialog>["onSubmitEnvironment"];
}

// WorkbenchDialogs renders the globally mounted modal layer for the workbench.
// WorkbenchDialogs 渲染工作台全局挂载的模态层。
export function WorkbenchDialogs(props: WorkbenchDialogsProps) {
  // projectPaths lets the folder picker offer the folders that already hold projects.
  // projectPaths 让目录选择器提供已有项目所在的目录。
  const projectPaths = React.useMemo(() => props.workbench.projects.map((project) => project.path), [props.workbench.projects]);
  return (
    <>
      {props.projectFolderPickerOpen ? <DeferredProjectFolderPicker
        open={props.projectFolderPickerOpen}
        projectPaths={projectPaths}
        onBrowse={props.onBrowseProjectDirectories}
        onConfirm={props.onConfirmProjectFolder}
        onOpenChange={props.onProjectFolderPickerOpenChange}
      /> : null}
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
        requester={props.pendingQuestionRequester}
        onReject={props.onRejectPendingQuestion}
        onSubmit={props.onSubmitPendingQuestion}
        onSubmitEnvironment={props.onSubmitPendingEnvironment}
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
        <ul className="vk-agent-list">
          {agents.map((agent) => (
            <li key={agent.taskId}>
              <strong>{agent.label}</strong>
              <code>{agent.taskId.slice(0, 8)}</code>
            </li>
          ))}
        </ul>
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
