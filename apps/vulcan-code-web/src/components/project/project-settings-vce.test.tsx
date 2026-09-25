// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { readLocaleMessage } from "@/i18n";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { VceJobSummary, VceProjectOverviewResponse, WorkbenchSnapshot } from "@/types";
import { ProjectSettingsDialog } from "./project-settings-dialog";
import type { ProjectSettingsDialogTarget } from "./project-settings-types";
import { VceJobObserverProvider } from "@/components/features/code-context/vce-job-observer-context";
import { createVceActivityFixture } from "@/components/features/code-context/vce-activity-test-fixture";
import {
  VCE_PROJECT_OVERVIEW_READ_DEADLINE_MS,
  VCE_PROJECT_OVERVIEW_READ_TIMEOUT_MESSAGE_KEY,
} from "@/components/features/code-context/vce-observer-timeouts";

// Mock transport and unrelated settings sections; retain the real dialog, semantic editor and observers.
// 模拟传输及无关设置分区；保留真实弹窗、语义编辑器和观察器。
const rpc = vi.hoisted(() => ({
  read: vi.fn(), jobs: vi.fn(), submit: vi.fn(), update: vi.fn(), overview: vi.fn(), cancel: vi.fn(),
  subscribe: vi.fn(), subscribeJobs: vi.fn(), engine: vi.fn(), close: vi.fn(),
  files: vi.fn(), operations: vi.fn(), issues: vi.fn(),
  loadVmm: vi.fn(), loadSkills: vi.fn(), toastSuccess: vi.fn(), toastError: vi.fn(),
}));
vi.mock("@/service/vce-service", () => ({
  loadVceProjectActivity: rpc.read, loadVceJob: rpc.jobs, submitVceJob: rpc.submit,
  updateVceProjectSettings: rpc.update, loadVceProjectOverview: rpc.overview,
  listVceFiles: rpc.files, listVceOperations: rpc.operations, listVceIntegrityIssues: rpc.issues,
  cancelVceJob: rpc.cancel,
  subscribeVceProjectChanged: rpc.subscribe, subscribeVceJobChanged: rpc.subscribeJobs,
}));
vi.mock("./project-settings-sections", () => ({
  ProjectAgentSkillsSection: () => null, ProjectAttachmentCleanupSection: () => null,
  ProjectOverviewSection: () => null, ProjectVmmSection: () => null,
}));
vi.mock("./project-knowledge-bases-section", () => ({ ProjectKnowledgeBasesSection: () => null }));
vi.mock("@/i18n", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/i18n")>();
  const t = (key: import("@/i18n").TranslationKey) => actual.readLocaleMessage("zh-CN", key);
  return { ...actual, useI18n: () => ({ locale: "zh-CN", setLocale: () => undefined, t }) };
});
vi.mock("sonner", () => ({ toast: { success: rpc.toastSuccess, error: rpc.toastError } }));

// host owns the isolated React root; Radix mounts the actual dialog in document.body.
// host 持有隔离的 React 根；Radix 将真实弹窗挂载到 document.body。
let host: HTMLDivElement;
// root preserves the dialog lifetime across close, reopen and project navigation.
// root 在关闭、重开及项目导航期间保留弹窗生命周期。
let root: Root;

// Return the exact project navigation target for projectId, without selecting a sidebar project.
// 为 projectId 返回精确项目导航目标，不选择侧栏项目。
function target(projectId: string): ProjectSettingsDialogTarget {
  return { project: { id: projectId, name: projectId, path: `D:/fixtures/${projectId}`, vceEnabled: true }, initialSection: "code-context" };
}

