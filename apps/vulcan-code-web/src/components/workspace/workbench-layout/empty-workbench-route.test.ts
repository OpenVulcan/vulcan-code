import { describe, expect, it } from "vitest";
import type { DraftConversationSelection } from "@/components/workspace/workspace-resizable-body";
import { resolveEmptyWorkbenchDraftRoute } from "@/components/workspace/workbench-layout/empty-workbench-route";
import type { ProjectSummary } from "@/types";

// PROJECTS is the ordered active project fixture shared by empty-workbench routing tests.
// PROJECTS 是空工作台路由测试共享的有序活动项目夹具。
const PROJECTS: ProjectSummary[] = [
  { id: "project-1", name: "Project 1", path: "D:/project-1" },
  { id: "project-2", name: "Project 2", path: "D:/project-2" },
];

describe("empty workbench draft routing", () => {
  it("opens a new conversation in the last active project after every session is archived", () => {
    // route is the coherent empty-session destination for the last active project.
    // route 是全部会话归档后最后活动项目对应的一致目标路由。
    const route = resolveEmptyWorkbenchDraftRoute(
      { conversations: [], projects: PROJECTS },
      "project-2",
      null,
    );

    expect(route).toEqual({
      draftConversation: {
        id: expect.stringMatching(/^draft-project-2-\d+$/),
        projectId: "project-2",
        projectName: "Project 2",
      },
      selectedConversationId: expect.stringMatching(/^draft-project-2-\d+$/),
    });
    expect(route?.selectedConversationId).toBe(route?.draftConversation.id);
  });

  it("selects the existing project draft after a stale archived selection is cleared", () => {
    // currentDraft is the already-created new-conversation surface for the active project.
    // currentDraft 是已为活动项目创建的新建会话界面。
    const currentDraft: DraftConversationSelection = {
      id: "draft-project-2-existing",
      projectId: "project-2",
      projectName: "Project 2",
    };
    // route must reuse the same draft identity so unsent local state remains addressable.
    // route 必须复用同一草稿身份，使未发送的本地状态仍可寻址。
    const route = resolveEmptyWorkbenchDraftRoute(
      { conversations: [], projects: PROJECTS },
      "project-2",
      currentDraft,
    );

    expect(route).toEqual({
      draftConversation: currentDraft,
      selectedConversationId: currentDraft.id,
    });
    expect(route?.draftConversation).toBe(currentDraft);
  });

  it("does not replace a still-active conversation with a draft route", () => {
    expect(resolveEmptyWorkbenchDraftRoute(
      {
        conversations: [{ id: "conversation-1", title: "Conversation 1", status: "idle", projectId: "project-1" }],
        projects: PROJECTS,
      },
      "project-1",
      null,
    )).toBeNull();
  });

  it("opens a new conversation when only assistant sessions remain active", () => {
    // route treats floating assistants as absent from the main conversation surface.
    // route 将浮动助手视为不属于主会话界面。
    const route = resolveEmptyWorkbenchDraftRoute(
      {
        conversations: [{
          id: "assistant-1",
          title: "Assistant 1",
          status: "idle",
          projectId: "project-1",
          dedicated: true,
          assistantName: "Helper",
        }],
        projects: PROJECTS,
      },
      "project-1",
      null,
    );

    expect(route?.draftConversation.projectId).toBe("project-1");
    expect(route?.selectedConversationId).toBe(route?.draftConversation.id);
  });
});
