// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { PermissionJudgeSettingsSnapshot, PermissionJudgeWorkerStatus } from "@/types";

// The translator returns keys, so assertions name controls by catalog key instead of wording.
// 翻译函数返回键名，使断言以目录键而非文案定位控件。
vi.mock("@/i18n", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/i18n")>();
  const t = (key: string) => key;
  return { ...actual, useI18n: () => ({ locale: "en-US", setLocale: () => undefined, t }) };
});

// Every service call is synthetic; nothing reaches a running service.
// 所有服务调用均为合成；不会访问运行中的服务。
const service = vi.hoisted(() => ({
  loadPermissionJudgeSettings: vi.fn(),
  loadPermissionJudgeStatus: vi.fn(),
  savePermissionJudgeSettings: vi.fn(),
  benchmarkPermissionJudge: vi.fn(),
  reloadPermissionJudge: vi.fn(),
  registerPermissionJudgeModel: vi.fn(),
  activatePermissionJudgeModel: vi.fn(),
  removePermissionJudgeModel: vi.fn(),
  rollbackPermissionJudgeModel: vi.fn(),
  installPermissionJudgeCuda: vi.fn(),
}));
vi.mock("@/service/permission-judge-service", () => service);
vi.mock("@/service/project-actions-service", () => ({ browseProjectDirectories: vi.fn(), openExternalUrl: vi.fn() }));
// The review route section keeps loading; its behavior has its own test.
// 复核路由区块保持加载中；其行为有独立测试。
vi.mock("@/service/settings-actions-service", () => ({
  loadPublicAiSettings: vi.fn(() => new Promise(() => undefined)),
  savePublicAiInferenceRoute: vi.fn(),
  testPublicAiInferenceRoute: vi.fn(),
}));

const { PermissionJudgeSettingsArea } = await import("./permission-judge-settings-area");

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
  vi.clearAllMocks();
});

// worker builds a worker status in the given phase with every metric absent.
// worker 构建指定阶段且所有指标缺省的 worker 状态。
function worker(phase: PermissionJudgeWorkerStatus["phase"]): PermissionJudgeWorkerStatus {
  return {
    phase, modelId: "model-a", device: "webgpu", error: phase === "failed" ? "load failed" : null, processId: null,
    verifyMs: null, loadMs: null, warmupMs: null, workingSetBytes: null, privateBytes: null,
    gpuDedicatedBytes: null, gpuSharedBytes: null, gpuUtilizationPercent: null,
    evaluations: 0, recentP50Ms: null, recentP95Ms: null,
  };
}

// snapshot builds an enabled judge with one active model; CUDA is reported unsupported.
// snapshot 构建已启用且有一个使用中模型的判定；CUDA 报告为不支持。
function snapshot(phase: PermissionJudgeWorkerStatus["phase"]): PermissionJudgeSettingsSnapshot {
  return {
    state: "enabled", backend: "local", device: "webgpu", recommendedDevice: "webgpu",
    devices: [{ device: "webgpu", unavailableReason: null }, { device: "cpu", unavailableReason: null }],
    llmMayAllowBlocked: false,
    models: [{ id: "model-a", name: "A", path: "D:/models/a", knownPackageId: null, policyVersion: "1", precision: "fp16", totalBytes: 1, registeredAtMs: 1 }],
    activeModelId: "model-a", previousModelId: null, knownPackages: [], policyOptions: [], activeModelIssue: null,
    cuda: {
      platform: "windows", supported: false, cudaMajor: null, minimumDriverMajor: null, driver: null, driverError: null,
      requiredLibraries: [], onnxruntimeDirectory: null, detectedDirectories: [], lockedDirectory: null,
      lockedDirectoryMissing: [], archives: [], links: [],
    },
    status: { worker: worker(phase), cudaInstall: null, pollIntervalMs: 2000 },
  };
}

// buttonByKey finds a button whose text is exactly a catalog key.
// buttonByKey 查找文本恰为某个目录键的按钮。
function buttonByKey(key: string): HTMLButtonElement | undefined {
  return Array.from(host.querySelectorAll("button")).find((button) => button.textContent === key);
}

async function mount(phase: PermissionJudgeWorkerStatus["phase"]) {
  service.loadPermissionJudgeSettings.mockResolvedValue(snapshot(phase));
  service.loadPermissionJudgeStatus.mockResolvedValue(snapshot(phase).status);
  await act(async () => root.render(<PermissionJudgeSettingsArea />));
}

