// Share the shipped Cargo package and artifact inventory with Unix builds and publication.
// 与 Unix 构建和发布流程共用分发 Cargo 包及制品清单。
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

// shippedRustPackages is also consumed by the PowerShell build and third-party notice inventory.
// shippedRustPackages 同时供 PowerShell 构建和第三方声明清单使用。
// Each item names either an executable (`binary`) or a C dynamic library (`library`) shipped in `bin`.
// 每项要么指名一个可执行文件（`binary`），要么指名一个 C 动态库（`library`），均分发在 `bin` 中。
export const shippedRustPackages = JSON.parse(readFileSync(new URL('./shipped-rust-packages.json', import.meta.url), 'utf8'));

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
  return shippedRustPackages.map(item => shippedFileName(item, platform));
}

// shippedCargoArgs returns `-p` selectors for one kind; libraries build in their own Cargo invocation.
// shippedCargoArgs 返回某一类的 `-p` 选择参数；库在独立的 Cargo 调用中构建。
// The service is outside Hakari traversal, so building a library beside it would compile LanceDB with the
// service's private dependency features, a second variant distinct from every other library build.
// 服务不在 Hakari 遍历范围内，与其同批构建库会以服务私有的依赖特性编译 LanceDB，产生与其他库构建不同的第二个变体。
export function shippedCargoArgs(kind) {
  return shippedRustPackages.filter(item => kind in item).flatMap(item => ['-p', item.package]);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [command, ...rest] = process.argv.slice(2);
  if (command === 'cargo-args' && rest.length === 1 && ['binary', 'library'].includes(rest[0])) {
    process.stdout.write(shippedCargoArgs(rest[0]).join(' '));
  } else if (command === 'file-name' && rest.length === 1) {
    // Shell builds resolve a package's output name here instead of repeating the platform rules.
    // Shell 构建在此解析包的输出文件名，而不重复平台规则。
    const item = shippedRustPackages.find(candidate => candidate.package === rest[0]);
    if (!item) throw new Error(`Package ${rest[0]} is not shipped.`);
    process.stdout.write(shippedFileName(item));
  } else {
    throw new Error('Usage: shipped-rust-packages cargo-args binary|library | file-name <package>');
  }
}
