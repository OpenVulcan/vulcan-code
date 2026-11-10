// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { browserFallbackSnapshot } from "@/service/browser-fallback-workbench";
import type { ProjectSummary, WorkbenchSnapshot } from "@/types";
import { useWorkbenchProjectActions } from "./project-actions";

// services isolates the project-add responses and keeps the real snapshot shape.
// services 隔离项目添加响应，并保留真实快照结构。
const services = vi.hoisted(() => ({ add: vi.fn(), addToGroup: vi.fn() }));
vi.mock("@/service/project-actions-service", async (original) => ({
  ...await original<object>(),
  addProjectFromPath: services.add,
  addProjectToGroup: services.addToGroup,
}));
vi.mock("@/ui-kit/primitives/toast", () => ({ toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn() } }));

// ADDED_PROJECT is the folder the picker returns; its stored path ends with a separator the picker omits.
// ADDED_PROJECT 是选择器返回的文件夹；其存储路径以选择器省略的末尾分隔符结束。
const ADDED_PROJECT: ProjectSummary = { id: "added-project", name: "Added", path: "D:/work/added/" };

// Mounted captures one real hook instance with the side effects this test observes.
// Mounted 捕获一个真实钩子实例及本测试观察的副作用。
interface Mounted {
  actions: () => ReturnType<typeof useWorkbenchProjectActions>;
  drafts: ReturnType<typeof vi.fn>;
  selected: ReturnType<typeof vi.fn>;
  pickerOpen: ReturnType<typeof vi.fn>;
  accept: ReturnType<typeof vi.fn>;
  unmount: () => Promise<void>;
}

// mountHarness renders the hook over a snapshot that already contains the added project.
// mountHarness 在已包含新增项目的快照上渲染钩子。
async function mountHarness(): Promise<Mounted> {
  const base = browserFallbackSnapshot();
  const snapshot: WorkbenchSnapshot = { ...base, projects: [...base.projects, ADDED_PROJECT] };
  services.add.mockResolvedValue(snapshot);
  services.addToGroup.mockResolvedValue(snapshot);
  const holder: { actions?: ReturnType<typeof useWorkbenchProjectActions> } = {};
  const drafts = vi.fn();
  const selected = vi.fn();
  const pickerOpen = vi.fn();
  const accept = vi.fn(() => true);
  const options: Parameters<typeof useWorkbenchProjectActions>[0] = {
    workbenchState: base,
    setWorkbenchState: vi.fn(),
    mergeWorkbenchWithLocalCaches: (_local, incoming) => incoming,
    acceptServiceWorkbenchSnapshot: accept,
    applyLocalPreviewWithLocalCaches: vi.fn(),
    setDraftConversation: drafts,
    setSelectedConversationId: selected,
    setRenameTarget: vi.fn(),
    setProjectIconTarget: vi.fn(),
    openProjectHome: vi.fn(),
    setProjectFolderPickerOpen: pickerOpen,
    setArchiveProjectTarget: vi.fn(),
    setRemoveProjectTarget: vi.fn(),
    expandedProjectIdsLocallyEditedRef: { current: false },
    applySkillWorkbenchAction: vi.fn(),
    applyLuaSkillsCatalogUpdate: vi.fn(),
    applyAgentSkillsCatalogUpdate: vi.fn(),
    applySubagentCatalogUpdate: vi.fn(),
    handleOpenWorkspaceChat: vi.fn(),
  };
  // Component owns the hook lifetime and exposes the latest callbacks to the test.
  // Component 持有钩子生命周期，并向测试暴露最新回调。
  function Component() {
    holder.actions = useWorkbenchProjectActions(options);
    return null;
  }
  const container = document.createElement("div");
  const root = createRoot(container);
  await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><Component /></I18nProvider>));
  return {
    actions: () => holder.actions!,
    drafts,
    selected,
    pickerOpen,
    accept,
    unmount: async () => { await act(async () => root.unmount()); },
  };
}

it("opens a new conversation in the project added from the new-project action", async () => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  const mounted = await mountHarness();
  try {
    await act(async () => mounted.actions().handleCreateProject({ startConversation: true }));
    expect(mounted.pickerOpen).toHaveBeenLastCalledWith(true);
    await act(async () => mounted.actions().handleConfirmProjectFolder("d:/work/added"));
    expect(services.add).toHaveBeenCalledWith("d:/work/added");
    expect(mounted.drafts).toHaveBeenCalledWith(expect.objectContaining({
      projectId: ADDED_PROJECT.id,
      projectName: ADDED_PROJECT.name,
    }));
    expect(mounted.selected).toHaveBeenCalledWith(expect.stringContaining(`draft-${ADDED_PROJECT.id}-`));
  } finally { await mounted.unmount(); }
});

it("adds a folder chosen for a group without opening a conversation", async () => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  const mounted = await mountHarness();
  try {
    await act(async () => mounted.actions().handleCreateProject({ groupId: "group-a" }));
    await act(async () => mounted.actions().handleConfirmProjectFolder("D:/work/added"));
    expect(services.addToGroup).toHaveBeenCalledWith("D:/work/added", "group-a");
    // Applying the snapshot clears any draft; only a created draft would carry a selection.
    // 应用快照会清空草稿；只有新建草稿才会携带选择对象。
    expect(mounted.drafts.mock.calls.filter(([value]) => value !== null)).toEqual([]);
  } finally { await mounted.unmount(); }
});

it("starts no conversation when the add response is rejected as stale", async () => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  const mounted = await mountHarness();
  try {
    mounted.accept.mockReturnValue(false);
    await act(async () => mounted.actions().handleCreateProject({ startConversation: true }));
    await act(async () => mounted.actions().handleConfirmProjectFolder("D:/work/added"));
    expect(mounted.drafts).not.toHaveBeenCalled();
  } finally { await mounted.unmount(); }
});
