import React from "react";
import { useI18n, type TranslationKey } from "@/i18n";
import { cn } from "@/lib/utils";
import type { ConversationHydrationProgress } from "@/types";

// ConversationLoadingCenter renders the cache-miss conversation hydration placeholder.
// ConversationLoadingCenter 渲染无缓存会话水合时的居中占位提示。
export function ConversationLoadingCenter(props: {
  // Current backend-projected loading progress.
  // 当前后端投影出的加载进度。
  progress: ConversationHydrationProgress | null;
}) {
  const { t } = useI18n();
  const percent = Math.max(0, Math.min(100, props.progress?.percent ?? 0));
  const phaseKey = conversationHydrationPhaseTranslationKey(props.progress?.phase);
  const phaseText = phaseKey ? t(phaseKey) : props.progress?.message ?? t("conversation.loading.requesting");
  return (
    <section className="conversation-loading-center" aria-live="polite" aria-busy="true">
      <div className="conversation-loading-card">
        <div className="conversation-loading-heading">
          <strong>{t("conversation.loading.title")}</strong>
          <span>{Math.round(percent)}%</span>
        </div>
        <div className="conversation-loading-bar" aria-label={t("conversation.loading.title")} role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(percent)}>
          <div className="conversation-loading-bar-value" style={{ width: `${percent}%` }} />
        </div>
        <span className="shimmer">{phaseText}</span>
      </div>
    </section>
  );
}
// conversationHydrationPhaseTranslationKey maps backend progress phase keys onto WebUI translation keys.
// conversationHydrationPhaseTranslationKey 将后端进度阶段键映射到 WebUI 翻译键。
function conversationHydrationPhaseTranslationKey(phase: string | null | undefined): TranslationKey | null {
  switch (phase) {
    case "locating":
      return "conversation.loading.locating";
    case "readingIndex":
      return "conversation.loading.index";
    case "readingMessages":
      return "conversation.loading.reading";
    case "loadingSubagents":
      return "conversation.loading.subagents";
    case "projecting":
      return "conversation.loading.projecting";
    case "done":
      return "conversation.loading.done";
    case "failed":
      return "conversation.loading.failed";
    default:
      return null;
  }
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
