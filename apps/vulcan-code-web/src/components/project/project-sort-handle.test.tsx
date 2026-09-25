// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { SidebarProvider } from "@/components/ui/sidebar";
import { ProjectSidebar } from "./project-sidebar";

// Verifies actual dnd-kit keyboard sorting has a separate activator from both project actions.
// 验证真实 dnd-kit 键盘排序入口与两个项目动作相互独立。
it("sorts with the handle while expansion and new conversation remain independent", async () => {
  // Geometry supplies two vertically ordered rows, which jsdom does not lay out itself.
  // 几何数据提供两个垂直排序行，jsdom 本身不计算布局。
  const geometry = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const row = this.closest(".project-tree-item");
    const top = row?.textContent?.includes("Project B") ? 60 : 0;
    return { x: 0, y: top, top, left: 0, bottom: top + 40, right: 240, width: 240, height: 40, toJSON: () => ({}) };
  });
  // Callbacks observe only synthetic project actions, never persistence or filesystem access.
  // 回调仅观察合成项目动作，不执行持久化或文件系统访问。
  const reorder = vi.fn(), expand = vi.fn(), create = vi.fn(), select = vi.fn();
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
      conversations={[]} expandedProjectIds={[]} pinnedConversationIds={new Set()} selectedConversationId={null} activeProjectId={null}
      onConversationMenuAction={noop} onCreateConversation={create} onCreateProject={noop} onExpandedProjectIdsChange={expand}
      onOpenAgents={noop} onOpenAgentSkills={noop} onOpenKnowledge={noop} onOpenMemory={noop} onOpenSkills={noop}
      onProjectMenuAction={noop} onReorderProjects={reorder} onSelectArea={noop} onSelectConversation={noop} onSelectProject={select}
      skillsActive={false} agentSkillsActive={false} agentsActive={false} memoryActive={false} knowledgeActive={false}
    /></SidebarProvider></I18nProvider>));
    const row = container.querySelector<HTMLDivElement>(".project-folder-row")!;
    const folder = row.querySelector<HTMLButtonElement>(".project-folder-button")!;
    const createButton = row.querySelector<HTMLButtonElement>(".project-create-conversation")!;
    const handle = row.querySelector<HTMLButtonElement>(".project-reorder-handle")!;
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
    expect(reorder).not.toHaveBeenCalled();
    await act(async () => createButton.focus());
    await key(createButton, "Space");
    await act(async () => createButton.click());
    expect(create).toHaveBeenCalledOnce();
    expect(expand).toHaveBeenCalledOnce();
    expect(reorder).not.toHaveBeenCalled();
    await act(async () => handle.focus());
    await key(handle, "Space");
    await key(handle, "ArrowDown");
    await key(handle, "Space");
    expect(reorder).toHaveBeenCalledExactlyOnceWith(["b", "a"]);
    expect(expand).toHaveBeenCalledOnce();
    expect(create).toHaveBeenCalledOnce();
    expect(select).toHaveBeenCalledOnce();
  } finally { await act(async () => root.unmount()); container.remove(); geometry.mockRestore(); }
});
