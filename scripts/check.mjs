// Run the static quality gates locally; tests stay in `make test`.
// 在本地运行静态质量门禁；测试仍由 `make test` 负责。
//
// Every warning is an error here, so a clean tree stays clean while hosted CI is paused.
// 此处所有警告都视为错误，使托管 CI 暂停期间干净的代码树保持干净。
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

// Repository root keeps invocation independent of the caller's working directory.
// 仓库根目录确保入口不依赖调用者当前工作目录。
const root = fileURLToPath(new URL('..', import.meta.url));

// Each step is one command; every step runs and the run fails if any of them failed.
// 每个步骤是一条命令；全部步骤都会执行，任一失败则整体失败。
const steps = [
  ['cargo', ['fmt', '--all', '--', '--check']],
  ['cargo', ['clippy', '--locked', '--workspace', '--all-targets', '--', '-D', 'warnings']],
  ['pnpm', ['--dir', 'apps/vulcan-code-web', 'run', 'lint']],
  ['pnpm', ['--dir', 'apps/vulcan-code-web', 'run', 'typecheck']],
  ['pnpm', ['--dir', 'apps/vulcan-code-web', 'run', 'check:ui-architecture']],
  ['pnpm', ['--dir', 'apps/vulcan-code-desktop', 'run', 'typecheck']],
  ['node', ['scripts/wording-assertions.mjs']],
  ['node', ['scripts/backend-cjk-literals.mjs']],
  ['node', ['scripts/test-home-isolation.mjs']],
];

// pnpm is a .cmd shim on Windows, which only a shell can launch.
// pnpm 在 Windows 上是 .cmd 包装脚本，只能通过 shell 启动。
const useShell = process.platform === 'win32';

const failed = [];
for (const [command, args] of steps) {
  const label = [command, ...args].join(' ');
  console.log(`\n==> ${label}`);
  const result = spawnSync(command, args, { cwd: root, stdio: 'inherit', shell: useShell && command === 'pnpm' });
  if (result.error) throw result.error;
  if (result.status !== 0) failed.push(label);
}
if (failed.length > 0) {
  console.error(`\nFailed gates:\n${failed.map((label) => `  - ${label}`).join('\n')}`);
  process.exit(1);
}
console.log('\nAll static quality gates passed.');
