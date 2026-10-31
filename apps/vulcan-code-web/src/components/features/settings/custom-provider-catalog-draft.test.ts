import { describe, expect, it } from "vitest"
import type { PublicAiServiceEntry } from "@/types"
import { customProviderCatalogDraft } from "./custom-provider-catalog-draft"
import { protocolCollectionFromEntry, protocolRoutesFromCollection } from "./custom-provider-protocol-drafts"

// fixture returns a shared catalog with conflicting historical per-route directory declarations.
// fixture 返回带有冲突历史逐路由目录声明的共享目录。
function fixture(): PublicAiServiceEntry {
  return {
    id: "default", display_name: "Fixture", kind: "conversation",
    base_url: "https://catalog.example.test/v1", credential_profiles: [],
    authentication: "bearer_authorization", model_discovery: "open_ai",
    model_discovery_path: "/shared-models",
    protocol_routes: [
      { protocol_profile_id: "anthropic-messages.v1", model_discovery: "anthropic_compatible", model_discovery_path: "/route-models", verified: true },
      { protocol_profile_id: "openai-responses.v1", model_discovery: "manual", verified: true },
      { protocol_profile_id: "openai-chat-completions.v1", verified: true },
    ],
  }
}

describe("shared custom provider catalog", () => {
  it("keeps the catalog choice and shared path independent of conversation route order and selection", () => {
    // entry models a saved multi-protocol provider reopened for editing.
    // entry 模拟重新打开编辑的已保存多协议供应商。
    const entry = fixture()
    // expected records the shared contract regardless of conversation protocol edits.
    // expected 记录不受会话协议编辑影响的共享契约。
    const expected = { modelDiscoveryProtocol: "open_ai", modelDiscoveryMode: "custom_path", modelDiscoveryPath: "/shared-models" }
    expect(customProviderCatalogDraft(entry)).toEqual(expected)
    entry.protocol_routes.reverse()
    expect(customProviderCatalogDraft(entry)).toEqual(expected)
    entry.protocol_routes = entry.protocol_routes.filter((route) => route.protocol_profile_id === "anthropic-messages.v1")
    expect(customProviderCatalogDraft(entry)).toEqual(expected)
    expect(protocolRoutesFromCollection(protocolCollectionFromEntry(entry))).toEqual([{ protocol: "anthropic_messages" }])
  })

  it.each([
    ["open_ai", "open_ai"], ["anthropic_compatible", "anthropic"], ["gemini", "gemini"],
  ] as const)("restores the selected %s format without reading route discovery settings", (kind, selected) => {
    // entry changes the shared format while retaining conflicting route declarations.
    // entry 在保留冲突路由声明的同时改变共享格式。
    const entry = fixture()
    entry.model_discovery = kind
    expect(customProviderCatalogDraft(entry).modelDiscoveryProtocol).toBe(selected)
  })

  it("keeps manual selection authoritative even when a route contains remote discovery fields", () => {
    // entry retains historical remote route fields after manual mode is selected.
    // entry 在选择手工模式后保留历史远端路由字段。
    const entry = fixture()
    entry.model_discovery = "manual"
    delete entry.model_discovery_path
    expect(customProviderCatalogDraft(entry).modelDiscoveryMode).toBe("manual")
    expect(protocolRoutesFromCollection(protocolCollectionFromEntry(entry)).every((route) => !Object.hasOwn(route, "model_discovery_mode"))).toBe(true)
  })
})
