import React from "react";
import { Bot, Lock, LockOpen } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { useI18n } from "@/i18n";
import { dedicatedMemoryLocked, type DedicatedMemorySelection } from "@/service/dedicated-memory";
import { setDedicatedMemoryLock } from "@/service/dedicated-session-service";
import { dedicatedErrorMessage } from "./dedicated-errors";
import { AssistantConvertDialog } from "@/components/assistant/assistant-convert-dialog";
import { useAssistantDock } from "@/components/assistant/assistant-dock-context";

/** Memory facts of the persisted dedicated session behind the composer; null for ordinary sessions and drafts.
 * 输入区背后已持久化专职会话的记忆事实；普通会话与草稿为 null。
 * Menus read it to hide settings frozen at creation instead of offering changes the service rejects.
 * 菜单读取它以隐藏创建时冻结的设置，而不是提供会被服务拒绝的修改。 */
export const DedicatedComposerContext = React.createContext<DedicatedMemorySelection | null>(null);

/** Returns the dedicated session behind the composer, or null.
 * 返回输入区背后的专职会话；不存在时返回 null。 */
export function useDedicatedComposer(): DedicatedMemorySelection | null {
  return React.useContext(DedicatedComposerContext);
}

/** Shows whether memory is locked and toggles it between turns.
 * 显示记忆是否已锁定，并在回合之间切换。
 *
 * @param props.memory facts of the selected dedicated session.
 *                     选中专职会话的事实。
 * @param props.running whether a turn is open; locking is refused until it ends. */
export function DedicatedMemoryLockBar(props: { memory: DedicatedMemorySelection; running: boolean }) {
  const { t } = useI18n();
  const [pending, setPending] = React.useState(false);
  const locked = dedicatedMemoryLocked(props.memory.lockedSpans);
  const sessionId = props.memory.sessionId;
  const toggle = async () => {
    setPending(true);
    try {
      await setDedicatedMemoryLock(sessionId, !locked);
    } catch (reason) {
      toast.error(dedicatedErrorMessage(reason, t));
    } finally {
      setPending(false);
    }
  };
  const Icon = locked ? Lock : LockOpen;
  // A locked dedicated session can leave the session list as a floating assistant.
  // 记忆已锁定的专职会话可以作为浮动助手移出会话列表。
  const dock = useAssistantDock();
  const [convertOpen, setConvertOpen] = React.useState(false);
  return (
    <div className="dedicated-composer-tab">
      <span className={locked ? "inline-flex min-w-0 items-center gap-1.5 text-primary" : "inline-flex min-w-0 items-center gap-1.5"} role="status">
        <Icon size={14} aria-hidden="true" />
        {locked ? t("composer.dedicated.lockedStatus") : t("composer.dedicated.unlockedStatus")}
      </span>
      {locked && dock ? (
        <Button
          className="ml-auto"
          disabled={pending || props.running}
          onClick={() => setConvertOpen(true)}
          size="xs"
          title={props.running ? t("composer.dedicated.lockWaitTurn") : undefined}
          type="button"
          variant="ghost"
        >
          <Bot aria-hidden="true" />
          {t("assistantDock.convert")}
        </Button>
      ) : null}
      <Button
        className={locked && dock ? undefined : "ml-auto"}
        disabled={pending || props.running}
        onClick={() => void toggle()}
        size="xs"
        title={props.running ? t("composer.dedicated.lockWaitTurn") : undefined}
        type="button"
        variant="ghost"
      >
        {locked ? t("composer.dedicated.unlock") : t("composer.dedicated.lock")}
      </Button>
      {dock ? (
        <AssistantConvertDialog
          defaultName={dock.defaultName(sessionId)}
          onConvert={(name) => dock.convert(sessionId, name)}
          onOpenChange={setConvertOpen}
          open={convertOpen}
        />
      ) : null}
    </div>
  );
}
