import React from "react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { useI18n } from "@/i18n";
import type { ProjectSummary } from "@/types";

// RenameTarget describes one pending rename dialog target.
// RenameTarget 描述一个待处理的重命名对话框目标。
export interface RenameTarget {
  // Target kind used to route the final rename command.
  // 用于路由最终重命名命令的目标类型。
  kind: "project" | "conversation";
  // Stable target identifier.
  // 稳定目标标识。
  id: string;
  // Current display name.
  // 当前显示名称。
  name: string;
}

// RenameDialog renders a controlled project or conversation rename form.
// RenameDialog 渲染受控的项目或会话重命名表单。
export function RenameDialog(props: {
  // Current rename target, or null when the dialog is closed.
  // 当前重命名目标；为 null 时对话框关闭。
  target: RenameTarget | null;
  // Cancels the rename flow.
  // 取消重命名流程。
  onCancel: () => void;
  // Confirms the edited name for the current target.
  // 确认当前目标的编辑后名称。
  onConfirm: (target: RenameTarget, name: string) => void;
}) {
  const { t } = useI18n();
  // value stores the pending display name edited by the user.
  // value 保存用户正在编辑的显示名称。
  const [value, setValue] = React.useState("");

  React.useEffect(() => {
    setValue(props.target?.name ?? "");
  }, [props.target]);

  const title = props.target?.kind === "project" ? t("projectDialog.renameProjectTitle") : t("projectDialog.renameConversationTitle");
  const description = props.target?.kind === "project"
    ? t("projectDialog.renameProjectDescription")
    : t("projectDialog.renameConversationDescription");

  return (
    <Dialog open={Boolean(props.target)} onOpenChange={(open) => !open && props.onCancel()}>
      <DialogContent size="sm">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        <form
          className="dialog-form"
          onSubmit={(event) => {
            event.preventDefault();
            if (props.target) {
              props.onConfirm(props.target, value);
            }
          }}
        >
          <Input
            autoFocus
            value={value}
            onChange={(event) => setValue(event.target.value)}
            placeholder={t("projectDialog.renamePlaceholder")}
          />
          <DialogFooter>
            <Button type="button" variant="ghost" onClick={props.onCancel}>{t("conversation.action.cancel")}</Button>
            <Button type="submit">{t("projectDialog.confirm")}</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

// ArchiveProjectDialog confirms bulk conversation archiving for one project.
// ArchiveProjectDialog 确认单个项目下的对话批量归档。
export function ArchiveProjectDialog(props: {
  // Whether the confirmation action is currently running.
  // 确认动作当前是否正在执行。
  busy: boolean;
  // Confirmation mode for archive-only or archive-and-remove.
  // 仅归档或归档并移除的确认模式。
  mode: "archive" | "remove";
  // Project being archived or removed.
  // 正在归档或移除的项目。
  project: ProjectSummary | null;
  // Number of conversations affected by the action.
  // 本次动作影响的会话数量。
  conversationCount: number;
  // Cancels the confirmation dialog.
  // 取消确认对话框。
  onCancel: () => void;
  // Confirms the archive or remove action.
  // 确认归档或移除动作。
  onConfirm: () => void;
}) {
  const { t } = useI18n();
  const title = props.mode === "remove"
    ? t("projectDialog.archiveRemoveTitle")
    : t("projectDialog.archiveTitle").replace("{count}", String(props.conversationCount));
  const description = props.mode === "remove"
    ? t("projectDialog.archiveRemoveDescription").replace("{project}", props.project?.name ?? t("projectDialog.currentProject")).replace("{count}", String(props.conversationCount))
    : t("projectDialog.archiveDescription").replace("{project}", props.project?.name ?? t("projectDialog.currentProject"));
  const confirmLabel = props.mode === "remove" ? t("projectDialog.archiveAndRemove") : t("projectDialog.archiveAll");
  const busyLabel = props.mode === "remove" ? t("projectDialog.removing") : t("projectDialog.archiving");

  return (
    <Dialog open={Boolean(props.project)} onOpenChange={(open) => !props.busy && !open && props.onCancel()}>
      <DialogContent
        className="archive-dialog"
        onEscapeKeyDown={(event) => {
          if (props.busy) {
            event.preventDefault();
          }
        }}
        onInteractOutside={(event) => {
          if (props.busy) {
            event.preventDefault();
          }
        }}
        showCloseButton={!props.busy}
      >
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>
            {description}
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button disabled={props.busy} type="button" variant="ghost" onClick={props.onCancel}>{t("conversation.action.cancel")}</Button>
          <Button disabled={props.busy} type="button" variant="destructive" onClick={props.onConfirm}>
            {props.busy ? <Spinner className="size-4" /> : null}
            {props.busy ? busyLabel : confirmLabel}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// restoreProjectDisplayName resolves the default project name used when rename input is empty.
// restoreProjectDisplayName 解析重命名输入为空时使用的默认项目名称。
export function restoreProjectDisplayName(project: ProjectSummary | null, target: RenameTarget): string {
  const folderName = project ? projectFolderName(project.path) : "";
  if (folderName) {
    return folderName;
  }
  const currentName = target.name.trim();
  if (currentName && !isPathSeparatorOnlyName(currentName)) {
    return currentName;
  }
  return target.id;
}

// projectFolderName extracts the folder name from a platform path.
// projectFolderName 从平台路径中提取文件夹名称。
function projectFolderName(path: string): string {
  return path.split(/[\\/]/).map((segment) => segment.trim()).filter(Boolean).pop() ?? "";
}

// isPathSeparatorOnlyName detects invalid slash-only names from older reset attempts.
// isPathSeparatorOnlyName 检测旧还原尝试产生的仅路径分隔符名称。
function isPathSeparatorOnlyName(name: string): boolean {
  return name.length > 0 && /^[\\/]+$/.test(name);
}
