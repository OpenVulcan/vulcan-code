import * as React from "react";
import type { BrowserDomRevision } from "./browser-dom-assets";
import type { HeldBrowserPointer } from "./use-browser-input-queue";

// Website markup is inert, with all network loading confined to locally materialized assets.
// 网站标记保持惰性，所有资源加载均限定为本地生成的资源。
const FRAME_DOCUMENT = '<!doctype html><html><head><meta http-equiv="Content-Security-Policy" content="default-src \'none\'; img-src blob: data:; style-src \'unsafe-inline\' blob:; font-src blob: data:; media-src blob: data:; base-uri \'none\'; form-action \'none\'"></head><body></body></html>';
const NODE_ID = "data-vulcan-node";

// The parent supplies the projected revision, current control grant and ordered input queue.
// 父组件提供投影修订、当前控制授权与有序输入队列。
type SurfaceProps = {
  snapshot: BrowserDomRevision;
  title: string;
  ownsControl: boolean;
  heldPointer: React.RefObject<HeldBrowserPointer | null>;
  heldKeys: React.RefObject<Set<string>>;
  queueInput: (kind: string, payload: Record<string, unknown>) => Promise<boolean>;
  queuePointerMove: (payload: Record<string, unknown>) => void;
};

// Local editor state is retained only while a candidate or committed edit is in flight.
// 只在候选词或已提交编辑仍在处理时保留本地编辑器状态。
type Field = HTMLInputElement | HTMLTextAreaElement;
type Editor = { field: Field; before: string };

// A text field must provide native composition and UTF-16 selection in its own frame realm.
// 文本框必须在所在框架中提供原生输入法组合和 UTF-16 选区。
function editable(target: EventTarget | null): Field | null {
  const field = target as Field | null;
  if (!field || (field.tagName !== "INPUT" && field.tagName !== "TEXTAREA")
    || field.readOnly || field.disabled || field.selectionStart === null
    || field.type === "password" || !field.getAttribute(NODE_ID)) return null;
  return field;
}

// Compute the smallest replacement by Unicode code points and report UTF-16 offsets to Chromium.
// 按 Unicode 码点计算最小替换范围，并向 Chromium 报告 UTF-16 偏移量。
function textDelta(before: string, after: string) {
  const previous = Array.from(before);
  const next = Array.from(after);
  let start = 0;
  while (start < previous.length && start < next.length && previous[start] === next[start]) start++;
  let end = previous.length;
  let nextEnd = next.length;
  while (end > start && nextEnd > start && previous[end - 1] === next[nextEnd - 1]) {
    end--;
    nextEnd--;
  }
  return { start: previous.slice(0, start).join("").length, end: previous.slice(0, end).join("").length,
    text: next.slice(start, nextEnd).join("") };
}

// Match stable IDs before comparing positional text and comments.
// 优先匹配稳定标识，再比较按位置排列的文本与注释。
function nodeId(node: Node | null): string | null {
  return node?.nodeType === Node.ELEMENT_NODE ? (node as Element).getAttribute(NODE_ID) : null;
}

