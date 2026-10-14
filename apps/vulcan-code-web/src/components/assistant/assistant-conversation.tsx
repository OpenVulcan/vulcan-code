import React from "react";
import { ArrowUp, Check, Copy, Square } from "lucide-react";
import { toast } from "sonner";
import { ComposerAttachmentStrip } from "@/components/composer/composer-attachment-controls";
import { composerAttachmentMedia, inferComposerAttachmentKindFromFile } from "@/components/composer/composer-attachment-media";
import { useComposerAttachments } from "@/components/composer/composer-attachments";
import { ComposerTextContextMenu } from "@/components/composer/composer-text-context-menu";
import { composerAttachmentForResource } from "@/components/conversation/conversation-branch-presentation-v1";
import { runConversationControlAction } from "@/components/conversation/conversation-control-feedback";
import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import { GeneratedMediaPlayerWindow, type GeneratedMediaAttachment } from "@/components/media/generated-media";
import { Button } from "@/components/ui/button";
import { InlineDisclosure } from "@/components/ui/disclosure";
import { Spinner } from "@/components/ui/spinner";
import { Textarea } from "@/components/ui/textarea";
import { copyText } from "@/components/workspace/workbench-layout/types";
import { isElectronRuntime } from "@/host-shell";
import { formatMessage, useI18n } from "@/i18n";
import { formatDurationSeconds, formatTokenCount } from "@/lib/usage-format";
import { createClientOwnedActionId } from "@/notifications/client-identity";
import { localMediaSourceUrl } from "@/resources/localMedia";
import {
  fetchModelAttachmentCapabilities,
  reconcileComposerAttachmentDraftLease,
  stageComposerAttachmentBytes,
  stopConversationRun,
  submitConversationPrompt,
} from "@/service/conversation-actions-service";
import type { ConversationSessionConfigV1 } from "@/service/conversation-event-window-client";
import { useConversationSessionEventWindowOwnerV1 } from "@/state/conversation-session-event-window-owner-v1";
import { useConversationSessionResourcesV1 } from "@/state/conversation-session-resources-v1";
import type { ComposerAttachmentCapabilitySummary, ComposerAttachmentSummary } from "@/types";
import type { AssistantEntry } from "./assistant-dock-context";
import { projectAssistantLastTurn, type AssistantReplyStep, type AssistantUsage } from "./assistant-last-turn";

/** Unsent input per assistant, kept while the user switches between assistants.
 * 每个助手尚未发送的输入，在用户切换助手时保留。 */
const assistantDrafts = new Map<string, string>();
/** Staged but unsent attachments per assistant, kept alongside the text draft.
 * 每个助手已暂存但未发送的附件，与文字草稿一同保留。 */
const assistantAttachmentDrafts = new Map<string, ComposerAttachmentSummary[]>();

/** Compact live view of one assistant: only its last turn plus an input; the session keeps everything.
 * 单个助手的精简实时视图：只显示最后一轮与输入框；完整内容保留在会话中。 */
export function AssistantConversation(props: { assistant: AssistantEntry }) {
  const { t } = useI18n();
  const owner = useConversationSessionEventWindowOwnerV1({
    sessionId: props.assistant.id,
    parentSessionId: null,
    live: true,
  });
  const turn = React.useMemo(() => projectAssistantLastTurn(owner.snapshot.events), [owner.snapshot.events]);
  const resources = useConversationSessionResourcesV1(props.assistant.id, turn.resourceIds);
  const promptAttachments = React.useMemo(
    () => turn.resourceIds.map((resourceId) => composerAttachmentForResource(resourceId, resources)),
    [resources, turn.resourceIds],
  );
  const [mediaPlayer, setMediaPlayer] = React.useState<GeneratedMediaAttachment | null>(null);
  const running = props.assistant.running;
  const scrollRef = React.useRef<HTMLDivElement | null>(null);
  // Follow the newest output while the turn is still open.
  // 回合仍开放时跟随最新输出。
  React.useLayoutEffect(() => {
    const element = scrollRef.current;
    if (element && running) element.scrollTop = element.scrollHeight;
  }, [running, turn]);
  return (
    <div className="assistant-conversation">
      <div ref={scrollRef} className="assistant-conversation-scroll">
        {owner.error ? <p className="assistant-conversation-error">{owner.error}</p> : null}
        {turn.prompt === null && !owner.loading ? (
          <p className="assistant-conversation-hint">
            {formatMessage(t("assistantDock.emptyTurn"), { name: props.assistant.name })}
          </p>
        ) : null}
        <AssistantAttachmentStrip attachments={promptAttachments} onOpenMedia={setMediaPlayer} />
        {turn.prompt ? (
          <div className="assistant-prompt">{turn.prompt}</div>
        ) : null}
        {turn.steps.map((step) => <AssistantStep key={step.apiCallId} step={step} />)}
        {turn.error ? <p className="assistant-conversation-error">{turn.error}</p> : null}
        {(owner.loading && turn.prompt === null) || (running && turn.steps.length === 0) ? (
          <Spinner className="size-4 text-muted-foreground" />
        ) : null}
      </div>
      {turn.total ? (
        <AssistantUsageLine
          className="assistant-turn-total"
          durationMs={turn.durationMs}
          label={t("assistantDock.turnTotal")}
          usage={turn.total}
        />
      ) : null}
      <AssistantComposer
        assistant={props.assistant}
        onOpenMedia={setMediaPlayer}
        running={running}
        sessionConfig={owner.snapshot.session_config}
      />
      {mediaPlayer ? <GeneratedMediaPlayerWindow attachment={mediaPlayer} onClose={() => setMediaPlayer(null)} /> : null}
    </div>
  );
}

