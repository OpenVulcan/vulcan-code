import { mkdir, readFile, writeFile } from "node:fs/promises"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"

// scriptDirectory is the stable package script location.
// scriptDirectory 是稳定的包脚本位置。
const scriptDirectory = dirname(fileURLToPath(import.meta.url))
// webRoot is the WebUI package root.
// webRoot 是 WebUI 包根目录。
const webRoot = resolve(scriptDirectory, "..")
// sourcePath is the exact owner of file-icon names.
// sourcePath 是文件图标名称的精确所有者。
const sourcePath = join(webRoot, "src/components/conversation/conversation-file-type-icon.tsx")
// catalogPath is the development-only full Material Icon Theme catalog.
// catalogPath 是仅供开发期使用的完整 Material Icon Theme 目录。
const catalogPath = join(webRoot, "node_modules/@iconify-json/material-icon-theme/icons.json")
// outputPath is the generated minimal runtime collection.
// outputPath 是生成后的最小运行时集合。
const outputPath = join(webRoot, "src/generated/file-icon-collection.ts")

// generateFileIconCollection extracts only names declared by the file presentation contract.
// generateFileIconCollection 仅提取文件展示契约明确声明的图标名称。
async function generateFileIconCollection() {
  // source contains every exact runtime icon reference.
  // source 包含每个精确的运行时图标引用。
  const source = await readFile(sourcePath, "utf8")
  // names is the stable unique icon-name inventory.
  // names 是稳定且去重的图标名称清单。
  const names = [...new Set([...source.matchAll(/material-icon-theme:([a-z0-9-]+)/g)].map((match) => match[1]))].sort()
  // catalog contains the full development package source.
  // catalog 包含完整的开发包来源。
  const catalog = JSON.parse(await readFile(catalogPath, "utf8"))
  // icons contains only exact file-presentation dependencies.
  // icons 只包含精确的文件展示依赖。
  const icons = Object.fromEntries(names.map((name) => {
    // icon is the exact catalog body required by one declared name.
    // icon 是一个已声明名称所需的精确目录主体。
    const icon = catalog.icons[name]
    if (!icon) {
      throw new Error(`Missing material-icon-theme icon: ${name}`)
    }
    return [name, icon]
  }))
  // generated contains deterministic TypeScript source and provenance comments.
  // generated 包含确定性的 TypeScript 源码与来源注释。
  const generated = `// Generated from the exact file icon names declared by conversation-file-type-icon.tsx.\n// 根据 conversation-file-type-icon.tsx 声明的精确文件图标名称生成。\nexport const FILE_ICON_COLLECTION = ${JSON.stringify({ prefix: catalog.prefix, icons }, null, 2)}\n`
  await mkdir(dirname(outputPath), { recursive: true })
  await writeFile(outputPath, generated, "utf8")
  process.stdout.write(`Generated ${names.length} file icons.\n`)
}

await generateFileIconCollection()
