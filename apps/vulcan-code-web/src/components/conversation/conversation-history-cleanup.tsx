import * as React from "react";
import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import { useI18n } from "@/i18n";
import type { ResourceId } from "@/service/conversation-event-window-store";
import { hasServiceRuntime, serviceClient } from "@/service/webui-service-runtime";
import { useConversationSessionResourcesV1 } from "@/state/conversation-session-resources-v1";
import { ConversationHistoryDialogContext } from "@/state/conversation-history-dialog";
import { Alert, AlertDescription } from "@/ui-kit/primitives/alert";
import { Button } from "@/ui-kit/primitives/button";
import { Checkbox } from "@/ui-kit/primitives/checkbox";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select";

// Event ranges are used only to select or hide the original execution stream.
// 事件范围只用于选择或隐藏原执行事件流。
export interface HistoryEventRange {
  // Inclusive first source event.
  // 来源事件的包含式起点。
  from: number;
  // Inclusive last source event.
  // 来源事件的包含式终点。
  through: number;
}

// A display record has its own identity and original text.
// 展示记录拥有自己的身份和原文。
interface HistoryText {
  // Independent deletion identity.
  // 独立删除标识。
  id: string;
  // Original text.
  // 原始正文。
  text: string;
}

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
type HistoryMutation = { operation: "clean"; through: number; generation: number; revision: number }
  | { operation: "delete"; ids: string[] };

// The service returns a snapshot of eligible source turns and independent records.
// 服务返回可清理来源回合与独立条目的快照。
interface HistorySnapshot {
  // Whether the runtime has an applicable compaction.
  // 运行时是否存在适用的压缩。
  enabled: boolean;
  // Replaced source ranges.
  // 已替换的来源范围。
  cleaned_ranges: HistoryEventRange[];
  // Complete old turns eligible for cleanup.
  // 可清理的完整旧回合。
  eligible: HistoryCutoff[];
  // Exact source execution generation.
  // 精确来源执行代次。
  generation: number;
  // Exact source execution revision.
  // 精确来源执行修订。
  revision: number;
  // Independent display records, separate from the source selector.
  // 与来源选择器分开的独立展示记录。
  history: {
    turns: Array<{
      id: string;
      closed_at_ms: number;
      inputs: Array<HistoryText & { supplementary: boolean; attachments: ResourceId[] }>;
      answer: HistoryText | null;
    }>;
    resources: Array<{
      id: string;
      resource_id: ResourceId;
      preview_resource_id: ResourceId | null;
      kind: "image" | "video";
      prompt: string | null;
    }>;
  };
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
  // Last completed compaction in the mounted canonical window, when present.
  // 已挂载规范窗口中最近完成的压缩（如有）。
  compactionRevision: number | null;
  // Receives the source ranges replaced by independent records.
  // 接收由独立条目替换的来源范围。
  onCoverage: (ranges: HistoryEventRange[]) => void;
}

