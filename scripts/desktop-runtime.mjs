// Build and launch the manifest-owned desktop distribution without an implicit browser download.
// 构建并启动清单拥有的桌面发行物，不隐式下载浏览器。
import { cp, readFile, writeFile, mkdir, access, realpath, rm, stat, lstat, readdir, rename } from 'node:fs/promises';
import { spawn, execFile } from 'node:child_process';
import { createRequire } from 'node:module';
import { promisify } from 'node:util';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { browserPackageInputs, browserVersions, verifyBrowserDependencies } from './browser-runtime.mjs';

// The repository is resolved from this script rather than the caller's working directory.
// 仓库由脚本位置确定，不依赖调用方工作目录。
const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
// Output defaults to the shared distribution; an explicit absolute root isolates release verification.
// 输出默认使用共享发行目录；显式绝对根目录可隔离发布验收。
const outputOverride = process.env.VULCAN_DESKTOP_DISTRIBUTION_ROOT;
if (outputOverride && !path.isAbsolute(outputOverride)) {
  throw new Error('VULCAN_DESKTOP_DISTRIBUTION_ROOT must be an absolute path.');
}
if (outputOverride) {
  const targetRoot = path.join(repository, 'target');
  const selectedRoot = path.resolve(outputOverride);
  const relative = path.relative(targetRoot, selectedRoot);
  if (!relative || relative.startsWith('..') || path.isAbsolute(relative)) {
    throw new Error('VULCAN_DESKTOP_DISTRIBUTION_ROOT must be inside the repository target directory.');
  }
}
const output = outputOverride ? path.resolve(outputOverride) : path.join(repository, 'output');
if (path.resolve(output) === repository) {
  throw new Error('The distribution root must not be the repository root.');
}
// Command and profile are explicit; a release launch cannot silently reuse a debug distribution.
// 命令和模式显式指定，发布启动不能静默复用调试发行物。
const [command, profile = 'debug'] = process.argv.slice(2);
// One schema authority is shared by packaging, launching, and the Core restart layout reader.
// 打包、启动与核心重启布局读取器共享唯一的架构版本权威。
const runtimeLayoutSchemaVersion = 2;
// The service describes its compiled profile without starting user services or touching settings.
// 服务描述自身编译模式，不启动用户服务或修改配置。
const execFileAsync = promisify(execFile);

// Reject an existing link in the isolated root path before any sync/build script can clean it.
// 在同步或构建脚本清理独立发行根之前，拒绝路径中已有的目录链接。
async function verifyIsolatedDistributionBoundary() {
  if (!outputOverride) return;
  // current walks only the validated target-root suffix, including the target directory itself.
  // current 仅沿已验证的 target 根后缀遍历，并包含 target 目录本身。
  let current = path.join(repository, 'target');
  const suffix = path.relative(current, output).split(path.sep);
  for (const component of ['', ...suffix]) {
    if (component) current = path.join(current, component);
    try {
      const metadata = await lstat(current);
      if (metadata.isSymbolicLink() || !metadata.isDirectory()) {
        throw new Error(`Isolated distribution path contains a link or non-directory: ${current}`);
      }
    } catch (error) {
      if (error.code === 'ENOENT') return;
      throw error;
    }
  }
}

// Convert one absolute distribution path into the portable slash-separated manifest form.
// 将一个绝对发行路径转换为清单使用的可移植斜杠格式。
export function packageRelativePath(distributionRoot, absolutePath) {
  return path.relative(distributionRoot, absolutePath).split(path.sep).join('/');
}

// Run one exact executable with inherited diagnostics and reject every failed exit.
// 运行一个确定的可执行文件并继承诊断输出，任何失败退出均报错。
// executable and args identify the process; options carry its working directory and environment.
// executable 与 args 标识进程；options 携带工作目录和环境。
async function run(executable, args, options = {}) {
  await new Promise((resolve, reject) => {
    // The child owns only this operation and does not inherit a visible console on Windows.
    // 子进程只拥有本次操作，在 Windows 上不创建可见控制台。
    const child = spawn(executable, args, { stdio: 'inherit', windowsHide: true, ...options });
    child.once('error', reject);
    child.once('exit', (code) => code === 0 ? resolve() : reject(new Error(`${executable} exited with ${code}`)));
  });
}

