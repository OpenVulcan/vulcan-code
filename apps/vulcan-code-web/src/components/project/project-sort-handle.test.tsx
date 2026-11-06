// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { SidebarProvider } from "@/ui-kit/shell/sidebar";
import { ProjectSidebar } from "./project-sidebar";
import { setSidebarFlow } from "./sidebar-flow";

// Verifies actual dnd-kit keyboard sorting has a separate activator from both project actions.
// 验证真实 dnd-kit 键盘排序入口与两个项目动作相互独立。
it("sorts with the handle while expansion and new conversation remain independent", async () => {
  // Geometry supplies two vertically ordered rows, which jsdom does not lay out itself.
  // 几何数据提供两个垂直排序行，jsdom 本身不计算布局。
  // Sorting belongs to the code flow's project tree.
  // 排序属于代码流的项目树。
  setSidebarFlow("code");
  const geometry = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const row = this.closest(".vk-tree-project");
    const top = row?.textContent?.includes("Project B") ? 60 : 0;
    return { x: 0, y: top, top, left: 0, bottom: top + 40, right: 240, width: 240, height: 40, toJSON: () => ({}) };
  });
  // Callbacks observe only synthetic project actions, never persistence or filesystem access.
  // 回调仅观察合成项目动作，不执行持久化或文件系统访问。
  const expand = vi.fn(), create = vi.fn(), select = vi.fn();
  const groupAction = vi.fn(async () => undefined);
  const noop = () => undefined;
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  // key sends one real bubbling DOM keyboard event and allows sensor effects to settle.
  // key 发送真实冒泡 DOM 键盘事件，并等待传感器副作用完成。
  const key = async (element: Element, code: string) => {
    await act(async () => { element.dispatchEvent(new KeyboardEvent("keydown", { key: code === "Space" ? " " : code, code, bubbles: true, cancelable: true })); });
    await act(async () => { await new Promise(resolve => setTimeout(resolve, 5)); });
  };
  try {
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><SidebarProvider defaultOpen><ProjectSidebar
      projects={[{ id: "a", name: "Project A", path: "D:/a" }, { id: "b", name: "Project B", path: "D:/b" }]}
      conversations={[]} expandedProjectIds={[]} pinnedConversationIds={new Set()} selectedConversationId={null} activeProjectId={null} rail={null}
      onConversationMenuAction={noop} onCreateConversation={create} onCreateProject={noop} onExpandedProjectIdsChange={expand}
      onProjectMenuAction={noop} onProjectGroupAction={groupAction} onSelectConversation={noop} onSelectProject={select}
    /></SidebarProvider></I18nProvider>));
    const row = container.querySelector<HTMLDivElement>('.vk-tree-item[data-kind="project"]')!;
    const folder = row.querySelector<HTMLButtonElement>(".vk-tree-fold")!;
    const createButton = row.querySelector<HTMLButtonElement>('[data-action="create"]')!;
    const handle = row.querySelector<HTMLButtonElement>(".vk-tree-grip")!;
    expect(row.getAttribute("role")).toBeNull();
    expect(row.getAttribute("tabindex")).toBeNull();
    expect(row.querySelector("button button")).toBeNull();
    expect(handle.getAttribute("aria-label")).toContain("Project A");
    await act(async () => folder.focus());
    await key(folder, "Space");
    // jsdom does not synthesize native button click activation from Space; exercise that boundary explicitly.
    // jsdom 不从空格合成原生按钮 click 激活，因此显式执行该边界。
    await act(async () => folder.click());
    expect(expand).toHaveBeenCalledExactlyOnceWith(["a"]);
    await act(async () => createButton.focus());
    await key(createButton, "Space");
    await act(async () => createButton.click());
    expect(create).toHaveBeenCalledOnce();
    expect(expand).toHaveBeenCalledOnce();
    await act(async () => handle.focus());
    await key(handle, "Space");
    await key(handle, "ArrowDown");
    await key(handle, "Space");
    // Unpinned projects are saved as one complete layout of every group, the default group included.
    // 未置顶项目作为一份包含所有分组（含默认分组）的完整布局保存。
    expect(groupAction).toHaveBeenCalledExactlyOnceWith({ kind: "layout", layout: { groups: [{ groupId: "default", projectIds: ["b", "a"] }] } });
    expect(expand).toHaveBeenCalledOnce();
    expect(create).toHaveBeenCalledOnce();
    expect(select).toHaveBeenCalledOnce();
  } finally { await act(async () => root.unmount()); container.remove(); geometry.mockRestore(); }
});

// A quick press that crosses the activation distance and releases before React renders must not leave
// the tree in sorting mode, which hides every row's hover actions.
// 越过激活距离并在 React 渲染前松开的快速按压，不得让树停留在排序状态并隐藏所有行的悬停操作。
it("leaves sorting mode after a drag that ends before its first render", async () => {
  setSidebarFlow("code");
  const geometry = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(() => (
    { x: 0, y: 0, top: 0, left: 0, bottom: 40, right: 240, width: 240, height: 40, toJSON: () => ({}) }
  ));
  const noop = () => undefined;
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  try {
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><SidebarProvider defaultOpen><ProjectSidebar
      projects={[{ id: "a", name: "Project A", path: "D:/a" }]}
      conversations={[]} expandedProjectIds={[]} pinnedConversationIds={new Set()} selectedConversationId={null} activeProjectId={null} rail={null}
      onConversationMenuAction={noop} onCreateConversation={noop} onCreateProject={noop} onExpandedProjectIdsChange={noop}
      onProjectMenuAction={noop} onProjectGroupAction={async () => undefined} onSelectConversation={noop} onSelectProject={noop}
    /></SidebarProvider></I18nProvider>));
    const handle = container.querySelector<HTMLButtonElement>(".vk-tree-grip")!;
    // pointer sends one bubbling primary pointer-typed event; jsdom lacks PointerEvent.
    // pointer 发送一条冒泡的主指针类型事件；jsdom 没有 PointerEvent。
    const pointer = (target: EventTarget, type: string, clientY: number) => {
      const event = new MouseEvent(type, { bubbles: true, cancelable: true, button: 0, clientX: 10, clientY });
      Object.defineProperty(event, "isPrimary", { value: true });
      target.dispatchEvent(event);
    };
    // Press, move past the activation distance and release inside one batch, before any render commits.
    // 在同一批次内按下、越过激活距离并松开，任何渲染都尚未提交。
    await act(async () => {
      pointer(handle, "pointerdown", 10);
      pointer(document, "pointermove", 30);
      pointer(document, "pointerup", 30);
    });
    await act(async () => { await new Promise(resolve => setTimeout(resolve, 5)); });
    expect(container.querySelector(".vk-project-tree")!.hasAttribute("data-sorting")).toBe(false);
  } finally { await act(async () => root.unmount()); container.remove(); geometry.mockRestore(); }
});