// Synchronize the live tree without remounting fields or pointer targets on every revision.
// 同步实时树，在各次修订间保留输入框和指针目标。
function reconcile(target: Node, source: Node, preserve: (element: Element) => boolean): void {
  if (target.nodeType === Node.TEXT_NODE || target.nodeType === Node.COMMENT_NODE) {
    if (target.nodeValue !== source.nodeValue) target.nodeValue = source.nodeValue;
    return;
  }
  const element = target as Element;
  const desired = source as Element;
  if (preserve(element)) return;
  for (const attr of Array.from(element.attributes)) {
    if (!desired.hasAttributeNS(attr.namespaceURI, attr.localName)) element.removeAttributeNS(attr.namespaceURI, attr.localName);
  }
  for (const attr of Array.from(desired.attributes)) {
    if (element.getAttributeNS(attr.namespaceURI, attr.localName) !== attr.value) {
      element.setAttributeNS(attr.namespaceURI, attr.name, attr.value);
    }
  }
  const keyed = new Map<string, Node>();
  for (const child of Array.from(element.childNodes)) {
    const id = nodeId(child);
    if (id) keyed.set(id, child);
  }
  let cursor = element.firstChild;
  for (const child of Array.from(desired.childNodes)) {
    const id = nodeId(child);
    const match = id ? keyed.get(id) : cursor && !nodeId(cursor) && cursor.nodeName === child.nodeName ? cursor : null;
    if (match && match.nodeName === child.nodeName) {
      if (match !== cursor) element.insertBefore(match, cursor);
      reconcile(match, child, preserve);
      cursor = match.nextSibling;
    } else {
      element.insertBefore(element.ownerDocument.importNode(child, true), cursor);
    }
  }
  while (cursor) {
    const next = cursor.nextSibling;
    cursor.remove();
    cursor = next;
  }
  if (element.tagName === "INPUT" || element.tagName === "TEXTAREA") {
    const field = element as Field;
    const value = (desired as Field).value;
    if (field.type !== "file" && field.type !== "password" && field.value !== value) field.value = value;
    if (field.tagName === "INPUT") (field as HTMLInputElement).checked = (desired as HTMLInputElement).checked;
  } else if (element.tagName === "OPTION") {
    (element as HTMLOptionElement).selected = (desired as HTMLOptionElement).selected;
  }
}

