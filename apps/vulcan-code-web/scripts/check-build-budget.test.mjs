import { describe, it } from "node:test"
import assert from "node:assert/strict"

import { evaluateBuildBudget } from "./check-build-budget.mjs"

describe("evaluateBuildBudget", () => {
  // Verifies measurements at or below every limit pass.
  // 验证不超过全部限制的测量值可以通过。
  it("accepts measurements within budget", () => {
    assert.deepEqual(evaluateBuildBudget({ workbench: { raw: 90, gzip: 40 } }, { workbench: { raw: 100, gzip: 50 } }), [])
  })

  // Verifies failures identify the exact asset and dimension.
  // 验证失败会指出精确资产与维度。
  it("reports the exact exceeded dimension", () => {
    assert.deepEqual(evaluateBuildBudget({ workbench: { raw: 101, gzip: 40 } }, { workbench: { raw: 100, gzip: 50 } }), ["workbench.raw: 101 > 100"])
  })

  // Verifies missing required assets cannot silently pass.
  // 验证缺失的必要资产不能静默通过。
  it("reports missing assets", () => {
    assert.deepEqual(evaluateBuildBudget({}, { workbench: { raw: 100 } }), ["workbench: asset not found"])
  })
})