// Resolve a layout entry beneath the distribution root, rejecting absolute and escaping entries.
// 在发行根内解析清单条目，拒绝绝对路径及逃逸路径。
function entry(relative, distributionRoot = output) {
  if (typeof relative !== 'string' || !relative || path.isAbsolute(relative)) throw new Error('Invalid layout entry.');
  // Containment is checked after resolution and before filesystem use.
  // 路径解析后、访问文件系统前检查包含关系。
  const root = path.resolve(distributionRoot);
  const resolved = path.resolve(root, relative);
  if (!resolved.startsWith(root + path.sep)) throw new Error('Layout entry escapes distribution.');
  return resolved;
}

// Verify every file and directory named by a package layout before publishing it.
// 在发布清单前验证其中声明的每个文件和目录确实存在。
export async function verifyPackagedLayout(layout, distributionRoot = output) {
  const entries = [
    { name: 'serviceExecutable', kind: 'file' },
    { name: 'desktopExecutable', kind: 'file' },
    { name: 'desktopEntry', kind: 'file' },
    { name: 'browserHostEntry', kind: 'file' },
    { name: 'browserHostExecutable', kind: 'file' },
    { name: 'chromiumExecutable', kind: 'file' },
    { name: 'webuiRoot', kind: 'directory' },
  ];
  for (const { name, kind } of entries) {
    const relative = layout[name];
    if (typeof relative !== 'string' || !relative) {
      throw new Error(`Missing package input ${name}: ${String(relative)}.`);
    }
    let resolved;
    try {
      resolved = entry(relative, distributionRoot);
    } catch (error) {
      throw new Error(`Invalid package input ${name} ${relative}: ${error.message}`, { cause: error });
    }
    let metadata;
    try {
      metadata = await stat(resolved);
    } catch (error) {
      if (error.code === 'ENOENT' || error.code === 'ENOTDIR') {
        throw new Error(`Missing package input ${name} ${relative}.`, { cause: error });
      }
      throw new Error(`Unable to inspect package input ${name} ${relative}: ${error.message}`, { cause: error });
    }
    const matchesKind = kind === 'directory' ? metadata.isDirectory() : metadata.isFile();
    if (!matchesKind) {
      throw new Error(`Invalid package input ${name} ${relative}: expected a ${kind}.`);
    }
  }
  // gui/main.js loads its sandbox preload from the application root on every navigation.
  // gui/main.js 每次导航都从应用根目录加载沙箱预加载文件；缺失时客户端会丢失窗口控制。
  const desktopPreload = path.posix.join(path.posix.dirname(layout.desktopEntry), '..', 'preload.cjs');
  await verifyPackageEntry('desktopPreload', desktopPreload, 'file', distributionRoot);
  if (layout.profile === 'release') await verifyReleaseAssets(layout, distributionRoot);
}

// Verify the non-layout assets that a release service needs before it can load plugins or runtimes.
// 验证发布服务加载插件和运行时前必须存在、但不重复写入布局清单的资源。
async function verifyReleaseAssets(layout, distributionRoot) {
  const requiredDirectories = [
    ['resources', 'resources'],
    ['plugins', 'plugins'],
    ['lua_runtime', 'lua_runtime'],
    ['tools', 'tools'],
  ];
  for (const [name, relative] of requiredDirectories) {
    await verifyPackageEntry(name, relative, 'directory', distributionRoot);
  }
  await verifyManagedRuntime(distributionRoot, layout, 'node');
  await verifyManagedRuntime(distributionRoot, layout, 'python');

  const hasChromium = layout.chromiumExecutable !== null && layout.chromiumExecutable !== undefined;
  const capabilities = Array.isArray(layout.capabilities) ? layout.capabilities : [];
  if (capabilities.includes('browser-host') !== hasChromium) {
    throw new Error('Release browser capability must match the declared Chromium package.');
  }
  if (hasChromium) await verifyChromiumPackage(layout, distributionRoot);
}

