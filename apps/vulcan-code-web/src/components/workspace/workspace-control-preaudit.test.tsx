// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { browserFallbackSnapshot, browserFallbackStartup } from "@/service/browser-fallback-workbench";
import { WorkspaceTitleMenu } from "./workspace-title-menu";
import { WorkspaceContextPanel, type WorkspaceContextPanelProps } from "./WorkspaceContextPanel";
import { WorkspaceResizableBody } from "./workspace-resizable-body";
import { WorkspaceFilesPanelHost } from "./WorkspaceFilesPanelHost";
import { DEFAULT_WORKSPACE_FILE_FILTERS } from "@/service/workbench-snapshot-service";
import { toast } from "sonner";
import { ConversationSessionTodolistPanel } from "@/components/conversation/session-todolist-panel";
import { TooltipProvider } from "@/components/ui/tooltip";

// Replace transcript IO and unrelated descendants, preserving the actual workspace controls and Radix dialogs.
// 替换转录输入输出及无关后代，保留真实工作区控件和 Radix 弹窗。
vi.mock("@/components/conversation/conversation-branch-workspace-v1", () => ({ ConversationBranchWorkspaceV1: () => null }));
vi.mock("@/state/conversation-live-stream-context", () => ({ useConversationTransientReasoningStatus: () => null }));
vi.mock("@/state/conversation-session-event-window-owner-v1", () => ({ useConversationSessionEventWindowOwnerV1: () => ({ snapshot: { events: [] }, loading: false, error: null }) }));
vi.mock("@/components/conversation/chat-area/conversation-scroll-controller", () => ({ useConversationScrollController: () => ({ scheduleStoredRestore: () => undefined }) }));
vi.mock("@/components/ui/sidebar", () => ({ SidebarTrigger: () => null }));
vi.mock("@/components/plugins/dynamic-plugin-mount-panel", () => ({ DynamicPluginMountPanel: () => null }));
vi.mock("sonner", () => ({ toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() } }));
vi.mock("@/components/features/code-context/vce-project-controls", () => ({ VceFloatingNode: () => null }));
vi.mock("@/components/features/code-context/vce-project-surface", () => ({ useVceProjectVisibility: () => false }));

// host/root own isolated DOM and never open native windows or execute service mutations.
// host/root 持有隔离文档，不打开原生窗口或执行服务写入。
let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  vi.clearAllMocks();
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => window.setTimeout(() => callback(0), 0));
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ x: 0, y: 0, left: 0, top: 0, right: 1600, bottom: 900, width: 1600, height: 900, toJSON: () => ({}) });
  Object.defineProperty(HTMLElement.prototype, "scrollTo", { configurable: true, value: vi.fn() });
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

/** Mounts content with the actual localization provider; resolves after effects settle.
 * 使用真实本地化提供器挂载 content；副作用结束后完成。 */
async function render(content: React.ReactNode): Promise<void> {
  await act(async () => root.render(<I18nProvider initialLocale="zh-CN">{content}</I18nProvider>));
}

/** Builds exact workspace props with fictional parent/task identities and rejecting unused IO.
 * 以虚构父级/任务身份及明确拒绝的未使用输入输出构建精确工作区 props。 */
function contextProps(): WorkspaceContextPanelProps {
  // workbench is the production typed in-memory baseline; no fixture performs runtime discovery.
  // workbench 为生产强类型内存基线；夹具不执行运行时发现。
  const workbench = browserFallbackSnapshot();
  workbench.subagentNavigation = { parentConversationId: "parent", activeConversationId: "task-a", tasks: ["task-a", "task-b"].map(taskId => ({ taskId, publicTaskId: taskId, label: taskId, runState: "running", isRunning: true, isActive: true })) };
  workbench.plugins = []; workbench.conversations = [{ id: "parent", title: "Parent", status: "running" }];
  // rejectUnused fails any unexpected transport invocation instead of returning a fabricated success.
  // rejectUnused 对意外传输调用直接失败，不返回虚构成功。
  const rejectUnused = async (): Promise<never> => { throw new Error("Unexpected audit IO"); };
  return { workbench, selectedDraftConversation: null, selectedConversationId: "parent",
    localBrowserAvailable: false,
    tabs: ["task-a", "task-b"].map(id => ({ id, kind: "subagent", title: id, subagentParentConversationId: "parent", subagentTaskId: id, createdAt: 0 })), activeTabId: "task-a",
    reviewFocusRequest: { sequence: 0, repositoryKey: null }, onOpenContextTab: vi.fn(), onCloseContextTab: vi.fn(), onActivateContextTab: vi.fn(), onFileTabTitleChange: vi.fn(),
    onStopCommandSession: rejectUnused, onStopSubagentTask: rejectUnused, onCloseSubagentTask: rejectUnused, onOpenSubagentPanel: rejectUnused,
    onCopyRawDiff: rejectUnused, onCopyText: rejectUnused, resolveMediaSourceUrl: path => path, onRunVmmAction: rejectUnused,
    loadWorkspaceDiffSnapshot: rejectUnused, loadWorkspaceDiffFileDetail: rejectUnused, loadWorkspaceDiffSearch: rejectUnused, runWorkspaceGitFileAction: rejectUnused,
    loadWorkspaceFileDirectory: rejectUnused, loadWorkspaceFilePreview: rejectUnused };
}

