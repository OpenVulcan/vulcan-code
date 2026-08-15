import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";

// ADAPTER_VERSION identifies the exact Vulcan source-adaptation contract.
// ADAPTER_VERSION 标识精确的 Vulcan 来源适配契约。
const ADAPTER_VERSION = 1;
// UPSTREAM_REPOSITORY is provenance only and is never a runtime dependency.
// UPSTREAM_REPOSITORY 仅用于来源追踪，绝不是运行时依赖。
const UPSTREAM_REPOSITORY = "https://github.com/farion1231/cc-switch.git";
// UPSTREAM_SOURCE_PATH identifies the audited upstream source file.
// UPSTREAM_SOURCE_PATH 标识已审计的上游源文件。
const UPSTREAM_SOURCE_PATH = "src-tauri/src/database/schema.rs";
// GENERATED_BLOCK_BEGIN marks the first generated runtime line.
// GENERATED_BLOCK_BEGIN 标记运行时生成区块的首行。
const GENERATED_BLOCK_BEGIN = "-- BEGIN GENERATED VULCAN DEFAULT PRICING CATALOG";
// GENERATED_BLOCK_END marks the last generated runtime line.
// GENERATED_BLOCK_END 标记运行时生成区块的末行。
const GENERATED_BLOCK_END = "-- END GENERATED VULCAN DEFAULT PRICING CATALOG";

// Fail with one deterministic adapter diagnostic.
// 使用确定性的适配器诊断终止执行。
//
// message is the exact non-secret failure reason.
// message 是精确的非敏感失败原因。
// This function never returns.
// 此函数永不返回。
function fail(message) {
  throw new Error(`pricing adapter: ${message}`);
}

// Parse one required command-line option without positional ambiguity.
// 解析一个必需命令行选项且不引入位置歧义。
//
// name is the exact long option name.
// name 是精确的长选项名。
// Returns the following argument or fails explicitly.
// 返回后续参数，缺失时显式失败。
function requiredOption(name) {
  // optionIndex is the exact position of the requested option.
  // optionIndex 是请求选项的精确位置。
  const optionIndex = process.argv.indexOf(name);
  if (optionIndex < 0 || optionIndex + 1 >= process.argv.length) {
    fail(`missing required option ${name}`);
  }
  return process.argv[optionIndex + 1];
}

// Extract one Rust array expression while honoring strings and comments.
// 提取一个 Rust 数组表达式，同时正确处理字符串与注释。
//
// source is the complete audited Rust source and marker identifies one let binding.
// source 是完整的已审计 Rust 源码，marker 标识一个 let 绑定。
// Returns the array body without its outer brackets.
// 返回不含最外层方括号的数组正文。
function extractRustArray(source, marker) {
  // markerIndex locates the unique audited binding.
  // markerIndex 定位唯一的已审计绑定。
  const markerIndex = source.indexOf(marker);
  if (markerIndex < 0 || source.indexOf(marker, markerIndex + marker.length) >= 0) {
    fail(`expected exactly one marker ${marker}`);
  }
  // arrayStart locates the opening bracket after the binding.
  // arrayStart 定位绑定之后的起始方括号。
  const arrayStart = source.indexOf("[", markerIndex + marker.length);
  if (arrayStart < 0) fail(`missing array after ${marker}`);
  // depth tracks nested Rust array brackets.
  // depth 跟踪嵌套 Rust 数组方括号。
  let depth = 0;
  // mode tracks code, string, line-comment, or block-comment parsing.
  // mode 跟踪代码、字符串、行注释或块注释解析状态。
  let mode = "code";
  // escaped records whether the previous string byte was a backslash.
  // escaped 记录前一个字符串字节是否为反斜杠。
  let escaped = false;
  for (let index = arrayStart; index < source.length; index += 1) {
    // character is the current UTF-16 source unit; all syntax tokens are ASCII.
    // character 是当前 UTF-16 源单元；全部语法标记均为 ASCII。
    const character = source[index];
    // nextCharacter supports exact two-byte comment delimiters.
    // nextCharacter 用于识别精确的双字节注释分隔符。
    const nextCharacter = source[index + 1];
    if (mode === "string") {
      if (escaped) escaped = false;
      else if (character === "\\") escaped = true;
      else if (character === '"') mode = "code";
      continue;
    }
    if (mode === "line-comment") {
      if (character === "\n") mode = "code";
      continue;
    }
    if (mode === "block-comment") {
      if (character === "*" && nextCharacter === "/") {
        mode = "code";
        index += 1;
      }
      continue;
    }
    if (character === '"') mode = "string";
    else if (character === "/" && nextCharacter === "/") {
      mode = "line-comment";
      index += 1;
    } else if (character === "/" && nextCharacter === "*") {
      mode = "block-comment";
      index += 1;
    } else if (character === "[") depth += 1;
    else if (character === "]") {
      depth -= 1;
      if (depth === 0) return source.slice(arrayStart + 1, index);
    }
  }
  fail(`unterminated array after ${marker}`);
}

