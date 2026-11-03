// Link literals are web addresses, file: URLs and absolute file paths written as plain transcript text. They become
// links only where their end is unambiguous: web addresses keep to URL characters, so whitespace, CJK text and
// full-width punctuation end them; file references must end in a file extension.
// 链接字面量是以普通转录文本写出的网址、file: URL 与绝对文件路径。只有结尾明确时才转为链接：网址只含 URL 字符，
// 遇到空白、中日韩文字与全角标点即结束；文件引用必须以文件扩展名结尾。

// LinkLiteral is one link found in plain text: the text range it covers and where it leads.
// LinkLiteral 是在普通文本中找到的单个链接：其覆盖的文本范围及目标地址。
export interface LinkLiteral {
  // Offset of the first character.
  // 首字符偏移。
  start: number;
  // Offset just past the last character.
  // 末字符之后的偏移。
  end: number;
  // Link target: the web address (www. gains https://) or a file: URL.
  // 链接目标：网址（www. 补全 https://）或 file: URL。
  href: string;
}

// FULL_WIDTH_PUNCTUATION lists CJK and full-width punctuation, plus curly quotes, ellipses and dashes, none of which
// belongs to a path or address written in prose.
// FULL_WIDTH_PUNCTUATION 列出中日韩与全角标点以及弯引号、省略号与破折号；正文中的路径或地址都不含这些字符。
const FULL_WIDTH_PUNCTUATION = "\\u2014\\u2018-\\u201F\\u2026\\u3001-\\u3003\\u3008-\\u3011\\u3014-\\u301F\\uFF01-\\uFF0F\\uFF1A-\\uFF20\\uFF3B-\\uFF40\\uFF5B-\\uFF65";

// SEGMENT matches one character of a path segment: anything but whitespace, separators, characters Windows forbids
// in names, "#", backticks and full-width punctuation. CJK letters stay, as folder and file names often use them.
// SEGMENT 匹配路径段中的单个字符：空白、分隔符、Windows 禁止用于名称的字符、"#"、反引号与全角标点之外的任何字符。
// 中日韩文字保留，因为文件夹与文件名常用它们。
const SEGMENT = `[^\\s\\\\/<>:"|?*#\`${FULL_WIDTH_PUNCTUATION}]`;

// FILE_NAME matches a file name ending in a letter-led extension that no letter or digit continues.
// FILE_NAME 匹配以字母开头的扩展名结尾、且其后不再紧跟字母或数字的文件名。
const FILE_NAME = `${SEGMENT}*\\.[A-Za-z][A-Za-z0-9]{0,9}(?![A-Za-z0-9])`;

// LINE_SUFFIX matches an optional ":12" or ":12-20" line suffix.
// LINE_SUFFIX 匹配可选的 ":12" 或 ":12-20" 行后缀。
const LINE_SUFFIX = "(?::\\d+(?:[-\\u2013]\\d+)?(?!\\d))?";

// WINDOWS_PATH matches a drive-letter path to a file, with either separator.
// WINDOWS_PATH 匹配指向文件的盘符路径，两种分隔符均可。
const WINDOWS_PATH = new RegExp(`[A-Za-z]:[\\\\/](?:${SEGMENT}+[\\\\/])*${FILE_NAME}${LINE_SUFFIX}`, "y");

// POSIX_PATH matches a rooted path to a file inside at least one folder, so prose such as "/etc" or "/a.b" stays text.
// POSIX_PATH 匹配位于至少一层文件夹内、指向文件的根路径，使 "/etc" 或 "/a.b" 之类的正文保持为文本。
const POSIX_PATH = new RegExp(`\\/(?:${SEGMENT}+\\/)+${FILE_NAME}${LINE_SUFFIX}`, "y");

// FILE_URL matches a file: URL to a file on a drive or under the root, with an optional line suffix or "#L" fragment.
// FILE_URL 匹配指向盘符下或根路径下文件的 file: URL，可带行后缀或 "#L" 片段。
const FILE_URL = new RegExp(
  `file:\\/\\/(?:\\/?[A-Za-z]:[\\\\/]|\\/)(?:${SEGMENT}+[\\\\/])*${FILE_NAME}${LINE_SUFFIX}(?:#L\\d+(?:-L?\\d+)?(?!\\d))?`,
  "iy",
);

