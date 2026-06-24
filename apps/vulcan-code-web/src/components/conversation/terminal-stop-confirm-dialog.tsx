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

// TerminalStopConfirmDialog renders the shared confirmation UI for force-stopping terminal sessions.
// TerminalStopConfirmDialog 渲染强制终止终端会话的共享确认界面。
export function TerminalStopConfirmDialog(props: {
  // Whether the confirmation dialog is visible.
  // 确认弹窗当前是否可见。
  open: boolean;
  // Human-readable terminal session name.
  // 用户可见的终端会话名称。
  sessionName: string;
  // Command line started by the terminal session.
  // 终端会话启动时使用的命令行。
  command: string;
  // Whether the stop request is currently running.
  // 终止请求当前是否正在执行。
  stopping: boolean;
  // Updates dialog visibility.
  // 更新弹窗可见状态。
  onOpenChange: (open: boolean) => void;
  // Confirms the force-stop action.
  // 确认执行强制终止动作。
  onConfirm: () => void;
}) {
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="terminal-stop-confirm-dialog" showCloseButton={!props.stopping}>
        <DialogHeader>
          <DialogTitle>确认强制终止终端会话？</DialogTitle>
          <DialogDescription>
            该操作会停止正在运行的终端进程，请确认当前会话不再需要继续输出。
          </DialogDescription>
        </DialogHeader>
        <div className="terminal-stop-confirm-body">
          <span>{props.sessionName}</span>
          <code>{props.command}</code>
        </div>
        <DialogFooter>
          <Button
            disabled={props.stopping}
            onClick={() => props.onOpenChange(false)}
            type="button"
            variant="outline"
          >
            取消
          </Button>
          <Button
            disabled={props.stopping}
            onClick={props.onConfirm}
            type="button"
            variant="destructive"
          >
            {props.stopping ? "正在终止" : "确认终止"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
