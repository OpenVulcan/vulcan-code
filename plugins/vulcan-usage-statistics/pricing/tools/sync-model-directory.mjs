// Derive searchable model identities from the same bundled index shipped with the host.
// 从宿主随包发布的同一索引派生可搜索模型标识。
import { readFileSync, writeFileSync } from "node:fs";
import {
  gptReasoningBase, gptReasoningEfforts, hyphenatedGptAlias, isPricingModel, isRetiredGptModel, minimumGptPricingVersion, namedGptPricing,
} from "./gpt-pricing-models.mjs";
import { billingModelFormat, catalogBillingModelId, isRetiredMiniMaxModel, matchesBillingFamily } from "./sync-reference-pricing.mjs";

// manifestRoot anchors the indexed model sources independently of the working directory.
// manifestRoot 锚定索引中的模型来源，避免依赖当前工作目录。
const manifestRoot = new URL("../../../../crates/vulcan-runtime-assets/resources/provider-manifests/", import.meta.url);
// bundle is the self-contained Lua runtime consumed by the plugin host.
// bundle 是插件宿主加载的独立 Lua 运行包。
const bundle = new URL("../../runtime/plugin.bundle.lua", import.meta.url);
// directory lists the same bundled model groups for readers outside the app.
// directory 为应用之外的读者列出同一批随包模型组。
const directory = new URL("../catalog/model-directory.md", import.meta.url);
// referenceCatalog is the generated reference for both native currencies.
// referenceCatalog 是生成的双币种原币参考目录。
const referenceCatalog = new URL("../catalog/vulcan-model-pricing.v1.json", import.meta.url);

// Read one indexed JSON resource and return its parsed document.
// 读取一份索引中的 JSON 资源并返回解析后的文档。
function readManifest(relativePath) {
  return JSON.parse(readFileSync(new URL(relativePath, manifestRoot), "utf8"));
}

// Return the final slash component of modelId using the same boundary as the runtime.
// 按运行时相同的边界返回 modelId 最后一个斜杠后的分段。
function modelIdSuffix(modelId) {
  return modelId.split("/").at(-1) || modelId;
}

// Identify a provider display label such as "openai: gpt image 2", preserving ID suffixes like ":free".
// 识别形如 "openai: gpt image 2" 的供应商展示名，并保留标识中的 ":free" 等后缀。
function isDisplayModelLabel(modelId) {
  return /:\s/.test(modelId);
}

// Return the canonical model for a suffix covered by an approved family, or undefined.
// 返回已确认系列覆盖的后缀所对应的主型号，无匹配时返回 undefined。
function familyBillingModelId(suffix) {
  for (const [prefix, canonical] of billingPrefixes) {
    if (matchesBillingFamily(suffix.toLowerCase(), prefix)) return canonical;
  }
}

// Resolve the final slash component through explicit aliases and approved family prefixes.
// 通过明确的别名与已确认的系列前缀解析最后一个斜杠分段。
function billingModelId(modelId, aliases) {
  // suffix is independent of the route used by the provider.
  // suffix 与实际调用的供应商路由无关。
  const suffix = modelIdSuffix(modelId);
  // canonical applies the catalog-wide Preview and Claude rules before publishing a row.
  // canonical 在发布目录条目前应用全表的 Preview 和 Claude 规则。
  const canonical = catalogBillingModelId(suffix);
  return aliases.get(suffix) ?? aliases.get(canonical) ?? familyBillingModelId(canonical) ?? canonical;
}