// WEB_ADDRESS matches an http(s) or www. address followed by URL characters only (RFC 3986 unreserved, reserved and
// "%"); trailing punctuation is trimmed afterwards.
// WEB_ADDRESS 匹配 http(s) 或 www. 地址，其后只含 URL 字符（RFC 3986 非保留字符、保留字符与 "%"）；随后再修剪末尾标点。
const WEB_ADDRESS = /(?:https?:\/\/|www\.)[A-Za-z0-9\-._~:/?#[\]@!$&'()*+,;=%]+/iy;

// CANDIDATE_START finds the places where a link literal may begin.
// CANDIDATE_START 查找链接字面量可能开始的位置。
const CANDIDATE_START = /https?:\/\/|www\.|file:\/\/|[A-Za-z]:[\\/]|\//gi;

// TRAILING_PUNCTUATION ends sentences and clauses rather than addresses.
// TRAILING_PUNCTUATION 结束的是句子与分句，而不是地址。
const TRAILING_PUNCTUATION = ".,;:!?'*";

// findLinkLiterals returns the link literals in `text`, in order and without overlaps.
// findLinkLiterals 按顺序返回 `text` 中互不重叠的链接字面量。
export function findLinkLiterals(text: string): LinkLiteral[] {
  const literals: LinkLiteral[] = [];
  CANDIDATE_START.lastIndex = 0;
  for (let found = CANDIDATE_START.exec(text); found; found = CANDIDATE_START.exec(text)) {
    const literal = linkLiteralAt(text, found.index);
    if (literal) {
      literals.push(literal);
      CANDIDATE_START.lastIndex = literal.end;
    } else {
      CANDIDATE_START.lastIndex = found.index + 1;
    }
  }
  return literals;
}

// linkLiteralAt reads the link literal starting exactly at `start`, or returns null.
// linkLiteralAt 读取恰好从 `start` 开始的链接字面量；不存在时返回 null。
function linkLiteralAt(text: string, start: number): LinkLiteral | null {
  const before = start > 0 ? text[start - 1] : "";
  if (/[A-Za-z0-9_]/.test(before)) return null;
  const web = stickyMatch(WEB_ADDRESS, text, start);
  if (web !== null) {
    const address = trimTrailingPunctuation(web);
    if (/[\-.@/]/.test(before) || !hasValidHost(address)) return null;
    return { start, end: start + address.length, href: /^www\./i.test(address) ? `https://${address}` : address };
  }
  const fileUrl = stickyMatch(FILE_URL, text, start);
  if (fileUrl !== null) return { start, end: start + fileUrl.length, href: fileUrl };
  const path = stickyMatch(WINDOWS_PATH, text, start)
    ?? (/[\-.~:/\\@]/.test(before) ? null : stickyMatch(POSIX_PATH, text, start));
  if (path === null) return null;
  const urlPath = path.replace(/%/g, "%25").replace(/\\/g, "/").replace(/^\/+/, "");
  return { start, end: start + path.length, href: `file:///${urlPath}` };
}

// stickyMatch runs a sticky pattern at `start` and returns the matched text, or null.
// stickyMatch 在 `start` 处运行粘性正则并返回匹配文本；不匹配时返回 null。
function stickyMatch(pattern: RegExp, text: string, start: number): string | null {
  pattern.lastIndex = start;
  return pattern.exec(text)?.[0] ?? null;
}

// trimTrailingPunctuation drops sentence punctuation from the end of an address, and closing brackets that have no
// opening partner inside it, so "(see https://a.com/x)." keeps "https://a.com/x".
// trimTrailingPunctuation 去掉地址末尾的句读标点及在地址内没有配对开括号的闭括号，使 "(see https://a.com/x)."
// 保留 "https://a.com/x"。
function trimTrailingPunctuation(address: string): string {
  let end = address.length;
  while (end > 0) {
    const last = address[end - 1];
    const body = address.slice(0, end);
    if (TRAILING_PUNCTUATION.includes(last)) end -= 1;
    else if (last === ")" && count(body, "(") < count(body, ")")) end -= 1;
    else if (last === "]" && count(body, "[") < count(body, "]")) end -= 1;
    else break;
  }
  return address.slice(0, end);
}

// count returns how often `character` occurs in `text`.
// count 返回 `character` 在 `text` 中出现的次数。
function count(text: string, character: string): number {
  return text.split(character).length - 1;
}

// hasValidHost accepts addresses whose host is localhost, an IP address, a name with a port, or a dotted name
// ending in an alphabetic top-level domain, so "https://exa" (still streaming) and "www.x" stay text.
// hasValidHost 接受主机为 localhost、IP 地址、带端口的名称或以字母顶级域结尾的点分名称的地址，使仍在流式输出的
// "https://exa" 与 "www.x" 保持为文本。
function hasValidHost(address: string): boolean {
  const authority = address.replace(/^https?:\/\//i, "").split(/[/?#]/, 1)[0];
  const hostAndPort = authority.slice(authority.lastIndexOf("@") + 1);
  if (/^\[[0-9A-Fa-f:.]+\](?::\d+)?$/.test(hostAndPort)) return true;
  const parts = /^([^:]+)(?::(\d+))?$/.exec(hostAndPort);
  if (!parts) return false;
  const [, host, port] = parts;
  const labels = host.split(".");
  if (!labels.every((label) => /^[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?$/.test(label))) return false;
  if (port !== undefined || host.toLowerCase() === "localhost" || /^\d{1,3}(?:\.\d{1,3}){3}$/.test(host)) return true;
  return labels.length >= 2 && /^(?:[A-Za-z]{2,63}|xn--[A-Za-z0-9-]+)$/.test(labels[labels.length - 1]);
}

// MarkdownTreeNode is the part of a Markdown syntax tree node the link literal pass reads and edits.
// MarkdownTreeNode 是链接字面量处理读取和修改的 Markdown 语法树节点部分。
interface MarkdownTreeNode {
  type: string;
  value?: string;
  url?: string;
  title?: string | null;
  children?: MarkdownTreeNode[];
  position?: { start: { offset?: number } };
}

// LITERAL_FREE_NODES never hold link literals: their text is code, markup or already a link.
// LITERAL_FREE_NODES 从不含链接字面量：其文本是代码、标记或已经是链接。
const LITERAL_FREE_NODES = new Set(["code", "definition", "html", "inlineCode", "link", "linkReference"]);

// remarkLinkLiterals runs after remark-gfm. It turns the addresses GFM linked from plain text back into text, since
// GFM extends them over CJK text and full-width punctuation, then links every link literal in text nodes. Written
// links (`[text](url)` and `<url>`) and email addresses stay as they are.
// remarkLinkLiterals 在 remark-gfm 之后运行。它把 GFM 从普通文本链接出的地址还原为文本（GFM 会将其延伸到中日韩文字与
// 全角标点），再为文本节点中的所有链接字面量建立链接。手写链接（`[text](url)` 与 `<url>`）与电子邮件地址保持不变。
export function remarkLinkLiterals() {
  return (tree: MarkdownTreeNode, file: { value: unknown }) => {
    const source = String(file.value);
    const visit = (node: MarkdownTreeNode) => {
      if (!node.children) return;
      node.children = linkLiteralChildren(node.children, source);
      for (const child of node.children) if (!LITERAL_FREE_NODES.has(child.type)) visit(child);
    };
    visit(tree);
  };
}

// linkLiteralChildren unwraps GFM's plain-text links among `children`, merges the text they leave and splits each
// text node around its link literals.
// linkLiteralChildren 还原 `children` 中由 GFM 从普通文本生成的链接，合并由此留下的文本，并按链接字面量拆分每个文本节点。
function linkLiteralChildren(children: MarkdownTreeNode[], source: string): MarkdownTreeNode[] {
  const merged: MarkdownTreeNode[] = [];
  for (const child of children) {
    const node = isPlainTextLink(child, source)
      ? { type: "text", value: (child.children ?? []).map((part) => part.value ?? "").join("") }
      : child;
    const previous = merged[merged.length - 1];
    if (node.type === "text" && previous?.type === "text") {
      merged[merged.length - 1] = { type: "text", value: (previous.value ?? "") + (node.value ?? "") };
    } else {
      merged.push(node);
    }
  }
  return merged.flatMap((node) => (node.type === "text" ? splitTextNode(node) : [node]));
}

// isPlainTextLink tells whether `node` is a link GFM made from a bare web address: its source does not start with
// "[" or "<", and it is not an email address.
// isPlainTextLink 判断 `node` 是否为 GFM 由裸网址生成的链接：其源文本不以 "[" 或 "<" 开头，且不是电子邮件地址。
function isPlainTextLink(node: MarkdownTreeNode, source: string): boolean {
  const offset = node.position?.start.offset;
  if (node.type !== "link" || offset === undefined || /^mailto:/i.test(node.url ?? "")) return false;
  return source[offset] !== "[" && source[offset] !== "<";
}

// splitTextNode returns `node` split into text and link nodes around its link literals.
// splitTextNode 返回按链接字面量拆分为文本与链接节点的 `node`。
function splitTextNode(node: MarkdownTreeNode): MarkdownTreeNode[] {
  const text = node.value ?? "";
  const literals = findLinkLiterals(text);
  if (literals.length === 0) return [node];
  const parts: MarkdownTreeNode[] = [];
  let cursor = 0;
  for (const literal of literals) {
    if (literal.start > cursor) parts.push({ type: "text", value: text.slice(cursor, literal.start) });
    parts.push({
      type: "link",
      url: literal.href,
      title: null,
      children: [{ type: "text", value: text.slice(literal.start, literal.end) }],
    });
    cursor = literal.end;
  }
  if (cursor < text.length) parts.push({ type: "text", value: text.slice(cursor) });
  return parts;
}
