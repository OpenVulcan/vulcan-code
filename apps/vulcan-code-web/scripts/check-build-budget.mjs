import { readFile, readdir, stat } from "node:fs/promises"
import { gzipSync } from "node:zlib"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"

// scriptDirectory is the stable location used to resolve build output and budget configuration.
// scriptDirectory 是用于解析构建输出与预算配置的稳定位置。
const scriptDirectory = dirname(fileURLToPath(import.meta.url))
// webRoot is the package root that owns dist.
// webRoot 是拥有 dist 的包根目录。
const webRoot = resolve(scriptDirectory, "..")

// measureFile returns raw and gzip byte counts for one emitted asset.
// measureFile 返回单个构建资产的原始与 gzip 字节数。
export async function measureFile(path) {
  // bytes contains the exact emitted file content.
  // bytes 包含精确的构建文件内容。
  const bytes = await readFile(path)
  return { raw: bytes.byteLength, gzip: gzipSync(bytes).byteLength }
}

// evaluateBuildBudget compares named measurements with explicit limits.
// evaluateBuildBudget 将具名测量值与明确限制进行比较。
export function evaluateBuildBudget(measurements, budgets) {
  // failures accumulates every exact exceeded dimension for one actionable report.
  // failures 累积每个精确超限维度，以生成可操作报告。
  const failures = []
  for (const [name, limits] of Object.entries(budgets)) {
    // measurement is the exact named build result required by the configuration.
    // measurement 是配置要求的精确具名构建结果。
    const measurement = measurements[name]
    if (!measurement) {
      failures.push(`${name}: asset not found`)
      continue
    }
    for (const [dimension, limit] of Object.entries(limits)) {
      if (measurement[dimension] > limit) {
        failures.push(`${name}.${dimension}: ${measurement[dimension]} > ${limit}`)
      }
    }
  }
  return failures
}

// findSingleAsset resolves exactly one hashed asset by prefix and suffix.
// findSingleAsset 根据前缀与后缀精确解析一个带哈希资产。
function findSingleAsset(assetNames, prefix, suffix) {
  // matches contains only emitted assets owned by the requested logical chunk.
  // matches 只包含请求逻辑分块拥有的构建资产。
  const matches = assetNames.filter((name) => name.startsWith(prefix) && name.endsWith(suffix))
  if (matches.length !== 1) {
    throw new Error(`Expected one ${prefix}*${suffix} asset, found ${matches.length}.`)
  }
  return matches[0]
}

// collectBuildMeasurements reads the current production output without relying on hashed filenames.
// collectBuildMeasurements 在不依赖哈希文件名的情况下读取当前生产构建输出。
export async function collectBuildMeasurements(distDirectory = join(webRoot, "dist")) {
  // assetsDirectory owns all Vite emitted assets.
  // assetsDirectory 拥有全部 Vite 构建资产。
  const assetsDirectory = join(distDirectory, "assets")
  // assetNames is the exact emitted asset inventory.
  // assetNames 是精确的构建资产清单。
  const assetNames = await readdir(assetsDirectory)
  // indexHtml contains the boot entry reference selected by Vite.
  // indexHtml 包含 Vite 选择的启动入口引用。
  const indexHtml = await readFile(join(distDirectory, "index.html"), "utf8")
  // bootMatch captures the exact module script asset.
  // bootMatch 捕获精确的模块脚本资产。
  const bootMatch = /<script[^>]+src="\/assets\/([^"]+\.js)"/.exec(indexHtml)
  if (!bootMatch) {
    throw new Error("Production index.html does not contain a module entry asset.")
  }
  // deferredJavaScriptNames excludes the boot and workbench entries for the independent deferred ceiling.
  // deferredJavaScriptNames 为独立延迟分块上限排除启动与工作台入口。
  const deferredJavaScriptNames = assetNames.filter((name) => name.endsWith(".js") && name !== bootMatch[1] && !name.startsWith("WorkbenchApp-"))
  // deferredMeasurements contains exact raw sizes used to find the largest deferred chunk.
  // deferredMeasurements 包含用于查找最大延迟分块的精确原始大小。
  const deferredMeasurements = await Promise.all(deferredJavaScriptNames.map(async (name) => ({ name, ...(await stat(join(assetsDirectory, name))) })))
  // largestDeferred is the single largest non-workbench JavaScript chunk.
  // largestDeferred 是最大单个非工作台 JavaScript 分块。
  const largestDeferred = deferredMeasurements.sort((left, right) => right.size - left.size)[0]
  return {
    bootEntry: await measureFile(join(assetsDirectory, bootMatch[1])),
    workbench: await measureFile(join(assetsDirectory, findSingleAsset(assetNames, "WorkbenchApp-", ".js"))),
    workbenchCss: await measureFile(join(assetsDirectory, findSingleAsset(assetNames, "WorkbenchApp-", ".css"))),
    projectIconLucideCatalog: await measureFile(join(assetsDirectory, findSingleAsset(assetNames, "project-icon-catalog-lucide-", ".js"))),
    projectIconMaterialCatalog: await measureFile(join(assetsDirectory, findSingleAsset(assetNames, "project-icon-catalog-material-", ".js"))),
    projectIconSkillCatalog: await measureFile(join(assetsDirectory, findSingleAsset(assetNames, "project-icon-catalog-skill-", ".js"))),
    largestDeferredJavaScript: { raw: largestDeferred.size, name: largestDeferred.name },
  }
}

// runBuildBudgetCheck validates current dist and prints stable measurements for CI and review.
// runBuildBudgetCheck 验证当前 dist，并为 CI 与评审输出稳定测量值。
async function runBuildBudgetCheck() {
  // budgets is the reviewed limit set committed with the package.
  // budgets 是随包提交并经过评审的限制集合。
  const budgets = JSON.parse(await readFile(join(scriptDirectory, "webui-build-budget.json"), "utf8"))
  // measurements contains current exact production artifact sizes.
  // measurements 包含当前精确的生产构建资产大小。
  const measurements = await collectBuildMeasurements()
  // failures contains every exceeded or missing budget dimension.
  // failures 包含每个超限或缺失的预算维度。
  const failures = evaluateBuildBudget(measurements, budgets)
  for (const [name, measurement] of Object.entries(measurements)) {
    process.stdout.write(`${name}: raw=${measurement.raw}${measurement.gzip === undefined ? "" : ` gzip=${measurement.gzip}`}${measurement.name ? ` asset=${measurement.name}` : ""}\n`)
  }
  if (failures.length > 0) {
    throw new Error(`WebUI build budget failed:\n- ${failures.join("\n- ")}`)
  }
  process.stdout.write("WebUI build budget passed.\n")
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await runBuildBudgetCheck()
}
