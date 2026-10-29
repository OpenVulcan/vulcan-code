// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { PermissionJudgeSettingsSnapshot } from "@/types";

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
  loadPermissionJudgeEdgeModels: vi.fn(),
  savePermissionJudgeSettings: vi.fn(),
  inspectPermissionJudgePackage: vi.fn(),
  registerPermissionJudgeModel: vi.fn(),
  activatePermissionJudgeModel: vi.fn(),
}));
vi.mock("@/service/permission-judge-service", () => service);
// The API route section keeps loading; its behavior has its own test.
// API 路由区块保持加载中；其行为有独立测试。
vi.mock("@/service/settings-actions-service", () => ({
  loadPublicAiSettings: vi.fn(() => new Promise(() => undefined)),
  savePublicAiInferenceRoute: vi.fn(),
  testPublicAiInferenceRoute: vi.fn(),
}));

const { default: SetupGuideDialog } = await import("./setup-guide-dialog");
const { OpenVulcanAccountContext } = await import("@/components/features/account/openvulcan-account-context");
type OpenVulcanAccountController = import("@/components/features/account/openvulcan-account-context").OpenVulcanAccountController;

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

// snapshot builds an unset judge; `activeModelId` decides whether a local model is ready.
// snapshot 构建未设定的判定；`activeModelId` 决定本机模型是否就绪。
function snapshot(activeModelId: string | null, models = activeModelId ? [activeModelId] : []): PermissionJudgeSettingsSnapshot {
  return {
    backend: null, device: null, recommendedDevice: "cpu",
    devices: [{ device: "cpu", unavailableReason: null }],
    llmMayAllowBlocked: false,
    recordDecisions: false,
    approvalTimeoutSeconds: 0,
    maxApprovalTimeoutSeconds: 3600,
    models: models.map((id) => ({ id, name: id, path: `D:/models/${id}`, knownPackageId: null, policyVersion: "1", precision: "fp16", totalBytes: 1, registeredAtMs: 1 })),
    activeModelId, previousModelId: null, edgeModelId: null, edgeEndpointId: null, knownPackages: [], policyOptions: [], activeModelIssue: null,
    cuda: {
      platform: "windows", supported: false, cudaMajor: null, minimumDriverMajor: null, driver: null, driverError: null,
      requiredLibraries: [], onnxruntimeDirectory: null, detectedDirectories: [], lockedDirectory: null,
      lockedDirectoryMissing: [], archives: [], links: [],
    },
    status: { worker: {
      phase: activeModelId ? "ready" : "not_loaded", modelId: activeModelId, device: "cpu", error: null, processId: null,
      verifyMs: null, loadMs: null, warmupMs: null, workingSetBytes: null, privateBytes: null,
      gpuDedicatedBytes: null, gpuSharedBytes: null, gpuUtilizationPercent: null,
      evaluations: 0, recentP50Ms: null, recentP95Ms: null,
      maxTokens: null, maxRequestItems: null, microBatch: null, paddedBatchTokens: null,
    }, cudaInstall: null, modelDownload: null, pollIntervalMs: 2000 },
  };
}

// buttonByKey finds a button whose text is exactly a catalog key; the dialog renders in a portal.
// buttonByKey 查找文本恰为某个目录键的按钮；对话框渲染在 Portal 中。
function buttonByKey(key: string): HTMLButtonElement | undefined {
  return Array.from(document.body.querySelectorAll("button")).find((button) => button.textContent === key);
}

// choose selects one answer through its radio control.
// choose 通过单选控件选择一个答案。
async function choose(value: string) {
  const radio = document.body.querySelector<HTMLButtonElement>(`[data-setup-choice="${value}"] [role="radio"]`)!;
  await act(async () => radio.click());
}

// account builds an OpenVulcan account controller that is signed in or not.
// account 构建已登录或未登录的 OpenVulcan 账户控制器。
function account(connected: boolean): OpenVulcanAccountController {
  return {
    snapshot: { connected, connectionId: connected ? "c1" : null, displayName: connected ? "user" : null, region: null, message: null },
    initialized: true, open: false, busy: false, waiting: false, error: null,
    refresh: vi.fn(), setOpen: vi.fn(), login: vi.fn(), logout: vi.fn(),
  } as unknown as OpenVulcanAccountController;
}

async function mount(initial: PermissionJudgeSettingsSnapshot, onOpenChange = vi.fn(), connected = false) {
  service.loadPermissionJudgeSettings.mockResolvedValue(initial);
  await act(async () => root.render(<OpenVulcanAccountContext.Provider value={account(connected)}>
    <SetupGuideDialog open onOpenChange={onOpenChange} onOpenProviderSettings={() => undefined} />
  </OpenVulcanAccountContext.Provider>));
  return onOpenChange;
}

