// Turn the English release notes, written for the in-app renderer, into plain GitHub Markdown.
// 将为应用内渲染器编写的英文更新记录转换为纯 GitHub Markdown。
//
// The notes use answer components (`ui-*` fences, grammar in
// `resources/agent-prompts/answer-mode-interactive.md`) that only Vulcan Code renders. A GitHub
// Release shows them as raw code blocks, so the release body is converted: rows become lists,
// `ui-callout` becomes a GitHub alert and `ui-tabs` sections become headings. A component this
// module cannot convert is an error, and `release-notes-check.mjs` runs the conversion on every
// release so that `make check` reports it before a release does.
// 更新记录使用只有 Vulcan Code 能渲染的回答组件（`ui-*` 围栏，语法见
// `resources/agent-prompts/answer-mode-interactive.md`）。GitHub Release 会把它们显示为原始代码块，
// 因此发布正文需要转换：行组件变为列表，`ui-callout` 变为 GitHub 提示块，`ui-tabs` 各节变为标题。
// 本模块无法转换的组件即为错误；`release-notes-check.mjs` 对每个发布执行转换，使 `make check`
// 在发布之前报告问题。
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// The English notes are the GitHub Release body; other languages stay in the app.
// 英文说明是 GitHub Release 正文；其他语言只留在应用内。
export const GITHUB_NOTES_LANGUAGE = 'en-US';

// Checklist statuses and row tones in the order of their meaning, each with the mark GitHub shows.
// 清单状态与行色调对应的 GitHub 显示标记。
const STATUS_MARKS = { ok: '✅', warn: '⚠️', no: '❌', todo: '⬜' };
const TONE_MARKS = { ok: '✅', warn: '⚠️', no: '❌' };

// Callout tones mapped to GitHub alert types.
// 提示块色调到 GitHub 提示类型的映射。
const ALERT_TYPES = { info: 'NOTE', success: 'TIP', warning: 'WARNING', danger: 'CAUTION' };

// fenceOpening returns the marker and info string of a fence opening line, or null.
// fenceOpening 返回围栏起始行的标记与信息字符串，不是起始行时返回 null。
function fenceOpening(line) {
  const match = /^ {0,3}(`{3,}|~{3,})(.*)$/.exec(line);
  if (!match) return null;
  if (match[1][0] === '`' && match[2].includes('`')) return null;
  return { marker: match[1], info: match[2].trim() };
}

// closes reports whether `line` ends a fence opened with `marker`.
// closes 判断 `line` 是否结束以 `marker` 开启的围栏。
function closes(line, marker) {
  const match = /^ {0,3}(`{3,}|~{3,})\s*$/.exec(line);
  return Boolean(match) && match[1][0] === marker[0] && match[1].length >= marker.length;
}

// blocks splits lines into text lines and fences, each fence with its info string and body lines.
// blocks 将行拆分为普通文本行与围栏，每个围栏带有信息字符串和正文行。
function blocks(lines) {
  const result = [];
  for (let index = 0; index < lines.length; index += 1) {
    const opening = fenceOpening(lines[index]);
    if (!opening) {
      result.push({ text: lines[index] });
      continue;
    }
    let end = index + 1;
    while (end < lines.length && !closes(lines[end], opening.marker)) end += 1;
    if (end === lines.length) throw new Error(`unclosed fence \`${lines[index].trim()}\``);
    result.push({ info: opening.info, source: lines.slice(index, end + 1), body: lines.slice(index + 1, end) });
    index = end;
  }
  return result;
}

// rows parses a row component body: one row per nonblank line, cells split on unescaped `|`,
// outer pipes optional, a Markdown separator row ignored and `\|` restored to `|`.
// rows 解析行组件正文：每个非空行一行，按未转义的 `|` 分格，外侧竖线可省略，忽略 Markdown
// 分隔行，并将 `\|` 还原为 `|`。
function rows(body, component) {
  return body.filter((line) => line.trim() !== '' && !/^\s*\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)*\|?\s*$/.test(line))
    .map((line) => {
      const cells = line.trim().split(/(?<!\\)\|/).map((cell) => cell.trim().replace(/\\\|/g, '|'));
      if (line.trim().startsWith('|')) cells.shift();
      if (line.trim().endsWith('|') && !line.trim().endsWith('\\|')) cells.pop();
      if (cells.some((cell) => cell === '')) throw new Error(`${component}: empty cell in \`${line.trim()}\``);
      return cells;
    });
}

// expectCells rejects a row whose cell count is outside `minimum`..`maximum`.
// expectCells 拒绝单元格数量不在 `minimum`..`maximum` 之间的行。
function expectCells(row, minimum, maximum, component) {
  if (row.length < minimum || row.length > maximum) {
    throw new Error(`${component}: \`${row.join(' | ')}\` needs ${minimum} to ${maximum} cells`);
  }
}

