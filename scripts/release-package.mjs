// Package one verified release distribution into a platform archive plus the checksums installers trust.
// 将一份已验证的发行物打包为平台归档，并生成安装脚本信任的校验和。
import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { copyFile, mkdir, readdir, readFile, rename, rm, writeFile } from 'node:fs/promises';
import { execFile, execFileSync } from 'node:child_process';
import { promisify } from 'node:util';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { managedRuntimePlatformKey, validateRuntimeLayoutIdentity, verifyPackagedLayout } from './desktop-runtime.mjs';

const execFileAsync = promisify(execFile);
// The repository is resolved from this script rather than the caller's working directory.
// 仓库由脚本位置确定，不依赖调用方工作目录。
const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

// checksumFileName is the one index install.sh downloads before choosing and verifying an archive.
// checksumFileName 是 install.sh 在选择并校验归档前下载的唯一索引。
export const checksumFileName = 'SHA256SUMS';
// installerFileName is published beside the archives so `curl .../install.sh | sh` finds them by relative name.
// installerFileName 与归档一同发布，使 `curl .../install.sh | sh` 能按相对名称找到归档。
export const installerFileName = 'install.sh';
// releaseAssetPattern matches every archive this script names: version, platform key, then format.
// releaseAssetPattern 匹配本脚本命名的全部归档：版本、平台键与格式。
const releaseAssetPattern = /^vulcan-code-(\d+\.\d+\.\d+(?:-[0-9A-Za-z.]+)?)-([a-z]+-[a-z0-9]+)\.(?:tar\.gz|zip)$/;

// releaseAssetName derives the archive name from a runtime layout and the built service version.
// releaseAssetName 根据运行时布局与已构建服务版本推导归档名称。
// Windows ships a zip for Explorer; Unix ships tar.gz, which keeps symlinks and executable bits.
// Windows 提供资源管理器可直接打开的 zip；Unix 提供保留符号链接与可执行位的 tar.gz。
export function releaseAssetName(layout, version) {
  const name = `vulcan-code-${version}-${managedRuntimePlatformKey(layout)}.${layout.platform === 'win32' ? 'zip' : 'tar.gz'}`;
  if (!releaseAssetPattern.test(name)) throw new Error(`Invalid release version: ${version}.`);
  return name;
}

// archiveTool selects Windows' own bsdtar explicitly, because a GNU tar from Git can shadow it and cannot write zip.
// archiveTool 显式选择 Windows 自带的 bsdtar，因为 Git 附带的 GNU tar 可能遮蔽它且无法写入 zip。
function archiveTool() {
  return process.platform === 'win32' ? path.join(process.env.SystemRoot ?? 'C:\\Windows', 'System32', 'tar.exe') : 'tar';
}

// sha256File streams one file so multi-gigabyte archives never load into memory.
// sha256File 流式读取文件，避免数 GB 归档整体载入内存。
async function sha256File(file) {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(file)) hash.update(chunk);
  return hash.digest('hex');
}

// writeChecksums rewrites the index over every archive and installer in root; mixed versions are rejected
// so an installer can never face two candidates for one platform.
// writeChecksums 基于 root 中全部归档与安装脚本重写索引；拒绝混合版本，使安装脚本不会面对同一平台的两个候选。
export async function writeChecksums(root) {
  const names = (await readdir(root)).filter(name => releaseAssetPattern.test(name) || name === installerFileName).sort();
  const versions = new Set(names.filter(name => name !== installerFileName).map(name => releaseAssetPattern.exec(name)[1]));
  if (versions.size > 1) throw new Error(`One release directory must hold one version; found ${[...versions].join(', ')}.`);
  if (versions.size === 0) throw new Error(`No release archive was found in ${root}.`);
  // Two spaces keep the file readable by `sha256sum -c` and `shasum -a 256 -c`.
  // 两个空格使文件可直接被 `sha256sum -c` 与 `shasum -a 256 -c` 读取。
  const lines = [];
  for (const name of names) lines.push(`${await sha256File(path.join(root, name))}  ${name}`);
  const index = path.join(root, checksumFileName);
  await writeFile(`${index}.partial`, lines.join('\n') + '\n');
  await rename(`${index}.partial`, index);
  return names;
}

