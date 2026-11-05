import { describe, expect, it } from "vitest"

import {
  DEFAULT_COMMIT_BRANCH_PREFIX,
  gitCommitSettingsDraft,
  gitCommitSettingsDraftKey,
} from "@/components/features/settings/git-commit-settings-draft"

describe("git commit settings draft", () => {
  it("treats a saved snapshot as clean whatever order the service serialized its keys in", () => {
    // The model picker builds the selection as mode, providerId, modelId.
    const picked = {
      newBranchPrefix: "vulcan/",
      summaryModel: { mode: "fixed" as const, providerId: "opencode-go", modelId: "glm-5.2" },
      summaryExtraPrompt: "",
    }
    // The service returns object keys in alphabetical order.
    const saved = JSON.parse(
      '{"newBranchPrefix":"vulcan/","summaryExtraPrompt":"","summaryModel":{"mode":"fixed","modelId":"glm-5.2","providerId":"opencode-go"}}',
    )
    expect(gitCommitSettingsDraftKey(gitCommitSettingsDraft(saved))).toBe(gitCommitSettingsDraftKey(picked))
  })

  it("still detects a changed model", () => {
    const saved = gitCommitSettingsDraft({
      newBranchPrefix: "vulcan/",
      summaryExtraPrompt: "",
      summaryModel: { mode: "fixed", providerId: "opencode-go", modelId: "glm-5.2" },
    })
    const edited = { ...saved, summaryModel: { mode: "fixed" as const, providerId: "opencode-go", modelId: "glm-5.3" } }
    expect(gitCommitSettingsDraftKey(edited)).not.toBe(gitCommitSettingsDraftKey(saved))
  })

  it("fills defaults before the first save", () => {
    expect(gitCommitSettingsDraft(undefined)).toEqual({
      newBranchPrefix: DEFAULT_COMMIT_BRANCH_PREFIX,
      summaryModel: { mode: "session" },
      summaryExtraPrompt: "",
    })
  })
})
