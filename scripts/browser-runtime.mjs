// Prepare the pinned, offline browser component at build time.
// 在构建时准备锁定版本、可离线使用的浏览器组件。
import { access, readFile, readdir, mkdir, rm } from 'node:fs/promises';
import { spawn, execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { managedNodeVersions, prepareManagedNode } from './managed-node.mjs';
import { buildInputPaths } from './build-input-paths.mjs';

// repository and pluginNode identify the authoritative source package.
// repository 与 pluginNode 标识权威源码包。
const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const pluginNode = path.join(repository, 'plugins/vulcan-browser/node');
// This tuple is an adaptation boundary, never a moving registry tag.
// 此版本组合是适配边界，绝不使用浮动注册表标签。
export const browserVersions = Object.freeze({ mcp: '0.0.82', playwright: '1.64.0-alpha-1789764292000', node: '24.18.0', pnpm: '11.11.0' });
// Reject managed runtime upgrades until the browser adaptation is explicitly updated.
// 在浏览器适配被显式更新前拒绝受管运行时升级。
if (managedNodeVersions.node_version !== browserVersions.node || managedNodeVersions.pnpm_version !== browserVersions.pnpm) {
  throw new Error('Managed Node/pnpm versions differ from the adapted browser tuple.');
}
// cacheRoot separates Playwright Chromium from Electron and user browser state.
// cacheRoot 将 Playwright Chromium 与 Electron、用户浏览器状态分离。
export const browserCacheRoot = path.join(buildInputPaths(repository).browsers, browserVersions.playwright, `${process.platform}-${process.arch}`);
// execute captures machine-readable output from the exact managed Node binary.
// execute 从精确受管 Node 程序捕获机器可读输出。
const execute = promisify(execFile);

// Resolve the pinned managed Node under root; return its executable or fail with a build instruction.
// 解析 root 中锁定的受管 Node；返回可执行程序，缺失时给出构建指引。
export async function browserNodeExecutable(root) {
  // platform follows the managed runtime manifest's published platform contract.
  // platform 遵循受管运行时清单的平台契约。
  const platforms = { win32: 'windows', darwin: 'macos', linux: 'linux' };
  const platform = platforms[process.platform];
  if (!platform) throw new Error(`Unsupported browser platform: ${process.platform}`);
  const installRoot = path.join(root, 'dependencies/runtimes/node', `node-${browserVersions.node}-${platform}-${process.arch}`);
  const manifest = JSON.parse(await readFile(path.join(installRoot, 'runtime-manifest.json'), 'utf8'));
  if (manifest.runtime !== 'node' || manifest.version !== browserVersions.node || manifest.platform !== `${platform}-${process.arch}`) {
    throw new Error('Pinned browser Node runtime does not match; run make init-system.');
  }
  const executable = path.resolve(installRoot, manifest.executable);
  if (!executable.startsWith(installRoot + path.sep)) throw new Error('Managed Node executable escapes its package.');
  await access(executable);
  return executable;
}

// Check installed dependencies against the adapted tuple; no registry or user configuration is accessed.
// 按已适配组合检查已安装依赖；不访问注册表，也不访问用户配置。
export async function verifyBrowserDependencies(nodeRoot = pluginNode) {
  const names = { '@playwright/mcp': browserVersions.mcp, playwright: browserVersions.playwright, 'playwright-core': browserVersions.playwright };
  for (const [name, version] of Object.entries(names)) {
    const installed = JSON.parse(await readFile(path.join(nodeRoot, 'node_modules', name, 'package.json'), 'utf8'));
    if (installed.version !== version) throw new Error(`Browser dependency ${name} must be ${version}, found ${installed.version}.`);
  }
  const mcp = JSON.parse(await readFile(path.join(nodeRoot, 'node_modules/@playwright/mcp/package.json'), 'utf8'));
  if (mcp.dependencies.playwright !== browserVersions.playwright || mcp.dependencies['playwright-core'] !== browserVersions.playwright) {
    throw new Error('MCP dependency protocol differs from the adapted Playwright version.');
  }
  // The plugin loader rejects any link inside a package, so a tree with one would ship a disabled browser.
  // 插件加载器拒绝包内任何链接，带链接的依赖树会发布出一个不可用的浏览器。
  const link = await findLink(path.join(nodeRoot, 'node_modules'));
  if (link) throw new Error(`Browser dependency tree must not contain links: ${link}`);
}

// Remove the `.bin` shims pnpm writes; on Linux and macOS they are symbolic links, and the plugin
// launches its MCP server from the package's own `bin` entry instead.
// 移除 pnpm 写入的 `.bin` 入口；它们在 Linux 与 macOS 上是符号链接，插件改由包自身的 `bin` 条目启动 MCP。
// directory is a node_modules tree; nested node_modules trees are handled recursively.
// directory 是一个 node_modules 目录树；嵌套的 node_modules 递归处理。
async function removeBinShims(directory) {
  for (const item of await readdir(directory, { withFileTypes: true })) {
    const itemPath = path.join(directory, item.name);
    if (item.name === '.bin' && path.basename(directory) === 'node_modules') await rm(itemPath, { recursive: true, force: true });
    else if (item.isDirectory()) await removeBinShims(itemPath);
  }
}

// Return the first symbolic link or junction under directory, or null when there is none.
// 返回 directory 下第一个符号链接或目录联接；没有时返回 null。
async function findLink(directory) {
  for (const item of await readdir(directory, { withFileTypes: true })) {
    const itemPath = path.join(directory, item.name);
    if (item.isSymbolicLink()) return itemPath;
    if (item.isDirectory()) {
      const nested = await findLink(itemPath);
      if (nested) return nested;
    }
  }
  return null;
}

// Read prepared browser inputs for packaging; root selects the distribution's managed Node.
// 读取已准备的浏览器打包输入；root 选择发行物的受管 Node。
export async function browserPackageInputs(root) {
  await verifyBrowserDependencies();
  const executable = await browserNodeExecutable(root);
  // Resolve through Playwright in a fresh process so its registry sees the owned browser cache.
  // 在新进程中通过 Playwright 解析路径，确保其注册表读取自有浏览器缓存。
  const { stdout } = await execute(executable, [...playwrightNodeArguments, '-e', "process.stdout.write(require('playwright-core').chromium.executablePath())"], {
    cwd: pluginNode, windowsHide: true, env: { ...process.env, PLAYWRIGHT_BROWSERS_PATH: browserCacheRoot },
  });
  const chromiumExecutable = path.resolve(stdout.trim());
  const relative = path.relative(browserCacheRoot, chromiumExecutable);
  if (!relative || relative.startsWith('..') || path.isAbsolute(relative)) throw new Error('Chromium executable escapes the prepared cache.');
  await access(chromiumExecutable);
  const sourceRoot = path.join(browserCacheRoot, relative.split(path.sep)[0]);
  // The cache holds exactly what `install chromium` chose for this platform: besides Chromium that is
  // ffmpeg for video recording and, on Windows, winldd. Ship every such component rather than restating
  // Playwright's list; dot entries are its bookkeeping, not components.
  // 缓存内恰是 `install chromium` 为本平台选定的内容：除 Chromium 外还有录屏用的 ffmpeg，以及
  // Windows 上的 winldd。随包带上全部组件而不复述 Playwright 的清单；以点开头的条目是其记账信息。
  const componentRoots = (await readdir(browserCacheRoot, { withFileTypes: true }))
    .filter(item => item.isDirectory() && !item.name.startsWith('.') && item.name !== path.basename(sourceRoot))
    .map(item => path.join(browserCacheRoot, item.name));
  return { executable, sourceRoot, executableRelative: path.relative(sourceRoot, chromiumExecutable), componentRoots };
}

// Run one build-only process; reject failed exits and preserve upstream diagnostics.
// 运行单个仅用于构建的进程；拒绝失败退出并保留上游诊断。
async function run(executable, args, options = {}) {
  await new Promise((resolve, reject) => {
    const child = spawn(executable, args, { stdio: 'inherit', windowsHide: true, ...options });
    child.once('error', reject);
    child.once('exit', code => code === 0 ? resolve() : reject(new Error(`Browser preparation failed: ${code}`)));
  });
}

// playwrightNodeArguments precede every Playwright build command: on macOS x64 they preload playwright-x64-host.cjs
// so Playwright selects the x64 browsers under Rosetta too.
// playwrightNodeArguments 位于每个 Playwright 构建命令之前：在 macOS x64 上预加载 playwright-x64-host.cjs，使
// Playwright 在 Rosetta 下同样选择 x64 浏览器。
const playwrightNodeArguments = process.platform === 'darwin' && process.arch === 'x64'
  ? ['--require', fileURLToPath(new URL('./playwright-x64-host.cjs', import.meta.url))]
  : [];

// Prepare all network dependencies explicitly during sync, never from an end-user session.
// 在同步期间显式准备全部网络依赖，绝不由最终用户会话下载。
// root is the profile-owned distribution root chosen by the caller, so one profile never prepares another's tree.
// root 是调用方选定、由模式拥有的发行根，因此一个模式绝不会准备另一个模式的目录树。
async function prepare(root) {
  await prepareManagedNode(root);
  const executable = await browserNodeExecutable(root);
  const pnpm = path.join(root, 'dependencies/runtimes/node', `pnpm-${browserVersions.pnpm}`, 'bin/pnpm.cjs');
  await run(executable, [pnpm, 'install', '--frozen-lockfile', '--ignore-scripts', '--node-linker=hoisted'], { cwd: pluginNode });
  await removeBinShims(path.join(pluginNode, 'node_modules'));
  await verifyBrowserDependencies();
  await mkdir(browserCacheRoot, { recursive: true });
  await run(executable, [...playwrightNodeArguments, path.join(pluginNode, 'node_modules/playwright-core/cli.js'), 'install', 'chromium', '--no-shell'], {
    cwd: pluginNode, env: { ...process.env, PLAYWRIGHT_BROWSERS_PATH: browserCacheRoot },
  });
  await browserPackageInputs(root);
  console.log(`Built-in browser prepared: MCP ${browserVersions.mcp}, Playwright ${browserVersions.playwright}`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv[2] !== 'prepare' || process.argv.length !== 4) {
    throw new Error('Usage: node scripts/browser-runtime.mjs prepare <distribution-root>');
  }
  await prepare(path.resolve(process.argv[3]));
}
