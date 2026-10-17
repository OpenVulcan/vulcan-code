// Prepare the pinned shared Node runtime without starting Core or requiring an existing output.
// 无需启动 Core 或预先存在 output，即可准备锁定的共享 Node 运行时。
import { access, cp, mkdir, mkdtemp, readFile, rename, rm, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { buildInputPaths } from './build-input-paths.mjs';

// repository locates the existing authoritative runtime version manifest.
// repository 定位现有的权威运行时版本清单。
const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
// execute preserves native process errors and hides Windows helper windows.
// execute 保留原生进程错误并隐藏 Windows 辅助窗口。
const execute = promisify(execFile);
// managedNodeVersions shares the exact versions already used by the Core installer.
// managedNodeVersions 与 Core 安装器共用相同的精确版本。
export const managedNodeVersions = JSON.parse(await readFile(path.join(repository, 'crates/vulcan-service-runtime/managed-runtimes.json'), 'utf8'));

// Verify bytes against a published digest; algorithm and encoding describe the upstream checksum format.
// 按公开摘要校验字节；algorithm 与 encoding 描述上游校验格式。
export function verifyDownload(bytes, expected, algorithm, encoding) {
  if (createHash(algorithm).update(bytes).digest(encoding) !== expected) throw new Error(`Managed runtime ${algorithm} checksum mismatch.`);
}

// Fetch one official asset; url identifies the exact release and failures stop initialization.
// 获取一个官方资产；url 标识精确发布版本，失败会终止初始化。
async function download(url) {
  // response must succeed before its bytes can be trusted or extracted.
  // response 必须成功，随后才允许校验并解压其字节。
  const response = await fetch(url, { signal: AbortSignal.timeout(30 * 60 * 1000) });
  if (!response.ok) throw new Error(`Managed runtime download failed (${response.status}): ${url}`);
  return Buffer.from(await response.arrayBuffer());
}

// Validate an existing installation against identity; return false only when the directory is absent.
// 按 identity 验证已有安装；仅在目录不存在时返回 false。
async function installed(directory, identity) {
  try { await access(directory); } catch (error) {
    if (error.code === 'ENOENT') return false;
    throw error;
  }
  // manifest uses the same schema as the native managed runtime installer.
  // manifest 使用与原生受管运行时安装器相同的结构。
  const manifest = JSON.parse(await readFile(path.join(directory, 'runtime-manifest.json'), 'utf8'));
  for (const [key, value] of Object.entries(identity)) {
    if (manifest[key] !== value) throw new Error(`Invalid managed runtime ${directory}: ${key} must be ${value}.`);
  }
  await access(path.join(directory, identity.executable));
  return true;
}

// Publish a verified archive into an absent target; validate the binary before the atomic rename.
// 将已验证归档发布到尚不存在的 target；原子重命名前验证程序版本。
// archiveRoot names the official archive root; command produces the version probe for the staged root.
// archiveRoot 指定官方归档根目录；command 生成暂存根目录的版本探测命令。
async function publish(target, identity, bytes, archiveName, archiveRoot, source, command) {
  // staging is an exclusively owned sibling; cleanup never touches an existing runtime.
  // staging 是本次独占的同级临时目录；清理不会触碰已有运行时。
  const staging = await mkdtemp(path.join(path.dirname(target), '.prepare-'));
  try {
    // archive stores only the already verified official payload.
    // archive 仅存放已经验证的官方载荷。
    const archive = path.join(staging, archiveName);
    await writeFile(archive, bytes);
    await execute('tar', ['-xf', archive, '-C', staging], { windowsHide: true });
    // extracted is the exact documented root, never a candidate-path search.
    // extracted 是文档规定的精确根目录，绝不进行候选路径搜索。
    const extracted = path.join(staging, archiveRoot);
    // probe runs the staged binary before it becomes visible to Core.
    // probe 在暂存程序对 Core 可见前执行版本校验。
    const probe = command(extracted);
    const { stdout } = await execute(probe[0], probe.slice(1), { windowsHide: true });
    if (stdout.trim().replace(/^v/, '') !== identity.version) throw new Error(`Unexpected ${identity.runtime} version: ${stdout.trim()}`);
    await writeFile(path.join(extracted, 'runtime-manifest.json'), JSON.stringify({ ...identity, source }, null, 2));
    try { await rename(extracted, target); } catch (error) {
      // A concurrent initializer may win publication; accept only a validated identical runtime.
      // 并发初始化可能先完成发布；仅接受已验证且身份相同的运行时。
      if (!['EEXIST', 'ENOTEMPTY', 'EPERM'].includes(error.code) || !await installed(target, identity)) throw error;
    }
  } finally {
    await rm(staging, { recursive: true, force: true });
  }
}

// Copy one validated cached runtime from source to destination; identity must match before publication.
// 从 source 向 destination 复制一个已验证的缓存运行时；发布前 identity 必须匹配。
// Preserve an existing valid installation and return without touching its files.
// 保留已有有效安装，直接返回且不触碰其文件。
async function synchronizeRuntime(source, destination, identity) {
  if (await installed(destination, identity)) return;
  // parent and staging keep publication on the destination filesystem, isolated from persistent inputs.
  // parent 与 staging 使发布保持在目标文件系统内，并与持久输入隔离。
  const parent = path.dirname(destination);
  await mkdir(parent, { recursive: true });
  const staging = await mkdtemp(path.join(parent, '.sync-'));
  try {
    // prepared is a full independent copy; relative symlinks must not be rewritten to the cache.
    // prepared 是完整独立副本；相对符号链接不得被改写为指向缓存。
    const prepared = path.join(staging, 'runtime');
    await cp(source, prepared, { recursive: true, verbatimSymlinks: true });
    await installed(prepared, identity);
    try { await rename(prepared, destination); } catch (error) {
      if (!['EEXIST', 'ENOTEMPTY', 'EPERM'].includes(error.code) || !await installed(destination, identity)) throw error;
    }
  } finally {
    if (path.dirname(staging) !== parent || !path.basename(staging).startsWith('.sync-')) throw new Error('Invalid managed runtime staging boundary.');
    await rm(staging, { recursive: true, force: true });
  }
}

// Prepare verified Node/pnpm in repositoryRoot's third_party, then copy into distributionRoot.
// 在 repositoryRoot 的 third_party 中准备已验证 Node/pnpm，再复制至 distributionRoot。
// Return exact distribution launch paths; the optional repositoryRoot isolates offline test fixtures.
// 返回发行物的精确启动路径；可选 repositoryRoot 用于隔离离线测试夹具。
export async function prepareManagedNode(distributionRoot, repositoryRoot = repository) {
  // platform follows the Core manifest, while archivePlatform follows Node's official archive names.
  // platform 遵循 Core 清单，而 archivePlatform 遵循 Node 官方归档命名。
  const platformNames = { win32: 'windows', linux: 'linux', darwin: 'macos' };
  const archiveNames = { win32: 'win', linux: 'linux', darwin: 'darwin' };
  const platform = `${platformNames[process.platform]}-${process.arch}`;
  if (!managedNodeVersions.supported_platforms.includes(platform)) throw new Error(`Unsupported managed Node platform: ${platform}`);
  // nodeRoot persists across Cargo cleanup and output reconstruction; output is only a deployment copy.
  // nodeRoot 在 Cargo 清理及 output 重建后仍保留；output 仅是部署副本。
  const nodeRoot = buildInputPaths(repositoryRoot).managedNode;
  await mkdir(nodeRoot, { recursive: true });
  // identities reproduce the native installer's exact public manifest contract.
  // identities 复用原生安装器精确的公开清单契约。
  const nodeIdentity = { schema_version: 1, runtime: 'node', version: managedNodeVersions.node_version, platform, executable: process.platform === 'win32' ? 'node.exe' : 'bin/node' };
  const pnpmIdentity = { schema_version: 1, runtime: 'pnpm', version: managedNodeVersions.pnpm_version, platform: 'any', executable: 'bin/pnpm.cjs' };
  const nodeDirectory = path.join(nodeRoot, `node-${nodeIdentity.version}-${platform}`);
  const pnpmDirectory = path.join(nodeRoot, `pnpm-${pnpmIdentity.version}`);
  const executable = path.join(nodeDirectory, nodeIdentity.executable);
  if (!await installed(nodeDirectory, nodeIdentity)) {
    // asset and checksum are fetched from the same exact official Node release.
    // asset 与 checksum 从同一个精确的 Node 官方发布目录获取。
    const archiveRoot = `node-v${nodeIdentity.version}-${archiveNames[process.platform]}-${process.arch}`;
    const archiveExtensions = { win32: 'zip', linux: 'tar.xz', darwin: 'tar.gz' };
    const asset = `${archiveRoot}.${archiveExtensions[process.platform]}`;
    const base = `https://nodejs.org/dist/v${nodeIdentity.version}`;
    console.log(`Preparing managed Node ${nodeIdentity.version} (${platform})...`);
    const checksums = (await download(`${base}/SHASUMS256.txt`)).toString('utf8');
    const matches = checksums.split(/\r?\n/).map(line => line.trim().split(/\s+/)).filter(fields => fields[1] === asset);
    if (matches.length !== 1 || !/^[a-f0-9]{64}$/.test(matches[0][0])) throw new Error(`Missing unique Node checksum for ${asset}.`);
    const bytes = await download(`${base}/${asset}`);
    verifyDownload(bytes, matches[0][0], 'sha256', 'hex');
    await publish(nodeDirectory, nodeIdentity, bytes, asset, archiveRoot, `${base}/${asset}`, root => [path.join(root, nodeIdentity.executable), '--version']);
  }
  if (!await installed(pnpmDirectory, pnpmIdentity)) {
    // metadata selects an exact npm release and supplies its mandatory SHA-512 integrity.
    // metadata 选择精确 npm 发布版本，并提供强制 SHA-512 完整性校验值。
    console.log(`Preparing managed pnpm ${pnpmIdentity.version}...`);
    const metadata = JSON.parse((await download(`https://registry.npmjs.org/pnpm/${pnpmIdentity.version}`)).toString('utf8'));
    if (metadata.version !== pnpmIdentity.version || !metadata.dist.integrity.startsWith('sha512-')) throw new Error('Invalid pinned pnpm metadata.');
    const bytes = await download(metadata.dist.tarball);
    verifyDownload(bytes, metadata.dist.integrity.slice(7), 'sha512', 'base64');
    await publish(pnpmDirectory, pnpmIdentity, bytes, 'pnpm.tgz', 'package', metadata.dist.tarball, root => [executable, path.join(root, pnpmIdentity.executable), '--version']);
  }
  // distributionNode and distributionPnpm preserve the native installer's established runtime layout.
  // distributionNode 与 distributionPnpm 保持原生安装器既有的运行时布局。
  const distributionNode = path.resolve(distributionRoot, 'dependencies/runtimes/node', path.basename(nodeDirectory));
  const distributionPnpm = path.resolve(distributionRoot, 'dependencies/runtimes/node', path.basename(pnpmDirectory));
  await synchronizeRuntime(nodeDirectory, distributionNode, nodeIdentity);
  await synchronizeRuntime(pnpmDirectory, distributionPnpm, pnpmIdentity);
  return { executable: path.join(distributionNode, nodeIdentity.executable), pnpm: path.join(distributionPnpm, pnpmIdentity.executable) };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv.length !== 3) throw new Error('Usage: node scripts/managed-node.mjs <distribution-root>');
  await prepareManagedNode(path.resolve(process.argv[2]));
  console.log('Pinned managed Node and pnpm are ready.');
}
