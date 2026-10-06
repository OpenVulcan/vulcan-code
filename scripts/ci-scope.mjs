// Classify complete Git changes and validate the required job contract without third-party dependencies.
// 无第三方依赖地分类完整 Git 变更，并验证必需作业契约。
import { execFileSync } from 'node:child_process';
import { appendFileSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// JOBS is the shared identity authority for selection and required-result validation.
// JOBS 是选择作业与验证必需结果共用的身份权威。
export const JOBS = ['scripts', 'frontend', 'desktop', 'rustfmt', 'rust'];

/** Select jobs for repository-relative files; full explicitly selects every gate.
 * 根据仓库相对文件路径选择作业；full 显式选择全部检查。
 * @param {string[]} files Changed old and new paths. 变更的旧路径与新路径。
 * @param {boolean} full Whether the event requires full validation. 事件是否要求完整验证。
 * @returns {object} Boolean job flags and diagnostic reasons. 作业布尔标记与诊断原因。
 */
export function classifyChanges(files, full = false) {
  // scope accumulates the union, so unrelated changes cannot turn an earlier gate off.
  // scope 累积并集，后续无关变更不能关闭已选中的检查。
  const scope = { ...Object.fromEntries(JOBS.map(job => [job, false])), fixtures: false, full, reasons: [] };
  // select enables named gates and records the matched classification rule.
  // select 启用指定检查并记录匹配的分类规则。
  const select = (jobs, reason) => { for (const job of jobs) scope[job] = true; scope.reasons.push(reason); };
  if (full) select([...JOBS, 'fixtures'], 'explicit full validation');
  // file is an exact Git pathname, never a shell expression or normalized guess.
  // file 是精确 Git 路径，不作为 shell 表达式，也不猜测规范化结果。
  for (const file of files) {
    if (/^(?:\.github\/|scripts\/|make(?:\.|$)|Makefile$|schemas\/)/.test(file)
      || file === 'crates/vulcan-service-runtime/managed-runtimes.json') {
      select([...JOBS, 'fixtures'], `build infrastructure: ${file}`);
    } else if (file.startsWith('apps/vulcan-code-web/')) {
      select(['frontend'], `web: ${file}`);
      if (/^apps\/vulcan-code-web\/(?:scripts\/|package\.json$|pnpm-lock\.yaml$|eslint|vite\.config|tsconfig)/.test(file)) scope.fixtures = true;
    } else if (file.startsWith('apps/vulcan-code-desktop/')) {
      select(['desktop'], `desktop: ${file}`);
    } else if (/^plugins\/[^/]+\/(?:ui|locales)\//.test(file)) {
      select(['frontend'], `plugin UI: ${file}`);
    } else if (file.startsWith('plugins/')) {
      select(['frontend', 'rustfmt', 'rust'], `plugin runtime: ${file}`);
    } else if (/^crates\/(?:vulcan-service-api|vulcan-ai-contract|vulcan-workbench-model|vulcan-provider-config|vulcan-terminal-host-protocol|vulcan-plugin-runtime|vulcan-i18n)\//.test(file)
      || /^docs\/contracts\/.*\.(?:json|yaml|yml|proto)$/.test(file)) {
      select(['frontend', 'desktop', 'rustfmt', 'rust'], `shared contract: ${file}`);
    } else if (/^(?:crates\/|apps\/vulcan-code-service\/|apps\/vulcan-code-tui\/|Cargo\.(?:toml|lock)$|rust-toolchain\.toml$|\.cargo\/|\.config\/nextest\.toml$)/.test(file)) {
      select(['rustfmt', 'rust'], `rust: ${file}`);
    } else if (/^docs\/.*\.md$/.test(file) || /^(?:README(?:[.-].*)?\.md|AGENTS\.md|LICENSE(?:[.-].*)?|CHANGELOG\.md)$/.test(file)) {
      scope.reasons.push(`documentation: ${file}`);
    } else {
      // Unknown inputs deliberately expand coverage; newly added directories must receive an explicit rule.
      // 未知输入明确扩大覆盖；新增目录必须补充显式分类规则。
      select([...JOBS, 'fixtures'], `unclassified; all gates required: ${file}`);
    }
  }
  return scope;
}

/** Read complete event changes from Git; missing push history conservatively requests all gates.
 * 从 Git 读取完整事件变更；推送历史缺失时保守要求全部检查。
 * @param {string} eventName GitHub event name. GitHub 事件名称。
 * @param {object} event Parsed GitHub event. 已解析的 GitHub 事件。
 * @param {string} cwd Repository working directory. 仓库工作目录。
 * @returns {object} Changed files, full flag and reason. 变更文件、完整标记及原因。
 */
export function eventChanges(eventName, event, cwd = process.cwd()) {
  if (['schedule', 'workflow_dispatch'].includes(eventName)) return { files: [], full: true, reason: eventName };
  if (!['push', 'pull_request'].includes(eventName)) throw new Error(`Unsupported event: ${eventName}`);
  // base/head come from documented event fields, never a truncated webhook commits array.
  // base/head 来自规定的事件字段，绝不读取可能截断的 webhook commits 数组。
  const base = eventName === 'push' ? event.before : event.pull_request.base.sha;
  const head = eventName === 'push' ? event.after : event.pull_request.head.sha;
  if (![base, head].every(sha => typeof sha === 'string' && /^[a-f0-9]{40}$/.test(sha))) throw new Error('Missing or invalid event commit SHA.');
  if (/^0+$/.test(head)) throw new Error('Deleted branch has no testable head.');
  if (/^0+$/.test(base)) return { files: [], full: true, reason: 'initial push' };
  // options bounds command output and preserves filenames containing whitespace or newlines.
  // options 限制命令输出，并保留带空格或换行的文件名。
  const options = { cwd, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024, stdio: ['ignore', 'pipe', 'pipe'] };
  try { execFileSync('git', ['cat-file', '-e', `${base}^{commit}`], options); }
  catch (error) {
    if (eventName === 'push' && error.status === 128) return { files: [], full: true, reason: 'push base unavailable; full validation' };
    throw error;
  }
  // range uses GitHub's three-dot PR semantics and endpoint push semantics across every pushed commit.
  // range 使用 GitHub 的 PR 三点差异及覆盖所有推送提交的端点差异语义。
  const range = eventName === 'pull_request' ? [`${base}...${head}`] : [base, head];
  // Renames are represented as deletion plus addition so both ownership boundaries trigger.
  // 将重命名表示为删除加新增，确保新旧归属边界都触发。
  const output = execFileSync('git', ['diff', '--name-only', '-z', '--no-renames', ...range, '--'], options);
  return { files: output.split('\0').filter(Boolean), full: false, reason: `${eventName}: ${base} -> ${head}` };
}

/** Reject missing, failed, cancelled or unexpectedly skipped jobs against the classifier's frozen outputs.
 * 根据分类器冻结的输出拒绝缺失、失败、取消或意外跳过的作业。
 * @param {object} needs GitHub needs context including changes and every gated job. 包含 changes 与所有条件作业的 GitHub needs 上下文。
 * @returns {void} Throws on any contract mismatch. 契约不符时抛出异常。
 */
export function verifyRequired(needs) {
  if (needs.changes?.result !== 'success') throw new Error('Change classification did not succeed.');
  if (Object.keys(needs).sort().join() !== ['changes', ...JOBS].sort().join()) throw new Error('Unexpected or missing required job identity.');
  // job checks the output as a string because GitHub exposes all job outputs as strings.
  // job 按字符串检查输出，因为 GitHub 的全部作业输出都是字符串。
  for (const job of JOBS) {
    // selected is validated before interpreting intentional skips.
    // selected 在解释有意跳过之前先进行合法性校验。
    const selected = needs.changes.outputs[job];
    if (!['true', 'false'].includes(selected)) throw new Error(`Invalid selection for ${job}: ${selected}`);
    // expected permits skipped only when the classifier explicitly excluded this job.
    // expected 仅允许分类器明确排除的作业跳过。
    const expected = selected === 'true' ? 'success' : 'skipped';
    if (needs[job].result !== expected) throw new Error(`${job}: expected ${expected}, received ${needs[job].result}`);
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv[2] === '--verify-required') {
    verifyRequired(JSON.parse(process.env.CI_NEEDS));
    console.log('All selected quality gates succeeded.');
  } else {
    // changes supports explicit local filenames or the documented workflow event input.
    // changes 支持显式本地文件名或规定的工作流事件输入。
    const changes = process.argv[2] === '--from-event'
      ? eventChanges(process.env.GITHUB_EVENT_NAME, JSON.parse(readFileSync(process.env.GITHUB_EVENT_PATH, 'utf8')))
      : { files: process.argv.slice(2), full: false, reason: 'explicit local paths' };
    // scope is emitted once, before any dependent job starts.
    // scope 在任何依赖作业启动之前一次性输出。
    const scope = classifyChanges(changes.files, changes.full);
    console.log(JSON.stringify({ ...scope, event: changes.reason }, null, 2));
    if (process.env.GITHUB_OUTPUT) {
      for (const key of [...JOBS, 'fixtures', 'full']) appendFileSync(process.env.GITHUB_OUTPUT, `${key}=${scope[key]}\n`);
    }
    if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY,
      `### Selected quality gates\n\n${JOBS.map(job => `- ${job}: ${scope[job] ? 'required' : 'skipped'}`).join('\n')}\n\n${changes.files.length} changed paths; ${changes.reason}\n`);
  }
}
