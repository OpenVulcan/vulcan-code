import { describe, it } from "node:test"
import assert from "node:assert/strict"

import { collectStaticImportFiles, evaluateBuildBudget } from "./check-build-budget.mjs"

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

describe("collectStaticImportFiles", () => {
  // The initial graph counts the eager English catalog while excluding on-demand languages and features.
  // 首屏导入图计入预加载英文目录，并排除按需加载的语言与功能分块。
  it("includes the eager fallback catalog and excludes dynamic imports", () => {
    const manifest = {
      "index.html": { file: "assets/index.js", dynamicImports: ["_zh.js"] },
      "src/app/workbench-app.tsx": {
        file: "assets/workbench-app.js",
        imports: ["index.html", "_en.js"],
        dynamicImports: ["src/optional.tsx"],
      },
      "_zh.js": { file: "assets/i18n-messages.zh-CN.js" },
      "_en.js": { file: "assets/i18n-messages.en-US.js" },
      "src/optional.tsx": { file: "assets/optional.js" },
    }
    assert.deepEqual(collectStaticImportFiles(manifest, ["index.html", "src/app/workbench-app.tsx"]), [
      "assets/i18n-messages.en-US.js",
      "assets/index.js",
      "assets/workbench-app.js",
    ])
  })

  // A broken manifest edge must fail the budget instead of silently undercounting.
  // 断裂的清单边必须使预算失败，不能静默少计。
  it("rejects missing static imports", () => {
    assert.throws(() => collectStaticImportFiles({ "index.html": { file: "assets/index.js", imports: ["missing"] } }, ["index.html"]), /missing static import/)
  })
})
