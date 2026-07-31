import { describe, expect, it } from "vitest"

import type { VmmMemorySnapshot } from "@/types"

import { shouldRequestVmmManagementCatalog } from "./MemoryArea"

// emptyCatalog returns one unloaded management directory for first-run gate tests.
// emptyCatalog 返回一份尚未加载的管理目录，用于首次运行门控测试。
function emptyCatalog(): VmmMemorySnapshot["managementCatalog"] {
  return {
    loading: false,
    users: [],
    projects: [],
  }
}

describe("VMM management catalog readiness gate", () => {
  it("does not contact VMM before the managed process is ready", () => {
    expect(shouldRequestVmmManagementCatalog({
      activeSection: "overview",
      operationBusy: false,
      processReady: false,
      requestIssued: false,
      catalog: emptyCatalog(),
    })).toBe(false)
  })

  it("loads once after readiness and skips busy, repeated, populated, or unrelated states", () => {
    expect(shouldRequestVmmManagementCatalog({
      activeSection: "overview",
      operationBusy: false,
      processReady: true,
      requestIssued: false,
      catalog: emptyCatalog(),
    })).toBe(true)
    expect(shouldRequestVmmManagementCatalog({
      activeSection: "profile",
      operationBusy: false,
      processReady: true,
      requestIssued: false,
      catalog: emptyCatalog(),
    })).toBe(true)
    expect(shouldRequestVmmManagementCatalog({
      activeSection: "overview",
      operationBusy: true,
      processReady: true,
      requestIssued: false,
      catalog: emptyCatalog(),
    })).toBe(false)
    expect(shouldRequestVmmManagementCatalog({
      activeSection: "overview",
      operationBusy: false,
      processReady: true,
      requestIssued: true,
      catalog: emptyCatalog(),
    })).toBe(false)
    expect(shouldRequestVmmManagementCatalog({
      activeSection: "rules",
      operationBusy: false,
      processReady: true,
      requestIssued: false,
      catalog: emptyCatalog(),
    })).toBe(false)
    expect(shouldRequestVmmManagementCatalog({
      activeSection: "overview",
      operationBusy: false,
      processReady: true,
      requestIssued: false,
      catalog: {
        ...emptyCatalog(),
        users: [{
          stableKey: "1",
          displayName: "Visible User",
          memoryStatus: "ready",
          protected: false,
        }],
      },
    })).toBe(false)
  })
})