// createReleaseArchive archives the distribution's top-level entries (no wrapping directory) and refreshes the index.
// createReleaseArchive 归档发行物的顶层条目（不额外包一层目录），并刷新索引。
// distributionRoot must hold a release runtime-layout.json; the finished archive path is returned.
// distributionRoot 必须包含 release 运行时布局；返回完成的归档路径。
export async function createReleaseArchive(distributionRoot, outputRoot, version) {
  const layout = JSON.parse(await readFile(path.join(distributionRoot, 'runtime-layout.json'), 'utf8'));
  if (layout.profile !== 'release') throw new Error('Only a release distribution can be packaged.');
  const name = releaseAssetName(layout, version);
  await mkdir(outputRoot, { recursive: true });
  const destination = path.join(outputRoot, name);
  // A partial name keeps an interrupted run from leaving a plausible but truncated archive.
  // 部分文件名确保中断时不会留下看似完整却被截断的归档。
  const partial = `${destination}.partial`;
  const format = name.endsWith('.zip') ? ['--format', 'zip'] : ['-z'];
  try {
    await execFileAsync(archiveTool(), [...format, '-cf', partial, '-C', distributionRoot, ...(await readdir(distributionRoot)).sort()], {
      windowsHide: true, maxBuffer: 16 * 1024 * 1024,
    });
    await rename(partial, destination);
  } finally {
    await rm(partial, { force: true });
  }
  await writeChecksums(outputRoot);
  return destination;
}

// requiredGlibc reports the newest GLIBC symbol version the given ELF files reference, or null when none do.
// requiredGlibc 返回给定 ELF 文件引用的最新 GLIBC 符号版本；均未引用时返回 null。
export async function requiredGlibc(files) {
  let newest = null;
  for (const file of files) {
    for (const match of (await readFile(file)).toString('latin1').matchAll(/GLIBC_(\d+)\.(\d+)/g)) {
      const version = [Number(match[1]), Number(match[2])];
      if (!newest || version[0] > newest[0] || (version[0] === newest[0] && version[1] > newest[1])) newest = version;
    }
  }
  return newest && newest.join('.');
}

// packageCurrentDistribution verifies the live distribution exactly as a launch would, then archives it.
// packageCurrentDistribution 按启动时的标准验证当前发行物，再将其归档。
async function packageCurrentDistribution(outputOverride) {
  // Only the release distribution is ever archived, and it lives in its own root beside the debug one.
  // 只归档正式版发行物，它位于与调试版并列的独立目录。
  const root = execFileSync(process.execPath, [path.join(repository, 'scripts/desktop-runtime.mjs'), 'root', 'release'], { encoding: 'utf8' }).trim();
  const layout = JSON.parse(await readFile(path.join(root, 'runtime-layout.json'), 'utf8'));
  validateRuntimeLayoutIdentity(layout, 'release');
  await verifyPackagedLayout(layout, root);
  // The version comes from the built service itself, so a stale layout cannot mislabel the archive.
  // 版本取自已构建服务本身，过期布局无法给归档贴错标签。
  const service = path.join(root, layout.serviceExecutable);
  const description = JSON.parse((await execFileAsync(service, ['--describe'], { windowsHide: true, timeout: 20_000 })).stdout);
  if (description.profile !== 'release') throw new Error(`The service in ${root} is a ${description.profile} build.`);
  const outputRoot = outputOverride ?? path.join(repository, 'target', 'release-packages', description.serviceVersion);
  await mkdir(outputRoot, { recursive: true });
  await copyFile(path.join(repository, 'scripts', installerFileName), path.join(outputRoot, installerFileName));
  const archive = await createReleaseArchive(root, outputRoot, description.serviceVersion);
  console.log(`Release archive: ${archive}`);
  console.log(`Checksums: ${path.join(outputRoot, checksumFileName)}`);
  if (layout.platform === 'linux') {
    // Installers on older distributions fail their smoke run below this floor; publish it with the release.
    // 低于此基线的旧发行版会在安装脚本的试运行中失败；需随发行说明一同公布。
    const binaries = (await readdir(path.join(root, 'bin'))).map(name => path.join(root, 'bin', name));
    console.log(`Shipped binaries require glibc >= ${await requiredGlibc(binaries) ?? 'unknown'}.`);
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [command, ...rest] = process.argv.slice(2);
  if (command === 'package' && (rest.length === 0 || (rest.length === 2 && rest[0] === '--out'))) {
    await packageCurrentDistribution(rest[1] && path.resolve(rest[1]));
  } else if (command === 'checksums' && rest.length === 1) {
    // Re-index after archives built on other machines are collected into one directory.
    // 将其他机器构建的归档汇集到同一目录后重新生成索引。
    console.log((await writeChecksums(path.resolve(rest[0]))).join('\n'));
  } else {
    throw new Error('Usage: node scripts/release-package.mjs package [--out <dir>] | checksums <dir>');
  }
}