// Verify one fixed package asset with the same containment and filesystem checks as layout entries.
// 使用与布局条目相同的边界和文件类型检查验证一个固定发行资源。
async function verifyPackageEntry(name, relative, kind, distributionRoot) {
  let resolved;
  try {
    resolved = entry(relative, distributionRoot);
  } catch (error) {
    throw new Error(`Invalid package input ${name} ${relative}: ${error.message}`, { cause: error });
  }
  let metadata;
  try {
    metadata = await stat(resolved);
  } catch (error) {
    if (error.code === 'ENOENT' || error.code === 'ENOTDIR') {
      throw new Error(`Missing package input ${name} ${relative}.`, { cause: error });
    }
    throw new Error(`Unable to inspect package input ${name} ${relative}: ${error.message}`, { cause: error });
  }
  const matchesKind = kind === 'directory' ? metadata.isDirectory() : metadata.isFile();
  if (!matchesKind) throw new Error(`Invalid package input ${name} ${relative}: expected a ${kind}.`);
  return resolved;
}

// Map the runtime layout identity to the managed-runtime platform key used by LuaSkills.
// 将运行时布局身份映射为 LuaSkills 使用的受管运行时平台键。
function managedRuntimePlatformKey(layout) {
  const key = `${layout.platform}-${layout.architecture}`;
  const platformKeys = {
    'win32-x64': 'windows-x64',
    'linux-x64': 'linux-x64',
    'linux-arm64': 'linux-arm64',
    'darwin-x64': 'macos-x64',
    'darwin-arm64': 'macos-arm64',
  };
  const platform = platformKeys[key];
  if (!platform) throw new Error(`Unsupported release managed-runtime platform: ${key}.`);
  return platform;
}

// Verify a matching shared runtime manifest and its declared executable without requiring plugin dependencies.
// 验证匹配的共享运行时清单及其声明的可执行文件，但不要求插件私有依赖已安装。
async function verifyManagedRuntime(distributionRoot, layout, runtime) {
  const label = runtime === 'node' ? 'Node' : 'Python';
  const runtimeRoot = await verifyPackageEntry(`managed${label}RuntimeRoot`, `dependencies/runtimes/${runtime}`, 'directory', distributionRoot);
  const expectedPlatform = managedRuntimePlatformKey(layout);
  const candidates = await readdir(runtimeRoot, { withFileTypes: true });
  for (const candidate of candidates) {
    if (!candidate.isDirectory()) continue;
    const installRoot = path.join(runtimeRoot, candidate.name);
    const manifestPath = path.join(installRoot, 'runtime-manifest.json');
    let manifest;
    try {
      manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
    } catch (error) {
      if (error.code === 'ENOENT' || error.code === 'ENOTDIR') continue;
      throw new Error(`Invalid managed Node runtime manifest ${path.relative(distributionRoot, manifestPath)}: ${error.message}`, { cause: error });
    }
    if (manifest.runtime !== runtime || manifest.platform !== expectedPlatform) continue;
    if (typeof manifest.version !== 'string' || !/^\d+\.\d+\.\d+$/.test(manifest.version)) {
      throw new Error(`Invalid managed ${label} runtime version in ${path.relative(distributionRoot, manifestPath)}.`);
    }
    if (typeof manifest.executable !== 'string' || !manifest.executable.trim()) {
      throw new Error(`Managed ${label} runtime manifest ${path.relative(distributionRoot, manifestPath)} has no executable.`);
    }
    await verifyPackageEntry(`managed${label}Executable`, path.relative(distributionRoot, path.join(installRoot, manifest.executable)), 'file', distributionRoot);
    return;
  }
  throw new Error(`Missing managed ${label} runtime for platform ${expectedPlatform}.`);
}

// Verify the generated Chromium inventory, version evidence, and every copied component file.
// 验证生成的 Chromium 文件清单、版本证据以及每个已复制的组件文件。
async function verifyChromiumPackage(layout, distributionRoot) {
  await verifyPackageEntry('chromiumRoot', 'browser/chromium', 'directory', distributionRoot);
  const manifestRelative = 'browser/chromium/package-manifest.json';
  const manifestPath = await verifyPackageEntry('chromiumPackageManifest', manifestRelative, 'file', distributionRoot);
  let manifest;
  try {
    manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
  } catch (error) {
    throw new Error(`Invalid Chromium package manifest ${manifestRelative}: ${error.message}`, { cause: error });
  }
  if (manifest.schemaVersion !== 1 || manifest.executable !== layout.chromiumExecutable
    || typeof manifest.version !== 'string' || !/^\d+\.\d+\.\d+(?:\.\d+)?$/.test(manifest.version)
    || !Array.isArray(manifest.files) || manifest.files.length === 0) {
    throw new Error(`Invalid Chromium package declaration ${manifestRelative}.`);
  }
  const listed = new Set();
  for (const component of manifest.files) {
    if (!component || typeof component.path !== 'string' || listed.has(component.path)
      || !Number.isSafeInteger(component.bytes) || component.bytes < 0) {
      throw new Error(`Invalid Chromium component declaration in ${manifestRelative}.`);
    }
    listed.add(component.path);
    const componentPath = await verifyPackageEntry('chromiumComponent', component.path, 'file', distributionRoot);
    const componentStat = await stat(componentPath);
    if (componentStat.size !== component.bytes) {
      throw new Error(`Chromium component size changed: ${component.path}.`);
    }
  }
  if (!listed.has(layout.chromiumExecutable)) {
    throw new Error(`Chromium package manifest does not list ${layout.chromiumExecutable}.`);
  }
}

