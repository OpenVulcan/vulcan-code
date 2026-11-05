// Install reviewed build inputs without resolving mutable upstream release aliases.
// 安装已评审的构建输入，不解析上游可变发行别名。
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { access, chmod, cp, lstat, mkdir, mkdtemp, readFile, readdir, rename, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { obtainLockedInput } from './locked-download.mjs';
import { buildInputPaths } from './build-input-paths.mjs';

// resolveToolPlatform maps explicit OS/CPU inputs to archive identities; unsupported hosts throw.
// resolveToolPlatform 将明确的系统与处理器输入映射到归档身份；不支持的宿主抛出异常。
export function resolveToolPlatform(platform = process.platform, architecture = process.arch) {
  // cpu and system retain the existing distribution target naming contract.
  // cpu 与 system 保留既有发行目标命名契约。
  const cpu = { x64: 'x86_64', arm64: 'aarch64' }[architecture];
  // system selects the upstream operating-system triple.
  // system 选择上游操作系统三元组。
  const system = { win32: 'pc-windows-msvc', linux: 'unknown-linux-gnu', darwin: 'apple-darwin' }[platform];
  if (!cpu || !system) throw new Error(`Unsupported build host: ${platform}/${architecture}`);
  return {
    target: `${cpu}-${system}`,
    suffix: platform === 'win32' ? '.exe' : '',
    archiveKind: platform === 'win32' ? 'zip' : 'tar.gz',
    rgTarget: platform === 'linux' ? `${cpu}-unknown-linux-musl` : `${cpu}-${system}`,
    protoc: platform === 'win32' ? 'win64' : platform === 'darwin' ? 'osx-universal_binary' : architecture === 'arm64' ? 'linux-aarch_64' : 'linux-x86_64',
  };
}

// extractArchive expands a verified archive into destination and throws on extractor failure.
// extractArchive 将已校验归档展开到 destination，解压器失败时抛出异常。
function extractArchive(archive, destination) {
  if (archive.endsWith('.zip') && process.platform === 'win32') {
    execFileSync('pwsh', ['-NoProfile', '-Command', 'Expand-Archive -LiteralPath $env:VULCAN_ARCHIVE -DestinationPath $env:VULCAN_EXTRACT -Force'], {
      env: { ...process.env, VULCAN_ARCHIVE: archive, VULCAN_EXTRACT: destination }, stdio: 'inherit',
    });
  } else {
    execFileSync(archive.endsWith('.zip') ? 'unzip' : 'tar',
      archive.endsWith('.zip') ? ['-q', archive, '-d', destination] : ['-xzf', archive, '-C', destination], { stdio: 'inherit' });
  }
}

// locateBinary returns the unique archive member with name; ambiguity or absence fails explicitly.
// locateBinary 返回归档中名称为 name 的唯一成员；歧义或缺失均明确失败。
async function locateBinary(directory, name) {
  // matches contains only regular files; archive symlinks are not executable candidates.
  // matches 只包含普通文件；归档符号链接不作为可执行文件候选。
  const matches = [];
  // entry retains the exact parent directory returned by the filesystem walk.
  // entry 保留文件系统遍历返回的精确父目录。
  for (const entry of await readdir(directory, { withFileTypes: true, recursive: true })) {
    if (entry.isFile() && entry.name === name) matches.push(path.join(entry.parentPath, entry.name));
  }
  if (matches.length !== 1) throw new Error(`Expected one ${name}, found ${matches.length} in ${directory}`);
  return matches[0];
}

// pathExists distinguishes an absent path from permission or filesystem errors.
// pathExists 区分路径缺失与权限或文件系统错误。
async function pathExists(file) {
  try { await access(file); return true; }
  catch (error) { if (error.code === 'ENOENT') return false; throw error; }
}

// expectedVersionLine returns the version line tool `id` at `version` must print; it includes the tool name so
// unrelated executables cannot pass a version substring check.
// expectedVersionLine 返回工具 `id` 在 `version` 时必须输出的版本行；其中包含工具名，避免无关程序通过版本子串检查。
function expectedVersionLine(id, version) {
  return id === 'protoc' ? `libprotoc ${version}` : id === 'nextest' ? `cargo-nextest ${version}`
    : id === 'sg' ? `ast-grep ${version}` : id === 'rtk' ? `rtk ${version.replace(/^v/, '')}` : `ripgrep ${version}`;
}

// versionLine reduces the `--version` output of tool `id` to its first line without documented build metadata
// (ripgrep's revision suffix, nextest's build details), keeping the complete version token.
// versionLine 将工具 `id` 的 `--version` 输出化简为去除规定构建元数据（ripgrep 的修订后缀、nextest 的构建
// 详情）后的首行，同时保留完整版本标记。
function versionLine(id, reported) {
  const firstLine = reported.split(/\r?\n/)[0];
  return id === 'rg' ? firstLine.replace(/ \(rev [a-f0-9]+\)$/, '')
    : id === 'nextest' ? firstLine.split(' ').slice(0, 2).join(' ') : firstLine;
}

// installedCurrent reports whether `directory` already holds tool `id` at `version`: its executable runs and
// prints the locked version, protoc keeps its include files, and a bundled tool's record names the same locked
// archive (`record` holds its tag, asset and sha256).
// installedCurrent 判断 `directory` 是否已包含版本为 `version` 的工具 `id`：其可执行文件可运行并输出锁定版本，
// protoc 保留其 include 文件，随附工具的记录指向同一锁定归档（`record` 含其 tag、asset 与 sha256）。
async function installedCurrent(directory, id, version, installedName, record) {
  const binary = path.join(directory, 'bin', installedName);
  if (!await pathExists(binary)) return false;
  if (id === 'protoc' && !await pathExists(path.join(directory, 'include', 'google', 'protobuf', 'descriptor.proto'))) return false;
  if (record) {
    const file = path.join(directory, `${id}.json`);
    if (!await pathExists(file)) return false;
    const installed = JSON.parse(await readFile(file, 'utf8'));
    if (installed.schema_version !== 2 || installed.tag !== record.tag || installed.asset !== record.asset
      || installed.sha256 !== record.sha256 || !await pathExists(path.join(directory, 'licenses', id))) return false;
  }
  try {
    return versionLine(id, execFileSync(binary, ['--version'], { encoding: 'utf8' }).trim()) === expectedVersionLine(id, version);
  } catch {
    return false;
  }
}

// publishInstallation replaces destination only after preparation; failures restore the previous directory.
// publishInstallation 仅在准备完成后替换 destination；失败时恢复上一版目录。
async function publishInstallation(prepared, destination) {
  // backup is deterministic so an interrupted rename can be recovered on the next invocation.
  // backup 使用确定名称，便于下次调用恢复被中断的重命名。
  const backup = `${destination}.previous`;
  if (await pathExists(backup)) throw new Error(`Unresolved previous installation: ${backup}`);
  // hadPrevious records whether rollback must restore an existing installation.
  // hadPrevious 记录回滚是否必须恢复既有安装。
  const hadPrevious = await pathExists(destination);
  if (hadPrevious) await rename(destination, backup);
  try { await rename(prepared, destination); }
  catch (error) {
    if (hadPrevious) await rename(backup, destination);
    throw error;
  }
  if (hadPrevious) await rm(backup, { recursive: true });
}

// JUDGE_RUNTIME_MANIFEST names the manifest the service reads to locate the bundled judge runtime.
// JUDGE_RUNTIME_MANIFEST 是服务读取以定位随包判定执行库的清单文件名。
export const JUDGE_RUNTIME_MANIFEST = 'judge-runtime.json';

// bundledToolIds returns the bundled tool ids the lock declares for one target, in installation order.
// bundledToolIds 返回锁文件为某个目标声明的随包工具标识，按安装顺序排列。
//
// Parameters: lock is the parsed build-input lock and target is a resolveToolPlatform target triple.
// 参数：lock 是已解析的构建输入锁文件，target 是 resolveToolPlatform 给出的目标三元组。
// Returns: sg and rg always; rtk only where its lock names an archive or a source build for the host.
// 返回值：始终包含 sg 与 rg；rtk 仅在其锁文件为该宿主声明归档或源码构建时包含。
export function bundledToolIds(lock, target) {
  const rtk = lock.tools.rtk;
  return ['sg', 'rg', ...(rtk.targets[target] || rtk.source_targets.includes(target) ? ['rtk'] : [])];
}

// pinnedToolchain returns the repository's Rust channel so source-built tools use the same compiler as the product.
// pinnedToolchain 返回仓库固定的 Rust 频道，使源码构建的工具与产品使用同一编译器。
async function pinnedToolchain() {
  const toml = await readFile(new URL('../rust-toolchain.toml', import.meta.url), 'utf8');
  const channel = toml.match(/^channel\s*=\s*"([^"]+)"/m)?.[1];
  if (!channel) throw new Error('rust-toolchain.toml declares no channel.');
  return channel;
}