/** Attachment tiles reused from the main composer; read-only unless `onRemove` is given.
 * 复用主输入区的附件块；未提供 `onRemove` 时为只读。 */
function AssistantAttachmentStrip(props: {
  attachments: ComposerAttachmentSummary[];
  onOpenMedia: (media: GeneratedMediaAttachment) => void;
  onRemove?: (index: number) => void;
}) {
  return (
    <ComposerAttachmentStrip
      attachments={props.attachments}
      onOpenMedia={(attachment) => {
        const media = composerAttachmentMedia(attachment);
        if (media) props.onOpenMedia(media);
      }}
      onRemove={props.onRemove}
      resolvePreview={(attachment) => {
        const media = composerAttachmentMedia(attachment);
        if (!media) return null;
        return {
          kind: media.kind,
          sourceUrl: media.kind === "image" ? localMediaSourceUrl(media.previewLocalPath ?? media.localPath) : undefined,
        };
      }}
    />
  );
}

/** Attachment gates of the assistant's model route; a tool-less assistant accepts only what the model reads natively.
 * 助手模型路线的附件门控；无工具助手只接受模型原生可读的素材。
 *
 * @param sessionConfig hydrated session configuration, or null before hydration.
 *                      已水合的会话配置；水合前为 null。
 * @returns accepted gates; empty until the route is known and its gates are read. */
function useAssistantAttachmentCapabilities(
  sessionConfig: ConversationSessionConfigV1 | null,
): ComposerAttachmentCapabilitySummary[] {
  const request = sessionConfig?.current_request;
  const providerId = request?.provider ?? null;
  const connectionId = request?.connection_id ?? null;
  const modelId = request?.model ?? null;
  const nativeOnly = sessionConfig?.dedicated?.tool_access === "none";
  const [capabilities, setCapabilities] = React.useState<ComposerAttachmentCapabilitySummary[]>([]);
  React.useEffect(() => {
    setCapabilities([]);
    if (!providerId || !connectionId || !modelId) return;
    let current = true;
    fetchModelAttachmentCapabilities(providerId, connectionId, modelId).then(
      (next) => { if (current) setCapabilities(next); },
      (reason: unknown) => { if (current) toast.error(reason instanceof Error ? reason.message : String(reason)); },
    );
    return () => { current = false; };
  }, [connectionId, modelId, providerId]);
  return React.useMemo(
    () => (nativeOnly ? capabilities.filter((capability) => capability.native) : capabilities),
    [capabilities, nativeOnly],
  );
}

/** Keeps the session's composer-draft lease equal to the staged attachments, as the main composer does;
 * submission rejects staged media whose draft slot holds no lease.
 * 使会话的输入区草稿租约与已暂存附件保持一致（与主输入框相同）；提交会拒绝草稿槽未持有租约的暂存素材。
 *
 * @param assistant the session that owns the `conversation:` draft slot.
 *                  持有 `conversation:` 草稿槽的会话。
 * @param attachments the canonical staged set; an empty set needs no lease.
 *                    规范已暂存集合；空集合无需租约。
 * @returns `ready` once this exact set is leased, and `release`, which leases the set without one attachment. */
