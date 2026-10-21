// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as service from "@/service/markdown-memory-service";
import type { MemorySettingsView } from "@/types";
import { retryMemorySettings, useMemorySettings, useMemorySettingsLoadError } from "./memory-mode-store";

// Synthetic settings returned once the service recovers.
// 服务恢复后返回的合成设置。
const settings = { mode: "markdown" } as unknown as MemorySettingsView;

// Probe exposes the store state as data attributes.
// Probe 以数据属性暴露存储状态。
function Probe() {
  const current = useMemorySettings();
  const error = useMemorySettingsLoadError();
  return <div data-mode={current?.mode ?? ""} data-error={error ?? ""} />;
}

let root: Root;
let host: HTMLDivElement;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("memory settings store", () => {
  it("reports a failed read and recovers on retry", async () => {
    const load = vi.spyOn(service, "loadMemorySettings").mockRejectedValueOnce(new Error("service offline"));
    await act(async () => root.render(<Probe />));
    const probe = host.firstElementChild as HTMLElement;
    expect(probe.dataset.error).toBe("service offline");
    expect(probe.dataset.mode).toBe("");

    load.mockResolvedValueOnce(settings);
    await act(async () => retryMemorySettings());
    expect(probe.dataset.error).toBe("");
    expect(probe.dataset.mode).toBe("markdown");
    expect(load).toHaveBeenCalledTimes(2);
  });
});
