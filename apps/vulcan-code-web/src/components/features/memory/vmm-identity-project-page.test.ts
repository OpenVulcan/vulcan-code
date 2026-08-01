import { describe, expect, it } from "vitest"

import { canonicalProjectPath } from "./vmm-identity-project-page"

describe("VMM canonical project path", () => {
  it("joins the explicit Team, Space, and Project hierarchy", () => {
    expect(canonicalProjectPath("Team A", "Space A", "Project A"))
      .toBe("Team A/Space A/Project A")
  })

  it("trims segments and prevents embedded separators from adding hierarchy levels", () => {
    expect(canonicalProjectPath(" Team/A ", " Space\\A ", " Project/A "))
      .toBe("Team-A/Space-A/Project-A")
  })
})