function useAssistantDraftLease(assistant: AssistantEntry, attachments: ComposerAttachmentSummary[]) {
  const { id, projectId } = assistant;
  // queueRef orders replacements so an older request cannot re-lease a removed attachment.
  // queueRef 串行化替换，避免较早请求重新租用已移除的附件。
  const queueRef = React.useRef<Promise<unknown>>(Promise.resolve());
  const [leased, setLeased] = React.useState<ComposerAttachmentSummary[] | null>(null);
  const reconcile = React.useCallback((next: ComposerAttachmentSummary[]) => {
    const request = () => reconcileComposerAttachmentDraftLease(projectId, id, `conversation:${id}`, next);
    const result = queueRef.current.then(request, request);
    queueRef.current = result.catch(() => undefined);
    return result;
  }, [id, projectId]);
  React.useEffect(() => {
    if (!attachments.length) return;
    let current = true;
    reconcile(attachments).then(
      () => { if (current) setLeased(attachments); },
      (reason: unknown) => { if (current) reportLeaseError(reason); },
    );
    return () => { current = false; };
  }, [attachments, reconcile]);
  const release = React.useCallback(
    (attachment: ComposerAttachmentSummary, from: ComposerAttachmentSummary[]) =>
      reconcile(from.filter((candidate) => candidate !== attachment)),
    [reconcile],
  );
  return { ready: attachments.length === 0 || leased === attachments, release };
}

/** Shows a failed draft-lease update; the input stays blocked until a later change succeeds.
 * 显示草稿租约更新失败；输入保持阻止直到后续变更成功。 */
function reportLeaseError(reason: unknown) {
  toast.error(reason instanceof Error ? reason.message : String(reason));
}

/** Characters of streaming thinking kept for the one-line ticker; far wider than any assistant window.
 * 单行滚动条保留的流式思考字符数；远宽于任何助手窗口。 */
const THINKING_TICKER_TAIL = 400;

/** Collapsed live thinking as one line that starts at the beginning and then keeps its newest text in view;
 * it only moves forward and never rewinds.
 * 收起状态下的实时思考单行：从开头输出，之后始终显示最新文字；只向前推进，从不回卷。 */
function AssistantThinkingTicker(props: { text: string }) {
  const ref = React.useRef<HTMLDivElement | null>(null);
  const line = props.text.slice(-THINKING_TICKER_TAIL).replace(/\s+/gu, " ").trim();
  React.useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    element.scrollLeft = element.scrollWidth;
    element.toggleAttribute("data-overflow", element.scrollWidth > element.clientWidth);
  }, [line]);
  return <div aria-hidden="true" className="assistant-thinking-ticker" ref={ref}>{line}</div>;
}

/** Keeps a scrolling box at its bottom while its content grows, until the reader scrolls up; scrolling back to
 * the bottom pins it again. A box that mounts (the thinking is expanded) starts at the bottom.
 * 内容增长时让滚动框保持在底部，直到读者向上滚动；滚回底部后重新锁定。新挂载（思考展开）的滚动框从底部开始。 */
function useStickToBottom(content: string) {
  const element = React.useRef<HTMLDivElement | null>(null);
  const pinned = React.useRef(true);
  const ref = React.useCallback((node: HTMLDivElement | null) => {
    element.current = node;
    pinned.current = true;
    if (node) node.scrollTop = node.scrollHeight;
  }, []);
  React.useLayoutEffect(() => {
    const node = element.current;
    if (node && pinned.current) node.scrollTop = node.scrollHeight;
  }, [content]);
  const onScroll = React.useCallback(() => {
    const node = element.current;
    if (node) pinned.current = node.scrollHeight - node.scrollTop - node.clientHeight < 8;
  }, []);
  return { ref, onScroll };
}

/** One provider request: thinking folded by default (a one-line ticker while it streams), then the answer.
 * 一次供应商请求：思考默认收起（流式输出时显示单行滚动），随后是回答。 */
