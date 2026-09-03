import { describe, expect, it } from "vitest"

import type { VmmNativeProjectContentCounts } from "@/types"

import {
  nativeProjectConfirmationRequest,
  type NativeProjectPreview,
} from "./vmm-native-project-page"

// EMPTY_COUNTS provides a complete neutral native-project content summary for request tests.
// EMPTY_COUNTS 为请求测试提供完整的中性原生项目内容汇总。
const EMPTY_COUNTS: VmmNativeProjectContentCounts = {
  sessions: 0,
  turns: 0,
  memories: 0,
  profileNodes: 0,
  contextEvidence: 0,
  compactBoundaries: 0,
  vectorRows: 0,
}

describe("native project confirmation requests", () => {
  it("reuses the same migration command identity after a lost response", () => {
    // preview represents one accepted confirmation dialog that remains mounted across retries.
    // preview 表示一个已接受且在重试期间保持挂载的确认对话框。
    const preview: NativeProjectPreview = {
      commandId: "11111111-1111-4111-8111-111111111111",
      kind: "migration",
      value: {
        storageVersion: 7,
        sourceProjectId: 2,
        targetProjectId: 3,
        source: EMPTY_COUNTS,
        targetBefore: EMPTY_COUNTS,
        targetAfter: EMPTY_COUNTS,
        sessionRenames: [{ from: "session", to: "session-1" }],
        records: [],
        previewFingerprint: "preview-sha256",
      },
    }

    // firstAttempt and retry model an applied mutation whose first response never reached the UI.
    // firstAttempt 与 retry 模拟变更已落盘但首次响应未到达界面的场景。
    const firstAttempt = nativeProjectConfirmationRequest(preview)
    const retry = nativeProjectConfirmationRequest(preview)

    expect(retry).toEqual(firstAttempt)
    expect(retry).toEqual({
      action: "native_project_migrate",
      commandId: "11111111-1111-4111-8111-111111111111",
      payload: {
        sourceProjectId: 2,
        targetProjectId: 3,
        expectedStorageVersion: 7,
        expectedPreviewFingerprint: "preview-sha256",
        acceptSessionRenames: true,
      },
    })
  })

  it("requires binding detachment when a delete affects recall-only consumers", () => {
    // preview captures a project used only through linked recall associations.
    // preview 捕获一个仅通过连带召回关联被使用的项目。
    const preview: NativeProjectPreview = {
      commandId: "22222222-2222-4222-8222-222222222222",
      kind: "delete",
      value: {
        storageVersion: 9,
        project: {
          projectId: 4,
          name: "shared",
          tag: "shared",
          protected: false,
          ownerVulcanProjectIds: [],
          linkedVulcanProjectIds: ["project-a"],
        },
        content: EMPTY_COUNTS,
      },
    }

    expect(nativeProjectConfirmationRequest(preview).payload.confirmDetachBindings).toBe(true)
  })
})
