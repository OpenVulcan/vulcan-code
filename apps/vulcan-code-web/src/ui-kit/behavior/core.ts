import React from "react";

// cx joins class names, skipping empty values. Kit classes never conflict, so no merge step is needed.
// cx 拼接类名并跳过空值。kit 类名互不冲突，因此无需合并步骤。
export function cx(...values: Array<string | false | null | undefined>): string {
  let out = "";
  for (const value of values) if (value) out = out ? `${out} ${value}` : value;
  return out;
}

// setRef assigns one value to a callback or object ref.
// setRef 将一个值赋给回调或对象引用。
function setRef<T>(ref: React.Ref<T> | undefined, value: T | null): void {
  if (typeof ref === "function") ref(value);
  else if (ref) (ref as React.MutableRefObject<T | null>).current = value;
}

// composeRefs returns one callback ref that forwards to every given ref.
// composeRefs 返回一个转发到所有给定引用的回调引用。
export function composeRefs<T>(...refs: Array<React.Ref<T> | undefined>): React.RefCallback<T> {
  return (value) => refs.forEach((ref) => setRef(ref, value));
}

// useComposedRefs memoizes composeRefs for the given refs.
// useComposedRefs 为给定引用缓存 composeRefs 的结果。
export function useComposedRefs<T>(...refs: Array<React.Ref<T> | undefined>): React.RefCallback<T> {
  return React.useCallback(composeRefs(...refs), refs);
}

// composeHandlers runs the caller handler first and the internal one unless the caller prevented default.
// composeHandlers 先运行调用方处理器，调用方未阻止默认行为时再运行内部处理器。
export function composeHandlers<E extends { defaultPrevented: boolean }>(
  external: ((event: E) => void) | undefined,
  internal: (event: E) => void,
): (event: E) => void {
  return (event) => {
    external?.(event);
    if (!event.defaultPrevented) internal(event);
  };
}

// useControllableState follows a controlled value when one is given and keeps its own state otherwise.
// useControllableState 在给定受控值时跟随该值，否则维护自身状态。
export function useControllableState<T>(
  controlled: T | undefined,
  defaultValue: T,
  onChange?: (value: T) => void,
): [T, (value: T) => void] {
  const [own, setOwn] = React.useState(defaultValue);
  const isControlled = controlled !== undefined;
  const value = isControlled ? controlled : own;
  const onChangeRef = React.useRef(onChange);
  onChangeRef.current = onChange;
  const valueRef = React.useRef(value);
  valueRef.current = value;
  const set = React.useCallback((next: T) => {
    if (Object.is(next, valueRef.current)) return;
    if (!isControlled) setOwn(next);
    onChangeRef.current?.(next);
  }, [isControlled]);
  return [value, set];
}

// useStableId returns a document-unique id, preferring a caller id.
// useStableId 返回文档内唯一的 id，优先使用调用方 id。
export function useStableId(prefix: string, given?: string): string {
  const generated = React.useId();
  return given ?? `${prefix}-${generated.replace(/:/g, "")}`;
}

// usePressToggle returns trigger handlers that act on a primary pointer press (so menus open as the button
// goes down) and on a click that no press preceded (keyboard activation, assistive tools, scripted clicks).
// usePressToggle 返回触发器处理器：在主指针按下时生效（菜单随按下即打开），也在没有按下前导的点击时生效
// （键盘激活、辅助工具与脚本点击）。
export function usePressToggle(onPress: (via: "pointer" | "click") => void): {
  onPointerDown: (event: React.PointerEvent<HTMLElement>) => void;
  onClick: (event: React.MouseEvent<HTMLElement>) => void;
} {
  const pressed = React.useRef(false);
  const latest = React.useRef(onPress);
  latest.current = onPress;
  return React.useMemo(() => ({
    onPointerDown: (event: React.PointerEvent<HTMLElement>) => {
      if (event.button !== 0 || event.ctrlKey || (event.currentTarget as HTMLButtonElement).disabled) return;
      pressed.current = true;
      window.setTimeout(() => {
        pressed.current = false;
      }, 1000);
      latest.current("pointer");
    },
    onClick: (event: React.MouseEvent<HTMLElement>) => {
      if (pressed.current) {
        pressed.current = false;
        return;
      }
      if (event.defaultPrevented) return;
      latest.current("click");
    },
  }), []);
}

// FOCUSABLE_SELECTOR matches elements that can take keyboard focus.
// FOCUSABLE_SELECTOR 匹配可获得键盘焦点的元素。
export const FOCUSABLE_SELECTOR = [
  "a[href]", "button:not([disabled])", "input:not([disabled]):not([type=hidden])", "select:not([disabled])",
  "textarea:not([disabled])", "[tabindex]:not([tabindex='-1'])", "[contenteditable='true']",
].join(",");

// focusableWithin lists visible focusable descendants in DOM order.
// focusableWithin 按 DOM 顺序列出可见的可聚焦后代。
export function focusableWithin(root: HTMLElement): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)).filter(
    (element) => !element.hasAttribute("disabled") && element.getAttribute("aria-hidden") !== "true",
  );
}

// focusElement moves focus without scrolling the page.
// focusElement 移动焦点且不滚动页面。
export function focusElement(element: HTMLElement | null | undefined): void {
  element?.focus({ preventScroll: true });
}

// keyboardModality records whether the latest user input was a key press rather than a pointer.
// keyboardModality 记录最近一次用户输入是否为按键而非指针。
let keyboardModality = false;
// modalityTracked guards the one-time install of the document listeners.
// modalityTracked 保证文档监听器只安装一次。
let modalityTracked = false;

// trackInputModality installs capture listeners that keep keyboardModality current.
// trackInputModality 安装捕获阶段监听器以保持 keyboardModality 为最新值。
export function trackInputModality(): void {
  if (modalityTracked || typeof document === "undefined") return;
  modalityTracked = true;
  document.addEventListener("keydown", () => { keyboardModality = true; }, true);
  const pointer = () => { keyboardModality = false; };
  document.addEventListener("pointerdown", pointer, true);
  document.addEventListener("pointermove", pointer, true);
}

// lastInputWasKeyboard reports whether the latest input was a key press.
// lastInputWasKeyboard 报告最近一次输入是否为按键。
// Returns: true after a keydown with no pointer activity since.
// 返回值：在一次按键之后且此后无指针活动时为 true。
export function lastInputWasKeyboard(): boolean {
  return keyboardModality;
}
