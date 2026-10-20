import React from "react";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent,
  AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/ui-kit/primitives/alert-dialog";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { readElectronLocalBrowserBridge } from "@/host-shell/electron-bridge";
import { useI18n } from "@/i18n";

// LocalBrowserCloseDialog confirms closing one conversation's native pages using the shared application modal.
// LocalBrowserCloseDialog 使用共享应用模态框确认关闭一个会话的原生页面。
// ownerSessionId is fixed for this mount; callbacks dismiss or remove the owning tab only after successful closure.
// ownerSessionId 在本次挂载期间固定；回调取消弹窗，或在成功关闭后移除所属页签。
export function LocalBrowserCloseDialog(props: {
  // Exact conversation whose pages the user requested to close.
  // 用户要求关闭页面的精确会话。
  ownerSessionId: string;
  // Dismisses without destroying any page.
  // 取消弹窗，不销毁任何页面。
  onCancel: () => void;
  // Removes the panel after the host confirms resource destruction.
  // 宿主确认资源已销毁后移除面板。
  onClosed: () => void;
}) {
  // t uses the current application language instead of the native operating-system dialog.
  // t 使用当前应用语言，而非操作系统原生弹窗。
  const { t } = useI18n();
  // bridge is the native capability fixed when this confirmation is opened.
  // bridge 是打开本次确认时固定的原生能力。
  const [bridge] = React.useState(readElectronLocalBrowserBridge);
  // count is unknown until the host supplies a snapshot owned by this exact conversation.
  // 宿主提供归属精确匹配的会话快照前，count 为未知。
  const [count, setCount] = React.useState<number | null>(null);
  // error retains snapshot or destruction failures in the dialog for inspection and retry.
  // error 在弹窗中保留快照或销毁失败，便于查看与重试。
  const [error, setError] = React.useState("");
  // closing disables controls, while pending synchronously rejects repeated confirmation clicks.
  // closing 禁用控件，pending 则同步拒绝重复确认点击。
  const [closing, setClosing] = React.useState(false);
  const pending = React.useRef(false);
  // mounted prevents a delayed host response from affecting a different conversation after navigation.
  // mounted 防止导航后迟到的宿主响应影响其他会话。
  const mounted = React.useRef(true);
  React.useEffect(() => {
    mounted.current = true;
    if (!bridge) {
      setError("Local browser is unavailable.");
      return () => { mounted.current = false; };
    }
    void bridge.snapshot().then((snapshot) => {
      if (!mounted.current) return;
      if (snapshot.ownerSessionId !== props.ownerSessionId) throw new Error("Local browser conversation changed.");
      setCount(snapshot.tabs.length);
    }).catch((cause: unknown) => {
      if (mounted.current) setError(cause instanceof Error ? cause.message : String(cause));
    });
    return () => { mounted.current = false; };
  }, [bridge, props.ownerSessionId]);

  // confirm closes only the captured owner; returns after destruction or exposes a retryable UI error.
  // confirm 仅关闭捕获的会话；销毁后返回，失败则显示可重试的界面错误。
  const confirm = async (): Promise<void> => {
    if (!bridge || count === null || pending.current) return;
    pending.current = true;
    setClosing(true);
    setError("");
    try {
      if (!await bridge.closeOwner(props.ownerSessionId, true)) throw new Error("Browser close was not completed.");
      if (mounted.current) props.onClosed();
    } catch (cause) {
      if (mounted.current) setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      pending.current = false;
      if (mounted.current) setClosing(false);
    }
  };
  return <AlertDialog open onOpenChange={(open) => { if (!open && !pending.current) props.onCancel(); }}>
    <AlertDialogContent onEscapeKeyDown={(event) => { if (pending.current) event.preventDefault(); }}>
      <AlertDialogHeader>
        <AlertDialogTitle>{t("browser.closeBrowser2")}</AlertDialogTitle>
        <AlertDialogDescription>{t("browser.closeLocalPages").replace("{count}", count === null ? "…" : String(count))}</AlertDialogDescription>
      </AlertDialogHeader>
      {count === null && !error ? <StreamPlaceholder /> : null}
      {error ? <p role="alert" className="vk-field-error">{error}</p> : null}
      <AlertDialogFooter>
        <AlertDialogCancel disabled={closing}>{t("browser.cancel")}</AlertDialogCancel>
        <AlertDialogAction disabled={closing || count === null} onClick={(event) => { event.preventDefault(); void confirm(); }}>
          {closing ? t("browser.closing") : t("browser.confirmClose")}
        </AlertDialogAction>
      </AlertDialogFooter>
    </AlertDialogContent>
  </AlertDialog>;
}
