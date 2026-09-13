import * as React from "react";
import { Sparkles } from "lucide-react";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { useI18n } from "@/i18n";
import { useStreamingThinkingViewport } from "./use-streaming-thinking-viewport";

// ThinkingPopupContext shares the popup preference across independent transcript renderers.
// ThinkingPopupContext 在独立转录渲染器之间共享弹窗偏好。
export const ThinkingPopupContext = React.createContext(false);

// ThinkingPreview retains complete reasoning behind a compact, hoverable and clickable entry.
// ThinkingPreview 通过紧凑、可悬停和点击的入口保留完整思考。
// children is the existing Markdown renderer; streaming controls auto-open lifecycle; compact selects an icon-only tool prefix.
// children 为现有 Markdown 渲染器；streaming 控制自动弹出生命周期；compact 选择工具前的纯图标入口。
// Returns a nonmodal reading surface with bounded natural height and independent internal scrolling.
// 返回自然高度受限、独立内部滚动的非模态阅读浮层。
export function ThinkingPreview(props: { children: React.ReactNode; streaming?: boolean; compact?: boolean }) {
  // t supplies localized labels and popupEnabled governs automatic opening only.
  // t 提供本地化文案，popupEnabled 仅控制自动打开。
  const { t } = useI18n();
  const popupEnabled = React.useContext(ThinkingPopupContext);
  // automatic opens only while the current reasoning is live in popup mode.
  // automatic 仅在弹窗模式下当前思考仍在生成时自动打开。
  const automatic = popupEnabled && Boolean(props.streaming);
  const [open, setOpen] = React.useState(automatic);
  // pinned keeps click-open content available while the pointer leaves to select or copy text.
  // pinned 保持点击打开的内容可用，便于移开指针后选择和复制文字。
  const pinned = React.useRef(false);
  // Timer handles separate delayed hover entry from the trigger-to-popup exit grace period.
  // 定时器句柄分别管理悬停延迟进入和入口到浮窗之间的退出宽限。
  const openTimer = React.useRef<ReturnType<typeof setTimeout> | null>(null);
  const closeTimer = React.useRef<ReturnType<typeof setTimeout> | null>(null);
  const viewport = useStreamingThinkingViewport({ enabled: open && Boolean(props.streaming) });
  // cancelHoverTimers clears both pending hover actions; takes no parameters and returns nothing.
  // cancelHoverTimers 清除两个待执行悬停动作；无参数且无返回值。
  const cancelHoverTimers = React.useCallback(() => {
    if (openTimer.current !== null) clearTimeout(openTimer.current);
    if (closeTimer.current !== null) clearTimeout(closeTimer.current);
    openTimer.current = null;
    closeTimer.current = null;
  }, []);
  React.useEffect(() => { cancelHoverTimers(); setOpen(automatic); pinned.current = false; }, [automatic, cancelHoverTimers]);
  React.useEffect(() => cancelHoverTimers, [cancelHoverTimers]);
  // enter waits for sustained hover before opening; already open content remains visible. Returns nothing.
  // enter 等待持续悬停后打开；已经打开的内容保持可见。无返回值。
  const enter = () => {
    cancelHoverTimers();
    if (open) return;
    openTimer.current = setTimeout(() => { openTimer.current = null; setOpen(true); }, 500);
  };
  // leave bridges the trigger-to-portal pointer gap without closing pinned or live content.
  // leave 容许指针穿过入口与 Portal 间隙，不关闭固定或实时内容。
  const leave = () => {
    cancelHoverTimers();
    closeTimer.current = setTimeout(() => { if (!pinned.current && !automatic) setOpen(false); }, 180);
  };
  return (
    <Popover open={open} onOpenChange={(next) => { cancelHoverTimers(); pinned.current = next; setOpen(next); }}>
      <PopoverTrigger asChild>
        <button type="button" className={`thinking-preview-trigger${props.compact ? " compact" : ""}`}
          aria-label={t("contextCompaction.thinkingLabel")} onMouseEnter={enter} onMouseLeave={leave}
          onClick={(event) => { event.preventDefault(); cancelHoverTimers(); pinned.current = !pinned.current; setOpen(pinned.current); }}>
          <Sparkles size={15} aria-hidden="true" />
          {!props.compact ? <span>{t(props.streaming ? "contextCompaction.thinkingActive" : "contextCompaction.thinkingLabel")}</span> : null}
        </button>
      </PopoverTrigger>
      <PopoverContent className="thinking-preview-surface" align="start" side="bottom" collisionPadding={16}
        aria-label={t("contextCompaction.thinkingLabel")}
        onOpenAutoFocus={(event) => event.preventDefault()} onCloseAutoFocus={(event) => event.preventDefault()}
        onMouseEnter={enter} onMouseLeave={leave} onPointerDown={() => { pinned.current = true; }}>
        <div className="thinking-preview-body" ref={viewport.bodyRef} onScroll={viewport.handleBodyScroll}
          onPointerDown={viewport.handleBodyPointerDown} onWheel={viewport.handleBodyWheel}>
          <div ref={viewport.contentRef}>{props.children}</div>
        </div>
      </PopoverContent>
    </Popover>
  );
}
