// Derive reusable compiler cache identity without coupling it to the commit or unrelated installed toolchains.
// 计算可复用的编译缓存身份，不与提交或未使用的已安装工具链绑定。
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { appendFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// compilerIdentity hashes the selected rustc identity and effective compiler environment; returns a safe key fragment.
// compilerIdentity 对实际选择的 rustc 身份及编译环境求摘要，返回可安全用于键名的片段。
// Parameters: rustcVersion is rustc -vV output; environment contains the job's process variables.
// 参数：rustcVersion 为 rustc -vV 输出；environment 包含作业进程环境变量。
export function compilerIdentity(rustcVersion, environment) {
  if (!rustcVersion.trim()) throw new Error('Selected rustc identity is empty.');
  // Compiler inputs must separate incompatible archives, while GitHub run IDs must not defeat reuse.
  // 编译输入必须隔离不兼容归档，GitHub 运行编号则不能破坏复用。
  const compilerEnvironment = Object.entries(environment)
    .filter(([name]) => /^(CARGO|RUST|CC|CXX|CFLAGS|CPPFLAGS|CMAKE|LDFLAGS|CL$|_CL_$|LIB$|INCLUDE$)/.test(name))
    .sort(([left], [right]) => left < right ? -1 : left > right ? 1 : 0);
  return createHash('sha256').update(JSON.stringify([rustcVersion.trim(), compilerEnvironment])).digest('hex');
}

// emitCacheConfiguration writes the selected compiler hash and the single authoritative path list for restore/save.
// emitCacheConfiguration 写入选中编译器摘要及恢复、保存共同使用的唯一路径清单。
// Parameters: environment must declare GITHUB_OUTPUT and an isolated CARGO_TARGET_DIR; returns no value.
// 参数：environment 必须声明 GITHUB_OUTPUT 与独立的 CARGO_TARGET_DIR；无返回值。
function emitCacheConfiguration(environment) {
  if (!environment.GITHUB_OUTPUT || !environment.CARGO_TARGET_DIR) {
    throw new Error('Rust CI cache requires GITHUB_OUTPUT and CARGO_TARGET_DIR.');
  }
  // Only Cargo downloads and build products enter the archive, never application output or credentials.
  // 归档只包含 Cargo 下载内容和编译产物，不包含应用输出或凭证。
  const cargoHome = environment.CARGO_HOME || path.join(os.homedir(), '.cargo');
  const cachePaths = [path.join(cargoHome, 'registry'), path.join(cargoHome, 'git'), environment.CARGO_TARGET_DIR];
  const compiler = compilerIdentity(execFileSync('rustc', ['-vV'], { encoding: 'utf8' }), environment);
  appendFileSync(environment.GITHUB_OUTPUT, `compiler=${compiler}\npaths<<RUST_CACHE_PATHS\n${cachePaths.join('\n')}\nRUST_CACHE_PATHS\n`);
}

// Imports expose the pure identity calculation to tests; only direct invocation writes workflow outputs.
// 导入仅向测试暴露纯身份计算函数，只有直接执行才写入工作流输出。
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  emitCacheConfiguration(process.env);
}
