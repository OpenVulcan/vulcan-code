// Check record derivation, cross referencing and serialization of the common catalog generator.
// 校验公共目录生成器的记录派生、交叉参照与序列化。
import assert from 'node:assert/strict';
import test from 'node:test';
import {
  catalogDocument,
  coreRecords,
  crossReference,
  publishedRecords,
  serialize,
  upstreamRecords,
} from './sync-common-model-capabilities.mjs';

// coreList builds a minimal core model list from [model_id, capabilities, aliases] tuples.
// coreList 由 [model_id, capabilities, aliases] 元组构造最小核心模型清单。
const coreList = (...entries) => ({
  models: entries.map(([model_id, capabilities = {}, aliases = []]) => ({ model_id, capabilities, aliases })),
});
const chat = { operations: ['conversation_respond'] };

test('every core entry becomes one record keyed by its model id with tail and core aliases', () => {
  const records = coreRecords(coreList(['vendor/model-x', { ...chat, context_limit: 2 }, ['model x']]));
  assert.deepEqual(records.map((record) => [record.key, record.context_window]), [['vendor/model-x', 2]]);
  assert.deepEqual(records[0].aliases, ['model x', 'model-x']);
});

test('undeclared core fields stay unknown and declared ones carry over', () => {
  const records = coreRecords(coreList(
    ['plain', { ...chat }],
    ['seer', { ...chat, input_resources: ['image', 'hologram'], output_limit: 64, reasoning: 'native', reasoning_efforts: ['low', 'high'] }],
  ));
  const byKey = Object.fromEntries(records.map((record) => [record.key, record]));
  assert.deepEqual([byKey.plain.reasoning, byKey.plain.output_limit, byKey.plain.reasoning_efforts], [null, null, []]);
  assert.deepEqual(byKey.seer.input_modalities, ['image', 'text']);
  assert.equal(byKey.seer.output_limit, 64);
  assert.deepEqual(byKey.seer.reasoning_efforts, ['low', 'high']);
});

test('operations decide implicit text and an explicitly empty list removes it', () => {
  const records = coreRecords(coreList(
    ['embed', { operations: ['embedding_create'] }],
    ['none', { operations: [], input_resources: ['audio'] }],
    ['implicit'],
  ));
  const byKey = Object.fromEntries(records.map((record) => [record.key, record]));
  assert.deepEqual([byKey.embed.input_modalities, byKey.embed.output_modalities], [['text'], []]);
  assert.deepEqual([byKey.none.input_modalities, byKey.none.output_modalities], [['audio'], []]);
  assert.deepEqual([byKey.implicit.input_modalities, byKey.implicit.output_modalities], [['text'], []]);
});

test('a core list without a models array is rejected', () => {
  assert.throws(() => coreRecords({}), /models array/);
});

test('upstream fills only empty fields, matches a unique tail and sizes only text output', () => {
  const records = coreRecords(coreList(
    ['model-a', chat],
    ['painter', { operations: ['image_generate'], output_resources: ['image'] }],
    ['sized', { ...chat, context_limit: 10 }],
    ['vendor/model-b', chat],
  ));
  const upstream = upstreamRecords({ data: [
    { id: 'Lab/Model-A', name: 'Lab: Model A', context_length: 500, architecture: { input_modalities: ['text', 'image'] } },
    { id: 'lab/painter', context_length: 900, architecture: { output_modalities: ['image'] } },
    { id: 'lab/sized', context_length: 900 },
    { id: 'one/model-b', context_length: 1 },
    { id: 'two/model-b', context_length: 2 },
    { id: 'lab/upstream-only' },
  ] });
  const merged = crossReference(records, upstream, { matchTail: true });
  const byKey = Object.fromEntries(merged.records.map((record) => [record.key, record]));
  assert.equal(byKey['model-a'].context_window, 500);
  assert.deepEqual(byKey['model-a'].input_modalities, ['text']);
  assert.deepEqual(byKey['model-a'].aliases, ['lab: model a', 'model-a']);
  assert.equal(byKey.painter.context_window, null);
  assert.equal(byKey.sized.context_window, 10);
  assert.equal(byKey['vendor/model-b'].context_window, null);
  assert.deepEqual([merged.enriched, merged.excluded], [3, 3]);
});

test('duplicate upstream identifiers are rejected', () => {
  assert.throws(() => upstreamRecords({ data: [{ id: 'a' }, { id: 'A' }] }), /duplicate/);
});

test('the document keeps only uniquely owned aliases, falls back for text and omits undeclared fields', () => {
  const base = { context_window: null, output_limit: null, reasoning: null, reasoning_control: null, reasoning_efforts: [] };
  const document = catalogDocument([
    { ...base, key: 'z/model', aliases: ['shared', 'model', 'a/model'], input_modalities: ['text'], output_modalities: ['text'] },
    { ...base, key: 'a/model', aliases: ['shared', 'model-a'], input_modalities: ['text'], output_modalities: ['image'], output_limit: 8, reasoning_efforts: ['high'] },
  ]);
  assert.deepEqual(document.models, [
    { key: 'a/model', aliases: ['model-a'], input_modalities: ['text'], output_modalities: ['image'], output_limit: 8, reasoning_efforts: ['high'] },
    { key: 'z/model', aliases: ['model'], context_window: 200000, input_modalities: ['text'], output_modalities: ['text'] },
  ]);
});

test('offline regeneration from the published catalog reproduces it byte for byte', () => {
  const sources = coreList(['vendor/model-a', chat], ['painter', { operations: ['image_generate'], output_resources: ['image'] }]);
  const upstream = upstreamRecords({ data: [
    { id: 'vendor/model-a', canonical_slug: 'vendor/model-a-2026', context_length: 400, architecture: { input_modalities: ['text', 'file'] } },
  ] });
  const online = serialize(catalogDocument(crossReference(coreRecords(sources), upstream, { matchTail: true }).records));
  const published = publishedRecords(JSON.parse(online));
  const offline = serialize(catalogDocument(crossReference(coreRecords(sources), published, { matchTail: false }).records));
  assert.equal(offline, online);
  assert.match(online, /"vendor\/model-a-2026"/);
  assert.ok(!online.includes('\r'));
});
