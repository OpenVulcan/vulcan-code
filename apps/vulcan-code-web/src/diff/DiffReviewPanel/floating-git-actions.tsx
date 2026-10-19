import { MoreHorizontal, Plus, RotateCcw, Undo2 } from "lucide-react";
import * as React from "react";
import { toast } from "@/ui-kit/primitives/toast";

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/ui-kit/primitives/dropdown-menu";
import { useI18n } from "@/i18n";
import type { DiffFile, DiffReviewSelection, WorkspaceGitFileActionKind } from "../types";
// DiffReviewFloatingGitActionsProps describes the bottom floating Git action bar.
// DiffReviewFloatingGitActionsProps 描述底部浮动 Git 操作条。
export interface DiffReviewFloatingGitActionsProps {
  // Current review mode that decides the available Git actions.
  // 决定可用 Git 操作的当前审核模式。
  mode: DiffReviewSelection["mode"];
  // Current selected diff file for file-scoped actions.
  // 文件级操作使用的当前选中 diff 文件。
  selectedFile: DiffFile | null;
  // Whether buttons should be temporarily disabled.
  // 按钮是否应临时禁用。
  busy: boolean;
  // Dispatches one Git action to the owning workspace host.
  // 向所属工作区宿主派发一次 Git 操作。
  onAction: (action: WorkspaceGitFileActionKind, file?: DiffFile | null) => void | Promise<void>;
}

// DiffReviewFloatingGitActions renders file-level Git actions over the review panel bottom edge.
// DiffReviewFloatingGitActions 在审核面板底部边缘渲染文件级 Git 操作。
export function DiffReviewFloatingGitActions(props: DiffReviewFloatingGitActionsProps) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  // Host state can lag one event; the local lock owns the complete action promise.
  // 宿主状态可能晚一个事件，本地锁持有完整动作 Promise。
  const pendingRef = React.useRef(false);
  const [pending, setPending] = React.useState(false);
  const busy = props.busy || pending;
  const selectedFileDisabled = busy || !props.selectedFile;
  const runAction = async (action: WorkspaceGitFileActionKind, file?: DiffFile | null) => {
    if (props.busy || pendingRef.current) {
      return;
    }
    pendingRef.current = true;
    setPending(true);
    try {
      await props.onAction(action, file);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      pendingRef.current = false;
      setPending(false);
    }
  };
  if (props.mode === "staged") {
    return (
      <div className="diff-review-floating-actions">
        <button
          className="diff-review-floating-action-button"
          disabled={busy}
          onClick={() => runAction("unstageAll")}
          type="button"
        >
          <Undo2 size={14} />
          <span>{t("diffAction.unstageAll")}</span>
        </button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button
              aria-label={t("diffAction.moreStaging")}
              className="diff-review-floating-action-button icon-only"
              disabled={busy}
              type="button"
            >
              <MoreHorizontal size={15} />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="center" className="diff-floating-action-menu" side="top" sideOffset={8}>
            <DropdownMenuItem
              disabled={selectedFileDisabled}
              onSelect={() => runAction("unstageFile", props.selectedFile)}
            >
              <Undo2 size={14} />
              <span>{t("diffAction.unstageFile")}</span>
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    );
  }
  return (
    <div className="diff-review-floating-actions">
      <button
        className="diff-review-floating-action-button"
        disabled={busy}
        onClick={() => runAction("restoreAll")}
        type="button"
      >
        <RotateCcw size={14} />
        <span>{t("diffAction.restoreAll")}</span>
      </button>
      <button
        className="diff-review-floating-action-button"
        disabled={busy}
        onClick={() => runAction("stageAll")}
        type="button"
      >
        <Plus size={14} />
        <span>{t("diffAction.stageAll")}</span>
      </button>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            aria-label={t("diffAction.moreStaging")}
            className="diff-review-floating-action-button icon-only"
            disabled={busy}
            type="button"
          >
            <MoreHorizontal size={15} />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="center" className="diff-floating-action-menu" side="top" sideOffset={8}>
          <DropdownMenuItem
            disabled={selectedFileDisabled}
            onSelect={() => runAction("stageFile", props.selectedFile)}
          >
            <Plus size={14} />
            <span>{t("diffAction.stageFile")}</span>
          </DropdownMenuItem>
          <DropdownMenuItem
            disabled={selectedFileDisabled}
            onSelect={() => runAction("restoreFile", props.selectedFile)}
          >
            <RotateCcw size={14} />
            <span>{t("diffAction.restoreFile")}</span>
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}

