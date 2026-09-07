// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { VceJobSummary, VceProjectActivityResponse } from "@/types";
import { VceProjectSurfaceProvider, useVceProjectSurface, useVceProjectVisibility, vceSurfaceGate } from "./vce-project-surface";
import { VceComposerMenu, VceFloatingNode } from "./vce-project-controls";
import { DropdownMenu, DropdownMenuContent, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { VceJobObserverProvider } from "./vce-job-observer-context";
import { useVceProjectActivity } from "./use-vce-project-activity";
import { ConversationSessionTodolistPanel } from "@/components/conversation/session-todolist-panel";

// Replace only service transport; production ownership hooks and UI projection remain under test.
// 仅替换服务传输；生产归属钩子与界面投影仍接受测试。
const rpc = vi.hoisted(() => ({ read: vi.fn(), jobs: vi.fn(), update: vi.fn(), submit: vi.fn(),
  cancel: vi.fn(), subscribe: vi.fn(), subscribeJobs: vi.fn(), openEngine: vi.fn(), openProject: vi.fn() }));
vi.mock("@/service/vce-service", () => ({ loadVceProjectActivity: rpc.read, loadVceJob: rpc.jobs,
  updateVceProjectSettings: rpc.update, submitVceJob: rpc.submit, cancelVceJob: rpc.cancel,
  subscribeVceProjectChanged: rpc.subscribe, subscribeVceJobChanged: rpc.subscribeJobs }));
vi.mock("@/i18n", () => ({ useI18n: () => ({ locale: "zh-CN", t: (key: string) => key }) }));

// Build exact host activity for one initialized project; no Git metadata is supplied.
// 为一个已初始化项目构建精确宿主活动；不提供 Git 元数据。
// projectId selects ownership. Returns an isolated full wire fixture.
// projectId 选择归属。返回独立完整线协议夹具。
function activity(projectId = "project-a"): VceProjectActivityResponse {
  return { configuration: { ready: true, consumerProfileId: "vce", queryRewritePurposeId: "code_retrieval.query_rewrite",
    reasoningPolicy: "fixed_off", embedding: { configured: true, available: true },
    queryRewrite: { configured: true, available: true }, rerank: { configured: true, available: true }, issues: [] },
    settings: { enabled: true, updateMode: "manual", autoUpdateMinIntervalSeconds: 300, revision: 4,
      migratedFromCodebaseMemory: false, maxFileBytes: 1048576, extraIgnorePatterns: [], allowQueryRewrite: false,
      allowRerank: true, embeddingMaxConcurrency: 4, queryMaxConcurrency: 3, maxResults: 10, maxContextChars: 32000 },
    status: { projectId, enabled: true, settingsRevision: 4, lifecycle: "ready", checkpointVersion: 2,
      presentFiles: 7, checkpointBlobs: 6 }, queryAvailable: true, lastSuccessfulUpdateUnixMs: 1000,
    sourceFreshness: { watching: false, pendingChanges: false, reconciliationRequired: true } };
}

// Keep one real React root and observable production state per isolated test.
// 为每项独立测试保留一个真实 React 根与可观察生产状态。
let root: Root;
let host: HTMLDivElement;
let surface: ReturnType<typeof useVceProjectSurface>;
// Count layout-only renders independently from the progressing leaf node.
// 独立于更新进度的叶节点统计仅布局渲染。
let visibilityRenders = 0;
// settingsActivity models the settings panel's independent subscription to the same project.
// settingsActivity 模拟设置面板对同项目的独立订阅。
let settingsActivity: ReturnType<typeof useVceProjectActivity> | null = null;

// Simulate the chat shell's visibility-only subscription without reading progress.
// 模拟聊天外壳仅可见性订阅，不读取进度。
// Takes no parameters. Returns no DOM and records each geometry consumer render.
// 无参数。不返回文档节点，记录每次布局消费者渲染。
function VisibilityProbe() { useVceProjectVisibility(); visibilityRenders += 1; return null; }

// Observe the production context and render the actual semantic status node.
// 观察生产上下文并渲染真实语义状态节点。
// Takes no parameters. Returns the floating node without any Git or conversation fixture.
// 无参数。不提供 Git 或会话夹具，返回浮动节点。
function Probe() { surface = useVceProjectSurface(); return <VceFloatingNode />; }

// Render the real session panel with no Git, task, terminal or subagent rows; only VCE can make it visible.
// 渲染真实会话浮窗，不提供 Git、任务、终端或子代理行；仅 VCE 能使其可见。
// Takes no parameters. Returns the full collapsed/expanded panel and observes its project context.
// 无参数。返回完整折叠／展开浮窗，并观察其项目上下文。
function SessionPanelProbe() {
  surface = useVceProjectSurface();
  return <ConversationSessionTodolistPanel todolist={null} reviewSummary={null} reviewRepositories={[]}
    commitDialogOpen={false} commitBusyState={null} terminalSessions={[]} subagentNavigation={null}
    expanded={false} onExpandedChange={() => undefined} onCommitDialogOpenChange={() => undefined}
    onOpenReviewChanges={rpc.openProject} onStopCommandSession={async () => undefined}
    onOpenSubagentPanel={async () => false} />;
}

// Observe projectId through the exact hook used by project settings; returns no DOM.
// 通过项目设置实际使用的钩子观察 projectId；不返回文档节点。
function SettingsActivityProbe(props: { projectId: string | null }) {
  settingsActivity = useVceProjectActivity(props.projectId, "4");
  return null;
}

// Mount the provider without changing child identity during project navigation.
// 在项目导航期间不改变子界面身份地挂载提供者。
// projectId and enabled are current workbench authority. Returns a complete test tree.
// projectId 与 enabled 为当前工作台权威。返回完整测试树。
function Harness(props: { projectId: string | null; enabled?: boolean; revisionKey?: string; settingsVisible?: boolean; epoch?: number; menu?: boolean; sessionPanel?: boolean }) {
  return <VceJobObserverProvider serviceEpoch={props.epoch ?? 1}><VceProjectSurfaceProvider projectId={props.projectId} projectEnabled={props.enabled ?? true}
    revisionKey={props.revisionKey ?? "1:1:4"} onOpenEngineSettings={rpc.openEngine} onOpenProjectSettings={rpc.openProject}>
    <VisibilityProbe />{props.sessionPanel ? <SessionPanelProbe /> : <Probe />}
    {props.settingsVisible ? <SettingsActivityProbe projectId={props.projectId} /> : null}
    {props.menu ? <DropdownMenu open modal={false}><DropdownMenuTrigger>＋</DropdownMenuTrigger>
      <DropdownMenuContent><VceComposerMenu /></DropdownMenuContent></DropdownMenu> : null}
  </VceProjectSurfaceProvider></VceJobObserverProvider>;
}

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.resetAllMocks();
  visibilityRenders = 0;
  settingsActivity = null;
  rpc.read.mockImplementation(async (projectId: string) => activity(projectId));
  rpc.jobs.mockResolvedValue(null);
  rpc.subscribe.mockResolvedValue(() => undefined);
  rpc.subscribeJobs.mockResolvedValue(() => undefined);
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

// Find one exact accessible item in the real portal-mounted menu; missing or duplicate labels fail.
// 在真实传送挂载菜单中查找一个精确可访问项；标签缺失或重复即失败。
// label and role select the intended control. Returns its live DOM element.
// label 和 role 选择目标控件。返回其存活文档元素。
function menuItem(label: string, role = "menuitem"): HTMLElement {
  // Matching uses rendered human text and ARIA roles, never private component internals.
  // 匹配使用已渲染用户文本及可访问角色，绝不使用组件私有内部结构。
  const matches = [...document.querySelectorAll<HTMLElement>(`[role="${role}"]`)].filter((item) => item.textContent === label);
  expect(matches).toHaveLength(1);
  return matches[0]!;
}

describe("shared project semantic surfaces", () => {
  // Workbench revocation must close both menus and mutation admission while the metadata read is pending.
  // 元数据读取等待期间，工作台撤销必须同时关闭菜单与修改准入。
  it.each(["1:1:4", "1:1:5"])("revokes ready menu actions immediately when the workbench disables the project at revision %s", async (revisionKey) => {
    await act(async () => root.render(<Harness projectId="project-a" />));
    expect(surface!.gate).toBe("ready");
    // retained belongs to the formerly enabled render and must not submit after authority changes.
    // retained 属于之前已启用的渲染，权威变化后不得提交。
    const retained = surface!.synchronize;
    rpc.read.mockImplementation(() => new Promise(() => undefined));
    await act(async () => root.render(<Harness projectId="project-a" enabled={false} revisionKey={revisionKey} menu />));
    expect(surface!.visible).toBe(false);
    expect(surface!.gate).toBe("project_off");
    await act(async () => menuItem("语义引擎").click());
    expect(menuItem("项目未启用（点击设置）")).not.toBeNull();
    await act(async () => {
      await retained();
      await surface!.synchronize();
      await surface!.saveMode("automatic");
    });
    expect(rpc.submit).not.toHaveBeenCalled();
    expect(rpc.update).not.toHaveBeenCalled();
  });

  // Preserve long diagnostics while explicitly overriding the enclosing menu's non-wrapping layout.
  // 保留长诊断，同时明确覆盖外层菜单不换行的布局。
  it("opts the semantic panel out of inherited nowrap without truncating diagnostics", async () => {
    // A production activity error may contain an unbroken provider or path identifier.
    // 生产活动错误可能含不间断的供应商或路径标识。
    const value = activity();
    value.status.lastError = `Unavailable provider: ${"identifier".repeat(60)}`;
    rpc.read.mockResolvedValue(value);
    host.style.whiteSpace = "nowrap";
    await act(async () => root.render(<Harness projectId="project-a" />));
    // Layout geometry is validated separately in the real browser; this guards the explicit CSS contract.
    // 布局几何另由真实浏览器验证；此处保护明确的样式契约。
    const panel = host.querySelector<HTMLElement>('section[aria-label="项目语义索引"]')!;
    expect(panel).not.toBeNull();
    expect(panel.classList.contains("whitespace-normal")).toBe(true);
    expect(panel.classList.contains("[overflow-wrap:anywhere]")).toBe(true);
    expect(panel.classList.contains("min-w-0")).toBe(true);
    expect(panel.textContent).toContain(value.status.lastError);
  });

  // The outer session panel must not discard a semantic-only project just because Git is absent.
  // 外层会话浮窗不得仅因没有 Git 而丢弃仅包含语义索引的项目。
  it("renders and expands the real non-Git panel, then removes it when the project is disabled", async () => {
    await act(async () => root.render(<Harness projectId="project-a" sessionPanel />));
    // The collapsed summary is one button; the semantic row must not introduce invalid nested buttons.
    // 折叠摘要是单个按钮；语义行不得引入无效嵌套按钮。
    const expand = host.querySelector<HTMLButtonElement>('button[aria-label="sessionTools.expand"]');
    expect(expand).not.toBeNull();
    expect(expand!.textContent).toContain("语义索引");
    expect(host.querySelector("button button")).toBeNull();
    await act(async () => expand!.click());
    expect(host.querySelector('aside[aria-label="sessionTools.menu"]')).not.toBeNull();
    expect(host.querySelector('section[aria-label="项目语义索引"]')).not.toBeNull();
    expect(host.textContent).toContain("上次成功更新");
    expect(rpc.submit).not.toHaveBeenCalled();
    expect(rpc.update).not.toHaveBeenCalled();
    await act(async () => root.render(<Harness projectId="project-a" enabled={false} sessionPanel />));
    expect(host.querySelector(".conversation-session-tool-surface")).toBeNull();
    expect(host.textContent).not.toContain("语义索引");
    await act(async () => root.render(<Harness projectId={null} sessionPanel />));
    expect(host.querySelector(".conversation-session-tool-surface")).toBeNull();
    expect(rpc.submit).not.toHaveBeenCalled();
    expect(rpc.update).not.toHaveBeenCalled();
  });
  // Real nested menu items retain prerequisite priority and invoke the corresponding navigation callback.
  // 真实嵌套菜单项保持前提优先级，并调用对应导航回调。
  it.each(["engine", "project", "index"] as const)("opens the correct settings from the %s prerequisite menu", async (gate) => {
    // Value deliberately leaves lower-priority prerequisites incomplete to prove ordering.
    // value 故意让低优先级前提也不完整，以证明顺序。
    const value = activity();
    value.configuration.ready = gate !== "engine";
    value.settings.enabled = gate === "index";
    value.status.checkpointVersion = null; value.queryAvailable = false;
    rpc.read.mockResolvedValue(value);
    await act(async () => root.render(<Harness projectId="project-a" menu />));
    await act(async () => menuItem("语义引擎").click());
    // Label is the exact human-facing item selected by the production gate.
    // label 是生产门禁选择的精确用户菜单项。
    const label = gate === "engine" ? "引擎未启用（点击设置）"
      : gate === "project" ? "项目未启用（点击设置）" : "索引未初始化（点击设置）";
    await act(async () => menuItem(label).click());
    expect(rpc.openEngine).toHaveBeenCalledTimes(gate === "engine" ? 1 : 0);
    expect(rpc.openProject).toHaveBeenCalledTimes(gate === "engine" ? 0 : 1);
    expect(rpc.update).not.toHaveBeenCalled(); expect(rpc.submit).not.toHaveBeenCalled();
  });

  // Production radio semantics expose the project setting, not a session-local toggle.
  // 生产单选语义暴露项目设置，而非会话内开关。
  it("uses checked radio items to save the project mode without creating an index", async () => {
    await act(async () => root.render(<Harness projectId="project-a" menu />));
    await act(async () => menuItem("语义引擎").click());
    await act(async () => menuItem("索引更新模式").dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true })));
    expect(menuItem("手动更新", "menuitemradio").getAttribute("aria-checked")).toBe("true");
    expect(menuItem("自动更新", "menuitemradio").getAttribute("aria-checked")).toBe("false");
    // Saved authority drives the following read; the menu must not invent local checked state.
    // 已保存权威驱动后续读取；菜单不得虚构本地勾选状态。
    const saved = activity(); saved.settings.updateMode = "automatic"; saved.settings.revision = 5;
    rpc.update.mockResolvedValue(saved.settings); rpc.read.mockResolvedValue(saved);
    await act(async () => menuItem("自动更新", "menuitemradio").click());
    expect(rpc.update).toHaveBeenCalledWith("project-a", 4, expect.objectContaining({ updateMode: "automatic", autoUpdateMinIntervalSeconds: 300 }));
    expect(rpc.submit).not.toHaveBeenCalled();
    await act(async () => menuItem("语义引擎").click());
    await act(async () => menuItem("索引更新模式").click());
    expect(menuItem("自动更新", "menuitemradio").getAttribute("aria-checked")).toBe("true");
  });

  // A recent successful update cannot impose automatic cooldown on the explicit human menu command.
  // 最近成功更新不能让明确的人为菜单命令受到自动冷却约束。
  it("submits manual synchronization immediately and projects its durable receipt", async () => {
    // Activity has a fresh timestamp and an automatic five-minute project policy.
    // 活动具有刚更新的时间戳及自动五分钟项目策略。
    const value = activity(); value.settings.updateMode = "automatic"; value.lastSuccessfulUpdateUnixMs = Date.now();
    rpc.read.mockResolvedValue(value);
    // Receipt is supplied by the host without waiting for any indexing phase to finish.
    // 回执由宿主提供，不等待任何索引阶段完成。
    const receipt: VceJobSummary = { projectId: "project-a", jobId: "manual-sync", kind: "synchronize", state: "queued",
      sequence: 1, createdAtUnixMs: 2000, updatedAtUnixMs: 2000, sourceCheckpointVersion: 2,
      resultCheckpointVersion: null, followupRequested: false, error: null, progress: null };
    rpc.submit.mockResolvedValue(receipt);
    await act(async () => root.render(<Harness projectId="project-a" menu />));
    await act(async () => menuItem("语义引擎").click());
    await act(async () => menuItem("手动更新索引").click());
    expect(rpc.submit).toHaveBeenCalledTimes(1);
    expect(rpc.submit).toHaveBeenCalledWith(expect.objectContaining({ projectId: "project-a", action: "synchronize",
      expectedSettingsRevision: 4, expectedCheckpointVersion: 2 }));
    expect(surface!.jobState.job?.jobId).toBe("manual-sync");
    expect(surface!.gate).toBe("ready");
    expect(surface!.activity!.lastSuccessfulUpdateUnixMs).toBe(value.lastSuccessfulUpdateUnixMs);
    expect(host.textContent).toContain("会话和已有索引查询仍可正常使用");
    expect(rpc.update).not.toHaveBeenCalled();
  });

  // An old surface closure must not acquire a newly mounted project's current action owner.
  // 旧界面闭包不得取得新挂载项目的当前操作归属。
  it("rejects retired menu actions after project navigation, re-entry and unmount", async () => {
    await act(async () => root.render(<Harness projectId="project-a" />));
    // Retired closures capture the old project's activity and must never dispatch against it later.
    // 退役闭包捕获旧项目活动，之后绝不能继续向其派发操作。
    const retired = surface!;
    await act(async () => root.render(<Harness projectId="project-b" />));
    await act(async () => { void retired.saveMode("automatic"); void retired.synchronize(); });
    expect(rpc.update).not.toHaveBeenCalled();
    expect(rpc.submit).not.toHaveBeenCalled();
    await act(async () => root.render(<Harness projectId="project-a" />));
    await act(async () => { void retired.saveMode("automatic"); void retired.synchronize(); });
    expect(rpc.update).not.toHaveBeenCalled();
    expect(rpc.submit).not.toHaveBeenCalled();
    // Last callbacks also lose authority when only the semantic surface unmounts.
    // 即使仅语义界面卸载，最后一组回调也失去权威。
    const unmounted = surface!;
    await act(async () => root.render(null));
    await act(async () => { void unmounted.saveMode("automatic"); void unmounted.synchronize(); });
    expect(rpc.update).not.toHaveBeenCalled();
    expect(rpc.submit).not.toHaveBeenCalled();
  });

  // Service replacement must retire callbacks even when project and persisted revisions stay equal.
  // 即使项目及持久修订相同，服务替换也必须使回调退役。
  it("rejects menu mutations retained from the previous service epoch", async () => {
    await act(async () => root.render(<Harness projectId="project-a" epoch={1} />));
    // Old ownership must not be borrowed from the new context through a shared mutable ref.
    // 旧归属不得通过共享可变引用从新上下文借用。
    const retired = surface!;
    await act(async () => root.render(<Harness projectId="project-a" epoch={2} />));
    await act(async () => { void retired.saveMode("automatic"); void retired.synchronize(); });
    expect(rpc.update).not.toHaveBeenCalled();
    expect(rpc.submit).not.toHaveBeenCalled();
  });

  // Freshness diagnostics follow host evidence without hiding an already queryable checkpoint.
  // 新鲜度诊断跟随宿主证据，不隐藏已可查询的检查点。
  it("shows unknown, pending and acknowledged freshness without changing query admission", async () => {
    await act(async () => root.render(<Harness projectId="project-a" />));
    expect(host.textContent).toContain("新鲜度待确认");
    expect(host.textContent).toContain("上次成功更新");
    expect(surface!.gate).toBe("ready");
    // Pending evidence remains visible even after switching to manual mode and removing the watcher.
    // 即使切换手动模式并移除监听，待同步证据仍可见。
    const pending = activity(); pending.sourceFreshness.pendingChanges = true;
    rpc.read.mockResolvedValue(pending);
    await act(async () => rpc.subscribe.mock.calls[0][0]({ projectId: "project-a", settingsRevision: 4, lifecycle: "ready" }));
    expect(host.textContent).toContain("有待同步修改");
    expect(surface!.gate).toBe("ready");
    // Acknowledged observation is narrower than a promise that the filesystem can never change.
    // 已确认观察比承诺文件系统永不变化更窄。
    const synced = activity(); synced.sourceFreshness = { watching: true, pendingChanges: false, reconciliationRequired: false };
    rpc.read.mockResolvedValue(synced);
    await act(async () => rpc.subscribe.mock.calls[0][0]({ projectId: "project-a", settingsRevision: 4, lifecycle: "ready" }));
    expect(host.textContent).toContain("已同步至最近观察");
    expect(surface!.gate).toBe("ready");
    expect(rpc.submit).not.toHaveBeenCalled();
  });

  // The selected-project surfaces and an independently opened settings observer share exact activity.
  // 所选项目界面和独立打开的设置观察者共享精确活动。
  it("shares activity with project settings without another subscription or initial read", async () => {
    await act(async () => root.render(<Harness projectId="project-a" />));
    // initialReads includes the one-time subscription acknowledgement reconciliation.
    // initialReads 包含一次性订阅确认后的核对。
    const initialReads = rpc.read.mock.calls.length;
    await act(async () => root.render(<Harness projectId="project-a" settingsVisible />));
    expect(rpc.read).toHaveBeenCalledTimes(initialReads);
    expect(rpc.subscribe).toHaveBeenCalledTimes(1);
    expect(settingsActivity!.activity).toBe(surface!.activity);
    // changed updates the project mode exactly once in the shared observer.
    // changed 在共享观察者中精确更新一次项目模式。
    const changed = activity(); changed.settings.updateMode = "automatic";
    rpc.read.mockResolvedValue(changed);
    await act(async () => rpc.subscribe.mock.calls[0][0]({ projectId: "project-a", settingsRevision: 4, lifecycle: "ready" }));
    expect(settingsActivity!.activity).toBe(surface!.activity);
    expect(surface!.activity!.settings.updateMode).toBe("automatic");
    expect(rpc.read).toHaveBeenCalledTimes(initialReads + 1);
  });

  // A host restart replaces metadata ownership even if project identity and configuration keys stay equal.
  // 即使项目身份和配置键不变，宿主重启也会替换元数据归属。
  it("fences an old activity read and refresh action after a service epoch change", async () => {
    await act(async () => root.render(<Harness projectId="project-a" settingsVisible epoch={1} />));
    // retiredReload must not refresh the new service instance through an old UI closure.
    // retiredReload 不得通过旧界面闭包刷新新服务实例。
    const retiredReload = settingsActivity!.reload;
    // release controls a read owned by the old service generation.
    // release 控制旧服务代次所属的读取。
    let release!: (value: VceProjectActivityResponse) => void;
    rpc.read.mockImplementationOnce(() => new Promise<VceProjectActivityResponse>((resolve) => { release = resolve; }));
    await act(async () => { void settingsActivity!.reload(); });
    // replacement belongs to the new service and must survive the older response.
    // replacement 属于新服务，必须在旧响应到达后仍保留。
    const replacement = activity(); replacement.status.checkpointVersion = 9;
    rpc.read.mockResolvedValue(replacement);
    await act(async () => root.render(<Harness projectId="project-a" settingsVisible epoch={2} />));
    expect(surface!.activity!.status.checkpointVersion).toBe(9);
    await act(async () => release(activity()));
    expect(surface!.activity!.status.checkpointVersion).toBe(9);
    // reads captures the final authority before invoking an explicitly retired callback.
    // reads 在调用明确退役的回调前捕获最终权威调用量。
    const reads = rpc.read.mock.calls.length;
    await act(async () => retiredReload());
    expect(rpc.read).toHaveBeenCalledTimes(reads);
    expect(settingsActivity!.activity).toBe(surface!.activity);
  });

  // Prerequisites keep their explicit priority and checkpoint zero remains a valid identity.
  // 前提保持明确优先级，零检查点仍为有效身份。
  it("uses engine, project, initialization and compatibility gates independently", () => {
    const value = activity();
    expect(vceSurfaceGate(null, null, null, false)).toBe("no_project");
    expect(vceSurfaceGate("project-a", null, null, true)).toBe("loading");
    value.configuration.ready = false; value.settings.enabled = false;
    expect(vceSurfaceGate("project-a", value, null, false)).toBe("engine_off");
    value.configuration.ready = true;
    expect(vceSurfaceGate("project-a", value, null, true)).toBe("project_off");
    value.settings.enabled = true; value.status.checkpointVersion = null;
    expect(vceSurfaceGate("project-a", value, null, true)).toBe("uninitialized");
    value.status.lifecycle = "rebuild_required";
    expect(vceSurfaceGate("project-a", value, null, true)).toBe("rebuild_required");
    value.status.lifecycle = "indexing"; value.status.checkpointVersion = 0;
    expect(vceSurfaceGate("project-a", value, null, true)).toBe("ready");
    value.status.lifecycle = "failed";
    expect(vceSurfaceGate("project-a", value, null, true)).toBe("ready");
    value.queryAvailable = false;
    expect(vceSurfaceGate("project-a", value, null, true)).toBe("unavailable");
  });

  // Merely mounting a projectless composer cannot inspect or initialize historical projects.
  // 仅挂载无项目输入区不能检查或初始化历史项目。
  it("makes no metadata or job request without a project", async () => {
    await act(async () => root.render(<Harness projectId={null} />));
    expect(surface?.gate).toBe("no_project");
    expect(rpc.read).not.toHaveBeenCalled(); expect(rpc.jobs).not.toHaveBeenCalled();
    expect(rpc.submit).not.toHaveBeenCalled(); expect(host.textContent).toBe("");
  });

  // A delayed previous-project read cannot overwrite the newly selected project or its floating state.
  // 延迟的前一项目读取不能覆盖新选项目或其浮动状态。
  it("rejects late previous-project completion and hides disabled project nodes", async () => {
    let release!: (value: VceProjectActivityResponse) => void;
    rpc.read.mockImplementation((projectId: string) => projectId === "project-a"
      ? new Promise<VceProjectActivityResponse>((resolve) => { release = resolve; }) : Promise.resolve(activity(projectId)));
    await act(async () => root.render(<Harness projectId="project-a" />));
    expect(surface?.gate).toBe("loading");
    await act(async () => root.render(<Harness projectId="project-b" />));
    expect(surface?.activity?.status.projectId).toBe("project-b");
    await act(async () => release(activity("project-a")));
    expect(surface?.activity?.status.projectId).toBe("project-b");
    expect(host.textContent).toContain("语义索引");
    await act(async () => root.render(<Harness projectId="project-b" enabled={false} />));
    expect(host.textContent).toBe(""); expect(rpc.submit).not.toHaveBeenCalled();
  });

  // The menu saves full project policy through the existing revision contract, never session configuration.
  // 菜单通过既有修订契约保存完整项目策略，绝不保存会话配置。
  it("saves project mode without starting an indexing job", async () => {
    await act(async () => root.render(<Harness projectId="project-a" />));
    rpc.update.mockResolvedValue({ ...activity().settings, updateMode: "automatic", revision: 5 });
    await act(async () => surface!.saveMode("automatic"));
    expect(rpc.update).toHaveBeenCalledWith("project-a", 4, expect.objectContaining({ updateMode: "automatic", autoUpdateMinIntervalSeconds: 300 }));
    expect(rpc.update.mock.calls[0][2]).not.toHaveProperty("revision");
    expect(rpc.submit).not.toHaveBeenCalled();
  });

  // An ordinary running job preserves query controls and shares real progress with the floating node.
  // 普通运行作业保留查询控件，并与浮动节点共享真实进度。
  it("shows real job progress without replacing the last successful update time", async () => {
    const job: VceJobSummary = { projectId: "project-a", jobId: "job-1", kind: "synchronize", state: "running",
      sequence: 2, createdAtUnixMs: 2000, updatedAtUnixMs: 3000, sourceCheckpointVersion: 2,
      resultCheckpointVersion: null, followupRequested: false, error: null,
      progress: { phase: "embedding", operationId: "job-1", unit: "batches", completed: 3, total: 7, currentPath: null } };
    rpc.jobs.mockResolvedValue(job);
    await act(async () => root.render(<Harness projectId="project-a" />));
    expect(surface?.gate).toBe("ready"); expect(surface?.activity?.lastSuccessfulUpdateUnixMs).toBe(1000);
    expect(host.querySelector("progress")?.getAttribute("value")).toBe("3");
    expect(host.querySelector("progress")?.getAttribute("max")).toBe("7");
    expect(host.textContent).toContain("会话和已有索引查询仍可正常使用");
    const initialLayoutRenders = visibilityRenders;
    await act(async () => rpc.subscribeJobs.mock.calls[0][0]({ ...job, sequence: 3,
      progress: { ...job.progress!, completed: 4 } }));
    expect(host.querySelector("progress")?.getAttribute("value")).toBe("4");
    expect(visibilityRenders).toBe(initialLayoutRenders);
  });

  // First-build retry is an explicit new command with an exact failed receipt as its origin.
  // 首次建库重试是以精确失败回执为来源的明确新命令。
  it("retries failed initialization through durable admission without inventing a checkpoint", async () => {
    const failed: VceJobSummary = { projectId: "project-a", jobId: "failed-first", kind: "initialize", state: "failed",
      sequence: 4, createdAtUnixMs: 2000, updatedAtUnixMs: 3000, sourceCheckpointVersion: null,
      resultCheckpointVersion: null, followupRequested: false, error: "fixture failure", progress: null };
    const value = activity(); value.status.checkpointVersion = null; value.status.lifecycle = "failed"; value.queryAvailable = false;
    rpc.read.mockResolvedValue(value); rpc.jobs.mockResolvedValue(failed);
    rpc.submit.mockResolvedValue({ ...failed, jobId: "retried-first", state: "queued", sequence: 1,
      createdAtUnixMs: 4000, updatedAtUnixMs: 4000, retryOfJobId: "failed-first", error: null });
    await act(async () => root.render(<Harness projectId="project-a" />));
    expect(surface?.gate).toBe("uninitialized"); expect(host.textContent).toContain("重试索引任务");
    await act(async () => surface!.retryLatest());
    expect(rpc.submit).toHaveBeenCalledWith(expect.objectContaining({ projectId: "project-a", action: "initialize",
      expectedCheckpointVersion: null, expectedSettingsRevision: 4, retryOfJobId: "failed-first" }));
    expect(surface?.jobState.job?.jobId).toBe("retried-first");
    expect(surface?.gate).toBe("uninitialized");
  });
});
