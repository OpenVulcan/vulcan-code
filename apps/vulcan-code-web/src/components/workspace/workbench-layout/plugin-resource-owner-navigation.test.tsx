// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider, readLocaleMessage } from "@/i18n";
import { browserFallbackSnapshot, browserFallbackStartup } from "@/service/browser-fallback-workbench";
import { diagnosticReport, diagnosticResource } from "@/service/__fixtures__/plugin-resource-diagnostics";
import { serviceClient } from "@/service/webui-service-runtime";
import type { WorkbenchSnapshot } from "@/types";
import type { WorkspaceShellProps } from "../workspace-shell/types";
import type { WorkbenchDeltaSignal } from "./types";
import { PluginResourceDiagnostics } from "@/components/features/skills/plugin-resource-diagnostics";
import { WorkbenchLayout } from "../workbench-layout";

// stream records actual layout commits without requiring an unrelated transcript event registry.
// stream 记录实际布局提交，不要求无关转录事件注册表。
const stream = vi.hoisted(() => ({ resetForWorkbenchSnapshot: vi.fn(), flushNotifications: vi.fn() }));
vi.mock("@/state/conversation-live-stream-context", () => ({ useConversationLiveStreamStore: () => stream }));
vi.mock("@/notifications/coordinator", () => ({ notificationCoordinator: { setRouteHandler: vi.fn() } }));
vi.mock("@/service/webui-service-runtime", () => ({ hasServiceRuntime: () => true,
  serviceClient: { request: vi.fn(), subscribe: vi.fn().mockResolvedValue(() => undefined) } }));
vi.mock("@/service/plugin-resource-diagnostics-service", () => ({ readPluginResourceDiagnostics: vi.fn().mockResolvedValue({
  resources: { ...diagnosticReport, resources: [diagnosticResource] }, update: null, uninstall: null }) }));
vi.mock("@/components/workspace/workbench-layout/shell-chrome", () => ({ useWorkbenchShellChrome: () => ({ shellChromeActions: null, showWindowControls: false, windowDragRegionEnabled: false }) }));
vi.mock("@/components/workspace/workbench-layout/conversation-actions", () => ({ useWorkbenchConversationActions: () => ({}) }));
vi.mock("@/components/workspace/workbench-layout/session-actions", () => ({ useWorkbenchSessionActions: () => ({}) }));
vi.mock("@/components/workspace/workbench-layout/project-actions", () => ({ useWorkbenchProjectActions: () => ({ luaSkillActions: {}, agentSkillActions: {}, agentActions: {} }) }));
vi.mock("@/components/shell/activity-rail", () => ({
  // ActivityRail keeps the actual layout's session route callback while simplifying unrelated tree presentation.
  // ActivityRail 保留真实布局的会话路由回调，同时简化无关树展示。
  ActivityRail: (props: { onSelectSessions: () => void }) => <button onClick={props.onSelectSessions}>Return through session rail</button>,
}));
vi.mock("@/components/project/terminal/project-floating-terminal-layer", () => ({ ProjectFloatingTerminalLayer: () => null }));
// The feedback center reads announcements through the same service client once its module loads, which would add
// unrelated calls to the navigation requests these tests count.
// 反馈中心在其模块加载后经同一服务客户端读取公告，会给这些测试统计的导航请求加入无关调用。
vi.mock("@/components/features/feedback/feedback-center-runtime", () => ({ startFeedbackCenter: () => ({ stop: () => undefined }) }));
vi.mock("@/components/workspace/workspace-shell", () => ({
  // WorkspaceShell substitutes only presentation; navigation callbacks are supplied by the actual WorkbenchLayout.
  // WorkspaceShell 仅替换展示；导航回调由实际 WorkbenchLayout 提供。
  // usageOpen models the maintenance drawer that hosts plugin usage; closing it disposes the diagnostic view.
  // usageOpen 模拟承载插件使用情况的维护抽屉；关闭即销毁诊断视图。
  WorkspaceShell: function Shell(props: WorkspaceShellProps) {
    const [usageOpen, setUsageOpen] = React.useState(true);
    return <div data-main-view={props.mainView} data-selected-owner={props.selectedConversationId}
    data-navigation-phase={props.conversationNavigation === null || props.conversationNavigation === undefined ? "none" : props.conversationNavigation.phase}>
    {props.rail}
    <button onClick={() => { setUsageOpen(true); props.onSlashCommand({ id: "skills", command: "/skills" } as Parameters<WorkspaceShellProps["onSlashCommand"]>[0]); props.onSelectSkillSubArea("plugins"); }}>Open plugin diagnostics</button>
    <button onClick={() => setUsageOpen(false)}>Close plugin usage</button>
    <button onClick={() => props.onOpenMemory()}>Leave for memory</button>
    <button onClick={() => props.onSelectConversation("conversation-original")}>Sidebar selection</button>
    <button onClick={() => props.onSelectConversation("other-service-selected")}>Sidebar selection B</button>
    {props.mainView === "skills" && usageOpen && <PluginResourceDiagnostics selection={diagnosticSelection} onSelect={() => undefined}
      conversations={props.workbench.conversations} onSelectPluginResourceOwner={props.onSelectPluginResourceOwner} />}
  </div>;
  },
}));
// preview is the current detached browser fixture returned by its authoritative factory.
// preview 是权威工厂返回的当前脱离引用浏览器夹具。
const preview = browserFallbackSnapshot();
// source supplies a complete real browser fixture conversation by its selected identity, never its position.
// source 按选中身份提供完整真实浏览器夹具会话，绝不按位置选择。
const source = preview.conversations.find((conversation) => conversation.id === preview.selectedConversationId);
if (!source) throw new Error("Expected selected browser fixture conversation; its identity may be outdated");
// ownerProject supplies a real complete project shape for the independently removable original owner.
// ownerProject 为可独立移除的原所有者提供真实完整项目形状。
const ownerProject = preview.projects.find((project) => project.id === source.projectId);
if (!ownerProject) throw new Error("Expected selected browser fixture project; its identity may be outdated");
// workbench retains the initial owner and includes a cached original whose storage may disappear before selection.
// workbench 保留初始所有者，并包含选择前存储可能消失的缓存原实例。
const workbench: WorkbenchSnapshot & { revision: number } = { ...preview, revision: 1,
  projects: [...preview.projects, { ...ownerProject, id: "original-owner-project" }], conversations: [source,
  { ...source, id: "conversation-original", projectId: "original-owner-project" }, { ...source, id: "other-service-selected" }], plugins: [] };
