import { describe, expect, it } from "vitest"

import type { VmmMemorySnapshot } from "@/types"

import {
  externalSetupActionConnection,
  materializeExternalFixedBindingPlan,
  shouldRequestVmmManagementCatalog,
  shouldRouteAdvancedSaveToSetup,
  vmmUsesNativeMemoryMaintenance,
} from "./MemoryArea"
import { vmmNativeBindingModeKey } from "./vmm-overview-page"

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
  it("does not contact VMM before the selected backend is ready", () => {
    expect(shouldRequestVmmManagementCatalog({
      activeSection: "overview",
      deployment: "external",
      operationBusy: false,
      processReady: false,
      requestIssued: false,
      catalog: emptyCatalog(),
    })).toBe(false)
  })

  it("loads once after readiness and skips busy, repeated, populated, or unrelated states", () => {
    expect(shouldRequestVmmManagementCatalog({
      activeSection: "overview",
      deployment: "external",
      operationBusy: false,
      processReady: true,
      requestIssued: false,
      catalog: emptyCatalog(),
    })).toBe(true)
    expect(shouldRequestVmmManagementCatalog({
      activeSection: "profile",
      deployment: "external",
      operationBusy: false,
      processReady: true,
      requestIssued: false,
      catalog: emptyCatalog(),
    })).toBe(true)
    expect(shouldRequestVmmManagementCatalog({
      activeSection: "overview",
      deployment: "external",
      operationBusy: true,
      processReady: true,
      requestIssued: false,
      catalog: emptyCatalog(),
    })).toBe(false)
    expect(shouldRequestVmmManagementCatalog({
      activeSection: "overview",
      deployment: "external",
      operationBusy: false,
      processReady: true,
      requestIssued: true,
      catalog: emptyCatalog(),
    })).toBe(false)
    expect(shouldRequestVmmManagementCatalog({
      activeSection: "rules",
      deployment: "external",
      operationBusy: false,
      processReady: true,
      requestIssued: false,
      catalog: emptyCatalog(),
    })).toBe(false)
    expect(shouldRequestVmmManagementCatalog({
      activeSection: "overview",
      deployment: "external",
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

  it("never loads the external identity catalog for the native backend", () => {
    expect(shouldRequestVmmManagementCatalog({
      activeSection: "overview",
      deployment: "native",
      operationBusy: false,
      processReady: true,
      requestIssued: false,
      catalog: emptyCatalog(),
    })).toBe(false)
  })
})

describe("VMM native overview identity", () => {
  it("shows exactly the configured native project binding policy", () => {
    expect(vmmNativeBindingModeKey("fixed")).toBe("vmm.experience.nativeBindingFixed")
    expect(vmmNativeBindingModeKey("per_project")).toBe("vmm.experience.nativeBindingPerProject")
  })
})

describe("VMM content maintenance backend", () => {
  it("uses internal maintenance only for the native backend", () => {
    expect(vmmUsesNativeMemoryMaintenance("native")).toBe(true)
    expect(vmmUsesNativeMemoryMaintenance("external")).toBe(false)
  })
})

describe("VMM external setup connection projection", () => {
  it("keeps pre-confirmation probes request-scoped and excludes management credentials", () => {
    expect(externalSetupActionConnection({
      mode: "external",
      externalEndpoint: " 127.0.0.1:50051 ",
      externalApiKey: "grpc-secret",
      externalManagementBaseUrl: "https://management.example.test",
      externalManagementToken: "management-secret",
    })).toEqual({
      connection: {
        grpcTarget: "127.0.0.1:50051",
        grpcApiKey: "grpc-secret",
      },
    })
  })
})

describe("VMM external fixed binding plan", () => {
  it("materializes only default rows and preserves explicit per-project choices", () => {
    expect(materializeExternalFixedBindingPlan([
      {
        vulcanProjectId: "project-default",
        action: "bind_existing",
        projectName: null,
        targetProjectId: null,
      },
      {
        vulcanProjectId: "project-existing",
        action: "bind_existing",
        projectName: null,
        targetProjectId: 42,
      },
      {
        vulcanProjectId: "project-create",
        action: "create_and_bind",
        projectName: "custom-project",
        targetProjectId: null,
      },
    ], 7)).toEqual([
      {
        vulcanProjectId: "project-default",
        action: "bind_existing",
        projectName: null,
        targetProjectId: 7,
      },
      {
        vulcanProjectId: "project-existing",
        action: "bind_existing",
        projectName: null,
        targetProjectId: 42,
      },
      {
        vulcanProjectId: "project-create",
        action: "create_and_bind",
        projectName: "custom-project",
        targetProjectId: null,
      },
    ])
  })

  it("rejects an invalid materialized fixed project identity", () => {
    expect(() => materializeExternalFixedBindingPlan([], 0)).toThrow(
      "requires a positive project id",
    )
  })
})

describe("VMM advanced backend switch gate", () => {
  it("routes only backend changes through full setup reconciliation", () => {
    expect(shouldRouteAdvancedSaveToSetup("native", "native")).toBe(false)
    expect(shouldRouteAdvancedSaveToSetup("external_grpc", "external_grpc")).toBe(false)
    expect(shouldRouteAdvancedSaveToSetup("native", "external_grpc")).toBe(true)
    expect(shouldRouteAdvancedSaveToSetup("external_grpc", "native")).toBe(true)
  })
})
