import { readdir, readFile } from "node:fs/promises"
import { dirname, extname, join, relative, resolve } from "node:path"
import { fileURLToPath } from "node:url"

// scriptDirectory is the stable location used to resolve package sources.
// scriptDirectory 是用于解析包源码的稳定位置。
const scriptDirectory = dirname(fileURLToPath(import.meta.url))
// sourceRoot is the WebUI source tree whose stylesheets and components are compared.
// sourceRoot 是对比其样式表与组件的 WebUI 源码树。
const sourceRoot = resolve(scriptDirectory, "../src")
// markupExtensions are the source formats that can reference class names.
// markupExtensions 是可以引用类名的源码格式。
const markupExtensions = new Set([".ts", ".tsx"])

// collectFiles recursively returns files with the given extensions in stable order.
// collectFiles 递归返回指定扩展名的文件，并保持稳定顺序。
async function collectFiles(directory, extensions) {
  // entries contains the exact directory children.
  // entries 包含精确的目录子项。
  const entries = await readdir(directory, { withFileTypes: true })
  // files accumulates matching descendants.
  // files 累积匹配的后代文件。
  const files = []
  for (const entry of entries) {
    // path is the exact current child path.
    // path 是精确的当前子路径。
    const path = join(directory, entry.name)
    if (entry.isDirectory()) {
      files.push(...await collectFiles(path, extensions))
    } else if (extensions.has(extname(entry.name))) {
      files.push(path)
    }
  }
  return files.sort()
}

// selectorClasses returns the plain class names used in a stylesheet's selectors, ignoring declarations and escaped utilities.
// selectorClasses 返回样式表选择器中使用的普通类名，忽略声明块与转义的工具类。
export function selectorClasses(css) {
  // stripped removes comments and quoted strings so their dots never look like selectors.
  // stripped 移除注释与引号字符串，避免其中的点号被误认为选择器。
  const stripped = css.replace(/\/\*[\s\S]*?\*\//g, "").replace(/"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'/g, '""')
  // classes collects every class found in a rule prelude.
  // classes 收集规则前导部分中出现的每个类名。
  const classes = new Set()
  // prelude is the text since the previous brace; it is a selector when a block opens after it.
  // prelude 是自上一个花括号以来的文本；其后开启块时即为选择器。
  let prelude = ""
  for (const character of stripped) {
    if (character === "{") {
      if (!prelude.trim().startsWith("@")) {
        for (const match of prelude.matchAll(/(?<![\w\\-])\.(-?[A-Za-z_][\w-]*)(?![\w\\-])/g)) classes.add(match[1])
      }
      prelude = ""
    } else if (character === "}" || character === ";") {
      prelude = ""
    } else {
      prelude += character
    }
  }
  return classes
}

// findUnusedCssClasses lists stylesheet classes that no TypeScript source mentions, split by whether a template prefix might build them.
// findUnusedCssClasses 列出没有任何 TypeScript 源码提及的样式类，并按是否可能由模板前缀拼出进行区分。
export async function findUnusedCssClasses(root = sourceRoot) {
  // markup is the concatenated component and logic source that may reference classes.
  // markup 是可能引用类名的组件与逻辑源码拼接结果。
  const markup = (await Promise.all((await collectFiles(root, markupExtensions)).map((path) => readFile(path, "utf8")))).join("\n")
  // words holds every class-shaped token in the markup sources.
  // words 保存标记源码中所有形似类名的标记。
  const words = new Set(markup.match(/[A-Za-z_][\w-]*/g) ?? [])
  // templatePrefixes are literal fragments such as `status-${state}` that can assemble class names at runtime.
  // templatePrefixes 是诸如 `status-${state}` 的字面片段，可在运行时拼出类名。
  const templatePrefixes = [...new Set([...markup.matchAll(/([A-Za-z_][\w-]*-)\$\{/g)].map((match) => match[1]))]
  // unused and dynamic map each class to the stylesheets that declare it.
  // unused 与 dynamic 将每个类名映射到声明它的样式表。
  const unused = new Map()
  const dynamic = new Map()
  for (const path of await collectFiles(root, new Set([".css"]))) {
    // name is the package-relative stylesheet path used in the report.
    // name 是报告中使用的包相对样式表路径。
    const name = relative(root, path).replace(/\\/g, "/")
    for (const className of selectorClasses(await readFile(path, "utf8"))) {
      if (words.has(className)) continue
      // target receives classes a template prefix could produce; they need manual review rather than deletion.
      // target 接收可能由模板前缀产生的类名；这些类需要人工复核而非直接删除。
      const target = templatePrefixes.some((prefix) => className.startsWith(prefix)) ? dynamic : unused
      target.set(className, [...(target.get(className) ?? []), name])
    }
  }
  return { unused, dynamic }
}

// runReport prints candidates without failing, because third-party DOM and plugin markup also consume these classes.
// runReport 只输出候选项而不使进程失败，因为第三方 DOM 与插件标记同样会使用这些类名。
async function runReport() {
  // unused and dynamic are the current candidate groups.
  // unused 与 dynamic 是当前的候选分组。
  const { unused, dynamic } = await findUnusedCssClasses()
  // format renders one group sorted by stylesheet, then class.
  // format 按样式表、再按类名排序渲染一个分组。
  const format = (group) => [...group].map(([className, files]) => `${files.join(", ")}: .${className}`).sort().join("\n")
  process.stdout.write(`Unused CSS report (heuristic; review before deleting — classes may be set by third-party libraries or plugins).\n`)
  process.stdout.write(`\nNot referenced by any src/**/*.ts(x) token (${unused.size}):\n${format(unused) || "(none)"}\n`)
  process.stdout.write(`\nOnly reachable through a template prefix (${dynamic.size}):\n${format(dynamic) || "(none)"}\n`)
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await runReport()
}