// diagnosticSelection preserves the retained report while actual lifecycle deltas rerender its conversation projection.
// diagnosticSelection 在真实生命周期增量重新渲染会话投影时保留资源报告。
const diagnosticSelection = { kind: "plugin", pluginId: "fixture" } as const;
// consumed observes the exact real synchronization acknowledgement for each lifecycle batch.
// consumed 观察每个生命周期批次的精确真实同步确认。
const consumed = vi.fn();
// host owns the real layout's rendered navigation state and diagnostic row.
// host 拥有真实布局渲染的导航状态及诊断行。
let host: HTMLDivElement;
// root disposes the actual layout and its pending owner observation after each test.
// root 在每个测试后销毁实际布局及其待完成所有者观测。
let root: Root;
beforeEach(async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true); vi.clearAllMocks();
  // Navigation timers read the stylesheet's motion tokens; declare them so transitions keep real durations.
  // 导航计时器读取样式表动效标记；在此声明，使过渡保持真实时长。
  document.documentElement.style.setProperty("--vk-t-fast", "120ms"); document.documentElement.style.setProperty("--vk-t-med", "180ms");
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", { configurable: true, value: vi.fn() });
  vi.mocked(serviceClient.request).mockResolvedValue(null);
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  await renderLayout(null);
  await click("Open plugin diagnostics");
  vi.mocked(serviceClient.request).mockClear(); stream.resetForWorkbenchSnapshot.mockClear(); stream.flushNotifications.mockClear();
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });
// renderLayout delivers a real service delta to the existing synchronization hook and waits for React commits.
// renderLayout 将真实服务增量交付既有同步钩子，并等待 React 提交。
// signal is the exact backend envelope batch; returns after the real revision gates have processed it.
// signal 是精确后端信封批次；真实版本闸门处理后返回。
async function renderLayout(signal: WorkbenchDeltaSignal | null): Promise<void> {
  await act(async () => root.render(<I18nProvider initialLocale="en-US"><WorkbenchLayout serviceInstanceEpoch={1}
    startup={browserFallbackStartup} workbench={workbench} workbenchDelta={signal} onWorkbenchDeltaConsumed={consumed}
    onSelectedConversationIdChanged={() => undefined} isConversationInitialized={() => true} /></I18nProvider>));
}
// click locates a semantic real control and returns after its React updates complete.
// click 定位语义真实控件，在其 React 更新完成后返回。
async function click(label: string): Promise<void> {
  // found locates the currently mounted button without depending on evolving row positions.
  // found 定位当前挂载按钮，不依赖演进行位置。
  const found = [...host.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent === label);
  if (!found) throw new Error(`Expected ${label}; its fixture label or availability may have changed`);
  await act(async () => found.click());
}

