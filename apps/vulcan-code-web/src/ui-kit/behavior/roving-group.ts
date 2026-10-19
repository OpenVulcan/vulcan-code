import React from "react";
import { focusElement } from "./core";

// Roving tab stop for radio groups, segmented controls and tab lists: the group is one Tab stop, arrow keys
// move between its items, and the selected item (or the first enabled one) is where focus lands.
// 单选组、分段控件与标签列表的漫游焦点：整组只占一个 Tab 位置，方向键在项之间移动，焦点落在选中项（或首个可用项）上。

// RovingOrientation limits which arrow keys move focus.
// RovingOrientation 限定哪些方向键移动焦点。
export type RovingOrientation = "horizontal" | "vertical" | "both";

// isEnabled reports whether a group item can take focus.
// isEnabled 判断组内项是否可以获得焦点。
function isEnabled(item: HTMLElement): boolean {
  return !(item as HTMLButtonElement).disabled && !item.hasAttribute("data-disabled") && item.getAttribute("aria-disabled") !== "true";
}

// isSelected reports whether a group item is the current choice.
// isSelected 判断组内项是否为当前选择。
function isSelected(item: HTMLElement): boolean {
  return item.getAttribute("aria-checked") === "true" || item.getAttribute("aria-selected") === "true" || item.dataset.state === "active";
}

// useRovingGroup keeps exactly one tab stop among the items matched by `selector` inside `root`.
// `onFocusItem` runs when arrow keys move focus (tabs and radios select on focus).
// useRovingGroup 在 `root` 内与 `selector` 匹配的项中保持唯一的 Tab 位置；方向键移动焦点时调用 `onFocusItem`
// （标签与单选在获得焦点时即选中）。
export function useRovingGroup(
  root: React.RefObject<HTMLElement | null>,
  selector: string,
  orientation: RovingOrientation,
  options: { loop?: boolean; onFocusItem?: (item: HTMLElement) => void; dir?: "ltr" | "rtl" } = {},
): { tabIndex: number; onFocus: React.FocusEventHandler<HTMLElement>; onKeyDown: React.KeyboardEventHandler<HTMLElement> } {
  const latest = React.useRef(options);
  latest.current = options;
  const items = React.useCallback(
    () => (root.current ? Array.from(root.current.querySelectorAll<HTMLElement>(selector)).filter((item) => item.closest("[data-vk-roving]") === root.current) : []),
    [root, selector],
  );
  // Recompute the tab stop after every render: selection and disabled flags may have changed.
  // 每次渲染后重新计算 Tab 位置：选中与禁用状态可能已经变化。
  React.useLayoutEffect(() => {
    const all = items();
    const selected = all.find(isSelected);
    const stop = selected ? (isEnabled(selected) ? selected : null) : all.find(isEnabled) ?? null;
    for (const item of all) item.tabIndex = item === stop ? 0 : -1;
  });
  const onFocus = React.useCallback((event: React.FocusEvent<HTMLElement>) => {
    if (event.target !== event.currentTarget) return;
    const all = items().filter(isEnabled);
    const target = all.find((item) => item.tabIndex === 0) ?? all[0];
    focusElement(target);
  }, [items]);
  const onKeyDown = React.useCallback((event: React.KeyboardEvent<HTMLElement>) => {
    const element = root.current;
    if (!element) return;
    if (event.key === "Tab" && event.shiftKey) {
      // Leaving backwards must not land on the group itself, which would bounce focus back in.
      // 反向离开时不能落在组自身上，否则焦点会被弹回组内。
      element.tabIndex = -1;
      window.setTimeout(() => {
        element.tabIndex = 0;
      }, 0);
      return;
    }
    const all = items().filter(isEnabled);
    const index = all.findIndex((item) => item === document.activeElement);
    if (index < 0) return;
    const rtl = latest.current.dir === "rtl";
    const forward = (orientation !== "vertical" && event.key === (rtl ? "ArrowLeft" : "ArrowRight")) || (orientation !== "horizontal" && event.key === "ArrowDown");
    const backward = (orientation !== "vertical" && event.key === (rtl ? "ArrowRight" : "ArrowLeft")) || (orientation !== "horizontal" && event.key === "ArrowUp");
    let next: HTMLElement | undefined;
    const loop = latest.current.loop ?? true;
    if (forward) next = all[index + 1] ?? (loop ? all[0] : undefined);
    else if (backward) next = all[index - 1] ?? (loop ? all[all.length - 1] : undefined);
    else if (event.key === "Home") next = all[0];
    else if (event.key === "End") next = all[all.length - 1];
    else return;
    event.preventDefault();
    if (!next) return;
    focusElement(next);
    latest.current.onFocusItem?.(next);
  }, [items, orientation, root]);
  return { tabIndex: 0, onFocus, onKeyDown };
}