// Render one target through the production dialog; epoch selects the service connection generation.
// 通过生产弹窗渲染一个 target；epoch 选择服务连接代次。
// Returns after React effects settle, without waiting for indexing or invoking a provider.
// React 副作用完成后返回，不等待索引或调用供应商。
async function renderDialog(dialogTarget: ProjectSettingsDialogTarget | null, epoch = 1) {
  // These are the exact workbench fields read by project navigation; unrelated sections are not rendered.
  // 这些是项目导航实际读取的工作台字段；无关分区不渲染。
  const workbench = { conversations: [], agentSkills: { records: [] }, vmmMemory: { projectDraft: { hasOverride: false } } } as unknown as WorkbenchSnapshot;
  await act(async () => root.render(<VceJobObserverProvider serviceEpoch={epoch}>
    <ProjectSettingsDialog target={dialogTarget} workbench={workbench} settingsRevision={{ vce: 4, vmm: 0, agentSkills: 0 }}
      onCancel={rpc.close} onOpenEngineSettings={rpc.engine} onLoadAgentSkillOverrides={rpc.loadSkills}
      onLoadVmmDraft={rpc.loadVmm} onSaveVmmDraft={vi.fn()} onSetAgentSkillState={vi.fn()}
      onPreviewAttachmentCleanup={vi.fn()} onExecuteAttachmentCleanup={vi.fn()} />
  </VceJobObserverProvider>));
}

