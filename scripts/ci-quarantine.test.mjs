// Validate the CI quarantine schema and the nextest filtersets derived from it.
// 校验 CI 隔离清单结构及由其派生的 nextest 过滤表达式。
import assert from 'node:assert/strict';
import test from 'node:test';
import { loadQuarantine, parseQuarantine, quarantinedFilter } from './ci-quarantine.mjs';

// entry is one valid quarantine record that negative cases mutate.
// entry 是一条有效隔离记录，供反向用例修改。
const entry = () => ({ package: 'vulcan-llm-core', test: 'stream::tests::retries', reason: 'Races the mock server.', added: '2026-11-03', owner: 'lingeasy' });

test('the repository list is valid', () => {
  loadQuarantine();
});

test('an empty list selects no test, and entries select exact package and test pairs', () => {
  assert.equal(quarantinedFilter(parseQuarantine({ schema_version: 1, tests: [] })), 'none()');
  const second = { ...entry(), package: 'vulcan-code-service', test: 'logging::tests::rotate' };
  assert.equal(quarantinedFilter(parseQuarantine({ schema_version: 1, tests: [entry(), second] })),
    '(package(=vulcan-llm-core) & test(=stream::tests::retries)) | (package(=vulcan-code-service) & test(=logging::tests::rotate))');
});

test('malformed lists and entries are rejected', () => {
  assert.throws(() => parseQuarantine({ schema_version: 2, tests: [] }), /schema/);
  assert.throws(() => parseQuarantine({ schema_version: 1 }), /schema/);
  for (const mutate of [
    value => { delete value.owner; },
    value => { value.extra = 'x'; },
    value => { value.reason = ' '; },
    value => { value.owner = ''; },
    value => { value.added = '3 Nov 2026'; },
    value => { value.test = 'name with spaces'; },
    value => { value.package = 'a)|all()|('; },
  ]) {
    const value = entry();
    mutate(value);
    assert.throws(() => parseQuarantine({ schema_version: 1, tests: [value] }));
  }
  assert.throws(() => parseQuarantine({ schema_version: 1, tests: [entry(), entry()] }), /duplicates/);
});
