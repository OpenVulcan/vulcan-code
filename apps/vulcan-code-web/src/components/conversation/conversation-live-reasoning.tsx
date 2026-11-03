import React from "react";
import { Brain, Columns2, PanelRightOpen } from "lucide-react";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/ui-kit/primitives/tooltip";
import { useI18n } from "@/i18n";
import { formatTokenCount } from "@/lib/usage-format";
import { IconButton } from "@/ui-kit/primitives/button";
import type { ConversationBlock } from "@/components/conversation/conversation-model";
import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import {
  loadStreamingThinkingLayoutMode,
  nextStreamingThinkingLayoutMode,
  persistStreamingThinkingLayoutMode,
} from "@/components/conversation/streaming-thinking-layout";
import { useStreamingThinkingViewport } from "@/components/conversation/use-streaming-thinking-viewport";
import {
  loadStreamingThinkingMarkdownEnabled,
  persistStreamingThinkingMarkdownEnabled,
} from "@/components/conversation/streaming-thinking-markdown";
import type { TransientReasoningStatus } from "@/types";
import { THINKING_OVERLAY_EXIT_MS } from "@/components/conversation/chat-area/visual-state";
import type { RuntimeCue } from "@/components/conversation/conversation-model";
import { formatWorkProcessDuration } from "@/components/conversation/conversation-timeline";

// RUNTIME_CUE_TICK_MS is the refresh interval of the elapsed time shown beside a runtime cue.
// RUNTIME_CUE_TICK_MS 是运行提示旁已等待时长的刷新间隔。
const RUNTIME_CUE_TICK_MS = 1000;

// RuntimeCueLine renders the session's in-progress cue at the transcript tail with the time spent in it.
// The clock starts when the cue mounts; callers key it by the cue text so each phase counts from zero.
// RuntimeCueLine 在转录尾部渲染会话进行中提示及其已持续时长。
// 计时从提示挂载开始；调用方以提示文本作为 key，使每个阶段从零计时。
export function RuntimeCueLine(props: {
  // Cue resolved from the backend session run phase.
  // 由后端会话运行阶段解析出的提示。
  cue: RuntimeCue;
}) {
  const { locale } = useI18n();
  const [startedAtMs] = React.useState(() => Date.now());
  const [nowMs, setNowMs] = React.useState(startedAtMs);
  React.useEffect(() => {
    const timer = window.setInterval(() => setNowMs(Date.now()), RUNTIME_CUE_TICK_MS);
    return () => window.clearInterval(timer);
  }, []);
  return (
    <div aria-busy="true" aria-live="polite" className="vk-reason-status" data-runtime-cue="">
      <div className="vk-reason-line">
        <span className="vk-shimmer-text">{props.cue.text}</span>
        <span className="vk-reason-elapsed">{formatWorkProcessDuration(nowMs - startedAtMs, locale)}</span>
      </div>
    </div>
  );
}

// TransientReasoningStatusLine renders request-local reasoning progress outside the durable event ledger.
// TransientReasoningStatusLine 在持久化事件账本之外渲染请求局部的思考进度。
export function TransientReasoningStatusLine(props: {
  // Latest request-local transient status.
  // 最新请求局部瞬态状态。
  status: TransientReasoningStatus;
}) {
  // t formats the status through the active interface language.
  // t 使用当前界面语言格式化状态。
  const { t } = useI18n();
  // tokenSuffix is omitted when the provider exposes no dynamic reasoning count.
  // tokenSuffix 在供应商未公开动态推理计数时省略。
  const tokenSuffix = typeof props.status.reasoningTokens === "number"
    ? t("runtime.thinkingTokenCount").replace(
        "{count}",
        formatTokenCount(props.status.reasoningTokens),
      )
    : "";
  return (
    <div
      aria-busy="true"
      aria-live="polite"
      className="vk-reason-status"
      data-reasoning-status-kind={props.status.kind}
    >
      <div className="vk-reason-line vk-shimmer-text">
        {`${t("runtime.thinking")}${tokenSuffix}`}
      </div>
    </div>
  );
}

