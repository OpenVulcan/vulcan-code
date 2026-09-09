// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { VceBlobDetailResponse, VceFileSummary, VceJobSummary, VceMaintenancePlanResponse, VceProjectOverviewResponse, VceProjectSettingsInput, VceProjectSettingsSummary } from "@/types";
import { CodeContextArea } from "./code-context-area";
import { VceJobObserverProvider } from "./vce-job-observer-context";
import { createVceActivityFixture } from "./vce-activity-test-fixture";
import { VCE_MANAGEMENT_PAGE_SIZE } from "./code-context-pagination";

// Replace only IO; the real editor, native controls and job-observer ownership stay under test.
// 仅替换输入输出；真实编辑器、原生控件和作业观察归属仍接受测试。
const rpc = vi.hoisted(() => ({ overview: vi.fn(), files: vi.fn(), operations: vi.fn(), issues: vi.fn(), detail: vi.fn(), plan: vi.fn(), execute: vi.fn(), save: vi.fn(), reload: vi.fn(), jobs: vi.fn(), subscribeJobs: vi.fn() }));
vi.mock("@/service/vce-service", () => ({
  loadVceJob: rpc.jobs, subscribeVceJobChanged: rpc.subscribeJobs,
  cancelVceJob: async () => { throw new Error("unexpected job cancellation"); },
  loadVceProjectOverview: rpc.overview, listVceFiles: rpc.files, listVceOperations: rpc.operations,
  listVceIntegrityIssues: rpc.issues, loadVceBlobDetail: rpc.detail,
  planVceMaintenance: rpc.plan, executeVceMaintenance: rpc.execute,
}));
vi.mock("@/i18n", () => ({ useI18n: () => ({ locale: "zh-CN", t: (key: string) => key }) }));

// Each test owns a real DOM mount and disposes its observer timers on unmount.
// 每项测试持有真实文档挂载，并在卸载时释放观察计时器。
let host: HTMLDivElement;
// The root retains editor identity while authoritative metadata changes.
// root 在权威元数据变化时保留编辑器身份。
let root: Root;

// Render overview/error through the production editor; returns after React effects settle.
// 通过生产编辑器渲染 overview/error；React 副作用完成后返回。
async function renderEditor(overview: VceProjectOverviewResponse | null, error: string | null = null, serviceEpoch = 1) {
  await act(async () => root.render(<VceJobObserverProvider serviceEpoch={serviceEpoch}>
    <CodeContextArea projectId="project-a" overview={overview} activityError={error} busy={false}
      onSave={rpc.save} onReloadActivity={rpc.reload} onOverviewChanged={() => undefined}
      onRefresh={async () => { throw new Error("unexpected index admission"); }} onOpenEngineSettings={() => undefined} />
  </VceJobObserverProvider>));
}

