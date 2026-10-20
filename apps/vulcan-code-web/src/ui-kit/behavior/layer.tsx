import React from "react";
import { createPortal } from "react-dom";

// Overlay layers: every menu, popover, dialog and toast host is portaled into its own transparent
// `.vk-layer` element at the end of <body>, and every open dismissable layer joins one stack ordered by
// nesting depth, then by opening order. The top layer owns Escape. An outside press dismisses the layers
// above the highest blocking layer (dialogs, menus, listboxes) whose React tree does not contain the press, so
// a press inside a submenu or a select portaled out of a dialog still counts as inside that dialog.
// 覆盖层：每个菜单、弹出层、对话框与通知宿主都传送到 <body> 末尾各自透明的 `.vk-layer` 元素；所有打开的可关闭层
// 组成一个栈，先按嵌套深度、再按打开顺序排列。顶层负责 Esc。外部按下会关闭最高阻断层（对话框、菜单、列表框）及其上方、
// React 树不包含该按下的层，因此在从对话框传送出去的子菜单或选择框内按下仍算作对话框内部。

// LayerPortal renders children into a transparent `.vk-layer` host appended to <body>. The host exists from
// the first render and is attached in the mutation phase, so the portaled content mounts in the same commit and
// everything in the layout phase (React autoFocus, placement, focus scopes) already sees it in the document.
// LayerPortal 将子元素渲染到追加在 <body> 上的透明 `.vk-layer` 宿主中。宿主从首次渲染起就存在并在变更阶段挂载，
// 因此传送内容在同一次提交中挂载，布局阶段的一切（React autoFocus、定位、焦点范围）都已能看到它位于文档中。
export function LayerPortal(props: { children: React.ReactNode; className?: string; container?: HTMLElement | null }) {
  const [host] = React.useState(() => {
    const element = document.createElement("div");
    element.setAttribute("data-vk-portal", "");
    return element;
  });
  host.className = props.className ? `vk-layer ${props.className}` : "vk-layer";
  React.useInsertionEffect(() => {
    (props.container ?? document.body).appendChild(host);
    return () => {
      host.remove();
    };
  }, [host, props.container]);
  return createPortal(props.children, host);
}

// LayerDepthContext tells a layer how deeply it is nested inside other layers.
// LayerDepthContext 告知层在其他层中的嵌套深度。
const LayerDepthContext = React.createContext(0);

// LayerDepth provides the depth for layers rendered inside one layer.
// LayerDepth 为某一层内部渲染的层提供深度。
export const LayerDepth = LayerDepthContext.Provider;

// DismissableEntry is one open layer in the stack.
// DismissableEntry 是栈中的一个打开层。
interface DismissableEntry {
  depth: number;
  order: number;
  blocking: boolean;
  // insideTree is set by the layer's own capture handlers while an event travels through its React tree.
  // insideTree 在事件经过该层的 React 树时由其捕获处理器设置。
  insideTree: boolean;
  element: () => HTMLElement | null;
  // Extra elements that count as inside (the trigger, so pressing it toggles instead of re-opening).
  // 视为内部的额外元素（触发器，使按下它时切换而不是重新打开）。
  branches: () => Array<HTMLElement | null>;
  onEscape: (event: KeyboardEvent) => void;
  onPointerDownOutside: (event: PointerEvent) => void;
  onFocusOutside: (event: FocusEvent) => void;
}

// stack holds the open layers; ordered() sorts them from bottom to top.
// stack 保存打开的层；ordered() 将其自底向顶排序。
const stack: DismissableEntry[] = [];
let nextOrder = 0;

// ordered returns the open layers from bottom to top.
// ordered 自底向顶返回打开的层。
function ordered(): DismissableEntry[] {
  return [...stack].sort((a, b) => a.depth - b.depth || a.order - b.order);
}

// contains reports whether the event happened inside the layer.
// contains 判断事件是否发生在该层内部。
function contains(entry: DismissableEntry, target: Node): boolean {
  if (entry.insideTree || entry.element()?.contains(target)) return true;
  return entry.branches().some((branch) => branch?.contains(target));
}

// outsideCandidates lists the layers an outside interaction may dismiss, top first.
// outsideCandidates 自顶向下列出外部交互可能关闭的层。
function outsideCandidates(): DismissableEntry[] {
  const layers = ordered().reverse();
  const blockingIndex = layers.findIndex((entry) => entry.blocking);
  return blockingIndex < 0 ? layers : layers.slice(0, blockingIndex + 1);
}

// resetTreeFlags clears the per-event inside markers after one event was handled.
// resetTreeFlags 在处理完一次事件后清除逐事件的内部标记。
function resetTreeFlags(): void {
  for (const entry of stack) entry.insideTree = false;
}

// listening tracks whether the shared document listeners are attached.
// listening 记录共享文档监听器是否已挂载。
let listening = false;

function onDocumentKeyDown(event: KeyboardEvent): void {
  if (event.key !== "Escape" || stack.length === 0) return;
  const layers = ordered();
  layers[layers.length - 1].onEscape(event);
}

function onDocumentPointerDown(event: PointerEvent): void {
  if (stack.length === 0 || !(event.target instanceof Node)) return;
  const target = event.target;
  const candidates = outsideCandidates().filter((entry) => !contains(entry, target));
  resetTreeFlags();
  for (const entry of candidates) entry.onPointerDownOutside(event);
}

