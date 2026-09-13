import { describe, expect, it, vi } from "vitest";
import { serviceClient } from "@/service/webui-service-runtime";
import { createAuditPublicAi, createAuditWorkbench } from "./fixtures";

describe("isolated full-workbench transport", () => {
  it("serves real typed fixture contracts without fetch or WebSocket discovery", async () => {
    // network spies prove even callers that directly use serviceClient stay within memory.
    // 网络观察器证明直接使用 serviceClient 的调用方也保持在内存中。
    const fetchSpy = vi.fn(() => Promise.reject(new Error("network forbidden")));
    const socketSpy = vi.fn(() => { throw new Error("socket forbidden"); });
    vi.stubGlobal("fetch", fetchSpy); vi.stubGlobal("WebSocket", socketSpy);
    try {
      expect(await serviceClient.request("startup.snapshot")).toMatchObject({ ready: true });
      expect(await serviceClient.request("service_public_ai_settings_snapshot")).toMatchObject({ providers: [] });
      expect(await serviceClient.request("service_mcp_runtime_snapshot")).toEqual([]);
      await expect(serviceClient.request("service_update_public_ai_connection", {})).rejects.toThrow("UI_AUDIT_UNIMPLEMENTED_RPC");
      await expect(serviceClient.request("service_run_vmm_memory_action", { request: { action: "native_project_delete", payload: { vmmProjectId: 1 } } })).rejects.toThrow("UI_AUDIT_UNIMPLEMENTED_MEMORY_ACTION");
      expect(fetchSpy).not.toHaveBeenCalled(); expect(socketSpy).not.toHaveBeenCalled();
    } finally { vi.unstubAllGlobals(); }
  });
  it("creates empty and long-list data with explicit invalid defaults rather than assuming live availability", () => {
    expect(createAuditWorkbench("empty").projects).toHaveLength(0);
    expect(createAuditWorkbench("long-list").subagentCatalog.definitions).toHaveLength(45);
    expect(createAuditPublicAi("long-list").providers).toHaveLength(24);
    expect(createAuditPublicAi("invalid-default").active?.model_id).toBe("deleted-default-model");
    // Replacement admission and chat routes must use their distinct production authorities.
    // 替代准入与聊天路由必须使用各自独立的生产权威。
    const configured = createAuditPublicAi("invalid-default");
    expect(configured.mediaUnderstandingModelCandidates?.[0].selection).toMatchObject({ model_id: "audit-chat-1", connection_id: "audit-account-1" });
    expect(configured.multimodalUnderstandingCandidateModels).toEqual([{ providerId: "custom:audit-1", modelId: "audit-chat-1" }]);
    expect(configured.providerToolTargets[0].target.contract.capability).toBe("image_understand");
    expect(createAuditWorkbench("invalid-default").providers[0].models?.[0].routeRevision).toBe("ui-audit-v1");
  });
});
