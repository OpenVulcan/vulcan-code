// Verify the complete Linux replacement/rollback cycle with real disposable ELF owners.
// 使用真实且可丢弃的 ELF 持有者验证 Linux 完整替换及回退流程。
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { access, mkdir, mkdtemp, readFile, readdir, readlink, realpath, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import test from 'node:test';
import { restartLinuxCore } from './core-cli.mjs';
import { runtimeLayoutSchemaVersion } from './desktop-runtime.mjs';

// Poll an owned fixture file; filename/predicate select the condition and no user service is queried.
// 轮询自有夹具文件；filename/predicate 指定条件，不查询任何用户服务。
// Return the parsed matching document or fail after the fixture deadline.
// 返回匹配条件的已解析文档，超过夹具截止时间则失败。
async function waitForDocument(filename, predicate) {
  // This timeout belongs to the test harness, not to production startup policy.
  // 此超时属于测试宿主，不属于生产启动策略。
  const deadline = Date.now() + 5_000;
  while (Date.now() < deadline) {
    try {
      const document = JSON.parse(await readFile(filename, 'utf8'));
      if (predicate(document)) return document;
    } catch (error) { if (error.code !== 'ENOENT') throw error; }
    await delay(20);
  }
  throw new Error('Fixture did not publish the expected document.');
}

// Build a complete distribution with native fixture owners and a real loopback health responder.
// 创建完整发行夹具，包含原生持有者和真实回环健康应答器。
// t owns cleanup; generation selects a healthy candidate (1) or a startup failure (2).
// t 管理清理；generation 选择健康候选（1）或启动失败（2）。
async function createRestartFixture(t, generation) {
  // All source, executables, discovery documents, and subprocesses stay inside this exact fixture.
  // 全部源码、可执行文件、发现文档及子进程都属于此精确夹具。
  const root = await mkdtemp(path.join(os.tmpdir(), 'vulcan-native-core-cycle-'));
  assert.equal(path.dirname(root), path.resolve(os.tmpdir()));
  const output = path.join(root, 'output');
  const core = path.join(output, 'bin/core');
  const candidate = path.join(root, 'candidate');
  const discovery = path.join(root, 'user/code/webui-service.json');
  await mkdir(path.dirname(core), { recursive: true });
  await mkdir(path.dirname(discovery), { recursive: true });
  // Cleanup selects only fixture-native executables by their procfs identity before signalling.
  // 清理发送信号前，通过 procfs 身份仅选中夹具原生可执行程序。
  t.after(async () => {
    try {
      const document = JSON.parse(await readFile(discovery, 'utf8'));
      if (await readlink(`/proc/${document.pid}/exe`) === core) {
        process.kill(document.pid, 'SIGTERM');
        for (let count = 0; count < 100; count++) {
          try { await readlink(`/proc/${document.pid}/exe`); }
          catch (error) { if (error.code === 'ENOENT' || error.code === 'ESRCH') break; throw error; }
          await delay(20);
        }
      }
    } catch (error) { if (error.code !== 'ENOENT' && error.code !== 'ESRCH') throw error; }
    await rm(root, { recursive: true, force: true });
  });
  // The manifest uses the actual host identity and every package input is present.
  // 清单使用真实宿主身份，并包含全部发行输入。
  const layout = { schemaVersion: runtimeLayoutSchemaVersion, profile: 'debug', platform: process.platform, architecture: process.arch,
    serviceExecutable: 'bin/core', tuiExecutable: 'bin/tui', desktopExecutable: 'desktop/electron', desktopEntry: 'desktop/gui/main.js',
    browserHostEntry: 'browser/main.js', browserHostExecutable: 'browser/node', chromiumExecutable: 'browser/chrome', webuiRoot: 'webui' };
  for (const relative of [layout.tuiExecutable, layout.desktopExecutable, layout.desktopEntry, 'desktop/preload.cjs',
    layout.browserHostEntry, layout.browserHostExecutable, layout.chromiumExecutable]) {
    await mkdir(path.dirname(path.join(output, relative)), { recursive: true });
    await writeFile(path.join(output, relative), 'fixture');
  }
  await mkdir(path.join(output, layout.webuiRoot));
  await writeFile(path.join(output, 'runtime-layout.json'), JSON.stringify(layout));
  await mkdir(path.join(root, 'scripts'));
  await writeFile(path.join(root, 'scripts/desktop-runtime.mjs'),
    `console.log(process.argv[2] === 'root' ? ${JSON.stringify(output)} : ${JSON.stringify(JSON.stringify(layout))});\n`);
  // Description data is emitted by an actual invocation of each selected native fixture artifact.
  // 描述数据由各选定原生夹具制品的实际调用输出。
  const descriptionScript = path.join(root, 'describe.cjs');
  await writeFile(descriptionScript, `console.log(JSON.stringify({ protocolVersion: 1, profile: 'debug', modes: ['--headless'],
    executablePath: require('node:fs').realpathSync(process.argv[2]), dataRoot: ${JSON.stringify(path.dirname(discovery))},
    discoveryPath: ${JSON.stringify(discovery)} }));\n`);
  // Health identifies the native parent, while the child responder has no business APIs or database.
  // 健康身份指向原生父进程；子应答器不含业务 API 或数据库。
  const serverScript = path.join(root, 'server.cjs');
  await writeFile(serverScript, `const fs = require('node:fs'); const http = require('node:http');
    let document; const server = http.createServer((req, res) => { res.setHeader('Content-Type', 'application/json'); res.end(JSON.stringify(document)); });
    server.listen(0, '127.0.0.1', () => { const port = server.address().port;
      document = { host: '127.0.0.1', port, baseUrl: 'http://127.0.0.1:' + port, pid: Number(process.argv[2]),
        status: 'ready', startedAt: String(Date.now()), serviceToken: 'fixture-only-token', generation: Number(process.argv[3]) };
      fs.writeFileSync(${JSON.stringify(discovery)}, JSON.stringify(document)); });
    process.on('SIGTERM', () => server.close(() => process.exit(0)));\n`);
  await writeFile(path.join(output, 'desktop/gui/package.json'), '{"type":"module"}');
  await writeFile(path.join(output, 'desktop/gui/service-launcher.js'), `import { execFileSync } from 'node:child_process'; import { readFile } from 'node:fs/promises';
    export async function readServiceLaunchDescription(executable) { return JSON.parse(execFileSync(executable, ['--describe'], { encoding: 'utf8' })); }
    export async function readLiveServiceInstance(filename) { try { const document = JSON.parse(await readFile(filename, 'utf8'));
      const response = await fetch(document.baseUrl + '/health', { signal: AbortSignal.timeout(500) }); const health = await response.json();
      return ['host', 'port', 'baseUrl', 'pid', 'startedAt', 'serviceToken'].every(key => document[key] === health[key]) ? health : null;
    } catch { return null; } }\n`);
  // The small native parent keeps /proc/<pid>/exe tied to the fixture artifact, unlike a shell wrapper.
  // 小型原生父进程使 /proc/<pid>/exe 绑定夹具制品，而非 shell 包装器。
  const nativeSource = path.join(root, 'owner.c');
  await writeFile(nativeSource, `#include <signal.h>
#include <stdio.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>
// Keep signal handling async-safe; main performs child shutdown.
// 保持信号处理异步安全；由 main 关闭子进程。
static volatile sig_atomic_t stopped = 0;
// Mark termination; sig identifies the delivered signal and no value is returned.
// 标记终止；sig 表示收到的信号，不返回值。
static void stop_owner(int sig) { (void)sig; stopped = 1; }
// Implement describe/headless fixture modes; argc/argv are process arguments and exit status reports success.
// 实现描述及无头夹具模式；argc/argv 为进程参数，退出码报告成功与否。
int main(int argc, char **argv) {
  if (argc > 1 && strcmp(argv[1], "--describe") == 0) {
    execl(${JSON.stringify(process.execPath)}, "node", ${JSON.stringify(descriptionScript)}, argv[0], (char *)0); return 90;
  }
  if (GENERATION == 2) return 23;
  signal(SIGTERM, stop_owner);
  // The responder is owned solely by this parent.
  // 应答器仅属于此父进程。
  pid_t child = fork();
  if (child < 0) return 91;
  if (child == 0) {
    // Pass the parent's identity explicitly to the isolated responder.
    // 向隔离应答器显式传递父进程身份。
    char owner[32], generation[16];
    snprintf(owner, sizeof(owner), "%ld", (long)getppid()); snprintf(generation, sizeof(generation), "%d", GENERATION);
    execl(${JSON.stringify(process.execPath)}, "node", ${JSON.stringify(serverScript)}, owner, generation, (char *)0); return 92;
  }
  while (!stopped) usleep(10000);
  kill(child, SIGTERM); waitpid(child, 0, 0); return 0;
}\n`);
  execFileSync('cc', ['-DGENERATION=0', nativeSource, '-o', core]);
  execFileSync('cc', [`-DGENERATION=${generation}`, nativeSource, '-o', candidate]);
  // Cargo's executable protocol points only to the candidate compiled above.
  // Cargo 可执行制品协议仅指向上面已编译的候选。
  await writeFile(path.join(root, 'cargo'), '#!/usr/bin/env node\n'
    + `console.log(${JSON.stringify(JSON.stringify({ reason: 'compiler-artifact', target: { name: 'vulcan-code-service', kind: ['bin'] }, executable: candidate }))});\n`
    + `console.log(${JSON.stringify(JSON.stringify({ reason: 'build-finished', success: true }))});\n`, { mode: 0o755 });
  const owner = spawn(core, ['--headless'], { stdio: 'ignore' });
  await new Promise((resolve, reject) => { owner.once('spawn', resolve); owner.once('error', reject); });
  await waitForDocument(discovery, document => document.pid === owner.pid && document.generation === 0);
  return { root, output, core, candidate, discovery, owner };
}

for (const generation of [1, 2]) {
  test(`native restart ${generation === 1 ? 'publishes the candidate' : 'restores and restarts the previous Core on startup failure'}`,
    { skip: process.platform !== 'linux' }, async t => {
      const fixture = await createRestartFixture(t, generation);
      const before = createHash('sha256').update(await readFile(fixture.core)).digest('hex');
      const previousPath = process.env.PATH;
      process.env.PATH = `${fixture.root}${path.delimiter}${previousPath}`;
      try {
        if (generation === 1) await restartLinuxCore(fixture.root);
        else await assert.rejects(restartLinuxCore(fixture.root), /previous Core was restored and restarted/);
      } finally { process.env.PATH = previousPath; }
      const document = await waitForDocument(fixture.discovery, document => document.pid !== fixture.owner.pid && document.generation === (generation === 1 ? 1 : 0));
      assert.equal(await readlink(`/proc/${document.pid}/exe`), await realpath(fixture.core));
      const after = createHash('sha256').update(await readFile(fixture.core)).digest('hex');
      assert.equal(before === after, generation === 2);
      assert.deepEqual((await readdir(path.dirname(fixture.core))).filter(name => name.endsWith('.previous') || name.endsWith('.next')), []);
      await assert.rejects(access(`${fixture.output}.publishing`), { code: 'ENOENT' });
      await assert.rejects(access(`/proc/${fixture.owner.pid}/exe`), { code: 'ENOENT' });
    });
}