// buildLockedSource compiles the single source tree in extracted with the pinned toolchain and returns the release binary.
// Building on the release host links against that host's C library, so the result runs wherever the product runs.
// buildLockedSource 用固定工具链编译 extracted 中唯一的源码树并返回 release 二进制。
// 在发行宿主上构建会链接该宿主的 C 库，因此产物可在产品能运行的所有系统上运行。
async function buildLockedSource(extracted, source, binaryName, targetDirectory, offline) {
  const roots = await readdir(extracted, { withFileTypes: true });
  if (roots.length !== 1 || !roots[0].isDirectory()) throw new Error(`Expected one source directory in ${extracted}`);
  const sourceRoot = path.join(extracted, roots[0].name);
  execFileSync('cargo', [
    'build', '--release', '--locked', ...(offline ? ['--offline'] : []), ...source.build_args, '--target-dir', targetDirectory,
  ], { cwd: sourceRoot, env: { ...process.env, RUSTUP_TOOLCHAIN: await pinnedToolchain() }, stdio: 'inherit' });
  return path.join(targetDirectory, 'release', binaryName);
}

// prepareJudgeRuntime copies only the declared members of each verified archive into prepared and writes the manifest.
// prepareJudgeRuntime 只把每个已校验归档中声明的成员复制到 prepared，并写入清单。
async function prepareJudgeRuntime(runtime, target, temporary, prepared, cache, offline) {
  // files records the digest of every installed file for runtime integrity checks.
  // files 记录每个已安装文件的摘要，供运行时完整性检查。
  const files = {};
  for (const [index, archive] of target.archives.entries()) {
    // local keeps a .zip suffix for wheels because the Windows extractor accepts only that extension.
    // local 为 wheel 保留 .zip 后缀，因为 Windows 解压器只接受该扩展名。
    const local = path.join(temporary, archive.name.replace(/\.whl$/, '.zip'));
    await writeFile(local, await obtainLockedInput(archive, cache, offline));
    // extracted isolates this archive from the others.
    // extracted 将此归档与其他归档隔离。
    const extracted = path.join(temporary, `judge-runtime-${index}`);
    await mkdir(extracted);
    extractArchive(local, extracted);
    for (const [member, destination] of Object.entries(archive.files)) {
      // source must be a regular file; links and absent members fail instead of being followed or skipped.
      // source 必须是普通文件；链接与缺失成员直接失败，不跟随也不跳过。
      const source = path.join(extracted, ...member.split('/'));
      if (!(await lstat(source)).isFile()) throw new Error(`Locked member is not a regular file: ${member}`);
      // installed is the declared relative destination inside the prepared runtime.
      // installed 是 prepared 执行库内声明的相对目标路径。
      const installed = path.join(prepared, ...destination.split('/'));
      await mkdir(path.dirname(installed), { recursive: true });
      await cp(source, installed);
      files[destination] = createHash('sha256').update(await readFile(installed)).digest('hex');
    }
  }
  for (const library of [target.core_library, target.webgpu_library]) {
    if (!files[library]) throw new Error(`Judge runtime library is not among the locked files: ${library}`);
  }
  await writeFile(path.join(prepared, JUDGE_RUNTIME_MANIFEST), JSON.stringify({
    schema_version: 1, onnxruntime: runtime.onnxruntime, webgpu_plugin: runtime.webgpu_plugin,
    core_library: target.core_library, webgpu_library: target.webgpu_library, files,
  }, null, 2) + '\n');
}

