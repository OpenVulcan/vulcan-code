//! Adapts Go resource-regex tokens while retaining the bounded UTF-8 Rust matcher.
//! 适配 Go 资源正则标记，同时保留有界 UTF-8 Rust 匹配器。

use regex::{Regex, RegexBuilder};
use regex_syntax::ast::{
    self, AssertionKind, Ast, ClassPerl, ClassPerlKind, ClassSetItem, Span, Visitor,
};
use std::fmt::Write;

/// Compiles source rules without silently changing Perl classes or Unicode word boundaries.
/// 编译来源规则，不静默改变 Perl 字符类或 Unicode 词边界。
///
/// # Errors
/// Returns a resource syntax or matcher compilation error before any rule can execute.
/// 在任何规则执行前返回资源语法或匹配器编译错误。
pub fn compile_source_regex(pattern: &str, case_insensitive: bool) -> Result<Regex, String> {
    let lowered = lower_source_tokens(pattern)?;
    let parsed = ast::parse::Parser::new()
        .parse(&lowered)
        .map_err(|error| error.to_string())?;
    let mut edits = ast::visit(&parsed, AsciiClasses::default())?;
    // AST spans distinguish real classes from escaped backslashes and quoted text.
    // AST 范围区分真实字符类与转义反斜杠、引用文本。
    edits.sort_by_key(|edit| edit.span.start.offset);
    // Append each source segment once; repeated in-place replacement is quadratic for large rule files.
    // 每个来源片段只追加一次；重复原位替换会使大规则文件产生平方级开销。
    let mut compiled_pattern = String::with_capacity(lowered.len());
    let mut cursor = 0;
    for edit in edits {
        compiled_pattern.push_str(&lowered[cursor..edit.span.start.offset]);
        compiled_pattern.push_str(&edit.replacement);
        cursor = edit.span.end.offset;
    }
    compiled_pattern.push_str(&lowered[cursor..]);
    RegexBuilder::new(&compiled_pattern)
        .case_insensitive(case_insensitive)
        .build()
        .map_err(|error| error.to_string())
}

/// Represents one non-overlapping AST token substitution in UTF-8 byte coordinates.
/// 表示 UTF-8 字节坐标中的一次非重叠 AST 标记替换。
struct TokenEdit {
    /// Selects the exact parsed token, not a substring guess.
    /// 选择精确解析标记，而非猜测子串。
    span: Span,
    /// Supplies an equivalent explicit ASCII class or boundary.
    /// 提供等价的显式 ASCII 字符类或边界。
    replacement: String,
}

/// Collects class and assertion edits with the dependency's constant-stack visitor.
/// 使用依赖库的常量栈访问器收集字符类与断言编辑。
#[derive(Default)]
struct AsciiClasses {
    /// Retains disjoint source token edits until parsing has completely succeeded.
    /// 在解析完全成功前保留互不重叠的来源标记编辑。
    edits: Vec<TokenEdit>,
}

impl AsciiClasses {
    /// Maps one Go Perl class to explicit ASCII ranges, including classes nested in brackets.
    /// 将一个 Go Perl 字符类映射为显式 ASCII 范围，包括括号内的字符类。
    fn push_class(&mut self, class: &ClassPerl) {
        let range = match class.kind {
            ClassPerlKind::Digit => "0-9",
            ClassPerlKind::Space => r"\t\n\f\r ",
            ClassPerlKind::Word => "0-9A-Za-z_",
        };
        self.edits.push(TokenEdit {
            span: class.span,
            replacement: format!("[{}{range}]", if class.negated { "^" } else { "" }),
        });
    }
}

impl Visitor for AsciiClasses {
    type Output = Vec<TokenEdit>;
    type Err = String;

    /// Returns substitutions only after the complete AST traversal succeeds.
    /// 仅在完整 AST 遍历成功后返回替换。
    fn finish(self) -> Result<Self::Output, Self::Err> {
        Ok(self.edits)
    }

