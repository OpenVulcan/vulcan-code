// Runs the compiled preload the way a sandboxed renderer does and checks the channels its bridge uses.
// 以沙箱渲染进程的方式运行已编译的预加载脚本，并检查其桥实际使用的通道。
//
// A sandboxed preload cannot require a relative local module, so the channel names are inlined in
// src/preload.cts. Calling every bridge method against a recording `electron` stand-in keeps the
// inlined names and the shared definition identical.
// 沙箱预加载无法 require 相对本地模块，因此通道名内联在 src/preload.cts 中。对记录调用的 `electron`
// 替身调用桥的每个方法，使内联通道名与共享定义保持一致。

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import vm from 'node:vm';

import {
  SERVICE_CHANNELS,
  STARTUP_CHANNELS,
  CLIPBOARD_CHANNELS,
  LOCAL_BROWSER_CHANNELS,
  LOCAL_FILE_CHANNELS,
  NOTIFICATION_CHANNELS,
  NOTIFICATION_EVENT_CHANNELS,
  WINDOW_COMMAND_CHANNELS,
  WINDOW_EVENT_CHANNELS,
} from './ipc-channels.cjs';

// The compiled preload sits one level above this compiled test.
// 已编译的预加载脚本位于本已编译测试的上一级目录。
const preloadPath = path.resolve(import.meta.dirname, '..', 'preload.cjs');

/** Returns every channel value declared by the shared definition. */
/** 返回共享定义声明的所有通道值。 */
function sharedChannelValues(): string[] {
  return [
    ...Object.values(SERVICE_CHANNELS),
    ...Object.values(STARTUP_CHANNELS),
    ...Object.values(CLIPBOARD_CHANNELS),
    ...Object.values(LOCAL_BROWSER_CHANNELS),
    ...Object.values(LOCAL_FILE_CHANNELS),
    ...Object.values(WINDOW_COMMAND_CHANNELS),
    ...Object.values(WINDOW_EVENT_CHANNELS),
    ...Object.values(NOTIFICATION_CHANNELS),
    ...Object.values(NOTIFICATION_EVENT_CHANNELS),
  ];
}

/** Runs the preload with a recording `electron` and returns the channels every method used. */
/** 以记录调用的 `electron` 运行预加载脚本，返回所有方法使用的通道。 */
async function channelsUsedByBridge(): Promise<Set<string>> {
  const used = new Set<string>();
  const exposed = new Map<string, unknown>();
  const ipcRenderer = {
    invoke: async (channel: string) => {
      used.add(channel);
    },
    on: (channel: string) => {
      used.add(channel);
    },
    removeListener: () => undefined,
  };
  const electron = {
    contextBridge: {
      exposeInMainWorld: (name: string, api: unknown) => exposed.set(name, api),
    },
    ipcRenderer,
    // Supplies the local File metadata helper which has no IPC channel of its own.
    // 提供没有独立 IPC 通道的本地 File 元数据助手。
    webUtils: { getPathForFile: () => '' },
  };
  // The sandbox offers `electron` but no relative local modules; the stand-in refuses them too.
  // 沙箱提供 `electron` 但不提供相对本地模块；替身同样拒绝它们。
  const sandboxRequire = (specifier: string): unknown => {
    if (specifier === 'electron') return electron;
    throw new Error(`a sandboxed preload cannot require ${specifier}`);
  };
  const module = { exports: {} };
  vm.runInNewContext(readFileSync(preloadPath, 'utf8'), {
    require: sandboxRequire,
    module,
    exports: module.exports,
    process: { platform: process.platform, execPath: process.execPath },
  });

  const calls: Promise<unknown>[] = [];
  const visit = (value: unknown): void => {
    if (typeof value === 'function') {
      calls.push(Promise.resolve(value('id', 'value', () => undefined)));
    } else if (value && typeof value === 'object') {
      Object.values(value).forEach(visit);
    }
  };
  exposed.forEach(visit);
  await Promise.all(calls);
  return used;
}

test('the preload bridge uses exactly the shared channel names', async () => {
  const used = await channelsUsedByBridge();
  const shared = new Set(sharedChannelValues());
  for (const channel of shared) {
    assert.ok(used.has(channel), `no bridge method uses ${channel}; the bridge and main process would disagree`);
  }
  for (const channel of used) {
    assert.ok(shared.has(channel), `the bridge uses unknown channel ${channel}; add it to gui/ipc-channels.cts`);
  }
});
