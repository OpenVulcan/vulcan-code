import React from "react";
import { ArrowUp, Check, Copy, Square } from "lucide-react";
import { toast } from "sonner";
import { ComposerAttachmentStrip } from "@/components/composer/composer-attachment-controls";
import { composerAttachmentMedia, inferComposerAttachmentKindFromFile } from "@/components/composer/composer-attachment-media";
import { useComposerAttachments } from "@/components/composer/composer-attachments";
import { composerAttachmentForResource } from "@/components/conversation/conversation-branch-presentation-v1";
import { runConversationControlAction } from "@/components/conversation/conversation-control-feedback";
import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import { GeneratedMediaPlayerWindow, type GeneratedMediaAttachment } from "@/components/media/generated-media";
import { Button } from "@/components/ui/button";
import { InlineDisclosure } from "@/components/ui/disclosure";
import { Spinner } from "@/components/ui/spinner";
import { Textarea } from "@/components/ui/textarea";
import { copyText } from "@/components/workspace/workbench-layout/types";
import { formatMessage, useI18n } from "@/i18n";
import { formatDurationSeconds, formatTokenCount } from "@/lib/usage-format";
import { createClientOwnedActionId } from "@/notifications/client-identity";
import { localMediaSourceUrl } from "@/resources/localMedia";
import {
  fetchModelAttachmentCapabilities,
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

/** One provider request: thinking shown inline while it streams and folded afterwards, then the answer.
 * 一次供应商请求：思考在流式输出时内联展开、结束后收起，随后是回答。 */
function AssistantStep(props: { step: AssistantReplyStep }) {
  const { t } = useI18n();
  const { step } = props;
  // userThinkingOpen records an explicit toggle; until then the thinking follows its live state.
  // userThinkingOpen 记录用户的明确切换；在此之前思考区跟随其实时状态。
  const [userThinkingOpen, setUserThinkingOpen] = React.useState<boolean | null>(null);
  const thinkingLive = Boolean(step.reasoning?.streaming) && !step.answer;
  return (
    <div className="assistant-step">
      {step.reasoning ? (
        <InlineDisclosure
          label={thinkingLive ? t("assistantDock.thinkingLive") : t("assistantDock.thinking")}
          onOpenChange={setUserThinkingOpen}
          open={userThinkingOpen ?? thinkingLive}
        >
          <div className="assistant-thinking-text">{step.reasoning.text}</div>
        </InlineDisclosure>
      ) : null}
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
function AssistantComposer(props: {
  assistant: AssistantEntry;
  running: boolean;
  sessionConfig: ConversationSessionConfigV1 | null;
  onOpenMedia: (media: GeneratedMediaAttachment) => void;
}) {
  const { t } = useI18n();
  const { assistant } = props;
  const [text, setText] = React.useState(() => assistantDrafts.get(assistant.id) ?? "");
  const [pending, setPending] = React.useState(false);
  const capabilities = useAssistantAttachmentCapabilities(props.sessionConfig);
  const staging = useComposerAttachments({
    attachmentCapabilities: capabilities,
    conversationId: assistant.id,
    initialAttachments: assistantAttachmentDrafts.get(assistant.id),
    onStageAttachmentBytes: stageComposerAttachmentBytes,
    projectId: assistant.projectId,
  });
  const { attachments } = staging;
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
  const blocked = props.running || pending || staging.pendingAttachmentCount > 0;
  const send = async () => {
    const prompt = text.trim();
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
      setText("");
      staging.setAttachments([]);
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
          if (attachment) staging.removeComposerAttachment(attachment);
        }}
      />
      <div className="assistant-composer-row">
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
          rows={1}
          value={text}
        />
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