    /// Changes Perl classes and word assertions while leaving literals and explicit Unicode intact.
    /// 更改 Perl 字符类与词断言，保持字面值和显式 Unicode 不变。
    fn visit_pre(&mut self, node: &Ast) -> Result<(), Self::Err> {
        match node {
            Ast::ClassPerl(class) => self.push_class(class),
            Ast::Assertion(assertion) => {
                let replacement = match assertion.kind {
                    AssertionKind::WordBoundary => Some(r"(?-u:\b)"),
                    AssertionKind::NotWordBoundary => Some(r"(?-u:\B)"),
                    _ => None,
                };
                if let Some(replacement) = replacement {
                    self.edits.push(TokenEdit {
                        span: assertion.span,
                        replacement: replacement.to_owned(),
                    });
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Visits bracket members separately because they are not top-level AST class nodes.
    /// 单独访问括号成员，因为它们不是顶层 AST 字符类节点。
    fn visit_class_set_item_pre(&mut self, item: &ClassSetItem) -> Result<(), Self::Err> {
        if let ClassSetItem::Perl(class) = item {
            self.push_class(class);
        }
        Ok(())
    }
}

/// Lowers Go literal, escape, bracket and repeat tokens before the Rust AST parser.
/// 在 Rust AST 解析前转换 Go 字面值、转义、括号与重复标记。
/// Go parse.go treats malformed repeats and nested opening brackets as literals, not extensions.
/// Go parse.go 将不成形的重复标记及括号内左括号视为字面值，而非扩展语法。
fn lower_source_tokens(pattern: &str) -> Result<String, String> {
    let mut output = String::with_capacity(pattern.len());
    let mut cursor = 0;
    let mut in_class = false;
    let mut first_in_class = false;
    while cursor < pattern.len() {
        let Some(character) = pattern[cursor..].chars().next() else {
            return Err("invalid resource regex cursor".to_owned());
        };
        cursor += character.len_utf8();
        if character == '\\' {
            lower_source_escape(pattern, &mut cursor, in_class, &mut output)?;
            first_in_class = false;
            continue;
        }
        if in_class {
            match character {
                '[' => {
                    if pattern.as_bytes().get(cursor) == Some(&b':')
                        && let Some(end) = pattern[cursor..].find(":]")
                    {
                        output.push('[');
                        output.push_str(&pattern[cursor..cursor + end + 2]);
                        cursor += end + 2;
                    } else {
                        output.push_str(r"\[");
                    }
                }
                ']' if first_in_class => output.push_str(r"\]"),
                ']' => {
                    output.push(']');
                    in_class = false;
                }
                '&' | '~' => {
                    output.push('\\');
                    output.push(character);
                }
                '-' if first_in_class => output.push_str(r"\-"),
                _ => output.push(character),
            }
            first_in_class = false;
        } else if character == '[' {
            output.push('[');
            in_class = true;
            first_in_class = true;
            if pattern.as_bytes().get(cursor) == Some(&b'^') {
                output.push('^');
                cursor += 1;
            }
        } else if character == '(' && pattern.as_bytes().get(cursor) == Some(&b'?') {
            lower_source_group(pattern, &mut cursor, &mut output)?;
        } else if character == '{' {
            if let Some(end) = source_repeat_end(pattern, cursor)? {
                output.push('{');
                output.push_str(&pattern[cursor..end]);
                cursor = end;
            } else {
                output.push_str(r"\{");
            }
        } else {
            output.push(character);
        }
    }
    Ok(output)
}

/// Preserves numbered captures and Go flags while removing unused capture names.
/// 保留编号捕获及 Go 标志，同时移除不参与资源执行的捕获名称。
/// PII templates and conditions only address numeric groups; Go permits duplicate and numeric names.
/// PII 模板和条件仅引用编号组；Go 允许重复名称和数字名称。
fn lower_source_group(
    pattern: &str,
    cursor: &mut usize,
    output: &mut String,
) -> Result<(), String> {
    let remainder = &pattern[*cursor..];
    let name_start = if remainder.starts_with("?P<") {
        Some(3)
    } else if remainder.starts_with("?<") {
        Some(2)
    } else {
        None
    };
    if let Some(start) = name_start {
        let end = remainder
            .find('>')
            .ok_or_else(|| "unclosed Go named capture".to_owned())?;
        let name = &remainder[start..end];
        if name.is_empty()
            || !name
                .bytes()
                .all(|value| value.is_ascii_alphanumeric() || value == b'_')
        {
            return Err("invalid Go resource capture name".to_owned());
        }
        output.push('(');
        *cursor += end + 1;
        return Ok(());
    }
    *cursor += 1;
    let mut flags = [None; 4];
    let mut negated = false;
    let mut saw_negated_flag = false;
    while let Some(value) = pattern.as_bytes().get(*cursor) {
        *cursor += 1;
        match value {
            b'i' | b'm' | b's' | b'U' => {
                let index = match value {
                    b'i' => 0,
                    b'm' => 1,
                    b's' => 2,
                    _ => 3,
                };
                flags[index] = Some(!negated);
                saw_negated_flag |= negated;
            }
            b'-' if !negated => negated = true,
            b':' | b')' if !negated || saw_negated_flag => {
                output.push_str("(?");
                for (flag, enabled) in ['i', 'm', 's', 'U'].into_iter().zip(flags) {
                    if enabled == Some(true) {
                        output.push(flag);
                    }
                }
                if flags.contains(&Some(false)) {
                    output.push('-');
                    for (flag, enabled) in ['i', 'm', 's', 'U'].into_iter().zip(flags) {
                        if enabled == Some(false) {
                            output.push(flag);
                        }
                    }
                }
                if *value == b')' && flags.iter().all(Option::is_none) {
                    output.push(':');
                }
                output.push(char::from(*value));
                return Ok(());
            }
            _ => return Err("invalid Go resource regex group or flags".to_owned()),
        }
    }
    Err("unclosed Go resource regex group or flags".to_owned())
}

/// Consumes one source escape, retaining quoted text and Unicode escapes as atomic tokens.
/// 消费一个来源转义，将引用文本及 Unicode 转义保留为原子标记。
fn lower_source_escape(
    pattern: &str,
    cursor: &mut usize,
    in_class: bool,
    output: &mut String,
) -> Result<(), String> {
    let escaped = pattern[*cursor..]
        .chars()
        .next()
        .ok_or_else(|| "trailing resource regex backslash".to_owned())?;
    *cursor += escaped.len_utf8();
    match escaped {
        'Q' if !in_class => {
            let remainder = &pattern[*cursor..];
            let end = remainder.find(r"\E").unwrap_or(remainder.len());
            output.push_str(&regex::escape(&remainder[..end]));
            *cursor += end;
            if *cursor < pattern.len() {
                *cursor += 2;
            }
        }
        '0'..='7' => {
            let mut codepoint = u32::from(escaped) - u32::from('0');
            let mut digits = 1;
            while digits < 3 {
                match pattern.as_bytes().get(*cursor) {
                    Some(value @ b'0'..=b'7') => {
                        codepoint = codepoint * 8 + u32::from(value - b'0');
                        *cursor += 1;
                        digits += 1;
                    }
                    _ => break,
                }
            }
            if escaped != '0' && digits == 1 {
                return Err("Go resource regex does not support backreferences".to_owned());
            }
            write!(output, r"\x{{{codepoint:x}}}").map_err(|error| error.to_string())?;
        }
        'p' | 'P' | 'x' => {
            output.push('\\');
            output.push(escaped);
            // Keep property/hex braces opaque; they are not repetition candidates.
            // 保持属性/十六进制花括号不透明；它们不是重复标记候选。
            if pattern.as_bytes().get(*cursor) == Some(&b'{') {
                let length = pattern[*cursor..]
                    .find('}')
                    .ok_or_else(|| "unclosed resource regex escape".to_owned())?
                    + 1;
                output.push_str(&pattern[*cursor..*cursor + length]);
                *cursor += length;
            }
        }
        value if value.is_ascii() && !value.is_ascii_alphanumeric() => {
            write!(output, r"\x{{{:x}}}", u32::from(value)).map_err(|error| error.to_string())?;
        }
        'a' | 'f' | 'n' | 'r' | 't' | 'v' | 'd' | 'D' | 's' | 'S' | 'w' | 'W' => {
            output.push('\\');
            output.push(escaped);
        }
        'A' | 'b' | 'B' | 'z' if !in_class => {
            output.push('\\');
            output.push(escaped);
        }
        _ => return Err(format!("unsupported Go resource regex escape: \\{escaped}")),
    }
    Ok(())
}

/// Recognizes Go's counted-repeat grammar and rejects its invalid numeric sizes.
/// 识别 Go 的计数重复语法并拒绝其非法数值范围。
/// None means a literal opening brace; numeric overflow remains an error, never a literal fallback.
/// None 表示字面左花括号；数值溢出始终是错误，绝不回退为字面值。
fn source_repeat_end(pattern: &str, mut cursor: usize) -> Result<Option<usize>, String> {
    let Some(minimum) = source_repeat_number(pattern, &mut cursor) else {
        return Ok(None);
    };
    let maximum = if pattern.as_bytes().get(cursor) == Some(&b',') {
        cursor += 1;
        if pattern.as_bytes().get(cursor) == Some(&b'}') {
            None
        } else if let Some(value) = source_repeat_number(pattern, &mut cursor) {
            Some(value)
        } else {
            return Ok(None);
        }
    } else {
        Some(minimum)
    };
    if pattern.as_bytes().get(cursor) != Some(&b'}') {
        return Ok(None);
    }
    if minimum > 1000 || maximum.is_some_and(|value| value > 1000 || value < minimum) {
        return Err("Go resource regex repetition count is invalid or exceeds 1000".to_owned());
    }
    Ok(Some(cursor + 1))
}

/// Reads a Go repetition integer without leading zeroes or unbounded integer arithmetic.
/// 读取不含前导零的 Go 重复整数，避免无界整数计算。
fn source_repeat_number(pattern: &str, cursor: &mut usize) -> Option<u32> {
    let bytes = pattern.as_bytes();
    let first = *bytes.get(*cursor)?;
    if !first.is_ascii_digit()
        || (first == b'0' && bytes.get(*cursor + 1).is_some_and(u8::is_ascii_digit))
    {
        return None;
    }
    let mut value: u32 = 0;
    while let Some(digit) = bytes.get(*cursor).filter(|value| value.is_ascii_digit()) {
        value = (value * 10 + u32::from(digit - b'0')).min(1001);
        *cursor += 1;
    }
    Some(value)
}