// Render the projection in a sandboxed frame and forward interactions in source viewport coordinates.
// 在沙箱框架内渲染投影，并以源页面视口坐标转发交互。
export function BrowserDomSurface(props: SurfaceProps) {
  // Keep the registered frame listeners live across snapshots without resetting an active drag or IME.
  // 保持已注册的框架监听器跨快照可用，避免重置正在进行的拖动或输入法。
  const latest = React.useRef(props);
  latest.current = props;
  const stage = React.useRef<HTMLDivElement>(null);
  const iframe = React.useRef<HTMLIFrameElement>(null);
  const composition = React.useRef<Editor | null>(null);
  const pending = React.useRef(new Map<Field, Promise<boolean>>());
  const previousScroll = React.useRef(new Set<string>());
  const [frameDocument, setFrameDocument] = React.useState<Document | null>(null);
  const [bounds, setBounds] = React.useState({ width: 0, height: 0 });
  // Reconcile an edit refused by Core even if the authoritative revision is unchanged.
  // 即使权威修订没有变化，也重新协调被 Core 拒绝的编辑。
  const [refusedEdits, setRefusedEdits] = React.useState(0);
  const { snapshot } = props;
  const scale = Math.min(bounds.width / snapshot.width, bounds.height / snapshot.height);

  React.useLayoutEffect(() => {
    const element = stage.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => {
      setBounds({ width: entry.contentRect.width, height: entry.contentRect.height });
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  React.useLayoutEffect(() => {
    if (!frameDocument) return;
    const source = new DOMParser().parseFromString(snapshot.html, "text/html");
    // The frame's CSP is installed by srcdoc. Match it as the first head element on every patch.
    // 框架的 CSP 由 srcdoc 安装。每次更新都将它作为第一个 head 元素匹配。
    const policy = frameDocument.head.querySelector('meta[http-equiv="Content-Security-Policy"]');
    if (policy) source.head.prepend(source.importNode(policy, true));
    reconcile(frameDocument.documentElement, source.documentElement,
      element => element === composition.current?.field || pending.current.has(element as Field));
    for (const field of pending.current.keys()) {
      // Pending fields are protected by their dirty value until the queued edit is acknowledged.
      // 待确认字段利用其已修改的值保持本地文本，直到队列确认编辑。
      if (!field.isConnected) pending.current.delete(field);
    }
    const scrolled = new Set(snapshot.scroll.map(([id]) => id));
    for (const id of previousScroll.current) if (!scrolled.has(id)) {
      frameDocument.querySelector(`[${NODE_ID}="${CSS.escape(id)}"]`)?.scrollTo(0, 0);
    }
    for (const [id, x, y] of snapshot.scroll) {
      frameDocument.querySelector(`[${NODE_ID}="${CSS.escape(id)}"]`)?.scrollTo(x, y);
    }
    previousScroll.current = scrolled;
    const viewport = frameDocument.defaultView;
    if (viewport && (viewport.scrollX !== snapshot.windowScroll[0] || viewport.scrollY !== snapshot.windowScroll[1])) {
      viewport.scrollTo(...snapshot.windowScroll);
    }
    if (document.activeElement === iframe.current && snapshot.focused
      && !composition.current && !pending.current.size && !props.heldPointer.current) {
      const focused = frameDocument.querySelector<HTMLElement>(`[${NODE_ID}="${CSS.escape(snapshot.focused)}"]`);
      if (focused && focused !== frameDocument.activeElement) focused.focus({ preventScroll: true });
    }
  }, [frameDocument, snapshot, props.heldPointer, refusedEdits]);

  React.useEffect(() => {
    if (!frameDocument) return;
    const doc = frameDocument;
    const beforeValues = new WeakMap<Field, string>();
    const view = doc.defaultView as (Window & typeof globalThis) | null;
    if (!view) return;
    const listeners = new view.AbortController();
    const options = { capture: true, signal: listeners.signal };

    // Replay each committed candidate or normal edit once, then resume source reconciliation.
    // 每个已确认候选词或普通编辑仅回放一次，随后恢复源页面协调。
    function commit({ field, before }: Editor) {
      const current = latest.current;
      if (!current.ownsControl || before === field.value) return;
      const id = field.getAttribute(NODE_ID);
      const operation = current.queueInput("textEdit", {
        id, documentId: current.snapshot.documentId, before, ...textDelta(before, field.value),
      });
      pending.current.set(field, operation);
      void operation.then(applied => {
        if (pending.current.get(field) !== operation) return;
        pending.current.delete(field);
        if (!applied) setRefusedEdits(value => value + 1);
      });
    }

    doc.addEventListener("compositionstart", event => {
      const field = editable(event.target);
      if (field && latest.current.ownsControl) composition.current = { field, before: field.value };
    }, options);
    doc.addEventListener("compositionend", () => {
      const editor = composition.current;
      composition.current = null;
      if (editor) {
        beforeValues.delete(editor.field);
        commit(editor);
      }
    }, options);
    doc.addEventListener("beforeinput", event => {
      if (!latest.current.ownsControl) { event.preventDefault(); return; }
      const field = editable(event.target);
      if (field && !composition.current && !event.isComposing) beforeValues.set(field, field.value);
    }, options);
    doc.addEventListener("input", event => {
      const field = editable(event.target);
      if (!field || composition.current || (event as InputEvent).isComposing) return;
      const before = beforeValues.get(field);
      beforeValues.delete(field);
      if (before !== undefined) commit({ field, before });
    }, options);

    doc.addEventListener("pointerdown", event => {
      const current = latest.current;
      if (!current.ownsControl || !event.isPrimary || current.heldPointer.current) return;
      const element = event.target as Element;
      // Only fields need native pointer defaults for focus and selection. Other drags run in Core.
      // 只有表单字段需要原生指针默认行为来获得焦点与选区，其他拖动由 Core 执行。
      if (!element.closest("input,textarea,select")) event.preventDefault();
      const button = event.button === 2 ? "right" : event.button === 1 ? "middle" : "left";
      const coordinates = { x: event.clientX, y: event.clientY };
      current.heldPointer.current = { pointerId: event.pointerId, button, ...coordinates };
      if (event.isTrusted) element.setPointerCapture(event.pointerId);
      void current.queueInput("pointer", { type: "down", button, ...coordinates });
    }, options);
    doc.addEventListener("pointermove", event => {
      const current = latest.current;
      const held = current.heldPointer.current;
      if (!current.ownsControl || (held && held.pointerId !== event.pointerId)) return;
      const coordinates = { x: event.clientX, y: event.clientY };
      if (held) current.heldPointer.current = { ...held, ...coordinates };
      current.queuePointerMove({ type: "move", ...coordinates });
    }, options);

    // Release the exact pressed button after the last queued drag position.
    // 在最后一个排队的拖动位置之后释放确切的已按下按钮。
    function releasePointer(event: PointerEvent) {
      const current = latest.current;
      const held = current.heldPointer.current;
      if (!held || held.pointerId !== event.pointerId) return;
      current.heldPointer.current = null;
      const coordinates = event.type === "pointerup" ? { x: event.clientX, y: event.clientY } : { x: held.x, y: held.y };
      void current.queueInput("pointer", { type: "up", button: held.button, ...coordinates });
    }

    doc.addEventListener("pointerup", releasePointer, options);
    doc.addEventListener("pointercancel", releasePointer, options);
    doc.addEventListener("lostpointercapture", releasePointer, options);
    for (const event of ["click", "auxclick", "contextmenu", "submit", "dragstart"]) {
      doc.addEventListener(event, event => event.preventDefault(), options);
    }
    doc.addEventListener("wheel", event => {
      event.preventDefault();
      const current = latest.current;
      if (!current.ownsControl) return;
      const factor = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? current.snapshot.height : 1;
      void current.queueInput("wheel", { x: event.clientX, y: event.clientY,
        deltaX: event.deltaX * factor, deltaY: event.deltaY * factor });
    }, { ...options, passive: false });
    doc.addEventListener("keydown", event => {
      const current = latest.current;
      if (!current.ownsControl) { event.preventDefault(); return; }
      if (event.isComposing || event.key === "Process") return;
      const field = editable(event.target);
      // Local text edits create input events. Forward only application commands as keys.
      // 本地文字编辑会产生 input 事件。仅将应用命令作为按键转发。
      if (field && (event.key.length === 1 || ["Backspace", "Delete", "Home", "End", "ArrowLeft", "ArrowRight"].includes(event.key)
        || (field.tagName === "TEXTAREA" && event.key === "Enter"))) return;
      event.preventDefault();
      if (event.repeat) return;
      current.heldKeys.current.add(event.key);
      void current.queueInput("key", { type: "down", key: event.key });
    }, options);
    doc.addEventListener("keyup", event => {
      const current = latest.current;
      if (current.heldKeys.current.delete(event.key)) void current.queueInput("key", { type: "up", key: event.key });
    }, options);
    view.addEventListener("blur", event => {
      // Moving focus between elements inside the frame must leave the drag active.
      // 框架内元素之间移动焦点时应保持拖动有效。
      if (event.target !== view) return;
      const current = latest.current;
      for (const key of current.heldKeys.current) void current.queueInput("key", { type: "up", key });
      current.heldKeys.current.clear();
      const held = current.heldPointer.current;
      if (held) {
        current.heldPointer.current = null;
        void current.queueInput("pointer", { type: "up", button: held.button, x: held.x, y: held.y });
      }
    }, options);
    return () => listeners.abort();
  }, [frameDocument]);

  return (
    <div ref={stage} className="vk-browser-dom-surface">
      <iframe ref={iframe} title={props.title} sandbox="allow-same-origin" srcDoc={FRAME_DOCUMENT}
        tabIndex={props.ownsControl ? 0 : -1}
        onLoad={() => setFrameDocument(iframe.current?.contentDocument ?? null)}
        style={{ width: snapshot.width, height: snapshot.height, transform: `translate(-50%, -50%) scale(${scale})` }} />
    </div>
  );
}
