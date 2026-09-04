import { describe, expect, it } from "vitest"

import { vmmManagementSectionTranslationKeys } from "./vmm-memory-sections"

// VMM navigation copy must never expose external user identity concepts in native mode.
// VMM 导航文案不得在原生模式暴露外置用户身份概念。
describe("VMM memory section navigation", () => {
  // Native navigation uses project-only identity wording and backend-specific page descriptions.
  // 原生导航使用仅项目身份文案和后端专属页面说明。
  it("selects native project-only navigation copy", () => {
    expect(vmmManagementSectionTranslationKeys("identity_projects", "native")).toEqual({
      detail: "memory.identity.nativeDescription",
      label: "memory.identity.nativeTitle",
    })
    expect(vmmManagementSectionTranslationKeys("content", "native")?.detail).toBe(
      "memory.content.nativeDescription",
    )
    expect(vmmManagementSectionTranslationKeys("profile", "native")?.detail).toBe(
      "memory.profile.nativeDescription",
    )
  })

  // External navigation retains user-and-project identity wording and remote page descriptions.
  // 外置导航保留用户与项目身份文案及远端页面说明。
  it("retains external user-and-project navigation copy", () => {
    expect(vmmManagementSectionTranslationKeys("identity_projects", "external")).toEqual({
      detail: "memory.identity.description",
      label: "memory.identity.title",
    })
    expect(vmmManagementSectionTranslationKeys("content", "external")?.detail).toBe(
      "memory.content.description",
    )
    expect(vmmManagementSectionTranslationKeys("profile", "external")?.detail).toBe(
      "memory.profile.description",
    )
  })
})