it("locks monitoring synchronously and keeps a new project visit locked when an old request settles", async () => {
  // Each invocation has a separate continuation so an older project visit can finish after the newer visit.
  // 每次调用持有独立续传，使较早项目访问可在较新访问之后结束。
  const releases: Array<() => void> = [];
  const toggle = vi.fn(() => new Promise<void>(resolve => releases.push(resolve)));
  const props: React.ComponentProps<typeof ConversationSessionTodolistPanel> = {
    expanded: true, todolist: null, reviewSummary: null, terminalSessions: [], subagentNavigation: null, commitDialogOpen: false, commitBusyState: null,
    reviewRepositories: [{ repositoryKey: "repo", repositoryName: "Audit", relativePath: "repo", isPrimary: true, summary: null, monitoringEnabled: true, loading: false, error: null }],
    onExpandedChange: vi.fn(), onCommitDialogOpenChange: vi.fn(), onOpenReviewChanges: vi.fn(), onStopCommandSession: async () => undefined, onOpenSubagentPanel: async () => false, onToggleReviewMonitoring: toggle,
  };
  // Render the real repository control with an explicit project identity and no service transport.
  // 使用明确项目身份渲染真实仓库控件，不接入服务传输。
  const content = (project: string) => <TooltipProvider><ConversationSessionTodolistPanel {...props} reviewRepositoryOrderStorageKey={project} /></TooltipProvider>;
  await render(content("project-a"));
  const button = host.querySelector<HTMLButtonElement>(".conversation-session-review-monitor-toggle")!;
  expect(button).not.toBeNull();
  await act(async () => { button.click(); button.click(); });
  expect(toggle).toHaveBeenCalledTimes(1);
  await render(content("project-b"));
  await render(content("project-a"));
  await act(async () => host.querySelector<HTMLButtonElement>(".conversation-session-review-monitor-toggle")!.click());
  expect(toggle).toHaveBeenCalledTimes(2);
  await act(async () => releases[0]());
  expect(host.querySelector<HTMLButtonElement>(".conversation-session-review-monitor-toggle")!.disabled).toBe(true);
  await act(async () => releases[1]());
  expect(host.querySelector<HTMLButtonElement>(".conversation-session-review-monitor-toggle")!.disabled).toBe(false);
});

it("keeps hook order stable across conversation, empty, and restored title menus", async () => {
  const conversation = { id: "parent", title: "Parent", status: "idle" };
  const action = vi.fn();
  await render(<WorkspaceTitleMenu title="Parent" conversation={conversation} onConversationMenuAction={action} />);
  expect(host.querySelector("button")).not.toBeNull();
  await render(<WorkspaceTitleMenu title="Empty" onConversationMenuAction={action} />);
  expect(host.querySelector("button")).toBeNull();
  await render(<WorkspaceTitleMenu title="Parent" conversation={conversation} onConversationMenuAction={action} />);
  expect(host.querySelector("button")).not.toBeNull();
});

it("navigates context tabs with arrows and boundary keys and omits unimplemented shortcut hints", async () => {
  const props = contextProps();
  await render(<WorkspaceContextPanel {...props} />);
  const tabs = Array.from(host.querySelectorAll<HTMLButtonElement>('[role="tab"]'));
  expect(tabs.map(tab => tab.tabIndex)).toEqual([0, -1]);
  await act(async () => tabs[0].dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true })));
  expect(props.onActivateContextTab).toHaveBeenLastCalledWith("task-b");
  expect(document.activeElement).toBe(tabs[1]);
  await act(async () => tabs[1].dispatchEvent(new KeyboardEvent("keydown", { key: "Home", bubbles: true })));
  expect(document.activeElement).toBe(tabs[0]);
  await act(async () => tabs[0].dispatchEvent(new KeyboardEvent("keydown", { key: "End", bubbles: true })));
  expect(document.activeElement).toBe(tabs[1]);
  await act(async () => tabs[1].dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true })));
  expect(document.activeElement).toBe(tabs[0]);
  await render(<WorkspaceContextPanel {...props} tabs={[]} activeTabId={null} />);
  expect(host.textContent).not.toContain("Ctrl+");
});

