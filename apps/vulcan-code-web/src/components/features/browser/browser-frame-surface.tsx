import * as React from "react";
import type { CoreBrowserFrame } from "@/service/core-browser-service";
import { useBrowserFrameRate } from "./use-browser-frame-rate";
import "@/ui-kit/styles/features/browser.css"

// The picture and FPS own high-frequency updates. The surrounding controls only need
// the transition between having no accepted frame and having one.
// 画面及帧率独立持有高频更新；外围控件只需要获知有无有效画面的转换。
export interface BrowserFrameSource {
  subscribe: (listener: () => void) => () => void;
  snapshot: () => string | null;
  publish: (frame: CoreBrowserFrame) => void;
  clear: () => void;
}

export function createBrowserFrameSource(): BrowserFrameSource {
  let url: string | null = null;
  const listeners = new Set<() => void>();
  const replace = (next: string | null) => {
    if (url === next) return;
    const previous = url;
    url = next;
    if (previous !== null) URL.revokeObjectURL(previous);
    for (const listener of [...listeners]) if (listeners.has(listener)) listener();
  };
  return {
    snapshot: () => url,
    subscribe(listener) {
      listeners.add(listener);
      return () => { listeners.delete(listener); };
    },
    publish(frame) {
      replace(URL.createObjectURL(new Blob([frame.image], { type: frame.mimeType })));
    },
    // Clear is reusable across page changes and React strict-effect cleanup/setup cycles.
    // 清空操作可在页面切换及 React 严格模式 Effect 清理、重建周期中复用。
    clear: () => replace(null),
  };
}

const emptySnapshot = () => null;
export function BrowserFrameImage({ source }: { source: BrowserFrameSource }) {
  const imageUrl = React.useSyncExternalStore(source.subscribe, source.snapshot, emptySnapshot);
  return <img className="vk-browser-frame" src={imageUrl ?? undefined} alt="" draggable={false} />;
}

export function BrowserFrameRateCounter(props: {
  source: BrowserFrameSource;
  owner: string;
  browser: string | null;
  page: string | null;
  title: string;
}) {
  const { fps, recordFrame } = useBrowserFrameRate(props.owner, props.browser, props.page);
  React.useLayoutEffect(() => props.source.subscribe(() => {
    if (props.source.snapshot() !== null) recordFrame();
  }), [props.source, recordFrame]);
  return <span className="vk-num" title={props.title}>{fps.toFixed(1)} FPS</span>;
}