// index defines the core catalog and provider manifests shipped in the same generation.
// index 定义同一代际随包发布的核心目录和供应商清单。
const index = readManifest("bundled-index.json");
// ids contains the declared model ID suffixes used to seed the pricing directory.
// ids 包含用于填充费用目录的已声明模型标识后缀。
const coreModels = readManifest(index.core_models.relative_url).models;
const ids = new Set(coreModels.map((model) => model.model_id));
// aliases combines declared identities with explicitly requested billing groups.
// aliases 合并清单声明的模型别名与用户明确指定的定价分组。
const aliases = new Map();
// catalog supplies retained prices and model groups before aliases are projected to the runtime.
// catalog 提供保留报价与型号分组，再投影别名到运行时。
const catalog = JSON.parse(readFileSync(referenceCatalog, "utf8"));
// Native-unit quotes identify media models without giving them a synthetic Token tariff.
// 原始单位报价标识媒体模型，不为它们构造虚假的 Token 单价。
for (const id of Object.keys(catalog.nativeUnitReferences)) ids.add(id);
// billingPrefixes maps complete family stems to their canonical model IDs.
// billingPrefixes 将完整系列主干映射到主型号标识。
const billingPrefixes = new Map();
for (const { canonicalModelId, prefixes } of catalog.models) {
  for (const prefix of prefixes ?? []) {
    for (const [existing, modelId] of billingPrefixes) {
      if (modelId !== canonicalModelId && (matchesBillingFamily(prefix, existing) || matchesBillingFamily(existing, prefix))) {
        throw new Error(`Overlapping billing family: ${prefix}`);
      }
    }
    billingPrefixes.set(prefix, canonicalModelId);
  }
}

// Register a declared spelling for modelId, rejecting conflicting targets for the same suffix.
// 为 modelId 登记一个已声明写法，同一个后缀指向不同型号时明确报错。
function addDeclaredAlias(rawAlias, modelId) {
  if (isDisplayModelLabel(rawAlias)) return;
  // alias retains meaningful case and snapshot differences but strips the provider path.
  // alias 保留有意义的大小写和快照差异，并去掉供应商路径。
  const alias = modelIdSuffix(rawAlias);
  if (alias === modelId) return;
  // prior permits the same alias mapping to be declared by multiple routes.
  // prior 允许多条路由声明相同的别名映射。
  const prior = aliases.get(alias);
  if (prior !== undefined && prior !== modelId) throw new Error(`Conflicting model alias: ${alias}`);
  aliases.set(alias, modelId);
}

for (const model of catalog.models) {
  for (const spelling of [...model.aliases, ...(model.members ?? [])]) addDeclaredAlias(spelling, model.canonicalModelId);
}
for (const model of coreModels) {
  for (const alias of model.aliases ?? []) {
    // suffix must not override another independently declared core model.
    // suffix 不能覆盖另一项独立声明的核心型号。
    const suffix = modelIdSuffix(alias);
    if (ids.has(suffix) && suffix !== model.model_id) throw new Error(`Ambiguous model alias: ${alias}`);
    addDeclaredAlias(alias, billingModelId(model.model_id, aliases));
  }
}
// Group the Fable 5.1 spellings only for pricing; provider routing still uses its declared IDs.
// Fable 5.1 的不同写法仅在计费时归组；供应商路由仍使用原有声明的标识。
for (const alias of ["claude-fable-5-1-thinking", "claude-fable-5.1", "fable5-1", "fable-5.1"]) {
  addDeclaredAlias(alias, "claude-fable-5-1");
}
// Google's product names identify these two image models even without token reference prices.
// Google 的产品名指向这两个图像型号，即使它们没有 Token 参考价也可以归组。
addDeclaredAlias("nano-banana-2", "gemini-3.1-flash-image");
addDeclaredAlias("nano-banana-2-lite", "gemini-3.1-flash-lite-image");
// The fixed 256K Kimi Code route has its own billing identity below the general K3 stem.
// 固定 256K 的 Kimi Code 路线拥有比通用 K3 主干更具体的独立定价标识。
addDeclaredAlias("k3-256k", "kimi-k3-256k");
for (const provider of Object.values(index.providers)) {
  const manifest = readManifest(provider.relative_url);
  for (const service of manifest.services) {
    // Only declared catalog entries identify models. Rule matchers can describe routing policies.
    // 只从已声明的目录条目获取模型标识，规则匹配器也可用于描述路由策略。
    for (const model of service.model_catalog?.models ?? []) {
      if (!isDisplayModelLabel(model.model_id)) ids.add(modelIdSuffix(model.model_id));
    }
  }
  for (const rule of manifest.model_rules) {
    // Only an exact binding to a core model supplies alias evidence.
    // 只有精确绑定到核心模型的规则可作为别名依据。
    if (rule.matcher.kind !== "exact" || !rule.core_model_ref || rule.matcher.value === rule.core_model_ref) continue;
    addDeclaredAlias(rule.matcher.value, billingModelId(rule.core_model_ref, aliases));
  }
}

