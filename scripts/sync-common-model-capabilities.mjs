// Regenerate the common model capability catalog from the bundled provider manifests.
// 由随包供应商清单重新生成公共模型能力目录。
//
// Usage: node scripts/sync-common-model-capabilities.mjs [--offline] [--source <file|url>] [--check] [--dry-run]
// 1. Every exact manifest model rule and every static catalog model becomes one record; official
//    manifests own a shared identifier before cloud platforms, and cloud platforms before relays.
// 2. The records are cross referenced with OpenRouter's public model list, which only fills fields a
//    manifest leaves empty and contributes aliases; upstream-only models are excluded.
// 3. --offline cross references the published catalog instead, so adding a manifest model offline
//    keeps every upstream alias and window already published.
// 4. --check writes nothing and fails when the published catalog differs from the generated one.
import { readFileSync, readdirSync, renameSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// repositoryRoot anchors every path so the script works from any working directory.
// repositoryRoot 锚定所有路径，使脚本可在任意工作目录运行。
const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const manifestDirectory = path.join(repositoryRoot, 'crates/vulcan-runtime-assets/resources/provider-manifests/manifests');
const catalogFile = path.join(repositoryRoot, 'crates/vulcan-provider-adapters/resources/common_model_capabilities.json');
const upstreamUrl = 'https://openrouter.ai/api/v1/models?output_modalities=all';

// MODALITIES is the closed Vulcan modality vocabulary; anything else is dropped.
// MODALITIES 是封闭的 Vulcan 模态词表；其余取值一律丢弃。
const MODALITIES = new Set(['text', 'image', 'audio', 'video', 'file']);

// TEXT_ONLY_FALLBACK_WINDOW is the context window given to text-only records nobody sizes. It mirrors
// Rust's COMMON_MODEL_FALLBACK_CONTEXT_WINDOW, and the embedded-catalog loader asserts they match.
// TEXT_ONLY_FALLBACK_WINDOW 是无人声明窗口的纯文本记录所得的上下文窗口。它对应 Rust 的
// COMMON_MODEL_FALLBACK_CONTEXT_WINDOW，嵌入目录加载器会断言两者一致。
const TEXT_ONLY_FALLBACK_WINDOW = 200000;

// normalizeKey turns one public model identifier into the runtime comparison key.
// normalizeKey 将一个公共模型标识规范化为运行时比较键。
export function normalizeKey(value) {
  return String(value ?? '').trim().toLowerCase();
}

// tailOf returns the identifier after the last slash, which is how suffix matching sees a model.
// tailOf 返回最后一个斜杠之后的标识，即后缀匹配看到的模型名。
function tailOf(key) {
  return key.split('/').at(-1);
}

// uniqueSorted deduplicates and sorts by code unit so output never depends on the host locale.
// uniqueSorted 去重并按码元排序，使输出不依赖宿主区域设置。
function uniqueSorted(values) {
  return [...new Set(values)].sort((left, right) => (left < right ? -1 : left > right ? 1 : 0));
}

// modalityList maps declared or reported modalities into the vocabulary, sorted and deduplicated.
// modalityList 将声明或上报的模态映射到词表，并排序去重。
function modalityList(values) {
  return uniqueSorted((values ?? []).map(normalizeKey).filter((value) => MODALITIES.has(value)));
}

// declared reads one capability by field presence: a rule field, even null or empty, overrides the default.
// declared 按字段存在性读取能力：规则字段即使为 null 或空也覆盖默认值。
function declared(defaults, specific, name) {
  if (specific && Object.hasOwn(specific, name)) return specific[name];
  if (defaults && Object.hasOwn(defaults, name)) return defaults[name];
  return null;
}

// providerRecord builds one record from a manifest's defaults and the optional matched rule.
// providerRecord 由清单默认值与可选匹配规则构建一条记录。
//
// Manifests declare only non-text resources: any declared operation implies text input, and
// conversation_respond implies text output. Undeclared operations count as one implicit operation.
// 清单只声明非文本资源：任一已声明 operation 隐含文本输入，conversation_respond 隐含文本输出。
// 未声明 operations 视为一项隐含 operation。
function providerRecord(key, manifest, rule) {
  const defaults = manifest.model_defaults?.capabilities ?? null;
  const specific = rule?.capabilities ?? null;
  const operations = declared(defaults, specific, 'operations');
  const contextLimit = declared(defaults, specific, 'context_limit');
  const efforts = declared(defaults, specific, 'reasoning_efforts');
  const tail = tailOf(key);
  return {
    key,
    aliases: tail === key ? [] : [tail],
    context_window: contextLimit > 0 ? contextLimit : null,
    input_modalities: modalityList([
      ...(operations === null || operations.length > 0 ? ['text'] : []),
      ...(declared(defaults, specific, 'input_resources') ?? []),
    ]),
    output_modalities: modalityList([
      ...((operations ?? []).includes('conversation_respond') ? ['text'] : []),
      ...(declared(defaults, specific, 'output_resources') ?? []),
    ]),
    output_limit: declared(defaults, specific, 'output_limit'),
    reasoning: declared(defaults, specific, 'reasoning'),
    reasoning_control: declared(defaults, specific, 'reasoning_control'),
    reasoning_efforts: efforts ?? [],
  };
}

// bestRule picks the rule a catalog model resolves to: an exact rule first, then the longest prefix.
// bestRule 选出目录模型解析到的规则：精确规则优先，其次最长前缀。
function bestRule(rules, key) {
  let best = null;
  let bestRank = -1;
  for (const rule of rules) {
    const value = normalizeKey(rule.matcher?.value);
    const kind = rule.matcher?.kind;
    let rank = -1;
    if (kind === 'exact' && value === key) rank = Number.MAX_SAFE_INTEGER;
    else if (kind === 'prefix' && key.startsWith(value)) rank = value.length;
    if (rank > bestRank) {
      best = rule;
      bestRank = rank;
    }
  }
  return best;
}

// CATEGORY_AUTHORITY ranks manifest categories for shared model facts; a lower tier wins. Cloud
// platforms are more authoritative than relays, which resell upstream access and carry risk.
// CATEGORY_AUTHORITY 为公共模型事实给清单类别排序，层级越低越优先。云平台比中转更权威，中转转售上游访问且存在风险。
const CATEGORY_AUTHORITY = { official: 0, cloud_maa_s: 1, relay: 2, unclassified: 2 };

// providerRecords inventories every exact rule and static catalog model; the first owner of a key wins.
// providerRecords 盘点每条精确规则与静态目录模型；首个持有某键的清单胜出。
//
// manifests is a list of { fileName, manifest }. Official providers own shared model facts first,
// then cloud platforms, then relays and unclassified ones; file names decide within a tier.
// manifests 为 { fileName, manifest } 列表。官方供应商最先持有公共模型事实，其次云平台，再次中转与未分类；同层按文件名。
export function providerRecords(manifests) {
  const ordered = manifests
    .map((source) => ({ ...source, authority: CATEGORY_AUTHORITY[source.manifest.display?.category] ?? CATEGORY_AUTHORITY.relay }))
    .sort((left, right) => left.authority - right.authority || (left.fileName < right.fileName ? -1 : 1));
  const records = [];
  const keys = new Set();
  const claim = (key, manifest, rule) => {
    if (!key || keys.has(key)) return;
    keys.add(key);
    records.push(providerRecord(key, manifest, rule));
  };
  for (const { manifest } of ordered) {
    const rules = manifest.model_rules ?? [];
    // Only exact matchers name a model identity; prefix matchers are family selectors.
    // 只有 exact 匹配器才指向一个模型身份；prefix 匹配器是族选择器。
    for (const rule of rules) {
      if (rule.matcher?.kind === 'exact') claim(normalizeKey(rule.matcher.value), manifest, rule);
    }
    for (const service of manifest.services ?? []) {
      for (const model of service.model_catalog?.models ?? []) {
        const key = normalizeKey(model.model_id);
        claim(key, manifest, bestRule(rules, key));
      }
    }
  }
  return records;
}

// upstreamRecords reads OpenRouter's model list into cross-reference records.
// upstreamRecords 将 OpenRouter 模型列表读取为交叉参照记录。
//
// Missing upstream modality metadata stays unknown; only signed manifests supply text defaults.
// 上游模态元数据缺失时维持未知；只有受签名清单提供文本默认值。
export function upstreamRecords(response) {
  if (!Array.isArray(response?.data)) throw new Error('OpenRouter model response does not contain a data array.');
  const keys = new Set();
  const records = [];
  for (const model of response.data) {
    const key = normalizeKey(model.id);
    if (!key) continue;
    if (keys.has(key)) throw new Error(`OpenRouter model response contains duplicate model identifier '${key}'.`);
    keys.add(key);
    const aliases = [model.canonical_slug, model.name, tailOf(key)]
      .map(normalizeKey)
      .filter((alias) => alias && alias !== key);
    records.push({
      key,
      aliases: uniqueSorted(aliases),
      context_window: model.context_length > 0 ? model.context_length : null,
      input_modalities: modalityList(model.architecture?.input_modalities),
      output_modalities: modalityList(model.architecture?.output_modalities),
    });
  }
  return records;
}

// publishedRecords reads a published catalog as the offline cross reference.
// publishedRecords 将已发布目录读取为离线交叉参照。
export function publishedRecords(document) {
  return (document?.models ?? []).map((model) => ({
    key: model.key,
    aliases: model.aliases ?? [],
    context_window: model.context_window ?? null,
    input_modalities: model.input_modalities ?? [],
    output_modalities: model.output_modalities ?? [],
  }));
}

// isTextOnly reports whether a record produces only text, the one case a token window sizes.
// isTextOnly 判断记录是否只产出文本，这是令牌窗口唯一能界定的情形。
function isTextOnly(outputs) {
  return outputs.length === 1 && outputs[0] === 'text';
}

// crossReference fills empty provider fields from the reference records and adds their aliases.
// crossReference 用参照记录填充供应商留空的字段并补充其别名。
//
// Provider declarations stay authoritative and reference-only models are excluded. matchTail lets a
// provider key match a reference by its unique tail, which only the upstream list needs.
// 供应商声明保持权威，仅存在于参照中的模型被排除。matchTail 允许供应商键按唯一尾名匹配参照，
// 只有上游列表需要它。
export function crossReference(records, references, { matchTail }) {
  const byKey = new Map();
  const byTail = new Map();
  for (const reference of references) {
    if (!byKey.has(reference.key)) byKey.set(reference.key, reference);
    const tail = tailOf(reference.key);
    byTail.set(tail, byTail.has(tail) ? null : reference);
  }
  const matched = new Set();
  const merged = records.map((record) => {
    const reference = byKey.get(record.key) ?? (matchTail ? byTail.get(tailOf(record.key)) : null) ?? null;
    if (!reference) return record;
    matched.add(reference.key);
    // A remote token window is not proof of a media-generation endpoint's input budget.
    // 远端 Token 窗口不是媒体生成入口输入预算的证明。
    const fillWindow = record.context_window === null && isTextOnly(record.output_modalities) && reference.context_window > 0;
    return {
      ...record,
      aliases: uniqueSorted([...record.aliases, ...reference.aliases]),
      context_window: fillWindow ? reference.context_window : record.context_window,
      input_modalities: record.input_modalities.length ? record.input_modalities : modalityList(reference.input_modalities),
      output_modalities: record.output_modalities.length ? record.output_modalities : modalityList(reference.output_modalities),
    };
  });
  return { records: merged, enriched: matched.size, excluded: references.length - matched.size };
}

// catalogDocument builds the deterministic catalog document; an alias survives only with one owner.
// catalogDocument 构建确定性目录文档；别名只有唯一归属时才保留。
export function catalogDocument(records) {
  const canonical = new Set(records.map((record) => record.key));
  const owners = new Map();
  for (const record of records) {
    for (const alias of new Set(record.aliases)) owners.set(alias, (owners.get(alias) ?? 0) + 1);
  }
  const models = [...records]
    .sort((left, right) => (left.key < right.key ? -1 : left.key > right.key ? 1 : 0))
    .map((record) => {
      const entry = {
        key: record.key,
        aliases: uniqueSorted(record.aliases.filter((alias) => owners.get(alias) === 1 && !canonical.has(alias))),
      };
      if (record.context_window > 0) entry.context_window = record.context_window;
      else if (isTextOnly(record.output_modalities)) entry.context_window = TEXT_ONLY_FALLBACK_WINDOW;
      entry.input_modalities = record.input_modalities;
      entry.output_modalities = record.output_modalities;
      if (record.output_limit !== null) entry.output_limit = record.output_limit;
      if (record.reasoning !== null) entry.reasoning = record.reasoning;
      if (record.reasoning_control !== null) entry.reasoning_control = record.reasoning_control;
      if (record.reasoning_efforts.length > 0) entry.reasoning_efforts = record.reasoning_efforts;
      return entry;
    });
  return {
    schema_version: 2,
    sources: [
      'vulcan:bundled-provider-manifests',
      'vulcan:provider-specific-capabilities',
      'openrouter:/api/v1/models?output_modalities=all (cross reference for context and modality metadata only)',
    ],
    fallback: { context_window: TEXT_ONLY_FALLBACK_WINDOW, input_modalities: ['text'], output_modalities: ['text'] },
    models,
  };
}

// serialize renders the catalog the same way on every host: two-space JSON, LF, trailing newline.
// serialize 在任何宿主上都以同样方式输出目录：两空格缩进 JSON、LF 换行、末尾换行。
export function serialize(document) {
  return `${JSON.stringify(document, null, 2)}\n`;
}

// readJson parses one UTF-8 JSON file, tolerating a byte-order mark.
// readJson 解析一个 UTF-8 JSON 文件，容忍字节序标记。
function readJson(file) {
  return JSON.parse(readFileSync(file, 'utf8').replace(/^﻿/, ''));
}

// loadUpstream reads the upstream model list from a local file or fetches it from a URL.
// loadUpstream 从本地文件读取上游模型列表，或从 URL 获取。
async function loadUpstream(source) {
  if (!/^https?:\/\//.test(source)) return readJson(path.resolve(source));
  const response = await fetch(source, { headers: { 'User-Agent': 'VulcanCode-CapabilityCatalog/1.0' } });
  if (!response.ok) throw new Error(`OpenRouter model request failed with HTTP ${response.status}.`);
  return response.json();
}

// parseArguments reads the command line; an unknown flag is an error rather than ignored.
// parseArguments 读取命令行；未知参数直接报错而非忽略。
function parseArguments(argv) {
  const options = { offline: false, check: false, dryRun: false, source: upstreamUrl };
  for (let index = 0; index < argv.length; index += 1) {
    const flag = argv[index];
    if (flag === '--offline') options.offline = true;
    else if (flag === '--check') options.check = true;
    else if (flag === '--dry-run') options.dryRun = true;
    else if (flag === '--source' && index + 1 < argv.length) options.source = argv[(index += 1)];
    else throw new Error(`Unknown argument '${flag}'.`);
  }
  if (options.offline && options.source !== upstreamUrl) throw new Error('--offline and --source are exclusive.');
  return options;
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  const manifests = readdirSync(manifestDirectory)
    .filter((name) => name.endsWith('.json'))
    .map((fileName) => ({ fileName, manifest: readJson(path.join(manifestDirectory, fileName)) }));
  const records = providerRecords(manifests);
  const publishedText = readFileSync(catalogFile, 'utf8').replace(/\r\n/g, '\n');
  const published = JSON.parse(publishedText);
  const references = options.offline ? publishedRecords(published) : upstreamRecords(await loadUpstream(options.source));
  const merged = crossReference(records, references, { matchTail: !options.offline });
  const document = catalogDocument(merged.records);
  const text = serialize(document);
  console.log(
    `Inventoried ${records.length} provider-configured record(s); cross-referenced ${merged.enriched} against `
      + `${references.length} ${options.offline ? 'published' : 'upstream'} record(s) and excluded ${merged.excluded}.`,
  );

  const generated = new Set(document.models.map((model) => model.key));
  const removed = published.models.filter((model) => !generated.has(model.key)).map((model) => model.key);
  const added = document.models.filter((model) => !published.models.some((old) => old.key === model.key)).map((model) => model.key);
  if (added.length) console.log(`Adding: ${added.join(', ')}`);
  if (removed.length) console.log(`Removing records no bundled manifest declares: ${removed.join(', ')}`);

  if (options.check) {
    if (text === publishedText) return console.log('Common model capability catalog is up to date.');
    console.error('Common model capability catalog is stale; run `node scripts/sync-common-model-capabilities.mjs --offline`.');
    process.exit(1);
  }
  if (options.dryRun) return;
  const temporary = `${catalogFile}.tmp`;
  writeFileSync(temporary, text);
  renameSync(temporary, catalogFile);
  console.log(`Wrote ${document.models.length} records to ${path.relative(repositoryRoot, catalogFile)}.`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(error.message);
    process.exit(1);
  });
}
