import * as React from "react";
import { CheckIcon, ChevronRightIcon, MinusIcon } from "lucide-react";
import { composeHandlers, cx, focusElement, useComposedRefs, useControllableState, useStableId } from "../behavior/core";
import { FloatingContent } from "../behavior/floating-content";
import type { AnchorLike, FloatingAlign, FloatingSide } from "../behavior/floating";
import { moveInList } from "../behavior/roving";
import { Slot } from "../behavior/slot";
import type { FloatingContentWidth } from "./popover";

// Menus: one engine behind the dropdown and context menus. A menu is a list of commands; arrows, Home, End
// and typed letters move between rows, Enter or Space runs one, submenus open to the side, and choosing a
// command closes the whole chain. An outside press only closes the menu; it does not also activate what lies
// beneath it.
// 菜单：下拉菜单与右键菜单共用的引擎。菜单是一组命令；方向键、Home、End 与输入字母在行间移动，Enter 或 Space 执行，
// 子菜单向侧边展开，选择命令后关闭整条菜单链。外部按下只关闭菜单，不会同时触发其下方的元素。

// ITEM_SELECTOR matches the focusable rows of one menu surface.
// ITEM_SELECTOR 匹配单个菜单表面中可聚焦的行。
const ITEM_SELECTOR = '[role="menuitem"], [role="menuitemcheckbox"], [role="menuitemradio"]';

// RootMenuContextValue is shared by every surface in one menu chain.
// RootMenuContextValue 由同一菜单链中的所有表面共享。
interface RootMenuContextValue {
  slot: "dropdown-menu" | "context-menu";
  open: boolean;
  setOpen: (open: boolean) => void;
  closeAll: () => void;
  contentId: string;
  triggerRef: React.RefObject<HTMLElement | null>;
  rootContentRef: React.RefObject<HTMLElement | null>;
  anchor: React.RefObject<AnchorLike>;
  openedByKeyboard: React.RefObject<"first" | "last" | null>;
}

const RootMenuContext = React.createContext<RootMenuContextValue | null>(null);

// useRootMenu reads the menu chain.
// useRootMenu 读取菜单链。
export function useRootMenu(): RootMenuContextValue {
  const context = React.useContext(RootMenuContext);
  if (!context) throw new Error("Menu parts must be rendered inside a menu.");
  return context;
}

// SurfaceContextValue lets items close sibling submenus of the surface they belong to.
// SurfaceContextValue 让菜单项关闭其所属表面中的兄弟子菜单。
interface SurfaceContextValue {
  activeSub: string | null;
  setActiveSub: (id: string | null) => void;
}

const SurfaceContext = React.createContext<SurfaceContextValue>({ activeSub: null, setActiveSub: () => {} });

// MenuRoot provides the open state of a menu chain.
// MenuRoot 提供菜单链的打开状态。
export function MenuRoot({ slot, open: controlled, defaultOpen = false, onOpenChange, children }: {
  slot: RootMenuContextValue["slot"];
  open?: boolean;
  defaultOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
  modal?: boolean;
  children?: React.ReactNode;
}) {
  const [open, setOpen] = useControllableState(controlled, defaultOpen, onOpenChange);
  const contentId = useStableId(`vk-${slot}`);
  const triggerRef = React.useRef<HTMLElement | null>(null);
  const rootContentRef = React.useRef<HTMLElement | null>(null);
  const anchor = React.useRef<AnchorLike>(null);
  const openedByKeyboard = React.useRef<"first" | "last" | null>(null);
  const context = React.useMemo<RootMenuContextValue>(() => ({
    slot,
    open,
    setOpen,
    closeAll: () => setOpen(false),
    contentId,
    triggerRef,
    rootContentRef,
    anchor,
    openedByKeyboard,
  }), [slot, open, setOpen, contentId]);
  return <RootMenuContext.Provider value={context}>{children}</RootMenuContext.Provider>;
}

