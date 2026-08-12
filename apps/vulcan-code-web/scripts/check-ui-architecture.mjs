import { readdir, readFile } from "node:fs/promises"
import { dirname, extname, join, relative, resolve } from "node:path"
import { fileURLToPath } from "node:url"

// scriptDirectory is the stable location used to resolve package sources.
// scriptDirectory 是用于解析包源码的稳定位置。
const scriptDirectory = dirname(fileURLToPath(import.meta.url))
// sourceRoot is the WebUI source tree checked by architectural guards.
// sourceRoot 是架构守卫检查的 WebUI 源码树。
const sourceRoot = resolve(scriptDirectory, "../src")
// sourceExtensions contains the source formats relevant to UI architecture.
// sourceExtensions 包含与 UI 架构相关的源码格式。
const sourceExtensions = new Set([".ts", ".tsx", ".css"])

// collectSourceFiles recursively returns relevant source files in stable order.
// collectSourceFiles 递归返回稳定排序的相关源码文件。
async function collectSourceFiles(directory) {
  // entries contains the exact directory children.
  // entries 包含精确的目录子项。
  const entries = await readdir(directory, { withFileTypes: true })
  // files accumulates relevant descendants.
  // files 累积相关后代文件。
  const files = []
  for (const entry of entries) {
    // path is the exact current child path.
    // path 是精确的当前子路径。
    const path = join(directory, entry.name)
    if (entry.isDirectory()) {
      files.push(...await collectSourceFiles(path))
    } else if (sourceExtensions.has(extname(entry.name))) {
      files.push(path)
    }
  }
  return files.sort()
}

// checkUiArchitecture validates boundaries that must not regress through ordinary component work.
// checkUiArchitecture 验证普通组件工作不得破坏的架构边界。
export async function checkUiArchitecture(root = sourceRoot) {
  // files is the exact source inventory under review.
  // files 是当前评审的精确源码清单。
  const files = await collectSourceFiles(root)
  // failures accumulates file-specific actionable violations.
  // failures 累积具体到文件的可操作违规项。
  const failures = []
  for (const path of files) {
    // name is the package-relative path used in diagnostics.
    // name 是诊断使用的包相对路径。
    const name = relative(root, path).replace(/\\/g, "/")
    // source is the exact current file text.
    // source 是精确的当前文件文本。
    const source = await readFile(path, "utf8")
    if (/\<select(?:\s|>)/.test(source)) {
      failures.push(`${name}: native <select> is forbidden; use the shared Select contract`)
    }
    if (/components\/ui\/(item|resizable|skeleton)/.test(source)) {
      failures.push(`${name}: imports a removed unused UI component`)
    }
    // ownsDeferredIconCatalog identifies the two exact source-isolated catalog chunks.
    // ownsDeferredIconCatalog 标识两个按来源隔离的精确延迟图标目录分块。
    const ownsDeferredIconCatalog = /^components\/project\/project-icon-catalog-(material|skill)\.ts$/.test(name)
    if (!ownsDeferredIconCatalog && /@iconify-json\/(material-icon-theme|skill-icons)/.test(source)) {
      failures.push(`${name}: full icon catalogs are only allowed in their source-isolated deferred catalog`)
    }
    if (!name.includes(".test.") && /focus-visible:ring-3|aria-invalid:ring-3/.test(source)) {
      failures.push(`${name}: three-pixel control focus rings are forbidden`)
    }
  }
  return failures
}

// runUiArchitectureCheck prints every violation and fails the calling process.
// runUiArchitectureCheck 输出每个违规项并使调用进程失败。
async function runUiArchitectureCheck() {
  // failures contains the current exact architecture violations.
  // failures 包含当前精确的架构违规项。
  const failures = await checkUiArchitecture()
  if (failures.length > 0) {
    throw new Error(`WebUI architecture check failed:\n- ${failures.join("\n- ")}`)
  }
  process.stdout.write("WebUI architecture check passed.\n")
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await runUiArchitectureCheck()
}