// Nothing is saved until a change; the save request carries the whole draft.
// 修改前不保存；保存请求携带完整草稿。
it("saves only a changed draft and sends every system choice", async () => {
  await mount("ready");
  expect(buttonByKey("settings.permissionJudge.save")?.disabled).toBe(true);
  service.savePermissionJudgeSettings.mockResolvedValue({ ...snapshot("ready"), llmMayAllowBlocked: true });
  const llmSwitch = host.querySelectorAll<HTMLButtonElement>('[role="switch"]')[1];
  await act(async () => llmSwitch.click());
  await act(async () => buttonByKey("settings.permissionJudge.save")!.click());
  expect(service.savePermissionJudgeSettings).toHaveBeenCalledExactlyOnceWith({
    state: "enabled", backend: "local", device: "webgpu", cudaLibraryDirectory: null, llmMayAllowBlocked: true,
  });
  expect(buttonByKey("settings.permissionJudge.save")?.disabled).toBe(true);
});

// Speed tests need a ready worker and run the chosen batch size.
// 速度测试需要就绪的 worker，并按所选批量大小执行。
it("runs speed tests only on a ready model", async () => {
  await mount("warming");
  expect(buttonByKey("settings.permissionJudge.benchmarkBatch")?.disabled).toBe(true);
  await act(async () => root.unmount());
  root = createRoot(host);
  await mount("ready");
  service.benchmarkPermissionJudge.mockResolvedValue({
    device: "webgpu", modelId: "model-a", batchSize: 10, rounds: 5, p50Ms: 40, p95Ms: 45, perItemMs: 4, itemsPerSecond: 250,
  });
  await act(async () => buttonByKey("settings.permissionJudge.benchmarkBatch")!.click());
  expect(service.benchmarkPermissionJudge).toHaveBeenCalledExactlyOnceWith(10);
});

// A failed load is retried only by the explicit retry action.
// 加载失败只能由明确的重试操作重新加载。
it("offers a retry only after a failed load", async () => {
  await mount("ready");
  expect(buttonByKey("settings.permissionJudge.reload")).toBeUndefined();
  await act(async () => root.unmount());
  root = createRoot(host);
  await mount("failed");
  expect(service.reloadPermissionJudge).not.toHaveBeenCalled();
  service.reloadPermissionJudge.mockResolvedValue(snapshot("ready").status);
  await act(async () => buttonByKey("settings.permissionJudge.reload")!.click());
  expect(service.reloadPermissionJudge).toHaveBeenCalledOnce();
});

// A saved cloud judge runs speed tests without a local worker and hides the local-model controls.
// 已保存的云端判定无需本机 worker 即可执行速度测试，并隐藏本机模型控件。
it("benchmarks a saved cloud judge without the local model", async () => {
  const cloud = { ...snapshot("not_loaded"), backend: "cloud" as const };
  service.loadPermissionJudgeSettings.mockResolvedValue(cloud);
  service.loadPermissionJudgeStatus.mockResolvedValue(cloud.status);
  await act(async () => root.render(<PermissionJudgeSettingsArea />));
  expect(host.querySelector('[aria-label="settings.permissionJudge.device"]')).toBeNull();
  expect(host.querySelector('[aria-label="settings.permissionJudge.registerPath"]')).toBeNull();
  service.benchmarkPermissionJudge.mockResolvedValue({
    device: null, modelId: "jev", batchSize: 1, rounds: 1, p50Ms: 300, p95Ms: 300, perItemMs: 300, itemsPerSecond: 3.3,
  });
  await act(async () => buttonByKey("settings.permissionJudge.benchmarkSingle")!.click());
  expect(service.benchmarkPermissionJudge).toHaveBeenCalledExactlyOnceWith(1);
});

// A backend that was never saved is the local one, so a ready local model can be tested.
// 从未保存的后端即为本机后端，因此就绪的本机模型可以测速。
it("tests a ready model before any backend is saved", async () => {
  const unset = { ...snapshot("ready"), backend: null };
  service.loadPermissionJudgeSettings.mockResolvedValue(unset);
  service.loadPermissionJudgeStatus.mockResolvedValue(unset.status);
  await act(async () => root.render(<PermissionJudgeSettingsArea />));
  expect(buttonByKey("settings.permissionJudge.benchmarkSingle")?.disabled).toBe(false);
});

// The live status is re-read at the interval the service publishes in it.
// 实时状态按服务在其中发布的间隔重新读取。
it("polls the status at the interval it publishes", async () => {
  vi.useFakeTimers();
  try {
    const initial = snapshot("ready");
    service.loadPermissionJudgeSettings.mockResolvedValue({ ...initial, status: { ...initial.status, pollIntervalMs: 500 } });
    service.loadPermissionJudgeStatus.mockResolvedValue({ ...initial.status, pollIntervalMs: 500 });
    await act(async () => root.render(<PermissionJudgeSettingsArea />));
    expect(service.loadPermissionJudgeStatus).not.toHaveBeenCalled();
    await act(async () => { vi.advanceTimersByTime(1000); });
    expect(service.loadPermissionJudgeStatus).toHaveBeenCalledTimes(2);
  } finally {
    vi.useRealTimers();
  }
});
