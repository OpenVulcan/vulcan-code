// Refresh the bundled provider-manifest baseline after manifest edits with one command.
// 编辑清单后用一条命令刷新随包供应商清单基线。
//
// Usage: node scripts/provider-manifests-refresh.mjs [--dry-run] [--no-sync]
// 1. Compares every manifest file with the digest pinned by bundled-index.json.
// 2. Bumps `revision` and `published_at` of each changed manifest whose revision was not raised yet.
// 3. Rebuilds bundled-index.json with the next index revision, then runs `validate-all`.
// 4. Runs `make.ps1 sync` so `output/` serves the new baseline (skip with --no-sync).
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// repositoryRoot anchors every path so the script works from any working directory.
// repositoryRoot 锚定所有路径，使脚本可在任意工作目录运行。
const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const baselineDirectory = path.join(repositoryRoot, 'crates/vulcan-runtime-assets/resources/provider-manifests');
const manifestDirectory = path.join(baselineDirectory, 'manifests');
const indexFile = path.join(baselineDirectory, 'bundled-index.json');

// sha256 returns the lowercase hex digest the index pins for one file.
// sha256 返回索引为单个文件固定的小写十六进制摘要。
export function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

// planRefresh decides which manifests need a revision bump and whether the index must be rebuilt.
// planRefresh 判定哪些清单需要递增修订，以及是否必须重建索引。
// index is the parsed bundled index; manifests maps provider file stem to { bytes, revision }.
// index 为已解析的随包索引；manifests 将供应商文件名映射到 { bytes, revision }。
export function planRefresh(index, manifests) {
  const changed = [];
  const bumps = [];
  for (const [providerId, manifest] of Object.entries(manifests)) {
    const pinned = index.providers[providerId];
    if (pinned && pinned.sha256 === sha256(manifest.bytes)) continue;
    changed.push(providerId);
    if (pinned && manifest.revision <= pinned.revision) bumps.push({ providerId, revision: pinned.revision + 1 });
  }
  const removed = Object.keys(index.providers).filter((providerId) => !(providerId in manifests));
  return {
    changed,
    removed,
    bumps,
    rebuildIndex: changed.length > 0 || removed.length > 0,
    indexRevision: index.index_revision + 1,
  };
}

// bumpManifestHeader rewrites only the top-level revision and published_at lines, keeping hand formatting.
// bumpManifestHeader 只重写顶层 revision 与 published_at 行，保留手工排版。
export function bumpManifestHeader(text, revision, publishedAt) {
  const revisionPattern = /^( {2})"revision": \d+,/m;
  const publishedPattern = /^( {2})"published_at": "[^"]*",/m;
  if (!revisionPattern.test(text) || !publishedPattern.test(text)) {
    throw new Error('Manifest header must contain top-level "revision" and "published_at" lines.');
  }
  return text
    .replace(revisionPattern, `$1"revision": ${revision},`)
    .replace(publishedPattern, `$1"published_at": "${publishedAt}",`);
}

// run executes one command inheriting stdio and fails the script on a non-zero exit.
// run 继承标准输入输出执行一条命令，非零退出时令脚本失败。
function run(command, args) {
  console.log(`> ${command} ${args.join(' ')}`);
  execFileSync(command, args, { cwd: repositoryRoot, stdio: 'inherit' });
}

function main() {
  const dryRun = process.argv.includes('--dry-run');
  const sync = !process.argv.includes('--no-sync');
  const index = JSON.parse(readFileSync(indexFile, 'utf8'));
  const manifests = {};
  for (const name of readdirSync(manifestDirectory).filter((entry) => entry.endsWith('.json')).sort()) {
    const bytes = readFileSync(path.join(manifestDirectory, name));
    manifests[name.slice(0, -'.json'.length)] = { bytes, revision: JSON.parse(bytes.toString('utf8')).revision };
  }
  const plan = planRefresh(index, manifests);
  if (!plan.rebuildIndex) {
    console.log('Provider manifests match bundled-index.json; nothing to refresh.');
    return;
  }
  console.log(`Changed: ${plan.changed.join(', ') || '(none)'}`);
  if (plan.removed.length) console.log(`Removed: ${plan.removed.join(', ')}`);
  for (const { providerId, revision } of plan.bumps) console.log(`Bump ${providerId} to revision ${revision}`);
  console.log(`Index revision ${index.index_revision} -> ${plan.indexRevision}`);
  if (dryRun) return;

  const now = Date.now();
  const publishedAt = new Date(now).toISOString().replace(/\.\d{3}Z$/, 'Z');
  for (const { providerId, revision } of plan.bumps) {
    const file = path.join(manifestDirectory, `${providerId}.json`);
    writeFileSync(file, bumpManifestHeader(readFileSync(file, 'utf8'), revision, publishedAt));
  }
  const cli = ['run', '--quiet', '-p', 'vulcan-provider-manifest-cli', '--'];
  run('cargo', [
    ...cli, 'build-index', path.relative(repositoryRoot, manifestDirectory).replaceAll('\\', '/'),
    '--revision', String(plan.indexRevision),
    '--key-id', index.signing_key_id,
    '--issued-ms', String(now),
    '--expires-ms', String(index.expires_at_unix_ms),
    '--relative-prefix', 'manifests',
    '--out', path.relative(repositoryRoot, indexFile).replaceAll('\\', '/'),
  ]);
  run('cargo', [...cli, 'validate-all']);
  if (sync) run('pwsh', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', 'make.ps1', 'sync']);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
