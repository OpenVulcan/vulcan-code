// @vitest-environment jsdom
import * as React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { createRoot } from "react-dom/client";
import { act } from "react";
import { describe, expect, it, vi } from "vitest";

import { I18nProvider } from "@/i18n";
import { SidebarProvider } from "@/ui-kit/shell/sidebar";
import { ProjectSidebar } from "@/components/project/project-sidebar";

// noop provides a stable callback for server-rendering navigation-only test props.
// noop 为仅验证导航的服务端渲染属性提供稳定回调。
const noop = () => undefined;

describe("ProjectSidebar", () => {
  // Feature areas moved to the activity rail; the session panel renders the rail it is given and only sessions.
  // 功能区已移至活动轨道；会话面板渲染传入的轨道，自身只展示会话。
  it("renders the supplied rail beside sessions and an actionable authoritative empty project directory", async () => {
    Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
    // onCreateProject observes the empty-state action without opening real filesystem UI.
    // onCreateProject 观察空状态动作，不打开真实文件系统界面。
    const onCreateProject = vi.fn();
    // markup captures the same expanded ProjectSidebar tree used by the chat workspace.
    // markup 捕获会话工作台实际使用的展开 ProjectSidebar 树。
    const element = (
      <I18nProvider initialLocale="zh-CN">
        <SidebarProvider defaultOpen>
          <ProjectSidebar
            activeProjectId={null}
            rail={<nav className="test-activity-rail" />}
            conversations={[]}
            onConversationMenuAction={noop}
            onCreateConversation={noop}
            onCreateProject={onCreateProject}
            onExpandedProjectIdsChange={noop}
            onProjectMenuAction={noop}
            onReorderProjects={noop}
            onSelectConversation={noop}
            onSelectProject={noop}
            pinnedConversationIds={new Set()}
            projects={[]}
            selectedConversationId={null}
          />
        </SidebarProvider>
      </I18nProvider>
    );
    const markup = renderToStaticMarkup(element);
    expect(markup).toContain("test-activity-rail");
    expect(markup).not.toContain("知识库");
    expect(markup).toContain("添加文件夹，开始一个项目");
    expect(markup).not.toContain("等待 Workspace 快照载入");
    // root exercises the visible empty-state button's real event path.
    // root 验证可见空状态按钮的真实事件路径。
    const container = document.createElement("div");
    const root = createRoot(container);
    document.body.append(container);
    try {
      await act(async () => root.render(element));
      await act(async () => Array.from(container.querySelectorAll("button")).find(button => button.textContent?.includes("添加文件夹，开始一个项目"))?.click());
      expect(onCreateProject).toHaveBeenCalledOnce();
    } finally { await act(async () => root.unmount()); container.remove(); }
  });

  // StrictMode project expansion must notify the parent from the event boundary, not a state updater.
  // StrictMode 下项目展开必须在事件边界通知父组件，不能在状态 updater 中更新父组件。
  it("keeps rapid expansion toggles warning-free and persists each resulting state once", async () => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    const root = createRoot(container);
    // onExpandedProjectIdsChange records the parent persistence callback under test.
    // onExpandedProjectIdsChange 记录本次测试中的父组件持久化回调。
    const onExpandedProjectIdsChange = vi.fn();
    // consoleError captures React render warnings for the regression assertion.
    // consoleError 捕获 React 渲染警告，供回归断言使用。
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => undefined);
    // project is the minimal visible folder fixture used by the controlled sidebar.
    // project 是受控侧边栏使用的最小可见文件夹测试数据。
    const project = { id: "project-a", name: "Project A", path: "D:/project-a" };
    // StatefulProjectSidebar is a controlled parent harness for real parent state updates.
    // StatefulProjectSidebar 是用于真实父状态更新的受控父组件测试壳。
    // Parameters: externalExpandedProjectIds optionally replaces the parent's controlled expansion.
    // 参数：externalExpandedProjectIds 可选替换父组件受控的展开项目。
    // Returns: the production ProjectSidebar rendered beneath the stateful parent.
    // 返回值：状态父组件下渲染的生产 ProjectSidebar。
    function StatefulProjectSidebar(props: { externalExpandedProjectIds?: string[] }) {
      // expandedProjectIds stores the expansion state owned by the parent harness.
      // expandedProjectIds 保存父组件测试壳持有的展开状态。
      const [expandedProjectIds, setExpandedProjectIds] = React.useState<string[]>([]);
      // This effect applies an externally supplied parent update to the controlled state.
      // 此副作用将外部传入的父组件更新应用到受控状态。
      React.useEffect(() => {
        if (props.externalExpandedProjectIds) {
          setExpandedProjectIds(props.externalExpandedProjectIds);
        }
      }, [props.externalExpandedProjectIds]);
      // This parent callback records and applies each event result to real parent state.
      // 此父回调记录每个事件结果并将其应用到真实父状态。
      return (
        <ProjectSidebar
          activeProjectId={null}
          rail={<nav className="test-activity-rail" />}
          conversations={[]}
          expandedProjectIds={expandedProjectIds}
          onConversationMenuAction={noop}
          onCreateConversation={noop}
          onCreateProject={noop}
          onExpandedProjectIdsChange={(next) => {
            onExpandedProjectIdsChange(next);
            setExpandedProjectIds(next);
          }}
          onProjectMenuAction={noop}
          onReorderProjects={noop}
          onSelectConversation={noop}
          onSelectProject={noop}
          pinnedConversationIds={new Set()}
          projects={[project]}
          selectedConversationId={null}
        />
      );
    }
    // render mounts the StrictMode parent harness with an optional external expansion projection.
    // render 在 StrictMode 下挂载父组件测试壳及可选外部展开投影。
    // Parameters: externalExpandedProjectIds optionally supplies a parent-side expansion update.
    // 参数：externalExpandedProjectIds 可选提供父组件侧展开更新。
    // Returns: the mounted test tree for the React root.
    // 返回值：供 React 根节点挂载的测试树。
    const render = (externalExpandedProjectIds?: string[]) => (
      <React.StrictMode>
        <I18nProvider initialLocale="zh-CN">
          <SidebarProvider defaultOpen>
            <StatefulProjectSidebar externalExpandedProjectIds={externalExpandedProjectIds} />
          </SidebarProvider>
        </I18nProvider>
      </React.StrictMode>
    );

    try {
      await act(async () => { root.render(render()); });
      const button = container.querySelector<HTMLButtonElement>(".project-folder-button");
      expect(button).not.toBeNull();
      await act(async () => { button?.click(); button?.click(); });
      expect(onExpandedProjectIdsChange).toHaveBeenNthCalledWith(1, ["project-a"]);
      expect(onExpandedProjectIdsChange).toHaveBeenNthCalledWith(2, []);
      expect(onExpandedProjectIdsChange).toHaveBeenCalledTimes(2);
      await act(async () => { root.render(render(["project-a"])); });
      await act(async () => { button?.click(); });
      expect(onExpandedProjectIdsChange).toHaveBeenCalledTimes(3);
      expect(onExpandedProjectIdsChange).toHaveBeenNthCalledWith(3, []);
      expect(consoleError.mock.calls.flat().join(" ")).not.toContain("Cannot update a component");
    } finally {
      await act(async () => { root.unmount(); });
      consoleError.mockRestore();
      container.remove();
    }
  });
});
