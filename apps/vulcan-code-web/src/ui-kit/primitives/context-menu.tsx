import * as React from "react";
import { composeHandlers, lastInputWasKeyboard, trackInputModality } from "../behavior/core";
import { Slot } from "../behavior/slot";
import {
  MenuCheckboxItem,
  MenuContent,
  MenuGroup,
  MenuItem,
  MenuLabel,
  MenuRadioGroup,
  MenuRadioItem,
  MenuRoot,
  MenuSeparator,
  MenuShortcut,
  MenuSub,
  MenuSubContent,
  MenuSubTrigger,
  useRootMenu,
  type MenuContentProps,
} from "./menu";
import type { FloatingContentWidth } from "./popover";

// Right-click menus open at the pointer over a region; the engine lives in ./menu. A touch press held for
// LONG_PRESS_MS opens the menu as well, and the keyboard context-menu key opens it at the region.
// 右键菜单在区域内的指针处打开；引擎位于 ./menu。触摸按住 LONG_PRESS_MS 也会打开菜单，键盘菜单键会在区域处打开它。

// LONG_PRESS_MS is how long a touch press must rest before the menu opens.
// LONG_PRESS_MS 是触摸按住多久后打开菜单。
const LONG_PRESS_MS = 700;

// ContextMenu holds the open state of one right-click menu.
// ContextMenu 保存单个右键菜单的打开状态。
function ContextMenu(props: Omit<React.ComponentProps<typeof MenuRoot>, "slot" | "open" | "defaultOpen">) {
  return <MenuRoot slot="context-menu" {...props} />;
}

// ContextMenuTrigger is the region that opens the menu; it is not a branch of the menu, so a left press
// inside the region still dismisses an open menu.
// ContextMenuTrigger 是打开菜单的区域；它不是菜单的分支，因此在区域内左键按下仍会关闭已打开的菜单。
function ContextMenuTrigger({ asChild, disabled, onContextMenu, onPointerDown, onPointerMove, onPointerUp, onPointerCancel, ...props }: React.ComponentProps<"span"> & { asChild?: boolean; disabled?: boolean }) {
  const menu = useRootMenu();
  const timer = React.useRef(0);
  React.useEffect(() => {
    trackInputModality();
    return () => window.clearTimeout(timer.current);
  }, []);
  const openAt = (point: { x: number; y: number } | HTMLElement, byKeyboard: boolean) => {
    menu.anchor.current = point;
    menu.openedByKeyboard.current = byKeyboard ? "first" : null;
    menu.setOpen(true);
  };
  const clearLongPress = () => window.clearTimeout(timer.current);
  const Comp = asChild ? Slot : "span";
  type Handler<E> = ((event: E) => void) | undefined;
  return (
    <Comp
      data-state={menu.open ? "open" : "closed"}
      data-disabled={disabled ? "" : undefined}
      data-slot="context-menu-trigger"
      style={{ WebkitTouchCallout: "none" }}
      onContextMenu={composeHandlers(onContextMenu as Handler<React.MouseEvent<HTMLElement>>, (event) => {
        clearLongPress();
        if (disabled) return;
        event.preventDefault();
        // The keyboard context-menu key may report no position (anchor to the region) or the caret position;
        // either way a keyboard opening focuses the first row.
        // 键盘菜单键可能不提供位置（锚定到区域）或提供光标位置；两种情况下键盘打开都会聚焦首行。
        const unpositioned = event.button === 0 && event.clientX === 0 && event.clientY === 0;
        openAt(unpositioned ? event.currentTarget : { x: event.clientX, y: event.clientY }, unpositioned || lastInputWasKeyboard());
      })}
      onPointerDown={composeHandlers(onPointerDown as Handler<React.PointerEvent<HTMLElement>>, (event) => {
        if (disabled || event.pointerType !== "touch") return;
        const point = { x: event.clientX, y: event.clientY };
        clearLongPress();
        timer.current = window.setTimeout(() => openAt(point, false), LONG_PRESS_MS);
      })}
      onPointerMove={composeHandlers(onPointerMove as Handler<React.PointerEvent<HTMLElement>>, (event) => {
        if (event.pointerType === "touch") clearLongPress();
      })}
      onPointerUp={composeHandlers(onPointerUp as Handler<React.PointerEvent<HTMLElement>>, clearLongPress)}
      onPointerCancel={composeHandlers(onPointerCancel as Handler<React.PointerEvent<HTMLElement>>, clearLongPress)}
      {...(props as React.HTMLAttributes<HTMLElement>)}
    />
  );
}

// ContextMenuContent opens to the right of the pointer and sizes to its rows by default.
// ContextMenuContent 默认在指针右侧打开，并按行内容确定宽度。
function ContextMenuContent({ width = "content", side = "right", align = "start", sideOffset = 2, ...props }: Omit<MenuContentProps, "width"> & {
  width?: Exclude<FloatingContentWidth, "trigger">;
}) {
  return <MenuContent width={width} side={side} align={align} sideOffset={sideOffset} {...props} />;
}

// ContextMenuPortal is kept for API compatibility; menu surfaces always render in their own layer.
// ContextMenuPortal 为兼容 API 而保留；菜单表面始终渲染在独立层中。
function ContextMenuPortal({ children }: { children?: React.ReactNode; container?: HTMLElement | null; forceMount?: boolean }) {
  return <>{children}</>;
}

const ContextMenuItem = MenuItem;
const ContextMenuCheckboxItem = MenuCheckboxItem;
const ContextMenuRadioItem = MenuRadioItem;
const ContextMenuLabel = MenuLabel;
const ContextMenuSeparator = MenuSeparator;
const ContextMenuShortcut = MenuShortcut;
const ContextMenuGroup = MenuGroup;
const ContextMenuSub = MenuSub;
const ContextMenuSubContent = MenuSubContent;
const ContextMenuSubTrigger = MenuSubTrigger;
const ContextMenuRadioGroup = MenuRadioGroup;

export {
  ContextMenu,
  ContextMenuTrigger,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuCheckboxItem,
  ContextMenuRadioItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuShortcut,
  ContextMenuGroup,
  ContextMenuPortal,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  ContextMenuRadioGroup,
};
