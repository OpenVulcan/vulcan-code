import { describe, expect, it } from "vitest"

import type { PublicAiConnection, PublicAiProviderDescriptor } from "@/types"

import {
  buildProviderDirectory,
  moveItem,
  priorityWritesForOrder,
  providerMatchesFilter,
} from "./provider-directory"

// provider builds the minimal descriptor the directory reads.
// provider 构造目录读取的最小描述。
function provider(id: string, extra: Partial<PublicAiProviderDescriptor> = {}): PublicAiProviderDescriptor {
  return {
    id,
    display_name: id.toUpperCase(),
    kind: "system",
    category: "official",
    credential_directory_visibility: "configured_only",
    credential_registration: "direct",
    service_entries: [],
    ...extra,
  }
}

// connection builds one account owned by providerId.
// connection 构造一个属于 providerId 的账号。
function connection(providerId: string): PublicAiConnection {
  return { provider_id: providerId } as PublicAiConnection
}

describe("buildProviderDirectory", () => {
  it("puts configured providers first in priority order and the rest in recommended order", () => {
    const providers = [
      provider("late", { directory_rank: 50 }),
      provider("unranked"),
      provider("early", { directory_rank: 10 }),
      provider("mine-a", { directory_rank: 30 }),
      provider("mine-b", { directory_rank: 20 }),
      provider("custom", { kind: "custom" }),
    ]
    const directory = buildProviderDirectory({
      providers,
      connections: [connection("mine-a"), connection("mine-b")],
      providerPriorities: { "mine-a": 5 },
    })
    expect(directory.configured.map((item) => item.id)).toEqual(["mine-a", "mine-b", "custom"])
    expect(directory.available.map((item) => item.id)).toEqual(["early", "late", "unranked"])
  })

  it("pins core accounts above every list whatever their priority or configuration", () => {
    const core = { credential_directory_visibility: "core_account" as const }
    const directory = buildProviderDirectory({
      providers: [
        provider("mine", { directory_rank: 1 }),
        provider("core-b", { ...core, directory_rank: 9 }),
        provider("core-a", { ...core, directory_rank: 5 }),
        provider("other", { directory_rank: 2 }),
      ],
      connections: [connection("mine"), connection("core-b")],
      providerPriorities: { mine: 9, "core-b": -3 },
    })
    expect(directory.pinned.map((item) => item.id)).toEqual(["core-a", "core-b"])
    expect(directory.configured.map((item) => item.id)).toEqual(["mine"])
    expect(directory.available.map((item) => item.id)).toEqual(["other"])
  })
})

describe("providerMatchesFilter", () => {
  it("combines the service tag with a case-insensitive search over id and names", () => {
    const search = provider("tavily", { service_tags: ["search"] })
    expect(providerMatchesFilter(search, "Tavily", "search", "")).toBe(true)
    expect(providerMatchesFilter(search, "Tavily", "model", "")).toBe(false)
    expect(providerMatchesFilter(search, "Recherche Tavily", "all", "RECHER")).toBe(true)
    expect(providerMatchesFilter(search, "Tavily", "all", "deep")).toBe(false)
  })
})

describe("priorityWritesForOrder", () => {
  it("writes only the priorities that change to realize a moved order", () => {
    const order = moveItem(["a", "b", "c"], 2, 0)
    expect(order).toEqual(["c", "a", "b"])
    expect(priorityWritesForOrder(order, { a: 2, b: 1 })).toEqual([{ providerId: "c", priority: 3 }])
    expect(moveItem(["a"], 0, 3)).toEqual(["a"])
  })
})
