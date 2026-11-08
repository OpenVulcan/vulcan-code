// Regenerate the common model capability catalog from the bundled core model list.
// 由随包核心模型清单重新生成公共模型能力目录。
//
// Usage: node scripts/sync-common-model-capabilities.mjs [--offline] [--source <file|url>] [--check] [--dry-run]
// 1. Every core model becomes one record keyed by its model id with the core aliases; provider
//    manifests and their categories no longer decide shared model facts.
// 2. The records are cross referenced with OpenRouter's public model list, which only fills fields the
//    core leaves empty and contributes aliases; upstream-only models are excluded.
// 3. --offline cross references the published catalog instead, so adding a core model offline
//    keeps every upstream alias and window already published.
// 4. --check writes nothing and fails when the published catalog differs from the generated one.
import { readFileSync, renameSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// repositoryRoot anchors every path so the script works from any working directory.
// repositoryRoot 锚定所有路径，使脚本可在任意工作目录运行。
const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const coreModelsFile = path.join(repositoryRoot, 'crates/vulcan-runtime-assets/resources/provider-manifests/core-models.json');
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

// declared reads one core capability; an undeclared field stays unknown.
// declared 读取一项核心能力；未声明的字段保持未知。
function declared(capabilities, name) {
  return capabilities && Object.hasOwn(capabilities, name) ? capabilities[name] : null;
}

// coreRecord builds one catalog record from a core model entry.
// coreRecord 由一个核心模型条目构建一条目录记录。
//
// The core declares only non-text resources: any declared operation implies text input, and
// conversation_respond implies text output. Undeclared operations count as one implicit operation.
// 核心只声明非文本资源：任一已声明 operation 隐含文本输入，conversation_respond 隐含文本输出。
// 未声明 operations 视为一项隐含 operation。
function coreRecord(entry) {
  const key = normalizeKey(entry.model_id);
  const capabilities = entry.capabilities ?? {};
  const operations = declared(capabilities, 'operations');
  const contextLimit = declared(capabilities, 'context_limit');
  const tail = tailOf(key);
  return {
    key,
    aliases: uniqueSorted([...(tail === key ? [] : [tail]), ...(entry.aliases ?? []).map(normalizeKey)].filter((alias) => alias && alias !== key)),
    context_window: contextLimit > 0 ? contextLimit : null,
    input_modalities: modalityList([
      ...(operations === null || operations.length > 0 ? ['text'] : []),
      ...(declared(capabilities, 'input_resources') ?? []),
    ]),
    output_modalities: modalityList([
      ...((operations ?? []).includes('conversation_respond') ? ['text'] : []),
      ...(declared(capabilities, 'output_resources') ?? []),
    ]),
    output_limit: declared(capabilities, 'output_limit'),
    reasoning: declared(capabilities, 'reasoning'),
    reasoning_control: declared(capabilities, 'reasoning_control'),
    reasoning_efforts: declared(capabilities, 'reasoning_efforts') ?? [],
  };
}

// coreRecords turns every entry of the core model list into one catalog record.
// coreRecords 将核心模型清单的每个条目转为一条目录记录。
export function coreRecords(document) {
  if (!Array.isArray(document?.models)) throw new Error('Core model list does not contain a models array.');
  return document.models.map(coreRecord);
}

// upstreamRecords reads OpenRouter's model list into cross-reference records.
// upstreamRecords 将 OpenRouter 模型列表读取为交叉参照记录。
//
// Missing upstream modality metadata stays unknown; only the signed core supplies text defaults.
// 上游模态元数据缺失时维持未知；只有受签名核心清单提供文本默认值。
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

// crossReference fills empty core fields from the reference records and adds their aliases.
// crossReference 用参照记录填充核心留空的字段并补充其别名。
//
// Core declarations stay authoritative and reference-only models are excluded. matchTail lets a
// core key match a reference by its unique tail, which only the upstream list needs.
// 核心声明保持权威，仅存在于参照中的模型被排除。matchTail 允许核心键按唯一尾名匹配参照，
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
      'vulcan:bundled-core-models',
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
  const records = coreRecords(readJson(coreModelsFile));
  const publishedText = readFileSync(catalogFile, 'utf8').replace(/\r\n/g, '\n');
  const published = JSON.parse(publishedText);
  const references = options.offline ? publishedRecords(published) : upstreamRecords(await loadUpstream(options.source));
  const merged = crossReference(records, references, { matchTail: !options.offline });
  const document = catalogDocument(merged.records);
  const text = serialize(document);
  console.log(
    `Inventoried ${records.length} core model record(s); cross-referenced ${merged.enriched} against `
      + `${references.length} ${options.offline ? 'published' : 'upstream'} record(s) and excluded ${merged.excluded}.`,
  );

  const generated = new Set(document.models.map((model) => model.key));
  const removed = published.models.filter((model) => !generated.has(model.key)).map((model) => model.key);
  const added = document.models.filter((model) => !published.models.some((old) => old.key === model.key)).map((model) => model.key);
  if (added.length) console.log(`Adding: ${added.join(', ')}`);
  if (removed.length) console.log(`Removing records the core model list does not declare: ${removed.join(', ')}`);

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
