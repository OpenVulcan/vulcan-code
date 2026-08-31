// Read-only evidence extractor for the frozen decomposition review; no source writes or automatic approvals.
// 冻结拆分审核的只读证据提取器；不写入源码，也不自动判定审核通过。
"use strict";
// Standard-library dependencies for files, paths, hashes and the original Git source.
// 文件、路径、摘要与原始Git源码所需的标准库依赖。
const fs = require("node:fs"), path = require("node:path"), crypto = require("node:crypto"), cp = require("node:child_process");
// Resolve the repository from this review directory, not from the caller's working directory.
// 从审核目录定位仓库，不依赖调用方工作目录。
const root = path.resolve(__dirname, "../../..");
// The immutable source commit established before the first split.
// 首次拆分前已确定的不可变源码提交。
const revision = "111540689b777ddc1229a6f17293a42818f2155e";
// Cache lexical evidence per exact source text during this short-lived read-only process.
// 在本次短生命周期只读进程中按精确源码文本缓存词法证据。
const tokenCache = new Map();

/** Tokenize Rust text; input is source, output preserves literals and offsets while skipping comments/spacing.
 * 对Rust源码分词；输入源码，输出保留字面量及位置的词元，仅跳过注释和空白。 */
function tokens(source) {
  if (tokenCache.has(source)) return tokenCache.get(source);
  // Cursor and accumulated token records; literal contents are never normalized.
  // 游标与累计词元记录；绝不归一化字面量内容。
  let i = 0; const result = [];
  while (i < source.length) {
    if (/\s/.test(source[i])) { i++; continue; }
    if (source.startsWith("//", i)) {
      // A line comment ends at the next newline or EOF.
      // 行注释在下一换行或文件结尾结束。
      const end = source.indexOf("\n", i); i = end < 0 ? source.length : end; continue;
    }
    if (source.startsWith("/*", i)) {
      // Rust block comments nest; unmatched comments are errors.
      // Rust块注释允许嵌套；未闭合注释必须报错。
      let depth = 1; i += 2;
      while (i < source.length && depth) {
        if (source.startsWith("/*", i)) { depth++; i += 2; }
        else if (source.startsWith("*/", i)) { depth--; i += 2; }
        else i++;
      }
      if (depth) throw new Error("Unclosed block comment");
      continue;
    }
    // Match literals before identifiers so raw strings remain one exact token.
    // 先匹配字面量再匹配标识符，确保原始字符串保持一个精确词元。
    const start = i, tail = source.slice(i), raw = /^(?:br|cr|r)(#*)"/.exec(tail);
    if (raw) {
      // A raw literal closes with its exact hash count.
      // 原始字面量使用完全相同数量的井号闭合。
      const close = '"' + raw[1], end = source.indexOf(close, i + raw[0].length);
      if (end < 0) throw new Error("Unclosed raw literal");
      i = end + close.length;
    } else if (/^(?:b|c)?"/.test(tail)) {
      i += tail[0] === '"' ? 1 : 2;
      while (i < source.length && source[i] !== '"') { i += source[i] === "\\" ? 2 : 1; }
      if (i >= source.length) throw new Error("Unclosed string");
      i++;
    } else {
      // Distinguish character literals from lifetimes without consuming their identifiers.
      // 区分字符字面量与生命周期，不吞掉生命周期标识符。
      const character = /^(?:b)?'(?:\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|.)|[^'\\\r\n])'/.exec(tail);
      const word = /^[A-Za-z_][A-Za-z_0-9]*|^[0-9][A-Za-z_0-9]*/.exec(tail);
      i += character ? character[0].length : word ? word[0].length : 1;
    }
    result.push({ value: source.slice(start, i), start, end: i });
  }
  tokenCache.set(source, result);
  return result;
}

/** Extract one declaration at a verified one-based line; return source and exact lexical sequence.
 * 在已确认的一基行号提取完整声明；返回源码与精确词法序列。 */
