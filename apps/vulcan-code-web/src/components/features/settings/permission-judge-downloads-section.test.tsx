// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { PermissionJudgeKnownPackage, PermissionJudgeModelDownload } from "@/types";
import { PermissionJudgeDownloadsSection } from "./permission-judge-downloads-section";

// The translator returns keys, so assertions name controls by catalog key instead of wording.
// 翻译函数返回键名，使断言以目录键而非文案定位控件。
const t = (key: string) => key;

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
});

// One family with an unpublished recommended format and a published one.
// 一个家族：推荐格式尚未发布下载，另一格式已发布。
const packages: PermissionJudgeKnownPackage[] = [{
  id: "risk",
  displayName: "Risk",
  variants: [
    { format: "fp16-mixed", devices: ["cuda", "cpu"], downloadBytes: null, recommended: true, installedModelId: null },
    { format: "fp32", devices: ["cpu"], downloadBytes: 2048, recommended: false, installedModelId: null },
  ],
}];

async function render(download: PermissionJudgeModelDownload | null, onDownload = vi.fn()) {
  await act(async () => root.render(<PermissionJudgeDownloadsSection busy={null} download={download}
    knownPackages={packages} t={t} onDownload={onDownload} />));
  return onDownload;
}

function downloadButton(variant: string): HTMLButtonElement {
  const item = host.querySelector(`[data-judge-variant="${variant}"]`);
  const button = item?.querySelector("button");
  if (!(button instanceof HTMLButtonElement)) throw new Error(`no download button for ${variant}`);
  return button;
}

// Only a published format downloads; the recommendation is marked on its own format.
// 只有已发布的格式可以下载；推荐标记位于对应格式上。
it("downloads published formats and marks the recommended one", async () => {
  const onDownload = await render(null);
  const recommended = host.querySelector('[data-judge-variant="risk:fp16-mixed"]');
  expect(recommended?.querySelector("[data-judge-recommended]")).not.toBeNull();
  expect(host.querySelector('[data-judge-variant="risk:fp32"] [data-judge-recommended]')).toBeNull();
  expect(downloadButton("risk:fp16-mixed").disabled).toBe(true);
  expect(downloadButton("risk:fp32").disabled).toBe(false);
  await act(async () => downloadButton("risk:fp32").click());
  expect(onDownload).toHaveBeenCalledWith("risk", "fp32");
});

// A running download shows its progress and blocks starting another.
// 运行中的下载显示进度，并阻止开始另一个下载。
it("shows progress and blocks other downloads while one runs", async () => {
  await render({
    packageId: "risk", format: "fp32", running: true, phase: "downloading",
    completedBytes: 1024, totalBytes: 2048, modelId: null, error: null,
  });
  expect(host.querySelector("[data-judge-download-progress]")?.textContent)
    .toContain("settings.permissionJudge.downloads.running");
  expect(downloadButton("risk:fp32").disabled).toBe(true);
});
