// Exercise the GitHub Release form of the release notes.
// 验证更新记录的 GitHub Release 形式。
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { RESOURCES_DIR } from './release-notes-check.mjs';
import { GITHUB_NOTES_LANGUAGE, githubReleaseNotes } from './release-notes-github.mjs';

// fence wraps body lines in a fence of `marker` with `info`.
// fence 用带 `info` 的 `marker` 围栏包裹正文行。
function fence(info, lines, marker = '```') {
  return [`${marker}${info}`, ...lines, marker].join('\n');
}

test('row components become Markdown list items with the cells kept', () => {
  const notes = [
    fence('ui-kv', ['| Label | Value |', '|---|---|', 'Command | irm x \\| iex']),
    fence('ui-checklist', ['ok | Done | Note', 'todo | Later']),
    fence('ui-cards', ['Title | Text | warn Beta', 'Plain | Text']),
  ].join('\n\n');
  const lines = githubReleaseNotes(notes).trim().split('\n').filter((line) => line !== '');
  assert.equal(lines.length, 6);
  assert.ok(lines.every((line) => line.startsWith('- ')));
  assert.ok(lines[0].includes('Label') && lines[0].includes('Value'));
  assert.ok(lines[1].includes('irm x | iex'));
  assert.ok(lines[2].includes('Done') && lines[2].includes('Note'));
  assert.ok(lines[4].includes('Beta'));
});

test('a callout becomes a GitHub alert and tabs become headings', () => {
  const notes = [
    fence('ui-callout warning Back up', ['First line', '', 'Second line']),
    fence('ui-tabs', ['### One', 'Text one', '', fence('ui-kv', ['A | B']), '### Two', 'Text two'], '````'),
  ].join('\n\n');
  const lines = githubReleaseNotes(notes).split('\n');
  const alert = lines.indexOf('> [!WARNING]');
  assert.notEqual(alert, -1);
  assert.ok(lines.slice(alert, alert + 6).every((line) => line.startsWith('>')));
  assert.deepEqual(lines.filter((line) => line.startsWith('#### ')).length, 2);
  assert.ok(lines.some((line) => line.startsWith('- ') && line.includes('A') && line.includes('B')));
  assert.ok(!lines.some((line) => line.includes('```')));
});

test('ordinary fences and text pass through unchanged', () => {
  const notes = ['Intro | with a pipe', '', fence('bash', ['a | b', '### not a heading'])].join('\n');
  assert.equal(githubReleaseNotes(notes), `${notes}\n`);
});

test('components without a GitHub form and malformed rows are rejected', () => {
  assert.throws(() => githubReleaseNotes(fence('ui-steps', ['done | Step'])));
  assert.throws(() => githubReleaseNotes(fence('ui-checklist', ['maybe | Item'])));
  assert.throws(() => githubReleaseNotes(fence('ui-callout loud', ['Text'])));
  assert.throws(() => githubReleaseNotes(fence('ui-kv', ['Only one cell'])));
  assert.throws(() => githubReleaseNotes(fence('ui-tabs', ['### Only one', 'Text'], '````')));
  assert.throws(() => githubReleaseNotes('```ui-kv\nA | B\n'));
});

test('every shipped English release note converts without answer components', () => {
  const changelog = path.join(RESOURCES_DIR, 'changelog');
  const versions = readdirSync(changelog).filter((name) => name !== 'releases.json');
  assert.ok(versions.length > 0);
  for (const version of versions) {
    const notes = readFileSync(path.join(changelog, version, `${GITHUB_NOTES_LANGUAGE}.md`), 'utf8');
    assert.doesNotMatch(githubReleaseNotes(notes), /^\s*(`{3,}|~{3,})ui-/m, version);
  }
});