// swallowNextClick cancels the click that completes an outside press, so dismissing a menu does not also
// press the button under the pointer.
// swallowNextClick 取消完成外部按下的那次点击，使关闭菜单时不会同时按下指针下方的按钮。
function swallowNextClick(target: EventTarget | null): void {
  const swallow = (event: MouseEvent) => {
    if (event.target === target || (target instanceof Node && event.target instanceof Node && target.contains(event.target))) {
      event.preventDefault();
      event.stopPropagation();
    }
  };
  document.addEventListener("click", swallow, { capture: true, once: true });
  window.setTimeout(() => document.removeEventListener("click", swallow, { capture: true }), 600);
}

// MenuTrigger opens a dropdown on press, Enter, Space or the arrow keys.
// MenuTrigger 在按下、Enter、Space 或方向键时打开下拉菜单。
export function MenuTrigger({ asChild, ref, onPointerDown, onClick, onKeyDown, disabled, ...props }: React.ComponentProps<"button"> & { asChild?: boolean }) {
  const menu = useRootMenu();
  const composed = useComposedRefs(menu.triggerRef as React.Ref<HTMLButtonElement>, ref);
  const pressed = React.useRef(false);
  const Comp = asChild ? Slot : "button";
  type Handler<E> = ((event: E) => void) | undefined;
  return (
    <Comp
      ref={composed as never}
      type={asChild ? undefined : "button"}
      id={`${menu.contentId}-trigger`}
      aria-haspopup="menu"
      aria-expanded={menu.open}
      aria-controls={menu.open ? menu.contentId : undefined}
      data-state={menu.open ? "open" : "closed"}
      data-disabled={disabled ? "" : undefined}
      data-slot={`${menu.slot}-trigger`}
      disabled={disabled}
      onPointerDown={composeHandlers(onPointerDown as Handler<React.PointerEvent<HTMLElement>>, (event) => {
        if (disabled || event.button !== 0 || event.ctrlKey) return;
        pressed.current = true;
        window.setTimeout(() => {
          pressed.current = false;
        }, 1000);
        menu.openedByKeyboard.current = null;
        menu.setOpen(!menu.open);
        // Focus the trigger by hand and cancel the press default, so the surface can take focus once mounted
        // and give it back to the trigger when it closes.
        // 手动聚焦触发器并取消按下默认行为，使表面挂载后能接管焦点，并在关闭时把焦点还给触发器。
        if (!menu.open) {
          focusElement(event.currentTarget);
          event.preventDefault();
        }
      })}
      onClick={composeHandlers(onClick as Handler<React.MouseEvent<HTMLElement>>, () => {
        if (pressed.current) {
          pressed.current = false;
          return;
        }
        if (disabled) return;
        menu.openedByKeyboard.current = null;
        menu.setOpen(!menu.open);
      })}
      onKeyDown={composeHandlers(onKeyDown as Handler<React.KeyboardEvent<HTMLElement>>, (event) => {
        if (disabled) return;
        if (event.key === "Enter" || event.key === " " || event.key === "ArrowDown" || event.key === "ArrowUp") {
          event.preventDefault();
          menu.openedByKeyboard.current = event.key === "ArrowUp" ? "last" : "first";
          if (event.key === "Enter" || event.key === " ") menu.setOpen(!menu.open);
          else menu.setOpen(true);
        }
      })}
      {...(props as React.HTMLAttributes<HTMLElement>)}
    />
  );
}

// MenuSurfaceProps are shared by root and sub surfaces.
// MenuSurfaceProps 由根表面与子表面共享。
export interface MenuContentProps extends React.ComponentProps<"div"> {
  side?: FloatingSide;
  align?: FloatingAlign;
  sideOffset?: number;
  alignOffset?: number;
  collisionPadding?: number;
  width?: FloatingContentWidth;
  loop?: boolean;
  forceMount?: boolean;
  onCloseAutoFocus?: (event: Event) => void;
  onOpenAutoFocus?: (event: Event) => void;
  onEscapeKeyDown?: (event: KeyboardEvent) => void;
  onPointerDownOutside?: (event: PointerEvent) => void;
  onFocusOutside?: (event: FocusEvent) => void;
  onInteractOutside?: (event: PointerEvent | FocusEvent) => void;
}

