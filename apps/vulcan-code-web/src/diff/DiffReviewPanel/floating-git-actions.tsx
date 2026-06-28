import { MoreHorizontal, Plus, RotateCcw, Undo2 } from "lucide-react";

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
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
  const selectedFileDisabled = props.busy || !props.selectedFile;
  const runAction = (action: WorkspaceGitFileActionKind, file?: DiffFile | null) => {
    if (props.busy) {
      return;
    }
    void props.onAction(action, file);
  };
  if (props.mode === "staged") {
    return (
      <div className="diff-review-floating-actions">
        <button
          className="diff-review-floating-action-button"
          disabled={props.busy}
          onClick={() => runAction("unstageAll")}
          type="button"
        >
          <Undo2 size={14} />
          <span>取消全部暂存</span>
        </button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button
              aria-label="更多暂存操作"
              className="diff-review-floating-action-button icon-only"
              disabled={props.busy}
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
              <span>取消当前文件暂存</span>
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
        disabled={props.busy}
        onClick={() => runAction("restoreAll")}
        type="button"
      >
        <RotateCcw size={14} />
        <span>还原全部</span>
      </button>
      <button
        className="diff-review-floating-action-button"
        disabled={props.busy}
        onClick={() => runAction("stageAll")}
        type="button"
      >
        <Plus size={14} />
        <span>暂存全部</span>
      </button>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            aria-label="更多暂存操作"
            className="diff-review-floating-action-button icon-only"
            disabled={props.busy}
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
            <span>暂存此文件</span>
          </DropdownMenuItem>
          <DropdownMenuItem
            disabled={selectedFileDisabled}
            onSelect={() => runAction("restoreFile", props.selectedFile)}
          >
            <RotateCcw size={14} />
            <span>还原此文件</span>
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}