function declaration(source, line) {
  if (!Number.isInteger(line) || line < 1) throw new Error("A verified positive line is required");
  // Line offset and balanced delimiters bound one item, including its full method body.
  // 行偏移与配对分隔符限定一个条目，包含完整方法体。
  const offset = source.split("\n").slice(0, line - 1).join("\n").length + (line > 1 ? 1 : 0);
  const list = tokens(source); const stack = [];
  // Locate the declaration in the one shared token sequence instead of lexing every suffix.
  // 在唯一共享词元序列中定位声明，避免对每个后缀重复分词。
  const first = list.findIndex(token => token.start >= offset);
  if (first < 0) throw new Error("Declaration line is outside source");
  let end = -1, body = false;
  for (let index = first; index < list.length; index++) {
    // Current lexical delimiter; parentheses/brackets in signatures cannot end an item.
    // 当前词法分隔符；签名中的圆括号与方括号不能结束条目。
    const value = list[index].value;
    if ("([{".includes(value) && value.length === 1) {
      if (value === "{" && stack.length === 0) body = true;
      stack.push(value);
    } else if (")]}".includes(value) && value.length === 1) {
      if (stack.pop() !== ({ ")": "(", "]": "[", "}": "{" })[value]) throw new Error("Unbalanced declaration");
      if (body && stack.length === 0) { end = index; break; }
    } else if (value === ";" && stack.length === 0) { end = index; break; }
  }
  if (end < 0) throw new Error("Cannot bound declaration at line " + line);
  return { source: source.slice(offset, list[end].end), values: list.slice(first, end + 1).map(t => t.value) };
}

/** Read an exact original path from the frozen commit; return UTF-8 source without altering Git state.
 * 从冻结提交读取精确原始路径；返回UTF-8源码，不修改Git状态。 */
function original(file) {
  return cp.execFileSync("rtk", ["proxy", "git", "show", revision + ":" + file], {
    cwd: root, encoding: "utf8", maxBuffer: 20 * 1024 * 1024, windowsHide: true
  });
}

/** Hash bytes or text with SHA-256; return lowercase hexadecimal evidence.
 * 对字节或文本计算SHA-256；返回小写十六进制证据。 */
function hash(content) { return crypto.createHash("sha256").update(content).digest("hex"); }

/** Read a repository-relative file; return UTF-8 text and never probe alternate paths.
 * 读取仓库相对文件；返回UTF-8文本，不探测备用路径。 */
function read(file) { return fs.readFileSync(path.join(root, file), "utf8"); }

/** Enumerate Rust files under one exact directory; return sorted repository-relative paths.
 * 枚举一个精确目录中的Rust文件；返回排序后的仓库相对路径。 */
function rustFiles(directory) {
  return fs.readdirSync(path.join(root, directory), { withFileTypes: true }).flatMap(entry =>
    entry.isDirectory() ? rustFiles(directory + "/" + entry.name) :
      entry.name.endsWith(".rs") ? [directory + "/" + entry.name] : []
  ).sort();
}

/** Collect line-start Rust declarations using lexical offsets; input is source, output includes nested items.
 * 依据词法位置收集行首Rust声明；输入源码，输出包含嵌套条目。 */
function declarations(source, file) {
  // Token starts exclude declaration-looking text inside raw literals and comments.
  // 词元起点排除原始字面量和注释中外形类似声明的文本。
  const starts = new Set(tokens(source).map(token => token.start));
  const result = []; let offset = 0;
  for (const [index, line] of source.split("\n").entries()) {
    // Declaration grammar here is only a candidate filter, not a Rust parser or an ownership authority.
    // 此处声明语法只用于候选筛选，不充当Rust解析器或所有权权威。
    const match = /^(\s*)(?:pub(?:\([^)]*\))?\s+)?(?:(?:async|unsafe|const)\s+)?(fn|struct|enum|type|trait)\s+([A-Za-z_][A-Za-z_0-9]*)/.exec(line);
    if (match && starts.has(offset + match[1].length)) {
      result.push({ file, start: index + 1, kind: match[2], name: match[3], ...declaration(source, index + 1) });
    }
    offset += line.length + 1;
  }
  return result;
}

/** Collect exact identity and body differences for verified mappings; return evidence, never a review verdict.
 * 为已核实映射收集精确身份及函数体差异；返回证据，绝不代替审核结论。 */
