import { Button } from "@/ui-kit/primitives/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/ui-kit/primitives/dialog";
import { useI18n } from "@/i18n";

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
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent showCloseButton={!props.stopping} size="sm">
        <DialogHeader>
          <DialogTitle>{t("terminalStop.title")}</DialogTitle>
          <DialogDescription>
            {t("terminalStop.description")}
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
            {t("conversation.action.cancel")}
          </Button>
          <Button
            disabled={props.stopping}
            onClick={props.onConfirm}
            type="button"
            variant="destructive"
          >
            {props.stopping ? t("terminalStop.stopping") : t("terminalStop.confirm")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
