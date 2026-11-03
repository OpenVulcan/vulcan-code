// Check the release notes shipped as resources: the shape the service, the TUI and the WebUI read.
// 校验作为资源发行的更新记录：即服务、TUI 与 WebUI 读取的形状。
//
// `resources/changelog/releases.json` lists releases newest first with strictly decreasing `x.y.z`
// versions and a `YYYY-MM-DD` date, or `null` for the newest release while it is unreleased. The
// newest release equals the version in `resources/product.json`. Each release owns
// `<version>/<code>.md` for exactly the languages in `catalogs/locales.json`, none empty; stray
// directories or files are rejected. Component fences are checked by the WebUI and TUI renderers' tests.
// `resources/changelog/releases.json` 按从新到旧列出发布，版本为严格递减的 `x.y.z`，日期为
// `YYYY-MM-DD`；仅最新发布在未发布时可为 `null`。最新发布等于 `resources/product.json` 中的版本。
// 每个发布持有与 `catalogs/locales.json` 语言完全一致且非空的 `<version>/<code>.md`；多余的目录或
// 文件一律拒绝。组件围栏由 WebUI 与 TUI 渲染器的测试检查。
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// Repository root keeps the check independent of the caller's working directory.
// 仓库根目录使检查不依赖调用者当前工作目录。
const root = fileURLToPath(new URL('..', import.meta.url));

// Authoritative resource locations, synchronized into every distribution by `make sync`.
// 权威资源位置，由 `make sync` 同步进每个发行物。
export const RESOURCES_DIR = path.join(root, 'crates', 'vulcan-runtime-assets', 'resources');
export const LOCALES_PATH = path.join(root, 'crates', 'vulcan-i18n', 'catalogs', 'locales.json');

// parseVersion returns [major, minor, patch] for an exact `x.y.z`, or null.
// parseVersion 对精确的 `x.y.z` 返回 [major, minor, patch]，否则返回 null。
export function parseVersion(version) {
  const match = /^(\d+)\.(\d+)\.(\d+)$/.exec(typeof version === 'string' ? version : '');
  return match ? match.slice(1).map(Number) : null;
}

// compareVersions orders two parsed versions numerically.
// compareVersions 按数值比较两个已解析版本。
function compareVersions(left, right) {
  for (let index = 0; index < 3; index += 1) {
    if (left[index] !== right[index]) return left[index] - right[index];
  }
  return 0;
}

// isDate accepts `YYYY-MM-DD` with a plausible month and day.
// isDate 接受月、日取值合理的 `YYYY-MM-DD`。
function isDate(value) {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(typeof value === 'string' ? value : '');
  if (!match) return false;
  const month = Number(match[2]);
  const day = Number(match[3]);
  return month >= 1 && month <= 12 && day >= 1 && day <= 31;
}

// checkReleaseNotes returns every problem of `changelogDir` against `codes` and `productVersion`.
// checkReleaseNotes 返回 `changelogDir` 相对 `codes` 与 `productVersion` 的全部问题。
export function checkReleaseNotes(changelogDir, codes, productVersion) {
  const problems = [];
  const indexPath = path.join(changelogDir, 'releases.json');
  let releases;
  try {
    releases = JSON.parse(readFileSync(indexPath, 'utf8')).releases;
  } catch (error) {
    return [`${indexPath}: ${error.message}`];
  }
  if (!Array.isArray(releases) || releases.length === 0) {
    return ['changelog/releases.json: `releases` must be a nonempty array'];
  }
  if (releases[0]?.version !== productVersion) {
    problems.push(`changelog: the newest release \`${releases[0]?.version}\` must equal product.json \`${productVersion}\``);
  }
  const expected = new Set(codes.map((code) => `${code}.md`));
  const listed = new Set();
  let previous = null;
  releases.forEach((release, position) => {
    const keys = Object.keys(release ?? {}).filter((key) => key !== 'version' && key !== 'date');
    if (keys.length > 0) problems.push(`changelog: unexpected fields ${keys.join(', ')}`);
    const parsed = parseVersion(release?.version);
    if (!parsed) {
      problems.push(`changelog: \`${release?.version}\` is not an \`x.y.z\` version`);
      return;
    }
    if (previous && compareVersions(parsed, previous) >= 0) {
      problems.push(`changelog: \`${release.version}\` must be older than the release above it`);
    }
    previous = parsed;
    listed.add(release.version);
    if (!(release.date === null && position === 0) && !isDate(release.date)) {
      problems.push(`changelog: \`${release.version}\` needs a YYYY-MM-DD \`date\`; only the newest release may be null`);
    }
    const directory = path.join(changelogDir, release.version);
    if (!existsSync(directory) || !statSync(directory).isDirectory()) {
      problems.push(`${directory}: missing`);
      return;
    }
    const present = new Set(readdirSync(directory));
    const missing = [...expected].filter((name) => !present.has(name));
    const unexpected = [...present].filter((name) => !expected.has(name));
    if (missing.length > 0 || unexpected.length > 0) {
      problems.push(`${directory}: needs exactly one Markdown file per registered language; missing [${missing.join(', ')}], unexpected [${unexpected.join(', ')}]`);
    }
    for (const name of [...expected].filter((file) => present.has(file))) {
      if (readFileSync(path.join(directory, name), 'utf8').trim() === '') {
        problems.push(`${path.join(directory, name)}: notes must not be empty`);
      }
    }
  });
  for (const name of readdirSync(changelogDir)) {
    if (name !== 'releases.json' && !listed.has(name)) {
      problems.push(`changelog/${name} is not listed in releases.json`);
    }
  }
  return problems;
}

// readProductVersion returns the version of one `product.json`, rejecting anything but an exact version.
// readProductVersion 返回一个 `product.json` 的版本，非精确版本一律拒绝。
export function readProductVersion(productPath) {
  const { version } = JSON.parse(readFileSync(productPath, 'utf8'));
  if (!parseVersion(version)) throw new Error(`${productPath}: \`${version}\` is not an exact x.y.z version`);
  return version;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) {
  const codes = JSON.parse(readFileSync(LOCALES_PATH, 'utf8')).locales.map((locale) => locale.code);
  const version = readProductVersion(path.join(RESOURCES_DIR, 'product.json'));
  const problems = checkReleaseNotes(path.join(RESOURCES_DIR, 'changelog'), codes, version);
  if (problems.length > 0) {
    console.error(`Release notes are invalid:\n${problems.map((problem) => `  - ${problem}`).join('\n')}`);
    process.exit(1);
  }
  console.log(`Release notes cover every language; the newest release is ${version}.`);
}
