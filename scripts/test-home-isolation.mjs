// Fail when a workspace crate whose tests reach vulcan-runtime-assets does not enable its
// `test-support` feature as a dev-dependency; without it that crate's tests resolve the real user
// home and can overwrite the user's configuration.
// 当测试会触及 vulcan-runtime-assets 的工作区 crate 未在开发依赖中启用其 `test-support` 特性时失败；
// 缺少该特性时，该 crate 的测试会解析真实用户主目录，并可能覆盖用户配置。
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

// Repository root keeps invocation independent of the caller's working directory.
// 仓库根目录确保入口不依赖调用者当前工作目录。
const root = fileURLToPath(new URL('..', import.meta.url));
const TARGET = 'vulcan-runtime-assets';

const metadata = spawnSync('cargo', ['metadata', '--format-version', '1', '--no-deps', '--locked'], {
  cwd: root,
  encoding: 'utf8',
  maxBuffer: 64 * 1024 * 1024,
});
if (metadata.error) throw metadata.error;
if (metadata.status !== 0) {
  process.stderr.write(metadata.stderr);
  process.exit(1);
}
const packages = new Map(JSON.parse(metadata.stdout).packages.map((pkg) => [pkg.name, pkg]));

// normalDependencies lists the workspace crates one member depends on outside tests.
// normalDependencies 列出一个成员在测试之外依赖的工作区 crate。
const normalDependencies = (pkg) =>
  pkg.dependencies.filter((dep) => dep.kind === null && packages.has(dep.name)).map((dep) => dep.name);

const reachesTarget = new Map();
function reaches(name, visiting = new Set()) {
  if (reachesTarget.has(name)) return reachesTarget.get(name);
  if (visiting.has(name)) return false;
  visiting.add(name);
  const result = normalDependencies(packages.get(name)).some((dep) => dep === TARGET || reaches(dep, visiting));
  reachesTarget.set(name, result);
  return result;
}

const missing = [...packages.values()]
  .filter((pkg) => pkg.name !== TARGET && reaches(pkg.name))
  .filter((pkg) => !pkg.dependencies.some(
    (dep) => dep.name === TARGET && dep.kind === 'dev' && dep.features.includes('test-support'),
  ))
  .map((pkg) => pkg.name)
  .sort();

if (missing.length > 0) {
  console.error('These crates reach vulcan-runtime-assets but their tests would use the real user home.');
  console.error('Add to [dev-dependencies]: vulcan-runtime-assets = { workspace = true, features = ["test-support"] }');
  for (const name of missing) console.error(`  - ${name}`);
  process.exit(1);
}
console.log('Every crate that reaches vulcan-runtime-assets tests with a private user home.');
