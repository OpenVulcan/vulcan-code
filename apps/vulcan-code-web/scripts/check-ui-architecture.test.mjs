import { describe, it } from "node:test"
import assert from "node:assert/strict"
import { mkdtemp, rm, writeFile } from "node:fs/promises"
import { join } from "node:path"
import { tmpdir } from "node:os"

import { checkUiArchitecture } from "./check-ui-architecture.mjs"

// withFixture creates and removes one isolated architecture-check source tree.
// withFixture 创建并移除一个隔离的架构检查源码树。
async function withFixture(source, run) {
  // directory is the exact temporary fixture root.
  // directory 是精确的临时样本根目录。
  const directory = await mkdtemp(join(tmpdir(), "vulcan-ui-architecture-"))
  try {
    await writeFile(join(directory, "fixture.tsx"), source, "utf8")
    await run(directory)
  } finally {
    await rm(directory, { recursive: true, force: true })
  }
}

describe("checkUiArchitecture", () => {
  // Verifies shared Select usage passes the architecture guard.
  // 验证共享 Select 用法可以通过架构守卫。
  it("accepts shared controls", async () => {
    await withFixture('import { Select } from "@/components/ui/select"\nexport const value = <Select />\n', async (directory) => {
      assert.deepEqual(await checkUiArchitecture(directory), [])
    })
  })

  // Verifies native selects produce an actionable file-specific failure.
  // 验证原生选择框会产生具体到文件的可操作失败。
  it("rejects native selects", async () => {
    await withFixture("export const value = <select />\n", async (directory) => {
      assert.deepEqual(await checkUiArchitecture(directory), ["fixture.tsx: native <select> is forbidden; use the shared Select contract"])
    })
  })

  // Verifies full icon catalogs cannot return to ordinary runtime modules.
  // 验证完整图标目录不能回流到普通运行时模块。
  it("rejects full icon catalog imports", async () => {
    await withFixture('import { icons } from "@iconify-json/skill-icons"\nexport { icons }\n', async (directory) => {
      assert.deepEqual(await checkUiArchitecture(directory), ["fixture.tsx: full icon catalogs are only allowed in their source-isolated deferred catalog"])
    })
  })
})
