import { describe, expect, it } from "vitest"

import type { VmmNativeProjectCatalogSummary } from "@/types"

import {
  nativeAdjacentTurns,
  nativeMemoryRecordsRequest,
  nativePreferredSessionId,
  nativeProjectOwnerId,
  nativeRecordCounts,
} from "./vmm-native-memory-records"

// CATALOG provides stable native owners and one protected default for selector tests.
// CATALOG 为选择器测试提供稳定原生归属与一个受保护默认项目。
const CATALOG: VmmNativeProjectCatalogSummary = {
  projects: [
    {
      projectId: 1,
      name: "default",
      tag: "default",
      protected: true,
      ownerVulcanProjectIds: [],
      linkedVulcanProjectIds: [],
    },
    {
      projectId: 2,
      name: "project memory",
      tag: "project-a",
      protected: false,
      ownerVulcanProjectIds: ["project-a"],
      linkedVulcanProjectIds: [],
    },
  ],
  bindings: [{
    vulcanProjectId: "project-a",
    ownerProjectId: 2,
    linkedProjectIds: [1],
  }],
}

describe("native memory provenance requests", () => {
  it("navigates only server-provided same-session neighbors in the server order", () => {
    // records deliberately uses reverse display order and includes an unrelated session.
    // records 有意使用反向显示顺序并包含无关会话。
    const records = [10, 4, 3, 2].map((id) => ({
      turnId: id,
      sessionSourceId: id === 4 ? "foreign" : "session",
      previousTurnIds: [],
      nextTurnIds: [],
    }))
    // anchor includes missing and foreign IDs to ensure neither can be opened.
    // anchor 包含缺失及无关会话 ID，以确保它们均不可打开。
    const anchor = { turnId: 5, sessionSourceId: "session", previousTurnIds: [2, 3, 4], nextTurnIds: [10, 99] }
    expect(nativeAdjacentTurns(anchor, records)).toEqual({ previous: [records[3], records[2]], next: [records[0]] })
    expect(nativeAdjacentTurns(null, records)).toEqual({ previous: [], next: [] })
  })

  it("opens the selected VulcanCode project's stable native owner", () => {
    expect(nativeProjectOwnerId(CATALOG, "project-a")).toBe(2)
  })

  it("uses the protected default only when the host project has no binding", () => {
    expect(nativeProjectOwnerId(CATALOG, "unbound-project")).toBe(1)
  })

  it("keeps the stable session id and omits blank overview identities", () => {
    expect(nativeMemoryRecordsRequest(2, " session-id ")).toEqual({
      vmmProjectId: 2,
      sessionSourceId: "session-id",
    })
    expect(nativeMemoryRecordsRequest(2, "  ")).toEqual({ vmmProjectId: 2 })
  })

  it("keeps project overview unless the exact host session exists", () => {
    const sessions = [{
      status: null,
      sessionSourceId: "session-a",
      turnCount: 0,
      turnStatuses: {
        pending: 0,
        completed: 0,
        processedUnknown: 0,
        analysisBypassed: 0,
        corrupted: 0,
      },
      memoryCount: 0,
      profileNodeCount: 0,
      directMemoryCount: 0,
      memoryLevels: { session: 0, phase: 0, stable: 0, persistent: 0 },
    }]

    expect(nativePreferredSessionId(sessions)).toBeNull()
    expect(nativePreferredSessionId(sessions, "missing")).toBeNull()
    expect(nativePreferredSessionId(sessions, " session-a ")).toBe("session-a")
  })

  it("counts every authoritative project turn status and memory level", () => {
    expect(nativeRecordCounts(
      [
        { status: "completed" },
        { status: "pending" },
        { status: "processed_unknown" },
        { status: "analysis_bypassed" },
        { status: "corrupted" },
      ],
      [
        { memoryLevel: "session" },
        { memoryLevel: "phase" },
        { memoryLevel: "stable" },
        { memoryLevel: "persistent" },
      ],
    )).toEqual({
      turnStatuses: {
        pending: 1,
        completed: 1,
        processedUnknown: 1,
        analysisBypassed: 1,
        corrupted: 1,
      },
      memoryLevels: {
        session: 1,
        phase: 1,
        stable: 1,
        persistent: 1,
      },
    })
  })
})