// toned splits a leading tone word off a cell, returning its mark and the remaining text.
// toned 从单元格开头拆出色调词，返回其标记与剩余文本。
function toned(cell) {
  const match = /^(ok|warn|no)(?:\s+(.*))?$/.exec(cell);
  return match ? { mark: TONE_MARKS[match[1]], text: match[2] ?? '' } : { mark: '', text: cell };
}

// detail joins a bold lead with an optional note.
// detail 将加粗的开头与可选说明拼接。
function detail(lead, note) {
  return note ? `**${lead}** — ${note}` : `**${lead}**`;
}

// sections splits a `ui-tabs` or `ui-columns` body at its top-level `### Title` lines.
// sections 按顶层 `### 标题` 行拆分 `ui-tabs` 或 `ui-columns` 正文。
function sections(body, component) {
  const result = [];
  for (const block of blocks(body)) {
    const heading = block.text === undefined ? null : /^###\s+(.+?)\s*$/.exec(block.text);
    if (heading) {
      result.push({ title: heading[1], lines: [] });
    } else if (result.length === 0) {
      if (block.text === undefined || block.text.trim() !== '') throw new Error(`${component}: content before the first \`### \` section`);
    } else {
      result.at(-1).lines.push(...(block.text === undefined ? block.source : [block.text]));
    }
  }
  if (result.length < 2) throw new Error(`${component}: needs at least two \`### \` sections`);
  return result;
}

// component converts one `ui-*` fence into GitHub Markdown lines.
// component 将一个 `ui-*` 围栏转换为 GitHub Markdown 行。
function component(info, body) {
  const [name, ...rest] = info.split(/\s+/);
  switch (name) {
    case 'ui-kv':
      return rows(body, name).map((row) => {
        expectCells(row, 2, 2, name);
        return `- **${row[0]}:** ${row[1]}`;
      });
    case 'ui-checklist':
      return rows(body, name).map((row) => {
        expectCells(row, 2, 3, name);
        const mark = STATUS_MARKS[row[0]];
        if (!mark) throw new Error(`${name}: unknown status \`${row[0]}\``);
        return `- ${mark} ${detail(row[1], row[2])}`;
      });
    case 'ui-cards':
      return rows(body, name).map((row) => {
        expectCells(row, 2, 3, name);
        const tag = row[2] ? toned(row[2]) : null;
        const label = tag ? ` _(${[tag.mark, tag.text].filter(Boolean).join(' ')})_` : '';
        return `- ${detail(row[0], row[1])}${label}`;
      });
    case 'ui-callout': {
      const type = ALERT_TYPES[rest[0]];
      if (!type) throw new Error(`ui-callout: unknown tone \`${rest[0] ?? ''}\``);
      const title = rest.slice(1).join(' ');
      const content = [...(title ? [`**${title}**`, ''] : []), ...convert(body)];
      return [`> [!${type}]`, ...content.map((line) => (line === '' ? '>' : `> ${line}`))];
    }
    case 'ui-tabs':
    case 'ui-columns':
      return sections(body, name).flatMap((section) => ['', `#### ${section.title}`, '', ...convert(section.lines), '']);
    default:
      throw new Error(`\`${name}\` has no GitHub form; extend scripts/release-notes-github.mjs or use Markdown`);
  }
}

// convert turns note lines into GitHub Markdown lines, leaving ordinary fences untouched.
// convert 将说明行转换为 GitHub Markdown 行，普通围栏保持原样。
function convert(lines) {
  const output = [];
  for (const block of blocks(lines)) {
    if (block.text !== undefined) output.push(block.text);
    else if (block.info.startsWith('ui-')) output.push('', ...component(block.info, block.body), '');
    else output.push(...block.source);
  }
  return output;
}

// githubReleaseNotes converts one notes document; it throws on a component without a GitHub form.
// githubReleaseNotes 转换一份说明文档；遇到没有 GitHub 形式的组件时抛出错误。
export function githubReleaseNotes(markdown) {
  const lines = convert(markdown.replace(/\r\n/g, '\n').split('\n'));
  return `${lines.join('\n').replace(/\n{3,}/g, '\n\n').trim()}\n`;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) {
  const version = process.argv[2];
  if (!version) {
    console.error('usage: node scripts/release-notes-github.mjs <version>');
    process.exit(2);
  }
  const root = fileURLToPath(new URL('..', import.meta.url));
  const source = path.join(root, 'crates', 'vulcan-runtime-assets', 'resources', 'changelog', version, `${GITHUB_NOTES_LANGUAGE}.md`);
  process.stdout.write(githubReleaseNotes(readFileSync(source, 'utf8')));
}
