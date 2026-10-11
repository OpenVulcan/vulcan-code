// Ratchet gate against CJK string literals in Rust production code.
// 防止 Rust 生产代码出现 CJK 字符串字面量的棘轮门禁。
//
// Backend text is English: model-facing tool results and errors are read by models, and service
// errors, feedback and labels reach users through `webui` catalog keys rendered by each client. A
// literal counts when it contains CJK characters and sits outside comments and test code (test
// files, `tests` directories and `#[cfg(test)]` items). Each file may keep at most its recorded
// count; `--update` records counts that only went down. New files start at zero. Data that genuinely
// matches CJK input (for example query keyword lists or tokenizer fixtures) is recorded by editing
// the baseline by hand, so the exception stays visible in review.
// 后端文本使用英文：面向模型的工具结果与错误由模型读取，服务错误、回馈与标签通过各客户端渲染的
// `webui` 目录键到达用户。字面量含 CJK 字符且位于注释与测试代码（测试文件、`tests` 目录与
// `#[cfg(test)]` 项）之外时计数。每个文件最多保留已记录的数量；`--update` 仅记录下降后的数量。新文件
// 从零开始。确实需要匹配 CJK 输入的数据（例如查询关键词表或分词夹具）需手动编辑基线登记，使该例外
// 在审查中可见。
import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { compareWithBaseline, tokenize } from './wording-assertions.mjs';

// Repository root keeps the scan independent of the caller's working directory.
// 仓库根目录使扫描不依赖调用者当前工作目录。
const root = fileURLToPath(new URL('..', import.meta.url));

// Baseline of tolerated literals per repository-relative file.
// 按仓库相对路径记录的可容忍字面量数量基线。
export const BASELINE_PATH = path.join(root, 'scripts', 'backend-cjk-literals-baseline.json');

// Scanned source trees; generated, vendored and build directories are skipped by name.
// 扫描的源码树；生成、第三方与构建目录按名称跳过。
const SCAN_ROOTS = ['crates', 'apps', 'plugins'];
const SKIPPED_DIRECTORIES = new Set(['node_modules', 'target', 'dist', 'output', 'gui', 'vendor', 'tests']);

// Test-only file names that hold no production code.
// 不含生产代码的测试专用文件名。
const TEST_FILE = /^(?:tests|test_support|.*_tests)\.rs$/u;

// Attribute that marks the following item as test-only.
// 将其后条目标记为仅测试的属性。
const CFG_TEST = /#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]/gu;

// testItemRanges returns the `[start, end)` offsets of every `#[cfg(test)]` item in masked code.
// testItemRanges 返回已遮蔽代码中每个 `#[cfg(test)]` 条目的 `[start, end)` 偏移。
//
// # Parameters
// 参数
// `code` is tokenized Rust whose comments and literal contents are blanked.
// `code` 是已清空注释与字面量内容的 Rust 代码。
//
// # Returns
// 返回值
// Ranges covering each attribute through the end of its item: the matching `}` of a block item or
// the `;` of a declaration such as `mod tests;`.
// 返回覆盖各属性至其条目结尾的区间：块条目的匹配 `}`，或 `mod tests;` 等声明的 `;`。
export function testItemRanges(code) {
  const ranges = [];
  for (const match of code.matchAll(CFG_TEST)) {
    const start = match.index;
    let cursor = start + match[0].length;
    while (cursor < code.length && code[cursor] !== '{' && code[cursor] !== ';') cursor += 1;
    if (code[cursor] === ';') {
      ranges.push([start, cursor + 1]);
      continue;
    }
    let depth = 0;
    for (; cursor < code.length; cursor += 1) {
      if (code[cursor] === '{') depth += 1;
      else if (code[cursor] === '}') {
        depth -= 1;
        if (depth === 0) break;
      }
    }
    ranges.push([start, cursor + 1]);
  }
  return ranges;
}

// countCjkLiterals counts CJK string literals outside comments and `#[cfg(test)]` items.
// countCjkLiterals 统计注释与 `#[cfg(test)]` 条目之外的 CJK 字符串字面量。
//
// # Parameters
// 参数
// `source` is the text of one Rust file.
// `source` 是一个 Rust 文件的文本。
//
// # Returns
// 返回值
// The number of counted literals.
// 返回计数的字面量数量。
export function countCjkLiterals(source) {
  const { code, cjkLiterals } = tokenize(source, 'rust');
  if (cjkLiterals.length === 0) return 0;
  const tests = testItemRanges(code);
  return cjkLiterals.filter(([start]) => !tests.some(([from, to]) => start >= from && start < to)).length;
}

// scanRepository counts CJK literals in every scanned Rust production file.
// scanRepository 统计每个扫描的 Rust 生产文件中的 CJK 字面量。
//
// # Returns
// 返回值
// A map from repository-relative POSIX path to a positive count.
// 返回从仓库相对 POSIX 路径到正数计数的映射。
export function scanRepository() {
  const counts = new Map();
  const visit = (directory) => {
    let entries;
    try {
      entries = readdirSync(directory, { withFileTypes: true });
    } catch {
      return;
    }
    for (const entry of entries) {
      const full = path.join(directory, entry.name);
      if (entry.isDirectory()) {
        if (!SKIPPED_DIRECTORIES.has(entry.name) && !entry.name.startsWith('.')) visit(full);
        continue;
      }
      if (!entry.name.endsWith('.rs') || TEST_FILE.test(entry.name)) continue;
      const count = countCjkLiterals(readFileSync(full, 'utf8'));
      if (count > 0) counts.set(path.relative(root, full).split(path.sep).join('/'), count);
    }
  };
  for (const scanRoot of SCAN_ROOTS) visit(path.join(root, scanRoot));
  return counts;
}

// main runs the gate; `--update` records counts that only went down.
// main 运行门禁；`--update` 仅记录下降后的计数。
function main() {
  const update = process.argv.includes('--update');
  const baseline = JSON.parse(readFileSync(BASELINE_PATH, 'utf8'));
  const counts = scanRepository();
  const { exceeded, lowered } = compareWithBaseline(counts, baseline);
  if (exceeded.length > 0) {
    console.error('Backend code gained CJK literals; write English, and use webui catalog keys for user-facing text:');
    for (const { file, count, allowed } of exceeded) console.error(`  ${file}: ${count} (allowed ${allowed})`);
    process.exit(1);
  }
  if (lowered.length === 0) {
    console.log('Backend CJK literal counts are within the baseline.');
    return;
  }
  if (!update) {
    console.error('CJK literals were removed; run `node scripts/backend-cjk-literals.mjs --update` to lower the baseline:');
    for (const { file, count, allowed } of lowered) console.error(`  ${file}: ${count} (recorded ${allowed})`);
    process.exit(1);
  }
  const next = Object.fromEntries([...counts.entries()].sort(([left], [right]) => left.localeCompare(right)));
  writeFileSync(BASELINE_PATH, `${JSON.stringify(next, null, 2)}\n`);
  console.log(`Lowered the baseline for ${lowered.length} file(s).`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
