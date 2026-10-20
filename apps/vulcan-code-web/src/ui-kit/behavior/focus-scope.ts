import React from "react";
import { focusElement, focusableWithin } from "./core";

// Focus scope for overlays: move focus in when the layer opens, optionally keep Tab inside it, and give
// focus back to the element that had it when the layer closes. Both automatic moves are preventable through
// Radix-compatible callbacks so call sites keep their existing focus rules.
// 覆盖层的焦点范围：打开时把焦点移入，可选地将 Tab 限制在层内，关闭时把焦点还给打开前持有焦点的元素。
// 两次自动移动都可通过 Radix 兼容回调阻止，使调用处保留既有焦点规则。

// FocusScopeOptions configures one scope.
// FocusScopeOptions 配置一个焦点范围。
export interface FocusScopeOptions {
  // trapped keeps Tab and Shift+Tab cycling inside the scope (modal dialogs).
  // trapped 让 Tab 与 Shift+Tab 在范围内循环（模态对话框）。
  trapped?: boolean;
  // onOpenAutoFocus runs before focus moves in; preventDefault keeps focus where it is.
  // onOpenAutoFocus 在焦点移入前运行；preventDefault 保持焦点不动。
  onOpenAutoFocus?: (event: Event) => void;
  // onCloseAutoFocus runs before focus returns; preventDefault leaves focus alone.
  // onCloseAutoFocus 在焦点归还前运行；preventDefault 不再移动焦点。
  onCloseAutoFocus?: (event: Event) => void;
  // initialFocus picks the element to focus first; the first focusable descendant otherwise.
  // initialFocus 选择首个获得焦点的元素；否则为第一个可聚焦后代。
  initialFocus?: (scope: HTMLElement) => HTMLElement | null;
  // returnFocus picks the element that gets focus back on close; the element focused before opening otherwise.
  // returnFocus 选择关闭时取回焦点的元素；否则为打开前聚焦的元素。
  returnFocus?: () => HTMLElement | null;
}

// useFocusScope manages focus for an overlay element while `active`.
// useFocusScope 在 `active` 期间管理覆盖层元素的焦点。
export function useFocusScope(
  active: boolean,
  scope: React.RefObject<HTMLElement | null>,
  options: FocusScopeOptions = {},
): void {
  const latest = React.useRef(options);
  latest.current = options;
  // opening records the element focused when the scope opens. It is read while rendering the opening update,
  // because a child's autoFocus takes focus during commit, before any effect could see the opener.
  // opening 记录范围打开时持有焦点的元素。它在渲染打开更新时读取，因为子元素的 autoFocus 会在提交阶段、
  // 任何副作用运行之前夺走焦点。
  const [opening, setOpening] = React.useState<{ active: boolean; opener: HTMLElement | null }>({ active: false, opener: null });
  if (opening.active !== active) {
    setOpening({ active, opener: active && document.activeElement instanceof HTMLElement ? document.activeElement : null });
  }
  const opener = opening.opener;
  React.useEffect(() => {
    if (!active) return;
    const element = scope.current;
    if (!element) return;
    const focused = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const previous = opener && !element.contains(opener) ? opener : focused && !element.contains(focused) ? focused : null;
    // A child may already have taken focus (autoFocus on an input); keep it.
    // 子元素可能已经取得焦点（输入框的 autoFocus）；保留它。
    if (!element.contains(document.activeElement)) {
      const openEvent = new Event("vk.focusScope.open", { cancelable: true });
      latest.current.onOpenAutoFocus?.(openEvent);
      if (!openEvent.defaultPrevented) {
        const target = latest.current.initialFocus?.(element) ?? focusableWithin(element)[0] ?? element;
        focusElement(target);
      }
    }
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Tab" || !latest.current.trapped || event.defaultPrevented) return;
      const items = focusableWithin(element);
      if (items.length === 0) {
        event.preventDefault();
        return;
      }
      const first = items[0];
      const last = items[items.length - 1];
      if (event.shiftKey && (document.activeElement === first || !element.contains(document.activeElement))) {
        event.preventDefault();
        focusElement(last);
      } else if (!event.shiftKey && (document.activeElement === last || !element.contains(document.activeElement))) {
        event.preventDefault();
        focusElement(first);
      }
    };
    element.addEventListener("keydown", onKeyDown);
    return () => {
      element.removeEventListener("keydown", onKeyDown);
      const closeEvent = new Event("vk.focusScope.close", { cancelable: true });
      latest.current.onCloseAutoFocus?.(closeEvent);
      if (closeEvent.defaultPrevented) return;
      // Return focus only when it would otherwise be lost (inside the closing layer or on <body>).
      // 仅在焦点本会丢失（位于正在关闭的层内或 <body> 上）时归还焦点。
      const current = document.activeElement;
      const lost = !current || current === document.body || element.contains(current) || !current.isConnected;
      const target = latest.current.returnFocus?.() ?? previous;
      if (lost && target?.isConnected) focusElement(target);
    };
  }, [active, scope, opener]);
}
