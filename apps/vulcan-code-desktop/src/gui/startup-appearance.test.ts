// Behavior of the startup appearance validation: renderer reports and stored files are untrusted input.
// 启动外观校验的行为：渲染端上报与存储文件都是不受信输入。

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { parseStartupAppearance } from './startup-appearance.js';

const valid = { locale: 'zh-CN', theme: 'dark', text: 's', motion: 'system', ground: '#0c0e11' };

test('accepts a complete report and keeps only known fields', () => {
  assert.deepEqual(parseStartupAppearance({ ...valid, extra: 'ignored' }), valid);
});

test('rejects unusable fields instead of guessing', () => {
  for (const change of [
    { locale: 'xx-XX' },
    { theme: 'Dark Theme' },
    { text: '' },
    { motion: 42 },
    { ground: 'oklch(0.2 0 0)' },
    { ground: '#0c0e11cc' },
  ]) {
    assert.equal(parseStartupAppearance({ ...valid, ...change }), null, JSON.stringify(change));
  }
  assert.equal(parseStartupAppearance(null), null);
  assert.equal(parseStartupAppearance('dark'), null);
});