// referencePrices tracks which currencies are quoted for each retained model group.
// referencePrices 记录每个保留模型组已有报价的币种。
const referencePrices = new Map(catalog.models.map(({ canonicalModelId, prices }) => [
  canonicalModelId,
  [prices.inputPerMillion !== undefined ? "USD" : null, prices.inputPerMillionCny !== undefined ? "CNY" : null].filter(Boolean).join(" / "),
]));
// Non-Token reference prices remain visible without entering the Token estimator.
// 非 Token 参考价保持可见，但不参与 Token 费用估算。
for (const id of Object.keys(catalog.nativeUnitReferences)) referencePrices.set(id, "CNY（按媒体单位）");
// hiddenIds suppresses removed names in the directory while retaining their usage and user overrides.
// hiddenIds 让移除的名称不再显示在费用目录中，同时保留相关用量和用户配置。
const hiddenIds = new Set([...catalog.excludedGptModelIds, ...catalog.excludedMiniMaxModelIds]);
// The seeded Haiku 5 ID has no corresponding model in Anthropic's public catalog.
// 种子清单中的 Haiku 5 标识在 Anthropic 公开目录中没有对应型号。
hiddenIds.add("claude-haiku-5");
// knownModels includes only retained billing identities so an excluded ID cannot become a fallback stem.
// knownModels 只包含保留的计费标识，避免已移除的标识成为回退匹配主干。
const knownModels = new Set([...ids, ...referencePrices.keys()].map((id) => billingModelId(id, aliases))
  .filter((id) => !hiddenIds.has(id) && isPricingModel(id) && !isRetiredMiniMaxModel(id, catalog.minimumMiniMaxVersion)));

// Register a spelling for an existing billing model, rejecting contradictory provider declarations.
// 为已有计费型号登记一种写法；如果与供应商的声明冲突，则明确报错。
function addPricingAlias(alias, modelId) {
  if (knownModels.has(alias)) throw new Error(`Conflicting pricing alias: ${alias}`);
  addDeclaredAlias(alias, modelId);
}

for (const id of knownModels) {
  // dashed is a displayable compatibility name, whereas effort aliases are for billing history only.
  // dashed 是可以显示的兼容名称；思考档位别名只用于归并历史用量。
  const dashed = hyphenatedGptAlias(id);
  if (dashed !== null) addPricingAlias(dashed, id);
}
for (const id of new Set([...knownModels, ...aliases.keys()])) {
  for (const effort of gptReasoningEfforts) {
    // alias must be a recognized GPT reasoning suffix and cannot replace a real model like Codex Max.
    // alias 必须带有已识别的 GPT 思考后缀，且不能覆盖 Codex Max 等实际型号。
    const alias = `${id}-${effort}`;
    if (gptReasoningBase(alias) !== id || knownModels.has(alias)) continue;
    // canonical resolves the base spelling directly to its price, so aliases never chain at runtime.
    // canonical 将基础写法直接指向其价格，避免运行时出现多级别名。
    const canonical = billingModelId(id, aliases);
    if (!knownModels.has(canonical)) continue;
    addPricingAlias(alias, canonical);
    hiddenIds.add(alias);
  }
}
for (const id of ids) {
  // canonical can come from a declared spelling of an older model.
  // canonical 也可能来自旧型号的某个已声明写法。
  const canonical = billingModelId(id, aliases);
  if (!isPricingModel(canonical) || isRetiredMiniMaxModel(canonical, catalog.minimumMiniMaxVersion)) hiddenIds.add(id);
}
for (const id of referencePrices.keys()) {
  if (billingModelId(id, aliases) !== id) throw new Error(`Reference prices must be attached to their billing key: ${id}`);
}

