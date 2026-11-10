import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { isPricingModel } from "./gpt-pricing-models.mjs";

// priceKeys match the four dimensions in the plugin's pricing settings.
// priceKeys 对应用量插件定价设置中的四个计价维度。
const priceKeys = ["inputPerMillion", "outputPerMillion", "cacheReadPerMillion", "cacheWritePerMillion"];
// sourceFile is the audited official-price and retention overlay relative to the plugin.
// sourceFile 是相对于插件目录的官网价格与保留规则来源。
const sourceFile = "pricing/upstream/official-model-pricing.v1.json";

// Serialize a JSON document deterministically for publication and hashing.
// 用确定性格式序列化 JSON 文档，供发布和哈希计算使用。
function jsonText(value) {
  return JSON.stringify(value, null, 2) + "\n";
}

// Convert a catalog value to a Lua literal with explicit keys for objects and positional keys for arrays.
// 将目录值转为 Lua 字面量，映射使用显式键，数组使用位置键。
function luaLiteral(value) {
  if (typeof value === "string" || typeof value === "number") return JSON.stringify(value);
  if (Array.isArray(value)) return `{ ${value.map(luaLiteral).join(", ")} }`;
  return `{ ${Object.entries(value).map(([key, item]) => `[${JSON.stringify(key)}] = ${luaLiteral(item)}`).join(", ")} }`;
}

// Return whether id names a MiniMax M version below the configured major and minor floor.
// 判断 id 是否表示低于 minimum 主版本与小版本下限的 MiniMax M 型号。
export function isRetiredMiniMaxModel(id, minimum) {
  // version captures the numeric model version without consuming variant suffixes.
  // version 捕获数字版本号，保留其后的型号后缀。
  const version = /^minimax-m(\d+)(?:\.(\d+))?/i.exec(id);
  if (!version) return false;
  // major and minor are compared as separate version components.
  // major 和 minor 分别比较，避免把版本号当作小数。
  const major = Number(version[1]);
  const minor = Number(version[2] ?? 0);
  return major < minimum.major || (major === minimum.major && minor < minimum.minor);
}

// Match a family stem or a nonempty hyphen suffix of that stem in a case-folded model ID.
// 匹配已折叠大小写的型号标识中的系列主干，或该主干后带非空连字符后缀的写法。
export function matchesBillingFamily(id, stem) {
  return id === stem || (id.length > stem.length + 1 && id.startsWith(`${stem}-`));
}

// Return the normalized spelling for a case-folded model ID and whether it had a Preview suffix.
// 返回大小写折叠后型号的规范写法，以及原名称是否带 Preview 后缀。
export function billingModelFormat(id) {
  // claude scopes dotted version and Thinking normalization to Claude's naming scheme.
  // claude 将点号版本和 Thinking 写法的归一限定在 Claude 命名中。
  const claude = id.startsWith("claude-");
  // stem removes only a complete date tail, preserving a distinct snapshot on exact lookup.
  // stem 仅去掉完整日期后缀，精确匹配仍可保留有独立报价的快照。
  const stem = (claude ? id.replace(/-thinking$/, "") : id).replace(/^(.+)(?:[-@]20\d{6}|-20\d{2}-\d{2}-\d{2})$/, "$1");
  // preview removes the terminal Preview marker and an optional rolling label.
  // preview 去掉末尾的 Preview 标记以及可选的滚动标记。
  const preview = stem.replace(/^(.+)-preview(?:-latest)?$/, "$1");
  return { id: claude ? preview.replaceAll(".", "-") : preview, isPreview: preview !== stem };
}

// Return a canonical billing key for a catalog ID, preserving other dated price entries.
// 返回目录型号的主计费键，同时保留其他带日期的独立报价。
export function catalogBillingModelId(id) {
  // format identifies Preview variants and Claude spellings that can share a catalog key.
  // format 识别可以共用目录键的 Preview 变体和 Claude 写法。
  const format = billingModelFormat(id);
  return format.isPreview || /^claude-(?:haiku|opus|sonnet|fable|mythos)-\d+(?:-\d{1,2})?$/.test(format.id) ? format.id : id;
}