// Renders the selected session's history modal and keeps transcript coverage current.
// 渲染当前会话的历史弹窗，并保持时间线的清理覆盖范围为最新。
// Props supply session identity, execution status and the coverage callback. Returns the controlled dialog.
// 属性提供会话标识、执行状态和覆盖回调，返回受控弹窗。
export function ConversationHistoryCleanup({ sessionId, running, compactionRevision, onCoverage }: HistoryPanelProps) {
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
  // Prevent an older coverage response from overwriting the result of a mutation.
  // 防止旧覆盖响应覆盖变更结果。
  const coverageEpoch = React.useRef(0);
  // Current failure is shown next to the operation.
  // 当前错误显示在操作旁边。
  const [error, setError] = React.useState<string | null>(null);
  // Require explicit confirmation for cleanup and deletion.
  // 清理与删除都要求明确确认。
  const [confirmation, setConfirmation] = React.useState<HistoryMutation | null>(null);
  // Selection persists only in the open management panel.
  // 选择状态仅保留在打开的管理面板中。
  const [selected, setSelected] = React.useState<ReadonlySet<string>>(() => new Set());
  // Collect exact existing asset references for the ordinary catalog.
  // 为普通资源目录收集精确的现有素材引用。
  const resourceIds = React.useMemo(() => snapshot ? [
    ...snapshot.history.resources.flatMap((asset) => asset.preview_resource_id === null
      ? [asset.resource_id] : [asset.resource_id, asset.preview_resource_id]),
    ...snapshot.history.turns.flatMap((turn) => turn.inputs.flatMap((input) => input.attachments)),
  ] : [], [snapshot]);
  // Catalog ownership, authentication and unavailable resources follow the existing path.
  // 资源目录归属、鉴权与不可用资源沿用既有链路。
  const open = owner?.sessionId === sessionId && owner.open;
  // Catalog reads are needed only while the modal is open.
  // 只在弹窗打开时读取资源目录。
  const resources = useConversationSessionResourcesV1(open && snapshot ? sessionId : null, resourceIds);

  React.useEffect(() => {
    if (!hasServiceRuntime()) return;
    // Ignore a coverage response that belongs to an unmounted panel.
    // 忽略属于已卸载面板的覆盖响应。
    let active = true;
    // Capture the publication epoch before reading lightweight coverage.
    // 读取轻量覆盖信息前记录发布代次。
    const epoch = coverageEpoch.current;
    void serviceClient.request<HistorySnapshot>("service_conversation_history_v1", {
      operation: "coverage", session_id: sessionId,
    }).then((result) => {
      if (active && epoch === coverageEpoch.current) {
        onCoverage(result.cleaned_ranges);
      }
    // The modal reads its own authoritative snapshot and displays read errors there.
    // 弹窗会读取自己的权威快照并显示读取错误。
    }).catch(() => undefined);
    return () => { active = false; };
  }, [compactionRevision, onCoverage, sessionId]);

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
      coverageEpoch.current += 1;
      setSnapshot(result);
      onCoverage(result.cleaned_ranges);
      setThrough(null);
      setSelected(new Set());
      setConfirmation(null);
    } catch (reason) {
      setError(String(reason));
    } finally {
      inFlight.current = false;
      setBusy(false);
    }
  }, [onCoverage, sessionId]);

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

  // Renders identical single-delete and batch-selection controls for each independent ID.
  // 为每个独立标识渲染一致的单删与批量选择控件。
  function entryActions(id: string) {
    return <span className="inline-flex items-center gap-2">
      <Checkbox aria-label={t("conversation.history.select")} checked={selected.has(id)}
        disabled={busy} onCheckedChange={(checked) => {
          setSelected((previous) => {
            // Copy the previous immutable selection.
            // 复制前一次不可变选择。
            const next = new Set(previous);
            if (checked) next.add(id);
            else next.delete(id);
            return next;
          });
        }} />
      <Button type="button" size="xs" variant="ghost" disabled={busy}
        onClick={() => setConfirmation({ operation: "delete", ids: [id] })}>{t("conversation.history.delete")}</Button>
    </span>;
  }

  // Builds a catalog-backed link only for an available resource.
  // 仅为可用资源构造目录支持的链接。
  function resourceLink(id: ResourceId) {
    // Catalog metadata provides the display filename and availability.
    // 目录元数据提供展示文件名和可用状态。
    const resource = resources.resources.get(id);
    return resource?.availability === "available"
      ? <a href={resources.contentUrl(id)} target="_blank" rel="noreferrer" className="underline">
          {resource.original_name}
        </a>
      : <span>{t("conversation.history.resourceUnavailable")}</span>;
  }

  return <Dialog open={open} onOpenChange={(nextOpen) => { if (!nextOpen) close(); }}>
    <DialogContent size="lg" scrollMode="fixed-footer" showCloseButton={!busy}>
    <DialogHeader>
      <DialogTitle>{t("conversation.history.title")}</DialogTitle>
      <DialogDescription>{t(confirmation?.operation === "delete" ? "conversation.history.deleteConfirm" :
        confirmation ? "conversation.history.cleanConfirm" : "conversation.history.rangeDescription")}</DialogDescription>
    </DialogHeader>
    <div className="grid gap-4">
      {error && <Alert variant="destructive"><AlertDescription>{error}</AlertDescription></Alert>}
      {busy && !snapshot && <p role="status">{t("conversation.history.loading")}</p>}
      {snapshot && confirmation?.operation !== "delete" && <>
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
      {snapshot && !confirmation && <>
        {(snapshot.history.turns.length > 0 || snapshot.history.resources.length > 0) &&
          <Button type="button" size="sm" variant="outline" className="justify-self-start" disabled={busy || selected.size === 0}
            onClick={() => setConfirmation({ operation: "delete", ids: [...selected] })}>{t("conversation.history.deleteSelected")}</Button>}
        {snapshot.history.resources.length > 0 && <details>
          <summary className="cursor-pointer">{t("conversation.history.resources")} ({snapshot.history.resources.length})</summary>
          <ul className="grid gap-2">
            {snapshot.history.resources.map((asset) => <li key={asset.id} className="grid gap-1 border-b p-2">
              {resourceLink(asset.resource_id)}
              {asset.kind === "image" && resources.resources.get(asset.resource_id)?.availability === "available" &&
                <img loading="lazy" className="max-h-32 max-w-full object-contain object-left"
                  src={resources.contentUrl(asset.preview_resource_id !== null
                    && resources.resources.get(asset.preview_resource_id)?.availability === "available"
                    ? asset.preview_resource_id : asset.resource_id)}
                  alt="" />}
              {asset.preview_resource_id !== null && resourceLink(asset.preview_resource_id)}
              {asset.prompt && <p className="whitespace-pre-wrap break-words">{asset.prompt}</p>}
              {entryActions(asset.id)}
            </li>)}
          </ul>
        </details>}
        {snapshot.history.turns.map((turn) => <article key={turn.id} className="grid gap-2 rounded-md border p-2">
          <div className="flex flex-wrap justify-between gap-2">
            <time>{new Date(turn.closed_at_ms).toLocaleString()}</time>
            {entryActions(turn.id)}
          </div>
          {turn.inputs.filter((input) => !input.supplementary).map((input) => <div key={input.id}>
            <MarkdownMessage content={input.text} />{input.attachments.map((id, index) =>
              <React.Fragment key={index}>{resourceLink(id)}{" "}</React.Fragment>)}{entryActions(input.id)}
          </div>)}
          {turn.inputs.some((input) => input.supplementary) && <details>
            <summary className="cursor-pointer">{t("conversation.history.supplementary")}</summary>
            <ol className="grid gap-2 border-l pl-3">
              {turn.inputs.filter((input) => input.supplementary).map((input) => <li key={input.id}>
                <MarkdownMessage content={input.text} />{input.attachments.map((id, index) =>
                  <React.Fragment key={index}>{resourceLink(id)}{" "}</React.Fragment>)}{entryActions(input.id)}
              </li>)}
            </ol>
          </details>}
          {turn.answer && <div className="border-t pt-2">
            <MarkdownMessage content={turn.answer.text} />{entryActions(turn.answer.id)}
          </div>}
        </article>)}
      </>}
    </div>
    <DialogFooter>
      {error && !confirmation && <Button type="button" variant="ghost" disabled={busy}
        onClick={() => void run({ operation: "read" })}>{t("conversation.history.refresh")}</Button>}
      <Button type="button" variant="outline" disabled={busy}
        onClick={() => confirmation ? setConfirmation(null) : close()}>{t("conversation.action.cancel")}</Button>
      {confirmation ? <Button type="button" variant="danger" disabled={busy || (confirmation.operation === "clean" && running)}
        onClick={() => void run(confirmation)}>{t(confirmation.operation === "clean" ? "conversation.history.clean" : "conversation.history.delete")}</Button>
        : <Button type="button" disabled={busy || running || !end} onClick={() => {
          if (snapshot && end) setConfirmation({ operation: "clean", through: end.through, generation: snapshot.generation, revision: snapshot.revision });
        }}>{t("conversation.history.clean")}</Button>}
    </DialogFooter>
    </DialogContent>
  </Dialog>;
}
