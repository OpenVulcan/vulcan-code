import { describe, expect, it } from "vitest"

import { buildProviderModelAccessRows } from "@/components/features/settings/provider-model-access-dialog"
import type { PublicAiSettingsSnapshot } from "@/types"

// snapshotFixture contains one provider-disabled model that remains present only in its support catalog.
// snapshotFixture 包含一个仅保留在支持目录中的供应商已关闭模型。
const snapshotFixture = {
  connections: [{ id: "account", provider_id: "native" }],
  modelSupportCatalogs: {
    account: {
      models: [
        { id: "enabled", display_name: "Enabled" },
        { id: "disabled", display_name: "Disabled" },
      ],
    },
  },
  modelAccessRecords: [
    { providerId: "native", modelId: "enabled", enabled: true, defaultEnabled: true },
    { providerId: "native", modelId: "disabled", enabled: false, defaultEnabled: true },
  ],
  customModels: {},
} as unknown as PublicAiSettingsSnapshot

describe("buildProviderModelAccessRows", () => {
  it("keeps disabled models visible in provider configuration", () => {
    expect(buildProviderModelAccessRows(snapshotFixture, "native").map((row) => ({
      id: row.id,
      displayName: row.displayName,
      enabled: row.enabled,
      defaultEnabled: row.defaultEnabled,
    }))).toEqual([
      { id: "disabled", displayName: "Disabled", enabled: false, defaultEnabled: true },
      { id: "enabled", displayName: "Enabled", enabled: true, defaultEnabled: true },
    ])
  })
})
