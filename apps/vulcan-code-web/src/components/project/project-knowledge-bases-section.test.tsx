// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ProjectKnowledgeBasesSection } from "./project-knowledge-bases-section";
import type { KnowledgeLibrarySummary } from "@/types";

// services isolates authoritative reads and writes from real project configuration.
// services 将权威读写与真实项目配置隔离。
const services = vi.hoisted(() => ({ libraries: vi.fn(), bindings: vi.fn(), save: vi.fn(), notify: vi.fn(), success: vi.fn(), error: vi.fn() }));
vi.mock("@/service/knowledge-base-service", () => ({ listKnowledgeLibraries: services.libraries, listProjectKnowledgeBindings: services.bindings, setKnowledgeProjectBinding: services.save, notifyKnowledgeProjectBindingsChanged: services.notify }));
vi.mock("sonner", () => ({ toast: { success: services.success, error: services.error } }));
vi.mock("@/i18n", () => ({ useI18n: () => ({ t: translate }) }));
// translate keeps the callback identity stable while exposing exact message keys.
// translate 保持回调身份稳定并暴露精确文案键。
function translate(key: string): string { return key; }
// library is a complete synthetic summary following the current knowledge settings contract.
// library 是遵守当前知识库设置合同的完整合成摘要。
const library: KnowledgeLibrarySummary = {
  id: "library", name: "Synthetic library", description: "", lifecycleState: "active", documentCount: 0, chunkCount: 0, indexStatus: "ready", createdAtMs: 1, updatedAtMs: 1,
  settings: { retrievalMode: "raw_bm25", fallbackPolicy: "explicit_only", curation: { enabled: false, promptVersion: "knowledge-display-v1" }, displayIndex: { status: "not_created", generatedDocumentCount: 0 }, embedding: { enabled: false, embeddingBatchSize: 10, batchSizeSource: "fallback_default" }, vectorIndex: { status: "disabled" } },
};

describe("project knowledge authoritative state", () => {
  let host: HTMLDivElement;
  let root: Root;
  beforeEach(() => {
    Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
    vi.clearAllMocks(); services.libraries.mockResolvedValue([library]); services.bindings.mockResolvedValue([]); services.save.mockResolvedValue(undefined);
    host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  });
  afterEach(async () => { await act(async () => root.unmount()); host.remove(); });
  it("distinguishes a failed read from an empty library and retries only reads", async () => {
    services.libraries.mockRejectedValueOnce(new Error("read failed"));
    await act(async () => root.render(<ProjectKnowledgeBasesSection projectId="a" />));
    expect(host.querySelector('[role="alert"]')?.textContent).toContain("read failed");
    expect(host.textContent).not.toContain("knowledge.emptyLibraries");
    await act(async () => host.querySelector("button")!.click());
    expect(host.querySelectorAll('[role="checkbox"], [role="switch"]')).toHaveLength(2);
    expect(services.save).not.toHaveBeenCalled();
  });
  it("removes the previous project rows when the new project fails to load", async () => {
    await act(async () => root.render(<ProjectKnowledgeBasesSection projectId="a" />));
    services.bindings.mockRejectedValueOnce(new Error("project b unavailable"));
    await act(async () => root.render(<ProjectKnowledgeBasesSection projectId="b" />));
    expect(host.querySelectorAll('[role="checkbox"]')).toHaveLength(0);
    expect(host.textContent).not.toContain(library.name);
    expect(host.querySelector('[role="alert"]')?.textContent).toContain("project b unavailable");
  });
  it("reports committed state with failed refresh and does not replay the write on retry", async () => {
    await act(async () => root.render(<ProjectKnowledgeBasesSection projectId="a" />));
    services.libraries.mockRejectedValueOnce(new Error("refresh failed"));
    await act(async () => host.querySelector<HTMLButtonElement>('[role="checkbox"]')!.click());
    expect(services.save).toHaveBeenCalledWith("library", "a", true, false);
    expect(services.success).not.toHaveBeenCalled();
    expect(host.querySelector('[role="alert"]')?.textContent).toContain("projectSettings.knowledgeBasesSavedRefreshFailed");
    expect(host.querySelector('[role="alert"]')?.textContent).toContain("refresh failed");
    await act(async () => host.querySelector("button")!.click());
    expect(services.save).toHaveBeenCalledTimes(1);
  });
});
