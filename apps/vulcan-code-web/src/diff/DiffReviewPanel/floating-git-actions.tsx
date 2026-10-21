import { FolderOpen, MoreHorizontal, Plus, RotateCcw, Undo2 } from "lucide-react";
import * as React from "react";
import { toast } from "@/ui-kit/primitives/toast";

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/ui-kit/primitives/alert-dialog";
import { Button, IconButton } from "@/ui-kit/primitives/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/ui-kit/primitives/dropdown-menu";
import { formatMessage, useI18n } from "@/i18n";
import type { DiffFile, DiffReviewSelection, WorkspaceGitFileActionKind } from "../types";
import { diffRevealRelativePath } from "./search";

// DiffReviewFloatingGitActionsProps describes the bottom floating Git action bar.
// DiffReviewFloatingGitActionsProps 描述底部浮动 Git 操作条。
export interface DiffReviewFloatingGitActionsProps {
  // Current review mode that decides the available Git actions.
  // 决定可用 Git 操作的当前审核模式。
  mode: DiffReviewSelection["mode"];
  // Current selected diff file for file-scoped actions.
  // 文件级操作使用的当前选中 diff 文件。
  selectedFile: DiffFile | null;
  // Number of changed files, named when every unstaged change is restored.
  // 变更文件数，在还原全部未暂存修改时说明。
  fileCount: number;
  // Whether buttons should be temporarily disabled.
  // 按钮是否应临时禁用。
  busy: boolean;
  // Dispatches one Git action to the owning workspace host.
  // 向所属工作区宿主派发一次 Git 操作。
  onAction: (action: WorkspaceGitFileActionKind, file?: DiffFile | null) => void | Promise<void>;
  // Shows the selected file in the system file manager.
  // 在系统文件管理器中显示所选文件。
  onRevealFile?: (file: DiffFile) => void;
}

// DiffReviewFloatingGitActions acts on the file the review shows: stage or unstage it, restore it after a
// confirmation, or show where it lives. Whole-repository actions sit behind the more menu, and restoring
// every file is confirmed as well because both restores discard work that cannot be recovered.
// DiffReviewFloatingGitActions 针对审核当前展示的文件操作：暂存或取消暂存、确认后还原，或显示其位置。整库操作位于更多菜单中；
// 还原全部同样需要确认，因为两种还原都会丢弃无法恢复的修改。
export function DiffReviewFloatingGitActions(props: DiffReviewFloatingGitActionsProps) {
  const { t } = useI18n();
  // Host state can lag one event; the local lock owns the complete action promise.
  // 宿主状态可能晚一个事件，本地锁持有完整动作 Promise。
  const pendingRef = React.useRef(false);
  const [pending, setPending] = React.useState(false);
  // confirming names the restore waiting for the user's confirmation.
  // confirming 指明等待用户确认的还原操作。
  const [confirming, setConfirming] = React.useState<"restoreFile" | "restoreAll" | null>(null);
  const busy = props.busy || pending;
  const file = props.selectedFile;
  const runAction = async (action: WorkspaceGitFileActionKind, target?: DiffFile | null) => {
    if (props.busy || pendingRef.current) {
      return;
    }
    pendingRef.current = true;
    setPending(true);
    try {
      await props.onAction(action, target);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      pendingRef.current = false;
      setPending(false);
    }
  };
  const reveal = file && props.onRevealFile ? (
    <IconButton aria-label={t("workspaceFiles.revealInFolder")} disabled={busy} onClick={() => props.onRevealFile?.(file)} size="sm" title={t("workspaceFiles.revealInFolder")}>
      <FolderOpen size={14} />
    </IconButton>
  ) : null;
  if (props.mode === "staged") {
    return (
      <div className="vk-float-actions">
        <Button disabled={busy || !file} onClick={() => void runAction("unstageFile", file)} size="sm" variant="primary">
          <Undo2 />
          {t("diffAction.unstageFile")}
        </Button>
        {reveal}
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <IconButton aria-label={t("diffAction.moreStaging")} disabled={busy} size="sm" title={t("diffAction.moreStaging")}>
              <MoreHorizontal size={14} />
            </IconButton>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" side="top" sideOffset={8}>
            <DropdownMenuItem onSelect={() => void runAction("unstageAll")}>
              <Undo2 size={14} />
              <span>{t("diffAction.unstageAll")}</span>
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    );
  }
  const fileName = file ? (diffRevealRelativePath(file) ?? file.key).split("/").pop() ?? file.key : "";
  return (
    <div className="vk-float-actions">
      <Button disabled={busy || !file} onClick={() => void runAction("stageFile", file)} size="sm" variant="primary">
        <Plus />
        {t("diffAction.stageFile")}
      </Button>
      <Button disabled={busy || !file} onClick={() => setConfirming("restoreFile")} size="sm" variant="ghost">
        <RotateCcw />
        {t("diffAction.restoreFile")}
      </Button>
      {reveal}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <IconButton aria-label={t("diffAction.moreStaging")} disabled={busy} size="sm" title={t("diffAction.moreStaging")}>
            <MoreHorizontal size={14} />
          </IconButton>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" side="top" sideOffset={8}>
          <DropdownMenuItem onSelect={() => void runAction("stageAll")}>
            <Plus size={14} />
            <span>{t("diffAction.stageAll")}</span>
          </DropdownMenuItem>
          <DropdownMenuItem onSelect={() => setConfirming("restoreAll")}>
            <RotateCcw size={14} />
            <span>{t("diffAction.restoreAll")}</span>
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <AlertDialog onOpenChange={(open) => { if (!open) setConfirming(null); }} open={confirming !== null}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {confirming === "restoreAll" ? t("diffAction.confirmRestoreAll") : formatMessage(t("diffAction.confirmRestoreFile"), { name: fileName })}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {confirming === "restoreAll"
                ? formatMessage(t("diffAction.confirmRestoreAllBody"), { count: props.fileCount })
                : t("diffAction.confirmRestoreFileBody")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("conversation.action.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => void runAction(confirming === "restoreAll" ? "restoreAll" : "restoreFile", confirming === "restoreAll" ? null : file)}
              variant="danger"
            >
              {confirming === "restoreAll" ? t("diffAction.restoreAll") : t("diffAction.restoreFile")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