export function AssistantStep(props: { step: AssistantReplyStep }) {
  const { t } = useI18n();
  const { step } = props;
  const [thinkingOpen, setThinkingOpen] = React.useState(false);
  const thinkingLive = Boolean(step.reasoning?.streaming) && !step.answer;
  const thinkingScroll = useStickToBottom(step.reasoning?.text ?? "");
  return (
    <div className="assistant-step">
      {step.reasoning ? (
        <InlineDisclosure
          label={thinkingLive ? t("assistantDock.thinkingLive") : t("assistantDock.thinking")}
          onOpenChange={setThinkingOpen}
          open={thinkingOpen}
        >
          <div className="assistant-thinking-text" onScroll={thinkingScroll.onScroll} ref={thinkingScroll.ref}>{step.reasoning.text}</div>
        </InlineDisclosure>
      ) : null}
      {step.reasoning && thinkingLive && !thinkingOpen ? <AssistantThinkingTicker text={step.reasoning.text} /> : null}
      {step.tools.length > 0 ? (
        <div className="assistant-tool-list">
          {step.tools.map((tool) => <span key={tool.id} className="assistant-tool-chip">{tool.name}</span>)}
        </div>
      ) : null}
      {step.answer ? <MarkdownMessage content={step.answer.text} eager streaming={step.answer.streaming} /> : null}
      {step.usage || (step.answer && !step.answer.streaming) ? (
        <div className="assistant-step-footer">
          {step.usage ? <AssistantUsageLine durationMs={step.durationMs} usage={step.usage} /> : null}
          {step.answer && !step.answer.streaming ? <AssistantCopyButton text={step.answer.text} /> : null}
        </div>
      ) : null}
    </div>
  );
}

/** Copies one finished answer's Markdown source and briefly confirms it.
 * 复制一段已完成回答的 Markdown 原文，并短暂显示确认。 */
function AssistantCopyButton(props: { text: string }) {
  const { t } = useI18n();
  const [copied, setCopied] = React.useState(false);
  const label = copied ? t("conversation.action.copiedFinal") : t("conversation.action.copyFinal");
  const copy = async () => {
    setCopied(false);
    setCopied(await runConversationControlAction(() => copyText(props.text), t("conversation.control.copyFailed")));
  };
  return (
    <Button
      aria-label={label}
      className="assistant-copy-button"
      onClick={() => void copy()}
      onMouseLeave={() => setCopied(false)}
      size="icon-xs"
      title={label}
      type="button"
      variant="ghost"
    >
      {copied ? <Check size={13} /> : <Copy size={13} />}
    </Button>
  );
}

/** Usage written out in one quiet line; nothing hides behind a hover.
 * 以一行低调文字直接写出用量；不藏在悬停提示后。 */
function AssistantUsageLine(props: {
  usage: AssistantUsage;
  durationMs: number | null;
  label?: string;
  className?: string;
}) {
  const { t } = useI18n();
  const metric = (key: Parameters<typeof t>[0], value: number | undefined) => (
    value === undefined ? null : formatMessage(t(key), { value: formatTokenCount(value) })
  );
  const parts = [
    props.label ?? null,
    metric("assistantDock.usage.input", props.usage.input),
    metric("assistantDock.usage.cached", props.usage.cached),
    metric("assistantDock.usage.output", props.usage.output),
    metric("assistantDock.usage.total", props.usage.total),
    props.durationMs === null ? null : formatDurationSeconds(props.durationMs),
  ].filter((part): part is string => part !== null);
  return <p className={props.className ? `assistant-usage-line ${props.className}` : "assistant-usage-line"}>{parts.join(" · ")}</p>;
}

/** One-line input that grows with its text; Enter sends, Shift+Enter breaks the line.
 * 随内容增高的单行输入；Enter 发送，Shift+Enter 换行。 */
