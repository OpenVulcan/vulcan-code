import { describe, expect, it } from "vitest"

import type { LuaSkillsGithubMirror, LuaSkillsNetworkSettings } from "@/types"

import {
  LUASKILLS_HUB_SITE_URLS,
  activeMirrorId,
  customMirrorId,
  hubRegionPromptTarget,
  mirrorChoices,
} from "./lua-skills-network-sources"

// network builds one network block with defaults overridden by change.
// network 构建一个网络块，以 change 覆盖默认值。
function network(change: Partial<LuaSkillsNetworkSettings> = {}): LuaSkillsNetworkSettings {
  return {
    allowDownloads: true,
    githubBaseUrl: null,
    githubApiBaseUrl: null,
    officialSkillHubBaseUrl: null,
    enablePrivateUrlSkillInstall: false,
    privateSkillSourceAllowlist: [],
    customGithubMirrors: [],
    dismissedHubRegionPrompts: [],
    ...change,
  }
}

const BUILTINS: LuaSkillsGithubMirror[] = [
  { id: "github", label: "GitHub", kind: "direct", region: "global", githubBaseUrl: null },
  { id: "ghfast-top", label: "ghfast.top", kind: "prefix", region: "china", githubBaseUrl: "https://ghfast.top/https://github.com" },
]

describe("hubRegionPromptTarget", () => {
  it("suggests the account region only when the Hub is the other official site", () => {
    expect(hubRegionPromptTarget(network(), "china")).toBe("china")
    expect(hubRegionPromptTarget(network(), "global")).toBeNull()
    expect(hubRegionPromptTarget(network({ officialSkillHubBaseUrl: LUASKILLS_HUB_SITE_URLS.china }), "global")).toBe("global")
  })

  it("stays silent when signed out, dismissed or on a custom Hub", () => {
    expect(hubRegionPromptTarget(network(), null)).toBeNull()
    expect(hubRegionPromptTarget(network({ dismissedHubRegionPrompts: ["china"] }), "china")).toBeNull()
    expect(hubRegionPromptTarget(network({ officialSkillHubBaseUrl: "https://hub.example.test" }), "china")).toBeNull()
  })
})

describe("activeMirrorId", () => {
  it("matches the saved download address, treating empty as direct GitHub", () => {
    const choices = mirrorChoices(BUILTINS, [])
    expect(activeMirrorId(choices, network({ githubBaseUrl: "" }))).toBe("github")
    expect(activeMirrorId(choices, network({
      githubBaseUrl: "https://ghfast.top/https://github.com/",
      githubApiBaseUrl: "https://api.example.test",
    }))).toBe("ghfast-top")
    expect(activeMirrorId(choices, network({ githubBaseUrl: "https://other.example.test/https://github.com" }))).toBeNull()
  })

  it("selects a user mirror by its download address", () => {
    const choices = mirrorChoices(BUILTINS, [{ id: "custom-1", label: "Mine", githubBaseUrl: "https://mirror.example.test/https://github.com" }])
    expect(activeMirrorId(choices, network({ githubBaseUrl: "https://mirror.example.test/https://github.com" }))).toBe("custom-1")
  })
})

describe("customMirrorId", () => {
  it("never reuses an existing identifier", () => {
    const choices = mirrorChoices(BUILTINS, [{ id: "custom-2", label: "Mine", githubBaseUrl: "https://a.example.test" }])
    const id = customMirrorId(choices)
    expect(choices.some((choice) => choice.id === id)).toBe(false)
  })
})
