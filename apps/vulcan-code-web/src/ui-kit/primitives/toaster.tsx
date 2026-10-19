import * as React from "react";
import { CircleCheck, CircleX, Info, TriangleAlert, XIcon } from "lucide-react";
import { cx } from "../behavior/core";
import { LayerPortal } from "../behavior/layer";
import { DEFAULT_DURATION_MS, getToasts, removeToast, subscribeToasts, type ToastRecord, type ToastTone } from "./toast";
import { useUiText } from "./ui-text";

// Toaster renders the notices raised through `toast` in the bottom-right corner, above dialogs. Pointing at or
// focusing the stack pauses every lifetime timer, and so does a hidden window, so a notice is never missed
// while the user is reading it or away.
// Toaster 在右下角、对话框之上渲染通过 `toast` 发出的提示。指向或聚焦提示堆叠会暂停所有存续计时器，窗口隐藏时同样暂停，
// 用户阅读或离开期间不会错过提示。

// TONE_ICONS are the default leading icons; neutral notices show none.
// TONE_ICONS 是默认前置图标；中性提示不显示图标。
const TONE_ICONS: Partial<Record<ToastTone, React.ReactNode>> = {
  success: <CircleCheck />,
  info: <Info />,
  warning: <TriangleAlert />,
  error: <CircleX />,
  loading: <span className="vk-spinner" aria-hidden="true" />,
};

// ToastItem renders one notice and owns its lifetime timer.
// ToastItem 渲染一条提示并负责其存续计时器。
function ToastItem({ record, paused, closeLabel }: { record: ToastRecord; paused: boolean; closeLabel: string }) {
  const remaining = React.useRef(record.duration ?? DEFAULT_DURATION_MS);
  const latest = React.useRef(record);
  latest.current = record;
  // An in-place update restarts the lifetime.
  // 就地更新会重新开始存续时间。
  React.useEffect(() => {
    remaining.current = record.duration ?? DEFAULT_DURATION_MS;
  }, [record.revision, record.duration]);
  React.useEffect(() => {
    const lifetime = remaining.current;
    if (paused || record.tone === "loading" || !Number.isFinite(lifetime)) return;
    const started = Date.now();
    const timer = window.setTimeout(() => {
      removeToast(latest.current.id);
      latest.current.onAutoClose?.(latest.current);
    }, Math.max(0, lifetime));
    return () => {
      window.clearTimeout(timer);
      remaining.current = Math.max(0, lifetime - (Date.now() - started));
    };
  }, [paused, record.revision, record.tone, record.duration]);
  const close = () => {
    removeToast(record.id);
    record.onDismiss?.(record);
  };
  const icon = record.icon ?? TONE_ICONS[record.tone];
  return (
    <li
      role={record.tone === "error" ? "alert" : "status"}
      aria-atomic="true"
      data-slot="toast"
      data-tone={record.tone}
      className={cx("vk-toast", record.className)}
    >
      {icon ? <span className="vk-toast-icon" aria-hidden="true">{icon}</span> : null}
      <div className="vk-toast-body">
        <div className="vk-toast-title">{record.title}</div>
        {record.description ? <div className="vk-toast-description">{record.description}</div> : null}
      </div>
      {record.action ? (
        <button
          type="button"
          className="vk-btn vk-toast-action"
          data-variant="secondary"
          data-size="sm"
          onClick={(event) => {
            record.action?.onClick(event);
            if (!event.defaultPrevented) close();
          }}
        >
          {record.action.label}
        </button>
      ) : null}
      <button type="button" className="vk-toast-close" aria-label={closeLabel} onClick={close}>
        <XIcon aria-hidden="true" />
      </button>
    </li>
  );
}

// Toaster renders the stack; mount it once near the application root.
// Toaster 渲染提示堆叠；在应用根附近挂载一次。
function Toaster({ className, containerAriaLabel }: { className?: string; containerAriaLabel?: string }) {
  const text = useUiText();
  const records = React.useSyncExternalStore(subscribeToasts, getToasts, getToasts);
  const [hovered, setHovered] = React.useState(false);
  const [focused, setFocused] = React.useState(false);
  const [hidden, setHidden] = React.useState(() => typeof document !== "undefined" && document.hidden);
  React.useEffect(() => {
    const onVisibility = () => setHidden(document.hidden);
    document.addEventListener("visibilitychange", onVisibility);
    return () => document.removeEventListener("visibilitychange", onVisibility);
  }, []);
  const paused = hovered || focused || hidden;
  return (
    <LayerPortal className="vk-layer-toast">
      <section aria-label={containerAriaLabel ?? text.notifications} aria-live="polite" data-slot="toaster" className={cx("vk-toasts", className)}>
        <ol
          onPointerEnter={() => setHovered(true)}
          onPointerLeave={() => setHovered(false)}
          onFocus={() => setFocused(true)}
          onBlur={(event) => {
            if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setFocused(false);
          }}
        >
          {records.map((record) => <ToastItem key={record.id} record={record} paused={paused} closeLabel={text.close} />)}
        </ol>
      </section>
    </LayerPortal>
  );
}

export { Toaster };