// Compare two exact decimal strings without treating trailing zeros as a rate difference.
// 精确比较两个十进制字符串，避免把尾部补零误判为费率差异。
function sameRate(left, right) {
  return left.replace(/(\.\d*?)0+$/, "$1").replace(/\.$/, "") === right.replace(/(\.\d*?)0+$/, "$1").replace(/\.$/, "");
}

// Merge quoted members into canonical, validating overlapping rates and tariff schedules.
// 将已报价成员合并到 canonical，校验重叠费率和时段。
function mergeGroupQuotes(canonical, quotes, group) {
  // reference keeps a verified quote's metadata when the rates agree.
  // reference 在费率一致时保留有来源的报价元数据。
  const reference = quotes.find((quote) => quote.sources !== undefined) ?? quotes[0];
  if (reference === undefined) throw new Error(`Missing reference prices for billing group ${canonical}`);
  if (group.preferCanonicalQuote) {
    if (quotes[0].canonicalModelId !== canonical) throw new Error(`Missing canonical reference for ${canonical}`);
    return quotes[0];
  }
  // prices starts from the verified reference and only adds other currencies when approved.
  // prices 以已核验的报价为基础，只在明确允许时补入其他币种。
  const prices = { ...reference.prices };
  for (const quote of quotes) {
    if (jsonText(quote.offPeak) !== jsonText(reference.offPeak)
      || jsonText(quote.contextTiers) !== jsonText(reference.contextTiers)
      || (!group.combineCurrencies && Object.keys(quote.prices).length !== Object.keys(reference.prices).length)) {
      throw new Error(`Conflicting tariffs for billing group ${canonical}`);
    }
    for (const [key, value] of Object.entries(quote.prices)) {
      if (Object.hasOwn(prices, key) ? !sameRate(prices[key], value) : !group.combineCurrencies) {
        throw new Error(`Conflicting rate ${key} for billing group ${canonical}`);
      }
      prices[key] ??= value;
    }
  }
  return { ...reference, prices };
}

// Compile exact provider reference quotes and expand their declared tariff schedule.
// 编译供应商精确报价，并展开其声明的时段费率规则。
// entries supplies source quotes; schedules maps schedule names to local time windows.
// entries 提供来源报价；schedules 将时段规则名称映射到当地时间窗口。
// Returns the model-indexed price table consumed by the runtime.
// 返回运行时使用的按模型索引的价格表。
function compileProviderPrices(entries, schedules) {
  return Object.fromEntries(Object.entries(entries).map(([id, quote]) => {
    if (id !== catalogBillingModelId(id) || quote.sources.length === 0 ||
        quote.sources.some((url) => !url.startsWith("https://"))) {
      throw new Error(`Invalid provider reference quote: ${id}`);
    }
    for (const rates of [quote.prices, ...(quote.offPeak === undefined ? [] : [quote.offPeak.prices])]) {
      if (priceKeys.some((key) => rates[key] === undefined && rates[`${key}Cny`] === undefined)) {
        throw new Error(`Incomplete provider reference quote: ${id}`);
      }
    }
    if (quote.offPeak === undefined) return [id, quote.prices];
    // schedule is an exact catalog reference shared by models with the same local windows.
    // schedule 为共享同一当地时段的型号提供精确的目录引用。
    const schedule = schedules[quote.offPeak.scheduleRef];
    if (schedule === undefined) throw new Error(`Unknown provider tariff schedule for ${id}`);
    return [id, { ...quote.prices, offPeak: { ...schedule, prices: quote.offPeak.prices } }];
  }));
}

