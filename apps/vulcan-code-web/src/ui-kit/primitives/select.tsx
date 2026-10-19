import * as React from "react";
import { createPortal } from "react-dom";
import { CheckIcon, ChevronDownIcon } from "lucide-react";
import { composeHandlers, cx, focusElement, useComposedRefs, useControllableState, useStableId } from "../behavior/core";
import { FloatingContent } from "../behavior/floating-content";
import { listRows, moveInList } from "../behavior/roving";

// Select picks one value from a short list. The trigger shows the chosen row's own content: the selected row
// renders its text into the trigger through a portal, so rows with icons or secondary copy look the same in
// both places. While closed the rows stay mounted in a detached fragment so the trigger can still show them.
// 选择框从一个短列表中选取一个值。触发器显示所选行自身的内容：所选行通过门户把文本渲染进触发器，因此带图标或次要
// 文案的行在两处显示一致。关闭期间各行保持挂载在脱离文档的片段中，使触发器仍能显示它们。

// OPTION_SELECTOR matches the choosable rows of the listbox.
// OPTION_SELECTOR 匹配列表框中可选择的行。
const OPTION_SELECTOR = '[role="option"]';

// SelectContextValue is the state shared by the trigger, the value and the rows.
// SelectContextValue 是触发器、值与各行共享的状态。
interface SelectContextValue {
  value: string | undefined;
  setValue: (value: string) => void;
  open: boolean;
  setOpen: (open: boolean) => void;
  disabled: boolean;
  contentId: string;
  triggerRef: React.RefObject<HTMLButtonElement | null>;
  valueNode: HTMLElement | null;
  setValueNode: (node: HTMLElement | null) => void;
  valueHasChildren: boolean;
  setValueHasChildren: (has: boolean) => void;
  openedByKeyboard: React.RefObject<boolean>;
}

const SelectContext = React.createContext<SelectContextValue | null>(null);

// useSelect reads the enclosing select.
// useSelect 读取外层选择框。
function useSelect(): SelectContextValue {
  const context = React.useContext(SelectContext);
  if (!context) throw new Error("Select parts must be rendered inside a Select.");
  return context;
}

// SelectProps follow the Radix select root.
// SelectProps 沿用 Radix 选择框根组件。
export interface SelectProps {
  value?: string;
  defaultValue?: string;
  onValueChange?: (value: string) => void;
  open?: boolean;
  defaultOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
  disabled?: boolean;
  required?: boolean;
  name?: string;
  dir?: "ltr" | "rtl";
  children?: React.ReactNode;
}

// Select holds the value and the open state; with `name` it also submits the value in forms.
// Select 保存值与打开状态；提供 `name` 时还会在表单中提交该值。
function Select({ value: controlledValue, defaultValue, onValueChange, open: controlledOpen, defaultOpen = false, onOpenChange, disabled = false, required, name, children }: SelectProps) {
  const [value, setValue] = useControllableState<string | undefined>(controlledValue, defaultValue, onValueChange as ((value: string | undefined) => void) | undefined);
  const [open, setOpen] = useControllableState(controlledOpen, defaultOpen, onOpenChange);
  const [valueNode, setValueNode] = React.useState<HTMLElement | null>(null);
  const [valueHasChildren, setValueHasChildren] = React.useState(false);
  const contentId = useStableId("vk-select");
  const triggerRef = React.useRef<HTMLButtonElement | null>(null);
  const openedByKeyboard = React.useRef(false);
  const context = React.useMemo<SelectContextValue>(() => ({
    value,
    setValue: (next) => setValue(next),
    open,
    setOpen,
    disabled,
    contentId,
    triggerRef,
    valueNode,
    setValueNode,
    valueHasChildren,
    setValueHasChildren,
    openedByKeyboard,
  }), [value, setValue, open, setOpen, disabled, contentId, valueNode, valueHasChildren]);
  return (
    <SelectContext.Provider value={context}>
      {children}
      {name ? <input type="hidden" name={name} value={value ?? ""} required={required} disabled={disabled} /> : null}
    </SelectContext.Provider>
  );
}

// SelectGroup groups related rows under a label.
// SelectGroup 将相关行归入同一标签下。
function SelectGroup({ className, ...props }: React.ComponentProps<"div">) {
  return <div role="group" data-slot="select-group" className={className} {...props} />;
}

// SelectValue shows the selected row's content, or the placeholder while nothing is chosen.
// SelectValue 显示所选行的内容；尚未选择时显示占位文本。
function SelectValue({ placeholder, children, className, ref, ...props }: React.ComponentProps<"span"> & { placeholder?: React.ReactNode }) {
  const select = useSelect();
  const composed = useComposedRefs(select.setValueNode, ref);
  const hasChildren = children !== undefined && children !== null;
  const { setValueHasChildren } = select;
  React.useLayoutEffect(() => setValueHasChildren(hasChildren), [hasChildren, setValueHasChildren]);
  const empty = select.value === undefined || select.value === "";
  return (
    <span ref={composed} data-slot="select-value" className={cx("vk-select-value", className)} {...props}>
      {empty ? placeholder : children}
    </span>
  );
}