function onDocumentFocusIn(event: FocusEvent): void {
  if (stack.length === 0 || !(event.target instanceof Node)) return;
  const target = event.target;
  const candidates = outsideCandidates().filter((entry) => !contains(entry, target));
  resetTreeFlags();
  for (const entry of candidates) entry.onFocusOutside(event);
}

function attach(): void {
  if (listening) return;
  listening = true;
  // Bubble phase: React's capture handlers on the layers have already marked presses inside their trees.
  // 冒泡阶段：各层上的 React 捕获处理器已经标记了发生在其树内的按下。
  document.addEventListener("keydown", onDocumentKeyDown);
  document.addEventListener("pointerdown", onDocumentPointerDown);
  document.addEventListener("focusin", onDocumentFocusIn);
}

function detach(): void {
  if (!listening || stack.length > 0) return;
  listening = false;
  document.removeEventListener("keydown", onDocumentKeyDown);
  document.removeEventListener("pointerdown", onDocumentPointerDown);
  document.removeEventListener("focusin", onDocumentFocusIn);
}

// DismissOptions are the Radix-compatible dismissal callbacks a layer accepts. Each receives the native
// event; calling preventDefault keeps the layer open.
// DismissOptions 是层接受的 Radix 兼容关闭回调。每个回调都收到原生事件；调用 preventDefault 可保持层打开。
export interface DismissOptions {
  onEscapeKeyDown?: (event: KeyboardEvent) => void;
  onPointerDownOutside?: (event: PointerEvent) => void;
  onFocusOutside?: (event: FocusEvent) => void;
  onInteractOutside?: (event: PointerEvent | FocusEvent) => void;
}

// DismissableConfig adds branches, focus-outside dismissal and blocking to the callbacks.
// DismissableConfig 在回调之外增加分支元素、焦点外移关闭与阻断设置。
export interface DismissableConfig extends DismissOptions {
  branches?: Array<React.RefObject<HTMLElement | null>>;
  dismissOnFocusOutside?: boolean;
  // blocking layers (dialogs, menus, listboxes) shield the layers below from outside presses.
  // 阻断层（对话框、菜单、列表框）使下方的层不受外部按下影响。
  blocking?: boolean;
}

// DismissableLayer is what a layer spreads on its element and provides to its children.
// DismissableLayer 是层铺在自身元素上、并提供给子元素的内容。
export interface DismissableLayer {
  depth: number;
  handlers: {
    onPointerDownCapture: () => void;
    onFocusCapture: () => void;
  };
}

// useDismissable registers an open layer while `active`; `onDismiss` closes it. Spread `handlers` on the
// layer element and wrap its children in <LayerDepth value={depth}>.
// useDismissable 在 `active` 期间登记打开的层；`onDismiss` 负责关闭它。将 `handlers` 铺在层元素上，并用
// <LayerDepth value={depth}> 包裹其子元素。
export function useDismissable(
  active: boolean,
  element: React.RefObject<HTMLElement | null>,
  onDismiss: () => void,
  config: DismissableConfig = {},
): DismissableLayer {
  const depth = React.useContext(LayerDepthContext) + 1;
  const latest = React.useRef({ onDismiss, config });
  latest.current = { onDismiss, config };
  const entryRef = React.useRef<DismissableEntry | null>(null);
  React.useEffect(() => {
    if (!active) return;
    const entry: DismissableEntry = {
      depth,
      order: nextOrder++,
      blocking: latest.current.config.blocking ?? false,
      insideTree: false,
      element: () => element.current,
      branches: () => (latest.current.config.branches ?? []).map((branch) => branch.current),
      onEscape: (event) => {
        latest.current.config.onEscapeKeyDown?.(event);
        if (!event.defaultPrevented) {
          event.preventDefault();
          latest.current.onDismiss();
        }
      },
      onPointerDownOutside: (event) => {
        latest.current.config.onPointerDownOutside?.(event);
        latest.current.config.onInteractOutside?.(event);
        if (!event.defaultPrevented) latest.current.onDismiss();
      },
      onFocusOutside: (event) => {
        if (!latest.current.config.dismissOnFocusOutside) return;
        latest.current.config.onFocusOutside?.(event);
        latest.current.config.onInteractOutside?.(event);
        if (!event.defaultPrevented) latest.current.onDismiss();
      },
    };
    entryRef.current = entry;
    stack.push(entry);
    attach();
    return () => {
      const index = stack.indexOf(entry);
      if (index >= 0) stack.splice(index, 1);
      entryRef.current = null;
      detach();
    };
  }, [active, element, depth]);
  const handlers = React.useMemo(() => ({
    onPointerDownCapture: () => {
      if (entryRef.current) entryRef.current.insideTree = true;
    },
    onFocusCapture: () => {
      if (entryRef.current) entryRef.current.insideTree = true;
    },
  }), []);
  return { depth, handlers };
}

// isTopLayer reports whether the given element hosts the top-most open layer.
// isTopLayer 判断给定元素是否承载最顶层的打开层。
export function isTopLayer(element: HTMLElement | null): boolean {
  const layers = ordered();
  return layers.length > 0 && layers[layers.length - 1].element() === element;
}
