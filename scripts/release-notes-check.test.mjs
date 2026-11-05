// Exercise the release-notes check on synthetic changelog trees.
// 使用合成的更新记录目录验证更新记录检查。
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { checkReleaseNotes } from './release-notes-check.mjs';

// changelog writes `releases` and one notes file per code and version, and returns its directory.
// changelog 写入 `releases` 及每个版本、每种语言的说明文件，并返回其目录。
function changelog(releases, codes, notes = 'Notes.') {
  const directory = mkdtempSync(path.join(os.tmpdir(), 'release-notes-'));
  writeFileSync(path.join(directory, 'releases.json'), JSON.stringify({ releases }));
  for (const { version } of releases) {
    mkdirSync(path.join(directory, version));
    for (const code of codes) writeFileSync(path.join(directory, version, `${code}.md`), notes);
  }
  return directory;
}

test('accepts ordered releases that cover every language and match the product version', () => {
  const directory = changelog([{ version: '0.2.0', date: null }, { version: '0.1.0', date: '2026-11-01' }], ['en-US', 'zh-CN']);
  assert.deepEqual(checkReleaseNotes(directory, ['en-US', 'zh-CN'], '0.2.0'), []);
  rmSync(directory, { recursive: true });
});

test('rejects a newest release that differs from the product version', () => {
  const directory = changelog([{ version: '0.1.0', date: null }], ['en-US']);
  assert.equal(checkReleaseNotes(directory, ['en-US'], '0.2.0').length, 1);
  rmSync(directory, { recursive: true });
});

test('rejects English notes whose components have no GitHub Release form', () => {
  const directory = changelog([{ version: '0.1.0', date: null }], ['en-US', 'zh-CN'], '```ui-steps\ndone | Step\n```\n');
  assert.equal(checkReleaseNotes(directory, ['en-US', 'zh-CN'], '0.1.0').length, 1);
  rmSync(directory, { recursive: true });
});

test('rejects unordered versions and a null date below the top', () => {
  const directory = changelog([{ version: '0.1.0', date: '2026-11-01' }, { version: '0.2.0', date: null }], ['en-US']);
  assert.equal(checkReleaseNotes(directory, ['en-US'], '0.1.0').length, 2);
  rmSync(directory, { recursive: true });
});

test('rejects a missing language, empty notes and unlisted directories', () => {
  const directory = changelog([{ version: '0.1.0', date: null }], ['en-US'], ' ');
  mkdirSync(path.join(directory, '0.0.9'));
  const problems = checkReleaseNotes(directory, ['en-US', 'zh-CN'], '0.1.0');
  assert.equal(problems.length, 3);
  rmSync(directory, { recursive: true });
});
