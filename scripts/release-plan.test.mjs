// Exercise the release plan on synthetic resource trees.
// 使用合成的资源目录验证发布计划。
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { NOTES_SCHEMA, notesDocument, releasePlan } from './release-plan.mjs';

const codes = ['en-US', 'zh-CN'];

// resources writes product.json and a one-release changelog dated `date`, and returns the directory.
// resources 写入 product.json 与一条日期为 `date` 的更新记录，并返回目录。
function resources(version, date, installEpoch = 1) {
  const directory = mkdtempSync(path.join(os.tmpdir(), 'release-plan-'));
  writeFileSync(path.join(directory, 'product.json'), JSON.stringify({ version, installEpoch }));
  const changelog = path.join(directory, 'changelog');
  mkdirSync(path.join(changelog, version), { recursive: true });
  writeFileSync(path.join(changelog, 'releases.json'), JSON.stringify({ releases: [{ version, date }] }));
  for (const code of codes) writeFileSync(path.join(changelog, version, `${code}.md`), `Notes in ${code}.`);
  return directory;
}

test('a dated release of the product version is planned and its notes cover every language', t => {
  const directory = resources('1.2.0', '2026-11-05', 2);
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const plan = releasePlan(directory, codes, '1.2.0');
  assert.deepEqual(plan, { version: '1.2.0', installEpoch: 2, date: '2026-11-05' });
  const document = notesDocument(directory, codes, plan);
  assert.equal(document.schema, NOTES_SCHEMA);
  assert.equal(document.version, '1.2.0');
  assert.deepEqual(Object.keys(document.notes), codes);
});

test('an undated release, another version or a missing install epoch is refused', t => {
  const undated = resources('1.2.0', null);
  const dated = resources('1.2.0', '2026-11-05');
  const unepoched = resources('1.2.0', '2026-11-05', 0);
  t.after(() => {
    for (const directory of [undated, dated, unepoched]) rmSync(directory, { recursive: true, force: true });
  });
  assert.throws(() => releasePlan(undated, codes, '1.2.0'), /not ready/);
  assert.throws(() => releasePlan(dated, codes, '1.2.1'), /not ready/);
  assert.throws(() => releasePlan(unepoched, codes, '1.2.0'), /not ready/);
});
