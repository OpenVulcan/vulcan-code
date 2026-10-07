// Native browser addresses are normalized on the host so filesystem paths never become web hosts.
// 原生浏览器地址在宿主侧规范化，避免文件系统路径被误当作网页主机。
import { posix } from 'node:path';
import { pathToFileURL } from 'node:url';

// The OS default handler executes scripts and programs, so external file links are limited to display-only types.
// 操作系统默认处理器会执行脚本与程序，因此外部文件链接仅限只读展示类型。
const EXTERNAL_FILE_EXTENSIONS: ReadonlySet<string> = new Set([
  '.html', '.htm', '.xhtml', '.svg', '.pdf', '.png', '.jpg', '.jpeg', '.gif', '.webp', '.bmp', '.avif', '.ico',
  '.txt', '.md', '.json', '.mp4', '.webm', '.mp3', '.wav',
]);

// normalizeLocalBrowserUrl accepts an address and protected origins, returning a navigable canonical URL.
// normalizeLocalBrowserUrl 接受地址和受保护来源，返回可导航的规范 URL。
// Absolute paths become file URLs; unsupported schemes and protected origins throw validation errors.
// 绝对路径转换为文件 URL；不支持的协议和受保护来源抛出校验错误。
export function normalizeLocalBrowserUrl(input: string, blockedOrigins: ReadonlySet<string>): string {
  // Strip paired quotes commonly included by the operating system's copy-path command.
  // 去除操作系统复制路径命令常附带的成对引号。
  const trimmed = input.trim();
  const value = trimmed.startsWith('"') && trimmed.endsWith('"') ? trimmed.slice(1, -1) : trimmed;
  if (!value) throw new Error('Enter a website address or an absolute file path.');
  // Recognize drive paths before URL schemes, including slash-prefixed drive paths from local links.
  // 在 URL 协议之前识别盘符路径，包括本地链接中带前导斜杠的盘符路径。
  const drivePath = /^\/?[a-z]:[\\/]/i.test(value);
  // UNC paths retain their server/share identity through Node's Windows file URL encoder.
  // UNC 路径通过 Node 的 Windows 文件 URL 编码器保留服务器与共享身份。
  const uncPath = value.startsWith('\\\\');
  // Node encodes literal percent, hash, spaces and Unicode without treating them as URL delimiters.
  // Node 编码字面百分号、井号、空格及 Unicode，不将其误作 URL 分隔符。
  let destination: URL;
  if (drivePath || uncPath) {
    destination = pathToFileURL(drivePath && value.startsWith('/') ? value.slice(1) : value, { windows: true });
  } else if (value.startsWith('/')) {
    destination = pathToFileURL(value, { windows: false });
  } else {
    // Development loopback servers use HTTP; ordinary hostnames keep the existing HTTPS default.
    // 本机开发服务器使用 HTTP；普通主机名保留现有 HTTPS 默认行为。
    const loopback = /^(?:localhost|127(?:\.\d{1,3}){3}|\[::1\])(?::|\/|$)/i.test(value);
    // A hostname with a numeric port is not a URI scheme, even though both contain a colon.
    // 带数字端口的主机名不是 URI 协议，尽管两者都包含冒号。
    const hostWithPort = /^[^\s/:?#]+:\d+(?:[/?#]|$)/.test(value);
    const explicitScheme = /^[a-z][a-z\d+.-]*:/i.test(value) && !hostWithPort;
    destination = new URL(explicitScheme ? value : `${loopback ? 'http' : 'https'}://${value}`);
  }
  if (!isAllowedLocalBrowserUrl(destination.href, blockedOrigins)) {
    throw new Error('This address is unavailable in the local browser.');
  }
  return destination.href;
}

// isAllowedLocalBrowserUrl checks a canonical destination against the native navigation policy.
// isAllowedLocalBrowserUrl 按原生导航策略校验规范目标地址。
// target is a URL and blockedOrigins contains privileged service origins; returns false for invalid input.
// target 是 URL，blockedOrigins 包含特权服务来源；非法输入返回 false。
export function isAllowedLocalBrowserUrl(target: string, blockedOrigins: ReadonlySet<string>): boolean {
  try {
    // File pages use the same sandbox as web pages and receive no workbench preload or Node access.
    // 文件页面与网页使用相同沙箱，不获得工作台预加载桥或 Node 访问权。
    const parsed = new URL(target);
    return parsed.href === 'about:blank' || parsed.protocol === 'file:'
      || (['http:', 'https:'].includes(parsed.protocol) && !blockedOrigins.has(parsed.origin));
  } catch {
    return false;
  }
}

// validateExternalBrowserUrl accepts only explicit website/file URLs outside privileged workbench origins.
// validateExternalBrowserUrl 仅接受特权工作台来源之外的明确网页或文件 URL。
// input is the clicked URL and blockedOrigins is the host-owned origin set; returns a canonical URL or throws.
// input 为点击的 URL，blockedOrigins 为宿主持有的来源集合；返回规范 URL，非法时抛错。
export function validateExternalBrowserUrl(input: string, blockedOrigins: ReadonlySet<string>): string {
  // Unlike the address bar, links must already declare their protocol and cannot launch arbitrary OS handlers.
  // 与地址栏不同，链接必须已声明协议，不能调用任意操作系统协议处理器。
  const destination = new URL(input);
  if (!['http:', 'https:', 'file:'].includes(destination.protocol)
    || !isAllowedLocalBrowserUrl(destination.href, blockedOrigins)) {
    throw new Error('Only website or file URLs outside the workbench can be opened externally.');
  }
  if (destination.protocol === 'file:' && !isDisplayOnlyFilePath(destination.pathname)) {
    throw new Error('This file type cannot be opened externally; open it in the built-in browser instead.');
  }
  return destination.href;
}

// isDisplayOnlyFilePath judges the decoded path the OS receives, so encoded dots and stream suffixes cannot hide a type.
// isDisplayOnlyFilePath 按操作系统实际收到的解码路径判断，编码点号与数据流后缀无法隐藏真实类型。
// pathname is a file URL pathname; returns false for malformed encoding or any extension outside the allowlist.
// pathname 为文件 URL 路径；编码非法或扩展名不在允许列表内时返回 false。
function isDisplayOnlyFilePath(pathname: string): boolean {
  try {
    return EXTERNAL_FILE_EXTENSIONS.has(posix.extname(decodeURIComponent(pathname)).toLowerCase());
  } catch {
    return false;
  }
}
