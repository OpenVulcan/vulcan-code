import { describe, it } from "node:test"
import assert from "node:assert/strict"
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises"
import { dirname, join } from "node:path"
import { tmpdir } from "node:os"

import { checkUiArchitecture, componentFunctionCodeLines } from "./check-ui-architecture.mjs"

// componentSource builds one component function whose body spans the requested code lines.
// componentSource 构建一个函数体占用指定代码行数的组件函数。
// Parameters: name is the component; codeLines counts the declaration and closing lines; padding adds blank and comment lines.
// 参数：name 是组件名；codeLines 包含声明行与结束行；padding 追加空行与注释行。
function componentSource(name, codeLines, padding = 0) {
  // body holds the statements between the declaration and closing brace.
  // body 保存声明与结束大括号之间的语句。
  const body = Array.from({ length: codeLines - 2 }, (_, index) => `  const value${index} = ${index}`)
  // filler contains lines the measurement must ignore.
  // filler 包含测量必须忽略的行。
  const filler = Array.from({ length: padding }, (_, index) => (index % 3 === 0 ? "" : index % 3 === 1 ? "  // note" : "      {/* note */}"))
  return [`export function ${name}() {`, ...filler, ...body, "}", ""].join("\n")
}

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

  // Verifies blank and comment-only lines do not count toward the component limit.
  // 验证空行与纯注释行不计入组件上限。
  it("measures component code lines without blanks or comments", () => {
    assert.deepEqual(componentFunctionCodeLines(componentSource("Panel", 400, 90)), [{ name: "Panel", codeLines: 400 }])
    assert.deepEqual(
      componentFunctionCodeLines("export const Row = React.memo(function Row() {\n  return null\n});\nfunction helper() {\n}\n"),
      [{ name: "Row", codeLines: 3 }],
    )
  })

  // Verifies a new component over the limit fails while one at the limit passes.
  // 验证新超限组件会失败，而恰好达到上限的组件可以通过。
  it("rejects component functions over the code-line limit", async () => {
    await withFixture(componentSource("Panel", 400), async (directory) => {
      assert.deepEqual(await checkUiArchitecture(directory), [])
    })
    await withFixture(componentSource("Panel", 401), async (directory) => {
      assert.deepEqual(await checkUiArchitecture(directory), [
        "fixture.tsx#Panel: component function has 401 code lines (limit 400); extract hooks or child components",
      ])
    })
  })

  // Verifies ratchet entries allow shrinking, reject growth and must be removed once the component fits.
  // 验证棘轮条目允许缩小、拒绝增长，且组件回到上限内后必须删除条目。
  it("ratchets oversized component ceilings", async () => {
    // ceilings is an explicit ratchet table for the fixture tree.
    // ceilings 是样本源码树的显式棘轮表。
    const ceilings = new Map([["fixture.tsx#Panel", 450]])
    await withFixture(componentSource("Panel", 440), async (directory) => {
      assert.deepEqual(await checkUiArchitecture(directory, ceilings), [])
    })
    await withFixture(componentSource("Panel", 451), async (directory) => {
      assert.deepEqual(await checkUiArchitecture(directory, ceilings), [
        "fixture.tsx#Panel: oversized component grew to 451 code lines (ratchet ceiling 450); extract hooks or child components before adding more",
      ])
    })
    await withFixture(componentSource("Panel", 400), async (directory) => {
      assert.deepEqual(await checkUiArchitecture(directory, ceilings), [
        "fixture.tsx#Panel: component no longer exceeds 400 code lines or was renamed; remove its oversizedComponentCeilings entry",
      ])
    })
  })

  // Verifies tests cannot assert on TypeScript source text while other fixture reads stay allowed.
  // 验证测试不能断言 TypeScript 源码文本，而其他样本读取仍被允许。
  it("rejects tests that read TypeScript source text", async () => {
    // failure is the expected diagnostic for a source-reading test.
    // failure 是读取源码的测试应得到的诊断。
    const failure = "panel.test.ts: tests must not read TypeScript source text; assert rendered behavior or exported contracts instead"
    await withFixture('import { readFileSync } from "node:fs"\nconst source = readFileSync("src/panel.tsx", "utf8")\n', async (directory) => {
      assert.deepEqual(await checkUiArchitecture(directory), [failure])
    }, "panel.test.ts")
    await withFixture('import source from "./panel.tsx?raw"\n', async (directory) => {
      assert.deepEqual(await checkUiArchitecture(directory), [failure])
    }, "panel.test.ts")
    await withFixture('import { readFileSync } from "node:fs"\nconst data = readFileSync("fixtures/panel.json", "utf8")\n', async (directory) => {
      assert.deepEqual(await checkUiArchitecture(directory), [])
    }, "panel.test.ts")
  })
})
