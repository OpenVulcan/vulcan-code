import assert from "node:assert/strict"
import { spawnSync } from "node:child_process"
import { createRequire } from "node:module"
import { dirname, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import test from "node:test"
import { ESLint } from "eslint"

// webRoot makes API and CLI checks use the actual package configuration from any working directory.
// webRoot 使 API 与命令行检查从任意工作目录使用真实包配置。
const webRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..")
// require resolves the pinned local ESLint installation without invoking a global executable.
// require 解析锁定的本地 ESLint 安装，不调用全局程序。
const require = createRequire(import.meta.url)
// cliPath resolves the installed package entry instead of assuming a node_modules layout.
// cliPath 解析已安装包的入口，不假设 node_modules 布局。
const cliPath = resolve(dirname(require.resolve("eslint/package.json")), "bin/eslint.js")

// Checks known semantic violations with the production config; the input and expected rule form the fixture.
// 使用生产配置检查已知语义违规；输入与预期规则组成夹具。
for (const [rule, source] of [
  ["no-async-promise-executor", "new Promise(async (resolve) => { resolve(1) })"],
  ["no-unsafe-finally", "function unsafe() { try { throw Error('lost') } finally { return 1 } }"],
  ["no-constant-binary-expression", "const value = {} || fallback"],
  ["no-unsafe-optional-chaining", "const result = (object?.method)()"],
]) {
  test(`production lint rejects ${rule}`, async () => {
    const lint = new ESLint({ cwd: webRoot })
    const [result] = await lint.lintText(source, { filePath: "src/lint-fixture.ts" })
    assert.ok(result.messages.some((message) => message.ruleId === rule), `${rule} must remain enabled`)
  })
}

test("lint CLI fails a violation and accepts valid typed source", () => {
  for (const [source, expectedStatus] of [
    ["new Promise(async (resolve) => { resolve(1) })", 1],
    ["const value: number = 1; export { value }", 0],
  ]) {
    const result = spawnSync(process.execPath, [cliPath, "--stdin", "--stdin-filename", "src/lint-fixture.ts", "--max-warnings", "0"], {
      cwd: webRoot, encoding: "utf8", input: source,
    })
    assert.equal(result.status, expectedStatus, `${result.error ?? ""}\n${result.stdout}\n${result.stderr}`)
  }
})