// useMenuKeyboard moves focus through a surface's rows and handles Tab.
// useMenuKeyboard 在表面的行之间移动焦点并处理 Tab。
function useMenuKeyboard(element: React.RefObject<HTMLElement | null>, loop: boolean) {
  return React.useCallback((event: React.KeyboardEvent<HTMLElement>) => {
    const surface = element.current;
    if (!surface) return;
    // Keys pressed inside a nested surface are that surface's business.
    // 嵌套表面内按下的键由该表面自行处理。
    if (event.target instanceof Node && (event.target as HTMLElement).closest?.('[role="menu"]') !== surface) return;
    if (event.key === "Tab") {
      event.preventDefault();
      return;
    }
    moveInList(event, surface, ITEM_SELECTOR, { loop });
  }, [element, loop]);
}

// focusInitialRow focuses the first or last row after a keyboard open, or the surface otherwise.
// focusInitialRow 在键盘打开后聚焦第一行或最后一行，否则聚焦表面本身。
function focusInitialRow(surface: HTMLElement, which: "first" | "last" | null): HTMLElement {
  if (!which) return surface;
  const rows = Array.from(surface.querySelectorAll<HTMLElement>(ITEM_SELECTOR)).filter((row) => !row.hasAttribute("data-disabled"));
  return (which === "first" ? rows[0] : rows[rows.length - 1]) ?? surface;
}