// Parse top-level Rust tuples into decoded string fields.
// 将顶层 Rust 元组解析为已解码字符串字段。
//
// arrayBody is one bracket-free Rust array body.
// arrayBody 是一段不含外层方括号的 Rust 数组正文。
// Returns tuples in source order.
// 按源码顺序返回元组。
function parseRustStringTuples(arrayBody) {
  // tuples receives decoded tuples in declaration order.
  // tuples 按声明顺序接收解码后的元组。
  const tuples = [];
  // tupleStart stores the opening parenthesis for one top-level tuple.
  // tupleStart 保存一个顶层元组的起始圆括号位置。
  let tupleStart = -1;
  // depth tracks nested parentheses outside comments and strings.
  // depth 跟踪字符串与注释之外的嵌套圆括号。
  let depth = 0;
  // mode tracks lexical context.
  // mode 跟踪词法上下文。
  let mode = "code";
  // escaped tracks string escapes.
  // escaped 跟踪字符串转义。
  let escaped = false;
  for (let index = 0; index < arrayBody.length; index += 1) {
    // character is the current syntax unit.
    // character 是当前语法单元。
    const character = arrayBody[index];
    // nextCharacter supports comment delimiters.
    // nextCharacter 用于识别注释分隔符。
    const nextCharacter = arrayBody[index + 1];
    if (mode === "string") {
      if (escaped) escaped = false;
      else if (character === "\\") escaped = true;
      else if (character === '"') mode = "code";
      continue;
    }
    if (mode === "line-comment") {
      if (character === "\n") mode = "code";
      continue;
    }
    if (mode === "block-comment") {
      if (character === "*" && nextCharacter === "/") {
        mode = "code";
        index += 1;
      }
      continue;
    }
    if (character === '"') mode = "string";
    else if (character === "/" && nextCharacter === "/") {
      mode = "line-comment";
      index += 1;
    } else if (character === "/" && nextCharacter === "*") {
      mode = "block-comment";
      index += 1;
    } else if (character === "(") {
      if (depth === 0) tupleStart = index;
      depth += 1;
    } else if (character === ")") {
      depth -= 1;
      if (depth < 0) fail("encountered an unmatched tuple terminator");
      if (depth === 0) {
        // tupleSource is the exact top-level tuple source.
        // tupleSource 是精确的顶层元组源码。
        const tupleSource = arrayBody.slice(tupleStart, index + 1);
        // literalPattern accepts standard Rust string literals used by the audited source.
        // literalPattern 接受已审计源码使用的标准 Rust 字符串字面量。
        const literalPattern = /"(?:\\.|[^"\\])*"/g;
        // literals contains JSON-compatible decoded string values.
        // literals 包含与 JSON 兼容的已解码字符串值。
        const literals = [...tupleSource.matchAll(literalPattern)].map((match) => JSON.parse(match[0]));
        tuples.push(literals);
      }
    }
  }
  if (depth !== 0 || mode === "string" || mode === "block-comment") {
    fail("unterminated Rust tuple source");
  }
  return tuples;
}

// Validate one canonical non-negative decimal string.
// 校验一个规范的非负十进制字符串。
//
// value is one upstream price field and label identifies diagnostics.
// value 是一个上游价格字段，label 标识诊断位置。
// Returns the unchanged canonical string.
// 返回未改变的规范字符串。
function canonicalDecimal(value, label) {
  if (!/^(?:0|[1-9]\d*)(?:\.\d+)?$/.test(value)) {
    fail(`${label} is not a canonical non-negative decimal: ${JSON.stringify(value)}`);
  }
  return value;
}

