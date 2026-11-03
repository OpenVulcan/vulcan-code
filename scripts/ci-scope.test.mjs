// Exercise selection and failure propagation with actual Git histories, not mocked filename lists alone.
// 使用真实 Git 历史验证选择与失败传播，不仅模拟文件名列表。
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, renameSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { GATES, JOBS, classifyChanges, eventChanges, verifyRequired } from './ci-scope.mjs';

// cases map meaningful ownership boundaries to exactly the required jobs.
// cases 将有意义的归属边界映射到精确的必需作业集合。
const cases = [
  ['apps/vulcan-code-web/src/view.tsx', ['frontend', 'static']],
  ['apps/vulcan-code-web/src/i18n-catalogs.ts', ['frontend', 'static']],
  ['crates/vulcan-i18n/catalogs/webui/zh-CN.json', ['frontend', 'desktop', 'static', 'rust']],
  ['crates/vulcan-runtime-assets/resources/changelog/0.1.0/en-US.md', ['frontend', 'desktop', 'static', 'rust']],
  ['apps/vulcan-code-tui/src/ui/theme.rs', ['static', 'rust']],
  ['apps/vulcan-code-desktop/src/gui/main.ts', ['desktop', 'static']],
  ['docs/contracts/plugin-execution-engine.md', []],
  ['README.md', []],
  ['plugins/context7/ui/index.js', ['frontend', 'static']],
  ['plugins/context7/locales/ja-JP.yaml', ['frontend', 'static']],
  ['plugins/context7/plugin.yaml', ['frontend', 'static', 'rust']],
  ['plugins/context7/runtime/plugin.bundle.lua', ['frontend', 'static', 'rust']],
  ['plugins/context7/prompts/example.md', ['frontend', 'static', 'rust']],
  ['crates/vulcan-runtime-assets/resources/skills/example.md', ['static', 'rust']],
  ['crates/vulcan-service-api/src/service.rs', ['frontend', 'desktop', 'static', 'rust']],
  ['docs/contracts/plugin.schema.json', ['frontend', 'desktop', 'static', 'rust']],
  ['Cargo.lock', ['static', 'rust']],
  ['.cargo/config.toml', ['static', 'rust']],
  ['.config/ci-quarantine.json', ['static', 'rust']],
  ['crates/vulcan-llm-core/src/lib.rs', ['static', 'rust']],
  ['scripts/install_lua_deps.ps1', JOBS],
  ['.github/workflows/ci.yml', JOBS],
  ['.github/workflows/nightly.yml', JOBS],
  ['crates/vulcan-service-runtime/managed-runtimes.json', JOBS],
  ['new-runtime/input.dat', JOBS],
];
for (const [file, expected] of cases) {
  test(`classify ${file}`, () => {
    // scope is the public classification result for one ownership boundary.
    // scope 是单个归属边界的公开分类结果。
    const scope = classifyChanges([file]);
    assert.deepEqual(JOBS.filter(job => scope[job]), expected);
  });
}

test('unions mixed paths and runs checker fixtures only for checker inputs', () => {
  assert.equal(classifyChanges(['apps/vulcan-code-web/src/view.tsx']).fixtures, false);
  assert.equal(classifyChanges(['apps/vulcan-code-web/scripts/check-ui-architecture.mjs']).fixtures, true);
  assert.equal(classifyChanges(['apps/vulcan-code-web/pnpm-lock.yaml']).fixtures, true);
  assert.equal(classifyChanges(['docs/a.md', 'Cargo.toml']).rust, true);
  assert.equal(classifyChanges([], true).fixtures, true);
  assert.ok(JOBS.every(job => classifyChanges([], true)[job]));
});

/** Construct GitHub needs values for one selection; returns mutable fixtures for negative tests.
 * 为一个选择构造 GitHub needs 值；返回可修改的夹具以进行反向测试。
 * @param {string[]} files Changed paths. 变更路径。
 * @returns {object} Complete needs fixture. 完整 needs 夹具。
 */
function resultsFor(files) {
  // scope drives both job results and string outputs as GitHub would.
  // scope 同时驱动作业结果和字符串输出，保持 GitHub 语义。
  const scope = classifyChanges(files);
  return {
    changes: { result: 'success', outputs: Object.fromEntries(JOBS.map(job => [job, String(scope[job])])) },
    ...Object.fromEntries(Object.entries(GATES).map(([job, flag]) => [job, { result: scope[flag] ? 'success' : 'skipped' }])),
  };
}

test('required summary accepts intentional skips and refuses every non-success selected result', () => {
  verifyRequired(resultsFor(['docs/notes.md']));
  verifyRequired(resultsFor(['apps/vulcan-code-web/src/view.tsx']));
  for (const state of ['failure', 'cancelled', 'skipped', 'timed_out', undefined]) {
    // needs models an upstream selected gate that failed to complete successfully.
    // needs 模拟未能成功完成的已选中上游检查。
    // Each Rust gate fails on its own, since both share one selection flag.
    // 两个 Rust 检查共用一个选择标记，因此各自单独失败。
    for (const job of ['rust-check', 'rust-tests']) {
      const needs = resultsFor(['Cargo.toml']);
      needs[job].result = state;
      assert.throws(() => verifyRequired(needs), new RegExp(`${job}:`));
    }
  }
});