it.each(["other", "cleared", "missing", "rejected"] as const)("preserves diagnostics and exposes %s original-owner confirmation failure in the real handler", async (outcome) => {
  if (outcome === "rejected") vi.mocked(serviceClient.request).mockRejectedValueOnce(new Error("Original selection rejected"));
  else vi.mocked(serviceClient.request).mockResolvedValueOnce(outcome === "missing" ? null : {
    ...workbench, revision: workbench.revision + 1, selectedConversationId: outcome === "other" ? "other-service-selected" : null });
  await click(readLocaleMessage("en-US", "pluginDiagnostics.locate"));
  expect(serviceClient.request).toHaveBeenCalledExactlyOnceWith("service_select_conversation", { conversationId: "conversation-original" });
  expect(host.querySelector('[data-main-view="skills"]')).not.toBeNull();
  expect(host.querySelector('[data-selected-owner]')?.getAttribute("data-selected-owner")).toBe(source.id);
  expect(host.querySelector('[role="alert"]')?.textContent).toContain("Original owner conversation unavailable or archived");
  expect(host.textContent).toContain(diagnosticResource.resourceId);
  expect(stream.resetForWorkbenchSnapshot).not.toHaveBeenCalled();
  if (outcome === "rejected") expect(host.textContent).toContain("Original selection rejected");
});

it("opens chat only after exact service confirmation and sends only the original owner ID", async () => {
  vi.mocked(serviceClient.request).mockResolvedValueOnce({ ...workbench, revision: workbench.revision + 1, selectedConversationId: "conversation-original",
    conversations: workbench.conversations.map((conversation) => conversation.id === "conversation-original" ? { ...conversation, unread: true } : conversation) });
  await click(readLocaleMessage("en-US", "pluginDiagnostics.locate"));
  expect(serviceClient.request).toHaveBeenNthCalledWith(1, "service_select_conversation", { conversationId: "conversation-original" });
  expect(host.querySelector('[data-main-view="chat"]')).not.toBeNull();
  expect(host.querySelector('[data-selected-owner]')?.getAttribute("data-selected-owner")).toBe("conversation-original");
  expect(stream.resetForWorkbenchSnapshot).toHaveBeenCalledTimes(1);
  expect(serviceClient.request).toHaveBeenCalledWith("service_mark_conversation_read", { conversationId: "conversation-original" });
});

it("does not persist a read marker for an owner the selection reply already shows as read", async () => {
  vi.mocked(serviceClient.request).mockResolvedValueOnce({ ...workbench, revision: workbench.revision + 1, selectedConversationId: "conversation-original",
    conversations: workbench.conversations.map((conversation) => conversation.id === "conversation-original" ? { ...conversation, unread: false } : conversation) });
  await click(readLocaleMessage("en-US", "pluginDiagnostics.locate"));
  expect(host.querySelector('[data-main-view="chat"]')).not.toBeNull();
  expect(serviceClient.request).toHaveBeenCalledWith("service_select_conversation", { conversationId: "conversation-original" });
  expect(serviceClient.request).not.toHaveBeenCalledWith("service_mark_conversation_read", expect.anything());
});

