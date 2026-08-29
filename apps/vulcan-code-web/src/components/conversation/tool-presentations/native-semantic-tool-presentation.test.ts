import { describe, expect, it } from "vitest";
import type { ToolExecutionConversationBlock } from "@/components/conversation/conversation-tool-execution-types";
import { resolveToolPresentation } from "@/components/conversation/conversation-tool-presentations";
import {
  resolveGenericToolMarkdownPresentation,
  resolveNativeSemanticToolPresentation,
} from "./native-semantic-tool-presentation";

describe("native semantic tool presentation", () => {
  // nativeToolCases lock every native identity to its exact business title.
  // nativeToolCases 将每个原生工具身份锁定到精确业务标题。
  const nativeToolCases = [
    ["todolist", "toolPresentation.native.todolist"],
    ["get_goal", "toolPresentation.native.getGoal"],
    ["create_goal", "toolPresentation.native.createGoal"],
    ["update_goal", "toolPresentation.native.updateGoal"],
    ["knowledge_search", "toolPresentation.native.knowledgeSearch"],
    ["knowledge_browse", "toolPresentation.native.knowledgeBrowse"],
    ["knowledge_read", "toolPresentation.native.knowledgeRead"],
    ["skill", "toolPresentation.native.skill"],
    ["vmm_memory_search", "toolPresentation.native.memorySearch"],
    ["vmm_turn_details", "toolPresentation.native.turnDetails"],
    ["vmm_memory_write", "toolPresentation.native.memoryWrite"],
    ["vmm_memory_delete", "toolPresentation.native.memoryDelete"],
    ["runtime-config", "toolPresentation.native.runtimeConfig"],
  ] as const;

  it.each(nativeToolCases)("resolves %s to its semantic business card", (toolName, titleKey) => {
    // blocks contains only the authoritative tool identity because arguments may still be streaming.
    // blocks 仅包含权威工具身份，因为参数此时仍可能处于流式构造中。
    const blocks = [{
      body: "",
      id: `${toolName}-request`,
      kind: "tool-call",
      summary: toolName,
      title: toolName,
      toolCallId: `${toolName}-call`,
      toolName,
    }] as ToolExecutionConversationBlock[];

    expect(resolveNativeSemanticToolPresentation(blocks, "running")).toMatchObject({
      kind: "native_semantic",
      titleKey,
    });
  });

  it("resolves the exact todolist id to a semantic TODO card", () => {
    const blocks = [{
      body: "",
      id: "todo-call",
      kind: "tool-call",
      rawMetadata: {
        arguments: { items: [{ status: "in_progress", step: "Implement cards" }] },
      },
      summary: "todolist",
      title: "todolist",
      toolCallId: "todo-call-1",
      toolName: "todolist",
    }] as ToolExecutionConversationBlock[];

    expect(resolveNativeSemanticToolPresentation(blocks, "running")).toMatchObject({
      family: "todo",
      kind: "native_semantic",
      titleKey: "toolPresentation.native.todolist",
      toolId: "todolist",
    });
  });

  it("normalizes the conditional runtime-config id without broad matching", () => {
    const blocks = [{
      body: "",
      id: "runtime-call",
      kind: "tool-call",
      summary: "runtime-config",
      title: "runtime-config",
      toolCallId: "runtime-call-1",
      toolName: "runtime-config",
    }] as ToolExecutionConversationBlock[];

    expect(resolveNativeSemanticToolPresentation(blocks, "running")).toMatchObject({
      kind: "native_semantic",
      toolId: "runtime_config",
    });
  });

  it("renders unknown tool output inline and preserves its raw name", () => {
    const blocks = [{
      body: "",
      id: "unknown-call",
      kind: "tool-call",
      summary: "custom_widget",
      title: "custom_widget",
      toolCallId: "unknown-call-1",
      toolName: "custom_widget",
    }, {
      body: "## Result\n\nDone",
      id: "unknown-result",
      kind: "tool-result",
      summary: "",
      title: "",
      toolCallId: "unknown-call-1",
      toolName: "custom_widget",
    }] as ToolExecutionConversationBlock[];

    expect(resolveGenericToolMarkdownPresentation(blocks, "completed")).toMatchObject({
      kind: "generic_tool_markdown",
      markdown: "## Result\n\nDone",
      toolName: "custom_widget",
    });
  });

  it("keeps known file and ask identities on their semantic cards before arguments arrive", () => {
    const fileBlocks = [{
      body: "",
      id: "file-request",
      kind: "tool-call",
      summary: "file",
      title: "file",
      toolCallId: "file-call-1",
      toolName: "file",
    }] as ToolExecutionConversationBlock[];
    const askBlocks = [{
      body: "",
      id: "ask-request",
      kind: "tool-call",
      summary: "ask",
      title: "ask",
      toolCallId: "ask-call-1",
      toolName: "ask",
    }] as ToolExecutionConversationBlock[];

    expect(resolveToolPresentation(fileBlocks, "running").kind).toBe("file_change");
    expect(resolveToolPresentation(askBlocks, "running").kind).toBe("ask_question");
  });
});
