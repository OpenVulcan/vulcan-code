import assert from 'node:assert/strict';
import { test } from 'node:test';
import { isAllowedLocalBrowserUrl, normalizeLocalBrowserUrl, validateExternalBrowserUrl } from './local-browser-url.js';

// The real manager supplies its privileged service origins to both entry and navigation checks.
// 真实管理器向地址入口与导航校验提供特权服务来源。
const blockedOrigins = new Set(['http://127.0.0.1:17921']);

test('external conversation links accept explicit websites and files but never arbitrary OS protocols', () => {
  for (const url of ['https://example.com/page', 'http://localhost:5173/', 'file:///C:/my%20files/page.html#demo']) {
    assert.equal(validateExternalBrowserUrl(url, blockedOrigins), url);
  }
  for (const url of ['http://127.0.0.1:17921/private', 'about:blank', 'javascript:alert(1)',
    'data:text/html,hello', 'vulcan://private', 'ms-settings:display', 'C:/page.html', 'example.com']) {
    assert.throws(() => validateExternalBrowserUrl(url, blockedOrigins), /Only website|Invalid URL/, url);
  }
});

test('converts absolute file paths before considering web hosts', () => {
  // These forms cover the reported Windows path, copied paths, UNC and POSIX absolute paths.
  // 这些形式覆盖反馈中的 Windows 路径、复制路径、UNC 与 POSIX 绝对路径。
  const cases = [
    [String.raw`C:\Users\lingeasy\.vulcan-debug\projects\vulcan_project\pelican-bicycle.html`,
      'file:///C:/Users/lingeasy/.vulcan-debug/projects/vulcan_project/pelican-bicycle.html'],
    ['C:/Users/test/page.html', 'file:///C:/Users/test/page.html'],
    ['/C:/Users/test/page.html', 'file:///C:/Users/test/page.html'],
    [' "C:\\目录\\a #100%.html" ', 'file:///C:/%E7%9B%AE%E5%BD%95/a%20%23100%25.html'],
    [String.raw`\\server\share\a #1.html`, 'file://server/share/a%20%231.html'],
    ['/tmp/a #100%.html', 'file:///tmp/a%20%23100%25.html'],
    ['file:///C:/my%20files/a%231.html#section', 'file:///C:/my%20files/a%231.html#section'],
    ['file:///tmp/page.html', 'file:///tmp/page.html'],
  ];
  for (const [input, expected] of cases) {
    assert.equal(normalizeLocalBrowserUrl(input!, blockedOrigins), expected, input);
  }
});

test('preserves websites, loopback ports and explicit schemes', () => {
  assert.equal(normalizeLocalBrowserUrl('localhost:5173/app', blockedOrigins), 'http://localhost:5173/app');
  assert.equal(normalizeLocalBrowserUrl('127.0.0.1:5173', blockedOrigins), 'http://127.0.0.1:5173/');
  assert.equal(normalizeLocalBrowserUrl('[::1]:5173/', blockedOrigins), 'http://[::1]:5173/');
  assert.equal(normalizeLocalBrowserUrl('example.com:8443/a', blockedOrigins), 'https://example.com:8443/a');
  assert.equal(normalizeLocalBrowserUrl('example.com', blockedOrigins), 'https://example.com/');
  assert.equal(normalizeLocalBrowserUrl('https://c/Users/page.html', blockedOrigins), 'https://c/Users/page.html');
  assert.equal(normalizeLocalBrowserUrl('about:blank', blockedOrigins), 'about:blank');
});

test('rejects protected origins and executable schemes at entry and navigation', () => {
  for (const input of ['http://127.0.0.1:17921/private', 'javascript:alert(1)', 'data:text/html,hello', 'vulcan://private']) {
    assert.throws(() => normalizeLocalBrowserUrl(input, blockedOrigins), /unavailable/);
    assert.equal(isAllowedLocalBrowserUrl(input, blockedOrigins), false);
  }
  assert.throws(() => normalizeLocalBrowserUrl('', blockedOrigins), /Enter/);
  assert.equal(isAllowedLocalBrowserUrl('not a URL', blockedOrigins), false);
  assert.equal(isAllowedLocalBrowserUrl('file:///C:/page.html', blockedOrigins), true);
});
