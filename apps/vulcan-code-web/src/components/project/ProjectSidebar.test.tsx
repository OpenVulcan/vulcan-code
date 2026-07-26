import * as React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { I18nProvider } from "@/i18n";
import { SidebarProvider } from "@/components/ui/sidebar";
import { ProjectSidebar } from "@/components/project/ProjectSidebar";

// noop provides a stable callback for server-rendering navigation-only test props.
// noop 为仅验证导航的服务端渲染属性提供稳定回调。
const noop = () => undefined;

describe("ProjectSidebar", () => {
  // Verifies the real chat workspace sidebar exposes knowledge library management beside memory.
  // 验证真实会话工作台侧栏在记忆旁暴露知识库管理入口。
  it("renders the knowledge management entry", () => {
    // markup captures the same expanded ProjectSidebar tree used by the chat workspace.
    // markup 捕获会话工作台实际使用的展开 ProjectSidebar 树。
    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <SidebarProvider defaultOpen>
          <ProjectSidebar
            agentSkillsActive={false}
            agentsActive={false}
            conversations={[]}
            knowledgeActive={false}
            memoryActive={false}
            onConversationMenuAction={noop}
            onCreateConversation={noop}
            onCreateProject={noop}
            onExpandedProjectIdsChange={noop}
            onOpenAgents={noop}
            onOpenAgentSkills={noop}
            onOpenKnowledge={noop}
            onOpenMemory={noop}
            onOpenSkills={noop}
            onProjectMenuAction={noop}
            onReorderProjects={noop}
            onSelectArea={noop}
            onSelectConversation={noop}
            pinnedConversationIds={new Set()}
            projects={[]}
            selectedConversationId={null}
            skillsActive={false}
          />
        </SidebarProvider>
      </I18nProvider>,
    );

    expect(markup).toContain("知识库");
  });
});