// lookupKeys folds ASCII case for declared models and aliases while preserving their canonical IDs.
// lookupKeys 对已声明型号和别名统一 ASCII 大小写，同时保留其主标识。
const lookupKeys = new Map();
for (const entries of Object.values(catalog.providerModels)) {
  for (const id of Object.keys(entries)) {
    if (billingModelId(id, aliases) === id) lookupKeys.set(id.toLowerCase(), id);
  }
}
for (const [suffix, canonical] of [...[...knownModels].map((id) => [id, id]), ...aliases]) {
  // key follows SQLite's ASCII case folding so all pricing entry points agree.
  // key 与 SQLite 的 ASCII 大小写转换规则一致，保证各计费入口一致。
  const key = suffix.replace(/[A-Z]/g, (letter) => letter.toLowerCase());
  if (lookupKeys.has(key) && lookupKeys.get(key) !== canonical) throw new Error(`Conflicting model suffix: ${suffix}`);
  lookupKeys.set(key, canonical);
}
// segmentStems mirrors the runtime candidates without letting exact-only aliases become families.
// segmentStems 与运行时候选一致，不把仅用于精确匹配的别名扩展成系列。
const segmentModels = new Map([...knownModels].map((id) => [id.toLowerCase(), id]));
for (const [stem, canonical] of billingPrefixes) {
  if (segmentModels.has(stem) && segmentModels.get(stem) !== canonical) throw new Error(`Conflicting billing stem: ${stem}`);
  segmentModels.set(stem, canonical);
}
// segmentStems orders candidates by specificity; the lookup separately gives raw spellings precedence on ties.
// segmentStems 按具体程度排列候选，查找时另外让原始写法在同长度时优先。
const segmentStems = [...segmentModels].sort(([left], [right]) => right.length - left.length);
for (const key of [...lookupKeys.keys()].sort((left, right) => left.length - right.length || (left < right ? -1 : 1))) {
  // Keep exact model keys for fast lookup and use the longest segment to eliminate redundant aliases.
  // 保留主型号的精确键供快速查找，并用最长完整段去掉可推导的别名。
  const canonical = lookupKeys.get(key);
  if (key === canonical.toLowerCase()) continue;
  const { id: normalized } = billingModelFormat(key);
  const raw = segmentStems.find(([stem]) => matchesBillingFamily(key, stem));
  const formatted = normalized === key ? raw : segmentStems.find(([stem]) => matchesBillingFamily(normalized, stem));
  const segment = (formatted?.[0].length ?? 0) > (raw?.[0].length ?? 0) ? formatted : raw;
  const fallback = (normalized !== key ? lookupKeys.get(normalized) : undefined) ?? segment?.[1];
  if (fallback === canonical) lookupKeys.delete(key);
}