it("shows the built-in browser entry only for a capable GUI without requiring the AI browser plugin", async () => {
  const props = contextProps();
  // The empty plugin catalog proves the local browser card is a built-in GUI entry.
  // 空插件目录证明本地浏览器卡片是 GUI 内置入口。
  expect(props.workbench.plugins).toEqual([]);
  await render(<WorkspaceContextPanel {...props} localBrowserAvailable tabs={[]} activeTabId={null} />);
  const localBrowserCard = Array.from(host.querySelectorAll<HTMLButtonElement>(".context-launcher-card"))
    .find(button => button.textContent?.includes("内置浏览器"));
  expect(localBrowserCard).toBeDefined();
  expect(host.textContent).not.toContain("AI 浏览器");
  await act(async () => localBrowserCard!.click());
  expect(props.onOpenContextTab).toHaveBeenCalledExactlyOnceWith("local-browser");
  await render(<WorkspaceContextPanel {...props} tabs={[]} activeTabId={null} />);
  expect(host.textContent).not.toContain("内置浏览器");
});

it.each(["stop", "close"] as const)("retains %s failure, prevents duplicate requests, and isolates a different task tab", async kind => {
  const props = contextProps();
  // rejectAction releases a delayed failure only after repeated native clicks.
  // rejectAction 在重复原生点击后才释放迟到失败。
  let rejectAction!: (error: Error) => void;
  const action = vi.fn().mockImplementationOnce(() => new Promise<void>((_resolve, reject) => { rejectAction = reject; })).mockResolvedValue(undefined);
  if (kind === "stop") props.onStopSubagentTask = action; else props.onCloseSubagentTask = action;
  await render(<WorkspaceContextPanel {...props} />);
  const buttons = host.querySelectorAll<HTMLButtonElement>(".workspace-subagent-footer button");
  await act(async () => buttons[kind === "stop" ? 0 : 1].click());
  const confirm = document.querySelector<HTMLButtonElement>('[role="dialog"] button[data-variant="destructive"]')!;
  expect(confirm).not.toBeNull();
  await act(async () => { confirm.click(); confirm.click(); });
  expect(action).toHaveBeenCalledExactlyOnceWith("parent", "task-a");
  await act(async () => rejectAction(new Error("Synthetic denied")));
  expect(document.querySelector('[role="dialog"] [role="alert"]')?.textContent).toBe("Synthetic denied");
  expect(confirm.disabled).toBe(false);
  await render(<WorkspaceContextPanel {...props} activeTabId="task-b" />);
  expect(document.querySelector('[role="dialog"]')).toBeNull();
});

