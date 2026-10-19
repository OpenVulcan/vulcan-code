import * as React from "react";
import { composeHandlers, cx, useComposedRefs, useControllableState, useStableId } from "../behavior/core";
import { FloatingContent } from "../behavior/floating-content";
import type { FloatingAlign, FloatingSide } from "../behavior/floating";
import { Slot } from "../behavior/slot";

// Popovers are non-modal anchored panels for small inline tasks; focus moves in on open and returns on close.
// 弹出层是用于小型内联任务的非模态锚定面板；打开时焦点移入，关闭时焦点归还。

// FloatingContentWidth is the width strategy shared by popovers, menus and hover cards.
// FloatingContentWidth 是弹出层、菜单与悬停卡片共用的宽度策略。
export type FloatingContentWidth = "trigger" | "content" | "sm" | "md" | "lg";

// PopoverContextValue holds the open state and the anchor of one popover.
// PopoverContextValue 保存一个弹出层的打开状态与锚点。
interface PopoverContextValue {
  open: boolean;
  setOpen: (open: boolean) => void;
  contentId: string;
  triggerRef: React.RefObject<HTMLElement | null>;
  anchorRef: React.RefObject<HTMLElement | null>;
}

const PopoverContext = React.createContext<PopoverContextValue | null>(null);

// usePopover reads the enclosing popover.
// usePopover 读取外层弹出层。
function usePopover(): PopoverContextValue {
  const context = React.useContext(PopoverContext);
  if (!context) throw new Error("Popover parts must be rendered inside a Popover.");
  return context;
}

// Popover provides the open state.
// Popover 提供打开状态。
export function Popover({ open: controlled, defaultOpen = false, onOpenChange, children }: {
  open?: boolean;
  defaultOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
  modal?: boolean;
  children?: React.ReactNode;
}) {
  const [open, setOpen] = useControllableState(controlled, defaultOpen, onOpenChange);
  const contentId = useStableId("vk-popover");
  const triggerRef = React.useRef<HTMLElement | null>(null);
  const anchorRef = React.useRef<HTMLElement | null>(null);
  const context = React.useMemo(() => ({ open, setOpen, contentId, triggerRef, anchorRef }), [open, setOpen, contentId]);
  return <PopoverContext.Provider value={context}>{children}</PopoverContext.Provider>;
}

// PopoverTrigger toggles the popover on click.
// PopoverTrigger 在点击时切换弹出层。
export function PopoverTrigger({ asChild, onClick, ref, ...props }: React.ComponentProps<"button"> & { asChild?: boolean }) {
  const popover = usePopover();
  const composed = useComposedRefs(popover.triggerRef as React.Ref<HTMLButtonElement>, ref);
  const Comp = asChild ? Slot : "button";
  return (
    <Comp
      ref={composed as never}
      type={asChild ? undefined : "button"}
      aria-haspopup="dialog"
      aria-expanded={popover.open}
      aria-controls={popover.open ? popover.contentId : undefined}
      data-state={popover.open ? "open" : "closed"}
      data-slot="popover-trigger"
      onClick={composeHandlers(onClick as React.MouseEventHandler<HTMLElement> | undefined, () => popover.setOpen(!popover.open))}
      {...(props as React.HTMLAttributes<HTMLElement>)}
    />
  );
}

// PopoverAnchor positions the popover against an element other than the trigger.
// PopoverAnchor 让弹出层相对触发器以外的元素定位。
export function PopoverAnchor({ asChild, ref, ...props }: React.ComponentProps<"div"> & { asChild?: boolean }) {
  const popover = usePopover();
  const composed = useComposedRefs(popover.anchorRef as React.Ref<HTMLDivElement>, ref);
  const Comp = asChild ? Slot : "div";
  return <Comp ref={composed as never} data-slot="popover-anchor" {...(props as React.HTMLAttributes<HTMLElement>)} />;
}

// PopoverContentProps follow the Radix popover content.
// PopoverContentProps 沿用 Radix 弹出层内容。
export interface PopoverContentProps extends React.ComponentProps<"div"> {
  side?: FloatingSide;
  align?: FloatingAlign;
  sideOffset?: number;
  alignOffset?: number;
  collisionPadding?: number;
  width?: Exclude<FloatingContentWidth, "trigger">;
  forceMount?: boolean;
  onOpenAutoFocus?: (event: Event) => void;
  onCloseAutoFocus?: (event: Event) => void;
  onEscapeKeyDown?: (event: KeyboardEvent) => void;
  onPointerDownOutside?: (event: PointerEvent) => void;
  onFocusOutside?: (event: FocusEvent) => void;
  onInteractOutside?: (event: PointerEvent | FocusEvent) => void;
}

// PopoverContent renders the anchored panel while open.
// PopoverContent 在打开期间渲染锚定面板。
export function PopoverContent({
  className,
  side = "bottom",
  align = "center",
  sideOffset = 4,
  alignOffset,
  collisionPadding,
  width = "lg",
  forceMount: _forceMount,
  onOpenAutoFocus,
  onCloseAutoFocus,
  onInteractOutside,
  ...props
}: PopoverContentProps) {
  const popover = usePopover();
  const interactedOutside = React.useRef(false);
  return (
    <FloatingContent
      open={popover.open}
      anchor={() => popover.anchorRef.current ?? popover.triggerRef.current}
      onDismiss={() => popover.setOpen(false)}
      branches={[popover.triggerRef]}
      dismissOnFocusOutside
      onInteractOutside={(event) => {
        onInteractOutside?.(event);
        interactedOutside.current = !event.defaultPrevented;
      }}
      // Focus goes back to the trigger on close, unless the user closed the panel by moving on elsewhere.
      // 关闭时焦点回到触发器，除非用户是通过转向别处关闭面板的。
      focus={{
        onOpenAutoFocus,
        returnFocus: () => popover.triggerRef.current,
        onCloseAutoFocus: (event) => {
          const outside = interactedOutside.current;
          interactedOutside.current = false;
          onCloseAutoFocus?.(event);
          if (outside && !event.defaultPrevented) event.preventDefault();
        },
      }}
      side={side}
      align={align}
      sideOffset={sideOffset}
      alignOffset={alignOffset}
      collisionPadding={collisionPadding}
      cssVarPrefix="popover"
      id={popover.contentId}
      role="dialog"
      data-slot="popover-content"
      data-width={width}
      className={cx("vk-pop vk-popover", className)}
      {...props}
    />
  );
}

// PopoverHeader stacks a title and description at the top of the panel.
// PopoverHeader 在面板顶部堆叠标题与说明。
export function PopoverHeader({ className, ...props }: React.ComponentProps<"div">) {
  return <div data-slot="popover-header" className={cx("vk-popover-header", className)} {...props} />;
}

// PopoverTitle names the panel.
// PopoverTitle 为面板命名。
export function PopoverTitle({ className, ...props }: React.ComponentProps<"h2">) {
  return <h2 data-slot="popover-title" className={cx("vk-popover-title", className)} {...props} />;
}

// PopoverDescription explains the panel.
// PopoverDescription 说明面板用途。
export function PopoverDescription({ className, ...props }: React.ComponentProps<"p">) {
  return <p data-slot="popover-description" className={cx("vk-popover-description", className)} {...props} />;
}
