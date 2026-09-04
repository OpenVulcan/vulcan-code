import { describe, expect, it } from "vitest"

import { setupBindingTargetProjectId } from "./vmm-setup-dialog"

// DEFAULT_DECISION represents one unedited row that follows the global fixed-project selection.
// DEFAULT_DECISION 表示一条仍跟随全局固定项目选择的未编辑配置行。
const DEFAULT_DECISION = {
  action: "bind_existing" as const,
  projectName: "project-a",
  targetProjectId: "__global_fixed_vmm_project__",
}

describe("VMM setup fixed-project row materialization", () => {
  it("uses the latest native fixed project instead of the initialization-time value", () => {
    // targetProjectId represents the new top-level selection made after row initialization.
    // targetProjectId 表示配置行初始化后重新选择的顶部默认项目。
    const targetProjectId = setupBindingTargetProjectId({
      mode: "native",
      bindingMode: "fixed",
      decision: DEFAULT_DECISION,
      nativeFixedProjectId: "7",
    })

    expect(targetProjectId).toBe(7)
  })

  it("keeps an external pending fixed target unresolved until remote creation", () => {
    // targetProjectId must remain null so the caller can materialize the authoritative remote ID.
    // targetProjectId 必须保持 null，使调用方可以物化权威远端 ID。
    const targetProjectId = setupBindingTargetProjectId({
      mode: "external",
      bindingMode: "fixed",
      decision: DEFAULT_DECISION,
      nativeFixedProjectId: "1",
    })

    expect(targetProjectId).toBeNull()
  })

  it("preserves an explicit alternate existing project", () => {
    // targetProjectId records an explicit row override that must not follow later default changes.
    // targetProjectId 记录一条不得跟随后续默认值变化的显式逐行覆盖。
    const targetProjectId = setupBindingTargetProjectId({
      mode: "native",
      bindingMode: "fixed",
      decision: { ...DEFAULT_DECISION, targetProjectId: "5" },
      nativeFixedProjectId: "7",
    })

    expect(targetProjectId).toBe(5)
  })
})
