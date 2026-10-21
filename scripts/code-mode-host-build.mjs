// Build the standalone code-mode host workspace and print the absolute path of the host executable.
// 构建独立的代码模式宿主工作区，并输出宿主可执行文件的绝对路径。
//
// Usage: node scripts/code-mode-host-build.mjs <debug|release>
// 用法：node scripts/code-mode-host-build.mjs <debug|release>
//
// The host links V8 with the static C runtime and therefore lives in its own Cargo workspace
// (native/code-mode-host) with its own .cargo/config.toml. Cargo reads that configuration from the
// current directory, so the build always runs inside the workspace. Cargo progress goes to stderr;
// stdout carries only the executable path for the calling build script.
// 宿主以静态 C 运行时链接 V8，因此位于自带 .cargo/config.toml 的独立 Cargo 工作区
// （native/code-mode-host）。Cargo 从当前目录读取该配置，所以构建始终在工作区内运行。Cargo 进度写入
// stderr；stdout 只输出可执行文件路径，供调用方构建脚本使用。
import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { prepareV8BuildRoot } from './prepare-v8-build-root.mjs';

// HOST_BINARY is both the Cargo package and binary name of the code-mode host.
// HOST_BINARY 同时是代码模式宿主的 Cargo 包名与二进制名。
const HOST_BINARY = 'vulcan-code-mode-host';

// hostWorkspace is the standalone code-mode host workspace directory.
// hostWorkspace 是独立的代码模式宿主工作区目录。
const hostWorkspace = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', 'native', 'code-mode-host');

// Run cargo inside the host workspace, streaming its output to stderr; throws on failure.
// 在宿主工作区内运行 cargo，并把输出转发到 stderr；失败时抛出异常。
function cargo(args, { capture = false } = {}) {
  const result = spawnSync('cargo', args, {
    cwd: hostWorkspace,
    encoding: 'utf8',
    maxBuffer: 256 * 1024 * 1024,
    stdio: capture ? ['ignore', 'pipe', 'inherit'] : ['ignore', process.stderr, 'inherit'],
    windowsHide: true,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`cargo ${args.join(' ')} failed in ${hostWorkspace} (exit ${result.status}).`);
  return result.stdout;
}

// Build the host for `profile` and return the absolute executable path.
// 以 `profile` 构建宿主并返回可执行文件的绝对路径。
export function buildCodeModeHost(profile) {
  if (profile !== 'debug' && profile !== 'release') throw new Error(`Unknown profile "${profile}"; expected debug or release.`);
  for (const { profile: prepared, action } of prepareV8BuildRoot()) {
    if (action === 'created') console.error(`==> Created v8 gn_root junction for the ${prepared} profile.`);
  }
  cargo(['build', '--locked', '-p', HOST_BINARY, ...(profile === 'release' ? ['--release'] : [])]);
  const metadata = JSON.parse(cargo(['metadata', '--format-version', '1', '--no-deps', '--locked'], { capture: true }));
  const suffix = process.platform === 'win32' ? '.exe' : '';
  const executable = path.join(metadata.target_directory, profile, `${HOST_BINARY}${suffix}`);
  if (!existsSync(executable)) throw new Error(`Cargo finished but ${executable} does not exist.`);
  return executable;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    console.log(buildCodeModeHost(process.argv[2]));
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
