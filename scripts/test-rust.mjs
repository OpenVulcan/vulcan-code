// Run the Rust test suite locally: `fast` for the inner loop, `full` before merging or releasing.
// 在本地运行 Rust 测试：`fast` 用于内循环，`full` 用于合并或发布前。
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// Repository root keeps invocation independent of the caller's working directory.
// 仓库根目录确保入口不依赖调用者当前工作目录。
const root = fileURLToPath(new URL('..', import.meta.url));

// mode selects the suite; nextest profiles and thread limits stay in .config/nextest.toml.
// mode 选择测试集；nextest 配置与线程上限保留在 .config/nextest.toml 中。
const mode = process.argv[2] ?? 'fast';

// The standalone code-mode host workspace reads its own .cargo and nextest configuration from its directory.
// 独立的代码模式宿主工作区从其目录读取自己的 .cargo 与 nextest 配置。
const codeModeHost = 'native/code-mode-host';

// Each step is one command with an optional working directory relative to the repository root;
// the first failing step stops the run with its exit status.
// 每个步骤是一条命令，可选相对仓库根目录的工作目录；首个失败步骤以其退出码终止运行。
const steps = {
  fast: [
    ['cargo', ['nextest', 'run', '--locked', '--workspace', '--profile', 'fast', '--no-fail-fast']],
    ['node', ['scripts/prepare-v8-build-root.mjs']],
    ['cargo', ['nextest', 'run', '--locked', '--workspace', '--profile', 'fast', '--no-fail-fast'], codeModeHost],
  ],
  full: [
    ['cargo', ['nextest', 'run', '--locked', '--workspace', '--no-fail-fast']],
    ['node', ['scripts/prepare-v8-build-root.mjs']],
    ['cargo', ['nextest', 'run', '--locked', '--workspace', '--no-fail-fast'], codeModeHost],
    // Process-global LuaSkills callback tests need one fresh process each.
    // 进程全局 LuaSkills 回调测试需要各自独立的进程。
    ['node', ['scripts/test-isolated-callbacks.mjs']],
    // nextest does not run doctests.
    // nextest 不运行文档测试。
    ['cargo', ['test', '--locked', '--workspace', '--doc']],
  ],
}[mode];

if (!steps) {
  console.error(`Unsupported test mode '${mode}'; use 'fast' or 'full'.`);
  process.exit(1);
}
for (const [command, args, directory] of steps) {
  const result = spawnSync(command, args, { cwd: directory ? path.join(root, directory) : root, stdio: 'inherit' });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
