import assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, writeFile, rename, rm, readdir } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { publishPreparedDirectory, recoverPublication } from './directory-publication.mjs';
import { acquirePublicationGuard, publishDistribution } from './publish-distribution.mjs';
import { shippedArtifactNames } from './shipped-rust-packages.mjs';

// withFixture provides isolated old/new generations to action and always removes its own temporary root.
// withFixture 向 action 提供隔离的新旧版本，并始终移除自身临时根目录。
async function withFixture(action) {
  // root owns every filesystem mutation performed by the test.
  // root 拥有测试执行的全部文件系统变更。
  const root = await mkdtemp(path.join(os.tmpdir(), 'vulcan-publication-'));
  // destination and stage model separate complete generations on one filesystem.
  // destination 与 stage 模拟同一文件系统上两个独立的完整版本。
  const destination = path.join(root, 'output');
  const stage = path.join(root, 'stage');
  await mkdir(destination); await mkdir(stage);
  await writeFile(path.join(destination, 'version'), 'old');
  await writeFile(path.join(stage, 'version'), 'new');
  try { await action({ root, destination, stage }); }
  finally { await rm(root, { recursive: true, force: true }); }
}

// validateFixture rejects incomplete generations and returns no value.
// validateFixture 拒绝不完整版本，无返回值。
async function validateFixture(root) {
  assert.match(await readFile(path.join(root, 'version'), 'utf8'), /^(old|new)$/);
}

// Successful promotion retains the previous generation until a later verified invocation.
// 提升成功后保留上一代，直到后续调用验证通过。
test('successful publication preserves old generation and recovery validates new generation', async () => withFixture(async ({ destination, stage }) => {
  await publishPreparedDirectory(stage, destination, validateFixture);
  assert.equal(await readFile(path.join(destination, 'version'), 'utf8'), 'new');
  assert.equal(await readFile(path.join(destination + '.previous', 'version'), 'utf8'), 'old');
  await recoverPublication(destination, validateFixture);
  await assert.rejects(readFile(path.join(destination + '.previous', 'version')), { code: 'ENOENT' });
}));

// Invalid staged contents never move the running directory.
// 暂存内容无效时绝不移动正式目录。
test('validation failure preserves the old output', async () => withFixture(async ({ destination, stage }) => {
  await writeFile(path.join(stage, 'version'), 'broken');
  await assert.rejects(publishPreparedDirectory(stage, destination, validateFixture));
  assert.equal(await readFile(path.join(destination, 'version'), 'utf8'), 'old');
}));

// A real preparation-copy failure cannot replace the published files or leave an active guard.
// 真实准备复制失败不能替换正式文件，也不能遗留活跃保护目录。
test('missing compiled artifact fails during staging without changing output', async () => withFixture(async ({ root, destination }) => {
  await assert.rejects(publishDistribution(root, destination, shippedArtifactNames().map(name => path.join(root, name)), 'debug'), { code: 'ENOENT' });
  assert.equal(await readFile(path.join(destination, 'version'), 'utf8'), 'old');
  assert.deepEqual(await readdir(path.join(root, 'target')), []);
  await assert.rejects(readFile(path.join(destination + '.publishing', 'owner.json')), { code: 'ENOENT' });
}));

// Inject disk-full and permission failures at promotion, after the old directory was safely preserved.
// 在旧目录安全保留之后，于提升阶段注入磁盘不足与权限故障。
for (const code of ['ENOSPC', 'EACCES']) {
  test(`promotion ${code} restores old output`, async () => withFixture(async ({ destination, stage }) => {
    await assert.rejects(publishPreparedDirectory(stage, destination, validateFixture, async (source, target) => {
      if (source === stage) throw Object.assign(new Error(code), { code });
      await rename(source, target);
    }), { code });
    assert.equal(await readFile(path.join(destination, 'version'), 'utf8'), 'old');
    assert.equal(await readFile(path.join(stage, 'version'), 'utf8'), 'new');
  }));
}

// Interrupted filesystem states are reconstructed directly, as a fresh process would observe them.
// 直接重建中断后的文件系统状态，与新进程观察到的状态一致。
test('restart recovers an interrupted directory switch', async () => withFixture(async ({ destination }) => {
  await rename(destination, destination + '.previous');
  await recoverPublication(destination, validateFixture);
  assert.equal(await readFile(path.join(destination, 'version'), 'utf8'), 'old');
}));

// A damaged new generation is preserved for diagnosis while the valid old generation is restored.
// 损坏的新一代被保留供诊断，同时恢复有效的上一代。
test('restart rolls back invalid promoted generation', async () => withFixture(async ({ root, destination, stage }) => {
  await rename(destination, destination + '.previous');
  await rename(stage, destination);
  await writeFile(path.join(destination, 'version'), 'broken');
  await recoverPublication(destination, validateFixture);
  assert.equal(await readFile(path.join(destination, 'version'), 'utf8'), 'old');
  assert.ok((await readdir(root)).some(name => name.startsWith('output.rejected-')));
}));

// A failed rollback must explicitly identify the recoverable old directory.
// 回滚失败必须明确指出可恢复的旧目录。
test('rollback failure retains recoverable backup', async () => withFixture(async ({ destination, stage }) => {
  await assert.rejects(publishPreparedDirectory(stage, destination, validateFixture, async (source, target) => {
    if (source !== destination) throw new Error('injected filesystem failure');
    await rename(source, target);
  }), /old generation remains/);
  assert.equal(await readFile(path.join(destination + '.previous', 'version'), 'utf8'), 'old');
  await recoverPublication(destination, validateFixture);
  assert.equal(await readFile(path.join(destination, 'version'), 'utf8'), 'old');
}));

// A live publisher cannot be displaced; an actual exited child process can be recovered.
// 活跃发布者不能被替换；实际已退出的子进程所有权可以恢复。
test('publication ownership rejects live owner and recovers exited owner', async () => withFixture(async ({ root }) => {
  // guard is isolated from application distribution paths.
  // guard 与应用发行目录隔离。
  const guard = path.join(root, 'output.publishing');
  await acquirePublicationGuard(guard);
  await assert.rejects(acquirePublicationGuard(guard), /already owned/);
  await rm(guard, { recursive: true });
  // child supplies a real exited process identity rather than guessing an unused PID.
  // child 提供真实已退出进程身份，而不是猜测未使用的进程号。
  const child = spawn(process.execPath, ['-e', 'process.exit(0)'], { stdio: 'ignore', windowsHide: true });
  const pid = child.pid;
  await once(child, 'exit');
  await mkdir(guard);
  await writeFile(path.join(guard, 'owner.json'), JSON.stringify({ pid }));
  await acquirePublicationGuard(guard);
  assert.equal(JSON.parse(await readFile(path.join(guard, 'owner.json'), 'utf8')).pid, process.pid);
}));
