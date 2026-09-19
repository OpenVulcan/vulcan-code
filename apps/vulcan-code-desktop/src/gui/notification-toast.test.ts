import assert from 'node:assert/strict';
import test from 'node:test';
import { createWindowsToastXml } from './notification-toast.js';

test('Windows toast body click carries the exact notification id and escapes visible text', () => {
  const xml = createWindowsToastXml('notice-123', '中文<&', 'body > & <');
  assert.equal(xml, '<toast launch="type=click&amp;tag=notice-123"><visual>'
    + '<binding template="ToastGeneric"><text>中文&lt;&amp;</text>'
    + '<text>body &gt; &amp; &lt;</text></binding></visual></toast>');
});

test('Windows toast text keeps valid Unicode while dropping XML-invalid code points', () => {
  // xml is the exact native payload assembled from mixed valid and invalid Unicode.
  // xml 是由有效与无效 Unicode 混合输入生成的精确原生载荷。
  const xml = createWindowsToastXml('notice-456', '标题\u0000😀', '正文\u000b\ud800\ufffe结束');
  assert.ok(xml.includes('<text>标题😀</text><text>正文结束</text>'));
  assert.ok(!xml.includes('\u0000') && !xml.includes('\u000b') && !xml.includes('\ud800'));
});