// Build a deterministic inventory so a relocated release can detect truncated Chromium payloads.
// 构建确定性文件清单，使迁移后的发行物能够发现不完整的 Chromium 载荷。
async function chromiumPackageInventory(root, current = root, files = []) {
  const entries = await readdir(current, { withFileTypes: true });
  for (const item of entries) {
    const itemPath = path.join(current, item.name);
    // A previously packaged source may contain this generated manifest; never list the file
    // whose contents are being replaced, or its recorded size will be stale immediately.
    // 复用既有发行载荷作为源时可能带着生成清单；绝不将即将覆盖的文件列入自身清单。
    if (itemPath === path.join(root, 'browser', 'chromium', 'package-manifest.json')) continue;
    if (item.isDirectory()) await chromiumPackageInventory(root, itemPath, files);
    else if (item.isFile()) {
      const metadata = await stat(itemPath);
      files.push({ path: packageRelativePath(output, itemPath), bytes: metadata.size });
    }
  }
  return files.sort((left, right) => left.path.localeCompare(right.path));
}

// Write Chromium version and component evidence after launching the copied browser with the pinned driver.
// 使用锁定驱动启动已复制的浏览器后，写入 Chromium 版本和组件证据。
// chromiumRoot is the copied browser directory; chromiumExecutable is its exact launch target.
// chromiumRoot 是已复制的浏览器目录；chromiumExecutable 是其精确启动目标。
// Returns after the browser has closed and the manifest has been persisted.
// 浏览器关闭且清单持久化后返回。
async function writeChromiumPackageManifest(chromiumRoot, chromiumExecutable) {
  // Windows Chrome for Testing may exit 3 for --version even though Playwright launches it.
  // Windows 上 Chrome for Testing 的 --version 可能返回 3，但 Playwright 仍可正常启动它。
  const requireBrowser = createRequire(path.join(repository, 'plugins/vulcan-browser/node/package.json'));
  const { chromium } = requireBrowser('playwright-core');
  const browser = await chromium.launch({ executablePath: chromiumExecutable, headless: true });
  let version;
  try {
    version = browser.version();
  } finally {
    await browser.close();
  }
  if (!/^\d+\.\d+\.\d+(?:\.\d+)?$/.test(version)) {
    throw new Error('The packaged Chromium did not report a semantic version.');
  }
  const files = await chromiumPackageInventory(output, chromiumRoot);
  await writeFile(path.join(chromiumRoot, 'package-manifest.json'), JSON.stringify({
    schemaVersion: 1,
    executable: packageRelativePath(output, chromiumExecutable),
    version,
    playwrightVersion: browserVersions.playwright,
    files,
  }, null, 2) + '\n');
}

// Replaces only a checked generated subtree so stale Chromium or WebUI files cannot survive a
// profile change or an optional-browser build. The real parent must stay inside the named output.
// 仅替换已核对的生成子树，避免切换配置或不带可选浏览器时残留旧 Chromium 与 WebUI 文件；
// 真实父目录必须位于指定的 output 目录内。
async function resetGeneratedDirectory(relative) {
  const requested = entry(relative);
  const parent = path.dirname(requested);
  await mkdir(parent, { recursive: true });
  const [realOutput, realParent] = await Promise.all([realpath(output), realpath(parent)]);
  if (realParent !== realOutput && !realParent.startsWith(realOutput + path.sep)) {
    throw new Error(`Generated directory parent escapes distribution: ${relative}`);
  }
  const target = path.join(realParent, path.basename(requested));
  if (!target.startsWith(realOutput + path.sep)) {
    throw new Error(`Generated directory escapes distribution: ${relative}`);
  }
  await rm(target, { recursive: true, force: true });
}