test('required summary refuses invalid classification, missing jobs and unexpected jobs', () => {
  for (const mutate of [
    needs => { needs.changes.result = 'failure'; },
    needs => { delete needs['rust-tests']; },
    needs => { needs.extra = { result: 'success' }; },
    needs => { delete needs.changes.outputs.rust; },
    needs => { needs['rust-check'].result = 'success'; },
  ]) {
    // needs starts from an otherwise valid documentation-only result.
    // needs 从原本有效的纯文档结果开始。
    const needs = resultsFor(['docs/notes.md']);
    mutate(needs);
    assert.throws(() => verifyRequired(needs));
  }
});

test('real Git covers multi-commit pushes, cross-boundary renames, deletion and PR merge bases', () => {
  // root is a unique disposable repository owned solely by this test.
  // root 是仅归本测试所有的独立可丢弃仓库。
  const root = mkdtempSync(path.join(os.tmpdir(), 'vulcan-ci-scope-'));
  // git executes argument arrays without a shell and returns trimmed command output.
  // git 不经 shell 执行参数数组，返回去除首尾空白的命令输出。
  const git = (...args) => execFileSync('git', args, { cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  // write creates a tracked fixture at a repository-relative pathname.
  // write 在仓库相对路径创建受版本控制的夹具。
  const write = (file, content) => { mkdirSync(path.dirname(path.join(root, file)), { recursive: true }); writeFileSync(path.join(root, file), content); };
  // commit returns the identity of the newly staged test snapshot.
  // commit 返回新暂存测试快照的提交身份。
  const commit = () => { git('add', '.'); git('commit', '-m', 'fixture'); return git('rev-parse', 'HEAD'); };
  try {
    git('init', '-b', 'main'); git('config', 'user.email', 'ci@example.invalid'); git('config', 'user.name', 'CI fixture');
    write('docs/start.md', 'base');
    // base anchors all commits in the simulated multi-commit push.
    // base 锚定模拟多提交推送中的全部提交。
    const base = commit();
    write('apps/vulcan-code-web/src/with space.ts', 'first'); commit();
    write('docs/last.md', 'last');
    // head includes an earlier source edit even though the final commit only changes docs.
    // head 即使最后提交只修改文档，也包含较早的源码编辑。
    const head = commit();
    assert.equal(classifyChanges(eventChanges('push', { before: base, after: head }, root).files).frontend, true);
    mkdirSync(path.join(root, 'crates/example/src'), { recursive: true });
    renameSync(path.join(root, 'apps/vulcan-code-web/src/with space.ts'), path.join(root, 'crates/example/src/lib.rs'));
    // moved exposes both old and new paths, even with identical bytes.
    // moved 即使字节完全相同，也暴露旧路径和新路径。
    const moved = eventChanges('push', { before: head, after: commit() }, root);
    assert.equal(classifyChanges(moved.files).frontend, true); assert.equal(classifyChanges(moved.files).rust, true);
    git('checkout', '-b', 'proposal', base); write('docs/proposal.md', 'proposal');
    // proposal diverges from main, whose Rust edit must not be attributed to the PR.
    // proposal 与 main 分叉，不应将主分支的 Rust 编辑归入 PR。
    const proposal = commit(); git('checkout', 'main');
    assert.deepEqual(eventChanges('pull_request', { pull_request: { base: { sha: git('rev-parse', 'HEAD') }, head: { sha: proposal } } }, root).files, ['docs/proposal.md']);
    assert.equal(eventChanges('push', { before: '0'.repeat(40), after: head }, root).full, true);
    assert.equal(eventChanges('push', { before: '1'.repeat(40), after: head }, root).full, true);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('manual runs select all, scheduled runs select all once per new main commit, malformed events fail', () => {
  const head = 'a'.repeat(40);
  assert.equal(eventChanges('workflow_dispatch', {}).full, true);
  assert.equal(eventChanges('schedule', {}, undefined, { head, tested: 'b'.repeat(40) }).full, true);
  assert.equal(eventChanges('schedule', {}, undefined, { head, tested: '' }).full, true);
  // A scheduled run on an already passed commit selects no gate, which the required summary accepts as intentional skips.
  // 已通过提交上的定时运行不选择任何检查，必需汇总将其视为有意跳过。
  const passed = classifyChanges(eventChanges('schedule', {}, undefined, { head, tested: head }).files);
  assert.equal(JOBS.some(job => passed[job]), false);
  assert.throws(() => eventChanges('schedule', {}, undefined, {}), /scheduled commit SHA/);
  assert.throws(() => eventChanges('push', { before: 'bad', after: 'bad' }), /SHA/);
  assert.throws(() => eventChanges('unknown', {}), /Unsupported/);
});