it.each(["leave", "collapse"] as const)("does not reopen chat after %s invalidates a pending exact owner reply", async (navigation) => {
  // finish completes the same real selection RPC after its diagnostic source has become unavailable.
  // finish 在其诊断来源不可用后完成相同真实选择 RPC。
  let finish!: (snapshot: WorkbenchSnapshot) => void;
  vi.mocked(serviceClient.request).mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  await click(readLocaleMessage("en-US", "pluginDiagnostics.locate"));
  expect(host.querySelector('[data-main-view="skills"]')).not.toBeNull();
  if (navigation === "leave") await click("Leave for memory");
  else await click("Close plugin usage");
  await act(async () => finish({ ...workbench, revision: workbench.revision + 1, selectedConversationId: "conversation-original" }));
  expect(host.querySelector(`[data-main-view="${navigation === "leave" ? "memory" : "skills"}"]`)).not.toBeNull();
  expect(host.querySelector('[data-selected-owner]')?.getAttribute("data-selected-owner")).toBe(source.id);
  expect(serviceClient.request).toHaveBeenCalledExactlyOnceWith("service_select_conversation", { conversationId: "conversation-original" });
  expect(stream.resetForWorkbenchSnapshot).not.toHaveBeenCalled();
});

it("preserves the existing sidebar behavior for a service-selected replacement conversation", async () => {
  vi.mocked(serviceClient.request).mockResolvedValueOnce({ ...workbench, revision: workbench.revision + 1, selectedConversationId: "other-service-selected" });
  await click("Sidebar selection");
  expect(host.querySelector('[data-main-view="chat"]')).not.toBeNull();
  await act(async () => { await new Promise((resolve) => window.setTimeout(resolve, 230)); });
  expect(host.querySelector('[data-selected-owner]')?.getAttribute("data-selected-owner")).toBe("other-service-selected");
});

it("does not publish a route or read marker after the actual layout is disposed", async () => {
  // finish delivers the original service reply after both layout and diagnostic ownership have ended.
  // finish 在布局及诊断所有权结束后交付原服务响应。
  let finish!: (snapshot: WorkbenchSnapshot) => void;
  vi.mocked(serviceClient.request).mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  await click(readLocaleMessage("en-US", "pluginDiagnostics.locate"));
  await act(async () => root.unmount());
  await act(async () => finish({ ...workbench, revision: workbench.revision + 1, selectedConversationId: "conversation-original" }));
  expect(serviceClient.request).toHaveBeenCalledExactlyOnceWith("service_select_conversation", { conversationId: "conversation-original" });
  expect(stream.resetForWorkbenchSnapshot).not.toHaveBeenCalled();
  // root is renewed only for the common cleanup after the explicit disposal assertion.
  // root 仅为明确销毁断言后的共用清理重新建立。
  root = createRoot(host);
});

it.each(["conversationArchived", "conversationsArchived", "projectRemoved"] as const)("rejects a deferred exact response after real %s synchronization removes its owner", async (kind) => {
  // finish delivers an older exact selection only after the newer lifecycle batch has committed.
  // finish 仅在较新生命周期批次提交后交付旧的精确选择响应。
  let finish!: (snapshot: WorkbenchSnapshot) => void;
  vi.mocked(serviceClient.request).mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  await click(readLocaleMessage("en-US", "pluginDiagnostics.locate"));
  // archive identifies the original controller owner and retains the current source selection.
  // archive 标识原控制器所有者并保留当前来源选择。
  const archive = { conversationId: "conversation-original", archivedConversation: null, selectedConversationId: source.id };
  // signal enters the production synchronization hook with the same committed lifecycle shape as the service.
  // signal 以与服务相同的已提交生命周期形状进入生产同步钩子。
  const signal: WorkbenchDeltaSignal = { seq: 1, envelopes: [{ generation: workbench.revision + 2,
    delta: kind === "conversationArchived" ? { kind, payload: archive }
      : kind === "conversationsArchived" ? { kind, payload: { archivedConversations: [archive] } }
        : { kind, payload: { projectId: "original-owner-project", projects: preview.projects, expandedProjectIds: [], archivedConversations: [archive] } } }] };
  await renderLayout(signal);
  expect(consumed).toHaveBeenCalledWith(signal.seq);
  expect(host.textContent).toContain("Original owner conversation unavailable or archived");
  expect(host.textContent).toContain(diagnosticResource.resourceId);
  await act(async () => finish({ ...workbench, revision: workbench.revision + 1, selectedConversationId: "conversation-original" }));
  expect(host.querySelector('[data-main-view="skills"]')).not.toBeNull();
  expect(host.querySelector('[data-selected-owner]')?.getAttribute("data-selected-owner")).toBe(source.id);
  expect(stream.resetForWorkbenchSnapshot).not.toHaveBeenCalled();
  expect(serviceClient.request).toHaveBeenCalledExactlyOnceWith("service_select_conversation", { conversationId: "conversation-original" });
});