// MenuContent renders the root surface of a menu chain.
// MenuContent 渲染菜单链的根表面。
export function MenuContent({
  className,
  side = "bottom",
  align = "start",
  sideOffset = 4,
  alignOffset,
  collisionPadding = 8,
  width = "trigger",
  loop = false,
  forceMount: _forceMount,
  onCloseAutoFocus,
  onOpenAutoFocus,
  onPointerDownOutside,
  onInteractOutside,
  onKeyDown,
  ref,
  ...props
}: MenuContentProps) {
  const menu = useRootMenu();
  const element = React.useRef<HTMLDivElement | null>(null);
  const composed = useComposedRefs(element, ref, menu.rootContentRef as React.Ref<HTMLDivElement>);
  const [activeSub, setActiveSub] = React.useState<string | null>(null);
  const surface = React.useMemo(() => ({ activeSub, setActiveSub }), [activeSub]);
  const interactedOutside = React.useRef(false);
  const keyboard = useMenuKeyboard(element, loop);
  React.useEffect(() => {
    if (!menu.open) setActiveSub(null);
  }, [menu.open]);
  return (
    <SurfaceContext.Provider value={surface}>
      <FloatingContent
        ref={composed}
        open={menu.open}
        anchor={() => menu.anchor.current ?? menu.triggerRef.current}
        onDismiss={menu.closeAll}
        branches={[menu.triggerRef]}
        dismissOnFocusOutside
        blocking
        onPointerDownOutside={(event) => {
          onPointerDownOutside?.(event);
          if (!event.defaultPrevented) swallowNextClick(event.target);
        }}
        onInteractOutside={(event) => {
          onInteractOutside?.(event);
          interactedOutside.current = !event.defaultPrevented;
        }}
        focus={{
          onOpenAutoFocus,
          initialFocus: (scope) => focusInitialRow(scope, menu.openedByKeyboard.current),
          returnFocus: () => menu.triggerRef.current,
          // After an outside press the user has moved on; do not pull focus back to the trigger.
          // 外部按下后用户已转向别处；不要把焦点拉回触发器。
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
        id={menu.contentId}
        role="menu"
        aria-orientation="vertical"
        aria-labelledby={menu.slot === "dropdown-menu" ? `${menu.contentId}-trigger` : undefined}
        data-slot={`${menu.slot}-content`}
        data-width={width}
        className={cx("vk-pop vk-menu", className)}
        onKeyDown={composeHandlers(onKeyDown, keyboard)}
        {...props}
      />
    </SurfaceContext.Provider>
  );
}

// MenuItemProps follow the Radix menu item; onSelect may preventDefault to keep the menu open.
// MenuItemProps 沿用 Radix 菜单项；onSelect 可调用 preventDefault 以保持菜单打开。
export interface MenuItemProps extends Omit<React.ComponentProps<"div">, "onSelect"> {
  onSelect?: (event: Event) => void;
  disabled?: boolean;
  textValue?: string;
  inset?: boolean;
  variant?: "default" | "destructive";
  asChild?: boolean;
}

// useItemBehavior wires pointer highlight, keyboard activation and selection for one row.
// useItemBehavior 为一行接入指针高亮、键盘激活与选择行为。
function useItemBehavior(props: {
  disabled?: boolean;
  onSelect?: (event: Event) => void;
  onActivate?: () => void;
  keepOpen?: boolean;
}) {
  const menu = useRootMenu();
  const surface = React.useContext(SurfaceContext);
  return {
    onPointerMove: (event: React.PointerEvent<HTMLElement>) => {
      if (event.pointerType === "touch" || props.disabled) return;
      if (surface.activeSub) surface.setActiveSub(null);
      if (document.activeElement !== event.currentTarget) focusElement(event.currentTarget);
    },
    onPointerLeave: (event: React.PointerEvent<HTMLElement>) => {
      if (document.activeElement === event.currentTarget) focusElement(event.currentTarget.closest<HTMLElement>('[role="menu"]'));
    },
    onFocus: (event: React.FocusEvent<HTMLElement>) => event.currentTarget.setAttribute("data-highlighted", ""),
    onBlur: (event: React.FocusEvent<HTMLElement>) => event.currentTarget.removeAttribute("data-highlighted"),
    onKeyDown: (event: React.KeyboardEvent<HTMLElement>) => {
      if (event.key !== "Enter" && event.key !== " ") return;
      if (event.key === " " && (event.currentTarget.closest('[role="menu"]') as HTMLElement | null)?.dataset.typing) return;
      event.preventDefault();
      if (!props.disabled) event.currentTarget.click();
    },
    onClick: () => {
      if (props.disabled) return;
      props.onActivate?.();
      const selectEvent = new CustomEvent("menu.itemSelect", { bubbles: true, cancelable: true });
      props.onSelect?.(selectEvent);
      if (!selectEvent.defaultPrevented && !props.keepOpen) menu.closeAll();
    },
  };
}

// mergeItemHandlers runs the caller handler first and the menu behavior unless the caller prevented default.
// mergeItemHandlers 先运行调用方处理器，调用方未阻止默认行为时再运行菜单行为。
function mergeItemHandlers(
  own: ReturnType<typeof useItemBehavior>,
  given: Partial<Record<keyof ReturnType<typeof useItemBehavior>, ((event: never) => void) | undefined>>,
) {
  const merged: Record<string, (event: never) => void> = {};
  for (const key of Object.keys(own) as Array<keyof typeof own>) {
    merged[key] = composeHandlers(given[key] as ((event: { defaultPrevented: boolean }) => void) | undefined, own[key] as (event: { defaultPrevented: boolean }) => void);
  }
  return merged;
}

// MenuItem renders one command row.
// MenuItem 渲染一行命令。
export function MenuItem({
  className,
  inset,
  variant = "default",
  disabled,
  textValue,
  onSelect,
  asChild,
  onPointerMove,
  onPointerLeave,
  onFocus,
  onBlur,
  onKeyDown,
  onClick,
  ...props
}: MenuItemProps) {
  const menu = useRootMenu();
  const behavior = useItemBehavior({ disabled, onSelect });
  const Comp = asChild ? Slot : "div";
  return (
    <Comp
      role="menuitem"
      tabIndex={-1}
      aria-disabled={disabled || undefined}
      data-disabled={disabled ? "" : undefined}
      data-text-value={textValue}
      data-inset={inset ? "" : undefined}
      data-variant={variant}
      data-tone={variant === "destructive" ? "bad" : undefined}
      data-slot={`${menu.slot}-item`}
      className={cx("vk-menu-item", className)}
      {...mergeItemHandlers(behavior, { onPointerMove, onPointerLeave, onFocus, onBlur, onKeyDown, onClick } as never)}
      {...(props as React.HTMLAttributes<HTMLElement>)}
    />
  );
}

// CheckedState is a checkbox row value; "indeterminate" shows a dash.
// CheckedState 是复选行的值；"indeterminate" 显示短横线。
type CheckedState = boolean | "indeterminate";

// MenuCheckboxItem renders a toggle row.
// MenuCheckboxItem 渲染一个开关行。
export function MenuCheckboxItem({
  className,
  children,
  checked = false,
  onCheckedChange,
  inset,
  disabled,
  textValue,
  onSelect,
  onPointerMove,
  onPointerLeave,
  onFocus,
  onBlur,
  onKeyDown,
  onClick,
  ...props
}: Omit<MenuItemProps, "variant" | "asChild"> & { checked?: CheckedState; onCheckedChange?: (checked: boolean) => void }) {
  const menu = useRootMenu();
  const behavior = useItemBehavior({
    disabled,
    onSelect,
    onActivate: () => onCheckedChange?.(checked === "indeterminate" ? true : !checked),
  });
  const state = checked === "indeterminate" ? "indeterminate" : checked ? "checked" : "unchecked";
  return (
    <div
      role="menuitemcheckbox"
      tabIndex={-1}
      aria-checked={checked === "indeterminate" ? "mixed" : checked}
      aria-disabled={disabled || undefined}
      data-state={state}
      data-disabled={disabled ? "" : undefined}
      data-text-value={textValue}
      data-inset={inset ? "" : undefined}
      data-slot={`${menu.slot}-checkbox-item`}
      className={cx("vk-menu-item vk-menu-check", className)}
      {...mergeItemHandlers(behavior, { onPointerMove, onPointerLeave, onFocus, onBlur, onKeyDown, onClick } as never)}
      {...props}
    >
      <span className="vk-menu-indicator" data-slot={`${menu.slot}-checkbox-item-indicator`} aria-hidden="true">
        {state === "checked" ? <CheckIcon /> : state === "indeterminate" ? <MinusIcon /> : null}
      </span>
      {children}
    </div>
  );
}

// RadioContext shares a radio group's value with its rows.
// RadioContext 与各行共享单选组的值。
const RadioContext = React.createContext<{ value?: string; onValueChange?: (value: string) => void }>({});

// MenuRadioGroup groups mutually exclusive rows.
// MenuRadioGroup 组合互斥的行。
export function MenuRadioGroup({ value, onValueChange, ...props }: Omit<React.ComponentProps<"div">, "onChange"> & { value?: string; onValueChange?: (value: string) => void }) {
  const menu = useRootMenu();
  const context = React.useMemo(() => ({ value, onValueChange }), [value, onValueChange]);
  return (
    <RadioContext.Provider value={context}>
      <div role="group" data-slot={`${menu.slot}-radio-group`} {...props} />
    </RadioContext.Provider>
  );
}

// MenuRadioItem renders one exclusive choice row.
// MenuRadioItem 渲染一个互斥选择行。
export function MenuRadioItem({
  className,
  children,
  value,
  inset,
  disabled,
  textValue,
  onSelect,
  onPointerMove,
  onPointerLeave,
  onFocus,
  onBlur,
  onKeyDown,
  onClick,
  ...props
}: Omit<MenuItemProps, "variant" | "asChild"> & { value: string }) {
  const menu = useRootMenu();
  const group = React.useContext(RadioContext);
  const checked = group.value === value;
  const behavior = useItemBehavior({ disabled, onSelect, onActivate: () => group.onValueChange?.(value) });
  return (
    <div
      role="menuitemradio"
      tabIndex={-1}
      aria-checked={checked}
      aria-disabled={disabled || undefined}
      data-state={checked ? "checked" : "unchecked"}
      data-disabled={disabled ? "" : undefined}
      data-text-value={textValue}
      data-inset={inset ? "" : undefined}
      data-slot={`${menu.slot}-radio-item`}
      className={cx("vk-menu-item vk-menu-check", className)}
      {...mergeItemHandlers(behavior, { onPointerMove, onPointerLeave, onFocus, onBlur, onKeyDown, onClick } as never)}
      {...props}
    >
      <span className="vk-menu-indicator" data-slot={`${menu.slot}-radio-item-indicator`} aria-hidden="true">
        {checked ? <CheckIcon /> : null}
      </span>
      {children}
    </div>
  );
}

// MenuLabel titles a group of rows; it is not focusable.
// MenuLabel 为一组行命名；不可聚焦。
export function MenuLabel({ className, inset, ...props }: React.ComponentProps<"div"> & { inset?: boolean }) {
  const menu = useRootMenu();
  return <div data-slot={`${menu.slot}-label`} data-inset={inset ? "" : undefined} className={cx("vk-menu-title", className)} {...props} />;
}

// MenuSeparator divides groups of rows.
// MenuSeparator 分隔各组行。
export function MenuSeparator({ className, ...props }: React.ComponentProps<"div">) {
  const menu = useRootMenu();
  return <div role="separator" aria-orientation="horizontal" data-slot={`${menu.slot}-separator`} className={cx("vk-menu-sep", className)} {...props} />;
}

// MenuShortcut shows a row's keyboard shortcut at its trailing edge.
// MenuShortcut 在行尾显示该行的键盘快捷键。
export function MenuShortcut({ className, ...props }: React.ComponentProps<"span">) {
  const menu = useRootMenu();
  return <span data-slot={`${menu.slot}-shortcut`} className={cx("vk-menu-meta", className)} {...props} />;
}

// MenuGroup groups related rows.
// MenuGroup 组合相关的行。
export function MenuGroup(props: React.ComponentProps<"div">) {
  const menu = useRootMenu();
  return <div role="group" data-slot={`${menu.slot}-group`} {...props} />;
}

// SubMenuContextValue is the state of one submenu.
// SubMenuContextValue 是单个子菜单的状态。
interface SubMenuContextValue {
  id: string;
  open: boolean;
  setOpen: (open: boolean) => void;
  triggerRef: React.RefObject<HTMLElement | null>;
  focusFirst: React.RefObject<boolean>;
}

const SubMenuContext = React.createContext<SubMenuContextValue | null>(null);

// useSubMenu reads the enclosing submenu.
// useSubMenu 读取外层子菜单。
function useSubMenu(): SubMenuContextValue {
  const context = React.useContext(SubMenuContext);
  if (!context) throw new Error("Submenu parts must be rendered inside a submenu.");
  return context;
}

// MenuSub holds a submenu's open state; the parent surface keeps at most one submenu open.
// MenuSub 保存子菜单的打开状态；父表面最多保持一个子菜单打开。
export function MenuSub({ open: controlled, defaultOpen = false, onOpenChange, children }: {
  open?: boolean;
  defaultOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
  children?: React.ReactNode;
}) {
  const surface = React.useContext(SurfaceContext);
  const id = useStableId("vk-submenu");
  const [ownOpen, setOwnOpen] = useControllableState(controlled, defaultOpen, onOpenChange);
  const triggerRef = React.useRef<HTMLElement | null>(null);
  const focusFirst = React.useRef(false);
  // A submenu is open when it says so and no sibling took over the surface.
  // 子菜单在自身声明打开且没有兄弟子菜单接管表面时才处于打开状态。
  const open = ownOpen && (surface.activeSub === null || surface.activeSub === id);
  const context = React.useMemo<SubMenuContextValue>(() => ({
    id,
    open,
    triggerRef,
    focusFirst,
    setOpen: (next) => {
      if (next) surface.setActiveSub(id);
      else if (surface.activeSub === id) surface.setActiveSub(null);
      setOwnOpen(next);
    },
  }), [id, open, surface, setOwnOpen]);
  return <SubMenuContext.Provider value={context}>{children}</SubMenuContext.Provider>;
}

// MenuSubTrigger is the row that opens a submenu on hover, ArrowRight, Enter or Space.
// MenuSubTrigger 是在悬停、ArrowRight、Enter 或 Space 时打开子菜单的行。
export function MenuSubTrigger({ className, inset, disabled, children, ref, onPointerMove, onKeyDown, onClick, onFocus, onBlur, ...props }: React.ComponentProps<"div"> & { inset?: boolean; disabled?: boolean }) {
  const menu = useRootMenu();
  const sub = useSubMenu();
  const composed = useComposedRefs(sub.triggerRef as React.Ref<HTMLDivElement>, ref);
  return (
    <div
      ref={composed}
      role="menuitem"
      tabIndex={-1}
      aria-haspopup="menu"
      aria-expanded={sub.open}
      aria-disabled={disabled || undefined}
      data-state={sub.open ? "open" : "closed"}
      data-disabled={disabled ? "" : undefined}
      data-inset={inset ? "" : undefined}
      data-slot={`${menu.slot}-sub-trigger`}
      className={cx("vk-menu-item vk-menu-sub-trigger", className)}
      onPointerMove={composeHandlers(onPointerMove, (event) => {
        if (event.pointerType === "touch" || disabled) return;
        if (document.activeElement !== event.currentTarget) focusElement(event.currentTarget);
        if (!sub.open) sub.setOpen(true);
      })}
      onFocus={composeHandlers(onFocus, (event) => event.currentTarget.setAttribute("data-highlighted", ""))}
      onBlur={composeHandlers(onBlur, (event) => event.currentTarget.removeAttribute("data-highlighted"))}
      onClick={composeHandlers(onClick, () => {
        if (!disabled) sub.setOpen(true);
      })}
      onKeyDown={composeHandlers(onKeyDown, (event) => {
        if (disabled) return;
        if (event.key === "ArrowRight" || event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          event.stopPropagation();
          sub.focusFirst.current = true;
          sub.setOpen(true);
        }
      })}
      {...props}
    >
      {children}
      <ChevronRightIcon className="vk-menu-sub-chevron" aria-hidden="true" />
    </div>
  );
}

// MenuSubContent renders a submenu beside its trigger.
// MenuSubContent 在其触发行旁渲染子菜单。
export function MenuSubContent({
  className,
  sideOffset = 2,
  alignOffset = -5,
  collisionPadding = 8,
  loop = false,
  forceMount: _forceMount,
  onKeyDown,
  ref,
  ...props
}: Omit<MenuContentProps, "side" | "align" | "width">) {
  const menu = useRootMenu();
  const sub = useSubMenu();
  const element = React.useRef<HTMLDivElement | null>(null);
  const composed = useComposedRefs(element, ref);
  const [activeSub, setActiveSub] = React.useState<string | null>(null);
  const surface = React.useMemo(() => ({ activeSub, setActiveSub }), [activeSub]);
  const keyboard = useMenuKeyboard(element, loop);
  const open = menu.open && sub.open;
  return (
    <SurfaceContext.Provider value={surface}>
      <FloatingContent
        ref={composed}
        open={open}
        anchor={() => sub.triggerRef.current}
        onDismiss={() => sub.setOpen(false)}
        onEscapeKeyDown={(event) => {
          // Escape closes the whole menu chain, not only this submenu.
          // Esc 关闭整条菜单链，而不仅是当前子菜单。
          event.preventDefault();
          menu.closeAll();
        }}
        onPointerDownOutside={(event) => {
          const target = event.target as Node;
          if (menu.rootContentRef.current?.contains(target)) return;
          // A press outside the whole chain closes everything.
          // 在整条菜单链之外按下会关闭全部菜单。
          event.preventDefault();
          swallowNextClick(event.target);
          menu.closeAll();
        }}
        branches={[sub.triggerRef]}
        blocking
        focus={{
          initialFocus: (scope) => {
            const first = sub.focusFirst.current;
            sub.focusFirst.current = false;
            return first ? focusInitialRow(scope, "first") : null;
          },
          // Focus stays on the trigger row when the submenu opened by hover.
          // 子菜单由悬停打开时焦点保持在触发行上。
          onOpenAutoFocus: (event) => {
            if (!sub.focusFirst.current) event.preventDefault();
          },
          onCloseAutoFocus: (event) => {
            event.preventDefault();
            if (element.current?.contains(document.activeElement) || document.activeElement === document.body) focusElement(sub.triggerRef.current);
          },
        }}
        side="right"
        align="start"
        sideOffset={sideOffset}
        alignOffset={alignOffset}
        collisionPadding={collisionPadding}
        role="menu"
        aria-orientation="vertical"
        data-slot={`${menu.slot}-sub-content`}
        className={cx("vk-pop vk-menu", className)}
        onKeyDown={composeHandlers(onKeyDown, (event) => {
          if (event.key === "ArrowLeft") {
            event.preventDefault();
            event.stopPropagation();
            sub.setOpen(false);
            focusElement(sub.triggerRef.current);
            return;
          }
          keyboard(event);
        })}
        {...props}
      />
    </SurfaceContext.Provider>
  );
}
