import { describe, expect, it } from "vitest"

import {
  vmmProfileBundlePreview,
  vmmProfileCatalogAction,
  vmmProfileEffectiveTarget,
  vmmProfileNodeAction,
  vmmProfileUsesFixedNativeUser,
  vmmProfileUsesNativeProjectCatalog,
} from "./vmm-profile-page"

describe("VMM profile backend separation", () => {
  it("uses the one-level native project catalog while preserving both profile targets", () => {
    expect(vmmProfileUsesNativeProjectCatalog("native")).toBe(true)
    expect(vmmProfileCatalogAction("native")).toBe("native_project_catalog")
    expect(vmmProfileNodeAction("native")).toBe("native_profile_records")
    expect(vmmProfileEffectiveTarget("native", "user")).toBe("user")
    expect(vmmProfileEffectiveTarget("native", "project")).toBe("project")
    expect(vmmProfileUsesFixedNativeUser("native", "user")).toBe(true)
    expect(vmmProfileUsesFixedNativeUser("native", "project")).toBe(false)
  })

  it("keeps the external user and project catalog behavior", () => {
    expect(vmmProfileUsesNativeProjectCatalog("external")).toBe(false)
    expect(vmmProfileCatalogAction("external")).toBe("load_users_projects")
    expect(vmmProfileNodeAction("external")).toBe("load_profile_nodes")
    expect(vmmProfileEffectiveTarget("external", "user")).toBe("user")
    expect(vmmProfileEffectiveTarget("external", "project")).toBe("project")
    expect(vmmProfileUsesFixedNativeUser("external", "user")).toBe(false)
  })

  it("renders only the selected user or project profile section", () => {
    // bundle preserves independently addressable profile sections from one complete response.
    // bundle 保存一次完整响应中可独立寻址的画像段落。
    const bundle = {
      mode: "PROFILE_BUNDLE_MODE_SPLIT" as const,
      combined_text: "combined",
      project_profile: "project profile",
      user_profile: "user profile",
      trace_id: "trace",
    }

    expect(vmmProfileBundlePreview(bundle, "user")).toBe("user profile")
    expect(vmmProfileBundlePreview(bundle, "project")).toBe("project profile")
  })

  it("consumes only Full text even when unexpected split fields are populated", () => {
    const bundle = { mode: "PROFILE_BUNDLE_MODE_FULL" as const, combined_text: "full with explanation", user_profile: "must not duplicate user", project_profile: "must not duplicate project", trace_id: "full" }
    for (const target of ["user", "project"] as const) {
      expect(vmmProfileBundlePreview(bundle, target)).toBe("full with explanation")
      expect(vmmProfileBundlePreview({ ...bundle, combined_text: "" }, target)).toBe("")
    }
  })

  it("does not invent a bundle mode from populated fields", () => {
    expect(vmmProfileBundlePreview({ mode: "PROFILE_BUNDLE_MODE_UNSPECIFIED", combined_text: "ambiguous", user_profile: "user", project_profile: "project", trace_id: "unknown" }, "user")).toBeNull()
  })

  it("keeps an empty Split target empty without falling back to combined or the other role", () => {
    expect(vmmProfileBundlePreview({ mode: "PROFILE_BUNDLE_MODE_SPLIT", combined_text: "must not fallback", user_profile: "", project_profile: "other role", trace_id: "split" }, "user")).toBe("")
  })
})
