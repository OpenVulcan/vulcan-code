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
  downloadPermissionJudgeModel: vi.fn(),
  loadProjectPermissionJudgePolicy: vi.fn(),
  saveProjectPermissionJudgePolicy: vi.fn(),
  loadPermissionJudgeRecords: vi.fn(),
  markPermissionJudgeRecord: vi.fn(),
  exportPermissionJudgeRecords: vi.fn(),
  clearPermissionJudgeRecords: vi.fn(),
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

// snapshot builds a judge configured on the local backend with one active model; CUDA is reported
// unsupported.
// snapshot 构建已配置为本地后端且有一个使用中模型的判定；CUDA 报告为不支持。
function snapshot(phase: PermissionJudgeWorkerStatus["phase"]): PermissionJudgeSettingsSnapshot {
  return {
    backend: "local", device: "webgpu", recommendedDevice: "webgpu",
    devices: [{ device: "webgpu", unavailableReason: null }, { device: "cpu", unavailableReason: null }],
    llmMayAllowBlocked: false,
    recordDecisions: false,
    approvalTimeoutSeconds: 0,
    maxApprovalTimeoutSeconds: 3600,
    models: [{ id: "model-a", name: "A", path: "D:/models/a", knownPackageId: null, policyVersion: "1", precision: "fp16", totalBytes: 1, registeredAtMs: 1 }],
    activeModelId: "model-a", previousModelId: null, knownPackages: [], policyOptions: [], activeModelIssue: null,
    cuda: {
      platform: "windows", supported: false, cudaMajor: null, minimumDriverMajor: null, driver: null, driverError: null,
      requiredLibraries: [], onnxruntimeDirectory: null, detectedDirectories: [], lockedDirectory: null,
      lockedDirectoryMissing: [], archives: [], links: [],
    },
    status: { worker: worker(phase), cudaInstall: null, modelDownload: null, pollIntervalMs: 2000 },
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

// openView opens one sub-page through its overview entry.
// openView 通过总览入口打开一个子页面。
async function openView(view: string) {
  await act(async () => host.querySelector<HTMLButtonElement>(`[data-judge-open="${view}"]`)!.click());
  expect(host.querySelector(`[data-judge-view="${view}"]`)).not.toBeNull();
}

// switchByKey finds a switch by its catalog-key label.
// switchByKey 按目录键标签查找开关。
function switchByKey(key: string): HTMLButtonElement {
  return host.querySelector<HTMLButtonElement>(`[role="switch"][aria-label="${key}"]`)!;
}

// A choice saves at once, and the request carries every system choice.
// 选择会立即保存，请求携带全部系统选择。
it("saves a changed choice at once with every system choice", async () => {
  await mount("ready");
  service.savePermissionJudgeSettings.mockResolvedValue({ ...snapshot("ready"), llmMayAllowBlocked: true });
  await act(async () => switchByKey("settings.permissionJudge.llmMayAllowBlocked").click());
  expect(service.savePermissionJudgeSettings).toHaveBeenCalledExactlyOnceWith({
    backend: "local", device: "webgpu", cudaLibraryDirectory: null, llmMayAllowBlocked: true, recordDecisions: false, approvalTimeoutSeconds: 0,
  });
});

// Before a backend is chosen, other choices save without configuring the judge; the judge becomes
// configured only through the explicit use action.
// 选择后端之前，其他选择照常保存但不会配置判定器；判定器只经由明确的使用操作成为已配置。
it("configures the judge only through the explicit use action", async () => {
  const unset = { ...snapshot("ready"), backend: null };
  service.loadPermissionJudgeSettings.mockResolvedValue(unset);
  service.loadPermissionJudgeStatus.mockResolvedValue(unset.status);
  await act(async () => root.render(<PermissionJudgeSettingsArea />));
  expect(host.querySelector('[data-judge-switching="setup"]')).not.toBeNull();
  service.savePermissionJudgeSettings.mockResolvedValueOnce({ ...unset, llmMayAllowBlocked: true });
  await act(async () => switchByKey("settings.permissionJudge.llmMayAllowBlocked").click());
  expect(service.savePermissionJudgeSettings).toHaveBeenLastCalledWith({
    backend: null, device: "webgpu", cudaLibraryDirectory: null, llmMayAllowBlocked: true, recordDecisions: false, approvalTimeoutSeconds: 0,
  });
  service.savePermissionJudgeSettings.mockResolvedValueOnce({ ...snapshot("ready"), llmMayAllowBlocked: true });
  await act(async () => buttonByKey("settings.permissionJudge.useBackend")!.click());
  expect(service.savePermissionJudgeSettings).toHaveBeenLastCalledWith({
    backend: "local", device: "webgpu", cudaLibraryDirectory: null, llmMayAllowBlocked: true, recordDecisions: false, approvalTimeoutSeconds: 0,
  });
  expect(host.querySelector("[data-judge-switching]")).toBeNull();
});

// Without an active model the local backend cannot be used yet, so the use action stays disabled.
// 没有使用中模型时本地后端尚不可用，因此使用操作保持禁用。
it("keeps the use action disabled until the backend is set up", async () => {
  const unset = { ...snapshot("ready"), backend: null, activeModelId: null };
  service.loadPermissionJudgeSettings.mockResolvedValue(unset);
  service.loadPermissionJudgeStatus.mockResolvedValue(unset.status);
  await act(async () => root.render(<PermissionJudgeSettingsArea />));
  expect(buttonByKey("settings.permissionJudge.useBackend")?.disabled).toBe(true);
});

// An enabled judge never switches to a backend that is not set up; the switch waits for it.
// 已启用的判定不会切换到尚未设置的后端；切换会等待其设置完成。
it("waits to switch an enabled judge to a backend that is not set up", async () => {
  await mount("ready");
  const cloud = host.querySelector<HTMLButtonElement>('[data-slot="segmented-control-item"][value="cloud"]')!;
  await act(async () => cloud.click());
  expect(service.savePermissionJudgeSettings).not.toHaveBeenCalled();
  expect(host.querySelector("[data-judge-switching]")).not.toBeNull();
  expect(host.querySelector('[aria-label="settings.permissionJudge.device"]')).toBeNull();
});

// Speed tests need a ready worker and run the chosen batch size.
// 速度测试需要就绪的 worker，并按所选批量大小执行。
it("runs speed tests only on a ready model", async () => {
  await mount("warming");
  await openView("diagnostics");
  expect(buttonByKey("settings.permissionJudge.benchmarkBatch")?.disabled).toBe(true);
  await act(async () => root.unmount());
  root = createRoot(host);
  await mount("ready");
  await openView("diagnostics");
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
  expect(host.querySelector('[data-judge-open="models"]')).toBeNull();
  await openView("diagnostics");
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
  await openView("diagnostics");
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

// The categories of the model in use are listed on the overview and open per project.
// 使用中模型的类别列在总览上，并按项目打开。
it("lists the model's categories and opens them for the first project", async () => {
  const withOptions = { ...snapshot("ready"), policyOptions: [{ key: "database_reset", defaultMode: "confirm", modes: ["confirm", "allow"] }] };
  service.loadPermissionJudgeSettings.mockResolvedValue(withOptions);
  service.loadPermissionJudgeStatus.mockResolvedValue(withOptions.status);
  service.loadProjectPermissionJudgePolicy.mockResolvedValue({
    projectId: "p1", options: [{ ...withOptions.policyOptions[0], chosenMode: null }], unsupportedOptions: {}, issue: null,
  });
  const projects = [{ id: "p1", name: "One", path: "D:/one" }, { id: "p2", name: "Two", path: "D:/two" }] as never;
  await act(async () => root.render(<PermissionJudgeSettingsArea projects={projects} />));
  expect(host.querySelector("[data-judge-policy-summary]")?.textContent).toContain("projectSettings.permissionJudge.option.databaseReset");
  await openView("policy");
  expect(service.loadProjectPermissionJudgePolicy).toHaveBeenCalledExactlyOnceWith("p1");
  expect(host.querySelector('[data-judge-policy] [data-judge-option="database_reset"]')).not.toBeNull();
});

// The decision log switch saves at once with every system choice.
// 判定记录开关会立即保存，并携带全部系统选择。
it("turns the decision log on from its page", async () => {
  await mount("ready");
  service.loadPermissionJudgeRecords.mockResolvedValue({
    records: [], hasMore: false, total: 0, marked: 0, workspaces: [], retentionRecords: 5000, retentionDays: 30,
  });
  expect(host.querySelector('[data-records-state="off"]')).not.toBeNull();
  await openView("records");
  service.savePermissionJudgeSettings.mockResolvedValue({ ...snapshot("ready"), recordDecisions: true });
  await act(async () => switchByKey("settings.permissionJudge.records.enabled").click());
  expect(service.savePermissionJudgeSettings).toHaveBeenCalledExactlyOnceWith({
    backend: "local", device: "webgpu", cudaLibraryDirectory: null, llmMayAllowBlocked: false, recordDecisions: true, approvalTimeoutSeconds: 0,
  });
});

// The approval wait saves on commit within its published limit; text outside it returns to the saved value.
// 审批等待时长在发布的上限内提交时保存；超出上限的输入恢复为已保存值。
it("saves the approval wait only within its limit", async () => {
  await mount("ready");
  const input = host.querySelector<HTMLInputElement>("[data-approval-timeout]")!;
  const commit = async (value: string) => {
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await act(async () => { input.focus(); input.blur(); });
  };
  await commit("3601");
  expect(service.savePermissionJudgeSettings).not.toHaveBeenCalled();
  expect(input.value).toBe("0");
  service.savePermissionJudgeSettings.mockResolvedValue({ ...snapshot("ready"), approvalTimeoutSeconds: 90 });
  await commit("90");
  expect(service.savePermissionJudgeSettings).toHaveBeenCalledExactlyOnceWith({
    backend: "local", device: "webgpu", cudaLibraryDirectory: null, llmMayAllowBlocked: false, recordDecisions: false, approvalTimeoutSeconds: 90,
  });
  expect(input.value).toBe("90");
});
