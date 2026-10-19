import React from "react";
import { useI18n } from "@/i18n";
import { Progress } from "@/ui-kit/primitives/progress";
import type { DraftConversationPendingSubmission } from "@/components/workspace/workspace-resizable-body";

// ConversationScrollTail renders the end of the transcript scroll content: the tail reserve and the sticky footer.
// ConversationScrollTail 渲染转录滚动内容的末尾：尾部预留与粘性页脚。
// The scroll controller sizes the reserve so a followed tail never jumps back when the transcript shrinks.
// 滚动控制器调整预留高度，使跟随中的尾部在转录收缩时不会回跳。
export function ConversationScrollTail(props: {
  // Optional footer pinned to the bottom of the scrollport.
  // 固定在滚动视口底部的可选页脚。
  footer?: React.ReactNode;
}) {
  return (
    <>
      <div className="vk-transcript-tail" aria-hidden="true" />
      {props.footer ? (
        <div className="vk-transcript-foot" data-conversation-scroll-footer="true">
          {props.footer}
        </div>
      ) : null}
    </>
  );
}

// ConversationSubmissionStatus renders one bounded, non-blocking first-submit preparation stage.
// ConversationSubmissionStatus 渲染首条提交的有界非阻塞准备阶段。
export function ConversationSubmissionStatus(props: {
  // Current optimistic submission stage.
  // 当前乐观提交阶段。
  stage: "creating" | "preparing";
}) {
  const { t } = useI18n();
  const label = props.stage === "creating"
    ? t("conversation.submission.creating")
    : t("conversation.submission.preparing");
  return (
    <div className="vk-submit-status" aria-live="polite" aria-busy="true">
      <span className="vk-dot" data-tone="ember" data-live="" aria-hidden="true" />
      <span>{label}</span>
    </div>
  );
}

// ConversationLoadingCenter renders the cache-miss conversation hydration placeholder.
// ConversationLoadingCenter 渲染无缓存会话水合时的居中占位提示。
export function ConversationLoadingCenter() {
  const { t } = useI18n();
  return (
    <section className="vk-chat-loading" aria-live="polite" aria-busy="true">
      <div className="vk-chat-loading-card">
        <strong>{t("conversation.loading.title")}</strong>
        <span className="vk-shimmer-text">{t("conversation.loading.requesting")}</span>
        <Progress aria-hidden="true" />
      </div>
    </section>
  );
}

// DraftSubmissionTranscript shows the accepted first prompt before the durable session and V1 owner exist.
// DraftSubmissionTranscript 在持久会话与 V1 归属建立前展示已接纳的首条提示词。
// Parameters: submission is the exact pending draft action; surfaceId is its current draft or durable id; scrollRef owns the timeline viewport.
// 参数：submission 是精确的待处理草稿动作；surfaceId 是当前草稿或持久标识；scrollRef 持有时间线视口。
// Returns: a transcript-shaped optimistic user row that can hand off to the canonical renderer.
// 返回值：可交接给规范渲染器的转录形态乐观用户行。
export function DraftSubmissionTranscript(props: {
  submission: DraftConversationPendingSubmission;
  surfaceId: string;
  scrollRef: React.Ref<HTMLElement>;
}) {
  return (
    <section className="vk-transcript" data-draft="" data-conversation-surface-id={props.surfaceId} ref={props.scrollRef}>
      <div className="vk-transcript-inner">
        <div className="vk-timeline">
          <article className="vk-prompt-row" data-client-action-id={props.submission.clientSubmissionId}>
            {props.submission.attachments.length > 0 ? (
              <ul className="vk-prompt-files">
                {props.submission.attachments.map((attachment) => <li key={attachment.fileName}>{attachment.fileName}</li>)}
              </ul>
            ) : null}
            <div className="vk-prompt"><p className="vk-prompt-text">{props.submission.prompt}</p></div>
          </article>
        </div>
      </div>
    </section>
  );
}
// EmptyConversationPrompt renders the blank-chat prompt above the shared composer dock.
// EmptyConversationPrompt 在共享输入区上方渲染空会话提示。
export function EmptyConversationPrompt(props: {
  // Whether the prompt is playing its exit animation.
  // 当前提示是否正在播放退出动画。
  leaving: boolean;
  // Project name used by the empty-state prompt.
  // 空状态提示使用的项目名称。
  projectName: string;
}) {
  // t translates the empty conversation prompt while projectName remains user data.
  // t 翻译空会话引导语，projectName 仍保留为用户数据。
  const { t } = useI18n();
  // promptText inserts the selected project name into the localized empty prompt.
  // promptText 将当前项目名称插入本地化空会话提示。
  const promptText = t("console.emptyPrompt").replace("{projectName}", props.projectName);

  return (
    <section className="vk-empty-prompt" data-leaving={props.leaving ? "" : undefined} aria-hidden={props.leaving}>
      <h2 title={promptText}>{promptText}</h2>
    </section>
  );
}
