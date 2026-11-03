// Local file actions refuse runnable files and relative paths before the OS sees them.
// 本地文件操作在交给操作系统之前拒绝可运行文件与相对路径。

import assert from 'node:assert/strict';
import { mkdtemp, rm, writeFile, mkdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { test } from 'node:test';

import {
  inspectLocalFile,
  isRunnableFilePath,
  openLocalFileWithDefaultApp,
  showLocalFileInFolder,
} from './local-file-actions.js';

test('runnable files are recognized by extension, ignoring Windows name tricks', () => {
  assert.equal(isRunnableFilePath('C:\\tools\\setup.EXE', 0, 'win32'), true);
  assert.equal(isRunnableFilePath('C:\\tools\\run.bat. . ', 0, 'win32'), true);
  assert.equal(isRunnableFilePath('C:\\tools\\run.ps1:hidden', 0, 'win32'), true);
  assert.equal(isRunnableFilePath('C:\\docs\\notes.zip', 0, 'win32'), false);
  assert.equal(isRunnableFilePath('/home/u/install.sh', 0o644, 'linux'), true);
  assert.equal(isRunnableFilePath('/home/u/archive.tar.gz', 0o644, 'linux'), false);
});

test('an executable permission bit marks a runnable file outside Windows', () => {
  assert.equal(isRunnableFilePath('/usr/local/bin/tool', 0o755, 'darwin'), true);
  assert.equal(isRunnableFilePath('C:\\tools\\tool', 0o755, 'win32'), false);
});

test('only existing, ordinary files open with the default program', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'vulcan-local-file-'));
  try {
    const document = path.join(root, 'design.psd');
    const script = path.join(root, 'build.cmd');
    await writeFile(document, 'image');
    await writeFile(script, 'echo');
    await mkdir(path.join(root, 'folder'));
    const opened: string[] = [];
    const openPath = async (filePath: string) => { opened.push(filePath); return ''; };

    assert.deepEqual(await inspectLocalFile(document), { exists: true, opensWithDefaultApp: true });
    assert.deepEqual(await inspectLocalFile(script), { exists: true, opensWithDefaultApp: false });
    assert.deepEqual(await inspectLocalFile(path.join(root, 'folder')), { exists: true, opensWithDefaultApp: false });
    assert.deepEqual(await inspectLocalFile(path.join(root, 'missing.psd')), { exists: false, opensWithDefaultApp: false });

    await openLocalFileWithDefaultApp(document, openPath);
    await assert.rejects(openLocalFileWithDefaultApp(script, openPath));
    await assert.rejects(openLocalFileWithDefaultApp(path.join(root, 'missing.psd'), openPath));
    await assert.rejects(openLocalFileWithDefaultApp(document, async () => 'no handler'));
    assert.deepEqual(opened, [document]);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('show in folder accepts runnable files but not relative or missing paths', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'vulcan-local-file-'));
  try {
    const script = path.join(root, 'build.cmd');
    await writeFile(script, 'echo');
    const shown: string[] = [];
    await showLocalFileInFolder(script, (filePath) => shown.push(filePath));
    await assert.rejects(showLocalFileInFolder('build.cmd', () => undefined), TypeError);
    await assert.rejects(showLocalFileInFolder(path.join(root, 'missing.txt'), () => undefined));
    await assert.rejects(inspectLocalFile(42), TypeError);
    assert.deepEqual(shown, [script]);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
