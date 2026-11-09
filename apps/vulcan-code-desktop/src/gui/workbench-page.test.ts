import assert from 'node:assert/strict';
import { test } from 'node:test';
import { loadWorkbenchPage } from './workbench-page.js';

test('successful workbench loading removes the startup page only after navigation completes', async () => {
  const calls: string[] = [];
  let finishNavigation!: () => void;
  const navigation = new Promise<void>((resolve) => { finishNavigation = resolve; });
  const window = {
    loadURL: async (url: string) => { calls.push(url); await navigation; },
    webContents: { navigationHistory: { clear: () => { calls.push('clear'); } } },
  };
  const loading = loadWorkbenchPage(window, 'http://127.0.0.1:17921/');
  assert.deepEqual(calls, ['http://127.0.0.1:17921/']);
  finishNavigation();
  await loading;
  assert.deepEqual(calls, ['http://127.0.0.1:17921/', 'clear']);
});

test('failed workbench loading preserves history and the exact navigation failure', async () => {
  const failure = new Error('navigation failed');
  let cleared = false;
  const window = {
    loadURL: async () => { throw failure; },
    webContents: { navigationHistory: { clear: () => { cleared = true; } } },
  };
  await assert.rejects(loadWorkbenchPage(window, 'http://127.0.0.1:17921/'), (error) => error === failure);
  assert.equal(cleared, false);
});