export function AssistantComposer(props: {
  assistant: AssistantEntry;
  running: boolean;
  sessionConfig: ConversationSessionConfigV1 | null;
  onOpenMedia: (media: GeneratedMediaAttachment) => void;
}) {
  const { t } = useI18n();
  const { assistant } = props;
  const [text, setText] = React.useState(() => assistantDrafts.get(assistant.id) ?? "");
  const [pending, setPending] = React.useState(false);
  const inputRef = React.useRef<HTMLTextAreaElement | null>(null);
  const capabilities = useAssistantAttachmentCapabilities(props.sessionConfig);
  const staging = useComposerAttachments({
    attachmentCapabilities: capabilities,
    conversationId: assistant.id,
    initialAttachments: assistantAttachmentDrafts.get(assistant.id),
    onStageAttachmentBytes: stageComposerAttachmentBytes,
    projectId: assistant.projectId,
  });
  const { attachments } = staging;
  const lease = useAssistantDraftLease(assistant, attachments);
  React.useEffect(() => {
    if (text) assistantDrafts.set(assistant.id, text);
    else assistantDrafts.delete(assistant.id);
  }, [assistant.id, text]);
  React.useEffect(() => {
    if (attachments.length) assistantAttachmentDrafts.set(assistant.id, attachments);
    else assistantAttachmentDrafts.delete(assistant.id);
  }, [assistant.id, attachments]);
  // paste leaves plain text to the textarea; files the assistant cannot take are reported, not silently dropped.
  // 粘贴时纯文本交给输入框；助手无法接收的文件会提示，而不是静默丢弃。
  const paste = (event: React.ClipboardEvent<HTMLTextAreaElement>) => {
    staging.handleComposerPaste(event);
    if (event.defaultPrevented) return;
    const hasMedia = Array.from(event.clipboardData.items ?? []).some((item) => {
      const file = item.kind === "file" ? item.getAsFile() : null;
      return file !== null && inferComposerAttachmentKindFromFile(file) !== null;
    });
    if (hasMedia) {
      event.preventDefault();
      toast.info(props.sessionConfig?.dedicated?.tool_access === "none"
        ? t("assistantDock.attachmentNativeOnly")
        : t("composer.attachment.unsupported"));
    }
  };
  const blocked = props.running || pending || staging.pendingAttachmentCount > 0 || !lease.ready;
  const send = async () => {
    const sentText = text;
    const prompt = sentText.trim();
    if ((!prompt && attachments.length === 0) || blocked || staging.hasPendingAttachmentStaging()) return;
    setPending(true);
    try {
      const ack = await submitConversationPrompt(
        createClientOwnedActionId(),
        assistant.projectId,
        assistant.id,
        `conversation:${assistant.id}`,
        prompt,
        undefined,
        attachments,
        "build",
        null,
        null,
        null,
      );
      if (!ack) {
        toast.info(t("conversationActions.previewCannotSend"));
        return;
      }
      // The input stays editable while the receipt is pending, so only what was sent is cleared.
      // 等待回执期间输入框仍可编辑，因此只清除已发送的内容。
      setText((current) => (current === sentText ? "" : current));
      staging.setAttachments((current) => current.filter((attachment) => !attachments.includes(attachment)));
    } catch (reason) {
      toast.error(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setPending(false);
    }
  };
  const stop = async () => {
    try {
      await stopConversationRun(assistant.id, "main_and_subagents");
    } catch (reason) {
      toast.error(reason instanceof Error ? reason.message : String(reason));
    }
  };
  return (
    <div className="assistant-composer">
      <AssistantAttachmentStrip
        attachments={attachments}
        onOpenMedia={props.onOpenMedia}
        onRemove={(index) => {
          const attachment = attachments[index];
          if (attachment) void lease.release(attachment, attachments).then(() => staging.removeComposerAttachment(attachment), reportLeaseError);
        }}
      />
      <div className="assistant-composer-row">
        {/* The desktop shell has no native editing menu, so the input uses the main composer's one. */}
        {/* 桌面壳没有原生编辑菜单，因此输入框使用与主输入区相同的菜单。 */}
        <ComposerTextContextMenu
          disabled={false}
          enabled={isElectronRuntime()}
          onChange={setText}
          ownerKey={`assistant:${assistant.id}`}
          textareaRef={inputRef}
          value={text}
        >
          <Textarea
            aria-label={formatMessage(t("assistantDock.placeholder"), { name: assistant.name })}
            className="assistant-composer-input"
            onChange={(event) => setText(event.target.value)}
            onPaste={paste}
            onKeyDown={(event) => {
              if (event.key !== "Enter" || event.shiftKey || event.nativeEvent.isComposing) return;
              event.preventDefault();
              void send();
            }}
            placeholder={formatMessage(t("assistantDock.placeholder"), { name: assistant.name })}
            ref={inputRef}
            rows={1}
            value={text}
          />
        </ComposerTextContextMenu>
        {props.running ? (
          <Button aria-label={t("assistantDock.stop")} onClick={() => void stop()} size="icon-sm" type="button" variant="secondary">
            <Square size={14} />
          </Button>
        ) : (
          <Button
            aria-label={t("assistantDock.send")}
            disabled={(!text.trim() && attachments.length === 0) || blocked}
            onClick={() => void send()}
            size="icon-sm"
            type="button"
          >
            <ArrowUp size={16} />
          </Button>
        )}
      </div>
    </div>
  );
}
