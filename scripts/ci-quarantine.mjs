// Read the CI quarantine list and turn it into nextest filtersets.
// 读取 CI 隔离清单，并转换为 nextest 过滤表达式。
//
// The required fast set excludes quarantined tests; nightly runs them on their own to collect data.
// Each entry names one exact test with its reason, date and owner, and leaves the list once fixed.
// 必过快速集排除被隔离的测试；夜间单独运行它们以收集数据。每项精确指定一个测试及其原因、日期和负责人，
// 修好后移出清单。
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// QUARANTINE_PATH is the single list shared by CI, nightly and local runs.
// QUARANTINE_PATH 是 CI、夜间与本地运行共用的唯一清单。
export const QUARANTINE_PATH = fileURLToPath(new URL('../.config/ci-quarantine.json', import.meta.url));

// NAME admits Cargo package names and Rust test paths, which never need filterset quoting.
// NAME 只接受 Cargo 包名和 Rust 测试路径，二者都无需在过滤表达式中转义。
const NAME = /^[A-Za-z0-9_:-]+$/;

/** Validate parsed quarantine content and return its entries.
 * 校验已解析的隔离清单内容并返回其条目。
 * @param {unknown} document Parsed JSON. 已解析的 JSON。
 * @returns {{package: string, test: string, reason: string, added: string, owner: string}[]} Entries. 条目。
 */
export function parseQuarantine(document) {
  if (document?.schema_version !== 1 || !Array.isArray(document.tests)) throw new Error('Unsupported CI quarantine schema.');
  const seen = new Set();
  for (const [index, entry] of document.tests.entries()) {
    const label = `CI quarantine entry ${index}`;
    if (Object.keys(entry).sort().join() !== 'added,owner,package,reason,test') throw new Error(`${label} must have exactly package, test, reason, added and owner.`);
    if (!NAME.test(entry.package) || !NAME.test(entry.test)) throw new Error(`${label} has an invalid package or test name.`);
    if (typeof entry.reason !== 'string' || !entry.reason.trim()) throw new Error(`${label} needs a reason.`);
    if (typeof entry.owner !== 'string' || !entry.owner.trim()) throw new Error(`${label} needs an owner.`);
    if (!/^\d{4}-\d{2}-\d{2}$/.test(entry.added) || Number.isNaN(Date.parse(entry.added))) throw new Error(`${label} needs an added date (YYYY-MM-DD).`);
    const key = `${entry.package} ${entry.test}`;
    if (seen.has(key)) throw new Error(`${label} duplicates ${key}.`);
    seen.add(key);
  }
  return document.tests;
}

/** Build the filterset that selects exactly the quarantined tests, or none() when the list is empty.
 * 构造精确选中被隔离测试的过滤表达式；清单为空时返回 none()。
 * @param {{package: string, test: string}[]} entries Validated entries. 已校验条目。
 * @returns {string} nextest filterset. nextest 过滤表达式。
 */
export function quarantinedFilter(entries) {
  if (entries.length === 0) return 'none()';
  return entries.map(entry => `(package(=${entry.package}) & test(=${entry.test}))`).join(' | ');
}

/** Read and validate the repository quarantine list.
 * 读取并校验仓库隔离清单。
 * @returns {object[]} Entries. 条目。
 */
export function loadQuarantine() {
  return parseQuarantine(JSON.parse(readFileSync(QUARANTINE_PATH, 'utf8')));
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  // Direct invocation prints the selection filter, for ad-hoc `cargo nextest run -E` use.
  // 直接执行时输出选择过滤表达式，供临时的 `cargo nextest run -E` 使用。
  console.log(quarantinedFilter(loadQuarantine()));
}