// Find the sole rendered dialog button with label; duplicates or missing actions fail explicitly.
// 查找唯一具有 label 的已渲染弹窗按钮；重复或缺失操作明确失败。
function button(label: string): HTMLButtonElement {
  // buttons uses the actual dialog portal and exact human text, not private component callbacks.
  // buttons 使用真实弹窗传送节点及精确用户文本，不使用组件私有回调。
  const buttons = [...document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')]
    .filter((candidate) => candidate.textContent?.trim() === label);
  expect(buttons).toHaveLength(1);
  return buttons[0]!;
}

// Create one durable queued receipt for projectId; no test worker completes the job implicitly.
// 为 projectId 创建一个持久排队回执；测试工作器不会隐式完成作业。
function receipt(projectId: string): VceJobSummary {
  return { projectId, jobId: `job-${projectId}`, kind: "initialize", state: "queued", sequence: 1,
    createdAtUnixMs: 1000, updatedAtUnixMs: 1000, sourceCheckpointVersion: null,
    resultCheckpointVersion: null, followupRequested: false, error: null, progress: null };
}

// abortAwareRead models the browser transport rejecting immediately after a caller aborts.
// abortAwareRead 模拟浏览器传输在调用方中止后立即拒绝请求。
// Parameters: signal is the caller-owned abort signal observed by the fixture.
// 参数：signal 是测试替身监听的调用方中止信号。
// Returns: a promise rejected with AbortError on abort and otherwise left pending.
// 返回值：信号中止时以 AbortError 拒绝、否则保持 pending 的承诺。
function abortAwareRead<T>(signal: AbortSignal | undefined): Promise<T> {
  if (!signal) return Promise.reject(new Error("missing abort signal"));
  return new Promise<T>((_resolve, reject) => {
    const onAbort = () => {
      const error = new Error("request aborted");
      error.name = "AbortError";
      reject(error);
    };
    if (signal.aborted) onAbort();
    else signal.addEventListener("abort", onAbort, { once: true });
  });
}

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.resetAllMocks();
  rpc.read.mockImplementation(async (projectId: string) => {
    // value keeps models and project enabled while deliberately omitting a committed index.
    // value 保持模型和项目启用，同时明确没有已提交索引。
    const value = createVceActivityFixture(projectId);
    value.status.lifecycle = "uninitialized";
    value.status.checkpointVersion = null;
    value.status.presentFiles = 0;
    value.status.checkpointBlobs = 0;
    value.queryAvailable = false;
    value.lastSuccessfulUpdateUnixMs = null;
    return value;
  });
  rpc.jobs.mockResolvedValue(null);
  rpc.subscribe.mockResolvedValue(() => undefined);
  rpc.subscribeJobs.mockResolvedValue(() => undefined);
  rpc.files.mockResolvedValue({ items: [] });
  rpc.operations.mockResolvedValue({ items: [] });
  rpc.issues.mockResolvedValue({ items: [] });
  rpc.loadVmm.mockResolvedValue(null);
  rpc.loadSkills.mockResolvedValue({});
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

describe("project settings semantic integration", () => {
  // Entering, refreshing and reopening settings are observations, never authority to build an index.
  // 进入、刷新及重开设置均是观察，不具有创建索引的权限。
  it("opens the requested section and refreshes metadata without starting indexing", async () => {
    await renderDialog(target("project-a"));
    expect(button("创建语义索引").disabled).toBe(false);
    // refresh is selected by its production accessibility label, since it is an icon-only control.
    // refresh 按生产可访问标签选择，因为它是纯图标控件。
    const refresh = document.querySelector<HTMLButtonElement>('[role="dialog"] button[aria-label="刷新状态"]')!;
    expect(refresh).not.toBeNull();
    await act(async () => refresh.click());
    await renderDialog(null);
    await renderDialog(target("project-b"));
    // projectRead is the exact project-b activity read used to inspect transport options.
    // projectRead 是用于检查传输选项的 project-b 精确活动读取。
    const projectRead = rpc.read.mock.calls.find(([projectId]) => projectId === "project-b");
    expect(projectRead?.[0]).toBe("project-b");
    expect(projectRead?.[1]?.signal).toBeInstanceOf(AbortSignal);
    expect(projectRead?.[1]?.signal.aborted).toBe(false);
    expect(button("创建语义索引").disabled).toBe(false);
    expect(rpc.submit).not.toHaveBeenCalled();
    expect(rpc.update).not.toHaveBeenCalled();
    expect(rpc.overview).not.toHaveBeenCalled();
  });

  // A hanging management overview is aborted at its deadline and can succeed after the dialog reopens.
  // 挂起的管理概览会在期限到达时中止，弹窗重开后可以成功读取。
  it("aborts a hanging management overview and succeeds after reopening", async () => {
    vi.useFakeTimers();
    // activity keeps the production dialog admitted while the management overview is tested.
    // activity 让生产弹窗保持准入，以便测试管理概览。
    const activity = createVceActivityFixture("project-a");
    rpc.read.mockResolvedValue(activity);
    // managementOverview is the successful full projection used after reopening.
    // managementOverview 是重开后使用的成功完整投影。
    const managementOverview: VceProjectOverviewResponse = {
      ...activity,
      readIdentity: "space-a:2",
    };
    // overviewSignal captures the exact signal given to the production overview loader.
    // overviewSignal 捕获生产概览加载器收到的精确信号。
    let overviewSignal: AbortSignal | undefined;
    // This mock respects the production signal so deadline behavior is observable.
    // 此 mock 遵循生产 signal，使期限行为可观测。
    rpc.overview.mockImplementationOnce((_projectId: string, options?: { signal?: AbortSignal }) => {
      overviewSignal = options?.signal;
      return abortAwareRead<VceProjectOverviewResponse>(options?.signal);
    });

    try {
      await renderDialog(target("project-a"));
      await act(async () => {
        button("查看索引详情与统计").click();
        await vi.advanceTimersByTimeAsync(0);
      });
      expect(overviewSignal).toBeDefined();
      expect(overviewSignal?.aborted).toBe(false);
      expect(rpc.submit).not.toHaveBeenCalled();

      await act(async () => {
        await vi.advanceTimersByTimeAsync(VCE_PROJECT_OVERVIEW_READ_DEADLINE_MS);
      });
      expect(overviewSignal?.aborted).toBe(true);
      expect(document.querySelector('[role="dialog"]')?.textContent).toContain(readLocaleMessage("zh-CN", VCE_PROJECT_OVERVIEW_READ_TIMEOUT_MESSAGE_KEY));
      expect(rpc.submit).not.toHaveBeenCalled();

      await renderDialog(null);
      rpc.overview.mockResolvedValueOnce(managementOverview);
      await renderDialog(target("project-a"));
      await act(async () => {
        button("查看索引详情与统计").click();
        await vi.advanceTimersByTimeAsync(0);
      });
      expect(rpc.overview).toHaveBeenCalledTimes(2);
      expect(document.querySelector('[role="dialog"]')?.textContent).not.toContain(readLocaleMessage("zh-CN", VCE_PROJECT_OVERVIEW_READ_TIMEOUT_MESSAGE_KEY));
      expect(rpc.submit).not.toHaveBeenCalled();
    } finally {
      await renderDialog(null);
      vi.useRealTimers();
    }
  });

  // Only a real human button event admits initialization and projects the receipt without awaiting completion.
  // 只有真实人为按钮事件准入初始化，并投影回执而不等待完成。
  it("submits exact revision guards and shows the queued receipt immediately", async () => {
    rpc.submit.mockResolvedValue(receipt("project-a"));
    await renderDialog(target("project-a"));
    await act(async () => button("创建语义索引").click());
    expect(rpc.submit).toHaveBeenCalledExactlyOnceWith({ projectId: "project-a", action: "initialize",
      expectedSettingsRevision: 4, expectedCheckpointVersion: null, idempotencyKey: expect.any(String) });
    expect(document.querySelector('[role="dialog"]')!.textContent).toContain("已排队，等待索引资源");
    expect(rpc.toastSuccess).toHaveBeenCalledWith("索引作业已提交，可关闭设置并继续会话。");
    expect(rpc.update).not.toHaveBeenCalled();
  });

  // Late receipts cannot cross project, dialog-open or service-connection ownership boundaries.
  // 迟到回执不能跨越项目、弹窗打开周期或服务连接归属边界。
  it.each(["project", "reopen", "reconnect"] as const)("discards old admission completions after %s", async (change) => {
    // finish deterministically releases the old transport only after the next project is rendered.
    // finish 仅在下一项目渲染后确定性释放旧传输。
    let finish!: (job: VceJobSummary) => void;
    rpc.submit.mockImplementationOnce(() => new Promise<VceJobSummary>((resolve) => { finish = resolve; }));
    await renderDialog(target("project-a"));
    await act(async () => button("创建语义索引").click());
    // nextProject and epoch express the exact changed ownership axis for this case.
    // nextProject 与 epoch 表示本用例准确变化的归属维度。
    const nextProject = change === "project" ? "project-b" : "project-a";
    const epoch = change === "reconnect" ? 2 : 1;
    if (change === "reopen") await renderDialog(null);
    await renderDialog(target(nextProject), epoch);
    await act(async () => finish(receipt("project-a")));
    expect(button("创建语义索引").disabled).toBe(false);
    expect(document.querySelector('[role="dialog"]')!.textContent).not.toContain("已排队，等待索引资源");
    expect(rpc.toastSuccess).not.toHaveBeenCalled();
    rpc.submit.mockResolvedValue(receipt(nextProject));
    await act(async () => button("创建语义索引").click());
    expect(rpc.submit).toHaveBeenLastCalledWith(expect.objectContaining({ projectId: nextProject }));
    expect(document.querySelector('[role="dialog"]')!.textContent).toContain("已排队，等待索引资源");
  });

  // Unready models leave project configuration reachable but route model setup to the fixed engine page.
  // 模型未就绪时项目配置仍可访问，但模型配置导航到固定引擎页面。
  it("keeps configuration accessible and forwards the engine settings navigation", async () => {
    // value deliberately denies model readiness without changing project intent.
    // value 明确拒绝模型就绪，不改变项目意图。
    const value = createVceActivityFixture();
    value.configuration.ready = false;
    rpc.read.mockResolvedValue(value);
    await renderDialog(target("project-a"));
    await act(async () => button("配置语义引擎模型").click());
    expect(rpc.engine).toHaveBeenCalledTimes(1);
    expect(rpc.submit).not.toHaveBeenCalled();
    expect(rpc.update).not.toHaveBeenCalled();
  });
});