// Edit the interval using a native input event; value is user-entered text. Returns after React commits.
// 使用原生输入事件编辑间隔；value 为用户输入文本。React 提交后返回。
async function editInterval(value: string) {
  // input is selected by its existing explicit interval bounds, not DOM order.
  // input 根据既有明确间隔边界选择，不依赖文档顺序。
  const input = host.querySelector<HTMLInputElement>('input[min="60"]')!;
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

// Click an exact visible button label; returns after its event and subsequent microtasks settle.
// 点击精确可见按钮标签；其事件及后续微任务完成后返回。
async function click(label: string) {
  // button identifies the actual rendered user action instead of calling an internal callback.
  // button 标识实际渲染的用户操作，不直接调用内部回调。
  const button = Array.from(host.querySelectorAll("button")).find((candidate) => candidate.textContent?.trim() === label);
  expect(button, label).toBeDefined();
  await act(async () => button!.click());
}

// Clicks an exact action inside the shared AlertDialog portal.
// 点击共享 AlertDialog 门户中的精确操作。
async function clickDialog(label: string) {
  const dialog = document.querySelector<HTMLElement>('[role="alertdialog"]');
  const button = Array.from(dialog?.querySelectorAll("button") ?? []).find((candidate) => candidate.textContent?.trim() === label);
  expect(button, label).toBeDefined();
  await act(async () => button!.click());
}

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.resetAllMocks();
  rpc.jobs.mockResolvedValue(null);
  rpc.subscribeJobs.mockResolvedValue(() => undefined);
  rpc.overview.mockResolvedValue({ ...createVceActivityFixture(), readIdentity: "space-a:2" });
  for (const loader of [rpc.files, rpc.operations, rpc.issues]) loader.mockResolvedValue({ items: [], nextCursor: null, total: 0 });
  rpc.save.mockImplementation(async (settings: VceProjectSettingsInput, revision: number) => ({ ...settings, revision: revision + 1, migratedFromCodebaseMemory: false }));
  rpc.reload.mockResolvedValue(undefined);
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

describe("project semantic settings editor", () => {
  // An expected replacement-read race is rendered as localized status, never as a red activity error.
  // 预期的替换读取竞争显示为本地化状态，绝不显示为红色活动错误。
  it("renders replacement progress as neutral localized status", async () => {
    rpc.jobs.mockResolvedValue({
      projectId: "project-a", jobId: "replacement-job", kind: "rebuild", state: "running",
      sequence: 1, createdAtUnixMs: 1, updatedAtUnixMs: 1, sourceCheckpointVersion: 1,
      resultCheckpointVersion: null, followupRequested: false, error: null,
      progress: { phase: "embedding", operationId: "replacement-job", unit: "batches", completed: 1, total: 2, currentPath: null },
    } satisfies VceJobSummary);
    await renderEditor(createVceActivityFixture(), "VCE explicit replacement is in progress; observe its durable job");

    expect(host.querySelector('[role="status"]')?.textContent).toContain("projectSettings.codeContext.generating");
    expect(host.textContent).not.toContain("VCE explicit replacement is in progress; observe its durable job");
    expect(host.querySelector('[role="alert"]')).toBeNull();
  });

  // A fresh settings mount must rehydrate the server-owned running job even when overview restoration is temporarily blocked.
  // 全新设置页面挂载时，即使概览恢复暂时被阻断，也必须重新载入服务端持有的运行中作业。
  it("rehydrates an active job while activity is unavailable", async () => {
    rpc.jobs.mockResolvedValue({
      projectId: "project-a", jobId: "replacement-job", kind: "rebuild", state: "running",
      sequence: 1, createdAtUnixMs: 1, updatedAtUnixMs: 1, sourceCheckpointVersion: 1,
      resultCheckpointVersion: null, followupRequested: false, error: null,
      progress: { phase: "embedding", operationId: "replacement-job", unit: "batches", completed: 1, total: 2, currentPath: null },
    } satisfies VceJobSummary);
    await renderEditor(null, "VCE explicit replacement is in progress; observe its durable job");

    expect(host.textContent).toContain("正在重建语义索引");
    expect(host.textContent).not.toContain("VCE explicit replacement is in progress; observe its durable job");
    expect(host.querySelector('[role="alert"]')).toBeNull();
  });

  // The host lifecycle must block duplicate admission during the short gap before the receipt observer loads.
  // 在回执观察器加载完成前的短暂间隙内，宿主生命周期也必须阻止重复准入。
  it("does not offer a duplicate admission while the host reports indexing", async () => {
    const activity = createVceActivityFixture();
    activity.status.lifecycle = "indexing";
    rpc.jobs.mockResolvedValue(null);
    await renderEditor(activity);

    const button = Array.from(host.querySelectorAll("button")).find((candidate) => candidate.textContent?.trim() === "索引任务进行中");
    expect(button).toBeDefined();
    expect((button as HTMLButtonElement).disabled).toBe(true);
    expect(host.querySelector('[role="status"]')?.textContent).toContain("projectSettings.codeContext.generating");
  });

  // Replacing a physical namespace may reuse checkpoint one; durable job transitions must retire old content.
  // 替换物理命名空间可能复用检查点一；持久作业状态变化必须使旧内容退役。
  // state selects the durable terminal outcome; returns after reloading host identity without altering the settings draft.
  // state 选择持久终态；重新读取宿主身份且不改变设置草稿后返回。
  it.each(["succeeded", "failed"] as const)("reloads management after a same-number replacement ending in %s without treating progress as a content change", async (state) => {
    // Both namespaces have the same visible settings and checkpoint, but different host-issued read identities.
    // 两个命名空间具有相同可见设置和检查点，但宿主签发读取身份不同。
    const activity = createVceActivityFixture();
    activity.status.checkpointVersion = 1;
    rpc.overview.mockResolvedValue({ ...activity, readIdentity: "old-space:1" });
    // The existing preview belongs to the old publication and must disappear after replacement.
    // 既有预览属于旧发布，替换后必须消失。
    rpc.plan.mockResolvedValue({ mode: "repair", affectedBlobCount: 1, embeddingDimension: 1024,
      embeddingFingerprint: "old-space", actions: [{ actionId: "old-plan", description: "old publication preview", blobNames: ["old-blob"], destructive: true }] });
    await renderEditor(activity);
    await click("查看索引详情与统计");
    await editInterval("600");
    await click("生成修复计划");
    expect(host.textContent).toContain("old publication preview");
    // The worker is already running; the event shape is the existing durable service contract.
    // 工作器已经运行；事件结构使用既有持久服务契约。
    const running: VceJobSummary = { projectId: "project-a", jobId: "replace-space", kind: "rebuild", state: "running",
      sequence: 2, createdAtUnixMs: 2000, updatedAtUnixMs: 2000, sourceCheckpointVersion: 1,
      resultCheckpointVersion: null, followupRequested: false, error: null,
      progress: { phase: "embedding", operationId: "replace-space", unit: "batches", completed: 1, total: 3, currentPath: null } };
    await act(async () => rpc.subscribeJobs.mock.calls[0][0](running));
    // Only numeric progress changes here; an expanded panel still must not scan on every event.
    // 此处仅数值进度变化；即使面板展开，也不得在每次事件时扫描。
    const readsBeforeProgress = rpc.overview.mock.calls.length;
    await act(async () => rpc.subscribeJobs.mock.calls[0][0]({ ...running, sequence: 3,
      progress: { ...running.progress!, completed: 2 } }));
    expect(rpc.overview).toHaveBeenCalledTimes(readsBeforeProgress);
    // Install a fresh plan after the running transition so completion itself must invalidate it.
    // 在运行态变化后建立新预览，要求完成事件本身使其失效。
    await click("生成修复计划");
    rpc.overview.mockResolvedValue({ ...activity, readIdentity: "new-space:1" });
    // Publication can succeed before a later runtime handoff fails; either terminal outcome requires a fresh read.
    // 发布可以成功而后续运行时交接失败；任一终态均要求重新读取。
    await act(async () => rpc.subscribeJobs.mock.calls[0][0]({ ...running, sequence: 4,
      state, resultCheckpointVersion: state === "succeeded" ? 1 : null, progress: null,
      error: state === "failed" ? "runtime handoff failed after publication" : null }));
    expect(rpc.overview).toHaveBeenCalledTimes(readsBeforeProgress + 1);
    expect(rpc.files).toHaveBeenLastCalledWith("project-a", "new-space:1", null, VCE_MANAGEMENT_PAGE_SIZE);
    expect(rpc.issues).toHaveBeenLastCalledWith("project-a", "new-space:1", null, VCE_MANAGEMENT_PAGE_SIZE);
    expect(host.textContent).not.toContain("old publication preview");
    expect(host.textContent).not.toContain("确认并执行");
    expect(host.querySelector<HTMLInputElement>('input[min="60"]')!.value).toBe("600");
    expect(rpc.save).not.toHaveBeenCalled();
    await click("收起索引详情");
    // Hidden management must remain read-free even when another durable job arrives.
    // 即使另一持久作业到达，隐藏的管理区也必须保持不读取。
    const readsAfterCollapse = rpc.overview.mock.calls.length;
    await act(async () => rpc.subscribeJobs.mock.calls[0][0]({ ...running, jobId: "next-job", createdAtUnixMs: 3000 }));
    expect(rpc.overview).toHaveBeenCalledTimes(readsAfterCollapse);
  });

  // Retain unfinished lines while typing; normalize rules only in the explicit save request.
  // 输入时保留未完成行；仅在明确保存请求中规范化规则。
  // No parameters; verifies real textarea events, metadata refresh and the acknowledged draft.
  // 无参数；验证真实文本域事件、元数据刷新及已确认草稿。
  it("preserves newlines while editing ignore patterns and normalizes only on save", async () => {
    await renderEditor(createVceActivityFixture());
    // textarea is the editor's sole multiline ignore-rule input.
    // textarea 是编辑器唯一的多行忽略规则输入框。
    const textarea = host.querySelector("textarea")!;
    for (const value of ["target/", "target/\n", "target/\n  generated/**  \n\n"]) {
      await act(async () => {
        Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!.call(textarea, value);
        textarea.dispatchEvent(new Event("input", { bubbles: true }));
      });
      expect(textarea.value).toBe(value);
    }
    // A checkpoint change has no authority to remove the user's unfinished multiline draft.
    // 检查点变化无权移除用户尚未完成的多行草稿。
    const updated = createVceActivityFixture();
    updated.status.checkpointVersion = 3;
    await renderEditor(updated);
    expect(textarea.value).toBe("target/\n  generated/**  \n\n");
    expect(rpc.save).not.toHaveBeenCalled();
    await click("保存设置");
    expect(rpc.save).toHaveBeenCalledWith(expect.objectContaining({ extraIgnorePatterns: ["target/", "generated/**"] }), 4);
    expect(textarea.value).toBe("target/\ngenerated/**");
  });

  // A retired service cannot overwrite a preserved draft with its delayed save acknowledgement.
  // 已退役服务不能用迟到保存确认覆盖保留的草稿。
  it("rejects an old service save acknowledgement after reconnecting the same project", async () => {
    // Resolve the first save after the same editor has adopted another service lifetime.
    // 在同一编辑器采用另一服务生命周期后才完成首次保存。
    let resolveSave!: (settings: VceProjectSettingsSummary) => void;
    rpc.save.mockReturnValueOnce(new Promise<VceProjectSettingsSummary>((resolve) => { resolveSave = resolve; }));
    await renderEditor(createVceActivityFixture());
    await editInterval("600");
    await click("保存间隔");
    await renderEditor(createVceActivityFixture(), null, 2);
    await editInterval("900");
    await act(async () => resolveSave({ ...createVceActivityFixture().settings, revision: 5, autoUpdateMinIntervalSeconds: 600 }));
    expect(host.querySelector<HTMLInputElement>('input[min="60"]')!.value).toBe("900");
    await click("保存间隔");
    expect(rpc.save).toHaveBeenLastCalledWith(expect.objectContaining({ autoUpdateMinIntervalSeconds: 900 }), 4);
  });

  // Plan responses must share management invalidation and cannot revive an obsolete confirmation action.
  // 计划响应必须共享管理失效边界，不能重新显示过期确认操作。
  it.each(["checkpoint", "collapse", "reconnect"] as const)("retires a delayed maintenance plan after %s", async (transition) => {
    // This deferred real service shape is the only IO replacement for the plan control.
    // 该延迟真实服务结构是计划控件唯一替换的输入输出。
    let resolvePlan!: (plan: VceMaintenancePlanResponse) => void;
    rpc.plan.mockReturnValueOnce(new Promise<VceMaintenancePlanResponse>((resolve) => { resolvePlan = resolve; }));
    await renderEditor(createVceActivityFixture());
    await click("查看索引详情与统计");
    await click("生成修复计划");
    if (transition === "checkpoint") {
      // Replace the publication while the preview is still awaiting the old host read.
      // 预览仍等待旧宿主读取时替换发布。
      const updated = createVceActivityFixture(); updated.status.checkpointVersion = 3;
      rpc.overview.mockResolvedValue({ ...updated, readIdentity: "space-a:3" });
      await renderEditor(updated);
    } else if (transition === "collapse") {
      await click("收起索引详情");
    } else {
      await renderEditor(createVceActivityFixture(), null, 2);
    }
    await act(async () => resolvePlan({
      mode: "repair", affectedBlobCount: 1,
      embeddingDimension: 1024, embeddingFingerprint: "space-a",
      actions: [{ actionId: "old-plan", description: "retired plan must not reappear", blobNames: ["old-blob"], destructive: true }],
    }));
    if (transition === "collapse") await click("查看索引详情与统计");
    expect(host.textContent).not.toContain("retired plan must not reappear");
    expect(host.textContent).not.toContain("确认并执行");
  });

  // An uninitialized failed publication can request a rebuild plan without opening management details.
  // 尚未发布的失败索引可以直接请求重建计划，不需要展开管理详情。
  it("plans recovery from the top-level failed-index action", async () => {
    const overview = createVceActivityFixture();
    overview.status.checkpointVersion = null;
    overview.status.lifecycle = "rebuild_required";
    overview.status.lastError = "initial publication failed";
    rpc.plan.mockResolvedValue({ mode: "rebuild", affectedBlobCount: 0, embeddingDimension: 1024,
      embeddingFingerprint: "current-model", actions: [{ actionId: "rebuild-action", description: "replace incomplete namespace", blobNames: [], destructive: true }] });
    rpc.execute.mockResolvedValue({ kind: "job", job: { projectId: "project-a", jobId: "rebuild-job", kind: "initialize", state: "queued",
      sequence: 1, createdAtUnixMs: 1_000, updatedAtUnixMs: 1_000, sourceCheckpointVersion: null,
      resultCheckpointVersion: null, progress: null, followupRequested: false, error: null } });
    await renderEditor(overview);
    await click("生成重建计划");
    expect(rpc.plan).toHaveBeenCalledExactlyOnceWith("project-a", "rebuild");
    expect(host.textContent).toContain("replace incomplete namespace");
    expect(host.textContent).toContain("确认并执行");
    expect(host.textContent).not.toContain("文件与 Blob");
    await click("确认并执行");
    expect(rpc.execute).not.toHaveBeenCalled();
    expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("目标向量维度");
    await clickDialog("取消");
    expect(document.querySelector('[role="alertdialog"]')).toBeNull();
    await click("确认并执行");
    await clickDialog("确认并执行");
    expect(rpc.execute).toHaveBeenCalledExactlyOnceWith("project-a", "rebuild", 4, null, ["rebuild-action"], expect.any(String), "current-model");
  });

  // A failed confirmation keeps the exact recovery entry visible and exposes the failure in the dialog.
  // 确认执行失败后保留精确恢复入口，并在弹窗内显示失败原因。
  it("keeps the rebuild confirmation recoverable after execution failure", async () => {
    const overview = createVceActivityFixture();
    overview.status.checkpointVersion = null;
    overview.status.lifecycle = "rebuild_required";
    rpc.plan.mockResolvedValue({ mode: "rebuild", affectedBlobCount: 0, embeddingDimension: 1024,
      embeddingFingerprint: "current-model", actions: [{ actionId: "rebuild-action", description: "replace incomplete namespace", blobNames: [], destructive: true }] });
    rpc.execute.mockRejectedValueOnce(new Error("current model unavailable"));
    await renderEditor(overview);
    await click("生成重建计划");
    await click("确认并执行");
    await clickDialog("确认并执行");
    expect(rpc.execute).toHaveBeenCalledTimes(1);
    expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("current model unavailable");
    expect(host.textContent).toContain("replace incomplete namespace");
    await clickDialog("取消");
    await click("重新生成重建计划");
    expect(rpc.plan).toHaveBeenCalledTimes(2);
  });

  // A live durable job keeps recovery planning disabled until its exact cancellation or terminal receipt arrives.
  // 活跃持久作业在收到精确取消或终态回执前必须禁用恢复计划。
  it("does not start a rebuild while an indexing job is active", async () => {
    const overview = createVceActivityFixture();
    overview.status.checkpointVersion = null;
    overview.status.lifecycle = "rebuild_required";
    rpc.jobs.mockResolvedValue({ projectId: "project-a", jobId: "active-initialize", kind: "initialize", state: "running",
      sequence: 2, createdAtUnixMs: 2_000, updatedAtUnixMs: 2_000, sourceCheckpointVersion: null,
      resultCheckpointVersion: null, progress: { phase: "embedding", operationId: "active-initialize", unit: "batches", completed: 0, total: 1, currentPath: null },
      followupRequested: false, error: null });
    await renderEditor(overview);
    const rebuildButton = Array.from(host.querySelectorAll("button")).find((button) => button.textContent?.trim() === "生成重建计划");
    expect(rebuildButton).toBeDefined();
    expect(rebuildButton?.hasAttribute("disabled")).toBe(true);
    expect(host.textContent).toContain("请先取消遗留任务");
    expect(rpc.plan).not.toHaveBeenCalled();
    const cancelReceipt = { projectId: "project-a", jobId: "active-initialize", kind: "initialize" as const, state: "cancelled" as const,
      sequence: 3, createdAtUnixMs: 2_000, updatedAtUnixMs: 3_000, sourceCheckpointVersion: null,
      resultCheckpointVersion: null, progress: null, followupRequested: false, error: "cancelled by user" };
    await act(async () => rpc.subscribeJobs.mock.calls[0][0](cancelReceipt));
    const recoveredButton = Array.from(host.querySelectorAll("button")).find((button) => button.textContent?.trim() === "生成重建计划");
    expect(recoveredButton?.hasAttribute("disabled")).toBe(false);
  });

  // A terminal initialize/rebuild receipt is recovery evidence while the fresh namespace status catches up.
  // 新命名空间状态尚未追上时，初始化／重建终态回执仍是恢复依据。
  it.each([
    { kind: "initialize" as const, state: "failed" as const },
    { kind: "initialize" as const, state: "cancelled" as const },
    { kind: "initialize" as const, state: "interrupted" as const },
    { kind: "rebuild" as const, state: "failed" as const },
    { kind: "rebuild" as const, state: "cancelled" as const },
    { kind: "rebuild" as const, state: "interrupted" as const },
  ])("keeps recovery available for a terminal $kind/$state job before status recovery", async ({ kind, state }) => {
    const overview = createVceActivityFixture();
    overview.status.checkpointVersion = null;
    overview.status.lifecycle = "uninitialized";
    rpc.jobs.mockResolvedValue({ projectId: "project-a", jobId: `${kind}-${state}`, kind, state,
      sequence: 2, createdAtUnixMs: 2_000, updatedAtUnixMs: 2_000, sourceCheckpointVersion: null,
      resultCheckpointVersion: null, progress: null, followupRequested: false, error: "index did not publish" });
    rpc.plan.mockResolvedValue({ mode: "rebuild", affectedBlobCount: 0, embeddingDimension: 1024,
      embeddingFingerprint: "current-model", actions: [{ actionId: "rebuild-action", description: "recover fresh namespace", blobNames: [], destructive: true }] });
    await renderEditor(overview);
    const rebuildButton = Array.from(host.querySelectorAll("button")).find((button) => button.textContent?.trim() === "生成重建计划");
    expect(rebuildButton).toBeDefined();
    expect(rebuildButton?.hasAttribute("disabled")).toBe(false);
    expect(host.textContent).toContain("现在可以生成并确认重建计划");
    expect(host.textContent).not.toContain("请先取消遗留任务");
    await click("生成重建计划");
    expect(rpc.plan).toHaveBeenCalledExactlyOnceWith("project-a", "rebuild");
  });

  // Missing publication identity cannot be substituted with the visible numeric checkpoint.
  // 不能用可见数字检查点替代缺失的发布身份。
  it("rejects an overview without content read authority", async () => {
    rpc.overview.mockResolvedValue(createVceActivityFixture());
    await renderEditor(createVceActivityFixture());
    await click("查看索引详情与统计");
    expect(host.textContent).toContain("索引尚无可读取的已发布版本");
    expect(rpc.files).not.toHaveBeenCalled();
    expect(rpc.issues).not.toHaveBeenCalled();
  });

  // Every page carries the original physical identity and exposes host rejection without silent retry.
  // 每页携带原物理身份，宿主拒绝直接显示，不静默重试。
  it("keeps page requests on the overview identity until an explicit reload", async () => {
    rpc.files.mockResolvedValueOnce({ items: [], nextCursor: "100", total: 200 });
    await renderEditor(createVceActivityFixture());
    await click("查看索引详情与统计");
    expect(rpc.files).toHaveBeenLastCalledWith("project-a", "space-a:2", null, VCE_MANAGEMENT_PAGE_SIZE);
    expect(rpc.issues).toHaveBeenLastCalledWith("project-a", "space-a:2", null, VCE_MANAGEMENT_PAGE_SIZE);
    rpc.files.mockRejectedValueOnce(new Error("fixture index changed; refresh required"));
    await click("下一页");
    expect(rpc.files).toHaveBeenLastCalledWith("project-a", "space-a:2", "100", VCE_MANAGEMENT_PAGE_SIZE);
    expect(host.textContent).toContain("fixture index changed");
    expect(rpc.overview).toHaveBeenCalledTimes(1);
  });

  // A detail admitted from an old list must not reappear after a new checkpoint reloads the panel.
  // 旧列表准入的详情不能在新检查点重载面板后重新出现。
  it("discards delayed blob detail after the management identity changes", async () => {
    // The sole old file provides a real clickable list entry.
    // 唯一旧文件提供实际可点击的列表项。
    const file: VceFileSummary = { path: "src/old.rs", blobName: "old-blob", committedBlobName: "old-blob",
      status: "present", generation: 2, size: 20, chunkCount: 1, vectorCount: 1, symbolCount: 0,
      pathChannel: { state: "ready" }, symbolChannel: { state: "ready" } };
    rpc.files.mockResolvedValueOnce({ items: [file], nextCursor: null, total: 1 });
    // resolveDetail is controlled by the test to finish only after the publication changes.
    // resolveDetail 由测试控制，只在发布变化后完成。
    let resolveDetail!: (detail: VceBlobDetailResponse) => void;
    rpc.detail.mockReturnValueOnce(new Promise<VceBlobDetailResponse>((resolve) => { resolveDetail = resolve; }));
    await renderEditor(createVceActivityFixture());
    await click("查看索引详情与统计");
    // Select by the exact source path rendered in the actual file-list button.
    // 根据实际文件列表按钮中渲染的精确源码路径选择。
    const fileButton = Array.from(host.querySelectorAll("button")).find((button) => button.textContent?.includes("src/old.rs"));
    expect(fileButton).toBeDefined();
    await act(async () => fileButton!.click());
    expect(rpc.detail).toHaveBeenCalledWith("project-a", "old-blob", "space-a:2");
    // New publication has no files and a different host-issued identity.
    // 新发布没有文件，并具有不同的宿主签发身份。
    const updated = createVceActivityFixture(); updated.status.checkpointVersion = 3;
    rpc.overview.mockResolvedValue({ ...updated, readIdentity: "space-a:3" });
    await renderEditor(updated);
    await act(async () => resolveDetail({ blobName: "old-blob", path: "src/old.rs", generation: 2,
      pathChannel: { state: "ready" }, symbolChannel: { state: "ready" },
      chunks: [{ chunkId: "old-chunk", startLine: 1, endLine: 1, startCharOffset: 0,
        kind: "recursive", preview: "retired preview must not reappear", vectorReady: true }] }));
    expect(host.textContent).not.toContain("retired preview must not reappear");
    expect(host.textContent).not.toContain("src/old.rs");
    expect(rpc.files).toHaveBeenLastCalledWith("project-a", "space-a:3", null, VCE_MANAGEMENT_PAGE_SIZE);
  });

  // Checkpoint changes and metadata polling cannot erase unsaved settings or trigger detail scans.
  // 检查点变化及元数据轮询不能擦除未保存设置或触发详情扫描。
  it("preserves a dirty interval through same-revision checkpoint refreshes", async () => {
    await renderEditor(createVceActivityFixture());
    await editInterval("600");
    // updated changes only indexed content, not the saved project policy.
    // updated 仅改变索引内容，不改变保存的项目策略。
    const updated = createVceActivityFixture(); updated.status.checkpointVersion = 3;
    await renderEditor(updated);
    expect(host.querySelector<HTMLInputElement>('input[min="60"]')!.value).toBe("600");
    expect(rpc.overview).not.toHaveBeenCalled(); expect(rpc.files).not.toHaveBeenCalled();
    expect(rpc.operations).not.toHaveBeenCalled(); expect(rpc.issues).not.toHaveBeenCalled();
    await click("保存间隔");
    expect(rpc.save).toHaveBeenCalledWith(expect.objectContaining({ autoUpdateMinIntervalSeconds: 600 }), 4);
    // A delayed older metadata projection cannot undo the acknowledged saved settings either.
    // 延迟的旧元数据投影同样不能撤销已确认保存的设置。
    await renderEditor(createVceActivityFixture());
    expect(host.querySelector<HTMLInputElement>('input[min="60"]')!.value).toBe("600");
  });

  // Remote edits retain the local draft and its old CAS revision until an explicit discard.
  // 远端编辑保留本地草稿及旧比较交换修订，直到明确放弃。
  it("does not silently rebase a dirty draft onto another window's settings", async () => {
    await renderEditor(createVceActivityFixture());
    await editInterval("600");
    // remote represents another window's acknowledged policy change.
    // remote 表示另一窗口已确认的策略变化。
    const remote = createVceActivityFixture("project-a", 5); remote.settings.autoUpdateMinIntervalSeconds = 900;
    rpc.overview.mockResolvedValue(remote);
    rpc.save.mockRejectedValueOnce(new Error("stale settings revision"));
    await renderEditor(remote);
    expect(host.textContent).toContain("未保存修改已保留");
    expect(host.querySelector<HTMLInputElement>('input[min="60"]')!.value).toBe("600");
    await click("保存间隔");
    expect(rpc.save).toHaveBeenCalledWith(expect.objectContaining({ autoUpdateMinIntervalSeconds: 600 }), 4);
    expect(host.querySelector<HTMLInputElement>('input[min="60"]')!.value).toBe("600");
    expect(rpc.reload).toHaveBeenCalledTimes(1);
    expect(rpc.overview).not.toHaveBeenCalled();
    await click("放弃修改并载入当前设置");
    expect(host.querySelector<HTMLInputElement>('input[min="60"]')!.value).toBe("900");
    await editInterval("1000");
    await click("保存间隔");
    expect(rpc.save).toHaveBeenLastCalledWith(expect.objectContaining({ autoUpdateMinIntervalSeconds: 1000 }), 5);
  });

  // Files/history/integrity and aggregate counts are requested only after a deliberate expansion.
  // 文件、历史、完整性及聚合计数仅在明确展开后请求。
  it("loads management details on demand and stops refreshing them after collapse", async () => {
    await renderEditor(createVceActivityFixture());
    expect(rpc.overview).not.toHaveBeenCalled();
    await click("查看索引详情与统计");
    expect(rpc.overview).toHaveBeenCalledTimes(1); expect(rpc.files).toHaveBeenCalledTimes(1);
    expect(rpc.operations).toHaveBeenCalledTimes(1); expect(rpc.issues).toHaveBeenCalledTimes(1);
    await click("收起索引详情");
    // updated must remain a lightweight update while management is collapsed.
    // updated 在管理区折叠时必须保持轻量更新。
    const updated = createVceActivityFixture(); updated.status.checkpointVersion = 3;
    await renderEditor(updated);
    expect(rpc.overview).toHaveBeenCalledTimes(1); expect(rpc.files).toHaveBeenCalledTimes(1);
  });

  // Metadata failure renders a recovery action, not an endless loading spinner or implicit init.
  // 元数据失败渲染恢复操作，不显示无尽加载或隐式初始化。
  it("shows read failures with an explicit lightweight retry", async () => {
    await renderEditor(null, "fixture service unavailable");
    expect(host.querySelector('[role="alert"]')?.textContent).toContain("service unavailable");
    await click("重新读取状态");
    expect(rpc.reload).toHaveBeenCalledTimes(1);
    expect(rpc.overview).not.toHaveBeenCalled();
  });
});
