// Verify real fatal exit evidence, including privacy and retained nonzero exit semantics.
// 验证真实致命退出证据，包括隐私和保留非零退出语义。
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { spawnSync } from 'node:child_process';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';

test('fatal monitor retains a redacted report without suppressing Node failure', async () => {
  // root belongs exclusively to this child and is removed after its confirmed exit.
  // root 仅属于本测试子进程，在确认退出后删除。
  const root = await mkdtemp(path.join(tmpdir(), 'vulcan-host-crash-'));
  try {
    // The script imports the compiled monitor and triggers Node's real unhandled-rejection exit.
    // 脚本导入已编译监视器，并触发 Node 真实的未处理拒绝退出。
    const script = `import { monitorHostCrash } from ${JSON.stringify(new URL('./crash-report.js', import.meta.url).href)};
monitorHostCrash(${JSON.stringify(root)}, 'browser-test', '2');
Promise.reject(new Error('relay ws://127.0.0.1:1234/devtools/browser/secret-token failed'));`;
    const child = spawnSync(process.execPath, ['--input-type=module', '-e', script], { windowsHide: true, encoding: 'utf8' });
    assert.equal(child.status, 1);
    const report = JSON.parse(await readFile(path.join(root, 'browser-host-crash.json'), 'utf8'));
    assert.equal(report.sessionId, 'browser-test');
    assert.equal(report.hostGeneration, '2');
    assert.equal(report.origin, 'unhandledRejection');
    assert.match(report.stack, /\[redacted\]/);
    assert.doesNotMatch(report.stack, /secret-token/);
  } finally { await rm(root, { recursive: true, force: true }); }
});
