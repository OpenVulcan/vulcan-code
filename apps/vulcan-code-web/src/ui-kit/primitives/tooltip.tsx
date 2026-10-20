import * as React from "react";
import { composeHandlers, cx, useComposedRefs, useControllableState, useStableId } from "../behavior/core";
import { FloatingContent } from "../behavior/floating-content";
import type { FloatingAlign, FloatingSide } from "../behavior/floating";
import { Slot } from "../behavior/slot";

// Tooltips name an icon or explain a control after a short hover, or at once on keyboard focus. Moving
// between neighbouring triggers opens the next tooltip instantly. Focus that follows a pointer press stays
// quiet, so clicking a toolbar button does not leave a tooltip hanging; the next keyboard navigation key
// restores focus tooltips.
// 提示在短暂悬停后、或键盘聚焦时立即为图标命名或解释控件。在相邻触发器之间移动时下一个提示立即打开。
// 指针按下引起的聚焦保持安静，避免点击工具栏按钮后提示悬挂；下一次键盘导航键会恢复聚焦提示。

// pointerFocusSuppression holds triggers focused by a pointer press since the last navigation key.
// pointerFocusSuppression 保存自上次导航键以来因指针按下而获得焦点的触发器。
let pointerFocusSuppression = new WeakSet<HTMLElement>();

