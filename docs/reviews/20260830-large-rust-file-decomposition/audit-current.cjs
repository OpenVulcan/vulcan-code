// Verify all frozen split families without modifying files; this is evidence, not an automatic code-review verdict.
// 不修改文件地核验全部冻结拆分源码族；这只是证据，不自动代替代码审核结论。
"use strict";
// Read-only primitives and explicitly frozen scope/evidence.
// 只读原语与明确冻结的范围及证据。
const a = require("./audit-decomposition.cjs"), fs = require("node:fs"), path = require("node:path");
const scopes = require("./AUDIT-SCOPE.json"), baseline = require("./AUDIT-BASELINE.json");

/** Return attributes immediately preceding a verified declaration; inputs are source/line, output is token arrays.
 * 返回紧邻已确认声明的属性；输入源码和行号，输出属性词元数组。 */
function attributes(source, line) {
  // Scan backward from the item rather than matching text inside comments or string literals.
  // 从条目向前扫描，避免匹配注释或字符串中的文本。
  const offset = source.split("\n").slice(0, line - 1).join("\n").length;
  const list = a.tokens(source); let i = list.findIndex(token => token.start >= offset) - 1;
  const result = [];
  while (list[i]?.value === "]") {
    let depth = 1, j = i - 1;
    while (j >= 0 && depth) { if (list[j].value === "]") depth++; if (list[j].value === "[") depth--; j--; }
    if (list[j]?.value !== "#") break;
    result.unshift(list.slice(j, i + 1).map(token => token.value)); i = j - 1;
  }
  return result;
}

/** Return test identities/attributes for one exact source; output excludes fixtures and inactive comments.
 * 返回精确源码的测试身份及属性；输出不包含夹具或注释中的伪测试。 */
function tests(source, file) {
  return a.declarations(source, file).flatMap(item => {
    // Attributes determine test identity, never a filename suffix alone.
    // 以属性确定测试身份，不能只凭文件名后缀。
    const attrs = attributes(source, item.start);
    return attrs.some(attr => attr[2] === "test" ||
      (attr[2] === "tokio" && attr.slice(3, 6).join("") === "::test")) ? [{ name: item.name, attrs }] : [];
  });
}

/** Resolve conventional external Rust module declarations; input is a source file, output is exact existing paths.
 * 解析常规外部Rust模块声明；输入源码文件，输出精确存在路径。 */
function modules(file) {
  // All split module declarations use conventional Rust resolution; no candidate is executed or rewritten.
  // 拆分模块均使用Rust常规解析规则；不会执行候选路径或改写源码。
  const list = a.tokens(a.read(file)), parent = file.endsWith("/mod.rs") ? path.posix.dirname(file) : file.slice(0, -3);
  const result = [];
  for (let i = 0; i < list.length - 2; i++) {
    if (list[i].value !== "mod" || list[i + 2].value !== ";") continue;
    // Rust explicitly supports these two forms; exactly one must exist.
    // Rust明确支持这两种模块形式；必须且只能存在一种。
    const name = list[i + 1].value, candidates = [parent + "/" + name + ".rs", parent + "/" + name + "/mod.rs"];
    const existing = candidates.filter(candidate => fs.existsSync(path.join(a.root, candidate)));
    if (existing.length !== 1) throw new Error("Ambiguous or missing module: " + file + " -> " + name);
    result.push(existing[0]);
  }
  return result;
}

/** Project exact unresolved declaration residuals to stable digests; input is an item list, output is comparable evidence.
 * 将精确的剩余声明差异投影为稳定摘要；输入条目列表，输出可比较证据。 */
function residuals(items) {
  return items.map(({ normalized, ...item }) => ({ ...item, hash: a.hash(JSON.stringify(normalized)) }));
}

// Gather each complete family, retaining original/current counts and every reviewed residual.
// 收集每个完整源码族，保留原始/当前数量及全部已经审核的剩余差异。
const reports = [], allFiles = new Set(baseline.flatMap(scope => scope.files.map(file => file.file)));
for (const scope of scopes) {
  // The current scan must match both reviewed lexical residuals and complete file bytes.
  // 当前扫描必须同时匹配已审核词法差异及完整文件字节。
  const expected = baseline.find(item => item.id === scope.id), current = a.familyEvidence(scope);
  const actual = {
    id: current.id,
    files: current.files.map(file => ({ file, lines: a.read(file).split("\n").length - 1, sha256: a.hash(fs.readFileSync(path.join(a.root, file))) })),
    original: current.original, current: current.current, exact: current.exact,
    before: residuals(current.before), after: residuals(current.after)
  };
  if (JSON.stringify(actual) !== JSON.stringify(expected)) throw new Error("Evidence drift in family " + scope.id);
  // Test attributes and identity must survive, except the one explicitly retired non-production test.
  // 测试属性与身份必须保留，唯一例外是已明确淘汰的脱离生产旧测试。
  const oldTests = tests(a.original(scope.original), scope.original).filter(item =>
    !(scope.id === "14" && item.name === "provider_response_trace_body_uses_the_smaller_exact_encoding"));
  const newTests = current.files.flatMap(file => tests(a.read(file), file));
  const sort = items => items.sort((left, right) => left.name.localeCompare(right.name));
  if (JSON.stringify(sort(oldTests)) !== JSON.stringify(sort(newTests))) throw new Error("Test identity/attribute drift in family " + scope.id);
  reports.push({ id: scope.id, files: current.files.length, declarations: current.current, exact: current.exact, reviewedResiduals: current.after.length, tests: newTests.length });
}
// Every split product must be reachable through actual module declarations from its original root.
// 每个拆分产物必须能通过真实模块声明从原根文件到达。
const visited = new Set(), pending = scopes.map(scope => scope.original);
while (pending.length) {
  const file = pending.pop();
  if (visited.has(file)) continue;
  visited.add(file);
  for (const child of modules(file)) if (allFiles.has(child)) pending.push(child);
}
const unreachable = [...allFiles].filter(file => !visited.has(file));
if (unreachable.length) throw new Error("Unreachable split products: " + unreachable.join(", "));
// File-location-sensitive macros require explicit human confirmation, even when function tokens match.
// 即使函数词元相同，依赖源码位置的宏也必须明确人工确认。
const locationMacros = [];
for (const file of allFiles) {
  const list = a.tokens(a.read(file));
  for (let i = 0; i < list.length - 1; i++) {
    if (["file", "module_path", "include", "include_str", "include_bytes"].includes(list[i].value) && list[i + 1].value === "!") {
      locationMacros.push({ file, name: list[i].value, arguments: list.slice(i + 2, i + 11).map(token => token.value) });
    }
  }
}
// Freeze only related resources, necessary consumer entrances, and explicitly excluded source boundaries.
// 仅冻结关联资源、必要消费者入口及明确排除的源码边界。
const supplements = require("./AUDIT-SUPPLEMENT.json");
for (const entry of supplements) {
  if (a.hash(fs.readFileSync(path.join(a.root, entry.file))) !== entry.sha256) {
    throw new Error("Related resource or excluded-boundary drift: " + entry.file);
  }
}
process.stdout.write(JSON.stringify({ reports, reachable: visited.size, supplementalFiles: supplements.length, locationMacros }, null, 2));