// SelectTrigger is the button that shows the value and opens the list.
// SelectTrigger 是显示值并打开列表的按钮。
function SelectTrigger({ className, size = "default", children, disabled, ref, onPointerDown, onClick, onKeyDown, ...props }: React.ComponentProps<"button"> & { size?: "sm" | "default" }) {
  const select = useSelect();
  const composed = useComposedRefs(select.triggerRef, ref);
  const pressed = React.useRef(false);
  const isDisabled = select.disabled || disabled;
  const empty = select.value === undefined || select.value === "";
  return (
    <button
      ref={composed}
      type="button"
      role="combobox"
      aria-controls={select.open ? select.contentId : undefined}
      aria-expanded={select.open}
      aria-autocomplete="none"
      data-state={select.open ? "open" : "closed"}
      data-disabled={isDisabled ? "" : undefined}
      data-placeholder={empty ? "" : undefined}
      data-slot="select-trigger"
      data-size={size}
      disabled={isDisabled}
      className={cx("vk-select", className)}
      onPointerDown={composeHandlers(onPointerDown, (event) => {
        if (isDisabled || event.button !== 0 || event.ctrlKey || event.pointerType === "touch") return;
        pressed.current = true;
        window.setTimeout(() => {
          pressed.current = false;
        }, 1000);
        // Focus the trigger by hand so the list can take focus and give it back on close.
        // 手动聚焦触发器，使列表能接管焦点并在关闭时归还。
        focusElement(event.currentTarget);
        event.preventDefault();
        select.openedByKeyboard.current = false;
        select.setOpen(!select.open);
      })}
      onClick={composeHandlers(onClick, () => {
        if (pressed.current) {
          pressed.current = false;
          return;
        }
        if (isDisabled) return;
        select.openedByKeyboard.current = false;
        select.setOpen(!select.open);
      })}
      onKeyDown={composeHandlers(onKeyDown, (event) => {
        if (isDisabled) return;
        if (event.key === "Enter" || event.key === " " || event.key === "ArrowDown" || event.key === "ArrowUp") {
          event.preventDefault();
          select.openedByKeyboard.current = true;
          select.setOpen(true);
        }
      })}
      {...props}
    >
      {children}
      <ChevronDownIcon className="vk-select-chevron" aria-hidden="true" />
    </button>
  );
}

// SelectContentPlacement prefers a side; "auto" flips to the side with more room.
// SelectContentPlacement 指定优先的一侧；"auto" 会翻转到空间更大的一侧。
type SelectContentPlacement = "auto" | "top" | "bottom";

// SelectContentProps choose the placement of the list.
// SelectContentProps 选择列表的位置。
export interface SelectContentProps extends React.ComponentProps<"div"> {
  placement?: SelectContentPlacement;
  position?: "popper" | "item-aligned";
  onCloseAutoFocus?: (event: Event) => void;
  onEscapeKeyDown?: (event: KeyboardEvent) => void;
  onPointerDownOutside?: (event: PointerEvent) => void;
}

// ContentOpenContext tells rows whether they render inside the visible list.
// ContentOpenContext 告知各行是否渲染在可见列表中。
const ContentOpenContext = React.createContext(false);

// SelectContent renders the listbox while open, and keeps the rows mounted off-document while closed.
// SelectContent 在打开期间渲染列表框，关闭期间让各行保持挂载在文档之外。
function SelectContent({ className, children, placement = "auto", position: _position, onCloseAutoFocus, onEscapeKeyDown, onPointerDownOutside, onKeyDown, ...props }: SelectContentProps) {
  const select = useSelect();
  // The fragment is created after mount: the server renderer cannot portal, so a server render shows no rows.
  // fragment 在挂载后创建：服务端渲染器无法使用门户，因此服务端渲染不显示任何行。
  const [fragment, setFragment] = React.useState<DocumentFragment | null>(null);
  React.useLayoutEffect(() => setFragment(document.createDocumentFragment()), []);
  if (!select.open) {
    return fragment ? createPortal(<ContentOpenContext.Provider value={false}><div>{children}</div></ContentOpenContext.Provider>, fragment) : null;
  }
  return (
    <FloatingContent
      open
      anchor={() => select.triggerRef.current}
      onDismiss={() => select.setOpen(false)}
      branches={[select.triggerRef]}
      blocking
      onEscapeKeyDown={onEscapeKeyDown}
      onPointerDownOutside={onPointerDownOutside}
      focus={{
        initialFocus: (scope) => {
          const rows = listRows(scope, OPTION_SELECTOR);
          return rows.find((row) => row.getAttribute("aria-selected") === "true") ?? rows[0] ?? null;
        },
        onCloseAutoFocus,
      }}
      side={placement === "top" ? "top" : "bottom"}
      align="start"
      sideOffset={4}
      collisionPadding={8}
      avoidCollisions={placement === "auto"}
      cssVarPrefix="select"
      id={select.contentId}
      role="listbox"
      data-slot="select-content"
      className={cx("vk-pop vk-select-list", className)}
      onKeyDown={composeHandlers(onKeyDown, (event) => {
        // Keys from the trigger's portaled value text are not the list's business.
        // 来自触发器中门户值文本的按键不由列表处理。
        if (!(event.target instanceof Node) || !event.currentTarget.contains(event.target)) return;
        if (event.key === "Tab") {
          event.preventDefault();
          return;
        }
        if (event.key === "Enter" || (event.key === " " && !moveInList(event, event.currentTarget, OPTION_SELECTOR))) {
          event.preventDefault();
          const row = (event.target as HTMLElement).closest<HTMLElement>(OPTION_SELECTOR);
          row?.click();
          return;
        }
        moveInList(event, event.currentTarget, OPTION_SELECTOR);
      })}
      {...props}
    >
      <ContentOpenContext.Provider value>{children}</ContentOpenContext.Provider>
    </FloatingContent>
  );
}

