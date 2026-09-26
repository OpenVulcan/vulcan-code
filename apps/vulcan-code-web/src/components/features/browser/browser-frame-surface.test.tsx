// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { BrowserFrameImage, BrowserFrameRateCounter, createBrowserFrameSource } from "./browser-frame-surface";

it("isolates picture and meter updates, resets on clear, and survives strict effect remounts", async () => {
  vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "performance"] });
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  const previousCreate = URL.createObjectURL, previousRevoke = URL.revokeObjectURL;
  let number = 0;
  URL.createObjectURL = vi.fn(() => `blob:frame-${++number}`);
  URL.revokeObjectURL = vi.fn();
  const source = createBrowserFrameSource();
  const host = document.createElement("div"), root = createRoot(host);
  const frame = { sequence: "1", pageId: "a", pageGeneration: "1", viewportGeneration: "1", width: 800, height: 600,
    capturedAt: "now", mimeType: "image/png", image: new Uint8Array([1]) };
  let parentRenders = 0;
  function Surface({ page }: { page: string }) {
    parentRenders++;
    return <><BrowserFrameImage source={source} /><BrowserFrameRateCounter source={source} owner="owner" browser="browser" page={page} title="rate" /></>;
  }
  try {
    await act(async () => root.render(<React.StrictMode><Surface page="a" /></React.StrictMode>));
    const initial = parentRenders;
    await act(async () => {
      for (let index = 0; index < 20; index++) { source.publish(frame); vi.advanceTimersByTime(50); }
    });
    expect(parentRenders).toBe(initial);
    expect(host.textContent).toBe("10.0 FPS");
    expect(host.querySelector("img")?.getAttribute("src")).toBe("blob:frame-20");
    await act(async () => source.clear());
    expect(host.querySelector("img")?.hasAttribute("src")).toBe(false);
    await act(async () => vi.advanceTimersByTime(2000));
    expect(host.textContent).toBe("0.0 FPS");
    await act(async () => source.publish(frame));
    await act(async () => root.render(<React.StrictMode><Surface page="b" /></React.StrictMode>));
    expect(host.textContent).toBe("0.0 FPS");
    await act(async () => source.clear());
    expect(URL.revokeObjectURL).toHaveBeenCalledTimes(21);
  } finally {
    await act(async () => root.unmount()); source.clear();
    URL.createObjectURL = previousCreate; URL.revokeObjectURL = previousRevoke;
    vi.useRealTimers(); vi.unstubAllGlobals();
  }
});