it("resizes the dock with keyboard and releases pointer listeners after cancellation", async () => {
  // A narrow dock leaves 469px after the reading column, padding, reasoning gutter, and handle.
  // 窄停靠容器保留阅读列、内边距、思考留白和手柄后，剩余 469px。
  vi.mocked(HTMLElement.prototype.getBoundingClientRect).mockReturnValue({ x: 0, y: 0, left: 0, top: 0, right: 1400, bottom: 900, width: 1400, height: 900, toJSON: () => ({}) });
  const context = contextProps();
  const props: React.ComponentProps<typeof WorkspaceResizableBody> = { ...context, selectedDraftConversation: null, contextPanelTabs: [], contextPanelActiveTabId: null,
    aiBrowserAvailable: false, rightPanelOpen: true, rightPanelFloating: false, startup: browserFallbackStartup, children: <div>Body</div>,
    onFileContextTabTitleChange: vi.fn(), onConversationMenuAction: vi.fn(), onToggleRightPanel: vi.fn(), onToggleLeftPanel: vi.fn(), onTrace: vi.fn() };
  await render(<WorkspaceResizableBody {...props} />);
  const handle = host.querySelector<HTMLElement>('[role="separator"]')!;
  expect(handle.tabIndex).toBe(0);
  expect(handle.getAttribute("aria-valuemax")).toBe("469");
  await act(async () => handle.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowLeft", bubbles: true })));
  expect(handle.getAttribute("aria-valuenow")).toBe("440");
  await act(async () => handle.dispatchEvent(new MouseEvent("pointerdown", { button: 0, clientX: 900, bubbles: true })));
  await act(async () => window.dispatchEvent(new MouseEvent("pointermove", { clientX: 880 })));
  expect(handle.getAttribute("aria-valuenow")).toBe("460");
  await act(async () => window.dispatchEvent(new Event("pointercancel")));
  await act(async () => window.dispatchEvent(new MouseEvent("pointermove", { clientX: 850 })));
  expect(handle.getAttribute("aria-valuenow")).toBe("460");
  await act(async () => handle.dispatchEvent(new KeyboardEvent("keydown", { key: "End", bubbles: true })));
  expect(handle.getAttribute("aria-valuenow")).toBe("469");
  await act(async () => handle.dispatchEvent(new KeyboardEvent("keydown", { key: "Home", bubbles: true })));
  expect(handle.getAttribute("aria-valuenow")).toBe("320");
  await render(<WorkspaceResizableBody {...props} rightPanelOpen={false} />);
  expect(host.querySelector("#workspaceContext")?.hasAttribute("inert")).toBe(true);
});

it("lets a wide right dock consume whitespace without shrinking the conversation and reasoning gutter", async () => {
  // The 1800px shell can allocate 869px right after reserving 922px of conversation and the handle.
  // 1800px 外壳保留 922px 会话区与手柄后，可为右侧分配 869px。
  vi.mocked(HTMLElement.prototype.getBoundingClientRect).mockReturnValue({ x: 0, y: 0, left: 0, top: 0, right: 1800, bottom: 900, width: 1800, height: 900, toJSON: () => ({}) });
  const context = contextProps();
  const props: React.ComponentProps<typeof WorkspaceResizableBody> = { ...context, selectedDraftConversation: null, contextPanelTabs: [], contextPanelActiveTabId: null,
    aiBrowserAvailable: false, rightPanelOpen: true, rightPanelFloating: false, startup: browserFallbackStartup, children: <div>Body</div>,
    onFileContextTabTitleChange: vi.fn(), onConversationMenuAction: vi.fn(), onToggleRightPanel: vi.fn(), onToggleLeftPanel: vi.fn(), onTrace: vi.fn() };
  await render(<WorkspaceResizableBody {...props} />);
  const handle = host.querySelector<HTMLElement>('[role="separator"]')!;
  expect(handle.getAttribute("aria-valuemax")).toBe("869");
  await act(async () => handle.dispatchEvent(new KeyboardEvent("keydown", { key: "End", bubbles: true })));
  expect(handle.getAttribute("aria-valuenow")).toBe("869");
});

it("does not report copied file content when the host clipboard rejects", async () => {
  const workbench = browserFallbackSnapshot();
  workbench.projects = [{ id: "copy-project", name: "Copy project", path: "X:/copy-audit" }];
  workbench.conversations = [{ id: "copy-session", title: "Copy", status: "idle", projectId: "copy-project" }];
  // entry and preview preserve the exact file identity across listing, content loading, and clipboard delivery.
  // entry 和 preview 在目录、正文读取及剪贴板投递之间保留精确文件身份。
  const entry = { name: "code.ts", path: "X:/copy-audit/code.ts", relativePath: "code.ts", kind: "code" as const, isDirectory: false, hidden: false, hasChildren: false };
  const onCopyText = vi.fn().mockRejectedValue(new Error("Clipboard denied"));
  await render(<WorkspaceFilesPanelHost instanceId="copy-audit" selectedDraftConversation={null} selectedConversationId="copy-session" workbench={workbench}
    loadWorkspaceFileDirectory={async () => ({ workspacePath: "X:/copy-audit", path: "X:/copy-audit", relativePath: "", entries: [entry], filters: DEFAULT_WORKSPACE_FILE_FILTERS, truncated: false, entryCount: 1 })}
    loadWorkspaceFilePreview={async () => ({ ...entry, workspacePath: "X:/copy-audit", sizeBytes: 5, content: "hello", truncated: false })}
    resolveMediaSourceUrl={path => path} onCopyText={onCopyText} onTitleChange={vi.fn()} />);
  // menu opens through the same pointer trigger used by the production Radix dropdown.
  // menu 通过生产 Radix 下拉菜单的同一指针触发器打开。
  const menu = host.querySelector<HTMLButtonElement>(".workspace-file-row-menu-trigger")!;
  expect(menu).not.toBeNull();
  await act(async () => menu.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true, button: 0 })));
  const content = Array.from(document.querySelectorAll<HTMLElement>('[role="menuitem"]')).find(item => item.textContent?.includes("复制文件内容"))!;
  expect(content).toBeDefined();
  await act(async () => content.click());
  expect(onCopyText).toHaveBeenCalledExactlyOnceWith("hello");
  expect(toast.success).not.toHaveBeenCalled();
  expect(toast.error).toHaveBeenCalledWith("Clipboard denied");
});
