// Decide what one release publishes and write the documents the release tool signs.
// 确定一次发布的内容，并写出发布工具要签署的文档。
//
// `check --version V` is the release workflow's first step: V must equal `product.json`, the release notes
// must pass the shipped-notes check, and the newest release must carry its date, so an unreleased
// version can never be published by accident. `notes --out FILE` writes the notes document the manifest
// references: `{"schema":1,"version":V,"date":"YYYY-MM-DD","notes":{"<language>":"<markdown>"}}`.
// `check --version V` 是发布工作流的第一步：V 必须等于 `product.json`，更新记录必须通过发行检查，且最新
// 发布必须已填写日期，避免误发未发布的版本。`notes --out FILE` 写出版本清单引用的更新说明文档：
// `{"schema":1,"version":V,"date":"YYYY-MM-DD","notes":{"<语言>":"<markdown>"}}`。
// `platform --tree DIR` prints the release platform of a built tree, from `managedRuntimePlatformKey`, the one
// definition of the platform names; the build jobs compare it with the platform they publish.
// `platform --tree DIR` 输出已构建目录的发布平台，取自平台名称的唯一定义 `managedRuntimePlatformKey`；构建作业
// 用它核对自己发布的平台。
import { appendFileSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { managedRuntimePlatformKey } from './desktop-runtime.mjs';
import { LOCALES_PATH, RESOURCES_DIR, checkReleaseNotes, readProductVersion } from './release-notes-check.mjs';

// NOTES_SCHEMA versions the notes document; clients reject other values.
// NOTES_SCHEMA 是更新说明文档的格式版本；客户端拒绝其他值。
export const NOTES_SCHEMA = 1;

// releasePlan validates the release of `requested` from `resourcesDir` and returns its facts.
// releasePlan 校验 `resourcesDir` 中 `requested` 版本的发布，并返回其事实。
// Every problem is reported together, so one run shows all that must change before releasing.
// 一次报告全部问题，使一次运行就能看到发布前必须修改的所有内容。
export function releasePlan(resourcesDir, codes, requested) {
  const productPath = path.join(resourcesDir, 'product.json');
  const version = readProductVersion(productPath);
  const { installEpoch } = JSON.parse(readFileSync(productPath, 'utf8'));
  const problems = [];
  if (requested !== version) problems.push(`the requested version ${requested} differs from product.json ${version}`);
  if (!Number.isSafeInteger(installEpoch) || installEpoch < 1) problems.push('product.json needs a positive integer `installEpoch`');
  const changelogDir = path.join(resourcesDir, 'changelog');
  problems.push(...checkReleaseNotes(changelogDir, codes, version));
  const newest = JSON.parse(readFileSync(path.join(changelogDir, 'releases.json'), 'utf8')).releases?.[0];
  if (newest?.version === version && newest.date === null) {
    problems.push(`changelog: set the release date of ${version} before releasing it`);
  }
  if (problems.length > 0) throw new Error(`The release is not ready:\n${problems.map(problem => `  - ${problem}`).join('\n')}`);
  return { version, installEpoch, date: newest.date };
}

// notesDocument collects the newest release's notes in every registered language.
// notesDocument 收集最新发布在每种注册语言中的更新说明。
export function notesDocument(resourcesDir, codes, plan) {
  const directory = path.join(resourcesDir, 'changelog', plan.version);
  const notes = Object.fromEntries(codes.map(code => [code, readFileSync(path.join(directory, `${code}.md`), 'utf8')]));
  return { schema: NOTES_SCHEMA, version: plan.version, date: plan.date, notes };
}

if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) {
  const [command, flag, value] = process.argv.slice(2);
  const codes = JSON.parse(readFileSync(LOCALES_PATH, 'utf8')).locales.map(locale => locale.code);
  if (command === 'check' && flag === '--version' && value) {
    const plan = releasePlan(RESOURCES_DIR, codes, value);
    console.log(`Releasing ${plan.version} (install epoch ${plan.installEpoch}, dated ${plan.date}).`);
    if (process.env.GITHUB_OUTPUT) {
      appendFileSync(process.env.GITHUB_OUTPUT, `version=${plan.version}\ninstall_epoch=${plan.installEpoch}\n`);
    }
  } else if (command === 'platform' && flag === '--tree' && value) {
    console.log(managedRuntimePlatformKey(JSON.parse(readFileSync(path.join(value, 'runtime-layout.json'), 'utf8'))));
  } else if (command === 'notes' && flag === '--out' && value) {
    const product = JSON.parse(readFileSync(path.join(RESOURCES_DIR, 'product.json'), 'utf8'));
    const document = notesDocument(RESOURCES_DIR, codes, releasePlan(RESOURCES_DIR, codes, product.version));
    writeFileSync(value, `${JSON.stringify(document, null, 2)}\n`);
    console.log(`Release notes of ${document.version}: ${value}`);
  } else {
    throw new Error('Usage: node scripts/release-plan.mjs check --version <version> | notes --out <file> | platform --tree <dir>');
  }
}
