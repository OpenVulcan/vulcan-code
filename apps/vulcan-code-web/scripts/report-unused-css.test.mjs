import { describe, it } from "node:test"
import assert from "node:assert/strict"
import { mkdtemp, rm, writeFile } from "node:fs/promises"
import { join } from "node:path"
import { tmpdir } from "node:os"

import { findUnusedCssClasses, selectorClasses } from "./report-unused-css.mjs"

describe("selectorClasses", () => {
  // Verifies only selector classes count; values, comments, strings and escaped utilities are ignored.
  // 验证只统计选择器中的类名；声明值、注释、字符串与转义工具类均被忽略。
  it("reads classes from rule preludes only", () => {
    // css mixes every construct that contains a dot outside a selector.
    // css 混合了所有在选择器之外包含点号的写法。
    const css = '/* .commented */\n.card > .title:hover, .w-1\\.5 { margin: .5rem; background: url("a.png"); }\n@media (min-width: 1px) { .narrow { gap: 0; } }\n'
    assert.deepEqual([...selectorClasses(css)].sort(), ["card", "narrow", "title"])
  })
})

describe("findUnusedCssClasses", () => {
  // Verifies referenced, unreferenced and template-built classes land in the right groups.
  // 验证已引用、未引用与模板拼接的类名分别归入正确分组。
  it("separates unused classes from template-prefixed candidates", async () => {
    // directory is the exact temporary source root.
    // directory 是精确的临时源码根目录。
    const directory = await mkdtemp(join(tmpdir(), "vulcan-unused-css-"))
    try {
      await writeFile(join(directory, "view.tsx"), "export const view = (state: string) => <div className={`used status-${state}`} />\n", "utf8")
      await writeFile(join(directory, "view.css"), ".used {} .status-ready {} .orphan {}\n", "utf8")
      const { unused, dynamic } = await findUnusedCssClasses(directory)
      assert.deepEqual([...unused], [["orphan", ["view.css"]]])
      assert.deepEqual([...dynamic], [["status-ready", ["view.css"]]])
    } finally {
      await rm(directory, { recursive: true, force: true })
    }
  })
})
