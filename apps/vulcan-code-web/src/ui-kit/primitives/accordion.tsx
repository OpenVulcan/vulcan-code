import * as React from "react";
import { ChevronDown } from "lucide-react";
import { cx, useStableId } from "../behavior/core";

// Accordion is a stack of collapsible items; "single" keeps at most one open, "multiple" any number.
// Accordion 是一组可折叠项；"single" 最多展开一项，"multiple" 可展开任意多项。

// AccordionContextValue tracks which item values are open.
// AccordionContextValue 记录哪些项值处于展开状态。
interface AccordionContextValue {
  isOpen: (value: string) => boolean;
  toggle: (value: string) => void;
}

const AccordionContext = React.createContext<AccordionContextValue | null>(null);
const AccordionItemContext = React.createContext<{ value: string; open: boolean; id: string; disabled: boolean } | null>(null);

// AccordionProps follow the Radix accordion for both single and multiple modes.
// AccordionProps 沿用 Radix 手风琴的单选与多选模式。
export type AccordionProps = Omit<React.ComponentProps<"div">, "defaultValue"> & (
  | { type: "single"; value?: string; defaultValue?: string; onValueChange?: (value: string) => void; collapsible?: boolean }
  | { type: "multiple"; value?: string[]; defaultValue?: string[]; onValueChange?: (value: string[]) => void; collapsible?: never }
);

// Accordion renders the item stack.
// Accordion 渲染项堆叠。
export function Accordion(props: AccordionProps) {
  const { type, value, defaultValue, onValueChange, collapsible, className, ...rest } = props as AccordionProps & {
    value?: string | string[];
    defaultValue?: string | string[];
    onValueChange?: (value: string | string[]) => void;
    collapsible?: boolean;
  };
  const initial = defaultValue ?? (type === "single" ? "" : []);
  const [own, setOwn] = React.useState<string | string[]>(initial);
  const current = value ?? own;
  const context = React.useMemo<AccordionContextValue>(() => ({
    isOpen: (item) => (Array.isArray(current) ? current.includes(item) : current === item),
    toggle: (item) => {
      let next: string | string[];
      if (type === "single") {
        if (current === item) {
          if (!collapsible) return;
          next = "";
        } else next = item;
      } else {
        const list = Array.isArray(current) ? current : [];
        next = list.includes(item) ? list.filter((entry) => entry !== item) : [...list, item];
      }
      if (value === undefined) setOwn(next);
      onValueChange?.(next);
    },
  }), [current, type, collapsible, value, onValueChange]);
  return (
    <AccordionContext.Provider value={context}>
      <div data-slot="accordion" data-orientation="vertical" className={cx("vk-accordion", className)} {...rest} />
    </AccordionContext.Provider>
  );
}

// AccordionItem is one collapsible entry identified by value.
// AccordionItem 是以 value 标识的一个可折叠条目。
export function AccordionItem({ value, disabled = false, className, ...props }: React.ComponentProps<"div"> & { value: string; disabled?: boolean }) {
  const accordion = React.useContext(AccordionContext);
  if (!accordion) throw new Error("AccordionItem must be rendered inside an Accordion.");
  const open = accordion.isOpen(value);
  const id = useStableId("vk-accordion");
  return (
    <AccordionItemContext.Provider value={{ value, open, id, disabled }}>
      <div data-slot="accordion-item" data-state={open ? "open" : "closed"} className={cx("vk-accordion-item", className)} {...props} />
    </AccordionItemContext.Provider>
  );
}

// AccordionTrigger is the heading button of an item.
// AccordionTrigger 是条目的标题按钮。
export function AccordionTrigger({ className, children, onClick, ...props }: React.ComponentProps<"button">) {
  const accordion = React.useContext(AccordionContext);
  const item = React.useContext(AccordionItemContext);
  if (!accordion || !item) throw new Error("AccordionTrigger must be rendered inside an AccordionItem.");
  return (
    <h3 className="vk-accordion-heading">
      <button
        type="button"
        id={`${item.id}-trigger`}
        aria-controls={`${item.id}-content`}
        aria-expanded={item.open}
        data-state={item.open ? "open" : "closed"}
        data-slot="accordion-trigger"
        disabled={item.disabled || undefined}
        className={cx("vk-accordion-trigger", className)}
        onClick={(event) => {
          onClick?.(event);
          if (!event.defaultPrevented) accordion.toggle(item.value);
        }}
        {...props}
      >
        {children}
        <ChevronDown data-slot="accordion-trigger-icon" aria-hidden="true" className="vk-disclosure-chevron" />
      </button>
    </h3>
  );
}

// AccordionContent is the item body, rendered while open.
// AccordionContent 是条目正文，仅在展开时渲染。
export function AccordionContent({ className, children, ...props }: React.ComponentProps<"div">) {
  const item = React.useContext(AccordionItemContext);
  if (!item) throw new Error("AccordionContent must be rendered inside an AccordionItem.");
  if (!item.open) return null;
  return (
    <div
      role="region"
      id={`${item.id}-content`}
      aria-labelledby={`${item.id}-trigger`}
      data-state="open"
      data-slot="accordion-content"
      className="vk-accordion-content"
      {...props}
    >
      <div className={cx("vk-accordion-body", className)}>{children}</div>
    </div>
  );
}