// verifyJudgeRuntime returns the installed judge runtime of this host after checking it against the lock,
// or null when the lock declares no runtime for the host; a declared but missing or altered runtime throws.
// verifyJudgeRuntime 按锁文件校验本宿主已安装的判定执行库后返回其目录；锁文件未为本宿主声明执行库时返回 null，
// 已声明但缺失或被改动的执行库直接抛出异常。
export async function verifyJudgeRuntime(repositoryRoot) {
  // lock is the authority the installed manifest must reproduce exactly.
  // lock 是已安装清单必须精确复现的权威。
  const lock = JSON.parse(await readFile(new URL('./build-inputs.lock.json', import.meta.url), 'utf8'));
  if (lock.schema_version !== 1) throw new Error('Unsupported build input lock schema.');
  const platform = resolveToolPlatform();
  const target = lock.judge_runtime.targets[platform.target];
  if (!target) return null;
  const directory = path.join(path.resolve(repositoryRoot), 'third_party', 'judge-runtime', platform.target);
  // missing tells the developer which command restores the declared runtime.
  // missing 告知开发者用哪个命令恢复已声明的执行库。
  const missing = `The judge runtime for ${platform.target} is missing or outdated; run make deps tools before synchronization.`;
  if (!await pathExists(path.join(directory, JUDGE_RUNTIME_MANIFEST))) throw new Error(missing);
  const manifest = JSON.parse(await readFile(path.join(directory, JUDGE_RUNTIME_MANIFEST), 'utf8'));
  // expected lists every destination the lock declares for this host.
  // expected 列出锁文件为本宿主声明的全部目标路径。
  const expected = target.archives.flatMap(archive => Object.values(archive.files)).sort();
  if (manifest.schema_version !== 1 || manifest.onnxruntime !== lock.judge_runtime.onnxruntime
    || manifest.webgpu_plugin !== lock.judge_runtime.webgpu_plugin || manifest.core_library !== target.core_library
    || manifest.webgpu_library !== target.webgpu_library
    || JSON.stringify(Object.keys(manifest.files).sort()) !== JSON.stringify(expected)) throw new Error(missing);
  for (const [file, digest] of Object.entries(manifest.files)) {
    // installed must still be the regular file whose digest the installer recorded.
    // installed 必须仍是安装器记录摘要时的那个普通文件。
    const installed = path.join(directory, ...file.split('/'));
    if (!await pathExists(installed) || !(await lstat(installed)).isFile()
      || createHash('sha256').update(await readFile(installed)).digest('hex') !== digest) throw new Error(missing);
  }
  return directory;
}

