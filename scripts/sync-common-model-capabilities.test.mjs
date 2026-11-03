// Check record derivation, cross referencing and serialization of the common catalog generator.
// 校验公共目录生成器的记录派生、交叉参照与序列化。
import assert from 'node:assert/strict';
import test from 'node:test';
import {
  catalogDocument,
  crossReference,
  providerRecords,
  publishedRecords,
  serialize,
  upstreamRecords,
} from './sync-common-model-capabilities.mjs';

// manifest builds a minimal manifest with the given category, default capabilities, rules and catalog ids.
// manifest 用给定分类、默认能力、规则与目录标识构造最小清单。
const manifest = (category, defaults, rules = [], catalog = []) => ({
  display: { category },
  model_defaults: { capabilities: defaults },
  model_rules: rules,
  services: [{ model_catalog: { models: catalog.map((model_id) => ({ model_id })) } }],
});
const exact = (value, capabilities) => ({ matcher: { kind: 'exact', value }, capabilities });
const prefix = (value, capabilities) => ({ matcher: { kind: 'prefix', value }, capabilities });
const chat = { operations: ['conversation_respond'] };

test('official manifests own a shared identifier before relays', () => {
  const records = providerRecords([
    { fileName: 'a-relay.json', manifest: manifest('relay', chat, [exact('Vendor/Model-X', { context_limit: 1 })]) },
    { fileName: 'z-official.json', manifest: manifest('official', chat, [exact('vendor/model-x', { context_limit: 2 })]) },
  ]);
  assert.deepEqual(records.map((record) => [record.key, record.context_window]), [['vendor/model-x', 2]]);
  assert.deepEqual(records[0].aliases, ['model-x']);
});

test('a rule field overrides the default by presence and catalog models use the best rule', () => {
  const defaults = { ...chat, reasoning: 'native', reasoning_efforts: ['low', 'high'], output_resources: [] };
  const rules = [
    exact('plain', { reasoning: null, reasoning_efforts: [] }),
    prefix('fam', { input_resources: ['image', 'hologram'] }),
    prefix('fam-pro', { input_resources: ['audio'], output_limit: 64 }),
  ];
  const records = providerRecords([{ fileName: 'p.json', manifest: manifest('official', defaults, rules, ['fam-pro-1', 'other']) }]);
  const byKey = Object.fromEntries(records.map((record) => [record.key, record]));
  assert.equal(byKey.plain.reasoning, null);
  assert.deepEqual(byKey.plain.reasoning_efforts, []);
  assert.deepEqual(byKey['fam-pro-1'].input_modalities, ['audio', 'text']);
  assert.equal(byKey['fam-pro-1'].output_limit, 64);
  assert.deepEqual(byKey.other.reasoning_efforts, ['low', 'high']);
});

test('operations decide implicit text and an explicitly empty list removes it', () => {
  const records = providerRecords([{
    fileName: 'p.json',
    manifest: manifest('official', chat, [
      exact('embed', { operations: ['embedding_create'] }),
      exact('none', { operations: [], input_resources: ['audio'] }),
    ]),
  }]);
  const byKey = Object.fromEntries(records.map((record) => [record.key, record]));
  assert.deepEqual([byKey.embed.input_modalities, byKey.embed.output_modalities], [['text'], []]);
  assert.deepEqual([byKey.none.input_modalities, byKey.none.output_modalities], [['audio'], []]);
});

test('upstream fills only empty fields, matches a unique tail and sizes only text output', () => {
  const records = providerRecords([{
    fileName: 'p.json',
    manifest: manifest('official', chat, [
      exact('model-a', {}),
      exact('painter', { operations: ['image_generate'], output_resources: ['image'] }),
      exact('sized', { context_limit: 10 }),
      exact('vendor/model-b', {}),
    ]),
  }]);
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
  const sources = [{
    fileName: 'p.json',
    manifest: manifest('official', chat, [exact('vendor/model-a', {}), exact('painter', { operations: ['image_generate'], output_resources: ['image'] })]),
  }];
  const upstream = upstreamRecords({ data: [
    { id: 'vendor/model-a', canonical_slug: 'vendor/model-a-2026', context_length: 400, architecture: { input_modalities: ['text', 'file'] } },
  ] });
  const online = serialize(catalogDocument(crossReference(providerRecords(sources), upstream, { matchTail: true }).records));
  const published = publishedRecords(JSON.parse(online));
  const offline = serialize(catalogDocument(crossReference(providerRecords(sources), published, { matchTail: false }).records));
  assert.equal(offline, online);
  assert.match(online, /"vendor\/model-a-2026"/);
  assert.ok(!online.includes('\r'));
});