// StreamingThinkingOverlay renders the live-only reasoning stream without depending on the legacy transcript renderer.
// StreamingThinkingOverlay 在不依赖旧转录渲染器的前提下渲染仅实时存在的思考流。
export function StreamingThinkingOverlay(props: {
  // Currently streaming reasoning block.
  // 当前正在流式输出的思考块。
  block: ConversationBlock;
  // Conversation identity used by the live stream store.
  // 实时流存储使用的会话身份。
  conversationId: string;
  // Whether the popup is in its fade-out phase.
  // 弹窗是否处于淡出阶段。
  leaving: boolean;
}) {
  // t resolves localized labels for the floating thinking surface.
  // t 解析浮动思考面板使用的本地化文案。
  const { t } = useI18n();
  // thinkingViewport supplies shared popup scrolling without duplicating live Markdown logic.
  // thinkingViewport 提供共享弹窗滚动行为，避免重复实时 Markdown 逻辑。
  const thinkingViewport = useStreamingThinkingViewport({ enabled: true });
  // layoutMode stores the globally remembered popup placement selected by the user.
  // layoutMode 保存用户选择并全局记忆的弹窗位置。
  const [layoutMode, setLayoutMode] = React.useState(loadStreamingThinkingLayoutMode);
  // markdownEnabled remembers only the automatic popup's presentation, leaving manual previews unchanged.
  // markdownEnabled 仅记忆自动弹窗的显示方式，不影响手动预览。
  const [markdownEnabled, setMarkdownEnabled] = React.useState(loadStreamingThinkingMarkdownEnabled);

  // handleMarkdownToggle takes no arguments and switches and saves this client's popup rendering preference.
  // handleMarkdownToggle 无参数，切换并保存当前客户端的弹窗渲染偏好。
  // Returns nothing; persistence follows only an explicit click, never incoming stream updates.
  // 无返回值；仅明确点击时持久化，流式内容更新不会写入偏好。
  const handleMarkdownToggle = React.useCallback(() => {
    // nextEnabled is the exact selection applied to both the popup and persistent storage.
    // nextEnabled 是同时应用于弹窗与持久存储的明确选择。
    const nextEnabled = !markdownEnabled;
    setMarkdownEnabled(nextEnabled);
    persistStreamingThinkingMarkdownEnabled(nextEnabled);
  }, [markdownEnabled]);

  // markdownToggleLabel describes the next rendering action in the current interface language.
  // markdownToggleLabel 以当前界面语言描述下一次渲染切换动作。
  const markdownToggleLabel = markdownEnabled
    ? t("runtime.thinkingOverlayMarkdownDisable")
    : t("runtime.thinkingOverlayMarkdownEnable");

  // handleLayoutToggle switches and persists the popup placement.
  // handleLayoutToggle 切换并持久化弹窗位置。
  const handleLayoutToggle = React.useCallback(() => {
    setLayoutMode((currentMode) => {
      const nextMode = nextStreamingThinkingLayoutMode(currentMode);
      persistStreamingThinkingLayoutMode(nextMode);
      return nextMode;
    });
  }, []);

  // layoutToggleLabel describes the action performed by the header button.
  // layoutToggleLabel 描述标题栏按钮将执行的布局动作。
  const layoutToggleLabel = layoutMode === "centered"
    ? t("runtime.thinkingOverlayDockRight")
    : t("runtime.thinkingOverlayRestoreCentered");

  return (
    <section
      className="vk-think-float"
      data-dock={layoutMode === "right_docked" ? "right" : "center"}
      data-leaving={props.leaving || undefined}
      style={{ "--vk-think-exit": `${THINKING_OVERLAY_EXIT_MS}ms` } as React.CSSProperties}
      aria-live="polite"
      aria-busy={!props.leaving}
    >
      <header className="vk-think-float-head">
        <Brain aria-hidden="true" />
        <b>{t("runtime.thinking")}</b>
        <div className="vk-actions">
          <Tooltip>
            <TooltipTrigger asChild>
              <IconButton
                aria-label={markdownToggleLabel}
                aria-pressed={markdownEnabled}
                onClick={handleMarkdownToggle}
                type="button"
              >
                <span aria-hidden="true" className="vk-think-md">MD</span>
              </IconButton>
            </TooltipTrigger>
            <TooltipContent side="left">{markdownToggleLabel}</TooltipContent>
          </Tooltip>
          <Tooltip>
            <TooltipTrigger asChild>
              <IconButton
                aria-label={layoutToggleLabel}
                onClick={handleLayoutToggle}
                type="button"
              >
                {layoutMode === "centered" ? <PanelRightOpen aria-hidden="true" /> : <Columns2 aria-hidden="true" />}
              </IconButton>
            </TooltipTrigger>
            <TooltipContent side="left">{layoutToggleLabel}</TooltipContent>
          </Tooltip>
        </div>
      </header>
      <div
        className="vk-think-float-body"
        data-plain={markdownEnabled ? undefined : ""}
        onPointerDown={thinkingViewport.handleBodyPointerDown}
        onScroll={thinkingViewport.handleBodyScroll}
        onWheel={thinkingViewport.handleBodyWheel}
        ref={thinkingViewport.bodyRef}
      >
        <div className="vk-think-float-content" ref={thinkingViewport.contentRef}>
          {markdownEnabled ? (
            <MarkdownMessage
              content={props.block.body || t("runtime.thinking")}
              eager
              streaming={!props.leaving}
            />
          ) : (
            <>{props.block.body || t("runtime.thinking")}</>
          )}
        </div>
      </div>
    </section>
  );
}
