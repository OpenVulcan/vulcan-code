// Fail when a macOS release tree carries a Mach-O file without the platform's architecture. An x64 release is built
// on an Apple silicon Mac under Rosetta, where tools that sniff the hardware instead of the process (Playwright,
// Electron's installer) pick arm64 binaries. npm packages named for one platform (for example
// `@reflink/reflink-darwin-arm64`) are installed for every platform and chosen at run time, so they are skipped.
// 当 macOS 发行目录树中有不含该平台架构的 Mach-O 文件时失败。x64 发行物在 Apple 芯片 Mac 的 Rosetta 下构建，
// 按硬件而非进程判断的工具（Playwright、Electron 安装器）会选择 arm64 程序。以单一平台命名的 npm 包（例如
// `@reflink/reflink-darwin-arm64`）为所有平台安装并在运行时选择，因此跳过。
//
//   node scripts/check-binary-architecture.mjs <tree> <macos-x64|macos-arm64>
import { open, readdir } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// CPU_TYPES maps each macOS platform to its Mach-O CPU type.
// CPU_TYPES 将每个 macOS 平台映射到其 Mach-O CPU 类型。
const CPU_TYPES = { 'macos-x64': 0x01000007, 'macos-arm64': 0x0100000c };

// machOArchitectures returns the CPU types of a Mach-O or universal file from its header, or null for any other file.
// machOArchitectures 从文件头返回 Mach-O 或通用文件的 CPU 类型；其他文件返回 null。
export function machOArchitectures(header) {
  if (header.length < 8) return null;
  const magic = header.readUInt32BE(0);
  if (magic === 0xcffaedfe || magic === 0xcefaedfe) return [header.readInt32LE(4)];
  if (magic === 0xfeedfacf || magic === 0xfeedface) return [header.readInt32BE(4)];
  if (magic === 0xcafebabe || magic === 0xcafebabf) {
    // Java class files share 0xcafebabe; their second word is a version far above any plausible architecture count.
    // Java class 文件同样以 0xcafebabe 开头；其第二个字是版本号，远大于任何合理的架构数量。
    const count = header.readUInt32BE(4);
    if (count === 0 || count > 16) return null;
    const entry = magic === 0xcafebabf ? 32 : 20;
    const types = [];
    for (let index = 0; index < count && 8 + (index + 1) * entry <= header.length; index += 1) {
      types.push(header.readInt32BE(8 + index * entry));
    }
    return types;
  }
  return null;
}

// platformPackage reports whether a path segment names an npm package built for one darwin architecture.
// platformPackage 判断路径段是否命名了为单一 darwin 架构构建的 npm 包。
function platformPackage(relative) {
  return relative.split(path.sep).some(segment => /(^|-)darwin-(x64|arm64|universal)$/.test(segment));
}

// mismatches lists the tree's Mach-O files that lack cpuType, relative to root.
// mismatches 列出树中不含 cpuType 的 Mach-O 文件（相对于 root）。
export async function mismatches(root, cpuType) {
  const found = [];
  for (const entry of await readdir(root, { recursive: true, withFileTypes: true })) {
    if (!entry.isFile()) continue;
    const file = path.join(entry.parentPath, entry.name);
    const relative = path.relative(root, file);
    if (platformPackage(relative)) continue;
    const handle = await open(file, 'r');
    let header;
    try {
      const buffer = Buffer.alloc(8 + 16 * 32);
      const { bytesRead } = await handle.read(buffer, 0, buffer.length, 0);
      header = buffer.subarray(0, bytesRead);
    } finally {
      await handle.close();
    }
    const types = machOArchitectures(header);
    if (types && !types.includes(cpuType)) found.push(relative);
  }
  return found;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [root, platform] = process.argv.slice(2);
  const cpuType = CPU_TYPES[platform];
  if (!root || cpuType === undefined) throw new Error('Usage: check-binary-architecture <tree> <macos-x64|macos-arm64>');
  const wrong = await mismatches(path.resolve(root), cpuType);
  if (wrong.length) {
    console.error(`${wrong.length} Mach-O file(s) lack the ${platform} architecture, for example:\n${wrong.slice(0, 20).join('\n')}`);
    process.exit(1);
  }
  console.log(`Every Mach-O file carries the ${platform} architecture.`);
}
