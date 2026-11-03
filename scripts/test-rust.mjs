// Run the Rust test suite: `fast` for the inner loop and the CI set, `full` before merging, before releasing
// and in the full-tests workflow, `quarantine` for the known-unstable tests that the other two leave out.
// 运行 Rust 测试：`fast` 用于内循环与 CI，`full` 用于合并、发布前与完整测试工作流，`quarantine` 运行另两者
// 排除的已知不稳定测试。
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { loadQuarantine, quarantinedFilter } from './ci-quarantine.mjs';

// Repository root keeps invocation independent of the caller's working directory.
// 仓库根目录确保入口不依赖调用者当前工作目录。
const root = fileURLToPath(new URL('..', import.meta.url));

// mode selects the suite; nextest profiles and thread limits stay in .config/nextest.toml.
// mode 选择测试集；nextest 配置与线程上限保留在 .config/nextest.toml 中。
const mode = process.argv[2] ?? 'fast';

// Test binaries load the native LanceDB library from the profile directory, and test builds never produce it.
// 测试二进制从 profile 目录加载原生 LanceDB 库，而测试构建不会生成它。
const library = ['cargo', ['build', '--locked', '-p', 'vulcan-lancedb-native']];

// quarantined selects the known-unstable tests of .config/ci-quarantine.json; fast and full leave them out.
// quarantined 选中 .config/ci-quarantine.json 中已知不稳定的测试；fast 与 full 不运行它们。
const quarantine = loadQuarantine();
const quarantined = quarantinedFilter(quarantine);
const stable = ['-E', `not (${quarantined})`];

// Each step is one command; the first failing step stops the run with its exit status.
// 每个步骤是一条命令；首个失败步骤以其退出码终止运行。
const steps = {
  fast: [
    library,
    ['cargo', ['nextest', 'run', '--locked', '--workspace', '--profile', 'fast', '--no-fail-fast', ...stable]],
  ],
  full: [
    library,
    ['cargo', ['nextest', 'run', '--locked', '--workspace', '--no-fail-fast', ...stable]],
    // Process-global LuaSkills callback tests need one fresh process each.
    // 进程全局 LuaSkills 回调测试需要各自独立的进程。
    ['node', ['scripts/test-isolated-callbacks.mjs']],
    // nextest does not run doctests.
    // nextest 不运行文档测试。
    ['cargo', ['test', '--locked', '--workspace', '--doc']],
  ],
  // Only the quarantined tests, outside any profile filter, so full-test runs keep collecting their results.
  // 只运行被隔离的测试且不受任何配置过滤，使完整测试运行持续收集其结果。
  quarantine: quarantine.length === 0 ? [] : [
    library,
    ['cargo', ['nextest', 'run', '--locked', '--workspace', '--no-fail-fast', '-E', quarantined]],
  ],
}[mode];

if (!steps) {
  console.error(`Unsupported test mode '${mode}'; use 'fast', 'full' or 'quarantine'.`);
  process.exit(1);
}
if (steps.length === 0) console.log('The CI quarantine list is empty; no tests to run.');
for (const [command, args] of steps) {
  const result = spawnSync(command, args, { cwd: root, stdio: 'inherit' });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