// NAVIGATION_KEYS release the suppression because they signal keyboard use.
// NAVIGATION_KEYS 表示正在使用键盘，因此会解除抑制。
const NAVIGATION_KEYS = new Set(["Tab", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Home", "End"]);

// releaseListenerAttached ensures the shared keydown listener is attached once.
// releaseListenerAttached 确保共享键盘监听器只挂载一次。
let releaseListenerAttached = false;

// attachReleaseListener starts clearing the suppression on navigation keys.
// attachReleaseListener 开始在导航键按下时清除抑制。
function attachReleaseListener(): void {
  if (releaseListenerAttached || typeof document === "undefined") return;
  releaseListenerAttached = true;
  document.addEventListener("keydown", (event) => {
    if (NAVIGATION_KEYS.has(event.key)) pointerFocusSuppression = new WeakSet<HTMLElement>();
  }, true);
}

// ProviderContextValue shares delays and the instant-reopen window among tooltips.
// ProviderContextValue 在提示之间共享延迟与立即重开窗口。
interface ProviderContextValue {
  delayDuration: number;
  skipDelayDuration: number;
  lastClosedAt: React.RefObject<number>;
}

// TOOLTIP_DELAY_MS and TOOLTIP_SKIP_DELAY_MS are the one tooltip timing: the first open waits, and a neighbour
// opened within the skip window shows at once.
// TOOLTIP_DELAY_MS 与 TOOLTIP_SKIP_DELAY_MS 是唯一的提示计时：首次打开需等待，跳过窗口内打开的相邻提示立即显示。
const TOOLTIP_DELAY_MS = 400;
const TOOLTIP_SKIP_DELAY_MS = 300;

// DEFAULT_PROVIDER applies when no TooltipProvider wraps a tooltip.
// DEFAULT_PROVIDER 在提示外层没有 TooltipProvider 时使用。
const DEFAULT_PROVIDER: ProviderContextValue = { delayDuration: TOOLTIP_DELAY_MS, skipDelayDuration: TOOLTIP_SKIP_DELAY_MS, lastClosedAt: { current: 0 } };

const ProviderContext = React.createContext<ProviderContextValue>(DEFAULT_PROVIDER);

// TooltipProvider sets the hover delay for the tooltips inside it.
// TooltipProvider 为其内部的提示设置悬停延迟。
export function TooltipProvider({ delayDuration = TOOLTIP_DELAY_MS, skipDelayDuration = TOOLTIP_SKIP_DELAY_MS, children }: {
  delayDuration?: number;
  skipDelayDuration?: number;
  disableHoverableContent?: boolean;
  children?: React.ReactNode;
}) {
  const lastClosedAt = React.useRef(0);
  React.useEffect(attachReleaseListener, []);
  const value = React.useMemo(() => ({ delayDuration, skipDelayDuration, lastClosedAt }), [delayDuration, skipDelayDuration]);
  return <ProviderContext.Provider value={value}>{children}</ProviderContext.Provider>;
}

// TooltipState is "closed", or how the tooltip opened.
// TooltipState 为 "closed"，或表示提示的打开方式。
type TooltipState = "closed" | "delayed-open" | "instant-open";

// TooltipContextValue is the state of one tooltip.
// TooltipContextValue 是单个提示的状态。
interface TooltipContextValue {
  state: TooltipState;
  contentId: string;
  triggerRef: React.RefObject<HTMLElement | null>;
  openAfterDelay: () => void;
  openNow: () => void;
  close: () => void;
}

const TooltipContext = React.createContext<TooltipContextValue | null>(null);

// useTooltip reads the enclosing tooltip.
// useTooltip 读取外层提示。
function useTooltip(): TooltipContextValue {
  const context = React.useContext(TooltipContext);
  if (!context) throw new Error("Tooltip parts must be rendered inside a Tooltip.");
  return context;
}

// Tooltip holds one tooltip's open state and timers.
// Tooltip 保存单个提示的打开状态与计时器。
export function Tooltip({ open: controlled, defaultOpen = false, onOpenChange, delayDuration, children }: {
  open?: boolean;
  defaultOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
  delayDuration?: number;
  disableHoverableContent?: boolean;
  children?: React.ReactNode;
}) {
  const provider = React.useContext(ProviderContext);
  const [open, setOpen] = useControllableState(controlled, defaultOpen, onOpenChange);
  const [delayed, setDelayed] = React.useState(false);
  const timer = React.useRef(0);
  const contentId = useStableId("vk-tooltip");
  const triggerRef = React.useRef<HTMLElement | null>(null);
  React.useEffect(() => () => window.clearTimeout(timer.current), []);
  React.useEffect(attachReleaseListener, []);
  // Escape closes an open tooltip first and stops there; the next press reaches the layers below.
  // Esc 先关闭打开的提示并到此为止；下一次按键才会到达下层。
  React.useEffect(() => {
    if (!open) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.stopPropagation();
      window.clearTimeout(timer.current);
      setOpen(false);
    };
    document.addEventListener("keydown", onKeyDown, true);
    return () => document.removeEventListener("keydown", onKeyDown, true);
  }, [open, setOpen]);
  const context = React.useMemo<TooltipContextValue>(() => ({
    state: open ? (delayed ? "delayed-open" : "instant-open") : "closed",
    contentId,
    triggerRef,
    openNow: () => {
      window.clearTimeout(timer.current);
      setDelayed(false);
      setOpen(true);
    },
    openAfterDelay: () => {
      window.clearTimeout(timer.current);
      const wait = delayDuration ?? provider.delayDuration;
      if (Date.now() - provider.lastClosedAt.current < provider.skipDelayDuration || wait <= 0) {
        setDelayed(false);
        setOpen(true);
        return;
      }
      timer.current = window.setTimeout(() => {
        setDelayed(true);
        setOpen(true);
      }, wait);
    },
    close: () => {
      window.clearTimeout(timer.current);
      if (open) provider.lastClosedAt.current = Date.now();
      setOpen(false);
    },
  }), [open, delayed, contentId, delayDuration, provider, setOpen]);
  return <TooltipContext.Provider value={context}>{children}</TooltipContext.Provider>;
}

// TooltipTrigger wires hover, focus and press to its child; it is almost always used with asChild.
// TooltipTrigger 将悬停、聚焦与按下行为接到子元素上；几乎总是配合 asChild 使用。
export function TooltipTrigger({
  asChild,
  ref,
  onPointerMove,
  onPointerLeave,
  onPointerDown,
  onPointerDownCapture,
  onFocus,
  onFocusCapture,
  onBlur,
  onClick,
  ...props
}: React.ComponentProps<"button"> & { asChild?: boolean }) {
  const tooltip = useTooltip();
  const composed = useComposedRefs(tooltip.triggerRef as React.Ref<HTMLButtonElement>, ref);
  const suppressedFocus = React.useRef(false);
  const Comp = asChild ? Slot : "button";
  type Handler<E> = ((event: E) => void) | undefined;
  return (
    <Comp
      ref={composed as never}
      type={asChild ? undefined : "button"}
      aria-describedby={tooltip.state !== "closed" ? tooltip.contentId : undefined}
      data-state={tooltip.state}
      data-slot="tooltip-trigger"
      onPointerMove={composeHandlers(onPointerMove as Handler<React.PointerEvent<HTMLElement>>, (event) => {
        if (event.pointerType === "touch" || tooltip.state !== "closed") return;
        tooltip.openAfterDelay();
      })}
      onPointerLeave={composeHandlers(onPointerLeave as Handler<React.PointerEvent<HTMLElement>>, () => tooltip.close())}
      onPointerDownCapture={(event: React.PointerEvent<HTMLElement>) => {
        (onPointerDownCapture as Handler<React.PointerEvent<HTMLElement>>)?.(event);
        if (!event.defaultPrevented) pointerFocusSuppression.add(event.currentTarget);
      }}
      onPointerDown={composeHandlers(onPointerDown as Handler<React.PointerEvent<HTMLElement>>, () => tooltip.close())}
      onFocusCapture={(event: React.FocusEvent<HTMLElement>) => {
        (onFocusCapture as Handler<React.FocusEvent<HTMLElement>>)?.(event);
        suppressedFocus.current = !event.defaultPrevented && pointerFocusSuppression.has(event.currentTarget);
      }}
      onFocus={composeHandlers(onFocus as Handler<React.FocusEvent<HTMLElement>>, () => {
        if (!suppressedFocus.current) tooltip.openNow();
      })}
      onBlur={composeHandlers(onBlur as Handler<React.FocusEvent<HTMLElement>>, () => tooltip.close())}
      onClick={composeHandlers(onClick as Handler<React.MouseEvent<HTMLElement>>, () => tooltip.close())}
      {...(props as React.HTMLAttributes<HTMLElement>)}
    />
  );
}

// TooltipContentProps place the bubble around the trigger.
// TooltipContentProps 设置气泡相对触发器的位置。
export interface TooltipContentProps extends React.ComponentProps<"div"> {
  side?: FloatingSide;
  align?: FloatingAlign;
  sideOffset?: number;
  alignOffset?: number;
  collisionPadding?: number;
  showArrow?: boolean;
  forceMount?: boolean;
  onEscapeKeyDown?: (event: KeyboardEvent) => void;
  onPointerDownOutside?: (event: PointerEvent) => void;
}

// TooltipContent renders the bubble while open; Escape closes it.
// TooltipContent 在打开期间渲染气泡；Esc 关闭气泡。
export function TooltipContent({
  className,
  side = "top",
  align = "center",
  sideOffset = 6,
  alignOffset,
  collisionPadding,
  showArrow = true,
  forceMount: _forceMount,
  children,
  ...props
}: TooltipContentProps) {
  const tooltip = useTooltip();
  return (
    <FloatingContent
      open={tooltip.state !== "closed"}
      anchor={() => tooltip.triggerRef.current}
      onDismiss={tooltip.close}
      dismissable={false}
      focus={false}
      layer="tooltip"
      side={side}
      align={align}
      sideOffset={sideOffset}
      alignOffset={alignOffset}
      collisionPadding={collisionPadding}
      id={tooltip.contentId}
      role="tooltip"
      data-slot="tooltip-content"
      data-state={tooltip.state}
      data-arrow={showArrow ? "" : undefined}
      className={cx("vk-tooltip", className)}
      {...props}
    >
      {children}
    </FloatingContent>
  );
}
