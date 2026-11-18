// Coordinate the opt-in VMM HTTP acceptance test through a private per-run directory.
// 通过每次独立的私有目录协调显式启动的 VMM HTTP 验收测试。
import { spawn } from 'node:child_process';
import { access, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';

// Resolve test paths from this checkout regardless of the caller's current directory.
// 从当前检出解析测试路径，无需调用者位于仓库根目录。
const root = fileURLToPath(new URL('..', import.meta.url));
const web = path.join(root, 'apps', 'vulcan-code-web');
const vitest = path.join(web, 'node_modules', 'vitest', 'vitest.mjs');
const handshakes = path.join(root, 'target', 'vmm-http-acceptance');

// Starts the named command with the supplied working directory and environment. Its result always settles.
// 使用指定目录和环境启动命令，返回始终会结束的进程结果。
function start(command, args, options) {
  const child = spawn(command, args, { stdio: 'inherit', windowsHide: true, ...options });
  // The settled flag lets the coordinator distinguish a running peer from a completed command.
  // 使用结束标记区分仍在运行的配套进程与已经结束的命令。
  let exited = false;
  const result = new Promise(resolve => {
    child.once('error', error => resolve(error));
    child.once('close', (code, signal) => resolve(code === 0 ? null : new Error(signal ?? `exit ${code}`)));
  }).then(error => {
    exited = true;
    return error;
  });
  return { child, result, get exited() { return exited; } };
}

// Waits for one command and identifies the failed stage without reading its private output.
// 等待指定命令，并在失败时标识阶段，无需读取私有输出。
async function requireSuccess(job, stage) {
  const error = await job.result;
  if (error) throw new Error(`${stage} failed: ${error.message}`);
}

// Waits until the Rust fixture has published a complete discovery document, or reports its early exit.
// 等待 Rust 夹具写完发现文档，或报告该进程提前退出。
async function waitForDiscovery(discovery, backend) {
  while (!backend.exited) {
    try {
      JSON.parse(await readFile(discovery, 'utf8'));
      return;
    } catch (error) {
      // Writing the discovery file is synchronous on the Rust side, but the reader can see an intermediate file.
      // Rust 侧同步写入发现文件，但读取方仍可能看到写入中的文件。
      if (error.code !== 'ENOENT' && !(error instanceof SyntaxError)) throw error;
      await delay(100);
    }
  }
  await requireSuccess(backend, 'Rust VMM acceptance');
  throw new Error('Rust VMM acceptance exited before publishing service discovery.');
}

// Runs both sides of the private test and removes its temporary discovery credential on completion.
// 运行私有测试的两端，并在结束后移除临时发现凭据。
async function run() {
  await access(vitest);
  // Test binaries load the separately built LanceDB library from the Cargo profile directory.
  // 测试二进制从 Cargo 构建模式目录加载单独构建的 LanceDB 库。
  await requireSuccess(start('cargo', ['build', '--locked', '-p', 'vulcan-lancedb-native'], { cwd: root }), 'LanceDB build');

  // A fresh directory prevents readiness or completion files from an earlier run being reused.
  // 每次创建新目录，防止误用旧运行的就绪或完成标记。
  await mkdir(handshakes, { recursive: true });
  const directory = await mkdtemp(path.join(handshakes, 'run-'));
  const discovery = path.join(directory, 'discovery.json');
  const env = { ...process.env, VULCAN_VMM_HTTP_ACCEPTANCE_DIR: directory };
  console.log(`VMM HTTP acceptance handshake: ${directory}`);
  const backend = start('cargo', [
    'nextest', 'run', '--locked', '--lib', '-p', 'vulcan-service-kernel',
    '--run-ignored', 'only', '--test-threads=1', '--no-tests=fail',
    '-E', 'test(/::real_controller_policy_components_over_http$/)',
  ], { cwd: root, env });

  // Start Vitest only after the isolated Rust HTTP service is ready.
  // 仅在隔离的 Rust HTTP 服务就绪后启动 Vitest。
  let frontend;
  try {
    await waitForDiscovery(discovery, backend);
    frontend = start(process.execPath, [vitest, 'run', 'src/components/features/memory/vmm-native-policy-http.test.tsx'], { cwd: web, env });
    await Promise.all([
      requireSuccess(backend, 'Rust VMM acceptance'),
      requireSuccess(frontend, 'WebUI VMM acceptance'),
    ]);
    await access(path.join(directory, 'finish.ack'));
  } finally {
    // Signal a failed peer through the fixture's abort protocol before releasing its discovery file.
    // 通过夹具的中止协议通知失败的配套进程，再移除发现文件。
    try {
      if (frontend && !frontend.exited) {
        frontend.child.kill();
        await frontend.result;
      }
      if (!backend.exited) {
        await writeFile(path.join(directory, 'abort.ready'), 'acceptance runner aborted');
        await backend.result;
      }
    } finally {
      await rm(discovery, { force: true });
    }
  }
}

try {
  await run();
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
