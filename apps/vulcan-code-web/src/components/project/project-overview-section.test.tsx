// @vitest-environment jsdom
// 使用 jsdom 验证概览权威、异步隔离与设置入口。
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import type { KnowledgeProjectBinding, VmmConfigDraftSummary } from "@/types";
import { createVceActivityFixture } from "@/components/features/code-context/vce-activity-test-fixture";
import { ProjectOverviewSection } from "./project-overview-section";

// reads isolate only host IO; the overview and its real callbacks stay mounted.
// reads 仅隔离宿主输入输出；概览与其真实回调保持挂载。
const reads = vi.hoisted(() => ({ bindings: vi.fn(), catalog: vi.fn(), unsubscribe: vi.fn(), subscribe: vi.fn() }));
vi.mock("@/service/knowledge-base-service", () => ({ listProjectKnowledgeBindings: reads.bindings, subscribeKnowledgeProjectBindingsChanged: reads.subscribe }));
vi.mock("@/service/memory-actions-service", () => ({ runVmmMemoryAction: reads.catalog }));
// draft supplies all real serialized settings, including secrets that must never enter overview markup.
// draft 提供全部真实序列化设置，包括绝不可进入概览标记的秘密。
const draft: VmmConfigDraftSummary = {
  hasOverride: false, backendMode: "native", nativeProjectBindingMode: "fixed", externalProjectBindingMode: "fixed",
  nativeFixedProjectId: "1", externalProjectPrefix: "vulcan/space", enabled: true, grpcTarget: "http://localhost:50051",
  grpcApiKey: "secret-never-render", managementBaseUrl: "http://localhost:8080", managementBearerToken: "secret-never-render",
  managementTimeoutMs: "30000", userId: "2", projectId: "1", receiveTimeoutMs: "30000", handshakeTimeoutMs: "10000",
  maxReceiveMessageBytes: "16777216", grpcKeepaliveTimeMs: "30000", grpcKeepaliveTimeoutMs: "10000", grpcKeepalivePermitWithoutCalls: false,
  aiToolsEnabled: true, precheckEnabled: true, memoryWritebackEnabled: false, precheckMode: "inherit", memoryWritebackMode: "disabled",
  implicitMemoryTurns: "8", profileRefreshTurns: "24", profileNodeLimit: "64", injectionTag: "memory", injectionDescription: "description",
};
// OverviewProps is the production input contract used by each scenario.
// OverviewProps 是每个场景使用的生产输入契约。
type OverviewProps = React.ComponentProps<typeof ProjectOverviewSection>;
// initialProps starts with genuinely unavailable settings and another project's runtime.
// initialProps 从确实不可用的设置与另一项目的运行态开始。
const initialProps: OverviewProps = {
  project: { id: "project-a", name: "Project A", path: "D:/workspace/project-a" }, attachmentRoot: "D:/attachments/project-a", conversationCount: 3,
  vmmDraft: null, vmmLoading: false, vmmDirty: false, vmmError: null, vmmRevision: 0,
  memory: { selectedProjectId: "project-other", runtime: { processReady: true, backendState: "ready", scopeReady: true,
    precheckReady: true, postactionReady: true, profileInstructionReady: true, rerankReady: true,
    outboxCount: 0, profileSyncCount: 0, completedTurns: 2, profilePreview: "" } },
  agentOverrides: {}, agentLoaded: false, agentLoading: false, agentError: null, vceActivity: null, vceError: null,
  onOpenSection: () => undefined,
};
// host and root isolate each mounted overview lifecycle.
// host 与 root 隔离每个挂载概览的生命周期。
let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  vi.clearAllMocks(); reads.bindings.mockResolvedValue([]); reads.subscribe.mockReturnValue(reads.unsubscribe);
  reads.catalog.mockResolvedValue({ data: { kind: "native_project_catalog", value: {
    projects: [{ projectId: 7, name: "Project A memory", tag: "project-a", protected: false, ownerVulcanProjectIds: ["project-a"], linkedVulcanProjectIds: [] }],
    bindings: [{ vulcanProjectId: "project-a", ownerProjectId: 7, linkedProjectIds: [9] }],
  } } });
  host = document.createElement("div"); document.body.appendChild(host); root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount()); host.remove(); vi.restoreAllMocks();
  delete (navigator as unknown as { clipboard?: unknown }).clipboard;
  delete (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT;
});
// renderOverview renders the real component with supplied source changes, then settles React effects.
// renderOverview 使用给定来源变化渲染真实组件，然后等待 React 副作用完成。
// Parameters: overrides changes only the requested source. Returns: completion signal.
// 参数：overrides 仅改变请求的来源。返回值：完成信号。
async function renderOverview(overrides: Partial<OverviewProps> = {}, locale: "zh-CN" | "en-US" = "zh-CN") {
  await act(async () => root.render(<I18nProvider initialLocale={locale}><ProjectOverviewSection {...initialProps} {...overrides} /></I18nProvider>));
}
describe("project overview authority and navigation", () => {
  it("does not turn unloaded settings or failed binding reads into inherited or empty results", async () => {
    reads.bindings.mockRejectedValue(new Error("authorization unavailable"));
    await renderOverview({ vmmError: "memory unavailable", agentError: "skills unavailable" });
    expect(host.textContent).toContain("记忆配置尚未取得");
    expect(host.textContent).toContain("技能覆盖尚未取得");
    expect(host.textContent).toContain("authorization unavailable");
    expect(host.textContent).not.toContain("继承全局");
    expect(host.textContent).not.toContain("已授权 0");
    expect(reads.catalog).not.toHaveBeenCalled();
  });
  it("shows verified native binding and indexed availability without leaking another session or credentials", async () => {
    reads.bindings.mockResolvedValue([{ projectId: "project-a", libraryId: "library-a", defaultEnabled: true }]);
    await renderOverview({ vmmDraft: draft, agentLoaded: true, vceActivity: createVceActivityFixture() });
    expect(host.textContent).toContain("Project A memory (#7)");
    expect(host.textContent).toContain("已授权 1 个知识库 · 默认启用 1 个");
    expect(host.textContent).toContain("代码语义索引可检索");
    expect(host.textContent).toContain("此项目暂无当前会话运行快照");
    expect(host.textContent).not.toContain("当前会话: 记忆作用域已就绪");
    expect(host.textContent).not.toContain("secret-never-render");
    expect(reads.catalog).toHaveBeenCalledTimes(1);
    expect(reads.catalog.mock.calls[0][0]).toMatchObject({ action: "native_project_catalog", scope: "global", projectId: null });
  });
  it("rejects late knowledge results after switching projects and never uses mismatched index metadata", async () => {
    // finishOld resolves the old project's read after the new project has already rendered its own zero bindings.
    // finishOld 在新项目已渲染自身零绑定后完成旧项目读取。
    let finishOld!: (rows: KnowledgeProjectBinding[]) => void;
    reads.bindings.mockImplementation((projectId: string) => projectId === "project-a"
      ? new Promise<KnowledgeProjectBinding[]>((resolve) => { finishOld = resolve; }) : Promise.resolve([]));
    await renderOverview();
    await renderOverview({ project: { id: "project-b", name: "Project B", path: "D:/workspace/project-b" }, vceActivity: createVceActivityFixture("project-a") });
    await act(async () => finishOld([{ libraryId: "old", projectId: "project-a", defaultEnabled: true }]));
    expect(host.textContent).toContain("已授权 0 个知识库");
    expect(host.textContent).not.toContain("已授权 1 个知识库");
    expect(host.textContent).toContain("索引状态尚未取得");
    expect(reads.unsubscribe).toHaveBeenCalledTimes(1);
  });
  it("routes every setting action in the same project and retains readable paths after clipboard failure", async () => {
    // open records section navigation; clipboard rejection exercises the user-visible failure path.
    // open 记录分区导航；剪贴板拒绝触发用户可见的失败路径。
    const open = vi.fn();
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: vi.fn().mockRejectedValue(new Error("denied")) } });
    await renderOverview({ onOpenSection: open });
    for (const label of ["记忆设置", "代码语义索引设置", "知识库设置", "技能设置", "查看与清理附件"]) {
      await act(async () => host.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!.click());
    }
    expect(open.mock.calls.map(([section]) => section)).toEqual(["vmm", "code-context", "knowledge-bases", "agent-skills", "attachments"]);
    await act(async () => host.querySelector<HTMLButtonElement>('button[aria-label="复制项目路径"]')!.click());
    expect(host.textContent).toContain("复制失败，请选择路径文本复制");
    expect(host.querySelector("details")!.textContent).toContain("D:/workspace/project-a");
  });
  it("exposes a missing native owner and actual model-route failures instead of claiming readiness", async () => {
    reads.catalog.mockResolvedValue({ data: { kind: "native_project_catalog", value: { projects: [], bindings: [{ vulcanProjectId: "project-a", ownerProjectId: 7, linkedProjectIds: [] }] } } });
    // index keeps a real ready lifecycle but explicitly revokes query admission through the host contract.
    // index 保留真实 ready 生命周期，但通过宿主契约明确撤销查询准入。
    const index = createVceActivityFixture();
    index.queryAvailable = false; index.configuration.ready = false; index.configuration.issues = ["embedding route unavailable"];
    await renderOverview({ vmmDraft: draft, vceActivity: index });
    expect(host.querySelector('[role="alert"]')!.textContent).toContain("记忆绑定目标 #7 不在当前目录中");
    expect(host.querySelector('[role="alert"]')!.textContent).toContain("embedding route unavailable");
    expect(host.textContent).toContain("代码语义索引暂不可检索");
  });
  it("uses the existing locale mechanism for new status and action text", async () => {
    await renderOverview({}, "en-US");
    expect(host.textContent).toContain("Memory settings unavailable");
    expect(host.textContent).toContain("0 authorized libraries");
    expect(host.querySelector('button[aria-label="Code semantic index settings"]')).not.toBeNull();
  });
  it("discards a native catalog that returns after project navigation", async () => {
    // finishOld publishes an old catalog after the next project's own unbound catalog has arrived.
    // finishOld 在下个项目自身的未绑定目录到达后发布旧目录。
    let finishOld!: (value: unknown) => void;
    reads.catalog.mockReturnValueOnce(new Promise((resolve) => { finishOld = resolve; }))
      .mockResolvedValueOnce({ data: { kind: "native_project_catalog", value: { projects: [], bindings: [] } } });
    await renderOverview({ vmmDraft: draft });
    await renderOverview({ project: { id: "project-b", name: "Project B", path: "D:/workspace/project-b" }, vmmDraft: draft });
    await act(async () => finishOld({ data: { kind: "native_project_catalog", value: {
      projects: [{ projectId: 7, name: "Old owner", tag: "old", protected: false, ownerVulcanProjectIds: [], linkedVulcanProjectIds: [] }],
      bindings: [{ vulcanProjectId: "project-b", ownerProjectId: 7, linkedProjectIds: [] }],
    } } }));
    expect(host.textContent).toContain("尚未绑定记忆项目");
    expect(host.textContent).not.toContain("Old owner");
  });
  it("refreshes binding authority after the existing settings revision changes", async () => {
    await renderOverview({ vmmDraft: draft });
    expect(host.textContent).toContain("Project A memory (#7)");
    reads.catalog.mockResolvedValueOnce({ data: { kind: "native_project_catalog", value: { projects: [], bindings: [] } } });
    await renderOverview({ vmmDraft: draft, vmmRevision: 1 });
    expect(host.textContent).toContain("尚未绑定记忆项目");
    expect(reads.catalog).toHaveBeenCalledTimes(2);
  });
});
