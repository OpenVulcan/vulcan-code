// @vitest-environment jsdom
import { act, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { browserFallbackSnapshot } from "@/service/browser-fallback-workbench";
import type { ProjectSummary, WorkbenchSnapshot } from "@/types";
import { useWorkbenchProjectActions } from "./project-actions";
import { useWorkbenchRevisionGates } from "./use-workbench-revision-sync";

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
  // The add RPC receipt and the service broadcast share this revision in the duplicate-delivery case.
  // 在重复投递场景中，添加 RPC 回执与服务广播使用这个相同版本。
  receipt: WorkbenchSnapshot;
  // Applies a broadcast through the same revision gate used by the add RPC.
  // 通过添加 RPC 共用的版本闸门应用广播。
  applyBroadcast: (snapshot: WorkbenchSnapshot) => boolean;
  unmount: () => Promise<void>;
}

// mountHarness renders the project actions with real workbench state and revision gates, and returns their test controls.
// mountHarness 用真实的工作台状态和版本闸门渲染项目操作，并返回对应的测试控件。
async function mountHarness(): Promise<Mounted> {
  // The receipt advances the revision and adds the chosen project to the initial workbench.
  // 回执推进版本，并将选中的项目加入初始工作台。
  const base = { ...browserFallbackSnapshot(), revision: 1 };
  // The RPC fixture is also usable as the matching full service broadcast.
  // RPC 测试数据也可作为与之匹配的服务全量广播。
  const receipt: WorkbenchSnapshot = { ...base, revision: 2, projects: [...base.projects, ADDED_PROJECT] };
  services.add.mockResolvedValue(receipt);
  services.addToGroup.mockResolvedValue(receipt);
  // Keep the latest callbacks from the mounted React component.
  // 保存已挂载 React 组件的最新回调。
  const holder: Partial<Pick<Mounted, "applyBroadcast">> & { actions?: ReturnType<typeof useWorkbenchProjectActions> } = {};
  const drafts = vi.fn();
  const selected = vi.fn();
  const pickerOpen = vi.fn();
  // The harness supplies real state and gates when mounting the shared action options.
  // 挂载共享操作选项时，测试组件提供真实状态和版本闸门。
  const options: Omit<Parameters<typeof useWorkbenchProjectActions>[0], "workbenchState" | "setWorkbenchState" | "acceptServiceWorkbenchSnapshot"> = {
    mergeWorkbenchWithLocalCaches: (_local, incoming) => incoming,
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
    // Use the workbench's real state and revision acceptance for each delivery.
    // 为每次投递使用工作台的真实状态与版本接纳规则。
    const [workbenchState, setWorkbenchState] = useState<WorkbenchSnapshot>(base);
    // Keep the latest rendered snapshot available to the revision gates.
    // 为版本闸门提供最新渲染的快照。
    const workbenchStateRef = useRef(workbenchState);
    workbenchStateRef.current = workbenchState;
    // Both the full broadcast and the RPC response use the production revision gate.
    // 全量广播和 RPC 回执共用生产版本闸门。
    const { acceptServiceWorkbenchSnapshot } = useWorkbenchRevisionGates({ serviceInstanceEpoch: 1, workbenchState, setWorkbenchState, workbenchStateRef });
    holder.applyBroadcast = (snapshot) => {
      if (!acceptServiceWorkbenchSnapshot(snapshot)) return false;
      setWorkbenchState(snapshot);
      return true;
    };
    holder.actions = useWorkbenchProjectActions({ ...options, workbenchState, setWorkbenchState, acceptServiceWorkbenchSnapshot });
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
    receipt,
    applyBroadcast: (snapshot) => holder.applyBroadcast!(snapshot),
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

it("opens the added project when its broadcast arrives before the matching RPC receipt", async () => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  // Mount the real action and revision hooks before ordering the two deliveries.
  // 挂载真实操作与版本钩子，再安排两次投递的顺序。
  const mounted = await mountHarness();
  try {
    services.add.mockImplementationOnce(async () => {
      expect(mounted.applyBroadcast(mounted.receipt)).toBe(true);
      return mounted.receipt;
    });
    await act(async () => mounted.actions().handleCreateProject({ startConversation: true }));
    await act(async () => mounted.actions().handleConfirmProjectFolder("D:/work/added"));
    expect(mounted.drafts).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ projectId: ADDED_PROJECT.id }));
  } finally { await mounted.unmount(); }
});

it("does not reopen a project absent from a newer broadcast", async () => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  // The newer workbench simulates the project being removed before its add receipt arrives.
  // 更新的工作台模拟项目在添加回执到达前已被移除。
  const mounted = await mountHarness();
  try {
    await act(async () => {
      expect(mounted.applyBroadcast({ ...browserFallbackSnapshot(), revision: (mounted.receipt.revision ?? 0) + 1 })).toBe(true);
    });
    await act(async () => mounted.actions().handleCreateProject({ startConversation: true }));
    await act(async () => mounted.actions().handleConfirmProjectFolder("D:/work/added"));
    expect(mounted.drafts).not.toHaveBeenCalled();
  } finally { await mounted.unmount(); }
});
