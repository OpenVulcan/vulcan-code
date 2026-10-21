// Run the static quality gates locally; tests stay in `make test`.
// 在本地运行静态质量门禁；测试仍由 `make test` 负责。
//
// Every warning is an error here, so a clean tree stays clean while hosted CI is paused.
// 此处所有警告都视为错误，使托管 CI 暂停期间干净的代码树保持干净。
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// Repository root keeps invocation independent of the caller's working directory.
// 仓库根目录确保入口不依赖调用者当前工作目录。
const root = fileURLToPath(new URL('..', import.meta.url));

// The standalone code-mode host workspace reads its own .cargo configuration from its directory.
// 独立的代码模式宿主工作区从其目录读取自己的 .cargo 配置。
const codeModeHost = 'native/code-mode-host';

// Each step is one command with an optional working directory relative to the repository root;
// every step runs and the run fails if any of them failed.
// 每个步骤是一条命令，可选相对仓库根目录的工作目录；全部步骤都会执行，任一失败则整体失败。
const steps = [
  ['cargo', ['fmt', '--all', '--', '--check']],
  ['cargo', ['clippy', '--locked', '--workspace', '--all-targets', '--', '-D', 'warnings']],
  ['cargo', ['fmt', '--all', '--', '--check'], codeModeHost],
  // The v8 build script needs its gn_root junction before clippy compiles the host workspace.
  // 在 clippy 编译宿主工作区之前，v8 构建脚本需要其 gn_root 联接。
  ['node', ['scripts/prepare-v8-build-root.mjs']],
  ['cargo', ['clippy', '--locked', '--workspace', '--all-targets', '--', '-D', 'warnings'], codeModeHost],
  ['pnpm', ['--dir', 'apps/vulcan-code-web', 'run', 'lint']],
  ['pnpm', ['--dir', 'apps/vulcan-code-web', 'run', 'typecheck']],
  ['pnpm', ['--dir', 'apps/vulcan-code-web', 'run', 'check:ui-architecture']],
  ['pnpm', ['--dir', 'apps/vulcan-code-desktop', 'run', 'typecheck']],
  ['node', ['scripts/wording-assertions.mjs']],
  ['node', ['scripts/backend-cjk-literals.mjs']],
  ['node', ['scripts/test-home-isolation.mjs']],
];

// pnpm is a .cmd shim on Windows, which only a shell can launch. The shell receives one command line (the
// fixed step arguments contain no spaces or quotes), since passing an argument list with a shell is deprecated.
// pnpm 在 Windows 上是 .cmd 包装脚本，只能通过 shell 启动。shell 接收一整行命令（固定步骤参数不含空格或引号），
// 因为在 shell 模式下传入参数列表已被弃用。
const useShell = process.platform === 'win32';

const failed = [];
for (const [command, args, directory] of steps) {
  const commandLine = [command, ...args].join(' ');
  const label = directory ? `${commandLine} (in ${directory})` : commandLine;
  const cwd = directory ? path.join(root, directory) : root;
  console.log(`\n==> ${label}`);
  const result = useShell && command === 'pnpm'
    ? spawnSync(commandLine, { cwd, stdio: 'inherit', shell: true })
    : spawnSync(command, args, { cwd, stdio: 'inherit' });
  if (result.error) throw result.error;
  if (result.status !== 0) failed.push(label);
}
if (failed.length > 0) {
  console.error(`\nFailed gates:\n${failed.map((label) => `  - ${label}`).join('\n')}`);
  process.exit(1);
}
console.log('\nAll static quality gates passed.');
