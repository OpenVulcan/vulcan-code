import * as React from "react";
import { useI18n } from "@/i18n";
import type { CleanedHistoryPublication } from "@/components/conversation/conversation-cleaned-history";
import { serviceClient } from "@/service/webui-service-runtime";
import { ConversationHistoryDialogContext } from "@/state/conversation-history-dialog";
import { Alert, AlertDescription } from "@/ui-kit/primitives/alert";
import { Button } from "@/ui-kit/primitives/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select";

// A cutoff label shows the initiating prompt and its original time.
// 截止标签展示发起提示词及其原始时间。
interface HistoryCutoff {
  // Inclusive terminal source position.
  // 包含式来源终点。
  through: number;
  // Bounded text from the initiating prompt.
  // 限长的发起提示词开头。
  prompt_preview: string;
  // Original prompt time, or turn start when there is no prompt.
  // 原始提示词时间，无提示词时为回合开始时间。
  started_at_ms: number;
}

// A pending mutation is confirmed inside the same Vulcan dialog.
// 待执行变更在同一个 Vulcan 弹窗内确认。
type HistoryMutation = { operation: "clean"; through: number; generation: number; revision: number };

// The service returns a snapshot of eligible source turns and independent records.
// 服务返回可清理来源回合与独立条目的快照。
interface HistorySnapshot extends CleanedHistoryPublication {
  // Whether the runtime has an applicable compaction.
  // 运行时是否存在适用的压缩。
  enabled: boolean;
  // Complete old turns eligible for cleanup.
  // 可清理的完整旧回合。
  eligible: HistoryCutoff[];
  // Exact source execution generation.
  // 精确来源执行代次。
  generation: number;
  // Exact source execution revision.
  // 精确来源执行修订。
  revision: number;
}

// Props bind the history panel and coverage callback to an exact selected session.
// 属性将历史面板与覆盖回调绑定到精确选中的会话。
interface HistoryPanelProps {
  // Indexed session identity.
  // 索引会话标识。
  sessionId: string;
  // Whether execution currently owns the session.
  // 会话当前是否正被执行占用。
  running: boolean;
  // Applies the exact publication returned by a successful cleanup.
  // 应用清理成功后返回的精确发布。
  onCleaned: (publication: CleanedHistoryPublication) => void;
}

