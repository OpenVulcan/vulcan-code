// Ratchet gate against assertions that compare Chinese wording instead of behavior.
// 防止断言比较中文文案而非行为的棘轮门禁。
//
// An assertion counts when its statement contains a string (or TypeScript regex) literal with CJK characters: Rust
// `assert!`/`assert_eq!`/`assert_ne!`, and TypeScript `expect(...)` chains or `*ByText(...)` queries.
// Each file may keep at most its recorded count; rewriting assertions lowers the count, and
// `--update` records the lower counts. New files start at zero. A test whose behavior genuinely
// depends on CJK input (for example tokenization) is recorded by editing the baseline by hand, so the
// exception stays visible in review.
// 断言语句含有带 CJK 字符的字符串（或 TypeScript 正则）字面量时计数：Rust 的 `assert!`/`assert_eq!`/`assert_ne!`，以及
// TypeScript 的 `expect(...)` 链或 `*ByText(...)` 查询。每个文件最多保留已记录的数量；改写断言会降低
// 数量，`--update` 记录更低的数量。新文件从零开始。行为确实依赖 CJK 输入的测试（例如分词）需手动
// 编辑基线登记，使该例外在审查中可见。
import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// Repository root keeps the scan independent of the caller's working directory.
// 仓库根目录使扫描不依赖调用者当前工作目录。
const root = fileURLToPath(new URL('..', import.meta.url));

// Baseline of tolerated legacy assertions per repository-relative file.
// 按仓库相对路径记录的可容忍遗留断言数量基线。
export const BASELINE_PATH = path.join(root, 'scripts', 'wording-assertions-baseline.json');

// Scanned source trees; generated, vendored and build directories are skipped by name.
// 扫描的源码树；生成、第三方与构建目录按名称跳过。
const SCAN_ROOTS = ['crates', 'apps', 'plugins'];
const SKIPPED_DIRECTORIES = new Set(['node_modules', 'target', 'dist', 'output', 'gui', 'vendor']);

// CJK unified ideographs, kana and hangul mark user-facing wording.
// CJK 统一表意文字、假名与谚文标识面向用户的文案。
const CJK = /[぀-ヿ㐀-䶿一-鿿가-힯]/u;

// Starts of counted assertion statements per language.
// 各语言中计数断言语句的起始形式。
const RUST_ASSERTION = /\b(?:assert|assert_eq|assert_ne)!\s*\(/gu;
const TS_ASSERTION = /\b(?:expect|(?:get|getAll|find|findAll|query|queryAll)ByText)\s*\(/gu;

// Rust character literal; any other apostrophe starts a lifetime or label.
// Rust 字符字面量；其他撇号表示生命周期或标签。
const RUST_CHAR = /^'(?:\\(?:u\{[0-9a-fA-F]{1,6}\}|x[0-9a-fA-F]{2}|.)|[^\\'\n])'/u;

