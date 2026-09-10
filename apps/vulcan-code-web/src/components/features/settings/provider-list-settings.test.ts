import { describe, expect, it } from "vitest"

import { credentialSupportsModelSynchronization } from "./provider-list-settings"

describe("credential model synchronization visibility", () => {
  // Dynamic catalogs are the only directories whose remote membership can be synchronized.
  // 动态目录是唯一可以同步远端成员关系的目录。
  it("shows synchronization only for dynamic credential directories", () => {
    expect(credentialSupportsModelSynchronization("dynamic")).toBe(true)
    expect(credentialSupportsModelSynchronization("fixed")).toBe(false)
    expect(credentialSupportsModelSynchronization("manual")).toBe(false)
    expect(credentialSupportsModelSynchronization("unavailable")).toBe(false)
    expect(credentialSupportsModelSynchronization(undefined)).toBe(false)
  })
})
