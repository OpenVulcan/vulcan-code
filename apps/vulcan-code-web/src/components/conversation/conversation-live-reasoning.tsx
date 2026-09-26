import React from "react";
import ReactMarkdown from "react-markdown";
import { Columns2, PanelRightOpen } from "lucide-react";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { useI18n } from "@/i18n";
import { formatTokenCount } from "@/lib/usage-format";
import { cn } from "@/lib/utils";
import type { ConversationBlock } from "@/components/conversation/conversation-model";
import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import {
  loadStreamingThinkingLayoutMode,
  nextStreamingThinkingLayoutMode,
  persistStreamingThinkingLayoutMode,
} from "@/components/conversation/streaming-thinking-layout";
import { useStreamingThinkingViewport } from "@/components/conversation/use-streaming-thinking-viewport";
import type { TransientReasoningStatus } from "@/types";

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
  // summaries preserves provider node order and discards empty transport fragments.
  // summaries 保留供应商节点顺序，并排除空传输片段。
  const summaries = props.status.kind === "summary"
    ? (props.status.summaries ?? []).filter((summary) => summary.text.trim().length > 0)
    : [];
  // rows shows one node per standard summary part, or one encrypted fallback before any summary exists.
  // rows 为每个标准摘要部分显示一个节点；在摘要出现前则显示一个加密回退节点。
  const rows = summaries.length > 0
    ? summaries.map((summary, index) => ({
        id: summary.itemId,
        text: `${summary.text}${index === summaries.length - 1 ? tokenSuffix : ""}`,
      }))
    : [{ id: "encrypted-progress", text: `${t("runtime.thinking")}${tokenSuffix}` }];
  return (
    <div
      aria-busy="true"
      aria-live="polite"
      className="transient-reasoning-status-stack"
      data-reasoning-status-kind={props.status.kind}
    >
      {rows.map((row) => (
        <div
          className="tool-execution-row transient-reasoning-status-line"
          key={row.id}
        >
          <div className="shimmer transient-reasoning-status-copy">
            <ReactMarkdown
              allowedElements={["p", "strong", "em", "br", "code"]}
              unwrapDisallowed
            >
              {row.text}
            </ReactMarkdown>
          </div>
        </div>
      ))}
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
      className={cn(
        "streaming-thinking-overlay",
        layoutMode === "right_docked" && "right-docked",
        props.leaving && "leaving",
      )}
      aria-live="polite"
      aria-busy={!props.leaving}
    >
      <header className="streaming-thinking-header">
        <span className="streaming-thinking-title">
          <span className="shimmer">{t("runtime.thinking")}</span>
        </span>
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              aria-label={layoutToggleLabel}
              className="streaming-thinking-layout-toggle"
              onClick={handleLayoutToggle}
              type="button"
            >
              {layoutMode === "centered" ? <PanelRightOpen size={16} /> : <Columns2 size={16} />}
            </button>
          </TooltipTrigger>
          <TooltipContent side="left">{layoutToggleLabel}</TooltipContent>
        </Tooltip>
      </header>
      <div
        className="streaming-thinking-body"
        onPointerDown={thinkingViewport.handleBodyPointerDown}
        onScroll={thinkingViewport.handleBodyScroll}
        onWheel={thinkingViewport.handleBodyWheel}
        ref={thinkingViewport.bodyRef}
      >
        <div className="streaming-thinking-content" ref={thinkingViewport.contentRef}>
          <MarkdownMessage
            content={props.block.body || t("runtime.thinking")}
            eager
            streaming={!props.leaving}
          />
        </div>
      </div>
    </section>
  );
}
