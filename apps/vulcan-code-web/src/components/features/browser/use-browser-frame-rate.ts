import * as React from "react";

// FRAME_WINDOW_MS is the fixed receive-rate window, independent of host frame sequence numbers.
// FRAME_WINDOW_MS 是固定接收帧率窗口，与宿主帧序号无关。
const FRAME_WINDOW_MS = 2000;

// useBrowserFrameRate measures accepted frames for one exact owner/browser/page selection.
// useBrowserFrameRate 测量精确归属、浏览器与页面选择下的已接受帧。
// Identity arguments reset the sample; returns FPS plus a callback for each accepted frame.
// 身份参数变化时重置采样；返回 FPS 及每个已接受帧调用的回调。
export function useBrowserFrameRate(owner: string, browser: string | null, page: string | null) {
  // received retains only local monotonic arrival times within the measurement window.
  // received 仅保留测量窗口内的本地单调到达时间。
  const received = React.useRef<number[]>([]);
  // fps is a measured rolling receive rate and decays to zero when frames stop arriving.
  // fps 是实测滚动接收帧率，停止收帧后降为零。
  const [fps, setFps] = React.useState(0);
  // recordFrame counts only frames that the caller has already accepted for the selected page.
  // recordFrame 仅统计调用方已确认属于所选页面的帧；无参数、无返回值。
  const recordFrame = React.useCallback(() => {
    const now = performance.now();
    received.current = received.current.filter((time) => time > now - FRAME_WINDOW_MS);
    received.current.push(now);
  }, []);
  React.useLayoutEffect(() => {
    received.current = [];
    setFps(0);
    if (!browser || !page) return;
    // timer updates idle periods too, so a disconnected picture never retains a positive FPS.
    // timer 同样更新空闲时段，避免断流后仍保留正数 FPS。
    const timer = window.setInterval(() => {
      const now = performance.now();
      received.current = received.current.filter((time) => time > now - FRAME_WINDOW_MS);
      setFps(received.current.length * 1000 / FRAME_WINDOW_MS);
    }, 500);
    return () => window.clearInterval(timer);
  }, [owner, browser, page]);
  return { fps, recordFrame };
}
