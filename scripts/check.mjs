// Run the static quality gates locally; tests stay in `make test`.
// 在本地运行静态质量门禁；测试仍由 `make test` 负责。
//
// Every warning is an error here; hosted CI runs the same groups (`node scripts/check.mjs <group>...`).
// 此处所有警告都视为错误；托管 CI 运行同样的分组（`node scripts/check.mjs <分组>...`）。
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

// Repository root keeps invocation independent of the caller's working directory.
// 仓库根目录确保入口不依赖调用者当前工作目录。
const root = fileURLToPath(new URL('..', import.meta.url));

// Each group lists one-command steps; CI runs single groups, so every gate is defined only here.
// 每个分组列出单条命令步骤；CI 按分组运行，使每项门禁只在此处定义。
const groups = {
  rustfmt: [['cargo', ['fmt', '--all', '--', '--check']]],
  clippy: [['cargo', ['clippy', '--locked', '--workspace', '--all-targets', '--', '-D', 'warnings']]],
  web: [
    ['pnpm', ['--dir', 'apps/vulcan-code-web', 'run', 'lint']],
    ['pnpm', ['--dir', 'apps/vulcan-code-web', 'run', 'typecheck']],
    ['pnpm', ['--dir', 'apps/vulcan-code-web', 'run', 'check:ui-architecture']],
  ],
  desktop: [['pnpm', ['--dir', 'apps/vulcan-code-desktop', 'run', 'typecheck']]],
  repository: [
    ['node', ['scripts/wording-assertions.mjs']],
    ['node', ['scripts/backend-cjk-literals.mjs']],
    ['node', ['scripts/test-home-isolation.mjs']],
  ],
};

// selected names the groups to run; no argument runs every group in declaration order.
// selected 指定要运行的分组；不带参数时按声明顺序运行全部分组。
const selected = process.argv.length > 2 ? process.argv.slice(2) : Object.keys(groups);
const unknown = selected.filter((name) => !Object.hasOwn(groups, name));
if (unknown.length > 0) {
  console.error(`Unknown check groups: ${unknown.join(', ')}; expected ${Object.keys(groups).join(', ')}.`);
  process.exit(1);
}
const steps = selected.flatMap((name) => groups[name]);

// pnpm is a .cmd shim on Windows, which only a shell can launch. The shell receives one command line (the
// fixed step arguments contain no spaces or quotes), since passing an argument list with a shell is deprecated.
// pnpm 在 Windows 上是 .cmd 包装脚本，只能通过 shell 启动。shell 接收一整行命令（固定步骤参数不含空格或引号），
// 因为在 shell 模式下传入参数列表已被弃用。
const useShell = process.platform === 'win32';

const failed = [];
for (const [command, args] of steps) {
  const label = [command, ...args].join(' ');
  console.log(`\n==> ${label}`);
  const result = useShell && command === 'pnpm'
    ? spawnSync(label, { cwd: root, stdio: 'inherit', shell: true })
    : spawnSync(command, args, { cwd: root, stdio: 'inherit' });
  if (result.error) throw result.error;
  if (result.status !== 0) failed.push(label);
}
if (failed.length > 0) {
  console.error(`\nFailed gates:\n${failed.map((label) => `  - ${label}`).join('\n')}`);
  process.exit(1);
}
console.log('\nAll static quality gates passed.');