// Skipping closes the guide without writing anything, so the judge stays unset.
// 跳过会关闭引导且不写入任何内容，判定保持未设定。
it("closes without saving when skipped", async () => {
  const onOpenChange = await mount(snapshot(null));
  await act(async () => buttonByKey("setupGuide.skip")!.click());
  expect(onOpenChange).toHaveBeenCalledWith(false);
  expect(service.savePermissionJudgeSettings).not.toHaveBeenCalled();
});

// Not now leaves the judge unconfigured: it closes the guide and writes nothing.
// 暂不启用让判定保持未配置：关闭引导且不写入任何内容。
it("writes nothing for not now", async () => {
  const onOpenChange = await mount(snapshot(null));
  await choose("off");
  await act(async () => buttonByKey("setupGuide.off.confirm")!.click());
  expect(service.savePermissionJudgeSettings).not.toHaveBeenCalled();
  expect(onOpenChange).toHaveBeenCalledWith(false);
});

// The local choice waits for a model in use; registering one activates it in place.
// 本机选项需等待使用中模型；登记的模型会就地启用。
it("enables the local judge only after a registered model warms up", async () => {
  await mount(snapshot(null));
  expect(buttonByKey("setupGuide.local.confirm")?.disabled).toBe(true);
  service.registerPermissionJudgeModel.mockResolvedValue(snapshot(null, ["model-a"]));
  service.activatePermissionJudgeModel.mockResolvedValue(snapshot("model-a"));
  const path = document.body.querySelector<HTMLInputElement>('input[aria-label="settings.permissionJudge.registerPath"]')!;
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    setter.call(path, "D:/models/model-a");
    path.dispatchEvent(new Event("input", { bubbles: true }));
  });
  service.inspectPermissionJudgePackage.mockResolvedValue({ issue: null, package: {
    modelId: "model-a", knownPackageId: null, defaultName: "model-a", policyVersion: "1", precision: "fp16", totalBytes: 1, registered: false,
  } });
  await act(async () => { path.dispatchEvent(new FocusEvent("focusout", { bubbles: true })); });
  await act(async () => document.body.querySelector<HTMLButtonElement>("[data-judge-register-custom]")!.click());
  await act(async () => buttonByKey("settings.permissionJudge.registerSubmit")!.click());
  expect(service.registerPermissionJudgeModel).toHaveBeenCalledExactlyOnceWith("D:/models/model-a", "", true);
  expect(service.activatePermissionJudgeModel).toHaveBeenCalledExactlyOnceWith("model-a");
  expect(service.savePermissionJudgeSettings).not.toHaveBeenCalled();
  service.savePermissionJudgeSettings.mockResolvedValue({ ...snapshot("model-a"), backend: "local" });
  await act(async () => buttonByKey("setupGuide.local.confirm")!.click());
  expect(service.savePermissionJudgeSettings).toHaveBeenCalledExactlyOnceWith({
    backend: "local", edgeModelId: null, device: "cpu", cudaLibraryDirectory: null, llmMayAllowBlocked: false, recordDecisions: false, approvalTimeoutSeconds: 0,
  });
});

// The API choice cannot be confirmed before a decision route is saved.
// 保存判定路由之前无法确认 API 选项。
it("waits for an API route before enabling the API judge", async () => {
  await mount(snapshot(null));
  await choose("api");
  expect(buttonByKey("setupGuide.api.confirm")?.disabled).toBe(true);
});

// The cloud choice waits for a sign-in and reads no model list until then.
// 云端选项等待登录，在此之前不读取模型清单。
it("waits for a sign-in before enabling the cloud judge", async () => {
  await mount(snapshot(null));
  await choose("cloud");
  expect(service.loadPermissionJudgeEdgeModels).not.toHaveBeenCalled();
  expect(buttonByKey("setupGuide.cloud.confirm")?.disabled).toBe(true);
});

// Signed in, the cloud choice saves the cloud backend with the first model the server enables.
// 已登录时，云端选项以服务端启用的第一个模型保存云端后端。
it("saves the cloud judge with the first enabled official model", async () => {
  service.loadPermissionJudgeEdgeModels.mockResolvedValue({ models: [{ modelId: "shell-mini" }, { modelId: "shell" }] });
  const onOpenChange = await mount(snapshot(null), vi.fn(), true);
  await choose("cloud");
  expect(service.loadPermissionJudgeEdgeModels).toHaveBeenCalledOnce();
  service.savePermissionJudgeSettings.mockResolvedValue({ ...snapshot(null), backend: "cloud", edgeModelId: "shell-mini" });
  await act(async () => buttonByKey("setupGuide.cloud.confirm")!.click());
  expect(service.savePermissionJudgeSettings).toHaveBeenCalledExactlyOnceWith({
    backend: "cloud", edgeModelId: "shell-mini", device: "cpu", cudaLibraryDirectory: null, llmMayAllowBlocked: false, recordDecisions: false, approvalTimeoutSeconds: 0,
  });
  expect(onOpenChange).toHaveBeenCalledWith(false);
});
