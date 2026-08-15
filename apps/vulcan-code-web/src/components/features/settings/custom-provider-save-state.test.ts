import { describe, expect, it, vi } from "vitest"

import { persistAndApplyCustomProviderSnapshot } from "@/components/features/settings/custom-provider-save-state"
import type {
  PublicAiCustomProviderSaveRequest,
  PublicAiSettingsSnapshot,
} from "@/types"

// requestFixture returns one complete typed custom-provider save command.
// requestFixture 返回一条完整类型化自定义供应商保存命令。
function requestFixture(providerId?: string): PublicAiCustomProviderSaveRequest {
  return {
    providerId,
    displayName: "Fixture Provider",
    baseUrl: "https://example.test/v1",
    protocols: ["open_ai_responses"],
    protocolRoutes: [],
    remoteCompactionPolicy: "disabled",
    websocketPolicy: "disabled",
    authentication: "bearer_authorization",
    modelDiscoveryMode: "manual",
    requestHeaders: {},
    allowManualFallback: true,
    allowPendingValidation: false,
    confirmInactiveProtocolOverrideDeletion: false,
  }
}

describe("custom provider save state", () => {
  it("publishes exactly the snapshot returned after durable success", async () => {
    const snapshot = { providers: [] } as unknown as PublicAiSettingsSnapshot
    const persist = vi.fn().mockResolvedValue(snapshot)
    const apply = vi.fn()

    await expect(
      persistAndApplyCustomProviderSnapshot(requestFixture(), persist, apply),
    ).resolves.toBe(snapshot)
    expect(persist).toHaveBeenCalledOnce()
    expect(apply).toHaveBeenCalledExactlyOnceWith(snapshot)
  })

  it("preserves the previous UI snapshot when durable persistence fails", async () => {
    const persist = vi.fn().mockRejectedValue(new Error("durable save failed"))
    const apply = vi.fn()

    await expect(
      persistAndApplyCustomProviderSnapshot(requestFixture(), persist, apply),
    ).rejects.toThrow("durable save failed")
    expect(apply).not.toHaveBeenCalled()
  })

  // Verifies three concurrently edited providers retain their exact command identity and apply
  // only the snapshot returned for that provider, even when completions arrive out of order.
  // 验证三个并发编辑的供应商保持各自精确命令身份；即使完成顺序不同，也只应用该供应商
  // 返回的快照。
  it("isolates three concurrent provider saves", async () => {
    // providerIds identifies the three independently edited custom-provider documents.
    // providerIds 标识三个独立编辑的自定义供应商文档。
    const providerIds = ["custom:alpha", "custom:beta", "custom:gamma"]
    // snapshots gives every provider a distinct immutable Service result.
    // snapshots 为每个供应商提供不同的不可变 Service 结果。
    const snapshots = new Map(providerIds.map((providerId) => [
      providerId,
      { providers: [{ id: providerId }] } as unknown as PublicAiSettingsSnapshot,
    ]))
    // resolvers controls completion order without sharing provider state.
    // resolvers 控制完成顺序，同时不共享供应商状态。
    const resolvers = new Map<string, (snapshot: PublicAiSettingsSnapshot) => void>()
    // persist captures the provider-owned resolver for each exact typed request.
    // persist 为每条精确类型化请求捕获该供应商自有的完成器。
    const persist = vi.fn((request: PublicAiCustomProviderSaveRequest) => (
      new Promise<PublicAiSettingsSnapshot>((resolve) => {
        resolvers.set(request.providerId!, resolve)
      })
    ))
    // apply records only snapshots that have crossed the durable Service boundary.
    // apply 仅记录已经跨过持久化 Service 边界的快照。
    const apply = vi.fn()
    // saves starts all three provider commands before any result completes.
    // saves 在任何结果完成前启动全部三个供应商命令。
    const saves = providerIds.map((providerId) => persistAndApplyCustomProviderSnapshot(
      requestFixture(providerId),
      persist,
      apply,
    ))

    for (const providerId of ["custom:gamma", "custom:alpha", "custom:beta"]) {
      resolvers.get(providerId)!(snapshots.get(providerId)!)
      await Promise.resolve()
    }
    await expect(Promise.all(saves)).resolves.toEqual(
      providerIds.map((providerId) => snapshots.get(providerId)),
    )
    expect(persist.mock.calls.map(([request]) => request.providerId)).toEqual(providerIds)
    expect(apply.mock.calls.map(([snapshot]) => snapshot)).toEqual([
      snapshots.get("custom:gamma"),
      snapshots.get("custom:alpha"),
      snapshots.get("custom:beta"),
    ])
  })
})