// Rust raw string opener `r"`, `r#"`, `br##"` and so on.
// Rust 原始字符串起始 `r"`、`r#"`、`br##"` 等。
const RUST_RAW = /^b?r(#*)"/u;

// tokenize blanks comments and the contents of string and TypeScript regex literals, keeping offsets and newlines.
// tokenize 清空注释与字符串及 TypeScript 正则字面量内容，保持偏移与换行不变。
//
// # Parameters
// 参数
// `source` is one file and `language` is `rust` or `typescript`.
// `source` 是一个文件，`language` 为 `rust` 或 `typescript`。
//
// # Returns
// 返回值
// `{ code, cjkLiterals }`: `code` holds only code characters and literal delimiters, and
// `cjkLiterals` lists the `[start, end)` offsets of string literals that contain CJK characters.
// 返回 `{ code, cjkLiterals }`：`code` 仅保留代码字符与字面量定界符，`cjkLiterals` 列出含 CJK 字符的
// 字符串字面量 `[start, end)` 偏移。
export function tokenize(source, language) {
  const code = [];
  const cjkLiterals = [];
  // Without the u flag each UTF-16 unit becomes one space, keeping code offsets aligned with cjkLiterals.
  // 不带 u 标志时每个 UTF-16 单元替换为一个空格，使 code 偏移与 cjkLiterals 对齐。
  const blank = (text) => text.replace(/[^\n]/g, ' ');
  let index = 0;
  // literal records one string literal of `length` characters and masks its contents.
  // literal 记录一个长度为 `length` 的字符串字面量并遮蔽其内容。
  const literal = (length, open, close) => {
    const text = source.slice(index, index + length);
    if (CJK.test(text)) cjkLiterals.push([index, index + length]);
    code.push(text.slice(0, open), blank(text.slice(open, length - close)), text.slice(length - close));
    index += length;
  };
  // quotedLength returns the length of a backslash-escaped literal closed by `quote`.
  // quotedLength 返回以 `quote` 结束、支持反斜杠转义的字面量长度。
  const quotedLength = (quote) => {
    let cursor = index + 1;
    while (cursor < source.length && source[cursor] !== quote) cursor += source[cursor] === '\\' ? 2 : 1;
    return Math.min(cursor + 1, source.length) - index;
  };
  while (index < source.length) {
    const character = source[index];
    const rest = source.slice(index, index + 64);
    if (character === '/' && source[index + 1] === '/') {
      const end = source.indexOf('\n', index);
      const stop = end === -1 ? source.length : end;
      code.push(blank(source.slice(index, stop)));
      index = stop;
      continue;
    }
    if (character === '/' && source[index + 1] === '*') {
      const end = source.indexOf('*/', index + 2);
      const stop = end === -1 ? source.length : end + 2;
      code.push(blank(source.slice(index, stop)));
      index = stop;
      continue;
    }
    if (language === 'rust') {
      const raw = /\w/u.test(source[index - 1] ?? '') ? null : RUST_RAW.exec(rest);
      if (raw !== null) {
        const terminator = `"${raw[1]}`;
        const end = source.indexOf(terminator, index + raw[0].length);
        const stop = end === -1 ? source.length : end + terminator.length;
        literal(stop - index, raw[0].length, Math.min(terminator.length, stop - index - raw[0].length));
        continue;
      }
      if (character === "'") {
        const char = RUST_CHAR.exec(rest);
        if (char !== null) {
          literal(char[0].length, 1, 1);
          continue;
        }
        code.push(character);
        index += 1;
        continue;
      }
      if (character === '"') {
        literal(quotedLength('"'), 1, 1);
        continue;
      }
    } else if (character === '"' || character === "'" || character === '`') {
      literal(quotedLength(character), 1, 1);
      continue;
    } else if (character === '/' && regexMayStartAfter(precedingToken(code))) {
      // A regex literal is masked like a string so quotes inside it never open one; its flags stay code.
      // 正则字面量像字符串一样遮蔽，使其中的引号不会开启字符串；其标志保留为代码。
      const length = regexLength(source, index);
      if (length !== null) {
        literal(length, 1, 1);
        continue;
      }
    }
    code.push(character);
    index += 1;
  }
  return { code: code.join(''), cjkLiterals };
}

// Keywords after which a TypeScript `/` starts a regex literal rather than a division.
// TypeScript 中其后的 `/` 开启正则字面量而非除法的关键字。
const REGEX_KEYWORDS = new Set([
  'await', 'case', 'delete', 'do', 'else', 'in', 'instanceof', 'new', 'of', 'return', 'throw', 'typeof', 'void', 'yield',
]);

// Punctuation after which a TypeScript `/` starts a regex literal. `<`, `>`, `}`, `)` and `]` are left out so
// JSX closing tags (`</a>`, `<a />`, `{x} />`) and divisions stay code; `=>` is checked separately.
// 其后的 TypeScript `/` 开启正则字面量的标点。排除 `<`、`>`、`}`、`)` 与 `]`，使 JSX 结束标签
// （`</a>`、`<a />`、`{x} />`）与除法保持为代码；`=>` 单独判断。
const REGEX_PUNCTUATION = new Set(['(', ',', '=', ':', '[', '!', '&', '|', '?', ';', '+', '-', '*', '%', '~', '^', '{']);

// precedingToken returns the last code token emitted so far: a whole identifier, or the last two characters
// of trailing punctuation; masked comments and literal contents are blank and therefore skipped.
// precedingToken 返回目前已输出的最后一个代码记号：完整标识符，或末尾标点的最后两个字符；被遮蔽的注释与
// 字面量内容为空白，因此会被跳过。
function precedingToken(code) {
  let text = '';
  for (let piece = code.length - 1; piece >= 0; piece -= 1) {
    text = code[piece] + text;
    const trimmed = text.trimEnd();
    if (trimmed.length === 0) continue;
    if (!/[\w$]$/u.test(trimmed)) return trimmed.slice(-2);
    const word = /[\w$]+$/u.exec(trimmed)[0];
    if (word.length < trimmed.length || piece === 0) return word;
  }
  return '';
}

// regexMayStartAfter tells whether a `/` following `token` opens a regex literal.
// regexMayStartAfter 判断位于 `token` 之后的 `/` 是否开启正则字面量。
function regexMayStartAfter(token) {
  if (token === '') return true;
  if (/[\w$]$/u.test(token)) return REGEX_KEYWORDS.has(token);
  return token === '=>' || REGEX_PUNCTUATION.has(token[token.length - 1]);
}

// regexLength returns the length of the regex literal opening at `start`, up to and including its closing
// slash, or null when the line ends first; a slash inside a character class or after a backslash does not close.
// regexLength 返回从 `start` 开始的正则字面量到其结束斜杠（含）的长度；行先结束时返回 null；
// 字符类内或反斜杠后的斜杠不会结束字面量。
function regexLength(source, start) {
  let inClass = false;
  for (let cursor = start + 1; cursor < source.length; cursor += 1) {
    const character = source[cursor];
    if (character === '\n') return null;
    if (character === '\\') cursor += 1;
    else if (character === '[') inClass = true;
    else if (character === ']') inClass = false;
    else if (character === '/' && !inClass) return cursor + 1 - start;
  }
  return null;
}

// balancedEnd returns the offset just past the parenthesis matching the one before `start`.
// balancedEnd 返回与 `start` 之前左括号匹配的右括号之后的偏移。
function balancedEnd(code, start) {
  let depth = 1;
  for (let index = start; index < code.length; index += 1) {
    if (code[index] === '(') depth += 1;
    else if (code[index] === ')') {
      depth -= 1;
      if (depth === 0) return index + 1;
    }
  }
  return code.length;
}

// chainEnd extends a TypeScript `expect(...)` call through `.matcher(...)` and `.not` members.
// chainEnd 将 TypeScript `expect(...)` 调用延伸到其后的 `.matcher(...)` 与 `.not` 成员。
function chainEnd(code, start) {
  let end = start;
  for (;;) {
    const member = /^\s*\.\s*[A-Za-z_$][\w$]*/u.exec(code.slice(end, end + 256));
    if (member === null) return end;
    end += member[0].length;
    const call = /^\s*\(/u.exec(code.slice(end, end + 64));
    if (call !== null) end = balancedEnd(code, end + call[0].length);
  }
}

// countWordingAssertions counts assertion statements that compare CJK wording in one file.
// countWordingAssertions 统计单个文件中比较 CJK 文案的断言语句数量。
//
// # Parameters
// 参数
// `source` is the file text and `language` is `rust` or `typescript`.
// `source` 是文件文本，`language` 为 `rust` 或 `typescript`。
//
// # Returns
// 返回值
// The number of counted assertion statements.
// 返回计数的断言语句数量。
export function countWordingAssertions(source, language) {
  const { code, cjkLiterals } = tokenize(source, language);
  if (cjkLiterals.length === 0) return 0;
  const pattern = language === 'rust' ? RUST_ASSERTION : TS_ASSERTION;
  let count = 0;
  let consumed = 0;
  for (const match of code.matchAll(pattern)) {
    if (match.index < consumed) continue;
    let end = balancedEnd(code, match.index + match[0].length);
    if (language !== 'rust') end = chainEnd(code, end);
    consumed = end;
    if (cjkLiterals.some(([start, stop]) => start >= match.index && stop <= end)) count += 1;
  }
  return count;
}

// languageOf maps a scanned file name to its language, or null when the file is not scanned.
// languageOf 将扫描文件名映射到语言；不扫描的文件返回 null。
function languageOf(name) {
  if (name.endsWith('.rs')) return 'rust';
  if (/\.test\.(?:ts|tsx|mts|mjs)$/u.test(name)) return 'typescript';
  return null;
}

// scanRepository counts wording assertions in every scanned file of the repository.
// scanRepository 统计仓库中每个扫描文件的文案断言数量。
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
      const language = languageOf(entry.name);
      if (language === null) continue;
      const count = countWordingAssertions(readFileSync(full, 'utf8'), language);
      if (count > 0) counts.set(path.relative(root, full).split(path.sep).join('/'), count);
    }
  };
  for (const scanRoot of SCAN_ROOTS) visit(path.join(root, scanRoot));
  return counts;
}

