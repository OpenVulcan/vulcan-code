// Real Electron history regression using a private profile and a synthetic loopback workbench.
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { once } from 'node:events';
import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { promisify } from 'node:util';
import electronExecutable from 'electron';

// This function runs in the isolated Electron main process, not in the installed application.
async function inspectHistory({ BrowserWindow }, parameters) {
  const { loadWorkbenchPage } = await import(parameters.moduleUrl);
  const { once } = await import('node:events');
  const window = new BrowserWindow({ show: false, webPreferences: { sandbox: true } });
  const contents = window.webContents;
  const origin = new URL(parameters.webUiUrl).origin;
  // Retain the existing workbench page-navigation restriction while replaying history navigation.
  const restrict = (event, url) => { if (new URL(url).origin !== origin) event.preventDefault(); };
  contents.on('will-navigate', restrict);
  contents.on('will-redirect', restrict);
  await window.loadFile(parameters.startupFile);
  await loadWorkbenchPage(window, parameters.webUiUrl);
  const entries = contents.navigationHistory.getAllEntries().map((entry) => entry.url);
  const couldGoBack = contents.navigationHistory.canGoBack();
  if (couldGoBack) {
    const navigated = once(contents, 'did-finish-load');
    contents.navigationHistory.goBack();
    await navigated;
  }
  const afterBack = contents.getURL();
  await loadWorkbenchPage(window, parameters.webUiUrl);
  const reloadedEntries = contents.navigationHistory.getAllEntries().map((entry) => entry.url);
  // A different browser contents must retain ordinary backward navigation.
  const browser = new BrowserWindow({ show: false, webPreferences: { sandbox: true } });
  try {
    await browser.loadURL(`${parameters.webUiUrl}first`);
    await browser.loadURL(`${parameters.webUiUrl}second`);
    const browserCanGoBack = browser.webContents.navigationHistory.canGoBack();
    const navigated = once(browser.webContents, 'did-finish-load');
    browser.webContents.navigationHistory.goBack();
    await navigated;
    return {
      entries, couldGoBack, afterBack, reloadedEntries, browserCanGoBack,
      browserAfterBack: browser.webContents.getURL(),
    };
  } finally {
    browser.destroy();
  }
}

const desktop = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const directory = await mkdtemp(path.join(tmpdir(), 'vulcan-workbench-history-'));
const server = createServer((_request, response) => {
  response.writeHead(200, { 'Content-Type': 'text/html' });
  response.end('<!doctype html><title>Isolated workbench</title><body>Ready</body>');
});
try {
  const profile = path.join(directory, 'profile');
  await mkdir(profile);
  const startupFile = path.join(directory, 'desktop-startup.html');
  await writeFile(startupFile, '<!doctype html><title>Startup</title><body>Loading</body>');
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const address = server.address();
  assert.ok(address && typeof address !== 'string');
  const webUiUrl = `http://127.0.0.1:${address.port}/`;
  const parameters = {
    startupFile, webUiUrl,
    moduleUrl: pathToFileURL(path.join(desktop, 'dist/gui/workbench-page.js')).href,
  };
  const entry = path.join(directory, 'main.cjs');
  await writeFile(entry, `
    const electron = require('electron');
    const { app } = electron;
    app.setPath('userData', process.env.VULCAN_HISTORY_TEST_PROFILE);
    app.disableHardwareAcceleration();
    app.whenReady().then(() => (${inspectHistory.toString()})(electron, ${JSON.stringify(parameters)}))
      .then((result) => process.stdout.write(JSON.stringify(result) + '\\n', () => app.exit(0)))
      .catch((error) => process.stderr.write(String(error.stack || error) + '\\n', () => app.exit(1)));
  `);
  const { stdout } = await promisify(execFile)(electronExecutable, [entry], {
    env: { ...process.env, VULCAN_HISTORY_TEST_PROFILE: profile },
    timeout: 30_000,
    windowsHide: true,
  });
  const result = JSON.parse(stdout.trim());
  console.log(JSON.stringify(result, null, 2));
  assert.equal(result.couldGoBack, false, 'The startup page must not remain reachable by history back.');
  assert.deepEqual(result.entries, [webUiUrl]);
  assert.equal(result.afterBack, webUiUrl);
  assert.deepEqual(result.reloadedEntries, [webUiUrl]);
  assert.equal(result.browserCanGoBack, true);
  assert.equal(result.browserAfterBack, `${webUiUrl}first`);
  console.log('Workbench history and independent browser navigation passed.');
} finally {
  server.closeAllConnections();
  await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
  await rm(directory, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
}