// Renders the selected session's history modal and keeps transcript coverage current.
// 渲染当前会话的历史弹窗，并保持时间线的清理覆盖范围为最新。
// Props supply session identity, execution status and the coverage callback. Returns the controlled dialog.
// 属性提供会话标识、执行状态和覆盖回调，返回受控弹窗。
export function ConversationHistoryCleanup({ sessionId, running, onCleaned }: HistoryPanelProps) {
  // Localized labels come from the shared catalog.
  // 本地化标签来自共享目录。
  const { t, locale } = useI18n();
  // Only the owning conversation can show this modal.
  // 只有所属会话可以显示此弹窗。
  const owner = React.useContext(ConversationHistoryDialogContext);
  // Load the full management view on demand.
  // 按需加载完整管理视图。
  const [snapshot, setSnapshot] = React.useState<HistorySnapshot | null>(null);
  // Only the selected cutoff is editable; an empty choice follows the newest eligible turn.
  // 只编辑截止回合，未手动选择时采用最后一个可清理回合。
  const [through, setThrough] = React.useState<number | null>(null);
  // A mutation is exclusive with another action from the same panel.
  // 同一面板内的变更与其他操作互斥。
  const [busy, setBusy] = React.useState(false);
  // Close the synchronous gap before React publishes the disabled state.
  // 在 React 发布禁用状态前阻止重复操作。
  const inFlight = React.useRef(false);
  // Current failure is shown next to the operation.
  // 当前错误显示在操作旁边。
  const [error, setError] = React.useState<string | null>(null);
  // Preserve the exact cutoff and preview revision until cleanup is confirmed.
  // 保留精确的截止位置与预览修订，直至确认清理。
  const [confirmation, setConfirmation] = React.useState<HistoryMutation | null>(null);
  const open = owner?.sessionId === sessionId && owner.open;

  // Submits a read or confirmed mutation and refreshes the authoritative snapshot.
  // 提交读取或已确认的变更，并刷新权威快照。
  const run = React.useCallback(async (request: { operation: "read" } | HistoryMutation) => {
    if (inFlight.current) return;
    inFlight.current = true;
    setBusy(true);
    setError(null);
    try {
      // Each mutation returns the authoritative replacement.
      // 每次变更都返回权威替换结果。
      const result = await serviceClient.request<HistorySnapshot>("service_conversation_history_v1", {
        ...request, session_id: sessionId,
      });
      setSnapshot(result);
      if (request.operation === "clean") onCleaned(result);
      setThrough(null);
      setConfirmation(null);
    } catch (reason) {
      setError(String(reason));
    } finally {
      inFlight.current = false;
      setBusy(false);
    }
  }, [onCleaned, sessionId]);

  React.useEffect(() => {
    if (open) void run({ operation: "read" });
  }, [open, run]);

  // The start is fixed, and the effective cutoff defaults to the end of the eligible list.
  // 起点固定，默认截止到可清理列表末尾。
  const candidates = snapshot?.eligible ?? [];
  // A custom selection is stored by the authoritative terminal position.
  // 手动选择以权威终点位置保存。
  const last = through === null ? candidates.length - 1 : candidates.findIndex((turn) => turn.through === through);
  // The displayed start and end come from the same preview snapshot.
  // 展示的起止来自同一份预览快照。
  const start = candidates[0];
  const end = candidates[last];

  // Renders the initiating prompt and keeps the timestamp visible when text truncates.
  // 渲染发起提示词，并在正文截断时保持时间可见。
  // Takes a cutoff and returns its display row for both the range and the picker.
  // 接收截止回合，返回供范围预览和选择器共用的展示行。
  function cutoffLabel(turn: HistoryCutoff) {
    return <span className="flex w-full min-w-0 items-baseline gap-x-2">
      <span className="min-w-0 truncate" title={turn.prompt_preview}>{turn.prompt_preview || t("conversation.history.noPrompt")}</span>
      <time className="shrink-0 whitespace-nowrap text-xs">{new Date(turn.started_at_ms).toLocaleString(locale)}</time>
    </span>;
  }

  // Close or cancel only when a request has not acquired the dialog.
  // 仅在请求未占用弹窗时关闭或取消。
  function close() {
    if (inFlight.current) return;
    setConfirmation(null);
    owner?.setOpen(false);
  }

  return <Dialog open={open} onOpenChange={(nextOpen) => { if (!nextOpen) close(); }}>
    <DialogContent size="lg" scrollMode="fixed-footer" showCloseButton={!busy}>
    <DialogHeader>
      <DialogTitle>{t("conversation.history.title")}</DialogTitle>
      <DialogDescription>{t(confirmation ? "conversation.history.cleanConfirm" : "conversation.history.rangeDescription")}</DialogDescription>
    </DialogHeader>
    <div className="grid gap-4">
      {error && <Alert variant="destructive"><AlertDescription>{error}</AlertDescription></Alert>}
      {busy && !snapshot && <p role="status">{t("conversation.history.loading")}</p>}
      {snapshot && <>
        {start && end ? <div className="grid min-w-0 gap-2">
          <div className="grid gap-1">
            <span className="text-sm font-medium">{t("conversation.history.from")}</span>
            {cutoffLabel(start)}
          </div>
          <div className="grid min-w-0 gap-1">
            <span className="text-sm font-medium">{t("conversation.history.through")}</span>
            <Select value={String(end.through)} onValueChange={(value) => setThrough(Number(value))}>
              <SelectTrigger aria-label={t("conversation.history.through")} className="w-full" disabled={busy || confirmation !== null}>
                <SelectValue className="truncate">{cutoffLabel(end)}</SelectValue>
              </SelectTrigger>
              <SelectContent>
                {candidates.map((turn) => <SelectItem key={turn.through} value={String(turn.through)}>
                  {cutoffLabel(turn)}
                </SelectItem>)}
              </SelectContent>
            </Select>
          </div>
          <p role="status" className="text-sm">{t("conversation.history.turnCount").replace("{count}", String(last + 1))}</p>
        </div> : <p>{t(snapshot.enabled ? "conversation.history.noEligible" : "conversation.history.requiresCompaction")}</p>}
      </>}
    </div>
    <DialogFooter>
      {error && !confirmation && <Button type="button" variant="ghost" disabled={busy}
        onClick={() => void run({ operation: "read" })}>{t("conversation.history.refresh")}</Button>}
      <Button type="button" variant="outline" disabled={busy}
        onClick={() => confirmation ? setConfirmation(null) : close()}>{t("conversation.action.cancel")}</Button>
      {confirmation ? <Button type="button" variant="danger" disabled={busy || running}
        onClick={() => void run(confirmation)}>{t("conversation.history.clean")}</Button>
        : <Button type="button" disabled={busy || running || !end} onClick={() => {
          if (snapshot && end) setConfirmation({ operation: "clean", through: end.through, generation: snapshot.generation, revision: snapshot.revision });
        }}>{t("conversation.history.clean")}</Button>}
    </DialogFooter>
    </DialogContent>
  </Dialog>;
}
