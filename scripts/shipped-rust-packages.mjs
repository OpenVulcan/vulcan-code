// Share the shipped Cargo package and artifact inventory with Unix builds and publication.
// 与 Unix 构建和发布流程共用分发 Cargo 包及制品清单。
import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

// shippedRustPackages is also consumed by the PowerShell build and third-party notice inventory.
// shippedRustPackages 同时供 PowerShell 构建和第三方声明清单使用。
// Each item names either an executable (`binary`) or a C dynamic library (`library`) shipped in `bin`.
// 每项要么指名一个可执行文件（`binary`），要么指名一个 C 动态库（`library`），均分发在 `bin` 中。
// `platforms` limits an item to the listed Node platforms; `isolated` builds it in its own Cargo invocation.
// `platforms` 把某项限定在所列 Node 平台；`isolated` 使其在独立的 Cargo 调用中构建。
const inventory = JSON.parse(readFileSync(new URL('./shipped-rust-packages.json', import.meta.url), 'utf8'));

// shippedRustPackagesFor returns the items shipped on platform.
// shippedRustPackagesFor 返回在 platform 上分发的清单项。
export function shippedRustPackagesFor(platform) {
  return inventory.filter(item => !item.platforms || item.platforms.includes(platform));
}

// shippedRustPackages holds the items shipped on the current platform.
// shippedRustPackages 保存在当前平台上分发的清单项。
export const shippedRustPackages = shippedRustPackagesFor(process.platform);

// shippedFileName returns the Cargo output file name of one inventory item on platform.
// shippedFileName 返回某清单项在 platform 上的 Cargo 输出文件名。
export function shippedFileName(item, platform = process.platform) {
  if (item.binary) return item.binary + (platform === 'win32' ? '.exe' : '');
  if (platform === 'win32') return `${item.library}.dll`;
  return `lib${item.library}${platform === 'darwin' ? '.dylib' : '.so'}`;
}

// shippedArtifactNames returns every required file name for platform; package/target aliases come from Cargo declarations.
// shippedArtifactNames 返回 platform 对应的全部必需文件名；包与目标别名来自 Cargo 声明。
export function shippedArtifactNames(platform = process.platform) {
  return shippedRustPackagesFor(platform).map(item => shippedFileName(item, platform));
}

// shippedCargoArgs returns `-p` selectors for one kind (`binary`, `library` or `isolated`) on platform; each
// kind builds in its own Cargo invocation, and `isolated` items never join the other two.
// shippedCargoArgs 返回 platform 上某一类（`binary`、`library` 或 `isolated`）的 `-p` 选择参数；每类在独立的
// Cargo 调用中构建，`isolated` 项从不加入另外两类。
// The service is outside Hakari traversal, so building a library beside it would compile LanceDB with the
// service's private dependency features, a second variant distinct from every other library build.
// 服务不在 Hakari 遍历范围内，与其同批构建库会以服务私有的依赖特性编译 LanceDB，产生与其他库构建不同的第二个变体。
// The Windows launcher changes only with an install epoch, so features unified from other packages must not
// change its bytes either.
// Windows 启动器只随安装纪元变化，因此其他包统一进来的特性同样不得改变其字节。
export function shippedCargoArgs(kind, platform = process.platform) {
  const selected = shippedRustPackagesFor(platform).filter(item => shippedKindOf(item) === kind);
  return [...new Set(selected.map(item => item.package))].flatMap(name => ['-p', name]);
}

// shippedKindOf returns the Cargo invocation (`binary`, `library` or `isolated`) that builds an item.
// shippedKindOf 返回构建某清单项的 Cargo 调用（`binary`、`library` 或 `isolated`）。
export function shippedKindOf(item) {
  if (item.isolated === true) return 'isolated';
  return item.binary ? 'binary' : 'library';
}

// prebuiltKinds reads `VULCAN_PREBUILT_KINDS`: the invocations the release workflow restored from a published
// release instead of building (release-reuse.mjs), their files already in the Cargo profile directory.
// prebuiltKinds 读取 `VULCAN_PREBUILT_KINDS`：发布工作流从已发布版本还原而无需构建的调用（release-reuse.mjs），
// 其文件已位于 Cargo 配置目录中。
export function prebuiltKinds(value = process.env.VULCAN_PREBUILT_KINDS ?? '') {
  const kinds = value.split(' ').filter(Boolean);
  for (const kind of kinds) {
    if (!['binary', 'library', 'isolated'].includes(kind)) throw new Error(`VULCAN_PREBUILT_KINDS names unknown kind ${kind}`);
  }
  return kinds;
}

// prebuiltArtifacts describes the restored files of `kinds` in `directory` the way Cargo's compiler-artifact
// messages do, so the Windows build takes them like built ones; a missing file fails here.
// prebuiltArtifacts 以 Cargo compiler-artifact 消息的形式描述 `directory` 中 `kinds` 的已还原文件，使 Windows
// 构建像对待已构建文件一样使用它们；文件缺失时在此失败。
export function prebuiltArtifacts(kinds, directory, platform = process.platform) {
  return shippedRustPackagesFor(platform).filter(item => kinds.includes(shippedKindOf(item))).map(item => {
    const file = path.join(directory, shippedFileName(item, platform));
    if (!existsSync(file)) throw new Error(`the restored ${path.basename(file)} is missing from ${directory}`);
    return item.binary
      ? { target: { name: item.binary, kind: ['bin'] }, executable: file, filenames: [file] }
      : { target: { name: item.library, kind: ['cdylib'] }, executable: null, filenames: [file] };
  });
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [command, ...rest] = process.argv.slice(2);
  if (command === 'cargo-args' && rest.length === 1 && ['binary', 'library', 'isolated'].includes(rest[0])) {
    process.stdout.write(shippedCargoArgs(rest[0]).join(' '));
  } else if (command === 'file-name' && rest.length === 1) {
    // Shell builds resolve a package's output name here instead of repeating the platform rules.
    // Shell 构建在此解析包的输出文件名，而不重复平台规则。
    const item = shippedRustPackages.find(candidate => candidate.package === rest[0]);
    if (!item) throw new Error(`Package ${rest[0]} is not shipped.`);
    process.stdout.write(shippedFileName(item));
  } else if (command === 'prebuilt-kinds' && rest.length === 0) {
    process.stdout.write(prebuiltKinds().join(' '));
  } else if (command === 'prebuilt-artifacts' && rest.length === 1) {
    process.stdout.write(JSON.stringify(prebuiltArtifacts(prebuiltKinds(), rest[0])));
  } else {
    throw new Error('Usage: shipped-rust-packages cargo-args binary|library|isolated | file-name <package> | prebuilt-kinds | prebuilt-artifacts <dir>');
  }
}
