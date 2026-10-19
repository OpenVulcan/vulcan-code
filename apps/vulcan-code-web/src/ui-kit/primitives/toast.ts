import type * as React from "react";

// Toasts: short-lived notices in the bottom-right corner. The `toast` API keeps the call shape of the library
// it replaces (sonner): a tone method takes the message and options, returns the toast id, and reusing an id
// updates that toast in place, so a loading notice can turn into its result. The store lives at module level
// so any code can raise a notice; the Toaster in ./toaster renders it.
// 通知：右下角的短时提示。`toast` API 保留其替代库（sonner）的调用形态：语气方法接收消息与选项并返回通知标识，
// 复用标识会就地更新该通知，使加载提示可以变为结果。存储位于模块级，任何代码都能发出提示；./toaster 中的 Toaster
// 负责渲染。

// ToastTone is the kind of notice; it sets the icon, the colour accent and the default lifetime.
// ToastTone 是提示种类；决定图标、强调色与默认存续时间。
export type ToastTone = "default" | "success" | "info" | "warning" | "error" | "loading";

// ToastId identifies one toast; callers pass strings, generated ids are numbers.
// ToastId 标识一个通知；调用方传入字符串，自动生成的标识为数字。
export type ToastId = string | number;

// ToastAction is the single inline action button; preventDefault in onClick keeps the toast open.
// ToastAction 是唯一的行内操作按钮；在 onClick 中调用 preventDefault 可保持通知打开。
export interface ToastAction {
  label: React.ReactNode;
  onClick: (event: React.MouseEvent<HTMLButtonElement>) => void;
}

// ToastOptions are the per-toast options call sites use.
// ToastOptions 是调用处使用的单个通知选项。
export interface ToastOptions {
  id?: ToastId;
  description?: React.ReactNode;
  // duration in milliseconds; Infinity keeps the toast until dismissed.
  // duration 以毫秒计；Infinity 表示保留到被关闭为止。
  duration?: number;
  action?: ToastAction;
  icon?: React.ReactNode;
  className?: string;
  onDismiss?: (toast: ToastRecord) => void;
  onAutoClose?: (toast: ToastRecord) => void;
}

// ToastRecord is one live toast in the store.
// ToastRecord 是存储中的一个存活通知。
export interface ToastRecord extends ToastOptions {
  id: ToastId;
  tone: ToastTone;
  title: React.ReactNode;
  // revision changes on every in-place update so the lifetime timer restarts.
  // revision 在每次就地更新时变化，使存续计时器重新开始。
  revision: number;
}

// DEFAULT_DURATION_MS is how long a notice stays without an explicit duration.
// DEFAULT_DURATION_MS 是未指定时长时提示的停留时间。
export const DEFAULT_DURATION_MS = 4000;

// MAX_TOASTS bounds the stack; the oldest notices give way first.
// MAX_TOASTS 限制堆叠数量；最早的提示先让位。
export const MAX_TOASTS = 5;

let records: ToastRecord[] = [];
let nextId = 1;
let revision = 0;
const listeners = new Set<() => void>();

// emit publishes a new snapshot to the Toaster.
// emit 向 Toaster 发布新快照。
function emit(next: ToastRecord[]): void {
  records = next;
  for (const listener of listeners) listener();
}

// subscribeToasts and getToasts are the external-store pair the Toaster reads.
// subscribeToasts 与 getToasts 是 Toaster 读取的外部存储接口对。
export function subscribeToasts(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function getToasts(): ToastRecord[] {
  return records;
}

// show adds a toast, or updates the toast that already has the same id.
// show 添加通知，或更新已有相同标识的通知。
function show(tone: ToastTone, title: React.ReactNode, options: ToastOptions = {}): ToastId {
  const id = options.id ?? nextId++;
  const record: ToastRecord = { ...options, id, tone, title, revision: ++revision };
  const index = records.findIndex((item) => item.id === id);
  if (index >= 0) {
    const next = records.slice();
    next[index] = record;
    emit(next);
  } else {
    emit([...records, record].slice(-MAX_TOASTS));
  }
  return id;
}

// removeToast drops a toast without running its callbacks (used after they already ran).
// removeToast 移除通知而不运行其回调（用于回调已经运行之后）。
export function removeToast(id: ToastId): void {
  if (records.some((item) => item.id === id)) emit(records.filter((item) => item.id !== id));
}

// dismiss closes one toast, or every toast without an id, and runs onDismiss.
// dismiss 关闭一个通知；不传标识时关闭全部，并运行 onDismiss。
function dismiss(id?: ToastId): void {
  const closing = id === undefined ? records : records.filter((item) => item.id === id);
  if (closing.length === 0) return;
  emit(records.filter((item) => !closing.includes(item)));
  for (const item of closing) item.onDismiss?.(item);
}

// toast is the notice API; calling it directly raises a neutral notice.
// toast 是提示 API；直接调用会发出中性提示。
export const toast = Object.assign(
  (title: React.ReactNode, options?: ToastOptions) => show("default", title, options),
  {
    success: (title: React.ReactNode, options?: ToastOptions) => show("success", title, options),
    info: (title: React.ReactNode, options?: ToastOptions) => show("info", title, options),
    warning: (title: React.ReactNode, options?: ToastOptions) => show("warning", title, options),
    error: (title: React.ReactNode, options?: ToastOptions) => show("error", title, options),
    loading: (title: React.ReactNode, options?: ToastOptions) => show("loading", title, options),
    message: (title: React.ReactNode, options?: ToastOptions) => show("default", title, options),
    dismiss,
  },
);
