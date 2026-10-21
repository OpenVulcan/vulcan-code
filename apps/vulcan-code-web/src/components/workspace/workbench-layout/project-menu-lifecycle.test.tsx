// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { toast } from "@/ui-kit/primitives/toast";
import { I18nProvider } from "@/i18n";
import { browserFallbackSnapshot } from "@/service/browser-fallback-workbench";
import type { ProjectSummary, WorkbenchSnapshot } from "@/types";
import { useWorkbenchProjectActions } from "./project-actions";

// services isolates acknowledged mutations and delayed authoritative reads from real configuration.
// services 将已确认变更和延迟权威读取与真实配置隔离。
const services = vi.hoisted(() => ({ toggle: vi.fn(), open: vi.fn(), read: vi.fn(), expand: vi.fn() }));
vi.mock("@/service/project-actions-service", async (original) => ({ ...await original<object>(), toggleProjectPinned: services.toggle, openProjectDirectory: services.open, setExpandedTaskProjects: services.expand }));
vi.mock("@/service/workbench-snapshot-service", async (original) => ({ ...await original<object>(), loadWorkbenchSnapshot: services.read }));
vi.mock("@/ui-kit/primitives/toast", () => ({ toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn() } }));

// Verify unrelated failures, duplicate toggles and rejected stale reads never restore an old workbench.
// 验证无关动作失败、重复切换和被拒绝的旧读取都不会恢复旧工作台。
it("keeps current state and drafts across directory failures and stale pin reads", async () => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  // current is a test-owned projection; updates model independent workspace events during a request.
  // current 是测试持有的投影；更新模拟请求期间独立的工作台事件。
  let current = browserFallbackSnapshot();
  // project supplies the exact ID used by the void toggle protocol.
  // project 提供无返回值切换协议使用的准确标识。
  const project: ProjectSummary = { id: "pin-a", name: "A", path: "D:/isolated-a", pinned: false };
  current = { ...current, projects: [project] };
  // actions captures the real hook callbacks without rendering unrelated workspace surfaces.
  // actions 捕获真实钩子回调，不渲染无关工作台区域。
  let actions!: ReturnType<typeof useWorkbenchProjectActions>;
  // drafts and selected observe forbidden side effects from menu operations.
  // drafts 和 selected 观察菜单操作不应触发的副作用。
  const drafts = vi.fn();
  const selected = vi.fn();
  // accept stands for the host's monotonic revision gate, including rejection of a late response.
  // accept 表示宿主的单调版本门禁，包括拒绝迟到响应。
  const accept = vi.fn(() => false);
  // merge must receive the latest local state when an authoritative response is accepted.
  // merge 在权威响应被接纳时必须收到最新本地状态。
  const merge = vi.fn((local: WorkbenchSnapshot, incoming: WorkbenchSnapshot) => ({ ...incoming, conversations: local.conversations }));
  // options wires exact hook dependencies to local state and side-effect observers.
  // options 将准确钩子依赖连接到本地状态和副作用观察器。
  const options: Parameters<typeof useWorkbenchProjectActions>[0] = {
    workbenchState: current, setWorkbenchState: (update) => { current = typeof update === "function" ? update(current) : update; },
    mergeWorkbenchWithLocalCaches: merge, acceptServiceWorkbenchSnapshot: accept,
    applyLocalPreviewWithLocalCaches: vi.fn(), setDraftConversation: drafts, setSelectedConversationId: selected,
    setRenameTarget: vi.fn(), setProjectIconTarget: vi.fn(), setProjectSettingsTarget: vi.fn(), setProjectFolderPickerOpen: vi.fn(),
    setArchiveProjectTarget: vi.fn(), setRemoveProjectTarget: vi.fn(), expandedProjectIdsLocallyEditedRef: { current: false },
    applySkillWorkbenchAction: vi.fn(), applyLuaSkillsCatalogUpdate: vi.fn(), applyAgentSkillsCatalogUpdate: vi.fn(),
    applySubagentCatalogUpdate: vi.fn(), handleOpenWorkspaceChat: vi.fn(),
  };
  // Harness owns the hook lifetime and returns no interaction substitutes.
  // Harness 持有钩子生命周期，不提供交互替代实现。
  function Harness() { actions = useWorkbenchProjectActions(options); return null; }
  // container and root host only this isolated hook instance.
  // container 和 root 仅承载本隔离钩子实例。
  const container = document.createElement("div");
  const root = createRoot(container);
  try {
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><Harness /></I18nProvider>));
    services.open.mockRejectedValue(new Error("directory denied"));
    // fresh records an independent project event before the old callback settles.
    // fresh 记录旧回调结束前发生的独立项目事件。
    const fresh = { ...current, projects: [...current.projects, { id: "pin-b", name: "B", path: "D:/isolated-b" }] };
    current = fresh;
    await actions.handleProjectMenuAction("open-directory", project);
    expect(current).toBe(fresh);
    // finishRead holds the first snapshot until duplicate submission and independent updates occur.
    // finishRead 将首个快照挂起，等待重复提交和独立更新发生。
    let finishRead!: (snapshot: WorkbenchSnapshot) => void;
    services.toggle.mockResolvedValue(undefined);
    services.read.mockReturnValueOnce(new Promise<WorkbenchSnapshot>((resolve) => { finishRead = resolve; }));
    // request captures the full toggle-and-read lifetime.
    // request 捕获切换及读取的完整生命周期。
    const request = actions.handleProjectMenuAction("toggle-pin", project);
    await Promise.resolve();
    await actions.handleProjectMenuAction("toggle-pin", project);
    expect(services.toggle).toHaveBeenCalledTimes(1);
    finishRead(options.workbenchState);
    await request;
    expect(accept).toHaveBeenCalledOnce();
    expect(merge).not.toHaveBeenCalled();
    expect(current).toBe(fresh);
    accept.mockReturnValue(true);
    services.read.mockResolvedValueOnce({ ...fresh, projects: fresh.projects.map((entry) => entry.id === project.id ? { ...entry, pinned: true } : entry) });
    await actions.handleProjectMenuAction("toggle-pin", project);
    expect(merge.mock.calls[0][0]).toBe(fresh);
    expect(current.projects.find((entry) => entry.id === "pin-b")?.name).toBe("B");
    expect(current.projects[0].pinned).toBe(true);
    const ordered = current;
    services.expand.mockResolvedValueOnce(null);
    await act(async () => actions.handleExpandedProjectIdsChange(["pin-a"]));
    expect(current.expandedProjectIds).toEqual(["pin-a"]);
    expect(current.projects).toBe(ordered.projects);
    expect(toast.warning).toHaveBeenLastCalledWith(expect.stringContaining("当前展开视图已保留"), expect.any(Object));
    expect(drafts).not.toHaveBeenCalled();
    expect(selected).not.toHaveBeenCalled();
  } finally { await act(async () => root.unmount()); }
});