// block identifies the generated section while leaving the reference prices independently owned.
// block 标识生成区块，让参考价格保持独立维护。
const block = /-- BEGIN GENERATED VULCAN MODEL DIRECTORY\r?\n[\s\S]*?-- END GENERATED VULCAN MODEL DIRECTORY/;
// current preserves the bundle's line endings when rebuilding the section.
// current 保留运行包的原有换行格式。
const current = readFileSync(bundle, "utf8");
if (!block.test(current)) throw new Error("The bundled model directory markers are missing.");
// generated contains identities only. Pricing still requires an exact reference or user override.
// generated 仅包含模型标识，定价仍须匹配精确参考价或用户覆盖。
const generated = [
  "-- BEGIN GENERATED VULCAN MODEL DIRECTORY",
  "-- Generated by pricing/tools/sync-model-directory.mjs.",
  "-- 由 pricing/tools/sync-model-directory.mjs 生成。",
  "local DECLARED_MODEL_IDS = {",
  ...[...new Set([...ids].filter((id) => !hiddenIds.has(id)).map((id) => billingModelId(id, aliases)))].sort()
    .map((id) => `    ${JSON.stringify(id)},`),
  "}",
  "-- Match declared model suffixes and aliases using ASCII case folding.",
  "-- 匹配已声明的型号后缀与别名时统一 ASCII 大小写。",
  "local DECLARED_MODEL_ALIASES = {",
  ...[...lookupKeys.keys()].sort().map((id) => `    [${JSON.stringify(id)}] = ${JSON.stringify(lookupKeys.get(id))},`),
  "}",
  "-- Configured families match the stem itself or a nonempty hyphen suffix.",
  "-- 已配置的系列匹配主干本身，或主干后带非空连字符后缀的写法。",
  "local DECLARED_MODEL_PREFIXES = {",
  ...[...billingPrefixes].sort(([left], [right]) => left.localeCompare(right)).map(([prefix, id]) =>
    `    [${JSON.stringify(prefix)}] = ${JSON.stringify(id)},`),
  "}",
  "-- Hidden names can still contribute usage after resolving to a retained model.",
  "-- 隐藏的名称在解析到保留型号后仍可贡献用量。",
  "local PRICING_HIDDEN_MODEL_IDS = {",
  ...[...hiddenIds].sort().map((id) => `    [${JSON.stringify(id)}] = true,`),
  "}",
  "-- Apply the same GPT version floor to history and bundled rows.",
  "-- 对历史用量和随包条目应用同一 GPT 版本下限。",
  `local MINIMUM_GPT_PRICING_VERSION = { major = ${minimumGptPricingVersion.major}, minor = ${minimumGptPricingVersion.minor} }`,
  "-- Use the same MiniMax version floor for historical usage and bundled prices.",
  "-- 历史用量与随包价格使用同一 MiniMax 版本下限。",
  `local MINIMUM_MINIMAX_PRICING_VERSION = { major = ${catalog.minimumMiniMaxVersion.major}, minor = ${catalog.minimumMiniMaxVersion.minor} }`,
  "-- Named GPT families have independent prices for each listed model suffix.",
  "-- 具名 GPT 代际对列出的每种型号后缀分别定价。",
  `local NAMED_GPT_PRICING = { major = ${namedGptPricing.major}, minor = ${namedGptPricing.minor}, astra_major = ${namedGptPricing.astraMajor}, variants = { ${namedGptPricing.variants.map((variant) => JSON.stringify(variant)).join(", ")} } }`,
  "-- END GENERATED VULCAN MODEL DIRECTORY",
].join(current.includes("\r\n") ? "\r\n" : "\n");
// updated replaces only the generated section.
// updated 只替换生成区块。
const updated = current.replace(block, () => generated);