it("rejects an exact owner snapshot older than an accepted unrelated archive while the original owner stays visible", async () => {
  // finish isolates the shared revision gate from row removal and local observation cancellation.
  // finish 将共享版本闸门与资源行移除及本地观察取消隔离。
  let finish!: (snapshot: WorkbenchSnapshot) => void;
  vi.mocked(serviceClient.request).mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  await click(readLocaleMessage("en-US", "pluginDiagnostics.locate"));
  await renderLayout({ seq: 2, envelopes: [{ generation: workbench.revision + 2, delta: { kind: "conversationArchived",
    payload: { conversationId: "other-service-selected", archivedConversation: null, selectedConversationId: source.id } } }] });
  expect([...host.querySelectorAll("button")].some((button) => button.textContent === readLocaleMessage("en-US", "pluginDiagnostics.locate"))).toBe(true);
  await act(async () => finish({ ...workbench, revision: workbench.revision + 1, selectedConversationId: "conversation-original" }));
  expect(host.querySelector('[data-main-view="skills"]')).not.toBeNull();
  expect(host.querySelector('[data-selected-owner]')?.getAttribute("data-selected-owner")).toBe(source.id);
  expect(host.querySelector('[role="alert"]')?.textContent).toContain("Original owner conversation unavailable or archived");
  expect(stream.resetForWorkbenchSnapshot).not.toHaveBeenCalled();
  expect(serviceClient.request).toHaveBeenCalledExactlyOnceWith("service_select_conversation", { conversationId: "conversation-original" });
});

it.each(["missing", "rejected", "cancelled"] as const)("clears a superseded sidebar transition before %s owner navigation returns through the real session rail", async (outcome) => {
  // finishSidebar retains the original pending sidebar RPC whose old version loses authority to diagnostics.
  // finishSidebar 保留原待完成侧栏 RPC，其旧版本将向诊断让出权限。
  let finishSidebar!: (snapshot: WorkbenchSnapshot) => void;
  vi.mocked(serviceClient.request).mockImplementationOnce(() => new Promise((resolve) => { finishSidebar = resolve; }));
  await click("Sidebar selection B");
  expect(host.querySelector('[data-navigation-phase]')?.getAttribute("data-navigation-phase")).toBe("exiting");
  await click("Open plugin diagnostics");
  // finishOwner is used only to deliver a cancelled exact reply after the diagnostic section is collapsed.
  // finishOwner 仅用于在诊断区域折叠后交付已取消的精确响应。
  let finishOwner!: (snapshot: WorkbenchSnapshot) => void;
  if (outcome === "missing") vi.mocked(serviceClient.request).mockResolvedValueOnce(null);
  else if (outcome === "rejected") vi.mocked(serviceClient.request).mockRejectedValueOnce(new Error("Owner selection failed"));
  else vi.mocked(serviceClient.request).mockImplementationOnce(() => new Promise((resolve) => { finishOwner = resolve; }));
  await click(readLocaleMessage("en-US", "pluginDiagnostics.locate"));
  if (outcome === "cancelled") {
    await click("Close plugin usage");
    await act(async () => finishOwner({ ...workbench, revision: workbench.revision + 1, selectedConversationId: "conversation-original" }));
  }
  await click("Return through session rail");
  await act(async () => {
    finishSidebar({ ...workbench, revision: workbench.revision + 1, selectedConversationId: "other-service-selected" });
    await new Promise((resolve) => window.setTimeout(resolve, 230));
  });
  expect(host.querySelector('[data-main-view="chat"]')).not.toBeNull();
  expect(host.querySelector('[data-selected-owner]')?.getAttribute("data-selected-owner")).toBe(source.id);
  expect(host.querySelector('[data-navigation-phase]')?.getAttribute("data-navigation-phase")).toBe("none");
  expect(stream.resetForWorkbenchSnapshot).not.toHaveBeenCalled();
  expect(serviceClient.request).toHaveBeenCalledTimes(2);
  expect(serviceClient.request).toHaveBeenNthCalledWith(1, "service_select_conversation", { conversationId: "other-service-selected" });
  expect(serviceClient.request).toHaveBeenNthCalledWith(2, "service_select_conversation", { conversationId: "conversation-original" });
});
