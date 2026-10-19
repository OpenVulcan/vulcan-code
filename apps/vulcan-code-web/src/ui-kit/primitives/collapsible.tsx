import * as React from "react";
import { composeHandlers, useControllableState, useStableId } from "../behavior/core";
import { Slot } from "../behavior/slot";

// Collapsible is the open/closed state shared by a trigger and its content region; disclosure sections,
// accordions and inline toggles are built on it.
// Collapsible 是触发器与其内容区共享的展开/收起状态；折叠区、手风琴与行内展开都基于它。

// CollapsibleContextValue exposes the state to the trigger and content.
// CollapsibleContextValue 向触发器与内容区暴露状态。
interface CollapsibleContextValue {
  open: boolean;
  disabled: boolean;
  contentId: string;
  toggle: () => void;
}

const CollapsibleContext = React.createContext<CollapsibleContextValue | null>(null);

// useCollapsible reads the enclosing collapsible.
// useCollapsible 读取外层可折叠容器。
function useCollapsible(): CollapsibleContextValue {
  const context = React.useContext(CollapsibleContext);
  if (!context) throw new Error("Collapsible parts must be rendered inside a Collapsible.");
  return context;
}

// CollapsibleProps follow the Radix collapsible root.
// CollapsibleProps 沿用 Radix 可折叠根组件。
export interface CollapsibleProps extends React.ComponentProps<"div"> {
  open?: boolean;
  defaultOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
  disabled?: boolean;
}

// Collapsible renders the root element carrying data-state.
// Collapsible 渲染携带 data-state 的根元素。
export function Collapsible({ open: controlled, defaultOpen = false, onOpenChange, disabled = false, ...props }: CollapsibleProps) {
  const [open, setOpen] = useControllableState(controlled, defaultOpen, onOpenChange);
  const contentId = useStableId("vk-collapsible");
  const context = React.useMemo(() => ({ open, disabled, contentId, toggle: () => setOpen(!open) }), [open, disabled, contentId, setOpen]);
  return (
    <CollapsibleContext.Provider value={context}>
      <div data-state={open ? "open" : "closed"} data-disabled={disabled ? "" : undefined} {...props} />
    </CollapsibleContext.Provider>
  );
}

// CollapsibleTrigger toggles the region; asChild lends the behavior to a custom element.
// CollapsibleTrigger 切换区域；asChild 将行为赋予自定义元素。
export function CollapsibleTrigger({ asChild, onClick, ...props }: React.ComponentProps<"button"> & { asChild?: boolean }) {
  const context = useCollapsible();
  const Comp = asChild ? Slot : "button";
  return (
    <Comp
      type={asChild ? undefined : "button"}
      aria-controls={context.contentId}
      aria-expanded={context.open}
      data-state={context.open ? "open" : "closed"}
      data-disabled={context.disabled ? "" : undefined}
      disabled={context.disabled || undefined}
      onClick={composeHandlers(onClick as React.MouseEventHandler<HTMLElement> | undefined, () => context.toggle())}
      {...(props as React.HTMLAttributes<HTMLElement>)}
    />
  );
}

// CollapsibleContent renders while open; forceMount keeps it mounted and hidden while closed.
// CollapsibleContent 在展开时渲染；forceMount 让其在收起时保持挂载并隐藏。
export function CollapsibleContent({ forceMount, hidden, ...props }: React.ComponentProps<"div"> & { forceMount?: boolean }) {
  const context = useCollapsible();
  if (!context.open && !forceMount) return null;
  return (
    <div
      id={context.contentId}
      data-state={context.open ? "open" : "closed"}
      hidden={hidden ?? !context.open}
      {...props}
    />
  );
}
