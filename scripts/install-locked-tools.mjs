// Install reviewed build inputs without resolving mutable upstream release aliases.
// 安装已评审的构建输入，不解析上游可变发行别名。
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { access, chmod, cp, lstat, mkdir, mkdtemp, readFile, readdir, rename, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { obtainLockedInput } from './locked-download.mjs';

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
  if (group === 'nextest' && (process.platform !== 'win32' || process.arch !== 'x64')) throw new Error('The CI nextest input supports Windows x64 only.');
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
  const cache = path.join(root, 'target', 'build-input-cache');
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
    if (group === 'bundled' && await pathExists(destination)) await cp(destination, prepared, { recursive: true });
    // entries identifies exactly the requested tool set; tools are validated before any installation switch.
    // entries 明确请求的工具集合；所有工具均在安装切换之前验证。
    const entries = group === 'bundled' ? ['sg', 'rg'] : group === 'judge-runtime' ? [] : [group];
    if (judgeTarget) await prepareJudgeRuntime(lock.judge_runtime, judgeTarget, temporary, prepared, cache, offline);
    // id selects the exact declared tool contract.
    // id 选择明确声明的工具契约。
    for (const id of entries) {
      // version comes exclusively from the reviewed manifest.
      // version 仅来自已评审清单。
      const version = id === 'protoc' || id === 'nextest' ? lock[id].version : lock.tools[id].tag_name;
      // archiveName follows the upstream tool-specific platform naming contract.
      // archiveName 遵循各上游工具的平台命名契约。
      const archiveName = id === 'protoc' ? `protoc-${version}-${platform.protoc}.zip`
        : id === 'nextest' ? `cargo-nextest-${version}-${platform.target}.zip`
        : id === 'sg' ? `app-${platform.target}.zip` : `ripgrep-${version}-${platform.rgTarget}.${platform.archiveKind}`;
      // assets is the declared archive list for this tool.
      // assets 是此工具声明的归档列表。
      const assets = id === 'protoc' || id === 'nextest' ? lock[id].assets : lock.tools[id].assets;
      // matches rejects missing and duplicated identities instead of selecting a fallback.
      // matches 拒绝缺失和重复身份，不选择替代项。
      const matches = assets.filter(asset => asset.name === archiveName);
      if (matches.length !== 1) throw new Error(`Expected one locked archive: ${archiveName}`);
      // asset is the unique manifest member.
      // asset 是清单中的唯一匹配成员。
      const asset = matches[0];
      // identity normalizes the two explicit lock shapes for the shared downloader.
      // identity 将两种明确的锁结构规范化后交给共用下载器。
      const identity = id === 'protoc' || id === 'nextest' ? asset : { url: asset.browser_download_url, sha256: asset.digest.replace(/^sha256:/, '') };
      // archive holds only digest-verified bytes.
      // archive 仅保存摘要已验证的字节。
      const archive = path.join(temporary, archiveName);
      await writeFile(archive, await obtainLockedInput(identity, cache, offline));
      // extracted isolates the current archive from other tools.
      // extracted 将当前归档与其他工具隔离。
      const extracted = path.join(temporary, id);
      await mkdir(extracted);
      extractArchive(archive, extracted);
      // sourceName preserves the upstream ast-grep executable name before installing the sg alias.
      // sourceName 在安装 sg 别名之前保留上游 ast-grep 可执行文件名。
      const sourceName = `${id === 'sg' ? 'ast-grep' : id === 'nextest' ? 'cargo-nextest' : id}${platform.suffix}`;
      // binary is the unique regular executable member.
      // binary 是唯一的普通可执行文件成员。
      const binary = await locateBinary(extracted, sourceName);
      if (process.platform !== 'win32') await chmod(binary, 0o755);
      // reported captures a real execution check before replacing the old installation.
      // reported 在替换旧安装之前记录真实执行检查结果。
      const reported = execFileSync(binary, ['--version'], { encoding: 'utf8' }).trim();
      // expected includes the tool name so unrelated executables cannot pass a version substring check.
      // expected 包含工具名，避免无关程序通过版本子串检查。
      const expected = id === 'protoc' ? `libprotoc ${version}` : id === 'nextest' ? `cargo-nextest ${version}`
        : id === 'sg' ? `ast-grep ${version}` : `ripgrep ${version}`;
      // ripgrep includes a revision suffix in its first line; compare the full version token, not a substring.
      // ripgrep 首行包含修订后缀；比较完整版本标记，而非子串。
      const firstLine = reported.split(/\r?\n/)[0];
      // versionLine removes documented build metadata while preserving the complete version token.
      // versionLine 去除规定的构建元数据，同时保留完整版本标记。
      const versionLine = id === 'rg' ? firstLine.replace(/ \(rev [a-f0-9]+\)$/, '')
        : id === 'nextest' ? firstLine.split(' ').slice(0, 2).join(' ') : firstLine;
      if (versionLine !== expected) throw new Error(`Unexpected ${id} version: ${reported}`);
      await mkdir(path.join(prepared, 'bin'), { recursive: true });
      await cp(binary, path.join(prepared, 'bin', id === 'nextest' ? sourceName : `${id}${platform.suffix}`));
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
