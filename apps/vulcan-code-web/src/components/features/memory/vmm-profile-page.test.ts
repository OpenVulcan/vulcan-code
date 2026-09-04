import { describe, expect, it } from "vitest"

import {
  vmmProfileCatalogAction,
  vmmProfileEffectiveTarget,
  vmmProfileUsesNativeProjectCatalog,
} from "./vmm-profile-page"

describe("VMM profile backend separation", () => {
  it("uses the one-level native project catalog without exposing a user target", () => {
    expect(vmmProfileUsesNativeProjectCatalog("native")).toBe(true)
    expect(vmmProfileCatalogAction("native")).toBe("native_project_catalog")
    expect(vmmProfileEffectiveTarget("native", "user")).toBe("project")
  })

  it("keeps the external user and project catalog behavior", () => {
    expect(vmmProfileUsesNativeProjectCatalog("external")).toBe(false)
    expect(vmmProfileCatalogAction("external")).toBe("load_users_projects")
    expect(vmmProfileEffectiveTarget("external", "user")).toBe("user")
    expect(vmmProfileEffectiveTarget("external", "project")).toBe("project")
  })
})
