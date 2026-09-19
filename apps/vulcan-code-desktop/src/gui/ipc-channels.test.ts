// Guard test that the inlined preload channels and the shared channel definition never drift.
// 守护测试：内联的预加载通道与共享通道定义绝不漂移。
//
// A sandboxed preload cannot require a relative local module, so the channel names are inlined in
// src/preload.cts. This test is the automated check that keeps the two definitions identical.
// 沙箱预加载无法 require 相对本地模块，因此通道名内联在 src/preload.cts 中。本测试是保持两份定义
// 一致的自动检查。

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';

import {
  SERVICE_CHANNELS,
  LOCAL_BROWSER_CHANNELS,
  NOTIFICATION_CHANNELS,
  NOTIFICATION_EVENT_CHANNELS,
  WINDOW_COMMAND_CHANNELS,
  WINDOW_EVENT_CHANNELS,
} from './ipc-channels.cjs';

// preloadSourcePath resolves the preload source relative to the compiled test location.
// preloadSourcePath 相对已编译测试位置解析预加载源码路径。
const preloadSourcePath = path.resolve(import.meta.dirname, '..', '..', 'src', 'preload.cts');

/** Returns every channel value declared by the shared definition. */
/** 返回共享定义声明的所有通道值。 */
function sharedChannelValues(): string[] {
  return [
    ...Object.values(SERVICE_CHANNELS),
    ...Object.values(LOCAL_BROWSER_CHANNELS),
    ...Object.values(WINDOW_COMMAND_CHANNELS),
    ...Object.values(WINDOW_EVENT_CHANNELS),
    ...Object.values(NOTIFICATION_CHANNELS),
    ...Object.values(NOTIFICATION_EVENT_CHANNELS),
  ];
}

test('the preload declares exactly the shared channel names', () => {
  const preloadSource = readFileSync(preloadSourcePath, 'utf8');
  const declaredInPreload = new Set(
    [...preloadSource.matchAll(/'((?:vulcan):[a-z-]+:[a-z-]+)'/g)].map((match) => match[1] ?? ''),
  );
  const shared = new Set(sharedChannelValues());

  for (const channel of shared) {
    assert.ok(
      declaredInPreload.has(channel),
      `preload.cts is missing channel ${channel}; the bridge and main process would disagree`,
    );
  }
  for (const channel of declaredInPreload) {
    assert.ok(
      shared.has(channel),
      `preload.cts declares unknown channel ${channel}; add it to gui/ipc-channels.cts`,
    );
  }
});

test('the preload does not require a relative local module', () => {
  // A relative require would throw inside the sandbox and silently remove the whole bridge.
  // 相对 require 会在沙箱内抛出异常，并静默移除整个桥。
  const preloadSource = readFileSync(preloadSourcePath, 'utf8');
  assert.equal(
    /require\(\s*['"]\./.test(preloadSource),
    false,
    'a sandboxed preload cannot require a relative local module',
  );
});
