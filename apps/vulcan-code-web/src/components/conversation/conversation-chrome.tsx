import React from "react";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/utils";

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
    <div className="conversation-submission-status" aria-live="polite" aria-busy="true">
      <span className="conversation-submission-status-dot" aria-hidden="true" />
      <span>{label}</span>
    </div>
  );
}

// ConversationLoadingCenter renders the cache-miss conversation hydration placeholder.
// ConversationLoadingCenter 渲染无缓存会话水合时的居中占位提示。
export function ConversationLoadingCenter() {
  const { t } = useI18n();
  return (
    <section className="conversation-loading-center" aria-live="polite" aria-busy="true">
      <div className="conversation-loading-card">
        <div className="conversation-loading-heading">
          <strong>{t("conversation.loading.title")}</strong>
        </div>
        <span className="shimmer">{t("conversation.loading.requesting")}</span>
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
    <section className={cn("empty-conversation-prompt", props.leaving && "leaving")} aria-hidden={props.leaving}>
      <h2>{promptText}</h2>
    </section>
  );
}
