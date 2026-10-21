// Prepare the `gn_root` directory junctions that the v8 crate's Windows build script needs when the
// code-mode host target directory and the Cargo registry live on different drives.
// 当代码模式宿主目标目录与 Cargo 注册表位于不同盘符时，为 v8 crate 的 Windows 构建脚本准备 `gn_root`
// 目录联接。
//
// The v8 build script resolves its bundled source binding through `<target>/<profile>/gn_root`. Across
// drives it creates that entry with `symlink_dir`, which needs Developer Mode or elevation and otherwise
// panics with "Failed to create symlink". A directory junction needs no privilege, and the build script
// accepts any existing entry whose canonical path is the crate root, so this script creates junctions
// ahead of Cargo. It is idempotent, replaces junctions left by an older v8 version, and does nothing on
// other platforms or when both directories share a drive.
// v8 构建脚本通过 `<target>/<profile>/gn_root` 定位随 crate 提供的源绑定。跨盘符时它用 `symlink_dir`
// 创建该入口，需要开发者模式或管理员权限，否则以 "Failed to create symlink" 崩溃。目录联接不需要权限，
// 构建脚本也接受规范路径等于 crate 根目录的既有入口，因此本脚本在 Cargo 之前创建联接。脚本幂等，
// 会替换旧版 v8 留下的联接；在其他平台或两者同盘时不做任何事。
import { spawnSync } from 'node:child_process';
import { lstatSync, mkdirSync, realpathSync, rmSync, symlinkSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// PROFILES lists the Cargo profile directories whose builds link V8.
// PROFILES 列出会链接 V8 的 Cargo 模式目录。
const PROFILES = ['debug', 'release'];

// hostWorkspace is the standalone code-mode host workspace, the only workspace that depends on v8.
// hostWorkspace 是独立的代码模式宿主工作区，也是唯一依赖 v8 的工作区。
const hostWorkspace = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', 'native', 'code-mode-host');

// Return Cargo metadata for the host workspace. Running inside the workspace applies its .cargo
// configuration (notably the target directory) and resolves the lockfile and crate sources.
// 返回宿主工作区的 Cargo 元数据。在工作区内运行会应用其 .cargo 配置（尤其是目标目录），并解析锁文件与
// crate 源码。
function hostMetadata() {
  const result = spawnSync('cargo', ['metadata', '--format-version', '1'], {
    cwd: hostWorkspace,
    encoding: 'utf8',
    maxBuffer: 256 * 1024 * 1024,
    windowsHide: true,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`cargo metadata failed in ${hostWorkspace} (exit ${result.status}): ${result.stderr}`);
  return JSON.parse(result.stdout);
}

// Return the drive prefix of an absolute Windows path in upper case.
// 返回 Windows 绝对路径的大写盘符前缀。
function driveOf(target) {
  return path.parse(path.resolve(target)).root.toUpperCase();
}

// Ensure `<profileDirectory>/gn_root` is a junction to `crateRoot`; returns the action taken.
// 确保 `<profileDirectory>/gn_root` 是指向 `crateRoot` 的联接；返回所执行的动作。
function ensureJunction(profileDirectory, crateRoot) {
  const link = path.join(profileDirectory, 'gn_root');
  const expected = realpathSync.native(crateRoot);
  let existing = null;
  try {
    existing = lstatSync(link);
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
  }
  if (existing) {
    let resolved = null;
    try {
      resolved = realpathSync.native(link);
    } catch {
      // A dangling junction from a removed v8 version cannot be resolved and is replaced below.
      // 已移除 v8 版本留下的悬空联接无法解析，下方会替换它。
    }
    if (resolved && resolved.toLowerCase() === expected.toLowerCase()) return 'kept';
    if (!existing.isSymbolicLink()) {
      throw new Error(`${link} exists and is not a junction; remove it and rerun.`);
    }
    // Removing a junction deletes only the link, never the registry sources it points to.
    // 删除联接只删除链接本身，绝不删除其指向的注册表源码。
    rmSync(link, { force: true });
  }
  mkdirSync(profileDirectory, { recursive: true });
  symlinkSync(expected, link, 'junction');
  return 'created';
}

// Prepare every profile directory; returns per-profile actions for logging.
// 为每个模式目录做准备；返回各模式的动作，供日志使用。
export function prepareV8BuildRoot() {
  if (process.platform !== 'win32') return [];
  const metadata = hostMetadata();
  const v8 = metadata.packages.find((pkg) => pkg.name === 'v8' && pkg.source?.startsWith('registry+'));
  if (!v8) throw new Error(`The code-mode host workspace no longer depends on the v8 crate; update ${fileURLToPath(import.meta.url)}.`);
  const crateRoot = path.dirname(v8.manifest_path);
  const targetDirectory = metadata.target_directory;
  if (driveOf(targetDirectory) === driveOf(crateRoot)) return [];
  return PROFILES.map((profile) => ({
    profile,
    action: ensureJunction(path.join(targetDirectory, profile), crateRoot),
  }));
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  for (const { profile, action } of prepareV8BuildRoot()) {
    if (action === 'created') console.log(`==> Created v8 gn_root junction for the ${profile} profile.`);
  }
}
