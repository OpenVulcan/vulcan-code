import React from "react";
import { useComposedRefs } from "./core";
import { useFloating, type AnchorLike, type FloatingOptions } from "./floating";
import { useFocusScope, type FocusScopeOptions } from "./focus-scope";
import { LayerDepth, LayerPortal, useDismissable, type DismissOptions } from "./layer";

// FloatingContent is the one anchored overlay surface behind menus, popovers, listboxes and hover cards:
// a portal, viewport-aware placement, the shared dismiss stack and optional focus management.
// FloatingContent 是菜单、弹出层、列表框与悬停卡片共用的锚定覆盖层：门户、感知视口的定位、共享关闭栈与可选焦点管理。

// FloatingContentProps adds placement, dismissal and focus options to ordinary div attributes.
// FloatingContentProps 在普通 div 属性之上增加定位、关闭与焦点选项。
export interface FloatingContentProps extends Omit<React.HTMLAttributes<HTMLDivElement>, "onFocus">, FloatingOptions, DismissOptions {
  open: boolean;
  // anchor returns the element or point the surface attaches to.
  // anchor 返回浮层附着的元素或坐标点。
  anchor: () => AnchorLike;
  // onDismiss closes the surface after Escape or an outside interaction that was not prevented.
  // onDismiss 在 Esc 或未被阻止的外部交互后关闭浮层。
  onDismiss: () => void;
  // branches count as inside for outside-press detection (usually the trigger).
  // branches 在外部按下检测中视为内部（通常是触发器）。
  branches?: Array<React.RefObject<HTMLElement | null>>;
  dismissOnFocusOutside?: boolean;
  // dismissable false keeps the surface out of the dismiss stack (tooltips never block other layers).
  // dismissable 为 false 时浮层不加入关闭栈（提示不阻挡其他层）。
  dismissable?: boolean;
  // blocking shields the layers below from outside presses (menus and listboxes; popovers stay non-blocking).
  // blocking 使下方的层不受外部按下影响（菜单与列表框；弹出层保持非阻断）。
  blocking?: boolean;
  // focus enables the focus scope; false leaves focus with the trigger (tooltips, hover cards).
  // focus 启用焦点范围；false 让焦点留在触发器上（提示与悬停卡片）。
  focus?: FocusScopeOptions | false;
  // layer names the portal host so tooltips and toasts can stack above dialogs.
  // layer 命名门户宿主，使提示与通知能叠在对话框之上。
  layer?: string;
  onFocus?: React.FocusEventHandler<HTMLDivElement>;
  ref?: React.Ref<HTMLDivElement>;
}

// FloatingContent renders the open surface; it renders nothing while closed.
// FloatingContent 渲染打开状态的浮层；关闭时不渲染任何内容。
export function FloatingContent({
  open,
  anchor,
  onDismiss,
  branches,
  dismissOnFocusOutside,
  dismissable = true,
  blocking = false,
  focus,
  layer,
  side,
  align,
  sideOffset,
  alignOffset,
  collisionPadding,
  avoidCollisions,
  onEscapeKeyDown,
  onPointerDownOutside,
  onFocusOutside,
  onInteractOutside,
  ref,
  style,
  children,
  ...props
}: FloatingContentProps) {
  const element = React.useRef<HTMLDivElement | null>(null);
  const composed = useComposedRefs(element, ref);
  const placement = useFloating(open, anchor, element, { side, align, sideOffset, alignOffset, collisionPadding, avoidCollisions });
  const dismissLayer = useDismissable(open && dismissable, element, onDismiss, {
    blocking,
    branches,
    dismissOnFocusOutside,
    onEscapeKeyDown,
    onPointerDownOutside,
    onFocusOutside,
    onInteractOutside,
  });
  useFocusScope(open && focus !== false, element, focus || {});
  if (!open) return null;
  return (
    <LayerPortal className={layer ? `vk-layer-${layer}` : undefined}>
      <div
        ref={composed}
        data-state="open"
        data-side={placement.side}
        data-align={placement.align}
        tabIndex={-1}
        style={{ position: "fixed", left: 0, top: 0, ...style }}
        {...dismissLayer.handlers}
        {...props}
      >
        <LayerDepth value={dismissLayer.depth}>{children}</LayerDepth>
      </div>
    </LayerPortal>
  );
}