// Produce the reference catalog and runtime block from the frozen snapshot and audited overlay.
// 从冻结快照及已核查的覆盖数据生成参考目录和运行时区块。
// root is the plugin directory; check rejects stale generated files without writing them.
// root 是插件目录；check 模式只报生成产物过期，不写入文件。
export function syncReferencePricing(root, check = false) {
  // snapshot retains the exact upstream input and its license provenance.
  // snapshot 保留上游输入的原始内容和许可证来源。
  const snapshotText = readFileSync(resolve(root, "pricing/upstream/cc-switch-v3.19.2-model-pricing.v1.json"), "utf8");
  const snapshot = JSON.parse(snapshotText);
  // official contributes verified quotes and explicit launch evidence for legacy removals.
  // official 提供已核实的报价与清理历史价格所需的发布日期证据。
  const official = JSON.parse(readFileSync(resolve(root, sourceFile), "utf8"));
  // models starts with retained USD references and lets official prices replace matching entries.
  // models 先纳入保留的美元参考价，再由官网报价覆盖同名条目。
  const models = new Map(snapshot.models.map((model) => [model.modelId, {
    canonicalModelId: model.modelId,
    displayName: model.displayName,
    aliases: [],
    prices: Object.fromEntries(priceKeys.map((key, index) => [key, [
      model.inputCostPerMillion, model.outputCostPerMillion,
      model.cacheReadCostPerMillion, model.cacheCreationCostPerMillion,
    ][index]])),
  }]));
  for (const [id, evidence] of Object.entries(official.removedLegacy)) {
    if (!models.delete(id) || evidence.released >= official.retainFrom || !evidence.source.startsWith("https://")) {
      throw new Error(`Invalid dated legacy pricing removal for ${id}`);
    }
  }
  for (const [id, entry] of Object.entries(official.models)) {
    if (Object.hasOwn(official.removedLegacy, id)) throw new Error(`Official quote would restore retired reference ${id}`);
    if (entry.sources.length === 0 || entry.sources.some((url) => !url.startsWith("https://"))) {
      throw new Error(`Official quote needs a provider source for ${id}`);
    }
    models.set(id, { canonicalModelId: id, displayName: entry.displayName, aliases: [],
      prices: entry.prices, contextTiers: entry.contextTiers, offPeak: entry.offPeak, sources: entry.sources, basis: entry.basis });
  }
  for (const [canonical, group] of Object.entries(official.billingGroups)) {
    // prefixes contains the family stems used to identify quoted variants.
    // prefixes 保存识别已报价变体所需的系列主干。
    const prefixes = group.prefixes ?? [];
    // aliases keeps spellings that cannot already resolve through the family's rule.
    // aliases 只保留无法通过该系列规则直接归组的写法。
    const aliases = [...new Set(group.aliases ?? [])].filter((id) =>
      id.toLowerCase() !== canonical.toLowerCase() && !prefixes.some((prefix) => matchesBillingFamily(id.toLowerCase(), prefix))).sort();
    // members folds quoted variants into the canonical quote without publishing them as aliases.
    // members 将已报价的变体合并到主报价，生成目录时无需为这些变体再声明别名。
    const members = new Set([...aliases, ...(group.members ?? [])]);
    for (const id of models.keys()) {
      if (id !== canonical && prefixes.some((prefix) => matchesBillingFamily(id.toLowerCase(), prefix))) members.add(id);
    }
    // quotes places the canonical quote first when an audited exception selects its tariff.
    // quotes 在已核查的例外明确采用主型号费率时，将主报价置于首位。
    const quotes = [canonical, ...members].filter((id) => models.has(id)).map((id) => models.get(id));
    const reference = mergeGroupQuotes(canonical, quotes, group);
    for (const id of members) models.delete(id);
    models.set(canonical, { ...reference, canonicalModelId: canonical, aliases,
      ...(group.members?.length ? { members: group.members } : {}), ...(prefixes.length ? { prefixes } : {}) });
  }
  // formattedGroups applies the same Preview and Claude identity rule to every priced model.
  // formattedGroups 对每个有报价的型号应用同一套 Preview 和 Claude 身份规则。
  const formattedGroups = new Map();
  for (const [id, model] of models) {
    // canonical is also the key the runtime searches after removing format-only differences.
    // canonical 同时是运行时去除纯格式差异后使用的查找键。
    const canonical = catalogBillingModelId(id);
    if (!formattedGroups.has(canonical)) formattedGroups.set(canonical, []);
    formattedGroups.get(canonical).push(model);
    if (id !== canonical) models.delete(id);
  }
  for (const [canonical, quotes] of formattedGroups) {
    if (quotes.length === 1 && quotes[0].canonicalModelId === canonical) continue;
    // reference keeps the original tariff while the display name follows the canonical group.
    // reference 保留原费率，展示名随主费用组更新。
    const reference = mergeGroupQuotes(canonical, quotes, {});
    models.set(canonical, { ...reference, canonicalModelId: canonical,
      displayName: reference.displayName.replace(/[-\s]preview(?:[-\s]latest)?$/i, "") });
  }
  // excludedGptModelIds records displaced pricing keys without changing the frozen source.
  // excludedGptModelIds 记录从费用表移出的定价键，冻结来源仍保持原样。
  const excludedGptModelIds = [...models.keys()].filter((id) => !isPricingModel(id)).sort();
  for (const id of excludedGptModelIds) models.delete(id);
  // excludedMiniMaxModelIds lists the archived quotes dropped by the MiniMax version floor.
  // excludedMiniMaxModelIds 列出按 MiniMax 版本下限移除的历史报价。
  const excludedMiniMaxModelIds = [...models.keys()].filter((id) => isRetiredMiniMaxModel(id, official.minimumMiniMaxVersion)).sort();
  for (const id of excludedMiniMaxModelIds) models.delete(id);
  // ordered models and stable material keep content revisions independent of source file ordering.
  // 有序模型与稳定的哈希材料使内容修订号独立于来源文件的行顺序。
  const ordered = [...models.keys()].sort().map((id) => models.get(id));
  // providerModels keeps exact provider tariffs separate from shared model defaults.
  // providerModels 将供应商精确报价与共享模型默认价分开。
  const providerModels = official.providerModels;
  // providerEntryModels groups entries that share a regional tariff without duplicating source quotes.
  // providerEntryModels 将共享地区费率的入口归组，避免复制来源报价。
  const providerEntryModels = official.providerEntryModels;
  // nativeUnitReferences are display-only quotes because recorded usage contains Token counts.
  // nativeUnitReferences 仅展示报价，因为现有用量记录保存的是 Token 数。
  const nativeUnitReferences = official.nativeUnitReferences;
  // compiledProviders and compiledEntries are exact runtime indexes for provider and regional prices.
  // compiledProviders 与 compiledEntries 是供应商及地区价格的精确运行时索引。
  const compiledProviders = Object.fromEntries(Object.entries(providerModels).map(([provider, entries]) =>
    [provider, compileProviderPrices(entries, official.tariffSchedules)]));
  const compiledEntries = Object.fromEntries(Object.entries(providerEntryModels).map(([provider, groups]) => {
    // entries maps stable service entry IDs to their shared regional tariff table.
    // entries 将稳定服务入口标识映射至共用的地区报价表。
    const entries = {};
    for (const group of Object.values(groups)) {
      // compiled stores rates shared by all explicitly listed entries in one region.
      // compiled 保存同一地区明确列出的入口共用费率。
      const compiled = compileProviderPrices(group.models, official.tariffSchedules);
      for (const id of group.serviceEntryIds) {
        if (Object.hasOwn(entries, id)) throw new Error(`Duplicate service entry tariff: ${provider}/${id}`);
        entries[id] = compiled;
      }
    }
    return [provider, entries];
  }));
  const material = { schemaVersion: 1, currencies: ["USD", "CNY"], unit: "per_million_tokens",
    compiledProviders, compiledEntries, nativeUnitReferences,
    models: ordered.map(({ canonicalModelId, prices, contextTiers, offPeak, aliases, members, prefixes }) =>
      ({ canonicalModelId, prices, contextTiers, offPeak, aliases, members, prefixes })) };
  const revision = `sha256:${createHash("sha256").update(jsonText(material)).digest("hex")}`;
  const catalog = {
    schemaVersion: 1,
    catalogId: "vulcan.reference.model-pricing",
    catalogRevision: revision,
    adapterVersion: 2,
    currencies: material.currencies,
    unit: material.unit,
    matching: { strategy: "exact_canonical_model_id", aliases: "explicit_only" },
    source: {
      kind: "official_prices_and_vendored_snapshot",
      snapshotSha256: createHash("sha256").update(jsonText(snapshot)).digest("hex"),
      provenance: snapshot.source,
      official: sourceFile,
      verifiedOn: official.verifiedOn,
      retainFrom: official.retainFrom,
    },
    excludedGptModelIds,
    excludedMiniMaxModelIds,
    minimumMiniMaxVersion: official.minimumMiniMaxVersion,
    providerModels,
    providerEntryModels,
    tariffSchedules: official.tariffSchedules,
    nativeUnitReferences,
    models: ordered,
  };
  // block matches only the catalog region of the self-contained bundle.
  // block 只匹配自包含运行包中的价格目录区块。
  const block = /-- BEGIN GENERATED VULCAN DEFAULT PRICING CATALOG\r?\n[\s\S]*?-- END GENERATED VULCAN DEFAULT PRICING CATALOG/;
  const bundlePath = resolve(root, "runtime/plugin.bundle.lua");
  const bundle = readFileSync(bundlePath, "utf8");
  if (!block.test(bundle)) throw new Error("The bundled pricing catalog markers are missing.");
  // generated projects only fields consumed by the Lua estimator.
  // generated 只投影 Lua 估算器消费的字段。
  const generated = [
    "-- BEGIN GENERATED VULCAN DEFAULT PRICING CATALOG",
    "-- Generated by pricing/tools/sync-reference-pricing.mjs.",
    "-- 由 pricing/tools/sync-reference-pricing.mjs 生成。",
    "local DEFAULT_PRICING_CATALOG = {",
    `    catalog_id = ${JSON.stringify(catalog.catalogId)},`,
    `    revision = ${JSON.stringify(revision)},`,
    '    currency = "USD",',
    '    matching_strategy = "exact_canonical_model_id",',
    `    entry_count = ${ordered.length},`,
    "    models = {",
    ...ordered.map(({ canonicalModelId, prices, contextTiers, offPeak }) =>
      `    [${JSON.stringify(canonicalModelId)}] = ${luaLiteral({ ...prices,
        ...(contextTiers ? { contextTiers } : {}), ...(offPeak ? { offPeak } : {}) })},`),
    "    },",
    `    provider_models = ${luaLiteral(compiledProviders)},`,
    `    provider_entry_models = ${luaLiteral(compiledEntries)},`,
    `    native_unit_references = ${luaLiteral(nativeUnitReferences)},`,
    "}",
    "-- END GENERATED VULCAN DEFAULT PRICING CATALOG",
  ].join(bundle.includes("\r\n") ? "\r\n" : "\n");
  const catalogPath = resolve(root, "pricing/catalog/vulcan-model-pricing.v1.json");
  const updated = bundle.replace(block, () => generated);
  if (check) {
    if (updated !== bundle || readFileSync(catalogPath, "utf8").replaceAll("\r\n", "\n") !== jsonText(catalog)) {
      throw new Error("Run pricing/tools/sync-reference-pricing.mjs to refresh the reference catalog.");
    }
  } else {
    writeFileSync(catalogPath, jsonText(catalog));
    if (updated !== bundle) writeFileSync(bundlePath, updated);
  }
  return { catalogEntryCount: ordered.length, catalogRevision: revision, snapshotSha256: catalog.source.snapshotSha256 };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  console.log(syncReferencePricing(fileURLToPath(new URL("../..", import.meta.url)), process.argv.includes("--check")));
}
