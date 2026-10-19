import * as React from "react";
import { composeHandlers, cx, useComposedRefs, useControllableState } from "../behavior/core";
import { FloatingContent } from "../behavior/floating-content";
import type { FloatingAlign, FloatingSide } from "../behavior/floating";
import { Slot } from "../behavior/slot";
import type { FloatingContentWidth } from "./popover";

// Hover cards preview richer detail when the pointer rests on a trigger; moving into the card keeps it open.
// 悬停卡片在指针停留于触发器时预览更丰富的细节；移入卡片会保持其打开。

// HoverCardContextValue is the open state and the delayed open and close actions.
// HoverCardContextValue 是打开状态以及延迟打开与关闭操作。
interface HoverCardContextValue {
  open: boolean;
  triggerRef: React.RefObject<HTMLElement | null>;
  scheduleOpen: () => void;
  scheduleClose: () => void;
  close: () => void;
}

const HoverCardContext = React.createContext<HoverCardContextValue | null>(null);

// useHoverCard reads the enclosing hover card.
// useHoverCard 读取外层悬停卡片。
function useHoverCard(): HoverCardContextValue {
  const context = React.useContext(HoverCardContext);
  if (!context) throw new Error("HoverCard parts must be rendered inside a HoverCard.");
  return context;
}

// HoverCard holds the open state and the hover delays.
// HoverCard 保存打开状态与悬停延迟。
export function HoverCard({ open: controlled, defaultOpen = false, onOpenChange, openDelay = 700, closeDelay = 300, children }: {
  open?: boolean;
  defaultOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
  openDelay?: number;
  closeDelay?: number;
  children?: React.ReactNode;
}) {
  const [open, setOpen] = useControllableState(controlled, defaultOpen, onOpenChange);
  const timer = React.useRef(0);
  const triggerRef = React.useRef<HTMLElement | null>(null);
  React.useEffect(() => () => window.clearTimeout(timer.current), []);
  const context = React.useMemo<HoverCardContextValue>(() => ({
    open,
    triggerRef,
    scheduleOpen: () => {
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => setOpen(true), openDelay);
    },
    scheduleClose: () => {
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => setOpen(false), closeDelay);
    },
    close: () => {
      window.clearTimeout(timer.current);
      setOpen(false);
    },
  }), [open, setOpen, openDelay, closeDelay]);
  return <HoverCardContext.Provider value={context}>{children}</HoverCardContext.Provider>;
}

// HoverCardTrigger opens the card after the pointer rests on it or keyboard focus reaches it.
// HoverCardTrigger 在指针停留或键盘焦点到达时打开卡片。
export function HoverCardTrigger({ asChild, ref, onMouseEnter, onMouseLeave, onFocus, onBlur, ...props }: React.ComponentProps<"a"> & { asChild?: boolean }) {
  const card = useHoverCard();
  const composed = useComposedRefs(card.triggerRef as React.Ref<HTMLAnchorElement>, ref);
  const Comp = asChild ? Slot : "a";
  type Handler<E> = ((event: E) => void) | undefined;
  return (
    <Comp
      ref={composed as never}
      data-state={card.open ? "open" : "closed"}
      data-slot="hover-card-trigger"
      onMouseEnter={composeHandlers(onMouseEnter as Handler<React.MouseEvent<HTMLElement>>, card.scheduleOpen)}
      onMouseLeave={composeHandlers(onMouseLeave as Handler<React.MouseEvent<HTMLElement>>, card.scheduleClose)}
      onFocus={composeHandlers(onFocus as Handler<React.FocusEvent<HTMLElement>>, card.scheduleOpen)}
      onBlur={composeHandlers(onBlur as Handler<React.FocusEvent<HTMLElement>>, card.scheduleClose)}
      {...(props as React.HTMLAttributes<HTMLElement>)}
    />
  );
}

// HoverCardContentProps place the card and choose its width.
// HoverCardContentProps 设置卡片位置并选择宽度。
export interface HoverCardContentProps extends React.ComponentProps<"div"> {
  side?: FloatingSide;
  align?: FloatingAlign;
  sideOffset?: number;
  alignOffset?: number;
  collisionPadding?: number;
  width?: Exclude<FloatingContentWidth, "trigger">;
  forceMount?: boolean;
}

// HoverCardContent renders the card while open.
// HoverCardContent 在打开期间渲染卡片。
export function HoverCardContent({
  className,
  side = "bottom",
  align = "center",
  sideOffset = 4,
  alignOffset,
  collisionPadding,
  width = "md",
  forceMount: _forceMount,
  onMouseEnter,
  onMouseLeave,
  ...props
}: HoverCardContentProps) {
  const card = useHoverCard();
  return (
    <FloatingContent
      open={card.open}
      anchor={() => card.triggerRef.current}
      onDismiss={card.close}
      branches={[card.triggerRef]}
      focus={false}
      side={side}
      align={align}
      sideOffset={sideOffset}
      alignOffset={alignOffset}
      collisionPadding={collisionPadding}
      cssVarPrefix="hover-card"
      data-slot="hover-card-content"
      data-width={width}
      className={cx("vk-pop vk-hover-card", className)}
      onMouseEnter={composeHandlers(onMouseEnter, () => card.scheduleOpen())}
      onMouseLeave={composeHandlers(onMouseLeave, card.scheduleClose)}
      {...props}
    />
  );
}
