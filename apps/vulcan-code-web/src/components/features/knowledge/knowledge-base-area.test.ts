import { describe, expect, it } from "vitest";

import type { KnowledgeDisplayIndexState } from "@/types";
import { knowledgeDisplayBuildAction } from "./KnowledgeBaseArea";

// displayIndex creates one durable display lifecycle fixture for button-action tests.
// displayIndex 为按钮动作测试创建一份持久化显示库生命周期夹具。
// Parameters: overrides replaces exact lifecycle fields. Returns: a complete display-index state.
// 参数：overrides 替换精确生命周期字段。返回值：完整显示库索引状态。
function displayIndex(overrides: Partial<KnowledgeDisplayIndexState>): KnowledgeDisplayIndexState {
  return {
    status: "not_created",
    activeGeneration: null,
    pendingGeneration: null,
    generatedDocumentCount: 0,
    ...overrides,
  };
}

describe("knowledgeDisplayBuildAction", () => {
  it("opens progress instead of starting a duplicate build while generation is running", () => {
    expect(knowledgeDisplayBuildAction(displayIndex({ status: "building", pendingGeneration: "pending-a" }))).toEqual({
      mode: "progress",
      label: "knowledge.experience.viewProgress",
      disabled: false,
    });
  });

  it("offers continuation when a failed or interrupted generation was retained", () => {
    expect(knowledgeDisplayBuildAction(displayIndex({ status: "failed", pendingGeneration: "pending-a" }))).toEqual({
      mode: "resume",
      label: "knowledge.experience.resume",
      disabled: false,
    });
  });

  it("disables generation after a published display corpus is complete", () => {
    expect(knowledgeDisplayBuildAction(displayIndex({ status: "ready", activeGeneration: "active-a" }))).toEqual({
      mode: "complete",
      label: "knowledge.experience.displayReady",
      disabled: true,
    });
  });

  it("offers rebuild when the published generation became stale", () => {
    expect(knowledgeDisplayBuildAction(displayIndex({ status: "stale", activeGeneration: "active-a" }))).toEqual({
      mode: "start",
      label: "knowledge.experience.regenerate",
      disabled: false,
    });
  });

  // A per-run omission must remain resumable even when all selected work was published successfully.
  // 即使已选工作全部发布成功，仅本次未选的文件仍必须支持继续补充。
  it("offers continuation for files unchecked only for the completed run", () => {
    expect(knowledgeDisplayBuildAction(displayIndex({
      status: "ready",
      activeGeneration: "active-a",
      skippedDocuments: [{ sourceDocumentId: "later", relativePath: "later.md", byteSize: 12, byteLimit: 65536, reason: "not_selected" }],
    }))).toEqual({ mode: "resume", label: "knowledge.experience.resume", disabled: false });
  });
});
