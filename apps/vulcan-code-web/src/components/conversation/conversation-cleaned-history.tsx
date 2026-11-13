import * as React from "react";
import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import { useI18n } from "@/i18n";
import type { ResourceId } from "@/service/conversation-event-window-store";
import { hasServiceRuntime, serviceClient } from "@/service/webui-service-runtime";
import { useConversationSessionResourcesV1 } from "@/state/conversation-session-resources-v1";
import { Alert, AlertDescription } from "@/ui-kit/primitives/alert";
import { Button } from "@/ui-kit/primitives/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog";

// Source ranges only suppress replaced runtime events.
// 来源范围仅用于隐藏已替换的运行时事件。
export interface HistoryEventRange {
  // Inclusive original start and end.
  // 原始事件的包含式起点和终点。
  from: number;
  through: number;
}

// Archived display records retain their own identities and original text.
// 历史展示记录保留独立标识及原始正文。
interface HistoryText {
  // Independent display identity and original text.
  // 独立展示标识和原始正文。
  id: string;
  text: string;
}

// Display records and source coverage are returned from one publication.
// 展示条目与来源覆盖范围来自同一次发布。
export interface CleanedHistoryPublication {
  // Coverage and retained content from one authoritative read.
  // 同一次权威读取获得的覆盖范围和保留内容。
  cleaned_ranges: HistoryEventRange[];
  history: {
    turns: Array<{
      id: string;
      closed_at_ms: number;
      inputs: Array<HistoryText & { created_at_ms: number; supplementary: boolean; attachments: ResourceId[] }>;
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

// Loads the selected session's display publication and exposes its update and removal operations.
// 加载选中会话的展示发布，并提供更新与移除操作。
// A null session disables reads. Returns session-scoped state and mutation handlers.
// 会话为空时禁用读取，返回仅适用于当前会话的状态与变更函数。
export function useConversationCleanedHistory(sessionId: string | null) {
  // Pair the records with their owner so an old request cannot paint another session.
  // 将记录与所属会话配对，避免旧请求结果显示到另一会话。
  const [state, setState] = React.useState<{ sessionId: string; publication: CleanedHistoryPublication } | null>(null);
  // A completed mutation supersedes any previously started cold read.
  // 已完成的变更优先于此前启动的冷加载读取。
  const version = React.useRef(0);
  // Apply one complete publication atomically to the timeline.
  // 将一份完整发布原子应用到时间线。
  const accept = React.useCallback((publication: CleanedHistoryPublication) => {
    if (sessionId === null) return;
    version.current += 1;
    setState({ sessionId, publication });
  }, [sessionId]);

  React.useEffect(() => {
    if (sessionId === null || !hasServiceRuntime()) return;
    // The effect owns the lifetime of this particular cold read.
    // 此副作用管理这一次冷加载读取的生命周期。
    let active = true;
    // Capture the version before sending the request.
    // 发出请求前记录当前版本。
    const started = version.current;
    void serviceClient.request<CleanedHistoryPublication>("service_conversation_history_v1", {
      operation: "coverage", session_id: sessionId,
    }).then((publication) => {
      if (active && started === version.current) accept(publication);
    }).catch(() => undefined);
    return () => { active = false; };
  }, [accept, sessionId]);

  // Removes a single independent record and returns after replacing the display publication.
  // 移除单个独立条目，替换展示发布后返回。
  const remove = React.useCallback(async (id: string) => {
    if (sessionId === null) return;
    accept(await serviceClient.request<CleanedHistoryPublication>("service_conversation_history_v1", {
      operation: "delete", session_id: sessionId, ids: [id],
    }));
  }, [accept, sessionId]);

  return { publication: state && state.sessionId === sessionId ? state.publication : undefined, accept, remove };
}

// Props provide one session's display publication and an independent-ID removal function.
// 属性提供一个会话的展示发布和按独立标识移除的函数。
interface CleanedHistoryProps {
  // Exact owner, display records, and removal callback.
  // 精确会话所有者、展示记录和移除回调。
  sessionId: string;
  history: CleanedHistoryPublication["history"];
  onRemove: (id: string) => Promise<void>;
}

// Renders preserved turns in the transcript and confirms removal under the corresponding prompt.
// 在会话时间线显示保留的回合，并在对应提示词下确认移除。
// Takes the published display records and returns read-only transcript rows.
// 接收已发布的展示记录，返回只读会话行。
export function ConversationCleanedHistory({ sessionId, history, onRemove }: CleanedHistoryProps) {
  const { t, locale } = useI18n();
  // A root prompt removes its turn; supplementary prompts and resources are independently removable.
  // 根提示词移除其所属回合，追加提示词和素材可独立移除。
  const [target, setTarget] = React.useState<{ id: string; kind: "turn" | "input" | "resource" } | null>(null);
  // Keep the failed removal available for retry and serialize confirmation.
  // 保留失败的移除以便重试，并串行执行确认。
  const [error, setError] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState(false);
  const submitting = React.useRef(false);
  // Resolve only resources referenced by retained display entries.
  // 只解析保留展示条目引用的素材。
  const resourceIds = React.useMemo(() => [
    ...history.turns.flatMap((turn) => turn.inputs.flatMap((input) => input.attachments)),
    ...history.resources.flatMap((asset) => asset.preview_resource_id === null
      ? [asset.resource_id] : [asset.resource_id, asset.preview_resource_id]),
  ], [history]);
  const resources = useConversationSessionResourcesV1(sessionId, resourceIds);

  // Renders a catalog-backed link for the supplied resource ID.
  // 为给定素材 ID 渲染由资源目录支持的链接。
  function resourceLink(id: ResourceId) {
    const resource = resources.resources.get(id);
    return resource?.availability === "available"
      ? <a href={resources.contentUrl(id)} target="_blank" rel="noreferrer" className="underline">{resource.original_name}</a>
      : <span>{t("conversation.history.resourceUnavailable")}</span>;
  }

  // Opens the confirmation for the supplied independent record.
  // 为给定独立条目打开确认框。
  function beginRemoval(id: string, kind: "turn" | "input" | "resource") {
    setError(null);
    setTarget({ id, kind });
  }

  // Removes the pending record once, keeping a failed request available for retry.
  // 移除当前记录一次，失败时保留确认框以便重试。
  async function confirmRemoval() {
    if (!target || submitting.current) return;
    submitting.current = true;
    setBusy(true);
    setError(null);
    try {
      await onRemove(target.id);
      setTarget(null);
    } catch (reason) {
      setError(String(reason));
    } finally {
      submitting.current = false;
      setBusy(false);
    }
  }

  return <>
    {history.turns.filter((turn) => turn.inputs.length > 0 || turn.answer).map((turn) => (
      <div className="grid gap-3" key={turn.id} data-cleaned-history-turn={turn.id}>
        {turn.inputs.map((input) => (
          <article className="vk-prompt-row" key={input.id}>
            {input.supplementary && <span className="text-xs">{t("conversation.history.supplementary")}</span>}
            {input.text && <div className="vk-prompt"><p className="vk-prompt-text">{input.text}</p></div>}
            {input.attachments.length > 0 && <ul className="vk-prompt-files">
              {input.attachments.map((id, occurrence) => <li key={`${id}:${occurrence}`}>{resourceLink(id)}</li>)}
            </ul>}
            <div className="vk-prompt-foot">
              <time>{new Date(input.created_at_ms).toLocaleString(locale)}</time>
              <Button size="xs" variant="ghost" onClick={() => input.supplementary ? beginRemoval(input.id, "input") : beginRemoval(turn.id, "turn")}>
                {t("conversation.history.remove")}
              </Button>
            </div>
          </article>
        ))}
        {turn.answer && <article className="vk-answer-row" data-role="assistant" data-final="">
          <div className="vk-flow"><div className="vk-flow-body"><MarkdownMessage content={turn.answer.text} /></div></div>
        </article>}
        {turn.inputs.every((input) => input.supplementary) && <Button size="xs" variant="ghost" className="justify-self-end"
          onClick={() => beginRemoval(turn.id, "turn")}>{t("conversation.history.remove")}</Button>}
      </div>
    ))}
    {history.resources.length > 0 && <details>
      <summary className="cursor-pointer">{t("conversation.history.resources")} ({history.resources.length})</summary>
      <ul className="grid gap-2">
        {history.resources.map((asset) => <li key={asset.id} className="grid gap-1 border-b p-2">
          {resourceLink(asset.resource_id)}
          {asset.kind === "image" && resources.resources.get(asset.resource_id)?.availability === "available" &&
            <img loading="lazy" className="max-h-32 max-w-full object-contain object-left"
              src={resources.contentUrl(asset.preview_resource_id !== null
                && resources.resources.get(asset.preview_resource_id)?.availability === "available"
                ? asset.preview_resource_id : asset.resource_id)} alt="" />}
          {asset.preview_resource_id !== null && resourceLink(asset.preview_resource_id)}
          {asset.prompt && <p className="whitespace-pre-wrap break-words">{asset.prompt}</p>}
          <Button size="xs" variant="ghost" className="justify-self-start"
            onClick={() => beginRemoval(asset.id, "resource")}>{t("conversation.history.remove")}</Button>
        </li>)}
      </ul>
    </details>}
    <Dialog open={target !== null} onOpenChange={(open) => { if (!open && !submitting.current) setTarget(null); }}>
      <DialogContent size="sm" showCloseButton={!busy}>
        <DialogHeader>
          <DialogTitle>{t("conversation.history.remove")}</DialogTitle>
          <DialogDescription>{t(target?.kind === "resource" ? "conversation.history.removeResourceConfirm"
            : target?.kind === "input" ? "conversation.history.removeInputConfirm"
            : "conversation.history.removeTurnConfirm")}</DialogDescription>
        </DialogHeader>
        {error && <Alert variant="destructive"><AlertDescription>{error}</AlertDescription></Alert>}
        <DialogFooter>
          <Button variant="outline" disabled={busy} onClick={() => setTarget(null)}>{t("conversation.action.cancel")}</Button>
          <Button variant="danger" disabled={busy} onClick={() => void confirmRemoval()}>{t("conversation.history.remove")}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </>;
}
