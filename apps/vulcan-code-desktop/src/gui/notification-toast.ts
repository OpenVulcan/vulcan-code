// Build an explicit Windows toast launch argument so Electron can recover the exact notification id.
// 显式生成 Windows 通知启动参数，使 Electron 能恢复精确的通知 ID。

// escapeToastText encodes the XML text nodes used by the native toast template.
// escapeToastText 编码原生通知模板中的 XML 文本节点。
// value is untrusted notification copy; the returned text is safe inside an XML element.
// value 是不可信的通知文案；返回值可安全放入 XML 元素。
function escapeToastText(value: string): string {
  // XML 1.0 rejects most control characters and lone UTF-16 surrogates even inside text nodes.
  // XML 1.0 即使在文本节点中也拒绝大多数控制字符和孤立的 UTF-16 代理码元。
  // escaped accumulates only valid XML text and entity references.
  // escaped 仅累积合法的 XML 文本和实体引用。
  let escaped = '';
  // character iterates Unicode code points so valid surrogate pairs stay intact.
  // character 按 Unicode 码点迭代，以保留合法代理对。
  for (const character of value) {
    // scalar is the current Unicode scalar value used by the XML 1.0 allowlist.
    // scalar 是按 XML 1.0 白名单判断的当前 Unicode 标量值。
    const scalar = character.codePointAt(0)!;
    if (!(scalar === 9 || scalar === 10 || scalar === 13
      || (scalar >= 0x20 && scalar <= 0xD7FF)
      || (scalar >= 0xE000 && scalar <= 0xFFFD)
      || (scalar >= 0x10000 && scalar <= 0x10FFFF))) continue;
    escaped += character === '&' ? '&amp;'
      : character === '<' ? '&lt;'
        : character === '>' ? '&gt;' : character;
  }
  return escaped;
}

// createWindowsToastXml preserves the id in the toast's body-click launch argument.
// createWindowsToastXml 将 ID 放入通知主体点击的启动参数。
// id is the validated ASCII notification id; title and body are displayed text; returns WinRT toast XML.
// id 是已验证的 ASCII 通知 ID，title 和 body 是显示文案；返回 WinRT 通知 XML。
export function createWindowsToastXml(id: string, title: string, body: string): string {
  return `<toast launch="type=click&amp;tag=${id}"><visual><binding template="ToastGeneric">`
    + `<text>${escapeToastText(title)}</text><text>${escapeToastText(body)}</text>`
    + '</binding></visual></toast>';
}
