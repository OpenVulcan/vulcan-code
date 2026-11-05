// Choose the runner of each self-hosted pool for one CI or release run (docs/contracts/self-hosted-runners.md).
// 为一次 CI 或发布运行选择各自托管池的运行器（docs/contracts/self-hosted-runners.md）。
//
// A pool is one build platform served by the maintainer's own machines, each registered with the pool's label.
// The output `pools` maps every pool to use in this run to its label; a job runs on
// `fromJSON(pools)[<pool>] || <its hosted image>`, so a pool left out means the hosted image.
// 池是由维护者自有机器提供的一个构建平台，每台机器以该池的标签注册。输出 `pools` 将本次运行要使用的每个池映射
// 到其标签；作业运行于 `fromJSON(pools)[<池>] || <其托管镜像>`，因此未列出的池即使用托管镜像。
//
// Modes: `auto` uses a pool only while one of its runners is online (needs RUNNER_STATUS_TOKEN, a token that may
// read the repository's runners), `hosted` uses none, `self-hosted` uses every pool even when no runner is online,
// so its jobs wait for one. On the maintainer's machine, `register` and `unregister` set up a runner on a machine
// reached over SSH, in a WSL distribution of this machine, or on this machine itself.
// 模式：`auto` 只在池中有运行器在线时使用该池（需要可读取仓库运行器的 RUNNER_STATUS_TOKEN）；`hosted` 不使用
// 任何池；`self-hosted` 无论是否在线都使用全部池，其作业将等待运行器上线。在维护者机器上，`register` 与
// `unregister` 在经 SSH 访问的机器、本机的 WSL 发行版或本机上设置运行器。
import { spawnSync } from 'node:child_process';
import { appendFileSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

// POOLS lists the platforms whose jobs may run on the maintainer's machines. A linux-x64 machine must run
// Ubuntu 22.04, since the release archive links against its glibc 2.35 (release.yml checks it); linux-arm64
// stays hosted because the build scripts produce the platform of the machine they run on.
// POOLS 列出作业可以运行在维护者机器上的平台。linux-x64 机器必须运行 Ubuntu 22.04，因为发布归档链接其
// glibc 2.35（release.yml 会检查）；linux-arm64 保留托管，因为构建脚本只产出其所在机器的平台。
export const POOLS = ['macos-x64', 'macos-arm64', 'linux-x64', 'windows-x64'];

// MODES are the accepted values of the `runners` input.
// MODES 是 `runners` 输入接受的取值。
export const MODES = ['auto', 'hosted', 'self-hosted'];

// poolLabel returns the runner label of `pool`; a machine registers with exactly this label.
// poolLabel 返回 `pool` 的运行器标签；机器正是以此标签注册。
export function poolLabel(pool) {
  return `vulcan-${pool}`;
}

// ALIASES name jobs that any of several pools may take, in order of preference: the macOS checks of ci.yml
// compile platform code, which either processor architecture checks alike. An alias takes the first of its
// pools with a runner online, or its first chosen pool when the status is unknown.
// ALIASES 命名可由多个池中任一个运行的作业，按优先顺序排列：ci.yml 的 macOS 检查编译平台代码，两种处理器
// 架构检查效果相同。别名取其池中第一个有运行器在线的池；状态未知时取其第一个被选中的池。
export const ALIASES = { macos: ['macos-arm64', 'macos-x64'] };

// selectPools returns the map from each pool and alias to use to its label. `runners` is the GitHub runner
// list (`{ status, labels: [{ name }] }`), or null when it could not be read.
// selectPools 返回要使用的各池与别名到其标签的映射。`runners` 为 GitHub 运行器列表
// （`{ status, labels: [{ name }] }`），无法读取时为 null。
export function selectPools(mode, runners) {
  if (!MODES.includes(mode)) throw new Error(`unknown runners mode "${mode}"; expected ${MODES.join(', ')}`);
  if (mode === 'hosted') return {};
  const forced = mode === 'self-hosted';
  const online = pool => (runners ?? []).some(runner =>
    runner.status === 'online' && runner.labels.some(entry => entry.name === poolLabel(pool)));
  const pools = {};
  for (const pool of POOLS) {
    if (forced || online(pool)) pools[pool] = poolLabel(pool);
  }
  for (const [alias, members] of Object.entries(ALIASES)) {
    const pool = members.find(online) ?? members.find(member => pools[member]);
    if (pool) pools[alias] = pools[pool];
  }
  return pools;
}

// listRunners reads every self-hosted runner of `repository` with `token`.
// listRunners 使用 `token` 读取 `repository` 的全部自托管运行器。
async function listRunners(repository, token) {
  const runners = [];
  for (let page = 1; ; page++) {
    const response = await fetch(`https://api.github.com/repos/${repository}/actions/runners?per_page=100&page=${page}`, {
      headers: { Accept: 'application/vnd.github+json', Authorization: `Bearer ${token}`, 'X-GitHub-Api-Version': '2022-11-28' },
    });
    if (!response.ok) throw new Error(`listing the runners of ${repository} failed with HTTP ${response.status}`);
    const body = await response.json();
    runners.push(...body.runners);
    if (runners.length >= body.total_count || body.runners.length === 0) return runners;
  }
}

// USAGE describes the register and unregister commands.
// USAGE 描述 register 与 unregister 命令。
const USAGE = `usage: ci-runners.mjs register <where> <${POOLS.join('|')}> [--name <name>] [--dir <directory>] [--manual]
       ci-runners.mjs unregister <where> [--dir <directory>]
<where> is <user@host> (SSH), wsl:<distribution> or local; --manual installs no service, so the runner is
online only while its run script is started by hand.`;

// runnerCommand registers (`register`) or removes (`unregister`) the runner of a machine: `where` is an SSH
// target, `wsl:<distribution>` or `local`. The token comes from the maintainer's gh and goes to
// scripts/self-hosted-runner.sh (or .ps1 on Windows) on standard input, so it is never printed.
// runnerCommand 注册（`register`）或移除（`unregister`）一台机器的运行器：`where` 为 SSH 目标、
// `wsl:<发行版>` 或 `local`。令牌取自维护者的 gh，经标准输入交给 scripts/self-hosted-runner.sh
// （Windows 上为 .ps1），因此从不打印。
function runnerCommand(command, argv) {
  const { values, positionals } = parseArgs({
    args: argv,
    allowPositionals: true,
    options: { name: { type: 'string' }, dir: { type: 'string' }, manual: { type: 'boolean', default: false } },
  });
  const [where, pool] = positionals;
  if (!where || (command === 'register' && !POOLS.includes(pool))) throw new Error(USAGE);
  const run = (program, args, input) => {
    const result = spawnSync(program, args, { input, encoding: 'utf8', stdio: ['pipe', 'pipe', 'inherit'] });
    if (result.status !== 0) throw new Error(`${program} failed with status ${result.status ?? result.error?.message}`);
    return result.stdout;
  };
  const repository = run('gh', ['repo', 'view', '--json', 'nameWithOwner', '-q', '.nameWithOwner']).trim();
  const kind = command === 'register' ? 'registration-token' : 'remove-token';
  const token = run('gh', ['api', '-X', 'POST', `repos/${repository}/actions/runners/${kind}`, '-q', '.token']).trim();
  const tokenLine = `${token}\n`;
  const quote = value => `'${value.replaceAll("'", '')}'`;
  const options = [
    ...(command === 'register' ? ['--repository', repository, '--label', poolLabel(pool)] : []),
    ...(values.name ? ['--name', values.name] : []),
    ...(values.dir ? ['--dir', values.dir] : []),
    ...(values.manual ? ['--manual'] : []),
  ];
  const action = command === 'register' ? 'install' : 'remove';
  if (where === 'local' && process.platform === 'win32') {
    const script = fileURLToPath(new URL('./self-hosted-runner.ps1', import.meta.url));
    process.stdout.write(run('pwsh', ['-NoProfile', '-File', script, action, ...options], tokenLine));
    return;
  }
  // A POSIX machine gets the script first, then runs it with the token on standard input.
  // POSIX 机器先收到脚本，再以标准输入上的令牌运行它。
  const shell = where === 'local' ? ['sh', '-c']
    : where.startsWith('wsl:') ? ['wsl.exe', '-d', where.slice(4), '--', 'sh', '-c']
      : ['ssh', '-o', 'BatchMode=yes', where];
  const script = '"$HOME/.vulcan-self-hosted-runner.sh"';
  run(shell[0], [...shell.slice(1), `cat > ${script}`], readFileSync(new URL('./self-hosted-runner.sh', import.meta.url), 'utf8'));
  const args = [action, ...options].map(quote).join(' ');
  process.stdout.write(run(shell[0], [...shell.slice(1), `sh ${script} ${args}; code=$?; rm -f ${script}; exit $code`], tokenLine));
}

if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) {
  const [command, ...rest] = process.argv.slice(2);
  if (command === 'register' || command === 'unregister') {
    runnerCommand(command, rest);
    process.exit(0);
  }
  if (command) throw new Error(`unknown command ${command}; expected register or unregister`);
  const mode = process.env.RUNNERS_MODE || 'auto';
  const { GITHUB_OUTPUT: output, GITHUB_REPOSITORY: repository, RUNNER_STATUS_TOKEN: token } = process.env;
  if (!output || !repository) throw new Error('GITHUB_OUTPUT and GITHUB_REPOSITORY are required.');
  // `self-hosted` reads the status too when it can, so an alias prefers a pool with a runner online.
  // `self-hosted` 在可以时同样读取状态，使别名优先选择有运行器在线的池。
  let runners = null;
  const fallback = mode === 'auto' ? 'every job runs on its hosted image' : 'aliases take their first pool';
  if (mode !== 'hosted') {
    if (!token) {
      console.log(`::warning::RUNNER_STATUS_TOKEN is not set, so ${fallback}.`);
    } else {
      try {
        runners = await listRunners(repository, token);
      } catch (error) {
        console.log(`::warning::${error.message}; ${fallback}.`);
      }
    }
  }
  const pools = selectPools(mode, runners);
  for (const pool of [...POOLS, ...Object.keys(ALIASES)]) console.log(`${pool}: ${pools[pool] ?? 'hosted image'}`);
  appendFileSync(output, `pools=${JSON.stringify(pools)}\n`);
}