// Compute one lowercase SHA-256 digest for exact UTF-8 text.
// 为精确 UTF-8 文本计算一个小写 SHA-256 摘要。
//
// text is the exact content to hash.
// text 是要计算哈希的精确内容。
// Returns the hexadecimal digest without a prefix.
// 返回不含前缀的十六进制摘要。
function sha256(text) {
  return createHash("sha256").update(text, "utf8").digest("hex");
}

// Serialize deterministic JSON with one trailing newline.
// 序列化确定性 JSON 并附加一个尾随换行。
//
// value is already key-ordered by the adapter.
// value 已由适配器完成键顺序安排。
// Returns human-readable UTF-8 text.
// 返回便于审计的 UTF-8 文本。
function jsonText(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

// Escape one ASCII catalog field for a Lua double-quoted literal.
// 为 Lua 双引号字面量转义一个 ASCII 目录字段。
//
// value is one validated model or price string.
// value 是一个已校验的模型或价格字符串。
// Returns a Lua-safe string body.
// 返回可安全用于 Lua 的字符串正文。
function luaString(value) {
  return value.replaceAll("\\", "\\\\").replaceAll('"', '\\"').replaceAll("\n", "\\n");
}

// Write one generated file after ensuring its parent directory exists.
// 确保父目录存在后写入一个生成文件。
//
// filePath is absolute and content is exact UTF-8 text.
// filePath 是绝对路径，content 是精确 UTF-8 文本。
// This function returns nothing.
// 此函数无返回值。
function writeGeneratedFile(filePath, content) {
  mkdirSync(dirname(filePath), { recursive: true });
  writeFileSync(filePath, content, "utf8");
}

// sourceFile is the exact cloned cc-switch schema source.
// sourceFile 是精确克隆的 cc-switch Schema 源文件。
const sourceFile = resolve(requiredOption("--source"));
// pluginRoot is the destination Vulcan statistics plugin directory.
// pluginRoot 是目标 Vulcan 统计插件目录。
const pluginRoot = resolve(requiredOption("--plugin-root"));
// upstreamTag is the frozen official release tag.
// upstreamTag 是冻结的官方发布标签。
const upstreamTag = requiredOption("--tag");
// upstreamCommit is the frozen source commit.
// upstreamCommit 是冻结的源码提交。
const upstreamCommit = requiredOption("--commit");
// sourceText contains the exact audited Rust bytes decoded as UTF-8.
// sourceText 包含按 UTF-8 解码的精确已审计 Rust 字节。
const sourceText = readFileSync(sourceFile, "utf8");
// seedTuples contains every declared seed row in source order.
// seedTuples 按源码顺序包含每条声明的种子记录。
const seedTuples = parseRustStringTuples(extractRustArray(sourceText, "let pricing_data ="));
// repairTuples contains every guarded repair row in source order.
// repairTuples 按源码顺序包含每条带守卫的修复记录。
const repairTuples = parseRustStringTuples(extractRustArray(sourceText, "let pricing_fixes ="));
// models owns the exact final fresh-database price image keyed by model id.
// models 按模型 ID 持有精确的全新数据库最终价格镜像。
const models = new Map();

for (const [tupleIndex, tuple] of seedTuples.entries()) {
  if (tuple.length !== 6) fail(`seed tuple ${tupleIndex} has ${tuple.length} strings instead of 6`);
  // modelId is the exact upstream pricing identity.
  // modelId 是精确的上游计价身份。
  const [modelId, displayName, input, output, cacheRead, cacheCreation] = tuple;
  if (!/^[a-z0-9][a-z0-9._-]*$/.test(modelId)) fail(`invalid model id ${JSON.stringify(modelId)}`);
  if (models.has(modelId)) fail(`duplicate seed model ${modelId}`);
  models.set(modelId, {
    modelId,
    displayName,
    inputCostPerMillion: canonicalDecimal(input, `${modelId}.input`),
    outputCostPerMillion: canonicalDecimal(output, `${modelId}.output`),
    cacheReadCostPerMillion: canonicalDecimal(cacheRead, `${modelId}.cacheRead`),
    cacheCreationCostPerMillion: canonicalDecimal(cacheCreation, `${modelId}.cacheCreation`),
  });
}

// appliedRepairCount counts repairs whose exact old-value guard matched the fresh seed image.
// appliedRepairCount 统计旧值守卫精确命中新种子镜像的修复数量。
let appliedRepairCount = 0;
for (const [tupleIndex, tuple] of repairTuples.entries()) {
  if (tuple.length !== 10) fail(`repair tuple ${tupleIndex} has ${tuple.length} strings instead of 10`);
  // tuple fields mirror the upstream UPDATE statement: identity, new values, then guarded old values.
  // tuple 字段镜像上游 UPDATE：身份、新值，然后是带守卫的旧值。
  const [modelId, displayName, nextInput, nextOutput, nextCacheRead, nextCacheCreation,
    oldInput, oldOutput, oldCacheRead, oldCacheCreation] = tuple;
  // current is the price image at this exact source-order repair boundary.
  // current 是当前源码顺序修复边界上的价格镜像。
  const current = models.get(modelId);
  if (!current) fail(`repair references unknown model ${modelId}`);
  if (current.inputCostPerMillion === oldInput
    && current.outputCostPerMillion === oldOutput
    && current.cacheReadCostPerMillion === oldCacheRead
    && current.cacheCreationCostPerMillion === oldCacheCreation) {
    models.set(modelId, {
      modelId,
      displayName,
      inputCostPerMillion: canonicalDecimal(nextInput, `${modelId}.input`),
      outputCostPerMillion: canonicalDecimal(nextOutput, `${modelId}.output`),
      cacheReadCostPerMillion: canonicalDecimal(nextCacheRead, `${modelId}.cacheRead`),
      cacheCreationCostPerMillion: canonicalDecimal(nextCacheCreation, `${modelId}.cacheCreation`),
    });
    appliedRepairCount += 1;
  }
}

// sortedModels makes every generated artifact independent from Map implementation details.
// sortedModels 使全部生成产物不依赖 Map 实现细节。
const sortedModels = [...models.values()].sort((left, right) => left.modelId.localeCompare(right.modelId));
// provenance captures the frozen source without creating any runtime coupling.
// provenance 记录冻结来源且不产生任何运行时耦合。
const provenance = {
  repository: UPSTREAM_REPOSITORY,
  tag: upstreamTag,
  commit: upstreamCommit,
  path: UPSTREAM_SOURCE_PATH,
  symbols: ["Database::seed_model_pricing", "Database::repair_current_model_pricing"],
  license: "MIT",
};
// upstreamSnapshot is the plugin-owned reference copy in upstream-compatible field names.
// upstreamSnapshot 是使用上游兼容字段名的插件自有参考副本。
const upstreamSnapshot = {
  version: 1,
  source: provenance,
  currency: "USD",
  unit: "per_million_tokens",
  seedEntryCount: seedTuples.length,
  declaredRepairCount: repairTuples.length,
  appliedRepairCount,
  models: sortedModels,
};
// upstreamSnapshotText is hashed exactly as written.
// upstreamSnapshotText 按实际写入内容精确计算哈希。
const upstreamSnapshotText = jsonText(upstreamSnapshot);
// canonicalModels removes upstream naming from the Vulcan runtime contract.
// canonicalModels 从 Vulcan 运行时契约中移除上游命名。
const canonicalModels = sortedModels.map((model) => ({
  canonicalModelId: model.modelId,
  displayName: model.displayName,
  aliases: [],
  prices: {
    inputPerMillion: model.inputCostPerMillion,
    outputPerMillion: model.outputCostPerMillion,
    cacheReadPerMillion: model.cacheReadCostPerMillion,
    cacheWritePerMillion: model.cacheCreationCostPerMillion,
  },
}));
// catalogRevisionMaterial excludes provenance so equivalent price content has one stable identity.
// catalogRevisionMaterial 排除来源信息，使等价价格内容拥有唯一稳定身份。
const catalogRevisionMaterial = jsonText({
  schemaVersion: 1,
  currency: "USD",
  unit: "per_million_tokens",
  matchingStrategy: "exact_canonical_model_id",
  models: canonicalModels,
});
// catalogRevision is the content-addressed Vulcan pricing identity.
// catalogRevision 是内容寻址的 Vulcan 价格身份。
const catalogRevision = `sha256:${sha256(catalogRevisionMaterial)}`;
// vulcanCatalog is the only price contract consumed by Vulcan plugin logic.
// vulcanCatalog 是 Vulcan 插件逻辑消费的唯一价格契约。
const vulcanCatalog = {
  schemaVersion: 1,
  catalogId: "vulcan.reference.model-pricing",
  catalogRevision,
  adapterVersion: ADAPTER_VERSION,
  currency: "USD",
  unit: "per_million_tokens",
  matching: {
    strategy: "exact_canonical_model_id",
    aliases: "explicit_only",
  },
  source: {
    kind: "vendored_reference_snapshot",
    snapshotSha256: sha256(upstreamSnapshotText),
    provenance,
  },
  models: canonicalModels,
};
// upstreamOutput is the plugin-local frozen reference snapshot.
// upstreamOutput 是插件本地冻结参考快照。
const upstreamOutput = resolve(pluginRoot, "pricing/upstream/cc-switch-v3.19.2-model-pricing.v1.json");
// catalogOutput is the Vulcan-owned canonical catalog.
// catalogOutput 是 Vulcan 自有的规范价格目录。
const catalogOutput = resolve(pluginRoot, "pricing/catalog/vulcan-model-pricing.v1.json");
writeGeneratedFile(upstreamOutput, upstreamSnapshotText);
writeGeneratedFile(catalogOutput, jsonText(vulcanCatalog));

// luaRows contains the generated exact-match runtime index.
// luaRows 包含生成的精确匹配运行时索引。
const luaRows = canonicalModels.map((model) => `    ["${luaString(model.canonicalModelId)}"] = { input_per_million = "${luaString(model.prices.inputPerMillion)}", output_per_million = "${luaString(model.prices.outputPerMillion)}", cache_read_per_million = "${luaString(model.prices.cacheReadPerMillion)}", cache_write_per_million = "${luaString(model.prices.cacheWritePerMillion)}" },`);
// generatedLua is replaced atomically between stable marker lines.
// generatedLua 在稳定标记行之间被原子替换。
const generatedLua = [
  GENERATED_BLOCK_BEGIN,
  "-- This block is generated by pricing/tools/convert-cc-switch-pricing.mjs.",
  "-- 此区块由 pricing/tools/convert-cc-switch-pricing.mjs 生成。",
  "local DEFAULT_PRICING_CATALOG = {",
  "    catalog_id = \"vulcan.reference.model-pricing\",",
  `    revision = "${catalogRevision}",`,
  "    currency = \"USD\",",
  "    matching_strategy = \"exact_canonical_model_id\",",
  `    entry_count = ${canonicalModels.length},`,
  "    models = {",
  ...luaRows,
  "    },",
  "}",
  GENERATED_BLOCK_END,
].join("\n");
// runtimeBundlePath identifies the sole managed Lua entrypoint.
// runtimeBundlePath 标识唯一的受管 Lua 入口。
const runtimeBundlePath = resolve(pluginRoot, "runtime/plugin.bundle.lua");
// runtimeBundle is the existing hand-written bundle surrounding the generated block.
// runtimeBundle 是生成区块外围现有的手写 Bundle。
const runtimeBundle = readFileSync(runtimeBundlePath, "utf8");
// blockPattern locates exactly one complete generated section.
// blockPattern 精确定位一个完整生成区段。
const blockPattern = new RegExp(`${GENERATED_BLOCK_BEGIN.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}[\\s\\S]*?${GENERATED_BLOCK_END.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}`);
if (!blockPattern.test(runtimeBundle)) fail("runtime bundle is missing the generated catalog markers");
writeGeneratedFile(runtimeBundlePath, runtimeBundle.replace(blockPattern, generatedLua));

process.stdout.write(`${JSON.stringify({
  adapterVersion: ADAPTER_VERSION,
  sourceTag: upstreamTag,
  sourceCommit: upstreamCommit,
  seedEntryCount: seedTuples.length,
  declaredRepairCount: repairTuples.length,
  appliedRepairCount,
  catalogEntryCount: canonicalModels.length,
  catalogRevision,
  snapshotSha256: sha256(upstreamSnapshotText),
})}\n`);
