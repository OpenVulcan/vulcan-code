import { describe, it } from "node:test"
import assert from "node:assert/strict"
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises"
import { dirname, join } from "node:path"
import { tmpdir } from "node:os"

import { checkPluginUiStyle, checkUiArchitecture, componentFunctionCodeLines, moduleClassNames } from "./check-ui-architecture.mjs"

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
    await withFixture('import { Select } from "@/ui-kit/primitives/select"\nexport const value = <Select />\n', async (directory) => {
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

  // Verifies new feedback shells fail, baseline shells pass, page kit classes are exempt and stale entries must be removed.
  // 验证新反馈外壳失败、基线外壳通过、页面套件类名豁免，且失效条目必须删除。
  it("ratchets bespoke empty-state and notice shells", async () => {
    // shells is an explicit baseline for the fixture tree.
    // shells 是样本源码树的显式基线。
    const shells = new Set(["legacy-empty"])
    await withFixture('export const a = <div className="legacy-empty page-notice" />\n', async (directory) => {
      assert.deepEqual(await checkUiArchitecture(directory, new Map(), shells), [])
    })
    await withFixture('export const a = <div className="legacy-empty" />\nexport const b = <p className="fresh-notice" />\n', async (directory) => {
      assert.deepEqual(await checkUiArchitecture(directory, new Map(), shells), [
        'fixture.tsx: bespoke feedback shell "fresh-notice"; use PageEmptyState or PageNotice from @/components/page',
      ])
    })
    await withFixture('export const a = <div className="page-empty-state" />\n', async (directory) => {
      assert.deepEqual(await checkUiArchitecture(directory, new Map(), shells), [
        "legacy-empty: feedback shell is no longer used; remove its legacyFeedbackShells entry",
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

// withPluginFixture creates one plugins tree with a single UI module and a plugin stylesheet vocabulary.
// withPluginFixture 创建只含单个 UI 模块的插件目录树与插件样式表词汇。
// Parameters: files maps "<plugin>/ui/<file>" to module text; run receives the root and vocabulary path.
// 参数：files 将 "<plugin>/ui/<file>" 映射到模块文本；run 接收根目录与词汇表路径。
async function withPluginFixture(files, run) {
  // directory is the exact temporary plugins root.
  // directory 是精确的临时插件根目录。
  const directory = await mkdtemp(join(tmpdir(), "vulcan-plugin-style-"))
  try {
    for (const [name, source] of Object.entries(files)) {
      await mkdir(dirname(join(directory, "plugins", name)), { recursive: true })
      await writeFile(join(directory, "plugins", name), source, "utf8")
    }
    await writeFile(join(directory, "vocabulary.css"), "/* .commented-out */\n.plugin-demo-cell, .plugin-demo-cell.is-active { color: var(--primary); }\n", "utf8")
    await run(join(directory, "plugins"), join(directory, "vocabulary.css"))
  } finally {
    await rm(directory, { recursive: true, force: true })
  }
}

describe("checkPluginUiStyle", () => {
  // Verifies page kit composition with vocabulary classes and token colors passes.
  // 验证使用词汇表类名与主题变量颜色的页面套件组合可以通过。
  it("accepts kit components, vocabulary classes and theme tokens", async () => {
    const source = [
      "// A comment may mention document.body, #fff and position: fixed.",
      'export const view = (h, kit, active) => h(kit.PageSection, { title: "Demo" },',
      '  h("span", { className: active ? "plugin-demo-cell is-active" : "plugin-demo-cell", style: { color: "var(--chart-1)", width: "50%" } }),',
      '  h(kit.Button, { onClick: () => window.open("https://example.com/#section") }, "Open"))',
      "",
    ].join("\n")
    await withPluginFixture({ "demo/ui/index.mjs": source, "demo/ui/index.test.mjs": "document.body.innerHTML = '<style></style>'\n" }, async (root, vocabulary) => {
      assert.deepEqual(await checkPluginUiStyle(root, vocabulary), [])
    })
  })

  // Verifies each custom-style limit produces its own actionable diagnostic.
  // 验证每条自定义样式限制都会产生各自的可操作诊断。
  it("rejects stylesheets, DOM access, color literals, overlays and pixel fonts", async () => {
    const source = [
      'export const a = (h) => h("style", null, ".x{}")',
      "export const b = () => document.querySelector(\"body\")",
      'export const c = { color: "#ff0000" }',
      'export const d = { position: "fixed", zIndex: 50 }',
      "export const e = { fontSize: 13 }",
      "",
    ].join("\n")
    await withPluginFixture({ "demo/ui/index.mjs": source }, async (root, vocabulary) => {
      assert.deepEqual(await checkPluginUiStyle(root, vocabulary), [
        "demo/ui/index.mjs: style elements and stylesheet injection are forbidden",
        "demo/ui/index.mjs: direct DOM access is forbidden; render through sdk.React and sdk.components",
        "demo/ui/index.mjs: color literals are forbidden; use host theme tokens such as var(--primary)",
        "demo/ui/index.mjs: fixed or sticky positioning and z-index are reserved for the host shell",
        "demo/ui/index.mjs: pixel font sizes are forbidden; use host typography tokens such as var(--vcn-font-size-sm)",
      ])
    })
  })

  // Verifies host utility classes and unpublished class names are rejected once each.
  // 验证宿主工具类与未发布类名会被拒绝，且每个只报告一次。
  it("rejects class names outside the published vocabulary", async () => {
    const source = 'export const a = (h) => [h("div", { className: "p-4 text-sm" }), h("div", { className: `plugin-demo-cell ${state} p-4` }), h("i", { className: "commented-out" })]\n'
    await withPluginFixture({ "demo/ui/index.mjs": source }, async (root, vocabulary) => {
      assert.deepEqual(await checkPluginUiStyle(root, vocabulary), [
        "demo/ui/index.mjs: class names p-4, text-sm, commented-out are not defined in styles/plugin-host-components.css; use page kit components or add a plugin-scoped rule there",
      ])
    })
    assert.deepEqual(moduleClassNames('h("a", { className: cond ? "one two" : `three ${x}` })'), ["one", "two", "three"])
  })
})