// Verify the actual binary, not only a possibly stale manifest's profile label.
// 核对实际程序，而非仅信任可能过期的清单模式标签。
// executable is the resolved service file; mismatch aborts packaging or launch.
// executable 为已解析的服务文件；不匹配时终止打包或启动。
async function verifyServiceProfile(executable) {
  const { stdout } = await execFileAsync(executable, ['--describe'], { windowsHide: true, timeout: 20_000 });
  const description = JSON.parse(stdout);
  if (description.protocolVersion !== 1 || description.profile !== profile) {
    throw new Error(`Service artifact profile ${description.profile} does not match requested ${profile}. Rebuild first.`);
  }
}

async function main() {
  await verifyIsolatedDistributionBoundary();
  if (command === 'root') {
    process.stdout.write(output);
    return;
  }
  if (!['package', 'run', 'service', 'layout'].includes(command) || !['debug', 'release'].includes(profile)) {
    throw new Error('Usage: node scripts/desktop-runtime.mjs root|package|run|service|layout [debug|release]');
  }
  if (command === 'package') {
  // Validate source artifacts before removing an existing generated application tree.
  // 在移除现有生成应用目录前验证源产物，避免配置或编译错误破坏已有发行物。
  const desktop = path.join(repository, 'apps', 'vulcan-code-desktop');
  const serviceExecutable = `bin/vulcan-code-service${process.platform === 'win32' ? '.exe' : ''}`;
  await verifyServiceProfile(entry(serviceExecutable));
  await run(process.execPath, [path.join(desktop, 'node_modules/typescript/bin/tsc'), '-b'], { cwd: desktop });
  await access(path.join(desktop, 'node_modules/electron/dist'));
  await access(path.join(repository, 'apps/vulcan-code-web/dist'));
  // Validate the compiled bridge before replacing any existing GUI files.
  // 替换现有 GUI 文件前先验证编译出的桥接文件。
  await access(path.join(desktop, 'dist/preload.cjs'));
  if (profile === 'release') {
    for (const relative of ['resources', 'plugins', 'lua_runtime', 'tools']) {
      await verifyPackageEntry(relative, relative, 'directory', output);
    }
    const runtimeLayout = { platform: process.platform, architecture: process.arch };
    await verifyManagedRuntime(output, runtimeLayout, 'node');
    await verifyManagedRuntime(output, runtimeLayout, 'python');
  }
  // Require the adapted browser and synced plugin before modifying any existing distribution.
  // 修改已有发行物前，必须具备已适配浏览器与已同步插件。
  const chromiumSource = await browserPackageInputs(output);
  await verifyBrowserDependencies(path.join(output, 'plugins/vulcan-browser/node'));
  await resetGeneratedDirectory('desktop/runtime');
  await resetGeneratedDirectory('webui');
  await resetGeneratedDirectory('browser/chromium');
  await resetGeneratedDirectory('browser/runtime');
  // Electron owns only GUI pages; managed Node and pinned Chromium own Core automation.
  // Electron 仅拥有 GUI 页面；受管 Node 与锁定 Chromium 拥有 Core 自动化。
  // The compiled application preserves ESM and the CommonJS sandbox preload.
  // 编译应用保留 ESM 与沙箱使用的 CommonJS 预加载。
  const appEntry = process.platform === 'darwin'
    ? 'desktop/runtime/Electron.app/Contents/Resources/app' : 'desktop/runtime/resources/app';
  const appRoot = path.join(output, appEntry);
  await mkdir(appRoot, { recursive: true });
  // desktopNotificationIdentity keeps executable metadata and the runtime identity resolver aligned.
  // desktopNotificationIdentity 使可执行文件元数据与运行时身份解析器保持一致。
  let desktopNotificationIdentity = {
    name: 'Vulcan Code Desktop',
    appUserModelId: null,
    toastActivatorClsid: null,
  };
  // Preserve relative framework symlinks so a relocated macOS bundle does not point into node_modules.
  // 保留框架的相对符号链接，避免搬移后的 macOS 应用包仍指向 node_modules。
  await cp(path.join(desktop, 'node_modules/electron/dist'), path.join(output, 'desktop/runtime'), { recursive: true, verbatimSymlinks: true });
  // Give the shipped Windows executable its own file identity and product resources before
  // Electron registers the Start Menu shortcut used by native toast notifications.
  // 在 Electron 为原生通知注册开始菜单快捷方式前，为发行版 Windows 程序写入独立文件身份和产品资源。
  if (process.platform === 'win32') {
    // Keep debug and release executable identities separate for native toast registration.
    // 调试版与正式版使用不同可执行文件身份，以隔离原生通知注册。
    const desktopExecutableName = profile === 'release'
      ? 'VulcanCodeDesktop.exe' : 'VulcanCodeDesktopDebug.exe';
    const sourceExecutable = entry('desktop/runtime/electron.exe');
    const brandedExecutable = entry(`desktop/runtime/${desktopExecutableName}`);
    await rename(sourceExecutable, brandedExecutable);
    // identityModule is the compiled authoritative resolver used by the packaged Electron process.
    // identityModule 是打包 Electron 进程使用的已编译权威身份解析器。
    const identityModule = await import(pathToFileURL(
      path.join(desktop, 'dist/gui/notification-identity-values.js'),
    ).href);
    desktopNotificationIdentity = identityModule.resolveGuiNotificationIdentity(
      brandedExecutable,
      process.platform,
    );
    const { rcedit } = createRequire(path.join(desktop, 'package.json'))('rcedit');
    await rcedit(brandedExecutable, {
      'version-string': {
        ProductName: desktopNotificationIdentity.name,
        FileDescription: desktopNotificationIdentity.name,
        InternalName: profile === 'release' ? 'VulcanCodeDesktop' : 'VulcanCodeDesktopDebug',
        OriginalFilename: desktopExecutableName,
      },
      icon: path.join(repository, 'apps/vulcan-code-service/icons/icon.ico'),
    });
  }
  await cp(path.join(desktop, 'dist/gui'), path.join(appRoot, 'gui'), { recursive: true });
  // Sandboxed Electron needs this CommonJS entry beside gui/, even though the app uses ESM.
  // 即使应用使用 ESM，沙箱 Electron 仍需要位于 gui/ 同级目录的 CommonJS 预加载入口。
  await cp(path.join(desktop, 'dist/preload.cjs'), path.join(appRoot, 'preload.cjs'));
  await writeFile(path.join(appRoot, 'desktop-entry.js'), "import './gui/main.js';\n");
  // Browser code and dependencies live outside Electron's resources and cannot load its Chromium.
  // 浏览器代码与依赖位于 Electron 资源目录之外，不会加载其 Chromium。
  const browserRoot = path.join(output, 'browser/runtime');
  await mkdir(browserRoot, { recursive: true });
  await cp(path.join(desktop, 'dist/chromium-host'), path.join(browserRoot, 'chromium-host'), {
    recursive: true, filter: source => !source.endsWith('install.js') && !source.endsWith('.test.js'),
  });
  await mkdir(path.join(browserRoot, 'browser-host'), { recursive: true });
  for (const name of ['control-discovery.js', 'control-server.js', 'cdp-discovery.js', 'host-protocol.js']) {
    await cp(path.join(desktop, 'dist/browser-host', name), path.join(browserRoot, 'browser-host', name));
  }
  await cp(path.join(repository, 'plugins/vulcan-browser/node/node_modules/playwright-core'), path.join(browserRoot, 'node_modules/playwright-core'), { recursive: true, dereference: true });
  await cp(path.join(desktop, 'node_modules/ws'), path.join(browserRoot, 'node_modules/ws'), { recursive: true, dereference: true });
  await writeFile(path.join(browserRoot, 'package.json'), JSON.stringify({ private: true, type: 'module', browserVersions }));
  // The authenticated automation gateway uses the pinned WebSocket implementation at runtime.
  // 经鉴权的自动化网关在运行时使用锁定版本的 WebSocket 实现。
  await cp(path.join(desktop, 'node_modules/ws'), path.join(appRoot, 'node_modules/ws'), { recursive: true, dereference: true });
  // Keep the packaged JavaScript app identity aligned with the branded executable and no-argument entry.
  // 保持打包后的 JavaScript 应用身份与品牌化程序及无参数启动入口一致。
   const desktopPackage = JSON.parse(await readFile(path.join(desktop, 'package.json'), 'utf8'));
   await writeFile(path.join(appRoot, 'package.json'), JSON.stringify({
     name: desktopPackage.name,
     productName: desktopNotificationIdentity.name,
     version: desktopPackage.version,
     type: 'module',
     main: 'desktop-entry.js',
   }));
  await cp(path.join(repository, 'apps/vulcan-code-web/dist'), path.join(output, 'webui'), { recursive: true });
  // Resolve the installed platform executable without inventing a Windows-only suffix.
  // 按安装平台解析可执行文件，不使用仅适用于 Windows 的后缀。
  const electronBinary = process.platform === 'win32'
    ? (profile === 'release' ? 'VulcanCodeDesktop.exe' : 'VulcanCodeDesktopDebug.exe')
    : process.platform === 'darwin' ? 'Electron.app/Contents/MacOS/Electron' : 'electron';
  // Read actual runtime and service versions from their authoritative artifacts.
  // 从权威制品读取实际运行时和服务版本。
  const electronPackage = JSON.parse(await readFile(path.join(desktop, 'node_modules/electron/package.json'), 'utf8'));
  // The prepared source always contains the executable selected by the adapted Playwright version.
  // 已准备源码始终包含已适配 Playwright 版本选定的可执行程序。
  let chromiumExecutable;
  if (chromiumSource) {
    const { sourceRoot, executableRelative } = chromiumSource;
    await cp(sourceRoot, path.join(output, 'browser/chromium'), { recursive: true, dereference: true });
    chromiumExecutable = path.posix.join('browser/chromium', ...executableRelative.split(path.sep));
    await writeChromiumPackageManifest(path.join(output, 'browser/chromium'), entry(chromiumExecutable));
  }
  const layout = {
    schemaVersion: runtimeLayoutSchemaVersion, profile,
    serviceExecutable,
    desktopExecutable: `desktop/runtime/${electronBinary}`,
    desktopEntry: `${appEntry}/gui/main.js`, browserHostEntry: 'browser/runtime/chromium-host/main.js',
    browserHostExecutable: packageRelativePath(output, chromiumSource.executable),
    chromiumExecutable,
    webuiRoot: 'webui', runtimeVersion: electronPackage.version,
    platform: process.platform, architecture: process.arch,
    capabilities: chromiumExecutable ? ['desktop', 'browser-host', 'webui'] : ['desktop', 'webui'],
  };
  await verifyPackagedLayout(layout);
  await verifyServiceProfile(entry(layout.serviceExecutable));
  await writeFile(path.join(output, 'runtime-layout.json'), JSON.stringify(layout, null, 2) + '\n');
  console.log(`Desktop ${profile} distribution: ${output}`);
} else {
  // Only the generated layout chooses processes and static resources.
  // 进程和静态资源只由生成的布局清单确定。
  const manifestPath = path.join(output, 'runtime-layout.json');
  const layout = JSON.parse(await readFile(manifestPath, 'utf8'));
  if (layout.schemaVersion !== runtimeLayoutSchemaVersion || layout.profile !== profile
    || layout.platform !== process.platform || layout.architecture !== process.arch) {
    throw new Error('The built distribution does not match the requested profile, platform, or architecture. Rebuild first.');
  }
  // Core-only replacement needs the authoritative layout identity, without launching or repackaging other components.
  // 仅替换核心需要权威布局身份，无须启动或重新打包其他组件。
  if (command === 'layout') {
    process.stdout.write(JSON.stringify(layout));
    return;
  }
  await verifyPackagedLayout(layout);
  // The same absolute manifest reaches GUI, Core, and every Core-owned browser host.
  // 同一绝对清单传递给 GUI、Core 和每个 Core 拥有的浏览器宿主。
  const env = { ...process.env, VULCAN_RUNTIME_LAYOUT: manifestPath };
  await verifyServiceProfile(entry(layout.serviceExecutable));
  if (command === 'service') {
    await run(entry(layout.serviceExecutable), ['--service'], { env });
  } else {
    await run(entry(layout.desktopExecutable), [entry(layout.desktopEntry), `--service-executable=${entry(layout.serviceExecutable)}`], { env });
  }
  }
}

const isMainModule = process.argv[1]
  && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMainModule) await main();
