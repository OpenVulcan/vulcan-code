//! Small Markdown primitives for explicit conversation-tool result projections.
//! 为显式会话工具结果投影提供小型 Markdown 基础函数。

/// Quotes one identifier or label without allowing its backticks to end the code span.
/// 引用一个标识或标签，防止其反引号提前结束行内代码。
/// Accepts arbitrary text and returns a single-line Markdown code span.
/// 接受任意文本并返回单行 Markdown 行内代码。
pub fn inline_code(text: &str) -> String {
    // Normalize line breaks because identifiers are displayed on a single metadata line.
    // 统一换行，因为标识展示在单行元信息中。
    let normalized = text.replace(['\r', '\n'], " ");
    // A longer delimiter also handles identifiers containing literal Markdown syntax.
    // 更长的分隔符也能处理包含 Markdown 字面语法的标识。
    let delimiter = "`".repeat(longest_backtick_run(&normalized) + 1);
    format!("{delimiter} {normalized} {delimiter}")
}

/// Wraps source content in a fence that cannot be terminated by the content itself.
/// 将源内容放入不会被内容自身提前终止的围栏。
/// Accepts unmodified content and returns a fenced block without adding line-number prefixes.
/// 接受未修改的正文并返回不添加行号前缀的围栏代码块。
pub fn fenced_code(content: &str) -> String {
    // Three is Markdown's minimum fence width, not a content or output budget.
    // 三是 Markdown 围栏的最小宽度，并非内容或输出预算。
    let delimiter = "`".repeat(longest_backtick_run(content).saturating_add(1).max(3));
    // A closing delimiter must start on its own line; existing final newlines remain unchanged.
    // 结束分隔符必须独占一行；保留正文已有的末尾换行。
    let separator = if content.ends_with('\n') { "" } else { "\n" };
    format!("{delimiter}\n{content}{separator}{delimiter}")
}

/// Formats a readable source link using a caller-validated URL.
/// 使用调用方已校验的地址生成可读来源链接。
/// Accepts the visible label and URL; returns escaped Markdown without changing source content.
/// 接受可见标签和地址；返回已转义 Markdown，不修改来源正文。
pub fn source_link(label: &str, url: &str) -> String {
    // Escape label syntax so untrusted source titles cannot create additional Markdown elements.
    // 转义标签语法，防止不可信来源标题产生额外 Markdown 元素。
    let mut escaped = String::new();
    for character in label.chars() {
        match character {
            '\r' | '\n' => escaped.push(' '),
            '\\' | '[' | ']' | '<' | '>' | '`' | '*' | '_' | '!' => {
                escaped.push('\\');
                escaped.push(character);
            }
            _ => escaped.push(character),
        }
    }
    // Angle-delimited destinations accept parentheses; encode delimiters and whitespace explicitly.
    // 尖括号包围的目标允许圆括号；显式编码边界字符和空白。
    let destination = url
        .replace('<', "%3C")
        .replace('>', "%3E")
        .replace(' ', "%20")
        .replace('\r', "%0D")
        .replace('\n', "%0A");
    format!("[{escaped}](<{destination}>)")
}

/// Measures literal backtick runs for the quoting helpers; returns zero when absent.
/// 为引用函数计算连续反引号长度；不存在时返回零。
fn longest_backtick_run(text: &str) -> usize {
    text.split(|character| character != '`')
        .map(str::len)
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies embedded fences and Unicode source text cannot escape a result code block.
    /// 验证内嵌围栏及 Unicode 源文本不能逃逸结果代码块。
    #[test]
    fn quotes_source_without_duplicating_or_rewriting_it() {
        // The unique marker must occur once even when the original source contains a fence.
        // 即使原文包含围栏，唯一标记也必须只出现一次。
        let source = "唯一正文\n```rust\nlet x = 1;\n```\n";
        let rendered = fenced_code(source);
        assert_eq!(rendered, format!("````\n{source}````"));
        assert_eq!(rendered.matches("唯一正文").count(), 1);
        assert_eq!(inline_code("a`b"), "`` a`b ``");
    }

    /// Verifies source labels cannot inject a second link and URL parentheses remain intact.
    /// 验证来源标签不能注入第二个链接，且地址中的圆括号保持完整。
    #[test]
    fn links_escape_labels_and_destinations() {
        assert_eq!(
            source_link("[title]\n*", "https://example.test/a(b)> c"),
            "[\\[title\\] \\*](<https://example.test/a(b)%3E%20c>)"
        );
    }
}