// installLockedTools installs group into repositoryRoot using verified cache bytes; offline prohibits downloads.
// installLockedTools 使用已验证缓存字节将 group 安装到 repositoryRoot；offline 禁止下载。
export async function installLockedTools(group, repositoryRoot, offline = false) {
  if (!['bundled', 'protoc', 'nextest', 'judge-runtime'].includes(group)) throw new Error('Expected bundled, protoc, nextest or judge-runtime.');
  if (group === 'nextest' && (!['win32', 'linux'].includes(process.platform) || process.arch !== 'x64')) throw new Error('The CI nextest input supports Windows and Linux x64 only.');
  // lock is the sole version/digest authority shared by every host installer.
  // lock 是所有宿主安装器共享的唯一版本与摘要权威。
  const lock = JSON.parse(await readFile(new URL('./build-inputs.lock.json', import.meta.url), 'utf8'));
  if (lock.schema_version !== 1) throw new Error('Unsupported build input lock schema.');
  // platform identifies the host archive set.
  // platform 标识宿主归档集合。
  const platform = resolveToolPlatform();
  // judgeTarget is the declared judge runtime of this host; hosts without an official build install nothing.
  // judgeTarget 是本宿主声明的判定执行库；没有官方构建的宿主不安装任何内容。
  const judgeTarget = group === 'judge-runtime' ? lock.judge_runtime.targets[platform.target] : undefined;
  if (group === 'judge-runtime' && !judgeTarget) {
    console.log(`No official judge runtime is declared for ${platform.target}; the local judge stays unavailable on this host.`);
    return;
  }
  // root is the explicit installation boundary.
  // root 是明确的安装边界。
  const root = path.resolve(repositoryRoot);
  // cache stores bytes keyed by their reviewed digest.
  // cache 按已评审摘要保存字节。
  const cache = buildInputPaths(root).lockedDownloads;
  // destination is the established installation path consumed by Cargo or resource sync.
  // destination 是 Cargo 或资源同步使用的既有安装路径。
  const destination = group === 'protoc' ? path.join(root, '.cargo', 'protoc')
    : group === 'nextest' ? path.join(root, 'target', 'ci-tools')
    : group === 'judge-runtime' ? path.join(root, 'third_party', 'judge-runtime', platform.target)
    : path.join(root, 'third_party', 'tools', platform.target);
  // parent keeps staging and destination on the same filesystem for rename.
  // parent 使暂存与目标位于同一文件系统，支持重命名。
  const parent = path.dirname(destination);
  await mkdir(parent, { recursive: true });
  // guard serializes installations for the same destination without touching another process's files.
  // guard 将同一目标的安装串行化，不触碰其他进程的文件。
  const guard = `${destination}.installing`;
  await mkdir(guard);
  // temporary is assigned only after this invocation acquires the installation guard.
  // temporary 仅在本次调用获得安装保护后赋值。
  let temporary;
  try {
    // Restore a interrupted old-directory move before preparing the next transaction.
    // 准备下次事务前恢复被中断的旧目录移动。
    if (!await pathExists(destination) && await pathExists(`${destination}.previous`)) await rename(`${destination}.previous`, destination);
    temporary = await mkdtemp(path.join(parent, '.vulcan-install-'));
    // prepared contains the complete replacement before any old file is moved.
    // prepared 在移动任何旧文件之前包含完整替代安装。
    const prepared = path.join(temporary, 'prepared');
    await mkdir(prepared);
    // A judge runtime that still matches the lock file by file is kept as it is.
    // 逐文件仍与锁文件一致的判定执行库保持不变。
    if (judgeTarget && await verifyJudgeRuntime(root).then(Boolean, () => false)) {
      console.log(`Locked judge runtime for ${platform.target} is already installed: ${destination}`);
      return;
    }
    // The tools of the current installation start the replacement; each one still current is kept below.
    // 以当前安装中的工具作为替代安装的起点；仍为当前版本的工具在下面保留。
    if (!judgeTarget && await pathExists(destination)) await cp(destination, prepared, { recursive: true });
    // entries identifies exactly the requested tool set; tools are validated before any installation switch.
    // entries 明确请求的工具集合；所有工具均在安装切换之前验证。
    const entries = group === 'bundled' ? bundledToolIds(lock, platform.target) : group === 'judge-runtime' ? [] : [group];
    // changed records whether anything was installed, so an unchanged installation is not swapped.
    // changed 记录是否安装了任何内容，使未变化的安装不被替换。
    let changed = false;
    if (judgeTarget) {
      await prepareJudgeRuntime(lock.judge_runtime, judgeTarget, temporary, prepared, cache, offline);
      changed = true;
    }
    // id selects the exact declared tool contract.
    // id 选择明确声明的工具契约。
    for (const id of entries) {
      // version comes exclusively from the reviewed manifest.
      // version 仅来自已评审清单。
      const version = id === 'protoc' || id === 'nextest' ? lock[id].version : lock.tools[id].tag_name;
      // sourceBuild selects the reviewed source archive where the upstream binary cannot run on the supported floor.
      // sourceBuild 在上游二进制无法在受支持的最低系统上运行时，改用已评审的源码归档。
      const sourceBuild = id === 'rtk' && lock.tools.rtk.source_targets.includes(platform.target);
      // archiveName follows the upstream tool-specific platform naming contract.
      // archiveName 遵循各上游工具的平台命名契约。
      const archiveName = id === 'protoc' ? `protoc-${version}-${platform.protoc}.zip`
        : id === 'nextest' ? `cargo-nextest-${version}-${platform.target}.${platform.archiveKind}`
        : id === 'sg' ? `app-${platform.target}.zip`
        : sourceBuild ? `rtk-${version}-source.tar.gz`
        : id === 'rtk' ? lock.tools.rtk.targets[platform.target]
        : `ripgrep-${version}-${platform.rgTarget}.${platform.archiveKind}`;
      // identity is the {url, sha256} the shared downloader verifies, normalized from the explicit lock shapes.
      // identity 是共用下载器校验的 {url, sha256}，由各明确的锁结构规范化而来。
      let identity = sourceBuild ? lock.tools.rtk.source : undefined;
      if (!identity) {
        // assets is the declared archive list for this tool.
        // assets 是此工具声明的归档列表。
        const assets = id === 'protoc' || id === 'nextest' ? lock[id].assets : lock.tools[id].assets;
        // matches rejects missing and duplicated identities instead of selecting a fallback.
        // matches 拒绝缺失和重复身份，不选择替代项。
        const matches = assets.filter(asset => asset.name === archiveName);
        if (matches.length !== 1) throw new Error(`Expected one locked archive: ${archiveName}`);
        const asset = matches[0];
        identity = id === 'protoc' || id === 'nextest' ? asset : { url: asset.browser_download_url, sha256: asset.digest.replace(/^sha256:/, '') };
      }
      // sourceName preserves the upstream ast-grep executable name before installing the sg alias.
      // sourceName 在安装 sg 别名之前保留上游 ast-grep 可执行文件名。
      const sourceName = `${id === 'sg' ? 'ast-grep' : id === 'nextest' ? 'cargo-nextest' : id}${platform.suffix}`;
      const installedName = id === 'nextest' ? sourceName : `${id}${platform.suffix}`;
      // A tool already installed from this exact locked archive is kept: a kept installation (a self-hosted
      // runner keeps third_party/ and target/) is neither extracted nor rebuilt until the lock changes.
      // 已由同一锁定归档安装的工具保持不变：保留下来的安装（自托管运行器保留 third_party/ 与 target/）在锁文件
      // 变化之前既不重新解压也不重新构建。
      const record = id === 'protoc' || id === 'nextest' ? null : { tag: version, asset: archiveName, sha256: identity.sha256 };
      if (await installedCurrent(prepared, id, version, installedName, record)) {
        console.log(`Locked ${id} ${version} is already installed.`);
        continue;
      }
      // archive holds only digest-verified bytes.
      // archive 仅保存摘要已验证的字节。
      const archive = path.join(temporary, archiveName);
      await writeFile(archive, await obtainLockedInput(identity, cache, offline));
      // extracted isolates the current archive from other tools.
      // extracted 将当前归档与其他工具隔离。
      const extracted = path.join(temporary, id);
      await mkdir(extracted);
      extractArchive(archive, extracted);
      // binary is the unique regular executable member.
      // binary 是唯一的普通可执行文件成员。
      const binary = sourceBuild
        ? await buildLockedSource(extracted, lock.tools.rtk.source, sourceName, path.join(temporary, `${id}-target`), offline)
        : await locateBinary(extracted, sourceName);
      if (process.platform !== 'win32') await chmod(binary, 0o755);
      // reported captures a real execution check before replacing the old installation.
      // reported 在替换旧安装之前记录真实执行检查结果。
      const reported = execFileSync(binary, ['--version'], { encoding: 'utf8' }).trim();
      if (versionLine(id, reported) !== expectedVersionLine(id, version)) throw new Error(`Unexpected ${id} version: ${reported}`);
      await mkdir(path.join(prepared, 'bin'), { recursive: true });
      await cp(binary, path.join(prepared, 'bin', installedName));
      changed = true;
      if (id === 'protoc') {
        await access(path.join(extracted, 'include', 'google', 'protobuf', 'descriptor.proto'));
        await cp(path.join(extracted, 'include'), path.join(prepared, 'include'), { recursive: true });
      } else if (id !== 'nextest') {
        // licenseRoot is the established sync-compatible license directory.
        // licenseRoot 是兼容既有同步流程的许可证目录。
        const licenseRoot = path.join(prepared, 'licenses', id);
        await mkdir(licenseRoot, { recursive: true });
        // license supplies its own reviewed content digest.
        // license 提供自身已评审的内容摘要。
        for (const license of lock.tools[id].licenses) await writeFile(path.join(licenseRoot, license.name), await obtainLockedInput(license, cache, offline));
        await writeFile(path.join(prepared, `${id}.json`), JSON.stringify({
          schema_version: 2, tool: id, repository: lock.tools[id].repository, tag: version,
          asset: archiveName, source_binary: sourceName, sha256: identity.sha256,
        }, null, 2) + '\n');
      }
    }
    if (!changed) {
      console.log(`Locked ${group} is already current: ${destination}`);
      return;
    }
    await publishInstallation(prepared, destination);
    console.log(`Verified locked ${group} installed: ${destination}`);
  } finally {
    // Only remove the unique child allocated under this destination's parent.
    // 仅移除在此目标父目录下分配的唯一子目录。
    if (temporary && path.dirname(temporary) === parent && path.basename(temporary).startsWith('.vulcan-install-')) await rm(temporary, { recursive: true, force: true });
    await rm(guard, { recursive: true });
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  // arguments selects the installation group and optional isolated repository root.
  // arguments 选择安装组及可选的隔离仓库根目录。
  const [group, ...argumentsList] = process.argv.slice(2);
  // verify-judge-runtime prints the verified runtime directory, or nothing for an undeclared host, for the sync scripts.
  // verify-judge-runtime 为同步脚本打印已校验的执行库目录；未声明的宿主不打印任何内容。
  const verifying = group === 'verify-judge-runtime';
  // bundled-tool-ids prints the space-separated tools the sync scripts must find for this host.
  // bundled-tool-ids 打印同步脚本在本宿主上必须找到的工具，以空格分隔。
  if (group === 'bundled-tool-ids') {
    const lock = JSON.parse(await readFile(new URL('./build-inputs.lock.json', import.meta.url), 'utf8'));
    console.log(bundledToolIds(lock, resolveToolPlatform().target).join(' '));
    process.exit(0);
  }
  // root defaults to the checkout containing this installer.
  // root 默认使用包含此安装器的检出目录。
  let root = path.resolve(fileURLToPath(new URL('..', import.meta.url)));
  // offline becomes true only through the explicit command-line switch.
  // offline 仅通过显式命令行开关设为真。
  let offline = false;
  // index consumes each argument once, including the repository-root value.
  // index 将每个参数消费一次，包括仓库根目录值。
  for (let index = 0; index < argumentsList.length; index += 1) {
    if (argumentsList[index] === '--offline') offline = true;
    else if (argumentsList[index] === '--repository-root' && argumentsList[index + 1]) root = path.resolve(argumentsList[++index]);
    else throw new Error(`Unknown installer argument: ${argumentsList[index]}`);
  }
  if (verifying) {
    const directory = await verifyJudgeRuntime(root);
    if (directory) console.log(directory);
  } else await installLockedTools(group, root, offline);
}