// SelectLabel titles a group of rows.
// SelectLabel 为一组行命名。
function SelectLabel({ className, ...props }: React.ComponentProps<"div">) {
  return <div data-slot="select-label" className={cx("vk-menu-title", className)} {...props} />;
}

// SelectItem is one choosable row; the selected row also renders its content into the trigger.
// SelectItem 是一个可选择的行；所选行还会把其内容渲染进触发器。
function SelectItem({ className, children, value, disabled, textValue, onClick, onPointerMove, onPointerLeave, onFocus, onBlur, ...props }: React.ComponentProps<"div"> & {
  value: string;
  disabled?: boolean;
  textValue?: string;
}) {
  const select = useSelect();
  const visible = React.useContext(ContentOpenContext);
  const selected = select.value === value;
  const id = useStableId("vk-select-option");
  // inside keeps events bubbling from the trigger's portaled copy away from the row.
  // inside 使从触发器中门户副本冒泡上来的事件不影响该行。
  const inside = (event: React.SyntheticEvent<HTMLElement>) => event.target instanceof Node && event.currentTarget.contains(event.target);
  return (
    <div
      id={id}
      role="option"
      tabIndex={-1}
      aria-selected={selected}
      aria-disabled={disabled || undefined}
      data-state={selected ? "checked" : "unchecked"}
      data-disabled={disabled ? "" : undefined}
      data-text-value={textValue}
      data-slot="select-item"
      className={cx("vk-menu-item vk-menu-check vk-select-item", className)}
      onClick={composeHandlers(onClick, (event) => {
        if (!inside(event) || disabled) return;
        select.setValue(value);
        select.setOpen(false);
      })}
      onPointerMove={composeHandlers(onPointerMove, (event) => {
        if (!inside(event) || disabled || event.pointerType === "touch") return;
        if (document.activeElement !== event.currentTarget) focusElement(event.currentTarget);
      })}
      onPointerLeave={composeHandlers(onPointerLeave, (event) => {
        if (document.activeElement === event.currentTarget) focusElement(event.currentTarget.closest<HTMLElement>('[role="listbox"]'));
      })}
      onFocus={composeHandlers(onFocus, (event) => {
        if (inside(event)) event.currentTarget.setAttribute("data-highlighted", "");
      })}
      onBlur={composeHandlers(onBlur, (event) => {
        if (inside(event)) event.currentTarget.removeAttribute("data-highlighted");
      })}
      {...props}
    >
      <span className="vk-menu-indicator" aria-hidden="true">{selected ? <CheckIcon /> : null}</span>
      <span data-slot="select-item-text" className="vk-select-item-text">{children}</span>
      {selected && select.valueNode && !select.valueHasChildren && (visible || !select.open) ? createPortal(children, select.valueNode) : null}
    </div>
  );
}

// SelectSeparator divides groups of rows.
// SelectSeparator 分隔各组行。
function SelectSeparator({ className, ...props }: React.ComponentProps<"div">) {
  return <div aria-hidden="true" data-slot="select-separator" className={cx("vk-menu-sep", className)} {...props} />;
}

// SelectScrollUpButton and SelectScrollDownButton are kept for API compatibility; the list scrolls natively.
// SelectScrollUpButton 与 SelectScrollDownButton 为兼容 API 而保留；列表使用原生滚动。
function SelectScrollUpButton(_props: React.ComponentProps<"div">) {
  return null;
}

function SelectScrollDownButton(_props: React.ComponentProps<"div">) {
  return null;
}

export {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectScrollDownButton,
  SelectScrollUpButton,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
};