function compare(nodes, originalPath) {
  // One original source and file cache serve the whole mapped family.
  // 同一原始源码与文件缓存服务完整映射范围。
  const before = original(originalPath), cache = new Map(), results = [];
  for (const node of nodes) {
    if (!Number.isInteger(node.original)) throw new Error("Missing original identity: " + node.id);
    if (!cache.has(node.file)) cache.set(node.file, read(node.file));
    // Original/current declarations and leading-only visibility normalization.
    // 原始与当前声明，以及仅限声明开头的可见性归一化。
    const a = declaration(before, node.original), b = declaration(cache.get(node.file), node.start);
    const normalize = values => {
      const out = values.slice();
      if (out[0] === "pub" && out[1] === "(") out.splice(0, out.indexOf(")") + 1);
      return out;
    };
    const left = normalize(a.values), right = normalize(b.values);
    let prefix = 0, suffix = 0;
    while (prefix < left.length && prefix < right.length && left[prefix] === right[prefix]) prefix++;
    while (suffix < left.length - prefix && suffix < right.length - prefix &&
      left[left.length - suffix - 1] === right[right.length - suffix - 1]) suffix++;
    results.push({
      ...node, equal: prefix === left.length && prefix === right.length,
      beforeHash: hash(JSON.stringify(left)), afterHash: hash(JSON.stringify(right)),
      difference: { index: prefix, before: left.slice(prefix, left.length - suffix), after: right.slice(prefix, right.length - suffix) }
    });
  }
  return results;
}

/** Compare an entire frozen source family by declaration multisets; scope selects exact files, output lists every residual.
 * 按声明多重集合比较完整冻结源码族；scope选择精确文件，输出列出全部剩余差异。 */
function familyEvidence(scope) {
  // Source family membership is explicit; the session document excludes its pre-existing generation/recovery modules.
  // 源码族成员明确指定；会话文档不把既有generation/recovery模块算作拆分产物。
  const directory = scope.original.slice(0, -3);
  const files = [scope.original, ...(scope.children ? scope.children.map(name => directory + "/" + name + ".rs") : rustFiles(directory))];
  const before = declarations(original(scope.original), scope.original);
  const after = files.flatMap(file => declarations(read(file), file));
  // Restricted declaration visibility is inspected separately; all body tokens remain exact.
  // 受限声明可见性另行审核；函数体所有词元保持精确比较。
  const normalize = item => {
    const values = item.values.slice();
    if (values[0] === "pub" && values[1] === "(") values.splice(0, values.indexOf(")") + 1);
    return values;
  };
  const remaining = after.map(item => ({ ...item, normalized: normalize(item) }));
  const unmatched = []; let exact = 0;
  for (const item of before) {
    // Consume only one identical declaration, so duplicated or omitted copies cannot cancel silently.
    // 只消费一个完全相同的声明，避免重复或遗漏副本被静默抵消。
    const values = normalize(item), signature = JSON.stringify(values);
    const index = remaining.findIndex(candidate => JSON.stringify(candidate.normalized) === signature);
    if (index >= 0) { remaining.splice(index, 1); exact++; }
    else unmatched.push({ ...item, normalized: values });
  }
  return {
    id: scope.id, files, original: before.length, current: after.length, exact,
    before: unmatched.map(({ source, values, ...item }) => item),
    after: remaining.map(({ source, values, ...item }) => item)
  };
}

/** Display bounded edit hunks without approving them; left/right are token arrays, output preserves every changed span.
 * 展示有界差异块但不批准差异；输入为前后词元数组，输出保留全部变动区间。 */
function editHunks(left, right) {
  // Exact common runs partition display only; comparison still uses the full token sequences.
  // 精确共同片段仅用于分块展示；核验仍使用完整词元序列。
  const result = []; let i = 0, j = 0;
  while (i < left.length || j < right.length) {
    if (i < left.length && j < right.length && left[i] === right[j]) { i++; j++; continue; }
    // Find the nearest exact eight-token anchor; absent anchors retain the entire remaining difference.
    // 查找最近的精确八词元锚点；没有锚点时保留全部剩余差异。
    let next = null;
    for (let distance = 1; distance <= 120 && !next; distance++) {
      for (let delta = 0; delta <= distance; delta++) {
        const x = i + delta, y = j + distance - delta;
        if (x + 8 <= left.length && y + 8 <= right.length &&
            left.slice(x, x + 8).every((value, index) => value === right[y + index])) { next = [x, y]; break; }
      }
    }
    if (!next) next = [left.length, right.length];
    result.push({ left: i, right: j, before: left.slice(i, next[0]), after: right.slice(j, next[1]) });
    [i, j] = next;
  }
  return result;
}
// Export the small read-only primitives for command-specific verification; direct execution is intentionally inert.
// 导出少量只读原语供专项核验使用；直接执行不进行任何修改或判定。
module.exports = { root, revision, tokens, declaration, declarations, original, hash, read, rustFiles, compare, familyEvidence, editHunks };