// compareWithBaseline lists files above their baseline and files whose count dropped.
// compareWithBaseline 列出超过基线的文件与计数下降的文件。
//
// # Parameters
// 参数
// `counts` is the current scan and `baseline` maps paths to tolerated counts.
// `counts` 是当前扫描结果，`baseline` 将路径映射到可容忍数量。
//
// # Returns
// 返回值
// `{ exceeded, lowered }`, each a list of `{ file, count, allowed }`.
// 返回 `{ exceeded, lowered }`，每项为 `{ file, count, allowed }` 列表。
export function compareWithBaseline(counts, baseline) {
  const exceeded = [];
  const lowered = [];
  const files = new Set([...counts.keys(), ...Object.keys(baseline)]);
  for (const file of [...files].sort()) {
    const count = counts.get(file) ?? 0;
    const allowed = baseline[file] ?? 0;
    if (count > allowed) exceeded.push({ file, count, allowed });
    else if (count < allowed) lowered.push({ file, count, allowed });
  }
  return { exceeded, lowered };
}

// main runs the gate; `--update` records counts that only went down.
// main 运行门禁；`--update` 仅记录下降后的计数。
function main() {
  const update = process.argv.includes('--update');
  const baseline = JSON.parse(readFileSync(BASELINE_PATH, 'utf8'));
  const counts = scanRepository();
  const { exceeded, lowered } = compareWithBaseline(counts, baseline);
  if (exceeded.length > 0) {
    console.error('New assertions compare Chinese wording; assert codes, catalog keys or test ids instead:');
    for (const { file, count, allowed } of exceeded) console.error(`  ${file}: ${count} (allowed ${allowed})`);
    process.exit(1);
  }
  if (lowered.length === 0) {
    console.log('Wording assertion counts are within the baseline.');
    return;
  }
  if (!update) {
    console.error('Wording assertions were removed; run `node scripts/wording-assertions.mjs --update` to lower the baseline:');
    for (const { file, count, allowed } of lowered) console.error(`  ${file}: ${count} (recorded ${allowed})`);
    process.exit(1);
  }
  const next = Object.fromEntries([...counts.entries()].sort(([left], [right]) => left.localeCompare(right)));
  writeFileSync(BASELINE_PATH, `${JSON.stringify(next, null, 2)}\n`);
  console.log(`Lowered the baseline for ${lowered.length} file(s).`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