// grouped records each canonical ID with its declared, searchable aliases.
// grouped 记录每个定价主标识及其已声明且可搜索的别名。
const grouped = new Map();
for (const id of [...new Set([...ids, ...aliases.keys(), ...referencePrices.keys()])].sort()) {
  const canonical = billingModelId(id, aliases);
  if (hiddenIds.has(id) || hiddenIds.has(canonical)) continue;
  if (!grouped.has(canonical)) grouped.set(canonical, []);
}
for (const [alias, canonical] of [...lookupKeys].sort(([left], [right]) => left.localeCompare(right))) {
  if (alias !== canonical.toLowerCase() && !hiddenIds.has(alias) && grouped.has(canonical)) {
    grouped.get(canonical).push(alias);
  }
}
// markdown is the reproducible model list, with price availability for each group.
// markdown 是可重现的模型清单，同时标明每组是否已有参考价。
const markdown = [
  "# 用量统计随包模型清单",
  "",
  `随包共 **${grouped.size}** 个模型组，其中 **${referencePrices.size}** 组有美元或人民币参考价，**${grouped.size - referencePrices.size}** 组待配置。用量记录中的其他模型会另外进入费用表，也可以在搜索栏输入完整模型 ID 后直接编辑。`,
  "",
  "费用表按模型组共用一份配置。匹配时取最后一个斜杠后的模型 ID，忽略英文字母大小写。先匹配精确型号或别名，再按连字符边界匹配最长的已知型号段。例如 glm-5.3-flash-xxx 优先匹配 glm-5.3-flash。日期、Preview 和 Claude 兼容写法也按规则归组。别名栏只列需要单独映射的名称。",
  "",
  "每百万 Token 的输入、输出、缓存读和缓存写都可以分别填写美元和人民币。只填一个币种时，按统计页的统一汇率折算另一个币种。两个币种都填写时，各自采用填写值。",
  "可用精确供应商 ID 为模型绑定独立价格。该供应商优先使用专属价格，其余请求沿用共享价格。模板会将基础价、谷价和上下文档位复制到草稿，保存后可单独编辑。",
  "启用谷价后，以请求终止时间和模型配置的固定 UTC 偏移匹配每周时段，区间包含开始时间，不含结束时间。时段之外使用基础价。",
  "上下文分档以单次请求的总输入 Token 选档。等于阈值时使用前一档，超过阈值后整笔请求使用新档。基础价和谷价可分别配置多个档位。",
  "百炼媒体模型的按张、按秒及按字符刊例，以及百炼专属的 MiniMax M2.5 报价，见 [官网价格核查](official-price-audit-2026-11-10.md#百炼-token-plan-补齐项)。",
  "",
  "| 定价模型组 | 随包参考价 | 已声明别名 |",
  "| --- | --- | --- |",
  ...[...grouped.keys()].sort().map((id) =>
    `| \`${id}\` | ${referencePrices.get(id) ?? "待配置"} | ${grouped.get(id).map((alias) => `\`${alias}\``).join("、") || "—"} |`),
  "",
  "## GPT 费用表清理清单",
  "",
  `下列 **${catalog.excludedGptModelIds.length}** 个旧条目已移出费用表。数字 GPT 型号从 \`gpt-${minimumGptPricingVersion.major}.${minimumGptPricingVersion.minor}\` 起保留。可确定且仍保留主型号的历史用量继续按主型号归组。原始用量保留，已退役或无法确定具体型号时不套用其他型号的参考价。`,
  "",
  "| 已移除的独立条目 | 历史用量的定价归组 |",
  "| --- | --- |",
  ...catalog.excludedGptModelIds.map((id) =>
    `| \`${id}\` | ${isRetiredGptModel(id) ? "版本低于保留下限" : aliases.has(id) ? `\`${aliases.get(id)}\`` : "无对应主型号"} |`),
  "",
  `MiniMax M 系列从 M${catalog.minimumMiniMaxVersion.major}.${catalog.minimumMiniMaxVersion.minor} 起保留。移除的随包报价包括 ${catalog.excludedMiniMaxModelIds.map((id) => `\`${id}\``).join("、")}。`,
  "",
].join("\n");

if (process.argv.includes("--check")) {
  if (updated !== current || readFileSync(directory, "utf8").replaceAll("\r\n", "\n") !== markdown) {
    throw new Error("Run node plugins/vulcan-usage-statistics/pricing/tools/sync-model-directory.mjs to refresh the model directory.");
  }
} else {
  if (updated !== current) writeFileSync(bundle, updated);
  writeFileSync(directory, markdown);
}
