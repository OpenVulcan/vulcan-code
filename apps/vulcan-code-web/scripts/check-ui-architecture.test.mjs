import { describe, it } from "node:test"
import assert from "node:assert/strict"
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises"
import { dirname, join } from "node:path"
import { tmpdir } from "node:os"

import { checkUiArchitecture } from "./check-ui-architecture.mjs"

// withFixture creates and removes one isolated architecture-check source tree.
// withFixture 创建并移除一个隔离的架构检查源码树。
async function withFixture(source, run, name = "fixture.tsx") {
  // directory is the exact temporary fixture root.
  // directory 是精确的临时样本根目录。
  const directory = await mkdtemp(join(tmpdir(), "vulcan-ui-architecture-"))
  try {
    await mkdir(dirname(join(directory, name)), { recursive: true })
    await writeFile(join(directory, name), source, "utf8")
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

  // Verifies transcript owners cannot bypass the single scroll controller.
  // 验证转录所有者不能绕过唯一的滚动控制器。
  it("rejects direct transcript scroll writes", async () => {
    // owner is the transcript surface whose scroll writes are delegated.
    // owner 是滚动写入被委托出去的转录界面。
    const owner = "components/conversation/chat-area.tsx"
    await withFixture("export function jump(element) { element.scrollTop = 0 }\n", async (directory) => {
      assert.deepEqual(await checkUiArchitecture(directory), [`${owner}: transcript scroll writes belong to chat-area/conversation-scroll-controller`])
    }, owner)
  })

  // Verifies scroll reads and unrelated viewports stay allowed.
  // 验证滚动读取与无关视口仍被允许。
  it("allows scroll reads in transcript owners and writes elsewhere", async () => {
    await withFixture("export const atTop = (element) => element.scrollTop === 0\n", async (directory) => {
      assert.deepEqual(await checkUiArchitecture(directory), [])
    }, "components/conversation/chat-area.tsx")
    await withFixture("export function reset(element) { element.scrollTo({ top: 0 }) }\n", async (directory) => {
      assert.deepEqual(await checkUiArchitecture(directory), [])
    }, "components/conversation/prompt-list.tsx")
  })
})
